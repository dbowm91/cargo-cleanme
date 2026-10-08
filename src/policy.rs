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

/// Provenance of a cleanup root candidate.
///
/// Explicit roots are caller-selected (`scan.root`, `scan ROOT`, `clean ROOT`)
/// and authoritative: any defect is fatal. Automatic roots (seed hints and
/// learned state) are search hints only: proven absence/non-directory omits
/// them from the current invocation, while uncertainty blocks mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootProvenance {
    Explicit,
    Automatic,
}

/// A safely omittable automatic root: its absence or non-directory type was
/// positively established with `symlink_metadata`, so excluding it from this
/// invocation cannot conceal an expected scan tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutomaticOmission {
    NotFound(PathBuf),
    NonDirectory(PathBuf),
}

/// An automatic root that cannot be safely omitted: a symlink (never followed,
/// including broken links) or an indeterminate filesystem failure
/// (`PermissionDenied`, `EIO`, unknown). Omitting it could conceal an
/// expected scan tree, so the run must block mutation with a typed report
/// rather than reduce the ownership universe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticBlock {
    pub path: PathBuf,
    pub reason: String,
}

/// The outcome of provenance-aware admission over one candidate root set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassifiedRoots {
    pub admitted: Vec<PathBuf>,
    pub omitted: Vec<AutomaticOmission>,
    pub blocked: Vec<AutomaticBlock>,
}

impl ClassifiedRoots {
    pub fn omitted_paths(&self) -> Vec<PathBuf> {
        self.omitted
            .iter()
            .map(|o| match o {
                AutomaticOmission::NotFound(p) | AutomaticOmission::NonDirectory(p) => p.clone(),
            })
            .collect()
    }

    /// Deterministic, bounded human diagnostics for safely omitted roots.
    pub fn omission_diagnostics(&self) -> Vec<String> {
        let mut diagnostics: Vec<String> = self
            .omitted
            .iter()
            .map(|o| match o {
                AutomaticOmission::NotFound(p) => {
                    format!("omitting unavailable learned root {}", p.display())
                }
                AutomaticOmission::NonDirectory(p) => {
                    format!("omitting non-directory learned root {}", p.display())
                }
            })
            .collect();
        diagnostics.sort();
        diagnostics
    }
}

/// Classify cleanup root candidates by provenance without touching
/// canonicalization, so invalid identities cannot be hidden by collapse.
///
/// Uses `symlink_metadata` and the actual `io::ErrorKind`: `Path::exists` and
/// `Path::is_dir` conflate errors and follow symlinks. There is deliberately
/// no catch-all `Err(_) => continue`: every indeterminate failure blocks.
pub fn classify_cleanup_roots(entries: CleanupCandidates) -> Result<ClassifiedRoots, AppError> {
    let mut out = ClassifiedRoots::default();
    for (path, provenance) in entries {
        match (fs::symlink_metadata(&path), provenance) {
            (Ok(meta), RootProvenance::Explicit) => {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return Err(AppError::InvalidRoot {
                        path: path.display().to_string(),
                        reason: "expected a real directory".into(),
                    });
                }
                out.admitted.push(path);
            }
            (Ok(meta), RootProvenance::Automatic) => {
                if meta.file_type().is_symlink() {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!(
                            "automatic root {} is a symlink; refusing to follow or omit",
                            path.display()
                        ),
                    });
                } else if meta.is_dir() {
                    out.admitted.push(path);
                } else {
                    out.omitted.push(AutomaticOmission::NonDirectory(path));
                }
            }
            (Err(e), RootProvenance::Explicit) => {
                return Err(AppError::InvalidRoot {
                    path: path.display().to_string(),
                    reason: e.to_string(),
                });
            }
            (Err(e), RootProvenance::Automatic) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    out.omitted.push(AutomaticOmission::NotFound(path));
                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!("cannot access automatic root {}: {e}", path.display()),
                    });
                } else {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!(
                            "indeterminate filesystem state for automatic root {}: {e}",
                            path.display()
                        ),
                    });
                }
            }
        }
    }
    out.admitted.sort();
    out.admitted.dedup();
    Ok(out)
}

