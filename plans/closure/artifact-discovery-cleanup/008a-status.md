# Artifact Discovery and Cleanup M008A Status

Plan: `plans/implementation/artifact-discovery-cleanup/008a-workspace-selective-cleanup-policy.md`

Disposition: **conditionally closed**

Implementation commit: `aa758f6af6cdc4c007a6c05bde6938940ba7f2bf` (`Add workspace cleanup selection policy`).

Date: 2026-10-04

## Implemented

- Added minimum fresh reclaimable-byte and inactivity-age filters, plus canonical-workspace include/exclude globs, to ROOT/`--known`/`--full` cleanup.
- Policy-rejected units are filtered after complete workspace resolution, so they remain participants in the combined ownership universe.
- The existing final proof now retains fresh source/output activity timestamps and sums fresh measured bytes. Size and age policy are checked against those proof observations before Cargo is spawned.
- Existing configurations remain valid; `[cleanup.policy]` is optional and defaults to neutral values. Config and CLI patterns are validated.
- Documented cleanup policy and added legacy-config, invalid-policy, and CLI parsing tests.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Neutral backward-compatible config | `config::tests::legacy_cleanup_config_loads_with_neutral_policy` | Pass |
| Invalid glob and overflowing duration rejected | `config::tests::cleanup_policy_globs_and_durations_are_validated` | Pass |
| Policy CLI accepted for repeated patterns and orchestration modes | `cli::tests::cleanup_policy_options_are_repeatable_and_mode_independent` | Pass |
| Fresh size/activity data collected by final proof | `cleanup::final_cleanup_proof_roots`; existing final-proof race suite | Implemented; policy-specific race fixtures still required |
| Excluded workspaces remain in ownership universe | Policy selection occurs after `resolve_cleanup_scope`; no dedicated exclusion/shared-output fixture yet | Implemented by ordering; evidence incomplete |
| Policy outcomes are typed and machine-projectable | Current `CleanResult` retains policy reasons only in `detail` | **Not met** |
| Hosted Linux/macOS/Windows and Rust 1.89 gates | Local Rust 1.89 checks/tests passed; no hosted matrix run in this implementation | Local pass; hosted evidence outstanding |

## Verification run

- `rtk cargo fmt --check` — passed.
- `rtk cargo test --all-targets --all-features` — passed, 187 tests.
- `rtk cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rtk rustup run 1.89 cargo test --locked --all-targets` — passed, 187 tests.
- `rtk git diff --check` — passed.

## Qualification required before M008B

1. Add a typed stable policy disposition to cleanup results; rendered `detail` is not an adequate interface dependency.
2. Add policy-specific fresh-size shrink, source-age change, output-age change, excluded-owner/shared-output, and mode-parity fixtures proving no Cargo process starts when rejected.
3. Run hosted Linux/macOS/Windows checks and record their results.

No unsafe behavior was identified in the implemented path. M008B remains blocked until the typed interface is frozen and the qualification items above are complete. M008C remains blocked on M008A/M008B and its independent real-Cargo selector matrix.
