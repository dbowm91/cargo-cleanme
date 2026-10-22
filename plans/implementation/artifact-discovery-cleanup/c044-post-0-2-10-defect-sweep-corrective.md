# C044 — Post-0.2.10 Defect Sweep Corrective

Status: **closing** — implemented and ladder-green in the working tree on
`main` at `cf4a6a0` (0.2.10, unpublished; latest published tag `v0.2.10`).
Not published: no tag, no release, no crates.io publication. Evidence in
`plans/closure/artifact-discovery-cleanup/c044-status.md`.

Date: 2026-10-22.
Repository baseline: `main` at `cf4a6a0` (0.2.10, unpublished).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: M014 (the interactive selection surface),
`plans/implementation/artifact-discovery-cleanup/m014-*.md`; C039 (the
compact selection surface),
`plans/implementation/artifact-discovery-cleanup/c039-rip-style-compact-selection-surface-corrective.md`;
C042 (the interactive rendering and cursor lifecycle),
`plans/implementation/artifact-discovery-cleanup/c042-interactive-surface-rendering-corrective.md`;
C043 (the interactive frame geometry),
`plans/implementation/artifact-discovery-cleanup/c043-interactive-frame-geometry-corrective.md`;
C036 (documentation drift),
`plans/implementation/artifact-discovery-cleanup/c036-cli-surface-truthfulness-and-documentation-drift-corrective.md`.
Original closure records:
`plans/closure/artifact-discovery-cleanup/m014-status.md`,
`c039-status.md`, `c042-status.md`, `c043-status.md`.
Related: ADR 001 (raw mode and alternate-screen rendering rejected for
*progress*), ADR 006 (the selection is a filter over already-proven units),
ADR 007 (the scoped proof boundary), ADR 008 (the canonical Full spelling and
the root front door), overview findings 23, 53, 54 and 59.

## Objective

Repair thirteen defects found by an interrogation of the 0.2.10 tree on
2026-10-22, and one optimisation. The defects are listed as D1–D13 below;
D14 records an examined-and-not-reproduced report.

The sweep was ordered by the repository's own rule — *nothing is deleted on
the strength of a timestamp; every candidate must first prove exclusive
ownership of its physical bytes, and that proof must still hold at the moment
of deletion* — and **D1 is the one that breaks it**: the tool deleted a
package's checked-in files and reported success.

## Non-goals

- **Not** a change to the cleanup contract. Cleanup remains Cargo-mediated and
  fail-closed; deletion is always `cargo clean`, never direct filesystem
  removal; the three exit codes, the JSON envelope, the reason codes and the
  scope rules are untouched.
- **Not** a change to the interactive selection contract. What the operator
  sees is still the engine's own unit set, a selection can only *remove*
  candidates, and the committed roots are still the checked roots in candidate
  order.
- **Not** a new feature. Nothing here adds a flag, a mode, a configuration
  key, a reason code, or a dependency. The two behaviour changes that exist
  (D1's refusal, D2's usage error) both make an existing promise true.
- **Not** the C036 documentation-drift corrective. C036 stays open; this
  corrective touches only the documents that describe what it changed, and
  records the census correction (D12) here.
- **Not** a publication. The corrective is implemented and ladder-green, and
  stays **open** until a published release carries it.
- **Not** a pty harness. Finding 48 remains open and is still the right next
  step for the parts a byte-level model cannot prove.

## Current implementation evidence

Every defect below was reproduced against a locally built 0.2.10 binary or
against the crate's own test doubles, before the fix. The reproduction is
named; the fix is named; the regression test that would have failed before the
fix is named.

### D1 — critical — an output root *inside* another package's directory was classified `PrivateBounded` and cleaned

`contains(c, &m.source_root)` refused an output root that *contains* a
member's source tree. Its mirror — an output root that *lives inside* one —
was never tested, and the only net was `is_conventional_source_dir`, which
knows the names `src`, `tests`, `benches` and `examples`. The per-spawn
re-proof in `cleanup.rs` had the identical gap.

