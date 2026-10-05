---
name: cleanup-safety
description: Changing anything that can delete bytes in cargo-cleanme — ownership proof, authorization, cleanup units, fail-closed rules, and the traps that weaken them invisibly
version: 1.0.0
tags:
  - safety
  - cleanup
  - ownership
  - invariant
---

# Cleanup Safety

`cleanup.rs` is the only place in the repository that destroys data. Everything
here is a consequence of one idea:

> **Nothing is ever deleted on the strength of a timestamp. Every candidate must
> first prove exclusive ownership of its physical bytes, and that proof must
> still hold at the moment of deletion.**

The architecture reference is
[`architecture/09-cleanup.md`](../../architecture/09-cleanup.md) and ADR 001.
This skill is about *how to change it without weakening it*.

## When to Load

- Any edit to `src/cleanup.rs`, or to `src/workspace.rs` where it builds groups
  or cleanup units
- Any change to `cleanup.allowed_output_roots` or the `[cleanup.policy]` section
- Any change that alters which paths become `PrivateBounded`
- Any performance work in the cleanup or traversal path

## The invariants, and the one that is easiest to break

1. **Ownership proof, not timestamps.** `OutputOwnershipClass` in
   `domain.rs:186` is the vocabulary. `Shared`, `Uncertain`, and
   `ExternalUnproven` are **inventory-only, forever**.
2. **Authorization never manufactures proof.** `allowed_output_roots` lets a
   user authorize a location; it cannot upgrade an ownership class.
   `covering_is_authorized_roots` returns `Ok(false)` for any non-`PrivateBounded`
   class *before* it inspects a path. Do not "helpfully" reorder those checks.
3. **Re-validate at the last moment.** Proof is repeated immediately before each
   spawn, because a workspace can change between scan and delete.
4. **Fail closed on uncertainty.** Every uncertainty becomes a diagnostic, and
   uncertainty propagates into state reconciliation.
5. **The destructive unit is one workspace `CleanupUnit`** over its complete
   `OutputSet` — exactly one `cargo clean` invocation, one resolved workspace,
   every physical group it can affect. A private `target` must never carry an
   `ExternalUnproven`/`Shared`/`Uncertain` sibling `build` directory into the
   same invocation; such a unit is skipped **whole**.

## Invariant 2 is the trap

It is one `if` in one function, it is easy to read as redundant, and relaxing it
converts a user-facing configuration key into a way to delete shared build
output. The check is deliberately placed before any path handling so that a
future refactor cannot accidentally make authorization load-bearing for
ownership. If you touch `covering_is_authorized_roots`, re-read
[`overview.md` §6 invariant 2](../../architecture/overview.md) first and say
in your change why the ordering still holds.

## Deletion is always Cargo's

Deletion is performed by `cargo clean`, never by direct filesystem removal, so
Cargo's own bookkeeping cannot be bypassed. Do not "optimise" a direct
`remove_dir_all` in for performance — it would be a semantic change to the
authority model, not an optimisation.

## The three modes, and the two spellings

| Mode | Flag | Cargo clean invoked? |
|---|---|---|
| `Preview` | default, or `--dry-run` | yes, as `cargo clean --dry-run --verbose` |
| `Simulate` | `--dryrun` | **never**, not even preview |
| `Execute` | `--yes` | yes |

`--dry-run` and `--dryrun` are deliberately different spellings with different
meanings, and they conflict pairwise with `--yes`. This has been called
error-prone in review more than once. The CLI contract says: reconcile the
contract explicitly rather than silently aliasing. Do not "help" by making one
an alias of the other.

`--dryrun` is not a weaker mode: it runs discovery, resolution, filtering,
sizing, authorization, revalidation, progress, and reporting, and only the
deletion step is a no-op. It shares the same completeness and final-proof gates
as `Execute`, so it is the mode to use when testing a change to the gates.

## Unresolved ownership blocks the whole scope

Every `Cargo.toml` under the selected cleanup root must be accounted for by
direct successful resolution or by successful workspace metadata listing it as a
member/root. Any remaining unresolved manifest blocks Preview, Simulate, and
Execute **for the complete selected-root scope** — no `cargo clean` runs at all,
including Cargo's own preview dry-run. Read-only scans stay partial-result
tolerant; cleanup does not. That asymmetry is intentional.

Note the consequence for tests: asserting "zero clean spawns" under a blocked
scope is the strongest available assertion, and there is an all-mode
zero-spawn test for it in `cleanup.rs`. Prefer that over asserting an error
message.

## Before you claim the change is safe

- `--dryrun` produces the same unit set, the same skips, and the same typed
  reason codes as `--yes`, with zero spawns.
- Any new `Shared`/`Uncertain`/`ExternalUnproven` case is inventory-only in
  **both** the scan report and the cleanup decision.
- `cargo cleanme clean … --format json` round-trips: the `reason_code` for each
  unit explains its skip, and `skipped_ownership_unproven` is distinguishable
  from `skipped_changed_before_cleanup` (they warrant different operator
  responses).
- Nested and equal `target`/`build` roots still collapse to one group and one
  deduplicated byte union, so one invocation produces one result and one
  measurement.

Then run the full gate (see [`test-evidence`](../test-evidence/SKILL.md)):
`cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D
warnings`, `cargo test --all-targets --all-features`.

## Related

- [`architecture/09-cleanup.md`](../../architecture/09-cleanup.md) — the
  authorization, proof, and pre-spawn decision in detail
- [`architecture/04-policy-and-scope.md`](../../architecture/04-policy-and-scope.md)
  — the Routine/Full decision that selects cleanup roots
- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md) — the
  machine-readable contract these decisions surface through
