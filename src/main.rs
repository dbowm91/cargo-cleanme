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
            dry_run: _,
            yes,
        }) => {
            let config = config::load(&path)?;
            let mode = if yes {
                cargo_cleanme::cleanup::CleanMode::Execute
            } else {
                cargo_cleanme::cleanup::CleanMode::DryRun
            };
            // Accept a relative CLI root for ergonomics, but keep the
            // cleanup safety boundary absolute.
            let effective_root = cli::absolutize_root(&root);
            let report =
                cargo_cleanme::cleanup::clean(&effective_root, config.scan.recency_seconds, mode)?;
            println!("{}", report.render());
            Ok(if report.failed > 0 { 1 } else { 0 })
        }
        Some(Command::Scan { root }) => run_scan(root, &path),
        None => run_scan(None, &path),
    }
}
fn run_scan(
    root: Option<std::path::PathBuf>,
    config_path: &std::path::Path,
) -> Result<i32, AppError> {
    let scan_start = std::time::SystemTime::now();
    let c = config::load(config_path)?;
    let p = cargo_cleanme::policy::resolve(
        cargo_cleanme::domain::ScanRequest { cli_root: root },
        &c.scan,
    )?;
    let mut report = cargo_cleanme::discovery::scan(&p)?;
    let clock = cargo_cleanme::analysis::ScanClock::new(scan_start, p.recency)
        .ok_or_else(|| AppError::Config("recency window exceeds system time range".into()))?;
    let candidates = std::mem::take(&mut report.eligible);
    // Candidate analysis shares one bounded worker pool for target sizing;
    // source activity still short-circuits sequentially per candidate.
    let discovered: Vec<cargo_cleanme::domain::DiscoveredProject> = candidates
        .into_iter()
        .map(|candidate| cargo_cleanme::domain::DiscoveredProject {
            project_root: candidate.project_root.clone(),
            manifest_path: candidate.project_root.join("Cargo.toml"),
            target_path: candidate.artifact.target_path,
        })
        .collect();
    let (eligible, diagnostics) = cargo_cleanme::analysis::analyze_many(&discovered, &clock);
    report.eligible = eligible;
    report.diagnostics.extend(diagnostics);
    println!("{}", cargo_cleanme::report::render(&mut report));
    if !report.diagnostics.is_empty() {
        eprintln!(
            "{} filesystem diagnostics; rerun with a bounded root if needed",
            report.diagnostics.len()
        );
    }
    Ok(0)
}
