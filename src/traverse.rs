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
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

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
/// directory, and each answer costs a `read_dir`. Only *positives* are
/// recorded, so the memo is bounded by the number of nested repositories rather
/// than by the size of the tree, and a miss costs exactly what it cost before.
/// Shared across the walk's worker threads, so it is a `Mutex`; the critical
/// section is one hash lookup and never spans I/O.
#[derive(Clone, Default)]
struct NestedRepoMarkers(Arc<Mutex<HashSet<PathBuf>>>);

impl NestedRepoMarkers {
    fn probe(&self, path: &Path) -> bool {
        if self.0.lock().is_ok_and(|known| known.contains(path)) {
            return true;
        }
        if !contains_vcs_marker(path) {
            return false;
        }
        if let Ok(mut known) = self.0.lock() {
            known.insert(path.to_path_buf());
        }
        true
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
        return ordered
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
    ordered
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
pub fn source_activity(
    project: &Path,
    target: &Path,
    start: SystemTime,
    cutoff: SystemTime,
) -> Result<SourceActivity, ()> {
    let project_root = project.to_path_buf();
    let target_root = target.to_path_buf();
    let is_recent = move |time: SystemTime| time >= cutoff || time > start;
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
pub fn workspace_member_activity(
    member_root: &Path,
    output_roots: &[PathBuf],
    start: SystemTime,
    cutoff: SystemTime,
) -> Result<SourceActivity, ()> {
    let root = member_root.to_path_buf();
    let outputs = output_roots.to_vec();
    let outputs_for_closure = outputs.clone();
    let is_recent = move |time: SystemTime| time >= cutoff || time > start;
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
}
