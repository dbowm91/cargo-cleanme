# C039 — Rip-Style Compact Selection Surface: closure record

Status: **closed — shipped in v0.2.10** (tag `v0.2.10` at `270cceb2be31`,
GitHub release published 2026-10-21T00:35:53Z and immutable with a valid
attestation, crates.io sha256 `0f2e471b…`, automatic smoke `39522277814`
green on five targets). Receipt:
`plans/closure/distribution-release-update/r030-status.md`.

Plan: `plans/implementation/artifact-discovery-cleanup/c039-rip-style-compact-selection-surface-corrective.md`

Repository baseline: `main` at `413f25d` (v0.2.9, published).

Implementation: `382192d` (`src/interactive.rs`, `src/cli.rs`, the regenerated
root man page). Documentation re-derivation: `3d29720`.

## The executive finding

The `-i` selection surface was **functionally correct and ergonomically weak**,
and the weakness was structural rather than cosmetic: it had been built as a
*list*, not as a *finder*.

The frame was exactly the terminal height, so the list filled the screen
instead of being a window. There was no scroll-position indicator, so an
operator looking at nine of twenty-four rows could not tell there were fifteen
more. There was no paging key at all — `Key` had no PageUp, PageDown, Home or
End — so a scope with three hundred workspaces cost three hundred `Down`
presses. Every row spelled out its whole absolute path, which spent most of
the width on a prefix every row shares and pushed the part that distinguishes
one workspace from another to the far end of the line. Every row repeated the
word `private`, whether or not that was news.

Nothing about that was wrong. Everything about it was *unusable at scale*.

The owner directed the redesign: *"something visually closer to rip is what we
are looking for, since it's compact and can deal with a large list of
directories"*, and then took three decisions:

| Decision | Choice | Consequence |
|---|---|---|
| Interaction model | **Keep NOR/INS** and the published key table; adopt `rip`'s *styling* only | `rip`'s own model — no modes, every printable key filters, Space toggles — was considered and declined. The mode switch survives, and a space remains typeable into the filter. |
| Row content | **The shortest suffix that uniquely identifies the row** | siblings read `project-01`, `project-19`; a colliding pair reads `a/x` and `b/x` |
| Colour | **Yes** | dim chrome, reverse-video cursor row, green checkbox, cyan reclaimable column, yellow non-private annotation; `NO_COLOR` and `TERM=dumb` turn it off |

## What changed

`src/interactive.rs`, production code only:

1. **A bounded page.** `list_rows()` returns `min(PAGE_ROWS, visible rows,
   terminal height - chrome)` with `PAGE_ROWS = 15`. The frame's height no
   longer depends on the candidate count and never exceeds the terminal.
2. **A named row.** `unique_labels()` gives each candidate the shortest
   trailing path components that no other candidate shares — a pure function
   of the whole candidate set, computed once per session.
3. **Columns.** The reclaimable size is right-aligned in a fixed field; the
   ownership annotation is omitted entirely when every candidate is private,
   and degrades to the bare class label on a terminal too narrow for the full
   wording.
4. **A gutter.** `>` marks the cursor, `^` and `v` mark the edges of the
   window when there are rows above or below it.
5. **A column header and a key-hint bar**, so the numbers have something to be
   numbers of and the keys do not have to be remembered.
6. **Paging keys.** `PgUp`, `PgDn`, `Home` and `End` in NOR; ignored in INS,
   like the arrows.
7. **Colour**, emitted as ANSI escapes inside the rendered lines, so the
   `Terminal` trait and its `draw(&[String])` seam are unchanged. The decision
   is made in the driver (`color_enabled_with`), so `Session` stays a pure
   function of its own state.

**Not changed:** the selection contract, the three endings, the all-toggle row
and its "ignores the filter" semantics, the filter's matching rule, the
commit rule, the exit arming, the NOR/INS model, the published key table
(except the four new paging keys), the `Terminal` trait, the raw-mode
lifecycle, and the inline rendering. **No dependency was added**: `ratatui`
and `inquire` were not adopted.

