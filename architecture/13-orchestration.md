# Orchestration — argument dispatch and the two pipelines

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** re-derived against `src/main.rs` at 2500 lines. The substantive
> changes since the previous revision of this file are: **ADR 007** made the
> proof boundary the *selected scope*, so an out-of-boundary unprovable is
> reported and never blocks; **M014** added the CWD-anchored default, `scan
> --all`, and the `-i` selection surface; **M017** removed the island machinery
> this document's predecessor still described; **ADR 003** made bare
> `cargo cleanme` a Routine *execute* cleanup and `--dry-run` the zero-spawn
> simulation preview; and `config`/`update` learned `--format log` while
> `config --format json` became a hard rejection; and **ADR 009** joined the
> working directory to the maintenance scope; and **M020** added the global
> `--verbose` flag and reshaped the human output surface around it; and
> **M018** made the Full cleanup's proof boundary the *learned root*; and
> **M019** made the root the cleanup front door. Every citation below was
> re-read at the cited line; none was shifted arithmetically.

`src/main.rs`, 2500 lines, twelve inline tests. The only module that reaches every
other one, and the only place the two pipelines are stitched together.

## 1. Responsibility

`main.rs` owns argv parsing and the exit-code contract (`:11-39`), collaborator
wiring, the reconciliation branch deciding whether persisted state is rewritten
(`:1646-1766`), and the choice of output surface. It owns **no domain logic**:
scope to `policy::resolve` (`:947-953`, `:1053-1062`), classification to
`policy::classify_cleanup_roots` (`:772`, `:988`, `:1065`, `:1091`), authorization
and mode semantics to `cleanup::clean_with_roots_policy_selector_guarded`
(`:750-758`), and traversal completeness to
`ManifestDiscovery::coverage_complete` (`:1607`), whose one value feeds both the
Full reconciliation (`:1683`) and the exit status (`:1965`).

**It declares five types.** `RunOptions` (`:65-71`) is the process-wide quadruple
`no_progress` / `stats` / `format` / `verbose`, built once in `run` (`:72-120`) and
passed to both report-producing operations. `ResolvedCleanup` (`:337-360`) carries the
resolved roots, the machine scope label, the discovery-state generation, the
remembered omissions, a typed whole-scope block, the **unproven learned roots**
(M018, `:350-354`), the coverage-verification elapsed time, and the `automatic`
flag that decides whether a fatal `InvalidRoot` may be reclassified rather than
propagated. `SelectionOutcome` (`:635-643`) is the two-way result of the `-i`
step. `EmitReport` (`:1505-1516`) is the two-valued switch that keeps an internal
scan off stdout. `ScanOutcome` (`:1518-1972`) carries the exit code plus the
proven Full generation and whether its publication survived. All exist to make
the "one resolved intent per invocation" property structural: because
`Cli::invocation` collapses every cleanup spelling into one `CleanupRequest`
(`src/cli.rs:472-558`), there is exactly one place where mode, scope, and
policy are bound, and one place that emits the report.

**It has twelve inline tests**, not none: `grep -cE '#\[cfg(test)\]|#\[test\]'
src/main.rs` → `10`, which is the nine `#[test]` functions plus the one
`#[cfg(test)]` module attribute (see §10).

Its top `use` block is ten lines (`:1-10`) and names `cli` (with `CleanupRequest`,
`CleanupScope`, `Cli`, `ConfigCommand`, `Invocation`, `OutputFormat`,
`ScanIntent`), `config` (with `ConfigPathResolver`), `domain` (`ScanScope`),
`error` (`AppError`), and `std::collections::BTreeMap`. Three function-scoped
imports live inside `run_scan` — `use cargo_cleanme::{domain, progress};`
(`:1491`) and `use std::time::SystemTime;` (`:1492`), plus
`use cargo_cleanme::discovery_state::StateLoad;` (`:1605`) — and everything
else is named at the point of use (`cargo_cleanme::update::update` `:102`,
`cargo_cleanme::workspace::SystemCargoRunner` `:1595``,
`cargo_cleanme::progress::IndicatifRenderer` `:483`). Consequence: renaming a
library function breaks this file at many scattered sites, and there is no
single list of what it depends on. Modules reached: `cli`, `config`, `error`,
`domain`, `discovery`, `workspace`, `discovery_state`, `cleanup`, `update`,
`editor`, `interactive`, `progress`, `report`, `output`, plus `serde_json`
(`:118``, `:1283``) and `directories` (`:1091``).

**Twenty-five free functions, three structs and two enums**, all private:

| Item | Lines | Role |
|---|---:|---|
| `main` | `11-37` | parse, dispatch, fatal rendering, `process::exit` |
| `operation_name` | `41-50` | `Invocation` → stable `op=` token for the log fatal line |
| `error_code` | `54-62` | `AppError` → stable `reason=` code |
| `run` | `72-94` | config path, `RunOptions`, five-arm dispatch |
| `run_update` | `96-222` | plan, JSON / dry-run four-liner / one-line success |
| `run_config` | `224-298` | `path`, `show`, `edit` + three post-edit checks |
| `run_cleanup` | `321-500` | the cleanup pipeline (§5) |
| `resolve_candidate_selection` | `533-602` | the `-i` step and the anchor resolution (§5) |
| `interactive_outcome` | `609-622` | the `-i` session's three endings, folded into the exit code |
| `run_engine_guarded` | `635-726` | admission→validation race defence (C028, §5) |
| `resolve_cleanup_roots` | `732-879` | scope → admitted roots + label + generation (§5) |
| `working_directory_scope_paths` | `888-895` | the two operator roots ADR 009 adds to the Routine scope |
| `working_directory_entries` | `903-910` | the operator roots as discovery candidates |
| `working_directory_is_unreached` | `937-944` | whether the Routine scope already reaches the operator's directory |
| `roots_are_related` | `947-952` | canonical-or-raw containment test for the ADR 009 join |
| `admit_automatic` | `958-1012` | provenance admission before collapse (C028, §5) |
| `emit_cleanup` | `1018-1067` | five-branch format dispatch for a cleanup report |
| `human_options` | `1074-1083` | `RunOptions` + roots → the human render options (M020) |
| `human_cleanup_roots` | `1089-1095` | the roots a cleanup report may spell paths relative to (M020) |
| `human_scan_roots` | `1098-1113` | the roots a scan report may spell paths relative to (M020) |
| `absolutize_for_display` | `1120-1122` | absolutises a root the scope kept as the operator spelled it (M020) |
| `report_has_nothing_to_report` | `1131-1138` | whether a cleanup report has any row to show |
| `collapse_roots` | `1140-1154` | canonicalize, sort, dedupe, nest-collapse (§6) |
| `scope_label` | `1164-1170` | `ScanScope` → JSON `scope` string |
| `run_scan` | `1199-1620` | the scan pipeline (§4, §7) |
| `RunOptions` | `63-71` | process-wide output options |
| `ResolvedCleanup` | `300-319` | admitted roots + label + generation + block |
| `SelectionOutcome` | `504-512` | `-i` outcome: chosen, cancelled, or nothing to clean |
| `EmitReport` | `1172-1183` | whether a scan writes its report to stdout |
| `ScanOutcome` | `1185-1197` | exit code plus the proven Full generation (C028, §7) |

The two log helpers — `operation_name` and `error_code` — are new in M012B and
exist only to serve `main`'s fatal path. They are why `invocation` is captured
before the move into `run` (`:72`), and `format` after it (`:68`). Neither helper
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

**`main` now parses argv itself** (`:11`), and `run` receives an already-parsed
`Cli`. This is M012B's consequence, not a style choice: a fatal error can only be
rendered format-aware if the format is known before the operation runs, and a
`try_parse` buried inside `run` could not tell "clap rejected this" from "the
filesystem is broken". The price is two values captured purely for the failure
path — `invocation` at `:17`, before the move into `run`, and `format` at `:68`.

`Err(e)` prints one line on **stderr** and exits **2**; every `AppError` variant
(`src/error.rs:3-16`) exits 2. A command can also return `Ok(1)`. **Exit 1 means
"ran, and something failed"; exit 2 means "could not run."** The only `Ok(1)`
producers: `:282`` (editor failed), `:424`` (automatic-scope block), `:533-537``
(cleanup partial failure or scope block), `:1851-1857`` (scan Full incomplete).

