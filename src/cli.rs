use crate::cleanup::{CleanMode, CleanupSelection};
use clap::{CommandFactory, Parser, Subcommand};
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

/// The cleanup surface: **one** declaration, shared by the root and the
/// hidden `clean` alias.
///
/// C045. The root is the canonical cleanup front door (ADR 008 §4b) and
/// `clean` is a hidden compatibility alias for the spellings published since
/// v0.2.0. Both levels accept the same cleanup flags, so the flags are
/// declared once here and flattened into both. A flag that exists on one
/// level and not the other is not a detail — it is a published spelling that
/// stopped working — which is why
/// `the_alias_accepts_every_cleanup_flag_the_root_declares` reads the clap
/// model and fails if the two levels ever diverge.
///
/// The flags that belong to one level only are *not* here: `--interactive` is
/// global on the root, and `--known` and `--yes` belong to the alias.
///
/// The display orders are pinned because the root's option order is part of
/// the published surface: `--dry-run` sits before `-i, --interactive`, which
/// sits before `--full`.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct CleanupFlags {
    /// Simulate: run every decision, proof, and reporting step, but invoke no
    /// `cargo clean` process at all. Honoured wherever it appears, so the
    /// `cargo cleanme --dry-run ROOT` spelling cannot fall back to
    /// Execute. Only the cleanup surface (and `update`'s own `--dry-run`)
    /// honour it: `scan` and `config` ignore a global `--dry-run` (read-only /
    /// local-only, no behaviour change).
    #[arg(long, conflicts_with = "dryrun_legacy", display_order = 5)]
    pub dry_run: bool,
    /// Historical spelling of `--dry-run`; retained as a hidden alias.
    #[arg(long = "dryrun", hide = true, conflicts_with = "dry_run")]
    pub dryrun_legacy: bool,
    /// Complete Full reconciliation before cleaning its learned roots.
    ///
    /// The canonical spelling of the Full cleanup operation (ADR 008 §4b).
    /// `cargo cleanme clean --full` remains accepted as a hidden alias.
    #[arg(long, conflicts_with = "root")]
    pub full: bool,
    /// Optional bounded sandbox root. Omit to use the maintenance scope
    /// (identical to bare invocation) or select `--full`.
    ///
    /// A directory whose name collides with a subcommand name (`scan`,
    /// `clean`, `config`, `update`) is read as that subcommand: the
    /// subcommand wins. Spell such a directory `./scan` to name it.
    #[arg(conflicts_with = "full")]
    pub root: Option<PathBuf>,
    /// Require at least this many reclaimable bytes per workspace.
    #[arg(long)]
    pub min_reclaimable_bytes: Option<u64>,
    /// Require this many seconds of inactivity.
    #[arg(long)]
    pub older_than: Option<u64>,
    /// Include canonical workspace roots matching this glob (repeatable).
    #[arg(long = "include")]
    pub include: Vec<String>,
    /// Exclude canonical workspace roots matching this glob (repeatable).
    #[arg(long = "exclude")]
    pub exclude: Vec<String>,
    /// Ask Cargo to clean one named profile. Selector byte estimates are unknown.
    #[arg(long, conflicts_with = "package", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub profile: Option<String>,
    /// Clean one validated workspace package through qualified Cargo versions.
    #[arg(long, conflicts_with = "profile", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub package: Option<String>,
    /// Cargo preview: after the same complete final proof, invoke Cargo's own
    /// `clean --dry-run --verbose`. Never removes anything.
    #[arg(long, conflicts_with_all = ["dry_run", "dryrun_legacy"])]
    pub cargo_preview: bool,
}

impl CleanupFlags {
    /// The policy overrides these flags carry.
    pub fn overrides(&self) -> CleanupOverrides {
        CleanupOverrides {
            min_reclaimable_bytes: self.min_reclaimable_bytes,
            older_than: self.older_than,
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            profile: self.profile.clone(),
            package: self.package.clone(),
        }
    }
}

#[derive(Debug, clap::Args)]
#[command(
    name = "cargo-cleanme",
    version,
    about = "Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only",
    long_about = "Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only.\n\n\
Bare `cargo cleanme` runs routine maintenance: it selects the maintenance scope \
(bounded seed roots plus active learned roots plus the working directory, or a \
configured scan.root when one exists), re-proves complete ownership for every \
discovered participant, and executes eligible Cargo cleanup units. The working \
directory is part of that scope, so a bare run cleans what is under it -- and the \
scope only ever grows, never narrows. `--dry-run` performs the identical decision \
and proof path while invoking no `cargo clean` process at all.\n\n\
The root is the cleanup front door: `cargo cleanme ROOT` cleans one bounded \
root, `cargo cleanme --full` runs the Full cleanup -- a complete machine \
reconciliation, then the learned roots it can prove -- and `--cargo-preview` and \
the policy overrides work here too. A directory whose name collides with a \
subcommand name is read as that subcommand; spell it `./scan` to name it.\n\n\
`cargo cleanme scan` needs a scope: `--all` is the full read-only machine \
reconciliation and the only operation allowed to expire learned roots, \
`scan ROOT` is a bounded explicit scan, and `scan --known` is the read-only \
maintenance inventory.\n\n\
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
    /// Show the detailed human surface: every workspace considered (including
    /// the ones that were skipped), the scope census, the per-phase counters,
    /// and the diagnostic detail.
    ///
    /// The default surface names only what the run acted on. This flag changes
    /// the *human* format only — the JSON document and the log line are
    /// identical with and without it.
    #[arg(long, global = true)]
    pub verbose: bool,
    /// Choose the workspaces to clean from an inline selection list (NOR/INS).
    ///
    /// Resolves the cleanup units read-only, presents one row per unit with its
    /// reclaimable bytes, and cleans only what is checked. A selection can
    /// remove candidates but never add one: ownership is still proven across
    /// the complete universe and the engine still re-proves it per spawn.
    ///
    /// The list is a bounded page, not the whole screen: PageUp, PageDown,
    /// Home and End move through it, and `i` filters it by path.
    ///
    /// Requires a terminal on stdin and stdout. Combine with `--dry-run` to
    /// review a selection without invoking `cargo clean`.
    //
    // The flag is global, so it reaches the `clean` alias too: it is the one
    // cleanup flag the alias inherits rather than carrying.
    #[arg(short = 'i', long, global = true, display_order = 7)]
    pub interactive: bool,
    /// The cleanup surface, shared with the hidden `clean` alias.
    #[command(flatten)]
    pub cleanup: CleanupFlags,
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The command model, with the alias's presentation applied.
///
/// C045: the alias and the root declare the *same* flags, and the alias's
/// difference is presentation — it must not advertise a second front door.
/// Applying that here (rather than at each parse site) keeps one model: the
/// binary, the tests and the generated man pages and completions all see the
/// same surface.
impl CommandFactory for Cli {
    fn command() -> clap::Command {
        let command = <Self as clap::Args>::augment_args(clap::Command::new("cargo-cleanme"));
        configure_cleanup_alias(command)
    }

    fn command_for_update() -> clap::Command {
        <Self as clap::Args>::augment_args_for_update(clap::Command::new("cargo-cleanme"))
    }
}

impl Parser for Cli {}

/// The hidden `clean` alias's `--dry-run` help.
///
/// The root's `--dry-run` help explains the whole simulation contract; the
/// alias says the same thing in one line. The text is the alias's, verbatim,
/// because the alias's rendered help is a published surface.
const ALIAS_DRY_RUN_HELP: &str = "Application simulation: run the full decision/proof/report path \
     but invoke no Cargo clean command. The default mode is Execute";

