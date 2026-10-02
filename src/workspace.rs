//! Workspace/output resolution, physical grouping, and fail-fast pipeline.
//!
//! Cargo is authoritative for workspace and output configuration. This module
//! never reimplements Cargo config merge rules. It uses `cargo locate-project`
//! and `cargo metadata` through a test-injectable runner, caches by workspace,
//! groups overlapping physical output, and measures each group once.

use crate::{domain::*, progress::ProgressObserver, traverse};
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
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
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

fn output_root(kind: OutputRootKind, logical: PathBuf) -> OutputRoot {
    match fs::symlink_metadata(&logical) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: false,
            is_symlink: false,
        },
        Err(_) => OutputRoot {
            kind,
            logical_path: logical,
            physical_path: None,
            exists: false,
            is_symlink: false,
        },
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

fn resolve_one_workspace(
    manifest: &Path,
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Option<ResolvedWorkspace> {
    // 1. locate-project
    let locate_args: Vec<std::ffi::OsString> = ["locate-project", "--workspace", "--manifest-path"]
        .into_iter()
        .map(Into::into)
        .chain(std::iter::once(manifest.as_os_str().to_owned()))
        .collect();
    counters.cargo_locate_calls += 1;
    let locate_out = match runner.run(manifest.parent().unwrap_or(Path::new("/")), &locate_args) {
        Ok(o) => o,
        Err(e) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.to_path_buf()),
                message: format!("cargo locate-project could not start: {e}"),
            });
            return None;
        }
    };
    if !locate_out.success {
        counters.cargo_failures += 1;
        observer.cargo_failure();
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(manifest.to_path_buf()),
            message: format!(
                "cargo locate-project failed: {}",
                String::from_utf8_lossy(&locate_out.stderr).trim()
            ),
        });
        return None;
    }
    let locate: LocateOutput = match serde_json::from_slice(&locate_out.stdout) {
        Ok(v) => v,
        Err(e) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.to_path_buf()),
                message: format!("cannot parse cargo locate-project output: {e}"),
            });
            return None;
        }
    };
    let root_manifest_path = PathBuf::from(&locate.root);
    let canonical_root_manifest = match canonical_or_absolute(&root_manifest_path) {
        Ok(p) => p,
        Err(msg) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.to_path_buf()),
                message: msg,
            });
            return None;
        }
    };

    // Caller handles caching; this resolves metadata for a fresh workspace.
    let _ = canonical_root_manifest;
    // 2. metadata
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
    .chain(std::iter::once(root_manifest_path.as_os_str().to_owned()))
    .collect();
    // Use workspace root dir as cwd for stable config resolution.
    let cwd = root_manifest_path
        .parent()
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    counters.cargo_metadata_calls += 1;
    let meta_out = match runner.run(&cwd, &meta_args) {
        Ok(o) => o,
        Err(e) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.to_path_buf()),
                message: format!("cargo metadata could not start: {e}"),
            });
            return None;
        }
    };
    if !meta_out.success {
        counters.cargo_failures += 1;
        observer.cargo_failure();
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(manifest.to_path_buf()),
            message: format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&meta_out.stderr).trim()
            ),
        });
        return None;
    }
    let raw: RawMetadata = match serde_json::from_slice(&meta_out.stdout) {
        Ok(v) => v,
        Err(e) => {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.to_path_buf()),
                message: format!("cannot parse cargo metadata output: {e}"),
            });
            return None;
        }
    };
    // Build resolved workspace. Unknown/malformed capability is conservative.
    let had_build = raw.build_directory.is_some()
        || String::from_utf8_lossy(&meta_out.stdout).contains("build_directory");
    // Re-derive had_build from JSON presence: serde default None could mean
    // absent or null; check raw string for field presence for fixture parity.
    // If metadata JSON lacks the key, capability is Unavailable/Unknown.
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
                path: Some(manifest.to_path_buf()),
                message: msg,
            });
            return None;
        }
    };
    let root_manifest_canonical = match fs::canonicalize(&root_manifest_path)
        .or_else(|_| canonical_or_absolute(&root_manifest_path))
    {
        Ok(p) => p,
        Err(_) => root_manifest_path.clone(),
    };
    let mut members = Vec::new();
    for pkg in &raw.packages {
        let mp = PathBuf::from(&pkg.manifest_path);
        // Only include packages that are workspace members? Raw packages with
        // --no-deps are workspace members only, so include all.
        let source_root = mp
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| mp.clone());
        members.push(WorkspaceMember {
            manifest_path: mp,
            source_root,
        });
    }
    // Ensure at least the root manifest is represented even if packages empty.
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
    let target_root = output_root(OutputRootKind::Target, target_logical);
    let build_root = output_root(OutputRootKind::Build, build_logical);

    Some(ResolvedWorkspace {
        id: WorkspaceId(workspace_root.clone()),
        root: workspace_root,
        root_manifest: root_manifest_canonical,
        members,
        output: OutputSet {
            target: target_root,
            build: build_root,
        },
        capability,
    })
}

