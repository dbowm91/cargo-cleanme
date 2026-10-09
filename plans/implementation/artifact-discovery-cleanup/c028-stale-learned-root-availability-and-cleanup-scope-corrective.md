# C028 — Stale Learned-Root Availability and Cleanup-Scope Corrective

Status: **closed — shipped in 0.2.4.** Implementation `24a9e82` (plus
macOS case-twin test fix `a25b4e8`); closure record
`plans/closure/artifact-discovery-cleanup/c028-status.md`; release record
`plans/closure/distribution-release-update/r024-status.md` (tag `v0.2.4` at
`24ec57a`, automatic five-target smoke `37850582892` green).

Repository baseline: `bd89fb1a5f8e1ba5ec5e41ff5586e082c2352149` (`main`, 2026-10-08).

Planning branch: `plans/c028-stale-learned-roots-corrective`.

Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`; corrective to adaptive Routine/Full discovery (M006E/C005), learned/full cleanup orchestration (M007/C006), and canonical default Routine Execute (M012A).

Prior accepted plans / closure evidence being corrected **forward**, not rewritten:

- `plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md`; `plans/closure/artifact-discovery-cleanup/006e-status.md` (historical closure).
- `plans/implementation/artifact-discovery-cleanup/c005-uncertainty-aware-state-reconciliation-and-config-edit-hardening.md`; `plans/closure/artifact-discovery-cleanup/c005-status.md`.
- `plans/implementation/artifact-discovery-cleanup/007-learned-full-cleanup-orchestration.md`; `plans/closure/artifact-discovery-cleanup/007-status.md`.
- `plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md`; `plans/closure/artifact-discovery-cleanup/c006-status.md`.
- `plans/closure/artifact-discovery-cleanup/m012a-status.md` (the newly canonical bare Execute path exposes the defect).

Normative constraints: `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`, `plans/adr/002-adaptive-routine-full-discovery-state.md`, `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`, `plans/output-schema-v1.md`. Read the planning/cleanup/discovery/test-evidence skills in `.skills/` before implementation.

Primary class: runtime reliability corrective with destructive-scope integrity requirements.

Dependencies: no unresolved upstream dependency; implementation-ready after confirming the baseline. Operational closure requires a *future* published release containing the fix, following the established release gates. **This plan does not authorize a release or specify a version.**

## 1. Objective and observed failure

A deleted, automatically learned scan location must not abort otherwise safe Routine maintenance. Report unavailable learned locations truthfully, omit those that provably cannot contain a directory tree from the *current* effective cleanup scope, and preserve complete fresh ownership proofs over every remaining selected root. Preserve hard errors for explicit roots and fail-closed behavior for uncertain filesystem coverage.

Reproduction reported on Linux:

```text
cargo cleanme
cargo-cleanme: invalid scan root /home/sugarwookie/.codex/worktrees/1749: No such file or directory (os error 2)
```

The `.codex/worktrees/1749` path is an example of a transient developer tree, **not** a special-case pattern or path to hard-code. All automatically learned transient locations must obey the same contract.

## 2. Verified root cause and missing regression

1. `src/discovery_state.rs` `reconcile_full` derives a learned root from a workspace parent and records its `last_project_seen_at`. An ephemeral worktree can become a learned root.
2. `src/policy.rs` `routine_roots_from_state` checks seed roots with `is_dir()` but checks learned roots only against age/retention; a deleted learned root remains in the Routine set for up to the default 30-day window (indefinitely when retention is 0).
3. `src/main.rs` `resolve_cleanup_roots` hands Routine roots to `run_cleanup`; `collapse_roots` retains unresolved paths on failed canonicalization.
4. `src/cleanup.rs` `clean_with_roots_policy_selector` calls `validate_cleanup_root` on *every* root before discovery; `symlink_metadata` `NotFound` becomes fatal `AppError::InvalidRoot`. One stale advisory path prevents all other roots from being processed.
5. `clean --full` bypasses Routine policy: `resolve_cleanup_roots` re-reads `state.learned_roots` directly after `run_scan(Full)`, so a Routine-only filter would not fix the Full cleanup surface.
6. `src/discovery.rs` already emits per-root diagnostics for read-only discovery, but cleanup's separate strict validation treats automatically selected roots as explicit caller inputs.

Prior C005 tests exercise retention, uncertain coverage and old-root preservation; C006 tests prove combined ownership, mode parity and invalid *explicit* roots. They do **not** exercise a directory that was legitimately learned while present and removed before the next bare Execute/Simulate, nor do they prove the CLI command reaches a valid sibling root afterward. Tests on stable `tempdir` roots and validation-only negative tests thus missed the cross-layer lifecycle failure. Capture a failing-direction reproduction against the baseline before changing production behavior.

## 3. Non-negotiable invariants and mode semantics

- `cargo cleanme` remains Routine Execute; `--dry-run` remains zero-`cargo clean` simulation; `clean --cargo-preview` keeps the same scope/proof. No CLI meaning or selector eligibility changes.
- `scan.root`, `scan ROOT`, `clean ROOT` and other explicitly selected roots remain authoritative. A missing, symlinked, non-directory, relative/invalid or unreadable explicit cleanup root must **not** be silently replaced by a smaller implicit set.
- Learned and seeded roots are **search hints**, never authorization or ownership proof. No `allowed_output_roots` or `PrivateBounded` promotion.
- The effective root set is frozen only after provenance-aware admission; all remaining roots form **one combined ownership universe**. Every discovered manifest must be resolved/covered or all candidate cleanup is blocked. Never rescue a root error by independently cleaning a sibling with a narrower proof.
- Do not follow directory symlinks. Do not convert `PermissionDenied`, `EIO`, or otherwise indeterminate metadata into `NotFound` or successful absence.
- Every candidate must pass the unchanged final full-universe revalidation immediately before destructive Cargo spawn. Cancellation, discovery uncertainty, and a changed skipped-root premise must prevent mutation, not become a successful no-op.
- Routine/Explicit scans may update *positive* learned-state observations, but **must not persist negative pruning**. Only complete Full reconciliation can remove learned roots on evidence of absence/age, subject to ADR 002 and uncertainty rules.
- No direct filesystem deletions; cleanup remains Cargo-mediated. State schema, config default retention/recency, the JSON v1 envelope and bounded log contract remain compatible unless an explicit separate contract decision is registered.

## 4. Required filesystem decision table

Classify paths with `symlink_metadata` and inspect the actual `io::ErrorKind`; do not rely on `Path::exists`/`is_dir`, which conflate errors and may follow symlinks.

| At admission | Automatically seeded/learned | User-selected explicit |
|---|---|---|
| Existing real directory | Include in selected root universe | Include |
| Definitive `NotFound` (`ENOENT`) | Omit from this invocation; bounded nonfatal diagnostic; remember omitted path for late check | `InvalidRoot`; abort before any Cargo process |
| Existing regular file / other non-directory | Omit with diagnostic only when type is positively established; late-check it cannot become a directory unnoticed | `InvalidRoot` |
| Symlink (including broken symlink) | Never follow. Mark anomalous and conservatively block mutation if omission could conceal an expected scan tree; present a typed blocked report, not a fatal process error | `InvalidRoot` |
| `PermissionDenied`, unknown metadata/`EIO`, other indeterminate failure | Incomplete effective universe: report a scope block, no `cargo clean` in **any** mode; never claim zero roots | `InvalidRoot` / current fatal error |
| Empty automatic scope after safe omissions | Successful, truthfully empty no-op; zero clean/preview spawns and bounded diagnostics | Explicit empty/invalid invocation remains an error |

Refine the exact treatment of a symlinked *seed* path with an explicit failing-direction test before implementation. A skipped symlink must never be traversed, treated as positive absence, or secretly bypass C004's completeness requirement. If safe omission cannot be proven, block rather than reduce the selected ownership universe. Do **not** add a blanket catch-all `Err(_) => continue`.

## 5. Work package A — Provenance-aware effective root resolution

Implement one shared classification seam usable by bare Routine cleanup, `clean --known`, `clean --full`, and `scan --known` without weakening the library's strict `clean_with_roots_policy_selector` public root contract.

- Preserve origin (explicit vs automatic seed vs learned state) until validation; if this requires a private richer resolved-root type, keep it internal to `policy.rs`/`main.rs` where feasible rather than broadening the public `ScanScope` API.
- Perform classification **before** collapse/canonicalization can hide invalid root identities. Separate the paths admitted for traversal, paths positively absent/non-directory, and paths whose uncertainty blocks mutation. Canonicalize/dedupe/collapse the admitted paths only.
- Ensure filtering does not accidentally discard a valid sibling or an explicit configured `scan.root`, and do not let an existing ancestor mask an omitted stale child without preserving the late-check condition.
- Emit deterministic, bounded diagnostics for omitted automatic roots (including number and optionally paths in human mode). Prevent one missing path from causing a fatal exit `2`, retain the actual source label `routine`/`full`, and do not print paths in `--format log`.
- Do not make automatic policy resolution implicitly write state; keep runtime selection pure and make any state change an explicit Full publication.

## 6. Work package B — Unified cleanup orchestration and TOCTOU defense

- Ensure the Routine, `clean --known`, bare Execute, root-level `--dry-run`, `clean --full`, and Cargo preview spellings share the classification/effective-root path.
- Pass one canonical admitted root set to the existing combined cleanup engine. No per-root execution fallback and no bypass of `validate_cleanup_root` for a root *once selected*.
- A root filtered on proven absence/non-directory status is an assumption of the ownership-universe boundary. Retain its identity for a **late pre-spawn recheck**; if it reappears as a real directory, becomes unreadable, or otherwise invalidates the admission premise, block the pending destructive candidate(s) with a stable reason and no further Cargo spawn. Run this guard before the first cleanup candidate and before any later spawn; do not use timers/sleeps or cached metadata.
- Protect against the narrower race in which an *admitted* root disappears between classification and the cleanup engine's own validation: if proven `NotFound`, reclassify/re-resolve the whole automatic root set and rebuild the entire discovery universe **before** any cleanup; if safe reconstruction cannot be proved, return a blocked report. Never silently drop a root from `cleanup.rs` while maintaining old discovery/proof snapshots.
- Retain existing `C003/C004/C006` final workspace/physical-output proofs. Existing `ProofUniverse` hoisting may remain an optimization only if it does not cache an invalid root boundary.
- For `clean --full`, consume roots from **the Full reconciliation just completed**. Inspect the current code path that treats a failed discovery-state publish as a warning but may reload an older generation; a failed/unsupported/unavailable state publication must not become permission to destructively clean stale historical Full roots. Either carry a proven in-memory generation or block the Full cleanup. This is a root-provenance issue, not permission to weaken Full's completeness requirement.

## 7. Work package C — Learned-state reconciliation without destructive widening

- Keep `learned_root_retention_days = 30` by default and `0` meaning no *age-based* expiration.
- Runtime omission of an absent root **does not rewrite** `discovery-state.json` and does not delete its project records; later recurrence remains discoverable and Full can relearn it.
- In complete Full reconciliation only, add a narrowly specified removal rule for a learned-root directory **positively confirmed nonexistent** at reconciliation time, independent of project-recency retention, provided the relevant coverage was complete and no path-intersecting uncertainty applies. Distinguish `path is physically absent` from `path exists but no Cargo.toml was found`; the latter retains ADR 002 age behavior.
- Confirm this narrow absence case is compatible with ADR 002 §§5–6; if implementation requires changing an accepted decision, add a clearly labeled *forward* ADR amendment before changing the pruning rule. Do not silently reinterpret a prior closed plan.
- Preserve newer-schema protection, corrupt-state read fallback, atomic publication, future-clock semantics, deduplication/collapse and concurrent publisher behavior. No state format migration for this corrective.
- Never prune a root from a partial/failed Full pass, including inaccessible ancestor/mount conditions that make positive absence uncertain.

## 8. Work package D — Regression/negative-control evidence

Add unit fixtures in `policy.rs` / `discovery_state.rs` and **subprocess** CLI contract tests in `tests/cli_contract.rs` or `tests/end_to_end.rs`. Minimum discriminating matrix:

1. Full-learn a Rust workspace beneath a disposable temporary container, delete that container, then run actual bare `cargo cleanme --dry-run` and bare Execute against an independently eligible sibling root. Assert no fatal `InvalidRoot`; assert the surviving root is in the effective universe and receives the same decision in Simulate/Execute; Execute invokes Cargo only for the separately proven eligible workspace.
2. Repeat for `clean --known` and `clean --full`, with the direct binary and (at least one) staged `cargo cleanme` external-subcommand form, proving that the installed command under test actually ran.
3. Assert a missing explicitly configured `scan.root` and an explicit `clean ROOT` remain fatal with zero Cargo clean spawns, even alongside available seed/learned paths.
4. A missing auto root plus an unrelated healthy root: stable selected-roots/accounting and deterministic output. All roots safely missing: valid empty no-op; zero Cargo clean/preview spawns.
5. Auto root replaced by regular file, symlink to directory, broken symlink, unreadable root, and injected non-`NotFound` metadata error: prove each category, no symlink traversal, no mistaken successful-absence classification, and no mutation when coverage is uncertain.
6. Deterministically change a skipped root from absent to existing/uncertain **between initial selection and final spawn** through an explicit fixture seam or controlled runner (no sleeps); require a negative outcome with zero Cargo clean spawns before that decision and no hidden bypass after a previous candidate.
7. Deterministically delete an admitted root after selection; prove either a completely fresh combined-universe rebuild or a blocked result, **never** independent sibling mutation against the stale proof.
8. Full reconciliation: newly observed roots retained; positively nonexistent old roots removed only after complete certain Full; existing-but-no-project roots still respect retention; intersecting uncertainty, failed/cancelled Full, newer/invalid state and unpublishable state do not destructively shrink the learned scope.
9. Re-run `Shared`/`Uncertain`/`ExternalUnproven` multi-root fixtures, unresolved manifest blocks, external output authorization, overlapping source-tree protection and all-mode parity from C003/C004/C006/C023.
10. Assert JSON is exactly one v1 envelope and `selected_roots` describes what was actually traversed; log is one bounded ASCII line, no leaked filesystem paths, with truthful diagnostic/block count; successful omission returns `0`, blocked cleanup returns `1`, explicit invalid argument returns `2`.

**Premise-negative control:** first exercise the missing-root reproduction against `bd89fb1` or an equivalent unfixed baseline and capture the exact failing command/test. Make the fixed test demonstrably distinguish the absence-path defect from a different error, or it cannot close this corrective.

## 9. Verification and documentation

Run repository verification in sequence; stop at the first failure and report unrun lanes as unrun:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
git diff --check
```

