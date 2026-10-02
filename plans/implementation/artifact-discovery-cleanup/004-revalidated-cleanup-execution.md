# Artifact Discovery and Cleanup Milestone 004 — Revalidated Cleanup Execution

Status: closing

Repository baseline: 8f05de958cf0616c07e13f0f1bf3ba94cc7c06ed

Source roadmap: plans/subsystems/artifact-discovery-cleanup-roadmap.md#9-milestone-m004--revalidated-cleanup-execution

Canonical requirements:

- plans/000-long-term-specification.md#16-cleanup-capability-requirements
- plans/001-terminology-and-domain-model.md#22-cleanup-candidate
- plans/003-planning-process.md

Hard dependencies:

- M001, M002, M003 closed.
- M003 real-tree report reviewed by the user on 2026-10-02; user authorized M004 continuation.

Primary class: capability / destructive invariant

## 1. Objective

Add an explicit, bounded `clean` operation that revalidates an eligible conventional Cargo target immediately before acting, invokes Cargo against only that verified target, isolates per-project failures, and measures post-clean disk state. Normal scans remain read-only.

## 2. Current implementation evidence

- `cargo-cleanme scan` discovers and analyzes conventional direct-child `target/` directories.
- `EligibleProject` has project root and target path/size, but is only inventory evidence and must not be trusted by cleanup without revalidation.
- No deletion or Cargo-clean invocation exists in production.
- Cargo supports `clean --dry-run`, verbose dry-run output, `--manifest-path`, `--package`, and `--target-dir`.
- Cargo config is hierarchical; `CARGO_TARGET_DIR` and `build.target-dir` can redirect output. `build.build-dir` can be separate and Cargo clean may clean both target and build directories.
- Cargo clean without a package selector cleans the workspace target directory; shared workspaces are therefore out of scope for this milestone.

References:

- https://doc.rust-lang.org/cargo/commands/cargo-clean.html
- https://doc.rust-lang.org/cargo/reference/config.html#buildtarget-dir
- https://doc.rust-lang.org/cargo/reference/config.html#buildbuild-dir
- https://doc.rust-lang.org/cargo/reference/build-cache.html

## 3. Safety invariants

- No-argument invocation and `scan` remain read-only.
- Cleanup requires the explicit `clean ROOT` command and an explicit `--yes` to execute. Without `--yes`, behavior is dry-run only.
- `clean ROOT` requires an absolute, existing, real directory root; user ignore/unignore filters are bypassed because the root is the sandbox.
- Before every candidate operation, capture a fresh clock and rerun source/target activity analysis. A recent/future mtime, uncertainty, missing candidate, or changed target path skips that project.
- The project root, manifest, and direct `target/` must remain real, non-symlink filesystem objects. The target must have Cargo's `CACHEDIR.TAG` ownership marker.
- Cargo metadata must show that its effective target directory is exactly the candidate target, the workspace root is the candidate project root, and the workspace has exactly one member. Otherwise refuse that candidate.
- Any configured or environment-provided separate build directory makes the candidate ineligible. Redirected/shared Cargo outputs remain for M005.
- Paths are passed as separate `OsStr` arguments to `Command`; no shell is used.
- Use Cargo's package/workspace semantics. Do not introduce `remove_dir_all`, `remove_file`, trash, or custom recursive deletion.
- Candidate-specific preflight, process, or measurement failures do not stop unrelated candidates.
- Summaries distinguish previewed, cleaned, skipped, failed, pre-clean estimate, and observed post-clean size decrease. Never call an estimate “recovered” space.

## 4. Scope

### In scope

- `cargo-cleanme clean ROOT [--dry-run | --yes]`; dry-run is the default.
- Fresh per-candidate revalidation.
- Offline Cargo metadata preflight and ownership checks.
- Cargo clean dry-run preview.
- Explicit `--yes` execution, one candidate at a time.
- Post-clean target measurement and isolated results.
- Structured clean result/status types and summary output.
- Tests with temporary projects and a fake Cargo runner; no test may execute destructive Cargo clean.

### Out of scope

- Global-scope cleanup; explicit root is mandatory.
- Shared/multi-member workspace cleanup.
- Redirected target-dir or separate build-dir cleanup.
- Selective profiles/packages or external/shared output.
- Background cleanup, automatic scheduling, or implicit execution after scan.
- Cleanup of Cargo registry, Git caches, or non-Cargo directories.

## 5. Ordered work packages

### A — CLI and result model

Add explicit `clean ROOT`, default dry-run, `--yes` execution override, mutually exclusive flags, and per-candidate outcome types. Parser tests prove no-argument behavior is unchanged.

### B — Candidate revalidation and Cargo ownership preflight

