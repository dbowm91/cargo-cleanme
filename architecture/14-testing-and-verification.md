# Testing & Verification — how this project keeps its tests honest

> Component deep dive · part of the [architecture overview](overview.md)

---

## 1. Responsibility

The verification apparatus does three distinct jobs. The third is what makes it unusual.

**(a) Product behaviour.** 602 declared inline `#[test]` functions in `src/`, plus two
integration suites: one drives the compiled binary as a subprocess, the other drives the
library's scan entry point in-process. Between them they cover ownership proof, cleanup
authorization, the three clean modes, workspace resolution, real Cargo failure presentation,
repeated stale-root admission, interactive selection, and the versioned JSON contract.

**(b) Structural contracts invisible at runtime.** 17 scripts in `scripts/` and 6 GitHub
workflows. No product code path can detect a file left out of the published crate's `include`
allowlist, completions drifted from `cli.rs`, a tag pointing at the wrong commit, or release
bytes differing from qualified bytes. These are properties of the repository *as an artifact* —
see [Distribution & release](15-distribution-and-release.md). **The inventory of those 17
scripts is itself machine-checked**: `check-doc-citations.py` requires the
[§5](#5-the-contract-checkers) table to be exactly the set of scripts on disk, in both
directions and without duplicates, so a new checker cannot ship while the table keeps
describing an older repository.

**(c) Properties of the test fixtures themselves.** The category this project invented. A test
that stubs a tool the subject cannot invoke is indistinguishable from one that does not stub
it; a test whose pass condition is an unrelated failure reports coverage that does not exist.
Both classes shipped here ([§7](#7-the-false-green-problem)). The response was not more tests
but *premise guards*: assertions inside the fixture, before the behaviour under test runs, that
fail loudly when the fixture can no longer prove what it claims to.
`check-fixture-portability.py` generalises the idea into a static checker.

A green result here is not a sufficient claim on its own. The correct question is: *what
were the premises, and is any of them unchecked?*

---

## 2. The test pyramid as it actually stands

| Layer | Location | Count | What it can catch | What it cannot catch |
|---|---|---:|---|---|
| Inline unit tests | `src/**/*.rs`, in-file `#[cfg(test)] mod tests` | 602 declared `#[test]` | Logic errors, ownership/authorization decisions, argument construction, refactor regressions | Anything needing a real process, filesystem, or network; any premise of a fake |
| Integration — CLI contract | `tests/cli_contract.rs` (5,345 lines) | 83 `#[test]` | The subprocess boundary end to end: argv normalization, exit codes, JSON shape, log-line shape, `--stats` stream discipline, selector behaviour, usage conflicts, stale-learned-root admission, repeated-root noise bounds, incomplete-discovery stage counts, real Cargo failure summaries, default Routine execution against real Cargo | Internals of `run()`; anything on a real machine; platform-gated cases are excluded on other lanes |
| Integration — end to end | `tests/end_to_end.rs` (840 lines) | 4 `#[test]` | Full production-pipeline scans against synthetic trees with real mtimes, under both a filtered Routine scope and an explicit scope | Cleanup, update, config editing |
| Shared harness | `tests/common/mod.rs` (286 lines) | 0 tests | `set_path_modified` / `backdate_file` (per-platform mtime control), `inactive_project` tree construction, `write_fake_cargo` / `cargo_calls` (unix only) | — |
| Contract checkers | `scripts/` | **17** (see note) | Published-crate contents, fixture portability, installer/shape/smoke contracts, release identity, benchmark regression, and the completeness of this inventory | That a fixture case proves its intended branch (explicitly disclaimed) |
| CI | `.github/workflows/` | 6 | Cross-platform, cross-toolchain, MSRV, generated-doc freshness, release drift | Doctests (`cargo test --all-targets` skips them) |

**Note on the script count.** `scripts/` contains **17** regular files — 14 `*.py` and 3 `*.sh`,
no subdirectories. The count grew during Phase 11 as the selector-qualification,
staged-validation, smoke-transition, and release-attestation checks were added.

**Eleven of the seventeen implement a `--self-test` mode** — a mode that feeds the checker a
known-bad sample and requires a rejection. The six that do not are
`release-check.sh`, `gen-release-workflow-shape.py`, `release-benchmark.py`,
`smoke-release-candidate.py`, `validate-staged-release.py`, and `post-release-smoke.sh`.
`release-check.sh` is the interesting one: it *invokes* twelve other self-tests (script lines
72, 79, 86, 90, 97, 103, 110, 112, 119, 126, 129, 133) but provides none of its own, because
it is a composite gate rather than a checker. The other five have no subject a self-test could
sample: `gen-release-workflow-shape.py` generates an artifact that is compared to a committed
one, `release-benchmark.py` compares against a recorded baseline, and
`smoke-release-candidate.py`, `validate-staged-release.py` and `post-release-smoke.sh` each
need a real build, a real staged draft, or a real published release.

**The §5 inventory is now gated, and that is the substantive change.** It previously carried 16
rows against 17 files: `check-doc-citations.py` was missing, and nothing noticed, because a
prose table is nobody's obligation. `check-doc-citations.py` now enumerates `scripts/*.py` and
`scripts/*.sh` on disk, extracts the basenames from that table, and requires exact set parity
plus uniqueness — failing on a script absent from the table, a row naming a script that no
longer exists, and a script listed twice. Its `--self-test` proves all three directions fail
when they should and asserts each mutation **actually changed the subject**, because a guard
whose mutation silently became a no-op reports "rejected" for a tree containing no defect at
all. The guard was found to be correct the moment it was written: on first run against the
pre-corrective tree it failed with `` `scripts/check-doc-citations.py` exists but is absent from
the inventory ``.

The guard deliberately stops at set parity. *Which workflow invokes a script* is natural
language, and automating it means parsing English for "wired into" — encoding one reviewer's
phrasing as a release contract and failing on a correct reword. Wiring is a reviewer's job, and
the table is shaped so it can be reviewed.

**What the guard does not cover, and this matters more than the parity rule.** The citation
regex requires the `file.rs:N` form: a bare `:N` is invisible to it. The bare form is the
dominant convention inside the deep dives — `architecture/07-workspace.md` alone carries 448
bare `:N` references — so the large majority of in-file citations across the architecture set
are **not** range-checked by any tool. The gate covers the citations that happen to spell the
file name; it does not cover the ones that rely on the reader knowing which file they are in.
That asymmetry is the honest boundary of "machine-checked" in this repository, and it is why
§10's checklist asks a reviewer to re-derive by hand.

### Per-file inline test distribution

`mod tests opens at` is the line of the `mod … {` declaration that the
module's `#[cfg(test)]` attribute gates; `lines in test module` counts that
line to end of file. Both are recomputed
from `src/`, never copied between documents.

| File | `#[test]` | `mod tests` opens at | Total lines | Lines in test module |
|---|---:|---:|---:|---:|
| `src/cleanup.rs` | 138 | 4542 | 12174 | 7633 |
| `src/workspace.rs` | 87 | 2420 | 6930 | 4511 |
| `src/interactive.rs` | 76 | 1819 | 3606 | 1788 |
| `src/update.rs` | 54 | 1501 | 3342 | 1842 |
| `src/cli.rs` | 61 | 982 | 2874 | 1893 |
| `src/discovery.rs` | 42 | 1127 | 2646 | 1520 |
| `src/policy.rs` | 26 | 827 | 1666 | 840 |
| `src/output.rs` | 22 | 274 (`path_tests`), 513 (`tests`), 993 (`update_tests`) | 1113 | 840 |
| `src/config.rs` | 21 | 618 | 1547 | 930 |
| `src/discovery_state.rs` | 18 | 638 | 1335 | 698 |
| `src/progress.rs` | 16 | 543 (`test_env`), 570 (`tests`) | 1002 | 460 |
| `src/report.rs` | 11 | 192 | 387 | 196 |
| `src/main.rs` | 12 | 2010 | 2513 | 504 |
| `src/traverse.rs` | 12 | 772 | 1098 | 327 |
| `src/editor.rs` | 5 | 162 | 292 | 131 |
| `src/domain.rs` | 1 | 473 | 500 | 28 |
| `src/error.rs` | 0 | — | 16 | — |
| `src/lib.rs` | 0 | — | 16 | — |
| **Total** | **602** | | | |

**The count is declared, not executed.** 602 `#[test]` functions exist in `src/`; **600
compile on Linux x86_64**, because two are gated to the other platforms
(`config.rs:1044`, `#[cfg(windows)]`, and `discovery.rs:1678`,
`#[cfg(any(target_os = "macos", windows))]`). Beyond that, 50 more are `#[cfg(unix)]` and five
are Linux-only (`config.rs:1492`, `discovery.rs:1698`, `output.rs:281`, `traverse.rs:1076`,
`workspace.rs:6034`), so the other two lanes compile smaller sets. The three hosted lanes
therefore compile different sets — 600 on Linux x86_64, 596 on macOS, 547 on Windows. Platform-gated tests mean a green local run does not mean a green run
elsewhere — which is the whole reason the OS matrix exists, and the reason
`check-fixture-portability.py` exists to catch a *new* ungated fixture before a lane disagrees
about it.

> **Count the gates with a parser, not a grep.** A naive "the `#[cfg(unix)]`
> immediately above `#[test]`" search finds 38, not 50, because Rust allows
> `#[test]` to precede the gate — twelve of the 50 have it that way
> (`src/cleanup.rs:7591` does exactly that). The
> figures above come from a brace-tracking evaluation against
> `rustc --print cfg --target <triple>`, cross-checked against
> `cargo test --lib -- --list` on the host platform. The Windows total is
> **build-verified** as well: the three test binaries cross-compiled for
> `x86_64-pc-windows-gnu` carry exactly the derived set of 547 test names and
> none of the 55 excluded ones. The macOS total remains source-derived —
> the hosted CI matrix is the only thing that proves it.

`cleanup.rs` (138) + `workspace.rs` (87) = 225 of 602 = **37.4%**. The two largest and most
safety-critical modules — authorization, ownership proof, pre-spawn decision, and the `cargo
metadata` resolution everything depends on — hold roughly two fifths of the unit tests. That
allocation is correct, and it is the one place the pyramid is emphatically *not* inverted.

### The structural observation: the testing is inverted

The convention is a separate `tests/` tree beside a lean `src/`. This crate does the
opposite. `cleanup.rs` is 12,174 lines of which **7,638 are test code** — 63% of that file is
fake-Cargo runners and assertion bodies. `workspace.rs` is 6930 lines, 4512 of them tests.
`report.rs` runs the other way: its test module opens at line 191, halfway down its 387
lines, so the rendering tests sit *below* about half of the production code they exercise.

The consequence for review is concrete. Opening `src/cleanup.rs` to understand the pre-spawn
decision puts 7,757 lines of `FakeCleanupRunner` variants (`cleanup.rs:4418`–`12174`) between
the reader and nearly all production code. Locate the boundary first — `mod tests {` at
`cleanup.rs:4418` — and read upward. Do not infer "few tests" from a file's position in the
tree, and do not count `cleanup.rs` as production code when estimating where complexity
lives.

---

## 3. Inline unit tests

### The test-double architecture

Two process seams keep almost all of the 602 declared tests from spawning a real `cargo`:

- **`CargoRunner`** — `src/workspace.rs:29`. `fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>`.
- **`CleanupRunner`** — declared in `src/cleanup.rs` for the destructive step, so cleanup is exercised while `cargo clean` is a no-op returning recorded bytes.

Three test sites reach past the seam and drive the real `cargo` deliberately:
`workspace.rs:5572` (`a_relative_root_resolves_the_same_workspaces_as_its_absolute_spelling`),
`cleanup.rs:7558` (`real_temp_project_preview_and_execute_with_redirected_target`)
and `cleanup.rs:7697` (both inside
`real_cargo_package_selector_preserves_sibling_workspace_artifacts`).

A third real-Cargo call is made by *production* code rather than by a test, and it is the
one that matters for R7 below: `runtime_clean_capabilities` (`cleanup.rs:3944`) shells out to
`cargo --version` to decide which selectors the host Cargo supports. That is why a
*selector* case can still be sensitive to the host's Cargo version even where the Cargo it
drives is a fake: the capability gate is consulted before any command is built.

Two further seams remove the remaining side effects:

- **`UpdateEnvironment`** (`src/update.rs`) — the entire network/filesystem surface of self-update, so `update.rs` tests need neither network nor process globals.
- **`ProgressObserver`** (`src/progress.rs:39`) — `NoopObserver` (`progress.rs:57`) is the zero-cost default the cleanup tests pass in, so progress rendering never interferes with an assertion.

### Catalogue of fakes

Production impls, for contrast: `SystemCargoRunner` (`workspace.rs:78`),
`SystemCleanupRunner` (`cleanup.rs:1009`, impls `CargoRunner` at `:698` and `CleanupRunner` at
`:710`), `WorkspaceCleanupAdapter<'_>` (`cleanup.rs:2790`), `ProofRefreshMeter<'_>`
(`cleanup.rs:3061`), `HttpEnvironment` (`update.rs:854`), `IndicatifRenderer`
(`progress.rs:213`), `NoopObserver` (`progress.rs:57`).

| Test fake | File:line | Failure mode simulated |
|---|---|---|
| `FakeCargo` | `workspace.rs:2869` (impl 2879) | Baseline. Flags `fail_locate`, `fail_metadata`, `malformed` cover non-zero exit and unparseable `metadata` JSON |
| `CountingRunner` | `workspace.rs:3083` (impl 3087) | Asserts `cargo metadata` is invoked exactly N times — the no-redundant-call invariant |
| `MemberFirstFails` | `workspace.rs:3401` (impl 3406) | `locate-project` fails for the member manifest but not the root; proves the root still resolves |
| `TwoWsRunner` | `workspace.rs:3987` (impl 3993) | Two independent workspaces; pins *two locates, two metadatas* — no cross-workspace reuse |
| `FailFirstRunner` | `workspace.rs:4056` (impl 4060) | One workspace's locate fails; proves the failure does not poison an unrelated one |
| `RecordingRunner` | `workspace.rs:6134` (impl 6138) | Records `(cwd, args)` so a test asserts argument *construction*, not the fake's result |
| `FixtureEnvironment` | `update.rs:1511` (impl 1681) | Replaces network, `curl` discovery, process globals; drives `staging` and a fixed `version`. Records **its own** staging paths (`created_staging`, `update.rs:1316`) so leak evidence is invocation-scoped rather than a scan of the temp directory — see [§7](#7-the-false-green-problem) |
| `TestObserver` | `progress.rs:65` (impl 107) | Records semantic events (`phases`, `visited`, `pruned`, `manifests`, `workspaces`, `cargo_failures`) — deliberately not refresh counts, which are renderer-side |
| `FakeCleanupRunner` | `cleanup.rs:4755` (impls 4765 / 4788 / 4871) | The workhorse. Records every call; `remove_on_execute` and `unresolved_manifest` toggle Execute's delete and the unresolved-manifest block |
| `ChangingTargetRunner` | `cleanup.rs:7758` (impls 7764 / 7784) | `target` changes between calls — the first-generation race |
| `PerPathFailRunner` | `cleanup.rs:7905` (impls 7909 / 7952) | One manifest path fails to locate; proves the *entire* selected scope is blocked, not just the failing unit |
| `RaceRunner` | `cleanup.rs:8857` (impls 8867 / 8893 / 8956) | Mutates the filesystem on the *second* `metadata` call — between scan and final proof; `clean_calls` proves no clean was staged |
| `BuildChangingRunner` | `cleanup.rs:9113` (impls 9119 / 9164) | Swaps `build/<hash>` between resolution passes; targets marker-qualification revalidation |
| `StagedCargo` | `cleanup.rs:9609` (impls 9637 / 9747 / 9757 / 9850) | Returns `"mock cargo clean success"` — a clean that succeeds *without deleting*, so the test asserts the call, not the filesystem |
| `ParallelProbeRunner` | `cleanup.rs:9875` (impls 9880 / 9893) | Tracks peak concurrent `metadata` calls; proves the parallel fan-out is real |
| `MetadataMutationRunner` | `cleanup.rs:9914` (impls 9920 / 9965) | Wraps `StagedCargo`, mutating emitted metadata (`MetadataMutation` at `cleanup.rs:8562`: `WorkspaceRoot`, `AddMember`, `Malformed`) after the inner call |
| `MetadataOnly` | `tests/end_to_end.rs:73` (impl 75) | Integration-suite Cargo stub — the only fake outside `src/` |

Two deserve a note. `RecordingRunner` carries a comment (`workspace.rs:6124-6131`) stating
the *premise* of its own test: a prior implementation "resolved zero workspaces, exited 0,
and reported no groups", and a recording runner catches that at the argument level without
the network. That is the right instinct — assert on the call, not on the outcome the fake
produces. `RaceRunner`'s `mutate` is a closure parameter, so each race case supplies its own
mutation rather than the fake hardcoding one scenario.

`RecordingRunner` is also now defined *inside* its test function
(`workspace.rs:6134`) rather than at module scope, because exactly one case needs it. That
is the more honest shape: a fake used once is not a reusable seam.

### Zero-test modules

`grep -cE '^\s*#\[test\]'` over `src/error.rs src/lib.rs src/main.rs` returns `0` for all
three. `src/domain.rs` no longer joins them — it carries one test
(`domain.rs:483`). They split into two very different groups.

**Defensible.** `lib.rs` (16 lines, module manifest) and `error.rs` (16 lines, six `AppError`
variants) are exercised transitively by every test in the modules that consume them.
`domain.rs` (500 lines of pure data types — no I/O, no branching on process state) now has
exactly one inline test, and it is a good one: `elapsed_nanos_saturates_instead_of_wrapping`
(`domain.rs:483`) pins the saturating conversion against the wrapping read it exists to
prevent, with the premise stated in the test's own doc comment.

**Not defensible, and significant.**

- **`src/main.rs` (2513 lines, 12 tests).** The composition root. It *is* covered —
  `cli_contract.rs` runs the binary as a subprocess, so `main()` and `run()` execute in 81 of
  the 83 cases, and the twelve inline tests pin the orchestrator's own decisions
  (the interactive surface's three endings, and the report verdict). The Cargo-failure cases check real process behavior, bounded human grouping,
  and unchanged JSON diagnostic coverage. This remains black-box coverage of an opaque
  process: nothing asserts on `run()`'s structure, and the exit-code table is only as complete
  as the cases someone remembered to write. The two pipelines are joined here and nowhere
  else, and C023 §11.4 established that the cleanup progress-clear ordering in this file is
  unreachable from the tests — reverting it leaves every test green, because the bar only
  draws on an attended terminal — and that pinning it needs a pty harness and a new
  dev-dependency that do not yet exist.
- **`update_json` (`output.rs:903-940`)** *was* the sharpest single gap, and C022 closed the
  test half of it. The only machine-readable output path that does **not** go through
  `EnvelopeV1`; it hand-builds a `serde_json::Map` with its own `json!(1)` at `output.rs:905`
  and a bespoke `result` object. It now has four contract cases (`output.rs:969`, `:995`,
  `:1009`, `:1048`) and a failure-shape integration case in `cli_contract.rs`. What is still open is the schema half:
  it shares `schema_version: 1` with the envelope while being a different document, and
  reconciling that needs a plan and a version bump.

### The module that left the list: `output.rs`

`src/output.rs` was one of the five zero-test modules until M012B added `--format log`. It now
carries **21 `#[test]` functions**: 2 in `path_tests` (`output.rs:273-300`), 15 inside
`pub mod log`'s own `#[cfg(test)] mod tests` (`output.rs:509-884`), and 4 more that C022 added
in a top-level `mod update_tests` (`output.rs:945-1065`) for the `update` JSON document, which
is the module's one non-envelope contract and previously had no test anywhere. They cover what
the log line newly made risky and previously untested: one-line bounded ASCII output
(`scan_line_is_one_bounded_ascii_line`, `output.rs:581`, against the `assert_ascii_and_bounded`
helper at `output.rs:566`), byte-limit truncation that drops an overlong optional whole rather
than mid-token (`output.rs:771`), the structural head fitting at the worst case
(`output.rs:802`), field-order determinism across calls (`output.rs:816`), and the status
triage for ok / blocked / failed (`output.rs:600`, `:630`, `:695`). One of them is worth
singling out: `cleanup_json_result_key_set_is_stable` (`output.rs:662`) pins the *key set* of
the cleanup JSON result, so adding a field to the machine result fails a test rather than
slipping through as a quiet schema drift.

That is a real gain and it is also the *easy* half. The envelope layer that predates M012B is
unchanged and still has no in-file test: `EnvelopeV1` (`output.rs:8`), `scan()` (`output.rs:105`)
and `cleanup()` (`output.rs:161`) hardcode `schema_version: 1` at `output.rs:143` and
`output.rs:212` with nothing inside `src/` pinning either the struct shape or the literal.
Those are asserted only from outside the process, so adding a field to `EnvelopeV1` still
requires someone to notice. Reading "the zero-test list shrank" as "the DTO layer is now
pinned" would be the wrong conclusion.

### `traverse.rs`

Its test module opens at line 495 and contains **6** `#[test]` functions. For the module
that walks the entire filesystem — parallel size and recency measurement, symlink and
`CACHEDIR.TAG` handling, `ReadDir` error attribution, pruning — that is low. The
parallel-probe machinery has a dedicated counting fake in `cleanup.rs`
(`ParallelProbeRunner`, `cleanup.rs:8520`); `traverse.rs` has nothing comparable.

`source_activity` is declared at `src/traverse.rs:311` and has exactly one call site in the
crate: `src/traverse.rs:619`, inside the test module opening at 495. **There is no
production caller** — production uses `workspace_source_activity` (`src/workspace.rs:2143`,
called from `src/workspace.rs:1692`) instead. So `traverse::source_activity` is a `pub`
function with no production consumer whose sole exercise is its own test. That test proves
the function works and nothing about the system's behaviour: a coverage claim that looks green
and is empty, and a milder cousin of the C009–C013 incidents.

And the test does not discriminate what its own name claims.
`source_scan_short_circuits_recent_files` (`src/traverse.rs:611`) writes one fresh file into a
freshly created temp directory and then walks *that directory* as the project root. The walk
is `ParentFirst` (`src/traverse.rs:328`), and the root's own mtime is fresh, so
`SourceActivity::Recent` is returned on the **directory** before the file is ever examined.
The case would still pass if `fresh.rs` were deleted, or if the per-file guard were removed
altogether. It is a true statement about a directory, wearing the name of a statement about a
file. See R8.

### `interactive.rs`

`src/interactive.rs` arrived with M014 and was rebuilt by C039 and C042. It is 3606 lines with
**76** `#[test]` functions (test module opens at `src/interactive.rs:1818`), which makes it the
third-largest inline test suite in the crate and gives it more tests than `discovery_state.rs`
and `policy.rs` combined (18 + 26).

The shape is worth recording because it is the one place the project chose a *pure* seam over
a process seam: `Session` (`src/interactive.rs:204`) is a state machine that owns mode, cursor,
filter, and selection and renders itself to a `Vec<String>` with no terminal in scope, while
everything that touches a real terminal lives behind the `Terminal` trait
(`src/interactive.rs:120`, whose `restore` is at `src/interactive.rs:127`). The consequence is
that the failure that actually matters — a session that exits without restoring the terminal —
is testable at all, and the test suite is where the "restore on unwind" guarantee is pinned
(`src/interactive.rs:2042` catches the unwind).

What the module does **not** change is the reachability of the interactive path from the
integration suites: `cli_contract.rs` exercises `-i` only through the usage-error and
machine-format refusals (`cli_contract.rs:1533`, `:1567`, `:1619`, `:2127`), because a real
selection session needs a terminal and the suite runs without one. The selection *behaviour* is
therefore covered in `src/interactive.rs` and the selection *surface* is covered nowhere
outside it.

---

## 4. Integration suites

### How they invoke the binary

**Definitive answer: as a subprocess.** `tests/cli_contract.rs` uses
`env!("CARGO_BIN_EXE_cargo-cleanme")` with `std::process::Command`, never a library call.
`tests/cli_contract.rs:15` resolves the staged binary path; it is used at 43 sites (lines 91,
122, 153, 177, 229, 288, 507, 588, 628, 670, 704, 741, 845, 901, 936, 980, 1019, 1059, 1142,
1248, 1274, 1301, 1328, 1379, 1902, 2471, 2516, 2552, 2715, 2758, 2786, 2829, 2854, 2910,
2936, 2976, 3032, 3064, 3099, 3422, 3447, 3477, 3967). Five cases go further, invoking a real
`cargo` as `Command::new("cargo")` — three of them (`tests/cli_contract.rs:96`, `:130`,
`:161`) compare external subcommand behaviour against direct invocation, and two
(`tests/cli_contract.rs:1422`, `:3514`) are plugin-invocation helpers that also put `cleanme`
first. `end_to_end.rs` is the exception: it drives the library's scan entry point in-process
with a `MetadataOnly` Cargo stub (`end_to_end.rs:73-75`).

Consequence: `main.rs` executes end to end in 81 of the 83 `cli_contract.rs` cases — argv
normalization (`Cli::parse_normalized`), dispatch, exit codes — but only as an opaque process.
The two cases that do not run the binary are
`update_dry_run_reaches_the_same_non_mutating_path_from_either_spelling`
(`tests/cli_contract.rs:2800`), which drives the library in-process, and
`the_json_update_branch_prints_only_the_tested_seam`
(`tests/cli_contract.rs:3784`), which reads `src/main.rs` as text. `run()`'s internals are
untested, and the `update` command has no successful integration case at all.

### `tests/common/mod.rs`

286 lines, no tests. Exports `set_path_modified` (`tests/common/mod.rs:24`) and
`backdate_file` (`tests/common/mod.rs:98`), which set file mtimes directly, plus
`inactive_project` (`tests/common/mod.rs:111`), which writes a manifest-and-source tree.
`set_path_modified` is hand-rolled per platform because the inactivity threshold is
time-based and tests must not sleep: `File::set_modified` on Unix
(`tests/common/mod.rs:84-94`, with the FILETIME-ticks epoch constant at
`tests/common/mod.rs:36`) and `SetFileTime` on Windows (`tests/common/mod.rs:70`). This is
the most load-bearing piece of portable test infrastructure in the repository — without it
"inactive" cannot be tested at all, and on Windows the naive approach was precisely the
C009 defect class.

Two unix-only helpers joined the harness since: `write_fake_cargo`
(`tests/common/mod.rs:173`) writes the `cargo` stub that the `#[cfg(unix)]` Execute cases put
on `PATH`, and `cargo_calls` (`tests/common/mod.rs:256`) counts the log entries it appends.
Both are gated `#[cfg(unix)]` with the reason recorded in the doc comment at
`tests/common/mod.rs:158`, which is exactly the discipline the fixture portability checker
exists to enforce.

### `tests/cli_contract.rs` — 83 declared cases

Twenty-two run on every platform; fifty-nine are `#[cfg(unix)]` because they need the
`write_fake_cargo` stub on `PATH`, and two more are Linux-only. A Windows lane therefore
runs 22 of the 83.

| Test | Line | Contract pinned |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 74 | The premise: the built artifact is reachable as `cargo-cleanme` on `PATH` and is what `cargo` dispatches to. Without it, the three "external == direct" comparisons prove nothing |
| `cargo_external_subcommand_help_matches_direct_help` | 114 | `cargo cleanme --version` and `--help` byte-match direct invocation |
| `cargo_external_config_edit_help_matches_direct` | 148 | Same for the subcommand form, which exercises a different clap path |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 462 | The scan envelope, plus stream separation (`cli_contract.rs:193`, `:198`) and newline discipline |
| `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` | 504 | `--known` reports the Routine scope; an explicit `--scope` reports that scope; `scan.root` does not |
| `repeated_real_cargo_failures_are_grouped_without_losing_json_diagnostics` | 547 | Real Cargo reports one bounded human cause group for repeated orphan-workspace failures while JSON retains every per-manifest diagnostic |
| `real_cargo_workspace_failure_matrix_records_stage_and_context` | 647 | Seven manifest shapes are probed with actual Cargo; locate/metadata outcomes are asserted with tool version, cwd and argv context |
| `unavailable_cargo_executable_is_reported_as_a_locate_spawn_failure` | 783 | An empty PATH exercises the real executable-spawn failure and pins partial-result behavior plus exact counters |
| `cargo_failures_with_shared_generic_line_keep_distinct_cause_groups` | 838 | Ten controlled causes sharing Cargo's generic first line produce eight human groups plus a hidden-group count, while JSON stays complete and `--format log` excludes Cargo stderr |
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 954 | Contradictory scope selectors exit 2 without walking anything — fail-closed at the boundary |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 986 | `clean --dry-run` envelope and summary |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 1025 | A blocked scope still emits parseable JSON *and* exits 1 (`cli_contract.rs:757`). That pairing is the automation contract |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 1072 | `#[cfg(unix)]`. Unattended Execute reaches a real `cargo clean` through a fake, emitting `reason_code: "cleaned"` with selector fields and byte counts |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 1467 | `config edit` really execs a process (`$EDITOR`), and an invalid edit is rejected rather than written |
| `interactive_without_a_terminal_is_a_usage_error_that_names_the_reason` | 1895 | The `-i` surface fails closed without a terminal, naming the reason rather than silently scanning |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` | 2002 | M012A: bare `cargo cleanme` resolves to `CleanupScope::Maintenance` and runs cleanup, not a scan |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` | 2039 | `--dry-run` is application simulation: the same decision path, zero `cargo clean` processes |
| `routine_bare_dry_run_and_execute_use_real_cargo_on_default_seed` | 2078 | `#[cfg(unix)]`. Isolated `$HOME/projects` is the Routine scope; real Cargo build/clean proves one clean reclaims bytes and simulation keeps the same unit without spawning clean |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` | 2263 | The empty-roots short circuit exits 0 with the human notice, having spawned nothing |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` | 2303 | Unresolved ownership blocks the whole scope and runs no clean |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` | 2520 | `clean` defaults to Execute; Cargo's own preview is `--cargo-preview`, not the default |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` | 2578 | `--dryrun`/`--yes` are hidden aliases resolving to the same modes as `--dry-run`/`--yes` |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` | 2825 | Rootless `scan` is Full and ignores `scan.root`; `scan --known` is the Routine inventory |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 2970 | An empty root is a valid zero-group report, exit 0 — not an error |
| `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` | 3076 | M012B: the shared `assert_bounded_log_line` contract (`cli_contract.rs:2592`) — ASCII, ≤ `MAX_BYTES`, exactly one newline, `cargo-cleanme op=` prefix, no control sequences — and nothing on stderr |
| `log_mode_distinguishes_simulate_from_cargo_preview` | 3110 | `mode=simulate` and `mode=preview` are distinct on the wire, matching `CleanMode::as_str` |
| `log_mode_scope_block_is_one_line_with_a_typed_reason_and_exit_one` | 3143 | Blocked scope → one line, typed `reason=`, exit 1 |
| `log_mode_failed_cleanup_is_one_line_with_status_failed_and_exit_one` | 3171 | A failed unit degrades to `status=failed` without a scope reason |
| `log_mode_zero_result_maintenance_is_status_ok` | 3213 | Zero groups is `status=ok`, not a special case |
| `log_mode_scan_reports_the_resolved_scope` | 3242 | The scan log line carries the same resolved-scope label as the JSON envelope |
| `log_mode_stats_is_opt_in_stderr_and_leaves_the_single_line_alone` | 3313 | `--stats` stays an explicit stderr override; stdout is byte-identical with and without it (`cli_contract.rs:2897-2901`) |
| `log_mode_suppresses_normal_diagnostic_fan_out` | 3367 | The per-diagnostic stderr fan-out is gated off in log mode; the count rides in `diagnostics=N` |
| `log_mode_fatal_error_is_one_bounded_stderr_line_and_no_stdout` | 3436 | A fatal error is a bounded stderr line; stdout stays empty, so a consumer reads one stream |
| `json_and_human_output_are_unchanged_by_log_mode` | 3479 | Adding the third format did not perturb the two existing byte-for-byte outputs |
| `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 3525 | `log` is an accepted `--format` value; an unknown one still exits 2 (`cli_contract.rs:3097`) |
| `scan_emits_exactly_one_json_document` | 3560 | The one-document-per-invocation invariant, asserted as a **source string**: `src/main.rs` must still contain `enum EmitReport` (`src/main.rs:1341`). This is the closest the evidence base comes to `clean --full`, and it does not run it — see below |
| `json_update_refusal_emits_no_document_and_exits_non_zero` | 3732 | A forbidden-provenance `update` emits zero stdout bytes and exits 2, and its stdout is not even parseable as a success document |
| `the_json_update_branch_prints_only_the_tested_seam` | 3783 | The only `cli_contract.rs` case that does not execute the binary: it reads `src/main.rs` and asserts the JSON branch has exactly one unterminated stdout write and prints `update_json_stream` |
| `repeated_routine_runs_bound_stale_root_noise_without_mutating_state` | 4159 | Two Routine Simulate invocations over 100 missing hints retain one bounded summary, preserve state bytes and keep the healthy sibling selected |
| `incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean` | 4416 | A Full scan that hit an unreadable root reports the real counts and spawns no clean, rather than reporting a clean inventory |

The discovery unit suite also has a Unix permission-premise fixture in
`discovery.rs::unreadable_child_keeps_accessible_inventory_and_unknown_coverage_bounded`
(`src/discovery.rs:1168`). It verifies `read_dir` actually fails for the test process before
relying on mode `000`, then checks that the readable sibling manifest remains inventoried,
one diagnostic is retained with no fabricated path, and the traversal is marked incomplete. It
intentionally fails if run as an identity that can still read the fixture, rather than
reporting a false green.

#### The `clean --full` gap, stated precisely

There is **no live `clean --full` run anywhere in the evidence base**, and there cannot be one
on a normal workstation. `clean --full` consumes the generation reconciled by the internal
Full traversal, and `policy::full_cleanup_roots` (`src/policy.rs:549`) fails closed with an
`AppError::Config` — exit 2 — whenever that reconciliation reported an incomplete scan
(`src/policy.rs:523`). A Full traversal is incomplete whenever the host has an unreadable
directory, which is the normal case, not the exceptional one. The product behaves correctly;
the *evidence* cannot reach it.

What exists instead is `scan_emits_exactly_one_json_document`
(`tests/cli_contract.rs:3115`), and its own doc comment is honest about the substitution: it
runs a bounded `scan`, proves one document and one newline, and then asserts that
`src/main.rs` still contains `enum EmitReport` (`src/main.rs:1341`) — the guard that keeps
`clean --full` from emitting two documents in a row. That is a **source-string guard**, the
weakest link in this document's evidence: it would not notice a change that broke the guard's
behaviour while leaving the identifier in place. It is the same shape as
`the_json_update_branch_prints_only_the_tested_seam` (`tests/cli_contract.rs:3310`). Both are
recorded here rather than presented as coverage.

### The machine-contract checklist

This is the real, executable shape of the versioned JSON contract. Every item is a live
assertion; change the DTO and these are the lines that fail.

| Assertion | Line |
|---|---:|
| `["schema_version"] == 1` (scan) | 195 |
| `["operation"] == "scan"` | 196 |
| `["scope"] == "explicit"` | 197 |
| `--stats` output appears on **stderr** (`"scan stats:"`), not stdout | 198 |
| `--stats` does not perturb stdout | 193 |
| stdout contains **exactly one** `\n` — one document, no trailing blank line | 199 |
| resolved scope is reported, not the CLI flags (Routine / explicit / configured root) | 245, 254 |
| `["schema_version"] == 1` (cleanup) | 724 |
| `["operation"] == "clean"` | 725 |
| `["mode"] == "simulate"` — the requested mode is echoed, not inferred | 726 |
| `["result"]["summary"]["simulated"]` (zero for `--cargo-preview`) | 727 |
| single trailing newline on cleanup stdout | 728 |
| exit code `2` on conflicting scope selectors, before traversal | 674 |
| exit code `1` on a blocked scope | 754 |
| `["result"]["scope_blocked"] == true` | 756 |
| `["result"]["scope_reason"]` is a typed reason, not prose | 758 |
| `["result"]["summary"]["cleaned"] == 1` | 872 |
| `["result"]["units"][0]["reason_code"] == "cleaned"` | 873 |
| `["result"]["selector_kind"] == "profile"` | 874 |
| `["result"]["selector_value"] == "dev"` | 875 |
| `units[0]["before_bytes"]` is **null** — a selector run cannot attribute bytes to the selector | 876-879 |
| `units[0]["selector_estimate_bytes"]` is **null** for the same reason | 880-883 |
| `units[0]["output_union_before_bytes"]` present and numeric | 887-889 |
| simulate mode reports `outcome == "simulated"` | 926 |
| policy-skipped case reports its `reason_code` (`selector_unsupported`) | 964 |
| policy-skipped case reports `policy_disposition` (`selector_estimate_unavailable`) | 968 |
| invalid selector reports `reason_code` (`selector_invalid`) | 1007 |
| unsupported selector (too-old Cargo) reports `reason_code` (`selector_unsupported`) | 1047 |
| package selector succeeds: `reason_code: "cleaned"` | 1086 |
| `selector_kind == "package"` | 1087 |
| `selector_value == "fixture@0.1.0"` — name **and** version | 1088 |
| the internal-failure path exits non-zero (`1`), and the editor's own status `17` is reported in the prose | 1153, 1157-1160 |
| `mode=preview` distinct from `mode=simulate` (Cargo's own preview) | 2672 |
| `--format log` accepted; an invalid format still exits `2` | 3076 |

`tests/end_to_end.rs:203-205` adds three: `["result"]["groups"][0]["ownership"] ==
"private"`, `["result"]["summary"]["group_count"] == 1`, and `groups[0]["bytes"]` as a
`u64`.

The three byte-count assertions are separated deliberately, and two of them are *absences*.
`before_bytes` is one unit's own size and `selector_estimate_bytes` is what the selector's
evidence justified *before* the final proof — but a selector run cannot attribute bytes to
the selector, so both are `null` and the JSON says so rather than guessing
(`cli_contract.rs:879-886`). `output_union_before_bytes` is the union across the unit's
complete output set, and that one *is* a number in both modes (`cli_contract.rs:890-892`).
Conflating any two would misstate what a cleanup reclaims — or claim a bound the proof never
reached.

### What the suites deliberately do not pin

- **No `update` success document.** `update_json`'s *failure* direction is pinned through the
  real binary (`tests/cli_contract.rs:3259`) and its shape in `src/output.rs`, but no case
  represents a successful live update: every success path resolves the version authority from
  crates.io and then replaces bytes.
- **No negative CLI parsing coverage beyond the exit code.** `cli.rs` now has 32 inline tests,
  but the suites pin only the code, never the message: `2` for usage conflicts
  (`cli_contract.rs:677`, `:3076`), `1` for a blocked scope (`cli_contract.rs:757`). Nothing
  asserts *which* usage error was reported. There is no exit code `17` any more — the
  process now exits `1` and reports the editor's status `17` in the prose
  (`cli_contract.rs:1156`, `:1157-1160`).
- **No assertion on human-readable output.** The default `--format text` path through
  `report::render` is covered only by `report.rs`'s 7 inline tests.
- **No destructive Execute case on Windows.** Every case that actually drives a fake `cargo`
  is `#[cfg(unix)]`, so Windows exercises the argv, dispatch, and envelope layers but never
  the spawn-and-delete layer.
- **No timing, progress, or TTY behaviour.** `--no-progress` is passed
  (`cli_contract.rs:178`) to make output deterministic, so `IndicatifRenderer` and the
  terminal-capability detection in `progress.rs` are integration-untested by construction. The
  log-mode cases assert the *absence* of control sequences on stdout
  (`cli_contract.rs:2610-2613`), which is the closest the suites come.

---

## 5. The contract checkers

17 scripts. The dividing line is the one this project cares about: **outcome checkers** fail
when the subject misbehaves; **premise checkers** fail when the *evidence* is no longer
valid. Premise checkers are the distinctive category.

| Script | Class | Enforces | Wired into |
|---|---|---|---|
| `check-release-contract.py` | premise | Compares product surfaces against Eggpack producer authority: every contracted target reachable through the wrappers, the generated workflow, the packaging payload and the README support matrix; wrapper target mapping is a projection of the contract rather than a second schema; the packaging payload ships the wrappers the release needs; the package metadata the contract implies is in `Cargo.toml`; every required target builds under one **exact** Rust release, not a channel; the candidate workflow may stage a draft but never publish. Retains the cargo-cleanme-owned invariants: the `include` allowlist is explicit and contains `/config.toml`, the repository URL matches the release origin, the `xtask` generator stays out of the published crate, license files exist when SPDX claims them | `ci.yml` `generated-docs` `:110`, `:115`; `release-drift.yml` `:49`, `:55`; `release-check.sh` `:71`, `:72` |
| `check-fixture-portability.py` | premise | Every external-process fixture in `src/**/*.rs` uses `#[cfg(unix)]`/`#[cfg(windows)]` gating, avoids unconditional `/bin/sh`, `sh -c`, `.sh` scripts, hardcoded `/tmp`, `:`-separated `PATH`, and a fixed fake binary name that collides across gates | `ci.yml` `checks` `:38`, `:40`; `release-check.sh` `:129`, `:130` |
| `check-doc-citations.py` | premise | `file.rs:N` citations in `architecture/` resolve to an existing file and an in-range, correctly ordered line; **and** the [§5](#5-the-contract-checkers) inventory is exactly the set of scripts in `scripts/`, in both directions, without duplicates | `ci.yml` `checks` `:46`, `:48`; `release-check.sh` `:133`, `:134` |
| `check-installer-contract.py` | premise | The `packaging/install.sh` and `packaging/install.ps1` wrappers are a projection of the Eggpack contract: every contracted triple is reachable from a recognized host, no wrapper names a triple the contract does not publish, the Windows wrapper's target set is exactly the contract's Windows set, the asset name is derived from product-then-target, both install names derive from the contract, the Cargo fallback families are exactly the contract-omitted ones and are documented, both wrappers are present with the POSIX one executable (recorded git mode, or the filesystem when there is no record), and neither escalates privilege or bypasses TLS | `ci.yml` `generated-docs` `:137`, `:139`; `release-drift.yml` `:61`, `:64`; `release-check.sh` `:86`, `:87` |
| `check-release-identity.py` | premise | Tag/source identity: the release tag matches the source commit, version and tag agree, and the provenance identity machine is internally consistent | `release-drift.yml` `:58`; `validate-staged-release.yml` `:194`; `release-check.sh` `:76`, `:79` |
| `check-post-release-smoke-contract.py` | premise | The post-release smoke workflow is itself well-formed: required steps, lanes, and assertions present and in order | `post-release-smoke.yml` `:167`; `release-drift.yml` `:68`, `:69`; `release-check.sh` `:90`, `:91` |
| `check-staged-validation-contract.py` | premise | The hosted validation workflow is upstream-bound and cannot publish: exact upstream workflow name, `completed` only, no push/pull/schedule, an **explicitly declared** `contents` scope and no other capability (`actions`/`id-token`/`packages`/`deployments`), **no publication command in any form** — `gh release publish/create/edit/delete/upload`, `gh api` against the releases endpoint with a mutating verb in either order, `curl ... PATCH`, `cargo publish`, `git push --tags` — checkout bound to the upstream `head_sha`, upstream event *and* conclusion compared (not merely mentioned), identity and validator both invoked, no inlined reimplementation | `ci.yml` `generated-docs` `:133`, `:135`; `release-check.sh` `:103`, `:104` |
| `check-selector-qualification.py` | premise | The Cargo selector support claim is a projection of one policy: runtime allowlist == `release/selector-qualification.json` == hosted matrix == the script's own inputs, all exact `X.Y.Z`, package ⊆ profile, exploratory disjoint, workflow read-only and promotion-proof | `ci.yml` `generated-docs` `:122`, `:124`; `qualify-cargo-selectors.yml` `:65`, `:66`; `release-check.sh` `:110`, `:111` |
| `verify-release-attestation.py` | premise | A published release is immutable, carries a valid GitHub attestation for *that* repository and tag, and every contracted local asset hashes to the attested digest | **`release-check.sh` `:119`, `:120` only** — no workflow. Event-specific: needs a published, immutable release and a live `gh` |
| `resolve-smoke-transition.py` | premise | An exact `from`→`to` transition is resolved numerically, a predecessor is never silently skipped, and crates.io is polled within a finite deadline with backoff | `post-release-smoke.yml` `:95`, `:100`, `:109`; `ci.yml` `generated-docs` `:144`; `release-check.sh` `:97` |
| `gen-release-workflow-shape.py` | generator | Generates the *expected* shape of `release-binaries.yml` from the release contract, so the workflow is a checked-in generated artifact rather than hand-edited prose | `release-drift.yml` `:38`; `release-check.sh` `:61` |
| `validate-staged-release.py` | outcome | Six-step staged-release proof: exact artifact inventory, sidecar integrity (checksums), manifest agreement, contract agreement, static ABI, real-installer qualification — i.e. published bytes are the qualified bytes | **`validate-staged-release.yml` `:204` only** (M011B). Not in `release-check.sh` and not in any other workflow |
| `release-benchmark.py` | outcome | Compares current scan performance against `release/baseline-benchmark.json`; semantic counters are a hard gate and wall-clock timings are recorded but never fail | `ci.yml` `benchmark` `:156`; `release-check.sh` `:55` |
| `smoke-release-candidate.py` | outcome | Release-candidate smoke: the built binaries respond correctly before publication | `ci.yml` `checks` `:33`; `release-check.sh` `:137`; and indirectly on every release target, because `release/eggpack/consumer-validators.json` names it as the per-target validator invoked by the `qualify_build_*` jobs |
| `post-release-smoke.sh` | qualification | Post-publication verification against the *actual published bytes* — the external `update` check that first exposed C011 | **`post-release-smoke.yml` `:174` only.** Event-specific by construction: it can only run once a version exists on crates.io |
| `qualify-cargo-selectors.sh` | qualification | Real-Cargo selector characterization: what `--profile`/`--package` actually do on genuine Cargo, and where the support boundary is (M011D) | `qualify-cargo-selectors.yml` `:112`, `:146`; `ci.yml` `generated-docs` `:127`; `release-check.sh` `:112` |
| `release-check.sh` | composite gate | The local pre-push gate: full crate test suite, `cargo test --doc` (which CI does not run), all the Python checkers, the generated-shape comparison, `fmt`/`clippy` hygiene | **No workflow, by design.** Run by an operator; it refuses a dirty tree and requires `eggpack` and toolchain 1.89 |

Five notes:

1. **`/config.toml` in the allowlist is load-bearing.** `src/config.rs` embeds the template with `include_str!` at compile time, so a published crate without `/config.toml` in `include` builds fine and fails at runtime the first time it needs to create a config. `check-release-contract.py` asserts the entry specifically because no test and no runtime check can catch it — a pure packaging invariant, and a clear case of category (b).

2. **`check-fixture-portability.py` is a premise checker, and its own docstring (script lines 2-57) states the limit of its guarantee.** It scans `src/**/*.rs` (globs at script lines 70-75) and rejects statically-detectable POSIX-only fixture shape. It explicitly does **not** check whether a case's pass condition proves the intended branch — "reviewing that is the point of the C013 inventory" (script lines 46-47). A static checker can catch a missing `#[cfg(unix)]`. It cannot catch a test that would have passed anyway. Read its green as "no visibly platform-shaped fixture is ungated", never as "every fixture proves something on every lane".

3. **The Phase 11 checkers all guard against the same shape, and it is a shape the
   repository has already met seven times: *a guard that cannot fail*.**
   `check-staged-validation-contract.py`, `check-selector-qualification.py`,
   `verify-release-attestation.py`, and `resolve-smoke-transition.py` each ship a
   `--self-test` that mutates the guarded artifact and requires a rejection, and **two** of
   them also assert a **control** — that the mutation actually changed something
   (`check-staged-validation-contract.py` script lines 453-618, `check-selector-qualification.py`
   script lines 462-542). `verify-release-attestation.py` and `resolve-smoke-transition.py` do
   not assert that control: they require a rejection and, for the attestation, a specific
   failure code, but neither proves its mutation was not a no-op. That control is not
   decoration: during implementation, a self-test case reported "rejected" for a tree that
   contained no defect at all, because its mutation had silently become a no-op after an
   unrelated reformat. A guard with a no-op mutation is the worst outcome, because it converts
   a broken tree into a passing check. `check-doc-citations.py`'s inventory self-test follows
   the same rule for the same reason, and so does `check-installer-contract.py`, whose
   `expect` helper asserts all three properties — the mutation changed the fixture, the
   checker rejected it, and it rejected it *for the intended diagnostic* (script lines
   365-388).

4. **`release-check.sh` closes a CI gap of its own making.** `ci.yml` runs `cargo test --all-targets --all-features`, which does not execute doctests; the local gate runs `cargo test --doc` separately. Doc examples are verified by a human with a shell script, not by CI.

5. **`check-installer-contract.py` now proves its own detection (C022).** It was the one
   contract checker in this table with no `--self-test` and no argument parsing at all, so its
   green was a statement about the tree and never about itself. It now takes a fixture root and a
   recorded exec bit as parameters (`check` at `scripts/check-installer-contract.py:125`), and its
   `--self-test` (`:354`) builds a self-contained fixture tree — a deliberately tiny contract that
   is **not** copied from the live one, so a legitimate contract change cannot silently rewrite
   the thing that is supposed to be testing it. Each case asserts three things, not one: the
   mutation **changed** the fixture, the checker **rejected** it, and it rejected it *for the
   intended diagnostic*. That third assertion is not decoration — during C022's own
   implementation, deleting the missing-mapping rejection left the checker still rejecting the
   tree, for an unrelated reason, and only the attribution assertion caught it. The exec-bit
   cases exercise both branches without depending on the host: the recorded-mode branch is
   exercised by passing a mode directly, because a mutation the host filesystem has to cooperate
   with cannot prove the Windows-checkout path.
   Six defect classes are covered by mutations — a missing contracted mapping, an extra mapping,
   PowerShell target-set drift, a reversed asset-name expansion, an undocumented or unexpected
   Cargo-only family, a missing wrapper of either kind, and a non-executable POSIX installer —
   plus a coherent positive control and a recorded-mode case.

---

## 6. CI as the enforcement point

`.github/workflows/ci.yml` defines five jobs, and they divide along lines worth
naming: **`checks` and `installers` run the product**, while `generated-docs`,
`benchmark` and `msrv` run the properties of the repository as an artifact. Six
of the ten Python guards CI invokes live in `generated-docs`, not `checks` — the
separation is easy to misremember, and an earlier version of this document did.

**`checks`** (`.github/workflows/ci.yml:4`) — the bulk of the gate. OS matrix of
`ubuntu-latest`, `macos-latest`, `windows-latest`, so the unit tests that compile on this
platform and the 63 integration cases (19 of them on a Windows lane) execute across all
three, and on stable. Runs `cargo fmt --check`, `cargo clippy --all-targets --all-features --
-D warnings` (a real lint gate — warnings are errors), `cargo test --all-targets
--all-features`, then **four** Python guards: `smoke-release-candidate.py`
(`.github/workflows/ci.yml:33`), and two premise guards that each run their own
`--self-test` first — `check-fixture-portability.py` (`--self-test` at
`.github/workflows/ci.yml:38`, the checker at `:40`) and `check-doc-citations.py`
(`--self-test` at `.github/workflows/ci.yml:46`, the checker at `:48`). A red build catches:
any behavioural regression on any of the three platforms; any new warning; formatting drift; a
new ungated POSIX-shaped fixture; a deep-dive citation whose line no longer resolves; **and,
since C021, a verification script that exists but is missing from the inventory that
documents it**. `--all-features` is what makes the matrix meaningful for the update path.

**`installers`** (`.github/workflows/ci.yml:49`) — runs the installer fixture suite
`packaging/tests/test_installers.py` across the same three OSes
(`.github/workflows/ci.yml:68`), with its own `--self-test` at `.github/workflows/ci.yml:95`
on the Windows lane specifically, because that is where the C012 defect lived. The static
wrapper check, `check-installer-contract.py`, is **not** here despite its name: it lives in
`generated-docs` (`.github/workflows/ci.yml:137`, self-test `:139`) and in
`release-drift.yml:64` (self-test `:61`). Catches a wrapper that stopped being executable, lost
its shebang, or diverged on one platform.

**`generated-docs`** (`.github/workflows/ci.yml:96`) — regenerates completions and manpages via
`xtask` and fails if the committed files differ, then runs the other **six** Python guards:
`check-release-contract.py` (`.github/workflows/ci.yml:110`, `--self-test` `:115`),
`check-selector-qualification.py` (`:122`, `--self-test` `:124`),
`qualify-cargo-selectors.sh --self-test` (`:127`),
`check-staged-validation-contract.py` (`:133`, `--self-test` `:135`),
`check-installer-contract.py` (`:139`, `--self-test` `:137`), and
`resolve-smoke-transition.py --self-test` (`:144`). Six of the eight per-commit `--self-test`
invocations in CI live here. A *freshness* contract plus the repository's structural
contracts; it is also the one place a script's output is compared to a committed artifact —
the same pattern as `gen-release-workflow-shape.py`.

**`benchmark`** (`.github/workflows/ci.yml:145`) — runs `release-benchmark.py` against
`release/baseline-benchmark.json` (`.github/workflows/ci.yml:156`). Semantic counters are a
hard gate; wall-clock timings are recorded and printed but never fail, because shared-runner
timing is noise. Catches a scan that has become materially slower in a way no correctness test
notices.

**`msrv`** (`.github/workflows/ci.yml:157`) — a dedicated job pinned to Rust **1.89** via
`dtolnay/rust-toolchain@master`, matching `rust-version = "1.89"` in `Cargo.toml`, against a
crate on **edition 2024**. It runs `cargo +1.89 check --locked --all-targets` and
`cargo +1.89 test --locked --all-targets`. A red build means the code has started using a
feature stabilized after 1.89. Because edition 2024 requires a comparatively recent toolchain,
the two settings are coupled and cannot drift apart silently.

Across `ci.yml`, **eight** guards run a `--self-test` on every push — `check-fixture-portability.py`,
`check-doc-citations.py`, `check-release-contract.py`, `check-selector-qualification.py`,
`qualify-cargo-selectors.sh`, `check-staged-validation-contract.py`,
`check-installer-contract.py`, `resolve-smoke-transition.py` — plus
`packaging/tests/test_installers.py --self-test` at `.github/workflows/ci.yml:95`. The
remaining three are `check-post-release-smoke-contract.py`, `check-release-identity.py`, and
`verify-release-attestation.py`, whose self-tests run from `release-drift.yml`,
`validate-staged-release.yml`, and the operator gate respectively.

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
| **`validate-staged-release.py`** | yes — `validate-staged-release.yml:204`, under M011B | **no** |
| **`post-release-smoke.sh`** | yes — `post-release-smoke.yml:174` | **no** |
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
from `plans/registry.md:270-284`, cross-checked against the code.

**C009 — a POSIX-only fixture passing on no platform.** A Windows fixture used POSIX-only
behaviour, so it did not work on Windows *and* was not runnable on Unix. The hosted Windows
lane — the evidence every Phase 10 milestone depended on — was broken, and the fixture was
green nowhere. Closure restored the green hosted Windows lane. This motivated
`check-fixture-portability.py`.

**C011 — a feature that could not reach its authority, green anyway.** `cargo cleanme update`
returned HTTP 403 from crates.io on **every host**, because the production transport
(`eggup-curl`) cannot set a `User-Agent`, which crates.io requires. The feature was green in
CI because the *fixture registry always answered*. A stub standing in for an authority the
subject cannot reach is indistinguishable from a stub that succeeds. Corrective C011 reversed
the transport to `eggup-eggfetch` and added a no-downgrade guard; the transport choice is
explained in `Cargo.toml:76-91` and documented in [Self-update](12-self-update.md). The
original milestone's closure record was deliberately **not** edited to imply success — it
carries an addendum naming the unsatisfied requirement.

**C012 — a Windows case that had never executed.** Green only because an unrelated real
`cargo install` failed first: the pass condition was an unrelated failure, so the case
reported coverage it did not have. Fixed with a `cargo.cmd` shim, `os.pathsep`, and a
**premise check**. Unusually, it was disclosed by the product *starting to work correctly* —
once the real `cargo install` succeeded, the fake was no longer reached, and the case went red
for the right reason.

**C013 — the sweep, and its most serious finding.** A repository-wide sweep of external-process
fixtures produced a 25-row inventory and corrected **nine** evidence defects, each new premise
guard proven to fail in the failing direction. The most serious finding was **not a test**: the
release-candidate smoke validator was green on all three hosted lanes *while the subject under
test was failing*. Tightening it exposed a real product defect, registered as C015. C013 closed
without repairing any production defect, by its own failure semantics — which is what a
verification milestone is for.

**C014 / C016 / C017 — two shipped product defects.** C016: the self-update identity check ran
the downloaded candidate **with no arguments**, so its stdout could never equal a version
string — and before failing, it performed a filesystem scan of the whole machine. The
transaction aborted safely every time, so nothing shipped is corrupt, but the advertised feature
had never once worked in any published version. C017: a `cargo install --root` binary was
misread as self-managed and **replaced**, silently, leaving `cargo install --list` reporting a
version the file no longer had; it also parsed a `.crates.toml` schema no current Cargo writes
— the only defect in the line that mutated on-disk state owned by another tool. Both were found
by C014's live rehearsal against a published release, not by any test. The corrective line
needed **seven published versions**, and three product defects (C015, C016, C017) were found by
the line's own evidence work. The registry records the shipped consequences at
`plans/registry.md:219-225`.

**C021 — a staging-cleanup test whose observation surface was wider than its subject.** Recorded
here as a near-miss rather than a shipped defect: no product behaviour was wrong, and the test
was green or flaky rather than confidently false. `staging_is_cleaned_up_on_success_and_on_failure`
called a helper that enumerated the **process-wide** temp directory and reported every path
whose basename began `cargo-cleanme-update-test-` — the same prefix every fixture in the module
mints through `unique("cargo-cleanme-update-test")`. Because Rust runs tests in parallel, a
sibling test's live staging directory could satisfy that scan and fail the assertion for a
transaction that had removed its own directory correctly.

The two failures of premise are worth naming, because they are the same shape as the incidents
above. First, the assertion's subject was ambiguous: *whose* staging directory? Second, and
more reusable, **the test could not fail for its own reason and could not pass for its own
reason either** — it was not a weak check, it was a check about the wrong thing. C021 proved the
mechanism deterministically before changing anything (a foreign same-prefix directory makes the
old scan fail on a subject that leaked nothing), then rebuilt the evidence on the paths each
fixture itself created (`update.rs:1415`), and added two controls: a fixture that deliberately
leaves *its own* staging directory behind, which the new check must reject, and a live foreign
same-prefix directory, which the new check must ignore (`update.rs:2686`). Neither control is
optional — without the first the check could not fail, and without the second it could not be
distinguished from the old one.

### The generalised lesson

The project's own formulation (`plans/registry.md:272-282`):

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
| Pass condition is an unrelated failure | C012's premise check; `packaging/tests/test_installers.py --self-test` (`self_test()` at script line 1585), which asserts each guard *rejects* a broken setup — "self test passed: every premise guard rejects a broken setup" (script line 1708) |
| Published crate missing a runtime file | `check-release-contract.py`, including the `include_str!`-embedded `/config.toml` |
| Tag/source identity drift | `check-release-identity.py` in `release-drift.yml` and `validate-staged-release.yml` |
| Smoke workflow edited into meaninglessness | `check-post-release-smoke-contract.py` |
| Docs/completions drift | `generated-docs` job |
| Documentation citing lines that no longer exist | `check-doc-citations.py`, in `ci.yml` `checks` |
| **Documentation inventory that silently lost a script** | `check-doc-citations.py` §5 parity rule — added by C021 |
| **A test whose observation surface is wider than its subject** | C021's fixture-owned staging evidence (`leaked_staging`, `src/update.rs:1415`) plus the foreign-path concurrency control at `src/update.rs:2686` |
| Performance regression | `benchmark` job vs `release/baseline-benchmark.json` |

**Residual risks in the present tree.**

**R1 — Event-specific verification has wiring but no receipts.** Three surfaces run only when a
real release event occurs: `validate-staged-release.py` (a staged draft),
`post-release-smoke.sh` (a published version), and `verify-release-attestation.py` (an immutable
release plus a live `gh`). All three are now genuinely wired — the first two to workflows, the
third to the operator gate — so the earlier claim that they were invoked by nothing is false and
has been removed. What remains true is narrower and worth stating precisely: **no artifact in
the repository records that any of them ran for a particular tag.** Nothing distinguishes "was run
and passed" from "was never run", so a regression in which bytes get published would be caught
only by the validator, and per the C013 lesson the validator is exactly the thing most likely to
be edited down at the moment it starts failing. This is why C018, M011A, M011B, and M011C all
still carry outstanding publication evidence rather than claiming closure.

**R2 — Real-Cargo selector behaviour is characterised by an event-specific harness.**
`scripts/qualify-cargo-selectors.sh` is the only place genuine Cargo's `--profile`/`--package`
semantics and the support boundary are measured, it is POSIX-only, and the full real-Cargo run
is a weekly/dispatch lane rather than a per-commit one — CI runs its `--self-test` on every
push. The automated coverage of selector behaviour (`cli_contract.rs:1089-1091`,
`:1007-1047`) runs against a *stub* whose version string the guard itself reads. If Cargo
changed what `--profile dev` selects, or moved the version floor, nothing in the per-commit
gates would notice; the hosted matrix would, but not immediately.

**R3 — The `update` machine-output surface is pinned, but only at the seam and in the failing
direction.** `update_json` (`output.rs:903`) is the one machine-readable surface bypassing
`EnvelopeV1`. C022 moved it out of `main.rs` into `output.rs` and gave it four cases
(`output.rs:969`) pinning `schema_version`, `operation`, dry-run/`changed` semantics, every
identity field, and all four provenance codes, plus a stream case asserting one document and
one newline byte-for-byte. Two things remain unproven and are recorded rather than papered
over: a **successful** run cannot be represented hermetically — every success path resolves the
version authority from crates.io and then replaces bytes, so proving it needs either published
release state or a production-only network hook — and `cli_contract.rs` therefore pins the
*failure* direction through the real binary (`cli_contract.rs:3240`: a forbidden-provenance
refusal emits zero stdout bytes and a non-zero exit) and pins the wiring separately, by
asserting that the JSON branch of `run_update` has exactly one unterminated stdout write and
that it prints `output::update_json_stream` (`cli_contract.rs:3291`). That last assertion is a
source-string guard, and it is the weakest link in this surface's evidence: it would not notice
a change that broke the stream inside the library. C016 and C017 both lived on this path.

**R4 — `traverse::source_activity` is test-only, and `traverse.rs` is thin.** `source_activity`
(`src/traverse.rs:311`) has one call site, in its own test module (`src/traverse.rs:619`). A
`pub` function with no production consumer whose only exercise is its own assertion is a
coverage entry reporting capability the system does not use — a mild instance of a formally
registered class. `traverse.rs` has 6 tests for the module that walks the whole filesystem.

**R5 — Closed by C022.** This risk read: *`check-installer-contract.py` is the only contract
checker without a `--self-test`, so its green is an assertion about the tree the repository never
demonstrates in the failing direction.* The checker now takes a fixture root and a recorded exec
bit (`check-installer-contract.py:125`), `--self-test` at `:354` mutates isolated fixtures across
the defect classes, and every gate that trusted the checker — `ci.yml:137`,
`release-drift.yml:61`, `release-check.sh:86` — runs the failing direction **before** the check
it was previously the only evidence for. What remains is narrower and is not recorded as a risk:
the asset-name guard is a shape guard (it proves the name is built from product-then-target, not
that the full template equals the contract's expansion), so a wrapper that appended a suffix
after a correct construction would not be caught by that assertion alone.

**R6 — A test premise was process-global rather than invocation-scoped.**
`staging_is_cleaned_up_on_success_and_on_failure` asserted that *no* `cargo-cleanme-update-test-`
path existed anywhere in the temp directory, while every fixture in the module mints staging
names from that same prefix and Rust runs tests in parallel. A concurrent test's live staging
directory could therefore fail the assertion for a transaction that leaked nothing — the test
could not pass for its own reason, and could not fail for its own reason either. C021 proved the
mechanism deterministically and rebuilt the evidence on paths the fixture itself created. It is
written up in [§7](#7-the-false-green-problem) history below because the class is not specific
to staging directories: **a test whose observation surface is wider than the subject it claims to
test can report another actor's behaviour as its own.**

**R7 — A test that failed on the current host, and what it took to fix it.**
*(Corrective: C037 — `plans/implementation/artifact-discovery-cleanup/c037-host-cargo-dependent-selector-premise-corrective.md`.)*
`cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
(`src/cleanup.rs:7014`) drives the **real** Cargo through `SystemCleanupRunner`,
and production code inside that path shells out to the real one to learn the
host's version: `runtime_clean_capabilities` (`src/cleanup.rs:3944`) reads
`cargo --version` and feeds it to `clean_capabilities_from_version`
(`src/workspace.rs:146`), which enables `profile_selector` for exactly nine
exact versions (1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0,
1.98.1, 1.99.0), with `1.99.1` explicitly excluded and asserted as excluded at
`src/cleanup.rs:4542`. The case's profile-selector step asserted
`CleanOutcome::Previewed` (`src/cleanup.rs:7131` pre-fix), so on a host running
Cargo **1.99.1** the capability is false, the engine takes the fail-closed
branch (`src/cleanup.rs:1821`) and reports `CleanOutcome::Skipped` with
`CleanupReasonCode::SelectorUnsupported` — **no Cargo clean is invoked**, which
is the safe direction. The *test* was what failed.

**The defect was the unstated premise, not the assertion.** The case never said
"this requires a profile-qualified Cargo"; it *discovered* that at run time, so
it could not fail for its own reason and it could not pass for its own reason.
Because the allowlist matches exact `X.Y.Z` strings, the **first patch release
of any qualified minor** is outside it — the case had a known expiry date and
no alarm before it. The hosted break is exactly that: the `checks` lane's
`dtolnay/rust-toolchain@stable` moved from **1.99.0** to **1.99.1** (run log of
`39307925648`: `stable-x86_64-unknown-linux-gnu updated - rustc 1.99.1
(58cf8f9c3 2026-10-13) (from rustc 1.99.0 (b940084d7 2026-09-28))`), and every
lane went red from run `38912914854` (2026-10-16) onwards. It was also worse
than one red: `cargo test` stops scheduling the remaining targets behind a
failing `--lib` target, so the whole 62-case `cli_contract` and 4-case
`end_to_end` integration surface was never executed on CI.

**The repair states the premise and keeps the assertion tight in both
directions.** The step now calls
`assert_profile_preview_matches_host_premise` (`src/cleanup.rs:9897`), which
derives the expected outcome from `runtime_clean_capabilities()` — the
product's own premise function — and asserts a *specific* outcome per premise:
a qualified host must report `Previewed` and the output-union line, an
unqualified one must report `Skipped` with `SelectorUnsupported`. It is
deliberately **not** a disjunction: accepting either outcome would stay green
on a host where the selector was wrongly refused *and* on a host where it was
wrongly offered. The premise is derived from the runtime table rather than the
policy file, and the chain is still sound because
`workspace::tests::runtime_allowlist_agrees_with_the_selector_qualification_policy`
(`src/workspace.rs:2213`) independently asserts that the table agrees with
`release/selector-qualification.json` — exhaustively, over every
`major.minor.patch` in `0..3 × 0..110 × 0..3`. A table that drifts from the
policy fails there; a gate that stops consulting the table fails in the
amended case.

**The repair is proven, not asserted.** Three mutations, each reverted:

| Mutation | Result |
|---|---|
| The gate no longer consults the capability (`&& !selector_capabilities.profile_selector` → `&& false`) | the case **fails**: `left: Previewed, right: Skipped` |
| The gate always refuses (`&& true`) with the table made to qualify the host | the case **fails**: `left: Skipped, right: Previewed` |
| `--profile` is dropped from the built command line | the new case **fails**: `--profile is missing from [...]` |

The supported branch is no longer only reachable on a qualified host: the new
`a_selector_reaches_the_cargo_command_line` (`src/cleanup.rs:9969`) pins that a
`--profile` selector reaches `cargo clean` as `--profile <name>` and a
`--package` selector as `--package <spec>`, with neither implying the other.
That composition is host-independent, which is what the capability gate
otherwise hides.

**No production behaviour changed.** `src/workspace.rs` and
`release/selector-qualification.json` are byte-identical to the baseline. The
repair is line-count neutral above the end of the test module, so the cited
lines in this file did not move.

**R8 — Two tests whose names promise more than their assertions deliver.** This is the C012
shape in its quietest form: a green case that cannot fail for the reason its name states.

- `traverse::tests::source_scan_short_circuits_recent_files` (`src/traverse.rs:611`) writes one
  fresh file into a freshly created temp directory and then walks **that directory** as the
  project root. The walk is parent-first, and the directory's own mtime is fresh, so
  `SourceActivity::Recent` is returned on the directory before the file is examined. The case
  would still pass if the file were deleted, or if the per-file guard were removed. It is a true
  statement about a directory wearing the name of a statement about a file.
- `discovery_state::tests::zero_retention_disables_expiration_and_nested_roots_collapse`
  (`src/discovery_state.rs:993`) no longer demonstrates either half of its name. Its only
  assertion is `assert!(state.learned_roots.is_empty())` (`src/discovery_state.rs:1029`), which
  holds because the fixture lives under the host's temporary directory and temporary roots are
  deliberately excluded from automatic maintenance learning — not because retention was zero and
  not because nested roots collapsed. The test's own comment says so.

**R9 — Two rules with no discriminating coverage.** A rule can be correct, documented, and
exercised, and still be untested in the sense that matters: if no case's outcome *depends* on
it, deleting it changes nothing.

- The C028 narrow-absence rule (`src/discovery_state.rs:362-371`) prunes a learned root that a
  complete, certain Full pass positively confirmed nonexistent. The only test that reaches this
  code, `complete_certain_full_prunes_only_positively_absent_learned_roots`
  (`src/discovery_state.rs:1036`), uses a tempdir path — and that path is already pruned by the
  earlier `automatic_root_is_maintainable` guard at `src/discovery_state.rs:352`, which runs
  first. The absence probe is therefore never the deciding branch in the case that claims to
  cover it: delete it, and every assertion in that case is still satisfied by the earlier
  guard. The "uncertainty vetoes absence" half of the same case
  (`src/discovery_state.rs:1093-1097`) asserts that the root is *retained*, which is also what
  the rule's absence would produce.
- `discovery_state::sweep_own_stale_temps` (`src/discovery_state.rs:560`) — the routine that
  removes this process's own stale `.{name}.{pid}.{attempt}.tmp` litter older than an hour —
  has **no test at all**. Its only references are the call site
  (`src/discovery_state.rs:537`) and its own definition.

**R10 — The citation gate covers only the `file.rs:N` form.** `check-doc-citations.py` resolves
a citation only when the file name and the line number are written together. The bare `:N`
convention, which is the dominant style inside the deep dives, is invisible to it —
`architecture/07-workspace.md` alone carries 448 bare `:N` references, and `09-cleanup.md`
carries 689. The mechanically checked half of "architecture cites source by line" is therefore
much smaller than it looks, and the semantic half — whether a cited line still says what the
sentence claims — is a reviewer's job in *every* document, checked or not. The §5 inventory
parity rule is unaffected: it does not depend on line citations at all.

---

## 8. Release verification

Three workflows verify properties of *published* artifacts — things no amount of in-tree
testing can see, because the subject is bytes on a registry and a CDN.

**`post-release-smoke.yml`** runs after publication against the **real published release**, not
the staging tree. It is guarded by `check-post-release-smoke-contract.py`, which validates the
smoke workflow's own shape — required steps, lanes, assertions — so the verifier cannot be
quietly reduced to a shape that always passes. This is where C011 was found: `update` returned
HTTP 403 in production while the suite was green. It drives `scripts/post-release-smoke.sh`
(`.github/workflows/post-release-smoke.yml:174`). The lesson it encodes: the only trustworthy
test of a distribution mechanism is one that uses the distribution mechanism.

**`release-binaries.yml`** builds the artifacts and, before publication, runs
`check-release-identity.py` (tag/source identity), compares `release-binaries.yml` against the
shape generated by `gen-release-workflow-shape.py` — so the workflow is a checked-in generated
artifact, not hand-edited prose — and runs `smoke-release-candidate.py` against the candidates.
Identity matters because a specific failure mode is invisible afterwards: if tag and source
commit disagree, the release is irreproducible in a way no later check can detect. C014 added a
machine-checked tag/source identity gate for this reason. `smoke-release-candidate.py` is the
script C013 found green while its subject was failing; it is now a checked workflow step.

**`release-drift.yml`** verifies the release did not drift from the tag: that the commit a tag
points at, the version in `Cargo.toml`, and the built artifacts still agree, and that
regeneration is stable. It is the post-hoc detector for the class where someone amends or
re-points a release after the fact.

**Relationship to `validate-staged-release.py` and Eggpack.** The Eggpack producer contract
(`release/eggpack`, `packaging/`) supplies the generators and manifests these workflows consume.
`validate-staged-release.py` is the offline six-step proof over a staged tree, invoked by
`validate-staged-release.yml` under M011B, whose own premises are gated by
`check-staged-validation-contract.py`. Its six steps map onto the questions each workflow asks,
and the hosted workflow is what asks them automatically. It is still the one script
`release-check.sh` does not chain, which is correct rather than an oversight — see R1 in
[§7](#7-the-false-green-problem) for the narrower claim that survives.

**The benchmark.** `release/baseline-benchmark.json` records measured scan performance for a
reference tree; `release-benchmark.py` re-measures and fails on a configurable regression
percentage, run by the `benchmark` job. The regression it catches is the class no correctness
test notices: a change that keeps every answer right but makes the scan substantially slower.
With `cleanup.rs` at 12,174 lines and the parallel probe fan-out a live optimisation surface
(`ParallelProbeRunner` exists precisely to assert the fan-out), that is a realistic and
otherwise invisible failure mode. The trade-off is inherent: a wall-clock threshold is flaky on
shared runners, so this gate is the one most likely to be loosened — itself a thing to watch in
review.

---

## 9. Coverage assessment

| Module | Lines | Tests | Tests / prod. lines | Verdict |
|---|---:|---:|---:|---|
| `cleanup.rs` | 12174 | 138 | 138 / 4536 | Appropriate. Most safety-critical module has the most tests; the race fakes show the hard cases were found and pinned — and it is also the module that carried the long-standing red, R7, now corrected by C037 |
| `workspace.rs` | 6930 | 87 | 87 / 2418 | Appropriate, and the fakes are argument-focused rather than result-focused — the right instinct |
| `update.rs` | 3342 | 54 | 54 / 1499 | Reasonable unit coverage, but this is the module whose defects (C011, C016, C017) escaped to production. Unit tests cannot see a transport that cannot reach its authority. Its transaction-hygiene evidence was briefly a false green too — see R6 |
| `interactive.rs` | 3606 | 76 | 76 / 1817 | Good for a new module, and the only one that put the terminal behind a pure seam. Its *surface* — a real selection session — is covered nowhere outside the module, because the integration suite has no terminal |
| `discovery.rs` | 2646 | 42 | 42 / 1125 | Permission traversal has a premise-checked Unix fixture; 1,001 synthetic errors prove per-root aggregation stays bounded |
| `cli.rs` | 2874 | 61 | 61 / 980 | Good |
| `output.rs` | 1113 | 22 | 22 / 543 | **Split verdict, and the split is the finding.** The in-file tests cover bounded log output and update JSON; the envelope layer has exactly **one** direct test — `cleanup_json_result_key_set_is_stable` (`output.rs:710`) pins the cleanup envelope's `result` key set — and the *scan* envelope has none |
| `config.rs` | 1547 | 21 | 21 / 616 | Good for its surface |
| `progress.rs` | 1002 | 16 | 16 / 542 | Thin relative to size. Terminal-capability detection is mostly untested, and integration tests force `--no-progress` (`cli_contract.rs:273`), excluding the renderer from end-to-end coverage by construction |
| `discovery_state.rs` | 1335 | 18 | 18 / 636 | Candidate provenance and cache/runtime exclusion are pinned alongside C028 absence handling — but see R9: the absence rule itself and `sweep_own_stale_temps` are not discriminated by any case |
| `policy.rs` | 1666 | 26 | 26 / 825 | Adequate for the module deciding the Routine/Full scope boundary: decision table, symlink refusal, premise recheck, Full-generation freshness, and raw-candidate identity are pinned |
| `traverse.rs` | 1098 | 12 | 12 / 770 | **Still thin for the module that walks the whole filesystem.** No test double for traversal-scale error attribution; `source_activity` (`traverse.rs:575`) has no production caller, and its one test does not discriminate what it claims (R8) |
| `report.rs` | 387 | 11 | 11 / 190 | **Unusually good for its size.** Covers control-character and non-UTF-8 path escaping, zero-state wording, the full `format_bytes` ladder including `u64::MAX`, group ordering with the path tie-break, and total de-duplication across equal physical roots. Two are `#[cfg(unix)]` (`report.rs:207`, `:220`) because they use `OsString::from_vec` |
| `editor.rs` | 292 | 5 | 5 / 160 | Low, but the process-exec behaviour is covered end to end at `cli_contract.rs:1468`, the right place for it |
| `domain.rs` | 500 | 1 | 1 / 471 | Defensible. Pure data, exercised transitively, with one well-premised test for the saturating counter conversion. Semantic *rules* live in the modules that interpret the types |
| `error.rs` | 16 | 0 | — | Defensible |
| `lib.rs` | 16 | 0 | — | Defensible |
| `main.rs` | 2513 | 12 | 12 / 2008 | **Twelve inline tests for the composition root**, added by C038 and extended since: the interactive surface's three endings, the report verdict, and the M018 coverage gate's three endings. Still thin — `cli_contract.rs` runs the binary as a subprocess and pins several non-zero exit paths, but `run()`'s structure remains opaque, and C023 established that the cleanup progress-bar ordering here is unreachable from the tests |

The contrast between `report.rs` and `output.rs` is the finding. The *human*
renderer has 11 unusually strong tests, while the envelope layer below the log
renderer has one — and the
one surface automation actually depends on has all its assertions living in a different
crate (`tests/`). Add a field to `EnvelopeV1` and the cleanup envelope's key set
is caught by that one in-crate test; the *scan* envelope's shape is caught by
nothing inside the crate — only `cli_contract.rs` looks at it, and only if a new
case happens to. M012B made this sharper, not softer: `output.rs` left the
zero-test list, so a reader scanning counts now sees a covered module where the
versioned DTOs are exactly as unpinned as before.

### Behaviours a reviewer should treat as under-verified

1. **`main.rs` composition and the exit-code table.** `run()` stitches the two pipelines together and maps outcomes to codes. **All three** codes are pinned: `1` asserted at `cli_contract.rs:1048` and `:3149`, `2` at `:969` and `:3451`, and `0` at `:2448` and `:2478` (the resolved-scope label test is at `:505`). The *distinct failure routes* are not all pinned — a usage error and a config error both surface as `2`, and nothing distinguishes them at the exit-code level. `src/main.rs:1995` shows the `full_incomplete` → 1 path. The cleanup pipeline has its own `Ok(1)` route — a failed unit or a blocked scope (`src/main.rs:524`) — now tested through the `scope_blocked.is_some()` shape M012A introduced, and M012A also made bare `cargo cleanme` a cleanup path, so `run()`'s dispatch is materially larger than when this list was written.
2. **`clean --full` itself.** No live run exists, and none can exist on a host with an unreadable directory, because `policy::full_cleanup_roots` (`src/policy.rs:548`) fails closed with exit 2 whenever the Full reconciliation was incomplete (`src/policy.rs:553`). The only evidence is a source-string guard on `enum EmitReport` (`tests/cli_contract.rs:3591`, `src/main.rs:1533`). A pty harness, as C023 proposed for the progress-bar ordering, is the shape of thing that would close this properly.
3. **`update_json` output shape** (`output.rs:950-986`). Bypasses `EnvelopeV1` and has its own `schema_version` literal at `output.rs:952`. C022 added the four missing cases (`output.rs:1016`, `:1042`, `:1056`, `:1096`) plus the CLI failure-shape case; what remains unpinned is a successful live document and the schema reconciliation itself. See R3.
4. **`output.rs` envelope field completeness.** `scan()` (`output.rs:105`) and `cleanup()` (`output.rs:161`): adding, removing, or renaming a field breaks no in-crate test. The 18 tests in the log and path modules all live below `pub mod log` or in `path_tests`; the four `update_tests` cover the other contract.
5. **`report.rs` rendering is covered, but not end to end.** The 11 inline tests are strong; the default `--format text` path is never asserted through the binary.
6. **`domain.rs` semantics.** One test is defensible for the data shapes, but any *meaningful* method on those types is untested by construction — nothing would catch one being added.
7. **`traverse.rs` measurement.** 12 tests for parallel size/recency measurement. Symlink handling, `CACHEDIR.TAG` qualification, and per-entry `ReadDir` error attribution are what would go wrong on a real machine, and what the platform matrix would be best placed to catch — but the fakes are hand-rolled per test, so real behaviour is only sampled, and the one activity test does not discriminate (R8).
8. **The whole self-update path.** 54 inline tests against `FixtureEnvironment`, plus manual live rehearsal. Every published defect in this line escaped that apparatus, and the reason is structural: a fixture that replaces the network can never discover that the real network rejects you. The automated evidence is a substitute for the thing, not a check of it.
9. **Cross-platform behaviour generally.** 59 of the 83 `cli_contract.rs` cases are `#[cfg(unix)]`, so a Windows lane exercises the argv, dispatch, and envelope layers but never the spawn-and-delete layer. `check-fixture-portability.py` closes the ungated-POSIX-fixture hole, and its own docstring (script lines 2-57) disclaims checking whether a case's pass condition proves its intended branch. That disclaimer *is* the guarantee's boundary: a green run means "no statically visible platform-shaped fixture is ungated", not "every fixture proved something on every lane". The C013 25-row inventory remains the only real audit of that question, and it is a point-in-time artifact, not a continuous guarantee.

---

## 10. Review checklist

1. **Does a new external-process fixture prove its own premise?** Add a guard that fails when the fake cannot be reached, before the behaviour assertion. References: C012's premise check; `packaging/tests/test_installers.py::self_test` (script line 1585), which proves each guard rejects a broken setup. Without failing-direction proof, it is a C012 waiting to happen.
2. **Is a new fixture platform-gated?** Any `#[cfg(test)]` block spawning a process, writing a shell script, or using `os.pathsep` needs `#[cfg(unix)]` / `#[cfg(windows)]` discipline or a portable implementation. `scripts/check-fixture-portability.py` catches the static shape, in `ci.yml` `checks`.
3. **Is a new external-process fixture actually exercised — not just present?** The premise is untested by default. Confirm the stub is *reached*: assert on what the subject passed to it (the `RecordingRunner` pattern, `workspace.rs:6134`), not only on what came back. C012 was green because an unrelated failure satisfied the assertion.
4. **Is a new `scripts/` file wired into a workflow or the operator gate?** All 17 are reachable from an automated surface; the three exceptions (`validate-staged-release.py`, `post-release-smoke.sh`, `verify-release-attestation.py`) are event-specific by construction and say so in the [§5](#5-the-contract-checkers) table — see [§6](#6-ci-as-the-enforcement-point). A new checker reachable from neither repeats that gap. Wire it, or document why not, **in the same change**: `check-doc-citations.py` will fail if the new script is not in the §5 table, so the omission cannot ship.
5. **Changing a field in `output.rs`?** Exactly one in-crate test sits below `pub mod log`: `cleanup_json_result_key_set_is_stable` (`output.rs:710`) pins the *cleanup* envelope's `result` key set. The *scan* envelope's shape, and every other field, are asserted only from outside — `cli_contract.rs:463`, `:505`, `:987`, `:1026`, `:1073`, `:3480`, `:3561`. Update them in the same commit; nothing inside the crate will remind you.
6. **Changing `update_json` (`output.rs:950`) or `update_json_stream` (`output.rs:988`)?** Four cases in `output.rs:1016`, `:1042`, `:1056`, `:1096` pin the document and the stream byte-for-byte, and `cli_contract.rs` pins the failure shape and the stdout-write count. What is still unpinned is a *successful* live run; if you need one, the honest route is a published release (the post-release smoke), not a test hook. Routing it through `EnvelopeV1` remains a contract change and needs its own plan and a version bump — it was deliberately not done here.
7. **Touching `output.rs:143` or `output.rs:212` (`schema_version` literals)?** Confirm the trailing-newline invariant and the stderr-separation invariant (`cli_contract.rs:463`, `:3561`) still hold for the surface you touched. They are the reason the JSON is safe to parse unattended. The log surface has its own, stronger invariant — `assert_bounded_log_line` (`cli_contract.rs:3037`) — so a log-mode edit should run the eleven log-mode cases in `cli_contract.rs` rather than the JSON ones.
8. **New exit codes in `main.rs`** — add a subprocess assertion in `cli_contract.rs` beside the existing ones (`:1048` for `1`, `:969` for `2`, `:2448` for `0`, `:505` for the scope label). The three exit-code *values* are pinned; the distinct *routes* to `2` are not, and nothing distinguishes a usage error from a config error by exit status.
9. **Touching `traverse.rs`?** 12 tests (module opens at `:772`), and `source_activity` (`:575`) has no production caller — its only call site is `traverse.rs:994`, inside its own test module. If you are adding a caller, say so; if not, its test proves nothing about the system. If you are touching the recency short-circuit, note that the existing case resolves on the *directory's* mtime, not the file's (R8) — a new case must make the file the deciding entry.
10. **New behaviour in `policy.rs`** — 26 tests, and this module decides the Routine/Full scope boundary, a documented invariant. Cheap to cover, still thinner than the module's importance.
11. **Reactivating or adding a `.crates.toml` provenance path** — C017 was exactly this: a schema no current Cargo writes, misread as self-managed, which *replaced a binary Cargo owned*. The classification lives in `src/update.rs`, and its tests run against `FixtureEnvironment` (`update.rs:1511`), which cannot reproduce real Cargo's on-disk state.
12. **Any change to the three event-specific scripts** (`validate-staged-release.py`, `post-release-smoke.sh`, `verify-release-attestation.py`) — they have wiring but no receipts, so nothing records whether they ever ran. That is R1 in [§7](#7-the-false-green-problem), and the C013 lesson is that verifiers fail exactly when they start being useful.
13. **Does a new or edited test observe wider than its subject?** If an assertion inspects a shared surface — the process-wide temp directory, a global registry, one lock file — ask whose state it can see. `staging_is_cleaned_up_on_success_and_on_failure` scanned every `cargo-cleanme-update-test-*` path in `/tmp` while sibling tests minted names from the same prefix; the subject's own state was never actually checked. Bind the evidence to the fixture or transaction, and add the two controls C021 did: one that makes the check **fail** on a deliberate leak of its own state, and one that makes it **pass** while a foreign same-prefix path is live.
14. **Adding a checker without a `--self-test`?** There is no exception left. C022 removed the last one (`check-installer-contract.py`), and §5 note 5 records what its self-test has to assert to be worth running: the mutation changed the fixture, the checker rejected it, **and** it rejected it for the intended diagnostic. That third assertion is the one this repository most often gets wrong — a check that keeps rejecting for an unrelated reason looks identical to a working guard until you delete the invariant and watch the case stay green.
15. **Is the rule you just wrote actually *decided* by a case?** A green case that would stay green with the rule deleted is decoration with a passing status. Ask what input makes the case fail *for that rule specifically*. R9 records two live instances: the C028 narrow-absence probe (`src/discovery_state.rs:355`) and `sweep_own_stale_temps` (`src/discovery_state.rs:562`).
16. **Is a test's name a claim the assertions support?** `source_scan_short_circuits_recent_files` and `zero_retention_disables_expiration_and_nested_roots_collapse` both name a mechanism the assertions do not exercise. A name is a contract with the next reader.
17. **Citing a line in `architecture/`?** Write the `file.rs:N` form, not the bare `:N` — only the former is range-checked by `check-doc-citations.py` (R10). And re-derive the line by reading it; a shifted number that still resolves is worse than no number, because it looks verified.
18. **Is a failing test recorded with a corrective?** A red in the evidence base with no plan behind it (R7) is a gap the correction process is supposed to catch. If a test fails on a host the project supports, the answer is a corrective plan, not a closure record that calls it pre-existing. R7 is the worked example: it sat in a closure record as "needs its own corrective" for four days and no plan was opened, and the red it described was silently suppressing the entire integration-test surface on CI.
19. **Does a test state its premise?** A case whose expected outcome depends on the *host* — a toolchain version, a filesystem feature, a locale — must say so and derive the expectation from the product's own premise. R7 asserted one outcome for a host-dependent branch; the repair derives the expectation and asserts a *specific* outcome per premise, never a disjunction that accepts whatever the host produces.

---

## See also

[Architecture overview](overview.md) · [Orchestration](13-orchestration.md) ·
[Cleanup](09-cleanup.md) · [Self-update](12-self-update.md) · [Reporting](10-reporting.md) ·
[Interactive](16-interactive.md) · [Distribution & release](15-distribution-and-release.md) ·
and `plans/registry.md:270-284` for the project's own account of the false-green incidents, in
its own words.
