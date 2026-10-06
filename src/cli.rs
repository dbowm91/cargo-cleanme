use crate::cleanup::CleanMode;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    /// One bounded ASCII summary line per report, for unattended schedulers
    /// that retain a short history tail. Not a machine contract: complete
    /// consumers use `json`.
    Log,
}

#[derive(Debug, Parser)]
#[command(
    name = "cargo-cleanme",
    version,
    about = "Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only",
    long_about = "Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only.\n\n\
Bare `cargo cleanme` runs routine maintenance: it selects the maintenance scope \
(bounded seed roots plus active learned roots, or a configured scan.root when one exists), \
re-proves complete ownership for every discovered participant, and executes eligible Cargo \
cleanup units. `--dry-run` performs the identical decision and proof path while invoking no \
`cargo clean` process at all.\n\n\
`cargo cleanme scan` with no ROOT is the full read-only machine reconciliation; it is the only \
operation allowed to expire learned roots. `cargo cleanme scan ROOT` is a bounded explicit scan, \
and `cargo cleanme scan --known` is the read-only routine inventory.\n\n\
Deletion is always delegated to Cargo; cargo-cleanme never removes a directory itself."
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Disable transient progress UI (useful for benchmarks and debugging).
    #[arg(long, global = true)]
    pub no_progress: bool,
    /// Print detailed scan/cleanup counters and phase timings to stderr.
    /// Never changes deterministic stdout. `--no-progress --stats` is the
    /// canonical benchmark/debug combination.
    #[arg(long, global = true)]
    pub stats: bool,
    /// Select human-readable output, the versioned JSON automation contract, or
    /// one bounded ASCII summary line for unattended schedulers.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    /// Simulate: run every decision, proof, and reporting step, but invoke no
    /// `cargo clean` process at all. Honoured wherever it appears, so the
    /// `cargo cleanme --dry-run clean ROOT` spelling cannot fall back to
    /// Execute.
    #[arg(long, conflicts_with = "dryrun_legacy")]
    pub dry_run: bool,
    /// Historical spelling of `--dry-run`; retained as a hidden alias.
    #[arg(long = "dryrun", hide = true, conflicts_with = "dry_run")]
    pub dryrun_legacy: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Reconcile the machine read-only: full by default, explicit with ROOT,
    /// routine inventory with `--known`.
    Scan {
        /// Optional bounded root. Makes the scan Explicit and bypasses
        /// configured ignore/unignore filters.
        root: Option<PathBuf>,
        /// Routine read-only inventory over the maintenance scope.
        #[arg(long, conflicts_with = "root")]
        known: bool,
        /// Historical spelling of no-root Full `scan`; retained as a hidden alias.
        #[arg(long, hide = true, conflicts_with_all = ["root", "known"])]
        full: bool,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Preview or execute cleanup of revalidated Cargo build artifacts.
    Clean {
        /// Optional bounded sandbox root. Omit to use the maintenance scope
        /// (identical to bare invocation) or select `--known` / `--full`.
        root: Option<PathBuf>,
        /// Clean bounded roots selected by the current Routine policy.
        #[arg(long, conflicts_with_all = ["root", "full"])]
        known: bool,
        /// Complete Full reconciliation before cleaning its learned roots.
        #[arg(long, conflicts_with_all = ["root", "known"])]
        full: bool,
        /// Require at least this many reclaimable bytes per workspace.
        #[arg(long)]
        min_reclaimable_bytes: Option<u64>,
        /// Require this many seconds of inactivity.
        #[arg(long)]
        older_than: Option<u64>,
        /// Include canonical workspace roots matching this glob (repeatable).
        #[arg(long = "include")]
        include: Vec<String>,
        /// Exclude canonical workspace roots matching this glob (repeatable).
        #[arg(long = "exclude")]
        exclude: Vec<String>,
        /// Ask Cargo to clean one named profile. Selector byte estimates are unknown.
        #[arg(long, conflicts_with = "package", value_parser = clap::builder::NonEmptyStringValueParser::new())]
        profile: Option<String>,
        /// Clean one validated workspace package through qualified Cargo versions.
        #[arg(long, conflicts_with = "profile", value_parser = clap::builder::NonEmptyStringValueParser::new())]
        package: Option<String>,
        /// Application simulation: run the full decision/proof/report path but
        /// invoke no Cargo clean command. The default mode is Execute.
        #[arg(long, conflicts_with_all = ["cargo_preview", "yes_legacy", "dryrun_legacy"])]
        dry_run: bool,
        /// Cargo preview: after the same complete final proof, invoke Cargo's
        /// own `clean --dry-run --verbose`. Never removes anything.
        #[arg(long, conflicts_with_all = ["dry_run", "yes_legacy", "dryrun_legacy"])]
        cargo_preview: bool,
        /// Historical spelling of the (now default) Execute mode; retained as a
        /// hidden alias.
        #[arg(long = "yes", hide = true, conflicts_with_all = ["dry_run", "cargo_preview", "dryrun_legacy"])]
        yes_legacy: bool,
        /// Historical spelling of `--dry-run`; retained as a hidden alias.
        #[arg(long = "dryrun", hide = true, conflicts_with_all = ["dry_run", "cargo_preview", "yes_legacy"])]
        dryrun_legacy: bool,
    },
    /// Update this cargo-cleanme to the latest stable published release.
    ///
    /// The registry is the version authority, the release tag is constructed
    /// from that version rather than scraped, and the candidate is replaced
    /// only after its `.sha256` evidence is verified and it identifies itself
    /// as exactly the target version. A Cargo-managed installation is refused
    /// with the exact manager command, because Cargo owns that file and its
    /// bookkeeping.
    Update {
        /// Resolve the plan and report it without acquiring or replacing bytes.
        #[arg(long)]
        dry_run: bool,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Subcommand)]
