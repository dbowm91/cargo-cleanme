# cargo-cleanme Active Planning Registry

This file is the compact control surface for active planning. Detailed requirements live in canonical documents, accepted ADRs, the subsystem roadmap, implementation plans, and closure records.

Canonical direction:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

Accepted architecture decisions:

- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md
- plans/adr/002-adaptive-routine-full-discovery-state.md

Planning framework baseline: 97dee9fa64868534cedf3a9310e7160335a91be2
M006 implementation baseline: 47574fa8a087ac2f9111e29821c23b13657e7bbe

## Status vocabulary

- proposed — direction exists but is not implementation-ready.
- ready — dependencies/contracts are satisfied.
- active — implementation is in progress.
- blocked — a named dependency/evidence requirement prevents progress.
- closing — implementation landed and closure evidence is being gathered.
- closed — closure record accepted.
- conditionally closed — substantial work landed but named evidence remains.
- superseded — replaced by a newer plan.
- archived — retained for traceability only.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies / blockers |
|---|---|---|---|---|
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M006E adaptive Routine/Full discovery | M006A/C/D retain historical conditional closure evidence; M006B is closed. ADR 002 resolves the scope decision. M006E is ready; M006F follows for exhaustive hot-path qualification; M007 is proposed after M006E. M005 remains closed/green. |

## Implementation handoffs

| Subsystem | Milestone | Status | Implementation plan | Dependency / handoff note |
|---|---|---|---|---|
| Artifact discovery and cleanup | M001 foundation, CLI, config, domain | **closed** | plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md | Closure: plans/closure/artifact-discovery-cleanup/001-status.md. |
| Artifact discovery and cleanup | M002 fast discovery and scope filters | **closed** | plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md | Closure: plans/closure/artifact-discovery-cleanup/002-status.md. |
| Artifact discovery and cleanup | M003 activity, sizing, read-only report | **closed** | plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md | Closure: plans/closure/artifact-discovery-cleanup/003-status.md. |
| Artifact discovery and cleanup | M004 revalidated cleanup execution | **closed** | plans/implementation/artifact-discovery-cleanup/004-revalidated-cleanup-execution.md | Closure: plans/closure/artifact-discovery-cleanup/004-status.md. |
| Artifact discovery and cleanup | C001 public-contract/MSRV/traversal reconciliation | **closed** | plans/implementation/artifact-discovery-cleanup/c001-public-contract-msrv-traversal-reconciliation.md | Closure: plans/closure/artifact-discovery-cleanup/c001-status.md. |
| Artifact discovery and cleanup | M005A workspace output resolution, fail-fast scan, inline progress | **closed** | plans/implementation/artifact-discovery-cleanup/005a-workspace-output-resolution-fail-fast-progress.md | Closure: plans/closure/artifact-discovery-cleanup/005a-status.md. |
| Artifact discovery and cleanup | M005B authorized redirected cleanup + full simulation | **closed** | plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md | Historical closure: plans/closure/artifact-discovery-cleanup/005b-status.md; post-closure defects tracked by C002. |
| Artifact discovery and cleanup | C002 M005 ownership safety, simulation parity, progress, global qualification | **closed** | plans/implementation/artifact-discovery-cleanup/c002-m005-safety-progress-performance-reconciliation.md | Historical closure: plans/closure/artifact-discovery-cleanup/c002-status.md; post-closure cleanup-unit defects tracked by C003. |
| Artifact discovery and cleanup | C003 workspace cleanup-unit atomicity + full ownership-graph revalidation | **closed** | plans/implementation/artifact-discovery-cleanup/c003-workspace-cleanup-unit-atomicity.md | Historical closure: plans/closure/artifact-discovery-cleanup/c003-status.md; post-closure unresolved-participant defect tracked by C004. |
| Artifact discovery and cleanup | C004 complete ownership-universe resolution coverage | **closed** | plans/implementation/artifact-discovery-cleanup/c004-complete-ownership-resolution-coverage.md | Closure: plans/closure/artifact-discovery-cleanup/c004-status.md. M005 destructive release qualification is restored. |
| Artifact discovery and cleanup | M006A global scan pruning, traversal policy, path normalization | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/006a-global-scan-pruning-and-path-normalization.md | Closure: plans/closure/artifact-discovery-cleanup/006a-status.md. Policy/path work landed; its scan miss is preserved as historical evidence and motivated ADR 002's Routine/Full split. |
| Artifact discovery and cleanup | M006B cleanup proof resolution efficiency | **closed** | plans/implementation/artifact-discovery-cleanup/006b-cleanup-proof-resolution-efficiency.md | Closure: plans/closure/artifact-discovery-cleanup/006b-status.md. Preserves C003/C004 freshness and fail-closed semantics; proof/process reductions are measured. |
| Artifact discovery and cleanup | M006C global traversal throughput without scope narrowing | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/006c-global-traversal-throughput-without-scope-narrowing.md | Closure: plans/closure/artifact-discovery-cleanup/006c-status.md. Bounded traversal and parity work landed; exact no-argument scan remains over 120 seconds. |
| Artifact discovery and cleanup | M006D global scan completion qualification | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/006d-global-scan-completion-qualification.md | Closure: plans/closure/artifact-discovery-cleanup/006d-status.md. Optimizations preserve scope and safety, but the exact native scan remains over 120 seconds. |
| Artifact discovery and cleanup | M006E adaptive Routine/Full discovery + state/config UX | **ready** | plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md | ADR 002 accepted. Implements bounded Routine roots, explicit Full reconciliation, learned project/root state, 30-day configurable retention, automatic config bootstrap, and config edit. |
| Artifact discovery and cleanup | M006E old scope/latency decision | **superseded** | plans/implementation/artifact-discovery-cleanup/006e-global-scan-scope-and-latency-policy.md | Replaced by ADR 002 and the ready adaptive-discovery M006E plan. |
| Artifact discovery and cleanup | M006F exhaustive traversal hot-path qualification | **ready after M006E** | plans/implementation/artifact-discovery-cleanup/006f-exhaustive-traversal-hot-path-qualification.md | Measure bare dua-core ceiling, remove optional attribution from normal hot path, qualify traversal order/workers, record completed Full baseline. |
| Artifact discovery and cleanup | M007 learned/full cleanup orchestration | **proposed** | plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md | Hard dependency: M006E closure. Learned state may select bounded roots but never provides cleanup ownership/authorization proof. |

