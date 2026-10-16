# M017 — Scoped Proof Default (ADR 007 implementation)

Status: **active**. Owner-directed 2026-10-16; ADR 007 accepted. No gates:
the decision supersedes M016's opt-in design and C035-as-config before any
publication (0.2.9 unpublished; neither shipped to users).

Baseline: current HEAD (M016 conditionally closed on branch
`plans/m015-proof-island-certificate-and-partition-report`).

## Objective

Make scoped proof the only behavior: the proof boundary is the selected
scope (CWD anchor for bare/`--known`, explicit roots for `clean ROOT`,
full system for `clean --full`); out-of-boundary unprovable participants
are bounded diagnostics, never blocks; exits are boundary-scoped. Remove
the `--partitioned` flag, the `islands` report axis, and the whole-scope
partition report they served. Do not merge C035-as-config; its premise is
adopted here, its mechanism is abandoned.

## Non-goals

- No config-file scope declarations. Boundary selection stays exactly
  today's argv: anchor, explicit roots, `--full`.
- No islands within a boundary: in-boundary unresolved (non-passenger)
  blocks the whole boundary. The refused-manifest passenger rule is
  retained, not extended.
- No change to `clean --full` blocking, `scan --all`, discovery, grouping,
  unit construction, per-spawn revalidation, or the `update_json` surface.

## Contract (decided, ADR 007 items 5–6)

- Exits: 0 when the requested boundary is fully cleaned (out-of-boundary
  poison reported as diagnostics); 1 when the boundary itself is blocked or
  a unit fails; 2 for usage. `main.rs` exit logic keys off
  `scope_blocked`/failures already — it changes only in what sets the
  block. Zero-unit edge (fail-closed, M012A): reported notes never block
  proceeding units, but zero proven units plus any unproven participant
  fails closed with the standard unproven-ownership block. Units merely
  unselected do not count.
- JSON: remove `CleanupV1.islands` / `IslandV1` (never published);
  `unresolved_ownership` lists boundary-relevant participants only;
  out-of-boundary poison travels in `diagnostics` (existing C032 chains).
  No new reason code, no envelope bump.
- Human: rows + block as today; out-of-boundary unprovables named in the
  existing unresolved-manifest listing (C032, up to five) plus one line
  stating N out-of-scope participant(s) reported, not blocking. Remove the
  M016 suffix and island-outcomes section.
- `--partitioned` removed from clap, `CleanupRequest`, threading, tests,
  docs, completions/man (regenerated). 0.2.9 never published, so removal is
  free and total — no alias, no deprecation path.
- `-i`: lists anchored candidates as today; the M016 merged-refusal goes
  away with the merge rule it served. Blocked boundary shows the block.

## Engine

Boundary rule (single rule, in `resolve_cleanup_scope` + refresh):

- A participant is in-boundary iff its manifest path is under an
  in-selection root: for `All` selection every selected root counts
  (`--full` and explicit roots keep today's behavior exactly); for
  `Ancestry`/`Roots` selection only the selected roots count.
- In-boundary `MergeAll` unknowns (byte-capability unproven) block the
  boundary with the existing typed block. In-boundary passengers and scope
  notes are reported, never blocking. Out-of-boundary unresolved becomes a
  bounded diagnostic and never sets `scope_blocked`.
- Zero-unit edge: zero proven units plus any unproven participant (in the
  boundary or out) fails closed with the standard unproven-ownership
  block; units merely unselected do not count.
- Discovery, resolution, grouping, and unit construction stay combined over
  the whole selected-root scope (M014 measurement: narrowing discovery
  promotes `Shared` to `PrivateBounded`). Cross-boundary resolved claims
  still force `Shared`/skip at grouping time.
- `refresh_combined_universe` stays combined (maximal late-change
  detection) but re-applies the boundary rule against the fresh workspaces
  instead of carrying a scan-time tolerated list: out-of-boundary and
  in-boundary passenger/note unknowns stay tolerated; only a fresh
  in-boundary `MergeAll` invalidates the refresh. H-2 rediscovery stays
  combined (mid-run change detection is not poison reporting).
- `output_env_matches` re-verification stays unconditional per dispatch.
- Delete: `ProofPartition` computation, `retain_proceed_island_units`,
  `partition_merge_cause_label`, `partition_outcome_section`,
  `append_partitioned_suffix`, M015 whole-scope partition report. Keep:
  `UnknownCapability` classification + refused-passenger attribution (still
  load-bearing for the in-boundary passenger rule).

## Tests

- Delete the flag-off parity test, the CLI flag parse test, and the
  `--partitioned` argv from the five island subprocess tests. G1 (in-root
  scope-note orphan) proceeds with a reported note, exit 0 — the defect
  fix. G1c (refused passenger) still proceeds — keep as the passenger
  rule. Drop the mid-run-growth and passenger-redirect twins only
  if their mechanism is deleted; keep each twin whose mechanism survives
  (H-2 growth block, passenger attribution, env probe) with flagless argv.
- New (all discriminating, each names its failing input):
  1. Out-of-anchor poison twin (lib `out_of_anchor_poison_does_not_block_anchor`
     + subprocess `out_of_anchor_poison_does_not_block_bare_routine_cleanup`
     with a selective stub refusal outside the CWD anchor): anchor cleans,
     exit 0, poison in `diagnostics` as `out_of_scope` and absent from
     `unresolved_ownership`, zero `scope_blocked`. Fails pre-change as a
     whole-scope block (this is the codified defect).
  2. `--full` and explicit-root behavior unchanged: `All` selection keeps
     the old rule exactly (covered by the metadata-failure and
     zero-unit lib tests, which still block).
  3. Zero-unit edge twin (`empty_proof_universe_blocks_listing` + engine
     paths): zero proven units plus any unproven participant fails closed.
     Also guards the M014 elsewhere case (unselected units do not count)
     and the empty-scope success case.
- Audit every existing test asserting `scope_blocked`/exit 1 with poison
  outside its effective boundary (C006 cross-root, C031/C034 blocked-scope
  cases): each keeps passing only if its poison is in-boundary; any test
  whose block depended on out-of-boundary poison is updated to the new
  rule *individually with its reason stated* — no blanket find-and-replace.
- Key-set test back to 14 keys; log-line test unchanged (counts-only);
  render tests updated for the removed suffix/outcomes and the new
  out-of-scope line.

## Docs and version

- `architecture/09-cleanup.md`: rewrite the M016 activation section as the
  default scoped rule; `02-cli`: remove flag row; `07-workspace`,
  `10-reporting` (remove `islands`), `13-orchestration`,
  `architecture/overview.md` counts; `docs/USAGE.md` boundary semantics;
  `CHANGELOG.md [Unreleased]`: replace the `--partitioned` entry with the
  scoped-proof entry. Regenerate completions/man (`generate-docs`).
- Version stays 0.2.9 (unpublished). No publication authorized here; the
  ADR 004 item (e) release gate transfers to M017's release.

## Verification and stop conditions

Full ladder (`fmt`, `clippy -D warnings`, `cargo test --all-targets
--all-features`, both checker self-tests + runs, `generate-docs --check`,
`git diff --check`), hosted Linux/macOS/Windows/MSRV lanes on the PR,
closure record `m017-status.md`.

**Stop** if any change narrows discovery/grouping to the boundary, lets an
in-boundary unresolved participant proceed, promotes `Shared`/`Uncertain`/
`ExternalUnproven`, alters `clean --full` blocking, or changes the
`update_json` surface. Out-of-scope blocking reappearing anywhere is the
defect — fix it, do not re-gate it behind a flag.
