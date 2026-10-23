# Progress — observing work without owning the terminal

> Component deep dive · part of the [architecture overview](overview.md)

`src/progress.rs` — 1007 lines, 535 production (`progress.rs:597` opens the test
module), 15 `#[test]` functions.

## 1. Responsibility

Owns **transient terminal feedback on stderr** and nothing else: the decision of
*whether* to draw, the phase vocabulary, the trait producers emit through, and the
one-line `indicatif` composition.

Does **not** own:

- **The final report** — that is stdout, via `report::render` (`main.rs:1791`) or
  `output::scan` (`main.rs:1778`) or `output::log::scan` (`main.rs:1787`).
  `Reporting` (`progress.rs:21`) is announced (`main.rs:1762`) then immediately
  followed by `finish_and_clear()` (`main.rs:1763`); the phase is written into the
  message and a draw is requested, but the clear lands before any `Reporting`
  frame can survive on screen, so no `Reporting` frame is ever seen.
- **Any domain truth.** Every number the renderer shows is a side effect of an
  observer call; the report's numbers come from `ScanCounters`, carried in from
  discovery at `main.rs:1548` and written into the report at `main.rs:1759`.
  Independent counter sets — see §8.
- **Phase sequencing.** `cleanup.rs:1680-1684` maps `CleanMode` → `ScanPhase`; the
  renderer reacts to whatever phase a caller names.

**The stdout/stderr separation is a contract, verified on three axes:**

| Axis | Evidence |
|---|---|
| Draw target is fd 2 | `ProgressDrawTarget::stderr_with_hz(10)` (`progress.rs:269`) |
| Report is fd 1 | `println!` for human (`main.rs:1791`), JSON (`main.rs:1776`), log (`main.rs:1776`), and both cleanup renderers (`main.rs:1280`, `main.rs:1318`) |
| Clear precedes report | `finish_and_clear()` at `main.rs:1763` (scan), `main.rs:588` (cleanup) |

The user-facing consequence: `cargo cleanme scan | jq` shows progress on the
terminal and keeps stdout pipeable. §5 shows the TTY check is on **stderr only**,
which is exactly the axis that preserves this.

**Dependency footprint is now zero.** The production half of `progress.rs` has
**no** `crate::` reference at all. The last one — `crate::report::format_bytes`,
used for the five candidate-row sizes — went away with the candidate rows M020
retired. Everything the production half needs is `std` (`progress.rs:7-14`) or
`indicatif`. The two `crate::` references that remain are test-only:
`crate::report::format_bytes` (`progress.rs:885`) and `crate::error::AppError`
(`progress.rs:969`).

> **Correction to the layer map.** The layer map (`architecture/overview.md` §5)
> has drawn an edge from `progress` down to `traverse (measurement)`. There is
> **no** `crate::traverse` reference anywhere in `progress.rs`, production or test.
> The `progress → report` edge the same diagram omits was real until M020 and is
> now **test-only** (`progress.rs:885`), so `progress` has no production crate
> edge left at all. The overview's test-only back-edge table (§5) lists
> `progress → error` only and needs `report` added to that row.

## 2. Public surface

`ScanPhase` — `progress.rs:16-25`. `Clone + Copy + Debug + Eq + PartialEq`.

| Variant | Work it represents | Entered from | Total known in advance? |
|---|---|---|---|
| `Discovery` | Filesystem walk for `Cargo.toml` | `main.rs:1522` | No — indeterminate |
| `Resolution` | `cargo metadata` per manifest | `main.rs:1577` | No — no `units_total` ever |
| `Analysis` | Parallel sizing + activity classification | `main.rs:1714-1715`, total at `workspace.rs:1818` | Yes — determinate |
| `Reporting` | About to write the stdout report | `main.rs:1762` | No — never drawn |
| `CleanupPreview` | Cargo's own `clean --dry-run` preview | `cleanup.rs:1681` | Yes — `cleanup.rs:1867` |
| `CleanupSimulate` | Application simulation, no Cargo spawned | `cleanup.rs:1682` | Yes — `cleanup.rs:1867` |
| `CleanupExecute` | Real cleanup through Cargo | `cleanup.rs:1683` | Yes — `cleanup.rs:1867` |

`ProgressObserver` — `progress.rs:39-53`. `Send + Sync`. All thirteen methods take
`&self` and **all thirteen have default empty bodies**, so an implementor overrides
only what it needs; `impl ProgressObserver for NoopObserver {}` (`progress.rs:59`)
is a complete, allocation-free implementation.

| Method | Argument | Meaning | Phases that use it |
|---|---|---|---|
| `phase(ScanPhase)` | new phase | Announce a transition; re-scopes the determinate scope | All four scan + all three cleanup phases |
| `dirs_visited(u64)` | batch delta | Entries walked, batched at 512 (`discovery.rs:778-780`) | `Discovery` |
| `dirs_pruned(u64)` | batch delta | Entries cut by prune rules, batched at 64 (`discovery.rs:843-845`) | `Discovery` |
| `manifests_found(u64)` | batch delta | Validated `Cargo.toml` files (`discovery.rs:840`) | `Discovery` |
| `discovery_issues(u64)` | final count | Discovery-stage diagnostics, kept separate from Cargo resolution failures (`main.rs:1541`) | `Discovery` → `Resolution` |
| `workspaces_resolved(u64)` | batch delta | Successful `cargo metadata` runs (`workspace.rs:1025`) | `Resolution` |
| `cargo_failure()` | — | One Cargo workspace-resolution command failed, at either locate or metadata (`workspace.rs:908`, `:928`, `:943`, `:955`, `:1003`, `:1159`, `:1181`, `:1205`) | `Resolution` |
| `empty_skipped()` | — | Group skipped: no eligible bytes (`workspace.rs:1844`, `1851`) | `Analysis` |
| `active_skipped()` | — | Group or workspace skipped: protected by source activity (`workspace.rs:1902`, `1864`) | `Analysis` |
| `group_measured(u64)` | bytes | A group finished sizing (`workspace.rs:2127`) | `Analysis` |
| `reportable_group(&Path, u64)` | path, bytes | A group qualifies for the report (`workspace.rs:2156`, `cleanup.rs:2727`) | `Analysis`, cleanup |
| `units_total(ScanPhase, u64)` | phase, total | Announce the unit count for a phase | `Analysis` (`workspace.rs:1818`), cleanup (`cleanup.rs:1867`) |
| `unit_completed(ScanPhase)` | phase | One unit finished | `Analysis` (`workspace.rs:2012-2128`), cleanup (19 sites, `cleanup.rs:1941-2410`) |

