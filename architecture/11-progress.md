# Progress — observing work without owning the terminal

> Component deep dive · part of the [architecture overview](overview.md)

`src/progress.rs` — 873 lines, 533 production (`progress.rs:534` opens the test
module), 13 `#[test]` functions.

## 1. Responsibility

Owns **transient terminal feedback on stderr** and nothing else: the decision of
*whether* to draw, the phase vocabulary, the trait producers emit through, and the
six-bar `indicatif` composition.

Does **not** own:

- **The final report** — that is stdout, via `report::render` (`main.rs:1421`) or
  `output::scan` (`main.rs:1409`) or `output::log::scan` (`main.rs:1418`).
  `Reporting` (`progress.rs:21`) is announced (`main.rs:1393`) then immediately
  followed by `finish_and_clear()` (`main.rs:1394`); the prefix is set and a draw
  is requested, but the clear lands before any `Reporting` frame can survive on
  screen, so no `Reporting` frame is ever seen.
- **Any domain truth.** Every number the renderer shows is a side effect of an
  observer call; the report's numbers come from `ScanCounters`, carried in from
  discovery at `main.rs:1210` and written into the report at `main.rs:1390`.
  Independent counter sets — see §8.
- **Phase sequencing.** `cleanup.rs:1222-1226` maps `CleanMode` → `ScanPhase`; the
  renderer reacts to whatever phase a caller names.

**The stdout/stderr separation is a contract, verified on three axes:**

| Axis | Evidence |
|---|---|
| Draw target is fd 2 | `ProgressDrawTarget::stderr_with_hz(10)` (`progress.rs:267`) |
| Report is fd 1 | `println!` for human (`main.rs:1421`), JSON (`main.rs:1407`), log (`main.rs:1416`), and both cleanup renderers (`main.rs:1028`, `:899`) |
| Clear precedes report | `finish_and_clear()` at `main.rs:1394` (scan), `main.rs:467` (cleanup) |

The user-facing consequence: `cargo cleanme scan | jq` shows progress on the
terminal and keeps stdout pipeable. §5 shows the TTY check is on **stderr only**,
which is exactly the axis that preserves this.

**Dependency footprint is one item.** The only `crate::` reference in the
production half is `crate::report::format_bytes` (`progress.rs:195`), used for the
five candidate-row sizes at `progress.rs:413` — sharing the formatter with the
report so a size never renders in two notations within one run. Everything else is
`std` (`progress.rs:7-14`) or `indicatif`. `progress.rs:835` is test-only.

> **Correction to the layer map.** The layer map (`architecture/overview.md` §5)
> has drawn an edge from `progress` down to `traverse (measurement)`. There is
> **no** `crate::traverse` reference anywhere in `progress.rs`, production or test.
> The only real crate edge out of `progress.rs` is `progress → report`
> (`progress.rs:195`), and the diagram omits that one.

## 2. Public surface

`ScanPhase` — `progress.rs:16-25`. `Clone + Copy + Debug + Eq + PartialEq`.

| Variant | Work it represents | Entered from | Total known in advance? |
|---|---|---|---|
| `Discovery` | Filesystem walk for `Cargo.toml` | `main.rs:1184` | No — indeterminate |
| `Resolution` | `cargo metadata` per manifest | `main.rs:1231` | No — no `units_total` ever |
| `Analysis` | Parallel sizing + activity classification | `main.rs:1345-1346`, total at `workspace.rs:1619` | Yes — determinate |
| `Reporting` | About to write the stdout report | `main.rs:1393` | No — never drawn |
| `CleanupPreview` | Cargo's own `clean --dry-run` preview | `cleanup.rs:1223` | Yes — `cleanup.rs:1409` |
| `CleanupSimulate` | Application simulation, no Cargo spawned | `cleanup.rs:1224` | Yes — `cleanup.rs:1409` |
| `CleanupExecute` | Real cleanup through Cargo | `cleanup.rs:1225` | Yes — `cleanup.rs:1409` |

`ProgressObserver` — `progress.rs:39-53`. `Send + Sync`. All thirteen methods take
`&self` and **all thirteen have default empty bodies**, so an implementor overrides
only what it needs; `impl ProgressObserver for NoopObserver {}` (`progress.rs:59`)
is a complete, allocation-free implementation.

