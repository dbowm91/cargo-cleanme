# C042 — the interactive surface's rendering and cursor lifecycle — closure record

Plan: `plans/implementation/artifact-discovery-cleanup/c042-interactive-surface-rendering-corrective.md`
Status: **closed — shipped in v0.2.11** (tag `v0.2.11` at `1d679833ae62`, GitHub release published 2026-10-25T17:23:10Z and immutable with a valid attestation, crates.io sha256 `96298f83…`, automatic smoke `40216211513` green on five targets). Receipt:
`plans/closure/distribution-release-update/r031-status.md`.
Commits: `c74f3f2` (the fix), `445fcaf` (the documents).
Baseline: `main` at `de9628d` (0.2.10, unpublished).
Date: 2026-10-22.

## The executive finding

The interactive selection surface had **never been observed by an automated
test**, and it did not render what it drew.

`crossterm 0.28`'s `enable_raw_mode` calls `cfmakeraw`, which clears
`OPOST`. In raw mode a bare `\n` is therefore a linefeed **without** a
carriage return. The frame was written with `writeln!`, so every line started
at the column the previous one ended in. The frame occupied **twice** the rows
it claimed, every line lost its first character to the row above, and the
rewind — which moved up by the frame's *line count* rather than by the rows it
actually occupied — erased most of the frame it was meant to keep.

On a 30-row terminal with four candidates, the operator saw **one** candidate,
a key-hint bar with its leading bracket gone, and a status line reading
`OR  filter: <none>` instead of `NOR  filter: <none>`.

The same capture showed `ESC[?25l` (Hide) twice and `ESC[?25h` (Show)
**zero** times, so a session that ended in NOR — the default mode, and the mode
you are in when you press `q` — left the operator's shell with **no visible
caret**. That is overview finding **36**, open since C039.

The reason no test caught either is structural, not accidental: the `Terminal`
seam records *lines*, not bytes. A fake cannot see a byte-level defect, and a
frame is bytes. C039's mutation evidence mutated the frame's *shape* — the
number of lines — which a line-count assertion catches; it never mutated a
`\r` into a `\n`, which nothing in the suite could catch.

## What changed

Seven defects, all rendering or lifecycle. None touches a proof, a scope, a
candidate, or a deletion path.

| # | Defect | Fix |
|---|---|---|
| D1 | The frame was written with a bare newline — a linefeed without a carriage return — so it rendered as a staircase and the rewind erased most of it. | `frame_bytes` writes `\r{line}\n` per line; `rewind_bytes` moves to column zero and then up. |
| D2 | The terminal cursor was never restored. | `TerminalGuard::restore` issues the mode restore **and** `set_cursor_visible(true)`, attempting both even when the first fails. |
| D3 | The hardware cursor was shown in INS, at the end of the frame — nowhere near the filter field. | The hardware cursor is hidden for the session's whole life; the drawn underscore is the caret. |
| D4 | A terminal read failure was swallowed as an unbound key, which turns a vanished terminal into a busy repaint loop. | `normalize_event` returns the error. |
| D5 | The painted region was left standing when the session ended in an error. | The erase is one statement after the paint loop, not one per `?` and per ending. |
| D6 | The armed-exit hint was truncated away by the filter, and the filter field was resized when the exit was armed. | The hint's slot is reserved whether or not the exit is armed; the filter is the only thing that gives way. |
| D7 | The frame's geometry was measured in `chars()`, not terminal **cells**, so a CJK path painted past the right edge. | `char_width` / `display_width`, and cell-aware `truncate`, `truncate_end`, `pad`, `pad_left`, `visible_width`. |

The selection contract, the published key table, the three endings, the exit
codes, the reason codes, the JSON envelope and the configuration are all
unchanged. **No dependency was added.**

## The requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| The frame lands on the rows it claims | `the_frame_lands_on_the_rows_it_claims` — replays the byte stream through the in-test terminal emulator |
| A repaint replaces the frame | `a_repaint_replaces_the_frame_instead_of_stacking_under_it` |
| Every line starts with a carriage return | `every_line_of_a_frame_starts_with_a_carriage_return` |
| The cursor is visible when the session ends | `the_cursor_is_visible_again_when_the_session_ends` — commit path, quit path, and a quit out of NOR after INS |
| The cursor is hidden while the session runs | `the_hardware_cursor_stays_hidden_for_the_whole_session` — asserts the whole sequence, not its last value |
| A read failure is not an unbound key | `a_read_failure_is_not_an_unbound_key` |
| The region is erased on the error path | `a_read_failure_restores_the_terminal_and_propagates` |
| The armed hint survives a long filter | `the_armed_exit_hint_survives_a_long_filter` |
| Arming the exit does not resize the filter field | `arming_the_exit_does_not_resize_the_filter_field` |
| A wide character is two cells | `a_wide_character_is_two_cells_wide` |
| A wide path does not overflow | `a_wide_character_path_does_not_overflow_the_frame` |
| The width is a promise about cells | `the_frame_is_bounded_and_never_overflows_the_width` (now cell-based) |