pub enum ConfigCommand {
    Path,
    Show,
    Edit,
}

/// Which roots a cleanup intent selects.
///
/// Bare maintenance and `clean` with no scope selector deliberately resolve to
/// the *same* variant, so the two spellings cannot drift apart in root
/// selection, policy, proof, or reporting: there is one cleanup engine and one
/// set of gates behind both.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanupScope {
    /// Maintenance scope, resolved by [`crate::policy::resolve`] with no CLI
    /// root and no Full request: a configured legacy `scan.root` wins as an
    /// exclusive Explicit override, otherwise bounded seed roots plus active
    /// learned roots form the Routine scope.
    Maintenance,
    /// One bounded explicit root supplied as `clean ROOT`.
    Root(PathBuf),
    /// `clean --full`: complete Full reconciliation first, then cleanup of the
    /// learned roots it retained.
    Full,
}

/// Which roots a read-only scan selects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanIntent {
    /// `scan` with no ROOT: full machine reconciliation under platform policy.
    ///
    /// This never consults a configured `scan.root`; configuration must not be
    /// able to silently narrow the canonical reconciliation command.
    Full,
    /// `scan ROOT`: bounded explicit scan that bypasses discovery filters.
    Explicit(PathBuf),
    /// `scan --known`: the non-Full maintenance scope, read-only. Learned roots
    /// may advance positive observations but are never pruned here.
    Maintenance,
}

/// Advanced cleanup policy overrides.
///
/// These live only on the `clean` subcommand by design (M012A work package D):
/// bare maintenance intentionally uses the configured/default policy rather
/// than cloning the whole advanced option surface onto the root command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanupOverrides {
    pub min_reclaimable_bytes: Option<u64>,
    pub older_than: Option<u64>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub profile: Option<String>,
    pub package: Option<String>,
}

/// One resolved cleanup intent: what to clean, in what mode, with which
/// policy overrides.
///
/// Bare maintenance and every advanced cleanup spelling produce this same
/// value, so there is exactly one cleanup code path behind all of them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupRequest {
    pub scope: CleanupScope,
    pub mode: CleanMode,
    pub overrides: CleanupOverrides,
}

