# C030 — Learned-Root Provenance and Routine Maintenance Scope Hygiene

Status: **conditionally closed** (implementation and local gates complete; hosted macOS checks and installer jobs remain queued in CI run `37953997910`).
Planning branch: `plans/c029-c031-permission-discovery-cleanup-reliability`.
Repository baseline: `f08705e77716e445e50097333a0313cab9574cdd` (`main`, 2026-10-09).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: M006E/C005, M007/C006, M012A, C028, and ADR 002 §4. Historic closure records remain unchanged.
Dependency: C029's typed coverage model is an **interface dependency** for negative-state reconciliation; learned-root candidate classification and regression fixtures can proceed independently. C031's effective-scope decision depends on this plan's verified root provenance.
Skills: `.skills/planning-and-closure`, `.skills/discovery-and-ownership`, `.skills/cleanup-safety`, `.skills/test-evidence`, `.skills/json-and-exit-contract`.

## 1. Reported failure, code path, and previous evidence gap

Following Full discovery, bare `cargo cleanme` selected ten maintenance roots including `~/.codex/worktrees/<id>`, Trash, nested Minimax/OpenClaw/node_modules package caches, a Go module download tree, a fixture directory, `~/projects`, and `/tmp`. Seven now-deleted Codex paths were correctly omitted by C028, but the retained effective root set was still too broad and noisy for daily cleaning.

`src/discovery_state.rs::reconcile_full` builds a `ProjectObservation` for **every** discovered manifest. The `workspace` field is optional when Cargo resolution fails; the state then stores an unresolved project with its manifest parent as `workspace`. Its learned-root loop uses `o.workspace.as_deref().or_else(|| o.manifest.parent())`, so unresolved/vendor/cache `Cargo.toml` files may generate learned roots, promoted to their parent directory. The broad-root guard `is_broad_root` only protects selected filesystem/platform paths and direct home, not package managers' download directories, temp fixtures, or ephemeral tool worktrees. Age retention permits those roots for up to 30 days; parent-container collapse can amplify their effect. C028 removed positively missing roots **at admission** but correctly did not address whether a still-existing learned location is meaningful.

The earlier closure evidence validated retention, disappearance, reconciliation, and completeness primarily on well-behaved local workspaces. It did not exercise a realistic full-home mixture of Cargo manifests from nested npm/Go caches, Trash, transient tools and legitimate user projects, nor distinguish *discovered* from *appropriate to learn*. Current integration tests demonstrate that an admitted root is safe to validate, not that the default selection is helpful.

## 2. Contract, invariants, and non-goals

Full `cargo cleanme scan` remains an exhaustive, read-only best-effort inventory of platform roots under the accepted explicit system/symlink prunes. **Do not silently hide legitimate projects from Full inventory merely because they live under an unusual directory.** The learned maintenance scope is a different, more selective concept: automatic re-visitation hints, not a security or ownership policy.

Only **Cargo-resolved** workspace identities may contribute candidates to persisted learned maintenance roots. Unresolved Cargo manifests can remain visible in the exact project inventory with `ManifestObservedUnresolved` and diagnostic context, but they are not positive evidence for expanding routine scope. If resolution genuinely cannot occur because of access/IO, guard against destructive negative pruning using C029 coverage; never convert failed resolution into inferred absence.

Recognized vendor/cache/Trash/temp/runtime directories must not be **implicitly learned** for Routine use. Explicit `scan ROOT`, configured `scan.root`, and deliberately opted-in paths remain authoritative under existing strict admission/safety rules. Do not confuse `skip automatic learning` with `exclude from Full discovery`, or with `safe to clean`. Do not make a globally blacklisted basename like `tmp`, `build`, `cache`, `target`, `Code`, or `worktrees` suppress arbitrary user projects: use canonical location + ownership/provenance + path-component context and a tested documented override.

No automatic `sudo`, root walks for Routine, arbitrary direct deletion, or cleanup of Cargo home/global caches. Learned state cannot authorize output or prove exclusive ownership. Conventional `target`/build and physical output relationships remain Cargo-authoritative. Preserve `recency_seconds=300`, learned retention 30 days (0 disables age expiration), and state/schema compatibility or make a separate migration decision.

## 3. Proposed policy decisions to implement and test

1. **Candidate source:** for newly observed manifests, require a successfully resolved workspace. Resolve the workspace root and its direct parent through the accepted ADR 002 rule before applying the new maintenance-root classification.
2. **Origin typing:** distinguish: user-selected explicit/configured root; bounded developer-directory seed; successfully resolved developer workspace learned candidate; unresolved discovered manifest; previously persisted learned state of unknown provenance. Do this without treating persistent state as authority. Keep visible exact manifest inventory separate.
3. **Cache/runtime hygiene:** classify common package-management and ephemeral locations as `not_auto_learned` (e.g. `$HOME/.local/share/Trash`, `$HOME/.npm*/.../node_modules`, typical Go module caches under `GOMODCACHE` when available, `$CARGO_HOME/registry`, transient `$HOME/.codex/worktrees/<id>`, ephemeral `/tmp`/platform temporary directory and fixtures). Prefer environment/platform canonical roots and tested ancestor relationships over fragile textual substring matching. Do **not** prevent explicit scans of these directories.
4. **Parent promotion:** never climb from a resolved workspace into a transient/cache/Trash root or an unreasonably broad home/platform ancestor. Where suitable, retain the workspace root itself *only if* it is a durable, appropriate Routine candidate; otherwise inventory it but do not learn it automatically. Codex worktrees are legitimate Rust projects, but ephemeral by default and should not persist as recurring daily roots.
5. **Persisted-state hygiene:** on complete, certain Full reconciliation, revalidate previously learned unknown-origin locations against the same candidate policy. Remove disallowed automatic roots using **positive proof of disallowed provenance/class**, not guessed absence or path-access failure. Respect C028 last-seen and uncertainty handling; preserve schema-forward safety, partial Full no-negative-pruning behavior, old valid roots and atomic publication. If accepted ADR 002's age/absence semantics would be changed, document and secure a forward ADR clarification **before implementing destructive-negative state changes**. Do not silently prune records merely because a partial scan found no manifests.
6. **Maintenance admission:** C028 still omits missing automatic roots and blocks uncertain/symlink admissions. Ensure Routine read-only `scan --known` and bare Execute/Simulate resolve the same effective set after root-hygiene filtering, without independently broadening the set. Do not write negative state on Routine use.
7. **Escape hatch:** preserve current explicit `scan.root` for one deliberate exclusive scope. Consider a separately planned explicit additive `scan.known_roots` only if necessary; if added, document config/schema/migration and retain strict path safety. Do not introduce an undocumented environment switch or default to `--full` for maintenance.

