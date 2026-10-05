use cargo_cleanme::{
    cli::{
        self, CleanupRequest, CleanupScope, Cli, ConfigCommand, Invocation, OutputFormat,
        ScanIntent,
    },
    config::{self, ConfigPathResolver},
    domain::ScanScope,
    error::AppError,
};
fn main() {
    // clap prints its own message and exits on a usage error, so parsing here
    // rather than inside `run` is what lets a fatal error know the output
    // format: log mode has to report it as one bounded line, not as prose.
    let cli = Cli::parse_normalized();
    // Captured before the move so a fatal error can still name the operation.
    let invocation = cli.invocation();
    let format = cli.format;
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if format == OutputFormat::Log {
                // No report exists yet, so there is no summary line to write;
                // this is the single bounded stderr diagnostic instead. It
                // carries a typed code and no text: a retained history pane has
                // no use for a path or a Cargo error.
                eprintln!(
                    "{}",
                    cargo_cleanme::output::log::fatal(operation_name(&invocation), error_code(&e))
                );
            } else {
                eprintln!("cargo-cleanme: {e}");
            }
            std::process::exit(2)
        }
    }
}

/// Stable `op=` value for an invocation, for the one line log mode writes when
/// an operation fails before producing a report.
fn operation_name(invocation: &Invocation) -> &'static str {
    match invocation {
        Invocation::Cleanup(_) => "clean",
        Invocation::Scan(_) => "scan",
        Invocation::Config(_) => "config",
        Invocation::Update { .. } => "update",
    }
}

/// Stable `reason=` code for a fatal error. The prose stays in the human path;
/// this exists so an unattended log can be grepped and alerted on.
fn error_code(error: &AppError) -> &'static str {
    match error {
        AppError::Config(_) | AppError::InvalidRoot { .. } => "config",
        AppError::Io(_) => "io",
        AppError::Scan(_) => "scan",
        AppError::Provenance(_) => "provenance",
        AppError::Update(_) => "update",
    }
}
/// Process-wide output options that apply to every report-producing operation.
#[derive(Clone, Copy, Debug)]
struct RunOptions {
    no_progress: bool,
    stats: bool,
    format: OutputFormat,
}
fn run(cli: Cli) -> Result<i32, AppError> {
    let path = ConfigPathResolver::new(cli.config.clone()).path()?;
    let options = RunOptions {
        no_progress: cli.no_progress,
        stats: cli.stats,
        format: cli.format,
    };
    // One resolved intent per invocation. Bare maintenance, `clean ROOT`,
    // `clean --known`, and `clean --full` all arrive as a CleanupRequest, so
    // they cannot reach different proof, policy, or reporting paths.
    match cli.invocation() {
        Invocation::Update { dry_run } => run_update(dry_run, options.format),
        Invocation::Config(command) => run_config(command, &path),
        Invocation::Scan(intent) => run_scan(intent, &path, options, EmitReport::Yes),
        Invocation::Cleanup(request) => run_cleanup(request, &path, options),
    }
}

fn run_update(dry_run: bool, format: OutputFormat) -> Result<i32, AppError> {
    let plan = cargo_cleanme::update::update(dry_run)?;
    if format == OutputFormat::Json {
        println!("{}", update_json(&plan, dry_run));
    } else if dry_run {
        println!(
            "cargo-cleanme: {} -> {} available for {}",
            plan.from_version, plan.to_version, plan.target
        );
        println!("  release: {}", plan.tag);
        println!("  asset:   {}", plan.asset);
        println!("  install: not changed (--dry-run)");
    } else {
        println!("cargo-cleanme: updated to {}", plan.to_version);
    }
    Ok(0)
}

