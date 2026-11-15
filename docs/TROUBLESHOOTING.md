# Troubleshooting cargo-cleanme

Start by checking the version and the command you ran:

```sh
cargo cleanme --version
```

For a safe preview of a bounded cleanup, use `cargo cleanme ROOT --dry-run`.
For a read-only inventory, use `cargo cleanme scan ROOT`.

## Known issues in older releases

Use the latest release. These historical versions had important defects:

| Version | Issue | Fixed |
|---|---|---|
| `0.2.0` | Could run a real cleanup for a misplaced `--dry-run`, and could delete another project's source tree. This release is yanked. | `0.2.1` |
| `0.2.10` and earlier | A nested output root could be treated as private even when it was inside another package's source tree. | `0.2.11` |
| `0.2.12` | The crate was published without a matching GitHub release, so self-update to that version cannot find its release assets. | `0.2.13` |

If you ran `0.2.0` or `0.2.10` and earlier, review your version-control status and backups for affected source files. See the [changelog](../CHANGELOG.md) for release-specific details.

## Cleanup finds nothing

Bare `cargo cleanme` performs cleanup. It uses the maintenance scope and
selects workspaces related to the current working directory. A configured
`scan.root` replaces the normal maintenance scope. A project outside the
selected roots, a project with no build output, or output touched within the
activity window may produce nothing to clean.

Try a read-only scan or name the root explicitly:

```sh
cargo cleanme scan ~/projects/my-app
cargo cleanme ~/projects/my-app --dry-run
```

The default activity window is 300 seconds. Recently active projects are
protected; wait for the window to pass rather than changing file timestamps.

## Cleanup is blocked or exits 1

Cleanup fails closed when the selected scope cannot be proven complete. Check
the report and stderr for unresolved manifests or discovery errors inside the
selected boundary. An unresolved manifest outside that boundary is reported
but does not block it.

For a manifest Cargo cannot resolve, reproduce the lookup with:

```sh
cargo locate-project --workspace --manifest-path /path/to/Cargo.toml
```

Fix the manifest or workspace configuration, then retry the same preview.
An active unresolved manifest with a known output location may hold only the
cleanup units whose outputs intersect it. If the output location cannot be
identified, cleanup of the affected boundary is blocked.

A Full cleanup (`cargo cleanme --full`) uses learned roots and verifies each
root. If some roots cannot be proven, it can clean the proven roots and exits
`1` to report the partial result. If there are no learned roots, it prints a
message recommending `cargo cleanme scan` and exits `0`; run the scan before
using Full cleanup.

Exit code `1` can also mean a cleanup unit failed or was held by an unresolved
output claim. Read the report before retrying.

## A filter seems to have no effect

`scan.ignore` and `scan.unignore` apply to Routine maintenance when no
`scan.root` is configured, and to machine-wide scans. A configured `scan.root`
or a command-line root is an explicit scope, so the filters are bypassed:

```sh
cargo cleanme scan . --config ./filters.toml
```

The command above uses an explicit root, so configured ignore filters do not
apply. To test those filters, run `cargo cleanme scan` or a bare maintenance
command with no configured `scan.root`. `scan.ignore` patterns and
`scan.unignore` paths must be absolute.
`scan.unignore` is a literal path, not a glob.

Cleanup `include` and `exclude` patterns match canonical workspace roots, not
build output directories. For example, `**/archived` matches a workspace root
named `archived`; `**/archived/**` requires another path segment after it.

## `--dry-run` and `--cargo-preview` differ

`--dry-run` runs the application's cleanup checks without invoking `cargo
clean`. `--cargo-preview` runs Cargo's own `clean --dry-run --verbose` after the
same safety checks. Use `--dry-run` when you want the application simulation
with no cleanup command.

## A scheduled run cleans nothing or fails

Check these in order:

1. **Working directory:** a bare scheduled command is anchored to the scheduler's
   working directory. Use an explicit `ROOT` for a fixed scope.
2. **Config:** a configured `scan.root` replaces the normal maintenance scope.
3. **Account and permissions:** run as the user who can read the intended project
   roots. Running as root may use a different home and config.
4. **Activity:** projects touched within `scan.recency_seconds` are protected.
5. **Exit code:** use the process status. A partial Full cleanup may have a log
   line with `status=ok` while exiting `1`.

See [Unattended operation](AUTOMATION.md) for scheduling and output guidance.

## The command is not found

For Cargo installations, make sure Cargo's binary directory is on PATH. For
prebuilt installations, add the destination reported by the installer. The
POSIX installer prints PATH guidance and can update supported zsh or bash
startup files. The PowerShell installer does not modify PATH.

See [Installing](INSTALLING.md#path) for details.

## The installer fails

- **No prebuilt binary for this host:** use `cargo install cargo-cleanme --locked`.
- **Missing or mismatched checksum:** stop and retry later. The installer
  refuses to place an unverified binary.
- **Network or release-host error:** retry later or install from crates.io. A
  transport failure is not treated as a missing asset.
- **Destination already exists:** pass `--force` (PowerShell: `-Force`) only if
  you intend to replace it.

## Self-update refuses

A Cargo-managed installation must be updated through Cargo:

```sh
cargo install cargo-cleanme --locked --force
```

`cargo cleanme update` only replaces a supported self-managed installation. It
also refuses to downgrade, to replace a build newer than the published release,
or to update a host without a published binary. See [Self-update](UPDATE.md).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Completed, including when there was nothing to clean or safe skips occurred |
| `1` | Incomplete scan, blocked or partial cleanup, or an operation failure |
| `2` | Invalid command, configuration, or another fatal error before a report |

When reporting a problem, include the version, operating system, exact command,
exit code, and human-readable output. Redact private paths before sharing logs.
