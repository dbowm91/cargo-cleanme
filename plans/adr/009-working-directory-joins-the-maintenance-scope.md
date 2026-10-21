# ADR 009 (accepted) — The Working Directory Joins the Maintenance Scope

Status: **accepted 2026-10-20** — owner-directed. Amends ADR 003 §1 and ADR 006
§1/§3; does not amend ADR 007. The safety boundary is unchanged: ownership is
still proved over a universe that may only ever grow.

Related: ADR 003 §1/§4 (canonical invocation), ADR 006 (working-directory
anchored selection), ADR 007 §1/§3 (scoped proof boundary), ADR 008 §4 (root
flag placement — still open), M014, M017, C028, C032, C033, C039.

Scope: the **scope** the maintenance cleanup spelling selects. Not the
selection filter, not the ownership proof, not the deletion mechanism, not the
Full gate.

## Context

ADR 006 decided that bare invocation *anchors* — it discovers as broadly as
before and filters the candidates to the working directory. The intent was
recorded in its own Consequences:

> Bare `cargo cleanme` becomes predictable: it cleans what is here.

That intent is not met outside the Routine scope, and the reason is
structural. The anchor is a **post-classification filter**, and the
classification universe is the resolved roots:

```rust
// src/cleanup.rs:1216
let resolved = crate::domain::EffectiveScanPolicy {
    scope: ScanScope::ExplicitRoots(roots.clone()),
    discovery_filters: crate::domain::DiscoveryFilters::Bypassed,
};
```

The resolved roots for the maintenance spelling are the seed roots
(`src/policy.rs:60` — thirteen fixed names under `$HOME`) plus the age-eligible
learned roots (`src/policy.rs:475`). **The working directory is not among
them.** An anchor can only filter what was discovered, so when the working
directory lies outside the Routine scope, the anchor filters nothing into
existence and the run proves nothing.

Measured on the operator's machine on 2026-10-20, from `~/.codex`:

| Spelling | Discovered | Would clean |
|---|---:|---|
| `cargo cleanme --dry-run` | 335 manifests, 97 workspaces, scope `~/m013-fixture` + `~/projects` | **0** |
| `cargo cleanme --dry-run clean ~/.codex` | 303 manifests, 128 workspaces, scope `~/.codex` | **4 units, 107.15 GiB** |

The same directory is, by spelling, both "nothing" and "107 GiB". The
difference is not the operator's intent — both spellings were run from
`~/.codex` — but whether the directory happened to be a seed or learned root.

Two further facts from the same machine shaped the decision.

**The learned state already holds the answer, and the tool refuses it.** The
live state carries 17 learned roots, 14 of which sit at locations
`automatic_root_is_maintainable` forbids by name (`src/discovery_state.rs:114`):
eight under `~/.codex/worktrees` (line 126), `~/.local/share/Trash`, two
`node_modules` subtrees, `~/go/pkg/mod`, and `/tmp`. Those roots are excluded
*as roots*. The workspaces under them are excluded from the automatic scope
entirely, so their build output is never reclaimed.

**The forbidden check is a path-prefix test, so a parent root escapes it.**
The comparison at `src/discovery_state.rs:192` asks whether the candidate is
*under* a forbidden entry. `~/.codex` is not under `~/.codex/worktrees`, so
`~/.codex` is maintainable and a root at `~/.codex` carries the worktrees in
with it. The exclusion is therefore about the *root*, never about the subtree —
and the operator standing in `~/.codex` is not "an automatic root" at all.

ADR 006 §Context measured the alternative and rejected it: with two
workspaces where one redirects its `target-dir` into the other's, scoping
discovery to one of them classifies the shared bytes `private` and deletes
17.65 MiB the other workspace is using. Narrowing discovery promotes `shared`
to the only class that authorizes deletion.

## Decision

