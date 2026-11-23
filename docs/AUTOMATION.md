# Unattended operation

cargo-cleanme has no daemon or scheduler. Run it from cron, systemd, or another scheduler, and use the process exit code to determine success.

Use `--format log` for a short retained summary or `--format json` when a consumer needs complete, versioned per-unit data. Log output is for people and scheduler history, not for parsing.

## Preview the exact scope

Cleanup runs by default. Test the same scope with `--dry-run` before scheduling it:

```sh
cargo cleanme /home/user/projects/my-app --dry-run --format log
cargo cleanme /home/user/projects/my-app --format log
```

`--dry-run` runs the application's cleanup checks without invoking `cargo clean`. `--cargo-preview` is different: it invokes Cargo's own dry-run command after the same safety checks.

If you schedule bare `cargo cleanme`, choose its working directory intentionally. Maintenance is anchored to that directory. A configured `scan.root` replaces the routine maintenance scope. Use an explicit `ROOT` when the job should always cover the same bounded directory.

## Output

A successful `--format log` invocation writes one bounded ASCII line to stdout. It contains no paths and is limited to 384 bytes, excluding the newline. Optional fields may be omitted. A fatal runtime error before a report exists is written as one bounded line to stderr. Command-line usage errors use the normal parser message. `--stats`, if requested, also writes to stderr.

Example:

```text
cargo-cleanme op=clean status=ok scope=explicit mode=execute cleaned=2 skipped=1 failed=0 reclaimed_bytes=12345 diagnostics=0 unresolved=0
```

Use JSON instead of log output when a program needs complete fields or per-workspace details:

```sh
cargo cleanme /home/user/projects/my-app --format json
```

`--verbose` changes human-readable output only. It does not change JSON or log output. Progress is disabled in log mode.

## Scheduling examples

Use an absolute path to the executable because schedulers may not inherit your interactive shell's PATH.

### cron

```cron
0 2 * * * /home/user/.cargo/bin/cargo-cleanme /home/user/projects/my-app --format log
0 3 * * 0 /home/user/.cargo/bin/cargo-cleanme scan --format log
```

The first job cleans one bounded root. The weekly scan does not clean build output. It reconciles learned roots across the machine and may take several minutes. Redirect stdout and stderr if your scheduler does not retain them.

### systemd service

```ini
[Unit]
Description=cargo-cleanme maintenance

[Service]
Type=oneshot
User=user
WorkingDirectory=/home/user/projects/my-app
ExecStart=/home/user/.cargo/bin/cargo-cleanme --format log
```

Set `User` and `WorkingDirectory` to the account and scope you intend. Avoid running as root for developer-home maintenance because root may use a different home directory and config.

## Exit status and alerts

| Code | Meaning | Suggested action |
|---|---|---|
| `0` | Completed, including safe skips or nothing to clean | No action needed |
| `1` | An incomplete scan or cleanup report, blocked or partial cleanup, a failed cleanup unit, or an editor that exits unsuccessfully | Inspect the report and stderr before treating the job as complete |
| `2` | Invalid command, configuration, or a fatal error before a report, including update failures | Fix the job or config; retrying unchanged will not help |

The process exit code is authoritative. A partial Full cleanup can log `status=ok` for the roots that were proven while exiting `1` because another learned root could not be proven. Do not alert on `status=` alone.

For details, rerun the same scope with `--dry-run` and human output, or use `--format json` for complete per-unit data. See [Using cargo-cleanme](USAGE.md) for the safety model and [Troubleshooting](TROUBLESHOOTING.md) for common causes.
