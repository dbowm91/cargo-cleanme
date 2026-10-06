# cargo-cleanme Long-Term Roadmap

Status: canonical ordered roadmap

This roadmap decomposes the product specification into dependency-ordered phases. Milestone implementation plans under plans/implementation/ are bounded handoff artifacts and may evolve without rewriting this roadmap.

## Phase 0 — Planning and contracts

Status: complete for initial implementation handoff.

Deliverables:

- canonical product specification;
- canonical terminology;
- planning governance;
- active registry;
- artifact-discovery/cleanup subsystem roadmap;
- bounded M001-M003 implementation plans.

Exit condition:

- V0.1 behavior is specified without requiring destructive implementation.

## Phase 1 — Foundation, CLI, configuration, and domain contracts

Roadmap milestone: M001.

Objective:

Create a small Rust crate/binary foundation whose public behavior can support the scanner without prematurely coupling CLI, config, traversal, and reporting.

Deliverables:

- cargo-cleanme binary usable as cargo cleanme;
- default no-argument read-only scan command surface;
- typed configuration and path validation;
- platform config location, automatic first-use config bootstrap, config path/show/edit commands;
- domain records for scan scope, discovered project, activity result, artifact analysis, diagnostic, and report;
- library/CLI separation suitable for deterministic tests;
- Rust 1.89+ / Rust 2024 toolchain contract;
- baseline CI and lint/test gates.

Exit condition:

- configuration precedence and CLI behavior are testable without real machine-wide traversal;
- no destructive function exists.

## Phase 2 — Fast discovery and search-scope filtering

Roadmap milestone: M002.

Objective:

Find conventional Cargo project/target pairs quickly while pruning irrelevant filesystem work.

Deliverables:

- traversal adapter with dua-core 4.1 as the preferred implementation;
- global vs explicit scope resolution;
- absolute ignore glob matcher;
- exact unignore path semantics with ancestry-preserving traversal;
- platform safety pruning;
- symlink refusal;
- Cargo.toml + direct target discovery;
- nested/workspace duplicate behavior;
- bounded diagnostics and permission-error handling;
- fixture and performance-regression coverage.

Exit condition:

- discovery can enumerate candidate project roots without recursively sizing target trees;
- root/filter precedence exactly matches the specification.

## Phase 3 — Activity analysis, sizing, and V0.1 reporting

Roadmap milestone: M003.

Objective:

Turn discovered candidates into trustworthy inactive-project inventory results.

Deliverables:

- fixed scan-start cutoff;
- source activity short-circuit;
- target activity plus artifact-presence and size analysis;
- allocated/on-disk size metric with documented platform fallback;
- deterministic size-descending report;
- summary totals and diagnostic summary;
- optional machine-readable report if it does not expand the milestone materially;
- end-to-end Linux/macOS/Windows fixture qualification;
- representative real-tree performance evidence.

Exit condition:

- cargo cleanme can be used safely as a read-only inventory utility;
- no active project within the guard window appears as eligible in qualification fixtures;
- no deletion or cargo clean invocation exists.

## Phase 4 — V0.1 release and field qualification

Status: closed sufficiently to unblock destructive development; M003/C001 and subsequent hosted/real-tree qualification provide the historical field evidence.

Objective:

Ship and observe the read-only scanner before adding cleanup.

Expected work:

- packaging/release workflow;
- install documentation;
- cross-platform binaries if warranted;
- real-machine validation on large developer trees;
- performance and diagnostic polish;
- config migration policy once real usage exists.

Exit condition:

- the scanner has enough operational evidence to freeze its eligibility contract.

## Phase 5 — Revalidated Cargo cleanup execution

Roadmap milestone: M004.

Status: closed.

Objective:

Add a destructive command that cleans only freshly revalidated, ownership-safe candidates and reports actual recovered space.

Required direction:

- re-run recency checks immediately before cleanup;
- validate manifest/artifact ownership;
- use Cargo semantics rather than arbitrary recursive deletion;
- provide dry-run/confirmation policy;
- keep per-project failures isolated;
- measure post-clean storage;
- report actual recovered bytes;
- define interruption/cancellation behavior;
- preserve an audit-friendly per-candidate result.

Historical dependency:

Phase 4/read-only qualification; satisfied before M004 closure.

