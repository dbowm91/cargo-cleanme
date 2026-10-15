# M014 — Working-Directory-Scoped Default and Interactive Workspace Selection

Status: **active** — plan accepted, implementation in progress.
Baseline: `ba44927` (`main`, v0.2.7 published).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Decision record: `plans/adr/006-accepted-working-directory-anchored-selection.md`.
Corrects forward: M012A canonical bare Execute (its *root selection*, not its mode contract);
ADR 001 §C004/C006 remain binding and are **not** amended by this milestone.
Skills: planning-and-closure; discovery-and-ownership; cleanup-safety; test-evidence; json-and-exit-contract.

## Objective

Three operator-facing changes, in one milestone because the first two are
inseparable from the third:

1. Bare `cargo cleanme` anchors its **selection** to the working directory.
2. `cargo cleanme scan` requires an explicit `-a`/`--all` to mean Full.
3. `cargo cleanme -i` presents an interactive selection list (NOR/INS).

## The problem this milestone must not create

The naive reading of (1) — "scan only the directory the user is in" — is **not
implementable as a safe default**, and the repository has a proposed ADR on
exactly this point (ADR 004 §"Alternatives and risks": *"Require explicit scope
for all cleanup: simpler but changes the canonical bare `cargo cleanme`
contract, so would need its own product/CLI decision"*).

Narrowing the *discovery universe* silently upgrades an ownership verdict.
Measured against a disposable fixture at `ba44927` — two independent
workspaces, `other` redirecting its output into `ws/target` through
`CARGO_TARGET_DIR`, 18,505,728 allocated bytes with two live claimants:

| Scan root | `workspace_roots` | `ownership` | Simulate disposition |
|---|---|---|---|
| `/tmp/cwdtest` | `other`, `ws` | `shared` | `0 would-clean, 2 skipped` — *"shared output remains inventory-only"* |
| `/tmp/cwdtest/ws` | `ws` | `private` | **`1 would-clean, 17.65 MiB`** |

Nothing but the width of the scan changed. Because `ownership` is computed
over the union of *discovered* workspaces, shrinking discovery makes
out-of-scope claimants invisible, and `PrivateBounded` — the only class that
authorizes `cargo clean` — is reachable for bytes another live workspace owns.
ADR 004 rejects this shape outright: *"Treat selected roots as isolated by
default: rejected because Cargo can redirect outputs across roots; root
containment is not a Cargo configuration boundary."*

## Decision: filter the selection, never the universe

The working directory constrains **which resolved `CleanupUnit`s are
candidates**. It never constrains **which workspaces are discovered or
classified**.

- Root resolution is unchanged: bare invocation still resolves the full Routine
  scope (configured `scan.root` override, else seed + learned roots) and still
  proves ownership across that whole combined universe.
- `workspace::build_groups` still classifies every discovered workspace, so a
  target shared with a workspace outside the anchor stays `Shared`.
- A new selection filter drops non-matching units **after** classification.

This delivers the operator-visible behaviour ("only clean what is here")
without weakening C003/C004/C006. It is strictly *narrower* than today's bare
invocation in what it deletes, and strictly *equal* in what it proves.

### Selection vocabulary

```rust
pub enum CleanupSelection {
    /// No filtering. `clean ROOT`, `clean --full`.
    All,
    /// Units whose canonical workspace root equals the anchor, is an ancestor
    /// of it, or is a descendant of it. Bare invocation; the anchor is `$PWD`.
    Ancestry(PathBuf),
    /// Units whose canonical workspace root is exactly one of these.
    Roots(Vec<PathBuf>),
}
```

`Ancestry` matches in **both** directions deliberately. A virtual workspace at
`~/projects` with the operator standing in `~/projects/member` must still clean
that workspace, and an operator in `~/projects` must be able to clean one child
project. A descendant-or-ancestor rule is the only one that makes "where I am
standing" mean the intuitive thing in both layouts.

### Why filtered-out units are reported, not silently dropped

M012A shipped a defect where a configured `scan.root` made `clean --known` a
*silent no-op* it never performed. A selection filter can reproduce exactly that
failure. Every unit excluded by the selection is therefore reported as a
`Skipped` result carrying a typed reason code naming the anchor, so "nothing
was selected" is distinguishable from "nothing was eligible".

## `scan` requires an explicit Full

`cargo cleanme scan` with no ROOT currently means Full — the machine-wide
reconciliation and the only operation allowed to expire learned roots. Under the
new default that spelling would be a footgun (a wide destructive-adjacent read
one character away from the new default), so it becomes `scan --all` / `scan -a`.
`scan ROOT` and `scan --known` are unchanged. The hidden `scan --full` alias is
retained and is equivalent to `--all`.

## Interactive selection

`-i`/`--interactive` runs a read-only resolution first, then presents **one row
per resolved `CleanupUnit`** — not raw directories — carrying the reclaimable
bytes. Rows are what the engine can act on, and known bytes are what make
"print the size of the deleted contents" meaningful.

| Key | Mode | Effect |
|---|---|---|
| `↑`/`k`, `↓`/`j` | NOR | Move the cursor (vim-style) |
| `space` | NOR | Toggle the row under the cursor |
| `i` | NOR | Enter INS |
| `Esc` | INS | Return to NOR, keeping the filter |
| `Esc` | NOR | Arm exit; a second `Esc` exits |
| `q`, `Ctrl-C` | either | Exit immediately |
| `Enter` | NOR | Clean the selected units |
| printable keys | INS | Append to the filter field |

Row 0 is an **all** toggle. INS filters live against the full unit list, so a
filter never discards an existing selection.

Hard constraints:

- **Raw mode is restored on every exit path, including panic** (RAII guard). A
  tool that can leave a terminal in raw mode is not shippable.
- Refuses to start when stdin or stdout is not a TTY, with a typed usage error.
- The TUI is a *selection* surface. It feeds the existing cleanup engine and
  becomes `CleanupSelection::Roots`; it never introduces a second deletion path.
- `--dry-run -i` selects and reports but spawns no `cargo clean`.
- Rendering is **inline** (no alternate screen), matching ADR 001's standing
  rejection of alternate-screen/raw-mode *progress* rendering.

## Dependency decision

`crossterm` is added for raw mode, key events, and rendering. ADR 001 rejected
`ratatui`/`crossterm` for *transient progress*, which is a different requirement
(an always-on inline status line, not an operator-driven session). The rejection
is not extended to a selection UI that did not exist when it was written.

Cost is recorded honestly: this adds a dependency to a crate whose minimal set
is a stated property. `crossterm` is cross-platform, which matters because
Windows terminal handling is where this repository has repeatedly been bitten
(C020, C021, C024).

## Work packages

- **A.** `CleanupSelection` in `workspace.rs`/`cleanup.rs`, applied after
  classification. Typed skip code for filtered-out units.
- **B.** `CleanupRequest.selection`; bare invocation and `clean --known` anchor
  to `$PWD`; `clean ROOT`/`--full` stay `All`.
- **C.** `scan --all`/`-a` with `required_unless_present_all`.
- **D.** `src/interactive.rs`: NOR/INS state machine, pure and testable, with
  the terminal behind a narrow seam.
- **E.** Generated docs, architecture deep dives, citation re-derivation.
- **F.** Verification ladder + hosted evidence.

## Hard invariants (unchanged by this milestone)

- Ownership proof, not timestamps. Nothing becomes `PrivateBounded` because it
  was selected, anchored, or chosen by the operator.
- Authorization never manufactures proof.
- Re-validation at the last moment, per spawn.
- Unresolved ownership blocks the whole scope.
- Deletion is always `cargo clean`.
- Selection is **advisory narrowing only**: it can remove candidates, never add
  one, never reclassify one, and never rescue a blocked scope.

## Required evidence

1. The `shared`-stays-`shared` control above, asserted against the shipped
   binary at both widths, with `ownership` read from the JSON contract.
2. A unit excluded by selection appears as a typed skip, not as silence.
3. `scan` with no ROOT and no `-a` is a usage error; `scan -a` and `scan --full`
   reach Full identically.
4. Simulate and Execute receive the identical selected unit set.
5. TUI state-machine tests with a fake terminal: key routing per mode, filter
   persistence across mode changes, all-toggle, exit arming.
6. A pty test proving raw mode is restored after an aborted or panicking session.