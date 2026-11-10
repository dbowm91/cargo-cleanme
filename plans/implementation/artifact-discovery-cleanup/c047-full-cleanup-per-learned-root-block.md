# C047 — A Full-cleanup unknown blocks its learned root

Status: **closing**. Implemented in the working tree on 2026-11-09.
Not published, so the plan stays open. The contract is the accepted ADR
007 amendment of 2026-10-21 (ADR 008). No new decision was required.
Date: 2026-11-09.
Repository baseline: `main` at `5ad76cf` (published v0.2.13).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: **M018** (`plans/implementation/artifact-discovery-cleanup/m018-proof-scoped-full-cleanup-gate.md`;
evidence `plans/closure/artifact-discovery-cleanup/m018-status.md`).
Authority: ADR 007 amendment 2026-10-21, item recorded in
`plans/adr/007-accepted-scoped-proof-boundary.md`, and ADR 008 §5
(`plans/adr/008-accepted-canonical-full-cleanup-spelling-and-proof-scoped-full-gate.md`).
Related: ADR 001 C004/C006, ADR 006, M017, M022 (closing; not a dependency
of this plan).

## Defect

`cargo cleanme --full` still treats every **proven** learned root as one
proof boundary. An unresolved manifest inside one proven root blocks
`cargo clean` for every other proven root.

ADR 007's amendment, effective when M018 shipped, says the Full proof
boundary is the learned root and that an unknown blocks the root being
cleaned. M018 implemented the half of that sentence about a root whose
**traversal** did not complete: `CleanupSelection::ExcludeRoots` removes
that root from the candidate set and from `boundary_admits`. The other
half was left as `CleanupSelection::All`. For `All`, `boundary_admits`
returns true for every manifest that is not under an excluded root
(`src/cleanup.rs:2913-2943`, baseline `5ad76cf`). One in-boundary
`MergeAll` participant then raises `ScopeBlockReason::OwnershipUnproven`
and returns before any spawn (`src/cleanup.rs:1923-1937`). A discovery
diagnostic attributed to any proven root does the same
(`src/cleanup.rs:1910-1920`, `src/cleanup.rs:3011-3036`).

`--full` selects `CleanupSelection::All` (`src/cli.rs:693-697`) and only
then excludes unproven roots (`src/main.rs:685-688`).

## Why previous verification missed it

M018's fixtures are one proven root beside one **unproven** root. They
prove that an unreadable unproven root stays in the discovery set, leaves
the candidate set, and does not block its proven sibling. They never
place a `MergeAll` unknown inside a second root whose traversal
**completed**. The early return on `blocking_unresolved > 0` is therefore
green while it still means "any proven root". The live run below is that
hole.

## Live evidence (2026-11-09)

`cargo cleanme --full` on this machine, after a platform walk that was
not complete:

- discovery state was not published; the in-memory generation kept the
  learned roots from the last complete reconciliation (2026-10-07,
  `~/Library/Application Support/cargo-cleanme/discovery-state.json`,
  30 learned roots);
- 10 of those roots are not directories and were skipped with
  `scan root is unavailable`;
- 20 roots are still directories, including `/Users/davidbowman/projects`
  and separate roots such as `/Users/davidbowman/helix`,
  `/Users/davidbowman/bandwhich`, `/Users/davidbowman/miniserve`, and
  `/Users/davidbowman/i2p-rs`;
- the run then stopped with `cleanup ownership could not be proven: 324
  discovered Cargo manifests did not resolve; no cleanup commands were
  run`.

The 324 are discovered manifests, not 324 workspaces. A failed `cargo
metadata` is re-accounted for every manifest that shares that failure
(`src/workspace.rs:1133-1149`). Two of the pasted failures are a frozen
fixture whose member manifest has no target, and `rustwaf2`, whose path
dependency `cloakrs` points at a missing `Cargo.toml`. Both sit under
`/Users/davidbowman/projects`. That root's failure also vetoed the other
proven roots.

`attribute_unknown` maps every participant that is not
`RefusedByCargo` to `MergeAll` (`src/workspace.rs:463-468`,
`src/domain.rs:190-198`). This plan does not change that mapping.
M022 is the plan that would, and it has no authority until ADR 010 is
accepted.

## Objective

On `cargo cleanme --full`, a blocking unknown is attributed to the proven
learned roots that contain it, and only those roots lose their cleanup
candidates. Every other proven root is still cleaned through `cargo
clean`, under the same per-unit proof the engine already runs.

