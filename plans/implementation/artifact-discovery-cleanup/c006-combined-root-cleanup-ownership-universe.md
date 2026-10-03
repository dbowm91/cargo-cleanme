# Corrective C006 — Combined-Root Cleanup Ownership Universe and Simulation Parity

Status: ready after C005

Repository baseline: C005 planning head

Corrects:

- M007 learned/full cleanup orchestration
- M007 blocked status remains historical until this corrective closes
- C003/C004 single-root safety semantics are preserved and generalized, not weakened

Authoritative references:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
- `plans/adr/002-adaptive-routine-full-discovery-state.md`
- `plans/implementation/artifact-discovery-cleanup/c003-workspace-cleanup-unit-atomicity.md`
- `plans/implementation/artifact-discovery-cleanup/c004-complete-ownership-resolution-coverage.md`
- `plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md`
- `plans/closure/artifact-discovery-cleanup/007-status.md`

Primary class: corrective / destructive safety / multi-root ownership proof / preview-simulation parity

## 1. Objective

Replace M007's current per-root cleanup composition with one fresh combined ownership universe spanning every selected bounded root.

The current implementation correctly blocks multi-root Execute, but Preview/Simulate still evaluate selected roots independently. That can produce optimistic would-clean results when a workspace under one selected root redirects output into another selected root or otherwise shares output across roots.

C006 must make Preview, Simulate, and Execute consume the same combined-root ownership proof before any per-CleanupUnit disposition.

## 2. Triggering finding

C003/C004 prove cleanup safety inside one bounded `clean ROOT`:

- all discovered manifests under ROOT must be authoritatively covered;
- every resolved workspace in that universe participates in the physical output graph;
- immediately before candidate disposition the bounded universe is refreshed;
- another workspace moving into overlap causes fail-closed behavior.

M007 currently invokes this proof independently for several learned roots.

For selected roots A and B, proving A alone cannot establish that a workspace under B does not resolve into A's output region, and vice versa.

Therefore:

- multi-root Execute is currently blocked correctly;
- Preview/Simulate can still describe a unit as previewable/would-clean even though the combined selected universe would classify its output as Shared/Uncertain.

The safety problem is not filesystem traversal; it is proof scope.

## 3. Combined cleanup scope

Introduce a combined orchestration domain equivalent in responsibility to:

```text
CombinedCleanupScope {
    roots: Vec<CanonicalRoot>,
    discovered_manifests: ...,
    resolution_coverage: ...,
    workspaces: ...,
    physical_output_graph: ...,
    cleanup_units: ...,
}
```

Exact type names are implementation choices.

The selected root vector is:

- canonicalized;
- sorted;
- deduplicated;
- containment-collapsed.

The combined root set is the complete source-discovery boundary for that cleanup operation.

A source path outside all selected roots is not part of the combined ownership universe.

## 4. Combined manifest discovery

Discovery MUST enumerate every Cargo.toml reachable beneath every selected root using the existing explicit-root safety semantics.

Requirements:

- no symlink directory following;
- internal target/VCS pruning unchanged;
- no user global ignore/unignore semantics inside explicit cleanup roots;
- canonical manifest identity deduplicates overlap;
- contained selected roots do not cause duplicate traversal;
- one root's discovery failure is preserved as a combined-scope diagnostic.

All discovered manifests from all selected roots feed one C004-style coverage calculation.

## 5. Combined resolution coverage

Generalize C004's rule:

```text
combined unresolved =
    all manifests discovered under all selected roots
    -
    authoritative Cargo manifest coverage from all successfully resolved workspaces
```

If any unresolved ownership participant remains anywhere in the combined selected scope:

- Preview is blocked;
- Simulate is blocked;
- Execute is blocked;
- no `cargo clean` or `cargo clean --dry-run` process may run.

This is intentionally scope-wide. An unresolved workspace under root B could resolve to output owned by a candidate under root A, so per-root isolation is not sound.

The report must state that the combined ownership universe is incomplete.

## 6. Combined physical output graph

Build one physical output graph from every successfully resolved workspace in the combined selected universe.

Equal, nested, overlapping, redirected, and shared target/build roots are classified together regardless of which selected source root owns the workspace.

Examples that must be caught:

- workspace under A redirects target into B/project/target;
- workspaces under A and B both use one external shared cache;
- workspace under B changes output into A's conventional target between initial resolution and final proof;
- nested selected roots resolve to the same workspace;
- distinct source roots resolve members of one workspace spanning both selected roots.

