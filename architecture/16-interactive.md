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
never `cargo clean` (`src/cleanup.rs:4471-4497`).

Quitting is a decision, not a failure. A cancelled session prints one line on
stderr and exits 0 (`src/main.rs:428-435`). An empty candidate set is not a
decision at all: it prints `Nothing to clean.` on stdout and exits 0
(`src/main.rs:436-444`).

## 2. The shape: one pure machine, one thin terminal

`Session` (`src/interactive.rs:204-233`) owns mode, cursor, filter, selection,
and the geometry, and renders itself to a `Vec<String>`. It performs no IO,
reads no clock, and holds no terminal. Every method is a total function of
`self` and the key pressed.

Two vocabularies sit on top of it. `Mode` (`src/interactive.rs:46-64`) is
`Normal` or `Insert`, labelled `NOR` and `INS` in the status line. `Key`
(`src/interactive.rs:68-89`) is the normalization point: the state machine
consumes it rather than `crossterm::event::KeyEvent`, and `normalize`
(`src/interactive.rs:1245-1274`) maps onto it as a *total* function: a key with
no binding becomes `Key::Ignored`, never an error. An unknown escape sequence
is a normal terminal event, not a failure. `Ctrl-C` is recognised before any
other examination, so it quits from either mode; every other control
combination is `Ignored` (`src/interactive.rs:1248-1254`).

A key press produces an `Outcome` (`src/interactive.rs:91-102`) — continue,
commit the checked roots, or quit — and the driver turns that into a
`SessionResult` (`src/interactive.rs:105-114`): the selected roots, `Cancelled`,
or `NothingToClean` for a candidate set that proved no cleanup unit at all.

Everything that touches a real terminal sits behind the `Terminal` trait
(`src/interactive.rs:120-140`), implemented once by `CrosstermTerminal`
(`src/interactive.rs:1176-1243`). The split exists so that the whole
operator-facing surface — key routing, filtering, scrolling, layout — is
assertable against a fake, including the failure that actually matters: a
session that exits without restoring the terminal.

`Session` owns the geometry as well as the state, and clamps it: width is never
below `MIN_WIDTH` (40) and height never below the five chrome lines plus one
candidate row (`src/interactive.rs:143-197`, `src/interactive.rs:281-292`). The
driver re-measures before every frame (`src/interactive.rs:1148`), so a resize
mid-session re-lays-out instead of painting over itself, and the non-key
events crossterm delivers (resize, focus, mouse, paste) need no handling of
their own — they normalize to `Key::Ignored`, and the next frame is already
measured again (`src/interactive.rs:1265`).

`render_to_string` (`src/interactive.rs:1301-1310`) flattens a frame for tests
and diagnostics; the driver uses `Session::frame` line by line.

## 3. Why the listing is not a second proof

The interactive list must show the engine's own units. A parallel copy of the
resolution pipeline would be a second ownership proof free to drift from the
first — which is the ADR 006 failure wearing a UI: a row could be offered that
the engine refuses, or a unit the engine would consider could be absent from the
list the operator chose from.

So there is exactly one pipeline. `cleanup::discover_and_resolve_universe`
(`src/cleanup.rs:1190-1281`) is called by both the destructive engine and
`cleanup::list_cleanup_candidates` (`src/cleanup.rs:2071-2146`). The extraction
exists for no other reason: it is what makes "the list is the engine's unit set"
a structural property rather than a promise.

The contract test `the_candidate_listing_is_the_engines_own_unit_set`
(`src/cleanup.rs:4328-4468`) pins it, over a fixture with deliberately mixed
ownership (`src/cleanup.rs:4277-4325`): `alpha` and `beta` own private targets
while `gamma` redirects its output into `alpha`'s, so the comparison is between
two units the engine would clean and one it would only inventory. Against a
single-workspace fixture a listing that quietly filtered candidates would still
agree with the engine, and the test would be decoration — that was the first
version of this test, and it survived its own mutation.

The same test carries the *selection* half of the contract: the listing and the
engine are handed the same `CleanupSelection::Roots`, both narrow to the same
unit, and every deselected unit is reported as `SkippedOutsideSelection`
rather than dropped (`src/cleanup.rs:4416-4467`). That is the property the TUI
depends on — what the operator sees is what the engine treats as candidates.

## 4. The safety boundary

The listing resolves candidates with the invocation's resolved selection
(`src/main.rs:568-582`): maintenance scopes offer only anchor-matching units,
so `-i` is bounded by the working directory it runs in; `clean ROOT` and
`clean --full` resolve to `CleanupSelection::All` and stay unfiltered because
their roots already bound them. The proof universe stays wide in every case —
`resolved.roots` still carry the full Routine set — so shared output stays
`Shared` (`src/main.rs:514-517`). What the operator *gets* is always narrower
than what they *saw* — `Roots` can only remove, and for maintenance it can only
remove within the anchor.

