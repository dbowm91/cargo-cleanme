# Progress — observing work without owning the terminal

> Component deep dive · part of the [architecture overview](overview.md)

`src/progress.rs` — 703 lines, 498 production (`progress.rs:499` opens the test
module), 12 `#[test]` functions.

## 1. Responsibility

Owns **transient terminal feedback on stderr** and nothing else: the decision of
*whether* to draw, the phase vocabulary, the trait producers emit through, and the
six-bar `indicatif` composition.

Does **not** own:

- **The final report** — that is stdout, via `report::render` (`main.rs:667`) or
  `output::scan` (`main.rs:652`) or `output::log::scan` (`main.rs:664`).
  `Reporting` (`progress.rs:21`) is announced (`main.rs:645`) then immediately
  followed by `finish_and_clear()` (`main.rs:646`); no bar is ever drawn for it.
- **Any domain truth.** Every number the renderer shows is a side effect of an
  observer call; the report's numbers come from `ScanCounters`, carried in from
  discovery at `main.rs:479` and written into the report at `main.rs:642`.
  Independent counter sets — see §8.
- **Phase sequencing.** `cleanup.rs:829-833` maps `CleanMode` → `ScanPhase`; the
  renderer reacts to whatever phase a caller names.

**The stdout/stderr separation is a contract, verified on three axes:**

| Axis | Evidence |
|---|---|
| Draw target is fd 2 | `ProgressDrawTarget::stderr_with_hz(10)` (`progress.rs:239`) |
| Report is fd 1 | `println!` for human (`main.rs:667`), JSON (`main.rs:650`), log (`main.rs:662`), and both cleanup renderers (`main.rs:365`, `:372`) |
| Clear precedes report | `finish_and_clear()` at `main.rs:646` (scan), `main.rs:234` (cleanup) |

The user-facing consequence: `cargo cleanme scan | jq` shows progress on the
terminal and keeps stdout pipeable. §5 shows the TTY check is on **stderr only**,
which is exactly the axis that preserves this.

**Dependency footprint is one item.** The only `crate::` reference in the
production half is `crate::report::format_bytes` (`progress.rs:171`), used for the
five candidate-row sizes at `progress.rs:383` — sharing the formatter with the
report so a size never renders in two notations within one run. Everything else is
`std` (`progress.rs:7-14`) or `indicatif`. `progress.rs:501` is test-only.

> **Correction to the layer map.** `overview.md:48-56` draws an edge from
> `progress` down to `traverse (measurement)`. There is **no** `crate::traverse`
> reference anywhere in `progress.rs`, production or test. The only real crate edge
> is `progress → report` (`progress.rs:171`), and the diagram omits that one.

## 2. Public surface

`ScanPhase` — `progress.rs:16-25`. `Clone + Copy + Debug + Eq + PartialEq`.

| Variant | Work it represents | Entered from | Total known in advance? |
|---|---|---|---|
| `Discovery` | Filesystem walk for `Cargo.toml` | `main.rs:469`, `cleanup.rs:835` | No — indeterminate |
| `Resolution` | `cargo metadata` per manifest | `main.rs:500` | No — no `units_total` ever |
| `Analysis` | Parallel sizing + activity classification | `main.rs:601-602`, total at `workspace.rs:932` | Yes — determinate |
| `Reporting` | About to write the stdout report | `main.rs:645` | No — never drawn |
| `CleanupPreview` | Cargo's own `clean --dry-run` preview | `cleanup.rs:830` | Yes — `cleanup.rs:927` |
| `CleanupSimulate` | Application simulation, no Cargo spawned | `cleanup.rs:831` | Yes — `cleanup.rs:927` |
| `CleanupExecute` | Real cleanup through Cargo | `cleanup.rs:832` | Yes — `cleanup.rs:927` |

`ProgressObserver` — `progress.rs:39-52`. `Send + Sync`. All twelve methods take
`&self` and **all twelve have default empty bodies**, so an implementor overrides
only what it needs; `impl ProgressObserver for NoopObserver {}` (`progress.rs:58`)
is a complete, allocation-free implementation.

