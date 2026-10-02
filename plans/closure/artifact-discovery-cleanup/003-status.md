# Artifact Discovery and Cleanup M003 Closure

Plan: `plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md`

Disposition: **closed**

Implementation commits: `64b13a7b7746bd4ed60f8d8f6c481517b50bcb9a` through `0709d240154f1a96cf0f02764b9c0f4694390b19` (branch `implementation/artifact-discovery-cleanup`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| One immutable scan clock/cutoff; equality and future timestamps are active | `analysis` clock boundary tests | Pass |
| Recent source activity short-circuits target access | Test uses a recent source tree and a missing target; result is inactive-scan exclusion rather than target uncertainty | Pass |
| Target artifacts measured, empty targets excluded, uncertainty disqualifies | Sizing tests and end-to-end fixture | Pass |
| Source walk excludes target, VCS metadata, and nested VCS boundaries | `source_activity` traversal filter | Implemented |
| Unix/Windows allocated-size metric; apparent fallback elsewhere | `filesize::file_real_size_fast`; explicit `SizeMetric` classification | Implemented; Windows CI passed; no separate allocated-size runtime measurement was run |
| Stable size-descending report and deterministic ties | Report golden assertions and end-to-end fixture | Pass |
| Non-UTF-8 output strategy | Valid UTF-8 paths print plainly; non-UTF-8 paths use escaped `OsStr` debug formatting | Pass; `report_escapes_non_utf8_paths` |
| Diagnostics separated from stdout | Report goes to stdout; summary count goes to stderr | Pass |
| Full read-only pipeline | `tests/end_to_end.rs` exercises explicit scope, discovery, analysis, and report | Pass |
| No destructive behavior | Static source scan and command parser surface contain no cleanup command/path | Pass |
| Representative large-tree performance | Release scan of `/Users/davidbowman/projects`: 28 inactive candidates, 138.85 GiB estimated allocated, 19.94 s | Pass; local observation only |

## Verification run

- `cargo test activity` — passed (1 test).
- `cargo test sizing` — passed (1 test).
- `cargo test report` — passed (2 tests).
- `cargo test end_to_end` — passed (1 test).
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (20 tests).
- `cargo check --locked` — passed.
- Native platform: macOS 26.6.2 / Darwin 25.6.

## Limitations and unresolved findings

- Hosted Linux, macOS, and Windows CI passed in run [37052261642](https://github.com/dbowm91/cargo-cleanme/actions/runs/37052261642).
- No separate Windows allocated-size measurement was run.
- JSON output is intentionally omitted; the plan made it optional.
- The real-tree run is one read-only local observation. The user reviewed the report and approved proceeding to M004 on 2026-10-02. This satisfies M003 field review for planning M004.

Severity: no known correctness or security findings.

## Dependency disposition

M003 is closed. The user accepted the real-tree read-only observation and authorized continuing to M004. This closes the field-review dependency; M004 still owns separate destructive-safety acceptance gates.
