# Corrective C004 Closure — Complete Ownership-Universe Resolution Coverage

Plan: `plans/implementation/artifact-discovery-cleanup/c004-complete-ownership-resolution-coverage.md`

Disposition: **conditionally closed pending hosted CI**

Repository baseline: `819f71cd50bebb3b68ae54ef22e6951f08edcbbe`

Implementation commit: pending push.

## Requirement-to-evidence mapping

| C004 requirement | Evidence |
|---|---|
| Retain coverage for each discovered cleanup-scope manifest | `ResolutionCoverage` returns resolved workspaces, discovered count, and canonicalized unresolved participants; `resolve_workspaces` remains the partial-result wrapper for read-only scans. |
| Clear a failed direct attempt only through authoritative later metadata | `workspace::tests::failed_member_first_is_cleared_by_later_authoritative_workspace_metadata` makes the member locate fail, then root metadata lists it; unresolved count is zero while the original warning remains. |
| Retain locate, metadata, and malformed-metadata failures as unresolved | `unresolved_discovered_manifest_remains_in_coverage_result` and `metadata_failure_or_malformed_metadata_remains_unresolved` check stages and participant identity. |
| Block Preview, Simulate, and Execute scope-wide before sizing/proof | `cleanup::tests::unresolved_discovered_manifest_blocks_every_cleanup_mode_before_clean` checks all three modes, zero results/bytes, retained participant and summary, `unresolved_ownership=1`, and zero Cargo clean calls. `resolve_cleanup_scope` returns before output analysis when coverage is incomplete. |
| Preserve C003 final ownership refresh once initial coverage is complete | Existing C003 regression tests remain in the suite, including final-universe re-resolution, overlap into target/build, symlink/unresolvable overlap, and one-invocation/one-result tests. The existing `final_cleanup_proof` path is unchanged. |
| Keep scan partial-result tolerant | `workspace::tests::failed_first_locate_does_not_poison_unrelated_workspace` and `workspace::tests::malformed_and_failure_isolation` exercise the legacy `resolve_workspaces` wrapper; ordinary scan still uses that API. |
| Expose ownership incompleteness in stats | Cleanup counters include `unresolved_ownership`; the all-mode regression asserts `stats_line()` includes `unresolved_ownership=1`. |
| No direct recursive output deletion | Static review of production `src/*.rs` finds no recursive deletion path; cleanup remains Cargo-mediated. Config initialization's temporary-file cleanup is separate. |

## Manifest coverage matrix

| Discovered input | Direct attempt | Authoritative metadata coverage | Result |
|---|---|---|---|
| Root manifest resolves | locate + metadata succeed | Root/member manifest listed | Covered |
| Member discovered before root; member locate fails | locate fails | Later root metadata lists member | Covered; failure diagnostic retained |
| Locate fails with no later covering metadata | locate fails | None | Unresolved participant |
| Locate succeeds, metadata fails | locate succeeds | None | Unresolved participant |
| Metadata output is malformed | locate succeeds | None | Unresolved participant |
| Two otherwise independent projects, one failure | One workspace succeeds | Only successful workspace covered | Failed manifest remains unresolved; cleanup scope blocks |

All comparisons use the canonical/absolute manifest key shared with the existing member cache. No path ancestry or parsed TOML inference is used.

## Verification actually run

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed, 153 tests across four suites.
- `cargo check --locked` — passed.
- `rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rustup run 1.89 cargo test --locked --all-targets` — passed, including 150 unit tests, 2 CLI contract tests, and 1 end-to-end test.
- `git diff --check` — passed.

The repository CI workflow also requires stable Linux/macOS/Windows and Rust 1.89 CI. Those hosted results are pending the implementation push; final disposition will be updated from the workflow run.

## Unresolved findings and dependency disposition

- No C004 finding remains open in local source/tests.
- Hosted platform/MSRV qualification remains an operational closure dependency while CI runs.
- C003's measured O(N²) final-proof refresh cost, macOS source-root canonicalization inefficiency, and generic no-argument scan performance remain deferred candidate work, not blocked plans.
- M005 destructive release qualification remains reopened until hosted C004 qualification closes. On closure it can return to the roadmap's release-qualified state. No other ready/active/blocked implementation plan depends on C004.
- Disposition: **conditionally closed pending hosted CI**.