| Code | Meaning | Produced by |
|---:|---|---|
| 0 | Success | `:1896-1900``; Update `:149``, `:190``, `:226``; Config `:302``; empty-roots cleanup `:455``; cleanup `:533-537`` when `failed == 0 && scope_blocked.is_none()` |
| 1 | Ran, partially failed | editor failure `:282``; automatic-scope block `:424``; cleanup `:533-537``; scan `:1889-1895`` |
| 2 | Could not run | any `AppError` via `main` `:11`; clap usage errors (clap's own path) |
| 0 (clap) | `--help` / `--version` | clap, not this file |

| Command | Codes | Decided at |
|---|---|---|
| bare `cargo-cleanme` / `clean` / `clean --known` | 0, 1, 2 | `:43-47` |
| `scan ROOT` / `scan --known` / `scan --all` | 0, 1, 2 | `:1851-1862`` |
| `clean ROOT …` | 0, 1, 2 | `:533-537`` |
| `clean --full …` | 0, **2**, 1, 2 | `:959-967`` then `:533-537`` |
| `config path` / `show` | 0, 2 | `src/main.rs:249-253``, `src/main.rs:325`` |
| `config edit` | 0, 1, 2 | `:264-300`` |
| `update [--dry-run]` | 0, 2 | `:101-227`` |
| any (clap usage error) | 0 (help/version), 2 | clap |

**The `clean` row has no "no root source" case any more.** Before M012A, `clean`
with no `ROOT`, no `--known` and no `--full` was a usage error
(`AppError::Config("clean requires ROOT, --known, or --full")`). It is now the
maintenance scope — the same intent as bare invocation — so there is no user-facing
way to reach that 2.

**`clean --full` moved a failure from 1 to 2.** A Full scan that does not fully
complete now raises `AppError::Config` from
`policy::full_cleanup_roots` (`src/policy.rs:498-503`) → exit 2, instead of
returning the scan's own code silently. The old behaviour handed the caller a
bare code with no indication that *no cleanup ran*; the new one says so. The
trade-off is real: "incomplete cleanup scope" now splits across two codes
depending on whether the incompleteness was detected during scan (2) or during
unit execution (1). Both are fail-closed.

`update` can never return 1. A benign no-op — `UpdateError::AlreadyCurrent`
or `UpdateError::NewerThanPublished` — is reported in prose and exits **0**
(`:150``, `:191``), so schedulers do not alert on "already up to date", matching
`docs/TROUBLESHOOTING.md` calling that state success. Every other
`UpdateError` still converts via `From<UpdateError> for AppError`
(`src/update.rs:393`) to `AppError::Provenance` or `AppError::Update`,
both exit 2. Before M012B the benign variants shared that exit-2 path, which is
what the previous revision of this file recorded as a defect.

### Clap's errors bypass `AppError` entirely

`Cli::parse_normalized()` (`:15`) = `Self::parse_from(normalize_cargo_argv(args_os))`
(`src/cli.rs:451-454`), and unlike `try_parse_from` it **cannot return `Err`**:
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

`run` matches on **`Invocation`**, not on `Option<Command>` (`:43-58`):

```rust
match cli.invocation() {
    Invocation::Update { dry_run } => run_update(dry_run, options.format),
    Invocation::Config(command)    => run_config(command, &path, options.format),
    Invocation::Scan(intent)       => run_scan(intent, &path, options, EmitReport::Yes,
                                              ReconciliationMode::IncompleteDiscard)
                                         .map(|outcome| outcome.code),
    Invocation::Cleanup(request)   => run_cleanup(request, &path, options),
    Invocation::UsageError(message) => Err(AppError::Config(message)),
}
```

**The dispatch table moved into `cli.rs`.** `Cli::invocation()`
(`src/cli.rs:463-522`) is now the single place that turns parsed argv into a
resolved intent, and `main` no longer inspects `command` at all — it does not even
read the field, apart from lifting the three globals at `:74-79`. This is M012A's
structural change and it is the right one: it makes "what did the user ask for" a
pure function of argv, testable without a subprocess, and it removes the
`None`-falls-through-to-scan fallthrough that used to live here.

**`Invocation` now has five variants** (`src/cli.rs:409-425`). The fifth,
`UsageError(String)`, exists because the compiler cannot see clap's invariant
that `scan` always carries a scope selector. Its two alternatives were both
wrong: defaulting to Full would silently run the machine-wide reconciliation the
flag exists to gate, and panicking would exit 101, outside the 0/1/2 contract.
`run` maps it to `AppError::Config` (`:72`) → exit 2, and `operation_name` maps
it to the unreachable token `"usage"` (`:48`).

Ordering consequence: **the config path is still resolved for every command,
including `update`** (`:46` runs before the match), so if the platform config
directory is unavailable (`src/config.rs:84-86`) then even `update` and `config
path` fail with exit 2 before reaching their arms.

### What `invocation` resolves

| Input | `Invocation` | Source |
|---|---|---|
| no subcommand | `Cleanup(Maintenance, Execute)` | `src/cli.rs:472-490` |
| `cargo cleanme ROOT` | `Cleanup(Root(root), …)` | `src/cli.rs:475-476` |
| `cargo cleanme --full` | `Cleanup(Full, …)` | `src/cli.rs:473-474` |
| `scan ROOT` | `Scan(Explicit(root))` | `src/cli.rs:482` |
| `scan --known` | `Scan(Maintenance)` | `src/cli.rs:483` |
| `scan --all` (or hidden `--full`) | `Scan(Full)` | `src/cli.rs:487` |
| `scan` (bare) | clap usage error → exit 2 | `src/cli.rs:244`, `src/cli.rs:489-492` |
| `config path/show/edit` | `Config(*)` | `src/cli.rs:498` |
| `update [--dry-run]` | `Update { dry_run }` | `src/cli.rs:504-511` |
| `clean …` (hidden alias) | `Cleanup(<scope>, <mode>, <overrides>)` | `src/cli.rs:520-526` |

Five of those rows are worth calling out:

- **`None` is a cleanup.** A bare `cargo-cleanme` becomes
  `CleanupScope::Maintenance` with `CleanMode::Execute`, i.e. a Routine
  maintenance run that will actually delete eligible artifacts. It is *not* a
  read-only scan. The root `command` doc string says so in prose
  (`src/cli.rs:103-121`) and the enum comment names the intent
  (`src/cli.rs:318-323`).
- **The root is the cleanup front door (M019).** `cargo cleanme ROOT` is a
  bounded cleanup of `ROOT`, and `cargo cleanme --full` is the Full cleanup
  (`src/cli.rs:47-52`). The `clean` subcommand survives as a **hidden alias**
  (`src/cli.rs:270-296`) and both spellings reach the same `CleanupRequest`
  through `cleanup_request` (`src/cli.rs:540-629`). `cargo cleanme .` is
  now a bounded cleanup of `.`, not a usage error.
- **`clean` with no scope selector is the same thing.**
  `src/cli.rs:607-611` maps `(None, false, _)` to `CleanupScope::Maintenance`,
  and `src/cli.rs:1585` asserts that `["cargo-cleanme", "clean"]` and
  `["cargo-cleanme"]` produce identical `invocation()` values. This is why
  `main.rs` needs no "no root source" error branch (§2).
- **A cleanup flag handed to a read-only subcommand is a usage error
  (M019).** `reject_cleanup_flags_for` (`src/cli.rs:643-684`) rejects `--full`,
  ROOT, and the seven policy overrides on `scan`, `config` and `update` before
  those arms run. A discarded flag would look exactly like a honoured one.
- **A bare `scan` is no longer a thing.** M014 made the scope selector
  mandatory: `scan` requires exactly one of ROOT / `--known` / `--all`,
  expressed as a required `ArgGroup` (`src/cli.rs:244`) so that the requiredness
  and the mutual exclusion cannot drift apart. The machine-wide Full
  reconciliation is now `scan --all`, one explicit flag away, rather than the
  zero-argument default it was. `scan --full` survives as a hidden alias
  (`src/cli.rs:260-261`).

Mode resolution is a free function rather than inline matching
(`src/cli.rs:432-444`): `dry_run || dryrun_legacy` → `Simulate`, else
`cargo_preview` → `CargoPreview`, else `Execute`. **Execute is the default.** The
hidden compatibility aliases (`--dryrun`, `--yes`) are folded into the same two
canonical modes; `--yes` is deliberately never bound (`src/cli.rs:512-515`),
since Execute is now reachable without it and binding it would print nothing
useful.

**`Update`** (`:84-210`) — `update::update(dry_run)?` then, per format, the JSON
stream (`:194-201``), the log line (`:202-214``), the human dry-run four-liner
(`:215-222``), or the one-line success (`:223-224``); `Ok(0)` at `:226``. The two
benign-error arms repeat the same three-way shape and return `Ok(0)` at `:149``
and `:190``. `--format` **is** honoured on every arm, log mode included.

**`Config`** (`src/main.rs:240-326`), three arms. **`Path`** (`src/main.rs:260-266`)
prints the path and never loads, creates, or validates the file, so it works when
the file is absent or corrupt — it needs only the *path* to be derivable. **`Show`**
(`src/main.rs:267-274``) is `config::show(&config::load_or_create(path)?)?` and
therefore **creates** the file if missing (`src/config.rs:405-419`) — a write side
effect. **`Edit`** (`src/main.rs:275-323``) does `config::ensure_exists`
(create-if-absent *without* requiring current contents to parse,
`src/config.rs:422-427`), resolves `$VISUAL`/`$EDITOR` (`src/editor.rs:7-8`),
spawns, then three checks.

**`--format json` is now rejected for every `Config` arm** (`src/main.rs:249-253`):
`config` has no JSON envelope, and emitting a raw path or TOML text on a
`--format json` stdout would break the one-JSON-document automation contract,
so the combination exits 2 with a message that names it. `--format log` **is**
honoured (`src/main.rs:258``): the three arms print `output::log::config("path" | "show" |
"edit")` (`src/main.rs:262``, `src/main.rs:270``, `src/main.rs:319``) and the path or TOML text is withheld, because
a path in a retained history pane is exactly what that format refuses.

**The three post-edit checks.** (1) *Non-zero editor exit* (`src/main.rs:286-306``) prints
`cargo-cleanme: editor exited with {status}; config was not reverted: {path}` to
stderr — in `--format log` it prints the single typed line
`output::log::fatal("config", "config")` (`src/main.rs:296`) instead of that prose,
because that format promises no paths — and returns **`Ok(1)`** — the editor's own
status is **never** propagated as our exit code, because the exit contract is
0/1/2 and an arbitrary editor status is not one of them; pinned by
`tests/cli_contract.rs:1106`. (2) *File deleted* (`src/main.rs:307-314`): `load` treats an
absent file as "all defaults" (`src/config.rs:96-98`), so a deleted config would
silently reset every policy → `Err(AppError::Config(…))` → exit 2. (3) *File
modified into an invalid state* (`src/main.rs:315-317``): `config::load` re-wrapped as
`AppError::Config("edited config {path} is invalid: {e}")`; pinned by
`tests/cli_contract.rs:1106`, which also asserts the bad content is left in
place (the message says "was not reverted"). Checks 2 and 3 exit 2; check 1
exits 1. `Ok(0)` at `src/main.rs:325`` is reached only for `Path`, `Show`, and a successful
`Edit`.

**Bare invocation is Execute, and that is a standing decision.** Nothing in
`main.rs` gates it: there is no confirmation prompt, and `--dry-run` is opt-in.
The safety property that makes this acceptable is not in this file at all — it is
that every candidate must independently prove exclusive ownership before deletion
and that deletion is always `cargo clean` (§5). But the *default* is worth naming
plainly: a user who types `cargo cleanme` in a project directory gets bytes
deleted, not a report. If that is ever contentious it is a one-line change at
`src/cli.rs:415-421`, and `main.rs` would not need to move.

**The `about` text is the user-facing statement of that default.** `src/cli.rs:103-116`
describes the bare invocation correctly (`src/cli.rs:105-109`), and **the false
sentence is gone**: the `long_about` now says that `scan` **needs a scope** and
that `--all` is the full read-only machine reconciliation
(`src/cli.rs:117-120`). M014 made a bare `scan` a clap usage error
(`src/cli.rs:244`), and C040 rewrote the text to match — the manpage is rendered
from that same `long_about`, so the help output and the manpage cannot drift
apart again. The same drift **does** survive one file over, in the comment at
`src/main.rs:1552`, which still opens "*Rootless `scan` is Full*" — the
*substance* is right (the Full intent ignores a configured `scan.root`), but the
spelling it describes no longer exists.

## 4. The scan pipeline

`run_scan` — `src/main.rs:1524-1972`, **five** parameters: `intent: ScanIntent`,
`config_path`, `options: RunOptions`, `emit_report: EmitReport`, and — since M018 —
`reconciliation_mode: ReconciliationMode`. Since C028 it returns `ScanOutcome`
(`:1518-1522`) — the exit code plus the proven Full
generation (§7). In order:

1. `scan_start` (`:1788`, the recency reference) and `wall_start` (`:1788`, behind
   `--stats`); `config::load_or_create(config_path)?` (`:1536``) — may create the
   file.
2. `intent` → `(root, full)` (`:87-91`). Rootless Full is `(None, true)` and
   *never* consults a configured `scan.root`; `scan --known` is `(None, false)`;
   `scan ROOT` is `(Some(root), false)`. **A relative ROOT is folded against
   the invocation directory *lexically* here** (`cli::absolutize_root`,
   `src/main.rs:1567`), exactly as the cleanup path folds it. The two
   spellings of one operation have to agree: the OS resolves `..` physically —
   through symlinks — while Cargo resolves a manifest path lexically, so a root
   that crossed a symlink used to make the scan discover a manifest it could
   then not resolve, and report an empty inventory with exit 0 while the
   cleanup path named a different directory entirely.
3. `policy::resolve(domain::ScanRequest { cli_root: root, full }, &c.scan)?`
   (`:1547-1553``); `recency` bound at `:1576``.
4. `IndicatifRenderer::new(!show, options.verbose)` (`:1582``) where `show` =
   `format == Human`
   **and** `should_show_progress(no_progress)`; `observer.phase(Discovery)`
   (`:1548``).
5. `discovery::discover_manifests_with_attribution(&policy, observer, stats)`
   (`:1556-1560``); a discovery error clears the bars before escaping, otherwise
   the drop would finish them into retained lines. `discovery_nanos += elapsed`
   saturating (`:1575-1577``); `_resolution_elapsed` is measured at `:1605`` and
   **discarded**.
6. `uncertainty` extracted by filtering `PermissionDenied | Metadata |
   Vanished` (`:1578-1590``); `manifests_found` set (`:1593``).
7. `observer.phase(Resolution)` (`:1595``) and
   `workspace::resolve_workspaces(…)` with `SystemCargoRunner` (`:1634-1641``),
   mutating `counters` and `diagnostics`.
8. State reconciliation (`:1653-1765``) — §7.
9. `build_groups(&workspaces)` (`:1772``), `phase(Analysis)` (`:1773``),
   `units_total(Analysis, groups.len())` (`:1774``).
10. `clock_cutoff` via `checked_sub(recency)` (`:1792-1802`); a clock-overflow
    error clears the bars before escaping, like the discovery error in step 5.
11. `analyze_groups(&workspaces, groups, scan_start, clock_cutoff, recency, …)`
     (`:1788-1797``), mutating `counters` and `diagnostics`.