Two refusals happen before any Cargo process is spawned, in this order:

1. **Machine output formats** (`src/main.rs:543-549`). JSON and log are
   automation contracts. Carrying a terminal session in either would require
   inventing a document shape for "the operator stopped here", which is a new
   schema nobody asked for. This is checked first because it is a static property
   of the invocation — knowable without touching a stream, the same call clap
   makes by rejecting conflicting argv before anything runs.
2. **Terminal presence** (`src/main.rs:550-564`). Checked before the listing
   rather than after, because the listing is a full read-only resolution that
   spawns `cargo metadata` across every resolved root. Spending the operator's
   time and spawning processes for a run that was never going to happen would be
   a bad trade for a better error message.

The same terminal precondition is enforced twice: once by the orchestrator
(`src/main.rs:555-564`) and once inside the module's own entry point
(`src/interactive.rs:1280-1299`), so the module cannot be driven headless by a
caller that forgets to check.

Inside the module the order is the other way round, and deliberately so. An
empty candidate list is settled *before* the terminal precondition
(`src/interactive.rs:1282-1284`): there is no session to drive, so there is
nothing a terminal could be needed for, and the result is a plain negative
result rather than a refusal. That ordering is also what makes the case
testable without a pty — a precondition that can only be satisfied by a real
terminal cannot be exercised by a test process. The orchestrator's own check
still comes first (`src/main.rs:555-564`), so a redirected `-i` still refuses
before the expensive listing; what the module's ordering buys is the verdict,
not the ordering of the run.

**A blocked boundary blocks the listing; out-of-boundary poison does not.**
This is the ADR 007 rule, and it is the one place where the interactive path
changed after M014 shipped. The listing applies the identical boundary gates as
the engine (`src/cleanup.rs:2081-2128`): an in-boundary discovery diagnostic or
an in-boundary byte-capable unknown raises a block, and the orchestrator then
refuses to present a session at all (`src/main.rs:583-592`). Offering rows from
an unproven universe would be ADR 006 in a new costume: the operator would be
choosing from a list whose classification is exactly what failed. An unresolved
manifest *outside* the selected scope is reported and never blocks
(`src/cleanup.rs:2276-2314`), so a healthy anchored scope can still be chosen
from while poison exists elsewhere.

The block names the unresolved participants (bounded, human surface only) with
the same workspace-membership hint the engine's blocked report carries
(`src/cleanup.rs:1973-2020`), so the operator knows which manifest to fix or
which healthy workspace to clean explicitly with `clean ROOT` instead.

The zero-unit edge still refuses: a scope that proves no units at all while a
participant is unproven is an unproven universe, and an empty offer would read
as success (`src/cleanup.rs:2107-2125`, `src/cleanup.rs:4504-4538`).

## 5. Raw mode, and why rendering is inline

`RawModeGuard` (`src/interactive.rs:1046-1080`) restores the terminal on every
exit path. It is created before the first key is read and holds a mutable borrow
of the terminal, so restoration cannot be skipped by "using the terminal once
more" — the guard's lifetime and the driver's ability to keep driving are the
same lifetime. `Drop` restores, which covers an unwinding panic as well as
every `return` and `?` in the driver.

The restore is issued at most once. `RawModeGuard::restore` sets its `restored`
flag *before* calling through (`src/interactive.rs:1052-1069`), so a failing
restore is not retried from `Drop` — a second `disable_raw_mode` on a handle
that has already reported itself broken is not obviously safe
(`src/interactive.rs:2063-2086`).

A tool that can leave a terminal in raw mode is not shippable: the operator
loses echo and `Ctrl-C` and gets a shell they must reset by hand. The test
`the_guard_restores_when_dropped_during_unwind` (`src/interactive.rs:2035-2061`)
pins the panic path, and asserts the session was *genuinely* in raw mode when it
panicked — otherwise it would pass against a restore that never had an enter to
undo. The commit and quit paths are pinned separately
(`src/interactive.rs:1925-1957`), as is the read-failure path
(`src/interactive.rs:1996-2033`).

Rendering is **inline**. No alternate screen: the list repaints in place and the
region is erased on exit (`src/interactive.rs:1141-1174`). ADR 001 rejects raw
mode and alternate-screen rendering for *progress*, where it strands the
operator's scrollback and makes a crash unrecoverable. That rejection is not
extended to a transient selection list, which redraws in place and leaves
nothing behind. The differentiator is duration and reversibility, not the
presence of escape sequences.