`active_skipped` is deliberately counted per **workspace** at
`workspace.rs:1902` and per **group** at `workspace.rs:2115`; the source comment
at `workspace.rs:1898-1901` records why the workspace count must not be added to
the group counter.

Everything else:

| Item | Signature | Contract |
|---|---|---|
| `NoopObserver` | `pub struct NoopObserver` (`:57`) | Zero-cost observer; the crate's default in unit tests (152 `NoopObserver` references against 15 `TestObserver` ones across `src/` and `tests/`) |
| `TestObserver::new` | `-> Self` (`:83`) | `Default` constructor |
| `TestObserver::total_for` | `(phase: ScanPhase) -> Option<u64>` (`:87`) | **Last** total announced for a phase, or `None` |
| `TestObserver::completed_count` | `(phase: ScanPhase) -> usize` (`:97`) | Count of `unit_completed` events for a phase |
| `should_show_progress` | `(no_progress: bool) -> bool` (`:177`) | Gate; see §5 |
| `IndicatifRenderer::new` | `(hidden: bool, verbose: bool) -> Self` (`:279`) | **Inverted** flag: `true` means *hidden*; `verbose` adds the counters to the line |
| `…::new_in_memory_for_test` | `(verbose: bool) -> (Self, InMemoryTerm)` (`:300`) | Same composition, vt100-backed screen |
| `…::is_hidden` | `-> bool` (`:313`) | Construction flag, not terminal capability |
| `…::coordinated_bar_count` | `-> usize` (`:321`) | 1 visible / 0 hidden |
| `…::has_shared_draw_target` | `-> bool` (`:328`) | `multi.is_some()` |
| `…::set_determinate_total` | `(&self, total: u64)` (`:340`) | **No callers anywhere** — §6 |
| `…::finish_and_clear` | `(&self)` (`:477`) | Wipes the transient region; returns `()` |
| `…::refresh_count` | `-> u64` (`progress.rs:436`) | Draws that passed the 100 ms gate |
| `…::top_groups` | `-> Vec<(PathBuf, u64)>` (`:500`) | Clone of the ≤5-entry candidate list |
| `…::determinate_total_value` | `-> u64` (`:504`) | Current scope's total |
| `…::determinate_done_value` | `-> u64` (`:508`) | Current scope's completed count |

`IndicatifRenderer::refreshes` is the one public *field* (`progress.rs:220`); the
rest of the state is private.

## 3. The observer trait

The trait is the module's reason for existing; everything else is an
implementation detail of one renderer.

**The three-way split is what makes determinate progress possible.**

- `phase(p)` declares *which* work is starting. It carries no count.
- `units_total(p, n)` declares *how much* work this phase has — once, after the
  total is genuinely known.
- `unit_completed(p)` reports *one* unit finished.

Collapsing any two loses something. Without `phase`, a renderer cannot re-scope:
cleanup counts would inherit analysis counts and a cleanup bar could open at 60%.
Without `units_total`, every bar is a spinner. Without `unit_completed`, a known
total has no denominator.

The renderer enforces the split strictly. `group_measured` is a **pure redraw
trigger** — the renderer discards its `bytes` argument entirely
(`progress.rs:496-498`) and never touches `determinate_done`; `active_skipped` is
likewise draw-only (`progress.rs:491-495`). Asserted at `progress.rs:837-838`:
after `group_measured(100)`, `determinate_done_value()` is still `0`; only
`unit_completed` moves it (`progress.rs:839-840`).

**`Send + Sync` (`progress.rs:39`) is a contract, and today no call site
exercises it across a thread.** The observer is threaded as a
`&dyn ProgressObserver` (`main.rs:1521`, `discovery.rs:336`, `workspace.rs:524`),
but every observer call in the crate happens on the calling thread. The
`dua_core::walk_roots` pool (`discovery.rs:685`) is handed the *descend filter*
closure (`discovery.rs:694`), not the observer; the observer is called from the
consumer loop at `discovery.rs:741`. The bounded-parallel Cargo resolution
(`workspace.rs:548-563`) likewise keeps the observer out of its workers. So
`Send + Sync` is forward-looking permission, not a bound the current code needs —
and it is exactly why the trait is also the reason every method takes `&self`:
shared counters under `&self` require interior mutability, `AtomicU64` for the
counters, `Mutex` for the candidate list, the phase, and the draw timestamp
(`progress.rs:218-230`).

**Why a trait and not a direct `indicatif` call.** Two independent reasons.
**Testability:** `discovery`, `workspace` and `cleanup` are tested with
`NoopObserver` (152 references across `src/` and `tests/`; `cleanup.rs` alone
names it 81 times) and with
`TestObserver` for event assertions (`cleanup.rs:8996`, `:8569`, `:9133`;
`workspace.rs:5447`, `:5710`); without a trait every one of those tests would need
a real terminal or a global renderer hook. **Multiple independent producers, one
renderer:** the scan and cleanup paths each build a single `IndicatifRenderer`
(`main.rs:569`, `main.rs:1518`) and never learn about one another — none of
`discovery`, `workspace`, `cleanup` imports `indicatif` (the only `indicatif` use
in `src/` is in this file), and none knows whether the renderer is a spinner, a TTY
bar, a test double, or absent. A third producer exists since M014: the
interactive candidate listing runs on its own **hidden** renderer
(`main.rs:688`), because a live bar would fight the repainting selection list.

**"The observer must never be able to fail a scan" — honoured.** Enforced at four
levels, and `main.rs:1519-1520` is accurate. **No fallible signature exists:**
every method returns `()` (`progress.rs:40-52`), so there is nothing to propagate
and no `?` is ever written against an observer. **Construction is infallible:**
`new` returns `Self`, not `Result` (`progress.rs:261`) — a terminal that cannot be
drawn to is the `hidden` branch, not an error (`progress.rs:262-264`).
**The one fallible indicatif call is discarded:** `finish_and_clear` does
`let _ = multi.clear();` (`progress.rs:429`). **Throttling bounds the cost**, so
progress cannot dominate a fast scan.