The victim does not have to be a workspace the tool could resolve. Cargo
refuses a nested package that is not a workspace member ("current package
believes it's in a workspace when it's not"), so the package sitting inside the
output root is exactly the one the resolved universe does *not* contain — and
the member-source check cannot see it.

**Reproduction.** A workspace `app` whose `.cargo/config.toml` sets
`build.target-dir = "lib1/assets"`, with an independent package `lib1` nested
in its tree, `lib1/assets/logo.txt` checked in, and `lib1/src/lib.rs`
including it. The group was reported `ownership: "private"` and the run
deleted `lib1/assets` outright, reporting `12.00 KiB reclaimed`.

**Fix.** `workspace.rs` now classifies a covering root that is strictly inside
a *package directory* — a directory holding a `Cargo.toml` — as `Uncertain`,
with its own diagnostic (*"output root … lies inside a package directory"*).
The walk is bounded by the owning workspace's own root, so the ordinary
`<ws>/target` is unaffected. `cleanup.rs` re-proves the same shape against
the filesystem at the moment of the spawn.

**Why the verification missed it.** Every existing guard tested the *other*
direction, and the only fixture with a nested package had an unresolved
manifest, which the scope block caught for an unrelated reason — so the clean
never ran and the test passed for the wrong reason. The name-based net covers
four conventional names and the fixture's victim directory was called
`assets`.

**Regression evidence.** `workspace::tests::an_output_root_nested_inside_a_package_directory_is_never_private`,
`workspace::tests::the_ordinary_target_directory_is_not_a_nested_package_overlap`,
`cleanup::tests::an_output_root_inside_a_package_directory_is_never_cleaned`.
The cleanup test asserts the *premise* (both workspaces resolve) so it cannot
pass for the scope block's reason.

### D2 — high — a root `--profile` silently discarded a `clean --package`

`--profile` and `--package` are mutually exclusive at one level, and clap
enforces that. Across the root/`clean` boundary nothing did: `merge_overrides`
merged the two independently, so the profile won and the package vanished. The
run exited **0**, reported `selector_kind: "profile"`, and the string `demo`
appeared nowhere in stdout, stderr or JSON. The consequence is not a wrong
report; it is a *different destructive action* than the one that was asked for.

**Fix.** `merge_overrides` checks the pair before merging either member, and
the cross-level combination is a usage error (exit `2`) naming both flags.

**Why the verification missed it.** The existing test covered the *same-level*
pair, which clap refuses; the cross-level spelling was never parsed.

**Regression evidence.** `cli::tests::a_selector_at_the_root_and_the_other_on_clean_is_a_usage_error`.

### D3 — medium — the interactive empty-result row was not truncated

`pad` does not truncate. The literal is 37 cells wide, so below 37 columns the
row wrapped, the frame occupied one row more than its line count, and the
erase — which moves up by the line count — left the **title** standing above
the operator's prompt.

**Fix.** The row is truncated like every other line in the frame.

**Why the verification missed it.** C043's `the_frame_fits_the_terminal_in_both_axes`
never applies a filter, so it never rendered the empty row. This is C043's
defect class, not a new one.

**Regression evidence.** `interactive::tests::the_empty_result_row_is_truncated_to_the_frame_width`,
which replays the frame through the crate's own `TinyTerminal` emulator and
asserts that no row survives the erase.

### D4 — medium — the ownership column vanished at 27–30 columns

`columns()` computed the ownership column's width as
`class.min(budget - MIN_NAME_WIDTH)`. Below the budget that yields **zero**,
and a zero-width column is dropped — together with its yellow styling. A
`Shared` (inventory-only) unit then rendered byte-identically to a cleanable
one, at exactly the widths where the operator has the least room to check.

**Fix.** The reclaimable-size column yields first: when the ownership column
would not fit, the size column is dropped and the ownership column keeps the
full class label. A one-cell column carries the class's first letter (`p`,
`s`) rather than a truncation, which would be an ellipsis and would mark
nothing.

**Why the verification missed it.** The width table was derived from the
formula, and the formula's zero case was never rendered.

**Regression evidence.** `interactive::tests::the_ownership_column_survives_every_legal_terminal_width`.

### D5 — medium — a staging-cleanup failure claimed the executable was replaced

