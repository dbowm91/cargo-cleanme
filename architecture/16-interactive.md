# 16 — Interactive workspace selection

Module: `src/interactive.rs` · Added: M014 (work package D) · Decisions:
[ADR 006](../plans/adr/006-accepted-working-directory-anchored-selection.md) (selection),
[ADR 007](../plans/adr/007-accepted-scoped-proof-boundary.md) (proof boundary)

This is the only module that takes *control* of a terminal. Everything else in
the crate writes through `ProgressObserver`, `report.rs`, or a plain stream —
`progress.rs` draws a bar on stderr, but it never takes the terminal's mode,
its cursor, or its keystrokes. This one is deliberately an exception — see §5.

It is also the only surface in the crate that renders **colour**: the
deterministic human report stays plain text, because its consumers are scripts
and diffs, but a surface that owns the terminal can afford to use it (§7).

---

## 1. What it is

`cargo cleanme -i` lets an operator choose which resolved cleanup units to
clean. It presents one row per unit with its reclaimable bytes, and the choice
becomes `CleanupSelection::Roots`, which the existing engine consumes unchanged.

It is a **selection surface**, not a deletion path, and the module says so in
its own opening paragraph (`src/interactive.rs:5-14`): it resolves the cleanup
units the engine would consider, lets the operator choose among them, and
returns the chosen workspace roots as `CleanupSelection::Roots`. It never
deletes anything and it never becomes a second deletion path: the bytes it can
release are removed by exactly the same `cargo clean` spawns, under exactly the
same final proof, as a non-interactive run. Nothing here can add a candidate,
reclassify one, or unblock a scope.

That is why the selection is expressed as a filter over *already proven* units
rather than as a way to point the engine at new ones (`src/interactive.rs:12-14`).

The listing itself is read-only by contract: it spawns `cargo metadata` and
never `cargo clean` (`src/cleanup.rs:5065-5091`).

Quitting is a decision, not a failure. A cancelled session prints one line on
stderr and exits 0 (`src/main.rs:464-467`). An empty candidate set is not a
decision at all: it prints `Nothing to clean.` on stdout and exits 0
(`src/main.rs:464-472`).

## 2. The shape: one pure machine, one thin terminal

`Session` (`src/interactive.rs:220-249`) owns mode, cursor, filter, selection,
and the geometry, and renders itself to a `Vec<String>`. It performs no IO,
reads no clock, and holds no terminal. Every method is a total function of
`self` and the key pressed.

Two vocabularies sit on top of it. `Mode` (`src/interactive.rs:46-49`) is
`Normal` or `Insert`, labelled `NOR` and `INS` in the status line. `Key`
(`src/interactive.rs:68-89`) is the normalization point: the state machine
consumes it rather than `crossterm::event::KeyEvent`, and `normalize`
(`src/interactive.rs:1427-1456`) maps onto it as a *total* function: a key with
no binding becomes `Key::Ignored`, never an error. An unknown escape sequence
is a normal terminal event, not a failure. `Ctrl-C` is recognised before any
other examination, so it quits from either mode; every other control
combination is `Ignored` (`src/interactive.rs:86-92`).

A key press produces an `Outcome` (`src/interactive.rs:91-101`) — continue,
commit the checked roots, or quit — and the driver turns that into a
`SessionResult` (`src/interactive.rs:105-113`): the selected roots, `Cancelled`,
or `NothingToClean` for a candidate set that proved no cleanup unit at all.

Everything that touches a real terminal sits behind the `Terminal` trait
(`src/interactive.rs:120-149`), implemented once by `CrosstermTerminal`
(`src/interactive.rs:1360-1404`). The split exists so that the whole
operator-facing surface — key routing, filtering, scrolling, layout — is
assertable against a fake, including the failure that actually matters: a
session that exits without restoring the terminal.

`Session` owns the geometry as well as the state, and clamps it: width is never
below `MIN_WIDTH` (40) and height never below the five chrome lines plus one
candidate row (`src/interactive.rs:152-213`, `src/interactive.rs:297-308`). The
driver re-measures before every frame (`src/interactive.rs:1292`), so a resize
mid-session re-lays-out instead of painting over itself, and the non-key
events crossterm delivers (resize, focus, mouse, paste) need no handling of
their own — they normalize to `Key::Ignored`, and the next frame is already
measured again (`src/interactive.rs:1447`).

