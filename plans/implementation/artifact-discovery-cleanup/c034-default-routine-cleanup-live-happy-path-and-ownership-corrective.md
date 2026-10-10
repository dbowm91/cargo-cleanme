# C034 — Default Routine Cleanup Live Happy-Path and Ownership Corrective

Status: **active** — a disposable Unix real-Cargo default-Routine positive control now proves dry-run parity and real byte reclamation. Remaining heterogeneous Full→Routine qualification, blocked-scope controls, installed v0.2.5 stage capture, cross-platform/MSRV hosted evidence, and closure are outstanding. **Blocked** for independent partial-scope destructive execution unless proposed ADR 004 is accepted with a concrete safety proof.
Planning-only branch: `plans/c032-c034-routine-cleanup-reliability`.
Baseline: `7c874ab7998bdac28a6236ec21ae59ec24b915f8` (`main`, v0.2.5, 2026-10-09).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: M007 and C006 combined-universe cleanup, M012A canonical bare Execute, C028 admission, C029–C031's **closed** safe line; original closure records remain historical.
Hard dependency: interface/fixture outputs from C032 and C033 for final integrated qualification. Partial-cleanup execution additionally requires accepted forward ADR 004 and any corresponding ADR 001/C004/C006 amendment.
Skills: planning-and-closure; discovery-and-ownership; cleanup-safety; test-evidence; json-and-exit-contract.

## Incident and why existing evidence is insufficient

After an exhaustive `cargo cleanme scan` produced extensive `cargo locate-project failed: failed searching for potential workspace` warnings, the user ran bare `cargo cleanme` and saw many `omitting unavailable learned root .../.codex/worktrees/<id>` messages but **no apparent `cargo clean` execution**. This is an observed functional failure. Do not claim a specific code-level blocker until the actual `--format log`, JSON result and `--stats` counters identify the stage.

Source establishes multiple distinct zero-clean causes:
- `src/main.rs::resolve_cleanup` selects Routine seed/learned roots, classifies automatic omissions/blocks and may return a whole-scope block before invoking the engine when one root is symlinked/unreadable/indeterminate.
- `src/cleanup.rs::clean_with_roots_policy_selector` now resolves discovered workspaces before the discovery diagnostic gate, but **any** incomplete discovery blocks the entire admitted combined scope, even if some units were resolved (C031).
- `src/workspace.rs::resolve_workspaces_with_coverage` marks every non-covered discovered manifest unresolved. `src/cleanup.rs::resolve_cleanup_scope` and its caller block Preview/Simulate/Execute when even one participant cannot be resolved. A Cargo workspace-search failure on a temporary/transitive manifest can thus block `~/projects` even though that directory is sound.
- Output/ownership/activity/recency/authorization policy can additionally skip all qualifying units; there may simply be no old private Cargo artifact root. These distinct outcomes must be distinguishable in user output and verified rather than conflated.
- C029–C031 closed on fixture and hosted CI evidence, but did **not** prove the reported user's heterogeneous installed v0.2.5 Full → Routine → real Cargo cleanup case. Those tests asserted known-good scopes and zero spawns for bad mixed scopes. No regression gate required a representative noisy machine root set to result in a real cleanup.

This corrective exists to close the **actual user-level contract**, not to report '376 tests green' again.

## Objectives and hard invariants

The canonical bare `cargo cleanme` must be practical for a developer with real projects, stale worktrees and ecosystem caches. A truly healthy effective Routine scope containing one inactive, privately owned Cargo workspace with a conventional `target` must execute exactly one real `cargo clean` for that workspace, with deterministic bytes/exit/report output; `--dry-run` must report the *same qualified unit* and execute **zero** Cargo clean subprocesses. Invalid/missing unrelated *search hints* must not defeat a provably healthy scope; neither can an active/unresolved participant that could own shared output be silently dismissed.

Existing C003/C004/C006/ADR 001 ownership and source-disjointness requirements remain binding:
- A cleanup command must prove exclusive ownership of **every** affected physical target/build output (one atomic workspace CleanupUnit) against the complete selected ownership universe, freeze Cargo config and perform fresh rechecks before spawn.
- An unresolved manifest, unknown unreadable subtree, Shared/Uncertain/ExternalUnproven, active or unauthorized output cannot be converted to PrivateBounded simply because a neighboring workspace is healthy. User-configured authorization does not manufacture ownership proof.
- Explicit `clean ROOT` remains a strict bounded operator path, not an automatic per-root fallback used to disguise failure of default `cargo cleanme`. No direct filesystem deletion, no `sudo`, and no scheme that permits a workspace to redirect into another candidate's source or output tree.
- Full read-only manifest inventory may remain broader than automatic Routine cleanup candidate hints; do not hide all malformed manifests from Full merely to force a green cleanup.

