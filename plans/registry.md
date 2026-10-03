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
Current repository planning baseline: 35705a2

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
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M005 complete (C002 closed) | C002 closed; M005 again release-qualified. No blocked milestones. |

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
| Artifact discovery and cleanup | C002 M005 ownership safety, simulation parity, progress, global qualification | **closed** | plans/implementation/artifact-discovery-cleanup/c002-m005-safety-progress-performance-reconciliation.md | Closure: plans/closure/artifact-discovery-cleanup/c002-status.md. M005 again release-qualified. |

## Immediate handoff

C002 closed; M005 release qualification unblocked.

Expected order:

~~~text
M001-M004 -> closed
C001 -> closed
ADR 001 -> accepted
M005A -> closed
M005B -> historically closed
C002 ownership/simulation/preflight/progress/performance correction -> closed
M005 complete (M005A + M005B historical + C002) -> release-qualified
~~~

Post-M005 audit findings (now corrected by C002 with history preserved): ExternalUnproven authorization violating ADR 001, incomplete Simulate/pre-spawn gate parity, incomplete final build/output preflight, unqualified multi-line progress/cleanup determinate state, redundant per-member locate calls, and unqualified generic-scan performance.

M005 safety boundary (restored by C002):

- Cargo resolves workspace/output configuration.
- Only `PrivateBounded` may reach Cargo cleanup; shared, uncertain, and external-unproven physical output is inventory-only under any authorization configuration.
- `cleanup.allowed_output_roots` is location authorization only for already-private groups and never manufactures ownership proof.
- Progress UI is transient stderr state (one coordinated `MultiProgress`, determinate via observer contract) and is cleared before deterministic final stdout.
- `--dryrun` runs the full decision path with all non-mutating gates but invokes no Cargo clean command.
- Detailed counters/timings are opt-in via `--stats` (`--no-progress --stats` canonical).
