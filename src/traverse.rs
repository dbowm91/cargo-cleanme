//! Single ownership point for filesystem traversal.
//!
//! Discovery, source-activity scanning, target sizing, and post-clean
//! measurement all go through `dua-core` here. Domain and CLI layers never
//! see `dua-core` types; they work with paths, sizes, and timestamps only.
//!
//! `dua-core` supplies parallel directory reads from a bounded worker pool
//! and never follows symlinks. Per-file size and timestamp semantics stay
//! identical to the previous sequential implementation on purpose: every
//! entry is re-stat'ed with `std::fs::symlink_metadata` (target sizing) or
//! `std::fs::metadata` (source activity), so platform behaviour, error
//! conservatism, and the allocated-bytes metric do not drift with the engine
//! change. Dropping any walk stops and joins its workers; no partial scan is
//! ever presented as complete.

#![allow(clippy::result_unit_err)]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

/// Whether `path` is a symlink, or a spelling of one.
///
/// **One definition, four call sites.** The predicate answered "is this path a
/// symlink?" had four byte-identical copies — one in each module that probes
/// an output root — and a copy is a place the predicate can drift: the four
/// must agree, because the same path is classified by several of them and a
/// disagreement would make one module treat a symlinked output as a plain
/// directory while another treats it as a symlink.
///
/// A trailing separator or a `/.` component makes `symlink_metadata` follow
/// the link, so the spelling is trimmed and re-probed. The two spellings that
/// hide a symlink are `link/` — POSIX `symlink_metadata` follows the final
/// component — and `link/.`, which stats through the link as well.
///
/// The trimming is done with `Path`'s own component handling, which knows
/// which characters separate components *on this platform*: `\` separates on
/// Windows and is an ordinary filename character on Unix, where a directory
/// may legitimately be named `link\`. The bytes are never decoded, so a name
/// that is not valid UTF-8 survives the trimming — a lossy conversion would
/// turn it into a path that does not exist, and the probe would answer about
/// a different path than the one it was asked about.
pub fn path_is_symlink(path: &Path) -> bool {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return true;
    }
    match trimmed_spelling(path) {
        // The trimmed spelling is the same path when the original spelling
        // hides nothing, so there is nothing left to probe.
        Some(trimmed) if trimmed.as_os_str() != path.as_os_str() => {
            fs::symlink_metadata(&trimmed).is_ok_and(|m| m.file_type().is_symlink())
        }
        _ => false,
    }
}

/// The path with its trailing separators and trailing `.` component removed.
///
/// `Path` already knows the platform's separators, so the result is the same
/// path on every platform: `link/` and `link/.` both become `link`, while a
/// Unix path whose name ends in `\` is left alone — that is a directory of
/// that name, not a spelling of a link.
fn trimmed_spelling(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?;
    let parent = path.parent()?;
    Some(parent.join(name))
}

/// The lexical identity of a path: `.` and `..` resolved textually, with no
/// filesystem access.
///
/// `..` is popped lexically rather than resolved through the filesystem,
/// because the path may not exist — and a path that does not exist still has
/// to be comparable with its siblings. A relative path is returned unchanged:
/// there is no absolute spelling to reduce it to.
pub fn lexical_identity(path: &Path) -> PathBuf {
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

/// Resolve a path as far as the filesystem allows.
///
/// Canonicalizes the deepest existing ancestor and re-appends the remaining
/// components, so a path that does not exist yet is still compared in its
/// physical location instead of lexically. This keeps the cross-workspace
/// overlap check sound when a path component (for example macOS `/var`, or a
/// symlinked ancestor of a configured output directory) is not canonical.
pub fn resolve_as_far_as_possible(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    resolve_beneath_existing_ancestor(path)
}

/// The walk-up half of [`resolve_as_far_as_possible`], for a caller that has
/// already established that the path does not resolve.
fn resolve_beneath_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while let Some(name) = current.file_name() {
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            break;
        };
        if let Ok(canonical) = fs::canonicalize(parent) {
            let mut resolved = canonical;
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        if parent.as_os_str().is_empty() {
            break;
        }
        current = parent;
    }
    path.to_path_buf()
}

/// A scope root, resolved for comparison and kept for reporting.
///
/// The two are separate because a root that does not resolve still has to be
/// *compared* in its physical location — otherwise a symlinked prefix makes
/// one physical root look like two — while it still has to be *reported* by
/// the name the operator gave it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoot {
    /// The identity the root is compared by: its physical location, resolved
    /// as far as the filesystem allows.
    pub identity: PathBuf,
    /// The spelling the root is reported as: the canonical one when the root
    /// resolves, and the caller's own spelling when it does not.
    pub spelling: PathBuf,
    /// The error that stopped the root from resolving, if it did not resolve.
    pub error: Option<String>,
}

