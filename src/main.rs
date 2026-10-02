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
    match cli.command {
        Some(Command::Config { command }) => {
            match command {
                ConfigCommand::Path => println!("{}", path.display()),
                ConfigCommand::Init { force } => {
                    config::init(&path, force)?;
                    println!("created {}", path.display())
                }
                ConfigCommand::Show => println!("{}", config::show(&config::load(&path)?)?),
            }
            Ok(0)
        }
        Some(Command::Clean {
            root,
            dry_run,
            dryrun,
            yes,
        }) => {
            let config = config::load(&path)?;
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
            let effective_root = cli::absolutize_root(&root);
            let show = cargo_cleanme::progress::should_show_progress(no_progress);
            let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show);
            let report = cargo_cleanme::cleanup::clean_with(
                &effective_root,
                config.scan.recency_seconds,
                &config.cleanup.allowed_output_roots,
                mode,
                &cargo_cleanme::cleanup::SystemCleanupRunner,
                &renderer,
            )?;
            renderer.finish_and_clear();
            println!("{}", report.render());
            Ok(if report.failed > 0 { 1 } else { 0 })
        }
        Some(Command::Scan { root }) => run_scan(root, &path, no_progress),
        None => run_scan(None, &path, no_progress),
    }
}
fn run_scan(
    root: Option<std::path::PathBuf>,
    config_path: &std::path::Path,
    no_progress: bool,
) -> Result<i32, AppError> {
    use cargo_cleanme::{domain, progress};
    use std::time::{Duration, SystemTime};

    let scan_start = SystemTime::now();
    let wall_start = std::time::Instant::now();
    let c = config::load(config_path)?;
    let policy = cargo_cleanme::policy::resolve(domain::ScanRequest { cli_root: root }, &c.scan)?;
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
    let discovered = cargo_cleanme::discovery::discover_manifests(&policy, observer)?;
    let mut counters = discovered.counters.clone();
    let mut diagnostics: Vec<domain::ScanDiagnostic> = discovered.diagnostics;
    let manifests = discovered.manifests;
    counters.manifests_found = manifests.len() as u64;

    observer.phase(progress::ScanPhase::Resolution);
    let runner = cargo_cleanme::workspace::SystemCargoRunner;
    let workspaces = cargo_cleanme::workspace::resolve_workspaces(
        &manifests,
        &runner,
        &mut counters,
        &mut diagnostics,
        observer,
    );

    // Physical grouping before sizing; equal/nested output counted once.
    let groups = cargo_cleanme::workspace::build_groups(&workspaces);
    renderer.set_determinate_total(groups.len() as u64);
    observer.phase(progress::ScanPhase::Analysis);

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
    // Performance/debug counters on stderr (never stdout): semantic gates plus
    // wall time. Renderer overhead must be negligible; sustained >5% on the
    // representative explicit-root scan requires investigation.
    {
        let elapsed = wall_start.elapsed();
        eprintln!(
            "scan counters: visited={} pruned={} manifests={} workspaces={} locate={} metadata={} failures={} empty_skipped={} active_skipped={} groups_measured={} bytes={} reportable={} elapsed={:.2}s",
            counters.directories_visited,
            counters.directories_pruned,
            counters.manifests_found,
            counters.unique_workspaces,
            counters.cargo_locate_calls,
            counters.cargo_metadata_calls,
            counters.cargo_failures,
            counters.empty_no_output_skipped + counters.missing_output_skipped,
            counters.active_skipped,
            counters.groups_measured,
            counters.bytes_measured,
            counters.reportable_groups,
            elapsed.as_secs_f64(),
        );
        let _ = Duration::from_secs(0);
    }
    Ok(0)
}
