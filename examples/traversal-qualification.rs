//! M006F matched callback-stage harness.
//!
//! Run with: `cargo run --release --example traversal-qualification -- <stage> [workers] [completion]`
//! Stages 1-5 traverse the production Full roots/prunes with increasingly rich
//! callbacks. Stage 6 invokes production manifest discovery and reports its
//! counters. Run each stage with identical arguments and a warm/cold-cache note.
use std::{collections::HashMap, path::PathBuf, time::Instant};

fn entry_within(entry: &dua_core::Entry, root: &std::path::Path, prefix: &std::path::Path) -> bool {
    if entry.depth == 0 {
        return root.starts_with(prefix);
    }
    entry.parent_path.starts_with(prefix)
        || (prefix.parent() == Some(entry.parent_path.as_ref())
            && prefix
                .file_name()
                .is_some_and(|name| name == entry.file_name))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let stage: u8 = args.get(1).and_then(|x| x.parse().ok()).unwrap_or(1);
    let workers: usize = args.get(2).and_then(|x| x.parse().ok()).unwrap_or(8);
    let completion = args.get(3).is_some_and(|x| x == "completion");
    if stage == 6 {
        let policy = cargo_cleanme::policy::resolve(
            cargo_cleanme::domain::ScanRequest {
                cli_root: None,
                full: true,
            },
            &cargo_cleanme::config::Config::default().scan,
        )
        .unwrap();
        let start = Instant::now();
        let result = cargo_cleanme::discovery::discover_manifests_with_attribution(
            &policy,
            &cargo_cleanme::progress::NoopObserver,
            true,
        )
        .unwrap();
        println!(
            "stage=6 elapsed={:.3} visited={} manifests={} attribution_roots={}",
            start.elapsed().as_secs_f64(),
            result.visited_entries,
            result.manifests.len(),
            result.top_level_entries.len()
        );
        return;
    }
    let policy = cargo_cleanme::policy::global_discovery_policy();
    let roots: Vec<_> = policy
        .roots
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.clone()))
        .collect();
    let roots_for_walk = roots.clone();
    let roots_for_descend = roots_for_walk.clone();
    let mut options = dua_core::Options::default();
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        options = options.skip_metadata();
    }
    let started = Instant::now();
    let mut visited = 0u64;
    let mut manifests = Vec::new();
    let mut progress_batch = 0u64;
    let mut attribution: HashMap<(usize, PathBuf), u64> = HashMap::new();
    let cargo_prunes = cargo_cleanme::discovery::cargo_home_prunes();
    let rustup_prunes = cargo_cleanme::discovery::effective_rustup_home()
        .into_iter()
        .collect::<Vec<_>>();
    let pruned = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let pruned_cb = pruned.clone();
    let walk = dua_core::walk_roots(
        roots,
        workers.clamp(1, 32),
        if completion {
            dua_core::Order::Completion
        } else {
            dua_core::Order::ParentFirst
        },
        options,
        move |idx, entry| {
            let Some((_, root)) = roots_for_descend.get(idx) else {
                return false;
            };
            let descend = entry.file_type.is_dir()
                && !entry.file_type.is_symlink()
                && !(entry.depth > 0
                    && (entry.file_name == "target"
                        || [".git", ".hg", ".svn"]
                            .iter()
                            .any(|n| entry.file_name == *n)))
                && !cargo_prunes
                    .iter()
                    .chain(&rustup_prunes)
                    .chain(&policy.global_only_prunes)
                    .any(|p| entry_within(entry, root, p));
            if stage >= 2 && entry.file_type.is_dir() && !descend {
                pruned_cb.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            descend
        },
    );
    for (idx, event) in walk {
        let Ok(entry) = (match event {
            dua_core::RootEvent::Entry(value) => value,
            dua_core::RootEvent::Finished => continue,
        }) else {
            continue;
        };
        visited += 1;
        if stage >= 3 && entry.file_type.is_file() && entry.file_name == "Cargo.toml" {
            manifests.push(entry.path());
        }
        if stage >= 4 {
            progress_batch += 1;
            if progress_batch == 512 {
                std::hint::black_box(progress_batch);
                progress_batch = 0;
            }
        }
        if stage >= 5
            && let Some((_, root)) = roots_for_walk.get(idx)
        {
            let component = if entry.depth == 0 {
                root.clone()
            } else {
                entry
                    .parent_path
                    .strip_prefix(root)
                    .ok()
                    .and_then(|p| p.components().next())
                    .map(|p| root.join(p.as_os_str()))
                    .unwrap_or_else(|| root.clone())
            };
            *attribution.entry((idx, component)).or_default() += 1;
        }
    }
    if stage >= 3 {
        manifests.sort();
        manifests.dedup();
    }
    println!(
        "stage={stage} workers={workers} order={} elapsed={:.3} visited={visited} pruned={} manifests={} attributed_roots={}",
        if completion {
            "completion"
        } else {
            "parent-first"
        },
        started.elapsed().as_secs_f64(),
        pruned.load(std::sync::atomic::Ordering::Relaxed),
        manifests.len(),
        attribution.len()
    );
}
