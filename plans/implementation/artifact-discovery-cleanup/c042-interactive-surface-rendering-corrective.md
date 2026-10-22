# C042 — Interactive Surface Rendering Corrective

Status: **closing** — implemented and ladder-green on `main` at `de9628d`
(0.2.10, unpublished; latest published tag `v0.2.10`). Not published: no
tag, no release, no crates.io publication. Evidence in
`plans/closure/artifact-discovery-cleanup/c042-status.md`.

Date: 2026-10-22.
Repository baseline: `main` at `de9628d` (0.2.10, unpublished).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: M014 (the interactive selection surface),
`plans/implementation/artifact-discovery-cleanup/m014-*.md`; C039 (the
compact selection surface rebuild),
`plans/implementation/artifact-discovery-cleanup/c039-rip-style-compact-selection-surface-corrective.md`.
Original closure records:
`plans/closure/artifact-discovery-cleanup/m014-status.md` and
`plans/closure/artifact-discovery-cleanup/c039-status.md`.
Related: ADR 001 (raw mode and alternate-screen rendering rejected for
*progress*), ADR 006 (the selection is a filter over already-proven units),
ADR 007 (the scoped proof boundary), C036 (documentation drift),
overview findings 36, 48, and the width-in-characters gap.

## Objective

Make the `-i` selection surface render what it says it renders.

The interactive surface is the only part of the crate that takes control of a
terminal, and it had **never been observed by an automated test**. A live
capture over a real pty on 2026-10-22 showed that the surface it painted was
not the surface it drew: the frame was laid out as a staircase, every line
lost its first character to the row above, three of four candidate rows fell
off the bottom of the screen, the key-hint bar was erased, and the operator's
shell was left with no visible caret.

Seven defects are in scope. They are listed as D1–D7 below.

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
- **Not** a pty harness in CI. The byte stream is now a *pure function*, and
  the tests replay it through a small in-test terminal emulator. A pty
  harness remains open (finding 48) and is still the right next step for the
  parts a byte-level model cannot prove — that the OS honours the calls.

## Current implementation evidence

Captured on 2026-10-22 over a **real pty** (`pty.fork`, 100x30, the slave in
raw mode), driving the real binary against a four-project fixture, and
replayed through a terminal emulator that models xterm's pending-wrap rule.

**Before.** The byte stream of one frame contained **zero** carriage returns.
`crossterm 0.28`'s `enable_raw_mode` calls `cfmakeraw`, which clears `OPOST`,
so a bare `\n` is a linefeed *without* a carriage return: every line started
at the column the previous one ended in. The capture, rendered:

```
|cargo-cleanme — select workspaces — 0 selected of 4, 0.00 B reclaimable   |  <- row 0
|                                                                          N|  <- row 1
|OR  filter: <none>                                                     —/4 |  <- row 2
|     WORKSPACE                                               RECLAIMABLE  |  <- row 4
|                                                                          >|  <- row 5
| [ ] all                                                    (0/4 selected)|  <- row 6
| [ ] alpha                                                    8.00 KiB   |  <- row 8
| [ ] beta                                                     8.00 KiB   |  <- row 10
| [ ] delta                                                    8.00 KiB   |  <- row 12
| [ ] gamma                                                    8.00 KiB   |  <- row 14
|                                                                          [|  <- row 15
|↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]     |  <- row 16
```

Nine lines of frame occupied seventeen rows. `MoveUp(9)` then moved up nine
rows and erased from there, so the rewind that was meant to *keep* the frame
erased most of it.

The same capture contained `ESC[?25l` (hide cursor) **twice** and
`ESC[?25h` (show cursor) **zero** times, and the session exited 0 with
`cargo-cleanme: interactive selection cancelled; no cleanup commands were
run` — leaving the operator's shell with no caret.

**After.** The same capture contains ten carriage returns for ten newlines,
`ESC[?25l` once and `ESC[?25h` once, and the frame lands on nine
consecutive rows:

```
|cargo-cleanme — select workspaces — 0 selected of 4, 0.00 B reclaimable   |  <- row 0
|NOR  filter: <none>                                                     —/4|  <- row 1
|      WORKSPACE                                               RECLAIMABLE |  <- row 2
|> [ ] all                                                    (0/4 selected)|  <- row 3
|  [ ] alpha                                                    8.00 KiB   |  <- row 4
|  [ ] beta                                                     8.00 KiB   |  <- row 5
|  [ ] delta                                                    8.00 KiB   |  <- row 6
|  [ ] gamma                                                    8.00 KiB   |  <- row 7
|[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]     |  <- row 8
```

## The defects