## Non-goals

- Not a change to what `MergeAll` means inside the root it belongs to.
  One blocking unknown inside `/Users/davidbowman/projects` still blocks
  every candidate under that root. Splitting that root by output claim
  is M022 / ADR 010 and is not authorized here.
- Not a change to which directories are learned. A workspace at
  `~/projects/foo` is still learned as `~/projects` when that parent is
  not a broad root (`src/discovery_state.rs:286-289`,
  `src/discovery_state.rs:417-442`).
- Not retirement of a learned root that is no longer a directory. Those
  roots are already skipped. Expiring them on an incomplete walk is
  ADR 005, still proposed, and is not this plan.
- Not a change to bare `cargo cleanme`, `clean --known`, or an explicit
  `cargo cleanme ROOT`. Those boundaries stay one scope. Poison inside
  an explicit root still blocks that root.
- Not a change to discovery, grouping, or classification. The blocked
  root's manifests stay in the universe. A `target` shared with a root
  that does proceed stays `Shared` and is not deleted.
- Not a new flag, config key, or env var. Not `--partitioned`. That
  flag was removed before publication.
- Not a change to `RefusedByCargo` passengers and scope notes. They
  still do not block.
- Not a release. Publication of the behavior change is a version bump
  and stays owner-gated.

## Invariants

1. Nothing is deleted without exclusive ownership of its physical bytes,
   re-checked at the spawn.
2. Deletion is one `cargo clean` of one workspace's complete `OutputSet`.
3. The discovery universe does not shrink. Withholding a blocked root's
   manifests is the ADR 006 defect.
4. `allowed_output_roots` does not manufacture proof.
5. An unattributable discovery diagnostic — one that belongs to no
   root's walk — still blocks the whole run.
6. A root contained by a blocked root is blocked too. Retained state can
   hold both a parent and a child (`IncompleteRetain` does not narrow).
   Containment in any blocked root wins.
7. The cross-root residual ADR 007 already accepts for an out-of-boundary
   unknown applies between proven learned roots: an unresolved manifest
   in root A can still have configured its output at a path inside root
   B, and this plan does not disprove that. Resolved cross-root claims
   still force `Shared`. The closure record and the troubleshooting doc
   say this in those words. M022 is the plan that binds a finite claim.

## Contract

- **Attribution.** A blocking unresolved participant is attributed to
  every proven learned root that contains its manifest. A discovery
  diagnostic with a path is attributed the same way. A pathless
  discovery diagnostic is attributed to the root whose walk produced
  it, which is the attribution M018 already records. A diagnostic
  attributed to no root blocks the run.
- **Blocked root.** A proven root with at least one such attribution is
  blocked. Its candidate units are reported and not spawned. It stays
  a discovery root.
- **Proceeding root.** A proven root with no such attribution proceeds
  through the existing unit gates. Policy skips, shared outputs, and
  per-spawn revalidation are unchanged.
- **Reason code.** Additive `skipped_root_blocked`. The detail names
  the learned root, whether the cause was an unresolved manifest or a
  discovery diagnostic, and the count. It does not reuse
  `skipped_incomplete_coverage`, which means the root's own traversal
  did not complete.
- **Whole-run block.** If every candidate is removed by this rule, or
  an unattributable diagnostic remains, the run is the existing
  scope block (`OwnershipUnproven` or `IncompleteDiscovery`), exit 1,
  zero `cargo clean` spawns. It does not exit 0 with zero rows.
- **Exit when some roots proceed.** Exit 1. A partial Full cleanup
  must not read as a complete one. Exit 0 only when every proven root
  proceeded and the usual per-unit result is success. Unproven roots
  already force exit 1; that is unchanged.
- **JSON.** One additive `reason_code` value. No field added or
  removed. No envelope version bump. `unresolved_ownership` still
  lists the blocking participants. Out-of-boundary participants stay
  `out_of_scope` diagnostics.
- **Human.** The blocked roots are rows with the typed reason. The
  unresolved listing is not withheld. Log format carries the count in
  its one line and does not gain a path.

## Work packages

1. **Failing fixture first.** Two proven learned roots under a
   disposable home. Root A contains a manifest `cargo metadata` cannot
   load. Root B is a workspace with a private `target` and real build
   output. Assert the current failure: zero `cargo clean` spawns. This
   assertion is rewritten only as part of package 3, so the suite cannot
   go green on the old block.
