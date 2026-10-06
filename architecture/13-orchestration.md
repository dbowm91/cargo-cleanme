# Orchestration — argument dispatch and the two pipelines

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** M012A and M012B have rewritten this file's subject. Bare
> `cargo-cleanme` is now a Routine **execute cleanup**, not a scan; argv
> normalization and intent resolution moved into `Cli::invocation`
> (`src/cli.rs:250-305`); `--format log` added a third output format; and
> `clean --full` on an incomplete scan now fails closed. Sections 1-6 below have
> been re-derived against the current source. The earlier status note recording
> the `scope`-label fix and the state-publish exit-code fix is preserved by the
> findings themselves (§7, §8), not by a banner claiming them.

`src/main.rs`, 737 lines, zero inline tests. The only module that reaches every
other one, and the only place the two pipelines are stitched together.

## 1. Responsibility

`main.rs` owns argv parsing and the exit-code contract (`:10-36`), collaborator
wiring, the reconciliation branch deciding whether persisted state is rewritten
(`:500-579`), and the choice of output surface. It owns **no domain logic**:
scope to `policy::resolve` (`:310-316`, `:438-444`), classification to
`workspace::analyze_groups` (`:594-603`), authorization and mode semantics to
`cleanup::clean_with_roots_policy_selector` (`:224-233`), and even "is this
incomplete?" to merely *reading* a `PlatformRoot` + `Error` diagnostic
(`:524-527`, `:605-609`).

**It declares two types.** `RunOptions` (`:60-66`) is the process-wide triple
`no_progress` / `stats` / `format`, built once in `run` (`:69-73`) and passed to
both report-producing operations. `ResolvedCleanup` (`:144-150`) carries the
resolved roots, the machine scope label, and the discovery-state generation.
Both exist to make the "one resolved intent per invocation" property structural:
because `Cli::invocation` collapses every cleanup spelling into one
`CleanupRequest` (`src/cli.rs:291-302`), there is exactly one place where mode,
scope, and policy are bound, and one place that emits the report.

**It has no inline tests**: `grep -cE '#\[cfg(test)\]|#\[test\]' src/main.rs`
→ `0` (see §10).

Its top `use` block is nine lines (`:1-9`) and names `cli`, `config`, `domain`
(`ScanScope`), `error`, and `std::path`. Two function-scoped imports live inside
`run_scan` — `use cargo_cleanme::{domain, progress};` (`:423`) and
`use cargo_cleanme::discovery_state::StateLoad;` (`:498`) — and one inside
`run_update`/`run_scan` is fully qualified (`serde_json::…`). Everything else is
named at the point of use (`cargo_cleanme::update::update` `:86`,
`cargo_cleanme::workspace::SystemCargoRunner` `:488`,
`cargo_cleanme::progress::IndicatifRenderer` `:265`). Consequence: renaming a
library function breaks this file at many scattered sites, and there is no single
list of what it depends on. Modules reached: `cli`, `config`, `error`, `domain`,
`discovery`, `workspace`, `discovery_state`, `cleanup`, `update`, `editor`,
`progress`, `report`, `output`, plus `serde_json` and `directories` (`:535`).

**Twelve free functions and two structs**, all private:

| Function | Lines | Role |
|---|---:|---|
| `main` | `10-36` | parse, dispatch, fatal rendering, `process::exit` |
| `operation_name` | `40-47` | `Invocation` → stable `op=` token for the log fatal line |
| `error_code` | `51-59` | `AppError` → stable `reason=` code |
| `run` | `67-83` | config path, `RunOptions`, four-arm dispatch |
| `run_update` | `85-101` | plan, JSON / dry-run four-liner / one-line success |
| `run_config` | `:122` | `path`, `show`, `edit` + three post-edit checks |
| `run_cleanup` | `:195` | the cleanup pipeline (§5) |
| `resolve_cleanup_roots` | `:314` | scope → roots + label + generation (§5) |
| `emit_cleanup` | `:405` | three-branch format dispatch for a cleanup report |
| `collapse_roots` | `:440` | canonicalize, sort, dedupe, nest-collapse (§6) |
| `scope_label` | `:464` | `ScanScope` → JSON `scope` string |
| `run_scan` | `:485` | the scan pipeline (§4) |
| `update_json` | `:724` | the one hand-rolled envelope (§8) |

The two log helpers are new in M012B and exist only to serve `main`'s fatal path;
they are the reason `main` parses argv itself instead of leaving it inside `run`
(§8). Neither is reachable from any other function.

## 2. Entry and exit

```rust
fn main() {                                                   // :10-36
    let cli = Cli::parse_normalized();                        // :14
    let invocation = cli.invocation();                        // :16  (captured pre-move)
    let format = cli.format;
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if format == OutputFormat::Log {
                eprintln!("{}", output::log::fatal(operation_name(&invocation),
                                                     error_code(&e)));   // :26-29
            } else {
                eprintln!("cargo-cleanme: {e}");                          // :31
            }
            std::process::exit(2)
        }
    }
}
```

**`main` now parses argv itself** (`:14`), and `run` receives an already-parsed
`Cli`. This is M012B's consequence, not a style choice: a fatal error can only be
rendered format-aware if the format is known before the operation runs, and a
`try_parse` buried inside `run` could not tell "clap rejected this" from "the
filesystem is broken". The price is two values captured purely for the failure
path — `invocation` at `:16`, before the move into `run`, and `format` at `:17`.

`Err(e)` prints one line on **stderr** and exits **2**; every `AppError` variant
(`src/error.rs:3-16`) exits 2. A command can also return `Ok(1)`. **Exit 1 means
"ran, and something failed"; exit 2 means "could not run."** The only `Ok(1)`
producers: `:125` (editor passthrough), `:255-259` (cleanup partial failure),
`:707-709` (scan Full incomplete). The old `clean --full` preflight `Ok(1)` is
gone — it is now an `AppError`, so 2 (§5).

