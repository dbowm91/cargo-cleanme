# Artifact Discovery and Cleanup M006B Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006b-cleanup-proof-resolution-efficiency.md`

Disposition: **closed**

Implementation commits: `2d2a37f` (`Implement M006 global discovery and proof optimizations`) and `755eb66` (`Make Rustup path fixture portable to Windows`)

Date: 2026-10-03

M006B removes redundant final-proof `cargo locate-project` calls, shares initial/final Cargo metadata decoding, and refreshes the entire ownership universe freshly for every candidate. Independent read-only refreshes run with a fixed cap of four, are fully joined before graph construction, and preserve deterministic results. There is no cross-candidate snapshot reuse and Cargo clean remains sequential.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Known root manifest refresh is direct Cargo metadata | `workspace::tests::direct_known_root_refresh_uses_one_metadata_call_and_no_locate` verifies one metadata call and zero locate calls; `workspace::refresh_workspace_from_root_manifest` uses `--offline --locked --no-deps --format-version 1 --manifest-path KNOWN_ROOT`. | Pass |
| Initial and direct refresh share metadata decoding and path normalization | Direct refresh uses the same metadata decoder/resolved workspace constructor as initial locate-plus-metadata resolution; parser parity and source-root canonicalization fixtures pass. | Pass |
| Root/member/output identity changes fail closed | `direct_metadata_refresh_detects_workspace_root_and_member_changes` checks root/member changes in Preview, Simulate, and Execute; `candidate_target_or_build_change_still_skips` checks target/build changes; malformed and failed metadata regressions remain covered. | Pass |
| Complete C003/C004 universe remains fresh for each candidate | `proof_universes_refreshed == N x N`, `proof_metadata == N x N`, and `proof_locate == 0` on reportable fixtures. `universe_workspace_that_cannot_reresolve_fails_closed` checks a failed universe refresh blocks the candidate. | Pass; no stale cross-candidate reuse |
| Bounded independent refresh and stable results | `proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` checks actual peak, cap, counters, and ordering. `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition` checks disposition and graph parity. Batches are joined before errors are returned; no cancellation or detached subprocesses are used. | Pass |
| C003/C004 destructive proof semantics remain intact | Full regression suite includes changed target/build, shared and symlink overlaps, unresolved participants, activity/marker races, one invocation/one result, Preview/Simulate/Execute equivalence, and Simulate zero-clean coverage. | Pass |
| Measure 1/8/32 workspace and mixed set performance | Matched release `clean ROOT --dryrun --no-progress --stats` runs on generated valid Cargo workspaces with lockfiles, markers, targets, and backdated quiet outputs. Full results follow. | Pass |
| At least 25% 8-workspace improvement | Matched 8-workspace total wall time improved 73.5% (3.961s → 1.048s); the new 1.048s is also 51.5% below the historical C003 ~2.16s result. | Pass |
| Hosted Linux/macOS/Windows and Rust 1.89 | CI run [37103354510](https://github.com/dbowm91/cargo-cleanme/actions/runs/37103354510) passed all four jobs on corrected commit `755eb66`. An earlier run caught and corrected a Windows-only POSIX-path test fixture, not an implementation failure. | Pass |

## Matched release performance results

Each before/after run used baseline commit `47574fa8a087ac2f9111e29821c23b13657e7bbe` and the M006 implementation on the same Darwin x86_64 host and equivalent temporary Cargo fixtures.

| Fixture | Before wall / app elapsed | After wall / app elapsed | Proof refreshes | Before proof locate / metadata | After proof locate / metadata | After peak metadata concurrency |
|---|---:|---:|---:|---:|---:|---:|
| 1 workspace | 0.210s / 0.19s | 0.737s / 0.10s | 1 | 1 / 1 | 0 / 1 | 1 |
| 8 workspaces | 3.961s / 3.95s | 1.048s / 1.03s | 64 | 64 / 64 | 0 / 64 | 4 |
| 32 workspaces | 56.594s / 56.58s | 14.741s / 14.73s | 1,024 | 1,024 / 1,024 | 0 / 1,024 | 4 |
| Mixed, 8 workspaces (4 active, 4 recent/active-skipped) | — | 4.172s / 3.81s | 32 | — | 0 / 32 | 4 |

The 1-workspace external wall measurement includes process-launch/system variance; the application `--stats` elapsed phase improved from 0.19s to 0.10s, and the proof concurrency path remains synchronous for one candidate. The 8- and 32-workspace fixtures showed substantial end-to-end improvements. Proof metadata timing in `--stats` is accumulated call duration and can exceed wall time when calls overlap.

The 8-workspace fixture had eight reportable quiet output trees. It performed 64 complete universe refreshes before and after; the new code removes 64 locate calls without reducing metadata or proof coverage. The mixed fixture retained four reportable workspaces, skipped four recent workspaces, and ran 32 full proof metadata refreshes for the four candidates.

## Verification

- `cargo fmt --check` — passed.
- `cargo test --all-targets --all-features` — passed, 163 tests across 4 suites.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rustup run 1.89.0 cargo test --all-targets --all-features` — passed, 163 tests.
- `rustup run 1.89.0 cargo check --all-targets --all-features` — passed.
- Hosted CI: Linux, macOS, Windows, and Rust 1.89 all passed in run [37103354510](https://github.com/dbowm91/cargo-cleanme/actions/runs/37103354510).
- Implementation and closure/status commits are pushed to `origin/implementation/m006a-m006b-performance`.

## Findings and downstream disposition

- No known correctness or safety finding remains in M006B. C003/C004 freshness, complete-coverage, fail-closed, and sequential-clean invariants remain in force.
- M006A's independent global scan qualification miss remains open under its conditional closure; it is not hidden or changed by M006B.
- M006C is **ready** and unblocked by the measured M006A miss and subtree profile. It does not depend on M006B, and commit `755eb66` is its repository baseline.
- No other future plan was found blocked on M006B. M005A/M005B and earlier milestones are already closed and unchanged.