| Method | Argument | Meaning | Phases that use it |
|---|---|---|---|
| `phase(ScanPhase)` | new phase | Announce a transition; re-scopes the determinate scope | All four scan + all three cleanup phases |
| `dirs_visited(u64)` | batch delta | Entries walked, batched at 512 (`discovery.rs:629-631`) | `Discovery` |
| `dirs_pruned(u64)` | batch delta | Entries cut by prune rules, batched at 64 (`discovery.rs:685-687`) | `Discovery` |
| `manifests_found(u64)` | batch delta | Validated `Cargo.toml` files (`discovery.rs:682`) | `Discovery` |
| `workspaces_resolved(u64)` | batch delta | Successful `cargo metadata` runs (`workspace.rs:425`) | `Resolution` |
| `cargo_failure()` | — | One `cargo metadata` run failed (`workspace.rs:360,375,388,526,541,556`) | `Resolution` |
| `empty_skipped()` | — | Group skipped: no eligible bytes (`workspace.rs:958,1154`) | `Analysis` |
| `active_skipped()` | — | Group skipped: protected by source activity (`workspace.rs:1008,1167`) | `Analysis` |
| `group_measured(u64)` | bytes | A group finished sizing (`workspace.rs:1179`) | `Analysis` |
| `reportable_group(&Path, u64)` | path, bytes | A group qualifies for the report (`workspace.rs:1208`, `cleanup.rs:1333`) | `Analysis`, cleanup |
| `units_total(ScanPhase, u64)` | phase, total | Announce the unit count for a phase | `Analysis` (`workspace.rs:932`), cleanup (`cleanup.rs:927`) |
| `unit_completed(ScanPhase)` | phase | One unit finished | `Analysis` (`workspace.rs:1108-1180`), cleanup (16 sites, `cleanup.rs:958-1236`) |

Everything else:

| Item | Signature | Contract |
|---|---|---|
| `NoopObserver` | `pub struct NoopObserver` (`:56`) | Zero-cost observer; ~90% of the crate's tests |
| `TestObserver::new` | `-> Self` (`:81`) | `Default` constructor |
| `TestObserver::total_for` | `(phase: ScanPhase) -> Option<u64>` (`:85`) | **Last** total announced for a phase, or `None` |
| `TestObserver::completed_count` | `(phase: ScanPhase) -> usize` (`:95`) | Count of `unit_completed` events for a phase |
| `should_show_progress` | `(no_progress: bool) -> bool` (`:149`) | Gate; see §5 |
| `IndicatifRenderer::new` | `(hidden: bool) -> Self` (`:231`) | **Inverted** flag: `true` means *hidden* |
| `…::new_in_memory_for_test` | `-> (Self, InMemoryTerm)` (`:257`) | Same composition, vt100-backed screen |
| `…::is_hidden` | `-> bool` (`:271`) | Construction flag, not terminal capability |
| `…::coordinated_bar_count` | `-> usize` (`:279`) | 6 visible / 0 hidden |
| `…::has_shared_draw_target` | `-> bool` (`:286`) | `multi.is_some()` |
| `…::set_determinate_total` | `(&self, total: u64)` (`:298`) | **No callers anywhere** — §6 |
| `…::finish_and_clear` | `(&self)` (`:392`) | Wipes the transient region; returns `()` |
| `…::refresh_count` | `-> u64` (`:407`) | Draws that passed the 100 ms gate |
| `…::top_groups` | `-> Vec<(PathBuf, u64)>` (`:411`) | Clone of the ≤5-entry candidate list |
| `…::determinate_total_value` | `-> u64` (`:415`) | Current scope's total |
| `…::determinate_done_value` | `-> u64` (`:419`) | Current scope's completed count |

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
(`progress.rs:471-473`) and never touches `determinate_done`; `active_skipped` is
likewise draw-only (`progress.rs:466-470`). Asserted at `progress.rs:638-639`:
after `group_measured(100)`, `determinate_done_value()` is still `0`; only
`unit_completed` moves it (`progress.rs:640-641`).

**`Send + Sync` (`progress.rs:39`) is load-bearing.** The observer is handed into a
`dua_core::walk_roots` closure (`discovery.rs:517`, `move |root_idx, entry|` at
`discovery.rs:526`) running on dua-core's work-stealing pool; the closure calls
`dirs_visited`, `dirs_pruned`, `manifests_found` (`discovery.rs:630`, `:686`,
`:682`). A shared `&dyn ProgressObserver` can only cross that boundary if it is
`Sync`. With `&self` on every method, this forces interior mutability: `AtomicU64`
for counters, `Mutex` for the candidate list, the phase, and the draw timestamp
(`progress.rs:192-202`).

**Why a trait and not a direct `indicatif` call.** Two independent reasons.
**Testability:** `discovery`, `workspace` and `cleanup` are tested with
`NoopObserver` at ~90 sites (`cleanup.rs` alone passes it at 60+) and with
`TestObserver` for event assertions (`cleanup.rs:4604`, `:4612`, `:4620`); without
a trait every one of those tests would need a real terminal or a global renderer
hook. **Multiple independent producers, one renderer:** all three modules report
into a single `IndicatifRenderer` created in `main.rs` (`:222`, `:451`) and never
learn about one another — none imports `indicatif`, and none knows whether the
renderer is a spinner, a TTY bar, a test double, or absent.

