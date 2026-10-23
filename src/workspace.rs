//! Workspace/output resolution, physical grouping, and fail-fast pipeline.
//!
//! Cargo is authoritative for workspace and output configuration. This module
//! never reimplements Cargo config merge rules. It uses `cargo locate-project`
//! and `cargo metadata` through a test-injectable runner, caches by workspace,
//! groups overlapping physical output, and measures each group once.

use crate::{
    domain::*,
    progress::{ProgressObserver, ScanPhase},
    traverse,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[derive(Debug)]
pub struct ProcessOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CargoRunner: Sync {
    fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput>;
}

/// Anchor a manifest path for use as Cargo's `--manifest-path`.
///
/// The child's working directory is set to the manifest's own parent, so a
/// *relative* manifest path is resolved against the wrong directory: for a
/// discovered `fixture/Cargo.toml`, Cargo is asked for
/// `<root>/fixture/fixture/Cargo.toml`, which does not exist. The scan then
/// degrades to zero resolved workspaces, still exits 0, and reports no groups.
///
/// This is a lexical absolutization against the process working directory, not
/// a canonicalization: it does not resolve symlinks, so the path Cargo sees
/// keeps the spelling the user gave and Cargo's own config discovery from the
/// manifest's directory is unchanged. A relative path that cannot be anchored
/// is passed through unchanged, and Cargo produces the error it always would.
fn manifest_path_for_cargo(manifest: &Path) -> PathBuf {
    if manifest.is_absolute() {
        return manifest.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(manifest),
        Err(_) => manifest.to_path_buf(),
    }
}

/// Refresh one previously resolved root manifest directly through Cargo metadata.
/// The caller must compare the returned identity and complete workspace shape
/// with the state it is revalidating; this function never locates a new root.
pub fn refresh_workspace_from_root_manifest(
    root_manifest: &Path,
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Option<ResolvedWorkspace> {
    let canonical_key = canonical_manifest_key(root_manifest);
    resolve_one_workspace_cached(
        root_manifest,
        &canonical_key,
        runner,
        counters,
        diagnostics,
        observer,
        root_manifest,
    )
}

pub struct SystemCargoRunner;

impl CargoRunner for SystemCargoRunner {
    fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
        let output = std::process::Command::new("cargo")
            .args(args)
            .current_dir(cwd)
            .output()?;
        Ok(ProcessOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[derive(Debug, Deserialize)]
struct LocateOutput {
    root: String,
}

#[derive(Debug, Deserialize)]
struct RawMetadata {
    packages: Vec<RawPackage>,
    #[allow(dead_code)]
    workspace_members: Vec<String>,
    workspace_root: String,
    target_directory: String,
    #[serde(default)]
    build_directory: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPackage {
    id: String,
    name: String,
    #[serde(default)]
    version: String,
    manifest_path: String,
}

fn cargo_env_build_dir_set() -> bool {
    std::env::var_os("CARGO_BUILD_BUILD_DIR").is_some()
}

pub fn capability_from_metadata(
    had_build_field: bool,
    target: &str,
    build: Option<&str>,
    env_set: bool,
) -> CargoCapabilities {
    let build_dir = match (had_build_field, build) {
        (false, _) if env_set => CargoBuildDirCapability::Unknown,
        (false, _) => CargoBuildDirCapability::Unavailable,
        (true, None) => CargoBuildDirCapability::Unknown,
        (true, Some(b)) if b == target => CargoBuildDirCapability::Equal,
        (true, Some(_)) => CargoBuildDirCapability::Distinct,
    };
    CargoCapabilities {
        build_dir,
        metadata_had_build_directory: had_build_field,
        env_build_dir_set: env_set,
    }
}

/// Selector capabilities are qualified against real Cargo behavior, not the
/// cargo-cleanme crate MSRV. Unknown/future versions are deliberately false.
///
/// The qualification is by **exact release**: a Cargo release is claimed only
/// where hosted real-Cargo evidence exists for *that* release, and a patch
/// release of a claimed minor is a different release with no evidence of its
/// own. The policy and its evidence lifecycle live in
/// `release/selector-qualification.json`, and
/// `scripts/check-selector-qualification.py` is the gate that keeps this
/// function and that file in exact agreement.
///
/// The consequence — which the operator is told about — is that a selector is
/// refused on a Cargo the tool has no qualification for. The refusal is
/// fail-closed and correct; what is *not* correct is stating it as a fact
/// about the host's capabilities, which is why the refusal names the
/// qualification rather than the host.
pub fn clean_capabilities_from_version(version: &str) -> CargoCleanCapabilities {
    let profile_selector = matches!(
        version,
        "1.89.0"
            | "1.90.0"
            | "1.91.1"
            | "1.92.0"
            | "1.93.1"
            | "1.94.1"
            | "1.95.0"
            | "1.98.1"
            | "1.99.0"
    );
    CargoCleanCapabilities {
        profile_selector,
        // The configured build.target discrepancy is bounded to exact tested
        // releases where configured and explicit target dry-runs agree.
        package_selector: matches!(version, "1.98.1" | "1.99.0"),
    }
}

/// True for a filesystem root (`/`, `C:\`), which is never a Cargo artifact
/// directory.
///
/// A *relative* path is not a filesystem root: `target` has no parent, but it
/// is a relative name, not the top of a filesystem. The predicate used to
/// treat a relative path's empty parent as a root, which would have refused
/// a relative `target_directory` with a message about a filesystem root.
fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

fn canonical_or_absolute(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        fs::canonicalize(path)
            .map_err(|e| format!("cannot resolve Cargo path {}: {e}", path.display()))
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "Cargo returned a relative path: {}",
            path.display()
        ))
    }
}

/// Bounded causal excerpt from a failed Cargo invocation's stderr.
///
/// The other failure modes in this module carry their cause — "could not start:
/// {e}", "cannot parse … output: {e}" — so a bare "cargo metadata failed" is the
/// one shape that leaves the user with a project missing from every report and
/// no way to learn why. Cargo already names the offending manifest on stderr,
/// and the remedy is "fix the manifest", which is not guessable from a label.
///
/// Keep a few non-empty lines because Cargo commonly puts the useful cause
/// after a generic `failed searching` summary. Collapse line breaks and remove
/// terminal control characters before this reaches human or JSON output.
fn cargo_failure_reason(stderr: &[u8]) -> String {
    const MAX_REASON_BYTES: usize = 480;
    const MAX_REASON_LINES: usize = 3;
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_REASON_LINES)
        .map(|line| {
            line.chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>()
        })
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "cargo wrote nothing to stderr".into();
    }
    let mut reason = lines.join(" | ");
    if reason.len() > MAX_REASON_BYTES {
        let mut end = MAX_REASON_BYTES;
        while end > 0 && !reason.is_char_boundary(end) {
            end -= 1;
        }
        reason.truncate(end);
        reason.push_str("… (truncated)");
    }
    reason
}

/// `symlink_metadata("link/")` follows the final component on POSIX, so a
/// trailing slash hides a symlink from the `is_symlink` check. Judge the
/// path with its trailing separators stripped as well. A trailing `/.`
/// spells the same bypass, so dot components are stripped too.
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

/// Probe one declared output root.
///
/// The path comes from untrusted `cargo metadata`, so a probe can fail for
/// reasons that are not absence (EACCES, ELOOP, a name too long). Those are
/// reported instead of being folded into "does not exist", and a root that is a
/// filesystem root is refused outright: no Cargo project uses `/` as its
/// artifact directory, and sizing one walks the entire machine.
fn output_root(
    kind: OutputRootKind,
    logical: PathBuf,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> OutputRoot {
    if is_filesystem_root(&logical) {
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(logical.clone()),
            message: format!(
                "Cargo declares a filesystem root as its {} directory; it will not be sized or cleaned",
                match kind {
                    OutputRootKind::Target => "target",
                    OutputRootKind::Build => "build",
                }
            ),
        });
        return OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: false,
            is_symlink: false,
        };
    }
    match fs::symlink_metadata(&logical) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: false,
            is_symlink: false,
        },
        // A trailing slash (`link/`) makes `symlink_metadata` follow the
        // link, so check the stripped spelling too before trusting `Ok`.
        Ok(meta) if meta.file_type().is_symlink() || path_is_symlink(&logical) => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: true,
            is_symlink: true,
        },
        Err(e) => {
            // Unreadable is not absent: say so, and keep the root unusable.
            // The root reports `exists: true` with no physical identity so
            // `build_groups` marks the workspace uncertain instead of
            // silently dropping the root from the exclusivity check, which
            // would let a sibling claim `PrivateBounded` over bytes this
            // root may share.
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(logical.clone()),
                message: format!(
                    "cannot inspect {} output directory {}: {e}",
                    match kind {
                        OutputRootKind::Target => "target",
                        OutputRootKind::Build => "build",
                    },
                    logical.display()
                ),
            });
            OutputRoot {
                kind,
                logical_path: logical,
                physical_path: None,
                exists: true,
                is_symlink: false,
            }
        }
        Ok(meta) if !meta.is_dir() => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: true,
            is_symlink: false,
        },
        Ok(_) => match fs::canonicalize(&logical) {
            Ok(canonical) => OutputRoot {
                kind,
                logical_path: logical,
                physical_path: Some(canonical),
                exists: true,
                is_symlink: false,
            },
            Err(_) => OutputRoot {
                kind,
                logical_path: logical,
                physical_path: None,
                exists: true,
                is_symlink: false,
            },
        },
    }
}

fn canonical_manifest_key(path: &Path) -> PathBuf {
    canonical_or_absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The workspace manifest Cargo names when a package claims a workspace that
/// does not claim it (`current package believes it's in a workspace when it's
/// not`). Cargo prints `current:` and `workspace:` paths; the sanitized reason
/// joins lines with ` | `, so the workspace path is the `Cargo.toml` token
/// after `workspace:`.
///
/// `None` when the class does not apply or no workspace token is present
/// (including truncation). Callers fail closed on `None`.
fn believed_workspace_manifest(reason: &str) -> Option<PathBuf> {
    if !reason.contains("believes it's in a workspace") {
        return None;
    }
    named_workspace_manifest(reason)
}

/// The workspace manifest Cargo names, if it names one.
///
/// The parse itself is separate from the sentence that introduces it, so a
/// caller that only wants the *path* does not have to re-implement it, and a
/// caller that wants a tolerant reading of the same refusal can ask for the
/// path without inheriting the strict sentence gate.
fn named_workspace_manifest(reason: &str) -> Option<PathBuf> {
    let idx = reason.find("workspace:")?;
    let after = &reason[idx + "workspace:".len()..];
    for token in after.split_whitespace() {
        let t = token.trim_matches(|c: char| {
            c == '|' || c == '"' || c == '\'' || c == ',' || c == ';' || c == '(' || c == ')'
        });
        if t.contains("Cargo.toml") {
            return Some(PathBuf::from(t));
        }
    }
    None
}

/// Whether a refusal reason is Cargo's "this package believes it is in a
/// workspace that does not claim it" refusal.
///
/// The detection is on Cargo's **structured** output — the `current:` manifest
/// it examined and the `workspace:` manifest it believes the package belongs
/// to — with the English sentence accepted as one spelling of the same
/// condition.
///
/// Matching the sentence alone made the *guidance* depend on Cargo's prose:
/// a Cargo that reworded the sentence, while still naming both manifests,
/// silently stopped producing the hint that tells an operator what to do
/// about the refusal. The hint is guidance, so the bias is towards tolerance:
/// offering it for a refusal that is not this one costs a sentence, while
/// withholding it costs the operator the whole explanation.
///
/// This is deliberately *not* [`believed_workspace_manifest`]: that one gates a
/// safety decision (a refused manifest rides as a passenger of the workspace
/// Cargo names), so it stays strict.
pub(crate) fn refused_nested_workspace(reason: &str) -> bool {
    if reason.contains("believes it's in a workspace") {
        return true;
    }
    reason.contains("current:") && named_workspace_manifest(reason).is_some()
}

/// Island assignment for one unresolved manifest (M015 certificate §4.3).
///
/// Total over every failure class: only `RefusedByCargo` unknowns (Cargo
/// examined the manifest and refused it, so it owns no bytes) avoid the
/// merge. Everything else — transport, unparseable output, lost workers,
/// metadata, identity — is `MergeAll`, today's whole-scope block. A refused
/// manifest that names a workspace resolved in this same run rides as a
/// passenger of that island; a refused manifest with no resolved owner is a
/// scope-level note. Neither merges nor blocks beyond the engine's own block,
/// which M015 never relaxes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnknownDisposition {
    /// Byte-incapable; Cargo named this workspace manifest as the believed
    /// owner and it resolved here. Reported on that island, merges nothing.
    PassengerOf { named_manifest: PathBuf },
    /// Byte-incapable with no resolved owner named. A scope-level note that
    /// merges nothing.
    ScopeNote,
    /// Byte-capability unproven: merges every island (today's block).
    MergeAll,
}

/// Decide what one unresolved manifest means for the proof-island partition.
///
/// Pure: no spawns, no filesystem mutation, no counter or diagnostic changes.
/// Takes the resolved workspace list directly — the rest of `coverage`
/// (counts, the unresolved ledger itself) carries no signal this decision
/// reads. Consortium with [`believed_workspace_manifest`]: the parse
/// returning `None` (wrong class, truncation) fails closed to `ScopeNote`
/// for refused manifests — never to a passenger of an unproven island.
pub fn attribute_unknown(
    participant: &UnresolvedOwnershipParticipant,
    workspaces: &[ResolvedWorkspace],
) -> UnknownDisposition {
    if participant.capability != UnknownCapability::RefusedByCargo {
        return UnknownDisposition::MergeAll;
    }
    match believed_workspace_manifest(&participant.reason) {
        Some(named) => {
            // The named manifest does not depend on the workspace being
            // tested, so it is canonicalized once instead of once per
            // workspace: every canonicalization is a stat and a realpath.
            let named_key = canonical_manifest_key(&named);
            if workspaces
                .iter()
                .any(|w| canonical_manifest_key(&w.root_manifest) == named_key)
            {
                UnknownDisposition::PassengerOf {
                    named_manifest: named,
                }
            } else {
                UnknownDisposition::ScopeNote
            }
        }
        None => UnknownDisposition::ScopeNote,
    }
}

/// Resolve manifests to unique workspaces with caching.
///
/// Returns workspaces in deterministic order plus diagnostics. Cache key is the
/// canonical workspace root manifest from locate-project.
///
/// Resolution runs in bulk phases: every manifest is located (bounded
/// parallelism), manifests are grouped by located root in first-seen order,
/// then one metadata runs per group (bounded parallelism), and a sequential
/// emulation pass replays manifest order over the precomputed results. The
/// emulation keeps the historical per-manifest semantics exactly: a manifest
/// already listed by earlier metadata is a dedup hit with no new metadata, a
/// repeat trigger of a failed root re-accounts metadata like a fresh spawn,
/// and diagnostics, coverage, workspace order, and the unresolved ledger are
/// identical to sequential evaluation. Only `cargo_locate_calls` /
/// `cargo_locate_nanos` grow (one locate per manifest instead of one per
/// unresolved-unknown manifest), because the sequential fast path that
/// skipped locates serializes the whole phase on member order. Metadata stays
/// one spawn per unique root; failed roots re-account without re-spawning.
/// Membership is never inferred from path ancestry or hand-parsed TOML.
pub fn resolve_workspaces(
    manifests: &[PathBuf],
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Vec<ResolvedWorkspace> {
    resolve_workspaces_with_coverage(manifests, runner, counters, diagnostics, observer).workspaces
}

/// Bound on concurrent Cargo resolution spawns. Cargo invocations are
/// read-only here (`locate-project`, `metadata --offline --locked --no-deps`)
/// and share no mutable state, but each spawn pays process-startup I/O, so
/// the pool stays small. Same bound as the proof-refresh pool
/// (`PROOF_REFRESH_CONCURRENCY_CAP` in `cleanup.rs`); keep the two aligned.
const RESOLUTION_CONCURRENCY_CAP: usize = 4;

fn resolution_concurrency() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, RESOLUTION_CONCURRENCY_CAP)
}

/// Bounded parallel map preserving input order.
///
/// Work items run in input-order chunks of [`resolution_concurrency`];
/// single-item chunks skip thread spawning. A panicking worker degrades its
/// slot to `None` and the caller fails that item closed — never a panic, a
/// lost item, or an out-of-order result.
fn parallel_map<T: Send>(len: usize, work: &(dyn Fn(usize) -> T + Sync)) -> Vec<Option<T>> {
    let concurrency = resolution_concurrency();
    let mut ordered = Vec::with_capacity(len);
    for batch_start in (0..len).step_by(concurrency) {
        let batch_end = (batch_start + concurrency).min(len);
        if batch_end - batch_start == 1 {
            ordered.push(Some(work(batch_start)));
        } else {
            std::thread::scope(|scope| {
                let handles: Vec<_> = (batch_start..batch_end)
                    .map(|index| scope.spawn(move || work(index)))
                    .collect();
                for handle in handles {
                    ordered.push(handle.join().ok());
                }
            });
        }
    }
    ordered
}

/// One `cargo locate-project` spawn, classified without touching shared
/// resolution state. Workers are total: every transport, Cargo-reported, and
/// parse outcome maps to a variant the sequential emulation fails closed on.
enum LocateOutcome {
    Transport {
        error: String,
        nanos: u64,
    },
    Failed {
        reason: String,
        nanos: u64,
    },
    Unparseable {
        error: String,
        nanos: u64,
    },
    Located {
        root_manifest: PathBuf,
        canonical_key: PathBuf,
        nanos: u64,
    },
    /// A parallel worker slot was lost; fail-closed as a spawn failure with
    /// a deterministic message rather than a panic or a lost manifest.
    WorkerFailed,
}

/// Measured wall time of one classified locate outcome, if it consumed a
/// subprocess. A lost worker slot has no timing to report.
fn locate_nanos(outcome: &LocateOutcome) -> Option<u64> {
    match outcome {
        LocateOutcome::Transport { nanos, .. }
        | LocateOutcome::Failed { nanos, .. }
        | LocateOutcome::Unparseable { nanos, .. }
        | LocateOutcome::Located { nanos, .. } => Some(*nanos),
        LocateOutcome::WorkerFailed => None,
    }
}

fn locate_one(manifest: &Path, runner: &dyn CargoRunner) -> LocateOutcome {
    let locate_args: Vec<std::ffi::OsString> = ["locate-project", "--workspace", "--manifest-path"]
        .into_iter()
        .map(Into::into)
        // Anchored for Cargo, not for the diagnostic below: the recorded
        // path keeps the spelling the user gave.
        .chain(std::iter::once(
            manifest_path_for_cargo(manifest).into_os_string(),
        ))
        .collect();
    // `"Cargo.toml".parent()` is `Some("")`, not `None`, so the empty parent
    // must be filtered for the fallback to apply (L-4).
    let cwd = manifest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("/"));
    let locate_start = std::time::Instant::now();
    let elapsed = || crate::domain::elapsed_nanos(locate_start.elapsed());
    let locate_out = match runner.run(cwd, &locate_args) {
        Ok(output) => output,
        Err(e) => {
            return LocateOutcome::Transport {
                error: e.to_string(),
                nanos: elapsed(),
            };
        }
    };
    let nanos = elapsed();
    if !locate_out.success {
        return LocateOutcome::Failed {
            reason: cargo_failure_reason(&locate_out.stderr),
            nanos,
        };
    }
    match serde_json::from_slice::<LocateOutput>(&locate_out.stdout) {
        Ok(locate) => {
            let root_manifest = PathBuf::from(&locate.root);
            let canonical_key =
                canonical_or_absolute(&root_manifest).unwrap_or(root_manifest.clone());
            LocateOutcome::Located {
                root_manifest,
                canonical_key,
                nanos,
            }
        }
        Err(e) => LocateOutcome::Unparseable {
            error: e.to_string(),
            nanos,
        },
    }
}

/// The workspace that encloses `dir`, if any (ADR 009).
///
/// Cargo's own answer, obtained by running `cargo locate-project --workspace`
/// *in* the directory: Cargo walks up from the child's working directory to
/// the nearest manifest and reports the workspace that owns it. A directory
/// with no manifest above it has no enclosing workspace, and Cargo says so by
/// failing — which is a normal answer here, not an error.
///
/// The child gets the directory as its working directory and **no**
/// `--manifest-path`. That option names a manifest to report on, not a place
/// to search from, so it would answer for a manifest instead of for the
/// operator's position.
///
/// A failure of any kind — a transport error, a Cargo refusal, an
/// unparseable answer — yields "no enclosing workspace". The working
/// directory remains a scope root on its own, so the worst case is that the
/// enclosing workspace is not cleaned, never that something is cleaned
/// without proof.
pub fn enclosing_workspace_root(dir: &Path, runner: &dyn CargoRunner) -> Option<PathBuf> {
    let args: Vec<std::ffi::OsString> = ["locate-project", "--workspace"]
        .into_iter()
        .map(Into::into)
        .collect();
    let output = runner.run(dir, &args).ok()?;
    if !output.success {
        return None;
    }
    let locate: LocateOutput = serde_json::from_slice(&output.stdout).ok()?;
    let manifest = PathBuf::from(&locate.root);
    // The workspace root is the manifest's directory. An empty parent would
    // name no directory at all, so it is filtered rather than admitted.
    let root = manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())?
        .to_path_buf();
    Some(root)
}

/// The scope roots the working directory contributes to the maintenance scope
/// (ADR 009).
///
/// Two roots: the workspace that encloses the working directory, when there
/// is one, and the working directory itself.
///
/// The enclosing workspace is not optional. A member directory's manifest
/// sits *above* the member, so a root at the member cannot discover it — and
/// an operator standing in `workspace/crates/member/src` expects that
/// workspace's output to be reclaimed, exactly as `cargo clean` would. The
/// anchor already admits that workspace in both directions
/// (`CleanupSelection::Ancestry`); this is the discovery half of the same
/// rule.
///
/// The enclosing workspace always contains the working directory, so the two
/// roots collapse to one when there is a workspace; the working directory is
/// still named separately so the scope reads as the operator's position when
/// there is not.
pub fn working_directory_scope_roots(dir: &Path, runner: &dyn CargoRunner) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(workspace) = enclosing_workspace_root(dir, runner)
        && workspace != dir
    {
        roots.push(workspace);
    }
    roots.push(dir.to_path_buf());
    roots
}

/// One `cargo metadata` spawn for a located group. The worker only spawns;
/// parsing, member validation, diagnostics, and counters stay in the
/// sequential emulation so their order matches sequential evaluation.
struct MetadataSpawn {
    result: Result<ProcessOutput, String>,
    nanos: u64,
}

fn metadata_one(root_manifest: &Path, runner: &dyn CargoRunner) -> MetadataSpawn {
    let meta_args: Vec<std::ffi::OsString> = [
        "metadata",
        "--offline",
        "--locked",
        "--no-deps",
        "--format-version",
        "1",
        "--manifest-path",
    ]
    .into_iter()
    .map(Into::into)
    .chain(std::iter::once(root_manifest.as_os_str().to_owned()))
    .collect();
    // As above: filter the empty parent (`Some("")`) so the fallback applies.
    let cwd = root_manifest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("/"));
    let meta_start = std::time::Instant::now();
    let elapsed = || crate::domain::elapsed_nanos(meta_start.elapsed());
    match runner.run(cwd, &meta_args) {
        Ok(output) => MetadataSpawn {
            result: Ok(output),
            nanos: elapsed(),
        },
        Err(e) => MetadataSpawn {
            result: Err(e.to_string()),
            nanos: elapsed(),
        },
    }
}

/// A metadata outcome a repeat trigger re-accounts without re-spawning.
///
/// The sequential loop re-ran metadata for every manifest locating to a root
/// whose metadata had failed (a failed root never enters the caches). The
/// message here is path-independent — the manifest path travels in the
/// diagnostic's `path` field, never in the text — so one recorded outcome
/// replays for every later trigger with identical accounting. Whether the
/// outcome counts as a Cargo failure is part of the template: transport,
/// Cargo-reported, and parse failures do, while member-identity and
/// relative-root diagnostics do not (matching the sequential branches).
struct MetadataFailure {
    message: String,
    counts_as_failure: bool,
}

/// Manifests locating to one workspace root, in input order. One metadata
/// spawn per group is the same spawn set the sequential loop issued (every
/// trigger ran metadata), only issued up front instead of interleaved.
struct ResolutionGroup {
    root_manifest: PathBuf,
}