CleanupUnit construction remains workspace-level over the complete OutputSet.

## 7. Authorization semantics remain unchanged

Combining source roots broadens only the ownership universe, not cleanup authorization.

A physical output group is not automatically authorized merely because another selected source root contains it.

Existing ADR 001 rules remain authoritative:

- Shared remains non-destructive;
- Uncertain remains non-destructive;
- ExternalUnproven remains non-destructive;
- cleanup.allowed_output_roots does not manufacture ownership proof;
- symlinked/ambiguous output remains non-destructive.

Where existing code treats output physically contained within the selected clean source sandbox as bounded, the combined implementation must define containment against the union of selected roots without accidentally authorizing unrelated external caches.

Add explicit tests for this boundary.

## 8. Fresh final proof

Before each CleanupUnit Preview/Simulate/Execute disposition, revalidate against a fresh combined selected-root universe.

The simplest safe implementation is allowed to be expensive:

1. rediscover/re-resolve the complete combined selected-root ownership universe;
2. verify complete manifest coverage;
3. rebuild the full physical output graph;
4. remap the candidate workspace and complete OutputSet;
5. verify expected physical covering roots and ownership class;
6. re-run activity/marker/authorization gates;
7. only then perform the selected mode.

Do not reuse one stale combined snapshot across multiple destructive candidates merely to improve performance.

Performance optimization is deferred unless it can preserve C003/C004 freshness exactly.

## 9. Preview, Simulate, and Execute parity

All cleanup modes use the same combined scope and final proof.

### Preview

May invoke `cargo clean --dry-run --verbose` only after combined proof authorizes the CleanupUnit.

### Simulate / `--dryrun`

Runs the same discovery, coverage, ownership, activity, authorization, and final-proof path but invokes zero Cargo clean processes.

### Execute / `--yes`

May invoke real Cargo clean only after the same proof.

A unit rejected by combined proof must have the same ownership/safety disposition in all three modes.

C006 therefore removes the current semantic gap where multi-root Preview/Simulate may be optimistic while Execute is globally blocked.

## 10. Known cleanup orchestration

`clean --known`:

1. resolve active Routine roots from C005/M006E;
2. canonicalize/deduplicate/containment-collapse;
3. construct one CombinedCleanupScope from the entire root set;
4. run combined resolution/analysis/final proof;
5. emit deterministic aggregated results.

If no known roots exist, return successful no-op output and do not fall back to Full.

Corrupt/unavailable discovery state follows C005 Routine fallback semantics; it does not broaden cleanup scope.

## 11. Full cleanup orchestration

`clean --full` has two strict phases:

### Phase 1 — Full reconciliation

Run C005-compliant Full discovery/state reconciliation.

If the Full traversal is incomplete/fatal or no trustworthy state generation can be produced under C005's rules, stop before cleanup.

Localized diagnostics that C005 can safely reconcile do not automatically block phase 2.

### Phase 2 — Combined bounded cleanup

Select the active learned roots produced by the successful reconciliation.

Construct one CombinedCleanupScope across those roots.

Do not reuse discovery-state workspace/output records as cleanup proof; re-discover/re-resolve for cleanup.

## 12. Reporting and accounting

The final report must remain deterministic and avoid duplicate bytes.

Report at least:

- selected root count and ordered root list when useful;
- discovered manifest count;
- unresolved ownership count;
- resolved workspace count;
- CleanupUnits considered;
- units skipped/previewed/simulated/cleaned/failed;
- deduplicated would-clean estimate for Simulate;
- observed pre/post deduplicated union for Execute;
- per-unit actionable skip reason.

If a combined scope is blocked before per-unit proof due unresolved ownership, do not print would-clean/recovered totals that imply eligibility.

Cross-root overlapping physical outputs are counted once.

## 13. Failure semantics

A combined-scope discovery or resolution incompleteness is scope-wide fail-closed for cleanup modes.

Once the combined initial universe is complete, a failure isolated to one CleanupUnit's activity/authorization/final proof may skip/fail that unit without necessarily blocking independent units, consistent with existing cleanup semantics.

Cancellation stops launching new units and uses existing Cargo child handling for any active subprocess.

## 14. Required production changes

### A — Multi-root discovery API

Add a cleanup-specific discovery entry point accepting a slice/vector of explicit roots and returning one deduplicated manifest set plus diagnostics/counters.

