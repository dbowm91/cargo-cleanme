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
M008 planning baseline: 3ee9699a0b0d987287d08e427195284e08d079f7

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
| Artifact discovery and cleanup | closed/current objectives | plans/subsystems/artifact-discovery-cleanup-roadmap.md | Phase 9 closed; C009 corrective closed | C008 closed the cleanup feature roadmap's current objectives; C009 then restored the green hosted Windows lane. Phase 10 is owned by the separate distribution/release/update subsystem. |
| Distribution, release, and update | M010A-M010C closed; M010D publication half blocked | plans/subsystems/distribution-release-update-roadmap.md | M010D publication + operational polish + release closure | M010A, M010B, and M010C are closed. The Eggup curl dependency is discharged. The only remaining Phase 10 work is the publication sequence in the M010D closure record, which needs registry authentication and an explicit human decision to publish. |

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
| Artifact discovery and cleanup | M006E adaptive Routine/Full discovery + state/config UX | **closed** | plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md | Historical closure: plans/closure/artifact-discovery-cleanup/006e-status.md. Corrective closure: plans/closure/artifact-discovery-cleanup/c005-status.md; localized uncertainty now reconciles conservatively and publishes a useful generation. |
| Artifact discovery and cleanup | M006E old scope/latency decision | **superseded** | plans/implementation/artifact-discovery-cleanup/006e-global-scan-scope-and-latency-policy.md | Replaced by ADR 002 and the adaptive-discovery M006E plan. |
| Artifact discovery and cleanup | M006F exhaustive traversal hot-path qualification | **closed** | plans/implementation/artifact-discovery-cleanup/006f-exhaustive-traversal-hot-path-qualification.md | Closure: plans/closure/artifact-discovery-cleanup/006f-status.md. Hosted Linux/macOS/Windows and Rust 1.89 CI passed in run 37139155641. |
| Artifact discovery and cleanup | M007 learned/full cleanup orchestration | **closed** | plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md | Corrective closure: plans/closure/artifact-discovery-cleanup/c006-status.md. Combined selected-root proof, known/full orchestration, mode parity, zero-clean simulation, and platform/MSRV gates pass. Historical blocked record is unchanged. |
| Artifact discovery and cleanup | C005 uncertainty-aware state reconciliation + config/edit hardening | **closed** | plans/implementation/artifact-discovery-cleanup/c005-uncertainty-aware-state-reconciliation-and-config-edit-hardening.md | Closure: plans/closure/artifact-discovery-cleanup/c005-status.md. Localized Full uncertainty reconciles conservatively; isolated real Full published schema 2; hosted platform/MSRV gates pass. |
| Artifact discovery and cleanup | C006 combined-root cleanup ownership universe | **closed** | plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md | Closure: plans/closure/artifact-discovery-cleanup/c006-status.md. Combined discovery/coverage/ownership proof and final per-candidate refresh pass hosted platform/MSRV gates. |
| Artifact discovery and cleanup | C007 discovery-state recovery + post-M007 planning cleanup | **closed** | plans/implementation/artifact-discovery-cleanup/c007-discovery-state-recovery-and-post-m007-planning-cleanup.md | Closure: plans/closure/artifact-discovery-cleanup/c007-status.md. Hosted platform/MSRV run 37152008602 passed. |
| Artifact discovery and cleanup | M008A workspace selective cleanup policy | **closed** | plans/implementation/artifact-discovery-cleanup/008a-workspace-selective-cleanup-policy.md | Closure: plans/closure/artifact-discovery-cleanup/008a-status.md. Fresh policy races, excluded shared owner, all-mode zero-spawn assertions, Rust 1.89, and hosted platform CI passed. |
| Artifact discovery and cleanup | M008B machine-readable reporting + automation contract | **closed** | plans/implementation/artifact-discovery-cleanup/008b-machine-readable-reporting-and-automation-contract.md | Closure: plans/closure/artifact-discovery-cleanup/008b-status.md. Typed subreason codes, selector/report DTOs, unattended fake-Cargo Execute, JSON scope metadata, Rust 1.89, and hosted CI passed. |
| Artifact discovery and cleanup | M008C Cargo profile/package selective cleanup qualification | **closed by subsequent M008D evidence** | plans/implementation/artifact-discovery-cleanup/008c-cargo-profile-package-selective-cleanup-qualification.md | Historical conditional closure: plans/closure/artifact-discovery-cleanup/008c-status.md. M008D satisfies the deferred package line: plans/closure/artifact-discovery-cleanup/008d-status.md. Exact Cargo release limits remain in force. |
| Artifact discovery and cleanup | M008D Cargo package-selector qualification and enablement | **closed** | plans/implementation/artifact-discovery-cleanup/008d-cargo-package-selector-qualification-and-enablement.md | Closure: plans/closure/artifact-discovery-cleanup/008d-status.md. Exact Cargo 1.98.1/1.99.0 behavior, package identity validation, unknown-version failure, and hosted platform/MSRV gates pass. |
| Artifact discovery and cleanup | C009 Windows test-fixture portability | **closed** | plans/implementation/artifact-discovery-cleanup/c009-windows-test-fixture-portability.md | Closure: plans/closure/artifact-discovery-cleanup/c009-status.md. Restored the green hosted Windows lane (run `37225697173`) that every Phase 10 milestone needs for evidence. |
| Artifact discovery and cleanup | C008 Phase 9 closure + planning reconciliation | **closed** | plans/implementation/artifact-discovery-cleanup/c008-phase-9-closure-and-planning-reconciliation.md | Closure: plans/closure/artifact-discovery-cleanup/c008-status.md. Phase 9 canonical state reconciled. |
| Distribution, release, and update | M010A Eggpack distribution contract + package readiness | **closed** | plans/implementation/distribution-release-update/010a-eggpack-distribution-contract-and-package-readiness.md | Closure: plans/closure/distribution-release-update/010a-status.md. Implementation `b4662af`; no publication occurred. Its one green-CI qualification was discharged by C009. |
| Distribution, release, and update | M010B bootstrap installers + release artifact qualification | **closed** | plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md | Closure: plans/closure/distribution-release-update/010b-status.md. Wrappers + 17 fixture cases green on hosted Linux/macOS/Windows. Runtime qualification of real release bytes is open and is M010D step 7. |
| Distribution, release, and update | M010C Eggup self-update + install provenance | **closed** | plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md | Closure: plans/closure/distribution-release-update/010c-status.md. `eggup-curl 0.1.2` is published and was proven by resolving it, not by a search result. Measured cost +449,752 bytes (+9.0%) with zero transitive transport dependencies. |
| Distribution, release, and update | M010D publication + operational polish + release closure | **blocked (publication half)** | plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md | Closure: plans/closure/distribution-release-update/010d-status.md. Completions/manpage drift gate, benchmark baseline, release checklist, support policy, and troubleshooting are complete at `3e5f3ab`. Publication needs registry auth plus an explicit human decision; the ordered residual sequence is in the closure record. |