The comment does *not* claim a runtime recovery path, because there is none to
need: degradation happens once, at construction, via the caller's boolean.

## 4. Phases

**Indeterminate: `Discovery`, `Resolution`, `Reporting`.** No `units_total` is ever
emitted for these. `Discovery` cannot have one without a pre-scan
(`progress.rs:824-828` asserts the total must be `0`, with the reason in the assert
message). `Resolution` has no cheap cardinality — one subprocess per manifest,
failures unknown until run. `Reporting` is never left on screen.

**Determinate: `Analysis` and the three cleanup phases.** `Analysis` gets its
total from `workspace::analyze_groups` at `workspace.rs:1818` (and redundantly from
`main.rs:1715`). Cleanup gets its total at `cleanup.rs:1867`, gated on complete
ownership coverage — the comment at `cleanup.rs:1864-1865` is explicit that a
partial total would be a lie.

**Re-scoping semantics.** `main.rs:1712` says *"Phase first (re-scopes totals),
then total (starts fresh scope)"*. Verified against the renderer; the two steps
are distinct operations:

- `phase(p)` → `reset_totals_for_phase(p)` (`progress.rs:455`, `:326-333`): if `p`
  differs from `current_phase`, zero both determinate counters. The bar itself
  carries no length to re-scope — it is a spinner — so the visible effect is that
  the *message* loses its fraction, because the total is zero again.
- `units_total(p, n)` → `reset_totals_for_phase(p)` **again** (`progress.rs:513`),
  then `apply_total(n)` (`progress.rs:514`, `:335-341`): store `n`, zero
  `determinate_done`. The bar's position is re-applied on the next draw
  (`progress.rs:415-419`).

The second reset is idempotent (same phase, so the `if` at `progress.rs:328` does
not fire) and exists as defence in depth: `units_total` re-scopes on the phase
carried *by the event itself*, not only on an explicit `phase()` call. That is the
M8 fix named at `progress.rs:510-512`, tested at `progress.rs:843-860`.

**User-visible meaning:** a phase change makes the fraction vanish from the line
rather than carrying a partial count into unrelated work. Asserted at
`progress.rs:868-871` (`Analysis` at 1/5, then `CleanupSimulate` → total `0`,
done `0`).

## 5. Deciding whether to show progress

**Two gates, and they are different kinds of thing.** A call site must satisfy
both before a renderer exists at all.

1. **Format gate** — `main.rs:1516-1517` (scan) and `main.rs:567-568` (cleanup),
   both `options.format == OutputFormat::Human && progress::should_show_progress(no_progress)`.
2. **Construction gate** — `IndicatifRenderer::new(!show, options.verbose)` at
   `main.rs:1518` and `main.rs:569`. The renderer's `hidden` flag is the
   *negation* of the gate, so `show` being false produces a renderer whose every
   draw path short-circuits.

A third condition sits behind the format gate, inside
`should_show_progress(no_progress: bool) -> bool` — `progress.rs:163-177`. Three
conditions, evaluated in order, each an early `return false`:

| # | Condition | Line | Effect |
|---|---|---|---|
| 1 | `no_progress == true` | `progress.rs:164-166` | `--no-progress` wins outright |
| 2 | `TERM == "dumb"` | `progress.rs:167-169`, via `term_is_dumb` (`:185-187`) | Dumb terminal |
| 3 | `!stderr_is_terminal()` | `progress.rs:173-175` | stderr is not a TTY |

**TTY detection is on stderr, and only stderr.** `stderr_is_terminal`
(`progress.rs:189-192`) is `std::io::stderr().is_terminal()` via
`std::io::IsTerminal`. **stdout is never inspected.**

The table below therefore describes `--format human` only. Under the default
format, the four rows hold:

| Scenario | Progress shown? | Why |
|---|---|---|
| `cleanme scan \| jq` | **Yes** | stdout is a pipe, stderr is still the TTY |
| `cleanme scan 2> log` | **No** | stderr is a file, stdout is still the TTY |
| `cleanme scan > out.txt` | **Yes** | stdout is a file, stderr is the TTY — terminal still painted |
| `cleanme scan \| tee f \| jq` | **Yes** | piping stdout never suppresses progress |

Checking stderr is correct for the stated contract: the guarantee to protect is
"a piped stdout is not polluted", and stdout's own nature is irrelevant to that.
The row-3 asymmetry is real but is conventional Unix behaviour (a tool writes to
the terminal it is attached to); the crate is *more* permissive here than an
`is_stdout_terminal` check would be.

**Both non-Human formats suppress progress on a TTY** (`main.rs:1516-1517`,
`main.rs:567-568`). JSON is a deliberate choice: a machine-readable consumer should get
no ANSI noise in any captured stream. But the *scope* is too wide, because
progress goes to stderr and JSON to stdout. A user running `--format json` on a
TTY loses all feedback about a scan that can take minutes, for no correctness
benefit: `jq` is unaffected by stderr activity. The narrower rule "no progress
when stdout is a pipe" would give machine consumers silence only where they need
it.

**Log mode is a stronger case for silence, and gets it for a second reason.**
`--format log` exists to be *read by a program* line by line. A spinner
interleaving control sequences with `LogLine` records would corrupt that stream,
so the format gate is load-bearing there in a way it is not for JSON. Log mode
also drops the scan's per-diagnostic stderr fan-out (`main.rs:1796`, gated on
`options.format != OutputFormat::Log`) so its stderr carries
only `--stats`; the diagnostics instead ride along as the log line's
`diagnostics=N` count (`output.rs:344`). Progress suppression and diagnostic
suppression are separate decisions that happen to agree, and each was made for
its own reason.