/// Re-examine one admission premise immediately before a destructive spawn.
///
/// `omitted` holds the safely omitted automatic roots with the kind of
/// positive evidence each was omitted on, and `admitted` holds the roots the
/// ownership universe was built from:
/// - a `NotFound` omission is violated by any existence or any indeterminate
///   failure;
/// - a `NonDirectory` omission is violated only by a directory, a symlink, or
///   an indeterminate failure — a deleted file is still no tree;
/// - an admitted root must still be a real (non-symlink) directory.
///
/// Any violation invalidates the boundary the proof was built on, so the
/// caller must block rather than spawn. Returns the first invalidating path
/// and its reason, if any.
pub fn recheck_admission_premise(
    admitted: &[PathBuf],
    omitted: &[AutomaticOmission],
) -> Option<(PathBuf, String)> {
    for omission in omitted {
        let (path, must_stay_absent) = match omission {
            AutomaticOmission::NotFound(p) => (p, true),
            AutomaticOmission::NonDirectory(p) => (p, false),
        };
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Some((
                    path.clone(),
                    format!(
                        "skipped root {} reappeared as a symlink; ownership boundary is unproven",
                        path.display()
                    ),
                ));
            }
            Ok(meta) if meta.is_dir() => {
                return Some((
                    path.clone(),
                    format!(
                        "skipped root {} reappeared as a directory; ownership boundary is unproven",
                        path.display()
                    ),
                ));
            }
            Ok(_) if must_stay_absent => {
                return Some((
                    path.clone(),
                    format!(
                        "skipped root {} reappeared; ownership boundary is unproven",
                        path.display()
                    ),
                ));
            }
            // A non-directory that is still a non-directory — or has since
            // vanished entirely — still contributes no tree.
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Some((
                    path.clone(),
                    format!(
                        "skipped root {} is now unreadable ({e}); ownership boundary is unproven",
                        path.display()
                    ),
                ));
            }
        }
    }
    for path in admitted {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Some((
                    path.clone(),
                    format!(
                        "admitted root {} changed type; ownership proof is stale",
                        path.display()
                    ),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Some((
                    path.clone(),
                    format!(
                        "admitted root {} disappeared; ownership proof is stale",
                        path.display()
                    ),
                ));
            }
            Err(e) => {
                return Some((
                    path.clone(),
                    format!(
                        "admitted root {} is now unreadable ({e}); ownership proof is stale",
                        path.display()
                    ),
                ));
            }
        }
    }
    None
}

/// Cleanup root candidates with provenance: each path plus whether it is a
/// caller-selected explicit root or an automatic search hint.
pub type CleanupCandidates = Vec<(PathBuf, RootProvenance)>;

/// Decide `clean --full` roots from a finished Full reconciliation.
///
/// The roots come from the in-memory generation the traversal just proved,
/// never from a disk reload that could be an older generation when
/// publication failed, was unsupported, or was unavailable. A failed save
/// stays a stderr warning for the caller; only a missing fresh generation
/// (or a non-zero reconciliation) refuses cleanup. Every returned root is
/// [`RootProvenance::Automatic`] so the shared admission path applies.
pub fn full_cleanup_roots(
    reconciliation_code: i32,
    state: Option<crate::discovery_state::DiscoveryState>,
) -> Result<(CleanupCandidates, Option<u64>), AppError> {
    if reconciliation_code != 0 {
        return Err(AppError::Config(format!(
            "full reconciliation reported an incomplete scan (exit {reconciliation_code}); \
             no cleanup commands were run against its unproven learned roots"
        )));
    }
    let Some(state) = state else {
        return Err(AppError::Config(
            "full reconciliation produced no fresh generation; \
             no cleanup commands were run against stale learned roots"
                .into(),
        ));
    };
    let generation = state.last_full_at;
    let entries = state
        .learned_roots
        .into_iter()
        .map(|r| (r.path, RootProvenance::Automatic))
        .collect();
    Ok((entries, generation))
}

