use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
#[derive(Debug, Parser)]
#[command(
    name = "cargo-cleanme",
    version,
    about = "Find inactive Cargo artifacts and safely preview their cleanup"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Disable transient progress UI (useful for benchmarks and debugging).
    #[arg(long, global = true)]
    pub no_progress: bool,
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
        /// Cargo preview: invoke Cargo's own dry-run (the default mode).
        /// Distinct from `--dryrun` (cargo-cleanme simulation).
        #[arg(long, conflicts_with = "yes", conflicts_with = "dryrun")]
        dry_run: bool,
        /// Application simulation: run the full decision/progress/report path
        /// but invoke no Cargo clean command. Distinct from `--dry-run`.
        #[arg(long = "dryrun", conflicts_with = "yes", conflicts_with = "dry_run")]
        dryrun: bool,
        /// Execute Cargo clean after per-workspace revalidation.
        #[arg(long, conflicts_with = "dry_run", conflicts_with = "dryrun")]
        yes: bool,
    },
}
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Path,
    Init {
        /// Replace the existing config file instead of refusing to overwrite.
        #[arg(long)]
        force: bool,
    },
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

    /// Parse the current process argv after Cargo external-subcommand normalization.
    pub fn parse_normalized() -> Self {
        Self::parse_from(normalize_cargo_argv(std::env::args_os()))
    }

    /// Parse an explicit argv vector after Cargo external-subcommand normalization.
    pub fn try_parse_normalized_from(
        args: impl IntoIterator<Item = OsString>,
    ) -> Result<Self, clap::Error> {
        Self::try_parse_from(normalize_cargo_argv(args))
    }
}

/// Normalize Cargo external-subcommand argv before clap parses it.
///
/// Cargo executes an external subcommand `cargo cleanme ...` by invoking the
/// `cargo-cleanme` binary with the subcommand name injected as the second argv
/// element (`["cargo-cleanme", "cleanme", ...]`). Direct invocation never
/// produces that shape for a valid command because `cleanme` is not a
/// `cargo-cleanme` subcommand; the only valid first arguments are
/// `scan`/`config`/`clean`/help/version flags or no argument at all.
///
/// This strips exactly one `cleanme` token in the documented second position
/// and leaves every later `cleanme` path/value untouched.
pub fn normalize_cargo_argv(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = args.into_iter().collect();
    if argv.len() >= 2 && argv[1] == "cleanme" {
        argv.remove(1);
    }
    argv
}

/// Resolve a user-supplied `clean ROOT` against the invocation directory into
/// an absolute lexical path.
///
/// The internal cleanup boundary keeps its absolute-root invariant; this only
/// normalizes CLI ergonomics. Canonicalization is deliberately not required:
/// the policy/cleanup layers still verify that the effective root exists, is
/// a real directory, and is not a symlink.
pub fn absolutize_root(root: &Path) -> PathBuf {
    if root.is_absolute() {
        return root.to_path_buf();
    }
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    absolutize_relative(&base, root)
}

/// Deterministic absolutization against an explicit base directory.
///
/// Production uses [`absolutize_root`] (current directory as base); tests use
/// this helper to avoid mutating the process-global current directory.
#[doc(hidden)]
pub fn absolutize_root_for_test(base: &Path, root: &Path) -> PathBuf {
    if root.is_absolute() {
        return root.to_path_buf();
    }
    absolutize_relative(base, root)
}