The cleanup error path reported *"the executable was replaced, but its staging
directory … could not be removed"* regardless of the receipt. `RolledBack`
means the old binary was **restored**; the claim was false.

**Fix.** The message follows the receipt's own disposition.

**Why the verification missed it.** The existing test exercised the committed
path, where the claim happens to be true.

**Regression evidence.** `update::tests::the_staging_cleanup_message_follows_the_receipt_disposition`.

### D6 — medium — `UpdateError::Receipt` dropped the retained backup's path

`receipt.recovery_path()` exists in eggup-core and was never read. The
`RecoveryRequired` case retains the only copy of the user's previous working
binary and the only pointer to it, and the error carried neither the path nor
the rollback failure. The variant's own doc comment contradicted its meaning.

**Fix.** The variant carries `recovery_path` and `rollback_failure`, and the
`Display` implementation reports them.

**Why the verification missed it.** No test constructed a `RecoveryRequired`
receipt, and the `Display` had no case for the fields because it had none.

**Regression evidence.** `update::tests::a_receipt_error_carries_the_retained_backup_and_the_rollback_failure`.

### D7 — medium — a scope-conflict message named a flag the operator never typed

`duplicate_override("--known")` said *"--known was given at the root and on
`clean`"* when `--known` was given only on `clean`; the flag that actually
collided was the root's own `--full` or `ROOT`. The rejection was right; the
message was false.

**Fix.** The message names the two flags that actually collide.

**Why the verification missed it.** The message was asserted only by its
presence, never by its content.

**Regression evidence.** `cli::tests::a_scope_conflict_names_the_flags_that_actually_collide`.

### D8 — low — the coloured all-toggle row overflowed the frame

The plain row was built from truncated pieces; the coloured row was rebuilt
from the raw label, so it was two cells wider than the frame on a narrow
terminal, it wrapped, and the erase left residue above the prompt.

**Fix.** The coloured row is built from the same truncated pieces as the plain
one.

**Why the verification missed it.** The width tests used the plain path; the
cursor starts on the all-toggle row, which *is* the plain path.

**Regression evidence.** `interactive::tests::the_coloured_all_row_fits_the_frame`.

### D9 — low — the staging directory was not private

`std::fs::create_dir` asks the platform for `0o777` and lets the umask take
the rest away. Under the common `0002` umask the staging directory is `0775`,
and under `0000` it is `0777`, while the contract calls it *private* and
`eggup-core` uses `0700`/`0600`.

**Fix.** The directory is created with an owner-only mode, at creation, so
there is no window in which it is wider than it should be.

**Why the verification missed it.** The mode was never asserted; the fixture
created its own directory and never looked at it.

**Regression evidence.** `update::tests::the_staging_directory_is_private_to_its_owner`.

### D10 — low — the hidden `scan --full` alias was advertised

`scan --full` is a hidden alias of `scan --all`. It was a separate argument,
and it was a member of the `scan-scope` `ArgGroup` — and an `ArgGroup` renders
**every** one of its members in the usage line. The alias therefore appeared
in `scan --help`'s synopsis and in four of the five generated completions,
which is what "hidden" is supposed to mean. (`architecture/overview.md` §10
finding 23's premise — that the alias "does not appear in `scan --help`" —
was false; finding 23's *primary* claim, that a user-facing diagnostic points
at `scan --full`, is unaffected and stays open under C036 D3.)

**Fix.** The alias is an alias of the `--all` argument rather than an
argument of its own, so it is not a group member and is not rendered.

**Why the verification missed it.** The existing test asserted that the
argument existed and was hidden — which is the *shape* that causes the defect,
not the property.

**Regression evidence.** `cli::tests::hidden_full_alias_is_not_advertised_in_scan_help`,
rewritten to assert the property (no `--full` in the usage line or the help)
rather than the shape.

### D11 — low — the progress tick-semantics claim was false

The comment claimed that `tick` advances the spinner glyph in both modes.
Production installs `enable_steady_tick`, and indicatif's `tick` is a **no-op**
once a ticker is installed (indicatif `progress_bar.rs:226-231`): there the
ticker thread owns the glyph. The in-memory renderer installs no ticker, so
the same call is what drives its glyph. The two modes differ in *who*
advances the spinner, not in what the operator sees — and the comment, and
the deep dive that repeated it, said otherwise.