**"The observer must never be able to fail a scan" — honoured.** Enforced at four
levels, and `main.rs:466-467` is accurate. **No fallible signature exists:** every
method returns `()` (`progress.rs:40-51`), so there is nothing to propagate and no
`?` is ever written against an observer. **Construction is infallible:** `new`
returns `Self`, not `Result` (`progress.rs:231`) — a terminal that cannot be drawn
to is the `hidden` branch, not an error (`progress.rs:232-234`). **The one
fallible indicatif call is discarded:** `finish_and_clear` does
`let _ = multi.clear();` (`progress.rs:397`). **Throttling bounds the cost**, so
progress cannot dominate a fast scan.

The comment does *not* claim a runtime recovery path, because there is none to
need: degradation happens once, at construction, via the caller's boolean.

## 4. Phases

**Indeterminate: `Discovery`, `Resolution`, `Reporting`.** No `units_total` is ever
emitted for these. `Discovery` cannot have one without a pre-scan
(`progress.rs:625-629` asserts the total must be `0`, with the reason in the assert
message). `Resolution` has no cheap cardinality — one subprocess per manifest,
failures unknown until run. `Reporting` is never drawn.

**Determinate: `Analysis` and the three cleanup phases.** `Analysis` gets its
total from `workspace::analyze_groups` at `workspace.rs:932` (and redundantly from
`main.rs:602`). Cleanup gets its total at `cleanup.rs:927`, gated on complete
ownership coverage — the comment at `cleanup.rs:925-926` is explicit that a partial
total would be a lie.

**Re-scoping semantics.** `main.rs:599` says *"Phase first (re-scopes totals), then
total (starts fresh scope)"*. Verified against the renderer; the two steps are
distinct operations:

- `phase(p)` → `reset_totals_for_phase(p)` (`progress.rs:425-426`, `:308-321`): if `p`
  differs from `current_phase`, zero both determinate counters and set the bar to
  `set_length(0); set_position(0)`.
- `units_total(p, n)` → `reset_totals_for_phase(p)` **again** (`progress.rs:488`),
  then `apply_total(n)` (`progress.rs:323-336`): store `n`, zero
  `determinate_done`, `set_length(n); set_position(0)`.

The second reset is idempotent (same phase, so the `if` at `progress.rs:310` does
not fire) and exists as defence in depth: `units_total` re-scopes on the phase
carried *by the event itself*, not only on an explicit `phase()` call. That is the
M8 fix named at `progress.rs:485-487`, tested at `progress.rs:645-661`.

**User-visible meaning:** a phase change visibly snaps the bar to 0% rather than
carrying a partial fill into unrelated work. Asserted at `progress.rs:669-672`
(`Analysis` at 1/5, then `CleanupSimulate` → total `0`, done `0`).

## 5. Deciding whether to show progress

**Two gates, and they are different kinds of thing.** A call site must satisfy
both before a renderer exists at all.

1. **Format gate** — `main.rs:483-484` (scan) and `main.rs:220-221` (cleanup),
   both `options.format == OutputFormat::Human && progress::should_show_progress(no_progress)`.
2. **Construction gate** — `IndicatifRenderer::new(!show)` at `main.rs:465` and
   `:222`. The renderer's `hidden` flag is the *negation* of the gate, so `show`
   being false produces a renderer whose every draw path short-circuits.

A third condition sits behind the format gate, inside
`should_show_progress(no_progress: bool) -> bool` — `progress.rs:149-163`. Three
conditions, evaluated in order, each an early `return false`:

| # | Condition | Line | Effect |
|---|---|---|---|
| 1 | `no_progress == true` | `progress.rs:150-152` | `--no-progress` wins outright |
| 2 | `TERM == "dumb"` | `progress.rs:153-155` | Dumb terminal |
| 3 | `!stderr_is_terminal()` | `progress.rs:159-161` | stderr is not a TTY |

**TTY detection is on stderr, and only stderr.** `stderr_is_terminal`
(`progress.rs:165-168`) is `std::io::stderr().is_terminal()` via
`std::io::IsTerminal`. **stdout is never inspected.**

The table below therefore describes `--format human` only. Under the default
format, the three rows hold:

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

**Both non-Human formats suppress progress on a TTY** (`main.rs:483-484`,
`:220-221`). JSON is a deliberate choice: a machine-readable consumer should get
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
also drops the scan's per-diagnostic stderr fan-out (`main.rs:678`, gated on
`options.format != OutputFormat::Log`) so its stderr carries
only `--stats`; the diagnostics instead ride along as the log line's
`diagnostics=N` count (`output.rs:364-365`). Progress suppression and diagnostic
suppression are separate decisions that happen to agree, and each was made for
its own reason.

**Testability of the gate.** `should_show_progress_respects_flag_and_dumb_term`
(`progress.rs:593-603`) covers conditions 1 and 2. Condition 3 — the TTY branch —
is **never exercised by any test in the crate**: every integration case that runs
a scan or cleanup passes `--no-progress` (26 occurrences, `tests/cli_contract.rs:183`
through `:1701`) and `tests/end_to_end.rs:132` uses `NoopObserver` directly. The
format gate itself has no unit test at all — it is a bare `&&` at two sites in
`main.rs`, and `main.rs` has no tests. The most consequential branch is the only
untested one.