| Method | Argument | Meaning | Phases that use it |
|---|---|---|---|
| `phase(ScanPhase)` | new phase | Announce a transition; re-scopes the determinate scope | All four scan + all three cleanup phases |
| `dirs_visited(u64)` | batch delta | Entries walked, batched at 512 (`discovery.rs:671-673`) | `Discovery` |
| `dirs_pruned(u64)` | batch delta | Entries cut by prune rules, batched at 64 (`discovery.rs:736-738`) | `Discovery` |
| `manifests_found(u64)` | batch delta | Validated `Cargo.toml` files (`discovery.rs:733`) | `Discovery` |
| `discovery_issues(u64)` | final count | Discovery-stage diagnostics, kept separate from Cargo resolution failures (`main.rs:1203`) | `Discovery` → `Resolution` |
| `workspaces_resolved(u64)` | batch delta | Successful `cargo metadata` runs (`workspace.rs:971`) | `Resolution` |
| `cargo_failure()` | — | One Cargo workspace-resolution command failed, at either locate or metadata (`workspace.rs:850`, `:795`, `:810`, `:822`, `:879`, `:1039`, `:1061`, `:1085`) | `Resolution` |
| `empty_skipped()` | — | Group skipped: no eligible bytes (`workspace.rs:1645`, `1784`) | `Analysis` |
| `active_skipped()` | — | Group or workspace skipped: protected by source activity (`workspace.rs:1703`, `1797`) | `Analysis` |
| `group_measured(u64)` | bytes | A group finished sizing (`workspace.rs:1876`) | `Analysis` |
| `reportable_group(&Path, u64)` | path, bytes | A group qualifies for the report (`workspace.rs:1905`, `cleanup.rs:2245`) | `Analysis`, cleanup |
| `units_total(ScanPhase, u64)` | phase, total | Announce the unit count for a phase | `Analysis` (`workspace.rs:1619`), cleanup (`cleanup.rs:1409`) |
| `unit_completed(ScanPhase)` | phase | One unit finished | `Analysis` (`workspace.rs:1805-1877`), cleanup (19 sites, `cleanup.rs:1474-1923`) |

`active_skipped` is deliberately counted per **workspace** at
`workspace.rs:1703` and per **group** at `workspace.rs:1864`; the source comment
at `workspace.rs:1699-1702` records why the workspace count must not be added to
the group counter.

Everything else:

| Item | Signature | Contract |
|---|---|---|
| `NoopObserver` | `pub struct NoopObserver` (`:57`) | Zero-cost observer; the crate's default in unit tests (152 `NoopObserver` references against 15 `TestObserver` ones across `src/` and `tests/`) |
| `TestObserver::new` | `-> Self` (`:83`) | `Default` constructor |
| `TestObserver::total_for` | `(phase: ScanPhase) -> Option<u64>` (`:87`) | **Last** total announced for a phase, or `None` |
| `TestObserver::completed_count` | `(phase: ScanPhase) -> usize` (`:97`) | Count of `unit_completed` events for a phase |
| `should_show_progress` | `(no_progress: bool) -> bool` (`:163`) | Gate; see §5 |
| `IndicatifRenderer::new` | `(hidden: bool) -> Self` (`:259`) | **Inverted** flag: `true` means *hidden* |
| `…::new_in_memory_for_test` | `-> (Self, InMemoryTerm)` (`:285`) | Same composition, vt100-backed screen |
| `…::is_hidden` | `-> bool` (`:299`) | Construction flag, not terminal capability |
| `…::coordinated_bar_count` | `-> usize` (`:307`) | 6 visible / 0 hidden |
| `…::has_shared_draw_target` | `-> bool` (`:314`) | `multi.is_some()` |
| `…::set_determinate_total` | `(&self, total: u64)` (`:326`) | **No callers anywhere** — §6 |
| `…::finish_and_clear` | `(&self)` (`:422`) | Wipes the transient region; returns `()` |
| `…::refresh_count` | `-> u64` (`:437`) | Draws that passed the 100 ms gate |
| `…::top_groups` | `-> Vec<(PathBuf, u64)>` (`:441`) | Clone of the ≤5-entry candidate list |
| `…::determinate_total_value` | `-> u64` (`:445`) | Current scope's total |
| `…::determinate_done_value` | `-> u64` (`:449`) | Current scope's completed count |

`IndicatifRenderer::refreshes` is the one public *field* (`progress.rs:218`); the
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
(`progress.rs:506-508`) and never touches `determinate_done`; `active_skipped` is
likewise draw-only (`progress.rs:501-505`). Asserted at `progress.rs:702-703`:
after `group_measured(100)`, `determinate_done_value()` is still `0`; only
`unit_completed` moves it (`progress.rs:704-705`).

**`Send + Sync` (`progress.rs:39`) is a contract, and today no call site
exercises it across a thread.** The observer is threaded as a
`&dyn ProgressObserver` (`main.rs:1183`, `discovery.rs:325`, `workspace.rs:464`),
but every observer call in the crate happens on the calling thread. The
`dua_core::walk_roots` pool (`discovery.rs:578`) is handed the *descend filter*
closure (`discovery.rs:587`), not the observer; the observer is called from the
consumer loop at `discovery.rs:634`. The bounded-parallel Cargo resolution
(`workspace.rs:488-503`) likewise keeps the observer out of its workers. So
`Send + Sync` is forward-looking permission, not a bound the current code needs —
and it is exactly why the trait is also the reason every method takes `&self`:
shared counters under `&self` require interior mutability, `AtomicU64` for the
counters, `Mutex` for the candidate list, the phase, and the draw timestamp
(`progress.rs:216-228`).