`render_to_string` (`src/interactive.rs:1483-1488`) flattens a frame for tests
and diagnostics; the driver uses `Session::frame` line by line.

## 3. Why the listing is not a second proof

The interactive list must show the engine's own units. A parallel copy of the
resolution pipeline would be a second ownership proof free to drift from the
first — which is the ADR 006 failure wearing a UI: a row could be offered that
the engine refuses, or a unit the engine would consider could be absent from the
list the operator chose from.

So there is exactly one pipeline. `cleanup::discover_and_resolve_universe`
(`src/cleanup.rs:1503-1594`) is called by both the destructive engine and
`cleanup::list_cleanup_candidates` (`src/cleanup.rs:2393-2468`). The extraction
exists for no other reason: it is what makes "the list is the engine's unit set"
a structural property rather than a promise.

The contract test `the_candidate_listing_is_the_engines_own_unit_set`
(`src/cleanup.rs:4657-4797`) pins it, over a fixture with deliberately mixed
ownership (`src/cleanup.rs:4606-4654`): `alpha` and `beta` own private targets
while `gamma` redirects its output into `alpha`'s, so the comparison is between
two units the engine would clean and one it would only inventory. Against a
single-workspace fixture a listing that quietly filtered candidates would still
agree with the engine, and the test would be decoration — that was the first
version of this test, and it survived its own mutation.

The same test carries the *selection* half of the contract: the listing and the
engine are handed the same `CleanupSelection::Roots`, both narrow to the same
unit, and every deselected unit is reported as `SkippedOutsideSelection`
rather than dropped (`src/cleanup.rs:4745-4796`). That is the property the TUI
depends on — what the operator sees is what the engine treats as candidates.

## 4. The safety boundary

The listing resolves candidates with the invocation's resolved selection
(`src/main.rs:625-636`): maintenance scopes offer only anchor-matching units,
so `-i` is bounded by the working directory it runs in; `cargo cleanme ROOT` and
`cargo cleanme --full` resolve to `CleanupSelection::All` and stay unfiltered
because their roots already bound them. The proof universe stays wide in every
case — `resolved.roots` still carry the full Routine set — so shared output
stays `Shared` (`src/main.rs:630`). What the operator *gets* is always narrower
than what they *saw* — `Roots` can only remove, and for maintenance it can only
remove within the anchor.

**M018's coverage exclusion applies to the interactive path too.** The anchor
is built with `.excluding(resolved.unproven_roots.clone())`
(`src/main.rs:625-628`), so a workspace under a learned root whose coverage
could not be proven is not offered — and the non-interactive path gets the same
narrowing (`src/main.rs:578-587`). The exclusion is applied to the *declared
selection*, after ownership classification, so it can only ever remove
candidates: a shared output between a proven root and an unproven one stays
`Shared` and stays inventory-only.

Two refusals happen before any Cargo process is spawned, in this order:

1. **Machine output formats** (`src/main.rs:594-600`). JSON and log are
   automation contracts. Carrying a terminal session in either would require
   inventing a document shape for "the operator stopped here", which is a new
   schema nobody asked for. This is checked first because it is a static property
   of the invocation — knowable without touching a stream, the same call clap
   makes by rejecting conflicting argv before anything runs.
2. **Terminal presence** (`src/main.rs:606-615`). Checked before the listing
   rather than after, because the listing is a full read-only resolution that
   spawns `cargo metadata` across every resolved root. Spending the operator's
   time and spawning processes for a run that was never going to happen would be
   a bad trade for a better error message.

The same terminal precondition is enforced twice: once by the orchestrator
(`src/main.rs:606-615`) and once inside the module's own entry point
(`src/interactive.rs:1462-1481`), so the module cannot be driven headless by a
caller that forgets to check.

Inside the module the order is the other way round, and deliberately so. An
empty candidate list is settled *before* the terminal precondition
(`src/interactive.rs:1464-1466`): there is no session to drive, so there is
nothing a terminal could be needed for, and the result is a plain negative
result rather than a refusal. That ordering is also what makes the case
testable without a pty — a precondition that can only be satisfied by a real
terminal cannot be exercised by a test process. The orchestrator's own check
still comes first (`src/main.rs:606-615`), so a redirected `-i` still refuses
before the expensive listing; what the module's ordering buys is the verdict,
not the ordering of the run.