C039 kept that decision and did not reopen it. The reference surface it was
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
  (`src/interactive.rs:485-499`). A toggle that obeyed the filter would make
  "all" mean "whatever happens to be on screen". The title line states the
  arithmetic (`N selected of M`) and the row carries its own mark and count
  (`x` when every candidate is checked, `-` when some are), so the scope is
  never in doubt (`src/interactive.rs:602-626`, `src/interactive.rs:687-718`).
- **`q` quits from NOR and types in INS** (`src/interactive.rs:376-406`).
  INS is a text field; every printable key inserts. `Ctrl-C` is the only key that
  quits from inside it.
- **Space is a filter character in INS** (`src/interactive.rs:408-438`).
  `normalize` maps `Char(' ')` onto `Key::Space` in every mode, so treating it as
  a no-op would make any workspace path containing a space impossible to filter
  for. `a_filter_can_contain_a_space_because_paths_can`
  (`src/interactive.rs:1473-1486`) pins it against a path that contains one.
- **Enter on an empty selection does not commit** (`src/interactive.rs:551-568`).
  Closing the session there would look exactly like one that cleaned nothing —
  the M012A shape, a run that reports a success it never performed.
- **The filter matches the full path, not the displayed name**
  (`src/interactive.rs:501-522`). A row is *named* by the shortest suffix that
  identifies it (§7), but the filter still matches the whole path, because a
  filter that only matched the displayed name would silently stop finding
  workspaces. `filtering_is_case_insensitive_and_matches_the_full_path`
  (`src/interactive.rs:1583-1608`) pins both halves.

Two more are deliberate defaults: **nothing is checked initially**, because a
selection UI whose default is "delete all of it" is a trap wearing a checkbox
(`src/interactive.rs:241-263`); and **one `Esc` never destroys a session**,
because the operator may still be reading it. Any navigation cancels a pending
exit (`src/interactive.rs:440-478`), and so does toggling a row
(`src/interactive.rs:485-499`).

Editing the filter parks the cursor on the all-toggle
(`src/interactive.rs:501-522`): carrying a row offset across a changing result
set is how a cursor ends up pointing at a different unit than the one it was on
when it was moved. The check flags live beside the candidates and are never
resized, so a filter can never renumber an existing check
(`src/interactive.rs:211-215`).

A committed selection is the checked roots in **candidate** order, not in
filtered or screen order (`src/interactive.rs:551-568`).

## 7. Rendering as a projection of ownership

### The layout contract

A frame is exactly `CHROME_LINES + page` lines — five lines of chrome plus one
line per visible row (`src/interactive.rs:143-149`, `src/interactive.rs:597`).
The chrome is the title line, the status line, the column header, the
all-toggle row, and the key-hint bar (`src/interactive.rs:570-600`):

- the title line carries the arithmetic — `N selected of M, X reclaimable`
  (`src/interactive.rs:602-626`);
- the status line carries the mode, the filter in **both** modes so the
  operator can see what is narrowing the list without entering INS, the armed
  exit, and the position indicator (`src/interactive.rs:628-665`);
- the column header names the columns — `WORKSPACE`, `RECLAIMABLE`,
  `OWNERSHIP` — so the numbers have something to be numbers of
  (`src/interactive.rs:667-685`);
- the all-toggle row carries the mark and its own count
  (`src/interactive.rs:687-718`);
- the key-hint bar names the keys, because a surface that has keys the operator
  has to remember is a surface that teaches them by refusing
  (`src/interactive.rs:803-817`).

**The page is bounded.** It holds at most `PAGE_ROWS` (15) candidate rows, and
never more than the terminal has room for (`src/interactive.rs:354-361`). The
frame therefore does not grow with the candidate count and does not grow with
the terminal: three units and three thousand paint the same shape. This is the
single most important property of the surface, and it is what C039 was written
for — the old frame filled the terminal exactly, so a long list was a screenful
to scroll rather than a list to page through.

An empty result set renders an explicit row rather than blank space
(`src/interactive.rs:581-584`), and a long name is truncated **from the left**,
because the tail of a path is the part that distinguishes one project from
another (`src/interactive.rs:956-969`). Every line is exactly the frame's
visible width (`src/interactive.rs:1026-1034`). The test
`the_frame_is_bounded_and_never_overflows_the_width`
(`src/interactive.rs:1702-1725`) pins the geometry across four sizes,
`the_frame_does_not_grow_with_the_candidate_count`
(`src/interactive.rs:1727-1754`) pins the boundedness, and
`a_long_display_name_is_truncated_from_the_left_not_the_right`
(`src/interactive.rs:1773-1786`) pins the truncation.

### A row is named, not spelled out

