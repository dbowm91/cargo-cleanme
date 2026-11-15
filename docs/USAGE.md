# Using cargo-cleanme

This guide covers the commands, cleanup scopes, safety checks, configuration,
and output formats. For a first run, start with the [README](../README.md).

## Commands

| Command | What it does |
|---|---|
| `cargo cleanme` | Clean the maintenance scope (default mode: execute) |
| `cargo cleanme --dry-run` | Simulate maintenance without invoking `cargo clean` |
| `cargo cleanme ROOT` | Clean one bounded root |
| `cargo cleanme --full` | Clean learned roots that can be proven |
| `cargo cleanme scan` | Inventory the machine and reconcile learned roots; does not clean build output |
| `cargo cleanme scan ROOT` | Inventory one directory and update its learned roots; does not clean build output |
| `cargo cleanme --interactive` | Choose cleanup units from a terminal list |
| `cargo cleanme config path` | Print the config file path |
| `cargo cleanme config show` | Print the effective config |
| `cargo cleanme config edit` | Open the config in an editor |
| `cargo cleanme update --dry-run` | Check for an update without replacing the binary |
| `cargo cleanme update` | Update a supported self-managed installation |

The hidden `clean` subcommand remains as a compatibility alias. The canonical
cleanup spellings are the bare command, `cargo cleanme ROOT`, and
`cargo cleanme --full`.

A directory named `scan`, `config`, `update`, or `clean` is parsed as a
subcommand. Use `./scan` to name a directory called `scan`.

## Flags

| Flag | Applies to | Effect |
|---|---|---|
| `--dry-run` | cleanup, update | Simulate cleanup without invoking `cargo clean`, or check an update without replacing the binary |
| `--cargo-preview` | cleanup | After the safety checks, invoke Cargo's own `clean --dry-run --verbose` |
| `--interactive`, `-i` | cleanup | Choose units in a terminal; requires a TTY |
| `--format human\|json\|log` | reports | Select human-readable, JSON, or bounded log output |
| `--verbose` | reports | Show detailed human output; it does not change JSON or log output |
| `--no-progress` | scan, cleanup | Disable the transient progress display |
| `--stats` | scan, cleanup | Print counters and timings to stderr |
| `--config PATH` | all commands | Use a specific config file |
| `--min-reclaimable-bytes N` | cleanup | Require at least this many reclaimable bytes per workspace |
| `--older-than SECONDS` | cleanup | Add a minimum quiet period on top of the activity guard |
| `--include GLOB` | cleanup | Include matching workspace roots; repeatable |
| `--exclude GLOB` | cleanup | Exclude matching workspace roots; repeatable |
| `--profile NAME` | cleanup | Ask Cargo to clean one profile when supported by that Cargo version |
| `--package SPEC` | cleanup | Ask Cargo to clean one validated workspace package when supported |

`--dry-run` and `--cargo-preview` are different. Simulation performs the
application's decision and proof checks without invoking `cargo clean`.
`--cargo-preview` invokes Cargo's own dry-run command. Use simulation when you
want no cleanup command to run.

The hidden `--dryrun` spelling remains an alias for `--dry-run`. The hidden
`--yes` spelling on `clean` is an alias for the default execute mode.

Cleanup flags are rejected when passed to `scan`, `config`, or `update`, rather
than silently ignored. `--dry-run` is ignored by read-only commands because it
does not change their behavior.

## Cleanup scopes

| Scope | Command | What is selected |
|---|---|---|
| Maintenance | `cargo cleanme` | Bounded seed roots under your home, active learned roots, and the working directory when needed; selection is anchored to the working directory |
| Explicit | `cargo cleanme ROOT` | Workspaces found under the named root |
| Full cleanup | `cargo cleanme --full` | Learned roots that can be proven; this does not perform a new machine-wide walk |

A configured `scan.root` is an exclusive override for the maintenance scope.
The working directory is added only when the existing scope does not already
reach it. The bare command then selects workspaces whose roots equal, contain,
or are contained by the working directory. Use `ROOT` to clean a different
bounded area.

`scan.ignore` and `scan.unignore` apply to Routine maintenance when no
`scan.root` is configured, and to machine-wide scans. Explicit roots, including
a configured `scan.root`, bypass those filters.

### Scanning

```sh
cargo cleanme scan                 # inventory without cleaning build output
cargo cleanme scan ~/projects      # bounded inventory without cleanup
```

A machine scan reconciles the learned project roots and can take several
minutes. If it cannot read every relevant area, it reports partial coverage and
exits `1`; it does not publish a complete new learned-root list. A bounded
`scan ROOT` updates the learned list for projects found under that directory.

A scan never cleans or changes build output. A machine scan reconciles local
learned-root state, and `scan ROOT` updates that state for the named directory.
Scans inventory output groups and estimate their size; they do not reclaim
space. A project touched within the activity window is protected and may not
appear among the inactive groups.

### Cleanup modes

```sh
cargo cleanme ROOT --dry-run       # simulate, do not invoke cargo clean
cargo cleanme ROOT --cargo-preview # ask Cargo for its own preview
cargo cleanme --full --dry-run     # simulate Full cleanup
```

