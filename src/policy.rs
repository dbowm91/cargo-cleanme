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
            // `config::load` range-checks retention to 0–3650 days.
            ScanScope::Routine(routine_roots(config.learned_root_retention_days as u16)),
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
    let (roots, warning) = routine_roots_from_state(
        &home,
        retention_days,
        crate::discovery_state::load_default(),
    );
    if let Some(warning) = warning {
        eprintln!("cargo-cleanme: {warning}; using seed/configured Routine roots");
    }
    roots
}

fn routine_roots_from_state(
    home: &Path,
    retention_days: u16,
    load: crate::discovery_state::StateLoad,
) -> (Vec<PathBuf>, Option<String>) {
    let mut roots: Vec<PathBuf> = routine_seed_candidates(home)
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    let warning = match load {
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
            None
        }
        load => load.diagnostic(),
    };
    (canonical_dedup_roots(roots), warning)
}
#[cfg(unix)]
pub fn global_discovery_policy() -> GlobalDiscoveryPolicy {
    compose_global_policy(unix_policy(
        cfg!(target_os = "macos"),
        cfg!(target_os = "linux"),
        cfg!(target_os = "macos") && Path::new("/usr/local").is_dir(),
    ))
}

/// Turn a platform's declared roots and prunes into the production global
/// policy: collapse redundant roots, then fill in the managed-tool prunes.
///
/// Split out from [`global_discovery_policy`] so the composition itself — not
/// just the platform declaration — is reachable from a test. A green assertion
/// about the pre-collapse roots proved nothing about what production walks.
#[cfg(unix)]
fn compose_global_policy(platform: GlobalDiscoveryPolicy) -> GlobalDiscoveryPolicy {
    // A root sitting strictly inside a global-only prune is that prune's
    // exception, so it must survive the collapse as a root of its own. Folded
    // into its ancestor it would be indistinguishable from the pruned subtree,
    // and the prune below would swallow it whole (macOS `/usr` vs `/usr/local`).
    let protected: Vec<PathBuf> = platform
        .roots
        .iter()
        .filter(|root| {
            platform
                .global_only_prunes
                .iter()
                .any(|prune| prune != *root && root.starts_with(prune))
        })
        .cloned()
        .collect();
    GlobalDiscoveryPolicy {
        roots: canonical_dedup_roots_protecting(platform.roots, &protected),
        global_only_prunes: platform.global_only_prunes,
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

/// Lexically normalize an absolute path without touching the filesystem.
///
/// Used as the identity of a root that cannot be canonicalized (deleted, or
/// unreadable). It is only ever compared against other roots, never walked.
fn lexical_identity(path: &Path) -> PathBuf {
    if !path.is_absolute() {
        return path.to_path_buf();
    }
    let mut out = PathBuf::from(std::path::MAIN_SEPARATOR_STR);
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn canonical_dedup_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    canonical_dedup_roots_protecting(roots, &[])
}

/// Dedup and collapse, keeping every root in `protected` even when an already
/// collapsed root contains it.
///
/// A protected root is one the caller listed *in addition to* the ancestor that
/// contains it, because it is that ancestor's exception (see
/// [`compose_global_policy`]). Collapsing it would re-create the ambiguity the
/// caller resolved, and the scan would double-count nothing while losing the
/// exception.
fn canonical_dedup_roots_protecting(roots: Vec<PathBuf>, protected: &[PathBuf]) -> Vec<PathBuf> {
    // Dedup and collapse on the best available identity for each root. A root
    // that cannot be canonicalized previously kept its raw spelling, so it was
    // neither deduped nor collapsed against a parent root and an overlapping
    // subtree was scanned twice (L16). Each surviving root keeps its own
    // spelling, so "scan root is unavailable" still names what the user asked
    // for.
    let mut identified: Vec<(PathBuf, PathBuf)> = roots
        .into_iter()
        .map(|root| match fs::canonicalize(&root) {
            // Resolvable roots keep their canonical spelling, as before.
            Ok(canonical) => (canonical.clone(), canonical),
            // Unresolvable roots keep their own spelling, but are compared by
            // their lexical identity so an overlapping parent still collapses.
            Err(_) => (lexical_identity(&root), root),
        })
        .collect();
    identified.sort();
    identified.dedup_by(|a, b| a.0 == b.0);
    let mut collapsed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (identity, root) in identified {
        if protected.contains(&identity)
            || !collapsed
                .iter()
                .any(|(parent, _): &(PathBuf, PathBuf)| identity.starts_with(parent))
        {
            collapsed.push((identity, root));
        }
    }
    collapsed.into_iter().map(|(_, root)| root).collect()
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
        // Composed, not merely declared: `compose_global_policy` is what
        // `global_discovery_policy` applies, so this asserts the roots
        // production actually walks. Asserting on `unix_policy` proved only
        // that the exception root existed before the collapse removed it.
        let policy = compose_global_policy(unix_policy(true, false, true));
        assert!(policy.global_only_prunes.contains(&PathBuf::from("/usr")));
        assert!(
            policy.roots.contains(&PathBuf::from("/usr/local")),
            "the /usr/local exception root must survive composition, got {:?}",
            policy.roots
        );
        assert!(policy.roots.contains(&PathBuf::from("/")));
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
    fn a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root() {
        // The collapse folds a nested root into its ancestor. For `/usr/local`
        // that folds the exception into the very prune it exists to escape.
        let roots = vec![
            PathBuf::from("/"),
            PathBuf::from("/usr/local"),
            PathBuf::from("/usr/local/bin"),
        ];
        let composed = canonical_dedup_roots_protecting(roots, &[PathBuf::from("/usr/local")]);
        assert!(composed.contains(&PathBuf::from("/")));
        assert!(composed.contains(&PathBuf::from("/usr/local")));
        // An ordinary nested root is still folded, so the protection is not a
        // blanket licence to walk everything twice.
        assert!(!composed.contains(&PathBuf::from("/usr/local/bin")));
        // With nothing protected, the plain collapse behaves as before.
        let plain = canonical_dedup_roots(vec![PathBuf::from("/"), PathBuf::from("/usr/local")]);
        assert_eq!(plain, vec![PathBuf::from("/")]);
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

    #[test]
    fn full_resolution_ignores_a_configured_root() {
        // M012A §7: rootless `scan` is Full and configuration must not be able
        // to silently narrow the canonical reconciliation command. The
        // `full` arm returns before `config.root` is ever consulted, so this is
        // the whole argument — a real Full walk is not needed to prove it, and
        // deliberately is not run in a test.
        let d = tempdir().unwrap();
        let c = ScanConfig {
            root: Some(d.path().to_path_buf()),
            ..Default::default()
        };
        let p = resolve(
            ScanRequest {
                cli_root: None,
                full: true,
            },
            &c,
        )
        .unwrap();
        assert!(
            matches!(p.scope, ScanScope::Global(_)),
            "a configured scan.root must not narrow a Full scan: {:?}",
            p.scope
        );
        assert_ne!(p.scope, ScanScope::Explicit(d.path().to_path_buf()));
    }

    #[test]
    fn maintenance_scope_is_routine_without_a_configured_root() {
        // M012A: the maintenance scope is Routine whenever no `scan.root` is
        // configured, whatever seed roots happen to exist on this machine. The
        // assertion is about the resolved variant, never the root list, so the
        // case is machine-independent.
        let c = ScanConfig::default();
        assert!(c.root.is_none());
        let p = resolve(
            ScanRequest {
                cli_root: None,
                full: false,
            },
            &c,
        )
        .unwrap();
        assert!(matches!(p.scope, ScanScope::Routine(_)), "{:?}", p.scope);
    }

    #[test]
    fn routine_state_errors_fall_back_to_seeds_with_one_warning() {
        let d = tempdir().unwrap();
        let seed = d.path().join("Projects");
        fs::create_dir(&seed).unwrap();
        let states = [
            crate::discovery_state::StateLoad::RecoverableInvalid(
                crate::discovery_state::StateProblem::Invalid {
                    path: d.path().join("state.json"),
                    detail: "invalid schema 0".into(),
                },
            ),
            crate::discovery_state::StateLoad::UnsupportedNewer {
                path: d.path().join("state.json"),
                schema: 99,
                current: crate::discovery_state::CURRENT_SCHEMA,
            },
        ];
        for state in states {
            let (roots, warning) = routine_roots_from_state(d.path(), 30, state);
            assert_eq!(roots, vec![fs::canonicalize(&seed).unwrap()]);
            assert!(warning.is_some());
        }
    }
}