## 6. The indicatif renderer

**Internal state** (`progress.rs:187-203`):

| Field | Purpose |
|---|---|
| `hidden: bool` | Construction flag; short-circuits every draw path |
| `multi: Option<MultiProgress>` | The single shared draw owner |
| `main: Option<ProgressBar>` | Phase label + determinate bar + counter line |
| `rows: Vec<ProgressBar>` | Exactly 5; **positional, not keyed** — `rows[i]` is display slot *i*, nothing more |
| `top: Mutex<Vec<(PathBuf, u64)>>` | ≤5 largest reportable groups, sorted |
| `last_draw: Mutex<Option<Instant>>` | The throttle's previous-draw timestamp |
| `refreshes: AtomicU64` | Draws that passed the gate; `pub` (`:194`) |
| `visited/pruned/manifests/workspaces/eligible` | Display-only counters |
| `determinate_total/determinate_done` | The current scope's progress pair |
| `current_phase: Mutex<Option<ScanPhase>>` | Re-scoping detector |

**Throttling is three independent mechanisms, not one:** (1) `maybe_draw`'s own
100 ms wall-clock gate (`progress.rs:346-355`), the one `refreshes` counts; (2)
indicatif's `RateLimiter` built by `stderr_with_hz(10)`, 100 ms interval, burst
capacity 20 (indicatif `draw_target.rs:443-470`, `MAX_BURST = 20` at `:510`); (3)
`enable_steady_tick(100 ms)` (`progress.rs:242`), indicatif's ticker thread, which
advances the spinner independently of any event. Effective ceiling: 10 Hz with
bursts to 20. The doc comment's "Refresh capped at 10 Hz; never redrawn per
filesystem entry" (`progress.rs:184`) is accurate but understates that there are
two gates.

**`set_determinate_total` (`progress.rs:298-306`) is dead code.** It stores
`determinate_total` and calls `main.set_length(total)` — but does **not** zero
`determinate_done`, unlike `apply_total` (`progress.rs:323-336`). Grep across
`src/` and `tests/` returns exactly one hit: its own definition. Its doc comment
claims "this concrete helper remains for direct callers" (`progress.rs:295-297`);
there are none. It is also the one entry point that could produce an inconsistent
state (total set, done left over from a previous scope).

**Over-100% is guarded.** `maybe_draw` clamps at `progress.rs:375`:
`main.set_position(done.min(total))`. A caller emitting more `unit_completed` than
announced units stops the bar at 100% rather than rendering `130/100`. The atomic
`determinate_done` is *not* clamped, so `determinate_done_value()` can exceed
`determinate_total_value()` — visible only through the test accessor.

**`refresh_count`, `determinate_total_value`, `determinate_done_value`,
`top_groups`, `is_hidden`, `coordinated_bar_count`, `has_shared_draw_target` all
have no production caller** — they are consumed only by the module's own tests
(`progress.rs:511-519`, `:586-589`, `:641`, `:653-658`). They are `pub` rather than
`#[cfg(test)]` so `cleanup.rs` and the integration tests *could* assert on them.

**`finish_and_clear` (`progress.rs:392-405`)** calls `multi.clear()` then
`finish_and_clear()` on the main bar and each row. Clearing matters because the
report is written with `println!` (`main.rs:667`, `:636`), which emits `\n` and
nothing else — any residual frame would remain interleaved above the report in
terminal history. Idempotent (`progress.rs:688-689` calls it twice) and safe when
hidden (`progress.rs:393-395`).

**`top_groups` is a count, not a sort key.** "Top" means: sorted descending by
bytes with a path-ascending tiebreak, **truncated to 5**
(`progress.rs:479-480`). The sort is stable and total, so the same input yields the
same five rows in the same order. `maybe_draw` writes `rows[i]` from `top[i]` and
blanks surplus slots (`progress.rs:381-388`).

**Hidden short-circuit.** `maybe_draw` returns at `progress.rs:343-345` *before*
touching the mutex or calling `Instant::now()`; `set_determinate_total` returns at
`:300-302` after its atomic store; `finish_and_clear` at `:393-395`;
`coordinated_bar_count` / `has_shared_draw_target` return `0`/`false` at
`:280-282`/`:287-289`. Test at `progress.rs:515-517` fires 1000 events and asserts
`refresh_count() == 0`.

**The `in_memory` feature.** `Cargo.toml:56` declares
`indicatif = { version = "0.18.6", features = ["in_memory"] }`. In indicatif 0.18.6
that is exactly `in_memory = ["vt100"]` (indicatif `Cargo.toml:55`): it pulls in the
`vt100` terminal emulator so indicatif can expose `InMemoryTerm` — a draw target
backed by an in-process `Screen` whose frames are read back as a `String` via
`contents()` (indicatif `in_memory.rs:19`, `:32`).