/// Resolve discovered manifests while retaining proof of which manifests were
/// authoritatively represented by successful Cargo workspace metadata.
/// Read-only scan callers can continue using [`resolve_workspaces`], whose
/// partial-result behavior is intentionally unchanged.
///
/// The implementation is bulk-parallel but sequentially faithful (see
/// [`resolve_workspaces`]): locate runs bounded-parallel over all manifests,
/// metadata runs bounded-parallel over located groups, and a manifest-order
/// emulation replays per-manifest decisions over the precomputed results.
pub fn resolve_workspaces_with_coverage(
    manifests: &[PathBuf],
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> ResolutionCoverage {
    let mut cache: HashMap<PathBuf, ResolvedWorkspace> = HashMap::new();
    let mut ordered_keys: Vec<PathBuf> = Vec::new();
    // Authoritative member -> root mapping seeded from Cargo metadata.
    let mut member_to_root: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut authoritative_coverage: HashSet<PathBuf> = HashSet::new();
    // Bulk phase 1 (parallel): locate every manifest. Each spawn is
    // independent — a locate result never depends on another manifest's —
    // so the whole phase runs in input-order chunks of worker threads and
    // joins in order. A lost worker slot degrades to a spawn failure.
    let locate_spawns = parallel_map(manifests.len(), &|index| {
        locate_one(&manifests[index], runner)
    });
    // Bulk phase 2 (sequential, no subprocess): collect locate outcomes in
    // input order and group manifests by located workspace root, first-seen
    // first. Group order is the order the old sequential loop would have
    // discovered workspaces in.
    let mut locate_outcomes: Vec<LocateOutcome> = Vec::with_capacity(manifests.len());
    let mut group_index: HashMap<PathBuf, usize> = HashMap::new();
    let mut groups: Vec<ResolutionGroup> = Vec::new();
    let mut group_of: Vec<Option<usize>> = vec![None; manifests.len()];
    for (index, spawn) in locate_spawns.into_iter().enumerate() {
        let outcome = spawn.unwrap_or(LocateOutcome::WorkerFailed);
        if let LocateOutcome::Located {
            root_manifest,
            canonical_key,
            ..
        } = &outcome
        {
            let group = match group_index.get(canonical_key) {
                Some(&group) => group,
                None => {
                    let group = groups.len();
                    group_index.insert(canonical_key.clone(), group);
                    groups.push(ResolutionGroup {
                        root_manifest: root_manifest.clone(),
                    });
                    group
                }
            };
            group_of[index] = Some(group);
        }
        locate_outcomes.push(outcome);
    }
    // Bulk phase 3 (parallel): one metadata spawn per located group. Every
    // group has a trigger (the first manifest locating to it), and the old
    // loop ran metadata for every trigger, so this is the same spawn set —
    // issued up front instead of interleaved with locates.
    let metadata_spawns = parallel_map(groups.len(), &|group| {
        metadata_one(&groups[group].root_manifest, runner)
    });
    let metadata_spawns: Vec<MetadataSpawn> = metadata_spawns
        .into_iter()
        .map(|spawn| {
            spawn.unwrap_or(MetadataSpawn {
                result: Err("parallel metadata worker failed".into()),
                nanos: 0,
            })
        })
        .collect();
    // Bulk phase 4 (sequential emulation): replay the manifest loop over the
    // precomputed results. Every branch below matches the old sequential
    // loop decision-for-decision — same diagnostics in the same order, same
    // coverage sets, same workspace order — except that locates were already
    // issued for all manifests, so the fast path saves metadata but not the
    // locate itself (see [`resolve_workspaces`]).
    //
    // Repeat triggers of a failed root reuse the recorded outcome: the old
    // loop re-spawned metadata here (a failed root never enters the caches),
    // so accounting matches a fresh spawn while one pass cannot fork the
    // ledger on concurrent mutation. Repeat triggers of a successful root
    // never reach the metadata code — the caches below divert them first.
    let mut failed_templates: HashMap<PathBuf, (MetadataFailure, u64)> = HashMap::new();
    // Explicit failure stage and byte-capability per manifest, recorded where
    // the diagnostic is pushed so `unresolved` labels do not depend on
    // re-parsing message text. Only the branch where Cargo itself ran and
    // refused the manifest (`Failed`) is `RefusedByCargo`: transport,
    // unparseable output, and lost workers never examined the manifest, so
    // byte-capability stays unproven there.
    let mut unresolved_stage: HashMap<PathBuf, (&'static str, UnknownCapability)> = HashMap::new();
    for (index, manifest) in manifests.iter().enumerate() {
        // Every manifest located exactly once up front, so the spawn is
        // accounted here — before any fast path. A fast-pathed manifest
        // still ran its locate (its outcome is simply unneeded); only
        // failures observed by the old loop are reported.
        counters.cargo_locate_calls += 1;
        if let Some(nanos) = locate_nanos(&locate_outcomes[index]) {
            counters.cargo_locate_nanos = counters.cargo_locate_nanos.saturating_add(nanos);
        }
        // C002 fast path: if this manifest was listed as a member (or root)
        // by an earlier successful metadata response, reuse that workspace
        // without another metadata process. (The locate above already ran;
        // only the metadata is still saved.)
        let manifest_key = canonical_manifest_key(manifest);
        if let Some(root_key) = member_to_root.get(&manifest_key)
            && cache.contains_key(root_key)
        {
            counters.deduped_workspace_hits += 1;
            continue;
        }
        let outcome = &locate_outcomes[index];
        let canonical_key = match outcome {
            LocateOutcome::Transport { error, .. } => {
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: format!("cargo locate-project could not start: {error}"),
                });
                unresolved_stage.insert(manifest.clone(), ("locate", UnknownCapability::Unproven));
                continue;
            }
            LocateOutcome::Failed { reason, .. } => {
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: format!("cargo locate-project failed: {reason}"),
                });
                unresolved_stage.insert(
                    manifest.clone(),
                    ("locate", UnknownCapability::RefusedByCargo),
                );
                continue;
            }
            LocateOutcome::Unparseable { error, .. } => {
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: format!("cannot parse cargo locate-project output: {error}"),
                });
                unresolved_stage.insert(manifest.clone(), ("locate", UnknownCapability::Unproven));
                continue;
            }
            LocateOutcome::WorkerFailed => {
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: "cargo locate-project could not start: parallel locate worker failed"
                        .into(),
                });
                unresolved_stage.insert(manifest.clone(), ("locate", UnknownCapability::Unproven));
                continue;
            }
            LocateOutcome::Located { canonical_key, .. } => canonical_key,
        };
        // The memo: this workspace root has already been resolved, so this
        // manifest is one more member of it and needs no metadata of its own.
        //
        // It used to be a *second* map beside the resolved-workspace cache,
        // whose value was never read and whose key set was always the cache's
        // own — so the check below always fired first and the second map's
        // own check was unreachable. One map now, so the two cannot disagree.
        if cache.contains_key(canonical_key) {
            counters.deduped_workspace_hits += 1;
            // Seed this manifest as a known member for future lookups.
            // `manifest_key` *is* `canonical_manifest_key(manifest)`, already
            // computed above; re-canonicalizing it is a syscall for nothing.
            authoritative_coverage.insert(manifest_key.clone());
            member_to_root.insert(manifest_key, canonical_key.clone());
            continue;
        }
        // Metadata once per workspace root; repeats re-account from the memo.
        // Phase 2 assigns a group to every located manifest, so `None` here
        // is unreachable; fail closed with a diagnostic (never a panic) so a
        // logic regression stays inside the 0/1/2 exit contract (M-8). The
        // manifest keeps no coverage and lands in `unresolved`, blocking
        // cleanup scope rather than attributing it to a wrong workspace.
        let Some(group_idx) = group_of[index] else {
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.clone()),
                message: "internal error: located manifest has no resolution group".into(),
            });
            unresolved_stage.insert(manifest.clone(), ("identity", UnknownCapability::Unproven));
            continue;
        };
        if let Some((template, _)) = failed_templates.get(canonical_key) {
            // Repeat triggers re-account the call and the diagnostic like a
            // fresh spawn, but wall time is added once per unique root:
            // re-adding the memo nanos per manifest multiplied one bad root's
            // timing by its manifest count (L-6).
            counters.cargo_metadata_calls += 1;
            if template.counts_as_failure {
                counters.cargo_failures += 1;
                observer.cargo_failure();
            }
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.clone()),
                message: template.message.clone(),
            });
            unresolved_stage.insert(manifest.clone(), ("metadata", UnknownCapability::Unproven));
            continue;
        }
        counters.cargo_metadata_calls += 1;
        counters.cargo_metadata_nanos = counters
            .cargo_metadata_nanos
            .saturating_add(metadata_spawns[group_idx].nanos);
        let (workspace, failure) = complete_workspace_from_metadata(
            &metadata_spawns[group_idx].result,
            canonical_key,
            manifest,
            counters,
            diagnostics,
            observer,
        );
        if let Some(ws) = workspace {
            counters.unique_workspaces += 1;
            observer.workspaces_resolved(1);
            ordered_keys.push(canonical_key.clone());
            // Seed authoritative member cache from Cargo metadata (C002 §7.6).
            member_to_root.insert(canonical_key.clone(), canonical_key.clone());
            member_to_root.insert(manifest_key, canonical_key.clone());
            for member in &ws.members {
                let key = canonical_manifest_key(&member.manifest_path);
                authoritative_coverage.insert(key.clone());
                member_to_root.entry(key).or_insert(canonical_key.clone());
            }
            // Also register the canonical root manifest path itself.
            let root_key = canonical_manifest_key(&ws.root_manifest);
            authoritative_coverage.insert(root_key.clone());
            member_to_root
                .entry(root_key)
                .or_insert(canonical_key.clone());
            cache.insert(canonical_key.clone(), ws);
        } else if let Some(template) = failure {
            failed_templates.insert(
                canonical_key.clone(),
                (template, metadata_spawns[group_idx].nanos),
            );
            unresolved_stage.insert(manifest.clone(), ("metadata", UnknownCapability::Unproven));
        }
    }
    let workspaces: Vec<_> = ordered_keys
        .into_iter()
        .filter_map(|k| cache.remove(&k))
        .collect();
    let mut unresolved: Vec<_> = manifests
        .iter()
        .filter_map(|manifest| {
            let key = canonical_manifest_key(manifest);
            if authoritative_coverage.contains(&key) {
                return None;
            }
            let message = diagnostics
                .iter()
                .rev()
                .find(|d| d.path.as_ref() == Some(manifest))
                .map(|d| d.message.clone())
                .unwrap_or_else(|| {
                    "no successful Cargo workspace metadata covered this manifest".into()
                });
            // Stage and capability are recorded where the diagnostic is pushed,
            // not re-parsed from message text: a copy-edit to a diagnostic
            // must not silently reclassify the stage or the merge behavior.
            let (stage, capability) = unresolved_stage
                .get(manifest)
                .copied()
                .unwrap_or(("identity", UnknownCapability::Unproven));
            Some(UnresolvedOwnershipParticipant {
                manifest: manifest.clone(),
                stage,
                reason: message,
                capability,
            })
        })
        .collect();
    unresolved.sort_by(|a, b| a.manifest.cmp(&b.manifest));
    // Sort by canonical identity first, then dedup on that same identity.
    // Canonicalizing per comparison is O(n log n) syscalls, so the plain sort
    // above orders the vector and `sort_by_cached_key` (stable) groups equal
    // identities adjacently; `dedup_by` then folds each adjacent run once, for
    // O(n) canonicalizations. Symlink/`..` spellings of one file are one
    // participant; the observation count is preserved separately in
    // `discovered_manifest_count` (L-7).
    unresolved.sort_by_cached_key(|p| canonical_manifest_key(&p.manifest));
    unresolved.dedup_by(|a, b| {
        canonical_manifest_key(&a.manifest) == canonical_manifest_key(&b.manifest)
    });
    // No enforcement pass-through here, deliberately. A refused manifest that
    // names a resolved workspace is still an unresolved participant and still
    // blocks the whole scope under ADR 001 C004/C006: withholding it from
    // `unresolved` would narrow the block without ADR cover, which is exactly
    // what M015's non-goals forbid. Report-only attribution for that shape
    // lives in [`attribute_unknown`] (passenger of the named island), and any
    // future enforcement belongs to ADR-004-gated activation (M016), not to
    // resolution. Permission, transport, parse, and metadata failures are
    // untouched, as is every other failure class.
    ResolutionCoverage {
        workspaces,
        unresolved,
        discovered_manifest_count: manifests.len(),
    }
}

fn resolve_one_workspace_cached(
    root_manifest: &Path,
    canonical_key: &Path,
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
    original_manifest: &Path,
) -> Option<ResolvedWorkspace> {
    let spawn = metadata_one(root_manifest, runner);
    counters.cargo_metadata_calls += 1;
    counters.cargo_metadata_nanos = counters.cargo_metadata_nanos.saturating_add(spawn.nanos);
    let (workspace, _) = complete_workspace_from_metadata(
        &spawn.result,
        canonical_key,
        original_manifest,
        counters,
        diagnostics,
        observer,
    );
    workspace
}

/// Finish one workspace resolution from an already-spawned `cargo metadata`
/// result.
///
/// This is the post-spawn half of [`resolve_one_workspace_cached`], shared by
/// the single-workspace refresh path and the bulk emulation: identical
/// parsing, member validation, diagnostics, and failure accounting for the
/// same output. Returns the workspace and, on failure, the path-independent
/// failure record a repeat trigger re-accounts (see [`MetadataFailure`]).
/// Diagnostic paths name `original_manifest`, so the same record replays for
/// any later trigger of the same root.
#[allow(clippy::too_many_arguments)]
fn complete_workspace_from_metadata(
    result: &Result<ProcessOutput, String>,
    canonical_key: &Path,
    original_manifest: &Path,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> (Option<ResolvedWorkspace>, Option<MetadataFailure>) {
    let meta_out = match result {
        Ok(output) => output,
        Err(error) => {
            let message = format!("cargo metadata could not start: {error}");
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: message.clone(),
            });
            return (
                None,
                Some(MetadataFailure {
                    message,
                    counts_as_failure: true,
                }),
            );
        }
    };
    if !meta_out.success {
        let message = format!(
            "cargo metadata failed: {}",
            cargo_failure_reason(&meta_out.stderr)
        );
        counters.cargo_failures += 1;
        observer.cargo_failure();
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(original_manifest.to_path_buf()),
            message: message.clone(),
        });
        return (
            None,
            Some(MetadataFailure {
                message,
                counts_as_failure: true,
            }),
        );
    }
    let stdout_str = String::from_utf8_lossy(&meta_out.stdout);
    // JSON field pattern, not a substring: a package name or path containing
    // `build_directory` must not flip `Unavailable` to `Unknown` (L-5).
    let had_build = stdout_str.contains("\"build_directory\":");
    let raw: RawMetadata = match serde_json::from_slice(&meta_out.stdout) {
        Ok(v) => v,
        Err(e) => {
            let message = format!("cannot parse cargo metadata output: {e}");
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: message.clone(),
            });
            return (
                None,
                Some(MetadataFailure {
                    message,
                    counts_as_failure: true,
                }),
            );
        }
    };
    let build_opt = raw.build_directory.as_deref();
    let capability = capability_from_metadata(
        had_build,
        &raw.target_directory,
        build_opt,
        cargo_env_build_dir_set(),
    );
    let workspace_root = match canonical_or_absolute(Path::new(&raw.workspace_root)) {
        Ok(p) => p,
        Err(message) => {
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: message.clone(),
            });
            return (
                None,
                Some(MetadataFailure {
                    message,
                    counts_as_failure: false,
                }),
            );
        }
    };
    let root_manifest_canonical = canonical_key.to_path_buf();
    let mut members = Vec::new();
    for pkg in &raw.packages {
        let mp = PathBuf::from(&pkg.manifest_path);
        let source_root = mp
            .parent()
            .and_then(|p| fs::canonicalize(p).ok())
            .unwrap_or_else(|| mp.clone());
        if !source_root.is_absolute() || !source_root.is_dir() {
            let message = format!(
                "cannot establish physical workspace member source root for {}",
                mp.display()
            );
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: message.clone(),
            });
            return (
                None,
                Some(MetadataFailure {
                    message,
                    counts_as_failure: false,
                }),
            );
        }
        members.push(WorkspaceMember {
            manifest_path: mp,
            source_root,
        });
    }
    if members.is_empty() {
        let source_root = root_manifest_canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.clone());
        members.push(WorkspaceMember {
            manifest_path: root_manifest_canonical.clone(),
            source_root,
        });
    }
    members.sort_by(|a, b| a.source_root.cmp(&b.source_root));
    members.dedup_by(|a, b| a.source_root == b.source_root);

    let target_logical = PathBuf::from(&raw.target_directory);
    let build_logical = build_opt
        .map(PathBuf::from)
        .unwrap_or_else(|| target_logical.clone());
    let target_root = output_root(OutputRootKind::Target, target_logical, diagnostics);
    let build_root = output_root(OutputRootKind::Build, build_logical, diagnostics);

    let mut packages: Vec<WorkspacePackage> = raw
        .packages
        .iter()
        .map(|pkg| WorkspacePackage {
            id: pkg.id.clone(),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            manifest_path: PathBuf::from(&pkg.manifest_path),
        })
        .collect();
    packages.sort();
    packages.dedup();

    (
        Some(ResolvedWorkspace {
            id: WorkspaceId(workspace_root.clone()),
            root: workspace_root,
            root_manifest: root_manifest_canonical,
            members,
            packages,
            output: OutputSet {
                target: target_root,
                build: build_root,
            },
            capability,
        }),
        None,
    )
}

// Silence dead-code for the single-workspace helper used by tests/future reuse.
/// Returns true if `maybe_child` equals or is contained in `parent`.
fn contains(parent: &Path, maybe_child: &Path) -> bool {
    maybe_child == parent || maybe_child.starts_with(parent)
}

/// Cargo's conventional source directories.
///
/// A declared output root with one of these names is a configuration mistake,
/// not a build artifact: `target-dir = "src"` would make the tool delete the
/// crate's own sources. This is a name-based guard on top of the structural
/// `contains` check, and it fails safe - it can only ever *withhold* a cleanup,
/// never authorize one.
fn is_conventional_source_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| matches!(name, "src" | "tests" | "benches" | "examples"))
}

/// True when `dir` holds a Cargo manifest, i.e. it is a package directory.
///
/// The manifest name is the one the whole tool shares
/// ([`crate::domain::is_manifest_shaped_name`]), so the inventory and the
/// ownership proof cannot disagree about what a package directory is: a
/// directory the discovery calls a package directory is always one this guard
/// refuses to delete. The rule is deliberately broader than the discovery's
/// own ("would Cargo open this file?"): a cleanup would take a manifest-shaped
/// file with it whatever Cargo would have made of it.
fn is_package_directory(dir: &Path) -> bool {
    fs::read_dir(dir).ok().is_some_and(|mut children| {
        children.any(|child| {
            child
                .ok()
                .is_some_and(|child| crate::domain::is_manifest_shaped_name(&child.file_name()))
        })
    })
}

/// Cargo's conventional source directories, relative to a package root.
const SOURCE_DIR_NAMES: [&str; 4] = ["src", "tests", "benches", "examples"];

/// True when `covering` lies inside a workspace's source tree.
///
/// The conventional source directories are where a crate's own files live. An
/// output root *inside* one of them is that crate's source, not a build
/// artifact: `cargo clean --target-dir <root>` removes the whole root, so
/// `target-dir = "src/foo"` would delete the crate's `.rs` files.
///
/// [`is_conventional_source_dir`] is the name-based guard and only sees the
/// covering root's own final component, so `src/foo` slips past it. This is
/// the containment form of the same guard, and it fails safe - it can only
/// ever *withhold* a cleanup, never authorize one.
pub(crate) fn inside_source_tree(covering: &Path, workspaces: &[ResolvedWorkspace]) -> bool {
    inside_source_tree_dirs(covering, &source_tree_dirs(workspaces))
}

/// The conventional source directories of every resolved workspace.
///
/// The set does not depend on the covering root being tested, so it is
/// computed once per scan rather than once per group: the old form
/// allocated eight `PathBuf`s per workspace member for every group it
/// classified.
fn source_tree_dirs(workspaces: &[ResolvedWorkspace]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for w in workspaces {
        for m in &w.members {
            for name in SOURCE_DIR_NAMES {
                dirs.push(m.source_root.join(name));
            }
        }
        for name in SOURCE_DIR_NAMES {
            dirs.push(w.root.join(name));
        }
    }
    dirs
}

/// [`inside_source_tree`] against an already-computed source-tree set.
fn inside_source_tree_dirs(covering: &Path, source_dirs: &[PathBuf]) -> bool {
    source_dirs.iter().any(|dir| contains(dir, covering))
}

/// True when a covering root is nested inside a package directory.
///
/// This is the mirror of the structural `contains` check: that one refuses an
/// output root that *contains* a member's source tree, and this one refuses an
/// output root that *lives inside* one. `cargo clean` removes whatever is under
/// the root it is given, so a target directory pointed at another package's
/// tree destroys that package's checked-in files.
///
/// The victim does not have to be a workspace the tool could resolve. Cargo
/// refuses a nested package that is not a workspace member ("current package
/// believes it's in a workspace when it's not"), so the package sitting inside
/// the output root is exactly the one the resolved universe does *not* contain:
/// the resolved-member check cannot see it, and the filesystem can.
///
/// The owning workspace's own root is the boundary. `<ws>/target` sits inside
/// the owner's package directory, and that is the ordinary case; the walk stops
/// there and never asks whether the workspace root is itself a package.
pub(crate) fn nested_package_overlap(covering: &[PathBuf], workspace_root: &Path) -> bool {
    covering.iter().any(|root| {
        if !strictly_below(workspace_root, root) {
            return false;
        }
        let mut dir = root.parent();
        while let Some(candidate) = dir {
            if candidate == workspace_root {
                break;
            }
            // The walk never leaves the owning workspace's tree.
            if !candidate.starts_with(workspace_root) {
                break;
            }
            if is_package_directory(candidate) {
                return true;
            }
            dir = candidate.parent();
        }
        false
    })
}

/// Returns true if `maybe_child` is strictly below `parent`.
///
/// `contains` is deliberately inclusive because grouping needs it; an *artifact*
/// must instead sit strictly inside its workspace, so the workspace root itself
/// can never be an output root.
fn strictly_below(parent: &Path, maybe_child: &Path) -> bool {
    maybe_child != parent && maybe_child.starts_with(parent)
}

/// Compute minimal outermost canonical roots (those not contained in another).
pub fn outermost(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = roots.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut out = Vec::new();
    for candidate in sorted {
        if out
            .iter()
            .any(|outer: &PathBuf| contains(outer, &candidate))
        {
            continue;
        }
        // Remove any existing that are contained in candidate (keep outer).
        out.retain(|existing: &PathBuf| !contains(&candidate, existing));
        out.push(candidate);
    }
    out.sort();
    out
}

/// Best-effort physical aliases of an output root with no provable identity.
///
/// The declared logical spelling always names where the root was said to be. A
/// symlink additionally contributes its link target when the link can be read
/// and the target canonicalized, so a link *into* a group's covering still
/// blocks that group (a purely logical check would miss it). When the target
/// cannot be resolved only the logical spelling is known, which is still enough
/// to block a group covering that spelling. This is scan-time grouping only;
/// cleanup re-proves ownership immediately before each spawn.
fn uncertain_root_aliases(root: &OutputRoot) -> Vec<PathBuf> {
    let mut aliases = vec![root.logical_path.clone()];
    let target = if root.is_symlink {
        fs::read_link(&root.logical_path).ok().map(|t| {
            if t.is_absolute() {
                t
            } else {
                root.logical_path.parent().map(|p| p.join(&t)).unwrap_or(t)
            }
        })
    } else {
        Some(root.logical_path.clone())
    };
    if let Some(t) = target
        && let Ok(canonical) = fs::canonicalize(&t)
        && !aliases.contains(&canonical)
    {
        aliases.push(canonical);
    }
    aliases
}

/// True when an unrepresentable root may alias the group: any of its known
/// aliases contains, or sits inside, a covering root. Either direction means
/// `cargo clean` on the covering root could destroy the other workspace's
/// bytes, so the group cannot claim exclusivity.
fn group_overlaps_uncertain(covering: &[PathBuf], uncertain_aliases: &[PathBuf]) -> bool {
    uncertain_aliases
        .iter()
        .any(|u| covering.iter().any(|c| contains(u, c) || contains(c, u)))
}