## Ordered implementation work packages

### A. Stage-accurate live failure proof (required first, ready)

Collect a reproducible, privacy-sanitized *installed binary* v0.2.5 baseline on the actual developer machine or a faithful disposable host replica, using `--version`, `cargo cleanme scan --known --no-progress --stats`, `cargo cleanme --dry-run --format json`, `cargo cleanme --dry-run --format log`, and a specific `stat`/Cargo workspace repro for one problem path. The **first step is simulation**, never a live destructive scan. Capture exact effective roots, omitted vs blocked count, true Cargo process counts, first ownership blocker and whether any cleanup units are privately eligible. Treat privacy/sensitive paths cautiously; report only redacted samples.

Create a hermetic mixed-home fixture with at least two healthy Cargo projects (one inactive private output and one recent), 100 deleted Codex learned worktree records, existing transient/cache roots, several broken or orphaned Cargo.toml files in a separate *Full-only* tree, one unreadable path, and a reachable normal developer directory. Run Full then Routine for two sequential invocations. Use a real Cargo binary for final validation (the `CargoRunner` fake alone is insufficient to qualify Cargo cleanup).

Before and after, record exact `target` identity/size and ensure a disposable workspace; never run tests that could delete real developer files. Independently assert that default root selection is the scope that the test claims, not an accidental configured `scan.root`.

### B. Operator-visible disposition and source-of-truth parity (ready)

Provide a bounded, stage-specific end-state diagnosis: automatic roots selected/admitted/omitted/blocked, Cargo manifests discovered/resolved/unresolved, workspace output ownership units eligible/recent/uncertain, first blocking stage, total cleaned/skipped, and why no action occurred. Distinguish 'no project qualifies' (legitimate no-op) from 'blocked by incomplete discovery', 'blocked by unresolved Cargo workspace', 'unavailable hint omitted', 'output shared', 'recent activity', and 'Cargo clean actually failed'. Use typed internal outcomes; never infer `cleaned` from files disappearing without process evidence. Do not print every session ID in default human output. Preserve JSON v1 contract and `--format log` one-line ASCII <=384 bytes/no paths; if additive reporting is impossible without breaking schema, register the versioned change and delay it.

Make `--stats` expose useful observed counters (locate, metadata, clean spawns, elapsed) without overly expensive per-manifest logging. Ensure successful no-op reports 0 reclaimed bytes, not 'cleaned 0' while a blocking error is present. Avoid ambiguously calling Cargo 'preview' or no-op 'Execute' a real clean.

### C. Existing-proof Routine completion (ready after C032/C033)

First exploit safe scope hygiene and deterministic root admission from C033, the resolved-only learning behavior of C030 and Cargo diagnostic normalization from C032. Prove that normal `~/projects` containers are admitted, historical ephemeral roots are either conservatively excluded on a verified premise or bounded as warnings, and a healthy complete selected union proceeds through resolution, physical grouping, source/output activity, fresh proof and real Cargo Execute. Confirm `--dry-run` Simulation parity and explicit-root path.

If a genuine discovered unresolved manifest **inside the admitted combined scope** still blocks (expected), report precisely which manifest/stage caused it; do not delete the blocking manifest or alter Full filters to create a passing test. A normative whole-scope block with correct cause is an accepted safety result, but not automatically a satisfactory *general* UX; document the residual limit clearly. Test the practical workaround of a user-selected configured `scan.root` or `clean ROOT`, not an implicit fallback.

### D. Destructive scope partitioning research and explicit decision gate (research ready, implementation blocked)

Use the already-proposed `plans/adr/004-proposed-bounded-independent-cleanup-proof-islands.md`. The problem is deeper than two disjoint directory strings: Cargo can redirect `target-dir` and `build-dir` across roots, and a failed or inaccessible Cargo manifest may have an unknown configured output. Test a broken manifest under path B pointing through config/env to the output of healthy workspace A, a valid B actually overlapping A, and mid-run reconfiguration/reappearance. The independence proof must remain sound after final pre-spawn revalidation across all affected root sets.