The default mode executes eligible cleanup through `cargo clean`. A Full
cleanup re-checks learned roots and can clean the roots it proves. If some
learned roots cannot be proven, it reports the partial result and exits `1`.
When there are no learned roots, run `cargo cleanme scan` first.

The interactive selector only removes candidates from the already-proven set;
it cannot add workspaces or bypass safety checks. It requires an interactive
terminal and cannot be combined with JSON or log output. An empty selection
prints `Nothing to clean.` and exits `0`.

## Cleanup safety

Cargo-cleanme never deletes directories directly. Every cleanup is performed
through Cargo, and ownership is checked again immediately before each cleanup
command.

A directory is eligible only when it is identified as build output and every
workspace using it is idle. A shared directory stays protected while any owner
is active. Source trees, symlinks, and paths whose identity is uncertain are
never cleaned. The default activity window is 300 seconds; `--older-than` adds
a stricter quiet-age requirement without replacing that guard.

An active unresolved manifest inside the selected proof boundary can block
cleanup if its output cannot be identified. If Cargo's output location can be
determined, only overlapping cleanup units are held. Manifests outside the
selected boundary are reported but do not block it. A boundary cleanup is not
a claim that the whole machine is clear.

`cleanup.allowed_output_roots` authorizes a location for already-proven private
output outside the requested root. It never proves ownership or makes uncertain
output eligible.

## Configuration

The config is created on first use in the platform's application config
directory. It is not read from the current working directory unless you pass
`--config PATH`. It is not overwritten automatically. `config edit` validates
the file after the editor closes.

```toml
[scan]
recency_seconds = 300
# root = "/path/to/projects"  # optional exclusive maintenance scope
ignore = []                   # absolute glob patterns
unignore = []                 # exact absolute paths, not globs
learned_root_retention_days = 30

[cleanup]
allowed_output_roots = []

[cleanup.policy]
min_reclaimable_bytes = 0
# min_inactive_seconds = 2592000
include = []
exclude = []
```

| Setting | Purpose |
|---|---|
| `scan.recency_seconds` | Protect output associated with recently active projects. Default: `300`. Must be greater than zero. |
| `scan.root` | Exclusive root for maintenance. A command-line `ROOT` takes precedence. |
| `scan.ignore` | Absolute glob patterns pruned from maintenance and machine-wide discovery. |
| `scan.unignore` | Literal absolute directories re-included under ignored trees. Not globs. |
| `scan.learned_root_retention_days` | Retain learned roots for this many days. Default: `30`; `0` disables age-based expiration. |
| `cleanup.allowed_output_roots` | Authorize already-proven private output outside the requested cleanup root. Does not grant ownership. |
| `cleanup.policy.min_reclaimable_bytes` | Minimum measured size. Default: `0`. |
| `cleanup.policy.min_inactive_seconds` | Optional additional quiet period. |
| `cleanup.policy.include` / `exclude` | Patterns matched against canonical workspace roots, not output directories. |

Path settings and `scan.ignore` entries must be absolute. `scan.unignore` is a
literal path, so a `*` in it is not expanded. Explicit roots bypass the ignore
filters. Invalid config keys, paths, or patterns produce an error instead of
being silently ignored.

## Output formats

| Format | Use |
|---|---|
| `human` (default) | Human-readable results. Use `--verbose` for workspace, counter, and diagnostic detail. |
| `json` | One newline-terminated schema-version-1 document for software. |
| `log` | One bounded ASCII summary line for scheduler history. It is not a machine contract. |

Scripts should parse JSON, not log output. Log output is limited to 384 bytes
(excluding the newline), contains no paths, and may omit optional fields. The
JSON schema is versioned, but the project is pre-1.0 and its CLI and JSON
contracts may change in a 0.x release.

`config` has no JSON document: `config path` prints a path and `config show`
prints TOML, so `--format json` is rejected. `--format log` is accepted and
reports the action. `update --format json` has its own schema-version-1 shape,
not the scan and cleanup envelope.

Progress and `--stats` use stderr. Human-readable diagnostic details may also
appear on stderr; JSON reports include diagnostics in the document. `--verbose`
changes human output only. See [Unattended operation](AUTOMATION.md) for
scheduler guidance.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Completed, including when no units were eligible or safe skips occurred |
| `1` | Incomplete scan, blocked or partial cleanup, or an operation failure |
| `2` | Invalid invocation, configuration, or another fatal error before a report |

A Full cleanup can clean proven roots and still exit `1` if another learned root
could not be proven. Check the report and stderr for details. `config edit`
returns the editor's exit status.

## Cleanup selectors

`--profile NAME` and `--package SPEC` are supported only for qualified Cargo
versions. Profile selection is qualified for Cargo 1.89.0, 1.90.0, 1.91.1,
1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0. Package selection is
qualified only for 1.98.1 and 1.99.0. Other or unknown versions fail closed
before Cargo cleanup.

Selectors cannot be combined with one another. Their byte estimates are not
known, so using either selector with a nonzero `--min-reclaimable-bytes` is
rejected before cleanup.

## See also

- [README](../README.md) for the quick start and command summary
- [Unattended operation](AUTOMATION.md) for scheduler guidance
- [Troubleshooting](TROUBLESHOOTING.md) for common problems
- [Self-update](UPDATE.md) for update behavior