Why a **production** dependency, not a dev-dependency: the affordance is compiled
into the shipped library API. `new_in_memory_for_test` (`progress.rs:257`) is a
`pub fn` on a `pub struct` in `src/` returning `(Self, indicatif::InMemoryTerm)` —
it names the type in its signature, and a dev-dependency is not linked into the
library, so a `pub fn` there could not mention `InMemoryTerm` at all.
`#[cfg(test)]`-gating would mean the composition is only ever compiled under
`cargo test`, so production and test paths could silently diverge. As written,
`progress.rs:262-267` builds the *same* one-`MultiProgress` + main + 5-rows
composition as `new` (`progress.rs:238-249`), differing only in the draw target:
the test exercises production wiring, not a parallel one.

## 7. Coordinated multi-bar rendering

**The coordination mechanism is `MultiProgress::add` interception.** `new()`
creates one `MultiProgress` over a single `stderr_with_hz(10)` target
(`progress.rs:238-240`), then adds the main bar and five rows via `multi.add(...)`
(`progress.rs:241`, `:246`). Per indicatif's contract, a bar inserted with `add`
has *"the draw target changed to a remote draw target that is intercepted by the
multi progress object overriding custom `ProgressDrawTarget` settings"* (indicatif
`multi.rs:211-213`). Consequence: **no bar owns the terminal** — they report state
to the `MultiProgress`, which is the only thing that moves the cursor. This is
what prevents the classic multi-bar failure where each bar independently rewrites
the same region. The comment at `progress.rs:235-237` states this correctly.

**`coordinated_bar_count` (`progress.rs:279-284`) and `has_shared_draw_target`
(`progress.rs:286-291`)** assert that structure: 6 bars visible / 0 hidden, and
`multi.is_some()`. **Both are consumed only by the tests at `progress.rs:513-514`
and `:527-528`.** No production code reads them, so they cannot influence
behaviour — the tests that use them are the only evidence the six-bar composition
is intact.

**A phase transition does not retire bars.** All six are created once in `new()`
(`progress.rs:241-248`) and live for the whole process. A phase change rewrites the
main bar's prefix (`progress.rs:440`) and its length/position
(`progress.rs:317-318`); the five rows keep their text until the next draw
re-labels them. The visual effect reads as "one bar became another", but the bar
objects are the same six throughout, so artefacts are a non-issue *within* a run;
the single lifecycle event is `finish_and_clear`.

**Terminal resize is not handled by this module.** No `SIGWINCH` handling, no
cached width, no reflow logic — nothing in `progress.rs` reads a terminal size.
Re-measurement is indicatif's job per draw. The one fixed size is the test
constant `InMemoryTerm::new(24, 120)` (`progress.rs:258`), which gives the vt100
screen a defined frame and has no production counterpart.

**Determinate and indeterminate never actually coexist.** The main bar is created
as `ProgressBar::new_spinner()` (`progress.rs:241`) and becomes determinate purely
by `set_length` + `set_position` (`progress.rs:317-318`, `:333-334`), with
`maybe_draw` choosing per draw (`progress.rs:374-378`): `set_position(done.min(total))`
when `determinate_total > 0`, else `tick()`. The five rows are *always* spinners
(`progress.rs:246`) and only ever receive `set_message` — never a position or a
length (`progress.rs:384-386`). So there is one bar flipping mode and five
that are pure text, not two objects on one screen.

**Capability degradation is two-layer, and only the first is visible here.** Layer
1 is the crate's own: `TERM=dumb` (`progress.rs:153`) and stderr-is-a-TTY
(`progress.rs:159`) — no `CI` check, no `NO_COLOR` check, no "is `TERM` unset"
check; grepping `src/` for `NO_COLOR` / `CI` finds nothing progress-related. Layer
2 is indicatif's own: `stderr_with_hz` returns `Self::hidden()` when
`!term.is_term() || is_dumb()` (indicatif `draw_target.rs:80-81`, `is_dumb` from
`console` at `draw_target.rs:12`).

> **Divergence worth naming.** `IndicatifRenderer::is_hidden()` (`progress.rs:271-273`)
> returns the *construction flag*, not the effective terminal capability;
> `MultiProgress::is_hidden()` (indicatif `multi.rs:321-323`) is the capability.
> They differ whenever the crate's gate and indicatif's gate disagree, and
> **nothing in this crate consults the latter.** Relatedly, `overview.md:108`
> calls this module "terminal capability detection"; it performs *gating*
> detection (flag + TTY + dumb) and never queries what the terminal can do.

## 8. Invariants and edge cases

