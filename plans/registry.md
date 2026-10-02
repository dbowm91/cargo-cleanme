# cargo-cleanme Active Planning Registry

This file is the compact control surface for active planning. Detailed requirements live in canonical documents, the subsystem roadmap, implementation plans, and closure records.

Canonical direction:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

Planning framework baseline: 97dee9fa64868534cedf3a9310e7160335a91be2
Current repository audit baseline: 811e7d0833cda3d9eec35b2c77bbc3deb17aa379

## Status vocabulary

- proposed — direction exists but is not implementation-ready.
- ready — dependencies/contracts are satisfied.
- active — implementation is in progress.
- blocked — a named dependency/evidence requirement prevents progress.
- closing — implementation landed and closure evidence is being gathered.
- closed — closure record accepted.
- conditionally closed — substantial work landed but named evidence remains.
- superseded — replaced by another plan.
- archived — retained for traceability only.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies / blockers |
|---|---|---|---|---|
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | C001 ready | Post-M004 review found public CLI/config, MSRV-evidence, traversal-maintenance/performance, and planning-state gaps. M005 ownership design remains proposed; M005 implementation is blocked until C001 closes and its own ownership/ADR decision is complete. |

## Implementation handoffs

| Subsystem | Milestone | Status | Implementation plan | Dependency / handoff note |
|---|---|---|---|---|
| Artifact discovery and cleanup | M001 foundation, CLI, config, domain | **closed** | plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md | Closure: plans/closure/artifact-discovery-cleanup/001-status.md. Later public-contract drift is tracked by C001. |
| Artifact discovery and cleanup | M002 fast discovery and scope filters | **closed** | plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md | Closure: plans/closure/artifact-discovery-cleanup/002-status.md. Traversal consolidation/performance follow-up is tracked by C001. |
| Artifact discovery and cleanup | M003 activity, sizing, read-only report | **closed** | plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md | Closure: plans/closure/artifact-discovery-cleanup/003-status.md. Activity/eligibility semantics remain authoritative during C001. |
| Artifact discovery and cleanup | M004 revalidated cleanup execution | **closed** | plans/implementation/artifact-discovery-cleanup/004-revalidated-cleanup-execution.md | Closure: plans/closure/artifact-discovery-cleanup/004-status.md. M004 safety/ownership boundary must not regress during C001. |
| Artifact discovery and cleanup | C001 public contract, MSRV, traversal, and planning reconciliation | **ready** | plans/implementation/artifact-discovery-cleanup/c001-public-contract-msrv-traversal-reconciliation.md | Start here. Corrects post-M004 findings without broadening cleanup ownership. |

## Proposed / blocked follow-on work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Artifact discovery and cleanup | M005 redirected/shared output | **proposed / implementation blocked** | not yet implementation-ready | C001 closure plus explicit shared/redirected-output ownership model and ADR decision. Design research may continue, but no production implementation should start. |

## Immediate handoff

Implement C001.

Expected order:

~~~text
M001-M004 -> closed
C001 corrective/reconciliation -> ready
C001 -> closure
M005 ownership/ADR design -> reconcile against corrected baseline
M005 implementation plan -> only after ownership decision
~~~

The corrective pass must preserve M004's deliberately narrow destructive boundary: explicit cleanup scope, dry-run by default, --yes for execution, fresh activity revalidation, Cargo ownership checks, and no direct recursive deletion.
