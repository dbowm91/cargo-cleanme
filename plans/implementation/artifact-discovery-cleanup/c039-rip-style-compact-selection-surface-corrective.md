# C039 — Rip-Style Compact Selection Surface Corrective

Status: **closed** — implementation landed and verified, locally and on the
hosted lanes (CI `39378266722`, all nine jobs green, plus the Release drift
guard `39378266902`; and, on the final tree, CI `39379480713` and the drift
guard `39379480734`, all nine jobs green again), and the publication that was
the last remaining condition has happened: **v0.2.10** carries this corrective
(tag `v0.2.10` at `270cceb2be31`; GitHub release published 2026-10-21T00:35:53Z,
immutable with a valid attestation; crates.io sha256 `0f2e471b…`; automatic
smoke `39522277814` green on five targets). Evidence in
`plans/closure/artifact-discovery-cleanup/c039-status.md`; release record in
`plans/closure/distribution-release-update/r030-status.md`.

Repository baseline: `main` at `413f25d` (v0.2.9, published).

Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.

Original plan and closure record: `plans/implementation/artifact-discovery-cleanup/m014-cwd-scoped-default-and-interactive-selection.md`
and `plans/closure/artifact-discovery-cleanup/m014-status.md`.

Related: `architecture/16-interactive.md`; `architecture/overview.md` §10 findings
**36** (the terminal cursor is not restored on exit) and **48** (the selection
surface has no integration coverage); ADR 006 §7 (`crossterm` is adopted for raw
mode, keys, and rendering, **inline rather than alternate-screen**); the C038
corrective, whose interactive contract (three endings, an empty set is a plain
negative result) remains binding and unmodified.

Owner direction (2026-10-20): the `-i` surface is *functional but not very
usable, especially in large directories*, and should look and behave like
`rip` — compact, and able to deal with a large list.

## Objective

Make the interactive selection surface a **finder** rather than a **list**.

`cargo cleanme -i` was built as a list: it renders every resolved unit as a row
in a frame that occupies the entire terminal, and it gives the operator one
unlabelled mode switch to reach a filter. On a machine with a few dozen
workspaces that is already awkward; on a machine with hundreds it is unusable,
because there is no scroll position, no paging, and the absolute path of every
row occupies most of the width while the part that distinguishes one workspace
from another sits at the far end of it.

The corrective changes the **presentation** of the selection surface to the
`rip` idiom — a bounded page, aligned columns, a column header, a key-hint
bar, a scroll gutter, and colour — and it changes the **information density**
of a row: the path column shows the shortest suffix that identifies the
workspace uniquely among the candidates, and the ownership column disappears
when it would say the same thing on every row.

The surface's **contract** does not change. What it selects, what it refuses,
what it returns, and how it ends are all exactly what they are today.

## Non-goals

- **Not** a change to the selection contract. The listing is still the engine's
  own unit set; the selection can only remove candidates, never add one; the
  final proof is still per-spawn in the engine.
- **Not** a change to the interaction model. The owner decided to keep NOR/INS
  and the existing key table, and to keep the all-toggle row. This corrective
  restyles the surface; it does not re-key it.
- **Not** an alternate screen. ADR 006 §7 decides "inline rather than
  alternate-screen", and `rip`'s reference UI is itself inline. The rendering
  stays inline: the region repaints in place and is erased on exit.
- **Not** a change to the JSON or log contracts, or to any exit code.
- **Not** a fix for overview finding **36** (the terminal cursor is not restored
  on exit). That defect is separate and stays open; this corrective must not
  silently absorb it, and it must not make the test that currently pins it
  wrong in a new direction.
- **Not** a fuzzy matcher. The filter stays a case-insensitive substring match
  against the **full path**, exactly as today. A fuzzy matcher is a separate
  decision with its own scoring rules.
- **Not** a new dependency. `crossterm` is already here; `ratatui` and
  `inquire` are not, and are not introduced.

## Current implementation evidence

Captured by driving the built binary over a real pty at a chosen size, with 24
resolved candidate workspaces.

**The frame is the whole terminal.** `list_rows()`
(`src/interactive.rs:259-261`) returns `height - CHROME_LINES`, and the frame
is exactly `CHROME_LINES + page` lines (`src/interactive.rs:134`,
`src/interactive.rs:506`). At 100x12, the surface painted 12 lines for 24
candidates:

```
cargo-cleanme — select workspaces — 0 selected of 24, 0.00 B reclaimable
NOR  filter: <none>
>[ ] all
 [ ] /home/sugarwookie/projects/tui-fixture/project-01  9.66 MiB  private
 ...
```

**There is no scroll position on screen.** The same 100x12 capture shows nine
of twenty-four rows and nothing that says so.

**There is no paging.** `Key` (`src/interactive.rs:67-78`) has no PageUp,
PageDown, Home or End; every unbound key normalises to `Key::Ignored`
(`src/interactive.rs:751-756`). A 300-unit scope is 300 `Down` presses.

**Filtering costs a mode and then costs navigation.** `i` enters INS
(`src/interactive.rs:285-290`), and in INS the arrows are ignored
(`src/interactive.rs:330`).

