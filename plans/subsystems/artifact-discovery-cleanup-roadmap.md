## Artifact Discovery and Cleanup Roadmap

Status: Phase 12 implementation remains **closed** under ADR 003. Its first published realization, immutable v0.2.0, carried two destructive defects; **C023 is closed** — its last open criterion, a green **automatic** `release: published` five-lane smoke, was met by the v0.2.2 release (run `37561575727`). Both defects are fixed and shipped in v0.2.1, and crates.io 0.2.0 is yanked and disclosed. **C024** — Windows glob canonicalization — is **closed and shipped in v0.2.2**: the canonical-glob rewrite now runs on Windows, and the hosted Windows lane observed the fix. **C025** is closed. **C026 is closed** — a cross-subsystem planning/architecture/repository-hygiene corrective that changed no cleanup or matching semantics. **C028 is closed and shipped in v0.2.4**: provenance admission for automatic roots, late admission rechecks, Full-generation freshness, and Full-only absence pruning fix the stale-learned-root abort; hosted lanes green, automatic five-target smoke green (`37850582892`), release record `r024-status.md`. Historical Phase 12, C023–C026 closures are unchanged. **C029–C031 safe tracks are closed**: corrected-head hosted run `37953997910` is green on Ubuntu, Windows, macOS, MSRV, generated docs, benchmark, and Linux/Windows/macOS installers. C031's proposed ADR 004 still blocks independent partial-scope Execute.


Repository audit baseline: 3ee9699a0b0d987287d08e427195284e08d079f7

Canonical references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

## 1. Purpose and ownership boundary

This workstream owns Cargo project discovery, search-scope policy, activity classification, artifact sizing/reporting, and—only after the read-only boundary closes—safe Cargo-mediated cleanup.

It does not own Cargo global cache GC, arbitrary build-system caches, a general disk-usage browser, filesystem indexing, Git repository management, background watchers, or a general-purpose deletion engine.

## 2. Discovery invariants

- Every `scan` operation is read-only. Under ADR 003, bare invocation becomes canonical Routine cleanup, but it must compose the same separately proven cleanup engine rather than creating a weaker destructive path.
- No symlink directory is followed.
- scan_start_time is captured once.
- default recency_seconds is 300.
- filesystem mtimes, not Git commit history, determine recent activity.
- recent source activity and recent target activity both protect a project.
- future timestamps protect a project.
- explicit scan scope supersedes user ignore/unignore search filters.
- unignore paths can remain reachable through an otherwise ignored ancestor.
- target sizing does not happen during the broad discovery pass.
- filesystem errors never silently turn uncertain candidates into eligible results.
- output ordering is deterministic.
- internal traversal types do not leak into the domain/CLI model.
- cleanup is limited to the separately planned and revalidated M004 capability.

## 3. Current-state evidence

At C008 closure, M001-M008D and corrective C001-C008 are closed on main; M006A/C/D retain historical conditional evidence but are not active blockers. The repository is Rust 1.89 / edition 2024, uses manifest-first discovery plus Cargo-authoritative workspace/output resolution, supports bounded Routine and explicit exhaustive Full discovery with machine-local learned state, groups physical target/build output, reports deterministic allocated bytes, provides coordinated inline progress/stats, and performs Cargo-mediated workspace CleanupUnit execution only after complete combined-root manifest ownership coverage and fresh full-graph revalidation. Phase 9 is closed for current objectives: workspace selection policy, machine-readable automation contracts, and exact-release profile/package selectors. Phase 10 has been explicitly handed off to the distribution/release/update subsystem; it does not change this subsystem's destructive safety contract.

Relevant current ecosystem evidence:

1. dua-core 4.1.0 is a released crate exposing parallel work-stealing walks, descent predicates, non-following symlinks, errors as iterator items, cancellation, and stream_roots for a fixed pool shared by newly submitted roots.

2. filesize 0.2 exposes platform-aware on-disk sizing. Unix can use metadata blocks directly; Windows uses GetCompressedFileSizeW.

3. current Cargo supports both build.target-dir and build.build-dir. The latter can differ from target-dir, so arbitrary configured output cannot safely be folded into the first conventional-target release.

4. existing tools such as Kondo and cargo-clean-all validate the usefulness of recursive Cargo artifact discovery, but cargo-cleanme intentionally defines activity from project + build filesystem state and keeps destructive cleanup out of the initial release.

Primary references:

- https://docs.rs/dua-core/4.1.0/dua_core/
- https://docs.rs/filesize/latest/filesize/
- https://doc.rust-lang.org/cargo/reference/config.html
- https://doc.rust-lang.org/cargo/reference/build-cache.html

## 4. Target architecture

~~~text
CLI / Config
    |
    v
EffectiveScanPolicy
    |
    v
Manifest Discovery --------> platform/global prune policy
    |                        (dua-core bounded traversal)
    v
Cargo Workspace Resolver
    |
    v
ResolvedWorkspace + OutputSet
    |
    v
Physical Output Grouping
    |
    +-----------------------------+
    |                             |
    v                             v
Read-only Analysis            Cleanup Scope
source activity               complete manifest coverage
output activity/size          CleanupUnit construction
    |                             |
    v                             v
ScanReport                    final fresh ownership proof
                                  |
                                  v
                              Cargo clean
                                  |
                                  v
                          pre/post union report
~~~

M006 optimized the discovery and final-proof edges of this architecture without changing the ownership/safety boundary. ADR 002 separates fast adaptive Routine discovery from exhaustive Full reconciliation while keeping learned state outside destructive proof. M008A adds a selection-policy layer after complete ownership resolution; policy-rejected workspaces remain in the ownership universe. M008B adds a versioned output projection over typed results. M008C may narrow Cargo's mutation selector only after the complete ownership proof and real-Cargo qualification.

## 5. Dependency graph

~~~text
M001 foundation / CLI / config / domain
                  |
                  v
M002 discovery / scope / filters
                  |
                  v
M003 activity / size / reporting / V0.1 qualification
                  |
                  v
read-only field qualification + release
                  |
                  v
M004 cleanup execution [closed]
                   |
                   v
C001 public-contract/MSRV/traversal reconciliation [closed]
                   |
                   v
M005A workspace/output resolution + fail-fast progress [closed]
                   |
                   v
M005B authorized redirected cleanup + --dryrun [historically closed]
                   |
                   v
C002 M005 safety/progress/performance reconciliation [historically closed]
                   |
                   v
C003 cleanup-unit atomicity/full ownership graph [historically closed]
                   |
                  v
C004 complete ownership-universe resolution coverage [closed]
                   |
                   v
M006A global scan/pruning/path normalization [conditionally closed]
M006B cleanup-proof resolution efficiency [closed]
M006C global traversal throughput without scope narrowing [conditionally closed]
M006D global scan completion qualification [conditionally closed]
M006E adaptive Routine/Full discovery state [closed by corrective C005]
M006F exhaustive traversal hot-path qualification [closed]
      |
      v
C005 uncertainty-aware state reconciliation + config/edit hardening [closed]
      |
      v
M006E closure
      |
      v
C006 combined-root cleanup ownership universe [closed]
      |
      v
M007 learned/full cleanup orchestration [closed by corrective C006]
      |
      v
C007 discovery-state recovery/planning cleanup [closed]
      |
      v
M008A workspace selective cleanup policy [closed]
      |
      v
M008B machine-readable reporting/automation contract [closed]
      |
      v
M008C Cargo profile/package selector qualification [closed by M008D evidence; historical conditional closure retained]
      |
      v
M008D Cargo package-selector qualification and enablement [closed]
      |
      v
