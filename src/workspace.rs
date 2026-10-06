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

pub trait CargoRunner {
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
fn is_filesystem_root(path: &Path) -> bool {
    path.parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
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

/// Bounded first meaningful line of a failed Cargo invocation's stderr.
///
/// The other failure modes in this module carry their cause — "could not start:
/// {e}", "cannot parse … output: {e}" — so a bare "cargo metadata failed" is the
/// one shape that leaves the user with a project missing from every report and
/// no way to learn why. Cargo already names the offending manifest on stderr,
/// and the remedy is "fix the manifest", which is not guessable from a label.
///
/// Only the length is bounded: a diagnostic message is serialized verbatim into
/// JSON, the log line, and the human report, and Cargo's stderr is unbounded.
/// Content is left intact — it is Cargo talking about the user's own paths,
/// which that user invoked the command to find out about.
fn cargo_failure_reason(stderr: &[u8]) -> String {
    const MAX_REASON_BYTES: usize = 200;
    let text = String::from_utf8_lossy(stderr);
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if first.is_empty() {
        return "cargo wrote nothing to stderr".into();
    }
    let mut end = MAX_REASON_BYTES.min(first.len());
    while end > 0 && !first.is_char_boundary(end) {
        end -= 1;
    }
    if end == first.len() {
        return first.to_owned();
    }
    format!("{}… (truncated)", &first[..end])
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
        Err(e) => {
            // Unreadable is not absent: say so, and keep the root unusable.
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
                exists: false,
                is_symlink: false,
            }
        }
        Ok(meta) if meta.file_type().is_symlink() => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: true,
            is_symlink: true,
        },
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

/// Resolve manifests to unique workspaces with caching.
///
/// Returns workspaces in deterministic order plus diagnostics. Cache key is the
/// canonical workspace root manifest from locate-project.
///
/// C002 §7.6: after the first successful metadata resolution for a workspace,
/// every member manifest path returned by that authoritative metadata is
/// registered as mapping to the workspace. Later discovered manifests known
/// from that metadata require neither another locate nor another metadata
/// process. For an N-member workspace discovered in arbitrary order, expected
/// successful Cargo calls are one locate and one metadata. Membership is never
/// inferred from path ancestry or hand-parsed TOML.
pub fn resolve_workspaces(
    manifests: &[PathBuf],
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Vec<ResolvedWorkspace> {
    resolve_workspaces_with_coverage(manifests, runner, counters, diagnostics, observer).workspaces
}

/// Resolve discovered manifests while retaining proof of which manifests were
/// authoritatively represented by successful Cargo workspace metadata.
/// Read-only scan callers can continue using [`resolve_workspaces`], whose
/// partial-result behavior is intentionally unchanged.
pub fn resolve_workspaces_with_coverage(
    manifests: &[PathBuf],
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> ResolutionCoverage {
    let mut cache: HashMap<PathBuf, ResolvedWorkspace> = HashMap::new();
    let mut ordered_keys: Vec<PathBuf> = Vec::new();
    let mut locate_cache: HashMap<PathBuf, PathBuf> = HashMap::new();
    // Authoritative member -> root mapping seeded from Cargo metadata.
    let mut member_to_root: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut authoritative_coverage: HashSet<PathBuf> = HashSet::new();
    for manifest in manifests {
        // C002 fast path: if this manifest was listed as a member (or root)
        // by an earlier successful metadata response, reuse that workspace
        // without any Cargo subprocess.
        let manifest_key = canonical_manifest_key(manifest);
        if let Some(root_key) = member_to_root.get(&manifest_key)
            && cache.contains_key(root_key)
        {
            counters.deduped_workspace_hits += 1;
            authoritative_coverage.insert(manifest_key);
            continue;
        }
        let locate_args: Vec<std::ffi::OsString> =
            ["locate-project", "--workspace", "--manifest-path"]
                .into_iter()
                .map(Into::into)
                // Anchored for Cargo, not for the diagnostic below: the recorded
                // path keeps the spelling the user gave.
                .chain(std::iter::once(
                    manifest_path_for_cargo(manifest).into_os_string(),
                ))
                .collect();
        counters.cargo_locate_calls += 1;
        let locate_start = std::time::Instant::now();
        let cwd = manifest.parent().unwrap_or(Path::new("/"));
        let locate_out = match runner.run(cwd, &locate_args) {
            Ok(o) => o,
            Err(e) => {
                counters.cargo_locate_nanos = counters
                    .cargo_locate_nanos
                    .saturating_add(crate::domain::elapsed_nanos(locate_start.elapsed()));
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: format!("cargo locate-project could not start: {e}"),
                });
                continue;
            }
        };
        counters.cargo_locate_nanos = counters
            .cargo_locate_nanos
            .saturating_add(crate::domain::elapsed_nanos(locate_start.elapsed()));
        if !locate_out.success {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.clone()),
                message: format!(
                    "cargo locate-project failed: {}",
                    cargo_failure_reason(&locate_out.stderr)
                ),
            });
            continue;
        }
        let locate: LocateOutput = match serde_json::from_slice(&locate_out.stdout) {
            Ok(v) => v,
            Err(e) => {
                counters.cargo_failures += 1;
                observer.cargo_failure();
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(manifest.clone()),
                    message: format!("cannot parse cargo locate-project output: {e}"),
                });
                continue;
            }
        };
        let root_manifest = PathBuf::from(&locate.root);
        let canonical_key = canonical_or_absolute(&root_manifest).unwrap_or(root_manifest.clone());
        if locate_cache.contains_key(&canonical_key) {
            counters.deduped_workspace_hits += 1;
            // Seed this manifest as a known member for future lookups.
            // `manifest_key` *is* `canonical_manifest_key(manifest)`, already
            // computed above; re-canonicalizing it is a syscall for nothing.
            authoritative_coverage.insert(manifest_key.clone());
            member_to_root.insert(manifest_key, canonical_key);
            continue;
        }
        if cache.contains_key(&canonical_key) {
            counters.deduped_workspace_hits += 1;
            locate_cache.insert(canonical_key.clone(), root_manifest);
            authoritative_coverage.insert(manifest_key.clone());
            member_to_root.insert(manifest_key, canonical_key);
            continue;
        }
        // Metadata once per workspace.
        if let Some(ws) = resolve_one_workspace_cached(
            &root_manifest,
            &canonical_key,
            runner,
            counters,
            diagnostics,
            observer,
            manifest,
        ) {
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
            locate_cache.insert(canonical_key, root_manifest);
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
            let stage = if message.contains("locate-project") {
                "locate"
            } else if message.contains("metadata") {
                "metadata"
            } else {
                "identity"
            };
            Some(UnresolvedOwnershipParticipant {
                manifest: manifest.clone(),
                stage,
                reason: message,
            })
        })
        .collect();
    unresolved.sort_by(|a, b| a.manifest.cmp(&b.manifest));
    // Sort by canonical identity first, then dedup on the plain spelling. The
    // original compared `canonical_manifest_key` on both sides of `dedup_by`,
    // which canonicalizes once per comparison — O(n log n) syscalls for a set
    // that is almost always already unique.
    unresolved.sort_by_cached_key(|p| canonical_manifest_key(&p.manifest));
    unresolved.dedup_by(|a, b| a.manifest == b.manifest);
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
    let cwd = root_manifest.parent().unwrap_or(Path::new("/"));
    counters.cargo_metadata_calls += 1;
    let meta_start = std::time::Instant::now();
    let meta_out = match runner.run(cwd, &meta_args) {
        Ok(o) => {
            counters.cargo_metadata_nanos = counters
                .cargo_metadata_nanos
                .saturating_add(crate::domain::elapsed_nanos(meta_start.elapsed()));
            o
        }
        Err(e) => {
            counters.cargo_metadata_nanos = counters
                .cargo_metadata_nanos
                .saturating_add(crate::domain::elapsed_nanos(meta_start.elapsed()));
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: format!("cargo metadata could not start: {e}"),
            });
            return None;
        }
    };
    // Count wall time for failed metadata responses as well.
    // Success path already accumulated above; failure `success == false`
    // still consumed a subprocess, so timing is already recorded.
    if !meta_out.success {
        counters.cargo_failures += 1;
        observer.cargo_failure();
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(original_manifest.to_path_buf()),
            message: format!(
                "cargo metadata failed: {}",
                cargo_failure_reason(&meta_out.stderr)
            ),
        });
        return None;
    }
    let stdout_str = String::from_utf8_lossy(&meta_out.stdout);
    let had_build = stdout_str.contains("build_directory");
    let raw: RawMetadata = match serde_json::from_slice(&meta_out.stdout) {
        Ok(v) => v,
        Err(e) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: format!("cannot parse cargo metadata output: {e}"),
            });
            return None;
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
        Err(msg) => {
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: msg,
            });
            return None;
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
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(original_manifest.to_path_buf()),
                message: format!(
                    "cannot establish physical workspace member source root for {}",
                    mp.display()
                ),
            });
            return None;
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
    })
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

