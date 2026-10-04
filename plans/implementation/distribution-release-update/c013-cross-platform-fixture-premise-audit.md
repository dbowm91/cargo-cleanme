# C013 — Cross-Platform Fixture Premise Audit

Status: ready

Repository baseline: `c9b0104c5c6480c4056be3cacd11b893b7d52642`

Original findings:

- `plans/closure/distribution-release-update/c012-status.md`
- `plans/closure/artifact-discovery-cleanup/c009-status.md`
- `plans/closure/distribution-release-update/c011-status.md`

Source roadmap: post-Phase-10 distribution/release/update corrective line

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: corrective / test-infrastructure qualification

Hard dependencies: none. Phase 10, C011, and C012 are closed.

Downstream dependency: C014 may prepare non-publication work in parallel, but its v0.1.2 publication step must not proceed until C013 closes.

## 1. Objective

Close the low-severity test-evidence debt left by C012: audit every repository fixture that substitutes or resolves an external executable, PATH entry, shell/editor command, or platform-specific process and prove that the executable the test *intends* to exercise is the executable the subject actually resolves on every platform where that test claims coverage.

This corrective is not a request to make every Unix-only test cross-platform. A deliberately platform-scoped fixture may remain scoped when the scope is explicit and truthful.

The purpose is narrower: remove false-green coverage caused by an invalid fixture premise.

## 2. Current implementation evidence

Three Phase 10 findings establish the failure class:

- C009 found a POSIX test-fixture assumption that broke hosted Windows qualification.
- C011 found a fixture transport that always answered while the production transport could not reach its live version authority; all fixture tests were green.
- C012 found a Windows installer case whose fake `cargo` was not invocable by PowerShell and whose PATH was joined with the POSIX separator. The case therefore used the real Cargo and stayed green because an unrelated command happened to fail.

The current tree already contains several examples of the correct treatment:

- `packaging/tests/test_installers.py` now uses `cargo.cmd` on Windows, `os.pathsep`, and a premise test that probes `Get-Command cargo` / `command -v cargo`.
- `tests/cli_contract.rs::json_unattended_yes_executes_through_cargo_and_emits_typed_result` uses a POSIX shell Cargo stub but is explicitly `#[cfg(unix)]`; that is truthful platform scoping, not a defect.
- the config-editor fixture supplies distinct Unix shell and Windows PowerShell implementations.

No current production defect is asserted by C013. The open requirement is evidence quality: the rest of the repository has not been mechanically reviewed for the same class.

## 3. Invariants

- Do not weaken, skip, or delete a test merely to make a hosted lane green.
- A fixture that substitutes an executable must either:
  - prove the subject resolves that exact fixture; or
  - invoke the fixture by an exact path that bypasses discovery.
- PATH construction must use platform-correct APIs (`std::env::join_paths`, `std::env::split_paths`, `os.pathsep`, or an equivalent), never an unqualified literal separator in cross-platform code.
- Windows-executed command stubs must be resolvable under Windows executable lookup semantics. An extensionless POSIX shebang file is not a Windows executable fixture.
- A fixture's pass condition must prove the intended behavior/branch, not merely observe a nonzero exit that could come from an unrelated failure.
- Platform-scoped tests may remain scoped when the unsupported platform is explicit in the source and closure evidence.
- Production cleanup, updater, installer, release, and configuration semantics are out of scope unless the audit exposes a real product defect; such a defect requires a separate corrective plan.

## 4. Scope

Audit at minimum:

- `tests/**`;
- test modules under `src/**`;
- `packaging/tests/**`;
- fixture-oriented scripts under `scripts/**`;
- maintenance generators/tests under `xtask/**` where they invoke external commands;
- CI/release smoke helpers that prepend PATH or substitute tools.

Search mechanically for:

- `Command::new`, `std::process::Command`, `subprocess`, `Popen`, shell invocation, and PowerShell invocation;
- `PATH`, `PATHEXT`, `VISUAL`, `EDITOR`, `CARGO`, `RUSTC`, `curl`, `git`, `pwsh`, and other executable-discovery inputs;
- generated fake/stub/shim binaries and scripts;
- literal `":"` / `";"` PATH assembly;
- `#!/bin/sh` or other POSIX-only executable fixtures in code that claims Windows coverage;
- tests whose success condition is only "command failed" without asserting which command/branch failed.

The closure record must include an inventory of every external-process fixture found and its disposition.

## 5. Ordered work packages

### A. Build the external-process fixture inventory

Produce a compact table with:

- file/test name;
- substituted or discovered executable;
- platforms on which the case runs;
- how executable resolution occurs;
- whether the intended executable is asserted;
- whether the expected branch/effect is asserted;
- disposition: correct, corrected, explicitly platform-scoped, or separate product finding.