| # | Defect | Severity |
|---|---|---|
| D1 | The frame was written with a bare newline. In raw mode that is a linefeed without a carriage return, so the frame was painted as a staircase: it occupied twice the rows it claimed, every line lost its first character to the row above, and the rewind — which moved up by the frame's *line count* rather than by the rows it actually occupied — erased most of the frame. | **critical** |
| D2 | The terminal cursor was never restored. `set_cursor_visible(true)` was called nowhere in the crate, so a session that ended in NOR left the operator's shell with no visible caret. (Overview finding 36, open since C039.) | **high** |
| D3 | The hardware cursor was *shown* in INS mode, at the end of the frame — nowhere near the filter field the operator was typing into. It was a third, unrelated cursor. | **medium** |
| D4 | A terminal read failure was swallowed as an unbound key. A terminal that went away turned the session into a busy loop, repainting frames at full speed with nothing left to read. | **high** |
| D5 | The painted region was left standing when the session ended in an error, contradicting the module's own contract that the region is erased on exit. | **medium** |
| D6 | The armed-exit hint was truncated away — and mangled — because the filter grew into it. The hint is the one part of the status line that carries an *action*. | **low** |
| D7 | The frame's geometry was measured in `chars()`, not in terminal **cells**. A path with CJK characters in it painted past the right edge, wrapped, and broke the layout exactly as D1 does. | **medium** |

## Why the previous verification missed it

- **The seam hid the bytes.** The `Terminal` trait's fake records *lines*,
  not bytes. Every assertion about the rendered surface therefore held
  against a frame that was never painted the way the production code paints
  it. The defect was in the one layer the seam could not see.
- **There is no pty harness** (overview finding 48). The rendered surface had
  never been observed by an automated test; C039's evidence was hand-captured.
- **The test that should have caught D2 pinned it.** `the_driver_shows_the_hardware_cursor_only_in_insert_mode`
  asserted that the *last* recorded cursor visibility was `Some(false)` — it
  was green *because* it asserted the bug. This is the AGENTS.md §5 lesson
  in its purest form.
- **C039's mutation evidence mutated the frame's shape, never its bytes.** A
  mutation that changes the number of lines is caught by a line-count
  assertion; a mutation that changes a `\r` into a `\n` is caught by nothing
  the suite had.
- **The width gap was disclosed but not exercised.** The deep dive recorded
  "width counted in characters rather than display cells" as an open,
  low-severity gap, and no fixture used a wide character.

## Invariants

These are the properties the fix must not break.

1. **Nothing is deleted on the strength of a timestamp.** Unchanged — this
   corrective touches no proof, no scope, and no deletion path.
2. **The selection is a filter over already-proven units.** Unchanged: D1–D7
   are rendering and lifecycle defects, and none of them can add, reclassify,
   or unblock a candidate.
3. **A tool that leaves the terminal in a state the operator did not ask for
   is not shippable.** The cursor obligation is widened from "the mode" to
   "the mode *and* the cursor", because they are separate escape sequences.
4. **The frame's geometry is a promise about terminal cells.** Every line is
   exactly the frame's visible width, and the frame occupies exactly the rows
   it claims.
5. **The region's erase is a property of the scope, not of each exit path.**
6. **A read failure is a failure.** An unbound key is not a failure.

## Production changes

All in `src/interactive.rs`. No other module changes.

1. **The byte stream becomes a pure function.** `frame_bytes(lines)`,
   `rewind_bytes(lines)` and `clear_bytes()` are the exact bytes the
   crossterm terminal writes. The `crossterm::execute!` calls for the cursor
   movement and the erase are replaced by the same sequences crossterm emits
   (`ESC[1G`, `ESC[nA`, `ESC[J`), so the plan is assertable without a pty.
2. **Every line starts with a carriage return.** `frame_bytes` writes
   `\r{line}\n` per line. The rewind moves to column zero and then up by the
   frame's line count, which — with the trailing newline — is the first
   painted line.
3. **The guard restores the cursor.** `RawModeGuard` becomes `TerminalGuard`
   and its `restore` issues the mode restore *and* `set_cursor_visible(true)`,
   attempting both even when the first fails. The cursor is restored on the
   commit path, the quit path, the read-error path, and an unwinding panic.
4. **The hardware cursor is hidden for the whole session.** In NOR the
   highlighted row *is* the cursor; in INS the status line draws the caret as
   an underscore beside the filter text. The cursor is shown only at the end.
5. **A read failure propagates.** `normalize_event` is split out of
   `CrosstermTerminal::read_key` so the error path is assertable, and it
   returns the error.