12. `full_incomplete` (`:1799``); `domain::ScanReport` built (`:1843-1861`),
    carrying `visited_entries` through.
13. `phase(Reporting)` (`:1818``), `finish_and_clear()` (`:1819``), deferred state
    notices (`:1823-1825``), **output always
    emitted** (`:1828-1848``), then diagnostics to stderr (`:1852-1937``) and
    `--stats` to stderr (`:1942-1959``).

Cargo locate/metadata failures remain one diagnostic per manifest in the report
and JSON. Human stderr groups identical bounded Cargo causes, shows at most three
sanitized representative paths per cause and at most eight causes; the exact
unresolved participant ledger and counters are not compressed. Other human
diagnostics retain their own ten-item display cap. This is presentation only and
does not alter scan coverage or cleanup eligibility.

**That fan-out is now the verbose surface.** The one-line count
(`:1852-1858``) prints on every non-log run, but the detail behind it — the
grouped Cargo causes, the example paths, the non-Cargo lines — is gated on
`options.verbose` (`:1860``) and is debugging information, not a result. The
count line's wording did not change.

**Step 2 deserves the emphasis.** Before M012A a rootless `scan` resolved through
`policy::resolve` with `full: false`, which meant a configured `scan.root` could
silently narrow the canonical reconciliation command — and, because reconciliation
is the only thing that expires learned roots, could *forget* roots outside that
one root. Configuration must not be able to do that to the one operation whose
job is to decide what the whole machine looks like. `scan --known` survives as the
read-only inventory over the maintenance scope, which is where a configured root
still wins as an Explicit override (`src/policy.rs:31-43`). M014 did not
re-open this: the Full intent is now reached only through an explicit `--all`,
and the requirement is spelled out at `src/main.rs:1552-1555`.

**Step 6, in detail.** `uncertainty` (`:1616-1628``) filters
`PermissionDenied | Metadata | Vanished`, then `.filter_map(|d| d.path.clone())`.
It reads `discovered.diagnostics`, the **pre-resolution** vector — *not* the
`diagnostics` vec `resolve_workspaces` goes on to mutate at `:786-800`, so a
permission/metadata diagnostic raised during `cargo metadata` resolution cannot
reach `uncertainty` and cannot protect a learned root. And `filter_map`
silently drops matching diagnostics carrying no path, so `uncertainty` is
path-addressed only.

**The filter is a `matches!` over three of the seven `DiagnosticCategory`
variants, with no `_` arm** (`:1581-1588``). That is a live hazard: adding a
fourth "something might be wrong here" category to
`src/domain.rs:68-81` compiles cleanly and is *silently* excluded from
`uncertainty`, so a newly-uncertain path would not protect a learned root and
the exclusion would read as deliberate. `OutOfScope` is correctly excluded — ADR
007 requires an out-of-boundary unprovable to be reported and never to block,
and `uncertainty` is exactly the "retain intersecting old roots" input.

**Step 10.** `checked_sub` returns `None` if the clock is earlier than
`1970-01-01 + recency_seconds`, killing the scan with exit 2 and a
*config-flavoured* message. `config::load` already range-checks
`recency_seconds` (`src/config.rs:99-106`), so only a mis-set clock causes this.
The same condition is handled non-fatally in `now_seconds()`
(`src/discovery_state.rs:461-466`, `unwrap_or_default()` → 0) — one root cause,
two policies. `tests/end_to_end.rs` uses the *unchecked* subtraction, so the
production guard is untested at the integration level.

### Report emission is an explicit parameter, not a side effect

`run_scan` takes `emit_report: EmitReport` (`src/main.rs:1524-1972`, passed at
`:90`). `Invocation::Scan` passes `EmitReport::Yes` (`:90`); the Full
reconciliation `cargo cleanme --full` runs to prove a fresh generation before
consuming it (`:1020`) passes `EmitReport::No`.

That parameter is not a formatting convenience. It is the only thing standing
between `clean --full` and a **double stdout document**, and the JSON envelope is
built on exactly one document per invocation. So `EmitReport::No` suppresses the
report and *nothing else*: diagnostics (`:1852-1937``) and `--stats`
(`:1942-1959``) still reach stderr, state reconciliation still runs, and every
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
`tests/cli_contract.rs:3116` (`scan_emits_exactly_one_json_document`) proves the
one-document property for a scan and then reads `src/main.rs` as text and
asserts that the source still contains `"enum EmitReport"`
(`tests/cli_contract.rs:3145-3148`) — the test's own doc comment
(`tests/cli_contract.rs:3101-3114`) records the reasoning, that a real
`clean --full` case would be a full filesystem walk the bounded suite cannot
afford. That is a *name* check, not a *behaviour* check: renaming the enum, or
keeping the enum while every call site passes `EmitReport::Yes`, would leave the
assertion green. See §10 and §11 item 18.

## 5. The cleanup pipeline

`run_cleanup` — `src/main.rs:427-631`. It no longer branches on a `Clean` command
variant; every cleanup spelling arrives as a `CleanupRequest` whose `scope`,
`mode`, `selection` and `interactive` flag were resolved by `Cli::invocation`
(`src/cli.rs:472-558`).

**Mode** is taken straight from the request (`:345``). There is no mode-selection
logic left in this file — no `yes`/`dryrun` matching, no `let _ = dry_run;`
discard, and no defaulting. That entire hazard is gone because the parse-time
resolution is the only place that decides. `CleanMode` itself is
`Execute | Simulate | CargoPreview` with **no `Default` impl**
(`src/cleanup.rs:36-47`): the mode must be chosen, and every spelling that reaches
here chose one.

**The one surviving semantic hazard** is upstream, in the flag names. `--dry-run`
is now *simulation* (the project's own full ownership-proof pass, no `cargo clean`
spawned — `src/cleanup.rs:39-41`) and Cargo's own preview is `--cargo-preview`
(`src/cleanup.rs:42-45`). The flags are two hyphens apart in meaning and one word
apart in spelling. clap makes them pairwise exclusive
(`src/cli.rs:257-262`) and the source comments are explicit
(`src/cli.rs:255-262`), but a user typing the wrong one still gets a different
operation, not an error. `--dryrun` and `--yes` survive as hidden aliases
(`src/cli.rs:206-212`) that map onto `Simulate` and the (now default) `Execute`
respectively.

**Root selection** is `resolve_cleanup_roots` (`:956-1126`), matching on
`CleanupScope` — three arms, and *no error arm*, because M012A deleted the
`"clean requires ROOT, --known, or --full"` case. Since C028 every arm
classifies before collapsing, and automatic arms remember omissions and
convert uncertainty into a block rather than a fatal error:

| Scope | Behaviour | Label |
|---|---|---|
| `Root(p)` (`:965-985`) | `cli::absolutize_root(p)` classified `Explicit`; any defect is fatal | `"explicit"` |
| `Full` (`:986-1029`) | run a Full scan, then consume the in-memory generation via `policy::full_cleanup_roots` — never a disk reload | `"full"` |
| `Maintenance` (`:1034-1137`) | configured `scan.root` takes the strict explicit path; otherwise raw Routine candidates (`policy::routine_cleanup_candidates`) classified `Automatic` | `"routine"` or `"explicit"` |

### The Full branch and the coverage boundary (M018)

The `Full` arm (`:986-1029`) is the most intricate block in the file, and it
changed shape under M018. Before, an incomplete Full reconciliation was a
**refusal**: `policy::full_cleanup_roots` returned an error and the cleanup
never ran. Now it is a **verdict**, and the arm does four things in order:

1. **Run the Full scan, silently** (`:998-1004`). The scan exists to produce a
   fresh in-memory generation, not to be reported — emitting its document
   would put two JSON documents on one stdout. Its reconciliation mode is
   `ReconciliationMode::IncompleteRetain`, because an incomplete traversal
   must not rewrite the persisted learned state but the cleanup still needs a
   generation to judge coverage against.
2. **Verify each learned root's coverage** (`:1018-1022`). The verification pass
   (`verify_learned_root_coverage`, `:903-945`) runs **only** when the platform
   walk was incomplete; a completed platform walk read every learned root's
   subtree and proves them by definition.
3. **Ask the gate for a verdict** (`:1023-1027`):
   `policy::full_cleanup_roots(outcome.code, outcome.full_state, coverage)` —
   the coverage slice is the third argument M018 added.
4. **Turn the verdict into a scope** (`resolve_full_verdict`, `:875-886`).

