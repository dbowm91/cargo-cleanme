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
   `src/domain.rs:255` is the vocabulary. `Shared`, `Uncertain`, and
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
6. **The discovery universe may only ever grow.** ADR 009 widened the
   maintenance scope by adding the working directory — *added*, never
   substituted for the Routine roots (`src/main.rs:1189-1198`). A wider
   universe can only reclassify `shared` bytes to `private`; it can never
   promote an unproven shared output. Any change that **narrows** the
   universe re-opens exactly the hazard invariant 1 exists to prevent, so
   treat a narrowing change as a safety change, not a cleanup.
7. **An unproven learned root leaves the *candidate* set, never the
   *discovery* set.** The Full cleanup's gate is per-root
   (`src/policy.rs:551-620`): a learned root whose subtree could not be fully
   read is disclosed and its workspaces are dropped from the candidate set —
   but they are still discovered, resolved, sized and classified, so a shared
   output between a proven root and an unproven one stays `Shared` and stays
   inventory-only. Dropping *candidates* cannot reclassify anything; dropping
   *participants* can, which is invariant 6's hazard in a new costume.

## Invariant 2 is the trap

It is one `if` in one function, it is easy to read as redundant, and relaxing it
converts a user-facing configuration key into a way to delete shared build
output. The check is deliberately placed before any path handling so that a
future refactor cannot accidentally make authorization load-bearing for
ownership. If you touch `covering_is_authorized_roots`, re-read
[`overview.md` §8 invariant 2](../../architecture/overview.md) first and say
in your change why the ordering still holds.

## Deletion is always Cargo's

Deletion is performed by `cargo clean`, never by direct filesystem removal, so
Cargo's own bookkeeping cannot be bypassed. Do not "optimise" a direct
`remove_dir_all` in for performance — it would be a semantic change to the
authority model, not an optimisation.

## The three modes, and the hidden aliases

`src/cli.rs::clean_mode` is authoritative. `src/cleanup.rs::CleanMode` is the
vocabulary.

| Mode | Flag | Cargo clean invoked? |
|---|---|---|
| `Execute` | default, or `--yes` (hidden alias) | yes |
| `Simulate` | `--dry-run`, or `--dryrun` (hidden alias) | **never**, not even preview |
| `CargoPreview` | `--cargo-preview` | yes, as `cargo clean --dry-run --verbose` |

`--dry-run` and `--cargo-preview` are deliberately different spellings with
different meanings. `--dryrun` is a hidden alias of `--dry-run` and `--yes` is a
hidden alias of the (now default) `Execute`; neither alias introduces a fourth
mode or a different safety path. Do not "help" by reintroducing a
`Preview`-default table or by making `--dry-run` spawn a Cargo preview.

Two details the shorthand hides:

- The three flags conflict pairwise **on `clean`**. Across the root and the
  `clean` alias they do not: a simulation at *either* level wins over a Cargo
  preview at *either* level, because simulation spawns nothing
  (`src/cli.rs:631-636`). See
  [`cli-surface-and-argv`](../cli-surface-and-argv/SKILL.md).
- There is **no `Preview` mode**. `CargoPreview` maps to the *wire* label
  `"preview"` (`src/cleanup.rs:52`), and `CleanOutcome::Previewed` is an
  outcome, not a mode. Writing "Preview" in a mode table is how the old
  whole-scope rule survived in this file for so long.

`Simulate` (`--dry-run`) is not a weaker mode: it runs discovery, resolution,
filtering, sizing, authorization, revalidation, progress, and reporting, and
only the deletion step is a no-op. It shares the same completeness and
final-proof gates as `Execute`, so it is the mode to use when testing a change
to the gates.

## Unresolved ownership blocks the boundary, not the universe

This is the rule most often got wrong, including by earlier versions of this
skill. It is **not** "any unresolved manifest blocks the whole scope".

Every `Cargo.toml` under the selected cleanup root must be accounted for by
direct successful resolution or by successful workspace metadata listing it as a
member/root. What happens to the ones that remain is a **three-way split**
(ADR 007 §1–§4, implemented by `split_unresolved_for_boundary`,
`src/cleanup.rs:2742-2766`):

| Where the unresolved manifest is | What happens |
|---|---|
| **Outside the proof boundary** | Reported as an `out_of_scope` diagnostic. **Never blocks.** |
| **Inside the boundary**, unknown owner (`MergeAll`) | **Blocks the boundary.** Fails closed. |
| **Inside the boundary**, Cargo named the owner (`PassengerOf`) or the scope (`ScopeNote`) | Reported. Never blocking. |

The **proof boundary** is the scope the operator selected: the working-directory
anchor, the explicit roots, or the Full set.

Cleaning a healthy boundary while poison exists elsewhere is the *intended*
behaviour, precisely so that a boundary clean can never be read as a system
clean. The regression test is `out_of_anchor_poison_does_not_block_anchor`
(`src/cleanup.rs:6035`) — it **fails by construction** against the old
whole-scope rule, which is what makes it worth keeping.