**A blocked boundary blocks the listing; out-of-boundary poison does not.**
This is the ADR 007 rule, and it is the one place where the interactive path
changed after M014 shipped. The listing applies the identical boundary gates as
the engine (`src/cleanup.rs:2403-2450`): an in-boundary discovery diagnostic or
an in-boundary byte-capable unknown raises a block, and the orchestrator then
refuses to present a session at all (`src/main.rs:637-646`). Offering rows from
an unproven universe would be ADR 006 in a new costume: the operator would be
choosing from a list whose classification is exactly what failed. An unresolved
manifest *outside* the selected scope is reported and never blocks
(`src/cleanup.rs:2605-2643`), so a healthy anchored scope can still be chosen
from while poison exists elsewhere.

The block names the unresolved participants (bounded, human surface only) with
the same workspace-membership hint the engine's blocked report carries
(`src/cleanup.rs:2295-2342`), so the operator knows which manifest to fix or
which healthy workspace to clean explicitly with `cargo cleanme ROOT` instead.

The zero-unit edge still refuses: a scope that proves no units at all while a
participant is unproven is an unproven universe, and an empty offer would read
as success (`src/cleanup.rs:2429-2447`, `src/cleanup.rs:5098-5132`).

## 5. The terminal lifecycle, and why rendering is inline

`TerminalGuard` (`src/interactive.rs:1161-1164`) restores the terminal on every
exit path. It is created before the first key is read and holds a mutable borrow
of the terminal, so restoration cannot be skipped by "using the terminal once
more" — the guard's lifetime and the driver's ability to keep driving are the
same lifetime. `Drop` restores, which covers an unwinding panic as well as
every `return` and `?` in the driver.

**The guard restores two things, not one.** The terminal *mode* and the
terminal *cursor* are separate escape sequences: `disable_raw_mode` puts the
line discipline back and says nothing about the caret. A session that hides the
cursor therefore owns the obligation to show it again, and that obligation
belongs to the same guard — a scope, not a `?`. `TerminalGuard::restore`
(`src/interactive.rs:1166-1183`) issues the mode restore *and*
`set_cursor_visible(true)`, and attempts **both** even when the first fails,
because the second does not depend on it.

The restore is issued at most once. `TerminalGuard::restore` sets its `restored`
flag *before* calling through (`src/interactive.rs:1166-1183`), so a failing
restore is not retried from `Drop` — a second `disable_raw_mode` on a handle
that has already reported itself broken is not obviously safe
(`src/interactive.rs:2325-2365`).

A tool that can leave a terminal in raw mode is not shippable: the operator
loses echo and `Ctrl-C` and gets a shell they must reset by hand. The test
`the_guard_restores_when_dropped_during_unwind` (`src/interactive.rs:2280-2305`)
pins the panic path, and asserts the session was *genuinely* in raw mode when it
panicked — otherwise it would pass against a restore that never had an enter to
undo. The commit and quit paths are pinned separately
(`src/interactive.rs:2119-2150`), as is the read-failure path
(`src/interactive.rs:2237-2277`).

**The cursor is restored when the session ends, and it is the assertion that
matters.** `the_cursor_is_visible_again_when_the_session_ends`
(`src/interactive.rs:2163-2178`) covers the commit path, the quit path, and a
quit out of NOR after a spell in INS. This test used to assert the opposite:
it asserted that the *last* recorded cursor visibility was `Some(false)`, which
is the defect (overview finding 36) stated as a requirement. It was green
*because* it asserted the bug, and it had to be rewritten before the bug could
be fixed — otherwise the fix would have swapped one green lie for another.
`the_hardware_cursor_stays_hidden_for_the_whole_session`
(`src/interactive.rs:2193-2215`) asserts the other half, over the whole
sequence rather than its last value: the hardware cursor is hidden for the
session's life and shown once, at the end.

