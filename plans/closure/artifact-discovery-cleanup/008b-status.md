# Artifact Discovery and Cleanup M008B Status

Plan: `plans/implementation/artifact-discovery-cleanup/008b-machine-readable-reporting-and-automation-contract.md`

Disposition: **conditionally closed**

Implementation commit: `a702efb41c2ed2a144416b905dfd1c8775795ed6` (`Add versioned JSON reporting contract`).

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
| Every safety reason has its own stable reason code | Current schema distinguishes ownership classes and generic `skipped_safety`; active/marker/race/authorization subreasons are not all typed | **Incomplete** |
| Full schema fixture matrix, scan/cleanup cross-format parity, Full state metadata, fake-Cargo unattended Execute, hosted Linux/macOS/Windows | Only structural CLI fixtures run locally; see remaining qualification | **Incomplete** |

## Verification run

- `rtk cargo fmt --check` — passed.
- `rtk cargo test --all-targets --all-features` — passed, 191 tests.
- `rtk cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rtk rustup run 1.89 cargo test --locked --all-targets --quiet` — passed, 191 tests (184 library, 6 CLI, 1 end-to-end).
- `rtk git diff --check` — passed.

## Remaining qualification before M008C

1. Finish typed reason codes for active, marker, authorization, changed-before-cleanup, Cargo failure, and measurement failure paths without parsing human detail.
2. Add deterministic schema fixtures for the full plan matrix, byte-identical serialization, progress parity, and human/JSON summary parity.
3. Add fake-Cargo non-interactive `--yes` integration and verify exit semantics for each documented result class.
4. Run hosted Linux/macOS/Windows and Rust 1.89 gates.

M008C remains blocked: M008A and M008B are conditionally closed rather than fully closed, and the required real-Cargo selector qualification has not been run.
