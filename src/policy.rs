use crate::{
    config::ScanConfig,
    domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope},
    error::AppError,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// Roots and exclusions that apply only to an implicit machine-wide scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalDiscoveryPolicy {
    pub roots: Vec<PathBuf>,
    pub global_only_prunes: Vec<PathBuf>,
    pub managed_tool_prunes: Vec<PathBuf>,
}
pub fn resolve(request: ScanRequest, config: &ScanConfig) -> Result<EffectiveScanPolicy, AppError> {
    if request.full {
        return Ok(EffectiveScanPolicy {
            recency: Duration::from_secs(config.recency_seconds),
            scope: ScanScope::Global(global_discovery_policy().roots),
            discovery_filters: DiscoveryFilters::Active {
                ignore: config.ignore.clone(),
                unignore: config.unignore.clone(),
            },
        });
    }
    let explicit = request.cli_root.or_else(|| config.root.clone());
    let (scope, discovery_filters) = if let Some(root) = explicit {
        let m = fs::symlink_metadata(&root).map_err(|e| AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: e.to_string(),
        })?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(AppError::InvalidRoot {
                path: root.display().to_string(),
                reason: "expected a real directory".into(),
            });
        }
        (ScanScope::Explicit(root), DiscoveryFilters::Bypassed)
    } else {
        (
            ScanScope::Routine(routine_roots(config.learned_root_retention_days)),
            DiscoveryFilters::Active {
                ignore: config.ignore.clone(),
                unignore: config.unignore.clone(),
            },
        )
    };
    Ok(EffectiveScanPolicy {
        recency: Duration::from_secs(config.recency_seconds),
        scope,
        discovery_filters,
    })
}

pub fn routine_seed_candidates(home: &Path) -> Vec<PathBuf> {
    [
        "Projects",
        "projects",
        "Developer",
        "dev",
        "Code",
        "code",
        "src",
        "repos",
        "Repos",
        "GitHub",
        "github",
        "workspace",
        "workspaces",
    ]
    .iter()
    .map(|name| home.join(name))
    .collect()
}

fn routine_roots(retention_days: u16) -> Vec<PathBuf> {
    let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = routine_seed_candidates(&home)
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    match crate::discovery_state::load_default() {
        crate::discovery_state::StateLoad::Loaded(state) => {
            let now = crate::discovery_state::now_seconds();
            roots.extend(
                state
                    .learned_roots
                    .into_iter()
                    .filter(|r| {
                        retention_days == 0
                            || now < r.last_project_seen_at
                            || now.saturating_sub(r.last_project_seen_at)
                                <= u64::from(retention_days) * 86400
                    })
                    .map(|r| r.path),
            );
        }
        load => {
            if let Some(error) = load.diagnostic() {
                eprintln!("cargo-cleanme: {error}; using seed/configured Routine roots");
            }
        }
    }
    canonical_dedup_roots(roots)
}
#[cfg(unix)]
#[cfg(unix)]
pub fn global_discovery_policy() -> GlobalDiscoveryPolicy {
    let policy = unix_policy(
        cfg!(target_os = "macos"),
        cfg!(target_os = "linux"),
        cfg!(target_os = "macos") && Path::new("/usr/local").is_dir(),
    );
    GlobalDiscoveryPolicy {
        roots: canonical_dedup_roots(policy.roots),
        global_only_prunes: policy.global_only_prunes,
        managed_tool_prunes: managed_rust_prunes(),
    }
}

#[cfg(unix)]
fn unix_policy(macos: bool, linux: bool, usr_local_present: bool) -> GlobalDiscoveryPolicy {
    let mut roots = vec![PathBuf::from("/")];
    let global_only_prunes = if macos {
        if usr_local_present {
            roots.push(PathBuf::from("/usr/local"));
        }
        ["/System", "/dev", "/bin", "/sbin", "/usr"]
            .map(PathBuf::from)
            .to_vec()
    } else if linux {
        ["/proc", "/sys", "/dev", "/run"]
            .map(PathBuf::from)
            .to_vec()
    } else {
        Vec::new()
    };
    GlobalDiscoveryPolicy {
        roots,
        global_only_prunes,
        managed_tool_prunes: Vec::new(),
    }
}

#[cfg(windows)]
pub fn global_discovery_policy() -> GlobalDiscoveryPolicy {
    use windows_sys::Win32::{
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
        System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
    };
    let mask = unsafe { GetLogicalDrives() };
    let roots = (0..26)
        .filter_map(|i| {
            if mask & (1 << i) == 0 {
                return None;
            }
            let letter = char::from(b'A' + i as u8);
            let path = format!("{letter}:\\");
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (kind == DRIVE_FIXED || kind == DRIVE_REMOVABLE).then(|| PathBuf::from(path))
        })
        .collect();
    GlobalDiscoveryPolicy {
        roots: canonical_dedup_roots(roots),
        global_only_prunes: Vec::new(),
        managed_tool_prunes: managed_rust_prunes(),
    }
}

fn canonical_dedup_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<_> = roots
        .into_iter()
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
        .collect();
    roots.sort();
    roots.dedup();
    let mut collapsed = Vec::new();
    for root in roots {
        if !collapsed
            .iter()
            .any(|parent: &PathBuf| root.starts_with(parent))
        {
            collapsed.push(root);
        }
    }
    collapsed
}

fn managed_rust_prunes() -> Vec<PathBuf> {
    crate::discovery::effective_rustup_home()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfig;
    use tempfile::tempdir;

    #[test]
    #[cfg(unix)]
    fn macos_global_policy_prunes_protected_usr_and_enumerates_usr_local() {
        let policy = unix_policy(true, false, true);
        assert!(policy.global_only_prunes.contains(&PathBuf::from("/usr")));
        assert!(policy.roots.contains(&PathBuf::from("/usr/local")));
        assert!(
            !policy
                .global_only_prunes
                .contains(&PathBuf::from("/Library"))
        );
        assert!(
            !policy
                .global_only_prunes
                .contains(&PathBuf::from("/Applications"))
        );
    }

    #[test]
    #[cfg(unix)]
    fn linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes() {
        let policy = unix_policy(false, true, false);
        for path in ["/proc", "/sys", "/dev", "/run"] {
            assert!(policy.global_only_prunes.contains(&PathBuf::from(path)));
        }
        assert!(!policy.global_only_prunes.contains(&PathBuf::from("/usr")));
    }
    #[test]
    fn cli_root_wins_and_bypasses_filters() {
        let d = tempdir().unwrap();
        let cli = d.path().to_path_buf();
        let configured = PathBuf::from("/tmp");
        let c = ScanConfig {
            root: Some(configured),
            ignore: vec!["/tmp/*".into()],
            unignore: vec![PathBuf::from("/tmp/keep")],
            ..Default::default()
        };
        let p = resolve(
            ScanRequest {
                cli_root: Some(cli.clone()),
                full: false,
            },
            &c,
        )
        .unwrap();
        assert_eq!(p.scope, ScanScope::Explicit(cli));
        assert_eq!(p.discovery_filters, DiscoveryFilters::Bypassed);
        assert_eq!(p.recency, Duration::from_secs(300));
    }
    #[test]
    fn configured_root_wins_over_global() {
        let d = tempdir().unwrap();
        let c = ScanConfig {
            root: Some(d.path().into()),
            ..Default::default()
        };
        assert!(matches!(
            resolve(
                ScanRequest {
                    cli_root: None,
                    full: false
                },
                &c
            )
            .unwrap()
            .scope,
            ScanScope::Explicit(_)
        ));
    }
}
