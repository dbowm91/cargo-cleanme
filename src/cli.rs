use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Debug, Parser)]
#[command(
    name = "cargo-cleanme",
    version,
    about = "Find inactive Cargo artifacts and safely preview their cleanup"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    Scan {
        root: Option<PathBuf>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Preview or execute cleanup of revalidated Cargo build artifacts.
    Clean {
        /// Required sandbox root. Cleanup never scans the whole machine.
        root: PathBuf,
        /// Print Cargo's verbose dry-run plan (the default mode).
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
        /// Execute Cargo clean after per-project revalidation.
        #[arg(long, conflicts_with = "dry_run")]
        yes: bool,
    },
}
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Path,
    Init,
    Show,
}
impl Cli {
    pub fn scan_root(&self) -> Option<PathBuf> {
        match &self.command {
            None => None,
            Some(Command::Scan { root }) => root.clone(),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn empty_is_scan_intent() {
        let c = Cli::try_parse_from(["cargo-cleanme"]).unwrap();
        assert!(c.command.is_none());
    }
    #[test]
    fn root_parses() {
        let c = Cli::try_parse_from(["cargo-cleanme", "scan", "/tmp"]).unwrap();
        assert_eq!(c.scan_root(), Some(PathBuf::from("/tmp")));
    }
    #[test]
    fn config_command_parses() {
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "path"]).is_ok());
    }
    #[test]
    fn clean_requires_root_and_accepts_explicit_execution() {
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes"]).is_ok());
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes", "--dry-run"]).is_err()
        );
    }
}
