#![allow(clippy::collapsible_if)]
use crate::{domain::*, error::AppError, progress::ProgressObserver, traverse};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    collections::HashMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
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
    /// Whether a user rule names this directory **itself**.
    ///
    /// This is the only place globs are matched, so it also pins the
    /// repository's existing matching model: the lossy string form of the
    /// path, so a broad pattern such as `*` must still match when a name is not
    /// valid UTF-8 (Linux only). A pattern that spells the valid part still
    /// only matches what it spells.
    fn directly_ignored(&self, p: &Path) -> bool {
        if self.globs.is_empty() {
            return false;
        }
        self.globs.is_match(p.to_string_lossy().as_ref())
    }
    /// Whether `p`, or any directory above it, is named directly by an ignore
    /// rule — that is, whether `p` carries excluded state rather than merely
    /// matching nothing.
    ///
    /// `Path::ancestors` yields `p` first and walks up to the filesystem root,
    /// so a directory a rule names outright is covered here too. The empty
    /// component a relative path would contribute is skipped: it is not a
    /// directory, and a bare `*` would otherwise match it.
    fn inherits_ignore(&self, p: &Path) -> bool {
        p.ancestors()
            .any(|a| !a.as_os_str().is_empty() && self.directly_ignored(a))
    }
    /// The filter disposition of one directory, derived from the configured
    /// rules and the path alone.
    ///
    /// Inheriting excluded state is the whole of the C019 defect. A literal
    /// ignored ancestor does not textually match its children, so a stateless
    /// "does this exact path match a glob?" test loses the fact that
    /// `/archive/other` is still underneath `/archive`, and the sibling is
    /// walked. Testing the ancestry restores it.
    ///
    /// The cost is bounded by path depth and rule count — never by the size of
    /// the discovered tree — and it is paid on the directory that is about to be
    /// refused, so a broad ignored region still prunes before it is descended.
    fn disposition(&self, p: &Path) -> Disposition {
        // The exact unignore path and everything below it are in scope. Tested
        // first because it is the cheapest test and the only one that can
        // clear an inherited exclusion.
        if self.unignore.iter().any(|u| p.starts_with(u)) {
            return Disposition::ReIncluded;
        }
        // No rule names this directory or anything above it, so there is no
        // excluded state to inherit. This is also the whole explicit-scope
        // case, which arrives with an empty glob set.
        if !self.inherits_ignore(p) {
            return Disposition::Included;
        }
        // Excluded here. An exact unignore path at or below this directory means
        // the walk has to come in far enough to reach it — and no further.
        if self.unignore.iter().any(|u| u.starts_with(p)) {
            return Disposition::PassThrough;
        }
        Disposition::Pruned
    }
}
/// What the user ignore/unignore policy says about one directory.
///
/// The two excluded states are not interchangeable, because they lead to
/// opposite decisions about the walk:
///
/// - `Pruned` refuses the subtree;
/// - `PassThrough` is *still excluded*, but the walk must enter it to reach an
///   exact `unignore` path below. That permission is for the ancestry route to
///   the exception and does **not** extend to the directory's other children.
///
/// `ReIncluded` is the only state that clears excluded state, and it clears it
/// for the whole subtree rooted at the exact unignore path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Disposition {
    Included,
    Pruned,
    PassThrough,
    ReIncluded,
}
/// Whether an on-disk name is a Cargo manifest on this platform's volumes.
///
/// macOS and Windows volumes are case-insensitive by default, so a manifest
/// *stored* as `cargo.toml` is opened by Cargo exactly like `Cargo.toml` and
/// is a real, buildable project - matching it case-sensitively would silently
/// drop it from every report. Linux volumes are case-sensitive, where a
/// lowercase manifest is not a project Cargo can build, so it must not match.
fn is_manifest_name(name: &OsStr) -> bool {
    #[cfg(any(target_os = "macos", windows))]
    {
        name.to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("Cargo.toml"))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        name == OsStr::new("Cargo.toml")
    }
}
fn vcs(n: &OsStr) -> bool {
    n == OsStr::new(".git") || n == OsStr::new(".hg") || n == OsStr::new(".svn")
}
fn system_prune(_path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    if ["/proc", "/sys", "/dev", "/run"]
        .iter()
        .any(|excluded| _path == Path::new(excluded))
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    if _path == Path::new("/dev") {
        return true;
    }
    false
}

/// Effective rustup home when its identity is known and absolute.
pub fn effective_rustup_home() -> Option<PathBuf> {
    effective_rustup_home_from(
        env::var_os("RUSTUP_HOME"),
        directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()),
    )
}

fn effective_rustup_home_from(
    configured: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        let path = PathBuf::from(configured);
        return path.is_absolute().then_some(path);
    }
    home.map(|path| path.join(".rustup"))
}

/// Effective Cargo home when its path is known and absolute.
pub fn effective_cargo_home() -> Option<PathBuf> {
    if let Some(home) = env::var_os("CARGO_HOME") {
        let p = PathBuf::from(home);
        if p.is_absolute() {
            return Some(p);
        }
        return None;
    }
    directories::BaseDirs::new().map(|b| b.home_dir().join(".cargo"))
}

/// Registry/git source trees that must never trigger Cargo resolution.
pub fn cargo_home_prunes() -> Vec<PathBuf> {
    let Some(home) = effective_cargo_home() else {
        return Vec::new();
    };
    vec![home.join("registry"), home.join("git")]
}

fn is_cargo_home_pruned(path: &Path, prunes: &[PathBuf]) -> bool {
    prunes.iter().any(|p| path == *p || path.starts_with(p))
}

const MAX_DISCOVERY_PROFILE_WORKERS: usize = 32;
const GLOBAL_DISCOVERY_WORKER_CAP: usize = 8;

fn discovery_profile_workers(
    profiling: bool,
    requested: Option<&str>,
    default_workers: usize,
) -> usize {
    if !profiling {
        return default_workers;
    }
    requested
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|workers| *workers > 0)
        .map(|workers| workers.min(MAX_DISCOVERY_PROFILE_WORKERS))
        .unwrap_or(default_workers)
}

fn discovery_profile_skip_metadata(
    profiling: bool,
    requested: Option<&str>,
    default_skip_metadata: bool,
) -> bool {
    if !profiling {
        return default_skip_metadata;
    }
    match requested {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default_skip_metadata,
    }
}

fn discovery_profile_order(profiling: bool, requested: Option<&str>) -> dua_core::Order {
    if profiling && requested == Some("completion") {
        dua_core::Order::Completion
    } else {
        dua_core::Order::ParentFirst
    }
}

fn top_level_component<'a>(entry: &'a dua_core::Entry, root: &Path) -> Option<&'a OsStr> {
    if entry.depth == 0 {
        return None;
    }
    if entry.depth == 1 {
        return Some(&entry.file_name);
    }
    entry
        .parent_path
        .strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|component| component.as_os_str())
}

fn entry_is_within(entry: &dua_core::Entry, root: &Path, prefix: &Path) -> bool {
    if entry.depth == 0 {
        return root.starts_with(prefix);
    }
    let parent = entry.parent_path.as_ref();
    parent.starts_with(prefix)
        || (prefix.parent() == Some(parent)
            && prefix
                .file_name()
                .is_some_and(|component| component == entry.file_name))
}

fn entry_is_within_any(entry: &dua_core::Entry, root: &Path, prefixes: &[PathBuf]) -> bool {
    prefixes
        .iter()
        .any(|prefix| entry_is_within(entry, root, prefix))
}

/// Whether a *global-only* system prune covers this entry.
///
/// A walk root strictly inside a system prune re-opens that prune for its own
/// subtree: the platform policy listed both, and the more specific list wins.
/// Without this, macOS's `/usr` prune also swallows the `/usr/local` exception
/// root the same policy adds — `entry_is_within` matches on `parent_path`, so
/// every entry under `/usr/local` is "within" `/usr` and the subtree is never
/// walked. Only global-only prunes get the exemption: a Cargo or rustup home
/// stays refused even when a root sits inside it.
fn entry_is_system_pruned(entry: &dua_core::Entry, root: &Path, prefixes: &[PathBuf]) -> bool {
    prefixes.iter().any(|prefix| {
        !(root != prefix && root.starts_with(prefix)) && entry_is_within(entry, root, prefix)
    })
}