**Every keystroke repaints the whole screen.** Each frame is
`MoveUp(n)` + `Clear(FromCursorDown)` + a rewrite of every line
(`src/interactive.rs:651-660`, `src/interactive.rs:705-715`). The capture shows
`ESC[12A ESC[J` followed by all twelve lines, on every key.

**The columns do not line up and the path dominates.** The size field is
left-aligned and ragged; the absolute path occupies 44 of 100 columns while
the distinguishing tail (`project-01`) sits at the end of it.

**The ownership column is noise when it is uniform.** Every row reads
`private` (`src/interactive.rs:485-489` at the baseline).

**There is no key hint anywhere in the frame.**

## Why previous verification missed it

M014's evidence was the right evidence for the contract it was checking, and
the wrong evidence for usability.

- The module's own tests assert the frame's **shape** — that it is exactly the
  terminal height, that a row carries `[x]`, that the header carries
  `N selected of M`. Those assertions *pin the shape that is now the defect*:
  `the_frame_is_exactly_the_terminal_height_and_never_overflows_the_width`
  (`src/interactive.rs:1188-1201`) and
  `the_driver_repaints_in_place_without_an_alternate_screen`
  (`src/interactive.rs:1404-1411`) fail first if the page becomes bounded.
- There is **no pty harness** (overview finding **48**). The rendered surface
  has never been observed by an automated test; only `Session::frame()`'s
  strings have been, and only as substrings.
- A green test proved its own premises: that the frame has the shape the
  implementation chose. It could not prove that the shape was usable, because
  "usable" was never an assertion.

This corrective therefore carries a **new** kind of regression evidence: the
rendered frame is compared against a *compactness* property (the frame is
bounded and does not grow with the candidate count) rather than against a
transcript, and a pty capture is part of the closure evidence.

## Invariants

1. **The listing is still the engine's own unit set.** No row is invented,
   reclassified, or hidden.
2. **A selection can only remove candidates.** The all-toggle still applies to
   the whole candidate set and ignores the filter.
3. **The filter still matches the full path**, case-insensitively, even though
   the *displayed* path is a suffix. A filter that matched only the displayed
   suffix would silently stop finding workspaces.
4. **Enter on an empty selection does not commit.**
5. **The three endings are unchanged**: selected, cancelled, and an empty
   candidate set as a plain negative result.
6. **Rendering stays inline.** The region is repainted in place and erased on
   exit; no alternate screen is entered.
7. **The frame never exceeds the terminal.** A bounded page is only safe if the
   frame still fits the terminal the driver measured.
8. **`Session` stays pure.** It renders to `Vec<String>` with no terminal in
   scope, and every method remains a total function of `self` and the key.
9. **Colour is opt-out.** `NO_COLOR` disables it; so does `TERM=dumb`. The
   surface stays fully usable without colour.

## Production changes

All in `src/interactive.rs` unless stated.

### A. The layout

The frame becomes a fixed five-line skeleton around a bounded page:

| Line | Content |
|---|---|
| 1 | title — `cargo-cleanme — select workspaces — N selected of M, X reclaimable` |
| 2 | status — `NOR`/`INS`, the filter, the armed exit |
| 3 | column header — `WORKSPACE`, `RECLAIMABLE`, and the position `cursor/total` |
| 4 | the all-toggle row — fixed, never scrolls |
| 5.. | the candidate rows, bounded |
| last | the key-hint bar |

The page is `min(15, visible rows, terminal height - 5)` — `rip`'s own page
size, capped so the frame never overflows the terminal. The frame height is
therefore **bounded**: it does not grow with the candidate count and does not
grow with the terminal.

### B. The row

```
<gutter> <checkbox> <workspace>  <reclaimable>  <ownership>
```

- **gutter** — one column: `>` on the cursor's row, `^` on the first visible
  row when there are rows above it, `v` on the last visible row when there are
  rows below it, otherwise blank.
- **checkbox** — `[x]` / `[ ]`, as today.
- **workspace** — the **shortest suffix of the path that identifies this
  workspace uniquely among all candidates**. A pure function of the candidate
  set, computed once per session. Siblings read `project-01`, `project-19`; a
  colliding pair reads `a/x` and `b/x`. The full path is still what the filter
  matches and what the selection carries.
- **reclaimable** — right-aligned in a fixed field, so the sizes line up and
  the eye can find the big one.
- **ownership** — rendered only when it is not uniform. When every candidate
  is private the column is omitted entirely; otherwise every row carries its
  class, and a non-private row still reads `<class> — inventory only`. A row
  is never hidden and never mistaken for a candidate.

### C. Colour

`Session` gains a `color` flag, set by the driver from `NO_COLOR` and
`TERM=dumb`. The styles are: chrome dim, the cursor's row reverse-video across
the full width, a checked checkbox green, the reclaimable column cyan, a
non-private ownership annotation yellow. The escape sequences are emitted by
the renderer as part of the line, so the `Terminal` trait and its
`draw(&[String])` seam are unchanged.

### D. Paging keys

