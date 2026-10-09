# ADR 004 (proposed) — Bounded Independent Cleanup Proof Islands Under Partial Discovery

Status: **proposed — not accepted, not authorization for implementation**.
Date: 2026-10-09.
Repository baseline: `f08705e77716e445e50097333a0313cab9574cdd`.
Related corrective: `plans/implementation/artifact-discovery-cleanup/c031-happy-path-ownership-scope-and-partial-failure-corrective.md`.
Supersedes nothing until accepted. This is an explicit **proposed change** to the operational consequences of accepted ADR 001 (C003/C004/C006), not a reinterpretation of those existing guarantees.

## Context

Live Routine Execute selected ten persisted/seeded roots, found 2,014 manifests, emitted ten discovery diagnostics and reported **zero resolved workspaces and CleanupUnits**. The zeroes mean that `src/cleanup.rs` returned early from `if !diagnostics.is_empty()`, before Cargo resolution, **not** that the resolver failed 2,014 times. Current C006 treats every admitted Routine root as one combined ownership universe. A single unreadable traversal subtree or unresolved `Cargo.toml` blocks all cleanup modes for every workspace in that universe.

This is conservative and guards against undiscovered workspaces sharing or redirecting output. It is also unexpectedly fragile for a routine developer utility that learns directories across independently changing tools, fixtures and worktrees. C029 addresses accurate errors and Full coverage; C030 addresses root contamination. Neither independently establishes that one unresolved manifest *inside an otherwise valid selected root* should be allowed to coexist with destructive cleanup of a sibling.

## Present binding decision

ADR 001 §C004 and C006's accepted corrective require complete coverage of every selected root in the combined scope and block all modes on unresolved manifests and discovery uncertainty. **This remains binding.** No implementation may simply remove `if !diagnostics.is_empty()`, reinterpret all `PermissionDenied` as safe, skip unresolved participants, or restart cleaning each root with a narrower list.

## Question to decide

May automatically selected Routine roots be partitioned into smaller *independently complete and defensibly non-overlapping* ownership proof universes, allowing a sound private workspace to clean while a disconnected candidate is blocked? If so, what constitutes independently proven non-overlap when Cargo permits redirected `target-dir`/`build-dir` and environment-selected output even across source roots?

Explicit `clean ROOT` should remain bounded to the user-selected root and retain the current complete-scope proof by default. `clean --full` should not weaken the fresh Full-generation contract. An explicit operator opt-in cannot manufacture exclusive ownership evidence.

## Proposed conditional direction (requires acceptance and tests)

1. Separate **discovery reachability** from **cleanup proof completeness**. Discovery may yield useful positives while tracking exact missed subtrees/unknown unknowns. A cleanup proof unit must still be complete over its declared scope.
2. Construct prospective components from canonical discovered manifest identities, Cargo-authoritative workspace root/member/output sets, and physical-output overlap. An unresolved manifest is a potential owner, not a definitively disjoint project merely because it resides in a different folder. A failed traversal with unknown child path is an unknown participant.
3. The partitioning algorithm must treat any uncertain cross-component relationship as an **edge**, merging the connected scopes or blocking all potentially affected scopes. Do not rely on path disjointness of source trees alone; a crate under `/tmp` can set `CARGO_TARGET_DIR` to the target of a workspace under `~/projects`.
4. A proposed *independence certificate* must identify the exact finite selected roots, every resolved workspace/output location, every unresolved manifest and unreadable region relevant to those roots, Cargo/environment configuration provenance and an explicit proof of why no excluded participant may claim bytes in the proposed CleanupUnit. Without a defensible certificate, do not execute cleanup. Merely restricting a unit to a conventional `target` directory is not sufficient proof of universal exclusivity.
5. No partition may be constructed solely by removing an unsafe root after initial combined discovery. Freeze root classification and provenance, then rebuild discovery and proof within the chosen authoritative boundary. Keep omitted-root and selected-root late checks from C028 before every spawn.
6. Every atomic Cargo invocation remains one `CleanupUnit` covering the full `OutputSet`; any Shared, Uncertain, ExternalUnproven, or unauthorized output blocks the entire unit. Never use partial metadata as a reason to convert an output to PrivateBounded.
7. Preserve per-mode parity: Preview, Simulate and Execute receive identical partition/block dispositions and full proof. Simulate invokes no `cargo clean`; Preview cannot bypass ownership; Execute invokes Cargo only for proven units. Reporting must show true partial failures and whether units were considered, not misleading `0 resolved` or claimed recovered bytes.
8. If actual deterministic tests cannot establish the certificate model in the presence of redirected/cross-root output, **reject this direction** and retain the C006 whole-scope block. Pursue better root hygiene and more targeted operator-selected scopes instead.

## Alternatives and risks

- **Remove the diagnostic guard and continue:** rejected; discovery holes can hide owners.
- **Ignore only permission warnings:** rejected as a cleanup authorization shortcut. It is reasonable for read-only discovery but insufficient for exclusive ownership proof.
- **Per-root fallback after combined-scope failure:** rejected; silently shrinks the universe and changes what overlap means.
- **Treat selected roots as isolated by default:** rejected because Cargo can redirect outputs across roots; root containment is not a Cargo configuration boundary.
- **Keep the combined universe and improve discovery/learned scope (C029/C030):** safe fallback; may produce the primary desired happy path without this decision. Preserves status quo for remaining genuinely uncertain scopes.
- **Require explicit scope for all cleanup:** simpler but changes the canonical bare `cargo cleanme` contract, so would need its own product/CLI decision.

## Evidence required before acceptance

A bounded graph/certificate design with exact candidate and negative participant semantics; demonstrably failing baseline fixture; a failing-direction **cross-root redirected output** control (source A and B under disjoint roots, same configured build/target); unresolved-manifest overlap; permissions with known and unknown child; symlink output; altered environment between discovery and pre-spawn; a target under another source tree; parallel cleanup and TOCTOU; partial success output/exit contracts. Include hosted Linux/macOS/Windows and Cargo 1.89 compatibility before any destructive activation.

Acceptance requires: (a) explicit amendment/clarification of ADR 001 C004/C006; (b) a fresh, auditable proof design with negative controls; (c) versioned/output-contract assessment if statuses change; (d) a rollback/kill-switch plan; (e) a distinct release gate with disposable fixtures and published-artifact smoke. Until these exist, C031 is **blocked** as to destructive partitioning and may conduct research only.

## Current disposition

**Open question, proposed**. C029/C030 are the immediate safe corrective line. Historical records must remain unchanged, and planning handoff does not authorize new destructive behavior.