/// Resolve manifests to unique workspaces with caching.
///
/// Returns workspaces in deterministic order plus diagnostics. Cache key is the
/// canonical workspace root manifest from locate-project.
pub fn resolve_workspaces(
    manifests: &[PathBuf],
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Vec<ResolvedWorkspace> {
    let mut cache: HashMap<PathBuf, ResolvedWorkspace> = HashMap::new();
    let mut ordered_keys: Vec<PathBuf> = Vec::new();
    // For caching we need locate first; to avoid double locate for same
    // workspace we still locate each manifest (cheap) but metadata once.
    // Plan requires: do not repeat metadata for members of same workspace.
    let mut locate_cache: HashMap<PathBuf, PathBuf> = HashMap::new();
    for manifest in manifests {
        // Fast path: if manifest itself is already a known root manifest, reuse.
        // We still need locate to collapse members to workspace identity.
        let locate_args: Vec<std::ffi::OsString> =
            ["locate-project", "--workspace", "--manifest-path"]
                .into_iter()
                .map(Into::into)
                .chain(std::iter::once(manifest.as_os_str().to_owned()))
                .collect();
        counters.cargo_locate_calls += 1;
        let cwd = manifest.parent().unwrap_or(Path::new("/"));
        let locate_out = match runner.run(cwd, &locate_args) {
            Ok(o) => o,
            Err(e) => {
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
        if !locate_out.success {
            counters.cargo_failures += 1;
            observer.cargo_failure();
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::CandidateUncertain,
                path: Some(manifest.clone()),
                message: "cargo locate-project failed".into(),
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
        if let Some(cached) = locate_cache.get(&canonical_key) {
            let _ = cached;
            counters.deduped_workspace_hits += 1;
            continue;
        }
        if cache.contains_key(&canonical_key) {
            counters.deduped_workspace_hits += 1;
            locate_cache.insert(canonical_key, root_manifest);
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
            cache.insert(canonical_key.clone(), ws);
            locate_cache.insert(canonical_key, root_manifest);
        }
    }
    ordered_keys
        .into_iter()
        .filter_map(|k| cache.remove(&k))
        .collect()
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
    let meta_out = match runner.run(cwd, &meta_args) {
        Ok(o) => o,
        Err(e) => {
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
    if !meta_out.success {
        counters.cargo_failures += 1;
        observer.cargo_failure();
        diagnostics.push(ScanDiagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::CandidateUncertain,
            path: Some(original_manifest.to_path_buf()),
            message: "cargo metadata failed".into(),
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
            .map(Path::to_path_buf)
            .unwrap_or_else(|| mp.clone());
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
    let target_root = output_root(OutputRootKind::Target, target_logical);
    let build_root = output_root(OutputRootKind::Build, build_logical);

    Some(ResolvedWorkspace {
        id: WorkspaceId(workspace_root.clone()),
        root: workspace_root,
        root_manifest: root_manifest_canonical,
        members,
        output: OutputSet {
            target: target_root,
            build: build_root,
        },
        capability,
    })
}

// Silence dead-code for the single-workspace helper used by tests/future reuse.
#[allow(dead_code)]
pub fn resolve_single_for_test(
    manifest: &Path,
    runner: &dyn CargoRunner,
    counters: &mut ScanCounters,
    diagnostics: &mut Vec<ScanDiagnostic>,
    observer: &dyn ProgressObserver,
) -> Option<ResolvedWorkspace> {
    resolve_one_workspace(manifest, runner, counters, diagnostics, observer)
}

/// Returns true if `maybe_child` equals or is contained in `parent`.
fn contains(parent: &Path, maybe_child: &Path) -> bool {
    maybe_child == parent || maybe_child.starts_with(parent)
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
/// - Uncertain: symlink, unresolvable, or missing physical identity.
/// - Shared: multiple workspaces own overlapping physical output.
/// - PrivateBounded: single owner, output inside workspace root.
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
        let ownership = if owners.len() > 1 {
            OutputOwnershipClass::Shared
        } else {
            let ws = &workspaces[owners[0]];
            // If workspace had any uncertain symlink root, mark Uncertain
            // only if that symlink overlaps this group? Conservative: if the
            // workspace is uncertain anywhere, groups it owns are Uncertain
            // when they cannot be proven exclusive. For M005A we mark the
            // group Uncertain if its sole owner is in uncertain_ws and the
            // group physicals include no proven exclusive root? Simplify:
            // if sole owner uncertain, mark Uncertain.
            if uncertain_ws.contains(&owners[0]) {
                OutputOwnershipClass::Uncertain
            } else if covering.iter().all(|c| contains(&ws.root, c)) {
                OutputOwnershipClass::PrivateBounded
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

/// Full fail-fast pipeline for resolved workspaces.
///
/// Gate order: existence/type -> artifact presence -> source activity -> deep
/// sizing. Returns eligible groups, updated counters, and diagnostics.
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
    // Batch-measure all covering roots with one bounded pool.
    let mut all_covering: Vec<(usize, PathBuf)> = Vec::new();
    // Map group idx -> covering indices for later aggregation.
    let mut group_covering: Vec<Vec<PathBuf>> = Vec::new();
    for g in &groups {
        group_covering.push(g.covering.clone());
        for c in &g.covering {
            let idx = all_covering.len();
            all_covering.push((idx, c.clone()));
        }
    }
    // Cheap presence gate before any source walk or deep sizing.
    // We check each covering root for at least one entry; groups with no
    // entries are skipped without source/deep work.
    let mut group_has_entries: Vec<bool> = Vec::new();
    for (gi, covering) in group_covering.iter().enumerate() {
        let _ = gi;
        let mut any = false;
        let mut missing_all = true;
        for c in covering {
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
        let owns_nonempty = groups
            .iter()
            .enumerate()
            .any(|(gi, g)| g.owners.contains(&wi) && *group_has_entries.get(gi).unwrap_or(&false));
        if !owns_nonempty {
            // Already counted as empty above; no source walk.
            ws_source_recent.insert(wi, false);
            continue;
        }
        match workspace_source_activity(ws, &all_outputs, clock_start, clock_cutoff) {
            Ok(true) => {
                ws_source_recent.insert(wi, true);
                counters.active_skipped += 1;
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
    let mut survivor_covering: Vec<(usize, PathBuf)> = Vec::new();
    let mut survivor_group_idx: Vec<usize> = Vec::new();
    // Map covering global idx -> (group idx, path)
    let mut covering_to_group: HashMap<usize, usize> = HashMap::new();
    {
        let mut global = 0usize;
        for (gi, g) in groups.iter().enumerate() {
            if !*group_has_entries.get(gi).unwrap_or(&false) {
                // Already counted.
                global += g.covering.len();
                continue;
            }
            // If any owner source-recent, skip group without sizing.
            // Already counted per workspace above; do not double-count here.
            if g.owners
                .iter()
                .any(|o| *ws_source_recent.get(o).unwrap_or(&false))
            {
                global += g.covering.len();
                continue;
            }
            if g.owners.iter().any(|o| ws_source_uncertain.contains(o)) {
                counters.uncertain_skipped += 1;
                global += g.covering.len();
                continue;
            }
            if g.ownership == OutputOwnershipClass::Uncertain {
                counters.uncertain_skipped += 1;
                global += g.covering.len();
                continue;
            }
            for c in &g.covering {
                survivor_covering.push((global, c.clone()));
                survivor_group_idx.push(gi);
                covering_to_group.insert(global, gi);
                global += 1;
            }
        }
        // Adjust: survivor_covering indices must match all_covering? Simpler:
        // measure survivor_covering directly with fresh indices.
    }
    // Measure survivors with one bounded pool.
    let measure_targets: Vec<(usize, PathBuf)> = survivor_covering
        .iter()
        .enumerate()
        .map(|(i, (_, p))| (i, p.clone()))
        .collect();
    let measured = if measure_targets.is_empty() {
        Vec::new()
    } else {
        traverse::measure_many_targets(&measure_targets)
    };
    let mut bytes_by_survivor: HashMap<usize, traverse::TargetStats> = HashMap::new();
    for (i, stats) in measured {
        bytes_by_survivor.insert(i, stats);
    }
    // Aggregate per group.
    let mut eligible = Vec::new();
    // Map group idx -> list of survivor positions.
    let mut group_to_survivors: HashMap<usize, Vec<usize>> = HashMap::new();
    for (pos, gi) in survivor_group_idx.iter().enumerate() {
        group_to_survivors.entry(*gi).or_default().push(pos);
    }
    for (gi, g) in groups.iter().enumerate() {
        let Some(survivors) = group_to_survivors.get(&gi) else {
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
            continue;
        }
        if entries == 0 {
            counters.empty_no_output_skipped += 1;
            observer.empty_skipped();
            continue;
        }
        // Output activity gate: recent output protects group.
        let is_recent = newest.is_some_and(|t| t >= clock_cutoff || t > clock_start);
        if is_recent {
            counters.active_skipped += 1;
            observer.active_skipped();
            continue;
        }
        // Survivor.
        counters.groups_measured += 1;
        counters.bytes_measured = counters.bytes_measured.saturating_add(bytes);
        observer.group_measured(bytes);
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
        eligible.push(physical);
    }
    // Deterministic size-descending with stable path tie-break.
    eligible.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.display_path.cmp(&b.display_path))
    });
    eligible
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
        assert_eq!(counters.deduped_workspace_hits, 1);
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
            output: OutputSet { target, build },
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
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
            } else if let Ok(d) = std::fs::File::open(&current) {
                let _ = d.set_modified(old);
            }
        }
        if let Ok(d) = std::fs::File::open(path) {
            let _ = d.set_modified(old);
        }
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
        std::fs::create_dir_all(proj.join(".cargo")).unwrap();
        std::fs::write(
            proj.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = \"{}\"\n", external.display()),
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
}