fn run_config(command: ConfigCommand, path: &std::path::Path) -> Result<i32, AppError> {
    match command {
        ConfigCommand::Path => println!("{}", path.display()),
        ConfigCommand::Show => {
            println!("{}", config::show(&config::load_or_create(path)?)?)
        }
        ConfigCommand::Edit => {
            config::ensure_exists(path)?;
            let (program, args) =
                cargo_cleanme::editor::resolve_editor().map_err(AppError::Config)?;
            let status = std::process::Command::new(&program)
                .args(args)
                .arg(path)
                .status()
                .map_err(|e| {
                    AppError::Config(format!("cannot launch editor {}: {e}", program.display()))
                })?;
            if !status.success() {
                eprintln!(
                    "cargo-cleanme: editor exited with {status}; config was not reverted: {}",
                    path.display()
                );
                return Ok(status.code().unwrap_or(1));
            }
            if !path.exists() {
                // `load` treats an absent file as "all defaults", so a
                // deleted config would silently reset every policy.
                return Err(AppError::Config(format!(
                    "editor removed the config file {}; refusing to treat that as an edit to defaults",
                    path.display()
                )));
            }
            config::load(path).map_err(|e| {
                AppError::Config(format!("edited config {} is invalid: {e}", path.display()))
            })?;
            println!("edited {}", path.display());
        }
    }
    Ok(0)
}

/// The roots a resolved cleanup intent operates on, plus the machine scope
/// label describing what was actually resolved.
struct ResolvedCleanup {
    roots: Vec<std::path::PathBuf>,
    scope_label: &'static str,
    state_generation: Option<u64>,
}

fn run_cleanup(
    request: CleanupRequest,
    config_path: &std::path::Path,
    options: RunOptions,
) -> Result<i32, AppError> {
    let config = config::load_or_create(config_path)?;
    let mode = request.mode;
    let mut resolved = resolve_cleanup_roots(&request.scope, config_path, &config, options)?;
    // Canonicalize, dedupe, and collapse exactly as the combined-root cleanup
    // path requires, so the reported root list, the empty-scope check, and the
    // roots the engine re-collapses all describe the same set.
    resolved.roots = collapse_roots(resolved.roots);
    let configured = &config.cleanup.policy;
    let overrides = &request.overrides;
    let policy = cargo_cleanme::cleanup::CleanupPolicy {
        min_reclaimable_bytes: overrides
            .min_reclaimable_bytes
            .unwrap_or(configured.min_reclaimable_bytes),
        min_inactive_seconds: overrides.older_than.or(configured.min_inactive_seconds),
        include: if overrides.include.is_empty() {
            configured.include.clone()
        } else {
            overrides.include.clone()
        },
        exclude: if overrides.exclude.is_empty() {
            configured.exclude.clone()
        } else {
            overrides.exclude.clone()
        },
    };
    let selector = overrides
        .profile
        .clone()
        .map(cargo_cleanme::cleanup::CleanupSelector::Profile)
        .or_else(|| {
            overrides
                .package
                .clone()
                .map(cargo_cleanme::cleanup::CleanupSelector::Package)
        });
    if resolved.roots.is_empty() {
        // A successful no-op, not an error: there was simply nothing bounded
        // to clean. The report still states the resolved operation, scope, and
        // mode, so a machine consumer reads the same shape either way.
        let empty = cargo_cleanme::cleanup::CleanReport {
            results: Vec::new(),
            diagnostics: 0,
            failed: 0,
            mode,
            counters: Default::default(),
            scope_blocked: None,
            unresolved_ownership: Vec::new(),
            selected_roots: Vec::new(),
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: Some(policy),
            selector,
        };
        emit_cleanup(
            &empty,
            &resolved,
            &[],
            options,
            "no bounded cleanup roots are known; no cleanup commands were run",
        );
        return Ok(0);
    }
    let show = options.format == OutputFormat::Human
        && cargo_cleanme::progress::should_show_progress(options.no_progress);
    let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show);
    let wall_start = std::time::Instant::now();
    let report = cargo_cleanme::cleanup::clean_with_roots_policy_selector(
        &resolved.roots,
        config.scan.recency_seconds,
        &config.cleanup.allowed_output_roots,
        mode,
        &cargo_cleanme::cleanup::SystemCleanupRunner,
        &renderer,
        &policy,
        selector,
    )?;
    renderer.finish_and_clear();
    if options.format == OutputFormat::Human
        && let Some(generation) = resolved.state_generation
    {
        println!("state generation last_full_at={generation} (Unix seconds)");
    }
    emit_cleanup(&report, &resolved, &resolved.roots, options, "");
    if options.stats {
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
    Ok(if report.failed > 0 || report.scope_blocked.is_some() {
        1
    } else {
        0
    })
}