**Why a trait and not a direct `indicatif` call.** Two independent reasons.
**Testability:** `discovery`, `workspace` and `cleanup` are tested with
`NoopObserver` (152 references across `src/` and `tests/`; `cleanup.rs` alone
names it 81 times) and with
`TestObserver` for event assertions (`cleanup.rs:7790`, `:7798`, `:8362`;
`workspace.rs:4797`, `:5142`); without a trait every one of those tests would need
a real terminal or a global renderer hook. **Multiple independent producers, one
renderer:** the scan and cleanup paths each build a single `IndicatifRenderer`
(`main.rs:448`, `main.rs:1180`) and never learn about one another — none of
`discovery`, `workspace`, `cleanup` imports `indicatif` (the only `indicatif` use
in `src/` is in this file), and none knows whether the renderer is a spinner, a TTY
bar, a test double, or absent. A third producer exists since M014: the
interactive candidate listing runs on its own **hidden** renderer
(`main.rs:567`), because a live bar would fight the repainting selection list.

**"The observer must never be able to fail a scan" — honoured.** Enforced at four
levels, and `main.rs:1181-1182` is accurate. **No fallible signature exists:**
every method returns `()` (`progress.rs:40-52`), so there is nothing to propagate
and no `?` is ever written against an observer. **Construction is infallible:**
`new` returns `Self`, not `Result` (`progress.rs:259`) — a terminal that cannot be
drawn to is the `hidden` branch, not an error (`progress.rs:260-262`).
**The one fallible indicatif call is discarded:** `finish_and_clear` does
`let _ = multi.clear();` (`progress.rs:427`). **Throttling bounds the cost**, so
progress cannot dominate a fast scan.

The comment does *not* claim a runtime recovery path, because there is none to
need: degradation happens once, at construction, via the caller's boolean.

## 4. Phases

**Indeterminate: `Discovery`, `Resolution`, `Reporting`.** No `units_total` is ever
emitted for these. `Discovery` cannot have one without a pre-scan
(`progress.rs:689-693` asserts the total must be `0`, with the reason in the assert
message). `Resolution` has no cheap cardinality — one subprocess per manifest,
failures unknown until run. `Reporting` is never left on screen.

**Determinate: `Analysis` and the three cleanup phases.** `Analysis` gets its
total from `workspace::analyze_groups` at `workspace.rs:1619` (and redundantly from
`main.rs:1346`). Cleanup gets its total at `cleanup.rs:1409`, gated on complete
ownership coverage — the comment at `cleanup.rs:1407-1408` is explicit that a
partial total would be a lie.

**Re-scoping semantics.** `main.rs:1343` says *"Phase first (re-scopes totals),
then total (starts fresh scope)"*. Verified against the renderer; the two steps
are distinct operations:

- `phase(p)` → `reset_totals_for_phase(p)` (`progress.rs:456`, `:336-349`): if `p`
  differs from `current_phase`, zero both determinate counters and set the bar to
  `set_length(0); set_position(0)`.
- `units_total(p, n)` → `reset_totals_for_phase(p)` **again** (`progress.rs:523`),
  then `apply_total(n)` (`progress.rs:351-364`): store `n`, zero
  `determinate_done`, `set_length(n); set_position(0)`.

The second reset is idempotent (same phase, so the `if` at `progress.rs:338` does
not fire) and exists as defence in depth: `units_total` re-scopes on the phase
carried *by the event itself*, not only on an explicit `phase()` call. That is the
M8 fix named at `progress.rs:520-522`, tested at `progress.rs:708-725`.

**User-visible meaning:** a phase change visibly snaps the bar to 0% rather than
carrying a partial fill into unrelated work. Asserted at `progress.rs:734-736`
(`Analysis` at 1/5, then `CleanupSimulate` → total `0`, done `0`).

## 5. Deciding whether to show progress

**Two gates, and they are different kinds of thing.** A call site must satisfy
both before a renderer exists at all.

1. **Format gate** — `main.rs:1178-1179` (scan) and `main.rs:446-447` (cleanup),
   both `options.format == OutputFormat::Human && progress::should_show_progress(no_progress)`.
2. **Construction gate** — `IndicatifRenderer::new(!show)` at `main.rs:1180` and
   `main.rs:448`. The renderer's `hidden` flag is the *negation* of the gate, so
   `show` being false produces a renderer whose every draw path short-circuits.

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