/// Build physical-output groups from resolved workspaces.
///
/// Only existing non-symlink canonical roots participate. Symlink roots make
/// their workspace uncertain. Equal/ancestor-descendant overlaps form connected
/// groups. Classification:
/// - Uncertain: symlink, unresolvable, or missing physical identity, or an
///   output root that is (or contains) any resolved workspace's member source
///   tree, or one that is nested inside a package directory, or one of Cargo's
///   conventional source directories (`src`, `tests`, `benches`, `examples`).
/// - Shared: multiple workspaces own overlapping physical output, or the group's
///   covering overlaps an unrepresentable root's known aliases, so exclusivity
///   is unprovable for that group.
/// - PrivateBounded: single owner, every covering root strictly below the
///   workspace root, disjoint from every resolved member source root, and
///   disjoint from every unrepresentable root's known aliases.
/// - ExternalUnproven: single owner, output outside workspace root.
pub fn build_groups(workspaces: &[ResolvedWorkspace]) -> Vec<RawGroup> {
    // Collect (workspace_idx, physical_path) for existing non-symlink roots.
    #[derive(Clone)]
    struct Node {
        ws: usize,
        physical: PathBuf,
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut uncertain_ws: HashSet<usize> = HashSet::new();
    // Known aliases of roots with no provable physical identity (M-1): always
    // the declared logical spelling, plus a resolved link target when one can
    // be read and canonicalized. A group disjoint from every alias cannot be
    // the same bytes, so only an overlapping group loses its private claim.
    let mut uncertain_aliases: Vec<PathBuf> = Vec::new();
    for (idx, ws) in workspaces.iter().enumerate() {
        // Equal target/build share one physical identity; pushing it twice
        // only doubles union-find work before the later dedup (L-8). Collapse
        // only when both iterations would push the identical node, so no
        // uncertainty marker is ever lost.
        let build_duplicates_target = !ws.output.target.is_symlink
            && !ws.output.build.is_symlink
            && ws.output.target.exists
            && ws.output.build.exists
            && ws.output.target.physical_path.is_some()
            && ws.output.target.physical_path == ws.output.build.physical_path;
        for root in std::iter::once(&ws.output.target)
            .chain((!build_duplicates_target).then_some(&ws.output.build))
        {
            if root.is_symlink {
                uncertain_ws.insert(idx);
                uncertain_aliases.extend(uncertain_root_aliases(root));
                continue;
            }
            if !root.exists {
                continue;
            }
            if let Some(physical) = &root.physical_path {
                nodes.push(Node {
                    ws: idx,
                    physical: physical.clone(),
                });
            } else {
                uncertain_ws.insert(idx);
                uncertain_aliases.extend(uncertain_root_aliases(root));
            }
        }
    }
    // Union-find over nodes by overlap.
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }
    // The overlap relation is "one path contains the other", and containment
    // runs down a single chain of ancestors: a node that contains another
    // node is an ancestor of it. So a node reaches its group through the
    // marked ancestors above it, and unioning each node with its nearest
    // marked ancestor connects exactly the pairs the all-pairs comparison
    // connected -- every comparable pair is joined by the chain of marked
    // ancestors between them -- without comparing every pair.
    //
    // The same physical path twice is one identity, and the all-pairs
    // comparison unioned the two, so the duplicates are unioned here too.
    let mut marked: HashMap<&Path, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        match marked.get(node.physical.as_path()) {
            Some(first) => union(&mut parent, i, *first),
            None => {
                marked.insert(node.physical.as_path(), i);
            }
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        let mut ancestor = node.physical.parent();
        while let Some(candidate) = ancestor {
            if let Some(j) = marked.get(candidate) {
                union(&mut parent, i, *j);
                break;
            }
            ancestor = candidate.parent();
        }
    }
    let mut groups_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..nodes.len() {
        let r = find(&mut parent, i);
        groups_map.entry(r).or_default().push(i);
    }
    let mut groups = Vec::new();
    // The source-tree directories of every resolved workspace. They are the
    // same for every group, so they are computed once here rather than once
    // per group and covering root.
    let source_dirs = source_tree_dirs(workspaces);
    // A root that contributes no node (symlink, no physical identity) is
    // invisible to the union-find above, yet it can still point into a group.
    // Exclusivity is therefore per group (M-1): only a group overlapped by an
    // unrepresentable root's known aliases loses `PrivateBounded`.
    for (_, idxs) in groups_map {
        let mut physicals: Vec<PathBuf> = idxs.iter().map(|i| nodes[*i].physical.clone()).collect();
        physicals.sort();
        physicals.dedup();
        let mut owners: Vec<usize> = idxs.iter().map(|i| nodes[*i].ws).collect();
        owners.sort();
        owners.dedup();
        let covering = outermost(&physicals);
        // Primary display: first covering root sorted.
        let display = covering
            .first()
            .cloned()
            .unwrap_or_else(|| physicals[0].clone());
        // Classification.
        let mut source_overlap = false;
        let mut nested_source_overlap = false;
        let ownership = if owners.len() > 1 {
            OutputOwnershipClass::Shared
        } else {
            let ws = &workspaces[owners[0]];
            // A covering root that is the source tree itself, or an ancestor of
            // a member source root, is not an artifact: cleaning it would
            // delete the project. `contains` is inclusive, so equality counts.
            //
            // Every workspace in the universe is consulted, not only the
            // group's own owner: `cargo clean` deletes whatever is under the
            // root, so a second resolved project whose *sources* live inside it
            // is destroyed just as surely as the owner's own. Consulting only
            // `ws.members` would ratify exactly the shape this refuses.
            source_overlap = covering.iter().any(|c| {
                workspaces
                    .iter()
                    .any(|w| w.members.iter().any(|m| contains(c, &m.source_root)))
                    || is_conventional_source_dir(c)
                    // The mirror of the containment above: an output root that
                    // lives *inside* a crate's source tree is that crate's
                    // source, not a build artifact.
                    || inside_source_tree_dirs(c, &source_dirs)
            });
            // The mirror: a covering root that lives *inside* a package
            // directory. The resolved members cannot see this shape — Cargo
            // refuses a nested package that is not a workspace member, so the
            // victim is exactly the package the universe does not contain.
            nested_source_overlap = nested_package_overlap(&covering, &ws.root);
            source_overlap |= nested_source_overlap;
            let inside_root = covering.iter().all(|c| strictly_below(&ws.root, c));
            if uncertain_ws.contains(&owners[0]) || source_overlap {
                // The owning workspace is uncertain, or its own source tree
                // would be deleted. Never a private artifact.
                OutputOwnershipClass::Uncertain
            } else if inside_root && !group_overlaps_uncertain(&covering, &uncertain_aliases) {
                OutputOwnershipClass::PrivateBounded
            } else if inside_root {
                // Inside the workspace root but overlapped by an
                // unrepresentable root: exclusivity is unprovable for this
                // group, while disjoint groups keep their private claim.
                OutputOwnershipClass::Shared
            } else {
                OutputOwnershipClass::ExternalUnproven
            }
        };
        groups.push(RawGroup {
            physicals,
            covering,
            display,
            owners,
            ownership,
            source_overlap,
            nested_source_overlap,
        });
    }
    // Deterministic order by display path.
    groups.sort_by(|a, b| a.display.cmp(&b.display));
    groups
}

#[derive(Clone, Debug)]
pub struct RawGroup {
    pub physicals: Vec<PathBuf>,
    pub covering: Vec<PathBuf>,
    pub display: PathBuf,
    pub owners: Vec<usize>,
    pub ownership: OutputOwnershipClass,
    /// A covering root is (or contains) a workspace member source root, or is
    /// nested inside a package directory, so this group is source code rather
    /// than an artifact. Surfaced as a diagnostic: the group is dropped from
    /// the inventory, so silence would be misleading.
    pub source_overlap: bool,
    /// A covering root is nested inside a package directory.
    ///
    /// The mirror of [`Self::source_overlap`]'s first case: the root does not
    /// *contain* a source tree, it *lives inside* one. Kept separate so the
    /// diagnostic can name the shape the operator has to fix.
    pub nested_source_overlap: bool,
}

/// Cheap bounded artifact-presence check: true if the directory contains at
/// least one entry. Missing/unreadable returns None (caller treats as skip).
pub fn has_artifact_entries(path: &Path) -> Option<bool> {
    let mut entries = fs::read_dir(path).ok()?;
    Some(entries.next().is_some())
}

pub struct GroupAnalysisInput {
    pub group: RawGroup,
    pub owner_workspaces: Vec<ResolvedWorkspace>,
}

pub fn workspace_member_roots(ws: &ResolvedWorkspace) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ws.members.iter().map(|m| m.source_root.clone()).collect();
    roots.sort();
    roots.dedup();
    outermost(&roots)
}

/// Why a physical output group did not become a measured candidate (C003 §7.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupSkipReason {
    MissingOutput,
    EmptyOutput,
    ActiveSource,
    UncertainSource,
    UncertainOwnership,
    UncertainMeasurement,
    ActiveOutput,
}

impl GroupSkipReason {
    pub fn detail(self) -> &'static str {
        match self {
            Self::MissingOutput => "output directory is missing",
            Self::EmptyOutput => "output directory contains no artifacts",
            Self::ActiveSource => "workspace source activity is recent",
            Self::UncertainSource => "workspace source activity could not be established",
            Self::UncertainOwnership => "output ownership could not be established",
            Self::UncertainMeasurement => "output activity or size could not be established",
            Self::ActiveOutput => "output activity is recent",
        }
    }
}

/// Per-group result of the fail-fast pipeline (C003 §7.2).
///
/// Either the measured candidate, or the blocking reason. A workspace
/// CleanupUnit is authorized only when every physical group it can affect has
/// a measured candidate.
#[derive(Clone, Debug)]
pub struct GroupOutcome {
    pub group: RawGroup,
    pub measured: Option<PhysicalOutputGroup>,
    pub skip: Option<GroupSkipReason>,
}

/// Full fail-fast pipeline for resolved workspaces, per group.
///
/// Gate order: existence/type -> artifact presence -> source activity -> deep
/// sizing. Returns one outcome per input group in deterministic display order.
///
/// C003: the skip reason is retained so a workspace-level cleanup decision can
/// name the sibling output group that blocks the whole unit, instead of
/// silently dropping one physical group from a multi-group OutputSet.
#[allow(clippy::too_many_arguments)]
pub fn analyze_groups_detailed(
    workspaces: &[ResolvedWorkspace],
    groups: Vec<RawGroup>,
    clock_start: SystemTime,
    clock_cutoff: SystemTime,
    recency: Duration,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Vec<GroupOutcome> {
    // C002 §7.5: determinate analysis total once group count is known.
    // Emitted through the observer trait so any renderer/test observer
    // observes it without knowing the concrete type.
    observer.units_total(ScanPhase::Analysis, groups.len() as u64);
    // Cheap presence gate before any source walk or deep sizing.
    // We check each covering root for at least one entry; groups with no
    // entries are skipped without source/deep work.
    let mut group_has_entries: Vec<bool> = Vec::new();
    let mut group_missing_all: Vec<bool> = Vec::new();
    for g in &groups {
        let mut any = false;
        let mut missing_all = true;
        for c in &g.covering {
            match has_artifact_entries(c) {
                Some(true) => {
                    any = true;
                    missing_all = false;
                }
                Some(false) => {
                    missing_all = false;
                }
                None => {}
            }
        }
        if !any {
            if missing_all {
                counters.missing_output_skipped += 1;
            } else {
                counters.empty_no_output_skipped += 1;
                observer.empty_skipped();
            }
        }
        group_has_entries.push(any);
        group_missing_all.push(missing_all);
    }

    // Source activity per workspace (cached), excluding all resolved outputs.
    // Avoid duplicate traversal for overlapping member roots via outermost.
    let mut ws_source_recent: HashMap<usize, bool> = HashMap::new();
    let mut ws_source_uncertain: HashSet<usize> = HashSet::new();
    // Collect all output roots for exclusion (existing only).
    let mut all_outputs: Vec<PathBuf> = Vec::new();
    for ws in workspaces {
        for r in [&ws.output.target, &ws.output.build] {
            if let Some(p) = &r.physical_path {
                all_outputs.push(p.clone());
            } else {
                all_outputs.push(r.logical_path.clone());
            }
        }
    }
    // Workspaces owning at least one non-empty group, computed in one pass.
    // The alternative -- asking `groups.iter().any(...)` per workspace -- made
    // this stage quadratic in workspaces × groups before any source walk.
    let mut ws_owns_nonempty: HashSet<usize> = HashSet::new();
    for (gi, g) in groups.iter().enumerate() {
        if *group_has_entries.get(gi).unwrap_or(&false) {
            ws_owns_nonempty.extend(g.owners.iter().copied());
        }
    }
    for (wi, ws) in workspaces.iter().enumerate() {
        // If workspace has no existing output at all, it cannot be eligible;
        // skip source walk entirely (fail-fast).
        let has_existing = ws.output.target.exists || ws.output.build.exists;
        if !has_existing {
            counters.missing_output_skipped += 1;
            ws_source_recent.insert(wi, false);
            continue;
        }
        // If all groups owned by this workspace are already empty, skip source.
        if !ws_owns_nonempty.contains(&wi) {
            // Already counted as empty above; no source walk.
            ws_source_recent.insert(wi, false);
            continue;
        }
        let source_start = std::time::Instant::now();
        let activity = workspace_source_activity(ws, &all_outputs, clock_start, clock_cutoff);
        counters.source_activity_nanos = counters
            .source_activity_nanos
            .saturating_add(crate::domain::elapsed_nanos(source_start.elapsed()));
        match activity {
            Ok(true) => {
                ws_source_recent.insert(wi, true);
                // Progress reports one active workspace; the *counter* is bumped
                // per skipped group below, because `stats_line` compares it with
                // `groups_measured`. Counting workspaces here under-reported a
                // workspace that owns two skipped groups as one.
                observer.active_skipped();
            }
            Ok(false) => {
                ws_source_recent.insert(wi, false);
            }
            Err(()) => {
                ws_source_uncertain.insert(wi);
                ws_source_recent.insert(wi, false);
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(ws.root.clone()),
                    message: "workspace source activity could not be established".into(),
                });
            }
        }
    }

    // Deep sizing only for survivors: groups with entries, all owners
    // source-quiet and certain, and not already uncertain.
    let mut survivor_covering: Vec<PathBuf> = Vec::new();
    let mut survivor_group_idx: Vec<usize> = Vec::new();
    let mut pre_skip: Vec<Option<GroupSkipReason>> = vec![None; groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        if !*group_has_entries.get(gi).unwrap_or(&false) {
            pre_skip[gi] = Some(if *group_missing_all.get(gi).unwrap_or(&false) {
                GroupSkipReason::MissingOutput
            } else {
                GroupSkipReason::EmptyOutput
            });
            continue;
        }
        // If any owner source-recent, skip group without sizing.
        // Counted per *group*, alongside every other skip reason below, so the
        // counter is comparable with `groups_measured`.
        if g.owners
            .iter()
            .any(|o| *ws_source_recent.get(o).unwrap_or(&false))
        {
            counters.active_skipped += 1;
            pre_skip[gi] = Some(GroupSkipReason::ActiveSource);
            continue;
        }
        if g.owners.iter().any(|o| ws_source_uncertain.contains(o)) {
            counters.uncertain_skipped += 1;
            pre_skip[gi] = Some(GroupSkipReason::UncertainSource);
            continue;
        }
        if g.ownership == OutputOwnershipClass::Uncertain {
            counters.uncertain_skipped += 1;
            pre_skip[gi] = Some(GroupSkipReason::UncertainOwnership);
            if g.source_overlap {
                // Never silently drop a project whose output root is its own
                // source tree: the user configured `target-dir` that way and
                // needs to know why nothing is reported or cleaned.
                let message = if g.nested_source_overlap {
                    format!(
                        "output root {} lies inside a package directory, not a build artifact; it is not sized and not eligible for cleanup",
                        g.display.display()
                    )
                } else {
                    format!(
                        "output root {} contains a workspace source tree, not a build artifact; it is not sized and not eligible for cleanup",
                        g.display.display()
                    )
                };
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(g.display.clone()),
                    message,
                });
            }
            continue;
        }
        for c in &g.covering {
            survivor_covering.push(c.clone());
            survivor_group_idx.push(gi);
        }
    }
    // Measure survivors with one bounded pool.
    let measure_targets: Vec<(usize, PathBuf)> = survivor_covering
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.clone()))
        .collect();
    let sizing_start = std::time::Instant::now();
    let measured = if measure_targets.is_empty() {
        Vec::new()
    } else {
        traverse::measure_many_targets(&measure_targets)
    };
    counters.output_sizing_nanos = counters
        .output_sizing_nanos
        .saturating_add(crate::domain::elapsed_nanos(sizing_start.elapsed()));
    let mut bytes_by_survivor: HashMap<usize, traverse::TargetStats> = HashMap::new();
    for (i, stats) in measured {
        bytes_by_survivor.insert(i, stats);
    }
    // Aggregate per group.
    let mut outcomes: Vec<GroupOutcome> = Vec::new();
    // Map group idx -> list of survivor positions.
    let mut group_to_survivors: HashMap<usize, Vec<usize>> = HashMap::new();
    for (pos, gi) in survivor_group_idx.iter().enumerate() {
        group_to_survivors.entry(*gi).or_default().push(pos);
    }
    for (gi, g) in groups.iter().enumerate() {
        let Some(survivors) = group_to_survivors.get(&gi) else {
            // Early-skipped group (empty/active/uncertain before sizing):
            // still advances determinate progress (C002 §7.5).
            observer.unit_completed(ScanPhase::Analysis);
            outcomes.push(GroupOutcome {
                group: g.clone(),
                measured: None,
                skip: pre_skip[gi],
            });
            continue;
        };
        let mut bytes = 0u64;
        let mut entries = 0u64;
        let mut newest: Option<SystemTime> = None;
        let mut uncertain = false;
        for pos in survivors {
            if let Some(stats) = bytes_by_survivor.get(pos) {
                if stats.uncertain {
                    uncertain = true;
                    break;
                }
                bytes = bytes.saturating_add(stats.bytes);
                entries = entries.saturating_add(stats.entries);
                if let Some(t) = stats.newest {
                    newest = Some(newest.map_or(t, |old| old.max(t)));
                }
            } else {
                uncertain = true;
                break;
            }
        }
        if uncertain {
            counters.uncertain_skipped += 1;
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(g.display.clone()),
                message: "output activity or size could not be established".into(),
            });
            observer.unit_completed(ScanPhase::Analysis);
            outcomes.push(GroupOutcome {
                group: g.clone(),
                measured: None,
                skip: Some(GroupSkipReason::UncertainMeasurement),
            });
            continue;
        }
        // A covering root that holds a package directory is not a build
        // artifact. `cargo clean` removes whatever is under the root it is
        // given, so the package's checked-in sources would go with it, and the
        // discovery cannot see the package: it does not descend into an
        // output root, so the victim is exactly the package the resolved
        // universe does not contain.
        //
        // The traversal that sized the root visited every entry under it, so
        // this costs nothing extra.
        let package_dir = survivors.iter().find_map(|pos| {
            bytes_by_survivor
                .get(pos)
                .and_then(|stats| stats.package_dir.clone())
        });
        if let Some(package_dir) = package_dir {
            counters.uncertain_skipped += 1;
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(g.display.clone()),
                message: format!(
                    "output root {} holds the package directory {}, not only build output; it is not sized and not eligible for cleanup",
                    g.display.display(),
                    package_dir.display()
                ),
            });
            observer.unit_completed(ScanPhase::Analysis);
            outcomes.push(GroupOutcome {
                group: RawGroup {
                    ownership: OutputOwnershipClass::Uncertain,
                    source_overlap: true,
                    ..g.clone()
                },
                measured: None,
                skip: Some(GroupSkipReason::UncertainOwnership),
            });
            continue;
        }
        if entries == 0 {
            counters.empty_no_output_skipped += 1;
            observer.empty_skipped();
            observer.unit_completed(ScanPhase::Analysis);
            outcomes.push(GroupOutcome {
                group: g.clone(),
                measured: None,
                skip: Some(GroupSkipReason::EmptyOutput),
            });
            continue;
        }
        // Output activity gate: recent output protects group.
        //
        // The cutoff is `clock_start - recency` with an *unsigned* recency, so
        // `clock_cutoff <= clock_start` and "modified since the scan began" is
        // already inside "modified since the cutoff". This used to carry a
        // second disjunct for the former; it was subsumed by the first and
        // could never change the verdict.
        let is_recent = newest.is_some_and(|t| t >= clock_cutoff);
        if is_recent {
            counters.active_skipped += 1;
            observer.active_skipped();
            observer.unit_completed(ScanPhase::Analysis);
            outcomes.push(GroupOutcome {
                group: g.clone(),
                measured: None,
                skip: Some(GroupSkipReason::ActiveOutput),
            });
            continue;
        }
        // Survivor.
        counters.groups_measured += 1;
        counters.bytes_measured = counters.bytes_measured.saturating_add(bytes);
        observer.group_measured(bytes);
        observer.unit_completed(ScanPhase::Analysis);
        let owners: Vec<WorkspaceId> = g.owners.iter().map(|o| workspaces[*o].id.clone()).collect();
        let metric = {
            #[cfg(any(unix, windows))]
            {
                SizeMetric::Allocated
            }
            #[cfg(not(any(unix, windows)))]
            {
                SizeMetric::Apparent
            }
        };
        // `recency` is deliberately not consulted here: the inactivity window is
        // applied by the caller through `clock_cutoff`. Accepting a duration and
        // ignoring it invited the belief that the policy lived here.
        let _ = recency;
        let physical = PhysicalOutputGroup {
            covering_roots: g.covering.clone(),
            physical_paths: g.physicals.clone(),
            display_path: g.display.clone(),
            owners,
            ownership: g.ownership,
            bytes,
            metric,
            newest_mtime: newest,
            artifact_entries: entries,
            uncertain: false,
        };
        observer.reportable_group(&g.display, bytes);
        counters.reportable_groups += 1;
        outcomes.push(GroupOutcome {
            group: g.clone(),
            measured: Some(physical),
            skip: None,
        });
    }
    outcomes
}

/// Measured candidates only, in deterministic size-descending order.
///
/// The read-only scan report is physical-group oriented and unchanged by C003.
#[allow(clippy::too_many_arguments)]
pub fn analyze_groups(
    workspaces: &[ResolvedWorkspace],
    groups: Vec<RawGroup>,
    clock_start: SystemTime,
    clock_cutoff: SystemTime,
    recency: Duration,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Vec<PhysicalOutputGroup> {
    let mut eligible: Vec<PhysicalOutputGroup> = analyze_groups_detailed(
        workspaces,
        groups,
        clock_start,
        clock_cutoff,
        recency,
        counters,
        diagnostics,
        observer,
    )
    .into_iter()
    .filter_map(|o| o.measured)
    .collect();
    // Deterministic size-descending with stable path tie-break.
    eligible.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.display_path.cmp(&b.display_path))
    });
    eligible
}

/// Physical groups a workspace's complete OutputSet can touch (C003 §4).
///
/// Every existing, provably-identified physical target/build root of one
/// workspace maps to exactly one physical group. Equal or nested roots map to
/// the same group and are therefore counted once. Missing roots (nothing for
/// Cargo clean to remove) and unprovable roots (symlink or unresolvable
/// identity, which `build_groups` excludes) are not affected groups.
///
/// `physical path → group index` for one group set.
///
/// Built once and shared. `build_cleanup_units` calls the mapper per workspace,
/// and rebuilding the whole map each time made a Full scan quadratic in
/// workspaces × groups before it did any work.
fn physical_group_index(groups: &[RawGroup]) -> HashMap<&Path, usize> {
    let mut by_physical: HashMap<&Path, usize> = HashMap::new();
    for (gi, g) in groups.iter().enumerate() {
        for p in &g.physicals {
            by_physical.insert(p.as_path(), gi);
        }
    }
    by_physical
}

/// Returns group indices in deterministic display order.
pub fn map_workspace_groups(ws: &ResolvedWorkspace, groups: &[RawGroup]) -> Vec<usize> {
    map_workspace_groups_indexed(ws, &physical_group_index(groups), groups)
}

fn map_workspace_groups_indexed(
    ws: &ResolvedWorkspace,
    by_physical: &HashMap<&Path, usize>,
    groups: &[RawGroup],
) -> Vec<usize> {
    let mut idxs: Vec<usize> = Vec::new();
    for root in [&ws.output.target, &ws.output.build] {
        if root.is_symlink || !root.exists {
            continue;
        }
        let Some(physical) = root.physical_path.as_ref() else {
            continue;
        };
        if let Some(&gi) = by_physical.get(physical.as_path())
            && !idxs.contains(&gi)
        {
            idxs.push(gi);
        }
    }
    idxs.sort_by(|a, b| groups[*a].display.cmp(&groups[*b].display));
    idxs
}

/// Physical output group touched by one workspace's Cargo clean invocation.
#[derive(Clone, Debug)]
pub struct AffectedGroup {
    pub display: PathBuf,
    pub covering: Vec<PathBuf>,
    pub ownership: OutputOwnershipClass,
    /// Logical output kinds (target/build) of this workspace contained in the
    /// group; both when target and build collapse into one physical group.
    pub kinds: Vec<OutputRootKind>,
    pub measured: Option<PhysicalOutputGroup>,
    pub skip: Option<GroupSkipReason>,
}

impl AffectedGroup {
    /// Human-readable output kind(s) for skip reasons.
    pub fn kind_label(&self) -> String {
        let mut kinds = self.kinds.clone();
        kinds.sort_by_key(|k| match k {
            OutputRootKind::Target => 0,
            OutputRootKind::Build => 1,
        });
        kinds.dedup();
        match kinds.as_slice() {
            [] => "output".to_owned(),
            [OutputRootKind::Target] => "target".to_owned(),
            [OutputRootKind::Build] => "build".to_owned(),
            [OutputRootKind::Target, OutputRootKind::Build] => "target+build".to_owned(),
            _ => "output".to_owned(),
        }
    }
}

/// One Cargo workspace cleanup invocation and everything it can affect
/// (C003 §4).
///
/// A CleanupUnit is the unit for destructive authorization, Cargo invocation,
/// progress, and cleanup reporting. `covering` is the deduplicated union of the
/// physical covering roots Cargo clean can affect, and `bytes` is the
/// deduplicated pre-clean size of that union.
#[derive(Clone, Debug)]
pub struct CleanupUnit {
    pub workspace_idx: usize,
    pub id: WorkspaceId,
    pub root: PathBuf,
    pub root_manifest: PathBuf,
    pub output: OutputSet,
    pub capability: CargoCapabilities,
    pub groups: Vec<AffectedGroup>,
    /// Existing output roots with unprovable physical identity; any entry blocks
    /// the whole unit because their cleanup footprint cannot be bounded.
    pub unmapped: Vec<OutputRoot>,
    pub covering: Vec<PathBuf>,
    pub bytes: u64,
}

impl CleanupUnit {
    /// Every affected group passed the fail-fast pipeline.
    pub fn fully_measured(&self) -> bool {
        self.unmapped.is_empty() && self.groups.iter().all(|g| g.measured.is_some())
    }
}

/// Build one cleanup candidate per resolved workspace (C003 §7.1).
///
/// A workspace with no measured affected output does not become a unit. Equal
/// and nested roots collapse to one physical group and are counted once.
/// Order is deterministic: descending aggregated bytes, then workspace root.
pub fn build_cleanup_units(
    workspaces: &[ResolvedWorkspace],
    outcomes: &[GroupOutcome],
) -> Vec<CleanupUnit> {
    let groups: Vec<RawGroup> = outcomes.iter().map(|o| o.group.clone()).collect();
    let by_physical = physical_group_index(&groups);
    let mut units = Vec::new();
    for (wi, ws) in workspaces.iter().enumerate() {
        let idxs = map_workspace_groups_indexed(ws, &by_physical, &groups);
        let mut unmapped: Vec<OutputRoot> = Vec::new();
        for root in [&ws.output.target, &ws.output.build] {
            if root.is_symlink || !root.exists {
                continue;
            }
            if root.physical_path.is_none() {
                unmapped.push(root.clone());
            }
        }
        if idxs.is_empty() && unmapped.is_empty() {
            continue;
        }
        let mut affected: Vec<AffectedGroup> = Vec::new();
        for gi in idxs {
            let o = &outcomes[gi];
            let mut kinds: Vec<OutputRootKind> = Vec::new();
            for r in [&ws.output.target, &ws.output.build] {
                if let Some(p) = r.physical_path.as_ref()
                    && o.group.physicals.contains(p)
                {
                    kinds.push(r.kind);
                }
            }
            affected.push(AffectedGroup {
                display: o.group.display.clone(),
                covering: o.group.covering.clone(),
                ownership: o.group.ownership,
                kinds,
                measured: o.measured.clone(),
                skip: o.skip,
            });
        }
        // C003 §7.1: a workspace with no reportable affected output is not a
        // cleanup unit at all.
        if !affected.iter().any(|g| g.measured.is_some()) {
            continue;
        }
        let covering = outermost(
            &affected
                .iter()
                .flat_map(|g| g.covering.iter().cloned())
                .collect::<Vec<_>>(),
        );
        let bytes = affected
            .iter()
            .filter_map(|g| g.measured.as_ref().map(|m| m.bytes))
            .fold(0u64, |acc, b| acc.saturating_add(b));
        units.push(CleanupUnit {
            workspace_idx: wi,
            id: ws.id.clone(),
            root: ws.root.clone(),
            root_manifest: ws.root_manifest.clone(),
            output: ws.output.clone(),
            capability: ws.capability,
            groups: affected,
            unmapped,
            covering,
            bytes,
        });
    }
    units.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.root.cmp(&b.root)));
    units
}

