# Troubleshooting cargo-cleanme

Start with the version and exact command:

```sh
cargo cleanme --version
```

For a bounded preview, run `cargo cleanme ROOT --dry-run`. For an inventory without cleanup, run `cargo cleanme scan ROOT`.

## Known issues in older releases

Use the latest release. These older versions had important defects:

| Version | Issue | Fixed |
|---|---|---|
| `0.2.0` | Could run a real cleanup for a misplaced `--dry-run` and could delete another project's source tree. This release is yanked. | `0.2.1` |
| `0.2.10` and earlier | A nested output root could be treated as private even when it was inside another package's source tree. | `0.2.11` |
| `0.2.12` | Published on crates.io without a matching GitHub release, so self-update could not find its release assets. | `0.2.13` |

If you used `0.2.0` or `0.2.10` and earlier, check version control and backups for affected source files. See the [changelog](../CHANGELOG.md) for release-specific details.

## Cleanup finds nothing

Bare `cargo cleanme` performs cleanup for workspaces selected within the maintenance scope and anchored to the working directory. A configured `scan.root` replaces the routine scope. A project outside the selected roots, a project without build output, or recently active output may leave nothing eligible.

Try a scan and then preview the same bounded root:

```sh
cargo cleanme scan ~/projects/my-app
cargo cleanme ~/projects/my-app --dry-run
```

The default recent-activity window is 300 seconds. Recently used projects are protected. Wait for the window to pass rather than changing timestamps.

## Cleanup is blocked or exits 1

Check the report and stderr for discovery errors or unresolved manifests inside the selected proof boundary. An unresolved manifest outside that boundary is reported but does not block the selected cleanup. A known output location may hold only overlapping cleanup units; an unknown location can block the boundary.

For a manifest Cargo cannot resolve, try:

```sh
cargo locate-project --workspace --manifest-path /path/to/Cargo.toml
```

Fix the manifest or workspace configuration, then preview the same scope again. A Full cleanup rechecks learned roots. It may clean proven roots and still exit `1` if another root cannot be proven. If no roots are learned, run `cargo cleanme scan` first.

## A filter seems to have no effect

`scan.ignore` and `scan.unignore` apply to routine maintenance and machine-wide scans. An explicit root, including configured `scan.root`, bypasses them. To test the filters, run `cargo cleanme scan` or routine maintenance without `scan.root`.

Both settings require absolute paths. `scan.ignore` accepts glob patterns; `scan.unignore` is a literal path, not a glob. Cleanup `include` and `exclude` patterns match canonical workspace roots, not build-output directories.

## `--dry-run` and `--cargo-preview` differ

`--dry-run` runs the application's decision and proof checks without invoking `cargo clean`. `--cargo-preview` invokes Cargo's own `clean --dry-run --verbose` after the same safety checks. Use `--dry-run` when no cleanup command should run.

## A scheduled run cleans nothing or fails

Check these in order:

1. **Working directory:** a bare command is anchored to the scheduler's working directory. Use an explicit `ROOT` for a fixed scope.
2. **Config:** configured `scan.root` replaces the routine maintenance scope.
3. **Account and permissions:** use the account that can read the intended roots. Root may use a different home directory and config.
4. **Activity:** projects touched within `scan.recency_seconds` are protected.
5. **Exit code:** trust the process status. A partial Full cleanup can have `status=ok` in log output while exiting `1`.

See [Unattended operation](AUTOMATION.md) for scheduler guidance.

## The command is not found

For Cargo installations, put Cargo's binary directory on PATH. For prebuilt installations, add the installer destination. The POSIX installer can update supported zsh or bash startup files; the PowerShell installer prints PATH guidance but does not edit PATH. See [Installing](INSTALLING.md#destinations-and-path).

## Installer or self-update fails

- **No prebuilt binary for the host or release:** install from crates.io with `cargo install cargo-cleanme --locked`.
- **Missing or mismatched checksum:** stop and retry later. The installer refuses to place an unverified binary.
- **Network or release-host error:** retry later. A transport failure does not trigger a Cargo fallback.
- **Destination already exists:** use `--force` (PowerShell: `-Force`) only if you intend to replace it.
- **Cargo-managed binary:** update it with `cargo install cargo-cleanme --locked --force`.
- **Self-update cannot identify the install:** reinstall with Cargo or the [release installer](INSTALLING.md).
- **Running build is newer than published:** the updater leaves it unchanged and will not downgrade it.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Completed, including safe skips or nothing to clean |
| `1` | An incomplete scan or cleanup report, blocked or partial cleanup, a failed cleanup unit, or an editor that exits unsuccessfully |
| `2` | Invalid command, configuration, or a fatal error before a report, including update failures |

When reporting a problem, include the version, operating system, exact command, exit code, and human-readable output. Redact private paths before sharing logs.