**Both non-Human formats suppress progress on a TTY** (`main.rs:1178-1179`,
`main.rs:446-447`). JSON is a deliberate choice: a machine-readable consumer should get
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
also drops the scan's per-diagnostic stderr fan-out (`main.rs:1426`, gated on
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
restore of the process-global `TERM` is an RAII guard (`progress.rs:654-667`), so
a panic mid-test cannot leave `TERM=dumb` behind for every later test in the
binary.
`should_show_progress_respects_flag_and_dumb_term`
(`progress.rs:635-652`) covers conditions 1 and 2. Condition 3 — the TTY branch —
is **never exercised by any test in the crate**: every integration case that runs
a scan or cleanup passes `--no-progress` (42 occurrences, `tests/cli_contract.rs:178`
through `:3968`) and `tests/end_to_end.rs:132` uses `NoopObserver` directly. The
format gate itself has no unit test at all — it is a bare `&&` at two sites in
`main.rs`, and none of `main.rs`'s four inline tests reaches it. The most
consequential branch is the only untested one.

## 6. The indicatif renderer

**Internal state** (`progress.rs:211-229`):

| Field | Purpose |
|---|---|
| `hidden: bool` | Construction flag; short-circuits every draw path |
| `multi: Option<MultiProgress>` | The single shared draw owner |
| `main: Option<ProgressBar>` | Phase label + determinate bar + counter line |
| `rows: Vec<ProgressBar>` | Exactly 5; **positional, not keyed** — `rows[i]` is display slot *i*, nothing more |
| `top: Mutex<Vec<(PathBuf, u64)>>` | ≤5 largest reportable groups, sorted |
| `last_draw: Mutex<Option<Instant>>` | The throttle's previous-draw timestamp |
| `refreshes: AtomicU64` | Draws that passed the gate; `pub` (`progress.rs:218`) |
| `visited/pruned/manifests/workspaces/discovery-errors/cargo-failures/eligible` | Display-only counters. Discovery-stage diagnostics and Cargo resolution failures remain separate totals. |
| `determinate_total/determinate_done` | The current scope's progress pair |
| `current_phase: Mutex<Option<ScanPhase>>` | Re-scoping detector |

**Throttling is three independent mechanisms, not one:** (1) `maybe_draw`'s own
100 ms wall-clock gate (`progress.rs:374-380`), the one `refreshes` counts; (2)
indicatif's `RateLimiter` built by `stderr_with_hz(10)`, 100 ms interval, burst
capacity 20 (indicatif `draw_target.rs:445-459`, `MAX_BURST = 20` at `:495`); (3)
`enable_steady_tick(100 ms)` (`progress.rs:270`), indicatif's ticker thread, which
advances the spinner independently of any event. Effective ceiling: 10 Hz with
bursts to 20. The doc comment's "Refresh capped at 10 Hz; never redrawn per
filesystem entry" (`progress.rs:208`) is accurate but understates that the
crate's own gate sits in front of indicatif's.

**`set_determinate_total` (`progress.rs:326-334`) is dead code.** It stores
`determinate_total` and calls `main.set_length(total)` — but does **not** zero
`determinate_done`, unlike `apply_total` (`progress.rs:351-364`). Grep across
`src/` and `tests/` returns exactly one hit: its own definition. Its doc comment
claims "this concrete helper remains for direct callers" (`progress.rs:323-325`);
there are none. It is also the one entry point that could produce an inconsistent
state (total set, done left over from a previous scope). The canonical inventory
of deliberately-kept dead public API is `architecture/overview.md` §10.1; this
method belongs in it.

**Over-100% is guarded.** `maybe_draw` clamps at `progress.rs:405`:
`main.set_position(done.min(total))`. A caller emitting more `unit_completed` than
announced units stops the bar at 100% rather than rendering `130/100`. The atomic
`determinate_done` is *not* clamped, so `determinate_done_value()` can exceed
`determinate_total_value()` — visible only through the test accessor.

**`refresh_count`, `determinate_total_value`, `determinate_done_value`,
`top_groups`, `is_hidden`, `coordinated_bar_count`, `has_shared_draw_target` all
have no production caller** — they are consumed only by the module's own tests
(`progress.rs:546-554`, `:562-563`, `:611`, `:679`, `:747`, `:765`, `:851`).
Grep across `src/` and `tests/` finds no use of any of them outside
`src/progress.rs`. They are `pub` rather than `#[cfg(test)]` so an out-of-crate
consumer could assert on them.

**`finish_and_clear` (`progress.rs:422-435`)** calls `multi.clear()` then
`finish_and_clear()` on the main bar and each row. Clearing matters because the
report is written with `println!` (`main.rs:1421`, `:1234`), which emits `\n` and
nothing else — any residual frame would remain interleaved above the report in
terminal history. Idempotent (`progress.rs:752-753` calls it twice) and safe when
hidden (`progress.rs:423-425`).

**`top_groups` is a count, not a sort key.** "Top" means: sorted descending by
bytes with a path-ascending tiebreak, **truncated to 5**
(`progress.rs:513-515`). The sort is stable and total, so the same input yields the
same five rows in the same order. `maybe_draw` writes `rows[i]` from `top[i]` and
blanks surplus slots (`progress.rs:411-418`).

