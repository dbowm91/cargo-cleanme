# ADR 010 (proposed) — A Bounded Output Claim for an Unresolved Manifest

Status: **accepted 2026-11-09**, owner-directed by the instruction to
complete M022. The acceptance is the amendment block in ADR 007 item 4
dated 2026-11-09. This file did not accept itself. Acceptance is not
publication and authorizes no release.
Date: 2026-11-09.
Baseline: `5ad76cf` (published v0.2.13).
Related: ADR 001 C004/C006, ADR 004 (accepted, then superseded as the
default by ADR 007), ADR 007 item 4, ADR 008 §5, C047, M015, M016.
Implementing plan, blocked on this ADR:
`plans/implementation/artifact-discovery-cleanup/m022-bounded-output-claim-cleanup.md`.

## Problem

C047, once implemented, makes `cargo cleanme --full` clean a proven
learned root that contains no blocking unknown. It does not make
`/Users/davidbowman/projects` cleanable. That directory is one learned
root: learning stores the parent of a workspace when the parent is not
the home directory and not a broad root such as `/Users`
(`src/discovery_state.rs:286-289`, `src/discovery_state.rs:417-442`).
Every crate directly under `~/projects` is one boundary.

On 2026-11-09, `cargo cleanme --full` discovered 324 manifests `cargo
metadata` could not load and ran no `cargo clean`. The pasted causes
are ordinary broken trees, not an unreadable filesystem: a frozen
fixture member with no target, and a workspace whose path dependency
points at a missing manifest. `cargo locate-project` can still name
those workspace roots. The metadata failure is classified `Unproven`,
and `attribute_unknown` maps every `Unproven` participant to `MergeAll`
(`src/workspace.rs:463-468`). `MergeAll` blocks the whole boundary
(ADR 007 item 4).

That classification is right when the failed manifest's output
directory is unknown. Cargo honors `CARGO_TARGET_DIR` and
`build.target-dir`, so two source trees in different directories can
share one `target`. It is the wrong tool when the output directory can
be named and does not intersect a healthy workspace's output. The
failed workspace is not itself a cleanup candidate either way: it did
not resolve, so there is no `OutputSet` to hand to `cargo clean`.

ADR 004 designed a proof-island partition for this and then authorized
it only behind `--partitioned`. ADR 007 made scoped proof the default
and removed the flag before publication. Item 4 still blocks the
boundary on any in-boundary `MergeAll`. This ADR proposes a narrower
amendment to that item. It does not revive the flag.

## Proposed decision (requires acceptance)

Amend ADR 007 item 4 as follows. The rest of ADR 007 is unchanged,
including "discovery and grouping stay whole-system" and the
refused-manifest passenger rule.

1. **A finite claim does not block the boundary.** An unresolved
   manifest whose output locations can be named is a **claim**: a
   finite set of directories it may have written. A cleanup unit
   proceeds only when none of its physical output paths intersect that
   set. The unit still passes every existing ownership, authorization,
   and per-spawn gate. The unresolved manifest stays in the discovery
   universe and is not a candidate.
2. **An unbounded claim blocks the current boundary.** If the claim
   cannot be named, the participant stays `MergeAll` and blocks the
   boundary exactly as item 4 does today. After C047, the Full boundary
   is the learned root that contains the manifest. For an explicit root
   and for the maintenance anchor, the boundary is that scope. This
   ADR does not depend on C047. Without C047, an unbounded claim still
   blocks every proven learned root at once, which is today's Full
   behavior and is the fail-closed direction.
3. **The same rule on every boundary.** A broken tree under
   `~/projects` blocks `cargo cleanme ~/projects` and `cargo cleanme
   --full` alike, except for units whose outputs miss the claim. A
   rule that freed space only on `--full` would make the wider spelling
   the looser one.
4. **One claim per located workspace.** Manifests that share one failed
   `cargo metadata` root are one claim, not one block each. A manifest
   that was never located, or whose workspace root cannot be named, is
   an unbounded claim.
5. **What "named" means.** The claim is the output directories Cargo
   1.89 would use for that workspace in the environment the cleanup
   spawn would use:
   - `CARGO_TARGET_DIR`, when set;
   - `build.target-dir` from the Cargo config Cargo would actually
     read for that workspace (the workspace `.cargo/config.toml`, its
     ancestors, and the Cargo home config);
   - the `build-dir` source Cargo 1.89 honors, under the same rule;
   - otherwise `<workspace root>/target`.
   Paths are canonicalized before the intersection test. A relative
   config value is resolved from the config file that set it, which is
   Cargo's rule. A symlink is followed for the comparison and is not
   treated as a different directory.