fn top_level_entry_map(
    roots: &[(usize, PathBuf)],
    root_counts: &HashMap<usize, u64>,
    counts: &HashMap<usize, HashMap<OsString, u64>>,
) -> std::collections::BTreeMap<PathBuf, u64> {
    let mut result = std::collections::BTreeMap::new();
    for (&root_idx, &count) in root_counts {
        if let Some((_, root)) = roots.get(root_idx) {
            *result.entry(root.clone()).or_insert(0) += count;
        }
    }
    for (&root_idx, components) in counts {
        let Some((_, root)) = roots.get(root_idx) else {
            continue;
        };
        for (component, &count) in components {
            *result.entry(root.join(component)).or_insert(0) += count;
        }
    }
    result
}

pub struct ManifestDiscovery {
    pub manifests: Vec<PathBuf>,
    pub visited_entries: u64,
    pub pruned_dirs: u64,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub counters: ScanCounters,
    /// Bounded top-level subtree attribution for `--stats` qualification.
    pub top_level_entries: std::collections::BTreeMap<PathBuf, u64>,
}

/// Manifest-first discovery: every plausible user `Cargo.toml` without
/// requiring a sibling `target/`. Cheap prunes run before manifest handling;
/// ignored/pruned paths never invoke Cargo.
pub fn discover_manifests(
    policy: &EffectiveScanPolicy,
    observer: &dyn ProgressObserver,
) -> Result<ManifestDiscovery, AppError> {
    discover_manifests_with_attribution(policy, observer, false)
}

pub fn discover_manifests_with_attribution(
    policy: &EffectiveScanPolicy,
    observer: &dyn ProgressObserver,
    attribution: bool,
) -> Result<ManifestDiscovery, AppError> {
    let global = matches!(policy.scope, ScanScope::Global(_) | ScanScope::Routine(_));
    let configured_global_policy =
        matches!(policy.scope, ScanScope::Global(_)).then(crate::policy::global_discovery_policy);
    let global_policy = match (&policy.scope, configured_global_policy) {
        (ScanScope::Global(roots), Some(candidate))
            if canonical_roots(roots) == canonical_roots(&candidate.roots) =>
        {
            Some(candidate)
        }
        _ => None,
    };
    let roots = match (&policy.scope, &global_policy) {
        (ScanScope::Explicit(p), _) => vec![p.clone()],
        (ScanScope::ExplicitRoots(roots), _) => roots.clone(),
        (ScanScope::Global(_), Some(p)) => p.roots.clone(),
        (ScanScope::Global(v), None) => v.clone(),
        (ScanScope::Routine(v), _) => v.clone(),
    };
    let (ignore, unignore) = match &policy.discovery_filters {
        DiscoveryFilters::Active { ignore, unignore } => (ignore.as_slice(), unignore.as_slice()),
        DiscoveryFilters::Bypassed => (&[][..], &[][..]),
    };
    let filters = Filters::new(ignore, unignore)?;
    let prunes = cargo_home_prunes();
    let rustup_prunes = if global {
        global_policy
            .as_ref()
            .map_or_else(Vec::new, |p| p.managed_tool_prunes.clone())
    } else {
        Vec::new()
    };
    let mut manifests = Vec::new();
    let mut diagnostics = Vec::new();
    let mut visited = 0u64;
    let mut pruned = 0u64;
    if global {
        if roots.is_empty() && matches!(policy.scope, ScanScope::Routine(_)) {
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Info,
                category: DiagnosticCategory::PlatformRoot,
                path: None,
                message:
                    "no Routine roots are available; run `scan --full` or scan an explicit root"
                        .into(),
            });
        }
        let profile_subtrees = env::var_os("CARGO_CLEANME_PROFILE_SUBTREES").is_some();
        let discovery_workers = discovery_profile_workers(
            profile_subtrees,
            env::var("CARGO_CLEANME_PROFILE_THREADS").ok().as_deref(),
            traverse::worker_threads().min(GLOBAL_DISCOVERY_WORKER_CAP),
        );
        let skip_metadata = discovery_profile_skip_metadata(
            profile_subtrees,
            env::var("CARGO_CLEANME_PROFILE_SKIP_METADATA")
                .ok()
                .as_deref(),
            cfg!(any(target_os = "linux", target_os = "macos")),
        );
        let order = discovery_profile_order(
            profile_subtrees,
            env::var("CARGO_CLEANME_PROFILE_ORDER").ok().as_deref(),
        );
        return discover_global_roots(
            &roots,
            &filters,
            &prunes,
            &rustup_prunes,
            global_policy
                .as_ref()
                .map_or(&[], |p| p.global_only_prunes.as_slice()),
            &mut manifests,
            &mut diagnostics,
            &mut visited,
            &mut pruned,
            observer,
            discovery_workers,
            profile_subtrees,
            profile_subtrees || attribution,
            skip_metadata,
            order,
        );
    }
    let explicit = matches!(
        policy.scope,
        ScanScope::Explicit(_) | ScanScope::ExplicitRoots(_)
    );
    let mut visited_dirs = 0u64;
    for root in roots {
        if !root_is_usable(&root, &mut diagnostics) {
            continue;
        }
        discover_manifests_root(
            &root,
            &filters,
            explicit,
            &prunes,
            &mut manifests,
            &mut diagnostics,
            &mut visited,
            &mut visited_dirs,
            &mut pruned,
            observer,
        )?;
    }
    manifests.sort();
    manifests.dedup();
    let counters = ScanCounters {
        directories_visited: visited_dirs,
        directories_pruned: pruned,
        manifests_found: manifests.len() as u64,
        ..Default::default()
    };
    Ok(ManifestDiscovery {
        manifests,
        visited_entries: visited,
        pruned_dirs: pruned,
        diagnostics,
        counters,
        top_level_entries: std::collections::BTreeMap::new(),
    })
}

fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<_> = roots
        .iter()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// A scan root must be an existing, real directory.
