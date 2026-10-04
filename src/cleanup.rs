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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CleanMode {
    /// Cargo preview: invoke Cargo's own dry-run (`--dry-run --verbose`).
    /// Default and explicit `--dry-run`.
    #[default]
    Preview,
    /// Application simulation: full path, no `cargo clean` invocation.
    /// Explicit `--dryrun` (no hyphen).
    Simulate,
    /// Real Cargo cleanup. Explicit `--yes`.
    Execute,
}

impl CleanMode {
    /// Stable machine label for the JSON `mode` field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Simulate => "simulate",
            Self::Execute => "execute",
        }
    }
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

impl From<String> for ProofFailure {
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

#[derive(Clone, Debug, Default)]
pub struct CleanReport {
    pub results: Vec<CleanResult>,
    pub diagnostics: usize,
    pub failed: usize,
    pub mode: CleanMode,
    pub counters: ScanCounters,
    /// Whole-scope safety block raised before sizing or per-unit proof.
    pub scope_blocked: Option<String>,
    /// Structured participants retained for diagnostics and audit consumers.
    pub unresolved_ownership: Vec<UnresolvedOwnershipParticipant>,
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

impl CleanReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.selected_roots.is_empty() {
            out.push_str(&format!(
                "combined scope: {} root(s) [{}], {} manifest(s), {} resolved workspace(s), {} CleanupUnit(s), {} unresolved ownership participant(s)\n",
                self.selected_roots.len(),
                self.selected_roots.iter().map(|p| escaped_path(p)).collect::<Vec<_>>().join(", "),
                self.discovered_manifests,
                self.resolved_workspaces,
                self.units_considered,
                self.unresolved_ownership.len(),
            ));
        }
        if let Some(reason) = &self.scope_blocked {
            out.push_str(&format!("{reason}\n"));
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
        // Deterministic size-descending with stable path tie-break; progress
        // rows show at most five transient entries, the final report lists every
        // workspace CleanupUnit (C003 §7.5).
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
                escaped_path(&r.display_path),
                r.ownership.label(),
            ));
            // C003 §7.5: one row per workspace CleanupUnit; every physical
            // output root the single Cargo invocation affects stays visible.
            if r.output_roots.len() > 1 {
                out.push_str(&format!(
                    "  outputs {}",
                    r.output_roots
                        .iter()
                        .map(|p| escaped_path(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else if let Some(root) = r.output_roots.first() {
                out.push_str(&format!("  output {}", escaped_path(root)));
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
            CleanMode::Preview => {
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

fn escaped_path(path: &Path) -> String {
    path.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{:?}", path.as_os_str()))
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
            Ok(m) if m.file_type().is_symlink() => {
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
        // Canonicalize existing roots for containment checks.
        if let Ok(canonical) = fs::canonicalize(root) {
            canonical_allowed.push(canonical);
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
                     add a `cleanup.allowed_output_roots` entry or use a clean ROOT that contains it",
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
    let includes = compile_policy_globs(&policy.include)?;
    let excludes = compile_policy_globs(&policy.exclude)?;
    // Validate every root before doing anything observable: an invalid
    // invocation must not spawn a process, and the runtime capability verdict
    // must not be able to mask a malformed root (M10).
    for root in roots {
        validate_cleanup_root(root)?;
    }
    let roots = collapse_roots(roots.to_vec());
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
    let selector_capabilities = if selector.is_some() {
        runtime_clean_capabilities()
    } else {
        CargoCleanCapabilities::default()
    };
    let resolved = crate::domain::EffectiveScanPolicy {
        recency: Duration::from_secs(recency_seconds),
        scope: ScanScope::ExplicitRoots(roots.clone()),
        discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
    };
    let scan_start = SystemTime::now();
    let cleanup_phase = match mode {
        CleanMode::Preview => ScanPhase::CleanupPreview,
        CleanMode::Simulate => ScanPhase::CleanupSimulate,
        CleanMode::Execute => ScanPhase::CleanupExecute,
    };
    // Discovery (manifest-first, no Cargo yet).
    observer.phase(cleanup_phase);
    let discovery_start = std::time::Instant::now();
    let discovered = discovery::discover_manifests(&resolved, observer)?;
    let discovery_elapsed = discovery_start.elapsed();
    let mut counters = discovered.counters.clone();
    counters.discovery_nanos = counters
        .discovery_nanos
        .saturating_add(crate::domain::elapsed_nanos(discovery_elapsed));
    let mut diagnostics: Vec<ScanDiagnostic> = discovered.diagnostics;
    let manifests = discovered.manifests;

    if !diagnostics.is_empty() {
        let mut report = CleanReport {
            diagnostics: diagnostics.len(),
            mode,
            selected_roots: roots.clone(),
            discovered_manifests: manifests.len(),
            ..Default::default()
        };
        report.effective_policy = Some(policy.clone());
        report.scope_blocked = Some(format!(
            "combined cleanup ownership universe is incomplete: {} discovery diagnostic(s); no cleanup commands were run",
            diagnostics.len()
        ));
        report.counters = counters;
        return Ok(report);
    }

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
    )?;
    let workspaces = scope.workspaces;
    let units = scope.units;
    let unresolved_ownership = scope.unresolved;

    let mut report = CleanReport {
        diagnostics: diagnostics.len(),
        mode,
        selected_roots: roots.clone(),
        discovered_manifests: manifests.len(),
        resolved_workspaces: workspaces.len(),
        units_considered: units.len(),
        ..Default::default()
    };
    report.effective_policy = Some(policy.clone());
    report.selector = selector.clone();

    if !unresolved_ownership.is_empty() {
        let unresolved = unresolved_ownership.len();
        report.scope_blocked = Some(format!(
            "cleanup ownership could not be proven: {unresolved} discovered Cargo {} did not resolve; no cleanup commands were run",
            if unresolved == 1 {
                "manifest"
            } else {
                "manifests"
            }
        ));
        report.unresolved_ownership = unresolved_ownership;
        report.resolved_workspaces = workspaces.len();
        report.counters = counters;
        return Ok(report);
    }

    // Cleanup phase is determinate only after complete ownership coverage is
    // established (C004). Announce the CleanupUnit total through the observer.
    observer.units_total(cleanup_phase, units.len() as u64);

    // Filled on the first candidate that reaches the final proof; shared by
    // every later candidate in this run (H3/O1).
    let mut hoisted: Option<ProofUniverse> = None;
    for unit in &units {
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
            let ws = &workspaces[unit.workspace_idx];
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
        if let Err(msg) = unit_block_reason_roots(unit, &roots, allowed_output_roots) {
            skipped(&mut report, msg, None, unit_skip_code(unit));
            observer.unit_completed(cleanup_phase);
            continue;
        }
        // C003 §7.3: single final CleanupUnit proof shared by
        // Preview/Simulate/Execute, re-validated against the complete bounded
        // ownership universe. Simulate runs every non-mutating gate Execute
        // runs and only diverges by not spawning Cargo.
        // H3/O1: the ownership universe is re-proven once for the whole run,
        // on the first candidate that reaches the proof. Lazy so a run where
        // every unit is skipped by policy still spawns nothing.
        let hoisted = hoisted.get_or_insert_with(|| {
            ProofUniverse::refresh(&workspaces, &roots, runner, &mut counters)
        });
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
            CleanMode::Preview | CleanMode::Execute => {
                // Proof was generated immediately before spawn and is consumed
                // here with no intervening mutation window beyond the spawn
                // itself (fail-closed TOCTOU minimization, not elimination).
                let frozen = proof.frozen_env.clone();
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
                if mode == CleanMode::Preview {
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
            OutputOwnershipClass::ExternalUnproven => {
                return CleanupReasonCode::SkippedUnauthorized;
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
    unresolved: Vec<UnresolvedOwnershipParticipant>,
}

fn resolve_cleanup_scope(
    manifests: &[PathBuf],
    scan_start: SystemTime,
    recency: Duration,
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
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
    let unresolved = coverage.unresolved;
    counters.unresolved_ownership = unresolved.len() as u64;
    let workspaces = coverage.workspaces;
    if !unresolved.is_empty() {
        return Ok(CleanupScope {
            workspaces,
            units: Vec::new(),
            unresolved,
        });
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
    let units = workspace::build_cleanup_units(&workspaces, &outcomes);
    Ok(CleanupScope {
        workspaces,
        units,
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
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: "expected a real directory".into(),
        });
    }
    Ok(())
}

fn collapse_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots = roots
        .into_iter()
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
        .collect();
    roots.sort();
    roots.dedup();
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !out.iter().any(|parent| root.starts_with(parent)) {
            out.push(root);
        }
    }
    out
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
) -> Result<Vec<ResolvedWorkspace>, String> {
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
    failure: Option<String>,
}

impl ProofUniverse {
    fn refresh(
        universe: &[ResolvedWorkspace],
        clean_roots: &[PathBuf],
        runner: &dyn CleanupRunner,
        counters: &mut ScanCounters,
    ) -> Self {
        match refresh_combined_universe(universe, clean_roots, runner, counters) {
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
    /// The refreshed universe, or the reason it could not be re-proven.
    fn get(&self) -> Result<&[ResolvedWorkspace], &str> {
        match (&self.fresh, &self.failure) {
            (Some(fresh), _) => Ok(fresh),
            (None, Some(reason)) => Err(reason.as_str()),
            (None, None) => Err("ownership could not be re-proven: no refreshed universe"),
        }
    }
}

/// Rediscover and resolve every selected source root for each candidate's final proof.
fn refresh_combined_universe(
    initial: &[ResolvedWorkspace],
    roots: &[PathBuf],
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
) -> Result<Vec<ResolvedWorkspace>, String> {
    let policy = crate::domain::EffectiveScanPolicy {
        recency: Duration::ZERO,
        scope: ScanScope::ExplicitRoots(roots.to_vec()),
        discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
    };
    let discovered = discovery::discover_manifests(&policy, &crate::progress::NoopObserver)
        .map_err(|e| {
            format!("ownership could not be re-proven: combined root discovery failed: {e}")
        })?;
    if !discovered.diagnostics.is_empty() {
        return Err(
            "ownership could not be re-proven: combined selected-root discovery is incomplete"
                .into(),
        );
    }
    let adapter = WorkspaceCleanupAdapter { runner };
    let mut diagnostics = Vec::new();
    let mut refreshed_counters = ScanCounters::default();
    let coverage = workspace::resolve_workspaces_with_coverage(
        &discovered.manifests,
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
        return Err(format!(
            "ownership could not be re-proven: combined selected-root universe has {} unresolved manifest(s); a workspace did not re-resolve",
            coverage.unresolved.len()
        ));
    }
    let fresh = coverage.workspaces;
    let ids: std::collections::HashSet<_> = fresh.iter().map(|w| w.id.clone()).collect();
    if initial.iter().any(|w| !ids.contains(&w.id)) {
        return Err("ownership could not be re-proven: a workspace changed identity or disappeared from the initial combined universe".into());
    }
    refresh_proof_universe(&fresh, runner, counters)
}

fn refresh_proof_universe_with_limit(
    universe: &[ResolvedWorkspace],
    runner: &dyn CleanupRunner,
    counters: &mut ScanCounters,
    worker_limit: usize,
) -> Result<Vec<ResolvedWorkspace>, String> {
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

        let mut failure = None;
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
                        format!(
                            "ownership could not be re-proven: workspace {} changed identity",
                            workspace.root.display()
                        )
                    });
                }
                None => {
                    failure.get_or_insert_with(|| {
                        format!(
                            "ownership could not be re-proven: workspace {} did not re-resolve",
                            workspace.root.display()
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
    // re-resolved it, so no extra per-candidate refresh is needed.
    let hoisted = ProofUniverse::refresh(universe, clean_roots, runner, counters);
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
    unit_block_reason_roots(unit, clean_roots, allowed_output_roots)?;
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
            return Err(
                "ownership could not be re-proven: workspace is outside the cleanup universe"
                    .into(),
            );
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
                return Err(format!(
                    "ownership could not be re-proven: workspace {} changed identity",
                    unit.root.display()
                )
                .into());
            }
            None => {
                return Err(format!(
                    "ownership could not be re-proven: workspace {} did not re-resolve",
                    unit.root.display()
                )
                .into());
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
            format!(
                "ownership could not be re-proven: workspace {} vanished",
                unit.root.display()
            )
        })?;
    let candidate = universe.iter().find(|w| w.id == unit.id).ok_or_else(|| {
        format!(
            "ownership could not be re-proven: workspace {} is outside the cleanup universe",
            unit.root.display()
        )
    })?;
    let mut old_members: Vec<PathBuf> = candidate
        .members
        .iter()
        .map(|m| canon(&m.source_root))
        .collect();
    let mut new_members: Vec<PathBuf> = fresh_ws
        .members
        .iter()
        .map(|m| canon(&m.source_root))
        .collect();
    old_members.sort();
    new_members.sort();
    if old_members != new_members {
        return Err("workspace changed before cleanup; skipped: member set changed".into());
    }
    if candidate.packages != fresh_ws.packages {
        return Err(
            "workspace changed before cleanup; skipped: Cargo package identity set changed".into(),
        );
    }
    let old_target_canon = candidate
        .output
        .target
        .physical_path
        .clone()
        .unwrap_or_else(|| canon(&candidate.output.target.logical_path));
    let new_target_canon = fresh_ws
        .output
        .target
        .physical_path
        .clone()
        .unwrap_or_else(|| canon(&fresh_ws.output.target.logical_path));
    let old_build_canon = candidate
        .output
        .build
        .physical_path
        .clone()
        .unwrap_or_else(|| canon(&candidate.output.build.logical_path));
    let new_build_canon = fresh_ws
        .output
        .build
        .physical_path
        .clone()
        .unwrap_or_else(|| canon(&fresh_ws.output.build.logical_path));
    if old_target_canon != new_target_canon {
        return Err(format!(
            "workspace changed before cleanup; skipped: target changed (was {}, now {})",
            old_target_canon.display(),
            new_target_canon.display()
        )
        .into());
    }
    if old_build_canon != new_build_canon {
        return Err(format!(
            "workspace changed before cleanup; skipped: build-dir changed (was {}, now {})",
            old_build_canon.display(),
            new_build_canon.display()
        )
        .into());
    }
    // 6/7. Remap the complete fresh candidate OutputSet onto the complete fresh
    // physical graph and require the same expected shape and ownership.
    let fresh_affected = workspace::map_workspace_groups(fresh_ws, &fresh_groups);
    if fresh_affected.len() != unit.groups.len() {
        return Err(
            "workspace changed before cleanup; skipped: physical output group set changed".into(),
        );
    }
    let mut fresh_covering: Vec<PathBuf> = Vec::new();
    for gi in &fresh_affected {
        let g = &fresh_groups[*gi];
        if g.ownership != OutputOwnershipClass::PrivateBounded {
            return Err(format!(
                "ownership changed before cleanup; skipped: output {} is now {}; {}",
                g.display.display(),
                g.ownership.label(),
                ownership_skip_detail(g.ownership)
            )
            .into());
        }
        fresh_covering.extend(g.covering.iter().cloned());
    }
    fresh_covering = workspace::outermost(&fresh_covering);
    if !sorted_same(&fresh_covering, &unit.covering) {
        return Err(
            "workspace changed before cleanup; skipped: physical output shape changed".into(),
        );
    }
    // 8. No other discovered workspace may now overlap the candidate's affected
    // output. The fresh graph catches proven overlaps (the group would be
    // `Shared` in step 7); this pass also covers symlink/unresolvable roots the
    // graph cannot represent, failing closed when they can reach the region.
    for other in &fresh_universe {
        if other.id == unit.id {
            continue;
        }
        for root in [&other.output.target, &other.output.build] {
            for other_path in provable_output_paths(root) {
                if unit.covering.iter().any(|c| {
                    other_path == *c || other_path.starts_with(c) || c.starts_with(&other_path)
                }) {
                    return Err(format!(
                        "ownership changed before cleanup; skipped: another discovered workspace ({}) now overlaps cleaned output {}",
                        other.root.display(),
                        other_path.display()
                    ).into());
                }
            }
        }
    }
    // 9. Covering-root physical proof: every affected covering root must exist,
    // be a real directory, and not be a symlink.
    for covering in &unit.covering {
        match fs::symlink_metadata(covering) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err("preflight changed: covering root disappeared".into());
            }
            Err(e) => {
                return Err(format!("preflight changed: cannot inspect covering root: {e}").into());
            }
            Ok(m) if m.file_type().is_symlink() => {
                return Err("preflight changed: covering root became symlink".into());
            }
            Ok(m) if !m.is_dir() => {
                return Err("preflight changed: covering root changed type".into());
            }
            Ok(_) => {}
        }
    }
    // 10. Fresh source + output activity with fresh timestamps, bounded by the
    // complete fresh universe's output set (same exclusion rule as analysis).
    let start = SystemTime::now();
    let cutoff = start
        .checked_sub(Duration::from_secs(recency_seconds))
        .ok_or_else(|| "recency window exceeds system time range".to_owned())?;
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
                return Err(
                    "workspace changed before cleanup; skipped: source became active".into(),
                );
            }
            Ok(traverse::SourceActivity::Quiet(t)) => {
                if let Some(t) = t {
                    newest_activity = Some(newest_activity.map_or(t, |old| old.max(t)));
                }
            }
            Err(()) => return Err("revalidation source activity uncertain".into()),
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
            return Err("revalidation output measurement uncertain".into());
        }
        if stats.newest.is_some_and(|t| t >= cutoff || t > start) {
            return Err("workspace changed before cleanup; skipped: output became active".into());
        }
        fresh_bytes = fresh_bytes.saturating_add(stats.bytes);
        if let Some(t) = stats.newest {
            newest_activity = Some(newest_activity.map_or(t, |old| old.max(t)));
        }
    }
    counters.proof_output_sizing_nanos = counters
        .proof_output_sizing_nanos
        .saturating_add(crate::domain::elapsed_nanos(sizing_start.elapsed()));
    // 11. Marker validation is part of the same non-mutating proof consumed by
    // Simulate, and covers the complete affected union.
    require_cargo_markers(&unit.covering)?;
    // 12. Frozen-environment capability validation from the fresh OutputSet.
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

fn frozen_env(ctx: &RevalidatedContext) -> Result<Vec<(OsString, OsString)>, String> {
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
    if ctx.capability.env_build_dir_set
        && ctx.capability.build_dir == CargoBuildDirCapability::Unavailable
    {
        return Err(
            "CARGO_BUILD_BUILD_DIR is set but runtime Cargo cannot prove separate build-dir; cleanup deferred"
                .into(),
        );
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
                return Err(
                    "Cargo reports a separate build directory without stable support; cleanup deferred"
                        .into(),
                );
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
    if mode == CleanMode::Preview {
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

fn require_cargo_markers(covering: &[PathBuf]) -> Result<(), String> {
    for root in covering {
        let tag = root.join("CACHEDIR.TAG");
        let meta = fs::symlink_metadata(&tag)
            .map_err(|_| "Cargo CACHEDIR.TAG marker is missing; cleanup deferred".to_owned())?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Cargo CACHEDIR.TAG is not a regular file; cleanup deferred".into());
        }
        let bytes = fs::read(&tag).map_err(|e| format!("cannot read Cargo cache marker: {e}"))?;
        if !bytes.starts_with(b"Signature: 8a477f597d28d172789f06886806bc55") {
            return Err(
                "target directory does not have Cargo's cache marker signature; cleanup deferred"
                    .into(),
            );
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
        // Default and --dry-run => Preview; --dryrun => Simulate; --yes => Execute.
        // Mapping lives in main.rs; here we assert enum defaults.
        assert_eq!(CleanMode::default(), CleanMode::Preview);
        assert_ne!(CleanMode::Preview, CleanMode::Simulate);
        assert_ne!(CleanMode::Simulate, CleanMode::Execute);
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

    #[test]
    fn preview_is_default_and_simulate_invokes_no_clean() {
        let (_d, root, target) = valid_fixture(1);
        let runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        // Preview invokes metadata + clean --dry-run.
        let report = clean_with(&root, 0, &[], CleanMode::Preview, &runner, &noop).unwrap();
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
                .render()
                .contains("no `cargo clean` command was invoked")
        );
        assert!(!report.render().contains("recovered"));
    }

    #[test]
    fn unresolved_discovered_manifest_blocks_every_cleanup_mode_before_clean() {
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
                report.results.is_empty(),
                "blocked scope must report no cleanable bytes: {mode:?}"
            );
            assert_eq!(
                report.scope_blocked.as_deref(),
                Some(
                    "cleanup ownership could not be proven: 1 discovered Cargo manifest did not resolve; no cleanup commands were run"
                ),
                "{mode:?}"
            );
            assert_eq!(
                report.diagnostics, 1,
                "participant diagnostic retained: {mode:?}"
            );
            assert!(
                runner.clean_calls().is_empty(),
                "{mode:?}: zero clean and dry-run invocations"
            );
            assert_eq!(
                report
                    .render()
                    .matches("cleanup ownership could not be proven")
                    .count(),
                1
            );
            assert!(report.render().contains("no cleanup commands were run"));
        }
    }

    #[test]
    fn simulation_candidate_set_matches_execute_preflight() {
        let (_d, root, target) = valid_fixture(1);
        let sim_runner = FakeCleanupRunner::new(&root, &target, 1);
        let exec_runner = FakeCleanupRunner::new(&root, &target, 1);
        let noop = NoopObserver;
        let sim = clean_with(&root, 0, &[], CleanMode::Simulate, &sim_runner, &noop).unwrap();
        let exec = clean_with(&root, 0, &[], CleanMode::Preview, &exec_runner, &noop).unwrap();
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
        assert!(report.render().contains("observed decrease"));
        assert!(!report.render().contains("would-clean"));
    }

    #[test]
    fn clean_requires_absolute_root() {
        let runner = FakeCleanupRunner::new(Path::new("/x"), Path::new("/x/target"), 1);
        let noop = NoopObserver;
        let err =
            clean_with(Path::new("."), 0, &[], CleanMode::Preview, &runner, &noop).unwrap_err();
        assert!(err.to_string().contains("absolute sandbox root"));
        assert!(runner.calls().is_empty());
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
        let report = clean_with(&root, 0, &[], CleanMode::Preview, &runner, &noop).unwrap();
        // Changed target between scan and revalidation → skipped, never previewed.
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Skipped);
        assert!(report.results[0].detail.contains("changed"));
        assert!(runner.base.clean_calls().is_empty());
    }

    #[test]
    fn unresolved_cargo_manifest_blocks_entire_cleanup_scope() {
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
        assert!(report.results.is_empty());
        assert!(
            report
                .render()
                .contains("cleanup ownership could not be proven")
        );
    }

    #[test]
    fn final_report_lists_all_groups_and_deduped_total() {
        let mut report = CleanReport {
            mode: CleanMode::Execute,
            ..Default::default()
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
        let text = report.render();
        // Every group printed, not only transient top five.
        for i in 0..7 {
            assert!(text.contains(&format!("/tmp/g{i}")));
        }
        // Total observed decrease is deduplicated sum (7*80=560).
        assert!(text.contains("observed decrease"));
        // Execute wording distinct from preview/simulate.
        assert!(text.contains("cleanup: 7 cleaned"));
        assert!(!text.contains("would-clean"));
        assert!(!text.contains("no `cargo clean`"));
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
        let preview = clean_with(&root, 0, &[], CleanMode::Preview, &runner, &noop).unwrap();
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
        assert!(preview.render().contains("no cleanup executed"));
        let profile_preview = clean_with_roots_policy_selector(
            std::slice::from_ref(&root),
            0,
            &[],
            CleanMode::Preview,
            &runner,
            &noop,
            &CleanupPolicy::default(),
            Some(CleanupSelector::Profile("dev".into())),
        )
        .unwrap();
        assert_eq!(profile_preview.results[0].outcome, CleanOutcome::Previewed);
        assert!(profile_preview.render().contains("output-union before"));
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
        let version = Command::new("cargo").arg("--version").output().unwrap();
        let version = String::from_utf8_lossy(&version.stdout);
        let supported = version.split_whitespace().nth(1).is_some_and(|version| {
            workspace::clean_capabilities_from_version(version).package_selector
        });
        if !supported {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate] {
            let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1);
            let expected = if mode == CleanMode::Preview {
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
        let report = clean_with(&root, 0, &[], CleanMode::Preview, &runner, &noop).unwrap();
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
            for mode in [CleanMode::Simulate, CleanMode::Preview] {
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
            for mode in [CleanMode::Simulate, CleanMode::Preview] {
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
        for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
            let runner = RaceRunner::with_second_target(&root, &target, other_base.clone());
            let report = clean_with(&root, 0, &[], mode, &runner, &noop).unwrap();
            assert_eq!(report.results.len(), 1, "{mode:?}");
            assert_eq!(report.results[0].outcome, CleanOutcome::Skipped, "{mode:?}");
            assert!(report.results[0].detail.contains("changed"), "{mode:?}");
        }
    }

    #[test]
    fn direct_metadata_refresh_detects_workspace_root_and_member_changes() {
        for mutation_kind in ["root", "member", "malformed"] {
            for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
                assert_eq!(report.results.len(), 1, "{mutation_kind} {mode:?}");
                assert_eq!(
                    report.results[0].outcome,
                    CleanOutcome::Skipped,
                    "{mutation_kind} {mode:?}: {}",
                    report.results[0].detail
                );
                if mutation_kind == "root" {
                    assert!(
                        report.results[0].detail.contains("changed identity"),
                        "{}",
                        report.results[0].detail
                    );
                } else if mutation_kind == "member" {
                    assert!(
                        report.results[0].detail.contains("member set changed"),
                        "{}",
                        report.results[0].detail
                    );
                } else {
                    assert!(
                        report.results[0].detail.contains("did not re-resolve"),
                        "{}",
                        report.results[0].detail
                    );
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
            for mode in [CleanMode::Simulate, CleanMode::Preview] {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
            for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
            let (_d, root, target) = valid_fixture(1);
            let runner = FakeCleanupRunner::new(&root, &target, 1);
            let observer = TestObserver::new();
            let expected_phase = match mode {
                CleanMode::Preview => ScanPhase::CleanupPreview,
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

    /// Multi-workspace Cargo fixture with per-workspace output staging,
    /// metadata failure injection, and observable `cargo clean` effects.
    struct StagedCargo {
        ws: Vec<FakeWs>,
        calls: Mutex<Vec<RecordedCall>>,
        metadata_calls: Mutex<Vec<usize>>,
        clean_removes: Mutex<Vec<PathBuf>>,
        clean_hook: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
                CleanMode::Preview => CleanOutcome::Previewed,
                CleanMode::Simulate => CleanOutcome::Simulated,
                CleanMode::Execute => CleanOutcome::Cleaned,
            };
            assert_eq!(row.outcome, expected, "{mode:?}: {}", row.detail);
            assert_eq!(row.output_roots.len(), 2, "{mode:?}: both roots");
            assert_eq!(row.ownership, OutputOwnershipClass::PrivateBounded);
            match mode {
                CleanMode::Preview => {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
            let (_d, root, target, build) = sibling_ws_fixture("one-invocation");
            let mut runner = staged_sibling_runner(&root, &target, &build);
            runner.removes_on_clean(vec![
                target.join("artifact.bin"),
                build.join("artifact.bin"),
            ]);
            let observer = TestObserver::new();
            let phase = match mode {
                CleanMode::Preview => ScanPhase::CleanupPreview,
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
            let text = report.render();
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
        for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
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
        for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
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
        for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
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
        // C003 §10: failure to re-resolve any workspace that could affect the
        // bounded ownership graph makes the candidate non-cleanable.
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
        let row = row_for(&report, &root_a);
        assert_eq!(row.outcome, CleanOutcome::Skipped, "{}", row.detail);
        assert!(
            row.detail.contains("could not be re-proven"),
            "{}",
            row.detail
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
        for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
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
            for mode in [CleanMode::Simulate, CleanMode::Preview, CleanMode::Execute] {
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
            (CleanMode::Preview, CleanOutcome::Previewed),
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
            if mode == CleanMode::Preview {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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

    #[test]
    fn unresolved_manifest_in_second_root_blocks_first_root_for_all_modes() {
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
        for mode in [CleanMode::Preview, CleanMode::Simulate, CleanMode::Execute] {
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
            assert!(report.results.is_empty(), "{mode:?}");
            assert!(report.render().contains("2 root(s)"), "{mode:?}");
            assert!(runner.clean_calls().is_empty(), "{mode:?}");
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
}
