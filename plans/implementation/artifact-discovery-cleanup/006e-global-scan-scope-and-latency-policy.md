# M006E — Global Scan Scope and Latency Policy

Status: proposed

Repository baseline: M006D closure commit, recorded in `plans/closure/artifact-discovery-cleanup/006d-status.md`.

Source milestone: M006 performance hardening and routine machine-wide qualification

Primary class: product policy / global discovery

## 1. Objective

Resolve the conflict between the current full-machine project discovery contract and the 120-second no-argument scan target, using M006D's measured completion evidence. Obtain an explicit product decision before changing the default scan scope or introducing scope controls.

## 2. Triggering evidence

M006D retained all existing project-bearing domains and safety behavior, but native arm64 and Rosetta release scans both exceeded 120 seconds. Type-only traversal, bounded worker variations, and event-order variants did not complete the walk in time. See `plans/closure/artifact-discovery-cleanup/006d-status.md`.

## 3. Decision to obtain

Select and document one supported product direction:

1. Keep full-machine global discovery and revise the routine latency target to a measured, representative bound.
2. Keep the latency target and adopt an explicitly narrower default global scope, while retaining explicit-root discovery for other paths.
3. Keep both capabilities by defining opt-in scope controls and a clear default, then specify CLI/config precedence and reporting behavior.

No option is selected by this proposal. Until a decision is recorded, preserve the current full-scope behavior and do not claim the 120-second objective is met.

## 4. Invariants after a decision

- Explicit roots remain authoritative and retain current filter behavior.
- Traversal does not follow symlinks.
- Every manifest inside the selected scope remains part of discovery and cleanup proof completeness.
- Cleanup proof freshness, fail-closed behavior, and destructive authorization are unchanged.
- Any latency target is qualified using a matched, unprofiled release scan on a named representative host and reports full completion statistics.

## 5. Required closure evidence

- Record the selected scope/latency policy and its user-facing rationale.
- Update the CLI/config contract and roadmap only after policy selection.
- Add parity tests for defaults, explicit roots, filters, symlinks, manifest completeness, and scope/reporting behavior.
- Qualify the resulting reference-host scan and run Linux/macOS/Windows and Rust 1.89 CI.
- If full scope remains the selected default, define a revised measurable bound rather than implying the existing 120-second bound passes.
