use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Debug, Parser)]
#[command(
    name = "cargo-cleanme",
    version,
    about = "Read-only inventory of inactive Cargo artifacts"
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
}