## The requirement-to-evidence matrix

| # | Invariant | Evidence |
|---|---|---|
| 1 | The listing is still the engine's own unit set | `the_candidate_listing_is_the_engines_own_unit_set` (`src/cleanup.rs:4328-4468`), unchanged by this corrective |
| 2 | A selection can only remove candidates; the all-toggle ignores the filter | `the_all_row_checks_every_candidate_including_filtered_ones` (`src/interactive.rs:1430`), unchanged |
| 3 | The filter still matches the full path, even though a row is *named* by a suffix | `filtering_is_case_insensitive_and_matches_the_full_path` (`src/interactive.rs:1597`) — proven by mutation M8 |
| 4 | Enter on an empty selection does not commit | `src/interactive.rs:551-568`, unchanged |
| 5 | The three endings are unchanged | `an_empty_candidate_set_is_a_plain_negative_result_not_a_usage_error`, `the_driver_restores_the_terminal_on_the_quit_path`, `the_driver_restores_the_terminal_on_the_commit_path` |
| 6 | Rendering stays inline | `the_driver_repaints_in_place_without_an_alternate_screen` (`src/interactive.rs:1991`) — no alternate-screen escape anywhere in the captured bytes |
| 7 | The frame never exceeds the terminal | `the_frame_is_bounded_and_never_overflows_the_width` (`src/interactive.rs:1716`), `the_page_is_capped_by_the_terminal_height` (`src/interactive.rs:1770`) |
| 8 | `Session` stays pure | `Session` still renders to `Vec<String>` with no terminal in scope; the colour flag is set by the driver, and `colour_can_be_turned_off` proves the flag is honoured |
| 9 | Colour is opt-out | `color_enabled_with` honours `NO_COLOR` and `TERM=dumb` — `the_colour_decision_honours_no_color_and_a_dumb_terminal` (`src/interactive.rs:2391`) |

## The mutation evidence

Each mutation breaks exactly one behaviour the new tests claim. A mutation
that leaves the suite green means the test is decoration. Backups were `cp`
copies, never `git checkout --`.

| # | Mutation | Tests that failed |
|---|---|---|
| M1 | The bounded page removed (`PAGE_ROWS.min(wanted).min(available)` → `available`) | `the_frame_does_not_grow_with_the_candidate_count`, `the_frame_is_bounded_and_never_overflows_the_width`, `the_driver_repaints_in_place_without_an_alternate_screen`, `a_resize_keeps_the_selection_and_reclamps_the_cursor`, `an_empty_candidate_list_still_renders_and_exits` |
| M2 | A row shows the whole absolute path again | `a_row_is_named_by_the_shortest_suffix_that_identifies_it`, `a_colliding_basename_is_told_apart_by_more_of_the_path`, `the_rendered_row_carries_the_short_name_not_the_whole_path` |
| M3 | The ownership column always present | `the_ownership_column_is_omitted_when_every_candidate_is_private` |
| M4 | The colour flag ignored | `colour_can_be_turned_off` |
| M5 | The paging keys do nothing | `paging_keys_move_the_cursor_by_a_page` |
| M6 | The cursor row is not highlighted | `the_cursor_row_is_reverse_video` |
| M7 | The gutter marks nothing | `the_gutter_marks_the_edges_of_the_window` |
| M8 | The filter matches the displayed name, not the full path | `filtering_is_case_insensitive_and_matches_the_full_path`, `a_filter_can_contain_a_space_because_paths_can` |

## The live evidence

The repository has **no pty harness** (overview finding **48**), so the
rendered surface has never been observed by an automated test. It was
observed by hand, over a real pty, with a fixture of 24 resolved candidate
workspaces.

**Before — the published v0.2.9 binary, at 100x30.** Thirty lines painted
for 24 candidates: the whole terminal, no position indicator, every row
spelling out its absolute path and its ownership class.

