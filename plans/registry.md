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
| Artifact discovery and cleanup | active | plans/subsystems/artifact-discovery-cleanup-roadmap.md | M003 conditionally closed | Local read-only implementation and real-tree observation complete. Hosted Linux/macOS/Windows CI and human field review are still required before M004 planning. |

## Implementation handoffs

| Subsystem | Milestone | Status | Implementation plan | Dependency / handoff note |
|---|---|---|---|---|
| Artifact discovery and cleanup | M001 foundation, CLI, config, domain | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md | Closure: plans/closure/artifact-discovery-cleanup/001-status.md. Hosted platform matrix is pending. |
| Artifact discovery and cleanup | M002 fast discovery and scope filters | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md | Closure: plans/closure/artifact-discovery-cleanup/002-status.md. Platform qualification is pending. |
| Artifact discovery and cleanup | M003 activity, sizing, read-only report | **conditionally closed** | plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md | Closure: plans/closure/artifact-discovery-cleanup/003-status.md. Field review and hosted platform qualification are pending. |
| Artifact discovery and cleanup | M004 revalidated cleanup execution | **deferred** | not written | Do not plan/implement until M003 closes and read-only behavior has been exercised on real systems. |

## Immediate handoff

M001-M003 are implemented with conditional closure records. M004 remains deferred pending hosted CI and real-world read-only field review.

Expected implementation order:

~~~text
M001 -> conditional closure (CI pending)
M002 -> conditional closure (CI pending)
M003 -> conditional closure (CI and field review pending)
read-only field qualification
then consider M004 planning
~~~

The read-only boundary is intentional. A coding agent working M001-M003 must not add cargo clean, remove_dir_all, trash integration, or another deletion path as opportunistic scope.
