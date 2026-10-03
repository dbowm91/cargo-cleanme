# M006F — Exhaustive Traversal Hot-Path Qualification

Status: closed

Closure: `plans/closure/artifact-discovery-cleanup/006f-status.md`. M006E is conditionally closed; the traversal hot-path qualification is complete and its state-publication limitation is recorded separately.

Repository baseline: M006E implementation head

Source milestone: M006 performance hardening

Primary class: performance qualification / traversal internals

## 1. Objective

Finish the implementation-side performance work that remains valuable after Full discovery is no longer the routine default. Quantify the bare dua-core ceiling, remove optional accounting from the production hot path, select the best deterministic traversal delivery mode, and establish a measured exhaustive-scan baseline without narrowing Full scope.

M006F is not allowed to change the Routine/Full product contract accepted by ADR 002.

## 2. Triggering evidence

M006D established that traversal breadth dominates the reference macOS Full scan. Current global discovery still performs bounded first-level subtree attribution bookkeeping for every entry even when profiling output is not enabled. Existing qualification also suggests completion-order delivery visits more entries within the same timeout than parent-first ordering, but prior samples were not controlled before/after benchmarks.

cargo-cleanme already uses dua-core 4.1.0, so this milestone should measure and optimize the adapter/hot path rather than replace the walker speculatively.

## 3. Invariants

- Full scope and project reachability are unchanged.
- Routine/Explicit semantics are unchanged.
- no symlink-directory traversal;
- Cargo.toml regular-file/non-symlink revalidation remains;
- ignore/unignore semantics remain exact;
- final manifest/result ordering remains deterministic;
- diagnostics/counters remain semantically equivalent when enabled;
- cleanup proof behavior is untouched;
- Rust 1.89 remains supported.

## 4. Work package A — Bare walker ceiling benchmark

Add a benchmark/debug harness that can traverse the exact Full root/prune policy with progressively richer callbacks:

1. dua-core walk with only entry counting;
2. + descend/prune predicate;
3. + manifest-name recognition;
4. + progress counters;
5. + attribution/profiling;
6. full production discovery event handling.

Use the same roots, worker cap, metadata mode, and traversal order for each matched run.

Record wall/user/system time and visited-entry throughput. The purpose is to distinguish filesystem/dua-core limits from cargo-cleanme adapter overhead.

The harness need not become stable CLI.

## 5. Work package B — Remove optional attribution from ordinary hot path

Current `discover_global_roots` maintains top-level root/component HashMaps for every entry before finalizing `top_level_entries`.

Change the production path so component extraction, strip_prefix, HashMap updates, and final attribution materialization occur only when the user explicitly requests the relevant stats/profiling surface.

Normal Routine/Full scans without stats/profiling should pay no subtree-attribution cost.

Progress counters that are part of the normal UI may remain, but prefer atomic/batched updates and avoid per-entry allocation/string conversion.

Add structural/performance tests proving attribution maps remain empty/unallocated when disabled if practical.

## 6. Work package C — Controlled traversal-order qualification

Run matched release benchmarks for:

- ParentFirst
- Completion

with the same worker count and metadata/type-only mode on:

- synthetic deep tree;
- synthetic wide tree;
- representative developer subtree;
- reference Full scan.

Because final manifests/results are sorted after discovery, choose Completion for Full production if it materially improves completion time/throughput without changing manifest sets, diagnostics, or counters.

Routine may choose independently if its smaller roots show no benefit.

Do not claim speedup from unmatched architecture/Rosetta runs.

## 7. Work package D — Worker/metadata revalidation

Revalidate the current eight-worker cap and type-only metadata choice after M006E/root-policy changes.

Test bounded caps such as 4/8/16, but do not raise production concurrency without a reproducible matched gain. Existing evidence that 16 workers did not materially improve the macOS tail should be treated as the prior.

Do not add unbounded thread creation.

## 8. Work package E — Filesystem wait evidence

On the reference host, gather OS-level process timing/evidence sufficient to characterize whether Full traversal is syscall/I/O wait bound versus userspace CPU bound.

Portable production code must not depend on platform profiler APIs; this is qualification evidence.

Record enough context to prevent future agents from repeatedly attempting CPU/thread optimizations against an I/O-bound path.

## 9. Required tests

- identical sorted manifest set across benchmark adapter stages where semantics apply;
- ParentFirst/Completion parity for manifest set and filter behavior;
- attribution enabled/disabled parity in discovery results;
- no stats/profiling output on default path;
- same diagnostic categories for fixture failures;
- Linux/macOS/Windows;
- Rust 1.89;
- all M006E Routine/Full/Explicit tests.

## 10. Acceptance criteria

M006F closes when:

- bare-walker versus production overhead is measured;
- optional subtree attribution is removed from the normal hot path;
- traversal order and worker/metadata defaults are selected from matched evidence;
- a complete reference Full scan baseline is recorded;
- no Full scope narrowing is introduced;
- no safety/regression suite fails.

There is no arbitrary 120-second closure gate. Full is an explicitly exhaustive operation after ADR 002.

## 11. Stop conditions

Stop rather than expanding scope if:

- further improvement would require persistent filesystem indexing/watchers;
- a proposed optimization changes which manifests are reachable;
- higher concurrency only shifts I/O contention without reproducible gain;
- meaningful remaining cost is demonstrated to be the host filesystem rather than cargo-cleanme.