C008 Phase 9 closure/planning reconciliation [closed]
      |
      v
Phase 10 distribution and operational polish [owned by distribution/release/update subsystem]
~~~

## 6. Milestone M001 — Foundation, CLI, config, and domain contracts

Status: closed.

Primary class: infrastructure / invariant.

Plan:

- plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md

Outcome:

- establish the Cargo package and module boundaries;
- support cargo-cleanme and cargo cleanme entry points;
- define no-argument scan behavior;
- load/validate config.toml with 300-second default;
- define explicit scan-scope precedence;
- define typed domain results and diagnostics;
- establish test/CI/MSRV gates;
- ensure no destructive code path exists.

## 7. Milestone M002 — Fast project discovery and scope filters

Status: closed.

Primary class: infrastructure / capability.

Plan:

- plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md

Outcome:

- integrate a replaceable dua-core-backed traversal engine;
- enumerate global/explicit roots;
- implement ignore glob and exact unignore semantics;
- preserve unignore ancestry;
- discover Cargo.toml + direct real target pairs;
- prune target and VCS trees;
- refuse symlink targets and symlink traversal;
- expose bounded diagnostics without target sizing.

## 8. Milestone M003 — Activity, size analysis, and read-only reporting

Status: closed.

Primary class: capability / invariant.

Plan:

- plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md

Outcome:

- protect recently edited or recently built projects;
- short-circuit source activity;
- compute target activity and allocated/on-disk size;
- reject uncertain candidates;
- render deterministic size-descending output and totals;
- qualify the whole read-only pipeline on Linux/macOS/Windows;
- collect representative performance evidence.

M003 is the first product-capability closure boundary.

## 9. Milestone M004 — Revalidated cleanup execution

Status: closed. M003 is closed and field-reviewed; M004 implementation and hosted matrix are complete. M005 ownership/ADR design is unblocked, while implementation remains deferred.

Implemented direction:

- explicit destructive subcommand;
- revalidation immediately before each clean;
- cargo clean invocation bound to intended manifest/output ownership;
- dry-run/confirmation policy;
- per-project result isolation;
- cancellation/process handling;
- post-clean measurement;
- observed pre/post target-size report; it does not promise reclaimed disk space.

Conventional local target ownership is the first cleanup target. External/shared target-dir/build-dir remains out of scope.


## 9A. Corrective C001 — Public contract, MSRV, traversal, and planning reconciliation

Status: closed. Implementation landed in `d04b9bb` through `59de87b`; closure: `plans/closure/artifact-discovery-cleanup/c001-status.md`.

Primary class: corrective / invariant / polish.

Plan:

- plans/implementation/artifact-discovery-cleanup/c001-public-contract-msrv-traversal-reconciliation.md

Purpose:

- repair advertised Cargo external-subcommand invocation;
- make the shipped configuration template a single source of truth and add explicit safe force initialization;
- reconcile relative cleanup-root CLI ergonomics with the absolute internal sandbox invariant;
- continuously qualify the declared Rust 1.89 MSRV;
- reconcile duplicated dua-core/walkdir traversal ownership and use measured bounded parallelism for expensive analysis where justified;
- update stale README/roadmap/registry state without broadening M004 cleanup ownership.

C001 is a hard implementation-baseline dependency for M005. M005 design research may continue in parallel, but an M005 production handoff must be written against the corrected post-C001 repository state.

## 10. Milestone M005 — Workspace-aware redirected/shared output

Status: closed / destructive release qualification restored. ADR 001 accepted. M005A/M005B/C002/C003 historical closures remain recorded; C004 closed the remaining ownership-coverage corrective.

Accepted decision:

- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

### M005A — Workspace output resolution, fail-fast scan, and inline progress

Status: closed.

Closure:

- plans/closure/artifact-discovery-cleanup/005a-status.md

Plan:

- plans/implementation/artifact-discovery-cleanup/005a-workspace-output-resolution-fail-fast-progress.md

Expected outcome:

- discover manifests independently of a direct local target;
- resolve workspace identity and target/build output through Cargo;
- support multi-member workspace inventory;
- group equal/nested/shared physical output and count it once;
- retain shared/external-unproven output as inventory-only;
- order exclusion gates so ignored, empty, active, and uncertain candidates avoid unnecessary Cargo/source/deep-size work;
- emit structured progress events;
- show immediate inline attended-terminal status with an indeterminate discovery bar, bounded candidate rows with sizes, and clear-before-final behavior;
- preserve plain deterministic output when stderr is non-interactive.

### M005B — Authorized redirected cleanup and simulation

Status: closed.

Closure:

- plans/closure/artifact-discovery-cleanup/005b-status.md

Plan:

- plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md

Expected outcome:

- authorize private output under the clean sandbox or configured absolute cleanup-output roots;
- preserve shared/uncertain/unproven output as non-destructive;
- re-resolve/freeze workspace target/build paths immediately before Cargo cleanup;
- support multi-member workspace Cargo cleanup where ownership is private;
- add `--dryrun` simulation that runs the full cargo-cleanme decision/progress/report path but invokes no Cargo clean command;
- keep existing Cargo `--dry-run` preview semantically distinct;
- clear transient progress and then print every cleaned/would-clean physical output group with sizes and a deduplicated total.

M005 does not parse Cargo-private build-cache layout and does not add selective package cleanup.

## 10A. Corrective C002 — M005 safety, progress, and global qualification

Status: closed.

Plan:

- plans/implementation/artifact-discovery-cleanup/c002-m005-safety-progress-performance-reconciliation.md

Closure:

- plans/closure/artifact-discovery-cleanup/c002-status.md

Outcome:

- restored ADR 001 so ExternalUnproven output is inventory-only and authorization never substitutes for ownership proof;
- `--dryrun` now consumes the same non-mutating pre-spawn gates as Execute through a single final `ExecutionProof`;
- target, build, physical group, authorization, marker, source activity, and output activity are revalidated immediately before destructive spawn;
- inline multi-line progress is coordinated via one `MultiProgress` with determinate totals through the observer contract;
- workspace-member resolution is seeded from Cargo metadata (N-member → 1 locate + 1 metadata);
- detailed scan counters moved behind `--stats`;
- generic scan qualified via synthetic/global-like fixture plus representative real tree (full `/` impractical on this host; conditional reason recorded).

C002 does not rewrite the M005A/M005B closure history. Its historical closure remains preserved.

## 10B. Corrective C003 — Workspace cleanup-unit atomicity and full ownership-graph revalidation

Status: closed.

Plan:

- plans/implementation/artifact-discovery-cleanup/c003-workspace-cleanup-unit-atomicity.md

Closure:

- plans/closure/artifact-discovery-cleanup/c003-status.md

Purpose:

- align the destructive authorization/reporting unit with one workspace-level Cargo clean invocation and the complete resolved target/build OutputSet;
- require every physical output group affected by that invocation to be PrivateBounded and authorized before any mutation;
- prevent a private target group from carrying an ExternalUnproven, Shared, Uncertain, or unauthorized sibling build directory into Cargo clean;
- re-resolve the complete bounded workspace ownership universe under clean ROOT immediately before mutation so another discovered workspace moving into overlap is detected;
- measure and report one deduplicated pre/post output union per Cargo invocation;
- accumulate final-proof Cargo work into cleanup --stats.

Outcome:

- one Cargo clean invocation is authorized as one workspace CleanupUnit covering the complete resolved target/build OutputSet, and every affected physical group must be `PrivateBounded` and authorized;
- a private target group can no longer carry an `ExternalUnproven`, `Shared`, `Uncertain`, or unauthorized sibling build group into the same invocation (whole unit is skipped with an actionable reason);
- Preview/Simulate/Execute each emit one result per unit, cleanup progress totals count CleanupUnits, and one post-clean measurement covers the complete deduplicated output union;
- the final proof re-resolves every workspace discovered under clean ROOT, rebuilds the complete fresh physical graph, and rejects the candidate when any other discovered workspace (including symlink or unresolvable output roots) can reach the affected output, failing closed when a universe workspace cannot be re-resolved;
- `cleanup --stats` reports final-proof Cargo work separately from the initial scan counters.

C003 keeps generic global-scan performance as a separate future optimization line. M005 destructive release qualification was reopened until C003 closed and is unblocked again by its closure.

Deferred (recorded in the C003 closure, not a blocked plan): the ownership-universe refresh performs one bounded re-resolution per cleanable candidate (N candidates over N discovered workspaces ⇒ N² Cargo resolutions, recorded with `--stats`); and on macOS a symlinked path component of the clean root leaves member source roots non-canonical, so the source-activity walk does not prune output directories (eligibility-neutral, wasted traversal, candidate follow-up corrective).

## 10C. Corrective C004 — Complete ownership-universe resolution coverage

Status: closed. Closure: `plans/closure/artifact-discovery-cleanup/c004-status.md`.

Plan:

- plans/implementation/artifact-discovery-cleanup/c004-complete-ownership-resolution-coverage.md

Purpose:

- retain resolution coverage for every Cargo.toml discovered under explicit clean ROOT;
- treat a manifest as covered only through direct successful Cargo resolution or authoritative member/root metadata from a successfully resolved workspace;
- preserve unresolved manifests as ownership participants instead of dropping them after diagnostics;
- block Preview, Simulate, and Execute scope-wide while any unresolved ownership participant remains;
- keep ordinary read-only scan partial-result tolerant;
- preserve C003 CleanupUnit atomicity and full final ownership-graph revalidation once initial coverage is complete.

C004 does not optimize the C003 N² final-proof cost, macOS source-root canonicalization, or generic no-argument scan performance. M005 destructive release qualification is restored after C004 closure.

Outcome: cleanup resolution now retains authoritative manifest coverage and unresolved participants; any remaining unresolved participant blocks the complete cleanup scope before output sizing or per-unit proof. Read-only scan retains partial-result behavior. M005 destructive release qualification is restored after C004 closure. The C003 N² refresh cost, macOS source-root canonicalization, and generic scan performance remain deferred follow-up candidates, not blocked implementation plans.

## 10D. Milestone M006 — Performance hardening

Status: closed for current objectives; M006A/C/D retain historical conditional evidence and are not blockers.

M006 is split into ordered implementation tracks. M006A, M006C, and M006D are conditionally closed after preserving exhaustive reachability while documenting the reference-host traversal bottleneck; M006B is closed. ADR 002 resolves the former scope-versus-latency question by separating bounded adaptive Routine discovery from explicit exhaustive Full reconciliation. M006E implemented that product split, machine-local learned discovery state, configurable 30-day retention, automatic config bootstrap, and `config edit`. Corrective C005 closes M006E's remaining state/config requirements: the reference Full traversal completed with localized diagnostics and Cargo resolution failures, and an isolated run published a schema 2 generation with 1,804 observations and six learned roots. Historical M006E closure evidence remains unchanged; see `plans/closure/artifact-discovery-cleanup/c005-status.md`. M006F's exhaustive-walker hot-path qualification is closed after hosted Linux/macOS/Windows and Rust 1.89 CI passed in run 37139155641.

### M006A — Global scan pruning, traversal policy, and path normalization

Status: conditionally closed. Closure: `plans/closure/artifact-discovery-cleanup/006a-status.md`.

Plan:

- plans/implementation/artifact-discovery-cleanup/006a-global-scan-pruning-and-path-normalization.md

Purpose:

- finish the previously timed-out no-argument macOS scan;
- replace ad-hoc platform pruning with a testable global discovery policy;
- prune only measured OS/tool-managed trees while preserving writable project-bearing domains;
- prune rustup-managed toolchain trees before Cargo manifest handling;
- share one bounded traversal across multiple global roots;
- normalize workspace member source roots to the physical identity used by target/build exclusions.

Closure requires the representative macOS no-argument scan to complete within the prior 120-second timeout.

### M006B — Cleanup proof resolution efficiency

Status: closed. Closure: `plans/closure/artifact-discovery-cleanup/006b-status.md`.

Plan:

- plans/implementation/artifact-discovery-cleanup/006b-cleanup-proof-resolution-efficiency.md

Purpose:

- remove redundant locate-project subprocesses from C003/C004 final ownership-universe refresh;
- refresh known root manifests directly through Cargo metadata and re-check workspace_root/member/output identity;
- add bounded parallel metadata refresh inside one candidate proof only if measurement justifies it;
- preserve fresh complete ownership proof immediately before every Preview/Simulate/Execute disposition;
- explicitly reject cross-candidate stale snapshot reuse in this milestone.

M006B changes performance only; C003/C004 destructive semantics remain authoritative.

### M006C — Global traversal throughput without scope narrowing

Status: conditionally closed. Closure: `plans/closure/artifact-discovery-cleanup/006c-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/006c-global-traversal-throughput-without-scope-narrowing.md`.

M006C preserved global scope, bounded the normal worker cap, added opt-in worker/engine profiling, and reduced per-entry attribution/path-name overhead. Synthetic wide/deep fixtures preserve the same manifest set and counter totals across worker caps, metadata modes, and event orders. The exact reference-host no-argument scan still exceeded 120 seconds; M006D investigated the remaining traversal cost.

### M006D — Global scan completion qualification

Status: conditionally closed. Closure: `plans/closure/artifact-discovery-cleanup/006d-status.md`.

M006D identified traversal as the remaining measured cost and preserved exhaustive reachability, no-follow, manifest, filter, ordering, counter, and cleanup-proof semantics. Its exact native scan still exceeded 120 seconds. That evidence motivated ADR 002; it is not discarded.

### M006E — Adaptive Routine discovery and Full reconciliation state

Status: closed by corrective C005. Plan: `plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md`. Historical closure: `plans/closure/artifact-discovery-cleanup/006e-status.md`; corrective closure: `plans/closure/artifact-discovery-cleanup/c005-status.md`.

Architecture: `plans/adr/002-adaptive-routine-full-discovery-state.md`.

M006E changes the no-argument/default mode to a bounded Routine scan over conservative seeds plus active learned developer roots, adds explicit `scan --full` exhaustive reconciliation, records exact Cargo workspaces and learned roots in versioned machine-local state, and expires learned roots only after successful Full reconciliation using a configurable 30-day default retention. It also replaces explicit config initialization with first-use bootstrap and adds `config edit` using VISUAL/EDITOR/fallback editor resolution with no shell execution.

The old scope-decision proposal at `006e-global-scan-scope-and-latency-policy.md` is superseded.

### M006F — Exhaustive traversal hot-path qualification

Status: closed. Plan: `plans/implementation/artifact-discovery-cleanup/006f-exhaustive-traversal-hot-path-qualification.md`. Closure: `plans/closure/artifact-discovery-cleanup/006f-status.md`.

M006F measures the bare dua-core ceiling versus cargo-cleanme callback overhead, removes first-level subtree attribution from the normal unprofiled hot path, qualifies completion-order versus parent-first delivery, and records a completed Full baseline without narrowing exhaustive scope.

### C005 — Uncertainty-aware discovery-state reconciliation and config/edit hardening