**Hidden short-circuit.** `maybe_draw` returns at `progress.rs:371-373` *before*
touching the mutex or calling `Instant::now()`; `set_determinate_total` returns at
`progress.rs:328-330` after its atomic store; `finish_and_clear` at
`progress.rs:423-425`; `coordinated_bar_count` / `has_shared_draw_target` return
`0`/`false` at `progress.rs:308-310`/`progress.rs:315-317`. Test at
`progress.rs:550-552` fires 1000 events and asserts `refresh_count() == 0`.

**The `in_memory` feature.** `Cargo.toml:70` declares
`indicatif = { version = "0.18.6", features = ["in_memory"] }`. In indicatif 0.18.6
that is exactly `in_memory = ["vt100"]` (indicatif `Cargo.toml:55`): it pulls in the
`vt100` terminal emulator so indicatif can expose `InMemoryTerm` — a draw target
backed by an in-process `Screen` whose frames are read back as a `String` via
`contents()` (indicatif `in_memory.rs:19`, `:32`).

Why a **production** dependency, not a dev-dependency: the affordance is compiled
into the shipped library API. `new_in_memory_for_test` (`progress.rs:285`) is a
`pub fn` on a `pub struct` in `src/` returning `(Self, indicatif::InMemoryTerm)` —
it names the type in its signature, and a dev-dependency is not linked into the
library, so a `pub fn` there could not mention `InMemoryTerm` at all.
`#[cfg(test)]`-gating would mean the composition is only ever compiled under
`cargo test`, so production and test paths could silently diverge. As written,
`progress.rs:287-296` builds the *same* one-`MultiProgress` + main + 5-rows
composition as `new` (`progress.rs:266-277`), differing only in the draw target:
the test exercises production wiring, not a parallel one.

## 7. Coordinated multi-bar rendering

**The coordination mechanism is `MultiProgress::add` interception.** `new()`
creates one `MultiProgress` over a single `stderr_with_hz(10)` target
(`progress.rs:266-268`), then adds the main bar and five rows via `multi.add(...)`
(`progress.rs:269`, `:274`). Per indicatif's contract, a bar inserted with `add`
has *"the draw target changed to a remote draw target that is intercepted by the
multi progress object overriding custom `ProgressDrawTarget` settings"* (indicatif
`multi.rs:211-212`). Consequence: **no bar owns the terminal** — they report state
to the `MultiProgress`, which is the only thing that moves the cursor. This is
what prevents the classic multi-bar failure where each bar independently rewrites
the same region. The comment at `progress.rs:263-265` states this correctly.

**`coordinated_bar_count` (`progress.rs:307-312`) and `has_shared_draw_target`
(`progress.rs:314-319`)** assert that structure: 6 bars visible / 0 hidden, and
`multi.is_some()`. **Both are consumed only by the tests at `progress.rs:548-549`
and `:562-563`.** No production code reads them, so they cannot influence
behaviour — the tests that use them are the only evidence the six-bar composition
is intact.

**A phase transition does not retire bars.** All six are created once in `new()`
(`progress.rs:269-276`) and live for the whole process. A phase change rewrites the
main bar's prefix (`progress.rs:470`) and its length/position
(`progress.rs:345-346`); the five rows keep their text until the next draw
re-labels them. The visual effect reads as "one bar became another", but the bar
objects are the same six throughout, so artefacts are a non-issue *within* a run;
the single lifecycle event is `finish_and_clear`.

**Terminal resize is not handled by this module.** No `SIGWINCH` handling, no
cached width, no reflow logic — nothing in `progress.rs` reads a terminal size.
Re-measurement is indicatif's job per draw. The one fixed size is the test
constant `InMemoryTerm::new(24, 120)` (`progress.rs:286`), which gives the vt100
screen a defined frame and has no production counterpart.

**Determinate and indeterminate never actually coexist.** The main bar is created
as `ProgressBar::new_spinner()` (`progress.rs:269`) and becomes determinate purely
by `set_length` + `set_position` (`progress.rs:345-346`, `:361-362`), with
`maybe_draw` choosing per draw (`progress.rs:404-408`): `set_position(done.min(total))`
when `determinate_total > 0`, else `tick()`. The five rows are *always* spinners
(`progress.rs:274`) and only ever receive `set_message` — never a position or a
length (`progress.rs:411-418`). So there is one bar flipping mode and five
that are pure text, not two objects on one screen.

**Capability degradation is two-layer, and only the first is visible here.** Layer
1 is the crate's own: `TERM=dumb` (`progress.rs:167`) and stderr-is-a-TTY
(`progress.rs:173`) — no `CI` check, no `NO_COLOR` check, no "is `TERM` unset"
check; grepping `src/` for `NO_COLOR` / `CI` finds nothing progress-related. Layer
2 is indicatif's own: `stderr_with_hz` returns `Self::hidden()` when
`!term.is_term() || is_dumb()` (indicatif `draw_target.rs:80-81`, `is_dumb` from
`console` at `draw_target.rs:12`).

