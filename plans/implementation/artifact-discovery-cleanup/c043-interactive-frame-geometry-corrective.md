# C043 — Interactive Frame Geometry Corrective

Status: **closing** — implemented and ladder-green on `main` at `1d5f1e8`
(0.2.10, unpublished; latest published tag `v0.2.10`). Not published: no
tag, no release, no crates.io publication. Evidence in
`plans/closure/artifact-discovery-cleanup/c043-status.md`.

Date: 2026-10-22.
Repository baseline: `main` at `1d5f1e8` (0.2.10, unpublished).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: M014 (the interactive selection surface),
`plans/implementation/artifact-discovery-cleanup/m014-*.md`; C039 (the
compact selection surface rebuild),
`plans/implementation/artifact-discovery-cleanup/c039-rip-style-compact-selection-surface-corrective.md`;
C042 (the interactive surface's rendering and cursor lifecycle),
`plans/implementation/artifact-discovery-cleanup/c042-interactive-surface-rendering-corrective.md`.
Original closure records:
`plans/closure/artifact-discovery-cleanup/m014-status.md`,
`plans/closure/artifact-discovery-cleanup/c039-status.md` and
`plans/closure/artifact-discovery-cleanup/c042-status.md`.
Related: ADR 001 (raw mode and alternate-screen rendering rejected for
*progress*), ADR 006 (the selection is a filter over already-proven units),
ADR 007 (the scoped proof boundary), C036 (documentation drift),
overview findings 60, 66, 48, and the width-in-characters gap.

## Objective

Make the interactive frame fit the terminal it is painted on.

C042 made the frame's *bytes* correct. It did not make the frame's
*geometry* correct, and the two are independent: a frame can be written
perfectly — every line starting with a carriage return, every character
intact — and still be the wrong shape for the terminal it lands in. A live
capture over a real pty on 2026-10-22 showed both axes wrong at once: in a
terminal narrower than forty columns the frame **wrapped**, so the region the
operator was supposed to be left with was never erased; in a terminal shorter
than ten rows the frame **scrolled**, so its own title line — the arithmetic
`N selected of M, X reclaimable` — was pushed off the top of the screen.

Two defects are in scope. They are listed as D1–D2 below.

## Non-goals

- **Not** a change to the selection contract. What the operator sees is still
  the engine's own unit set, a selection can only *remove* candidates, and
  the committed roots are still the checked roots in candidate order.
- **Not** a change to the NOR/INS model, the published key table, the three
  endings, the exit codes, the JSON envelope, or the reason codes.
- **Not** an alternate-screen surface. ADR 001's rejection of raw mode and
  alternate-screen rendering for *progress* is not extended to a transient
  selection list; C039 kept the inline decision and this corrective keeps it.
- **Not** a new dependency. The width table is small and in-crate; the crate's
  minimal dependency set is a stated property.
- **Not** the C036 documentation-drift corrective. C036 stays open; this
  corrective touches only the documents that describe the surface it changed.
- **Not** a pty harness in CI. The byte stream is a pure function, and the
  tests replay it through a small in-test terminal emulator. A pty harness
  remains open (finding 48) and is still the right next step for the parts a
  byte-level model cannot prove — that the OS honours the calls.

## Current implementation evidence

Captured on 2026-10-22 over a **real pty** (`pty.fork`, the slave in raw
mode), driving the real binary against a four-project fixture, and replayed
through a terminal emulator that models xterm's pending-wrap rule.

**Before — the width axis.** A 24-row, 39-column terminal. The frame was
forty cells wide, so every line wrapped; the frame occupied nine rows' worth
of lines in eighteen rows, and the erase — which moved up by the frame's
*line count* — left the residue standing:

```
|…s — 0 selected of 4, 0.00 B reclaimabl|  <- row 0
|e                                      |  <- row 1
|NOR  filter: …                       —/|  <- row 2
|4                                      |  <- row 3
|      WORKSPACE              RECLAIMABL|  <- row 4
|E                                      |  <- row 5
|> [ ] all                 (0/4 selected|  <- row 6
|)                                      |  <- row 7
|  [ ] alpha                   300.00 Ki|  <- row 8
|cargo-cleanme: interactive selection ca|  <- row 9
|ncelled; no cleanup commands were run  |  <- row 10
```

Rows 0–8 are the frame the erase was supposed to remove. The operator's
prompt came back *underneath* it.

**Before — the height axis.** An eight-row terminal. The frame was eight
lines, and the newline after the last of them took a ninth row, so the
terminal scrolled and the title line was gone before the operator could read
it:

```
|NOR  filter: <none>                                       —/4|  <- row 0
|      WORKSPACE                                 RECLAIMABLE|  <- row 1
|> [ ] all                                (0/4 selected)    |  <- row 2
|  [ ] alpha                                 300.00 KiB    |  <- row 3
|  [ ] beta                                  300.00 KiB    |  <- row 4
|v [ ] delta                                 300.00 KiB    |  <- row 5
|[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]|  <- row 6
```

At five rows the title *and* the status line were gone; at four rows the
column header too; at three rows only one candidate row and the key-hint bar
survived. The operator's scrollback was pushed up by the same scroll.

**After.** The frame is exactly the terminal's width and never more than the
terminal's height minus one row. At 39 columns:

```
|… — 0 selected of 4, 0.00 B reclaimable                     |  <- row 0
|NOR  filter: <none>                                  —/4     |  <- row 1
|> [ ] all                (0/4 selected)                     |  <- row 2
|      WORKSPACE             RECLAIMABLE                     |  <- row 3
|  [ ] alpha                  300.00 KiB                     |  <- row 4
|  [ ] beta                   300.00 KiB                     |  <- row 5
|  [ ] delta                  300.00 KiB                     |  <- row 6
|  [ ] gamma                  300.00 KiB                     |  <- row 7
|[↑↓ PgUp/PgDn • Space • i • Enter • Es…                      |  <- row 8
```

and the region is erased, leaving only the operator's own line. At eight rows
the chrome gives way and the title survives:

```
|cargo-cleanme — select workspaces — 0 selected of 4, 0.00 B reclaimable|  <- row 0
|NOR  filter: <none>                                                     —/4|  <- row 1
|> [ ] all                                                    (0/4 selected)|  <- row 2
|      WORKSPACE                                                 RECLAIMABLE|  <- row 3
|  [ ] alpha                                                    300.00 KiB|  <- row 4
|v [ ] beta                                                     300.00 KiB|  <- row 5
|[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]     |  <- row 6
```

A terminal smaller than twenty columns or six rows is now **refused**, with
an actionable message, before the listing spawns anything:

```
cargo-cleanme: configuration error: interactive selection requires a usable
terminal (a terminal of 100x5 is too small for the interactive list; it needs
at least 20x6); drop -i to clean without choosing interactively, or use
`cargo cleanme scan --all` to inspect the same units read-only
```

## The defects

| # | Defect | Severity |
|---|---|---|
| D1 | The frame's width was `max(MIN_WIDTH, terminal_width)` — clamped **up** to forty cells. In a terminal narrower than forty columns the frame was wider than the terminal it was painted on: every line wrapped, the frame occupied twice the rows it claimed, and the erase — which moves up by the frame's *line count*, not by the rows it occupies — left the mangled frame standing above the operator's prompt. **The surface was not cleaned up.** | **high** |
| D2 | The frame's height was bounded in *lines* against the terminal's height, but the frame *occupies* one row more than its line count: every line ends with a newline, and the last one moves the cursor onto the row below. A frame whose lines equalled the terminal's height therefore scrolled the terminal — pushing its own title line off the top in any terminal shorter than ten rows, and pushing the operator's scrollback up with it. | **high** |

## Why the previous verification missed it

- **The test asserted a clamped geometry, not the terminal's.** `the_frame_is_bounded_and_never_overflows_the_width` asserted that the frame fitted `max(w, MIN_WIDTH)` and `max(h, CHROME_LINES + MIN_LIST_ROWS)`. It was green *because* it blessed the clamp: the defect was the clamp. This is the AGENTS.md §5 lesson in the same form C042 found — a test that pins the defect.
- **The second test asserted a line count, not the rows occupied.** `the_page_is_capped_by_the_terminal_height` asserted `frame.len() <= h`. The frame occupies `frame.len() + 1` rows, so the assertion was one row short of the property that mattered.
- **C042's emulator modelled the carriage return, not the wrap.** The in-test terminal emulator models xterm's pending-wrap rule, so it *could* have caught D1 — but no test fed it a frame wider than its terminal, because the layout was never asked to.
- **C042 proved the bytes, and the bytes were not the problem.** The capture that found D1 had ten carriage returns for ten newlines. A frame can be byte-perfect and the wrong shape.

## Invariants

These are the properties the fix must not break.

1. **Nothing is deleted on the strength of a timestamp.** Unchanged — this
   corrective touches no proof, no scope, and no deletion path.
2. **The selection is a filter over already-proven units.** Unchanged: D1–D2
   are rendering defects, and neither can add, reclassify, or unblock a
   candidate.
3. **A tool that leaves the terminal in a state the operator did not ask for
   is not shippable.** Widened: the region must be *erased*, and the erase
   only removes the frame if the frame fits.
4. **The frame's geometry is a promise about the terminal it is painted on.**
   Every line is at most the terminal's width, and the frame occupies at most
   the terminal's height.
5. **The region's erase is a property of the scope, not of each exit path.**

## Production changes

All in `src/interactive.rs` and `src/main.rs`. No other module changes.

1. **The geometry is adopted, not clamped.** `set_size` takes the terminal's
   width and height as they are, floored only by the lead's own width (six
   cells), because a row cannot be drawn below it.