/// Resolve one scope root.
pub fn resolve_root(path: &Path) -> ResolvedRoot {
    match fs::canonicalize(path) {
        Ok(canonical) => ResolvedRoot {
            identity: lexical_identity(&canonical),
            spelling: canonical,
            error: None,
        },
        Err(e) => {
            let resolved = resolve_beneath_existing_ancestor(path);
            ResolvedRoot {
                identity: lexical_identity(&resolved),
                spelling: path.to_path_buf(),
                error: Some(e.to_string()),
            }
        }
    }
}

/// Collapse a set of scope roots to the outermost ones.
///
/// A root contained in another root is dropped: the ancestor covers it, so
/// walking the ancestor covers the descendant too.
///
/// **One definition, three call sites.** The collapse — canonicalise, dedup,
/// drop nested roots — was implemented four times, and the copies disagreed
/// about a root that cannot be resolved: one swallowed the failure, one failed
/// the whole run, one compared the unresolved root lexically against
/// canonicalised siblings, and one did not canonicalise at all. The *policy*
/// still belongs to the caller (the cleanup surface fails closed; the scan
/// and display surfaces keep the spelling), but the *mechanism* is one.
///
/// The comparison is by [`ResolvedRoot::identity`], so a symlinked prefix
/// cannot make one physical root look like two: `/var/x` and `/private/var/x`
/// are one root on macOS, not two.
pub fn collapse_outermost(roots: Vec<PathBuf>) -> Vec<ResolvedRoot> {
    collapse_resolved_protecting(resolve_roots(roots), &[])
}

/// Collapse a set of scope roots to the outermost ones, keeping every root
/// whose identity is in `protected` even when an ancestor contains it.
///
/// The protection is the only part that is specific to a surface that has one:
/// the policy surface's protected roots are their ancestor's exceptions, and
/// collapsing them would re-create the ambiguity the caller resolved. The
/// mechanism — resolve, sort, dedup, drop nested roots — is the shared one.
pub fn collapse_outermost_protecting(
    roots: Vec<PathBuf>,
    protected: &[PathBuf],
) -> Vec<ResolvedRoot> {
    collapse_resolved_protecting(resolve_roots(roots), protected)
}

/// Collapse roots that are already resolved.
///
/// The cleanup surface resolves its roots itself, because a root that does not
/// resolve fails the run there; collapsing the roots it already resolved
/// keeps the filesystem from being asked the same question twice.
pub fn collapse_resolved(resolved: Vec<ResolvedRoot>) -> Vec<ResolvedRoot> {
    collapse_resolved_protecting(resolved, &[])
}

/// Resolve every root once.
fn resolve_roots(roots: Vec<PathBuf>) -> Vec<ResolvedRoot> {
    roots.into_iter().map(|root| resolve_root(&root)).collect()
}

/// The collapse itself, over roots that are already resolved.
fn collapse_resolved_protecting(
    mut resolved: Vec<ResolvedRoot>,
    protected: &[PathBuf],
) -> Vec<ResolvedRoot> {
    // Sort and dedup by the reported spelling, so the surviving root of a
    // duplicated pair is the one the per-surface implementations kept.
    resolved.sort_by(|a, b| a.spelling.cmp(&b.spelling));
    resolved.dedup_by(|a, b| a.spelling == b.spelling);
    let mut collapsed: Vec<ResolvedRoot> = Vec::new();
    for root in resolved {
        if protected.contains(&root.identity)
            || !collapsed
                .iter()
                .any(|parent: &ResolvedRoot| root.identity.starts_with(&parent.identity))
        {
            collapsed.push(root);
        }
    }
    collapsed
}

/// Whether `path` is a directory holding a nested-repository marker.
///
/// One definition, four call sites: the descend decision and the entry
/// classification both ask this question, and they must agree or a boundary
/// entry would be treated differently depending on which one ran.
fn contains_vcs_marker(path: &Path) -> bool {
    fs::read_dir(path).ok().is_some_and(|mut children| {
        children.any(|child| {
            child
                .ok()
                .is_some_and(|child| is_vcs_name(&child.file_name().to_string_lossy()))
        })
    })
}