/// Build physical-output groups from resolved workspaces.
///
/// Only existing non-symlink canonical roots participate. Symlink roots make
/// their workspace uncertain. Equal/ancestor-descendant overlaps form connected
/// groups. Classification:
/// - Uncertain: symlink, unresolvable, or missing physical identity, or an
///   output root that is (or contains) any resolved workspace's member source
///   tree, or one of Cargo's conventional source directories (`src`, `tests`,
///   `benches`, `examples`).
/// - Shared: multiple workspaces own overlapping physical output, or exclusivity
///   cannot be proven because some root in the universe has no graph node.
/// - PrivateBounded: single owner, every covering root strictly below the
///   workspace root, disjoint from every resolved member source root, and every
///   root in the universe represented in the graph.
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
    for (idx, ws) in workspaces.iter().enumerate() {
        for root in [&ws.output.target, &ws.output.build] {
            if root.is_symlink {
                uncertain_ws.insert(idx);
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
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            if contains(&nodes[i].physical, &nodes[j].physical)
                || contains(&nodes[j].physical, &nodes[i].physical)
            {
                union(&mut parent, i, j);
            }
        }
    }
    let mut groups_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..nodes.len() {
        let r = find(&mut parent, i);
        groups_map.entry(r).or_default().push(i);
    }
    let mut groups = Vec::new();
    // A root that contributes no node (symlink, no physical identity) is
    // invisible to the union-find below, yet it can still point into any group.
    // Exclusivity is therefore only provable while every root in the universe
    // is represented; otherwise no group may claim `PrivateBounded`.
    let exclusivity_unproven = !uncertain_ws.is_empty();
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
            });
            let inside_root = covering.iter().all(|c| strictly_below(&ws.root, c));
            if uncertain_ws.contains(&owners[0]) || source_overlap {
                // The owning workspace is uncertain, or its own source tree
                // would be deleted. Never a private artifact.
                OutputOwnershipClass::Uncertain
            } else if inside_root && !exclusivity_unproven {
                OutputOwnershipClass::PrivateBounded
            } else if inside_root {
                // Inside the workspace root but not provably exclusive: it may
                // be the output of a workspace whose roots are unrepresentable.
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
    /// A covering root is (or contains) a workspace member source root, so this
    /// group is source code rather than an artifact. Surfaced as a diagnostic:
    /// the group is dropped from the inventory, so silence would be misleading.
    pub source_overlap: bool,
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
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CandidateUncertain,
                    path: Some(g.display.clone()),
                    message: format!(
                        "output root {} contains a workspace source tree, not a build artifact; it is not sized and not eligible for cleanup",
                        g.display.display()
                    ),
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
        let is_recent = newest.is_some_and(|t| t >= clock_cutoff || t > clock_start);
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
    for root in member_roots {
        match traverse::workspace_member_activity(&root, all_outputs, start, cutoff) {
            Ok(traverse::SourceActivity::Recent(_)) => return Ok(true),
            Ok(traverse::SourceActivity::Quiet(_)) => {}
            Err(()) => return Err(()),
        }
    }
    Ok(false)
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
        // C002 §7.6: N-member workspace requires exactly one locate + one
        // metadata after first authoritative resolution.
        assert_eq!(counters.cargo_metadata_calls, 1);
        assert_eq!(
            counters.cargo_locate_calls, 1,
            "member cache must eliminate redundant locates"
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

    #[test]
    fn n_member_workspace_requires_one_locate_one_metadata_regardless_of_order() {
        // Discovery order with a member before the root still => one locate +
        // one metadata (C002 §9).
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
            assert_eq!(counters.cargo_locate_calls, 1, "order={order}");
        }
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
        let outer_root = d.path().join("outer");
        let outer_out = outer_root.join("out");
        let inner_root = outer_out.join("inner");
        std::fs::create_dir_all(inner_root.join("src")).unwrap();
        std::fs::write(inner_root.join("src/lib.rs"), b"fn main(){}").unwrap();
        // The inner workspace's own output lives outside the covering root.
        let inner_out = d.path().join("inner-target");
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
}
