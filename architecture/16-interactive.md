# 16 — Interactive workspace selection

Module: `src/interactive.rs` · Added: M014 (work package D) · Decision:
[ADR 006](../plans/adr/006-accepted-working-directory-anchored-selection.md)

This is the only module that touches a terminal directly. Everything else in the
crate writes through `ProgressObserver`, `report.rs`, or a plain stream, and this
one is deliberately an exception — see §5.

---

## 1. What it is

`cargo cleanme -i` lets an operator choose which resolved cleanup units to
clean. It presents one row per unit with its reclaimable bytes, and the choice
becomes `CleanupSelection::Roots`, which the existing engine consumes unchanged.

It is a **selection surface**, not a deletion path. The bytes it can release are
removed by exactly the same `cargo clean` spawns, under exactly the same final
proof, as a non-interactive run. Nothing here can add a candidate, reclassify
one, or unblock a scope.

## 2. The shape: one pure machine, one thin terminal

`Session` (`src/interactive.rs:142-160`) owns mode, cursor, filter, and
selection, and renders itself to a `Vec<String>`. It performs no IO, reads no
clock, and holds no terminal. Every method is a total function of `self` and the
key pressed.

Everything that touches a real terminal sits behind the `Terminal` trait
(`src/interactive.rs:106-126`), implemented once by `CrosstermTerminal`
(`src/interactive.rs:675-738`). The split exists so that the whole
operator-facing surface — key routing, filtering, scrolling, layout — is
assertable against a fake, including the failure that actually matters: a session
that exits without restoring the terminal.

`Key` (`src/interactive.rs:67-78`) is the normalization point. The state machine
consumes it rather than `crossterm::event::KeyEvent`, and `normalize`
(`src/interactive.rs:744-762`) maps onto it as a *total* function: a key with no
binding becomes `Key::Ignored`, never an error. An unknown escape sequence is a
normal terminal event, not a failure.

## 3. Why the listing is not a second proof

The interactive list must show the engine's own units. A parallel copy of the
resolution pipeline would be a second ownership proof free to drift from the
first — which is the ADR 006 failure wearing a UI: a row could be offered that
the engine refuses, or a unit the engine would consider could be absent from the
list the operator chose from.

So there is exactly one pipeline. `cleanup::discover_and_resolve_universe`
(`src/cleanup.rs:1096-1177`) is called by both the destructive engine and
`cleanup::list_cleanup_candidates` (`src/cleanup.rs:1832-1885`). The extraction
exists for no other reason: it is what makes "the list is the engine's unit set"
a structural property rather than a promise.

The contract test `the_candidate_listing_is_the_engines_own_unit_set`
(`src/cleanup.rs:3863-4003`) pins it, over a fixture with deliberately mixed
ownership (`src/cleanup.rs:3812-3860`): `alpha` and `beta` own private targets
while `gamma` redirects its output into `alpha`'s, so the comparison is between
two units the engine would clean and one it would only inventory. Against a
single-workspace fixture a listing that quietly filtered candidates would still
agree with the engine, and the test would be decoration — that was the first
version of this test, and it survived its own mutation.

## 4. The safety boundary

The listing resolves candidates with `CleanupSelection::All`
(`src/main.rs:553-553`), deliberately offering every unit in scope. The operator
is choosing; a list that hid the units their anchor excluded would just move
the surprise into the report. What the operator *gets* is always narrower than
what they *saw* — `Roots` can only remove.

Two refusals happen before any Cargo process is spawned
(`src/main.rs:510-512`), in this order:

1. **Machine output formats** (`src/main.rs:519-525`). JSON and log are
   automation contracts. Carrying a terminal session in either would require
   inventing a document shape for "the operator stopped here", which is a new
   schema nobody asked for. This is checked first because it is a static property
   of the invocation — knowable without touching a stream, the same call clap
   makes by rejecting conflicting argv before anything runs.
2. **Terminal presence** (`src/main.rs:531-539`). Checked before the listing
   rather than after, because the listing is a full read-only resolution that
   spawns `cargo metadata` across every resolved root. Spending the operator's
   time and spawning processes for a run that was never going to happen would be
   a bad trade for a better error message.

An unproven universe blocks the listing outright rather than offering rows
(`src/main.rs:555-563`). Offering a selection from a universe whose ownership
could not be proven would be ADR 006 in a new costume: the operator would be
choosing from a list whose classification is exactly what failed.

## 5. Raw mode, and why rendering is inline

`RawModeGuard` (`src/interactive.rs:565-589`) restores the terminal on every exit
path. It is created before the first key is read and holds a mutable borrow of
the terminal, so restoration cannot be skipped by "using the terminal once more"
— the guard's lifetime and the driver's ability to keep driving are the same
lifetime. `Drop` restores, which covers an unwinding panic as well as every
`return` and `?` in the driver.