## The mutation evidence

Seven mutations, one per defect. Each puts the defect back and runs the
interactive suite. **A mutation that leaves the suite green is a test that is
not testing anything**, so this is the evidence that matters for the new
tests.

| Mutation | Caught by |
|---|---|
| M1 — the frame is written with a bare newline again | `the_frame_lands_on_the_rows_it_claims`, `a_repaint_replaces_the_frame_instead_of_stacking_under_it`, `every_line_of_a_frame_starts_with_a_carriage_return` |
| M2 — the guard restores the mode but not the cursor | `the_cursor_is_visible_again_when_the_session_ends`, `the_hardware_cursor_stays_hidden_for_the_whole_session` |
| M3 — the width is measured in characters again | `a_wide_character_is_two_cells_wide`, `a_wide_character_path_does_not_overflow_the_frame` |
| M4 — a read failure is swallowed again | `a_read_failure_is_not_an_unbound_key` |
| M5 — the armed-exit hint is truncated away again | `arming_the_exit_does_not_resize_the_filter_field` |
| M6 — the painted region is left standing on the error path | `a_read_failure_restores_the_terminal_and_propagates`, and the two layout tests |
| M7 — the hardware cursor is shown during the session | `the_hardware_cursor_stays_hidden_for_the_whole_session` |

**Seven of seven caught.** The first run of M5 was *not* caught: the
filter-field test compared the filter's start column, which does not move
when the slot is not reserved. It was rewritten to compare the whole status
line with the hint blanked out, which does. A mutation table is only as good
as the mutations in it.

## The live evidence

A **real pty**, not a string comparison: `pty.fork`, a 100×30 window, the
slave in raw mode (`OPOST` cleared, so a bare `\n` is *not* translated), the
real binary, a four-project fixture whose artifacts are backdated 60 days so
the recency guard does not skip them.

The capture is replayed through a terminal emulator that models xterm's
pending-wrap rule — a character written in the last column leaves the cursor
there until the next printable character wraps it.

**Before** (`c74f3f2^`): 1,103 bytes, **1** carriage return, 10 newlines,
`ESC[?25l` × 2, `ESC[?25h` × **0**.

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

Nine lines of frame over seventeen rows. The `MoveUp(9)` that erases the old
frame lands in the middle of it, so the bottom half of the frame — `beta`,
`delta`, `gamma` and the hint bar — is erased by the very rewind that was
meant to keep it.

**After** (`c74f3f2`): 1,116 bytes, **10** carriage returns for 10 newlines,
`ESC[?25l` × 1, `ESC[?25h` × 1.

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

Nine rows, nine lines, every line at column zero, every character intact, and
the cursor restored.

The control experiment matters: the same harness, with the slave in raw mode,
shows that a bare `\n` arrives as a bare `\n` (`AAA\nBBB\n`, no carriage
return), so the capture is not an artefact of the harness.

## The census re-derivation

Derived, never copied. Two parsers, both validated:

- the **module-size** method reproduces all 17 untouched modules' published
  figures exactly (after the string-literal bug in the first attempt was
  fixed: `config.rs` has a fixture string with an unbalanced brace, and a naive
  brace count puts the end of the test scope in the wrong place);
- the **census** method is cross-checked against the compiler's own list:
  `cargo test --lib -- --list` = 520 and `cargo test --bin cargo-cleanme -- --list`
  = 9, so Linux must be 529 — and the parser says 529.

| | before | after |
|---|---|---|
| declared `#[test]` functions in `src/` | 522 | **531** |
| Linux x86_64 | 520 | **529** |
| Windows | 478 | **487** |
| macOS | 517 | **526** |
| exactly-`#[cfg(unix)]` gates | 40 | 40 |
| of which a naive grep finds | 30 | 30 |