/// Give the hidden `clean` alias its presentation.
///
/// C045: the alias carries the root's cleanup surface — the same
/// declaration, flattened into both levels — but it advertises only `--dry-run`
/// and its own `--known`. Everything else it accepts is a hidden alias, which
/// is what keeps the root the only advertised front door.
///
/// The two levels differ in *presentation* only. Every flag the root declares
/// is accepted by the alias, which is what
/// `the_alias_accepts_every_cleanup_flag_the_root_declares` proves.
fn configure_cleanup_alias(command: clap::Command) -> clap::Command {
    // `mut_subcommands` walks the subcommands in place; `mut_subcommand`
    // would move `clean` to the end of the list and reorder the generated
    // completions, which is a published surface.
    command.mut_subcommands(|subcommand| {
        if subcommand.get_name() != "clean" {
            return subcommand;
        }
        subcommand.mut_args(|arg| {
            let id = arg.get_id().clone();
            match id.as_str() {
                // The alias's one advertised cleanup flag. Its help is the
                // short form, and it sorts after the inherited globals.
                "dry_run" => arg.help(ALIAS_DRY_RUN_HELP).display_order(900),
                // The alias's own scope selector, advertised next to the
                // inherited globals.
                "known" => arg.display_order(1),
                // Everything else the alias carries is a hidden alias.
                _ => arg.hide(true),
            }
        })
    })
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Reconcile the machine read-only: full with `--all`, explicit with ROOT,
    /// routine inventory with `--known`.
    ///
    /// Exactly one scope selector is required. An `ArgGroup` expresses that
    /// rather than a per-argument `required_unless_present_all`, because a
    /// positional ROOT does not satisfy `required_unless_present` in clap 4 —
    /// `scan /tmp` still demanded `--all`. The group also makes the selectors
    /// mutually exclusive as a consequence of the same declaration, so the
    /// conflict rules cannot drift from the requiredness rules.
    #[command(group = clap::ArgGroup::new("scan-scope").required(true).args(["root", "known", "all"]))]
    Scan {
        /// Optional bounded root. Makes the scan Explicit and bypasses
        /// configured ignore/unignore filters.
        root: Option<PathBuf>,
        /// Routine read-only inventory over the maintenance scope.
        #[arg(long)]
        known: bool,
        /// Full machine reconciliation.
        ///
        /// Required when no ROOT and no `--known` is given: this is the only
        /// operation that may expire learned roots, and it walks the whole
        /// platform, so it is never one keystroke from the front door.
        //
        // The historical spelling is a hidden alias of this argument rather
        // than an argument of its own. An `ArgGroup` renders every one of its
        // members in the usage line, so a separate hidden argument would have
        // advertised the alias in `scan --help` — which is what "hidden" is
        // supposed to mean — and in the generated completions.
        #[arg(short = 'a', long, alias = "full")]
        all: bool,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Preview or execute cleanup of revalidated Cargo build artifacts.
    ///
    /// A hidden compatibility alias (ADR 008 §4b): the root is the canonical
    /// cleanup front door, and this subcommand keeps every published spelling
    /// working — `clean ROOT`, `clean --known` and `clean --full` — without
    /// advertising a second front door.
    //
    // C045: the alias exists for **published-spelling compatibility** and for
    // nothing else. It is not what makes a bare run in a target directory
    // work: the working directory joins the maintenance scope on its own
    // (ADR 009 / C040), so `cargo cleanme` in a target directory already
    // cleans that directory. Keeping that distinction here is what stops the
    // alias from being "simplified" into the canonical path — which would
    // deprecate the bare invocation ADR 009 made canonical.
    #[command(hide = true)]
    Clean {
        /// The cleanup surface, shared with the root.
        #[command(flatten)]
        flags: CleanupFlags,
        /// Clean bounded roots selected by the current Routine policy.
        #[arg(long, conflicts_with_all = ["root", "full"], display_order = 1)]
        known: bool,
        /// Historical spelling of the (now default) Execute mode; retained as a
        /// hidden alias.
        #[arg(long = "yes", hide = true, conflicts_with_all = ["dry_run", "cargo_preview", "dryrun_legacy"])]
        yes_legacy: bool,
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
    ///
    /// ADR 009 adds the working directory to that scope as an additional
    /// root. The scope only ever grows: narrowing it would hide the
    /// out-of-scope claimants of a shared target and promote `shared` bytes
    /// to `private`, the only class that authorizes deletion.
    Maintenance,
    /// One bounded explicit root supplied as `cargo cleanme ROOT`.
    Root(PathBuf),
    /// `cargo cleanme --full`: complete Full reconciliation first, then cleanup
    /// of the learned roots it retained.
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
/// These live on the **root** and on the hidden `clean` alias, and nowhere
/// else (M019, ADR 008 §4b): the root is the canonical cleanup front door,
/// and the alias exists to keep the spellings published since v0.2.0
/// working. Both levels read the same declaration — [`CleanupFlags`] — so
/// the two cannot drift apart.
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
    /// Which resolved cleanup units are candidates. M014: the bare invocation
    /// anchors this to the working directory; `cargo cleanme ROOT` and
    /// `cargo cleanme --full` leave it unfiltered because their roots already
    /// bound it.
    ///
    /// `WorkingDirectory` here is *unresolved intent*. `main.rs` must call
    /// [`CleanupSelection::resolved`] before the engine runs.
    ///
    /// `-i` narrows this further: the interactive session offers only
    /// anchor-matching units and replaces them with the operator's choice,
    /// which is a strictly narrower `Roots` subset of the anchor.
    pub selection: CleanupSelection,
    /// `-i`/`--interactive`: present the selection list before cleaning.
    ///
    /// Kept separate from `selection` because the two answer different
    /// questions — *how* units are chosen versus *whether* a session runs at
    /// all — and because the interactive selection can only be resolved once a
    /// terminal exists, which argv parsing must not do.
    pub interactive: bool,
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
    Update {
        dry_run: bool,
    },
    /// An argv shape clap should have rejected before it reached this point.
    ///
    /// `scan` requires one of ROOT / `--known` / `--all`; clap enforces that,
    /// so this arm is unreachable through the parser. It exists because the
    /// compiler cannot see clap's invariant, and the two available fallbacks
    /// were both wrong: defaulting to Full would silently run the machine-wide
    /// reconciliation the flag exists to gate, and panicking would exit 101,
    /// outside the 0/1/2 contract.
    UsageError(String),
}

/// Resolve the mode flag triple onto the canonical mode.
///
/// Execute is the default: absent a mode flag, cleanup executes eligible units.
/// The hidden compatibility aliases select the same two canonical modes and
/// never introduce a fourth mode or a different safety path.
pub fn clean_mode(dry_run: bool, cargo_preview: bool, dryrun_legacy: bool) -> CleanMode {
    debug_assert!(
        !((dry_run || dryrun_legacy) && cargo_preview),
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
            // The root is the cleanup front door (ADR 008 §4b): it carries the
            // scope, the mode, and the policy overrides.
            //
            // C045: the bare invocation is the *same* derivation with an empty
            // alias, not a second one. There is one scope rule.
            None => match self.cleanup_request(&CleanAlias::default()) {
                Ok(request) => Invocation::Cleanup(request),
                Err(message) => Invocation::UsageError(message),
            },
            Some(Command::Scan { root, known, all }) => {
                if let Some(message) = self.reject_cleanup_flags_for("scan") {
                    return Invocation::UsageError(message);
                }
                match (root, *known, *all) {
                    (Some(root), _, _) => Invocation::Scan(ScanIntent::Explicit(root.clone())),
                    (None, true, _) => Invocation::Scan(ScanIntent::Maintenance),
                    // clap requires one of ROOT / --known / --all, so reaching the
                    // Full arm means an explicit flag (or its hidden alias) was
                    // actually given rather than defaulted.
                    (None, false, true) => Invocation::Scan(ScanIntent::Full),
                    // Unreachable through `clap`; fail closed rather than guess.
                    (None, false, false) => {
                        Invocation::UsageError("scan requires a ROOT, --known, or --all".into())
                    }
                }
            }
            Some(Command::Config { command }) => {
                if let Some(message) = self.reject_cleanup_flags_for("config") {
                    return Invocation::UsageError(message);
                }
                Invocation::Config(*command)
            }
            // The root-level `--dry-run` is honoured here too, not only on the
            // bare front door: `cargo cleanme --dry-run clean ROOT` is the
            // spelling Cargo itself passes, so a flag parsed before the
            // subcommand must never silently fall back to Execute.
            Some(Command::Update { dry_run }) => {
                if let Some(message) = self.reject_cleanup_flags_for("update") {
                    return Invocation::UsageError(message);
                }
                Invocation::Update {
                    dry_run: *dry_run || self.cleanup.dry_run || self.cleanup.dryrun_legacy,
                }
            }
            // `yes_legacy` is intentionally not bound: `--yes` used to be the
            // only way to ask for Execute, Execute is now the default, so the
            // flag carries no intent of its own and printing nothing about it
            // is what keeps unattended runs quiet.
            Some(Command::Clean {
                flags,
                known,
                yes_legacy: _,
            }) => match self.cleanup_request(&CleanAlias {
                flags: flags.clone(),
                known: *known,
            }) {
                Ok(request) => Invocation::Cleanup(request),
                Err(message) => Invocation::UsageError(message),
            },
        }
    }

    /// The cleanup intent behind one invocation, across the root and the alias.
    ///
    /// M019: the root and `clean` are two spellings of one intent. A value
    /// given at the root wins; a value given on `clean` is used when the root
    /// did not supply one; the same override at both levels is a usage error,
    /// because silently preferring one of two values is the discarded-flag
    /// failure mode ADR 008 §3 exists to prevent.
    ///
    /// C045: this is the **only** scope rule. The bare invocation is this
    /// derivation with an empty alias, so the two spellings cannot drift
    /// apart in root selection, policy, proof, or reporting — there is one
    /// cleanup engine and one set of gates behind both.
    fn cleanup_request(&self, alias: &CleanAlias) -> Result<CleanupRequest, String> {
        // The scope selectors: `--full` and the ROOT positional at either
        // level, and `--known` on `clean`.
        if self.cleanup.full && alias.flags.full {
            return Err(duplicate_override("--full"));
        }
        if self.cleanup.root.is_some() && alias.flags.root.is_some() {
            return Err(duplicate_override("ROOT"));
        }
        if self.cleanup.cargo_preview && alias.flags.cargo_preview {
            return Err(duplicate_override("--cargo-preview"));
        }
        // The same simulation at both levels is a usage error too, whichever
        // spelling carries it: the rule is one override at one level, and
        // silently preferring one of two values is the discarded-flag failure
        // mode ADR 008 §3 exists to prevent.
        if (self.cleanup.dry_run || self.cleanup.dryrun_legacy)
            && (alias.flags.dry_run || alias.flags.dryrun_legacy)
        {
            return Err(duplicate_dry_run(&dry_run_spellings(
                self.cleanup.dry_run,
                self.cleanup.dryrun_legacy,
                alias.flags.dry_run,
                alias.flags.dryrun_legacy,
            )));
        }
        // A Full scope and a bounded ROOT select different scopes, and they
        // cannot be merged: one of them would have to be discarded, and the
        // discarded one here is the *bounded* selector, so a bounded cleanup
        // would silently become a machine-wide one.
        if self.cleanup.full && alias.flags.root.is_some() {
            return Err(conflicting_scope_selectors("`--full`", "ROOT"));
        }
        if self.cleanup.root.is_some() && alias.flags.full {
            return Err(conflicting_scope_selectors("ROOT", "`--full`"));
        }
        let scope = if self.cleanup.full || alias.flags.full {
            // `clean --known` is the bare invocation, and the bare invocation
            // is the maintenance scope: a Full scope and `--known` select
            // different scopes, and `clean --full --known` is already refused
            // by clap.
            if alias.known {
                // `--known` was given only on `clean`; the flag that collides
                // with it is the root's own scope selector.
                return Err(conflicting_scopes(if self.cleanup.full {
                    "`--full`"
                } else {
                    "`--full` on `clean`"
                }));
            }
            CleanupScope::Full
        } else if let Some(root) = self
            .cleanup
            .root
            .clone()
            .or_else(|| alias.flags.root.clone())
        {
            if alias.known {
                // As above: `clean ROOT --known` is refused by clap, so the
                // ROOT that collides with `--known` is the root-level one.
                return Err(conflicting_scopes(if self.cleanup.root.is_some() {
                    "`ROOT`"
                } else {
                    "`ROOT` on `clean`"
                }));
            }
            CleanupScope::Root(root)
        } else {
            // `clean` with no selector is the same intent as the bare
            // invocation, not a third scope.
            CleanupScope::Maintenance
        };
        Ok(CleanupRequest {
            selection: selection_for(&scope),
            scope,
            mode: merged_clean_mode(
                (
                    self.cleanup.dry_run,
                    self.cleanup.cargo_preview,
                    self.cleanup.dryrun_legacy,
                ),
                (
                    alias.flags.dry_run,
                    alias.flags.cargo_preview,
                    alias.flags.dryrun_legacy,
                ),
            ),
            overrides: merge_overrides(&self.cleanup.overrides(), &alias.flags.overrides())?,
            interactive: self.interactive,
        })
    }

    /// A root-level cleanup flag handed to a read-only or local-only
    /// subcommand is a usage error, never a silently discarded flag.
    ///
    /// M019: `--full` selects a cleanup operation and the policy overrides
    /// select Cargo targets; neither means anything to `scan`, `config` or
    /// `update`. Discarding them would look exactly like the flag was
    /// honoured.
    ///
    /// `--interactive` is a cleanup flag too: it selects which cleanup units
    /// are cleaned. A read-only subcommand has nothing to select, so the
    /// flag is refused rather than ignored.
    fn reject_cleanup_flags_for(&self, subcommand: &str) -> Option<String> {
        let mut given: Vec<&str> = Vec::new();
        if self.cleanup.full {
            given.push("--full");
        }
        if self.cleanup.root.is_some() {
            given.push("ROOT");
        }
        if self.interactive {
            given.push("--interactive");
        }
        if self.cleanup.min_reclaimable_bytes.is_some() {
            given.push("--min-reclaimable-bytes");
        }
        if self.cleanup.older_than.is_some() {
            given.push("--older-than");
        }
        if !self.cleanup.include.is_empty() {
            given.push("--include");
        }
        if !self.cleanup.exclude.is_empty() {
            given.push("--exclude");
        }
        if self.cleanup.profile.is_some() {
            given.push("--profile");
        }
        if self.cleanup.package.is_some() {
            given.push("--package");
        }
        if self.cleanup.cargo_preview {
            given.push("--cargo-preview");
        }
        if given.is_empty() {
            return None;
        }
        Some(format!(
            "`{subcommand}` does not accept {}: these are cleanup flags, and \
             `cargo cleanme {subcommand}` is read-only. Run the cleanup without the \
             subcommand, or run `{subcommand}` on its own.",
            given.join(", ")
        ))
    }
}

/// The selection that goes with a scope.
///
/// M014: the working-directory anchor belongs to the *maintenance* spellings,
/// whose Routine scope spans the machine. A bounded root and the Full scope
/// already bound their own scope and stay unfiltered, so the two cannot
/// silently disagree about what "everything" means.
fn selection_for(scope: &CleanupScope) -> CleanupSelection {
    match scope {
        CleanupScope::Maintenance => CleanupSelection::WorkingDirectory,
        CleanupScope::Root(_) | CleanupScope::Full => CleanupSelection::All,
    }
}

/// The cleanup mode across the root and the `clean` alias.
///
/// M019: a simulation at either level wins over a Cargo preview at either
/// level. `--dry-run` says "invoke no `cargo clean` at all"; a Cargo preview
/// does invoke one. The single-level [`clean_mode`] keeps its stricter
/// contract, because there clap has already rejected the combination.
fn merged_clean_mode(root: (bool, bool, bool), clean: (bool, bool, bool)) -> CleanMode {
    let (root_dry_run, root_cargo_preview, root_dryrun_legacy) = root;
    let (clean_dry_run, clean_cargo_preview, clean_dryrun_legacy) = clean;
    if root_dry_run || root_dryrun_legacy || clean_dry_run || clean_dryrun_legacy {
        CleanMode::Simulate
    } else if root_cargo_preview || clean_cargo_preview {
        CleanMode::CargoPreview
    } else {
        CleanMode::Execute
    }
}

/// The same override given at two levels is ambiguous.
fn duplicate_override(flag: &str) -> String {
    format!(
        "{flag} was given at the root and on `clean`. The two values are ambiguous; \
         give it at one level only."
    )
}

/// The spellings of `--dry-run` the operator actually used.
///
/// `--dryrun` is the hidden legacy alias. Naming the canonical spelling when
/// the operator typed the alias points them at a flag they did not use, and
/// one `--help` does not show.
fn dry_run_spellings(
    cleanup_dry_run: bool,
    cleanup_dryrun_legacy: bool,
    alias_dry_run: bool,
    alias_dryrun_legacy: bool,
) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    if cleanup_dry_run || alias_dry_run {
        names.push("--dry-run");
    }
    if cleanup_dryrun_legacy || alias_dryrun_legacy {
        names.push("--dryrun");
    }
    names
}

