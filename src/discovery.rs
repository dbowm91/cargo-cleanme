#![allow(clippy::collapsible_if)]
use crate::{domain::*, error::AppError};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Filters {
    globs: GlobSet,
    unignore: Vec<PathBuf>,
}
impl Filters {
    fn new(ignore: &[String], unignore: &[PathBuf]) -> Result<Self, AppError> {
        let mut b = GlobSetBuilder::new();
        for g in ignore {
            b.add(
                Glob::new(g)
                    .map_err(|e| AppError::Config(format!("invalid ignore glob {g}: {e}")))?,
            );
        }
        Ok(Self {
            globs: b.build().map_err(|e| AppError::Config(e.to_string()))?,
            unignore: unignore.to_vec(),
        })
    }
    fn ignored(&self, p: &Path) -> bool {
        let Some(s) = p.to_str() else {
            return false;
        };
        self.globs.is_match(s) && !self.unignore.iter().any(|u| p.starts_with(u))
    }
    fn exception_below(&self, p: &Path) -> bool {
        self.unignore.iter().any(|u| u.starts_with(p))
    }
}
fn vcs(n: &str) -> bool {
    matches!(n, ".git" | ".hg" | ".svn")
}
fn system_prune(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    if matches!(
        path,
        Path::new("/proc") | Path::new("/sys") | Path::new("/dev") | Path::new("/run")
    ) {
        return true;
    }
    #[cfg(target_os = "macos")]
    if path == Path::new("/dev") {
        return true;
    }
    false
}
pub fn scan(policy: &EffectiveScanPolicy) -> Result<ScanReport, AppError> {
    let roots = match &policy.scope {
        ScanScope::Explicit(p) => vec![p.clone()],
        ScanScope::Global(v) => v.clone(),
    };
    let (ignore, unignore) = match &policy.discovery_filters {
        DiscoveryFilters::Active { ignore, unignore } => (ignore.as_slice(), unignore.as_slice()),
        DiscoveryFilters::Bypassed => (&[][..], &[][..]),
    };
    let filters = Filters::new(ignore, unignore)?;
    let mut out = ScanReport::default();
    for root in roots {
        if !root.exists() {
            out.diagnostics.push(diag(
                DiagnosticCategory::PlatformRoot,
                &root,
                "scan root is unavailable",
            ));
            continue;
        }
        discover_root(
            &root,
            &filters,
            matches!(policy.scope, ScanScope::Explicit(_)),
            &mut out,
        )?;
    }
    out.discovered = out.eligible.len() as u64;
    Ok(out)
}
fn diag(cat: DiagnosticCategory, p: &Path, msg: &str) -> ScanDiagnostic {
    ScanDiagnostic {
        severity: DiagnosticSeverity::Warning,
        category: cat,
        path: Some(p.into()),
        message: msg.into(),
    }
}
fn discover_root(
    root: &Path,
    filters: &Filters,
    explicit: bool,
    out: &mut ScanReport,
) -> Result<(), AppError> {
    let walker_root = root.to_path_buf();
    let root_path = walker_root.clone();
    let descend_filters = Filters {
        globs: filters.globs.clone(),
        unignore: filters.unignore.clone(),
    };
    let mut walk = dua_core::walk(
        &walker_root,
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8),
        dua_core::Order::ParentFirst,
        dua_core::Options::default(),
        move |entry| {
            let path = entry.path();
            if !entry.file_type.is_dir() {
                return false;
            }
            if path != root_path
                && path
                    .file_name()
                    .is_some_and(|n| n == "target" || vcs(&n.to_string_lossy()))
            {
                return false;
            }
            if system_prune(&path) {
                return false;
            }
            explicit || !descend_filters.ignored(&path) || descend_filters.exception_below(&path)
        },
    );
    for item in &mut walk {
        let entry = match item {
            Ok(e) => e,
            Err(e) => {
                let category = if e.kind() == std::io::ErrorKind::PermissionDenied {
                    DiagnosticCategory::PermissionDenied
                } else {
                    DiagnosticCategory::Metadata
                };
                out.diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category,
                    path: None,
                    message: "filesystem traversal entry could not be read".into(),
                });
                continue;
            }
        };
        out.visited_entries = out.visited_entries.saturating_add(1);
        let path = entry.path();
        if entry.file_type.is_file() && path.file_name().is_some_and(|n| n == "Cargo.toml") {
            if let Some(project_root) = path.parent() {
                let target = project_root.join("target");
                if fs::symlink_metadata(&target)
                    .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
                {
                    out.eligible.push(EligibleProject {
                        project_root: project_root.to_path_buf(),
                        artifact: ArtifactAnalysis {
                            target_path: target,
                            bytes: 0,
                            metric: SizeMetric::Apparent,
                            newest_mtime: None,
                            artifact_entries: 0,
                        },
                    });
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn discovers_real_target_and_prunes_target() {
        let d = tempdir().unwrap();
        let p = d.path().join("p");
        fs::create_dir_all(p.join("target/nested")).unwrap();
        fs::write(p.join("Cargo.toml"), "").unwrap();
        fs::write(p.join("target/nested/Cargo.toml"), "").unwrap();
        let f = Filters::new(&[], &[]).unwrap();
        let mut r = ScanReport::default();
        discover_root(d.path(), &f, true, &mut r).unwrap();
        assert_eq!(r.eligible.len(), 1);
        assert_eq!(r.eligible[0].project_root, p);
        assert_eq!(
            r.visited_entries, 4,
            "target descendants must be pruned during discovery"
        );
    }
    #[test]
    fn discovery_reaches_unignored_project_without_entering_ignored_sibling() {
        let d = tempdir().unwrap();
        let archive = d.path().join("archive");
        for name in ["keep", "skip"] {
            let p = archive.join(name);
            fs::create_dir_all(p.join("target")).unwrap();
            fs::write(p.join("Cargo.toml"), "").unwrap();
        }
        let f = Filters::new(
            &[format!("{}/*", archive.display())],
            &[archive.join("keep")],
        )
        .unwrap();
        let mut r = ScanReport::default();
        discover_root(d.path(), &f, false, &mut r).unwrap();
        assert_eq!(r.eligible.len(), 1);
        assert_eq!(r.eligible[0].project_root, archive.join("keep"));
    }
    #[cfg(unix)]
    #[test]
    fn discovery_rejects_symlink_target() {
        use std::os::unix::fs::symlink;
        let d = tempdir().unwrap();
        let external = d.path().join("external");
        fs::create_dir_all(&external).unwrap();
        let project = d.path().join("p");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "").unwrap();
        symlink(&external, project.join("target")).unwrap();
        let f = Filters::new(&[], &[]).unwrap();
        let mut r = ScanReport::default();
        discover_root(d.path(), &f, true, &mut r).unwrap();
        assert!(r.eligible.is_empty());
    }
    #[test]
    fn filters_ignored_parent_keeps_exception_route() {
        let f = Filters::new(
            &["/tmp/archive/*".into()],
            &[PathBuf::from("/tmp/archive/keep")],
        )
        .unwrap();
        assert!(f.ignored(Path::new("/tmp/archive/old")));
        assert!(f.exception_below(Path::new("/tmp/archive")));
        assert!(!f.ignored(Path::new("/tmp/archive/keep/child")));
    }
}