**Fix.** The comment and `architecture/11-progress.md` state the difference
accurately. No behaviour changed: installing a ticker in the in-memory
renderer would make the test frame advance on a background thread, which is
worse than the difference it would remove.

**Why the verification missed it.** The claim was prose, and no gate reads
prose.

**Regression evidence.** None — this is a documentation defect, and the
correction is the correction.

### D12 — medium — the authoritative module-line census was wrong

`architecture/overview.md` §3 is the source of truth for the module line
counts. It reported 38,641 lines while citing `wc -l src/*.rs`, which yields
**38,642**; production 17,695 against a measured **17,694**;
`interactive.rs` production 1724 against **1722**; `main.rs` 2258/1878 against
**2259/1879**. The two wrong modules are the two C043 touched.

**Fix.** The census was re-derived from `src/` with a brace-tracking parser
that excludes every `#[cfg(test)]` scope, and the counts were re-derived
*after* this corrective's own code changes, because a census is a statement
about the tree and this corrective changed the tree. The same pass corrected
two stale per-platform test counts and two stale prose figures in the deep
dives, and the C043 closure record's historical figures were **left alone**:
a closure record is history and is corrected forward, not edited.

**Why the verification missed it.** `check-doc-citations.py` resolves a
citation's *existence* and *range*; it cannot judge a number's value, and the
census has no gate at all.

**Regression evidence.** None — a census has no gate. The derivation is
recorded in the closure record so the next reader can re-derive it rather
than trust it.

### D13 — optimisation — every source directory was read twice

The nested-repository marker memo recorded only *positives*, so the answer
that mattered — "no marker here" — was thrown away and recomputed by the
entry pass. Measured on a 2,000-directory source tree: **16,025 → 12,022**
`getdents64` calls (**−25%**) and a median wall clock of 0.13 s → 0.11 s.

**Fix.** Both answers are recorded. The memo is created per walk and dropped
with it, so its size is bounded by the number of directories that walk
visited; the bound the old design traded away was never a lifetime bound.

**Why it was missed.** The design comment justified the positives-only memo
with a *tree-size* bound, and the memo was already per-walk.

**Regression evidence.** `traverse::tests::the_marker_memo_records_both_answers`.

### D14 — examined and not reproduced

Three load-sensitive tests were reported to be sensitive to host load. They
**passed** on this 16-core host under idle conditions, under eight busy
loops, under three concurrent `cargo build` runs, and under 32 busy loops with
each test pinned to one CPU. The report is recorded as **not reproduced**, not
as refuted: a load-sensitive assertion stays load-sensitive, and this host
simply could not make the premise hold. `worker_threads()` clamps to eight.

## Verification

The full ladder is green on the corrected tree:

```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features                  # 625 passed, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
python3 scripts/check-release-contract.py
```

The inline test census moved from 534 declared / 532 on Linux / 529 on macOS
/ 490 on Windows to **546 / 544 / 541 / 500**; the Windows figure is
source-derived, as the previous one was.

**Not verified here:** the MSRV lane (toolchain 1.89) and the hosted
ubuntu/macos/windows lanes. The changes touch path comparison, file modes and
terminal rendering, so the hosted lanes are the ones that can disagree.

## What stays open

- **Publication.** This corrective is implemented and ladder-green and is
  **not** published, so it stays open (publication is not closure).
- **C036** stays open, including D3 — the user-facing diagnostic that points
  the operator at `scan --full`. D10 removed the alias from the *help*; it did
  not change the diagnostic.
- **C043's** own class — the frame's geometry — is repaired in three places
  (the empty row, the coloured all-toggle row, the ownership column), and the
  pty harness (finding 48) remains the right next step.
- **The census has no gate.** D12 corrected the numbers; the *mechanism* that
  let them drift — a hand-maintained figure with nothing over it — is
  unchanged, and the drift is expected to recur until a gate exists. This is
  the same mechanism as C036's D29.
