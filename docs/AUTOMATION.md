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

`--format log` emits **exactly one ASCII line on stdout** for each scan,
cleanup, `config`, or `update` invocation, and nothing on stderr for a
successful run.

When cleanup is blocked by incomplete discovery, the human/JSON report includes
the workspaces and cleanup units actually resolved after the walk. A diagnostic
count no longer implies that Cargo resolution was skipped. The `reason` field
names one of **two** block reason codes — `incomplete_discovery` and
`ownership_unproven` — and both are **boundary-scoped**: they describe the
proof boundary that was blocked, not the whole machine.
[Operational limits](#operational-limits) has the exact wording for each.

```console
$ cargo cleanme --format log
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0

$ cargo cleanme scan --all --format log
cargo-cleanme op=scan status=ok scope=full manifests=184 groups=31 bytes=51335569408 diagnostics=2

$ cargo cleanme --format log
cargo-cleanme op=clean status=blocked scope=routine mode=execute cleaned=0 skipped=0 failed=0 reason=ownership_unproven diagnostics=1

$ cargo cleanme --format log config path
cargo-cleanme op=config status=ok action=path
```

`config` and `update` report the action, never the content: `config path`'s
whole output *is* a path and `config show`'s is the config file, and both are
exactly what a retained history pane must not hold. Use the default human format
when you want the path or the text.

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
| `mode` | cleanup and update | `execute`, `simulate`, or `preview` (cleanup); `simulate` or `execute` (update) |
| `reason` | when blocked or error | a stable code, never English prose — the two sets are listed below |
| `action` | `config` | the action that ran: `path`, `show`, or `edit`. Never the path, never the text |
| `from`, `to` | `update` | the version being replaced and the version resolved |
| `target` | `update` | the target triple — a triple, never a path |
| `manifests`, `groups`, `bytes`, `diagnostics` | `scan` | base-10 ASCII integers: discovered manifests, output groups, total bytes, diagnostics |
| `cleaned`, `skipped`, `failed`, `diagnostics` | `cleanup` | base-10 ASCII integers: units cleaned, units skipped, units failed, diagnostics |
| `reclaimed_bytes` | `cleanup`, only when it executed | base-10 ASCII integer; never printed for a simulation or a Cargo preview |

`reason` is a code, and there are exactly two sets, told apart by `status`:

| `status` | `reason=` codes | What the code means |
|---|---|---|
| `blocked` | `ownership_unproven`, `incomplete_discovery` | the proof boundary was blocked; see [Operational limits](#operational-limits) |
| `error` | `config`, `io`, `scan`, `provenance`, `update` | the run failed before any report existed: a configuration or root problem (`config`), an I/O failure (`io`), a scanner failure (`scan`), a self-update refusal (`provenance`), or a self-update failure (`update`) |

**The JSON envelope drops three things the safety model depends on.** A
scheduler parsing `--format json` cannot see any of them:

- `capability` — *which kind* of unproven ownership a participant carries:
  `RefusedByCargo` (Cargo ran and refused the manifest — it is in no workspace
  Cargo can see, so it owns no bytes) or `Unproven` (byte capability unproven,
  which merges everything). The serialized participant
  (`unresolved_participants[]`) carries `manifest`, `stage`, and `reason` only.
- `out_of_scope_unresolved` — the count of manifests that did not resolve
  **outside** the selected scope. The human output renders it as one bounded
  line, and the entries are folded into the cleanup JSON's
  `summary.diagnostics` **count**, but there is no field for it: a JSON
  consumer cannot tell an out-of-scope diagnostic from any other diagnostic.
- the **stable block code** — `scope_reason` in the cleanup JSON carries the
  human prose. The code itself (`ownership_unproven`, `incomplete_discovery`)
  is a log-mode field only, so a JSON consumer sees *that* the boundary was
  blocked and has to parse English to learn *why*.

So a JSON consumer can see *that* a boundary was blocked, but not *what kind* of
unproven ownership caused it, not the boundary's own block code, and cannot
tell "the boundary is clean" from "the system is clean" without also reading
the human output.

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
command = ["/home/user/.cargo/bin/cargo-cleanme", "scan", "--all", "--format", "log"]
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
  `cargo cleanme --format log` is cleanup of the maintenance scope, anchored
  at the job's working directory, so run the daily job from your home
  directory (or name an explicit root). The working directory joins that scope
  only when the Routine scope does not already reach it, and your home
  directory *contains* the seed roots — so a daily job run from your home
  keeps the bounded machine-wide sweep and does not widen into everything
  under your home. `cargo cleanme scan --all --format log` is Full
  reconciliation and **never deletes
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
0 3 * * 0 /home/user/.cargo/bin/cargo-cleanme scan --all --format log >> /home/user/.local/state/cargo-cleanme.log 2>&1
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
- It will not clean a group whose exclusive ownership it cannot prove.
  Cleanup fails closed **only for the proof boundary** — the scope you
  selected: the working-directory anchor, the explicit roots, or the Full set.
  A manifest Cargo cannot resolve *inside* that boundary blocks it. A manifest
  *outside* it is reported as a bounded `out_of_scope` diagnostic and never
  blocks: cleaning a healthy anchor while poison exists elsewhere is the
  intended behaviour, so a boundary clean is never read as a system clean.
- A Full cleanup (`cargo cleanme --full`) will not clean a learned root it
  could not prove. The proof boundary is the **learned root**: each one is
  re-walked as its own walk root and judged on its own traversal, and a root
  whose coverage could not be proven is dropped from the candidate set and
  reported per unit with the reason code `skipped_incomplete_coverage`. That
  code is a **machine** field: it appears in the JSON envelope's
  `units[].reason_code`. The human surface names the same fact in prose —
  the root, the cause, and the alternative — and does not print the code
  itself. The run exits `1` when **at least one** learned
  root was unproven, even if every root that could be proven was cleaned — a
  partial Full cleanup must not read as a complete one. When **no** learned root
  could be proven, the scope is blocked with the typed unproven-ownership block
  and the run exits `1`; it never exits `0` with zero rows. The one refusal the
  Full cleanup still has is a reconciliation that produced **no fresh
  generation** at all: that exits `2` with `full reconciliation produced no
  fresh generation; no cleanup commands were run against stale learned roots`.
- It will not grow. `cargo cleanme` never installs a service and never spawns a
  resident process. The tool itself never modifies your `PATH`; the separate
  bootstrap installer adds one guarded block to a zsh or bash startup file so a
  new shell can find the binary (see
  [docs/INSTALLING.md](INSTALLING.md#path-integration)), and `--no-shell-profile`
  or `--no-path` suppresses it.

### Exit codes

| Code | Meaning | What a scheduler should do |
|---|---|---|
| `0` | completed, including safe per-unit skips and "nothing to clean" | nothing |
| `1` | scope incomplete, ownership unproven, a learned root could not be proven, or an operation failed | **read the log line.** `status=blocked` with a `reason=` code is usually an operator problem, not a crash |
| `2` | could not run — usage or configuration error | fix the job definition; retrying will not help |

`status=blocked` on exit `1` is a *success-shaped* result with a typed blocker,
not a crash. The reasons you will actually see:

| `reason=` | What it means |
|---|---|
| `ownership_unproven` | coverage of the **proof boundary** could not be proven — a manifest inside it did not resolve with an unknown owner, or an authorization or re-proof step could not be verified — so **nothing in the boundary** was cleaned |
| `incomplete_discovery` | discovery produced diagnostics **inside the proof boundary**, so the combined ownership universe for the boundary is incomplete — including an in-boundary automatic (seed/learned) root that is a symlink or cannot be read, which is never silently omitted |

There is one exit-`1` case the log line does **not** mark as `status=blocked`:
a Full cleanup (`cargo cleanme --full`) in which **at least one learned root
could not be proven**. The line then reads `status=ok` — the roots that could be
proven were cleaned — while the process exits `1`, because the requested
boundary was not fully cleaned:

```console
$ cargo cleanme --full --dry-run --format log
cargo-cleanme op=clean status=ok scope=full mode=simulate cleaned=0 skipped=0 failed=0 diagnostics=4
```

**The exit code is the authority, not `status=`.** A scheduler that alerts on
the line alone will miss a partial Full cleanup; a scheduler that alerts on the
exit code will not.

Both reasons are boundary-scoped. Not every unresolved manifest inside the
boundary blocks it: one that Cargo *named* the owner for, or named as a member
of the scope, is reported and never blocks — there is no unknown owner left to
merge. Only an in-boundary manifest with a genuinely unknown owner blocks.

The early blocks — discovery diagnostics, unprovable ownership — are raised
before the boundary is cleaned and run no cleanup command at all. A block
raised mid-run — a changed admission premise, a changed manifest set, a changed
output environment — stops the remaining units and says "no further cleanup
commands were run", so units cleaned before it keep their `cleaned` rows.
Re-running rarely helps; the cause is usually an unbuildable or in-flux
workspace **inside the boundary**.

### Interpreting a nonzero exit

A `status=error reason=config` line means the run failed before any report
existed — an unreadable or invalid `config.toml`, or an unusable root. There is
no summary line for that case, because there is no report to summarise. Run the
same command in the foreground without `--format` to get the full message:

```sh
cargo cleanme --config ./config.toml          # human prose, exit 2
```

These commands can refuse with exit `2` before any report exists, and they all
read as "the tool declined" rather than as a crash:

| Command | Exit | Stderr line (human format) |
|---|---|---|
| `cargo cleanme config show --format json` | `2` | `cargo-cleanme: configuration error: config command does not support --format json; use human or log output` |
| `cargo cleanme --full` (no fresh generation) | `2` | `cargo-cleanme: configuration error: full reconciliation produced no fresh generation; no cleanup commands were run against stale learned roots` |
| `cargo cleanme --full scan --all` | `2` | `cargo-cleanme: configuration error: `scan` does not accept --full: these are cleanup flags, and `cargo cleanme scan` is read-only. Run the cleanup without the subcommand, or run `scan` on its own.` |
| `cargo cleanme --older-than 3600 clean --older-than 60` | `2` | `cargo-cleanme: configuration error: --older-than was given at the root and on `clean`. The two values are ambiguous; give it at one level only.` |

In `--format log` the same refusals are one bounded stderr line —
`cargo-cleanme op=config status=error reason=config` — and the prose is not
printed at all.

`config` has no JSON envelope: `config path` and `config show` are a path and
the config file's text, and emitting them on a `--format json` stdout would
break the one-document contract. `--format log` is accepted and exits `0`,
reporting the action instead of the content.

`cargo cleanme --full` refuses rather than cleaning against learned roots that
the current reconciliation could not prove. That refusal is not transient:
re-running the same command changes nothing, so fix the input or the
reconciliation rather than the schedule.

The last two rows are **usage** errors, and they are the tool's own doing:
`--full` selects a cleanup operation, and a cleanup flag handed to a read-only
or local-only subcommand is rejected rather than silently discarded — a flag you
typed and the tool ignored looks exactly like a flag the tool honoured. The same
override given at both the root and on `clean` is ambiguous, so it is rejected
rather than silently resolved in favour of one of the two values. Neither is
transient: fix the command line.

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