> **Divergence worth naming.** `IndicatifRenderer::is_hidden()` (`progress.rs:299-301`)
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
   statements: `discovery.rs:672`, `workspace.rs:1619`, `cleanup.rs:1409`,
   `main.rs:1346`.
3. `new` is infallible (`progress.rs:259`); the only fallible indicatif call,
   `multi.clear()`, has its `io::Result` dropped at `progress.rs:427`.
4. The exit code comes from scan truth only: `Ok(1)` iff `full_incomplete`
   (`main.rs:1534-1540`), else `Ok(0)` (`main.rs:1541-1545`). No progress value
   participates, and a failed state publish does not participate either.

`main.rs:1181-1182` is accurate, and the mechanism is exactly infallible
construction plus refresh throttling.

**Is progress output guaranteed to go to stderr and never stdout? Yes.** The only
production draw target is `stderr_with_hz` (`progress.rs:267`); the in-memory
constructor never touches fd 2. Every stdout write in the pipeline is a report
`println!` — scan at `main.rs:1407`, `:1243`, `:1248` and cleanup at `main.rs:1028`,
`:894`, `:899`, plus the empty-scope note at `:909`. The `--stats` case holds
too: stats go to stderr (`main.rs:479`, `:1338`), as `main.rs:475-488` and
`main.rs:1509-1526` require.
Cleanup's state-generation line (`main.rs:472`) is likewise stdout, so it is
behind the same Human-only gate as the human report.

**Are the observer calls cheap enough for a hot loop? Mostly yes, one exception.**
The trait doc requires *"Implementations MUST NOT allocate per visited entry;
directory counters are aggregated in batches"* (`progress.rs:30-31`), and producers
honour it: `discovery.rs:671-673` batches `dirs_visited` at 512 entries,
`discovery.rs:736-738` batches `dirs_pruned` at 64. Per-call cost is one relaxed
`fetch_add` (`progress.rs:475`, `:479`).

The exception: **`unit_completed` takes a `Mutex` on every call** —
`reset_totals_for_phase` locks `current_phase` (`progress.rs:337`) before checking
whether the phase changed. That lock is taken by every `unit_completed` (19 sites
in `cleanup.rs:1474-1923`, 5 in `workspace.rs:1805-1877`) and every `units_total`.
Bounded by group and unit count, not entry count, so not a hot-loop problem — but
an uncontended lock per progress unit that a `load`/`store` on an `AtomicU8` phase
tag would avoid.

**Does a hidden renderer still pay observer cost? Yes, on two paths.** Counters are
never gated on `hidden` — each of the four batch methods is one relaxed
`fetch_add` (`progress.rs:474-493`). Negligible, but the module doc's "the no-op
observer is effectively free" (`progress.rs:4`) describes `NoopObserver`, **not**
`IndicatifRenderer::new(true)`. And `reportable_group` (`progress.rs:509-518`)
increments `eligible`, takes the `top` mutex, **clones the `PathBuf`**, pushes,
sorts and truncates — *then* calls `maybe_draw`, which returns immediately. Test
`progress.rs:553-554` confirms the hidden renderer still builds its top-5
(`assert_eq!(hidden.top_groups().len(), 1)`). `cargo_failure`, `empty_skipped`,
`active_skipped` and `group_measured` are pure `maybe_draw` calls, one `hidden`
check each.

**Can the bar's counts disagree with the report? Yes, by construction.** The
renderer's counters (`progress.rs:219-225`) and `ScanCounters` (`main.rs:1390`)
are independent values fed by independent increments. The bar's `visited` counts
**every entry** — `dirs_visited` receives `batch_visited += 1` per entry including
files (`discovery.rs:665`) — while `ScanCounters.directories_visited` counts
**directories only**: *"`directories_visited` counts directories only; `visited`
keeps counting every entry (files included) as the traversal total (L6)"*
(`discovery.rs:667-668`). So one run shows "visited N" on the bar and a different,
smaller visited-entries number in the report (`main.rs:1389`), and neither labels
the other. Sharper still: the message template hardcodes the word `analyzing`
whenever a total exists (`progress.rs:395`), so a `CleanupExecute` bar reads
`analyzing 3/12 visited …` regardless of phase. The duplication is the root cause;
sharing one counter set removes both symptoms.

**Is `ScanPhase` exhaustively handled, so a new phase cannot be silently ignored?
Yes.** The single `match` over `ScanPhase` in the entire crate is at
`progress.rs:461-469`, exhaustive over all seven variants with **no `_` catch-all
arm**. Adding a variant is a compile error there. This is the correct choice: a
`_` arm would let a new phase render with no label and no re-scoping — exactly the
silent-ignore failure the question is about.