/// The same override given at two levels, named by the spelling used.
///
/// The flag is a boolean, so there are no two *values* to be ambiguous about:
/// both spellings mean the same thing, and the refusal is about the
/// invocation, not the value. Saying "the two values are ambiguous" about a
/// flag that carries no value points the operator at a conflict that does not
/// exist.
fn duplicate_dry_run(spellings: &[&str]) -> String {
    let named = spellings.join(" and ");
    let verb = if spellings.len() == 1 { "was" } else { "were" };
    format!("{named} {verb} given at the root and on `clean`. Give it at one level only.")
}

/// Two mutually exclusive selectors were given at different levels.
///
/// `--profile` and `--package` are one logical override: clap refuses them at
/// the same level, and they are refused across the root/`clean` boundary for
/// the same reason. Merging them independently would let the profile win and
/// the package disappear — the discarded-flag failure mode ADR 008 §3 exists
/// to prevent, and the consequence here is a *different* destructive action
/// than the one that was asked for.
fn conflicting_selectors(root_flag: &str, clean_flag: &str) -> String {
    format!(
        "{root_flag} was given at the root and {clean_flag} on `clean`. The two selectors are \
         mutually exclusive; give one selector at one level only."
    )
}

/// Two scope selectors were given at different levels.
///
/// A Full scope and `--known` select different scopes, so the root's scope
/// selector and `clean --known` cannot both stand. The message names the two
/// flags that actually collide: the root's own flag was never `--known`.
fn conflicting_scopes(root_flag: &str) -> String {
    format!(
        "{root_flag} at the root and `--known` on `clean` select different scopes; \
         give one scope selector at one level only."
    )
}

/// Two scope selectors that select different scopes were given at different levels.
///
/// A Full scope sweeps the machine; a bounded ROOT confines the cleanup to one
/// tree. They cannot be merged into one intent, so one of them would have to
/// be discarded — and the discarded-flag failure mode ADR 008 §3 exists to
/// prevent is at its worst here, because the discarded selector is the
/// *bounded* one.
fn conflicting_scope_selectors(root_flag: &str, clean_flag: &str) -> String {
    format!(
        "{root_flag} at the root and {clean_flag} on `clean` select different scopes; \
         give one scope selector at one level only."
    )
}

