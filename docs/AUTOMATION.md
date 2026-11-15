# Unattended operation

cargo-cleanme has no daemon or scheduler. Run it from cron, systemd, or another
scheduler and use its process exit code to determine success.

Use `--format log` for a short retained summary, or `--format json` when a
consumer needs complete, versioned per-unit data. The log format is intended for
people and scheduler history, not as a parsing contract.

## Preview before scheduling

The default command performs cleanup. Test the exact scope with `--dry-run`
before scheduling it:

```sh
cargo cleanme ~/projects/my-app --dry-run --format log
```

Then schedule the same bounded root without `--dry-run`:

```sh
cargo cleanme ~/projects/my-app --format log
```

`--dry-run` performs the cleanup checks without invoking `cargo clean`.
`--cargo-preview` is different: it invokes Cargo's own dry-run command.

If you schedule the bare command instead, set its working directory
intentionally. The maintenance scope is anchored to that directory. A
configured `scan.root` replaces the normal maintenance scope. Use an explicit
`ROOT` when the job should always operate on the same bounded directory.

## Log output

A successful invocation in log mode writes one bounded ASCII summary line to
stdout. It contains no paths and is limited to 384 bytes, excluding the final
newline. Optional fields may be omitted to stay within the limit. A fatal
runtime error before a report exists is reported as one bounded line on stderr.
Command-line usage errors use the normal parser message. `--stats`, if requested,
also writes to stderr.

Example:

```text
cargo-cleanme op=clean status=ok scope=explicit mode=execute cleaned=2 skipped=1 failed=0 reclaimed_bytes=12345 diagnostics=0 unresolved=0
```

Use JSON instead of log output when a program needs complete fields or
per-workspace details:

```sh
cargo cleanme ~/projects/my-app --format json
```

`--verbose` changes human-readable output only. It does not change JSON or log
output. Progress is disabled in log mode.

## Scheduling examples

Use an absolute path to the executable. Schedulers may not inherit the PATH of
your interactive shell.

### cron

```cron
0 2 * * * /home/user/.cargo/bin/cargo-cleanme /home/user/projects/my-app --format log
0 3 * * 0 /home/user/.cargo/bin/cargo-cleanme scan --format log
```

The first command cleans a fixed bounded root. The weekly scan does not clean
build output. It updates learned-root state, walks the platform, and may take
several minutes. Redirect stdout and stderr to a log file if your scheduler does
not retain them.

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

Set `User` and `WorkingDirectory` to the account and scope you intend. Avoid
running as root for developer-home maintenance, since root may use a different
home directory and configuration.

## Exit status and alerts

| Code | Meaning | Suggested action |
|---|---|---|
| `0` | Completed, including safe skips or nothing to clean | No action needed |
| `1` | Incomplete scan, blocked or partial cleanup, or an operation failure | Inspect the report and stderr; address the cause before treating the job as complete |
| `2` | Invalid command, configuration, or another fatal error before a report | Fix the job or config; retrying unchanged will not help |

The process exit code is authoritative. In a partial Full cleanup, the log line
can say `status=ok` for the roots that were proven while the process exits `1`
because another learned root could not be proven. Do not alert on `status=`
alone.

For a cleanup failure, inspect the recorded stderr or repeat the same scope
with `--dry-run` and without `--format log` to see human-readable details without
cleaning. `--format json` includes complete per-unit report data. See [Using
cargo-cleanme](USAGE.md) for the safety model and [Troubleshooting](TROUBLESHOOTING.md)
for common causes.