| Code | Meaning | Produced by |
|---:|---|---|
| 0 | Success | `:710`; Update `:100`; Config `:141`; empty-roots cleanup `:218`; cleanup `:255-259` when `failed == 0 && scope_blocked.is_none()` |
| 1 | Ran, partially failed | editor's own code `:125`; cleanup `:255-259`; scan `:707-709` |
| 2 | Could not run | any `AppError` via `main` `:33`; clap usage errors (clap's own path) |
| 0 (clap) | `--help` / `--version` | clap, not this file |

| Command | Codes | Decided at |
|---|---|---|
| bare `cargo-cleanme` / `scan [ROOT]` / `scan --known` | 0, 1, 2 | `:707-710` |
| `clean ROOT …` / `clean` / `clean --known` | 0, 1, 2 | `:255-259` |
| `clean --full …` | 0, **2**, 1, 2 | `:282-291` then `:255-259` |
| `config path` / `show` | 0, 2 | `:105`, `:107`, `:141` |
| `config edit` | 0, **editor's code**, 2 | `:125`, `:117-138` |
| `update [--dry-run]` | 0, 2 | `:100`, `:86` |
| any (clap usage error) | 0 (help/version), 2 | clap |

**The `clean` row has no "no root source" case any more.** Before M012A, `clean`
with no `ROOT`, no `--known` and no `--full` was a usage error
(`AppError::Config("clean requires ROOT, --known, or --full")`). It is now the
maintenance scope — the same intent as bare invocation — so there is no user-facing
way to reach that 2.

**`clean --full` moved a failure from 1 to 2.** A Full scan that does not fully
complete now raises `AppError::Config` (`:287-290`) → exit 2, instead of returning
the scan's own code silently. The old behaviour handed the caller a bare code with
no indication that *no cleanup ran*; the new one says so. The trade-off is real:
"incomplete cleanup scope" now splits across two codes depending on whether the
incompleteness was detected during scan (2) or during unit execution (1). Both are
fail-closed.

`update` can never return 1: `From<UpdateError> for AppError`
(`src/update.rs:355-365`) maps everything to `AppError::Provenance` or
`AppError::Update`, both exit 2 — **including the benign
`UpdateError::AlreadyCurrent` (`src/update.rs:983-987`)**. So "already up to date"
exits **2**, while `docs/TROUBLESHOOTING.md` calls that state success. Unchanged
by M012A/M012B, and still wrong.

### Clap's errors bypass `AppError` entirely

`Cli::parse_normalized()` (`:14`) = `Self::parse_from(normalize_cargo_argv(args_os))`
(`src/cli.rs:238-240`), and unlike `try_parse_from` it **cannot return `Err`**:
on a parse error clap prints its message plus usage and calls its own
`process::exit`. So for a bad flag, unknown subcommand, or violated
`conflicts_with`, the `AppError` branch at `:20-34` is never reached, no
`cargo-cleanme: ` prefix is printed, and the exit code is **2** (clap 4.5's
`Error::exit`; `Cargo.toml:51` pins `clap = "4.5"`). `--help`/`--version` exit
**0**.

**Definitive:** `cargo cleanme clean /x --dry-run --cargo-preview` shows a clap
usage error and exit 2 — indistinguishable by status from a fatal `AppError`. A
script must match the message prefix to separate "you mistyped" from "the
filesystem is broken". This is now *harder* to write than before, because the log
fatal line is also one bare stderr line with no `cargo-cleanme:` prefix: a
log-mode consumer distinguishing the two must check for `status=error`.

## 3. Command dispatch

`run` matches on **`Invocation`**, not on `Option<Command>` (`:77-82`):

```rust
match cli.invocation() {
    Invocation::Update { dry_run } => run_update(dry_run, options.format),
    Invocation::Config(command)    => run_config(command, &path),
    Invocation::Scan(intent)       => run_scan(intent, &path, options),
    Invocation::Cleanup(request)   => run_cleanup(request, &path, options),
}
```

**The dispatch table moved into `cli.rs`.** `Cli::invocation()`
(`src/cli.rs:250-305`) is now the single place that turns parsed argv into a
resolved intent, and `main` no longer inspects `command` at all — it does not even
read the field, apart from lifting the three globals at `:69-73`. This is M012A's
structural change and it is the right one: it makes "what did the user ask for" a
pure function of argv, testable without a subprocess, and it removes the
`None`-falls-through-to-scan fallthrough that used to live here.

Ordering consequence: **the config path is still resolved for every command,
including `update`** (`:68` runs before the match), so if `ProjectDirs::from`
returns `None` (`src/config.rs:83-85`) then even `update` and `config path` fail
with exit 2 before reaching their arms.

### What `invocation` resolves

| Input | `Invocation` | Source |
|---|---|---|
| no subcommand | `Cleanup(Maintenance, Execute)` | `src/cli.rs:253-257` |
| `scan ROOT` | `Scan(Explicit(root))` | `src/cli.rs:259` |
| `scan --known` | `Scan(Maintenance)` | `src/cli.rs:260` |
| `scan` (bare) | `Scan(Full)` | `src/cli.rs:261` |
| `config path/show/edit` | `Config(*)` | `src/cli.rs:263` |
| `update [--dry-run]` | `Update { dry_run }` | `src/cli.rs:264` |
| `clean …` | `Cleanup(<scope>, <mode>, <overrides>)` | `src/cli.rs:269-303` |

Two of those rows are the M012A defaults and both deserve to be called out:

- **`None` is a cleanup.** A bare `cargo-cleanme` becomes
  `CleanupScope::Maintenance` with `CleanMode::Execute`, i.e. a Routine
  maintenance run that will actually delete eligible artifacts. It is *not* a
  read-only scan. The root `command` doc string says so in prose
  (`src/cli.rs:22-30`) and the enum comment names the intent
  (`src/cli.rs:204-208`).
- **`clean` with no scope selector is the same thing.**
  `src/cli.rs:284-290` maps `(None, false, _)` to `CleanupScope::Maintenance`, and
  `src/cli.rs:505-510` asserts that `["cargo-cleanme", "clean"]` and
  `["cargo-cleanme"]` produce identical `invocation()` values. This is why
  `main.rs` needs no "no root source" error branch (§2).

Mode resolution is a free function rather than inline matching
(`src/cli.rs:222-234`): `dry_run || dryrun_legacy` → `Simulate`, else
`cargo_preview` → `CargoPreview`, else `Execute`. **Execute is the default.** The
hidden compatibility aliases (`--dryrun`, `--yes`) are folded into the same two
canonical modes; `--yes` is deliberately never bound (`src/cli.rs:265-268`), since
Execute is now reachable without it and binding it would print nothing useful.

**`Update`** (`:85-101`) — `update::update(dry_run)?` then JSON (`:87-88`), human
dry-run four-liner (`:89-96`), or one-line success (`:97-98`); `Ok(0)` at `:100`.
`--format` **is** honoured (`:87`). Note this is the only surface that still has a
hand-rolled envelope (§8), and the only one where `--format log` is *not*
implemented: it falls into the human `else`.

**`Config`** (`:103-142`), three arms. **`Path`** (`:105`) prints the path and
never loads, creates, or validates the file, so it works when the file is absent
or corrupt — it needs only the *path* to be derivable. **`Show`** (`:106-108`) is
`config::show(&config::load_or_create(path)?)?` and therefore **creates** the file
if missing (`src/config.rs:217-222`) — a write side effect. **`Edit`** (`:109-139`)
does `config::ensure_exists` (create-if-absent *without* requiring current contents
to parse, `src/config.rs:225-230`), resolves `$VISUAL`/`$EDITOR`
(`src/editor.rs:7-8`), spawns, then three checks.

**`--format` is not honoured for any `Config` arm** — all three print
unconditionally (`:105`, `:107`, `:138`); `format` is never passed in. Verified,
not inferred: `--format json config show` emits TOML.

**The three post-edit checks.** (1) *Non-zero editor exit* (`:120-126`) prints
`cargo-cleanme: editor exited with {status}; config was not reverted: {path}` to
stderr and returns `Ok(status.code().unwrap_or(1))` — the editor's code is
propagated verbatim, and a signal-killed editor (`code()` is `None`) collapses
to 1; pinned by `tests/cli_contract.rs:724` (`Some(17)`). (2) *File deleted*
(`:127-134`): `load` treats an absent file as "all defaults"
(`src/config.rs:95-97`), so a deleted config would silently reset every policy
→ `Err(AppError::Config(…))` → exit 2. (3) *File modified into an invalid state*
(`:135-137`): `config::load` re-wrapped as
`AppError::Config("edited config {path} is invalid: {e}")`; pinned by
`tests/cli_contract.rs:725-728`, which also asserts the bad content is left in
place (the message says "was not reverted"). Checks 2 and 3 exit 2; check 1
exits with the editor's code. `Ok(0)` at `:141` is reached only for `Path`,
`Show`, and a successful `Edit`.

**Bare invocation is Execute, and that is a standing decision.** Nothing in
`main.rs` gates it: there is no confirmation prompt, and `--dry-run` is opt-in.
The safety property that makes this acceptable is not in this file at all — it is
that every candidate must independently prove exclusive ownership before deletion
and that deletion is always `cargo clean` (§5). But the *default* is worth naming
plainly: a user who types `cargo cleanme` in a project directory gets bytes
deleted, not a report. If that is ever contentious it is a one-line change at
`src/cli.rs:253-257`, and `main.rs` would not need to move.

## 4. The scan pipeline

`run_scan` — `src/main.rs:437-734`, **four** parameters: `intent: ScanIntent`,
`config_path`, `options: RunOptions`, `emit_report: EmitReport`. In order:

1. `scan_start` (`:426`, the recency reference) and `wall_start` (`:427`, behind
   `--stats`); `config::load_or_create(config_path)?` (`:428`) — may create the
   file.
2. `intent` → `(root, full)` (`:433-437`). **This is the M012A change.** Rootless
   `scan` is `ScanIntent::Full` → `(None, true)` and *never* consults a configured
   `scan.root`; `scan --known` is `(None, false)`; `scan ROOT` is
   `(Some(root), false)`.
3. `policy::resolve(domain::ScanRequest { cli_root: root, full }, &c.scan)?`
   (`:438-444`); `recency` bound at `:445`.
4. `IndicatifRenderer::new(!show)` (`:519-521`) where `show` = `format == Human`
   **and** `should_show_progress(no_progress)`; `observer.phase(Discovery)`
   (`:455`).
5. `discovery::discover_manifests_with_attribution(&policy, observer, stats)?`
   (`:459-463`); `discovery_nanos += elapsed` saturating (`:466-468`);
   `_resolution_elapsed` is measured at `:496` and **discarded**.
6. `uncertainty` extracted by filtering `PermissionDenied | Metadata |
   Vanished` (`:469-481`); `manifests_found` set (`:484`).
7. `observer.phase(Resolution)` (`:486`) and
   `workspace::resolve_workspaces(…)` with `SystemCargoRunner` (`:488-495`),
   mutating `counters` and `diagnostics`.
8. State reconciliation (`:498-579`) — §7.
9. `build_groups(&workspaces)` (`:586`), `phase(Analysis)` (`:587`),
   `units_total(Analysis, groups.len())` (`:588`).
10. `clock_cutoff = scan_start.checked_sub(recency)` (`:590-592`).
11. `analyze_groups(&workspaces, groups, scan_start, clock_cutoff, recency, …)`
    (`:594-603`), mutating `counters` and `diagnostics`.
12. `full_incomplete` (`:605-609`); `domain::ScanReport` built (`:611-629`),
    carrying `visited_entries` through.
13. `phase(Reporting)` (`:699`), `finish_and_clear()` (`:700`), **output always
    emitted** (`:635-651`), then diagnostics to stderr (`:655-678`) and `--stats`
    to stderr (`:682-699`).

**Step 2 deserves the emphasis.** Before M012A a rootless `scan` resolved through
`policy::resolve` with `full: false`, which meant a configured `scan.root` could
silently narrow the canonical reconciliation command — and, because reconciliation
is the only thing that expires learned roots, could *forget* roots outside that
one root. Configuration must not be able to do that to the one operation whose
job is to decide what the whole machine looks like. `scan --known` survives as the
read-only inventory over the maintenance scope, which is where a configured root
still wins as an Explicit override (`src/policy.rs:30-42`).

**Step 6, in detail.** `uncertainty` (`:469-481`) filters
`PermissionDenied | Metadata | Vanished`, then `.filter_map(|d| d.path.clone())`.
It reads `discovered.diagnostics`, the **pre-resolution** vector — *not* the
`diagnostics` vec `resolve_workspaces` goes on to mutate at `:493`, so a
permission/metadata diagnostic raised during `cargo metadata` resolution cannot
reach `uncertainty` and cannot protect a learned root. And `filter_map`
silently drops matching diagnostics carrying no path, so `uncertainty` is
path-addressed only.

**Step 10.** `checked_sub` returns `None` if the clock is earlier than
`1970-01-01 + recency_seconds`, killing the scan with exit 2 and a
*config-flavoured* message. `config::load` already range-checks
`recency_seconds` (`src/config.rs:98-105`), so only a mis-set clock causes this.
The same condition is handled non-fatally in `now_seconds()`
(`src/discovery_state.rs:264-269`, `unwrap_or_default()` → 0) — one root cause,
two policies. `tests/end_to_end.rs` uses the *unchecked* subtraction, so the
production guard is untested at the integration level.

### Report emission is an explicit parameter, not a side effect

`run_scan` takes `emit_report: EmitReport` (`src/main.rs:432-435`, passed at
`:441`). `Invocation::Scan` passes `EmitReport::Yes` (`:80`); the Full
reconciliation `clean --full` runs to refresh `discovery-state.json` before
reading `learned_roots` back (`:287`) passes `EmitReport::No`.

That parameter is not a formatting convenience. It is the only thing standing
between `clean --full` and a **double stdout document**, and the JSON envelope is
built on exactly one document per invocation. So `EmitReport::No` suppresses the
report and *nothing else*: diagnostics (`:669-692`) and `--stats`
(`:696-713`) still reach stderr, state reconciliation still runs, and every
decision downstream still reads the report. Only its rendering is withheld.

**This parameter was briefly deleted and had to be restored.** M012A removed the
old `emit_output: bool` on the reasoning that the flag was only ever passed
`false` once, and then `clean --full` emitted the intervening scan report ahead
of the cleanup report — two JSON documents on one stream in `--format json`. The
regression was caught during the M012B documentation pass, not by a test,
because **no test runs `clean --full` at any format** (§10). The lesson is
recorded rather than quietly fixed: a flag that exists for exactly one call site
looks like dead code, and deleting it looks like simplification.

## 5. The cleanup pipeline

`run_cleanup` — `src/main.rs:152-260`. It no longer branches on a `Clean` command
variant; every cleanup spelling arrives as a `CleanupRequest` whose `scope` and
`mode` were resolved by `Cli::invocation` (`src/cli.rs:269-303`).

**Mode** is taken straight from the request (`:158`). There is no mode-selection
logic left in this file — no `yes`/`dryrun` matching, no `let _ = dry_run;`
discard, and no defaulting. That entire hazard is gone because the parse-time
resolution is the only place that decides. `CleanMode` itself is
`Execute | Simulate | CargoPreview` with **no `Default` impl**
(`src/cleanup.rs:36-46`): the mode must be chosen, and every spelling that reaches
here chose one.

**The one surviving semantic hazard** is upstream, in the flag names. `--dry-run`
is now *simulation* (the project's own full ownership-proof pass, no `cargo clean`
spawned — `src/cleanup.rs:39-41`) and Cargo's own preview is `--cargo-preview`
(`src/cleanup.rs:42-45`). The flags are two hyphens apart in meaning and one word
apart in spelling. clap makes them pairwise exclusive
(`src/cli.rs:107-112`) and the source comments are explicit
(`src/cli.rs:105-112`), but a user typing the wrong one still gets a different
operation, not an error. `--dryrun` and `--yes` survive as hidden aliases
(`src/cli.rs:113-119`) that map onto `Simulate` and the (now default) `Execute`
respectively.

**Root selection** is `resolve_cleanup_roots` (`:266-345`), matching on
`CleanupScope` — three arms, and *no error arm*, because M012A deleted the
`"clean requires ROOT, --known, or --full"` case:

| Scope | Behaviour | Label |
|---|---|---|
| `Root(p)` (`:273-279`) | `vec![cli::absolutize_root(p)]`, no `policy::resolve`, no state read | `"explicit"` |
| `Full` (`:280-304`) | run a Full scan, then read `learned_roots[*].path` | `"full"` |
| `Maintenance` (`:309-343`) | `policy::resolve(ScanRequest{None, false}, &config.scan)` | `"routine"` or `"explicit"` |

Two details in the `Maintenance` arm are worth more than their line count.
First, it does **not** keep only `ScanScope::Routine(roots)` and discard the rest
— it maps every variant (`:317-333`): `Routine` → `"routine"`, `Explicit` and
`ExplicitRoots` → `"explicit"`, and `Global` → a hard
`AppError::Config("maintenance scope resolved to a global scan, which is
unreachable; refusing to treat it as an empty cleanup scope")`. Discarding the
configured root produced a bare maintenance run that reported a successful
Explicit no-op it never performed; treating `Global` as "nothing to do" would fail
open on a scope that covers the whole machine. Both are the right call.

Second, **`state_generation` is read here for every non-`Full` scope**
(`:334-337`) even though only the human branch prints it (`:235-239`). For
`Maintenance` it is `state.last_full_at` from a fresh `load_default()`, so bare
invocation does a second state read that the earlier Full scan also does.

`resolved.roots = collapse_roots(resolved.roots)` runs immediately after
(`:163`), **before** the empty check, so the reported root list, the empty-scope
test, and the roots the engine re-collapses all describe one set.

**Policy** (`:164-181`): `min_reclaimable_bytes` and `min_inactive_seconds`
override cleanly (`Option`-on-scalar, so "unset" is distinguishable from "set
to 0"). `include`/`exclude` are different — `Vec<String>`, and clap cannot
distinguish "no `--include`" from "zero values", so **an empty CLI list falls
back to the configured list** (`:171-180`). Consequence: **`--include`/`--exclude`
cannot clear a configured list.** A user with `[cleanup.policy] exclude = ["**/target"]`
who wants a one-off unfiltered run has no flag for it. Defensible — the empty
list almost always means "I did not mention it", and treating it as "clear"
would be worse in the common case — but undocumented in `--help` and silently
surprising when it bites. A `clear-include` flag pair would fix it without
changing the default. Deliberate choice, not a defect.

**Selector** (`:182-191`): `profile.map(…).or_else(|| package.map(…))` looks
like silent-precedence, but **it is unreachable**. clap already enforces mutual
exclusion: `src/cli.rs:100` gives `profile` `conflicts_with = "package"` and
`:103` gives `package` `conflicts_with = "profile"`; `src/cli.rs:511-513` asserts
the combined forms are `Err`. The `.or_else` is defensive dead code, and a user
supplying both gets a clap usage error and exit 2 — correct, by a different
mechanism than the expression suggests.

**Empty-roots short circuit** (`:192-219`). If `collapse_roots` returns nothing,
the pipeline stops before any Cargo invocation. The `CleanReport` literal
(`:196-210`) names **every field explicitly** — M012A removed the
`..Default::default()` spread, so a new `CleanReport` field is now a compile error
here rather than a silent default. That is a real robustness gain and it is why
the aggregate is still readable: `results`, `diagnostics`, `failed`, `mode`,
`counters`, `scope_blocked`, `unresolved_ownership`, `selected_roots`,
`discovered_manifests`, `resolved_workspaces`, `units_considered`,
`effective_policy`, `selector`. Note `selected_roots: Vec::new()` (`:204`) even
though `resolved.roots` is empty by construction — consistent, not contradictory.

`emit_cleanup` is called with a fixed `empty_note` (`:211-217`) and **`Ok(0)` at
`:218`** — "nothing to do" is success.

**Reporting** (`:220-240`): `show` requires `format == Human` **and** a capable
terminal (`:263-264`) — so progress is disabled for both JSON *and* log mode, which
is what log mode wants; `wall_start` at `:266`; the engine call is `:271-280`;
`renderer.finish_and_clear()` at `:281` clears transient UI **before** the report,
matching the scan ordering. It runs on the **failure** path too: the result is
bound to `cleanup` and `finish_and_clear()` is called before the `?`
(`:281-282`), because `BarState::drop` *finishes* a bar rather than clearing it,
and a `?` that returned first left a retained bar line above the
`cargo-cleanme: …` message. The state-generation `println!` (`:235-239`) is also
Human-only, so it cannot corrupt JSON *or* a log line.

**Exit code** (`:299-303`): `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`failed > 0` means at least one `CleanOutcome::Failed`, counted in
`CleanReport::render`'s tally (`src/cleanup.rs:337-339`) — "ran; at least one
workspace could not be cleaned". `scope_blocked.is_some()` means the scan
boundary itself was refused and no work was attempted; the block is now a typed
`ScopeBlock` (`src/cleanup.rs:211-220`) whose `message` is what renders
(`src/cleanup.rs:288-291`). Pinned by `tests/cli_contract.rs:361-363`
(`Some(1)`, `scope_blocked == true`).

**`--stats`** (`:241-254`) goes to **stderr** via `eprintln!`, consistent with
`src/cli.rs:38-42`, and prints four counter strings plus mode and elapsed:
`stats_line()` (`:248`), `timings_line()` (`:249`), `proof_stats_line()` (`:250`),
`proof_timings_line()` (`:251`). The two `proof_*` lines are the point of the
comment at `:243-244` — "C003 §7.6: cleanup `--stats` must account for the
final ownership-universe proof work, not only the initial scan". The scan's
`--stats` (`:684-689`) has no proof counters, correctly: a scan performs no
proof. So the cleanup line is a superset and the C003 requirement is satisfied
at this call site rather than inside `cleanup.rs`.

**`--stats` is not disabled in log mode.** This is an explicit override, and it is
the only stderr output log mode does not suppress (progress and the diagnostic
fan-out both are). An unattended `--format log --stats` run therefore writes its
line to stdout and the detailed counters to stderr — which is correct, because
`--stats` is opt-in and its documented stream is stderr regardless of format.

## 6. Root resolution and collapse

`collapse_roots` now sits at **`src/main.rs:386-400`**, not at the top of the file
and not inside the cleanup pipeline. Its position matters: `run_cleanup` calls it
exactly once (`:163`), immediately after `resolve_cleanup_roots` returns and before
the empty check, so every later consumer — the empty-roots report, the engine
call, the human `combined roots` line — sees one identical set.

```rust
// collapse_roots — src/main.rs:386-400
let mut roots: Vec<_> = roots.into_iter()
    .map(|p| std::fs::canonicalize(&p).unwrap_or(p)).collect();   // :387-390
roots.sort();                                                    // :391
roots.dedup();                                                   // :392
let mut collapsed: Vec<std::path::PathBuf> = Vec::new();          // :393
for path in roots {
    if !collapsed.iter().any(|parent| path.starts_with(parent)) {  // :395
        collapsed.push(path);
    }
}
collapsed
```

**Canonicalization is best-effort.** `unwrap_or(p)` (`:389`) means a non-existent
or unresolvable path passes through **unchanged and unvalidated**. The
absolute-root invariant the cleanup boundary depends on is established *earlier*,
by `absolutize_root` (`src/cli.rs:334-340`), and only for the explicit-`ROOT`
source. For the other two sources the roots were canonicalized when *learned*
(`src/discovery_state.rs:153`), so in practice they resolve; a root deleted
between learning and cleaning falls through unchanged and is handed to the
cleanup layer as-is.

**The sort is load-bearing, not cosmetic.** `PathBuf` ordering is component-wise
and a parent always sorts before its children (`/a/b` < `/a/b/c`). The
containment filter at `:395` only works because of that: a child is tested only
after its parent has been visited and accepted. Remove the `sort()` and the same
input in reverse order keeps *both* `/a` and `/a/b` — collapse silently stops
collapsing. `reconcile_full` argues the same case about its own sort
(`src/discovery_state.rs:189-192`), so the codebase is internally consistent.
`Path::starts_with` is component-wise, not string-prefix (`/a/bc` does not
`starts_with` `/a/b`), which makes this correct rather than merely plausible.

**Safety property:** after collapse, no accepted root contains another, so
overlapping roots cannot produce overlapping deletion candidates — a single
physical `target/` cannot be reachable through two surviving roots, so it cannot
be counted twice, cleaned twice, or have its ownership universe computed against
two boundaries.

**Do the three sources mix badly?** In practice no: `absolutize_root` yields a
lexically absolute path, learned roots were canonicalized on write, Routine roots
were canonicalized by `policy::canonical_dedup_roots` (`src/policy.rs:124`,
`:134`) and pass through `scan.root` when one is configured. The residual case is
a learned root that has since become a symlink or whose canonicalization now
fails — it passes through `unwrap_or` and may sit in an uncollapsed overlap with
a canonically-spelled sibling. Narrow, low-likelihood; the deeper authorization
in `cleanup.rs` is the real boundary.

**Untested at every level.** `collapse_roots` is private to a file with no inline
tests, and no integration test can call it. The nest-collapse property — the one
thing the function exists for — has no test (§10).

## 7. State reconciliation

`src/main.rs:519-599` — the only genuinely intricate block, and where a
reviewer should spend the most time.

```rust
let loaded_state = discovery_state::load_default();            // :499
if full { … } else if let StateLoad::Loaded(prior) = loaded_state { … }  // :500, :563
```

There is no `state_reconciled` flag. It was removed when the failed-publish
exit-code escalation was fixed; see "A failed publish no longer changes the
exit code" below. Nothing in this block can now influence the process exit
status — the stderr notice is the whole of the consequence.

**Prior selection.** `full_reconciliation_prior` returns
`Option<(DiscoveryState, bool)>` (`src/discovery_state.rs:54-61`): `Loaded(s)` →
`Some((s.clone(), false))`; `Missing` → `Some((default, false))`;
`RecoverableInvalid(_)` → `Some((default, true))` where the `bool` is
`replacing_invalid`; `UnsupportedNewer` and `Unavailable` → `None`. The `None`
case is the important safety choice: **state written by a newer binary, or
simply unreadable, is never overwritten.** The `else` at `:557-562` prints
`cargo-cleanme: {message}; Full state reconciliation was skipped`, where
`message` is `loaded_state.diagnostic()` (`src/discovery_state.rs:33-49`) with a
generic fallback.

**The join** (`:504-513`) maps canonicalized member manifest → workspace root;
**observations** (`:514-523`) apply the same canonicalize-or-raw rule to each
discovered manifest, so the join succeeds whenever both sides are
canonicalizable. `cargo metadata` reports absolute real paths and discovery
manifests come from the walk, so both canonicalize to the same bytes in
practice. When they do not, the failure is a **safe degradation, not a wrong
answer**: `workspace: None` → `ResolutionStatus::ManifestObservedUnresolved` and
the learned-root candidate falls back to `manifest.parent()`
(`src/discovery_state.rs:129-147`) — a *broader* root, not a narrower one.

**`complete`** (`:524-527`) is true unless some diagnostic is `PlatformRoot` +
`Error`, and it gates everything: `reconcile_full` returns
`NoPublication("Full traversal was incomplete")` before touching anything
(`src/discovery_state.rs:122-124`). That is the counterpart of `full_incomplete`
at `:605-609`, which recomputes the *same predicate* for the exit code. Two
identical expressions in one file; they could share a helper.

**Publish** (`:539-556`): on `Reconciliation::Publish(next)`, `publish(&next)`
succeeding may print a "replaced unusable discovery state" notice if
`replacing_invalid` (`:542-546`); failing prints
`cargo-cleanme: discovery state was not saved: {error}` (`:549`) and **changes
nothing else**. `NoPublication(reason)` prints `… was not reconciled: {reason}`
(`:554`).

### A failed publish no longer changes the exit code

`src/main.rs:723-733` is the whole exit decision:

```rust
if full_incomplete {
    return Ok(1);
}
Ok(0)
```

**This was a real defect, and it is fixed.** The block previously read
`if full_incomplete || (full && !state_reconciled) { return Ok(1); }`, so a
failed state publish returned `Ok(1)`: a good scan report on stdout, one stderr
line explaining the state was not saved, and a non-zero exit. That contradicted
`src/discovery_state.rs:338`, directly above the publish implementation, which
states the module's own position: *"State is an optimization only: concurrent
publishers use atomic last-writer-wins replacement."* A read-only or full state
directory — plausible on a locked-down machine, in a container with a read-only
home, or under `chattr +i` — turned a **purely informational scan** into a
non-zero exit for a reason unrelated to what the scan found, and scripts gating
on status reported a scan failure that did not happen.

The `state_reconciled` flag is gone, so the two branches no longer disagree:
the Routine branch's byte-identical "state was not saved" warning (`:577`) and
the Full branch's (`:549`) now both leave the exit code alone. A genuinely
incomplete Full scan still exits `1`, via `full_incomplete`.

**M012A widened what `full` means, so this fix now protects more.** Rootless
`scan` is Full (`:434`), which means the ordinary no-argument scan is the one most
likely to hit an unwritable state directory — and it is now protected by the same
rule the fix established. The cost is on the other side: `run_scan` returning 1
from `clean --full` now escalates to exit 2 (§5), so the *only* way state can
influence an exit code is through that one deliberate call site.

### The Routine branch (`:563-579`)

Matches on `StateLoad::Loaded` specifically, so a Routine scan *silently*
ignores `Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable` — no
diagnostic at all, unlike the Full branch's `else`. It computes
`observed: Vec<_> = workspaces.iter().map(|w| w.root.clone())` (`:565`), then for
each project bumps `last_project_seen_at` on every learned root for which
`project.starts_with(&root.path)` (`:567-573`), and finally publishes only if
`!observed.is_empty()` (`:574-578`), printing `discovery state was not saved:
{error}` to stderr on failure.

**The asymmetry.** This branch only ever *touches* `last_project_seen_at`. It
does not add or remove learned roots, does not update `projects`, does not touch
`last_full_at`. Learning and forgetting happen **only** in Full
(`src/discovery_state.rs:145-218`). That is the correct split — a Routine scan
is a cheap probe over a bounded scope and must not rewrite what it did not
observe — and the `!observed.is_empty()` guard prevents a probe that resolved
nothing from republishing unchanged state.

**Which scans reach it.** Only `scan --known` (`:436` → `full: false`) and
`scan ROOT`. Bare `cargo-cleanme` never reaches `run_scan` at all any more — it is
a cleanup (§3) — so the branch is reached by two spellings of the read-only
inventory rather than by the default invocation. `clean --full` passes through the
Full branch instead.

**`starts_with` is un-canonicalized.** `project` comes from `cargo metadata`;
`root.path` is canonicalized from when it was learned
(`src/discovery_state.rs:153`). A learned root spelled canonically and a
workspace root reached through a symlink fail to match — a **missed touch**, so
`last_project_seen_at` goes stale and the root eventually ages out of
`policy::routine_roots` (`src/policy.rs:112-118`). Self-limiting: the root is
dropped from Routine scope, not deleted, and a later Full scan re-learns it.
`main.rs` canonicalizes deliberately in the Full branch (`:508`, `:517`) and not
here. Small, real, one-sided in severity.

**No duplicate-publication exposure.** `manifests` is sorted and deduped by
discovery before return (`src/discovery.rs:735-738`), so `observations` is 1:1
with distinct manifests and cannot duplicate `ProjectRecord`s; and
`reconcile_full` separately collapses nested learned roots by nearest ancestor
(`src/discovery_state.rs:189-216`). `main.rs` needs no defence against either,
and has none.

## 8. Output selection

`OutputFormat` (`src/cli.rs:5-14`) has **three** variants — `Human` (default),
`Json`, and `Log` — and is a **global** flag (`src/cli.rs:45`), so it may appear
before or after the subcommand. `Log` is M012B's addition and is not a machine
contract; see [10-reporting §7](10-reporting.md).

| Path | Human | JSON | Log |
|---|---|---|---|
| scan | `report::render(&mut report)` `:650` | `to_string(output::scan(&report, scope))` `:635-643` | `output::log::scan(&report, scope, full_incomplete)` `:644-648` |
| cleanup | `println!("combined roots …\n{}", report.render())` `:371-380` | `to_string(output::cleanup(&report, scope, gen))` `:358-365` | `output::log::cleanup(&report, scope)` `:366-370` |
| cleanup, empty roots | the fixed note `:382` | the all-fields-explicit report `:196-217` | same call, via `emit_cleanup` `:211-217` |
| update | three literal shapes `:89-99` | `update_json(&plan, dry_run)` `:87-88` | **falls through to human** |
| config | `println!` only | **not honoured** | **not honoured** |
| fatal error | `cargo-cleanme: {e}` `:31` | none — stderr + exit 2 `:33` | `output::log::fatal(op, code)` `:26-29` |

**The third column has two holes worth naming.** `update --format log` silently
produces human output, because `run_update` (`:85-101`) tests only
`format == OutputFormat::Json` and takes everything else down the human path. A
scheduler running `update` with `--format log` gets three-to-four lines, not one,
and no error. `config` ignores `format` entirely (a pre-existing gap, §3). Neither
is load-bearing for unattended cleanup — the surfaces a scheduler actually drives
are scan and clean, both of which are covered — but "log mode" is not a property
of the *binary*, it is a property of the two report-producing operations.

### The `scope` label is derived from the resolved policy, not the flags

`scope_label` (`:410-416`) matches on `ScanScope` — `Global` → `"full"`,
`Explicit`/`ExplicitRoots` → `"explicit"`, `Routine` → `"routine"` — and is called
with `policy.scope` (`:640`, `:647`), which `policy::resolve` returned at `:438`.

This was a real contract bug and it is fixed. The previous code derived the label
from `full` and `root.is_some()`; mechanism is in
[10-reporting §9](10-reporting.md) ("Can `scope` be wrong?"). The residual note is that `scan` and
`cleanup` now share one label vocabulary — `full`, `explicit`, `routine` — where
before cleanup also emitted `known`.

### `state generation last_full_at=` goes to stdout

`src/main.rs:235-239` uses `println!` — **stdout**, not stderr — guarded to human
format (`:235`), so it cannot corrupt JSON *or* a log line. But it is
machine-adjacent metadata interleaved into a human report, on exactly the stream
that pipelines capture. The project's stated discipline is stdout = deterministic
report, stderr = transient/diagnostic. This line is neither. It belongs on stderr
beside `--stats`, or in the JSON envelope as a field — `output::cleanup` already
accepts a `state_generation` argument (`src/output.rs:161`), and `log::cleanup`
deliberately does not, so a log consumer cannot recover it at all.

### Serialization failure

Two sites, inconsistent: `:642` and `:364` map to
`AppError::Config(format!("cannot serialize JSON report: {e}"))`, which is the
wrong *variant* — the user sees `cargo-cleanme: configuration error: cannot
serialize JSON report: …`, sending them to `config.toml` when the fault is in
report serialization (it does at least match the existing habit: `config::show`
does the same at `src/config.rs:370-371`). **M012A removed the third site.** The old
empty-roots cleanup branch used `AppError::Config(e.to_string())` with no
descriptive prefix; the branch now goes through `emit_cleanup` like every other
cleanup report (`:211-217`), so the prefix inconsistency is gone with it.

In practice `serde_json::to_string` on these DTOs cannot fail — `EnvelopeV1` is
owned primitives, strings and `Option`s, with no map keys and no `f64`
NaN/Infinity — so this is a defensive branch that is effectively dead. The real
risk is that the *human* and *log* paths have no equivalent guard.

### `update_json` — the unversioned, untested surface

`update_json` (`:715-737`) hand-builds a `serde_json::Map` with
`schema_version: 1` and `operation: "update"` (`:717`, `:722`). The discipline is
deliberate (the doc comment at `:713-714` says so) but is not shared with
`output.rs`'s `EnvelopeV1` — it is a second, hand-maintained envelope. With the
§10 finding that this file has **zero** inline tests, `update_json` is the only
machine-readable output surface with **no test coverage of any kind**: no unit
test (no module exists) and no integration test (nothing in `tests/` invokes
`update`). A renamed `UpdatePlan` field or a changed key name fails nothing.

### Ordering divergence between the two cleanup projections

**Confirmed.** `CleanReport::render` takes `&self` (`src/cleanup.rs:275`) and
therefore **cannot** reorder its own data: it builds a local
`ordered: Vec<&CleanResult>` (`src/cleanup.rs:303`), sorts by `before_bytes`
descending with a `display_path` ascending tie-break (`src/cleanup.rs:304-309`),
and iterates *that* (`:310`). `output::cleanup` takes `&CleanReport`
(`src/output.rs:158`) and iterates `report.results` in **original insertion order**
(`src/output.rs:164-189`) with no sort at all.

So JSON `units[]` and the human unit rows are in different orders whenever
insertion order is not already size-descending, and a consumer matching the two
positionally or diffing them sees spurious differences. The paths never
co-execute (`emit_cleanup` branches at `:358`/`:371`), so no test can observe it,
and `tests/cli_contract.rs` only ever indexes `units[0]` — order-independent in
its fixtures.

The **scan** side is the opposite and self-consistent: `report::render` takes
`&mut ScanReport` and sorts `report.groups` **in place**
(`src/report.rs:45-49`), which `run_scan` relies on by passing `&mut report`
(`:650`). The asymmetry between the two `render` signatures — `&self` + local copy
for cleanup, `&mut self` + in-place sort for scan — is the root of the finding.

## 9. Invariants and edge cases

**Is the exit code consistent? Mostly.** 1 = "ran, partially failed",
2 = "could not run". Two known outliers: `config edit` is the deliberate one,
laundering the editor's own status (`:125`, pinned by
`tests/cli_contract.rs:724`); and `update` cannot express partial failure at all.
M012A added a third, also deliberate: `clean --full` on an incomplete scan is 2
(`:282-291`).

**Can a command return 0 while having printed to stderr? Yes, routinely.**

| stderr message | Line | Exit effect |
|---|---:|---|
| `N filesystem diagnostics; rerun with a bounded root if needed` (+ ≤10 details) | `:656-677` | **none** — a scan full of `PlatformRoot` errors still exits 0 |
| `scan stats: …` | `:684-689` | none (opt-in) |
| `cleanup stats: …` | `:245-253` | none (opt-in) |
| `discovery state was not saved: {error}` — **Routine** | `:577` | **none** |
| `{warning}; using seed/configured Routine roots` (from policy) | `src/policy.rs:91` | none |
| `replaced unusable discovery state after successful Full reconciliation` | `:543-545` | none (a *success* notice) |
| `discovery state was not reconciled: {reason}` | `:554` | **none** (fixed; see §7) |
| `discovery state was not saved: {error}` — **Full** | `:549` | **none** (fixed; see §7) |
| `{message}; Full state reconciliation was skipped` | `:561` | **none** |
| `editor exited with {status}; …` | `:121-124` | **→ editor's code** |
| `cannot launch editor {program}: {e}` | `:117-119` | → 2 |

The state-warning rows were **fixed**, not left as a finding: the Full branch used
to escalate both to exit 1 while the Routine branch ignored the byte-identical
message. Both are now inert, which is why the "sharp inconsistency" the earlier
revision described no longer exists here. The remaining asymmetry is cosmetic —
the Full branch explains itself on stderr while the Routine branch is silent about
a load problem it never reports.

**Can `run_scan` be reached twice in one process?** No. The `Invocation` arms are
mutually exclusive, and the Cleanup arm is the only caller of `run_scan` from
inside `run`, calling it once (`:281`). So the "re-scoped totals" comment at
`:585-588` describes the phase sequence *within* one call, not a second call.
There is no double-initialised renderer, no global state, no `static mut` in the
file — the renderer is a plain local (`:265` for cleanup, `:519` for scan) whose
`finish_and_clear()` runs once (`:281`, `:700`) before the report, failure path
included.

**Is `checked_sub` the only overflow guard? Yes, in this file.** `scan_start`
(`:426`) is used unguarded at `:597` and guarded at `:590` (§4).

**Config directory read-only.** Only *creation* fails. `load_or_create` (`:428`,
`:157`) calls `create_initial`, which does `fs::create_dir_all(parent)?`
(`src/config.rs:259-262`) → `AppError::Io` via `#[from]` (`src/error.rs:9`) →
exit 2. If the file exists and is readable, `load` performs no writes and
everything works. Scope: `scan`, bare invocation, `clean …`, `config show` and
`config edit` all exit 2; `config path` works (resolves without touching the FS,
`:105`) and `update` works (never loads config, `:86`). `--config /other/path.toml`
is the documented escape hatch (`src/config.rs:80-82`).

**Can `process::exit` truncate buffered stdout? No.** `main` calls
`std::process::exit` at `:19` and `:33`, which does skip destructors — but
nothing is buffered waiting for them. Rust's `std::io::Stdout` is wrapped in a
`LineWriter`, so it flushes on every `\n` regardless of whether the target is a
TTY or a pipe. Every output statement here is a `println!`/`eprintln!` ending in a
newline (`:26-31`, `:90-99`, `:105`, `:107`, `:121-124`, `:138`, `:211-217`,
`:235-239`, `:245-253`, `:358-380`, `:636-648`, `:656-677`, `:684-697`). The
C-stdio block-until-`exit` failure mode does not apply to Rust, and the
one-newline assertions at `tests/cli_contract.rs:204`/`:335` are consistent with
nothing being lost. The only theoretical exposure is a *partial* final line with no
trailing newline, which this file never produces. **Definitive: piped JSON output
cannot be truncated by `process::exit`.** The corollary is the actual risk: safety
comes from `LineWriter` plus newline-terminated writes, not from an explicit
flush, so adding a buffered writer or a non-`println!` path would remove it
silently.

**What the safety-critical code does *not* do.** This file cannot delete anything.
It resolves roots, merges policy, chooses a mode, calls the engine, and renders.
Every destructive decision — ownership proof, authorization, recency, and the
`cargo clean` invocation itself — happens inside `cleanup.rs` behind
`clean_with_roots_policy_selector` (`:224-233`). The most consequential thing
`main.rs` does is *choose what to pass*: a wrong root, a wrong mode, or a wrong
`selected_roots` label is a correctness defect here and a safety defect only if
`cleanup.rs` trusts it, which it does not (it re-proves every candidate). That
separation is the reason this file can be 737 lines of untested composition.

## 10. Testing

**`src/main.rs` has zero inline tests.** Verified:
`grep -cE '#\[cfg(test)\]|#\[test\]' src/main.rs` → `0`. There is no
`#[cfg(test)] mod tests` in the file; the `use` statements at `:423-424` and
`:498` are ordinary function-scoped production imports inside `run_scan` — a
common source of the opposite impression. The same grep on the two output
modules: `src/main.rs` **0**, `src/output.rs` **12** (all inside `pub mod log`,
`src/output.rs:426-671`), and `src/report.rs` **8** — it has an inline suite
(`src/report.rs:72-195`). See [10-reporting §10](10-reporting.md) for the log
tests and what they leave uncovered.

**Consequently `update_json` (`:715-737`) is entirely untested** — no unit test
(no module exists) and no integration test (nothing in `tests/` invokes
`update`), and it is the only machine-readable surface hand-building its
envelope outside `output.rs`'s `EnvelopeV1` (§8).

### `tests/cli_contract.rs` — 1711 lines, 29 tests

Drives `env!("CARGO_BIN_EXE_cargo-cleanme")` as a subprocess and asserts on
status, stdout and stderr. The pre-M012A tests survived; M012A and M012B added
twenty-one, and they are the ones that cover the behaviour this file changed.
Verified line numbers:

| Test | Line | What it pins about `main.rs` |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 80 | `cargo cleanme --version` (external) byte-identical to direct — i.e. the argv normalization in `parse_normalized` (`:14`) is correct; plus a premise guard that the staged binary is the one Cargo resolves |
| `cargo_external_subcommand_help_matches_direct_help` | 120 | `--help` exits 0, identical both forms |
| `cargo_external_config_edit_help_matches_direct` | 154 | `config edit --help` exits 0, identical both forms |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 177 | `schema_version == 1` (`:200`), `operation == "scan"` (`:201`), `scope == "explicit"` (`:202`); **`plain.stdout == with_stats.stdout`** (`:198`) — `--stats` cannot perturb stdout; stderr contains `scan stats:` (`:203`); **exactly one `\n`** (`:204`) |
| `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` | 219 | a **configured** `scan.root` under `scan --known` reports `"explicit"` (`:249-252`) — the regression for the flag-derived label (§8) |
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 268 | parser-level rejection, asserted on clap's conflict error rather than "the command failed" |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 300 | `schema_version == 1` (`:331`), `operation == "clean"` (`:332`), `mode == "simulate"` (`:333` — `--dryrun` → `CleanMode::Simulate`), `summary.simulated == 0` (`:334`), one `\n` (`:335`) |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 339 | **`status.code() == Some(1)`** (`:361`) with `result.scope_blocked == true` (`:363`) — pins the `:255-259` exit rule |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 379 (`#[cfg(unix)]`) | six invocations against a `/bin/sh` cargo stub: `summary.cleaned == 1` (`:472`), `units[0].reason_code == "cleaned"` (`:473`), `selector_kind/value == "profile"/"dev"` (`:474-475`); `before_bytes` and `selector_estimate_bytes` both `null` under a selector while `output_union_before_bytes >= 4096` (`:476-489`); `units[0].outcome == "simulated"` under `--dryrun` (`:520`); `reason_code == "selector_unsupported"` + `policy_disposition == "selector_estimate_unavailable"` (`:552-559`); `selector_kind == "package"` with `fixture@0.1.0` (`:659-661`). Also asserts the stub's call count after each run |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 676 | **`status.code() == Some(17)`** (`:724`) — the passthrough at `:125`; an invalid edit exits non-zero with `is invalid` in stderr (`:727`) and leaves the bad content in place (`:728`) |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` | 856 | **the M012A default.** `operation == "clean"` (`:870`), `units` is an array where a scan would have `groups` (`:874`), `mode == "execute"` (`:877`), `scope == "explicit"` (`:880`), and — the discriminating assertion — `clean_calls() == 1` with the artifact gone (`:885-886`) |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` | 893 | `mode == "simulate"` (`:904`), `simulated == 1 && cleaned == 0` (`:906-907`), **`clean_calls() == 0`** (`:911-915`), artifact still present (`:916-919`), and Cargo *was* used for resolution (`:922`) — so this is a simulation, not a skip |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` | 935 | exit 0, `scope == "routine"` (`:959`), `mode == "execute"` (`:960`), `units == []` (`:962`) — the `:192-219` short circuit through the binary |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` | 969 | blocked bare invocation, exit 1 (`:980`), no `cargo clean` |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` | 1002 | `clean ROOT` with no mode flag executes (`:1015-1019`); `--cargo-preview` gives `mode == "preview"` (`:1028`) |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` | 1060 | `--dryrun`/`--yes` map onto `Simulate`/default `Execute` |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` | 1098 | `scan --known` is `routine` with no configured root, `explicit` with one |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 1155 | the bounded-zero-result scan document |
| `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` … `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 1258-1711 | the eleven M012B log cases, via a shared `assert_bounded_log_line` (`:1218`): one line, bounded, silent stderr; `simulate` vs `preview` distinguishable; blocked carries a typed reason and exit 1 (`:1325`); failed cleanup is `status=failed` (`:1353`); zero-result maintenance is `status=ok` (`:1392`); scan reports the resolved scope (`:1418`); `--stats` stays opt-in on stderr and leaves the one line alone (`:1486`); the human diagnostic fan-out is suppressed (`:1536`); a fatal error is **one bounded stderr line and empty stdout**, exit 2 (`:1605`, `:1619-1642`), carrying a typed code and *not* the config path or parse error (`:1639-1642`); JSON and human are unchanged by log mode's existence (`:1648`); an unknown format still fails closed (`:1693`) |

### `tests/end_to_end.rs` — 347 lines, 2 tests

`end_to_end_reports_only_inactive_artifact_projects` (`:114`) does *not* invoke
the binary; it drives the library directly with a fake `CargoRunner` (`:75`) through
`policy::resolve` → `discovery` → `resolve_workspaces` → `build_groups` →
`analyze_groups` → `report::render`, reproducing `run_scan`'s sequence by hand. Its
JSON assertions: `result.groups[0].ownership == "private"` (`:203`),
`result.summary.group_count == 1` (`:204`), `bytes >= 8192` (`:209`), and
human/machine agreement (`:210`).
`a_literal_ignored_ancestor_yields_one_workspace_while_an_explicit_root_yields_two`
(`:285`) drives the same helper (`:216`) over two scopes and asserts the
ignore/unignore literal-path behaviour end to end. **Still no cleanup projection
assertions** — every end-to-end case here is scan-side.

### `tests/common/mod.rs` — 229 lines, not a test target

(Cargo compiles only top-level `tests/*.rs`; each suite pulls it in with
`mod common;`.) Five fixture helpers: `set_path_modified` (`:24`), `backdate_file`
(`:98`), `inactive_project` (`:111`), `write_fake_cargo` (`:167`), and `cargo_calls`
(`:223`). It asserts nothing about the binary.

### What is well pinned, and what is not

**Well pinned — the machine contract, the new defaults, and log mode.** Argument
parsing in both invocation forms; the JSON envelope's
`schema_version` / `operation` / `scope` / `mode` by name, value and type; the
per-unit `reason_code`, `policy_disposition`, `outcome`, `selector_kind`,
`selector_value`; the `before_bytes` vs `output_union_before_bytes` distinction
under a selector; the exit-1 rule for `scope_blocked`; the editor's exit-code
passthrough and its three post-edit checks; one trailing newline; stdout/stderr
separation for `--stats`; **and, new in M012A/M012B, the behaviour rather than the
label** — bare invocation really does invoke Cargo and really does remove bytes,
and bare `--dry-run` really does invoke nothing. That last distinction is the one
the earlier revision of this document called untested; it is now the best-covered
thing in the file.

The key structural observation survives unchanged: **`output.rs`'s DTO layer has no
unit tests, yet its output is pinned by name, value and type from the outside.**
A subprocess test asserts the real serialised bytes, which is defensible — but the
coverage is *incidental* to the assertions someone happened to write, not
systematic. A field nobody wrote an assertion for has no protection at all, which
is exactly how the `scope`-label bug survived a suite that *did* assert `scope`
(§8).

**Only incidentally covered, or not at all:**

| Orchestration decision | Coverage |
|---|---|
| one JSON document on stdout for `clean --full` | **restored but untested** (§4). `EmitReport::No` suppresses the intervening scan report, and the bug it fixes was found by review rather than by a test, because no case runs `clean --full` at any format. The flag is deliberately named and documented so the next reader does not mistake it for dead code |
| `clean --full` fail-closed on an incomplete scan (`:282-291`) | **none** — needs a state file with learned roots *and* an unreadable platform root |
| Exit code when a **Full** publish fails (`:548-550`) | **none** — needs an unwritable state directory |
| Routine-vs-Full divergence of the *same* message | **none** — both are now inert, so the divergence is unobservable *and* untested |
| `--stats` **content** | partial — `cli_contract.rs:203` greps `scan stats:`. The four counter strings on the cleanup line (`:248-251`) are never asserted |
| `collapse_roots` | **none** — private to this untested file, so the nest-collapse safety property is untested at every level |
| Root-source precedence | only via clap's conflict declarations (`src/cli.rs:66-86`) and the `invocation()` unit tests — the parse-level guarantee plus a pure mapping, but not the runtime `resolve_cleanup_roots` selection |
| `state generation last_full_at=` on stdout | **none** — no test runs cleanup with a `Some` generation in human format |
| Human vs JSON cleanup **ordering** | **none** — never co-executed; every assertion indexes `units[0]` |
| `update --format log` falling through to human | **none** — and it *is* a gap (§8), not just an untested branch |
| Read-only config directory | **none** |
| `update` / `update_json` | **none** |

**Honest assessment.** `main.rs` is the composition root — the only place the
two pipelines are wired — and composition is precisely what unit tests with fakes
do not cover. What M012A did well is push the *decisions* out to where they can be
tested: `Cli::invocation` is now a pure function with its own unit tests in
`src/cli.rs`, and the M012A behaviours are pinned through the binary by name. What
is still un-testable in place is the *wiring*: which arm a given invocation reaches,
and what `run_scan` writes when called from inside `resolve_cleanup_roots`.
`end_to_end.rs` re-implements `run_scan`'s call sequence in the test body, which
is itself a structural risk: the sequence can drift from `main.rs` and the test
stays green. It has already drifted once, per its own doc comment (`:6-9`).
`main.rs` itself remains untested, and the decisions that belong to it rather than
to a module are the ones with no coverage.

## 11. Review checklist

1. **`clean --full` now writes the intervening scan report to stdout.** With
   a suppressed internal scan report (§4), a `clean --full --format json` could
   emit **two** JSON documents: the Full scan's, then the cleanup's. That is what
   happened once. The one-document-per-invocation
   property that `tests/cli_contract.rs:204` and `:335` assert elsewhere is not
   asserted here, and no test runs `clean --full` at any format (§10). This is the
   highest-value item on the list: either confirm it is intended, or reinstate a
   narrower suppression for the `resolve_cleanup_roots` call site only
   (`main.rs:281`).
2. **Exit-code split for incomplete cleanup scope.** `main.rs:282-291` raises
   `AppError::Config` → exit 2 for an incomplete Full scan, while
   `main.rs:255-259` returns 1 for a scope blocked during cleanup. Line 27 of
   `plans/output-schema-v1.md` assigns both to 1. Confirm the plan document
   records the move, and that no consumer treats 2 as "the tool is broken".
3. **Bare invocation deletes bytes.** `src/cli.rs:253-257` makes a no-subcommand
   invocation a Routine Execute cleanup with no prompt. The safety argument is
   that `cleanup.rs` re-proves every candidate and deletion is always
   `cargo clean`, not that the default is conservative. Confirm that reading, and
   that `src/cli.rs:22-30` (the `about` text) stays accurate — it is the only
   place a user is told.
4. **`update --format log` silently emits human output.** `main.rs:87-99` tests
   only `OutputFormat::Json`; `Log` falls through to the multi-line human path.
   Either add the branch or make the flag error, because "log mode" currently
   means different things for different subcommands.
5. **`process::exit` and buffered stdout.** `main.rs:19`/`:33` skip destructors;
   safety comes from `Stdout` being a `LineWriter` plus newline-terminated
   writes, *not* from an explicit flush. A new buffered writer, a `write!`, or a
   non-newline-terminated final line removes that property silently — and the log
   mode's bounded-line contract (`src/output.rs:397-424`) is newline-sensitive too.
6. **`ScopeBlock.reason` is dropped by every report path except log.**
   `main.rs` passes `resolved.scope_label` to `output::cleanup`, which projects
   only `.message` (`src/output.rs:216`). The typed code survives only in the log
   line (`src/output.rs:352`). A schema-version decision, not a patch — see
   [10-reporting §11](10-reporting.md) item 12.
7. **stdout/stderr interleaving.** `main.rs:235-239` puts
   `state generation last_full_at=` on stdout, against the project's own rule
   (stdout = report, stderr = diagnostic). Human-only today (`:235` gates on
   `Human`), and log mode cannot see the generation at all; a landmine if `:235`
   is ever relaxed.
8. **Serialization errors use the wrong variant.** `main.rs:659` and `:364`
   produce `configuration error: cannot serialize JSON report: …`, sending the
   user to `config.toml` when the fault is in report serialization
   (`src/config.rs:370-371` does the same). The third site that omitted the prefix
   was removed with the old empty-roots branch (§8), so this is now the only
   inconsistency left.
9. **Redundant root echo.** `main.rs:373-379` and `src/cleanup.rs:277-287` print
   the same root list, in two formats, as the first two lines of a human `clean`
   report (`selected_roots` is `roots.clone()`, `src/cleanup.rs:855`/`:896`).
10. **Human/JSON cleanup ordering divergence.** `src/cleanup.rs:303-310` sorts a
    *local* vec that `output::cleanup` (`src/output.rs:164-189`) never sees, so
    JSON `units[]` is insertion-ordered while human rows are size-descending. The
    scan side is self-consistent (`src/report.rs:44-49` sorts in place). Confirm
    the projections are not expected to agree positionally.
11. **`--include`/`--exclude` cannot clear a configured list.** `main.rs:171-180`
    falls back to config on an empty CLI vec. Deliberate but undocumented; clearing
    needs a distinct flag, not a reinterpretation of the empty list.
12. **`collapse_roots` canonicalization is best-effort.** `main.rs:395` uses
    `unwrap_or(p)`, so a missing or unresolvable root passes through unvalidated.
    Confirm the cleanup layer is the only thing between that and a deletion
    boundary, and that the `sort()` at `main.rs:397` is understood to be
    *required* by the containment filter at `:401`.
13. **Explicit `ROOT` skips `policy::resolve`.** `main.rs:273-279` never calls it
    (only `Maintenance` does, `:310-316`), so the exists/is-a-directory/
    is-not-a-symlink checks at `src/policy.rs:32-41` never run for `clean ROOT` — a
    promise `src/cli.rs:330-333` makes while naming a different layer.
14. **`main.rs` has no tests; `update_json` is uncovered.** Zero `#[cfg(test)]`
    here; 12 in `output.rs` (all in `log`); 8 in `report.rs`. The machine contract
    *is* pinned by `tests/cli_contract.rs` (§10), so the gap is specifically the
    *orchestration* decisions — which arm a given invocation reaches, and what the
    `clean --full` scan writes — plus `update_json`, the one hand-rolled envelope
    with no test of any kind.