## Immediate handoff

M006A, M006C, and M006D remain conditionally closed as historical exhaustive-scan evidence; M006B is closed. ADR 002 has resolved the product decision. The immediate handoff is ready M006E.

Expected order:

~~~text
M001-M005 + C001-C004 -> closed
M006A global discovery/pruning/path normalization -> conditionally closed (historical evidence)
M006B cleanup-proof resolution efficiency -> closed
M006C traversal throughput without scope narrowing -> conditionally closed (historical evidence)
M006D global scan completion qualification -> conditionally closed (historical evidence)
ADR 002 adaptive Routine/Full discovery -> accepted
M006E adaptive Routine/Full discovery + state/config edit -> ready
M006F exhaustive traversal hot-path qualification -> ready after M006E
M007 learned/full cleanup orchestration -> proposed after M006E
~~~

M006E owns the next implementation handoff: bounded Routine roots, explicit `scan --full`, exact project inventory, learned-root derivation, successful-Full-only retention pruning, 30-day configurable retention, atomic machine-local state, automatic config creation, removal of `config init`, and `config edit` with VISUAL/EDITOR/fallback resolution.

M006F owns the remaining implementation-side exhaustive traversal qualification. It does not own scope narrowing.

M007 is intentionally separate because using learned/full discovery to select cleanup scopes must preserve the existing C003/C004 bounded fresh-proof model.

M006B continues to own cleanup proof subprocess cost and MUST NOT reuse one stale ownership-universe snapshot across destructive candidates.

M005 destructive release qualification remains restored and is not blocked by M006/M007 planning.

### Current destructive safety boundary

- Cargo resolves workspace/output configuration; cargo-cleanme does not infer unresolved output paths.
- Every Cargo manifest discovered under clean ROOT must be authoritatively covered before any Cargo clean command may run.
- Unresolved ownership participants block Preview, Simulate, and Execute for the entire clean scope.
- The destructive unit is one workspace CleanupUnit over its complete OutputSet.
- Every affected physical group must be PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under fresh complete ownership-graph revalidation.
- cleanup.allowed_output_roots never manufactures ownership proof.
- --dryrun remains zero-clean and shares the same completeness/final-proof gates as Execute.
- --stats is the qualification/debug surface.