Status: closed. Plan: `plans/implementation/artifact-discovery-cleanup/c005-uncertainty-aware-state-reconciliation-and-config-edit-hardening.md`. Closure: `plans/closure/artifact-discovery-cleanup/c005-status.md`.

C005 corrects M006E's all-or-nothing Full publication gate. A completed Full scan may publish positive Cargo-manifest/workspace evidence even when unrelated localized diagnostics exist. Learned-root expiration becomes path-scoped: only roots with trustworthy negative coverage may age out; roots intersecting traversal uncertainty are retained. C005 also persists containment-collapsed learned roots, surfaces corrupt/newer state diagnostics fail-soft, hardens state publication temp naming, defines retention=0 as expiration disabled, and completes cross-platform `config edit` editor-resolution/fake-editor qualification.

C005 does not change cleanup proof or authorization. State remains a search optimization only.

### M007 — Learned/full cleanup orchestration

Status: closed by corrective C006. Plan: `plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md`. Historical closure: `plans/closure/artifact-discovery-cleanup/007-status.md`; corrective closure: `plans/closure/artifact-discovery-cleanup/c006-status.md`.

Corrective C006 satisfies M007's remaining acceptance criteria with one combined selected-root ownership proof refreshed before each candidate disposition. Learned state remains discovery input only and never authorizes mutation.

### C006 — Combined-root cleanup ownership universe and simulation parity

Status: closed. Plan: `plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md`. Closure: `plans/closure/artifact-discovery-cleanup/c006-status.md`.

C006 generalizes the existing C003/C004 safety model from one explicit cleanup root to the full selected root set used by `clean --known` and `clean --full`. Every selected root contributes to one manifest-coverage universe and one physical output graph; unresolved ownership anywhere in that combined scope blocks Preview/Simulate/Execute. Before each CleanupUnit disposition, the complete selected-root universe is refreshed so cross-root redirected/shared output and races are detected. `clean ROOT` should delegate to the same engine with a one-root vector where practical.

C006 removes the blanket multi-root Execute block only after this proof is qualified.

### C007 — Discovery-state recovery and post-M007 planning cleanup

Status: closed. Plan: `plans/implementation/artifact-discovery-cleanup/c007-discovery-state-recovery-and-post-m007-planning-cleanup.md`. Closure: `plans/closure/artifact-discovery-cleanup/c007-status.md`.

C007 is a bounded operational/documentation corrective after M007 closure. It allows a completed trustworthy Full scan to replace recoverable-invalid local discovery state such as schema 0 or corrupt supported data, while preserving newer unsupported schemas and unreadable/I/O-failed state exactly. Routine fallback remains fail-soft.

The same corrective reconciles active planning so closed M007 is no longer presented as the current milestone, audits stale canonical wording, and records safe retirement criteria for merged/superseded branches. It does not change discovery scope, retention, cleanup proof, authorization, or feature roadmap semantics.

## 10E. Milestone M008 — Selective cleanup and policy

M008 implements Phase 9 as dependency-ordered tracks. Workspace-level policy is deliberately separated from Cargo package/profile selectors because the former only narrows which complete CleanupUnits are selected, while the latter narrows Cargo's mutation request inside a workspace and therefore requires separate runtime/accounting proof.

### M008A — Workspace selective cleanup policy

Status: closed by `plans/closure/artifact-discovery-cleanup/008a-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/008a-workspace-selective-cleanup-policy.md`.

Purpose:

- add minimum reclaimable-size and minimum inactivity-age thresholds;
- add canonical workspace include/exclude selection;
- preserve newest trustworthy source activity through workspace analysis;
- keep policy-rejected workspaces inside the complete ownership universe;
- recompute fresh size/activity policy inputs during the existing final proof before Cargo spawn;
- introduce typed policy dispositions for later machine output.

M008A does not change the CleanupUnit destructive boundary and does not add package/profile cleanup.

### M008B — Machine-readable reporting and automation contract

Status: closed by `plans/closure/artifact-discovery-cleanup/008b-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/008b-machine-readable-reporting-and-automation-contract.md`.

Purpose:

- add versioned deterministic JSON projections for scan and cleanup;
- replace automation-relevant string parsing with stable typed reason/disposition codes;
- define process exit semantics for partial/skipped/failed operations;
- document supported unattended `--yes` operation while keeping scheduling external.

M008B must not serialize internal structs directly as an accidental public compatibility contract and must not change destructive authority.

### M008C — Cargo profile/package selective cleanup qualification

Status: closed by subsequent M008D evidence. Historical conditional closure: `plans/closure/artifact-discovery-cleanup/008c-status.md`; package-selector closure: `plans/closure/artifact-discovery-cleanup/008d-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/008c-cargo-profile-package-selective-cleanup-qualification.md`.

Purpose:

- qualify package/profile `cargo clean` behavior across relevant runtime Cargo capability boundaries;
- keep complete workspace/output ownership proof even when the requested Cargo mutation is selective;
- define selector-specific accounting or explicitly report it as unknown;
- fail closed for unsupported/unqualified runtime-selector combinations;
- avoid all parsing of Cargo-private artifact layout.

Profile selection is enabled for exact qualified releases 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0 with unknown selector byte estimates and a zero minimum-size threshold. Package selection is enabled only on exact qualified Cargo 1.98.1 and 1.99.0; M008D records the identity, shared-dependency, and configured-target evidence.

### M008D — Cargo package-selector qualification and enablement

Status: closed. Closure: `plans/closure/artifact-discovery-cleanup/008d-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/008d-cargo-package-selector-qualification-and-enablement.md`.

M008D owns package-spec identity/ambiguity semantics, runtime Cargo capability gating, minimum-runtime configured-target qualification, shared dependency accounting, and the final package selector Execute decision. It inherits M008A's complete workspace ownership proof and M008B's typed output contract.

### C008 — Phase 9 closure and planning reconciliation

Status: closed. Closure: `plans/closure/artifact-discovery-cleanup/c008-status.md`. Plan: `plans/implementation/artifact-discovery-cleanup/c008-phase-9-closure-and-planning-reconciliation.md`.

C008 is a documentation/planning-only corrective after M008D closure. It reconciles the canonical Phase 9 roadmap and dependency graph, the durable specification/terminology with implemented cleanup-policy/JSON/selector behavior, the active M008C/M008D terminal status, README/schema wording, and the post-Phase-9 registry/subsystem state.

C008 must preserve period-accurate closure evidence, make no production Rust behavior changes, and leave Phase 10 distribution deferred. After C008 closes, the subsystem should have no active implementation handoff unless separate Phase 10 research/planning is explicitly registered.

## 10F. M011D — Cargo selector qualification lifecycle

Status: **closed** — `plans/closure/artifact-discovery-cleanup/m011d-status.md`.

Plan:

- `plans/implementation/artifact-discovery-cleanup/011d-cargo-selector-qualification-lifecycle.md`

Purpose:

- bind every exact runtime-enabled profile/package selector version to one
  guarded real-Cargo qualification matrix;
- make `scripts/qualify-cargo-selectors.sh` assertion-oriented rather than an
  unwired characterization script;
- add a dedicated hosted compatibility workflow for the complete supported
  matrix plus a separate exploratory current-stable lane;
- fail ordinary CI on policy/matrix/runtime-table drift;
- preserve exact-version fail-closed support: an exploratory Cargo version is
  never promoted automatically.

Outcome: `release/selector-qualification.json` is the single authority; the
runtime allowlist in `workspace.rs`, the hosted matrix, and
`scripts/qualify-cargo-selectors.sh` are all proved against it by
`check-selector-qualification.py`. Real-Cargo evidence for all nine claimed
releases (79 assertions) is recorded in the closure record. The hosted matrix is
a recurring maintenance lane, not a release gate, so no hosted evidence holds this
closure.