/// Merge the root's cleanup overrides with `clean`'s.
///
/// M019: a value given at the root wins; a value given on `clean` is used when
/// the root did not supply one; the same override at both levels is a usage
/// error.
fn merge_overrides(
    root: &CleanupOverrides,
    clean: &CleanupOverrides,
) -> Result<CleanupOverrides, String> {
    // The two selectors are mutually exclusive as a pair, not as two
    // independent values, so the pair is checked before either is merged.
    if root.profile.is_some() && clean.package.is_some() {
        return Err(conflicting_selectors("--profile", "--package"));
    }
    if root.package.is_some() && clean.profile.is_some() {
        return Err(conflicting_selectors("--package", "--profile"));
    }
    Ok(CleanupOverrides {
        min_reclaimable_bytes: merge_one(
            "--min-reclaimable-bytes",
            root.min_reclaimable_bytes,
            clean.min_reclaimable_bytes,
        )?,
        older_than: merge_one("--older-than", root.older_than, clean.older_than)?,
        include: merge_many("--include", &root.include, &clean.include)?,
        exclude: merge_many("--exclude", &root.exclude, &clean.exclude)?,
        profile: merge_one("--profile", root.profile.clone(), clean.profile.clone())?,
        package: merge_one("--package", root.package.clone(), clean.package.clone())?,
    })
}