/// The resolved operation, scope, and mode for one invocation.
///
/// This is the single internal representation shared by bare maintenance and
/// every advanced cleanup spelling, so "no subcommand" is an explicit Routine
/// cleanup intent rather than a fallthrough to scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Invocation {
    Cleanup(CleanupRequest),
    Scan(ScanIntent),
    Config(ConfigCommand),
    Update { dry_run: bool },
}

/// Resolve the mode flag triple onto the canonical mode.
///
/// Execute is the default: absent a mode flag, cleanup executes eligible units.
/// The hidden compatibility aliases select the same two canonical modes and
/// never introduce a fourth mode or a different safety path.
pub fn clean_mode(dry_run: bool, cargo_preview: bool, dryrun_legacy: bool) -> CleanMode {
    debug_assert!(
        !(dry_run && cargo_preview),
        "clap rejects conflicting mode flags before this mapping runs"
    );
    if dry_run || dryrun_legacy {
        CleanMode::Simulate
    } else if cargo_preview {
        CleanMode::CargoPreview
    } else {
        CleanMode::Execute
    }
}

impl Cli {
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

    /// Collapse parsed argv into one resolved operation/scope/mode.
    pub fn invocation(&self) -> Invocation {
        match &self.command {
            // No subcommand *is* routine maintenance, not a scan fallthrough.
            None => Invocation::Cleanup(CleanupRequest {
                scope: CleanupScope::Maintenance,
                mode: clean_mode(self.dry_run, false, self.dryrun_legacy),
                overrides: CleanupOverrides::default(),
            }),
            Some(Command::Scan { root, known, .. }) => Invocation::Scan(match (root, *known) {
                (Some(root), _) => ScanIntent::Explicit(root.clone()),
                (None, true) => ScanIntent::Maintenance,
                (None, false) => ScanIntent::Full,
            }),
            Some(Command::Config { command }) => Invocation::Config(*command),
            // The root-level `--dry-run` is honoured here too, not only on the
            // bare front door: `cargo cleanme --dry-run clean ROOT` is the
            // spelling Cargo itself passes, so a flag parsed before the
            // subcommand must never silently fall back to Execute.
            Some(Command::Update { dry_run }) => Invocation::Update {
                dry_run: *dry_run || self.dry_run || self.dryrun_legacy,
            },
            // `yes_legacy` is intentionally not bound: `--yes` used to be the
            // only way to ask for Execute, Execute is now the default, so the
            // flag carries no intent of its own and printing nothing about it
            // is what keeps unattended runs quiet.
            Some(Command::Clean {
                root,
                known,
                full,
                dry_run,
                cargo_preview,
                dryrun_legacy,
                min_reclaimable_bytes,
                older_than,
                include,
                exclude,
                profile,
                package,
                ..
            }) => {
                let scope = match (root, *full, *known) {
                    (Some(root), _, _) => CleanupScope::Root(root.clone()),
                    (None, true, _) => CleanupScope::Full,
                    // `clean` with no selector is the same intent as bare
                    // invocation, not a third scope.
                    (None, false, _) => CleanupScope::Maintenance,
                };
                Invocation::Cleanup(CleanupRequest {
                    scope,
                    // A dry run asked for *before* the subcommand is a dry run,
                    // exactly like one asked for after it: clap cannot make a
                    // root flag mutually exclusive with a subcommand's flags,
                    // so the combination is resolved here instead. Simulation
                    // outranks every other cleanup mode because it is the one
                    // that spawns no Cargo process at all.
                    mode: if self.dry_run || self.dryrun_legacy {
                        CleanMode::Simulate
                    } else {
                        clean_mode(*dry_run, *cargo_preview, *dryrun_legacy)
                    },
                    overrides: CleanupOverrides {
                        min_reclaimable_bytes: *min_reclaimable_bytes,
                        older_than: *older_than,
                        include: include.clone(),
                        exclude: exclude.clone(),
                        profile: profile.clone(),
                        package: package.clone(),
                    },
                })
            }
        }
    }
}

