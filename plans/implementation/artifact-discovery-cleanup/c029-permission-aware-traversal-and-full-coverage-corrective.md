# C029 — Permission-Aware Traversal, Diagnostic Attribution, and Full-Coverage Corrective

Status: **conditionally closed** (implementation and local gates complete; hosted macOS checks and installer jobs remain queued in CI run `37953997910`).
Planning branch: `plans/c029-c031-permission-discovery-cleanup-reliability`.
Repository baseline: `f08705e77716e445e50097333a0313cab9574cdd` (`main`, 2026-10-09).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: M002, M006A/C/D/E, C005, M007/C006, M012B, and C028. Historical plans and closure receipts stay unchanged.
Hard dependency: none; C030 may proceed against the diagnostic/coverage interface defined here. C031 must consume this line's verified outcomes.
Read `.skills/planning-and-closure`, `.skills/discovery-and-ownership`, `.skills/cleanup-safety`, `.skills/json-and-exit-contract`, and `.skills/test-evidence` before implementation.

## 1. Incident, diagnosis, and why tests missed it

A non-root Linux `cargo cleanme scan` emitted 1,102+ substantially identical traversal diagnostics:

```text
warning: filesystem traversal entry could not be read: Permission denied (os error 13) (/)
... 1092 more
```

At baseline `src/policy.rs` `unix_policy` starts Full discovery at `/`. In `src/discovery.rs` `discover_global_roots`, `dua_core::RootEvent::Entry(Err(e))` has no failing child path; it appends one warning for every error, **incorrectly attributes each to the selected root** (typically `/`), and buffers all warnings in `walk_diagnostics`. `discover_manifests_root` follows the same pattern. `src/main.rs` prints only ten lines but retains all entries; the uncertainty projection treats these fabricated root paths as actual uncertain regions. A `/` uncertainty intersects every learned root. The Full-scan completeness and exit decision currently check only `PlatformRoot + Error`, not ordinary permission traversal warnings. These are two distinct defects: noisy, misleading observation, and unsound state-completeness inference.

Previous tests mainly exercise small accessible temporary trees and synthetic error handling, rather than an **actual non-root exhaustive traversal with inaccessible siblings** and a state-pruning negative control. A report limited to ten displayed diagnostics was mistaken for bounded diagnostic production. C028 validated missing learned roots at admission, not deep traversal errors. Earlier Full qualification evidence itself recorded hundreds of warnings without resolving the ambiguity.

## 2. Objective and non-goals

Make Full read-only discovery useful as an ordinary unprivileged user: never escalate, never follow directory symlinks, prune unreadable subtrees as early as filesystem APIs permit, continue accessible discovery, and emit concise, truthful diagnostics with precise coverage semantics. This is **not** permission to ignore uncertainty during cleanup or to mark partial Full traversal authoritative for negative learning. Root execution still traverses only paths the process actually has access to and must retain normal excludes and safeguards.

Non-goals: run automatically as root; call `sudo`; increase scanning rights; delete anything; reinterpret `Cargo.toml` ownership; add wildcard system paths that can shadow valid explicit roots; silently change `--format log` or JSON v1 schema.

## 3. Required behavior and safety invariants

1. **Permission denied:** treat `EACCES`/`EPERM` on a child directory as a skipped, unreadable subtree; do not recursively retry it or spam per-entry messages. POSIX mode prechecking is only an optional optimization: it cannot replace a real `openat`/`read_dir` error because ACLs and mount policy apply. Effective UID, not user intent, determines access.
2. **Accurate attribution:** a discovered failing child's path, when available, defines an uncertain coverage subtree; the parent of an unreadable child is not thereby unreadable. Never label the error `/` merely because the traversal engine omitted the child path. If the engine cannot supply a safe path, explicitly model **unknown-location coverage loss**, with conservative reconciliation and no fabricated precise attribution.
3. **Distinct kinds:** permission-denied, disappeared (`ENOENT`), I/O error, symlink/identity concern, and explicit root failure must remain distinguishable. Disappearance in a racy Full walk is not permission to infer absence of a learned project from the whole parent.
4. **Bounded memory/output:** aggregate by (true subtree/path when known, category/error kind, selected root). Keep total event counts and distinct affected counts, and a small deterministic example sample; do not accumulate 1,102 cloned warnings before rendering. The existing human ten-example bound and machine log length ceiling remain intact.
5. **Coverage:** separate 'walk finished' from 'every selected region was provably covered'. One skip is a partial Full result. Positive manifests may be inventoried, but an intersecting/unknown skip must not cause learned-state negative pruning or a false complete-generation claim. A partial attempt must not impersonate a fresh complete Full generation used by `clean --full`.
6. **Explicit root:** a directly named root that cannot be admitted stays an explicit failure; unreadable descendants are represented as partial coverage, not automatically ignored for destructive ownership. No `cargo clean` on an unproven universe.
7. **Compatibility:** preserve CLI scan/clean meaning, `scan.root` precedence, config, Rust 1.89, JSON v1 identity/fields, log one-line contract, and existing cleanup fail-closed behavior. If fixing scan partial exit/status requires a contract change, document and decide it before implementing; do not silently change exit 1/0 semantics.

## 4. Ordered work packages

