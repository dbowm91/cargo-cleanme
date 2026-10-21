# M020 — A Quiet Human Output Surface (one line while it works, one line per directory cleaned, one tally)

Status: **active** — owner-directed 2026-10-21 and implemented in the working
tree. **Not closed**: it has not been carried by a published release, and
"publication is not closure" is not a formality here.
Date: 2026-10-21.
Repository baseline: `main` at `1a55f9e` (0.2.10 published).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Owner directive: 2026-10-21 — *"we need to cleanup the output of the scan/clean/bare
etc commands. it's very messy/noisy … we do NOT want to see potentially thousands
of lines of lines that were not picked, as this is essentially debugging information
and is contraindicated for general use. we can add a --verbose command for this."*
Related: M005A (the five transient progress rows this milestone retires), C002
§7.4–§7.5, C003 §7.5, M017 / ADR 007 (the safety disclosures that must survive
the quiet surface), ADR 008 (the Full cleanup's proof boundary — untouched here),
`plans/output-schema-v1.md` (the machine contract — untouched here).

## Objective

Make the human surface say what happened and stop there.

- **While the command runs:** exactly **one** line, carrying a spinner and the
  phase. It is cleared before the report is printed. No second line, no
  candidate rows, no counter salad.
- **When it finishes:** **one line per directory the run actually cleaned** (or
  would clean, in a preview or a simulation), with the size, and the path
  spelled **relative to the scope root** whenever the scope is bounded.
  Directories the run *skipped* are not listed at all.
- **At the bottom:** **one tally line** that states the space saved in words a
  person can act on.
- **`--verbose`** restores the detailed surface for the people who want it.

## Non-goals

- **Not** the machine contract. `--format json` and `--format log` are
  unchanged, byte for byte, with and without `--verbose`. The JSON envelope is
  versioned (`plans/output-schema-v1.md`); the human surface is not.
- **Not** the cleanup engine, the ownership proof, the policy evaluation, the
  scope resolution, or any exit code. This milestone changes what is *printed*.
- **Not** a change to *what* is cleaned. The set of cleaned directories is
  identical with and without `--verbose`.
- **Not** the removal of the safety disclosures. ADR 007's "boundary clean must
  not read as system clean" obligations survive the quiet surface unchanged —
  see "What the quiet surface may not hide".
- **Not** a change to the `--stats` line, which is a separate stderr channel.

## The problem, as observed

On a bounded fixture with four cleanable workspaces, `cargo cleanme clean .`
prints:

```
combined roots /tmp/cc-demo
combined scope: 1 root(s) [/tmp/cc-demo], 4 manifest(s), 4 resolved workspace(s), 4 CleanupUnit(s), 0 unresolved ownership participant(s)
Cleaned  /tmp/cc-demo/alpha  [private]  output /tmp/cc-demo/alpha/target  before   7.66 MiB  after   0.00 B  observed decrease   7.66 MiB  — Removed 42 files, 7.6MiB total
Cleaned  /tmp/cc-demo/beta  [private]  output /tmp/cc-demo/beta/target  before   7.66 MiB  after   0.00 B  observed decrease   7.66 MiB  — Removed 42 files, 7.6MiB total
Cleaned  /tmp/cc-demo/delta  [private]  output /tmp/cc-demo/delta/target  before   7.66 MiB  after   0.00 B  observed decrease   7.66 MiB  — Removed 42 files, 7.6MiB total
Cleaned  /tmp/cc-demo/gamma  [private]  output /tmp/cc-demo/gamma/target  before   7.66 MiB  after   0.00 B  observed decrease   7.66 MiB  — Removed 42 files, 7.6MiB total

cleanup: 4 cleaned, 0 skipped, 0 failed; pre-clean estimate  30.64 MiB, post-clean measured   0.00 B, observed decrease  30.64 MiB; 0 filesystem diagnostics
```

and, with a TTY attached, the live frame is **six lines** tall — one main bar
plus five transient candidate rows — of which the five rows are blank apart
from a spinner glyph for almost the whole run.

Three separate problems, one owner directive:

1. **The progress frame is six lines, not one.** The five candidate rows were
   specified by M005A §7.4 ("show at most five reportable directories/output
   groups beneath the main bar"). They are transient and mostly empty, and the
   owner has now rejected them.
2. **The report prints a row per unit considered.** On a machine with a
   thousand Cargo workspaces that is a thousand lines, and the overwhelming
   majority of them say "skipped". That is debugging output.
3. **The preamble is a scope census** — roots, manifest counts, resolved
   workspace counts, CleanupUnit counts, unresolved-participant counts. Useful
   once, to someone debugging; noise the other thousand times.

## The new human surface

### While it runs

Exactly **one** line, on stderr, cleared before the report is printed:

```
⠁ scanning…
⠁ analyzing 3/12…
```

- One `ProgressBar`. No `MultiProgress`, no rows, no second line.
- The **phase** is the message. The phase transitions (`Discovery`,
  `Resolution`, `Analysis`, `Cleanup`) are visible, which they are not today:
  the current renderer writes the phase into the bar's *prefix*, and indicatif's
  default spinner template does not render the prefix at all.
- `--verbose` adds the counters to the same line (`visited 21 pruned 4
  manifests 4 workspaces 4 …`). Still **one** line.
- The bar is cleared with `finish_and_clear()` before the report is printed, so
  the terminal shows the report and nothing else.

### When it finishes

One line per directory the run acted on, columns aligned, path relative to the
scope root when the scope is bounded:

```
Cleaned  alpha/target    7.66 MiB
Cleaned  beta/target     7.66 MiB
Cleaned  delta/target    7.66 MiB
Cleaned  gamma/target    7.66 MiB

30.64 MiB reclaimed from 4 directories
```

- The **path** is the physical output root — the directory Cargo emptied. When
  a unit has more than one, they are joined on the same line; the line count
  stays one per unit.
- The **size** is the bytes that were reclaimed (`observed decrease`), the bytes
  that would be reclaimed (simulation), or the pre-clean estimate (Cargo
  preview).
- The **tally** states the space saved in plain words. Execute says
  `reclaimed`; a simulation or a preview says `would be reclaimed`. The two
  are distinguishable by substring, and the wording distinction is asserted by
  a test.
- **Skipped units are not listed.** Their count appears in the tally as
  `(N skipped)` when it is not zero, so the run is still honest about what it
  did not pick — in one number, not one line each.
- **Failed units are always listed.** A failure is not a skip; silence about it
  would be a silent failure.

### Verbose

`--verbose` restores the detailed surface: every unit (including skipped
ones), the scope census, the output roots, the ownership class, the before and
after figures, the per-unit detail, and the diagnostics detail on stderr.

`--verbose` changes **only the human format**. The JSON document and the log
line are identical with and without it.

### Relative paths

A path is shown relative to the scope root that contains it. The rule:

- bounded scope (explicit root, or the Routine scope) → relative to the
  containing scope root;
- Full scope → absolute. A Full scan has no single root to be relative to, and
  a relative path under `/` would be a lie.

A path that no scope root contains stays absolute. The scope root itself is
shown as `.`.

## What the quiet surface may not hide

ADR 007's obligations are about *what the report means*, not about how long it
is. The quiet surface keeps every one of them:

| Disclosure | Why it stays |
|---|---|
| the scope-block message | the run refused; a silent refusal is a silent failure |
| the unresolved-participant listing | the operator must know which manifest to fix |
| the out-of-scope unprovable count | "boundary clean" must not read as "system clean" |
| failed units | a failure must never be silent |
| the scope-block hint text | it explains what to do next |

The **scope census** (roots, manifest counts, workspace counts, unit counts)
moves behind `--verbose`. It is a count of things, not a safety disclosure: it
tells the reader how much was considered, and the quiet surface says how much
was *acted on* in the tally.

## Engine

- `cli.rs` gains a **global** `--verbose` flag on the root `Cli`. Global, not
  per-subcommand: it selects a rendering shape, and every report-producing
  subcommand renders one. It does not collide with any existing argument id.
- `RunOptions` (`src/main.rs:63-69`) gains `verbose: bool`.
- `report.rs` gains `HumanOptions { roots, verbose }` — the rendering shape
  and the path base. One type, used by both report renderers.
- `report::render` and `CleanReport::render` take `&HumanOptions`.
- `progress::IndicatifRenderer::new(hidden)` becomes
  `new(hidden, verbose)`.
- `progress::coordinated_bar_count()` returns **1** for a visible renderer and
  **0** for a hidden one. The "six bars" figure is retired.

## Tests

Every test names the input that makes it fail.

1. **The live frame is one line.** A visible renderer draws exactly one
   progress line. *Fails if:* the renderer ever owns a second bar.
2. **The phase is visible.** The rendered line contains the phase label.
   *Fails if:* the phase is written to a prefix the template does not render —
   the defect that hides the phase today.
3. **The quiet cleanup report lists only what was acted on.** A report with one
   cleaned unit and three skipped units renders exactly one unit row, and the
   skipped units' paths appear nowhere. *Fails if:* the quiet path re-admits
   the skipped rows.
4. **The tally states the space saved.** The last line of the quiet report
   contains the byte figure and the word `reclaimed`. *Fails if:* the tally is
   dropped or the wording regresses to the census.
5. **Execute and simulation wording differ.** Execute renders `reclaimed` and
   not `would be reclaimed`; simulation renders `would be reclaimed`. *Fails
   if:* one mode's wording leaks into the other.
6. **Failures are never silent.** A failed unit's path is in the quiet report
   even though no other skipped unit is. *Fails if:* failures are filtered with
   the skips.
7. **`--verbose` restores the detail.** The verbose report contains the
   skipped units' paths and the scope census. *Fails if:* the flag is parsed
   but not plumbed.
8. **The machine contract is untouched.** The JSON document and the log line
   are byte-identical with and without `--verbose`. *Fails if:* a rendering
   option leaks into the projection.
9. **Bounded paths are relative; Full paths are absolute.** A bounded cleanup
   renders `alpha/target`; a Full-scope render keeps the absolute path.
   *Fails if:* the relativization leaks into the Full scope.
10. **The safety disclosures survive.** The scope-block message, the
    unresolved-participant listing, and the out-of-scope count are in the
    quiet report. *Fails if:* the quiet path drops a disclosure.

## Documentation updates

- `docs/USAGE.md`, `README.md` — the sample transcripts.
- `architecture/02-cli.md`, `10-reporting.md`, `11-progress.md`,
  `13-orchestration.md` — the deep dives, with citations re-derived from the
  new source rather than shifted arithmetically.
- `.skills/cli-surface-and-argv/SKILL.md`, `.skills/terminal-and-interactive/SKILL.md`
  — the flag surface and the progress contract.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`,
  `plans/registry.md` — this milestone's registration.

## Verification commands

The ordinary ladder, in order, stopping at the first failure:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

The last step **must fail before the artifacts are regenerated** — that is the
drift gate proving it detects this milestone's change.

## Acceptance criteria

- The ten tests above exist, pass, and each names its failing input.
- A live run on a bounded fixture prints one progress line, one line per
  cleaned directory, and one tally — and `--verbose` restores the old surface.
- The JSON and log outputs are byte-identical with and without `--verbose`.
- The safety disclosures are present in the quiet report.
- The generated artifacts are current.
- The ladder is green.

## Stop conditions

Stop and write a corrective rather than continuing if:

- the quiet surface cannot be produced without changing what is cleaned, what
  is proven, or any exit code;
- a safety disclosure turns out to depend on the per-unit rows it would
  remove;
- the JSON or log projection would have to change to carry the new options;
- any test would have to assert an unrelated failure to pass.

## Closure evidence requirements

A closure record is written under `plans/closure/artifact-discovery-cleanup/`
and must carry:

1. the **live** before-and-after transcripts for `scan`, `clean`, and a bare
   run, with their exit codes;
2. the **byte-identity** evidence for the JSON and log surfaces with and
   without `--verbose`;
3. the **ladder's** results, quoted.

Publication is a separate decision. A milestone that is fixed but not yet
carried by a published release stays **open**.