pub(crate) fn absolutize_relative(base: &Path, relative: &Path) -> PathBuf {
    let mut out = if base.is_absolute() {
        base.to_path_buf()
    } else {
        PathBuf::from("/").join(base)
    };
    for component in relative.components() {
        match component {
            Component::Prefix(part) => {
                out = PathBuf::from(part.as_os_str());
            }
            Component::RootDir => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Lexical pop only: never follows symlinks. A `..` that would
                // rise above the filesystem root stays at the root; otherwise
                // it resolves against the actual caller-selected directory
                // stack without escaping it through surprising semantics.
                let _ = out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::ffi::OsString;
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
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "init"]).is_ok());
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "init", "--force"]).is_ok());
    }
    #[test]
    fn clean_requires_root_and_accepts_explicit_execution() {
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes"]).is_ok());
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes", "--dry-run"]).is_err()
        );
    }
    #[test]
    fn clean_modes_conflict_pairwise_and_default_is_preview() {
        // Default (no flag) parses; mode mapping is tested in cleanup.
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp"]).is_ok());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--dry-run"]).is_ok());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--dryrun"]).is_ok());
        // Pairwise conflicts.
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes", "--dry-run"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--yes", "--dryrun"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--dry-run", "--dryrun"])
                .is_err()
        );
        // External subcommand form works for all modes.
        for args in [
            vec!["cargo-cleanme", "cleanme", "clean", "/x"],
            vec!["cargo-cleanme", "cleanme", "clean", "/x", "--dry-run"],
            vec!["cargo-cleanme", "cleanme", "clean", "/x", "--dryrun"],
            vec!["cargo-cleanme", "cleanme", "clean", "/x", "--yes"],
        ] {
            let normalized: Vec<OsString> = args.iter().map(OsString::from).collect::<Vec<_>>();
            let parsed = Cli::try_parse_from(normalize_cargo_argv(normalized)).unwrap();
            assert!(matches!(parsed.command, Some(Command::Clean { .. })));
        }
    }

    fn argv(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn direct_argv_is_unchanged() {
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme"])),
            argv(&["cargo-cleanme"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "scan", "/x"])),
            argv(&["cargo-cleanme", "scan", "/x"])
        );
    }

    #[test]
    fn cargo_external_argv_strips_exactly_one_documented_token() {
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme"])),
            argv(&["cargo-cleanme"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "scan", "/x"])),
            argv(&["cargo-cleanme", "scan", "/x"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "config", "init"])),
            argv(&["cargo-cleanme", "config", "init"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "clean", "/x", "--yes"])),
            argv(&["cargo-cleanme", "clean", "/x", "--yes"])
        );
    }

    #[test]
    fn later_literal_cleanme_values_are_preserved() {
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "scan", "cleanme"])),
            argv(&["cargo-cleanme", "scan", "cleanme"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "scan", "cleanme"])),
            argv(&["cargo-cleanme", "scan", "cleanme"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "clean", "./cleanme", "--yes"])),
            argv(&["cargo-cleanme", "clean", "./cleanme", "--yes"])
        );
    }

    #[test]
    fn help_and_version_forms_normalize() {
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "--help"])),
            argv(&["cargo-cleanme", "--help"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "--version"])),
            argv(&["cargo-cleanme", "--version"])
        );
        assert_eq!(
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "clean", "--help"])),
            argv(&["cargo-cleanme", "clean", "--help"])
        );
    }

    #[test]
    fn direct_and_external_forms_parse_equivalently() {
        for (direct, external) in [
            (
                argv(&["cargo-cleanme"]),
                argv(&["cargo-cleanme", "cleanme"]),
            ),
            (
                argv(&["cargo-cleanme", "scan", "/x"]),
                argv(&["cargo-cleanme", "cleanme", "scan", "/x"]),
            ),
            (
                argv(&["cargo-cleanme", "config", "path"]),
                argv(&["cargo-cleanme", "cleanme", "config", "path"]),
            ),
            (
                argv(&["cargo-cleanme", "clean", "/x"]),
                argv(&["cargo-cleanme", "cleanme", "clean", "/x"]),
            ),
        ] {
            let a = Cli::try_parse_from(normalize_cargo_argv(direct)).unwrap();
            let b = Cli::try_parse_from(normalize_cargo_argv(external)).unwrap();
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "direct and external forms must agree"
            );
        }
    }

    #[test]
    fn normalized_config_init_force_parses() {
        let c =
            Cli::try_parse_normalized_from(argv(&["cargo-cleanme", "cleanme", "config", "init"]))
                .unwrap();
        assert!(matches!(
            c.command,
            Some(Command::Config {
                command: ConfigCommand::Init { force: false }
            })
        ));
        let c = Cli::try_parse_normalized_from(argv(&[
            "cargo-cleanme",
            "cleanme",
            "config",
            "init",
            "--force",
        ]))
        .unwrap();
        assert!(matches!(
            c.command,
            Some(Command::Config {
                command: ConfigCommand::Init { force: true }
            })
        ));
    }

    #[test]
    fn absolute_root_is_stable() {
        let absolute = if cfg!(windows) {
            PathBuf::from(r"C:\projects")
        } else {
            PathBuf::from("/projects")
        };
        assert_eq!(absolutize_root(&absolute), absolute);
    }

    #[test]
    fn relative_root_becomes_absolute() {
        let cwd = std::env::current_dir().unwrap();
        let joined = absolutize_root(Path::new("./project"));
        assert!(joined.is_absolute());
        assert_eq!(joined, cwd.join("project"));
        let dotdot = absolutize_relative(Path::new("/a/b"), Path::new("c/../d"));
        assert_eq!(dotdot, PathBuf::from("/a/b/d"));
        let up = absolutize_relative(Path::new("/a/b"), Path::new("../outside"));
        assert_eq!(up, PathBuf::from("/a/outside"));
        let rooted = absolutize_relative(Path::new("/a"), Path::new("../../x"));
        assert_eq!(rooted, PathBuf::from("/x"));
    }

    #[test]
    fn no_progress_flag_parses_globally() {
        let c = Cli::try_parse_from(["cargo-cleanme", "--no-progress", "scan", "/tmp"]).unwrap();
        assert!(c.no_progress);
        let c = Cli::try_parse_from(["cargo-cleanme", "scan", "/tmp"]).unwrap();
        assert!(!c.no_progress);
        let c = Cli::try_parse_from(["cargo-cleanme", "--no-progress"]).unwrap();
        assert!(c.no_progress);
        assert!(c.command.is_none());
    }
}
