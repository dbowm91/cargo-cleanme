use cargo_cleanme::{
    cli::{Cli, Command, ConfigCommand},
    config::{self, ConfigPathResolver},
    error::AppError,
};
use clap::Parser;
fn main() {
    if let Err(e) = run() {
        eprintln!("cargo-cleanme: {e}");
        std::process::exit(2)
    }
}
fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let path = ConfigPathResolver::new(cli.config.clone()).path()?;
    match cli.command {
        Some(Command::Config { command }) => match command {
            ConfigCommand::Path => println!("{}", path.display()),
            ConfigCommand::Init => {
                config::init(&path)?;
                println!("created {}", path.display())
            }
            ConfigCommand::Show => println!("{}", config::show(&config::load(&path)?)?),
        },
        _ => {
            let scan_start = std::time::SystemTime::now();
            let root = cli.scan_root();
            let c = config::load(&path)?;
            let p = cargo_cleanme::policy::resolve(
                cargo_cleanme::domain::ScanRequest { cli_root: root },
                &c.scan,
            )?;
            let mut report = cargo_cleanme::discovery::scan(&p)?;
            let clock = cargo_cleanme::analysis::ScanClock::new(scan_start, p.recency).ok_or_else(
                || AppError::Config("recency window exceeds system time range".into()),
            )?;
            let candidates = std::mem::take(&mut report.eligible);
            for candidate in candidates {
                match cargo_cleanme::analysis::analyze(
                    &cargo_cleanme::domain::DiscoveredProject {
                        project_root: candidate.project_root.clone(),
                        manifest_path: candidate.project_root.join("Cargo.toml"),
                        target_path: candidate.artifact.target_path,
                    },
                    &clock,
                ) {
                    Ok(Some(eligible)) => report.eligible.push(eligible),
                    Ok(None) => {}
                    Err(d) => report.diagnostics.push(d),
                }
            }
            println!("{}", cargo_cleanme::report::render(&mut report));
            if !report.diagnostics.is_empty() {
                eprintln!(
                    "{} filesystem diagnostics; rerun with a bounded root if needed",
                    report.diagnostics.len()
                );
            }
        }
    }
    Ok(())
}