## Phase 6 — Redirected/shared Cargo output awareness

Roadmap milestone: M005.

Status: closed. M005A/M005B landed; C002-C004 preserve the historical corrections that restored ownership, cleanup-unit, and complete-resolution safety.

Objective:

Resolve Cargo workspaces and target/build output through Cargo itself, deduplicate physical output, inventory shared/external caches safely, reject expensive work as early as possible, provide immediate inline scan status, and then extend Cargo-mediated cleanup only to private authorized redirected output.

Accepted decision:

- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

Implementation sequence (closed):

1. M005A — workspace/output resolution, physical-output grouping, fail-fast scan pipeline, and inline progress/status.
2. M005B — authorized private redirected cleanup plus explicit `--dryrun` full simulation mode.
3. C002-C004 — post-M005 ownership/simulation/progress, CleanupUnit atomicity/full graph revalidation, and complete discovered-manifest ownership coverage.

Required cases:

- multi-member workspaces;
- .cargo/config.toml / environment target-dir resolved by Cargo;
- target/build directories equal or distinct;
- Cargo 1.89-1.90 runtime behavior vs Cargo 1.91+ build-dir capability;
- workspace-relative/path-template resolution delegated to Cargo;
- identical/nested/overlapping physical output;
- intentionally shared caches;
- external private output requiring explicit authorization;
- non-TTY progress suppression and deterministic final output.

Shared, uncertain, and externally unproven physical output remains inventory-only in M005.

## Phase 7 — Performance hardening and adaptive discovery

Roadmap milestone: M006.

Status: closed for current objectives. M006A/C/D retain their historical conditional closure records; they are not active blockers.

Objective:

Make routine read-only scanning proportional to the developer's actual working set while preserving explicit exhaustive discovery, and reduce cleanup-proof/traversal overhead without weakening M005/C002-C004 semantics.

Implementation sequence:

1. M006A — global scan pruning, platform traversal policy, one-pool multi-root discovery, and workspace member/output path normalization (conditionally closed).
2. M006B — cleanup proof resolution efficiency (closed).
3. M006C — bounded exhaustive traversal throughput without scope narrowing (conditionally closed).
4. M006D — exhaustive scan completion qualification and bottleneck evidence (conditionally closed).
5. M006E — adaptive Routine/Full discovery, machine-local learned state, 30-day configurable retention, first-run config bootstrap, and `config edit` (closed by C005).
6. M006F — exhaustive traversal hot-path qualification after the product scope split (closed).
7. C005 — uncertainty-aware state reconciliation and config/edit hardening (closed; M006E corrective closure).

Hard constraints:

- Routine roots are bounded seeds plus learned developer roots;
- Full discovery remains exhaustive under the established platform policy and can rediscover expired locations;
- Explicit-root behavior remains authoritative;
- only successful Full reconciliation may expire learned roots from absence/age;
- machine-learned state never supplies cleanup ownership/authorization evidence;
- cleanup proof still refreshes the complete bounded ownership universe immediately before each candidate disposition;
- no cross-candidate stale ownership snapshot is introduced.

Exit condition:

- C005 closes M006E with path-scoped uncertainty-aware Full reconciliation that publishes positive project evidence on real hosts, conservative learned-root expiration, and cross-platform config/state qualification;
- M006F remains closed with the bare-walker/adapter cost split, optional-attribution removal, traversal-default qualification, and completed reference Full traversal baseline;
- M006B remains green with zero redundant proof locate-project calls and the C003/C004 regression matrix intact.

The historical 120-second full-machine no-argument target is superseded by ADR 002. Routine and Full are separately qualified.

## Phase 8 — Learned/full cleanup orchestration

Roadmap milestone: M007.

Status: closed by corrective C006; C005 closed M006E and C006 generalized the cleanup proof across all selected roots. Post-M007 corrective C007 is closed.

Objective:

Let known/adaptive and exhaustive discovery select bounded cleanup scopes without turning cached discovery state into destructive proof.

Expected direction:

- retain existing `clean ROOT` semantics unchanged;
- add mutually exclusive known/full cleanup selection;
- require successful Full reconciliation before Full cleanup begins;
- construct one combined manifest-coverage and physical-output ownership universe from every selected known/full root;
- refresh that complete selected-root universe before each CleanupUnit disposition;
- preserve `--dryrun` as zero-`cargo clean` application simulation and `--dry-run` as Cargo preview, with the same combined safety disposition as Execute;
- deduplicate overlapping learned roots and physical outputs;
- preserve the existing single-root `clean ROOT` safety boundary through the same combined-capable engine.

Plans:

- `plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md`
- `plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md`
- post-closure corrective: `plans/implementation/artifact-discovery-cleanup/c007-discovery-state-recovery-and-post-m007-planning-cleanup.md`

Phase 8 has no remaining implementation work. Phase 9 is now decomposed into the dependency-ordered M008A-M008D implementation series.

## Phase 9 — Selective cleanup and policy

Roadmap milestone series: M008A-M008D.

Status: closed for current objectives. Closure/planning reconciliation: C008; evidence: `plans/closure/artifact-discovery-cleanup/c008-status.md`.

Objective:

Phase 9 added cleanup selection policy, stable automation contracts, and narrowly qualified Cargo profile/package selectors without weakening the complete Cargo ownership proof. This phase is closed for the currently qualified capabilities; future Cargo releases require separate qualification.

Implementation sequence:

1. M008A — workspace selective cleanup policy:
   - minimum reclaimable size;
   - minimum inactivity age using newest trustworthy source/output activity;
   - canonical workspace include/exclude policy;
   - typed policy dispositions;
   - fresh final-proof size/age revalidation.
2. M008B — machine-readable reporting and automation contract:
   - versioned deterministic JSON for scan and cleanup;
   - stable policy/safety/action reason codes;
   - documented process exit semantics;
   - supported non-interactive execution contract without adding a scheduler/daemon.
3. M008C — Cargo profile/package selector qualification (historical conditional closure; now closed by subsequent M008D evidence): profile selection was enabled for exact qualified Cargo releases; package selection was deferred at that closure point.
4. M008D — Cargo package-selector qualification and enablement: package selection is enabled for exact Cargo 1.98.1 and 1.99.0 after identity, shared-dependency, and configured-target qualification.

Hard constraints:

- policy filtering occurs after complete manifest/workspace/output resolution; policy-rejected workspaces remain in the ownership universe;
- the destructive unit remains the complete workspace CleanupUnit; any qualified narrower Cargo selector cannot weaken complete ownership proof;
- M008A policy cannot promote Shared, Uncertain, ExternalUnproven, unresolved, unauthorized, active, symlinked, or marker-invalid output;
- dynamic size/age thresholds are re-evaluated from fresh final-proof observations before Cargo execution;
- machine-readable consumers never need to parse human detail text to determine disposition;
- cargo-cleanme remains a command-line tool; scheduling/load policy remains the responsibility of external orchestrators;
- package/profile cleanup must delegate selection to Cargo and must not parse Cargo-private artifact layout.
- profile selection is enabled only on exact qualified releases 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0;
- package selection is enabled only on exact Cargo 1.98.1 and 1.99.0; unqualified and future versions fail closed;
- selector-specific reclaimable bytes remain unknown, and any nonzero minimum-size policy with a selector fails closed;
- package cleanup may remove Cargo-shared dependency artifacts;
- unattended operation uses an external scheduler; cargo-cleanme has no daemon.

Plans:

- `plans/implementation/artifact-discovery-cleanup/008a-workspace-selective-cleanup-policy.md`
- `plans/implementation/artifact-discovery-cleanup/008b-machine-readable-reporting-and-automation-contract.md`
- `plans/implementation/artifact-discovery-cleanup/008c-cargo-profile-package-selective-cleanup-qualification.md`
- `plans/implementation/artifact-discovery-cleanup/008d-cargo-package-selector-qualification-and-enablement.md`

Exit condition:

- M008A, M008B, M008C, and M008D are closed with hosted platform/MSRV evidence;
- enabled profile/package selectors have real-Cargo qualification evidence and selector-specific accounting is explicitly unknown;
- unsupported fine-grained selectors remain fail-closed rather than weakening existing whole-workspace cleanup;
- existing C003/C004/C006 ownership and freshness regressions remain green.

## Phase 10 — Distribution and operational polish

