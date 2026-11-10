# M022 — Clean a proven workspace past a bounded unresolved claim

Status: **closing**. ADR 010 was accepted 2026-11-09, owner-directed,
and the amendment is in ADR 007 item 4. Implemented in the working tree
the same day. Not published, so the plan stays open. Acceptance is not
a release.
Date: 2026-11-09.
Repository baseline: `main` at `5ad76cf` (published v0.2.13).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Authority: `plans/adr/010-proposed-bounded-output-claim-for-unresolved-manifests.md`
(accepted 2026-11-09; the acceptance block is in ADR 007 item 4).
Depends on: ADR 010 acceptance — **hard**. C047 — **soft**. This plan
is safe without C047, because an unbounded claim still blocks the
whole current boundary. It is more useful after C047, because that
boundary is then one learned root rather than every proven root.
Related: ADR 001 C004/C006, ADR 004, ADR 007 item 4, M015, M016, M017,
M018, C047.

## Objective

A manifest `cargo metadata` cannot load stops blocking cleanup of
workspaces whose physical outputs do not intersect the directories
that failed manifest may have written. The failed manifest stays
discovered. Its own tree is not cleaned. A claim that cannot be named
still blocks the boundary.

This is the change that makes a learned root such as
`/Users/davidbowman/projects` usable. C047 only separates that root
from the other learned roots. Inside it, today's `MergeAll` rule still
holds until this milestone.

## Non-goals

- Not starting before ADR 010 is accepted. If the ADR is rejected,
  this plan is superseded and the `MergeAll` boundary block stands.
- Not changing which directories are learned, and not retiring missing
  learned roots. `~/projects` may remain one learned root. ADR 005
  stays the retirement proposal.
- Not reviving `--partitioned`, the M016 island report, or a second
  partition of the discovery universe.
- Not cleaning the unresolved workspace, by `cargo clean` or by
  deleting its `target` directly.
- Not treating sibling source directories as disjoint by themselves.
- Not a change to `RefusedByCargo` passengers and scope notes.
- Not a release. Acceptance of the ADR is not publication. The
  behavior change requires a version bump when it ships, and that
  release stays owner-gated.

## Current behavior

`attribute_unknown` returns `MergeAll` for every participant whose
capability is not `RefusedByCargo` (`src/workspace.rs:463-468`).
`split_unresolved_for_boundary` then counts those participants, and
`blocking_unresolved > 0` returns a scope block before any spawn
(`src/cleanup.rs:1923-1937`, `src/cleanup.rs:2960-2983`). A metadata
failure is `UnknownCapability::Unproven` even when locate already
named the workspace root (`src/workspace.rs:1133-1149`).

The 2026-11-09 `cargo cleanme --full` run stopped on 324 such
manifests. Two reproduced causes:

- `.../s001-frozen-fixture/Cargo.toml` — workspace member
  `catalog-format` has no target (`no targets specified in the
  manifest`). Locate can name the fixture directory.
- `/Users/davidbowman/projects/rustwaf2/Cargo.toml` — path dependency
  `cloakrs` points at a missing
  `/Users/davidbowman/projects/cloak/Cargo.toml`. Locate can name
  `rustwaf2`.

Neither failure, by itself, names another workspace's `target`. The
rule still has to prove that, per claim, or block.

## Invariants

1. The unresolved manifest remains in discovery, grouping, and the
   pre-spawn revalidation set. Removing it is the ADR 006 defect.
2. A unit is one workspace and one `cargo clean` of its complete
   `OutputSet`. A claim intersection holds the whole unit.
3. Authorization never manufactures proof. A claim intersection is not
   overridden by `allowed_output_roots`.
4. An unbounded claim is today's boundary block, not a skip of the
   one manifest.
5. Canonical paths decide intersection. A symlink to another `target`
   intersects.
6. The claim uses the environment the spawn would use, and is checked
   again before each spawn. A change blocks the units not yet spawned.
7. C047's cross-root residual, if C047 has shipped, is narrowed by
   this plan only where the claim is finite: a named redirect across
   learned roots holds the unit it touches. An unnamed redirect still
   blocks the boundary that contains the unknown.

## Contract

The contract is ADR 010's proposed decision, items 1–9. This plan
does not add a rule the ADR does not state. In particular:

- one claim per located workspace root;
- unread, unresolvable, or unaccounted config means the claim is
  unbounded;
- `skipped_output_claim` is the only new reason code;
- exit 1 when any unit was held or any boundary blocked;
- the rule applies to the Full boundary, to an explicit root, and to
  the maintenance anchor.

## Work packages

Do not start package 1 until the ADR 010 acceptance block exists in
ADR 007.

1. **Hostile fixtures first, all red.** The six cases in ADR 010's
   "Hostile cases" section, each asserting the safe outcome against
   the current binary: the redirect holds the healthy unit with zero
   spawns; the unreadable config blocks the boundary with zero
   spawns; the symlink holds the healthy unit; a `target` inside
   another source tree is not `PrivateBounded` because of this plan;
   a late `CARGO_TARGET_DIR` spawns nothing further; the disjoint
   conventional pair currently blocks with zero spawns. The disjoint
   pair is the only fixture whose expected spawn count changes, and
   it changes in package 3, after the red run is recorded.
