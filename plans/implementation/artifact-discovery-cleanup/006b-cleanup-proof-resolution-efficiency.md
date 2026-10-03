# M006B — Cleanup Proof Resolution Efficiency

Status: ready

Repository baseline: 47574fa8a087ac2f9111e29821c23b13657e7bbe

Source milestone: M006 performance hardening and routine machine-wide qualification

Primary class: performance / Cargo subprocess efficiency / destructive-proof preservation

## 1. Objective

Reduce the wall-clock and subprocess cost of C003/C004 cleanup ownership revalidation without weakening the requirement that each Preview/Simulate/Execute candidate receives a fresh complete bounded ownership proof immediately before disposition.

M006B is deliberately conservative: it may remove redundant Cargo work and parallelize independent read-only refreshes inside one proof, but it must not reuse a stale ownership snapshot across cleanup candidates unless a future design proves a complete invalidation model.

## 2. Baseline evidence

C003 closure measured:

- 8 cleanable workspaces;
- 64 proof universe refreshes;
- 64 proof cargo locate-project calls;
- 64 proof cargo metadata calls;
- about 2.16 s wall in --dryrun;
- weaker pre-C003 baseline about 0.49 s.

The N candidates x N workspaces refresh is an intentional safety cost. The first optimization target is the redundant locate-project subprocess inside each refresh.

A fresh root manifest is already known from the initial resolved workspace. Cargo metadata with --manifest-path returns workspace_root, package/member information, target_directory, and runtime build_directory when supported. A changed workspace root or member set can therefore still be detected from fresh metadata without first running locate-project.

Reference:

- https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
- https://doc.rust-lang.org/cargo/commands/cargo-locate-project.html

## 3. Safety invariants

- C004 complete initial manifest coverage remains mandatory.
- Each candidate still refreshes the complete bounded ownership universe immediately before its disposition.
- No cross-candidate reuse of resolved workspace/output state is allowed in M006B.
- Every refresh remains Cargo-authoritative.
- --offline --locked --no-deps semantics remain unchanged.
- A refresh failure for any universe workspace fails the candidate closed.
- Candidate workspace root/member set/target/build/group shape/ownership/activity/markers remain revalidated exactly as C003 requires.
- Simulate invokes zero cargo clean commands.
- Preview/Execute spawn only from a valid ExecutionProof.
- Cargo cleanup remains sequential.
- No direct recursive deletion is introduced.
- Rust 1.89 remains the MSRV.

## 4. Non-goals

- eliminating the N-candidate x N-workspace proof topology by caching across candidates;
- parsing Cargo config or Cargo-private cache layout;
- weakening locked/offline resolution;
- adding a daemon/index;
- changing C004 unresolved-participant policy;
- global-scan traversal optimization (M006A);
- parallel cargo clean execution.

## 5. Work package A — Direct metadata workspace refresh

Introduce an internal refresh path for an already-known workspace root manifest.

Instead of:

cargo locate-project --workspace --manifest-path ROOT
cargo metadata ... --manifest-path LOCATED_ROOT

use:

cargo metadata --offline --locked --no-deps --format-version 1 --manifest-path KNOWN_ROOT

Then reconstruct a fresh ResolvedWorkspace and compare:

- workspace_root;
- root manifest identity;
- members;
- target;
- build/capability.

If metadata reports a different workspace root, member set, target/build path, or capability relevant to safety, preserve current fail-closed behavior.

If the known root manifest no longer exists or is no longer a valid workspace context, fail closed.

Do not call locate-project merely to confirm information already present in metadata.

### Required accounting

Add/adjust proof stats so closure evidence can show:

- proof_locate expected to reach zero for normal C003/C004 final refresh;
- proof_metadata remains visible;
- refresh failures remain visible.

Do not hide subprocess reductions by changing counter meanings.

## 6. Work package B — Shared resolver construction

The initial resolver and final refresh currently duplicate portions of metadata-to-ResolvedWorkspace construction.

Extract one Cargo-metadata decoding/path-normalization boundary used by:

- initial workspace resolution after locate;
- direct final-proof refresh.

Requirements:

- same capability detection;
- same canonical workspace/output/member identity;
- same diagnostics/error classification where applicable;
- no public domain/API leakage unless already public;
- fixture parity across Cargo 1.89-style metadata and newer build_directory metadata.

This prevents performance code from creating a second, weaker workspace parser.

## 7. Work package C — Bounded parallel refresh inside one proof

After direct-metadata refresh lands, measure again.

Only if proof metadata remains a material fraction of cleanup wall time, add bounded parallelism for refreshing independent universe workspaces inside one candidate proof.

Constraints:

- read-only Cargo metadata only;
- one candidate proof waits for all refreshes before graph construction;
- fixed small concurrency cap, measured on CI/reference hosts;
- deterministic final workspace ordering independent of completion order;
- first failure may trigger cancellation/ignore of pending results, but no candidate proceeds with a partial universe;
- no unbounded task/process creation;
- no concurrent Cargo clean;
- test runner must be safe for concurrent calls or expose a separate batch-refresh test boundary.

