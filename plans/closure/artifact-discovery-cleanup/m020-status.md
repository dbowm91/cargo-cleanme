# M020 — A Quiet Human Output Surface: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**: publication is not closure, and no
release has carried this change yet.

Plan: `plans/implementation/artifact-discovery-cleanup/m020-quiet-human-output-surface.md`

Owner directive: 2026-10-21 — *"we need to cleanup the output of the
scan/clean/bare etc commands. it's very messy/noisy … we do NOT want to see
potentially thousands of lines of lines that were not picked, as this is
essentially debugging information and is contraindicated for general use. we
can add a --verbose command for this."*

Repository baseline: `main` at `1a55f9e` (v0.2.10, published).

Implementation: `12385e5` (`src/progress.rs`, `src/report.rs`, `src/cleanup.rs`,
`src/cli.rs`, `src/main.rs`, `tests/cli_contract.rs`, `tests/end_to_end.rs`, the
five regenerated completions and the root man page).

## The executive finding

The tool was **correct and exhausting**. Every one of its decisions was right,
and the operator paid for the audit trail of all of them on every single run.

Three separate defects in the *presentation*, none in the *decisions*:

1. **The progress frame was six terminal lines.** M005A §7.4 specified "at most
   five reportable directories/output groups beneath the main bar", and the
   renderer drew them. On a real run those five rows were empty — a spinner
   glyph and nothing else — for almost the whole traversal. The operator was
   given six lines of terminal height to learn nothing.

   Worse: the *phase* was written into the bar's **prefix**, and indicatif's
   default spinner template is `{spinner} {msg}` — it does not render the prefix
   at all. The transitions from discovery to resolution to analysis were
   **never visible**. The bar said "scanning…" for the whole run, whatever it
   was doing.

2. **The report printed a row per workspace considered.** On a machine with a
   thousand Cargo workspaces that is a thousand lines, and the overwhelming
   majority of them say "skipped". That is debugging information, and it is
   contraindicated for general use: it is the *result* an operator wants, and
   the result is the bytes that were freed.

3. **The preamble was a scope census** — roots, manifest counts, resolved
   workspace counts, CleanupUnit counts, unresolved-participant counts. A count
   of things considered, not a safety disclosure.

## What the surface says now

**While it runs**, exactly one line, erased and redrawn in place, and cleared
before the report:

```
⠁ scanning…
⠁ resolving workspaces…
⠁ analyzing output 3/12…
```

**When it finishes**, one line per directory the run acted on, and one tally:

```console
$ cargo cleanme clean /tmp/m020-demo --no-progress
Cleaned  alpha/target  7.66 MiB
Cleaned  beta/target   7.66 MiB
Cleaned  delta/target  7.66 MiB
Cleaned  gamma/target  7.66 MiB

30.64 MiB reclaimed from 4 directories
```

A simulation says what it would do, and says that it did nothing:

```console
$ cargo cleanme clean /tmp/m020-demo --dry-run --no-progress
Simulated  alpha/target  7.66 MiB
Simulated  beta/target   7.66 MiB
Simulated  delta/target  7.66 MiB
Simulated  gamma/target  7.66 MiB

30.64 MiB would be reclaimed from 4 directories (simulation only — no `cargo clean` command was invoked)
```

`--verbose` restores the previous surface in full — the census, every unit
including the skipped ones, the ownership class, the before/after figures, the
per-unit detail, the counters and the diagnostic fan-out:

```console
$ cargo cleanme clean /tmp/m020-demo --dry-run --no-progress --verbose
combined roots /tmp/m020-demo
combined scope: 1 root(s) [/tmp/m020-demo], 4 manifest(s), 4 resolved workspace(s), 4 CleanupUnit(s), 0 unresolved ownership participant(s)
Simulated  /tmp/m020-demo/alpha  [private]  output /tmp/m020-demo/alpha/target  before   7.66 MiB  — simulation: no `cargo clean` command was invoked
Simulated  /tmp/m020-demo/beta  [private]  output /tmp/m020-demo/beta/target  before   7.66 MiB  — simulation: no `cargo clean` command was invoked
Simulated  /tmp/m020-demo/delta  [private]  output /tmp/m020-demo/delta/target  before   7.66 MiB  — simulation: no `cargo clean` command was invoked
Simulated  /tmp/m020-demo/gamma  [private]  output /tmp/m020-demo/gamma/target  before   7.66 MiB  — simulation: no `cargo clean` command was invoked

simulation: 4 would-clean, 0 skipped, 0 failed; estimated would-clean  30.64 MiB; no `cargo clean` command was invoked; 0 filesystem diagnostics
```

A scan keeps every group — the group list *is* a scan's answer — and only
shortens the paths:

```console
$ cargo cleanme scan /tmp/m020-demo --no-progress
alpha/target  7.66 MiB  [private]
beta/target   7.66 MiB  [private]
delta/target  7.66 MiB  [private]
gamma/target  7.66 MiB  [private]

 30.64 MiB inventory estimate across 4 inactive Cargo output groups
```

## The relative-path rule

A path under a scope root is shown **relative to that root**. A path under no
scope root stays **absolute**. A **one-component** remainder is qualified with
the scope root's own name, so `~/projects/my-app` + `target` reads as
`my-app/target` rather than a bare `target`: a bare `target` says nothing about
*which* project it belongs to, and two scope roots that each hold a `target`
child would produce two indistinguishable rows. The path *is* the scope root →
the root's own name. A **Full scope** has no single root, so its paths stay
absolute; a relative path under `/` would name a fragment, not a place.

## What did not change

- **The set of cleaned directories.** Identical with and without `--verbose`.
- **Every proof, policy decision, scope resolution and exit code.** The flag
  changes a *rendering shape* and nothing else.
