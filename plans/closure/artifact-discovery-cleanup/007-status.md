# Artifact Discovery and Cleanup M007 Status

Plan: `plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md`

Disposition: **blocked**

Implementation commit: partial scaffolding is in `99b2bff` (`Implement adaptive discovery and qualify traversal hot path`); no M007 completion commit is claimed.

Date: 2026-10-03

## Blockers

1. M006E is conditionally closed. The reference Full traversal completed but its 741 diagnostics and 283 Cargo resolution failures prevented state publication. `clean --full` correctly refuses phase 2 when Full returns incomplete.
2. C003/C004 final ownership proof accepts one bounded `clean ROOT`. Running that proof independently for several roots cannot establish that a workspace in another selected root does not own a redirected output. Multi-root `--yes` therefore fails closed. This prevents the acceptance claim that known/full multi-root Execute is authorized by fresh complete cross-root proof.

## Implemented and verified portions

- CLI recognizes ROOT, `--known`, and `--full` as mutually exclusive, with preview, simulation, and execution modes.
- Known roots are canonicalized, deduplicated, and containment-collapsed.
- Each read-only/preview/simulation bounded scope is sent through the existing `clean_with` path; scope errors are isolated.
- `--dryrun` selects application simulation, which the existing cleanup tests prove invokes no Cargo clean process.
- Multi-root Execute fails before any cleanup scope starts.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| CLI modes and mutual exclusion | `cli::tests::orchestration_roots_are_mutually_exclusive_and_external_forms_match` | Pass locally |
| Per-root C004/C003 proof | Existing `clean_with` is invoked per root | Implemented for each bounded root; cross-root completeness is unproven |
| Multi-root Execute safety | Explicit fail-closed guard when more than one collapsed root is selected | Pass; requested capability remains incomplete |
| Full then cleanup | `--full` returns before cleanup if the Full scan is incomplete | Pass; the reference Full run is presently incomplete |
| Full orchestration acceptance | Fresh cross-root proof model and successful M006E state generation | **Not met** |

## Required unblocking work

- Close M006E with successful uncertainty-aware state reconciliation evidence.
- Define and implement a fresh proof that covers every selected root and every output owner across the combined root set, while preserving per-workspace CleanupUnit freshness and fail-closed handling.
- Add cross-root redirected-output, race, overlap, and zero-Cargo-clean regression fixtures, then repeat hosted Linux/macOS/Windows and Rust 1.89 qualification.