M011D is evidence-lifecycle hardening over the closed M008C/M008D capability.
It does not change cleanup ownership proof, selector accounting, minimum-size
policy, or the currently enabled version set merely to make qualification pass.
A semantic change in an already-supported Cargo release triggers a separate
capability/corrective plan.

## 10G. C019 — Exact unignore sibling-containment corrective

Status: **closed**.

Plan:

- `plans/implementation/artifact-discovery-cleanup/c019-exact-unignore-sibling-containment-corrective.md`

Corrects:

- M002 plan: `plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md`
- M002 closure: `plans/closure/artifact-discovery-cleanup/002-status.md`

Finding:

M002 specified exact unignore paths, ancestry-preserving exception traversal,
and ignored-sibling pruning. Its surviving regression fixture uses
`archive/*`, so the sibling directly matches the ignore glob and is pruned.
That fixture never covered a **literal ignored ancestor** such as
`ignore = ["/archive"]` with
`unignore = ["/archive/keep"]`.

On 0.1.6 the exception keeps `/archive` traversable, but the stateless child
test forgets inherited exclusion. `/archive/other` therefore becomes visible
even though it is not the exception. This violates the canonical exact
re-inclusion contract.

Required correction:

- carry enough filter state to distinguish ignored/prunable,
  ignored-pass-through-to-exception, and exact re-included subtree;
- traverse an ignored ancestor only along ancestry required to reach exact
  exception paths;
- retain exclusion for unrelated siblings;
- preserve the existing `/*` and `/**` configurations and exact re-included
  subtree behavior;
- preserve explicit-root filter bypass and every internal safety prune;
- keep user-ignore prune counters truthful;
- add a premise-negative literal-ancestor fixture that fails on the old code;
- update the 0.1.6 workaround documentation after corrected behavior ships.

C019 changes discovery scope only by making it narrower where the existing
contract already required narrowing. It does not alter cleanup ownership,
authorization, activity, markers, selectors, or final-proof semantics.

**Closure.** C019 closed. Its plan made a green hosted baseline a closure
precondition, and the baseline was red for two reasons unrelated to it: the
README/release-contract documentation drift, and the M011D gate, which had never
run. The first was reconciled in the C019 pass (the README now carries the
prebuilt target matrix that `check-release-contract.py` and
`check-installer-contract.py` both require); the second was fixed as its own
corrective against M011D. A third red — a C018 test whose premise (a raw-byte
file name on disk) does not hold on APFS — was fixed as a third corrective, and
`check-fixture-portability.py` gained the rule that should have caught it.

## 10H. Phase 12 — Canonical maintenance and unattended operation

Status: **closed** — M012A and M012B both closed.
Closure records: `plans/closure/artifact-discovery-cleanup/m012a-status.md`,
`m012b-status.md`.

Decision:

- `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Plans:

- `plans/implementation/artifact-discovery-cleanup/012a-canonical-maintenance-cli-and-dry-run-semantics.md`
- `plans/implementation/artifact-discovery-cleanup/012b-unattended-log-output-and-greggd-integration.md`

### M012A — Canonical maintenance CLI and dry-run semantics

Purpose:

- make bare `cargo cleanme` invoke Routine Execute over bounded seed + learned roots using the existing C006 combined-root engine;
- make `--dry-run` mean zero-`cargo clean` application simulation;
- retain Cargo's own dry-run only as explicitly named `--cargo-preview`;
- make advanced cleanup execute by default while retaining policy/selectors;
- make rootless `scan` the Full read-only reconciler, `scan ROOT` Explicit, and `scan --known` Routine read-only;
- reconcile JSON operation/scope/mode and generated CLI artifacts;
- carry the breaking bare-invocation change only across a pre-1.0 minor boundary.

M012A changes the command surface, not the destructive proof. Learned state remains advisory; unresolved ownership still blocks the complete selected scope; every CleanupUnit still requires fresh complete ownership/freshness proof immediately before Cargo.

### M012B — Bounded unattended log output and greggd integration

Purpose:

- add explicit `--format log` without auto-switching on non-TTY output;
- project ScanReport/CleanReport into one deterministic ASCII line capped at 384 bytes;
- suppress progress and ordinary diagnostic fan-out in log mode;
- preserve JSON as the complete machine contract and existing exit codes;
- document daily bare Routine maintenance plus weekly Full `scan` under greggd's load threshold/retry/max-wait scheduler;
- document absolute argv paths and the need for a user-owned/rootless greggd instance when maintaining developer-home Cargo trees;
- record, but do not cross-repo-block on, Gregg's stale `scan --deep` documentation example.

Outcome:

- M012A closed. Bare `cargo cleanme` is Routine Execute over the maintenance
  scope through the existing C006 combined-root engine; `--dry-run` is
  zero-`cargo clean` simulation; Cargo's own dry-run is `--cargo-preview`;
  `clean` defaults to Execute; rootless `scan` is Full and ignores a configured
  `scan.root`; `scan --known` is the Routine read-only inventory. One
  `Invocation` model means bare and advanced cleanup reach the same engine.
- M012B closed. `--format log` emits one bounded ASCII line per report (≤384
  bytes, no paths, no Cargo stderr), progress and ordinary diagnostic fan-out
  are suppressed in that mode, exit codes and JSON are unchanged, and
  `docs/AUTOMATION.md` documents the greggd integration.
- Two defects found during M012A implementation and fixed inside it: a configured
  `scan.root` made `clean --known` a silent no-op, and `clean --full` returned
  the scan's exit code silently.
- The destructive proof boundary is unchanged. Learned state is still advisory,
  unresolved ownership still blocks the complete selected scope, and every
  CleanupUnit still requires fresh complete ownership/freshness proof
  immediately before Cargo.
- Publication occurred in v0.2.0. Post-publication, C023 found two destructive
  defects in that release and fixed both in the published and attested v0.2.1;
  crates.io 0.2.0 is yanked and disclosed. C023 is **closed** — its last
  criterion, a green **automatic** `release: published` five-lane smoke, was met
  by the v0.2.2 release (run `37561575727`). No destructive-safety blocker from
  it remains open.

Dependency: M012B has an interface dependency on M012A's resolved operation/scope/mode vocabulary. Rendering work may proceed in parallel after that vocabulary is fixed.

Exit: both plans close with the normal verification ladder, generated-doc drift checks, and premise-negative tests. No scheduler, daemon, load sampler, persistent history, direct deletion, or weakened ownership proof is introduced.

## 10I. C021 — Pre-release test and verification-evidence reconciliation

Status: **closed**. Implemented on `81af7f4` and `bf99aa4` from baseline
`f3dfd7a`. Closure record:
[`plans/closure/artifact-discovery-cleanup/c021-status.md`](../closure/artifact-discovery-cleanup/c021-status.md).

Plan:

- `plans/implementation/artifact-discovery-cleanup/c021-pre-release-test-and-verification-evidence-reconciliation.md`

C021 is a post-Phase-12 release-readiness corrective. It does not reopen M012A
or M012B and does not change the destructive proof boundary.

It owned three findings that had to be closed before 0.2.0 staging, and all
three are now addressed:

1. **Updater staging-cleanup test premise.** Confirmed, and worse than reported:
   the test's observation surface was the process-wide temp directory, so a
   sibling's live staging directory could fail it — the test could not pass for
   its own reason either. Proven deterministically (a foreign same-prefix
   directory reproduces the historical failure on a subject that leaked
   nothing), then replaced with fixture-owned path evidence plus two controls: a
   deliberate leak of the fixture's *own* path must be rejected, and a live
   foreign path must be ignored. No sleeps, retries, or suite serialization.
   Production update code unchanged.

2. **Verification-inventory drift.** The 17-script table is now complete and
   machine-checked: `check-doc-citations.py` requires exact set parity with
   `scripts/` in both directions plus uniqueness, with premise-negative
   self-tests. It failed on its first run against the pre-corrective tree,
   naming the exact drift C019 had recorded. The stale claim that
   `validate-staged-release.py` and `qualify-cargo-selectors.sh` were unwired
   is deleted — M011B and M011D wired both.

3. **0.2.0 changelog completeness.** The `[Unreleased]` entry now also describes
   C018's fail-closed provenance correction and C020's Windows concurrent
   first-use configuration race, and claims no attestation, staged-draft, or
   post-publication smoke evidence that cannot exist before publication.

### Hosted evidence from the final head

| Workflow | Run | Result |
|---|---|---|
| CI | 37376843721 | **success — all 9 jobs, no cancellations** (head `bf99aa4`) |
| Release drift guard | 37375751351 | **success** |
| Cargo selector qualification | 37375754785 | **success**, including the exploratory lane |

The first push (`81af7f4`) failed the Windows lane on `-D dead_code` for a
helper used only by `#[cfg(unix)]` tests. Local Linux verification was green
throughout and could not have seen it. That defect is fixed in `bf99aa4` and
verified locally by cross-target clippy as well as by the lane that found it.