`Key` gains `PageUp`, `PageDown`, `Home` and `End`; `normalize` maps them.
In NOR they move by a page and to the ends of the list; in INS they are
ignored, like the arrows.

### E. What is *not* changed

The key table, the mode model, the all-toggle row and its semantics, the
filter's matching rule, the commit rule, the exit arming, the three endings,
the `Terminal` trait, the raw-mode lifecycle, and the inline rendering.

## Work packages

1. **Layout and row rendering** — the frame skeleton, the column arithmetic,
   the gutter, the bounded page.
2. **The unique suffix** — the per-candidate display name.
3. **Colour** — the styles, the `NO_COLOR`/`TERM=dumb` gate, and the driver's
   wiring.
4. **Paging keys** — the new `Key` variants, the normalisation, the movement.
5. **Tests** — the rewritten and added cases, and the mutation evidence.
6. **Documentation** — the deep dive, the user docs, the CLI help, the skill,
   the generated artifacts, the changelog, the registry, the closure record.

## Required tests

Rewritten (they pin the old shape and must now pin the new one):

- the frame is bounded and does not grow with the candidate count;
- the frame never exceeds the terminal width or height;
- the all-row still states its arithmetic;
- the status line still shows the filter in both modes and the armed exit;
- a row still annotates a non-private unit;
- the driver still repaints in place, without an alternate screen;
- scrolling still keeps the cursor's row inside the viewport.

Added:

- the shortest unique suffix distinguishes siblings, distinguishes a
  colliding pair, and falls back for an identical path;
- the filter still matches the **full** path, not the displayed suffix;
- the ownership column is omitted when every candidate is private and present
  when the set is mixed;
- the reclaimable column is right-aligned and the sizes line up;
- the cursor's row is reverse-video and the gutter marks the window edges;
- `NO_COLOR` and `TERM=dumb` produce an uncoloured frame;
- PageUp, PageDown, Home and End move as bound;
- the frame is bounded at a 200-row terminal and at a 4-row terminal.

Mutation evidence required for each: a bounded page that still fills the
terminal, a suffix that is not unique, a filter that matches the suffix, an
ownership column that is always present, a colour flag that is ignored.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test && python3 scripts/check-release-contract.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

Plus a pty capture of the real binary over a multi-candidate fixture, before
and after, at two terminal sizes.

## Documentation updates

- `architecture/16-interactive.md` — the layout contract, the key table, the
  row format, the ownership column rule, the colour rule.
- `architecture/overview.md` — the module table, the §4.17 summary, the
  finding dispositions.
- `architecture/14-testing-and-verification.md` — the counts.
- `docs/USAGE.md` — the interactive key table and the described surface.
- `src/cli.rs` — the `-i` help text, which names the interaction model and
  propagates into the man page and all five completions.
- `completions/`, `man/` — regenerated, never hand-edited.
- `.skills/terminal-and-interactive/SKILL.md` — the surface's shape.
- `CHANGELOG.md`, `plans/registry.md`, and the closure record.

## Rejected alternatives

- **A full-screen alternate-screen table** (`rip --live`). It contradicts ADR
  006 §7 and `rip`'s own default mode, and it strands the operator's
  scrollback for the whole session.
- **Adopting `rip`'s type-to-filter model.** It is the better model, and the
  owner considered it; it was declined in favour of keeping NOR/INS and the
  published key table. It would also have made a space untypeable in the
  filter, which the current contract explicitly allows.
- **Adding `ratatui` or `inquire`.** The layout is a fixed set of columns and
  a bounded page; a widget framework buys nothing here and costs a dependency
  the crate has twice declined.
- **A fuzzy matcher.** A separate decision, with its own scoring rules and its
  own evidence.
- **Colouring the rest of the crate.** The TUI is the only surface that owns
  the terminal; the deterministic human report stays plain text, because its
  consumers are scripts and diffs.

## Acceptance criteria

1. The frame is bounded: its height does not depend on the candidate count,
   and it never exceeds the terminal.
2. A 200-candidate scope is navigable with paging keys, and the operator can
   see where they are.
3. The workspace column shows a name that identifies the row, and the
   reclaimable column lines up.
4. The surface renders with colour, and renders correctly without it.
5. Every invariant above holds, and the tests that check them fail when the
   behaviour is broken.
6. The documentation describes the surface that ships.

## Stop conditions

- If the bounded page cannot be made to fit the terminal without losing a
  row, stop and record the geometry rather than shrinking the terminal
  contract.
- If the unique-suffix rule cannot be made to be unique for a pathological
  candidate set, fall back to the full path for the affected rows rather than
  showing an ambiguous name.
- If the colour work turns out to require changing the `Terminal` trait, stop
  and record why; the seam is the crate's main testability mechanism.

## Closure evidence requirements

- The implementation commit.
- A requirement-to-evidence matrix over the nine invariants.
- The verification commands above, actually run, with their results.
- The before-and-after pty captures at two terminal sizes.
- The mutation evidence for each rewritten test.
- Hosted Linux, macOS and Windows lane results, and the MSRV lane.
- Known limitations, and the unresolved findings by severity.
- The disposition, which is not `closed` until a published release carries it.