Rendering is **inline**. No alternate screen: the list repaints in place and the
region is erased on exit (`src/interactive.rs:1262-1280`). ADR 001 rejects raw
mode and alternate-screen rendering for *progress*, where it strands the
operator's scrollback and makes a crash unrecoverable. That rejection is not
extended to a transient selection list, which redraws in place and leaves
nothing behind. The differentiator is duration and reversibility, not the
presence of escape sequences.

The region's erase is a property of the **scope**, not of each exit path.
`session_loop` (`src/interactive.rs:1262-1280`) hands the paint loop to
`paint_frames` (`src/interactive.rs:1286-1306`) and erases once, after it, so a
read failure cannot leave a painted frame standing above the operator's
prompt.

C039 kept the inline decision and did not reopen it. The reference surface it was
modelled on (`rip`) has two modes: an inline `inquire` list, which is what the
owner pointed at, and a full-screen `ratatui` panel, which is `rip`'s
`--live` flag. The inline mode is the one that matches ADR 006 §7, and it is
the one that was adopted.

## 6. Interaction contract

| Key | Mode | Effect |
|---|---|---|
| `↑`/`k`, `↓`/`j` | NOR | Move the cursor |
| `PgUp`, `PgDn` | NOR | Move by one page of visible rows |
| `Home`, `End` | NOR | Move to the first / last candidate |
| `space` | NOR | Toggle the row under the cursor |
| `i` | NOR | Enter INS |
| `Esc` | INS | Return to NOR, keeping the filter |
| `Esc` | NOR | Arm exit; a second `Esc` exits |
| `q`, `Ctrl-C` | NOR | Exit immediately |
| `Ctrl-C` | INS | Exit immediately |
| `Enter` | either | Commit the checked roots; an empty selection does not commit |
| printable keys | INS | Append to the filter field |
| `Backspace` | INS | Delete the last filter character |
| `↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End` | INS | Ignored — INS is a one-line field |

The owner considered and declined `rip`'s own model, in which there are no
modes at all: every printable key filters, and `Space` toggles. C039 keeps the
published NOR/INS model and the published key table, and takes only the
*presentation* from `rip`. That decision has two consequences worth stating:
the mode switch survives, and a space remains typeable into the filter.

Five decisions in that table are load-bearing and were each pinned by a test
that failed first:

- **Row 0 is an all-toggle that ignores the filter**
  (`src/interactive.rs:501-515`). A toggle that obeyed the filter would make
  "all" mean "whatever happens to be on screen". The title line states the
  arithmetic (`N selected of M`) and the row carries its own mark and count
  (`x` when every candidate is checked, `-` when some are), so the scope is
  never in doubt (`src/interactive.rs:618-648`, `src/interactive.rs:722-752`).
- **`q` quits from NOR and types in INS** (`src/interactive.rs:1680-1710`).
  INS is a text field; every printable key inserts. `Ctrl-C` is the only key that
  quits from inside it.
- **Space is a filter character in INS** (`src/interactive.rs:424-454`).
  `normalize` maps `Char(' ')` onto `Key::Space` in every mode, so treating it as
  a no-op would make any workspace path containing a space impossible to filter
  for. `a_filter_can_contain_a_space_because_paths_can`
  (`src/interactive.rs:1655-1668`) pins it against a path that contains one.
- **Enter on an empty selection does not commit** (`src/interactive.rs:567-584`).
  Closing the session there would look exactly like one that cleaned nothing —
  the M012A shape, a run that reports a success it never performed.
- **The filter matches the full path, not the displayed name**
  (`src/interactive.rs:517-538`). A row is *named* by the shortest suffix that
  identifies it (§7), but the filter still matches the whole path, because a
  filter that only matched the displayed name would silently stop finding
  workspaces. `filtering_is_case_insensitive_and_matches_the_full_path`
  (`src/interactive.rs:1765-1790`) pins both halves.

Two more are deliberate defaults: **nothing is checked initially**, because a
selection UI whose default is "delete all of it" is a trap wearing a checkbox
(`src/interactive.rs:257-279`); and **one `Esc` never destroys a session**,
because the operator may still be reading it. Any navigation cancels a pending
exit (`src/interactive.rs:456-494`), and so does toggling a row
(`src/interactive.rs:501-515`).