/// Memo of directories already found to hold a nested-repository marker.
///
/// The descend callback and the entry loop ask the same question about the same
/// directory, and each answer costs a `read_dir`. Both answers are recorded, so
/// a directory is read once rather than twice: recording only the *positives*
/// threw away the answer that mattered, because the common answer is "no
/// marker here", and every source directory was therefore read twice.
///
/// The memo is created per walk and dropped with it, so its size is bounded by
/// the number of directories that walk visited. Shared across the walk's
/// worker threads, so it is a `Mutex`; the critical section is one hash lookup
/// and never spans I/O.
#[derive(Clone, Default)]
struct NestedRepoMarkers(Arc<Mutex<HashMap<PathBuf, bool>>>);

impl NestedRepoMarkers {
    fn probe(&self, path: &Path) -> bool {
        if let Ok(known) = self.0.lock()
            && let Some(found) = known.get(path)
        {
            return *found;
        }
        let found = contains_vcs_marker(path);
        if let Ok(mut known) = self.0.lock() {
            known.insert(path.to_path_buf(), found);
        }
        found
    }
}

/// Bounded worker count shared by every traversal.
///
/// One pool of at most eight workers is used per walk, and batched target
/// analysis shares a single pool across all candidate roots so candidate
/// count never spawns unbounded worker pools.
pub fn worker_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 8)
}

fn walk_options() -> dua_core::Options {
    dua_core::Options::default()
}

/// Summarized target-tree observation. `uncertain` means at least one walk,
/// metadata, or sizing step failed; callers must treat the candidate as
/// ineligible rather than promoting a partial measurement.
#[derive(Clone, Debug, Default)]
pub struct TargetStats {
    pub bytes: u64,
    pub entries: u64,
    pub newest: Option<SystemTime>,
    pub uncertain: bool,
    /// A package directory was found under the measured root.
    ///
    /// `cargo clean --target-dir <root>` removes the whole root, so a package
    /// directory inside it means the root holds checked-in sources and not
    /// only build output. The caller must refuse the candidate.
    pub package_dir: Option<PathBuf>,
}

/// The package directory a manifest file belongs to, if `path` names one.
///
/// A manifest directly under the measured root makes the root itself the
/// package directory.
fn manifest_package_dir(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?;
    if !crate::domain::is_manifest_shaped_name(name) {
        return None;
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn newest_time(current: Option<SystemTime>, next: SystemTime) -> Option<SystemTime> {
    Some(current.map_or(next, |old| old.max(next)))
}

fn is_vcs_name(name: &str) -> bool {
    matches!(name, ".git" | ".hg" | ".svn")
}

/// Measure one target tree with the shared `dua-core` engine.
///
/// Semantics match the previous sequential walk: the root itself is not
/// counted, every other entry increments the entry count, regular files add
/// allocated bytes, and any walk/metadata/sizing failure marks the result
/// uncertain.
///
/// This is the *proof* measurement: it re-stats every entry instead of
/// trusting the engine's cached metadata, so an entry that vanished between
/// the walk and the measurement is still observed.
pub fn measure_single_target(target: &Path) -> TargetStats {
    let mut stats = TargetStats::default();
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
        _ => {
            stats.uncertain = true;
            return stats;
        }
    }
    let root = target.to_path_buf();
    let walk = dua_core::walk(
        &root,
        worker_threads(),
        dua_core::Order::ParentFirst,
        walk_options(),
        |_| true,
    );
    for item in walk {
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => {
                stats.uncertain = true;
                break;
            }
        };
        let path = entry.path();
        if path == root {
            continue;
        }
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => {
                stats.uncertain = true;
                break;
            }
        };
        let modified = match meta.modified() {
            Ok(time) => time,
            Err(_) => {
                stats.uncertain = true;
                break;
            }
        };
        stats.newest = newest_time(stats.newest, modified);
        stats.entries = match stats.entries.checked_add(1) {
            Some(n) => n,
            None => {
                stats.uncertain = true;
                break;
            }
        };
        if stats.package_dir.is_none() {
            stats.package_dir = manifest_package_dir(&path);
        }
        if meta.is_file() {
            match filesize::file_real_size_fast(&path, &meta) {
                Ok(size) => match stats.bytes.checked_add(size) {
                    Some(n) => stats.bytes = n,
                    None => {
                        stats.uncertain = true;
                        break;
                    }
                },
                Err(_) => {
                    stats.uncertain = true;
                    break;
                }
            }
        }
    }
    stats
}

