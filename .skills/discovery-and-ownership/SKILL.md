---
name: discovery-and-ownership
description: cargo-cleanme discovery, workspace resolution, physical grouping, and the learned-state Routine/Full split — including the mistakes that resolve zero workspaces
version: 1.0.0
tags:
  - discovery
  - workspace
  - grouping
  - state
---

# Discovery and Ownership

This is the path that decides *which paths cargo-cleanme is even allowed to
consider*. It never deletes anything, but every cleanup guarantee rests on it
being complete, and completeness is where this subsystem has failed twice.

Reference: `architecture/05-discovery.md`, `06-discovery-state.md`,
`07-workspace.md`, `04-policy-and-scope.md`.

## When to Load

- Editing `discovery.rs`, `discovery_state.rs`, `workspace.rs`, or `policy.rs`
- Changing how manifests are found, filtered, or attributed
- Changing the Routine/Full scope decision
- Anything touching learned state, or `scan.root`

## The resolution chain

```
scan root set
  -> discovery (bounded walk, bounded worker pool, attribution for every prune)
  -> workspace::resolve_workspaces (one `cargo metadata` per unique canonical root)
  -> workspace::build_groups (physical grouping)
  -> workspace::analyze_groups (measure + classify activity)
```

Two independent bounded walks cover the filesystem — one for discovery, one for
measurement — so a root can be discoverable yet unmeasurable, and the report
distinguishes those cases. Do not unify them to save a pass; the duplication is
what makes the attribution honest.

`cargo metadata` is the only binary dependency that matters for safety. It is
invoked through a `CargoRunner` trait precisely so tests can substitute a runner
that **fails**, which is how the fail-closed blocking behaviour is testable at
all.

## C015: the mistake to not repeat

A relative scan root resolved **zero** Cargo workspaces. The walk ran correctly;
the root it produced was relative; `cargo metadata` was then invoked against a
relative path, which resolves against the *child's* working directory, not the
parent's. The result was an empty workspace set and a clean-looking report.

The lesson is about `cli::absolutize_root_for_test` existing at all. An absolute
root must be produced **before** the boundary is crossed, and the test helper
exists because the production path was not testable that way. When adding a new
root source, add its absolutization and a test that would fail without it.

## Grouping: physical, not logical

`build_groups` groups by **physical output roots**, not by workspace membership.
Two rules make the byte union correct:

- **Nested and equal roots are deduplicated.** `<ws>/target` and
  `<ws>/target/debug/build` must produce one `PhysicalOutputGroup` containing
  both, and `dua_core::walk_roots` asserts that the **root indices are unique**
  (`dua-core-4.1.0/src/lib.rs:659`:
  `assert_eq!(indices.len(), roots.len(), "root indices must be unique")`) so we
  never hand the engine two roots under one index.
- **Ordering is normalised, not assumed.** Output roots are sorted
  (by path length, then by path) before being handed to the walk, so the result
  vector is in ascending `usize` order rather than completion order. A test
  asserts the *consequence* — that no two outputs overlap, with
  `outputs[i].1 != outputs[i+1].1` — instead of asserting the sort itself,
  which would make the test a tautology on a different sort.

Do not "optimise" by asserting the comparator. See
[`test-evidence`](../test-evidence/SKILL.md).

## Candidate indices must be unique — and the panic is already fixed

A repeat `candidate_index` is measured once and every occurrence reports that
one measurement. `measure_many_targets` keeps a `seen_indices` set
(`src/traverse.rs:191`) and `continue`s on a repeat (`:198-200`): the first
occurrence wins, later duplicates are dropped rather than measured twice.

This was **not always so**. An earlier version fed two roots under one index
straight into `dua_core::walk_roots` and tripped its uniqueness assertion into
a panic. The fix is in, and the regression test is
`duplicate_candidate_indices_do_not_panic_and_are_measured_once`
(`src/traverse.rs:529-567`).

Two consequences for you:

- The test **pins the fix**. If you change the deduplication, that test must
  still say "does not panic".
- The assertion in `dua-core` is about **unique indices**, not about
  `target != build_root`. An earlier version of this skill described it as the
  latter, which is a different (and weaker) invariant. Do not rely on it to
  catch a nested-root mistake — the deduplication above does that.

## Routine vs Full, and what state is allowed to do

`policy::resolve` turns `{cli_root, full}` into a concrete scope. Full walks
every root under the scan root; Routine uses the learned state plus a recency
window (default 300 seconds, `config.toml`).

Learned state is **advisory, never authority**. Rules that are not negotiable:

- Manifests carrying `ignore.unignore` **are** literal **absolute** paths, **not**
  filesystem globs. `unignore` is absolute-checked (`src/config.rs:122-123` →
  `require_absolute(p, "unignore")`) and matched as `PatternKind::Literal`
  (`src/config.rs:161-162`), so it supports neither `*`, `**`, nor `?` — it
  predates glob support by design. Editing it needs a plan, not a doc update.
  By contrast `scan.ignore` **is** a glob (`src/config.rs:148-151`) and must be
  absolute (`src/config.rs:116-120`).
- There is **no `[package] ignore` feature.** An earlier version of this skill
  claimed manifests carrying one are pruned from discovery; nothing in `src/`
  reads it. Do not reintroduce the claim.
- State written by a **newer** binary, or unreadable, is never overwritten.

### The working directory joins the maintenance scope (ADR 009)

Since ADR 009, a bare `cargo cleanme` run's scope is the Routine scope
**plus the operator's position**: the workspace that encloses the working
directory, when there is one, and the working directory itself
(`src/workspace.rs:655`). The rules that make it safe are the ones to carry:

- **The universe may only ever grow.** The operator's roots are *added* to the
  Routine roots, never substituted for them (`src/main.rs:974-983`). A wider
  universe can only reclassify `shared` bytes to `private`; the reverse — the
  only direction that could promote an unproven shared output into something
  the engine would delete — is unreachable by construction.
- **The working directory joins only when the Routine scope does not already
  reach it** (`src/main.rs:937-944`). It may not be *inside* a routine root,
  and it may not *contain* one. The second exclusion is a measurement, not a
  taste: without it, a bare run from the home directory became a 65-second
  sweep of 264,183 directories that ended **blocked** on 1,357 unresolvable
  manifests in `~/.local/share/Trash`. `clean ROOT` is the spelling for that
  wider sweep.
- **The decision is made against the *admitted* routine roots**, so a root the
  maintainability rule refuses — an agent worktree, say — does not count as
  the Routine scope reaching the operator's directory.
- **The operator's roots carry `RootProvenance::Operator`**
  (`src/policy.rs:100`). They are exempt from the maintainability rule only —
  standing there is an explicit act, and `clean ROOT` already permits the same
  subtree. Every other defect keeps the automatic root's handling: a symlink
  blocks the scope, an absent or non-directory root is omitted.
- **`clean ROOT` and `clean --full` are unchanged**, and the JSON `scope`
  label stays `"routine"`.
- **The read-only `scan --known` inventory applies the same rule**
  (`src/main.rs:1232-1241`). An inventory that described a smaller universe
  than the cleanup would not be an inventory of it.

### `scan.root` is honoured *except* by a Full scan

`policy::resolve` (`src/policy.rs:19-30`) has two arms:

- The **Full** arm returns at `:20-29` **before the config is read**. A Full
  scan therefore **ignores `scan.root` entirely** — it is a machine-wide
  reconciliation, and the configured root is not a filter on it.
- Every other arm takes `request.cli_root.or_else(|| config.root.clone())`
  (`:30`), so `scan --known` and `scan ROOT` *do* honour it.

"Always overrides the discovered root set" was the old, wrong version of this
claim. The other half is easy to get wrong too: `scan.root` narrows the
**scan scope**, but the **working-directory anchor still narrows the
selection** for a cleanup (`src/cli.rs:387-390` →
`CleanupSelection::WorkingDirectory`). A configured root therefore does *not*
mean "cleanup targets exactly that root" — running from outside it yields
zero units.

## Uncertainty is reported, and the vector that would carry it is empty

Diagnostics carry a `kind` (`Untraversable`, `Unreadable`, `IoError`,
`Vanished`) and a `severity` (`Warning`, `Error`, `PlatformRoot`). Three
consequences:

- Diagnostics with `path: None` are **dropped before retention**, so a
  diagnostic change that starts emitting `None` can silently stop retaining
  anything.
- A walk-root path (on unix, `/`) retains **everything** under it, which is why
  platform-root errors force Full scans to stay partial.
- `PlatformRoot` + `Error` is what makes a Full scan `incomplete`. That
  predicate is computed **once**, as `full_traversal_complete`
  (`src/main.rs:1151-1155`), and reused: as `complete` for reconciliation
  (`:1103`) and as `full_incomplete` for the exit code (`:1198`). It is not
  two copies of one expression, and you should not make it two.

> **The uncertainty vector is empty in production.** The filter at
> `src/main.rs:1162-1170` collects the three uncertainty categories
> (`PermissionDenied`, `Metadata`, `Vanished`) and then `.filter_map(|d| d.path.clone())`
> — but all three categories are emitted with `path: None`
> (`src/discovery.rs:786-793` and `:981-988`), because `dua_core`'s error
> carries no child path. The vector is therefore **always empty**.
>
> Two traps follow. First, the filter is a `matches!` over **three of the seven**
> `DiagnosticCategory` variants (`src/domain.rs:68-78`) with **no `_` arm**, so
> a newly-added category is *silently* excluded. Second, do not describe the
> path-carrying protection as if it were live — an earlier version of this skill
> did, and `architecture/05-discovery.md` still carried the same error until
> this pass corrected it.

## State is an optimization only

A failure to persist state must never change a scan result or the exit code. It
did once: a failed Full publish returned `Ok(1)`, contradicting the module's own
stated invariant, and the Routine branch ignored the byte-identical warning.
The `state_reconciled` flag was removed. Do not reintroduce a save-result branch
into the exit decision.

## Before you claim the change is safe

- A `scan --full` from a relative root and from an absolute root produce the
  same workspace count. (C015's regression test.)
- Unresolved manifests appear in the report as `manifest_observed_unresolved`,
  and — for cleanup — block the **boundary** they are inside. An unresolved
  manifest *outside* the boundary is reported and never blocks. See
  [`cleanup-safety`](../cleanup-safety/SKILL.md).
- A `scan.root` set in config produces `scope: "explicit"` in JSON. The label is
  derived from the **resolved** `policy.scope`, never from the CLI flags.
- Repeated output roots do not panic and do not double-count bytes.
- A Full scan still ignores `scan.root`, and a non-Full scan still honours it.

Then run the full gate and `scripts/check-doc-citations.py`.

## Related

- [`cleanup-safety`](../cleanup-safety/SKILL.md) — what this subsystem's output
  is allowed to authorise
- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md)
- [`cli-surface-and-argv`](../cli-surface-and-argv/SKILL.md) — the selectors
  that decide which of these arms runs
