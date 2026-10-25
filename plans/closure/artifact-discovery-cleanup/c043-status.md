# C043 — the interactive frame's geometry — closure record

Plan: `plans/implementation/artifact-discovery-cleanup/c043-interactive-frame-geometry-corrective.md`
Status: **closed — shipped in v0.2.11** (tag `v0.2.11` at `1d679833ae62`, GitHub release published 2026-10-25T17:23:10Z and immutable with a valid attestation, crates.io sha256 `96298f83…`, automatic smoke `40216211513` green on five targets). Receipt:
`plans/closure/distribution-release-update/r031-status.md`.
Commits: `30bcff5` (the fix), `05f8cd7` (the documents).
Baseline: `main` at `1d5f1e8` (0.2.10, unpublished).
Date: 2026-10-22.

## The executive finding

C042 made the interactive frame's **bytes** correct. It did not make the
frame's **geometry** correct, and the two are independent: a frame can be
written perfectly — every line starting with a carriage return, every
character intact — and still be the wrong shape for the terminal it lands in.

A live capture over a real pty on 2026-10-22 showed both axes wrong at once.

**Width.** The frame's width was `max(MIN_WIDTH, terminal_width)` — clamped
**up** to forty cells. In a terminal narrower than forty columns the frame was
wider than the terminal it was painted on: every line wrapped, the frame
occupied twice the rows it claimed, and the erase — which moves up by the
frame's *line count*, not by the rows it occupies — left the mangled frame
standing above the operator's prompt. **The surface was not cleaned up.**

**Height.** The frame's height was bounded in *lines* against the terminal's
height, but the frame *occupies* one row more than its line count: every line
ends with a newline, and the last one moves the cursor onto the row below. A
frame whose lines equalled the terminal's height therefore **scrolled** the
terminal, pushing its own title line — the arithmetic `N selected of M, X
reclaimable` — off the top in any terminal shorter than ten rows, and pushing
the operator's scrollback up with it.

The reason no test caught either is the same shape C042 found, in a new place.
`the_frame_is_bounded_and_never_overflows_the_width` asserted that the frame
fitted `max(w, MIN_WIDTH)` and `max(h, CHROME_LINES + MIN_LIST_ROWS)`: it was
green *because* it blessed the clamp. And
`the_page_is_capped_by_the_terminal_height` asserted `frame.len() <= h` — the
*line count*, one row short of the property that mattered.

## What changed

Two defects, both rendering. Neither touches a proof, a scope, a candidate, or
a deletion path.

| # | Defect | Fix |
|---|---|---|
| D1 | The frame's width was clamped **up** to forty cells, so in a narrower terminal every line wrapped and the erase — which counts lines, not rows — left the frame standing above the prompt. | The frame adopts the terminal's width exactly, and the layout degrades so that it fits: the reclaimable column goes first, then the ownership column's wording, and the name keeps what is left. |
| D2 | The frame's height was bounded in *lines* against the terminal's height, but the frame *occupies* one row more than its line count, so the trailing newline scrolled the terminal and pushed the title line off the top. | The bound is on the rows occupied. The chrome gives way in the order of what it costs the operator least — the key-hint bar, then the column header, then the all-toggle row. |

A third change is a consequence rather than a defect: a terminal smaller than
twenty columns or six rows is now **refused** with an actionable message,
before the listing spawns anything. The layout fits such a terminal, but the
surface cannot be *used* in it.

The selection contract, the published key table, the three endings, the exit
codes, the reason codes, the JSON envelope and the configuration are all
unchanged. **No dependency was added.**

## The requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| The frame fits the terminal in both axes | `the_frame_fits_the_terminal_in_both_axes` — ten sizes, width and rows occupied |
| The erase removes every row the frame occupied | `the_erase_removes_every_row_the_frame_occupied` — replays the byte stream through the in-test terminal emulator |
| A terminal too small for the list is refused | `a_terminal_too_small_for_the_list_is_refused` |
| A narrow terminal degrades the columns and keeps the title | `a_narrow_terminal_degrades_the_columns_and_keeps_the_title` |
| The page is capped by the terminal's height | `the_page_is_capped_by_the_terminal_height` — rewritten to count rows occupied |
| The frame's bytes are still correct | `the_frame_lands_on_the_rows_it_claims`, `every_line_of_a_frame_starts_with_a_carriage_return`, and the eleven other C042 tests |

## The mutation evidence

Eight mutations, one per property the fix establishes. Each puts the defect
back and runs the interactive suite. **A mutation that leaves the suite green
is a test that is not testing anything**, so this is the evidence that matters
for the new tests.