/// Resolve a cleanup scope into concrete bounded roots and its machine label.
///
/// The label always describes the scope that was actually resolved, never the
/// flags that were passed, because it is a machine contract.
fn resolve_cleanup_roots(
    scope: &CleanupScope,
    config_path: &std::path::Path,
    config: &config::Config,
    options: RunOptions,
) -> Result<ResolvedCleanup, AppError> {
    match scope {
        CleanupScope::Root(root) => Ok(ResolvedCleanup {
            // Accept a relative CLI root for ergonomics, but keep the cleanup
            // safety boundary absolute.
            roots: vec![cli::absolutize_root(root)],
            scope_label: "explicit",
            state_generation: None,
        }),
        CleanupScope::Full => {
            // The Full reconciliation here is a *side effect of choosing cleanup
            // roots*, not something the user asked to see. Emitting its report
            // would put an unrelated document ahead of the cleanup report on
            // stdout and, in JSON mode, would emit two JSON documents — breaking
            // the one-document-per-invocation property the automation contract
            // depends on. Diagnostics and --stats still reach stderr.
            let code = run_scan(ScanIntent::Full, config_path, options, EmitReport::No)?;
            if code != 0 {
                // Previously this returned the scan's own exit code silently;
                // an incomplete reconciliation means the learned-root set is
                // not trustworthy enough to clean, and the caller must be told
                // that no cleanup happened rather than seeing a bare exit code.
                return Err(AppError::Config(format!(
                    "full reconciliation reported an incomplete scan (exit {code}); \
                     no cleanup commands were run against its unproven learned roots"
                )));
            }
            let state = match cargo_cleanme::discovery_state::load_default() {
                cargo_cleanme::discovery_state::StateLoad::Loaded(state) => Some(state),
                _ => None,
            };
            Ok(ResolvedCleanup {
                roots: state
                    .as_ref()
                    .map(|s| s.learned_roots.iter().map(|r| r.path.clone()).collect())
                    .unwrap_or_default(),
                scope_label: "full",
                state_generation: state.as_ref().and_then(|s| s.last_full_at),
            })
        }
        // Bare maintenance and `clean` with no scope selector land here and use
        // exactly the same maintenance policy: a configured legacy
        // `scan.root` is an exclusive Explicit override, otherwise bounded
        // seed roots plus active learned roots form the Routine scope.
        CleanupScope::Maintenance => {
            let policy = cargo_cleanme::policy::resolve(
                cargo_cleanme::domain::ScanRequest {
                    cli_root: None,
                    full: false,
                },
                &config.scan,
            )?;
            let (roots, scope_label) = match policy.scope {
                ScanScope::Routine(roots) => (roots, "routine"),
                // The configured-root override is the scope that gets
                // cleaned. Discarding it produced a bare maintenance run that
                // reported a successful Explicit no-op it never performed.
                ScanScope::Explicit(root) => (vec![root], "explicit"),
                ScanScope::ExplicitRoots(roots) => (roots, "explicit"),
                // A non-Full resolve never produces Global. Failing closed
                // beats silently treating it as "nothing to do".
                ScanScope::Global(_) => {
                    return Err(AppError::Config(
                        "maintenance scope resolved to a global scan, which is unreachable; \
                         refusing to treat it as an empty cleanup scope"
                            .into(),
                    ));
                }
            };
            let generation = match cargo_cleanme::discovery_state::load_default() {
                cargo_cleanme::discovery_state::StateLoad::Loaded(state) => state.last_full_at,
                _ => None,
            };
            Ok(ResolvedCleanup {
                roots,
                scope_label,
                state_generation: generation,
            })
        }
    }
}

