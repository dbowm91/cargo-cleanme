# C044 — the post-0.2.10 defect sweep — closure record

Plan: `plans/implementation/artifact-discovery-cleanup/c044-post-0-2-10-defect-sweep-corrective.md`
Status: **closed — shipped in v0.2.11** (tag `v0.2.11` at `1d679833ae62`, GitHub release published 2026-10-25T17:23:10Z and immutable with a valid attestation, crates.io sha256 `96298f83…`, automatic smoke `40216211513` green on five targets). Receipt:
`plans/closure/distribution-release-update/r031-status.md`.
Baseline: `main` at `cf4a6a0` (0.2.10, unpublished).
Date: 2026-10-22.

## The executive finding

An interrogation of the 0.2.10 tree on 2026-10-22 found thirteen defects and
one optimisation. One of them broke the repository's own first rule — *nothing
is deleted on the strength of a timestamp; every candidate must first prove
exclusive ownership of its physical bytes, and that proof must still hold at
the moment of deletion* — and it did so silently: the tool deleted a
package's checked-in files and reported the space it had reclaimed.

Every defect was reproduced before it was fixed. Every fix is pinned by a test
that fails before it and passes after it. Nothing was carried forward on the
strength of a reading.

## What changed

| # | Severity | Defect | Fix | Regression test |
|---|---|---|---|---|
| D1 | critical | An output root *inside* another package's directory was classified `PrivateBounded` and cleaned. | `workspace.rs` refuses a covering root nested inside a package directory; `cleanup.rs` re-proves it at the spawn. | `workspace::tests::an_output_root_nested_inside_a_package_directory_is_never_private`, `cleanup::tests::an_output_root_inside_a_package_directory_is_never_cleaned` |
| D2 | high | A root `--profile` silently discarded a `clean --package`; the run exited 0 and did a *different* destructive action. | The pair is checked before either is merged; the cross-level combination is a usage error. | `cli::tests::a_selector_at_the_root_and_the_other_on_clean_is_a_usage_error` |
| D3 | medium | The interactive empty-result row was not truncated, so the erase left the title standing above the prompt. | The row is truncated like every other frame line. | `interactive::tests::the_empty_result_row_is_truncated_to_the_frame_width` |
| D4 | medium | The ownership column vanished at 27–30 columns, and a `Shared` unit rendered identically to a cleanable one. | The reclaimable-size column yields first; a one-cell column carries the class's first letter. | `interactive::tests::the_ownership_column_survives_every_legal_terminal_width` |
| D5 | medium | A staging-cleanup failure claimed "the executable was replaced" regardless of the receipt; a rolled-back transaction *restored* the old binary. | The message follows the receipt's own disposition. | `update::tests::the_staging_cleanup_message_follows_the_receipt_disposition` |
| D6 | medium | `UpdateError::Receipt` dropped the retained backup's path and the rollback failure — the only pointer to the only copy of the user's previous binary. | The variant carries both, and the `Display` reports them. | `update::tests::a_receipt_error_carries_the_retained_backup_and_the_rollback_failure` |
| D7 | medium | A scope-conflict message named `--known` as given at the root when it was given only on `clean`. | The message names the two flags that actually collide. | `cli::tests::a_scope_conflict_names_the_flags_that_actually_collide` |
| D8 | low | The coloured all-toggle row was rebuilt from the raw label, two cells wider than the frame. | It is built from the same truncated pieces as the plain row. | `interactive::tests::the_coloured_all_row_fits_the_frame` |
| D9 | low | The staging directory was `0775` under a common umask, while the contract calls it private. | It is created owner-only, at creation. | `update::tests::the_staging_directory_is_private_to_its_owner` |
| D10 | low | The hidden `scan --full` alias was advertised in `scan --help`'s usage line and in the completions. | The alias is an alias of `--all`, not an argument of its own. | `cli::tests::hidden_full_alias_is_not_advertised_in_scan_help` |
| D11 | low | The progress tick-semantics comment, and the deep dive that repeated it, were false. | Both state the difference accurately. No behaviour changed. | — (documentation) |
| D12 | medium | The authoritative module-line census was wrong. | Re-derived from `src/` with a brace-tracking parser, after this corrective's own changes. | — (documentation) |
| D13 | optimisation | The marker memo recorded only positives, so every source directory was read twice. | Both answers are recorded; the memo is per-walk. | `traverse::tests::the_marker_memo_records_both_answers` |

## The requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| A cleanup target must prove exclusive ownership of its physical bytes | D1: the structural check is now two-sided, and the per-spawn re-proof matches it. The offending fixture deletes `lib1/assets` before the fix and refuses it after. |
| Cleanup is fail-closed | D1's refusal is `Uncertain`, which is never a cleanup target. No unit becomes cleanable by the fix. |
| The operator's selector decides what is cleaned | D2: the discarded flag is now a usage error (exit 2), not a silent preference. |
| The interactive surface leaves the terminal as it found it | D3, D4, D8: the frame's lines are all truncated to the terminal width, and the ownership column cannot vanish. |
| A failure report says what happened | D5, D6, D7: the three messages name what actually happened, and the recovery pointer is carried. |
| Staged bytes are private to the operator | D9: the staging directory is created `0700`. |
| A hidden alias is hidden | D10: the alias is not a group member, so the `ArgGroup` cannot render it. |
| The architecture corpus is true | D11, D12: the comment and the census were re-derived, not copied forward. |
| A walk does not read the same directory twice | D13: 16,025 → 12,022 `getdents64` on a 2,000-directory source tree. |