/// Merge one optional override given at the root and on `clean`.
fn merge_one<T: Clone>(flag: &str, root: Option<T>, clean: Option<T>) -> Result<Option<T>, String> {
    match (root, clean) {
        (Some(_), Some(_)) => Err(duplicate_override(flag)),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

/// Merge one repeatable override given at the root and on `clean`.
fn merge_many(flag: &str, root: &[String], clean: &[String]) -> Result<Vec<String>, String> {
    match (root.is_empty(), clean.is_empty()) {
        (false, false) => Err(duplicate_override(flag)),
        (false, true) => Ok(root.to_vec()),
        (true, false) => Ok(clean.to_vec()),
        (true, true) => Ok(Vec::new()),
    }
}

/// The hidden `clean` alias's own parsed flags.
///
/// M019: the alias carries the same intent as the root, so it is merged with
/// the root's flags rather than resolved on its own.
///
/// C045: the alias's cleanup flags are the *same declaration* as the root's
/// ([`CleanupFlags`]), not a second copy of it. The alias's own extra flags
/// are `--known` and `--yes`; everything else it accepts comes from the
/// shared declaration.
#[derive(Clone, Debug, Default)]
struct CleanAlias {
    flags: CleanupFlags,
    known: bool,
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

/// Resolve a user-supplied `cargo cleanme ROOT` against the invocation
/// directory into an absolute lexical path.
///
/// The internal cleanup boundary keeps its absolute-root invariant; this only
/// normalizes CLI ergonomics. Canonicalization is deliberately not required:
/// the policy/cleanup layers still verify that the effective root exists, is
/// a real directory, and is not a symlink.
///
/// A relative root with an unresolvable current directory is an `InvalidRoot`
/// usage error, never a silent fallback: hiding the IO error rewrote
/// `clean ./p` to `/p`.
pub fn absolutize_root(root: &Path) -> Result<PathBuf, crate::error::AppError> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }
    let base = std::env::current_dir().map_err(|e| crate::error::AppError::InvalidRoot {
        path: root.display().to_string(),
        reason: format!("cannot resolve current directory: {e}"),
    })?;
    Ok(absolutize_relative(&base, root))
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
        // Mirror `Cli::invocation` exactly: the working-directory anchor belongs
        // to the maintenance spellings, whose Routine scope spans the machine.
        // `cargo cleanme ROOT` / `cargo cleanme --full` are already bounded by
        // their own roots.
        let selection = match &scope {
            CleanupScope::Maintenance => CleanupSelection::WorkingDirectory,
            CleanupScope::Root(_) | CleanupScope::Full => CleanupSelection::All,
        };
        CleanupRequest {
            scope,
            mode,
            overrides: CleanupOverrides::default(),
            selection,
            interactive: false,
        }
    }
    /// M019 test 1: the canonical spelling resolves to the Full scope — the
    /// *same* value the compatibility alias resolves to.
    ///
    /// *Fails if:* the root flag is parsed but not bound.
    #[test]
    fn the_root_full_flag_resolves_to_the_full_scope() {
        let canonical = Cli::try_parse_from(["cargo-cleanme", "--full"])
            .unwrap()
            .invocation();
        assert_eq!(
            canonical,
            Invocation::Cleanup(cleanup(CleanupScope::Full, CleanMode::Execute)),
            "the canonical spelling must resolve to the Full scope"
        );
        // The alias resolves to the same value: the spelling changed, the
        // intent did not.
        let alias = Cli::try_parse_from(["cargo-cleanme", "clean", "--full"])
            .unwrap()
            .invocation();
        assert_eq!(canonical, alias, "the two spellings must not drift");
    }

    /// M019 test 2: the compatibility alias still resolves, and `clean` is
    /// absent from the root help.
    ///
    /// *Fails if:* the alias is dropped, or the hidden subcommand reappears in
    /// the help text.
    #[test]
    fn the_clean_alias_still_resolves_and_is_hidden_from_the_help() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "--full"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(CleanupScope::Full, CleanMode::Execute))
        );
        let help = Cli::command().render_long_help().to_string();
        // The `Commands:` section must not list the hidden subcommand. The
        // check is on the command *names*, not on the prose: the long-about
        // names `cargo cleanme` throughout.
        let listed: Vec<&str> = help
            .split("Commands:")
            .nth(1)
            .and_then(|rest| rest.split("Arguments:").next())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert!(
            !listed.contains(&"clean"),
            "the hidden subcommand must not reappear in the help text: {listed:?}"
        );
        let command = Cli::command();
        let clean = command.find_subcommand("clean").expect("clean subcommand");
        assert!(
            clean.is_hide_set(),
            "the clean alias must stay hidden from the root help"
        );
        assert!(
            help.contains("--full"),
            "the canonical Full cleanup flag must be advertised: {help}"
        );
    }

    /// M019 test 3: the policy overrides work at the root.
    ///
    /// *Fails if:* the root value is parsed but discarded.
    #[test]
    fn the_policy_overrides_work_at_the_root() {
        let root = Cli::try_parse_from([
            "cargo-cleanme",
            "--older-than",
            "3600",
            "--min-reclaimable-bytes",
            "4096",
            "--include",
            "/a/*",
            "--exclude",
            "/b/*",
        ])
        .unwrap()
        .invocation();
        let alias = Cli::try_parse_from([
            "cargo-cleanme",
            "clean",
            "--older-than",
            "3600",
            "--min-reclaimable-bytes",
            "4096",
            "--include",
            "/a/*",
            "--exclude",
            "/b/*",
        ])
        .unwrap()
        .invocation();
        let Invocation::Cleanup(root_request) = root else {
            panic!("the root spelling must resolve to a cleanup");
        };
        let Invocation::Cleanup(alias_request) = alias else {
            panic!("the alias must resolve to a cleanup");
        };
        assert_eq!(root_request.overrides, alias_request.overrides);
        assert_eq!(root_request.overrides.older_than, Some(3600));
        assert_eq!(root_request.overrides.min_reclaimable_bytes, Some(4096));
        assert_eq!(root_request.overrides.include, vec!["/a/*".to_string()]);
        assert_eq!(root_request.overrides.exclude, vec!["/b/*".to_string()]);
    }

    /// M019 test 11: the root positional resolves to the bounded-root scope —
    /// the *same* value the compatibility alias resolves to.
    ///
    /// *Fails if:* the root positional is parsed but not bound, or is bound to
    /// the maintenance scope.
    #[test]
    fn the_root_positional_resolves_to_the_bounded_root_scope() {
        let canonical = Cli::try_parse_from(["cargo-cleanme", "/tmp/projects"])
            .unwrap()
            .invocation();
        assert_eq!(
            canonical,
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(PathBuf::from("/tmp/projects")),
                CleanMode::Execute
            ))
        );
        let alias = Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp/projects"])
            .unwrap()
            .invocation();
        assert_eq!(canonical, alias, "the two spellings must not drift");
    }

    /// M019 test 12: a subcommand still wins over a positional of the same
    /// shape.
    ///
    /// *Fails if:* the positional/subcommand precedence is left to a default
    /// that resolves the other way — the case that would make
    /// `cargo cleanme scan` silently become a bounded cleanup of a directory
    /// called `scan`.
    #[test]
    fn a_subcommand_wins_over_a_positional_of_the_same_shape() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "scan", "--all"])
                .unwrap()
                .invocation(),
            Invocation::Scan(ScanIntent::Full)
        );
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "config", "path"])
                .unwrap()
                .invocation(),
            Invocation::Config(ConfigCommand::Path)
        );
    }

    /// M019 test 13: the deliberate behaviour change is pinned.
    ///
    /// `cargo cleanme .` no longer exits 2 with "unrecognized subcommand": it
    /// resolves to the bounded-root scope. A directory named like a
    /// subcommand is still the subcommand — `./scan` names such a directory.
    #[test]
    fn the_dot_positional_is_a_bounded_cleanup_and_not_a_usage_error() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "."])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(PathBuf::from(".")),
                CleanMode::Execute
            ))
        );
        // A directory whose name collides with a subcommand is the subcommand.
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "./scan"]).is_ok(),
            "an explicitly relative name must stay addressable"
        );
    }

    /// M019 test 15: `clean --known` is not orphaned by the hiding.
    ///
    /// It resolves to the bare invocation — the maintenance scope anchored to
    /// the working directory — so hiding `clean` orphans nothing by it.
    #[test]
    fn the_known_alias_is_not_orphaned_by_the_hiding() {
        let alias = Cli::try_parse_from(["cargo-cleanme", "clean", "--known"])
            .unwrap()
            .invocation();
        let bare = Cli::try_parse_from(["cargo-cleanme"]).unwrap().invocation();
        assert_eq!(alias, bare, "`clean --known` must stay the bare invocation");
        let Invocation::Cleanup(request) = alias else {
            panic!("`clean --known` must resolve to a cleanup");
        };
        assert_eq!(request.scope, CleanupScope::Maintenance);
        assert_eq!(request.selection, CleanupSelection::WorkingDirectory);
    }

    /// M019 test 10: the root help describes the canonical Full cleanup
    /// spelling, and the root man page renders from the same text.
    #[test]
    fn the_root_help_names_the_canonical_full_cleanup_spelling() {
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains("cargo cleanme --full"),
            "the help must name the canonical Full cleanup spelling: {help}"
        );
        assert!(
            help.contains("cargo cleanme ROOT"),
            "the help must name the canonical bounded-root spelling: {help}"
        );
    }

    /// M019: the resolved intent is unchanged by the spelling change. The
    /// root's spellings and the hidden alias's spellings must resolve to the
    /// same `CleanupRequest` — the scope, the selection, the mode and the
    /// overrides.
    ///
    /// This is what makes the change safe to review: the diff in the cleanup
    /// engine is zero.
    #[test]
    fn the_root_and_the_alias_resolve_to_the_same_intent() {
        for (root_args, alias_args) in [
            (vec!["cargo-cleanme"], vec!["cargo-cleanme", "clean"]),
            (
                vec!["cargo-cleanme", "/tmp/p"],
                vec!["cargo-cleanme", "clean", "/tmp/p"],
            ),
            (
                vec!["cargo-cleanme", "--full"],
                vec!["cargo-cleanme", "clean", "--full"],
            ),
            (
                vec!["cargo-cleanme", "--dry-run"],
                vec!["cargo-cleanme", "clean", "--dry-run"],
            ),
        ] {
            let root = Cli::try_parse_from(root_args.clone()).unwrap().invocation();
            let alias = Cli::try_parse_from(alias_args.clone())
                .unwrap()
                .invocation();
            assert_eq!(root, alias, "{root_args:?} and {alias_args:?} must agree");
        }
    }

    /// C045 test 1: the **whole** published spelling set agrees.
    ///
    /// The root and the hidden alias are two spellings of one intent, so every
    /// scope selector, every policy override and every mode must resolve to
    /// the same `Invocation` at either level. The table is the cross product,
    /// not a hand-picked sample: the earlier guard named four pairs, and a
    /// hand-written list is green for the pairs it names and silent about
    /// every pair it does not.
    ///
    /// *Fails if:* one spelling resolves differently from its twin — the shape
    /// of a consolidation that drops or mis-merges one override, or that
    /// loses the alias's copy of a flag the root still has.
    #[test]
    fn the_whole_published_spelling_set_agrees() {
        const SCOPES: &[&[&str]] = &[&[], &["/tmp/c045-fixture"], &["--full"]];
        const OVERRIDES: &[&[&str]] = &[
            &[],
            &["--min-reclaimable-bytes", "42"],
            &["--older-than", "60"],
            &["--include", "/a/*"],
            &["--exclude", "/b/*"],
            &["--profile", "release"],
            &["--package", "demo"],
        ];
        const MODES: &[&[&str]] = &[&[], &["--dry-run"], &["--dryrun"], &["--cargo-preview"]];

        let mut pairs = 0usize;
        for scope in SCOPES {
            for override_ in OVERRIDES {
                for mode in MODES {
                    let mut root = vec!["cargo-cleanme"];
                    root.extend_from_slice(scope);
                    root.extend_from_slice(override_);
                    root.extend_from_slice(mode);
                    let mut alias = vec!["cargo-cleanme", "clean"];
                    alias.extend_from_slice(scope);
                    alias.extend_from_slice(override_);
                    alias.extend_from_slice(mode);
                    let root_invocation = Cli::try_parse_from(root.clone())
                        .unwrap_or_else(|error| panic!("{root:?} must parse: {error}"))
                        .invocation();
                    let alias_invocation = Cli::try_parse_from(alias.clone())
                        .unwrap_or_else(|error| panic!("{alias:?} must parse: {error}"))
                        .invocation();
                    assert_eq!(
                        root_invocation, alias_invocation,
                        "{root:?} and {alias:?} must resolve to the same intent"
                    );
                    pairs += 1;
                }
            }
        }
        assert_eq!(
            pairs,
            SCOPES.len() * OVERRIDES.len() * MODES.len(),
            "the cross product must be enumerated in full"
        );
        assert!(pairs >= 80, "the table must cover the published surface");
    }

    /// C045 test 2: the **converse** compatibility guard.
    ///
    /// The old guard asked "does the alias carry flag F?" — the question that
    /// matters when a flag is being *removed*. Compatibility needs the
    /// opposite: does the alias accept **every** cleanup flag the root
    /// declares? This reads the clap model, so a root cleanup flag the alias
    /// would reject cannot be added without failing here.
    ///
    /// *Fails if:* the root declares a cleanup flag that
    /// `cargo cleanme clean <flag>` would reject — a published spelling that
    /// stops working while the suite stays green.
    #[test]
    fn the_alias_accepts_every_cleanup_flag_the_root_declares() {
        // Build the model first: that is when clap propagates the global
        // arguments to the subcommands and resolves each argument's value
        // count, so the guard sees the surface a user actually gets.
        let mut command = Cli::command();
        command.build();
        let clean = command.find_subcommand("clean").expect("clean subcommand");
        let mut checked = 0usize;
        for arg in command.get_arguments() {
            let id = arg.get_id().clone();
            if id == "help" || id == "version" {
                // clap's own built-ins, not cleanup flags.
                continue;
            }
            assert!(
                clean.get_arguments().any(|alias| *alias.get_id() == id),
                "the root's cleanup flag `{id}` must be accepted by `clean`"
            );
            // And it must *parse*: carrying the id is not enough if the
            // alias's copy rejects the value.
            let mut argv = vec!["cargo-cleanme".to_string(), "clean".to_string()];
            match (arg.get_long(), arg.is_positional()) {
                (Some(long), _) => {
                    argv.push(format!("--{long}"));
                    if let Some(value) = sample_value(arg) {
                        argv.push(value);
                    }
                }
                (None, true) => argv.push("/tmp/c045-fixture".to_string()),
                (None, false) => continue,
            }
            assert!(
                Cli::try_parse_from(argv.clone()).is_ok(),
                "the alias must accept the root's cleanup flag: {argv:?}"
            );
            checked += 1;
        }
        assert!(
            checked >= 10,
            "the guard must inspect the root's cleanup flags, not pass vacuously"
        );
    }

    /// A value that satisfies an argument's parser.
    ///
    /// The guard above parses every flag the root declares through the alias,
    /// so it needs a value each parser will accept: an enum's own first
    /// value, and a benign scalar for everything else.
    fn sample_value(arg: &clap::Arg) -> Option<String> {
        if !arg
            .get_num_args()
            .map(|range| range.takes_values())
            .unwrap_or(false)
        {
            return None;
        }
        if let Some(first) = arg.get_possible_values().first() {
            return Some(first.get_name().to_string());
        }
        Some("1".to_string())
    }

    /// C045 test 3: the alias is still hidden and still accepted.
    ///
    /// *Fails if:* the alias is dropped, or the hidden subcommand reappears in
    /// the root help, or one of the published `clean` spellings stops
    /// parsing.
    #[test]
    fn the_alias_is_still_hidden_and_still_accepted() {
        let help = Cli::command().render_long_help().to_string();
        let listed: Vec<&str> = help
            .split("Commands:")
            .nth(1)
            .and_then(|rest| rest.split("Arguments:").next())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert!(
            !listed.contains(&"clean"),
            "the hidden subcommand must not reappear in the help text: {listed:?}"
        );
        assert!(
            Cli::command()
                .find_subcommand("clean")
                .expect("clean subcommand")
                .is_hide_set(),
            "the clean alias must stay hidden from the root help"
        );
        // The published `clean` spellings still parse and still resolve.
        for (args, expected) in [
            (
                vec!["cargo-cleanme", "clean", "--known"],
                Invocation::Cleanup(maintenance(CleanMode::Execute)),
            ),
            (
                vec!["cargo-cleanme", "clean", "--full"],
                Invocation::Cleanup(cleanup(CleanupScope::Full, CleanMode::Execute)),
            ),
            (
                vec!["cargo-cleanme", "clean", "/tmp/c045-fixture"],
                Invocation::Cleanup(cleanup(
                    CleanupScope::Root(PathBuf::from("/tmp/c045-fixture")),
                    CleanMode::Execute,
                )),
            ),
        ] {
            assert_eq!(
                Cli::try_parse_from(args.clone()).unwrap().invocation(),
                expected,
                "{args:?} must still resolve"
            );
        }
    }

    /// C045 test 4: the alias's hide states are unchanged.
    ///
    /// The consolidation gives the alias the root's flags; the alias must
    /// still *hide* all of them but `--dry-run`, and still advertise its own
    /// `--known`. The root is the only advertised front door.
    ///
    /// *Fails if:* a consolidation flattens the alias and loses or gains a
    /// hide state — a second front door, or a published spelling that stopped
    /// being advertised.
    #[test]
    fn the_alias_hide_states_are_unchanged() {
        let command = Cli::command();
        let clean = command.find_subcommand("clean").expect("clean subcommand");
        for flag in [
            "root",
            "full",
            "min_reclaimable_bytes",
            "older_than",
            "include",
            "exclude",
            "profile",
            "package",
            "cargo_preview",
            "yes_legacy",
            "dryrun_legacy",
        ] {
            let arg = clean
                .get_arguments()
                .find(|arg| arg.get_id() == flag)
                .unwrap_or_else(|| panic!("the clean alias must still carry {flag}"));
            assert!(arg.is_hide_set(), "{flag} must be a hidden alias");
        }
        for flag in ["dry_run", "known"] {
            let arg = clean
                .get_arguments()
                .find(|arg| arg.get_id() == flag)
                .unwrap_or_else(|| panic!("the clean alias must still carry {flag}"));
            assert!(!arg.is_hide_set(), "{flag} must stay advertised");
        }
    }

    /// C045 test 5: the duplicate-override rule survives the consolidation.
    ///
    /// One override at one level: the same value at both levels is a usage
    /// error, because silently preferring one of two values is the
    /// discarded-flag failure mode ADR 008 §3 exists to prevent.
    ///
    /// *Fails if:* one of the two values is silently preferred.
    #[test]
    fn the_duplicate_override_rule_survives_the_consolidation() {
        for args in [
            vec![
                "cargo-cleanme",
                "--older-than",
                "3600",
                "clean",
                "--older-than",
                "60",
            ],
            vec![
                "cargo-cleanme",
                "--min-reclaimable-bytes",
                "10",
                "clean",
                "--min-reclaimable-bytes",
                "20",
            ],
            vec![
                "cargo-cleanme",
                "--include",
                "/a/*",
                "clean",
                "--include",
                "/b/*",
            ],
            vec![
                "cargo-cleanme",
                "--exclude",
                "/a/*",
                "clean",
                "--exclude",
                "/b/*",
            ],
            vec!["cargo-cleanme", "--full", "clean", "--full"],
            vec!["cargo-cleanme", "/tmp/x", "clean", "/tmp/y"],
            vec![
                "cargo-cleanme",
                "--cargo-preview",
                "clean",
                "--cargo-preview",
            ],
            vec!["cargo-cleanme", "--dry-run", "clean", "--dry-run"],
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            assert!(
                matches!(parsed.invocation(), Invocation::UsageError(_)),
                "{args:?} must be a usage error, got {:?}",
                parsed.invocation()
            );
        }
        // The *same level's* mutual exclusions are clap's, and they are the
        // reason the merge layer's cross-level check exists at all: `--profile`
        // and `--package` name different Cargo targets, so silently preferring
        // one would clean a target the operator did not ask for.
        for args in [
            vec!["cargo-cleanme", "--profile", "release", "--package", "demo"],
            vec![
                "cargo-cleanme",
                "clean",
                "--profile",
                "release",
                "--package",
                "demo",
            ],
            vec!["cargo-cleanme", "--dry-run", "--dryrun"],
            vec!["cargo-cleanme", "clean", "--dry-run", "--dryrun"],
        ] {
            assert!(
                Cli::try_parse_from(args.clone()).is_err(),
                "{args:?} must be refused by clap"
            );
        }
    }

    /// C045 test 6: the scope-conflict rules survive the consolidation.
    ///
    /// A Full scope and a bounded ROOT select different scopes, and
    /// `clean --known` selects the maintenance scope: none of them can be
    /// merged with another. The refusal must name the flags that actually
    /// collide.
    ///
    /// *Fails if:* a conflict check is lost in the collapse, or the message
    /// names a flag the operator never typed.
    #[test]
    fn the_scope_conflict_rules_survive_the_consolidation() {
        for (args, expected) in [
            (
                vec!["cargo-cleanme", "--full", "clean", "/tmp/x"],
                "`--full` at the root and ROOT on `clean`",
            ),
            (
                vec!["cargo-cleanme", "/tmp/x", "clean", "--full"],
                "ROOT at the root and `--full` on `clean`",
            ),
            (
                vec!["cargo-cleanme", "--full", "clean", "--known"],
                "`--full` at the root and `--known` on `clean`",
            ),
            (
                vec!["cargo-cleanme", "/tmp/x", "clean", "--known"],
                "`ROOT` at the root and `--known` on `clean`",
            ),
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            assert!(
                message.contains(expected),
                "{args:?}: the message must name the colliding flags: {message}"
            );
        }
    }

    /// C045 test 7: the bare invocation is untouched.
    ///
    /// The working directory joins the maintenance scope on its own (ADR 009
    /// / C040), so a bare run in a target directory already cleans that
    /// directory — the hidden alias is not what makes that work. The
    /// consolidation must not change what the bare invocation selects, and
    /// must not re-derive the bare invocation *through* the alias.
    ///
    /// *Fails if:* the consolidation changes what the bare invocation
    /// selects, or routes it through the alias's spelling.
    #[test]
    fn the_bare_invocation_is_untouched_by_the_consolidation() {
        let bare = Cli::try_parse_from(["cargo-cleanme"]).unwrap().invocation();
        let alias = Cli::try_parse_from(["cargo-cleanme", "clean"])
            .unwrap()
            .invocation();
        assert_eq!(bare, alias, "`clean` must stay the bare invocation");
        // The alias's own scope selector resolves to the same intent.
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "--known"])
                .unwrap()
                .invocation(),
            bare,
            "`clean --known` must stay the bare invocation"
        );
        let Invocation::Cleanup(request) = bare else {
            panic!("the bare invocation must resolve to a cleanup");
        };
        assert_eq!(request.scope, CleanupScope::Maintenance);
        assert_eq!(request.selection, CleanupSelection::WorkingDirectory);
    }

    /// C045 test 8: `--yes` stays `clean`-only and stays unbound.
    ///
    /// Execute is the default, so `--yes` carries no intent of its own
    /// (`src/cli.rs`'s `yes_legacy` is deliberately not bound). The root has
    /// no `--yes` and never had one; the alias accepts it as a hidden
    /// spelling.
    ///
    /// *Fails if:* a consolidation adds `--yes` to the root, or binds it.
    #[test]
    fn yes_stays_clean_only_and_stays_unbound() {
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "--yes"]).is_err(),
            "the root must not accept `--yes`"
        );
        let alias = Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp/c045-fixture", "--yes"])
            .unwrap()
            .invocation();
        assert_eq!(
            alias,
            Invocation::Cleanup(cleanup(
                CleanupScope::Root(PathBuf::from("/tmp/c045-fixture")),
                CleanMode::Execute,
            )),
            "`--yes` must stay unbound and must not change the mode"
        );
    }

    /// M019: a scope selector or a policy override handed to a read-only
    /// subcommand is a usage error, never a silently discarded flag.
    ///
    /// *Fails if:* the flag is accepted and discarded — the C034 "worst
    /// failure mode" shape.
    #[test]
    fn a_cleanup_flag_on_a_read_only_subcommand_is_a_usage_error() {
        for args in [
            vec!["cargo-cleanme", "--full", "scan", "--all"],
            vec!["cargo-cleanme", "--full", "config", "path"],
            vec!["cargo-cleanme", "--full", "update"],
            vec!["cargo-cleanme", "--profile", "release", "scan", "--all"],
            vec!["cargo-cleanme", "--older-than", "60", "scan", "--all"],
            vec!["cargo-cleanme", "--include", "/a/*", "update"],
            vec!["cargo-cleanme", "--cargo-preview", "config", "show"],
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            match parsed.invocation() {
                Invocation::UsageError(message) => {
                    assert!(!message.is_empty(), "{args:?} must say what it refused");
                }
                other => panic!("{args:?} must be a usage error, got {other:?}"),
            }
        }
    }

    /// M019: the same override at both levels is a usage error.
    ///
    /// *Fails if:* one of the two values is silently preferred — the
    /// discarded-flag failure mode ADR 008 §3 exists to prevent.
    #[test]
    fn the_same_override_at_both_levels_is_a_usage_error() {
        for args in [
            vec![
                "cargo-cleanme",
                "--older-than",
                "3600",
                "clean",
                "--older-than",
                "60",
            ],
            vec![
                "cargo-cleanme",
                "--min-reclaimable-bytes",
                "10",
                "clean",
                "--min-reclaimable-bytes",
                "20",
            ],
            vec![
                "cargo-cleanme",
                "--include",
                "/a/*",
                "clean",
                "--include",
                "/b/*",
            ],
            vec!["cargo-cleanme", "--full", "clean", "--full"],
            vec![
                "cargo-cleanme",
                "--cargo-preview",
                "clean",
                "--cargo-preview",
            ],
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            assert!(
                matches!(parsed.invocation(), Invocation::UsageError(_)),
                "{args:?} must be a usage error, got {:?}",
                parsed.invocation()
            );
        }
    }

    /// The two selectors are one logical override, and they are mutually
    /// exclusive across the root/`clean` boundary as well as within a level.
    ///
    /// *Fails if:* one of the two is silently preferred — the discarded-flag
    /// failure mode ADR 008 §3 exists to prevent. Here the consequence is a
    /// *different destructive action* than the one that was asked for: the
    /// operator asked for one package and the tool cleaned a profile.
    #[test]
    fn a_selector_at_the_root_and_the_other_on_clean_is_a_usage_error() {
        for args in [
            vec![
                "cargo-cleanme",
                "--profile",
                "release",
                "clean",
                "--package",
                "demo",
            ],
            vec![
                "cargo-cleanme",
                "--package",
                "demo",
                "clean",
                "--profile",
                "release",
            ],
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            assert!(
                message.contains("--profile") && message.contains("--package"),
                "{args:?}: the message must name both selectors: {message}"
            );
        }
    }

    /// A scope selector at the root and `--known` on `clean` select different
    /// scopes, and the refusal must name the flags that actually collide.
    ///
    /// *Fails if:* the message names a flag the operator never typed — the
    /// original wording said "`--known` was given at the root and on `clean`"
    /// when `--known` was given only on `clean`.
    #[test]
    fn a_scope_conflict_names_the_flags_that_actually_collide() {
        for (args, expected) in [
            (
                vec!["cargo-cleanme", "--full", "clean", "--known"],
                "`--full` at the root and `--known` on `clean`",
            ),
            (
                vec!["cargo-cleanme", "/tmp/x", "clean", "--known"],
                "`ROOT` at the root and `--known` on `clean`",
            ),
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            assert!(
                message.contains(expected),
                "{args:?}: the message must name the colliding flags: {message}"
            );
            assert!(
                !message.contains("was given at the root and on `clean`"),
                "{args:?}: `--known` was not given at the root: {message}"
            );
        }
    }

    /// A Full scope at one level and a bounded ROOT at the other select
    /// different scopes, and cannot be merged.
    ///
    /// *Fails if:* one of them is silently discarded — the discarded-flag
    /// failure mode ADR 008 §3 exists to prevent. Here the discarded selector
    /// is the *bounded* one, so a bounded cleanup becomes a machine-wide one.
    #[test]
    fn a_full_scope_at_one_level_and_a_root_at_the_other_is_a_usage_error() {
        for (args, expected) in [
            (
                vec!["cargo-cleanme", "--full", "clean", "/tmp/x"],
                "`--full` at the root and ROOT on `clean`",
            ),
            (
                vec!["cargo-cleanme", "/tmp/x", "clean", "--full"],
                "ROOT at the root and `--full` on `clean`",
            ),
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            assert!(
                message.contains(expected),
                "{args:?}: the message must name the colliding selectors: {message}"
            );
        }
    }

    /// The same simulation at both levels is a usage error, exactly like the
    /// same Cargo preview at both levels.
    ///
    /// *Fails if:* one of the two is silently preferred — the discarded-flag
    /// failure mode ADR 008 §3 exists to prevent.
    #[test]
    fn a_dry_run_at_both_levels_is_a_usage_error() {
        for (args, expected) in [
            (
                vec!["cargo-cleanme", "--dry-run", "clean", "--dry-run"],
                "--dry-run",
            ),
            (
                vec!["cargo-cleanme", "--dryrun", "clean", "--dryrun"],
                "--dryrun",
            ),
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            // The message names the spelling the operator actually used. It
            // used to always say `--dry-run`, so an operator who typed the
            // hidden `--dryrun` alias was pointed at a flag they did not use
            // and that `--help` does not show.
            assert!(
                message.contains(expected),
                "{args:?}: the message must name the spelling that was used: {message}"
            );
            if expected == "--dryrun" {
                assert!(
                    !message.contains("--dry-run"),
                    "{args:?}: the message must not name a spelling that was not used: {message}"
                );
            }
        }
    }

    /// The two spellings of the same override, given at two levels, are both
    /// named.
    ///
    /// `--dry-run` and `--dryrun` are one logical override. Saying only one of
    /// them leaves the operator unable to tell which of the two they repeated.
    #[test]
    fn a_mixed_dry_run_spelling_names_both_spellings() {
        let parsed = Cli::try_parse_from(vec!["cargo-cleanme", "--dryrun", "clean", "--dry-run"])
            .unwrap_or_else(|error| panic!("must parse: {error}"));
        let Invocation::UsageError(message) = parsed.invocation() else {
            panic!("must be a usage error, got {:?}", parsed.invocation());
        };
        assert!(
            message.contains("--dryrun") && message.contains("--dry-run"),
            "both spellings must be named: {message}"
        );
    }

    /// `--interactive` selects which cleanup units are cleaned, so a read-only
    /// subcommand has nothing for it to select.
    ///
    /// *Fails if:* the flag is silently ignored — the operator asked for an
    /// interactive selection and got a plain scan, which looks exactly like
    /// the flag was honoured.
    #[test]
    fn the_interactive_flag_is_refused_on_the_read_only_subcommands() {
        for args in [
            vec!["cargo-cleanme", "scan", "--known", "--interactive"],
            vec!["cargo-cleanme", "--interactive", "scan", "--known"],
            vec!["cargo-cleanme", "config", "show", "--interactive"],
            vec!["cargo-cleanme", "update", "--interactive"],
        ] {
            let parsed = Cli::try_parse_from(args.clone())
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"));
            let Invocation::UsageError(message) = parsed.invocation() else {
                panic!(
                    "{args:?} must be a usage error, got {:?}",
                    parsed.invocation()
                );
            };
            assert!(
                message.contains("--interactive"),
                "{args:?}: the message must name the refused flag: {message}"
            );
        }
    }

    /// A root-level flag that the `clean` alias also carries is merged,
    /// not duplicated. The root wins; the alias's value is used when the root
    /// did not supply one.
    #[test]
    fn a_root_level_override_wins_over_the_alias() {
        let parsed =
            Cli::try_parse_from(["cargo-cleanme", "--older-than", "3600", "clean", "/tmp/p"])
                .unwrap();
        let Invocation::Cleanup(request) = parsed.invocation() else {
            panic!("must resolve to a cleanup");
        };
        assert_eq!(request.overrides.older_than, Some(3600));
        assert_eq!(request.scope, CleanupScope::Root(PathBuf::from("/tmp/p")));
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
        // M014: Full is no longer reachable without an explicit selector.
        // `scan` alone is a usage error, so the machine-wide reconciliation --
        // the only operation that may expire learned roots -- is never one
        // keystroke from the front door.
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "scan"]).is_err(),
            "bare `scan` must require ROOT, --known, or --all"
        );
        // Both spellings of the Full selector reach the same intent.
        for spelling in [
            ["cargo-cleanme", "scan", "--all"],
            ["cargo-cleanme", "scan", "-a"],
            ["cargo-cleanme", "scan", "--full"],
        ] {
            assert_eq!(
                Cli::try_parse_from(spelling).unwrap().invocation(),
                Invocation::Scan(ScanIntent::Full),
                "{spelling:?} must resolve to Full"
            );
        }
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
    }

    #[test]
    fn the_interactive_flag_is_declared_advertised_and_not_implied() {
        use clap::CommandFactory;
        // M014 removed a previously-declared `-i` precisely because a flag that
        // parses and silently does nothing is worse than no flag. This is the
        // regression against reintroducing that shape.
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains("-i, --interactive"),
            "the flag must be advertised: {help}"
        );
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "-i"])
                .unwrap()
                .interactive
        );
        assert!(
            Cli::try_parse_from(["cargo-cleanme", "--interactive"])
                .unwrap()
                .interactive,
            "--interactive must be the same flag"
        );
        assert!(
            !Cli::try_parse_from(["cargo-cleanme"]).unwrap().interactive,
            "-i must not be implied by an ordinary invocation"
        );
        // It is global, so the subcommand spelling must carry it too.
        match Cli::try_parse_from(["cargo-cleanme", "clean", "--known", "-i"])
            .unwrap()
            .invocation()
        {
            Invocation::Cleanup(request) => assert!(
                request.interactive,
                "the flag must reach the cleanup intent, not just the parser"
            ),
            other => panic!("expected a cleanup intent, got {other:?}"),
        }
        // The flag does not manufacture a selection: it declares that a
        // session will produce one. The declared selection still travels
        // alongside it, so the non-interactive path is unchanged.
        match Cli::try_parse_from(["cargo-cleanme", "-i"])
            .unwrap()
            .invocation()
        {
            Invocation::Cleanup(request) => {
                assert_eq!(
                    request.selection,
                    CleanupSelection::WorkingDirectory,
                    "-i must not pre-empt the anchor; the session replaces it"
                );
            }
            other => panic!("expected a cleanup intent, got {other:?}"),
        }
    }

    #[test]
    fn interactive_without_a_terminal_still_means_human_format_only() {
        // The machine formats are refused in `main`, not here: this layer does
        // no IO and must stay pure. What it *can* prove is that the flag
        // coexists with every output format, so the refusal is a runtime
        // guard on a real conflict rather than an accident of parsing.
        for (flag, expected) in [
            ("human", OutputFormat::Human),
            ("json", OutputFormat::Json),
            ("log", OutputFormat::Log),
        ] {
            let cli = Cli::try_parse_from(["cargo-cleanme", "-i", "--format", flag]).unwrap();
            assert_eq!(cli.format, expected);
            assert!(cli.interactive);
        }
    }

    #[test]
    fn maintenance_spellings_anchor_selection_to_the_working_directory() {
        // M014 / ADR 006: the anchor belongs to the two spellings whose Routine
        // scope spans the machine. Both must agree, or `cargo cleanme` and
        // `cargo cleanme clean --known` would disagree about what "everything"
        // means — which is the M012A silent-no-op failure in a new spelling.
        for args in [
            vec!["cargo-cleanme"],
            vec!["cargo-cleanme", "clean"],
            vec!["cargo-cleanme", "clean", "--known"],
        ] {
            let Invocation::Cleanup(request) =
                Cli::try_parse_from(args.clone()).unwrap().invocation()
            else {
                panic!("{args:?} must resolve to a cleanup request");
            };
            assert_eq!(
                request.selection,
                CleanupSelection::WorkingDirectory,
                "{args:?} must anchor its selection to the working directory"
            );
        }
        // `cargo cleanme ROOT` and `cargo cleanme --full` are already bounded by
        // their own root selection; anchoring them too would silently narrow an
        // explicit scope.
        for args in [
            vec!["cargo-cleanme", "clean", "/x"],
            vec!["cargo-cleanme", "clean", "--full"],
        ] {
            let Invocation::Cleanup(request) =
                Cli::try_parse_from(args.clone()).unwrap().invocation()
            else {
                panic!("{args:?} must resolve to a cleanup request");
            };
            assert_eq!(
                request.selection,
                CleanupSelection::All,
                "{args:?} must stay unfiltered"
            );
        }
    }

    #[test]
    fn unresolved_working_directory_anchor_selects_nothing() {
        // Fail-closed: `cli.rs` emits the anchor unresolved because it must not
        // do IO. A caller that forgets to resolve it must clean nothing, not
        // everything. Returning `true` here would make that bug delete data.
        assert!(
            !CleanupSelection::WorkingDirectory.admits(Path::new("/any/workspace")),
            "an unresolved anchor must never admit a candidate"
        );
    }

    #[test]
    fn scan_scope_conflicts_fail_at_invocation_time() {
        for args in [
            vec!["cargo-cleanme", "scan", "/tmp", "--known"],
            vec!["cargo-cleanme", "scan", "/tmp", "--all"],
            vec!["cargo-cleanme", "scan", "/tmp", "--full"],
            vec!["cargo-cleanme", "scan", "--known", "--full"],
            vec!["cargo-cleanme", "scan", "--known", "--all"],
            vec!["cargo-cleanme", "scan", "--all", "--full"],
        ] {
            assert!(
                Cli::try_parse_from(args.clone()).is_err(),
                "{args:?} must be rejected before any traversal"
            );
        }
    }

    #[test]
    fn hidden_full_alias_is_not_advertised_in_scan_help() {
        // The *scan* subcommand's `--full` is the hidden alias for `--all`, and
        // it must not be advertised there. The root's own `--full` is the
        // canonical Full cleanup flag (M019) and is advertised at the root.
        //
        // The alias is an alias of the `--all` argument rather than an
        // argument of its own: an `ArgGroup` renders every one of its members
        // in the usage line, so a separate hidden argument would have
        // advertised the alias here.
        let mut command = Cli::command();
        let scan = command
            .find_subcommand_mut("scan")
            .expect("scan subcommand");
        let usage = scan.render_usage().to_string();
        assert!(
            !usage.contains("--full"),
            "the scan's --full alias must not be advertised in the usage line: {usage}"
        );
        let help = scan.render_long_help().to_string();
        assert!(
            !help.contains("--full"),
            "the scan's --full alias must not appear in the help: {help}"
        );
        // The alias still selects the full scope.
        let parsed = Cli::try_parse_from(["cargo-cleanme", "scan", "--full"])
            .expect("the historical spelling must still parse");
        let Some(Command::Scan { all, .. }) = parsed.command else {
            panic!("scan --full must be a scan");
        };
        assert!(all, "the alias must select the full scope");
        let all = scan
            .get_arguments()
            .find(|arg| arg.get_id() == "all")
            .expect("the scan --all flag");
        assert!(!all.is_hide_set(), "--all must stay advertised");
        let known = scan
            .get_arguments()
            .find(|arg| arg.get_id() == "known")
            .expect("the scan --known flag");
        assert!(!known.is_hide_set(), "--known must stay advertised");
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

    /// M018 asserted that this milestone grew no root-level flag. M019
    /// superseded it: the root-level `--full` is now the canonical Full
    /// cleanup spelling (ADR 008 §4b), and `clean --full` remains a hidden
    /// alias.
    ///
    /// The assertion became "the alias still resolves to the Full scope on its
    /// own" — the canonical spelling and the alias agreeing is pinned
    /// separately by `the_root_full_flag_resolves_to_the_full_scope`, so a
    /// change that made only one of them move would be caught.
    #[test]
    fn the_full_cleanup_alias_still_resolves_after_the_root_moved() {
        assert_eq!(
            Cli::try_parse_from(["cargo-cleanme", "clean", "--full"])
                .unwrap()
                .invocation(),
            Invocation::Cleanup(cleanup(CleanupScope::Full, CleanMode::Execute))
        );
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
        // `--dry-run` is unchanged: it was already global.
        assert!(clean_help.contains("--dry-run"), "{clean_help}");
        // M019: the alias's own flags are hidden aliases too — the root is the
        // only advertised front door. The declaration is checked, not the
        // rendered prose: the alias's own long-about *names* `clean --full`,
        // and prose is not an advertisement of the flag.
        let command = Cli::command();
        let clean = command.find_subcommand("clean").expect("clean subcommand");
        for flag in [
            "root",
            "full",
            "min_reclaimable_bytes",
            "older_than",
            "include",
            "exclude",
            "profile",
            "package",
            "cargo_preview",
            "yes_legacy",
            "dryrun_legacy",
        ] {
            let arg = clean
                .get_arguments()
                .find(|arg| arg.get_id() == flag)
                .unwrap_or_else(|| panic!("the clean alias must still carry {flag}"));
            assert!(arg.is_hide_set(), "{flag} must be a hidden alias");
        }
        for flag in ["dry_run", "known"] {
            let arg = clean
                .get_arguments()
                .find(|arg| arg.get_id() == flag)
                .unwrap_or_else(|| panic!("the clean alias must still carry {flag}"));
            assert!(!arg.is_hide_set(), "{flag} must stay advertised");
        }
    }

    #[test]
    fn partitioned_flag_is_gone_scoped_proof_is_the_only_behavior() {
        // ADR 007: the flag was never published (0.2.9 unreleased), so
        // removal is total — no alias, no deprecation path. Unknown flags
        // are a clap usage error, and scoped proof needs no opt-in.
        assert!(Cli::try_parse_from(["cargo-cleanme", "--partitioned"]).is_err());
        assert!(Cli::try_parse_from(["cargo-cleanme", "clean", "/x", "--partitioned"]).is_err());
        assert!(matches!(
            Cli::try_parse_from(["cargo-cleanme"]).unwrap().invocation(),
            Invocation::Cleanup(_)
        ));
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
            vec!["cargo-cleanme", "scan", "--all", "--format", "log"],
            vec!["cargo-cleanme", "--format", "log", "scan", "--all"],
            vec!["cargo-cleanme", "clean", "/x", "--format", "log"],
            vec![
                "cargo-cleanme",
                "cleanme",
                "--format",
                "log",
                "scan",
                "--all",
            ],
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
            matches!(parsed.command, Some(Command::Clean { flags, .. }) if flags.min_reclaimable_bytes == Some(42) && flags.older_than == Some(60) && flags.include.len() == 2 && flags.exclude == ["**/tmp"])
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
                selection: CleanupSelection::WorkingDirectory,
                interactive: false,
            }),
            "advanced policy overrides travel with the intent, not beside it"
        );
        let selected =
            Cli::try_parse_from(["cargo-cleanme", "clean", "/tmp", "--profile", "custom"]).unwrap();
        assert!(
            matches!(selected.command, Some(Command::Clean { flags, .. }) if flags.profile.as_deref() == Some("custom"))
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
        assert_eq!(absolutize_root(&absolute).unwrap(), absolute);
    }

    #[test]
    fn relative_root_becomes_absolute() {
        let cwd = std::env::current_dir().unwrap();
        let joined = absolutize_root(Path::new("./project")).unwrap();
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
