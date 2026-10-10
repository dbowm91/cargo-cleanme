# C033 — Learned-Root Reconciliation and Unavailable-Root Noise Corrective

Status: **active** for admission/reporting; deterministic reason-grouped omission summaries and two-invocation evidence over 100 stale roots are implemented. Persistent state recovery remains decision-gated; automatic **negative persistence** requires explicit adoption of proposed ADR 005, or a documented alternative preserving accepted ADR 002.
Planning-only branch: `plans/c032-c034-routine-cleanup-reliability`.
Baseline: `7c874ab7998bdac28a6236ec21ae59ec24b915f8` (`main`, published v0.2.5, 2026-10-09).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: M006E, C005, C028, C029, C030 and their closure records. Retains ADR 002 state-is-advisory, completeness, future-version, and publication protections.
Dependencies: C032's failure taxonomy is an interface dependency for diagnostic grouping; no dependency for positive filesystem admission. C034 consumes C033's verified effective selected root set.
Skills: planning-and-closure; discovery-and-ownership; cleanup-safety; json-and-exit-contract; test-evidence.

## User-observed failure and source-level facts

Bare `cargo cleanme` repeatedly reports `omitting unavailable learned root /home/username/.codex/worktrees/2495` and many nearby ephemeral names, even though the user can access the parent path. Distinguish a real existing leaf from a writable parent, a deleted worktree, a symlink/bind mount alias, and race/reappearance **before** assigning cause.

At v0.2.5, `src/policy.rs::classify_cleanup_roots_with_home` calls `fs::symlink_metadata` on **each** automatic root and emits `AutomaticOmission::NotFound` for `ErrorKind::NotFound`, rendered as `omitting unavailable learned root`. That is an *exact-path* absence result, not an access-denied result; the parent may remain perfectly readable. `routine_cleanup_candidates` pulls every age-active old `learned_roots` record from state, with **no provenance filter before admission**. For an existing known non-maintainable root, `automatic_root_is_maintainable` runs; for a now-missing root it does not, so ephemeral Codex locations are described as merely 'unavailable'. `src/discovery_state.rs::reconcile_full` removes old paths only after a complete certain Full pass (and C029 marks ordinary non-root `/` traversal partial on inaccessible subtrees), so stale entries can recur indefinitely within 30-day retention, or longer when configured. `src/main.rs::admit_automatic` derives one diagnostic **per omitted path** and currently prints them without a bounded count in the normal command path. `recheck_admission_premise` ensures omitted paths have not reappeared before a Cargo spawn; that proof must survive any state hygiene change.

**Prior verification miss:** C028 tested an omitted deleted root plus surviving sibling and explicit-root strictness, not dozens of successive Routine invocations over persisted missing worktrees. C030 tested recognized existing cache paths and a completed Full migration but did not assert that missing transient provenance is classified *before* the `NotFound` branch or that partial Full cannot serve as sole way to clean stale state. C029 deliberately makes Full incomplete when `/` contains unreadable areas; the combination creates a positive state-recovery liveness problem.

## Objective and non-goals

Make a normal Routine invocation quiet, fast, and accurate when learned transient paths disappear, without classifying unreadable/ambiguous paths as missing, discarding user-authored roots, following symlinks, or changing the ownership of build artifacts. Address repeated warnings and retained state separately. Preserve canonical Full inventory, explicit root strict validation, 30-day retention and user's configurable state strategy where applicable. No automatic sudo, no universal home-root scan, no direct cache GC, and no silent scope expansion.

A successful fix must **not** claim `PermissionDenied` on `/home/username/.codex/worktrees/2495` when the only error was `NotFound`. Conversely, `NotFound` may occur on a missing ancestor; if distinguishing leaf and ancestor is important, make that distinction by independently checked trusted path components rather than infer it from `symlink_metadata` alone.

## Required semantics

1. **Typed provenance before availability:** Evaluate known persistent vs ephemeral automatic origin without resolving a potentially missing path through a symlink. Keep raw path identity for a symlink/indeterminate admission block. Exact missing paths may be safely omitted for the *current* invocation under C028, with retained reappearance checks; never turn an unreadable path into omission.
2. **Bounded human diagnostics:** group omissions by `reason` and stable parent/provenance category, render total counts plus a small bounded deterministic sample (e.g., first 3–5 paths), and distinguish `missing`, `non-directory`, `known transient/non-maintainable`, `blocked symlink`, and `blocked uncertain access`. Do **not** emit one stderr line per deleted session ID. Keep details available via existing structured report or opt-in diagnostics as appropriate; preserve the single-line `--format log` no-path privacy requirement.
3. **State separation:** machine-local records are hints, not authorization. Distinguish observations from state mutation. Runtime omission cannot by itself manufacture *complete Full negative coverage*, cannot rewrite config.toml, and cannot delete `ProjectRecord` history/identities unrelated to specifically pruned hint paths.
4. **Partial Full:** no absence inference from untraversed regions and no advancement of `last_full_at` to a fabricated complete generation. Unknown-location C029 coverage loss remains conservative. A positive, directly observed `symlink_metadata` result about a specific automatic learned root is distinct from globally inferred absence; however persisting that negative outside Full needs an accepted forward state decision (ADR 005) before code.
5. **Safe state write:** if ADR 005 is accepted, use an immutable input generation/fingerprint and compare/recheck omitted **automatically learned** root candidates (not seed, explicit, user-authored config), preserve supported schema and future-schema refusals, atomic publication, handle concurrent writers, and never downgrade or overwrite a newer/invalid/unreadable state. Race between absence and commit should fail safe by retaining an entry or cancelling update; the C028 reappearance check remains mandatory before each spawned clean.
6. **Idempotence:** repeated bare `cargo cleanme` on an unchanged machine produces no additional omission warning flood and does not mutate state repeatedly; a complete later Full rediscovers reappeared workspaces. No changes to output coverage or cleanup eligibility merely because a hint was retired.