`resolve_full_verdict` is where the ADR 006 constraint is honoured, and it is
worth reading closely. **Every** learned root — proven or not — is admitted as
a cleanup **root** (`:881-886`). Withholding an unproven root would narrow the
*discovery* universe: the other claimant of a shared output would never be
seen, and those bytes would be promoted from `Shared` to `PrivateBounded`. The
unproven root's workspaces are removed from the *candidate* set by the
selection (`cleanup.rs`'s `CleanupSelection::ExcludeRoots`), never from the
discovery set.

**The fail-closed condition is about the proof, not the admitted roots.** If
the gate proved *no* root at all while there were roots to judge, the
operation proves nothing, and exiting 0 with zero rows would be the M012A
silent no-op. `zero_proven_block` (`:822-824`) builds the standard
unproven-ownership scope block, and `resolved.blocked` is set from it
(`:889-891`).

**The exit code says the boundary was incomplete** (`:849-858`):
`cleanup_exit_code` returns 1 when any root was unproven, even when every
root that *could* be proven was cleaned successfully. An unattended script
must not read a partial Full cleanup as a complete one.

**The unproven roots are disclosed, not hidden** (`:390-402`): one stderr line
per root, naming the root and the reason, in every format except `log` —
which carries only bounded `key=value` lines and no paths.

Two details in the `Maintenance` arm are worth more than their line count.
First, it does **not** keep only `ScanScope::Routine(roots)` and discard the rest
— it maps every variant: `Routine` → raw-candidate admission, `Explicit` and
`ExplicitRoots` → strict `"explicit"`, and `Global` → a hard
`AppError::Config("maintenance scope resolved to a global scan, which is
unreachable; refusing to treat it as an empty cleanup scope")` (`:1122-1128`).
Discarding the configured root produced a bare maintenance run that reported a
successful Explicit no-op it never performed; treating `Global` as "nothing to
do" would fail open on a scope that covers the whole machine. Both are the right
call.

Second, **classification precedes collapse** (`admit_automatic`, `:1211-1235`):
admitted paths only are collapsed (`:1272`), omissions are remembered for the
late pre-spawn recheck, and symlink/unreadable automatic roots become a
whole-scope `ScopeBlock` carried on `ResolvedCleanup.blocked`
(`:1239-1255`). A stale learned path therefore can no longer reach the
engine's strict validation and abort the run — the C028 defect.

**The working directory joins the Routine scope (ADR 009).** The
`Maintenance` arm contributes the operator's position to the scope
(`:1109-1120``): `working_directory_entries` (`:1156-1163``) turns the two roots
`workspace::working_directory_scope_roots` names — the enclosing workspace
and the directory itself — into candidates carrying
`RootProvenance::Operator`. `admit_automatic` classifies the Routine
candidates **first**, then asks `working_directory_is_unreached` (`:1190-1235`)
whether the Routine scope already reaches the operator's directory, and only
then classifies the operator's candidates and *extends* the admitted set
(`:1238-1247``).

Two rules make that safe, and they are the whole of ADR 009.

- **The universe may only ever grow.** The operator's roots are added, never
  substituted for the Routine roots. A wider universe can only reclassify
  `shared` bytes to `private`; a narrower one would do the reverse, and
  `private` is the only class that authorizes deletion (ADR 006). The
  mutation that replaced the routine roots with the operator's was caught by
  the test suite, which is why that mutation is one of the seven recorded in
  the C040 closure.
- **The working directory joins only when the Routine scope does not already
  reach it.** Two cases are excluded, for the same reason — the bounded
  machine-wide sweep must stay bounded. The working directory may not be
  *inside* a routine root (the routine scope already discovers it), and it may
  not *contain* one (that would widen a bounded sweep of the known roots into
  a sweep of everything under the operator's directory). The second case is
  not a taste: measured on the operator's machine, an unconditional join
  turned a bare run from the home directory into a 65-second sweep of 264,183
  directories that ended **blocked** on 1,357 unresolvable manifests in
  `~/.local/share/Trash`. `clean ROOT` is the spelling for that wider sweep,
  and it has always carried the same hazard.

The comparison is canonical-or-raw on both sides (`roots_are_related`,
`:1200-1205``), so a symlinked prefix (macOS `/var` vs `/private/var`) can neither
make two unrelated roots look related nor make related ones look unrelated.
The decision is made against the **admitted** routine roots, so a root the
maintainability rule refuses — an agent worktree, say — does not count as the
Routine scope reaching the operator's directory.

`clean ROOT` and `clean --full` are untouched: an explicit root keeps its own
scope, and the JSON `scope` label stays `"routine"`. The read-only
`scan --known` inventory applies the same rule (`src/main.rs:1502-1511`),
because an inventory that described a smaller universe than the cleanup would
not be an inventory of it.

**Admission→validation race** (`run_engine_guarded`, `:738-827`): a fatal
`InvalidRoot` over an *automatic* scope (`:765``) triggers one fresh
reclassification; only a now-proven-omittable offender rebuilds the universe from
scratch (`:772-803``), anything else becomes a typed block (`:805-825``). Every
other error — and every error at all over an explicit scope — propagates
unchanged through the catch-all arm at `:828``.

**Omission diagnostics** are printed to stderr for every format except log
(`:471-488``): policy groups them by typed omission reason and includes a count
plus at most three sanitized example paths (`src/policy.rs:167-211`); log mode
carries only the count in its one line. Only the reasons the operator can act
on are reported: a root omitted because it sits at a known-transient origin is
omitted *by design* — the tool would never have cleaned it — so it is not a
diagnostic and does not reach the operator (C038). A durable root that has
vanished, and a root that is no longer a directory, are reported. The full
omission list remains available to the late pre-spawn premise recheck, and
summarization does not mutate learned state, so a stale hint may be
reconsidered on a later Routine invocation. A
blocked automatic scope reports without traversing anything (`:377-400``,
`Ok(1)`); an empty admitted set is the successful no-op (`:401-431``, `Ok(0)`).

**Policy** (`:330-345``): `min_reclaimable_bytes` and `min_inactive_seconds`
override cleanly (`Option`-on-scalar, so "unset" is distinguishable from "set
to 0"). `include`/`exclude` are different — `Vec<String>`, and clap cannot
distinguish "no `--include`" from "zero values", so **an empty CLI list falls
back to the configured list** (`:335-344``). Consequence: **`--include`/`--exclude`
cannot clear a configured list.** A user with `[cleanup.policy] exclude = ["**/target"]`
who wants a one-off unfiltered run has no flag for it. Defensible — the empty
list almost always means "I did not mention it", and treating it as "clear"
would be worse in the common case — but undocumented in `--help` and silently
surprising when it bites. A `clear-include` flag pair would fix it without
changing the default. Deliberate choice, not a defect.

**Selector** (`:346-355``): `profile.map(…).or_else(|| package.map(…))` looks
like silent-precedence, but **it is unreachable**. clap already enforces mutual
exclusion: `src/cli.rs:74-78` gives `profile` `conflicts_with = "package"` and
gives `package` `conflicts_with = "profile"`; the combined forms are `Err`, and
`the_duplicate_override_rule_survives_the_consolidation`
(`src/cli.rs:1449`) asserts it at both levels. The `.or_else` is defensive dead code, and a user
supplying both gets a clap usage error and exit 2 — correct, by a different
mechanism than the expression suggests.

**Empty-roots short circuit** (`:427-457``). If the admitted set is empty and no
block was raised, the pipeline stops before any Cargo invocation. The `CleanReport` literal
(`:433-448``) names **every field explicitly** — M012A removed the
`..Default::default()` spread, so a new `CleanReport` field is now a compile error
here rather than a silent default. That is a real robustness gain and it is why
the aggregate is still readable: `results`, `diagnostics`, `failed`, `mode`,
`counters`, `scope_blocked`, `unresolved_ownership`, `out_of_scope_unresolved`,
`selected_roots`, `discovered_manifests`, `resolved_workspaces`,
`units_considered`, `effective_policy`, `selector`. Note `selected_roots:
Vec::new()` even though `resolved.roots` is empty by construction — consistent,
not contradictory.

`emit_cleanup` is called with a fixed `empty_note` and **`Ok(0)` at
`:456``** — "nothing to do" is success.

### The `-i` selection step

`resolve_candidate_selection` (`:577-653`) is new in M014 and is the only place
in `main.rs` that turns a *declared* selection into a *resolved* one.

Without `-i` it is trivial: `request.selection.resolved()?` (`:585``). The
`CleanupSelection` enum (`src/cleanup.rs:85-131`) has four variants — `All`,
`WorkingDirectory` (an **unresolved** anchor on the process working directory),
`Ancestry(PathBuf)`, and `Roots(Vec<PathBuf>)` — and `resolved()`
(`src/cleanup.rs:161`) turns `WorkingDirectory` into `Ancestry`, failing closed
if the working directory cannot be read. `All` selects everything;
`WorkingDirectory` selects nothing until resolved (`src/cleanup.rs:97`), so a
caller that forgets to resolve it cleans nothing rather than everything. This
split exists because `cli.rs` resolves argv but must not perform IO.

**Which spellings get the anchor.** `src/cli.rs:481-484` binds
`CleanupSelection::WorkingDirectory` to the two maintenance spellings (bare
invocation and `clean --known`), whose Routine scope spans the machine, and
`CleanupSelection::All` to `clean ROOT` and `clean --full`, which already bound
their own scope. `src/cli.rs:2168-2207` asserts exactly that split.

**With `-i`**, three refusals happen *before* anything is spawned, in this
order: the machine formats first (`:600-606`` — `--format json` and `--format log`
are automation contracts and cannot carry a terminal session), then the terminal
precondition (`:612-621``), then the listing itself (`:635-642``). The ordering is
deliberate and stated in the source: the listing is a full read-only resolution
that spawns `cargo metadata` across every resolved root, and spending the
operator's time and a process spawn on a run that was never going to happen is
worse than an early refusal.

The listing goes through the *same*
`cleanup::list_cleanup_candidates` pipeline the engine uses
(`src/cleanup.rs:2558`), so the offered rows are the engine's own units. A
blocked listing is refused outright (`src/main.rs:707-716``) — offering rows from an
unproven universe would be the ADR 006 shape in a new costume. The session
result maps to `SelectionOutcome::Chosen(Roots(..))`, `Cancelled`, or
`NothingToClean` (`:579-592`). A cancellation is a deliberate operator
decision, not a failure: it prints a plain sentence to stderr and returns
`Ok(0)` (`:492-499``). A scope that proved no cleanup unit is a plain negative
result, not a refusal: it prints `Nothing to clean.` on stdout and returns
`Ok(0)` (`:500-508``).

**The selection is a post-classification filter.** `resolve_cleanup_roots` and
the engine's own `build_groups` still see the full Routine universe, so output
shared with a workspace outside the anchor stays `Shared`. Narrowing root
selection *before* classification would be the ADR 006 defect.

### Reporting

**Reporting** (`:510-567``): `show` requires `format == Human` **and** a capable
terminal (`:510-511``) — so progress is disabled for both JSON *and* log mode,
which is what log mode wants; `wall_start` at `:513``; the engine call is
`:518-530``; `renderer.finish_and_clear()` at `:531`` clears transient UI
**before** the report, matching the scan ordering. It runs on the **failure**
path too: the result is bound to `cleanup` and `finish_and_clear()` is called
before the `?` (`:531-532``), because `BarState::drop` *finishes* a bar rather
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

The state-generation `println!` (`:533-540`) is gated on **both** `format == Human`
**and** `options.verbose` (`:533-535`), so it cannot corrupt JSON *or* a log line
— and since M020 it does not print on the default human surface either. Which
state generation the cleanup used is debugging information, not a result; it
belongs to the verbose surface.

`emit_cleanup` (`src/main.rs:1354`) is the cleanup's format dispatch, and it has **two**
human shapes (`src/main.rs:1384-1401``): the verbose one prints the `combined roots …`
preamble (`src/main.rs:1386-1397``) and then the report, the quiet one prints the report
alone (`src/main.rs:1398-1400``). The JSON and log branches are untouched by `--verbose`.

**Exit code** (`:563-567``): `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`failed > 0` means at least one `CleanOutcome::Failed`, counted in
`CleanReport::render` — the verbose shape's `failed` counter
(`src/cleanup.rs:952-954`) and the quiet shape's (`src/cleanup.rs:830`), which
renders it as a "N failed" note on the tally line (`src/cleanup.rs:846-848`).
`scope_blocked.is_some()` means the scan boundary itself was refused and no work
was attempted; the block is a typed `ScopeBlock` (`src/cleanup.rs:524-530`) whose
`message` is what renders (`src/cleanup.rs:526-530`). Pinned by
`tests/cli_contract.rs:735` (`Some(1)`, `scope_blocked == true`).

**`--stats`** (`:512-532``) goes to **stderr** via `eprintln!`, consistent with
`src/cli.rs:129-133`, and prints four counter strings plus mode and elapsed:
`stats_line()` (`:537``), `timings_line()` (`:538``), `proof_stats_line()` (`:539``),
`proof_timings_line()` (`:540``). The two `proof_*` lines are the point of the
comment at `:532-533`` — "C003 §7.6: cleanup `--stats` must account for the
final ownership-universe proof work, not only the initial scan". The scan's
`--stats` (`:2024-2041``) has no proof counters, correctly: a scan performs no
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
(`src/cleanup.rs:553-566`): `scope_blocked` / `unresolved_ownership` for the
boundary-relevant case, and `out_of_scope_unresolved` for the count of
manifests that did not resolve outside it. The human report renders the latter
as one bounded line (`src/cleanup.rs:697-699`) so that "boundary clean" cannot
read as "system clean".

**`main.rs` participates in the reporting, not the blocking.** The four
synthesized `CleanReport` literals in this file — the blocked scope
(`:433-452``), the empty-roots no-op (`:462-477``), and the two
`run_engine_guarded` fallbacks (`:721-809`, `:721-809`) — all set
`out_of_scope_unresolved: 0` (`:441``, `:470``, `:775``, `:800``). In the first two
that is *true*: no traversal happened, so nothing was observed out of scope. In
the `run_engine_guarded` fallbacks it is an **unmeasured** zero: the engine ran
and returned `Err`, so its report — and any out-of-scope count it carried — was
discarded. The JSON envelope does not carry the field at all
(`src/output.rs:55-63`); a JSON consumer sees out-of-scope poison only as
`out_of_scope`-category diagnostics (`src/output.rs:134`). See §11 for what this
does and does not claim.

## 6. Root resolution and collapse

`collapse_roots` sits at **`src/main.rs:1485-1499`**, called from
`admit_automatic` — after provenance admission, never before it. Its position
is the C028 fix: classification must see the raw identities, because
canonicalizing first would resolve a symlinked root into its target and admit
a real directory where a block belongs
(`policy::routine_cleanup_candidates`, `src/policy.rs:613`).

```rust
// collapse_roots — src/main.rs:1485-1499
fn collapse_roots(roots: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    // The collapse itself is the shared one (`traverse::collapse_outermost`):
    // canonicalise, dedup, drop nested roots. The scan and display surfaces
    // keep a root's own spelling when it does not resolve, where the cleanup
    // surface fails closed.
    cargo_cleanme::traverse::collapse_outermost(roots)
        .into_iter()
        .map(|root| root.spelling)
        .collect()
}
```