Qualify hosted Linux, macOS, Windows and Rust 1.89. A Linux-only missing-Codex-worktree reproduction is not sufficient: the `NotFound`/symlink/non-directory and staged external-command paths must be portable. If any new guard is added, supply its own failing-direction self-test. Run targeted cleanup/CLI fixtures in Execute, Simulate and Cargo preview modes with disposable Cargo workspaces only.

Update `architecture/04-policy-and-scope.md`, `05-discovery.md`, `06-discovery-state.md`, `09-cleanup.md`, `13-orchestration.md` and `14-testing-and-verification.md` only as affected; re-derive cited source line numbers, not offset arithmetic. Update `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `docs/AUTOMATION.md`, README scope guidance, and the Unreleased `CHANGELOG.md` entry where relevant. Do not rewrite historical closed plans/closure receipts. No CLI additions/manpage regeneration unless behavior genuinely changes the clap surface.

## 10. Acceptance criteria

C028 is implementation-qualified when:

1. The reported stale worktree scenario no longer returns fatal `InvalidRoot` from the canonical no-argument cleanup; valid independent roots still receive complete safety decisions.
2. Explicit roots retain strict failure semantics; missing/regular-file automatic roots can be omitted without treating unreadability or symlink traversal as benign.
3. Every destructive candidate is proven against one fresh combined eligible-root universe; skipped-root reappearance and admitted-root deletion cannot bypass completeness.
4. Routine negative observations do not persist state changes; a successfully completed certain Full may prune definitively nonexistent learned directories without incorrectly pruning existing, uncertain or aged-but-unexpired roots.
5. `clean --full` cannot act on an older discovery-state generation when the required fresh Full generation failed.
6. Human, JSON, and log modes tell the truth about selected roots/diagnostics and preserve existing output/exit contracts.
7. Negative controls and focused fixtures prove the original defect and each new safety gate; existing destructive-proof fixtures remain green.
8. All ordinary and hosted verification lanes pass, with exact execution evidence rather than inferred results.

Full `closed` status additionally requires a released version containing C028, real installed-command smoke verification, and an accepted closure record; code landing alone is `closing` or `conditionally closed`.

## 11. Stop conditions and deferred work

Stop and register another decision/corrective if the change would: loosen `PrivateBounded` or C004 proof; silently skip unreadable roots; follow symlinks; broaden cleanup from a known set to `/`; add a persistent background watcher, state schema migration or CLI flag; change long-term retention for **existing** directories; rewrite accepted ADR/closure history; or require a release-tag/registry mutation before product qualification.

No hard-coded exclusions for `.codex`, `.git`, temporary worktrees, filesystem paths or particular home names. This is a general filesystem lifecycle issue.

## 12. Required closure handoff

Write `plans/closure/artifact-discovery-cleanup/c028-status.md` only after implementation. Include the implementation commit/PR, requirement-to-test evidence matrix for §10, failing original baseline reproduction, exact post-fix direct and Cargo-subcommand runs, fixture-created and fixture-deleted paths, root-classification and reappearance evidence, all-mode zero-spawn controls, state before/after showing Full-only negative pruning, platform/MSRV CI links, commands actually run (including failures), residual risks, and eventual published artifact/installed-release evidence. Update this subsystem roadmap and `plans/registry.md` to the disposition actually earned. No green-by-description closure.