Status: **closed.** M010A-M010D are closed. The post-release corrective line
C013-C017 is also closed, and seven public versions (`v0.1.0` through
`v0.1.6`) record the evidence trail rather than rewriting history. C010 remains
a bounded upstream `eggup-curl` request and nothing in cargo-cleanme waits on it.

Subsystem roadmap:

- `plans/subsystems/distribution-release-update-roadmap.md`

Closed implementation sequence:

1. M010A — Eggpack distribution contract and Cargo package readiness.
2. M010B — product-owned bootstrap installers and release-artifact qualification.
3. M010C — Eggup-backed self-update and explicit install-provenance policy.
4. M010D — crates.io/GitHub publication, completions/manpage, benchmark/support evidence, and release closure.
5. C013 — cross-platform fixture-premise audit.
6. C014 — exact release-toolchain/source identity plus live updater rehearsal.
7. C015 — relative scan-root Cargo resolution corrective.
8. C016 — live self-update candidate-identity invocation corrective.
9. C017 — Cargo-managed provenance misdetection corrective.

Phase 10's durable direction remains:

- Eggpack owns producer-side release construction/qualification/generated CI;
- Eggup owns generic verified acquisition/staging/replacement/rollback;
- cargo-cleanme owns release/version/origin/fallback/provenance policy and public presentation;
- publication remains explicit after qualification;
- release artifacts, installers, updater behavior, and operational docs require evidence rather than build success alone.

Historical failures are retained in their original closure records. In
particular, `v0.1.1`/`v0.1.2` carry the C016 updater defect and
`v0.1.1`-`v0.1.4` carry the C017 Cargo-ownership defect; later releases fix
them without rewriting immutable history.

## Phase 11 — Hardening, release trust, and qualification automation

Status: **closed for implementation.** C018, C019, and M011A-M011D are implemented/closed; M011A-M011C and C018 still require the already-recorded future published-release operational evidence. This phase hardens the already-shipped product before new
feature expansion. It does not reopen Phase 10 and does not widen the destructive
cleanup boundary.

Implementation sequence / parallelism:

1. **C018 — self-update provenance uncertainty fail-closed** is an immediate
   safety-critical corrective. Positive Cargo-manager evidence that cannot be
   interpreted must never collapse to self-managed ownership.
2. **C019 — exact unignore sibling containment** is an independent immediate
   discovery-scope corrective against M002. Traversal through an ignored
   ancestor may reach an exact exception but must not admit unrelated siblings.
3. **M011A — immutable release attestation and verification** adds a
   post-publication authenticity layer while retaining SHA-256/manifest checks.
4. **M011B — staged-release validation workflow gate** makes the existing
   staged-draft validator an actual hosted publication prerequisite without
   modifying Eggpack-generated CI.
5. **M011C — published-release smoke automation** makes the five-target live
   updater rehearsal automatic on public release publication while retaining
   manual dispatch.
6. **M011D — Cargo selector qualification lifecycle** binds exact runtime
   profile/package support to repeatable real-Cargo evidence and keeps future
   versions fail-closed until deliberately qualified.

C018, C019, and M011A-M011D may be implemented in parallel where their files do
not conflict. The first future release after the relevant implementations supplies
operational closure evidence for C018 and M011A-M011C; planning does not reserve
a version number in advance. M011D closes independently on its hosted
qualification matrix.

Hard constraints:

- no hardening work may weaken ADR 001 cleanup ownership/freshness proof;
- manager/provenance uncertainty fails closed rather than becoming permission to
  replace a binary;
- do not hand-edit the Eggpack-generated release workflow to add product-specific
  signing/validation steps;
- immutable-release attestation must be described with its real GitHub trust
  boundary, not as an independent maintainer signature;
- SLSA/per-build/SBOM/independent-signing work requires Eggpack Phase 12 and its
  authenticity ADR threshold;
- staged validation and published smoke remain non-publishing, least-privilege
  evidence jobs;
- live-network evidence is never silently replaced by fixture evidence;
- Cargo selector support remains exact-version and fail-closed; exploratory
  current-stable qualification never automatically enables support;
- historical releases and closure records are immutable evidence.

Plans:

