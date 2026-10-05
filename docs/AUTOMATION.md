# Unattended operation

cargo-cleanme has no daemon and installs no service. Unattended use means
*something else* runs it on a schedule and reads what it printed.

This page covers the three things an unattended run needs to get right:

1. **[Log output](#bounded-log-output)** — one bounded line per run, for a
   scheduler that keeps a short history tail.
2. **[Scheduling](#scheduling-with-greggd)** — a worked greggd configuration,
   plus scheduler-neutral equivalents.
3. **[Operational limits](#operational-limits)** — what can go wrong, and what
   the exit codes mean when it does.

→ [USAGE.md](USAGE.md) for the full command surface and the safety model;
[TROUBLESHOOTING.md](TROUBLESHOOTING.md) for symptom-first diagnosis.

---

## Bounded log output

`--format log` emits **exactly one ASCII line on stdout** for each scan or
cleanup report, and nothing on stderr for a successful run.

```console
$ cargo cleanme --format log
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0

$ cargo cleanme scan --format log
cargo-cleanme op=scan status=ok scope=full manifests=184 groups=31 bytes=51335569408 diagnostics=2

$ cargo cleanme --format log
cargo-cleanme op=clean status=blocked scope=routine mode=execute cleaned=0 skipped=0 failed=0 reason=ownership_unproven diagnostics=1
```

The line is designed to be retained and read by a human in a pane, and to be
grepped by a monitor. It is deliberately **not** a machine contract:

| You want | Use |
|---|---|
| one bounded line for a history pane | `--format log` |
| complete, versioned, per-unit machine data | `--format json` |
| interactive detail | `--format human` |

**Anything that parses cargo-cleanme output should parse JSON.** Log exists
because a scheduler's retained history entry is small; JSON exists because a
consumer must be able to rely on a field. Log drops per-unit detail entirely.

### Fields

| Field | Present | Meaning |
|---|---|---|
| `op` | always | `scan`, `clean`, `config`, or `update` |
| `status` | always | `ok`, `blocked`, `failed`, or `error` |
| `scope` | reports | `full`, `explicit`, or `routine` — what was resolved, not what was typed |
| `mode` | cleanup only | `execute`, `simulate`, or `preview` |
| `reason` | when blocked or failed | a stable code, never English prose |
| counts and bytes | when they fit | base-10 ASCII integers |

Guarantees:

- **At most 384 bytes**, excluding the newline. Optional fields are dropped
  whole from the tail when the line would overflow — a value is never
  truncated, so nothing is ever cut in half.
- **ASCII only**, so no value ever needs quoting and no escaping changes the
  size.
- **No paths, no Cargo stderr, no config text.** Those belong in the exit-code
  triage, not in a retained history entry.
- **`reclaimed_bytes` is printed only for a cleanup that actually executed.**
  A simulation and a Cargo preview both print nothing there rather than a
  figure they did not earn.
- **One line per run.** Never two, never a paragraph.

### What log mode does *not* do

- It does **not** disable progress in other formats, and it is **not** selected
  automatically by a non-TTY stdout. `--format log` is always explicit.
- It does **not** suppress `--stats`. `--stats` remains an opt-in stderr
  override; leave it out of scheduled jobs.
- It does **not** change exit codes, and it does **not** reach into JSON. If
  both formats are needed, run twice, or use JSON.
- It does **not** hide a clap usage error. A mistyped flag is still clap's
  message on stderr and exit `2`; reimplementing the parser error stack to
  force a log line would trade a clear message for a terse one.

---

## Scheduling with greggd

**greggd** is a small scheduler suited to exactly
this workload: it runs an argv array directly, captures stdout and stderr into
bounded in-memory tails, serializes scheduled commands, and can defer work when
the machine is already busy.

### Worked configuration

```toml
# User-owned/rootless greggd recommended for developer-home maintenance.
scheduler_history_limit = 5

[[jobs]]
name = "cargo-cleanme-daily"
schedule = "0 2 * * *"
command = ["/home/user/.cargo/bin/cargo-cleanme", "--format", "log"]
max_load = 8.0
load_window = "15m"
retry_interval_ms = 300000
max_wait_ms = 86400000

[[jobs]]
name = "cargo-cleanme-weekly-scan"
schedule = "0 3 * * 0"
command = ["/home/user/.cargo/bin/cargo-cleanme", "scan", "--format", "log"]
max_load = 8.0
load_window = "15m"
retry_interval_ms = 300000
max_wait_ms = 86400000
```

### What to notice

- **Use an absolute path.** Scheduled jobs do not inherit your interactive
  `PATH`. `/home/user/.cargo/bin/cargo-cleanme` must exist and be executable by
  the account the job runs as; `command = ["cargo-cleanme", …]` will usually
  fail with "not found" and give you nothing to diagnose from.
- **`command` is an argv array, not a shell string.** There is no shell, so
  there is no `&&`, no pipes, no redirection, no variable expansion, and no
  globbing. Chain jobs with greggd's own dependency support, or run a second
  command as a second job.
- **The daily job cleans; the weekly job only reads.** Bare
  `cargo cleanme --format log` is Routine cleanup over the maintenance scope.
  `cargo cleanme scan --format log` is Full reconciliation and **never deletes
  anything**, but it walks the platform roots and can be expensive on a
  machine with a large filesystem — hence weekly, at night, behind load gating.
- **Load gating, retry, and max-wait belong to greggd, not to cargo-cleanme.**
  `max_load`/`load_window` defer the job; `retry_interval_ms`/`max_wait_ms`
  bound how long it keeps trying. cargo-cleanme has no scheduler, no sampling,
  and no backoff policy of its own, and adding them is out of scope.
- **The history pane is bounded and in memory.** `scheduler_history_limit` is a
  count of retained records per job, and each raw stream tail is bounded before
  being escaped for display. The 384-byte log line is sized to fit comfortably
  inside that bound — which is why log mode exists at all.
- **Never put credentials in argv.** Command lines are visible to other
  processes on the host and land in scheduler history. cargo-cleanme needs no
  credentials and emits none.
- **Do not read scheduler history as an authenticity boundary.** It records what
  a command printed. It is not a signature, and it is not a substitute for
  verifying that the binary you are running is the one you installed.

### Running as a user, not root

The normal Linux system service for greggd runs as the `greggd` service account
with `ProtectHome=true`. That is the right hardening for a server and the wrong
setup for this workload: **a developer home directory is exactly where cargo
build output lives**, so a system greggd will generally be unable to see or
clean it.

Two consequences:

- A system greggd will usually report no cleanup roots at all. That is not a
  cargo-cleanme failure — a successful run with nothing in scope looks
  identical (`status=ok … cleaned=0`).
- Jobs also inherit greggd's own uid/euid restrictions.

**Run a user-owned, rootless greggd instance for developer-home maintenance.**
It can read your home, it runs as you, and nothing needs `sudo`.

---

## Scheduler-neutral equivalents

The integration above is greggd-shaped; the principles are not.

**cron** — runs through a shell, so quoting is your problem, and there is no
load gating:

```cron
0 2 * * * /home/user/.cargo/bin/cargo-cleanme --format log >> /home/user/.local/state/cargo-cleanme.log 2>&1
0 3 * * 0 /home/user/.cargo/bin/cargo-cleanme scan --format log >> /home/user/.local/state/cargo-cleanme.log 2>&1
```

**systemd timers** — gives you `OnFailure`, `RandomizedDelaySec`, and the
standard journal, at the cost of maintaining unit files:

```ini
[Unit]
Description=cargo-cleanme routine maintenance

[Service]
Type=oneshot
ExecStart=/home/user/.cargo/bin/cargo-cleanme --format log
# The hard guarantee you want from a timer: never clean while you are logged in.
ExecCondition=/usr/bin/pgrep -u %u -x cargo
```

Whichever scheduler you use, the three rules are the same: **absolute path,
one bounded line, load-aware timing, and never root for a developer home.**

---

## Operational limits

### What an unattended run will not do

- It will not clean anything you built in the last `recency_seconds` (default
  300). Nightly schedules are far outside that window; a job that somehow runs
  during a build protects the build.
- It will not clean a group whose exclusive ownership it cannot prove, and it
  will not clean **any** group in a scope where one discovered manifest fails
  to resolve. That is the whole scope, not just the broken project.
- It will not grow. `cargo cleanme` never installs a service, never spawns a
  resident process, and never modifies your `PATH`.

### Exit codes

| Code | Meaning | What a scheduler should do |
|---|---|---|
| `0` | completed, including safe per-unit skips and "nothing to clean" | nothing |
| `1` | scope incomplete, ownership unproven, or an operation failed | **read the log line.** `status=blocked` with a `reason=` code is usually an operator problem, not a crash |
| `2` | could not run — usage or configuration error | fix the job definition; retrying will not help |

`status=blocked` on exit `1` is a *success-shaped* result with a typed blocker,
not a crash. The reasons you will actually see:

| `reason=` | What it means |
|---|---|
| `ownership_unproven` | at least one discovered manifest could not be resolved, so coverage of the scope is unproven and **nothing** was cleaned |
| `incomplete_discovery` | discovery produced diagnostics, so the combined ownership universe is incomplete |

Both mean "no bytes were removed", by construction. Re-running rarely helps;
the cause is usually an unbuildable or in-flux workspace inside the scope.

### Interpreting a nonzero exit

A `status=error reason=config` line means the run failed before any report
existed — an unreadable or invalid `config.toml`, or an unusable root. There is
no summary line for that case, because there is no report to summarise. Run the
same command in the foreground without `--format` to get the full message:

```sh
cargo cleanme --config ./config.toml          # human prose, exit 2
```

### Credentials and disclosure

cargo-cleanme emits no credentials and takes none. Its scheduled output contains
no paths in log mode, which is deliberate: a retained history pane is a
convenience, not a place to accumulate a directory listing. If you need the
paths a job touched, use `--format json` and write it somewhere you control.

---

## See also

- [USAGE.md](USAGE.md) — commands, the cleanup safety model, exit codes
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) — symptom-first diagnosis
- [UPDATE.md](UPDATE.md) — keeping the scheduled binary current