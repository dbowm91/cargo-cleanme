# Corrective C007 Closure — Discovery-State Recovery and Post-M007 Planning Cleanup

Plan: `plans/implementation/artifact-discovery-cleanup/c007-discovery-state-recovery-and-post-m007-planning-cleanup.md`

Disposition: **closed**

Implementation commits: `1dfab70` (typed state recovery and planning cleanup), `9c54ff3` (Routine fallback regression coverage).

Date: 2026-10-03

## Requirement-to-evidence mapping

| C007 requirement | Evidence |
|---|---|
| Typed state-load classification | `discovery_state::StateLoad` distinguishes missing, loaded/migrated, recoverable-invalid, unsupported-newer, and unavailable outcomes. `main` and Routine policy branch on variants; overwrite safety never depends on parsing rendered errors. |
| Recoverable-invalid Full recovery | `full_reconciliation_prior` selects empty prior state only for missing or recoverable-invalid data. Full reconciliation publishes after complete traversal; successful replacement emits a concise diagnostic. Unit fixtures recover schema-0 and malformed JSON and preserve their exact bytes when traversal is incomplete. |
| Preserve newer and unreadable state | Newer schema versions are identified before current-schema decoding. Fixtures assert exact-byte preservation and prohibit publication for newer state; an I/O failure fixture classifies a directory path as unavailable and excludes it from publication. |
| Preserve migration and atomic publication | v1 state reconciles and publishes as current schema. Existing collision and replacement-failure fixtures prove atomic replacement behavior and prior-path preservation. |
| Routine fail-soft behavior | A focused policy fixture verifies recoverable-invalid and newer state both return only existing seed roots and one bounded warning. |
| User-facing recovery documentation | README describes machine-local/self-managed state, Routine fallback, successful Full repair, incomplete-scan preservation, newer-schema protection, and manual deletion as non-normal recovery. |
| Post-M007 planning reconciliation | Registry and roadmaps show M006 and M007 closed, C007 closed, no active implementation handoff, and Phase 9 as future research/planning. M006A/C/D remain historical conditional closure records, not blockers. |
| Branch hygiene | No open PRs were found. Merged local/remote implementation and planning branches were retired. The unique M006E scope/latency branch was intentionally superseded by ADR 002; its plan and closure evidence remain on `main`, and it was deleted without merging. |

## Verification

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets` — passed, 184 tests across five suites.
- `rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rustup run 1.89 cargo test --locked --all-targets` — passed, 184 tests across five suites.
- `git diff --check` — passed.
- Hosted CI run [37152008602](https://github.com/dbowm91/cargo-cleanme/actions/runs/37152008602) — passed on Ubuntu, macOS, Windows, and Rust 1.89, including formatting, Clippy, all-feature tests, and MSRV check/test.
- Documentation status audit — no active M007/current-milestone contradiction or ready C007 handoff remains; stale wording inside C007 is retained only as the plan's audit checklist and historical trigger.

## Platform and fixture evidence

State fixtures cover absent, v1, current, schema 0, malformed JSON, structurally invalid current-schema data, maximal newer schema, incomplete Full preservation, successful Full recovery, unavailable I/O, and failed atomic replacement. Routine fixtures cover invalid/newer fallback. Hosted CI exercises the implementation on Linux, macOS, Windows, and Rust 1.89.

The host-wide `scan --full` command was not repeated as a recovery fixture because it traverses platform discovery roots; the decision and publication gates it uses are covered through the same typed loader/reconciliation helpers.

## Future-plan disposition

No blocked implementation plan became newly unblocked. M006A/C/D remain conditionally closed historical records and are not dependencies. M006E, M006F, M007, C005, and C006 remain closed. Phase 9 selective-cleanup/policy work remains future research and planning and has no registered implementation handoff.

## Unresolved findings and disposition

- No correctness or planning finding remains open.
- A real permission-bit denial was not used as a fixture because the runner's privilege level can make it nondeterministic; the stable directory-read I/O error exercises the same `Unavailable` classification and no-publication branch.
- Disposition: **closed**.
