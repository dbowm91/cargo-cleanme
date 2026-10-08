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
        Invocation::Config(command) => run_config(command, &path, options.format),
        Invocation::Scan(intent) => {
            run_scan(intent, &path, options, EmitReport::Yes).map(|outcome| outcome.code)
        }
        Invocation::Cleanup(request) => run_cleanup(request, &path, options),
    }
}

fn run_update(dry_run: bool, format: OutputFormat) -> Result<i32, AppError> {
    let plan = cargo_cleanme::update::update(dry_run)?;
    if format == OutputFormat::Json {
        // The stream, trailing newline included, comes from the library seam the
        // contract tests drive, so "one JSON document on stdout" is a property
        // this function holds rather than a convention it happens to follow.
        print!(
            "{}",
            cargo_cleanme::output::update_json_stream(&plan, dry_run)
        );
    } else if format == OutputFormat::Log {
        // One bounded line, never the human paragraphs below: a scheduler
        // tailing this stream gets `key=value` for every `op` the log format
        // documents, not prose.
        println!(
            "{}",
            cargo_cleanme::output::log::update(
                &plan.from_version,
                &plan.to_version,
                &plan.target,
                dry_run,
            )
        );
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

fn run_config(
    command: ConfigCommand,
    path: &std::path::Path,
    format: OutputFormat,
) -> Result<i32, AppError> {
    // `--format log` promises exactly one ASCII key=value line and no paths or
    // config text. `config path` and `config show` are nothing *but* a path and
    // config text, so in that format they report the action and leave their
    // content to the human and JSON forms.
    let log = format == OutputFormat::Log;
    match command {
        ConfigCommand::Path => {
            if log {
                println!("{}", cargo_cleanme::output::log::config("path"));
            } else {
                println!("{}", path.display());
            }
        }
        ConfigCommand::Show => {
            let shown = config::show(&config::load_or_create(path)?)?;
            if log {
                println!("{}", cargo_cleanme::output::log::config("show"));
            } else {
                println!("{shown}");
            }
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
            if log {
                println!("{}", cargo_cleanme::output::log::config("edit"));
            } else {
                println!("edited {}", path.display());
            }
        }
    }
    Ok(0)
}

/// The roots a resolved cleanup intent operates on, plus the machine scope
/// label describing what was actually resolved.
///
/// Admission is provenance-aware (C028): `roots` holds only admitted
/// automatic roots plus strict explicit roots, collapsed after admission so an
/// invalid identity can never be hidden by canonicalization. `omitted_paths`
/// remembers safely omitted automatic roots for the late pre-spawn recheck;
/// `blocked` carries a whole-scope safety block when an automatic root is a
/// symlink or indeterminately unreadable, in which case no cleanup runs.
struct ResolvedCleanup {
    roots: Vec<std::path::PathBuf>,
    scope_label: &'static str,
    state_generation: Option<u64>,
    omitted_diagnostics: Vec<String>,
    omitted: Vec<cargo_cleanme::policy::AutomaticOmission>,
    blocked: Option<cargo_cleanme::cleanup::ScopeBlock>,
    /// True for Routine/Full scopes whose roots are automatic hints; false
    /// for explicit scopes where every defect stays a fatal usage error.
    automatic: bool,
}

fn run_cleanup(
    request: CleanupRequest,
    config_path: &std::path::Path,
    options: RunOptions,
) -> Result<i32, AppError> {
    let config = config::load_or_create(config_path)?;
    let mode = request.mode;
    let resolved = resolve_cleanup_roots(&request.scope, config_path, &config, options)?;
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
    // Bounded, deterministic stderr notes for safely omitted automatic roots.
    // Human mode may name paths; log mode carries only the count in its one
    // line, so paths never reach a retained history pane.
    if !resolved.omitted_diagnostics.is_empty() && options.format != OutputFormat::Log {
        for diagnostic in &resolved.omitted_diagnostics {
            eprintln!("cargo-cleanme: {diagnostic}");
        }
    }
    if let Some(block) = resolved.blocked.clone() {
        // An automatic root is a symlink or indeterminately unreadable:
        // omission could conceal an expected scan tree, so nothing is
        // traversed and no Cargo process runs in any mode. This is a typed
        // whole-scope block (exit 1), never a fatal usage error.
        let report = cargo_cleanme::cleanup::CleanReport {
            results: Vec::new(),
            diagnostics: resolved.omitted.len(),
            failed: 0,
            mode,
            counters: Default::default(),
            scope_blocked: Some(block),
            unresolved_ownership: Vec::new(),
            selected_roots: resolved.roots.clone(),
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: Some(policy),
            selector,
        };
        emit_cleanup(&report, &resolved, &resolved.roots, options, "");
        return Ok(1);
    }
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
    // The bar must come down on the failure path too. `?` returned to the
    // top-level error reporter with the bar still live, and `BarState::drop`
    // *finishes* the bar rather than clearing it, so the retained line stayed
    // on screen above the `cargo-cleanme: …` message.
    let premise = cargo_cleanme::cleanup::AdmissionPremise {
        omitted: resolved.omitted.clone(),
    };
    let cleanup = run_engine_guarded(
        &resolved,
        &config,
        mode,
        &renderer,
        &policy,
        selector.clone(),
        Some(premise),
    );
    renderer.finish_and_clear();
    let report = cleanup?;
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

/// Run the combined cleanup engine against admitted roots, defending the
/// narrow race in which an admitted automatic root disappears between
/// admission and the engine's own strict validation.
///
/// On a fatal `InvalidRoot` over an automatic scope, the whole automatic set
/// is reclassified once with fresh metadata and — only when the offender now
/// proves safely omittable — the engine is re-run over the rebuilt universe
/// from scratch. Any other outcome becomes a typed whole-scope block, never a
/// silent sibling cleanup against a stale proof. Explicit scopes propagate
/// the fatal error unchanged.
#[allow(clippy::too_many_arguments)]
fn run_engine_guarded(
    resolved: &ResolvedCleanup,
    config: &config::Config,
    mode: cargo_cleanme::cleanup::CleanMode,
    renderer: &cargo_cleanme::progress::IndicatifRenderer,
    policy: &cargo_cleanme::cleanup::CleanupPolicy,
    selector: Option<cargo_cleanme::cleanup::CleanupSelector>,
    premise: Option<cargo_cleanme::cleanup::AdmissionPremise>,
) -> Result<cargo_cleanme::cleanup::CleanReport, AppError> {
    let attempt = |roots: &[std::path::PathBuf],
                   premise: Option<cargo_cleanme::cleanup::AdmissionPremise>| {
        cargo_cleanme::cleanup::clean_with_roots_policy_selector_guarded(
            roots,
            config.scan.recency_seconds,
            &config.cleanup.allowed_output_roots,
            mode,
            &cargo_cleanme::cleanup::SystemCleanupRunner,
            renderer,
            policy,
            selector.clone(),
            premise,
        )
    };
    match attempt(&resolved.roots, premise.clone()) {
        Ok(report) => Ok(report),
        Err(AppError::InvalidRoot { path, reason }) if resolved.automatic => {
            let entries: cargo_cleanme::policy::CleanupCandidates = resolved
                .roots
                .iter()
                .cloned()
                .map(|r| (r, cargo_cleanme::policy::RootProvenance::Automatic))
                .collect();
            match cargo_cleanme::policy::classify_cleanup_roots(entries) {
                Ok(reclassified)
                    if reclassified.blocked.is_empty()
                        && reclassified
                            .omitted_paths()
                            .iter()
                            .any(|p| p.display().to_string() == path) =>
                {
                    let omitted = reclassified.omitted.clone();
                    let admitted = collapse_roots(reclassified.admitted);
                    if admitted.is_empty() {
                        return Ok(cargo_cleanme::cleanup::CleanReport {
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
                            effective_policy: Some(policy.clone()),
                            selector: selector.clone(),
                        });
                    }
                    let rebuilt = cargo_cleanme::cleanup::AdmissionPremise { omitted };
                    attempt(&admitted, Some(rebuilt))
                }
                _ => Ok(cargo_cleanme::cleanup::CleanReport {
                    results: Vec::new(),
                    diagnostics: resolved.omitted.len(),
                    failed: 0,
                    mode,
                    counters: Default::default(),
                    scope_blocked: Some(cargo_cleanme::cleanup::ScopeBlock::new(
                        cargo_cleanme::cleanup::ScopeBlockReason::IncompleteDiscovery,
                        format!(
                            "cleanup root {path} became invalid during admission ({reason}); no cleanup commands were run"
                        ),
                    )),
                    unresolved_ownership: Vec::new(),
                    selected_roots: resolved.roots.clone(),
                    discovered_manifests: 0,
                    resolved_workspaces: 0,
                    units_considered: 0,
                    effective_policy: Some(policy.clone()),
                    selector: selector.clone(),
                }),
            }
        }
        Err(e) => Err(e),
    }
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
        CleanupScope::Root(root) => {
            // Explicit roots stay authoritative: no omission, no fallback to
            // a smaller implicit set. The engine's strict validation reports
            // the defect as a fatal usage error.
            let absolute = cli::absolutize_root(root);
            let classified = cargo_cleanme::policy::classify_cleanup_roots(vec![(
                absolute,
                cargo_cleanme::policy::RootProvenance::Explicit,
            )])?;
            Ok(ResolvedCleanup {
                roots: collapse_roots(classified.admitted),
                scope_label: "explicit",
                state_generation: None,
                omitted_diagnostics: Vec::new(),
                omitted: Vec::new(),
                blocked: None,
                automatic: false,
            })
        }
        CleanupScope::Full => {
            // The Full reconciliation here is a *side effect of choosing cleanup
            // roots*, not something the user asked to see. Emitting its report
            // would put an unrelated document ahead of the cleanup report on
            // stdout and, in JSON mode, would emit two JSON documents — breaking
            // the one-document-per-invocation property the automation contract
            // depends on. Diagnostics and --stats still reach stderr.
            //
            // C028: roots come from the in-memory generation just reconciled,
            // never from a disk reload that could be an older generation when
            // publication failed, was unsupported, or was unavailable. No
            // fresh generation means no cleanup.
            let outcome = run_scan(ScanIntent::Full, config_path, options, EmitReport::No)?;
            if !outcome.publish_ok {
                eprintln!(
                    "cargo-cleanme: discovery state was not saved; cleaning the reconciled in-memory generation only"
                );
            }
            let (entries, generation) =
                cargo_cleanme::policy::full_cleanup_roots(outcome.code, outcome.full_state)?;
            Ok(admit_automatic(entries, "full", generation))
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
            match policy.scope {
                // The configured-root override is the scope that gets
                // cleaned. Discarding it produced a bare maintenance run that
                // reported a successful Explicit no-op it never performed.
                ScanScope::Explicit(root) => {
                    let classified = cargo_cleanme::policy::classify_cleanup_roots(vec![(
                        root,
                        cargo_cleanme::policy::RootProvenance::Explicit,
                    )])?;
                    Ok(ResolvedCleanup {
                        roots: collapse_roots(classified.admitted),
                        scope_label: "explicit",
                        state_generation: match cargo_cleanme::discovery_state::load_default() {
                            cargo_cleanme::discovery_state::StateLoad::Loaded(state) => {
                                state.last_full_at
                            }
                            _ => None,
                        },
                        omitted_diagnostics: Vec::new(),
                        omitted: Vec::new(),
                        blocked: None,
                        automatic: false,
                    })
                }
                ScanScope::ExplicitRoots(roots) => {
                    let entries = roots
                        .into_iter()
                        .map(|r| (r, cargo_cleanme::policy::RootProvenance::Explicit))
                        .collect();
                    let classified = cargo_cleanme::policy::classify_cleanup_roots(entries)?;
                    Ok(ResolvedCleanup {
                        roots: collapse_roots(classified.admitted),
                        scope_label: "explicit",
                        state_generation: None,
                        omitted_diagnostics: Vec::new(),
                        omitted: Vec::new(),
                        blocked: None,
                        automatic: false,
                    })
                }
                ScanScope::Routine(_) => {
                    // C028: cleanup admission classifies the raw candidates —
                    // never the canonicalized `policy.scope` set, whose
                    // collapse would already have resolved a symlinked root
                    // into its target and hidden the invalid identity.
                    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
                    let Some(home) = home else {
                        return Ok(admit_automatic(Vec::new(), "routine", None));
                    };
                    let (entries, warning) = cargo_cleanme::policy::routine_cleanup_candidates(
                        &home,
                        config.scan.learned_root_retention_days as u16,
                        cargo_cleanme::discovery_state::load_default(),
                    );
                    if let Some(warning) = warning {
                        eprintln!("cargo-cleanme: {warning}; using seed/configured Routine roots");
                    }
                    let generation = match cargo_cleanme::discovery_state::load_default() {
                        cargo_cleanme::discovery_state::StateLoad::Loaded(state) => {
                            state.last_full_at
                        }
                        _ => None,
                    };
                    Ok(admit_automatic(entries, "routine", generation))
                }
                // A non-Full resolve never produces Global. Failing closed
                // beats silently treating it as "nothing to do".
                ScanScope::Global(_) => Err(AppError::Config(
                    "maintenance scope resolved to a global scan, which is unreachable; \
                         refusing to treat it as an empty cleanup scope"
                        .into(),
                )),
            }
        }
    }
}

/// Admit one automatic (Routine/Full) candidate set: classify by provenance
/// before collapse, collapse admitted paths only, remember omissions for the
/// late pre-spawn recheck, and convert symlink/uncertain roots into a typed
/// whole-scope block instead of a fatal error or a silent scope reduction.
fn admit_automatic(
    entries: cargo_cleanme::policy::CleanupCandidates,
    scope_label: &'static str,
    state_generation: Option<u64>,
) -> ResolvedCleanup {
    let classified = cargo_cleanme::policy::classify_cleanup_roots(entries)
        .expect("automatic classification never returns a fatal error");
    let omitted = classified.omitted.clone();
    let omitted_diagnostics = classified.omission_diagnostics();
    let blocked = if classified.blocked.is_empty() {
        None
    } else {
        let mut paths: Vec<String> = classified
            .blocked
            .iter()
            .map(|b| b.path.display().to_string())
            .collect();
        paths.sort();
        Some(cargo_cleanme::cleanup::ScopeBlock::new(
            cargo_cleanme::cleanup::ScopeBlockReason::IncompleteDiscovery,
            format!(
                "automatic cleanup scope is uncertain for {} root(s); no cleanup commands were run",
                paths.len(),
            ),
        ))
    };
    ResolvedCleanup {
        roots: collapse_roots(classified.admitted),
        scope_label,
        state_generation,
        omitted_diagnostics,
        omitted,
        blocked,
        automatic: true,
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

/// The outcome of one scan: the process exit code plus, for a Full scan,
/// the in-memory reconciled generation and whether its publication survived.
///
/// C028: `clean --full` consumes the generation reconciled by the traversal
/// it just ran — never a disk reload that could be an older generation when
/// publication failed, was unsupported, or was unavailable. A failed save
/// stays a stderr warning for a read-only scan, but a Full cleanup without a
/// fresh in-memory generation is blocked.
struct ScanOutcome {
    code: i32,
    full_state: Option<cargo_cleanme::discovery_state::DiscoveryState>,
    publish_ok: bool,
}

fn run_scan(
    intent: ScanIntent,
    config_path: &std::path::Path,
    options: RunOptions,
    emit_report: EmitReport,
) -> Result<ScanOutcome, AppError> {
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
    let mut full_state: Option<cargo_cleanme::discovery_state::DiscoveryState> = None;
    let mut publish_ok = true;
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
                    // The proven in-memory generation travels with this scan
                    // outcome regardless of what publication does next, so a
                    // Full cleanup can consume exactly what the traversal
                    // proved even when the save fails.
                    full_state = Some(next.clone());
                    match cargo_cleanme::discovery_state::publish(&next) {
                        Ok(()) => {
                            if replacing_invalid {
                                eprintln!(
                                    "cargo-cleanme: replaced unusable discovery state after successful Full reconciliation"
                                );
                            }
                        }
                        Err(error) => {
                            publish_ok = false;
                            eprintln!("cargo-cleanme: discovery state was not saved: {error}")
                        }
                    }
                }
                cargo_cleanme::discovery_state::Reconciliation::NoPublication(reason) => {
                    publish_ok = false;
                    eprintln!("cargo-cleanme: discovery state was not reconciled: {reason}")
                }
            }
        } else {
            publish_ok = false;
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
        return Ok(ScanOutcome {
            code: 1,
            full_state,
            publish_ok,
        });
    }
    Ok(ScanOutcome {
        code: 0,
        full_state,
        publish_ok,
    })
}