/// Measure many target trees with one bounded worker pool.
///
/// Each input is `(candidate_index, target_path)`. Results are returned in
/// input order. If the same `candidate_index` appears more than once it is
/// measured once, from the first path given, and every occurrence of that index
/// reports that measurement. Any per-root failure marks only that root
/// uncertain; other roots still complete. The pool size is [`worker_threads`],
/// independent of candidate count.
///
/// Every entry is re-stated, exactly as the proof measurement
/// ([`measure_single_target`]) does: an entry that vanished between the walk
/// and the measurement has to be observed, because a partial measurement must
/// never be promoted to a candidate.
pub fn measure_many_targets(targets: &[(usize, PathBuf)]) -> Vec<(usize, TargetStats)> {
    let mut ordered: Vec<(usize, PathBuf)> = targets.iter().map(|(i, p)| (*i, p.clone())).collect();
    ordered.sort_by_key(|(i, _)| *i);
    let mut stats_by_index: std::collections::HashMap<usize, TargetStats> =
        std::collections::HashMap::new();
    let mut roots: Vec<(usize, PathBuf)> = Vec::new();
    let mut seen_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (index, path) in &ordered {
        // `dua_core::walk_roots` asserts that root indices are unique, so a
        // repeated index would panic inside the engine. The first occurrence
        // wins; later duplicates are dropped rather than measured twice, which
        // also stops one target's bytes being accumulated into another's stats
        // through the shared `stats_by_index` entry.
        if !seen_indices.insert(*index) {
            continue;
        }
        let mut stats = TargetStats::default();
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                roots.push((*index, path.clone()));
                stats_by_index.insert(*index, stats);
            }
            _ => {
                stats.uncertain = true;
                stats_by_index.insert(*index, stats);
            }
        }
    }
    if roots.is_empty() {
        return targets
            .iter()
            .map(|(i, _)| (*i, stats_by_index.get(i).cloned().unwrap_or_default()))
            .collect();
    }
    let root_paths: std::collections::HashMap<usize, PathBuf> = roots.iter().cloned().collect();
    let walk = dua_core::walk_roots(
        roots,
        worker_threads(),
        dua_core::Order::ParentFirst,
        walk_options(),
        |_, _| true,
    );
    for (index, event) in walk {
        match event {
            dua_core::RootEvent::Entry(Ok(entry)) => {
                let Some(stats) = stats_by_index.get_mut(&index) else {
                    continue;
                };
                if stats.uncertain {
                    continue;
                }
                let path = entry.path();
                let Some(root) = root_paths.get(&index) else {
                    stats.uncertain = true;
                    continue;
                };
                if path == *root {
                    continue;
                }
                // The engine collected the metadata for this entry while it was
                // walking, but its type is platform-specific (macOS and Windows
                // carry their own `Metadata`), so it cannot be handed to
                // `filesize`. The re-stat stays.
                let meta = match fs::symlink_metadata(&path) {
                    Ok(meta) => meta,
                    Err(_) => {
                        stats.uncertain = true;
                        continue;
                    }
                };
                let modified = match meta.modified() {
                    Ok(time) => time,
                    Err(_) => {
                        stats.uncertain = true;
                        continue;
                    }
                };
                stats.newest = newest_time(stats.newest, modified);
                match stats.entries.checked_add(1) {
                    Some(n) => stats.entries = n,
                    None => {
                        stats.uncertain = true;
                        continue;
                    }
                }
                if stats.package_dir.is_none() {
                    stats.package_dir = manifest_package_dir(&path);
                }
                if meta.is_file() {
                    match filesize::file_real_size_fast(&path, &meta) {
                        Ok(size) => match stats.bytes.checked_add(size) {
                            Some(n) => stats.bytes = n,
                            None => stats.uncertain = true,
                        },
                        Err(_) => stats.uncertain = true,
                    }
                }
            }
            dua_core::RootEvent::Entry(Err(_)) => {
                if let Some(stats) = stats_by_index.get_mut(&index) {
                    stats.uncertain = true;
                }
            }
            dua_core::RootEvent::Finished => {}
        }
    }
    // One result per input, in input order. A repeated `candidate_index` gets
    // the same measurement as its first occurrence: stats are a function of the
    // index, so every occurrence of that index must agree.
    targets
        .iter()
        .map(|(i, _)| (*i, stats_by_index.get(i).cloned().unwrap_or_default()))
        .collect()
}

/// Outcome of a source-activity scan.
#[derive(Clone, Debug)]
pub enum SourceActivity {
    /// A recent (or future) source timestamp was observed; the value is that
    /// timestamp. Callers treat the candidate as active without sizing it.
    Recent(SystemTime),
    /// No recent source timestamp; the value is the newest observed source
    /// timestamp, if any entry could be stat'ed.
    Quiet(Option<SystemTime>),
}

