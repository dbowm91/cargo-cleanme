# ADR 007 (accepted) — Scoped Proof Boundary; Out-of-Scope Blocking Is a Product Defect

Status: **accepted 2026-10-16** — owner-directed. Intentional product change:
whole-scope blocking of healthy in-scope units by out-of-scope unprovable
participants is a core product defect, not a safety feature. This amends the
operational consequences of accepted ADR 001 (C003/C004/C006); the
underlying guarantees (complete proof *within* the boundary, Cargo-mediated
deletion only, per-spawn revalidation) are unchanged.

Related: M014 (CWD anchor as selection), M015 (island certificate), M016
(opt-in per-island activation — superseded by this decision before any
publication), C035 Route B (operator-declared scopes — superseded as a
mechanism, adopted as a premise).

## Context

The whole-scope block was the observed product failure behind the entire
C029→C035 lineage: one unreadable subtree or unresolvable manifest anywhere
in the selected roots blocked every cleanup mode for every workspace in that
universe, including healthy projects the operator could see were unrelated.
C035 proved the shape of the fix on its branch (combined discovery and
grouping, per-scope proof, poison blocks only its own scope) but tied the
boundary to a config-file declaration. M016 proved automatic per-island
activation behind an opt-in flag but kept blocking as the default. Both
treatments agree on the mechanism; this decision fixes the product rule:
**blocking is the defect, scoped proof is the default, and the boundary the
operator already asked for — the working directory — is the scope.**

## Decision

1. **The proof boundary is the selected scope, not the discovered universe.**
   Bare `cargo cleanme` and `clean --known` prove the working-directory
   anchor (M014's selection becomes the proof boundary, not just a candidate
   filter). `clean ROOT` proves exactly those roots. `clean --full` proves
   the full system and retains whole-system fail-closed blocking — the
   operation whose job is system-wide reconciliation keeps the old rule.
   `scan --all` is unchanged: read-only whole-system inventory.
2. **Discovery and grouping stay whole-system.** The M014 measurement stands:
   narrowing discovery silently promotes `Shared` to `PrivateBounded`.
   Cross-boundary output claims remain visible at grouping time and still
   force `Shared`/skip. What changes is only which unprovable participants
   can block a dispatch.
3. **Out-of-boundary unprovable is reported, never blocking.** An unresolved
   manifest or discovery diagnostic outside the proof boundary becomes a
   bounded diagnostic (existing C032 cause chains), not a block. Cleaning a
   healthy anchored scope while poison exists elsewhere is the corrected
   behavior, and any test asserting the old whole-scope block for
   out-of-boundary poison names the defect this decision fixes.
4. **In-boundary rules are unchanged and still fail closed.** An
   in-boundary `MergeAll` unknown (byte-capability unproven) blocks the
   boundary. Refused-manifest passengers and scope notes are reported,
   never blocking — the M016 rule as the default. `Shared`,
   `Uncertain`, `ExternalUnproven`, inactive, unauthorized, and unproven
   output stay inventory-only. Per-spawn revalidation, the hoisted-universe
   refresh, and environment re-verification still run per dispatch. The
   refused-manifest passenger rule (ADR 004 amendment, M016) is retained:
   a manifest Cargo itself refuses still rides as a passenger rather than
   blocking its boundary.
5. **Exits are boundary-scoped.** 0 when the requested boundary is fully
   cleaned (out-of-boundary poison reported as diagnostics); 1 when the
   boundary itself is blocked or a unit fails; 2 for usage errors. The M016
   rule (nonzero while anything anywhere is held) is superseded.
   Zero-unit edge (fail-closed, M012A): reported notes never block
   *proceeding* units — but a run that proves zero units while any
   participant is unproven proves nothing. Exiting 0 there would be the
   silent no-op, so it fails closed with the standard unproven-ownership
   block. Units merely unselected (outside the anchor) do not count.
6. **One mechanism, not two.** The `--partitioned` flag and the `islands`
   report axis are removed before any publication (0.2.9 was never
   published; the flag has no users). The `[maintenance] scopes` config
   mechanism is dropped; the C035 branch is superseded and must not be
   merged — its premise is adopted here, its mechanism is not. The M015
   whole-scope partition report retires with the whole-scope block it
   reported on; the island *certificate* computation is retained only where
   the scoped engine reuses it.

## Residuals (disclosed, not fixed)

- An out-of-boundary unresolvable manifest's unknown output cannot be
  disproven; combined grouping still catches every *resolved* cross-boundary
  claim, and the remainder is the same documented residual C035 carried.
- Poison *inside* the boundary still blocks the boundary, by design.
- `clean --full` still blocks whole-scope on any unprovable participant:
  that operation asserts system-wide proof and keeps the old rule
  deliberately.

## Consequences

- Machine contract: exits become boundary-scoped (decision item 5); the
  never-published `islands` field is removed; `unresolved_ownership` lists
  boundary-relevant participants only; out-of-boundary poison travels in
  `diagnostics`. Version bump required (0.2.9 carries it).
- Milestones: M017 implements this decision. M016 stays closed as
  implemented (its hoisted refresh, env re-verification, and passenger rule
  are reused) but its opt-in design is superseded. C035-as-config is
  superseded; its branch stays unmerged as the record of the rejected
  mechanism.