Editing the filter parks the cursor on the all-toggle
(`src/interactive.rs:517-538`): carrying a row offset across a changing result
set is how a cursor ends up pointing at a different unit than the one it was on
when it was moved. The check flags live beside the candidates and are never
resized, so a filter can never renumber an existing check
(`src/interactive.rs:227-231`).

A committed selection is the checked roots in **candidate** order, not in
filtered or screen order (`src/interactive.rs:567-584`).

## 7. Rendering as a projection of ownership

### The layout contract

A frame is exactly `CHROME_LINES + page` lines — five lines of chrome plus one
line per visible row (`src/interactive.rs:152-158`, `src/interactive.rs:613`).
The chrome is the title line, the status line, the column header, the
all-toggle row, and the key-hint bar (`src/interactive.rs:586-616`):

- the title line carries the arithmetic — `N selected of M, X reclaimable`
  (`src/interactive.rs:618-648`);
- the status line carries the mode, the filter in **both** modes so the
  operator can see what is narrowing the list without entering INS, the armed
  exit, and the position indicator (`src/interactive.rs:663-685`);
- the column header names the columns — `WORKSPACE`, `RECLAIMABLE`,
  `OWNERSHIP` — so the numbers have something to be numbers of
  (`src/interactive.rs:702-716`);
- the all-toggle row carries the mark and its own count
  (`src/interactive.rs:722-752`);
- the key-hint bar names the keys, because a surface that has keys the operator
  has to remember is a surface that teaches them by refusing
  (`src/interactive.rs:838-851`).

**The page is bounded.** It holds at most `PAGE_ROWS` (15) candidate rows, and
never more than the terminal has room for (`src/interactive.rs:370-377`). The
frame therefore does not grow with the candidate count and does not grow with
the terminal: three units and three thousand paint the same shape. This is the
single most important property of the surface, and it is what C039 was written
for — the old frame filled the terminal exactly, so a long list was a screenful
to scroll rather than a list to page through.

An empty result set renders an explicit row rather than blank space
(`src/interactive.rs:597-600`), and a long name is truncated **from the left**,
because the tail of a path is the part that distinguishes one project from
another (`src/interactive.rs:1045-1069`). Every line is exactly the frame's
visible width (`src/interactive.rs:1137-1145`). The test
`the_frame_is_bounded_and_never_overflows_the_width`
(`src/interactive.rs:1884-1907`) pins the geometry across four sizes,
`the_frame_does_not_grow_with_the_candidate_count`
(`src/interactive.rs:1909-1936`) pins the boundedness, and
`a_long_display_name_is_truncated_from_the_left_not_the_right`
(`src/interactive.rs:1955-1967`) pins the truncation.

### The frame is written as bytes, and the bytes are the contract

**The rendered surface is a byte stream, and the byte stream is a pure
function.** `frame_bytes` (`src/interactive.rs:1323-1331`),
`rewind_bytes` (`src/interactive.rs:1346-1355`) and `clear_bytes`
(`src/interactive.rs:1334-1336`) are the exact bytes the terminal receives;
`CrosstermTerminal` (`src/interactive.rs:1360-1409`) writes them. This is the
only way the surface can be asserted without a pty: a fake that records *lines*
cannot see a byte-level defect, and a byte-level defect is exactly what a
frame has.

**Every line starts with a carriage return**, and this is not a detail.
`crossterm 0.28`'s `enable_raw_mode` calls `cfmakeraw`, which clears `OPOST`,
so a bare `\n` in raw mode is a linefeed *without* a carriage return. A frame
written with `\n` alone therefore starts every line at the column the previous
one ended in: it occupies twice the rows it claims, every line loses its
first character to the row above, and the rewind — which moves up by the
frame's *line count* rather than by the rows it actually occupied — erases
most of the frame it was meant to keep. C042 found this by capture, not by
reading: the surface had been shipping since M014 and had never been observed.

The rewind is the other half of the contract. `rewind_bytes` moves to column
zero **and then** up: `MoveUp` keeps the column, and the erase starts at the
cursor, so a cursor left at the end of a full-width line would leave the last
cell of the first line standing. `MoveUp(0)` is never emitted — `CSI 0 A`
means "up one line" to a terminal, so a zero count would move the cursor off
the frame.