/// Raw Routine cleanup candidates with provenance, before any
/// canonicalization or collapse.
///
/// Seed hints that exist in any form plus age-eligible learned roots, each
/// labeled [`RootProvenance::Automatic`]. The caller classifies by provenance
/// *before* collapsing, so a symlink identity can never be canonicalized into
/// its target first (a symlinked seed passes `Path::is_dir`, which follows
/// the link). Scan policy keeps using [`resolve`]; only cleanup admission
/// uses this seam.
pub fn routine_cleanup_candidates(
    home: &Path,
    retention_days: u16,
    load: crate::discovery_state::StateLoad,
) -> (CleanupCandidates, Option<String>) {
    let mut entries: CleanupCandidates = routine_seed_candidates(home)
        .into_iter()
        .filter(|p| fs::symlink_metadata(p).is_ok())
        .map(|p| (p, RootProvenance::Automatic))
        .collect();
    let warning = match load {
        crate::discovery_state::StateLoad::Loaded(state) => {
            let now = crate::discovery_state::now_seconds();
            entries.extend(
                state
                    .learned_roots
                    .into_iter()
                    .filter(|r| {
                        retention_days == 0
                            || now < r.last_project_seen_at
                            || now.saturating_sub(r.last_project_seen_at)
                                <= u64::from(retention_days) * 86400
                    })
                    .map(|r| (r.path, RootProvenance::Automatic)),
            );
            None
        }
        load => load.diagnostic(),
    };
    (entries, warning)
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

    /// C028 decision table: proven absence/non-directory omits automatic
    /// roots, symlinks and indeterminate failures block, and every explicit
    /// defect stays a fatal `InvalidRoot`.
    #[test]
    fn automatic_roots_classify_by_provenance_and_explicit_roots_stay_strict() {
        let d = tempdir().unwrap();
        let real = d.path().join("present");
        fs::create_dir(&real).unwrap();
        let missing = d.path().join("deleted-worktree");
        assert!(!missing.exists());
        let file = d.path().join("regular-file");
        fs::write(&file, b"not a directory").unwrap();

        // Missing + non-directory automatic roots omit; the real sibling is
        // admitted and no block is raised.
        let classified = classify_cleanup_roots(vec![
            (real.clone(), RootProvenance::Automatic),
            (missing.clone(), RootProvenance::Automatic),
            (file.clone(), RootProvenance::Automatic),
        ])
        .unwrap();
        assert_eq!(classified.admitted, vec![real.clone()]);
        assert!(classified.blocked.is_empty());
        assert_eq!(classified.omitted.len(), 2);
        assert!(
            classified
                .omitted
                .contains(&AutomaticOmission::NotFound(missing))
        );
        assert!(
            classified
                .omitted
                .contains(&AutomaticOmission::NonDirectory(file))
        );

        // The same defects on explicit roots are fatal, even alongside a
        // valid automatic sibling.
        for bad in [d.path().join("nope"), d.path().join("regular-file")] {
            let Err(crate::error::AppError::InvalidRoot { .. }) = classify_cleanup_roots(vec![
                (real.clone(), RootProvenance::Automatic),
                (bad, RootProvenance::Explicit),
            ]) else {
                panic!("explicit defects must stay fatal");
            };
        }
    }

    /// C028: a symlinked automatic root (including a symlink to a directory,
    /// which `Path::is_dir` reports as a directory) is never followed and
    /// never silently omitted — it blocks. A broken symlink blocks too.
    #[test]
    #[cfg(unix)]
    fn symlinked_automatic_roots_block_and_are_never_followed_or_omitted() {
        let d = tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = d.path().join("link-to-dir");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let broken = d.path().join("broken-link");
        std::os::unix::fs::symlink(d.path().join("nowhere"), &broken).unwrap();
        // Premise of the test, not the product: `is_dir` follows the link,
        // which is exactly why classification must use `symlink_metadata`.
        assert!(link.is_dir());

        for sym in [&link, &broken] {
            let classified =
                classify_cleanup_roots(vec![(sym.clone(), RootProvenance::Automatic)]).unwrap();
            assert!(
                classified.admitted.is_empty(),
                "a symlink is never admitted"
            );
            assert!(
                classified.omitted.is_empty(),
                "a symlink is never positive absence"
            );
            assert_eq!(classified.blocked.len(), 1);
        }
        // Explicit symlinks stay fatal rather than blocking.
        let Err(crate::error::AppError::InvalidRoot { .. }) =
            classify_cleanup_roots(vec![(link, RootProvenance::Explicit)])
        else {
            panic!("explicit symlink must be fatal");
        };
    }

    /// C028 late premise recheck: a skipped root that reappears (or becomes
    /// unreadable) and an admitted root that disappears both invalidate the
    /// boundary the proof was built on.
    #[test]
    fn admission_premise_recheck_detects_reappearance_and_disappearance() {
        let d = tempdir().unwrap();
        let admitted = d.path().join("admitted");
        fs::create_dir(&admitted).unwrap();
        let skipped = d.path().join("skipped");
        assert!(!skipped.exists());
        let check = |admitted: &PathBuf, omission: &AutomaticOmission| {
            recheck_admission_premise(
                std::slice::from_ref(admitted),
                std::slice::from_ref(omission),
            )
        };
        let absent = AutomaticOmission::NotFound(skipped.clone());

        // Stable premise: nothing changed.
        assert!(check(&admitted, &absent).is_none());

        // The skipped path reappears as a directory.
        fs::create_dir(&skipped).unwrap();
        assert!(check(&admitted, &absent).is_some());

        // A file-typed omission is undisturbed by the file surviving — and by
        // its later deletion, which is still no tree — but not by a
        // directory taking its place.
        let file = d.path().join("file-skip");
        fs::write(&file, b"x").unwrap();
        let nondir = AutomaticOmission::NonDirectory(file.clone());
        assert!(check(&admitted, &nondir).is_none());
        fs::remove_file(&file).unwrap();
        assert!(check(&admitted, &nondir).is_none());
        fs::create_dir(&file).unwrap();
        assert!(check(&admitted, &nondir).is_some());
        fs::remove_dir(&file).unwrap();

        // The admitted root disappears instead.
        fs::remove_dir(&skipped).unwrap();
        fs::remove_dir(&admitted).unwrap();
        let (path, _) = check(&admitted, &absent).expect("must flag");
        assert_eq!(path, admitted);
    }

    /// C028: `clean --full` consumes the fresh in-memory generation or
    /// refuses. A non-zero reconciliation and a missing generation both stay
    /// fatal; a failed save is the caller's warning, not this decision.
    #[test]
    fn full_cleanup_consumes_only_a_fresh_in_memory_generation() {
        let state = || crate::discovery_state::DiscoveryState {
            schema_version: crate::discovery_state::CURRENT_SCHEMA,
            last_full_at: Some(77),
            learned_roots: vec![crate::discovery_state::LearnedRoot {
                path: PathBuf::from("/work/a"),
                last_project_seen_at: 77,
            }],
            ..Default::default()
        };
        // No fresh generation, no cleanup — even with a zero exit code.
        assert!(full_cleanup_roots(0, None).is_err());
        // An incomplete reconciliation never authorizes stale roots.
        assert!(full_cleanup_roots(1, Some(state())).is_err());
        // The fresh generation yields automatic roots plus its timestamp.
        let (entries, generation) = full_cleanup_roots(0, Some(state())).unwrap();
        assert_eq!(
            entries,
            vec![(PathBuf::from("/work/a"), RootProvenance::Automatic)]
        );
        assert_eq!(generation, Some(77));
    }

    /// C028: cleanup candidates preserve a symlinked seed's own identity.
    /// Classifying the *canonicalized* set would resolve the link into its
    /// target and admit a real directory — the exact hiding the admission
    /// path must not do. The candidates therefore carry the link itself, and
    /// classification blocks it.
    #[test]
    #[cfg(unix)]
    fn cleanup_candidates_preserve_symlinked_seed_identity_for_admission() {
        let home = tempdir().unwrap();
        // "Developer" has no case-variant among the seed names, so this stays
        // exactly one candidate on case-insensitive macOS runners too
        // ("Projects" would also match "projects" there and block twice).
        let seed_name = "Developer";
        let seed = home.path().join(seed_name);
        fs::create_dir(&seed).unwrap();
        let target = home.path().join("real-target");
        fs::create_dir(&target).unwrap();
        // Replace the seed directory with a symlink to another directory.
        fs::remove_dir(&seed).unwrap();
        std::os::unix::fs::symlink(&target, &seed).unwrap();
        assert!(seed.is_dir(), "premise: is_dir follows the seed symlink");

        let (entries, _) =
            routine_cleanup_candidates(home.path(), 30, crate::discovery_state::StateLoad::Missing);
        assert!(
            entries.contains(&(seed.clone(), RootProvenance::Automatic)),
            "candidates must carry the link, not its target: {entries:?}"
        );
        let classified = classify_cleanup_roots(entries).unwrap();
        assert!(classified.admitted.is_empty());
        assert_eq!(classified.blocked.len(), 1);
        assert_eq!(classified.blocked[0].path, seed);
    }
}