**Can a progress-render failure ever propagate into a scan result or change an
exit code? No — definitively.** Traced end to end:

1. No trait method returns a `Result` (`progress.rs:40-51`); nothing to propagate.
2. No caller in `src/` writes `?` against an observer call. All are bare
   statements: `discovery.rs:630`, `workspace.rs:932`, `cleanup.rs:927`,
   `main.rs:602`.
3. `new` is infallible (`progress.rs:231`); the only fallible indicatif call,
   `multi.clear()`, has its `io::Result` dropped at `progress.rs:397`.
4. The exit code comes from scan truth only: `Ok(1)` iff `full_incomplete`
   (`main.rs:724-726`), else `Ok(0)` (`main.rs:727`). No progress value
   participates, and a failed state publish does not participate either.

`main.rs:466-467` is accurate, and the mechanism is exactly infallible
construction plus refresh throttling.

**Is progress output guaranteed to go to stderr and never stdout? Yes.** The only
production draw target is `stderr_with_hz` (`progress.rs:239`); the in-memory
constructor never touches fd 2. Every stdout write in the pipeline is a report
`println!` — scan at `main.rs:661`, `:667`, `:673` and cleanup at `main.rs:371`,
`:373`, `:378`, plus the empty-scope note at `:388`. The `--stats` case holds
too: stats go to stderr (`main.rs:245`, `:707`), as `main.rs:243-244` and
`main.rs:704-723` require.
Cleanup's state-generation line (`main.rs:238`) is likewise stdout, so it is
behind the same Human-only gate as the human report.

**Are the observer calls cheap enough for a hot loop? Mostly yes, one exception.**
The trait doc requires *"Implementations MUST NOT allocate per visited entry;
directory counters are aggregated in batches"* (`progress.rs:30-31`), and producers
honour it: `discovery.rs:629-631` batches `dirs_visited` at 512 entries,
`discovery.rs:685-687` batches `dirs_pruned` at 64. Per-call cost is one relaxed
`fetch_add` (`progress.rs:445`, `:453`).

The exception: **`unit_completed` takes a `Mutex` on every call** —
`reset_totals_for_phase` locks `current_phase` (`progress.rs:309`) before checking
whether the phase changed. That lock is taken by every `unit_completed` (16 sites
in `cleanup.rs:958-1236`, 5 in `workspace.rs:1108-1180`) and every `units_total`.
Bounded by group and unit count, not entry count, so not a hot-loop problem — but
an uncontended lock per progress unit that a `load`/`store` on an `AtomicU8` phase
tag would avoid.**Does a hidden renderer still pay observer cost? Yes, on two paths.** Counters are
never gated on `hidden` — each of the four batch methods is one relaxed
`fetch_add` (`progress.rs:444-459`). Negligible, but the module doc's "the no-op
observer is effectively free" (`progress.rs:4`) describes `NoopObserver`, **not**
`IndicatifRenderer::new(true)`. And `reportable_group` (`progress.rs:474-483`)
increments `eligible`, takes the `top` mutex, **clones the `PathBuf`**, pushes,
sorts and truncates — *then* calls `maybe_draw`, which returns immediately. Test
`progress.rs:518-519` confirms the hidden renderer still builds its top-5
(`assert_eq!(hidden.top_groups().len(), 1)`). `cargo_failure`, `empty_skipped`,
`active_skipped` and `group_measured` are pure `maybe_draw` calls, one `hidden`
check each.

**Can the bar's counts disagree with the report? Yes, by construction.** The
renderer's counters (`progress.rs:195-199`) and `ScanCounters` (`main.rs:642`)
are independent values fed by independent increments. The bar's `visited` counts
**every entry** — `dirs_visited` receives `batch_visited += 1` per entry including
files (`discovery.rs:623`) — while `ScanCounters.directories_visited` counts
**directories only**: *"`directories_visited` counts directories only; `visited`
keeps counting every entry (files included) as the traversal total (L6)"*
(`discovery.rs:625-626`). So one run shows "visited N" on the bar and a different,
smaller visited-entries number in the report (`main.rs:641`), and neither labels
the other. Sharper still: the message template hardcodes the word `analyzing`
whenever a total exists (`progress.rs:365`), so a `CleanupExecute` bar reads
`analyzing 3/12 visited …` regardless of phase. The duplication is the root cause;
sharing one counter set removes both symptoms.

**Is `ScanPhase` exhaustively handled, so a new phase cannot be silently ignored?
Yes.** The single `match` over `ScanPhase` in the entire crate is at
`progress.rs:431-439`, exhaustive over all seven variants with **no `_` catch-all
arm**. Adding a variant is a compile error there. This is the correct choice: a
`_` arm would let a new phase render with no label and no re-scoping — exactly the
silent-ignore failure the question is about.

