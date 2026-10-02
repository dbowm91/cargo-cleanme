# Artifact Discovery and Cleanup M004 Closure

Plan: `plans/implementation/artifact-discovery-cleanup/004-revalidated-cleanup-execution.md`

Disposition: **closed**

Implementation commits: `beadea6c9b3e5f03588b2f0a0c9eb6d947f62293` through `edb6dd294bd04b5be64e4d3578fdb8dd2b9a4fb5` on branch `implementation/artifact-discovery-cleanup`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Cleanup is a separate explicit-root command; dry-run is default and execution requires `--yes` | CLI parser tests cover mandatory root, `--yes`, conflicting switches, and unchanged no-argument scan intent; `main` maps only `--yes` to execute mode | Pass |
| Candidates are freshly revalidated and uncertain/active candidates are skipped | `revalidate_rejects_recent_source_and_missing_target`; shared M003 activity boundary tests cover recent and future timestamps; revalidation is repeated immediately before execution | Pass |
| Only Cargo-owned conventional local targets are eligible | Ownership checks require the Cargo cache marker, one workspace member, workspace-root identity, exact effective target, and no separate build dir; tests cover missing marker, shared workspace, redirected target, build-dir config and environment | Pass |
| Cargo arguments are explicit and shell-free | `SystemRunner` uses `Command::new("cargo").args(...)`; fake-runner assertions check metadata/clean argument separation, `--dry-run`, `--verbose`, manifest and target-dir, and absence of `--dry-run` only in the explicit execution case | Pass |
| Dry-run is non-mutating | Temporary-project integration test runs actual `cargo clean --dry-run --verbose --offline --locked` and verifies unchanged `Cargo.lock`, artifact bytes, and `CACHEDIR.TAG` bytes | Pass |
| Execution measures actual post-command target size and observed decrease | Fake Cargo runner removes one fixture artifact without launching Cargo; test asserts the measured post-size is lower and observed decrease is positive | Pass; simulated process only |
| Per-project failures/results remain isolated; direct deletion is not used in production | Results are accumulated per candidate; production path delegates cleanup only to Cargo and contains no direct deletion API. Fixture-only fake runner removes a temporary artifact to exercise measurement | Pass |
| User documentation and bounded exclusions | README documents dry-run default, `--yes`, explicit root, ownership limits, skipped shared/redirected targets, and observed-size wording | Pass |

## Verification run

- `cargo test clean` — passed (8 matching cleanup/CLI tests in the focused run before the final ownership tests were added).
- `cargo test revalidate` — passed (1 focused test; shared M003 boundary tests also pass in the full suite).
- `cargo test ownership` — passed (4 focused tests, including environment build-dir refusal).
- `cargo test dry_run` — passed (2 focused tests).
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (30 tests in the final local source tree).
- `cargo check --locked` — passed.
- `git diff --check` — passed.
- Hosted Linux/macOS/Windows matrix for final implementation commit `edb6dd2`: [run 37056049686](https://github.com/dbowm91/cargo-cleanme/actions/runs/37056049686). All three jobs passed formatting, clippy, and tests.
- Static source review confirms production cleanup invokes Cargo only; no `remove_dir_all` path exists. `remove_file` occurrences are confined to test fixtures/fake execution.

## Limitations and unresolved findings

- No real mutating cleanup was run against a user project. Execution semantics are covered through a fake runner; the actual Cargo subprocess integration is dry-run only by design.
- Revalidation narrows but cannot eliminate a concurrent activity/configuration race. Cargo remains responsible for its own locking; shared/redirected output remains excluded.
- Cleanup deliberately requires an absolute root, a one-member workspace, the conventional local `target/`, a valid Cargo marker, and no separate build directory.

Severity: no known correctness or security findings within the bounded M004 scope.

## Dependency disposition

M004 is closed. The M005 dependency on M004 behavior/evidence is satisfied for design work. M005 remains deferred for implementation until a shared/redirected-output ownership model and any required ADR are complete; it is not yet an implementation-ready handoff.