2. **Claim construction.** For each unresolved workspace whose root
   locate named, compute the claim from the sources ADR 010 item 5
   lists, or mark it unbounded. Group manifests by that root before
   counting. The reader may be a small config walk or another
   mechanism; a source it does not implement forces unbounded, and a
   test must show that skip. Do not shell out to a Cargo command that
   itself fails closed by loading the broken manifest — `cargo
   metadata` does not print `target_directory` when the manifest
   fails to load.
3. **Spawn decision.** A unit whose physical outputs miss every finite
   claim, and whose boundary contains no unbounded claim, reaches the
   existing gates. Intersection holds the unit with
   `skipped_output_claim`. The unresolved workspace contributes no
   unit. Rewrite the disjoint-pair fixture to one `cargo clean` spawn
   and zero spawns for the failed workspace. The redirect fixtures
   stay at zero spawns.
4. **Re-check.** Before each spawn, recompute the claim against the
   current environment and config. A difference blocks the remainder.
   This hooks the existing per-dispatch environment check; it does
   not add a second policy.
5. **Selections.** The same decision for Full (per learned root once
   C047 has landed; per the union of proven roots until then), for
   explicit `ROOT`, and for the maintenance anchor. A test pins all
   three with the same poison.
6. **Docs.** `docs/TROUBLESHOOTING.md` and `docs/USAGE.md` describe
   the claim, the unbounded block, and the new reason. Architecture
   `09-cleanup.md` and `07-workspace.md` are re-derived from the
   source after the edit. The cross-root sentence C047 added is
   updated to say a named redirect holds the intersecting unit.

## Failure semantics

- Unreadable config, unresolvable path, unknown workspace root:
  unbounded claim, boundary block, zero spawns in that boundary.
- Config that appears or changes before a later spawn: that spawn and
  the ones after it do not run. The report says the environment
  changed, using the existing revalidation wording where it already
  matches, and the claim reason where it does not.
- A claim path that is not a directory is still a claim. Absence of
  the directory is not evidence the failed workspace never wrote
  there; another workspace's output at that path is the intersection.
- Parallel metadata resolution does not weaken the claim. The claim
  is computed from the coverage result after resolution finishes, not
  from a root that lost the race.

## Compatibility

No flag and no config key. Learned-state schema unchanged. One
additive reason code, no envelope bump. Exit 0 is no longer the
result of a run that held a unit or blocked a boundary; those runs
are exit 1 even when other units were cleaned. Human output lists
held units by the typed reason. Log format carries a count and no
path.

Explicit roots and the maintenance anchor change with this plan. That
is decision item 3 of the ADR. A test that still expects
`cargo cleanme ~/projects` to block on a disjoint broken sibling is
pinning the defect and is rewritten in package 3, after it has been
shown red.

## Verification

The ladder in `AGENTS.md` §3. The hostile fixtures run under
`cargo test --all-features --lib` and, for the subprocess spellings,
`cargo test --all-features --test cli_contract`. Each hostile case
must have been red on the baseline for the safe assertion, not for an
unrelated usage error. Hosted Linux, macOS, and Windows before
closure. MSRV `cargo +1.89 test --locked --all-targets`, because the
claim sources are pinned to Cargo 1.89: a newer host Cargo that reads
an extra config key must not make the tests green while the 1.89
claim reader ignores it. If 1.89 and current stable disagree on a
config source, the claim is unbounded on both until the reader
accounts for the wider set.

## Acceptance criteria

1. ADR 010 is accepted and ADR 007 item 4 carries the amendment block
   before any of this plan's production code merges.
2. Each hostile case in the ADR fails on the baseline in the safe
   direction and passes after the change, with spawn counts asserted
   rather than "the command failed".
3. The disjoint conventional pair performs one `cargo clean` of the
   healthy workspace and none of the failed one. The failed manifest
   is still present in the discovery set used for grouping.
4. A redirect, a symlink redirect, and an unreadable config perform
   zero spawns of the healthy unit.
5. A late `CARGO_TARGET_DIR` performs zero further spawns.
6. The same poison under `--full`, under an explicit root, and under
   the maintenance anchor has the same hold-or-clean result.
7. `RefusedByCargo` notes still do not block and do not create a
   claim.
8. JSON gains `skipped_output_claim` and no other field. A shared
   output is still not cleaned.
9. Docs state that an unnamed output blocks the boundary, and that a
   named one holds only the units it touches.

## Stop conditions

- Stop if ADR 010 is not accepted. Do not implement behind a flag to
  make progress.
- Stop if an unresolved manifest disappears from the discovery set,
  or if a held unit's output is reclassified private.
- Stop if the disjoint-directory fixture is the only hostile case.
  The redirect cases are the ones that justify the old block.
- Stop if the reader skips an unreadable config and still produces a
  finite claim.
- Stop if the failed workspace's `target` is deleted by any means.
- Stop if learned-root selection changes in the same change. That is
  a different plan.
- Stop if C047's per-root block is "fixed" by deleting it. Finite
  claims narrow a block; they do not restore the union boundary.

## Closure evidence

`plans/closure/artifact-discovery-cleanup/m022-status.md` after
implementation. The matrix maps each ADR hostile case to the test
that was red and then green, and records the commands actually run,
including the MSRV lane. The milestone stays open until a published
release carries it. This plan does not authorize that release.