2. **Per-root disposition.** Replace the single early return, for the
   Full selection only, with the attribution in the contract. Explicit
   and maintenance selections keep today's early return. The mechanic
   is the plan's freedom: a per-root block set consulted by the
   existing selection, or an equivalent split before spawn. It must
   not be a second discovery walk that omits the blocked root.
3. **Rewrite the fixture.** Root B is cleaned once. Root A's unit is
   `skipped_root_blocked`. The spawn count for A is zero. A second
   assertion deletes the private `target` check and fails if B is
   cleaned when its output is `Shared` with a workspace under A.
4. **The other edges.** Unattributable diagnostic blocks the run.
   Pathless diagnostic on root A's walk blocks A and not B. Overlapping
   retained roots: a manifest inside both blocks both. Every proven
   root blocked: scope block, zero spawns, exit 1. Explicit
   `cargo cleanme ROOT` containing the same poison: still one block,
   zero spawns. A `RefusedByCargo` note inside A does not block A.
5. **Docs.** `docs/TROUBLESHOOTING.md` states that a Full unknown blocks
   the learned root that contains it, names the cross-root residual,
   and says an explicit root is still one boundary. The architecture
   deep dive that describes `boundary_admits` (`architecture/09-cleanup.md`)
   is updated from the source after the edit, not by shifting old line
   numbers.

## Failure and contention

- A root that disappears between admission and the per-root check is
  the existing admission-premise failure: the run blocks, and no later
  unit spawns.
- The blocked set is fixed before the first spawn. A manifest created
  later is the existing hoisted-universe refresh: it blocks the units
  that have not spawned, it does not authorize one that already ran.
- Canonical path comparison, both directions, as `path_is_within` does
  today. A spelling that does not canonicalize must not leave a unit
  inside a blocked root eligible.

## Compatibility

No flag, config key, or learned-state schema change. One additive
reason code. Exit 0 becomes unreachable for a Full run that blocked
one proven root and cleaned another; that run is exit 1. Scripts that
treated exit 1 as "nothing was cleaned" were already wrong for
`skipped_incomplete_coverage`. The human and JSON rows are how they
tell a cleaned root from a blocked one.

## Verification

The ladder in `AGENTS.md` §3, in order, stopping at the first failure.
The focused tests are the fixture in work package 1–4, run as
`cargo test --all-features --lib <name>` and, where the case is a
subprocess, `cargo test --all-features --test cli_contract <name>`.
Hosted Linux, macOS, and Windows are required before closure: the
containment check is the one macOS symlinked temp roots have bitten
before. MSRV `cargo +1.89 test --locked --all-targets` is required
because this plan changes no Cargo-version behavior and must not
acquire one.

## Acceptance criteria

1. The package-1 fixture fails on the baseline with zero `cargo clean`
   spawns, and passes after the change with exactly one spawn, for
   root B.
2. Root A's candidate is `skipped_root_blocked` and was still
   discovered and classified.
3. A shared output between A and B is not cleaned.
4. An unattributable discovery diagnostic still produces a scope block
   and zero spawns.
5. Explicit `ROOT` and bare maintenance keep today's whole-boundary
   block. A test fails if they start proceeding.
6. `RefusedByCargo` inside an otherwise healthy proven root does not
   block that root.
7. Exit is 1 when any proven root was blocked, including when other
   roots were cleaned. Exit is 1 with a scope block and zero spawns
   when every proven root was blocked.
8. JSON gains the one reason code and no other field. Log format gains
   no path.
9. The troubleshooting doc states the cross-root residual.

## Stop conditions

- Stop if the implementation omits a blocked root from discovery, or
  reclassifies an output by doing so.
- Stop if a proceeding root is cleaned on the strength of source-tree
  disjointness from the unknown. The unit still passes the existing
  ownership gates; this plan only chooses which units reach them.
- Stop if the change applies to an explicit root or to the maintenance
  anchor. That is a different boundary decision.
- Stop if a new flag appears. The behavior is the Full boundary the
  ADR already states.
- Stop if the claim-set rule from M022 is pulled in to "finish the
  job". That rule is not accepted.

## Closure evidence

`plans/closure/artifact-discovery-cleanup/c047-status.md` after
implementation, with the requirement-to-evidence matrix, the commands
actually run, the hosted lanes, and the residual stated as a known
limitation rather than as a fixed case. The milestone stays open until
a published release carries it. This plan does not authorize that
release.