**Can the in-memory test renderer corrupt a real terminal? No.**
`new_in_memory_for_test` (`progress.rs:257-269`) builds
`ProgressDrawTarget::term_like_with_hz(Box::new(term.clone()), 10)` over a
vt100-backed `InMemoryTerm`. It never constructs a `console::Term` for fd 2, so it
cannot emit escape sequences to a real display even if a test leaked the renderer.

**Can a bar be left on screen after an early return or a panic? Yes, and it is
untested.** `finish_and_clear()` is called only on the success path:
`main.rs:646` follows four earlier `?`s (`main.rs:442`, `:444`, `:463`, `:592`),
and `main.rs:234` follows the `?` at `main.rs:157`, `:159`, `:233`. On those paths the renderer
is dropped and indicatif's own `impl Drop for BarState` (indicatif
`state.rs:219-232`) calls `finish_using_style` on an unfinished bar, then
`mark_zombie` — so the line is finalised by indicatif rather than erased by this
crate, and the exact residue is indicatif's behaviour, not this crate's. A panic is
worse: indicatif leaves partial frames and the prompt resumes mid-frame with no
newline. **No test covers any of this** — all 12 unit tests are renderer-local and
every integration test passes `--no-progress`. A `Drop` impl on
`IndicatifRenderer` would close this deterministically.

## 9. Testing

**Real numbers:** 12 `#[test]` functions, confirmed by grep. The test module opens
at `progress.rs:499-500`.

| # | Test | Line | Protects |
|---|---|---|---|
| 1 | `noop_observer_is_free_and_hidden_renderer_emits_nothing` | `:504` | Hidden short-circuit; `NoopObserver` total no-op |
| 2 | `coordinated_renderer_owns_six_bars_on_one_draw_target` | `:524` | Bar count + shared draw target |
| 3 | `in_memory_frame_contains_at_most_six_progress_lines` | `:533` | Rendered frame shape; no unbounded scroll |
| 4 | `renderer_caps_rows_at_five_largest_first` | `:566` | Top-5 truncation + descending sort |
| 5 | `test_observer_records_semantic_events` | `:578` | `TestObserver` bookkeeping |
| 6 | `should_show_progress_respects_flag_and_dumb_term` | `:593` | Gate conditions 1 and 2 |
| 7 | `renderer_refresh_bounded_independently_of_entry_count` | `:606` | Zero refreshes when hidden |
| 8 | `discovery_begins_indeterminate_then_analysis_determinate` | `:619` | Indeterminate→determinate switch; `group_measured` does not advance |
| 9 | `unit_events_rescope_totals_on_a_phase_change_without_an_explicit_phase_call` | `:645` | M8: event-carried phase re-scopes |
| 10 | `phase_total_resets_so_cleanup_cannot_reuse_analysis_counts` | `:664` | Phase change zeroes total and done |
| 11 | `sizes_render_in_top_rows_and_clear_before_report` | `:680` | Candidate list; `format_bytes`; idempotent clear |
| 12 | `renderer_error_falls_back_without_failing_scan` | `:693` | Hidden fallback is a no-op |

`TestObserver` (`progress.rs:64-78`) records semantic events: phase sequence and
totals in `Mutex<Vec<_>>`, counters in `AtomicU64`, the full reportable list in
`reportable: Mutex<Vec<(PathBuf, u64)>>`. `total_for` (`:85-93`) searches **in
reverse** for the last total announced for a phase, which is what makes it useful
for the re-announcement case; `completed_count` (`:95-102`) filters the full event
list.

**What `TestObserver` cannot observe** is the important half. It sees the *event
stream the producers emitted*, not what the renderer did with those events. It
cannot detect whether a bar was created, whether a draw occurred, whether a phase
label was applied, whether a size string was formatted, whether the frame stayed
within six lines, whether the terminal was cleared, or whether the bar exceeded
100%. Its own doc comment draws the line — *"Refresh counting is renderer-side;
this records semantic events only"* (`progress.rs:62`). Tests 8, 9, 10 protect the
*contract*; only test 3 touches the *renderer*.

**Do the tests assert on rendered output or only observer state? Both, but
rendering is covered by exactly one test, and only structurally.** Test 3
(`progress.rs:533-564`) is the sole assertion on rendered bytes: it reads
`term.contents()` and checks the composed frame has at most six non-empty lines
(`:544-549`) and that twenty further refreshes do not scroll the buffer
(`:550-561`). Every other test inspects accessors or `TestObserver` state. So:
**rendering is observable via the `in_memory` feature, but only as a line count.**

The gap is concrete. Test 3 never asserts the message text is correct, that
`format_bytes` output appears in a row, that the phase prefix (`"analyzing
output…"`) is present, that bar position tracks `done/total`, or that
`finish_and_clear` empties the screen. A renderer painting six blank lines would
pass all twelve tests. **A test asserting only observer state cannot catch a
rendering artefact**, and the one test that does read rendered output does not check
its content. Asserting that `contents()` contains `format_bytes(2048)` and
`"analyzing"` would close it with the machinery already in place.

