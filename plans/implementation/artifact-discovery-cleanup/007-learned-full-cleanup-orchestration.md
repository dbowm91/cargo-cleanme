# M007 — Learned/Full Cleanup Orchestration

Status: blocked

Blockers: Corrective C005 must close M006E state publication. Corrective C006 (`plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md`) must generalize C003/C004 proof across the complete selected known/full root set. Until then, multi-root Execute remains fail-closed and independent Preview/Simulate results are not sufficient closure evidence.

Repository baseline: post-M006E adaptive discovery implementation

Source roadmap: post-M006 performance capability extension

Architecture dependencies:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
- `plans/adr/002-adaptive-routine-full-discovery-state.md`

Primary class: destructive orchestration / safety-preserving scope composition

## 1. Objective

Allow the adaptive discovery model to drive cleanup without weakening the existing bounded `clean ROOT` proof model.

Target user-facing capabilities:

- clean currently known/routine developer roots;
- perform an exhaustive Full reconciliation, then clean the resulting bounded roots;
- support the existing `--dryrun` simulation and explicit `--yes` execution semantics.

This milestone must compose existing cleanup scopes. It must not convert cached discovery state into machine-wide cleanup authority.

## 2. Hard dependency

M006E must be closed and its state format/root derivation stable before M007 becomes ready.

M006F is a soft performance dependency only.

## 3. Candidate CLI contract

Preferred direction:

```text
cargo-cleanme clean ROOT [--dry-run|--dryrun|--yes]
cargo-cleanme clean --known [--dry-run|--dryrun|--yes]
cargo-cleanme clean --full  [--dry-run|--dryrun|--yes]
```

Exact spelling may be adjusted during implementation review, but ROOT, --known, and --full must be mutually exclusive.

Semantics:

- ROOT: existing bounded cleanup behavior, unchanged;
- --known: resolve the current active Routine root set and execute one bounded cleanup scope per deduplicated root;
- --full: first complete Full discovery/reconciliation, publish the successful new state, then derive bounded cleanup roots from that completed reconciliation and run the same bounded cleanup machinery.

`--dryrun` retains its current meaning: all non-mutating cargo-cleanme gates run, but no `cargo clean` command of any kind may be spawned.

`--dry-run` retains Cargo preview semantics for each qualified bounded scope.

## 4. Safety invariants

- learned state only selects candidate roots to inspect;
- each selected root independently runs complete C004 manifest-coverage qualification;
- each cleanup unit independently runs the C003 fresh complete ownership-graph proof immediately before disposition;
- cached workspace/output/ownership classifications are never reused as destructive proof;
- one learned root cannot authorize output in another root;
- overlapping learned roots are canonicalized/containment-collapsed before cleanup orchestration;
- unresolved ownership participants block that bounded cleanup scope exactly as today;
- failure of one bounded root does not silently authorize or contaminate another;
- shared, external-unproven, uncertain, unauthorized, active, symlinked, or marker-invalid output remains non-destructive;
- cancellation stops launching new scopes and handles any current Cargo subprocess according to existing cleanup cancellation semantics.

## 5. Work package A — Reusable bounded cleanup entry point

Refactor only as needed so the existing `clean ROOT` implementation can be called repeatedly by an orchestration layer without changing its safety contract.

The reusable operation should return a typed per-scope report rather than print directly.

Keep:

- explicit absolute sandbox;
- complete discovered-manifest coverage;
- workspace CleanupUnit atomicity;
- final fresh ownership proof;
- per-unit Cargo invocation;
- pre/post deduplicated union measurement.

Do not introduce a global stale workspace graph.

## 6. Work package B — Known-root orchestration

Resolve the same active Routine roots used by read-only discovery, then:

1. canonicalize/deduplicate/containment-collapse;
2. run bounded cleanup resolution for each selected root;
3. preserve deterministic scope ordering;
4. aggregate reports without double-counting overlapping physical outputs;
5. isolate per-scope failure.

A learned root that no longer exists is a diagnostic and may be retained for the next successful Full reconciliation to decide expiration; it is not a cleanup error that permits broader fallback.

Do not fall back to Full/machine-wide traversal when no known roots exist.

## 7. Work package C — Full reconciliation then cleanup

`clean --full` must have two explicit phases:

1. exhaustive read-only Full discovery/reconciliation;
2. bounded cleanup orchestration over roots derived from the successful reconciliation.

If Full discovery fails, is cancelled, times out, or cannot publish a trustworthy state generation, phase 2 must not begin.

The Full discovery phase itself never mutates Cargo artifacts.

After Full succeeds, cleanup still re-discovers/re-resolves each bounded root through the existing cleanup path. Avoiding that fresh work is a non-goal unless a separately reviewed proof model is introduced.

## 8. Work package D — Reporting/progress

The final report must distinguish:

- discovery/reconciliation phase;
- cleanup scopes considered;
- scopes skipped/failed;
- cleanup units previewed/simulated/executed;
- estimated would-clean bytes for --dryrun;
- observed pre/post bytes for execution;
- state generation timestamp used/created.

Transient progress remains stderr-only and clears before deterministic stdout.

Do not report Full-scan inventory bytes as recovered bytes.

## 9. Work package E — State interaction

Positive observations produced while orchestrating --known may refresh last-seen timestamps through the same M006E state API.

Cleanup outcome must not delete learned roots merely because output was cleaned; the source project still exists.

Only successful Full reconciliation applies retention pruning, exactly as ADR 002 specifies.

If state cannot be read, `clean --known` fails with an actionable error or uses seed/configured Routine roots only according to the M006E contract; it must never broaden to Full implicitly.

## 10. Required tests

### CLI

- ROOT/--known/--full mutual exclusion;
- all three cleanup modes parse with --dry-run/--dryrun/--yes;
- direct and Cargo external-subcommand forms match.

### Safety

- learned state with stale workspace/output data still gets fresh Cargo resolution;
- unresolved manifest in one bounded root blocks that root;
- another independent root may still proceed if orchestration contract permits per-scope isolation;
- overlapping roots collapse and do not double-clean;
- shared physical output across selected roots remains Shared/blocked after fresh proof;
- mutation race fixtures from C003/C004 remain green;
- --dryrun spawns zero Cargo clean processes across every scope;
- Full failure prevents all subsequent cleanup.

### State

- cleanup does not expire learned roots;
- successful Full before cleanup publishes exactly one reconciled generation;
- failed Full publishes none;
- positive known-root observations may advance timestamps.

### Reporting

- deterministic root/unit ordering;
- no duplicate recovered/estimated bytes;
- failure counts aggregate correctly;
- progress clears before final output.

### Platform

- Linux/macOS/Windows hosted CI;
- Rust 1.89 check/test.

## 11. Acceptance criteria

M007 may close only when:

- existing `clean ROOT` behavior remains regression-compatible;
- --known and --full are implemented as orchestration of bounded fresh-proof scopes;
- no cached learned/project record can authorize mutation;
- --dryrun remains zero-Cargo-clean globally;
- Full discovery must succeed before Full cleanup begins;
- overlapping roots/outputs cannot double-clean or double-count;
- full C003/C004 regression suites and hosted platform/MSRV gates pass.

## 12. Non-goals

- no direct recursive deletion;
- no cleanup from cached target/build paths;
- no filesystem watcher/daemon;
- no shared-cache package-level selective cleanup;
- no relaxation of current ownership classes;
- no global Cargo clean invocation;
- no cross-candidate stale ownership snapshot.