## The mutation evidence

Each fix was reverted, and the corresponding test was observed to fail:

| Fix | Reverted | Test observed failing |
|---|---|---|
| D1 | `nested_package_overlap` forced to `false` | `an_output_root_nested_inside_a_package_directory_is_never_private` (left `PrivateBounded`), `an_output_root_inside_a_package_directory_is_never_cleaned` (a `cargo clean` ran) |
| D3, D4, D8 | the truncation, the column-yield and the label-truncation reverted | all three interactive tests failed |
| D2, D7 | `merge_overrides`' pair check and the message reverted | the two cli tests failed |
| D13 | the memo reverted to positives-only | `the_marker_memo_records_both_answers` failed |

D5, D6, D9, D10, D11 and D12 are checked by construction — the mapping from
disposition to claim is a single function, the receipt variant's fields are
part of its type, the staging mode is asserted against the filesystem, and
the alias is asserted against the rendered help.

## The census re-derivation

`architecture/overview.md` §3 is the source of truth for the module line
counts, and it was wrong. It was re-derived with a parser that tracks braces
and excludes every `#[cfg(test)]` scope — including the *nested* test modules,
which is why `output.rs` has more production lines than a naive
"lines before the first `#[cfg(test)]`" reading reports.

The method was validated in both directions before it was trusted:

- against the **pre-corrective** tree it reproduces the published figures
  exactly (38,642 total / 17,694 production / 534 declared tests), and it
  reproduces every *unchanged* module's published row exactly — so the two
  modules the published table got wrong are the two C043 touched;
- against the **corrected** tree it gives 39,334 / 17,957 / 546.

The per-platform test counts were derived with a brace-tracking parser
evaluated against `rustc --print cfg --target <triple>`, and cross-checked
against `cargo test --lib -- --list` plus `cargo test --bin cargo-cleanme
-- --list` on the host: **544** on Linux, which is the parser's figure
exactly. The macOS (**541**) and Windows (**500**) figures are source-derived,
as the previous ones were.

The C043 closure record's figures were **not** edited: a closure record is
history, and the correction is recorded forward, here.

## The citation re-derivation

The corrective changed six source files, so every citation into them past the
change point moved. The citations were re-derived with a line-level diff
between the old and the new file, and **every** remap was verified by
comparing the text at the old line with the text at the new line: a remap
whose text did not match was reverted rather than applied. That check caught
bare citations that refer to a *different* file than the document's default —
`scripts/release-benchmark.py` and `examples/traversal-qualification.rs` in
the traverse deep dive — which a diff-based remap cannot tell apart.

Eighteen citations landed inside an *edited* region, where a diff cannot say
which line of the replacement the sentence meant. Each was re-derived by hand
against the source. Two of them were **already wrong before this corrective**:
`architecture/02-cli.md` pointed the `ArgGroup` at the match arm instead of
the group, and at the root's `long_about` for the sentence that describes the
`long_about`. Both are corrected here.

The per-test line-number columns in the deep dives were re-derived the same
way: a row's number is the line of the named `fn`, verified in the old file
before it was mapped and in the new file after.

## The verification actually run

```
cargo fmt --all -- --check                                     # clean
cargo clippy --all-targets --all-features -- -D warnings       # clean
cargo test --all-targets --all-features                        # 625 passed, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
python3 scripts/check-release-contract.py
```

The inline test census moved from 534 declared / 532 on Linux / 529 on macOS
/ 490 on Windows to **546 / 544 / 541 / 500**.

**Not run here:** the MSRV lane (toolchain 1.89) and the hosted
ubuntu/macos/windows lanes. The changes touch path comparison, file modes and
terminal rendering, so the hosted lanes are the ones that can disagree.

## Known limitations

- **The census has no gate.** D12 corrected the numbers; the mechanism that
  let them drift — a hand-maintained figure with nothing over it — is
  unchanged, and the drift is expected to recur until a gate exists. This is
  the same mechanism as C036's D29, and it is deliberately *not* fixed here:
  a gate is a feature, and this corrective authorises no feature.
- **D11 is a documentation fix, not a behavioural one.** The in-memory
  renderer still installs no ticker, so it still exercises a different
  mechanism from production. Installing one would put a background thread in
  the test frame.
- **D1's boundary is the owning workspace's own root.** A target directory
  pointed at the *owning* package's own subdirectory is still governed by the
  name-based net (`src`, `tests`, `benches`, `examples`), as before. That
  shape is a user configuring their own package, and it is out of this
  corrective's scope.

## The disposition

Implemented, ladder-green, **not published** — so this corrective stays
**open**. Publication is owner-gated; nothing here requests it.