The tests replay the byte stream through a small terminal emulator that models
xterm's pending-wrap rule (`src/interactive.rs:2375-2480`).
`the_frame_lands_on_the_rows_it_claims` (`src/interactive.rs:2494-2518`)
asserts that every line of a frame lands on the row it claims, at column zero,
and that nothing is painted below the frame;
`a_repaint_replaces_the_frame_instead_of_stacking_under_it`
(`src/interactive.rs:2522-2552`) asserts that a repaint replaces the frame
rather than stacking under it; and
`every_line_of_a_frame_starts_with_a_carriage_return`
(`src/interactive.rs:2556-2573`) asserts the byte-level rule directly.

### Width is measured in cells, not in characters

The frame's geometry is a promise about **terminal cells**. Measured in
characters, a path with CJK characters in it paints past the right edge,
wraps, and breaks the layout exactly as a missing carriage return does.
`char_width` (`src/interactive.rs:1001-1036`) and `display_width`
(`src/interactive.rs:1039-1041`) are the width functions, and `truncate`
(`src/interactive.rs:1045-1069`), `truncate_end`
(`src/interactive.rs:1073-1093`), `pad` (`src/interactive.rs:1095-1103`),
`pad_left` (`src/interactive.rs:1106-1115`) and `visible_width`
(`src/interactive.rs:1118-1134`) are all cell-aware.

The table is small and in-crate on purpose: the crate's minimal dependency set
is a stated property (ADR 001), and the ranges that decide the width of a
*path* are the East Asian wide and fullwidth forms, the emoji blocks, and the
zero-width marks that must not count at all.
`a_wide_character_is_two_cells_wide` (`src/interactive.rs:2631-2636`) and
`a_wide_character_path_does_not_overflow_the_frame`
(`src/interactive.rs:2640-2652`) pin it.

### The status line's fields are stable

The status line's right-hand side carries two right-anchored fields — the
armed-exit hint and the position indicator — and its left-hand side carries
the mode and the filter. **The hint's slot is reserved whether or not the exit
is armed**, so arming the exit does not resize the filter field under the
operator's eyes at the moment they are deciding whether to quit, and the
filter is the only thing on the line that gives way: it is the one of them
that can be retyped. Before C042 the hint was the *first* thing truncated
away — the filter grew into it — which left the operator pressing a key the
surface no longer explained.
`the_armed_exit_hint_survives_a_long_filter`
(`src/interactive.rs:2583-2596`) and
`arming_the_exit_does_not_resize_the_filter_field`
(`src/interactive.rs:2606-2620`) pin both halves.

### A row is named, not spelled out

A row that showed the whole absolute path spent most of its width on a prefix
every row shares, and pushed the part that tells one workspace apart from
another to the far end of the line.

So a row is **named** by the shortest suffix of its path that identifies it
uniquely among all candidates (`src/interactive.rs:954-978`). The rule is a
pure function of the whole candidate set, computed once per session, so two
workspaces that share a basename are told apart — `a/x` and `b/x` — rather than
silently rendered as the same name. Two candidates with the *same* path fall
back to the whole path, because there is no suffix that distinguishes them.

This is a **display** rule only. The filter still matches the full path (§6),
and the committed selection still carries the canonical root, not the name.

### Ownership as an annotation

Each row annotates its unit's ownership class (`src/interactive.rs:932-942`). A
non-private row reads `shared — inventory only` rather than being hidden: a
hidden row is indistinguishable from a row that does not exist, and the
operator's next question would again be "which one?".

The annotation column is **omitted entirely** when every candidate is private
(`src/interactive.rs:911-913`): a column that says the same word on every row
carries no information and costs a column's width on every row. When the set
is mixed, every row carries its class, and a non-private row still reads
`inventory only`. On a terminal too narrow for the full wording, the column
degrades to the bare class label (`src/interactive.rs:868-906`).

`CleanupCandidate::provably_private` (`src/cleanup.rs:2281-2283`) is a
**presentation hint about ownership only**. It is deliberately not an eligibility
verdict — policy gates and selector capability live in the engine, and
duplicating them here is how a listing starts disagreeing with the run.

### Colour