**Can the in-memory test renderer corrupt a real terminal? No.**
`new_in_memory_for_test` (`progress.rs:285-297`) builds
`ProgressDrawTarget::term_like_with_hz(Box::new(term.clone()), 10)` over a
vt100-backed `InMemoryTerm`. It never constructs a `console::Term` for fd 2, so it
cannot emit escape sequences to a real display even if a test leaked the renderer.

**Can a bar be left on screen after an early return? On the known error paths, no
— and the property is only half pinned.** The C023 work changed the shape of this
question. `finish_and_clear()` is now called on the failure paths, not only on
success: the scan path clears the renderer before returning the discovery error
(`main.rs:1199`) and before returning the recency-window error (`main.rs:1353`),
and the cleanup path clears **before** the fallible cleanup call
(`main.rs:467` before `main.rs:468`) rather than after it. The reason is
recorded twice, at `main.rs:450-453` (cleanup) and `main.rs:1187-1190` (scan): a
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
(`progress.rs:816-872`). A panic is still uncovered — indicatif leaves partial
frames and the prompt resumes mid-frame with no newline — and a `Drop` impl on
`IndicatifRenderer` would close that deterministically.

## 9. Testing

**Real numbers:** 13 `#[test]` functions, confirmed by grep. The test module opens
at `progress.rs:534-535`. The table cites each test's `fn` line.

| # | Test | Line | Protects |
|---|---|---|---|
| 1 | `noop_observer_is_free_and_hidden_renderer_emits_nothing` | `:539` | Hidden short-circuit; `NoopObserver` total no-op |
| 2 | `coordinated_renderer_owns_six_bars_on_one_draw_target` | `:559` | Bar count + shared draw target |
| 3 | `in_memory_frame_contains_at_most_six_progress_lines` | `:568` | Rendered frame shape; no unbounded scroll |
| 4 | `renderer_caps_rows_at_five_largest_first` | `:606` | Top-5 truncation + descending sort |
| 5 | `test_observer_records_semantic_events` | `:618` | `TestObserver` bookkeeping |
| 6 | `should_show_progress_respects_flag_and_dumb_term` | `:636` | Gate conditions 1 and 2 |
| 7 | `renderer_refresh_bounded_independently_of_entry_count` | `:670` | Zero refreshes when hidden |
| 8 | `discovery_begins_indeterminate_then_analysis_determinate` | `:683` | Indeterminate→determinate switch; `group_measured` does not advance |
| 9 | `unit_events_rescope_totals_on_a_phase_change_without_an_explicit_phase_call` | `:709` | M8: event-carried phase re-scopes |
| 10 | `phase_total_resets_so_cleanup_cannot_reuse_analysis_counts` | `:728` | Phase change zeroes total and done |
| 11 | `sizes_render_in_top_rows_and_clear_before_report` | `:744` | Candidate list; `format_bytes`; idempotent clear |
| 12 | `renderer_error_falls_back_without_failing_scan` | `:757` | Hidden fallback is a no-op |
| 13 | `finish_and_clear_leaves_no_live_bar_for_a_later_error_or_drop` | `:816` | The clear leaves no live bar and no redraw for a later error |

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
within six lines, whether the terminal was cleared, or whether the bar exceeded
100%. Its own doc comment draws the line — *"Refresh counting is renderer-side;
this records semantic events only"* (`progress.rs:63`). Tests 8, 9, 10 protect the
*contract*; only tests 3 and 13 touch the *renderer*.

**Do the tests assert on rendered output or only observer state? Both, and
rendering is covered by two tests.** Test 3 (`progress.rs:567-603`) reads
`term.contents()` and asserts that the composed frame carries the counter fields
`discovery-errors 7` and `cargo-failures 2` (`progress.rs:582-583`), that the frame
has at most six non-empty lines (`progress.rs:584-589`), and that twenty further
refreshes do not scroll the buffer (`progress.rs:590-601`). Test 13
(`progress.rs:816-872`) asserts that the bar really drew and was live before the
clear (`progress.rs:823-831`), that the clear empties the frame
(`progress.rs:839-843`), and that an error raised after the clear neither
resurrects a live bar nor redraws (`progress.rs:844-863`). Every other test
inspects accessors or
`TestObserver` state. So: **rendering is observable via the `in_memory` feature,
but only as message substrings, a line count, and frame emptiness.**

The gap is narrower than it was, and it is concrete. Nothing asserts that the
phase prefix (`"analyzing output…"`, `progress.rs:464`) is present, that
`format_bytes` output appears in a candidate row, or that the bar's position
tracks `done/total`. A renderer that painted the right counters on the wrong
line, with no phase prefix and no formatted sizes, would pass every current
test. Asserting that `contents()` contains
`"analyzing"` and `format_bytes(2048)` would close it with the machinery already
in place.