2. **The frame's shape is a plan.** `Plan::for_size` decides which chrome the
   terminal has room for. The chrome gives way in the order of what it costs
   the operator least: the key-hint bar, then the column header, then the
   all-toggle row. The title, the status line and at least one candidate row
   survive in a six-row terminal.
3. **The height bound is on the rows occupied.** The page is
   `budget - chrome`, where `budget` is the terminal's height less the row
   the trailing newline takes.
4. **The layout degrades in the width.** The reclaimable column goes first —
   the title line carries the total — then the ownership column's wording,
   because the class label is the shortest honest description of a
   non-private row; the workspace name keeps what is left. Every row's text
   is truncated rather than allowed to wrap.
5. **The status line's hint slot shrinks instead of overflowing.** The slot
   is the full hint when the terminal can hold it, the compact form when it
   cannot, and nothing when neither fits — reserved in every state, so
   arming the exit still does not resize the filter field.
6. **A terminal too small for the surface is refused.**
   `ensure_interactive_geometry` requires twenty columns and six rows, and
   the orchestrator checks it before the listing runs, so the refusal costs
   no `cargo metadata` spawns.

## Ordered work packages

- **W1 — the tests.** Rewrite the two tests that pinned the defect, *first*,
  so the fix cannot leave a green test pinning the new bug.
