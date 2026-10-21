---
name: terminal-and-interactive
description: Driving a terminal in cargo-cleanme — the -i selection TUI, raw mode, cursor visibility, TTY preconditions, and the exit codes the interactive path returns
version: 1.0.0
tags:
  - interactive
  - terminal
  - tty
  - tui
---

# Terminal and Interactive

`-i` / `--interactive` is the only part of this tool that **takes over the
operator's terminal**. It is also the newest surface, the least covered by
integration tests, and the one with a live defect recorded against it.

The reference is [`architecture/16-interactive.md`](../../architecture/16-interactive.md)
and [`architecture/11-progress.md`](../../architecture/11-progress.md). This
skill is about *how to change it without stranding a terminal*.

## When to Load

- Any edit to `src/interactive.rs`, `src/progress.rs`, or the `-i` path in
  `src/main.rs`
- Changing the selection list, the key bindings, or the rendering
- Touching the progress renderer or its TTY gate

## The one thing to get right

> **A tool that can leave a terminal in a state the operator did not ask for is
> not shippable.**

Raw mode is the obvious case: no echo, no `Ctrl-C`, a shell that must be reset
by hand. The **cursor** is the case that was missed. Cursor visibility is a
*separate escape sequence* from the terminal mode, so restoring raw mode does
**not** restore the cursor.

## The cursor is not restored — and a test pins it

This is the defect to internalise before you touch this module.

```text
src/interactive.rs:1145   session_loop hides the cursor on entry
src/interactive.rs:1155   each frame sets it to (mode == Insert)
src/interactive.rs:1159   the commit arm rewinds and clears — no show
src/interactive.rs:1164   the quit arm rewinds and clears — no show
```

`set_cursor_visible(true)` is **never called anywhere in the crate**.
`RawModeGuard::restore` (`src/interactive.rs:1052-1062`) restores the terminal
*mode* (`disable_raw_mode`) and nothing else. `main.rs` has no cursor handling
at all.

So a session that ends in NOR — which is the default mode, and the mode you are
in when you press `q` — leaves the operator's shell **with no visible caret**.

**Verified live**: driven over a raw pty, the tool emitted `ESC[?25l` (Hide)
twice and `ESC[?25h` (Show) **zero** times, then exited 0 with
`cargo-cleanme: interactive selection cancelled; no cleanup commands were run`.

Two tests stand next to this defect and neither catches it:

- `the_driver_restores_the_terminal_on_the_quit_path`
  (`src/interactive.rs:1946-1956`) asserts `!term.raw` — **raw mode only** — and
  `term.restored == 1`. The name over-promises: it says "terminal", it means
  "mode".
- `the_driver_shows_the_hardware_cursor_only_in_insert_mode`
  (`src/interactive.rs:1958-1974`) asserts the **last** recorded visibility is
  `Some(false)`. It *pins the defect*.

That second one is the lesson worth more than the fix: **a test can be green
because it asserts the wrong thing.** Before you "fix" the cursor, fix the test
that would fail on the fix — otherwise you have swapped one green lie for
another. The correct assertion is that the cursor is **visible** when the
session ends, on both the commit and the quit path.

## An empty selection is a plain negative result

| Spelling | Same scope, no cleanup units | Exit |
|---|---|---|
| `cargo cleanme clean ROOT` | reports zero units, cleans nothing | **0** |
| `cargo cleanme -i clean ROOT` | says "Nothing to clean." | **0** |

The path: `run_interactive` settles an empty candidate list before the terminal
precondition (`src/interactive.rs:1281-1283`) → the orchestrator maps it to
`SelectionOutcome::NothingToClean` (`src/main.rs:644-657`) → one line on
stdout and `Ok(0)` (`src/main.rs:464-472`).

The two spellings of one scope now agree, which is the point: an unattended
caller can tell "there was nothing to do"
from "the command was wrong". **Verified live.**

The other two interactive exits, for contrast:

- **Cancel** (`q`, `Esc` `Esc`, `Ctrl-C`) → exit **0**, with one plain stderr
  line: `cargo-cleanme: interactive selection cancelled; no cleanup commands
  were run` (`src/main.rs:456-472`). A deliberate operator decision is a
  success, not a failure.
- **A blocked scope** → the listing is refused outright
  (`src/main.rs:624-633`) → exit **2**, with the blocked-scope detail. Offering
  rows from an unproven universe would be the ADR 006 shape in a new costume:
  the operator would be picking from a list whose classification is exactly what
  failed.

## The selection is not a second deletion path

A selection can **remove** candidates and never add one. Ownership is still
proven across the complete universe, and the engine still re-proves it per
spawn. The interactive path is a *narrower* `CleanupSelection::Roots` subset of
the working-directory anchor, not a parallel cleanup route.

Do not add a "the user checked it, so skip the proof" shortcut.

## The TTY precondition is checked before raw mode

`ensure_interactive_terminal` (`src/interactive.rs:1082-1093`) is called by
`run_interactive` (`src/interactive.rs:1284-1294`) **before** any mode is
requested. A redirected invocation fails as a typed usage error rather than
driving a session whose keystrokes are going somewhere else.

It takes plain `bool` flags rather than `&dyn IsTerminal` on purpose: that std
trait is sealed, so taking it would make the one guard that has to hold
precisely on the path where everything else is already broken **untestable
against a fake**.

Keep that shape. If you need a richer check, take more plain flags.

## The machine formats are refused

The session refuses `--format json` and `--format log`. A selection list is an
operator surface, not a report: a machine consumer has no way to say "row 3".

## Interaction rules that are load-bearing

Each of these was pinned by a test that failed first:

- **Row 0 is an all-toggle that ignores the filter** (`src/interactive.rs:485-493`).
  A toggle that obeyed the filter would make "all" mean "whatever happens to be
  on screen".
- **`q` quits from NOR and types in INS** (`src/interactive.rs:376-406`). INS is
  a text field; `Ctrl-C` is the only key that quits from inside it.
- **Space is a filter character in INS** (`src/interactive.rs:417-424`) — a
  workspace path can contain a space.
- **Enter on an empty selection does not commit** (`src/interactive.rs:551-563`).
  Closing there would look exactly like a session that cleaned nothing — the
  M012A shape, a run that reports a success it never performed.
- **Nothing is checked initially**, because a selection UI whose default is
  "delete all of it" is a trap wearing a checkbox.
- **The filter matches the full path, not the displayed name**
  (`src/interactive.rs:501-522`). A row is *named* by the shortest suffix that
  identifies it, but the filter still matches the whole path — a filter that
  only matched the displayed name would silently stop finding workspaces.

## The shape of the surface

C039 rebuilt the surface in the `rip` idiom. Three properties of it are worth
carrying, because each one was a defect before:

- **The page is bounded.** At most fifteen candidate rows, and never more than
  the terminal has room for (`src/interactive.rs:354-361`). The old frame
  filled the terminal exactly, so a long list was a screenful to scroll rather
  than a list to page through. `PgUp`/`PgDn`/`Home`/`End` move through it.
- **A row is named, not spelled out.** The workspace column shows the shortest
  suffix of the path that identifies the row uniquely among all candidates
  (`src/interactive.rs:919-944`). Siblings read `project-01`, `project-19`;
  a colliding pair reads `a/x` and `b/x`.
- **The ownership column is omitted when it would say the same thing on every
  row** (`src/interactive.rs:876-880`). A column that reads `private` twenty
  times carries no information and costs a column's width twenty times.

The surface is also the only place in the crate that renders **colour**
(`src/interactive.rs:1100-1116`): `NO_COLOR` and `TERM=dumb` turn it off, and
the decision is made in the driver so `Session` stays pure.

The model was **not** changed. `rip`'s own model has no modes — every printable
key filters, and Space toggles — and the owner considered and declined it, so
the published NOR/INS model and key table stand. That is also why a space
remains typeable into the filter.

## How it is tested, and what that cannot prove

The `Terminal` trait (`src/interactive.rs:120`) is the injectable seam, and the
fake terminal is what makes the raw-mode lifecycle testable at all. But the fake
**proves our code restores on every exit path; it does not prove the OS honours
the call.**

**There is no pty harness in this repository.** C023 §11.4 already records the
same limit for the progress bar: the ordering of "clear the bar, then run the
fallible cleanup" is unreachable from the tests, because the bar only draws on
an attended terminal.

If you need to prove something about a *real* terminal, you need a pty. Two
things to know if you build one:

- `script -qec` allocates a pty that translates `\r` into `\n`. Open a **raw**
  pty and clear `ICRNL|INLCR|ONLCR` with `termios` instead.
- To backdate a fixture project, backdate the **directories** as well as the
  files. The activity guard keys on directory mtimes too
  (`src/traverse.rs:461-468`), so backdating only the files leaves the fixture
  looking active.

## The progress renderer is the other TTY surface

- `should_show_progress` (`src/progress.rs:163`) is the single gate: the flag,
  the `TERM=dumb` check, and whether stderr is attended.
- `--format json` gets **no progress at all** (`src/main.rs:1408-1409`), because
  the gate tests the *format* rather than the *stream*. Progress is on stderr,
  so a `jq` consumer is unaffected — but a minutes-long JSON scan gives the
  operator nothing.
- **The frame is exactly one line.** M020 retired M005A §7.4's five transient
  candidate rows: on a real run they were empty — a spinner glyph and nothing
  else — for almost the whole traversal, so the "progress" occupied six
  terminal lines and said nothing. `coordinated_bar_count()`
  (`src/progress.rs:303`) is **1** visible / 0 hidden, and it must stay 1: a
  second bar is a regression, and `coordinated_renderer_owns_exactly_one_bar_on_one_draw_target`
  (`src/progress.rs:550`) fails if one appears.
- **The phase goes in the message, never the prefix.** indicatif's default
  spinner template is `{spinner} {msg}` (indicatif 0.18.6,
  `src/style.rs:41`) and does **not** render the prefix, so a phase written
  with `set_prefix` was invisible on the terminal. `compose_message()`
  (`src/progress.rs:365`) builds the whole line: the phase, then ` {done}/{total}`
  once a total is known, then the counters under `--verbose`. The counters go
  on **the same line** — verbosity must never make the frame taller.
- `set_determinate_total` (`src/progress.rs:322`) is still inconsistent: it
  sets the total without zeroing `determinate_done`, unlike `apply_total`.
  It no longer touches the bar's length, so the inconsistency is now state
  only.

## Before you claim the change is safe

- The raw-mode guard still restores on the commit path, the quit path, the
  read-error path, and an unwinding panic — and the cursor is **visible** when
  the session ends.
- `-i` and non-`-i` agree on the exit code for a scope that has roots but no
  cleanup units, **or** the divergence is recorded as an accepted decision.
- A redirected `-i` still fails before raw mode is requested.
- The selection still cannot add a candidate.
- The machine formats are still refused.

Then run the full gate (see
[`test-evidence`](../test-evidence/SKILL.md)).

## Related

- [`cleanup-safety`](../cleanup-safety/SKILL.md) — what the selection is
  allowed to authorise
- [`cli-surface-and-argv`](../cli-surface-and-argv/SKILL.md) — the `-i` flag
  itself
- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md) — the exit
  codes the interactive path returns