6. **The region is erased on every exit path.** `session_loop` splits the
   paint loop out, so the erase is one statement after the loop rather than
   one per `?` and per ending.
7. **The status line has stable fields.** The armed-exit hint's slot is
   reserved whether or not the exit is armed, so arming the exit changes
   nothing but the hint; the filter is the only thing that gives way.
8. **Width is measured in cells.** `char_width`, `display_width`, and
   cell-aware `truncate`, `truncate_end`, `pad`, `pad_left` and
   `visible_width`.

## Ordered work packages

- **W1 — the byte plan.** Extract `frame_bytes`, `rewind_bytes` and
  `clear_bytes`; point the crossterm terminal at them.
- **W2 — the cursor lifecycle.** Widen the guard; rename it; hide the
  hardware cursor for the session.
- **W3 — the error paths.** `normalize_event`; the erase on every path.
- **W4 — the status line.** The reserved hint slot.
- **W5 — the cell width.** The width table and the call sites.
- **W6 — the tests.** The in-test terminal emulator, the byte assertions,
  the cursor assertions, the width assertions.
- **W7 — the records.** This plan, the closure record, the deep dive, the
  skill, the overview findings, the registry, the roadmap, and the re-derived
  census.

## Failure, cancellation, and contention semantics

- A **read failure** ends the session with the error propagated, the region
  erased, the terminal mode restored, and the cursor shown.
- A **failed restore** is still not retried from `Drop`; the cursor restore
  is attempted anyway, because it does not depend on the mode restore.
- A **cancelled session** (`q`, `Esc` `Esc`, `Ctrl-C`) is still a success:
  exit 0, one line on stderr.
- There is **no contention** surface: the interactive session owns one
  terminal.

## Compatibility and configuration effects

None. No CLI surface, no configuration key, no environment variable, no
reason code, no exit code, and no JSON field changes. `NO_COLOR` and
`TERM=dumb` behave as before.

## Required tests

| Requirement | Test |
|---|---|
| The frame lands on the rows it claims | `the_frame_lands_on_the_rows_it_claims` |
| A repaint replaces the frame | `a_repaint_replaces_the_frame_instead_of_stacking_under_it` |
| Every line starts with a carriage return | `every_line_of_a_frame_starts_with_a_carriage_return` |
| The cursor is visible when the session ends | `the_cursor_is_visible_again_when_the_session_ends` |
| The cursor is hidden while the session runs | `the_hardware_cursor_stays_hidden_for_the_whole_session` |
| A read failure is not an unbound key | `a_read_failure_is_not_an_unbound_key` |
| The region is erased on the error path | `a_read_failure_restores_the_terminal_and_propagates` |
| The armed hint survives a long filter | `the_armed_exit_hint_survives_a_long_filter` |
| Arming the exit does not resize the filter field | `arming_the_exit_does_not_resize_the_filter_field` |
| A wide character is two cells | `a_wide_character_is_two_cells_wide` |
| A wide path does not overflow | `a_wide_character_path_does_not_overflow_the_frame` |

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

- `architecture/16-interactive.md` — the rendering contract, the cursor
  lifecycle, the byte plan, and the known gaps.
- `.skills/terminal-and-interactive/SKILL.md` — the cursor defect is
  discharged; the pty harness note is corrected.
- `architecture/overview.md` — the module sizes, the census, and the findings.
- `architecture/14-testing-and-verification.md` — the census and the
  `interactive.rs` rows.
- `AGENTS.md` and `.skills/test-evidence/SKILL.md` — the re-derived census.
- `plans/registry.md` and the subsystem roadmap — the status.

## Acceptance criteria

1. The live pty capture contains a carriage return before every line, and
   `ESC[?25h` exactly once.
2. The rendered frame occupies exactly the rows it claims, with every line
   starting at column zero.
3. The cursor is visible when the session ends, on every path.
4. A read failure propagates and does not become a busy loop.
5. The region is erased on the error path.
6. Every line of the frame is exactly the frame's visible width, in cells.
7. The ladder is green.
8. Every mutation of the fix is caught by at least one test.

## Stop conditions

Stop and record, rather than continue, if:

- the fix requires a new dependency (it does not);
- the fix would change the selection contract, the key table, the three
  endings, or any machine contract (it does not);
- the width table cannot be derived without a dependency (it can);
- a mutation is not caught by any test (none is).

## Closure evidence requirements

1. The ladder, green, with the commands actually run recorded.
2. The live pty capture, before and after, rendered.
3. The mutation table: each mutation, and the tests that caught it.
4. The re-derived census, cross-checked against the compiler's own list.
5. The known limitations, and the findings that stay open.