/// Emit the resolved cleanup report in the selected format.
///
/// The scope label comes from the resolved scope, so a JSON consumer never has
/// to infer operation/scope/mode from argv spelling.
fn emit_cleanup(
    report: &cargo_cleanme::cleanup::CleanReport,
    resolved: &ResolvedCleanup,
    roots: &[std::path::PathBuf],
    options: RunOptions,
    empty_note: &str,
) {
    if options.format == OutputFormat::Json {
        let document = serde_json::to_string(&cargo_cleanme::output::cleanup(
            report,
            resolved.scope_label,
            resolved.state_generation,
        ))
        .expect("the cleanup envelope is built from owned strings and integers");
        println!("{document}");
    } else if options.format == OutputFormat::Log {
        println!(
            "{}",
            cargo_cleanme::output::log::cleanup(report, resolved.scope_label)
        );
    } else if empty_note.is_empty() {
        println!(
            "combined roots {}\n{}",
            roots
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            report.render()
        );
    } else {
        println!("{empty_note}");
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

/// The JSON `scope` label must describe the scope that was actually resolved,
/// not the flags that were passed.
///
/// Deriving it from `full` / `root.is_some()` was wrong for a configured
/// `scan.root`: `policy::resolve` turns that into `ScanScope::Explicit`, so a
/// scan pinned to one root reported `"routine"`. The label is a machine
/// contract (`plans/output-schema-v1.md`), so it is derived from the resolved
/// policy — the single source of truth for what was scanned.
fn scope_label(scope: &ScanScope) -> &'static str {
    match scope {
        ScanScope::Global(_) => "full",
        ScanScope::Explicit(_) | ScanScope::ExplicitRoots(_) => "explicit",
        ScanScope::Routine(_) => "routine",
    }
}

/// Whether a scan writes its report to stdout.
///
/// Not a formatting choice: an *internal* scan — the Full reconciliation
/// `clean --full` runs to refresh learned state — must not write anything to
/// stdout, because the caller is about to write the report the user asked for.
/// Emitting both would put an unrelated document ahead of the cleanup report
/// and, in JSON mode, would emit two JSON documents in one invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EmitReport {
    Yes,
    No,
}