Start with a conservative cap such as min(4, available_parallelism) and retain only if measurement shows benefit without lock/contention regressions.

## 8. Work package D — Proof-phase structure and timing

Refactor final_cleanup_proof into explicit measured phases if needed:

1. fresh universe metadata refresh;
2. fresh group construction;
3. candidate identity/output comparison;
4. cross-workspace overlap proof;
5. source activity;
6. output activity/sizing;
7. marker/path proof;
8. frozen environment.

Expose enough --stats timing to distinguish Cargo refresh from filesystem proof work.

Do not add high-cardinality per-workspace normal output.

## 9. Rejected optimization in M006B — cross-candidate snapshot reuse

Do not refresh the universe once and reuse it for multiple cleanup candidates.

Reason:

- another process can modify workspace manifests or Cargo configuration between candidate dispositions;
- Cargo config search includes contextual configuration outside the candidate manifest;
- cargo-cleanme intentionally does not parse/reimplement Cargo's configuration provenance;
- C003 requires fresh ownership proof immediately before each disposition.

A future asymptotic optimization would need a separately reviewed invalidation/provenance model or an external locking primitive that actually prevents relevant configuration changes.

M006B may reduce the constant factor of N² metadata refresh, not claim safe O(N) proof reuse.

## 10. Required tests

### Direct metadata refresh

- normal workspace refresh uses zero locate calls and one metadata call;
- workspace root changes => fail closed;
- member set changes => fail closed;
- target changes => fail closed;
- build changes => fail closed;
- metadata failure/malformed output => fail closed;
- Cargo 1.89-style no build_directory capability remains conservative;
- newer distinct build_directory remains frozen correctly.

### C003/C004 regression

- second workspace moves into candidate target => blocked;
- second workspace moves into candidate build => blocked;
- unresolved/symlink output overlap => blocked;
- universe workspace metadata fails during refresh => blocked;
- complete initial ownership coverage still mandatory;
- mixed target/build ownership matrix remains green;
- one invocation/one result remains green;
- Simulate zero-clean remains green.

### Parallel refresh, if implemented

- maximum concurrent metadata calls never exceeds cap;
- result ordering deterministic;
- one failure blocks candidate despite other successes;
- cancellation does not leak child processes;
- sequential and parallel proof produce identical disposition/output on same fixture.

## 11. Performance qualification

Use --dryrun --no-progress --stats to avoid mutation and UI noise.

Required fixture sizes:

- 1 workspace;
- 8 workspaces (C003 comparison case);
- 32 workspaces;
- at least one mixed active/skipped set.

Record:

- proof universe refresh count;
- proof locate count;
- proof metadata count;
- metadata wall time;
- total cleanup wall time;
- peak concurrent metadata processes if parallel path exists.

Hard call-count acceptance after work package A:

For N cleanable candidates over N workspaces:

- proof locate calls: 0;
- proof metadata calls: N x N;
- no reduction in safety checks.

Performance acceptance:

- 8-workspace reference case must materially improve over the recorded ~2.16 s C003 result on the same host/build;
- target: at least 25% lower total --dryrun wall time;
- if bounded parallelism is retained, it must show further measurable improvement on 8/32 workspace fixtures without worsening 1-workspace latency materially.

Do not close based solely on microbenchmarks; include a real multi-workspace tree.

## 12. Compatibility

- CLI/config unchanged.
- report semantics unchanged.
- proof counter values change intentionally because redundant locate calls disappear.
- safety disposition must be byte/semantically equivalent on unchanged fixtures.
- no change to read-only scan.
- Rust 1.89 unchanged.

## 13. Acceptance criteria

M006B closes only when:

- final proof refresh no longer uses locate-project for already-known root manifests;
- fresh metadata still detects workspace identity/member/output changes;
- proof_locate is zero in normal final refresh;
- C003/C004 destructive regression matrix remains green;
- no cross-candidate stale snapshot is introduced;
- measured 8-workspace --dryrun improves materially, target >=25%;
- any added refresh concurrency is bounded and deterministically joined;
- hosted Linux/macOS/Windows + Rust 1.89 CI is green.

## 14. Stop conditions

Stop and report rather than weakening proof if:

- metadata alone cannot detect a workspace identity transition that locate-project currently detects;
- Cargo behavior differs across supported runtimes in a way that makes direct refresh ambiguous;
- concurrency introduces Cargo lock contention that erases benefit or destabilizes behavior;
- meaningful speedup requires cross-candidate cache reuse without complete invalidation proof;
- performance requires dropping --locked/--offline/--no-deps safety semantics.

## 15. Closure evidence

Create:

plans/closure/artifact-discovery-cleanup/006b-status.md

Include:

- before/after 1/8/32 workspace measurements;
- proof call-count matrix;
- direct metadata identity-change matrix;
- concurrency cap/equivalence evidence if used;
- C003/C004 regression results;
- real multi-workspace --dryrun result;
- CI/MSRV;
- unresolved asymptotic cost and recommendation.