The census is therefore **531 declared / 529 Linux / 487 Windows / 526
macOS**; each platform gained exactly the nine tests C042 added.

The naive-grep figure was re-derived rather than carried forward: a grep for
`#[cfg(unix)]` *immediately above* `#[test]` finds **30** of the 40, because
Rust allows the gate to come *after* the test attribute (ten do — for example
`src/cleanup.rs:7072`). The skill's stale "26, not 36" was corrected.

`src/interactive.rs` grew from 2426 to 2985 lines, and its inline tests from
58 to 67.

## The verification actually run

On the committed tree (`c74f3f2`, `445fcaf`):

```sh
cargo fmt --all -- --check                                     # clean
cargo clippy --all-targets --all-features -- -D warnings       # clean
cargo test --all-targets --all-features                        # 520 lib / 9 bin / 77 cli_contract / 4 end_to_end = 610, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py   # 37 fixture files clean
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py                # 17 documents clean
cargo run --quiet --features dev-tools --bin generate-docs -- --check                                      # 13 artifacts match
```

**Not run:** `scripts/release-check.sh` (24 steps; needs a clean tree, the
`eggpack` binary, and toolchain 1.89). The MSRV lane and the hosted
Linux/macOS/Windows lanes were **not** run: this corrective touches the
terminal surface, which is exactly the area the hosted lanes exist for, so
**the platform-specific behaviour of the new escape sequences is unverified
on Windows and macOS**. The sequences are the ones crossterm already emitted
(`ESC[1G`, `ESC[nA`, `ESC[J`), so the risk is low, but "low" is not
"verified".

## The citation re-derivation

128 citations pointed at `src/interactive.rs` and every one of them moved.

They were **re-derived, not shifted**: the bulk through a diff-based mapping
in which every mapping is between two lines whose **text is identical**
(`difflib`'s opcodes over the line lists, with the equality asserted), and the
changed anchors — the guard, the driver, the rendering functions, the byte
plans, the new tests — by hand, reading the new line and checking it against
the sentence that cites it. A citation whose old line was rewritten was left
alone and reported for hand re-derivation rather than guessed.

The `check-doc-citations.py` gate covers the `file.rs:N` form; the bare `:N`
form (the dominant convention in the deep dives) is not machine-gated, so it
was verified by printing every cited line and reading it.

## Known limitations

- **The pty harness still does not exist** (finding 48). C042 narrowed the
  limit: the *layout* is now proved without a pty, because the byte stream is
  a pure function replayed through an in-test terminal emulator. What remains
  is the part only a real terminal can answer — that the OS honours the
  calls, and how a given terminal handles the pending-wrap edge cases. The
  live capture above is the hand evidence for that, and it is **not** in the
  test suite.
- **The width table is approximate at the edges.** It covers the East Asian
  wide and fullwidth forms, the emoji blocks, and the zero-width marks. Emoji
  presentation is genuinely ambiguous across terminals; a family emoji
  rendered as a text glyph is one cell here and two in some terminals.
- **The hardware cursor cannot be put where the operator is typing.** The
  drawn underscore is the caret; positioning the real cursor there would need
  a cursor-positioning scheme the surface does not have.
- **The hosted lanes have not run** for this tree.

## The unresolved findings, by severity

- **Finding 48 — no pty harness — stays open**, narrowed. See above.
- **Finding 66 is closed by this corrective** (the staircase).
- **Finding 36 is closed by this corrective** (the cursor).
- The width-in-characters gap is **closed** by this corrective.
- No new finding was opened.

## The disposition

**Closing.** Implemented, ladder-green, evidenced by a live pty capture and by
seven mutations that are all caught.

It stays **open**, not closed, for the ordinary reason: nothing is published.
Publication is owner-gated, and this record does not ask for it. The
corrective may ride with the next release, or be held.

## Reproduction

```sh
cd /home/sugarwookie/projects/cargo-cleanme

# The fix's own tests.
cargo test --all-features --lib interactive

# The mutation table: each mutation must be caught.
python3 /tmp/tui/mutate.py        # the harness is scratch; the mutations are
                                  # described in the table above
```

The live capture needs a pty harness, which this repository does not have.
The harness used here was scratch: a `pty.fork` child, a 100×30 window, the
slave in raw mode, and a small terminal emulator that models xterm's
pending-wrap rule. The *reason* it has to be raw is the finding itself — a
translating pty would have shown the frame as correct.