Do not treat a zero search result from one code-search mechanism as proof. Inspect the repository tree and the actual test/support files.

### B. Correct platform-resolution defects

For any fixture that claims Windows coverage but supplies a POSIX-only stub:

- provide a Windows-resolvable shim/executable; or
- narrow the test to its truthful platform scope if the behavior is genuinely Unix-only.

Use platform APIs for PATH construction.

Do not depend on the developer machine's real Cargo, Git, editor, curl, or other executable unless the test is explicitly an integration test of that real dependency.

### C. Add premise assertions where substitution matters

For every fixture whose correctness depends on tool discovery, assert the discovery premise before the behavior assertion.

Examples:

- resolve `cargo` exactly as the subject will and confirm it is inside the fixture directory;
- log a unique invocation token from the stub and assert that token was emitted;
- assert the candidate process actually ran before accepting its exit status.

Prefer proof through the subject-observable effect over merely inspecting the fixture setup.

### D. Centralize helpers where it reduces recurrence

If Rust integration tests duplicate PATH/stub construction, add a small cross-platform helper under `tests/common/`.

If Python packaging tests need a similar helper, keep it local to the Python suite rather than forcing a cross-language abstraction.

Do not refactor unrelated test infrastructure.

### E. Add a bounded static guard only if it is precise

A repository script such as `scripts/check-fixture-portability.py` is justified only if it can identify the known unsafe patterns without generating routine false positives.

Useful checks may include:

- literal PATH concatenation in cross-platform fixture code;
- an extensionless shell stub used in a Windows-executed fixture;
- a substituted tool with no premise/effect assertion.

If a reliable static guard is not practical, record that decision and rely on the reviewed inventory plus runtime premise assertions. Do not add a brittle grep gate merely to satisfy the plan.

## 6. Failure semantics

If the audit finds a product defect rather than a fixture defect:

1. do not repair it silently under C013;
2. classify severity;
3. author/register a separate corrective;
4. keep C013 focused on truthful evidence.

If a hosted platform cannot execute a test for environmental reasons, record the exact limitation. Do not convert the case to a pass.

## 7. Compatibility/configuration effects

Expected production compatibility impact: none.

Changes should be limited to tests, test helpers, CI/test scripts, and planning documentation unless the stop condition above triggers a separate product corrective.

## 8. Required tests

At minimum:

- all existing integration/unit tests;
- installer fixtures on Linux/macOS/Windows;
- any newly corrected external-tool fixture on every platform it claims;
- explicit premise assertions that fail when the stub is intentionally made unresolvable or PATH precedence is reversed.

Where a fixture remains `#[cfg(unix)]` or otherwise platform-scoped, add or retain source-level evidence that the scope is intentional.

## 9. Verification commands

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 packaging/tests/test_installers.py
bash scripts/release-check.sh
~~~

Run hosted CI on Linux, macOS, and Windows and record the exact run IDs. If C013 adds a static fixture-portability checker, include it in normal CI and `release-check.sh`.

## 10. Documentation updates

Update only the planning/closure documentation necessary to record the audit and any reusable fixture rule.

If a durable contributor rule is added, keep it concise: "a substituted executable is not evidence until the test proves the subject resolves/invokes it."

Do not add user-facing README material for a test-only corrective.

## 11. Acceptance criteria

C013 closes only when:

- the external-process fixture inventory is complete for the repository baseline;
- every fixture claiming Windows coverage uses Windows-resolvable process semantics;
- every cross-platform PATH mutation uses platform-correct construction;
- fixtures that depend on executable substitution prove the resolution/invocation premise or use an exact executable path;
- tests do not pass solely because an unrelated real executable failed;
- intentionally platform-scoped fixtures are explicitly scoped and documented;
- hosted Linux/macOS/Windows CI is green;
- no medium-or-higher fixture-evidence finding remains;
- any product defect discovered during the audit has been separated into its own registered corrective.

## 12. Stop conditions

Stop and write a separate corrective if:

- an audit case proves production behavior is wrong;
- making a fixture truthful requires changing product semantics;
- a supposedly cross-platform test cannot express the same requirement on a supported platform;
- the proposed static guard is too noisy to be a reliable CI gate.

Do not weaken assertions or broaden environment access to get a green result.

## 13. Closure evidence

Record:

- implementation commit;
- complete external-process fixture inventory/disposition table;
- before/after evidence for every corrected fixture;
- premise-negative tests showing the new guards actually fail when setup is broken;
- local verification commands/results;
- hosted Linux/macOS/Windows run IDs;
- any deliberately platform-scoped cases and why;
- unresolved findings by severity;
- disposition.

C013 does not reopen Phase 10. It is a post-release evidence-quality corrective and a publication precondition for C014.
