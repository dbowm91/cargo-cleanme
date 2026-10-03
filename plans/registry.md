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
Current repository planning baseline: caa4a6d667467fc529c6dad4e490f6575ff374bd

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
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M005 complete (C003 closed) | No blocked or ready plan. C003 closed the workspace cleanup-unit atomicity and full ownership-graph revalidation defects; M005 is release-qualified again. |

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
| Artifact discovery and cleanup | C003 workspace cleanup-unit atomicity + full ownership-graph revalidation | **closed** | plans/implementation/artifact-discovery-cleanup/c003-workspace-cleanup-unit-atomicity.md | Closure: plans/closure/artifact-discovery-cleanup/c003-status.md. M005A/M005B/C002 historical closures unchanged. |

## Immediate handoff

No ready, active, or blocked implementation plan remains in this subsystem. C003 is closed and M005 destructive release qualification is unblocked.

Completed order:

~~~text
M001-M004 -> closed
C001 -> closed
ADR 001 -> accepted
M005A -> closed
M005B -> historically closed
C002 -> historically closed
C003 workspace cleanup-unit atomicity / full ownership graph -> closed
M005 destructive release qualification -> unblocked by C003 closure
~~~

Post-C003 state: one Cargo clean invocation is authorized as one workspace CleanupUnit covering the complete resolved target/build OutputSet; every affected physical group must be `PrivateBounded` and authorized, or the whole unit is skipped; the final proof re-resolves the complete bounded ownership universe under clean ROOT and detects another discovered workspace moving into the affected output; Preview/Simulate/Execute emit one result and one deduplicated union measurement per unit; `cleanup --stats` reports the final-proof Cargo work.

M005 safety boundary (after C003):

- Cargo resolves workspace/output configuration.
- The destructive unit is one workspace CleanupUnit: one resolved workspace, its complete `OutputSet`, and every PhysicalOutputGroup that one `cargo clean` can affect.
- Every affected group must be `PrivateBounded`, authorized, inactive, non-symlink, and marker-qualified; shared, uncertain, and external-unproven physical output is inventory-only under any authorization configuration, and a private target group can never carry an unauthorized sibling build group into the same invocation.
- `cleanup.allowed_output_roots` is location authorization only and never manufactures ownership proof.
- Final revalidation rebuilds the complete fresh physical graph from every workspace discovered under `clean ROOT` and fails closed when that ownership cannot be re-proven.
- Progress UI is transient stderr state (one coordinated `MultiProgress`, determinate via observer contract, totals counting CleanupUnits) and is cleared before deterministic final stdout.
- `--dryrun` runs the full decision path with all non-mutating gates but invokes no Cargo clean command.
- Detailed counters/timings are opt-in via `--stats` (`--no-progress --stats` canonical), and cleanup `--stats` now includes final-proof work.

Deferred follow-ups (recorded in `plans/closure/artifact-discovery-cleanup/c003-status.md`, not blocking plans): the bounded ownership-universe refresh costs one re-resolution per cleanable candidate (N² Cargo resolutions for N cleanable workspaces, recorded with `--stats`), and on macOS a symlinked clean-root path component leaves member source roots non-canonical so the source-activity walk does not prune output directories (eligibility-neutral, wasted traversal, candidate corrective). The generic no-argument scan performance/pruning issue remains separate and was never part of C003.

M005 safety boundary (restored by C002, tightened by C003):

- Cargo resolves workspace/output configuration.
- Only `PrivateBounded` may reach Cargo cleanup; shared, uncertain, and external-unproven physical output is inventory-only under any authorization configuration.
- `cleanup.allowed_output_roots` is location authorization only for already-private groups and never manufactures ownership proof.
- Progress UI is transient stderr state (one coordinated `MultiProgress`, determinate via observer contract) and is cleared before deterministic final stdout.
- `--dryrun` runs the full decision path with all non-mutating gates but invokes no Cargo clean command.
- Detailed counters/timings are opt-in via `--stats` (`--no-progress --stats` canonical).
