# C029 — Permission-aware traversal and Full coverage corrective

Status: **closing** — local implementation and verification are complete; hosted qualification is pending.

Implementation commits: `653c8f6` (coverage-aware traversal and Full state/exit wiring), `651c97a` (1,001-event bounded aggregation control), plus the shared documentation/control-surface commit recorded in this branch.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Do not fabricate a child path when `dua_core` omits it | `discovery.rs::unreadable_child_keeps_accessible_inventory_and_unknown_coverage_bounded`; the diagnostic is pathless and says location unknown | Pass |
| Continue positive inventory beside an unreadable subtree | Same Unix test proves `read_dir` fails for the current test identity and retains the readable sibling `Cargo.toml` | Pass on local Linux, uid 1000 |
| Bound diagnostic retention independently of event count | `more_than_one_thousand_walk_errors_stay_one_bounded_root_record` feeds 1,001 permission errors to the production accumulator and asserts one root record with the complete count | Pass |
| Partial Full cannot publish negative state or report success | `ManifestDiscovery::coverage_complete` is shared by `reconcile_full` and Full exit selection; `discovery_state::tests` pins no publication for incomplete Full | Pass |
| Keep cleanup fail-closed | Existing cleanup whole-scope diagnostic gate plus C031 subprocess control; incomplete traversal emits no `cargo clean` | Pass |
| Preserve CLI/JSON/log schema | No DTO or log contract fields changed; full CLI contract suite passed | Pass |

## Verification actually run

On 2026-10-09, local uid 1000 (`sugarwookie`), Rust `1.99.0`:

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — 376 passed on Linux.
- `python3 scripts/check-fixture-portability.py --self-test` and the checker — passed; 36 fixture files clean.
- `python3 scripts/check-doc-citations.py --self-test` and the checker — passed; 16 documents clean.
- `git diff --check` — passed.

The first local clippy run rejected one nested conditional; it was corrected before the successful ladder. One earlier doc-citation run caught two out-of-range `main.rs` references; both were re-derived from the new source and the final checker passed.

## Platform and fixture evidence

The permission fixture verifies the unreadable premise before exercising mode `000`; it fails if the effective identity can still read the directory. The synthetic 1,001-event accumulator test is platform-independent. Hosted Linux/macOS/Windows and Rust 1.89 CI qualification is pending the pushed branch run.

## Limitations and findings

- `dua_core` 4.1 provides no failing child path on its error event. The implementation represents that as unknown-location coverage loss and refuses to publish a complete Full generation; it does not add a custom walker or guess a path.
- Diagnostic detail is bounded to one aggregate per selected root. It counts permission-denied, vanished, and other events; exact child attribution remains unavailable.
- No release or published-artifact qualification was authorized by this plan.

Unresolved findings: no known correctness defect. Cross-platform hosted qualification remains an external closure requirement.

Disposition: **closing** pending hosted qualification.
