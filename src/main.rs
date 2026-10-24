use cargo_cleanme::{
    cli::{
        self, CleanupRequest, CleanupScope, Cli, ConfigCommand, Invocation, OutputFormat,
        ScanIntent,
    },
    config::{self, ConfigPathResolver},
    domain::ScanScope,
    error::AppError,
};
use std::collections::BTreeMap;
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
        // Unreachable through `clap`, so no consumer can observe this value.
        Invocation::UsageError(_) => "usage",
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
    /// The detailed human surface. Never affects the JSON or log projections.
    verbose: bool,
}
fn run(cli: Cli) -> Result<i32, AppError> {
    let path = ConfigPathResolver::new(cli.config.clone()).path()?;
    let options = RunOptions {
        no_progress: cli.no_progress,
        stats: cli.stats,
        format: cli.format,
        verbose: cli.verbose,
    };
    // One resolved intent per invocation. Bare maintenance, a bounded `ROOT`,
    // `--full`, and the alias's `clean --known` all arrive as a CleanupRequest,
    // so they cannot reach different proof, policy, or reporting paths.
    match cli.invocation() {
        Invocation::Update { dry_run } => run_update(dry_run, options.format),
        Invocation::Config(command) => run_config(command, &path, options.format),
        Invocation::Scan(intent) => run_scan(
            intent,
            &path,
            options,
            EmitReport::Yes,
            cargo_cleanme::discovery_state::ReconciliationMode::IncompleteDiscard,
        )
        .map(|outcome| outcome.code),
        Invocation::Cleanup(request) => run_cleanup(
            request,
            &path,
            options,
            &cargo_cleanme::cleanup::SystemCleanupRunner,
        ),
        // Unreachable through `clap` (see `Invocation::UsageError`). Reported as
        // a usage error (exit 2) rather than guessing an operation.
        Invocation::UsageError(message) => Err(AppError::Config(message)),
    }
}

/// The log line for an update that had nothing to do.
///
/// The `mode` field is the invocation's own dry-run flag: a run that was asked
/// to simulate says `simulate`, and a run that was asked to execute says
/// `execute`. Hardcoding `simulate` here made the log line describe a
/// different run than the JSON document printed beside it — one machine
/// surface saying "simulation", the other saying `dry_run: false`.
fn update_nothing_to_do_log_line(current: &str, published: &str, dry_run: bool) -> String {
    cargo_cleanme::output::log::update(
        current,
        published,
        cargo_cleanme::update::host_target().unwrap_or("unknown"),
        dry_run,
    )
}

