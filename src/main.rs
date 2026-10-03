use cargo_cleanme::{
    cli::{self, Cli, Command, ConfigCommand},
    config::{self, ConfigPathResolver},
    error::AppError,
};
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("cargo-cleanme: {e}");
            std::process::exit(2)
        }
    }
}
fn run() -> Result<i32, AppError> {
    let cli = Cli::parse_normalized();
    let path = ConfigPathResolver::new(cli.config.clone()).path()?;
    let no_progress = cli.no_progress;
    let stats = cli.stats;
    match cli.command {
        Some(Command::Config { command }) => {
            match command {
                ConfigCommand::Path => println!("{}", path.display()),
                ConfigCommand::Show => {
                    println!("{}", config::show(&config::load_or_create(&path)?)?)
                }
                ConfigCommand::Edit => {
                    config::ensure_exists(&path)?;
                    let (program, args) =
                        cargo_cleanme::editor::resolve_editor().map_err(AppError::Config)?;
                    let status = std::process::Command::new(&program)
                        .args(args)
                        .arg(&path)
                        .status()
                        .map_err(|e| {
                            AppError::Config(format!(
                                "cannot launch editor {}: {e}",
                                program.display()
                            ))
                        })?;
                    if !status.success() {
                        eprintln!(
                            "cargo-cleanme: editor exited with {status}; config was not reverted: {}",
                            path.display()
                        );
                        return Ok(status.code().unwrap_or(1));
                    }
                    config::load(&path).map_err(|e| {
                        AppError::Config(format!(
                            "edited config {} is invalid: {e}",
                            path.display()
                        ))
                    })?;
                    println!("edited {}", path.display());
                }
            }
            Ok(0)
        }
        Some(Command::Clean {
            root,
            known,
            full,
            dry_run,
            dryrun,
            yes,
        }) => {
            let config = config::load_or_create(&path)?;
            // Distinct spellings, distinct semantics:
            // `--dry-run` (Cargo preview, default) vs `--dryrun` (simulation).
            let mode = if yes {
                cargo_cleanme::cleanup::CleanMode::Execute
            } else if dryrun {
                cargo_cleanme::cleanup::CleanMode::Simulate
            } else {
                // Default and explicit `--dry-run` both preview via Cargo.
                let _ = dry_run;
                cargo_cleanme::cleanup::CleanMode::Preview
            };
            // Accept a relative CLI root for ergonomics, but keep the
            // cleanup safety boundary absolute.
            let (roots, state_generation) = if let Some(root) = root {
                (vec![cli::absolutize_root(&root)], None)
            } else if full {
                let code = run_scan(None, true, &path, no_progress, stats)?;
                if code != 0 {
                    return Ok(code);
                }
                let state =
                    cargo_cleanme::discovery_state::load_default().map_err(AppError::Config)?;
                let generation = state.as_ref().and_then(|s| s.last_full_at);
                (
                    state
                        .map(|s| s.learned_roots.into_iter().map(|r| r.path).collect())
                        .unwrap_or_default(),
                    generation,
                )
            } else if known {
                let policy = cargo_cleanme::policy::resolve(
                    cargo_cleanme::domain::ScanRequest {
                        cli_root: None,
                        full: false,
                    },
                    &config.scan,
                )?;
                let roots = match policy.scope {
                    cargo_cleanme::domain::ScanScope::Routine(roots) => roots,
                    _ => Vec::new(),
                };
                let generation = cargo_cleanme::discovery_state::load_default()
                    .ok()
                    .flatten()
                    .and_then(|s| s.last_full_at);
                (roots, generation)
            } else {
                return Err(AppError::Config(
                    "clean requires ROOT, --known, or --full".into(),
                ));
            };
            let roots = collapse_roots(roots);
            if roots.is_empty() {
                println!("no bounded cleanup roots are known; no cleanup commands were run");
                return Ok(0);
            }
            let show = cargo_cleanme::progress::should_show_progress(no_progress);
            let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show);
            let wall_start = std::time::Instant::now();
            let report = cargo_cleanme::cleanup::clean_with_roots(
                &roots,
                config.scan.recency_seconds,
                &config.cleanup.allowed_output_roots,
                mode,
                &cargo_cleanme::cleanup::SystemCleanupRunner,
                &renderer,
            )?;
            renderer.finish_and_clear();
            if let Some(generation) = state_generation {
                println!("state generation last_full_at={generation} (Unix seconds)");
            }
            println!(
                "combined roots {}\n{}",
                roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                report.render()
            );
            if stats {
                let elapsed = wall_start.elapsed();
                // C003 §7.6: cleanup --stats must account for the final
                // ownership-universe proof work, not only the initial scan.
                eprintln!(
                    "cleanup stats: mode={:?} {} {} {} {} elapsed={:.2}s",
                    mode,
                    report.counters.stats_line(),
                    report.counters.timings_line(),
                    report.counters.proof_stats_line(),
                    report.counters.proof_timings_line(),
                    elapsed.as_secs_f64(),
                );
            }
            Ok(if report.failed > 0 { 1 } else { 0 })
        }
        Some(Command::Scan { root, full }) => run_scan(root, full, &path, no_progress, stats),
        None => run_scan(None, false, &path, no_progress, stats),
    }
}
fn collapse_roots(roots: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<_> = roots
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    roots.sort();
    roots.dedup();
    let mut collapsed: Vec<std::path::PathBuf> = Vec::new();
    for path in roots {
        if !collapsed.iter().any(|parent| path.starts_with(parent)) {
            collapsed.push(path);
        }
    }
    collapsed
}
fn run_scan(
    root: Option<std::path::PathBuf>,
    full: bool,
    config_path: &std::path::Path,
    no_progress: bool,
    stats: bool,
) -> Result<i32, AppError> {
    use cargo_cleanme::{domain, progress};
    use std::time::{Duration, SystemTime};

    let scan_start = SystemTime::now();
    let wall_start = std::time::Instant::now();
    let c = config::load_or_create(config_path)?;
    let policy = cargo_cleanme::policy::resolve(
        domain::ScanRequest {
            cli_root: root,
            full,
        },
        &c.scan,
    )?;
    let recency = policy.recency;

    // Inline progress is transient stderr state; final report remains stdout.
    // Non-TTY or `--no-progress` forces the no-op path without control noise.
    let show = progress::should_show_progress(no_progress);
    let renderer = progress::IndicatifRenderer::new(!show);
    // Progress-render failure must degrade to hidden rather than fail a scan.
    // Construction is infallible; refresh throttling bounds overhead.
    let observer: &dyn progress::ProgressObserver = &renderer;
    observer.phase(progress::ScanPhase::Discovery);

    // Manifest-first discovery (no sibling target required).
    let discovery_start = std::time::Instant::now();
    let discovered =
        cargo_cleanme::discovery::discover_manifests_with_attribution(&policy, observer, stats)?;
    let discovery_elapsed = discovery_start.elapsed();
    let mut counters = discovered.counters.clone();
    counters.discovery_nanos = counters
        .discovery_nanos
        .saturating_add(discovery_elapsed.as_nanos() as u64);
    let uncertainty: Vec<std::path::PathBuf> = discovered
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.category,
                domain::DiagnosticCategory::PermissionDenied
                    | domain::DiagnosticCategory::Metadata
                    | domain::DiagnosticCategory::Vanished
            )
        })
        .filter_map(|d| d.path.clone())
        .collect();
    let mut diagnostics: Vec<domain::ScanDiagnostic> = discovered.diagnostics;
    let manifests = discovered.manifests;
    counters.manifests_found = manifests.len() as u64;

    observer.phase(progress::ScanPhase::Resolution);
    let resolution_start = std::time::Instant::now();
    let runner = cargo_cleanme::workspace::SystemCargoRunner;
    let workspaces = cargo_cleanme::workspace::resolve_workspaces(
        &manifests,
        &runner,
        &mut counters,
        &mut diagnostics,
        observer,
    );
    let _resolution_elapsed = resolution_start.elapsed();

    let loaded_state = cargo_cleanme::discovery_state::load_default();
    let mut state_reconciled = !full;
    if full || loaded_state.as_ref().is_ok_and(Option::is_some) {
        let prior = match &loaded_state {
            Ok(Some(state)) => Some(state.clone()),
            Ok(None) => Some(cargo_cleanme::discovery_state::DiscoveryState::default()),
            Err(error) => {
                eprintln!("cargo-cleanme: {error}; discovery state was not changed");
                None
            }
        };
        if let Some(prior) = prior {
            let now = cargo_cleanme::discovery_state::now_seconds();
            if full {
                let mut resolved_by_manifest = std::collections::HashMap::new();
                for ws in &workspaces {
                    for member in &ws.members {
                        resolved_by_manifest.insert(
                            std::fs::canonicalize(&member.manifest_path)
                                .unwrap_or_else(|_| member.manifest_path.clone()),
                            ws.root.clone(),
                        );
                    }
                }
                let observations: Vec<_> = manifests
                    .iter()
                    .map(|manifest| {
                        let key =
                            std::fs::canonicalize(manifest).unwrap_or_else(|_| manifest.clone());
                        cargo_cleanme::discovery_state::ProjectObservation {
                            manifest: key.clone(),
                            workspace: resolved_by_manifest.get(&key).cloned(),
                        }
                    })
                    .collect();
                let complete = !diagnostics.iter().any(|d| {
                    d.category == domain::DiagnosticCategory::PlatformRoot
                        && d.severity == domain::DiagnosticSeverity::Error
                });
                match cargo_cleanme::discovery_state::reconcile_full(
                    &prior,
                    &observations,
                    &uncertainty,
                    complete,
                    now,
                    c.scan.learned_root_retention_days,
                    directories::BaseDirs::new()
                        .map(|b| b.home_dir().to_path_buf())
                        .as_deref(),
                ) {
                    cargo_cleanme::discovery_state::Reconciliation::Publish(next) => {
                        match cargo_cleanme::discovery_state::publish(&next) {
                            Ok(()) => state_reconciled = true,
                            Err(error) => {
                                eprintln!("cargo-cleanme: discovery state was not saved: {error}")
                            }
                        }
                    }
                    cargo_cleanme::discovery_state::Reconciliation::NoPublication(reason) => {
                        eprintln!("cargo-cleanme: discovery state was not reconciled: {reason}")
                    }
                }
            } else {
                let observed: Vec<_> = workspaces.iter().map(|w| w.root.clone()).collect();
                let mut next = prior;
                for project in &observed {
                    for root in &mut next.learned_roots {
                        if project.starts_with(&root.path) {
                            root.last_project_seen_at = root.last_project_seen_at.max(now);
                        }
                    }
                }
                if !observed.is_empty()
                    && let Err(error) = cargo_cleanme::discovery_state::publish(&next)
                {
                    eprintln!("cargo-cleanme: discovery state was not saved: {error}");
                }
            }
        }
    }

    // Physical grouping before sizing; equal/nested output counted once.
    // Determinate analysis total is announced through the observer trait
    // (C002 §7.5); `analyze_groups` also announces it and advances via
    // `unit_completed`, so scan progress is genuinely determinate.
    // Phase first (re-scopes totals), then total (starts fresh scope).
    let groups = cargo_cleanme::workspace::build_groups(&workspaces);
    observer.phase(progress::ScanPhase::Analysis);
    observer.units_total(progress::ScanPhase::Analysis, groups.len() as u64);

    let clock_cutoff = scan_start
        .checked_sub(recency)
        .ok_or_else(|| AppError::Config("recency window exceeds system time range".into()))?;

    let physical = cargo_cleanme::workspace::analyze_groups(
        &workspaces,
        groups,
        scan_start,
        clock_cutoff,
        recency,
        &mut counters,
        &mut diagnostics,
        observer,
    );

    let full_incomplete = full
        && diagnostics.iter().any(|d| {
            d.category == domain::DiagnosticCategory::PlatformRoot
                && d.severity == domain::DiagnosticSeverity::Error
        });
    // Deterministic final report: every group, deduplicated total, stable order.
    let mut report = domain::ScanReport {
        eligible: Vec::new(),
        groups: physical
            .into_iter()
            .map(|g| domain::EligibleOutputGroup {
                display_path: g.display_path,
                workspace_roots: g.owners.iter().map(|w| w.0.clone()).collect(),
                physical_paths: g.physical_paths,
                bytes: g.bytes,
                metric: g.metric,
                newest_mtime: g.newest_mtime,
                artifact_entries: g.artifact_entries,
                ownership: g.ownership,
            })
            .collect(),
        diagnostics,
        discovered: manifests.len() as u64,
        visited_entries: discovered.visited_entries,
        counters: counters.clone(),
    };

    observer.phase(progress::ScanPhase::Reporting);
    renderer.finish_and_clear();

    // M005A total is an inventory estimate, never recovered bytes.
    println!("{}", cargo_cleanme::report::render(&mut report));
    if !report.diagnostics.is_empty() {
        eprintln!(
            "{} filesystem diagnostics; rerun with a bounded root if needed",
            report.diagnostics.len()
        );
    }
    // C002 §7.8: detailed counters/timings are opt-in via `--stats` on
    // stderr (never stdout). Default scan emits no debug counter line.
    // `--no-progress --stats` is the canonical benchmark/debug combination.
    if stats {
        let elapsed = wall_start.elapsed();
        eprintln!(
            "scan stats: {} {} elapsed={:.2}s",
            counters.stats_line(),
            counters.timings_line(),
            elapsed.as_secs_f64(),
        );
        if !discovered.top_level_entries.is_empty() {
            let attribution = discovered
                .top_level_entries
                .iter()
                .map(|(path, count)| format!("{:?}={count}", path))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("scan top-level entries: {attribution}");
        }
        let _ = Duration::from_secs(0);
    }
    if full_incomplete || (full && !state_reconciled) {
        return Ok(1);
    }
    Ok(0)
}