A tool that can leave a terminal in raw mode is not shippable: the operator
loses echo and `Ctrl-C` and gets a shell they must reset by hand. The test
`the_guard_restores_when_dropped_during_unwind` (`src/interactive.rs:1442-1467`)
pins the panic path, and asserts the session was *genuinely* in raw mode when it
panicked — otherwise it would pass against a restore that never had an enter to
undo.

Rendering is **inline**. No alternate screen: the list repaints in place and the
region is erased on exit (`src/interactive.rs:640-670`). ADR 001 rejects raw mode
and alternate-screen rendering for *progress*, where it strands the operator's
scrollback and makes a crash unrecoverable. That rejection is not extended to a
transient selection list, which redraws in place and leaves nothing behind. The
differentiator is duration and reversibility, not the presence of escape
sequences.

## 6. Interaction contract

| Key | Mode | Effect |
|---|---|---|
| `↑`/`k`, `↓`/`j` | NOR | Move the cursor |
| `space` | NOR | Toggle the row under the cursor |
| `i` | NOR | Enter INS |
| `Esc` | INS | Return to NOR, keeping the filter |
| `Esc` | NOR | Arm exit; a second `Esc` exits |
| `q`, `Ctrl-C` | NOR | Exit immediately |
| `Ctrl-C` | INS | Exit immediately |
| `Enter` | either | Clean the selected units |
| printable keys | INS | Append to the filter field |
| `Backspace` | INS | Delete the last filter character |

Four decisions in that table are load-bearing and were each pinned by a test
that failed first:

- **Row 0 is an all-toggle that ignores the filter** (`src/interactive.rs:349-357`).
  A toggle that obeyed the filter would make "all" mean "whatever happens to be
  on screen". The row states its own arithmetic (`N/M selected`) so the scope is
  never in doubt.
- **`q` quits from NOR and types in INS** (`src/interactive.rs:274-300`).
  INS is a text field; every printable key inserts. `Ctrl-C` is the only key that
  quits from inside it.
- **Space is a filter character in INS** (`src/interactive.rs:311-318`).
  `normalize` maps `Char(' ')` onto `Key::Space` in every mode, so treating it as
  a no-op would make any workspace path containing a space impossible to filter
  for. This was a real defect, found by
  `insert_mode_types_keys_that_are_nor_commands`.
- **Enter on an empty selection does not commit** (`src/interactive.rs:415-427`).
  Closing the session there would look exactly like one that cleaned nothing —
  the M012A silent-no-op defect in a new costume.

Two more are deliberate defaults: **nothing is checked initially**, because a
selection UI whose default is "delete all of it" is a trap wearing a checkbox;
and **one `Esc` never destroys a session**, because the operator may still be
reading it. Any navigation cancels a pending exit (`src/interactive.rs:333-341`).

Editing the filter parks the cursor on the all-toggle
(`src/interactive.rs:365-382`): carrying a row offset across a changing result set
is how a cursor ends up pointing at a different unit than the one it was on when
it was moved.

## 7. Rendering as a projection of ownership

Each row annotates its unit's ownership class (`src/interactive.rs:480-484`). A
non-private row reads `shared — inventory only` rather than being hidden: a
hidden row is indistinguishable from a row that does not exist, and the
operator's next question would again be "which one?".

`CleanupCandidate::provably_private` (`src/cleanup.rs:1796-1798`) is a
**presentation hint about ownership only**. It is deliberately not an eligibility
verdict — policy gates and selector capability live in the engine, and
duplicating them here is how a listing starts disagreeing with the run.

## 8. Known gaps

- **No pty harness.** C023 already records that this repository cannot prove
  terminal-dependent behaviour end to end. The raw-mode lifecycle is therefore
  covered at the seam (`RawModeGuard` against a fake), which proves *our* code
  restores on every exit path — it does not prove the OS honours the call. The
  in-place redraw, cursor visibility, and resize handling are unverified against
  a real terminal for the same reason.
- **No eligibility preview.** A row may be checked that policy will later skip.
  This is deliberate: duplicating the policy gates in the listing is the drift
  this module exists to avoid. The report names the reason code per row.

---

## 9. Dependency

`crossterm 0.28` is a **non-optional** dependency. Default features are kept on
purpose (`Cargo.toml:50-62`): the `windows` feature pulls the console API that
crossterm's Windows raw-mode and cursor paths call unconditionally, and trimming
it compiles on Linux while being exactly the kind of platform-only gap this
repository has been bitten by before (C020, C021, C024). The `bracketed-paste`
that arrives with it is inert — nothing here enables paste reporting.

Cost is recorded honestly: this adds a dependency to a crate whose minimal set is
a stated property. ADR 001 rejected `ratatui`/`crossterm` for *transient
progress*, which is a different requirement and did not exist when it was
written.