**The collapse is one implementation, and the policy is the caller's.**
`traverse::collapse_outermost` (`src/traverse.rs:177-199`) canonicalises each
root, drops the duplicates, and drops a root that another root contains. The
comparison is by the root's **identity** — its physical location, resolved as
far as the filesystem allows (`traverse::resolve_root`,
`src/traverse.rs:143-160`) — so a symlinked prefix cannot make one physical
root look like two. The **spelling** a root is reported by is separate: the
canonical one when the root resolves, and the caller's own spelling when it
does not.

**Canonicalization is best-effort here.** A root that does not resolve keeps
its own spelling — but since C028 such a path never reaches this function
through the automatic scopes: proven-absent roots were already omitted and
uncertain ones already blocked, so only admitted real directories (plus strict
explicit roots) arrive here. The absolute-root invariant the cleanup boundary
depends on is established *earlier*, by `absolutize_root`
(`src/cli.rs:894-903`), and only for the explicit-`ROOT` source. For the other
two sources the roots were admitted as real directories by classification
(`src/policy.rs:252-254`), so in practice they resolve; the engine's own
strict validation (`validate_cleanup_root`, `src/cleanup.rs:3244-3262`) plus the
admission→validation race defence (`run_engine_guarded`, `src/main.rs:753`)
cover the residual window.

**The engine re-collapses, fail-closed.** `cleanup.rs` has its *own*
`collapse_roots` (`src/cleanup.rs:3264-3281`), and it is the strict one: a
canonicalize failure is an `InvalidRoot`, not a passthrough. So the permissive
spelling in `main.rs` is not the last word — the engine re-derives the
canonical set and refuses to continue if a root stopped resolving between
admission and the collapse.

**The sort is load-bearing, not cosmetic.** `PathBuf` ordering is component-wise
and a parent always sorts before its children (`/a/b` < `/a/b/c`). The
containment filter at `:1541`` only works because of that: a child is tested only
after its parent has been visited and accepted. Remove the `sort()` and the same
input in reverse order keeps *both* `/a` and `/a/b` — collapse silently stops
collapsing. `reconcile_full` argues the same case about its own sort
(`src/discovery_state.rs:382-385`: "*`learned` is sorted, and a parent always
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
(`tests/cli_contract.rs:3600-3697`); the admission-before-collapse ordering
itself is pinned by
`cleanup_candidates_preserve_symlinked_seed_identity_for_admission`
(`src/policy.rs:1317`).

## 7. State reconciliation

`src/main.rs:1646-1766` — the only genuinely intricate block, and where a
reviewer should spend the most time.

```rust
let loaded_state = discovery_state::load_default();            // :1568
if full { … } else if let StateLoad::Loaded(prior) = loaded_state { … }  // :1576, :1630
```

There is no `state_reconciled` flag. It was removed when the failed-publish
exit-code escalation was fixed; see "A failed publish no longer changes the
exit code" below. Nothing in this block can now influence the process exit
status — the stderr notice is the whole of the consequence. Since C028 the
block additionally yields the proven in-memory generation to the caller
(`ScanOutcome::full_state`, `:1520`), so a Full cleanup consumes exactly what
the traversal proved even when the save fails.

**Prior selection.** `full_reconciliation_prior` returns
`Option<(DiscoveryState, bool)>` (`src/discovery_state.rs:54-61`): `Loaded(s)` →
`Some((s.clone(), false))`; `Missing` → `Some((default, false))`;
`RecoverableInvalid(_)` → `Some((default, true))` where the `bool` is
`replacing_invalid`; `UnsupportedNewer` and `Unavailable` → `None`. The `None`
case is the important safety choice: **state written by a newer binary, or
simply unreadable, is never overwritten.** The `else` at `src/main.rs:1754-1760` records
a `{message}; Full state reconciliation was skipped` notice (fanned out to
stderr after the progress clear), where
`message` is `loaded_state.diagnostic()` (`src/discovery_state.rs:33-49`) with a
generic fallback.

**The join** (`src/main.rs:1675-1682`) maps canonicalized member manifest → workspace root;
**observations** (`src/main.rs:1683-1692`) apply the same canonicalize-or-raw rule to each
discovered manifest, so the join succeeds whenever both sides are
canonicalizable. `cargo metadata` reports absolute real paths and discovery
manifests come from the walk, so both canonicalize to the same bytes in
practice. When they do not, the failure is a **safe degradation, not a wrong
answer**: the record falls back to `manifest.parent()`
(`src/discovery_state.rs:269`) and is marked
`ResolutionStatus::ManifestObservedUnresolved` — a *broader* root, not a
narrower one. Note that such a record does **not** seed a learned root: only a
Cargo-resolved workspace does (`src/discovery_state.rs:283-284`), so an
unresolved manifest is inventory, never a cleanup candidate.

**`complete`** (`src/main.rs:1693`) is true unless some diagnostic is `PlatformRoot` +
`Error`. Since M018 it does not gate everything by itself: it selects the
**reconciliation mode** (`src/main.rs:1699-1703`). A complete traversal is always
`ReconciliationMode::Complete`; an incomplete one takes the mode the *caller*
passed in — `IncompleteDiscard` for the read-only `scan --all`
(`src/main.rs:91`), `IncompleteRetain` for the Full *cleanup*
(`src/main.rs:1021`). `IncompleteDiscard` makes `reconcile_full` return
`NoPublication("Full traversal was incomplete")` before touching anything
(`src/discovery_state.rs:248-250`). `IncompleteRetain` publishes a generation
that **retains** every prior learned root and expires nothing, and the caller
does not write it to disk — so the persisted state keeps describing the last
complete reconciliation. That is the counterpart of `full_incomplete`
at `src/main.rs:1811`, which recomputes the *same predicate* for the exit code. Two
identical expressions in one file; they could share a helper.

**Publish** (`src/main.rs:1715-1748`): on `Reconciliation::Publish(next)`, the proven
generation is stored into `ScanOutcome::full_state` first (`src/main.rs:1720`), then
`publish(&next)` succeeding may record a "replaced unusable discovery state"
notice if `replacing_invalid` (`src/main.rs:1724-1729`); failing records
`cargo-cleanme: discovery state was not saved: {error}` (`src/main.rs:1731-1735`) and
**changes nothing else**. `NoPublication(reason)` records
`… was not reconciled: {reason}` (`src/main.rs:1749-1752`). All three notices wait for the
progress clear before reaching stderr, so they cannot orphan bar rows.

**The Routine branch** (`src/main.rs:1761-1777`) is the mirror: it reconciles the
*previously loaded* state with no observations at all, so its only effect is
to expire learned roots whose absence was positively established, and a
failing `publish` records a notice (`src/main.rs:1772-1776`) without changing anything
else.

### A failed publish no longer changes the exit code

`src/main.rs:1959-1970` is the whole exit decision:

```rust
if full_incomplete {
    return Ok(ScanOutcome { code: 1, full_state, publish_ok });   // :1889-1895
}
Ok(ScanOutcome { code: 0, full_state, publish_ok })               // :1896-1900
```

**This was a real defect, and it is fixed.** The block previously read
`if full_incomplete || (full && !state_reconciled) { return Ok(1); }`, so a
failed state publish returned `Ok(1)`: a good scan report on stdout, one stderr
line explaining the state was not saved, and a non-zero exit. That contradicted
the module's own position, stated at `src/discovery_state.rs:535`: *"State is an
optimization only: concurrent publishers use atomic last-writer-wins
replacement."* A read-only or full state
directory — plausible on a locked-down machine, in a container with a read-only
home, or under `chattr +i` — turned a **purely informational scan** into a
non-zero exit for a reason unrelated to what the scan found, and scripts gating
on status reported a scan failure that did not happen.

The `state_reconciled` flag is gone, so the two branches no longer disagree:
the Routine branch's byte-identical "state was not saved" notice (`src/main.rs:1772-1776``)
and the Full branch's (`src/main.rs:1731-1735``) now both leave the exit code alone. A
genuinely incomplete Full scan still exits `1`, via `full_incomplete`. Since
C028 the in-memory generation travels with the outcome either way, so
`clean --full` consumes exactly what the traversal proved
(`policy::full_cleanup_roots`, `src/policy.rs:494-518`) instead of re-reading the
disk into a possibly older generation.

**M012A widened what `full` means, so this fix now protects more.** The Full
reconciliation is now reached by `scan --all` and by `clean --full`, which are
the two operations most likely to hit an unwritable state directory — and they
are protected by the same rule the fix established. The cost is on the other
side: `run_scan` returning a non-zero code from `clean --full` now escalates to
exit 2 (§5), so the *only* way state can influence an exit code is through that
one deliberate call site.

### The Routine branch (`src/main.rs:1761-1777``)

Matches on `StateLoad::Loaded` specifically, so a Routine scan *silently*
ignores `Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable` — no
diagnostic at all, unlike the Full branch's `else`. It computes
`observed: Vec<_> = workspaces.iter().map(|w| w.root.clone())` (`src/main.rs:1763``), then for
each project bumps `last_project_seen_at` on every learned root for which
`project.starts_with(&root.path)` (`src/main.rs:1765-1771``), and finally publishes only if
`!observed.is_empty()` (`src/main.rs:1772-1776``), printing `discovery state was not saved:
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
(`src/discovery_state.rs:299`). A learned root spelled canonically and a
workspace root reached through a symlink fail to match — a **missed touch**, so
`last_project_seen_at` goes stale and the root eventually ages out of the
Routine candidate set (`src/policy.rs:608-613`). Self-limiting: the root is
dropped from Routine scope, not deleted, and a later Full scan re-learns it.
`main.rs` canonicalizes deliberately in the Full branch (`:1547``, `:1556``) and
not here. Small, real, one-sided in severity.

**No duplicate-publication exposure.** `manifests` is sorted and deduped by
discovery before return (`src/discovery.rs:933-936`), so `observations` is 1:1
with distinct manifests and cannot duplicate `ProjectRecord`s; and
`reconcile_full` separately collapses nested learned roots by nearest ancestor
(`src/discovery_state.rs:382-413`). `main.rs` needs no defence against either,
and has none.

## 8. Output selection

`OutputFormat` (`src/cli.rs:6-14`) has **three** variants — `Human` (default),
`Json`, and `Log` — and is a **global** flag (`src/cli.rs:136`), so it may appear
before or after the subcommand. `Log` is M012B's addition and is not a machine
contract; see [10-reporting §7](10-reporting.md).

| Path | Human | JSON | Log |
|---|---|---|---|
| scan | `report::render(&mut report, &human)` `:1806-1807`` | `to_string(output::scan(&report, scope))` `:1794-1799`` | `output::log::scan(&report, scope, full_incomplete)` `:1803`` |
| cleanup | **two human shapes**: the verbose one prints the `combined roots …` preamble and then the report `:1338-1344``; the quiet one prints the report alone `:1347-1349`` | `to_string(output::cleanup(&report, scope, gen))` `:1313-1319`` | `output::log::cleanup(&report, scope)` `:1321-1324`` |
| cleanup, empty roots | the fixed note `:1326`` | the all-fields-explicit report `:433-448`` | same call, via `emit_cleanup` `:1320-1324`` |
| cleanup, nothing to report | `Nothing to clean.` `:1332`` | same call, via `emit_cleanup` `:1313-1319`` | same call, via `emit_cleanup` `:1321-1324`` |
| update | three literal shapes `:216-224`` | `update_json_stream(&plan, dry_run)` `:198-201`` | `output::log::update(...)` `:206-214`` |
| update, already current | `:146`` | `serde_json::json!` document `:118-133`` | `output::log::update(...)` `:133-141` |
| update, newer than published | `:187`` | `serde_json::json!` document `:159-174`` | `output::log::update(...)` `:174-182` |
| config | `println!` only `src/main.rs:264``, `src/main.rs:272``, `src/main.rs:321`` | **rejected** → exit 2 `src/main.rs:249-253`` | `output::log::config(action)` `src/main.rs:262``, `src/main.rs:270``, `src/main.rs:319`` |
| fatal error | `cargo-cleanme: {e}` `:32` | none — stderr + exit 2 `:34` | `output::log::fatal(op, code)` `:27-30` |

### `--verbose` reshapes the human column only

**`--verbose` is a global flag** (`src/cli.rs:145-146`), like `--config`,
`--no-progress`, `--stats` and `--format`, so every report-producing
subcommand honours it and it may appear before or after the subcommand name. It
reaches this file as the fourth field of `RunOptions` (`:65-71`), set from
`cli.verbose` at `:78`, and it is consumed only where a surface is chosen: the
two human render call sites (`:1430``, `:1928``), the progress renderer (`:483``,
`:648``, `:1630``), and the two stderr gates described below.