- `plans/implementation/distribution-release-update/c018-self-update-provenance-uncertainty-fail-closed.md`
- `plans/implementation/artifact-discovery-cleanup/c019-exact-unignore-sibling-containment-corrective.md`
- `plans/implementation/distribution-release-update/011a-immutable-release-attestation-and-verification.md`
- `plans/implementation/distribution-release-update/011b-staged-release-validation-workflow-gate.md`
- `plans/implementation/distribution-release-update/011c-published-release-smoke-automation.md`
- `plans/implementation/artifact-discovery-cleanup/011d-cargo-selector-qualification-lifecycle.md`

Exit condition:

- C018 is carried by a public release and real Cargo-managed installations
  refuse without remote acquisition or mutation when local provenance forbids
  replacement;
- C019 restores exact unignore sibling containment with a premise-negative
  literal-ancestor regression and hosted cross-platform evidence;
- a future public release is immutable and its release attestation/assets verify
  under the documented trust model;
- a real Eggpack staged draft automatically receives the full product staged
  validator before human publication;
- publication automatically launches the five-target real updater smoke after a
  bounded crates.io-authority wait;
- every enabled Cargo selector version is represented by current hosted
  real-Cargo qualification evidence;
- no medium-or-higher finding remains in these five hardening scopes.

## Phase 12 — Canonical maintenance UX and unattended operation

Status: **closed.** M012A and M012B are implemented and closed under ADR 003.
The public front door changed while the existing destructive
ownership/freshness boundary remained intact. `Cargo.toml` is now 0.2.0, so
the breaking bare-invocation change cannot ship as a 0.1.x patch.

Post-phase release-readiness corrective: **C021 ready** —
`plans/implementation/artifact-discovery-cleanup/c021-pre-release-test-and-verification-evidence-reconciliation.md`.
C021 does not reopen Phase 12; it reconciles a flaky updater-test premise,
verification-inventory drift, changelog completeness, and the final hosted
pre-release baseline before 0.2.0 staging.

Decision:

