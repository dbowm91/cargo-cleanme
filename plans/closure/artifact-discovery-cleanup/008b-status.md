# Artifact Discovery and Cleanup M008B Status

Plan: `plans/implementation/artifact-discovery-cleanup/008b-machine-readable-reporting-and-automation-contract.md`

Disposition: **closed**

Implementation commits: `a702efb41c2ed2a144416b905dfd1c8775795ed6` (`Add versioned JSON reporting contract`) and `ad5c84d91713f1dd5ddc4638ea2e871439b16c45` (`Complete M008A and M008B qualification fixtures`).

Date: 2026-10-04

## Implemented

- Added `--format human|json`; human remains default. JSON uses dedicated versioned DTOs in `src/output.rs`, not direct serialization of internal reports.
- Scan and cleanup emit one newline-terminated JSON document to stdout. JSON disables progress; `--stats` remains stderr-only.
- Schema v1 includes operation/scope/mode, summaries, physical scan groups, cleanup unit measurements, typed M008A policy dispositions, and stable coarse operation reason codes.
- Cleanup scope blocks and Cargo/process failures return nonzero status while still emitting JSON when a report exists.
- Documented schema and compatibility policy, process statuses, and explicit unattended `--yes` use with external scheduling.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Dedicated schema-versioned DTO boundary | `src/output.rs`; `plans/output-schema-v1.md` | Pass |
| JSON scan output and single-document stdout | `tests::json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | Pass |
| JSON cleanup output and mode identity | `tests::json_cleanup_emits_the_requested_mode_and_machine_summary` | Pass |
| Scope-blocked JSON with nonzero status | `tests::json_scope_block_is_emitted_with_nonzero_exit_status` | Pass |
| `--stats` stdout isolation | scan integration test compares byte-identical stdout with stats off/on | Pass |
| Stable policy disposition names | `cleanup::tests::policy_dispositions_have_stable_json_names` | Pass |
| Every safety reason has its own stable reason code | `CleanupReasonCode`, `proof_skip_code`, `unit_skip_code`; stable-name contract test | Pass; cleanup JSON projects typed reason codes |
| Schema scope, policy, state metadata, unresolved participants | `output::cleanup`; scope-blocked and successful JSON CLI fixtures | Pass |
| Fake-Cargo unattended Execute and exit/result semantics | `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | Pass; proves Cargo clean invocation and emitted typed result |
| Hosted Linux/macOS/Windows and Rust 1.89 | GitHub Actions CI run `37175693985` | Pass |

## Verification run

- `rtk cargo fmt --check` — passed.
- `rtk cargo test --all-targets --all-features` — passed, 196 tests.
- `rtk cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rtk rustup run 1.89 cargo test --locked --all-targets` — passed, 188 tests (180 library, 7 CLI, 1 end-to-end).
- `rtk git diff --check` — passed.

Stable subreason codes, complete scope metadata, unattended fake-Cargo execution, and hosted/MSRV evidence are implemented and verified. Remaining limitations: schema v1 remains a pre-release contract; output detail text is diagnostic and not stable, and CI does not exhaustively assert every JSON field across every platform. M008C is unblocked; its independent real-Cargo selector qualification is recorded in the M008C closure record.