**It selects a rendering shape, not an operation.** The two human shapes are
produced by `report::HumanOptions` (`src/report.rs:20`), built by
`human_options` (`src/main.rs:1410-1419`): verbose → `HumanOptions::verbose()`,
otherwise → `HumanOptions::quiet(roots)` with the roots the report may spell
paths relative to. Those roots come from `human_cleanup_roots` (`src/main.rs:1425-1431`)
and `human_scan_roots` (`src/main.rs:1434-1449`); the cleanup helper keeps a **Full**
cleanup's paths absolute, because a Full cleanup has no single root to be
relative to, and the scan helper gives **no** roots for a `Global` scan,
because a relative path under `/` would be a lie. `absolutize_for_display`
(`src/main.rs:1464-1467``) is the other half: the scope keeps the root as the operator
spelled it (`scan .`), so the base has to be absolutised — and canonicalised, like
the report's own paths — before a relative path can be taken against it.

**The JSON and log projections do not move.** Neither `output::scan`,
`output::cleanup` nor `output::log::*` is reached through `HumanOptions`, so
the JSON document and the log line are byte-identical with and without
`--verbose` — and so is the exit code. That is the machine contract, and it is
pinned end to end by
`the_machine_contract_is_identical_with_and_without_verbose`
(`tests/cli_contract.rs:4630`), which drives both formats with and without the
flag and compares stdout and the status.

**Two stderr surfaces moved under the flag.** The cleanup's
`state generation last_full_at=…` line (`:503-510``) and the scan's
per-diagnostic fan-out (`:1782-1859``) are now verbose-only; the scan's
one-line diagnostic count (`:1774-1780``) still prints on every non-log run, and
its wording did not change. Both moved surfaces were debugging information
printed on the default surface, which is the shape M020 removed.

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

`scope_label` (`:1411-1447`) matches on `ScanScope` — `Global` → `"full"`,
`Explicit`/`ExplicitRoots` → `"explicit"`, `Routine` → `"routine"` — and is called
with `policy.scope` (`:1720``, `:1727``), which `policy::resolve` returned at
`:1439-1445``.

This was a real contract bug and it is fixed. The previous code derived the label
from `full` and `root.is_some()`; mechanism is in
[10-reporting §9](10-reporting.md) ("Can `scope` be wrong?"). The residual note is
that `scan` and `cleanup` now share one label vocabulary — `full`, `explicit`,
`routine`.

### `state generation last_full_at=` goes to stdout

`src/main.rs:596-603` uses `println!` — **stdout**, not stderr — guarded to human
format **and** to `--verbose` (`:596-598`), so it cannot corrupt JSON *or* a log
line, and since M020 it does not print on the default human surface either. But
it is machine-adjacent metadata interleaved into a human report, on exactly the
stream that pipelines capture. The project's stated discipline is stdout =
deterministic report, stderr = transient/diagnostic. This line is neither. It
belongs on stderr beside `--stats`, or in the JSON envelope as a field —
`output::cleanup` already accepts a `state_generation` argument
(`src/output.rs:161-165`), and `log::cleanup` deliberately does not, so a log
consumer cannot recover it at all.

### Serialization failure

Two sites, inconsistent: `src/main.rs:1853`` and `src/main.rs:1369`` map to
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
(`src/output.rs:950` and `src/output.rs:988`), not in `main.rs`. `run_update`
calls the **stream** variant (`src/main.rs:209-212`) so that the exact bytes on
stdout come from the library seam the contract tests drive, rather than from a
`println!` assembled in the binary. The previous revision of this file located
`update_json` in `main.rs` and called it untested; both claims were stale. The
machine contract is now pinned by four unit tests in `output.rs`
(`src/output.rs:992-1113`) and by
`tests/cli_contract.rs:3311`, which reads `src/main.rs` as text and asserts that
`run_update` contains exactly one unterminated `print!(` and that it is the
`update_json_stream` call.

What remains true is the *shape* of the finding: `update_json` is a second,
hand-maintained envelope alongside `output.rs`'s `EnvelopeV1`, and the two
benign-error arms of `run_update` (`src/main.rs:122-238`, `:122-238`) hand-build
a `serde_json::json!` document in the binary — a third copy of that shape, now
outside `output.rs` entirely. C022 pinned the machine contract; migrating
`update_json` to `EnvelopeV1` is still a contract change needing a plan and a
version bump.

### Ordering divergence between the two cleanup projections

**Confirmed.** `CleanReport::render` takes `&self` (`src/cleanup.rs:720`) and
therefore **cannot** reorder its own data. Since M020 it is also a **dispatcher**:
it picks `render_verbose` (`:934``) or `render_quiet` (`:771``) from
`options.verbose` (`:721``). Each shape builds its own local
`ordered: Vec<&CleanResult>` — the quiet one at `src/cleanup.rs:777`, the
verbose one at `:960`` — sorts by `before_bytes` descending with a
`display_path` ascending tie-break (`:1831-1836` and `:1831-1836`), and iterates
*that* (`:786-797`` and `:967``). `output::cleanup` takes `&CleanReport`
(`src/output.rs:161-162`) and iterates `report.results` in **original insertion
order** (`src/output.rs:167-189`) with no sort at all.

So JSON `units[]` and the human unit rows are in different orders whenever
insertion order is not already size-descending, and a consumer matching the two
positionally or diffing them sees spurious differences. The paths never
co-execute (`emit_cleanup` branches at `src/main.rs:1361`/`src/main.rs:1384`), so no test can observe
it, and `tests/cli_contract.rs` only ever indexes `units[0]` — order-independent
in its fixtures.

The **scan** side is the opposite and self-consistent: `report::render` takes
`&mut ScanReport` (`src/report.rs:146`) and sorts `report.groups` **in place**
(`src/report.rs:147-151`), which `run_scan` relies on by passing `&mut report`
(`src/main.rs:1862``). The asymmetry between the two `render` signatures — `&self` + local
copy for cleanup, `&mut self` + in-place sort for scan — is the root of the
finding. The M020 change widened it: the cleanup surface now has **two** shapes
and each carries its own copy of the ordering logic, so the divergence is
reproduced twice on the human side and still once on the JSON side.

## 9. Invariants and edge cases

**Is the exit code consistent? Mostly.** 1 = "ran, partially failed",
2 = "could not run". Two known outliers: `config edit` is the deliberate one,
collapsing an arbitrary editor status into 1 (`:301``, pinned by
`tests/cli_contract.rs:1106`); and `update` cannot express partial failure at
all. M012A added a third, also deliberate: `clean --full` on an incomplete scan is 2
(`:959-967``).

**Can a command return 0 while having printed to stderr? Yes, routinely.**

