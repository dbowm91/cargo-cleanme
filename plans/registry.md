# cargo-cleanme Active Planning Registry

This file is the compact control surface for active planning. Detailed requirements live in canonical documents, accepted ADRs, the subsystem roadmap, implementation plans, and closure records.

Canonical direction:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

Accepted architecture decisions:

- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

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
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M006 performance hardening | M006A implementation is conditionally closed but missed the 120-second scan target; ready M006C investigates throughput without pruning writable domains. M006B local implementation/evidence is closing pending hosted CI. M005 remains closed/green. |

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
| Artifact discovery and cleanup | M006A global scan pruning, traversal policy, path normalization | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/006a-global-scan-pruning-and-path-normalization.md | Closure: plans/closure/artifact-discovery-cleanup/006a-status.md. Policy/path work landed; scan target miss is tracked by ready M006C. |
| Artifact discovery and cleanup | M006B cleanup proof resolution efficiency | **closing** | plans/implementation/artifact-discovery-cleanup/006b-cleanup-proof-resolution-efficiency.md | Sequential handoff after M006A disposition; local implementation/evidence complete, hosted CI pending; MUST preserve C003/C004 freshness and fail-closed semantics. |
| Artifact discovery and cleanup | M006C global traversal throughput without scope narrowing | **ready** | plans/implementation/artifact-discovery-cleanup/006c-global-traversal-throughput-without-scope-narrowing.md | Unblocked by M006A's measured >120-second qualification miss and bounded subtree profile; no M006B dependency. Preserve writable/project-bearing global domains. |

## Immediate handoff

M006A has been implemented and conditionally closed with a measured qualification miss; M006B has been implemented after that disposition and is closing after local performance and test evidence. M006C is ready: it has a measured trigger, dominant-tree profile, tests, and acceptance criteria; M006A/B commits will provide a stable baseline.

Expected order:

~~~text
M001-M005 + C001-C004 -> closed
M006A global discovery/pruning/path normalization -> conditionally closed (macOS scan >120 s)
M006B cleanup-proof resolution efficiency -> closing -> hosted CI -> closed
M006C traversal throughput without scope narrowing -> ready
M006 performance hardening -> remains active until global scan throughput is qualified
~~~

M006A owns the >120 s no-argument scan, global-only OS/tool-managed pruning, one-pool multi-root discovery, and the macOS member-source-root/output-exclusion identity mismatch. M006A's 120-second acceptance criterion remains unmet; `/Users` dominated the measured post-policy traversal.

M006B owns cleanup proof subprocess cost. It removes redundant final-proof locate-project calls and adds bounded parallel metadata refresh after measurement. It MUST NOT reuse one stale ownership-universe snapshot across destructive candidates.

M005 destructive release qualification remains restored and is not blocked by M006; M006 is performance hardening, not a safety prerequisite.

### Current destructive safety boundary

- Cargo resolves workspace/output configuration; cargo-cleanme does not infer unresolved output paths.
- Every Cargo manifest discovered under clean ROOT must be authoritatively covered before any Cargo clean command may run.
- Unresolved ownership participants block Preview, Simulate, and Execute for the entire clean scope.
- The destructive unit is one workspace CleanupUnit over its complete OutputSet.
- Every affected physical group must be PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under fresh complete ownership-graph revalidation.
- cleanup.allowed_output_roots never manufactures ownership proof.
- --dryrun remains zero-clean and shares the same completeness/final-proof gates as Execute.
- --stats is the qualification/debug surface.