**Testability of the gate.** The `TERM` branch is now asserted on
`term_is_dumb` (`progress.rs:185-187`) rather than on `should_show_progress`.
It used to assert `!should_show_progress(false)` under `TERM=dumb`, which holds
in CI because stderr is not a TTY there: the assertion passed with condition 2
deleted. The negation is checked too, under `TERM=xterm-256color`, so the test
now fails if the branch is removed rather than passing for the wrong reason. The
restore of the process-global `TERM` is an RAII guard (`progress.rs:549-552`, whose
`Drop` restores it at `progress.rs:584-593`), so a panic mid-test cannot leave
`TERM=dumb` behind for every later test in the binary.
`should_show_progress_respects_flag_and_dumb_term`
(`progress.rs:788-802`) covers conditions 1 and 2. Condition 3 — the TTY branch —
is **never exercised by any test in the crate**: every integration case that runs
a scan or cleanup passes `--no-progress` (46 occurrences, `tests/cli_contract.rs:178`
through `:4457`) and `tests/end_to_end.rs:132` uses `NoopObserver` directly. The
format gate itself has no unit test at all — it is a bare `&&` at two sites in
`main.rs`, and none of `main.rs`'s four inline tests reaches it. The most
consequential branch is the only untested one.

## 6. The indicatif renderer

**Internal state** (`progress.rs:213-231`):

| Field | Purpose |
|---|---|
| `hidden: bool` | Construction flag; short-circuits every draw path |
| `verbose: bool` | Construction flag; adds the counters to the one line |
| `multi: Option<MultiProgress>` | The single shared draw owner |
| `main: Option<ProgressBar>` | The one drawn line: phase, fraction, counters — all in its message |
| `top: Mutex<Vec<(PathBuf, u64)>>` | ≤5 largest reportable groups, sorted. **Not drawn** — `top_groups` is the only accessor |
| `last_draw: Mutex<Option<Instant>>` | The throttle's previous-draw timestamp |
| `refreshes: AtomicU64` | Draws that passed the gate; `pub` (`progress.rs:220`) |
| `visited/pruned/manifests/workspaces/discovery-errors/cargo-failures/eligible` | Display-only counters. Discovery-stage diagnostics and Cargo resolution failures remain separate totals. |
| `determinate_total/determinate_done` | The current scope's progress pair |
| `current_phase: Mutex<Option<ScanPhase>>` | Re-scoping detector |

**There is no `rows` field.** M005A §7.4 specified five transient candidate rows;
M020 retired them. See §7.

**The one line is composed, not templated.** `compose_message`
(`progress.rs:365-388`) builds a `String`: the phase label from `phase_label`
(`progress.rs:348-358`), then ` {done}/{total}` once a total is known
(`progress.rs:370-374`), then the counters when the renderer is verbose
(`progress.rs:378-387`). The result is handed to `main.set_message`
(`progress.rs:406`). Nothing else is written to the terminal.

**Throttling is three independent mechanisms, not one:** (1) `maybe_draw`'s own
100 ms wall-clock gate (`progress.rs:394-401`), the one `refreshes` counts; (2)
indicatif's `RateLimiter` built by `stderr_with_hz(10)`, 100 ms interval, burst
capacity 20 (indicatif `draw_target.rs:445-459`, `MAX_BURST = 20` at `:503`); (3)
`enable_steady_tick(100 ms)` (`progress.rs:272`), indicatif's ticker thread, which
advances the spinner independently of any event. Effective ceiling: 10 Hz with
bursts to 20. The doc comment's "Refresh capped at 10 Hz; never redrawn per
filesystem entry" (`progress.rs:210`) is accurate but understates that the
crate's own gate sits in front of indicatif's.

**`maybe_draw` calls `tick` in both modes** (`progress.rs:414`). Before M020 the
spinner froze the moment a total became known, because the tick was in the
indeterminate branch only. `main.set_position(..)` is now applied only when a
total is known (`progress.rs:415-419`).

The call does **not** advance the glyph in production. `new` installs
`enable_steady_tick(100 ms)` (`progress.rs:272`), and indicatif's `tick` is a
no-op once a ticker is installed (indicatif `progress_bar.rs:226-231`): there
the ticker thread owns the glyph. The in-memory renderer installs no ticker
(`progress.rs:282-292`), so in that mode the `tick` call is what advances it.
The two modes differ in *who* advances the spinner, not in what the operator
sees — but the test renderer therefore exercises a different mechanism from the
one production runs, and a test that asserts on the glyph is asserting on the
test renderer's mechanism.

**`set_determinate_total` (`progress.rs:322-324`) is dead code.** It stores
`determinate_total` and does nothing else: M020 removed its `main.set_length(..)`
call, so it no longer touches the bar at all. It still does **not** zero
`determinate_done`, unlike `apply_total` (`progress.rs:335-341`). Grep across
`src/` and `tests/` returns exactly one hit: its own definition. Its doc comment
claims "this concrete helper remains for direct callers" (`progress.rs:317-321`);
there are none. Because the drawn fraction is composed from those same two
atomics (`progress.rs:368-374`), it is still the one entry point that can leave
the *message* inconsistent — a total set with a done-count left over from a
previous scope. The canonical inventory of deliberately-kept dead public API is
`architecture/overview.md` §10.1; this method belongs in it.

**Over-100% is guarded on the bar, but not in the message.** `maybe_draw` clamps
at `progress.rs:418`: `main.set_position(done.min(total))`. A caller emitting more
`unit_completed` than announced units stops the *bar* at 100%. The atomic
`determinate_done` is *not* clamped, so `determinate_done_value()` can exceed
`determinate_total_value()` — visible only through the test accessor. The drawn
*message* is not clamped either: `compose_message` formats `done/total` from the
raw atomics (`progress.rs:370-374`), so an over-announced completion count
renders as `130/100` in the text while the bar's own position stops at 100%.

**`refresh_count`, `determinate_total_value`, `determinate_done_value`,
`top_groups`, `is_hidden`, `coordinated_bar_count`, `has_shared_draw_target` all
have no production caller** — they are consumed only by the module's own tests
(`progress.rs:608-616`, `:629-631`, `:764-767`, `:814`, `:825-840`, `:900`,
`:985-997`). Grep across `src/` and `tests/` finds no use of any of them outside
`src/progress.rs`. They are `pub` rather than `#[cfg(test)]` so an out-of-crate
consumer could assert on them.

**`finish_and_clear` (`progress.rs:424-434`)** calls `multi.clear()` then
`finish_and_clear()` on the main bar. Clearing matters because the
report is written with `println!` (`main.rs:1791`, `main.rs:1318`), which emits `\n` and
nothing else — any residual frame would remain interleaved above the report in
terminal history. Idempotent (`progress.rs:887-888` calls it twice) and safe when
hidden (`progress.rs:425-427`).