## Ordered work packages

### A. Prove the real premise and trace state

Construct isolated `HOME` + state fixture with 100+ stale `.codex/worktrees/<id>` roots, a real existing `projects` workspace, one existing but unreadable path, one symlink, one removed ancestor, and one worktree that reappears between admission and revalidation. First show baseline v0.2.5 repeats unbounded warnings on *two successive invocations*, with `symlink_metadata` returning `NotFound` for the missing leaf. Use a real path probe to differentiate leaf absence from supposed lack of permissions; do not hard-code a user account. Record effective UID, Cargo binary identity, state last-full generation, exact admitted/blocked/omitted roots, and current run cost.

### B. Centralize classified omissions and bounded report

Refactor `AutomaticOmission` and `ClassifiedRoots::omission_diagnostics`/main rendering as needed. Preclassify raw paths by trusted provenance, but keep pre-admission identity unchanged; no symlink-following canonicalization. Implement a finite, deterministic sample and typed totals without allocating one formatted warning per omission. If JSON v1 reports individual omissions, preserve that meaning or define an explicit schema change before replacing it with counts. Ensure `--format log` stays one 384-byte-or-less ASCII line without absolute paths. Require CLI stdout/stderr parity controls and C028 tests.

### C. State liveness and forward authority decision

Determine what can be improved under accepted ADR 002 by using only in-memory selected roots and bounded warnings. Then evaluate proposed `plans/adr/005-proposed-targeted-learned-root-retirement.md`. If accepted, implement only the narrowly defined positive-observation stale *automatic hint* retirement with concurrency-safe atomic compare-and-publish, no complete Full generation advancement, and no expiry from negative project discovery. If not accepted, **stop at reporting/admission**, retain persisted state and record the explicit unresolved liveness limitation. Do not claim recurring stale-root messages are fully eliminated just because they are summarized.

### D. Docs and operator behavior

Update architecture state/policy/orchestration descriptions, `docs/USAGE.md`, `TROUBLESHOOTING.md`, `AUTOMATION.md`, `README.md`, and Unreleased changelog. Document difference between positive path absence and unreadable parent, state path/rebuild considerations, and why a partial Full cannot prove a cleanup universe or remove unrelated hints. Give a bounded user-only diagnostic command to inspect a named stale root rather than suggesting `sudo` or wholesale state deletion.

## Required regression evidence

- Two separate real CLI invocations over same synthetic state: 100 missing worktrees, one healthy seed/root; bounded human diagnostics and exact omission count each run, and—only when ADR 005 accepted—second run lacks retired entries.
- Positively missing leaf and missing ancestor, symlinked worktree root, EACCES/EPERM/EIO, non-directory, deliberately configured explicit path, healthy learned path, future-dated learned record, stale state and unsupported-newer version. Every non-`NotFound` ambiguity blocks if it could conceal scan scope.
- Deterministic race: omitted root reappears before cleanup spawn; **zero Cargo clean** spawns regardless of earlier temporary negative observation, including if in-memory retirement is pending. A root changes to symlink at recheck; no followed symlink.
- Partial Full with unknown coverage does not purge unrelated hints, and supported state is not overwritten after a newer file arrives between load and write. Crash/atomic-write tests. No clock-edge accidental generation refresh.
- Proven healthy workspace remains in effective Routine set, clean/scan --known root selection consistent, explicit root strict, no authorization/ownership upgrade, no change to C032 resolution ledger. Linux/macOS/Windows/MSRV and fake+real Cargo controls.

## Closure requirements and stop conditions

Run `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, both fixture/doc checkers' self-tests and checks, `git diff --check`, hosted Linux/macOS/Windows/Rust 1.89. Measure output line/byte count and 100-root repeated invocation scenario. Every new test must be shown to fail v0.2.5 for the stated defect. Update source-code line citations semantically.

**Stop** on unsupported state migration, global Full-negative inference from one unavailable path, accidental skipping of an unreadable/symlinked root, a root reappearance that escapes late admission checks, an error incorrectly reported as permission denial, or an ADR 002/005 contradiction. Record blocked state-retirement work as blocked rather than declaring total closure.

Acceptance: root omissions are accurately classified, never flood CLI output, keep healthy roots and all pre-spawn proofs; safe state retirement is implemented **only if** decision authorized. Closure receipt: `plans/closure/artifact-discovery-cleanup/c033-status.md`. No publication authorized by this plan.
