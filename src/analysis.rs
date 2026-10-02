#![allow(clippy::collapsible_if, clippy::items_after_test_module)]
use crate::{domain::*, traverse};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub struct ScanClock {
    pub start: SystemTime,
    pub cutoff: SystemTime,
}
impl ScanClock {
    pub fn new(start: SystemTime, recency: Duration) -> Option<Self> {
        Some(Self {
            start,
            cutoff: start.checked_sub(recency)?,
        })
    }
}
fn recent(t: SystemTime, c: &ScanClock) -> bool {
    t >= c.cutoff || t > c.start
}
fn source_activity(
    project: &Path,
    target: &Path,
    clock: &ScanClock,
) -> Result<Option<SystemTime>, ()> {
    match traverse::source_activity(project, target, clock.start, clock.cutoff) {
        Err(()) => Err(()),
        Ok(traverse::SourceActivity::Recent(t)) => Ok(Some(t)),
        Ok(traverse::SourceActivity::Quiet(newest)) => Ok(newest),
    }
}
fn measure(target: &Path, clock: &ScanClock) -> Result<(ArtifactAnalysis, bool), ()> {
    let stats = traverse::measure_single_target(target);
    if stats.uncertain {
        return Err(());
    }
    let active = stats.newest.is_some_and(|t| recent(t, clock));
    Ok((
        ArtifactAnalysis {
            target_path: target.to_path_buf(),
            bytes: stats.bytes,
            metric: metric(),
            newest_mtime: stats.newest,
            artifact_entries: stats.entries,
        },
        active,
    ))
}
pub fn analyze(
    project: &DiscoveredProject,
    clock: &ScanClock,
) -> Result<Option<EligibleProject>, ScanDiagnostic> {
    match source_activity(&project.project_root, &project.target_path, clock) {
        Err(_) => {
            return Err(problem(
                &project.project_root,
                "source activity could not be established",
            ));
        }
        Ok(Some(t)) if recent(t, clock) => return Ok(None),
        _ => {}
    }
    match measure(&project.target_path, clock) {
        Err(_) => Err(problem(
            &project.target_path,
            "target activity or size could not be established",
        )),
        Ok((a, active)) => {
            if active || a.artifact_entries == 0 {
                Ok(None)
            } else {
                Ok(Some(EligibleProject {
                    project_root: project.project_root.clone(),
                    artifact: a,
                }))
            }
        }
    }
}