///
/// `Path::exists` and `Path::is_dir` follow symlinks, but the walker never does,
/// so a symlinked root has to be refused explicitly: it would otherwise report a
/// successful *empty* scan for a directory the user asked for (L7).
fn root_is_usable(root: &Path, diagnostics: &mut Vec<ScanDiagnostic>) -> bool {
    let problem = if fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        Some("scan root is a symlink; pass the real directory instead")
    } else if !root.is_dir() {
        Some("scan root is unavailable")
    } else {
        None
    };
    match problem {
        None => true,
        Some(message) => {
            diagnostics.push(ScanDiagnostic {
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::PlatformRoot,
                path: Some(root.to_path_buf()),
                message: message.into(),
            });
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn discover_global_roots(
    roots: &[PathBuf],
    filters: &Filters,
    cargo_prunes: &[PathBuf],
    rustup_prunes: &[PathBuf],
    system_prunes: &[PathBuf],
    manifests: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<ScanDiagnostic>,
    visited: &mut u64,
    pruned: &mut u64,
    observer: &dyn ProgressObserver,
    discovery_workers: usize,
    profile_subtrees: bool,
    collect_attribution: bool,
    skip_metadata: bool,
    order: dua_core::Order,
) -> Result<ManifestDiscovery, AppError> {
    let mut canonical_roots = Vec::new();
    for root in roots {
        if !root_is_usable(root, diagnostics) {
            continue;
        }
        canonical_roots.push((
            fs::canonicalize(root).unwrap_or_else(|_| root.clone()),
            root.clone(),
        ));
    }
    canonical_roots.sort_by(|a, b| a.0.cmp(&b.0));
    canonical_roots.dedup_by(|a, b| a.0 == b.0);
    // Use canonical identities for de-duplication while preserving each
    // selected root's spelling for ignore globs and diagnostics.
    let roots: Vec<_> = canonical_roots
        .into_iter()
        .map(|(_, logical)| logical)
        .collect();
    let roots: Vec<_> = roots.into_iter().enumerate().collect();
    let filters = Filters {
        globs: filters.globs.clone(),
        unignore: filters.unignore.clone(),
    };
    let cargo_prunes = cargo_prunes.to_vec();
    let rustup_prunes = rustup_prunes.to_vec();
    let system_prunes = system_prunes.to_vec();
    let roots_for_walk = roots.clone();
    let descend_filters = filters.clone();
    let descend_cargo_prunes = cargo_prunes.clone();
    let descend_rustup_prunes = rustup_prunes.clone();
    let descend_system_prunes = system_prunes.clone();
    let walk = dua_core::walk_roots(
        roots.iter().map(|(i, p)| (*i, p.clone())),
        discovery_workers,
        order,
        if skip_metadata {
            dua_core::Options::default().skip_metadata()
        } else {
            dua_core::Options::default()
        },
        move |root_idx, entry| {
            if !entry.file_type.is_dir() || entry.file_type.is_symlink() {
                return false;
            }
            let Some((_, root)) = roots_for_walk.get(root_idx) else {
                return false;
            };
            if entry.depth > 0 && (entry.file_name == "target" || vcs(&entry.file_name)) {
                return false;
            }
            if entry_is_system_pruned(entry, root, &descend_system_prunes)
                || entry_is_within_any(entry, root, &descend_cargo_prunes)
                || entry_is_within_any(entry, root, &descend_rustup_prunes)
            {
                return false;
            }
            if descend_filters.globs.is_empty() {
                return true;
            }
            let path = entry.path();
            // `PassThrough` descends, and so does `ReIncluded`; only `Pruned`
            // refuses. The internal prunes above are tested first, so a
            // `target`, `.git`, symlink, or Cargo/rustup home stays refused even
            // when it is also a `PassThrough` or `ReIncluded` directory.
            !matches!(descend_filters.disposition(&path), Disposition::Pruned)
        },
    );
    let mut system_count = 0;
    let mut cargo_count = 0;
    let mut rustup_count = 0;
    let mut target_count = 0;
    let mut ignore_count = 0;
    let mut batch_visited = 0;
    let mut batch_pruned = 0;
    let mut top_level_root_counts = collect_attribution.then(HashMap::new);
    let mut top_level_counts: Option<HashMap<usize, HashMap<OsString, u64>>> =
        collect_attribution.then(HashMap::new);
    let mut finished_roots = 0u64;
    let mut manifest_candidates = 0u64;
    let mut manifest_validated = 0u64;
    let mut manifest_validation_time = std::time::Duration::ZERO;
    let mut last_profile = std::time::Instant::now();
    let traversal_started = std::time::Instant::now();
    // Traversal errors arrive in worker-completion order, which is not
    // reproducible. They are buffered per root and flushed in root order so the
    // diagnostic stream is byte-identical across runs (L8).
    let mut walk_diagnostics: HashMap<usize, Vec<ScanDiagnostic>> = HashMap::new();
    let mut visited_dirs = 0u64;
    for (root_idx, event) in walk {
        let entry = match event {
            dua_core::RootEvent::Entry(Ok(e)) => e,
            dua_core::RootEvent::Entry(Err(e)) => {
                // dua-core reports the io error without the entry it applies
                // to, so the scan root is the finest honest attribution
                // available; a deeper path would be a guess. The OS error is
                // carried in the message so the cause stays actionable (L3).
                walk_diagnostics
                    .entry(root_idx)
                    .or_default()
                    .push(ScanDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        category: if e.kind() == std::io::ErrorKind::PermissionDenied {
                            DiagnosticCategory::PermissionDenied
                        } else {
                            DiagnosticCategory::Metadata
                        },
                        path: roots.get(root_idx).map(|(_, root)| root.clone()),
                        message: format!("filesystem traversal entry could not be read: {e}"),
                    });
                continue;
            }
            dua_core::RootEvent::Finished => {
                finished_roots += 1;
                continue;
            }
        };
        *visited = visited.saturating_add(1);
        if collect_attribution && let Some((_, root)) = roots.get(root_idx) {
            if entry.depth == 0 {
                *top_level_root_counts
                    .as_mut()
                    .unwrap()
                    .entry(root_idx)
                    .or_insert(0) += 1;
            } else if let Some(component) = top_level_component(&entry, root) {
                let subtrees = top_level_counts
                    .as_mut()
                    .unwrap()
                    .entry(root_idx)
                    .or_default();
                if let Some(count) = subtrees.get_mut(component) {
                    *count += 1;
                } else {
                    subtrees.insert(component.to_owned(), 1);
                }
            }
        }
        batch_visited += 1;
        if entry.file_type.is_dir() {
            // `directories_visited` counts directories only; `visited` keeps
            // counting every entry (files included) as the traversal total (L6).
            visited_dirs += 1;
        }
        if batch_visited >= 512 {
            observer.dirs_visited(batch_visited);
            batch_visited = 0;
        }
        if entry.file_type.is_dir() {
            let Some((_, root)) = roots.get(root_idx) else {
                continue;
            };
            let system = entry.depth > 0 && entry_is_system_pruned(&entry, root, &system_prunes);
            let cargo = entry.depth > 0 && entry_is_within_any(&entry, root, &cargo_prunes);
            let rustup = entry.depth > 0 && entry_is_within_any(&entry, root, &rustup_prunes);
            let target = entry.depth > 0 && (entry.file_name == "target" || vcs(&entry.file_name));
            // Counted only when the walk actually refused the subtree, so the
            // counter reports skips and nothing else. A `PassThrough` ancestor
            // was entered, and a `ReIncluded` directory is in scope, so neither
            // is a user-ignore prune; the inherited sibling that stays excluded
            // is. Unlike the other reasons this is counted at `depth == 0` as
            // well: a scan root that inherits exclusion from a literal ignored
            // ancestor is now refused outright, and an empty result that reports
            // zero prunes would be unexplainable (C019).
            let ignored = if !filters.globs.is_empty() {
                matches!(filters.disposition(&entry.path()), Disposition::Pruned)
            } else {
                false
            };
            if system {
                system_count += 1;
            } else if cargo {
                cargo_count += 1;
            } else if rustup {
                rustup_count += 1;
            } else if target {
                target_count += 1;
            } else if ignored {
                ignore_count += 1;
            }
            if system || cargo || rustup || target || ignored {
                *pruned += 1;
                batch_pruned += 1;
            }
        }
        if entry.file_type.is_file() && is_manifest_name(&entry.file_name) {
            manifest_candidates += 1;
            let path = entry.path();
            let validation_started = profile_subtrees.then(std::time::Instant::now);
            let valid = fs::symlink_metadata(&path)
                .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink());
            if let Some(started) = validation_started {
                manifest_validation_time += started.elapsed();
            }
            if valid {
                manifest_validated += 1;
                manifests.push(path);
                observer.manifests_found(1);
            }
        }
        if batch_pruned >= 64 {
            observer.dirs_pruned(batch_pruned);
            batch_pruned = 0;
        }
        if profile_subtrees && last_profile.elapsed() >= std::time::Duration::from_secs(5) {
            let top_level_entries = top_level_entry_map(
                &roots,
                top_level_root_counts.as_ref().unwrap(),
                top_level_counts.as_ref().unwrap(),
            );
            let profile = top_level_entries
                .iter()
                .map(|(path, count)| format!("{:?}={count}", path))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "scan subtree profile: workers={discovery_workers} skip_metadata={skip_metadata} order={} visited={} roots_finished={finished_roots} manifest_candidates={manifest_candidates} manifests_validated={manifest_validated} manifest_validation_seconds={:.3} {profile}",
                if matches!(order, dua_core::Order::Completion) {
                    "completion"
                } else {
                    "parent-first"
                },
                *visited,
                manifest_validation_time.as_secs_f64()
            );
            last_profile = std::time::Instant::now();
        }
    }
    if batch_visited > 0 {
        observer.dirs_visited(batch_visited);
    }
    if batch_pruned > 0 {
        observer.dirs_pruned(batch_pruned);
    }
    let mut buffered: Vec<(usize, ScanDiagnostic)> = walk_diagnostics
        .into_iter()
        .flat_map(|(idx, ds)| ds.into_iter().map(move |d| (idx, d)))
        .collect();
    buffered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.message.cmp(&b.1.message)));
    diagnostics.extend(buffered.into_iter().map(|(_, d)| d));
    if profile_subtrees {
        eprintln!(
            "scan phase: traversal_complete elapsed={:.3}s visited={} manifest_candidates={manifest_candidates} manifests_validated={manifest_validated} manifest_validation_seconds={:.3} diagnostics={}",
            traversal_started.elapsed().as_secs_f64(),
            *visited,
            manifest_validation_time.as_secs_f64(),
            diagnostics.len()
        );
    }
    let sort_started = profile_subtrees.then(std::time::Instant::now);
    manifests.sort();
    let sort_elapsed = sort_started.map_or(std::time::Duration::ZERO, |started| started.elapsed());
    let before_dedup = manifests.len();
    manifests.dedup();
    if profile_subtrees {
        eprintln!(
            "scan phase: manifest_sort_complete seconds={:.3} manifests_before_dedup={before_dedup} manifests_after_dedup={}",
            sort_elapsed.as_secs_f64(),
            manifests.len()
        );
    }
    let counters = ScanCounters {
        directories_visited: visited_dirs,
        directories_pruned: *pruned,
        platform_system_prunes: system_count,
        cargo_home_prunes: cargo_count,
        rustup_home_prunes: rustup_count,
        target_vcs_prunes: target_count,
        user_ignore_prunes: ignore_count,
        manifests_found: manifests.len() as u64,
        ..Default::default()
    };
    let attribution_started = collect_attribution.then(std::time::Instant::now);
    let top_level_entries = if collect_attribution {
        top_level_entry_map(
            &roots,
            top_level_root_counts.as_ref().unwrap(),
            top_level_counts.as_ref().unwrap(),
        )
    } else {
        std::collections::BTreeMap::new()
    };
    if profile_subtrees && let Some(started) = attribution_started {
        eprintln!(
            "scan phase: attribution_finalized seconds={:.3} paths={}",
            started.elapsed().as_secs_f64(),
            top_level_entries.len()
        );
    }
    Ok(ManifestDiscovery {
        manifests: manifests.clone(),
        visited_entries: *visited,
        pruned_dirs: *pruned,
        diagnostics: std::mem::take(diagnostics),
        counters,
        top_level_entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn discover_manifests_root(
    root: &Path,
    filters: &Filters,
    explicit: bool,
    prunes: &[PathBuf],
    manifests: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<ScanDiagnostic>,
    visited: &mut u64,
    visited_dirs: &mut u64,
    pruned: &mut u64,
    observer: &dyn ProgressObserver,
) -> Result<(), AppError> {
    let walker_root = root.to_path_buf();
    let root_path = walker_root.clone();
    let root_for_closure = root_path.clone();
    let descend_filters = Filters {
        globs: filters.globs.clone(),
        unignore: filters.unignore.clone(),
    };
    let descend_prunes = prunes.to_vec();
    let mut batch_visited = 0u64;
    let mut batch_pruned = 0u64;
    let mut walk = dua_core::walk(
        &walker_root,
        traverse::worker_threads(),
        dua_core::Order::ParentFirst,
        dua_core::Options::default(),
        move |entry| {
            let path = entry.path();
            if !entry.file_type.is_dir() {
                return false;
            }
            // Symlink directories are never followed (dua-core also refuses).
            if entry.file_type.is_symlink() {
                return false;
            }
            if path != root_for_closure && path.file_name().is_some_and(|n| n == "target" || vcs(n))
            {
                return false;
            }
            if system_prune(&path) {
                return false;
            }
            if is_cargo_home_pruned(&path, &descend_prunes) {
                return false;
            }
            explicit || !matches!(descend_filters.disposition(&path), Disposition::Pruned)
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
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category,
                    path: Some(root.to_path_buf()),
                    message: format!("filesystem traversal entry could not be read: {e}"),
                });
                continue;
            }
        };
        *visited = visited.saturating_add(1);
        if entry.file_type.is_dir() {
            // `directories_visited` counts directories only; `visited` keeps
            // counting every entry (files included) as the traversal total (L6).
            *visited_dirs = visited_dirs.saturating_add(1);
        }
        batch_visited += 1;
        if batch_visited >= 512 {
            observer.dirs_visited(batch_visited);
            batch_visited = 0;
        }
        let path = entry.path();
        // Count pruned dirs for instrumentation (target/VCS/system/cargo/ignored).
        // The ignore term is the same `Pruned` disposition the descend predicate
        // above uses, so a directory that was entered to reach an `unignore`
        // exception is not counted as a skip (C019).
        if entry.file_type.is_dir()
            && path != root_path
            && (path.file_name().is_some_and(|n| n == "target" || vcs(n))
                || system_prune(&path)
                || is_cargo_home_pruned(&path, prunes)
                || (!explicit && matches!(filters.disposition(&path), Disposition::Pruned)))
        {
            *pruned = pruned.saturating_add(1);
            batch_pruned += 1;
            if batch_pruned >= 64 {
                observer.dirs_pruned(batch_pruned);
                batch_pruned = 0;
            }
        }
        if entry.file_type.is_file() && path.file_name().is_some_and(is_manifest_name) {
            // Require a real file; symlinked manifests are ignored.
            if let Ok(meta) = fs::symlink_metadata(&path)
                && meta.is_file()
                && !meta.file_type().is_symlink()
            {
                manifests.push(path.clone());
                observer.manifests_found(1);
            }
        }
    }
    if batch_visited > 0 {
        observer.dirs_visited(batch_visited);
    }
    if batch_pruned > 0 {
        observer.dirs_pruned(batch_pruned);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::NoopObserver;
    use tempfile::tempdir;

    /// Collect the walk entries `dua_core` actually produces for `roots`, using
    /// the same `walk_roots` machinery production drives in
    /// [`discover_manifests`](super::discover_manifests).
    ///
    /// A test that hand-builds a `dua_core::Entry` literal can only do so on a
    /// platform where `Entry`'s fields are `std::fs` types. On macOS and Windows
    /// `dua_core` exports its own `FileType`/`Metadata`, so a literal written
    /// against `std::fs::FileType` compiles on Linux and fails on macOS
    /// (C023 WP-D) — a helper that is green on the lane that reviewed it and
    /// red on the lane that runs it. Walking a real tree instead means the
    /// entry under test is always the platform's own representation, produced by
    /// the same code path production uses.
    fn walk_entries(roots: &[(usize, PathBuf)]) -> Vec<(usize, dua_core::Entry)> {
        let mut collected = Vec::new();
        let walk = dua_core::walk_roots(
            roots.iter().cloned(),
            2,
            dua_core::Order::ParentFirst,
            dua_core::Options::default().skip_metadata(),
            |_, _| true,
        );
        for (root_idx, event) in walk {
            if let dua_core::RootEvent::Entry(Ok(entry)) = event {
                collected.push((root_idx, entry));
            }
        }
        collected
    }

    /// The single entry named `relative` seen while walking `root_idx`.
    ///
    /// Panics when the tree does not actually produce that entry, so a fixture
    /// that stops matching production enumeration fails loudly instead of
    /// quietly asserting nothing.
    fn entry_under<'a>(
        entries: &'a [(usize, dua_core::Entry)],
        root_idx: usize,
        relative: &str,
    ) -> &'a dua_core::Entry {
        entries
            .iter()
            .find(|(idx, entry)| *idx == root_idx && entry.path() == Path::new(relative))
            .map(|(_, entry)| entry)
            .unwrap_or_else(|| panic!("walk did not yield {relative} under root {root_idx}"))
    }

    #[test]
    fn a_root_inside_a_global_prune_reopens_that_prune() {
        // macOS lists both `/` and `/usr/local` as roots and `/usr` as a
        // global-only prune. Matched on `parent_path`, that prune also covers
        // every entry below `/usr/local`, so the exception root would never be
        // walked — the reason developer content under it stays invisible.
        //
        // The tree is a real one under a temp root rather than the literal
        // `/usr`, so the same premise is exercised identically on every lane;
        // the predicates compare prefixes, never absolute spellings.
        let d = tempdir().unwrap();
        let top = d.path();
        for relative in [
            "usr/System",
            "usr/local/bin",
            "usr/local/src/cargo",
            "usr/local/.git",
            "usr/local/cargo/registry/src/serde",
        ] {
            fs::create_dir_all(top.join(relative)).unwrap();
        }

        let prunes = vec![top.join("usr"), top.join("usr/System")];
        let roots = [(0, top.to_path_buf()), (1, top.join("usr/local"))];
        let entries = walk_entries(&roots);

        // From the top root, `usr` and `System` are still refused.
        assert!(entry_is_system_pruned(
            entry_under(&entries, 0, &top.join("usr").to_string_lossy()),
            top,
            &prunes
        ));
        assert!(entry_is_system_pruned(
            entry_under(&entries, 0, &top.join("usr/System").to_string_lossy()),
            top,
            &prunes
        ));

        // From the `/usr/local` root, `/usr` no longer applies at any depth,
        // and nothing else in the list does either.
        for relative in ["usr/local/bin", "usr/local/src/cargo", "usr/local/.git"] {
            assert!(
                !entry_is_system_pruned(
                    entry_under(&entries, 1, &top.join(relative).to_string_lossy()),
                    &top.join("usr/local"),
                    &prunes
                ),
                "{relative} under the /usr/local root must not be pruned by /usr"
            );
        }

        // The exemption is scoped to the root that re-opened the prune: the
        // same entry, walked from the top, is still refused.
        assert!(entry_is_system_pruned(
            entry_under(&entries, 0, &top.join("usr/local").to_string_lossy()),
            top,
            &prunes
        ));

        // Cargo and rustup homes get no such exemption: an explicitly rooted
        // subtree of one is still a registry cache that must not be walked.
        let cargo_prunes = vec![top.join("usr/local/cargo/registry")];
        assert!(entry_is_within_any(
            entry_under(
                &entries,
                1,
                &top.join("usr/local/cargo/registry/src/serde")
                    .to_string_lossy()
            ),
            &top.join("usr/local"),
            &cargo_prunes
        ));
    }

    /// The `/usr/local` exception root is walked by the *production* walk.
    ///
    /// [`a_root_inside_a_global_prune_reopens_that_prune`] is the shape of
    /// defect this commit describes: it asserts on `entry_is_system_pruned`
    /// over entries collected by a walk whose descend callback is `|_, _|
    /// true`, so it holds whether or not production ever consults the prune
    /// list. This one drives [`discover_global_roots`] itself — the production
    /// descend closure, the production prune counters, the production manifest
    /// validation — over the macOS root/prune shape with a **non-empty**
    /// `system_prunes`. No other test in this module passes a non-empty one.
    ///
    /// The tree sits under a temp root rather than the literal `/`, so the
    /// premise is exercised identically on every lane: production compares
    /// prefixes, never absolute spellings.
    #[test]
    fn global_walk_descends_into_a_root_that_reopens_a_system_prune() {
        let d = tempdir().unwrap();
        let top = d.path();
        // macOS lists `/` and `/usr/local` as roots and `/usr` as a global-only
        // prune. Matched on `parent_path`, that prune also covers every entry
        // below `/usr/local`, so without the exemption the exception root is
        // never walked and developer content under it stays invisible.
        let usr_local = top.join("usr/local");
        for relative in [
            // Developer content: must be found, from the `/usr/local` root.
            "usr/local/bin",
            "usr/local/src/cargo",
            // A Cargo home: the re-opened system prune must not re-open this.
            "usr/local/cargo/registry/src/serde",
            // The protected region: must stay refused.
            "usr/System/Library",
        ] {
            let dir = top.join(relative);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("Cargo.toml"), "").unwrap();
        }

        let found = discover_global_roots(
            &[top.to_path_buf(), usr_local.clone()],
            &Filters::new(&[], &[]).unwrap(),
            &[usr_local.join("cargo/registry")],
            &[],
            &[top.join("usr"), top.join("usr/System")],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut 0,
            &mut 0,
            &NoopObserver,
            traverse::worker_threads(),
            false,
            false,
            cfg!(target_os = "linux"),
            dua_core::Order::ParentFirst,
        )
        .unwrap();

        // The exact set, not a containment check: `usr/System` and the registry
        // appearing here would mean the prune list is not holding.
        assert_eq!(
            found.manifests,
            vec![
                usr_local.join("bin/Cargo.toml"),
                usr_local.join("src/cargo/Cargo.toml"),
            ],
            "a walk root inside a system prune must be walked; /usr/System and \
             the Cargo registry must stay refused"
        );
        // The reporting half travels with the walk: the re-opened prune must not
        // inflate the system-prune tally, or `--stats`/JSON claims directories
        // were skipped that were in fact visited.
        assert_eq!(
            found.counters.platform_system_prunes, 1,
            "only `usr` itself is refused from the top root"
        );
        assert_eq!(found.counters.cargo_home_prunes, 1);
        assert_eq!(found.counters.directories_pruned, 2);
    }

    #[test]
    fn rustup_home_uses_absolute_override_or_platform_home_only() {
        let home = PathBuf::from("/home/tester");
        let override_home = std::env::current_dir().unwrap().join("tool").join("rustup");
        assert_eq!(
            effective_rustup_home_from(None, Some(home.clone())),
            Some(home.join(".rustup"))
        );
        assert_eq!(
            effective_rustup_home_from(
                Some(override_home.as_os_str().to_owned()),
                Some(home.clone())
            ),
            Some(override_home)
        );
        assert_eq!(
            effective_rustup_home_from(Some("relative/rustup".into()), Some(home)),
            None
        );
        assert_eq!(effective_rustup_home_from(None, None), None);
    }

    #[test]
    fn global_rustup_prune_keeps_adjacent_user_project_reachable() {
        let d = tempdir().unwrap();
        let root = d.path();
        let rustup = root.join("managed/rustup");
        let project = root.join("managed/project");
        fs::create_dir_all(&rustup).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(rustup.join("Cargo.toml"), "").unwrap();
        fs::write(project.join("Cargo.toml"), "").unwrap();
        let found = discover_global_roots(
            &[root.to_path_buf()],
            &Filters::new(&[], &[]).unwrap(),
            &[],
            &[rustup],
            &[],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut 0,
            &mut 0,
            &NoopObserver,
            traverse::worker_threads(),
            false,
            false,
            cfg!(target_os = "linux"),
            dua_core::Order::ParentFirst,
        )
        .unwrap();
        assert_eq!(found.manifests, vec![project.join("Cargo.toml")]);
        assert_eq!(found.counters.rustup_home_prunes, 1);
        assert_eq!(
            found.counters.directories_pruned,
            found.counters.platform_system_prunes
                + found.counters.cargo_home_prunes
                + found.counters.rustup_home_prunes
                + found.counters.target_vcs_prunes
                + found.counters.user_ignore_prunes
        );

        let explicit = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Explicit(rustup_root_for_test(root)),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let explicit_found = discover_manifests(&explicit, &NoopObserver).unwrap();
        assert_eq!(
            explicit_found.manifests.len(),
            1,
            "explicit scopes are authoritative"
        );
    }

    fn rustup_root_for_test(root: &Path) -> PathBuf {
        root.join("managed/rustup")
    }

    #[test]
    fn global_multi_root_walk_deduplicates_equivalent_roots_and_manifests() {
        let d = tempdir().unwrap();
        let root = d.path();
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "").unwrap();
        let found = discover_global_roots(
            &[root.to_path_buf(), root.to_path_buf()],
            &Filters::new(&[], &[]).unwrap(),
            &[],
            &[],
            &[],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut 0,
            &mut 0,
            &NoopObserver,
            traverse::worker_threads(),
            false,
            false,
            cfg!(target_os = "linux"),
            dua_core::Order::ParentFirst,
        )
        .unwrap();
        assert_eq!(found.manifests, vec![project.join("Cargo.toml")]);
    }

    #[test]
    fn profiling_worker_override_is_opt_in_and_bounded() {
        assert_eq!(discovery_profile_workers(false, Some("2"), 4), 4);
        assert_eq!(discovery_profile_workers(true, Some("2"), 6), 2);
        assert_eq!(discovery_profile_workers(true, Some("64"), 6), 32);
        assert_eq!(discovery_profile_workers(true, Some("0"), 6), 6);
        assert_eq!(discovery_profile_workers(true, Some("invalid"), 6), 6);
        assert!(!discovery_profile_skip_metadata(false, Some("1"), false));
        assert!(discovery_profile_skip_metadata(true, Some("1"), false));
        assert!(!discovery_profile_skip_metadata(true, Some("0"), true));
        assert!(matches!(
            discovery_profile_order(false, Some("completion")),
            dua_core::Order::ParentFirst
        ));
        assert!(matches!(
            discovery_profile_order(true, Some("completion")),
            dua_core::Order::Completion
        ));
    }

    #[test]
    fn synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps() {
        let d = tempdir().unwrap();
        let root = d.path().join("tree");
        let mut expected = Vec::new();
        for branch in 0..24 {
            for leaf in 0..12 {
                let project = root
                    .join(format!("branch-{branch:02}"))
                    .join(format!("leaf-{leaf:02}"));
                fs::create_dir_all(&project).unwrap();
                let manifest = project.join("Cargo.toml");
                fs::write(&manifest, "[package]\nname='fixture'\nversion='0.1.0'\n").unwrap();
                fs::write(project.join("payload.dat"), [0u8; 64]).unwrap();
                expected.push(manifest);
            }
        }
        let deep = root.join("deep/a/b/c/d/e/f/g/h/i/j");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Cargo.toml"), "[workspace]\n").unwrap();
        expected.push(deep.join("Cargo.toml"));
        let hidden = root.join(".hidden-project");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("Cargo.toml"), "[workspace]\n").unwrap();
        expected.push(hidden.join("Cargo.toml"));
        let archive_keep = root.join("archive/keep");
        fs::create_dir_all(&archive_keep).unwrap();
        fs::write(archive_keep.join("Cargo.toml"), "[workspace]\n").unwrap();
        expected.push(archive_keep.join("Cargo.toml"));
        let archive_drop = root.join("archive/drop");
        fs::create_dir_all(&archive_drop).unwrap();
        fs::write(archive_drop.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::create_dir_all(root.join("branch-00/leaf-00/target/nested")).unwrap();
        fs::write(
            root.join("branch-00/leaf-00/target/nested/Cargo.toml"),
            "[workspace]\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            let outside = d.path().join("outside");
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("Cargo.toml"), "[workspace]\n").unwrap();
            std::os::unix::fs::symlink(&outside, root.join("linked-outside")).unwrap();
        }
        expected.sort();
        let filters = Filters::new(
            &[format!("{}/archive/*", root.display())],
            std::slice::from_ref(&archive_keep),
        )
        .unwrap();

        for workers in [1, 2, 4, 8, 16, 32] {
            for skip_metadata in [false, true] {
                for order in [dua_core::Order::ParentFirst, dua_core::Order::Completion] {
                    let started = std::time::Instant::now();
                    let found = discover_global_roots(
                        std::slice::from_ref(&root),
                        &filters,
                        &[],
                        &[],
                        &[],
                        &mut Vec::new(),
                        &mut Vec::new(),
                        &mut 0,
                        &mut 0,
                        &NoopObserver,
                        workers,
                        false,
                        true,
                        skip_metadata,
                        order,
                    )
                    .unwrap();
                    eprintln!(
                        "synthetic global discovery: workers={workers} skip_metadata={skip_metadata} order={} elapsed={:.3}s entries={} manifests={}",
                        if matches!(order, dua_core::Order::Completion) {
                            "completion"
                        } else {
                            "parent-first"
                        },
                        started.elapsed().as_secs_f64(),
                        found.visited_entries,
                        found.manifests.len()
                    );
                    assert_eq!(
                        found.manifests, expected,
                        "workers={workers} skip_metadata={skip_metadata}"
                    );
                    assert_eq!(found.counters.manifests_found, expected.len() as u64);
                    assert_eq!(
                        found.top_level_entries.values().sum::<u64>(),
                        found.visited_entries,
                        "top-level attribution must account for every visited entry"
                    );
                }
            }
        }
    }
    /// Explicit-root discovery, as `scan <dir>` runs it. An explicit root is
    /// scanned exactly as asked: `scan.ignore` deliberately does not apply.
    fn explicit_manifests(root: &Path) -> ManifestDiscovery {
        discover_manifests(
            &EffectiveScanPolicy {
                recency: std::time::Duration::from_secs(300),
                scope: ScanScope::Explicit(root.to_path_buf()),
                discovery_filters: DiscoveryFilters::Bypassed,
            },
            &NoopObserver,
        )
        .unwrap()
    }

    /// Materialise `Cargo.toml` at each of `projects` under `root`.
    ///
    /// Discovery only requires the manifest to be a real regular file, so an
    /// empty one is enough and keeps these fixtures about scope selection rather
    /// than about workspace resolution.
    fn project_fixtures(root: &Path, projects: &[&str]) {
        for name in projects {
            let dir = root.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("Cargo.toml"), "").unwrap();
        }
    }

    /// Routine-scope discovery, as `scan --config` runs it: the mode that
    /// consults `scan.ignore` against canonical walk paths (M2/H1).
    fn routine_manifests(
        roots: Vec<PathBuf>,
        ignore: &[String],
        unignore: &[PathBuf],
    ) -> ManifestDiscovery {
        discover_manifests(
            &EffectiveScanPolicy {
                recency: std::time::Duration::from_secs(300),
                scope: ScanScope::Routine(roots),
                discovery_filters: DiscoveryFilters::Active {
                    ignore: ignore.to_vec(),
                    unignore: unignore.to_vec(),
                },
            },
            &NoopObserver,
        )
        .unwrap()
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn manifest_name_is_matched_case_insensitively_where_the_volume_is() {
        // H2: on a case-insensitive volume a project may be stored as
        // `cargo.toml`. Cargo opens it, so the tool must find it too.
        let d = tempdir().unwrap();
        let p = d.path().join("lowercase");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("cargo.toml"), "").unwrap();
        let canonical = fs::canonicalize(d.path()).unwrap();
        let found = routine_manifests(vec![canonical.clone()], &[], &[]);
        assert_eq!(
            found.manifests,
            vec![canonical.join("lowercase/cargo.toml")],
            "lowercase manifest must be discovered"
        );
    }

    // Only Linux (and other Unix filesystems with raw byte names) can hold a
    // non-UTF-8 name; APFS/NTFS reject one.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_directory_still_matches_a_broad_ignore_pattern() {
        // L4: a non-UTF-8 name used to bypass every ignore glob, so `*` did not
        // prune it and the walker descended into a directory the user excluded.
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let d = tempdir().unwrap();
        let p = d.path().join("keep");
        let odd = p.join(OsString::from_vec(b"weird-\xff".to_vec()));
        fs::create_dir_all(&odd).unwrap();
        fs::write(odd.join("Cargo.toml"), "").unwrap();
        let canonical = fs::canonicalize(&p).unwrap();
        let filters = Filters::new(&[format!("{}/*", canonical.display())], &[]).unwrap();
        assert!(
            filters.directly_ignored(&odd),
            "a broad pattern must match a non-UTF-8 name"
        );
        assert_eq!(
            filters.disposition(&odd),
            Disposition::Pruned,
            "a directly matched name with no exception below it is refused"
        );
        // The finder itself is unchanged for valid paths.
        assert!(!filters.directly_ignored(&canonical));
    }

    #[test]
    fn directories_visited_counts_directories_not_files() {
        // L6: the counter claimed directories but tallied every entry.
        let d = tempdir().unwrap();
        let p = d.path().join("p");
        fs::create_dir_all(p.join("sub/deeper")).unwrap();
        for file in ["a", "sub/b", "sub/deeper/c"] {
            fs::write(p.join(file), "").unwrap();
        }
        fs::write(p.join("Cargo.toml"), "").unwrap();
        let found = explicit_manifests(d.path());
        assert_eq!(found.manifests, vec![p.join("Cargo.toml")]);
        // d.path(), p, p/sub, p/sub/deeper = 4 directories; entries = 8.
        assert_eq!(found.counters.directories_visited, 4);
        assert_eq!(found.visited_entries, 8);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_scan_root_is_refused_instead_of_scanning_nothing() {
        // L7: `is_dir` follows symlinks, so a symlinked root used to produce a
        // successful *empty* scan.
        use std::os::unix::fs::symlink;
        let d = tempdir().unwrap();
        let real = d.path().join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("Cargo.toml"), "").unwrap();
        let link = d.path().join("link");
        symlink(&real, &link).unwrap();
        let mut diagnostics = Vec::new();
        let found = discover_manifests(
            &EffectiveScanPolicy {
                recency: std::time::Duration::from_secs(300),
                scope: ScanScope::Explicit(link.clone()),
                discovery_filters: DiscoveryFilters::Bypassed,
            },
            &NoopObserver,
        )
        .unwrap();
        diagnostics.extend(found.diagnostics);
        assert!(found.manifests.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("symlink") && d.severity == DiagnosticSeverity::Error),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn discovers_real_manifest_and_prunes_target() {
        let d = tempdir().unwrap();
        let p = d.path().join("p");
        fs::create_dir_all(p.join("target/nested")).unwrap();
        fs::write(p.join("Cargo.toml"), "").unwrap();
        fs::write(p.join("target/nested/Cargo.toml"), "").unwrap();
        let found = explicit_manifests(d.path());
        // Only the real manifest: a nested manifest inside `target/` is pruned.
        assert_eq!(found.manifests, vec![p.join("Cargo.toml")]);
        assert_eq!(
            found.visited_entries, 4,
            "target descendants must be pruned during discovery"
        );
    }

    #[test]
    fn global_attribution_is_unallocated_when_not_requested() {
        let d = tempdir().unwrap();
        let project = d.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "").unwrap();
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Global(vec![d.path().to_path_buf()]),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let ordinary = discover_manifests_with_attribution(&policy, &NoopObserver, false).unwrap();
        let qualified = discover_manifests_with_attribution(&policy, &NoopObserver, true).unwrap();
        assert!(ordinary.top_level_entries.is_empty());
        assert_eq!(ordinary.manifests, qualified.manifests);
        assert_eq!(ordinary.counters, qualified.counters);
        assert!(!qualified.top_level_entries.is_empty());
    }
    #[test]
    fn manifest_discovery_finds_project_without_target() {
        let d = tempdir().unwrap();
        let p = d.path().join("proj");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("Cargo.toml"), "").unwrap();
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Explicit(d.path().to_path_buf()),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let noop = NoopObserver;
        let found = discover_manifests(&policy, &noop).unwrap();
        assert_eq!(found.manifests.len(), 1);
        assert_eq!(found.manifests[0], p.join("Cargo.toml"));
    }
    #[test]
    fn manifest_discovery_prunes_target_children() {
        let d = tempdir().unwrap();
        let p = d.path().join("p");
        fs::create_dir_all(p.join("target/nested")).unwrap();
        fs::write(p.join("Cargo.toml"), "").unwrap();
        fs::write(p.join("target/nested/Cargo.toml"), "").unwrap();
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Explicit(d.path().to_path_buf()),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let noop = NoopObserver;
        let found = discover_manifests(&policy, &noop).unwrap();
        assert_eq!(
            found.manifests.len(),
            1,
            "nested target manifest must be pruned"
        );
    }
    #[test]
    fn manifest_discovery_prunes_cargo_home_registry() {
        let d = tempdir().unwrap();
        let cargo_home = d.path().join("cargo-home");
        fs::create_dir_all(cargo_home.join("registry/src/pkg")).unwrap();
        fs::write(cargo_home.join("registry/src/pkg/Cargo.toml"), "").unwrap();
        let proj = d.path().join("proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("Cargo.toml"), "").unwrap();
        // Point CARGO_HOME at the temp home for this test.
        let prev = std::env::var_os("CARGO_HOME");
        unsafe { std::env::set_var("CARGO_HOME", &cargo_home) };
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Explicit(d.path().to_path_buf()),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let noop = NoopObserver;
        let found = discover_manifests(&policy, &noop).unwrap();
        if let Some(v) = prev {
            unsafe { std::env::set_var("CARGO_HOME", v) };
        } else {
            unsafe { std::env::remove_var("CARGO_HOME") };
        }
        assert_eq!(found.manifests.len(), 1);
        assert_eq!(found.manifests[0], proj.join("Cargo.toml"));
    }
    #[test]
    fn ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned() {
        // Fail-fast instrumentation: ignored trees contribute pruned counts and
        // no manifests, so no Cargo subprocess would be invoked for them.
        let d = tempdir().unwrap();
        let archive = d.path().join("archive");
        let skip = archive.join("skip");
        fs::create_dir_all(&skip).unwrap();
        fs::write(skip.join("Cargo.toml"), "").unwrap();
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Global(vec![d.path().to_path_buf()]),
            discovery_filters: DiscoveryFilters::Active {
                ignore: vec![format!("{}/*", archive.display())],
                unignore: vec![],
            },
        };
        let noop = NoopObserver;
        let found = discover_manifests(&policy, &noop).unwrap();
        assert!(found.manifests.is_empty());
        assert!(found.counters.directories_pruned > 0 || found.pruned_dirs > 0);
        assert_eq!(
            found.counters.user_ignore_prunes,
            found.counters.directories_pruned
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
        // Global and routine scans walk canonical roots, so ignore patterns are
        // matched against canonical paths. `config::load` normalizes a pattern's
        // literal prefix (M2/H1); the walker itself compares canonical spellings.
        let canonical = fs::canonicalize(d.path()).unwrap();
        let canonical_archive = canonical.join("archive");
        let found = routine_manifests(
            vec![canonical.clone()],
            &[format!("{}/*", canonical_archive.display())],
            &[canonical_archive.join("keep")],
        );
        assert_eq!(
            found.manifests,
            vec![canonical_archive.join("keep/Cargo.toml")]
        );
        // Without the exception the whole archive is pruned.
        let pruned = routine_manifests(
            vec![canonical],
            &[format!("{}/*", canonical_archive.display())],
            &[],
        );
        assert!(pruned.manifests.is_empty());
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
        // Manifest-first discovery still finds the manifest; eligibility is
        // decided later by output grouping (symlink -> Uncertain).
        let policy = EffectiveScanPolicy {
            recency: std::time::Duration::from_secs(300),
            scope: ScanScope::Explicit(d.path().to_path_buf()),
            discovery_filters: DiscoveryFilters::Bypassed,
        };
        let noop = NoopObserver;
        let found = discover_manifests(&policy, &noop).unwrap();
        assert_eq!(found.manifests.len(), 1);
    }
    #[test]
    fn filters_ignored_parent_keeps_exception_route() {
        // The historical `/*` shape, asserted through the disposition states
        // that replaced the old `ignored`/`exception_below` pair. The parent is
        // not itself matched, so it is simply included; `old` is matched
        // directly and refused; `keep` is the exception and everything under it
        // is re-included.
        let f = Filters::new(
            &["/tmp/archive/*".into()],
            &[PathBuf::from("/tmp/archive/keep")],
        )
        .unwrap();
        assert_eq!(
            f.disposition(Path::new("/tmp/archive")),
            Disposition::Included
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/old")),
            Disposition::Pruned
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/keep")),
            Disposition::ReIncluded
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/keep/child")),
            Disposition::ReIncluded
        );
    }

    #[test]
    fn literal_ignored_ancestor_becomes_pass_through_and_not_inherited_by_siblings() {
        // C019's defect, at the level of the state model. A literal ignore rule
        // does not textually match the children of the directory it names, so
        // the ancestry of `/archive` has to carry the excluded state down to
        // `other` while still letting the walk in to reach `keep`.
        let f = Filters::new(
            &["/tmp/archive".into()],
            &[
                PathBuf::from("/tmp/archive/keep"),
                PathBuf::from("/tmp/archive/nested/keep-b"),
            ],
        )
        .unwrap();
        assert_eq!(
            f.disposition(Path::new("/tmp/archive")),
            Disposition::PassThrough,
            "the ignored ancestor is entered only as the route to an exception"
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/other")),
            Disposition::Pruned,
            "an unrelated sibling inherits the exclusion"
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/keep")),
            Disposition::ReIncluded
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/nested")),
            Disposition::PassThrough,
            "intermediate ancestry for a nested exception is entered"
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/nested/other")),
            Disposition::Pruned,
            "a sibling below that intermediate level is still excluded"
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/nested/keep-b")),
            Disposition::ReIncluded
        );
        // A sibling name that merely shares a string prefix is not an ancestor.
        assert_eq!(
            f.disposition(Path::new("/tmp/archive-old")),
            Disposition::Included,
            "`/tmp/archive-old` is not under `/tmp/archive`"
        );
    }

    #[test]
    fn disposition_of_a_recursive_ignore_pattern_is_still_pass_through() {
        // The documented `/**` workaround keeps working, and it reaches the
        // exception through the same pass-through state rather than a special
        // case: `**` names the base directory itself.
        let f = Filters::new(
            &["/tmp/archive/**".into()],
            &[PathBuf::from("/tmp/archive/keep")],
        )
        .unwrap();
        assert_ne!(
            f.disposition(Path::new("/tmp/archive")),
            Disposition::Pruned,
            "the base directory must not be refused, or the exception is unreachable"
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/other")),
            Disposition::Pruned
        );
        assert_eq!(
            f.disposition(Path::new("/tmp/archive/keep/deep")),
            Disposition::ReIncluded
        );
    }

    #[test]
    fn literal_ignored_ancestor_does_not_readmit_ignored_siblings() {
        // C019's mandatory premise negative. `ignore` names the ancestor with a
        // literal path, so no pattern matches `other` on its own; only the
        // exclusion inherited from `/archive` keeps it out. The pre-C019
        // implementation discovers 2 manifests here.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep", "other"]);

        let found = routine_manifests(
            vec![root.clone()],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("keep")],
        );
        assert_eq!(
            found.manifests,
            vec![archive.join("keep/Cargo.toml")],
            "the ignored sibling must not re-enter through the exception route"
        );
        // The same tree with no exception at all is still wholly excluded, so
        // the assertion above cannot be satisfied by ignoring the rules.
        let none = routine_manifests(vec![root], &[archive.to_string_lossy().into_owned()], &[]);
        assert!(none.manifests.is_empty());
    }

    #[test]
    fn user_ignore_prune_counts_only_the_subtrees_the_walk_refused() {
        // C019's instrumentation contract: a sibling pruned because it inherits
        // exclusion is a user-ignore prune; the pass-through ancestor that was
        // entered on purpose, and the re-included directory, are not.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep", "other", "third"]);

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("keep")],
        );
        assert_eq!(
            found.counters.user_ignore_prunes, 2,
            "only `other` and `third` were refused; `archive` was entered and `keep` is in scope"
        );
        assert_eq!(found.counters.directories_pruned, 2);
        assert_eq!(
            found.counters.directories_pruned,
            found.counters.platform_system_prunes
                + found.counters.cargo_home_prunes
                + found.counters.rustup_home_prunes
                + found.counters.target_vcs_prunes
                + found.counters.user_ignore_prunes,
            "the five specific counters must still sum to the total"
        );
    }

    #[test]
    fn wildcard_and_recursive_ignore_patterns_keep_their_documented_behavior() {
        // The two shapes 0.1.6 documented as the workaround must still admit
        // only the exception. They are in one test on purpose: the assertion is
        // that correcting the literal rule left them alone.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep", "other"]);
        for pattern in ["*", "**"] {
            let found = routine_manifests(
                vec![root.clone()],
                &[format!("{}/{}", archive.display(), pattern)],
                &[archive.join("keep")],
            );
            assert_eq!(
                found.manifests,
                vec![archive.join("keep/Cargo.toml")],
                "ignore = archive/{pattern} must admit only the exception"
            );
        }
    }

    #[test]
    fn several_exceptions_under_one_ignored_ancestor_are_each_reachable() {
        // One direct exception and one nested exception, with siblings at both
        // levels. Only the two exception subtrees may be discovered.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(
            &archive,
            &["keep-a", "other", "nested/keep-b", "nested/other", "plain"],
        );

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("keep-a"), archive.join("nested/keep-b")],
        );
        let mut expected = vec![archive.join("keep-a/Cargo.toml")];
        expected.push(archive.join("nested/keep-b/Cargo.toml"));
        expected.sort();
        assert_eq!(found.manifests, expected);
        assert_eq!(
            found.counters.user_ignore_prunes, 3,
            "`other`, `nested/other`, and `plain` are refused; `archive` and `nested` are entered"
        );
    }

    #[test]
    fn a_deeply_nested_exception_never_visits_a_pruned_siblings_subtree() {
        // The strongest form of the containment claim: the pruned sibling holds
        // its own project, so finding it would prove the subtree was walked
        // rather than refused.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(
            &archive,
            &[
                "a/b/c/keep",
                "a/b/sibling/inner",
                "a/sibling/inner",
                "sibling/inner",
            ],
        );

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("a/b/c/keep")],
        );
        assert_eq!(
            found.manifests,
            vec![archive.join("a/b/c/keep/Cargo.toml")],
            "only the exception subtree is in scope, at every intermediate level"
        );
        assert_eq!(
            found.visited_entries, 10,
            "the three refused siblings' subtrees are never read, so the traversal \
             total counts only root + archive + the pass-through route + the \
             exception + its manifest"
        );
    }

    #[test]
    fn an_exact_unignore_reincludes_the_whole_subtree_at_its_root() {
        // Re-inclusion is for the subtree rooted at the exact path, so a nested
        // project beneath the exception root stays discoverable even though the
        // literal ancestor above it is ignored.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep", "keep/nested", "other"]);

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("keep")],
        );
        let mut expected = vec![archive.join("keep/Cargo.toml")];
        expected.push(archive.join("keep/nested/Cargo.toml"));
        expected.sort();
        assert_eq!(found.manifests, expected);
        assert_eq!(found.counters.user_ignore_prunes, 1, "only `other`");
    }

    #[test]
    fn an_explicit_root_still_bypasses_a_literal_ignore_entirely() {
        // Requirement: a named root means it. The corrected filter must not
        // narrow an explicit scope, or `scan <dir>` would silently lose
        // projects the caller asked for.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep", "other"]);

        let mut expected = vec![archive.join("keep/Cargo.toml")];
        expected.push(archive.join("other/Cargo.toml"));
        expected.sort();
        assert_eq!(explicit_manifests(&archive).manifests, expected);
    }

    #[test]
    fn an_exact_unignore_does_not_reopen_an_internal_prune() {
        // The internal prunes are decided before the user filters, so even a
        // literal unignore aimed straight at a `target` or `.git` tree leaves it
        // refused. This is the ordering that keeps unignore from becoming a way
        // around VCS/target/symlink policy.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        let keep = archive.join("keep");
        project_fixtures(&keep, &["", "target", ".git", "target/nested"]);
        fs::create_dir_all(archive.join("other")).unwrap();

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[keep.clone(), keep.join("target"), keep.join(".git")],
        );
        assert_eq!(
            found.manifests,
            vec![keep.join("Cargo.toml")],
            "only the plain project under the exception root is in scope"
        );
        assert_eq!(
            found.counters.target_vcs_prunes, 2,
            "`target` and `.git` are internal prunes"
        );
        assert_eq!(
            found.counters.user_ignore_prunes, 1,
            "only `other`, and an internal prune is never also counted as a user-ignore prune"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_exact_unignore_does_not_reopen_a_symlinked_directory() {
        use std::os::unix::fs::symlink;
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        let keep = archive.join("keep");
        // The target has to sit outside the scan root, or the walk would reach
        // it directly and the assertion would not be about the symlink at all.
        let outside_dir = tempdir().unwrap();
        let outside = fs::canonicalize(outside_dir.path()).unwrap();
        project_fixtures(&keep, &[""]);
        project_fixtures(&outside, &[""]);
        symlink(&outside, keep.join("link")).unwrap();

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[keep.clone(), keep.join("link")],
        );
        assert_eq!(
            found.manifests,
            vec![keep.join("Cargo.toml")],
            "a symlinked directory stays refused even when unignored by name"
        );
    }

    #[test]
    fn a_broad_ignored_region_is_pruned_before_its_siblings_are_read() {
        // C019's cost requirement: refusing a sibling must be a refusal, not a
        // cheap-looking check that still walks the subtree. Each sibling holds
        // its own project, so a walk that entered them would both discover them
        // and inflate the traversal total far past the refused-entry count.
        const SIBLINGS: usize = 200;
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        project_fixtures(&archive, &["keep"]);
        for index in 0..SIBLINGS {
            project_fixtures(&archive, &[&format!("s{index}/inner")]);
        }

        let found = routine_manifests(
            vec![root],
            &[archive.to_string_lossy().into_owned()],
            &[archive.join("keep")],
        );
        assert_eq!(found.manifests, vec![archive.join("keep/Cargo.toml")]);
        assert_eq!(
            found.counters.user_ignore_prunes, SIBLINGS as u64,
            "every sibling is refused, and each refusal is counted once"
        );
        assert_eq!(
            found.visited_entries,
            (SIBLINGS + 4) as u64,
            "root + archive + the exception directory + its manifest: a refused \
             sibling yields its directory entry and nothing beneath it"
        );
    }

    #[test]
    fn an_explicit_unignore_of_the_scan_root_prunes_the_whole_root_visibly() {
        // A scan root inside a literal ignored region is refused outright, and
        // unlike the other reasons that refusal is counted even at depth 0, so
        // an empty scan says why instead of reporting zero prunes.
        let d = tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let archive = root.join("archive");
        let inside = archive.join("inside");
        project_fixtures(&inside, &[""]);

        let found = routine_manifests(
            vec![inside.clone()],
            &[archive.to_string_lossy().into_owned()],
            &[],
        );
        assert!(found.manifests.is_empty());
        assert_eq!(found.counters.directories_pruned, 1);
        assert_eq!(found.counters.user_ignore_prunes, 1);
    }

    #[test]
    fn an_empty_policy_includes_everything() {
        // The explicit-scope case arrives with no rules at all; there is no
        // exclusion to inherit and nothing to re-include.
        let f = Filters::new(&[], &[]).unwrap();
        assert_eq!(
            f.disposition(Path::new("/any/archive")),
            Disposition::Included
        );
        assert!(!f.inherits_ignore(Path::new("/any/archive")));
    }
}