fn workspace_source_activity(
    ws: &ResolvedWorkspace,
    all_outputs: &[PathBuf],
    start: SystemTime,
    cutoff: SystemTime,
) -> Result<bool, ()> {
    // Returns Ok(true) if recent, Ok(false) if quiet, Err on uncertainty.
    let member_roots = workspace_member_roots(ws);
    // Only the output roots that can match an entry under this workspace's
    // member roots are worth testing against; the rest of the universe
    // matches nothing this walk can see.
    let relevant = outputs_relevant_to(&member_roots, all_outputs);
    for root in &member_roots {
        match traverse::workspace_member_activity(root, &relevant, start, cutoff) {
            Ok(traverse::SourceActivity::Recent(_)) => return Ok(true),
            Ok(traverse::SourceActivity::Quiet(_)) => {}
            Err(()) => return Err(()),
        }
    }
    Ok(false)
}

/// The output roots that can exclude an entry under `member_roots`.
///
/// Every path the walk visits is at or under one of the member roots, so an
/// output root can only ever match one when the two are prefix-related: the
/// output root is at or under the member root, or the member root is at or
/// under it. Every other output root in the universe matches nothing, and
/// testing every visited entry against all of them is what made the walk
/// quadratic in the number of workspaces.
///
/// The ancestor direction is kept on purpose: an output root above the member
/// root prunes the whole walk, and dropping it would turn a quiet workspace
/// into an active one.
fn outputs_relevant_to(member_roots: &[PathBuf], all_outputs: &[PathBuf]) -> Vec<PathBuf> {
    all_outputs
        .iter()
        .filter(|output| {
            member_roots
                .iter()
                .any(|root| output.starts_with(root) || root.starts_with(output.as_path()))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{NoopObserver, TestObserver};

    /// The checked-in selector qualification policy (M011D).
    ///
    /// Read at test time from the checkout rather than embedded with
    /// `include_str!`, so the published crate stays the deliberate subset
    /// `Cargo.toml`'s `include` allowlist describes: this file is a maintenance
    /// authority, not product surface.
    fn selector_policy() -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("release")
            .join("selector-qualification.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "the selector policy {} is unreadable: {error}",
                path.display()
            )
        });
        serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!(
                "the selector policy {} is not valid JSON: {error}",
                path.display()
            )
        })
    }

    fn policy_versions(key: &str) -> Vec<String> {
        selector_policy()
            .get(key)
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("the selector policy has no `{key}` list"))
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .unwrap_or_else(|| panic!("`{key}` holds a non-string entry"))
                    .to_owned()
            })
            .collect()
    }

    /// The runtime allowlist is the product's actual capability claim, so it is
    /// proved against the policy rather than allowed to define the truth.
    ///
    /// Everything here is checkable without a network or a second toolchain:
    /// each asserted version must be enabled, and *every other* version must not
    /// be. That second half is what stops an exploratory Cargo release from
    /// becoming a support claim by appearing in only one of the two places.
    #[test]
    fn runtime_allowlist_agrees_with_the_selector_qualification_policy() {
        let profile = policy_versions("profile_selector");
        let package = policy_versions("package_selector");
        assert!(
            !profile.is_empty(),
            "the policy claims no profile-qualified release"
        );
        assert!(
            !package.is_empty(),
            "the policy claims no package-qualified release"
        );

        for version in &profile {
            assert!(
                clean_capabilities_from_version(version).profile_selector,
                "the policy claims profile selection for Cargo {version} but the runtime table does not enable it"
            );
        }
        for version in &package {
            assert!(
                clean_capabilities_from_version(version).package_selector,
                "the policy claims package selection for Cargo {version} but the runtime table does not enable it"
            );
        }

        // Exact-version parsing is the fail-closed property: a version the
        // policy never observed must be false, and it must stay false for its
        // prefixes (`1.99` is not `1.99.0`) and its suffixes (`1.99.0-nightly`
        // is not `1.99.0`).
        for major in 0..3u32 {
            for minor in 0..110u32 {
                for patch in 0..3u32 {
                    let candidate = format!("{major}.{minor}.{patch}");
                    let expected_profile = profile.contains(&candidate);
                    let expected_package = package.contains(&candidate);
                    assert_eq!(
                        clean_capabilities_from_version(&candidate).profile_selector,
                        expected_profile,
                        "profile selection for Cargo {candidate} disagrees with the policy"
                    );
                    assert_eq!(
                        clean_capabilities_from_version(&candidate).package_selector,
                        expected_package,
                        "package selection for Cargo {candidate} disagrees with the policy"
                    );
                }
            }
        }

        for version in [
            "1.99",
            "1.9",
            "1.99.0-nightly",
            "1.89",
            "stable",
            "unknown",
            "",
        ] {
            assert!(
                !clean_capabilities_from_version(version).profile_selector,
                "Cargo {version:?} is not an exact qualified release but profile selection was enabled for it"
            );
            assert!(
                !clean_capabilities_from_version(version).package_selector,
                "Cargo {version:?} is not an exact qualified release but package selection was enabled for it"
            );
        }
    }

    /// An exploratory toolchain is observed, never claimed. A Cargo release that
    /// someone wanted to try out must not be able to reach the runtime table by
    /// appearing in the policy's exploratory list.
    #[test]
    fn exploratory_toolchains_are_never_promoted_by_accident() {
        for version in policy_versions("exploratory") {
            assert!(
                !clean_capabilities_from_version(&version).profile_selector,
                "exploratory toolchain {version} enables profile selection at runtime; exploratory evidence is a research signal, not a support claim"
            );
            assert!(
                !clean_capabilities_from_version(&version).package_selector,
                "exploratory toolchain {version} enables package selection at runtime; exploratory evidence is a research signal, not a support claim"
            );
        }
    }

    /// Package selection is claimed on a strict subset of profile selection,
    /// because M008D only authorized it on the releases where a configured
    /// `build.target` and an explicit `--target` select the same bytes.
    #[test]
    fn package_qualified_releases_are_a_subset_of_profile_qualified_ones() {
        let profile = policy_versions("profile_selector");
        for version in policy_versions("package_selector") {
            assert!(
                profile.contains(&version),
                "the policy claims package selection for Cargo {version} without claiming profile selection for it"
            );
        }
    }

    /// A runner that fails the chosen Cargo invocation with the configured
    /// stderr.
    ///
    /// `locate-project` has to succeed for the metadata path to be reachable at
    /// all, so the failing stage is a flag rather than "fail everything": both
    /// production call sites are driven from one fixture and cannot drift apart.
    struct FailingCargo {
        root_manifest: PathBuf,
        stderr: Vec<u8>,
        locate_succeeds: bool,
    }

    impl CargoRunner for FailingCargo {
        fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
            let locating = args.first().is_some_and(|a| a == "locate-project");
            if locating && self.locate_succeeds {
                let json = serde_json::json!({"root": self.root_manifest});
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            Ok(ProcessOutput {
                success: false,
                code: Some(1),
                stdout: Vec::new(),
                stderr: self.stderr.clone(),
            })
        }
    }

    /// stderr/reason pairs shared by both call-site tests: Cargo that said
    /// nothing, Cargo that complained once, and Cargo that complained far more
    /// than a diagnostic field can carry.
    fn cargo_stderr_cases() -> Vec<(Vec<u8>, String)> {
        vec![
            (Vec::new(), "cargo wrote nothing to stderr".to_owned()),
            (
                b"\n  error: no matching package named 'x'\n  Caused by: registry is offline\n"
                    .to_vec(),
                "error: no matching package named 'x' | Caused by: registry is offline".to_owned(),
            ),
            (
                vec![b'e'; 10_000],
                format!("{}… (truncated)", "e".repeat(480)),
            ),
        ]
    }

    /// A silent failure has to stay explicable. Before the reason existed the
    /// report named a missing project and stopped; the fallback keeps the
    /// diagnostic a sentence rather than a label followed by nothing.
    #[test]
    fn cargo_reason_names_the_absence_when_cargo_wrote_nothing() {
        // Whitespace-only stderr carries no cause either. Reporting it as an
        // empty reason would read as a truncated message instead of silence.
        for stderr in [
            b"".as_slice(),
            b"\n".as_slice(),
            b"   \n\t\n  ".as_slice(),
            b"\r\n\r\n".as_slice(),
        ] {
            assert_eq!(
                cargo_failure_reason(stderr),
                "cargo wrote nothing to stderr",
                "{stderr:?} must report the absence of Cargo's output"
            );
        }
    }

    #[test]
    fn cargo_reason_is_bounded_and_marked_when_stderr_is_long() {
        // Cargo's stderr is unbounded and the reason is serialized verbatim into
        // JSON, the log line and the human report. An unmarked cut would read as
        // Cargo's complete sentence, so the truncation has to be visible.
        let reason = cargo_failure_reason(&vec![b'x'; 10_000]);
        assert_eq!(
            reason,
            format!("{}… (truncated)", "x".repeat(480)),
            "the reason is 480 bytes of Cargo's own text plus the marker"
        );
        assert!(
            reason.len() < 10_000,
            "10_000 bytes of stderr must not be embedded whole"
        );

        // A line that is exactly the bound is complete, not truncated:
        // marking it would claim Cargo said less than it did.
        let at_bound = "y".repeat(480);
        assert_eq!(
            cargo_failure_reason(at_bound.as_bytes()),
            at_bound,
            "a line exactly at the bound needs no marker"
        );
    }

    #[test]
    fn cargo_reason_never_splits_a_multi_byte_character() {
        // Truncation is by byte count and a path is whatever the user typed, so
        // the bound routinely lands mid-character. Cutting there would either
        // panic or emit a replacement character inside the user's own path.
        let stderr = "誤".repeat(200);
        assert_eq!(stderr.len(), 600, "three bytes per character");
        let reason = cargo_failure_reason(stderr.as_bytes());
        let kept = reason
            .strip_suffix("… (truncated)")
            .unwrap_or_else(|| panic!("a truncated reason must be marked: {reason}"));
        assert_eq!(
            kept,
            "誤".repeat(160),
            "160 characters is the longest whole-character prefix within 480 bytes"
        );
        assert!(
            kept.chars().all(|c| c == '誤'),
            "the kept text is whole characters, not a partial one"
        );

        // A multi-byte reason below the bound is preserved in full.
        let two_byte = "é".repeat(200);
        assert_eq!(two_byte.len(), 400);
        assert_eq!(
            cargo_failure_reason(two_byte.as_bytes()),
            two_byte,
            "valid UTF-8 is retained when the bounded excerpt fits"
        );
    }

    #[test]
    fn cargo_reason_keeps_a_bounded_cause_chain_on_one_safe_line() {
        // Cargo indents and prefixes its real complaint, and follows it with a
        // cause chain. Preserve the useful cause while keeping the field one
        // line and bounded.
        assert_eq!(
            cargo_failure_reason(
                b"\n\n   \n  error: failed to parse manifest\n  Caused by: a much longer second line\n"
            ),
            "error: failed to parse manifest | Caused by: a much longer second line",
            "the cause chain is retained and leading whitespace is normalized"
        );

        // Only the first line is bounded, so a huge *later* line must not leak
        // into the reason at all.
        let noisy = format!("\nreal cause\nsecond cause\n{}", "noise ".repeat(5_000));
        let noisy_reason = cargo_failure_reason(noisy.as_bytes());
        assert!(noisy_reason.starts_with("real cause | second cause | noise"));
        assert!(noisy_reason.len() < 500);
        assert!(
            !cargo_failure_reason(noisy.as_bytes()).contains('\n'),
            "the reason occupies one line"
        );

        let controls = b"error: bad\x1b[31m\nCaused by: \x07path\n";
        let sanitized = cargo_failure_reason(controls);
        assert!(!sanitized.chars().any(char::is_control));
        assert_eq!(sanitized, "error: bad[31m | Caused by: path");
    }

    #[test]
    fn locate_failure_diagnostic_carries_cargo_stderr() {
        for (stderr, expected) in cargo_stderr_cases() {
            let d = tempfile::tempdir().unwrap();
            let root = d.path().join("ws/Cargo.toml");
            std::fs::create_dir_all(root.parent().unwrap()).unwrap();
            std::fs::write(&root, "").unwrap();
            let runner = FailingCargo {
                root_manifest: root.clone(),
                stderr: stderr.clone(),
                locate_succeeds: false,
            };
            let mut counters = ScanCounters::default();
            let mut diagnostics = vec![];
            let coverage = resolve_workspaces_with_coverage(
                std::slice::from_ref(&root),
                &runner,
                &mut counters,
                &mut diagnostics,
                &NoopObserver,
            );
            let wanted = format!("cargo locate-project failed: {expected}");
            assert!(
                diagnostics.iter().any(|d| d.message == wanted),
                "expected {wanted:?}, got {:?}",
                diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
            assert_eq!(coverage.unresolved.len(), 1);
            assert!(
                coverage.unresolved[0].reason == wanted,
                "the participant carries the same reason as the diagnostic: {:?}",
                coverage.unresolved[0].reason
            );
        }
    }

    #[test]
    fn metadata_failure_diagnostic_carries_cargo_stderr() {
        for (stderr, expected) in cargo_stderr_cases() {
            let d = tempfile::tempdir().unwrap();
            let root = d.path().join("ws/Cargo.toml");
            std::fs::create_dir_all(root.parent().unwrap()).unwrap();
            std::fs::write(&root, "").unwrap();
            let runner = FailingCargo {
                root_manifest: root.clone(),
                stderr: stderr.clone(),
                locate_succeeds: true,
            };
            let mut counters = ScanCounters::default();
            let mut diagnostics = vec![];
            let coverage = resolve_workspaces_with_coverage(
                std::slice::from_ref(&root),
                &runner,
                &mut counters,
                &mut diagnostics,
                &NoopObserver,
            );
            let wanted = format!("cargo metadata failed: {expected}");
            assert!(
                diagnostics.iter().any(|d| d.message == wanted),
                "expected {wanted:?}, got {:?}",
                diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
            assert_eq!(coverage.unresolved.len(), 1);
            assert_eq!(coverage.unresolved[0].stage, "metadata");
            assert!(
                coverage.unresolved[0].reason == wanted,
                "the participant carries the same reason as the diagnostic: {:?}",
                coverage.unresolved[0].reason
            );
        }
    }

    #[test]
    fn unresolved_stage_is_recorded_not_parsed_from_message() {
        // Discriminating case: the member-source diagnostic contains neither
        // "locate-project" nor "metadata", so message parsing would label it
        // "identity". The stage is recorded at the failure site instead.
        struct RelativeMemberRunner {
            root_manifest: PathBuf,
            target: PathBuf,
        }
        impl CargoRunner for RelativeMemberRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let json = serde_json::json!({"root": self.root_manifest});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                let json = serde_json::json!({
                    "packages": [{
                        "id": "m0",
                        "name": "m0",
                        "manifest_path": "relative/member/Cargo.toml",
                    }],
                    "workspace_members": ["m0"],
                    "workspace_root": self.root_manifest.parent().unwrap(),
                    "target_directory": self.target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let root_manifest = ws_root.join("Cargo.toml");
        std::fs::write(&root_manifest, "").unwrap();
        let target = ws_root.join("target");
        let runner = RelativeMemberRunner {
            root_manifest: root_manifest.clone(),
            target,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&root_manifest),
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert_eq!(coverage.unresolved.len(), 1);
        let reason = &coverage.unresolved[0].reason;
        assert!(
            !reason.contains("locate-project") && !reason.contains("metadata"),
            "test premise holds: {reason:?}"
        );
        assert_eq!(coverage.unresolved[0].stage, "metadata");
    }

    struct FakeCargo {
        locate_root: PathBuf,
        target: PathBuf,
        build: Option<PathBuf>,
        members: usize,
        fail_locate: bool,
        fail_metadata: bool,
        malformed: bool,
    }

    impl CargoRunner for FakeCargo {
        fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
            if args.first().is_some_and(|a| a == "locate-project") {
                if self.fail_locate {
                    return Ok(ProcessOutput {
                        success: false,
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"locate failed".to_vec(),
                    });
                }
                let json = serde_json::json!({"root": self.locate_root});
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                });
            }
            if self.fail_metadata {
                return Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"metadata failed".to_vec(),
                });
            }
            if self.malformed {
                return Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: b"not json".to_vec(),
                    stderr: Vec::new(),
                });
            }
            let members: Vec<String> = (0..self.members).map(|i| format!("m{i}")).collect();
            let mut packages = Vec::new();
            for i in 0..self.members {
                let mp = self
                    .locate_root
                    .parent()
                    .unwrap()
                    .join(format!("member{i}/Cargo.toml"));
                packages.push(serde_json::json!({
                    "id": format!("m{i}"),
                    "name": format!("m{i}"),
                    "manifest_path": mp,
                }));
            }
            let mut json = serde_json::json!({
                "packages": packages,
                "workspace_members": members,
                "workspace_root": self.locate_root.parent().unwrap(),
                "target_directory": self.target,
            });
            if let Some(b) = &self.build {
                json["build_directory"] = serde_json::json!(b);
            }
            Ok(ProcessOutput {
                success: true,
                code: Some(0),
                stdout: serde_json::to_vec(&json).unwrap(),
                stderr: Vec::new(),
            })
        }
    }

    fn temp_manifest(dir: &Path) -> PathBuf {
        let m = dir.join("Cargo.toml");
        std::fs::write(&m, "[package]\nname='x'\nversion='0.1.0'\n").unwrap();
        m
    }

    /// A filesystem root declared as the target directory is refused, not
    /// sized.
    ///
    /// `cargo metadata` really can declare `/`:
    /// `CARGO_TARGET_DIR=/ cargo metadata` reports
    /// `"target_directory":"/"`. The guard that refuses it is the only thing
    /// keeping `/` out of the group graph — without it the refusal degrades
    /// to a *false* message ("contains a workspace source tree", which `/`
    /// does not) while `/` becomes a real group member, and sizing one walks
    /// the entire machine.
    ///
    /// *Fails if:* the guard is deleted or its message changes.
    #[test]
    fn a_filesystem_root_target_directory_is_refused_and_never_sized() {
        struct RootTargetRunner {
            root_manifest: PathBuf,
        }
        impl CargoRunner for RootTargetRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let json = serde_json::json!({"root": self.root_manifest});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                let json = serde_json::json!({
                    "packages": [{"id": "m0", "name": "m0", "manifest_path": self.root_manifest}],
                    "workspace_members": ["m0"],
                    "workspace_root": self.root_manifest.parent().unwrap(),
                    "target_directory": "/",
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let root_manifest = d.path().join("ws/Cargo.toml");
        std::fs::create_dir_all(root_manifest.parent().unwrap()).unwrap();
        std::fs::write(&root_manifest, "").unwrap();
        let runner = RootTargetRunner {
            root_manifest: root_manifest.clone(),
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&root_manifest),
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("filesystem root")),
            "the refusal must name the filesystem root: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.message.contains("contains a workspace source tree")),
            "the refusal must not be the source-tree one, which `/` would make false: {diagnostics:?}"
        );
        // The root is refused, so it never becomes a *measurable* output root:
        // no physical path, so nothing can size it.
        assert!(
            coverage
                .workspaces
                .iter()
                .all(|w| { w.output.target.physical_path.is_none() && !w.output.target.exists }),
            "a filesystem root must not be a measurable output root: {:?}",
            coverage.workspaces
        );
    }

    /// A relative path is not a filesystem root.
    ///
    /// The predicate used to treat a relative path's empty parent as a root,
    /// so a relative `target_directory` would have been refused with a
    /// message about a filesystem root — a statement about the path that is
    /// false.
    #[test]
    fn a_relative_path_is_not_a_filesystem_root() {
        assert!(is_filesystem_root(Path::new("/")));
        assert!(!is_filesystem_root(Path::new("/var/tmp/target")));
        assert!(!is_filesystem_root(Path::new("target")));
        assert!(!is_filesystem_root(Path::new("./target")));
    }

    #[test]
    fn capability_without_build_field_is_unavailable() {
        let c = capability_from_metadata(false, "/t", None, false);
        assert_eq!(c.build_dir, CargoBuildDirCapability::Unavailable);
    }

    #[test]
    fn capability_equal_and_distinct() {
        let c = capability_from_metadata(true, "/t", Some("/t"), false);
        assert_eq!(c.build_dir, CargoBuildDirCapability::Equal);
        let c = capability_from_metadata(true, "/t", Some("/b"), false);
        assert_eq!(c.build_dir, CargoBuildDirCapability::Distinct);
    }

    #[test]
    fn capability_env_only_is_unknown() {
        let c = capability_from_metadata(false, "/t", None, true);
        assert_eq!(c.build_dir, CargoBuildDirCapability::Unknown);
    }

    #[test]
    fn capability_malformed_is_unknown() {
        let c = capability_from_metadata(true, "/t", None, false);
        assert_eq!(c.build_dir, CargoBuildDirCapability::Unknown);
    }

    #[test]
    fn repeated_members_invoke_metadata_once() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let root_manifest = ws_root.join("Cargo.toml");
        std::fs::write(&root_manifest, "").unwrap();
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        struct CountingRunner {
            inner: FakeCargo,
            metadata_calls: std::sync::atomic::AtomicU64,
        }
        impl CargoRunner for CountingRunner {
            fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "metadata") {
                    self.metadata_calls
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.inner.run(cwd, args)
            }
        }
        let runner = CountingRunner {
            inner: FakeCargo {
                locate_root: root_manifest.clone(),
                target: target.clone(),
                build: None,
                members: 2,
                fail_locate: false,
                fail_metadata: false,
                malformed: false,
            },
            metadata_calls: std::sync::atomic::AtomicU64::new(0),
        };
        let m1 = ws_root.join("member0/Cargo.toml");
        let m2 = ws_root.join("member1/Cargo.toml");
        std::fs::create_dir_all(m1.parent().unwrap()).unwrap();
        std::fs::create_dir_all(m2.parent().unwrap()).unwrap();
        std::fs::write(&m1, "").unwrap();
        std::fs::write(&m2, "").unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let ws = resolve_workspaces(&[m1, m2], &runner, &mut counters, &mut diags, &noop);
        assert_eq!(ws.len(), 1);
        assert_eq!(
            runner
                .metadata_calls
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        // Bulk resolution issues one locate per manifest up front (the
        // sequential fast path that skipped locates serialized the whole
        // phase), but metadata stays one call per workspace root and the
        // member cache still records the dedup hit without new metadata.
        assert_eq!(counters.cargo_metadata_calls, 1);
        assert_eq!(
            counters.cargo_locate_calls, 2,
            "every manifest locates once; members share one metadata"
        );
        assert_eq!(counters.deduped_workspace_hits, 1);
    }

    #[test]
    fn a_repeated_manifest_stays_covered_by_the_workspace_that_claimed_it() {
        // The fast path reuses an earlier workspace for a manifest it has
        // already resolved. It does **not** re-record the coverage: the
        // manifest is covered because the workspace's own member list and
        // root-manifest registration named it, which is the invariant that
        // lets the fast path stay a pure lookup.
        //
        // *Fails if:* a manifest enters the member map without entering the
        // coverage set — the repeated manifest would then be reported as
        // unresolved, and an unresolved manifest blocks the cleanup scope.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let root_manifest = ws_root.join("Cargo.toml");
        std::fs::write(&root_manifest, "").unwrap();
        let member_manifest = ws_root.join("member0/Cargo.toml");
        std::fs::create_dir_all(member_manifest.parent().unwrap()).unwrap();
        std::fs::write(&member_manifest, "").unwrap();
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let runner = FakeCargo {
            locate_root: root_manifest.clone(),
            target,
            build: None,
            members: 1,
            fail_locate: false,
            fail_metadata: false,
            malformed: false,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let noop = NoopObserver;
        let coverage = resolve_workspaces_with_coverage(
            &[root_manifest.clone(), root_manifest.clone()],
            &runner,
            &mut counters,
            &mut diagnostics,
            &noop,
        );
        assert_eq!(
            coverage.workspaces.len(),
            1,
            "one workspace for one root, however many times it is listed"
        );
        assert_eq!(
            coverage.unresolved.len(),
            0,
            "a repeated manifest is covered by the workspace that claimed it: {:?}",
            coverage.unresolved
        );
        assert_eq!(counters.deduped_workspace_hits, 1);
    }

    #[test]
    fn direct_known_root_refresh_uses_one_metadata_call_and_no_locate() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("ws");
        std::fs::create_dir_all(root.join("member0")).unwrap();
        let root_manifest = root.join("Cargo.toml");
        std::fs::write(&root_manifest, "").unwrap();
        let member_manifest = root.join("member0/Cargo.toml");
        std::fs::write(&member_manifest, "").unwrap();
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let runner = FakeCargo {
            locate_root: root_manifest.clone(),
            target,
            build: None,
            members: 1,
            fail_locate: false,
            fail_metadata: false,
            malformed: false,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let refreshed = refresh_workspace_from_root_manifest(
            &root_manifest,
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert!(refreshed.is_some(), "{diagnostics:?}");
        assert_eq!(counters.cargo_locate_calls, 0);
        assert_eq!(counters.cargo_metadata_calls, 1);
        let refreshed = refreshed.unwrap();
        assert!(refreshed.members.iter().any(|member| {
            member.manifest_path == member_manifest
                && member.source_root
                    == std::fs::canonicalize(member_manifest.parent().unwrap()).unwrap()
        }));
        let mut initial_counters = ScanCounters::default();
        let mut initial_diagnostics = Vec::new();
        let initial = resolve_workspaces(
            std::slice::from_ref(&root_manifest),
            &runner,
            &mut initial_counters,
            &mut initial_diagnostics,
            &NoopObserver,
        );
        assert_eq!(
            initial,
            vec![refreshed],
            "initial and proof parsing share the same metadata decoder"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolution_normalizes_member_roots_through_symlinked_ancestor() {
        use std::os::unix::fs::symlink;

        let d = tempfile::tempdir().unwrap();
        let physical = d.path().join("physical");
        let alias = d.path().join("alias");
        let ws = physical.join("ws");
        std::fs::create_dir_all(ws.join("member0/src")).unwrap();
        std::fs::create_dir_all(ws.join("target/debug")).unwrap();
        let physical_manifest = ws.join("Cargo.toml");
        let reported_member_manifest = ws.join("member0/Cargo.toml");
        std::fs::write(&physical_manifest, "").unwrap();
        std::fs::write(&reported_member_manifest, "").unwrap();
        std::fs::write(ws.join("member0/src/lib.rs"), "old source").unwrap();
        std::fs::write(ws.join("target/debug/artifact.bin"), "old output").unwrap();
        symlink(&physical, &alias).unwrap();
        let alias_manifest = alias.join("ws/Cargo.toml");
        let runner = FakeCargo {
            locate_root: alias_manifest.clone(),
            target: alias.join("ws/target"),
            build: None,
            members: 1,
            fail_locate: false,
            fail_metadata: false,
            malformed: false,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let resolved = resolve_workspaces(
            &[alias_manifest],
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert_eq!(resolved.len(), 1, "{diagnostics:?}");
        let member = resolved[0]
            .members
            .iter()
            .find(|member| member.manifest_path.ends_with("member0/Cargo.toml"))
            .unwrap();
        assert_eq!(member.manifest_path, alias.join("ws/member0/Cargo.toml"));
        assert_eq!(
            member.source_root,
            std::fs::canonicalize(ws.join("member0")).unwrap()
        );
        assert_eq!(
            resolved[0].output.target.physical_path.as_deref(),
            Some(std::fs::canonicalize(ws.join("target")).unwrap().as_path())
        );
        let physical_runner = FakeCargo {
            locate_root: physical_manifest.clone(),
            target: ws.join("target"),
            build: None,
            members: 1,
            fail_locate: false,
            fail_metadata: false,
            malformed: false,
        };
        let mut physical_counters = ScanCounters::default();
        let mut physical_diagnostics = Vec::new();
        let physical_resolved = resolve_workspaces(
            &[physical_manifest],
            &physical_runner,
            &mut physical_counters,
            &mut physical_diagnostics,
            &NoopObserver,
        );
        assert_eq!(
            workspace_member_roots(&resolved[0]),
            workspace_member_roots(&physical_resolved[0])
        );
        assert_eq!(
            resolved[0].output.target.physical_path,
            physical_resolved[0].output.target.physical_path
        );
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(d.path(), old);
        let output = resolved[0].output.target.physical_path.as_ref().unwrap();
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut alias_counters = ScanCounters::default();
        let mut alias_diagnostics = Vec::new();
        let alias_eligible = analyze_groups(
            &resolved,
            build_groups(&resolved),
            start,
            cutoff,
            Duration::from_secs(300),
            &mut alias_counters,
            &mut alias_diagnostics,
            &NoopObserver,
        );
        let mut physical_activity_counters = ScanCounters::default();
        let mut physical_activity_diagnostics = Vec::new();
        let physical_eligible = analyze_groups(
            &physical_resolved,
            build_groups(&physical_resolved),
            start,
            cutoff,
            Duration::from_secs(300),
            &mut physical_activity_counters,
            &mut physical_activity_diagnostics,
            &NoopObserver,
        );
        assert_eq!(alias_eligible, physical_eligible);
        assert_eq!(
            alias_eligible.len(),
            1,
            "old source and output remain eligible"
        );
        std::fs::write(ws.join("target/debug/artifact.bin"), "recent output").unwrap();
        assert!(
            matches!(
                traverse::workspace_member_activity(
                    &member.source_root,
                    std::slice::from_ref(output),
                    start,
                    cutoff
                ),
                Ok(traverse::SourceActivity::Quiet(_))
            ),
            "recent output is excluded from source activity when member roots are canonical"
        );
        assert!(
            traverse::measure_single_target(output)
                .newest
                .is_some_and(|mtime| mtime >= cutoff)
        );
        std::fs::write(ws.join("member0/src/lib.rs"), "recent source").unwrap();
        assert!(
            matches!(
                traverse::workspace_member_activity(
                    &member.source_root,
                    std::slice::from_ref(output),
                    start,
                    cutoff
                ),
                Ok(traverse::SourceActivity::Recent(_))
            ),
            "recent source outside output still protects the workspace"
        );
    }

    #[test]
    fn failed_member_first_is_cleared_by_later_authoritative_workspace_metadata() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("ws/Cargo.toml");
        let member = d.path().join("ws/member0/Cargo.toml");
        std::fs::create_dir_all(member.parent().unwrap()).unwrap();
        std::fs::write(&root, "").unwrap();
        std::fs::write(&member, "").unwrap();
        let target = d.path().join("ws/target");
        std::fs::create_dir_all(&target).unwrap();
        struct MemberFirstFails {
            root: PathBuf,
            member: PathBuf,
            target: PathBuf,
        }
        impl CargoRunner for MemberFirstFails {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let manifest = PathBuf::from(args.last().unwrap());
                    if manifest == self.member {
                        return Ok(ProcessOutput {
                            success: false,
                            code: Some(1),
                            stdout: vec![],
                            stderr: b"transient locate failure".to_vec(),
                        });
                    }
                    let json = serde_json::json!({"root": self.root});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: vec![],
                    });
                }
                let json = serde_json::json!({
                    "packages": [{"id":"root", "name":"root", "manifest_path":self.root}, {"id":"member", "name":"member", "manifest_path":self.member}],
                    "workspace_members": ["root", "member"],
                    "workspace_root": self.root.parent().unwrap(),
                    "target_directory": self.target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: vec![],
                })
            }
        }
        let runner = MemberFirstFails {
            root: root.clone(),
            member: member.clone(),
            target,
        };
        let manifests = vec![member, root];
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            &manifests,
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert_eq!(coverage.discovered_manifest_count, 2);
        assert_eq!(coverage.workspaces.len(), 1);
        assert!(coverage.unresolved.is_empty());
        assert_eq!(
            counters.cargo_failures, 1,
            "the diagnostic remains observable although metadata later covers the manifest"
        );
    }

    #[test]
    fn unresolved_discovered_manifest_remains_in_coverage_result() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("broken/Cargo.toml");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(&root, "").unwrap();
        let runner = FakeCargo {
            locate_root: root.clone(),
            target: d.path().join("target"),
            build: None,
            members: 1,
            fail_locate: true,
            fail_metadata: false,
            malformed: false,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&root),
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert!(coverage.workspaces.is_empty());
        assert_eq!(coverage.unresolved.len(), 1);
        assert_eq!(coverage.unresolved[0].manifest, root);
        assert_eq!(coverage.unresolved[0].stage, "locate");
    }

    #[test]
    fn thousand_failed_lookups_keep_every_unresolved_manifest() {
        let d = tempfile::tempdir().unwrap();
        let manifests: Vec<PathBuf> = (0..1_000)
            .map(|index| {
                let manifest = d.path().join(format!("broken-{index}/Cargo.toml"));
                std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
                std::fs::write(&manifest, "").unwrap();
                manifest
            })
            .collect();
        let runner = FakeCargo {
            locate_root: manifests[0].clone(),
            target: d.path().join("target"),
            build: None,
            members: 1,
            fail_locate: true,
            fail_metadata: false,
            malformed: false,
        };
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            &manifests,
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );

        assert_eq!(counters.cargo_locate_calls, 1_000);
        assert_eq!(counters.cargo_failures, 1_000);
        assert_eq!(diagnostics.len(), 1_000);
        assert_eq!(coverage.unresolved.len(), 1_000);
        assert!(coverage.workspaces.is_empty());
    }

    #[test]
    fn metadata_failure_or_malformed_metadata_remains_unresolved() {
        for (fail_metadata, malformed) in [(true, false), (false, true)] {
            let d = tempfile::tempdir().unwrap();
            let root = d.path().join("broken/Cargo.toml");
            std::fs::create_dir_all(root.parent().unwrap()).unwrap();
            std::fs::write(&root, "").unwrap();
            let runner = FakeCargo {
                locate_root: root.clone(),
                target: d.path().join("target"),
                build: None,
                members: 1,
                fail_locate: false,
                fail_metadata,
                malformed,
            };
            let mut counters = ScanCounters::default();
            let mut diagnostics = vec![];
            let coverage = resolve_workspaces_with_coverage(
                std::slice::from_ref(&root),
                &runner,
                &mut counters,
                &mut diagnostics,
                &NoopObserver,
            );
            assert!(coverage.workspaces.is_empty());
            assert_eq!(coverage.unresolved.len(), 1);
            assert_eq!(coverage.unresolved[0].stage, "metadata");
        }
    }

    /// Two observation sources can report one manifest, and the discovered
    /// list arrives unordered. `dedup_by` compares neighbours, not the whole
    /// vector, so the duplicate here is separated by a different manifest on
    /// purpose: it survives untouched unless the list is sorted first, and a
    /// repeated project in the report would look like two broken ones.
    #[test]
    fn repeated_unresolved_manifests_are_sorted_and_reported_once() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a/Cargo.toml");
        let b = d.path().join("b/Cargo.toml");
        let c = d.path().join("c/Cargo.toml");
        for m in [&a, &b, &c] {
            std::fs::create_dir_all(m.parent().unwrap()).unwrap();
            std::fs::write(m, "").unwrap();
        }
        let runner = FakeCargo {
            locate_root: a.clone(),
            target: d.path().join("target"),
            build: None,
            members: 1,
            fail_locate: true,
            fail_metadata: false,
            malformed: false,
        };
        // Observed out of order, with `a` reported twice.
        let manifests = vec![b.clone(), a.clone(), a.clone(), c.clone()];
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            &manifests,
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert_eq!(
            coverage.discovered_manifest_count, 4,
            "four observations were made; folding the participants must not rewrite that count"
        );
        assert_eq!(
            coverage
                .unresolved
                .iter()
                .map(|p| &p.manifest)
                .collect::<Vec<_>>(),
            vec![&a, &b, &c],
            "one participant per distinct manifest, in canonical order"
        );
        let keys: Vec<PathBuf> = coverage
            .unresolved
            .iter()
            .map(|p| canonical_manifest_key(&p.manifest))
            .collect();
        let mut ascending = keys.clone();
        ascending.sort();
        assert_eq!(
            keys, ascending,
            "participants are emitted in canonical identity order, which is what makes adjacent-only dedup sound"
        );
        for p in &coverage.unresolved {
            assert_eq!(p.stage, "locate");
            assert!(
                p.reason.contains("cargo locate-project failed"),
                "a folded participant still names why: {:?}",
                p.reason
            );
        }
    }

    /// Deduplication is by canonical identity, the same key used for sorting.
    /// Two spellings of one file are one participant: symlink/`..` aliases
    /// must not inflate `unresolved_ownership`. The observation count is kept
    /// separately in `discovered_manifest_count`, so folding participants
    /// never rewrites how many discoveries were made.
    #[test]
    fn two_spellings_of_one_unresolved_manifest_fold_to_one_participant() {
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("Cargo.toml"), "").unwrap();
        let direct = ws.join("Cargo.toml");
        // `Path` equality normalizes away `.`, so a spelling that survives it
        // needs a real `..`: same file, different `PathBuf`.
        let indirect = ws.join("..").join("ws").join("Cargo.toml");
        assert_ne!(
            direct, indirect,
            "the fixture needs two distinct spellings of one file"
        );
        assert_eq!(
            canonical_manifest_key(&direct),
            canonical_manifest_key(&indirect),
            "the fixture needs both spellings to name one file"
        );
        let runner = FakeCargo {
            locate_root: direct.clone(),
            target: d.path().join("target"),
            build: None,
            members: 1,
            fail_locate: true,
            fail_metadata: false,
            malformed: false,
        };
        let manifests = vec![direct.clone(), indirect.clone()];
        let mut counters = ScanCounters::default();
        let mut diagnostics = vec![];
        let coverage = resolve_workspaces_with_coverage(
            &manifests,
            &runner,
            &mut counters,
            &mut diagnostics,
            &NoopObserver,
        );
        assert_eq!(
            coverage.unresolved.len(),
            1,
            "equal canonical keys fold to one participant: {:?}",
            coverage.unresolved
        );
        // Either spelling may survive the fold; only the merge is at stake.
        assert!(
            coverage.unresolved[0].manifest == direct
                || coverage.unresolved[0].manifest == indirect,
            "{:?}",
            coverage.unresolved
        );
        assert_eq!(
            coverage.discovered_manifest_count, 2,
            "folding participants never rewrites the observation count"
        );
        assert_eq!(coverage.unresolved[0].stage, "locate");
    }

    #[test]
    fn n_member_workspace_requires_one_metadata_regardless_of_order() {
        // Discovery order with a member before the root still => one metadata
        // for the shared root (bulk resolution locates every manifest).
        for order in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let ws_root = d.path().join("ws");
            std::fs::create_dir_all(&ws_root).unwrap();
            let root_manifest = ws_root.join("Cargo.toml");
            std::fs::write(&root_manifest, "").unwrap();
            let target = ws_root.join("target");
            std::fs::create_dir_all(&target).unwrap();
            let m0 = ws_root.join("member0/Cargo.toml");
            let m1 = ws_root.join("member1/Cargo.toml");
            let m2 = ws_root.join("member2/Cargo.toml");
            for m in [&m0, &m1, &m2] {
                std::fs::create_dir_all(m.parent().unwrap()).unwrap();
                std::fs::write(m, "").unwrap();
            }
            let runner = FakeCargo {
                locate_root: root_manifest.clone(),
                target: target.clone(),
                build: None,
                members: 3,
                fail_locate: false,
                fail_metadata: false,
                malformed: false,
            };
            let mut counters = ScanCounters::default();
            let mut diags = Vec::new();
            let noop = NoopObserver;
            // Fake metadata lists member0..2; use member-before-root order when
            // `order` is true.
            let manifests = if order {
                vec![m0.clone(), m1.clone(), m2.clone(), root_manifest.clone()]
            } else {
                vec![root_manifest.clone(), m0.clone(), m1.clone(), m2.clone()]
            };
            let ws = resolve_workspaces(&manifests, &runner, &mut counters, &mut diags, &noop);
            assert_eq!(ws.len(), 1, "order={order}");
            assert_eq!(counters.cargo_metadata_calls, 1, "order={order}");
            // Bulk resolution locates every manifest (4) while metadata stays
            // one call for the shared root, in either discovery order.
            assert_eq!(counters.cargo_locate_calls, 4, "order={order}");
        }
    }

    /// A root whose metadata fails keeps sequential accounting without
    /// sequential spawns: every manifest locating to it re-accounts one
    /// metadata call, one failure, and its own diagnostic, while Cargo runs
    /// only once. Bulk issue must not collapse distinct participants into one
    /// ledger row, and must not re-spawn per participant either.
    #[test]
    fn shared_failing_root_reaccounts_metadata_per_manifest_from_one_spawn() {
        let d = tempfile::tempdir().unwrap();
        let fail_root = d.path().join("failing");
        std::fs::create_dir_all(&fail_root).unwrap();
        let m1 = fail_root.join("member-a/Cargo.toml");
        let m2 = fail_root.join("member-b/Cargo.toml");
        for m in [&m1, &m2] {
            std::fs::create_dir_all(m.parent().unwrap()).unwrap();
            std::fs::write(m, "").unwrap();
        }
        struct CountingFailRunner {
            root: PathBuf,
            metadata_runs: std::sync::atomic::AtomicU64,
        }
        impl CargoRunner for CountingFailRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let json = serde_json::json!({"root": self.root.join("Cargo.toml")});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                self.metadata_runs
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"metadata failed".to_vec(),
                })
            }
        }
        let runner = CountingFailRunner {
            root: fail_root,
            metadata_runs: std::sync::atomic::AtomicU64::new(0),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            &[m1.clone(), m2.clone()],
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert!(coverage.workspaces.is_empty());
        // Accounting matches two sequential metadata spawns ...
        assert_eq!(counters.cargo_metadata_calls, 2);
        assert_eq!(counters.cargo_failures, 2);
        // ... while Cargo ran only once.
        assert_eq!(
            runner
                .metadata_runs
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        // Each participant keeps its own diagnostic and ledger row.
        assert_eq!(diags.len(), 2);
        assert_eq!(coverage.unresolved.len(), 2);
        for ((manifest, diagnostic), participant) in [&m1, &m2]
            .into_iter()
            .zip(diags.iter())
            .zip(coverage.unresolved.iter())
        {
            assert_eq!(diagnostic.path.as_ref(), Some(manifest));
            assert!(diagnostic.message.contains("metadata failed"));
            assert_eq!(&participant.manifest, manifest);
            assert_eq!(participant.stage, "metadata");
        }
    }

    /// Locate spawns actually overlap: with several manifests and a sleeping
    /// runner, peak concurrency must exceed one. Gated on real parallelism —
    /// a single-core host resolves sequentially by design.
    #[test]
    fn resolution_overlaps_locate_spawns() {
        struct SleepRunner {
            active: std::sync::atomic::AtomicUsize,
            peak: std::sync::atomic::AtomicUsize,
        }
        impl CargoRunner for SleepRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                let active = self
                    .active
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;
                self.peak
                    .fetch_max(active, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.active
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                if args.first().is_some_and(|a| a == "locate-project") {
                    return Ok(ProcessOutput {
                        success: false,
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"locate failed".to_vec(),
                    });
                }
                Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"metadata failed".to_vec(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let manifests: Vec<PathBuf> = (0..8)
            .map(|i| {
                let m = d.path().join(format!("ws{i}/Cargo.toml"));
                std::fs::create_dir_all(m.parent().unwrap()).unwrap();
                std::fs::write(&m, "").unwrap();
                m
            })
            .collect();
        let runner = SleepRunner {
            active: std::sync::atomic::AtomicUsize::new(0),
            peak: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        resolve_workspaces_with_coverage(
            &manifests,
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert_eq!(counters.cargo_locate_calls, 8);
        assert_eq!(diags.len(), 8);
        if std::thread::available_parallelism().map_or(1, usize::from) > 1 {
            assert!(
                runner.peak.load(std::sync::atomic::Ordering::SeqCst) >= 2,
                "locate spawns must overlap on a multi-core host"
            );
        }
    }

    /// Mixed locate and metadata failures keep manifest input order in the
    /// diagnostic stream: a locate failure for an earlier manifest precedes
    /// the metadata failure its later neighbour triggers.
    #[test]
    fn mixed_locate_and_metadata_failures_keep_manifest_order() {
        let d = tempfile::tempdir().unwrap();
        let bad_locate = d.path().join("bad-locate/Cargo.toml");
        let bad_meta = d.path().join("bad-meta/Cargo.toml");
        let ok_manifest = d.path().join("ok/Cargo.toml");
        for m in [&bad_locate, &bad_meta, &ok_manifest] {
            std::fs::create_dir_all(m.parent().unwrap()).unwrap();
            std::fs::write(m, "").unwrap();
        }
        struct OrderRunner {
            fail_root: PathBuf,
            ok_root: PathBuf,
            ok_target: PathBuf,
        }
        impl CargoRunner for OrderRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                let manifest = args.last().unwrap().to_string_lossy().into_owned();
                if args.first().is_some_and(|a| a == "locate-project") {
                    if manifest.contains("bad-locate") {
                        return Ok(ProcessOutput {
                            success: false,
                            code: Some(1),
                            stdout: Vec::new(),
                            stderr: b"locate failed".to_vec(),
                        });
                    }
                    let root = if manifest.contains("bad-meta") {
                        self.fail_root.join("Cargo.toml")
                    } else {
                        self.ok_root.join("Cargo.toml")
                    };
                    let json = serde_json::json!({"root": root});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                if manifest.contains("fail_root") {
                    return Ok(ProcessOutput {
                        success: false,
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"metadata failed".to_vec(),
                    });
                }
                let json = serde_json::json!({
                    "packages": [{"id": "m", "name": "m", "manifest_path": self.ok_root.join("Cargo.toml")}],
                    "workspace_members": ["m"],
                    "workspace_root": self.ok_root,
                    "target_directory": self.ok_target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let fail_root = d.path().join("fail_root");
        let ok_root = d.path().join("ok");
        std::fs::create_dir_all(ok_root.join("target")).unwrap();
        let runner = OrderRunner {
            fail_root,
            ok_root: ok_root.clone(),
            ok_target: ok_root.join("target"),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            &[bad_locate.clone(), bad_meta.clone(), ok_manifest.clone()],
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert_eq!(coverage.workspaces.len(), 1);
        assert_eq!(
            diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
            vec![
                "cargo locate-project failed: locate failed",
                "cargo metadata failed: metadata failed",
            ]
        );
        assert_eq!(coverage.unresolved.len(), 2);
        assert_eq!(coverage.unresolved[0].stage, "locate");
        assert_eq!(coverage.unresolved[1].stage, "metadata");
    }

    #[test]
    fn two_independent_workspaces_require_two_locate_two_metadata() {
        let d = tempfile::tempdir().unwrap();
        struct TwoWsRunner {
            a_root: PathBuf,
            b_root: PathBuf,
            a_target: PathBuf,
            b_target: PathBuf,
        }
        impl CargoRunner for TwoWsRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let manifest = args.last().unwrap().to_string_lossy().into_owned();
                    let root = if manifest.contains("a_ws") {
                        self.a_root.join("Cargo.toml")
                    } else {
                        self.b_root.join("Cargo.toml")
                    };
                    let json = serde_json::json!({"root": root});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                // metadata: return workspace matching manifest-path arg.
                let manifest = args.last().unwrap().to_string_lossy().into_owned();
                let (ws_root, target) = if manifest.contains("a_ws") {
                    (&self.a_root, &self.a_target)
                } else {
                    (&self.b_root, &self.b_target)
                };
                let json = serde_json::json!({
                    "packages": [{"id": "m", "name": "m", "manifest_path": ws_root.join("Cargo.toml")}],
                    "workspace_members": ["m"],
                    "workspace_root": ws_root,
                    "target_directory": target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let a_ws = d.path().join("a_ws");
        let b_ws = d.path().join("b_ws");
        for ws in [&a_ws, &b_ws] {
            std::fs::create_dir_all(ws).unwrap();
            std::fs::write(ws.join("Cargo.toml"), "").unwrap();
            std::fs::create_dir_all(ws.join("target")).unwrap();
        }
        let runner = TwoWsRunner {
            a_root: a_ws.clone(),
            b_root: b_ws.clone(),
            a_target: a_ws.join("target"),
            b_target: b_ws.join("target"),
        };
        let manifests = vec![a_ws.join("Cargo.toml"), b_ws.join("Cargo.toml")];
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let ws = resolve_workspaces(&manifests, &runner, &mut counters, &mut diags, &noop);
        assert_eq!(ws.len(), 2);
        assert_eq!(counters.cargo_locate_calls, 2);
        assert_eq!(counters.cargo_metadata_calls, 2);
    }

    #[test]
    fn failed_first_locate_does_not_poison_unrelated_workspace() {
        struct FailFirstRunner {
            good_root: PathBuf,
            good_target: PathBuf,
        }
        impl CargoRunner for FailFirstRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let manifest = args.last().unwrap().to_string_lossy().into_owned();
                    if manifest.contains("bad") {
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
                let json = serde_json::json!({
                    "packages": [{"id": "m", "name": "m", "manifest_path": self.good_root.join("Cargo.toml")}],
                    "workspace_members": ["m"],
                    "workspace_root": self.good_root,
                    "target_directory": self.good_target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let good = d.path().join("good");
        std::fs::create_dir_all(&good).unwrap();
        let good_manifest = good.join("Cargo.toml");
        std::fs::write(&good_manifest, "").unwrap();
        let bad_manifest = d.path().join("bad/Cargo.toml");
        std::fs::create_dir_all(bad_manifest.parent().unwrap()).unwrap();
        std::fs::write(&bad_manifest, "").unwrap();
        let runner = FailFirstRunner {
            good_root: good.clone(),
            good_target: good.join("target"),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let ws = resolve_workspaces(
            &[bad_manifest, good_manifest],
            &runner,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].root, std::fs::canonicalize(&good).unwrap_or(good));
        assert!(!diags.is_empty());
    }

    /// The nested-workspace refusal is recognised from Cargo's *structured*
    /// output, not from one English sentence.
    ///
    /// The sentence is accepted, because it is the spelling the host Cargo
    /// uses today. But the guidance that follows a refusal must not depend on
    /// Cargo's prose: a Cargo that reworded the sentence while still naming
    /// both manifests used to silence the hint entirely, leaving an operator
    /// with a refusal and no explanation of what to do about it.
    #[test]
    fn refused_nested_workspace_is_recognised_without_the_english_sentence() {
        // The sentence Cargo uses today.
        assert!(refused_nested_workspace(
            "error: current package believes it's in a workspace when it's not: | current: /a/b/Cargo.toml | workspace: /a/Cargo.toml"
        ));
        // The same refusal, reworded: Cargo still names both manifests.
        assert!(refused_nested_workspace(
            "error: package is not a member of the workspace it claims: | current: /a/b/Cargo.toml | workspace: /a/Cargo.toml"
        ));
        // A different failure class: no hint.
        assert!(!refused_nested_workspace(
            "cargo locate-project failed: offline"
        ));
        // A mention of a workspace, but no manifest pair: no hint.
        assert!(!refused_nested_workspace(
            "error: failed to load manifest for workspace member"
        ));
    }

    #[test]
    fn believed_workspace_manifest_parses_sanitized_and_raw_forms() {
        // Sanitized (newlines joined with " | ") as produced by
        // `cargo_failure_reason`.
        assert_eq!(
            believed_workspace_manifest(
                "error: current package believes it's in a workspace when it's not: | current: /a/b/Cargo.toml | workspace: /a/Cargo.toml"
            ),
            Some(PathBuf::from("/a/Cargo.toml"))
        );
        // Raw multi-line form, in case a caller passes stderr through.
        assert_eq!(
            believed_workspace_manifest(
                "error: current package believes it's in a workspace when it's not:\ncurrent:   /a/b/Cargo.toml\nworkspace: /a/Cargo.toml\n"
            ),
            Some(PathBuf::from("/a/Cargo.toml"))
        );
        // Wrong failure class: no attribution.
        assert_eq!(
            believed_workspace_manifest("cargo locate-project failed: offline"),
            None
        );
        // Truncated before any Cargo.toml token: fail closed.
        assert_eq!(
            believed_workspace_manifest(
                "error: current package believes it's in a workspace when it's… (truncated)"
            ),
            None
        );
    }

    #[test]
    fn misconfigured_member_with_resolved_parent_still_blocks_but_attributes_as_passenger() {
        // The live case: `fuzz/Cargo.toml` claims parent workspace `ws`, which
        // resolves. Under ADR 001 C004/C006 the fuzz manifest stays an
        // unresolved participant and still blocks — M015 is report-only and
        // ships no enforcement pass-through. What M015 adds is report-only
        // attribution: the claimant rides as a passenger of the named island
        // instead of merging it. The diagnostic stays so the misconfiguration
        // is visible and fixable.
        struct BelievesRunner {
            parent_manifest: PathBuf,
            parent_root: PathBuf,
            parent_target: PathBuf,
        }
        impl CargoRunner for BelievesRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let manifest = args.last().unwrap().to_string_lossy().into_owned();
                    if manifest.contains("fuzz") {
                        let stderr = format!(
                            "error: current package believes it's in a workspace when it's not:\ncurrent: {}\nworkspace: {}",
                            manifest,
                            self.parent_manifest.display()
                        );
                        return Ok(ProcessOutput {
                            success: false,
                            code: Some(1),
                            stdout: Vec::new(),
                            stderr: stderr.into_bytes(),
                        });
                    }
                    let json = serde_json::json!({"root": self.parent_manifest});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                let json = serde_json::json!({
                    "packages": [{"id": "m", "name": "m", "manifest_path": self.parent_manifest}],
                    "workspace_members": ["m"],
                    "workspace_root": self.parent_root,
                    "target_directory": self.parent_target,
                });
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let parent = d.path().join("ws");
        let parent_manifest = parent.join("Cargo.toml");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(&parent_manifest, "").unwrap();
        let fuzz_manifest = parent.join("crates/inner/fuzz/Cargo.toml");
        std::fs::create_dir_all(fuzz_manifest.parent().unwrap()).unwrap();
        std::fs::write(&fuzz_manifest, "").unwrap();
        let runner = BelievesRunner {
            parent_manifest: parent_manifest.clone(),
            parent_root: parent.clone(),
            parent_target: parent.join("target"),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            &[parent_manifest.clone(), fuzz_manifest.clone()],
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert_eq!(coverage.workspaces.len(), 1);
        assert!(
            coverage
                .unresolved
                .iter()
                .any(|p| p.manifest == fuzz_manifest),
            "the claimant still blocks its resolved parent (no enforcement pass-through): {:?}",
            coverage.unresolved
        );
        assert!(
            diags.iter().any(|d| d.path.as_ref() == Some(&fuzz_manifest)
                && d.message.contains("believes it's in a workspace")),
            "the misconfiguration stays visible as a diagnostic: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        // Report-only attribution: the blocking claimant is a passenger of
        // the named island, not a merge cause.
        let participant = coverage
            .unresolved
            .iter()
            .find(|p| p.manifest == fuzz_manifest)
            .expect("claimant is unresolved");
        assert_eq!(participant.capability, UnknownCapability::RefusedByCargo);
        assert_eq!(
            attribute_unknown(participant, &coverage.workspaces),
            UnknownDisposition::PassengerOf {
                named_manifest: parent_manifest.clone(),
            },
            "a refused claimant naming a resolved workspace rides that island",
        );
    }

    #[test]
    fn misconfigured_member_with_unresolved_parent_still_blocks() {
        // Same claim, but the named parent does not resolve here: fail closed.
        struct BelievesOrphanRunner {
            parent_manifest: PathBuf,
        }
        impl CargoRunner for BelievesOrphanRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let manifest = args.last().unwrap().to_string_lossy().into_owned();
                    if manifest.contains("fuzz") {
                        let stderr = format!(
                            "error: current package believes it's in a workspace when it's not:\ncurrent: {}\nworkspace: {}",
                            manifest,
                            self.parent_manifest.display()
                        );
                        return Ok(ProcessOutput {
                            success: false,
                            code: Some(1),
                            stdout: Vec::new(),
                            stderr: stderr.into_bytes(),
                        });
                    }
                    let json = serde_json::json!({"root": self.parent_manifest});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                // Parent metadata fails: the named workspace never resolves.
                Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"metadata failed".to_vec(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let parent = d.path().join("ws");
        let parent_manifest = parent.join("Cargo.toml");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(&parent_manifest, "").unwrap();
        let fuzz_manifest = parent.join("fuzz/Cargo.toml");
        std::fs::create_dir_all(fuzz_manifest.parent().unwrap()).unwrap();
        std::fs::write(&fuzz_manifest, "").unwrap();
        let runner = BelievesOrphanRunner { parent_manifest };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&fuzz_manifest),
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert!(
            coverage
                .unresolved
                .iter()
                .any(|p| p.manifest == fuzz_manifest),
            "without a resolved parent the claim must still block: {:?}",
            coverage.unresolved
        );
    }

    /// M015 package B: every merge-default row of the failure-class table.
    ///
    /// The discriminating input per row is the exact reason text that row's
    /// branch records, and the required verdict is `MergeAll` in all of them
    /// — including a believed-workspace reason paired with an `Unproven`
    /// capability, which proves classification follows the recorded branch,
    /// never the prose. Fails if any class is ever attributed by text.
    #[test]
    fn attribute_unknown_merges_every_unproven_class() {
        let empty: &[ResolvedWorkspace] = &[];
        let rows = [
            (
                "locate",
                "cargo locate-project could not start: broken pipe",
            ),
            (
                "locate",
                "cargo locate-project could not start: parallel locate worker failed",
            ),
            (
                "locate",
                "cannot parse cargo locate-project output: unexpected EOF",
            ),
            (
                "locate",
                "error: current package believes it's in a workspace when it's not: | current: /w/fuzz/Cargo.toml | workspace: /w/Cargo.toml",
            ),
            ("metadata", "cargo metadata could not start: broken pipe"),
            (
                "metadata",
                "cargo metadata failed: failed to parse manifest",
            ),
            (
                "metadata",
                "cannot parse cargo metadata output: unexpected EOF",
            ),
            (
                "identity",
                "no successful Cargo workspace metadata covered this manifest",
            ),
        ];
        for (stage, reason) in rows {
            let participant = UnresolvedOwnershipParticipant {
                manifest: PathBuf::from("/w/Cargo.toml"),
                stage,
                reason: reason.into(),
                capability: UnknownCapability::Unproven,
            };
            assert_eq!(
                attribute_unknown(&participant, empty),
                UnknownDisposition::MergeAll,
                "stage={stage} reason={reason:?} must merge",
            );
        }
    }

    /// A refused manifest with no naming evidence is a scope-level note: it
    /// owns no bytes (Cargo refused it), so it merges nothing.
    #[test]
    fn attribute_unknown_reports_refused_manifest_as_scope_note_without_naming_evidence() {
        let empty: &[ResolvedWorkspace] = &[];
        for reason in [
            "cargo locate-project failed: could not find Cargo.toml in this directory",
            "cargo locate-project failed: error: manifest path is a directory",
        ] {
            let participant = UnresolvedOwnershipParticipant {
                manifest: PathBuf::from("/w/Cargo.toml"),
                stage: "locate",
                reason: reason.into(),
                capability: UnknownCapability::RefusedByCargo,
            };
            assert_eq!(
                attribute_unknown(&participant, empty),
                UnknownDisposition::ScopeNote,
                "reason={reason:?} names no workspace and must not merge",
            );
        }
    }

    /// The one attribution rule: a refused manifest naming a workspace that
    /// resolved here rides as that island's passenger.
    #[test]
    fn attribute_unknown_assigns_refused_believed_manifest_to_resolved_island() {
        let ws = ResolvedWorkspace {
            id: WorkspaceId(PathBuf::from("/w")),
            root: PathBuf::from("/w"),
            root_manifest: PathBuf::from("/w/Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: PathBuf::from("/w/target"),
                    physical_path: None,
                    exists: false,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: PathBuf::from("/w/target"),
                    physical_path: None,
                    exists: false,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Unavailable,
                metadata_had_build_directory: false,
                env_build_dir_set: false,
            },
        };
        let workspaces = vec![ws];
        let participant = UnresolvedOwnershipParticipant {
            manifest: PathBuf::from("/w/fuzz/Cargo.toml"),
            stage: "locate",
            reason: "error: current package believes it's in a workspace when it's not: | current: /w/fuzz/Cargo.toml | workspace: /w/Cargo.toml".into(),
            capability: UnknownCapability::RefusedByCargo,
        };
        assert_eq!(
            attribute_unknown(&participant, &workspaces),
            UnknownDisposition::PassengerOf {
                named_manifest: PathBuf::from("/w/Cargo.toml"),
            },
        );
    }

    /// Same claim, named parent not resolved here: refused (owns no bytes)
    /// but unattributable, so a scope note — never a passenger of an
    /// unproven island, and never a merge. Fails if the rule ever attributes
    /// by name alone without checking resolution.
    #[test]
    fn attribute_unknown_with_unresolved_named_parent_is_scope_note_not_passenger() {
        let empty: &[ResolvedWorkspace] = &[];
        let participant = UnresolvedOwnershipParticipant {
            manifest: PathBuf::from("/w/fuzz/Cargo.toml"),
            stage: "locate",
            reason: "error: current package believes it's in a workspace when it's not: | current: /w/fuzz/Cargo.toml | workspace: /w/Cargo.toml".into(),
            capability: UnknownCapability::RefusedByCargo,
        };
        assert_eq!(
            attribute_unknown(&participant, empty),
            UnknownDisposition::ScopeNote,
        );
    }

    /// Truncated believed reason: the parse fails closed to a scope note.
    #[test]
    fn attribute_unknown_truncated_believed_reason_is_scope_note() {
        let empty: &[ResolvedWorkspace] = &[];
        let participant = UnresolvedOwnershipParticipant {
            manifest: PathBuf::from("/w/fuzz/Cargo.toml"),
            stage: "locate",
            reason: "error: current package believes it's in a workspace when it's… (truncated)"
                .into(),
            capability: UnknownCapability::RefusedByCargo,
        };
        assert_eq!(
            attribute_unknown(&participant, empty),
            UnknownDisposition::ScopeNote,
        );
    }

    /// Resolution-level: a Cargo-refused locate records `RefusedByCargo`, and
    /// the attribution reads it back as a scope note — the branch, not the
    /// text, decides. Fails if the emulation ever records the wrong class.
    #[test]
    fn resolution_records_refused_capability_for_cargo_refused_locate() {
        struct RefusedRunner;
        impl CargoRunner for RefusedRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    return Ok(ProcessOutput {
                        success: false,
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr:
                            b"error: current package believes it's in a workspace when it's not"
                                .to_vec(),
                    });
                }
                panic!("metadata must never run for a refused manifest");
            }
        }
        let d = tempfile::tempdir().unwrap();
        let manifest = d.path().join("fuzz/Cargo.toml");
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(&manifest, "").unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&manifest),
            &RefusedRunner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert_eq!(coverage.unresolved.len(), 1);
        let participant = &coverage.unresolved[0];
        assert_eq!(participant.stage, "locate");
        assert_eq!(participant.capability, UnknownCapability::RefusedByCargo);
        assert_eq!(
            attribute_unknown(participant, &coverage.workspaces),
            UnknownDisposition::ScopeNote,
        );
    }

    /// Resolution-level: a metadata failure records `Unproven`, and the
    /// attribution merges it — even though its located root is known. The
    /// located path is not ownership evidence for unproven output. Fails if
    /// metadata failures ever stop merging.
    #[test]
    fn resolution_records_unproven_capability_for_metadata_failure() {
        struct MetadataFailRunner {
            root_manifest: PathBuf,
        }
        impl CargoRunner for MetadataFailRunner {
            fn run(&self, _cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                if args.first().is_some_and(|a| a == "locate-project") {
                    let json = serde_json::json!({"root": self.root_manifest});
                    return Ok(ProcessOutput {
                        success: true,
                        code: Some(0),
                        stdout: serde_json::to_vec(&json).unwrap(),
                        stderr: Vec::new(),
                    });
                }
                Ok(ProcessOutput {
                    success: false,
                    code: Some(101),
                    stdout: Vec::new(),
                    stderr: b"error: failed to parse manifest".to_vec(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        let root_manifest = d.path().join("ws/Cargo.toml");
        std::fs::create_dir_all(root_manifest.parent().unwrap()).unwrap();
        std::fs::write(&root_manifest, "").unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let coverage = resolve_workspaces_with_coverage(
            std::slice::from_ref(&root_manifest),
            &MetadataFailRunner {
                root_manifest: root_manifest.clone(),
            },
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        assert!(coverage.workspaces.is_empty());
        assert_eq!(coverage.unresolved.len(), 1);
        let participant = &coverage.unresolved[0];
        assert_eq!(participant.stage, "metadata");
        assert_eq!(participant.capability, UnknownCapability::Unproven);
        assert_eq!(
            attribute_unknown(participant, &coverage.workspaces),
            UnknownDisposition::MergeAll,
        );
    }

    /// Premise pin for M015/F: a symlinked target dir probes as
    /// `{physical: None, exists: true, is_symlink: true}` — present but
    /// without provable identity, so the partition merges rather than proving
    /// through the link. Unix-gated: symlink creation is not portable, and an
    /// ungated fixture would fail before the behavior under test runs.
    #[cfg(unix)]
    #[test]
    fn symlinked_output_root_reports_no_physical_identity() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real-target");
        std::fs::create_dir_all(&real).unwrap();
        let link = d.path().join("ws-target");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut diags = Vec::new();
        let root = output_root(OutputRootKind::Target, link, &mut diags);
        assert_eq!(root.physical_path, None);
        assert!(root.exists, "a symlinked output is present but unusable");
        assert!(root.is_symlink);
    }

    #[test]
    fn malformed_and_failure_isolation() {
        let d = tempfile::tempdir().unwrap();
        let m = temp_manifest(d.path());
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let bad = FakeCargo {
            locate_root: m.clone(),
            target: d.path().join("target"),
            build: None,
            members: 1,
            fail_locate: false,
            fail_metadata: false,
            malformed: true,
        };
        let ws = resolve_workspaces(
            std::slice::from_ref(&m),
            &bad,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(ws.is_empty());
        assert!(!diags.is_empty());
    }

    #[test]
    fn output_grouping_equal_nested_shared() {
        // Equal target/build in one workspace -> one group, private if inside root.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(ws_root.join("target")).unwrap();
        std::fs::write(ws_root.join("target/file"), b"x").unwrap();
        let canonical_target = std::fs::canonicalize(ws_root.join("target")).unwrap();
        let ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: std::fs::canonicalize(&ws_root).unwrap(),
            root_manifest: ws_root.join("Cargo.toml"),
            members: vec![WorkspaceMember {
                manifest_path: ws_root.join("Cargo.toml"),
                source_root: ws_root.clone(),
            }],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: ws_root.join("target"),
                    physical_path: Some(canonical_target.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: ws_root.join("target"),
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
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].covering.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::PrivateBounded);

        // Two workspaces sharing identical target -> Shared.
        let ws2 = ResolvedWorkspace {
            id: WorkspaceId(d.path().join("other")),
            root: d.path().join("other"),
            root_manifest: d.path().join("other/Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: ws_root.join("target"),
                    physical_path: Some(canonical_target.clone()),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: ws_root.join("target"),
                    physical_path: Some(canonical_target),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: ws.capability,
        };
        let groups = build_groups(&[ws, ws2]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::Shared);
    }

    /// Build a workspace whose declared target directory is `physical`, with an
    /// explicit member list, so ownership classification can be exercised
    /// directly.
    fn workspace_with_output(
        root: &Path,
        members: Vec<PathBuf>,
        target: Option<PathBuf>,
        symlink_target: bool,
    ) -> ResolvedWorkspace {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        // Production canonicalizes member source roots; mirroring that here
        // keeps the classification comparisons meaningful.
        let members: Vec<PathBuf> = members
            .into_iter()
            .map(|m| std::fs::canonicalize(&m).unwrap_or(m))
            .collect();
        let (physical, exists, is_symlink) = match target {
            Some(p) => (
                Some(std::fs::canonicalize(&p).unwrap_or(p)),
                true,
                symlink_target,
            ),
            // An existing root with no physical identity: a symlink, or a
            // regular file where a directory is expected.
            None if symlink_target => (None, true, true),
            None => (None, false, false),
        };
        ResolvedWorkspace {
            id: WorkspaceId(canonical_root.clone()),
            root: canonical_root,
            root_manifest: root.join("Cargo.toml"),
            members: members
                .into_iter()
                .map(|m| WorkspaceMember {
                    manifest_path: m.join("Cargo.toml"),
                    source_root: m,
                })
                .collect(),
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: root.join("target"),
                    physical_path: physical.clone(),
                    exists,
                    is_symlink,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: root.join("target"),
                    physical_path: physical,
                    exists,
                    is_symlink,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Equal,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        }
    }

    #[test]
    fn output_root_equal_to_the_source_tree_is_never_private() {
        // C1: `target-dir = "."` (or `"src"`) makes the declared output root the
        // source tree itself. Cleaning it would delete the project, so it must
        // never be classified as a private artifact.
        for declared in [".", "src"] {
            let d = tempfile::tempdir().unwrap();
            let ws_root = d.path().join("ws");
            std::fs::create_dir_all(ws_root.join("src")).unwrap();
            std::fs::write(ws_root.join("src/main.rs"), b"fn main(){}").unwrap();
            let output = if declared == "." {
                ws_root.clone()
            } else {
                ws_root.join(declared)
            };
            let ws = workspace_with_output(&ws_root, vec![ws_root.clone()], Some(output), false);
            let groups = build_groups(std::slice::from_ref(&ws));
            assert_eq!(groups.len(), 1, "{declared}");
            assert_eq!(
                groups[0].ownership,
                OutputOwnershipClass::Uncertain,
                "an output root that is the source tree must not be cleanable: {declared}"
            );
            assert!(groups[0].source_overlap, "{declared}");
            // And it is never authorized for cleanup.
            assert!(
                !crate::cleanup::covering_is_authorized(
                    groups[0].ownership,
                    &groups[0].covering,
                    &ws_root,
                    &[],
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn output_root_containing_a_member_source_root_is_never_private() {
        // A workspace whose members live in subdirectories: an output root that
        // is an ancestor of a member covers that member's source.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let member = ws_root.join("crates/inner");
        std::fs::create_dir_all(&member).unwrap();
        let output = ws_root.join("crates");
        let ws = workspace_with_output(
            &ws_root,
            vec![ws_root.clone(), member.clone()],
            Some(output),
            false,
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::Uncertain);
        assert!(groups[0].source_overlap);
    }

    #[cfg(unix)]
    #[test]
    fn an_output_root_nested_inside_a_package_directory_is_never_private() {
        // The mirror of the three tests above, and the shape that destroyed a
        // package's checked-in files: `cargo clean` removes whatever is under
        // the root it is given, so an output root that *lives inside* another
        // package's tree takes that package's files with it.
        //
        // The victim does not have to be a workspace the tool could resolve.
        // Cargo refuses a nested package that is not a workspace member
        // ("current package believes it's in a workspace when it's not"), so
        // the package sitting inside the output root is exactly the one the
        // resolved universe does *not* contain — and the member-source check
        // cannot see it. The filesystem can.
        let d = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(d.path()).unwrap();
        let ws_root = base.join("ws");
        let nested = ws_root.join("lib1");
        let output = nested.join("assets");
        std::fs::create_dir_all(&output).unwrap();
        // The victim's own package directory and a checked-in file in it.
        std::fs::write(
            nested.join("Cargo.toml"),
            b"[package]\nname = \"lib1\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(output.join("logo.txt"), b"LOGO\n").unwrap();

        let ws = workspace_with_output(&ws_root, vec![ws_root.clone()], Some(output), false);
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            groups[0].ownership,
            OutputOwnershipClass::Uncertain,
            "an output root inside another package's tree is not an artifact"
        );
        assert!(groups[0].source_overlap);
        assert!(groups[0].nested_source_overlap);
        assert!(
            !crate::cleanup::covering_is_authorized(
                groups[0].ownership,
                &groups[0].covering,
                &ws_root,
                &[],
            )
            .unwrap()
        );
    }

    #[test]
    fn the_ordinary_target_directory_is_not_a_nested_package_overlap() {
        // The guard must not fire on the ordinary shape: `<ws>/target` is
        // inside the owning workspace's own package directory, and that is
        // where every build artifact lives.
        let d = tempfile::tempdir().unwrap();
        let ws_root = std::fs::canonicalize(d.path()).unwrap().join("ws");
        std::fs::create_dir_all(ws_root.join("src")).unwrap();
        std::fs::write(
            ws_root.join("Cargo.toml"),
            b"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(ws_root.join("src/main.rs"), b"fn main() {}\n").unwrap();
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();

        let ws = workspace_with_output(&ws_root, vec![ws_root.clone()], Some(target), false);
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            groups[0].ownership,
            OutputOwnershipClass::PrivateBounded,
            "the conventional target directory is a private artifact"
        );
        assert!(!groups[0].source_overlap);
        assert!(!groups[0].nested_source_overlap);
    }

    #[test]
    fn an_output_root_inside_a_members_source_tree_is_not_private() {
        // `target-dir = "src/foo"` puts the output root inside the crate's own
        // source tree. `cargo clean --target-dir <root>` removes the whole
        // root, so the crate's own `.rs` files would go with it. The
        // name-based guard only sees the root's final component (`foo`), so
        // the containment form of the guard is what has to refuse it.
        let d = tempfile::tempdir().unwrap();
        let ws_root = std::fs::canonicalize(d.path()).unwrap().join("ws");
        std::fs::create_dir_all(ws_root.join("src/foo")).unwrap();
        std::fs::write(
            ws_root.join("Cargo.toml"),
            b"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(ws_root.join("src/main.rs"), b"fn main() {}\n").unwrap();
        std::fs::write(ws_root.join("src/foo/mod.rs"), b"pub fn f() {}\n").unwrap();
        let output = ws_root.join("src/foo");

        let ws = workspace_with_output(&ws_root, vec![ws_root.clone()], Some(output), false);
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            groups[0].ownership,
            OutputOwnershipClass::Uncertain,
            "an output root inside the crate's own source tree is not an artifact"
        );
        assert!(groups[0].source_overlap);
        assert!(
            !crate::cleanup::covering_is_authorized(
                groups[0].ownership,
                &groups[0].covering,
                &ws_root,
                &[],
            )
            .unwrap()
        );
    }

    #[test]
    fn an_output_root_holding_a_package_directory_is_not_private() {
        // A package directory *inside* the output root is the shape the
        // discovery cannot see: it does not descend into an output root, so
        // the nested package is never resolved and never enters the universe.
        // `cargo clean` removes whatever is under the root it is given, so the
        // nested package's checked-in sources would go with it.
        //
        // The classification cannot see it either - it works from the resolved
        // universe - so the measurement is where the shape has to be caught.
        let d = tempfile::tempdir().unwrap();
        let ws_root = std::fs::canonicalize(d.path()).unwrap().join("ws");
        std::fs::create_dir_all(ws_root.join("target/nested/src")).unwrap();
        std::fs::create_dir_all(ws_root.join("src")).unwrap();
        std::fs::write(
            ws_root.join("Cargo.toml"),
            b"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(ws_root.join("src/lib.rs"), b"fn main() {}\n").unwrap();
        std::fs::write(
            ws_root.join("target/nested/Cargo.toml"),
            b"[package]\nname = \"nested\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(ws_root.join("target/nested/src/lib.rs"), b"pub fn f() {}\n").unwrap();
        std::fs::write(ws_root.join("target/artifact.bin"), vec![1u8; 512]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);

        let ws = workspace_with_output(
            &ws_root,
            vec![ws_root.clone()],
            Some(ws_root.join("target")),
            false,
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            groups[0].ownership,
            OutputOwnershipClass::PrivateBounded,
            "the classification works from the resolved universe, which cannot contain the nested package"
        );
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let outcomes = analyze_groups_detailed(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::from_secs(300),
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(outcomes.len(), 1);
        assert!(
            outcomes[0].measured.is_none(),
            "an output root holding a package directory is not a measured candidate"
        );
        assert_eq!(
            outcomes[0].skip,
            Some(GroupSkipReason::UncertainOwnership),
            "the group is withheld, not measured"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("holds the package directory")),
            "the operator has to be told why nothing is cleaned: {diags:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn another_workspaces_source_tree_inside_a_covering_root_is_never_private() {
        // The two tests above both use a *single* workspace, which is the only
        // shape where `ws.members` is the complete set of relevant members. The
        // gap was the neighbour: an outer workspace whose output directory is
        // not literally named `target`, with a second real workspace whose
        // sources live inside that output directory. `cargo clean` on the outer
        // deleted the inner's Cargo.toml, Cargo.lock, and `src/` with exit 0 and
        // no diagnostic, because the inner's *own* output was outside the
        // covering root and so was refused -- which does not protect its source.
        let d = tempfile::tempdir().unwrap();
        // `workspace_with_output` canonicalizes every root, member and target,
        // so `covering` holds canonical paths. On macOS the temporary directory
        // sits under a symlinked prefix (`/var` -> `/private/var`), so a group
        // built from `d.path()` was compared against a spelling production
        // never produces and the case failed on the shape rather than on the
        // invariant. Derive every path from the canonical root.
        let base = std::fs::canonicalize(d.path()).unwrap();
        let outer_root = base.join("outer");
        let outer_out = outer_root.join("out");
        let inner_root = outer_out.join("inner");
        std::fs::create_dir_all(inner_root.join("src")).unwrap();
        std::fs::write(inner_root.join("src/lib.rs"), b"fn main(){}").unwrap();
        // The inner workspace's own output lives outside the covering root.
        let inner_out = base.join("inner-target");
        std::fs::create_dir_all(&inner_out).unwrap();

        let outer = workspace_with_output(
            &outer_root,
            vec![outer_root.clone()],
            Some(outer_out.clone()),
            false,
        );
        let inner = workspace_with_output(
            &inner_root,
            vec![inner_root.clone()],
            Some(inner_out),
            false,
        );
        let groups = build_groups(&[outer, inner]);
        assert_eq!(groups.len(), 2, "{groups:?}");

        let outer_group = groups
            .iter()
            .find(|g| g.covering.contains(&outer_out))
            .expect("the outer output root must form its own group");
        assert_eq!(
            outer_group.ownership,
            OutputOwnershipClass::Uncertain,
            "a covering root holding another workspace's sources is not an artifact"
        );
        assert!(outer_group.source_overlap);
        assert!(
            !crate::cleanup::covering_is_authorized(
                outer_group.ownership,
                &outer_group.covering,
                &outer_root,
                &[],
            )
            .unwrap(),
            "the group that would delete the inner project must never be authorized"
        );
    }

    #[cfg(unix)]
    #[test]
    fn another_workspaces_unrepresentable_root_blocks_a_private_claim() {
        // H4: workspace B's target is a symlink into A's target. B contributes
        // no node to the physical graph, so A's group must not claim to be
        // private while B is in the universe.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let root_a = d.path().join("a");
        let root_b = d.path().join("b");
        let target_a = root_a.join("target");
        std::fs::create_dir_all(&target_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        symlink(&target_a, root_b.join("target")).unwrap();

        let a = workspace_with_output(&root_a, vec![root_a.clone()], Some(target_a), false);
        let b = workspace_with_output(&root_b, vec![root_b.clone()], None, true);
        let groups = build_groups(&[a.clone(), b]);
        assert_eq!(groups.len(), 1, "B contributes no graph node");
        assert_ne!(
            groups[0].ownership,
            OutputOwnershipClass::PrivateBounded,
            "exclusivity is unprovable while another root is unrepresentable"
        );
        // Without B in the universe the claim is provable again.
        let alone = build_groups(std::slice::from_ref(&a));
        assert_eq!(alone[0].ownership, OutputOwnershipClass::PrivateBounded);
    }

    #[test]
    fn disjoint_groups_stay_private_despite_a_far_away_unrepresentable_root() {
        // M-1: one unrepresentable root anywhere used to demote every disjoint
        // group to `Shared`. Exclusivity is per group: groups disjoint from
        // every known alias of the unrepresentable root keep their claim,
        // while the overlapping shape above still blocks. This fails on the
        // old global flag (both groups `Shared`) and passes on per-group
        // overlap.
        let d = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(d.path()).unwrap();
        let root_a = base.join("a");
        let root_b = base.join("b");
        let far = base.join("far");
        let target_a = root_a.join("target");
        let target_b = root_b.join("target");
        std::fs::create_dir_all(&target_a).unwrap();
        std::fs::create_dir_all(&target_b).unwrap();
        std::fs::create_dir_all(&far).unwrap();
        let a = workspace_with_output(&root_a, vec![root_a.clone()], Some(target_a), false);
        let b = workspace_with_output(&root_b, vec![root_b.clone()], Some(target_b), false);
        // Uncertain workspace far from both groups: a symlink-claimed target
        // with nothing on disk (no resolvable link target, so only the
        // logical spelling is known) plus an unresolvable build directory.
        // Neither alias overlaps either covering root.
        let c = ResolvedWorkspace {
            id: WorkspaceId(far.clone()),
            root: far.clone(),
            root_manifest: far.join("Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: far.join("target"),
                    physical_path: None,
                    exists: true,
                    is_symlink: true,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: far.join("build"),
                    physical_path: None,
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        let groups = build_groups(&[a, b, c]);
        assert_eq!(groups.len(), 2, "the uncertain root contributes no node");
        for g in &groups {
            assert_eq!(
                g.ownership,
                OutputOwnershipClass::PrivateBounded,
                "a group disjoint from every unrepresentable alias stays private: {}",
                g.display.display()
            );
        }
    }

    #[test]
    fn source_tree_output_is_reported_as_a_diagnostic_not_silently_dropped() {
        // The C1 group is dropped from the inventory, so the user has to be told
        // why their project is missing.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(ws_root.join("target/inner")).unwrap();
        let ws = workspace_with_output(
            &ws_root,
            vec![ws_root.clone()],
            Some(ws_root.clone()),
            false,
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let start = SystemTime::now();
        analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            start,
            std::time::Duration::from_secs(300),
            &mut counters,
            &mut diagnostics,
            &crate::progress::NoopObserver,
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("contains a workspace source tree")),
            "{diagnostics:?}"
        );
        assert!(counters.uncertain_skipped >= 1);
    }

    #[test]
    fn nested_build_inside_target_is_counted_once() {
        // M3: with `build-dir` nested inside `target-dir` both physicals land in
        // one group whose covering root is the parent, so the reported size is
        // the parent's size - never the sum of parent and child.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        let build = target.join("build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("artifact"), vec![0u8; 4096]).unwrap();
        let old = SystemTime::now() - std::time::Duration::from_secs(7200);
        let artifact = build.join("artifact");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&artifact)
            .unwrap()
            .set_modified(old)
            .unwrap();
        for directory in [&build, &target, &ws_root] {
            // The workspace root's own mtime participates in the inactivity
            // verdict, so this must genuinely apply on every platform.
            set_path_modified(directory, old).unwrap();
        }
        let canonical_target = std::fs::canonicalize(&target).unwrap();
        let canonical_build = std::fs::canonicalize(&build).unwrap();
        let ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: std::fs::canonicalize(&ws_root).unwrap(),
            root_manifest: ws_root.join("Cargo.toml"),
            members: vec![WorkspaceMember {
                manifest_path: ws_root.join("Cargo.toml"),
                source_root: ws_root.clone(),
            }],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: target.clone(),
                    physical_path: Some(canonical_target),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: build,
                    physical_path: Some(canonical_build),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1, "nested roots form one group");
        assert_eq!(
            groups[0].covering,
            vec![std::fs::canonicalize(&target).unwrap()]
        );
        assert_eq!(groups[0].physicals.len(), 2);
        let mut counters = ScanCounters::default();
        let mut diagnostics = Vec::new();
        let start = SystemTime::now() - std::time::Duration::from_secs(7200);
        let measured = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            start,
            std::time::Duration::from_secs(300),
            &mut counters,
            &mut diagnostics,
            &crate::progress::NoopObserver,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(measured.len(), 1);
        let alone = traverse::measure_single_target(&std::fs::canonicalize(&target).unwrap());
        assert_eq!(
            measured[0].bytes, alone.bytes,
            "the union is measured once, not as parent + child"
        );
    }

    #[test]
    fn nested_target_build_deduped() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        let build = target.join("build");
        std::fs::create_dir_all(&build).unwrap();
        let ct = std::fs::canonicalize(&target).unwrap();
        let cb = std::fs::canonicalize(&build).unwrap();
        let ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: std::fs::canonicalize(&ws_root).unwrap(),
            root_manifest: ws_root.join("Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: target,
                    physical_path: Some(ct),
                    exists: true,
                    is_symlink: false,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: build,
                    physical_path: Some(cb),
                    exists: true,
                    is_symlink: false,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        };
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].covering.len(), 1, "nested must measure once");
    }

    #[test]
    fn symlink_output_is_uncertain() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: ws_root.clone(),
            root_manifest: ws_root.join("Cargo.toml"),
            members: vec![],
            packages: vec![],
            output: OutputSet {
                target: OutputRoot {
                    kind: OutputRootKind::Target,
                    logical_path: ws_root.join("target"),
                    physical_path: None,
                    exists: true,
                    is_symlink: true,
                },
                build: OutputRoot {
                    kind: OutputRootKind::Build,
                    logical_path: ws_root.join("target"),
                    physical_path: None,
                    exists: true,
                    is_symlink: true,
                },
            },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Unknown,
                metadata_had_build_directory: false,
                env_build_dir_set: false,
            },
        };
        let groups = build_groups(std::slice::from_ref(&ws));
        // Symlink roots produce no physical nodes; no groups, but workspace is
        // uncertain. Caller treats missing groups as skipped.
        assert!(groups.is_empty());
    }

    #[test]
    fn outermost_dedupes_nested_roots() {
        let roots = vec![
            PathBuf::from("/a"),
            PathBuf::from("/a/b"),
            PathBuf::from("/c"),
        ];
        assert_eq!(
            outermost(&roots),
            vec![PathBuf::from("/a"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn observer_counts_progress_without_allocation_per_entry() {
        let o = TestObserver::new();
        o.dirs_visited(100);
        o.dirs_pruned(5);
        o.manifests_found(2);
        use std::sync::atomic::Ordering;
        assert_eq!(o.visited.load(Ordering::Relaxed), 100);
    }

    fn make_workspace(
        root: &Path,
        target_physical: Option<PathBuf>,
        build_physical: Option<PathBuf>,
        members: Vec<PathBuf>,
    ) -> ResolvedWorkspace {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let target_logical = root.join("target");
        let build_logical = root.join("build");
        let target = match target_physical {
            Some(p) => OutputRoot {
                kind: OutputRootKind::Target,
                logical_path: target_logical,
                physical_path: Some(p),
                exists: true,
                is_symlink: false,
            },
            None => OutputRoot {
                kind: OutputRootKind::Target,
                logical_path: root.join("target"),
                physical_path: None,
                exists: false,
                is_symlink: false,
            },
        };
        let build = match build_physical {
            Some(p) => OutputRoot {
                kind: OutputRootKind::Build,
                logical_path: build_logical,
                physical_path: Some(p),
                exists: true,
                is_symlink: false,
            },
            None => OutputRoot {
                kind: OutputRootKind::Build,
                logical_path: root.join("build"),
                physical_path: None,
                exists: false,
                is_symlink: false,
            },
        };
        let member_records = members
            .into_iter()
            .map(|s| WorkspaceMember {
                manifest_path: s.join("Cargo.toml"),
                source_root: s,
            })
            .collect();
        ResolvedWorkspace {
            id: WorkspaceId(canonical_root.clone()),
            root: canonical_root,
            root_manifest: root.join("Cargo.toml"),
            members: member_records,
            packages: vec![],
            output: OutputSet { target, build },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
        }
    }

    /// Set a path's modification time, for a file *or* a directory.
    ///
    /// `std::fs::File::open` cannot open a directory on Windows, so a plain
    /// `set_modified` on a directory is a no-op there. `traverse::workspace_member_activity`
    /// includes the member root directory in its recency verdict, so a
    /// silently un-backdated workspace root would make every "this group is
    /// inactive" fixture mean something different per platform. Windows needs a
    /// handle opened with `FILE_FLAG_BACKUP_SEMANTICS` plus `SetFileTime`.
    fn set_path_modified(path: &Path, when: SystemTime) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
            use windows_sys::Win32::Storage::FileSystem::{
                CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, SetFileTime,
            };

            // Windows file times count 100-nanosecond intervals since
            // 1601-01-01, which is 11644473600 seconds after the Unix epoch.
            const EPOCH_OFFSET_TICKS: u128 = 11_644_473_600 * 10_000_000;
            // FILE_WRITE_ATTRIBUTES: the minimum right that permits setting a
            // timestamp, and the one right a directory handle can be granted.
            const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
            let since_epoch = when.duration_since(SystemTime::UNIX_EPOCH).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "timestamp predates the Unix epoch",
                )
            })?;
            let ticks =
                u64::try_from(since_epoch.as_nanos() / 100 + EPOCH_OFFSET_TICKS).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "timestamp out of range")
                })?;
            let filetime = FILETIME {
                dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
                dwHighDateTime: (ticks >> 32) as u32,
            };

            let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            wide.push(0);
            // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives
            // the call, the security-attributes pointer is null as documented,
            // and the returned handle is closed on every path below.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    FILE_WRITE_ATTRIBUTES,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE || handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `handle` was opened above and is closed exactly once;
            // `filetime` outlives the call.
            let ok = unsafe {
                SetFileTime(
                    handle,
                    std::ptr::null(),
                    std::ptr::null(),
                    &filetime as *const FILETIME,
                )
            };
            // SAFETY: `handle` was successfully opened above.
            unsafe { CloseHandle(handle) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            if std::fs::symlink_metadata(path)?.is_dir() {
                std::fs::File::open(path)?.set_modified(when)
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(path)?
                    .set_modified(when)
            }
        }
    }

    fn backdate_tree(path: &Path, old: SystemTime) {
        let mut stack = vec![path.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(meta) = std::fs::symlink_metadata(&current) {
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir()
                    && let Ok(children) = std::fs::read_dir(&current)
                {
                    for child in children.flatten() {
                        stack.push(child.path());
                    }
                }
            }
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&current) {
                let _ = f.set_modified(old);
            } else {
                let _ = set_path_modified(&current, old);
            }
        }
        let _ = set_path_modified(path, old);
    }

    #[test]
    fn distinct_sibling_target_build_measured_once_per_root() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        let build = ws_root.join("build");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(target.join("t.bin"), vec![1u8; 512]).unwrap();
        std::fs::write(build.join("b.bin"), vec![2u8; 512]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let ct = std::fs::canonicalize(&target).unwrap();
        let cb = std::fs::canonicalize(&build).unwrap();
        let ws = make_workspace(
            &ws_root,
            Some(ct.clone()),
            Some(cb.clone()),
            vec![ws_root.clone()],
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        // Distinct siblings do not overlap, so they form two physical groups.
        assert_eq!(groups.len(), 2);
        for g in &groups {
            assert_eq!(g.covering.len(), 1);
            assert_eq!(g.ownership, OutputOwnershipClass::PrivateBounded);
        }
        let start = SystemTime::now() + Duration::from_secs(5);
        let cutoff = start.checked_sub(Duration::ZERO).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let eligible = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(eligible.len(), 2);
        let total_entries: u64 = eligible.iter().map(|g| g.artifact_entries).sum();
        assert_eq!(total_entries, 2);
        let total_bytes: u64 = eligible.iter().map(|g| g.bytes).sum();
        assert!(total_bytes >= 1024);
    }

    #[test]
    fn target_nested_under_build_deduped() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let build = ws_root.join("build");
        let target = build.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("a.bin"), vec![1u8; 256]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let cb = std::fs::canonicalize(&build).unwrap();
        let ct = std::fs::canonicalize(&target).unwrap();
        let ws = make_workspace(&ws_root, Some(ct), Some(cb), vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].covering.len(),
            1,
            "nested target under build measures once"
        );
    }

    #[test]
    fn cross_overlap_target_build_is_shared() {
        let d = tempfile::tempdir().unwrap();
        let shared = d.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("f.bin"), b"x").unwrap();
        let canonical = std::fs::canonicalize(&shared).unwrap();
        let ws1_root = d.path().join("ws1");
        let ws2_root = d.path().join("ws2");
        std::fs::create_dir_all(&ws1_root).unwrap();
        std::fs::create_dir_all(&ws2_root).unwrap();
        let ws1 = make_workspace(
            &ws1_root,
            Some(canonical.clone()),
            None,
            vec![ws1_root.clone()],
        );
        let ws2 = make_workspace(
            &ws2_root,
            None,
            Some(canonical.clone()),
            vec![ws2_root.clone()],
        );
        let groups = build_groups(&[ws1, ws2]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::Shared);
    }

    #[test]
    fn missing_and_empty_output_skips_without_source_or_deep_work() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        // Missing output: no physical roots.
        let ws_missing = make_workspace(&ws_root, None, None, vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws_missing));
        assert!(groups.is_empty());
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let start = SystemTime::now() + Duration::from_secs(5);
        let cutoff = start.checked_sub(Duration::ZERO).unwrap();
        let eligible = analyze_groups(
            std::slice::from_ref(&ws_missing),
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(eligible.is_empty());
        assert_eq!(counters.groups_measured, 0);

        // Empty output: exists but zero entries.
        let empty = ws_root.join("empty-target");
        std::fs::create_dir_all(&empty).unwrap();
        backdate_tree(&empty, old);
        let canonical_empty = std::fs::canonicalize(&empty).unwrap();
        let ws_empty = make_workspace(&ws_root, Some(canonical_empty), None, vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws_empty));
        assert_eq!(groups.len(), 1);
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let eligible = analyze_groups(
            std::slice::from_ref(&ws_empty),
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(eligible.is_empty());
        assert_eq!(counters.groups_measured, 0);
        assert!(counters.empty_no_output_skipped > 0);
    }

    #[test]
    fn recent_member_source_skips_deep_sizing() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("artifact.bin"), vec![1u8; 2048]).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("src.rs"), "old").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        // Make source recent after backdate.
        std::fs::write(ws_root.join("src.rs"), "recent").unwrap();
        let canonical = std::fs::canonicalize(&target).unwrap();
        let ws = make_workspace(&ws_root, Some(canonical), None, vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let eligible = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::from_secs(300),
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(eligible.is_empty(), "recent source must skip deep sizing");
        assert_eq!(counters.groups_measured, 0);
        assert!(counters.active_skipped > 0);
    }

    /// `stats_line` prints `active_skipped` next to `groups_measured`, so the
    /// counter has to count groups, not workspaces. One workspace owning two
    /// output groups is one "active project" to a human and two skipped groups
    /// to that ratio; the assertion above cannot tell the two apart because its
    /// fixture has one workspace and one group, where both countings give 1.
    #[test]
    fn one_workspace_with_two_groups_counts_one_active_skip_per_group() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("src.rs"), "old").unwrap();
        // Disjoint roots are two physical groups of one workspace, so the single
        // recent source file below decides both of them.
        let target = output_dir(&ws_root.join("target"), 2048);
        let build = output_dir(&ws_root.join("build"), 1024);
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        std::fs::write(ws_root.join("src.rs"), "recent").unwrap();
        let ws = make_workspace(
            &ws_root,
            Some(std::fs::canonicalize(&target).unwrap()),
            Some(std::fs::canonicalize(&build).unwrap()),
            vec![ws_root.clone()],
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 2, "distinct siblings do not overlap");
        assert!(
            groups.iter().all(|g| g.owners == vec![0]),
            "one workspace owns both groups: {:?}",
            groups.iter().map(|g| &g.owners).collect::<Vec<_>>()
        );
        let group_count = groups.len() as u64;
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let observer = TestObserver::new();
        let outcomes = analyze_groups_detailed(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::from_secs(300),
            &mut counters,
            &mut diags,
            &observer,
        );
        assert_eq!(
            outcomes.len() as u64,
            group_count,
            "one outcome per group, skipped or not"
        );
        assert_eq!(
            counters.active_skipped, 2,
            "two skipped groups, not one active workspace: {counters:?}"
        );
        assert_eq!(counters.groups_measured, 0);
        // Conservation for this fixture: both groups had entries and both owners
        // were certain, so no other gate may claim one. There is no global
        // identity here -- `missing_output_skipped` is also bumped per workspace
        // with no existing output, which contributes no group at all.
        assert_eq!(counters.empty_no_output_skipped, 0);
        assert_eq!(counters.missing_output_skipped, 0);
        assert_eq!(counters.uncertain_skipped, 0);
        assert_eq!(
            counters.active_skipped + counters.groups_measured,
            group_count,
            "every group leaves the pipeline through exactly one gate: {counters:?}"
        );
        for outcome in &outcomes {
            assert_eq!(
                outcome.skip,
                Some(GroupSkipReason::ActiveSource),
                "{outcome:?}"
            );
            assert!(outcome.measured.is_none(), "{outcome:?}");
        }
        // Progress keeps reporting the workspace once. The counter and the event
        // are deliberately different quantities, and only the counter reaches
        // `--stats`; this pins that they are not re-coupled by accident.
        assert_eq!(
            observer
                .active_skipped
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "one active workspace observed, while the counter counts its two groups"
        );
    }

    #[test]
    fn recent_shared_output_protects_group() {
        let d = tempfile::tempdir().unwrap();
        let shared = d.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("artifact.bin"), vec![1u8; 1024]).unwrap();
        let ws1_root = d.path().join("ws1");
        let ws2_root = d.path().join("ws2");
        for r in [&ws1_root, &ws2_root] {
            std::fs::create_dir_all(r).unwrap();
            std::fs::write(r.join("Cargo.toml"), "").unwrap();
            std::fs::write(r.join("lib.rs"), "old").unwrap();
        }
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(d.path(), old);
        // Touch shared output to be recent.
        std::fs::write(shared.join("artifact.bin"), vec![2u8; 1024]).unwrap();
        let canonical = std::fs::canonicalize(&shared).unwrap();
        let ws1 = make_workspace(
            &ws1_root,
            Some(canonical.clone()),
            None,
            vec![ws1_root.clone()],
        );
        let ws2 = make_workspace(
            &ws2_root,
            Some(canonical.clone()),
            None,
            vec![ws2_root.clone()],
        );
        let workspaces = vec![ws1, ws2];
        let groups = build_groups(&workspaces);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::Shared);
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let eligible = analyze_groups(
            &workspaces,
            groups,
            start,
            cutoff,
            Duration::from_secs(300),
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(eligible.is_empty(), "recent shared output protects group");
    }

    #[test]
    fn out_of_tree_member_is_included_and_protects() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let outside = d.path().join("outside-member");
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(target.join("a.bin"), vec![1u8; 512]).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(outside.join("Cargo.toml"), "").unwrap();
        std::fs::write(outside.join("lib.rs"), "old").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(d.path(), old);
        std::fs::write(outside.join("lib.rs"), "recent").unwrap();
        let canonical = std::fs::canonicalize(&target).unwrap();
        let ws = make_workspace(
            &ws_root,
            Some(canonical),
            None,
            vec![ws_root.clone(), outside.clone()],
        );
        assert_eq!(ws.members.len(), 2);
        assert!(ws.members.iter().any(|m| m.source_root == outside));
        let groups = build_groups(std::slice::from_ref(&ws));
        let start = SystemTime::now() + Duration::from_secs(1);
        let cutoff = start.checked_sub(Duration::from_secs(300)).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let eligible = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::from_secs(300),
            &mut counters,
            &mut diags,
            &noop,
        );
        assert!(
            eligible.is_empty(),
            "out-of-tree recent member protects workspace"
        );
    }

    #[test]
    fn progress_disabled_vs_enabled_same_results() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("a.bin"), vec![1u8; 1024]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let canonical = std::fs::canonicalize(&target).unwrap();
        let ws = make_workspace(&ws_root, Some(canonical), None, vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws));
        let start = SystemTime::now() + Duration::from_secs(5);
        let cutoff = start.checked_sub(Duration::ZERO).unwrap();
        let mut c1 = ScanCounters::default();
        let mut d1 = Vec::new();
        let noop = NoopObserver;
        let e1 = analyze_groups(
            std::slice::from_ref(&ws),
            groups.clone(),
            start,
            cutoff,
            Duration::ZERO,
            &mut c1,
            &mut d1,
            &noop,
        );
        let mut c2 = ScanCounters::default();
        let mut d2 = Vec::new();
        let test_obs = TestObserver::new();
        let e2 = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut c2,
            &mut d2,
            &test_obs,
        );
        assert_eq!(e1.len(), e2.len());
        assert_eq!(e1[0].bytes, e2[0].bytes);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_output_is_observed() {
        use std::os::unix::ffi::OsStringExt;
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let bad = target.join(std::ffi::OsString::from_vec(b"bad-\xff.bin".to_vec()));
        std::fs::write(&bad, vec![1u8; 256]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let canonical = std::fs::canonicalize(&target).unwrap();
        let ws = make_workspace(&ws_root, Some(canonical), None, vec![ws_root.clone()]);
        let groups = build_groups(std::slice::from_ref(&ws));
        let start = SystemTime::now() + Duration::from_secs(5);
        let cutoff = start.checked_sub(Duration::ZERO).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let eligible = analyze_groups(
            std::slice::from_ref(&ws),
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].artifact_entries, 1);
    }

    fn write_valid_package(manifest: &Path, name: &str) {
        let content =
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        std::fs::write(manifest, content).unwrap();
        let src_dir = manifest.parent().unwrap().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();
    }

    /// A fixture that lives **under** the process working directory, plus its
    /// forward relative spelling.
    ///
    /// The `..` case is not a valid test of C015. A path like
    /// `../../tmp/x/Cargo.toml` joined onto the manifest's own parent composes
    /// back to the right file, so the defect is invisible — the first version of
    /// these cases proved nothing that way. The defect needs a *forward*
    /// relative spelling, which is what a user produces by running
    /// `cd project && cargo-cleanme scan fixture`: the manifest is
    /// `fixture/Cargo.toml` and the child's working directory is `fixture/`, so
    /// the two compose to a path that does not exist.
    ///
    /// The fixture is created under the repo's gitignored `target/` so the tree
    /// is not polluted, and is removed on drop.
    struct ForwardFixture {
        root: PathBuf,
        relative_root: PathBuf,
    }

    impl Drop for ForwardFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn forward_fixture(name: &str) -> ForwardFixture {
        let cwd = std::env::current_dir().expect("a process working directory");
        let root = cwd
            .join("target")
            .join(format!("c015-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("fixture directory");
        let relative_root = root
            .strip_prefix(&cwd)
            .expect("the fixture is created under the working directory")
            .to_path_buf();
        assert!(
            !relative_root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "the spelling must be forward-only, got {}",
            relative_root.display()
        );
        ForwardFixture {
            root,
            relative_root,
        }
    }

    /// The premise of every locate-project call: the `--manifest-path` we hand
    /// Cargo must resolve against the working directory we hand it.
    ///
    /// This is the C015 assertion. Before the fix, a discovered relative
    /// manifest was passed through verbatim while `cwd` was the manifest's own
    /// parent, so Cargo was asked for a path that cannot exist -- the scan
    /// resolved zero workspaces, exited 0, and reported no groups. A recording
    /// runner catches that at the argument level, without needing the network.
    #[test]
    fn the_composed_locate_arguments_resolve_against_the_supplied_working_directory() {
        struct RecordingRunner {
            seen: std::sync::Mutex<Vec<(PathBuf, Vec<std::ffi::OsString>)>>,
        }

        impl CargoRunner for RecordingRunner {
            fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                self.seen
                    .lock()
                    .unwrap()
                    .push((cwd.to_path_buf(), args.to_vec()));
                Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: b"recorded".to_vec(),
                })
            }
        }

        let fixture = forward_fixture("args");
        write_valid_package(&fixture.root.join("Cargo.toml"), "relative-fixture");
        let relative_manifest = fixture.relative_root.join("Cargo.toml");
        assert!(
            !relative_manifest.is_absolute() && relative_manifest.exists(),
            "the case must exercise a real relative manifest, got {}",
            relative_manifest.display()
        );

        let runner = RecordingRunner {
            seen: std::sync::Mutex::new(Vec::new()),
        };
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        resolve_workspaces_with_coverage(
            std::slice::from_ref(&relative_manifest),
            &runner,
            &mut counters,
            &mut diags,
            &NoopObserver,
        );
        let seen = runner.seen.lock().unwrap().clone();
        assert!(
            !seen.is_empty(),
            "the locate path was never exercised, so nothing was proven"
        );
        for (cwd, args) in seen {
            let manifest_arg = args
                .last()
                .expect("the locate invocation ends with --manifest-path's value");
            let resolved = cwd.join(manifest_arg);
            assert!(
                resolved.exists(),
                "Cargo would be asked for {} (cwd {} + {}), which does not exist",
                resolved.display(),
                cwd.display(),
                manifest_arg.to_string_lossy()
            );
        }
    }

    /// A relative root and its absolute spelling must agree.
    ///
    /// C015: the relative spelling silently resolved zero Cargo workspaces,
    /// reported no groups, and still exited 0. This uses the real Cargo, because
    /// the defect was in the arguments the real Cargo receives; a stub that
    /// ignores the manifest path would not have detected it.
    #[test]
    fn a_relative_root_resolves_the_same_workspaces_as_its_absolute_spelling() {
        // The version this toolchain can actually resolve, or the case cannot
        // run. Announced rather than silently returned.
        let version = std::process::Command::new("cargo")
            .arg("--version")
            .output()
            .expect("cargo must be on PATH to run this case");
        assert!(
            version.status.success(),
            "cargo --version failed: {}",
            String::from_utf8_lossy(&version.stderr)
        );
        let reported = String::from_utf8_lossy(&version.stdout).into_owned();
        let minor: u32 = reported
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.split('.').nth(1))
            .and_then(|m| m.parse().ok())
            .unwrap_or_else(|| panic!("cannot read a cargo minor version from {reported:?}"));
        // `cargo metadata --no-deps` landed in 1.77, which is below this
        // project's 1.89 MSRV, so every supported toolchain can run this case.
        // An earlier version of this gate compared the *patch* component, so
        // `1.99.0` read as patch 0, fell under the threshold, and the case
        // silently skipped -- passing without proving anything, twice.
        if minor < 77 {
            eprintln!("skipping: cargo {reported} predates `metadata --no-deps` support");
            return;
        }

        let fixture = forward_fixture("spelling");
        write_valid_package(&fixture.root.join("Cargo.toml"), "spelling-fixture");
        std::fs::write(fixture.root.join("Cargo.lock"), "version = 4\n").unwrap();

        let runner = SystemCargoRunner;
        let mut absolute = ScanCounters::default();
        let mut absolute_diags = Vec::new();
        let absolute_ws = resolve_workspaces(
            &[fixture.root.join("Cargo.toml")],
            &runner,
            &mut absolute,
            &mut absolute_diags,
            &NoopObserver,
        );

        let relative_manifest = fixture.relative_root.join("Cargo.toml");
        assert!(
            !relative_manifest.is_absolute() && relative_manifest.exists(),
            "the case must exercise a real relative manifest, got {}",
            relative_manifest.display()
        );

        let mut relative = ScanCounters::default();
        let mut relative_diags = Vec::new();
        let relative_ws = resolve_workspaces(
            &[relative_manifest],
            &runner,
            &mut relative,
            &mut relative_diags,
            &NoopObserver,
        );

        assert_eq!(
            absolute_ws.len(),
            1,
            "the fixture must resolve one workspace"
        );
        assert_eq!(
            absolute_ws.len(),
            relative_ws.len(),
            "the two spellings resolved different workspace counts \
             (absolute {}, relative {}); relative diagnostics: {:?}",
            absolute_ws.len(),
            relative_ws.len(),
            relative_diags
        );
        assert_eq!(
            relative.unique_workspaces, absolute.unique_workspaces,
            "the two spellings made different numbers of unique workspace resolutions"
        );
        assert_eq!(
            relative.cargo_failures, absolute.cargo_failures,
            "the relative spelling made Cargo fail where the absolute one did not"
        );
        assert!(
            relative_diags.is_empty(),
            "the relative spelling produced diagnostics: {relative_diags:?}"
        );
        assert_eq!(relative_ws[0].members.len(), absolute_ws[0].members.len());
    }

    #[test]
    fn real_multi_member_workspace_maps_to_one() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("ws");
        std::fs::create_dir_all(root.join("member-a/src")).unwrap();
        std::fs::create_dir_all(root.join("member-b/src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"member-a\", \"member-b\"]\n",
        )
        .unwrap();
        write_valid_package(&root.join("member-a/Cargo.toml"), "member-a");
        write_valid_package(&root.join("member-b/Cargo.toml"), "member-b");
        std::fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
        let runner = SystemCargoRunner;
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let manifests = vec![
            root.join("member-a/Cargo.toml"),
            root.join("member-b/Cargo.toml"),
        ];
        let ws = resolve_workspaces(&manifests, &runner, &mut counters, &mut diags, &noop);
        assert_eq!(ws.len(), 1, "multi-member must collapse to one workspace");
        assert_eq!(ws[0].members.len(), 2);
        assert_eq!(
            ws[0]
                .packages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["member-a", "member-b"]
        );
        assert!(
            ws[0]
                .packages
                .iter()
                .all(|p| !p.id.is_empty() && !p.version.is_empty())
        );
        assert_eq!(counters.cargo_metadata_calls, 1);
    }

    #[test]
    fn real_manifest_without_local_target_but_redirected_output() {
        let d = tempfile::tempdir().unwrap();
        let proj = d.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        write_valid_package(&proj.join("Cargo.toml"), "proj");
        // No local target/ directory.
        assert!(!proj.join("target").exists());
        let external = d.path().join("external-target");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(external.join("artifact.bin"), vec![1u8; 1024]).unwrap();
        // Redirect via workspace-local .cargo/config.toml (Cargo authoritative).
        // Use forward slashes so the TOML string is valid on Windows
        // (backslashes would need escaping and break config parsing).
        std::fs::create_dir_all(proj.join(".cargo")).unwrap();
        let external_toml = external.display().to_string().replace('\\', "/");
        std::fs::write(
            proj.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = \"{external_toml}\"\n"),
        )
        .unwrap();
        // Backdate source so activity does not protect; keep output old.
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&proj, old);
        backdate_tree(&external, old);
        let runner = SystemCargoRunner;
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let ws = resolve_workspaces(
            std::slice::from_ref(&proj.join("Cargo.toml")),
            &runner,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(ws.len(), 1);
        let resolved_target = &ws[0].output.target.logical_path;
        let canonical_external = std::fs::canonicalize(&external).unwrap();
        let canonical_resolved =
            std::fs::canonicalize(resolved_target).unwrap_or_else(|_| resolved_target.clone());
        assert_eq!(canonical_resolved, canonical_external);
        // Discovery finds the manifest even with no local target.
        let groups = build_groups(&ws);
        assert_eq!(groups.len(), 1);
        // Redirected outside the workspace root is external-unproven (inventory-only).
        assert_eq!(groups[0].ownership, OutputOwnershipClass::ExternalUnproven);
    }

    #[test]
    fn real_conventional_workspace_is_private_bounded() {
        let d = tempfile::tempdir().unwrap();
        let proj = d.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::create_dir_all(proj.join("target")).unwrap();
        write_valid_package(&proj.join("Cargo.toml"), "proj");
        std::fs::write(proj.join("target/artifact.bin"), vec![1u8; 512]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&proj, old);
        let runner = SystemCargoRunner;
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let ws = resolve_workspaces(
            std::slice::from_ref(&proj.join("Cargo.toml")),
            &runner,
            &mut counters,
            &mut diags,
            &noop,
        );
        assert_eq!(ws.len(), 1);
        let groups = build_groups(&ws);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ownership, OutputOwnershipClass::PrivateBounded);
    }

    // --- C003 §7.1 CleanupUnit mapping: one unit per workspace, not per group.

    fn analyze_units(workspaces: &[ResolvedWorkspace]) -> (Vec<CleanupUnit>, ScanCounters) {
        let groups = build_groups(workspaces);
        let start = SystemTime::now();
        let cutoff = start.checked_sub(Duration::ZERO).unwrap();
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = NoopObserver;
        let outcomes = analyze_groups_detailed(
            workspaces,
            groups,
            start,
            cutoff,
            Duration::ZERO,
            &mut counters,
            &mut diags,
            &noop,
        );
        (build_cleanup_units(workspaces, &outcomes), counters)
    }

    fn output_dir(path: &Path, bytes: usize) -> PathBuf {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        std::fs::write(path.join("artifact.bin"), vec![1u8; bytes]).unwrap();
        path.to_path_buf()
    }

    #[test]
    fn distinct_target_and_build_become_one_unit_with_deduped_union() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
        let target = output_dir(&ws_root.join("target"), 4096);
        let build = output_dir(&ws_root.join("build"), 8192);
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let ws = make_workspace(
            &ws_root,
            Some(std::fs::canonicalize(&target).unwrap()),
            Some(std::fs::canonicalize(&build).unwrap()),
            vec![ws_root.clone()],
        );
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 2, "distinct siblings are two physical groups");
        assert!(
            groups
                .iter()
                .all(|g| g.ownership == OutputOwnershipClass::PrivateBounded)
        );
        let (units, _c) = analyze_units(std::slice::from_ref(&ws));
        assert_eq!(units.len(), 1, "one Cargo invocation => one CleanupUnit");
        let unit = &units[0];
        assert_eq!(unit.groups.len(), 2);
        assert_eq!(unit.covering.len(), 2, "deduplicated union of both roots");
        let target_group = unit
            .groups
            .iter()
            .find(|g| g.kinds == vec![OutputRootKind::Target])
            .unwrap();
        let build_group = unit
            .groups
            .iter()
            .find(|g| g.kinds == vec![OutputRootKind::Build])
            .unwrap();
        assert_eq!(target_group.kind_label(), "target");
        assert_eq!(build_group.kind_label(), "build");
        // Pre-clean union aggregates both affected groups exactly once.
        let parts: u64 = unit
            .groups
            .iter()
            .map(|g| g.measured.as_ref().unwrap().bytes)
            .sum();
        assert_eq!(unit.bytes, parts);
        assert!(unit.bytes > 4096, "sibling build bytes are included");
        assert!(unit.fully_measured());
    }

    #[test]
    fn equal_and_nested_roots_collapse_to_one_physical_group() {
        // target == build, and both nesting directions.
        for shape in ["equal", "build_under_target", "target_under_build"] {
            let d = tempfile::tempdir().unwrap();
            let ws_root = d.path().join("ws");
            std::fs::create_dir_all(&ws_root).unwrap();
            std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
            std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
            let (target, build) = match shape {
                "equal" => {
                    let t = output_dir(&ws_root.join("target"), 4096);
                    (t.clone(), t)
                }
                "build_under_target" => {
                    let t = ws_root.join("target");
                    std::fs::create_dir_all(&t).unwrap();
                    std::fs::write(
                        t.join("CACHEDIR.TAG"),
                        b"Signature: 8a477f597d28d172789f06886806bc55\n",
                    )
                    .unwrap();
                    std::fs::write(t.join("artifact.bin"), vec![1u8; 4096]).unwrap();
                    let b = output_dir(&t.join("build"), 2048);
                    (t, b)
                }
                _ => {
                    let b = ws_root.join("build");
                    std::fs::create_dir_all(&b).unwrap();
                    std::fs::write(
                        b.join("CACHEDIR.TAG"),
                        b"Signature: 8a477f597d28d172789f06886806bc55\n",
                    )
                    .unwrap();
                    std::fs::write(b.join("artifact.bin"), vec![1u8; 4096]).unwrap();
                    let t = output_dir(&b.join("target"), 2048);
                    (t, b)
                }
            };
            let old = SystemTime::now() - Duration::from_secs(3600);
            backdate_tree(&ws_root, old);
            let ws = make_workspace(
                &ws_root,
                Some(std::fs::canonicalize(&target).unwrap()),
                Some(std::fs::canonicalize(&build).unwrap()),
                vec![ws_root.clone()],
            );
            let groups = build_groups(std::slice::from_ref(&ws));
            assert_eq!(groups.len(), 1, "{shape}: equal/nested is one group");
            let (units, _c) = analyze_units(std::slice::from_ref(&ws));
            assert_eq!(units.len(), 1, "{shape}: one unit");
            assert_eq!(units[0].groups.len(), 1, "{shape}: one affected group");
            assert_eq!(units[0].covering.len(), 1, "{shape}: deduplicated union");
            assert_eq!(
                units[0].groups[0].kinds,
                vec![OutputRootKind::Target, OutputRootKind::Build],
                "{shape}: both kinds collapse into one physical group"
            );
            assert_eq!(units[0].groups[0].kind_label(), "target+build");
            assert_eq!(
                units[0].bytes,
                units[0].groups[0].measured.as_ref().unwrap().bytes
            );
        }
    }

    #[test]
    fn unit_with_unmeasured_sibling_is_not_fully_measured() {
        // Private target measured + sibling build directory that never became a
        // candidate: the unit exists but is not cleanable as a whole.
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
        let target = output_dir(&ws_root.join("target"), 4096);
        // Existing but never populated build directory.
        let build = ws_root.join("build");
        std::fs::create_dir_all(&build).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let ws = make_workspace(
            &ws_root,
            Some(std::fs::canonicalize(&target).unwrap()),
            Some(std::fs::canonicalize(&build).unwrap()),
            vec![ws_root.clone()],
        );
        let (units, _c) = analyze_units(std::slice::from_ref(&ws));
        assert_eq!(units.len(), 1);
        assert!(!units[0].fully_measured());
        let unmeasured = units[0]
            .groups
            .iter()
            .find(|g| g.measured.is_none())
            .expect("one unmeasured sibling");
        assert_eq!(unmeasured.skip, Some(GroupSkipReason::EmptyOutput));
        assert_eq!(unmeasured.kinds, vec![OutputRootKind::Build]);
    }

    #[test]
    fn workspace_without_reportable_output_is_not_a_cleanup_unit() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
        // Existing but empty output directory.
        let target = ws_root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let ws = make_workspace(
            &ws_root,
            Some(std::fs::canonicalize(&target).unwrap()),
            None,
            vec![ws_root.clone()],
        );
        let (units, _c) = analyze_units(std::slice::from_ref(&ws));
        assert!(units.is_empty(), "an empty output directory is not a unit");
    }

    #[test]
    fn units_are_ordered_by_descending_aggregated_bytes() {
        let d = tempfile::tempdir().unwrap();
        let mut workspaces = Vec::new();
        for (name, bytes) in [("small", 1024usize), ("large", 16384usize)] {
            let ws_root = d.path().join(name);
            std::fs::create_dir_all(&ws_root).unwrap();
            std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
            std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
            let t = output_dir(&ws_root.join("target"), bytes);
            let b = output_dir(&ws_root.join("build"), bytes / 2);
            let old = SystemTime::now() - Duration::from_secs(3600);
            backdate_tree(&ws_root, old);
            workspaces.push(make_workspace(
                &ws_root,
                Some(std::fs::canonicalize(&t).unwrap()),
                Some(std::fs::canonicalize(&b).unwrap()),
                vec![ws_root.clone()],
            ));
        }
        let (units, _c) = analyze_units(&workspaces);
        assert_eq!(units.len(), 2);
        assert!(units[0].bytes > units[1].bytes);
        assert!(units[0].groups.len() == 2 && units[1].groups.len() == 2);
    }

    #[test]
    fn unprovable_output_identity_makes_the_group_uncertain() {
        let d = tempfile::tempdir().unwrap();
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        std::fs::write(ws_root.join("Cargo.toml"), "").unwrap();
        std::fs::write(ws_root.join("lib.rs"), "old").unwrap();
        let target = output_dir(&ws_root.join("target"), 4096);
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate_tree(&ws_root, old);
        let mut ws = make_workspace(
            &ws_root,
            Some(std::fs::canonicalize(&target).unwrap()),
            None,
            vec![ws_root.clone()],
        );
        // Existing build directory whose physical identity cannot be proven.
        let build = ws_root.join("build");
        std::fs::create_dir_all(&build).unwrap();
        backdate_tree(&ws_root, old);
        ws.output.build = OutputRoot {
            kind: OutputRootKind::Build,
            logical_path: build.clone(),
            physical_path: None,
            exists: true,
            is_symlink: false,
        };
        let groups = build_groups(std::slice::from_ref(&ws));
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].ownership,
            OutputOwnershipClass::Uncertain,
            "one unprovable output root makes every group of the workspace uncertain"
        );
        let (units, _c) = analyze_units(std::slice::from_ref(&ws));
        assert!(
            units.is_empty(),
            "an unprovable output root can never produce a cleanup unit"
        );
    }

    #[test]
    #[cfg(unix)]
    fn output_root_symlink_with_trailing_slash_is_still_a_symlink() {
        // B1: `symlink_metadata("link/")` follows the link on POSIX, so the
        // trailing-slash spelling must still probe as a symlink, never as a
        // real directory with a physical identity.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = d.path().join("link");
        symlink(&real, &link).unwrap();
        let mut diagnostics = Vec::new();
        let probed = output_root(
            OutputRootKind::Target,
            PathBuf::from(format!("{}/", link.display())),
            &mut diagnostics,
        );
        assert!(
            probed.is_symlink,
            "a symlinked output root with a trailing slash must probe as a symlink"
        );
        assert!(
            probed.physical_path.is_none(),
            "a symlinked output root must never gain a physical identity"
        );
        let mut diagnostics = Vec::new();
        let probed = output_root(
            OutputRootKind::Target,
            PathBuf::from(format!("{}/.", link.display())),
            &mut diagnostics,
        );
        assert!(
            probed.is_symlink,
            "a symlinked output root with a trailing /. must probe as a symlink"
        );
        assert!(probed.physical_path.is_none());
    }

    #[test]
    #[cfg(unix)]
    fn unreadable_output_root_is_uncertain_never_absent() {
        // B2: an output root that cannot be inspected (EACCES here) must
        // report `exists: true` with no physical identity, so `build_groups`
        // marks the workspace uncertain instead of silently dropping the
        // root from the exclusivity check.
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let parent = d.path().join("parent");
        let child = parent.join("target");
        std::fs::create_dir_all(&child).unwrap();
        let mut perms = std::fs::metadata(&parent).unwrap().permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&parent, perms).unwrap();
        // Premise probe: a privileged run (root) still inspects through
        // 0o000, so the fixture cannot produce the failure it tests and the
        // case exits instead of reporting green for nothing. Permissions are
        // restored before asserting so TempDir cleanup can run.
        let producible = std::fs::symlink_metadata(&child).is_err();
        let (probed, diagnostics) = if producible {
            let mut diagnostics = Vec::new();
            let probed = output_root(OutputRootKind::Target, child, &mut diagnostics);
            (Some(probed), Some(diagnostics))
        } else {
            (None, None)
        };
        let mut perms = std::fs::metadata(&parent).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&parent, perms).unwrap();
        let (Some(probed), Some(diagnostics)) = (probed, diagnostics) else {
            return;
        };
        assert!(probed.exists, "an unreadable output root is not absent");
        assert!(
            probed.physical_path.is_none(),
            "an unreadable output root has no provable identity"
        );
        assert!(
            !diagnostics.is_empty(),
            "an unreadable output root must leave a diagnostic"
        );
    }

    // ADR 009 — the working directory joins the maintenance scope.
    // ---------------------------------------------------------------------------

    /// ADR 009: the enclosing workspace is Cargo's own answer, asked in the
    /// operator's directory.
    ///
    /// The child gets the directory as its working directory and **no**
    /// `--manifest-path`: that option names a manifest to report on, not a place
    /// to search from, so it would answer for a manifest instead of for the
    /// operator's position.
    #[test]
    fn the_enclosing_workspace_is_asked_for_in_the_working_directory() {
        struct RecordingRunner {
            seen: std::sync::Mutex<Vec<(PathBuf, Vec<std::ffi::OsString>)>>,
            root_manifest: PathBuf,
        }
        impl CargoRunner for RecordingRunner {
            fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                self.seen
                    .lock()
                    .unwrap()
                    .push((cwd.to_path_buf(), args.to_vec()));
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::json!({"root": self.root_manifest})
                        .to_string()
                        .into_bytes(),
                    stderr: Vec::new(),
                })
            }
        }

        let d = tempfile::tempdir().unwrap();
        let workspace = d.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let runner = RecordingRunner {
            seen: std::sync::Mutex::new(Vec::new()),
            root_manifest: workspace.join("Cargo.toml"),
        };

        let member = workspace.join("crates/inner");
        std::fs::create_dir_all(&member).unwrap();
        let root = enclosing_workspace_root(&member, &runner).expect("an enclosing workspace");
        assert_eq!(root, workspace);

        let seen = runner.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "one spawn, not one per candidate: {seen:?}");
        assert_eq!(
            seen[0].0, member,
            "Cargo must be asked from the operator's directory"
        );
        let args: Vec<String> = seen[0]
            .1
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec!["locate-project".to_string(), "--workspace".to_string()],
            "the question is 'which workspace owns this directory', so there is no --manifest-path: {args:?}"
        );
    }

    /// ADR 009: Cargo refusing to name an enclosing workspace is a normal answer.
    ///
    /// The working directory remains a scope root on its own, so the worst case
    /// is that the enclosing workspace is not cleaned — never that something is
    /// cleaned without proof.
    #[test]
    fn a_refused_enclosing_workspace_is_not_an_error() {
        struct RefusingRunner;
        impl CargoRunner for RefusingRunner {
            fn run(&self, _cwd: &Path, _args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                Ok(ProcessOutput {
                    success: false,
                    code: Some(101),
                    stdout: Vec::new(),
                    stderr: b"error: could not find `Cargo.toml`".to_vec(),
                })
            }
        }
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            enclosing_workspace_root(d.path(), &RefusingRunner),
            None,
            "a refusal means no enclosing workspace, not a failure"
        );

        struct BrokenRunner;
        impl CargoRunner for BrokenRunner {
            fn run(&self, _cwd: &Path, _args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                Err(io::Error::other("transport"))
            }
        }
        assert_eq!(
            enclosing_workspace_root(d.path(), &BrokenRunner),
            None,
            "a transport failure means no enclosing workspace, not a failure"
        );

        struct GarbageRunner;
        impl CargoRunner for GarbageRunner {
            fn run(&self, _cwd: &Path, _args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: b"not json".to_vec(),
                    stderr: Vec::new(),
                })
            }
        }
        assert_eq!(
            enclosing_workspace_root(d.path(), &GarbageRunner),
            None,
            "an unparseable answer means no enclosing workspace, not a failure"
        );
    }

    /// ADR 009: the working directory contributes its enclosing workspace *and*
    /// itself.
    ///
    /// The enclosing workspace is not optional. A member directory's manifest sits
    /// above the member, so a root at the member cannot discover it.
    #[test]
    fn the_working_directory_contributes_the_enclosing_workspace_and_itself() {
        struct FixedRunner {
            root_manifest: PathBuf,
        }
        impl CargoRunner for FixedRunner {
            fn run(&self, _cwd: &Path, _args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::json!({"root": self.root_manifest})
                        .to_string()
                        .into_bytes(),
                    stderr: Vec::new(),
                })
            }
        }

        let d = tempfile::tempdir().unwrap();
        let workspace = d.path().join("workspace");
        let member = workspace.join("crates/inner");
        std::fs::create_dir_all(&member).unwrap();
        let runner = FixedRunner {
            root_manifest: workspace.join("Cargo.toml"),
        };

        let roots = working_directory_scope_roots(&member, &runner);
        assert_eq!(
            roots,
            vec![workspace.clone(), member.clone()],
            "the enclosing workspace and the working directory are both scope roots"
        );

        // Standing in the workspace root itself names it once, not twice.
        let roots = working_directory_scope_roots(&workspace, &runner);
        assert_eq!(
            roots,
            vec![workspace.clone()],
            "the working directory and its enclosing workspace are the same root: {roots:?}"
        );
    }
}