**`top_groups` is an observation, not a display.** "Top" means: sorted descending by
bytes with a path-ascending tiebreak, **truncated to 5**
(`progress.rs:502-505`). The sort is stable and total, so the same input yields the
same five entries in the same order. Since M020 nothing is drawn from it —
`maybe_draw` writes no rows at all — so it is purely the accessor for the largest
candidates the renderer observed, and the only thing keeping the `PathBuf` clone
and the sort alive.

**Hidden short-circuit.** `maybe_draw` returns at `progress.rs:391-393` *before*
touching the mutex or calling `Instant::now()`; `finish_and_clear` at
`progress.rs:425-427`; `coordinated_bar_count` / `has_shared_draw_target` return
`0`/`false` at `progress.rs:304-306`/`progress.rs:311-313`. `set_determinate_total`
has **no** hidden branch any more — it is a bare atomic store
(`progress.rs:322-324`), which is harmless because a hidden renderer never
composes a message. Test at `progress.rs:612-614` fires 1000 events and asserts
`refresh_count() == 0`.

**The `in_memory` feature.** `Cargo.toml:70` declares
`indicatif = { version = "0.18.6", features = ["in_memory"] }`. In indicatif 0.18.6
that is exactly `in_memory = ["vt100"]` (indicatif `Cargo.toml:55`): it pulls in the
`vt100` terminal emulator so indicatif can expose `InMemoryTerm` — a draw target
backed by an in-process `Screen` whose frames are read back as a `String` via
`contents()` (indicatif `in_memory.rs:19`, `:32`).

Why a **production** dependency, not a dev-dependency: the affordance is compiled
into the shipped library API. `new_in_memory_for_test` (`progress.rs:282`) is a
`pub fn` on a `pub struct` in `src/` returning `(Self, indicatif::InMemoryTerm)` —
it names the type in its signature, and a dev-dependency is not linked into the
library, so a `pub fn` there could not mention `InMemoryTerm` at all.
`#[cfg(test)]`-gating would mean the composition is only ever compiled under
`cargo test`, so production and test paths could silently diverge. As written,
`progress.rs:284-289` builds the *same* one-`MultiProgress` + one-bar
composition as `new` (`progress.rs:268-271`), differing only in the draw target:
the test exercises production wiring, not a parallel one.

## 7. Coordinated single-bar rendering

**The coordination mechanism is `MultiProgress::add` interception.** `new()`
creates one `MultiProgress` over a single `stderr_with_hz(10)` target
(`progress.rs:268-270`), then adds **one** bar via `multi.add(...)`
(`progress.rs:271`). Per indicatif's contract, a bar inserted with `add`
has *"the draw target changed to a remote draw target that is intercepted by the
multi progress object overriding custom `ProgressDrawTarget` settings"* (indicatif
`multi.rs:211-212`). Consequence: **no bar owns the terminal** — it reports state
to the `MultiProgress`, which is the only thing that moves the cursor. This is
what prevents the classic multi-bar failure where each bar independently rewrites
the same region. The comment at `progress.rs:265-267` states this correctly.

**The `MultiProgress` is retained even though it holds one bar** — deliberately,
so that there is exactly one owner of terminal state. `has_shared_draw_target`
(`progress.rs:310-315`) is the accessor for that fact and is still true for the
visible renderer. `coordinated_bar_count` (`progress.rs:303-308`) returns **1**
visible / 0 hidden. **Both are consumed only by the test at `progress.rs:630-631`.**
No production code reads them, so they cannot influence behaviour — the test that
uses them is the only evidence the one-bar composition is intact.

**The five candidate rows are gone.** M005A §7.4 specified them; M020 retired
them because on a real run they were empty (a spinner glyph and nothing else) for
almost the whole traversal, so the "progress" occupied six terminal lines and said
nothing. `rows: Vec<ProgressBar>` is no longer a field (`progress.rs:213-231`),
`maybe_draw` writes no rows, and `finish_and_clear` clears one bar, not six.

**The phase is the message, and that was a defect fix.** The old renderer wrote
the phase into the bar's *prefix* via `set_prefix`. indicatif's default spinner
template is `{spinner} {msg}` (indicatif `style.rs:41-42`, `default_spinner`) — it
does not render the prefix at all. The phase transitions were therefore
**invisible on the terminal**: the bar spun, but nothing said what work was
running. M020 moved the phase into the message (`progress.rs:454-463`,
`progress.rs:365-388`), and the label comes from the exhaustive `phase_label`
match (`progress.rs:348-358`).

**A phase transition does not retire the bar.** The single bar is created once in
`new()` (`progress.rs:271`) and lives for the whole process. A phase change
rewrites the *message* (`progress.rs:406`) and resets the determinate counters
(`progress.rs:326-333`); the bar object is the same throughout, so artefacts are a
non-issue *within* a run; the single lifecycle event is `finish_and_clear`.

**Terminal resize is not handled by this module.** No `SIGWINCH` handling, no
cached width, no reflow logic — nothing in `progress.rs` reads a terminal size.
Re-measurement is indicatif's job per draw. The one fixed size is the test
constant `InMemoryTerm::new(24, 120)` (`progress.rs:283`), which gives the vt100
screen a defined frame and has no production counterpart.

**Determinate and indeterminate never actually coexist.** The bar is created as
`ProgressBar::new_spinner()` (`progress.rs:271`) and never becomes a length bar:
`maybe_draw` sets a *position* only when a total is known
(`progress.rs:415-419`), and the determinate fraction is carried by the
**message** (`progress.rs:370-374`), not by the bar's own template. So the bar
spins in both modes and the numbers live in the text — which is exactly why the
fraction is visible at all under `{spinner} {msg}`.

**Capability degradation is two-layer, and only the first is visible here.** Layer
1 is the crate's own: `TERM=dumb` (`progress.rs:167`) and stderr-is-a-TTY
(`progress.rs:173`) — no `CI` check, no `NO_COLOR` check, no "is `TERM` unset"
check; grepping `src/` for `NO_COLOR` / `CI` finds nothing progress-related. Layer
2 is indicatif's own: `stderr_with_hz` returns `Self::hidden()` when
`!term.is_term() || is_dumb()` (indicatif `draw_target.rs:80-81`, `is_dumb` from
`console` at `draw_target.rs:12`).