/// Analyze many candidates while sharing one bounded worker pool for the
/// expensive target-sizing phase.
///
/// Source activity stays candidate-sequential to preserve cheap early
/// rejection: a recently edited project never pays for target sizing.
/// Survivors are then sized together with
/// [`traverse::measure_many_targets`], so candidate count grows work, not
/// worker pools. Eligibility, ordering, and uncertainty semantics match
/// [`analyze`].
pub fn analyze_many(
    projects: &[DiscoveredProject],
    clock: &ScanClock,
) -> (Vec<EligibleProject>, Vec<ScanDiagnostic>) {
    let mut survivors: Vec<(usize, DiscoveredProject)> = Vec::new();
    let mut eligible = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, project) in projects.iter().enumerate() {
        match source_activity(&project.project_root, &project.target_path, clock) {
            Err(_) => diagnostics.push(problem(
                &project.project_root,
                "source activity could not be established",
            )),
            Ok(Some(t)) if recent(t, clock) => {}
            _ => survivors.push((index, project.clone())),
        }
    }
    if survivors.is_empty() {
        return (eligible, diagnostics);
    }
    let targets: Vec<(usize, PathBuf)> = survivors
        .iter()
        .map(|(index, project)| (*index, project.target_path.clone()))
        .collect();
    let sized = traverse::measure_many_targets(&targets);
    let by_index: std::collections::HashMap<usize, traverse::TargetStats> =
        sized.into_iter().collect();
    for (index, project) in survivors {
        let Some(stats) = by_index.get(&index) else {
            diagnostics.push(problem(
                &project.target_path,
                "target activity or size could not be established",
            ));
            continue;
        };
        if stats.uncertain {
            diagnostics.push(problem(
                &project.target_path,
                "target activity or size could not be established",
            ));
            continue;
        }
        let active = stats.newest.is_some_and(|t| recent(t, clock));
        if active || stats.entries == 0 {
            continue;
        }
        eligible.push(EligibleProject {
            project_root: project.project_root.clone(),
            artifact: ArtifactAnalysis {
                target_path: project.target_path.clone(),
                bytes: stats.bytes,
                metric: metric(),
                newest_mtime: stats.newest,
                artifact_entries: stats.entries,
            },
        });
    }
    (eligible, diagnostics)
}
fn problem(path: &Path, msg: &str) -> ScanDiagnostic {
    ScanDiagnostic {
        severity: DiagnosticSeverity::Error,
        category: DiagnosticCategory::CandidateUncertain,
        path: Some(PathBuf::from(path)),
        message: msg.into(),
    }
}
#[cfg(test)]
mod analysis_fixtures {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    fn quiet_clock() -> ScanClock {
        ScanClock::new(SystemTime::now() + Duration::from_secs(5), Duration::ZERO).unwrap()
    }
    #[test]
    fn sizing_counts_artifact_entries_and_bytes() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("artifact.bin"), vec![1u8; 4096]).unwrap();
        let clock = quiet_clock();
        let (a, active) = measure(&target, &clock).unwrap();
        assert_eq!(a.artifact_entries, 1);
        assert!(a.bytes >= 4096);
        assert!(!active);
    }
    #[test]
    fn recent_source_short_circuits_missing_target() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("src.rs"), "recent").unwrap();
        let now = SystemTime::now() + Duration::from_secs(1);
        let clock = ScanClock::new(now, Duration::from_secs(300)).unwrap();
        let p = DiscoveredProject {
            project_root: d.path().to_path_buf(),
            manifest_path: d.path().join("Cargo.toml"),
            target_path: d.path().join("missing-target"),
        };
        assert!(analyze(&p, &clock).unwrap().is_none());
    }
    #[test]
    fn empty_target_has_no_artifacts() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let clock = quiet_clock();
        let (a, _) = measure(&target, &clock).unwrap();
        assert_eq!(a.artifact_entries, 0);
    }
    #[test]
    fn batched_and_single_analysis_agree() {
        let d = tempdir().unwrap();
        let mut projects = Vec::new();
        for name in ["one", "two", "three"] {
            let root = d.path().join(name);
            let target = root.join("target");
            fs::create_dir_all(&target).unwrap();
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname='x'\nversion='0.1.0'\n",
            )
            .unwrap();
            fs::write(target.join("artifact.bin"), vec![3u8; 1024]).unwrap();
            // Backdate everything so the quiet clock treats them as inactive.
            let old = SystemTime::now() - Duration::from_secs(3600);
            filetime_backdate(&root, old);
            projects.push(DiscoveredProject {
                project_root: root.clone(),
                manifest_path: root.join("Cargo.toml"),
                target_path: target,
            });
        }
        let clock = quiet_clock();
        let mut singles = Vec::new();
        for project in &projects {
            if let Some(eligible) = analyze(project, &clock).unwrap() {
                singles.push(eligible);
            }
        }
        let (mut batched, diagnostics) = analyze_many(&projects, &clock);
        assert!(diagnostics.is_empty());
        singles.sort_by(|a, b| a.project_root.cmp(&b.project_root));
        batched.sort_by(|a, b| a.project_root.cmp(&b.project_root));
        assert_eq!(singles.len(), batched.len());
        for (single, batch) in singles.iter().zip(batched.iter()) {
            assert_eq!(single.project_root, batch.project_root);
            assert_eq!(single.artifact.bytes, batch.artifact.bytes);
            assert_eq!(
                single.artifact.artifact_entries,
                batch.artifact.artifact_entries
            );
        }
    }
    #[test]
    fn analysis_bytes_match_traverse_measure() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("artifact.bin"), vec![5u8; 2048]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        filetime_backdate(&target, old);
        let clock = quiet_clock();
        let (measured, _) = measure(&target, &clock).unwrap();
        let direct = traverse::measure_single_target(&target);
        assert!(!direct.uncertain);
        assert_eq!(measured.bytes, direct.bytes);
        assert_eq!(measured.artifact_entries, direct.entries);
    }
    #[test]
    fn nested_vcs_boundary_excludes_nested_repo_activity() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("artifact.bin"), vec![1u8; 512]).unwrap();
        // Nested independent repository: activity inside it must not
        // activate the outer project because the nested boundary (including
        // the boundary directory's own mtime) carries no source evidence.
        let nested = d.path().join("nested");
        fs::create_dir_all(nested.join(".git")).unwrap();
        let fresh = nested.join("fresh.rs");
        fs::write(&fresh, "recent").unwrap();
        // Forward-date the nested file and run the clock after the project
        // setup: the project root (created now) is old relative to the
        // future clock, while the nested file is recent. No directory mtime
        // manipulation is needed because excluded boundaries (target, nested)
        // carry no evidence on any platform.
        let start = SystemTime::now() + Duration::from_secs(3600);
        let fresh_time = start;
        if let Ok(file) = fs::OpenOptions::new().write(true).open(&fresh) {
            let _ = file.set_modified(fresh_time);
        }
        let clock = ScanClock::new(start, Duration::from_secs(300)).unwrap();
        match source_activity(d.path(), &target, &clock) {
            Ok(Some(t)) if recent(t, &clock) => {
                panic!("nested VCS activity must be pruned")
            }
            Ok(_) => {}
            Err(()) => panic!("nested boundary must not be uncertain"),
        }
    }
    #[test]
    fn disappearing_target_is_uncertain() {
        let d = tempdir().unwrap();
        let missing = d.path().join("missing-target");
        let clock = quiet_clock();
        assert!(measure(&missing, &clock).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_paths_are_measured() {
        use std::os::unix::ffi::OsStringExt;
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let bad = target.join(std::ffi::OsString::from_vec(b"bad-\xff.bin".to_vec()));
        fs::write(&bad, vec![1u8; 256]).unwrap();
        let clock = quiet_clock();
        let (a, _) = measure(&target, &clock).unwrap();
        assert_eq!(a.artifact_entries, 1);
    }
    #[cfg(unix)]
    #[test]
    fn symlink_entries_are_not_followed() {
        use std::os::unix::fs::symlink;
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("real.bin"), vec![1u8; 512]).unwrap();
        let external = d.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("external.bin"), vec![2u8; 8192]).unwrap();
        symlink(&external, target.join("linked-dir")).unwrap();
        symlink(
            external.join("external.bin"),
            target.join("linked-file.bin"),
        )
        .unwrap();
        let clock = quiet_clock();
        let (a, _) = measure(&target, &clock).unwrap();
        // real file + two symlinks; linked directory contents are not walked.
        assert_eq!(a.artifact_entries, 3);
        assert!(
            a.bytes < 8192 + 512,
            "linked directory contents must not be sized"
        );
        // A symlinked target root itself is uncertain, never eligible.
        let link_target = d.path().join("link-target");
        symlink(&target, &link_target).unwrap();
        assert!(measure(&link_target, &clock).is_err());
    }
    fn filetime_backdate(path: &Path, old: SystemTime) {
        let mut stack = vec![path.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(meta) = fs::symlink_metadata(&current) {
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    if let Ok(children) = fs::read_dir(&current) {
                        for child in children.flatten() {
                            stack.push(child.path());
                        }
                    }
                }
            }
            if let Ok(file) = fs::OpenOptions::new().write(true).open(&current) {
                let _ = file.set_modified(old);
            } else if let Ok(dir) = fs::File::open(&current) {
                let _ = dir.set_modified(old);
            }
        }
        // Ensure the root itself is also backdated (directories need reopen).
        if let Ok(dir) = fs::File::open(path) {
            let _ = dir.set_modified(old);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_cutoff_and_future_time() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let c = ScanClock::new(now, Duration::from_secs(300)).unwrap();
        assert!(recent(c.cutoff, &c));
        assert!(!recent(c.cutoff - Duration::from_secs(1), &c));
        assert!(recent(now + Duration::from_secs(1), &c));
    }
    #[test]
    fn huge_window_rejected() {
        assert!(ScanClock::new(SystemTime::UNIX_EPOCH, Duration::MAX).is_none());
    }
}

fn metric() -> SizeMetric {
    #[cfg(any(unix, windows))]
    {
        SizeMetric::Allocated
    }
    #[cfg(not(any(unix, windows)))]
    {
        SizeMetric::Apparent
    }
}