| stderr message | Line | Exit effect |
|---|---:|---|
| `N filesystem diagnostics; rerun with a bounded root if needed` | `:1774-1780`` | **none** — a scan full of `PlatformRoot` errors still exits 0 |
| the diagnostic detail behind that count (grouped Cargo causes, example paths, non-Cargo lines) | `:1782-1859`` — **`--verbose` only** since M020 | none |
| `scan stats: …` | `:1864-1871` | none (opt-in) |
| `cleanup stats: …` | `:512-525` | none (opt-in) |
| `discovery state was not saved: {error}` — **Routine** | `src/main.rs:1772-1776`` | **none** |
| `{warning}; using seed/configured Routine roots` (from policy) | `src/policy.rs:633`, `src/main.rs:1119` | none |
| `replaced unusable discovery state after successful Full reconciliation` | `:1708-1714` | none (a *success* notice) |
| `discovery state was not reconciled: {reason}` | `:1734-1738` | **none** (fixed; see §7) |
| `discovery state was not saved: {error}` — **Full** | `:1758-1762` | **none** (fixed; see §7) |
| `{message}; Full state reconciliation was skipped` | `:1742-1746` | **none** |
| `editor exited with {status}; …` | `src/main.rs:298-301`` | **→ 1** (not the editor's code) |
| `cannot launch editor {program}: {e}` | `:283-285`` | → 2 |
| `interactive selection cancelled; no cleanup commands were run` | `:557-560` | none (`Ok(0)`) |
| `discovery state was not saved; cleaning the reconciled in-memory generation only` | `:1023-1027` | none (the Full cleanup proceeds on the in-memory generation) |

The state-warning rows were **fixed**, not left as a finding: the Full branch used
to escalate both to exit 1 while the Routine branch ignored the byte-identical
message. Both are now inert, which is why the "sharp inconsistency" the earlier
revision described no longer exists here. The remaining asymmetry is cosmetic —
the Full branch explains itself on stderr while the Routine branch is silent about
a load problem it never reports.

**Can `run_scan` be reached twice in one process?** No. The `Invocation` arms are
mutually exclusive, and the Cleanup arm is the only caller of `run_scan` from
inside `run`, calling it once (`:72`). So the "re-scoped totals" comment at
`:1764-1768`` describes the phase sequence *within* one call, not a second call.
There is no double-initialised renderer, no global state, no `static mut` in the
file — the renderer is a plain local (`:572`` for cleanup, `:1505`` for scan) whose
`finish_and_clear()` runs once (`:1819``, `:594``) before the report, failure path
included.

**Is `checked_sub` the only overflow guard? Yes, in this file.** `scan_start`
(`:1534``) is used unguarded at `:1789`` and guarded at `:1773`` (§4).

**Config directory read-only.** Only *creation* fails. `load_or_create`
(`:1506``, `:437``) calls `create_initial` (`:599``), which does
`fs::create_dir_all(parent)?` (`src/config.rs:457-461`) → `AppError::Io` via
`#[from]` (`src/error.rs:9`) → exit 2. If the file exists and is readable, `load`
performs no writes and everything works. Scope: `scan`, bare invocation,
`clean …`, `config show` and `config edit` all exit 2; `config path` works
(resolves without touching the FS, `:249-255``) and `update` works (never loads
config, `:102``). `--config /other/path.toml` is the documented escape hatch
(`src/cli.rs:124-125`).

**Can `process::exit` truncate buffered stdout? No.** `main` calls
`std::process::exit` at `:20` and `:34`, which does skip destructors — but
nothing is buffered waiting for them. Rust's `std::io::Stdout` is wrapped in a
`LineWriter`, so it flushes on every `\n` regardless of whether the target is a
TTY or a pipe. Every output statement here is a `println!`/`eprintln!` ending in a
newline, with one exception: the JSON update branch uses `print!`
(`src/main.rs:209-212`) — deliberately, because the library seam supplies the
trailing newline itself. The C-stdio block-until-`exit` failure mode does not
apply to Rust, and the one-newline assertions at `tests/cli_contract.rs:172` are
consistent with nothing being lost. The only theoretical exposure is a *partial*
final line with no trailing newline, which this file never produces.
**Definitive: piped JSON output cannot be truncated by `process::exit`.** The
corollary is the actual risk: safety comes from `LineWriter` plus
newline-terminated writes, not from an explicit flush, so adding a buffered
writer or a non-`println!` path would remove it silently.

**Can the maintenance scope ever get *narrower* because of ADR 009? No.** The
working directory's roots are added to the admitted routine roots
(`:1200-1209``), never substituted for them, and they join only when the Routine
scope does not already reach the operator's directory (`:1152-1159``). A wider
universe can only reclassify `shared` bytes to `private`; the reverse — the
only direction that could turn an unproven shared output into something the
engine would delete — is unreachable by construction. The mutation that made
the operator's roots *replace* the routine roots was caught by the test
suite, and the mutation that made them join unconditionally was caught by the
bounded-sweep test.

**What the safety-critical code does *not* do.** This file cannot delete anything.
It resolves roots, merges policy, chooses a mode, calls the engine, and renders.
Every destructive decision — ownership proof, authorization, recency, and the
`cargo clean` invocation itself — happens inside `cleanup.rs` behind
`clean_with_roots_policy_selector_guarded` (`:695-706``). The most consequential
thing `main.rs` does is *choose what to pass*: a wrong root, a wrong mode, or a
wrong `selected_roots` label is a correctness defect here and a safety defect
only if `cleanup.rs` trusts it, which it does not (it re-proves every candidate).
That separation is the reason this file can be 1684 lines of thinly tested
composition.

## 10. Testing

**`src/main.rs` has twelve inline tests.** Verified:
`grep -cE '#\[cfg(test)\]|#\[test\]' src/main.rs` → `10`, which is the nine
`#[test]` functions plus the one `#[cfg(test)]` module attribute. The
`#[cfg(test)] mod tests` block opens at `:1903`; the nine cases pin the
orchestrator's own decisions — the interactive surface's three endings, the
report verdict (`Nothing to clean.`), and, since M018, the coverage gate: a
completed platform walk needs no verification pass, a Full cleanup that proved
nothing blocks instead of exiting zero, an unproven root is admitted as a
*root* and excluded only as a *candidate*, a gate that proved nothing blocks
even though the roots are admitted, and an unproven learned root makes the
exit code non-zero. The `use` statements at `:1453-1454` and `:1567` are
ordinary function-scoped production imports inside `run_scan` — a common
source of the opposite impression. The same grep on the two output
modules: `src/main.rs` **10**, `src/output.rs` **24** (2 in `path_tests`,
15 inside `pub mod log` (`src/output.rs:512-990`), and 4 in `update_tests`
(`src/output.rs:992-1113`)), and `src/report.rs` **12** — the `#[test]`
function counts are 9, 21 and 11 respectively, and the report module's inline
suite grew with M020 (`src/report.rs:188-384`, was `src/report.rs:72-214`).
See [10-reporting §10](10-reporting.md) for the log tests and what they leave
uncovered.

**Consequently `update_json` is no longer uncovered** — but the gap moved. The
envelope implementations live in `output.rs` and are covered by four unit tests
(`src/output.rs:992-1113`); what is *not* covered is the two `serde_json::json!`
documents the binary hand-builds for the benign `update` outcomes
(`src/main.rs:139-154`, `:175-190`). Those are the same shape as the envelope
they imitate, written in the one file whose inline tests do not reach them.

### `tests/cli_contract.rs` — 4860 lines, 77 tests

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
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 664 | parser-level rejection, asserted on clap's conflict error rather than "the command failed" |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 696 | `schema_version == 1`, `operation == "clean"`, `mode == "simulate"` (`--dryrun` → `CleanMode::Simulate`), `summary.simulated == 0`, one `\n` |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 735 | **`status.code() == Some(1)`** with `result.scope_blocked == true` — pins the `:626-630`` exit rule |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 782 (`#[cfg(unix)]`) | six invocations against a `/bin/sh` cargo stub: `summary.cleaned == 1`, `units[0].reason_code == "cleaned"`, `selector_kind/value == "profile"/"dev"`; `before_bytes` and `selector_estimate_bytes` both `null` under a selector while `output_union_before_bytes >= 4096`; `units[0].outcome == "simulated"` under `--dryrun`; `reason_code == "selector_unsupported"` + `policy_disposition == "selector_estimate_unavailable"`; `selector_kind == "package"` with `fixture@0.1.0` |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 1106 | **`status.code() == Some(1)`** — the editor status is reported in prose and collapsed to 1 (`:305``); an invalid edit exits non-zero with `is invalid` in stderr and leaves the bad content in place |
| `working_directory_anchor_selects_without_narrowing_the_ownership_universe` | 1478 | the M014/ADR 006 anchor: selection narrows, the ownership universe does not |
| `interactive_without_a_terminal_is_a_usage_error_that_names_the_reason` | 1534 | the terminal precondition is checked before the listing (`src/main.rs:399-408``) |
| `interactive_refuses_the_machine_output_formats` | 1571 | `--format json`/`--format log` are refused for `-i` (`src/main.rs:387-393``) |
| `interactive_dry_run_still_requires_a_terminal` | 1623 | the refusal is a property of the *session*, not of the mode |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` | 1641 | **the M012A default.** `operation == "clean"`, `units` is an array where a scan would have `groups`, `mode == "execute"`, `scope == "explicit"`, and — the discriminating assertion — `clean_calls() == 1` with the artifact gone |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` | 1678 | `mode == "simulate"`, `simulated == 1 && cleaned == 0`, **`clean_calls() == 0`**, artifact still present, and Cargo *was* used for resolution — so this is a simulation, not a skip |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` | 1902 | exit 0, `scope == "routine"`, `mode == "execute"`, `units == []` — the `:517-549`` short circuit through the binary |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` | 1942 | blocked bare invocation, exit 1, no `cargo clean` |
| `blocked_human_cleanup_report_names_manifests_and_json_stays_one_object` | 1982 | the blocked-scope human surface and the one-document JSON surface coexist |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` | 2159 | `clean ROOT` with no mode flag executes; `--cargo-preview` gives `mode == "preview"` |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` | 2217 | `--dryrun`/`--yes` map onto `Simulate`/default `Execute` |
| `every_accepted_simulation_spelling_reaches_simulate_and_spawns_no_cargo_clean` | 2270 | six accepted `--dry-run` spellings, none of which spawns `cargo clean` |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` | 2464 | `scan --known` is `routine` with no configured root, `explicit` with one |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 2526 | the bounded-zero-result scan document |
| `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` … `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 2632-3081 | the M012B log cases, via a shared `assert_bounded_log_line` (`tests/cli_contract.rs:2592`): one line, bounded, silent stderr; `simulate` vs `preview` distinguishable; blocked carries a typed reason and exit 1; failed cleanup is `status=failed`; zero-result maintenance is `status=ok`; scan reports the resolved scope; `--stats` stays opt-in on stderr and leaves the one line alone; the human diagnostic fan-out is suppressed; a fatal error is **one bounded stderr line and empty stdout**, exit 2, carrying a typed code and *not* the config path or parse error; JSON and human are unchanged by log mode's existence; an unknown format still fails closed |
| `scan_emits_exactly_one_json_document` | 3116 | the one-document-per-invocation invariant, and — at source-text level only — that the `EmitReport` guard still exists (§4) |
| `the_json_update_branch_prints_only_the_tested_seam` | 3311 | reads `src/main.rs` as text and asserts `run_update` has exactly one `print!(` and that it calls `update_json_stream` (§8) |
| `stale_learned_root_is_omitted_and_bare_routine_execute_cleans_the_sibling` | 3600 | C028 premise-negative regression, direct and staged-plugin: exit 0, `scope == "routine"`, `cleaned == 1`, one Cargo clean, stale path absent from `selected_roots` |
| `stale_learned_root_simulate_agrees_with_execute_and_spawns_no_cargo_clean` | 3656 | C028 Simulate/Execute parity: `simulated == 1`, zero clean spawns, artifact kept |
| `missing_explicit_roots_stay_fatal_with_zero_cargo_clean_spawns` | 3729 | C028 negative control: missing configured `scan.root` and missing `clean ROOT` both exit 2 with zero spawns |
| `all_automatic_roots_missing_is_a_successful_empty_no_op` | 3769 | C028: all-missing automatic scope exits 0 with `units == []` and zero spawns of any kind |
| `automatic_file_root_is_omitted_and_symlink_root_blocks_without_traversal` | 3796 | C028 decision table at the CLI: file omitted (sibling cleaned), symlink-to-dir and broken link block with exit 1 and zero metadata calls |
| `out_of_anchor_poison_does_not_block_bare_routine_cleanup` | 3860 | **ADR 007.** A poisoned learned root *outside* the anchor does not block: exit 0, `cleaned == 1`, `scope_blocked == false`, one Cargo clean, and the poison is counted in the machine summary |
| `unreadable_automatic_root_blocks_instead_of_omitting` | 3899 | C028: `0o000` automatic root blocks with exit 1; premise-checked skip when the process can still read (e.g. root) |
| `incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean` | 3932 | **ADR 007, in-boundary.** An unreadable in-boundary root blocks with exit 1 and *real* resolution counts (`discovered_manifests == 1`, `resolved_workspaces == 1`, `units_considered == 1`) — the block is not allowed to report zero |
| `stale_and_blocked_automatic_roots_keep_the_bounded_log_contract` | 3968 | C028 machine contract: one bounded ASCII log line, no paths, exit 0 on omission and `status=blocked` with exit 1 on the symlink block |
| `json_format_config_is_rejected_not_emitted_as_non_json` | 4003 | `--format json config path/show` exits 2, names the unsupported combination, and puts **no** JSON on stdout (§8) |

### `tests/end_to_end.rs` — 840 lines, 4 tests

`end_to_end_reports_only_inactive_artifact_projects` (`tests/end_to_end.rs:114``) does *not* invoke
the binary; it drives the library directly with a fake `CargoRunner` through
`policy::resolve` → `discovery` → `resolve_workspaces` → `build_groups` →
`analyze_groups` → `report::render`, reproducing `run_scan`'s sequence by hand.
`a_literal_ignored_ancestor_yields_one_workspace_while_an_explicit_root_yields_two`
(`:290``) drives the same helper over two scopes and asserts the
ignore/unignore literal-path behaviour end to end.

**The previous revision's claim that "every end-to-end case here is scan-side" is
no longer true.** Two cleanup-side cases landed with the destructive-safety
work: `a_neighbouring_projects_sources_inside_a_covering_output_root_are_never_cleaned`
(`:679``) — the 0.2.0 defect, reproduced against a covering output root — and
`an_unrelated_sibling_workspace_stays_eligible_alongside_a_dangerous_one`
(`:1027``), which asserts the safe sibling is still cleaned next to it. These
drive `cleanup::clean_with_roots_policy_selector` directly, not through
`main.rs`.

### `tests/common/mod.rs` — 286 lines, not a test target

(Cargo compiles only top-level `tests/*.rs`; each suite pulls it in with
`mod common;`.) Five fixture helpers: `set_path_modified` (`tests/common/mod.rs:24`), `backdate_file`
(`tests/common/mod.rs:98``), `inactive_project` (`tests/common/mod.rs:111``), `write_fake_cargo` (`tests/common/mod.rs:173``), and `cargo_calls`
(`tests/common/mod.rs:280``). It asserts nothing about the binary.

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
(`src/output.rs:992-1113`) assert the real serialised bytes of
`update_json_stream` field by field, including that `changed` and `dry_run` are
the *only* two fields allowed to differ between the dry-run and mutating
documents (`src/output.rs:1047-1048`) — a discriminating assertion, not a
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
| one JSON document on stdout for `clean --full` | **partial** — `EmitReport::No` suppresses the intervening scan report, and `tests/cli_contract.rs:3116` asserts the one-document property for a scan plus the *existence* of the `enum EmitReport` guard (`tests/cli_contract.rs:3145-3148`). No case runs `clean --full` itself, because that is a full filesystem walk; the guard assertion is a source-text check, so it survives a rename and would survive the enum being kept while every call site passes `EmitReport::Yes` |
| `clean --full` fail-closed on an incomplete scan (`:770-772`) | **partial** — `policy::full_cleanup_roots` (`src/policy.rs:494-518`) refuses a non-zero reconciliation code (`src/policy.rs:498-503`) or a missing generation (`src/policy.rs:529-535`), pinned by `full_cleanup_consumes_only_a_fresh_in_memory_generation` (`src/policy.rs:1276`). **A live `clean --full` run now exists** — 2026-10-21, a non-root Linux host, exit `2`, empty stdout, and 128 permission-denied traversal errors under `/` (ADR 008's acceptance record) — but it exercises only the **refusal**, not a cleanup, because the gate refuses before any report is produced. The refusal path is still uncovered by a case whose code comes from a real traversal rather than a literal; that is ADR 008 evidence item 2, carried by M018 |
| Exit code when a **Full** publish fails | **covered by construction** — the proven generation travels in `ScanOutcome::full_state` (`:1405``), so a failed save cannot change the outcome; the warning path is the same stderr line as before |
| Routine-vs-Full divergence of the *same* message | **none** — both are now inert, so the divergence is unobservable *and* untested |
| `--stats` **content** | partial — the cleanup line's four counter strings (`:494-497``) are never asserted |
| `collapse_roots` | **indirect** — private to this file, so the nest-collapse property is asserted through admitted-root CLI envelopes (`tests/cli_contract.rs:3600-3897`) and the admission-before-collapse ordering through `cleanup_candidates_preserve_symlinked_seed_identity_for_admission` (`src/policy.rs:1317`) |
| Root-source precedence | only via clap's conflict declarations (`src/cli.rs:35-83`) and the `invocation()` unit tests — the parse-level guarantee plus a pure mapping, but not the runtime `resolve_cleanup_roots` selection |
| `state generation last_full_at=` on stdout | **none** — no test runs cleanup with a `Some` generation in verbose human format |
| Human vs JSON cleanup **ordering** | **none** — never co-executed; every assertion indexes `units[0]` |
| `out_of_scope_unresolved` | **none** — the field is not in the JSON envelope at all, and no contract test asserts the human "out-of-scope unprovable participant(s)" line. The ADR 007 *behaviour* is pinned (`tests/cli_contract.rs:3860`, `tests/cli_contract.rs:3899`), but the *reporting* of it is not |
| Read-only config directory | **none** |
| The two hand-built `update` JSON documents | **none** — `src/main.rs:46-61` and `:46-61` |

**Honest assessment.** `main.rs` is the composition root — the only place the
two pipelines are wired — and composition is precisely what unit tests with fakes
do not cover. What M012A did well is push the *decisions* out to where they can be
tested: `Cli::invocation` is now a pure function with its own unit tests in
`src/cli.rs`, and the M012A behaviours are pinned through the binary by name.
What M014 and M017 did well is keep the *bounds* in the library:
`CleanupSelection` is `src/cleanup.rs`'s type, not `main.rs`'s, and the
anchor's fail-closed property is a unit test in `src/cli.rs:2210`. What is still
un-testable in place is the *wiring*: which arm a given invocation reaches, and
what `run_scan` writes when called from inside `resolve_cleanup_roots`.
`end_to_end.rs` re-implements `run_scan`'s call sequence in the test body, which
is itself a structural risk: the sequence can drift from `main.rs` and the test
stays green. It has already drifted once, per its own doc comment (`:6-9`).
`main.rs` itself remains untested, and the decisions that belong to it rather than
to a module are the ones with no coverage.

## 11. Review checklist

1. **The `out_of_scope_unresolved` zeros in `main.rs` are unmeasured in two of
   four sites.** `src/main.rs:792` and `:818` build a `CleanReport` after the
   engine returned `Err`, so the engine's own out-of-scope count was discarded
   and the field is set to 0. The human report then says nothing about
   out-of-scope poison on a path that *did* traverse. The other two sites
   (`:506`, `:533`) are correct — no traversal happened. The JSON envelope has
   no such field either way (`src/output.rs:55-63`), which is the sibling
   finding tracked in [10-reporting §11](10-reporting.md) item 16 / G4; this
   item is the `main.rs`-side half of it, and the two are complementary rather
   than duplicate. Decide whether the two `run_engine_guarded` fallbacks should
   carry the engine's count, or whether "not measured" should render as "not
   measured" rather than as 0.
2. **The uncertainty filter has no `_` arm** (`src/main.rs:1610-1617`). It
   matches three of the seven `DiagnosticCategory` variants
   (`src/domain.rs:68-81`) by `matches!`, so a newly-added category is
   *silently* excluded from `uncertainty` and the exclusion reads as deliberate.
   `OutOfScope` is correctly excluded under ADR 007. Consider an explicit
   arm-per-variant form so the compiler sees the enumeration.
3. **The `update` JSON surface is a third copy of the envelope.** The two benign
   `update` outcomes hand-build `serde_json::json!` documents in the binary
   (`src/main.rs:139-154`, `:175-190``) while the mutating path uses the tested
   `output::update_json_stream` seam (`src/main.rs:211-214`). C022 pinned the
   machine contract for the seam; the two hand-built documents are in the one
   file with no tests. Moving them into `output.rs` would close the gap without
   a version bump.
4. **`config --format json` is a hard rejection** (`:249-253``) and
   `config --format log` withholds the path and the TOML text (`:258-273``). Both
   are deliberate. Confirm that an operator who scripts
   `cargo cleanme --format json config path` understands the exit 2 to be a
   *usage* rejection rather than a broken tool, and consider whether
   `config path` — a pure path lookup with no content to leak — should be
   allowed in JSON mode.
5. **`config edit` collapses the editor's status to 1** (`:305``), and the
   editor's own code appears only in prose (`src/main.rs:298-301``). The 0/1/2 contract is
   now uniform, which is the right call; confirm no consumer was relying on the
   old passthrough (`tests/cli_contract.rs:1106` now pins `Some(1)`).
6. **Bare invocation deletes bytes.** `src/cli.rs:415-421` makes a no-subcommand
   invocation a Routine Execute cleanup with no prompt. The safety argument is
   that `cleanup.rs` re-proves every candidate and deletion is always
   `cargo clean`, not that the default is conservative. Confirm that reading, and
   that `src/cli.rs:103-116` (the `about`/`long_about` text) stays accurate — it
   is the only place a user is told. **It is now accurate:** C040 rewrote the
   root `long_about` (`src/cli.rs:104-116`) to say that `scan` *needs a scope*
   and that `--all` is the full read-only machine reconciliation
   (`src/cli.rs:117-120`), and the manpage is rendered from that same text, so
   the two cannot drift apart again. The remaining drift is in the comment at
   `src/main.rs:1552` ("Rootless `scan` is Full"), a reader-visible falsehood
   about a *safety-relevant* spelling.
7. **`process::exit` and buffered stdout.** `src/main.rs:20`/`:34` skip
   destructors; safety comes from `Stdout` being a `LineWriter` plus
   newline-terminated writes, *not* from an explicit flush. The JSON update
   branch is the one place that uses `print!` rather than `println!`
   (`src/main.rs:209-212`), and it is safe only because the library seam
   supplies the trailing newline. A new buffered writer, a `write!`, or a
   non-newline-terminated final line removes that property silently — and the
   log mode's bounded-line contract (`src/output.rs:319`) is newline-sensitive
   too.
8. **`ScopeBlock.reason` is dropped by every report path except log.**
   `main.rs` passes `resolved.scope_label` to `output::cleanup`, which projects
   only `.message` (`src/output.rs:222`). The typed code survives only in the log
   line (`src/output.rs:387`). A schema-version decision, not a patch — see
   [10-reporting §11](10-reporting.md) item 12.
9. **stdout/stderr interleaving.** `src/main.rs:590-597` puts
   `state generation last_full_at=` on stdout, against the project's own rule
   (stdout = report, stderr = diagnostic). Human-only **and** `--verbose`-only
   today (`:596-598` gates on both), and log mode cannot see the generation at
   all; a landmine if `:596` is ever relaxed.
10. **Serialization errors use the wrong variant.** `src/main.rs:1838` and `:1354`
    produce `configuration error: cannot serialize JSON report: …`, sending the
    user to `config.toml` when the fault is in report serialization
    (`src/config.rs:610-612` does the same). The third site that omitted the
    prefix was removed with the old empty-roots branch (§8), so this is now the
    only inconsistency left.
11. **Redundant root echo.** `src/main.rs:1303-1331` and `src/cleanup.rs:894-904`
    print the same root list, in two formats, as the first two lines of a human
    `clean` report (`selected_roots` is `roots.clone()`,
    `src/cleanup.rs:1792`). **M020 moved both under `--verbose`**, so the default
    surface no longer echoes the roots at all — the redundancy survives only on
    the verbose surface.
12. **Human/JSON cleanup ordering divergence.** `src/cleanup.rs:918-925` sorts a
    *local* vec that `output::cleanup` (`src/output.rs:167-189`) never sees, so
    JSON `units[]` is insertion-ordered while human rows are size-descending. The
    quiet shape carries its own copy of the same sort (`src/cleanup.rs:735-756`).
    The scan side is self-consistent (`src/report.rs:147-151` sorts in place).
    Confirm the projections are not expected to agree positionally.
13. **`--include`/`--exclude` cannot clear a configured list.**
    `src/main.rs:441-450` falls back to config on an empty CLI vec. Deliberate
    but undocumented; clearing needs a distinct flag, not a reinterpretation of
    the empty list.
14. **`collapse_roots` canonicalization is best-effort.**
    `src/main.rs:1408` uses `unwrap_or(p)`, so a missing or unresolvable root
    passes through unvalidated. The engine's own `collapse_roots`
    (`src/cleanup.rs:3148-3174`) is the fail-closed one and is what actually
    guards the boundary. Confirm the permissive version in `main.rs` is
    understood to be a *pre*-pass, and that the `sort()` at `src/main.rs:1475` is
    understood to be *required* by the containment filter at `:1479`.
15. **Explicit `ROOT` skips `policy::resolve`.** `src/main.rs:1053-1074` never
    calls it (only `Maintenance` does, `:1052-1061`), so the exists/is-a-directory/
    is-not-a-symlink checks at `src/policy.rs:33-42` never run for `clean ROOT` —
    a promise `src/cli.rs:841-843` makes while naming a different layer. The
    engine's own validation (`src/cleanup.rs:3128-3146`) is what actually holds
    the line there.
16. **`main.rs` is almost untested.** Four `#[cfg(test)]` cases here; 21 in `output.rs`; 11 in
    `report.rs`. The machine contract *is* pinned by `tests/cli_contract.rs`
    (§10), so the gap is specifically the *orchestration* decisions — which arm a
    given invocation reaches, what the `clean --full` scan writes, and the two
    hand-built `update` documents.
17. **The progress-clear ordering is untested and recorded as such.**
    `src/main.rs:594-595` clears the bar before the fallible cleanup call, and
    the ordering is unreachable from the suite: the bar only draws on an
    attended terminal, and the failing-cleanup contract test passes
    `--no-progress`. C023 §11.4 records it as an open coverage gap needing a pty
    harness. See
    `plans/closure/distribution-release-update/c023-status.md`.
18. **The `EmitReport` guard is asserted by a source-text check, not a
    behavioural one.** `tests/cli_contract.rs:3145-3148` asserts that
    `src/main.rs` still contains the string `"enum EmitReport"`. That is the
    repository's own §4 lesson in miniature — *a green test proves only that its
    own premises hold*. The premises here are "the type is still spelled that
    way" and nothing about what any call site does with it. Renaming the enum
    breaks the test (good), but keeping the enum while every call site passes
    `EmitReport::Yes` leaves it green (bad) — and the bug it guards against
    (two JSON documents on one stdout, §4) would return. A behavioural
    assertion needs a `clean --full` case, which the bounded suite cannot
    afford; a cheaper substitute would be to assert that the *call site* passes
    `EmitReport::No` (`src/main.rs:1020`), which is the thing that actually
    matters and is a one-line source check that cannot be satisfied by a
    vestigial enum.