> **Divergence worth naming.** `IndicatifRenderer::is_hidden()` (`progress.rs:295-297`)
> returns the *construction flag*, not the effective terminal capability;
> `MultiProgress::is_hidden()` (indicatif `multi.rs:321-323`) is the capability.
> They differ whenever the crate's gate and indicatif's gate disagree, and
> **nothing in this crate consults the latter.** Relatedly, the module index
> (`architecture/overview.md` §4.15) calls this module's gate "the terminal
> capability decision"; it performs *gating* detection (flag + TTY + dumb) and
> never queries what the terminal can do.

## 8. Invariants and edge cases

**Can a progress-render failure ever propagate into a scan result or change an
exit code? No — definitively.** Traced end to end:

1. No trait method returns a `Result` (`progress.rs:40-52`); nothing to propagate.
2. No caller in `src/` writes `?` against an observer call. All are bare
   statements: `discovery.rs:779`, `workspace.rs:1818`, `cleanup.rs:1867`,
   `main.rs:1715`.
3. `new` is infallible (`progress.rs:261`); the only fallible indicatif call,
   `multi.clear()`, has its `io::Result` dropped at `progress.rs:429`.
4. The exit code comes from scan truth only: `Ok(1)` iff `full_incomplete`
   (`main.rs:1911-1917`), else `Ok(0)` (`main.rs:1918-1922`). No progress value
   participates, and a failed state publish does not participate either.

`main.rs:1519-1520` is accurate, and the mechanism is exactly infallible
construction plus refresh throttling.

**Is progress output guaranteed to go to stderr and never stdout? Yes.** The only
production draw target is `stderr_with_hz` (`progress.rs:269`); the in-memory
constructor never touches fd 2. Every stdout write in the pipeline is a report
`println!` — scan at `main.rs:1776` (JSON), `:1600` (log), `:1606` (human);
cleanup at `main.rs:1280` (JSON), `:1092` (log), `:1097` (the empty-scope note),
`:1103` ("Nothing to clean."), and `:1109` / `:1119` (human). The `--stats` case
holds too: stats go to stderr (`main.rs:603`, `:1701`), as `main.rs:599-612` and
`main.rs:1886-1903` require.
Cleanup's state-generation line (`main.rs:596`) is likewise stdout, so it is
behind the same Human-only gate as the human report — and, since M020, behind a
`--verbose` gate as well (`main.rs:590-592`), because which state generation a
run used is debugging information rather than a result.

**Are the observer calls cheap enough for a hot loop? Mostly yes, one exception.**
The trait doc requires *"Implementations MUST NOT allocate per visited entry;
directory counters are aggregated in batches"* (`progress.rs:30-31`), and producers
honour it: `discovery.rs:778-780` batches `dirs_visited` at 512 entries,
`discovery.rs:843-845` batches `dirs_pruned` at 64. Per-call cost is one relaxed
`fetch_add` (`progress.rs:465`, `:469`).

The exception: **`unit_completed` takes a `Mutex` on every call** —
`reset_totals_for_phase` locks `current_phase` (`progress.rs:327`) before checking
whether the phase changed. That lock is taken by every `unit_completed` (19 sites
in `cleanup.rs:1941-2410`, 5 in `workspace.rs:2012-2128`) and every `units_total`.
Bounded by group and unit count, not entry count, so not a hot-loop problem — but
an uncontended lock per progress unit that a `load`/`store` on an `AtomicU8` phase
tag would avoid.

**Does a hidden renderer still pay observer cost? Yes, on two paths.** Counters are
never gated on `hidden` — each of the counter methods is one relaxed
`fetch_add` (`progress.rs:464-487`). Negligible, but the module doc's "the no-op
observer is effectively free" (`progress.rs:4`) describes `NoopObserver`, **not**
`IndicatifRenderer::new(true, false)`. And `reportable_group` (`progress.rs:499-508`)
increments `eligible`, takes the `top` mutex, **clones the `PathBuf`**, pushes,
sorts and truncates — *then* calls `maybe_draw`, which returns immediately. Test
`progress.rs:615-616` confirms the hidden renderer still builds its top-5
(`assert_eq!(hidden.top_groups().len(), 1)`). `cargo_failure`, `empty_skipped`,
`active_skipped` and `group_measured` are pure `maybe_draw` calls, one `hidden`
check each.

**Can the bar's counts disagree with the report? Yes, by construction.** The
renderer's counters (`progress.rs:221-227`) and `ScanCounters` (`main.rs:1759`)
are independent values fed by independent increments. The bar's `visited` counts
**every entry** — `dirs_visited` receives `batch_visited += 1` per entry including
files (`discovery.rs:772`) — while `ScanCounters.directories_visited` counts
**directories only**: *"`directories_visited` counts directories only; `visited`
keeps counting every entry (files included) as the traversal total (L6)"*
(`discovery.rs:774-775`). So one run shows "visited N" on the bar and a different,
smaller visited-entries number in the report (`main.rs:1758`), and neither labels
the other. The hardcoded `analyzing` label that used to compound this is gone:
`phase_label` (`progress.rs:348-358`) names the real phase, so a `CleanupExecute`
run reads `cleaning 3/12 …` rather than `analyzing 3/12 …`. The duplication is the
root cause; sharing one counter set removes both symptoms.

**Is `ScanPhase` exhaustively handled, so a new phase cannot be silently ignored?
Yes.** The single `match` over `ScanPhase` in the entire crate is at
`progress.rs:348-358`, exhaustive over all seven variants with **no `_` catch-all
arm**. Adding a variant is a compile error there. This is the correct choice: a
`_` arm would let a new phase render with no label and no re-scoping — exactly the
silent-ignore failure the question is about.

**Can the in-memory test renderer corrupt a real terminal? No.**
`new_in_memory_for_test` (`progress.rs:282-293`) builds
`ProgressDrawTarget::term_like_with_hz(Box::new(term.clone()), 10)` over a
vt100-backed `InMemoryTerm`. It never constructs a `console::Term` for fd 2, so it
cannot emit escape sequences to a real display even if a test leaked the renderer.