Reuse the existing shared traversal machinery where practical.

### B — Combined resolution coverage

Generalize the C004 cleanup resolver from one source root to a selected-root set.

Do not weaken authoritative manifest coverage rules.

### C — Combined output grouping and authorization

Build groups/units from the full combined workspace set.

Make selected-root containment checks explicit and testable.

### D — Combined final proof

Generalize `final_cleanup_proof` or add a sibling abstraction that refreshes the complete selected-root universe before each candidate.

The existing single-root `clean ROOT` path should either:

- delegate to the combined implementation with a one-element root vector; or
- remain as a compatibility wrapper around the same proof engine.

Avoid maintaining two divergent safety models.

### E — Orchestrator

Refactor `clean --known` and `clean --full` to call the combined engine once rather than loop over independent `clean_with(root)` calls.

Remove the blanket `roots.len() > 1 && Execute` rejection only after the combined proof is fully qualified.

### F — Statistics

Aggregate counters/timings across the combined operation.

Do not report only the first root's counters, as the current scaffold does.

## 15. Required tests

### Combined discovery/coverage

- two independent selected roots, all manifests resolve -> complete;
- unresolved manifest in root B blocks candidate in root A in Preview/Simulate/Execute;
- overlapping selected roots deduplicate traversal/manifests;
- two selected roots containing members of one workspace collapse to one workspace identity;
- symlink aliases do not duplicate manifest/workspace identities where canonicalization is supported.

### Cross-root output ownership

- A and B both resolve same output -> Shared, zero clean calls;
- B redirects output into A's target -> A not PrivateBounded;
- A redirects output beneath B -> combined graph catches overlap;
- external shared cache used by A+B remains Shared;
- one external observed owner remains ExternalUnproven under existing ADR rules;
- allowed_output_roots does not convert Shared/Uncertain/ExternalUnproven into private ownership.

### Races/final proof

For candidate A initially private:

- B moves target into A output before A disposition -> block A;
- B changes build-dir into A output -> block A;
- B becomes unresolvable during fresh proof -> block A;
- B introduces a symlink/unresolvable path that can reach A output -> block A;
- candidate A output identity changes -> block A;
- candidate A activity becomes recent -> block A.

Repeat critical cases in Preview, Simulate, Execute.

### Mode parity

- same eligible units across Preview/Simulate/Execute before mode-specific Cargo spawn;
- Simulate invokes zero Cargo clean commands across all roots;
- blocked combined scope invokes zero Cargo clean commands including preview;
- multi-root Execute succeeds only in fixtures with complete combined proof;
- single-root behavior remains regression-compatible.

### Full/known orchestration

- no known roots -> successful no-op;
- known roots -> one combined proof;
- Full failure/incomplete -> phase 2 never starts;
- C005 successful Full with localized diagnostics -> phase 2 may start;
- Full learned roots are rediscovered freshly for cleanup;
- cleanup does not expire learned roots.

### Reporting

- deterministic root/unit order;
- cross-root overlapping outputs counted once;
- aggregated counters include all roots;
- unresolved scope emits no would-clean/recovered total;
- per-unit results do not duplicate workspace/output bytes.

### Platform

- Linux/macOS/Windows hosted CI;
- Rust 1.89 check/test;
- existing C003/C004 matrix remains green.

## 16. Acceptance criteria

C006 closes when:

- `clean ROOT`, `clean --known`, and `clean --full` share one combined-capable proof engine;
- every selected root participates in one initial resolution-coverage universe;
- every candidate final proof refreshes the complete selected-root universe;
- cross-root redirected/shared output is classified correctly;
- Preview/Simulate/Execute agree on safety disposition;
- Simulate remains globally zero-Cargo-clean;
- multi-root Execute no longer needs a blanket rejection because combined proof is sound;
- reporting/accounting is deduplicated and aggregates all roots;
- C003/C004 regressions and hosted Linux/macOS/Windows + Rust 1.89 gates pass.

On closure, M007 may move from blocked to closed if its remaining orchestration/reporting requirements are satisfied.

## 17. Stop conditions

Stop and preserve the current multi-root Execute fail-closed guard if:

- complete selected-root manifest coverage cannot be established;
- final proof would require trusting cached discovery state;
- authorization would need to treat selected roots as evidence of external ownership;
- a proposed optimization reuses stale combined ownership snapshots across candidates;
- Preview/Simulate cannot be made to share the same ownership disposition as Execute.
