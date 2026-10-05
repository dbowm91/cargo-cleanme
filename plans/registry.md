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
| Distribution, release, and update | open / post-release corrective | plans/subsystems/distribution-release-update-roadmap.md | C016 live self-update identity invocation | Phase 10 remains closed. C013, C014 (partly), and C015 are closed; C014 stays open only because its live rehearsal failed, which C016 owns. v0.1.3 carries the C015 and C016 fixes. C010 remains an upstream request that nothing here waits on. |

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
| Distribution, release, and update | M010C Eggup self-update + install provenance | **closed, with a recorded defect discharged by C011** | plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md | Closure: plans/closure/distribution-release-update/010c-status.md (addendum). The transport-selection requirement was **not** satisfied when written: `eggup-curl` cannot set a User-Agent, so crates.io returned 403. Provenance, identity, and ownership invariants are unaffected and stand. |
| Distribution, release, and update | M010D publication + operational polish + release closure | **closed** | plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md | Closure: plans/closure/distribution-release-update/010d-status.md. Both halves complete. v0.1.0 and v0.1.1 published; run `37232595211` green 20/20; v0.1.1 crate checksum `8dfb3d18f648…`. One requirement is closed partial: the live updater commit path, which needs a v0.1.2 to rehearse. |
| Distribution, release, and update | C011 self-update transport qualification corrective | **closed** | plans/implementation/distribution-release-update/c011-self-update-transport-qualification-corrective.md | Closure: plans/closure/distribution-release-update/c011-status.md. Implementation `95bb38e` (tag `v0.1.1`). Reversed the transport to `eggup-eggfetch` and added the missing no-downgrade guard. |
| Distribution, release, and update | C012 Windows installer-fixture corrective | **closed** | plans/implementation/distribution-release-update/c012-windows-installer-fixture-corrective.md | Closure: plans/closure/distribution-release-update/c012-status.md. Implementation `ab4b080`. The Windows case had never executed; it was green on an unrelated real-cargo failure. Fixed with a `cargo.cmd` shim, `os.pathsep`, and a premise check. CI `37235169841` green 9/9. |
| Distribution, release, and update | C013 cross-platform fixture-premise audit | **closed** | plans/implementation/distribution-release-update/c013-cross-platform-fixture-premise-audit.md | Closed by `51d70f1` and `plans/closure/distribution-release-update/c013-status.md`. 25-row fixture inventory; nine evidence defects corrected; every new premise guard proven to fail on a broken setup; hosted Linux/macOS/Windows green in run `37242665909`. Discharges C012's open sweep item. |
| Distribution, release, and update | C014 v0.1.2 live update + release reproducibility | **open** | plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md | v0.1.2 published and qualified; every reproducibility requirement met. The live `v0.1.1` -> `v0.1.2` self-update rehearsal **failed** and handed C016 the defect. Not closed: publication alone does not close this plan. |
| Distribution, release, and update | C015 relative scan root Cargo resolution | **closed** | plans/implementation/distribution-release-update/c015-relative-scan-root-cargo-resolution-corrective.md | Closed by `1316e81`; record in `plans/closure/distribution-release-update/c015-status.md`. Both required tests fail without the fix. Ships in v0.1.3. |
| Distribution, release, and update | C016 live self-update identity invocation | **ready** | plans/implementation/distribution-release-update/c016-live-self-update-identity-invocation-corrective.md | Product defect found by C014's live rehearsal: the identity check ran the candidate with no arguments, so it could never match and scanned the whole machine first. Never worked in any published version. Fix applied and gated locally; needs a published release to close. |
| Distribution, release, and update | C010 upstream: `CurlConfig::user_agent` seam | **proposed** | plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md | Bounded upstream Eggup request. No closure record: it cannot be closed from this repository, and nothing here is waiting on it. |

## Current handoff

Phase 10 is closed in full. M010A established the Eggpack producer contract and the Cargo package-readiness boundary; M010B added the public wrappers and their per-platform fixture qualification; M010C added bounded self-update on the published Eggup crates with a real install-provenance policy; M010D added the operational surface and performed the publication.

Publication is complete. `v0.1.0` and `v0.1.1` are public GitHub releases and `cargo-cleanme 0.1.0` and `0.1.1` are on crates.io, neither yanked. A registry-only `cargo install --locked` resolves with zero non-registry sources, the public installer over real HTTPS installs bytes that match the published release digest, and all five release targets were built, qualified, and validated in one green workflow run.

Publication was not a formality. The external smoke of `v0.1.0` found that `cargo cleanme update` returned HTTP 403 from crates.io on every host, because the production transport cannot set a `User-Agent`. Corrective **C011** replaced the transport and added the missing no-downgrade guard; it shipped as `v0.1.1`. M010C's closure record was **not** edited to imply it succeeded — it carries an addendum naming the unsatisfied requirement.