**Can a bar be left on screen after an early return? On the known error paths, no
— and the property is only half pinned.** The C023 work changed the shape of this
question. `finish_and_clear()` is now called on the failure paths, not only on
success: the scan path clears the renderer before returning the discovery error
(`main.rs:1537`) and before returning the recency-window error (`main.rs:1537`),
and the cleanup path clears **before** the fallible cleanup call
(`main.rs:588` before `main.rs:589`) rather than after it. The reason is
recorded twice, at `main.rs:577-580` (cleanup) and `main.rs:1603-1607` (scan): a
`?` escaping with the bar still live lets indicatif's `impl Drop for BarState`
(indicatif `state.rs:219-232`) *finish* the bar rather than clear it, so the
retained line stayed on screen above the `cargo-cleanme: …` message.

What is **not** pinned is the ordering itself. It lives in `main.rs`, which has no
tests, and it is unreachable from `src/progress.rs`: reverting it leaves every
test green, because the bar only draws on an attended terminal and the
failing-cleanup contract test passes `--no-progress`. Pinning it needs a pty
harness and a new dev-dependency. This is recorded as an open coverage gap in
`plans/closure/distribution-release-update/c023-status.md` §11.4. What
`src/progress.rs` does now test is the renderer half it relies on: that a
`finish_and_clear` leaves **no live bar** for a later error or a drop
(`progress.rs:949-1006`). A panic is still uncovered — indicatif leaves partial
frames and the prompt resumes mid-frame with no newline — and a `Drop` impl on
`IndicatifRenderer` would close that deterministically.

## 9. Testing

**Real numbers:** 15 `#[test]` functions, confirmed by grep. The test module opens
at `progress.rs:596-597`. The table cites each test's `fn` line.

| # | Test | Line | Protects |
|---|---|---|---|
| 1 | `noop_observer_is_free_and_hidden_renderer_emits_nothing` | `:601` | Hidden short-circuit; `NoopObserver` total no-op |
| 2 | `coordinated_renderer_owns_exactly_one_bar_on_one_draw_target` | `:627` | One bar + shared draw target |
| 3 | `in_memory_frame_contains_exactly_one_progress_line_naming_the_phase` | `:644` | One-line frame; the phase is on it; the fraction is on it |
| 4 | `the_verbose_surface_adds_counters_to_the_same_single_line` | `:694` | Verbose counters ride the same line |
| 5 | `the_quiet_surface_carries_no_counters` | `:734` | Quiet surface carries no counters |
| 6 | `renderer_caps_the_observed_candidates_at_five_largest_first` | `:759` | Top-5 truncation + descending sort of the *observed* candidates |
| 7 | `test_observer_records_semantic_events` | `:771` | `TestObserver` bookkeeping |
| 8 | `should_show_progress_respects_flag_and_dumb_term` | `:789` | Gate conditions 1 and 2 |
| 9 | `renderer_refresh_bounded_independently_of_entry_count` | `:805` | Zero refreshes when hidden |
| 10 | `discovery_begins_indeterminate_then_analysis_determinate` | `:818` | Indeterminate→determinate switch; `group_measured` does not advance |
| 11 | `unit_events_rescope_totals_on_a_phase_change_without_an_explicit_phase_call` | `:844` | M8: event-carried phase re-scopes |
| 12 | `phase_total_resets_so_cleanup_cannot_reuse_analysis_counts` | `:863` | Phase change zeroes total and done |
| 13 | `sizes_render_in_top_rows_and_clear_before_report` | `:879` | Candidate list; `format_bytes`; idempotent clear |
| 14 | `renderer_error_falls_back_without_failing_scan` | `:892` | Hidden fallback is a no-op |
| 15 | `finish_and_clear_leaves_no_live_bar_for_a_later_error_or_drop` | `:950` | The clear leaves no live bar and no redraw for a later error |

`TestObserver` (`progress.rs:64-80`) records semantic events: phase sequence and
totals in `Mutex<Vec<_>>`, counters in `AtomicU64`, the full reportable list in
`reportable: Mutex<Vec<(PathBuf, u64)>>`. `total_for` (`progress.rs:87-95`)
searches **in reverse** for the last total announced for a phase, which is what
makes it useful for the re-announcement case; `completed_count`
(`progress.rs:97-104`) filters the full event list.

**What `TestObserver` cannot observe** is the important half. It sees the *event
stream the producers emitted*, not what the renderer did with those events. It
cannot detect whether a bar was created, whether a draw occurred, whether a phase
label was applied, whether a size string was formatted, whether the frame stayed
within one line, whether the terminal was cleared, or whether the bar exceeded
100%. Its own doc comment draws the line — *"Refresh counting is renderer-side;
this records semantic events only"* (`progress.rs:63`). Tests 10, 11 and 12
protect the *contract*; only tests 2, 3, 4, 5 and 15 touch the *renderer*.

**Do the tests assert on rendered output or only observer state? Both, and
rendering is covered by four tests.** Test 3 (`progress.rs:643-686`) reads
`term.contents()` and asserts that the composed frame carries at most one
non-empty line (`progress.rs:659-664`), that it names the phase
(`progress.rs:665-668`), that it carries the done fraction once the total is
known (`progress.rs:669-672`), and that twenty further refreshes do not scroll the
buffer (`progress.rs:673-684`). Tests 4 and 5 (`progress.rs:694-727`,
`:734-751`) pin the verbose/quiet split on that same one-line frame: the verbose
line carries `discovery-errors 7` and `cargo-failures 2`, the quiet line carries
neither. Test 15 (`progress.rs:949-1006`) asserts that the bar really drew and
was live before the clear (`progress.rs:957-965`), that the clear empties the
frame (`progress.rs:973-977`), and that an error raised after the clear neither
resurrects a live bar nor redraws (`progress.rs:984-997`). Every other test
inspects accessors or
`TestObserver` state. So: **rendering is observable via the `in_memory` feature,
but only as message substrings, a line count, and frame emptiness.**