If there is **no demonstrable finite proof** that a potentially unresolved owner cannot share A's output, retain the entire combined scope block and **do not accept ADR 004**. Instead propose a separate, explicit user-facing scoped maintenance mode (e.g. config `scan.root` or opt-in named independent authoritative scope) for research, with its own product decision. A manually selected root is authoritative within its own boundary but cannot be silently constructed from an auto-selected subset after failed combined proof.

Only after acceptance of a forward ADR 001/C004/C006 ownership amendment and counterexample-negative tests may an implementation agent change destructive scope behavior. Capture status as blocked until then, and never claim the user-reported mixed-scope cleanup is fixed when only a negative-control is green.

### E. Qualification, documentation and release handoff

Repeat the exact user-facing workflow against the new implementation in a disposable environment and, where permitted, compare against the v0.2.5 installed release: first run `scan` (read-only), then `scan --known` and `--dry-run`, then a confirmed **real Cargo clean** on a fixture-owned inactive private workspace using bare `cargo cleanme`; prove bytes decrease and re-running is a genuine no-op. Also run blocked/unresolved and symlink negative controls. Run staged plugin spelling and direct binary, non-TTY/no-progress, JSON/log and any greggd-cron-compatible output.

Update `docs/USAGE.md`, `TROUBLESHOOTING.md`, `AUTOMATION.md`, relevant architecture sections including `09-cleanup.md` and `13-orchestration.md`, test matrix, README, Unreleased CHANGELOG; rederive cited source lines and regenerate docs if CLI changes. A publication plan is a separate gate after closure.

## Required regression/evidence matrix

1. **Positive real clean**: an inactive private fixture with nonzero output must make a real `cargo clean` call and reclaim bytes; validate process invocation/path/environment, no output-source overlap, resulting cargo-cleanme exit 0 and correct human/JSON/log summary. A fake that returns success without running cargo is **not** sufficient.
2. **Simulation parity**: same resolved candidate set and skip reasons for `--dry-run`, no `cargo clean` spawns, no on-disk mutation. Cargo `--cargo-preview` only after full ownership proof.
3. **Real-world heterogeneity**: Full sees broken/cache manifests but Routine does not learn them automatically; 100 stale root messages bounded and state-handled as C033 permits; one healthy developer root remains; two successive invocations qualify; no root permission escalation.
4. **Fail-closed controls**: one unresolved Cargo.toml *within admitted root*, one unreadable admitted subtree, symlink output, root reappearance, external shared target/build, recent activity, and changed Cargo config before spawn each lead to no forbidden cleanup. Preserve C006 cross-root overlap tests.
5. **Scope/exit contract**: missing seed vs explicit invalid root, pure no-op vs blocked, correct typed blockers, `--stats` causal counters, JSON exactly one object, log <=384 bytes and path-free. Linux/macOS/Windows, MSRV 1.89, CLI subprocess and actual Cargo fixtures. Demonstrate negative tests fail against v0.2.5 for the claimed defect.

## Verification, stop conditions, closure

Run fmt, Clippy with `-D warnings`, all-target/all-feature tests, fixture-portability and doc-citation checker self-tests/checks, `git diff --check`, release drift check if docs generation changes, hosted Linux/macOS/Windows/Rust 1.89 and a real non-root Linux run. Record which test commands actually ran with observed results; a green hosted suite without the positive real-clean fixture **does not qualify this plan**.

**Stop** if the only offered fix deletes checks from C004/C006, admits unresolved manifests as harmless, accepts arbitrary `PermissionDenied` as absence, converts 'no clean' into a successful report without a true eligibility proof, changes the JSON/log schema without decision, or modifies actual user build trees in tests. If ADR 004 remains unaccepted, close only the safe existing-proof track, classify general partial-universe cleaning as blocked, and record the live residual problem.

Acceptance: A real, reproducible and safe bare `cargo cleanme` invocation cleans at least one qualifying workspace on the meaningful default selected scope, plus clear deterministic reporting for all remaining no-clean blockers. Closure receipt: `plans/closure/artifact-discovery-cleanup/c034-status.md`, including real Cargo argv, recovered bytes, hosted runs and any remaining ADR 004 block. No release authorized here.