The surface is the only place in the crate that renders colour, and the colour
is opt-out: `NO_COLOR` disables it and so does `TERM=dumb`
(`src/interactive.rs:1221-1237`). The decision is made in the driver, not in
`Session`, so the state machine stays a pure function of its own state
(`src/interactive.rs:281-285`).

The styles are: chrome dim, the cursor's row reverse-video across the full
width, a checked checkbox green, the reclaimable column cyan, and a non-private
ownership annotation yellow (`src/interactive.rs:208-213`,
`src/interactive.rs:755-814`). The escape sequences are emitted by the renderer
as part of the line, so the `Terminal` trait and its `draw(&[String])` seam are
unchanged, and a frame with colour off is byte-identical to one with it on
minus the escapes (`src/interactive.rs:2915-2931`).

## 8. Known gaps

- **No pty harness.** C023 already records that a terminal-dependent ordering —
  the progress bar being cleared before the fallible cleanup call in `main.rs` —
  is unreachable from the tests, because the bar only draws on an attended
  terminal, and that pinning it needs a pty harness. The same limit applies here.
  The terminal lifecycle is covered at the seam (`TerminalGuard` against a
  fake), which proves *our* code restores on every exit path — it does not prove
  the OS honours the call. The in-place redraw and resize handling are
  unverified against a real terminal for the same reason. **C042 closed the
  part of this gap that could be closed without a pty**: the byte stream is
  now a pure function and the tests replay it through an in-test terminal
  emulator, so the *layout* is proved. What remains is the part only a real
  terminal can answer — that the OS honours the calls, and what a specific
  terminal does with the pending-wrap edge cases. The live pty capture in
  C042's closure record is the hand evidence for that, and it is not in the
  test suite.
- **The hardware cursor cannot be put where the operator is typing.** In INS
  the status line draws the caret as an underscore beside the filter text
  (`src/interactive.rs:671`), and the terminal's own cursor is hidden for the
  session's life (`src/interactive.rs:1269`). Before C042 it was *shown*
  in INS, at the end of the frame — a third, unrelated cursor, nowhere near
  the filter field. Positioning it correctly would need a cursor-positioning
  scheme the surface does not have, so the drawn underscore is the caret and
  the hardware cursor is restored only at the end.
- **An empty candidate list is a plain negative result.** `run_interactive` settles
  an empty list before the terminal precondition (`src/interactive.rs:1464-1466`)
  and returns `SessionResult::NothingToClean`; the orchestrator maps that to
  `SelectionOutcome::NothingToClean` (`src/main.rs:644-657`) and prints one line,
  `Nothing to clean.`, on stdout, exiting 0. The same scope without `-i` reports
  zero units and exits 0, so the two spellings of one scope agree. It was a
  usage error before C038: the refusal became `AppError::Config`, which
  exited 2.
- **No eligibility preview.** A row may be checked that policy will later skip.
  This is deliberate: duplicating the policy gates in the listing is the drift
  this module exists to avoid. The report names the reason code per row.

---

## 9. Dependency

`crossterm 0.28` is a **non-optional** dependency. Default features are kept on
purpose (`Cargo.toml:55-58`): the `windows` feature pulls the console API that
crossterm's Windows raw-mode and cursor paths call unconditionally, and trimming
it compiles on Linux while being exactly the kind of platform-only gap this
repository has been bitten by before (C020, C021, C024). The `bracketed-paste`
that arrives with it is inert — nothing here enables paste reporting
(`Cargo.toml:59-60`).

Cost is recorded honestly: this adds a dependency to a crate whose minimal set is
a stated property. ADR 001 rejected `ratatui`/`crossterm` for *transient
progress*, which is a different requirement and did not exist when it was
written (`Cargo.toml:62-64`).

The dependency is used in this module and nowhere else in the crate
(`src/interactive.rs:1360-1421`). The module's own production dependencies are
two: `cleanup` for the candidate type (`src/interactive.rs:38`) and `report` for
byte formatting (`src/interactive.rs:39`). Its `domain` reference is test-only
(`src/interactive.rs:1497`).

C039 added **no** dependency. The reference surface it was modelled on uses
`ratatui` and `inquire`; neither was adopted, because the layout is a fixed set
of columns and a bounded page, and a widget framework buys nothing here.