| Mutation | Caught by |
|---|---|
| M1 — the frame's width is clamped up to forty cells again | `the_frame_fits_the_terminal_in_both_axes`, `the_erase_removes_every_row_the_frame_occupied`, `a_narrow_terminal_degrades_the_columns_and_keeps_the_title` |
| M2 — the height bound forgets the row the trailing newline takes | `the_frame_fits_the_terminal_in_both_axes`, `the_erase_removes_every_row_the_frame_occupied`, `the_page_is_capped_by_the_terminal_height`, and four scrolling/paging tests |
| M3 — the chrome ladder never drops the key-hint bar | `the_frame_fits_the_terminal_in_both_axes`, `the_erase_removes_every_row_the_frame_occupied`, `the_page_is_capped_by_the_terminal_height` |
| M4 — the ownership annotation is not truncated to its column | `the_erase_removes_every_row_the_frame_occupied` |
| M5 — the all-toggle row is not truncated to the frame's width | `the_frame_fits_the_terminal_in_both_axes`, `the_erase_removes_every_row_the_frame_occupied` |
| M6 — the geometry gate accepts every terminal | `a_terminal_too_small_for_the_list_is_refused` |
| M7 — the reclaimable column is never dropped | `the_frame_fits_the_terminal_in_both_axes`, `the_erase_removes_every_row_the_frame_occupied`, `a_narrow_terminal_degrades_the_columns_and_keeps_the_title` |
| M8 — the all-toggle row is never dropped from a very short terminal | `the_frame_fits_the_terminal_in_both_axes` |

**Eight of eight caught**, on the first run.

## The live evidence

A **real pty**, not a string comparison: `pty.fork`, the slave in raw mode
(`OPOST` cleared, so a bare `\n` is *not* translated), the real binary, a
four-project fixture whose artifacts are backdated 60 days so the recency
guard does not skip them. The capture is replayed through a terminal emulator
that models xterm's pending-wrap rule.

**Before — the width axis**, a 24-row, 39-column terminal:

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

**Before — the height axis**, an eight-row terminal:

```
|NOR  filter: <none>                                       —/4|  <- row 0
|      WORKSPACE                                 RECLAIMABLE|  <- row 1
|> [ ] all                                (0/4 selected)    |  <- row 2
|  [ ] alpha                                 300.00 KiB    |  <- row 3
|  [ ] beta                                  300.00 KiB    |  <- row 4
|v [ ] delta                                 300.00 KiB    |  <- row 5
|[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]|  <- row 6
```

The title line is gone before the operator can read it. At five rows the
title *and* the status line were gone; at four rows the column header too; at
three rows only one candidate row and the key-hint bar survived.

**After**, the same 39 columns:

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
the chrome gives way and the title survives on row 0. At 100 columns the frame
is unchanged from C042.

**A terminal below 20x6 is refused**, with the terminal never taken over:

```
cargo-cleanme: configuration error: interactive selection requires a usable
terminal (a terminal of 100x5 is too small for the interactive list; it needs
at least 20x6); drop -i to clean without choosing interactively, or use
`cargo cleanme scan --all` to inspect the same units read-only
```

## The output-discipline review that found it

The same pass reviewed the `-i` surface's **output** and the rest of the
CLI's, against the question the owner asked: *is the information printed
minimally the actions actually taken, rather than a dump of the actions not
taken?*

**While the surface is active, nothing is printed.** The listing runs with a
silent progress renderer, `cargo metadata`'s output is captured rather than
inherited, and the frame is the only thing on the screen. Verified in the
capture: the byte stream between the first frame and the erase contains
nothing but the frame.

**After the operator commits, the region is erased and the report is one
line per cleaned directory plus one tally.** The live capture of a committed
run, in a 30×100 terminal:

```
|Cleaned  alpha/target  448.00 KiB    |  <- row 0
|Cleaned  beta/target   448.00 KiB    |  <- row 1
|Cleaned  gamma/target  448.00 KiB    |  <- row 2
|1.31 MiB reclaimed from 3 directories|  <- row 4
```

The progress bar's line is cleared before the report, and the tally is a
single line. The same shape holds for a failure — `900.00 KiB reclaimed from
3 directories (1 failed)` — so a failure is named rather than swallowed.

**The rest of the CLI was reviewed and needs no change.** The quiet cleanup
surface prints one row per *acted-on* directory and counts the skips in the
tally; the verbose surface is opt-in; the scan surface prints the group
inventory with each group's ownership class, and the groups a cleanup would
*not* act on are labelled, not hidden; the `--stats` line is one line; the log
format is one line; the JSON format is one envelope. The omitted-root and
unproven-root disclosures on stderr are bounded, and they are safety
disclosures rather than dumps of work not done.

## The census re-derivation

Derived, never copied. The **module-size** method reproduces all 17 untouched
modules' published figures exactly, and the **census** method is cross-checked
against the compiler's own list: `cargo test --lib -- --list` = 523 and
`cargo test --bin cargo-cleanme -- --list` = 9, so Linux must be 532 — and
the parser says 532.