### A — Reproduce and map the traversal-engine error API

Prove the current behavior against a disposable two-subtree fixture: readable Rust workspace next to an unreadable directory, plus a managed state fixture. Instrument and inspect `dua_core` 4.1.0 event/error contract and its options. Identify whether errors can carry the failing child path without changing the dependency, and whether descent callbacks receive enough information to avoid an attempted opening. Research upstream primitives before introducing a custom walker. Record a measured baseline (error count, attribution, traversal attempts, effect on `last_full_at` and learned-root retention).

If true error-path attribution is impossible through current `dua_core`, choose and document an alternative: bounded custom directory enumerator only for discovery, a narrowly proposed upstream patch, or a conservative `UnknownLocation` indicator. **Never guess path identity.** A local workaround that still tags `/` as the failing child is not acceptable.

### B — Typed traversal coverage and bounded aggregation

Introduce a single discovery-owned, testable coverage/diagnostic accumulator for global and explicit walkers; expose to state reconciliation an explicit (complete / known-uncertain-prefixes / unknown-uncertainty) result. Retain existing public diagnostic categories where possible; if a new type or output field is necessary, make the interface change explicit and cover JSON/log contracts. Aggregate before allocating individual report diagnostics; counters must count actual failures rather than displayed samples. Stable ordering must not depend on worker completion order. Keep first warning concise: number of permission-skipped subtrees/events; print no misleading `(/)` attribution.

### C — Full reconciliation and exit consistency

Use **one** coverage predicate for both `reconcile_full` and the Full scan's status/exit decision in `main.rs`; they currently drift. A partial Full walk is allowed to return **positive read-only results**, but may not perform negative learned-root pruning. If positive-only reconciliation from a partial Full is supported, it must preserve the prior complete `last_full_at` identity and existing negative observations instead of fabricating a new successful generation; otherwise defer state publication, with a precise diagnostic. Ensure `clean --full` remains blocked from cleanup when fresh complete Full evidence is unavailable.

The selected conservative default is a typed partial Full result and exit 1, with useful inventory on stdout and bounded stderr. A different exit behavior requires explicit machine-contract review. Permission events are normal operating conditions for a full unprivileged scan, not fatal process crashes; never classify them as explicit root validation failures merely due to walking `/`.

### D — Integration and documentation

Document that Full means exhaustive **attempted** discovery across platform roots, with inevitable protected subtrees, not root privilege. Document the exact read-only partial-result and state-update semantics, the meaning of permission skip counts, and how to bound scans intentionally. Update `architecture/04`, `05`, `06`, `13`, `14`, `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `docs/AUTOMATION.md`, README and Unreleased CHANGELOG as affected; re-derive all code line citations.

## 5. Required discriminating tests

- Linux unprivileged subprocess fixture with an unreadable subtree and an accessible Rust project; do **not** falsely pass a `0o000` test run as root. Use a controlled lowered-UID run or an equivalent verified premise; skip only with a recorded, proved environmental limitation.
- Deterministically injected `PermissionDenied`, `NotFound`, `EIO` and path-unavailable traversal errors across all supported OSes. Assert actual preserved error classification, bounded memory/sample cardinality, and stable ordering with multiple workers.
- More than 1,000 synthetic unreadable entries cannot produce 1,000 duplicate report strings, misleading literal-root attribution, or unbounded retained buffers.
- Partial Full: accessible manifests inventoried; known-region uncertainty protects intersecting state but not provably disjoint negatives; unknown-region uncertainty protects **all** negative pruning. Neither publishes a complete generation or permits `clean --full` Execute/Preview/Simulate to bypass the block.
- Complete Full: absence pruning and C028 stale-worktree behavior still operate; state schema/current-version and atomic publication preserved.
- Explicit root and explicit descendant-inaccessibility negative controls, symlink no-follow, depth and global-prune exception cases, root execution never treated as explicit permission to widen filters.
- CLI `scan`, `scan ROOT`, `scan --known`, `clean --full --dry-run`, direct and staged `cargo cleanme` forms; human, JSON, bounded ASCII log, `--stats`, and exact appropriate exit codes.
- Premise-negative test must demonstrably fail on `f08705e` for the reported class, not simply assert a warning exists.

## 6. Verification, stop conditions, closure

Run in order and record real outputs: `cargo fmt --all -- --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo test --all-targets --all-features`; fixture-portability self-test/check; doc-citation self-test/check; `git diff --check`. Require hosted Linux, macOS, Windows, and Rust 1.89 evidence. An OS incapable of enforcing the chmod fixture must exercise deterministic injection. Recheck integration with C028 and C004/C006 negative controls.

**Stop** if true path attribution is unavailable and the proposed change would silently narrow negative-uncertainty protection, if the new exit contract is unclear, or if any previously blocked cleanup can run because warnings were hidden. Record unresolved API/contract choices in the closure draft, not invented evidence.

Acceptance: the reported `/` warning explosion cannot recur; accessible projects are inventoried during a partial Full run; users get a bounded actionable summary; unverified absence never expires state; no new destructive authority. Closure record goes in `plans/closure/artifact-discovery-cleanup/c029-status.md` with baseline failure and subsequent hosted evidence. This plan does not authorize publication or claim implementation.
