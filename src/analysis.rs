#![allow(clippy::collapsible_if, clippy::items_after_test_module)]
use crate::domain::*;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

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
fn newest(a: Option<SystemTime>, b: SystemTime) -> Option<SystemTime> {
    Some(a.map_or(b, |x| x.max(b)))
}
fn time(path: &Path) -> std::io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}
fn internal(n: &str) -> bool {
    matches!(n, ".git" | ".hg" | ".svn")
}
fn source_activity(
    project: &Path,
    target: &Path,
    clock: &ScanClock,
) -> Result<Option<SystemTime>, ()> {
    let mut newest_time = None;
    let walker = WalkDir::new(project)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let p = entry.path();
            if p != project && (p == target || p.starts_with(target)) {
                return false;
            }
            if p != project
                && p.file_name()
                    .is_some_and(|n| internal(&n.to_string_lossy()))
            {
                return false;
            }
            if p != project && entry.file_type().is_dir() {
                if fs::read_dir(p).ok().is_some_and(|mut children| {
                    children.any(|x| {
                        x.ok()
                            .is_some_and(|x| internal(&x.file_name().to_string_lossy()))
                    })
                }) {
                    return false;
                }
            }
            true
        });
    for e in walker {
        let e = e.map_err(|_| ())?;
        let p = e.path();
        let t = time(p).map_err(|_| ())?;
        newest_time = newest(newest_time, t);
        if recent(t, clock) {
            return Ok(Some(t));
        }
    }
    Ok(newest_time)
}
fn measure(target: &Path, clock: &ScanClock) -> Result<(ArtifactAnalysis, bool), ()> {
    let meta = fs::symlink_metadata(target).map_err(|_| ())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(());
    }
    let mut bytes = 0u64;
    let mut entries = 0u64;
    let mut newest_time = None;
    let mut active = false;
    for e in WalkDir::new(target).follow_links(false).into_iter() {
        let e = e.map_err(|_| ())?;
        let p = e.path();
        if p == target {
            continue;
        }
        let m = fs::symlink_metadata(p).map_err(|_| ())?;
        let t = m.modified().map_err(|_| ())?;
        newest_time = newest(newest_time, t);
        active |= recent(t, clock);
        entries = entries.checked_add(1).ok_or(())?;
        if m.is_file() {
            let sz = filesize::file_real_size_fast(p, &m).map_err(|_| ())?;
            bytes = bytes.checked_add(sz).ok_or(())?;
        }
    }
    Ok((
        ArtifactAnalysis {
            target_path: target.to_path_buf(),
            bytes,
            metric: metric(),
            newest_mtime: newest_time,
            artifact_entries: entries,
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
    use tempfile::tempdir;
    #[test]
    fn sizing_counts_artifact_entries_and_bytes() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("artifact.bin"), vec![1u8; 4096]).unwrap();
        let clock =
            ScanClock::new(SystemTime::now() + Duration::from_secs(5), Duration::ZERO).unwrap();
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
        let clock =
            ScanClock::new(SystemTime::now() + Duration::from_secs(5), Duration::ZERO).unwrap();
        let (a, _) = measure(&target, &clock).unwrap();
        assert_eq!(a.artifact_entries, 0);
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
        assert!(!recent(c.cutoff - Duration::from_nanos(1), &c));
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