/// Normalize Cargo external-subcommand argv before clap parses it.
///
/// Cargo executes an external subcommand `cargo cleanme ...` by invoking the
/// `cargo-cleanme` binary with the subcommand name injected as the second argv
/// element (`["cargo-cleanme", "cleanme", ...]`). Direct invocation never
/// produces that shape for a valid command because `cleanme` is not a
/// `cargo-cleanme` subcommand; the only valid first arguments are
/// `scan`/`config`/`clean`/`update`/help/version flags or no argument at all.
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
    use clap::{CommandFactory, Parser};
    use std::ffi::OsString;

    fn maintenance(mode: CleanMode) -> CleanupRequest {
        cleanup(CleanupScope::Maintenance, mode)
    }

    fn cleanup(scope: CleanupScope, mode: CleanMode) -> CleanupRequest {
        CleanupRequest {
            scope,
            mode,
            overrides: CleanupOverrides::default(),
        }
    }
    #[test]
    fn bare_invocation_is_routine_execute_maintenance_not_scan() {
        // ADR 003 §1: bare invocation is the canonical maintenance operation.
        // This is the regression that fails against the old bare-read-only
        // dispatch, where `Cli::command` was None and main fell through to
        // `run_scan(None, false, ...)`.
        let parsed = Cli::try_parse_from(["cargo-cleanme"]).unwrap();
        assert!(parsed.command.is_none());
        assert_eq!(
            parsed.invocation(),
            Invocation::Cleanup(maintenance(CleanMode::Execute))
        );
    }

    #[test]
    fn bare_dry_run_is_simulate_and_the_legacy_alias_maps_to_it() {
        for args in [
            vec!["cargo-cleanme", "--dry-run"],
            vec!["cargo-cleanme", "--dryrun"],
        ] {
            let parsed = Cli::try_parse_from(args.clone()).unwrap();
            assert_eq!(
                parsed.invocation(),
                Invocation::Cleanup(maintenance(CleanMode::Simulate)),
                "{args:?}"
            );
        }
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "--dry-run", "--dryrun"]).is_err(),
            "the alias must not be combinable with the canonical spelling"
        );
    }

    #[test]
    fn a_dry_run_before_the_subcommand_is_still_a_dry_run() {
        // `cargo cleanme --dry-run clean ROOT` is the spelling Cargo itself
        // passes, and it used to parse cleanly while the root field was never
        // read on that arm -- so the flag was silently discarded and a real
        // `cargo clean` ran. The canonical spelling was correct, which is what
        // made it silent.
        for args in [
            vec!["cargo-cleanme", "--dry-run", "clean", "/tmp/p"],
            vec!["cargo-cleanme", "--dryrun", "clean", "/tmp/p"],
        ] {
            let parsed = Cli::try_parse_from(args.clone()).unwrap();
            assert_eq!(
                parsed.invocation(),
                Invocation::Cleanup(cleanup(
                    CleanupScope::Root(std::path::PathBuf::from("/tmp/p")),
                    CleanMode::Simulate,
                )),
                "{args:?} must not fall back to Execute"
            );
        }
        // The bare front door keeps its own spelling working.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "--dry-run"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(CleanupScope::Maintenance, CleanMode::Simulate))
        );
        // And so does the position after the subcommand.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp/p", "--dry-run"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(std::path::PathBuf::from("/tmp/p")),
                CleanMode::Simulate,
            ))
        );
        // Without the flag, Execute is still the default in that position.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp/p"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(std::path::PathBuf::from("/tmp/p")),
                CleanMode::Execute,
            ))
        );
    }

    #[test]
    fn a_dry_run_before_update_is_still_a_dry_run() {
        // Same route, same bug: the root flag was discarded before the
        // `Update` arm too, so `cargo cleanme --dry-run update` would have
        // replaced the binary.
        for args in [
            vec!["cargo-cleanme", "--dry-run", "update"],
            vec!["cargo-cleanme", "--dryrun", "update"],
        ] {
            assert_eq!(
                Cli::try_parse_from(args.clone()).unwrap().invocation(),
                Invocation::Update { dry_run: true },
                "{args:?} must not replace the binary"
            );
        }
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "update"])
                .unwrap()
                .invocation(),
            Invocation::Update { dry_run: false }
        );
    }

    #[test]
    fn a_root_dry_run_wins_over_cargo_preview_because_it_spawns_nothing() {
        // clap cannot make a root flag mutually exclusive with a subcommand's
        // flags, so the combination reaches here. Simulation is the mode that
        // spawns no Cargo process at all, and a user who asked for it before
        // the subcommand must not be handed a Cargo invocation instead.
        assert_eq!(
            Cli::try_parse_from([
                "cargo-cleanme",
                "--dry-run",
                "clean",
                "/tmp/p",
                "--cargo-preview"
            ])
            .unwrap()
            .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(std::path::PathBuf::from("/tmp/p")),
                CleanMode::Simulate,
            ))
        );
        // Without the root flag, Cargo preview is unaffected.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp/p", "--cargo-preview"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(std::path::PathBuf::from("/tmp/p")),
                CleanMode::CargoPreview,
            ))
        );
    }

    #[test]
    fn scan_scope_selection_is_full_explicit_or_maintenance() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "scan"])
                .unwrap()
                .invocation(),
            Invocation::Scan(ScanIntent::Full)
        );
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "scan", "/tmp"])
                .unwrap()
                .invocation(),
            Invocation::Scan(ScanIntent::Explicit(PathBuf::from("/tmp")))
        );
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "scan", "--known"])
                .unwrap()
                .invocation(),
            Invocation::Scan(ScanIntent::Maintenance)
        );
        // The hidden `--full` alias is exactly no-root Full.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "scan", "--full"])
                .unwrap()
                .invocation(),
            Invocation::Scan(ScanIntent::Full)
        );
    }

    #[test]
    fn scan_scope_conflicts_fail_at_invocation_time() {
        for args in [
            vec!["cargo-cleanme", "scan", "/tmp", "--known"],
            vec!["cargo-cleanme", "scan", "/tmp", "--full"],
            vec!["cargo-cleanme", "scan", "--known", "--full"],
        ] {
            assert!(
                Cli::try_parse_from(args.clone()).is_err(),
                "{args:?} must be rejected before any traversal"
            );
        }
    }

    #[test]
    fn hidden_full_alias_is_not_advertised_in_scan_help() {
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("--full"), "{help}");
        assert!(help.contains("--known"), "{help}");
    }

    #[test]
    fn clean_scope_selectors_stay_mutually_exclusive() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(PathBuf::from("/tmp")),
                CleanMode::Execute
            ))
        );
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "--known"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(maintenance(CleanMode::Execute))
        );
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "--full"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(CleanupScope::Full, CleanMode::Execute))
        );
        // No selector at all is the same intent as bare invocation.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean"])
                .unwrap()
                .invocation(),
            Cli::try_parse_from(["cargo-cleanme"]).unwrap().invocation()
        );
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/x", "--known"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "--known", "--full"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/x", "--full"]).is_err());
    }

    #[test]
    fn cleanup_modes_default_to_execute_and_expose_cargo_preview() {
        for (args, expected) in [
            (vec!["cargo-cleanme", "clean", "/tmp"], CleanMode::Execute),
            (
                vec!["cargo-cleanme", "clean", "/tmp", "--dry-run"],
                CleanMode::Simulate,
            ),
            (
                vec!["cargo-cleanme", "clean", "/tmp", "--cargo-preview"],
                CleanMode::CargoPreview,
            ),
            (
                vec!["cargo-cleanme", "clean", "/tmp", "--yes"],
                CleanMode::Execute,
            ),
            (
                vec!["cargo-cleanme", "clean", "/tmp", "--dryrun"],
                CleanMode::Simulate,
            ),
        ] {
            let parsed = Cli::try_parse_from(args.clone()).unwrap();
            assert_eq!(
                parsed.invocation(),
                Invocation::Cleanup(cleanup(CleanupScope::Root(PathBuf::from("/tmp")), expected)),
                "{args:?}"
            );
        }
        // Every mode pair is rejected rather than silently precedence-resolved.
        for args in [
            vec![
                "cargo-cleanme",
                "clean",
                "/tmp",
                "--dry-run",
                "--cargo-preview",
            ],
            vec!["cargo-cleanme", "clean", "/tmp", "--dry-run", "--yes"],
            vec!["cargo-cleanme", "clean", "/tmp", "--cargo-preview", "--yes"],
            vec!["cargo-cleanme", "clean", "/tmp", "--dry-run", "--dryrun"],
            vec![
                "cargo-cleanme",
                "clean",
                "/tmp",
                "--cargo-preview",
                "--dryrun",
            ],
            vec!["cargo-cleanme", "clean", "/tmp", "--yes", "--dryrun"],
        ] {
            assert!(Cli::try_parse_from(args.clone()).is_err(), "{args:?}");
        }
    }

    #[test]
    fn compatibility_aliases_are_hidden_but_accepted() {
        let clean_help = Cli::command()
            .find_subcommand_mut("clean")
            .expect("clean subcommand")
            .render_long_help()
            .to_string();
        assert!(!clean_help.contains("--yes"), "{clean_help}");
        assert!(!clean_help.contains("--dryrun"), "{clean_help}");
        assert!(clean_help.contains("--cargo-preview"), "{clean_help}");
        assert!(clean_help.contains("--dry-run"), "{clean_help}");
    }

    #[test]
    fn update_keeps_its_own_dry_run_meaning() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "update", "--dry-run"])
                .unwrap()
                .invocation(),
            Invocation::Update { dry_run: true }
        );
    }

    #[test]
    fn format_log_parses_globally() {
        // M012B §11.1: `--format log` is a global value, so it is accepted
        // before or after a subcommand and through the external form.
        for args in [
            vec!["cargo-cleanme", "--format", "log"],
            vec!["cargo-cleanme", "scan", "--format", "log"],
            vec!["cargo-cleanme", "--format", "log", "scan"],
            vec!["cargo-cleanme", "clean", "/x", "--format", "log"],
            vec!["cargo-cleanme", "cleanme", "--format", "log", "scan"],
        ] {
            let normalized: Vec<OsString> = args.iter().map(OsString::from).collect();
            let parsed = Cli::try_parse_from(normalize_cargo_argv(normalized)).unwrap();
            assert_eq!(parsed.format, OutputFormat::Log, "{args:?}");
        }
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme"]).unwrap().format,
            OutputFormat::Human,
            "log is opt-in; a non-TTY stdout must never select it"
        );
        assert!(Cli::try_parse_from(["cargo-cleanme", "--format", "yaml"]).is_err());
    }

    #[test]
    fn config_command_parses() {
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "path"]).is_ok());
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "edit"]).is_ok());
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "init"]).is_err());
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "config", "show"])
                .unwrap()
                .invocation(),
            Invocation::Config(ConfigCommand::Show)
        );
    }

    #[test]
    fn root_parses() {
        let c = Cli::try_parse_from(["cargo-cleanme", "scan", "/tmp"]).unwrap();
        assert_eq!(
            c.invocation(),
            Invocation::Scan(ScanIntent::Explicit(PathBuf::from("/tmp")))
        );
    }

    #[test]
    fn cleanup_policy_options_are_repeatable_and_mode_independent() {
        let parsed = Cli::try_parse_from([
            "cargo-cleanme",
            "clean",
            "--known",
            "--min-reclaimable-bytes",
            "42",
            "--older-than",
            "60",
            "--include",
            "**/one",
            "--include",
            "**/two",
            "--exclude",
            "**/tmp",
            "--dry-run",
        ])
        .unwrap();
        let intent = parsed.invocation();
        assert!(
            matches!(parsed.command, Some(Command::Clean { min_reclaimable_bytes: Some(42), older_than: Some(60), include, exclude, .. }) if include.len() == 2 && exclude == ["**/tmp"])
        );
        assert_eq!(
            intent,
            Invocation::Cleanup(CleanupRequest {
                scope: CleanupScope::Maintenance,
                mode: CleanMode::Simulate,
                overrides: CleanupOverrides {
                    min_reclaimable_bytes: Some(42),
                    older_than: Some(60),
                    include: vec!["**/one".into(), "**/two".into()],
                    exclude: vec!["**/tmp".into()],
                    profile: None,
                    package: None,
                },
            }),
            "advanced policy overrides travel with the intent, not beside it"
        );
        let selected =
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--profile", "custom"]).unwrap();
        assert!(
            matches!(selected.command, Some(Command::Clean { profile: Some(p), .. }) if p == "custom")
        );
        assert!(
            Cli::try_parse_from([
                "cargo-cleanme",
                "clean",
                "/tmp",
                "--profile",
                "release",
                "--package",
                "app",
            ])
            .is_err()
        );
    }

    #[test]
    fn orchestration_roots_are_mutually_exclusive_and_external_forms_match() {
        for mode in ["--known", "--full"] {
            let direct =
                Cli::try_parse_from(["cargo-cleanme", "clean", mode, "--dry-run"]).unwrap();
            let external = Cli::try_parse_normalized_from(argv(&[
                "cargo-cleanme",
                "cleanme",
                "clean",
                mode,
                "--dry-run",
            ]))
            .unwrap();
            assert_eq!(direct.invocation(), external.invocation());
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
            normalize_cargo_argv(argv(&["cargo-cleanme", "cleanme", "config", "show"])),
            argv(&["cargo-cleanme", "config", "show"])
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
                argv(&["cargo-cleanme", "--dry-run"]),
                argv(&["cargo-cleanme", "cleanme", "--dry-run"]),
            ),
            (
                argv(&["cargo-cleanme", "scan", "--known"]),
                argv(&["cargo-cleanme", "cleanme", "scan", "--known"]),
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
                a.invocation(),
                b.invocation(),
                "direct and external forms must agree"
            );
        }
    }

    #[test]
    fn config_init_is_removed_and_edit_parses() {
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "init"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "config", "edit"]).is_ok());
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
        assert!(matches!(
            c.invocation(),
            Invocation::Cleanup(CleanupRequest { .. })
        ));
    }

    #[test]
    fn stats_flag_parses_globally_in_direct_and_external_forms() {
        // C002 §7.8: `--stats` is additive and global.
        let c = Cli::try_parse_from(["cargo-cleanme", "--stats", "scan", "/tmp"]).unwrap();
        assert!(c.stats);
        let c = Cli::try_parse_from(["cargo-cleanme", "scan", "/tmp"]).unwrap();
        assert!(!c.stats);
        let c = Cli::try_parse_from(["cargo-cleanme", "--no-progress", "--stats", "scan", "/tmp"])
            .unwrap();
        assert!(c.no_progress);
        assert!(c.stats);
        // Clean modes.
        for args in [
            vec!["cargo-cleanme", "clean", "/x", "--stats"],
            vec!["cargo-cleanme", "clean", "/x", "--dry-run", "--stats"],
            vec!["cargo-cleanme", "clean", "/x", "--cargo-preview", "--stats"],
            vec!["cargo-cleanme", "clean", "/x", "--yes", "--stats"],
        ] {
            let c = Cli::try_parse_from(args.clone()).unwrap();
            assert!(c.stats, "{args:?}");
        }
        // External subcommand forms.
        for args in [
            vec!["cargo-cleanme", "cleanme", "--stats", "scan", "/x"],
            vec!["cargo-cleanme", "cleanme", "clean", "/x", "--stats"],
            vec![
                "cargo-cleanme",
                "cleanme",
                "--no-progress",
                "--stats",
                "scan",
                "/x",
            ],
        ] {
            let normalized: Vec<OsString> = args.iter().map(OsString::from).collect::<Vec<_>>();
            let parsed = Cli::try_parse_from(normalize_cargo_argv(normalized)).unwrap();
            assert!(parsed.stats, "{args:?}");
        }
    }
}