- **W2 — the plan and the layout.** `Plan::for_size`, the degraded columns,
  the truncated rows.
- **W3 — the geometry gate.** `ensure_interactive_geometry` and the
  orchestrator's check.
- **W4 — the evidence.** The live pty capture, before and after.
- **W5 — the records.** This plan, the closure record, the deep dive, the
  skills, the overview findings, the registry, the roadmap, and the
  re-derived census.

## Failure, cancellation, and contention semantics

- A **cancelled session** (`q`, `Esc` `Esc`, `Ctrl-C`) is still a success:
  exit 0, one line on stderr.
- A **terminal too small** is a usage error: exit 2, one message on stderr,
  and the terminal is never taken over — it is never put into raw mode, so
  there is nothing to restore.
- There is **no contention** surface: the interactive session owns one
  terminal.

## Compatibility and configuration effects

One behavioural change, and it is a refusal rather than a change of output:
a terminal smaller than 20x6 now gets a usage error (exit 2) instead of an
interactive session it cannot render. No CLI surface, no configuration key,
no environment variable, no reason code, no JSON field, and no exit code
*set* changes. `NO_COLOR` and `TERM=dumb` behave as before.

## Required tests

| Requirement | Test |
|---|---|
| The frame fits the terminal in both axes | `the_frame_fits_the_terminal_in_both_axes` |
| The erase removes every row the frame occupied | `the_erase_removes_every_row_the_frame_occupied` |
| A terminal too small for the list is refused | `a_terminal_too_small_for_the_list_is_refused` |
| A narrow terminal degrades the columns and keeps the title | `a_narrow_terminal_degrades_the_columns_and_keeps_the_title` |
| The page is capped by the terminal's height | `the_page_is_capped_by_the_terminal_height` |

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

## Documentation updates

- `architecture/16-interactive.md` — the geometry contract, the two axes, the
  chrome ladder, the refusal.
- `.skills/terminal-and-interactive/SKILL.md` — the frame-fits-the-terminal
  rule.
- `architecture/overview.md` — the module sizes, the census, and a new
  finding.
- `architecture/14-testing-and-verification.md` — the census and the
  `interactive.rs` row.
- `AGENTS.md` and `.skills/test-evidence/SKILL.md` — the re-derived census.
- `plans/registry.md` and the subsystem roadmap — the status.

## Acceptance criteria

1. The live pty capture shows the frame occupying no more rows than the
   terminal has, and no line exceeding the terminal's width, at 20, 30, 39,
   100 columns and at 6, 8, 10, 24 rows.
2. The erase leaves no residue at any of those sizes.
3. A terminal below 20x6 is refused with an actionable message, before the
   listing runs.
4. The two tests that pinned the defect were rewritten before the fix.
5. The ladder is green.
6. Every mutation of the fix is caught by at least one test.

## Stop conditions

Stop and record, rather than continue, if:

- the fix requires a new dependency (it does not);
- the fix would change the selection contract, the key table, the three
  endings, or any machine contract (it does not);
- a mutation is not caught by any test (none is).

## Closure evidence requirements

1. The ladder, green, with the commands actually run recorded.
2. The live pty capture, before and after, rendered.
3. The mutation table: each mutation, and the tests that caught it.
4. The re-derived census, cross-checked against the compiler's own list.
5. The known limitations, and the findings that stay open.