- **The machine contract.** Verified byte-identical (§ below).
- **The ADR 007 safety disclosures.** The scope block, the
  unresolved-participant listing and the out-of-scope count are **never**
  withheld. They say what the report *means*, not how long it is: dropping them
  because the report got shorter would turn "boundary clean" into "system
  clean", which is the M012A defect in a new place. A **failed** unit is
  always listed, for the same reason — a failure is not a skip.

## The machine contract is untouched

`--verbose` is a rendering flag, so it must not move a byte in the versioned
machine contract. Measured on the same fixture and the same filesystem state
(a simulation, so the two runs see the same bytes):

```
JSON: byte-identical (2144 bytes)
LOG:  byte-identical (105 bytes)
exit codes: json quiet=0 verbose=0
```

Pinned by `the_machine_contract_is_identical_with_and_without_verbose`
(`tests/cli_contract.rs`), which compares the two documents for both formats
and the exit codes.

## A real defect the rewrite found

`cleanup.rs`'s private `escaped_path` returned a valid-UTF-8 path **unchanged**,
so a directory whose name contained a newline or an escape sequence wrote that
sequence straight into the terminal — the exact hazard `report::path_text` was
written to prevent. The machine surface already escaped, so the two surfaces
disagreed about the same path. Both surfaces now render through
`HumanOptions::path` → `path_text`, and `escaped_path` is gone. Recorded as
finding 63 in `architecture/overview.md` §10.

## The tests

Ten discriminating cases, each naming its failing input:

| Test | Fails if |
|---|---|
| `coordinated_renderer_owns_exactly_one_bar_on_one_draw_target` | a second bar ever appears |
| `in_memory_frame_contains_exactly_one_progress_line_naming_the_phase` | the frame grows a second line, or the phase is written where the template does not render it |
| `the_verbose_surface_adds_counters_to_the_same_single_line` | the counters go onto a second line, or the verbose renderer stops showing them |
| `the_quiet_surface_carries_no_counters` | a counter is added to the quiet message |
| `the_quiet_cleanup_report_lists_only_what_the_run_acted_on` | the quiet surface re-admits a skipped row |
| `the_quiet_tally_states_the_space_saved` | the tally is dropped, or regresses to the census |
| `execute_and_simulation_words_their_tallies_differently` | one mode's wording leaks into the other |
| `a_failed_unit_is_always_listed` | the failure filter treats a failure like a skip |
| `the_verbose_surface_restores_the_skipped_rows_and_the_census` | the flag is parsed but not plumbed |
| `the_safety_disclosures_survive_the_quiet_surface` | the quiet surface drops a disclosure |

Plus, in `tests/cli_contract.rs`: `the_machine_contract_is_identical_with_and_without_verbose`,
`a_bounded_cleanup_names_its_directories_relative_to_the_scope_root`, and
`the_quiet_human_surface_is_a_row_per_cleaned_directory_and_one_tally`.

Three existing tests were **updated rather than deleted**, and each keeps its
original property:

- `execution_reports_observed_decrease_deduped` now asserts the Execute
  wording (`reclaimed`, and *not* `would be reclaimed`) and that the verbose
  surface keeps the precise `observed decrease` phrase.
- `final_report_lists_all_groups_and_deduped_total` now asserts the deduped
  total (`560.00 B`), the new tally, and that the verbose surface keeps the
  census and the per-mode summary.
- `scope_note_in_second_root_does_not_block_first_root` now takes its census
  assertion from the verbose surface, because the census is the verbose
  surface's.

Four `tests/cli_contract.rs` cases that assert the **diagnostic fan-out** now
pass `--verbose`, because that fan-out is the verbose surface's. Their
premises and their assertions are unchanged; only the rendering shape they ask
for changed.

## The verification ladder

All seven steps green on the final tree:

```
cargo fmt --all -- --check                                        OK
cargo clippy --all-targets --all-features -- -D warnings          OK
cargo test --all-targets --all-features
    487 passed (lib) · 4 passed (bin) · 74 passed (cli_contract)
    4 passed (end_to_end) · 0 failed
python3 scripts/check-fixture-portability.py --self-test && …      OK (37 fixtures clean)
python3 scripts/check-doc-citations.py --self-test && …            OK (17 documents clean)
cargo run --features dev-tools --bin generate-docs -- --check     OK (13 artifacts match)
python3 scripts/check-release-contract.py --self-test && …        OK (5 targets, 11 assets)
```

The inline-test census moved from 481 to **493** declared (491 Linux, 453
Windows, 488 macOS), and the module line counts were re-derived against the
tree by a brace-tracking parser calibrated to reproduce the published baseline
exactly before it was applied.

## Residuals, disclosed

- **The one-component qualification is a rule, not a coincidence.** It is
  stated in `docs/USAGE.md`, in `architecture/10-reporting.md` and in the
  `HumanOptions::relativized` doc comment. A future reader who finds it
  surprising should find it explained, not merely implemented.
- **A Full scan's paths stay absolute.** There is no single root to be
  relative to, and inventing one would name a fragment.
- **The quiet surface still prints one line per *cleaned* directory.** A run
  that cleans a thousand directories prints a thousand lines. That is the
  result, not the audit trail — but it is a real ceiling, and a machine with
  a thousand genuinely-cleanable workspaces will still be verbose. The next
  step, if it is ever wanted, is a summary mode; it is not this milestone.

## Open

- **Publication.** No release has carried this change. The milestone stays
  open until one does.
- **The MSRV and cross-platform lanes have not run.** CI runs a separate lane
  on toolchain 1.89 and a matrix over ubuntu/macos/windows. Nothing in this
  change touches the stdlib API surface, and no fixture, path comparison,
  mtime or symlink handling changed — but the hosted lanes are the only proof
  of that, and this record does not claim it.