## 4. Ordered implementation work

### A. Baseline and policy inventory

Create a purpose-built matrix from the observed root examples, `src/policy.rs`, `src/discovery_state.rs` and `docs/USAGE.md`. Capture one exact state fixture containing valid `~/projects` workspaces, unresolved dependency manifests, a resolved nested cache workspace, a current/removed Codex worktree, Trash, a Go module cache, and `/tmp`. Record which are discovered, which are learned and why, and which survive into Routine. Demonstrate that at least one junk automatic root gets learned at `f08705e`; otherwise the regression is non-discriminating.

### B. Pure maintenance-root classification and provenance seam

Implement shared, independently testable classification in the scope/state layer; thread it through Full candidate creation, state reconciliation, and Routine effective selection. Keep scan discovery filters separate; never globally prune directories solely to produce a cleaner learned-root report. Canonicalize with symlink protections and stable, auditable path rules. Ensure nested legitimate user projects and explicit override controls survive.

### C. State reconciliation and upgrade behavior

Qualify transitions for preexisting state generations whose roots do not carry newly introduced provenance information. Prefer a migration-free derived classification if it is sufficient. If storing provenance is necessary, introduce an explicit, versioned migration plan with future-schema protection. Preserve previous complete generation on partial Full, case-insensitive filesystems where applicable, and no silent destructive scope expansions after migration.

### D. Operator feedback and documentation

Report concise Routine root counts by origin (`seed`, `learned`, `omitted_absent`, `not_auto_learned`) where compatible; avoid dumping huge root lists or absolute paths into `--format log`. Explain exclusion decisions, Full vs Routine separation, and explicit overrides in `docs/USAGE.md`/`TROUBLESHOOTING.md`/`AUTOMATION.md`; update architecture, tests, release notes and generated docs only where affected. Do not rewrite historical closed plans.

## 5. Tests: positive, negative, and false-green controls

- Full scans find valid `Cargo.toml` in a package-manager cache but do not automatically select the cache for Routine. A valid workspace in `~/projects` is Full-discovered and learned normally.
- An unresolved manifest is retained as `ManifestObservedUnresolved` in positive inventory, but does not generate a learned root. A previously resolved workspace that temporarily fails metadata resolution is not declared gone based on that failure.
- Real Go module cache, npm package, Cargo registry, Trash, and per-session Codex worktree examples; use temp/home fixture variable paths and alternate spelling/platform cases rather than exact hardcoded user account names.
- Temporary worktree ephemeral vs lasting explicitly configured project; nested legitimate workspace in unusual paths; explicit `scan ROOT` always still discovers; configured `scan.root` remains an override, with no unexpected ancestor/broad path selection.
- Existing persisted state containing bad learned roots is handled conservatively in a complete certain Full; inaccessible or unknown-provenance paths do not trigger unsupported negative pruning. C029-known/unknown uncertainty cases, C028 missing/reappearing root, state v1/current/future schema guards, retention 0/30 and future clock.
- No automatic leaf-to-parent promotion into home, root, Trash, cache or transient installation; test actual resolved root set, not only the classifier itself.
- Bare `cargo cleanme --dry-run` with one legitimate project succeeds when every selected scope is provable; real Execute uses `cargo clean` only for proven private, inactive output. If unrelated unresolved manifests remain under an intentionally selected real root, keep existing whole-scope block until C031 is accepted and implemented.
- Stable human/JSON/log report shape and consistent selected roots in `scan --known` and Execute; no paths in log. Linux/macOS/Windows/MSRV 1.89 hosted qualification. Tests must fail on the baseline for the actual learned-root contamination, not just for a comment or diagnostic string.

## 6. Stop conditions, compatibility, and closure

**Stop** if classification suppresses Full inventory, broadens destructive scope, breaks explicit override, strips positive project records, or converts unknown coverage into negative evidence. Do not add a catch-all `path.contains("node_modules")` production filter without canonical-path and override controls. Do not use new selection heuristics as proof of output exclusivity.

Follow repository verification ladder in `.skills/test-evidence`: fmt; clippy all targets/features; tests all targets/features; fixture-portability and doc-citation checker self-tests/checks; `git diff --check`; hosted Linux/macOS/Windows and MSRV 1.89. Record CI links and full test outputs in `plans/closure/artifact-discovery-cleanup/c030-status.md`; implementation is not release qualification.

Acceptance: Full still inventories unusual accessible Cargo manifests; normal Routine roots are reasonable developer search locations rather than transitive vendor/cache/tmp state; unresolved manifests no longer fabricate learned roots; already learned roots are reconciled safely without losing coverage; default cleanup still follows C003/C004/C006 proof. This plan does not permit changing their complete combined-universe invariant.
