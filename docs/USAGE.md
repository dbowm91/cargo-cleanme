# Using cargo-cleanme

This guide explains cleanup scope, scan and cleanup modes, safety, configuration, and output. Start with the [README](../README.md) for installation and a quick start.

## First run

The default command performs cleanup. Use a bounded root and preview before executing:

```sh
cargo cleanme scan ~/projects/my-app
cargo cleanme ~/projects/my-app --dry-run
cargo cleanme ~/projects/my-app
```

The scan inventories the named directory and updates learned-root state, but does not clean build output. The preview runs the cleanup decision and proof checks without invoking `cargo clean`. The final command performs eligible cleanup.

## Commands and scopes

| Command | Scope and effect |
|---|---|
| `cargo cleanme` | Routine maintenance. The maintenance scope includes default project locations, active learned roots, and the working directory when needed. Candidate selection is anchored to the working directory. |
| `cargo cleanme ROOT` | Clean the explicitly named bounded root. A command-line root takes precedence over configured `scan.root`. |
| `cargo cleanme --full` | Recheck learned roots and clean the proven candidates. It does not perform a new machine-wide scan. |
| `cargo cleanme scan` | Read-only machine reconciliation. Learns project roots and expires stale learned roots. |
| `cargo cleanme scan ROOT` | Read-only inventory of one directory. Updates learned roots for that directory only. |

A configured `scan.root` replaces the routine maintenance scope. A bare cleanup selects workspaces whose roots equal, contain, or are contained by the working directory. Use `ROOT` to clean a different bounded area. If a directory name collides with a command, prefix it with `./`, such as `cargo cleanme ./scan`.

A machine scan may take several minutes. If it cannot establish complete coverage, it reports a partial scan and exits `1`; it does not replace the complete learned-root list with an incomplete result. Scans do not clean or modify build output, but they can update local learned-root state.

`scan.ignore` and `scan.unignore` apply to routine maintenance and machine-wide scans. Explicit roots, including configured `scan.root`, bypass these filters.

## Cleanup modes and flags

| Flag | Effect |
|---|---|
| `--dry-run` | Simulate cleanup through the full decision and proof path without invoking `cargo clean`. For `update`, check the release without downloading or replacing the binary. |
| `--cargo-preview` | After the same safety checks, invoke Cargo's own `clean --dry-run --verbose`. Cargo runs, but does not remove output. |
| `--interactive`, `-i` | Choose among proven cleanup units in a terminal. Requires a TTY and cannot be combined with JSON or log output. |
| `--min-reclaimable-bytes N` | Require at least this many reclaimable bytes per workspace. |
| `--older-than SECONDS` | Add a minimum quiet period on top of the recent-activity guard. |
| `--include GLOB`, `--exclude GLOB` | Filter canonical workspace roots. Repeatable. |
| `--profile NAME` | Ask Cargo to clean one profile when the installed Cargo version is qualified. |
| `--package SPEC` | Ask Cargo to clean one validated workspace package when the installed Cargo version is qualified. |
| `--config PATH` | Use a specific config file. |
| `--format human\|json\|log` | Select human-readable, JSON, or bounded log output. |
| `--verbose` | Add detail to human-readable reports. Does not change JSON or log output. |
| `--no-progress` | Disable transient progress output. |
| `--stats` | Print counters and timings to stderr. |

The root-level `--dry-run` flag is ignored by `scan` and all `config` subcommands. Scans can still update learned-root state, and `config edit` still opens the editor and can change the file. Use `--dry-run` for cleanup simulation; it does not protect `config edit`. The hidden `clean` command remains a compatibility alias; the root-level forms above are canonical.

### Cargo selectors

Selectors are enabled only for exact Cargo versions with hosted qualification evidence. Other or unknown versions fail closed before cleanup.

- `--profile`: Cargo 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0.
- `--package`: Cargo 1.98.1 and 1.99.0.

The selectors cannot be combined. Either selector is also rejected with a nonzero `--min-reclaimable-bytes`, because selector-specific byte estimates are unavailable.

## Cleanup safety

Cargo-cleanme never removes directories directly. It identifies build output, proves ownership, and delegates cleanup to Cargo. It rechecks ownership immediately before each cleanup command.

A shared output directory is eligible only when every known workspace using it is idle. The default activity window is 300 seconds. `--older-than` adds a stricter quiet-age requirement; it does not replace that guard. Source trees, symlinks, and paths whose identity cannot be proven are not cleaned.

An unresolved manifest inside the selected proof boundary can block cleanup or hold affected units. An unresolved manifest outside the boundary is reported but does not block the selected cleanup. A bounded cleanup does not claim the rest of the machine is clear.

`cleanup.allowed_output_roots` authorizes already-proven private output outside the requested root. It cannot prove ownership or make uncertain output eligible.

## Configuration

Configuration is created on first use in the platform application config directory. It is not loaded from the current directory unless you pass `--config PATH`. Use `cargo cleanme config path` to locate it, `config show` to view effective settings, and `config edit` to edit and validate it. The file is not overwritten automatically.

| Setting | Purpose |
|---|---|
| `scan.recency_seconds` | Recent-activity protection window. Default: `300`; must be greater than zero. |
| `scan.root` | Exclusive root for routine maintenance. A command-line `ROOT` takes precedence. |
| `scan.ignore` | Absolute glob patterns applied to routine maintenance and machine-wide discovery. |
| `scan.unignore` | Literal absolute paths re-included beneath ignored trees. Not globs. |
| `scan.learned_root_retention_days` | Expiration age for learned roots. Default: `30`; `0` disables age-based expiration; maximum: `3650`. |
| `cleanup.allowed_output_roots` | Authorize already-proven private output outside the requested root. Does not grant ownership. |
| `cleanup.policy.min_reclaimable_bytes` | Minimum measured size. Default: `0`. |
| `cleanup.policy.min_inactive_seconds` | Optional additional quiet period. |
| `cleanup.policy.include` / `exclude` | Patterns matched against canonical workspace roots, not build-output directories. |

`scan.unignore` treats each entry as a literal path. A `*` is not expanded. Invalid keys, paths, or patterns produce an error instead of being silently ignored.

## Output and exit codes

| Format | Use |
|---|---|
| `human` (default) | Readable results. Add `--verbose` for workspaces, counters, and diagnostic details. |
| `json` | Versioned machine-readable reports. Scan and cleanup use schema version 1. |
| `log` | One bounded ASCII scheduler summary line. Not a machine contract. |

The log line is limited to 384 bytes, excludes paths, and may omit optional fields. Use JSON for complete per-unit data. `update --format json` has a separate schema-version-1 shape. The `config` commands print a path or TOML, so `--format json` is rejected for them.

| Exit code | Meaning |
|---|---|
| `0` | Completed, including when nothing was eligible or safe skips occurred. |
| `1` | An incomplete scan or cleanup report, blocked or partial cleanup, a failed cleanup unit, or an editor that exits unsuccessfully. |
| `2` | Invalid invocation, configuration, or a fatal error before a report, including update failures. |

A Full cleanup can clean proven roots and still exit `1` if another learned root could not be proven. Use the process exit code, not only the bounded log line. If the editor launches but exits unsuccessfully, `config edit` exits `1` and reports the editor's status in its message; it does not propagate that status. Failure to resolve or launch the editor is fatal and exits `2`.

## See also

- [README](../README.md) for installation and quick start
- [Unattended operation](AUTOMATION.md) for scheduling and output guidance
- [Installing](INSTALLING.md) for platform support
- [Troubleshooting](TROUBLESHOOTING.md) for common problems
- [Self-update](UPDATE.md) for update behavior