### What C021 did not absorb

Four medium-or-low findings sit outside its authorized surfaces and are recorded
rather than fixed, per the corrective-pass rule: 257 opaque bare `:N` citations in
`architecture/12-self-update.md` that no tool can check; `check-installer-contract.py`
having no `--self-test`; hosted jobs being cancellable while queued for a runner;
and `update_json` remaining untested. Each needs its own corrective.

The baseline hosted runs at `f3dfd7a` (CI #250, Release drift #78, selector
qualification #5) were diagnosed rather than assumed: every cancelled job
reports zero executed steps, so they were cancelled while queued for a runner,
and no job that obtained a runner ever failed. That is runner-capacity
contention, not a product defect, and no CI policy change was made on that basis.



## 10J. C023 — v0.2.0 destructive-safety patch corrective

Status: **closed** — accepted closure record:
`plans/closure/distribution-release-update/c023-status.md`. Both destructive
invariants below are restored and shipped in v0.2.1; the published v0.2.1 binary
passes both safety fixtures. C023's one unmet criterion was distribution-side
(acceptance criterion 11, a green **automatic** M011C five-lane smoke), not a
cleanup-safety one.

Cross-subsystem plan:

- `plans/implementation/distribution-release-update/c023-v0.2.0-destructive-safety-patch-release-corrective.md`

*The requirements below are retained as written before implementation. What
actually happened is in the closure record.*

C023 corrects two published destructive violations of this subsystem's durable
boundary:

- canonical `--dry-run` must spawn zero Cargo clean processes, but v0.2.0 can
  execute cleanup when the flag precedes `clean`;
- `PrivateBounded` must be disjoint from every resolved source tree in the
  cleanup universe, but v0.2.0 checked only the group's owner and could delete a
  neighbouring project's sources.

The baseline fixes strengthen both dispatch and ownership/preflight proof, but
the baseline is red on macOS CI and is not a release candidate.

C023 requires premise-negative reproduction against published v0.2.0,
end-to-end regression tests, complete hosted qualification, and a v0.2.1 patch
release. The distribution/release roadmap owns publication/yank/attestation and
M011C automatic-smoke closure; this roadmap owns the destructive invariants.

**Outcome, in this subsystem's terms.** Both invariants are restored and
qualified by tests shown to fail against the old behaviour:

- canonical `--dry-run` spawns zero `cargo clean` processes across every
  accepted spelling, including the pre-subcommand form Cargo itself passes;
- a `PrivateBounded` classification is refused when any resolved workspace's
  source tree lies inside the cleaned region, and the final preflight catches it
  independently of the classifier.

**No destructive-safety blocker from C023 remains open.** The one corrective it
handed on, **C024**, is **closed** — the canonical spelling is `/`-normalized
and de-verbatimed before it is escaped, so the rewrite runs on Windows and an
`exclude` naming a bracketed directory matches it literally. It failed toward
*under*-matching and so was never a violation of either invariant above; the
fix narrows what can be cleaned rather than widening it. The next release
carries it.

## 11. Cross-cutting reliability concerns

### Races

A filesystem can change during any scan. V0.1 is inventory only, so it records a coherent best-effort observation around a fixed cutoff. Future cleanup MUST revalidate and cannot trust stale eligibility.

### Clock behavior

Wall-clock modification times can be skewed. Future mtimes are active. An inability to compare required timestamps safely disqualifies that candidate.

### Permissions

Permission errors outside a candidate are diagnostic. Permission/metadata failures inside a candidate disqualify it when they prevent reliable activity or size analysis.

### Parallelism

Parallel traversal must not change eligibility, ordering, or diagnostic classification. Final results are sorted after analysis.

### Memory

The scanner should stream discovery and analysis records rather than materialize a full machine tree.

## 12. Verification direction

Each milestone uses fixture-first tests plus platform CI.

Critical fixtures include:

- empty tree;
- plain Cargo project without target;
- empty target;
- inactive target;
- recent source edit;
- recent target edit;
- future mtime;
- workspace root/member;
- nested independent repository;
- target symlink;
- ignored tree;
- ignored tree with re-included descendant;
- explicit root bypassing filters;
- permission-denied subtree where platform test support permits;
- disappearing file during traversal;
- non-UTF-8 path on Unix;
- large wide/deep target tree.

## 13. Performance direction

M002/M003 should record benchmark-style comparisons, but no hardware-specific wall-clock promise is canonical.

The implementation should specifically verify:

- pruning happens before unnecessary target traversal;
- recent source activity can stop project analysis before sizing;
- candidate targets can share one bounded worker pool;
- increasing candidate count does not spawn unbounded worker pools;
- memory grows with active analysis state, not total discovered filesystem entries.

## 14. Completion definition

The initial read-only boundary was reached when M001-M003 closed with evidence that cargo-cleanme safely inventories inactive conventional Cargo target directories across supported platforms, obeys root/filter precedence, and produces deterministic size output. M004-M007 and C001-C007 establish Cargo-mediated cleanup, complete ownership/freshness proof, adaptive discovery, and combined-root orchestration. Scans remain read-only; ADR 003/Phase 12 intentionally changes bare invocation from a scan front door to Routine cleanup without changing that proof boundary. Phase 9 remains closed for current objectives: M008A/B policy and reporting, profile support from M008C, and package support from M008D. Profile support is limited to exact qualified releases 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0; package support is limited to exact Cargo 1.98.1 and 1.99.0. Selector estimates remain unknown and unqualified Cargo versions fail closed. C019 and C020 shipped in v0.2.0; C021 closed the pre-release evidence pass. Post-release C023 is closed: immutable v0.2.0 was found to violate the dry-run and global source-disjointness invariants, both are fixed in the published v0.2.1, crates.io 0.2.0 is yanked, and its last criterion — a green automatic `release: published` five-lane smoke — was met by the v0.2.2 release, run `37561575727`. C024 (Windows glob canonicalization was a no-op) is closed and **shipped in 0.2.2**; it was a matching-semantics defect that under-matched, not a destructive-safety violation. C028 is closed and **shipped in 0.2.4** (stale learned roots no longer abort bare Routine Execute; release record `plans/closure/distribution-release-update/r024-status.md`). Distribution/release publication/yank work is tracked by `plans/subsystems/distribution-release-update-roadmap.md`.


## Post-v0.2.2 repository reconciliation — C026

Status: **closed** — closure record:
`plans/closure/distribution-release-update/c026-status.md`, cross-listed from the
distribution/release planning line.

Plan:

- `plans/implementation/distribution-release-update/c026-post-v0.2.2-repository-reconciliation-cleanup-and-polish.md`

C026 was cross-listed here because its audit included this roadmap,
`architecture/overview.md`, agent-entry documentation, and stale branch
retirement. It did **not** reopen artifact-discovery/cleanup implementation and
did not authorize any change to ADR 001 ownership/freshness proof, ADR 003 CLI
semantics, selector support, policy, or destructive behavior.

It found no cleanup or discovery defect requiring a separate corrective, so this
roadmap's terminal sections needed no substantive change beyond the C026 status
line itself. See the closure record for the classification of every hit it
surveyed and the historical statements it deliberately left intact.


## Post-v0.2.3 corrective — C028 stale learned-root availability and cleanup-scope integrity

Status: **closed — shipped in 0.2.4** (implementation `24a9e82` + `a25b4e8`,
merged to `main`, CI green on branch and `main`; tag `v0.2.4` at `24ec57a`,
immutable; automatic five-target smoke `37850582892` green; release record
`plans/closure/distribution-release-update/r024-status.md`).

Planning branch (merged): `plans/c028-stale-learned-roots-corrective`; baseline: `bd89fb1a5f8e1ba5ec5e41ff5586e082c2352149`.

Plan:

- `plans/implementation/artifact-discovery-cleanup/c028-stale-learned-root-availability-and-cleanup-scope-corrective.md`

Closure record: `plans/closure/artifact-discovery-cleanup/c028-status.md`
(implementation, hosted qualification, publication, and installed-release
smoke all complete; release record
`plans/closure/distribution-release-update/r024-status.md`).

Corrects forward from M006E/C005 learned-state handling, M007/C006 combined-root cleanup and M012A's canonical bare Routine Execute; does not rewrite their closed implementation or closure records. A deleted automatically learned temporary worktree can remain inside the 30-day retention window and pass `policy::routine_roots_from_state` without filesystem validation. `clean_with_roots_policy_selector` then validates all roots as strict explicit inputs, so `NotFound` aborts the entire ordinary `cargo cleanme` command with exit 2 and leaves healthy siblings untouched. The `clean --full` path also reloads learned roots from the persisted state after a Full scan, so a Routine-only filter would be incomplete.

C028 requires provenance-aware root classification before combined discovery, definitive absence/non-directory filtering only for automatically selected roots, unmodified strict handling of user-selected explicit roots, bounded omission diagnostics, and no silent skip on unreadable/symlink-ambiguous coverage. All remaining roots must form one fresh combined ownership universe; late reappearance/deletion cannot bypass C003/C004/C006 proofs. Runtime omissions do not themselves write discovery state; complete certain Full reconciliation may prune definitely nonexistent learned directories through an explicit ADR-002-compatible forward rule, while preserving age retention for existing-but-empty roots and negative-state protection for partial/uncertain scans. Fresh Full cleanup must not use an older cached state generation after a failed reconciliation/publication.

Required evidence includes an initial failing baseline reproduction, real Cargo-external command fixtures alongside valid sibling roots, deterministic races and false-green controls, all cleanup modes and exit/output contract assertions, a Linux/macOS/Windows and Rust 1.89 qualification matrix, and a future published-artifact smoke/closure receipt. The implementation, verification (including hosted lanes), regression matrix, documentation, publication in v0.2.4, and installed-release smoke are recorded in the closure record and `r024-status.md`.

## Post-v0.2.4 corrective sequence — C029–C031 Full scan and Routine happy-path reliability

Status: **C029/C030/C031 safe tracks closed**. Implementations and local evidence are complete. Corrected-head hosted run `37953997910` is green on Ubuntu, Windows, macOS, MSRV, generated docs, benchmark, and Linux/Windows/macOS installers. C031's independent partial-scope execution remains blocked because ADR 004 is still proposed.

Source baseline: `f08705e77716e445e50097333a0313cab9574cdd` (`main`, 2026-10-09).
Planning branch: `plans/c029-c031-permission-discovery-cleanup-reliability`.
Reported Linux reproduction: `cargo cleanme scan` produced 1,102+ individually buffered, apparently identical `Permission denied (os error 13) (/)` warnings. Bare `cargo cleanme` correctly omitted seven vanished learned worktrees (C028), then selected ten remaining roots (including Trash, Node/Go caches, a Codex worktree, `~/projects`, `/tmp`), discovered 2,014 manifests, and returned **before Cargo resolution**, reporting zero workspaces and units because ten discovery diagnostics triggered the C006 whole-scope guard.

**Corrective dependency graph**

1. **C029 (closed):** `plans/implementation/artifact-discovery-cleanup/c029-permission-aware-traversal-and-full-coverage-corrective.md`. Implemented in `653c8f6` and `651c97a`; closure receipt: `plans/closure/artifact-discovery-cleanup/c029-status.md`.
2. **C030 (closed):** `plans/implementation/artifact-discovery-cleanup/c030-learned-root-provenance-and-maintenance-scope-hygiene-corrective.md`. Implemented in `e7a9a82`; closure receipt: `plans/closure/artifact-discovery-cleanup/c030-status.md`.
3. **C031 safe whole-scope track (closed):** `plans/implementation/artifact-discovery-cleanup/c031-happy-path-ownership-scope-and-partial-failure-corrective.md`. Implemented in `e58825b` with regression evidence in `0f8f10f`; closure receipt: `plans/closure/artifact-discovery-cleanup/c031-status.md`. The ADR 004 partial-scope proposal remains blocked and is not an unblocked downstream plan.
4. **ADR 004 proposed (not accepted):** `plans/adr/004-proposed-bounded-independent-cleanup-proof-islands.md`. This records the unresolved question of safety-certified independence in the presence of redirected target/build and unknown external owners. Accepted ADR 001/C003/C004/C006 remains binding; if the proof is not possible, retain the conservative combined-universe block.

**Why earlier verification did not catch this:** M006 scan tests mostly exercised accessible synthetic trees and preserved hundreds of warnings in Full qualification without correcting diagnostic origin; C005/ADR 002 learned-state tests did not model caches/transient tool trees and include unresolved manifests in positive observations; C028 tested missing automatic-root omission, not useful still-existing learned locations; C004/C006 deliberately asserted a whole-scope block when a second root is unresolved and did not provide a real mixed-developer-root success criterion. No prior test gave a discriminating, unprivileged Full → learned-state → subsequent bare Routine Execute fixture with cache and permission noise.

**Invariants and boundaries:** all destructive calls remain Cargo-mediated, workspace-atomic, fully validated against every potentially affected physical output; explicit roots remain strict, symlink traversal remains refused, unknown discovery coverage cannot prove negative state observations, Shared/Uncertain/ExternalUnproven never promoted, admitted/omitted root premises rechecked immediately before spawn. Machine JSON v1 envelope, unattended log bound (one ASCII line ≤384 bytes), Rust 1.89/MSRV, existing config precedence and conservative pre-spawn revalidation remain requirements. A read-only permission skip is not an implicit permission to clean.

**Closure evidence:** Separate receipts record implementation commits, premise-negative fixtures, no-clean controls, the local verification ladder, and current limitations. Hosted Linux/macOS/Windows and Rust 1.89 evidence is complete in run `37953997910`. A future published release must be qualified separately. Never rewrite the historical C028/v0.2.4 closure records or call these planning commits implementation.

## Post-v0.2.5 corrective sequence — C032–C034 Cargo failures, learned-root liveness, real default cleanup

Status: **C032 conditionally closed pending publication; C033 active for admission/reporting, proposed ADR 005 gates negative state persistence; C034 active for existing-proof qualification, destructive partition blocked by unaccepted ADR 004**. C032 implementation and hosted qualification are complete; its corrective remains open until a release carries it. C033 has a local safe implementation slice; C034 also has a Unix real-Cargo default-Routine positive control. C032's current-head hosted run `38021809640` is green on Linux, macOS, Windows, MSRV 1.89, generated docs, benchmark, and installer lanes. C033/C034 acceptance and closure records remain outstanding.

Planning baseline: `7c874ab7998bdac28a6236ec21ae59ec24b915f8` (`main`, 2026-10-09, v0.2.5). Branch: `plans/c032-c034-routine-cleanup-reliability`.

**Observed regression after v0.2.5:** a rootless `cargo cleanme scan` emits many `cargo locate-project failed: failed searching for potential workspace` warnings. Bare `cargo cleanme` emits numerous `omitting unavailable learned root /home/username/.codex/worktrees/<id>` lines and no apparent Cargo clean occurs. Exact default cleanup block stage is **not yet proven** because the user's `--stats`, JSON and log results were not provided; distinguish this from the independently confirmed code mechanisms below.

**Current code evidence and verification blind spots**

- `src/workspace.rs::resolve_workspaces_with_coverage` deliberately calls Cargo's `locate-project --workspace --manifest-path` for every manifest not already covered by metadata. Before C032 implementation, `cargo_failure_reason` retained only the first nonempty stderr line (200-byte bound), hiding informative downstream causes. C032 retains a bounded sanitized cause chain; the real-Cargo matrix covers seven manifest shapes, and an empty-PATH CLI case covers executable spawn failure. Distinct causes behind one generic Cargo line remain separate, ten distinct causes cap at eight human groups, and a 1,000-failure scan retains all JSON diagnostics, the unresolved ledger, valid sibling resolution and exact stats while bounding stderr. Baseline v0.2.5 emits ten per-manifest warnings for the same fixture. C029 bounded **filesystem traversal** errors, not Cargo subprocess diagnostics. Hosted qualification is recorded in `c032-status.md`.
- `src/policy.rs::routine_cleanup_candidates` reimports active persisted learned-root hints, including vanished transient worktrees, on every Routine invocation. In `classify_cleanup_roots_with_home`, exact `symlink_metadata` `NotFound` yields `AutomaticOmission::NotFound` and `omitting unavailable learned root`; this does **not** prove a permissions issue. Even known transient paths do not reach the non-maintainable branch when absent. `ClassifiedRoots::omission_diagnostics` materializes and prints one diagnostic per omitted path. C028 established safe omission *for one invocation*, C030 improved learning and Complete Full repair, but ordinary partial `/` scans do not retire old hints from state. C033 addresses admission/report liveness; persistence is a proposed ADR 002 exception, not assumed authority.
- `src/main.rs::admit_automatic` can whole-scope block before cleanup on uncertain/symlinked automatic roots. `src/cleanup.rs::clean_with_roots_policy_selector` resolves manifest/workspace metadata and counts before blocking on any discovery diagnostic; it later blocks on any remaining unresolved manifest. The same selected-source union is rechecked for ownership immediately before Cargo. Unit-test simulated resolution was insufficient to prove default Routine Execute, so C034 now has a disposable real Cargo positive control under the default `$HOME/projects` seed; broader heterogeneous and blocker-stage qualification remains.
- C029–C031 closed and shipped in v0.2.5 with full hosted/release smoke, but acceptance tested bounded synthetic errors and fail-closed behavior rather than a realistic heterogeneous personal machine Full→Routine→actual cleanup success. Post-v0.2.5 reports require forward corrective planning without editing their historical closure records.

**Ordered corrective dependency graph**

1. **C032 — conditionally closed, awaiting publication:** `plans/implementation/artifact-discovery-cleanup/c032-cargo-workspace-resolution-diagnostics-and-noise-corrective.md`; evidence in `plans/closure/artifact-discovery-cleanup/c032-status.md`. Implementation and hosted qualification are complete. Keep it open until a published release carries the corrective; C032 does not authorize a release.
2. **C033 — active / partial decision gate:** `plans/implementation/artifact-discovery-cleanup/c033-learned-root-reconciliation-and-unavailable-noise-corrective.md`. Admission summaries group omissions by exact typed reason with a count and bounded path samples. A two-invocation fixture over 100 missing hints proves bounded repeat output, unchanged state bytes, healthy sibling admission, and zero-clean simulation. Persistent advisory-hint retirement requires acceptance of `plans/adr/005-proposed-targeted-learned-root-retirement.md`, a forward exception to ADR 002 Full-only expiry. Until accepted, no Routine negative state write.
3. **C034 — active safe proof, independent execution blocked:** `plans/implementation/artifact-discovery-cleanup/c034-default-routine-cleanup-live-happy-path-and-ownership-corrective.md`. `tests/cli_contract.rs::routine_bare_dry_run_and_execute_use_real_cargo_on_default_seed` now proves a staged `cargo cleanme` with no configured `scan.root` resolves isolated `$HOME/projects` as Routine; a real Cargo build provides output, dry-run selects one unit without clean spawn or byte mutation, and Execute invokes real `cargo clean` once and reclaims bytes. Still outstanding: installed v0.2.5 stage capture, heterogeneous Full→Routine and blocked-scope controls, Linux/macOS/Windows/MSRV hosted results, and complete operator docs. Work that executes in one apparently healthy partition despite unknown/unresolved sources is not authorized by ADR 001/C004/C006: `plans/adr/004-proposed-bounded-independent-cleanup-proof-islands.md` remains **proposed**, with redirect/overlap/TOCTOU safety negative controls still required before acceptance.
4. **Publication qualification (future, not yet a numbered plan):** only after C032–C034 acceptance and hosted Linux/macOS/Windows/MSRV verification, stage and inspect *released binary behavior*, real mixed-home fixtures, and the five-target automatic published-release smoke. Do not tag on planning evidence alone.

**Cross-cutting invariants and closure gates**

Rootless `scan` remains attempted exhaustive read-only Full with accurate partial coverage; `scan ROOT` is strict Explicit; bare `cargo cleanme` is Routine Execute and `--dry-run` is Simulate with zero Cargo cleans. Learned state remains advisory and can never authorize bytes. No direct deletion. Every Cargo-mediated clean remains one workspace's full OutputSet; physical shared/external/unproven output is inventory-only, and successful fresh complete ownership proof across the selected source universe remains mandatory. A new CLI/JSON/log contract change requires its own explicit decision, regenerated completions/man pages and machine-contract tests; `--format log` stays <=384 ASCII bytes without paths. All correctives retain Rust 1.89 support and full state-schema forward compatibility.

Completion requires separate `plans/closure/artifact-discovery-cleanup/c032-status.md`, `c033-status.md`, `c034-status.md`, with implementation SHA, baseline failure, counterexample-negative fixtures that actually fail against v0.2.5, subprocess+real-Cargo fixture evidence, fmt/clippy/tests/checker self-tests and hosted OS/MSRV results. The specifically reported mixed-root no-clean case must either be demonstrably solved under accepted proof or transparently retained as a blocked limitation, not converted into a false green.