The lesson from three findings is recorded rather than left implicit: a green
test proves only that its own premises hold, and both "the code is exercised"
and "the code ran" are premises that go unchecked by default. C009 was a
POSIX-only fixture passing on no platform. C011 was a feature whose transport
could not reach its version authority, green because the fixture registry
always answered. C012 was a Windows installer case that had never executed and
was green only because an unrelated real `cargo install` failed — and it was
disclosed by the product starting to work correctly.

The general form: a fixture that stubs a tool the subject cannot invoke is
indistinguishable from no stub, and a case whose pass condition is an unrelated
failure reports coverage that does not exist. Correctives C011 and C012 both
landed a premise assertion — the transport's User-Agent, the stub's
resolvability — rather than only fixing the symptom.

The post-release corrective line is now sequenced. **C013** is closed: its repository-wide sweep of external-process fixtures recorded a 25-row inventory, corrected nine evidence defects, and proved every new premise guard in the failing direction. The sweep's most serious finding was not a test at all — the release-candidate smoke validator was green on all three hosted lanes while the subject under test was failing — and tightening it exposed a real product defect, now registered as **C015**. C013 was closed without repairing any production defect, by its own failure semantics. **C014** published v0.1.2 with every reproducibility requirement met — the exact `1.99.0` release builder that qualified v0.1.1, a machine-checked tag/source identity gate, and a release whose public bytes are provably the qualified bytes. It is **not closed**: its live `v0.1.1` -> `v0.1.2` updater rehearsal failed, and the failure is a real defect. **C016** owns it — the identity check ran the downloaded candidate with no arguments, so its stdout could never equal a version string, and before failing it performed a filesystem scan of the whole machine. The transaction aborted safely every time, so nothing shipped is corrupt, but the advertised feature has never once worked in any published version. The fix is applied and gated locally; a published release must carry it before the rehearsal can pass, because both `v0.1.1` and `v0.1.2` are immutable and both carry the broken updater. **C015** is closed: a relative scan root is anchored for Cargo, both required tests fail against the unfixed code, and the two spellings now produce identical counters, groups, and bytes. It failed toward *less* reported reclaimable space, so it did not block v0.1.2 publication, and it is disclosed there and fixed in v0.1.3. **C010** remains a bounded upstream request for a `User-Agent` seam on `eggup-curl`; it is `proposed`, upstream may accept or decline it, and cargo-cleanme waits on none of it because the production updater uses Eggfetch.

No secret was created or stored; publication used the operator's pre-existing `gh` and crates.io credentials. Eggpack required no upstream implementation plan because its existing producer contract, manifest, bootstrap, CI-generation, drift, and draft-staging surfaces already cover cargo-cleanme's direct single-binary release.

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
M010D publication + operational polish + release closure -> closed
C011 self-update transport qualification corrective -> closed
C012 Windows installer-fixture corrective -> closed
C013 cross-platform fixture-premise audit -> closed (discharges C012's open sweep item)
C015 relative scan root Cargo resolution -> closed (1316e81; ships in v0.1.3)
C014 v0.1.2 live update/release reproducibility -> OPEN: v0.1.2 published and qualified; live rehearsal failed
C016 live self-update identity invocation -> registered by C014; owns the fix and the remaining live evidence
C010 upstream CurlConfig::user_agent seam -> proposed (upstream; nothing waits on it)

Published: v0.1.0, v0.1.1, v0.1.2 (GitHub releases and crates.io)
Next release: v0.1.3 under C016, carrying the self-update fix
~~~

Phase 10 remains closed. C013/C014/C015/C016 are post-release correctives and do not reopen the feature phase or alter the destructive cleanup boundary. C013 is closed. C014 published v0.1.2 and stays open because its live rehearsal failed. C015 and C016 are the two product defects this line found. C015 is closed; C016's fix is applied and awaiting a release to carry it. Three tags/releases and three crate versions are published; v0.1.1 and v0.1.2 both ship a self-update command that cannot complete a commit, which is recorded rather than hidden.

### Current destructive safety boundary

- Cargo resolves workspace/output configuration; cargo-cleanme does not infer unresolved output paths.
- Every Cargo manifest discovered under the selected cleanup root set must be authoritatively covered before any Cargo clean command may run.
- Unresolved ownership participants block Preview, Simulate, and Execute for the complete selected-root scope.
- The destructive unit is one workspace CleanupUnit over its complete OutputSet.
- Every affected physical group must be PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under fresh complete combined ownership-graph revalidation.
- cleanup.allowed_output_roots never manufactures ownership proof.
- --dryrun remains zero-clean and shares the same completeness/final-proof gates as Execute.
- --stats is the qualification/debug surface.
