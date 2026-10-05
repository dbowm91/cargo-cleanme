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
that **fails**, which is how the blocking behaviour above is testable at all.

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
  both, and `dua_core::walk_roots` must assert `target != build_root` internally
  so we never count the shared prefix twice.
- **Ordering is normalised, not assumed.** Output roots are sorted
  (by path length, then by path) before being handed to the walk, so the result
  vector is in ascending `usize` order rather than completion order. A test
  asserts the *consequence* — that no two outputs overlap, with
  `outputs[i].1 != outputs[i+1].1` — instead of asserting the sort itself,
  which would make the test a tautology on a different sort.

Do not "optimise" by asserting the comparator. See
[`test-evidence`](../test-evidence/SKILL.md).

## Candidate indices must be unique

A repeat `candidate_index` is measured once and every occurrence reports that
one measurement. The reason is not tidiness: `dua_core::walk_roots` asserts
`target != build_root` internally, and a duplicate index trips that assertion
into a panic. The regression test is
`duplicate_candidate_indices_do_not_panic_and_are_measured_once` in
`traverse.rs`. If you add a way for two outputs to share an index, you have
reintroduced a panic on a duplicated manifest path.

## Routine vs Full, and what state is allowed to do

`policy::resolve` turns `{cli_root, full}` into a concrete scope. Full walks
every root under the scan root; Routine uses the learned state plus a recency
window (default 300 seconds, `config.toml`).

Learned state is **advisory, never authority**. Rules that are not negotiable:

- A manifest carrying `[package] ignore` is pruned from discovery. It is
  invisible, not reported.
- Manifests carrying `ignore.unignore` **are** the full relative path, **not** a
  filesystem glob. A 4-segment pattern is a literal 4-segment path plus literal
  children, and it does not support `*`, `**`, or `?` — it predates glob
  support by design. Editing it requires a plan, not a doc update.
- `scan.root` is a first-class config option and always overrides the
  discovered root set.
- State written by a **newer** binary, or unreadable, is never overwritten.

## Uncertainty is the interesting output

Diagnostics carry a `kind` (`Untraversable`, `Unreadable`, `IoError`,
`Vanished`) and a `severity` (`Warning`, `Error`, `PlatformRoot`). Three
consequences:

- Diagnostics with `path: None` are **dropped before retention**, so a
  diagnostic change that starts emitting `None` can silently stop retaining
  anything.
- A walk-root path (on unix, `/`) retains **everything** under it, which is why
  platform-root errors force Full scans to stay partial.
- `PlatformRoot` + `Error` is what makes a Full scan `incomplete` — and that
  predicate is evaluated twice in `main.rs` (once as `complete` for
  reconciliation, once as `full_incomplete` for the exit code). If you change it,
  change both or extract a helper; they are currently two copies of one
  expression.

## State is an optimization only

A failure to persist state must never change a scan result or the exit code. It
did once: a failed Full publish returned `Ok(1)`, contradicting the module's own
stated invariant, and the Routine branch ignored the byte-identical warning.
The `state_reconciled` flag was removed. Do not reintroduce a save-result branch
into the exit decision.

## Before you claim the change is safe

- A `scan --full` from a relative root and from an absolute root produce the
  same workspace count. (C015's regression test.)
- Unresolved manifests appear in the report as
  `manifest_observed_unresolved`, and — for cleanup — block the whole scope.
- A `scan.root` set in config produces `scope: "explicit"` in JSON. The label is
  derived from the **resolved** `policy.scope`, never from the CLI flags.
- Repeated output roots do not panic and do not double-count bytes.

Then run the full gate and `scripts/check-doc-citations.py`.

## Related

- [`cleanup-safety`](../cleanup-safety/SKILL.md) — what this subsystem's output
  is allowed to authorise
- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md)
