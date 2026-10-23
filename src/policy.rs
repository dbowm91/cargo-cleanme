use crate::{
    config::ScanConfig,
    domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope},
    error::AppError,
    traverse::path_is_symlink,
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
        if !m.is_dir() || m.file_type().is_symlink() || path_is_symlink(&root) {
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
/// Explicit roots are caller-selected (`scan.root`, `scan ROOT`,
/// `cargo cleanme ROOT`) and authoritative: any defect is fatal. Automatic roots (seed hints and
/// learned state) are search hints only: proven absence/non-directory omits
/// them from the current invocation, while uncertainty blocks mutation.
///
/// Operator roots are the roots the working directory contributes to the
/// maintenance scope (ADR 009). The operator chose them by standing there, so
/// the maintainability rule does not apply to them: that rule stops the tool
/// from *choosing* a transient origin on its own, and `cargo cleanme ROOT`
/// already permits the same subtree. Every other defect keeps the automatic root's
/// handling — a symlink blocks the scope, an absent or non-directory root is
/// omitted — because a scope that quietly reduced itself is worse than one
/// that stops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootProvenance {
    Explicit,
    Automatic,
    Operator,
}

/// A safely omittable automatic root: its absence or non-directory type was
/// positively established with `symlink_metadata`, so excluding it from this
/// invocation cannot conceal an expected scan tree.
///
/// The operator variants are the working directory's counterparts (ADR 009).
/// They are separate variants rather than reused `NotFound`/`NonDirectory`
/// because the cause is carried by the variant: the pre-spawn recheck reads
/// it to know what premise must still hold, and the diagnostic reads it to
/// name the root truthfully. An operator root was never a *learned* one, and
/// the omission vocabulary must not claim it was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutomaticOmission {
    NotFound(PathBuf),
    NonDirectory(PathBuf),
    NotMaintainable(PathBuf),
    /// An operator root that was absent when it was admitted.
    OperatorUnavailable(PathBuf),
    /// An operator root that was not a directory when it was admitted.
    OperatorNonDirectory(PathBuf),
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
                AutomaticOmission::NotFound(p)
                | AutomaticOmission::NonDirectory(p)
                | AutomaticOmission::NotMaintainable(p)
                | AutomaticOmission::OperatorUnavailable(p)
                | AutomaticOmission::OperatorNonDirectory(p) => p.clone(),
            })
            .collect()
    }

    /// Deterministic, bounded human diagnostics for safely omitted roots.
    ///
    /// Only omissions the operator can act on are reported. A root omitted
    /// because it sits at a known-transient origin — an agent worktree, a
    /// package cache, a temporary directory — is omitted *by design*: the
    /// tool would never have cleaned it, so its omission narrows nothing the
    /// operator was relying on and is not news. A durable root that has
    /// vanished, or one that is no longer a directory, is news, and is
    /// reported with a bounded sample of paths.
    pub fn omission_diagnostics(&self) -> Vec<String> {
        // Keyed by (reason, noun) so the learned-root vocabulary stays exactly
        // as it was, and an operator root's omission never claims to be a
        // learned one.
        let mut groups: std::collections::BTreeMap<(&str, &str), Vec<&PathBuf>> =
            std::collections::BTreeMap::new();
        for omission in &self.omitted {
            let (reason, noun, path) = match omission {
                AutomaticOmission::NotFound(path) => ("unavailable", "learned", path),
                AutomaticOmission::NonDirectory(path) => ("non-directory", "learned", path),
                // By design, not a diagnostic: the origin is transient and
                // would never have been cleaned. See the doc comment above.
                AutomaticOmission::NotMaintainable(_) => continue,
                AutomaticOmission::OperatorUnavailable(path) => ("unavailable", "operator", path),
                AutomaticOmission::OperatorNonDirectory(path) => {
                    ("non-directory", "operator", path)
                }
            };
            groups.entry((reason, noun)).or_default().push(path);
        }
        groups
            .into_iter()
            .map(|((reason, noun), mut paths)| {
                paths.sort();
                let examples = paths
                    .iter()
                    .take(3)
                    .map(|path| {
                        // The shared escaper, then the bound on the *rendered*
                        // text: a path of control characters must not be able
                        // to push the visible text past the bound by rendering
                        // shorter than it is.
                        crate::report::escape_path_text(&path.display().to_string())
                            .chars()
                            .take(200)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>();
                let sample = if examples.is_empty() {
                    String::new()
                } else {
                    format!("; examples: {}", examples.join(", "))
                };
                format!("omitting {} {reason} {noun} root(s){sample}", paths.len())
            })
            .collect()
    }
}

/// Classify cleanup root candidates by provenance without touching
/// canonicalization, so invalid identities cannot be hidden by collapse.
///
/// Uses `symlink_metadata` and the actual `io::ErrorKind`: `Path::exists` and
/// `Path::is_dir` conflate errors and follow symlinks. There is deliberately
/// no catch-all `Err(_) => continue`: every indeterminate failure blocks.
pub fn classify_cleanup_roots(entries: CleanupCandidates) -> Result<ClassifiedRoots, AppError> {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    classify_cleanup_roots_with_home(entries, home.as_deref())
}

fn classify_cleanup_roots_with_home(
    entries: CleanupCandidates,
    home: Option<&Path>,
) -> Result<ClassifiedRoots, AppError> {
    let mut out = ClassifiedRoots::default();
    for (path, provenance) in entries {
        match (fs::symlink_metadata(&path), provenance) {
            (Ok(meta), RootProvenance::Explicit) => {
                if !meta.is_dir() || meta.file_type().is_symlink() || path_is_symlink(&path) {
                    return Err(AppError::InvalidRoot {
                        path: path.display().to_string(),
                        reason: "expected a real directory".into(),
                    });
                }
                out.admitted.push(path);
            }
            (Ok(meta), RootProvenance::Automatic) => {
                // A trailing slash (`link/`) makes `symlink_metadata` follow
                // the link, so the stripped spelling is judged too.
                if meta.file_type().is_symlink() || path_is_symlink(&path) {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!(
                            "automatic root {} is a symlink; refusing to follow or omit",
                            path.display()
                        ),
                    });
                } else if meta.is_dir() {
                    if crate::discovery_state::automatic_root_is_maintainable(&path, home) {
                        out.admitted.push(path);
                    } else if let Err(error) = fs::read_dir(&path) {
                        out.blocked.push(AutomaticBlock {
                            path: path.clone(),
                            reason: format!(
                                "cannot establish coverage of non-maintainable automatic root {}: {error}",
                                path.display()
                            ),
                        });
                    } else {
                        out.omitted.push(AutomaticOmission::NotMaintainable(path));
                    }
                } else {
                    out.omitted.push(AutomaticOmission::NonDirectory(path));
                }
            }
            (Ok(meta), RootProvenance::Operator) => {
                // ADR 009: the operator chose this root by standing in it, so
                // the maintainability rule does not apply. Every other defect
                // keeps the automatic root's handling: a symlink blocks the
                // scope rather than being followed, and a non-directory root
                // is omitted.
                if meta.file_type().is_symlink() || path_is_symlink(&path) {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!(
                            "operator root {} is a symlink; refusing to follow or omit",
                            path.display()
                        ),
                    });
                } else if meta.is_dir() {
                    out.admitted.push(path);
                } else {
                    out.omitted
                        .push(AutomaticOmission::OperatorNonDirectory(path));
                }
            }
            (Err(e), RootProvenance::Explicit) => {
                return Err(AppError::InvalidRoot {
                    path: path.display().to_string(),
                    reason: e.to_string(),
                });
            }
            (Err(e), RootProvenance::Automatic) => {
                if e.kind() == std::io::ErrorKind::NotFound
                    && crate::discovery_state::automatic_root_is_maintainable(&path, home)
                {
                    out.omitted.push(AutomaticOmission::NotFound(path));
                } else if e.kind() == std::io::ErrorKind::NotFound {
                    // C033: typed provenance before availability. A missing
                    // path at a known-transient origin (deleted Codex
                    // worktree, emptied cache, removed node_modules subtree)
                    // is reported as what it is, not as merely
                    // "unavailable". Durable locations keep `NotFound`;
                    // symlinks never reach this arm (`symlink_metadata`
                    // succeeds for them, including broken links), and every
                    // other failure still blocks below.
                    out.omitted.push(AutomaticOmission::NotMaintainable(path));
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
            (Err(e), RootProvenance::Operator) => {
                // The operator root is present by construction — it is where
                // the process is running — so an absent one is an omission and
                // not a maintainability judgement. An indeterminate state
                // still blocks: the scope cannot be reduced silently.
                if e.kind() == std::io::ErrorKind::NotFound {
                    out.omitted
                        .push(AutomaticOmission::OperatorUnavailable(path));
                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!("cannot access operator root {}: {e}", path.display()),
                    });
                } else {
                    out.blocked.push(AutomaticBlock {
                        path: path.clone(),
                        reason: format!(
                            "indeterminate filesystem state for operator root {}: {e}",
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
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    for omission in omitted {
        let (path, must_stay_absent) = match omission {
            AutomaticOmission::NotFound(p) => (p, true),
            AutomaticOmission::NonDirectory(p) => (p, false),
            AutomaticOmission::NotMaintainable(p) => (p, false),
            // ADR 009: an operator root that was absent must stay absent, and
            // one that was not a directory must stay a non-directory — the
            // same premises the automatic roots carry.
            AutomaticOmission::OperatorUnavailable(p) => (p, true),
            AutomaticOmission::OperatorNonDirectory(p) => (p, false),
        };
        match fs::symlink_metadata(path) {
            Ok(meta) if matches!(omission, AutomaticOmission::NotMaintainable(_)) => {
                if crate::discovery_state::automatic_root_is_maintainable(path, home.as_deref()) {
                    return Some((
                        path.clone(),
                        format!(
                            "previously omitted root {} is now maintainable; ownership scope changed",
                            path.display()
                        ),
                    ));
                }
                if meta.file_type().is_symlink() {
                    return Some((
                        path.clone(),
                        format!("omitted root {} became a symlink", path.display()),
                    ));
                }
            }
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

/// The Full cleanup's proof verdict over the learned roots.
///
/// M018: the Full cleanup's proof boundary is the **learned root**, not the
/// platform root. The roots whose own traversal completed are cleanup
/// candidates; the roots whose traversal did not complete are reported with a
/// typed reason and are never candidates.
#[derive(Clone, Debug)]
pub struct FullCleanupVerdict {
    /// The learned roots that were proven, with their provenance.
    pub proven: CleanupCandidates,
    /// The learned roots whose coverage could not be proven.
    pub unproven: Vec<crate::domain::UnprovenRoot>,
    /// The state generation the roots came from.
    pub generation: Option<u64>,
}

impl FullCleanupVerdict {
    /// Every learned root, proven or not, is a cleanup **root**.
    ///
    /// M018: the engine must discover the unproven roots' workspaces. Withholding
    /// an unproven root from the cleanup scope would narrow the ownership
    /// universe: the other claimant of a shared output would never be seen, and
    /// those bytes would be promoted from `Shared` to `PrivateBounded` — the
    /// ADR 006 defect, and the forbidden shortcut.
    ///
    /// The unproven roots are removed from the **candidate** set, by the
    /// selection, not by withholding the root.
    pub fn admitted_roots(&self) -> CleanupCandidates {
        let mut roots: CleanupCandidates = self.proven.clone();
        roots.extend(
            self.unproven
                .iter()
                .map(|u| (u.root.clone(), RootProvenance::Automatic)),
        );
        roots
    }
}

/// Decide `cargo cleanme --full` roots from a finished Full reconciliation.
///
/// The roots come from the in-memory generation the traversal just proved,
/// never from a disk reload that could be an older generation when
/// publication failed, was unsupported, or was unavailable. A failed save
/// stays a stderr warning for the caller; only a missing fresh generation
/// refuses cleanup. Every returned root is [`RootProvenance::Automatic`] so
/// the shared admission path applies.
///
/// M018: a non-zero reconciliation code no longer refuses wholesale. The
/// platform walk's incompleteness is not evidence that *every* learned root
/// is unproven — it is evidence that the platform walk did not finish. The
/// per-root verdict is `coverage`, the verification pass that re-walks each
/// learned root as its own walk root; each root is then judged on its own
/// traversal.
///
/// `coverage` is `None` when the platform walk completed (every learned root
/// lies inside a traversal that read the whole platform, so every root is
/// proven by definition and no verification pass is needed). When the
/// platform walk was incomplete, `coverage` **must** carry the verification
/// result; a missing verdict fails closed and marks every root unproven,
/// because an unmeasured root is an unproven root.
pub fn full_cleanup_roots(
    reconciliation_code: i32,
    state: Option<crate::discovery_state::DiscoveryState>,
    coverage: Option<&[crate::domain::RootCoverage]>,
) -> Result<FullCleanupVerdict, AppError> {
    let Some(state) = state else {
        return Err(AppError::Config(
            "full reconciliation produced no fresh generation; \
             no cleanup commands were run against stale learned roots"
                .into(),
        ));
    };
    let generation = state.last_full_at;
    let mut proven = CleanupCandidates::new();
    let mut unproven = Vec::new();
    for learned in state.learned_roots {
        let canonical = fs::canonicalize(&learned.path).unwrap_or_else(|_| learned.path.clone());
        let verdict = if reconciliation_code == 0 {
            // The platform walk completed: it read every learned root's
            // subtree, so there is nothing left to measure.
            RootVerdict::Proven
        } else {
            match coverage.and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.canonical_root == canonical)
            }) {
                Some(entry) if entry.complete => RootVerdict::Proven,
                Some(entry) => RootVerdict::Unproven(entry.detail.clone()),
                None => RootVerdict::Unproven(None),
            }
        };
        match verdict {
            RootVerdict::Proven => proven.push((learned.path, RootProvenance::Automatic)),
            RootVerdict::Unproven(detail) => unproven.push(crate::domain::UnprovenRoot {
                root: learned.path,
                detail: detail.unwrap_or_else(|| {
                    "its coverage could not be measured; it may have disappeared or become \
                     unreadable since the reconciliation"
                        .into()
                }),
            }),
        }
    }
    Ok(FullCleanupVerdict {
        proven,
        unproven,
        generation,
    })
}

/// The per-root proof verdict the gate reaches.
enum RootVerdict {
    Proven,
    /// Unproven, with the coverage detail when there is one.
    Unproven(Option<String>),
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
                        crate::discovery_state::automatic_root_is_maintainable(&r.path, Some(home))
                            && (retention_days == 0
                                || now < r.last_project_seen_at
                                || now.saturating_sub(r.last_project_seen_at)
                                    <= u64::from(retention_days) * 86400)
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
///
/// The collapse itself is [`crate::traverse::collapse_outermost_protecting`] —
/// the one implementation the scan, display and cleanup surfaces share. What is
/// specific to this surface is the *protection*: a protected root survives
/// even when an ancestor contains it. The protection is compared by identity
/// too, so it holds for a root whose spelling differs from its physical
/// location.
fn canonical_dedup_roots_protecting(roots: Vec<PathBuf>, protected: &[PathBuf]) -> Vec<PathBuf> {
    // Dedup and collapse on the best available identity for each root. A root
    // that cannot be canonicalized previously kept its raw spelling, so it was
    // neither deduped nor collapsed against a parent root and an overlapping
    // subtree was scanned twice (L16). Each surviving root keeps its own
    // spelling, so "scan root is unavailable" still names what the user asked
    // for.
    let protected_identities: Vec<PathBuf> = protected
        .iter()
        .map(|root| {
            crate::traverse::lexical_identity(&crate::traverse::resolve_as_far_as_possible(root))
        })
        .collect();
    crate::traverse::collapse_outermost_protecting(roots, &protected_identities)
        .into_iter()
        .map(|root| root.spelling)
        .collect()
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("present");
        fs::create_dir(&real).unwrap();
        let missing = d.path().join("deleted-worktree");
        assert!(!missing.exists());
        let file = d.path().join("regular-file");
        fs::write(&file, b"not a directory").unwrap();

        // Missing + non-directory automatic roots omit; the real sibling is
        // admitted and no block is raised.
        let classified = classify_cleanup_roots_with_home(
            vec![
                (real.clone(), RootProvenance::Automatic),
                (missing.clone(), RootProvenance::Automatic),
                (file.clone(), RootProvenance::Automatic),
            ],
            Some(d.path()),
        )
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
        let d = tempfile::tempdir().unwrap();
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

    /// B1: a symlinked root with a trailing slash (`link/`) must classify
    /// exactly like without one: fatal for explicit roots, blocking for
    /// automatic roots. `symlink_metadata` follows the link when the slash
    /// is present, so the stripped spelling is judged too.
    #[test]
    #[cfg(unix)]
    fn trailing_slash_symlink_roots_classify_like_plain_symlinks() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = d.path().join("link-to-dir");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let slashed = PathBuf::from(format!("{}/", link.display()));
        let Err(crate::error::AppError::InvalidRoot { .. }) =
            classify_cleanup_roots(vec![(slashed.clone(), RootProvenance::Explicit)])
        else {
            panic!("explicit symlink with trailing slash must be fatal");
        };
        let classified =
            classify_cleanup_roots(vec![(slashed, RootProvenance::Automatic)]).unwrap();
        assert!(classified.admitted.is_empty());
        assert!(classified.omitted.is_empty());
        assert_eq!(classified.blocked.len(), 1);
        let with_dot = PathBuf::from(format!("{}/.", link.display()));
        assert!(path_is_symlink(&with_dot));
        let classified =
            classify_cleanup_roots(vec![(with_dot, RootProvenance::Automatic)]).unwrap();
        assert_eq!(classified.blocked.len(), 1);
    }

    /// C028 late premise recheck: a skipped root that reappears (or becomes
    /// unreadable) and an admitted root that disappears both invalidate the
    /// boundary the proof was built on.
    #[test]
    fn admission_premise_recheck_detects_reappearance_and_disappearance() {
        let d = tempfile::tempdir().unwrap();
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

    /// M018: the gate partitions the learned roots by their **own** coverage
    /// verdict. A root whose traversal completed is a candidate; a root whose
    /// traversal did not is reported with the reason and is never a
    /// candidate.
    ///
    /// A global verdict would put both roots on one side of the line, and the
    /// assertion below — one proven, one unproven — would fail.
    #[test]
    fn full_cleanup_gate_proves_the_roots_whose_own_traversal_completed() {
        let state = || crate::discovery_state::DiscoveryState {
            schema_version: crate::discovery_state::CURRENT_SCHEMA,
            last_full_at: Some(77),
            learned_roots: vec![
                crate::discovery_state::LearnedRoot {
                    path: PathBuf::from("/work/readable"),
                    last_project_seen_at: 77,
                },
                crate::discovery_state::LearnedRoot {
                    path: PathBuf::from("/work/unreadable"),
                    last_project_seen_at: 77,
                },
            ],
            ..Default::default()
        };
        let coverage = vec![
            crate::domain::RootCoverage {
                root: PathBuf::from("/work/readable"),
                canonical_root: PathBuf::from("/work/readable"),
                complete: true,
                error_events: 0,
                permission_denied: 0,
                vanished: 0,
                detail: None,
            },
            crate::domain::RootCoverage {
                root: PathBuf::from("/work/unreadable"),
                canonical_root: PathBuf::from("/work/unreadable"),
                complete: false,
                error_events: 3,
                permission_denied: 2,
                vanished: 0,
                detail: Some("2 permission-denied region(s) under /work/unreadable".into()),
            },
        ];
        let verdict = full_cleanup_roots(1, Some(state()), Some(&coverage))
            .expect("the gate returns a verdict");
        assert_eq!(
            verdict.proven,
            vec![(PathBuf::from("/work/readable"), RootProvenance::Automatic)],
            "only the root whose own traversal completed is a candidate"
        );
        assert_eq!(verdict.unproven.len(), 1);
        assert_eq!(verdict.unproven[0].root, PathBuf::from("/work/unreadable"));
        assert!(
            verdict.unproven[0].detail.contains("/work/unreadable"),
            "the unproven root carries its own reason: {:?}",
            verdict.unproven[0]
        );
        assert_eq!(verdict.generation, Some(77));
    }

    /// M018: a completed platform walk read every learned root's subtree, so
    /// every root is proven by definition and no verification pass is needed.
    #[test]
    fn full_cleanup_gate_proves_every_root_when_the_platform_walk_completed() {
        let state = || crate::discovery_state::DiscoveryState {
            schema_version: crate::discovery_state::CURRENT_SCHEMA,
            last_full_at: Some(77),
            learned_roots: vec![crate::discovery_state::LearnedRoot {
                path: PathBuf::from("/work/a"),
                last_project_seen_at: 77,
            }],
            ..Default::default()
        };
        // No coverage at all: the completed walk is the proof.
        let verdict =
            full_cleanup_roots(0, Some(state()), None).expect("the gate returns a verdict");
        assert_eq!(verdict.proven.len(), 1);
        assert!(verdict.unproven.is_empty());
    }

    /// M018: an incomplete walk whose verification pass measured nothing
    /// proves nothing. Every root is unproven, so the caller fails closed —
    /// the operation must never exit 0 with zero rows (the M012A silent
    /// no-op).
    #[test]
    fn full_cleanup_gate_proves_nothing_when_the_coverage_was_never_measured() {
        let state = || crate::discovery_state::DiscoveryState {
            schema_version: crate::discovery_state::CURRENT_SCHEMA,
            last_full_at: Some(77),
            learned_roots: vec![crate::discovery_state::LearnedRoot {
                path: PathBuf::from("/work/a"),
                last_project_seen_at: 77,
            }],
            ..Default::default()
        };
        let verdict =
            full_cleanup_roots(1, Some(state()), None).expect("the gate returns a verdict");
        assert!(
            verdict.proven.is_empty(),
            "an unmeasured root must never become a candidate"
        );
        assert_eq!(verdict.unproven.len(), 1);
        assert!(
            verdict.unproven[0].detail.contains("could not be measured"),
            "the reason must say the coverage was never measured: {:?}",
            verdict.unproven[0]
        );
    }

    /// C028 + M018: `cargo cleanme --full` consumes the fresh in-memory
    /// generation or refuses. A missing generation stays fatal; a failed save is the
    /// caller's warning, not this decision.
    ///
    /// M018: an incomplete reconciliation no longer refuses wholesale. It
    /// produces a *per-root* verdict, and a root whose coverage was not
    /// measured is unproven — never a candidate.
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
        assert!(full_cleanup_roots(0, None, None).is_err());
        // An incomplete reconciliation whose coverage was never measured: the
        // root's coverage is unknown, so it is unproven and not a candidate.
        // The old shape asserted `is_err()` on a literal `1`, which proved
        // only that a non-zero code was refused — never that a real traversal
        // produces one, which is how the defect survived.
        let verdict = full_cleanup_roots(1, Some(state()), None).unwrap();
        assert!(
            verdict.proven.is_empty(),
            "an unmeasured root must never become a cleanup candidate"
        );
        assert_eq!(verdict.unproven.len(), 1);
        assert_eq!(verdict.unproven[0].root, PathBuf::from("/work/a"));
        // The fresh generation yields automatic roots plus its timestamp.
        let verdict = full_cleanup_roots(0, Some(state()), None).unwrap();
        assert_eq!(
            verdict.proven,
            vec![(PathBuf::from("/work/a"), RootProvenance::Automatic)]
        );
        assert_eq!(verdict.generation, Some(77));
        assert!(verdict.unproven.is_empty());
    }

    /// C028: cleanup candidates preserve a symlinked seed's own identity.
    /// Classifying the *canonicalized* set would resolve the link into its
    /// target and admit a real directory — the exact hiding the admission
    /// path must not do. The candidates therefore carry the link itself, and
    /// classification blocks it.
    #[test]
    #[cfg(unix)]
    fn cleanup_candidates_preserve_symlinked_seed_identity_for_admission() {
        let home = tempfile::tempdir().unwrap();
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

    /// C033: typed provenance before availability. A missing path at a
    /// known-transient origin is `NotMaintainable`, not `NotFound`; a
    /// missing durable location stays `NotFound`. Neither blocks.
    #[test]
    fn missing_transient_roots_classify_as_non_maintainable_not_unavailable() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path();
        let missing_transient = home.join(".codex/worktrees/dead-session-id");
        let missing_cache = home.join("Library/Caches/Homebrew/cargo_cache");
        let missing_durable = home.join("Developer/vanished-project");
        for p in [&missing_transient, &missing_cache, &missing_durable] {
            assert!(!p.exists(), "premise: nothing created");
        }

        let classified = classify_cleanup_roots_with_home(
            vec![
                (missing_transient.clone(), RootProvenance::Automatic),
                (missing_cache.clone(), RootProvenance::Automatic),
                (missing_durable.clone(), RootProvenance::Automatic),
            ],
            Some(home),
        )
        .unwrap();
        assert!(classified.blocked.is_empty());
        assert!(classified.admitted.is_empty());
        assert!(
            classified
                .omitted
                .contains(&AutomaticOmission::NotMaintainable(missing_transient)),
            "deleted worktree must be non-maintainable: {:?}",
            classified.omitted
        );
        assert!(
            classified
                .omitted
                .contains(&AutomaticOmission::NotMaintainable(missing_cache)),
            "emptied platform cache must be non-maintainable: {:?}",
            classified.omitted
        );
        assert!(
            classified
                .omitted
                .contains(&AutomaticOmission::NotFound(missing_durable)),
            "deleted durable project stays unavailable: {:?}",
            classified.omitted
        );
    }

    #[test]
    fn omission_diagnostics_group_reasons_and_bound_path_samples() {
        let mut classified = ClassifiedRoots::default();
        for index in (0..100).rev() {
            classified
                .omitted
                .push(AutomaticOmission::NotFound(PathBuf::from(format!(
                    "/home/user/.codex/worktrees/{index}"
                ))));
        }
        classified
            .omitted
            .push(AutomaticOmission::NonDirectory(PathBuf::from(
                "/home/user/projects/file",
            )));

        let diagnostics = classified.omission_diagnostics();
        assert_eq!(
            diagnostics.len(),
            2,
            "each omission reason gets one summary"
        );
        assert!(diagnostics[0].starts_with("omitting 1 non-directory learned root(s)"));
        assert!(diagnostics[1].starts_with("omitting 100 unavailable learned root(s)"));
        assert!(diagnostics[1].contains("/home/user/.codex/worktrees/0"));
        assert!(diagnostics[1].contains("/home/user/.codex/worktrees/1"));
        assert!(diagnostics[1].contains("/home/user/.codex/worktrees/10"));
        assert!(!diagnostics[1].contains("/home/user/.codex/worktrees/11"));
        assert!(diagnostics.iter().all(|line| line.len() < 700));
    }

    /// A root omitted because it sits at a known-transient origin is omitted
    /// *by design*: the tool would never have cleaned it, so its omission
    /// narrows nothing the operator was relying on. Reporting it on every
    /// Routine invocation is noise that wears the tool's error prefix, and on
    /// a real machine it fires on essentially every run — a developer machine
    /// keeps agent worktrees, package caches and temp directories among its
    /// learned roots.
    ///
    /// A durable root that has *vanished* is different: there was a project
    /// there, and that is news the operator can act on. It stays reported.
    #[test]
    fn a_by_design_transient_omission_is_not_reported_but_a_vanished_root_is() {
        let mut classified = ClassifiedRoots::default();
        for index in 0..15 {
            classified
                .omitted
                .push(AutomaticOmission::NotMaintainable(PathBuf::from(format!(
                    "/home/user/.codex/worktrees/{index}"
                ))));
        }
        classified
            .omitted
            .push(AutomaticOmission::NotFound(PathBuf::from(
                "/home/user/projects/gone",
            )));

        let diagnostics = classified.omission_diagnostics();
        assert_eq!(
            diagnostics.len(),
            1,
            "the by-design transient bucket must not reach the operator: {diagnostics:?}"
        );
        assert!(
            diagnostics[0].starts_with("omitting 1 unavailable learned root(s)"),
            "a durable root that vanished is still news: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|line| line.contains("non-maintainable")),
            "the transient bucket must not reach the operator: {diagnostics:?}"
        );
    }

    // ADR 009 — operator provenance.
    // ---------------------------------------------------------------------------

    /// ADR 009: the operator's roots are exempt from the maintainability rule.
    ///
    /// The rule stops the tool from *choosing* a transient origin on its own. An
    /// operator standing in `~/.codex/worktrees` has chosen it, and
    /// `cargo cleanme ROOT` already permits the same subtree. The same path as an *automatic* root is
    /// still refused — the exemption is the provenance, not the path.
    #[test]
    fn operator_provenance_admits_what_the_maintainability_rule_refuses() {
        let home = tempfile::tempdir().unwrap();
        let forbidden = home.path().join(".codex/worktrees/agent");
        std::fs::create_dir_all(&forbidden).unwrap();

        // The home is supplied explicitly: `classify_cleanup_roots` reads the
        // process's real home, and the forbidden origins are named relative to
        // it. A test that used the real home would compare a temporary path
        // against the real `~/.codex/worktrees` and find nothing forbidden.
        let operator = classify_cleanup_roots_with_home(
            vec![(forbidden.clone(), RootProvenance::Operator)],
            Some(home.path()),
        )
        .unwrap();
        assert!(
            operator.admitted.contains(&forbidden),
            "the operator's root must be admitted: {:?}",
            operator.admitted
        );
        assert!(
            operator.blocked.is_empty(),
            "an operator root at a forbidden origin is not a block: {:?}",
            operator.blocked
        );

        let automatic = classify_cleanup_roots_with_home(
            vec![(forbidden.clone(), RootProvenance::Automatic)],
            Some(home.path()),
        )
        .unwrap();
        assert!(
            !automatic.admitted.contains(&forbidden),
            "the same path as an automatic root is still refused: {:?}",
            automatic.admitted
        );
        assert!(
            automatic
                .omitted
                .contains(&AutomaticOmission::NotMaintainable(forbidden.clone())),
            "the refusal keeps its reason: {:?}",
            automatic.omitted
        );
    }

    /// ADR 009: an operator root's omission is reported as an operator root's.
    ///
    /// The published "learned root(s)" wording belongs to the roots the tool
    /// chose. An operator root the operator chose is a different thing, and the
    /// diagnostic must not call it something it is not.
    #[test]
    fn an_operator_omission_is_not_reported_as_a_learned_root() {
        let mut classified = ClassifiedRoots::default();
        classified
            .omitted
            .push(AutomaticOmission::OperatorUnavailable(PathBuf::from(
                "/home/user/.codex/worktrees/gone",
            )));
        let diagnostics = classified.omission_diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].starts_with("omitting 1 unavailable operator root(s)"),
            "the operator's root must be named as the operator's: {diagnostics:?}"
        );
        assert!(
            !diagnostics[0].contains("learned"),
            "an operator root is not a learned root: {diagnostics:?}"
        );
    }

    /// ADR 009: the published learned-root wording is unchanged.
    ///
    /// C038's diagnostic is quoted in a closed closure record and in two
    /// integration assertions. Changing it here would make those records
    /// describe a message that no longer exists.
    #[test]
    fn the_learned_root_diagnostic_is_unchanged() {
        let mut classified = ClassifiedRoots::default();
        classified
            .omitted
            .push(AutomaticOmission::NotFound(PathBuf::from(
                "/home/user/gone",
            )));
        let diagnostics = classified.omission_diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].starts_with("omitting 1 unavailable learned root(s)"),
            "the learned-root wording is unchanged: {diagnostics:?}"
        );
    }

    /// ADR 009: an operator root's defect handling matches an automatic root's.
    ///
    /// A symlinked operator root blocks the scope rather than being followed:
    /// the scope may not quietly become smaller than the operator asked for.
    /// The exemption is from the maintainability rule only.
    ///
    /// Unix-gated for the reason every other symlink test in this module is:
    /// creating a symlink on Windows needs a privilege the CI runner does not
    /// have, so the test would be asserting against a path that is simply
    /// absent. The behaviour is the same on both platforms — it is the
    /// *creation* that is platform-specific, not the classification.
    #[test]
    #[cfg(unix)]
    fn an_operator_root_that_is_a_symlink_blocks_the_scope() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let classified =
            classify_cleanup_roots(vec![(link.clone(), RootProvenance::Operator)]).unwrap();
        assert!(
            classified.admitted.is_empty(),
            "a symlinked operator root is never followed: {:?}",
            classified.admitted
        );
        assert_eq!(
            classified.blocked.len(),
            1,
            "a symlinked operator root blocks the scope: {:?}",
            classified.blocked
        );
    }

    /// The policy surface's collapse is the shared collapse.
    ///
    /// *Fails if:* the policy surface keeps a copy of the collapse. The copy
    /// existed so a protected root could survive an ancestor; the protection
    /// was the only part specific to this surface, and it now rides on the
    /// shared mechanism. A copy would drift from it silently — the two would
    /// agree until one of them changed, and nothing would say which was
    /// right.
    #[test]
    fn the_policy_collapse_is_the_shared_collapse() {
        let d = tempfile::tempdir().unwrap();
        let outer = d.path().join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();
        let sibling = d.path().join("sibling");
        fs::create_dir_all(&sibling).unwrap();
        let roots = vec![outer.clone(), inner.clone(), sibling.clone(), outer.clone()];

        let shared = crate::traverse::collapse_outermost(roots.clone())
            .into_iter()
            .map(|root| root.spelling)
            .collect::<Vec<_>>();
        let policy = canonical_dedup_roots_protecting(roots, &[]);
        assert_eq!(
            policy, shared,
            "the policy collapse must be the shared one, with no protection"
        );
    }

    /// A protected root survives an ancestor that contains it.
    ///
    /// *Fails if:* the protection is applied by a separate mechanism that the
    /// shared collapse does not carry, or is dropped by the collapse. The
    /// protected root is the exception that keeps the ancestor's coverage
    /// from swallowing it.
    #[test]
    fn a_protected_root_survives_the_ancestor_that_contains_it() {
        let d = tempfile::tempdir().unwrap();
        let outer = d.path().join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();

        let collapsed = canonical_dedup_roots_protecting(
            vec![outer.clone(), inner.clone()],
            std::slice::from_ref(&inner),
        );
        assert!(
            collapsed.contains(&inner),
            "the protected root must survive: {collapsed:?}"
        );
        assert!(
            collapsed.contains(&outer),
            "the ancestor must still be there: {collapsed:?}"
        );
    }
}
