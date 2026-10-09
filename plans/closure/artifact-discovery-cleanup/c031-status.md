# C031 — Routine cleanup happy path, ownership scope, and partial failure

Status: **closing** — safe whole-scope path implemented; independent partial-scope execution remains blocked by unaccepted ADR 004.

Implementation commit: `e58825b` (read-only resolution/count reporting before the whole-scope incomplete-discovery block), with regression test `0f8f10f`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Healthy Routine cleanup can reach the existing proof engine | Existing `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` and simulate/execute parity cases exercise the real CLI with a premise-checked fake Cargo | Pass |
| Do not report unattempted resolution as measured zero | `incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean` asserts one discovered manifest, one resolved workspace and one unit after an unreadable sibling is detected | Pass on local Linux, uid 1000 |
| Incomplete discovery blocks the whole cleanup scope | Same test asserts `scope_blocked=true` and exit 1 after resolution | Pass |
| No cleanup process runs under incomplete coverage | Same test asserts the fake Cargo recorded metadata but zero `clean` calls | Pass |
| Preserve JSON v1 and bounded log contracts | Test asserts existing JSON fields; no schema or log shape changed; complete CLI contract suite passed | Pass |
| Decide whether independent proof islands are justified | Review of redirected Cargo target/build configuration, unresolved manifests, unknown unreadable regions, and late participant/config changes finds no filesystem-only independence certificate | Retain C004/C006 whole-scope fail-closed boundary |

## Verification actually run

On 2026-10-09, local uid 1000 (`sugarwookie`), Rust `1.99.0`:

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — 376 passed on Linux.
- `python3 scripts/check-fixture-portability.py --self-test` and the checker — passed; 36 fixture files clean.
- `python3 scripts/check-doc-citations.py --self-test` and the checker — passed; 16 documents clean.
- `git diff --check` — passed.

## Platform and fixture evidence

The subprocess regression proves its own denied-read premise, uses the fixture's resolved fake Cargo on `PATH`, asserts metadata invocation, and checks zero cleanup calls. Existing healthy Routine Execute/Simulate fixtures remain green. Hosted Linux/macOS/Windows and Rust 1.89 qualification is pending the pushed branch run.

## Known limitations and blocked work

- ADR 004 remains **proposed and unaccepted**. A broken or unreadable workspace cannot expose its Cargo output configuration, and another workspace may redirect outputs into an apparently separate root. Late changes require the existing full-universe refresh. Partial-scope Execute is therefore not implemented.
- Whole-scope fail-closed behavior remains for any unresolved manifest or traversal uncertainty. This intentionally leaves mixed healthy/broken scopes unavailable for cleaning until a rigorous, accepted independence proof exists.
- No JSON schema, exit-code, or publication change was included.

Unresolved finding: **medium, deferred by design** — independent proof islands are not established. It is not a defect in the retained whole-scope model and does not block closing the safe track.

Disposition: **closing** pending hosted qualification; the independent partial-scope track remains blocked.