The gap is narrower than it was, and it is concrete. M020 closed the worst of it:
the phase label is now asserted (`progress.rs:665-668`) and the done fraction
rides the same assertion (`progress.rs:669-672`). What is still unasserted is
indicatif's own **position** — `main.set_position(done.min(total))`
(`progress.rs:418`) is never compared against the drawn frame, so a renderer that
painted the right message with a wrong bar position would pass every current
test. And only one of the seven labels (`analyzing output`) is exercised; the
other six are unasserted. `format_bytes` no longer appears in any rendered output
at all — the renderer never renders a size — so the shared-formatter property now
lives in `interactive.rs` and in the report, not here.

**Three tests are misnamed relative to what they assert.**
`sizes_render_in_top_rows_and_clear_before_report` (`progress.rs:879`) is the
first: there are no rows any more. The test still checks that the observed
candidates are kept, that the shared byte formatter is the one in use, and that
the clear is idempotent — but "in top rows" names a surface M020 deleted, and the
name should follow what the test now protects.
And `renderer_error_falls_back_without_failing_scan` (`progress.rs:892`) injects no
error — there is no error channel to inject one into. It drives a hidden renderer
and asserts `refresh_count() == 0`, so it cannot fail if error handling
regressed, because error handling has no runtime surface; its comment
(`progress.rs:893-894`) concedes as much. And
`renderer_refresh_bounded_independently_of_entry_count` (`progress.rs:805`) has a
comment (`progress.rs:806-809`) claiming it asserts "that visible construction does
not allocate per entry", but the body (`progress.rs:810-814`) only ever constructs
a **hidden** renderer: the visible 10 Hz throttle is never measured, and the test
proves `0 == 0`.

**Platform risk.** The only environment-sensitive test is
`should_show_progress_respects_flag_and_dumb_term` (`progress.rs:788-802`), which
mutates the process-global `TERM` through the shared `test_env` guard
(`progress.rs:794-801`), whose `set` does the `unsafe { std::env::set_var }`
(`progress.rs:565-568`) — correctly wrapped, which edition 2024 /
`rust-version = "1.89"` requires (`Cargo.toml:4-5`). It asserts only the flag and
`TERM=dumb` branches, so it is **not** TTY-dependent and is safe on CI and
Windows. But `TERM` is process-global and `cargo test` runs tests in parallel
threads: the guard serialises *mutations* of `TERM` against each other
(`progress.rs:540-546`), but a test that only *reads* `TERM` — or lets
indicatif/`console` read it — does not take the lock, so the race against such a
reader remains. No other test in this file touches `TERM`, so it is contained
today.

**Coverage by the fixture checker.** `scripts/check-fixture-portability.py`
contains **no reference to `progress.rs` or to `progress` at all** (grepped
directly). Not covered by `tests/cli_contract.rs` either, which always passes
`--no-progress` and asserts only that `--stats` lands on stderr
(`tests/cli_contract.rs:198`), nor by `tests/end_to_end.rs`, which imports
`progress` solely for `NoopObserver` (`:10`, `:132`).

## 10. Review checklist

1. **Stdout/stderr separation intact.** The only production draw target is
   `stderr_with_hz` (`progress.rs:269`); every report write is a `println!`
   (`main.rs:1776`, `:1600`, `:1606`, `:1090`, `:1119`). A new code path passing
   `MultiProgress::new()` or any other target breaks pipeability silently.
2. **The TTY check is stderr-only and must stay that way.**
   `stderr_is_terminal` (`progress.rs:189-192`) inspects fd 2 and never fd 1.
   Adding a stdout check would suppress progress for `cleanme scan | jq` — the
   case the separation exists to support.
3. **The format gate at `main.rs:1516-1517` and `main.rs:567-568` is duplicated,
   not shared.** Both must be edited together or `scan` and `clean` will disagree;
   the `!show` inversion at `main.rs:1518` / `main.rs:569` is easy to drop, and
   so is the second argument — `options.verbose` must be threaded to both, or the
   two paths render different surfaces. Adding a fourth `OutputFormat` variant
   means touching both, and a variant added without them would silently inherit
   "progress allowed".
4. **No trait method may gain a `Result`, and no observer call may take a `?`.**
   The "progress can never fail a scan" guarantee is currently *type-level*
   (`progress.rs:40-52`); it becomes a convention the moment one signature
   changes. Preserve the `let _ =` at `progress.rs:429`.
5. **`ScanPhase` is exhaustively matched at `progress.rs:348-358` with no
   catch-all — keep it that way.** A `_` arm would let a new phase render unlabelled
   and un-rescoped with no compiler complaint.
6. **The >100% guard is `done.min(total)` at `progress.rs:418`, not the atomic.**
   `determinate_done` is intentionally unclamped; any refactor letting the atomic
   drive `set_position` directly pushes the bar's position past the end of the
   scale. Note that the *message* is not clamped either (`progress.rs:370-374`),
   so it will still read `130/100` — the guard covers the bar, not the text.
7. **The bar's counters and the report's already disagree.** Bar `visited` counts
   all entries (`discovery.rs:772`); `ScanCounters.directories_visited` counts
   directories only (`discovery.rs:774-775`). The `analyzing` literal is gone —
   the label comes from `phase_label` (`progress.rs:348-358`) — but the duplicated
   counter set is not. Fixing that means sharing one counter set, not patching
   the label.
8. **The error paths now clear the bar — and the ordering is still unpinned.**
   `main.rs:1537` and `main.rs:1537` clear before the scan error returns, and
   `main.rs:588` clears before the fallible cleanup call at `main.rs:589`. Reverting
   that ordering leaves every test green (C023 §11.4). A `Drop` impl on
   `IndicatifRenderer` would make the failure path deterministic without a pty
   harness.
9. **`set_determinate_total` (`progress.rs:322`) has zero callers and is
   inconsistent with `apply_total`** (it does not zero `determinate_done`). Delete
   it or route it through `apply_total`; do not leave a second way to set a total.
10. **The observability gap is the frame *content*.** Tests 3, 4, 5 and 15
    (`progress.rs:643-686`, `:694-727`, `:734-751`, `:949-1006`) read
    `term.contents()` and assert message substrings, a line count, and frame
    emptiness — but nothing asserts on the bar's *position* (`progress.rs:418`) or
    on any phase label other than `analyzing output`. Asserting those is nearly
    free and would catch position and label regressions that all fifteen current
    tests miss.