## Current handoff

Phase 10's engineering scope is complete. M010A established the Eggpack producer contract and the Cargo package-readiness boundary; M010B added the public wrappers and their per-platform fixture qualification; M010C added bounded self-update on the published Eggup crates with a real install-provenance policy. M010D's repository-internal scope — generated completions and manpages with a drift gate, the benchmark baseline, the operator release checklist, the support policy, and troubleshooting — is also complete.

The remaining Phase 10 work is publication, and it is not blocked on any repository-side gap. It is blocked on two things a planning session must not decide unilaterally: crates.io registry authentication, and the irreversible public publication of a crate and a GitHub release. The load-bearing dependency is structural rather than informational: the plan forbids publishing a target that lacks runtime evidence, and that evidence only exists once a release workflow run has produced a staged draft. The full ordered sequence is in `plans/closure/distribution-release-update/010d-status.md`.

Nothing was published, no tag was created, and no secret was created or stored. Eggpack required no upstream implementation plan because its existing producer contract, manifest, bootstrap, CI-generation, drift, and draft-staging surfaces already cover cargo-cleanme's direct single-binary release.

Milestone sequence:

~~~text
M001-M005 + C001-C004 -> closed
M006A/C/D historical conditional evidence -> retained
M006B/M006E/M006F -> closed
C005 -> closed
C006 -> closed
M007 -> closed by C006
C007 discovery-state recovery + post-M007 planning cleanup -> closed
M008A workspace selective cleanup policy -> closed
M008B machine-readable reporting/automation contract -> closed
M008C Cargo profile/package selective cleanup -> closed by M008D evidence; historical conditional closure retained
M008D Cargo package selector -> closed
C008 Phase 9 closure/planning reconciliation -> closed
M010A Eggpack distribution contract/package readiness -> closed
C009 Windows test-fixture portability -> closed (restores per-platform evidence)
M010B bootstrap installers/release qualification -> closed
M010C Eggup self-update + install provenance -> closed
M010D publication + operational polish -> publication half blocked, internal half complete
M010D publication -> BLOCKED on registry auth + an explicit human decision to publish
~~~

Phase 10 distribution is explicitly activated, M010A-M010C are closed, and M010D's repository-internal scope is complete. This work does not alter the destructive cleanup boundary: the distribution milestones touch release, packaging, and update paths only, and corrective C009 touched test fixtures alone. No publication, no tag, and no release were created.

### Current destructive safety boundary

- Cargo resolves workspace/output configuration; cargo-cleanme does not infer unresolved output paths.
- Every Cargo manifest discovered under the selected cleanup root set must be authoritatively covered before any Cargo clean command may run.
- Unresolved ownership participants block Preview, Simulate, and Execute for the complete selected-root scope.
- The destructive unit is one workspace CleanupUnit over its complete OutputSet.
- Every affected physical group must be PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under fresh complete combined ownership-graph revalidation.
- cleanup.allowed_output_roots never manufactures ownership proof.
- --dryrun remains zero-clean and shares the same completeness/final-proof gates as Execute.
- --stats is the qualification/debug surface.