| | before | after |
|---|---|---|
| declared `#[test]` functions in `src/` | 531 | **534** |
| Linux x86_64 | 529 | **532** |
| Windows | 487 | **490** |
| macOS | 526 | **529** |
| exactly-`#[cfg(unix)]` gates | 40 | 40 |
| of which a naive grep finds | 30 | 30 |

The census is therefore **534 declared / 532 Linux / 490 Windows / 529
macOS**; each platform gained exactly the three tests C043 added.

`src/interactive.rs` grew from 2985 to 3310 lines, and its inline tests from
67 to 70. `src/main.rs` grew 2244 → 2258. `src/` grew 38,302 → 38,641 lines,
of which production grew 17,450 → 17,695.

## The verification actually run

On the committed tree (`30bcff5`, `05f8cd7`):

```sh
cargo fmt --all -- --check                                     # clean
cargo clippy --all-targets --all-features -- -D warnings       # clean
cargo test --all-targets --all-features                        # 523 lib / 9 bin / 77 cli_contract / 4 end_to_end = 613, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py   # 37 fixture files clean
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py                # 17 documents clean
cargo run --quiet --features dev-tools --bin generate-docs -- --check                                      # 13 artifacts match
```

**Not run:** `scripts/release-check.sh` (24 steps; needs a clean tree, the
`eggpack` binary, and toolchain 1.89). The MSRV lane and the hosted
Linux/macOS/Windows lanes were **not** run: this corrective touches the
terminal surface, which is exactly the area the hosted lanes exist for, so
**the platform-specific behaviour is unverified on Windows and macOS**. The
escape sequences are unchanged from C042, so the risk is low, but "low" is not
"verified".

## The citation re-derivation

149 live citations point at `src/interactive.rs`, and every one of them
moved.

They were **re-derived, not shifted**: the bulk through a diff-based mapping
in which every mapping is between two lines whose **text is identical**
(`difflib`'s opcodes over the line lists, with the equality asserted), and
the eighteen anchors the mapping could not resolve — because the line they
named had been rewritten — by hand, reading the new line and checking it
against the sentence that cites it. A citation whose old line was rewritten
was left alone and reported for hand re-derivation rather than guessed.

The `check-doc-citations.py` gate covers the `file.rs:N` form; the bare `:N`
form (the dominant convention in the deep dives) is not machine-gated, so
every cited line was printed and read.

## Known limitations

- **The pty harness still does not exist** (finding 48). C043 narrowed it
  further: the frame's *geometry* is now asserted against the terminal it is
  painted on, and the erase is asserted to leave nothing standing, both
  replayed through the in-test terminal emulator. What remains is the part
  only a real terminal can answer — that the OS honours the calls, and how a
  given terminal handles the pending-wrap edge cases. The live captures above
  are the hand evidence for that, and they are **not** in the test suite.
- **The width table is approximate at the edges.** It covers the East Asian
  wide and fullwidth forms, the emoji blocks, and the zero-width marks. Emoji
  presentation is genuinely ambiguous across terminals.
- **The hardware cursor cannot be put where the operator is typing.** The
  drawn underscore is the caret; positioning the real cursor there would need
  a cursor-positioning scheme the surface does not have.
- **A very short terminal loses the all-toggle row.** Below four rows the
  frame is the title and the status line alone, so the cursor can point at a
  row that is not drawn. Such a terminal is refused by the session gate, so
  the case is unreachable in use; it is the layout's floor, not its shape.
- **The hosted lanes have not run** for this tree.

## The unresolved findings, by severity

- **Finding 48 — no pty harness — stays open**, narrowed twice over. See
  above.
- **Finding 67 is closed by this corrective** (the frame's geometry).
- **Finding 66 is closed by C042** (the staircase).
- **Finding 60 is closed by C039** (the list that filled the screen).
- No new finding was opened.

## The disposition

**Closing.** Implemented, ladder-green, evidenced by a live pty capture before
and after and by eight mutations that are all caught.

It stays **open**, not closed, for the ordinary reason: nothing is published.
Publication is owner-gated, and this record does not ask for it. The
corrective may ride with the next release, or be held.

## Reproduction

```sh
cd /home/sugarwookie/projects/cargo-cleanme

# The fix's own tests.
cargo test --all-features --lib interactive

# The mutation table: each mutation must be caught.
python3 /tmp/tui/mutate_c043.py     # the harness is scratch; the mutations are
                                    # described in the table above
```

The live capture needs a pty harness, which this repository does not have.
The harness used here was scratch: a `pty.fork` child, the slave in raw
mode, and a small terminal emulator that models xterm's pending-wrap rule.
The *reason* it has to be raw is the finding itself — a translating pty would
have shown the frame as correct.
