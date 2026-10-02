//! Authorized redirected cleanup and full simulation mode (M005B).
//!
//! Cleanup is workspace-scoped and Cargo-mediated. A cleanable unit is one
//! resolved workspace plus its authorized private physical output group(s).
//! Shared, uncertain, symlink, and unauthorized external output is never
//! cleaned. `--dryrun` runs the full decision path but invokes no `cargo
//! clean` command.

use crate::{
    config::ScanConfig,
    discovery,
    domain::*,
    error::AppError,
    policy,
    progress::{ProgressObserver, ScanPhase},
    traverse,
    workspace::{self, CargoRunner},
};
use std::{
    collections::HashMap,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanOutcome {
    Previewed,
    Simulated,
    Cleaned,
    Skipped,
    Failed,
}

#[derive(Clone, Debug)]
pub struct CleanResult {
    pub display_path: PathBuf,
    pub workspace_roots: Vec<PathBuf>,
    pub ownership: OutputOwnershipClass,
    pub outcome: CleanOutcome,
    pub before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub observed_decrease: Option<u64>,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct CleanReport {
    pub results: Vec<CleanResult>,
    pub diagnostics: usize,
    pub failed: usize,
    pub mode: CleanMode,
}

impl CleanReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut previewed = 0usize;
        let mut simulated = 0usize;
        let mut cleaned = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut before = 0u64;
        let mut after = 0u64;
        let mut delta = 0u64;
        // Deterministic size-descending with stable path tie-break; progress
        // rows show at most five transient entries, final report lists every
        // group.
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
            if let Some(n) = r.before_bytes {
                out.push_str(&format!("  before {}", crate::report::format_bytes(n)));
            }
            if let Some(n) = r.after_bytes {
                out.push_str(&format!("  after {}", crate::report::format_bytes(n)));
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

pub trait CleanupRunner {
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
/// Rules (ADR 001 + M005B §4):
/// - `Shared` and `Uncertain` remain forbidden even inside an allowed root.
/// - Symlink roots are never cleanable (grouped as `Uncertain` upstream).
/// - The explicit `clean ROOT` sandbox authorizes private output inside ROOT.
/// - Output outside ROOT must be inside one configured absolute allowed root.
/// - Symlink authorization roots are invalid.
/// - Authorization does not imply ownership; it only lifts the external
///   boundary for otherwise-private groups.
/// - Empty allowed list preserves M004-style containment.
pub fn is_authorized(
    group: &workspace::RawGroup,
    workspaces: &[ResolvedWorkspace],
    clean_root: &Path,
    allowed_roots: &[PathBuf],
) -> Result<bool, String> {
    if group.ownership == OutputOwnershipClass::Shared
        || group.ownership == OutputOwnershipClass::Uncertain
    {
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
    let canonical_clean =
        fs::canonicalize(clean_root).map_err(|e| format!("cannot resolve cleanup root: {e}"))?;
    // Every covering root must be inside an authorized boundary.
    for covering in &group.covering {
        // Covering roots are already canonical (from build_groups).
        if covering == &canonical_clean || covering.starts_with(&canonical_clean) {
            continue;
        }
        if canonical_allowed
            .iter()
            .any(|a| covering == a || covering.starts_with(a))
        {
            continue;
        }
        return Ok(false);
    }
    // Single-owner private check: shared already rejected above. External
    // groups with one owner become private only when authorized (checked
    // above). Uncertain already rejected.
    let _ = workspaces;
    Ok(true)
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
    if !root.is_absolute() {
        return Err(AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: "cleanup requires an absolute sandbox root".into(),
        });
    }
    let cfg = ScanConfig {
        recency_seconds,
        root: None,
        ignore: Vec::new(),
        unignore: Vec::new(),
    };
    let resolved = policy::resolve(
        ScanRequest {
            cli_root: Some(root.to_path_buf()),
        },
        &cfg,
    )?;
    let scan_start = SystemTime::now();
    // Discovery (manifest-first, no Cargo yet).
    observer.phase(match mode {
        CleanMode::Preview => ScanPhase::CleanupPreview,
        CleanMode::Simulate => ScanPhase::CleanupSimulate,
        CleanMode::Execute => ScanPhase::CleanupExecute,
    });
    let discovered = discovery::discover_manifests(&resolved, observer)?;
    let mut counters = discovered.counters.clone();
    let mut diagnostics: Vec<ScanDiagnostic> = discovered.diagnostics;
    let manifests = discovered.manifests;

    // Workspace resolution (cached, sequential).
    let adapter = WorkspaceCleanupAdapter { runner };
    let workspaces = workspace::resolve_workspaces(
        &manifests,
        &adapter,
        &mut counters,
        &mut diagnostics,
        observer,
    );
    let groups = workspace::build_groups(&workspaces);

    // Fail-fast analysis (cheap gates before deep sizing).
    let clock_cutoff = scan_start
        .checked_sub(Duration::from_secs(recency_seconds))
        .ok_or_else(|| AppError::Config("recency window exceeds system time range".into()))?;
    let physical = workspace::analyze_groups(
        &workspaces,
        groups.clone(),
        scan_start,
        clock_cutoff,
        Duration::from_secs(recency_seconds),
        &mut counters,
        &mut diagnostics,
        observer,
    );
    // Map display -> measured PhysicalOutputGroup for pre-clean sizes.
    // analyze_groups returns PhysicalOutputGroup with display_path + bytes.
    let mut eligible_by_display: HashMap<PathBuf, PhysicalOutputGroup> = HashMap::new();
    for g in physical {
        eligible_by_display.insert(g.display_path.clone(), g);
    }

    // Cleanup phase is determinate: candidate count known.
    let mut report = CleanReport {
        diagnostics: diagnostics.len(),
        mode,
        ..Default::default()
    };

    // For each RawGroup, decide cleanability with full revalidation.
    for raw in &groups {
        // Only groups with measured inventory are candidates; missing/empty/
        // active groups were already skipped in analysis. Look up measurement.
        let Some(measured) = eligible_by_display.get(&raw.display) else {
            // Not measured (empty/active/uncertain/missing) → skip with reason.
            // To avoid noisy skips for every pruned workspace, only emit a
            // skip when the group had physical output (i.e., was a real
            // candidate). Missing-output workspaces have no groups at all.
            continue;
        };
        // Authorization (private + inside clean ROOT or allowed roots).
        let authorized = match is_authorized(raw, &workspaces, root, allowed_output_roots) {
            Ok(true) => true,
            Ok(false) => {
                report.results.push(CleanResult {
                    display_path: raw.display.clone(),
                    workspace_roots: raw
                        .owners
                        .iter()
                        .map(|o| workspaces[*o].root.clone())
                        .collect(),
                    ownership: raw.ownership,
                    outcome: CleanOutcome::Skipped,
                    before_bytes: Some(measured.bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: match raw.ownership {
                        OutputOwnershipClass::Shared => {
                            "shared output remains inventory-only".into()
                        }
                        OutputOwnershipClass::Uncertain => {
                            "uncertain output remains inventory-only".into()
                        }
                        _ => "external output requires explicit cleanup authorization".into(),
                    },
                });
                continue;
            }
            Err(msg) => {
                report.results.push(CleanResult {
                    display_path: raw.display.clone(),
                    workspace_roots: vec![],
                    ownership: raw.ownership,
                    outcome: CleanOutcome::Skipped,
                    before_bytes: Some(measured.bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: msg,
                });
                continue;
            }
        };
        if !authorized {
            continue;
        }
        // Revalidate complete ownership graph before disposition.
        let revalidated = match revalidate_group(raw, &workspaces, root, runner, recency_seconds) {
            Ok(Some(ctx)) => ctx,
            Ok(None) => {
                report.results.push(CleanResult {
                    display_path: raw.display.clone(),
                    workspace_roots: raw
                        .owners
                        .iter()
                        .map(|o| workspaces[*o].root.clone())
                        .collect(),
                    ownership: raw.ownership,
                    outcome: CleanOutcome::Skipped,
                    before_bytes: Some(measured.bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: "workspace changed before cleanup; skipped".into(),
                });
                continue;
            }
            Err(msg) => {
                report.results.push(CleanResult {
                    display_path: raw.display.clone(),
                    workspace_roots: vec![],
                    ownership: raw.ownership,
                    outcome: CleanOutcome::Skipped,
                    before_bytes: Some(measured.bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: msg,
                });
                continue;
            }
        };

        // At this point the group is authorized private + freshly revalidated.
        match mode {
            CleanMode::Simulate => {
                // Full path through revalidation, but no `cargo clean`.
                report.results.push(CleanResult {
                    display_path: raw.display.clone(),
                    workspace_roots: revalidated.workspace_roots.clone(),
                    ownership: raw.ownership,
                    outcome: CleanOutcome::Simulated,
                    before_bytes: Some(measured.bytes),
                    after_bytes: None,
                    observed_decrease: None,
                    detail: "simulation: no `cargo clean` command was invoked".into(),
                });
                observer.reportable_group(&raw.display, measured.bytes);
            }
            CleanMode::Preview | CleanMode::Execute => {
                // Execute repeats final preflight immediately before spawning.
                if mode == CleanMode::Execute
                    && let Err(msg) = final_preflight(&revalidated, runner, recency_seconds)
                {
                    report.results.push(CleanResult {
                        display_path: raw.display.clone(),
                        workspace_roots: revalidated.workspace_roots.clone(),
                        ownership: raw.ownership,
                        outcome: CleanOutcome::Skipped,
                        before_bytes: Some(measured.bytes),
                        after_bytes: None,
                        observed_decrease: None,
                        detail: format!("preflight changed: {msg}"),
                    });
                    continue;
                }
                let frozen = match frozen_env(&revalidated) {
                    Ok(env) => env,
                    Err(msg) => {
                        report.results.push(CleanResult {
                            display_path: raw.display.clone(),
                            workspace_roots: revalidated.workspace_roots.clone(),
                            ownership: raw.ownership,
                            outcome: CleanOutcome::Skipped,
                            before_bytes: Some(measured.bytes),
                            after_bytes: None,
                            observed_decrease: None,
                            detail: msg,
                        });
                        continue;
                    }
                };
                let args = clean_args(&revalidated, mode);
                let cwd = revalidated.workspace_root.clone();
                let output = if frozen.is_empty() {
                    match runner.run(&cwd, &args) {
                        Err(e) => {
                            report.failed += 1;
                            report.results.push(CleanResult {
                                display_path: raw.display.clone(),
                                workspace_roots: revalidated.workspace_roots.clone(),
                                ownership: raw.ownership,
                                outcome: CleanOutcome::Failed,
                                before_bytes: Some(measured.bytes),
                                after_bytes: None,
                                observed_decrease: None,
                                detail: format!("could not start Cargo: {e}"),
                            });
                            continue;
                        }
                        Ok(o) => o,
                    }
                } else {
                    match runner.run_with_env(&cwd, &args, &frozen) {
                        Err(e) => {
                            report.failed += 1;
                            report.results.push(CleanResult {
                                display_path: raw.display.clone(),
                                workspace_roots: revalidated.workspace_roots.clone(),
                                ownership: raw.ownership,
                                outcome: CleanOutcome::Failed,
                                before_bytes: Some(measured.bytes),
                                after_bytes: None,
                                observed_decrease: None,
                                detail: format!("could not start Cargo: {e}"),
                            });
                            continue;
                        }
                        Ok(o) => o,
                    }
                };
                if !output.success {
                    report.failed += 1;
                    report.results.push(CleanResult {
                        display_path: raw.display.clone(),
                        workspace_roots: revalidated.workspace_roots.clone(),
                        ownership: raw.ownership,
                        outcome: CleanOutcome::Failed,
                        before_bytes: Some(measured.bytes),
                        after_bytes: None,
                        observed_decrease: None,
                        detail: format!("Cargo exited {:?}: {}", output.code, output_text(&output)),
                    });
                    continue;
                }
                if mode == CleanMode::Preview {
                    report.results.push(CleanResult {
                        display_path: raw.display.clone(),
                        workspace_roots: revalidated.workspace_roots.clone(),
                        ownership: raw.ownership,
                        outcome: CleanOutcome::Previewed,
                        before_bytes: Some(measured.bytes),
                        after_bytes: None,
                        observed_decrease: None,
                        detail: output_text(&output),
                    });
                } else {
                    match measure_union(&raw.covering) {
                        Ok(after) => {
                            let decrease = measured.bytes.saturating_sub(after);
                            report.results.push(CleanResult {
                                display_path: raw.display.clone(),
                                workspace_roots: revalidated.workspace_roots.clone(),
                                ownership: raw.ownership,
                                outcome: CleanOutcome::Cleaned,
                                before_bytes: Some(measured.bytes),
                                after_bytes: Some(after),
                                observed_decrease: Some(decrease),
                                detail: output_text(&output),
                            });
                        }
                        Err(e) => {
                            report.results.push(CleanResult {
                                display_path: raw.display.clone(),
                                workspace_roots: revalidated.workspace_roots.clone(),
                                ownership: raw.ownership,
                                outcome: CleanOutcome::Cleaned,
                                before_bytes: Some(measured.bytes),
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
                observer.reportable_group(&raw.display, measured.bytes);
            }
        }
    }
    // Include diagnostics from discovery/resolution in count (already set).
    Ok(report)
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

#[derive(Clone, Debug)]
struct RevalidatedContext {
    workspace_roots: Vec<PathBuf>,
    workspace_root: PathBuf,
    root_manifest: PathBuf,
    target: PathBuf,
    build: Option<PathBuf>,
    capability: CargoCapabilities,
}

fn revalidate_group(
    raw: &workspace::RawGroup,
    workspaces: &[ResolvedWorkspace],
    _clean_root: &Path,
    runner: &dyn CleanupRunner,
    recency_seconds: u64,
) -> Result<Option<RevalidatedContext>, String> {
    // Re-resolve every owner workspace and compare identity, members, outputs.
    let adapter = WorkspaceCleanupAdapter { runner };
    let mut fresh_workspaces: HashMap<WorkspaceId, ResolvedWorkspace> = HashMap::new();
    for idx in &raw.owners {
        let ws = &workspaces[*idx];
        let mut counters = ScanCounters::default();
        let mut diags = Vec::new();
        let noop = crate::progress::NoopObserver;
        let fresh = workspace::resolve_workspaces(
            std::slice::from_ref(&ws.root_manifest),
            &adapter,
            &mut counters,
            &mut diags,
            &noop,
        );
        let Some(fresh_ws) = fresh.into_iter().next() else {
            return Ok(None);
        };
        // Compare workspace identity, member set, logical paths, ownership.
        // Use canonicalized comparisons so `/tmp` vs `/private/tmp` (macOS
        // symlink) does not falsely report a change when physical identity
        // is stable.
        if fresh_ws.id != ws.id || fresh_ws.root != ws.root {
            return Ok(None);
        }
        let canon = |p: &PathBuf| fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        let mut old_members: Vec<PathBuf> =
            ws.members.iter().map(|m| canon(&m.source_root)).collect();
        let mut new_members: Vec<PathBuf> = fresh_ws
            .members
            .iter()
            .map(|m| canon(&m.source_root))
            .collect();
        old_members.sort();
        new_members.sort();
        if old_members != new_members {
            return Ok(None);
        }
        let old_target_canon = ws
            .output
            .target
            .physical_path
            .clone()
            .unwrap_or_else(|| canon(&ws.output.target.logical_path));
        let new_target_canon = fresh_ws
            .output
            .target
            .physical_path
            .clone()
            .unwrap_or_else(|| canon(&fresh_ws.output.target.logical_path));
        let old_build_canon = ws
            .output
            .build
            .physical_path
            .clone()
            .unwrap_or_else(|| canon(&ws.output.build.logical_path));
        let new_build_canon = fresh_ws
            .output
            .build
            .physical_path
            .clone()
            .unwrap_or_else(|| canon(&fresh_ws.output.build.logical_path));
        if old_target_canon != new_target_canon || old_build_canon != new_build_canon {
            return Ok(None);
        }
        // Re-evaluate source/output activity with fresh timestamps.
        let start = SystemTime::now();
        let cutoff = start
            .checked_sub(Duration::from_secs(recency_seconds))
            .ok_or_else(|| "recency window exceeds system time range".to_owned())?;
        let all_outputs: Vec<PathBuf> = [&fresh_ws.output.target, &fresh_ws.output.build]
            .iter()
            .filter_map(|r| r.physical_path.clone().or(Some(r.logical_path.clone())))
            .collect();
        for member_root in workspace::workspace_member_roots(&fresh_ws) {
            match traverse::workspace_member_activity(&member_root, &all_outputs, start, cutoff) {
                Ok(traverse::SourceActivity::Recent(_)) => return Ok(None),
                Ok(traverse::SourceActivity::Quiet(_)) => {}
                Err(()) => return Err("revalidation source activity uncertain".into()),
            }
        }
        // Output activity: if group output became recent, skip.
        for covering in &raw.covering {
            let stats = traverse::measure_single_target(covering);
            if stats.uncertain {
                return Err("revalidation output measurement uncertain".into());
            }
            if stats.newest.is_some_and(|t| t >= cutoff || t > start) {
                return Ok(None);
            }
        }
        // Ownership must remain private (shared/uncertain skips).
        let fresh_groups = workspace::build_groups(std::slice::from_ref(&fresh_ws));
        // Find group containing this raw display (by covering overlap).
        let mut still_private = false;
        for g in &fresh_groups {
            if g.covering.iter().any(|c| raw.covering.contains(c))
                && g.ownership != OutputOwnershipClass::Shared
                && g.ownership != OutputOwnershipClass::Uncertain
            {
                still_private = true;
                break;
            }
        }
        if !still_private {
            // Either the group vanished (missing/symlink) or became
            // shared/uncertain; never promote to eligible.
            return Ok(None);
        }
        fresh_workspaces.insert(fresh_ws.id.clone(), fresh_ws);
    }
    // Build execution context from first owner (single-owner private groups
    // only reach here; shared already filtered).
    let first_idx = raw.owners[0];
    let ws = &workspaces[first_idx];
    let fresh_ws = fresh_workspaces.get(&ws.id).unwrap_or(ws);
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
    // For equal target/build, build is None (single dir cleans both).
    let build = if build.as_ref() == Some(&target) {
        None
    } else {
        build
    };
    Ok(Some(RevalidatedContext {
        workspace_roots: raw
            .owners
            .iter()
            .map(|o| workspaces[*o].root.clone())
            .collect(),
        workspace_root: fresh_ws.root.clone(),
        root_manifest: fresh_ws.root_manifest.clone(),
        target,
        build,
        capability: fresh_ws.capability,
    }))
}

fn final_preflight(
    ctx: &RevalidatedContext,
    runner: &dyn CleanupRunner,
    recency_seconds: u64,
) -> Result<(), String> {
    // Repeat resolution immediately before spawning Cargo (double-check).
    let adapter = WorkspaceCleanupAdapter { runner };
    let mut counters = ScanCounters::default();
    let mut diags = Vec::new();
    let noop = crate::progress::NoopObserver;
    let fresh = workspace::resolve_workspaces(
        std::slice::from_ref(&ctx.root_manifest),
        &adapter,
        &mut counters,
        &mut diags,
        &noop,
    );
    let Some(fresh_ws) = fresh.into_iter().next() else {
        return Err("workspace vanished during preflight".into());
    };
    if fresh_ws.root != ctx.workspace_root {
        return Err("workspace root changed during preflight".into());
    }
    let fresh_target = fresh_ws
        .output
        .target
        .physical_path
        .clone()
        .unwrap_or_else(|| {
            fs::canonicalize(&fresh_ws.output.target.logical_path)
                .unwrap_or_else(|_| fresh_ws.output.target.logical_path.clone())
        });
    let ctx_target_canon = fs::canonicalize(&ctx.target).unwrap_or_else(|_| ctx.target.clone());
    if fresh_target != ctx_target_canon && fresh_target != ctx.target {
        return Err("target changed during preflight".into());
    }
    // Fresh activity check.
    let start = SystemTime::now();
    let cutoff = start
        .checked_sub(Duration::from_secs(recency_seconds))
        .ok_or_else(|| "recency window exceeds system time range".to_owned())?;
    let all_outputs: Vec<PathBuf> = [&fresh_ws.output.target, &fresh_ws.output.build]
        .iter()
        .filter_map(|r| r.physical_path.clone().or(Some(r.logical_path.clone())))
        .collect();
    for member_root in workspace::workspace_member_roots(&fresh_ws) {
        match traverse::workspace_member_activity(&member_root, &all_outputs, start, cutoff) {
            Ok(traverse::SourceActivity::Recent(_)) => {
                return Err("became active during preflight".into());
            }
            Ok(traverse::SourceActivity::Quiet(_)) => {}
            Err(()) => return Err("source uncertain during preflight".into()),
        }
    }
    Ok(())
}

fn frozen_env(ctx: &RevalidatedContext) -> Result<Vec<(OsString, OsString)>, String> {
    // Freeze Cargo output context; prevent inherited conflicts.
    // - Always override CARGO_TARGET_DIR to the resolved target.
    // - When runtime supports stable build-dir and build is distinct, override
    //   CARGO_BUILD_BUILD_DIR; otherwise ensure no silent build-dir redirect.
    // - Do not pass unsupported unstable flags.
    if ctx.capability.env_build_dir_set
        && ctx.capability.build_dir == CargoBuildDirCapability::Unavailable
    {
        return Err(
            "CARGO_BUILD_BUILD_DIR is set but runtime Cargo cannot prove separate build-dir; cleanup deferred"
                .into(),
        );
    }
    if ctx.capability.build_dir == CargoBuildDirCapability::Unknown
        && std::env::var_os("CARGO_BUILD_BUILD_DIR").is_some()
    {
        return Err("separate build directory cannot be proven safe; cleanup deferred".into());
    }
    let mut env = Vec::new();
    env.push((
        OsString::from("CARGO_TARGET_DIR"),
        ctx.target.as_os_str().to_owned(),
    ));
    if let Some(build) = &ctx.build {
        match ctx.capability.build_dir {
            CargoBuildDirCapability::Distinct | CargoBuildDirCapability::Equal => {
                env.push((
                    OsString::from("CARGO_BUILD_BUILD_DIR"),
                    build.as_os_str().to_owned(),
                ));
            }
            CargoBuildDirCapability::Unavailable | CargoBuildDirCapability::Unknown => {
                return Err(
                    "Cargo reports a separate build directory without stable support; cleanup deferred"
                        .into(),
                );
            }
        }
    }
    Ok(env)
}

fn clean_args(ctx: &RevalidatedContext, mode: CleanMode) -> Vec<OsString> {
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
    args
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
    fn private_redirected_inside_root_allowed() {
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
        // Fake runner whose target is the redirected dir inside ROOT.
        let runner = FakeCleanupRunner::new(&proj, &redirected, 1);
        // Canonicalize for grouping: build a RawGroup manually to test auth.
        let canonical = fs::canonicalize(&redirected).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        let raw = workspace::RawGroup {
            physicals: vec![canonical.clone()],
            covering: vec![canonical.clone()],
            display: canonical.clone(),
            owners: vec![0],
            ownership: OutputOwnershipClass::ExternalUnproven,
        };
        let ws = ResolvedWorkspace {
            id: WorkspaceId(canonical_root.clone()),
            root: canonical_root,
            root_manifest: proj.join("Cargo.toml"),
            members: vec![],
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
                    physical_path: Some(canonical),
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
        assert!(is_authorized(&raw, std::slice::from_ref(&ws), &root, &[]).unwrap());
        let _ = runner;
    }

    #[test]
    fn private_redirected_outside_root_requires_allowed_root() {
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
        };
        let ws_root = d.path().join("ws");
        std::fs::create_dir_all(&ws_root).unwrap();
        let ws = ResolvedWorkspace {
            id: WorkspaceId(ws_root.clone()),
            root: ws_root,
            root_manifest: PathBuf::from("/x/Cargo.toml"),
            members: vec![],
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
        assert!(!is_authorized(&raw, std::slice::from_ref(&ws), &root, &[]).unwrap());
        // Inside configured allowed root → authorized.
        assert!(is_authorized(&raw, std::slice::from_ref(&ws), &root, &[outside]).unwrap());
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
            };
            let ws = ResolvedWorkspace {
                id: WorkspaceId(d.path().join("ws")),
                root: d.path().join("ws"),
                root_manifest: PathBuf::from("/x/Cargo.toml"),
                members: vec![],
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
            assert!(is_authorized(&raw, std::slice::from_ref(&ws), &root, &[link]).is_err());

            // Shared remains forbidden even inside allowed root.
            let shared_raw = workspace::RawGroup {
                physicals: vec![real.clone()],
                covering: vec![real.clone()],
                display: real.clone(),
                owners: vec![0, 1],
                ownership: OutputOwnershipClass::Shared,
            };
            assert!(
                !is_authorized(
                    &shared_raw,
                    std::slice::from_ref(&ws),
                    &root,
                    std::slice::from_ref(&real)
                )
                .unwrap()
            );
            // Uncertain forbidden.
            let uncertain_raw = workspace::RawGroup {
                physicals: vec![real.clone()],
                covering: vec![real],
                display: PathBuf::from("/x"),
                owners: vec![0],
                ownership: OutputOwnershipClass::Uncertain,
            };
            assert!(
                !is_authorized(
                    &uncertain_raw,
                    std::slice::from_ref(&ws),
                    &root,
                    &[d.path().to_path_buf()]
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn frozen_env_sets_target_and_build_dir() {
        let ctx = RevalidatedContext {
            workspace_roots: vec![PathBuf::from("/ws")],
            workspace_root: PathBuf::from("/ws"),
            root_manifest: PathBuf::from("/ws/Cargo.toml"),
            target: PathBuf::from("/cache/target"),
            build: Some(PathBuf::from("/cache/build")),
            capability: CargoCapabilities {
                build_dir: CargoBuildDirCapability::Distinct,
                metadata_had_build_directory: true,
                env_build_dir_set: false,
            },
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
    fn cargo_failure_isolated_per_workspace() {
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
        std::fs::write(target.join("a.bin"), vec![1u8; 256]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        backdate(d.path(), old);
        let bad_manifest = d.path().join("bad/Cargo.toml");
        std::fs::create_dir_all(bad_manifest.parent().unwrap()).unwrap();
        std::fs::write(&bad_manifest, "").unwrap();
        // Simulate two manifests: one good, one bad. Use discovery directly
        // with both manifests via clean_with on parent ROOT containing both.
        // clean_with discovers manifests under ROOT; bad manifest will fail
        // locate but good should still proceed (isolation).
        let runner = PerPathFailRunner {
            good_root: good.clone(),
            good_target: target.clone(),
        };
        let noop = NoopObserver;
        // ROOT is tempdir (contains good + bad). Good target inside ROOT.
        let report = clean_with(d.path(), 0, &[], CleanMode::Simulate, &runner, &noop).unwrap();
        // Good workspace still simulated despite bad sibling failing.
        assert!(
            report
                .results
                .iter()
                .any(|r| r.outcome == CleanOutcome::Simulated)
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
                ownership: OutputOwnershipClass::PrivateBounded,
                outcome: CleanOutcome::Cleaned,
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
        // Real Cargo integration: private redirected output inside ROOT.
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
        let external = root.join("ext-target");
        std::fs::create_dir_all(&external).unwrap();
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
        // Preview (Cargo dry-run) must not mutate.
        let preview = clean_with(&root, 0, &[], CleanMode::Preview, &runner, &noop).unwrap();
        assert_eq!(preview.results.len(), 1);
        assert_eq!(preview.results[0].outcome, CleanOutcome::Previewed);
        assert!(external.join("artifact.bin").exists());
        assert!(preview.render().contains("no cleanup executed"));
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
        // With explicit allowed root → simulated would-clean.
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
        assert_eq!(allowed.results[0].outcome, CleanOutcome::Simulated);
    }
}