/// Scan one project source tree for recent activity with early exit.
///
/// The target subtree, VCS directories, and directories containing a nested
/// VCS marker are excluded from activity evidence, including the boundary
/// entries themselves. Any walk or metadata failure is conservative
/// (`Err(())`). Timestamps use `fs::metadata` (following) to preserve the
/// previous source-activity semantics for included entries.
///
/// `_start` is the instant the scan began; the cutoff is derived from it as
/// `start - recency`, so `cutoff <= start`. The parameter is kept because the
/// signature is public API, but the verdict is decided by the cutoff alone.
pub fn source_activity(
    project: &Path,
    target: &Path,
    _start: SystemTime,
    cutoff: SystemTime,
) -> Result<SourceActivity, ()> {
    let project_root = project.to_path_buf();
    let target_root = target.to_path_buf();
    // The cutoff is `start - recency` with an *unsigned* recency, so
    // `cutoff <= start` and "modified since the scan began" is already inside
    // "modified since the cutoff". The second disjunct was subsumed by the
    // first and could never change the verdict.
    let is_recent = move |time: SystemTime| time >= cutoff;
    let walk_root = project_root.clone();
    let descend_project = project_root.clone();
    let descend_target = target_root.clone();
    let markers = NestedRepoMarkers::default();
    let descend_markers = markers.clone();
    let walk = dua_core::walk(
        &walk_root,
        worker_threads(),
        dua_core::Order::ParentFirst,
        walk_options(),
        move |entry| {
            let path = entry.path();
            if path != descend_project
                && (path == descend_target || path.starts_with(&descend_target))
            {
                return false;
            }
            if path != descend_project
                && path
                    .file_name()
                    .is_some_and(|n| is_vcs_name(&n.to_string_lossy()))
            {
                return false;
            }
            if path != descend_project && entry.file_type.is_dir() && descend_markers.probe(&path) {
                return false;
            }
            true
        },
    );
    let mut newest: Option<SystemTime> = None;
    for item in walk {
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => return Err(()),
        };
        let path = entry.path();
        // Pruned boundary entries carry no source-activity evidence, even for
        // their own directory mtime (which updates when excluded descendants
        // change). This keeps nested-repository activity from protecting the
        // parent through the boundary directory's own timestamp.
        if is_excluded(
            &project_root,
            &target_root,
            &markers,
            &path,
            entry.file_type.is_dir(),
        ) {
            continue;
        }
        let modified = fs::metadata(&path)
            .map_err(|_| ())?
            .modified()
            .map_err(|_| ())?;
        newest = newest_time(newest, modified);
        if is_recent(modified) {
            return Ok(SourceActivity::Recent(modified));
        }
    }
    Ok(SourceActivity::Quiet(newest))
}

fn is_excluded(
    project: &Path,
    target: &Path,
    markers: &NestedRepoMarkers,
    path: &Path,
    is_dir: bool,
) -> bool {
    if path == project {
        return false;
    }
    if path == target || path.starts_with(target) {
        return true;
    }
    if path
        .file_name()
        .is_some_and(|n| is_vcs_name(&n.to_string_lossy()))
    {
        return true;
    }
    is_dir && markers.probe(path)
}

/// Scan one workspace member root for recent activity, excluding *all*
/// resolved output roots contained in the member tree.
///
/// VCS directories and nested-repository boundaries (directories containing a
/// nested VCS marker) are excluded including their own boundary mtime, so
/// activity inside an independent nested repo never protects the outer member.
/// Any walk/metadata failure is conservative (`Err(())`).
///
/// `_start` is the instant the scan began; the cutoff is derived from it as
/// `start - recency`, so `cutoff <= start`. The parameter is kept because the
/// signature is public API, but the verdict is decided by the cutoff alone.
pub fn workspace_member_activity(
    member_root: &Path,
    output_roots: &[PathBuf],
    _start: SystemTime,
    cutoff: SystemTime,
) -> Result<SourceActivity, ()> {
    let root = member_root.to_path_buf();
    let outputs = output_roots.to_vec();
    let outputs_for_closure = outputs.clone();
    // The cutoff is `start - recency` with an *unsigned* recency, so
    // `cutoff <= start` and "modified since the scan began" is already inside
    // "modified since the cutoff". The second disjunct was subsumed by the
    // first and could never change the verdict.
    let is_recent = move |time: SystemTime| time >= cutoff;
    let descend_root = root.clone();
    let markers = NestedRepoMarkers::default();
    let descend_markers = markers.clone();
    let walk = dua_core::walk(
        &root,
        worker_threads(),
        dua_core::Order::ParentFirst,
        walk_options(),
        move |entry| {
            let path = entry.path();
            if path != descend_root
                && outputs_for_closure
                    .iter()
                    .any(|o| path == *o || path.starts_with(o))
            {
                return false;
            }
            if path != descend_root
                && path
                    .file_name()
                    .is_some_and(|n| is_vcs_name(&n.to_string_lossy()))
            {
                return false;
            }
            if path != descend_root && entry.file_type.is_dir() && descend_markers.probe(&path) {
                return false;
            }
            true
        },
    );
    let mut newest: Option<SystemTime> = None;
    for item in walk {
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => return Err(()),
        };
        let path = entry.path();
        if is_excluded_multi(&root, &outputs, &markers, &path, entry.file_type.is_dir()) {
            continue;
        }
        let modified = fs::metadata(&path)
            .map_err(|_| ())?
            .modified()
            .map_err(|_| ())?;
        newest = newest_time(newest, modified);
        if is_recent(modified) {
            return Ok(SourceActivity::Recent(modified));
        }
    }
    Ok(SourceActivity::Quiet(newest))
}