**Two tests are misnamed relative to what they assert.**
`renderer_error_falls_back_without_failing_scan` (`:693`) injects no error — there
is no error channel to inject one into. It drives a hidden renderer and asserts
`refresh_count() == 0`, so it cannot fail if error handling regressed, because
error handling has no runtime surface; its comment (`:694-695`) concedes as much.
And `renderer_refresh_bounded_independently_of_entry_count` (`:606`) has a comment
(`:607-610`) claiming it asserts "that visible construction does not allocate per
entry", but the body (`:611-615`) only ever constructs a **hidden** renderer: the
visible 10 Hz throttle is never measured, and the test proves `0 == 0`.

**Platform risk.** The only environment-sensitive test is
`should_show_progress_respects_flag_and_dumb_term` (`progress.rs:593-603`), which
mutates the process-global `TERM` via `unsafe { std::env::set_var }` (`:596`,
`:599`) — correctly wrapped, which edition 2024 / `rust-version = "1.89"`
requires (`Cargo.toml:4-5`). It asserts only the flag and `TERM=dumb` branches, so
it is **not** TTY-dependent and is safe on CI and Windows. But `TERM` is
process-global and `cargo test` runs tests in parallel threads, so this is a latent
race against any other test — present or future — that reads `TERM` or lets
indicatif/`console` read it. No other test in this file touches `TERM`, so it is
contained today.

**Coverage by the fixture checker.** `scripts/check-fixture-portability.py`
contains **no reference to `progress.rs` or to `progress` at all** (grepped
directly). Not covered by `tests/cli_contract.rs` either, which always passes
`--no-progress` and asserts only that `--stats` lands on stderr
(`tests/cli_contract.rs:203`), nor by `tests/end_to_end.rs`, which imports
`progress` solely for `NoopObserver` (`:10`, `:132`).

## 10. Review checklist

1. **Stdout/stderr separation intact.** The only production draw target is
   `stderr_with_hz` (`progress.rs:239`); every report write is a `println!`
   (`main.rs:650`, `:645`, `:650`). A new code path passing `MultiProgress::new()` or
   any other target breaks pipeability silently.
2. **The TTY check is stderr-only and must stay that way.**
   `stderr_is_terminal` (`progress.rs:165-168`) inspects fd 2 and never fd 1.
   Adding a stdout check would suppress progress for `cleanme scan | jq` — the
   case the separation exists to support.
3. **The format gate at `main.rs:483-484` and `main.rs:220-221` is duplicated, not
   shared.** Both must be edited together or `scan` and `clean` will disagree; the
   `!show` inversion at `main.rs:465` / `:222` is easy to drop. Adding a fourth
   `OutputFormat` variant means touching both, and a variant added without them
   would silently inherit "progress allowed".
4. **No trait method may gain a `Result`, and no observer call may take a `?`.**
   The "progress can never fail a scan" guarantee is currently *type-level*
   (`progress.rs:40-51`); it becomes a convention the moment one signature
   changes. Preserve the `let _ =` at `progress.rs:397`.
5. **`ScanPhase` is exhaustively matched at `progress.rs:431-439` with no
   catch-all — keep it that way.** A `_` arm would let a new phase render unlabelled
   and un-rescoped with no compiler complaint.
6. **The >100% guard is `done.min(total)` at `progress.rs:375`, not the atomic.**
   `determinate_done` is intentionally unclamped; any refactor letting the atomic
   drive `set_position` directly reintroduces `130/100`.
7. **The bar's counters and the report's already disagree.** Bar `visited` counts
   all entries (`discovery.rs:623`); `ScanCounters.directories_visited` counts
   directories only (`discovery.rs:625-626`). The `analyzing` literal at
   `progress.rs:365` is wrong for every phase except `Analysis`. Fixing this means
   sharing one counter set, not patching the label.
8. **The error path never calls `finish_and_clear`.** `main.rs:234` follows the
   `?`s at `main.rs:157`, `:159`, `:233`; `main.rs:646` follows four earlier `?`s.
   Residue is left to indicatif's `Drop for BarState` (indicatif
   `state.rs:219-232`), untested. A `Drop` impl would make this deterministic.
9. **`set_determinate_total` (`progress.rs:298`) has zero callers and is
   inconsistent with `apply_total`** (it does not zero `determinate_done`). Delete
   it or route it through `apply_total`; do not leave a second way to set a total.
10. **The observability gap is the frame *content*.** Test 3
    (`progress.rs:533-564`) reads `term.contents()` but asserts only a line count.
    Asserting on `"analyzing"` and `format_bytes(2048)` in the rendered string is
    nearly free and would catch label, formatting, and row-population regressions
    that all twelve current tests miss.