- `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Implementation sequence:

1. **M012A — canonical maintenance CLI and dry-run semantics**
   - bare `cargo cleanme` becomes Routine Execute through the existing combined-root cleanup engine;
   - bare/cleanup `--dry-run` becomes zero-`cargo clean` application simulation;
   - Cargo's own preview moves to explicit `--cargo-preview`;
   - advanced `clean ROOT|--known|--full` remains available and executes by default;
   - rootless `cargo cleanme scan` becomes Full read-only reconciliation;
   - `scan ROOT` remains Explicit and `scan --known` preserves Routine read-only inventory;
   - historical `--yes`, `--dryrun`, and `scan --full` may remain hidden compatibility aliases if they do not create ambiguous parser/safety behavior;
   - the first release carrying the bare destructive default must cross a pre-1.0 minor boundary rather than ship as a `0.1.x` patch.

2. **M012B — bounded unattended log output and greggd integration**
   - add explicit `--format log`;
   - emit one deterministic ASCII summary line, bounded to 384 bytes, for scan/cleanup reports;
   - disable progress and ordinary diagnostic fan-out in log mode while preserving exit status;
   - retain JSON as the complete machine-readable contract;
   - document daily Routine cleanup and weekly Full reconciliation under greggd load gating;
   - recommend a user-owned/rootless greggd instance for developer-home maintenance and absolute argv paths;
   - record the downstream Gregg stale `scan --deep` documentation correction without making cargo-cleanme closure depend on another repository.

Plans / closure:

- `plans/implementation/artifact-discovery-cleanup/012a-canonical-maintenance-cli-and-dry-run-semantics.md`
- `plans/closure/artifact-discovery-cleanup/m012a-status.md`
- `plans/implementation/artifact-discovery-cleanup/012b-unattended-log-output-and-greggd-integration.md`
- `plans/closure/artifact-discovery-cleanup/m012b-status.md`
- post-phase corrective: `plans/implementation/artifact-discovery-cleanup/c021-pre-release-test-and-verification-evidence-reconciliation.md`

Hard constraints:

- ADR 001 ownership classes and authorization remain unchanged;
- C003/C004/C006 complete manifest coverage and fresh combined ownership proof remain unchanged;
- learned discovery state never becomes cleanup evidence;
- every `scan` remains read-only;
- `--dry-run` must invoke zero Cargo clean processes;
- log mode may not add another filesystem/Cargo pass merely to decorate output;
- cargo-cleanme does not gain a scheduler, daemon, load policy, or persistent job history;
- schema-v1 field meanings may not be silently redefined to fit the new CLI;
- the breaking bare-invocation change must be prominent at the release boundary.

Exit condition:

- M012A closes with premise-negative CLI/spawn evidence and all existing destructive safety regressions green;
- M012B closes with bounded-output tests, JSON/human compatibility evidence, generated CLI artifacts, and automation documentation;
- no medium-or-higher finding remains in the new default execution path or unattended-output path.

## Dependency summary

~~~text
Phase 0 -> M001 -> M002 -> M003 -> read-only qualification
                                  |
                                  v
M004 -> M005 -> M006 -> M007 -> M008A-D -> Phase 9 [closed]
                                              |
                                              v
Phase 10 distribution/release + C013-C017 [closed]
                                              |
             +--------------------------------+------------------+
             |                |               |                  |
             v                v               v                  v
C018 provenance        M011A immutable  M011B staged      M011C published
fail-closed [ready]    release trust    validation gate   smoke automation
                       [ready]          [ready]           [ready]

C019 exact-unignore sibling containment [ready; independent corrective]
M011D Cargo selector qualification lifecycle [ready; independent evidence line]

First future qualification release:
  carries C018 + supplies M011A/M011B/M011C operational evidence
                                              |
                                              v
ADR 003 -> M012A canonical CLI [closed] -> M012B unattended log/docs [closed]
                                              |
                                              v
                                C021 pre-release evidence [ready]
                                              |
                                              v
                                  first 0.2.0 release
~~~

Phase 12 is implemented and C021 is closed. The first release carrying M012A's
bare destructive default is 0.2.0. Phase 13 now owns its remaining release
gates: C022 must close before staging, then M013 carries the exact release
through publication and finishes the outstanding C018/M011A/M011B/M011C
operational evidence.


Destructive work MUST NOT be pulled forward merely to make the tool feel complete.

## Phase 13 — 0.2.0 release qualification and publication

Status: **ordered handoff**.

0.2.0 is already selected in `Cargo.toml`, but publication is deliberately
split from feature implementation.

Sequence:

1. **C022 — pre-release machine-contract hardening** (**closed**,
   `plans/closure/distribution-release-update/c022-status.md`)
   - pin `update --format json` schema/stream behavior with direct tests —
     done, by moving the serializer out of the binary so the tests can reach
     the production one;
   - add `check-installer-contract.py --self-test` with premise-negative
     mutations — done, twelve cases, each also asserting the rejection reason;
   - wire the self-test into CI, release drift, and `release-check.sh` — done;
   - no product/release semantics change — confirmed.

2. **M013 — 0.2.0 publication and Phase 11 operational closure**
   (**closed**; receipt in
   `plans/closure/distribution-release-update/013-status.md`)
   - freeze one exact release commit and `v0.2.0` tag;
   - run the complete release gate;
   - stage all five targets through Eggpack;
   - require automatic staged validation before human publication;
   - publish GitHub and crates.io from the same tag/source identity;
   - verify immutable release attestation and downloaded asset digests;
   - require automatic five-target public update smoke;
   - obtain C018 real Cargo-managed refusal evidence;
   - reconcile C018/M011A/M011B/M011C and record the release receipt.

Hard constraints:

- no staging before C022 closure;
- no publication on incomplete/cancelled/red hosted evidence;
- no bypass of automatic staged validation;
- no replacement of assets/tags after immutable publication;
- crates.io publication must come from a detached checkout of the exact tag;
- failures after publication remain recorded and require a new patch release
  when bytes/product behavior are defective.

Plans:

- `plans/implementation/distribution-release-update/c022-pre-release-machine-contract-hardening.md`
- `plans/implementation/distribution-release-update/013-0.2.0-publication-and-phase11-operational-closure.md`

Exit condition:

- C022 closed;
- v0.2.0 public on GitHub and crates.io from one exact source;
- immutable release+asset attestation verified;
- automatic staged validation and five-target post-release smoke green;
- C018 real Cargo-managed refusal evidence complete;
- M011A/M011B/M011C conditional operational evidence reconciled;
- M013 release receipt closed.

