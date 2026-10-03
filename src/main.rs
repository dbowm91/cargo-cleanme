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
                    config::load_or_create(&path)?;
                    let (program, args) = resolve_editor()?;
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
                        return Err(AppError::Config(format!(
                            "editor exited with {status}; config was not reverted: {}",
                            path.display()
                        )));
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
            let roots = if let Some(root) = root {
                vec![cli::absolutize_root(&root)]
            } else if full {
                let code = run_scan(None, true, &path, no_progress, stats)?;
                if code != 0 {
                    return Ok(code);
                }
                cargo_cleanme::discovery_state::load_default()
                    .ok()
                    .flatten()
                    .map(|s| s.learned_roots.into_iter().map(|r| r.path).collect())
                    .unwrap_or_default()
            } else if known {
                let policy = cargo_cleanme::policy::resolve(
                    cargo_cleanme::domain::ScanRequest {
                        cli_root: None,
                        full: false,
                    },
                    &config.scan,
                )?;
                match policy.scope {
                    cargo_cleanme::domain::ScanScope::Routine(roots) => roots,
                    _ => Vec::new(),
                }
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
            if mode == cargo_cleanme::cleanup::CleanMode::Execute && roots.len() > 1 {
                return Err(AppError::Config("multi-root execution is blocked: the fresh ownership proof is bounded to one clean ROOT and cannot qualify sharing across selected roots; run clean ROOT explicitly or use --dryrun".into()));
            }
            let show = cargo_cleanme::progress::should_show_progress(no_progress);
            let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show);
            let wall_start = std::time::Instant::now();
            let mut reports = Vec::new();
            for scope in &roots {
                match cargo_cleanme::cleanup::clean_with(
                    scope,
                    config.scan.recency_seconds,
                    &config.cleanup.allowed_output_roots,
                    mode,
                    &cargo_cleanme::cleanup::SystemCleanupRunner,
                    &renderer,
                ) {
                    Ok(report) => reports.push((scope.clone(), report)),
                    Err(error) => eprintln!("cleanup scope {} failed: {error}", scope.display()),
                }
            }
            renderer.finish_and_clear();
            for (scope, report) in &reports {
                println!("scope {}\n{}", scope.display(), report.render());
            }
            if stats {
                let elapsed = wall_start.elapsed();
                // C003 §7.6: cleanup --stats must account for the final
                // ownership-universe proof work, not only the initial scan.
                eprintln!(
                    "cleanup stats: mode={:?} {} {} {} {} elapsed={:.2}s",
                    mode,
                    reports
                        .first()
                        .map(|r| r.1.counters.stats_line())
                        .unwrap_or_default(),
                    reports
                        .first()
                        .map(|r| r.1.counters.timings_line())
                        .unwrap_or_default(),
                    reports
                        .first()
                        .map(|r| r.1.counters.proof_stats_line())
                        .unwrap_or_default(),
                    reports
                        .first()
                        .map(|r| r.1.counters.proof_timings_line())
                        .unwrap_or_default(),
                    elapsed.as_secs_f64(),
                );
            }
            Ok(if reports.iter().any(|(_, report)| report.failed > 0) {
                1
            } else {
                0
            })
        }
        Some(Command::Scan { root, full }) => run_scan(root, full, &path, no_progress, stats),
        None => run_scan(None, false, &path, no_progress, stats),
    }
}
fn resolve_editor() -> Result<(std::path::PathBuf, Vec<String>), AppError> {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            let mut words = split_editor_words(&value).map_err(AppError::Config)?;
            if !words.is_empty() {
                let program = words.remove(0);
                return Ok((program.into(), words));
            }
        }
    }
    for candidate in ["hx", "vim", "vi", "nano"] {
        if let Some(paths) = std::env::var_os("PATH")
            && std::env::split_paths(&paths).any(|p| p.join(candidate).is_file())
        {
            return Ok((candidate.into(), Vec::new()));
        }
    }
    Err(AppError::Config(
        "no editor found; set VISUAL or EDITOR, or install hx, vim, vi, or nano".into(),
    ))
}

fn split_editor_words(spec: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for ch in spec.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        match (quote, ch) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => word.push(ch),
            (Some('"'), '\\') => escaped = true,
            (Some('"'), _) => word.push(ch),
            (Some(_), _) => word.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
                started = true;
            }
            (None, '\\') => {
                escaped = true;
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if escaped || quote.is_some() {
        return Err("VISUAL/EDITOR has an unfinished escape or quote".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
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
    if full || loaded_state.as_ref().is_ok_and(Option::is_some) {
        if let Err(error) = &loaded_state {
            eprintln!("cargo-cleanme: {error}");
        }
        let state = loaded_state.clone().ok().flatten().unwrap_or_default();
        let now = cargo_cleanme::discovery_state::now_seconds();
        let observed: Vec<_> = workspaces.iter().map(|w| w.root.clone()).collect();
        let mut next = state;
        for project in &observed {
            for root in &mut next.learned_roots {
                if project.starts_with(&root.path) {
                    root.last_project_seen_at = now;
                }
            }
        }
        let uncertain_full = full && !diagnostics.is_empty();
        if full && !uncertain_full {
            next.schema_version = 1;
            next.last_full_at = Some(now);
            next.projects = observed
                .iter()
                .map(|workspace| cargo_cleanme::discovery_state::ProjectRecord {
                    workspace: workspace.clone(),
                })
                .collect();
            let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
            let mut learned = Vec::new();
            for project in &observed {
                let candidate = project
                    .parent()
                    .filter(|p| Some(*p) != home.as_deref() && !is_broad_learning_root(p))
                    .unwrap_or(project);
                let path =
                    std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
                learned.push(cargo_cleanme::discovery_state::LearnedRoot {
                    path,
                    last_project_seen_at: now,
                });
            }
            learned.sort_by(|a, b| a.path.cmp(&b.path));
            learned.dedup_by(|a, b| a.path == b.path);
            if !uncertain_full {
                let cutoff =
                    now.saturating_sub(u64::from(c.scan.learned_root_retention_days) * 86400);
                learned.extend(
                    next.learned_roots.into_iter().filter(|r| {
                        r.last_project_seen_at >= cutoff || r.last_project_seen_at > now
                    }),
                );
                learned.sort_by(|a, b| a.path.cmp(&b.path));
                learned.dedup_by(|a, b| a.path == b.path);
                next.learned_roots = learned;
            }
        }
        if loaded_state.is_ok()
            && ((full && !uncertain_full) || !observed.is_empty())
            && let Err(error) = cargo_cleanme::discovery_state::publish(&next)
        {
            eprintln!("cargo-cleanme: discovery state was not saved: {error}");
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
            matches!(
                d.category,
                domain::DiagnosticCategory::PermissionDenied | domain::DiagnosticCategory::Metadata
            )
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
    if full_incomplete {
        return Ok(1);
    }
    Ok(0)
}
fn is_broad_learning_root(path: &std::path::Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    #[cfg(unix)]
    if [
        "/Users",
        "/home",
        "/Volumes",
        "/Applications",
        "/Library",
        "/opt",
    ]
    .iter()
    .any(|p| path == std::path::Path::new(p))
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::split_editor_words;
    #[test]
    fn editor_spec_preserves_quoted_arguments_without_shell_evaluation() {
        assert_eq!(
            split_editor_words("code --wait 'path with spaces'").unwrap(),
            ["code", "--wait", "path with spaces"]
        );
        assert!(split_editor_words("vim 'unfinished").is_err());
    }
}