1. **The working directory joins the maintenance scope as an additional
   discovery root.** The scope of the maintenance spelling becomes the seed
   roots, the age-eligible learned roots, **and** the working directory's own
   scope roots.

   The working directory joins only when the Routine scope does not already
   reach it — that is, when no routine root contains the working directory and
   the working directory contains no routine root. The excluded cases are the
   bounded machine-wide sweep:

   - the working directory is *inside* a routine root: the Routine scope
     already discovers it, so adding it is redundant;
   - the working directory *contains* a routine root: adding it would widen a
     bounded sweep of the known roots into a sweep of everything under the
     operator's directory.

   The second exclusion is not a matter of taste; it was measured. On the
   operator's machine, a bare run from the home directory with the working
   directory added unconditionally swept **264,183 directories** and
   **2,421 manifests** instead of the bounded scope's handful, took 65 seconds
   instead of 9, and **ended blocked** — 1,357 manifests inside
   `~/.local/share/Trash` could not be resolved, they sat inside the ADR 007
   proof boundary, and the run exited `1` without cleaning anything. The
   Trash is one of the origins the maintainability rule already refuses as a
   working set; a sweep of the home directory drags it in as an in-boundary
   unknown. `clean ROOT` is the spelling for that wider sweep, and it has
   always carried the same hazard.

2. **The discovery universe for the maintenance spelling may only ever grow.**
   This is the load-bearing invariant and it is machine-checked. Adding the
   working directory can only add participants, so it can only reclassify
   `private` as `shared` — never the reverse. The failure ADR 006 measured is
   therefore structurally unreachable on this path.

   The universe is never narrowed *either*: the working directory's exclusion
   above removes a candidate that the pre-ADR-009 scope never had, so the
   resulting scope is still a superset of the old one.

3. **The scope roots the working directory contributes are the enclosing
   workspace and the working directory itself.** The enclosing workspace is
   Cargo's own answer, obtained with `cargo locate-project --workspace` run in
   the working directory through the existing `CargoRunner` seam
   (`src/workspace.rs:29`, the same invocation shape as
   `src/workspace.rs:548`). It is needed because a member directory's
   manifest sits *above* the member, so a root at the member cannot discover
   it; `CleanupSelection::Ancestry` (`src/cleanup.rs:101`) already admits that
   workspace in both directions, which is ADR 006 §3's bidirectional rule.

4. **The anchor is unchanged.** Selection for the maintenance spelling
   remains `Ancestry(working directory)`, applied after classification
   (`src/cleanup.rs:1449` decides the ADR 007 boundary with the same
   selection). ADR 006 §1's "the working directory anchors selection, not
   discovery" is amended only in that discovery now *includes* the working
   directory; the anchor still cannot add, reclassify, or unblock anything.

5. **The working directory's roots carry operator provenance.** The exemption
   is from `automatic_root_is_maintainable` only: standing in a location is an
   explicit act, `clean ROOT` already permits the same subtree, and the rule
   exists to stop the tool from *choosing* a transient origin on its own.
   Defect handling is otherwise identical to an automatic root: a symlinked
   root blocks the scope, an absent or non-directory root is omitted.

   The configured `scan.ignore` and `scan.unignore` filters are a separate
   matter, and the record must be exact about them. They apply to the
   **read-only scan path** and have **never** applied to a cleanup: every
   cleanup scope runs with the discovery filters bypassed, because ADR 007 §1
   makes an explicit root authoritative — "`clean ROOT` proves exactly those
   roots". So the working directory's roots bypass nothing new on the cleanup
   path, and on the scan path the filters still prune the subtree the
   operator is standing in. C040 adds no filter behaviour.

6. **`clean ROOT` and `clean --full` are unchanged.** They already bound
   their own scope and need no working directory. ADR 007 §1 continues to
   apply to them unchanged.

7. **The machine contract's `scope` label is unchanged.** The maintenance
   spelling keeps reporting `scope: "routine"`; `selected_roots` carries the
   actual root set, so a consumer sees the change without a schema change.
   The read-only `scan --known` inventory gains the same working-directory
   roots so that it remains a true inventory of what the maintenance spelling
   would act on.

