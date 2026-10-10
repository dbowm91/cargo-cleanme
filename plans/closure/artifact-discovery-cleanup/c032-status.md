# C032 Status — Cargo Workspace Resolution Diagnostics and Failure Noise

Plan: `plans/implementation/artifact-discovery-cleanup/c032-cargo-workspace-resolution-diagnostics-and-noise-corrective.md`

Disposition: **conditionally closed; publication pending.** Implementation and
hosted qualification are complete. The corrective remains open until a
published release carries it. C032 does not authorize a release, and no
release or installed-release smoke was performed.

Implementation commits:

- `60ead06` — bounded cause summaries, separate progress counters, real Cargo
  failure matrix, 1,000-failure coverage and unresolved cleanup controls.
- `bff54c2` — platform-tolerant 500 KB structured-report bound, after the first
  hosted run showed a 383,132-byte macOS report.
- `309b7f4` — real process-boundary test for an unavailable Cargo executable.
- `b180cdd` — ten-cause fixture pins the eight-group human cap.

Baseline: v0.2.5 commit `7c874ab7998bdac28a6236ec21ae59ec24b915f8`.
The baseline was built in a detached disposable worktree and run against the
same synthetic tree used for the 1,000-failure regression: 1,000 orphan
manifests shared one missing workspace and one valid sibling resolved. It
returned success with 1,000 JSON diagnostics, 235,132 stdout bytes, 2,245
stderr bytes and 13 stderr lines; stderr showed ten per-manifest Cargo warnings
instead of a cause-count summary. The baseline therefore fails the intended
grouped-summary assertion. The worktree and fixture were removed.

## Requirement-to-evidence map

| Requirement | Evidence |
|---|---|
| Real Cargo failure taxonomy with invocation context | `real_cargo_workspace_failure_matrix_records_stage_and_context` (`tests/cli_contract.rs:361`) probes seven standalone/workspace/inherited/unlisted/orphan/invalid/detached/vendor manifest shapes using actual Cargo `locate-project --workspace --manifest-path` and `metadata --offline --locked --no-deps --format-version 1 --manifest-path`, from each manifest parent. Assertion context includes Cargo version, cwd, argv and stderr. The observed Cargo was `1.99.0 (5f94df478 2026-08-27)`. `unavailable_cargo_executable_is_reported_as_a_locate_spawn_failure` (`:497`) uses an empty PATH and proves a spawn failure remains a partial read-only result with `locate=1`, `metadata=0`, `failures=1`. Resolver unit tests cover metadata non-zero, malformed output, and parse failures (`src/workspace.rs:2430` and nearby failure-path tests). |
| Bounded actionable causes and distinct grouping | `cargo_failure_reason` retains up to three non-empty sanitized lines within its byte bound (`src/workspace.rs:199-230`); its unit cases cover truncation, UTF-8 boundaries, controls and cause chains (`:1752-1845`). `cargo_failures_with_shared_generic_line_keep_distinct_cause_groups` (`tests/cli_contract.rs:550`) gives ten distinct causes the same generic first line, asserts eight visible groups plus the hidden-group count, retains all ten JSON diagnostics, and verifies Cargo stderr stays out of log mode. |
| 1,000 failures preserve report and resolution accounting | `repeated_real_cargo_failures_are_grouped_without_losing_json_diagnostics` (`tests/cli_contract.rs:262`) runs actual Cargo over 1,000 broken manifests and a valid sibling. It asserts all 1,000 JSON diagnostics, `discovered_manifests=1001`, `locate=1001`, `metadata=1`, `failures=1000`, and `workspaces=1`; stderr is at most 4,096 bytes/eight lines and JSON is below 500,000 bytes. On the observed local run Cargo resolution took 12.04 seconds and total elapsed time was 12.30 seconds. Hosted macOS measured the same report at 383,132 bytes, which exposed and corrected the original too-low 300 KB test ceiling. `thousand_failed_lookups_keep_every_unresolved_manifest` (`src/workspace.rs:2393`) preserves all failed participants. |
| Cleanup remains fail-closed | `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` (`tests/cli_contract.rs:1595`) injects a real locate-stage failure for Execute, Simulate and Cargo Preview; each mode reports a scope block and zero Cargo clean calls. |
| Full traversal and learning remain separate from Cargo resolution | `localized_uncertainty_retains_intersecting_roots_and_publishes_positives` (`src/discovery_state.rs:706`) publishes after complete traversal while retaining an unresolved manifest as `ManifestObservedUnresolved` and preserving roots intersecting uncertainty. `unresolved_and_cache_manifests_never_create_routine_roots` (`:503`) keeps Full inventory while excluding unresolved/cache/transient roots from Routine learning. |
| Progress and output contracts | `in_memory_frame_contains_at_most_six_progress_lines` (`src/progress.rs:559`) asserts separate discovery-error and Cargo-failure totals. The two-cause CLI case asserts log mode has no Cargo stderr and remains one ASCII line within `MAX_BYTES`; JSON diagnostics remain per manifest and the v1 schema is unchanged. |
| Operator and architecture documentation | Updated guidance is present in `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `README.md`, `CHANGELOG.md` (Unreleased), and architecture deep dives `07`, `11`, `13`, `14`, plus `architecture/overview.md`. |

## Verification actually run

| Command or run | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed on `60ead06` and `309b7f4`; final hosted stable and MSRV lanes also passed on `b180cdd`. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed locally on `60ead06` and `309b7f4`; final hosted stable and MSRV lanes passed on `b180cdd`. |
| `cargo test --all-targets --all-features` | Passed on `60ead06` (383 tests). On `309b7f4`, one local run had 329 pass and one unrelated updater failure, `a_verified_candidate_replaces_the_live_binary`: candidate spawn returned `Text file busy (os error 26)`. The isolated updater test passed immediately afterward. The final full hosted matrix on `b180cdd` passed; the failure remains recorded and is not reclassified as a pass. |
| Focused 1,000-failure CLI test | Passed after the platform-tolerant bound change (`bff54c2`). |
| Focused unavailable-Cargo CLI test | Passed on `309b7f4`. |
| Focused ten-cause/eight-group CLI test | Passed on `b180cdd`. |
| `python3 scripts/check-fixture-portability.py --self-test` and checker | Passed on `60ead06`; final hosted checks passed on `b180cdd`. |
| `python3 scripts/check-doc-citations.py --self-test` and checker | Passed on `60ead06`; final hosted checks passed on `b180cdd`. |
| `git diff --check 83f4311..b180cdd` | Passed. |
| Hosted CI `38021809640` (push) and `38021812352` (pull request) | Both completed successfully on head `b180cdd`. All nine jobs passed: stable checks on Ubuntu/macOS/Windows, MSRV 1.89, generated docs, benchmark and installer qualification on all three OSes. |
| Hosted runs `38020641452` and `38020643904` | Both failed on head `60ead06` because macOS produced a 383,132-byte JSON report above the initial 300 KB assertion. This failure led to `bff54c2`; it is retained as a failure. |

## Remaining limitation

The corrective has not shipped. Per repository planning conventions it remains
conditionally closed and open until a release containing these changes is
qualified. Publication, release tagging and installed-release smoke are
outside C032 authorization. No machine-readable schema, cleanup eligibility,
or Cargo authority changed.
