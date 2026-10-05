# Testing & Verification — how this project keeps its tests honest

> Component deep dive · part of the [architecture overview](overview.md)

---

## 1. Responsibility

The verification apparatus does three distinct jobs. The third is what makes it unusual.

**(a) Product behaviour.** 288 inline `#[test]` functions in `src/`, plus two integration suites that drive the compiled binary as a subprocess: ownership proof, cleanup authorization, the three clean modes, workspace resolution, the versioned JSON contract.

**(b) Structural contracts invisible at runtime.** 17 scripts in `scripts/` and 6 GitHub workflows. No product code path can detect a file left out of the published crate's `include` allowlist, completions drifted from `cli.rs`, a tag pointing at the wrong commit, or release bytes differing from qualified bytes. These are properties of the repository *as an artifact* — see [Distribution & release](15-distribution-and-release.md). **The inventory of those 17 scripts is itself machine-checked**: `check-doc-citations.py` requires the [§5](#5-the-contract-checkers) table to be exactly the set of scripts on disk, in both directions and without duplicates, so a new checker cannot ship while the table keeps describing an older repository.

**(c) Properties of the test fixtures themselves.** The category this project invented. A test that stubs a tool the subject cannot invoke is indistinguishable from one that does not stub it; a test whose pass condition is an unrelated failure reports coverage that does not exist. Both classes shipped here ([§7](#7-the-false-green-problem)). The response was not more tests but *premise guards*: assertions inside the fixture, before the behaviour under test runs, that fail loudly when the fixture can no longer prove what it claims to. `check-fixture-portability.py` generalises the idea into a static checker.

A green result here is not a sufficient claim on its own. The correct question is: *what
were the premises, and is any of them unchecked?*

---

## 2. The test pyramid as it actually stands

| Layer | Location | Count | What it can catch | What it cannot catch |
|---|---|---:|---|---|
| Inline unit tests | `src/**/*.rs`, in-file `#[cfg(test)] mod tests` | 288 `#[test]` | Logic errors, ownership/authorization decisions, argument construction, refactor regressions | Anything needing a real process, filesystem, or network; any premise of a fake |
| Integration — CLI contract | `tests/cli_contract.rs` (1,761 lines) | 30 `#[test]` | The subprocess boundary end to end: argv normalization, exit codes, JSON shape, log-line shape, `--stats` stream discipline, selector behaviour, usage conflicts | Internals of `run()`; anything on a real machine; 18 of the 30 are `#[cfg(unix)]` |
| Integration — end to end | `tests/end_to_end.rs` (347 lines) | 2 `#[test]` | Full production-pipeline scans against synthetic trees with real mtimes, under both a filtered Routine scope and an explicit scope | Cleanup, update, config editing |
| Shared harness | `tests/common/mod.rs` (229 lines) | 0 tests | `set_path_modified` / `backdate_file` (per-platform mtime control), `inactive_project` tree construction, `write_fake_cargo` / `cargo_calls` (unix only) | — |
| Contract checkers | `scripts/` | **17** (see note) | Published-crate contents, fixture portability, installer/shape/smoke contracts, release identity, benchmark regression, and the completeness of this inventory | That a fixture case proves its intended branch (explicitly disclaimed) |
| CI | `.github/workflows/` | 6 | Cross-platform, cross-toolchain, MSRV, generated-doc freshness, release drift | Doctests (`cargo test --all-targets` skips them) |

**Note on the script count.** `scripts/` contains **17** regular files — 14 `*.py` and 3 `*.sh`, no subdirectories. The count grew during Phase 11 as the selector-qualification, staged-validation, smoke-transition, and release-attestation checks were added.

**This table is now gated, and that is the substantive change.** It previously carried 16 rows against 17 files: `check-doc-citations.py` was missing, and nothing noticed, because a prose table is nobody's obligation. `check-doc-citations.py` now enumerates `scripts/*.py` and `scripts/*.sh` on disk, extracts the basenames from this table, and requires exact set parity plus uniqueness — failing on a script absent from the table, a row naming a script that no longer exists, and a script listed twice. Its `--self-test` proves all three directions fail when they should and asserts each mutation **actually changed the subject**, because a guard whose mutation silently became a no-op reports "rejected" for a tree containing no defect at all. The guard was found to be correct the moment it was written: on first run against the pre-corrective tree it failed with `` `scripts/check-doc-citations.py` exists but is absent from the inventory ``.

The guard deliberately stops at set parity. *Which workflow invokes a script* is natural language, and automating it means parsing English for "wired into" — encoding one reviewer's phrasing as a release contract and failing on a correct reword. Wiring is a reviewer's job, and the table is shaped so it can be reviewed.

### Per-file inline test distribution

`mod tests opens at` is the line of the module's `#[cfg(test)]` attribute;
`lines in test module` counts that line to end of file. Both are recomputed
from `src/`, never copied between documents.

| File | `#[test]` | `mod tests` opens at | Total lines | Lines in test module |
|---|---:|---:|---:|---:|
| `src/cleanup.rs` | 71 | 2393 | 6347 | 3955 |
| `src/workspace.rs` | 47 | 1446 | 3918 | 2473 |
| `src/update.rs` | 44 | 1211 | 2635 | 1425 |
| `src/discovery.rs` | 32 | 902 | 1866 | 965 |
| `src/cli.rs` | 24 | 381 | 901 | 521 |
| `src/config.rs` | 14 | 374 | 632 | 259 |
| `src/progress.rs` | 12 | 499 | 703 | 205 |
| `src/discovery_state.rs` | 11 | 402 | 656 | 255 |
| `src/output.rs` | 11 | 426 | 671 | 246 |
| `src/policy.rs` | 7 | 248 | 396 | 149 |
| `src/report.rs` | 7 | 72 | 195 | 124 |
| `src/traverse.rs` | 6 | 470 | 599 | 130 |
| `src/editor.rs` | 2 | 156 | 215 | 60 |
| `src/domain.rs` | 0 | — | 381 | — |
| `src/error.rs` | 0 | — | 16 | — |
| `src/lib.rs` | 0 | — | 15 | — |
| `src/main.rs` | 0 | — | 760 | — |
| **Total** | **288** | | | |

**The count is declared, not executed.** 288 `#[test]` functions exist in `src/`;
**287 compile and run on Linux**, because exactly one is gated to the other
platforms (`discovery.rs:1181`, `#[cfg(any(target_os = "macos", windows))]`). A
further 20 carry `#[cfg(unix)]` (18) or `#[cfg(target_os = "linux")]` (2), so on
Windows and on macOS the inline suite is 267 of 288. A green local run therefore
does not mean a green run elsewhere — which is the whole reason the OS matrix
exists, and the reason `check-fixture-portability.py` exists to catch a *new*
ungated fixture before a lane disagrees about it.

`cleanup.rs` (71) + `workspace.rs` (47) = 118 of 288 = **41.0%**. The two largest and most
safety-critical modules — authorization, ownership proof, pre-spawn decision, and the `cargo
metadata` resolution everything depends on — hold essentially half the unit tests. That
allocation is correct, and it is the one place the pyramid is emphatically *not* inverted.

### The structural observation: the testing is inverted

The convention is a separate `tests/` tree beside a lean `src/`. This crate does the
opposite. `cleanup.rs` is 6347 lines of which **3955 are test code** — 62% of that file is
fake-Cargo runners and assertion bodies. `workspace.rs` is 3918 lines, 2473 of them tests.
`report.rs` runs the other way: its test module opens at line 72, roughly the first third,
so the rendering tests sit *above* most of the production code they exercise.

The consequence for review is concrete. Opening `src/cleanup.rs` to understand the pre-spawn
decision puts 3954 lines of `FakeCleanupRunner` variants (`cleanup.rs:2394`–`6347`) between
the reader and nearly all production code. Locate the boundary first — `mod tests {` at
`cleanup.rs:2394` — and read upward. Do not infer "few tests" from a file's position in the
tree, and do not count `cleanup.rs` as production code when estimating where complexity
lives.

---

## 3. Inline unit tests

### The test-double architecture

Two process seams keep the 288 declared tests from spawning a real `cargo`:

- **`CargoRunner`** — `src/workspace.rs:29`. `fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>`.
- **`CleanupRunner`** — declared in `src/cleanup.rs` for the destructive step, so cleanup is exercised while `cargo clean` is a no-op returning recorded bytes.

Two further seams remove the remaining side effects:

- **`UpdateEnvironment`** (`src/update.rs`) — the entire network/filesystem surface of self-update, so `update.rs` tests need neither network nor process globals.
- **`ProgressObserver`** (`src/progress.rs:39`) — `NoopObserver` (`progress.rs:56`) is the zero-cost default the cleanup tests pass in, so progress rendering never interferes with an assertion.

### Catalogue of fakes

Production impls, for contrast: `SystemCargoRunner` (`workspace.rs:78`),
`SystemCleanupRunner` (`cleanup.rs:438`, impls `CargoRunner` at `:440`),
`WorkspaceCleanupAdapter<'_>` (`cleanup.rs:1402`), `ProofRefreshMeter<'_>`
(`cleanup.rs:1554`), `HttpEnvironment` (`update.rs:820`), `IndicatifRenderer`
(`progress.rs:187`), `NoopObserver` (`progress.rs:56`).

| Test fake | File:line | Failure mode simulated |
|---|---|---|
| `FakeCargo` | `workspace.rs:1598` (impl 1608) | Baseline. Flags `fail_locate`, `fail_metadata`, `malformed` cover non-zero exit and unparseable `metadata` JSON |
| `CountingRunner` | `workspace.rs:1716` (impl 1720) | Asserts `cargo metadata` is invoked exactly N times — the no-redundant-call invariant |
| `MemberFirstFails` | `workspace.rs:1978` (impl 1983) | `locate-project` fails for the member manifest but not the root; proves the root still resolves |
| `TwoWsRunner` | `workspace.rs:2150` (impl 2156) | Two independent workspaces; pins *two locates, two metadatas* — no cross-workspace reuse |
| `FailFirstRunner` | `workspace.rs:2219` (impl 2223) | One workspace's locate fails; proves the failure does not poison an unrelated one |
| `RecordingRunner` | `workspace.rs:3371` (impl 3375) | Records `(cwd, args)` so a test asserts argument *construction*, not the fake's result |
| `FixtureEnvironment` | `update.rs:1222` (impl 1357) | Replaces network, `curl` discovery, process globals; drives `staging` and a fixed `version`. Records **its own** staging paths (`created_staging`, `:1310`) so leak evidence is invocation-scoped rather than a scan of the temp directory — see [§7](#7-the-false-green-problem) |
| `TestObserver` | `progress.rs:64` (impl 105) | Records semantic events (`phases`, `visited`, `pruned`, `manifests`, `workspaces`, `cargo_failures`) — deliberately not refresh counts, which are renderer-side |
| `FakeCleanupRunner` | `cleanup.rs:2403` (impls 2436 / 2519) | The workhorse. Records every call; `remove_on_execute` and `unresolved_manifest` toggle Execute's delete and the unresolved-manifest block |
| `ChangingTargetRunner` | `cleanup.rs:3252` (impls 3258/3278) | `target` changes between calls — the first-generation race |
| `PerPathFailRunner` | `cleanup.rs:3314` (impls 3318/3361) | One manifest path fails to locate; proves the *entire* selected scope is blocked, not just the failing unit |
| `RaceRunner` | `cleanup.rs:3989` (impls 4025/4088) | Mutates the filesystem on the *second* `metadata` call — between scan and final proof; `clean_calls` proves no clean was staged |
| `BuildChangingRunner` | `cleanup.rs:4211` (impls 4217/4262) | Swaps `build/<hash>` between resolution passes; targets marker-qualification revalidation |
| `StagedCargo` | `cleanup.rs:4689` (impls 4795/4882) | Returns `"mock cargo clean success"` — a clean that succeeds *without deleting*, so the test asserts the call, not the filesystem |
| `ParallelProbeRunner` | `cleanup.rs:4900` (impls 4906/4927) | Tracks peak concurrent `metadata` calls; proves the parallel fan-out is real |
| `MetadataMutationRunner` | `cleanup.rs:4948` (impls 4954/4999) | Wraps `StagedCargo`, mutating emitted metadata (`MetadataMutation` at `cleanup.rs:4942`: `WorkspaceRoot`, `AddMember`, `Malformed`) after the inner call |
| `MetadataOnly` | `tests/end_to_end.rs:73` (impl 75) | Integration-suite Cargo stub — the only fake outside `src/` |

Two deserve a note. `RecordingRunner` carries a comment (`workspace.rs:3361-3368`) stating
the *premise* of its own test: a prior implementation "resolved zero workspaces, exited 0,
and reported no groups", and a recording runner catches that at the argument level without
the network. That is the right instinct — assert on the call, not on the outcome the fake
produces. `RaceRunner`'s `mutate` is a closure parameter, so each race case supplies its own
mutation rather than the fake hardcoding one scenario.

`RecordingRunner` is also now defined *inside* its test function
(`workspace.rs:3370`) rather than at module scope, because exactly one case needs it. That
is the more honest shape: a fake used once is not a reusable seam.

### Zero-test modules

`grep -cE '^\s*#\[test\]'` over `src/domain.rs src/error.rs src/lib.rs src/main.rs`
returns `0` for all four. They split into two very different groups.

**Defensible.** `lib.rs` (15 lines, module manifest), `error.rs` (16 lines, six `AppError` variants), and `domain.rs` (381 lines of pure data types — no I/O, no branching on process state). These are exercised transitively by every test in the modules that consume them.

**Not defensible, and significant.**

- **`src/main.rs` (760 lines, 0 tests).** The composition root. It *is* covered — `cli_contract.rs` runs the binary as a subprocess, so `main()` and `run()` execute in all 30 cases, including exit-code assertions at `cli_contract.rs:361` (`Some(1)`), `:980` (`Some(1)`) and `:724` (`Some(17)`). But that is black-box coverage of an opaque process: nothing asserts on `run()`'s structure, and the exit-code table is only as complete as the cases someone remembered to write. `main.rs` grew by 154 lines when M012A/M012B rewrote the dispatch, the format gate, and the bare-`cargo cleanme` path — with no test able to notice.
- **`update_json` (`main.rs:755`)** is the sharpest single gap. The only machine-readable output path that does **not** go through `EnvelopeV1`; it hand-builds a `serde_json::Map` with its own `json!(1)` at `main.rs:734` and a bespoke `result` object at `:724`. It has no test of any kind, and it is the one place where a schema version could silently disagree with the envelope's.

### The module that left the list: `output.rs`

`src/output.rs` was one of the five zero-test modules until M012B added `--format log`. It now carries **11 `#[test]` functions**, every one of them inside `pub mod log`'s own `#[cfg(test)] mod tests` (`output.rs:426-671`). They cover what the log line newly made risky and previously untested: one-line bounded ASCII output (`assert_ascii_and_bounded`, `output.rs:482`), byte-limit truncation that drops an overlong optional whole rather than mid-token (`output.rs:613`), the structural head fitting at the worst case (`output.rs:644`), field-order determinism across calls (`output.rs:658`), and the status triage for blocked / failed / ok (`output.rs:547`, `:572`, `:595`).

That is a real gain and it is also the *easy* half. The envelope layer that predates M012B is unchanged and still has no in-file test: `EnvelopeV1` (`output.rs:8`), `scan()` (`:103`) and `cleanup()` (`:158`) hardcode `schema_version: 1` at `:140` and `:206` with nothing inside `src/` pinning either the struct shape or the literal. Those are asserted only from outside the process, so adding a field to `EnvelopeV1` still requires someone to notice. Reading "the zero-test list shrank" as "the DTO layer is now pinned" would be the wrong conclusion.

### `traverse.rs`

Its test module opens at line 470 and contains **6** `#[test]` functions. For the module
that walks the entire filesystem — parallel size and recency measurement, symlink and
`CACHEDIR.TAG` handling, `ReadDir` error attribution, pruning — that is low. The
parallel-probe machinery has a dedicated counting fake in `cleanup.rs`
(`ParallelProbeRunner`, `cleanup.rs:4900`); `traverse.rs` has nothing comparable.

`source_activity` is declared at `src/traverse.rs:268` and has exactly one call site in the
crate: `src/traverse.rs:594`, inside the test module opening at 470. **There is no
production caller** — production uses `workspace_source_activity` (`src/workspace.rs:1428`)
instead. So `traverse::source_activity` is a `pub` function with no production consumer
whose sole exercise is its own test. That test proves the function works and nothing about
the system's behaviour: a coverage claim that looks green and is empty, and a milder cousin
of the C009–C013 incidents.

---

## 4. Integration suites

### How they invoke the binary

**Definitive answer: as a subprocess.** Both suites use `env!("CARGO_BIN_EXE_cargo-cleanme")` with `std::process::Command`, never a library call. `tests/cli_contract.rs:15` resolves the staged binary path; it is used at 31 sites (lines 96, 127, 158, 182, 234, 277, 311, 348, 445, 500, 530, 569, 603, 638, 715, 808, 938, 1114, 1159, 1195, 1355, 1395, 1423, 1466, 1491, 1543, 1569, 1609, 1665, 1697). Three cases go further, invoking a real `cargo` as `Command::new("cargo")` with `cleanme` first (lines 101, 135, 166) to compare external subcommand behaviour against direct invocation. `end_to_end.rs` is the exception: it drives the library's scan entry point in-process with a `MetadataOnly` Cargo stub (`end_to_end.rs:73-75`).

Consequence: `main.rs` executes end to end in all 30 `cli_contract.rs` cases — argv
normalization (`Cli::parse_normalized`), dispatch, exit codes — but only as an opaque
process. `run()`'s internals are untested, and the `update` command has no integration case
at all.

### `tests/common/mod.rs`

229 lines, no tests. Exports `set_path_modified` (`:24`) and `backdate_file` (`:98`), which
set file mtimes directly, plus `inactive_project` (`:111`), which writes a manifest-and-source
tree. `set_path_modified` is hand-rolled per platform because the inactivity threshold is
time-based and tests must not sleep: `File::set_modified` on Unix (`:84-94`, with the
FILETIME-ticks epoch constant at `:36`) and `SetFileTime` on Windows (`:70`). This is the
most load-bearing piece of portable test infrastructure in the repository — without it
"inactive" cannot be tested at all, and on Windows the naive approach was precisely the
C009 defect class.

Two unix-only helpers joined the harness since: `write_fake_cargo` (`:167`) writes the
`cargo` stub that the `#[cfg(unix)]` Execute cases put on `PATH`, and `cargo_calls`
(`:223`) counts the log entries it appends. Both are gated `#[cfg(unix)]` with the reason
recorded in the doc comment at `:158`, which is exactly the discipline the fixture
portability checker exists to enforce.

### `tests/cli_contract.rs` — 30 cases

Eleven run on every platform; eighteen are `#[cfg(unix)]` because they need the
`write_fake_cargo` stub on `PATH`.

| Test | Line | Contract pinned |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 80 | The premise: the built artifact is reachable as `cargo-cleanme` on `PATH` and is what `cargo` dispatches to. Without it, the three "external == direct" comparisons prove nothing |
| `cargo_external_subcommand_help_matches_direct_help` | 120 | `cargo cleanme --version` and `--help` byte-match direct invocation |
| `cargo_external_config_edit_help_matches_direct` | 154 | Same for the subcommand form, which exercises a different clap path |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 177 | The scan envelope, plus stream separation (`cli_contract.rs:198`, `:203`) and newline discipline |
| `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` | 219 | `--known` reports the Routine scope; an explicit `--scope` reports that scope; `scan.root` does not |
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 268 | Contradictory scope selectors exit 2 without walking anything — fail-closed at the boundary |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 300 | `clean --dry-run` envelope and summary |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 339 | A blocked scope still emits parseable JSON *and* exits 1 (`cli_contract.rs:361`). That pairing is the automation contract |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 379 | `#[cfg(unix)]`. Unattended Execute reaches a real `cargo clean` through a fake, emitting `reason_code: "cleaned"` with selector fields and byte counts |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 676 | `config edit` really execs a process (`$EDITOR`), and an invalid edit is rejected rather than written |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` | 856 | M012A: bare `cargo cleanme` resolves to `CleanupScope::Maintenance` and runs cleanup, not a scan |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` | 893 | `--dry-run` is application simulation: the same decision path, zero `cargo clean` processes |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` | 935 | The empty-roots short circuit exits 0 with the human notice, having spawned nothing |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` | 969 | Unresolved ownership blocks the whole scope and runs no clean |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` | 1002 | `clean` defaults to Execute; Cargo's own preview is `--cargo-preview`, not the default |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` | 1060 | `--dryrun`/`--yes` are hidden aliases resolving to the same modes as `--dry-run`/`--yes` |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` | 1098 | Rootless `scan` is Full and ignores `scan.root`; `scan --known` is the Routine inventory |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 1155 | An empty root is a valid zero-group report, exit 0 — not an error |
| `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` | 1258 | M012B: the shared `assert_bounded_log_line` contract (`cli_contract.rs:1218`) — ASCII, ≤ `MAX_BYTES`, exactly one newline, `cargo-cleanme op=` prefix, no control sequences — and nothing on stderr |
| `log_mode_distinguishes_simulate_from_cargo_preview` | 1292 | `mode=simulate` and `mode=preview` are distinct on the wire, matching `CleanMode::as_str` |
| `log_mode_scope_block_is_one_line_with_a_typed_reason_and_exit_one` | 1325 | Blocked scope → one line, typed `reason=`, exit 1 |
| `log_mode_failed_cleanup_is_one_line_with_status_failed_and_exit_one` | 1353 | A failed unit degrades to `status=failed` without a scope reason |
| `log_mode_zero_result_maintenance_is_status_ok` | 1392 | Zero groups is `status=ok`, not a special case |
| `log_mode_scan_reports_the_resolved_scope` | 1418 | The scan log line carries the same resolved-scope label as the JSON envelope |
| `log_mode_stats_is_opt_in_stderr_and_leaves_the_single_line_alone` | 1486 | `--stats` stays an explicit stderr override; stdout is byte-identical with and without it (`cli_contract.rs:1512-1514`) |
| `log_mode_suppresses_normal_diagnostic_fan_out` | 1536 | The per-diagnostic stderr fan-out is gated off in log mode; the count rides in `diagnostics=N` |
| `log_mode_fatal_error_is_one_bounded_stderr_line_and_no_stdout` | 1605 | A fatal error is a bounded stderr line; stdout stays empty, so a consumer reads one stream |
| `json_and_human_output_are_unchanged_by_log_mode` | 1648 | Adding the third format did not perturb the two existing byte-for-byte outputs |
| `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 1693 | `log` is an accepted `--format` value; an unknown one still exits 2 (`cli_contract.rs:1709`) |

### The machine-contract checklist

This is the real, executable shape of the versioned JSON contract. Every item is a live
assertion; change the DTO and these are the lines that fail.

| Assertion | Line |
|---|---:|
| `["schema_version"] == 1` (scan) | 200 |
| `["operation"] == "scan"` | 201 |
| `["scope"] == "explicit"` | 202 |
| `--stats` output appears on **stderr** (`"scan stats:"`), not stdout | 203 |
| `--stats` does not perturb stdout | 198 |
| stdout contains **exactly one** `\n` — one document, no trailing blank line | 204 |
| resolved scope is reported, not the CLI flags (Routine / explicit / configured root) | 250, 258 |
| `["schema_version"] == 1` (cleanup) | 331 |
| `["operation"] == "clean"` | 332 |
| `["mode"] == "simulate"` — the requested mode is echoed, not inferred | 333 |
| `["result"]["summary"]["simulated"]` (zero for `--cargo-preview`) | 334 |
| single trailing newline on cleanup stdout | 335 |
| exit code `2` on conflicting scope selectors, before traversal | 282 |
| exit code `1` on a blocked scope | 361 |
| `["result"]["scope_blocked"] == true` | 363 |
| `["result"]["scope_reason"]` is a typed reason, not prose | 985 |
| `["result"]["summary"]["cleaned"] == 1` | 472 |
| `["result"]["units"][0]["reason_code"] == "cleaned"` | 473 |
| `["result"]["selector_kind"] == "profile"` | 474 |
| `["result"]["selector_value"] == "dev"` | 475 |
| `units[0]["before_bytes"]` present and numeric | 477-480 |
| `units[0]["selector_estimate_bytes"]` present and numeric | 481-484 |
| `units[0]["output_union_before_bytes"]` present and numeric | 485-488 |
| simulate mode reports `outcome == "simulated"` | 520 |
| policy-disposition case reports its `reason_code` | 553 |
| policy-disposition case reports `policy_disposition` | 557 |
| invalid selector reports `reason_code` | 591 |
| unsupported selector (too-old Cargo) reports `reason_code` | 626 |
| package selector succeeds: `reason_code: "cleaned"` | 659 |
| `selector_kind == "package"` | 660 |
| `selector_value == "fixture@0.1.0"` — name **and** version | 661 |
| non-zero exit code `17` for the internal-failure path | 724 |
| `mode=preview` distinct from `mode=simulate` (Cargo's own preview) | 1028 |
| `--format log` accepted; an invalid format still exits `2` | 1709 |

`tests/end_to_end.rs:203-205` adds three: `["result"]["groups"][0]["ownership"] ==
"private"`, `["result"]["summary"]["group_count"] == 1`, and `groups[0]["bytes"]` as a
`u64`.

The three byte-count assertions are separated deliberately: `before_bytes` is one unit's own
size, `selector_estimate_bytes` what the selector's evidence justified *before* the final
proof, and `output_union_before_bytes` the union across its complete output set. Conflating
any two would misstate what a cleanup reclaims — or claim a bound the proof never reached.

### What the suites deliberately do not pin

- **No `update` JSON.** `update_json` is unasserted — no `schema_version` check, no newline invariant, no exit code, no case in either suite.
- **No negative CLI parsing coverage beyond the exit code.** `cli.rs` now has 24 inline tests, but the suites pin only the code, never the message: `2` for usage conflicts (`cli_contract.rs:282`, `:1709`), `1` for a blocked scope (`:361`), `17` for the internal path (`:724`). Nothing asserts *which* usage error was reported.
- **No assertion on human-readable output.** The default `--format text` path through `report::render` is covered only by `report.rs`'s 7 inline tests.
- **No destructive Execute case on Windows.** Every case that actually drives a fake `cargo` is `#[cfg(unix)]` — seven of the eleven new M012A cases, plus `json_unattended_yes_executes_through_cargo_and_emits_typed_result` (`cli_contract.rs:379`). Windows therefore exercises the argv, dispatch, and envelope layers but never the spawn-and-delete layer.
- **No timing, progress, or TTY behaviour.** `--no-progress` is passed (`cli_contract.rs:183`) to make output deterministic, so `IndicatifRenderer` and the terminal-capability detection in `progress.rs` are integration-untested by construction. The log-mode cases assert the *absence* of control sequences on stdout (`cli_contract.rs:1236-1239`), which is the closest the suites come.

---

## 5. The contract checkers

17 scripts. The dividing line is the one this project cares about: **outcome checkers** fail
when the subject misbehaves; **premise checkers** fail when the *evidence* is no longer
valid. Premise checkers are the distinctive category.

| Script | Class | Enforces | Wired into |
|---|---|---|---|
| `check-release-contract.py` | premise | Compares product surfaces against Eggpack producer authority: every contracted target reachable through the wrappers, the generated workflow, the packaging payload and the README support matrix; wrapper target mapping is a projection of the contract rather than a second schema; the packaging payload ships the wrappers the release needs; the package metadata the contract implies is in `Cargo.toml`; every required target builds under one **exact** Rust release, not a channel; the candidate workflow may stage a draft but never publish. Retains the cargo-cleanme-owned invariants: the `include` allowlist is explicit and contains `/config.toml`, the repository URL matches the release origin, the `xtask` generator stays out of the published crate, license files exist when SPDX claims them | `ci.yml` `generated-docs` `:86`, `:91`; `release-drift.yml` `:49`, `:55`; `release-check.sh` `:71`, `:72` |
| `check-fixture-portability.py` | premise | Every external-process fixture in `src/**/*.rs` uses `#[cfg(unix)]`/`#[cfg(windows)]` gating, avoids unconditional `/bin/sh`, `sh -c`, `.sh` scripts, hardcoded `/tmp`, `:`-separated `PATH`, and a fixed fake binary name that collides across gates | `ci.yml` `checks` `:34`, `:36`; `release-check.sh` `:125`, `:126` |
| `check-doc-citations.py` | premise | `file.rs:N` citations in `architecture/` resolve to an existing file and an in-range, correctly ordered line; **and** the [§5](#5-the-contract-checkers) inventory is exactly the set of scripts in `scripts/`, in both directions, without duplicates | `ci.yml` `checks` `:42`, `:44`; `release-check.sh` `:129`, `:130` |
| `check-installer-contract.py` | premise | The wrappers under `packaging/installers/` exist, are executable, carry the expected shebang, and the release manifest lists the expected per-platform entries | `ci.yml` `generated-docs` `:113`; `release-drift.yml` `:61`; `release-check.sh` `:83` |
| `check-release-identity.py` | premise | Tag/source identity: the release tag matches the source commit, version and tag agree, and the provenance identity machine is internally consistent | `release-drift.yml` `:58`; `validate-staged-release.yml` `:131`; `release-check.sh` `:76`, `:79` |
| `check-post-release-smoke-contract.py` | premise | The post-release smoke workflow is itself well-formed: required steps, lanes, and assertions present and in order | `post-release-smoke.yml` `:155`; `release-drift.yml` `:65`, `:66`; `release-check.sh` `:86`, `:87` |
| `check-staged-validation-contract.py` | premise | The hosted validation workflow is upstream-bound and read-only: exact upstream workflow name, `completed` only, no push/pull/schedule, `contents: read` only, no publication command, checkout bound to the upstream `head_sha`, upstream event *and* conclusion compared (not merely mentioned), identity and validator both invoked, no inlined reimplementation | `ci.yml` `generated-docs` `:109`, `:111`; `release-check.sh` `:99`, `:100` |
| `check-selector-qualification.py` | premise | The Cargo selector support claim is a projection of one policy: runtime allowlist == `release/selector-qualification.json` == hosted matrix == the script's own inputs, all exact `X.Y.Z`, package ⊆ profile, exploratory disjoint, workflow read-only and promotion-proof | `ci.yml` `generated-docs` `:98`, `:100`; `qualify-cargo-selectors.yml` `:65`, `:66`; `release-check.sh` `:106`, `:107` |
| `verify-release-attestation.py` | premise | A published release is immutable, carries a valid GitHub attestation for *that* repository and tag, and every contracted local asset hashes to the attested digest | **`release-check.sh` `:115`, `:116` only** — no workflow. Event-specific: needs a published, immutable release and a live `gh` |
| `resolve-smoke-transition.py` | premise | An exact `from`→`to` transition is resolved numerically, a predecessor is never silently skipped, and crates.io is polled within a finite deadline with backoff | `post-release-smoke.yml` `:95`, `:100`, `:103`; `ci.yml` `generated-docs` `:118`; `release-check.sh` `:93` |
| `gen-release-workflow-shape.py` | generator | Generates the *expected* shape of `release-binaries.yml` from the release contract, so the workflow is a checked-in generated artifact rather than hand-edited prose | `release-drift.yml` `:38`; `release-check.sh` `:61` |
| `validate-staged-release.py` | outcome | Six-step staged-release proof: exact artifact inventory, sidecar integrity (checksums), manifest agreement, contract agreement, static ABI, real-installer qualification — i.e. published bytes are the qualified bytes | **`validate-staged-release.yml` `:141` only** (M011B). Not in `release-check.sh` and not in any other workflow |
| `release-benchmark.py` | outcome | Compares current scan performance against `release/baseline-benchmark.json`; semantic counters are a hard gate and wall-clock timings are recorded but never fail | `ci.yml` `benchmark` `:130`; `release-check.sh` `:55` |
| `smoke-release-candidate.py` | outcome | Release-candidate smoke: the built binaries respond correctly before publication | `ci.yml` `checks` `:29`; `release-check.sh` `:133`; and indirectly on every release target, because `release/eggpack/consumer-validators.json` names it as the per-target validator invoked by the `qualify_build_*` jobs |
| `post-release-smoke.sh` | qualification | Post-publication verification against the *actual published bytes* — the external `update` check that first exposed C011 | **`post-release-smoke.yml` `:156` only.** Event-specific by construction: it can only run once a version exists on crates.io |
| `qualify-cargo-selectors.sh` | qualification | Real-Cargo selector characterization: what `--profile`/`--package` actually do on genuine Cargo, and where the support boundary is (M011D) | `qualify-cargo-selectors.yml` `:112`, `:146`; `ci.yml` `generated-docs` `:103`; `release-check.sh` `:108` |
| `release-check.sh` | composite gate | The local pre-push gate: full crate test suite, `cargo test --doc` (which CI does not run), all the Python checkers, the generated-shape comparison, `fmt`/`clippy` hygiene | **No workflow, by design.** Run by an operator; it refuses a dirty tree and requires `eggpack` and toolchain 1.89 |

Five notes:

1. **`/config.toml` in the allowlist is load-bearing.** `src/config.rs` embeds the template with `include_str!` at compile time, so a published crate without `/config.toml` in `include` builds fine and fails at runtime the first time it needs to create a config. `check-release-contract.py` asserts the entry specifically because no test and no runtime check can catch it — a pure packaging invariant, and a clear case of category (b).

2. **`check-fixture-portability.py` is a premise checker, and its own docstring (lines 23-32) states the limit of its guarantee.** It scans `src/**/*.rs` (globs at lines 54-59) and rejects statically-detectable POSIX-only fixture shape. It explicitly does **not** check whether a case's pass condition proves the intended branch — "reviewing that is the point of the C013 inventory". A static checker can catch a missing `#[cfg(unix)]`. It cannot catch a test that would have passed anyway. Read its green as "no visibly platform-shaped fixture is ungated", never as "every fixture proves something on every lane".

3. **The Phase 11 checkers all guard against the same shape, and it is a shape the
   repository has already met seven times: *a guard that cannot fail*.**
   `check-staged-validation-contract.py`, `check-selector-qualification.py`,
   `verify-release-attestation.py`, and `resolve-smoke-transition.py` each ship a
   `--self-test` that mutates the guarded artifact and requires a rejection, and three
   of them also assert a **control** — that the mutation actually changed something. That
   control is not decoration: during implementation, a self-test case reported "rejected"
   for a tree that contained no defect at all, because its mutation had silently become a
   no-op after an unrelated reformat. A guard with a no-op mutation is the worst outcome,
   because it converts a broken tree into a passing check. `check-doc-citations.py`'s
   inventory self-test follows the same rule for the same reason.

4. **`release-check.sh` closes a CI gap of its own making.** `ci.yml` runs `cargo test --all-targets --all-features`, which does not execute doctests; the local gate runs `cargo test --doc` separately. Doc examples are verified by a human with a shell script, not by CI.

5. **`check-installer-contract.py` is the one contract checker with no `--self-test`.**
   It has no argument parsing at all, so it cannot be required to reject a broken setup in
   CI. Every other checker in this table can prove its own detection in the failing
   direction; this one is trusted on inspection. That is a real asymmetry and the honest
   way to read its green.

---

## 6. CI as the enforcement point

`.github/workflows/ci.yml` defines five jobs, and they divide along lines worth
naming: **`checks` and `installers` run the product**, while `generated-docs`,
`benchmark` and `msrv` run the properties of the repository as an artifact. Six
of the ten Python guards CI invokes live in `generated-docs`, not `checks` — the
separation is easy to misremember, and an earlier version of this document did.

**`checks`** (`:4`) — the bulk of the gate. OS matrix of `ubuntu-latest`, `macos-latest`, `windows-latest`, so the unit tests that compile on this platform and the 30 integration cases (12 of them on a Windows lane) execute across all three, and on stable. Runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` (a real lint gate — warnings are errors), `cargo test --all-targets --all-features`, then **four** Python guards: `smoke-release-candidate.py` (`:29`), and two premise guards that each run their own `--self-test` first — `check-fixture-portability.py` (`--self-test` at `:34`, the checker at `:36`) and `check-doc-citations.py` (`--self-test` at `:42`, the checker at `:44`). A red build catches: any behavioural regression on any of the three platforms; any new warning; formatting drift; a new ungated POSIX-shaped fixture; a deep-dive citation whose line no longer resolves; **and, since C021, a verification script that exists but is missing from the inventory that documents it**. `--all-features` is what makes the matrix meaningful for the update path.

**`installers`** (`:45`) — runs the installer fixture suite `packaging/tests/test_installers.py` across the same three OSes (`:64`), with its own `--self-test` at `:71` on the Windows lane specifically, because that is where the C012 defect lived. The static wrapper check, `check-installer-contract.py`, is **not** here despite its name: it lives in `generated-docs` (`ci.yml:113`) and in `release-drift.yml:61`. Catches a wrapper that stopped being executable, lost its shebang, or diverged on one platform.

**`generated-docs`** (`:72`) — regenerates completions and manpages via `xtask` and fails if the committed files differ, then runs the other **six** Python guards: `check-release-contract.py` (`:86`, `--self-test` `:91`), `check-selector-qualification.py` (`:98`, `--self-test` `:100`), `qualify-cargo-selectors.sh --self-test` (`:103`), `check-staged-validation-contract.py` (`:109`, `--self-test` `:111`), `check-installer-contract.py` (`:113`), and `resolve-smoke-transition.py --self-test` (`:118`). Five of the seven per-commit `--self-test` invocations in CI live here. A *freshness* contract plus the repository's structural contracts; it is also the one place a script's output is compared to a committed artifact — the same pattern as `gen-release-workflow-shape.py`.

**`benchmark`** (`:119`) — runs `release-benchmark.py` against `release/baseline-benchmark.json` (`:130`). Semantic counters are a hard gate; wall-clock timings are recorded and printed but never fail, because shared-runner timing is noise. Catches a scan that has become materially slower in a way no correctness test notices.

**`msrv`** (`:131`) — a dedicated job pinned to Rust **1.89** via `dtolnay/rust-toolchain@master`, matching `rust-version = "1.89"` in `Cargo.toml`, against a crate on **edition 2024**. It runs `cargo +1.89 check --locked --all-targets` and `cargo +1.89 test --locked --all-targets`. A red build means the code has started using a feature stabilized after 1.89. Because edition 2024 requires a comparatively recent toolchain, the two settings are coupled and cannot drift apart silently.

Across `ci.yml`, **seven** guards run a `--self-test` on every push — `check-fixture-portability.py`, `check-doc-citations.py`, `check-release-contract.py`, `check-selector-qualification.py`, `qualify-cargo-selectors.sh`, `check-staged-validation-contract.py`, `resolve-smoke-transition.py` — plus `test_installers.py --self-test` at `:71`. The three not covered there are `check-post-release-smoke-contract.py`, `check-release-identity.py`, and `verify-release-attestation.py`, whose self-tests run from `release-drift.yml`, `validate-staged-release.yml`, and the operator gate respectively.

### Is every script wired into CI?

**Every one of the 17 is reachable from an automated surface, and 15 of them
from a workflow.** This was not true when this document was last written, and
the claim that it was — that `validate-staged-release.py` and
`qualify-cargo-selectors.sh` were invoked by nothing — was the single most
misleading sentence in the file. M011B and M011D wired both.

| Script | In a workflow? | In `release-check.sh`? |
|---|---|---|
| `check-release-contract.py` | yes (`ci.yml` `generated-docs`, `release-drift.yml`) | yes |
| `check-fixture-portability.py` | yes (`ci.yml` `checks`) | yes |
| `check-doc-citations.py` | yes (`ci.yml` `checks`) | yes |
| `check-installer-contract.py` | yes (`ci.yml` `generated-docs`, `release-drift.yml`) | yes |
| `check-release-identity.py` | yes (`release-drift.yml`, `validate-staged-release.yml`) | yes |
| `check-post-release-smoke-contract.py` | yes (`post-release-smoke.yml`, `release-drift.yml`) | yes |
| `check-staged-validation-contract.py` | yes (`ci.yml` `generated-docs`) | yes |
| `check-selector-qualification.py` | yes (`ci.yml` `generated-docs`, `qualify-cargo-selectors.yml`) | yes |
| `resolve-smoke-transition.py` | yes (`post-release-smoke.yml`, `ci.yml` `generated-docs`) | yes (`--self-test` only) |
| `gen-release-workflow-shape.py` | yes (`release-drift.yml`, output compared) | yes |
| `release-benchmark.py` | yes (`ci.yml` `benchmark`) | yes |
| `smoke-release-candidate.py` | yes (`ci.yml` `checks`; and per-target on every release via `release/eggpack/consumer-validators.json`) | yes |
| `qualify-cargo-selectors.sh` | yes (`qualify-cargo-selectors.yml`, `ci.yml` `--self-test`) | yes (`--self-test` only) |
| **`validate-staged-release.py`** | yes — `validate-staged-release.yml:141`, under M011B | **no** |
| **`post-release-smoke.sh`** | yes — `post-release-smoke.yml:156` | **no** |
| **`verify-release-attestation.py`** | **no** | yes |
| **`release-check.sh`** | **no** | n/a — it is the gate |

The three "no" entries are all explicable rather than accidental, and each is
event-specific by construction rather than merely unwired:

- **`validate-staged-release.py`** runs only from `validate-staged-release.yml`,
  and that workflow is `workflow_run`-bound to the upstream `stage` job: it
  exists to validate a staged draft, so before any release is staged there is
  nothing for it to do. It is the one script `release-check.sh` does not chain,
  which is correct — "I ran the local gate" and "I proved the release bytes"
  are different statements about different subjects, and merging them would make
  the local gate look stronger than it is.
- **`post-release-smoke.sh`** verifies the *actual published bytes* against
  crates.io. It cannot run before a version is published, and it is guarded for
  that reason by `check-post-release-smoke-contract.py` and
  `check-staged-validation-contract.py`.
- **`verify-release-attestation.py`** and **`release-check.sh`** are operator
  surfaces. The first needs a published immutable release and a live `gh`; the
  second refuses a dirty tree and requires `eggpack` and toolchain 1.89, which
  hosted runners are not guaranteed to provide.

The honest remaining gap is not "a script nobody runs". It is that **no artifact
in the repository records that any of these event-specific scripts actually
executed for a particular tag.** The wiring exists; the receipt does not. That is
why M011A/M011B/M011C closure records still list real-release evidence as
outstanding, and it is why `verify-release-attestation.py` is deliberately
reachable from the operator gate rather than only from a workflow.

---

## 7. The false-green problem

The reason this project has a `scripts/` directory at all. The account is the project's own,
from `plans/registry.md:92-107`, cross-checked against the code.

**C009 — a POSIX-only fixture passing on no platform.** A Windows fixture used POSIX-only behaviour, so it did not work on Windows *and* was not runnable on Unix. The hosted Windows lane — the evidence every Phase 10 milestone depended on — was broken, and the fixture was green nowhere. Closure restored the green hosted Windows lane (run `37225697173`, `plans/registry.md:69`). This motivated `check-fixture-portability.py`.

**C011 — a feature that could not reach its authority, green anyway.** `cargo cleanme update` returned HTTP 403 from crates.io on **every host**, because the production transport (`eggup-curl`) cannot set a `User-Agent`, which crates.io requires. The feature was green in CI because the *fixture registry always answered*. A stub standing in for an authority the subject cannot reach is indistinguishable from a stub that succeeds. Corrective C011 reversed the transport to `eggup-eggfetch` and added a no-downgrade guard (`plans/registry.md:75`); the transport choice is explained in `Cargo.toml:60-88` and documented in [Self-update](12-self-update.md). The original milestone's closure record was deliberately **not** edited to imply success — it carries an addendum naming the unsatisfied requirement (`plans/registry.md:73`, `:90`).

**C012 — a Windows case that had never executed.** Green only because an unrelated real `cargo install` failed first: the pass condition was an unrelated failure, so the case reported coverage it did not have. Fixed with a `cargo.cmd` shim, `os.pathsep`, and a **premise check**. Unusually, it was disclosed by the product *starting to work correctly* — once the real `cargo install` succeeded, the fake was no longer reached, and the case went red for the right reason (`plans/registry.md:76`, `:99`).

**C013 — the sweep, and its most serious finding.** A repository-wide sweep of external-process fixtures produced a 25-row inventory and corrected **nine** evidence defects, each new premise guard proven to fail in the failing direction (`plans/registry.md:77`). The most serious finding was **not a test**: the release-candidate smoke validator was green on all three hosted lanes *while the subject under test was failing*. Tightening it exposed a real product defect, registered as C015. `plans/registry.md:107` notes C013 closed without repairing any production defect, by its own failure semantics — which is what a verification milestone is for.

**C014 / C016 / C017 — two shipped product defects.** C016 (`plans/registry.md:80`, `:107`): the self-update identity check ran the downloaded candidate **with no arguments**, so its stdout could never equal a version string — and before failing, it performed a filesystem scan of the whole machine. The transaction aborted safely every time, so nothing shipped is corrupt, but the advertised feature had never once worked in any published version. C017 (`plans/registry.md:81`): a `cargo install --root` binary was misread as self-managed and **replaced**, silently, leaving `cargo install --list` reporting a version the file no longer had; it also parsed a `.crates.toml` schema no current Cargo writes — the only defect in the line that mutated on-disk state owned by another tool. Both were found by C014's live rehearsal against a published release, not by any test. `plans/registry.md:144` records the cost: the corrective line needed **seven published versions**, and three product defects (C015, C016, C017) were found by the line's own evidence work.

**C021 — a staging-cleanup test whose observation surface was wider than its subject.** Recorded here as a near-miss rather than a shipped defect: no product behaviour was wrong, and the test was green or flaky rather than confidently false. `staging_is_cleaned_up_on_success_and_on_failure` called a helper that enumerated the **process-wide** temp directory and reported every path whose basename began `cargo-cleanme-update-test-` — the same prefix every fixture in the module mints through `unique("cargo-cleanme-update-test")`. Because Rust runs tests in parallel, a sibling test's live staging directory could satisfy that scan and fail the assertion for a transaction that had removed its own directory correctly.

The two failures of premise are worth naming, because they are the same shape as the incidents above. First, the assertion's subject was ambiguous: *whose* staging directory? Second, and more reusable, **the test could not fail for its own reason and could not pass for its own reason either** — it was not a weak check, it was a check about the wrong thing. C021 proved the mechanism deterministically before changing anything (a foreign same-prefix directory makes the old scan fail on a subject that leaked nothing), then rebuilt the evidence on the paths each fixture itself created, and added two controls: a fixture that deliberately leaves *its own* staging directory behind, which the new check must reject, and a live foreign same-prefix directory, which the new check must ignore. Neither control is optional — without the first the check could not fail, and without the second it could not be distinguished from the old one.

### The generalised lesson

The project's own formulation (`plans/registry.md:92-105`):

> a green test proves only that its own premises hold, and both "the code is exercised" and "the code ran" are premises that go unchecked by default.

> a fixture that stubs a tool the subject cannot invoke is indistinguishable from no stub, and a case whose pass condition is an unrelated failure reports coverage that does not exist.

Both C011 and C012 were closed by landing a **premise assertion** — the transport's
User-Agent, the stub's resolvability — rather than only fixing the symptom. Fixing the
symptom would have made the test pass; the premise assertion is what makes it incapable of
passing for the wrong reason again.

### Assessment of the present tree

Now guarded:

| Defect class | Guard |
|---|---|
| POSIX-only fixture, ungated | `check-fixture-portability.py`, static, in `ci.yml` `checks` |
| Pass condition is an unrelated failure | C012's premise check; `packaging/tests/test_installers.py --self-test` (`self_test()` at line 785), which asserts each guard *rejects* a broken setup — "self test passed: every premise guard rejects a broken setup" (line 880) |
| Published crate missing a runtime file | `check-release-contract.py`, including the `include_str!`-embedded `/config.toml` |
| Tag/source identity drift | `check-release-identity.py` in `release-drift.yml` and `validate-staged-release.yml` |
| Smoke workflow edited into meaninglessness | `check-post-release-smoke-contract.py` |
| Docs/completions drift | `generated-docs` job |
| Documentation citing lines that no longer exist | `check-doc-citations.py`, in `ci.yml` `checks` |
| **Documentation inventory that silently lost a script** | `check-doc-citations.py` §5 parity rule — added by C021 |
| **A test whose observation surface is wider than its subject** | C021's fixture-owned staging evidence (`leaked_staging`, `src/update.rs:1310`) plus the foreign-path concurrency control at `:2451` |
| Performance regression | `benchmark` job vs `release/baseline-benchmark.json` |

**Residual risks in the present tree.**

**R1 — Event-specific verification has wiring but no receipts.** Three surfaces run only when a real release event occurs: `validate-staged-release.py` (a staged draft), `post-release-smoke.sh` (a published version), and `verify-release-attestation.py` (an immutable release plus a live `gh`). All three are now genuinely wired — the first two to workflows, the third to the operator gate — so the earlier claim that they were invoked by nothing is false and has been removed. What remains true is narrower and worth stating precisely: **no artifact in the repository records that any of them ran for a particular tag.** Nothing distinguishes "was run and passed" from "was never run", so a regression in which bytes get published would be caught only by the validator, and per the C013 lesson the validator is exactly the thing most likely to be edited down at the moment it starts failing. This is why C018, M011A, M011B, and M011C all still carry outstanding publication evidence rather than claiming closure.

**R2 — Real-Cargo selector behaviour is characterised by an event-specific harness.** `scripts/qualify-cargo-selectors.sh` is the only place genuine Cargo's `--profile`/`--package` semantics and the support boundary are measured, it is POSIX-only, and the full real-Cargo run is a weekly/dispatch lane rather than a per-commit one — CI runs its `--self-test` on every push. The automated coverage of selector behaviour (`cli_contract.rs:659-661`, `:591-626`) runs against a *stub* whose version string the guard itself reads. If Cargo changed what `--profile dev` selects, or moved the version floor, nothing in the per-commit gates would notice; the hosted matrix would, but not immediately.

**R3 — The `update` machine-output path has no test of any kind.** `update_json` (`main.rs:755`) is the only machine-readable surface bypassing `EnvelopeV1`, pinned by nothing: no `schema_version` assertion, no trailing-newline invariant, no exit code, no integration case. The two invariants that make the other JSON surfaces safe to automate against (`cli_contract.rs:204` single trailing newline, `:203` stream separation) do not extend to it — and M012B added a *fourth* JSON-shaped surface, `log`, whose shared helper `assert_bounded_log_line` (`cli_contract.rs:1218`) does encode those invariants in 18 cases. C016 and C017 both lived on the self-update path, whose automated output-shape evidence is nil.

**R4 — `traverse::source_activity` is test-only, and `traverse.rs` is thin.** `source_activity` (`src/traverse.rs:268`) has one call site, in its own test module (`traverse.rs:594`). A `pub` function with no production consumer whose only exercise is its own assertion is a coverage entry reporting capability the system does not use — a mild instance of a formally registered class. `traverse.rs` has 6 tests for the module that walks the whole filesystem.

**R5 — One checker cannot prove it can fail.** `check-installer-contract.py` is the only contract checker without a `--self-test`. Its green is an assertion about the tree that the repository never demonstrates in the failing direction, which is the C013 shape one step removed from the guards that do self-test.

**R6 — A test premise was process-global rather than invocation-scoped.** `staging_is_cleaned_up_on_success_and_on_failure` asserted that *no* `cargo-cleanme-update-test-` path existed anywhere in the temp directory, while every fixture in the module mints staging names from that same prefix and Rust runs tests in parallel. A concurrent test's live staging directory could therefore fail the assertion for a transaction that leaked nothing — the test could not pass for its own reason, and could not fail for its own reason either. C021 proved the mechanism deterministically and rebuilt the evidence on paths the fixture itself created. It is written up in [§7](#7-the-false-green-problem) history below because the class is not specific to staging directories: **a test whose observation surface is wider than the subject it claims to test can report another actor's behaviour as its own.**

---

## 8. Release verification

Three workflows verify properties of *published* artifacts — things no amount of in-tree
testing can see, because the subject is bytes on a registry and a CDN.

**`post-release-smoke.yml`** runs after publication against the **real published release**, not the staging tree. It is guarded by `check-post-release-smoke-contract.py`, which validates the smoke workflow's own shape — required steps, lanes, assertions — so the verifier cannot be quietly reduced to a shape that always passes. This is where C011 was found: `update` returned HTTP 403 in production while the suite was green. It drives `scripts/post-release-smoke.sh`. The lesson it encodes: the only trustworthy test of a distribution mechanism is one that uses the distribution mechanism.

**`release-binaries.yml`** builds the artifacts and, before publication, runs `check-release-identity.py` (tag/source identity), compares `release-binaries.yml` against the shape generated by `gen-release-workflow-shape.py` — so the workflow is a checked-in generated artifact, not hand-edited prose — and runs `smoke-release-candidate.py` against the candidates. Identity matters because a specific failure mode is invisible afterwards: if tag and source commit disagree, the release is irreproducible in a way no later check can detect. C014 added a machine-checked tag/source identity gate for this reason (`plans/registry.md:107`). `smoke-release-candidate.py` is the script C013 found green while its subject was failing; it is now a checked workflow step.

**`release-drift.yml`** verifies the release did not drift from the tag: that the commit a tag points at, the version in `Cargo.toml`, and the built artifacts still agree, and that regeneration is stable. It is the post-hoc detector for the class where someone amends or re-points a release after the fact.

**Relationship to `validate-staged-release.py` and Eggpack.** The Eggpack producer contract (`release/eggpack`, `packaging/`) supplies the generators and manifests these workflows consume. `validate-staged-release.py` is the offline six-step proof over a staged tree, invoked by `validate-staged-release.yml` under M011B, whose own premises are gated by `check-staged-validation-contract.py`. Its six steps map onto the questions each workflow asks, and the hosted workflow is what asks them automatically. It is still the one script `release-check.sh` does not chain, which is correct rather than an oversight — see R1 in [§7](#7-the-false-green-problem) for the narrower claim that survives.

**The benchmark.** `release/baseline-benchmark.json` records measured scan performance for a reference tree; `release-benchmark.py` re-measures and fails on a configurable regression percentage, run by the `benchmark` job. The regression it catches is the class no correctness test notices: a change that keeps every answer right but makes the scan substantially slower. With `cleanup.rs` at 6347 lines and the parallel probe fan-out a live optimisation surface (`ParallelProbeRunner` exists precisely to assert the fan-out), that is a realistic and otherwise invisible failure mode. The trade-off is inherent: a wall-clock threshold is flaky on shared runners, so this gate is the one most likely to be loosened — itself a thing to watch in review.

---

## 9. Coverage assessment

| Module | Lines | Tests | Tests / prod. lines | Verdict |
|---|---:|---:|---:|---|
| `cleanup.rs` | 6347 | 71 | 71 / 2392 | Appropriate. Most safety-critical module has the most tests; the race fakes show the hard cases were found and pinned |
| `workspace.rs` | 3918 | 47 | 47 / 1445 | Appropriate, and the fakes are argument-focused rather than result-focused — the right instinct |
| `update.rs` | 2635 | 44 | 44 / 1210 | Reasonable unit coverage, but this is the module whose defects (C011, C016, C017) escaped to production. Unit tests cannot see a transport that cannot reach its authority. Its transaction-hygiene evidence was briefly a false green too — see R6 |
| `discovery.rs` | 1866 | 32 | 32 / 901 | Adequate for a filesystem walk with attribution; the attribution logic is pure and testable |
| `cli.rs` | 901 | 24 | 24 / 380 | Good |
| `config.rs` | 632 | 14 | 14 / 373 | Good for its surface |
| `progress.rs` | 703 | 12 | 12 / 498 | Thin relative to size. Terminal-capability detection is mostly untested, and integration tests force `--no-progress` (`cli_contract.rs:183`), excluding the renderer from end-to-end coverage by construction |
| `discovery_state.rs` | 656 | 11 | 11 / 401 | Adequate |
| `output.rs` | 671 | 11 | 11 / 425 | **Split verdict, and the split is the finding.** All 11 tests are inside `pub mod log` (`output.rs:426`) and cover the bounded-ASCII-line contract hard — size bound, whole-value truncation, worst-case head fit, field-order determinism, status triage. The *envelope* layer above it (`EnvelopeV1` at `output.rs:8`, `scan()` at `:103`, `cleanup()` at `:158`, both `schema_version: 1` literals at `:140` and `:206`) still has **zero** in-file tests |
| `report.rs` | 195 | 7 | 7 / 71 | **Unusually good for its size.** Covers control-character and non-UTF-8 path escaping, zero-state wording, the full `format_bytes` ladder including `u64::MAX`, group ordering with the path tie-break, and total de-duplication across equal physical roots. Two are `#[cfg(unix)]` (`report.rs:88`, `:101`) because they use `OsString::from_vec` |
| `policy.rs` | 396 | 7 | 7 / 247 | Still low for the module deciding the Routine/Full scope boundary, though better than it was. Pure and cheap to test; the gap is real |
| `traverse.rs` | 599 | 6 | 6 / 469 | **Too low for the module that walks the whole filesystem.** No test double for traversal-scale error attribution; and `source_activity` (`:268`) has no production caller |
| `editor.rs` | 215 | 2 | 2 / 155 | Low, but the process-exec behaviour is covered end to end at `cli_contract.rs:676`, the right place for it |
| `domain.rs` | 381 | 0 | 0 / 381 | Defensible. Pure data, exercised transitively. Semantic *rules* live in the modules that interpret the types |
| `error.rs` | 16 | 0 | — | Defensible |
| `lib.rs` | 15 | 0 | — | Defensible |
| `main.rs` | 760 | 0 | 0 / 760 | **Zero inline tests for the composition root**, which grew 154 lines in M012A/M012B. Partly mitigated — `cli_contract.rs` runs the binary as a subprocess, so `main()` and `run()` execute in all 30 cases and three non-zero exit codes are pinned (`1` at `:361` and `:980`, `2` at `:282` and `:1709`, `17` at `:724`), with `0` asserted implicitly via `status.success()`. But `run()`'s structure is opaque, the format gate at `main.rs:483-484` has no test of any kind, and `update_json` (`:715`) is entirely unasserted |

The contrast in the last three rows is the finding. The *human* renderer beside the
*machine* DTO layer has 7 unusually strong tests, while the envelope layer below the log
renderer still has none — and the
one surface automation actually depends on has all its assertions living in a different
crate (`tests/`). Add a field to `EnvelopeV1` and nothing inside the crate fails; only
`cli_contract.rs` does, and only if a new case happens to look at it. M012B made this
sharper, not softer: `output.rs` left the zero-test list, so a reader scanning counts now
sees a covered module where the versioned DTOs are exactly as unpinned as before.

### Behaviours a reviewer should treat as under-verified

1. **`main.rs` composition and the exit-code table.** `run()` stitches the two pipelines together and maps outcomes to codes. Three are pinned (`1` asserted at `cli_contract.rs:361` and `:980`, `2` at `:282` and `:1709`, `17` at `:724`; the resolved-scope label test at `:219`); the rest are not. `main.rs:724-726` shows the `full_incomplete` → 1 path. (It used to read
   `full_incomplete || (full && !state_reconciled)`; that second clause is gone, and removing it is
   the fix recorded in [overview §7 item 8](overview.md).) The cleanup pipeline has its own `Ok(1)` route — a failed unit or a blocked scope (`main.rs:255-259`), now tested through the `scope_blocked.is_some()` shape M012A introduced — and M012A also made bare `cargo cleanme` a cleanup path, so `run()`'s dispatch is materially larger than when this list was written. None of that is visible from the outside except through the 11 new cases.
2. **`update_json` output shape** (`main.rs:738-760`). Bypasses `EnvelopeV1`, has its own `schema_version` literal at `main.rs:757`, no test. See R3.
3. **`output.rs` envelope field completeness.** `scan()` (`:103`) and `cleanup()` (`:158`): adding, removing, or renaming a field breaks no in-crate test. The 11 tests that do exist all live below `pub mod log`.
4. **`report.rs` rendering is covered, but not end to end.** The 7 inline tests are strong; the default `--format text` path is never asserted through the binary.
5. **`domain.rs` semantics.** Zero tests is defensible for the data shapes, but any *meaningful* method on those types is untested by construction — nothing would catch one being added.
6. **`traverse.rs` measurement.** 6 tests for parallel size/recency measurement. Symlink handling, `CACHEDIR.TAG` qualification, and per-entry `ReadDir` error attribution are what would go wrong on a real machine, and what the platform matrix would be best placed to catch — but the fakes are hand-rolled per test, so real behaviour is only sampled.
7. **The whole self-update path.** 42 inline tests against `FixtureEnvironment`, plus manual live rehearsal. Every published defect in this line escaped that apparatus, and the reason is structural: a fixture that replaces the network can never discover that the real network rejects you. The automated evidence is a substitute for the thing, not a check of it.
8. **Cross-platform behaviour generally.** 18 of the 29 `cli_contract.rs` cases are `#[cfg(unix)]`, so a Windows lane exercises the argv, dispatch, and envelope layers but never the spawn-and-delete layer. `check-fixture-portability.py` closes the ungated-POSIX-fixture hole, and its own docstring (lines 23-32) disclaims checking whether a case's pass condition proves its intended branch. That disclaimer *is* the guarantee's boundary: a green run means "no statically visible platform-shaped fixture is ungated", not "every fixture proved something on every lane". The C013 25-row inventory remains the only real audit of that question, and it is a point-in-time artifact, not a continuous guarantee.

---

## 10. Review checklist

1. **Does a new external-process fixture prove its own premise?** Add a guard that fails when the fake cannot be reached, before the behaviour assertion. References: C012's premise check; `packaging/tests/test_installers.py::self_test` (line 785), which proves each guard rejects a broken setup. Without failing-direction proof, it is a C012 waiting to happen.
2. **Is a new fixture platform-gated?** Any `#[cfg(test)]` block spawning a process, writing a shell script, or using `os.pathsep` needs `#[cfg(unix)]` / `#[cfg(windows)]` discipline or a portable implementation. `scripts/check-fixture-portability.py` catches the static shape, in `ci.yml` `checks`.
3. **Is a new external-process fixture actually exercised — not just present?** The premise is untested by default. Confirm the stub is *reached*: assert on what the subject passed to it (the `RecordingRunner` pattern, `workspace.rs:3371`), not only on what came back. C012 was green because an unrelated failure satisfied the assertion.
4. **Is a new `scripts/` file wired into a workflow or the operator gate?** All 17 are reachable from an automated surface; the three exceptions (`validate-staged-release.py`, `post-release-smoke.sh`, `verify-release-attestation.py`) are event-specific by construction and say so in the [§5](#5-the-contract-checkers) table — see [§6](#6-ci-as-the-enforcement-point). A new checker reachable from neither repeats that gap. Wire it, or document why not, **in the same change**: `check-doc-citations.py` will fail if the new script is not in the §5 table, so the omission cannot ship.
5. **Changing a field in `output.rs`?** There is no in-crate test below `pub mod log`. The only assertions are external: `cli_contract.rs:200-202`, `331-335`, `363`, `472-488`, `520`, `553-557`, `591`, `626`, `659-661`. Update them in the same commit; nothing inside the crate will remind you.
6. **Changing `update_json` (`main.rs:755`)?** Nothing will fail. It has no test and is not covered by the checklist invariants. Add a case, and consider routing it through `EnvelopeV1` so it stops being the one unversioned surface.
7. **Touching `output.rs:140` or `:206` (`schema_version` literals)?** Confirm the trailing-newline invariant (`cli_contract.rs:204`, `:335`) and stderr-separation invariant (`:203`) still hold for the surface you touched. They are the reason the JSON is safe to parse unattended. The log surface has its own, stronger invariant — `assert_bounded_log_line` (`cli_contract.rs:1218`) — so a log-mode edit should run those 11 cases rather than the JSON ones.
8. **New exit codes in `main.rs`** — add a subprocess assertion in `cli_contract.rs` beside the existing ones (`:361` for `1`, `:282` for `2`, `:724` for `17`, `:219` for the scope label). The exit-code table is otherwise unpinned.
9. **Touching `traverse.rs`?** 6 tests (module opens at `:470`), and `source_activity` (`:268`) has no production caller — its only call site is `traverse.rs:594`, inside its own test module. If you are adding a caller, say so; if not, its test proves nothing about the system.
10. **New behaviour in `policy.rs`** — only 7 tests, and this module decides the Routine/Full scope boundary, a documented invariant. Cheap to cover, still under-covered.
11. **Reactivating or adding a `.crates.toml` provenance path** — C017 was exactly this: a schema no current Cargo writes, misread as self-managed, which *replaced a binary Cargo owned*. The classification lives in `src/update.rs`, and its tests run against `FixtureEnvironment` (`update.rs:1222`), which cannot reproduce real Cargo's on-disk state.
12. **Any change to the three event-specific scripts** (`validate-staged-release.py`, `post-release-smoke.sh`, `verify-release-attestation.py`) — they have wiring but no receipts, so nothing records whether they ever ran. That is R1 in [§7](#7-the-false-green-problem), and the C013 lesson is that verifiers fail exactly when they start being useful.
13. **Does a new or edited test observe wider than its subject?** If an assertion inspects a shared surface — the process-wide temp directory, a global registry, one lock file — ask whose state it can see. `staging_is_cleaned_up_on_success_and_on_failure` scanned every `cargo-cleanme-update-test-*` path in `/tmp` while sibling tests minted names from the same prefix; the subject's own state was never actually checked. Bind the evidence to the fixture or transaction, and add the two controls C021 did: one that makes the check **fail** on a deliberate leak of its own state, and one that makes it **pass** while a foreign same-prefix path is live.
14. **Adding a checker without a `--self-test`?** `check-installer-contract.py` is the one existing exception, and §5 note 5 records it as an asymmetry rather than a precedent.

---

## See also

[Architecture overview](overview.md) · [Orchestration](13-orchestration.md) ·
[Cleanup](09-cleanup.md) · [Self-update](12-self-update.md) · [Reporting](10-reporting.md) ·
[Distribution & release](15-distribution-and-release.md) · and `plans/registry.md:92-107`
for the project's own account of the false-green incidents, in its own words.
