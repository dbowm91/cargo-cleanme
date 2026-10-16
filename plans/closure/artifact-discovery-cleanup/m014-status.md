# M014 Status — Working-Directory-Scoped Default and Interactive Selection

Plan: `plans/implementation/artifact-discovery-cleanup/m014-cwd-scoped-default-and-interactive-selection.md`
(status flipped proposed → closed by this record).

Disposition: **closed.** Implemented, hosted-qualified on its merge head,
and carried by published release **v0.2.8** (tag `v0.2.8` at `0a5a6e6`,
GitHub release 2026-10-15, `CHANGELOG.md` `[0.2.8]` documents all three
operator changes). No release is authorized by this record; the release
already happened.

Implementation (PR #13, merged 2026-10-15T04:09:40Z, merge commit
`4bff84e` onto `main`):

- `95e4a40` feat(cli): bare invocation and `clean --known` anchor selection
  to the working directory (`CleanupSelection::WorkingDirectory`, resolved
  against `$PWD`; `clean ROOT` / `clean --full` stay `All`).
- `e9739ac` feat(interactive): `-i` selection TUI over the engine's own
  candidate listing (`src/interactive.rs`), crossterm behind a narrow
  `Terminal` seam, RAII raw-mode guard, no alternate screen.
- `37e86cf` fix(ci): new interactive tests unix-gated, mixed-ownership
  fixture canonicalized for symlinked macOS TMPDIR (a Windows/macOS lane
  failure fixed pre-merge).
- `1574fa2` fix(test): redirected target-dir written with `{:?}` so
  Windows backslashes stay valid TOML (a Windows-lane failure fixed
  pre-merge).
- `cc89def` + `57c6af3` docs: CWD-anchored default, `scan --all`, `-i`;
  cleanup-safety skill mode table corrected to the post-M012A contract.

## Requirement-to-evidence map (plan §Required evidence)

| # | Requirement | Evidence |
|---|---|---|
| 1 | Shared stays shared at both anchor widths, shipped binary | `mixed_ownership_fixture` (`src/cleanup.rs:4824`): alpha/beta private, gamma redirects into alpha's target via real `cargo metadata`; `working_directory_anchor_selects_without_narrowing_the_ownership_universe` (`tests/cli_contract.rs:1475`) proves both directions — elsewhere: `ownership == "private"`, `reason_code == "skipped_outside_selection"`, `discovered_manifests == 1`, `resolved_workspaces == 1`; inside: `simulated == 1` |
| 2 | Excluded units carry a typed skip, never silence | Same subprocess test (`reason_code == "skipped_outside_selection"`) + `outside_selection_is_reported_rather_than_silently_dropped` (`src/cleanup.rs:4412`) pinning the code distinct from every ownership/safety code |
| 3 | Bare `scan` is a usage error; `scan -a` / `--full` reach Full identically | `Cli::try_parse_from` + `Invocation::UsageError("scan requires a ROOT, --known, or --all")` (`src/cli.rs:278,339,345`); `scan_scope_selection_is_full_explicit_or_maintenance` (`src/cli.rs:663`) and `scan_scope_conflicts_are_rejected_before_any_traversal` (`tests/cli_contract.rs:661`, exit 2, empty stdout) |
| 4 | Simulate and Execute receive the identical selected unit set | `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` (`:1638`: operation clean, mode execute, cleaned 1, artifact gone, one clean call) paired with `bare_dry_run_simulates_with_zero_cargo_clean_processes` (`:1675`: simulated 1, zero clean calls, artifact kept, metadata ≥ 1); `the_candidate_listing_is_the_engines_own_unit_set` (`src/cleanup.rs:4875`) proves the `-i` listing equals the engine's unit set including the non-private unit |
| 5 | TUI state-machine tests with a fake terminal | 40 `interactive::tests` cases (`src/interactive.rs:801+`): mode routing, vim keys, toggle, all-row, INS filter persistence, exit arming, empty-list render, viewport/truncation arithmetic |
| 6 | Raw mode restored on abort/panic | `the_driver_restores_the_terminal_on_the_commit_path`, `..._on_the_quit_path`, `a_read_failure_restores_the_terminal_and_propagates`, `the_guard_restores_when_dropped_during_unwind` (fake terminal records `raw_before_panic`, `catch_unwind` proves restore). No real-pty test exists — recorded below as a limitation, not a gap in the guarantee: the guard is constructed before the first key read, so every exit path is covered by construction |

## Verification actually run

| Command or run | Result |
|---|---|
| PR #13 final head `1574fa2`, hosted CI run `38679990768` | **success — all 9 jobs**: checks ubuntu/macos/windows, msrv, generated-docs, benchmark, installers ×3 |
| Release drift guard `38679990585` on the same head | success |
| Cargo selector qualification `38679990729` on the same head | success (Supported matrix + Exploratory current stable) |
| Pre-merge reds, fixed before merge | CI failed on `e9739ac`/`57c6af3` heads (unix-only test helper, mixed-ownership TMPDIR); fixed in `37e86cf`/`1574fa2`; PR checks green before merge |
| Publication | tag `v0.2.8` at `0a5a6e6`; GitHub release `v0.2.8` published 2026-10-15T04:52:28Z; `CHANGELOG.md [0.2.8]` documents `-i`, the CWD anchor, and `scan --all` |
| Current-tree re-verification (this record, 2026-10-16) | `cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; lib `interactive::` 40 passed; lib `cli::` 32 passed; 7 M014 `cli_contract` subprocess tests passed (`working_directory_anchor…`, `interactive_without_a_terminal…`, `interactive_refuses_the_machine_output_formats`, `interactive_dry_run_still_requires_a_terminal`, `bare_invocation_executes…`, `bare_dry_run_simulates…`, `scan_scope_conflicts…`); `check-fixture-portability.py` self-test + 37 clean; `check-doc-citations.py` self-test + 17 clean; `generate-docs --check` 13/13; `git diff --check` clean |

The current-tree re-run matters: M015/M016 later edited `cleanup.rs`
around the selection code, and the M014-owned tests still pass unmodified.

## Known limitations

- No real-pty test: restoration is proved with a fake terminal plus
  `catch_unwind`, not against an actual terminal device. The structural
  guarantee (guard built before the first read, `Drop` + explicit restore,
  idempotent) is reviewed, not executed, for a true pty abort.
- `interactive_without_a_terminal…` exercises the no-TTY refusal through the
  test harness's own redirected streams, not through a real terminal.
- Unix-gated subprocess coverage: the POSIX-stub fixtures do not execute on
  the Windows lane; Windows covers the equivalent strict-explicit-root path
  plus the TOML-escape regression (`1574fa2`) that its lane found.
- No `r028` distribution closure record was written for the v0.2.8 release;
  publication is attested by tag + GitHub release + CHANGELOG, not by a
  release-qualification receipt. Recorded as distribution-line debt in the
  registry; M014 does not depend on it.

## Unresolved findings by severity

- **None.** The two lane-caught defects (unix-gating, TOML escapes) were
  found and fixed pre-merge on PR #13 itself. No M014-owned defect is open.
- Per the corrective-pass rule, any future defect gets a new plan, not an
  edit to this record.