**Two tests are misnamed relative to what they assert.**
`renderer_error_falls_back_without_failing_scan` (`progress.rs:757`) injects no
error — there is no error channel to inject one into. It drives a hidden renderer
and asserts `refresh_count() == 0`, so it cannot fail if error handling
regressed, because error handling has no runtime surface; its comment
(`progress.rs:758-759`) concedes as much. And
`renderer_refresh_bounded_independently_of_entry_count` (`progress.rs:670`) has a
comment (`progress.rs:671-674`) claiming it asserts "that visible construction does
not allocate per entry", but the body (`progress.rs:675-679`) only ever constructs
a **hidden** renderer: the visible 10 Hz throttle is never measured, and the test
proves `0 == 0`.

**Platform risk.** The only environment-sensitive test is
`should_show_progress_respects_flag_and_dumb_term` (`progress.rs:635-652`), which
mutates the process-global `TERM` via `unsafe { std::env::set_var }`
(`progress.rs:645`, `:650`) — correctly wrapped, which edition 2024 /
`rust-version = "1.89"` requires (`Cargo.toml:4-5`). It asserts only the flag and
`TERM=dumb` branches, so it is **not** TTY-dependent and is safe on CI and
Windows. But `TERM` is process-global and `cargo test` runs tests in parallel
threads, so this is a latent race against any other test — present or future —
that reads `TERM` or lets indicatif/`console` read it. No other test in this file
touches `TERM`, so it is contained today.

**Coverage by the fixture checker.** `scripts/check-fixture-portability.py`
contains **no reference to `progress.rs` or to `progress` at all** (grepped
directly). Not covered by `tests/cli_contract.rs` either, which always passes
`--no-progress` and asserts only that `--stats` lands on stderr
(`tests/cli_contract.rs:198`), nor by `tests/end_to_end.rs`, which imports
`progress` solely for `NoopObserver` (`:10`, `:132`).

## 10. Review checklist

1. **Stdout/stderr separation intact.** The only production draw target is
   `stderr_with_hz` (`progress.rs:267`); every report write is a `println!`
   (`main.rs:1407`, `:1243`, `:1248`, `:892`, `:899`). A new code path passing
   `MultiProgress::new()` or any other target breaks pipeability silently.
2. **The TTY check is stderr-only and must stay that way.**
   `stderr_is_terminal` (`progress.rs:189-192`) inspects fd 2 and never fd 1.
   Adding a stdout check would suppress progress for `cleanme scan | jq` — the
   case the separation exists to support.
3. **The format gate at `main.rs:1178-1179` and `main.rs:446-447` is duplicated,
   not shared.** Both must be edited together or `scan` and `clean` will disagree;
   the `!show` inversion at `main.rs:1180` / `main.rs:448` is easy to drop.
   Adding a fourth `OutputFormat` variant means touching both, and a variant
   added without them would silently inherit "progress allowed".
4. **No trait method may gain a `Result`, and no observer call may take a `?`.**
   The "progress can never fail a scan" guarantee is currently *type-level*
   (`progress.rs:40-52`); it becomes a convention the moment one signature
   changes. Preserve the `let _ =` at `progress.rs:427`.
5. **`ScanPhase` is exhaustively matched at `progress.rs:461-469` with no
   catch-all — keep it that way.** A `_` arm would let a new phase render unlabelled
   and un-rescoped with no compiler complaint.
6. **The >100% guard is `done.min(total)` at `progress.rs:405`, not the atomic.**
   `determinate_done` is intentionally unclamped; any refactor letting the atomic
   drive `set_position` directly reintroduces `130/100`.
7. **The bar's counters and the report's already disagree.** Bar `visited` counts
   all entries (`discovery.rs:665`); `ScanCounters.directories_visited` counts
   directories only (`discovery.rs:667-668`). The `analyzing` literal at
   `progress.rs:395` is wrong for every phase except `Analysis`. Fixing this means
   sharing one counter set, not patching the label.
8. **The error paths now clear the bar — and the ordering is still unpinned.**
   `main.rs:1199` and `main.rs:1353` clear before the scan error returns, and
   `main.rs:467` clears before the fallible cleanup call at `main.rs:468`. Reverting
   that ordering leaves every test green (C023 §11.4). A `Drop` impl on
   `IndicatifRenderer` would make the failure path deterministic without a pty
   harness.
9. **`set_determinate_total` (`progress.rs:326`) has zero callers and is
   inconsistent with `apply_total`** (it does not zero `determinate_done`). Delete
   it or route it through `apply_total`; do not leave a second way to set a total.
10. **The observability gap is the frame *content*.** Tests 3 and 13
    (`progress.rs:567-603`, `progress.rs:816-872`) read `term.contents()` and
    assert message substrings, a line count, and frame emptiness — but nothing
    asserts on `"analyzing"` or `format_bytes(2048)` in the rendered string.
    Asserting those is nearly free and would catch label, formatting, and
    row-population regressions that all thirteen current tests miss.
