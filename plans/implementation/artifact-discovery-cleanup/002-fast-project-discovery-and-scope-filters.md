# Artifact Discovery and Cleanup Milestone 002 — Fast Project Discovery and Scope Filters

Status: blocked on M001 closure

Repository baseline: 97dee9fa64868534cedf3a9310e7160335a91be2

Source roadmap:

- plans/subsystems/artifact-discovery-cleanup-roadmap.md#7-milestone-m002--fast-project-discovery-and-scope-filters
- plans/002-long-term-roadmap.md#phase-2--fast-discovery-and-search-scope-filtering

Canonical requirements:

- plans/000-long-term-specification.md#5-canonical-v01-scan-pipeline
- plans/000-long-term-specification.md#9-scope-and-filter-precedence
- plans/000-long-term-specification.md#10-system-scan-semantics
- plans/000-long-term-specification.md#11-symlink-and-filesystem-boundary-policy
- plans/001-terminology-and-domain-model.md

Hard dependency:

- M001 closed with its CLI/config/domain contracts intact.

Primary class: infrastructure / capability

## 1. Objective

Implement the fast, non-sizing discovery layer that locates conventional Cargo project roots and target directories across the effective scan scope while obeying exact root/ignore/unignore semantics and conservative filesystem safety rules.

M002 discovers candidates. It does not classify activity or calculate target sizes.

## 2. Why this milestone is blocked

M002 requires M001's stable EffectiveScanPolicy, path/config semantics, domain records, and library/CLI boundary.

Once M001 closes, no additional architecture decision is expected before M002 begins.

## 3. Current implementation evidence

At the planning baseline there is no Rust code. The expected implementation baseline for actual handoff is the M001 closure commit.

External implementation evidence:

- dua-core 4.1.0 is released and provides a work-stealing worker pool;
- its descend predicate can prune directories before recursion;
- symlinks are yielded but not followed;
- errors are iterator items;
- stream_roots allows additional roots to share one fixed worker pool.

Reference:

https://docs.rs/dua-core/4.1.0/dua_core/

## 4. Invariants that must not regress

- No target sizing occurs during broad discovery.
- No symlink directory is followed.
- A symlink named target is not a conventional artifact target.
- Explicit scope bypasses user ignore/unignore filters.
- Internal safety pruning remains active in explicit scope.
- An ignored ancestor cannot prevent reaching an explicit unignore descendant.
- Discovery result ordering is not treated as stable; final presentation will sort later.
- Permission errors outside candidates do not abort the entire scan.
- Errors that make a candidate relationship ambiguous do not produce a trusted candidate.
- M002 introduces no destructive behavior.

## 5. Scope

### In scope

- filesystem traversal abstraction;
- dua-core 4.1.x adapter;
- bounded thread-count selection;
- platform global-root provider;
- built-in pseudo-filesystem/safety pruning;
- global ignore glob matcher;
- exact unignore path matcher;
- ancestry-preserving exception traversal;
- Cargo.toml + direct target detection;
- target symlink refusal;
- workspace/nested-project behavior;
- discovery diagnostics;
- fixture-scale performance instrumentation/tests.

### Explicitly out of scope

- source recency;
- target recency;
- allocated-size calculation;
- final human report;
- Cargo config target-dir/build-dir;
- Git subprocesses;
- cargo metadata subprocesses;
- cleanup/delete;
- persistent filesystem index.

## 6. Required production changes

### 6.1 Traversal boundary

Introduce an internal filesystem walker interface/adaptor whose public domain output is cargo-cleanme-owned.

Preferred engine: dua-core 4.1.x.

The adapter should expose only needed concepts:

- root;
- path/file name;
- file type without following symlinks;
- optional metadata/error;
- descend decision;
- root completion/cancellation if useful.

Do not leak dua_core::Entry through domain APIs.

### 6.2 Parallelism

Use one bounded traversal pool for broad discovery.

Default worker count may follow available parallelism through the engine, but:

- worker creation must be bounded;
- no worker pool is created per directory/project;
- tests must be able to force deterministic low parallelism where necessary;
- parallel order must not affect candidate semantics.

M003 may reuse stream_roots for target analysis, but M002 should not prematurely build a second architecture around it.

### 6.3 Platform global roots

Implement a small PlatformRoots boundary.

Required intent:

- Unix-like global scan begins at filesystem root;
- Windows enumerates suitable local filesystem roots/volumes;
- pseudo-filesystem roots that are unsafe/non-durable are pruned by platform policy;
- ordinary permission denial is diagnostic;
- behavior is documented exactly rather than implied as "literally every mounted thing."

Do not follow symlinks to expand scope.

### 6.4 Ignore matcher

Ignore patterns are absolute glob-style search exclusions.

Use a purpose-built matcher such as globset behind an internal policy type.

Do not canonicalize every traversed path merely for matching. Normalize configured paths once as far as platform semantics safely allow.

### 6.5 Unignore matcher

V0.1 unignore values are exact absolute directories.

The matcher must answer two questions efficiently:

1. is this path explicitly re-included?
2. is this ignored directory an ancestor of any explicit re-included path?

If #2 is true, traversal continues only as needed to reach the exception rather than pruning the ancestor wholesale.

Example:

~~~text
ignore:
  /projects/archive/*

unignore:
  /projects/archive/keep-me
~~~

The walker may prune /projects/archive/old-a and /projects/archive/old-b, but MUST traverse enough of /projects/archive to reach keep-me.

### 6.6 Explicit-scope semantics

When EffectiveScanPolicy::scope is explicit:

- start only from that root;
- do not apply user ignore/unignore rules;
- still apply no-symlink, VCS metadata, target-pruning, and platform safety invariants.

This is a hard contract.

### 6.7 Cargo candidate discovery

A project root is a directory with Cargo.toml.

A conventional target is a direct child real directory named target.

Discovery should inspect a directory's immediate entries once when possible so Cargo.toml and target can be classified without redundant stats.

When a project with target is found:

- emit DiscoveredProject;
- do not recurse into that target for nested projects;
- continue searching other relevant child directories.

When Cargo.toml exists without target:

- no candidate is emitted;
- nested directories may still contain independent projects.

Empty-target determination may be deferred to M003 target analysis. M002 only needs a real target directory relationship.

### 6.8 VCS and nested-repository boundaries

During discovery:

- .git directories are never traversed;
- analogous common VCS metadata MAY be pruned if cheap and documented;
- a nested repository can still contain its own Cargo project because its root directory itself must be discoverable before its .git child is pruned.

Do not use Git commands.

### 6.9 Diagnostics

Represent permission denied, vanished path, metadata failure, unsupported file type, and platform-root failure as structured diagnostics.

Normal output rendering can wait until M003, but M002 integration tests should inspect diagnostics directly.

Avoid logging one line per inaccessible system file by default.

## 7. Ordered work packages

### Work package A — Walker adapter and test harness

Intent:

Introduce dua-core without coupling the product model to it.

Required changes:

- dependency and adapter;
- forced-thread test option;
- path/error conversion;
- cancellation/drop behavior smoke test.

Acceptance evidence:

- symlink non-following;
- descent predicate pruning;
- permission/vanish errors remain values.

### Work package B — Scope provider

Intent:

Turn M001 policy into traversal roots.

Required changes:

- explicit-root validation;
- Unix platform roots;
- Windows local-volume roots;
- built-in safety pruning;
- bounded diagnostics.

Acceptance evidence:

- explicit root creates one scope;
- global root provider behaves per platform;
- invalid explicit root is fatal.

### Work package C — Filter engine

Intent:

Implement exact user semantics.

Required changes:

- compiled ignore globs;
- exact unignore paths;
- ancestor reachability index;
- filter bypass under explicit scope.

Acceptance evidence:

- ignored sibling pruning;
- re-included descendant reachable;
- filters unused under explicit root;
- no accidental scope escape.

### Work package D — Cargo project detection

Intent:

Emit lightweight candidate records only.

Required changes:

- Cargo.toml detection;
- direct real target detection;
- target symlink refusal;
- target subtree prune;
- nested/workspace fixtures.

Acceptance evidence:

- no target sizing/stat aggregation;
- workspace member without own target not duplicated;
- nested independent project can be found.

### Work package E — Performance and diagnostics

Intent:

Protect the "single command" whole-machine use case from obvious traversal regressions.

Required changes:

- wide/deep synthetic fixtures;
- entry/candidate/diagnostic counters;
- optional test-only timing/operation counts;
- bounded memory behavior.

Acceptance evidence:

- ignored/target subtrees demonstrate pruning by operation counts;
- candidate count does not create extra worker pools.

## 8. Failure, cancellation, restart, and contention semantics

Dropping/cancelling the scan should cause traversal workers to stop and join through the engine's supported cancellation/drop behavior.

A path may disappear between directory enumeration and metadata read. This is non-fatal unless the vanished path was required to establish one candidate's project/target relationship.

Concurrent filesystem creation/removal may cause a candidate to be observed or missed; V0.1 is not a snapshot filesystem. Future cleanup will revalidate.

## 9. Compatibility and migration

No persisted discovery state is introduced.

Filter syntax is user-visible and therefore must receive direct tests before release. If glob semantics change later, treat that as config compatibility work.

## 10. Required tests

### Focused unit tests

- absolute ignore matching;
- wildcard ignore;
- exact unignore;
- ignored ancestor + unignored descendant;
- explicit-scope bypass;
- built-in prune separate from user filters;
- target symlink rejected;
- normal directory target accepted.

### Integration fixtures

- Cargo.toml + target;
- Cargo.toml without target;
- target without Cargo.toml;
- workspace root/member;
- nested independent Cargo project;
- target containing nested directories that must not be searched for projects;
- .git subtree with misleading Cargo.toml;
- ignored tree;
- ignored/reincluded tree;
- permission-denied path where supported;
- disappearing path race harness;
- non-UTF-8 path on Unix.

### Performance/regression tests

Prefer operation-count/pruning assertions over fragile wall-clock thresholds in CI.

Record at least one release-build benchmark/measurement on a representative large tree for the closure record.

## 11. Required verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test discovery
cargo test filters
~~~

Add any benchmark/example command actually introduced by implementation to the closure record.

## 12. Documentation updates

- README discovery/scope behavior;
- config.example.toml filter examples;
- platform scan-root documentation;
- closure record;
- registry status reconciliation.

## 13. Acceptance criteria

M002 is complete when:

- global and explicit scopes produce candidate discovery through one bounded engine;
- ignore/unignore semantics match the canonical examples;
- explicit roots bypass user filters;
- Cargo.toml + direct real target pairs are discovered;
- symlink targets are rejected;
- target subtrees are not broadly searched/sized;
- ordinary traversal errors are bounded diagnostics;
- tests demonstrate pruning rather than only final results;
- CI remains green on Linux/macOS/Windows;
- no mtime eligibility, sizing, or deletion behavior has leaked into discovery.

## 14. Stop conditions

Stop and report if:

- dua-core cannot express required pruning/cancellation without unsafe or broad forks;
- platform global-root enumeration would silently include materially different scope than specified;
- unignore exceptions require inverse-glob semantics rather than exact paths;
- Cargo project detection requires cargo metadata to be correct;
- M001 policy/domain contracts are materially different from the plan.

## 15. Closure evidence required

The closure record must include:

- implementation commit/PR;
- dua-core version and adapter boundary;
- tests for symlink/filter/exception semantics;
- platform root evidence;
- pruning/operation-count evidence;
- representative performance observation;
- Linux/macOS/Windows CI;
- confirmation that target sizing/activity/deletion remain absent.

## 16. Handoff notes

Do not optimize by weakening semantics. Correct pruning is the optimization boundary: avoid work before adding caches, databases, or index persistence.
