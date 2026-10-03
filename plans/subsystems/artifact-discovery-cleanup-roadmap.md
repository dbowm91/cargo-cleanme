# Artifact Discovery and Cleanup Roadmap

Status: active

Repository audit baseline: 47574fa8a087ac2f9111e29821c23b13657e7bbe

Canonical references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

## 1. Purpose and ownership boundary

This workstream owns Cargo project discovery, search-scope policy, activity classification, artifact sizing/reporting, and—only after the read-only boundary closes—safe Cargo-mediated cleanup.

It does not own Cargo global cache GC, arbitrary build-system caches, a general disk-usage browser, filesystem indexing, Git repository management, background watchers, or a general-purpose deletion engine.

## 2. Discovery invariants

- Scanning is read-only; cleanup is a separate opt-in operation.
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

At the current audit baseline, M001-M005 and corrective C001-C004 are closed on main. The repository is Rust 1.89 / edition 2024, uses manifest-first discovery plus Cargo-authoritative workspace/output resolution, groups physical target/build output, reports deterministic allocated bytes, provides coordinated inline progress, and performs Cargo-mediated workspace CleanupUnit execution only after complete manifest ownership coverage and fresh full-graph revalidation. M005 destructive qualification is restored. The active work is M006 performance hardening: implementing ADR 002's bounded adaptive Routine discovery, explicit exhaustive Full reconciliation, and remaining traversal qualification without weakening those safety contracts.

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

M006 optimizes the discovery and final-proof edges of this architecture without changing the ownership/safety boundary. ADR 002 further separates fast adaptive Routine discovery from exhaustive Full reconciliation while keeping learned state outside destructive proof.

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

Status: active.

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

Status: ready. Plan: `plans/implementation/artifact-discovery-cleanup/c007-discovery-state-recovery-and-post-m007-planning-cleanup.md`.

C007 is a bounded operational/documentation corrective after M007 closure. It allows a completed trustworthy Full scan to replace recoverable-invalid local discovery state such as schema 0 or corrupt supported data, while preserving newer unsupported schemas and unreadable/I/O-failed state exactly. Routine fallback remains fail-soft.

The same corrective reconciles active planning so closed M007 is no longer presented as the current milestone, audits stale canonical wording, and records safe retirement criteria for merged/superseded branches. It does not change discovery scope, retention, cleanup proof, authorization, or feature roadmap semantics.

After C007 closes, this subsystem has no active implementation milestone; future selective-cleanup/policy work requires separate research and planning.

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

The initial read-only boundary was reached when M001-M003 closed with evidence that cargo cleanme safely inventories inactive conventional Cargo target directories across supported platforms, obeys root/filter precedence, and produces deterministic size output. M004 adds separately scoped Cargo-mediated cleanup while scans remain read-only.
