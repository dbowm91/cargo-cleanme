# Orchestration — argument dispatch and the two pipelines

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** re-derived against `src/main.rs` at 1562 lines. The substantive
> changes since the previous revision of this file are: **ADR 007** made the
> proof boundary the *selected scope*, so an out-of-boundary unprovable is
> reported and never blocks; **M014** added the CWD-anchored default, `scan
> --all`, and the `-i` selection surface; **M017** removed the island machinery
> this document's predecessor still described; **ADR 003** made bare
> `cargo cleanme` a Routine *execute* cleanup and `--dry-run` the zero-spawn
> simulation preview; and `config`/`update` learned `--format log` while
> `config --format json` became a hard rejection. Every citation below was
> re-read at the cited line; none was shifted arithmetically.

`src/main.rs`, 1562 lines, four inline tests. The only module that reaches every
other one, and the only place the two pipelines are stitched together.

## 1. Responsibility

`main.rs` owns argv parsing and the exit-code contract (`:11-37`), collaborator
wiring, the reconciliation branch deciding whether persisted state is rewritten
(`:1130-1215`), and the choice of output surface. It owns **no domain logic**:
scope to `policy::resolve` (`:1045-1051`, `:773-779`), classification to
`policy::classify_cleanup_roots` (`:732-735`), authorization and mode semantics
to `cleanup::clean_with_roots_policy_selector_guarded` (`:635-646`), and
traversal completeness to `ManifestDiscovery::coverage_complete` (`:1083`),
whose one value feeds both the Full reconciliation (`:1154`) and the exit status
(`:1249`).

**It declares five types.** `RunOptions` (`:63-69`) is the process-wide triple
`no_progress` / `stats` / `format`, built once in `run` (`:72-76`) and passed to
both report-producing operations. `ResolvedCleanup` (`:297-316`) carries the
resolved roots, the machine scope label, the discovery-state generation, the
remembered omissions, a typed whole-scope block, and the `automatic` flag that
decides whether a fatal `InvalidRoot` may be reclassified rather than
propagated. `SelectionOutcome` (`:490-500`) is the two-way result of the `-i`
step. `EmitReport` (`:997-1008`) is the two-valued switch that keeps an internal
scan off stdout. `ScanOutcome` (`:1010-1022`) carries the exit code plus the
proven Full generation and whether its publication survived. All exist to make
the "one resolved intent per invocation" property structural: because
`Cli::invocation` collapses every cleanup spelling into one `CleanupRequest`
(`src/cli.rs:382-399`), there is exactly one place where mode, scope, and
policy are bound, and one place that emits the report.

**It has no inline tests**: `grep -cE '#\[cfg(test)\]|#\[test\]' src/main.rs`
→ `0` (see §10).

Its top `use` block is ten lines (`:1-10`) and names `cli` (with `CleanupRequest`,
`CleanupScope`, `Cli`, `ConfigCommand`, `Invocation`, `OutputFormat`,
`ScanIntent`), `config` (with `ConfigPathResolver`), `domain` (`ScanScope`),
`error` (`AppError`), and `std::collections::BTreeMap`. Four function-scoped
imports live inside `run_scan` — `use cargo_cleanme::{domain, progress};`
(`:1030`) and `use std::time::SystemTime;` (`:1031`), plus
`use cargo_cleanme::discovery_state::StateLoad;` (`:1121`) — and everything
else is named at the point of use (`cargo_cleanme::update::update` `:94`,
`cargo_cleanme::workspace::SystemCargoRunner` `:1111`,
`cargo_cleanme::progress::IndicatifRenderer` `:442`). Consequence: renaming a
library function breaks this file at many scattered sites, and there is no
single list of what it depends on. Modules reached: `cli`, `config`, `error`,
`domain`, `discovery`, `workspace`, `discovery_state`, `cleanup`, `update`,
`editor`, `interactive`, `progress`, `report`, `output`, plus `serde_json`
(`:110`, `:915`) and `directories` (`:825`).

**Fifteen free functions, three structs and two enums**, all private:

| Item | Lines | Role |
|---|---:|---|
| `main` | `11-37` | parse, dispatch, fatal rendering, `process::exit` |
| `operation_name` | `41-50` | `Invocation` → stable `op=` token for the log fatal line |
| `error_code` | `54-62` | `AppError` → stable `reason=` code |
| `run` | `70-91` | config path, `RunOptions`, five-arm dispatch |
| `run_update` | `93-219` | plan, JSON / dry-run four-liner / one-line success |
| `run_config` | `221-295` | `path`, `show`, `edit` + three post-edit checks |
| `run_cleanup` | `318-488` | the cleanup pipeline (§5) |
| `resolve_candidate_selection` | `502-610` | the `-i` step and the anchor resolution (§5) |
| `run_engine_guarded` | `623-714` | admission→validation race defence (C028, §5) |
| `resolve_cleanup_roots` | `720-855` | scope → admitted roots + label + generation (§5) |
| `admit_automatic` | `861-899` | provenance admission before collapse (C028, §5) |
| `emit_cleanup` | `905-947` | five-branch format dispatch for a cleanup report |
| `collapse_roots` | `965-979` | canonicalize, sort, dedupe, nest-collapse (§6) |
| `scope_label` | `989-995` | `ScanScope` → JSON `scope` string |
| `run_scan` | `1024-1424` | the scan pipeline (§4, §7) |
| `RunOptions` | `63-69` | process-wide output options |
| `ResolvedCleanup` | `297-316` | admitted roots + label + generation + block |
| `SelectionOutcome` | `490-500` | `-i` outcome: chosen, cancelled, or nothing to clean |
| `EmitReport` | `997-1008` | whether a scan writes its report to stdout |
| `ScanOutcome` | `1010-1022` | exit code plus the proven Full generation (C028, §7) |

The two log helpers — `operation_name` and `error_code` — are new in M012B and
exist only to serve `main`'s fatal path. They are why `invocation` is captured
before the move into `run` (`:17`), and `format` after it (`:18`). Neither helper
is reachable from any other function.

## 2. Entry and exit

```rust
fn main() {                                                   // :11-37
    let cli = Cli::parse_normalized();                        // :15
    let invocation = cli.invocation();                        // :17  (captured pre-move)
    let format = cli.format;
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if format == OutputFormat::Log {
                eprintln!("{}", output::log::fatal(operation_name(&invocation),
                                                     error_code(&e)));   // :27-30
            } else {
                eprintln!("cargo-cleanme: {e}");                          // :32
            }
            std::process::exit(2)
        }
    }
}
```

**`main` now parses argv itself** (`:15`), and `run` receives an already-parsed
`Cli`. This is M012B's consequence, not a style choice: a fatal error can only be
rendered format-aware if the format is known before the operation runs, and a
`try_parse` buried inside `run` could not tell "clap rejected this" from "the
filesystem is broken". The price is two values captured purely for the failure
path — `invocation` at `:17`, before the move into `run`, and `format` at `:18`.

`Err(e)` prints one line on **stderr** and exits **2**; every `AppError` variant
(`src/error.rs:3-16`) exits 2. A command can also return `Ok(1)`. **Exit 1 means
"ran, and something failed"; exit 2 means "could not run."** The only `Ok(1)`
producers: `:274` (editor failed), `:384` (automatic-scope block), `:483-487`
(cleanup partial failure or scope block), `:1412-1418` (scan Full incomplete).