6. **Unread means unbounded.** Any of the following makes the claim
   unbounded, and the boundary blocks: the workspace root is not known;
   a config file on the chain Cargo would read cannot be read; a
   `target-dir` or `build-dir` value cannot be resolved to a
   directory; the implementation does not account for a source Cargo
   1.89 uses. A parser that silently skips a config file is a defect,
   not a default.
7. **The claim is re-checked at the spawn.** The existing environment
   re-verification already runs per dispatch. If `CARGO_TARGET_DIR` or
   the config chain changes after the claim was computed, the units
   not yet spawned block. A unit already spawned is not rolled back by
   this ADR; the existing last-moment proof is what protects it.
8. **Passengers and notes are unchanged.** `RefusedByCargo` attributed
   as `PassengerOf` or `ScopeNote` still does not block and does not
   grow a claim.
9. **Reporting.** A unit held because its output intersects a claim is
   `skipped_output_claim`. The detail names the workspace that did not
   resolve and the output path that intersected. An unbounded claim
   keeps the boundary block C047 or today's early return already
   reports. Exit 1 when any unit was held or any boundary blocked.
   Exit 0 only when every candidate in the requested boundary
   proceeded and succeeded. JSON gains the one additive reason code
   and no other field.

### What this acceptance would not do

- It would not authorize dropping an unresolved manifest from
  discovery, grouping, or the final revalidation universe.
- It would not authorize treating two source trees as disjoint because
  their directories do not nest. That alternative is rejected below.
- It would not authorize deleting a failed workspace's `target` by
  `remove_dir_all`, or cleaning that workspace at all. There is no
  resolved `OutputSet`.
- It would not change which directories are learned, and it would not
  retire missing learned roots.
- It would not authorize a release. M022 cannot start until this ADR
  is accepted, and acceptance is not publication.

## Soundness

The bytes at risk are the directories the failed workspace may have
written. Cargo's output location for a workspace is a function of that
workspace's root and of config and environment, not of which sibling
directory the source sits in. Naming those directories, and refusing
to proceed when they cannot be named, is the same standard ADR 004
required of an independence certificate, applied to one fact: the
output root. It is not a certificate that the failed manifest owns no
bytes anywhere. Those bytes, when named, stay on disk. When they
intersect a unit, that unit is held. When they cannot be named, the
boundary is held.

Resolved workspaces still contribute their real output paths to
grouping. A `target` shared between two resolved workspaces stays
`Shared` even if neither path appears in an unresolved claim.

## Hostile cases the implementation must fail

These are acceptance conditions of M022. Recording them here so a
later edit of the plan cannot drop them:

- A failed manifest whose `CARGO_TARGET_DIR`, or whose
  `.cargo/config.toml` `build.target-dir`, names a healthy workspace's
  `target`. The healthy unit is held. Zero `cargo clean` spawns for
  it.
- A config file on the ancestor chain that cannot be read. The
  boundary blocks. Zero spawns.
- A `target-dir` that is a symlink to another workspace's `target`.
  The comparison is on the canonical path. The healthy unit is held.
- A `target` directory that lives inside another workspace's source
  tree. Existing ownership classification still applies; the claim
  does not upgrade it to `PrivateBounded`.
- `CARGO_TARGET_DIR` set between claim computation and the spawn.
  Units not yet spawned do not spawn.
- Source trees in sibling directories, no redirect, conventional
  `target` directories, disjoint. The healthy workspace is cleaned
  once. The failed workspace is not cleaned. This is the case the
  2026-11-09 run could not reach.

## Rejected alternatives

- **Ignore metadata errors and clean every resolved workspace.**
  Rejected. A failed manifest can point its output at another
  workspace. The 2026-11-09 fixtures happen not to; the rule cannot
  special-case them.
- **Treat learned roots, or source directories, as isolated.**
  Rejected by ADR 004, and still rejected here. Isolation is a
  consequence of the claim sets being disjoint, not a premise.
- **Learn each workspace instead of its parent, so `~/projects` stops
  being one root.** Rejected as this decision. It would change which
  roots bare `cargo cleanme` maintains. The cleanup rule does not need
  it. A later plan can propose it on its own.
- **Revive `--partitioned`.** Rejected. ADR 007 removed it before
  publication. The amendment is the default inside the boundary, or
  it is not accepted.
- **Clean the failed workspace's conventional `target` anyway.**
  Rejected. That directory was not proved exclusive, and this tool
  does not delete except through `cargo clean` of a resolved unit.

## Acceptance

This file does not accept itself. Acceptance is an owner decision,
recorded as an amendment block in ADR 007 item 4 that names ADR 010
and the date, in the same shape as the ADR 008 amendment. That block
was written on 2026-11-09. Until it existed, M022 stayed blocked and
no cleanup path was authorized to implement the claim rule.