Re-run discovery and analysis for each candidate with a fresh scan clock. Check `CACHEDIR.TAG`, canonical direct target identity, Cargo metadata workspace identity/member count/effective target directory, and absence of separate build-dir configuration/environment.

Metadata invocation uses `cargo metadata --offline --locked --no-deps --format-version 1 --manifest-path PATH`; parse package/workspace identity with serde_json. Refuse non-UTF-8 paths if metadata cannot prove identity.

### C — Dry-run and execution process adapter

Build Cargo argument vectors without a shell. Preview mode invokes `cargo clean --dry-run --offline --locked --manifest-path PATH --target-dir TARGET`. Execute mode is gated by `--yes`, repeats revalidation immediately before the real Cargo command, and invokes `cargo clean --offline --locked --manifest-path PATH --target-dir TARGET`.

Capture stdout/stderr and exit code per candidate. Continue to later candidates after a candidate-specific failure.

### D — Post-clean measurement and summary

Measure the target after a successful Cargo exit. Missing target means zero remaining bytes. Report the observed decrease as `max(pre_size - post_size, 0)` and report remaining size separately. If measurement fails, the clean remains successful but its disk delta is unknown and a diagnostic is recorded.

### E — User documentation and qualification

Document scope/ownership exclusions, default dry-run, explicit `--yes`, Cargo subprocess behavior, and observed-size semantics. Add no destructive test invocation. Run the full OS CI matrix.

## 6. Failure, cancellation, restart, and contention

- Metadata/ownership uncertainty skips the candidate; it never falls back to direct deletion.
- Nonzero Cargo exit marks only that project failed and processing continues.
- If the process is interrupted, stop accepting new candidates; the current Cargo child receives console interruption where the OS provides it. Report only completed per-candidate results; do not claim a complete batch.
- There is no persistent state. Restart performs a new discovery and fresh revalidation.
- The recency recheck minimizes but cannot eliminate a concurrent source/build race between check and Cargo invocation. Cargo remains responsible for its internal target-dir coordination. Record this as a known limitation; do not claim filesystem snapshot or process exclusion.

## 7. Compatibility

- Existing config schema is unchanged.
- Existing scan output/exit behavior is unchanged.
- Cleanup is opt-in and requires explicit scope and execution acknowledgement.
- Cargo is resolved from `PATH`; failure to start Cargo is isolated to that candidate.

## 8. Required tests

- CLI `clean ROOT` parsing; dry-run default; `--yes`; conflicting flags rejected; no-arg remains scan.
- Revalidation skips recent source, recent target, future timestamps, vanished target, symlink target, and changed target.
- Ownership checks reject absent `CACHEDIR.TAG`, external target directory, multi-member workspace, non-root workspace member, separate `build.build-dir`, and environment build-dir override.
- Argument construction preserves spaces/non-UTF-8 `OsString` semantics where metadata permits and never uses a shell.
- Fake-runner tests assert dry-run never invokes a mutating command and execution invokes only Cargo clean with the verified manifest/target.
- Per-project failure isolation and partial summary accounting.
- Post-clean size decrease, unchanged/increased size, missing target, and measurement failure.
- End-to-end preview against a temporary one-package fixture using Cargo's documented `--dry-run` option; assert the target remains present and unchanged.
- No test invokes Cargo clean without `--dry-run`.

## 9. Verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test clean
cargo test revalidate
cargo test ownership
cargo test dry_run
cargo check --locked
~~~

## 10. Acceptance criteria

- Scan remains read-only.
- Clean requires explicit root and explicit `--yes` for any mutating Cargo invocation.
- Every execution candidate is freshly revalidated immediately beforehand.
- Only a single-member workspace whose resolved target exactly matches its real local conventional target and has no separate build-dir can be cleaned.
- Cargo is invoked with exact manifest/target arguments and no shell; no direct deletion implementation exists.
- Dry-run previews are non-mutating.
- Candidate failures are isolated and post-clean deltas are labeled as observed estimates.
- Linux/macOS/Windows CI passes.

## 11. Stop conditions

Stop and leave the candidate skipped if Cargo metadata cannot establish package/workspace/target identity, Cargo clean would touch a separate build directory, the target marker is missing, any symlink or path relationship is ambiguous, or cleanup requires shared/multi-member/redirected support.

## 12. Closure evidence required

Record implementation commit/PR; CLI confirmation evidence; fresh-revalidation fixtures; target/workspace/build-dir ownership cases; exact Cargo arguments; proof dry-run did not mutate; fake-runner evidence that tests never execute destructive clean; post-clean size evidence on all CI platforms; full CI matrix; and static search confirming no direct recursive deletion path.