```
cargo-cleanme: omitting 15 non-maintainable learned root(s); examples: ...
cargo-cleanme — select workspaces — 0 selected of 24, 0.00 B reclaimable
NOR  filter: <none>
>[ ] all
 [ ] /home/sugarwookie/projects/tui-fixture/project-01  9.66 MiB  private
 [ ] /home/sugarwookie/projects/tui-fixture/project-19  964.00 KiB  private
 ... (24 rows, one per candidate, filling the screen)
```

**After — this tree, at 100x30, after `Down`, `Down`, `Space`.** Twenty lines
painted, whatever the candidate count:

```
cargo-cleanme — select workspaces — 1 selected of 24, 964.00 KiB reclaimable
NOR  filter: <none>                                          2/24
      WORKSPACE                                       RECLAIMABLE
  [-] all                                       (1/24 selected)
  [ ] project-01                                  9.66 MiB
> [x] project-19                                964.00 KiB
  [ ] project-24                                964.00 KiB
  ...
v [ ] project-23                                568.00 KiB
[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]
```

The raw capture also shows the escape sequences: `ESC[?25l` (hide the
terminal's own cursor) on entry, `ESC[20A` + `ESC[J` + the frame on each
repaint, `ESC[7m` on the cursor's row, `ESC[36m` on the reclaimable column,
and `ESC[2m` on the chrome. **No alternate-screen sequence is emitted**, so
ADR 006 §7's inline decision holds.

Measured from the captured bytes, rather than read off the screen:

| | Lines painted per frame | Alternate-screen sequences |
|---|---:|---:|
| Before (published v0.2.9) | **30** — the whole terminal | 0 |
| After (this tree) | **20** — 5 chrome lines + a 15-row page | 0 |

The repaint size is the property that matters: it is now a function of the
page, not of the terminal, and it is the same whatever the candidate count.

**`NO_COLOR` was verified over the same pty.** With `NO_COLOR=1` in the
environment, the captured bytes contain **no** styling sequence — no
`ESC[0m`, `ESC[1m`, `ESC[2m`, `ESC[7m`, `ESC[32m`, `ESC[33m` or `ESC[36m` —
while the terminal control sequences (`ESC[?25l`, the hide-cursor) are still
there. The surface stays fully usable without colour; only its appearance
changes.

## What the hosted lanes caught

The first hosted run (CI `39375009699`) was **red on Windows only**, and the
failure was worth having: two of the new tests had hard-coded a POSIX path
separator.

| Test | Linux | Windows | Cause |
|---|---|---|---|
| `a_colliding_basename_is_told_apart_by_more_of_the_path` | pass | **fail** | expected `a/x`; the label is joined with the platform separator, so Windows produced `a\x` |
| `a_long_display_name_is_truncated_from_the_left_not_the_right` | pass | **fail** | expected the frame to contain `deep/project`; on Windows it holds `deep\project` |

Both were **test** bugs, not product bugs: the production code joins the
label's components with `PathBuf::push`, so it uses the platform separator
correctly. The fix builds the expectation from a path rather than from a
literal.

This is the AGENTS.md §3.1 rule in action — a change touching path rendering
is not verified until the hosted lanes run — and it is the argument for
running them rather than trusting a green local suite: the local suite could
not have caught either.

## The citation re-derivation

`src/interactive.rs` grew from 1627 to 2417 lines and `src/cli.rs` from 1273
to 1276, so every citation into them moved. They were re-derived from a
**content-exact** line map built from git's own unified diff — every mapped
line holds the same text it held at the baseline, which is a stronger and
machine-checkable property than "the numbers look right".

| Checked | Count | Result |
|---|---:|---|
| Explicit `file.rs:N` citations into the changed modules | 354 | all resolve to an existing file and an in-range line |
| … of those, content-exact against the baseline | 327 | the map moved them to the same text |
| … of those, pointing into rewritten code | 7 | hand re-derived against the new source |
| … of those, pointing into code that did not exist at the baseline | 20 | new code; no baseline counterpart to be exact against |
| Baseline-relative by design (the C039 plan's evidence section, which says so) | 12 | not live pointers |
| Bare `:N` citations in `architecture/16-interactive.md` | 0 | the rewritten deep dive uses the explicit form throughout |
| `check-doc-citations.py` | 17 documents | clean |

The plan's own evidence citations deliberately refer to the baseline
`413f25d` — that section describes the tree the corrective was written
against — and the plan now says so.

## The hosted evidence

CI `39378266722` and the Release drift guard `39378266902`, against
`b166d41`. **All nine jobs green**: `checks (ubuntu-latest)`,
`checks (macos-latest)`, `checks (windows-latest)`, `msrv` (toolchain 1.89,
`--locked`), `installers` ×3, `generated-docs`, and `benchmark`.

The **final tree** carries its own green run: CI `39379480713` and the Release
drift guard `39379480734`, both against `075842f` — all nine jobs green again,
including `generated-docs` and the MSRV lane.

That run failed once before it succeeded, and the failure was **not** a
regression. The `generated-docs` job never reached the repository's own drift
check: its `dtolnay/rust-toolchain@stable` setup step could not download
`channel-rust-stable.toml.sha256` from `static.rust-lang.org` (`Connection
timed out (os error 110)`) and the step exited non-zero. The eight
unaffected jobs were green on the same SHA, the commit was markdown-only, and
the local `generate-docs -- --check` gate was green on the identical tree.
Re-running the failed job (`gh run rerun --failed 39379480713`) produced the
green result above. Recording this rather than only the green outcome: a lane
that failed for infrastructure reasons is not a code signal, and a record that
showed only the passing re-run would misreport what happened.

The hosted lanes also **confirm the derived census**, name for name:

| Lane | lib | bin | total | Census |
|---|---:|---:|---:|---:|
| Linux | 468 | 4 | 472 | 472 ✓ |
| macOS | 465 | 4 | 469 | 469 ✓ |
| Windows | 431 | 4 | 435 | 435 ✓ |

Windows ran exactly the 19 `cli_contract` cases the `#[cfg(unix)]` gating
predicts (63 declared, 44 Unix-only).

## The census re-derivation

Counts are derived, not copied. A brace-tracking parser evaluates every
`#[cfg]` gate against `rustc --print cfg --target <triple>`, and the result
is cross-checked name-for-name against Cargo's own listing.

| Measure | Value |
|---|---:|
| Declared `#[test]` in `src/` | **474** |
| Linux / Windows / macOS | **472 / 435 / 469** |
| `src/` lines / production lines | **33,860 / 15,216** |
| `src/interactive.rs` | 2417 lines, 1311 production, 58 tests |
| `src/cli.rs` | 1276 lines, 484 production, 32 tests |
| Two-module concentration | 190 of 474 = 40.1% |

Cargo's listing on this host: 468 lib + 4 bin = 472, which is the Linux
figure; the two excluded are the `#[cfg(windows)]` case.

## The verification actually run

Every command below was run on this tree, in this order.

```sh
cargo fmt --all -- --check                              # ok
cargo clippy --all-targets --all-features -- -D warnings # ok, no warnings
cargo test --all-targets --all-features                 # 468 lib + 4 bin + 63 cli_contract + 4 end_to_end, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
                                                        # 37 fixture file(s) clean
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
                                                        # 17 document(s) clean
python3 scripts/check-release-contract.py --self-test && python3 scripts/check-release-contract.py
                                                        # 5 contracted targets, 11 release assets, wrappers aligned
cargo run --quiet --features dev-tools --bin generate-docs -- --check
                                                        # 13 artifacts match the clap model
```

One caveat on how that ladder was obtained, because a reader comparing
timestamps would otherwise be confused: this host's filesystem filled up
mid-ladder, and the `cargo test` step failed with `No space left on device`
inside `config::tests::concurrent_first_use_creates_one_complete_template` —
a **host condition, not a defect in this tree** (the failing code path is the
config directory creation, in a module C039 does not touch, and the same
command passed once space was available). The scratch Cargo target directories
left in `/tmp` by earlier milestone runs were then reclaimed with `cargo clean
--target-dir` — the same Cargo-mediated deletion this tool performs — and the
whole ladder was **re-run from the top** on the same tree. The figures above
are from that second, complete pass.

`scripts/release-check.sh` was **not** run: it requires a clean tree, the
`eggpack` binary, and toolchain 1.89, and it is the pre-release gate rather
than routine verification.

## Known limitations

- **No pty harness.** The live evidence above was captured by hand. The
  repository still cannot assert the rendered surface in CI, and the cursor
  defect below is still unproven by an automated test.
- **The terminal cursor is still not restored on exit** (overview finding
  **36**). C039 did not touch the cursor lifecycle; the test
  `the_driver_shows_the_hardware_cursor_only_in_insert_mode`
  (`src/interactive.rs:1973`) still **pins** the defect rather than catching
  it. The mode split that made the cursor question visible is gone, but the
  defect is not fixed, and it is not C039's to fix.
- **Width is counted in characters, not display cells.** `pad` and `truncate`
  count `chars()`, so a path containing wide (CJK) characters can still paint
  past the right edge. Pre-existing and unchanged.
- **The `-i` scope and the non-`-i` scope still report differently.** In a
  directory with nothing eligible, `-i` prints one line and the non-`-i` run
  prints the census and the out-of-anchor units. Both exit 0. Reprinting the
  census inside the interactive path is the noise this corrective exists to
  remove, so the divergence is deliberate.

## The unresolved findings, by severity

| Finding | Severity | State |
|---|---|---|
| 36 — the terminal cursor is not restored on exit | medium | **Open.** Pre-existing; pinned by a test; not C039's scope. |
| 48 — no pty harness for the selection surface | medium | **Open.** The live capture is hand evidence, not CI evidence. |
| 59 — load-sensitive real-Cargo assertions | low | **Open.** Pre-existing and proven on the unmodified baseline; not a C039 regression. |
| 58 — thirteen source doc comments still describe the pre-ADR 007 whole-scope block | low | **Open.** Carried by C036. |
| Width counted in characters rather than display cells | low | **Open.** Pre-existing. |

## The disposition

**Closed.** The implementation landed, is verified locally, and is green on
every hosted lane; and the publication that was the only remaining condition
has happened. **v0.2.10** carries the compact selection surface to the
public.

| | |
|---|---|
| Tag | `v0.2.10` at `270cceb2be318ab6e2f7da87a90d5ce73f7f292a` |
| GitHub release | published 2026-10-21T00:35:53Z, immutable, 15 assets, valid attestation |
| crates.io | published 2026-10-21T00:36:36Z, sha256 `0f2e471b…`, not yanked |
| Automatic smoke | `39522277814` — green on all five target lanes |
| External smoke | passed against the published crate and the published release asset |
| Release record | `plans/closure/distribution-release-update/r030-status.md` |

Publication is what closes a corrective in this repository — *compilation is
not closure, publication is not closure* — and the release notes are
maintainer-owned, so the publication was owner-gated as well as
gate-gated.

**Not closed by this**, and still open on its own merits:

- **Finding 36** — the interactive session leaves the terminal cursor hidden.
  It was a non-goal of this corrective and needs its own.
- **Finding 48** — the repository has no pty harness. The live capture is
  hand evidence, not CI evidence.
- **Finding 59** — the load-sensitive real-Cargo assertions. Pre-existing and
  proven on the unmodified baseline; not a C039 regression.

## Reproduction

```sh
cd /home/sugarwookie/projects/cargo-cleanme
cargo build
cd <a directory holding several resolved candidate workspaces>
/home/sugarwookie/projects/cargo-cleanme/target/debug/cargo-cleanme -i
```

To see the shape without a fixture, drive it over a pty of a chosen size:

```sh
python3 /tmp/pty_drive.py --cols 100 --rows 30 --wait 30 -- \
  /home/sugarwookie/projects/cargo-cleanme/target/debug/cargo-cleanme -i
```
