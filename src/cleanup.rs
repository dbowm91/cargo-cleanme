use crate::{
    analysis::{self, ScanClock},
    config::ScanConfig,
    discovery,
    domain::*,
    error::AppError,
    policy,
};
use serde::Deserialize;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CleanMode {
    #[default]
    DryRun,
    Execute,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanOutcome {
    Previewed,
    Cleaned,
    Skipped,
    Failed,
}
#[derive(Clone, Debug)]
pub struct CleanResult {
    pub project_root: PathBuf,
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
        let mut cleaned = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut before = 0u64;
        let mut after = 0u64;
        let mut delta = 0u64;
        for r in &self.results {
            match r.outcome {
                CleanOutcome::Previewed => previewed += 1,
                CleanOutcome::Cleaned => cleaned += 1,
                CleanOutcome::Skipped => skipped += 1,
                CleanOutcome::Failed => failed += 1,
            }
            if let Some(n) = r.before_bytes {
                before = before.saturating_add(n);
            }
            if let Some(n) = r.after_bytes {
                after = after.saturating_add(n);
            }
            if let Some(n) = r.observed_decrease {
                delta = delta.saturating_add(n);
            }
            out.push_str(&format!(
                "{:?}  {}",
                r.outcome,
                escaped_path(&r.project_root)
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
        if self.mode == CleanMode::DryRun {
            out.push_str(&format!(
                "\ndry-run: {previewed} previewed, {skipped} skipped, {failed} failed; pre-clean estimate {}; no cleanup executed; {} filesystem diagnostics\n",
                crate::report::format_bytes(before),
                self.diagnostics
            ));
        } else {
            out.push_str(&format!("\ncleanup: {cleaned} cleaned, {skipped} skipped, {failed} failed; pre-clean estimate {}, post-clean measured {}, observed decrease {}; {} filesystem diagnostics\n",crate::report::format_bytes(before),crate::report::format_bytes(after),crate::report::format_bytes(delta),self.diagnostics));
        }
        out
    }
}
fn escaped_path(path: &Path) -> String {
    path.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{:?}", path.as_os_str()))
}
#[derive(Debug)]
pub struct ProcessOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub trait ProcessRunner {
    fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput>;
}
pub struct SystemRunner;
impl ProcessRunner for SystemRunner {
    fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
        let output = Command::new("cargo").args(args).current_dir(cwd).output()?;
        Ok(ProcessOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
    workspace_root: String,
    target_directory: String,
    #[serde(default)]
    build_directory: Option<String>,
}
#[derive(Debug, Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    manifest_path: String,
}
#[derive(Clone, Debug)]
pub struct Ownership {
    pub package_id: String,
    pub package_name: String,
    pub manifest: PathBuf,
    pub target: PathBuf,
}

pub fn clean(root: &Path, recency_seconds: u64, mode: CleanMode) -> Result<CleanReport, AppError> {
    clean_with(root, recency_seconds, mode, &SystemRunner)
}
pub fn clean_with(
    root: &Path,
    recency_seconds: u64,
    mode: CleanMode,
    runner: &dyn ProcessRunner,
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
    let start = SystemTime::now();
    let mut report = discovery::scan(&resolved)?;
    let clock = ScanClock::new(start, Duration::from_secs(recency_seconds))
        .ok_or_else(|| AppError::Config("recency window exceeds system time range".into()))?;
    let candidates = std::mem::take(&mut report.eligible);
    let mut clean_report = CleanReport {
        diagnostics: report.diagnostics.len(),
        mode,
        ..Default::default()
    };
    for item in candidates {
        let project = DiscoveredProject {
            project_root: item.project_root.clone(),
            manifest_path: item.project_root.join("Cargo.toml"),
            target_path: item.artifact.target_path,
        };
        match analysis::analyze(&project, &clock) {
            Ok(Some(_eligible)) => {}
            Ok(None) => continue,
            Err(diag) => {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Skipped,
                    None,
                    None,
                    None,
                    diag.message,
                ));
                continue;
            }
        }
        let fresh = match revalidate(&project, recency_seconds) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Skipped,
                    None,
                    None,
                    None,
                    "project became active or changed before cleanup".into(),
                ));
                continue;
            }
            Err(message) => {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Skipped,
                    None,
                    None,
                    None,
                    message,
                ));
                continue;
            }
        };
        let ownership = match inspect_ownership(&fresh, runner) {
            Ok(o) => o,
            Err(message) => {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Skipped,
                    Some(fresh.artifact.bytes),
                    None,
                    None,
                    message,
                ));
                continue;
            }
        };
        if mode == CleanMode::Execute {
            let execution_candidate = match revalidate(&project, recency_seconds) {
                Ok(Some(candidate)) => candidate,
                Ok(None) => {
                    clean_report.results.push(result(
                        &item.project_root,
                        CleanOutcome::Skipped,
                        Some(fresh.artifact.bytes),
                        None,
                        None,
                        "project became active during cleanup preflight".into(),
                    ));
                    continue;
                }
                Err(message) => {
                    clean_report.results.push(result(
                        &item.project_root,
                        CleanOutcome::Skipped,
                        Some(fresh.artifact.bytes),
                        None,
                        None,
                        message,
                    ));
                    continue;
                }
            };
            let execution_ownership = match inspect_ownership(&execution_candidate, runner) {
                Ok(ownership) => ownership,
                Err(message) => {
                    clean_report.results.push(result(
                        &item.project_root,
                        CleanOutcome::Skipped,
                        Some(execution_candidate.artifact.bytes),
                        None,
                        None,
                        format!("ownership changed during cleanup preflight: {message}"),
                    ));
                    continue;
                }
            };
            if execution_ownership.package_id != ownership.package_id
                || execution_ownership.manifest != ownership.manifest
                || execution_ownership.target != ownership.target
            {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Skipped,
                    Some(execution_candidate.artifact.bytes),
                    None,
                    None,
                    "Cargo ownership changed during cleanup preflight".into(),
                ));
                continue;
            }
        }
        let args = clean_args(&ownership, mode);
        match runner.run(
            ownership.manifest.parent().unwrap_or(&item.project_root),
            &args,
        ) {
            Err(e) => {
                clean_report.failed += 1;
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Failed,
                    Some(fresh.artifact.bytes),
                    None,
                    None,
                    format!("could not start Cargo: {e}"),
                ));
            }
            Ok(output) if !output.success => {
                clean_report.failed += 1;
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Failed,
                    Some(fresh.artifact.bytes),
                    None,
                    None,
                    format!("Cargo exited {:?}: {}", output.code, output_text(&output)),
                ));
            }
            Ok(output) if mode == CleanMode::DryRun => {
                clean_report.results.push(result(
                    &item.project_root,
                    CleanOutcome::Previewed,
                    Some(fresh.artifact.bytes),
                    None,
                    None,
                    output_text(&output),
                ));
            }
            Ok(output) => match measure_target(&ownership.target) {
                Ok(after) => {
                    let decrease = fresh.artifact.bytes.saturating_sub(after);
                    clean_report.results.push(result(
                        &item.project_root,
                        CleanOutcome::Cleaned,
                        Some(fresh.artifact.bytes),
                        Some(after),
                        Some(decrease),
                        output_text(&output),
                    ));
                }
                Err(e) => {
                    clean_report.results.push(result(
                        &item.project_root,
                        CleanOutcome::Cleaned,
                        Some(fresh.artifact.bytes),
                        None,
                        None,
                        format!("Cargo succeeded; post-clean measurement failed: {e}"),
                    ));
                    clean_report.diagnostics += 1;
                }
            },
        }
    }
    Ok(clean_report)
}
fn result(
    root: &Path,
    outcome: CleanOutcome,
    before_bytes: Option<u64>,
    after_bytes: Option<u64>,
    observed_decrease: Option<u64>,
    detail: String,
) -> CleanResult {
    CleanResult {
        project_root: root.to_path_buf(),
        outcome,
        before_bytes,
        after_bytes,
        observed_decrease,
        detail,
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
fn clean_args(ownership: &Ownership, mode: CleanMode) -> Vec<std::ffi::OsString> {
    let mut args = vec!["clean".into()];
    if mode == CleanMode::DryRun {
        args.push("--dry-run".into());
        args.push("--verbose".into());
    }
    args.push("--offline".into());
    args.push("--locked".into());
    args.push("--manifest-path".into());
    args.push(ownership.manifest.as_os_str().to_owned());
    args.push("--target-dir".into());
    args.push(ownership.target.as_os_str().to_owned());
    args
}
fn revalidate(
    project: &DiscoveredProject,
    recency_seconds: u64,
) -> Result<Option<EligibleProject>, String> {
    let root_meta = fs::symlink_metadata(&project.project_root)
        .map_err(|e| format!("project root unavailable: {e}"))?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err("project root is no longer a real directory".into());
    }
    let manifest = fs::symlink_metadata(&project.manifest_path)
        .map_err(|e| format!("manifest unavailable: {e}"))?;
    if !manifest.is_file() || manifest.file_type().is_symlink() {
        return Err("Cargo.toml is no longer a real file".into());
    }
    let target_meta = fs::symlink_metadata(&project.target_path)
        .map_err(|e| format!("target unavailable: {e}"))?;
    if !target_meta.is_dir()
        || target_meta.file_type().is_symlink()
        || project.target_path != project.project_root.join("target")
    {
        return Err("candidate is no longer a direct real target directory".into());
    }
    let start = SystemTime::now();
    let clock = ScanClock::new(start, Duration::from_secs(recency_seconds))
        .ok_or_else(|| "recency window exceeds system time range".to_owned())?;
    analysis::analyze(project, &clock).map_err(|d| d.message)
}
pub fn inspect_ownership(
    project: &EligibleProject,
    runner: &dyn ProcessRunner,
) -> Result<Ownership, String> {
    let root = &project.project_root;
    let manifest = root.join("Cargo.toml");
    let target = root.join("target");
    for p in [&manifest, &target, root] {
        if p.to_str().is_none() {
            return Err("Cargo metadata cannot verify a non-UTF-8 project path".into());
        }
    }
    if project.artifact.target_path != target {
        return Err("artifact path differs from project/target".into());
    }
    let tag = target.join("CACHEDIR.TAG");
    let tag_meta = fs::symlink_metadata(&tag)
        .map_err(|_| "Cargo CACHEDIR.TAG marker is missing".to_owned())?;
    if !tag_meta.is_file() || tag_meta.file_type().is_symlink() {
        return Err("Cargo CACHEDIR.TAG is not a regular file".into());
    }
    let tag_bytes = fs::read(&tag).map_err(|e| format!("cannot read Cargo cache marker: {e}"))?;
    if !tag_bytes.starts_with(b"Signature: 8a477f597d28d172789f06886806bc55") {
        return Err("target directory does not have Cargo's cache marker signature".into());
    }
    if env::var_os("CARGO_BUILD_BUILD_DIR").is_some() {
        return Err(
            "CARGO_BUILD_BUILD_DIR is set; separate build directories are not supported".into(),
        );
    }
    if has_build_dir_config(root)? {
        return Err(
            "Cargo config declares build.build-dir; separate build directories are not supported"
                .into(),
        );
    }
    let args: Vec<std::ffi::OsString> = [
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
    .chain(std::iter::once(manifest.as_os_str().to_owned()))
    .collect();
    let output = runner
        .run(root, &args)
        .map_err(|e| format!("could not start Cargo metadata: {e}"))?;
    if !output.success {
        return Err(format!("Cargo metadata failed: {}", output_text(&output)));
    }
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cannot parse Cargo metadata: {e}"))?;
    if metadata.workspace_members.len() != 1 {
        return Err("workspace has multiple members; shared target cleanup is deferred".into());
    }
    let workspace_root = canonical_or_absolute(Path::new(&metadata.workspace_root))?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("cannot resolve project root: {e}"))?;
    if workspace_root != canonical_root {
        return Err("Cargo workspace root differs from candidate project root".into());
    }
    let resolved_target = canonical_or_absolute(Path::new(&metadata.target_directory))?;
    let canonical_target =
        fs::canonicalize(&target).map_err(|e| format!("cannot resolve target directory: {e}"))?;
    if resolved_target != canonical_target {
        return Err("Cargo resolves artifacts outside this conventional target directory".into());
    }
    if let Some(build) = metadata.build_directory {
        let resolved = canonical_or_absolute(Path::new(&build))?;
        if resolved != resolved_target {
            return Err("Cargo reports a separate build directory; cleanup is deferred".into());
        }
    }
    let canonical_manifest =
        fs::canonicalize(&manifest).map_err(|e| format!("cannot resolve manifest: {e}"))?;
    let pkg = metadata
        .packages
        .into_iter()
        .find(|p| {
            canonical_or_absolute(Path::new(&p.manifest_path))
                .ok()
                .is_some_and(|m| m == canonical_manifest)
        })
        .ok_or_else(|| "Cargo metadata does not identify the candidate package".to_owned())?;
    let _package_id = &pkg.id;
    Ok(Ownership {
        package_id: pkg.id,
        package_name: pkg.name,
        manifest,
        target: canonical_target,
    })
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
fn has_build_dir_config(root: &Path) -> Result<bool, String> {
    let mut candidates = Vec::new();
    for ancestor in root.ancestors() {
        let cargo = ancestor.join(".cargo");
        candidates.push(cargo.join("config.toml"));
        candidates.push(cargo.join("config"));
    }
    let cargo_home = if let Some(home) = env::var_os("CARGO_HOME") {
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return Err("relative CARGO_HOME prevents verifying global Cargo config".into());
        }
        Some(home)
    } else {
        directories::BaseDirs::new().map(|b| b.home_dir().join(".cargo"))
    };
    if let Some(home) = cargo_home {
        candidates.push(home.join("config.toml"));
        candidates.push(home.join("config"));
    }
    candidates.sort();
    candidates.dedup();
    for path in candidates {
        let content = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!(
                    "cannot inspect Cargo config {}: {e}",
                    path.display()
                ));
            }
        };
        let value: toml::Value = toml::from_str(&content)
            .map_err(|e| format!("cannot inspect Cargo config {}: {e}", path.display()))?;
        if value
            .get("build")
            .and_then(|v| v.get("build-dir"))
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn measure_target(target: &Path) -> io::Result<u64> {
    match fs::symlink_metadata(target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            return Err(io::Error::other("target path changed type"));
        }
        Ok(_) => {}
    }
    let mut bytes = 0u64;
    for entry in WalkDir::new(target).follow_links(false).into_iter() {
        let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
        if entry.path() == target {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.is_file() {
            bytes = bytes
                .checked_add(filesize::file_real_size_fast(entry.path(), &meta)?)
                .ok_or_else(|| io::Error::other("size overflow"))?;
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    struct FakeRunner {
        calls: Mutex<Vec<(PathBuf, Vec<std::ffi::OsString>)>>,
        workspace_members: usize,
        target_override: Option<PathBuf>,
        remove_artifact_on_execute: bool,
    }
    impl FakeRunner {
        fn new(workspace_members: usize, target_override: Option<PathBuf>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                workspace_members,
                target_override,
                remove_artifact_on_execute: false,
            }
        }
        fn removing_artifact() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                workspace_members: 1,
                target_override: None,
                remove_artifact_on_execute: true,
            }
        }
        fn calls(&self) -> Vec<(PathBuf, Vec<std::ffi::OsString>)> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl ProcessRunner for FakeRunner {
        fn run(&self, cwd: &Path, args: &[std::ffi::OsString]) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((cwd.to_path_buf(), args.to_vec()));
            if args.first().is_some_and(|a| a == "metadata") {
                let root = fs::canonicalize(cwd).unwrap();
                let target = self
                    .target_override
                    .clone()
                    .unwrap_or_else(|| root.join("target"));
                let manifest = root.join("Cargo.toml");
                let members = (0..self.workspace_members)
                    .map(|i| format!("fixture {i}"))
                    .collect::<Vec<_>>();
                let packages = vec![
                    serde_json::json!({"id":"fixture 0","name":"fixture","manifest_path":manifest}),
                ];
                let json = serde_json::json!({"packages":packages,"workspace_members":members,"workspace_root":root,"target_directory":target});
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: serde_json::to_vec(&json).unwrap(),
                    stderr: Vec::new(),
                })
            } else {
                if self.remove_artifact_on_execute && !args.iter().any(|arg| arg == "--dry-run") {
                    let target = args
                        .windows(2)
                        .find(|pair| pair[0] == "--target-dir")
                        .map(|pair| PathBuf::from(&pair[1]))
                        .ok_or_else(|| io::Error::other("missing target-dir argument"))?;
                    fs::remove_file(target.join("artifact.bin"))?;
                }
                Ok(ProcessOutput {
                    success: true,
                    code: Some(0),
                    stdout: b"mock cargo clean success".to_vec(),
                    stderr: Vec::new(),
                })
            }
        }
    }
    fn fixture() -> (TempDir, EligibleProject) {
        let d = tempfile::Builder::new()
            .prefix("cargo project ")
            .tempdir()
            .unwrap();
        fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::create_dir(d.path().join("src")).unwrap();
        fs::write(d.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(d.path().join("Cargo.lock"), "version = 4\n").unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(
            target.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        fs::write(target.join("artifact.bin"), vec![7u8; 4096]).unwrap();
        let project = EligibleProject {
            project_root: d.path().to_path_buf(),
            artifact: ArtifactAnalysis {
                target_path: target,
                bytes: 4096,
                metric: SizeMetric::Apparent,
                newest_mtime: None,
                artifact_entries: 2,
            },
        };
        (d, project)
    }
    #[test]
    fn ownership_rejects_missing_cargo_marker() {
        let (_d, mut p) = fixture();
        fs::remove_file(p.artifact.target_path.join("CACHEDIR.TAG")).unwrap();
        let runner = FakeRunner::new(1, None);
        assert!(
            inspect_ownership(&p, &runner)
                .unwrap_err()
                .contains("marker is missing")
        );
        assert!(runner.calls().is_empty());
        p.project_root = PathBuf::from("/still/untouched");
    }
    #[test]
    fn ownership_rejects_shared_workspace_and_redirected_target() {
        let (_d, p) = fixture();
        let shared = FakeRunner::new(2, None);
        assert!(
            inspect_ownership(&p, &shared)
                .unwrap_err()
                .contains("multiple members")
        );
        let other = PathBuf::from("/different/target");
        let redirected = FakeRunner::new(1, Some(other));
        assert!(inspect_ownership(&p, &redirected).is_err());
    }
    #[test]
    fn ownership_rejects_separate_build_directory_config() {
        let (d, p) = fixture();
        let cargo = d.path().join(".cargo");
        fs::create_dir(&cargo).unwrap();
        fs::write(
            cargo.join("config.toml"),
            "[build]\nbuild-dir = '../intermediates'\n",
        )
        .unwrap();
        let runner = FakeRunner::new(1, None);
        assert!(
            inspect_ownership(&p, &runner)
                .unwrap_err()
                .contains("build.build-dir")
        );
        assert!(runner.calls().is_empty());
    }
    #[test]
    fn revalidate_rejects_recent_source_and_missing_target() {
        let (_d, p) = fixture();
        fs::write(p.project_root.join("src/main.rs"), "fn main() { }\n").unwrap();
        assert!(
            revalidate(
                &DiscoveredProject {
                    project_root: p.project_root.clone(),
                    manifest_path: p.project_root.join("Cargo.toml"),
                    target_path: p.artifact.target_path.clone(),
                },
                300
            )
            .unwrap()
            .is_none()
        );
        fs::rename(
            &p.artifact.target_path,
            p.project_root.join("target-renamed"),
        )
        .unwrap();
        assert!(
            revalidate(
                &DiscoveredProject {
                    project_root: p.project_root.clone(),
                    manifest_path: p.project_root.join("Cargo.toml"),
                    target_path: p.project_root.join("target"),
                },
                0
            )
            .is_err()
        );
    }
    #[test]
    fn clean_requires_an_absolute_root() {
        let runner = FakeRunner::new(1, None);
        let err = clean_with(Path::new("."), 0, CleanMode::DryRun, &runner).unwrap_err();
        assert!(err.to_string().contains("absolute sandbox root"));
        assert!(runner.calls().is_empty());
    }
    #[test]
    fn dry_run_calls_only_cargo_dry_run_and_execution_is_explicit() {
        let (d, _p) = fixture();
        let dry = FakeRunner::new(1, None);
        let report = clean_with(d.path(), 0, CleanMode::DryRun, &dry).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].outcome, CleanOutcome::Previewed);
        let calls = dry.calls();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].1[0] == "metadata");
        assert!(calls[1].1[0] == "clean");
        assert!(calls[1].1.iter().any(|a| a == "--dry-run"));
        assert!(calls[1].1.iter().any(|a| a == "--verbose"));
        assert!(d.path().join("target/artifact.bin").exists());
        let exec = FakeRunner::new(1, None);
        let report = clean_with(d.path(), 0, CleanMode::Execute, &exec).unwrap();
        assert_eq!(report.results[0].outcome, CleanOutcome::Cleaned);
        let calls = exec.calls();
        let clean_call = calls
            .iter()
            .find(|(_, a)| a.first().is_some_and(|x| x == "clean"))
            .unwrap();
        assert!(!clean_call.1.iter().any(|a| a == "--dry-run"));
        assert!(clean_call.1.iter().any(|a| {
            a == fs::canonicalize(d.path().join("target"))
                .unwrap()
                .as_os_str()
        }));
        assert!(
            d.path().join("target/artifact.bin").exists(),
            "mock runner must not execute destructive cleanup"
        );
    }
    #[test]
    fn cargo_dry_run_leaves_fixture_untouched() {
        let (d, _p) = fixture();
        let marker_before = fs::read(d.path().join("target/CACHEDIR.TAG")).unwrap();
        let artifact_before = fs::read(d.path().join("target/artifact.bin")).unwrap();
        let lock_before = fs::read(d.path().join("Cargo.lock")).unwrap();
        let report = clean(d.path(), 0, CleanMode::DryRun).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(
            report.results[0].outcome,
            CleanOutcome::Previewed,
            "{}",
            report.render()
        );
        assert!(report.render().contains("no cleanup executed"));
        assert_eq!(fs::read(d.path().join("Cargo.lock")).unwrap(), lock_before);
        assert_eq!(
            fs::read(d.path().join("target/artifact.bin")).unwrap(),
            artifact_before
        );
        assert_eq!(
            fs::read(d.path().join("target/CACHEDIR.TAG")).unwrap(),
            marker_before
        );
    }
    #[test]
    fn execution_reports_measured_size_decrease_with_fake_cargo() {
        let (d, _p) = fixture();
        let fake = FakeRunner::removing_artifact();
        let report = clean_with(d.path(), 0, CleanMode::Execute, &fake).unwrap();
        assert_eq!(report.results[0].outcome, CleanOutcome::Cleaned);
        assert!(report.results[0].before_bytes.unwrap() > report.results[0].after_bytes.unwrap());
        assert!(report.results[0].observed_decrease.unwrap() > 0);
        assert!(!d.path().join("target/artifact.bin").exists());
        let calls = fake.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|(_, args)| args.first().is_some_and(|a| a == "metadata"))
                .count(),
            2,
            "execution repeats Cargo ownership metadata preflight"
        );
    }
}
