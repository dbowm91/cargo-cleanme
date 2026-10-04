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

Status: deferred.

Potential work:

- crates.io publication;
- release binaries for common Linux/macOS/Windows architectures;
- package-manager integration;
- optional shell installer/self-update only if justified;
- shell completion/manpage;
- benchmark tracking;
- support policy and release documentation.

## Dependency summary

~~~text
Phase 0 planning
      |
      v
M001 foundation
      |
      v
M002 discovery
      |
      v
M003 activity + size + report
      |
      v
read-only V0.1 field qualification
      |
      v
M004 destructive cleanup
      |
      v
M005 redirected/shared-output support
      |
      v
M006 adaptive discovery/performance [closed]
      |
      v
C005 M006E reconciliation corrective [closed]
      |
      v
C006 combined-root cleanup proof [closed]
      |
      v
M007 learned/full cleanup orchestration [closed]
      |
      v
C007 state recovery/planning cleanup [closed]
      |
      v
M008A workspace selective cleanup policy [closed]
      |
      v
M008B machine-readable automation contract [closed]
      |
      v
M008C Cargo profile/package selector qualification [closed by M008D evidence;
    historical conditional closure retained]
      |
      v
M008D Cargo package-selector qualification and enablement [closed]
      |
      v
Phase 10 distribution and operational polish [deferred]
~~~

Destructive work MUST NOT be pulled forward merely to make the tool feel complete.