## Consequences

- `cargo cleanme` from any directory cleans the Cargo output it can find
  there. The verbosity complaint that produced this decision — reaching for
  `cargo cleanme clean /absolute/path` — disappears for the common case.
- The maintenance scope's roots now vary with the working directory, so the
  human surface's `combined roots` line and the JSON `selected_roots` array
  vary with it too. That is the observable, intended change.
- A run from a directory with no Cargo project underneath proves nothing and
  exits `0` with `Nothing to clean.`, exactly as it does today.
- The exclusion of `~/.codex/worktrees` and the other transient origins holds
  for roots the tool *chooses*. It no longer holds for a subtree the operator
  is standing in. This is a deliberate narrowing of a scope-hygiene rule, and
  it is the reason the rule's exemption is stated here rather than left to
  the implementation.
- The discovery universe grows, so a maintenance run from a large directory
  costs more to resolve than it did when that directory was outside the
  scope. The cost is bounded by the directory's own size.
- ADR 003 §8's scheduler recipe changes meaning: `cargo-cleanme --format log`
  was "daily Routine cleanup" and becomes "clean the scheduler's working
  directory". Schedulers must move to `cargo-cleanme clean --known --format
  log`. This is a published-behaviour change and needs a pre-1.0 minor
  boundary, as ADR 003 §9 required for the earlier bare-invocation change.

## Rejected alternatives

- **Narrow the maintenance scope to the working directory.** Rejected with
  ADR 006's measurement: it promotes `shared` bytes to `private` and deletes
  output another workspace is using.
- **Add a `--here` flag and leave bare alone.** Rejected: ADR 006 §Alternatives
  rejected the same shape for the same reason — it leaves the footgun in
  place as the default and adds a spelling.
- **Make the working directory the *only* root, with no seed or learned
  roots.** Rejected for the same reason as the first alternative, and it also
  removes the machine-wide Routine cleanup from every spelling.
- **Make the maintenance spelling identical to `clean <cwd>`.** Rejected:
  `clean ROOT` bypasses the configured ignore filters and treats a defect in
  the root as fatal, while the maintenance spelling's roots are automatic
  hints whose defects are omitted or blocked. Collapsing the two would change
  the configured-ignore behaviour of the default spelling as a side effect.
- **Apply the maintainability rule to the working directory's roots.**
  Rejected by owner decision on 2026-10-20, in favour of the operator-
  directed exemption. The rule's purpose is to stop the tool from *choosing*
  a transient origin; an operator standing in `~/.codex` has chosen it, and
  `clean ~/.codex` already reaches the same bytes.
- **Report the working directory as a distinct scope label.** Rejected: the
  scope *kind* is unchanged, and `selected_roots` already carries the actual
  root set. A new label would be a gratuitous machine-contract change.
- **Resolve the enclosing workspace by walking up for `Cargo.toml`.**
  Rejected: `src/workspace.rs` states that Cargo is authoritative for
  workspace configuration and that this module never reimplements Cargo's
  rules. A walk-up finds the nearest *package*, not the workspace root, and
  the difference is exactly the case that matters here.

## Evidence required before closure

1. A live run of the maintenance spelling from a directory **outside** the
   seed and learned roots that proves and reclaims a unit there — the
   `~/.codex` shape, or a fixture that reproduces it.
2. A machine-checked proof that the maintenance scope's roots are a
   **superset** of the pre-ADR-009 roots, so the universe cannot shrink.
3. A regression test that the enclosing workspace is still cleaned when the
   operator stands in a member directory.
4. A test that the operator-direct provenance admits a root the
   maintainability rule would refuse, and that the rule still refuses the
   same path as an *automatic* root.
5. The generated `man/` and `completions/` artifacts regenerated, and the
   drift gate green.
6. The `plans/adr/003` §1/§8 text and ADR 006 §1/§3 amended or explicitly
   superseded by reference to this record — not edited to imply the original
   decisions were never made.
