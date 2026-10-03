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
- platform config location and config init/path commands;
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

## Phase 7 — Performance hardening and routine machine-wide qualification

Roadmap milestone: M006.

Status: active.

Objective:

Make the read-only global scan routine on representative developer machines and reduce cleanup-proof Cargo subprocess cost without weakening M005/C002-C004 semantics.

Implementation sequence:

1. M006A — global scan pruning, platform traversal policy, one-pool multi-root discovery, and workspace member/output path normalization.
2. M006B — cleanup proof resolution efficiency: direct Cargo metadata refresh from known root manifests, followed by evidence-gated bounded refresh concurrency if needed.
3. M006C — bounded global traversal throughput without narrowing writable project-bearing roots (conditionally closed; reference scan still exceeds 120 seconds).
4. M006D — global scan completion qualification (conditionally closed; exact scan exceeds 120 seconds).
5. M006E — resolve the global scan scope-versus-latency policy before further scope changes.

M006A and M006B may proceed in parallel.

Hard constraints:

- global performance pruning is evidence-based and must not silently remove writable user project domains;
- explicit-root behavior remains authoritative;
- cleanup proof still refreshes the complete bounded ownership universe immediately before each candidate disposition;
- no cross-candidate stale ownership snapshot is introduced.

Exit condition:

- M006A's global policy and path/activity work is qualified across platforms; M006D documents the still-open 120-second no-argument reference-host scan bound, and proposed M006E owns the required policy decision;
- M006B closes with zero redundant proof locate-project calls, material real-tree improvement, and the full C003/C004 regression matrix green.
- M006C's and M006D's traversal changes preserve the full manifest set and global reachability; the exact 120-second bound remains unmet pending M006E's scope-versus-latency policy decision.

## Phase 8 — Selective cleanup and policy

Status: deferred.

Potential capabilities:

- minimum reclaimable size;
- age classes;
- profile/package-aware cleanup where Cargo supports it safely;
- include/exclude project policy;
- non-interactive automation only after stable machine-readable contracts exist.

## Phase 9 — Distribution and operational polish

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
M006 performance hardening
      |
      v
selective cleanup / distribution
~~~

Destructive work MUST NOT be pulled forward merely to make the tool feel complete.