A row that showed the whole absolute path spent most of its width on a prefix
every row shares, and pushed the part that tells one workspace apart from
another to the far end of the line.

So a row is **named** by the shortest suffix of its path that identifies it
uniquely among all candidates (`src/interactive.rs:919-944`). The rule is a
pure function of the whole candidate set, computed once per session, so two
workspaces that share a basename are told apart — `a/x` and `b/x` — rather than
silently rendered as the same name. Two candidates with the *same* path fall
back to the whole path, because there is no suffix that distinguishes them.

This is a **display** rule only. The filter still matches the full path (§6),
and the committed selection still carries the canonical root, not the name.

### Ownership as an annotation

Each row annotates its unit's ownership class (`src/interactive.rs:897-909`). A
non-private row reads `shared — inventory only` rather than being hidden: a
hidden row is indistinguishable from a row that does not exist, and the
operator's next question would again be "which one?".

The annotation column is **omitted entirely** when every candidate is private
(`src/interactive.rs:876-880`): a column that says the same word on every row
carries no information and costs a column's width on every row. When the set
is mixed, every row carries its class, and a non-private row still reads
`inventory only`. On a terminal too narrow for the full wording, the column
degrades to the bare class label (`src/interactive.rs:833-874`).

`CleanupCandidate::provably_private` (`src/cleanup.rs:1959-1961`) is a
**presentation hint about ownership only**. It is deliberately not an eligibility
verdict — policy gates and selector capability live in the engine, and
duplicating them here is how a listing starts disagreeing with the run.

### Colour

The surface is the only place in the crate that renders colour, and the colour
is opt-out: `NO_COLOR` disables it and so does `TERM=dumb`
(`src/interactive.rs:1100-1116`). The decision is made in the driver, not in
`Session`, so the state machine stays a pure function of its own state
(`src/interactive.rs:265-269`).

The styles are: chrome dim, the cursor's row reverse-video across the full
width, a checked checkbox green, the reclaimable column cyan, and a non-private
ownership annotation yellow (`src/interactive.rs:186-197`,
`src/interactive.rs:720-784`). The escape sequences are emitted by the renderer
as part of the line, so the `Terminal` trait and its `draw(&[String])` seam are
unchanged, and a frame with colour off is byte-identical to one with it on
minus the escapes (`src/interactive.rs:2356-2372`).

## 8. Known gaps

- **No pty harness.** C023 already records that a terminal-dependent ordering —
  the progress bar being cleared before the fallible cleanup call in `main.rs` —
  is unreachable from the tests, because the bar only draws on an attended
  terminal, and that pinning it needs a pty harness. The same limit applies here.
  The raw-mode lifecycle is covered at the seam (`RawModeGuard` against a fake),
  which proves *our* code restores on every exit path — it does not prove the OS
  honours the call. The in-place redraw, cursor visibility, and resize handling
  are unverified against a real terminal for the same reason. C039 captured the
  real surface by hand over a pty; that evidence is in its closure record, not
  in the test suite.
- **The terminal cursor is not restored on exit.** The session hides the
  terminal's own cursor for its whole life and shows it only in INS, where it
  sits alongside the underscore the status line draws as the caret
  (`src/interactive.rs:1145`, `src/interactive.rs:1155`,
  `src/interactive.rs:636`), but the commit and quit paths rewind and clear
  without showing it again (`src/interactive.rs:1160-1169`). A session that ends
  in NOR therefore leaves the operator's shell with no visible cursor. The
  raw-mode guard restores the terminal *mode*; cursor visibility is a separate
  escape sequence and is not part of it. The current test asserts that the last
  recorded visibility is hidden (`src/interactive.rs:1959-1975`), so it pins the
  behaviour rather than catching it.
- **Width is counted in characters, not display cells.** `pad` and `truncate`
  count `chars()` (`src/interactive.rs:956-993`), so a path containing wide
  (CJK) characters can paint past the right edge even though the frame-length
  assertion still holds.
- **An empty candidate list is a plain negative result.** `run_interactive` settles
  an empty list before the terminal precondition (`src/interactive.rs:1282-1284`)
  and returns `SessionResult::NothingToClean`; the orchestrator maps that to
  `SelectionOutcome::NothingToClean` (`src/main.rs:603-616`) and prints one line,
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
(`src/interactive.rs:1176-1274`). The module's own production dependencies are
two: `cleanup` for the candidate type (`src/interactive.rs:38`) and `report` for
byte formatting (`src/interactive.rs:39`). Its `domain` reference is test-only
(`src/interactive.rs:1315`).

C039 added **no** dependency. The reference surface it was modelled on uses
`ratatui` and `inquire`; neither was adopted, because the layout is a fixed set
of columns and a bounded page, and a widget framework buys nothing here.
