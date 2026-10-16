# ADR 006 (accepted) — Working-Directory-Anchored Cleanup Selection

Status: **accepted**.
Date: 2026-10-15.
Repository baseline: `ba44927` (`main`, v0.2.7).
Milestone: `plans/implementation/artifact-discovery-cleanup/m014-cwd-scoped-default-and-interactive-selection.md`.
Amends: nothing. ADR 001 §C004/C006 and C003 remain binding and unmodified.

## Context

The operator-facing default is `cargo cleanme`, with no arguments, from
wherever the shell happens to be. Its Routine scope is seed roots plus learned
roots — conventionally something like `$HOME/projects` — so the command acts on
a whole machine's worth of projects regardless of intent, and an operator who
wants "clean this project" has no spelling for it.

The obvious fix is to scan only the working directory. That is unsafe, and
ADR 004 already identified why and named the tradeoff. Measured on a disposable
fixture at `ba44927` (two independent workspaces, one redirecting `target-dir`
into the other's, 18,505,728 bytes, two claimants):

| Scan root | `ownership` | Simulate |
|---|---|---|
| parent of both | `shared` | `0 would-clean, 2 skipped` |
| the one workspace | `private` | **`1 would-clean, 17.65 MiB`** |

Ownership is classified over the union of *discovered* workspaces. Narrowing
discovery therefore makes out-of-scope claimants invisible and can promote
shared bytes to `PrivateBounded`, the only class that authorizes deletion.

## Decision

1. **The working directory anchors selection, not discovery.** Root resolution
   and the ownership universe are unchanged for bare invocation. A selection
   filter runs *after* classification and removes non-matching `CleanupUnit`s.
2. **Selection can only remove candidates.** It cannot add one, reclassify one,
   or unblock a scope. It is strictly weaker than the status quo, never stronger.
3. **Anchor matching is bidirectional** — a unit qualifies when its canonical
   workspace root equals the anchor, contains it, or is contained by it — so
   both "standing in a member of a virtual workspace" and "standing above
   several projects" behave as an operator expects.
4. **Excluded units are reported** with a typed reason code naming the anchor.
   Silence would recreate the M012A silent-no-op defect.
5. **`scan` requires `-a`/`--all`** for Full. The machine-wide reconciliation
   that may expire learned roots is never one character from the new default.
6. **Interactive selection is a selection surface, not a deletion path.** It
   resolves `CleanupSelection::Roots` and reuses the one cleanup engine.
7. **`crossterm` is adopted** for raw mode, keys, and rendering, inline rather
   than alternate-screen. ADR 001's rejection of a full-screen TUI addressed
   *transient progress*, which is a different requirement from an
   operator-driven selection session, and is not extended to this milestone.

## Consequences

- Bare `cargo cleanme` becomes predictable: it cleans what is here. It
  discovers as broadly as before, so a shared target shared with a project
  elsewhere stays `shared` and stays inventory-only.
- Routine users keep ADR 004's redirect protection. The cost is a wider scan
  than a literal CWD scan would need — accepted deliberately, and the reason the
  default is not as fast as it could be.
- Two spellings that used to be identical can now differ: bare invocation and
  `clean --known` share the anchor; `clean ROOT` and `clean --full` do not.
  This is recorded in the CLI contract, not inferred.
- The crate gains a terminal dependency. Recorded in ADR 007.

## Alternatives rejected

- **Narrow the discovery universe to CWD.** Rejected above, with a measurement.
  It is also exactly what ADR 004 blocks.
- **Narrow discovery *and* add a confirmation prompt.** Rejected: the prompt is
  procedural, while the defect is a missing proof. It would make the unsafe
  verdict reachable behind an Enter key.
- **Keep the wide default, add `--here`.** Rejected: it leaves the footgun in
  place as the default and adds a spelling, which is what ADR 004 criticised.
- **Partition the universe (ADR 004).** Still proposed and still unaccepted
  *at the time of this decision*.
  M014 deliberately does not require it: filtering the selection achieves the
  operator goal without partitioning the proof at all.

> 2026-10-16 amendment: ADR 004 has since been **accepted** with an explicit
> C004/C006 amendment (per-island activation; refused manifests ride as
> passengers), recorded in ADR 001 and implemented by M016 behind the
> `--partitioned` opt-in flag. The rejected alternative above stays rejected —
> M014 never narrows the discovery universe — and partitioning the proof is
> now a governed opt-in rather than an unaccepted proposal. The history above
> is preserved; this note records what changed.