# Testing & Verification — how this project keeps its tests honest

> Component deep dive · part of the [architecture overview](overview.md)

---

## 1. Responsibility

The verification apparatus does three distinct jobs. The third is what makes it unusual.

**(a) Product behaviour.** 237 inline `#[test]` functions in `src/`, plus two integration suites that drive the compiled binary as a subprocess: ownership proof, cleanup authorization, the three clean modes, workspace resolution, the versioned JSON contract.

**(b) Structural contracts invisible at runtime.** 12 scripts in `scripts/` and 4 GitHub workflows. No product code path can detect a file left out of the published crate's `include` allowlist, completions drifted from `cli.rs`, a tag pointing at the wrong commit, or release bytes differing from qualified bytes. These are properties of the repository *as an artifact* — see [Distribution & release](15-distribution-and-release.md).

**(c) Properties of the test fixtures themselves.** The category this project invented. A test that stubs a tool the subject cannot invoke is indistinguishable from one that does not stub it; a test whose pass condition is an unrelated failure reports coverage that does not exist. Both classes shipped here ([§7](#7-the-false-green-problem)). The response was not more tests but *premise guards*: assertions inside the fixture, before the behaviour under test runs, that fail loudly when the fixture can no longer prove what it claims to. `check-fixture-portability.py` generalises the idea into a static checker.

A green result here is not a sufficient claim on its own. The correct question is: *what
were the premises, and is any of them unchecked?*

---

## 2. The test pyramid as it actually stands

| Layer | Location | Count | What it can catch | What it cannot catch |
|---|---|---:|---|---|
| Inline unit tests | `src/**/*.rs`, in-file `#[cfg(test)] mod tests` | 237 `#[test]` | Logic errors, ownership/authorization decisions, argument construction, refactor regressions | Anything needing a real process, filesystem, or network; any premise of a fake |
| Integration — CLI contract | `tests/cli_contract.rs` (708 lines) | 9 `#[test]` | The subprocess boundary end to end: argv normalization, exit codes, JSON shape, `--stats` stream discipline | Internals of `run()`; the `update` command; anything on a real machine |
| Integration — end to end | `tests/end_to_end.rs` (211 lines) | 1 `#[test]` | One full scan against a synthetic tree with real mtimes | Cleanup, update, config editing |
| Shared harness | `tests/common/mod.rs` (102 lines) | 0 tests | `set_path_modified` / `backdate_file` (per-platform mtime control) | — |
| Contract checkers | `scripts/` | **12** (not 13 — see note) | Published-crate contents, fixture portability, installer/shape/smoke contracts, release identity, benchmark regression | That a fixture case proves its intended branch (explicitly disclaimed) |
| CI | `.github/workflows/` | 4 | Cross-platform, cross-toolchain, MSRV, generated-doc freshness, release drift | Doctests (`cargo test --all-targets` skips them) |

**Note on the script count.** The brief for this document stated 13 scripts in `scripts/`. The directory contains **12**; `find scripts/ -type f` returns exactly twelve. All twelve are characterised in [§5](#5-the-contract-checkers). The likely source of the discrepancy is `packaging/tests/`, which holds two more Python files (`test_installers.py`, `fixture_server.py`) that are verification apparatus but live outside `scripts/`. Counting both directories gives 14.

### Per-file inline test distribution

| File | `#[test]` | `mod tests` opens at | Total lines | Lines in test module |
|---|---:|---:|---:|---:|
| `src/cleanup.rs` | 71 | 2324 | 6161 | 3838 |
| `src/workspace.rs` | 44 | 1447 | 3771 | 2325 |
| `src/update.rs` | 30 | 1021 | 1971 | 951 |
| `src/discovery.rs` | 18 | 827 | 1370 | 544 |
| `src/cli.rs` | 17 | 201 | 506 | 306 |
| `src/config.rs` | 14 | 309 | 548 | 240 |
| `src/progress.rs` | 12 | 500 | 703 | 204 |
| `src/discovery_state.rs` | 11 | 403 | 656 | 254 |
| `src/report.rs` | 7 | 73 | 195 | 123 |
| `src/policy.rs` | 5 | 249 | 349 | 101 |
| `src/traverse.rs` | 6 | 470 | 599 | 129 |
| `src/editor.rs` | 2 | 157 | 215 | 59 |
| `src/domain.rs` | 0 | — | 381 | — |
| `src/error.rs` | 0 | — | 16 | — |
| `src/lib.rs` | 0 | — | 15 | — |
| `src/main.rs` | 0 | — | 606 | — |
| `src/output.rs` | 0 | — | 265 | — |
| **Total** | **237** | | | |

**The count is declared, not executed.** 237 `#[test]` functions exist in `src/`; **236
compile and run on Linux**, because exactly one is gated to other platforms
(`discovery.rs:1092`, `#[cfg(any(target_os = "macos", windows))]`). A further 18 carry
`#[cfg(unix)]` or `#[cfg(target_os = "linux")]`, so on Windows the inline suite is 1 of
237. A green local run therefore does not mean a green run elsewhere — which is the
whole reason the OS matrix exists, and the reason
`check-fixture-portability.py` exists to catch a *new* ungated fixture before a lane
disagrees about it.

`cleanup.rs` (71) + `workspace.rs` (44) = 115 of 237 = **48.5%**. The two largest and most
safety-critical modules — authorization, ownership proof, pre-spawn decision, and the `cargo
metadata` resolution everything depends on — hold essentially half the unit tests. That
allocation is correct, and it is the one place the pyramid is emphatically *not* inverted.

### The structural observation: the testing is inverted

The convention is a separate `tests/` tree beside a lean `src/`. This crate does the
opposite. `cleanup.rs` is 6161 lines of which **3838 are test code** — 62% of that file is
fake-Cargo runners and assertion bodies. `workspace.rs` is 3771 lines, 2325 of them tests.
`report.rs` runs the other way: its test module opens at line 73, roughly the first quarter,
so the rendering tests sit *above* most of the production code they exercise.

The consequence for review is concrete. Opening `src/cleanup.rs` to understand the pre-spawn
decision puts 3838 lines of `FakeCleanupRunner` variants (`cleanup.rs:2324`–`6161`) between
the reader and nearly all production code. Locate the boundary first — `mod tests {` at
`cleanup.rs:2324` — and read upward. Do not infer "few tests" from a file's position in the
tree, and do not count `cleanup.rs` as production code when estimating where complexity
lives.

---

## 3. Inline unit tests

### The test-double architecture

Two process seams keep the 237 declared tests from spawning a real `cargo`:

- **`CargoRunner`** — `src/workspace.rs:29`. `fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>`.
- **`CleanupRunner`** — declared in `src/cleanup.rs` for the destructive step, so cleanup is exercised while `cargo clean` is a no-op returning recorded bytes.

Two further seams remove the remaining side effects:

- **`UpdateEnvironment`** (`src/update.rs`) — the entire network/filesystem surface of self-update, so `update.rs` tests need neither network nor process globals.
- **`ProgressObserver`** (`src/progress.rs:50`) — `NoopObserver` (`progress.rs:56`) is the zero-cost default the cleanup tests pass in, so progress rendering never interferes with an assertion.

### Catalogue of fakes

Production impls, for contrast: `SystemCargoRunner` (`workspace.rs:78`),
`SystemCleanupRunner` (`cleanup.rs:387`, impls `CargoRunner` at `:389`),
`WorkspaceCleanupAdapter<'_>` (`cleanup.rs:1336`), `ProofRefreshMeter<'_>`
(`cleanup.rs:1490`), `HttpEnvironment` (`update.rs:679`), `IndicatifRenderer`
(`progress.rs:424`), `NoopObserver` (`progress.rs:56`).

| Test fake | File:line | Failure mode simulated |
|---|---|---|
| `FakeCargo` | `workspace.rs:1451` (impl 1461) | Baseline. Flags `fail_locate`, `fail_metadata`, `malformed` cover non-zero exit and unparseable `metadata` JSON |
| `CountingRunner` | `workspace.rs:1569` (impl 1573) | Asserts `cargo metadata` is invoked exactly N times — the no-redundant-call invariant |
| `MemberFirstFails` | `workspace.rs:1832` (impl 1836) | `locate-project` fails for the member manifest but not the root; proves the root still resolves |
| `TwoWsRunner` | `workspace.rs:2003` (impl 2009) | Two independent workspaces; pins *two locates, two metadatas* — no cross-workspace reuse |
| `FailFirstRunner` | `workspace.rs:2072` (impl 2076) | One workspace's locate fails; proves the failure does not poison an unrelated one |
| `RecordingRunner` | `workspace.rs:3224` (impl 3228) | Records `(cwd, args)` so a test asserts argument *construction*, not the fake's result |
| `FixtureEnvironment` | `update.rs:1031` (impl 1088) | Replaces network, `curl` discovery, process globals; drives `staging` and a fixed `version` |
| `TestObserver` | `progress.rs:64` (impl 105) | Records semantic events (`phases`, `visited`, `pruned`, `manifests`, `workspaces`, `cargo_failures`) — deliberately not refresh counts, which are renderer-side |
| `FakeCleanupRunner` | `cleanup.rs:2333` (impls 2366 / 2449) | The workhorse. Records every call; `remove_on_execute` and `unresolved_manifest` toggle Execute's delete and the unresolved-manifest block |
| `ChangingTargetRunner` | `cleanup.rs:3149` (impls 3155/3175) | `target` changes between calls — the first-generation race |
| `PerPathFailRunner` | `cleanup.rs:3211` (impls 3215/3258) | One manifest path fails to locate; proves the *entire* selected scope is blocked, not just the failing unit |
| `RaceRunner` | `cleanup.rs:3875` (impls 3911/3974) | Mutates the filesystem on the *second* `metadata` call — between scan and final proof; `clean_calls` proves no clean was staged |
| `BuildChangingRunner` | `cleanup.rs:4089` (impls 4095/4140) | Swaps `build/<hash>` between resolution passes; targets marker-qualification revalidation |
| `StagedCargo` | `cleanup.rs` (impl 4748) | Returns `"mock cargo clean success"` — a clean that succeeds *without deleting*, so the test asserts the call, not the filesystem |
| `ParallelProbeRunner` | `cleanup.rs:4766` (impls 4772/4793) | Tracks peak concurrent `metadata` calls; proves the parallel fan-out is real |
| `MetadataMutationRunner` | `cleanup.rs:4814` (impls 4820/4865) | Wraps `StagedCargo`, mutating emitted metadata (`WorkspaceRoot`, `AddMember`, `Malformed`) after the inner call |
| `MetadataOnly` | `tests/end_to_end.rs:73` (impl 75) | Integration-suite Cargo stub — the only fake outside `src/` |

Two deserve a note. `RecordingRunner` carries a comment (`workspace.rs:3220-3223`) stating
the *premise* of its own test: a prior implementation "resolved zero workspaces, exited 0,
and reported no groups", and a recording runner catches that at the argument level without
the network. That is the right instinct — assert on the call, not on the outcome the fake
produces. `RaceRunner`'s `mutate` is a closure parameter, so each race case supplies its own
mutation rather than the fake hardcoding one scenario.

### Zero-test modules

`grep -cE '^\s*#\[test\]'` over `src/domain.rs src/error.rs src/lib.rs src/main.rs
src/output.rs` returns `0` for all five. They split into two very different groups.

**Defensible.** `lib.rs` (15 lines, module manifest), `error.rs` (16 lines, six `AppError` variants), and `domain.rs` (381 lines of pure data types — no I/O, no branching on process state). These are exercised transitively by every test in the modules that consume them.

**Not defensible, and significant.**

- **`src/main.rs` (590 lines, 0 tests).** The composition root. It *is* covered — `cli_contract.rs` runs the binary as a subprocess, so `main()` and `run()` execute in all 8 cases, including exit-code assertions at `cli_contract.rs:268` (`Some(1)`) and `:630` (`Some(17)`). But that is black-box coverage of an opaque process: nothing asserts on `run()`'s structure, and the exit-code table is only as complete as the cases someone remembered to write.
- **`src/output.rs` (265 lines, 0 tests).** The entire versioned DTO layer — `EnvelopeV1` (`output.rs:8`), `scan()` (`:103`), `cleanup()` (`:158`), both hardcoding `schema_version: 1` at `:140` and `:206`. Every field is asserted only from outside the process. No unit test pins the struct shape, so adding a field to `EnvelopeV1` requires someone to notice and update external assertions.
- **`update_json` (`main.rs:568`)** is the sharpest single gap. The only machine-readable output path that does **not** go through `EnvelopeV1`; it hand-builds a `serde_json::Map` with its own `json!(1)` at `main.rs:571` and a bespoke `result` object at `:578`. It has no test of any kind.

### `traverse.rs`

Its test module opens at line 457 and contains **5** `#[test]` functions. For the module
that walks the entire filesystem — parallel size and recency measurement, symlink and
`CACHEDIR.TAG` handling, `ReadDir` error attribution, pruning — that is low. The
parallel-probe machinery has a dedicated counting fake in `cleanup.rs`
(`ParallelProbeRunner`, `cleanup.rs:4766`); `traverse.rs` has nothing comparable.

`source_activity` is declared at `src/traverse.rs:268` and has exactly one call site in the
crate: `src/traverse.rs:593`, inside the test module opening at 470. **There is no
production caller** — production uses `workspace_source_activity` (`src/workspace.rs:1428`)
instead. So `traverse::source_activity` is a `pub` function with no production consumer
whose sole exercise is its own test. That test proves the function works and nothing about
the system's behaviour: a coverage claim that looks green and is empty, and a milder cousin
of the C009–C013 incidents.

---

## 4. Integration suites

### How they invoke the binary

**Definitive answer: as a subprocess.** Both suites use `env!("CARGO_BIN_EXE_cargo-cleanme")` with `std::process::Command`, never a library call. `tests/cli_contract.rs:15` resolves the staged binary path; it is used at lines 96, 127, 158, 182, 219, 255, 352, 406, 436, 475, 509, 544. Three cases go further, invoking a real `cargo` as `Command::new("cargo")` with `cleanme` first (lines 101, 135, 166) to compare external subcommand behaviour against direct invocation. `end_to_end.rs` is the exception: it drives the library's scan entry point in-process with a `MetadataOnly` Cargo stub (`end_to_end.rs:73-75`).

Consequence: `main.rs` executes end to end in all 8 `cli_contract.rs` cases — argv
normalization (`Cli::parse_normalized`), dispatch, exit codes — but only as an opaque
process. `run()`'s internals are untested, and the `update` command has no integration case
at all.

### `tests/common/mod.rs`

102 lines, no tests. Exports `set_path_modified` (`:24`) and `backdate_file` (`:98`), which
set file mtimes directly. `set_path_modified` is hand-rolled per platform because the
inactivity threshold is time-based and tests must not sleep: `utimensat` on Unix (`:36-40`,
with the FILETIME-ticks epoch constant) and `SetFileTime` on Windows (mirrored at
`cli_contract.rs:590`/`:598`). This is the most load-bearing piece of portable test
infrastructure in the repository — without it "inactive" cannot be tested at all, and on
Windows the naive approach was precisely the C009 defect class.

### `tests/cli_contract.rs` — 8 cases

| Test | Line | Contract pinned |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 80 | The `#[cfg(unix)]` premise: the built artifact is reachable as `cargo-cleanme` on `PATH` and is what `cargo` dispatches to. Without it, the three "external == direct" comparisons prove nothing |
| `cargo_external_subcommand_help_matches_direct_help` | 120 | `cargo cleanme --version` and `--help` byte-match direct invocation |
| `cargo_external_config_edit_help_matches_direct` | 154 | Same for the subcommand form, which exercises a different clap path |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 177 | The scan envelope, plus stream separation and newline discipline |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 208 | `clean --mode simulate` envelope and summary |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 246 | A blocked scope still emits parseable JSON *and* exits 1. That pairing is the automation contract |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 286 | `#[cfg(unix)]`. Unattended Execute with `--yes` reaches a real `cargo clean` through a fake, emitting `reason_code: "cleaned"` with selector fields and byte counts |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 582 | `config edit` really execs a process (`$EDITOR`), and an invalid edit is rejected rather than written |

### The machine-contract checklist

This is the real, executable shape of the versioned JSON contract. Every item is a live
assertion; change the DTO and these are the lines that fail.

| Assertion | Line |
|---|---:|
| `["schema_version"] == 1` (scan) | 200 |
| `["operation"] == "scan"` | 201 |
| `["scope"] == "explicit"` | 202 |
| `--stats` output appears on **stderr** (`"scan stats:"`), not stdout | 203 |
| stdout contains **exactly one** `\n` — one document, no trailing blank line | 204 |
| `["schema_version"] == 1` (cleanup) | 238 |
| `["operation"] == "clean"` | 239 |
| `["mode"] == "simulate"` — the requested mode is echoed, not inferred | 240 |
| single trailing newline on cleanup stdout | 242 |
| exit code `1` on a blocked scope | 268 |
| `["result"]["scope_blocked"] == true` | 270 |
| `["result"]["units"][0]["reason_code"] == "cleaned"` | 379 |
| `["result"]["selector_kind"] == "profile"` | 380 |
| `["result"]["selector_value"] == "dev"` | 381 |
| `units[0]["before_bytes"]` present and numeric | 383-390 |
| `units[0]["output_union_before_bytes"]` present and numeric | 391-395 |
| simulate mode reports `outcome == "simulated"` | 426 |
| policy-disposition case reports its `reason_code` | 459 |
| policy-disposition case reports `policy_disposition` | 463 |
| invalid selector reports `reason_code` | 497 |
| unsupported selector (too-old Cargo) reports `reason_code` | 532 |
| package selector succeeds: `reason_code: "cleaned"` | 565 |
| `selector_kind == "package"` | 566 |
| `selector_value == "fixture@0.1.0"` — name **and** version | 567 |
| non-zero exit code `17` for the internal-failure path | 630 |

`tests/end_to_end.rs:203-205` adds three: `["result"]["groups"][0]["ownership"] ==
"private"`, `["result"]["summary"]["group_count"] == 1`, and `groups[0]["bytes"]` as a
`u64`.

The two byte-count assertions are separated deliberately: `before_bytes` is one unit's own
size, `output_union_before_bytes` the union across its complete output set. Conflating them
would understate what a cleanup reclaims.

### What the suites deliberately do not pin

- **No `update` JSON.** `update_json` is unasserted — no `schema_version` check, no newline invariant, no exit code, no case in either suite.
- **No negative CLI parsing coverage.** `cli.rs` has 17 inline tests, but the suites pin only two non-zero exit codes: `1` and `17`.
- **No assertion on human-readable output.** The default `--format text` path through `report::render` is covered only by `report.rs`'s 7 inline tests.
- **No destructive Execute case on Windows.** The one Execute case is `#[cfg(unix)]` (`cli_contract.rs:284`).
- **No timing, progress, or TTY behaviour.** `--no-progress` is passed (`cli_contract.rs:183`) to make output deterministic, so `IndicatifRenderer` and the terminal-capability detection in `progress.rs` are integration-untested by construction.

---

## 5. The contract checkers

12 scripts. The dividing line is the one this project cares about: **outcome checkers** fail
when the subject misbehaves; **premise checkers** fail when the *evidence* is no longer
valid. Premise checkers are the distinctive category.

| Script | Enforces | Fails when | Wired into |
|---|---|---|---|
| `check-release-contract.py` | Published-crate contract: `Cargo.toml` `include` allowlist covers every runtime-input file; `/config.toml` present; the embedded template's content matches; `docs/reference.md`, `README.md`, `CHANGELOG.md` present; no `/target` leakage; required workflow/script files exist; `Cargo.toml` `version` is a plain semver triple | A file `src/config.rs` needs at runtime is absent from `include`, the embedded template drifted from `config.toml`, a required release surface is missing, or the version is unpublishable | `ci.yml` `checks`; `release-check.sh` |
| `check-fixture-portability.py` | Every external-process fixture in `src/**/*.rs` uses `#[cfg(unix)]`/`#[cfg(windows)]` gating, avoids unconditional `/bin/sh`, `sh -c`, `.sh` scripts, hardcoded `/tmp`, `:`-separated `PATH`, and a fixed fake binary name that collides across gates | A fixture is POSIX-shaped but ungated, or a gated fixture can be skipped on some lane and still pass | `ci.yml` `checks`; `release-check.sh` |
| `check-installer-contract.py` | The wrappers under `packaging/installers/` exist, are executable, carry the expected shebang, and the release manifest lists the expected per-platform entries | A wrapper is missing, not executable, or manifest and wrappers disagree | `ci.yml` `installers`; `release-check.sh` |
| `check-release-identity.py` | Tag/source identity: the release tag matches the source commit, version and tag agree, and the provenance identity machine is internally consistent | Tag and source disagree — the class that made a release irreproducible | `release-binaries.yml`; `release-check.sh` |
| `check-post-release-smoke-contract.py` | The post-release smoke workflow is itself well-formed: required steps, lanes, and assertions present and in order | The smoke workflow is edited down to a shape that cannot fail | `post-release-smoke.yml`; `release-check.sh` |
| `gen-release-workflow-shape.py` | Generates the *expected* shape of `release-binaries.yml` from the release contract, so the workflow is a checked-in generated artifact rather than hand-edited prose | The committed workflow drifts from the generated shape | `release-check.sh`; output compared in `release-binaries.yml` |
| `validate-staged-release.py` | Six-step staged-release proof: exact artifact inventory, sidecar integrity (checksums), manifest agreement, contract agreement, static ABI, real-installer qualification — i.e. published bytes are the qualified bytes | Any step cannot be satisfied against the staged tree | **Nowhere — see [§6](#6-ci-as-the-enforcement-point)** |
| `release-check.sh` | The local pre-push gate: full crate test suite, `cargo test --doc` (which CI does not run), all the Python checkers, the generated-shape comparison, `fmt`/`clippy` hygiene | Anything the local gate covers fails | `ci.yml` `checks`; also the documented manual path |
| `qualify-cargo-selectors.sh` | Real-Cargo selector characterization: what `--profile`/`--package` actually do on genuine Cargo, and where the support boundary is | Real behaviour does not match what the fakes assume | **Nowhere** |
| `release-benchmark.py` | Compares current scan performance against `release/baseline-benchmark.json`, fails on a configurable regression percentage | The scan is materially slower than the recorded baseline | `ci.yml` `benchmark` |
| `smoke-release-candidate.py` | Release-candidate smoke: the built binaries respond correctly before publication | A candidate binary misbehaves | `release-binaries.yml`; also manual |
| `post-release-smoke.sh` | Post-publication verification against the *actual published bytes* — the external `update` check that first exposed C011 | The published release does not behave as qualified | `post-release-smoke.yml` |

Three notes:

1. **`/config.toml` in the allowlist is load-bearing.** `src/config.rs` embeds the template with `include_str!` at compile time, so a published crate without `/config.toml` in `include` builds fine and fails at runtime the first time it needs to create a config. `check-release-contract.py` asserts the entry specifically because no test and no runtime check can catch it — a pure packaging invariant, and a clear case of category (b).

2. **`check-fixture-portability.py` is a premise checker, and its own docstring (lines 23-32) states the limit of its guarantee.** It scans `src/**/*.rs` (globs at lines 54-59) and rejects statically-detectable POSIX-only fixture shape. It explicitly does **not** check whether a case's pass condition proves the intended branch — "reviewing that is the point of the C013 inventory". A static checker can catch a missing `#[cfg(unix)]`. It cannot catch a test that would have passed anyway. Read its green as "no visibly platform-shaped fixture is ungated", never as "every fixture proves something on every lane".

3. **`release-check.sh` closes a CI gap of its own making.** `ci.yml` runs `cargo test --all-targets --all-features`, which does not execute doctests; the local gate runs `cargo test --doc` separately. Doc examples are verified by a human with a shell script, not by CI.

---

## 6. CI as the enforcement point

`.github/workflows/ci.yml` defines five jobs.

**`checks`** — the bulk of the gate. OS matrix of `ubuntu-latest`, `macos-latest`, `windows-latest`, plus a **toolchain matrix**, so the 236 unit tests that compile on this platform and 9 integration cases execute on the MSRV toolchain and on stable. Runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` (a real lint gate — warnings are errors), `cargo test --all-targets --all-features`, and both `check-release-contract.py` and `check-fixture-portability.py` directly. A red build catches: any behavioural regression on any of the three platforms; any new warning; formatting drift; an `include` edit that breaks the published crate; a new ungated POSIX-shaped fixture. `--all-features` is what makes the matrix meaningful for the update path.

**`installers`** — runs `check-installer-contract.py` and the installer fixture suite `packaging/tests/test_installers.py` across the same three OSes. Catches a wrapper that stopped being executable, lost its shebang, or diverged on one platform. This is also where the C012/C013 class of Windows-fixture defect would surface.

**`generated-docs`** — regenerates completions and manpages via `xtask` and fails if the committed files differ. A *freshness* contract: it catches a `cli.rs` change never fed through the generator. It is also the one place a script's output is compared to a committed artifact — the same pattern as `gen-release-workflow-shape.py`.

**`benchmark`** — runs `release-benchmark.py` against `release/baseline-benchmark.json`, failing on a configured threshold. Catches a scan that has become materially slower in a way no correctness test notices.

**`msrv`** — a dedicated job pinned to Rust **1.89**, matching `rust-version = "1.89"` in `Cargo.toml`, against a crate on **edition 2024**. A red build means the code has started using a feature stabilized after 1.89. Because edition 2024 requires a comparatively recent toolchain, the two settings are coupled and cannot drift apart silently.

### Is every script wired into CI?

**No. Two scripts are invoked by no workflow.** Verified by searching every file in `.github/` and `packaging/` for each script's filename, and separately searching `scripts/release-check.sh` for each.

| Script | In a workflow? | In `release-check.sh`? |
|---|---|---|
| `check-release-contract.py` | yes (`checks`) | yes |
| `check-fixture-portability.py` | yes (`checks`) | yes |
| `check-installer-contract.py` | yes (`installers`) | yes |
| `check-release-identity.py` | yes (`release-binaries.yml`) | yes |
| `check-post-release-smoke-contract.py` | yes (`post-release-smoke.yml`) | yes |
| `gen-release-workflow-shape.py` | yes (`release-binaries.yml`, output compared) | yes |
| `release-benchmark.py` | yes (`benchmark`) | yes |
| `smoke-release-candidate.py` | yes (`release-binaries.yml`) | yes |
| `post-release-smoke.sh` | yes (`post-release-smoke.yml`) | yes |
| `release-check.sh` | yes (`checks`) | n/a — it is the gate |
| **`validate-staged-release.py`** | **no** | **no** |
| **`qualify-cargo-selectors.sh`** | **no** | **no** |

This is the significant finding of this section, and it is exactly the class the project has
been bitten by before. `validate-staged-release.py` is the only thing that proves the
published artifact is the artifact that was qualified — the six-step inventory / sidecar /
manifest / contract / ABI / installer proof. It runs solely when a maintainer remembers to
run it. It is also the *only* script `release-check.sh` does not reference, so "I ran the
local gate" and "I proved the release bytes" are unrelated statements. By contrast
`check-release-contract.py` asserts that `scripts/release-check.sh` exists as a file — but
nothing asserts that `validate-staged-release.py` was ever *run*.

`qualify-cargo-selectors.sh` is the same shape, narrower blast radius: the only
characterisation of real-Cargo selector behaviour, POSIX-only, run by no lane. The
`selector_unsupported` path asserted at `cli_contract.rs:532` is driven by a *stubbed*
version string, so the stub is the only thing that version gate has ever read in an
automated run.

Neither is necessarily wrong — both are plausibly manual by design — but the honest
statement is that they are unguarded, and nothing in the repository distinguishes "was run
and passed" from "was never run".

---

## 7. The false-green problem

The reason this project has a `scripts/` directory at all. The account is the project's own,
from `plans/registry.md:92-107`, cross-checked against the code.

**C009 — a POSIX-only fixture passing on no platform.** A Windows fixture used POSIX-only behaviour, so it did not work on Windows *and* was not runnable on Unix. The hosted Windows lane — the evidence every Phase 10 milestone depended on — was broken, and the fixture was green nowhere. Closure restored the green hosted Windows lane (run `37225697173`, `plans/registry.md:69`). This motivated `check-fixture-portability.py`.

**C011 — a feature that could not reach its authority, green anyway.** `cargo cleanme update` returned HTTP 403 from crates.io on **every host**, because the production transport (`eggup-curl`) cannot set a `User-Agent`, which crates.io requires. The feature was green in CI because the *fixture registry always answered*. A stub standing in for an authority the subject cannot reach is indistinguishable from a stub that succeeds. Corrective C011 reversed the transport to `eggup-eggfetch` and added a no-downgrade guard (`plans/registry.md:75`); the transport choice is explained in `Cargo.toml:60-88` and documented in [Self-update](12-self-update.md). The original milestone's closure record was deliberately **not** edited to imply success — it carries an addendum naming the unsatisfied requirement (`plans/registry.md:73`, `:90`).

**C012 — a Windows case that had never executed.** Green only because an unrelated real `cargo install` failed first: the pass condition was an unrelated failure, so the case reported coverage it did not have. Fixed with a `cargo.cmd` shim, `os.pathsep`, and a **premise check**. Unusually, it was disclosed by the product *starting to work correctly* — once the real `cargo install` succeeded, the fake was no longer reached, and the case went red for the right reason (`plans/registry.md:76`, `:99`).

**C013 — the sweep, and its most serious finding.** A repository-wide sweep of external-process fixtures produced a 25-row inventory and corrected **nine** evidence defects, each new premise guard proven to fail in the failing direction (`plans/registry.md:77`). The most serious finding was **not a test**: the release-candidate smoke validator was green on all three hosted lanes *while the subject under test was failing*. Tightening it exposed a real product defect, registered as C015. `plans/registry.md:107` notes C013 closed without repairing any production defect, by its own failure semantics — which is what a verification milestone is for.

**C014 / C016 / C017 — two shipped product defects.** C016 (`plans/registry.md:80`, `:107`): the self-update identity check ran the downloaded candidate **with no arguments**, so its stdout could never equal a version string — and before failing, it performed a filesystem scan of the whole machine. The transaction aborted safely every time, so nothing shipped is corrupt, but the advertised feature had never once worked in any published version. C017 (`plans/registry.md:81`): a `cargo install --root` binary was misread as self-managed and **replaced**, silently, leaving `cargo install --list` reporting a version the file no longer had; it also parsed a `.crates.toml` schema no current Cargo writes — the only defect in the line that mutated on-disk state owned by another tool. Both were found by C014's live rehearsal against a published release, not by any test. `plans/registry.md:144` records the cost: the corrective line needed **seven published versions**, and three product defects (C015, C016, C017) were found by the line's own evidence work.

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
| Tag/source identity drift | `check-release-identity.py` in `release-binaries.yml` |
| Smoke workflow edited into meaninglessness | `check-post-release-smoke-contract.py` |
| Docs/completions drift | `generated-docs` job |
| Performance regression | `benchmark` job vs `release/baseline-benchmark.json` |

**Residual risks in the present tree.** The first two are the ones I would put in a review.

**R1 — The published-bytes proof is manual-only, and the local gate does not even chain it.** `scripts/validate-staged-release.py` is the sole mechanism establishing "the public bytes are the qualified bytes". It is invoked by no workflow and by no other script. No artifact, receipt, or log in the repository records that it ran for any particular tag, and `check-release-contract.py` asserts only that the *file* exists. A regression that changes which bytes get published would be caught by nothing automated — and per the C013 lesson, the validator that would catch it is exactly the thing most likely to be edited down at the moment it starts failing.

**R2 — Real-Cargo selector behaviour is characterised only by a script no lane runs.** `scripts/qualify-cargo-selectors.sh` is the only place genuine Cargo's `--profile`/`--package` semantics and the support boundary are measured. It is unwired and POSIX-only. The automated coverage of selector behaviour (`cli_contract.rs:565-567`, `:509-532`) runs against a *stub* whose version string the guard itself reads — the guard is tested against its own input assumption, which is the precise C011 shape: a fake standing in for an authority that could have disagreed with the subject. If Cargo changed what `--profile dev` selects, or moved the version floor, nothing in the tree would notice.

**R3 — The `update` machine-output path has no test of any kind.** `update_json` (`main.rs:568`) is the only machine-readable surface bypassing `EnvelopeV1`, pinned by nothing: no `schema_version` assertion, no trailing-newline invariant, no exit code, no integration case. The two invariants that make the other JSON surfaces safe to automate against (`cli_contract.rs:204` single trailing newline, `:203` stream separation) do not extend to it. C016 and C017 both lived on the self-update path, whose automated output-shape evidence is nil.

**R4 — `traverse::source_activity` is test-only, and `traverse.rs` is thin.** `source_activity` (`src/traverse.rs:254`) has one call site, in its own test module (`traverse.rs:539`). A `pub` function with no production consumer whose only exercise is its own assertion is a coverage entry reporting capability the system does not use — a mild instance of a formally registered class. `traverse.rs` has 5 tests for the module that walks the whole filesystem.

---

## 8. Release verification

Three workflows verify properties of *published* artifacts — things no amount of in-tree
testing can see, because the subject is bytes on a registry and a CDN.

**`post-release-smoke.yml`** runs after publication against the **real published release**, not the staging tree. It is guarded by `check-post-release-smoke-contract.py`, which validates the smoke workflow's own shape — required steps, lanes, assertions — so the verifier cannot be quietly reduced to a shape that always passes. This is where C011 was found: `update` returned HTTP 403 in production while the suite was green. It drives `scripts/post-release-smoke.sh`. The lesson it encodes: the only trustworthy test of a distribution mechanism is one that uses the distribution mechanism.

**`release-binaries.yml`** builds the artifacts and, before publication, runs `check-release-identity.py` (tag/source identity), compares `release-binaries.yml` against the shape generated by `gen-release-workflow-shape.py` — so the workflow is a checked-in generated artifact, not hand-edited prose — and runs `smoke-release-candidate.py` against the candidates. Identity matters because a specific failure mode is invisible afterwards: if tag and source commit disagree, the release is irreproducible in a way no later check can detect. C014 added a machine-checked tag/source identity gate for this reason (`plans/registry.md:107`). `smoke-release-candidate.py` is the script C013 found green while its subject was failing; it is now a checked workflow step.

**`release-drift.yml`** verifies the release did not drift from the tag: that the commit a tag points at, the version in `Cargo.toml`, and the built artifacts still agree, and that regeneration is stable. It is the post-hoc detector for the class where someone amends or re-points a release after the fact.

**Relationship to `validate-staged-release.py` and Eggpack.** The Eggpack producer contract (`release/eggpack`, `packaging/`) supplies the generators and manifests these workflows consume. `validate-staged-release.py` is the offline six-step proof over a staged tree, and it is the one verification step in this document with no automated caller — see R1. Its six steps map onto the questions each workflow asks, but only the live workflows ask them automatically.

**The benchmark.** `release/baseline-benchmark.json` records measured scan performance for a reference tree; `release-benchmark.py` re-measures and fails on a configurable regression percentage, run by the `benchmark` job. The regression it catches is the class no correctness test notices: a change that keeps every answer right but makes the scan substantially slower. With `cleanup.rs` at 6161 lines and the parallel probe fan-out a live optimisation surface (`ParallelProbeRunner` exists precisely to assert the fan-out), that is a realistic and otherwise invisible failure mode. The trade-off is inherent: a wall-clock threshold is flaky on shared runners, so this gate is the one most likely to be loosened — itself a thing to watch in review.

---

## 9. Coverage assessment

| Module | Lines | Tests | Tests / prod. lines | Verdict |
|---|---:|---:|---:|---|
| `cleanup.rs` | 6161 | 71 | 71 / 2323 | Appropriate. Most safety-critical module has the most tests; the race fakes show the hard cases were found and pinned |
| `workspace.rs` | 3771 | 44 | 44 / 1446 | Appropriate, and the fakes are argument-focused rather than result-focused — the right instinct |
| `update.rs` | 1971 | 30 | 30 / 1020 | Reasonable unit coverage, but this is the module whose defects (C011, C016, C017) escaped to production. Unit tests cannot see a transport that cannot reach its authority |
| `discovery.rs` | 1370 | 18 | 18 / 826 | Thin-ish for a filesystem walk with attribution, but the attribution logic is pure and testable |
| `cli.rs` | 506 | 17 | 17 / 200 | Good |
| `config.rs` | 548 | 14 | 14 / 308 | Good for its surface |
| `progress.rs` | 703 | 12 | 12 / 499 | Thin relative to size. Terminal-capability detection is mostly untested, and integration tests force `--no-progress` (`cli_contract.rs:183`), excluding the renderer from end-to-end coverage by construction |
| `discovery_state.rs` | 656 | 11 | 11 / 402 | Adequate |
| `report.rs` | 195 | 7 | 7 / 72 | **Unusually good for its size.** Covers control-character and non-UTF-8 path escaping, zero-state wording, the full `format_bytes` ladder including `u64::MAX`, group ordering with the path tie-break, and total de-duplication across equal physical roots. Two are `#[cfg(unix)]` (`report.rs:88`, `:101`) because they use `OsString::from_vec` |
| `policy.rs` | 349 | 5 | 5 / 248 | Low for the module deciding the Routine/Full scope boundary. Pure and cheap to test; the gap is real |
| `traverse.rs` | 544 | 5 | 5 / 456 | **Too low for the module that walks the whole filesystem.** No test double for traversal-scale error attribution; and `source_activity` (`:254`) has no production caller |
| `editor.rs` | 215 | 2 | 2 / 156 | Low, but the process-exec behaviour is covered end to end at `cli_contract.rs:582`, the right place for it |
| `domain.rs` | 381 | 0 | 0 / 381 | Defensible. Pure data, exercised transitively. Semantic *rules* live in the modules that interpret the types |
| `error.rs` | 16 | 0 | — | Defensible |
| `lib.rs` | 15 | 0 | — | Defensible |
| `main.rs` | 590 | 0 | 0 / 590 | **Zero inline tests for the composition root.** Partly mitigated — `cli_contract.rs` runs the binary as a subprocess, so `main()` and `run()` execute in all 8 cases and two exit codes are pinned. But `run()`'s structure is opaque and `update_json` (`:568`) is entirely unasserted |
| `output.rs` | 265 | 0 | 0 / 265 | **Zero tests for the entire versioned DTO layer.** The sharpest structural weakness in the tree |

The contrast in the last three rows is the finding. The *human* renderer beside the
*machine* DTO layer has 7 unusually strong tests, while the machine layer has none — and the
one surface automation actually depends on has all its assertions living in a different
crate (`tests/`). Add a field to `EnvelopeV1` and nothing inside the crate fails; only
`cli_contract.rs` does, and only if a new case happens to look at it.

### Behaviours a reviewer should treat as under-verified

1. **`main.rs` composition and the exit-code table.** `run()` stitches the two scan pipelines together and maps outcomes to codes. Three are pinned (`1` asserted at `cli_contract.rs:318`, `17` at `:680`, and the resolved-scope label test at `:215`); the rest are not. `main.rs:576-578` shows the `full_incomplete` → 1 path, untested through a subprocess. (It used to read
   `full_incomplete || (full && !state_reconciled)`; that second clause is gone, and removing it is
   the fix recorded in [overview §7 item 8](overview.md).)
2. **`update_json` output shape** (`main.rs:584-606`). Bypasses `EnvelopeV1`, has its own `schema_version` literal at `main.rs:586`, no test. See R3.
3. **`output.rs` DTO field completeness.** `scan()` (`:103`) and `cleanup()` (`:158`): adding, removing, or renaming a field breaks no in-crate test.
4. **`report.rs` rendering is covered, but not end to end.** The 7 inline tests are strong; the default `--format text` path is never asserted through the binary.
5. **`domain.rs` semantics.** Zero tests is defensible for the data shapes, but any *meaningful* method on those types is untested by construction — nothing would catch one being added.
6. **`traverse.rs` measurement.** 5 tests for parallel size/recency measurement. Symlink handling, `CACHEDIR.TAG` qualification, and per-entry `ReadDir` error attribution are what would go wrong on a real machine, and what the platform matrix would be best placed to catch — but the fakes are hand-rolled per test, so real behaviour is only sampled.
7. **The whole self-update path.** 30 inline tests against `FixtureEnvironment`, plus manual live rehearsal. Every published defect in this line escaped that apparatus, and the reason is structural: a fixture that replaces the network can never discover that the real network rejects you. The automated evidence is a substitute for the thing, not a check of it.
8. **Cross-platform behaviour generally.** `check-fixture-portability.py` closes the ungated-POSIX-fixture hole, and its own docstring (lines 23-32) disclaims checking whether a case's pass condition proves its intended branch. That disclaimer *is* the guarantee's boundary: a green run means "no statically visible platform-shaped fixture is ungated", not "every fixture proved something on every lane". The C013 25-row inventory remains the only real audit of that question, and it is a point-in-time artifact, not a continuous guarantee.

---

## 10. Review checklist

1. **Does a new external-process fixture prove its own premise?** Add a guard that fails when the fake cannot be reached, before the behaviour assertion. References: C012's premise check; `packaging/tests/test_installers.py::self_test` (line 785), which proves each guard rejects a broken setup. Without failing-direction proof, it is a C012 waiting to happen.
2. **Is a new fixture platform-gated?** Any `#[cfg(test)]` block spawning a process, writing a shell script, or using `os.pathsep` needs `#[cfg(unix)]` / `#[cfg(windows)]` discipline or a portable implementation. `scripts/check-fixture-portability.py` catches the static shape, in `ci.yml` `checks`.
3. **Is a new external-process fixture actually exercised — not just present?** The premise is untested by default. Confirm the stub is *reached*: assert on what the subject passed to it (the `RecordingRunner` pattern, `workspace.rs:3224`), not only on what came back. C012 was green because an unrelated failure satisfied the assertion.
4. **Is a new `scripts/` file wired into a workflow?** Two of the current 12 are not (`validate-staged-release.py`, `qualify-cargo-selectors.sh`) — see [§6](#6-ci-as-the-enforcement-point). A new checker no lane runs repeats that gap. Wire it, or document why not, in the same change.
5. **Changing a field in `output.rs`?** There is no in-crate test. The only assertions are external: `cli_contract.rs:200-202`, `238-241`, `270`, `379-381`, `383-395`, `426`, `459-465`, `497`, `532`, `565-567`. Update them in the same commit; nothing inside the crate will remind you.
6. **Changing `update_json` (`main.rs:584`)?** Nothing will fail. It has no test and is not covered by the checklist invariants. Add a case, and consider routing it through `EnvelopeV1` so it stops being the one unversioned surface.
7. **Touching `output.rs:140` or `:206` (`schema_version` literals)?** Confirm the trailing-newline invariant (`cli_contract.rs:204`, `:242`) and stderr-separation invariant (`:203`) still hold for the surface you touched. They are the reason the JSON is safe to parse unattended.
8. **New exit codes in `main.rs`** — add a subprocess assertion in `cli_contract.rs` beside the existing three (`:318` for `1`, `:680` for `17`, `:215` for the scope label). The exit-code table is otherwise unpinned.
9. **Touching `traverse.rs`?** 6 tests (module opens at `:470`), and `source_activity` (`:268`) has no production caller — its only call site is `traverse.rs:593`, inside its own test module. If you are adding a caller, say so; if not, its test proves nothing about the system.
10. **New behaviour in `policy.rs`** — only 5 tests, and this module decides the Routine/Full scope boundary, a documented invariant. Cheap to cover, currently under-covered.
11. **Reactivating or adding a `.crates.toml` provenance path** — C017 was exactly this: a schema no current Cargo writes, misread as self-managed, which *replaced a binary Cargo owned*. The classification lives in `src/update.rs`, and its tests run against `FixtureEnvironment` (`update.rs:1031`), which cannot reproduce real Cargo's on-disk state.
12. **Any change to the two manual-only scripts** (`validate-staged-release.py`, `qualify-cargo-selectors.sh`) — the repository's largest remaining false-green exposure. They are R1 and R2 in [§7](#7-the-false-green-problem); nothing records whether they were ever run, and the C013 lesson is that verifiers fail exactly when they start being useful.

---

## See also

[Architecture overview](overview.md) · [Orchestration](13-orchestration.md) ·
[Cleanup](09-cleanup.md) · [Self-update](12-self-update.md) · [Reporting](10-reporting.md) ·
[Distribution & release](15-distribution-and-release.md) · and `plans/registry.md:92-107`
for the project's own account of the false-green incidents, in its own words.
