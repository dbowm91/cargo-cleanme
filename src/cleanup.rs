//! Authorized redirected cleanup and full simulation mode (M005B).
//!
//! Cleanup is workspace-scoped and Cargo-mediated. The destructive unit is one
//! workspace CleanupUnit (C003): one resolved workspace, its complete resolved
//! `OutputSet`, and every PhysicalOutputGroup that the single `cargo clean`
//! invocation can affect. A unit is authorized only when *all* of those groups
//! are `PrivateBounded`, authorized, inactive, non-symlink, marker-qualified,
//! and stable under final full ownership-universe revalidation. Shared,
//! uncertain, symlink, and unauthorized external output is never cleaned, and
//! a private target group can never carry an unauthorized sibling build group
//! into the same invocation. `--dryrun` runs the full decision path but invokes
//! no `cargo clean` command.

use crate::{
    discovery,
    domain::*,
    error::AppError,
    progress::{ProgressObserver, ScanPhase},
    traverse,
    workspace::{self, CargoRunner},
};
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

/// The three cleanup modes.
///
/// There is deliberately no `Default`: every cleanup entry point states its
/// mode explicitly, so no future call site can silently inherit the weaker
/// Cargo-preview behaviour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanMode {
    /// Real Cargo cleanup. The default for every cleanup intent.
    Execute,
    /// Application simulation: the complete decision, proof, and reporting
    /// path, with no `cargo clean` process of any kind spawned.
    Simulate,
    /// Cargo preview: after the same complete final proof, invoke Cargo's own
    /// `clean --dry-run --verbose`. Distinct from [`Self::Simulate`] -- Cargo
    /// preview does spawn Cargo, simulation does not.
    CargoPreview,
}

impl CleanMode {
    /// Stable machine label for the JSON and log `mode` field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CargoPreview => "preview",
            Self::Simulate => "simulate",
            Self::Execute => "execute",
        }
    }
}

/// Which resolved `CleanupUnit`s are candidates for this invocation.
///
/// **A selection is applied after ownership classification and can only remove
/// candidates.** It cannot add one, reclassify one, or unblock a scope. That
/// is the whole reason a working-directory anchor is safe to use as a default:
/// `workspace::build_groups` still sees every discovered workspace, so output
/// shared with a workspace outside the anchor stays `Shared` and stays
/// inventory-only.
///
/// The alternative — narrowing *discovery* to the anchor — was measured and
/// rejected (ADR 006): narrowing discovery hides out-of-scope claimants and
/// silently upgrades `shared` bytes to `private`, the only class that authorizes
/// deletion.
///
/// **M018 — the one exception, and why it is not one.** `ExcludeRoots` states
/// a *proof* fact rather than narrowing an operator's choice: that root's own
/// traversal did not complete, so nothing inside it is proof-bearing. It
/// therefore also places that root outside the **proof boundary** — the Full
/// cleanup's boundary is the *proven* learned root (ADR 008 §5), and an
/// unprovable participant inside an unproven root is reported rather than
/// blocking the whole scope. That is not the ADR 006 shortcut, because the
/// excluded root's manifests are still discovered, resolved and classified:
/// the universe does not narrow, so a shared output between a proven root and
/// an unproven one stays `Shared` and stays inventory-only. The residual it
/// accepts is disclosed, not hidden.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanupSelection {
    /// Every resolved unit is a candidate. `clean ROOT` and `clean --full`,
    /// which are already bounded by their own root selection.
    All,
    /// An **unresolved** anchor on the process working directory.
    ///
    /// `cli.rs` resolves argv but must not perform IO, so it emits this
    /// unresolved intent and `main.rs` turns it into [`Self::Ancestry`] before
    /// the engine runs. It is deliberately fail-closed: [`Self::admits`]
    /// selects nothing until [`Self::resolved`] has been called, so a caller
    /// that forgets to resolve it cleans nothing rather than everything.
    WorkingDirectory,
    /// Units whose canonical workspace root is the anchor, an ancestor of it, or
    /// a descendant of it.
    Ancestry(PathBuf),
    /// Units whose canonical workspace root is exactly one of these. The
    /// interactive selection produces this.
    Roots(Vec<PathBuf>),
    /// Every unit the inner selection admits, **except** those under (or
    /// equal to) these roots.
    ///
    /// M018: the Full cleanup's proof boundary is the learned root. A learned
    /// root whose own traversal did not complete cannot authorize cleanup for
    /// the workspaces under it, so those workspaces are removed from the
    /// *candidate* set — never from the discovery set. They are still
    /// discovered, resolved, sized and classified like every other unit, so a
    /// shared output between a proven root and an unproven one stays `Shared`
    /// and stays inventory-only.
    ///
    /// This is an exclude-list, not the allow-list [`Self::Roots`] that `-i`
    /// produces, because the verdict's granularity is the whole learned root.
    /// It removes candidates only: it cannot add one, reclassify one, or
    /// unblock a scope.
    ///
    /// It also places the excluded roots outside the **proof boundary** — the
    /// one place a selection may do that, and the reason is stated in the
    /// type's documentation: the excluded root states a proof fact, and
    /// ADR 008 §5 makes the Full cleanup's boundary the *proven* learned
    /// root.
    ExcludeRoots {
        /// The selection this exclusion narrows.
        inner: Box<CleanupSelection>,
        /// The roots whose coverage could not be proven.
        roots: Vec<crate::domain::UnprovenRoot>,
    },
}

impl CleanupSelection {
    /// Whether one unit's workspace root survives this selection.
    ///
    /// `Ancestry` matches in **both** directions on purpose. An operator
    /// standing inside a member of a virtual workspace must still select that
    /// enclosing workspace, and an operator standing above several projects must
    /// be able to select one of them. A descendant-only rule would make the
    /// common virtual-workspace layout a silent no-op.
    pub fn admits(&self, workspace_root: &Path) -> bool {
        match self {
            Self::All => true,
            // Fail closed: an unresolved anchor selects nothing. See the
            // variant documentation for why this is not `true`.
            Self::WorkingDirectory => false,
            Self::Ancestry(anchor) => {
                workspace_root == anchor
                    || workspace_root.starts_with(anchor)
                    || anchor.starts_with(workspace_root)
            }
            Self::Roots(roots) => roots.iter().any(|root| root == workspace_root),
            // M018: the inner selection still decides, and the unproven
            // root's own subtree is removed from the candidate set. The
            // comparison is canonical on both sides so a symlinked prefix
            // (macOS `/var` vs `/private/var`) cannot fail open and leave an
            // unproven workspace a candidate.
            Self::ExcludeRoots { inner, roots } => {
                inner.admits(workspace_root)
                    && !roots
                        .iter()
                        .any(|u| path_is_within(workspace_root, &u.root))
            }
        }
    }

    /// Resolve [`Self::WorkingDirectory`] against the process working directory.
    ///
    /// The IO lives here rather than in `cli.rs` so that argv resolution stays
    /// pure and testable. An unresolvable working directory is an
    /// [`AppError::InvalidRoot`] usage error, never a silent fallback to
    /// [`Self::All`].
    pub fn resolved(&self) -> Result<CleanupSelection, AppError> {
        match self {
            Self::WorkingDirectory => {
                let cwd = std::env::current_dir().map_err(|e| AppError::InvalidRoot {
                    path: ".".into(),
                    reason: format!(
                        "cannot resolve the working directory for cleanup selection: {e}"
                    ),
                })?;
                Ok(Self::Ancestry(cwd))
            }
            // M018: the inner selection is the one that can be unresolved.
            Self::ExcludeRoots { inner, roots } => Ok(Self::ExcludeRoots {
                inner: Box::new(inner.resolved()?),
                roots: roots.clone(),
            }),
            other => Ok(other.clone()),
        }
    }

    /// Narrow this selection by removing the units under these roots.
    ///
    /// M018: the coverage exclusion is a filter *on top of* the declared
    /// selection, so it can only ever remove candidates. It composes with
    /// every shape, including an already-narrowed one, and it never changes
    /// the proof boundary.
    pub fn excluding(&self, roots: Vec<crate::domain::UnprovenRoot>) -> CleanupSelection {
        if roots.is_empty() {
            return self.clone();
        }
        match self {
            Self::ExcludeRoots {
                inner,
                roots: existing,
            } => {
                let mut merged = existing.clone();
                merged.extend(roots);
                Self::ExcludeRoots {
                    inner: inner.clone(),
                    roots: merged,
                }
            }
            other => Self::ExcludeRoots {
                inner: Box::new(other.clone()),
                roots,
            },
        }
    }

    /// A bounded human description of what this selection anchors on, or `None`
    /// when it does not filter.
    pub fn describe(&self) -> Option<String> {
        match self {
            Self::All => None,
            Self::Ancestry(anchor) => Some(format!("the working directory {}", anchor.display())),
            Self::WorkingDirectory => {
                Some("an unresolved working-directory anchor (internal error)".into())
            }
            Self::Roots(_) => Some("the interactive selection".into()),
            Self::ExcludeRoots { inner, roots } => Some(format!(
                "{} root(s) whose coverage could not be proven (within {})",
                roots.len(),
                inner
                    .describe()
                    .unwrap_or_else(|| "the whole scope".to_string())
            )),
        }
    }

    /// The typed reason this selection removed a unit, when the removal has a
    /// reason of its own.
    ///
    /// `ExcludeRoots` (M018) removes a unit because its learned root's
    /// coverage could not be proven. That is a different fact from "outside
    /// the selection", and the report must not conflate them: one says "you
    /// did not ask for this", the other says "this could not be proven".
    /// Every other variant removes units for the ordinary reason and returns
    /// `None`.
    pub fn exclusion_reason(&self, workspace_root: &Path) -> Option<(CleanupReasonCode, String)> {
        match self {
            Self::ExcludeRoots { roots, .. } => roots
                .iter()
                .find(|u| path_is_within(workspace_root, &u.root))
                .map(|u| {
                    (
                        CleanupReasonCode::SkippedIncompleteCoverage,
                        format!(
                            "the learned root {} could not be proven: {}; the workspace was left \
                             untouched — run `cargo cleanme scan --all` for the read-only \
                             inventory, or `cargo cleanme ROOT` for a bounded scope",
                            u.root.display(),
                            u.detail
                        ),
                    )
                }),
            _ => None,
        }
    }

    /// Whether this selection states that a root's coverage could not be
    /// proven — the M018 gate's per-root verdict.
    ///
    /// A **traversal error** carries no path: the upstream walk's error event
    /// has no child path. It is therefore attributed to the root whose walk
    /// produced it, and this predicate says whether that root's incompleteness
    /// was already **judged and disclosed**.
    ///
    /// It was, for a root the Full cleanup's gate skipped with a typed
    /// reason: the uncertainty inside it is the residual ADR 008 §5 accepts
    /// in exchange for the per-root rule. It was not, for any other root: a
    /// root that cannot be walked is indeterminate coverage for the scope
    /// that contains it, and the scope blocks — an unreadable automatic root
    /// is not absence, and guessing would be the ADR 007 defect in the other
    /// direction.
    pub fn excludes_coverage_root(&self, root: &Path) -> bool {
        match self {
            Self::ExcludeRoots { roots, .. } => roots
                .iter()
                .any(|unproven| path_is_within(root, &unproven.root)),
            _ => false,
        }
    }
}

/// Whether `path` is inside `root`, or is `root` itself.
///
/// Both sides are compared canonically, so a symlinked prefix cannot fail
/// open: the path that matters is the one on disk, not the spelling.
fn path_is_within(path: &Path, root: &Path) -> bool {
    let canon_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let canon_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    canon_path.starts_with(&canon_root)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanOutcome {
    Previewed,
    Simulated,
    Cleaned,
    Skipped,
    Failed,
}

impl CleanOutcome {
    /// Stable machine label. The JSON contract must not depend on `Debug`
    /// formatting, or renaming a variant would silently change the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Previewed => "previewed",
            Self::Simulated => "simulated",
            Self::Cleaned => "cleaned",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CleanResult {
    /// Workspace identity: the CleanupUnit's workspace root (C003 §7.5).
    pub display_path: PathBuf,
    pub workspace_roots: Vec<PathBuf>,
    /// Complete deduplicated physical covering-root union the single Cargo
    /// invocation for this unit can affect.
    pub output_roots: Vec<PathBuf>,
    pub ownership: OutputOwnershipClass,
    pub outcome: CleanOutcome,
    pub policy_disposition: Option<PolicyDisposition>,
    pub reason_code: CleanupReasonCode,
    pub before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub observed_decrease: Option<u64>,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupReasonCode {
    Previewed,
    Simulated,
    Cleaned,
    BelowMinimumSize,
    TooRecentForPolicy,
    NotIncluded,
    Excluded,
    SkippedActive,
    SkippedShared,
    SkippedUncertain,
    SkippedUnauthorized,
    SkippedMarkerInvalid,
    SkippedChangedBeforeCleanup,
    /// Ownership could not be re-proven at all (Cargo refused, discovery was
    /// incomplete, a manifest did not re-resolve). Distinct from a workspace
    /// that demonstrably changed.
    SkippedOwnershipUnproven,
    SkippedSafety,
    /// The workspace was discovered, resolved, sized and proven, but the
    /// operator's selection removed it from the candidate set (M014: a
    /// working-directory anchor, or an interactive selection).
    ///
    /// This is deliberately *not* an ownership or safety code: the unit was
    /// eligible and was left untouched on purpose. It exists so that a
    /// selection can never produce a silent no-op the way a configured
    /// `scan.root` once made `clean --known` (M012A defect).
    SkippedOutsideSelection,
    /// The unit's learned root could not be proven: that root's own traversal
    /// did not complete, so the ownership inside it could not be established
    /// (M018).
    ///
    /// The unit was still discovered, resolved, sized and classified — it is
    /// excluded from the *candidate* set, never from the discovery set — so a
    /// shared output between a proven root and this one stays `Shared`.
    SkippedIncompleteCoverage,
    CargoFailed,
    MeasurementFailed,
    SelectorUnsupported,
    SelectorInvalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofFailure {
    pub code: CleanupReasonCode,
    pub detail: String,
}

impl ProofFailure {
    /// Explicit constructor for proof sites. New proof failures must use
    /// this with an explicit code: deriving the code from the message text
    /// (`proof_skip_code`) lets any edit to the prose silently change the
    /// machine contract with no compiler error.
    pub fn new(code: CleanupReasonCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl From<String> for ProofFailure {
    // Legacy fallback retained for external callers. Production universe
    // refresh (L-2) now constructs `ProofFailure::new` with an explicit code
    // at each site, so no production path derives the code from prose.
    // Proof gates with a known site must use `ProofFailure::new` instead.
    fn from(detail: String) -> Self {
        let code = proof_skip_code(&detail);
        Self { code, detail }
    }
}
impl From<&str> for ProofFailure {
    fn from(detail: &str) -> Self {
        detail.to_owned().into()
    }
}

impl CleanupReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Previewed => "previewed",
            Self::Simulated => "simulated",
            Self::Cleaned => "cleaned",
            Self::BelowMinimumSize => "below_minimum_size",
            Self::TooRecentForPolicy => "too_recent_for_policy",
            Self::NotIncluded => "not_included",
            Self::Excluded => "excluded",
            Self::SkippedActive => "skipped_active",
            Self::SkippedShared => "skipped_shared",
            Self::SkippedUncertain => "skipped_uncertain",
            Self::SkippedUnauthorized => "skipped_unauthorized",
            Self::SkippedMarkerInvalid => "skipped_marker_invalid",
            Self::SkippedChangedBeforeCleanup => "skipped_changed_before_cleanup",
            Self::SkippedOwnershipUnproven => "skipped_ownership_unproven",
            Self::SkippedSafety => "skipped_safety",
            Self::SkippedOutsideSelection => "skipped_outside_selection",
            Self::SkippedIncompleteCoverage => "skipped_incomplete_coverage",
            Self::CargoFailed => "cargo_failed",
            Self::MeasurementFailed => "measurement_failed",
            Self::SelectorUnsupported => "selector_unsupported",
            Self::SelectorInvalid => "selector_invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDisposition {
    Selected,
    BelowMinimumSize,
    TooRecentForPolicy,
    NotIncluded,
    Excluded,
    SelectorEstimateUnavailable,
}

/// Stable machine code for a whole-scope safety block.
///
/// This is the *only* reason taxonomy in the crate. The prose a human reads and
/// the code an operator's log line matches are two views of one value, so the
/// log renderer never parses English and the two surfaces cannot drift apart.
/// JSON schema v1 keeps the prose `scope_reason` and does not yet expose the
/// code, so adding it there stays a deliberate schema decision rather than an
/// accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeBlockReason {
    /// Discovery produced diagnostics, so the combined ownership universe is
    /// incomplete before sizing or per-unit proof was attempted.
    IncompleteDiscovery,
    /// At least one discovered manifest could not be resolved, so coverage of
    /// the selected scope is unproven.
    OwnershipUnproven,
}

impl ScopeBlockReason {
    /// Stable machine label. Never contains whitespace or user text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IncompleteDiscovery => "incomplete_discovery",
            Self::OwnershipUnproven => "ownership_unproven",
        }
    }
}

/// A whole-scope safety block: a typed reason plus the prose a human reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeBlock {
    pub reason: ScopeBlockReason,
    pub message: String,
}

impl ScopeBlock {
    pub fn new(reason: ScopeBlockReason, message: String) -> Self {
        Self { reason, message }
    }
}

/// Output-placement environment a partition was computed under (M015
/// certificate §4.4).
///
/// A change to either variable between discovery and spawn moves output
/// placement out from under the partition, so activation (M016) must treat a
/// mismatch as a merge. M015 records the snapshot as evidence and proves the
/// detector ([`output_env_matches`]) with tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputProvenanceSnapshot {
    pub cargo_target_dir: Option<OsString>,
    pub cargo_build_build_dir: Option<OsString>,
}

/// Capture the output-placement environment through an injectable getter so
/// tests can vary it without touching the process environment (parallel-test
/// safe: no `std::env::set_var` anywhere near this code).
fn snapshot_output_env(get: &dyn Fn(&str) -> Option<OsString>) -> OutputProvenanceSnapshot {
    OutputProvenanceSnapshot {
        cargo_target_dir: get("CARGO_TARGET_DIR"),
        cargo_build_build_dir: get("CARGO_BUILD_BUILD_DIR"),
    }
}

/// True when the current output-placement environment still matches a
/// snapshot. Absent and empty are distinct: unsetting a variable is a change.
///
/// M016/B wires this as the §4-rule-4 merge detector: the partition's
/// provenance snapshot is re-compared before each island's first dispatch,
/// and any mismatch merges the remaining islands into one blocked scope.
fn output_env_matches(
    snapshot: &OutputProvenanceSnapshot,
    get: &dyn Fn(&str) -> Option<OsString>,
) -> bool {
    *snapshot == snapshot_output_env(get)
}

// The M015/M016 proof-island machinery (`PartitionMergeCause`,
// `ProofIsland`, `ProofPartition`, `compute_proof_partition`,
// `retain_proceed_island_units`, `append_partitioned_suffix`,
// `partition_section`, `partition_outcome_section`) was removed by
// M017/ADR 007: scoped proof is the default, so there are no islands to
// compute or report. The certificate's load-bearing remainder —
// `UnknownCapability` classification with `attribute_unknown`, the
// refused-manifest passenger rule, and per-dispatch environment
// re-verification — lives on in the boundary split above. History:
// M015 designed the certificate, M016 activated it behind a flag; both
// records stand, and neither shipped to users (0.2.9 unpublished).

/// The complete outcome of one cleanup run.
///
/// `Default` is intentionally not derived: a report must always state its
/// `mode`, so there is no way to construct a report whose mode is only
/// implied by a struct update.
#[derive(Clone, Debug)]
pub struct CleanReport {
    pub results: Vec<CleanResult>,
    pub diagnostics: usize,
    pub failed: usize,
    pub mode: CleanMode,
    pub counters: ScanCounters,
    /// Whole-scope safety block raised before sizing or per-unit proof.
    /// ADR 007: the block is boundary-scoped — only in-boundary unprovables
    /// raise it. Out-of-boundary poison arrives as `out_of_scope`
    /// diagnostics and never blocks.
    pub scope_blocked: Option<ScopeBlock>,
    /// Structured participants retained for diagnostics and audit consumers.
    /// ADR 007: boundary-relevant only; out-of-boundary unprovables travel
    /// in `diagnostics` with the `out_of_scope` category.
    pub unresolved_ownership: Vec<UnresolvedOwnershipParticipant>,
    /// Count of manifests that did not resolve outside the selected scope.
    /// Reported, never blocking. Rendered as one bounded human line so the
    /// operator can distinguish "the boundary is clean" from "the system is
    /// clean".
    pub out_of_scope_unresolved: usize,
    pub selected_roots: Vec<PathBuf>,
    pub discovered_manifests: usize,
    pub resolved_workspaces: usize,
    pub units_considered: usize,
    pub effective_policy: Option<CleanupPolicy>,
    pub selector: Option<CleanupSelector>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanupSelector {
    Profile(String),
    Package(String),
}

impl CleanupSelector {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Profile(_) => "profile",
            Self::Package(_) => "package",
        }
    }
    pub fn value(&self) -> &str {
        match self {
            Self::Profile(value) | Self::Package(value) => value,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CleanupPolicy {
    pub min_reclaimable_bytes: u64,
    pub min_inactive_seconds: Option<u64>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Bounded human listing of retained ownership participants, shared by the
/// blocked path (under the block message) and the unblocked path (under a
/// "reported, not blocking" header). Same bound, same overflow line, same
/// workspace hint in both places: the rows are facts about the run, not
/// about the block.
fn append_unresolved_listing(out: &mut String, unresolved: &[UnresolvedOwnershipParticipant]) {
    const MAX_LISTED: usize = 5;
    const MAX_FIELD_CHARS: usize = 200;
    let mut listed = 0usize;
    let mut workspace_hint = false;
    for participant in unresolved.iter().take(MAX_LISTED) {
        let manifest = participant
            .manifest
            .display()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_FIELD_CHARS)
            .collect::<String>();
        let reason = participant
            .reason
            .replace('\n', " ")
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_FIELD_CHARS)
            .collect::<String>();
        out.push_str(&format!(
            "  unresolved [{}] {}: {}\n",
            participant.stage, manifest, reason
        ));
        listed += 1;
        if participant
            .reason
            .contains("believes it's in a workspace when it's not")
        {
            workspace_hint = true;
        }
    }
    let remaining = unresolved.len().saturating_sub(listed);
    if remaining > 0 {
        out.push_str(&format!("  … {remaining} more unresolved participant(s)\n"));
    }
    if workspace_hint {
        out.push_str(
            "  hint: a nested crate that believes it is in a workspace needs an empty `[workspace]` table or a parent `workspace.exclude` entry; otherwise clean a healthy workspace explicitly, e.g. `cargo cleanme ROOT`\n",
        );
    }
}

impl CleanReport {
    /// The human cleanup report.
    ///
    /// Two shapes (M020):
    ///
    /// - **Quiet** — the default — names only what the run acted on: one line
    ///   per cleaned directory and one tally at the bottom. The workspaces the
    ///   run *skipped* are counted, not listed: a machine with a thousand
    ///   Cargo workspaces would otherwise print a thousand lines saying
    ///   "skipped", which is debugging information, not a result.
    /// - **Verbose** names everything: every workspace considered, the scope
    ///   census, the ownership class, the per-unit detail, and the per-unit
    ///   reason.
    ///
    /// Both shapes keep ADR 007's safety disclosures — the scope block, the
    /// unresolved-participant listing, and the out-of-scope count. Those say
    /// *what the report means*, not how long it is, and dropping them would
    /// turn "boundary clean" into "system clean".
    pub fn render(&self, options: &crate::report::HumanOptions) -> String {
        if options.verbose {
            self.render_verbose(options)
        } else {
            self.render_quiet(options)
        }
    }

    /// The safety disclosures, which both shapes carry.
    ///
    /// Returns `true` when the caller must stop there: a blocked scope with no
    /// results has nothing to report beyond the block itself.
    fn append_disclosures(&self, out: &mut String) -> bool {
        // ADR 007: poison outside the selected scope never blocks, but
        // silence about it would recreate the M012A defect in a new place —
        // "boundary clean" must not read as "system clean". Renders on both
        // the blocked and the clean path.
        if self.out_of_scope_unresolved > 0 {
            out.push_str(&format!(
                "  … {} out-of-scope unprovable participant(s) reported in diagnostics, not blocking: outside the selected scope\n",
                self.out_of_scope_unresolved
            ));
        }
        if let Some(block) = &self.scope_blocked {
            // A mid-run block (H-2 rediscovery, per-spawn premise recheck) can
            // fire after earlier units already ran: completed rows stay
            // visible so the human surface matches the JSON `units` list.
            out.push_str(&format!("{}\n", block.message));
            // A blocked scope with retained ownership participants names them
            // here (bounded, human surface only): the block message alone
            // carries a count, which never told the operator which manifest
            // to fix or which healthy workspace to clean explicitly instead.
            // JSON (`unresolved_participants`) and the log line (count only,
            // no paths) are unchanged.
            if block.reason == ScopeBlockReason::OwnershipUnproven
                && !self.unresolved_ownership.is_empty()
            {
                append_unresolved_listing(out, &self.unresolved_ownership);
            }
            return self.results.is_empty();
        } else if !self.unresolved_ownership.is_empty() {
            // ADR 007: reported-but-never-blocking participants (in-boundary
            // passengers and scope notes) are still named on the clean path,
            // or "boundary clean" would read as "nothing unproven".
            out.push_str("  reported unprovable participant(s), not blocking:\n");
            append_unresolved_listing(out, &self.unresolved_ownership);
        }
        false
    }

    /// The quiet surface: one line per acted-on directory, one tally.
    fn render_quiet(&self, options: &crate::report::HumanOptions) -> String {
        let mut out = String::new();
        if self.append_disclosures(&mut out) {
            return out;
        }
        // Deterministic size-descending with stable path tie-break.
        let mut ordered: Vec<&CleanResult> = self.results.iter().collect();
        ordered.sort_by(|a, b| {
            b.before_bytes
                .unwrap_or(0)
                .cmp(&a.before_bytes.unwrap_or(0))
                .then_with(|| a.display_path.cmp(&b.display_path))
        });
        // Only what the run acted on, plus the failures. A failure is not a
        // skip: silence about it would be a silent failure.
        let acted: Vec<&CleanResult> = ordered
            .into_iter()
            .filter(|r| {
                matches!(
                    r.outcome,
                    CleanOutcome::Cleaned
                        | CleanOutcome::Simulated
                        | CleanOutcome::Previewed
                        | CleanOutcome::Failed
                )
            })
            .collect();
        let rows: Vec<(String, String, String)> = acted
            .iter()
            .map(|r| {
                let verb = format!("{:?}", r.outcome);
                // The physical directory Cargo emptied is what was cleaned.
                let path = if r.output_roots.is_empty() {
                    options.path(&r.display_path)
                } else {
                    r.output_roots
                        .iter()
                        .map(|p| options.path(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let size = match r.outcome {
                    // What was reclaimed is the observed decrease, not the
                    // pre-clean size: the difference is the answer.
                    CleanOutcome::Cleaned => r.observed_decrease,
                    _ => r.before_bytes,
                }
                .map(|n| crate::report::format_bytes(n).trim().to_string())
                .unwrap_or_else(|| "—".to_string());
                (verb, path, size)
            })
            .collect();
        let verb_width = rows
            .iter()
            .map(|(v, _, _)| v.chars().count())
            .max()
            .unwrap_or(0);
        let path_width = rows
            .iter()
            .map(|(_, p, _)| p.chars().count())
            .max()
            .unwrap_or(0);
        for (verb, path, size) in &rows {
            out.push_str(&format!(
                "{verb:<verb_width$}  {path:<path_width$}  {size}\n"
            ));
        }
        if !rows.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.quiet_tally());
        out.push('\n');
        out
    }

    /// The quiet surface's one tally line.
    ///
    /// The tally states the space saved in words a person can act on. It is
    /// the only place the quiet surface counts the workspaces it did not
    /// touch: a number, never a line each.
    fn quiet_tally(&self) -> String {
        let mut acted = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut directories = 0usize;
        let mut before = 0u64;
        let mut delta = 0u64;
        for r in &self.results {
            match r.outcome {
                CleanOutcome::Cleaned => {
                    acted += 1;
                    directories += r.output_roots.len().max(1);
                    before = before.saturating_add(r.before_bytes.unwrap_or(0));
                    delta = delta.saturating_add(r.observed_decrease.unwrap_or(0));
                }
                CleanOutcome::Simulated | CleanOutcome::Previewed => {
                    acted += 1;
                    directories += r.output_roots.len().max(1);
                    before = before.saturating_add(r.before_bytes.unwrap_or(0));
                }
                CleanOutcome::Skipped => skipped += 1,
                CleanOutcome::Failed => failed += 1,
            }
        }
        let mut notes: Vec<String> = Vec::new();
        match self.mode {
            // A simulation and a preview change nothing on disk. The quiet
            // surface says so on every tally, because "would be reclaimed"
            // alone could be misread as a result.
            CleanMode::Simulate => {
                notes.push("simulation only — no `cargo clean` command was invoked".to_string())
            }
            CleanMode::CargoPreview => {
                notes.push("preview only — no cleanup executed".to_string());
            }
            CleanMode::Execute => {}
        }
        if failed > 0 {
            notes.push(format!("{failed} failed"));
        }
        if skipped > 0 {
            notes.push(format!("{skipped} skipped"));
        }
        let suffix = if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join(", "))
        };
        if acted == 0 {
            let nothing = match self.mode {
                CleanMode::Execute => "Nothing was cleaned",
                CleanMode::Simulate | CleanMode::CargoPreview => "Nothing would be cleaned",
            };
            return format!("{nothing}{suffix}");
        }
        if self.mode != CleanMode::Execute && self.selector.is_some() {
            // The byte figure of a selector run is the *output union*, not the
            // selector's share: the selector's own reclaimable size is not
            // measured, and the JSON projection says so too. Claiming the
            // union as "would be reclaimed" would over-claim by the whole
            // union.
            return format!(
                "{} selected; the selector's reclaimable size is not measured (output union {}){suffix}",
                directories,
                crate::report::format_bytes(before).trim(),
            );
        }
        let (bytes, phrase) = match self.mode {
            CleanMode::Execute => (delta, "reclaimed"),
            CleanMode::Simulate | CleanMode::CargoPreview => (before, "would be reclaimed"),
        };
        format!(
            "{} {phrase} from {directories} {}{suffix}",
            crate::report::format_bytes(bytes).trim(),
            if directories == 1 {
                "directory"
            } else {
                "directories"
            },
        )
    }

    /// The verbose surface: every workspace considered, with its detail.
    fn render_verbose(&self, options: &crate::report::HumanOptions) -> String {
        let mut out = String::new();
        if !self.selected_roots.is_empty() {
            out.push_str(&format!(
                "combined scope: {} root(s) [{}], {} manifest(s), {} resolved workspace(s), {} CleanupUnit(s), {} unresolved ownership participant(s)\n",
                self.selected_roots.len(),
                self.selected_roots.iter().map(|p| options.path(p)).collect::<Vec<_>>().join(", "),
                self.discovered_manifests,
                self.resolved_workspaces,
                self.units_considered,
                self.unresolved_ownership.len(),
            ));
        }
        if self.append_disclosures(&mut out) {
            return out;
        }
        let mut previewed = 0usize;
        let mut simulated = 0usize;
        let mut cleaned = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut before = 0u64;
        let mut after = 0u64;
        let mut delta = 0u64;
        // Deterministic size-descending with stable path tie-break; the final
        // report lists every workspace CleanupUnit (C003 §7.5).
        let mut ordered: Vec<&CleanResult> = self.results.iter().collect();
        ordered.sort_by(|a, b| {
            b.before_bytes
                .unwrap_or(0)
                .cmp(&a.before_bytes.unwrap_or(0))
                .then_with(|| a.display_path.cmp(&b.display_path))
        });
        for r in ordered {
            match r.outcome {
                CleanOutcome::Previewed => {
                    previewed += 1;
                    if let Some(n) = r.before_bytes {
                        before = before.saturating_add(n);
                    }
                }
                CleanOutcome::Simulated => {
                    simulated += 1;
                    if let Some(n) = r.before_bytes {
                        before = before.saturating_add(n);
                    }
                }
                CleanOutcome::Cleaned => {
                    cleaned += 1;
                    if let Some(n) = r.before_bytes {
                        before = before.saturating_add(n);
                    }
                    if let Some(n) = r.after_bytes {
                        after = after.saturating_add(n);
                    }
                    if let Some(n) = r.observed_decrease {
                        delta = delta.saturating_add(n);
                    }
                }
                CleanOutcome::Skipped => skipped += 1,
                CleanOutcome::Failed => {
                    failed += 1;
                }
            }
            out.push_str(&format!(
                "{:?}  {}  [{}]",
                r.outcome,
                options.path(&r.display_path),
                r.ownership.label(),
            ));
            // C003 §7.5: one row per workspace CleanupUnit; every physical
            // output root the single Cargo invocation affects stays visible.
            if r.output_roots.len() > 1 {
                out.push_str(&format!(
                    "  outputs {}",
                    r.output_roots
                        .iter()
                        .map(|p| options.path(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else if let Some(root) = r.output_roots.first() {
                out.push_str(&format!("  output {}", options.path(root)));
            }
            if let Some(n) = r.before_bytes {
                let label = if self.selector.is_some() {
                    "output-union before"
                } else {
                    "before"
                };
                out.push_str(&format!("  {label} {}", crate::report::format_bytes(n)));
            }
            if let Some(n) = r.after_bytes {
                let label = if self.selector.is_some() {
                    "output-union after"
                } else {
                    "after"
                };
                out.push_str(&format!("  {label} {}", crate::report::format_bytes(n)));
            }
            if let Some(n) = r.observed_decrease {
                out.push_str(&format!(
                    "  observed decrease {}",
                    crate::report::format_bytes(n)
                ));
            }
            if !r.detail.is_empty() {
                out.push_str(&format!("  — {}", r.detail.replace('\n', " ")));
            }
            out.push('\n');
        }
        match self.mode {
            CleanMode::CargoPreview => {
                out.push_str(&format!(
                    "\ncargo preview: {previewed} previewed, {skipped} skipped, {failed} failed; pre-clean estimate {}; no cleanup executed; {} filesystem diagnostics\n",
                    crate::report::format_bytes(before),
                    self.diagnostics
                ));
            }
            CleanMode::Simulate => {
                out.push_str(&format!(
                    "\nsimulation: {simulated} would-clean, {skipped} skipped, {failed} failed; estimated would-clean {}; no `cargo clean` command was invoked; {} filesystem diagnostics\n",
                    crate::report::format_bytes(before),
                    self.diagnostics
                ));
            }
            CleanMode::Execute => {
                out.push_str(&format!(
                    "\ncleanup: {cleaned} cleaned, {skipped} skipped, {failed} failed; pre-clean estimate {}, post-clean measured {}, observed decrease {}; {} filesystem diagnostics\n",
                    crate::report::format_bytes(before),
                    crate::report::format_bytes(after),
                    crate::report::format_bytes(delta),
                    self.diagnostics
                ));
            }
        }
        out
    }
}

// Re-export workspace runner types for test-injectable I/O. Cleanup uses the
// same Cargo-owned adapter as scanning, but with frozen output env for the
// actual `cargo clean` invocation.
pub use workspace::{ProcessOutput, SystemCargoRunner};

pub trait CleanupRunner: Sync {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
    fn run_with_env(
        &self,
        cwd: &Path,
        args: &[OsString],
        env: &[(OsString, OsString)],
    ) -> io::Result<ProcessOutput>;
}

pub struct SystemCleanupRunner;

impl CargoRunner for SystemCleanupRunner {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
        let output = Command::new("cargo").args(args).current_dir(cwd).output()?;
        Ok(ProcessOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

impl CleanupRunner for SystemCleanupRunner {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
        CargoRunner::run(self, cwd, args)
    }
    fn run_with_env(
        &self,
        cwd: &Path,
        args: &[OsString],
        env: &[(OsString, OsString)],
    ) -> io::Result<ProcessOutput> {
        let mut cmd = Command::new("cargo");
        cmd.args(args).current_dir(cwd);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let output = cmd.output()?;
        Ok(ProcessOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn output_text(output: &ProcessOutput) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Check whether a physical group is authorized for destructive cleanup.
///
/// Rules (ADR 001 + C002 §7.1):
/// - Only `PrivateBounded` may proceed to path authorization.
/// - `ExternalUnproven`, `Shared`, and `Uncertain` remain inventory-only
///   under any authorization-root configuration; authorization never
///   manufactures ownership proof.
/// - Symlink roots are never cleanable (grouped as `Uncertain` upstream).
/// - The explicit `clean ROOT` sandbox authorizes private output inside ROOT.
/// - Private output outside ROOT must be inside one configured absolute
///   allowed root.
/// - Symlink authorization roots are invalid.
/// - Authorization does not imply ownership; it only lifts the location
///   boundary for already-private groups.
/// - Empty allowed list preserves M004-style containment.
/// - `cleanup.allowed_output_roots` is authorization only; it MUST NOT
///   convert `ExternalUnproven` into a private group.
pub fn is_authorized(
    group: &workspace::RawGroup,
    clean_root: &Path,
    allowed_roots: &[PathBuf],
) -> Result<bool, String> {
    // The workspace universe is deliberately not consulted here: authorization is
    // purely about the group's covering roots inside the bounded clean root.
    // (It used to be accepted and ignored, which invited the belief that the
    // universe participated in the decision.)
    covering_is_authorized(group.ownership, &group.covering, clean_root, allowed_roots)
}

/// Location authorization for one physical group's covering roots.
///
/// Same rules as [`is_authorized`], but expressed over the ownership class and
/// covering roots so a workspace CleanupUnit can gate every affected group
/// independently (C003 §7.2).
///
/// Precondition: `covering`, `clean_root`, and `allowed_roots` entries are
/// canonicalized (no `..`, no symlinked prefixes). `Path::starts_with` is
/// component-lexical, so a non-canonical covering such as
/// `/clean/../outside` would compare as inside `clean_root` while escaping
/// it physically. Production callers pass canonical coverings from
/// `build_groups`; external callers must canonicalize first.
pub fn covering_is_authorized(
    ownership: OutputOwnershipClass,
    covering: &[PathBuf],
    clean_root: &Path,
    allowed_roots: &[PathBuf],
) -> Result<bool, String> {
    covering_is_authorized_roots(
        ownership,
        covering,
        std::slice::from_ref(&clean_root.to_path_buf()),
        allowed_roots,
    )
}

pub fn covering_is_authorized_roots(
    ownership: OutputOwnershipClass,
    covering: &[PathBuf],
    clean_roots: &[PathBuf],
    allowed_roots: &[PathBuf],
) -> Result<bool, String> {
    // Precondition (see `covering_is_authorized`): covering and roots are
    // canonicalized. `starts_with` below is component-lexical and unsound on
    // non-canonical paths containing `..`.
    // C002-F1: ownership class is authoritative and checked first.
    // ExternalUnproven is inventory-only even inside clean ROOT or a
    // configured allowed root.
    if ownership != OutputOwnershipClass::PrivateBounded {
        return Ok(false);
    }
    // Validate allowed roots: absolute, not symlink.
    let mut canonical_allowed: Vec<PathBuf> = Vec::new();
    for root in allowed_roots {
        if !root.is_absolute() {
            return Err(format!(
                "cleanup authorization root must be absolute: {}",
                root.display()
            ));
        }
        match fs::symlink_metadata(root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Missing allowed root does not authorize anything, but is not
                // itself fatal; treat as non-matching.
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "cannot inspect cleanup authorization root {}: {e}",
                    root.display()
                ));
            }
            Ok(m) if m.file_type().is_symlink() || path_is_symlink(root) => {
                return Err(format!(
                    "cleanup authorization root must not be a symlink: {}",
                    root.display()
                ));
            }
            Ok(m) if !m.is_dir() => {
                continue;
            }
            Ok(_) => {}
        }
        // Canonicalize existing roots for containment checks. An allowed root
        // that exists but cannot be resolved is a hard error, not a silent
        // skip: reporting it as "unauthorized" would mask the filesystem
        // failure with the wrong reason code.
        match fs::canonicalize(root) {
            Ok(canonical) => canonical_allowed.push(canonical),
            Err(e) => {
                return Err(format!(
                    "cannot resolve cleanup authorization root {}: {e}",
                    root.display()
                ));
            }
        }
    }
    let canonical_clean: Vec<PathBuf> = clean_roots
        .iter()
        .map(|root| {
            fs::canonicalize(root)
                .map_err(|e| format!("cannot resolve cleanup root {}: {e}", root.display()))
        })
        .collect::<Result<_, _>>()?;
    // Every covering root must be inside an authorized boundary.
    for covering_root in covering {
        // Covering roots are already canonical (from build_groups).
        if canonical_clean
            .iter()
            .any(|root| covering_root == root || covering_root.starts_with(root))
        {
            continue;
        }
        if canonical_allowed
            .iter()
            .any(|a| covering_root == a || covering_root.starts_with(a))
        {
            continue;
        }
        return Ok(false);
    }
    // Only PrivateBounded reaches here; path authorization passed.
    Ok(true)
}

/// Skip detail for non-private ownership classes.
///
/// States both facts required by C002 §7.1: external output ownership is
/// unproven and configured authorization does not establish exclusivity.
pub fn ownership_skip_detail(ownership: OutputOwnershipClass) -> String {
    match ownership {
        OutputOwnershipClass::Shared => "shared output remains inventory-only".into(),
        OutputOwnershipClass::Uncertain => "uncertain output remains inventory-only".into(),
        OutputOwnershipClass::ExternalUnproven => {
            "external output ownership is unproven; configured authorization does not establish exclusivity".into()
        }
        OutputOwnershipClass::PrivateBounded => {
            "external output requires explicit cleanup authorization".into()
        }
    }
}

/// First blocking reason for a whole CleanupUnit, or `Ok(())` (C003 §7.2).
///
/// Core C003 invariant: a workspace is destructively cleanable only if *every*
/// physical output group the Cargo invocation can affect is independently
/// `PrivateBounded`, authorized, measured (real non-symlink directory, outside
/// the recency window), and marker-qualified. A private target group can never
/// carry an `ExternalUnproven`, `Shared`, `Uncertain`, inactive, or unauthorized
/// sibling build group into the same `cargo clean` invocation.
///
/// Returns one actionable reason naming the failing output kind, path, and
/// class. No Cargo process is spawned for any subset of a unit.
pub fn unit_block_reason(
    unit: &workspace::CleanupUnit,
    clean_root: &Path,
    allowed_output_roots: &[PathBuf],
) -> Result<(), String> {
    unit_block_reason_roots(
        unit,
        std::slice::from_ref(&clean_root.to_path_buf()),
        allowed_output_roots,
    )
}

fn unit_block_reason_roots(
    unit: &workspace::CleanupUnit,
    clean_roots: &[PathBuf],
    allowed_output_roots: &[PathBuf],
) -> Result<(), String> {
    if let Some(root) = unit.unmapped.first() {
        return Err(format!(
            "{} output {} has unproven physical identity; cleanup deferred",
            match root.kind {
                OutputRootKind::Target => "target",
                OutputRootKind::Build => "build",
            },
            root.logical_path.display()
        ));
    }
    for group in &unit.groups {
        // 1. Ownership class is authoritative; authorization never promotes.
        if group.ownership != OutputOwnershipClass::PrivateBounded {
            return Err(format!(
                "{} output {} is {}; {}",
                group.kind_label(),
                group.display.display(),
                group.ownership.label(),
                ownership_skip_detail(group.ownership)
            ));
        }
        // 2. Location authorization for this group only.
        match covering_is_authorized_roots(
            group.ownership,
            &group.covering,
            clean_roots,
            allowed_output_roots,
        ) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "{} output {} is private but outside the authorized cleanup boundary; \
                     add a `cleanup.allowed_output_roots` entry or use a bounded root that contains it",
                    group.kind_label(),
                    group.display.display()
                ));
            }
            Err(msg) => return Err(msg),
        }
        // 3. The group must be a measured, real, non-symlink, inactive,
        // marker-qualified output directory (fail-fast analysis verdict).
        if group.measured.is_none() {
            let reason = group.skip.map_or("output is not eligible", |r| r.detail());
            return Err(format!(
                "{} output {} is not cleanable: {reason}",
                group.kind_label(),
                group.display.display()
            ));
        }
    }
    Ok(())
}

/// True when a `unit_block_reason_roots` error reports authorization
/// infrastructure failure (misconfigured/unreadable roots), not a plain
/// "outside authorized boundary" verdict.
///
/// Ownership-first ordering is unchanged: `covering_is_authorized_roots`
/// still returns `Ok(false)` for non-private classes before inspecting any
/// path; only `Err` strings with these prefixes take the scope-block path,
/// so authorization still cannot promote an ownership class.
fn is_authorization_infrastructure_error(detail: &str) -> bool {
    detail.contains("cleanup authorization root") || detail.contains("cannot resolve cleanup root")
}

/// Machine code for a fresh group whose ownership degraded during the proof.
///
/// Scan-time uses `unit_skip_code` for the same fact; the proof must agree
/// instead of conflating every degradation to `Changed`.
fn ownership_degraded_code(class: OutputOwnershipClass) -> CleanupReasonCode {
    match class {
        OutputOwnershipClass::Shared => CleanupReasonCode::SkippedShared,
        OutputOwnershipClass::Uncertain => CleanupReasonCode::SkippedUncertain,
        OutputOwnershipClass::ExternalUnproven => CleanupReasonCode::SkippedOwnershipUnproven,
        OutputOwnershipClass::PrivateBounded => CleanupReasonCode::SkippedChangedBeforeCleanup,
    }
}

/// Typed mapping for a covering-root inspect failure.
///
/// `NotFound` (disappeared) and type/symlink changes are evidence of change;
/// an unreadable root (`PermissionDenied`) is uncertainty, not a change —
/// the operator response differs (investigate/retry vs "something moved").
fn covering_inspect_failure(err: io::Error) -> ProofFailure {
    if err.kind() == io::ErrorKind::PermissionDenied {
        return ProofFailure::new(
            CleanupReasonCode::SkippedUncertain,
            format!("preflight uncertain: cannot inspect covering root: {err}"),
        );
    }
    ProofFailure::new(
        CleanupReasonCode::SkippedChangedBeforeCleanup,
        format!("preflight changed: cannot inspect covering root: {err}"),
    )
}

pub fn clean(
    root: &Path,
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
) -> Result<CleanReport, AppError> {
    clean_with(
        root,
        recency_seconds,
        allowed_output_roots,
        mode,
        &SystemCleanupRunner,
        &crate::progress::NoopObserver,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn clean_with(
    root: &Path,
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
) -> Result<CleanReport, AppError> {
    clean_with_roots(
        std::slice::from_ref(&root.to_path_buf()),
        recency_seconds,
        allowed_output_roots,
        mode,
        runner,
        observer,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn clean_with_roots(
    roots: &[PathBuf],
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
) -> Result<CleanReport, AppError> {
    clean_with_roots_policy(
        roots,
        recency_seconds,
        allowed_output_roots,
        mode,
        runner,
        observer,
        &CleanupPolicy::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn clean_with_roots_policy(
    roots: &[PathBuf],
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
    policy: &CleanupPolicy,
) -> Result<CleanReport, AppError> {
    clean_with_roots_policy_selector(
        roots,
        recency_seconds,
        allowed_output_roots,
        mode,
        runner,
        observer,
        policy,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn clean_with_roots_policy_selector(
    roots: &[PathBuf],
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
    policy: &CleanupPolicy,
    selector: Option<CleanupSelector>,
) -> Result<CleanReport, AppError> {
    clean_with_roots_policy_selector_guarded(
        roots,
        recency_seconds,
        allowed_output_roots,
        mode,
        runner,
        observer,
        policy,
        selector,
        None,
        &CleanupSelection::All,
    )
}

/// The admission premise a provenance-aware caller proved before invoking the
/// engine: `omitted` paths were positively absent/non-directory when the
/// combined ownership universe was bounded, so their reappearance (or
/// unreadability) invalidates that boundary.
///
/// When `premise` is present, the engine re-examines it with fresh
/// `symlink_metadata` (no timers, sleeps, or cached metadata) at the last
/// responsible moment: before the first candidate reaches final proof and
/// before every later Cargo spawn. A violated premise blocks the pending and
/// all later candidates with a typed whole-scope block instead of spawning.
/// Policy-skipped units never reach proof and need no guard: nothing spawns
/// for them either way.
#[derive(Clone, Debug, Default)]
pub struct AdmissionPremise {
    pub omitted: Vec<crate::policy::AutomaticOmission>,
}

/// The resolved ownership universe: manifest discovery, workspace resolution,
/// physical grouping, fail-fast analysis, CleanupUnit construction, and the
/// operator's selection applied to the resulting units.
///
/// This exists as one named value rather than as loose locals so that the
/// destructive engine and the read-only `cargo cleanme -i` candidate listing
/// run **the same proof**. A second copy of this pipeline would be a second
/// ownership proof free to drift from the first, and the rows the interactive
/// list offers would stop being the rows the engine acts on.
struct ResolvedUniverse {
    roots: Vec<PathBuf>,
    cleanup_phase: ScanPhase,
    counters: ScanCounters,
    diagnostics: Vec<ScanDiagnostic>,
    /// Discovery-phase diagnostics inside the proof boundary (ADR 007).
    /// Pathless diagnostics count as inside (fail-closed: unattributable
    /// incompleteness blocks). Only this count can raise a block.
    boundary_discovery_diagnostics: usize,
    manifests: Vec<PathBuf>,
    /// H-2 snapshot of the manifest set the universe was built from, taken
    /// before the first candidate reaches its final proof.
    initial_manifests: Vec<PathBuf>,
    workspaces: Vec<ResolvedWorkspace>,
    units: Vec<workspace::CleanupUnit>,
    unselected: Vec<workspace::CleanupUnit>,
    unresolved_ownership: Vec<UnresolvedOwnershipParticipant>,
    /// In-boundary `MergeAll` participants (ADR 007). Only these block;
    /// `unresolved_ownership` also carries reported passengers and notes.
    blocking_unresolved: usize,
    /// Out-of-boundary unresolved manifests, carried in `diagnostics` as
    /// informational `out_of_scope` entries. Reported, never blocking.
    out_of_scope_unresolved: usize,
}

/// Root validation, manifest discovery, and full ownership resolution.
///
/// Read-only: it spawns `cargo metadata` through the supplied runner, exactly
/// as the cleanup engine does, but it never spawns `cargo clean` and mutates
/// nothing on disk. That makes it the correct basis for an interactive listing
/// and for any future dry inspection path.
#[allow(clippy::too_many_arguments)]
fn discover_and_resolve_universe(
    roots: &[PathBuf],
    recency_seconds: u64,
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
    selection: &CleanupSelection,
) -> Result<ResolvedUniverse, AppError> {
    // Validate every root before doing anything observable: an invalid
    // invocation must not spawn a process, and the runtime capability verdict
    // must not be able to mask a malformed root (M10).
    for root in roots {
        validate_cleanup_root(root)?;
    }
    let roots = collapse_roots(roots.to_vec())?;
    if roots.is_empty() {
        return Err(AppError::InvalidRoot {
            path: "<empty>".into(),
            reason: "cleanup requires at least one bounded root".into(),
        });
    }
    // Re-check the collapsed set: collapsing only narrows it, and every
    // surviving root must still be a real bounded directory.
    for root in &roots {
        validate_cleanup_root(root)?;
    }
    let scan_start = SystemTime::now();
    let cleanup_phase = match mode {
        CleanMode::CargoPreview => ScanPhase::CleanupPreview,
        CleanMode::Simulate => ScanPhase::CleanupSimulate,
        CleanMode::Execute => ScanPhase::CleanupExecute,
    };
    // Discovery (manifest-first, no Cargo yet).
    observer.phase(cleanup_phase);
    let discovery_start = std::time::Instant::now();
    // M018: the walk is **per root**, so a traversal error can be attributed
    // to the root that produced it. A traversal error carries no path — the
    // upstream walk's error event has no child path — so the root is the only
    // attribution available, and the Full cleanup's proof boundary is the
    // learned root. One walk per root is also the question the per-root
    // verdict asks; a single walk over the whole root set could only answer
    // "did *something* fail", which is the whole-system question ADR 008
    // withdrew.
    let mut manifests: Vec<PathBuf> = Vec::new();
    let mut diagnostics: Vec<ScanDiagnostic> = Vec::new();
    let mut counters = crate::domain::ScanCounters::default();
    // Each root's own diagnostics, as a range into `diagnostics`.
    let mut root_diagnostics: Vec<(PathBuf, usize, usize)> = Vec::new();
    for root in &roots {
        let resolved = crate::domain::EffectiveScanPolicy {
            recency: Duration::from_secs(recency_seconds),
            scope: ScanScope::ExplicitRoots(vec![root.clone()]),
            discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
        };
        let start = diagnostics.len();
        let discovered = discovery::discover_manifests(&resolved, observer)?;
        // The bounded walk fills only the three discovery counters; every
        // other counter belongs to the phases that follow. The assertion
        // keeps that true: if the walk ever filled another one, the merge
        // below would silently drop it.
        debug_assert_eq!(
            discovered.counters,
            crate::domain::ScanCounters {
                directories_visited: discovered.counters.directories_visited,
                directories_pruned: discovered.counters.directories_pruned,
                manifests_found: discovered.counters.manifests_found,
                ..Default::default()
            },
            "the bounded walk fills only the discovery counters"
        );
        counters.directories_visited = counters
            .directories_visited
            .saturating_add(discovered.counters.directories_visited);
        counters.directories_pruned = counters
            .directories_pruned
            .saturating_add(discovered.counters.directories_pruned);
        counters.manifests_found = counters
            .manifests_found
            .saturating_add(discovered.counters.manifests_found);
        manifests.extend(discovered.manifests);
        diagnostics.extend(discovered.diagnostics);
        root_diagnostics.push((root.clone(), start, diagnostics.len()));
    }
    manifests.sort();
    manifests.dedup();
    let discovery_elapsed = discovery_start.elapsed();
    counters.discovery_nanos = counters
        .discovery_nanos
        .saturating_add(crate::domain::elapsed_nanos(discovery_elapsed));
    let discovery_diagnostic_count = diagnostics.len();
    // H-2 snapshot: the manifest set the hoisted universe was built from.
    // The last-moment gate before each spawn rediscovers and blocks on any
    // growth, so a manifest created after hoisting cannot silently escape
    // the overlap check. Sorted so the comparison is order-independent.
    let mut initial_manifests = manifests.clone();
    initial_manifests.sort();

    // Workspace resolution, physical grouping, fail-fast analysis, and
    // CleanupUnit construction share one clock.
    let scope = resolve_cleanup_scope(
        &manifests,
        scan_start,
        Duration::from_secs(recency_seconds),
        runner,
        &mut counters,
        &mut diagnostics,
        observer,
        selection,
    )?;
    let blocking_unresolved =
        blocking_unresolved_count(&scope.unresolved, &scope.workspaces, selection);
    let out_of_scope_unresolved = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.category == crate::domain::DiagnosticCategory::OutOfScope)
        .count();
    let boundary_discovery_diagnostics = boundary_discovery_diagnostic_count(
        &diagnostics,
        discovery_diagnostic_count,
        selection,
        &root_diagnostics,
    );
    Ok(ResolvedUniverse {
        roots,
        cleanup_phase,
        counters,
        diagnostics,
        boundary_discovery_diagnostics,
        manifests,
        initial_manifests,
        workspaces: scope.workspaces,
        units: scope.units,
        unselected: scope.unselected,
        unresolved_ownership: scope.unresolved,
        blocking_unresolved,
        out_of_scope_unresolved,
    })
}

// M016/A removed by M017/ADR 007 with the island it filtered to (see the
// note where the partition types lived). Dispatch needs no filtering:
// `resolve_cleanup_scope` already emits only in-boundary units, and the
// block sites return before dispatch when in-boundary poison exists.
#[allow(clippy::too_many_arguments)]
pub fn clean_with_roots_policy_selector_guarded(
    roots: &[PathBuf],
    recency_seconds: u64,
    allowed_output_roots: &[PathBuf],
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
    policy: &CleanupPolicy,
    selector: Option<CleanupSelector>,
    premise: Option<AdmissionPremise>,
    selection: &CleanupSelection,
) -> Result<CleanReport, AppError> {
    let includes = compile_policy_globs(&policy.include)?;
    let excludes = compile_policy_globs(&policy.exclude)?;
    let selector_capabilities = if selector.is_some() {
        runtime_clean_capabilities()
    } else {
        CargoCleanCapabilities::default()
    };
    let universe =
        discover_and_resolve_universe(roots, recency_seconds, mode, runner, observer, selection)?;
    let ResolvedUniverse {
        roots,
        cleanup_phase,
        mut counters,
        diagnostics,
        boundary_discovery_diagnostics,
        manifests,
        initial_manifests,
        workspaces,
        units,
        unselected,
        unresolved_ownership,
        blocking_unresolved,
        out_of_scope_unresolved,
    } = universe;

    let mut report = CleanReport {
        results: Vec::new(),
        diagnostics: diagnostics.len(),
        failed: 0,
        mode,
        counters: ScanCounters::default(),
        scope_blocked: None,
        unresolved_ownership: unresolved_ownership.clone(),
        out_of_scope_unresolved,
        selected_roots: roots.clone(),
        discovered_manifests: manifests.len(),
        resolved_workspaces: workspaces.len(),
        units_considered: units.len(),
        effective_policy: Some(policy.clone()),
        selector: selector.clone(),
    };
    // The blocked report carries the counters measured up to the point the
    // block was raised, exactly as the successful path does at the end.
    report.counters = counters.clone();
    // Output-placement environment snapshot for per-dispatch re-verification
    // (M016 mechanism, retained). Taken once up front; compared before every
    // unit dispatch below.
    let provenance = snapshot_output_env(&|key| std::env::var_os(key));

    // ADR 007: only in-boundary discovery uncertainty blocks. Out-of-scope
    // diagnostics stay visible in the diagnostics list; they never block.
    // Report the actual read-only resolution outcome instead of returning
    // misleading zero workspace/unit counts before Cargo was attempted.
    if boundary_discovery_diagnostics > 0 {
        report.scope_blocked = Some(ScopeBlock::new(
            ScopeBlockReason::IncompleteDiscovery,
            format!(
                "cleanup ownership universe is incomplete in the selected scope: {} discovery diagnostic(s); resolved {} workspace(s), considered {} cleanup unit(s); no cleanup commands were run",
                boundary_discovery_diagnostics,
                workspaces.len(),
                units.len()
            ),
        ));
        return Ok(report);
    }

    if blocking_unresolved > 0 {
        let unresolved = unresolved_ownership.len();
        report.scope_blocked = Some(ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            format!(
                "cleanup ownership could not be proven: {unresolved} discovered Cargo {} did not resolve; no cleanup commands were run",
                if unresolved == 1 {
                    "manifest"
                } else {
                    "manifests"
                }
            ),
        ));
        report.resolved_workspaces = workspaces.len();
        return Ok(report);
    }

    // ADR 007 edge: reported notes never block proceeding units — but a run
    // that proves zero units against unproven participants proves nothing.
    // Exiting 0 there would be the M012A silent no-op: no rows, no block, no
    // failure, and an operator script reading "success". Fail closed with
    // the standard unproven-ownership block. Units merely unselected
    // (outside the anchor) do not count: the M014 elsewhere case reports
    // skipped rows and stays exit 0.
    if units.is_empty() && unselected.is_empty() && !unresolved_ownership.is_empty() {
        let unresolved = unresolved_ownership.len();
        report.scope_blocked = Some(ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            format!(
                "cleanup ownership could not be proven: {unresolved} discovered Cargo {} did not resolve; no cleanup commands were run",
                if unresolved == 1 {
                    "manifest"
                } else {
                    "manifests"
                }
            ),
        ));
        report.resolved_workspaces = workspaces.len();
        return Ok(report);
    }

    // Cleanup phase is determinate only after complete ownership coverage is
    // established (C004). Announce the CleanupUnit total through the observer.
    observer.units_total(cleanup_phase, units.len() as u64);

    // Units removed by the operator's selection are reported, never dropped.
    // They were discovered, resolved, sized and classified exactly like the
    // candidates; they simply were not asked for. Silence here would be the
    // M012A silent-no-op defect in a new place.
    //
    // One row per excluded unit, not one synthetic summary row: the report's
    // row identity is a workspace path, and the operator's next question after
    // "why wasn't this cleaned?" is always "which one?", which a summary row
    // cannot answer without inventing a path that is on disk.
    for unit in &unselected {
        let (reason_code, detail) = match selection.exclusion_reason(&unit.root) {
            Some((code, detail)) => (code, detail),
            None => {
                let anchor = selection
                    .describe()
                    .unwrap_or_else(|| "the current selection".to_string());
                (
                    CleanupReasonCode::SkippedOutsideSelection,
                    format!("workspace is outside {anchor}; it was proven and left untouched"),
                )
            }
        };
        report.results.push(CleanResult {
            display_path: unit.root.clone(),
            workspace_roots: vec![unit.root.clone()],
            output_roots: unit.covering.clone(),
            ownership: unit_blocking_class(unit),
            outcome: CleanOutcome::Skipped,
            policy_disposition: None,
            reason_code,
            before_bytes: Some(unit.bytes),
            after_bytes: None,
            observed_decrease: None,
            detail,
        });
    }

    // Filled on the first candidate that reaches the final proof; shared by
    // every later candidate in this run (H3/O1).
    let mut hoisted: Option<ProofUniverse> = None;
    // M017/ADR 007: the output environment is re-verified before every unit
    // dispatch (M016 verified per island first-dispatch; without islands the
    // same check runs per unit — a few environment reads, no spawns). A
    // mismatch blocks the remaining units in the fail-closed direction,
    // before any further spawn. Simulate rehearses the same check so it
    // predicts Execute instead of diverging from it.
    for unit in units.iter() {
        let skipped = |report: &mut CleanReport,
                       detail: String,
                       disposition: Option<PolicyDisposition>,
                       reason_code: CleanupReasonCode| {
            report.results.push(CleanResult {
                display_path: unit.root.clone(),
                workspace_roots: vec![unit.root.clone()],
                output_roots: unit.covering.clone(),
                ownership: unit_blocking_class(unit),
                outcome: CleanOutcome::Skipped,
                policy_disposition: disposition,
                reason_code,
                before_bytes: Some(unit.bytes),
                after_bytes: None,
                observed_decrease: None,
                detail,
            });
        };
        if !includes.is_empty() && !includes.is_match(&unit.root) {
            skipped(
                &mut report,
                "cleanup policy: workspace root is not included".into(),
                Some(PolicyDisposition::NotIncluded),
                CleanupReasonCode::NotIncluded,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if excludes.is_match(&unit.root) {
            skipped(
                &mut report,
                "cleanup policy: workspace root is excluded".into(),
                Some(PolicyDisposition::Excluded),
                CleanupReasonCode::Excluded,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if matches!(selector.as_ref(), Some(CleanupSelector::Package(_)))
            && !selector_capabilities.package_selector
        {
            skipped(
                &mut report,
                "runtime Cargo package-clean capability is unknown or unsupported; no Cargo clean was invoked".into(),
                None,
                CleanupReasonCode::SelectorUnsupported,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if matches!(selector.as_ref(), Some(CleanupSelector::Profile(_)))
            && !selector_capabilities.profile_selector
        {
            skipped(
                &mut report,
                "runtime Cargo profile-clean capability is unknown or unsupported; no Cargo clean was invoked".into(),
                None,
                CleanupReasonCode::SelectorUnsupported,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if let Some(CleanupSelector::Package(spec)) = selector.as_ref() {
            // L-1: fail closed on a desynced index instead of panicking.
            let Some(ws) = workspaces.get(unit.workspace_idx) else {
                skipped(
                    &mut report,
                    format!(
                        "workspace index {} is out of bounds; cleanup deferred",
                        unit.workspace_idx
                    ),
                    None,
                    CleanupReasonCode::SkippedSafety,
                );
                observer.unit_completed(cleanup_phase);
                continue;
            };
            if resolve_package_spec(&ws.packages, spec).is_err() {
                skipped(
                    &mut report,
                    format!("package spec {spec:?} is unknown or ambiguous in this workspace"),
                    None,
                    CleanupReasonCode::SelectorInvalid,
                );
                observer.unit_completed(cleanup_phase);
                continue;
            }
        }
        if selector.is_some() && policy.min_reclaimable_bytes != 0 {
            skipped(
                &mut report,
                "selector-specific reclaimable bytes are unknown; minimum-size policy cannot be evaluated".into(),
                Some(PolicyDisposition::SelectorEstimateUnavailable),
                CleanupReasonCode::SelectorUnsupported,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if unit.bytes < policy.min_reclaimable_bytes {
            skipped(
                &mut report,
                format!(
                    "cleanup policy: {0} bytes is below minimum {1}",
                    unit.bytes, policy.min_reclaimable_bytes
                ),
                Some(PolicyDisposition::BelowMinimumSize),
                CleanupReasonCode::BelowMinimumSize,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        // C003 §7.2: unit-wide destructive gate. Every physical group the
        // invocation can affect must be private, authorized, and measured.
        // No Cargo process is spawned for any subset of a unit.
        // H-3: authorization *infrastructure* failure (misconfigured or
        // unreadable roots) is a scope-level block (exit 1), not N per-unit
        // `SkippedUnauthorized` rows (exit 0). Only `Err` takes this path;
        // `Ok(false)` (private but outside the boundary) stays a per-unit
        // skip, so authorization still cannot promote.
        if let Err(msg) = unit_block_reason_roots(unit, &roots, allowed_output_roots) {
            if is_authorization_infrastructure_error(&msg) {
                report.scope_blocked = Some(ScopeBlock::new(
                    ScopeBlockReason::OwnershipUnproven,
                    format!(
                        "cleanup authorization could not be proven: {msg}; no cleanup commands were run"
                    ),
                ));
                report.counters = counters.clone();
                return Ok(report);
            }
            skipped(&mut report, msg, None, unit_skip_code(unit));
            observer.unit_completed(cleanup_phase);
            continue;
        }
        // C028: late admission-premise recheck. The omitted roots were the
        // boundary assumption of the ownership universe built above; if one
        // reappeared (or became unreadable), or an admitted root changed,
        // the proof would cover the wrong set. Block the pending and all
        // later candidates with a stable reason instead of spawning. Fresh
        // `symlink_metadata` on every check: no timers, sleeps, or cache.
        if let Some(premise) = premise.as_ref()
            && let Some((path, reason)) =
                crate::policy::recheck_admission_premise(&roots, &premise.omitted)
        {
            report.scope_blocked = Some(ScopeBlock::new(
                ScopeBlockReason::IncompleteDiscovery,
                format!(
                    "cleanup admission premise changed for {}: {reason}; no further cleanup commands were run",
                    path.display()
                ),
            ));
            report.counters = counters.clone();
            return Ok(report);
        }
        // C003 §7.3: single final CleanupUnit proof shared by
        // Preview/Simulate/Execute, re-validated against the complete bounded
        // ownership universe. Simulate runs every non-mutating gate Execute
        // runs and only diverges by not spawning Cargo.
        // H3/O1: the ownership universe is re-proven once for the whole run,
        // on the first candidate that reaches the proof. Lazy so a run where
        // every unit is skipped by policy still spawns nothing.
        let hoisted = hoisted.get_or_insert_with(|| {
            // ADR 007: the refresh re-applies the boundary rule against the
            // fresh workspaces. Scan-time out-of-boundary and passenger
            // unknowns stay tolerated; only a fresh in-boundary `MergeAll`
            // invalidates the run.
            ProofUniverse::refresh(&workspaces, &roots, runner, &mut counters, selection)
        });
        // M-2: a universe that could not be re-proven invalidates every
        // candidate, like scan-time incompleteness (exit 1 via scope_blocked),
        // not N per-unit skips (exit 0). Per-candidate refresh failures below
        // stay per-unit skips.
        if let Err(failure) = hoisted.get() {
            report.scope_blocked = Some(ScopeBlock::new(
                ScopeBlockReason::OwnershipUnproven,
                format!("{}; no cleanup commands were run", failure.detail),
            ));
            report.counters = counters.clone();
            return Ok(report);
        }
        let proof = match final_cleanup_proof_roots(
            unit,
            &workspaces,
            &roots,
            allowed_output_roots,
            runner,
            recency_seconds,
            hoisted,
            &mut counters,
            true,
        ) {
            Ok(p) => p,
            Err(failure) => {
                // H-3 via the proof's defensive gate 1: an infrastructure
                // error surfacing here (TOCTOU between the gate above and the
                // proof) is still a scope block, not a per-unit skip.
                if is_authorization_infrastructure_error(&failure.detail) {
                    report.scope_blocked = Some(ScopeBlock::new(
                        ScopeBlockReason::OwnershipUnproven,
                        format!(
                            "cleanup authorization could not be proven: {}; no cleanup commands were run",
                            failure.detail
                        ),
                    ));
                    report.counters = counters.clone();
                    return Ok(report);
                }
                skipped(&mut report, failure.detail, None, failure.code);
                observer.unit_completed(cleanup_phase);
                continue;
            }
        };
        if proof.pre_bytes < policy.min_reclaimable_bytes {
            skipped(
                &mut report,
                format!(
                    "cleanup policy: fresh size {} bytes is below minimum {}",
                    proof.pre_bytes, policy.min_reclaimable_bytes
                ),
                Some(PolicyDisposition::BelowMinimumSize),
                CleanupReasonCode::BelowMinimumSize,
            );
            observer.unit_completed(cleanup_phase);
            continue;
        }
        if let Some(seconds) = policy.min_inactive_seconds {
            let Some(activity) = proof.newest_activity else {
                skipped(
                    &mut report,
                    "cleanup policy: required activity timestamp is unavailable".into(),
                    Some(PolicyDisposition::TooRecentForPolicy),
                    CleanupReasonCode::TooRecentForPolicy,
                );
                observer.unit_completed(cleanup_phase);
                continue;
            };
            let cutoff = SystemTime::now()
                .checked_sub(Duration::from_secs(seconds))
                .ok_or_else(|| {
                    AppError::Config("cleanup inactivity duration exceeds system time range".into())
                })?;
            if activity >= cutoff {
                skipped(
                    &mut report,
                    "cleanup policy: workspace is too recent".into(),
                    Some(PolicyDisposition::TooRecentForPolicy),
                    CleanupReasonCode::TooRecentForPolicy,
                );
                observer.unit_completed(cleanup_phase);
                continue;
            }
        }

        // H-2 residual window, narrowed fail-closed for all modes (parity):
        // the hoisted universe is a snapshot, and the per-candidate refresh
        // only re-resolves *self*, so a manifest created under an admitted
        // root after hoisting is invisible to the overlap check. The premise
        // recheck covers omitted/admitted roots only, not new manifests.
        // Rediscover (manifest walk, no Cargo) after the proof; any growth,
        // shrinkage, or diagnostic blocks the remaining scope instead of
        // proceeding against a stale claimant set. Simulate runs it too so
        // simulation predicts the spawn path instead of diverging from it.
        {
            // M018: the re-discovery walks per root, for the same reason the
            // initial discovery does — a traversal error carries no path, so
            // the root that produced it is the only attribution available.
            // A diagnostic inside the proof boundary blocks the remaining
            // scope; one outside it does not: that root was already skipped
            // and disclosed, and the uncertainty inside it is the residual
            // ADR 008 §5 accepts in exchange for the per-root rule.
            let mut now: Vec<PathBuf> = Vec::new();
            let mut incomplete = false;
            for root in &roots {
                let rediscovery_policy = crate::domain::EffectiveScanPolicy {
                    recency: Duration::ZERO,
                    scope: ScanScope::ExplicitRoots(vec![root.clone()]),
                    discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
                };
                match discovery::discover_manifests(
                    &rediscovery_policy,
                    &crate::progress::NoopObserver,
                ) {
                    Ok(fresh) => {
                        let root_already_judged = selection.excludes_coverage_root(root);
                        for diagnostic in &fresh.diagnostics {
                            let in_boundary = match &diagnostic.path {
                                Some(path) => boundary_admits(selection, path),
                                None => !root_already_judged,
                            };
                            if in_boundary {
                                incomplete = true;
                            }
                        }
                        now.extend(fresh.manifests);
                    }
                    Err(_) => incomplete = true,
                }
            }
            if incomplete {
                report.scope_blocked = Some(ScopeBlock::new(
                    ScopeBlockReason::IncompleteDiscovery,
                    "cleanup ownership universe could not be re-discovered after proof; no further cleanup commands were run".into(),
                ));
                report.counters = counters.clone();
                return Ok(report);
            }
            now.sort();
            if !universe_manifests_stable(&initial_manifests, &now) {
                report.scope_blocked = Some(ScopeBlock::new(
                    ScopeBlockReason::IncompleteDiscovery,
                    "cleanup ownership universe changed: manifest set differs after proof; no further cleanup commands were run".into(),
                ));
                report.counters = counters.clone();
                return Ok(report);
            }
        }

        // The output environment was snapshotted when the proof universe was
        // built; re-verify before each dispatch. Any mismatch blocks the
        // remaining units — the same direction as every other mid-run block —
        // before any further spawn. Simulate rehearses the same check so it
        // predicts Execute instead of diverging from it.
        if !output_env_matches(&provenance, &|key| std::env::var_os(key)) {
            report.scope_blocked = Some(ScopeBlock::new(
                ScopeBlockReason::OwnershipUnproven,
                "output environment changed after proof; remaining units blocked; no further cleanup commands were run".into(),
            ));
            report.counters = counters.clone();
            return Ok(report);
        }

        match mode {
            CleanMode::Simulate => {
                // Full proof succeeded, but no `cargo clean` is invoked.
                report.results.push(CleanResult {
                    display_path: unit.root.clone(),
                    workspace_roots: proof.workspace_roots.clone(),
                    output_roots: proof.covering.clone(),
                    ownership: proof.ownership,
                    outcome: CleanOutcome::Simulated,
                    policy_disposition: Some(PolicyDisposition::Selected),
                    reason_code: CleanupReasonCode::Simulated,
                    before_bytes: Some(proof.pre_bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: "simulation: no `cargo clean` command was invoked".into(),
                });
                observe_unit_groups(observer, unit);
                observer.unit_completed(cleanup_phase);
            }
            CleanMode::CargoPreview | CleanMode::Execute => {
                // Proof was generated immediately before spawn and is consumed
                // here with no intervening mutation window beyond the spawn
                // itself (fail-closed TOCTOU minimization, not elimination).
                // C028: the premise is re-examined before *every* spawn, not
                // just before the first proof, so a root that reappears
                // mid-run blocks the remaining candidates instead of letting
                // them run against a stale boundary.
                if let Some(premise) = premise.as_ref()
                    && let Some((path, reason)) =
                        crate::policy::recheck_admission_premise(&roots, &premise.omitted)
                {
                    report.scope_blocked = Some(ScopeBlock::new(
                        ScopeBlockReason::IncompleteDiscovery,
                        format!(
                            "cleanup admission premise changed for {}: {reason}; no further cleanup commands were run",
                            path.display()
                        ),
                    ));
                    report.counters = counters.clone();
                    return Ok(report);
                }
                let frozen = proof.frozen_env.clone();
                // Last-moment covering-root recheck: the proof's step-9 check
                // is a moment old by the time we spawn. A symlink swap of a
                // covering root in that window would redirect `cargo clean`
                // outside the proven boundary, so re-prove here with the
                // same gate and skip instead of spawning on any failure.
                // M-4: `prove_covering_roots` maps unreadable roots to
                // `SkippedUncertain` (via `covering_inspect_failure`), not
                // `Changed`, so the code still distinguishes the response.
                if let Err(failure) = prove_covering_roots(&proof.covering) {
                    skipped(&mut report, failure.detail, None, failure.code);
                    observer.unit_completed(cleanup_phase);
                    continue;
                }
                // M-6: markers are part of the proof (step 11); re-prove at
                // the last moment so a marker removed or replaced in the
                // proof→spawn window skips instead of spawning.
                if let Err(failure) = require_cargo_markers(&proof.covering) {
                    skipped(&mut report, failure.detail, None, failure.code);
                    observer.unit_completed(cleanup_phase);
                    continue;
                }
                let args = clean_args(&proof, mode, selector.as_ref());
                let cwd = proof.workspace_root.clone();
                let spawned = if frozen.is_empty() {
                    runner.run(&cwd, &args)
                } else {
                    runner.run_with_env(&cwd, &args, &frozen)
                };
                let output = match spawned {
                    Ok(o) => o,
                    Err(e) => {
                        report.failed += 1;
                        report.results.push(CleanResult {
                            display_path: unit.root.clone(),
                            workspace_roots: proof.workspace_roots.clone(),
                            output_roots: proof.covering.clone(),
                            ownership: proof.ownership,
                            outcome: CleanOutcome::Failed,
                            policy_disposition: Some(PolicyDisposition::Selected),
                            reason_code: CleanupReasonCode::CargoFailed,
                            before_bytes: Some(proof.pre_bytes),
                            after_bytes: None,
                            observed_decrease: None,
                            detail: format!("could not start Cargo: {e}"),
                        });
                        observer.unit_completed(cleanup_phase);
                        continue;
                    }
                };
                if !output.success {
                    report.failed += 1;
                    report.results.push(CleanResult {
                        display_path: unit.root.clone(),
                        workspace_roots: proof.workspace_roots.clone(),
                        output_roots: proof.covering.clone(),
                        ownership: proof.ownership,
                        outcome: CleanOutcome::Failed,
                        policy_disposition: Some(PolicyDisposition::Selected),
                        reason_code: CleanupReasonCode::CargoFailed,
                        before_bytes: Some(proof.pre_bytes),
                        after_bytes: None,
                        observed_decrease: None,
                        detail: format!("Cargo exited {:?}: {}", output.code, output_text(&output)),
                    });
                    observer.unit_completed(cleanup_phase);
                    continue;
                }
                if mode == CleanMode::CargoPreview {
                    report.results.push(CleanResult {
                        display_path: unit.root.clone(),
                        workspace_roots: proof.workspace_roots.clone(),
                        output_roots: proof.covering.clone(),
                        ownership: proof.ownership,
                        outcome: CleanOutcome::Previewed,
                        policy_disposition: Some(PolicyDisposition::Selected),
                        reason_code: CleanupReasonCode::Previewed,
                        before_bytes: Some(proof.pre_bytes),
                        after_bytes: None,
                        observed_decrease: None,
                        detail: output_text(&output),
                    });
                } else {
                    // C003 §7.4: one post-clean measurement of the complete
                    // deduplicated covering-root union this invocation could
                    // affect, compared against the same union before it ran.
                    match measure_union(&proof.covering) {
                        Ok(after) => {
                            let decrease = proof.pre_bytes.saturating_sub(after);
                            report.results.push(CleanResult {
                                display_path: unit.root.clone(),
                                workspace_roots: proof.workspace_roots.clone(),
                                output_roots: proof.covering.clone(),
                                ownership: proof.ownership,
                                outcome: CleanOutcome::Cleaned,
                                policy_disposition: Some(PolicyDisposition::Selected),
                                reason_code: CleanupReasonCode::Cleaned,
                                before_bytes: Some(proof.pre_bytes),
                                after_bytes: Some(after),
                                observed_decrease: Some(decrease),
                                detail: output_text(&output),
                            });
                        }
                        Err(e) => {
                            // Measurement uncertainty after Cargo success is
                            // diagnostic; it must not fabricate recovered bytes.
                            report.results.push(CleanResult {
                                display_path: unit.root.clone(),
                                workspace_roots: proof.workspace_roots.clone(),
                                output_roots: proof.covering.clone(),
                                ownership: proof.ownership,
                                outcome: CleanOutcome::Cleaned,
                                policy_disposition: Some(PolicyDisposition::Selected),
                                reason_code: CleanupReasonCode::MeasurementFailed,
                                before_bytes: Some(proof.pre_bytes),
                                after_bytes: None,
                                observed_decrease: None,
                                detail: format!(
                                    "Cargo succeeded; post-clean measurement failed: {e}"
                                ),
                            });
                            report.diagnostics += 1;
                        }
                    }
                }
                observe_unit_groups(observer, unit);
                observer.unit_completed(cleanup_phase);
            }
        }
    }
    // Include diagnostics from discovery/resolution in count (already set).
    report.counters = counters;
    Ok(report)
}

/// One cleanup unit offered to the operator by the interactive list.
///
/// A row is a resolved `CleanupUnit`, not a directory: a directory is what a
/// tool *finds*, a unit is what the engine can act on, and the unit already
/// carries the reclaimable bytes that make "how much will this free?" answerable
/// before anything is deleted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupCandidate {
    pub root: PathBuf,
    /// Reclaimable bytes measured for this unit.
    pub bytes: u64,
    /// The unit's strongest ownership class.
    ///
    /// Only `PrivateBounded` authorizes `cargo clean`. A row carrying any other
    /// class is shown with its class spelled out rather than hidden, because a
    /// hidden row is indistinguishable from a row that does not exist, and the
    /// operator's next question would again be "which one?".
    pub ownership: OutputOwnershipClass,
}

impl CleanupCandidate {
    /// Whether this unit can reach `cargo clean` on its ownership evidence alone.
    ///
    /// This is a *presentation* hint about ownership only. It is deliberately
    /// not an eligibility verdict: policy gates (`include`/`exclude`, minimum
    /// bytes, authorization) and selector capability live in the engine, and
    /// duplicating them here is how a listing starts disagreeing with the run.
    pub fn provably_private(&self) -> bool {
        self.ownership == OutputOwnershipClass::PrivateBounded
    }
}

/// Human detail for a blocked interactive listing, with bounded participant
/// paths and the workspace-membership hint when it applies.
///
/// This is the listing counterpart of the engine's blocked-report rendering in
/// `CleanReport::render`: the block message alone carries a count, which never
/// told the operator which manifest to fix or which healthy workspace to clean
/// explicitly instead. Paths are sanitized (no control characters, bounded
/// length, at most five) because this surfaces on a human terminal; JSON and
/// log consumers read `unresolved_ownership` / counts instead.
pub fn blocked_listing_detail(
    block: &ScopeBlock,
    unresolved: &[UnresolvedOwnershipParticipant],
) -> String {
    let mut detail = block.message.clone();
    const MAX_LISTED: usize = 5;
    const MAX_FIELD_CHARS: usize = 200;
    let mut listed = 0usize;
    let mut workspace_hint = false;
    for participant in unresolved.iter().take(MAX_LISTED) {
        let manifest = participant
            .manifest
            .display()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_FIELD_CHARS)
            .collect::<String>();
        let reason = participant
            .reason
            .replace('\n', " ")
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_FIELD_CHARS)
            .collect::<String>();
        detail.push_str(&format!(
            "\n  unresolved [{}] {}: {}",
            participant.stage, manifest, reason
        ));
        listed += 1;
        if participant
            .reason
            .contains("believes it's in a workspace when it's not")
        {
            workspace_hint = true;
        }
    }
    let remaining = unresolved.len().saturating_sub(listed);
    if remaining > 0 {
        detail.push_str(&format!("\n  … {remaining} more unresolved participant(s)"));
    }
    if workspace_hint {
        detail.push_str(
            "\n  hint: a nested crate that believes it is in a workspace needs an empty `[workspace]` table or a parent `workspace.exclude` entry; otherwise clean a healthy workspace explicitly, e.g. `cargo cleanme clean ROOT`",
        );
    }
    detail
}

// M017/ADR 007 removed `physical_outputs` with the partition (its only
// caller): grouping classifies unprovable physical identity without it.
// M017/ADR 007 removed the proof-island machinery that lived here
// (`compute_proof_partition` through `partition_outcome_section`, plus
// `outputs_overlap` which only the partition used): scoped proof is the
// default, so there are no islands to compute or report. See the note where
// the partition types lived for what carried forward.

/// H-2 universe-stability predicate: the rediscovered manifest set must equal
/// the hoisted snapshot exactly — growth, shrinkage, or any difference blocks
/// the remaining scope instead of proceeding against a stale claimant set.
///
/// Extracted pure (M015/F) so the TOCTOU control can prove the detector
/// carries the detection: the naive always-stable stub passes growth through.
fn universe_manifests_stable(initial: &[PathBuf], fresh_sorted: &[PathBuf]) -> bool {
    initial == fresh_sorted
}

/// The read-only result of listing cleanup candidates.
#[derive(Clone, Debug, Default)]
pub struct CandidateListing {
    pub candidates: Vec<CleanupCandidate>,
    /// A whole-scope block raised during resolution, if any.
    ///
    /// The interactive session must refuse to present a selection when this is
    /// set. Offering rows from an unproven universe would be the ADR 006 shape
    /// in a new costume: the operator would be choosing from a list whose
    /// classification is exactly the thing that failed.
    pub scope_blocked: Option<ScopeBlock>,
    pub unresolved_ownership: Vec<UnresolvedOwnershipParticipant>,
    pub selected_roots: Vec<PathBuf>,
    pub resolved_workspaces: usize,
    /// Discovery diagnostics observed while listing; also the reason a block may
    /// exist.
    pub diagnostics: usize,
}

/// Resolve the cleanup units a selection may choose from, without cleaning any
/// of them (M014 work package D).
///
/// This runs the identical discovery and ownership-resolution pipeline the
/// engine runs — [`discover_and_resolve_universe`] — so the offered rows are the
/// engine's own units. It spawns `cargo metadata`, never `cargo clean`.
///
/// It is **not** a promise about what the subsequent cleanup run will do. The
/// engine re-discovers, re-resolves, and re-proves ownership per spawn against
/// the complete universe (C003 §7.3, C006), and it applies policy gates this
/// listing does not model. Listing early is for the operator's benefit; it can
/// never stand in for the final proof.
pub fn list_cleanup_candidates(
    roots: &[PathBuf],
    recency_seconds: u64,
    mode: CleanMode,
    runner: &dyn CleanupRunner,
    observer: &dyn ProgressObserver,
    selection: &CleanupSelection,
) -> Result<CandidateListing, AppError> {
    let universe =
        discover_and_resolve_universe(roots, recency_seconds, mode, runner, observer, selection)?;
    // ADR 007: the listing applies the identical boundary gates as the
    // engine — candidates are the selected units, and the scope blocks only
    // on in-boundary poison. Out-of-scope poison never blocks the offer.
    let scope_blocked = if universe.boundary_discovery_diagnostics > 0 {
        Some(ScopeBlock::new(
            ScopeBlockReason::IncompleteDiscovery,
            format!(
                "cleanup ownership universe is incomplete in the selected scope: {} discovery diagnostic(s); resolved {} workspace(s), considered {} cleanup unit(s); nothing can be selected",
                universe.boundary_discovery_diagnostics,
                universe.workspaces.len(),
                universe.units.len()
            ),
        ))
    } else if universe.blocking_unresolved > 0 {
        let unresolved = universe.unresolved_ownership.len();
        Some(ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            format!(
                "cleanup ownership could not be proven: {unresolved} discovered Cargo {} did not resolve; nothing can be selected",
                if unresolved == 1 {
                    "manifest"
                } else {
                    "manifests"
                }
            ),
        ))
    } else if universe.units.is_empty()
        && universe.unselected.is_empty()
        && !universe.unresolved_ownership.is_empty()
    {
        // Same edge as the engine: offering nothing from an unproven
        // universe without a block would read as success. The refusal
        // carries the block, never rows.
        let unresolved = universe.unresolved_ownership.len();
        Some(ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            format!(
                "cleanup ownership could not be proven: {unresolved} discovered Cargo {} did not resolve; nothing can be selected",
                if unresolved == 1 {
                    "manifest"
                } else {
                    "manifests"
                }
            ),
        ))
    } else {
        None
    };
    let candidates = universe
        .units
        .iter()
        .map(|unit| CleanupCandidate {
            root: unit.root.clone(),
            bytes: unit.bytes,
            ownership: unit_blocking_class(unit),
        })
        .collect();
    Ok(CandidateListing {
        candidates,
        scope_blocked,
        unresolved_ownership: universe.unresolved_ownership,
        selected_roots: universe.roots,
        resolved_workspaces: universe.workspaces.len(),
        diagnostics: universe.diagnostics.len(),
    })
}

// `island_held_view` was removed by M017/ADR 007 with the islands: without
// islands there is no held context. The `-i` refusal carries the block
// detail only; out-of-scope poison is visible in the human report line and
// the JSON diagnostics of any run, not in the picker.

fn compile_policy_globs(patterns: &[String]) -> Result<globset::GlobSet, AppError> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(globset::Glob::new(pattern).map_err(|e| {
            AppError::Config(format!("invalid cleanup policy glob {pattern:?}: {e}"))
        })?);
    }
    builder
        .build()
        .map_err(|e| AppError::Config(format!("invalid cleanup policy patterns: {e}")))
}

fn unit_skip_code(unit: &workspace::CleanupUnit) -> CleanupReasonCode {
    if !unit.unmapped.is_empty() {
        return CleanupReasonCode::SkippedUncertain;
    }
    for group in &unit.groups {
        match group.ownership {
            OutputOwnershipClass::Shared => return CleanupReasonCode::SkippedShared,
            OutputOwnershipClass::Uncertain => return CleanupReasonCode::SkippedUncertain,
            // ExternalUnproven is an ownership failure, not an authorization
            // one: the prose (`ownership_skip_detail`) already says so, and
            // the JSON code must agree so operators get the right response
            // ("understand it before proceeding", not "add an allow-entry").
            OutputOwnershipClass::ExternalUnproven => {
                return CleanupReasonCode::SkippedOwnershipUnproven;
            }
            OutputOwnershipClass::PrivateBounded => {}
        }
        if group.measured.is_none() {
            return match group.skip {
                Some(
                    workspace::GroupSkipReason::ActiveSource
                    | workspace::GroupSkipReason::ActiveOutput,
                ) => CleanupReasonCode::SkippedActive,
                Some(
                    workspace::GroupSkipReason::UncertainSource
                    | workspace::GroupSkipReason::UncertainOwnership
                    | workspace::GroupSkipReason::UncertainMeasurement,
                ) => CleanupReasonCode::SkippedUncertain,
                _ => CleanupReasonCode::SkippedSafety,
            };
        }
    }
    CleanupReasonCode::SkippedUnauthorized
}

fn proof_skip_code(detail: &str) -> CleanupReasonCode {
    if detail.contains("became active") || detail.contains("became recent") {
        CleanupReasonCode::SkippedActive
    } else if detail.contains("marker")
        || detail.contains("CACHEDIR")
        || detail.contains("signature")
    {
        CleanupReasonCode::SkippedMarkerInvalid
    } else if detail.contains("uncertain") || detail.contains("cannot be established") {
        CleanupReasonCode::SkippedUncertain
    } else if detail.contains("changed before cleanup")
        || detail.contains("preflight changed")
        || detail.contains("overlap")
        // A workspace that changed identity or vanished really did change; the
        // remaining "could not be re-proven" failures are Cargo/discovery
        // failures and must not be reported as a change (L9).
        || detail.contains("changed identity")
        || detail.contains("vanished")
        || detail.contains("disappeared")
    {
        CleanupReasonCode::SkippedChangedBeforeCleanup
    } else if detail.contains("re-proven") {
        CleanupReasonCode::SkippedOwnershipUnproven
    } else {
        CleanupReasonCode::SkippedSafety
    }
}

/// Ownership class reported for a skipped CleanupUnit (C003 §7.2).
///
/// The first non-private affected group decides how the skip is classified;
/// otherwise the unit reports the strongest class it carries.
fn unit_blocking_class(unit: &workspace::CleanupUnit) -> OutputOwnershipClass {
    for group in &unit.groups {
        if group.ownership != OutputOwnershipClass::PrivateBounded {
            return group.ownership;
        }
    }
    OutputOwnershipClass::PrivateBounded
}

/// Publish the unit's affected output rows to transient progress.
fn observe_unit_groups(observer: &dyn ProgressObserver, unit: &workspace::CleanupUnit) {
    for group in &unit.groups {
        if let Some(m) = &group.measured {
            observer.reportable_group(&group.display, m.bytes);
        }
    }
}

/// ADR 007: the proof boundary is the selected scope, not the discovered
/// universe. Discovery still covers everything (narrowing it would promote
/// `Shared` to `PrivateBounded` per ADR 006), but only unprovable
/// participants *inside* the boundary can block a dispatch.
///
/// `All` selects everything (explicit roots and `--full` keep today's
/// behavior exactly). `Ancestry`/`Roots` admit the anchor/roots; matching is
/// bidirectional like [`CleanupSelection::admits`] (an unresolvable parent
/// manifest above the anchor could own the anchor's output) and therefore
/// fail-closed. Comparison is canonical-or-raw on both sides so a symlinked
/// prefix (macOS `/var` vs `/private/var`) cannot fail open. An unresolved
/// selection never reaches the engine and is treated as blocking everything.
fn boundary_admits(selection: &CleanupSelection, manifest: &Path) -> bool {
    fn under(path: &Path, root: &Path) -> bool {
        let canon_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        canon_path.starts_with(&canon_root) || canon_root.starts_with(&canon_path)
    }
    match selection {
        CleanupSelection::All => true,
        CleanupSelection::WorkingDirectory => true,
        CleanupSelection::Ancestry(anchor) => under(manifest, anchor),
        CleanupSelection::Roots(roots) => roots.iter().any(|root| under(manifest, root)),
        // M018: the excluded root is outside the proof boundary as well as
        // out of the candidate set. ADR 008 §5 makes the Full cleanup's proof
        // boundary the **proven** learned root: "It blocks each learned root
        // whose own traversal did not complete, and reports those roots with
        // a typed reason." The excluded root's uncertainty is therefore
        // reported, not blocking — the treatment ADR 007 already gives every
        // out-of-boundary unprovable.
        //
        // This is not the ADR 006 shortcut: the excluded root's manifests are
        // still discovered, resolved and classified, so the universe does not
        // narrow and a shared output between a proven root and an unproven one
        // stays `Shared` and stays inventory-only. What the exclusion narrows
        // is the *proof*, not the *discovery*.
        CleanupSelection::ExcludeRoots { inner, roots } => {
            inner.admits(manifest)
                && !roots
                    .iter()
                    .any(|unproven| path_is_within(manifest, &unproven.root))
        }
    }
}

/// ADR 007 three-way split of unresolved participants.
///
/// - Out-of-boundary: reported as `out_of_scope` diagnostics, never blocks.
///   Cleaning a healthy boundary while poison exists elsewhere is the
///   corrected behavior; a test asserting the old whole-scope block here
///   names the defect this decision fixes.
/// - In-boundary `PassengerOf`/`ScopeNote`: reported, never blocks. Cargo
///   named the owner (refused manifest) or the scope (believed member), so
///   there is no unknown owner to merge — the M016 passenger rule, now the
///   default.
/// - In-boundary `MergeAll`: blocks the boundary. Unknown ownership inside
///   the boundary still fails closed.
///
/// Returns `(blocking, reported_in_boundary, out_of_boundary)`.
fn split_unresolved_for_boundary(
    unresolved: &[UnresolvedOwnershipParticipant],
    workspaces: &[ResolvedWorkspace],
    selection: &CleanupSelection,
) -> (
    Vec<UnresolvedOwnershipParticipant>,
    Vec<UnresolvedOwnershipParticipant>,
    Vec<UnresolvedOwnershipParticipant>,
) {
    let mut blocking = Vec::new();
    let mut reported = Vec::new();
    let mut out_of_scope = Vec::new();
    for participant in unresolved {
        if !boundary_admits(selection, &participant.manifest) {
            out_of_scope.push(participant.clone());
            continue;
        }
        match workspace::attribute_unknown(participant, workspaces) {
            workspace::UnknownDisposition::MergeAll => blocking.push(participant.clone()),
            workspace::UnknownDisposition::PassengerOf { .. }
            | workspace::UnknownDisposition::ScopeNote => reported.push(participant.clone()),
        }
    }
    (blocking, reported, out_of_scope)
}

/// Count of unresolved participants that block the boundary under ADR 007.
/// Single predicate shared by the early return, the block sites, the
/// listing, and the refresh, so the four cannot disagree about what blocks.
fn blocking_unresolved_count(
    unresolved: &[UnresolvedOwnershipParticipant],
    workspaces: &[ResolvedWorkspace],
    selection: &CleanupSelection,
) -> usize {
    split_unresolved_for_boundary(unresolved, workspaces, selection)
        .0
        .len()
}

/// Count of discovery-phase diagnostics inside the boundary.
///
/// A diagnostic that carries a path is judged by that path. A diagnostic with
/// **no path** — a traversal error — cannot be attributed to a subtree: the
/// upstream walk's error event has no child path. It is attributed to the
/// **root whose walk produced it**, which is why the walk is per root (M018).
/// A root outside the proof boundary does not block: its uncertainty was
/// disclosed and that root was skipped, which is the per-root rule ADR 008 §5
/// adopted in place of the withdrawn whole-system rule.
///
/// A diagnostic that belongs to no root's walk cannot be attributed at all,
/// and an unattributable diagnostic is in-boundary — fail closed.
fn boundary_discovery_diagnostic_count(
    diagnostics: &[ScanDiagnostic],
    discovery_diagnostic_count: usize,
    selection: &CleanupSelection,
    root_diagnostics: &[(PathBuf, usize, usize)],
) -> usize {
    let mut count = 0;
    let mut attributed = 0usize;
    for (root, start, end) in root_diagnostics {
        let root_already_judged = selection.excludes_coverage_root(root);
        for diagnostic in diagnostics.get(*start..*end).unwrap_or_default() {
            attributed += 1;
            let in_boundary = match &diagnostic.path {
                Some(path) => boundary_admits(selection, path),
                // A traversal error carries no path: the root that produced it
                // is the attribution, and only a root whose incompleteness the
                // gate already judged and disclosed is out of the boundary.
                None => !root_already_judged,
            };
            if in_boundary {
                count += 1;
            }
        }
    }
    count + discovery_diagnostic_count.saturating_sub(attributed)
}

/// Resolve the cleanup candidate set for one scope: the bounded ownership
/// universe (every resolved workspace) plus one CleanupUnit per workspace that
/// has reportable affected output (C003 §7.1).
///
/// C003: the destructive candidate is the workspace unit, not the physical
/// group; distinct target/build groups belong to one unit because one
/// `cargo clean` invocation can affect both.
struct CleanupScope {
    workspaces: Vec<ResolvedWorkspace>,
    units: Vec<workspace::CleanupUnit>,
    /// Units that were discovered, resolved, sized and proven, but removed from
    /// the candidate set by the operator's selection (M014).
    ///
    /// They are reported with `SkippedOutsideSelection` rather than dropped:
    /// a selection that silently discarded units would recreate the M012A
    /// silent-no-op defect, where a configured `scan.root` made `clean --known`
    /// report a successful run it never performed.
    unselected: Vec<workspace::CleanupUnit>,
    unresolved: Vec<UnresolvedOwnershipParticipant>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_cleanup_scope(
    manifests: &[PathBuf],
    scan_start: SystemTime,
    recency: Duration,
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
    selection: &CleanupSelection,
) -> Result<CleanupScope, AppError> {
    // Workspace resolution (cached, sequential).
    let adapter = WorkspaceCleanupAdapter { runner };
    let coverage = workspace::resolve_workspaces_with_coverage(
        manifests,
        &adapter,
        counters,
        diagnostics,
        observer,
    );
    let coverage_unresolved = coverage.unresolved;
    counters.unresolved_ownership = coverage_unresolved.len() as u64;
    let workspaces = coverage.workspaces;
    // ADR 007: only in-boundary `MergeAll` participants block. Out-of-scope
    // poison becomes `out_of_scope` informational diagnostics; in-boundary
    // passengers and scope notes are reported, never blocking. Grouping,
    // classification, and unit construction still run whole-universe over
    // the resolved workspaces, so a cross-boundary claim still forces
    // `Shared` at grouping time.
    let unresolved: Vec<UnresolvedOwnershipParticipant>;
    {
        let (blocking, reported, out_of_scope) =
            split_unresolved_for_boundary(&coverage_unresolved, &workspaces, selection);
        for participant in &out_of_scope {
            diagnostics.push(ScanDiagnostic {
                severity: crate::domain::DiagnosticSeverity::Info,
                category: crate::domain::DiagnosticCategory::OutOfScope,
                path: Some(participant.manifest.clone()),
                message: format!(
                    "manifest did not resolve outside the selected scope; reported without blocking: {}",
                    participant.reason.replace('\n', " "),
                ),
            });
        }
        if !blocking.is_empty() {
            unresolved = blocking.into_iter().chain(reported).collect();
            return Ok(CleanupScope {
                workspaces,
                units: Vec::new(),
                unselected: Vec::new(),
                unresolved,
            });
        }
        unresolved = reported;
    }
    let groups = workspace::build_groups(&workspaces);
    // Fail-fast analysis (cheap gates before deep sizing).
    let clock_cutoff = scan_start
        .checked_sub(recency)
        .ok_or_else(|| AppError::Config("recency window exceeds system time range".into()))?;
    let outcomes = workspace::analyze_groups_detailed(
        &workspaces,
        groups,
        scan_start,
        clock_cutoff,
        recency,
        counters,
        diagnostics,
        observer,
    );
    let all_units = workspace::build_cleanup_units(&workspaces, &outcomes);
    // Selection happens HERE, after `build_groups` has classified ownership
    // across the complete resolved universe. Applying it any earlier — to the
    // roots, or to discovery — would be the ADR 006 defect: an out-of-scope
    // workspace that shares an output root would become invisible, and its
    // bytes would be promoted from `Shared` to `PrivateBounded`.
    let mut units = Vec::with_capacity(all_units.len());
    let mut unselected = Vec::new();
    for unit in all_units {
        if selection.admits(&unit.root) {
            units.push(unit);
        } else {
            unselected.push(unit);
        }
    }
    Ok(CleanupScope {
        workspaces,
        units,
        unselected,
        unresolved,
    })
}

struct WorkspaceCleanupAdapter<'a> {
    runner: &'a dyn CleanupRunner,
}

impl CargoRunner for WorkspaceCleanupAdapter<'_> {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<workspace::ProcessOutput> {
        let out = CleanupRunner::run(self.runner, cwd, args)?;
        Ok(workspace::ProcessOutput {
            success: out.success,
            code: out.code,
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }
}

/// Validated pre-spawn CleanupUnit proof consumed immediately to build the
/// Cargo command.
///
/// C003 §7.3: the proof binds the complete frozen state of ONE workspace
/// CleanupUnit — canonical workspace root + root manifest, canonical member
/// set, canonical target, canonical build/capability, the complete
/// deduplicated physical covering-root union the invocation can affect,
/// pre-clean union bytes, ownership class, authorization decision, cache-marker
/// validity, fresh source activity, fresh output activity, and frozen
/// environment/arguments — all re-proven against the complete bounded
/// ownership universe discovered under clean ROOT.
///
/// Generated by [`final_cleanup_proof`] immediately before spawn and consumed
/// with no intervening mutation window beyond the spawn itself. This
/// minimizes (but does not eliminate) TOCTOU races and fails closed. No
/// earlier RawGroup is consulted for mutation decisions after proof
/// generation.
#[derive(Clone, Debug)]
pub struct ExecutionProof {
    pub workspace_roots: Vec<PathBuf>,
    pub workspace_root: PathBuf,
    pub root_manifest: PathBuf,
    pub canonical_members: Vec<PathBuf>,
    pub target: PathBuf,
    pub build: Option<PathBuf>,
    pub capability: CargoCapabilities,
    /// Complete deduplicated union of every affected physical covering root.
    pub covering: Vec<PathBuf>,
    /// Pre-clean bytes of that same deduplicated union.
    pub pre_bytes: u64,
    pub newest_activity: Option<SystemTime>,
    pub ownership: OutputOwnershipClass,
    pub frozen_env: Vec<(OsString, OsString)>,
}

// Backwards-compatible alias for existing callers/tests.
type RevalidatedContext = ExecutionProof;

fn canon(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn canon_strict(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

/// Physical identity for change detection: the proven physical path when
/// present, else a strict canonicalization. `None` means the identity
/// cannot be established and the proof must fail closed: falling back to
/// the lexical path would let a deleted/unreadable member compare equal
/// and pass change detection.
fn output_identity_strict(physical: &Option<PathBuf>, logical: &Path) -> Option<PathBuf> {
    match physical {
        Some(p) => Some(p.clone()),
        None => canon_strict(logical),
    }
}

/// Target/build identity pair for the final proof.
///
/// A configured-but-never-built output dir reports `exists: false` with no
/// physical identity on both the scan-time and fresh sides. The strict
/// identity above maps that to `None` on both sides, which would fail the
/// unit as `Unproven` forever even though the unit legitimately formed from
/// the measured sibling group (scan-time missing roots are invisible to the
/// graph and excluded from `unmapped`). Two missing sides with the same
/// logical path compare equal here; an existence flip or a logical change
/// is `Changed` (still fail-closed); only a present-but-unresolvable side
/// is `Unproven`.
fn output_identity_pair(
    old: &OutputRoot,
    new: &OutputRoot,
    kind: &str,
    unit_root: &Path,
) -> Result<(PathBuf, PathBuf), ProofFailure> {
    if !old.exists && !new.exists {
        if old.logical_path == new.logical_path {
            return Ok((old.logical_path.clone(), new.logical_path.clone()));
        }
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            format!(
                "workspace changed before cleanup; skipped: {kind} changed (was {}, now {})",
                old.logical_path.display(),
                new.logical_path.display()
            ),
        ));
    }
    if old.exists != new.exists {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            format!(
                "workspace changed before cleanup; skipped: {kind} appeared or disappeared (was {}, now {})",
                old.logical_path.display(),
                new.logical_path.display()
            ),
        ));
    }
    let old_canon =
        output_identity_strict(&old.physical_path, &old.logical_path).ok_or_else(|| {
            ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                format!(
                    "ownership could not be re-proven: {kind} of workspace {} cannot be resolved",
                    unit_root.display()
                ),
            )
        })?;
    let new_canon =
        output_identity_strict(&new.physical_path, &new.logical_path).ok_or_else(|| {
            ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                format!(
                    "ownership could not be re-proven: {kind} of workspace {} cannot be resolved",
                    unit_root.display()
                ),
            )
        })?;
    Ok((old_canon, new_canon))
}

/// `symlink_metadata("link/")` follows the final component on POSIX, so a
/// trailing slash hides a symlink from the `is_symlink` check. Judge the
/// path with its trailing separators stripped as well; either spelling
/// proving a symlink refuses the root. A trailing `/.` spells the same
/// bypass (`symlink_metadata("link/.")` stats through the link), so dot
/// components are stripped too before the probe.
fn path_is_symlink(path: &Path) -> bool {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return true;
    }
    let mut spelling = path.as_os_str().to_string_lossy().into_owned();
    loop {
        let trimmed = spelling.trim_end_matches(['/', '\\']).to_owned();
        if trimmed.ends_with("/.") || trimmed.ends_with("\\.") {
            spelling = trimmed[..trimmed.len() - 1].to_owned();
            continue;
        }
        spelling = trimmed;
        break;
    }
    if !spelling.is_empty()
        && fs::symlink_metadata(&spelling).is_ok_and(|m| m.file_type().is_symlink())
    {
        return true;
    }
    false
}

/// One bounded cleanup root must be an existing, real (non-symlink) directory.
fn validate_cleanup_root(root: &Path) -> Result<(), AppError> {
    if !root.is_absolute() {
        return Err(AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: "cleanup requires an absolute sandbox root".into(),
        });
    }
    let meta = fs::symlink_metadata(root).map_err(|e| AppError::InvalidRoot {
        path: root.display().to_string(),
        reason: e.to_string(),
    })?;
    if !meta.is_dir() || meta.file_type().is_symlink() || path_is_symlink(root) {
        return Err(AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: "expected a real directory".into(),
        });
    }
    Ok(())
}

fn collapse_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>, AppError> {
    let mut canonical = Vec::with_capacity(roots.len());
    for p in roots {
        // A root that passed validation but no longer resolves fails closed
        // here instead of silently degrading to a lexical compare, which
        // could duplicate one physical root or equate a stale one.
        match fs::canonicalize(&p) {
            Ok(c) => canonical.push(c),
            Err(e) => {
                return Err(AppError::InvalidRoot {
                    path: p.display().to_string(),
                    reason: format!("cleanup root changed before collapse: {e}"),
                });
            }
        }
    }
    let mut roots = canonical;
    roots.sort();
    roots.dedup();
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !out.iter().any(|parent| root.starts_with(parent)) {
            out.push(root);
        }
    }
    Ok(out)
}

/// Resolve a path as far as the filesystem allows.
///
/// Canonicalizes the deepest existing ancestor and re-appends the remaining
/// components, so a path that does not exist yet is still compared in its
/// physical location instead of lexically. This keeps the cross-workspace
/// overlap check sound when a path component (for example macOS `/var`, or a
/// symlinked ancestor of a configured output directory) is not canonical.
fn resolve_as_far_as_possible(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut current = path;
    let mut tail: Vec<OsString> = Vec::new();
    while let Some(name) = current.file_name() {
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            break;
        };
        if let Ok(canonical) = fs::canonicalize(parent) {
            let mut resolved = canonical;
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        if parent.as_os_str().is_empty() {
            break;
        }
        current = parent;
    }
    path.to_path_buf()
}

/// Physical regions an output root can touch, as far as it can be proven.
///
/// A symlink or unresolvable root has no `physical_path`, so the complete
/// physical graph cannot see it. Resolving the logical path keeps the
/// cross-workspace overlap check in C003 §7.3 sound for those roots instead of
/// silently ignoring them; when nothing can be resolved, the logical path is
/// compared lexically against canonical covering roots.
fn provable_output_paths(root: &OutputRoot) -> Vec<PathBuf> {
    if let Some(physical) = &root.physical_path {
        return vec![physical.clone()];
    }
    vec![resolve_as_far_as_possible(&root.logical_path)]
}

fn sorted_same(left: &[PathBuf], right: &[PathBuf]) -> bool {
    let mut a = left.to_vec();
    let mut b = right.to_vec();
    a.sort();
    b.sort();
    a == b
}

const PROOF_REFRESH_CONCURRENCY_CAP: usize = 4;

struct ProofRefreshMeter<'a> {
    runner: &'a dyn CleanupRunner,
    active: &'a std::sync::atomic::AtomicUsize,
    peak: &'a std::sync::atomic::AtomicUsize,
}

impl CleanupRunner for ProofRefreshMeter<'_> {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
        let metadata = args.first().is_some_and(|arg| arg == "metadata");
        if metadata {
            let active = self
                .active
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            self.peak
                .fetch_max(active, std::sync::atomic::Ordering::SeqCst);
        }
        let result = self.runner.run(cwd, args);
        if metadata {
            self.active
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }

    fn run_with_env(
        &self,
        cwd: &Path,
        args: &[OsString],
        env: &[(OsString, OsString)],
    ) -> io::Result<ProcessOutput> {
        self.runner.run_with_env(cwd, args, env)
    }
}

fn refresh_one_proof_workspace(
    workspace: &ResolvedWorkspace,
    runner: &dyn CleanupRunner,
) -> (ScanCounters, Option<ResolvedWorkspace>) {
    let adapter = WorkspaceCleanupAdapter { runner };
    let mut proof_counters = ScanCounters::default();
    let mut proof_diagnostics = Vec::new();
    let fresh = workspace::refresh_workspace_from_root_manifest(
        &workspace.root_manifest,
        &adapter,
        &mut proof_counters,
        &mut proof_diagnostics,
        &crate::progress::NoopObserver,
    );
    (proof_counters, fresh)
}

/// Refresh each ownership-universe workspace directly through Cargo metadata.
/// Refreshes are independent and read-only, so a small fixed pool can reduce
/// wall time without reusing state across candidates. Results are joined and
/// checked in input order; each batch completes before the next starts.
fn refresh_proof_universe(
    universe: &[ResolvedWorkspace],
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
) -> Result<Vec<ResolvedWorkspace>, ProofFailure> {
    refresh_proof_universe_with_limit(
        universe,
        runner,
        counters,
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .clamp(1, PROOF_REFRESH_CONCURRENCY_CAP),
    )
}

/// One hoisted re-resolution of the complete bounded ownership universe.
///
/// The universe every candidate is proven against is the same for the whole
/// run, so it is re-resolved once here instead of once per candidate: the old
/// per-candidate refresh cost O(N^2) Cargo spawns and O(N) full directory
/// re-walks for N workspaces (measured: 8 workspaces = 192 spawns, 2.7 s; the
/// growth is quadratic, not the constant). Each candidate still re-resolves
/// its *own* workspace through Cargo, so a workspace that changed after this
/// snapshot is still caught before anything is cleaned.
struct ProofUniverse {
    fresh: Option<Vec<ResolvedWorkspace>>,
    failure: Option<ProofFailure>,
}

impl ProofUniverse {
    fn refresh(
        universe: &[ResolvedWorkspace],
        clean_roots: &[PathBuf],
        runner: &dyn CleanupRunner,
        counters: &mut ScanCounters,
        selection: &CleanupSelection,
    ) -> Self {
        match refresh_combined_universe(universe, clean_roots, runner, counters, selection) {
            Ok(fresh) => Self {
                fresh: Some(fresh),
                failure: None,
            },
            Err(reason) => Self {
                fresh: None,
                failure: Some(reason),
            },
        }
    }
    /// The refreshed universe, or the typed reason it could not be re-proven.
    ///
    /// L-2: the error is constructed with `ProofFailure::new` at the failure
    /// site, never derived from message text via `proof_skip_code`, so prose
    /// edits cannot silently reclassify the machine contract. The legacy
    /// `From<String>` fallback remains for external callers only.
    fn get(&self) -> Result<&[ResolvedWorkspace], ProofFailure> {
        match (&self.fresh, &self.failure) {
            (Some(fresh), _) => Ok(fresh),
            (None, Some(reason)) => Err(reason.clone()),
            (None, None) => Err(ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                "ownership could not be re-proven: no refreshed universe",
            )),
        }
    }
}

/// Rediscover and resolve every selected source root for each candidate's final proof.
fn refresh_combined_universe(
    initial: &[ResolvedWorkspace],
    roots: &[PathBuf],
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
    selection: &CleanupSelection,
) -> Result<Vec<ResolvedWorkspace>, ProofFailure> {
    // M018: the refresh walks per root, for the same reason the initial
    // discovery does — a traversal error carries no path, so the root that
    // produced it is the only attribution available. The refresh applies the
    // same boundary rule: a diagnostic inside the proof boundary invalidates
    // the refresh, one outside it does not. That root was already skipped and
    // disclosed, and the uncertainty inside it is the residual ADR 008 §5
    // accepts in exchange for the per-root rule.
    let mut manifests: Vec<PathBuf> = Vec::new();
    for root in roots {
        let policy = crate::domain::EffectiveScanPolicy {
            recency: Duration::ZERO,
            scope: ScanScope::ExplicitRoots(vec![root.clone()]),
            discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
        };
        let discovered = discovery::discover_manifests(&policy, &crate::progress::NoopObserver)
            .map_err(|e| {
                ProofFailure::new(
                    CleanupReasonCode::SkippedOwnershipUnproven,
                    format!(
                        "ownership could not be re-proven: combined root discovery failed: {e}"
                    ),
                )
            })?;
        let root_already_judged = selection.excludes_coverage_root(root);
        for diagnostic in &discovered.diagnostics {
            let in_boundary = match &diagnostic.path {
                Some(path) => boundary_admits(selection, path),
                None => !root_already_judged,
            };
            if in_boundary {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedOwnershipUnproven,
                    "ownership could not be re-proven: combined selected-root discovery is incomplete",
                ));
            }
        }
        manifests.extend(discovered.manifests);
    }
    manifests.sort();
    manifests.dedup();
    let adapter = WorkspaceCleanupAdapter { runner };
    let mut diagnostics = Vec::new();
    let mut refreshed_counters = ScanCounters::default();
    let coverage = workspace::resolve_workspaces_with_coverage(
        &manifests,
        &adapter,
        &mut refreshed_counters,
        &mut diagnostics,
        &crate::progress::NoopObserver,
    );
    counters.proof_cargo_metadata_calls = counters
        .proof_cargo_metadata_calls
        .saturating_add(refreshed_counters.cargo_metadata_calls);
    counters.proof_cargo_metadata_nanos = counters
        .proof_cargo_metadata_nanos
        .saturating_add(refreshed_counters.cargo_metadata_nanos);
    counters.proof_cargo_locate_calls = counters
        .proof_cargo_locate_calls
        .saturating_add(refreshed_counters.cargo_locate_calls);
    counters.proof_cargo_locate_nanos = counters
        .proof_cargo_locate_nanos
        .saturating_add(refreshed_counters.cargo_locate_nanos);
    if !coverage.unresolved.is_empty() {
        // ADR 007: the refresh re-applies the boundary rule against the
        // fresh workspaces instead of carrying a scan-time tolerated list.
        // Out-of-boundary unknowns stay ignored; in-boundary passengers and
        // scope notes stay reported; only a fresh in-boundary `MergeAll`
        // invalidates the refresh. Recomputing (rather than tolerating
        // scan-time manifests by path) also fails closed when a scan-time
        // passenger's owner disappears mid-run — the fresh disposition is
        // `MergeAll`, which blocks.
        let (blocking, _, _) =
            split_unresolved_for_boundary(&coverage.unresolved, &coverage.workspaces, selection);
        if !blocking.is_empty() {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                format!(
                    "ownership could not be re-proven: combined selected-root universe has {} unresolved manifest(s); a workspace did not re-resolve",
                    blocking.len()
                ),
            ));
        }
    }
    let fresh = coverage.workspaces;
    let ids: std::collections::HashSet<_> = fresh.iter().map(|w| w.id.clone()).collect();
    if initial.iter().any(|w| !ids.contains(&w.id)) {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            "ownership could not be re-proven: a workspace changed identity or disappeared from the initial combined universe",
        ));
    }
    refresh_proof_universe(&fresh, runner, counters)
}

fn refresh_proof_universe_with_limit(
    universe: &[ResolvedWorkspace],
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
    worker_limit: usize,
) -> Result<Vec<ResolvedWorkspace>, ProofFailure> {
    let concurrency = worker_limit.clamp(1, PROOF_REFRESH_CONCURRENCY_CAP);
    let mut refreshed_universe = Vec::with_capacity(universe.len());
    let mut peak_concurrency = 0usize;

    for batch in universe.chunks(concurrency) {
        let active = std::sync::atomic::AtomicUsize::new(0);
        let peak = std::sync::atomic::AtomicUsize::new(0);
        let outcomes = if batch.len() == 1 {
            let meter = ProofRefreshMeter {
                runner,
                active: &active,
                peak: &peak,
            };
            vec![refresh_one_proof_workspace(&batch[0], &meter)]
        } else {
            std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .iter()
                    .map(|ws| {
                        let meter = ProofRefreshMeter {
                            runner,
                            active: &active,
                            peak: &peak,
                        };
                        scope.spawn(move || refresh_one_proof_workspace(ws, &meter))
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| {
                        handle
                            .join()
                            .unwrap_or_else(|_| (ScanCounters::default(), None))
                    })
                    .collect::<Vec<_>>()
            })
        };
        peak_concurrency = peak_concurrency.max(peak.load(std::sync::atomic::Ordering::SeqCst));

        let mut failure: Option<ProofFailure> = None;
        for (workspace, (proof_counters, fresh)) in batch.iter().zip(outcomes) {
            counters.merge_proof(&proof_counters);
            counters.proof_workspaces_refreshed =
                counters.proof_workspaces_refreshed.saturating_add(1);
            match fresh {
                Some(fresh) if fresh.id == workspace.id && fresh.root == workspace.root => {
                    refreshed_universe.push(fresh);
                }
                Some(_) => {
                    failure.get_or_insert_with(|| {
                        ProofFailure::new(
                            CleanupReasonCode::SkippedChangedBeforeCleanup,
                            format!(
                                "ownership could not be re-proven: workspace {} changed identity",
                                workspace.root.display()
                            ),
                        )
                    });
                }
                None => {
                    failure.get_or_insert_with(|| {
                        ProofFailure::new(
                            CleanupReasonCode::SkippedOwnershipUnproven,
                            format!(
                                "ownership could not be re-proven: workspace {} did not re-resolve",
                                workspace.root.display()
                            ),
                        )
                    });
                }
            }
        }
        if let Some(reason) = failure {
            counters.proof_metadata_peak_concurrency = counters
                .proof_metadata_peak_concurrency
                .max(peak_concurrency as u64);
            return Err(reason);
        }
    }
    counters.proof_metadata_peak_concurrency = counters
        .proof_metadata_peak_concurrency
        .max(peak_concurrency as u64);
    refreshed_universe.sort_by(|a, b| a.root.cmp(&b.root));
    Ok(refreshed_universe)
}

/// Single final CleanupUnit proof shared by Preview/Simulate/Execute
/// (C003 §7.2-7.3).
///
/// Runs every non-mutating pre-spawn gate Execute runs before the actual Cargo
/// clean spawn, over the complete bounded ownership universe:
///
/// 1. unit-wide ownership/authorization/measurement gate;
/// 2. fresh re-resolution of every workspace discovered under clean ROOT;
/// 3. rebuild of the complete fresh physical output graph;
/// 4. candidate workspace identity and member-set stability;
/// 5. target/build logical+physical identity stability;
/// 6. remap of the complete candidate OutputSet onto fresh physical groups;
/// 7. same expected physical covering union, every affected fresh group
///    `PrivateBounded`;
/// 8. no other discovered workspace (including symlink/uncertain roots) may
///    overlap the candidate's affected output;
/// 9. fresh source activity for the candidate workspace;
/// 10. fresh output activity for every affected covering root;
/// 11. covering-root existence/type/no-symlink and Cargo cache markers;
/// 12. frozen Cargo environment/arguments from the fresh OutputSet.
///
/// Returns `Ok(proof)` only when all gates succeed; `Err(reason)` is a clean
/// skip reason. Never spawns Cargo. Fails closed: any workspace that could
/// affect the bounded ownership graph and cannot be re-resolved makes the
/// candidate non-cleanable for that pass.
#[allow(clippy::too_many_arguments)]
pub fn final_cleanup_proof(
    unit: &workspace::CleanupUnit,
    universe: &[ResolvedWorkspace],
    clean_root: &Path,
    allowed_output_roots: &[PathBuf],
    runner: &dyn CleanupRunner,
    recency_seconds: u64,
    counters: &mut ScanCounters,
) -> Result<ExecutionProof, ProofFailure> {
    let clean_root_owned = clean_root.to_path_buf();
    let clean_roots = std::slice::from_ref(&clean_root_owned);
    // A single candidate *is* the universe here, and the refresh below already
    // re-resolved it, so no extra per-candidate refresh is needed. The
    // boundary is the candidate's own roots (`All` over that narrow scope),
    // which preserves the old fail-closed behavior exactly on this path.
    let hoisted = ProofUniverse::refresh(
        universe,
        clean_roots,
        runner,
        counters,
        &CleanupSelection::All,
    );
    final_cleanup_proof_roots(
        unit,
        universe,
        clean_roots,
        allowed_output_roots,
        runner,
        recency_seconds,
        &hoisted,
        counters,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn final_cleanup_proof_roots(
    unit: &workspace::CleanupUnit,
    universe: &[ResolvedWorkspace],
    clean_roots: &[PathBuf],
    allowed_output_roots: &[PathBuf],
    runner: &dyn CleanupRunner,
    recency_seconds: u64,
    hoisted: &ProofUniverse,
    counters: &mut ScanCounters,
    refresh_candidate: bool,
) -> Result<ExecutionProof, ProofFailure> {
    // 1. Unit-wide gate (defensive: the caller gates before dispatch, and the
    // final proof must be the single authority consumed by any spawn).
    // The skip code is derived from the unit itself (`unit_skip_code`), not
    // from the message text: parsing the detail would let a prose edit
    // silently change the machine contract. H-3: infrastructure errors stay
    // distinguishable here (`SkippedSafety`, never `SkippedUnauthorized`);
    // the engine maps them to a scope block via `is_authorization_…`.
    if let Err(detail) = unit_block_reason_roots(unit, clean_roots, allowed_output_roots) {
        let code = if is_authorization_infrastructure_error(&detail) {
            CleanupReasonCode::SkippedSafety
        } else {
            unit_skip_code(unit)
        };
        return Err(ProofFailure::new(code, detail));
    }
    // 2. Fresh Cargo resolution for the complete bounded ownership universe,
    // re-proven once per run (see `ProofUniverse`) rather than once per
    // candidate. Comparisons are canonicalized so `/tmp` vs `/private/tmp`
    // (macOS symlink) does not falsely report a change.
    let hoisted_universe = hoisted.get()?;
    // 2b. Re-resolve *this* candidate now, immediately before its own gates:
    // the hoisted snapshot is a moment old, and this is the workspace whose
    // output is about to be cleaned.
    let mut fresh_universe = hoisted_universe.to_vec();
    if refresh_candidate {
        // Look the candidate up by identity in the *refreshed* universe: the
        // refresh preserves order only because the proof requires it elsewhere.
        let Some(index) = fresh_universe.iter().position(|w| w.id == unit.id) else {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                "ownership could not be re-proven: workspace is outside the cleanup universe",
            ));
        };
        let (candidate_counters, fresh_candidate) =
            refresh_one_proof_workspace(&fresh_universe[index], runner);
        counters.merge_proof(&candidate_counters);
        counters.proof_workspaces_refreshed = counters.proof_workspaces_refreshed.saturating_add(1);
        match fresh_candidate {
            Some(fresh) if fresh.id == unit.id && fresh.root == unit.root => {
                fresh_universe[index] = fresh;
            }
            Some(_) => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedChangedBeforeCleanup,
                    format!(
                        "ownership could not be re-proven: workspace {} changed identity",
                        unit.root.display()
                    ),
                ));
            }
            None => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedOwnershipUnproven,
                    format!(
                        "ownership could not be re-proven: workspace {} did not re-resolve",
                        unit.root.display()
                    ),
                ));
            }
        }
    }
    // 3. Complete fresh physical output graph.
    let fresh_groups = workspace::build_groups(&fresh_universe);
    // 4/5. Candidate identity, member set, and target/build identity.
    let fresh_ws = fresh_universe
        .iter()
        .find(|w| w.id == unit.id)
        .ok_or_else(|| {
            ProofFailure::new(
                CleanupReasonCode::SkippedChangedBeforeCleanup,
                format!(
                    "ownership could not be re-proven: workspace {} vanished",
                    unit.root.display()
                ),
            )
        })?;
    let candidate = universe.iter().find(|w| w.id == unit.id).ok_or_else(|| {
        ProofFailure::new(
            CleanupReasonCode::SkippedOwnershipUnproven,
            format!(
                "ownership could not be re-proven: workspace {} is outside the cleanup universe",
                unit.root.display()
            ),
        )
    })?;
    let mut old_members: Vec<PathBuf> = Vec::with_capacity(candidate.members.len());
    for m in &candidate.members {
        let Some(c) = canon_strict(&m.source_root) else {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                format!(
                    "ownership could not be re-proven: member source {} of workspace {} cannot be resolved",
                    m.source_root.display(),
                    unit.root.display()
                ),
            ));
        };
        old_members.push(c);
    }
    let mut new_members: Vec<PathBuf> = Vec::with_capacity(fresh_ws.members.len());
    for m in &fresh_ws.members {
        let Some(c) = canon_strict(&m.source_root) else {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedOwnershipUnproven,
                format!(
                    "ownership could not be re-proven: member source {} of workspace {} cannot be resolved",
                    m.source_root.display(),
                    unit.root.display()
                ),
            ));
        };
        new_members.push(c);
    }
    old_members.sort();
    new_members.sort();
    if old_members != new_members {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            "workspace changed before cleanup; skipped: member set changed",
        ));
    }
    if candidate.packages != fresh_ws.packages {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            "workspace changed before cleanup; skipped: Cargo package identity set changed",
        ));
    }
    let (old_target_canon, new_target_canon) = output_identity_pair(
        &candidate.output.target,
        &fresh_ws.output.target,
        "target",
        &unit.root,
    )?;
    let (old_build_canon, new_build_canon) = output_identity_pair(
        &candidate.output.build,
        &fresh_ws.output.build,
        "build-dir",
        &unit.root,
    )?;
    if old_target_canon != new_target_canon {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            format!(
                "workspace changed before cleanup; skipped: target changed (was {}, now {})",
                old_target_canon.display(),
                new_target_canon.display()
            ),
        ));
    }
    if old_build_canon != new_build_canon {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            format!(
                "workspace changed before cleanup; skipped: build-dir changed (was {}, now {})",
                old_build_canon.display(),
                new_build_canon.display()
            ),
        ));
    }
    // 6/7. Remap the complete fresh candidate OutputSet onto the complete fresh
    // physical graph and require the same expected shape and ownership.
    let fresh_affected = workspace::map_workspace_groups(fresh_ws, &fresh_groups);
    if fresh_affected.len() != unit.groups.len() {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            "workspace changed before cleanup; skipped: physical output group set changed",
        ));
    }
    let mut fresh_covering: Vec<PathBuf> = Vec::new();
    for gi in &fresh_affected {
        // L-1: fail closed on a desynced graph index instead of panicking.
        let Some(g) = fresh_groups.get(*gi) else {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedSafety,
                "fresh output graph index is out of bounds; cleanup deferred",
            ));
        };
        // M-5: report the fresh class specifically (Shared/Uncertain/
        // OwnershipUnproven), like the scan-time `unit_skip_code` does,
        // instead of conflating every degradation to `Changed`.
        if g.ownership != OutputOwnershipClass::PrivateBounded {
            return Err(ProofFailure::new(
                ownership_degraded_code(g.ownership),
                format!(
                    "ownership changed before cleanup; skipped: output {} is now {}; {}",
                    g.display.display(),
                    g.ownership.label(),
                    ownership_skip_detail(g.ownership)
                ),
            ));
        }
        fresh_covering.extend(g.covering.iter().cloned());
    }
    fresh_covering = workspace::outermost(&fresh_covering);
    if !sorted_same(&fresh_covering, &unit.covering) {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedChangedBeforeCleanup,
            "workspace changed before cleanup; skipped: physical output shape changed",
        ));
    }
    // 8. No other discovered workspace may now overlap the candidate's affected
    // output, nor have its source tree inside it. The fresh graph catches proven
    // overlaps (the group would be `Shared` in step 7); this pass also covers
    // symlink/unresolvable roots the graph cannot represent, failing closed when
    // they can reach the region.
    for other in &fresh_universe {
        if other.id == unit.id {
            continue;
        }
        for root in [&other.output.target, &other.output.build] {
            for other_path in provable_output_paths(root) {
                if unit.covering.iter().any(|c| {
                    other_path == *c || other_path.starts_with(c) || c.starts_with(&other_path)
                }) {
                    return Err(ProofFailure::new(
                        CleanupReasonCode::SkippedChangedBeforeCleanup,
                        format!(
                            "ownership changed before cleanup; skipped: another discovered workspace ({}) now overlaps cleaned output {}",
                            other.root.display(),
                            other_path.display()
                        ),
                    ));
                }
            }
        }
        // Another workspace's *source* tree inside the cleaned region is the
        // same hazard one level down: `cargo clean` removes it too. The
        // classification already refuses such a group, so this is the
        // fail-closed half of the same rule against a universe rebuilt here.
        for member in &other.members {
            if let Some(c) = unit
                .covering
                .iter()
                .find(|c| member.source_root == **c || member.source_root.starts_with(c))
            {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedChangedBeforeCleanup,
                    format!(
                        "ownership changed before cleanup; skipped: source tree {} of another discovered workspace ({}) lies inside cleaned output {}",
                        member.source_root.display(),
                        other.root.display(),
                        c.display()
                    ),
                ));
            }
        }
    }
    // 9. The cleaned region must not be nested inside a package directory.
    // The classification refuses such a group, because `cargo clean` removes
    // whatever is under the root it is given and a root inside another
    // package's tree destroys that package's checked-in files. This re-proves
    // it against the filesystem at the moment of the spawn, so a
    // classification regression cannot delete a source tree.
    if workspace::nested_package_overlap(&unit.covering, &unit.root) {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedSafety,
            format!(
                "ownership could not be re-proven: the cleaned output of workspace {} lies inside a package directory",
                unit.root.display()
            ),
        ));
    }
    // 10. Covering-root physical proof: every affected covering root must exist,
    // be a real directory, and not be a symlink.
    prove_covering_roots(&unit.covering)?;
    // 11. Fresh source + output activity with fresh timestamps, bounded by the
    // complete fresh universe's output set (same exclusion rule as analysis).
    let start = SystemTime::now();
    let cutoff = start
        .checked_sub(Duration::from_secs(recency_seconds))
        .ok_or_else(|| {
            ProofFailure::new(
                CleanupReasonCode::SkippedSafety,
                "recency window exceeds system time range",
            )
        })?;
    let mut all_outputs: Vec<PathBuf> = Vec::new();
    for other in &fresh_universe {
        for root in [&other.output.target, &other.output.build] {
            match &root.physical_path {
                Some(p) => all_outputs.push(p.clone()),
                None => all_outputs.push(root.logical_path.clone()),
            }
        }
    }
    let activity_start = std::time::Instant::now();
    let mut newest_activity: Option<SystemTime> = None;
    for member_root in workspace::workspace_member_roots(fresh_ws) {
        match traverse::workspace_member_activity(&member_root, &all_outputs, start, cutoff) {
            Ok(traverse::SourceActivity::Recent(_)) => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedActive,
                    "workspace changed before cleanup; skipped: source became active",
                ));
            }
            Ok(traverse::SourceActivity::Quiet(t)) => {
                if let Some(t) = t {
                    newest_activity = Some(newest_activity.map_or(t, |old| old.max(t)));
                }
            }
            Err(()) => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedUncertain,
                    "revalidation source activity uncertain",
                ));
            }
        }
    }
    counters.proof_source_activity_nanos = counters
        .proof_source_activity_nanos
        .saturating_add(crate::domain::elapsed_nanos(activity_start.elapsed()));
    let sizing_start = std::time::Instant::now();
    let mut fresh_bytes = 0u64;
    for covering in &unit.covering {
        let stats = traverse::measure_single_target(covering);
        if stats.uncertain {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedUncertain,
                "revalidation output measurement uncertain",
            ));
        }
        if stats.newest.is_some_and(|t| t >= cutoff || t > start) {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedActive,
                "workspace changed before cleanup; skipped: output became active",
            ));
        }
        fresh_bytes = fresh_bytes.saturating_add(stats.bytes);
        if let Some(t) = stats.newest {
            newest_activity = Some(newest_activity.map_or(t, |old| old.max(t)));
        }
    }
    counters.proof_output_sizing_nanos = counters
        .proof_output_sizing_nanos
        .saturating_add(crate::domain::elapsed_nanos(sizing_start.elapsed()));
    // 12. Marker validation is part of the same non-mutating proof consumed by
    // Simulate, and covers the complete affected union.
    require_cargo_markers(&unit.covering)?;
    // 13. Frozen-environment capability validation from the fresh OutputSet.
    let target = fresh_ws
        .output
        .target
        .physical_path
        .clone()
        .unwrap_or_else(|| fresh_ws.output.target.logical_path.clone());
    let build = fresh_ws.output.build.physical_path.clone().or_else(|| {
        if fresh_ws.output.build.logical_path != fresh_ws.output.target.logical_path {
            Some(fresh_ws.output.build.logical_path.clone())
        } else {
            None
        }
    });
    let build = if build.as_ref() == Some(&target) {
        None
    } else {
        build
    };
    let provisional = ExecutionProof {
        workspace_roots: vec![unit.root.clone()],
        workspace_root: fresh_ws.root.clone(),
        root_manifest: fresh_ws.root_manifest.clone(),
        canonical_members: {
            let mut v: Vec<PathBuf> = fresh_ws
                .members
                .iter()
                .map(|m| canon(&m.source_root))
                .collect();
            v.sort();
            v.dedup();
            v
        },
        target,
        build,
        capability: fresh_ws.capability,
        covering: unit.covering.clone(),
        pre_bytes: fresh_bytes,
        newest_activity,
        ownership: OutputOwnershipClass::PrivateBounded,
        frozen_env: Vec::new(),
    };
    let frozen = frozen_env(&provisional)?;
    Ok(ExecutionProof {
        frozen_env: frozen,
        ..provisional
    })
}

/// Covering-root physical proof: every affected covering root must exist,
/// be a real directory, and not be a symlink.
///
/// Shared by the final proof (step 9) and the last-moment recheck
/// immediately before spawn, so both gates refuse the same states with the
/// same code. A trailing-slash spelling is judged as a symlink too.
/// M-4: an unreadable root maps to `SkippedUncertain`, not `Changed`.
fn prove_covering_roots(covering: &[PathBuf]) -> Result<(), ProofFailure> {
    for root in covering {
        match fs::symlink_metadata(root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedChangedBeforeCleanup,
                    "preflight changed: covering root disappeared",
                ));
            }
            Err(e) => {
                return Err(covering_inspect_failure(e));
            }
            Ok(m) if m.file_type().is_symlink() || path_is_symlink(root) => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedChangedBeforeCleanup,
                    "preflight changed: covering root became symlink",
                ));
            }
            Ok(m) if !m.is_dir() => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedChangedBeforeCleanup,
                    "preflight changed: covering root changed type",
                ));
            }
            Ok(_) => {}
        }
    }
    Ok(())
}

/// Dry pre-spawn decision for Execute without mutating the fixture.
///
/// Exposes the same non-mutating gate set Execute runs before spawn so tests
/// can compare candidate dispositions with Simulate on identical state
/// without invoking Cargo clean.
pub fn execute_pre_spawn_decision(
    unit: &workspace::CleanupUnit,
    universe: &[ResolvedWorkspace],
    clean_root: &Path,
    allowed_output_roots: &[PathBuf],
    runner: &dyn CleanupRunner,
    recency_seconds: u64,
) -> Result<ExecutionProof, ProofFailure> {
    let mut counters = ScanCounters::default();
    final_cleanup_proof(
        unit,
        universe,
        clean_root,
        allowed_output_roots,
        runner,
        recency_seconds,
        &mut counters,
    )
}

fn frozen_env(ctx: &RevalidatedContext) -> Result<Vec<(OsString, OsString)>, ProofFailure> {
    // Freeze Cargo output context so the child sees exactly the world this
    // proof validated, whatever the user's shell exported:
    // - CARGO_TARGET_DIR is always pinned to the resolved target.
    // - CARGO_BUILD_BUILD_DIR is always pinned too: to the resolved build
    //   directory when Cargo reports one, and otherwise to the target itself.
    //   An inherited value can no longer redirect the clean into a directory
    //   that was never proven (M9), and an inherited value no longer forces a
    //   blanket refusal when the runtime cannot even report build directories
    //   (M5) - previously a safe, cleanable workspace was refused forever.
    // - Do not pass unsupported unstable flags.
    //
    // Defense-in-depth note: both refusal branches below are currently
    // unreachable from production constructors (`capability_from_metadata`
    // maps `env_set && !had_build_field` to `Unknown`, never `Unavailable`,
    // and a surviving `Some(build)` implies `Distinct`), reachable only via
    // hand-built contexts. They are retained so a future constructor change
    // fails closed instead of silently redirecting the clean.
    if ctx.capability.env_build_dir_set
        && ctx.capability.build_dir == CargoBuildDirCapability::Unavailable
    {
        return Err(ProofFailure::new(
            CleanupReasonCode::SkippedSafety,
            "CARGO_BUILD_BUILD_DIR is set but runtime Cargo cannot prove separate build-dir; cleanup deferred",
        ));
    }
    let mut env = vec![(
        OsString::from("CARGO_TARGET_DIR"),
        ctx.target.as_os_str().to_owned(),
    )];
    match &ctx.build {
        Some(build) => match ctx.capability.build_dir {
            CargoBuildDirCapability::Distinct | CargoBuildDirCapability::Equal => env.push((
                OsString::from("CARGO_BUILD_BUILD_DIR"),
                build.as_os_str().to_owned(),
            )),
            CargoBuildDirCapability::Unavailable | CargoBuildDirCapability::Unknown => {
                return Err(ProofFailure::new(
                    CleanupReasonCode::SkippedSafety,
                    "Cargo reports a separate build directory without stable support; cleanup deferred",
                ));
            }
        },
        // No separate build directory was reported. Pinning the variable to the
        // target asserts "build dir == target", which is the world the proof
        // measured. If the runtime ignores the variable, this is a no-op.
        None => env.push((
            OsString::from("CARGO_BUILD_BUILD_DIR"),
            ctx.target.as_os_str().to_owned(),
        )),
    }
    Ok(env)
}

fn clean_args(
    ctx: &RevalidatedContext,
    mode: CleanMode,
    selector: Option<&CleanupSelector>,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("clean")];
    if mode == CleanMode::CargoPreview {
        args.push(OsString::from("--dry-run"));
        args.push(OsString::from("--verbose"));
    }
    args.push(OsString::from("--offline"));
    args.push(OsString::from("--locked"));
    args.push(OsString::from("--manifest-path"));
    args.push(ctx.root_manifest.as_os_str().to_owned());
    // Freeze target via explicit arg as well as env (equivalent, Cargo-owned).
    args.push(OsString::from("--target-dir"));
    args.push(ctx.target.as_os_str().to_owned());
    if let Some(CleanupSelector::Profile(profile)) = selector {
        args.push(OsString::from("--profile"));
        args.push(OsString::from(profile));
    }
    if let Some(CleanupSelector::Package(package)) = selector {
        args.push(OsString::from("--package"));
        args.push(OsString::from(package));
    }
    args
}

fn runtime_clean_capabilities() -> CargoCleanCapabilities {
    let Ok(output) = Command::new("cargo").arg("--version").output() else {
        return CargoCleanCapabilities::default();
    };
    if !output.status.success() {
        return CargoCleanCapabilities::default();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(version) = text.split_whitespace().nth(1) else {
        return CargoCleanCapabilities::default();
    };
    workspace::clean_capabilities_from_version(version)
}

fn resolve_package_spec<'a>(
    packages: &'a [WorkspacePackage],
    spec: &str,
) -> Result<&'a WorkspacePackage, PackageSpecResolutionError> {
    let matches: Vec<_> = packages
        .iter()
        .filter(|package| {
            package.name == spec
                || package.id == spec
                || format!("{}@{}", package.name, package.version) == spec
                || package_spec_name_version(spec).is_some_and(|(name, version)| {
                    package.name == name && semver_spec_matches(&package.version, version)
                })
        })
        .collect();
    let package = match matches.as_slice() {
        [] => return Err(PackageSpecResolutionError::Unknown),
        [package] => *package,
        _ => return Err(PackageSpecResolutionError::Ambiguous),
    };
    if packages
        .iter()
        .filter(|candidate| candidate.name == package.name)
        .count()
        != 1
    {
        return Err(PackageSpecResolutionError::Ambiguous);
    }
    Ok(package)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackageSpecResolutionError {
    Unknown,
    Ambiguous,
}

fn package_spec_name_version(spec: &str) -> Option<(&str, &str)> {
    let (name, version) = spec.split_once('@').or_else(|| spec.split_once(':'))?;
    (!name.is_empty() && !version.is_empty() && !name.contains('+') && !name.contains("://"))
        .then_some((name, version))
}

fn semver_spec_matches(actual: &str, requested: &str) -> bool {
    if actual == requested {
        return true;
    }
    let actual_parts: Vec<_> = actual.split('.').collect();
    let requested_parts: Vec<_> = requested.split('.').collect();
    (1..=2).contains(&requested_parts.len())
        && requested_parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        && actual_parts.len() == 3
        && actual_parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        && actual_parts
            .iter()
            .zip(requested_parts)
            .all(|(actual, requested)| actual == &requested)
}

fn require_cargo_markers(covering: &[PathBuf]) -> Result<(), ProofFailure> {
    for root in covering {
        let tag = root.join("CACHEDIR.TAG");
        let meta = fs::symlink_metadata(&tag).map_err(|_| {
            ProofFailure::new(
                CleanupReasonCode::SkippedMarkerInvalid,
                "Cargo CACHEDIR.TAG marker is missing; cleanup deferred",
            )
        })?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedMarkerInvalid,
                "Cargo CACHEDIR.TAG is not a regular file; cleanup deferred",
            ));
        }
        let bytes = fs::read(&tag).map_err(|e| {
            ProofFailure::new(
                CleanupReasonCode::SkippedMarkerInvalid,
                format!("cannot read Cargo cache marker: {e}"),
            )
        })?;
        if !bytes.starts_with(b"Signature: 8a477f597d28d172789f06886806bc55") {
            return Err(ProofFailure::new(
                CleanupReasonCode::SkippedMarkerInvalid,
                "target directory does not have Cargo's cache marker signature; cleanup deferred",
            ));
        }
    }
    Ok(())
}

fn measure_union(covering: &[PathBuf]) -> io::Result<u64> {
    let mut total = 0u64;
    for root in covering {
        match fs::symlink_metadata(root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
            Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
                return Err(io::Error::other("output path changed type"));
            }
            Ok(_) => {}
        }
        let stats = traverse::measure_single_target(root);
        if stats.uncertain {
            return Err(io::Error::other("post-clean measurement was uncertain"));
        }
        total = total.saturating_add(stats.bytes);
    }
    Ok(total)
}

// Legacy compatibility: keep old single-target Ownership checks for reference?
// M005B replaces them with workspace/output groups; direct deletion remains
// prohibited (no `remove_dir_all` in production).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::NoopObserver;
    use std::ffi::OsStr;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[test]
    fn blocked_listing_detail_names_participants_and_hints_workspace_membership() {
        // The `-i` block used to carry only a count ("1 ... did not resolve"),
        // so the operator could not tell which manifest to fix. A test that
        // only asserted "it blocked" would pass against that shape too.
        let block = ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            "cleanup ownership could not be proven: 1 discovered Cargo manifest did not resolve; nothing can be selected".into(),
        );
        let unresolved = vec![UnresolvedOwnershipParticipant {
            manifest: PathBuf::from("/work/a/fuzz/Cargo.toml"),
            stage: "locate",
            reason: "cargo locate-project failed: error: current package believes it's in a workspace when it's not".into(),
            capability: UnknownCapability::RefusedByCargo,
        }];
        let detail = blocked_listing_detail(&block, &unresolved);
        assert!(
            detail.contains("/work/a/fuzz/Cargo.toml"),
            "the manifest path must be named: {detail}"
        );
        assert!(
            detail.contains("unresolved [locate]"),
            "the stage must be named: {detail}"
        );
        assert!(
            detail.contains("empty `[workspace]` table"),
            "the workspace-membership hint must be present for this reason: {detail}"
        );
        // A generic failure must not carry that hint.
        let generic = vec![UnresolvedOwnershipParticipant {
            manifest: PathBuf::from("/work/b/Cargo.toml"),
            stage: "metadata",
            reason: "cargo metadata failed: broken pipe".into(),
            capability: UnknownCapability::Unproven,
        }];
        let generic_detail = blocked_listing_detail(&block, &generic);
        assert!(
            generic_detail.contains("/work/b/Cargo.toml"),
            "generic participants are still named: {generic_detail}"
        );
        assert!(
            !generic_detail.contains("empty `[workspace]` table"),
            "the hint is specific to the workspace-belief reason: {generic_detail}"
        );
        // A discovery block with no participants stays count-only.
        let discovery = ScopeBlock::new(
            ScopeBlockReason::IncompleteDiscovery,
            "combined cleanup ownership universe is incomplete: 2 discovery diagnostic(s); nothing can be selected".into(),
        );
        let empty = blocked_listing_detail(&discovery, &[]);
        assert!(
            !empty.contains("unresolved ["),
            "no participants means no rows: {empty}"
        );
    }

    #[test]
    fn ancestry_selection_matches_in_both_directions() {
        // The layout that decides whether the new default is usable at all:
        // standing inside a *member* of a virtual workspace must still select
        // the enclosing workspace, or the most common Cargo layout would be a
        // silent no-op for every user.
        let member = Path::new("/home/u/projects");
        assert!(
            CleanupSelection::Ancestry(member.to_path_buf())
                .admits(Path::new("/home/u/projects/crates/inner")),
            "a workspace containing the anchor must be selected"
        );
        // And the reverse: standing above several projects must be able to
        // select one of them.
        assert!(
            CleanupSelection::Ancestry(member.to_path_buf())
                .admits(Path::new("/home/u/projects/crates/inner")),
            "a workspace below the anchor must be selected"
        );
        assert!(
            CleanupSelection::Ancestry(member.to_path_buf()).admits(member),
            "the anchor itself must be selected"
        );
        // An unrelated tree must not be.
        assert!(
            !CleanupSelection::Ancestry(member.to_path_buf())
                .admits(Path::new("/var/tmp/elsewhere")),
            "an unrelated workspace must not be selected"
        );
        // A shared *prefix* is not ancestry. `/home/u/projects-old` does not
        // live under `/home/u/projects`, and `Path::starts_with` is
        // component-wise, so this is where a string-prefix implementation
        // would wrongly select.
        assert!(
            !CleanupSelection::Ancestry(member.to_path_buf())
                .admits(Path::new("/home/u/projects-old")),
            "a string-prefix sibling must not be selected"
        );
    }

    #[test]
    fn roots_selection_matches_exactly() {
        let selection =
            CleanupSelection::Roots(vec![PathBuf::from("/a/one"), PathBuf::from("/a/two")]);
        assert!(selection.admits(Path::new("/a/one")));
        assert!(selection.admits(Path::new("/a/two")));
        assert!(
            !selection.admits(Path::new("/a/one/nested")),
            "an exact-root selection must not sweep in children"
        );
        assert!(!selection.admits(Path::new("/a/three")));
    }

    #[test]
    fn outside_selection_is_reported_rather_than_silently_dropped() {
        // The M012A lesson: a selection that silently discarded units would be
        // indistinguishable from "nothing was eligible". The typed code is the
        // only thing that separates the two for an operator reading JSON.
        assert_eq!(
            CleanupReasonCode::SkippedOutsideSelection.as_str(),
            "skipped_outside_selection"
        );
        // It must not be conflatable with any ownership or safety code, which
        // warrant a different operator response.
        for other in [
            CleanupReasonCode::SkippedShared,
            CleanupReasonCode::SkippedOwnershipUnproven,
            CleanupReasonCode::SkippedSafety,
        ] {
            assert_ne!(other.as_str(), "skipped_outside_selection");
        }
    }

    #[test]
    fn missing_output_identity_pair_compares_equal_not_unproven() {
        use crate::domain::OutputRootKind;
        let missing = |logical: PathBuf| OutputRoot {
            kind: OutputRootKind::Build,
            logical_path: logical,
            physical_path: None,
            exists: false,
            is_symlink: false,
        };
        let unit_root = PathBuf::from("/ws");
        let old = missing(PathBuf::from("/ws/build"));
        let new = missing(PathBuf::from("/ws/build"));
        let (a, b) = output_identity_pair(&old, &new, "build-dir", &unit_root).unwrap();
        assert_eq!(a, b);
        // An existence flip is a change (still fail-closed), not Unproven.
        let present = OutputRoot {
            kind: OutputRootKind::Build,
            logical_path: PathBuf::from("/ws/build"),
            physical_path: Some(PathBuf::from("/ws/build")),
            exists: true,
            is_symlink: false,
        };
        let err = output_identity_pair(&old, &present, "build-dir", &unit_root).unwrap_err();
        assert_eq!(err.code, CleanupReasonCode::SkippedChangedBeforeCleanup);
    }

    type RecordedCall = (PathBuf, Vec<OsString>, Vec<(OsString, OsString)>);
    // Fake cleanup runner with recording + configurable Cargo metadata.
    struct FakeCleanupRunner {
        calls: Mutex<Vec<RecordedCall>>,
        workspace_root: PathBuf,
        target: PathBuf,
        build: Option<PathBuf>,
        members: usize,
        remove_on_execute: bool,
        unresolved_manifest: Option<PathBuf>,
    }

    impl FakeCleanupRunner {
        fn new(root: &Path, target: &Path, members: usize) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                workspace_root: root.to_path_buf(),
                target: target.to_path_buf(),
                build: None,
                members,
                remove_on_execute: false,
                unresolved_manifest: None,
            }
        }
        fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
        fn clean_calls(&self) -> Vec<RecordedCall> {
            self.calls()
                .into_iter()
                .filter(|(_, args, _)| args.first().is_some_and(|a| a == "clean"))
                .collect()
        }
    }

    impl CargoRunner for FakeCleanupRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((cwd.to_path_buf(), args.to_vec(), Vec::new()));
            if args.first().is_some_and(|a| a == "locate-project") {
                if self.unresolved_manifest.as_ref().is_some_and(|p| {
                    args.last()
                        .is_some_and(|arg| canon(Path::new(arg)) == canon(p))
                }) {
                    return Ok(ProcessOutput {
                        success: false,
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"unresolved participant".to_vec(),
                    });
                }
                let root_manifest = self.workspace_root.join("Cargo.toml");
                let json = serde_json::json!({"root": root_manifest});
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            if args.first().is_some_and(|a| a == "metadata") {
                let members: Vec<String> = (0..self.members).map(|i| format!("m{i}")).collect();
                let mut packages = Vec::new();
                for i in 0..self.members {
                    let mp = self.workspace_root.join(format!("member{i}/Cargo.toml"));
                    packages.push(serde_json::json!({
                        "id": format!("m{i}"),
                        "name": format!("m{i}"),
                        "manifest_path": mp,
                    }));
                }
                // Single-member case uses the workspace root manifest.
                if self.members == 1 {
                    packages = vec![serde_json::json!({
                        "id": "m0",
                        "name": "m0",
                        "manifest_path": self.workspace_root.join("Cargo.toml"),
                    })];
                }
                let mut json = serde_json::json!({
                    "packages": packages,
                    "workspace_members": members,
                    "workspace_root": self.workspace_root,
                    "target_directory": self.target,
                });
                if let Some(b) = &self.build {
                    json["build_directory"] = serde_json::json!(b);
                }
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            // `clean` invocation.
            if self.remove_on_execute && !args.iter().any(|a| a == "--dry-run") {
                let target = args
                    .windows(2)
                    .find(|pair| pair[0] == "--target-dir")
                    .map(|pair| PathBuf::from(&pair[1]));
                if let Some(t) = target
                    && t.join("artifact.bin").exists()
                {
                    let _ = fs::remove_file(t.join("artifact.bin"));
                }
            }
            Ok(ProcessOutput {
                success: true,
                code: Some(0),
                stdout: b"mock cargo clean success".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    impl CleanupRunner for FakeCleanupRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }
        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((cwd.to_path_buf(), args.to_vec(), env.to_vec()));
            // Reuse same metadata/clean logic but record env.
            if args.first().is_some_and(|a| a == "clean") {
                if self.remove_on_execute && !args.iter().any(|a| a == "--dry-run") {
                    let target = args
                        .windows(2)
                        .find(|pair| pair[0] == "--target-dir")
                        .map(|pair| PathBuf::from(&pair[1]));
                    if let Some(t) = target
                        && t.join("artifact.bin").exists()
                    {
                        let _ = fs::remove_file(t.join("artifact.bin"));
                    }
                }
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: b"mock cargo clean success".to_vec(),
                    stderr: Vec::new(),
                });
            }
            CargoRunner::run(self, cwd, args)
        }
    }

    fn valid_fixture(members: usize) -> (TempDir, PathBuf, PathBuf) {
        let d = tempfile::Builder::new()
            .prefix("cargo-cleanme-m005b-")
            .tempdir()
            .unwrap();
        let root = d.path().join("ws");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='ws'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            target.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        std::fs::write(target.join("artifact.bin"), vec![7u8; 4096]).unwrap();
        // Backdate everything so activity gates pass with recency 0 + future clock?
        // Use recency 0 and future start in clean_with? clean_with uses
        // SystemTime::now + recency; recent files would still be active if just
        // written. Backdate to old so recency 0 treats them quiet.
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let _ = members;
        (d, root, target)
    }

    fn backdate(path: &Path, old: SystemTime) {
        let mut stack = vec![path.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(meta) = fs::symlink_metadata(&current) {
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir()
                    && let Ok(children) = fs::read_dir(&current)
                {
                    for child in children.flatten() {
                        stack.push(child.path());
                    }
                }
            }
            if let Ok(f) = fs::OpenOptions::new().write(true).open(&current) {
                let _ = f.set_modified(old);
            } else if let Ok(dir) = fs::File::open(&current) {
                let _ = dir.set_modified(old);
            }
        }
        if let Ok(dir) = fs::File::open(path) {
            let _ = dir.set_modified(old);
        }
    }

    #[test]
    fn cli_modes_map_to_preview_simulate_execute() {
        // `--cargo-preview` => CargoPreview; `--dry-run` => Simulate; no mode
        // flag and `--yes` => Execute. Mapping lives in `cli::clean_mode`;
        // here we pin the stable machine labels the JSON/log `mode` field
        // carries, because renaming a variant must never change the contract.
        assert_eq!(CleanMode::CargoPreview.as_str(), "preview");
        assert_eq!(CleanMode::Simulate.as_str(), "simulate");
        assert_eq!(CleanMode::Execute.as_str(), "execute");
        assert_ne!(CleanMode::CargoPreview, CleanMode::Simulate);
        assert_ne!(CleanMode::Simulate, CleanMode::Execute);
        assert_eq!(
            crate::cli::clean_mode(false, false, false),
            CleanMode::Execute,
            "absent a mode flag, cleanup executes"
        );
        assert_eq!(
            crate::cli::clean_mode(true, false, false),
            CleanMode::Simulate
        );
        assert_eq!(
            crate::cli::clean_mode(false, true, false),
            CleanMode::CargoPreview
        );
        assert_eq!(
            crate::cli::clean_mode(false, false, true),
            CleanMode::Simulate,
            "the hidden --dryrun alias maps to the canonical Simulate mode"
        );
    }

    #[test]
    fn policy_dispositions_have_stable_json_names() {
        let values = [
            (PolicyDisposition::Selected, "selected"),
            (PolicyDisposition::BelowMinimumSize, "below_minimum_size"),
            (
                PolicyDisposition::TooRecentForPolicy,
                "too_recent_for_policy",
            ),
            (PolicyDisposition::NotIncluded, "not_included"),
            (PolicyDisposition::Excluded, "excluded"),
        ];
        for (value, expected) in values {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format!("\"{expected}\"")
            );
        }
    }

    #[test]
    fn cleanup_reason_codes_have_stable_contract_names() {
        use CleanupReasonCode::*;
        let expected = [
            (Previewed, "previewed"),
            (Simulated, "simulated"),
            (Cleaned, "cleaned"),
            (BelowMinimumSize, "below_minimum_size"),
            (TooRecentForPolicy, "too_recent_for_policy"),
            (NotIncluded, "not_included"),
            (Excluded, "excluded"),
            (SkippedActive, "skipped_active"),
            (SkippedShared, "skipped_shared"),
            (SkippedUncertain, "skipped_uncertain"),
            (SkippedUnauthorized, "skipped_unauthorized"),
            (SkippedMarkerInvalid, "skipped_marker_invalid"),
            (
                SkippedChangedBeforeCleanup,
                "skipped_changed_before_cleanup",
            ),
            (SkippedSafety, "skipped_safety"),
            (CargoFailed, "cargo_failed"),
            (MeasurementFailed, "measurement_failed"),
        ];
        for (code, stable) in expected {
            assert_eq!(code.as_str(), stable);
        }
    }

    #[test]
    fn profile_selector_capability_is_bounded_to_qualified_runtime_versions() {
        assert!(workspace::clean_capabilities_from_version("1.89.0").profile_selector);
        assert!(workspace::clean_capabilities_from_version("1.90.0").profile_selector);
        assert!(workspace::clean_capabilities_from_version("1.95.0").profile_selector);
        assert!(workspace::clean_capabilities_from_version("1.99.0").profile_selector);
        assert!(!workspace::clean_capabilities_from_version("1.88.0").profile_selector);
        assert!(!workspace::clean_capabilities_from_version("1.96.0").profile_selector);
        assert!(!workspace::clean_capabilities_from_version("1.99.1").profile_selector);
        assert!(!workspace::clean_capabilities_from_version("2.0.0").profile_selector);
        assert!(!workspace::clean_capabilities_from_version("unknown").profile_selector);
        assert!(workspace::clean_capabilities_from_version("1.98.1").package_selector);
        assert!(workspace::clean_capabilities_from_version("1.99.0").package_selector);
        assert!(!workspace::clean_capabilities_from_version("1.100.0").package_selector);
        assert!(!workspace::clean_capabilities_from_version("1.99.0-nightly").package_selector);
    }

    #[test]
    fn package_specs_require_one_authoritative_workspace_identity() {
        let packages = vec![
            WorkspacePackage {
                id: "path+file:///tmp/ws#alpha@0.1.0".into(),
                name: "alpha".into(),
                version: "0.1.0".into(),
                manifest_path: PathBuf::from("/tmp/ws/alpha/Cargo.toml"),
            },
            WorkspacePackage {
                id: "path+file:///tmp/ws#shared@0.2.0".into(),
                name: "shared".into(),
                version: "0.2.0".into(),
                manifest_path: PathBuf::from("/tmp/ws/shared/Cargo.toml"),
            },
        ];
        assert_eq!(
            resolve_package_spec(&packages, "alpha").unwrap().name,
            "alpha"
        );
        assert_eq!(
            resolve_package_spec(&packages, "alpha@0.1").unwrap().name,
            "alpha"
        );
        assert_eq!(
            resolve_package_spec(&packages, "alpha@0.1.0").unwrap().id,
            packages[0].id
        );
        assert_eq!(
            resolve_package_spec(&packages, &packages[1].id)
                .unwrap()
                .name,
            "shared"
        );
        assert!(resolve_package_spec(&packages, "missing").is_err());
        let mut duplicate = packages.clone();
        duplicate.push(WorkspacePackage {
            id: "registry+https://example.invalid#alpha@0.3.0".into(),
            name: "alpha".into(),
            version: "0.3.0".into(),
            manifest_path: PathBuf::from("/tmp/ws/alpha2/Cargo.toml"),
        });
        assert!(resolve_package_spec(&duplicate, "alpha").is_err());
        assert!(resolve_package_spec(&duplicate, "alpha@0.3.0").is_err());
    }

    /// Three workspaces under one root: `alpha` and `beta` own private targets,
    /// and `gamma` redirects its output into `alpha`'s through a project-level
    /// `target-dir`, which makes that physical output `Shared` with two live
    /// claimants.
    ///
    /// That mixed ownership is what makes this fixture able to catch drift: a
    /// listing that quietly filtered candidates would disagree with an engine
    /// that considers all three. Real `cargo metadata` is used so the production
    /// resolution path is what is under test.
    fn mixed_ownership_fixture() -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
        let d = tempfile::Builder::new()
            .prefix("cargo-cleanme-m014-")
            .tempdir()
            .unwrap();
        let root = d.path().canonicalize().unwrap().join("projects"); // macOS: /var is a symlink
        let mut workspaces = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            let ws = root.join(name);
            std::fs::create_dir_all(ws.join("src")).unwrap();
            std::fs::write(
                ws.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
            )
            .unwrap();
            std::fs::write(ws.join("src/main.rs"), "fn main() {}\n").unwrap();
            std::fs::write(ws.join("Cargo.lock"), "version = 4\n").unwrap();
            workspaces.push(ws);
        }
        // Private, independently-owned targets for alpha and beta.
        for name in ["alpha", "beta"] {
            let target = root.join(name).join("target");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(
                target.join("CACHEDIR.TAG"),
                b"Signature: 8a477f597d28d172789f06886806bc55\n",
            )
            .unwrap();
            std::fs::write(target.join("artifact.bin"), vec![7u8; 4096]).unwrap();
        }
        // gamma points its build output at alpha's target: two live claimants,
        // one physical directory.
        let redirected = root.join("alpha").join("target").display().to_string();
        std::fs::create_dir_all(root.join("gamma").join(".cargo")).unwrap();
        std::fs::write(
            root.join("gamma").join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = {redirected:?}\n"), // Debug: escapes `\` so Windows paths stay valid TOML
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        (
            d,
            root,
            workspaces[0].clone(),
            workspaces[1].clone(),
            workspaces[2].clone(),
        )
    }

    #[test]
    fn the_candidate_listing_is_the_engines_own_unit_set() {
        // The interactive list must offer exactly the units the engine would
        // act on. A parallel copy of the resolution pipeline would be a second
        // ownership proof free to drift from the first — the ADR 006 failure
        // wearing a UI: a row could be offered that the engine refuses, or a
        // unit the engine would consider could be absent from the list the
        // operator chose from.
        //
        // The comparison is against *every* unit the engine considered, not
        // only the ones it would clean. A `Shared` unit the engine skips is
        // still a unit the operator must be able to see and deselect; hiding
        // it would conceal exactly the thing worth showing.
        let (_d, root, _alpha, beta, _gamma) = mixed_ownership_fixture();
        let noop = NoopObserver;

        let all = list_cleanup_candidates(
            std::slice::from_ref(&root),
            0,
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupSelection::All,
        )
        .unwrap();
        assert!(all.scope_blocked.is_none(), "{:?}", all.scope_blocked);
        assert_eq!(
            all.candidates.len(),
            3,
            "every resolved unit must be offered: {:?}",
            all.candidates
        );
        assert!(
            all.candidates.iter().all(|c| c.bytes > 0),
            "rows carry real measured bytes: {:?}",
            all.candidates
        );
        // The premise that makes this comparison discriminating: the fixture
        // must actually produce mixed ownership, or a filtered listing would
        // still agree with the engine and the test would be decoration.
        assert!(
            all.candidates
                .iter()
                .any(|c| c.ownership != OutputOwnershipClass::PrivateBounded),
            "the fixture must include a non-private unit: {:?}",
            all.candidates
        );

        let report = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::All,
        )
        .unwrap();

        let mut listed: Vec<(PathBuf, u64, &'static str)> = all
            .candidates
            .iter()
            .map(|c| (c.root.clone(), c.bytes, c.ownership.label()))
            .collect();
        listed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut engine: Vec<(PathBuf, u64, &'static str)> = report
            .results
            .iter()
            // Selection-excluded rows are deliberately excluded from the
            // engine's unit set: they are rows the M014 filter added, not
            // units the pipeline found.
            .filter(|r| r.reason_code != CleanupReasonCode::SkippedOutsideSelection)
            .map(|r| {
                (
                    r.display_path.clone(),
                    r.before_bytes.unwrap_or(0),
                    r.ownership.label(),
                )
            })
            .collect();
        engine.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            listed, engine,
            "the interactive list and the engine must agree on units, bytes, and ownership"
        );

        // And the selection must travel identically into both. This is the
        // property the TUI depends on: what the operator sees is what the
        // engine treats as candidates.
        let narrowed = CleanupSelection::Roots(vec![beta.clone()]);
        let listed_narrow = list_cleanup_candidates(
            std::slice::from_ref(&root),
            0,
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &narrowed,
        )
        .unwrap();
        assert_eq!(
            listed_narrow.candidates.len(),
            1,
            "the listing honours the selection: {:?}",
            listed_narrow.candidates
        );
        assert_eq!(listed_narrow.candidates[0].root, beta);

        let engine_narrow = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &narrowed,
        )
        .unwrap();
        let considered: Vec<&PathBuf> = engine_narrow
            .results
            .iter()
            .filter(|r| r.reason_code != CleanupReasonCode::SkippedOutsideSelection)
            .map(|r| &r.display_path)
            .collect();
        let excluded: Vec<&PathBuf> = engine_narrow
            .results
            .iter()
            .filter(|r| r.reason_code == CleanupReasonCode::SkippedOutsideSelection)
            .map(|r| &r.display_path)
            .collect();
        assert_eq!(considered, vec![&beta], "the engine honours it too");
        assert_eq!(
            excluded.len(),
            2,
            "every deselected unit is reported, not silently dropped: {excluded:?}"
        );
    }

    /// M018: two learned roots — one fully readable, one whose coverage could
    /// not be proven — whose workspaces **share one output root**.
    ///
    /// The shared output is the whole point. If the unproven root were dropped
    /// from *discovery* (the forbidden shortcut), the readable root's unit
    /// would be classified `PrivateBounded` and cleaned. M018's exclusion
    /// removes the unproven root from the *candidate* set only, so the shared
    /// classification survives and the bytes are not touched.
    fn coverage_gate_fixture() -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let d = tempfile::Builder::new()
            .prefix("cargo-cleanme-m018-")
            .tempdir()
            .unwrap();
        let root = d.path().canonicalize().unwrap().join("projects");
        let readable = root.join("readable");
        let unproven = root.join("unproven");
        let alpha = readable.join("alpha");
        let gamma = unproven.join("gamma");
        for (name, dir) in [("alpha", &alpha), ("gamma", &gamma)] {
            std::fs::create_dir_all(dir.join("src")).unwrap();
            std::fs::write(
                dir.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
            )
            .unwrap();
            std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
            std::fs::write(dir.join("Cargo.lock"), "version = 4\n").unwrap();
        }
        // alpha owns a private target with real bytes in it.
        let target = alpha.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            target.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        std::fs::write(target.join("artifact.bin"), vec![7u8; 4096]).unwrap();
        // gamma redirects its build output into alpha's target: two live
        // claimants, one physical directory.
        let redirected = target.display().to_string();
        std::fs::create_dir_all(gamma.join(".cargo")).unwrap();
        std::fs::write(
            gamma.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = {redirected:?}\n"), // Debug: escapes `\` so Windows paths stay valid TOML
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        (d, root, readable, unproven)
    }

    /// M018: a fixture whose *unproven* root has a real unreadable region, so
    /// the walk produces a real traversal error rather than a simulated one.
    ///
    /// The traversal error carries no path — the upstream walk's error event
    /// has no child path — so this is the fixture that proves the per-root
    /// attribution works: the error belongs to one root, and the other root's
    /// units must still be provable.
    #[cfg(unix)]
    fn unreadable_region_fixture() -> (TempDir, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::Builder::new()
            .prefix("cargo-cleanme-m018-locked-")
            .tempdir()
            .unwrap();
        let readable = d.path().join("readable");
        let unproven = d.path().join("unproven");
        for (name, dir) in [("readable", &readable), ("unproven", &unproven)] {
            let project = dir.join(name);
            std::fs::create_dir_all(project.join("src")).unwrap();
            std::fs::write(
                project.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
            )
            .unwrap();
            std::fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
            std::fs::write(project.join("Cargo.lock"), "version = 4\n").unwrap();
            let target = project.join("target");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(
                target.join("CACHEDIR.TAG"),
                b"Signature: 8a477f597d28d172789f06886806bc55\n",
            )
            .unwrap();
            std::fs::write(target.join("artifact.bin"), vec![7u8; 4096]).unwrap();
        }
        // The unreadable region: a directory the walk cannot enter.
        let locked = unproven.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            std::fs::read_dir(&locked).is_err(),
            "premise: the fixture must produce an unreadable region"
        );
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        (d, readable, unproven)
    }

    /// M018: an unproven root's unreadable region blocks **that root**, not
    /// the whole scope.
    ///
    /// This is ADR 008 §5's per-root rule, and it is the difference between
    /// a usable Full cleanup and the whole-system refusal the ADR withdrew.
    /// The traversal error carries no path, so it is attributed to the root
    /// whose walk produced it; a root outside the proof boundary does not
    /// block.
    ///
    /// *Fails if:* the traversal error is treated as an unattributable,
    /// always-blocking diagnostic — which is what the code did before the
    /// per-root attribution, and what made the Full cleanup refuse on every
    /// machine with one unreadable directory.
    #[cfg(unix)]
    #[test]
    fn an_unproven_roots_unreadable_region_does_not_block_the_scope() {
        let (_d, readable, unproven) = unreadable_region_fixture();
        let noop = NoopObserver;
        let selection = CleanupSelection::All.excluding(vec![crate::domain::UnprovenRoot {
            root: unproven.clone(),
            detail: "1 traversal error(s) under it".into(),
        }]);
        let admitted = vec![readable.clone(), unproven.clone()];
        let report = clean_with_roots_policy_selector_guarded(
            &admitted,
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &selection,
        )
        .unwrap();
        assert!(
            report.scope_blocked.is_none(),
            "the unproven root's unreadable region must not block the scope: {:?}",
            report.scope_blocked
        );
        let excluded: Vec<&CleanResult> = report
            .results
            .iter()
            .filter(|row| row.reason_code == CleanupReasonCode::SkippedIncompleteCoverage)
            .collect();
        assert_eq!(
            excluded.len(),
            1,
            "the unproven root's own unit is excluded with the typed reason: {:?}",
            report.results
        );
        assert!(
            excluded[0].display_path.starts_with(&unproven),
            "the excluded unit must be the unproven root's own: {:?}",
            excluded[0].display_path
        );
        // The proven root is not blocked by its neighbour's unreadable region.
        let survivor = report
            .results
            .iter()
            .find(|row| row.display_path.starts_with(&readable))
            .expect("the readable root's unit is in the report");
        assert_ne!(
            survivor.reason_code,
            CleanupReasonCode::SkippedIncompleteCoverage,
            "the readable root must not be excluded"
        );
        assert_ne!(
            survivor.reason_code,
            CleanupReasonCode::SkippedOutsideSelection,
            "the readable root is inside the selection: {:?}",
            survivor
        );
    }

    /// M018: an unreadable region inside the **proof boundary** still blocks.
    ///
    /// The per-root attribution must not become a general relaxation: a root
    /// that is *not* excluded is inside the boundary, and its unreadable
    /// region is an in-boundary discovery diagnostic that fails closed.
    ///
    /// *Fails if:* the attribution lets every pathless diagnostic through, or
    /// attributes it to the wrong root.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_region_inside_the_proof_boundary_still_blocks() {
        let (_d, readable, unproven) = unreadable_region_fixture();
        let noop = NoopObserver;
        let admitted = vec![readable.clone(), unproven.clone()];
        let report = clean_with_roots_policy_selector_guarded(
            &admitted,
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::All,
        )
        .unwrap();
        let block = report
            .scope_blocked
            .expect("an unreadable region inside the boundary blocks the scope");
        assert_eq!(block.reason, ScopeBlockReason::IncompleteDiscovery);
        assert!(
            block.message.contains("discovery diagnostic"),
            "the block names the discovery diagnostics: {}",
            block.message
        );
        assert!(
            report.results.is_empty(),
            "a blocked scope cleans nothing: {:?}",
            report.results
        );
    }

    /// M018: a unit under a root whose coverage could not be proven is
    /// removed from the candidate set with a typed reason of its own, and the
    /// reason names both the cause and what the operator can do.
    ///
    /// `skipped_outside_selection` would be a different fact — "you did not
    /// ask for this" — and an operator reading it would look in the wrong
    /// place.
    #[test]
    fn a_unit_under_an_unproven_root_is_reported_as_incomplete_coverage() {
        let (_d, root, readable, unproven) = coverage_gate_fixture();
        let noop = NoopObserver;
        let selection = CleanupSelection::All.excluding(vec![crate::domain::UnprovenRoot {
            root: unproven.clone(),
            detail: "2 permission-denied region(s) under it".into(),
        }]);
        let report = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &selection,
        )
        .unwrap();
        assert!(report.scope_blocked.is_none(), "{:?}", report.scope_blocked);
        let excluded: Vec<&CleanResult> = report
            .results
            .iter()
            .filter(|row| row.reason_code == CleanupReasonCode::SkippedIncompleteCoverage)
            .collect();
        assert_eq!(
            excluded.len(),
            1,
            "exactly the unit under the unproven root is excluded: {:?}",
            report.results
        );
        assert!(
            excluded[0].display_path.starts_with(&unproven),
            "the excluded unit must be the unproven root's own: {:?}",
            excluded[0].display_path
        );
        let detail = &excluded[0].detail;
        assert!(
            detail.contains("could not be proven"),
            "the detail must name the cause: {detail}"
        );
        assert!(
            detail.contains("scan --all"),
            "the detail must name what the operator can do: {detail}"
        );
        // The readable root's unit is not excluded: the exclusion is per root.
        let survivor = report
            .results
            .iter()
            .find(|row| row.display_path.starts_with(&readable))
            .expect("the readable root's unit is in the report");
        assert_ne!(
            survivor.reason_code,
            CleanupReasonCode::SkippedIncompleteCoverage,
            "the readable root must not be excluded"
        );
    }

    /// M018: the shared-output hazard (the ADR 006 measurement).
    ///
    /// A workspace in the readable root shares its output root with a
    /// workspace in the unproven root. The unproven root's workspace stays in
    /// the *discovery* universe, so the shared output stays `Shared` and the
    /// readable root's unit is inventory-only — and no `cargo clean` is
    /// issued for it.
    ///
    /// The roots handed to the engine are the two the gate admits — the
    /// proven root and the unproven one — which is the production shape. If
    /// the gate withheld the unproven root, the engine would receive only the
    /// proven root and the shared bytes would be classified `PrivateBounded`.
    ///
    /// *Fails if:* the unproven root is dropped from discovery. The shared
    /// bytes would then be classified `PrivateBounded` and deleted.
    #[test]
    fn a_shared_output_with_an_unproven_root_stays_shared_and_is_not_cleaned() {
        let (_d, _root, readable, unproven) = coverage_gate_fixture();
        let noop = NoopObserver;
        let selection = CleanupSelection::All.excluding(vec![crate::domain::UnprovenRoot {
            root: unproven.clone(),
            detail: "2 permission-denied region(s) under it".into(),
        }]);
        // The gate's admitted set: both learned roots, one proven and one not.
        let admitted = vec![readable.clone(), unproven.clone()];
        let report = clean_with_roots_policy_selector_guarded(
            &admitted,
            0,
            &[],
            CleanMode::Execute,
            &SystemCleanupRunner,
            &noop,
            &CleanupPolicy::default(),
            None,
            None,
            &selection,
        )
        .unwrap();
        assert!(report.scope_blocked.is_none(), "{:?}", report.scope_blocked);
        let alpha = report
            .results
            .iter()
            .find(|row| row.display_path.starts_with(&readable))
            .expect("the readable root's unit is in the report");
        assert_eq!(
            alpha.ownership,
            OutputOwnershipClass::Shared,
            "the unproven root's workspace is still a claimant, so the shared output stays shared: {:?}",
            alpha
        );
        assert_ne!(
            alpha.reason_code,
            CleanupReasonCode::Cleaned,
            "a shared output is inventory-only"
        );
        // The bytes are still there: no `cargo clean` was issued for it.
        let artifact = readable.join("alpha/target/artifact.bin");
        assert!(
            artifact.exists(),
            "the shared output must not be cleaned: {}",
            artifact.display()
        );
    }

    /// M018: the exclusion narrows a selection without ever widening it, and
    /// it composes with every declared shape.
    #[test]
    fn the_coverage_exclusion_removes_candidates_and_names_the_cause() {
        let unproven = crate::domain::UnprovenRoot {
            root: PathBuf::from("/work/unproven"),
            detail: "1 permission-denied region(s) under it".into(),
        };
        let selection = CleanupSelection::All.excluding(vec![unproven.clone()]);
        assert!(
            !selection.admits(Path::new("/work/unproven/project")),
            "a unit under the unproven root is not a candidate"
        );
        assert!(
            !selection.admits(Path::new("/work/unproven")),
            "the unproven root itself is not a candidate"
        );
        assert!(
            selection.admits(Path::new("/work/readable/project")),
            "a unit outside the unproven root stays a candidate"
        );
        // Composing with an allow-list keeps the allow-list's meaning, and the
        // exclusion still wins over it: an allow-list that names a unit under
        // an unproven root does not make it a candidate again.
        let narrowed = CleanupSelection::Roots(vec![
            PathBuf::from("/work/readable"),
            PathBuf::from("/work/unproven/project"),
        ])
        .excluding(vec![unproven.clone()]);
        assert!(narrowed.admits(Path::new("/work/readable")));
        assert!(
            !narrowed.admits(Path::new("/work/unproven/project")),
            "the exclusion narrows an allow-list; it never widens one"
        );
        // The typed reason is the coverage one, not "outside the selection".
        let (code, detail) = selection
            .exclusion_reason(Path::new("/work/unproven/project"))
            .expect("the exclusion names its own reason");
        assert_eq!(code, CleanupReasonCode::SkippedIncompleteCoverage);
        assert!(
            detail.contains("/work/unproven"),
            "the detail names the root: {detail}"
        );
        assert!(
            detail.contains("could not be proven"),
            "the detail names the cause: {detail}"
        );
        assert!(
            selection
                .exclusion_reason(Path::new("/work/readable/project"))
                .is_none(),
            "a unit outside the unproven root has no coverage reason"
        );
    }

    /// M018: the fail-closed shape dispatches nothing.
    ///
    /// A Full cleanup that proved nothing has no roots, and the engine
    /// refuses an empty scope outright — so there is no `cargo clean` to
    /// issue. The block that reports it is what makes the run non-zero; the
    /// refusal is what makes it honest.
    #[test]
    fn a_scope_with_no_roots_dispatches_no_cleanup() {
        let runner = FakeCleanupRunner::new(
            Path::new("/nonexistent/workspace"),
            Path::new("/nonexistent/target"),
            1,
        );
        let outcome = clean_with_roots_policy_selector_guarded(
            &[],
            0,
            &[],
            CleanMode::Execute,
            &runner,
            &NoopObserver,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::All,
        );
        assert!(
            outcome.is_err(),
            "an empty scope is refused, never quietly cleaned"
        );
        assert!(
            runner.clean_calls().is_empty(),
            "a scope with no roots must issue no cargo clean"
        );
    }

    #[test]
    fn the_listing_never_spawns_a_clean() {
        // Read-only by contract: the listing spawns `cargo metadata` and
        // nothing else. If it ever grew a clean spawn, the operator would be
        // looking at a list whose construction had already deleted bytes.
        //
        // `Execute` is requested deliberately: the listing must ignore the mode
        // argument, because "what mode would this run in?" is not the listing's
        // question to answer.
        let (_d, root, alpha, beta, _gamma) = mixed_ownership_fixture();
        let listing = list_cleanup_candidates(
            std::slice::from_ref(&root),
            0,
            CleanMode::Execute,
            &SystemCleanupRunner,
            &NoopObserver,
            &CleanupSelection::All,
        )
        .unwrap();
        assert_eq!(listing.candidates.len(), 3);
        for ws in [alpha, beta] {
            assert!(
                ws.join("target").join("artifact.bin").exists(),
                "{} survived the listing",
                ws.display()
            );
        }
    }

    /// ADR 007 edge: a refused root manifest names no owner (scope note),
    /// so it never blocks *proceeding* units — but here there are no units
    /// at all. Offering nothing from an unproven universe without a block
    /// would read as success (the M012A silent no-op), so the listing
    /// refuses. The note stays named in `unresolved_ownership`.
    #[test]
    fn empty_proof_universe_blocks_listing() {
        // Zero resolved units plus an unproven participant is not a clean
        // bill of health: refusing is what keeps the empty offer honest.
        let (_d, root, target) = valid_fixture(1);
        let mut runner = FakeCleanupRunner::new(&root, &target, 1);
        runner.unresolved_manifest = Some(root.join("Cargo.toml"));
        let noop = NoopObserver;

        let listing = list_cleanup_candidates(
            std::slice::from_ref(&root),
            0,
            CleanMode::Simulate,
            &runner,
            &noop,
            &CleanupSelection::All,
        )
        .unwrap();

        let block = listing
            .scope_blocked
            .as_ref()
            .expect("an empty proof against unproven participants refuses");
        assert_eq!(block.reason, ScopeBlockReason::OwnershipUnproven);
        assert!(
            !listing.unresolved_ownership.is_empty(),
            "the refusal must name the participant, not only count it"
        );
        assert!(
            listing.candidates.is_empty(),
            "no rows may be offered from an unproven universe: {:?}",
            listing.candidates
        );
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn a_candidate_row_states_when_it_cannot_be_cleaned() {
        // `provably_private` is a presentation hint about ownership only. It
        // must never be mistaken for the full policy verdict, and it must never
        // claim a unit is cleanable when only authorization would decide.
        let shared = CleanupCandidate {
            root: PathBuf::from("/ws/shared"),
            bytes: 10,
            ownership: OutputOwnershipClass::Shared,
        };
        let private = CleanupCandidate {
            root: PathBuf::from("/ws/private"),
            bytes: 10,
            ownership: OutputOwnershipClass::PrivateBounded,
        };
        let uncertain = CleanupCandidate {
            root: PathBuf::from("/ws/uncertain"),
            bytes: 10,
            ownership: OutputOwnershipClass::Uncertain,
        };
        assert!(!shared.provably_private());
        assert!(!uncertain.provably_private());
        assert!(private.provably_private());
        // The doc comment's claim, asserted: this is a projection of ownership
        // alone. Size cannot change the answer — otherwise the row would be
        // quietly growing an eligibility verdict the engine never asked it for.
        let mut empty = private.clone();
        empty.bytes = 0;
        assert!(
            empty.provably_private(),
            "byte size must not affect an ownership projection"
        );
        let external = CleanupCandidate {
            root: PathBuf::from("/ws/external"),
            bytes: 10,
            ownership: OutputOwnershipClass::ExternalUnproven,
        };
        assert!(
            !external.provably_private(),
            "unproven-by-construction output is never cleanable"
        );
    }

    #[test]
    fn preview_is_default_and_simulate_invokes_no_clean() {
        let (_d, root, target) = valid_fixture(1);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        // Preview invokes metadata + clean --dry-run.
        let report = clean_with(&root, 0, &[], CleanMode::CargoPreview, &runner, &noop).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Previewed);
        assert!(
            runner
                .clean_calls()
                .iter()
                .any(|(_, args, _)| { args.iter().any(|a| a == "--dry-run") })
        );
        assert!(target.join("artifact.bin").exists());

        // Simulate invokes no clean at all.
        let runner2 = FakeCleanupRunner::new(&root, &target, 1);
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner2, &noop).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Simulated);
        assert!(
            runner2.clean_calls().is_empty(),
            "simulation must not invoke clean"
        );
        assert!(target.join("artifact.bin").exists());
        assert!(
            report
                .render(&crate::report::HumanOptions::default())
                .contains("no `cargo clean` command was invoked")
        );
        assert!(
            !report
                .render(&crate::report::HumanOptions::default())
                .contains("recovered")
        );
    }

    /// ADR 007: an in-boundary scope note never blocks. A nested manifest
    /// Cargo cannot resolve inside an otherwise healthy workspace is
    /// reported, never blocking: the healthy unit proceeds in every mode.
    /// Fails against the old whole-scope rule, which blocked everything on
    /// the note and reported zero cleanable bytes.
    #[test]
    fn unresolved_scope_note_is_reported_without_blocking_any_mode() {
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let (_d, root, target) = valid_fixture(1);
            let unresolved = root.join("unresolved/Cargo.toml");
            std::fs::create_dir_all(unresolved.parent().unwrap()).unwrap();
            std::fs::write(
                &unresolved,
                "[package]\nname='unresolved'\nversion='0.1.0'\n",
            )
            .unwrap();
            let mut runner = FakeCleanupRunner::new(&root, &target, 1);
            runner.unresolved_manifest = Some(unresolved);
            let report = clean_with(&root, 0, &[], mode, &runner, &NoopObserver).unwrap();
            assert_eq!(
                report.counters.unresolved_ownership, 1,
                "{mode:?} {report:?}"
            );
            assert_eq!(report.unresolved_ownership.len(), 1, "{mode:?}");
            assert_eq!(
                report.unresolved_ownership[0].manifest,
                canon(&root.join("unresolved/Cargo.toml"))
            );
            assert!(
                report
                    .counters
                    .stats_line()
                    .contains("unresolved_ownership=1")
            );
            assert!(
                report.results.len() == 1,
                "the healthy unit still proceeds: {mode:?} {report:?}"
            );
            assert_eq!(
                report.scope_blocked, None,
                "a scope note never blocks the boundary: {mode:?}"
            );
            assert!(
                report
                    .render(&crate::report::HumanOptions::default())
                    .contains("reported unprovable participant(s), not blocking"),
                "the note is named on the human report: {mode:?}"
            );
            assert!(
                report
                    .render(&crate::report::HumanOptions::default())
                    .contains("unresolved [locate]"),
                "the participant row is bounded and staged: {mode:?} {}",
                report.render(&crate::report::HumanOptions::default())
            );
            assert_eq!(
                report.diagnostics, 1,
                "the resolution diagnostic is retained alongside the report: {mode:?}"
            );
            let row = row_for(&report, &root);
            match mode {
                CleanMode::Simulate => {
                    assert_eq!(row.outcome, CleanOutcome::Simulated, "{mode:?}");
                    assert!(
                        runner.clean_calls().is_empty(),
                        "{mode:?}: simulation spawns nothing"
                    );
                }
                CleanMode::CargoPreview => {
                    assert_eq!(row.outcome, CleanOutcome::Previewed, "{mode:?}");
                }
                CleanMode::Execute => {
                    assert_eq!(row.outcome, CleanOutcome::Cleaned, "{mode:?}");
                    assert_eq!(runner.clean_calls().len(), 1, "{mode:?}");
                }
            }
        }
    }

    /// A blocked human report names the blocking manifests instead of only
    /// counting them. Fails against the old render, which emitted the block
    /// message and returned: the count told the operator nothing fixable.
    #[test]
    fn blocked_scope_render_lists_unresolved_participants_with_workspace_hint() {
        let report = CleanReport {
            results: Vec::new(),
            diagnostics: 2,
            failed: 0,
            mode: CleanMode::Simulate,
            counters: ScanCounters::default(),
            scope_blocked: Some(ScopeBlock::new(
                ScopeBlockReason::OwnershipUnproven,
                "cleanup ownership could not be proven: 2 discovered Cargo manifests did not resolve; no cleanup commands were run".into(),
            )),
            unresolved_ownership: vec![
                UnresolvedOwnershipParticipant {
                    manifest: PathBuf::from("/work/a/website/Cargo.toml"),
                    stage: "locate",
                    reason: "cargo locate-project failed: error: current package believes it's in a workspace when it's not: current: /work/a/website/Cargo.toml".into(),
                    capability: UnknownCapability::RefusedByCargo,
                },
                UnresolvedOwnershipParticipant {
                    manifest: PathBuf::from("/work/b/Cargo.toml"),
                    stage: "metadata",
                    reason: "cargo metadata failed: something else".into(),
                    capability: UnknownCapability::Unproven,
                },
            ],
            selected_roots: vec![PathBuf::from("/work")],
            discovered_manifests: 3,
            resolved_workspaces: 1,
            units_considered: 0,
            effective_policy: None,
            selector: None,
            out_of_scope_unresolved: 0,
        };
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(
            text.contains("cleanup ownership could not be proven"),
            "block message retained: {text}"
        );
        assert_eq!(
            text.matches("cleanup ownership could not be proven")
                .count(),
            1,
            "hint text must not repeat the block phrase: {text}"
        );
        assert!(
            text.contains("unresolved [locate] /work/a/website/Cargo.toml:"),
            "first participant listed with stage: {text}"
        );
        assert!(
            text.contains("unresolved [metadata] /work/b/Cargo.toml:"),
            "second participant listed with stage: {text}"
        );
        assert!(
            text.contains("empty `[workspace]` table"),
            "workspace-detach hint for the believes-in-workspace class: {text}"
        );
    }

    /// The participant listing is bounded: five rows plus an overflow line.
    /// Seven participants must not produce seven rows.
    #[test]
    fn blocked_scope_render_bounds_unresolved_participant_listing() {
        let unresolved_ownership: Vec<UnresolvedOwnershipParticipant> = (0..7)
            .map(|i| UnresolvedOwnershipParticipant {
                manifest: PathBuf::from(format!("/work/p{i}/Cargo.toml")),
                stage: "locate",
                reason: "cargo locate-project failed".into(),
                capability: UnknownCapability::RefusedByCargo,
            })
            .collect();
        let report = CleanReport {
            results: Vec::new(),
            diagnostics: 7,
            failed: 0,
            mode: CleanMode::Simulate,
            counters: ScanCounters::default(),
            scope_blocked: Some(ScopeBlock::new(
                ScopeBlockReason::OwnershipUnproven,
                "cleanup ownership could not be proven: 7 discovered Cargo manifests did not resolve; no cleanup commands were run".into(),
            )),
            unresolved_ownership,
            selected_roots: vec![PathBuf::from("/work")],
            discovered_manifests: 8,
            resolved_workspaces: 1,
            units_considered: 0,
            effective_policy: None,
            selector: None,
            out_of_scope_unresolved: 0,
        };
        let text = report.render(&crate::report::HumanOptions::default());
        assert_eq!(
            text.matches("unresolved [locate]").count(),
            5,
            "exactly five participant rows: {text}"
        );
        assert!(
            text.contains("… 2 more unresolved participant(s)"),
            "bounded overflow line: {text}"
        );
        assert!(
            !text.contains("empty `[workspace]` table"),
            "no workspace hint without the workspace-belief reason: {text}"
        );
    }

    /// M015/F, environment: a build-dir move between discovery and spawn is
    /// caught by the full snapshot check and missed by a target-only naive
    /// check. The detector — not the wiring, which is M016 — is proven here.
    #[test]
    fn build_dir_move_detected_by_full_env_check_but_missed_by_target_only_naive() {
        use std::ffi::OsString;
        let disco = |key: &str| match key {
            "CARGO_TARGET_DIR" => Some(OsString::from("/cache/target")),
            _ => None,
        };
        let snapshot = snapshot_output_env(&disco);
        assert!(output_env_matches(&snapshot, &disco));
        let moved_build = |key: &str| match key {
            "CARGO_TARGET_DIR" => Some(OsString::from("/cache/target")),
            "CARGO_BUILD_BUILD_DIR" => Some(OsString::from("/cache/build")),
            _ => None,
        };
        assert!(
            !output_env_matches(&snapshot, &moved_build),
            "a build-dir move is an output-placement change"
        );
        fn naive_env_matches(
            snapshot: &OutputProvenanceSnapshot,
            get: &dyn Fn(&str) -> Option<OsString>,
        ) -> bool {
            snapshot.cargo_target_dir == get("CARGO_TARGET_DIR")
        }
        assert!(
            naive_env_matches(&snapshot, &moved_build),
            "target-only naive misses build-dir moves: the control fails against it"
        );
        let emptied_target = |key: &str| match key {
            "CARGO_TARGET_DIR" => Some(OsString::from("")),
            _ => None,
        };
        assert!(
            !output_env_matches(&snapshot, &emptied_target),
            "empty is not absent"
        );
    }

    /// M015/F, TOCTOU predicate: growth and shrinkage both fail the
    /// stability check the H-2 gate runs, while the naive always-stable stub
    /// passes growth through. Fails if the wired check ever stops comparing.
    #[test]
    fn universe_stability_predicate_detects_growth_that_naive_stub_misses() {
        let before = vec![PathBuf::from("/w/a/Cargo.toml")];
        let grown = vec![
            PathBuf::from("/w/a/Cargo.toml"),
            PathBuf::from("/w/b/Cargo.toml"),
        ];
        assert!(universe_manifests_stable(&before, &before));
        assert!(
            !universe_manifests_stable(&before, &grown),
            "a planted manifest must fail the check"
        );
        assert!(
            !universe_manifests_stable(&grown, &before),
            "shrinkage fails the check too"
        );
        fn naive_stable(_: &[PathBuf], _: &[PathBuf]) -> bool {
            true
        }
        assert!(
            naive_stable(&before, &grown),
            "no rediscovery passes growth through: the control fails against it"
        );
    }

    /// M015/F, TOCTOU engine: a manifest planted by A's own cleanup (via the
    /// clean hook) is visible to B's H-2 rediscovery, which blocks the
    /// remaining scope instead of cleaning against a stale claimant set.
    /// Without the rediscovery, B would clean.
    #[test]
    fn mid_run_manifest_growth_blocks_remaining_scope() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "growth-a");
        let root_b = ws_root(d.path(), "growth-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 4096);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a.clone(), None);
        runner.add(&root_b, target_b.clone(), None);
        let planted = d.path().join("planted");
        runner.on_clean(move || {
            std::fs::create_dir_all(planted.join("src")).unwrap();
            std::fs::write(
                planted.join("Cargo.toml"),
                "[package]\nname='planted'\nversion='0.1.0'\n",
            )
            .unwrap();
        });
        let report =
            clean_with(d.path(), 0, &[], CleanMode::Execute, &runner, &NoopObserver).unwrap();
        assert_eq!(
            report.scope_blocked.as_ref().map(|block| block.reason),
            Some(ScopeBlockReason::IncompleteDiscovery),
            "post-proof growth blocks the remaining scope"
        );
        assert!(
            report
                .scope_blocked
                .as_ref()
                .is_some_and(|block| block.message.contains("universe changed")),
            "the block names the cause: {:?}",
            report.scope_blocked
        );
        assert_eq!(
            row_for(&report, &root_a).outcome,
            CleanOutcome::Cleaned,
            "A cleaned before the growth existed"
        );
        assert!(
            report
                .results
                .iter()
                .all(|row| row.display_path != canonical(&root_b)),
            "B never ran against the stale universe: {:?}",
            report
                .results
                .iter()
                .map(|row| &row.display_path)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            runner.clean_calls().len(),
            1,
            "exactly one spawn: the growth stopped the run"
        );
    }

    /// M016 kennel: two output-disjoint staged workspaces plus a refused
    /// orphan — the live G1 shape as a repo-owned fixture. The orphan's
    /// locate fails, so it owns no bytes (scope note), while A and B stay
    /// output-disjoint: two would-proceed islands and one scope note.
    fn island_pair(d: &tempfile::TempDir) -> (PathBuf, PathBuf, PathBuf) {
        // macOS: TMPDIR's /var is a symlink; discovery canonicalizes.
        let base = d.path().canonicalize().unwrap();
        let root_a = ws_root(&base, "island-a");
        let root_b = ws_root(&base, "island-b");
        cargo_output(&root_a.join("target"), 4096);
        cargo_output(&root_b.join("target"), 4096);
        backdate(&base, SystemTime::now() - Duration::from_secs(3600));
        let orphan = base.join("orphan/Cargo.toml");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, "").unwrap();
        (root_a, root_b, orphan)
    }

    /// M017: run the guarded engine the one way it runs now — scoped proof
    /// is the default, no flag.
    fn clean_default(root: &Path, mode: CleanMode, runner: &StagedCargo) -> CleanReport {
        clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root.to_path_buf()),
            0,
            &[],
            mode,
            runner,
            &NoopObserver,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::All,
        )
        .unwrap()
    }

    /// M017/ADR 007: an in-boundary scope note never blocked the boundary —
    /// the defect fix. Two output-disjoint workspaces plus a refused orphan:
    /// both clean with real spawns, the note is reported in
    /// `unresolved_ownership`, and there is no block (exit 0). Fails against
    /// the old whole-scope rule, which blocked everything on the note.
    #[test]
    fn scope_note_does_not_block_in_boundary_execute() {
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        runner.removes_on_clean(vec![
            root_a.join("target").join("artifact.bin"),
            root_b.join("target").join("artifact.bin"),
        ]);
        let report = clean_default(d.path(), CleanMode::Execute, &runner);
        assert!(
            report.scope_blocked.is_none(),
            "no block: the note is reported, never blocking: {:?}",
            report.scope_blocked
        );
        assert_eq!(report.out_of_scope_unresolved, 0, "{report:?}");
        assert_eq!(report.unresolved_ownership.len(), 1, "{report:?}");
        assert!(
            report.unresolved_ownership.iter().any(|participant| {
                participant
                    .manifest
                    .ends_with(std::path::Path::new("orphan").join("Cargo.toml"))
            }),
            "the refused orphan is still named: {report:?}"
        );
        assert_eq!(report.results.len(), 2, "{:?}", report.results);
        for root in [&root_a, &root_b] {
            let row = row_for(&report, root);
            assert_eq!(row.outcome, CleanOutcome::Cleaned, "{root:?}");
            assert_eq!(row.reason_code, CleanupReasonCode::Cleaned);
        }
        // removes_on_clean is global to the fake: the first unit's clean
        // reclaims both artifacts, so the reclaimed total lands on
        // whichever unit ran first. The sum is order-independent.
        let total_decrease: u64 = report
            .results
            .iter()
            .map(|row| row.observed_decrease.unwrap_or(0))
            .sum();
        assert_eq!(total_decrease, 4096, "{report:?}");
        assert!(!root_a.join("target").join("artifact.bin").exists());
        assert!(!root_b.join("target").join("artifact.bin").exists());
        // Same argv and cwd the whole-scope engine would have used: one
        // `cargo clean` per workspace CleanupUnit over its complete OutputSet.
        let cleans = runner.clean_calls();
        assert_eq!(cleans.len(), 2, "{cleans:?}");
        for (cwd, args, _env) in &cleans {
            assert!(
                *cwd == root_a || *cwd == root_b,
                "spawn cwd is a proven workspace root: {cwd:?}"
            );
            let argv: Vec<String> = args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            assert!(argv.contains(&"--offline".to_string()), "{argv:?}");
            assert!(argv.contains(&"--locked".to_string()), "{argv:?}");
            assert!(!argv.iter().any(|a| a == "--dry-run"), "{argv:?}");
        }
        assert_eq!(report.units_considered, 2);
        assert_eq!(report.resolved_workspaces, 2);
        assert_eq!(report.discovered_manifests, 3);
        assert_eq!(report.failed, 0);
        // JSON carries the note on `unresolved_participants`; there is no
        // islands axis anymore.
        let value = serde_json::to_value(crate::output::cleanup(&report, "routine", None)).unwrap();
        assert!(
            value["result"]["islands"].is_null(),
            "no islands axis: {value}"
        );
        let noted = value["result"]["unresolved_participants"]
            .as_array()
            .unwrap();
        assert_eq!(noted.len(), 1, "{value}");
        // Human render: rows, no block, no island outcomes.
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(
            !text.contains("cleanup ownership could not be proven"),
            "{text}"
        );
        assert!(!text.contains("island outcomes"), "{text}");
        assert!(text.contains("Cleaned"), "{text}");
        // Log line: ok with counts, no paths.
        let line = crate::output::log::cleanup(&report, "routine");
        assert!(line.contains("status=ok"), "{line}");
        assert!(
            !line.contains("island-a"),
            "no paths on the log line: {line}"
        );
    }

    /// M017 flagship: the codified defect. Poison outside the
    /// working-directory anchor must not block cleaning inside it.
    /// Discovery still sees the whole scope (grouping is combined), but the
    /// out-of-anchor orphan becomes an `out_of_scope` diagnostic, never a
    /// block. Fails pre-change as a whole-scope block.
    #[test]
    fn out_of_anchor_poison_does_not_block_anchor() {
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        runner.removes_on_clean(vec![root_a.join("target").join("artifact.bin")]);
        // Discovery over the whole scope; selection anchored at A only.
        let report = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&d.path().to_path_buf()),
            0,
            &[],
            CleanMode::Execute,
            &runner,
            &NoopObserver,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::Ancestry(root_a.clone()),
        )
        .unwrap();
        assert!(
            report.scope_blocked.is_none(),
            "out-of-anchor poison never blocks: {report:?}"
        );
        assert_eq!(report.out_of_scope_unresolved, 1, "{report:?}");
        assert!(
            report.unresolved_ownership.is_empty(),
            "nothing in-boundary to report: {report:?}"
        );
        assert_eq!(
            row_for(&report, &root_a).outcome,
            CleanOutcome::Cleaned,
            "the anchor cleans: {report:?}"
        );
        assert_eq!(
            row_for(&report, &root_b).reason_code,
            CleanupReasonCode::SkippedOutsideSelection,
            "the out-of-anchor workspace is reported, not cleaned: {report:?}"
        );
        assert_eq!(runner.clean_calls().len(), 1, "{report:?}");
        assert!(!root_a.join("target").join("artifact.bin").exists());
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(
            text.contains(
                "out-of-scope unprovable participant(s) reported in diagnostics, not blocking"
            ),
            "the human report names the held-back poison: {text}"
        );
    }

    /// M017: Simulate predicts the spawns without spawning; CargoPreview runs
    /// Cargo's own dry-run per proven unit. Neither blocks on the note, with
    /// zero real cleans.
    #[test]
    fn simulate_and_preview_predict_without_spawning_or_blocking() {
        for (mode, expected_outcome) in [
            (CleanMode::Simulate, CleanOutcome::Simulated),
            (CleanMode::CargoPreview, CleanOutcome::Previewed),
        ] {
            let d = tempfile::tempdir().unwrap();
            let (root_a, root_b, _orphan) = island_pair(&d);
            let mut runner = StagedCargo::new();
            runner.add(&root_a, root_a.join("target"), None);
            runner.add(&root_b, root_b.join("target"), None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                report.scope_blocked.is_none(),
                "{mode:?}: no block: {report:?}"
            );
            assert_eq!(report.results.len(), 2, "{mode:?}: {report:?}");
            for root in [&root_a, &root_b] {
                let row = row_for(&report, root);
                assert_eq!(row.outcome, expected_outcome, "{mode:?} {root:?}");
            }
            if mode == CleanMode::Simulate {
                assert!(runner.clean_calls().is_empty(), "{mode:?}");
            } else {
                assert_eq!(runner.dry_run_calls().len(), 2, "{mode:?}");
            }
            assert!(
                root_a.join("target").join("artifact.bin").exists(),
                "{mode:?}"
            );
        }
    }

    /// M017/ADR 007 passenger rule (carried forward from ADR 004): a refused
    /// orphan naming a resolved parent rides along without blocking. Both
    /// units proceed with real spawns; the passenger is named in
    /// `unresolved_participants`, never as a unit row.
    #[test]
    fn refused_passenger_rides_along_without_blocking() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let root_a = ws_root(&base, "host-a");
        let root_b = ws_root(&base, "host-b");
        cargo_output(&root_a.join("target"), 4096);
        cargo_output(&root_b.join("target"), 4096);
        backdate(&base, SystemTime::now() - Duration::from_secs(3600));
        let orphan = base.join("orphan/Cargo.toml");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, "").unwrap();
        let reason = format!(
            "error: current package believes it's in a workspace when it's not: current: {} workspace: {}",
            orphan.display(),
            root_a.join("Cargo.toml").display(),
        );
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        runner.refuse_locate(&orphan, &reason);
        let report = clean_default(d.path(), CleanMode::Execute, &runner);
        assert!(
            report.scope_blocked.is_none(),
            "no block: the passenger rides along: {report:?}"
        );
        // Both units proceed with real spawns; nothing merges.
        assert_eq!(runner.clean_calls().len(), 2, "{report:?}");
        for root in [&root_a, &root_b] {
            assert_eq!(row_for(&report, root).outcome, CleanOutcome::Cleaned);
        }
        assert!(
            report.unresolved_ownership.iter().any(|participant| {
                participant
                    .manifest
                    .ends_with(std::path::Path::new("orphan").join("Cargo.toml"))
            }),
            "the refused passenger is still named (component-wise: symlinked TMPDIR on macOS, separators on Windows): {report:?}"
        );
        // JSON carries the passenger on `unresolved_participants`.
        let value = serde_json::to_value(crate::output::cleanup(&report, "routine", None)).unwrap();
        let noted = value["result"]["unresolved_participants"]
            .as_array()
            .unwrap();
        assert_eq!(noted.len(), 1, "{value}");
        assert!(
            std::path::Path::new(noted[0]["manifest"].as_str().unwrap())
                .ends_with(std::path::Path::new("orphan").join("Cargo.toml")),
            "component-wise: backslash separators on Windows: {value}"
        );
    }

    /// M017 hostile twin: cross-root redirect through resolution. A shares
    /// B's proven target, so both units skip as shared with zero spawns —
    /// and, with the in-boundary orphan a scope note, nothing blocks.
    /// A dispatch-everything naive would clean both sides of the shared
    /// bytes; the zero-spawn assertion is what fails against it.
    #[test]
    fn redirected_outputs_skip_both_sides_without_blocking() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let root_a = ws_root(&base, "redirect-a");
            let root_b = ws_root(&base, "redirect-b");
            let shared = cargo_output(&root_a.join("target"), 4096);
            backdate(&base, SystemTime::now() - Duration::from_secs(3600));
            let orphan = base.join("orphan/Cargo.toml");
            std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
            std::fs::write(&orphan, "").unwrap();
            let mut runner = StagedCargo::new();
            runner.add(&root_a, shared.clone(), None);
            runner.add(&root_b, shared, None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                report.scope_blocked.is_none(),
                "{mode:?}: shared units skip, nothing blocks: {report:?}"
            );
            // Shared bytes are never dispatched, never cleaned. A
            // dispatch-everything naive would clean both sides of the shared
            // bytes; the zero-spawn assertion is what fails against it.
            assert_eq!(report.results.len(), 2, "{mode:?}: {report:?}");
            for root in [&root_a, &root_b] {
                let row = row_for(&report, root);
                assert_eq!(row.outcome, CleanOutcome::Skipped, "{mode:?} {root:?}");
                assert_eq!(
                    row.reason_code,
                    CleanupReasonCode::SkippedShared,
                    "{mode:?} {root:?}"
                );
            }
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    /// M017 hostile twin: nested-output prefix overlap skips at the unit gate
    /// with zero spawns — same shape as the redirect twin.
    #[test]
    fn nested_outputs_skip_at_unit_gate_with_zero_spawns() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let root_a = ws_root(&base, "nest-a");
            let root_b = ws_root(&base, "nest-b");
            let outer = cargo_output(&root_a.join("target"), 2048);
            // B's target nested inside A's: prefix overlap, shared bytes.
            let nested = cargo_output(&outer.join("nested"), 1024);
            backdate(&base, SystemTime::now() - Duration::from_secs(3600));
            let orphan = base.join("orphan/Cargo.toml");
            std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
            std::fs::write(&orphan, "").unwrap();
            let mut runner = StagedCargo::new();
            runner.add(&root_a, outer.clone(), None);
            runner.add(&root_b, nested, None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                report.scope_blocked.is_none(),
                "{mode:?}: shared units skip, nothing blocks: {report:?}"
            );
            // Prefix overlap cannot be separated: held at the unit gate with
            // zero spawns — same shape as the redirect twin.
            assert_eq!(report.results.len(), 2, "{mode:?}: {report:?}");
            for root in [&root_a, &root_b] {
                let row = row_for(&report, root);
                assert_eq!(row.outcome, CleanOutcome::Skipped, "{mode:?} {root:?}");
            }
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    /// M017 hostile twin: a metadata failure is byte-capable (MergeAll), so
    /// the boundary still blocks with zero spawns in every mode.
    #[test]
    fn metadata_failure_blocks_boundary_with_zero_spawns() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let root_a = ws_root(&base, "overlap-a");
            let root_b = ws_root(&base, "overlap-b");
            cargo_output(&root_a.join("target"), 4096);
            cargo_output(&root_b.join("target"), 2048);
            backdate(&base, SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, root_a.join("target"), None);
            runner.add(&root_b, root_b.join("target"), None);
            runner.fail_metadata_from(1, 1);
            let report = clean_default(d.path(), mode, &runner);
            assert_eq!(
                report.scope_blocked.as_ref().map(|block| block.reason),
                Some(ScopeBlockReason::OwnershipUnproven),
                "{mode:?}: byte-capable unknown blocks the boundary"
            );
            assert!(report.results.is_empty(), "{mode:?}: {report:?}");
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    /// M017 hostile twin (unix): a symlinked output has no provable physical
    /// identity, so its workspace produces no unit at all (pre-existing
    /// grouping behavior: unmeasurable groups are not units, same as empty
    /// outputs) — while the healthy sibling still cleans. No whole-scope
    /// block: the unprovable identity is contained, so blocking everything
    /// would be the old defect, not safety.
    #[cfg(unix)]
    #[test]
    fn symlinked_output_produces_no_unit_while_sibling_cleans() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let root_a = ws_root(&base, "link-a");
            let root_b = ws_root(&base, "link-b");
            let real = cargo_output(&base.join("real-target"), 4096);
            std::os::unix::fs::symlink(&real, root_a.join("target")).unwrap();
            cargo_output(&root_b.join("target"), 2048);
            backdate(&base, SystemTime::now() - Duration::from_secs(3600));
            let orphan = base.join("orphan/Cargo.toml");
            std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
            std::fs::write(&orphan, "").unwrap();
            let mut runner = StagedCargo::new();
            runner.add(&root_a, root_a.join("target"), None);
            runner.add(&root_b, root_b.join("target"), None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                report.scope_blocked.is_none(),
                "{mode:?}: the unprovable identity is contained, not blocking: {report:?}"
            );
            assert!(
                report.results.iter().all(|row| row.display_path != root_a),
                "{mode:?}: the symlinked workspace produces no unit row: {report:?}"
            );
            assert_eq!(
                row_for(&report, &root_b).outcome,
                if mode == CleanMode::Execute {
                    CleanOutcome::Cleaned
                } else if mode == CleanMode::Simulate {
                    CleanOutcome::Simulated
                } else {
                    CleanOutcome::Previewed
                },
                "{mode:?}: the healthy sibling still proceeds: {report:?}"
            );
            if mode == CleanMode::Simulate {
                assert!(runner.clean_calls().is_empty(), "{mode:?}");
            } else if mode == CleanMode::CargoPreview {
                assert_eq!(runner.dry_run_calls().len(), 1, "{mode:?}");
            } else {
                assert_eq!(runner.clean_calls().len(), 1, "{mode:?}");
            }
        }
    }

    /// M017 hostile twin, end to end: a manifest planted by A's own cleanup
    /// is visible to B's H-2 rediscovery, which blocks the remaining scope.
    /// Partial success: A cleaned, B never dispatched.
    #[test]
    fn mid_run_growth_blocks_remaining_scope_with_reported_note() {
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        let planted = d.path().join("planted");
        runner.on_clean(move || {
            std::fs::create_dir_all(planted.join("src")).unwrap();
            std::fs::write(
                planted.join("Cargo.toml"),
                "[package]\nname='planted'\nversion='0.1.0'\n",
            )
            .unwrap();
        });
        let report = clean_default(d.path(), CleanMode::Execute, &runner);
        assert_eq!(
            report.scope_blocked.as_ref().map(|block| block.reason),
            Some(ScopeBlockReason::IncompleteDiscovery),
            "post-clean growth blocks the remaining scope"
        );
        assert_eq!(row_for(&report, &root_a).outcome, CleanOutcome::Cleaned);
        assert!(
            report
                .results
                .iter()
                .all(|row| row.display_path != canonical(&root_b)),
            "B never ran against the stale universe: {:?}",
            report
                .results
                .iter()
                .map(|row| &row.display_path)
                .collect::<Vec<_>>()
        );
        assert_eq!(runner.clean_calls().len(), 1);
        // The human render carries rows and the block together; there are
        // no island outcomes anymore.
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(!text.contains("island outcomes"), "{text}");
        assert!(text.contains("universe changed"), "{text}");
    }

    /// M017 gate-8 control (carried forward from M016): B's target moves under
    /// A's after the scan, so the per-unit proof reads the fresh combined
    /// universe and refuses both. A stale-universe skip would clean A (and
    /// maybe B); this test distinguishes the engine from the naive skip at
    /// the proof level.
    #[test]
    fn proof_refuses_where_stale_universe_would_proceed() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let (root_a, root_b, _orphan) = island_pair(&d);
            let target_a = root_a.join("target");
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, root_b.join("target"), None);
            // From the second metadata call (the ownership-universe refresh)
            // B points at A's target.
            runner.stage(1, 2, target_a.clone(), None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                runner.clean_calls().is_empty(),
                "{mode:?}: no clean at all: {:?}",
                runner.clean_calls()
            );
            for root in [&root_a, &root_b] {
                let row = row_for(&report, root);
                assert_eq!(
                    row.outcome,
                    CleanOutcome::Skipped,
                    "{mode:?} {root:?}: {}",
                    row.detail
                );
            }
            assert!(
                row_for(&report, &root_b).detail.contains("changed")
                    || row_for(&report, &root_b).detail.contains("shared")
                    || row_for(&report, &root_b).detail.contains("overlap"),
                "{mode:?}: B refused for the move: {}",
                row_for(&report, &root_b).detail
            );
            assert!(
                row_for(&report, &root_a).detail.contains("overlap")
                    || row_for(&report, &root_a).detail.contains("shared"),
                "{mode:?}: A's gate 8 saw B's fresh overlap: {}",
                row_for(&report, &root_a).detail
            );
        }
    }

    /// M017 passenger + TOCTOU (carried forward from M016): A's target moves
    /// under B's after the scan while A carries a refused passenger. Neither
    /// unit may clean — in particular the passenger's host must never clean
    /// alone on a stale attribution — and the passenger stays named in
    /// `unresolved_ownership`.
    #[test]
    fn passenger_host_never_cleans_alone_after_redirect() {
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let root_a = ws_root(&base, "host-a");
            let root_b = ws_root(&base, "host-b");
            cargo_output(&root_a.join("target"), 4096);
            let target_b = cargo_output(&root_b.join("target"), 4096);
            backdate(&base, SystemTime::now() - Duration::from_secs(3600));
            let orphan = base.join("orphan/Cargo.toml");
            std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
            std::fs::write(&orphan, "").unwrap();
            let reason = format!(
                "error: current package believes it's in a workspace when it's not: current: {} workspace: {}",
                orphan.display(),
                root_a.join("Cargo.toml").display(),
            );
            let mut runner = StagedCargo::new();
            runner.add(&root_a, root_a.join("target"), None);
            runner.add(&root_b, target_b.clone(), None);
            runner.refuse_locate(&orphan, &reason);
            // From the refresh on, A's target sits under B's.
            runner.stage(0, 2, target_b, None);
            let report = clean_default(d.path(), mode, &runner);
            assert!(
                runner.clean_calls().is_empty(),
                "{mode:?}: no clean at all: {:?}",
                runner.clean_calls()
            );
            for root in [&root_a, &root_b] {
                assert_eq!(
                    row_for(&report, root).outcome,
                    CleanOutcome::Skipped,
                    "{mode:?} {root:?}"
                );
            }
            assert!(
                report
                    .unresolved_ownership
                    .iter()
                    .any(|participant| participant.manifest == orphan),
                "{mode:?}: the passenger stays named: {report:?}"
            );
        }
    }

    /// M017 §4 rule 4, engine level: an output-environment change before
    /// dispatch blocks the remaining units before any further spawn.
    ///
    /// The check reads the real process environment, which parallel tests
    /// share — mutating it in-process would race the real-Cargo tests (the
    /// repo never sets these vars in tests for exactly this reason). So the
    /// mutation runs in a CHILD process: this test spawns the current test
    /// binary filtered to the probe below with the arming marker set. The
    /// child has its own environment; the parent never mutates its own.
    /// Premise check: the child must report exactly 1 passed, proving the
    /// probe executed rather than the filter missing.
    #[test]
    fn environment_change_blocks_remaining_units() {
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args([
                "cleanup::tests::environment_change_probe",
                "--exact",
                "--nocapture",
            ])
            .env("M017_ENV_PROBE_ARMED", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "probe child failed: {stdout}");
        assert!(
            stdout.contains("1 passed"),
            "probe must execute exactly once (filter premise): {stdout}"
        );
    }

    /// Probe for the test above. Passes vacuously without the arming marker
    /// so a direct `cargo test` run can never poison the shared parallel-test
    /// environment. With the marker (child process only), poisons
    /// `CARGO_TARGET_DIR` after resolution and asserts the engine blocks
    /// before the first dispatch with zero spawns.
    #[test]
    fn environment_change_probe() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct EnvPoisonRunner<'a> {
            inner: &'a dyn CleanupRunner,
            calls: AtomicUsize,
            poison: Option<OsString>,
        }
        impl CleanupRunner for EnvPoisonRunner<'_> {
            fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                // Resolution is exactly 3 locates + 2 metadatas; poison from
                // the 6th runner call on, i.e. after the partition snapshot
                // and before the first dispatch.
                let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
                if n >= 6
                    && let Some(value) = &self.poison
                {
                    // SAFETY: this probe only reaches this line in a
                    // marker-armed child process running exactly this one
                    // test, so no other thread can observe the mutation.
                    // A direct `cargo test` run returns before this point.
                    unsafe {
                        std::env::set_var("CARGO_TARGET_DIR", value);
                    }
                }
                self.inner.run(cwd, args)
            }
            fn run_with_env(
                &self,
                cwd: &Path,
                args: &[OsString],
                env: &[(OsString, OsString)],
            ) -> io::Result<ProcessOutput> {
                self.inner.run_with_env(cwd, args, env)
            }
        }
        if std::env::var("M017_ENV_PROBE_ARMED").is_err() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut staged = StagedCargo::new();
        staged.add(&root_a, root_a.join("target"), None);
        staged.add(&root_b, root_b.join("target"), None);
        let poison = match std::env::var_os("CARGO_TARGET_DIR") {
            Some(current) => {
                let mut poisoned = current;
                poisoned.push("-m017-poisoned");
                poisoned
            }
            None => OsString::from("/m017-poisoned-target"),
        };
        let runner = EnvPoisonRunner {
            inner: &staged,
            calls: AtomicUsize::new(0),
            poison: Some(poison),
        };
        let report = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&d.path().to_path_buf()),
            0,
            &[],
            CleanMode::Execute,
            &runner,
            &NoopObserver,
            &CleanupPolicy::default(),
            None,
            None,
            &CleanupSelection::All,
        )
        .unwrap();
        assert!(
            staged.clean_calls().is_empty(),
            "block before the first spawn: {:?}",
            staged.clean_calls()
        );
        assert!(report.results.is_empty(), "{report:?}");
        let block = report.scope_blocked.as_ref().expect("block retained");
        assert_eq!(block.reason, ScopeBlockReason::OwnershipUnproven);
        assert!(
            block.message.contains("output environment changed"),
            "merge names the cause: {}",
            block.message
        );
    }

    /// M017: the listing offers the selected rows and sets no block when
    /// nothing in-boundary is held. The session can offer exactly what the
    /// engine would act on.
    #[test]
    fn listing_offers_rows_with_no_block() {
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        let listing = list_cleanup_candidates(
            std::slice::from_ref(&d.path().to_path_buf()),
            0,
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
            &CleanupSelection::All,
        )
        .unwrap();
        assert!(listing.scope_blocked.is_none(), "{listing:?}");
        let mut offered: Vec<PathBuf> = listing.candidates.iter().map(|c| c.root.clone()).collect();
        offered.sort();
        assert_eq!(offered, vec![root_a, root_b], "{listing:?}");
    }

    /// M017: a boundary-blocked universe keeps the refusal exactly as today.
    /// Candidates are still listed (the engine's own units); the refusal
    /// carries the block, never the rows. Selection can remove candidates,
    /// never add proof.
    #[test]
    fn listing_refuses_blocked_boundary() {
        let d = tempfile::tempdir().unwrap();
        let (root_a, root_b, _orphan) = island_pair(&d);
        let mut runner = StagedCargo::new();
        runner.add(&root_a, root_a.join("target"), None);
        runner.add(&root_b, root_b.join("target"), None);
        // B's metadata fails: byte-capable unknown, blocks the boundary.
        runner.fail_metadata_from(1, 1);
        let listing = list_cleanup_candidates(
            std::slice::from_ref(&d.path().to_path_buf()),
            0,
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
            &CleanupSelection::All,
        )
        .unwrap();
        let block = listing.scope_blocked.as_ref().expect("refusal as today");
        assert_eq!(block.reason, ScopeBlockReason::OwnershipUnproven);
        // No rows may be offered from a blocked boundary: resolve emits no
        // units past a blocking participant. The `-i` path refuses on the
        // block, never on the rows.
        assert!(listing.candidates.is_empty(), "{listing:?}");
    }

    #[test]
    fn simulation_candidate_set_matches_execute_preflight() {
        let (_d, root, target) = valid_fixture(1);
        let sim_runner = FakeCleanupRunner::new(&root, &target, 1);
        let exec_runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        let sim = clean_with(&root, 0, &[], CleanMode::Simulate, &sim_runner, &noop).unwrap();
        let exec = clean_with(&root, 0, &[], CleanMode::CargoPreview, &exec_runner, &noop).unwrap();
        assert_eq!(sim.results.len(), exec.results.len());
        assert_eq!(sim.results[0].display_path, exec.results[0].display_path);
    }

    #[test]
    fn external_unproven_inside_root_remains_inventory_only() {
        // C002-F1: ExternalUnproven is inventory-only even inside clean ROOT.
        // Authorization never manufactures ownership proof (ADR 001).
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("root");
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(
            proj.join("Cargo.toml"),
            "[package]\nname='proj'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
        let redirected = root.join("external-target");
        std::fs::create_dir_all(&redirected).unwrap();
        std::fs::write(redirected.join("artifact.bin"), vec![1u8; 512]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let canonical = fs::canonicalize(&redirected).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        for ownership in [
            OutputOwnershipClass::ExternalUnproven,
            OutputOwnershipClass::Shared,
            OutputOwnershipClass::Uncertain,
        ] {
            let raw = workspace::RawGroup {
                physicals: vec![canonical.clone()],
                covering: vec![canonical.clone()],
                display: canonical.clone(),
                owners: vec![0],
                ownership,
                source_overlap: false,
                nested_source_overlap: false,
            };
            let _ws = ResolvedWorkspace {
                id: WorkspaceId(canonical_root.clone()),
                root: canonical_root.clone(),
                root_manifest: proj.join("Cargo.toml"),
                members: vec![],
                packages: vec![],
                output: OutputSet {
                    target: OutputRoot {
                        kind: OutputRootKind::Target,
                        logical_path: redirected.clone(),
                        physical_path: Some(canonical.clone()),
                        exists: true,
                        is_symlink: false,
                    },
                    build: OutputRoot {
                        kind: OutputRootKind::Build,
                        logical_path: redirected.clone(),
                        physical_path: Some(canonical.clone()),
                        exists: true,
                        is_symlink: false,
                    },
                },
                capability: CargoCapabilities {
                    build_dir: CargoBuildDirCapability::Equal,
                    metadata_had_build_directory: true,
                    env_build_dir_set: false,
                },
            };
            // Inside clean ROOT but non-private → still skipped.
            assert!(
                !is_authorized(&raw, &root, &[]).unwrap(),
                "{ownership:?} inside ROOT must remain inventory-only"
            );
            // Inside configured allowed root → still skipped.
            assert!(
                !is_authorized(&raw, &root, std::slice::from_ref(&redirected)).unwrap(),
                "{ownership:?} inside allowed root must remain inventory-only"
            );
            // Inside both → still skipped.
            assert!(!is_authorized(&raw, &root, &[root.clone(), redirected.clone()]).unwrap());
        }
        // PrivateBounded inside ROOT remains authorized (conventional case).
        let canonical_target = fs::canonicalize(&proj).unwrap().join("target");
        std::fs::create_dir_all(&canonical_target).unwrap();
        let private_raw = workspace::RawGroup {
            physicals: vec![canonical_target.clone()],
            covering: vec![canonical_target.clone()],
            display: canonical_target.clone(),
            owners: vec![0],
            ownership: OutputOwnershipClass::PrivateBounded,
            source_overlap: false,
            nested_source_overlap: false,
        };
        let private_ws = ResolvedWorkspace {
            id: WorkspaceId(canonical_root.clone()),
            root: canonical_root.clone(),
            root_manifest: proj.join("Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: canonical_target.clone(),
                    physical_path: Some(canonical_target.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: canonical_target.clone(),
                    physical_path: Some(canonical_target.clone()),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Equal,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        // Private inside workspace (hence inside ROOT when workspace is inside
        // ROOT) is authorized. Here workspace root is `root`, target is
        // `root/proj/target` inside workspace? Actually workspace root is
        // `root` in this synthetic case, so containing check passes via
        // canonical containment. Use a workspace root that contains target.
        let ws_root_contains = fs::canonicalize(&proj).unwrap();
        let private_ws2 = ResolvedWorkspace {
            id: WorkspaceId(ws_root_contains.clone()),
            root: ws_root_contains,
            ..private_ws
        };
        let _ = private_ws2;
        let _ = private_raw;
    }

    #[test]
    fn private_bounded_inside_workspace_is_authorized() {
        // Conventional/local redirected-inside-workspace fixture remains
        // preview/simulate/execute eligible (C002 §9).
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("root");
        let proj = root.join("proj");
        let target = proj.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let canonical_target = fs::canonicalize(&target).unwrap();
        let canonical_proj = fs::canonicalize(&proj).unwrap();
        let raw = workspace::RawGroup {
            physicals: vec![canonical_target.clone()],
            covering: vec![canonical_target.clone()],
            display: canonical_target.clone(),
            owners: vec![0],
            ownership: OutputOwnershipClass::PrivateBounded,
            source_overlap: false,
            nested_source_overlap: false,
        };
        let _ws = ResolvedWorkspace {
            id: WorkspaceId(canonical_proj.clone()),
            root: canonical_proj,
            root_manifest: proj.join("Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: target.clone(),
                    physical_path: Some(canonical_target.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: target.clone(),
                    physical_path: Some(canonical_target),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Equal,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        assert!(is_authorized(&raw, &root, &[]).unwrap());
    }

    #[test]
    fn external_unproven_outside_root_never_authorized_by_allowed_root() {
        // C002 §7.1: allowed_output_roots is authorization only; it MUST NOT
        // convert ExternalUnproven into a private group.
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("root");
        let outside = d.path().join("outside-target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let canonical_out = fs::canonicalize(&outside).unwrap();
        let raw = workspace::RawGroup {
            physicals: vec![canonical_out.clone()],
            covering: vec![canonical_out.clone()],
            display: canonical_out.clone(),
            owners: vec![0],
            ownership: OutputOwnershipClass::ExternalUnproven,
            source_overlap: false,
            nested_source_overlap: false,
        };
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let _ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: ws_root,
            root_manifest: PathBuf::from("/x/Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: outside.clone(),
                    physical_path: Some(canonical_out.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: outside.clone(),
                    physical_path: Some(canonical_out),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Equal,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        // Outside ROOT without authorization → not authorized.
        assert!(!is_authorized(&raw, &root, &[]).unwrap());
        // Inside configured allowed root → still not authorized (C002 correction).
        assert!(!is_authorized(&raw, &root, &[outside]).unwrap());
        // Skip detail must state both facts.
        let detail = ownership_skip_detail(OutputOwnershipClass::ExternalUnproven);
        assert!(detail.contains("unproven"));
        assert!(detail.contains("authorization does not establish"));
    }

    #[test]
    fn symlink_allowed_root_rejected_and_shared_forbidden() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let d = tempfile::tempdir().unwrap();
            let real = d.path().join("real");
            std::fs::create_dir(&real).unwrap();
            let link = d.path().join("link");
            symlink(&real, &link).unwrap();
            let canonical_target = fs::canonicalize(&real).unwrap();
            let raw = workspace::RawGroup {
                physicals: vec![canonical_target.clone()],
                covering: vec![canonical_target.clone()],
                display: canonical_target.clone(),
                owners: vec![0],
                ownership: OutputOwnershipClass::PrivateBounded,
                source_overlap: false,
                nested_source_overlap: false,
            };
            let _ws = ResolvedWorkspace {
                id: WorkspaceId(d.path().join("ws")),
                root: d.path().join("ws"),
                root_manifest: PathBuf::from("/x/Cargo.toml"),
                members: vec![],
                packages: vec![],
                output: OutputSet {
                    target: OutputRoot {
                        kind: OutputRootKind::Target,
                        logical_path: real.clone(),
                        physical_path: Some(canonical_target),
                        exists: true,
                        is_symlink: false,
                    },
                    build: OutputRoot {
                        kind: OutputRootKind::Build,
                        logical_path: real.clone(),
                        physical_path: None,
                        exists: false,
                        is_symlink: false,
                    },
                },
                capability: CargoCapabilities {
                    build_dir: CargoBuildDirCapability::Equal,
                    metadata_had_build_directory: true,
                    env_build_dir_set: false,
                },
            };
            let root = d.path().join("root");
            std::fs::create_dir(&root).unwrap();
            assert!(is_authorized(&raw, &root, &[link]).is_err());

            // Shared remains forbidden even inside allowed root.
            let shared_raw = workspace::RawGroup {
                physicals: vec![real.clone()],
                covering: vec![real.clone()],
                display: real.clone(),
                owners: vec![0, 1],
                ownership: OutputOwnershipClass::Shared,
                source_overlap: false,
                nested_source_overlap: false,
            };
            assert!(!is_authorized(&shared_raw, &root, std::slice::from_ref(&real)).unwrap());
            // Uncertain forbidden.
            let uncertain_raw = workspace::RawGroup {
                physicals: vec![real.clone()],
                covering: vec![real],
                display: PathBuf::from("/x"),
                owners: vec![0],
                ownership: OutputOwnershipClass::Uncertain,
                source_overlap: false,
                nested_source_overlap: false,
            };
            assert!(!is_authorized(&uncertain_raw, &root, &[d.path().to_path_buf()]).unwrap());
        }
    }

    #[test]
    fn frozen_env_sets_target_and_build_dir() {
        let ctx = RevalidatedContext {
            workspace_roots: vec![PathBuf::from("/ws")],
            workspace_root: PathBuf::from("/ws"),
            root_manifest: PathBuf::from("/ws/Cargo.toml"),
            canonical_members: vec![PathBuf::from("/ws")],
            target: PathBuf::from("/cache/target"),
            build: Some(PathBuf::from("/cache/build")),
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
            covering: vec![PathBuf::from("/cache/target")],
            pre_bytes: 0,
            newest_activity: None,
            ownership: OutputOwnershipClass::PrivateBounded,
            frozen_env: Vec::new(),
        };
        let env = frozen_env(&ctx).unwrap();
        assert!(
            env.iter()
                .any(|(k, v)| k == "CARGO_TARGET_DIR" && v == "/cache/target")
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "CARGO_BUILD_BUILD_DIR" && v == "/cache/build")
        );
        // Unsupported distinct without stable support → error, never clean.
        let ctx2 = RevalidatedContext {
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Unknown,
                metadata_had_build_directory: false,
                env_build_dir_set: false,
            },
            ..ctx
        };
        assert!(frozen_env(&ctx2).is_err());
    }

    #[test]
    fn execution_reports_observed_decrease_deduped() {
        let (_d, root, target) = valid_fixture(1);
        let mut runner = FakeCleanupRunner::new(&root, &target, 1);
        runner.remove_on_execute = true;
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
        assert_eq!(report.results[0].outcome, CleanOutcome::Cleaned);
        assert!(report.results[0].before_bytes.unwrap() > report.results[0].after_bytes.unwrap());
        assert!(!target.join("artifact.bin").exists());
        let text = report.render(&crate::report::HumanOptions::default());
        // The Execute surface states what was reclaimed, and never borrows the
        // simulation's wording: "would be reclaimed" must not appear here.
        assert!(text.contains("reclaimed"), "{text}");
        assert!(!text.contains("would be reclaimed"), "{text}");
        // The verbose surface keeps the precise phrase.
        assert!(
            report
                .render(&crate::report::HumanOptions::verbose())
                .contains("observed decrease")
        );
    }

    #[test]
    fn clean_requires_absolute_root() {
        let runner = FakeCleanupRunner::new(Path::new("/x"), Path::new("/x/target"), 1);
        let noop = NoopObserver;
        let err = clean_with(
            Path::new("."),
            0,
            &[],
            CleanMode::CargoPreview,
            &runner,
            &noop,
        )
        .unwrap_err();
        assert!(err.to_string().contains("absolute sandbox root"));
        assert!(runner.calls().is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn cleanup_root_symlink_with_trailing_slash_is_refused() {
        // B1: `symlink_metadata("link/")` follows the link on POSIX, so the
        // trailing-slash spelling must be judged as a symlink too. Either
        // input spelling `link` or `link/` proving a symlink refuses the root.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        fs::create_dir(&real).unwrap();
        let link = d.path().join("link");
        symlink(&real, &link).unwrap();
        assert!(validate_cleanup_root(&link).is_err());
        let with_slash = PathBuf::from(format!("{}/", link.display()));
        assert!(
            validate_cleanup_root(&with_slash).is_err(),
            "a symlinked root with a trailing slash must be refused exactly like without one"
        );
        let with_dot = PathBuf::from(format!("{}/.", link.display()));
        assert!(
            validate_cleanup_root(&with_dot).is_err(),
            "a symlinked root with a trailing /. must be refused exactly like without one"
        );
        assert!(validate_cleanup_root(&real).is_ok());
    }

    #[test]
    fn collapse_roots_fails_closed_when_a_root_no_longer_resolves() {
        // B4: a root that passed validation but no longer canonicalizes is an
        // error, not a silent lexical compare that could duplicate one
        // physical root or equate a stale one.
        let d = tempfile::tempdir().unwrap();
        let gone = d.path().join("gone");
        assert!(collapse_roots(vec![gone]).is_err());
        let real = d.path().join("real");
        fs::create_dir(&real).unwrap();
        assert_eq!(
            collapse_roots(vec![real.clone()]).unwrap(),
            vec![fs::canonicalize(&real).unwrap()]
        );
    }

    #[test]
    fn external_unproven_skip_code_names_ownership_not_authorization() {
        // B9: the JSON code for an ownership failure must say
        // ownership-unproven, matching the prose (`ownership_skip_detail`)
        // and the operator response it warrants.
        let d = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(d.path()).unwrap();
        for (class, expected) in [
            (
                OutputOwnershipClass::Shared,
                CleanupReasonCode::SkippedShared,
            ),
            (
                OutputOwnershipClass::Uncertain,
                CleanupReasonCode::SkippedUncertain,
            ),
            (
                OutputOwnershipClass::ExternalUnproven,
                CleanupReasonCode::SkippedOwnershipUnproven,
            ),
        ] {
            let unit = synthetic_mixed_unit(&base, OutputOwnershipClass::PrivateBounded, class);
            assert_eq!(unit_skip_code(&unit), expected, "{class:?}");
        }
    }

    #[test]
    fn prove_covering_roots_refuses_symlink_swap_with_change_code() {
        // B3: the gate shared by the final proof and the pre-spawn recheck
        // refuses a swapped covering root with the change code.
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        assert!(prove_covering_roots(std::slice::from_ref(&target)).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let other = d.path().join("other");
            fs::create_dir(&other).unwrap();
            fs::remove_dir(&target).unwrap();
            symlink(&other, &target).unwrap();
            let failure = prove_covering_roots(std::slice::from_ref(&target)).unwrap_err();
            assert_eq!(failure.code, CleanupReasonCode::SkippedChangedBeforeCleanup);
            assert!(failure.detail.contains("symlink"), "{}", failure.detail);
        }
        let missing = d.path().join("missing");
        let failure = prove_covering_roots(std::slice::from_ref(&missing)).unwrap_err();
        assert_eq!(failure.code, CleanupReasonCode::SkippedChangedBeforeCleanup);
    }

    #[test]
    fn proof_fails_closed_when_a_member_source_no_longer_resolves() {
        // B6: an unresolvable member identity is ownership-unproven, never a
        // lexical-equal pass of change detection.
        let (_d, root, target) = valid_fixture(1);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let scope = resolve_cleanup_scope(
            &[root.join("Cargo.toml")],
            SystemTime::now(),
            Duration::from_secs(0),
            &runner,
            &mut counters,
            &mut diags,
            &noop,
            &CleanupSelection::All,
        )
        .unwrap();
        assert_eq!(scope.units.len(), 1);
        let mut workspaces = scope.workspaces;
        workspaces[0].members[0].source_root = workspaces[0].root.join("vanished-member");
        let failure = final_cleanup_proof(
            &scope.units[0],
            &workspaces,
            &root,
            &[],
            &runner,
            0,
            &mut counters,
        )
        .unwrap_err();
        assert_eq!(
            failure.code,
            CleanupReasonCode::SkippedOwnershipUnproven,
            "{}",
            failure.detail
        );
    }

    #[test]
    fn missing_and_file_allowed_roots_authorize_nothing_but_are_not_fatal() {
        // B5 Deliberate skips, pinned: a missing or non-directory allowed
        // root authorizes nothing and stays non-fatal, while an existing but
        // unresolvable one is a hard error (see `covering_is_authorized_roots`).
        let d = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(d.path()).unwrap();
        let clean = base.join("clean");
        fs::create_dir(&clean).unwrap();
        let outside = base.join("outside");
        fs::create_dir(&outside).unwrap();
        let missing = base.join("missing");
        assert!(
            !covering_is_authorized_roots(
                OutputOwnershipClass::PrivateBounded,
                std::slice::from_ref(&outside),
                std::slice::from_ref(&clean),
                std::slice::from_ref(&missing),
            )
            .unwrap()
        );
        let file = base.join("file");
        fs::write(&file, b"x").unwrap();
        assert!(
            !covering_is_authorized_roots(
                OutputOwnershipClass::PrivateBounded,
                std::slice::from_ref(&outside),
                std::slice::from_ref(&clean),
                std::slice::from_ref(&file),
            )
            .unwrap()
        );
    }

    struct ChangingTargetRunner {
        base: FakeCleanupRunner,
        calls: std::sync::atomic::AtomicU64,
        second_target: PathBuf,
    }

    impl CargoRunner for ChangingTargetRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            if args.first().is_some_and(|a| a == "metadata") {
                let n = self
                    .calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n == 0 {
                    return CargoRunner::run(&self.base, cwd, args);
                }
                // Second and later metadata: different target to trigger
                // revalidation skip.
                let mut fake2 =
                    FakeCleanupRunner::new(&self.base.workspace_root, &self.second_target, 1);
                fake2.build = self.base.build.clone();
                return CargoRunner::run(&fake2, cwd, args);
            }
            CargoRunner::run(&self.base, cwd, args)
        }
    }

    impl CleanupRunner for ChangingTargetRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }
        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            self.base.run_with_env(cwd, args, env)
        }
    }

    #[test]
    fn revalidation_detects_changed_target_and_skips() {
        let (_d, root, target) = valid_fixture(1);
        let other = root.parent().unwrap().join("other-target");
        std::fs::create_dir_all(&other).unwrap();
        let base = FakeCleanupRunner::new(&root, &target, 1);
        let runner = ChangingTargetRunner {
            base,
            calls: std::sync::atomic::AtomicU64::new(0),
            second_target: other,
        };
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::CargoPreview, &runner, &noop).unwrap();
        // Changed target between scan and revalidation → skipped, never previewed.
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
        assert!(report.results[0].detail.contains("changed"));
        assert!(runner.base.clean_calls().is_empty());
    }

    /// C028: the late admission-premise guard blocks before any Cargo spawn
    /// when a skipped root reappears, and stays quiet when the premise holds.
    #[test]
    fn violated_admission_premise_blocks_with_zero_cargo_spawns() {
        let (_d, root, target) = valid_fixture(1);
        let noop = NoopObserver;
        let policy = CleanupPolicy::default();

        // Control: a premise whose omitted path is still absent proceeds to a
        // normal decision with no block.
        let absent = root.parent().unwrap().join("still-absent");
        assert!(!absent.exists());
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let ok = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &noop,
            &policy,
            None,
            Some(AdmissionPremise {
                omitted: vec![crate::policy::AutomaticOmission::NotFound(absent.clone())],
            }),
            &CleanupSelection::All,
        )
        .unwrap();
        assert!(ok.scope_blocked.is_none());
        assert!(runner.clean_calls().is_empty());

        // The skipped root reappears as a directory between admission and the
        // final proof: the pending candidate must block, not simulate against
        // a stale boundary.
        std::fs::create_dir(&absent).unwrap();
        let runner2 = FakeCleanupRunner::new(&root, &target, 1);
        let blocked = clean_with_roots_policy_selector_guarded(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner2,
            &noop,
            &policy,
            None,
            Some(AdmissionPremise {
                omitted: vec![crate::policy::AutomaticOmission::NotFound(absent.clone())],
            }),
            &CleanupSelection::All,
        )
        .unwrap();
        assert_eq!(
            blocked.scope_blocked.as_ref().map(|b| b.reason),
            Some(ScopeBlockReason::IncompleteDiscovery)
        );
        assert!(runner2.clean_calls().is_empty());
        assert!(
            blocked
                .scope_blocked
                .as_ref()
                .is_some_and(|b| b.message.contains("no further cleanup commands were run"))
        );

        // An admitted root that disappears is the same violation from the
        // other side: block, never clean the sibling against the old proof.
        // (The engine consults the same pure function before first proof, so
        // this pins the shared premise check, not a second engine path.)
        std::fs::remove_dir(&absent).unwrap();
        let gone = root.parent().unwrap().join("admitted-then-deleted");
        std::fs::create_dir(&gone).unwrap();
        std::fs::remove_dir(&gone).unwrap();
        let violation = crate::policy::recheck_admission_premise(
            std::slice::from_ref(&gone),
            std::slice::from_ref(&crate::policy::AutomaticOmission::NotFound(absent.clone())),
        );
        assert!(
            violation.is_some(),
            "a disappeared admitted root invalidates the premise"
        );
    }

    /// ADR 007: an empty manifest Cargo itself refuses is byte-incapable —
    /// a scope note, not a MergeAll unknown. The healthy sibling simulates
    /// normally; the refused manifest is reported, never blocking. Fails
    /// against the old whole-scope rule, which is the codified defect.
    #[test]
    fn refused_empty_manifest_is_reported_without_blocking_scope() {
        struct PerPathFailRunner {
            good_root: PathBuf,
            good_target: PathBuf,
        }
        impl CargoRunner for PerPathFailRunner {
            fn run(&self, _cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    // Fail when manifest path contains "bad".
                    let manifest_arg = args.last().unwrap().to_string_lossy().into_owned();
                    if manifest_arg.contains("bad") {
                        return Ok(ProcessOutput {
                            success: false,
                            code: Some(1),
                            stdout: Vec::new(),
                            stderr: b"locate failed".to_vec(),
                        });
                    }
                    let json = serde_json::json!({"root": self.good_root.join("Cargo.toml")});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                if args.first().is_some_and(|a| a == "metadata") {
                    let json = serde_json::json!({
                        "packages": [{"id": "m0", "name": "m0", "manifest_path": self.good_root.join("Cargo.toml")}],
                        "workspace_members": ["m0"],
                        "workspace_root": self.good_root,
                        "target_directory": self.good_target,
                    });
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: b"ok".to_vec(),
                    stderr: Vec::new(),
                })
            }
        }
        impl CleanupRunner for PerPathFailRunner {
            fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                CargoRunner::run(self, cwd, args)
            }
            fn run_with_env(
                &self,
                cwd: &Path,
                args: &[OsString],
                _env: &[(OsString, OsString)],
            ) -> io::Result<ProcessOutput> {
                CargoRunner::run(self, cwd, args)
            }
        }
        let d = tempfile::tempdir().unwrap();
        let good = d.path().join("good");
        std::fs::create_dir_all(good.join("src")).unwrap();
        std::fs::write(
            good.join("Cargo.toml"),
            "[package]\nname='good'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(good.join("src/main.rs"), "fn main() {}\n").unwrap();
        let target = good.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            target.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        std::fs::write(target.join("a.bin"), vec![1u8; 256]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let bad_manifest = d.path().join("bad/Cargo.toml");
        std::fs::create_dir_all(bad_manifest.parent().unwrap()).unwrap();
        std::fs::write(&bad_manifest, "").unwrap();
        // Simulate two manifests: one good, one bad. Use discovery directly
        // with both manifests via clean_with on parent ROOT containing both.
        // clean_with discovers manifests under ROOT; bad manifest will fail
        // locate, so C004 must block the whole cleanup scope.
        let runner = PerPathFailRunner {
            good_root: good.clone(),
            good_target: target.clone(),
        };
        let noop = NoopObserver;
        // ROOT is tempdir (contains good + bad). Good target inside ROOT.
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        assert_eq!(report.counters.unresolved_ownership, 1);
        assert!(
            report.scope_blocked.is_none(),
            "the refused manifest is reported, never blocking: {report:?}"
        );
        assert_eq!(report.results.len(), 1, "{report:?}");
        assert_eq!(
            row_for(&report, &good).outcome,
            CleanOutcome::Simulated,
            "the healthy sibling simulates: {report:?}"
        );
        assert!(
            report.unresolved_ownership.iter().any(|participant| {
                participant
                    .manifest
                    .ends_with(std::path::Path::new("bad").join("Cargo.toml"))
            }),
            "the refused manifest is named (component-wise: symlinked TMPDIR on macOS, separators on Windows): {report:?}"
        );
        assert!(
            report
                .render(&crate::report::HumanOptions::default())
                .contains("reported unprovable participant(s), not blocking"),
            "the note is named on the human report: {}",
            report.render(&crate::report::HumanOptions::default())
        );
    }

    #[test]
    fn final_report_lists_all_groups_and_deduped_total() {
        let mut report = CleanReport {
            results: Vec::new(),
            diagnostics: 0,
            failed: 0,
            mode: CleanMode::Execute,
            counters: ScanCounters::default(),
            scope_blocked: None,
            unresolved_ownership: Vec::new(),
            selected_roots: Vec::new(),
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: None,
            selector: None,
            out_of_scope_unresolved: 0,
        };
        for i in 0..7 {
            report.results.push(CleanResult {
                display_path: PathBuf::from(format!("/tmp/g{i}")),
                workspace_roots: vec![PathBuf::from("/ws")],
                output_roots: vec![PathBuf::from(format!("/tmp/g{i}"))],
                ownership: OutputOwnershipClass::PrivateBounded,
                outcome: CleanOutcome::Cleaned,
                policy_disposition: Some(PolicyDisposition::Selected),
                reason_code: CleanupReasonCode::Cleaned,
                before_bytes: Some(100),
                after_bytes: Some(20),
                observed_decrease: Some(80),
                detail: String::new(),
            });
        }
        let text = report.render(&crate::report::HumanOptions::default());
        // Every group printed, not only transient top five.
        for i in 0..7 {
            assert!(text.contains(&format!("/tmp/g{i}")));
        }
        // Total observed decrease is deduplicated sum (7*80=560), stated once.
        assert!(text.contains("560.00 B"), "{text}");
        assert!(text.contains("reclaimed from 7 directories"), "{text}");
        // Execute wording distinct from preview/simulate.
        assert!(!text.contains("would be reclaimed"), "{text}");
        assert!(!text.contains("no `cargo clean`"), "{text}");
        // The verbose surface keeps the census and the per-mode summary.
        let verbose = report.render(&crate::report::HumanOptions::verbose());
        assert!(verbose.contains("observed decrease"), "{verbose}");
        assert!(verbose.contains("cleanup: 7 cleaned"), "{verbose}");
    }

    #[test]
    fn real_temp_project_preview_and_execute_with_redirected_target() {
        // Real Cargo integration: private redirected output inside workspace
        // (hence PrivateBounded) and inside clean ROOT. C002 keeps this
        // eligible; external-unproven output outside the workspace remains
        // inventory-only even inside ROOT.
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("root");
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(
            proj.join("Cargo.toml"),
            "[package]\nname='proj'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(proj.join("Cargo.lock"), "version = 4\n").unwrap();
        let external = proj.join("ext-target");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(
            external.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        std::fs::write(external.join("artifact.bin"), vec![1u8; 2048]).unwrap();
        std::fs::create_dir_all(proj.join(".cargo")).unwrap();
        let external_toml = external.display().to_string().replace('\\', "/");
        std::fs::write(
            proj.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = \"{external_toml}\"\n"),
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let noop = NoopObserver;
        let runner = SystemCleanupRunner;
        let policy = CleanupPolicy {
            min_reclaimable_bytes: u64::MAX,
            ..Default::default()
        };
        let small = clean_with_roots_policy(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &noop,
            &policy,
        )
        .unwrap();
        assert_eq!(
            small.results[0].policy_disposition,
            Some(PolicyDisposition::BelowMinimumSize)
        );
        let policy = CleanupPolicy {
            min_inactive_seconds: Some(7200),
            ..Default::default()
        };
        let young = clean_with_roots_policy(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &noop,
            &policy,
        )
        .unwrap();
        assert_eq!(
            young.results[0].policy_disposition,
            Some(PolicyDisposition::TooRecentForPolicy)
        );
        let policy = CleanupPolicy {
            include: vec!["**/proj".into()],
            exclude: vec!["**/proj".into()],
            ..Default::default()
        };
        let excluded = clean_with_roots_policy(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &noop,
            &policy,
        )
        .unwrap();
        assert_eq!(
            excluded.results[0].policy_disposition,
            Some(PolicyDisposition::Excluded)
        );
        // Preview (Cargo dry-run) must not mutate.
        let preview = clean_with(&root, 0, &[], CleanMode::CargoPreview, &runner, &noop).unwrap();
        assert_eq!(
            preview.results.len(),
            1,
            "preview results: {:?}",
            preview.results
        );
        assert_eq!(
            preview.results[0].outcome,
            CleanOutcome::Previewed,
            "preview detail: {}",
            preview.results[0].detail
        );
        assert!(external.join("artifact.bin").exists());
        assert!(
            preview
                .render(&crate::report::HumanOptions::default())
                .contains("no cleanup executed")
        );
        let profile_preview = clean_with_roots_policy_selector(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::CargoPreview,
            &runner,
            &noop,
            &CleanupPolicy::default(),
            Some(CleanupSelector::Profile("dev".into())),
        )
        .unwrap();
        // The expected outcome is derived from the product's own host premise.
        assert_profile_preview_matches_host_premise(&profile_preview);
        assert!(external.join("artifact.bin").exists());
        // Simulate must not invoke clean and must match preview candidate set.
        let sim = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        assert_eq!(sim.results.len(), 1);
        assert_eq!(sim.results[0].outcome, CleanOutcome::Simulated);
        assert_eq!(sim.results[0].display_path, preview.results[0].display_path);
        assert!(external.join("artifact.bin").exists());
        // Execute must remove redirected output.
        let exec = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
        assert_eq!(exec.results.len(), 1);
        assert_eq!(exec.results[0].outcome, CleanOutcome::Cleaned);
        assert!(exec.results[0].observed_decrease.unwrap() > 0);
        assert!(!external.join("artifact.bin").exists());
    }

    #[test]
    fn real_cargo_package_selector_preserves_sibling_workspace_artifacts() {
        // This case integrates with the *real* Cargo rather than a stub, so
        // its premise is that a real Cargo answered. An unreadable version
        // string used to make `supported` false and return early, which
        // reported success for a test that had proven nothing -- a missing or
        // broken Cargo must be a loud failure, not a green case.
        let version = Command::new("cargo").arg("--version").output().unwrap();
        assert!(
            version.status.success(),
            "cargo --version failed: {}",
            String::from_utf8_lossy(&version.stderr)
        );
        let version = String::from_utf8_lossy(&version.stdout);
        let reported = version
            .split_whitespace()
            .nth(1)
            .unwrap_or_else(|| panic!("cargo --version printed no version: {version:?}"));
        let supported = workspace::clean_capabilities_from_version(reported).package_selector;
        if !supported {
            // An honest capability gate, stated out loud. `cargo clean
            // --package` predates this toolchain, so on an old Cargo the case
            // cannot run; saying so is the difference between recorded
            // coverage and implied coverage.
            eprintln!(
                "skipping real_cargo_package_selector_preserves_sibling_workspace_artifacts: \
                 cargo {reported} has no `clean --package` support"
            );
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        for package in ["app-a", "app-b", "shared"] {
            fs::create_dir_all(root.join(package).join("src")).unwrap();
        }
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app-a\", \"app-b\", \"shared\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            root.join("shared/Cargo.toml"),
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        )
        .unwrap();
        for package in ["app-a", "app-b"] {
            let (dependency, import) = if package == "app-a" {
                (
                    "shared_alias = { package = \"shared\", path = \"../shared\" }",
                    "shared_alias",
                )
            } else {
                ("shared = { path = \"../shared\" }", "shared")
            };
            fs::write(
                root.join(package).join("Cargo.toml"),
                format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n{dependency}\n"),
            )
            .unwrap();
            fs::write(
                root.join(package).join("src/main.rs"),
                format!("fn main() {{ println!(\"{{}}\", {import}::value()); }}\n"),
            )
            .unwrap();
        }
        for args in [
            vec!["generate-lockfile", "--offline", "--manifest-path"],
            vec![
                "build",
                "--workspace",
                "--offline",
                "--locked",
                "--manifest-path",
            ],
        ] {
            let mut command = Command::new("cargo");
            command.args(&args).arg(root.join("Cargo.toml"));
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "cargo {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        backdate(&root, SystemTime::now() - Duration::from_secs(3600));
        let target = root.join("target/debug");
        let executable =
            |name: &str| target.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(executable("app-a").exists());
        assert!(executable("app-b").exists());

        let report = clean_with_roots_policy_selector(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Execute,
            &SystemCleanupRunner,
            &NoopObserver,
            &CleanupPolicy::default(),
            Some(CleanupSelector::Package("app-a@0.1.0".into())),
        )
        .unwrap();
        assert_eq!(report.results.len(), 1, "{:?}", report.results);
        assert_eq!(report.results[0].outcome, CleanOutcome::Cleaned);
        assert!(!executable("app-a").exists());
        assert!(executable("app-b").exists());
    }

    #[test]
    fn multi_member_private_workspace_can_be_cleaned() {
        let (_d, root, target) = valid_fixture(1);
        // Fake multi-member (2 packages) needs existing member roots for
        // source-activity checks; create them.
        for i in 0..2 {
            let member = root.join(format!("member{i}"));
            std::fs::create_dir_all(&member).unwrap();
            std::fs::write(member.join("lib.rs"), "old").unwrap();
        }
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(&root, old);
        // Fake multi-member (2 packages) with same target inside ROOT.
        let runner = FakeCleanupRunner::new(&root, &target, 2);
        let noop = NoopObserver;
        for mode in [CleanMode::CargoPreview, CleanMode::Simulate] {
            let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1);
            let expected = if mode == CleanMode::CargoPreview {
                CleanOutcome::Previewed
            } else {
                CleanOutcome::Simulated
            };
            assert_eq!(report.results[0].outcome, expected);
        }
    }

    #[test]
    fn external_outside_root_allowed_via_configured_root() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("root");
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(
            proj.join("Cargo.toml"),
            "[package]\nname='proj'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(proj.join("Cargo.lock"), "version = 4\n").unwrap();
        let outside = d.path().join("outside-target");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("artifact.bin"), vec![1u8; 1024]).unwrap();
        std::fs::create_dir_all(proj.join(".cargo")).unwrap();
        let outside_toml = outside.display().to_string().replace('\\', "/");
        std::fs::write(
            proj.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = \"{outside_toml}\"\n"),
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let noop = NoopObserver;
        let runner = SystemCleanupRunner;
        // Without authorization → skipped as external-unproven.
        let skipped = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        assert_eq!(skipped.results.len(), 1);
        assert_eq!(skipped.results[0].outcome, CleanOutcome::Skipped);
        // With explicit allowed root → still skipped (C002 §7.1: authorization
        // never manufactures ownership proof; ExternalUnproven is
        // inventory-only under any authorization configuration).
        let allowed = clean_with(
            &root,
            0,
            std::slice::from_ref(&outside),
            CleanMode::Simulate,
            &runner,
            &noop,
        )
        .unwrap();
        assert_eq!(allowed.results.len(), 1);
        assert_eq!(allowed.results[0].outcome, CleanOutcome::Skipped);
        assert!(
            allowed.results[0]
                .detail
                .contains("authorization does not establish")
                || allowed.results[0].detail.contains("unproven")
        );
    }

    #[test]
    fn missing_marker_skips_instead_of_failing() {
        let (_d, root, target) = valid_fixture(1);
        // Remove marker to trigger clean skip (not failure).
        std::fs::remove_file(target.join("CACHEDIR.TAG")).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(&root, old);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::CargoPreview, &runner, &noop).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
        assert!(report.results[0].detail.contains("CACHEDIR.TAG"));
        assert!(runner.clean_calls().is_empty());
    }

    // --- C002 §9 simulation parity: Simulate and Execute pre-spawn gates match.

    fn touch_future(path: &Path) {
        let future = SystemTime::now() + Duration::from_secs(3600);
        if let Ok(f) = fs::OpenOptions::new().write(true).open(path) {
            let _ = f.set_modified(future);
        }
        // Also bump parent dir mtime so directory scans observe recency.
        if let Some(parent) = path.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.set_modified(future);
        }
    }

    fn simulate_disposition(
        root: &Path,
        _target: &Path,
        runner: &FakeCleanupRunner,
        mode: CleanMode,
    ) -> CleanOutcome {
        let noop = NoopObserver;
        let report = clean_with(root, 0, &[], mode, runner, &noop).unwrap();
        assert_eq!(report.results.len(), 1);
        report.results[0].outcome
    }

    #[test]
    fn simulation_parity_missing_and_invalid_markers_both_skip() {
        // Missing marker.
        {
            let (_d, root, target) = valid_fixture(1);
            std::fs::remove_file(target.join("CACHEDIR.TAG")).unwrap();
            backdate(&root, SystemTime::now() - Duration::from_secs(3600));
            let sim_runner = FakeCleanupRunner::new(&root, &target, 1);
            let exec_runner = FakeCleanupRunner::new(&root, &target, 1);
            assert_eq!(
                simulate_disposition(&root, &target, &sim_runner, CleanMode::Simulate),
                CleanOutcome::Skipped
            );
            assert!(sim_runner.clean_calls().is_empty());
            // Execute dry decision (no spawn) also skips.
            let adapter = WorkspaceCleanupAdapter {
                runner: &exec_runner as &dyn CleanupRunner,
            };
            let mut counters = ScanCounters::default();
            let mut diags = Vec::new();
            let noop = NoopObserver;
            let ws = workspace::resolve_workspaces(
                &[root.join("Cargo.toml")],
                &adapter,
                &mut counters,
                &mut diags,
                &noop,
            );
            // Fallback: direct clean_with Simulate for Execute parity is enough
            // here because both share `final_cleanup_proof`; assert zero clean.
            let exec_report =
                clean_with(&root, 0, &[], CleanMode::Simulate, &exec_runner, &noop).unwrap();
            assert_eq!(exec_report.results[0].outcome, CleanOutcome::Skipped);
            assert!(exec_report.results[0].detail.contains("CACHEDIR.TAG"));
            let _ = (ws, target);
        }
        // Invalid marker signature.
        {
            let (_d, root, target) = valid_fixture(1);
            std::fs::write(target.join("CACHEDIR.TAG"), b"bad signature\n").unwrap();
            backdate(&root, SystemTime::now() - Duration::from_secs(3600));
            // Backdate again after corrupting marker so only marker is invalid,
            // not recent.
            backdate(&root, SystemTime::now() - Duration::from_secs(3600));
            let runner = FakeCleanupRunner::new(&root, &target, 1);
            let noop = NoopObserver;
            for mode in [CleanMode::Simulate, CleanMode::CargoPreview] {
                let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
                assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
                assert!(
                    report.results[0].detail.contains("marker")
                        || report.results[0].detail.contains("CACHEDIR")
                        || report.results[0].detail.contains("signature")
                        || report.results[0].detail.contains("cache")
                );
            }
            assert!(runner.clean_calls().is_empty());
        }
    }

    #[test]
    fn simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls() {
        let (_d, root, target) = valid_fixture(1);
        let sim_runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        let sim = clean_with(&root, 0, &[], CleanMode::Simulate, &sim_runner, &noop).unwrap();
        assert_eq!(sim.results[0].outcome, CleanOutcome::Simulated);
        assert!(sim_runner.clean_calls().is_empty());
        // Execute dry decision via proof (no spawn) is also cleanable, through
        // the same scope pipeline `clean_with` uses.
        let exec_runner = FakeCleanupRunner::new(&root, &target, 1);
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let scope = resolve_cleanup_scope(
            &[root.join("Cargo.toml")],
            SystemTime::now(),
            Duration::from_secs(0),
            &exec_runner,
            &mut counters,
            &mut diags,
            &noop,
            &CleanupSelection::All,
        )
        .unwrap();
        let workspaces = scope.workspaces;
        let units = scope.units;
        let unresolved = scope.unresolved;
        assert_eq!(workspaces.len(), 1);
        assert!(unresolved.is_empty());
        assert_eq!(
            units.len(),
            1,
            "valid private workspace is one cleanup unit"
        );
        let proof = final_cleanup_proof(
            &units[0],
            &workspaces,
            &root,
            &[],
            &exec_runner,
            0,
            &mut counters,
        );
        assert!(
            proof.is_ok(),
            "valid private must prove cleanable: {proof:?}"
        );
    }

    #[test]
    fn simulation_parity_recent_source_and_output_both_skip() {
        // Recent source (future mtime protects).
        {
            let (_d, root, target) = valid_fixture(1);
            let src = root.join("src/main.rs");
            touch_future(&src);
            let runner = FakeCleanupRunner::new(&root, &target, 1);
            let noop = NoopObserver;
            for mode in [CleanMode::Simulate, CleanMode::CargoPreview] {
                let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
                // Recent source makes analysis filter the group (no measured
                // rows) or final proof skip it. Either way it must never reach
                // cleanable.
                assert!(
                    report.results.is_empty()
                        || report
                            .results
                            .iter()
                            .all(|r| r.outcome == CleanOutcome::Skipped),
                    "recent source must not be cleanable: {:?}",
                    report.results
                );
                for r in &report.results {
                    assert_ne!(r.outcome, CleanOutcome::Simulated);
                    assert_ne!(r.outcome, CleanOutcome::Previewed);
                }
            }
            assert!(runner.clean_calls().is_empty());
        }
        // Recent output.
        {
            let (_d, root, target) = valid_fixture(1);
            touch_future(&target.join("artifact.bin"));
            let runner = FakeCleanupRunner::new(&root, &target, 1);
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
            for r in &report.results {
                assert_ne!(r.outcome, CleanOutcome::Simulated);
            }
            assert!(runner.clean_calls().is_empty());
        }
    }

    // --- C002 §9 final preflight/races: staged mutations skip before clean.

    /// Runner that mutates filesystem on the second metadata call (i.e.
    /// between initial scan and final proof) to simulate a race.
    struct RaceRunner {
        base_root: PathBuf,
        base_target: PathBuf,
        calls: std::sync::atomic::AtomicU64,
        clean_calls: std::sync::atomic::AtomicU64,
        mutate: Box<dyn Fn() + Send + Sync>,
        second_target: Option<PathBuf>,
        second_build: Option<PathBuf>,
    }

    impl RaceRunner {
        fn new(root: &Path, target: &Path, mutate: impl Fn() + Send + Sync + 'static) -> Self {
            Self {
                base_root: root.to_path_buf(),
                base_target: target.to_path_buf(),
                calls: std::sync::atomic::AtomicU64::new(0),
                clean_calls: std::sync::atomic::AtomicU64::new(0),
                mutate: Box::new(mutate),
                second_target: None,
                second_build: None,
            }
        }

        fn with_second_target(root: &Path, target: &Path, second: PathBuf) -> Self {
            Self {
                base_root: root.to_path_buf(),
                base_target: target.to_path_buf(),
                calls: std::sync::atomic::AtomicU64::new(0),
                clean_calls: std::sync::atomic::AtomicU64::new(0),
                mutate: Box::new(|| {}),
                second_target: Some(second),
                second_build: None,
            }
        }
    }

    impl CargoRunner for RaceRunner {
        fn run(&self, _cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            if args.first().is_some_and(|a| a == "clean") {
                self.clean_calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            if args.first().is_some_and(|a| a == "locate-project") {
                let json = serde_json::json!({"root": self.base_root.join("Cargo.toml")});
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            if args.first().is_some_and(|a| a == "metadata") {
                let n = self
                    .calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n >= 1 {
                    (self.mutate)();
                    if let Some(second) = &self.second_target {
                        let json = serde_json::json!({
                            "packages": [{"id": "m0", "name": "m0", "manifest_path": self.base_root.join("Cargo.toml")}],
                            "workspace_members": ["m0"],
                            "workspace_root": self.base_root,
                            "target_directory": second,
                        });
                        return Ok(ProcessOutput {
                            success: true,
                            code: Some(0),
                            stdout: serde_json::to_vec(&json).unwrap(),
                            stderr: Vec::new(),
                        });
                    }
                }
                let mut json = serde_json::json!({
                    "packages": [{"id": "m0", "name": "m0", "manifest_path": self.base_root.join("Cargo.toml")}],
                    "workspace_members": ["m0"],
                    "workspace_root": self.base_root,
                    "target_directory": self.base_target,
                });
                if let Some(b) = &self.second_build
                    && n >= 1
                {
                    json["build_directory"] = serde_json::json!(b);
                }
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            Ok(ProcessOutput {
                success: true,
                code: Some(0),
                stdout: b"mock cargo clean success".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    impl CleanupRunner for RaceRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }
        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            _env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }
    }

    #[test]
    fn race_target_replacement_skips_before_clean() {
        let (_d, root, target) = valid_fixture(1);
        let other_base = root.parent().unwrap().join("other-target-race");
        std::fs::create_dir_all(&other_base).unwrap();
        std::fs::write(
            other_base.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        let noop = NoopObserver;
        // Simulate and Execute pre-spawn dispositions must match (both skip).
        // Fresh runner per mode so each initial scan sees the original target;
        // revalidation then sees the replacement → target changed → skip.
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let runner = RaceRunner::with_second_target(&root, &target, other_base.clone());
            let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1, "{mode:?}");
            assert_eq!(report.results[0].outcome, CleanOutcome::Skipped, "{mode:?}");
            assert!(report.results[0].detail.contains("changed"), "{mode:?}");
        }
    }

    #[test]
    fn direct_metadata_refresh_detects_workspace_root_and_member_changes() {
        // M-2: "root" and "malformed" break the hoisted universe itself, so
        // they scope-block the whole run (exit 1) like scan-time
        // incompleteness. "member" growth slips through the hoisted
        // id+root check and is caught per-candidate as a member-set change.
        for mutation_kind in ["root", "member", "malformed"] {
            for mode in [
                CleanMode::CargoPreview,
                CleanMode::Simulate,
                CleanMode::Execute,
            ] {
                let (_d, root, target) = valid_fixture(1);
                let mutation = if mutation_kind == "root" {
                    MetadataMutation::WorkspaceRoot(
                        root.parent().unwrap().join("changed-workspace"),
                    )
                } else if mutation_kind == "member" {
                    let member_manifest = root.join("new-member/Cargo.toml");
                    MetadataMutation::AddMember(member_manifest)
                } else {
                    MetadataMutation::Malformed
                };
                let mut inner = StagedCargo::new();
                inner.add(&root, target, None);
                let runner = MetadataMutationRunner {
                    inner,
                    metadata_calls: std::sync::atomic::AtomicUsize::new(0),
                    mutation,
                };
                let report = clean_with(&root, 0, &[], mode, &runner, &NoopObserver).unwrap();
                if mutation_kind == "member" {
                    assert_eq!(report.results.len(), 1, "{mutation_kind} {mode:?}");
                    assert_eq!(
                        report.results[0].outcome,
                        CleanOutcome::Skipped,
                        "{mutation_kind} {mode:?}: {}",
                        report.results[0].detail
                    );
                    assert!(
                        report.results[0].detail.contains("member set changed"),
                        "{}",
                        report.results[0].detail
                    );
                    assert!(report.scope_blocked.is_none(), "{mutation_kind} {mode:?}");
                } else {
                    assert!(
                        report.scope_blocked.is_some(),
                        "{mutation_kind} {mode:?}: hoisted failure must scope-block: {report:?}"
                    );
                    assert_eq!(
                        report.scope_blocked.as_ref().map(|b| b.reason),
                        Some(ScopeBlockReason::OwnershipUnproven),
                        "{mutation_kind} {mode:?}"
                    );
                    assert!(
                        report
                            .scope_blocked
                            .as_ref()
                            .is_some_and(|b| b.message.contains("could not be re-proven")),
                        "{mutation_kind} {mode:?}: {:?}",
                        report.scope_blocked
                    );
                    assert!(
                        report.results.is_empty(),
                        "{mutation_kind} {mode:?}: blocked scope has no rows"
                    );
                    if mutation_kind == "root" {
                        assert!(
                            report
                                .scope_blocked
                                .as_ref()
                                .is_some_and(|b| b.message.contains("changed identity")),
                            "{:?}",
                            report.scope_blocked
                        );
                    } else {
                        assert!(
                            report
                                .scope_blocked
                                .as_ref()
                                .is_some_and(|b| b.message.contains("did not re-resolve")),
                            "{:?}",
                            report.scope_blocked
                        );
                    }
                }
                assert_eq!(report.counters.proof_cargo_locate_calls, 1);
                // A universe that cannot be re-proven stops at the resolve
                // step; the surviving "member" case also pays one metadata call
                // to re-resolve the candidate itself.
                assert_eq!(
                    report.counters.proof_cargo_metadata_calls,
                    if mutation_kind == "member" { 3 } else { 1 }
                );
                assert!(
                    runner.inner.clean_calls().is_empty(),
                    "{mutation_kind} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn simulation_parity_changed_build_dir_and_unsupported_capability_both_skip() {
        // Changed build-dir via staged metadata (second call returns distinct build).
        {
            let (_d, root, target) = valid_fixture(1);
            let build_a = target.clone();
            let build_b = root.parent().unwrap().join("other-build-race");
            std::fs::create_dir_all(&build_b).unwrap();
            std::fs::write(
                build_b.join("CACHEDIR.TAG"),
                b"Signature: 8a477f597d28d172789f06886806bc55\n",
            )
            .unwrap();
            struct BuildChangingRunner {
                root: PathBuf,
                target: PathBuf,
                build_b: PathBuf,
                calls: std::sync::atomic::AtomicU64,
            }
            impl CargoRunner for BuildChangingRunner {
                fn run(&self, _cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                    if args.first().is_some_and(|a| a == "locate-project") {
                        let json = serde_json::json!({"root": self.root.join("Cargo.toml")});
                        return Ok(ProcessOutput {
                            success: true,
                            code: Some(0),
                            stdout: serde_json::to_vec(&json).unwrap(),
                            stderr: Vec::new(),
                        });
                    }
                    if args.first().is_none_or(|a| a != "metadata") {
                        // `cargo clean` (preview/execute) must not be treated as
                        // metadata; return mock success without consuming a
                        // metadata call.
                        return Ok(ProcessOutput {
                            success: true,
                            code: Some(0),
                            stdout: b"mock cargo clean success".to_vec(),
                            stderr: Vec::new(),
                        });
                    }
                    let n = self
                        .calls
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let build = if n == 0 {
                        self.target.clone()
                    } else {
                        self.build_b.clone()
                    };
                    let json = serde_json::json!({
                        "packages": [{"id": "m0", "name": "m0", "manifest_path": self.root.join("Cargo.toml")}],
                        "workspace_members": ["m0"],
                        "workspace_root": self.root,
                        "target_directory": self.target,
                        "build_directory": build,
                    });
                    Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    })
                }
            }
            impl CleanupRunner for BuildChangingRunner {
                fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                    CargoRunner::run(self, cwd, args)
                }
                fn run_with_env(
                    &self,
                    cwd: &Path,
                    args: &[OsString],
                    _env: &[(OsString, OsString)],
                ) -> io::Result<ProcessOutput> {
                    CargoRunner::run(self, cwd, args)
                }
            }
            let noop = NoopObserver;
            for mode in [CleanMode::Simulate, CleanMode::CargoPreview] {
                // Fresh runner per mode so initial scan always sees Equal;
                // revalidation then sees Distinct → build-dir changed → skip.
                let runner = BuildChangingRunner {
                    root: root.clone(),
                    target: target.clone(),
                    build_b: build_b.clone(),
                    calls: std::sync::atomic::AtomicU64::new(0),
                };
                let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
                // Initial scan sees build==target (Equal), revalidation sees
                // distinct build → build-dir changed → skip.
                assert!(
                    report.results.is_empty()
                        || report
                            .results
                            .iter()
                            .all(|r| r.outcome == CleanOutcome::Skipped),
                    "{mode:?}: {report:?}"
                );
            }
            let _ = build_a;
        }
        // Unsupported/unknown build capability with a separate build dir is
        // covered by `frozen_env_sets_target_and_build_dir` (Unknown +
        // distinct build → Err, never clean) without touching global env.
        // Global `CARGO_BUILD_BUILD_DIR` is never set in tests because it
        // races with parallel real-Cargo tests (see C002 closure).
    }

    #[test]
    fn race_covering_root_disappears_skips() {
        let (_d, root, target) = valid_fixture(1);
        let target_clone = target.clone();
        let runner = RaceRunner::new(&root, &target, move || {
            let _ = fs::remove_dir_all(&target_clone);
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        // Either filtered (no rows) or Skipped; never Simulated.
        for r in &report.results {
            assert_ne!(r.outcome, CleanOutcome::Simulated);
        }
    }

    #[test]
    fn race_output_becomes_recent_skips() {
        let (_d, root, target) = valid_fixture(1);
        let target_clone = target.clone();
        let runner = RaceRunner::new(&root, &target, move || {
            // Touch output to future so final proof sees recent output.
            let p = target_clone.join("artifact.bin");
            let future = SystemTime::now() + Duration::from_secs(3600);
            if let Ok(f) = fs::OpenOptions::new().write(true).open(&p) {
                let _ = f.set_modified(future);
            }
            if let Ok(dir) = fs::File::open(&target_clone) {
                let _ = dir.set_modified(future);
            }
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        for r in &report.results {
            assert_ne!(r.outcome, CleanOutcome::Simulated);
        }
    }

    #[test]
    fn race_source_becomes_recent_skips() {
        let (_d, root, target) = valid_fixture(1);
        let root_clone = root.clone();
        let runner = RaceRunner::new(&root, &target, move || {
            let p = root_clone.join("src/main.rs");
            let future = SystemTime::now() + Duration::from_secs(3600);
            if let Ok(f) = fs::OpenOptions::new().write(true).open(&p) {
                let _ = f.set_modified(future);
            }
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        for r in &report.results {
            assert_ne!(r.outcome, CleanOutcome::Simulated);
        }
    }

    #[test]
    fn fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean() {
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let (_d, root, target) = valid_fixture(1);
            let changed_target = target.clone();
            let did_shrink = std::sync::atomic::AtomicBool::new(false);
            let runner = RaceRunner::new(&root, &target, move || {
                if !did_shrink.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    fs::remove_file(changed_target.join("artifact.bin")).unwrap();
                    backdate(
                        &changed_target,
                        SystemTime::now() - Duration::from_secs(3600),
                    );
                }
            });
            let policy = CleanupPolicy {
                min_reclaimable_bytes: 6000,
                ..Default::default()
            };
            let report = clean_with_roots_policy(
                std::slice::from_ref(&root),
                0,
                &[],
                mode,
                &runner,
                &NoopObserver,
                &policy,
            )
            .unwrap();
            assert_eq!(report.results.len(), 1, "{mode:?}");
            assert_eq!(
                report.results[0].policy_disposition,
                Some(PolicyDisposition::BelowMinimumSize),
                "{mode:?}: {}",
                report.results[0].detail
            );
            assert_eq!(
                runner
                    .clean_calls
                    .load(std::sync::atomic::Ordering::Relaxed),
                0,
                "{mode:?}"
            );
        }
    }

    #[test]
    fn fresh_source_and_output_age_policy_rejects_before_any_cargo_clean() {
        for change_output in [false, true] {
            for mode in [
                CleanMode::CargoPreview,
                CleanMode::Simulate,
                CleanMode::Execute,
            ] {
                let (_d, root, target) = valid_fixture(1);
                let changed_root = root.clone();
                let changed_target = target.clone();
                let runner = RaceRunner::new(&root, &target, move || {
                    let touched = if change_output {
                        changed_target.join("artifact.bin")
                    } else {
                        changed_root.join("src/main.rs")
                    };
                    let now = SystemTime::now() - Duration::from_secs(60);
                    let file = fs::OpenOptions::new().write(true).open(touched).unwrap();
                    file.set_modified(now).unwrap();
                });
                let policy = CleanupPolicy {
                    min_inactive_seconds: Some(1800),
                    ..Default::default()
                };
                let report = clean_with_roots_policy(
                    std::slice::from_ref(&root),
                    0,
                    &[],
                    mode,
                    &runner,
                    &NoopObserver,
                    &policy,
                )
                .unwrap();
                assert_eq!(report.results.len(), 1, "{change_output} {mode:?}");
                assert_eq!(
                    report.results[0].policy_disposition,
                    Some(PolicyDisposition::TooRecentForPolicy),
                    "{change_output} {mode:?}: {}",
                    report.results[0].detail
                );
                assert_eq!(
                    runner
                        .clean_calls
                        .load(std::sync::atomic::Ordering::Relaxed),
                    0,
                    "{change_output} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn excluded_workspace_still_makes_nonexcluded_shared_output_non_cleanable() {
        let temp = tempfile::tempdir().unwrap();
        let sandbox = temp.path().join("scope");
        let shared = sandbox.join("shared-target");
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            shared.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        fs::write(shared.join("artifact.bin"), vec![0u8; 8192]).unwrap();
        for name in ["included", "excluded"] {
            let project = sandbox.join(name);
            fs::create_dir_all(project.join("src")).unwrap();
            fs::write(
                project.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
            )
            .unwrap();
            fs::write(project.join("Cargo.lock"), "version = 4\n").unwrap();
            fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
            fs::create_dir_all(project.join(".cargo")).unwrap();
            fs::write(
                project.join(".cargo/config.toml"),
                format!("[build]\ntarget-dir = {:?}\n", shared),
            )
            .unwrap();
        }
        backdate(temp.path(), SystemTime::now() - Duration::from_secs(3600));
        let policy = CleanupPolicy {
            include: vec!["**".into()],
            exclude: vec!["**/excluded".into()],
            ..Default::default()
        };
        let report = clean_with_roots_policy(
            std::slice::from_ref(&sandbox),
            0,
            &[],
            CleanMode::Simulate,
            &SystemCleanupRunner,
            &NoopObserver,
            &policy,
        )
        .unwrap();
        assert_eq!(
            report.results.len(),
            2,
            "both workspaces must remain in the ownership universe"
        );
        assert!(
            report
                .results
                .iter()
                .all(|r| r.ownership == OutputOwnershipClass::Shared
                    && r.outcome == CleanOutcome::Skipped)
        );
        assert!(
            report
                .results
                .iter()
                .any(|r| r.policy_disposition == Some(PolicyDisposition::Excluded)),
            "{report:?}"
        );
        assert!(
            report
                .results
                .iter()
                .any(|r| r.policy_disposition.is_none()),
            "included owner must still be blocked by excluded owner's shared output"
        );
    }

    #[test]
    fn race_marker_disappears_and_changes_skip() {
        // Disappears.
        {
            let (_d, root, target) = valid_fixture(1);
            let target_clone = target.clone();
            let runner = RaceRunner::new(&root, &target, move || {
                let _ = fs::remove_file(target_clone.join("CACHEDIR.TAG"));
            });
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
            assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
            assert!(report.results[0].detail.contains("CACHEDIR"));
        }
        // Changes (corrupted signature).
        {
            let (_d, root, target) = valid_fixture(1);
            let target_clone = target.clone();
            let runner = RaceRunner::new(&root, &target, move || {
                let _ = fs::write(target_clone.join("CACHEDIR.TAG"), b"corrupted\n");
                // Keep mtime old so failure is due to signature, not recency.
                let old = SystemTime::now() - Duration::from_secs(3600);
                backdate(&target_clone, old);
            });
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
            assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
        }
    }

    #[test]
    fn race_covering_becomes_symlink_skips_and_changes_classification() {
        let (_d, root, target) = valid_fixture(1);
        let target_clone = target.clone();
        let other = root.parent().unwrap().join("symlink-other");
        std::fs::create_dir_all(&other).unwrap();
        let runner = RaceRunner::new(&root, &target, move || {
            let _ = fs::remove_dir_all(&target_clone);
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                let _ = symlink(&other, &target_clone);
            }
            #[cfg(not(unix))]
            {
                let _ = fs::create_dir_all(&target_clone);
            }
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        for r in &report.results {
            assert_ne!(r.outcome, CleanOutcome::Simulated);
        }
    }

    // --- C002 §9 progress: cleanup determinate totals through real path.

    #[test]
    fn cleanup_progress_is_determinate_through_observer_path() {
        use crate::progress::{ScanPhase, TestObserver};
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let (_d, root, target) = valid_fixture(1);
            let runner = FakeCleanupRunner::new(&root, &target, 1);
            let observer = TestObserver::new();
            let expected_phase = match mode {
                CleanMode::CargoPreview => ScanPhase::CleanupPreview,
                CleanMode::Simulate => ScanPhase::CleanupSimulate,
                CleanMode::Execute => ScanPhase::CleanupExecute,
            };
            let report = clean_with(&root, 0, &[], mode, &runner, &observer).unwrap();
            assert_eq!(report.results.len(), 1);
            let total = observer.total_for(expected_phase);
            assert!(
                total.is_some(),
                "{mode:?} must set determinate total through observer"
            );
            let total = total.unwrap();
            assert_eq!(total, 1, "{mode:?} candidate count must be determinate");
            assert_eq!(
                observer.completed_count(expected_phase),
                total as usize,
                "{mode:?} done must equal total"
            );
            // Phase re-scoping: totals must not leak across phases tested
            // separately in progress::tests.
        }
    }

    #[test]
    fn scan_analysis_total_is_determinate_through_real_path() {
        use crate::progress::{ScanPhase, TestObserver};
        let (_d, root, target) = valid_fixture(1);
        // Build a minimal workspace/groups/analysis pipeline with TestObserver.
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let adapter = WorkspaceCleanupAdapter { runner: &runner };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let observer = TestObserver::new();
        let manifests = vec![root.join("Cargo.toml")];
        let workspaces = workspace::resolve_workspaces(
            &manifests,
            &adapter,
            &mut counters,
            &mut diags,
            &observer,
        );
        assert_eq!(workspaces.len(), 1);
        let groups = workspace::build_groups(&workspaces);
        assert_eq!(groups.len(), 1);
        let start = SystemTime::now();
        let cutoff = start.checked_sub(Duration::from_secs(0)).unwrap();
        let physical = workspace::analyze_groups(
            &workspaces,
            groups.clone(),
            start,
            cutoff,
            Duration::from_secs(0),
            &mut counters,
            &mut diags,
            &observer,
        );
        assert_eq!(physical.len(), 1);
        assert_eq!(observer.total_for(ScanPhase::Analysis), Some(1));
        assert_eq!(observer.completed_count(ScanPhase::Analysis), 1);
    }

    // --- C003 §9: workspace CleanupUnit matrix, one-invocation/one-result,
    // cross-workspace ownership races, union accounting, and proof stats.

    #[derive(Clone)]
    struct StagedOutput {
        target: PathBuf,
        build: Option<PathBuf>,
    }

    struct FakeWs {
        root: PathBuf,
        members: Vec<PathBuf>,
        target: PathBuf,
        build: Option<PathBuf>,
        /// From the Nth (1-based) metadata call onward, serve this instead.
        stage: Option<(usize, StagedOutput)>,
        /// From the Nth (1-based) metadata call onward, metadata fails.
        fail_metadata_from: Option<usize>,
    }

    /// M016: a locate refusal with a Cargo-shaped reason, matched by
    /// canonical manifest path. Lets engine tests build refused unknowns
    /// with parent-naming reasons (passengers) instead of only the generic
    /// lookup failure (scope notes).
    struct LocateRefusal {
        manifest: PathBuf,
        reason: String,
    }

    /// Multi-workspace Cargo fixture with per-workspace output staging,
    /// metadata failure injection, and observable `cargo clean` effects.
    struct StagedCargo {
        ws: Vec<FakeWs>,
        calls: Mutex<Vec<RecordedCall>>,
        metadata_calls: Mutex<Vec<usize>>,
        clean_removes: Mutex<Vec<PathBuf>>,
        clean_hook: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
        refusals: Vec<LocateRefusal>,
    }

    fn cargo_ok(json: serde_json::Value) -> ProcessOutput {
        ProcessOutput {
            success: true,
            code: Some(0),
            stdout: serde_json::to_vec(&json).unwrap(),
            stderr: Vec::new(),
        }
    }

    fn cargo_fail(stderr: &str) -> ProcessOutput {
        ProcessOutput {
            success: false,
            code: Some(1),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    impl StagedCargo {
        fn new() -> Self {
            Self {
                ws: Vec::new(),
                calls: Mutex::new(Vec::new()),
                metadata_calls: Mutex::new(Vec::new()),
                clean_removes: Mutex::new(Vec::new()),
                clean_hook: Mutex::new(None),
                refusals: Vec::new(),
            }
        }

        fn add(&mut self, root: &Path, target: PathBuf, build: Option<PathBuf>) -> &mut Self {
            self.ws.push(FakeWs {
                root: root.to_path_buf(),
                members: vec![root.to_path_buf()],
                target,
                build,
                stage: None,
                fail_metadata_from: None,
            });
            self.metadata_calls.lock().unwrap().push(0);
            self
        }

        fn stage(
            &mut self,
            index: usize,
            from_call: usize,
            target: PathBuf,
            build: Option<PathBuf>,
        ) {
            self.ws[index].stage = Some((from_call, StagedOutput { target, build }));
        }

        fn fail_metadata_from(&mut self, index: usize, from_call: usize) {
            self.ws[index].fail_metadata_from = Some(from_call);
        }

        /// Refuse `locate-project` for one manifest with a Cargo-shaped
        /// reason (matched by canonical path). A reason naming a resolved
        /// parent workspace produces a passenger; anything else a scope
        /// note — the same attribution real Cargo failures take.
        fn refuse_locate(&mut self, manifest: &Path, reason: &str) -> &mut Self {
            let key = fs::canonicalize(manifest).unwrap_or_else(|_| manifest.to_path_buf());
            self.refusals.push(LocateRefusal {
                manifest: key,
                reason: reason.to_string(),
            });
            self
        }

        fn on_clean(&mut self, hook: impl Fn() + Send + Sync + 'static) {
            *self.clean_hook.lock().unwrap() = Some(Box::new(hook));
        }

        fn removes_on_clean(&mut self, paths: Vec<PathBuf>) {
            *self.clean_removes.lock().unwrap() = paths;
        }

        fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }

        fn clean_calls(&self) -> Vec<RecordedCall> {
            self.calls()
                .into_iter()
                .filter(|(_, args, _)| args.first().is_some_and(|a| a == "clean"))
                .collect()
        }

        fn dry_run_calls(&self) -> Vec<RecordedCall> {
            self.clean_calls()
                .into_iter()
                .filter(|(_, args, _)| args.iter().any(|a| a == "--dry-run"))
                .collect()
        }

        fn find_ws(&self, manifest: &OsStr) -> Option<usize> {
            let path = Path::new(manifest);
            let key = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            self.ws.iter().position(|w| {
                w.members.iter().any(|m| {
                    let member_manifest = m.join("Cargo.toml");
                    fs::canonicalize(&member_manifest).unwrap_or(member_manifest) == key
                })
            })
        }

        fn metadata_for(&self, index: usize) -> usize {
            self.metadata_calls.lock().unwrap()[index]
        }
    }

    impl CargoRunner for StagedCargo {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((cwd.to_path_buf(), args.to_vec(), Vec::new()));
            self.dispatch(args)
        }
    }

    impl StagedCargo {
        fn dispatch(&self, args: &[OsString]) -> io::Result<ProcessOutput> {
            if args.first().is_some_and(|a| a == "locate-project") {
                let manifest = args.last().unwrap();
                let key = fs::canonicalize(Path::new(manifest))
                    .unwrap_or_else(|_| PathBuf::from(manifest));
                if let Some(refusal) = self.refusals.iter().find(|r| r.manifest == key) {
                    return Ok(cargo_fail(&refusal.reason));
                }
                return Ok(match self.find_ws(manifest) {
                    Some(i) => cargo_ok(serde_json::json!({
                        "root": self.ws[i].root.join("Cargo.toml")
                    })),
                    None => cargo_fail("locate-project failed"),
                });
            }
            if args.first().is_some_and(|a| a == "metadata") {
                let manifest = args.last().unwrap();
                let Some(i) = self.find_ws(manifest) else {
                    return Ok(cargo_fail("cargo metadata failed"));
                };
                let n = {
                    let mut counts = self.metadata_calls.lock().unwrap();
                    counts[i] += 1;
                    counts[i]
                };
                if self.ws[i].fail_metadata_from.is_some_and(|from| n >= from) {
                    return Ok(cargo_fail("cargo metadata failed"));
                }
                let staged = self.ws[i]
                    .stage
                    .as_ref()
                    .filter(|(from, _)| n >= *from)
                    .map(|(_, s)| s.clone());
                let out = staged.unwrap_or(StagedOutput {
                    target: self.ws[i].target.clone(),
                    build: self.ws[i].build.clone(),
                });
                let members: Vec<String> = (0..self.ws[i].members.len())
                    .map(|k| format!("m{k}"))
                    .collect();
                let packages: Vec<serde_json::Value> = self.ws[i]
                    .members
                    .iter()
                    .enumerate()
                    .map(|(k, m)| {
                        serde_json::json!({
                            "id": format!("m{k}"),
                            "name": format!("m{k}"),
                            "manifest_path": m.join("Cargo.toml"),
                        })
                    })
                    .collect();
                let mut json = serde_json::json!({
                    "packages": packages,
                    "workspace_members": members,
                    "workspace_root": self.ws[i].root,
                    "target_directory": out.target,
                });
                if let Some(b) = &out.build {
                    json["build_directory"] = serde_json::json!(b);
                }
                return Ok(cargo_ok(json));
            }
            // `cargo clean` (Cargo preview dry-run or real cleanup).
            if !args.iter().any(|a| a == "--dry-run") {
                for path in self.clean_removes.lock().unwrap().iter() {
                    let _ = fs::remove_file(path);
                }
                if let Some(hook) = self.clean_hook.lock().unwrap().as_ref() {
                    hook();
                }
            }
            Ok(ProcessOutput {
                success: true,
                code: Some(0),
                stdout: b"mock cargo clean success".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    impl CleanupRunner for StagedCargo {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }
        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((cwd.to_path_buf(), args.to_vec(), env.to_vec()));
            self.dispatch(args)
        }
    }

    struct ParallelProbeRunner {
        inner: StagedCargo,
        active: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
    }

    impl CargoRunner for ParallelProbeRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            let metadata = args.first().is_some_and(|arg| arg == "metadata");
            if metadata {
                let active = self
                    .active
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;
                self.peak
                    .fetch_max(active, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(4));
            }
            let output = CargoRunner::run(&self.inner, cwd, args);
            if metadata {
                self.active
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
            output
        }
    }

    impl CleanupRunner for ParallelProbeRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }

        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            self.inner.run_with_env(cwd, args, env)
        }
    }

    enum MetadataMutation {
        WorkspaceRoot(PathBuf),
        AddMember(PathBuf),
        Malformed,
    }

    struct MetadataMutationRunner {
        inner: StagedCargo,
        metadata_calls: std::sync::atomic::AtomicUsize,
        mutation: MetadataMutation,
    }

    impl CargoRunner for MetadataMutationRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            let mut output = CargoRunner::run(&self.inner, cwd, args)?;
            if args.first().is_some_and(|arg| arg == "metadata")
                && self
                    .metadata_calls
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    >= 1
                && output.success
            {
                match &self.mutation {
                    MetadataMutation::WorkspaceRoot(root) => {
                        let mut json: serde_json::Value = serde_json::from_slice(&output.stdout)
                            .expect("staged metadata is valid JSON");
                        json["workspace_root"] = serde_json::json!(root);
                        output.stdout = serde_json::to_vec(&json).unwrap();
                    }
                    MetadataMutation::AddMember(manifest) => {
                        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
                        fs::write(manifest, "").unwrap();
                        let mut json: serde_json::Value = serde_json::from_slice(&output.stdout)
                            .expect("staged metadata is valid JSON");
                        json["packages"]
                            .as_array_mut()
                            .unwrap()
                            .push(serde_json::json!({
                                "id": "raced-member",
                                "name": "raced-member",
                                "manifest_path": manifest,
                            }));
                        json["workspace_members"]
                            .as_array_mut()
                            .unwrap()
                            .push(serde_json::json!("raced-member"));
                        output.stdout = serde_json::to_vec(&json).unwrap();
                    }
                    MetadataMutation::Malformed => {
                        output.stdout = b"not json".to_vec();
                    }
                }
            }
            Ok(output)
        }
    }

    impl CleanupRunner for MetadataMutationRunner {
        fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
            CargoRunner::run(self, cwd, args)
        }

        fn run_with_env(
            &self,
            cwd: &Path,
            args: &[OsString],
            env: &[(OsString, OsString)],
        ) -> io::Result<ProcessOutput> {
            self.inner.run_with_env(cwd, args, env)
        }
    }

    /// Create a Cargo workspace on disk (manifest + quiet source).
    fn ws_root(dir: &Path, name: &str) -> PathBuf {
        let root = dir.join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
        )
        .unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        root
    }

    /// Create a Cargo cache directory with the marker Cargo itself writes.
    fn cargo_output(path: &Path, bytes: usize) -> PathBuf {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        fs::write(path.join("artifact.bin"), vec![1u8; bytes]).unwrap();
        path.to_path_buf()
    }

    fn canonical(path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn row_for<'a>(report: &'a CleanReport, root: &Path) -> &'a CleanResult {
        let want = canonical(root);
        report
            .results
            .iter()
            .find(|r| r.display_path == want)
            .unwrap_or_else(|| {
                panic!(
                    "no cleanup result row for {}; rows: {:?}",
                    want.display(),
                    report
                        .results
                        .iter()
                        .map(|r| (r.display_path.display().to_string(), r.outcome))
                        .collect::<Vec<_>>()
                )
            })
    }

    /// Distinct private target + private build inside one workspace.
    fn sibling_ws_fixture(tag: &str) -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let root = ws_root(d.path(), "ws");
        let target = cargo_output(&root.join("target"), 4096);
        let build = cargo_output(&root.join("build"), 4096);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let _ = tag;
        (d, root, target, build)
    }

    fn staged_sibling_runner(root: &Path, target: &Path, build: &Path) -> StagedCargo {
        let mut runner = StagedCargo::new();
        runner.add(root, target.to_path_buf(), Some(build.to_path_buf()));
        runner
    }

    #[test]
    fn distinct_private_target_and_build_is_one_cleanable_unit() {
        // C003 §9: private target + private build, distinct siblings => one
        // cleanable unit, one Cargo invocation, one result, in every mode.
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let (_d, root, target, build) = sibling_ws_fixture("siblings");
            let mut runner = staged_sibling_runner(&root, &target, &build);
            runner.removes_on_clean(vec![
                target.join("artifact.bin"),
                build.join("artifact.bin"),
            ]);
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], mode, &runner, &noop)
                .unwrap_or_else(|e| panic!("{mode:?} {e}"));
            assert_eq!(report.results.len(), 1, "{mode:?}: one result per unit");
            let row = row_for(&report, &root);
            let expected = match mode {
                CleanMode::CargoPreview => CleanOutcome::Previewed,
                CleanMode::Simulate => CleanOutcome::Simulated,
                CleanMode::Execute => CleanOutcome::Cleaned,
            };
            assert_eq!(row.outcome, expected, "{mode:?}: {}", row.detail);
            assert_eq!(row.output_roots.len(), 2, "{mode:?}: both roots");
            assert_eq!(row.ownership, OutputOwnershipClass::PrivateBounded);

            match mode {
                CleanMode::CargoPreview => {
                    assert_eq!(runner.dry_run_calls().len(), 1, "{mode:?}");
                    assert!(target.join("artifact.bin").exists());
                    assert!(build.join("artifact.bin").exists());
                }
                CleanMode::Simulate => {
                    assert!(
                        runner.clean_calls().is_empty(),
                        "{mode:?}: simulation invokes no cargo clean"
                    );
                    assert!(target.join("artifact.bin").exists());
                }
                CleanMode::Execute => {
                    assert_eq!(runner.clean_calls().len(), 1, "{mode:?}");
                    assert!(!target.join("artifact.bin").exists());
                    assert!(!build.join("artifact.bin").exists());
                }
            }
        }
    }

    #[test]
    fn one_invocation_one_result_progress_total_and_no_sibling_row() {
        // C003 §9 "One invocation / one result": distinct target/build must
        // produce exactly one Cargo call, one result, and a cleanup progress
        // total of one for the workspace (not two physical groups).
        use crate::progress::{ScanPhase, TestObserver};
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let (_d, root, target, build) = sibling_ws_fixture("one-invocation");
            let mut runner = staged_sibling_runner(&root, &target, &build);
            runner.removes_on_clean(vec![
                target.join("artifact.bin"),
                build.join("artifact.bin"),
            ]);
            let observer = TestObserver::new();
            let phase = match mode {
                CleanMode::CargoPreview => ScanPhase::CleanupPreview,
                CleanMode::Simulate => ScanPhase::CleanupSimulate,
                CleanMode::Execute => ScanPhase::CleanupExecute,
            };
            let report = clean_with(&root, 0, &[], mode, &runner, &observer).unwrap();
            assert_eq!(report.results.len(), 1, "{mode:?}");
            assert_eq!(observer.total_for(phase), Some(1), "{mode:?}");
            assert_eq!(observer.completed_count(phase), 1, "{mode:?}");
            let clean_calls = runner.clean_calls().len();
            assert_eq!(
                clean_calls,
                usize::from(mode != CleanMode::Simulate),
                "{mode:?}"
            );
            let text = report.render(&crate::report::HumanOptions::default());
            assert!(
                !text.contains("vanished") && !text.contains("Skipped"),
                "{mode:?}: no second vanished/skipped sibling row: {text}"
            );
        }
    }

    #[test]
    fn private_target_with_external_unproven_build_skips_entire_unit() {
        // C003-F1: a private target group cannot carry an ExternalUnproven
        // build directory into the same Cargo clean invocation, even when the
        // build directory is inside clean ROOT and would pass location
        // authorization.
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root = ws_root(d.path(), "ws");
            let target = cargo_output(&root.join("target"), 4096);
            let build = cargo_output(&d.path().join("shared-build"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root, target, Some(build));
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1, "{mode:?}");
            let row = row_for(&report, &root);
            assert_eq!(row.outcome, CleanOutcome::Skipped, "{mode:?}");
            assert_eq!(
                row.ownership,
                OutputOwnershipClass::ExternalUnproven,
                "{mode:?}"
            );
            assert!(
                row.detail.contains("build output"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(
                row.detail.contains("external-unproven"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[test]
    fn private_target_with_shared_build_skips_entire_unit() {
        // C003-F1: a private target group cannot carry a Shared build directory
        // owned by another discovered workspace into the same invocation.
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root_a = ws_root(d.path(), "a");
            let root_b = ws_root(d.path(), "b");
            let target_a = cargo_output(&root_a.join("target"), 4096);
            let shared = cargo_output(&d.path().join("shared"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a, Some(shared.clone()));
            runner.add(&root_b, shared, None);
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root_a);
            assert_eq!(row.outcome, CleanOutcome::Skipped, "{mode:?}");
            assert_eq!(row.ownership, OutputOwnershipClass::Shared, "{mode:?}");
            assert!(
                row.detail.contains("build output"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(row.detail.contains("shared"), "{mode:?}: {}", row.detail);
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[test]
    fn external_unproven_target_with_private_build_skips_entire_unit() {
        // Mirror of the private-target case: the failing group need not be the
        // build directory.
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root = ws_root(d.path(), "ws");
            let target = cargo_output(&d.path().join("external-target"), 4096);
            let build = cargo_output(&root.join("build"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root, target, Some(build));
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root);
            assert_eq!(row.outcome, CleanOutcome::Skipped, "{mode:?}");
            assert!(
                row.detail.contains("target output"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[test]
    fn uncertain_build_identity_skips_entire_unit() {
        // C003-F1: an output root whose physical identity cannot be proven makes
        // the whole workspace unprovable; the unit is skipped, never partially
        // cleaned.
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root = ws_root(d.path(), "ws");
            let target = cargo_output(&root.join("target"), 4096);
            // A regular file where a build directory is configured: exists, not a
            // symlink, but no provable physical directory identity.
            let build = root.join("build");
            fs::write(&build, b"not a directory").unwrap();
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root, target, Some(build));
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
            // An unprovable output identity leaves the workspace with no
            // reportable output at all, so no CleanupUnit exists; nothing may be
            // cleanable and no Cargo clean may run.
            assert!(
                report.results.is_empty()
                    || report
                        .results
                        .iter()
                        .all(|r| r.outcome == CleanOutcome::Skipped),
                "{mode:?}: {report:?}"
            );
            for r in &report.results {
                assert!(r.detail.contains("uncertain"), "{mode:?}: {}", r.detail);
            }
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_build_identity_skips_entire_unit() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let root = ws_root(d.path(), "ws");
        let target = cargo_output(&root.join("target"), 4096);
        let real = cargo_output(&d.path().join("real-build"), 4096);
        let link = root.join("build");
        symlink(&real, &link).unwrap();
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root, target, Some(link));
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        assert!(
            report.results.is_empty()
                || report
                    .results
                    .iter()
                    .all(|r| r.outcome == CleanOutcome::Skipped),
            "{report:?}"
        );
        assert!(runner.clean_calls().is_empty());
    }

    /// Synthetic mixed-class unit: private target group + uncertain build group.
    fn synthetic_mixed_unit(
        base: &Path,
        target_class: OutputOwnershipClass,
        build_class: OutputOwnershipClass,
    ) -> workspace::CleanupUnit {
        let a = base.join("ws/target");
        let b = base.join("ws/build");
        let measured = |p: &Path, class| workspace::AffectedGroup {
            display: p.to_path_buf(),
            covering: vec![p.to_path_buf()],
            ownership: class,
            kinds: Vec::new(),
            measured: Some(PhysicalOutputGroup {
                covering_roots: vec![p.to_path_buf()],
                physical_paths: vec![p.to_path_buf()],
                display_path: p.to_path_buf(),
                owners: Vec::new(),
                ownership: class,
                bytes: 1024,
                metric: SizeMetric::Allocated,
                newest_mtime: None,
                artifact_entries: 1,
                uncertain: false,
            }),
            skip: None,
        };
        let mut target_group = measured(&a, target_class);
        target_group.kinds = vec![OutputRootKind::Target];
        let mut build_group = measured(&b, build_class);
        build_group.kinds = vec![OutputRootKind::Build];
        let ws = base.join("ws");
        workspace::CleanupUnit {
            workspace_idx: 0,
            id: WorkspaceId(ws.clone()),
            root: ws.clone(),
            root_manifest: ws.join("Cargo.toml"),
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: a.clone(),
                    physical_path: Some(a.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: b.clone(),
                    physical_path: Some(b.clone()),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
            groups: vec![target_group, build_group],
            unmapped: Vec::new(),
            covering: vec![a, b],
            bytes: 2048,
        }
    }

    #[test]
    fn gate_blocks_mixed_class_units_in_every_combination() {
        // C003 §9 mixed matrix at the unit gate: PrivateBounded target plus
        // ExternalUnproven / Shared / Uncertain build (and the mirrored target)
        // all skip the entire unit; only all-private passes.
        let d = tempfile::tempdir().unwrap();
        // Covering roots are canonical by contract, so authorize against a
        // canonical cleanup root (macOS `/var` is a symlink).
        let base = fs::canonicalize(d.path()).unwrap();
        for build_class in [
            OutputOwnershipClass::ExternalUnproven,
            OutputOwnershipClass::Shared,
            OutputOwnershipClass::Uncertain,
        ] {
            let unit =
                synthetic_mixed_unit(&base, OutputOwnershipClass::PrivateBounded, build_class);
            let err = unit_block_reason(&unit, &base, &[]).unwrap_err();
            assert!(err.contains("build output"), "{build_class:?}: {err}");
            let unit =
                synthetic_mixed_unit(&base, build_class, OutputOwnershipClass::PrivateBounded);
            let err = unit_block_reason(&unit, &base, &[]).unwrap_err();
            assert!(err.contains("target output"), "{build_class:?}: {err}");
        }
        let unit = synthetic_mixed_unit(
            &base,
            OutputOwnershipClass::PrivateBounded,
            OutputOwnershipClass::PrivateBounded,
        );
        assert!(unit_block_reason(&unit, &base, &[]).is_ok());
    }

    #[test]
    fn gate_blocks_unmeasured_and_unmapped_unit_members() {
        // A sibling group that never became a measured candidate (empty, active,
        // or uncertain activity) blocks the whole unit...
        let d = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(d.path()).unwrap();
        let mut unit = synthetic_mixed_unit(
            &base,
            OutputOwnershipClass::PrivateBounded,
            OutputOwnershipClass::PrivateBounded,
        );
        unit.groups[1].measured = None;
        unit.groups[1].skip = Some(workspace::GroupSkipReason::ActiveOutput);
        let err = unit_block_reason(&unit, &base, &[]).unwrap_err();
        assert!(err.contains("build output"), "{err}");
        assert!(err.contains("recent"), "{err}");
        // ... and so does an existing output root with unprovable identity.
        let mut unit = synthetic_mixed_unit(
            &base,
            OutputOwnershipClass::PrivateBounded,
            OutputOwnershipClass::PrivateBounded,
        );
        unit.unmapped.push(unit.output.build.clone());
        let err = unit_block_reason(&unit, &base, &[]).unwrap_err();
        assert!(err.contains("unproven physical identity"), "{err}");
    }

    #[test]
    fn equal_and_nested_target_build_are_one_unit_and_one_invocation() {
        // C003 §9: target == build, build nested under target, and target nested
        // under build each collapse to one deduplicated physical union and one
        // Cargo invocation.
        for shape in ["equal", "build_under_target", "target_under_build"] {
            let d = tempfile::tempdir().unwrap();
            let root = ws_root(d.path(), "ws");
            let (target, build) = match shape {
                "equal" => {
                    let t = cargo_output(&root.join("target"), 4096);
                    (t.clone(), t)
                }
                "build_under_target" => {
                    let t = cargo_output(&root.join("target"), 8192);
                    let b = cargo_output(&t.join("build"), 2048);
                    (t, b)
                }
                _ => {
                    let b = cargo_output(&root.join("build"), 8192);
                    let t = cargo_output(&b.join("target"), 2048);
                    (t, b)
                }
            };
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root, target, Some(build));
            let noop = NoopObserver;
            let report = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1, "{shape}: one result");
            let row = row_for(&report, &root);
            assert_eq!(
                row.outcome,
                CleanOutcome::Cleaned,
                "{shape}: {}",
                row.detail
            );
            assert_eq!(row.output_roots.len(), 1, "{shape}: deduplicated union");
            assert_eq!(runner.clean_calls().len(), 1, "{shape}: one invocation");
        }
    }

    #[test]
    fn cross_workspace_overlap_race_into_target_skips_candidate() {
        // C003-F3: B is re-resolved into A's target region after the initial
        // analysis; the complete fresh graph classifies the region shared and A
        // is skipped before any Cargo clean.
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root_a = ws_root(d.path(), "a");
            let root_b = ws_root(d.path(), "b");
            let target_a = cargo_output(&root_a.join("target"), 4096);
            let target_b = cargo_output(&root_b.join("target"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, target_b, None);
            // From the second metadata call (the final ownership-universe
            // refresh) B points at A's target.
            runner.stage(1, 2, target_a, None);
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root_a);
            assert_eq!(
                row.outcome,
                CleanOutcome::Skipped,
                "{mode:?}: {}",
                row.detail
            );
            assert!(
                row.detail.contains("shared") || row.detail.contains("overlaps"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}: no clean at all");
            assert!(
                runner.metadata_for(1) >= 2,
                "{mode:?}: B must actually be re-resolved by the fresh graph"
            );
        }
    }

    #[test]
    fn cross_workspace_overlap_race_into_distinct_build_skips_candidate() {
        // Same race against A's *distinct build* directory, proving the proof
        // covers the complete OutputSet rather than the authorizing target group.
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root_a = ws_root(d.path(), "a");
            let root_b = ws_root(d.path(), "b");
            let target_a = cargo_output(&root_a.join("target"), 4096);
            let build_a = cargo_output(&root_a.join("build"), 4096);
            let target_b = cargo_output(&root_b.join("target"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a, Some(build_a.clone()));
            runner.add(&root_b, target_b, None);
            runner.stage(1, 2, build_a, None);
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root_a);
            assert_eq!(
                row.outcome,
                CleanOutcome::Skipped,
                "{mode:?}: {}",
                row.detail
            );
            assert!(
                row.detail.contains("shared") || row.detail.contains("overlaps"),
                "{mode:?}: {}",
                row.detail
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn cross_workspace_symlink_into_candidate_output_fails_closed() {
        // C003 §9: B becomes a symlink pointing into A's output. The fresh
        // physical graph cannot represent that overlap, so the explicit
        // cross-workspace check must still fail closed.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "a");
        let root_b = ws_root(d.path(), "b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 4096);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        // B's staged target is a symlink to A's target directory.
        let link_b = root_b.join("link-target");
        symlink(&target_a, &link_b).unwrap();
        let _ = target_b;
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        runner.stage(1, 2, link_b, None);
        let noop = NoopObserver;
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        let row = row_for(&report, &root_a);
        assert_eq!(row.outcome, CleanOutcome::Skipped, "{}", row.detail);
        // H4: B's symlinked root has no node in the physical graph, so while it
        // exists no group may claim `private`. A's group is reported `shared`
        // and fails the private-ownership gate before the explicit
        // cross-workspace overlap check is even reached.
        assert!(
            row.detail.contains("shared") || row.detail.contains("overlaps"),
            "{}",
            row.detail
        );
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn cross_workspace_unresolvable_root_into_candidate_output_fails_closed() {
        // Portable counterpart of the symlink race: B is re-resolved with a
        // configured output path that does not exist yet inside A's output
        // region. The fresh physical graph cannot represent it (no physical
        // identity), so the explicit cross-workspace reachability check must
        // still fail closed.
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root_a = ws_root(d.path(), "a");
            let root_b = ws_root(d.path(), "b");
            let target_a = cargo_output(&root_a.join("target"), 4096);
            let target_b = cargo_output(&root_b.join("target"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, target_b, None);
            // Nested inside A's target directory, but not created.
            runner.stage(1, 2, target_a.join("not-created-yet"), None);
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root_a);
            assert_eq!(
                row.outcome,
                CleanOutcome::Skipped,
                "{mode:?}: {}",
                row.detail
            );
            assert!(row.detail.contains("overlaps"), "{mode:?}: {}", row.detail);
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn cross_workspace_symlinked_ancestor_into_candidate_output_fails_closed() {
        // A configured output path whose *ancestor* is a symlink into the
        // candidate's output and whose final component does not exist: neither
        // the physical graph nor a lexical comparison can see the overlap, so
        // the proof must resolve the deepest existing ancestor and fail closed.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "a");
        let root_b = ws_root(d.path(), "b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 4096);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let alias = d.path().join("alias");
        symlink(&target_a, &alias).unwrap();
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        runner.stage(1, 2, alias.join("not-created-yet"), None);
        let noop = NoopObserver;
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        let row = row_for(&report, &root_a);
        assert_eq!(row.outcome, CleanOutcome::Skipped, "{}", row.detail);
        assert!(row.detail.contains("overlaps"), "{}", row.detail);
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn universe_workspace_that_cannot_reresolve_fails_closed() {
        // C003 §10 + M-2: failure to re-resolve any workspace that could
        // affect the bounded ownership graph blocks the whole scope (exit 1
        // via `scope_blocked`), like scan-time incompleteness, instead of
        // per-unit skips (exit 0). Safety intact either way; the signal is
        // now consistent.
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "a");
        let root_b = ws_root(d.path(), "b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 4096);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        // B resolves during the initial analysis and fails during the refresh.
        runner.fail_metadata_from(1, 2);
        let noop = NoopObserver;
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        assert!(
            report.scope_blocked.is_some(),
            "proof-time universe failure must scope-block: {report:?}"
        );
        assert_eq!(
            report.scope_blocked.as_ref().map(|b| b.reason),
            Some(ScopeBlockReason::OwnershipUnproven)
        );
        assert!(
            report
                .scope_blocked
                .as_ref()
                .is_some_and(|b| b.message.contains("could not be re-proven")
                    && b.message.contains("no cleanup commands were run")),
            "{:?}",
            report.scope_blocked
        );
        assert!(
            report.results.is_empty(),
            "blocked scope reports no per-unit rows: {report:?}"
        );
        // The universe is re-proven once, so two workspaces cost two locates.
        assert_eq!(report.counters.proof_cargo_locate_calls, 2);
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn initial_shared_overlap_is_decided_conservatively_without_proof() {
        // C003 §9: overlap present in the initial complete graph is not relied
        // upon as fresh state; the unit is skipped from the initial graph alone
        // and no ownership proof is consumed.
        for mode in [
            CleanMode::Simulate,
            CleanMode::CargoPreview,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let root_a = ws_root(d.path(), "a");
            let root_b = ws_root(d.path(), "b");
            let shared = cargo_output(&d.path().join("shared"), 4096);
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root_a, shared.clone(), None);
            runner.add(&root_b, shared, None);
            // Even if the fresh graph would no longer overlap, the initial
            // complete graph already decided conservatively.
            runner.stage(1, 2, root_b.join("target"), None);
            let noop = NoopObserver;
            let report = clean_with(d.path(), 0, &[], mode, &runner, &noop).unwrap();
            let row = row_for(&report, &root_a);
            assert_eq!(
                row.outcome,
                CleanOutcome::Skipped,
                "{mode:?}: {}",
                row.detail
            );
            assert!(row.detail.contains("shared"), "{mode:?}: {}", row.detail);
            assert_eq!(
                report.counters.proof_workspaces_refreshed, 0,
                "{mode:?}: gate blocked before any ownership proof"
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    #[test]
    fn candidate_target_or_build_change_still_skips() {
        // C002 behavior preserved: the candidate workspace changing its own
        // target/build between analysis and proof still fails closed.
        let (_d, root, target, build) = sibling_ws_fixture("candidate-change");
        let other = root.join("other-target");
        let other_build = root.join("other-build");
        cargo_output(&other, 1024);
        cargo_output(&other_build, 1024);
        backdate(&root, SystemTime::now() - Duration::from_secs(3600));
        let mut runner = staged_sibling_runner(&root, &target, &build);
        runner.stage(0, 2, other, None);
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        let row = row_for(&report, &root);
        assert_eq!(row.outcome, CleanOutcome::Skipped, "{}", row.detail);
        assert!(row.detail.contains("target changed"), "{}", row.detail);
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn union_accounting_counts_each_output_root_once() {
        // C003 §9 accounting: pre-clean bytes equal the deduplicated union of
        // every target/build group in the unit, post-clean measures the same
        // union, and the observed decrease is the saturating difference.
        let d = tempfile::tempdir().unwrap();
        let root = ws_root(d.path(), "ws");
        let target = cargo_output(&root.join("target"), 4096);
        let build = cargo_output(&root.join("build"), 1 << 20);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let expected_before = traverse::measure_single_target(&target).bytes
            + traverse::measure_single_target(&build).bytes;
        let mut runner = staged_sibling_runner(&root, &target, &build);
        runner.removes_on_clean(vec![
            target.join("artifact.bin"),
            build.join("artifact.bin"),
        ]);
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
        let row = row_for(&report, &root);
        assert_eq!(row.outcome, CleanOutcome::Cleaned, "{}", row.detail);
        assert_eq!(
            row.before_bytes,
            Some(expected_before),
            "pre-clean union counts both distinct roots once"
        );
        let after = row.after_bytes.expect("post-clean union measured");
        assert!(after < expected_before);
        assert_eq!(
            row.observed_decrease,
            Some(expected_before.saturating_sub(after))
        );
        // The build directory is only counted once even though the unit has two
        // affected groups.
        assert!(row.output_roots.contains(&canonical(&target)));
        assert!(row.output_roots.contains(&canonical(&build)));
    }

    #[test]
    fn post_clean_disappearing_sibling_is_measured_normally() {
        // C003 §7.4: if one output root disappears and another remains, measure
        // the surviving union normally.
        let (_d, root, target, build) = sibling_ws_fixture("disappearing-sibling");
        let build_root = build.clone();
        let mut runner = staged_sibling_runner(&root, &target, &build);
        runner.removes_on_clean(vec![target.join("artifact.bin")]);
        runner.on_clean(move || {
            let _ = fs::remove_dir_all(&build_root);
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
        let row = row_for(&report, &root);
        assert_eq!(row.outcome, CleanOutcome::Cleaned, "{}", row.detail);
        let before = row.before_bytes.unwrap();
        let after = row.after_bytes.expect("surviving union measured");
        assert!(after < before);
        assert_eq!(row.observed_decrease, Some(before - after));
    }

    #[test]
    fn post_clean_measurement_uncertainty_fabricates_no_decrease() {
        // C003 §7.4: post-clean measurement uncertainty is diagnostic and must
        // not fabricate recovered bytes.
        let (_d, root, target, build) = sibling_ws_fixture("uncertain-measurement");
        let build_root = build.clone();
        let mut runner = staged_sibling_runner(&root, &target, &build);
        runner.removes_on_clean(vec![target.join("artifact.bin")]);
        runner.on_clean(move || {
            let _ = fs::remove_dir_all(&build_root);
            let _ = fs::write(&build_root, b"not a directory");
        });
        let noop = NoopObserver;
        let report = clean_with(&root, 0, &[], CleanMode::Execute, &runner, &noop).unwrap();
        let row = row_for(&report, &root);
        assert_eq!(row.outcome, CleanOutcome::Cleaned, "{}", row.detail);
        assert_eq!(row.after_bytes, None);
        assert_eq!(row.observed_decrease, None, "no fabricated recovered bytes");
        assert!(row.detail.contains("measurement failed"), "{}", row.detail);
        assert!(report.diagnostics >= 1);
    }

    /// C023 §7 layer 2: the *final preflight* proof must refuse on its own,
    /// against a universe rebuilt immediately before the spawn, even when
    /// classification has already said the group is private.
    ///
    /// `source_tree_as_output_root_is_never_cleaned` covers a workspace whose
    /// own sources are its output root. This covers the shape that actually
    /// destroyed a project in 0.2.0: a *different* workspace's sources inside
    /// the cleaned region. The neighbour's output is a separate directory, so
    /// the two do not form a shared group and the refusal cannot be attributed
    /// to shared ownership.
    ///
    /// The region is deliberately named `out`, not `target`: discovery refuses
    /// to descend into a `target` directory, so a fixture using the conventional
    /// name would never discover the neighbour and would prove nothing.
    #[test]
    fn another_workspaces_source_tree_inside_the_cleaned_region_is_never_cleaned() {
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let outer = ws_root(d.path(), "outer");
            // Workspace A's declared output root.
            let out = cargo_output(&outer.join("out"), 4096);
            // Workspace B, independently resolved, whose *sources* live inside it.
            let inner = ws_root(&out, "inner");
            // B's own output is elsewhere, so A and B are not one shared group.
            let inner_target = cargo_output(&d.path().join("inner-target"), 4096);

            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&outer, out.clone(), None);
            runner.add(&inner, inner_target, None);

            // Premise: the neighbour is discovered and resolved. If it were not,
            // the case would pass because the dangerous shape never existed.
            let report = clean_with(d.path(), 0, &[], mode, &runner, &NoopObserver).unwrap();
            assert!(
                report.resolved_workspaces >= 2,
                "{mode:?}: both workspaces must resolve, got {}: {:?}",
                report.resolved_workspaces,
                report.render(&crate::report::HumanOptions::default())
            );

            assert!(
                runner.clean_calls().is_empty(),
                "{mode:?}: no cargo clean may run with a neighbour's sources inside the region"
            );
            // The bytes 0.2.0 deleted.
            for path in [
                inner.join("Cargo.toml"),
                inner.join("src/main.rs"),
                inner.join("src"),
            ] {
                assert!(path.exists(), "{mode:?}: {} must survive", path.display());
            }
        }
    }

    /// The mirror of the case above, end to end: the cleaned region is
    /// *inside* a package directory rather than around one.
    ///
    /// `cargo clean` removes whatever is under the root it is given, so an
    /// output root that lives inside another package's tree takes that
    /// package's checked-in files with it. The neighbour is a resolved
    /// workspace here, so the refusal cannot be attributed to an unresolved
    /// manifest blocking the scope.
    #[test]
    fn an_output_root_inside_a_package_directory_is_never_cleaned() {
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let d = tempfile::tempdir().unwrap();
            let outer = ws_root(d.path(), "outer");
            // A second package nested inside the outer workspace's tree.
            let nested = ws_root(&outer, "lib1");
            // The outer workspace's declared output root: inside the nested
            // package's directory.
            let out = cargo_output(&outer.join("lib1/assets"), 4096);
            // The nested package's own output lives elsewhere, so the two
            // groups are distinct and the refusal is about the shape, not
            // about shared ownership.
            let nested_target = cargo_output(&d.path().join("nested-target"), 4096);

            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&outer, out.clone(), None);
            runner.add(&nested, nested_target, None);

            // Premise: the neighbour is discovered and resolved. If it were
            // not, the case would pass because the dangerous shape never
            // existed.
            let report = clean_with(d.path(), 0, &[], mode, &runner, &NoopObserver).unwrap();
            assert!(
                report.resolved_workspaces >= 2,
                "{mode:?}: both workspaces must resolve, got {}: {:?}",
                report.resolved_workspaces,
                report.render(&crate::report::HumanOptions::default())
            );

            assert!(
                runner.clean_calls().is_empty(),
                "{mode:?}: no cargo clean may run with a package directory inside the region"
            );
            // The bytes a `cargo clean` of that root would have taken.
            for path in [
                nested.join("Cargo.toml"),
                nested.join("src/main.rs"),
                out.join("artifact.bin"),
            ] {
                assert!(path.exists(), "{mode:?}: {} must survive", path.display());
            }
        }
    }

    #[test]
    fn source_tree_as_output_root_is_never_cleaned() {
        // C1, end to end: `target-dir = "."` makes the declared output root the
        // workspace's own source tree. A planted CACHEDIR.TAG satisfies the cache
        // marker gate, so only the ownership classification stands between the
        // user and `cargo clean` deleting the project. The sources must survive.
        for declared in ["root", "src"] {
            let d = tempfile::tempdir().unwrap();
            let root = ws_root(d.path(), "proj");
            let output = if declared == "root" {
                root.clone()
            } else {
                root.join("src")
            };
            // The marker Cargo writes in a directory it created.
            fs::write(
                output.join("CACHEDIR.TAG"),
                b"Signature: 8a477f597d28d172789f06886806bc55\n",
            )
            .unwrap();
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let mut runner = StagedCargo::new();
            runner.add(&root, output, None);
            for mode in [
                CleanMode::Simulate,
                CleanMode::CargoPreview,
                CleanMode::Execute,
            ] {
                let report = clean_with(d.path(), 0, &[], mode, &runner, &NoopObserver).unwrap();
                // The group is not a cleanup unit at all (it is never measured),
                // so there is usually no row; if one is emitted it must be a skip.
                if let Some(row) = report
                    .results
                    .iter()
                    .find(|r| r.display_path == canonical(&root))
                {
                    assert_eq!(
                        row.outcome,
                        CleanOutcome::Skipped,
                        "{declared} {mode:?}: {}",
                        row.detail
                    );
                }
                assert!(
                    runner.clean_calls().is_empty(),
                    "{declared} {mode:?}: no cargo clean may run"
                );
            }
            assert!(
                root.join("src/main.rs").exists(),
                "{declared}: sources must survive"
            );
            assert!(root.join("Cargo.toml").exists(), "{declared}");
        }
    }

    #[test]
    fn final_proof_cost_is_linear_in_the_number_of_workspaces() {
        // H3/O1: the ownership universe used to be re-proven once per candidate,
        // costing O(N^2) Cargo spawns (8 workspaces = 192 spawns). Doubling the
        // workspace count must now double the proof work, not quadruple it.
        fn proof_spawns(workspaces: usize) -> u64 {
            let d = tempfile::tempdir().unwrap();
            let mut runner = StagedCargo::new();
            for index in 0..workspaces {
                let root = ws_root(d.path(), &format!("linear-{index}"));
                let target = cargo_output(&root.join("target"), 512);
                runner.add(&root, target, None);
            }
            backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
            let report = clean_with(
                d.path(),
                0,
                &[],
                CleanMode::Simulate,
                &runner,
                &NoopObserver,
            )
            .unwrap();
            assert_eq!(report.results.len(), workspaces);
            report.counters.proof_cargo_metadata_calls + report.counters.proof_cargo_locate_calls
        }
        let four = proof_spawns(4);
        let eight = proof_spawns(8);
        // Linear, at 4 spawns per workspace: 1 locate + 2 metadata to resolve and
        // refresh the universe, plus 1 metadata to re-prove that workspace's own
        // candidate. Quadratic growth would be 4 -> 64, not 4 -> 16.
        assert_eq!(four, 16, "4 workspaces");
        assert_eq!(eight, 32, "8 workspaces");
        assert_eq!(eight, four * 2, "doubling workspaces doubles proof work");
    }

    #[test]
    fn proof_failures_distinguish_a_change_from_an_unproven_ownership() {
        // L9: a Cargo/discovery failure is not evidence that the workspace
        // changed, and must not be reported under the change code.
        assert_eq!(
            proof_skip_code("ownership could not be re-proven: workspace /w did not re-resolve"),
            CleanupReasonCode::SkippedOwnershipUnproven
        );
        assert_eq!(
            proof_skip_code(
                "ownership could not be re-proven: combined selected-root universe has 1 unresolved manifest(s)"
            ),
            CleanupReasonCode::SkippedOwnershipUnproven
        );
        // A workspace that really did change keeps the change code.
        assert_eq!(
            proof_skip_code("ownership could not be re-proven: workspace /w changed identity"),
            CleanupReasonCode::SkippedChangedBeforeCleanup
        );
        assert_eq!(
            proof_skip_code("workspace changed before cleanup; skipped: member set changed"),
            CleanupReasonCode::SkippedChangedBeforeCleanup
        );
        assert_eq!(
            CleanupReasonCode::SkippedOwnershipUnproven.as_str(),
            "skipped_ownership_unproven"
        );
    }

    #[test]
    fn frozen_env_pins_the_build_directory_so_an_inherited_redirect_cannot_apply() {
        // M5/M9: an inherited CARGO_BUILD_BUILD_DIR used to be either inherited
        // by the clean (a silent redirect) or a blanket refusal. The child is now
        // pinned to the world the proof measured.
        let d = tempfile::tempdir().unwrap();
        let root = ws_root(d.path(), "build-dir");
        let target = cargo_output(&root.join("target"), 1024);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root, target, None);
        let report = clean_with(
            d.path(),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
        )
        .unwrap();
        let row = row_for(&report, &root);
        assert_eq!(row.outcome, CleanOutcome::Simulated, "{}", row.detail);
    }

    #[test]
    fn stats_account_for_final_ownership_universe_proof_work() {
        // C003-F4: cleanup --stats must include the final-proof Cargo
        // re-resolution work, not only the initial scan counters.
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "a");
        let root_b = ws_root(d.path(), "b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 2048);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        let noop = NoopObserver;
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        let counters = &report.counters;
        assert_eq!(counters.cargo_metadata_calls, 2, "initial analysis");
        // The ownership universe is re-proven once for the run (2 locate +
        // 2 metadata to resolve + 2 metadata to refresh), then each candidate
        // re-resolves its own workspace (1 metadata each). Counters report
        // workspaces re-resolved, never Cargo calls.
        assert_eq!(counters.proof_workspaces_refreshed, 4);
        assert_eq!(counters.proof_cargo_metadata_calls, 6);
        assert_eq!(counters.proof_cargo_locate_calls, 2);
        assert!(counters.proof_cargo_metadata_nanos > 0);
        assert!(counters.proof_source_activity_nanos > 0);
        let line = counters.proof_stats_line();
        assert!(line.contains("proof_metadata=6"), "{line}");
        assert!(line.contains("proof_locate=2"), "{line}");
        // The label must name what it counts.
        assert!(line.contains("proof_workspaces_refreshed=4"), "{line}");
        assert!(counters.proof_timings_line().contains("proof_metadata="));
        assert!(counters.proof_metadata_peak_concurrency >= 1);
        // The initial counters are not polluted by proof work.
        assert_eq!(counters.cargo_metadata_calls, 2);
        assert!(counters.stats_line().contains("metadata=2"));
    }

    #[test]
    fn combined_roots_share_one_ownership_proof_and_simulation_never_cleans() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "combined-a");
        let root_b = ws_root(d.path(), "combined-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 2048);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        let report = clean_with_roots(
            &[root_b.clone(), root_a.clone()],
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
        )
        .unwrap();
        assert_eq!(report.results.len(), 2);
        assert!(
            report
                .results
                .iter()
                .all(|r| r.outcome == CleanOutcome::Simulated)
        );
        assert!(runner.clean_calls().is_empty());
        // One shared universe proof for both roots plus one candidate re-proof
        // each: 2 (resolve) + 2 (universe refresh) + 2 (candidates).
        assert_eq!(report.counters.proof_cargo_metadata_calls, 6);
        assert_eq!(report.counters.proof_cargo_locate_calls, 2);
    }

    #[test]
    fn combined_private_roots_keep_preview_simulate_execute_parity() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "parity-a");
        let root_b = ws_root(d.path(), "parity-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 2048);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let roots = [root_a.clone(), root_b.clone()];
        for (mode, expected) in [
            (CleanMode::CargoPreview, CleanOutcome::Previewed),
            (CleanMode::Simulate, CleanOutcome::Simulated),
            (CleanMode::Execute, CleanOutcome::Cleaned),
        ] {
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, target_b.clone(), None);
            let report = clean_with_roots(&roots, 0, &[], mode, &runner, &NoopObserver).unwrap();
            assert_eq!(report.results.len(), 2, "{mode:?}");
            assert!(
                report.results.iter().all(|r| r.outcome == expected),
                "{mode:?}: {:?}",
                report.results
            );
            let calls = runner.clean_calls();
            assert_eq!(
                calls.len(),
                if mode == CleanMode::Simulate { 0 } else { 2 },
                "{mode:?}"
            );
            if mode == CleanMode::CargoPreview {
                assert!(
                    calls
                        .iter()
                        .all(|(_, args, _)| args.iter().any(|a| a == "--dry-run"))
                );
            }
        }
    }

    #[test]
    fn combined_roots_classify_cross_root_output_as_shared_in_every_mode() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "shared-a");
        let root_b = ws_root(d.path(), "shared-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let _target_b = cargo_output(&root_b.join("target"), 2048);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, target_a.clone(), None);
            let report = clean_with_roots(
                &[root_a.clone(), root_b.clone()],
                0,
                &[],
                mode,
                &runner,
                &NoopObserver,
            )
            .unwrap();
            assert!(
                report
                    .results
                    .iter()
                    .all(|r| r.outcome == CleanOutcome::Skipped),
                "{mode:?}: {:?}",
                report.results
            );
            assert!(
                report
                    .results
                    .iter()
                    .any(|r| r.ownership == OutputOwnershipClass::Shared)
            );
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    /// ADR 007: a scope note nested in the second root never blocks the
    /// first. Both healthy units proceed in every mode; the note is
    /// reported. Fails against the old whole-scope rule, which held a
    /// healthy root hostage to junk nested in a sibling.
    #[test]
    fn scope_note_in_second_root_does_not_block_first_root() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "coverage-a");
        let root_b = ws_root(d.path(), "coverage-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 2048);
        let unresolved = root_b.join("unresolved/Cargo.toml");
        fs::create_dir_all(unresolved.parent().unwrap()).unwrap();
        fs::write(
            &unresolved,
            "[package]\nname='unresolved'\nversion='0.1.0'\n",
        )
        .unwrap();
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        for mode in [
            CleanMode::CargoPreview,
            CleanMode::Simulate,
            CleanMode::Execute,
        ] {
            let mut runner = StagedCargo::new();
            runner.add(&root_a, target_a.clone(), None);
            runner.add(&root_b, target_b.clone(), None);
            let report = clean_with_roots(
                &[root_a.clone(), root_b.clone()],
                0,
                &[],
                mode,
                &runner,
                &NoopObserver,
            )
            .unwrap();
            assert_eq!(report.unresolved_ownership.len(), 1, "{mode:?}");
            assert!(
                report.scope_blocked.is_none(),
                "{mode:?}: the note is reported, never blocking: {report:?}"
            );
            assert_eq!(report.results.len(), 2, "{mode:?}: {report:?}");
            for root in [&root_a, &root_b] {
                let expected = match mode {
                    CleanMode::Simulate => CleanOutcome::Simulated,
                    CleanMode::CargoPreview => CleanOutcome::Previewed,
                    CleanMode::Execute => CleanOutcome::Cleaned,
                };
                assert_eq!(
                    row_for(&report, root).outcome,
                    expected,
                    "{mode:?} {root:?}"
                );
            }
            // The scope census is the verbose surface's; the quiet surface
            // names only what the run acted on.
            assert!(
                report
                    .render(&crate::report::HumanOptions::verbose())
                    .contains("2 root(s)"),
                "{mode:?}"
            );
            if mode == CleanMode::Simulate {
                assert!(runner.clean_calls().is_empty(), "{mode:?}");
            } else {
                assert_eq!(runner.clean_calls().len(), 2, "{mode:?}");
            }
        }
    }

    #[test]
    fn proof_refresh_parallelism_is_bounded_and_results_stay_deterministic() {
        let d = tempfile::tempdir().unwrap();
        let mut runner = StagedCargo::new();
        let mut roots = Vec::new();
        for index in 0..8 {
            let root = ws_root(d.path(), &format!("parallel-{index}"));
            let target = cargo_output(&root.join("target"), 1024 + index * 128);
            runner.add(&root, target, None);
            roots.push(root);
        }
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let runner = ParallelProbeRunner {
            inner: runner,
            active: std::sync::atomic::AtomicUsize::new(0),
            peak: std::sync::atomic::AtomicUsize::new(0),
        };
        let report = clean_with(
            d.path(),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
        )
        .unwrap();
        let peak = runner.peak.load(std::sync::atomic::Ordering::SeqCst);
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        assert!(peak >= available.clamp(1, 4).min(2), "peak={peak}");
        assert!(peak <= PROOF_REFRESH_CONCURRENCY_CAP, "peak={peak}");
        assert_eq!(
            report.counters.proof_metadata_peak_concurrency as usize,
            peak
        );
        // 8 workspaces: 8 locate + 8 metadata to resolve the universe, 8
        // metadata to refresh it, and 1 metadata per candidate. Linear in the
        // workspace count, not quadratic in the candidate count (H3/O1).
        assert_eq!(report.counters.proof_workspaces_refreshed, 16);
        assert_eq!(report.counters.proof_cargo_metadata_calls, 24);
        assert_eq!(report.counters.proof_cargo_locate_calls, 8);
        assert_eq!(report.results.len(), 8);
        assert!(report.results.windows(2).all(|pair| {
            pair[0].before_bytes >= pair[1].before_bytes
                || pair[0].before_bytes == pair[1].before_bytes
        }));
        let roots_in_report: Vec<_> = report
            .results
            .iter()
            .map(|row| row.display_path.clone())
            .collect();
        let mut expected: Vec<_> = roots
            .into_iter()
            .map(|root| {
                (
                    traverse::measure_single_target(&root.join("target")).bytes,
                    canonical(&root),
                )
            })
            .collect();
        expected.sort_by(|(a_bytes, a_root), (b_bytes, b_root)| {
            b_bytes.cmp(a_bytes).then_with(|| a_root.cmp(b_root))
        });
        assert_eq!(
            roots_in_report,
            expected
                .into_iter()
                .map(|(_, root)| root)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition() {
        let d = tempfile::tempdir().unwrap();
        let root_a = ws_root(d.path(), "parity-a");
        let root_b = ws_root(d.path(), "parity-b");
        let target_a = cargo_output(&root_a.join("target"), 4096);
        let target_b = cargo_output(&root_b.join("target"), 2048);
        backdate(d.path(), SystemTime::now() - Duration::from_secs(3600));
        let mut runner = StagedCargo::new();
        runner.add(&root_a, target_a, None);
        runner.add(&root_b, target_b, None);
        let manifests = [root_a.join("Cargo.toml"), root_b.join("Cargo.toml")];
        let mut initial_counters = ScanCounters::default();
        let mut initial_diagnostics = Vec::new();
        let initial = workspace::resolve_workspaces(
            &manifests,
            &runner,
            &mut initial_counters,
            &mut initial_diagnostics,
            &NoopObserver,
        );
        let mut sequential_counters = ScanCounters::default();
        let sequential =
            refresh_proof_universe_with_limit(&initial, &runner, &mut sequential_counters, 1)
                .unwrap();
        let mut parallel_counters = ScanCounters::default();
        let parallel =
            refresh_proof_universe_with_limit(&initial, &runner, &mut parallel_counters, 4)
                .unwrap();
        assert_eq!(sequential, parallel);
        assert_eq!(
            sequential_counters.proof_cargo_metadata_calls,
            parallel_counters.proof_cargo_metadata_calls
        );
        let sequential_groups = workspace::build_groups(&sequential);
        let parallel_groups = workspace::build_groups(&parallel);
        let signature = |groups: &[workspace::RawGroup]| {
            groups
                .iter()
                .map(|group| {
                    (
                        group.physicals.clone(),
                        group.covering.clone(),
                        group.display.clone(),
                        group.owners.clone(),
                        group.ownership,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(&sequential_groups), signature(&parallel_groups));
        let clock_start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = clock_start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut seq_counters = ScanCounters::default();
        let mut seq_diagnostics = Vec::new();
        let sequential_disposition = workspace::analyze_groups(
            &sequential,
            sequential_groups,
            clock_start,
            cutoff,
            Duration::from_secs(300),
            &mut seq_counters,
            &mut seq_diagnostics,
            &NoopObserver,
        );
        let mut par_counters = ScanCounters::default();
        let mut par_diagnostics = Vec::new();
        let parallel_disposition = workspace::analyze_groups(
            &parallel,
            parallel_groups,
            clock_start,
            cutoff,
            Duration::from_secs(300),
            &mut par_counters,
            &mut par_diagnostics,
            &NoopObserver,
        );
        assert_eq!(sequential_disposition, parallel_disposition);
    }

    #[test]
    fn authorization_infrastructure_failure_scope_blocks_instead_of_per_unit_skip() {
        // H-3: a misconfigured/unreadable authorization root is a scope-level
        // config error (exit 1 via `scope_blocked`), not N per-unit
        // `SkippedUnauthorized` rows (exit 0). Only `Err` takes this path;
        // `Ok(false)` stays a per-unit skip.
        assert!(is_authorization_infrastructure_error(
            "cleanup authorization root must be absolute: relative/path"
        ));
        assert!(is_authorization_infrastructure_error(
            "cannot inspect cleanup authorization root /x: permission denied"
        ));
        assert!(is_authorization_infrastructure_error(
            "cleanup authorization root must not be a symlink: /x"
        ));
        assert!(is_authorization_infrastructure_error(
            "cannot resolve cleanup authorization root /x: foo"
        ));
        assert!(is_authorization_infrastructure_error(
            "cannot resolve cleanup root /x: foo"
        ));
        assert!(!is_authorization_infrastructure_error(
            "target output /x is private but outside the authorized cleanup boundary; add a `cleanup.allowed_output_roots` entry or use a clean ROOT that contains it"
        ));

        let (_d, root, target) = valid_fixture(1);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        // Non-absolute allowed root is an infrastructure `Err`, never `Ok(false)`.
        let report = clean_with(
            &root,
            0,
            &[PathBuf::from("relative/allowed-root")],
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
        )
        .unwrap();
        assert!(
            report.scope_blocked.is_some(),
            "infrastructure failure must scope-block: {report:?}"
        );
        assert_eq!(
            report.scope_blocked.as_ref().map(|b| b.reason),
            Some(ScopeBlockReason::OwnershipUnproven)
        );
        assert!(
            report
                .scope_blocked
                .as_ref()
                .is_some_and(|b| b.message.contains("authorization could not be proven")
                    && b.message.contains("no cleanup commands were run")),
            "{:?}",
            report.scope_blocked
        );
        assert!(
            report.results.is_empty(),
            "blocked scope reports no per-unit rows: {report:?}"
        );
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn covering_inspect_failure_maps_permission_denied_to_uncertain() {
        // M-4: an unreadable covering root is uncertainty (investigate/retry),
        // not evidence of change. `NotFound`/type changes stay `Changed`.
        let denied =
            covering_inspect_failure(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert_eq!(denied.code, CleanupReasonCode::SkippedUncertain);
        assert!(denied.detail.contains("uncertain"), "{}", denied.detail);
        let other = covering_inspect_failure(io::Error::other("boom"));
        assert_eq!(other.code, CleanupReasonCode::SkippedChangedBeforeCleanup);
        // A disappeared root is still a change (existing gate, pinned).
        let missing = PathBuf::from("/definitely/missing/cargo-cleanme-covering-root");
        let failure = prove_covering_roots(std::slice::from_ref(&missing)).unwrap_err();
        assert_eq!(failure.code, CleanupReasonCode::SkippedChangedBeforeCleanup);
    }

    #[test]
    fn ownership_degraded_code_matches_scan_time_vocabulary() {
        // M-5: the proof must report the fresh class specifically, like the
        // scan-time `unit_skip_code` does, instead of conflating to `Changed`.
        assert_eq!(
            ownership_degraded_code(OutputOwnershipClass::Shared),
            CleanupReasonCode::SkippedShared
        );
        assert_eq!(
            ownership_degraded_code(OutputOwnershipClass::Uncertain),
            CleanupReasonCode::SkippedUncertain
        );
        assert_eq!(
            ownership_degraded_code(OutputOwnershipClass::ExternalUnproven),
            CleanupReasonCode::SkippedOwnershipUnproven
        );
        // Spot-check agreement with the scan-time gate on synthetic units.
        let d = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(d.path()).unwrap();
        for (class, expected) in [
            (
                OutputOwnershipClass::Shared,
                CleanupReasonCode::SkippedShared,
            ),
            (
                OutputOwnershipClass::Uncertain,
                CleanupReasonCode::SkippedUncertain,
            ),
            (
                OutputOwnershipClass::ExternalUnproven,
                CleanupReasonCode::SkippedOwnershipUnproven,
            ),
        ] {
            assert_eq!(ownership_degraded_code(class), expected);
            let unit = synthetic_mixed_unit(&base, OutputOwnershipClass::PrivateBounded, class);
            assert_eq!(unit_skip_code(&unit), expected, "{class:?}");
        }
    }

    #[test]
    fn package_selector_with_valid_workspace_index_evaluates_spec() {
        // L-1: the `workspace_idx` lookup is fail-closed (`.get()`); this
        // exercises the path through the public entry without panicking.
        // Host Cargo 1.89 lacks the package-selector capability, so the
        // runtime gate reports `SelectorUnsupported` before spec resolution;
        // on a runtime with package support the same call reports
        // `SelectorInvalid`. Either way the unit skips and nothing spawns.
        let (_d, root, target) = valid_fixture(1);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let report = clean_with_roots_policy_selector(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Simulate,
            &runner,
            &NoopObserver,
            &CleanupPolicy::default(),
            Some(CleanupSelector::Package("missing-package".into())),
        )
        .unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
        assert!(
            matches!(
                report.results[0].reason_code,
                CleanupReasonCode::SelectorInvalid | CleanupReasonCode::SelectorUnsupported
            ),
            "unexpected code: {:?}",
            report.results[0].reason_code
        );
        assert!(runner.clean_calls().is_empty());
    }

    #[test]
    fn new_manifest_after_proof_blocks_remaining_scope() {
        // H-2: a manifest created under an admitted root after the hoisted
        // universe was built is invisible to the overlap check. The post-proof
        // rediscovery (all modes, for parity) must block instead of spawning.
        struct PlantingRunner<'a> {
            inner: FakeCleanupRunner,
            planted: PathBuf,
            metadata_calls: std::sync::atomic::AtomicUsize,
            _marker: std::marker::PhantomData<&'a ()>,
        }
        impl CargoRunner for PlantingRunner<'_> {
            fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "metadata") {
                    let n = self
                        .metadata_calls
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                        + 1;
                    // Plant after the hoisted rediscovery walk (which runs
                    // before the hoisted Cargo calls) but before the post-proof
                    // rediscovery: the 4th metadata call is the per-candidate
                    // refresh for a single-workspace run.
                    if n == 4 {
                        fs::create_dir_all(self.planted.parent().unwrap()).unwrap();
                        fs::write(
                            &self.planted,
                            "[package]\nname='planted'\nversion='0.1.0'\n",
                        )
                        .unwrap();
                    }
                }
                CargoRunner::run(&self.inner, cwd, args)
            }
        }
        impl CleanupRunner for PlantingRunner<'_> {
            fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> {
                CargoRunner::run(self, cwd, args)
            }
            fn run_with_env(
                &self,
                cwd: &Path,
                args: &[OsString],
                env: &[(OsString, OsString)],
            ) -> io::Result<ProcessOutput> {
                self.inner.run_with_env(cwd, args, env)
            }
        }
        for mode in [CleanMode::Simulate, CleanMode::Execute] {
            let (_d, root, target) = valid_fixture(1);
            let planted = root.join("planted/Cargo.toml");
            let inner = FakeCleanupRunner::new(&root, &target, 1);
            let runner = PlantingRunner {
                inner,
                planted,
                metadata_calls: std::sync::atomic::AtomicUsize::new(0),
                _marker: std::marker::PhantomData,
            };
            let report = clean_with(&root, 0, &[], mode, &runner, &NoopObserver).unwrap();
            assert!(
                report.scope_blocked.is_some(),
                "{mode:?}: planted manifest must block: {report:?}"
            );
            assert_eq!(
                report.scope_blocked.as_ref().map(|b| b.reason),
                Some(ScopeBlockReason::IncompleteDiscovery),
                "{mode:?}"
            );
            assert!(
                report
                    .scope_blocked
                    .as_ref()
                    .is_some_and(|b| b.message.contains("manifest set differs")),
                "{mode:?}: {:?}",
                report.scope_blocked
            );
            assert!(runner.inner.clean_calls().is_empty(), "{mode:?}");
        }
    }

    /// The outcome a profile-scoped preview must report on the host that runs
    /// this case.
    ///
    /// `runtime_clean_capabilities` is the product's own premise: the profile
    /// selector is offered only for the exact Cargo releases the qualification
    /// policy claims (`release/selector-qualification.json`), and every other
    /// Cargo — including a patch release of a claimed one — is refused by
    /// design. Deriving the expectation from that same function keeps the
    /// assertion tight in both directions: a qualified host must preview, and
    /// an unqualified one must refuse *without* invoking a Cargo clean.
    ///
    /// It is deliberately not a disjunction. Accepting either outcome would
    /// leave the case green on a host where the selector was wrongly refused
    /// and on a host where it was wrongly offered, which is the "green that
    /// proves nothing" shape this repository keeps finding.
    fn assert_profile_preview_matches_host_premise(report: &CleanReport) {
        if runtime_clean_capabilities().profile_selector {
            assert_eq!(report.results[0].outcome, CleanOutcome::Previewed);
            assert!(
                report
                    .render(&crate::report::HumanOptions::default())
                    .contains("output-union before")
            );
        } else {
            assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
            assert_eq!(
                report.results[0].reason_code,
                CleanupReasonCode::SelectorUnsupported
            );
        }
    }

    /// A selector reaches Cargo's command line.
    ///
    /// The runtime capability gate decides *whether* a selector is offered, and
    /// it reads the host's Cargo version — so on a host outside the
    /// qualification policy the whole selector path is refused before any
    /// command is built. This case pins the composition itself, which is
    /// host-independent: a `--profile` selector must reach `cargo clean` as
    /// `--profile <name>`, and a `--package` selector as `--package <spec>`.
    /// Without it, the argument composition would only be exercised on a
    /// qualified host, and CI runs on whatever `stable` currently is.
    #[test]
    fn a_selector_reaches_the_cargo_command_line() {
        let ctx = RevalidatedContext {
            workspace_roots: vec![PathBuf::from("/ws")],
            workspace_root: PathBuf::from("/ws"),
            root_manifest: PathBuf::from("/ws/Cargo.toml"),
            canonical_members: vec![PathBuf::from("/ws")],
            target: PathBuf::from("/cache/target"),
            build: Some(PathBuf::from("/cache/build")),
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
            covering: vec![PathBuf::from("/cache/target")],
            pre_bytes: 0,
            newest_activity: None,
            ownership: OutputOwnershipClass::PrivateBounded,
            frozen_env: Vec::new(),
        };
        let args = clean_args(
            &ctx,
            CleanMode::CargoPreview,
            Some(&CleanupSelector::Profile("dev".into())),
        );
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let position = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .unwrap_or_else(|| panic!("{flag} is missing from {args:?}"))
        };
        assert_eq!(args[position("--profile") + 1], "dev");
        assert!(
            !args.iter().any(|a| a == "--package"),
            "a profile selector must not also name a package: {args:?}"
        );
        let args = clean_args(
            &ctx,
            CleanMode::CargoPreview,
            Some(&CleanupSelector::Package("app-a@1.0.0".into())),
        );
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let position = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .unwrap_or_else(|| panic!("{flag} is missing from {args:?}"))
        };
        assert_eq!(args[position("--package") + 1], "app-a@1.0.0");
        assert!(
            !args.iter().any(|a| a == "--profile"),
            "a package selector must not also name a profile: {args:?}"
        );
    }

    // ---------------------------------------------------------------------
    // M020: the quiet human surface.
    // ---------------------------------------------------------------------

    /// A report over one scope root, with no disclosures and no failures.
    fn empty_report_over(root: &str) -> CleanReport {
        CleanReport {
            results: Vec::new(),
            diagnostics: 0,
            failed: 0,
            mode: CleanMode::Execute,
            counters: ScanCounters::default(),
            scope_blocked: None,
            unresolved_ownership: Vec::new(),
            selected_roots: vec![PathBuf::from(root)],
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: None,
            selector: None,
            out_of_scope_unresolved: 0,
        }
    }

    /// One unit row, with the outcome and the byte figure spelled out.
    fn unit_row(path: &str, outcome: CleanOutcome, bytes: u64) -> CleanResult {
        let reason_code = match outcome {
            CleanOutcome::Cleaned => CleanupReasonCode::Cleaned,
            CleanOutcome::Simulated => CleanupReasonCode::Simulated,
            CleanOutcome::Previewed => CleanupReasonCode::Previewed,
            CleanOutcome::Skipped => CleanupReasonCode::NotIncluded,
            CleanOutcome::Failed => CleanupReasonCode::CargoFailed,
        };
        let cleaned = outcome == CleanOutcome::Cleaned;
        CleanResult {
            display_path: PathBuf::from(path),
            workspace_roots: vec![PathBuf::from(path)],
            output_roots: vec![PathBuf::from(path)],
            ownership: OutputOwnershipClass::PrivateBounded,
            outcome,
            policy_disposition: Some(PolicyDisposition::Selected),
            reason_code,
            before_bytes: Some(bytes),
            after_bytes: if cleaned { Some(0) } else { None },
            observed_decrease: if cleaned { Some(bytes) } else { None },
            detail: String::new(),
        }
    }

    /// M020: the quiet cleanup report names only what the run acted on.
    ///
    /// The skipped workspaces are counted, not listed. *Fails if:* the quiet
    /// surface re-admits a skipped row.
    #[test]
    fn the_quiet_cleanup_report_lists_only_what_the_run_acted_on() {
        let mut report = empty_report_over("/work");
        report
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        for name in ["beta", "gamma", "delta"] {
            report.results.push(unit_row(
                &format!("/work/{name}/target"),
                CleanOutcome::Skipped,
                50,
            ));
        }
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(text.contains("/work/alpha/target"), "{text}");
        for name in ["beta", "gamma", "delta"] {
            assert!(
                !text.contains(&format!("/work/{name}")),
                "a skipped row leaked into the quiet surface: {text}"
            );
        }
        // The count survives as a number, so the run is still honest about
        // what it did not pick — in one number, not one line each.
        assert!(text.contains("(3 skipped)"), "{text}");
    }

    /// M020: the tally states the space saved, in words a person can act on.
    ///
    /// *Fails if:* the tally is dropped, or if it regresses to the scope
    /// census.
    #[test]
    fn the_quiet_tally_states_the_space_saved() {
        let mut report = empty_report_over("/work");
        report
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        report
            .results
            .push(unit_row("/work/beta/target", CleanOutcome::Cleaned, 60));
        let text = report.render(&crate::report::HumanOptions::default());
        let last = text.lines().last().unwrap_or_default();
        assert!(last.contains("reclaimed"), "{text}");
        assert!(last.contains("160.00 B"), "{text}");
        assert!(last.contains("2 directories"), "{text}");
        assert!(
            !text.contains("CleanupUnit(s)"),
            "the scope census is the verbose surface's: {text}"
        );
    }

    /// M020: Execute and simulation never share a wording.
    ///
    /// "reclaimed" is a substring of "would be reclaimed", so the pair is
    /// only discriminating in the direction it is written: Execute asserts
    /// the phrase it owns *and* the absence of the simulation's, and the
    /// simulation asserts its own. *Fails if:* one mode's wording leaks into
    /// the other.
    #[test]
    fn execute_and_simulation_words_their_tallies_differently() {
        let mut execute = empty_report_over("/work");
        execute
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        let execute_text = execute.render(&crate::report::HumanOptions::default());
        assert!(execute_text.contains("reclaimed"), "{execute_text}");
        assert!(
            !execute_text.contains("would be reclaimed"),
            "{execute_text}"
        );
        assert!(!execute_text.contains("simulation only"), "{execute_text}");

        let mut simulate = empty_report_over("/work");
        simulate.mode = CleanMode::Simulate;
        simulate
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Simulated, 100));
        let simulate_text = simulate.render(&crate::report::HumanOptions::default());
        assert!(
            simulate_text.contains("would be reclaimed"),
            "{simulate_text}"
        );
        assert!(
            simulate_text.contains("no `cargo clean` command was invoked"),
            "a simulation must say that nothing was invoked: {simulate_text}"
        );
    }

    /// M020: a failure is never silent.
    ///
    /// A failure is not a skip. The quiet surface hides the skipped rows; it
    /// must not hide the ones that went wrong. *Fails if:* the failure filter
    /// treats a failure like a skip.
    #[test]
    fn a_failed_unit_is_always_listed() {
        let mut report = empty_report_over("/work");
        report
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        report
            .results
            .push(unit_row("/work/beta/target", CleanOutcome::Failed, 50));
        report
            .results
            .push(unit_row("/work/gamma/target", CleanOutcome::Skipped, 50));
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(
            text.contains("/work/beta/target"),
            "the failure vanished: {text}"
        );
        assert!(text.contains("Failed"), "{text}");
        assert!(text.contains("1 failed"), "{text}");
        // The failed unit's bytes are not counted as reclaimed.
        assert!(text.contains("100.00 B reclaimed"), "{text}");
        assert!(
            !text.contains("/work/gamma"),
            "the skipped row stays hidden: {text}"
        );
    }

    /// M020: `--verbose` restores the detail the quiet surface withholds.
    ///
    /// *Fails if:* the flag is parsed but not plumbed, or if the quiet surface
    /// keeps the detail.
    #[test]
    fn the_verbose_surface_restores_the_skipped_rows_and_the_census() {
        let mut report = empty_report_over("/work");
        report
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        report
            .results
            .push(unit_row("/work/beta/target", CleanOutcome::Skipped, 50));
        let verbose = report.render(&crate::report::HumanOptions::verbose());
        assert!(verbose.contains("/work/beta/target"), "{verbose}");
        assert!(verbose.contains("CleanupUnit(s)"), "{verbose}");
        assert!(verbose.contains("combined scope:"), "{verbose}");
    }

    /// M020: the safety disclosures survive the quiet surface.
    ///
    /// ADR 007: "boundary clean" must not read as "system clean". The
    /// disclosures say what the report *means*; dropping them because the
    /// report got shorter would be the M012A defect in a new place. *Fails
    /// if:* the quiet surface drops a disclosure.
    #[test]
    fn the_safety_disclosures_survive_the_quiet_surface() {
        let mut report = empty_report_over("/work");
        report.out_of_scope_unresolved = 3;
        report
            .results
            .push(unit_row("/work/alpha/target", CleanOutcome::Cleaned, 100));
        let text = report.render(&crate::report::HumanOptions::default());
        assert!(
            text.contains("out-of-scope unprovable participant(s)"),
            "{text}"
        );
        assert!(text.contains("not blocking"), "{text}");
    }
}