| Code | Meaning | Produced by |
|---:|---|---|
| 0 | Success | `:1419-1423`; Update `:141`, `:182`, `:218`; Config `:294`; empty-roots cleanup `:415`; cleanup `:483-487` when `failed == 0 && scope_blocked.is_none()` |
| 1 | Ran, partially failed | editor failure `:274`; automatic-scope block `:384`; cleanup `:483-487`; scan `:1412-1418` |
| 2 | Could not run | any `AppError` via `main` `:34`; clap usage errors (clap's own path) |
| 0 (clap) | `--help` / `--version` | clap, not this file |

| Command | Codes | Decided at |
|---|---|---|
| bare `cargo-cleanme` / `clean` / `clean --known` | 0, 1, 2 | `:483-487` |
| `scan ROOT` / `scan --known` / `scan --all` | 0, 1, 2 | `:1412-1423` |
| `clean ROOT …` | 0, 1, 2 | `:483-487` |
| `clean --full …` | 0, **2**, 1, 2 | `:764-766` then `:483-487` |
| `config path` / `show` | 0, 2 | `:294` |
| `config edit` | 0, 1, 2 | `:256-292` |
| `update [--dry-run]` | 0, 2 | `:93-219` |
| any (clap usage error) | 0 (help/version), 2 | clap |

**The `clean` row has no "no root source" case any more.** Before M012A, `clean`
with no `ROOT`, no `--known` and no `--full` was a usage error
(`AppError::Config("clean requires ROOT, --known, or --full")`). It is now the
maintenance scope — the same intent as bare invocation — so there is no user-facing
way to reach that 2.

**`clean --full` moved a failure from 1 to 2.** A Full scan that does not fully
complete now raises `AppError::Config` from
`policy::full_cleanup_roots` (`src/policy.rs:444-449`) → exit 2, instead of
returning the scan's own code silently. The old behaviour handed the caller a
bare code with no indication that *no cleanup ran*; the new one says so. The
trade-off is real: "incomplete cleanup scope" now splits across two codes
depending on whether the incompleteness was detected during scan (2) or during
unit execution (1). Both are fail-closed.

`update` can never return 1. A benign no-op — `UpdateError::AlreadyCurrent`
or `UpdateError::NewerThanPublished` — is reported in prose and exits **0**
(`:141`, `:182`), so schedulers do not alert on "already up to date", matching
`docs/TROUBLESHOOTING.md` calling that state success. Every other
`UpdateError` still converts via `From<UpdateError> for AppError`
(`src/update.rs:355`) to `AppError::Provenance` or `AppError::Update`,
both exit 2. Before M012B the benign variants shared that exit-2 path, which is
what the previous revision of this file recorded as a defect.

### Clap's errors bypass `AppError` entirely

`Cli::parse_normalized()` (`:15`) = `Self::parse_from(normalize_cargo_argv(args_os))`
(`src/cli.rs:300-302`), and unlike `try_parse_from` it **cannot return `Err`**:
on a parse error clap prints its message plus usage and calls its own
`process::exit`. So for a bad flag, unknown subcommand, violated
`conflicts_with`, or a missing `scan` scope selector, the `AppError` branch at
`:21-35` is never reached, no `cargo-cleanme: ` prefix is printed, and the exit
code is **2** (clap 4.5's `Error::exit`; `Cargo.toml:51` pins `clap = "4.5"`).
`--help`/`--version` exit **0**.

**Definitive:** `cargo cleanme clean /x --dry-run --cargo-preview` shows a clap
usage error and exit 2 — indistinguishable by status from a fatal `AppError`. A
script must match the message prefix to separate "you mistyped" from "the
filesystem is broken". This is now *harder* to write than before, because the log
fatal line is also one bare stderr line with no `cargo-cleanme:` prefix: a
log-mode consumer distinguishing the two must check for `status=error`.

## 3. Command dispatch

`run` matches on **`Invocation`**, not on `Option<Command>` (`:80-90`):

```rust
match cli.invocation() {
    Invocation::Update { dry_run } => run_update(dry_run, options.format),
    Invocation::Config(command)    => run_config(command, &path, options.format),
    Invocation::Scan(intent)       => run_scan(intent, &path, options, EmitReport::Yes)
                                         .map(|outcome| outcome.code),
    Invocation::Cleanup(request)   => run_cleanup(request, &path, options),
    Invocation::UsageError(message) => Err(AppError::Config(message)),
}
```

**The dispatch table moved into `cli.rs`.** `Cli::invocation()`
(`src/cli.rs:312-403`) is now the single place that turns parsed argv into a
resolved intent, and `main` no longer inspects `command` at all — it does not even
read the field, apart from lifting the three globals at `:72-76`. This is M012A's
structural change and it is the right one: it makes "what did the user ask for" a
pure function of argv, testable without a subprocess, and it removes the
`None`-falls-through-to-scan fallthrough that used to live here.

**`Invocation` now has five variants** (`src/cli.rs:261-277`). The fifth,
`UsageError(String)`, exists because the compiler cannot see clap's invariant
that `scan` always carries a scope selector. Its two alternatives were both
wrong: defaulting to Full would silently run the machine-wide reconciliation the
flag exists to gate, and panicking would exit 101, outside the 0/1/2 contract.
`run` maps it to `AppError::Config` (`:89`) → exit 2, and `operation_name` maps
it to the unreachable token `"usage"` (`:48`).

Ordering consequence: **the config path is still resolved for every command,
including `update`** (`:71` runs before the match), so if the platform config
directory is unavailable (`src/config.rs:84-86`) then even `update` and `config
path` fail with exit 2 before reaching their arms.

### What `invocation` resolves

| Input | `Invocation` | Source |
|---|---|---|
| no subcommand | `Cleanup(Maintenance, Execute)` | `src/cli.rs:315-321` |
| `scan ROOT` | `Scan(Explicit(root))` | `src/cli.rs:328` |
| `scan --known` | `Scan(Maintenance)` | `src/cli.rs:329` |
| `scan --all` (or hidden `--full`) | `Scan(Full)` | `src/cli.rs:333` |
| `scan` (bare) | clap usage error → exit 2 | `src/cli.rs:86`, `src/cli.rs:335-337` |
| `config path/show/edit` | `Config(*)` | `src/cli.rs:339` |
| `update [--dry-run]` | `Update { dry_run }` | `src/cli.rs:344-346` |
| `clean …` | `Cleanup(<scope>, <mode>, <overrides>)` | `src/cli.rs:366-399` |

Three of those rows are worth calling out:

- **`None` is a cleanup.** A bare `cargo-cleanme` becomes
  `CleanupScope::Maintenance` with `CleanMode::Execute`, i.e. a Routine
  maintenance run that will actually delete eligible artifacts. It is *not* a
  read-only scan. The root `command` doc string says so in prose
  (`src/cli.rs:20-30`) and the enum comment names the intent
  (`src/cli.rs:175-193`).
- **`clean` with no scope selector is the same thing.**
  `src/cli.rs:366-372` maps `(None, false, _)` to `CleanupScope::Maintenance`,
  and `src/cli.rs:858-863` asserts that `["cargo-cleanme", "clean"]` and
  `["cargo-cleanme"]` produce identical `invocation()` values. This is why
  `main.rs` needs no "no root source" error branch (§2).
- **A bare `scan` is no longer a thing.** M014 made the scope selector
  mandatory: `scan` requires exactly one of ROOT / `--known` / `--all`,
  expressed as a required `ArgGroup` (`src/cli.rs:86`) so that the requiredness
  and the mutual exclusion cannot drift apart. The machine-wide Full
  reconciliation is now `scan --all`, one explicit flag away, rather than the
  zero-argument default it was. `scan --full` survives as a hidden alias
  (`src/cli.rs:102-103`).

Mode resolution is a free function rather than inline matching
(`src/cli.rs:284-296`): `dry_run || dryrun_legacy` → `Simulate`, else
`cargo_preview` → `CargoPreview`, else `Execute`. **Execute is the default.** The
hidden compatibility aliases (`--dryrun`, `--yes`) are folded into the same two
canonical modes; `--yes` is deliberately never bound (`src/cli.rs:347-350`),
since Execute is now reachable without it and binding it would print nothing
useful.

**`Update`** (`:93-219`) — `update::update(dry_run)?` then, per format, the JSON
stream (`:186-193`), the log line (`:194-206`), the human dry-run four-liner
(`:207-214`), or the one-line success (`:215-216`); `Ok(0)` at `:218`. The two
benign-error arms repeat the same three-way shape and return `Ok(0)` at `:141`
and `:182`. `--format` **is** honoured on every arm, log mode included.

**`Config`** (`:221-295`), three arms. **`Path`** (`:241-247`) prints the path
and never loads, creates, or validates the file, so it works when the file is
absent or corrupt — it needs only the *path* to be derivable. **`Show`**
(`:248-255`) is `config::show(&config::load_or_create(path)?)?` and therefore
**creates** the file if missing (`src/config.rs:405-419`) — a write side effect.
**`Edit`** (`:256-292`) does `config::ensure_exists` (create-if-absent *without*
requiring current contents to parse, `src/config.rs:422-427`), resolves
`$VISUAL`/`$EDITOR` (`src/editor.rs:7-8`), spawns, then three checks.

**`--format json` is now rejected for every `Config` arm** (`:230-234`):
`config` has no JSON envelope, and emitting a raw path or TOML text on a
`--format json` stdout would break the one-JSON-document automation contract,
so the combination exits 2 with a message that names it. `--format log` **is**
honoured (`:239`): the three arms print `output::log::config("path" | "show" |
"edit")` (`:243`, `:251`, `:288`) and the path or TOML text is withheld, because
a path in a retained history pane is exactly what that format refuses.

**The three post-edit checks.** (1) *Non-zero editor exit* (`:267-275`) prints
`cargo-cleanme: editor exited with {status}; config was not reverted: {path}` to
stderr and returns **`Ok(1)`** — the editor's own status is reported in prose and
**never** propagated as our exit code, because the exit contract is 0/1/2 and an
arbitrary editor status is not one of them; pinned by
`tests/cli_contract.rs:1103`. (2) *File deleted* (`:276-283`): `load` treats an
absent file as "all defaults" (`src/config.rs:96-98`), so a deleted config would
silently reset every policy → `Err(AppError::Config(…))` → exit 2. (3) *File
modified into an invalid state* (`:284-286`): `config::load` re-wrapped as
`AppError::Config("edited config {path} is invalid: {e}")`; pinned by
`tests/cli_contract.rs:1103`, which also asserts the bad content is left in
place (the message says "was not reverted"). Checks 2 and 3 exit 2; check 1
exits 1. `Ok(0)` at `:294` is reached only for `Path`, `Show`, and a successful
`Edit`.

**Bare invocation is Execute, and that is a standing decision.** Nothing in
`main.rs` gates it: there is no confirmation prompt, and `--dry-run` is opt-in.
The safety property that makes this acceptable is not in this file at all — it is
that every candidate must independently prove exclusive ownership before deletion
and that deletion is always `cargo clean` (§5). But the *default* is worth naming
plainly: a user who types `cargo cleanme` in a project directory gets bytes
deleted, not a report. If that is ever contentious it is a one-line change at
`src/cli.rs:315-321`, and `main.rs` would not need to move.

**The `about` text is the user-facing statement of that default — and one of its
sentences is now false.** `src/cli.rs:20-30` describes the bare invocation
correctly (`:22-26`), but `:27` still says "*`cargo cleanme scan` with no ROOT is
the full read-only machine reconciliation*". M014 made a bare `scan` a clap
usage error (`src/cli.rs:86`): the machine-wide reconciliation is now
`scan --all`. A user reading `--help` is told about a spelling that the parser
rejects. The same drift is visible one file over, in the comment at
`src/main.rs:1036`, which still opens "*Rootless `scan` is Full*" — the
*substance* is right (the Full intent ignores a configured `scan.root`), but the
spelling it describes no longer exists.

## 4. The scan pipeline

`run_scan` — `src/main.rs:1024-1424`, **four** parameters: `intent: ScanIntent`,
`config_path`, `options: RunOptions`, `emit_report: EmitReport`. Since C028 it
returns `ScanOutcome` (`:1018-1022`) — the exit code plus the proven Full
generation (§7). In order:

1. `scan_start` (`:1033`, the recency reference) and `wall_start` (`:1034`, behind
   `--stats`); `config::load_or_create(config_path)?` (`:1035`) — may create the
   file.
2. `intent` → `(root, full)` (`:1040-1044`). Rootless Full is `(None, true)` and
   *never* consults a configured `scan.root`; `scan --known` is `(None, false)`;
   `scan ROOT` is `(Some(root), false)`.
3. `policy::resolve(domain::ScanRequest { cli_root: root, full }, &c.scan)?`
   (`:1045-1051`); `recency` bound at `:1052`.
4. `IndicatifRenderer::new(!show)` (`:1058`) where `show` = `format == Human`
   **and** `should_show_progress(no_progress)`; `observer.phase(Discovery)`
   (`:1062`).
5. `discovery::discover_manifests_with_attribution(&policy, observer, stats)`
   (`:1070-1074`); a discovery error clears the bars before escaping, otherwise
   the drop would finish them into retained lines. `discovery_nanos += elapsed`
   saturating (`:1089-1091`); `_resolution_elapsed` is measured at `:1119` and
   **discarded**.
6. `uncertainty` extracted by filtering `PermissionDenied | Metadata |
   Vanished` (`:1092-1104`); `manifests_found` set (`:1107`).
7. `observer.phase(Resolution)` (`:1109`) and
   `workspace::resolve_workspaces(…)` with `SystemCargoRunner` (`:1111-1118`),
   mutating `counters` and `diagnostics`.
8. State reconciliation (`:1130-1215`) — §7.
9. `build_groups(&workspaces)` (`:1222`), `phase(Analysis)` (`:1223`),
   `units_total(Analysis, groups.len())` (`:1224`).
10. `clock_cutoff` via `checked_sub(recency)` (`:1226-1236`); a clock-overflow
    error clears the bars before escaping, like the discovery error in step 5.
11. `analyze_groups(&workspaces, groups, scan_start, clock_cutoff, recency, …)`
     (`:1238-1247`), mutating `counters` and `diagnostics`.
12. `full_incomplete` (`:1249`); `domain::ScanReport` built (`:1251-1269`),
    carrying `visited_entries` through.
13. `phase(Reporting)` (`:1271`), `finish_and_clear()` (`:1272`), deferred state
    notices (`:1276-1278`), **output always
    emitted** (`:1281-1300`), then diagnostics to stderr (`:1304-1383`) and
    `--stats` to stderr (`:1387-1404`).

Cargo locate/metadata failures remain one diagnostic per manifest in the report
and JSON. Human stderr groups identical bounded Cargo causes, shows at most three
sanitized representative paths per cause and at most eight causes; the exact
unresolved participant ledger and counters are not compressed. Other human
diagnostics retain their own ten-item display cap. This is presentation only and
does not alter scan coverage or cleanup eligibility.

**Step 2 deserves the emphasis.** Before M012A a rootless `scan` resolved through
`policy::resolve` with `full: false`, which meant a configured `scan.root` could
silently narrow the canonical reconciliation command — and, because reconciliation
is the only thing that expires learned roots, could *forget* roots outside that
one root. Configuration must not be able to do that to the one operation whose
job is to decide what the whole machine looks like. `scan --known` survives as the
read-only inventory over the maintenance scope, which is where a configured root
still wins as an Explicit override (`src/policy.rs:30-42`). M014 did not
re-open this: the Full intent is now reached only through an explicit `--all`,
and the requirement is spelled out at `src/main.rs:1036-1039`.

**Step 6, in detail.** `uncertainty` (`:1092-1104`) filters
`PermissionDenied | Metadata | Vanished`, then `.filter_map(|d| d.path.clone())`.
It reads `discovered.diagnostics`, the **pre-resolution** vector — *not* the
`diagnostics` vec `resolve_workspaces` goes on to mutate at `:1105-1118`, so a
permission/metadata diagnostic raised during `cargo metadata` resolution cannot
reach `uncertainty` and cannot protect a learned root. And `filter_map`
silently drops matching diagnostics carrying no path, so `uncertainty` is
path-addressed only.

**The filter is a `matches!` over three of the seven `DiagnosticCategory`
variants, with no `_` arm** (`:1095-1102`). That is a live hazard: adding a
fourth "something might be wrong here" category to
`src/domain.rs:68-78` compiles cleanly and is *silently* excluded from
`uncertainty`, so a newly-uncertain path would not protect a learned root and
the exclusion would read as deliberate. `OutOfScope` is correctly excluded — ADR
007 requires an out-of-boundary unprovable to be reported and never to block,
and `uncertainty` is exactly the "retain intersecting old roots" input.

**Step 10.** `checked_sub` returns `None` if the clock is earlier than
`1970-01-01 + recency_seconds`, killing the scan with exit 2 and a
*config-flavoured* message. `config::load` already range-checks
`recency_seconds` (`src/config.rs:99-106`), so only a mis-set clock causes this.
The same condition is handled non-fatally in `now_seconds()`
(`src/discovery_state.rs:405-410`, `unwrap_or_default()` → 0) — one root cause,
two policies. `tests/end_to_end.rs` uses the *unchecked* subtraction, so the
production guard is untested at the integration level.

### Report emission is an explicit parameter, not a side effect

`run_scan` takes `emit_report: EmitReport` (`src/main.rs:997-1008`, passed at
`:1028`). `Invocation::Scan` passes `EmitReport::Yes` (`:84`); the Full
reconciliation `clean --full` runs to prove a fresh generation before
consuming it (`:758`) passes `EmitReport::No`.

That parameter is not a formatting convenience. It is the only thing standing
between `clean --full` and a **double stdout document**, and the JSON envelope is
built on exactly one document per invocation. So `EmitReport::No` suppresses the
report and *nothing else*: diagnostics (`:1304-1383`) and `--stats`
(`:1387-1404`) still reach stderr, state reconciliation still runs, and every
decision downstream still reads the report. Only its rendering is withheld.

**This parameter was briefly deleted and had to be restored.** M012A removed the
old `emit_output: bool` on the reasoning that the flag was only ever passed
`false` once, and then `clean --full` emitted the intervening scan report ahead
of the cleanup report — two JSON documents on one stream in `--format json`. The
regression was caught during the M012B documentation pass, not by a test,
because **no test runs `clean --full` at any format** (§10). The lesson is
recorded rather than quietly fixed: a flag that exists for exactly one call site
looks like dead code, and deleting it looks like simplification.

**The guard is now asserted, but only at the source-text level.**
`tests/cli_contract.rs:3095` (`scan_emits_exactly_one_json_document`) proves the
one-document property for a scan and then reads `src/main.rs` as text and
asserts that the source still contains `"enum EmitReport"`
(`tests/cli_contract.rs:3124-3127`) — the test's own doc comment
(`tests/cli_contract.rs:3080-3093`) records the reasoning, that a real
`clean --full` case would be a full filesystem walk the bounded suite cannot
afford. That is a *name* check, not a *behaviour* check: renaming the enum, or
keeping the enum while every call site passes `EmitReport::Yes`, would leave the
assertion green. See §10 and §11 item 18.

## 5. The cleanup pipeline

`run_cleanup` — `src/main.rs:318-488`. It no longer branches on a `Clean` command
variant; every cleanup spelling arrives as a `CleanupRequest` whose `scope`,
`mode`, `selection` and `interactive` flag were resolved by `Cli::invocation`
(`src/cli.rs:366-399`).

**Mode** is taken straight from the request (`:324`). There is no mode-selection
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
(`src/cli.rs:140-145`) and the source comments are explicit
(`src/cli.rs:138-145`), but a user typing the wrong one still gets a different
operation, not an error. `--dryrun` and `--yes` survive as hidden aliases
(`src/cli.rs:146-152`) that map onto `Simulate` and the (now default) `Execute`
respectively.

**Root selection** is `resolve_cleanup_roots` (`:720-855`), matching on
`CleanupScope` — three arms, and *no error arm*, because M012A deleted the
`"clean requires ROOT, --known, or --full"` case. Since C028 every arm
classifies before collapsing, and automatic arms remember omissions and
convert uncertainty into a block rather than a fatal error:

| Scope | Behaviour | Label |
|---|---|---|
| `Root(p)` (`:727-745`) | `cli::absolutize_root(p)` classified `Explicit`; any defect is fatal | `"explicit"` |
| `Full` (`:746-767`) | run a Full scan, then consume the in-memory generation via `policy::full_cleanup_roots` — never a disk reload | `"full"` |
| `Maintenance` (`:772-853`) | configured `scan.root` takes the strict explicit path; otherwise raw Routine candidates (`policy::routine_cleanup_candidates`) classified `Automatic` | `"routine"` or `"explicit"` |

Two details in the `Maintenance` arm are worth more than their line count.
First, it does **not** keep only `ScanScope::Routine(roots)` and discard the rest
— it maps every variant: `Routine` → raw-candidate admission, `Explicit` and
`ExplicitRoots` → strict `"explicit"`, and `Global` → a hard
`AppError::Config("maintenance scope resolved to a global scan, which is
unreachable; refusing to treat it as an empty cleanup scope")` (`:847-851`).
Discarding the configured root produced a bare maintenance run that reported a
successful Explicit no-op it never performed; treating `Global` as "nothing to
do" would fail open on a scope that covers the whole machine. Both are the right
call.

Second, **classification precedes collapse** (`admit_automatic`, `:861-899`):
admitted paths only are collapsed (`:891`), omissions are remembered for the
late pre-spawn recheck, and symlink/unreadable automatic roots become a
whole-scope `ScopeBlock` carried on `ResolvedCleanup.blocked`
(`:873-888`). A stale learned path therefore can no longer reach the
engine's strict validation and abort the run — the C028 defect.

**Admission→validation race** (`run_engine_guarded`, `:623-714`): a fatal
`InvalidRoot` over an *automatic* scope (`:650`) triggers one fresh
reclassification; only a now-proven-omittable offender rebuilds the universe from
scratch (`:657-687`), anything else becomes a typed block (`:689-709`). Every
other error — and every error at all over an explicit scope — propagates
unchanged through the catch-all arm at `:712`.

**Omission diagnostics** are printed to stderr for every format except log
(`:357-361`): policy groups them by typed omission reason and includes a count
plus at most three sanitized example paths (`src/policy.rs:143-180`); log mode
carries only the count in its one line. Only the reasons the operator can act
on are reported: a root omitted because it sits at a known-transient origin is
omitted *by design* — the tool would never have cleaned it — so it is not a
diagnostic and does not reach the operator (C038). A durable root that has
vanished, and a root that is no longer a directory, are reported. The full
omission list remains available to the late pre-spawn premise recheck, and
summarization does not mutate learned state, so a stale hint may be
reconsidered on a later Routine invocation. A
blocked automatic scope reports without traversing anything (`:362-385`,
`Ok(1)`); an empty admitted set is the successful no-op (`:386-416`, `Ok(0)`).

**Policy** (`:328-343`): `min_reclaimable_bytes` and `min_inactive_seconds`
override cleanly (`Option`-on-scalar, so "unset" is distinguishable from "set
to 0"). `include`/`exclude` are different — `Vec<String>`, and clap cannot
distinguish "no `--include`" from "zero values", so **an empty CLI list falls
back to the configured list** (`:333-342`). Consequence: **`--include`/`--exclude`
cannot clear a configured list.** A user with `[cleanup.policy] exclude = ["**/target"]`
who wants a one-off unfiltered run has no flag for it. Defensible — the empty
list almost always means "I did not mention it", and treating it as "clear"
would be worse in the common case — but undocumented in `--help` and silently
surprising when it bites. A `clear-include` flag pair would fix it without
changing the default. Deliberate choice, not a defect.

**Selector** (`:344-353`): `profile.map(…).or_else(|| package.map(…))` looks
like silent-precedence, but **it is unreachable**. clap already enforces mutual
exclusion: `src/cli.rs:133` gives `profile` `conflicts_with = "package"` and
`:133` gives `package` `conflicts_with = "profile"`; `src/cli.rs:1056-1067` asserts
the combined forms are `Err`. The `.or_else` is defensive dead code, and a user
supplying both gets a clap usage error and exit 2 — correct, by a different
mechanism than the expression suggests.

**Empty-roots short circuit** (`:386-416`). If the admitted set is empty and no
block was raised, the pipeline stops before any Cargo invocation. The `CleanReport` literal
(`:392-407`) names **every field explicitly** — M012A removed the
`..Default::default()` spread, so a new `CleanReport` field is now a compile error
here rather than a silent default. That is a real robustness gain and it is why
the aggregate is still readable: `results`, `diagnostics`, `failed`, `mode`,
`counters`, `scope_blocked`, `unresolved_ownership`, `out_of_scope_unresolved`,
`selected_roots`, `discovered_manifests`, `resolved_workspaces`,
`units_considered`, `effective_policy`, `selector`. Note `selected_roots:
Vec::new()` even though `resolved.roots` is empty by construction — consistent,
not contradictory.

`emit_cleanup` is called with a fixed `empty_note` and **`Ok(0)` at
`:415`** — "nothing to do" is success.

### The `-i` selection step

`resolve_candidate_selection` (`:502-610`) is new in M014 and is the only place
in `main.rs` that turns a *declared* selection into a *resolved* one.

Without `-i` it is trivial: `request.selection.resolved()?` (`:529`). The
`CleanupSelection` enum (`src/cleanup.rs:73-91`) has four variants — `All`,
`WorkingDirectory` (an **unresolved** anchor on the process working directory),
`Ancestry(PathBuf)`, and `Roots(Vec<PathBuf>)` — and `resolved()`
(`src/cleanup.rs:122`) turns `WorkingDirectory` into `Ancestry`, failing closed
if the working directory cannot be read. `All` selects everything;
`WorkingDirectory` selects nothing until resolved (`src/cleanup.rs:106`), so a
caller that forgets to resolve it cleans nothing rather than everything. This
split exists because `cli.rs` resolves argv but must not perform IO.

**Which spellings get the anchor.** `src/cli.rs:378-381` binds
`CleanupSelection::WorkingDirectory` to the two maintenance spellings (bare
invocation and `clean --known`), whose Routine scope spans the machine, and
`CleanupSelection::All` to `clean ROOT` and `clean --full`, which already bound
their own scope. `src/cli.rs:758-797` asserts exactly that split.

**With `-i`**, three refusals happen *before* anything is spawned, in this
order: the machine formats first (`:537-543` — `--format json` and `--format log`
are automation contracts and cannot carry a terminal session), then the terminal
precondition (`:549-558`), then the listing itself (`:569-576`). The ordering is
deliberate and stated in the source: the listing is a full read-only resolution
that spawns `cargo metadata` across every resolved root, and spending the
operator's time and a process spawn on a run that was never going to happen is
worse than an early refusal.

The listing goes through the *same*
`cleanup::list_cleanup_candidates` pipeline the engine uses
(`src/cleanup.rs:2071`), so the offered rows are the engine's own units. A
blocked listing is refused outright (`:577-586`) — offering rows from an
unproven universe would be the ADR 006 shape in a new costume. The session
result maps to `SelectionOutcome::Chosen(Roots(..))`, `Cancelled`, or
`NothingToClean` (`:597-610`). A cancellation is a deliberate operator
decision, not a failure: it prints a plain sentence to stderr and returns
`Ok(0)` (`:422-429`). A scope that proved no cleanup unit is a plain negative
result, not a refusal: it prints `Nothing to clean.` on stdout and returns
`Ok(0)` (`:430-438`).

**The selection is a post-classification filter.** `resolve_cleanup_roots` and
the engine's own `build_groups` still see the full Routine universe, so output
shared with a workspace outside the anchor stays `Shared`. Narrowing root
selection *before* classification would be the ADR 006 defect.

### Reporting

**Reporting** (`:440-482`): `show` requires `format == Human` **and** a capable
terminal (`:440-441`) — so progress is disabled for both JSON *and* log mode,
which is what log mode wants; `wall_start` at `:443`; the engine call is
`:448-460`; `renderer.finish_and_clear()` at `:461` clears transient UI
**before** the report, matching the scan ordering. It runs on the **failure**
path too: the result is bound to `cleanup` and `finish_and_clear()` is called
before the `?` (`:461-462`), because `BarState::drop` *finishes* a bar rather
than clearing it, and a `?` that returned first left a retained bar line above
the `cargo-cleanme: …` message.

**That ordering is an open coverage gap, and it is recorded as one.** C023
established it — `renderer.finish_and_clear()` runs before the fallible cleanup
call — but the ordering lives in `main.rs`, which has zero tests, and it is
unreachable from `src/progress.rs`: reverting it leaves every test green,
because the bar only draws on an attended terminal and the failing-cleanup
contract test passes `--no-progress`. Pinning it needs a pty harness and a new
dev-dependency. See
`plans/closure/distribution-release-update/c023-status.md` §11.4.

The state-generation `println!` (`:463-467`) is Human-only, so it cannot corrupt
JSON *or* a log line.

**Exit code** (`:483-487`): `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`failed > 0` means at least one `CleanOutcome::Failed`, counted in
`CleanReport::render`'s tally (`src/cleanup.rs:595-597`) — "ran; at least one
workspace could not be cleaned". `scope_blocked.is_some()` means the scan
boundary itself was refused and no work was attempted; the block is a typed
`ScopeBlock` (`src/cleanup.rs:328-333`) whose `message` is what renders
(`src/cleanup.rs:524-528`). Pinned by `tests/cli_contract.rs:732`
(`Some(1)`, `scope_blocked == true`).

**`--stats`** (`:469-482`) goes to **stderr** via `eprintln!`, consistent with
`src/cli.rs:38-42`, and prints four counter strings plus mode and elapsed:
`stats_line()` (`:476`), `timings_line()` (`:477`), `proof_stats_line()` (`:478`),
`proof_timings_line()` (`:479`). The two `proof_*` lines are the point of the
comment at `:471-472` — "C003 §7.6: cleanup `--stats` must account for the
final ownership-universe proof work, not only the initial scan". The scan's
`--stats` (`:1387-1404`) has no proof counters, correctly: a scan performs no
proof. So the cleanup line is a superset and the C003 requirement is satisfied
at this call site rather than inside `cleanup.rs`.

**`--stats` is not disabled in log mode.** This is an explicit override, and it is
the only stderr output log mode does not suppress (progress and the diagnostic
fan-out both are). An unattended `--format log --stats` run therefore writes its
line to stdout and the detailed counters to stderr — which is correct, because
`--stats` is opt-in and its documented stream is stderr regardless of format.

### ADR 007 — the proof boundary is the selected scope

The two properties that changed the shape of this pipeline are worth stating
separately, because they are what a reader comparing this file with an older
copy will be looking for.

**The boundary is the selection, not the machine.** An unprovable participant
*inside* the selected scope blocks the run; one *outside* it is reported and
never blocks. The `CleanReport` carries the two apart in two distinct fields
(`src/cleanup.rs:401-414`): `scope_blocked` / `unresolved_ownership` for the
boundary-relevant case, and `out_of_scope_unresolved` for the count of
manifests that did not resolve outside it. The human report renders the latter
as one bounded line (`src/cleanup.rs:518-522`) so that "boundary clean" cannot
read as "system clean".

**`main.rs` participates in the reporting, not the blocking.** The four
synthesized `CleanReport` literals in this file — the blocked scope
(`:367-382`), the empty-roots no-op (`:392-407`), and the two
`run_engine_guarded` fallbacks (`:669-684`, `:689-709`) — all set
`out_of_scope_unresolved: 0` (`:375`, `:400`, `:677`, `:702`). In the first two
that is *true*: no traversal happened, so nothing was observed out of scope. In
the `run_engine_guarded` fallbacks it is an **unmeasured** zero: the engine ran
and returned `Err`, so its report — and any out-of-scope count it carried — was
discarded. The JSON envelope does not carry the field at all
(`src/output.rs:55-63`); a JSON consumer sees out-of-scope poison only as
`out_of_scope`-category diagnostics (`src/output.rs:134`). See §11 for what this
does and does not claim.

## 6. Root resolution and collapse

`collapse_roots` sits at **`src/main.rs:965-979`**, called from
`admit_automatic` — after provenance admission, never before it. Its position
is the C028 fix: classification must see the raw identities, because
canonicalizing first would resolve a symlinked root into its target and admit
a real directory where a block belongs
(`policy::routine_cleanup_candidates`, `src/policy.rs:475`).

```rust
// collapse_roots — src/main.rs:965-979
let mut roots: Vec<_> = roots
    .into_iter()
    .map(|p| std::fs::canonicalize(&p).unwrap_or(p)).collect();   // :966-969
roots.sort();                                                    // :970
roots.dedup();                                                   // :971
let mut collapsed: Vec<std::path::PathBuf> = Vec::new();          // :972
for path in roots {
    if !collapsed.iter().any(|parent| path.starts_with(parent)) {  // :974
        collapsed.push(path);
    }
}
collapsed
```

**Canonicalization is best-effort.** `unwrap_or(p)` (`:968`) means a
non-existent or unresolvable path passes through **unchanged** — but since
C028 such a path never reaches this function through the automatic scopes:
proven-absent roots were already omitted and uncertain ones already blocked,
so only admitted real directories (plus strict explicit roots) arrive here.
The absolute-root invariant the cleanup boundary depends on is established
*earlier*, by `absolutize_root` (`src/cli.rs:435-444`), and only for the
explicit-`ROOT` source. For the other two sources the roots were admitted as
real directories by classification (`src/policy.rs:247-249`), so in practice they
resolve; the engine's own strict validation (`src/cleanup.rs:2627-2645`) plus
the admission→validation race defence (`run_engine_guarded`, `:623-714`)
cover the residual window.

**The engine re-collapses, fail-closed.** `cleanup.rs` has its *own*
`collapse_roots` (`src/cleanup.rs:2647-2673`), and it is the strict one: a
canonicalize failure is an `InvalidRoot`, not a passthrough. So the permissive
`unwrap_or` in `main.rs` is not the last word — the engine re-derives the
canonical set and refuses to continue if a root stopped resolving between
admission and the collapse.

**The sort is load-bearing, not cosmetic.** `PathBuf` ordering is component-wise
and a parent always sorts before its children (`/a/b` < `/a/b/c`). The
containment filter at `:974` only works because of that: a child is tested only
after its parent has been visited and accepted. Remove the `sort()` and the same
input in reverse order keeps *both* `/a` and `/a/b` — collapse silently stops
collapsing. `reconcile_full` argues the same case about its own sort
(`src/discovery_state.rs:326-329`: "*`learned` is sorted, and a parent always
sorts before its children, so every possible parent has been seen when its child
is reached*"), so the codebase is internally consistent.
`Path::starts_with` is component-wise, not string-prefix (`/a/bc` does not
`starts_with` `/a/b`), which makes this correct rather than merely plausible.

**Safety property:** after collapse, no accepted root contains another, so
overlapping roots cannot produce overlapping deletion candidates — a single
physical `target/` cannot be reachable through two surviving roots, so it cannot
be counted twice, cleaned twice, or have its ownership universe computed against
two boundaries.
**Do the three sources mix badly?** In practice no: `absolutize_root` yields a
lexically absolute path, admitted automatic roots were proven real directories
by classification before collapse, and the engine re-validates plus rechecks
the admission premise. The residual case in the old text — a learned root that
has since become a symlink — is now a whole-scope block at admission time, not
an uncollapsed overlap.

**Untested at orchestration level.** `collapse_roots` is private to a file with
no inline tests, and no integration test can call it directly. The
nest-collapse property is covered indirectly through the admitted-root
assertions in the C028 CLI contract tests
(`tests/cli_contract.rs:3560-3657`); the admission-before-collapse ordering
itself is pinned by
`cleanup_candidates_preserve_symlinked_seed_identity_for_admission`
(`src/policy.rs:1089`).

## 7. State reconciliation

`src/main.rs:1130-1215` — the only genuinely intricate block, and where a
reviewer should spend the most time.

```rust
let loaded_state = discovery_state::load_default();            // :1122
if full { … } else if let StateLoad::Loaded(prior) = loaded_state { … }  // :1130, :1199
```

There is no `state_reconciled` flag. It was removed when the failed-publish
exit-code escalation was fixed; see "A failed publish no longer changes the
exit code" below. Nothing in this block can now influence the process exit
status — the stderr notice is the whole of the consequence. Since C028 the
block additionally yields the proven in-memory generation to the caller
(`ScanOutcome::full_state`, `:1020`), so a Full cleanup consumes exactly what
the traversal proved even when the save fails.

**Prior selection.** `full_reconciliation_prior` returns
`Option<(DiscoveryState, bool)>` (`src/discovery_state.rs:54-61`): `Loaded(s)` →
`Some((s.clone(), false))`; `Missing` → `Some((default, false))`;
`RecoverableInvalid(_)` → `Some((default, true))` where the `bool` is
`replacing_invalid`; `UnsupportedNewer` and `Unavailable` → `None`. The `None`
case is the important safety choice: **state written by a newer binary, or
simply unreadable, is never overwritten.** The `else` at `:1192-1198` records
a `{message}; Full state reconciliation was skipped` notice (fanned out to
stderr after the progress clear), where
`message` is `loaded_state.diagnostic()` (`src/discovery_state.rs:33-49`) with a
generic fallback.

**The join** (`:1134-1143`) maps canonicalized member manifest → workspace root;
**observations** (`:1144-1153`) apply the same canonicalize-or-raw rule to each
discovered manifest, so the join succeeds whenever both sides are
canonicalizable. `cargo metadata` reports absolute real paths and discovery
manifests come from the walk, so both canonicalize to the same bytes in
practice. When they do not, the failure is a **safe degradation, not a wrong
answer**: the record falls back to `manifest.parent()`
(`src/discovery_state.rs:233`) and is marked
`ResolutionStatus::ManifestObservedUnresolved` — a *broader* root, not a
narrower one. Note that such a record does **not** seed a learned root: only a
Cargo-resolved workspace does (`src/discovery_state.rs:247-248`), so an
unresolved manifest is inventory, never a cleanup candidate.

**`complete`** (`:1154`) is true unless some diagnostic is `PlatformRoot` +
`Error`, and it gates everything: `reconcile_full` returns
`NoPublication("Full traversal was incomplete")` before touching anything
(`src/discovery_state.rs:220-221`). That is the counterpart of `full_incomplete`
at `:1249`, which recomputes the *same predicate* for the exit code. Two
identical expressions in one file; they could share a helper.

**Publish** (`:1166-1191`): on `Reconciliation::Publish(next)`, the proven
generation is stored into `ScanOutcome::full_state` first (`:1171`), then
`publish(&next)` succeeding may record a "replaced unusable discovery state"
notice if `replacing_invalid` (`:1173-1180`); failing records
`cargo-cleanme: discovery state was not saved: {error}` (`:1181-1185`) and
**changes nothing else**. `NoPublication(reason)` records
`… was not reconciled: {reason}` (`:1187-1190`). All three notices wait for the
progress clear before reaching stderr, so they cannot orphan bar rows.

### A failed publish no longer changes the exit code

`src/main.rs:1412-1423` is the whole exit decision:

```rust
if full_incomplete {
    return Ok(ScanOutcome { code: 1, full_state, publish_ok });   // :1413-1417
}
Ok(ScanOutcome { code: 0, full_state, publish_ok })               // :1419-1423
```

**This was a real defect, and it is fixed.** The block previously read
`if full_incomplete || (full && !state_reconciled) { return Ok(1); }`, so a
failed state publish returned `Ok(1)`: a good scan report on stdout, one stderr
line explaining the state was not saved, and a non-zero exit. That contradicted
the module's own position, stated at `src/discovery_state.rs:479`: *"State is an
optimization only: concurrent publishers use atomic last-writer-wins
replacement."* A read-only or full state
directory — plausible on a locked-down machine, in a container with a read-only
home, or under `chattr +i` — turned a **purely informational scan** into a
non-zero exit for a reason unrelated to what the scan found, and scripts gating
on status reported a scan failure that did not happen.

The `state_reconciled` flag is gone, so the two branches no longer disagree:
the Routine branch's byte-identical "state was not saved" notice (`:1210-1214`)
and the Full branch's (`:1181-1185`) now both leave the exit code alone. A
genuinely incomplete Full scan still exits `1`, via `full_incomplete`. Since
C028 the in-memory generation travels with the outcome either way, so
`clean --full` consumes exactly what the traversal proved
(`policy::full_cleanup_roots`, `src/policy.rs:440-464`) instead of re-reading the
disk into a possibly older generation.

**M012A widened what `full` means, so this fix now protects more.** The Full
reconciliation is now reached by `scan --all` and by `clean --full`, which are
the two operations most likely to hit an unwritable state directory — and they
are protected by the same rule the fix established. The cost is on the other
side: `run_scan` returning a non-zero code from `clean --full` now escalates to
exit 2 (§5), so the *only* way state can influence an exit code is through that
one deliberate call site.

### The Routine branch (`:1199-1215`)

Matches on `StateLoad::Loaded` specifically, so a Routine scan *silently*
ignores `Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable` — no
diagnostic at all, unlike the Full branch's `else`. It computes
`observed: Vec<_> = workspaces.iter().map(|w| w.root.clone())` (`:1201`), then for
each project bumps `last_project_seen_at` on every learned root for which
`project.starts_with(&root.path)` (`:1203-1209`), and finally publishes only if
`!observed.is_empty()` (`:1210-1214`), printing `discovery state was not saved:
{error}` to stderr on failure. Routine/Explicit scans update positive
observations only and never persist negative pruning (C028).

**The asymmetry.** This branch only ever *touches* `last_project_seen_at`. It
does not add or remove learned roots, does not update `projects`, does not touch
`last_full_at`. Learning and forgetting happen **only** in Full
(`src/discovery_state.rs:211-300`). That is the correct split — a Routine scan
is a cheap probe over a bounded scope and must not rewrite what it did not
observe — and the `!observed.is_empty()` guard prevents a probe that resolved
nothing from republishing unchanged state.

**Which scans reach it.** Only `scan --known` and `scan ROOT`. Bare
`cargo-cleanme` never reaches `run_scan` at all any more — it is a cleanup
(§3) — so the branch is reached by two spellings of the read-only inventory
rather than by the default invocation. `clean --full` passes through the
Full branch instead.

**`starts_with` is un-canonicalized.** `project` comes from `cargo metadata`;
`root.path` is canonicalized from when it was learned
(`src/discovery_state.rs:263`). A learned root spelled canonically and a
workspace root reached through a symlink fail to match — a **missed touch**, so
`last_project_seen_at` goes stale and the root eventually ages out of the
Routine candidate set (`src/policy.rs:492-497`). Self-limiting: the root is
dropped from Routine scope, not deleted, and a later Full scan re-learns it.
`main.rs` canonicalizes deliberately in the Full branch (`:1138`, `:1147`) and
not here. Small, real, one-sided in severity.

**No duplicate-publication exposure.** `manifests` is sorted and deduped by
discovery before return (`src/discovery.rs:809-812`), so `observations` is 1:1
with distinct manifests and cannot duplicate `ProjectRecord`s; and
`reconcile_full` separately collapses nested learned roots by nearest ancestor
(`src/discovery_state.rs:326-357`). `main.rs` needs no defence against either,
and has none.

## 8. Output selection

`OutputFormat` (`src/cli.rs:6-14`) has **three** variants — `Human` (default),
`Json`, and `Log` — and is a **global** flag (`src/cli.rs:45`), so it may appear
before or after the subcommand. `Log` is M012B's addition and is not a machine
contract; see [10-reporting §7](10-reporting.md).

| Path | Human | JSON | Log |
|---|---|---|---|
| scan | `report::render(&mut report)` `:1299` | `to_string(output::scan(&report, scope))` `:1287-1292` | `output::log::scan(&report, scope, full_incomplete)` `:1296` |
| cleanup | `println!("combined roots …\n{}", report.render())` `:936-944` | `to_string(output::cleanup(&report, scope, gen))` `:915-921` | `output::log::cleanup(&report, scope)` `:923-926` |
| cleanup, empty roots | the fixed note `:928` | the all-fields-explicit report `:392-407` | same call, via `emit_cleanup` `:922-926` |
| cleanup, nothing to report | `Nothing to clean.` `:934` | same call, via `emit_cleanup` `:915-921` | same call, via `emit_cleanup` `:923-926` |
| update | three literal shapes `:208-216` | `update_json_stream(&plan, dry_run)` `:190-193` | `output::log::update(...)` `:198-206` |
| update, already current | `:138` | `serde_json::json!` document `:110-125` | `output::log::update(...)` `:127-135` |
| update, newer than published | `:179` | `serde_json::json!` document `:151-166` | `output::log::update(...)` `:168-176` |
| config | `println!` only `:245`, `:253`, `:290` | **rejected** → exit 2 `:230-234` | `output::log::config(action)` `:243`, `:251`, `:288` |
| fatal error | `cargo-cleanme: {e}` `:32` | none — stderr + exit 2 `:34` | `output::log::fatal(op, code)` `:27-30` |

**The third column no longer has a hole.** The previous revision of this file
recorded two: `update --format log` silently produced human output, and `config`
ignored `format` entirely. Both are closed. `update` now has an explicit log
branch on all three of its shapes, and `config` has one too — and where log
mode cannot carry the content (`config path`, `config show`), the human content
is withheld in log mode rather than the log line being withheld, because a path
in a retained history pane is exactly what that format refuses.

What remains is the *opposite* of a hole: `config --format json` is a **hard
rejection** (exit 2) rather than a silent fallback to non-JSON on a
`--format json` stdout. The one-JSON-document automation contract is worth
more here than a `config` invocation that happens to work.

### The `scope` label is derived from the resolved policy, not the flags

`scope_label` (`:989-995`) matches on `ScanScope` — `Global` → `"full"`,
`Explicit`/`ExplicitRoots` → `"explicit"`, `Routine` → `"routine"` — and is called
with `policy.scope` (`:1289`, `:1296`), which `policy::resolve` returned at
`:1045-1051`.

This was a real contract bug and it is fixed. The previous code derived the label
from `full` and `root.is_some()`; mechanism is in
[10-reporting §9](10-reporting.md) ("Can `scope` be wrong?"). The residual note is
that `scan` and `cleanup` now share one label vocabulary — `full`, `explicit`,
`routine`.

### `state generation last_full_at=` goes to stdout

`src/main.rs:463-467` uses `println!` — **stdout**, not stderr — guarded to human
format (`:463`), so it cannot corrupt JSON *or* a log line. But it is
machine-adjacent metadata interleaved into a human report, on exactly the stream
that pipelines capture. The project's stated discipline is stdout = deterministic
report, stderr = transient/diagnostic. This line is neither. It belongs on stderr
beside `--stats`, or in the JSON envelope as a field — `output::cleanup` already
accepts a `state_generation` argument (`src/output.rs:161-165`), and
`log::cleanup` deliberately does not, so a log consumer cannot recover it at all.

### Serialization failure

Two sites, inconsistent: `:1291` and `:920` map to
`AppError::Config(format!("cannot serialize JSON report: {e}"))`, which is the
wrong *variant* — the user sees `cargo-cleanme: configuration error: cannot
serialize JSON report: …`, sending them to `config.toml` when the fault is in
report serialization (it does at least match the existing habit: `config::show`
does the same at `src/config.rs:610-612`).

In practice `serde_json::to_string` on these DTOs cannot fail — `EnvelopeV1` is
owned primitives, strings and `Option`s, with no map keys and no `f64`
NaN/Infinity — so this is a defensive branch that is effectively dead. The real
risk is that the *human* and *log* paths have no equivalent guard.

### `update_json` — the second, hand-rolled envelope

`update_json` and `update_json_stream` live in `output.rs`
(`src/output.rs:903` and `src/output.rs:941`), not in `main.rs`. `run_update`
calls the **stream** variant (`src/main.rs:190-193`) so that the exact bytes on
stdout come from the library seam the contract tests drive, rather than from a
`println!` assembled in the binary. The previous revision of this file located
`update_json` in `main.rs` and called it untested; both claims were stale. The
machine contract is now pinned by four unit tests in `output.rs`
(`src/output.rs:945-1065`) and by
`tests/cli_contract.rs:3271`, which reads `src/main.rs` as text and asserts that
`run_update` contains exactly one unterminated `print!(` and that it is the
`update_json_stream` call.

What remains true is the *shape* of the finding: `update_json` is a second,
hand-maintained envelope alongside `output.rs`'s `EnvelopeV1`, and the two
benign-error arms of `run_update` (`src/main.rs:110-125`, `:151-166`) hand-build
a `serde_json::json!` document in the binary — a third copy of that shape, now
outside `output.rs` entirely. C022 pinned the machine contract; migrating
`update_json` to `EnvelopeV1` is still a contract change needing a plan and a
version bump.

### Ordering divergence between the two cleanup projections

**Confirmed.** `CleanReport::render` takes `&self` (`src/cleanup.rs:501`) and
therefore **cannot** reorder its own data: it builds a local
`ordered: Vec<&CleanResult>` (`src/cleanup.rs:561`), sorts by `before_bytes`
descending with a `display_path` ascending tie-break (`src/cleanup.rs:562-567`),
and iterates *that* (`:581`). `output::cleanup` takes `&CleanReport`
(`src/output.rs:161-162`) and iterates `report.results` in **original insertion
order** (`src/output.rs:167-189`) with no sort at all.

So JSON `units[]` and the human unit rows are in different orders whenever
insertion order is not already size-descending, and a consumer matching the two
positionally or diffing them sees spurious differences. The paths never
co-execute (`emit_cleanup` branches at `:915`/`:936`), so no test can observe it,
and `tests/cli_contract.rs` only ever indexes `units[0]` — order-independent in
its fixtures.

The **scan** side is the opposite and self-consistent: `report::render` takes
`&mut ScanReport` and sorts `report.groups` **in place**
(`src/report.rs:44-49`), which `run_scan` relies on by passing `&mut report`
(`:1299`). The asymmetry between the two `render` signatures — `&self` + local copy
for cleanup, `&mut self` + in-place sort for scan — is the root of the finding.

## 9. Invariants and edge cases

**Is the exit code consistent? Mostly.** 1 = "ran, partially failed",
2 = "could not run". Two known outliers: `config edit` is the deliberate one,
collapsing an arbitrary editor status into 1 (`:274`, pinned by
`tests/cli_contract.rs:1103`); and `update` cannot express partial failure at
all. M012A added a third, also deliberate: `clean --full` on an incomplete scan is 2
(`:764-766`).

**Can a command return 0 while having printed to stderr? Yes, routinely.**

| stderr message | Line | Exit effect |
|---|---:|---|
| `N filesystem diagnostics; rerun with a bounded root if needed` (+ ≤10 details) | `:1305-1382` | **none** — a scan full of `PlatformRoot` errors still exits 0 |
| `scan stats: …` | `:1387-1404` | none (opt-in) |
| `cleanup stats: …` | `:469-482` | none (opt-in) |
| `discovery state was not saved: {error}` — **Routine** | `:1210-1214` | **none** |
| `{warning}; using seed/configured Routine roots` (from policy) | `src/policy.rs:517`, `src/main.rs:835` | none |
| `replaced unusable discovery state after successful Full reconciliation` | `:1173-1180` | none (a *success* notice) |
| `discovery state was not reconciled: {reason}` | `:1187-1190` | **none** (fixed; see §7) |
| `discovery state was not saved: {error}` — **Full** | `:1181-1185` | **none** (fixed; see §7) |
| `{message}; Full state reconciliation was skipped` | `:1192-1198` | **none** |
| `editor exited with {status}; …` | `:268-271` | **→ 1** (not the editor's code) |
| `cannot launch editor {program}: {e}` | `:264-266` | → 2 |
| `interactive selection cancelled; no cleanup commands were run` | `:425-427` | none (`Ok(0)`) |
| `discovery state was not saved; cleaning the reconciled in-memory generation only` | `:759-763` | none (the Full cleanup proceeds on the in-memory generation) |

The state-warning rows were **fixed**, not left as a finding: the Full branch used
to escalate both to exit 1 while the Routine branch ignored the byte-identical
message. Both are now inert, which is why the "sharp inconsistency" the earlier
revision described no longer exists here. The remaining asymmetry is cosmetic —
the Full branch explains itself on stderr while the Routine branch is silent about
a load problem it never reports.

**Can `run_scan` be reached twice in one process?** No. The `Invocation` arms are
mutually exclusive, and the Cleanup arm is the only caller of `run_scan` from
inside `run`, calling it once (`:758`). So the "re-scoped totals" comment at
`:1217-1221` describes the phase sequence *within* one call, not a second call.
There is no double-initialised renderer, no global state, no `static mut` in the
file — the renderer is a plain local (`:442` for cleanup, `:1058` for scan) whose
`finish_and_clear()` runs once (`:1272`, `:461`) before the report, failure path
included.

**Is `checked_sub` the only overflow guard? Yes, in this file.** `scan_start`
(`:1033`) is used unguarded at `:1242` and guarded at `:1226` (§4).

**Config directory read-only.** Only *creation* fails. `load_or_create`
(`:1035`, `:323`) calls `create_initial` (`:465`), which does
`fs::create_dir_all(parent)?` (`src/config.rs:457-461`) → `AppError::Io` via
`#[from]` (`src/error.rs:9`) → exit 2. If the file exists and is readable, `load`
performs no writes and everything works. Scope: `scan`, bare invocation,
`clean …`, `config show` and `config edit` all exit 2; `config path` works
(resolves without touching the FS, `:241-247`) and `update` works (never loads
config, `:94`). `--config /other/path.toml` is the documented escape hatch
(`src/cli.rs:33-34`).

**Can `process::exit` truncate buffered stdout? No.** `main` calls
`std::process::exit` at `:20` and `:34`, which does skip destructors — but
nothing is buffered waiting for them. Rust's `std::io::Stdout` is wrapped in a
`LineWriter`, so it flushes on every `\n` regardless of whether the target is a
TTY or a pipe. Every output statement here is a `println!`/`eprintln!` ending in a
newline, with one exception: the JSON update branch uses `print!`
(`src/main.rs:190-193`) — deliberately, because the library seam supplies the
trailing newline itself. The C-stdio block-until-`exit` failure mode does not
apply to Rust, and the one-newline assertions at `tests/cli_contract.rs:172` are
consistent with nothing being lost. The only theoretical exposure is a *partial*
final line with no trailing newline, which this file never produces.
**Definitive: piped JSON output cannot be truncated by `process::exit`.** The
corollary is the actual risk: safety comes from `LineWriter` plus
newline-terminated writes, not from an explicit flush, so adding a buffered
writer or a non-`println!` path would remove it silently.

**What the safety-critical code does *not* do.** This file cannot delete anything.
It resolves roots, merges policy, chooses a mode, calls the engine, and renders.
Every destructive decision — ownership proof, authorization, recency, and the
`cargo clean` invocation itself — happens inside `cleanup.rs` behind
`clean_with_roots_policy_selector_guarded` (`:635-646`). The most consequential
thing `main.rs` does is *choose what to pass*: a wrong root, a wrong mode, or a
wrong `selected_roots` label is a correctness defect here and a safety defect
only if `cleanup.rs` trusts it, which it does not (it re-proves every candidate).
That separation is the reason this file can be 1562 lines of thinly tested
composition.

## 10. Testing

**`src/main.rs` has four inline tests.** Verified:
`grep -cE '#\[cfg(test)\]|#\[test\]' src/main.rs` → `4`. The `#[cfg(test)] mod
tests` block opens at `:1426`; the four cases pin the orchestrator's own
decisions — the interactive surface's three endings, and the report verdict
(`Nothing to clean.`). The `use` statements at `:1030-1031` and `:1121` are
ordinary function-scoped production imports inside `run_scan` — a common source
of the opposite impression. The same grep on the two output modules:
`src/main.rs` **4**, `src/output.rs` **21** (2 in `path_tests`,
15 inside `pub mod log` (`src/output.rs:509-943`), and 4 in `update_tests`
(`src/output.rs:945-1065`)), and `src/report.rs` **7** — it has an inline suite
(`src/report.rs:72-195`). See [10-reporting §10](10-reporting.md) for the log
tests and what they leave uncovered.

**Consequently `update_json` is no longer uncovered** — but the gap moved. The
envelope implementations live in `output.rs` and are covered by four unit tests
(`src/output.rs:945-1065`); what is *not* covered is the two `serde_json::json!`
documents the binary hand-builds for the benign `update` outcomes
(`src/main.rs:110-125`, `:151-166`). Those are the same shape as the envelope
they imitate, written in the one file whose four inline tests do not reach them.

### `tests/cli_contract.rs` — 4050 lines, 63 tests

Drives `env!("CARGO_BIN_EXE_cargo-cleanme")` as a subprocess and asserts on
status, stdout and stderr. The pre-M012A tests survived; M012A, M012B, M014 and
M017 added the rest, and they are the ones that cover the behaviour this file
changed. Verified line numbers:

| Test | Line | What it pins about `main.rs` |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 75 | `cargo cleanme --version` (external) byte-identical to direct — i.e. the argv normalization in `parse_normalized` (`:15`) is correct; plus a premise guard that the staged binary is the one Cargo resolves |
| `cargo_external_subcommand_help_matches_direct_help` | 115 | `--help` exits 0, identical both forms |
| `cargo_external_config_edit_help_matches_direct` | 149 | `config edit --help` exits 0, identical both forms |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 172 | `schema_version == 1`, `operation == "scan"`, `scope == "explicit"`; **`plain.stdout == with_stats.stdout`** — `--stats` cannot perturb stdout; stderr contains `scan stats:`; **exactly one `\n`** |
| `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` | 214 | a **configured** `scan.root` under `scan --known` reports `"explicit"` — the regression for the flag-derived label (§8) |
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 661 | parser-level rejection, asserted on clap's conflict error rather than "the command failed" |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 693 | `schema_version == 1`, `operation == "clean"`, `mode == "simulate"` (`--dryrun` → `CleanMode::Simulate`), `summary.simulated == 0`, one `\n` |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 732 | **`status.code() == Some(1)`** with `result.scope_blocked == true` — pins the `:483-487` exit rule |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 779 (`#[cfg(unix)]`) | six invocations against a `/bin/sh` cargo stub: `summary.cleaned == 1`, `units[0].reason_code == "cleaned"`, `selector_kind/value == "profile"/"dev"`; `before_bytes` and `selector_estimate_bytes` both `null` under a selector while `output_union_before_bytes >= 4096`; `units[0].outcome == "simulated"` under `--dryrun`; `reason_code == "selector_unsupported"` + `policy_disposition == "selector_estimate_unavailable"`; `selector_kind == "package"` with `fixture@0.1.0` |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 1103 | **`status.code() == Some(1)`** — the editor status is reported in prose and collapsed to 1 (`:274`); an invalid edit exits non-zero with `is invalid` in stderr and leaves the bad content in place |
| `working_directory_anchor_selects_without_narrowing_the_ownership_universe` | 1475 | the M014/ADR 006 anchor: selection narrows, the ownership universe does not |
| `interactive_without_a_terminal_is_a_usage_error_that_names_the_reason` | 1531 | the terminal precondition is checked before the listing (`:549-558`) |
| `interactive_refuses_the_machine_output_formats` | 1568 | `--format json`/`--format log` are refused for `-i` (`:537-543`) |
| `interactive_dry_run_still_requires_a_terminal` | 1620 | the refusal is a property of the *session*, not of the mode |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` | 1638 | **the M012A default.** `operation == "clean"`, `units` is an array where a scan would have `groups`, `mode == "execute"`, `scope == "explicit"`, and — the discriminating assertion — `clean_calls() == 1` with the artifact gone |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` | 1675 | `mode == "simulate"`, `simulated == 1 && cleaned == 0`, **`clean_calls() == 0`**, artifact still present, and Cargo *was* used for resolution — so this is a simulation, not a skip |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` | 1899 | exit 0, `scope == "routine"`, `mode == "execute"`, `units == []` — the `:386-416` short circuit through the binary |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` | 1933 | blocked bare invocation, exit 1, no `cargo clean` |
| `blocked_human_cleanup_report_names_manifests_and_json_stays_one_object` | 1973 | the blocked-scope human surface and the one-document JSON surface coexist |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` | 2150 | `clean ROOT` with no mode flag executes; `--cargo-preview` gives `mode == "preview"` |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` | 2208 | `--dryrun`/`--yes` map onto `Simulate`/default `Execute` |
| `every_accepted_simulation_spelling_reaches_simulate_and_spawns_no_cargo_clean` | 2261 | six accepted `--dry-run` spellings, none of which spawns `cargo clean` |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` | 2455 | `scan --known` is `routine` with no configured root, `explicit` with one |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 2512 | the bounded-zero-result scan document |
| `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` … `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 2618-3060 | the M012B log cases, via a shared `assert_bounded_log_line` (`:2578`): one line, bounded, silent stderr; `simulate` vs `preview` distinguishable; blocked carries a typed reason and exit 1; failed cleanup is `status=failed`; zero-result maintenance is `status=ok`; scan reports the resolved scope; `--stats` stays opt-in on stderr and leaves the one line alone; the human diagnostic fan-out is suppressed; a fatal error is **one bounded stderr line and empty stdout**, exit 2, carrying a typed code and *not* the config path or parse error; JSON and human are unchanged by log mode's existence; an unknown format still fails closed |
| `scan_emits_exactly_one_json_document` | 3095 | the one-document-per-invocation invariant, and — at source-text level only — that the `EmitReport` guard still exists (§4) |
| `the_json_update_branch_prints_only_the_tested_seam` | 3271 | reads `src/main.rs` as text and asserts `run_update` has exactly one `print!(` and that it calls `update_json_stream` (§8) |
| `stale_learned_root_is_omitted_and_bare_routine_execute_cleans_the_sibling` | 3560 | C028 premise-negative regression, direct and staged-plugin: exit 0, `scope == "routine"`, `cleaned == 1`, one Cargo clean, stale path absent from `selected_roots` |
| `stale_learned_root_simulate_agrees_with_execute_and_spawns_no_cargo_clean` | 3616 | C028 Simulate/Execute parity: `simulated == 1`, zero clean spawns, artifact kept |
| `missing_explicit_roots_stay_fatal_with_zero_cargo_clean_spawns` | 3689 | C028 negative control: missing configured `scan.root` and missing `clean ROOT` both exit 2 with zero spawns |
| `all_automatic_roots_missing_is_a_successful_empty_no_op` | 3729 | C028: all-missing automatic scope exits 0 with `units == []` and zero spawns of any kind |
| `automatic_file_root_is_omitted_and_symlink_root_blocks_without_traversal` | 3756 | C028 decision table at the CLI: file omitted (sibling cleaned), symlink-to-dir and broken link block with exit 1 and zero metadata calls |
| `out_of_anchor_poison_does_not_block_bare_routine_cleanup` | 3820 | **ADR 007.** A poisoned learned root *outside* the anchor does not block: exit 0, `cleaned == 1`, `scope_blocked == false`, one Cargo clean, and the poison is counted in the machine summary |
| `unreadable_automatic_root_blocks_instead_of_omitting` | 3859 | C028: `0o000` automatic root blocks with exit 1; premise-checked skip when the process can still read (e.g. root) |
| `incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean` | 3892 | **ADR 007, in-boundary.** An unreadable in-boundary root blocks with exit 1 and *real* resolution counts (`discovered_manifests == 1`, `resolved_workspaces == 1`, `units_considered == 1`) — the block is not allowed to report zero |
| `stale_and_blocked_automatic_roots_keep_the_bounded_log_contract` | 3928 | C028 machine contract: one bounded ASCII log line, no paths, exit 0 on omission and `status=blocked` with exit 1 on the symlink block |
| `json_format_config_is_rejected_not_emitted_as_non_json` | 3963 | `--format json config path/show` exits 2, names the unsupported combination, and puts **no** JSON on stdout (§8) |

### `tests/end_to_end.rs` — 840 lines, 4 tests

`end_to_end_reports_only_inactive_artifact_projects` (`:114`) does *not* invoke
the binary; it drives the library directly with a fake `CargoRunner` through
`policy::resolve` → `discovery` → `resolve_workspaces` → `build_groups` →
`analyze_groups` → `report::render`, reproducing `run_scan`'s sequence by hand.
`a_literal_ignored_ancestor_yields_one_workspace_while_an_explicit_root_yields_two`
(`:285`) drives the same helper over two scopes and asserts the
ignore/unignore literal-path behaviour end to end.

**The previous revision's claim that "every end-to-end case here is scan-side" is
no longer true.** Two cleanup-side cases landed with the destructive-safety
work: `a_neighbouring_projects_sources_inside_a_covering_output_root_are_never_cleaned`
(`:619`) — the 0.2.0 defect, reproduced against a covering output root — and
`an_unrelated_sibling_workspace_stays_eligible_alongside_a_dangerous_one`
(`:801`), which asserts the safe sibling is still cleaned next to it. These
drive `cleanup::clean_with_roots_policy_selector` directly, not through
`main.rs`.

### `tests/common/mod.rs` — 262 lines, not a test target

(Cargo compiles only top-level `tests/*.rs`; each suite pulls it in with
`mod common;`.) Five fixture helpers: `set_path_modified` (`:24`), `backdate_file`
(`:98`), `inactive_project` (`:111`), `write_fake_cargo` (`:173`), and `cargo_calls`
(`:256`). It asserts nothing about the binary.

### What is well pinned, and what is not

**Well pinned — the machine contract, the new defaults, and log mode.** Argument
parsing in both invocation forms; the JSON envelope's
`schema_version` / `operation` / `scope` / `mode` by name, value and type; the
per-unit `reason_code`, `policy_disposition`, `outcome`, `selector_kind`,
`selector_value`; the `before_bytes` vs `output_union_before_bytes` distinction
under a selector; the exit-1 rule for `scope_blocked`; the editor's collapse to
exit 1 and its three post-edit checks; one trailing newline; stdout/stderr
separation for `--stats`; the `-i` refusals; **and, new in M012A/M012B, the
behaviour rather than the label** — bare invocation really does invoke Cargo and
really does remove bytes, and bare `--dry-run` really does invoke nothing. That
last distinction is the one the earlier revision of this document called
untested; it is now the best-covered thing in the file.

The key structural observation survives, but needs one refinement: **`output.rs`'s
*update* envelope has direct unit tests, and its *scan* and *cleanup* envelopes do
not — they are pinned only from the outside.** The four `update_tests`
(`src/output.rs:945-1065`) assert the real serialised bytes of
`update_json_stream` field by field, including that `changed` and `dry_run` are
the *only* two fields allowed to differ between the dry-run and mutating
documents (`src/output.rs:1000-1001`) — a discriminating assertion, not a
smoke test. `EnvelopeV1<ScanV1>` and `EnvelopeV1<CleanupV1>` have no such
direct coverage; their output is pinned by name, value and type only by
subprocess tests in `tests/cli_contract.rs`.

That is defensible — a subprocess test asserts the real bytes — but the coverage
is *incidental* to the assertions someone happened to write, not systematic. A
field nobody wrote an assertion for has no protection at all, which is exactly
how the `scope`-label bug survived a suite that *did* assert `scope` (§8).

**Only incidentally covered, or not at all:**

| Orchestration decision | Coverage |
|---|---|
| one JSON document on stdout for `clean --full` | **partial** — `EmitReport::No` suppresses the intervening scan report, and `tests/cli_contract.rs:3095` asserts the one-document property for a scan plus the *existence* of the `enum EmitReport` guard (`tests/cli_contract.rs:3124-3127`). No case runs `clean --full` itself, because that is a full filesystem walk; the guard assertion is a source-text check, so it survives a rename and would survive the enum being kept while every call site passes `EmitReport::Yes` |
| `clean --full` fail-closed on an incomplete scan (`:764-766`) | **partial** — `policy::full_cleanup_roots` (`src/policy.rs:440-464`) refuses a non-zero reconciliation code (`:443-448`) or a missing generation (`:449-455`), pinned by `full_cleanup_consumes_only_a_fresh_in_memory_generation` (`src/policy.rs:1059`); no live `clean --full` run exists, because it walks the whole machine |
| Exit code when a **Full** publish fails | **covered by construction** — the proven generation travels in `ScanOutcome::full_state` (`:1020`), so a failed save cannot change the outcome; the warning path is the same stderr line as before |
| Routine-vs-Full divergence of the *same* message | **none** — both are now inert, so the divergence is unobservable *and* untested |
| `--stats` **content** | partial — the cleanup line's four counter strings (`:476-479`) are never asserted |
| `collapse_roots` | **indirect** — private to this file, so the nest-collapse property is asserted through admitted-root CLI envelopes (`tests/cli_contract.rs:3560-3857`) and the admission-before-collapse ordering through `cleanup_candidates_preserve_symlinked_seed_identity_for_admission` (`src/policy.rs:1089`) |
| Root-source precedence | only via clap's conflict declarations (`src/cli.rs:115-118`) and the `invocation()` unit tests — the parse-level guarantee plus a pure mapping, but not the runtime `resolve_cleanup_roots` selection |
| `state generation last_full_at=` on stdout | **none** — no test runs cleanup with a `Some` generation in human format |
| Human vs JSON cleanup **ordering** | **none** — never co-executed; every assertion indexes `units[0]` |
| `out_of_scope_unresolved` | **none** — the field is not in the JSON envelope at all, and no contract test asserts the human "out-of-scope unprovable participant(s)" line. The ADR 007 *behaviour* is pinned (`tests/cli_contract.rs:3820`, `:3892`), but the *reporting* of it is not |
| Read-only config directory | **none** |
| The two hand-built `update` JSON documents | **none** — `src/main.rs:110-125` and `:151-166` |

**Honest assessment.** `main.rs` is the composition root — the only place the
two pipelines are wired — and composition is precisely what unit tests with fakes
do not cover. What M012A did well is push the *decisions* out to where they can be
tested: `Cli::invocation` is now a pure function with its own unit tests in
`src/cli.rs`, and the M012A behaviours are pinned through the binary by name.
What M014 and M017 did well is keep the *bounds* in the library:
`CleanupSelection` is `src/cleanup.rs`'s type, not `main.rs`'s, and the
anchor's fail-closed property is a unit test in `src/cli.rs:800`. What is still
un-testable in place is the *wiring*: which arm a given invocation reaches, and
what `run_scan` writes when called from inside `resolve_cleanup_roots`.
`end_to_end.rs` re-implements `run_scan`'s call sequence in the test body, which
is itself a structural risk: the sequence can drift from `main.rs` and the test
stays green. It has already drifted once, per its own doc comment (`:6-9`).
`main.rs` itself remains untested, and the decisions that belong to it rather than
to a module are the ones with no coverage.

## 11. Review checklist

1. **The `out_of_scope_unresolved` zeros in `main.rs` are unmeasured in two of
   four sites.** `src/main.rs:677` and `:702` build a `CleanReport` after the
   engine returned `Err`, so the engine's own out-of-scope count was discarded
   and the field is set to 0. The human report then says nothing about
   out-of-scope poison on a path that *did* traverse. The other two sites
   (`:375`, `:400`) are correct — no traversal happened. The JSON envelope has
   no such field either way (`src/output.rs:55-63`), which is the sibling
   finding tracked in [10-reporting §11](10-reporting.md) item 16 / G4; this
   item is the `main.rs`-side half of it, and the two are complementary rather
   than duplicate. Decide whether the two `run_engine_guarded` fallbacks should
   carry the engine's count, or whether "not measured" should render as "not
   measured" rather than as 0.
2. **The uncertainty filter has no `_` arm** (`src/main.rs:1095-1102`). It
   matches three of the seven `DiagnosticCategory` variants
   (`src/domain.rs:68-78`) by `matches!`, so a newly-added category is
   *silently* excluded from `uncertainty` and the exclusion reads as deliberate.
   `OutOfScope` is correctly excluded under ADR 007. Consider an explicit
   arm-per-variant form so the compiler sees the enumeration.
3. **The `update` JSON surface is a third copy of the envelope.** The two benign
   `update` outcomes hand-build `serde_json::json!` documents in the binary
   (`src/main.rs:110-125`, `:151-166`) while the mutating path uses the tested
   `output::update_json_stream` seam (`src/main.rs:190-193`). C022 pinned the
   machine contract for the seam; the two hand-built documents are in the one
   file with no tests. Moving them into `output.rs` would close the gap without
   a version bump.
4. **`config --format json` is a hard rejection** (`:230-234`) and
   `config --format log` withholds the path and the TOML text (`:239-254`). Both
   are deliberate. Confirm that an operator who scripts
   `cargo cleanme --format json config path` understands the exit 2 to be a
   *usage* rejection rather than a broken tool, and consider whether
   `config path` — a pure path lookup with no content to leak — should be
   allowed in JSON mode.
5. **`config edit` collapses the editor's status to 1** (`:274`), and the
   editor's own code appears only in prose (`:268-271`). The 0/1/2 contract is
   now uniform, which is the right call; confirm no consumer was relying on the
   old passthrough (`tests/cli_contract.rs:1103` now pins `Some(1)`).
6. **Bare invocation deletes bytes.** `src/cli.rs:315-321` makes a no-subcommand
   invocation a Routine Execute cleanup with no prompt. The safety argument is
   that `cleanup.rs` re-proves every candidate and deletion is always
   `cargo clean`, not that the default is conservative. Confirm that reading, and
   that `src/cli.rs:20-30` (the `about` text) stays accurate — it is the only
   place a user is told. **It is currently not accurate:** `src/cli.rs:27` still
   says "`cargo cleanme scan` with no ROOT is the full read-only machine
   reconciliation", a spelling M014 turned into a clap usage error
   (`src/cli.rs:86`). The machine-wide reconciliation is `scan --all`. The same
   drift is in the comment at `src/main.rs:1036` ("Rootless `scan` is Full"). Both
   are user- or reader-visible falsehoods about a *safety-relevant* spelling.
7. **`process::exit` and buffered stdout.** `src/main.rs:20`/`:34` skip
   destructors; safety comes from `Stdout` being a `LineWriter` plus
   newline-terminated writes, *not* from an explicit flush. The JSON update
   branch is the one place that uses `print!` rather than `println!`
   (`src/main.rs:190-193`), and it is safe only because the library seam
   supplies the trailing newline. A new buffered writer, a `write!`, or a
   non-newline-terminated final line removes that property silently — and the
   log mode's bounded-line contract (`src/output.rs:319`) is newline-sensitive
   too.
8. **`ScopeBlock.reason` is dropped by every report path except log.**
   `main.rs` passes `resolved.scope_label` to `output::cleanup`, which projects
   only `.message` (`src/output.rs:222`). The typed code survives only in the log
   line (`src/output.rs:387`). A schema-version decision, not a patch — see
   [10-reporting §11](10-reporting.md) item 12.
9. **stdout/stderr interleaving.** `src/main.rs:463-467` puts
   `state generation last_full_at=` on stdout, against the project's own rule
   (stdout = report, stderr = diagnostic). Human-only today (`:463` gates on
   `Human`), and log mode cannot see the generation at all; a landmine if `:463`
   is ever relaxed.
10. **Serialization errors use the wrong variant.** `src/main.rs:1291` and `:920`
    produce `configuration error: cannot serialize JSON report: …`, sending the
    user to `config.toml` when the fault is in report serialization
    (`src/config.rs:610-612` does the same). The third site that omitted the
    prefix was removed with the old empty-roots branch (§8), so this is now the
    only inconsistency left.
11. **Redundant root echo.** `src/main.rs:936-944` and `src/cleanup.rs:503-513`
    print the same root list, in two formats, as the first two lines of a human
    `clean` report (`selected_roots` is `roots.clone()`,
    `src/cleanup.rs:1334`).
12. **Human/JSON cleanup ordering divergence.** `src/cleanup.rs:561-568` sorts a
    *local* vec that `output::cleanup` (`src/output.rs:167-189`) never sees, so
    JSON `units[]` is insertion-ordered while human rows are size-descending. The
    scan side is self-consistent (`src/report.rs:44-49` sorts in place). Confirm
    the projections are not expected to agree positionally.
13. **`--include`/`--exclude` cannot clear a configured list.**
    `src/main.rs:333-342` falls back to config on an empty CLI vec. Deliberate
    but undocumented; clearing needs a distinct flag, not a reinterpretation of
    the empty list.
14. **`collapse_roots` canonicalization is best-effort.**
    `src/main.rs:968` uses `unwrap_or(p)`, so a missing or unresolvable root
    passes through unvalidated. The engine's own `collapse_roots`
    (`src/cleanup.rs:2647-2673`) is the fail-closed one and is what actually
    guards the boundary. Confirm the permissive version in `main.rs` is
    understood to be a *pre*-pass, and that the `sort()` at `src/main.rs:970` is
    understood to be *required* by the containment filter at `:974`.
15. **Explicit `ROOT` skips `policy::resolve`.** `src/main.rs:731-735` never
    calls it (only `Maintenance` does, `:773-779`), so the exists/is-a-directory/
    is-not-a-symlink checks at `src/policy.rs:32-41` never run for `clean ROOT` —
    a promise `src/cli.rs:428-430` makes while naming a different layer. The
    engine's own validation (`src/cleanup.rs:2627-2645`) is what actually holds
    the line there.
16. **`main.rs` is almost untested.** Four `#[cfg(test)]` cases here; 21 in `output.rs`; 7 in
    `report.rs`. The machine contract *is* pinned by `tests/cli_contract.rs`
    (§10), so the gap is specifically the *orchestration* decisions — which arm a
    given invocation reaches, what the `clean --full` scan writes, and the two
    hand-built `update` documents.
17. **The progress-clear ordering is untested and recorded as such.**
    `src/main.rs:461-462` clears the bar before the fallible cleanup call, and
    the ordering is unreachable from the suite: the bar only draws on an
    attended terminal, and the failing-cleanup contract test passes
    `--no-progress`. C023 §11.4 records it as an open coverage gap needing a pty
    harness. See
    `plans/closure/distribution-release-update/c023-status.md`.
18. **The `EmitReport` guard is asserted by a source-text check, not a
    behavioural one.** `tests/cli_contract.rs:3124-3127` asserts that
    `src/main.rs` still contains the string `"enum EmitReport"`. That is the
    repository's own §4 lesson in miniature — *a green test proves only that its
    own premises hold*. The premises here are "the type is still spelled that
    way" and nothing about what any call site does with it. Renaming the enum
    breaks the test (good), but keeping the enum while every call site passes
    `EmitReport::Yes` leaves it green (bad) — and the bug it guards against
    (two JSON documents on one stdout, §4) would return. A behavioural
    assertion needs a `clean --full` case, which the bounded suite cannot
    afford; a cheaper substitute would be to assert that the *call site* passes
    `EmitReport::No` (`src/main.rs:758`), which is the thing that actually
    matters and is a one-line source check that cannot be satisfied by a
    vestigial enum.