fn run_update(dry_run: bool, format: OutputFormat) -> Result<i32, AppError> {
    let plan = match cargo_cleanme::update::update(dry_run) {
        Ok(plan) => plan,
        // "Already at latest" is success with nothing to do (exit 0), not a
        // usage error (exit 2): schedulers must not alert on it. The JSON
        // branch still emits exactly one document with `changed:false`.
        // The two variants are matched separately (not by string equality on
        // the versions) so `01.02.03`-style spellings still report the exact
        // variant the version comparison produced.
        Err(cargo_cleanme::update::UpdateError::AlreadyCurrent { current, published }) => {
            if format == OutputFormat::Json {
                let target = cargo_cleanme::update::host_target().unwrap_or("unknown");
                let asset = cargo_cleanme::update::asset_for_target(target).unwrap_or("");
                let provenance = std::env::current_exe()
                    .ok()
                    .map(|exe| cargo_cleanme::update::classify_provenance(&exe).code())
                    .unwrap_or("unprovable_ownership");
                let document = serde_json::json!({
                    "schema_version": 1,
                    "cargo_cleanme_version": env!("CARGO_PKG_VERSION"),
                    "operation": "update",
                    "dry_run": dry_run,
                    "result": {
                        "from_version": current,
                        "to_version": published,
                        "target": target,
                        "asset": asset,
                        "tag": format!("v{published}"),
                        "provenance": provenance,
                        "changed": false,
                    }
                });
                println!("{document}");
            } else if format == OutputFormat::Log {
                println!(
                    "{}",
                    update_nothing_to_do_log_line(&current, &published, dry_run)
                );
            } else {
                println!(
                    "cargo-cleanme: already at the latest stable version ({current}; published is {published})"
                );
            }
            return Ok(0);
        }
        Err(cargo_cleanme::update::UpdateError::NewerThanPublished { current, published }) => {
            if format == OutputFormat::Json {
                let target = cargo_cleanme::update::host_target().unwrap_or("unknown");
                let asset = cargo_cleanme::update::asset_for_target(target).unwrap_or("");
                let provenance = std::env::current_exe()
                    .ok()
                    .map(|exe| cargo_cleanme::update::classify_provenance(&exe).code())
                    .unwrap_or("unprovable_ownership");
                let document = serde_json::json!({
                    "schema_version": 1,
                    "cargo_cleanme_version": env!("CARGO_PKG_VERSION"),
                    "operation": "update",
                    "dry_run": dry_run,
                    "result": {
                        "from_version": current,
                        "to_version": published,
                        "target": target,
                        "asset": asset,
                        "tag": format!("v{published}"),
                        "provenance": provenance,
                        "changed": false,
                    }
                });
                println!("{document}");
            } else if format == OutputFormat::Log {
                println!(
                    "{}",
                    update_nothing_to_do_log_line(&current, &published, dry_run)
                );
            } else {
                println!(
                    "cargo-cleanme: this build ({current}) is newer than the published release ({published}); refusing to downgrade it"
                );
            }
            return Ok(0);
        }
        Err(other) => return Err(other.into()),
    };
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
    // `config` has no JSON envelope: `path`/`show` are a raw path and TOML
    // text. Emitting those on a `--format json` stdout would break the
    // one-JSON-document automation contract, so reject loudly (exit 2) rather
    // than emitting non-JSON.
    if format == OutputFormat::Json {
        return Err(AppError::Config(
            "config command does not support --format json; use human or log output".into(),
        ));
    }
    // `--format log` promises exactly one ASCII key=value line and no paths or
    // config text. `config path` and `config show` are nothing *but* a path and
    // config text, so in that format they report the action and leave their
    // content to the human output.
    let log = format == OutputFormat::Log;
    match command {
        ConfigCommand::Path => {
            if log {
                println!("{}", cargo_cleanme::output::log::config("path"));
            } else {
                println!("{}", sanitize_path_text(&path.display().to_string()));
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
                if log {
                    // The log format promises exactly one ASCII key=value line
                    // and no paths or config text. The editor's own exit status
                    // and the config's path are both things this format
                    // refuses: a path in a retained history pane is a small
                    // disclosure, and prose where a monitor expects
                    // `key=value` is not machine-readable. The failure is
                    // reported the way every other failure is — a typed code
                    // and nothing else.
                    eprintln!("{}", cargo_cleanme::output::log::fatal("config", "config"));
                } else {
                    eprintln!(
                        "cargo-cleanme: editor exited with {status}; config was not reverted: {}",
                        sanitize_path_text(&path.display().to_string())
                    );
                }
                // The exit contract is 0/1/2: an editor's arbitrary status
                // is reported in prose above, never propagated as our code.
                return Ok(1);
            }
            if !path.exists() {
                // `load` treats an absent file as "all defaults", so a
                // deleted config would silently reset every policy.
                return Err(AppError::Config(format!(
                    "editor removed the config file {}; refusing to treat that as an edit to defaults",
                    sanitize_path_text(&path.display().to_string())
                )));
            }
            config::load(path).map_err(|e| {
                AppError::Config(format!(
                    "edited config {} is invalid: {e}",
                    sanitize_path_text(&path.display().to_string())
                ))
            })?;
            if log {
                println!("{}", cargo_cleanme::output::log::config("edit"));
            } else {
                println!("edited {}", sanitize_path_text(&path.display().to_string()));
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
    /// M018: the learned roots whose coverage could not be proven.
    ///
    /// Their workspaces stay in the discovery universe — discovered, resolved,
    /// sized and classified — but they are removed from the *candidate* set,
    /// so a shared output between a proven root and an unproven one stays
    /// `Shared`. Empty for every scope but the Full cleanup.
    unproven_roots: Vec<cargo_cleanme::domain::UnprovenRoot>,
    /// M018: how long the verification pass took, when it ran.
    ///
    /// The gate's cost is a real cost, and the plan's stop conditions make it
    /// measurable rather than assumed: `None` means the pass did not run,
    /// which is the case for every scope but an incomplete Full cleanup.
    verification_elapsed: Option<std::time::Duration>,
    /// True for Routine/Full scopes whose roots are automatic hints; false
    /// for explicit scopes where every defect stays a fatal usage error.
    automatic: bool,
}

/// The **static** preconditions of an interactive run.
///
/// Every check here is a property of the *invocation* — the requested output
/// format, whether the standard streams are a terminal, and the terminal's
/// geometry — and none of them needs the scope to be resolved first.
///
/// That is why they run before the scope resolution: resolving a scope spawns
/// Cargo (`cargo locate-project` for the working-directory join, and
/// `cargo metadata` for every resolved root), and a run that is going to be
/// refused must not spawn a single process. Under `--full` the scope
/// resolution is a whole platform reconciliation, so the refusal used to
/// arrive thousands of Cargo spawns later.
fn check_interactive_preconditions(
    request: &CleanupRequest,
    options: &RunOptions,
) -> Result<(), AppError> {
    if !request.interactive {
        return Ok(());
    }
    // The machine formats are refused first because that conflict is a static
    // property of the invocation: it is knowable without touching a single
    // stream, and clap makes the same kind of call by rejecting conflicting
    // argv before anything runs. The terminal precondition is reported next,
    // for the same reason in the opposite direction — both are checked before
    // the listing, which is the first thing that would spawn `cargo metadata`.
    if options.format != OutputFormat::Human {
        return Err(AppError::Config(
            "interactive selection requires human output; --format json and --format log are \
             automation contracts and cannot carry a terminal session"
                .into(),
        ));
    }
    // The terminal precondition is checked *before* the listing, not after.
    // The listing is a full read-only resolution that spawns `cargo metadata`
    // across every resolved root; doing that only to then refuse for lack of a
    // terminal would spend the operator's time and spawn processes for a run
    // that was never going to happen.
    if let Err(reason) = cargo_cleanme::interactive::ensure_interactive_terminal(
        std::io::IsTerminal::is_terminal(&std::io::stdin()),
        std::io::IsTerminal::is_terminal(&std::io::stdout()),
    ) {
        return Err(AppError::Config(format!(
            "interactive selection requires a terminal ({reason}); \
             drop -i to clean without choosing interactively, or \
             use `cargo cleanme scan --all` to inspect the same units read-only"
        )));
    }
    // The geometry is checked here too, for the same reason: the frame is
    // painted into the terminal, and a terminal too small to hold the list
    // cannot be made larger by resolving the scope first.
    let too_small = cargo_cleanme::interactive::terminal_geometry()
        .ok()
        .and_then(|(width, height)| {
            cargo_cleanme::interactive::ensure_interactive_geometry(width, height).err()
        });
    if let Some(reason) = too_small {
        return Err(AppError::Config(format!(
            "interactive selection requires a usable terminal ({reason}); \
             drop -i to clean without choosing interactively, or \
             use `cargo cleanme scan --all` to inspect the same units read-only"
        )));
    }
    Ok(())
}

fn run_cleanup(
    request: CleanupRequest,
    config_path: &std::path::Path,
    options: RunOptions,
    runner: &dyn cargo_cleanme::workspace::CargoRunner,
) -> Result<i32, AppError> {
    // The static preconditions come first, before the scope resolution: the
    // scope resolution spawns Cargo, and a refused run must not spawn a
    // single process to reach the refusal.
    check_interactive_preconditions(&request, &options)?;
    let config = config::load_or_create(config_path)?;
    let mode = request.mode;
    let resolved = resolve_cleanup_roots(&request.scope, config_path, &config, options, runner)?;
    let configured = &config.cleanup.policy;
    let overrides = &request.overrides;
    let policy = cargo_cleanme::cleanup::CleanupPolicy {
        min_reclaimable_bytes: overrides
            .min_reclaimable_bytes
            .unwrap_or(configured.min_reclaimable_bytes),
        min_inactive_seconds: overrides.older_than.or(configured.min_inactive_seconds),
        // The patterns are matched against canonicalized workspace roots, so a
        // pattern spelled the way the operator's own shell shows it (through a
        // symlink) would match nothing. The configured patterns are rewritten
        // when the config is loaded; the command-line ones need the same
        // rewrite, or an `--exclude` spelled through a symlink would protect
        // nothing and the tree it named would be cleaned anyway.
        include: if overrides.include.is_empty() {
            configured.include.clone()
        } else {
            config::canonical_policy_patterns(&overrides.include)
        },
        exclude: if overrides.exclude.is_empty() {
            configured.exclude.clone()
        } else {
            config::canonical_policy_patterns(&overrides.exclude)
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
    // M018: the roots whose coverage could not be proven are disclosed, not
    // hidden. A run that skipped them says so on stderr whether or not the
    // report carries a row for them — a root with no workspace under it has
    // no row, and its exclusion is still a fact the operator needs.
    if !resolved.unproven_roots.is_empty() && options.format != OutputFormat::Log {
        for unproven in &resolved.unproven_roots {
            eprintln!(
                "cargo-cleanme: learned root {} was skipped: {}",
                sanitize_path_text(&unproven.root.display().to_string()),
                unproven.detail
            );
        }
    }
    if let Some(block) = resolved.blocked.clone() {
        // An automatic root is a symlink or indeterminately unreadable:
        // omission could conceal an expected scan tree, so nothing is
        // traversed and no Cargo process runs in any mode. This is a typed
        // whole-scope block (exit 1), never a fatal usage error.
        let report = cargo_cleanme::cleanup::CleanReport {
            results: Vec::new(),
            // `diagnostics` counts *filesystem diagnostic notices* on every
            // path. Here nothing was traversed, so the only notices are the
            // omission ones — counted the way the stderr counts them, one per
            // (reason, noun) group, so the JSON agrees with the stderr lines.
            diagnostics: resolved.omitted_diagnostics.len(),
            failed: 0,
            mode,
            counters: Default::default(),
            scope_blocked: Some(block),
            unresolved_ownership: Vec::new(),
            out_of_scope_unresolved: 0,
            selected_roots: resolved.roots.clone(),
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: Some(policy),
            selector,
        };
        emit_cleanup(&report, &resolved, &resolved.roots, options, "")?;
        return Ok(1);
    }
    if resolved.roots.is_empty() {
        // A successful no-op, not an error: there was simply nothing bounded
        // to clean. The report still states the resolved operation, scope, and
        // mode, so a machine consumer reads the same shape either way.
        // `diagnostics` counts filesystem diagnostic notices, consistent with
        // the blocked scope above and with the engine path: the omission
        // notices, counted the way the stderr counts them (one per group), so
        // the JSON agrees with the stderr lines.
        let empty = cargo_cleanme::cleanup::CleanReport {
            results: Vec::new(),
            diagnostics: resolved.omitted_diagnostics.len(),
            failed: 0,
            mode,
            counters: Default::default(),
            scope_blocked: None,
            unresolved_ownership: Vec::new(),
            out_of_scope_unresolved: 0,
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
        )?;
        return Ok(0);
    }
    // Which resolved units are candidates. Resolved *here*, after the root set
    // is known, because `-i` needs those roots to list anything at all.
    let selection = match resolve_candidate_selection(&request, &resolved, &config, mode, options)?
    {
        SelectionOutcome::Chosen(selection) => selection,
        SelectionOutcome::Cancelled => {
            // A deliberate operator decision, not a failure and not a silent
            // no-op: say plainly that nothing ran.
            eprintln!(
                "cargo-cleanme: interactive selection cancelled; no cleanup commands were run"
            );
            return Ok(0);
        }
        SelectionOutcome::NothingToClean => {
            // The scope resolved and proved no cleanup unit. That is a plain
            // negative result, not a failure: the same scope cleaned without
            // `-i` completes normally and exits 0, so the two spellings of
            // one scope must agree. The verdict is one line, on stdout,
            // because it is the run's result rather than a diagnostic.
            println!("Nothing to clean.");
            return Ok(0);
        }
    };
    let show = options.format == OutputFormat::Human
        && cargo_cleanme::progress::should_show_progress(options.no_progress);
    let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show, options.verbose);
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
        &selection,
    );
    renderer.finish_and_clear();
    let report = cleanup?;
    if options.format == OutputFormat::Human
        && options.verbose
        && let Some(generation) = resolved.state_generation
    {
        // Which state generation the cleanup used is debugging information, not
        // a result. It belongs to the verbose surface (M020).
        println!("state generation last_full_at={generation} (Unix seconds)");
    }
    emit_cleanup(&report, &resolved, &resolved.roots, options, "")?;
    if options.stats {
        let elapsed = wall_start.elapsed();
        // C003 §7.6: cleanup --stats must account for the final
        // ownership-universe proof work, not only the initial scan.
        eprintln!(
            "cleanup stats: mode={:?} {} {} {} {} verification={} elapsed={:.2}s",
            mode,
            report.counters.stats_line(),
            report.counters.timings_line(),
            report.counters.proof_stats_line(),
            report.counters.proof_timings_line(),
            // M018: the gate's cost, measured. `skipped` means the platform
            // walk completed and every learned root was proven by definition,
            // so the verification pass did not run at all.
            match resolved.verification_elapsed {
                Some(duration) => format!("{:.2}s", duration.as_secs_f64()),
                None => "skipped".to_string(),
            },
            elapsed.as_secs_f64(),
        );
    }
    // M018: an unproven learned root means the requested boundary was not
    // fully cleaned, even when every root that *could* be proven was cleaned
    // successfully. The exit code says so, so an unattended script does not
    // read a partial Full cleanup as a complete one.
    Ok(cleanup_exit_code(&report, &resolved.unproven_roots))
}

/// What the selection step decided.
#[derive(Debug)]
enum SelectionOutcome {
    /// Run the engine with this selection.
    Chosen(cargo_cleanme::cleanup::CleanupSelection),
    /// The operator quit the interactive list. Nothing runs.
    Cancelled,
    /// The scope proved no cleanup unit at all. Nothing runs, and the
    /// operator is told so in one plain line.
    NothingToClean,
}

/// Resolve which resolved cleanup units are candidates for this run.
///
/// Without `-i` this is the CLI's declared selection, with `WorkingDirectory`
/// turned into concrete roots. `WorkingDirectory` is the only unresolved form
/// and `resolved()` fails closed rather than falling back to "everything".
///
/// This is a *post-classification* filter: `resolve_cleanup_roots` and
/// `workspace::build_groups` inside the engine still see the full Routine
/// universe, so output shared with a workspace outside the anchor stays
/// `Shared`. Narrowing root selection here instead would be the ADR 006 defect.
///
/// With `-i` the operator's choice narrows it further. The interactive path
/// resolves candidates through the *same*
/// [`cargo_cleanme::cleanup::list_cleanup_candidates`] pipeline the engine
/// uses, so the offered rows are the engine's own units, and the result is a
/// strictly narrower `Roots` selection — never a way to add, reclassify, or
/// unblock one. For maintenance scopes the listing is already anchored to the
/// working directory, so `-i` is bounded by where it runs; a bounded `ROOT`
/// and `--full` stay unfiltered because their roots already bound them.
fn resolve_candidate_selection(
    request: &CleanupRequest,
    resolved: &ResolvedCleanup,
    config: &config::Config,
    mode: cargo_cleanme::cleanup::CleanMode,
    options: RunOptions,
) -> Result<SelectionOutcome, AppError> {
    if !request.interactive {
        // M018: the coverage exclusion is applied to the declared selection,
        // so it can only ever remove candidates. The discovery universe stays
        // whole: a shared output between a proven root and an unproven one
        // stays `Shared`.
        let declared = request.selection.resolved()?;
        return Ok(SelectionOutcome::Chosen(
            declared.excluding(resolved.unproven_roots.clone()),
        ));
    }
    // The static preconditions are re-checked here, for the same reason the
    // listing itself is: this function must stay safe on its own. `run_cleanup`
    // already ran them *before* the scope resolution, so the refusal costs no
    // Cargo spawn.
    check_interactive_preconditions(request, &options)?;
    // Progress bars would fight the repainting list, so the listing runs
    // silent. It is a read-only resolution: `cargo metadata` only.
    let silent = cargo_cleanme::progress::IndicatifRenderer::new(true, options.verbose);
    // M014/ADR 006: the working directory anchors selection, not discovery.
    // Resolve the declared selection first so `-i` on a maintenance scope
    // offers only anchor-matching units. The proof universe stays wide —
    // `resolved.roots` still carry the full Routine set — so shared output
    // stays `Shared`. A bounded `ROOT`/`--full` resolve to `All` and stay
    // unfiltered because their roots already bound them.
    let anchor = request
        .selection
        .resolved()?
        .excluding(resolved.unproven_roots.clone());
    let listing = cargo_cleanme::cleanup::list_cleanup_candidates(
        &resolved.roots,
        config.scan.recency_seconds,
        mode,
        &cargo_cleanme::cleanup::SystemCleanupRunner,
        &silent,
        &anchor,
    )?;
    if let Some(block) = listing.scope_blocked {
        // Offering rows from an unproven universe would be the ADR 006 shape in
        // a new costume: the operator would be picking from a list whose
        // classification is exactly what failed.
        let detail =
            cargo_cleanme::cleanup::blocked_listing_detail(&block, &listing.unresolved_ownership);
        return Err(AppError::Config(format!(
            "cannot present an interactive selection: {detail}"
        )));
    }
    interactive_outcome(cargo_cleanme::interactive::run_interactive(
        listing.candidates,
    ))
}

/// Map the interactive surface's result onto the orchestrator's outcomes.
///
/// The three endings are three different facts and must not be collapsed: the
/// operator chose roots, the operator quit, or the scope proved no cleanup
/// unit at all. Only the third is a plain negative result.
fn interactive_outcome(
    result: Result<cargo_cleanme::interactive::SessionResult, String>,
) -> Result<SelectionOutcome, AppError> {
    match result {
        Ok(cargo_cleanme::interactive::SessionResult::Selected(roots)) => Ok(
            SelectionOutcome::Chosen(cargo_cleanme::cleanup::CleanupSelection::Roots(roots)),
        ),
        Ok(cargo_cleanme::interactive::SessionResult::Cancelled) => Ok(SelectionOutcome::Cancelled),
        Ok(cargo_cleanme::interactive::SessionResult::NothingToClean) => {
            Ok(SelectionOutcome::NothingToClean)
        }
        Err(reason) => Err(AppError::Config(reason)),
    }
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
    selection: &cargo_cleanme::cleanup::CleanupSelection,
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
            selection,
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
                            .any(|p| p == std::path::Path::new(&path)) =>
                {
                    let omitted = reclassified.omitted.clone();
                    let omission_notices = reclassified.omission_diagnostics().len();
                    let admitted = collapse_roots(reclassified.admitted);
                    if admitted.is_empty() {
                        let diagnostics = omission_notices;
                        return Ok(cargo_cleanme::cleanup::CleanReport {
                            results: Vec::new(),
                            diagnostics,
                            failed: 0,
                            mode,
                            counters: Default::default(),
                            scope_blocked: None,
                            unresolved_ownership: Vec::new(),
                            out_of_scope_unresolved: 0,
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
                    diagnostics: resolved.omitted_diagnostics.len(),
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
                    out_of_scope_unresolved: 0,
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

/// M018: the fail-closed rule for a Full cleanup that proved nothing.
///
/// If no learned root could be proven, the operation proves nothing: it
/// blocks with the standard unproven-ownership reason instead of exiting 0
/// with zero rows. The M012A silent no-op is exactly that shape — an
/// operator script reading "success" for a cleanup that never ran.
///
/// `None` when there was nothing to prove *and* nothing that failed: an
/// empty scope is an honest no-op, not a failure.
fn zero_proven_block(
    unproven: &[cargo_cleanme::domain::UnprovenRoot],
) -> Option<cargo_cleanme::cleanup::ScopeBlock> {
    if unproven.is_empty() {
        return None;
    }
    let mut paths: Vec<String> = unproven
        .iter()
        .map(|u| sanitize_path_text(&u.root.display().to_string()))
        .collect();
    paths.sort();
    Some(cargo_cleanme::cleanup::ScopeBlock::new(
        cargo_cleanme::cleanup::ScopeBlockReason::OwnershipUnproven,
        format!(
            "no learned root could be proven: {} root(s) had incomplete coverage ({}); \
             no cleanup commands were run — run `cargo cleanme scan --all` for the read-only \
             inventory, or `cargo cleanme ROOT` for a bounded scope",
            paths.len(),
            paths.join(", ")
        ),
    ))
}

/// M018: an unproven learned root means the requested boundary was not fully
/// cleaned, even when every root that *could* be proven was cleaned
/// successfully. The exit code says so, so an unattended script does not read
/// a partial Full cleanup as a complete one.
fn cleanup_exit_code(
    report: &cargo_cleanme::cleanup::CleanReport,
    unproven: &[cargo_cleanme::domain::UnprovenRoot],
) -> i32 {
    if report.failed > 0 || report.scope_blocked.is_some() || !unproven.is_empty() {
        1
    } else {
        0
    }
}

/// M018: turn the Full gate's verdict into a resolved cleanup scope.
///
/// Every learned root — proven or not — is admitted as a cleanup **root**.
/// Withholding an unproven root would narrow the *discovery* universe: the
/// other claimant of a shared output would never be seen, and those bytes
/// would be promoted from `Shared` to `PrivateBounded` — the ADR 006 defect,
/// and the forbidden shortcut. The unproven root's workspaces are removed
/// from the *candidate* set by the selection, never from the discovery set.
///
/// The fail-closed condition is about the **proof**, not about the admitted
/// roots: an unproven root is admitted as a root precisely so that its
/// workspaces are still *seen*; it is never a candidate. If the gate proved no
/// root at all while there were roots to judge, the operation proves nothing,
/// and exiting 0 with zero rows would be the M012A silent no-op — an operator
/// script reading "success" for a cleanup that never ran.
fn resolve_full_verdict(
    verdict: cargo_cleanme::policy::FullCleanupVerdict,
    verification_elapsed: Option<std::time::Duration>,
) -> Result<ResolvedCleanup, AppError> {
    let unproven = verdict.unproven.clone();
    let proven_count = verdict.proven.len();
    let mut resolved = admit_automatic(
        verdict.admitted_roots(),
        Vec::new(),
        "full",
        verdict.generation,
    )?;
    resolved.unproven_roots = unproven.clone();
    resolved.verification_elapsed = verification_elapsed;
    if proven_count == 0 && !unproven.is_empty() {
        resolved.blocked = zero_proven_block(&resolved.unproven_roots);
    }
    Ok(resolved)
}

/// M018: verify each learned root's coverage as its own walk root.
///
/// The pass runs only when the platform walk was incomplete. A completed
/// platform walk read every learned root's subtree, so every root is proven
/// by definition and a second traversal would buy nothing.
///
/// It uses the same policy — and therefore the same prune and filter sets —
/// as the platform walk, because its verdict stands in for that walk's.
fn verify_learned_root_coverage(
    outcome: &ScanOutcome,
    config: &config::Config,
    options: RunOptions,
) -> Result<
    Option<(
        Vec<cargo_cleanme::domain::RootCoverage>,
        std::time::Duration,
    )>,
    AppError,
> {
    if outcome.code == 0 {
        return Ok(None);
    }
    let Some(state) = outcome.full_state.as_ref() else {
        return Ok(None);
    };
    let roots: Vec<std::path::PathBuf> = state
        .learned_roots
        .iter()
        .map(|root| root.path.clone())
        .collect();
    if roots.is_empty() {
        return Ok(None);
    }
    let policy = cargo_cleanme::policy::resolve(
        cargo_cleanme::domain::ScanRequest {
            cli_root: None,
            full: true,
        },
        &config.scan,
    )?;
    let show = options.format == OutputFormat::Human
        && cargo_cleanme::progress::should_show_progress(options.no_progress);
    let renderer = cargo_cleanme::progress::IndicatifRenderer::new(!show, options.verbose);
    let observer: &dyn cargo_cleanme::progress::ProgressObserver = &renderer;
    observer.phase(cargo_cleanme::progress::ScanPhase::Discovery);
    let started = std::time::Instant::now();
    let coverage = cargo_cleanme::discovery::verify_root_coverage(&roots, &policy, observer);
    // The bars must come down before the cleanup's own renderer starts, or
    // two draw targets own the terminal at once.
    renderer.finish_and_clear();
    let (coverage, _diagnostics) = coverage?;
    // The gate's cost is measured, not assumed: the plan's stop conditions
    // make an unbounded verification pass a reason to stop, and a number is
    // the only way to know.
    Ok(Some((coverage, started.elapsed())))
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
    runner: &dyn cargo_cleanme::workspace::CargoRunner,
) -> Result<ResolvedCleanup, AppError> {
    match scope {
        CleanupScope::Root(root) => {
            // Explicit roots stay authoritative: no omission, no fallback to
            // a smaller implicit set. The engine's strict validation reports
            // the defect as a fatal usage error.
            let absolute = cli::absolutize_root(root)?;
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
                unproven_roots: Vec::new(),
                verification_elapsed: None,
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
            let outcome = run_scan(
                ScanIntent::Full,
                config_path,
                options,
                EmitReport::No,
                cargo_cleanme::discovery_state::ReconciliationMode::IncompleteRetain,
            )?;
            if !outcome.publish_ok {
                eprintln!(
                    "cargo-cleanme: discovery state was not saved; cleaning the reconciled in-memory generation only"
                );
            }
            // M018: the proof boundary is the learned root, not the platform
            // root. When the platform walk could not complete, each learned
            // root is re-walked as its own walk root, so the per-root
            // attribution decides — per root — whether that root's subtree was
            // fully read. The pass costs a second traversal of the learned
            // roots' subtrees, and only runs when the platform walk was
            // incomplete: a completed platform walk proves every learned root
            // by definition.
            let coverage = verify_learned_root_coverage(&outcome, config, options)?;
            let (coverage, verification_elapsed) = match coverage {
                Some((coverage, elapsed)) => (Some(coverage), Some(elapsed)),
                None => (None, None),
            };
            let verdict = cargo_cleanme::policy::full_cleanup_roots(
                outcome.code,
                outcome.full_state,
                coverage.as_deref(),
            )?;
            resolve_full_verdict(verdict, verification_elapsed)
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
                        unproven_roots: Vec::new(),
                        verification_elapsed: None,
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
                        unproven_roots: Vec::new(),
                        verification_elapsed: None,
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
                        return admit_automatic(Vec::new(), Vec::new(), "routine", None);
                    };
                    let (entries, warning) = cargo_cleanme::policy::routine_cleanup_candidates(
                        &home,
                        config.scan.learned_root_retention_days,
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
                    // ADR 009: the working directory joins the maintenance
                    // scope. The universe may only ever grow, so the
                    // operator's roots are *added* to the routine candidates
                    // — never substituted for them. A narrower scope would
                    // promote `shared` bytes to `private`, the only class that
                    // authorizes deletion (ADR 006).
                    admit_automatic(
                        entries,
                        working_directory_entries(runner),
                        "routine",
                        generation,
                    )
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

/// The working directory's scope roots (ADR 009).
///
/// The working directory is read with the same spelling the anchor uses
/// (`std::env::current_dir`, unresolved), so the scope root and the anchor
/// agree on one path. An unreadable working directory contributes no root at
/// all; the anchor's own resolution is what turns that into a usage error, so
/// the two cannot disagree about whether the operator has a position.
fn working_directory_scope_paths(
    runner: &dyn cargo_cleanme::workspace::CargoRunner,
) -> Vec<std::path::PathBuf> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    cargo_cleanme::workspace::working_directory_scope_roots(&cwd, runner)
}

/// The working directory's scope roots as operator-provenance candidates.
///
/// The operator chose these roots by standing where they stand, so they carry
/// operator provenance: the maintainability rule does not apply to them, and
/// every other defect keeps the automatic root's handling — a symlinked root
/// blocks the scope rather than being followed.
fn working_directory_entries(
    runner: &dyn cargo_cleanme::workspace::CargoRunner,
) -> cargo_cleanme::policy::CleanupCandidates {
    working_directory_scope_paths(runner)
        .into_iter()
        .map(|path| (path, cargo_cleanme::policy::RootProvenance::Operator))
        .collect()
}

/// Whether the working directory lies outside the scope the routine roots
/// already reach (ADR 009).
///
/// The working directory joins the maintenance scope only when the Routine
/// scope does not already reach it. Two cases are excluded, and both for the
/// same reason — the bounded machine-wide sweep must stay bounded:
///
/// - the working directory is *inside* a routine root: the routine scope
///   already discovers it, so adding it is redundant;
/// - the working directory *contains* a routine root: adding it would widen a
///   bounded sweep of the known roots into a sweep of everything under the
///   operator's directory. That is a different operation, it drags in the
///   origins the maintainability rule excludes — a Trash full of abandoned
///   manifests is the common case — and those unresolvable manifests sit
///   inside the ADR 007 proof boundary and block the run. Measured on the
///   operator's machine: a bare run from the home directory went from a
///   six-second bounded sweep to a sixty-five-second sweep of 264,183
///   directories that ended blocked on 1,357 unresolvable manifests.
///
/// A bounded `ROOT` is the spelling for that wider sweep, and it has always
/// carried the same hazard.
///
/// The comparison is canonical-or-raw on both sides, so a symlinked prefix
/// (macOS `/var` vs `/private/var`) cannot make two unrelated roots look
/// related, nor make related ones look unrelated.
fn working_directory_is_unreached(routine_roots: &[std::path::PathBuf]) -> bool {
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    !routine_roots
        .iter()
        .any(|root| roots_are_related(root, &cwd))
}

/// Whether two roots are in an ancestor relationship, one containing the other.
fn roots_are_related(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canonical =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (a, b) = (canonical(a), canonical(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// Canonicalize a path, remembering the answer.
///
/// The Full reconciliation asks for the same manifest twice — once through the
/// resolved workspace member and once through the discovered manifest — and
/// every canonicalization walks the path component by component. The answer
/// depends only on the path, so it is asked for once.
fn canonicalize_cached(
    cache: &mut std::collections::HashMap<std::path::PathBuf, std::path::PathBuf>,
    path: &std::path::Path,
) -> std::path::PathBuf {
    if let Some(cached) = cache.get(path) {
        return cached.clone();
    }
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    cache.insert(path.to_path_buf(), canonical.clone());
    canonical
}

/// A path rendered for a human-readable report.
///
/// Control characters are escaped and the text is bounded, so a path the
/// operator did not choose cannot rewrite the report's own layout. The
/// escaping is the shared one — the same routine the library's report and
/// selection surfaces use — and the bound is applied to the *escaped* text,
/// so a path of control characters cannot push the visible text past the
/// bound by rendering shorter than it is.
fn sanitize_path_text(path: &str) -> String {
    cargo_cleanme::report::escape_path_text(path)
        .chars()
        .take(200)
        .collect()
}

/// Admit one automatic (Routine/Full) candidate set: classify by provenance
/// before collapse, collapse admitted paths only, remember omissions for the
/// late pre-spawn recheck, and convert symlink/uncertain roots into a typed
/// whole-scope block instead of a fatal error or a silent scope reduction.
fn admit_automatic(
    entries: cargo_cleanme::policy::CleanupCandidates,
    operator_entries: cargo_cleanme::policy::CleanupCandidates,
    scope_label: &'static str,
    state_generation: Option<u64>,
) -> Result<ResolvedCleanup, AppError> {
    // Automatic entries never produce a fatal classification error (only
    // Explicit does), but a panic would exit 101 outside the 0/1/2 contract.
    // Propagate as a usage error instead.
    let classified = cargo_cleanme::policy::classify_cleanup_roots(entries)
        .map_err(|e| AppError::Config(format!("automatic cleanup classification failed: {e}")))?;
    // ADR 009: the working directory joins only when the Routine scope does
    // not already reach it. The decision is made against the *admitted*
    // routine roots, so a root the maintainability rule refuses — an agent
    // worktree, say — does not count as the routine scope reaching the
    // operator's directory.
    let operator_classified = if working_directory_is_unreached(&classified.admitted) {
        cargo_cleanme::policy::classify_cleanup_roots(operator_entries)
            .map_err(|e| AppError::Config(format!("operator root classification failed: {e}")))?
    } else {
        cargo_cleanme::policy::ClassifiedRoots::default()
    };
    let mut classified = classified;
    classified.admitted.extend(operator_classified.admitted);
    classified.omitted.extend(operator_classified.omitted);
    classified.blocked.extend(operator_classified.blocked);
    let omitted = classified.omitted.clone();
    let omitted_diagnostics = classified.omission_diagnostics();
    let blocked = if classified.blocked.is_empty() {
        None
    } else {
        // The block names the roots that caused it and why. A count alone
        // tells the operator that something is wrong but not what, and these
        // roots are the only thing they can act on: the sample is bounded so
        // a machine with many of them still gets a readable report.
        let mut entries: Vec<(String, String)> = classified
            .blocked
            .iter()
            .map(|b| {
                (
                    sanitize_path_text(&b.path.display().to_string()),
                    b.reason.clone(),
                )
            })
            .collect();
        entries.sort();
        let examples: Vec<String> = entries
            .iter()
            .take(3)
            .map(|(path, reason)| format!("{path} ({reason})"))
            .collect();
        let sample = if examples.is_empty() {
            String::new()
        } else {
            format!("; examples: {}", examples.join(", "))
        };
        Some(cargo_cleanme::cleanup::ScopeBlock::new(
            cargo_cleanme::cleanup::ScopeBlockReason::IncompleteDiscovery,
            format!(
                "automatic cleanup scope is uncertain for {} root(s); no cleanup commands were run{sample}",
                entries.len(),
            ),
        ))
    };
    Ok(ResolvedCleanup {
        roots: collapse_roots(classified.admitted),
        scope_label,
        state_generation,
        omitted_diagnostics,
        omitted,
        blocked,
        unproven_roots: Vec::new(),
        verification_elapsed: None,
        automatic: true,
    })
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
) -> Result<(), AppError> {
    if options.format == OutputFormat::Json {
        // Like scan: serialization failure is a usage error (exit 2), never a
        // panic (exit 101).
        let document = serde_json::to_string(&cargo_cleanme::output::cleanup(
            report,
            resolved.scope_label,
            resolved.state_generation,
        ))
        .map_err(|e| AppError::Config(format!("cannot serialize JSON report: {e}")))?;
        println!("{document}");
    } else if options.format == OutputFormat::Log {
        println!(
            "{}",
            cargo_cleanme::output::log::cleanup(report, resolved.scope_label)
        );
    } else if !empty_note.is_empty() {
        println!("{empty_note}");
    } else if report_has_nothing_to_report(report) {
        // A run that cleaned nothing, skipped nothing, proved no
        // unresolvable ownership and was not blocked has said everything
        // there is to say. The operator gets one plain line instead of a
        // scope census and a summary of zeroes.
        println!("Nothing to clean.");
    } else {
        let human = human_options(options, human_cleanup_roots(resolved));
        if options.verbose {
            // The verbose surface is the debugging surface: it keeps the
            // preamble that names the roots the run walked.
            println!(
                "combined roots {}\n{}",
                roots
                    .iter()
                    .map(|r| sanitize_path_text(&r.display().to_string()))
                    .collect::<Vec<_>>()
                    .join(", "),
                report.render(&human)
            );
        } else {
            println!("{}", report.render(&human));
        }
    }
    Ok(())
}

/// The human surface's rendering options for a run.
///
/// The verbose surface spells every path absolutely — it is the debugging
/// surface. The quiet surface spells a bounded scope's paths relative to the
/// scope root.
fn human_options(
    options: RunOptions,
    roots: Vec<std::path::PathBuf>,
) -> cargo_cleanme::report::HumanOptions {
    if options.verbose {
        cargo_cleanme::report::HumanOptions::verbose()
    } else {
        cargo_cleanme::report::HumanOptions::quiet(roots)
    }
}

/// The scope roots a cleanup report's paths are shown relative to.
///
/// A Full cleanup has no single root — it is the platform — so its paths stay
/// absolute. Every other scope is bounded.
fn human_cleanup_roots(resolved: &ResolvedCleanup) -> Vec<std::path::PathBuf> {
    if resolved.scope_label == "full" {
        Vec::new()
    } else {
        resolved.roots.clone()
    }
}

/// The scope roots a scan report's paths are shown relative to.
fn human_scan_roots(scope: &cargo_cleanme::domain::ScanScope) -> Vec<std::path::PathBuf> {
    match scope {
        cargo_cleanme::domain::ScanScope::Explicit(root) => {
            vec![absolutize_for_display(root)]
        }
        cargo_cleanme::domain::ScanScope::ExplicitRoots(roots) => {
            roots.iter().map(|r| absolutize_for_display(r)).collect()
        }
        cargo_cleanme::domain::ScanScope::Routine(roots) => {
            roots.iter().map(|r| absolutize_for_display(r)).collect()
        }
        // The Full scope is the platform root: a relative path under `/` would
        // name a fragment, not a place.
        cargo_cleanme::domain::ScanScope::Global(_) => Vec::new(),
    }
}

/// The absolute spelling of a scope root, for the relative-path base.
///
/// The scope keeps the root as the operator spelled it (`scan .`), while the
/// report's paths are absolute. The base has to be absolute too, or the two
/// never match and every path stays absolute.
///
/// It has to be **canonical** as well. The report's paths are canonical
/// (`workspace::canonical_or_absolute`), and the relative-path rule is a
/// component-wise `strip_prefix`: a lexical base and a canonical path never
/// match, so under a symlinked root every path stayed absolute — while the
/// cleanup surface, whose roots go through `collapse_roots` (which
/// canonicalizes), rendered the *same* root relatively. The two surfaces of
/// one tool disagreed about one root; this is the scan side catching up.
fn absolutize_for_display(root: &std::path::Path) -> std::path::PathBuf {
    let absolute = cargo_cleanme::cli::absolutize_root(root).unwrap_or_else(|_| root.to_path_buf());
    std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

/// Whether a cleanup report carries nothing beyond the empty case.
///
/// Every term is a thing the operator would otherwise need to read: rows
/// (cleaned, previewed, simulated, skipped, failed), ownership the run could
/// not prove, participants it reported out of scope, a scope block, a
/// discovery diagnostic, and a failure. A report with none of them describes
/// a run that found nothing and did nothing, which has one true sentence.
fn report_has_nothing_to_report(report: &cargo_cleanme::cleanup::CleanReport) -> bool {
    report.results.is_empty()
        && report.unresolved_ownership.is_empty()
        && report.out_of_scope_unresolved == 0
        && report.scope_blocked.is_none()
        && report.failed == 0
        && report.diagnostics == 0
}

fn collapse_roots(roots: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    // The collapse itself is the shared one (`traverse::collapse_outermost`):
    // canonicalise, dedup, drop nested roots. The scan and display surfaces
    // keep a root's own spelling when it does not resolve, where the cleanup
    // surface fails closed.
    cargo_cleanme::traverse::collapse_outermost(roots)
        .into_iter()
        .map(|root| root.spelling)
        .collect()
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
/// `--full` runs to refresh learned state — must not write anything to
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
/// C028: `--full` consumes the generation reconciled by the traversal
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
    reconciliation_mode: cargo_cleanme::discovery_state::ReconciliationMode,
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
    let is_maintenance = matches!(intent, ScanIntent::Maintenance);
    let (root, full) = match intent {
        ScanIntent::Full => (None, true),
        // The scan root is folded against the invocation directory *lexically*,
        // exactly as the cleanup path folds it (`cli::absolutize_root`).
        //
        // The two spellings of one relative root have to agree. The OS
        // resolves `..` physically — through symlinks — while Cargo resolves
        // a manifest path lexically, so a root that crossed a symlink made the
        // scan discover a manifest it could then not resolve, and report an
        // empty inventory with exit 0.
        ScanIntent::Explicit(root) => (Some(cli::absolutize_root(&root)?), false),
        ScanIntent::Maintenance => (None, false),
    };
    let mut policy = cargo_cleanme::policy::resolve(
        domain::ScanRequest {
            cli_root: root,
            full,
        },
        &c.scan,
    )?;
    // ADR 009: the read-only maintenance inventory must cover the same
    // universe the maintenance cleanup would act on, or it is not an
    // inventory of it. A configured `scan.root` is an exclusive override and
    // keeps its own scope.
    if is_maintenance && let domain::ScanScope::Routine(roots) = &mut policy.scope {
        // ADR 009: the same rule as the cleanup path — the working directory
        // joins only when the Routine scope does not already reach it.
        let routine = roots.clone();
        if working_directory_is_unreached(&routine) {
            roots.extend(working_directory_scope_paths(
                &cargo_cleanme::cleanup::SystemCleanupRunner,
            ));
        }
    }
    let recency = policy.recency;

    // Inline progress is transient stderr state; final report remains stdout.
    // Non-TTY or `--no-progress` forces the no-op path without control noise.
    let show = options.format == OutputFormat::Human
        && progress::should_show_progress(options.no_progress);
    let renderer = progress::IndicatifRenderer::new(!show, options.verbose);
    // Progress-render failure must degrade to hidden rather than fail a scan.
    // Construction is infallible; refresh throttling bounds overhead.
    let observer: &dyn progress::ProgressObserver = &renderer;
    observer.phase(progress::ScanPhase::Discovery);

    // Manifest-first discovery (no sibling target required). A discovery
    // failure returns through `?`, which would drop the renderer with live
    // bars — and `BarState::drop` *finishes* a live bar into a retained line
    // instead of clearing it — so the bars come down first. (Hidden renderer:
    // no-op.)
    let discovery_start = std::time::Instant::now();
    let discovered = match cargo_cleanme::discovery::discover_manifests_with_attribution(
        &policy,
        observer,
        options.stats,
    ) {
        Ok(discovered) => discovered,
        Err(error) => {
            renderer.finish_and_clear();
            return Err(error);
        }
    };
    observer.discovery_issues(discovered.diagnostics.len() as u64);
    let discovery_elapsed = discovery_start.elapsed();
    let full_traversal_complete = discovered.coverage_complete
        && !discovered.diagnostics.iter().any(|d| {
            d.category == domain::DiagnosticCategory::PlatformRoot
                && d.severity == domain::DiagnosticSeverity::Error
        });
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
    // State notices are collected here and fanned out to stderr only after
    // `finish_and_clear`: a raw `eprintln!` while the bars are live bypasses
    // the coordinated draw target and corrupts the frame, orphaning bar lines
    // that survive the clear as stray terminal rows.
    let mut pending_notices: Vec<String> = Vec::new();
    if full {
        let selected = cargo_cleanme::discovery_state::full_reconciliation_prior(&loaded_state);
        if let Some((prior, replacing_invalid)) = selected {
            let now = cargo_cleanme::discovery_state::now_seconds();
            let mut resolved_by_manifest = std::collections::HashMap::new();
            let mut canonical_cache = std::collections::HashMap::new();
            for ws in &workspaces {
                for member in &ws.members {
                    resolved_by_manifest.insert(
                        canonicalize_cached(&mut canonical_cache, &member.manifest_path),
                        ws.root.clone(),
                    );
                }
            }
            let observations: Vec<_> = manifests
                .iter()
                .map(|manifest| {
                    let key = canonicalize_cached(&mut canonical_cache, manifest);
                    cargo_cleanme::discovery_state::ProjectObservation {
                        manifest: key.clone(),
                        workspace: resolved_by_manifest.get(&key).cloned(),
                    }
                })
                .collect();
            let complete = full_traversal_complete;
            // M018: the mode decides what an incomplete traversal may do to
            // the learned state. The read-only Full scan discards; the Full
            // cleanup retains every prior root in memory, because its proof
            // boundary is the learned root and it needs a generation to judge
            // coverage against.
            let mode = if complete {
                cargo_cleanme::discovery_state::ReconciliationMode::Complete
            } else {
                reconciliation_mode
            };
            match cargo_cleanme::discovery_state::reconcile_full(
                &prior,
                &observations,
                &uncertainty,
                mode,
                now,
                c.scan.learned_root_retention_days,
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
                    if mode == cargo_cleanme::discovery_state::ReconciliationMode::Complete {
                        match cargo_cleanme::discovery_state::publish(&next) {
                            Ok(()) => {
                                if replacing_invalid {
                                    pending_notices.push(
                                        "replaced unusable discovery state after successful Full reconciliation"
                                            .into(),
                                    );
                                }
                            }
                            Err(error) => {
                                publish_ok = false;
                                pending_notices
                                    .push(format!("discovery state was not saved: {error}"));
                            }
                        }
                    } else {
                        // M018: the retained generation is consumed in memory
                        // only. The persisted state keeps describing the last
                        // *complete* reconciliation, so an incomplete pass can
                        // never advance the generation a later run trusts.
                        pending_notices.push(
                            "discovery state was not updated: the Full traversal was incomplete; \
                             the learned roots were retained and verified in memory"
                                .into(),
                        );
                    }
                }
                cargo_cleanme::discovery_state::Reconciliation::NoPublication(reason) => {
                    publish_ok = false;
                    pending_notices.push(format!("discovery state was not reconciled: {reason}"));
                }
            }
        } else {
            publish_ok = false;
            let message = loaded_state
                .diagnostic()
                .unwrap_or_else(|| "discovery state cannot be safely read".into());
            pending_notices.push(format!("{message}; Full state reconciliation was skipped"));
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
            pending_notices.push(format!("discovery state was not saved: {error}"));
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

    let clock_cutoff = match scan_start.checked_sub(recency) {
        Some(cutoff) => cutoff,
        // Same drop-finishes-bar hazard as the discovery error above: the
        // bars come down before the error escapes.
        None => {
            renderer.finish_and_clear();
            return Err(AppError::Config(
                "recency window exceeds system time range".into(),
            ));
        }
    };

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

    let full_incomplete = full && !full_traversal_complete;
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

    // Deferred state notices: same stderr lines as before, but emitted only
    // once the coordinated frame is down, so they cannot orphan bar rows.
    for notice in &pending_notices {
        eprintln!("cargo-cleanme: {notice}");
    }

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
        let human = human_options(options, human_scan_roots(&policy.scope));
        println!("{}", cargo_cleanme::report::render(&mut report, &human));
    }
    // Normal diagnostic fan-out is human-readable detail on stderr. Log mode
    // carries `diagnostics=N` in its one line instead, so a retained history
    // tail is not ten paragraphs long.
    if !report.diagnostics.is_empty() && options.format != OutputFormat::Log {
        eprintln!(
            "{} filesystem diagnostics; rerun with a bounded root if needed",
            report.diagnostics.len()
        );
        // The detail behind the count is debugging information: it belongs to
        // the verbose surface (M020), not to every run. The count itself
        // stays on the default surface — one line, always.
        if options.verbose {
            // Cargo failures can repeat for hundreds of manifests. Keep their
            // complete per-manifest diagnostics in the report/JSON, but group the
            // human presentation by the bounded cause excerpt.
            let mut cargo_groups: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
            let mut other_diagnostics = Vec::new();
            for diagnostic in &report.diagnostics {
                let (stage, reason) = if let Some(reason) = diagnostic
                    .message
                    .strip_prefix("cargo locate-project failed:")
                {
                    ("locate", reason)
                } else if let Some(reason) =
                    diagnostic.message.strip_prefix("cargo metadata failed:")
                {
                    ("metadata", reason)
                } else {
                    other_diagnostics.push(diagnostic);
                    continue;
                };
                let cause = reason.split(" | ").skip(1).collect::<Vec<_>>().join(" | ");
                let reason_key = if cause.is_empty() {
                    reason.trim()
                } else {
                    &cause
                };
                let group = cargo_groups
                    .entry(format!("{stage}: {reason_key}"))
                    .or_insert_with(|| (0, Vec::new()));
                group.0 += 1;
                if group.1.len() < 3
                    && let Some(path) = &diagnostic.path
                {
                    group
                        .1
                        .push(sanitize_path_text(&path.display().to_string()));
                }
            }
            for (cause, (count, paths)) in cargo_groups.iter().take(8) {
                if let Some((stage, reason)) = cause.split_once(": ") {
                    eprintln!("  {count} Cargo {stage} failure(s): {reason}");
                } else {
                    eprintln!("  {count} Cargo resolution failure(s): {cause}");
                }
                if !paths.is_empty() {
                    eprintln!("    examples: {}", paths.join(", "));
                }
            }
            let hidden_cargo_groups = cargo_groups.len().saturating_sub(8);
            if hidden_cargo_groups > 0 {
                eprintln!("  … {hidden_cargo_groups} additional Cargo failure cause(s)");
            }
            // Preserve other diagnostic detail, capped independently of Cargo's
            // repetition so the normal stderr path remains bounded.
            for diagnostic in other_diagnostics.iter().take(10) {
                // The message carries the path it is about, so it is escaped
                // too: a directory name must never carry a raw newline or
                // escape byte into the operator's terminal. The message keeps
                // its full length — truncating prose mid-sentence would hide
                // the reason — while the path field stays bounded.
                eprintln!(
                    "  {}: {}{}",
                    diagnostic.severity.as_str(),
                    cargo_cleanme::report::escape_path_text(&diagnostic.message),
                    diagnostic
                        .path
                        .as_ref()
                        .map(|p| format!(" ({})", sanitize_path_text(&p.display().to_string())))
                        .unwrap_or_default()
                );
            }
            if other_diagnostics.len() > 10 {
                eprintln!(
                    "  … {} more non-Cargo diagnostic(s)",
                    other_diagnostics.len() - 10
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_cleanme::cleanup::{
        CleanMode, CleanOutcome, CleanReport, CleanResult, CleanupReasonCode, ScopeBlock,
        ScopeBlockReason,
    };
    use cargo_cleanme::domain::{OutputOwnershipClass, ScanCounters};

    /// A report that says nothing at all.
    fn empty_report() -> CleanReport {
        CleanReport {
            results: Vec::new(),
            diagnostics: 0,
            failed: 0,
            mode: CleanMode::Execute,
            counters: ScanCounters::default(),
            scope_blocked: None,
            unresolved_ownership: Vec::new(),
            out_of_scope_unresolved: 0,
            selected_roots: Vec::new(),
            discovered_manifests: 0,
            resolved_workspaces: 0,
            units_considered: 0,
            effective_policy: None,
            selector: None,
        }
    }

    /// M018: the cost control. A completed platform walk read every learned
    /// root's subtree, so every root is proven by definition and the
    /// verification pass is not entered at all.
    ///
    /// *Fails if:* the cost control regresses into an unconditional re-walk.
    /// The Full cleanup would then traverse every learned root twice on every
    /// run, including the runs that never needed it.
    #[test]
    fn a_complete_platform_walk_needs_no_verification_pass() {
        let options = RunOptions {
            no_progress: true,
            stats: false,
            format: OutputFormat::Human,
            verbose: false,
        };
        let state = cargo_cleanme::discovery_state::DiscoveryState {
            schema_version: cargo_cleanme::discovery_state::CURRENT_SCHEMA,
            last_full_at: Some(1),
            learned_roots: vec![cargo_cleanme::discovery_state::LearnedRoot {
                path: std::path::PathBuf::from("/work/a"),
                last_project_seen_at: 1,
            }],
            ..Default::default()
        };
        let complete = ScanOutcome {
            code: 0,
            full_state: Some(state.clone()),
            publish_ok: true,
        };
        let coverage = verify_learned_root_coverage(&complete, &config::Config::default(), options)
            .expect("the pass does not fail");
        assert!(
            coverage.is_none(),
            "a completed platform walk needs no second walk"
        );
        // An incomplete one does enter the pass — otherwise the gate would
        // have nothing to judge and would fail closed forever.
        let incomplete = ScanOutcome {
            code: 1,
            full_state: Some(state),
            publish_ok: true,
        };
        let coverage =
            verify_learned_root_coverage(&incomplete, &config::Config::default(), options)
                .expect("the pass does not fail");
        assert!(
            coverage.is_some(),
            "an incomplete platform walk must be measured per root"
        );
    }

    /// M018: a Full cleanup that proved nothing blocks with the standard
    /// unproven-ownership reason instead of exiting 0 with zero rows.
    ///
    /// The M012A silent no-op is exactly that shape: an operator script
    /// reading "success" for a cleanup that never ran. A test that only
    /// asserted "it did not clean anything" would pass against it.
    #[test]
    fn a_full_cleanup_that_proved_nothing_blocks_instead_of_exiting_zero() {
        let unproven = vec![cargo_cleanme::domain::UnprovenRoot {
            root: std::path::PathBuf::from("/work/unproven"),
            detail: "2 permission-denied region(s) under it".into(),
        }];
        let block = zero_proven_block(&unproven).expect("an unproven root blocks");
        assert_eq!(block.reason, ScopeBlockReason::OwnershipUnproven);
        assert!(
            block.message.contains("/work/unproven"),
            "the block names the root: {}",
            block.message
        );
        assert!(
            block.message.contains("no cleanup commands were run"),
            "the block says nothing ran: {}",
            block.message
        );
        // A scope with nothing to prove and nothing that failed is an honest
        // no-op, not a failure.
        assert!(
            zero_proven_block(&[]).is_none(),
            "an empty scope is not a block"
        );
    }

    /// M018: the gate admits an unproven root as a **root**, so the ownership
    /// universe is not narrowed.
    ///
    /// The defect this pins is the ADR 006 forbidden shortcut. If the
    /// unproven root is withheld from the cleanup roots, the discovery
    /// universe shrinks to the proven roots; the other claimant of a shared
    /// output is never seen, and those bytes are promoted from `Shared` to
    /// `PrivateBounded` — the one class that authorizes deletion. The
    /// unproven root's workspaces must still be *discovered*; they are removed
    /// from the candidate set by the selection, never from the universe.
    ///
    /// *Fails if:* the Full branch passes `verdict.proven` where it must pass
    /// `verdict.admitted_roots()`.
    #[test]
    fn an_unproven_root_is_admitted_as_a_root_and_excluded_only_as_a_candidate() {
        let dir = maintainable_fixture("cleanme-gate-");
        let proven_root = dir.path().join("proven");
        let unproven_root = dir.path().join("unproven");
        std::fs::create_dir_all(&proven_root).expect("the proven root");
        std::fs::create_dir_all(&unproven_root).expect("the unproven root");
        let verdict = cargo_cleanme::policy::FullCleanupVerdict {
            proven: vec![(
                proven_root.clone(),
                cargo_cleanme::policy::RootProvenance::Automatic,
            )],
            unproven: vec![cargo_cleanme::domain::UnprovenRoot {
                root: unproven_root.clone(),
                detail: "1 permission-denied region(s) under it".into(),
            }],
            generation: Some(7),
        };
        let resolved =
            resolve_full_verdict(verdict, None).expect("the verdict resolves into a scope");
        let canonical =
            |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        assert!(
            resolved.roots.contains(&canonical(&unproven_root)),
            "the unproven root is a root: the discovery universe must not be narrowed; \
             roots: {:?}, omitted: {:?}, blocked: {:?}",
            resolved.roots,
            resolved.omitted,
            resolved.blocked
        );
        assert!(
            resolved.roots.contains(&canonical(&proven_root)),
            "the proven root is a root too: {:?}",
            resolved.roots
        );
        assert_eq!(
            resolved.unproven_roots.len(),
            1,
            "the unproven root is disclosed: {:?}",
            resolved.unproven_roots
        );
        assert!(
            resolved.blocked.is_none(),
            "one proven root is enough to run the scope: {:?}",
            resolved.blocked
        );
        // The root being admitted does not make its workspaces candidates.
        let selection = cargo_cleanme::cleanup::CleanupSelection::All
            .excluding(resolved.unproven_roots.clone());
        assert!(
            !selection.admits(&canonical(&unproven_root)),
            "the unproven root's units are not candidates"
        );
        assert!(
            selection.admits(&canonical(&proven_root)),
            "the proven root's units stay candidates"
        );
    }

    /// M018: a gate that proved nothing blocks the scope, even though the
    /// unproven roots are admitted as roots.
    ///
    /// The admitted roots exist so the universe is whole; they are not a
    /// licence to clean. With no proven root there is nothing to authorize a
    /// deletion, so the run must block rather than exit 0 with zero rows.
    ///
    /// *Fails if:* the fail-closed condition is tested against the admitted
    /// roots instead of the proof.
    #[test]
    fn a_gate_that_proved_nothing_blocks_even_though_the_roots_are_admitted() {
        let dir = maintainable_fixture("cleanme-gate-empty-");
        let unproven_root = dir.path().join("unproven");
        std::fs::create_dir_all(&unproven_root).expect("the unproven root");
        let verdict = cargo_cleanme::policy::FullCleanupVerdict {
            proven: Vec::new(),
            unproven: vec![cargo_cleanme::domain::UnprovenRoot {
                root: unproven_root,
                detail: "1 permission-denied region(s) under it".into(),
            }],
            generation: Some(7),
        };
        let resolved =
            resolve_full_verdict(verdict, None).expect("the verdict resolves into a scope");
        let block = resolved
            .blocked
            .expect("a gate that proved nothing blocks the scope");
        assert_eq!(block.reason, ScopeBlockReason::OwnershipUnproven);
        assert!(
            block.message.contains("no cleanup commands were run"),
            "the block says nothing ran: {}",
            block.message
        );
    }

    /// A fixture directory the maintainability rule admits.
    ///
    /// The rule withholds the platform temp root whenever the home is a real
    /// one — a temp fixture would be *omitted* rather than admitted, and the
    /// test would measure the omission rule instead of the gate. The crate's
    /// own build directory is a real directory that is disposable by
    /// construction, so it is the right home for a fixture that has to survive
    /// the maintainability check.
    fn maintainable_fixture(prefix: &str) -> tempfile::TempDir {
        let base = std::env::var_os("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .expect("the crate root")
                    .join("target")
            });
        std::fs::create_dir_all(&base).expect("the build directory");
        tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(&base)
            .expect("the fixture directory")
    }

    /// M018: an unproven learned root makes the exit code non-zero even when
    /// every root that could be proven was cleaned successfully. An unattended
    /// script must not read a partial Full cleanup as a complete one.
    #[test]
    fn an_unproven_learned_root_makes_the_exit_code_non_zero() {
        let unproven = vec![cargo_cleanme::domain::UnprovenRoot {
            root: std::path::PathBuf::from("/work/unproven"),
            detail: "1 permission-denied region(s) under it".into(),
        }];
        assert_eq!(
            cleanup_exit_code(&empty_report(), &unproven),
            1,
            "a skipped root means the boundary was not fully cleaned"
        );
        assert_eq!(
            cleanup_exit_code(&empty_report(), &[]),
            0,
            "a clean run stays zero"
        );
        let mut failed = empty_report();
        failed.failed = 1;
        assert_eq!(cleanup_exit_code(&failed, &[]), 1, "a failure is one");
        let mut blocked = empty_report();
        blocked.scope_blocked = Some(ScopeBlock::new(
            ScopeBlockReason::OwnershipUnproven,
            "blocked".into(),
        ));
        assert_eq!(cleanup_exit_code(&blocked, &[]), 1, "a block is one");
    }

    /// The interactive surface has three endings, and the orchestrator keeps
    /// them apart: a run to make, a deliberate quit, and a scope that proved
    /// no cleanup unit at all. Collapsing the third into a usage error is the
    /// defect this contract exists to prevent: the same scope cleaned without
    /// `-i` completes normally and exits 0.
    #[test]
    fn the_three_interactive_endings_map_to_three_distinct_outcomes() {
        let chosen = interactive_outcome(Ok(cargo_cleanme::interactive::SessionResult::Selected(
            vec![std::path::PathBuf::from("/tmp/project")],
        )));
        assert!(
            matches!(chosen, Ok(SelectionOutcome::Chosen(_))),
            "a chosen selection is a run to make: {chosen:?}"
        );

        let cancelled =
            interactive_outcome(Ok(cargo_cleanme::interactive::SessionResult::Cancelled));
        assert!(
            matches!(cancelled, Ok(SelectionOutcome::Cancelled)),
            "a deliberate quit is not a failure: {cancelled:?}"
        );

        let nothing = interactive_outcome(Ok(
            cargo_cleanme::interactive::SessionResult::NothingToClean,
        ));
        assert!(
            matches!(nothing, Ok(SelectionOutcome::NothingToClean)),
            "an empty surface is a plain negative result, not a usage error: {nothing:?}"
        );
    }

    /// A surface that could not be presented at all is still a usage error:
    /// the operator asked for something the run cannot deliver.
    #[test]
    fn a_surface_that_cannot_be_presented_is_still_a_usage_error() {
        let refused = interactive_outcome(Err("interactive selection requires a terminal".into()));
        assert!(
            matches!(refused, Err(AppError::Config(_))),
            "a refusal to present the surface is a usage error: {refused:?}"
        );
    }

    /// A runner that records every call, so a test can prove that a refusal
    /// reached the operator **without** spawning Cargo.
    struct CountingRunner {
        calls: std::sync::Mutex<Vec<Vec<std::ffi::OsString>>>,
    }

    impl CountingRunner {
        fn new() -> Self {
            Self {
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl cargo_cleanme::workspace::CargoRunner for CountingRunner {
        fn run(
            &self,
            _cwd: &std::path::Path,
            args: &[std::ffi::OsString],
        ) -> std::io::Result<cargo_cleanme::workspace::ProcessOutput> {
            self.calls.lock().unwrap().push(args.to_vec());
            Err(std::io::Error::other("the runner must not be reached"))
        }
    }

    /// The log line for an update that had nothing to do says the mode the
    /// run was actually asked for.
    ///
    /// The two "nothing to do" branches used to hardcode `simulate`, so
    /// `cargo cleanme update --format log` claimed a simulation while the
    /// JSON document printed beside it said `dry_run: false`. The two machine
    /// surfaces described one run differently.
    #[test]
    fn the_update_log_line_says_the_mode_the_run_was_asked_for() {
        let execute = update_nothing_to_do_log_line("0.2.10", "0.2.10", false);
        assert!(
            execute.contains("mode=execute"),
            "an execute-mode run must say so: {execute}"
        );
        let simulate = update_nothing_to_do_log_line("0.2.10", "0.2.10", true);
        assert!(
            simulate.contains("mode=simulate"),
            "a dry run must say so: {simulate}"
        );
    }

    /// A usage error that is knowable from the invocation alone is refused
    /// **before** the scope is resolved, and the scope resolution is what
    /// spawns Cargo.
    ///
    /// The refusal used to arrive after the resolution had already spawned
    /// `cargo locate-project` — and under `--full`, after a whole platform
    /// reconciliation had spawned thousands of `cargo metadata` processes.
    #[test]
    fn a_static_usage_error_is_refused_before_the_scope_spawns_cargo() {
        let d = tempfile::tempdir().unwrap();
        let request = CleanupRequest {
            scope: CleanupScope::Full,
            mode: CleanMode::Execute,
            overrides: cargo_cleanme::cli::CleanupOverrides::default(),
            selection: cargo_cleanme::cleanup::CleanupSelection::All,
            interactive: true,
        };
        let options = RunOptions {
            no_progress: true,
            stats: false,
            format: OutputFormat::Json,
            verbose: false,
        };
        let runner = CountingRunner::new();
        let refused = run_cleanup(request, &d.path().join("config.toml"), options, &runner);
        assert!(
            matches!(refused, Err(AppError::Config(_))),
            "a machine format with -i is a usage error: {refused:?}"
        );
        assert_eq!(
            runner.count(),
            0,
            "a refused run must not spawn Cargo at all"
        );
    }

    /// The scan surface spells its paths relative to the scope root, under a
    /// symlinked root exactly as it does under a plain one.
    ///
    /// The report's paths are canonical; the relative-path rule is a
    /// component-wise `strip_prefix`. A lexical base and a canonical path never
    /// match, so under a symlinked root the scan printed absolute paths while
    /// the cleanup surface — whose roots are canonicalized — printed relative
    /// ones, for the *same* root. On macOS this is the default for any root
    /// under `/tmp` (`/tmp` → `/private/tmp`).
    #[cfg(unix)]
    #[test]
    fn the_scan_surface_names_its_paths_relative_under_a_symlinked_root() {
        use std::os::unix::fs::symlink;

        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real/app");
        std::fs::create_dir_all(real.join("target")).unwrap();
        let link = d.path().join("lnk/app");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&real, &link).unwrap();

        let base = absolutize_for_display(&link);
        assert_eq!(
            base,
            std::fs::canonicalize(&real).unwrap(),
            "the relative-path base must be canonical, like the cleanup surface's"
        );

        // The group's own path is canonical (`workspace::canonical_or_absolute`),
        // and the base must match it component-wise.
        let display = std::fs::canonicalize(real.join("target")).unwrap();
        let options = cargo_cleanme::report::HumanOptions::quiet(vec![base]);
        assert_eq!(
            options.relativized(&display),
            std::path::PathBuf::from("app/target"),
            "the scan surface must name the path relative to the scope root"
        );
    }

    /// A report with nothing in it is the empty case: one true sentence.
    #[test]
    fn a_report_with_nothing_in_it_is_the_empty_case() {
        assert!(
            report_has_nothing_to_report(&empty_report()),
            "a run that cleaned nothing and has nothing to report has one true sentence"
        );
    }

    /// Every term of the verdict is something the operator would otherwise have
    /// to read. A report carrying any of them is not the empty case — in
    /// particular a unit that was found and deliberately left alone, which must
    /// never look like a machine with nothing to clean.
    #[test]
    fn a_report_that_says_anything_is_not_the_empty_case() {
        let mut with_row = empty_report();
        with_row.results.push(CleanResult {
            display_path: std::path::PathBuf::from("/tmp/project"),
            workspace_roots: Vec::new(),
            output_roots: Vec::new(),
            ownership: OutputOwnershipClass::PrivateBounded,
            outcome: CleanOutcome::Skipped,
            policy_disposition: None,
            reason_code: CleanupReasonCode::SkippedOutsideSelection,
            before_bytes: None,
            after_bytes: None,
            observed_decrease: None,
            detail: String::new(),
        });
        assert!(
            !report_has_nothing_to_report(&with_row),
            "a skipped unit the operator must see is not the empty case"
        );

        let mut with_diagnostic = empty_report();
        with_diagnostic.diagnostics = 1;
        assert!(
            !report_has_nothing_to_report(&with_diagnostic),
            "a discovery diagnostic the operator must see is not the empty case"
        );

        let mut with_out_of_scope = empty_report();
        with_out_of_scope.out_of_scope_unresolved = 1;
        assert!(
            !report_has_nothing_to_report(&with_out_of_scope),
            "an out-of-scope unprovable is not the empty case"
        );

        let mut with_failure = empty_report();
        with_failure.failed = 1;
        assert!(
            !report_has_nothing_to_report(&with_failure),
            "a failure is not the empty case"
        );

        let mut blocked = empty_report();
        blocked.scope_blocked = Some(ScopeBlock::new(
            ScopeBlockReason::IncompleteDiscovery,
            "the scope is uncertain".to_string(),
        ));
        assert!(
            !report_has_nothing_to_report(&blocked),
            "a blocked scope is not the empty case"
        );
    }
}
