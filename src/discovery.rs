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
    fn ignored(&self, p: &Path) -> bool {
        if self.globs.is_empty() {
            return false;
        }
        let Some(s) = p.to_str() else {
            return false;
        };
        self.globs.is_match(s) && !self.unignore.iter().any(|u| p.starts_with(u))
    }
    fn exception_below(&self, p: &Path) -> bool {
        self.unignore.iter().any(|u| u.starts_with(p))
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
    let global = matches!(policy.scope, ScanScope::Global(_));
    let configured_global_policy = global.then(crate::policy::global_discovery_policy);
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
        (ScanScope::Global(_), Some(p)) => p.roots.clone(),
        (ScanScope::Global(v), None) => v.clone(),
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
            skip_metadata,
            order,
        );
    }
    let explicit = matches!(policy.scope, ScanScope::Explicit(_));
    for root in roots {
        if !root.exists() {
            diagnostics.push(diag(
                DiagnosticCategory::PlatformRoot,
                &root,
                "scan root is unavailable",
            ));
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
            &mut pruned,
            observer,
        )?;
    }
    manifests.sort();
    manifests.dedup();
    let counters = ScanCounters {
        directories_visited: visited,
        directories_pruned: pruned,
        manifests_found: manifests.len() as u64,
        pruned_no_cargo: pruned,
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
    skip_metadata: bool,
    order: dua_core::Order,
) -> Result<ManifestDiscovery, AppError> {
    let mut canonical_roots = Vec::new();
    for root in roots {
        if !root.is_dir() {
            diagnostics.push(diag(
                DiagnosticCategory::PlatformRoot,
                root,
                "scan root is unavailable",
            ));
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
            if entry_is_within_any(entry, root, &descend_system_prunes)
                || entry_is_within_any(entry, root, &descend_cargo_prunes)
                || entry_is_within_any(entry, root, &descend_rustup_prunes)
            {
                return false;
            }
            if descend_filters.globs.is_empty() {
                return true;
            }
            let path = entry.path();
            !descend_filters.ignored(&path) || descend_filters.exception_below(&path)
        },
    );
    let mut system_count = 0;
    let mut cargo_count = 0;
    let mut rustup_count = 0;
    let mut target_count = 0;
    let mut ignore_count = 0;
    let mut batch_visited = 0;
    let mut batch_pruned = 0;
    let mut top_level_root_counts = HashMap::new();
    let mut top_level_counts: HashMap<usize, HashMap<OsString, u64>> = HashMap::new();
    let mut finished_roots = 0u64;
    let mut manifest_candidates = 0u64;
    let mut manifest_validated = 0u64;
    let mut manifest_validation_time = std::time::Duration::ZERO;
    let mut last_profile = std::time::Instant::now();
    let traversal_started = std::time::Instant::now();
    for (root_idx, event) in walk {
        let entry = match event {
            dua_core::RootEvent::Entry(Ok(e)) => e,
            dua_core::RootEvent::Entry(Err(e)) => {
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category: if e.kind() == std::io::ErrorKind::PermissionDenied {
                        DiagnosticCategory::PermissionDenied
                    } else {
                        DiagnosticCategory::Metadata
                    },
                    path: None,
                    message: "filesystem traversal entry could not be read".into(),
                });
                continue;
            }
            dua_core::RootEvent::Finished => {
                finished_roots += 1;
                continue;
            }
        };
        *visited = visited.saturating_add(1);
        if let Some((_, root)) = roots.get(root_idx) {
            if entry.depth == 0 {
                *top_level_root_counts.entry(root_idx).or_insert(0) += 1;
            } else if let Some(component) = top_level_component(&entry, root) {
                let subtrees = top_level_counts.entry(root_idx).or_default();
                if let Some(count) = subtrees.get_mut(component) {
                    *count += 1;
                } else {
                    subtrees.insert(component.to_owned(), 1);
                }
            }
        }
        batch_visited += 1;
        if batch_visited >= 512 {
            observer.dirs_visited(batch_visited);
            batch_visited = 0;
        }
        if entry.file_type.is_dir() {
            let Some((_, root)) = roots.get(root_idx) else {
                continue;
            };
            let system = entry.depth > 0 && entry_is_within_any(&entry, root, &system_prunes);
            let cargo = entry.depth > 0 && entry_is_within_any(&entry, root, &cargo_prunes);
            let rustup = entry.depth > 0 && entry_is_within_any(&entry, root, &rustup_prunes);
            let target = entry.depth > 0 && (entry.file_name == "target" || vcs(&entry.file_name));
            let ignored = if entry.depth > 0 && !filters.globs.is_empty() {
                let path = entry.path();
                filters.ignored(&path) && !filters.exception_below(&path)
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
        if entry.file_type.is_file() && entry.file_name == "Cargo.toml" {
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
            let top_level_entries =
                top_level_entry_map(&roots, &top_level_root_counts, &top_level_counts);
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
        directories_visited: *visited,
        directories_pruned: *pruned,
        platform_system_prunes: system_count,
        cargo_home_prunes: cargo_count,
        rustup_home_prunes: rustup_count,
        target_vcs_prunes: target_count,
        user_ignore_prunes: ignore_count,
        manifests_found: manifests.len() as u64,
        pruned_no_cargo: *pruned,
        ..Default::default()
    };
    let attribution_started = profile_subtrees.then(std::time::Instant::now);
    let top_level_entries = top_level_entry_map(&roots, &top_level_root_counts, &top_level_counts);
    if let Some(started) = attribution_started {
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

pub fn scan(policy: &EffectiveScanPolicy) -> Result<ScanReport, AppError> {
    // M004-compatible wrapper: manifest-first traversal, then conventional
    // direct-target filter. No Cargo subprocesses are invoked here.
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

#[allow(clippy::too_many_arguments)]
fn discover_manifests_root(
    root: &Path,
    filters: &Filters,
    explicit: bool,
    prunes: &[PathBuf],
    manifests: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<ScanDiagnostic>,
    visited: &mut u64,
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
                diagnostics.push(ScanDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    category,
                    path: None,
                    message: "filesystem traversal entry could not be read".into(),
                });
                continue;
            }
        };
        *visited = visited.saturating_add(1);
        batch_visited += 1;
        if batch_visited >= 512 {
            observer.dirs_visited(batch_visited);
            batch_visited = 0;
        }
        let path = entry.path();
        // Count pruned dirs for instrumentation (target/VCS/system/cargo/ignored).
        if entry.file_type.is_dir()
            && path != root_path
            && (path.file_name().is_some_and(|n| n == "target" || vcs(n))
                || system_prune(&path)
                || is_cargo_home_pruned(&path, prunes)
                || (!explicit && filters.ignored(&path)))
        {
            *pruned = pruned.saturating_add(1);
            batch_pruned += 1;
            if batch_pruned >= 64 {
                observer.dirs_pruned(batch_pruned);
                batch_pruned = 0;
            }
        }
        if entry.file_type.is_file() && path.file_name().is_some_and(|n| n == "Cargo.toml") {
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
    let prunes = cargo_home_prunes();
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
            if entry.file_type.is_symlink() {
                return false;
            }
            if path != root_path && path.file_name().is_some_and(|n| n == "target" || vcs(n)) {
                return false;
            }
            if system_prune(&path) {
                return false;
            }
            if is_cargo_home_pruned(&path, &prunes) {
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
            // Manifest must be a real file.
            if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
                && let Some(project_root) = path.parent()
            {
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
    use crate::progress::NoopObserver;
    use tempfile::tempdir;

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