fn is_excluded_multi(
    member_root: &Path,
    outputs: &[PathBuf],
    markers: &NestedRepoMarkers,
    path: &Path,
    is_dir: bool,
) -> bool {
    if path == member_root {
        return false;
    }
    if outputs.iter().any(|o| path == *o || path.starts_with(o)) {
        return true;
    }
    if path
        .file_name()
        .is_some_and(|n| is_vcs_name(&n.to_string_lossy()))
    {
        return true;
    }
    is_dir && markers.probe(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn worker_pool_is_bounded() {
        let threads = worker_threads();
        assert!((1..=8).contains(&threads));
    }

    #[test]
    fn single_and_batched_target_measures_agree() {
        let dir = tempfile::tempdir().unwrap();
        let mut targets = Vec::new();
        for name in ["a", "b", "c"] {
            let target = dir.path().join(name);
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("artifact.bin"), vec![9u8; 4096]).unwrap();
            targets.push(target);
        }
        let singles: Vec<TargetStats> = targets.iter().map(|t| measure_single_target(t)).collect();
        assert!(singles.iter().all(|s| !s.uncertain && s.entries == 1));
        let indexed: Vec<(usize, PathBuf)> = targets.iter().cloned().enumerate().collect();
        let batched = measure_many_targets(&indexed);
        assert_eq!(batched.len(), 3);
        for ((_, batch), single) in batched.iter().zip(singles.iter()) {
            assert!(!batch.uncertain);
            assert_eq!(batch.entries, single.entries);
            assert_eq!(batch.bytes, single.bytes);
        }
    }

    #[test]
    fn duplicate_candidate_indices_do_not_panic_and_are_measured_once() {
        // `dua_core::walk_roots` asserts that root indices are unique, so a
        // repeated index used to panic inside the engine. The first occurrence
        // wins and later duplicates are dropped, which also keeps one target's
        // bytes from accumulating into another's shared stats entry.
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("artifact.bin"), vec![9u8; 4096]).unwrap();
        fs::write(second.join("artifact.bin"), vec![9u8; 8192]).unwrap();

        let targets = vec![
            (7usize, first.clone()),
            (7usize, second.clone()),
            (9usize, second.clone()),
        ];
        let measured = measure_many_targets(&targets);
        assert_eq!(measured.len(), 3, "one result per input, in input order");

        // Index 7 is reported twice, measured from the first path only.
        let seven: Vec<&TargetStats> = measured
            .iter()
            .filter(|(i, _)| *i == 7)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(seven.len(), 2);
        assert!(seven.iter().all(|s| !s.uncertain && s.entries == 1));
        assert!(
            seven.windows(2).all(|w| w[0].bytes == w[1].bytes),
            "the duplicated index must not accumulate bytes twice"
        );
        assert_eq!(seven[0].bytes, measure_single_target(&first).bytes);

        // The distinct index is unaffected.
        let nine = measured.iter().find(|(i, _)| *i == 9).unwrap();
        assert_eq!(nine.1.bytes, measure_single_target(&second).bytes);
    }

    #[test]
    fn the_batched_measurement_returns_its_results_in_input_order() {
        // The contract is "one result per input, in input order". A caller
        // that zipped the results with its own inputs would otherwise
        // attribute one target's measurement to another target.
        let dir = tempfile::tempdir().unwrap();
        let mut targets = Vec::new();
        for name in ["a", "b", "c"] {
            let target = dir.path().join(name);
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("artifact.bin"), vec![9u8; 4096]).unwrap();
            targets.push(target);
        }
        // Deliberately not in index order.
        let indexed = vec![
            (2usize, targets[0].clone()),
            (0usize, targets[1].clone()),
            (1usize, targets[2].clone()),
        ];
        let measured = measure_many_targets(&indexed);
        assert_eq!(measured.len(), 3, "one result per input");
        assert_eq!(
            measured.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![2, 0, 1],
            "the results come back in the order the inputs were given"
        );
        for ((index, batch), (input_index, target)) in measured.iter().zip(indexed.iter()) {
            assert_eq!(*index, *input_index);
            let single = measure_single_target(target);
            assert_eq!(batch.bytes, single.bytes);
            assert_eq!(batch.entries, single.entries);
        }
    }

    #[test]
    fn a_package_directory_under_the_root_is_reported() {
        // `cargo clean --target-dir <root>` removes the whole root, so a
        // package directory inside it means the root holds checked-in
        // sources and not only build output. The measurement is where the
        // shape is caught, because the classification works from the resolved
        // universe and cannot see it.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir_all(target.join("nested/src")).unwrap();
        fs::write(target.join("nested/Cargo.toml"), b"[package]\n").unwrap();
        fs::write(target.join("artifact.bin"), vec![9u8; 4096]).unwrap();
        let stats = measure_single_target(&target);
        assert_eq!(
            stats.package_dir.as_deref(),
            Some(target.join("nested").as_path()),
            "a manifest under the root makes its directory a package directory"
        );

        // A manifest directly under the root makes the root itself one.
        let with_manifest = dir.path().join("root-with-manifest");
        fs::create_dir_all(&with_manifest).unwrap();
        fs::write(with_manifest.join("Cargo.toml"), b"[package]\n").unwrap();
        let stats = measure_single_target(&with_manifest);
        assert_eq!(
            stats.package_dir.as_deref(),
            Some(with_manifest.as_path()),
            "a manifest at the root makes the root the package directory"
        );

        // The batched measurement reports it too.
        let batched = measure_many_targets(&[(0usize, target.clone())]);
        assert_eq!(
            batched[0].1.package_dir.as_deref(),
            Some(target.join("nested").as_path())
        );
    }

    #[test]
    fn candidate_count_does_not_grow_worker_pool() {
        let dir = tempfile::tempdir().unwrap();
        let targets: Vec<(usize, PathBuf)> = (0..32)
            .map(|i| {
                let target = dir.path().join(format!("target-{i}"));
                fs::create_dir_all(&target).unwrap();
                fs::write(target.join("file.bin"), [i as u8; 64]).unwrap();
                (i, target)
            })
            .collect();
        assert!(worker_threads() <= 8);
        let results = measure_many_targets(&targets);
        assert_eq!(results.len(), 32);
        assert!(results.iter().all(|(_, s)| !s.uncertain && s.entries == 1));
    }

    #[test]
    fn synthetic_wide_and_deep_trees_count_entries() {
        let dir = tempfile::tempdir().unwrap();
        let wide = dir.path().join("wide");
        fs::create_dir(&wide).unwrap();
        for i in 0..64 {
            fs::write(wide.join(format!("file-{i}.bin")), vec![1u8; 128]).unwrap();
        }
        let stats = measure_single_target(&wide);
        assert!(!stats.uncertain);
        assert_eq!(stats.entries, 64);

        let mut deep = dir.path().join("deep");
        for _ in 0..12 {
            deep = deep.join("level");
            fs::create_dir_all(&deep).unwrap();
        }
        fs::write(deep.join("leaf.bin"), vec![2u8; 256]).unwrap();
        let stats = measure_single_target(&dir.path().join("deep"));
        assert!(!stats.uncertain);
        // 12 directories + 1 file beneath the measured root.
        assert_eq!(stats.entries, 13);
    }

    /// The marker memo records both answers.
    ///
    /// *Fails if:* only the positives are recorded. The descend pass asks
    /// whether a directory holds a nested repository and the entry pass asks
    /// again; recording only the positives threw away the answer that
    /// mattered — "no marker here" — so every source directory was read
    /// twice.
    #[test]
    fn the_marker_memo_records_both_answers() {
        let d = tempfile::tempdir().unwrap();
        let plain = d.path().join("plain");
        let nested = d.path().join("nested");
        fs::create_dir_all(&plain).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(".git"), b"").unwrap();
        let markers = NestedRepoMarkers::default();
        assert!(!markers.probe(&plain), "a plain directory holds no marker");
        assert!(markers.probe(&nested), "a nested repository holds a marker");
        let recorded = markers.0.lock().unwrap();
        assert_eq!(
            recorded.get(&plain),
            Some(&false),
            "a negative answer must be recorded: the miss is the common one"
        );
        assert_eq!(recorded.get(&nested), Some(&true));
    }

    #[test]
    fn source_scan_short_circuits_recent_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("fresh.rs"), "recent").unwrap();
        let now = SystemTime::now() + Duration::from_secs(1);
        let start = now;
        let cutoff = now - Duration::from_secs(300);
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        match source_activity(dir.path(), &target, start, cutoff).unwrap() {
            SourceActivity::Recent(_) => {}
            SourceActivity::Quiet(_) => panic!("fresh source must short-circuit"),
        }
    }

    /// The symlink predicate answers the same for every spelling of a link.
    ///
    /// *Fails if:* a spelling that hides the link is not recognised. The two
    /// hiding spellings are `link/` and `link/.`: POSIX `symlink_metadata`
    /// follows the final component for both, so the first probe answers "not
    /// a symlink" about a link. The predicate was introduced as *the*
    /// definition and had no test in this module at all, so both spellings
    /// were unexamined.
    #[test]
    #[cfg(unix)]
    fn path_is_symlink_recognises_every_spelling_of_a_link() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = d.path().join("link");
        symlink(&target, &link).unwrap();

        assert!(path_is_symlink(&link), "the plain spelling");
        assert!(
            path_is_symlink(&link.join("")),
            "a trailing separator must not hide the link"
        );
        assert!(
            path_is_symlink(Path::new(&format!("{}/.", link.display()))),
            "a trailing dot component must not hide the link"
        );

        // The negative cases: a plain directory is not a link, and a path that
        // does not exist is not a link.
        let plain = d.path().join("plain");
        fs::create_dir(&plain).unwrap();
        assert!(!path_is_symlink(&plain), "a plain directory is not a link");
        assert!(
            !path_is_symlink(&d.path().join("nowhere")),
            "a path that does not exist is not a link"
        );
    }

    /// A directory whose name ends in a backslash is not a symlink.
    ///
    /// *Fails if:* the trailing-separator trimming is not platform-aware. `\`
    /// is a separator on Windows and an ordinary filename character on Unix,
    /// so trimming it unconditionally rewrites the path to a *different* one:
    /// the predicate then answered about some other directory and reported a
    /// symlink that was not there. Every caller treats a true answer as "this
    /// is a symlink", so a real directory was refused as one.
    #[test]
    #[cfg(unix)]
    fn a_unix_directory_named_with_a_trailing_backslash_is_not_a_symlink() {
        let d = tempfile::tempdir().unwrap();
        let plain = d.path().join("link\\");
        fs::create_dir_all(&plain).unwrap();
        assert!(
            plain.exists(),
            "the fixture is a real directory with a backslash in its name"
        );
        assert!(
            !path_is_symlink(&plain),
            "a backslash is an ordinary filename character on Unix"
        );
    }

    /// A link whose name is not valid UTF-8 is still a link.
    ///
    /// *Fails if:* the spelling arithmetic goes through a lossy string. The
    /// bytes of a name that is not valid UTF-8 do not survive
    /// `to_string_lossy`, so the trimmed probe stat'ed a path that does not
    /// exist and the predicate answered "not a symlink" — about a spelling
    /// that hides a link, which is exactly what the guard exists to catch.
    ///
    /// The gate is `target_os = "linux"`, not `unix`: it is the *fixture* that
    /// is Linux-only — a file name that is not valid UTF-8 is not
    /// representable on macOS's filesystem. The behaviour under test is
    /// platform-independent, and the other two spellings are covered by the
    /// unix-gated test above.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_non_utf8_link_spelled_with_a_trailing_separator_is_still_a_symlink() {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target");
        fs::create_dir(&target).unwrap();
        let name = std::ffi::OsString::from_vec(b"bad-\xff".to_vec());
        let link = d.path().join(name);
        symlink(&target, &link).unwrap();
        assert!(path_is_symlink(&link), "the plain spelling");

        // The trailing separator is what hides the link from `symlink_metadata`.
        // It is built with `join`, not with `display`, so the bytes of the
        // name survive: a lossy rendering would spell a different path.
        let slashed = link.join("");
        assert!(
            path_is_symlink(&slashed),
            "a trailing separator must not hide a link whose name is not UTF-8"
        );
    }
}
