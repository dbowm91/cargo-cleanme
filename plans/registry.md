# cargo-cleanme Active Planning Registry

This file is the compact control surface for active planning. Detailed requirements live in canonical documents, the subsystem roadmap, implementation plans, and future closure records.

Canonical direction:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

Planning framework baseline: 97dee9fa64868534cedf3a9310e7160335a91be2

## Status vocabulary

- proposed — direction exists but is not implementation-ready.
- ready — dependencies/contracts are satisfied.
- active — implementation is in progress.
- blocked — a named dependency/evidence requirement prevents progress.
- closing — implementation landed and closure evidence is being gathered.
- closed — closure record accepted.
- conditionally closed — substantial work landed but named evidence remains.
- superseded — replaced by another document.
- archived — retained for traceability only.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies / blockers |
|---|---|---|---|---|
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M004 closing | M003 closed after hosted CI and user field review. M004 implementation is complete; hosted CI pending. |

## Implementation handoffs

| Subsystem | Milestone | Status | Implementation plan | Dependency / handoff note |
|---|---|---|---|---|
| Artifact discovery and cleanup | M001 foundation, CLI, config, domain | **closed** | plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md | Closure: plans/closure/artifact-discovery-cleanup/001-status.md. Hosted Linux/macOS/Windows matrix passed (run 37052261642). |
| Artifact discovery and cleanup | M002 fast discovery and scope filters | **closed** | plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md | Closure: plans/closure/artifact-discovery-cleanup/002-status.md. Hosted Linux/macOS/Windows matrix passed (run 37052261642). |
| Artifact discovery and cleanup | M003 activity, sizing, read-only report | **closed** | plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md | Closure: plans/closure/artifact-discovery-cleanup/003-status.md. Hosted matrix passed; user accepted the field observation. |
| Artifact discovery and cleanup | M004 revalidated cleanup execution | **closing** | plans/implementation/artifact-discovery-cleanup/004-revalidated-cleanup-execution.md | Implementation is committed locally; hosted Linux/macOS/Windows CI is pending. |

## Immediate handoff

M001-M003 are closed. M004 is closing with explicit-root, dry-run-first, ownership-checked Cargo cleanup. M005 remains deferred pending M004 closure and ownership design.

Expected implementation order:

~~~text
M001 -> closed
M002 -> closed
M003 -> closed (user field review accepted)
M004 -> closing (hosted CI pending)
M005 redirected/shared output remains deferred
~~~

The read-only boundary is intentional. A coding agent working M001-M003 must not add cargo clean, remove_dir_all, trash integration, or another deletion path as opportunistic scope.