Read-only scans stay partial-result tolerant; cleanup does not. That asymmetry
is intentional.

> **Do not reintroduce a whole-scope block to "be safe".** It would not make
> the tool safer; it would make it refuse the operation ADR 007 was accepted to
> fix, and it would silently contradict the documented behaviour in five files
> across `docs/` and `README.md`.

Note the consequence for tests: asserting "zero clean spawns" under a blocked
boundary is the strongest available assertion, and `cleanup.rs` has several —
`metadata_failure_blocks_boundary_with_zero_spawns`
(`src/cleanup.rs:6266`) and
`violated_admission_premise_blocks_with_zero_cargo_spawns`
(`src/cleanup.rs:7324`). Prefer that over asserting an error message.

## The Full cleanup gate is per-root, not whole-scope

`cargo cleanme --full` proves ownership **per learned root**. When the platform
walk could not complete, each learned root is re-walked as its own walk root
and the existing per-walk-root attribution decides whether that root's subtree
was fully read (`src/discovery.rs:586-592`). The pass uses the same prune and
filter sets as the platform walk, because a verdict computed under different
rules is not comparable to the verdict it stands in for.

The gate returns a verdict, not a refusal (`src/policy.rs:551-620`):

- A **proven** root is a cleanup candidate.
- An **unproven** root is disclosed on stderr (`src/main.rs:389-401`) and its
  workspaces leave the *candidate* set — each is still discovered and
  classified, and each appears in the report with the typed reason
  `skipped_incomplete_coverage` (`src/cleanup.rs:250-269`).
- If **no** root could be proven, the operation **blocks** with the standard
  unproven-ownership scope block (`src/main.rs:776-806`). It never exits 0
  with zero rows.
- If **some** root was unproven, the exit code is **1**
  (`src/main.rs:808-821`), so an unattended script cannot read a partial Full
  cleanup as a complete one.

The typed reason is not optional. "We could not read this root" and "you did
not ask for this" are different facts; that difference is the whole of
`skipped_incomplete_coverage` versus `skipped_outside_selection`, and a
consumer that cannot tell them apart cannot report coverage honestly.

Do not turn this back into a whole-scope refusal: the old gate refused the
entire Full cleanup because the reconciliation's exit code was non-zero, so
one unreadable directory anywhere on the machine blocked every learned root.
The C028 property survives — a missing fresh in-memory generation is still
fatal, and means no cleanup at all.

**The per-root rule needs an attribution to work.** A traversal error carries
**no path** — the upstream walk's error event has no child path — so it cannot
be placed in a subtree, and a rule that counts every pathless diagnostic as
in-boundary is the whole-system refusal wearing a new hat. The cleanup engine
therefore walks **per root**, so the error is attributed to the root whose
walk produced it (`src/cleanup.rs`), and only a root the gate already judged
and disclosed stays out of the boundary. Two rules keep this honest:

- a pathless diagnostic from a root the gate **did not** judge still blocks
  the scope that contains it — an unreadable automatic root is indeterminate
  coverage, not absence
  (`unreadable_automatic_root_blocks_instead_of_omitting`);
- the excluded root leaves the **proof boundary** as well as the candidate
  set, so an unprovable participant inside it is reported, not blocking. That
  is the one place a filter may unblock a scope, and it is safe only because
  discovery does not narrow: the root's manifests are still discovered,
  resolved and classified.

## The `skipped_outside_selection` reason code exists

An out-of-boundary participant is not a failure and not a skip of a candidate
that was in scope. It has its own reason code, `skipped_outside_selection`, so
a consumer can tell "never a candidate" from "considered and refused". Do not
collapse it into a generic `skipped`; the distinction is what lets a JSON
consumer report coverage honestly.

## Before you claim the change is safe

- `--dry-run` produces the same unit set, the same skips, and the same typed
  reason codes as default `Execute`, with zero spawns.
- Any new `Shared`/`Uncertain`/`ExternalUnproven` case is inventory-only in
  **both** the scan report and the cleanup decision.
- `cargo cleanme … --format json` round-trips: the `reason_code` for each
  unit explains its skip, and `skipped_ownership_unproven` is distinguishable
  from `skipped_changed_before_cleanup` (they warrant different operator
  responses).
- Nested and equal `target`/`build` roots still collapse to one group and one
  deduplicated byte union, so one invocation produces one result and one
  measurement.
- **An out-of-boundary unresolved manifest still does not block**, and an
  in-boundary unknown owner still does. If your change touched
  `split_unresolved_for_boundary` or `boundary_admits`, say which of the three
  ADR 007 cases you changed and why.
- A Full cleanup that could not prove a learned root reports that root with
  `skipped_incomplete_coverage` and exits 1 — never 0. A Full cleanup that
  proved *no* root blocks; it never exits 0 with zero rows.

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
- [`terminal-and-interactive`](../terminal-and-interactive/SKILL.md) — the `-i`
  selection, which can only *narrow* what this skill authorises
