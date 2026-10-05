# M012A — Canonical Maintenance CLI and Dry-Run Semantics

Status: closed — see `plans/closure/artifact-discovery-cleanup/m012a-status.md`

Repository baseline: `e4e9d92673e5f10548248e23db788c0906760916`

Source roadmap: Phase 12 — canonical maintenance UX and unattended operation

Subsystem roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Decision: `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Primary class: breaking CLI contract / orchestration

Dependencies: C003, C004, C006, M007, M008A-M008D closed. M012B depends on this plan's resolved operation/mode model but may develop output DTO/rendering in parallel once that model is fixed.

## 1. Objective

Make the mature maintenance behavior the shortest supported invocation while preserving the existing cleanup safety proof unchanged.

Target operator surface:

~~~text
cargo cleanme                         # Routine Execute
cargo cleanme --dry-run               # Routine Simulate, zero cargo clean

cargo cleanme scan                    # Full read-only reconciliation
cargo cleanme scan ROOT               # Explicit read-only scan
cargo cleanme scan --known            # Routine read-only inventory

cargo cleanme clean ROOT              # advanced Explicit Execute
cargo cleanme clean --known           # advanced Routine Execute
cargo cleanme clean --full            # advanced Full-reconcile-then-Execute
cargo cleanme clean ... --dry-run     # application simulation
cargo cleanme clean ... --cargo-preview
~~~

The implementation must not weaken ownership, authorization, activity, marker, selector, or final-proof semantics to make the simpler surface possible.

## 2. Non-goals

- no internal scheduler, daemon, cron parser, load threshold, retry queue, or history database;
- no new cleanup ownership class;
- no direct filesystem deletion;
- no change to learned-state authority;
- no widening of selector-qualified Cargo versions;
- no change to `config.toml` merely to remember invocation mode;
- no automatic update/install behavior;
- no literal `cargo scan` companion binary.

## 3. Current implementation evidence

At the baseline:

- `Cli.command: Option<Command>` maps `None` to `run_scan(None, false, ...)`, so bare invocation is Routine read-only;
- `scan` takes optional ROOT plus `--full`;
- `clean` requires ROOT, `--known`, or `--full` at runtime;
- cleanup defaults to Cargo Preview, `--dry-run` is explicit Cargo Preview, `--dryrun` is zero-spawn Simulate, and `--yes` is Execute;
- M007/C006 already provide combined-root Routine/Full cleanup orchestration without trusting learned state for ownership;
- M008B already supplies typed JSON and exit semantics;
- generated completions/manpage are derived from Clap and CI checks them for drift.

The product mechanics needed for Routine Execute already exist. The work is primarily command-intent composition, mode reconciliation, compatibility, tests, and documentation.

## 4. Invariants

- Bare maintenance selects roots using the same non-Full maintenance policy currently used by `clean --known`: a configured legacy `scan.root` remains an exclusive Explicit override; otherwise bounded seed + learned roots form Routine scope.
- The selected roots are canonicalized/deduplicated/collapsed exactly as the existing combined-root cleanup path requires.
- Learned roots are search hints only; complete manifest coverage and fresh ownership proof are rebuilt before cleanup.
- Unresolved ownership blocks all cleanup commands in Execute, Simulate, and Cargo Preview.
- Simulation invokes no `cargo clean` process.
- Cargo Preview may invoke `cargo clean --dry-run --verbose` only after the same final proof.
- Execute invokes real `cargo clean` only after the same final proof.
- Full scan remains read-only and is the only negative-evidence state-pruning authority.
- Explicit scan ROOT continues to bypass configured discovery ignore/unignore filters exactly as today.
- Existing exit-code meanings remain unless a separately justified typed contract change is recorded.
- JSON consumers must not have to infer the new operation/mode from argv spelling.

## 5. Work package A — Make operation intent explicit in orchestration

Refactor the CLI/orchestration boundary so "no subcommand" is an explicit Routine cleanup intent rather than a special fallthrough to `run_scan`.

Prefer one internal operation/mode representation shared by:

- bare Routine maintenance;
- `clean ROOT`;
- `clean --known`;
- `clean --full`.

Do not duplicate cleanup orchestration into a second code path for bare invocation. The root command must reach the same combined-root engine, policy resolution, progress, reporting, and exit classification as advanced cleanup.

The implementation should make it difficult for future changes to give bare cleanup weaker proof than `clean --known`.

## 6. Work package B — Reconcile cleanup modes

Define canonical modes:

- Execute — default for cleanup intents;
- Simulate — `--dry-run`, zero `cargo clean` processes;
- CargoPreview — `--cargo-preview`, invokes Cargo dry-run only after proof.

Update CLI help, internal names, report terminology, and tests so the semantic distinction is explicit.

Migration compatibility:

- historical `--dryrun` may remain accepted as a hidden alias for Simulate;
- historical `--yes` may remain accepted as a hidden alias for Execute;
- aliases must not create a fourth mode or different safety path;
- do not print deprecation chatter during normal unattended execution merely because a hidden compatibility alias was used; migration belongs in docs/release notes.

If retaining an alias creates ambiguous Clap conflicts or materially complicates the parser, stop and record the compatibility tradeoff rather than implementing surprising precedence.

## 7. Work package C — Reframe scan intent

Change read-only scan selection to:

- no ROOT and no `--known` => Full;
- ROOT => Explicit;
- `--known` => Routine;
- legacy `--full` may remain a hidden compatibility alias for no-root Full.

Reject incompatible combinations deterministically before traversal:

- ROOT + `--known`;
- ROOT + compatibility `--full`;
- `--known` + `--full`.

A configured legacy `scan.root` remains authoritative for bare maintenance and `scan --known`, where it resolves the scope as Explicit. Rootless `scan` MUST force Full and ignore configured `scan.root`; configuration may not silently narrow the canonical Full reconciliation command. `scan ROOT` remains the explicit CLI override. Preserve the existing rule that an explicit effective scope bypasses discovery ignore/unignore filters.

## 8. Work package D — Preserve advanced cleanup scope and policy

`clean ROOT`, `--known`, and `--full` remain mutually exclusive scope selectors.

All existing policy flags remain advanced-clean options:

- `--min-reclaimable-bytes`;
- `--older-than`;
- repeatable `--include` / `--exclude`;
- `--profile`;
- `--package`.

Bare maintenance intentionally uses configured/default policy rather than cloning the entire advanced option surface onto the root command. If evidence shows one policy override is essential at the root, stop and write a follow-up rather than gradually duplicating `clean` flags.

## 9. Work package E — Machine-readable operation/mode contract

Review `output.rs` and `plans/output-schema-v1.md`.

Required behavior:

- bare maintenance emits operation=`clean`, mode=`execute` or `simulate`, and scope derived from the resolved maintenance policy (`routine` normally, `explicit` when legacy configured `scan.root` wins);
- rootless `scan` emits operation=`scan`, scope=`full` regardless of configured `scan.root`;
- `scan --known` emits scope=`routine` without a configured root and `explicit` when configured `scan.root` wins;
- Explicit ROOT continues to emit scope=`explicit`;
- `--cargo-preview` maps to the existing preview/result semantics without reusing the word dry-run in a way that conflates simulation.

If schema-v1 can express these values without changing field meanings, keep schema_version=1. If a field's meaning must change incompatibly, stop and create an output-schema migration plan rather than silently redefining v1.

## 10. Work package F — Breaking-release migration boundary

This line changes bare invocation from read-only to destructive. Treat that as a release-level safety concern.

Required:

- the first release containing it must not be a `0.1.x` patch;
- CHANGELOG must call out the exact old/new behavior near the top;
- README quickstart must lead with bare maintenance and `--dry-run`;
- docs/USAGE.md must include a migration table from old spellings;
- docs/TROUBLESHOOTING.md should explain unexpected behavior after upgrade;
- generated manpage/completions must be regenerated, never hand-edited;
- release notes/checklist must require human confirmation that the breaking invocation change is visible before publication.

Do not reserve an exact future version in the implementation plan; only enforce a minor pre-1.0 boundary or later.

## 11. Required tests

At minimum add/adjust tests proving:

1. bare invocation reaches Routine cleanup Execute, not scan;
2. bare `--dry-run` reaches Routine Simulate and spawns zero `cargo clean`;
3. bare cleanup with no known roots is a successful no-op report, not an error;
4. bare cleanup with unresolved ownership blocks and runs zero Cargo clean;
5. `scan` with no ROOT resolves Full even when learned state exists;
6. `scan ROOT` remains Explicit;
7. `scan --known` resolves Routine without configured `scan.root` and Explicit with it;
8. rootless `scan` remains Full even when `scan.root` is configured;
9. scan scope conflicts fail at invocation time;
10. `clean ROOT` defaults Execute;
11. `clean ROOT --dry-run` simulates with zero Cargo clean;
12. `clean ROOT --cargo-preview` invokes Cargo dry-run but never real cleanup;
13. hidden compatibility aliases, if retained, map exactly to canonical modes;
14. direct and Cargo external-subcommand argv remain equivalent;
15. JSON operation/scope/mode fields match the resolved intent;
16. selector and minimum-size fail-closed behavior is unchanged;
17. current ownership/atomicity/final-proof regressions remain green;
18. a premise-negative test demonstrably fails against the old bare-read-only dispatch.

Cross-platform fixture premises must be explicit. Do not claim Windows execution for a POSIX shell stub.

## 12. Verification

Run the repository ladder in order:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs
cargo run --quiet --features dev-tools --bin generate-docs -- --check
~~~

Run focused CLI-contract tests first during development. Run the full release check only when its prerequisites are available and record exactly what ran.

## 13. Documentation and architecture reconciliation

Update after production behavior lands:

- README.md;
- docs/USAGE.md;
- docs/TROUBLESHOOTING.md;
- CHANGELOG.md;
- architecture/02-cli.md;
- architecture/04-policy-and-scope.md;
- architecture/09-cleanup.md;
- architecture/10-reporting.md as mode terminology changes;
- architecture/13-orchestration.md;
- architecture/14-testing-and-verification.md;
- AGENTS.md §7 so it no longer teaches the superseded `--dryrun`/`--yes` front door;
- generated man/completions.

Re-derive architecture source-line citations from the post-change source; do not arithmetically shift old line numbers.

## 14. Acceptance criteria

M012A closes when:

- bare `cargo cleanme` is Routine Execute through the existing combined-root safety engine;
- bare `--dry-run` is zero-Cargo-clean simulation;
- advanced cleanup defaults Execute and exposes Cargo preview explicitly;
- no-root `scan` is Full reconciliation and `scan --known` preserves Routine read-only inventory;
- JSON describes resolved operation/scope/mode correctly;
- all old ownership/freshness/selector safety regressions remain green;
- the breaking migration is documented and generated CLI artifacts agree;
- no new scheduler/daemon behavior was added;
- closure records the exact compatibility aliases retained or removed.

## 15. Stop conditions

Stop and write a follow-up plan if implementation would require:

- weakening complete ownership coverage for Routine cleanup;
- treating learned state as ownership or authorization evidence;
- changing the meaning of an existing JSON schema-v1 field incompatibly;
- adding an implicit shell;
- adding broad root-level policy flags until the root command duplicates `clean`;
- changing selector qualification;
- shipping the destructive default under a patch-level `0.1.x` release.

## 16. Closure evidence

Record:

- implementation commit/PR;
- old-to-new CLI matrix;
- exact alias/deprecation disposition;
- fake/real Cargo spawn evidence distinguishing Execute/Simulate/CargoPreview;
- JSON operation/scope/mode evidence;
- focused premise-negative regression result;
- full verification ladder results;
- generated-doc drift check;
- hosted platform run IDs if available;
- unresolved findings and severity;
- disposition.
