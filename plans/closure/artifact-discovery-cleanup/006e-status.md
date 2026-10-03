# Artifact Discovery and Cleanup M006E Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md`

Disposition: **conditionally closed**

Implementation commit: `99b2bff` (`Implement adaptive discovery and qualify traversal hot path`).

Date: 2026-10-03

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Routine/Full/Explicit intent and CLI | `scan --full`, explicit scan roots, implicit Routine scope, and conflicting `ROOT --full` are represented in CLI/policy. | Implemented; covered by CLI/policy regression tests |
| Bounded Routine roots and no implicit Full fallback | Existing home-relative developer seeds plus active machine-local learned roots are canonicalized, deduplicated, and containment-collapsed. Empty Routine scope reports a `scan --full` hint. | Implemented |
| Versioned machine-local state | JSON schema v1 separates exact workspace records from learned roots; writes use a sibling temporary file and rename. Round-trip and replacement are tested. | Partial; atomicity is tested on the current host only |
| Full state publication only after trustworthy reconciliation | Reference Full traversal completed, but the CLI recorded 741 filesystem diagnostics, 283 Cargo resolution failures, and did not publish state. This is fail-closed. | **Qualification remains** |
| Retention and positive observations | Configurable 0–3650 day retention (30 default); Routine roots retain future timestamps; Full pruning runs only when diagnostics are absent. | Implemented; uncertainty handling remains conservative and unqualified |
| Config first-use and edit UX | Operational config loading bootstraps from the embedded template; `config path` remains side-effect free; `config init` is removed; `config edit` parses quoted arguments without a shell and validates on return. | Implemented; interactive editor execution and cross-platform fake-editor evidence remain |
| Cleanup authority remains bounded | Known/Full modes call the existing bounded cleanup operation per collapsed root. Multiple-root Execute is rejected because existing fresh proof covers one root. | Safety boundary preserved; orchestration incomplete |
| Rust and regression checks | See commands in M006F closure; final checks passed: 168 tests, Clippy with warnings denied, and Rust 1.89 check/test. | Pass locally |

## Reference Full evidence

The production traversal visited 4,670,216 entries and found 1,803 manifests. Cargo resolution and analysis took the total invocation to 97.59 seconds, with 1,053 workspaces resolved, 283 resolution failures, and 741 filesystem diagnostics. The command exited 1 and did not publish a reconciled generation. Temporary XDG config/state directories isolated the run from the user's normal configuration. A prior direct discovery-only harness run recorded 4,670,246 entries and 1,803 manifests in 17.56 seconds.

## Limitations and downstream disposition

- M006E is conditionally closed until a reference Full reconciliation can distinguish recoverable partial coverage, preserve roots intersecting uncertainty, and publish a trustworthy generation.
- Corrective C005 is the ready handoff for that work: `plans/implementation/artifact-discovery-cleanup/c005-uncertainty-aware-state-reconciliation-and-config-edit-hardening.md`.
- C005 also owns containment-collapsed persisted roots, visible fail-soft state diagnostics, safer state temp publication, explicit retention=0 semantics, and cross-platform `config edit` fake-editor qualification.
- M007 remains blocked by this hard dependency and by the one-root scope of the current C003/C004 fresh ownership proof; corrective C006 owns the combined-root proof.
- No learned state is used as cleanup ownership or authorization proof.