fn run_scan(
    intent: ScanIntent,
    config_path: &std::path::Path,
    options: RunOptions,
    emit_report: EmitReport,
) -> Result<i32, AppError> {
    use cargo_cleanme::{domain, progress};
    use std::time::SystemTime;

    let scan_start = SystemTime::now();
    let wall_start = std::time::Instant::now();
    let c = config::load_or_create(config_path)?;
    // Rootless `scan` is Full and never consults a configured `scan.root`:
    // configuration must not be able to silently narrow the canonical
    // reconciliation command. `scan ROOT` is the explicit CLI override, and
    // `scan --known` is the read-only maintenance scope.
    let (root, full) = match intent {
        ScanIntent::Full => (None, true),
        ScanIntent::Explicit(root) => (Some(root), false),
        ScanIntent::Maintenance => (None, false),
    };
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
    let show = options.format == OutputFormat::Human
        && progress::should_show_progress(options.no_progress);
    let renderer = progress::IndicatifRenderer::new(!show);
    // Progress-render failure must degrade to hidden rather than fail a scan.
    // Construction is infallible; refresh throttling bounds overhead.
    let observer: &dyn progress::ProgressObserver = &renderer;
    observer.phase(progress::ScanPhase::Discovery);

    // Manifest-first discovery (no sibling target required).
    let discovery_start = std::time::Instant::now();
    let discovered = cargo_cleanme::discovery::discover_manifests_with_attribution(
        &policy,
        observer,
        options.stats,
    )?;
    let discovery_elapsed = discovery_start.elapsed();
    let mut counters = discovered.counters.clone();
    counters.discovery_nanos = counters
        .discovery_nanos
        .saturating_add(cargo_cleanme::domain::elapsed_nanos(discovery_elapsed));
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

    use cargo_cleanme::discovery_state::StateLoad;
    let loaded_state = cargo_cleanme::discovery_state::load_default();
    if full {
        let selected = cargo_cleanme::discovery_state::full_reconciliation_prior(&loaded_state);
        if let Some((prior, replacing_invalid)) = selected {
            let now = cargo_cleanme::discovery_state::now_seconds();
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
                    let key = std::fs::canonicalize(manifest).unwrap_or_else(|_| manifest.clone());
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
                c.scan.learned_root_retention_days as u16,
                directories::BaseDirs::new()
                    .map(|b| b.home_dir().to_path_buf())
                    .as_deref(),
            ) {
                cargo_cleanme::discovery_state::Reconciliation::Publish(next) => {
                    match cargo_cleanme::discovery_state::publish(&next) {
                        Ok(()) => {
                            if replacing_invalid {
                                eprintln!(
                                    "cargo-cleanme: replaced unusable discovery state after successful Full reconciliation"
                                );
                            }
                        }
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
            let message = loaded_state
                .diagnostic()
                .unwrap_or_else(|| "discovery state cannot be safely read".into());
            eprintln!("cargo-cleanme: {message}; Full state reconciliation was skipped");
        }
    } else if let StateLoad::Loaded(prior) = loaded_state {
        let now = cargo_cleanme::discovery_state::now_seconds();
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
        counters,
    };

    observer.phase(progress::ScanPhase::Reporting);
    renderer.finish_and_clear();

    // M005A total is an inventory estimate, never recovered bytes.
    if emit_report == EmitReport::No {
        // Suppressed: the report still existed and still drove every decision
        // below; only its rendering is withheld.
    } else if options.format == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string(&cargo_cleanme::output::scan(
                &report,
                scope_label(&policy.scope),
            ))
            .map_err(|e| AppError::Config(format!("cannot serialize JSON report: {e}")))?
        );
    } else if options.format == OutputFormat::Log {
        println!(
            "{}",
            cargo_cleanme::output::log::scan(&report, scope_label(&policy.scope), full_incomplete)
        );
    } else {
        println!("{}", cargo_cleanme::report::render(&mut report));
    }
    // Normal diagnostic fan-out is human-readable detail on stderr. Log mode
    // carries `diagnostics=N` in its one line instead, so a retained history
    // tail is not ten paragraphs long.
    if !report.diagnostics.is_empty() && options.format != OutputFormat::Log {
        eprintln!(
            "{} filesystem diagnostics; rerun with a bounded root if needed",
            report.diagnostics.len()
        );
        // Say what they were: a count alone cannot explain a project that is
        // deliberately not reported, for example one whose declared output root
        // turns out to be its own source tree.
        for diagnostic in report.diagnostics.iter().take(10) {
            eprintln!(
                "  {}: {}{}",
                format!("{:?}", diagnostic.severity).to_lowercase(),
                diagnostic.message,
                diagnostic
                    .path
                    .as_ref()
                    .map(|p| format!(" ({})", p.display()))
                    .unwrap_or_default()
            );
        }
        if report.diagnostics.len() > 10 {
            eprintln!("  … {} more", report.diagnostics.len() - 10);
        }
    }
    // C002 §7.8: detailed counters/timings are opt-in via `--stats` on
    // stderr (never stdout). Default scan emits no debug counter line.
    // `--no-progress --stats` is the canonical benchmark/debug combination.
    if options.stats {
        let elapsed = wall_start.elapsed();
        eprintln!(
            "scan stats: {} {} elapsed={:.2}s",
            report.counters.stats_line(),
            report.counters.timings_line(),
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
    }
    // Exit 1 means "the requested scan did not fully complete": either a Full
    // scan hit an unreadable platform root, or the caller asked for cleanup
    // and cleanup was blocked. It deliberately does NOT cover a failed
    // discovery-state save: `discovery_state` states that state is an
    // optimization only, the warning is already on stderr, and the Routine
    // branch has always ignored the identical failure. Failing the exit code
    // here made the two branches disagree about the same event.
    if full_incomplete {
        return Ok(1);
    }
    Ok(0)
}

/// One stable JSON object for `update`, matching the schema_version discipline
/// every other machine-readable surface in this CLI already follows.
fn update_json(plan: &cargo_cleanme::update::UpdatePlan, dry_run: bool) -> String {
    let mut document = serde_json::Map::new();
    document.insert("schema_version".into(), serde_json::json!(1));
    document.insert(
        "cargo_cleanme_version".into(),
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    document.insert("operation".into(), serde_json::json!("update"));
    document.insert("dry_run".into(), serde_json::json!(dry_run));
    document.insert(
        "result".into(),
        serde_json::json!({
            "from_version": plan.from_version,
            "to_version": plan.to_version,
            "target": plan.target,
            "asset": plan.asset,
            "tag": plan.tag,
            "provenance": plan.provenance.code(),
            "changed": !dry_run,
        }),
    );
    serde_json::Value::Object(document).to_string()
}
