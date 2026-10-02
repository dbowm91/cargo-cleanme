# Artifact Discovery and Cleanup C001 Closure

Plan: `plans/implementation/artifact-discovery-cleanup/c001-public-contract-msrv-traversal-reconciliation.md`

Disposition: **closed**

Implementation commits: `d04b9bb` through `59de87b` on `main`
(`d04b9bb` implements work packages A–E; `59de87b` corrects nested/VCS
source exclusion found by the first hosted Windows run).

## Requirement-to-evidence matrix (C001-F1 through C001-F6)

| Finding | Regression evidence | Result |
|---|---|---|
| C001-F1 — Cargo external-subcommand argv advertised but not normalized (`Cli::parse()` directly; no `cargo cleanme` integration proof) | `cli::tests::direct_argv_is_unchanged`, `cargo_external_argv_strips_exactly_one_documented_token`, `later_literal_cleanme_values_are_preserved`, `help_and_version_forms_normalize`, `direct_and_external_forms_parse_equivalently`, `normalized_config_init_force_parses`; integration `tests/cli_contract.rs`: `cargo_external_subcommand_help_matches_direct_help`, `cargo_external_config_init_help_matches_direct` (stages the built binary in a temp `PATH` and runs `cargo cleanme --help` with an isolated `CARGO_HOME`) | Pass |
| C001-F2 — Config template split (`config.example.toml` + hard-coded `CONFIG_TEMPLATE`; no byte guard; no `--force`) | `config.toml` is now the single `include_str!` source (`src/config.rs`); `config.example.toml` deleted; `config::tests::checked_in_template_parses_and_matches_defaults`, `config_template_matches_repository_file`, `init_bytes_equal_checked_in_template`, `init_refuses_overwrite` | Pass |
| C001-F3 — README `clean ./projects` conflicts with absolute-root rejection | `cli::tests::absolute_root_is_stable`, `relative_root_becomes_absolute`; `cleanup::tests::relative_cli_root_reaches_cleanup_as_absolute_scope`, `nonexistent_relative_root_fails_as_invalid_root`, `symlink_root_remains_rejected_after_normalization`, `dotdot_segments_resolve_lexically_without_escaping_caller_intent`; manual binary runs prove `scan ./projects` and `clean ./projects` execute from the containing directory (exit 0, no absolute-root error) | Pass |
| C001-F4 — Declared Rust 1.89 MSRV not qualified (stable-only CI) | `.github/workflows/ci.yml` `msrv` job (Ubuntu, `cargo +1.89 check --locked --all-targets`, `cargo +1.89 test --locked --all-targets`); hosted MSRV job green on the final implementation commit | Pass |
| C001-F5 — Traversal split (dua-core discovery vs sequential walkdir analysis; serial candidate sizing) | New `src/traverse.rs` ownership boundary (no dua-core types leak to domain APIs); `analysis::{analyze,analyze_many}` and `cleanup::measure_target` migrated to it; `discovery` uses `traverse::worker_threads()`; `walkdir` removed from `[dependencies]`; `traverse::tests::{worker_pool_is_bounded,single_and_batched_target_measures_agree,candidate_count_does_not_grow_worker_pool,synthetic_wide_and_deep_trees_count_entries,source_scan_short_circuits_recent_files}` plus `analysis::analysis_fixtures::{batched_and_single_analysis_agree,analysis_bytes_match_traverse_measure,nested_vcs_boundary_excludes_nested_repo_activity,symlink_entries_are_not_followed,disappearing_target_is_uncertain}` and (Linux) `non_utf8_paths_are_measured` | Pass |
| C001-F6 — Stale planning state (roadmap audit text; M004 labeled active) | `plans/subsystems/artifact-discovery-cleanup-roadmap.md` current-state rewritten to the post-C001 implementation, dependency graph moved C001 `ready` → `closing` → `closed`, §9A status updated; `plans/registry.md` C001 tracked `ready` → `active` → `closing` → `closed`; README corrected (see below); historical M001–M004 closures untouched (additive references only) | Pass |

## Direct vs Cargo-external invocation evidence

- Unit: direct `["cargo-cleanme", …]` vectors are unchanged; external vectors with exactly one `cleanme` token in position 1 strip once; later literal `cleanme` path/values preserved; help/version forms normalize; parsed `Cli` debug forms agree for no-arg/scan/config/clean pairs.
- Integration (`cargo test --test cli_contract`, 2 passed locally and in hosted CI): the built binary is staged as `cargo-cleanme` in a temp `PATH` with an isolated `CARGO_HOME`; `cargo cleanme --help` stdout is byte-equal to direct `cargo-cleanme --help`, and `cargo cleanme config init --help` advertises `--force`.
- `main` now parses via `Cli::parse_normalized()`; normalization lives in `cli::normalize_cargo_argv` and is tested from an explicit iterator, not hidden in `main`.

## Config template/init byte-equivalence evidence

- `config::CONFIG_TEMPLATE == include_str!("../config.toml")`; `config_template_matches_repository_file` asserts the embedded string equals the on-disk `config.toml`.
- `init_bytes_equal_checked_in_template`: `init(path, false)` bytes equal `CONFIG_TEMPLATE`, and the written file loads with `recency_seconds == 300`.
- Manual: `config init` writes the file, `diff` against the repository `config.toml` reports `BYTE-EQUAL`; second `init` without `--force` refuses with “rerun with --force”.

## Config force-write safety evidence

- Default `init(path, false)` retains create-new semantics and refuses overwrite.
- `init(path, true)` writes a `config.toml.tmp.<pid>.<attempt>` sibling with `create_new`, `write_all` + `sync_all`, then `rename` over the destination; the destination is never truncated before replacement is fully written.
- Tests: `init_force_replaces_existing_config` (60 → 300), `malformed_config_requires_force_for_replacement` (unparseable replaced only with force), `failed_force_leaves_previous_file_intact` (directory destination fails without removing the directory and leaves no `*.tmp.*` sibling; refused non-force init preserves bytes).
- Manual: `config init --force` after refusal rewrites the file and `diff` reports `FORCE-OK`.

## Relative-root CLI evidence

- `cleanup::clean_with` keeps its absolute-root invariant (`clean_requires_an_absolute_root` still rejects `"."` with no runner calls).
- `cli::absolutize_root` resolves relative CLI roots lexically against the invocation directory (absolute roots unchanged; no canonicalization, so symlink/existence validation still applies downstream in `policy::resolve`/cleanup).
- `..` handling is a lexical pop that stays at the filesystem root and otherwise resolves against the caller-selected stack (`/a/b + c/../d → /a/b/d`; `/a/b + ../outside → /a/outside`).
- Manual binary proof (temp project, `cd` into parent): `scan ./projects` and `clean ./projects` both exit 0 with no absolute-root error; `clean ./projects` reports `dry-run: …; no cleanup executed` as documented. README examples therefore execute as written.

## Hosted Rust 1.89 run

- Final implementation commit `59de87b`: [run 37059942430](https://github.com/dbowm91/cargo-cleanme/actions/runs/37059942430), `msrv` job (Ubuntu, Rust 1.89, locked check + locked test) — **success**.
- Local MSRV qualification on the same tree: `cargo +1.89 check --locked --all-targets` passed; `cargo +1.89 test --locked --all-targets` passed (57 lib + 2 CLI-contract + 1 end-to-end).

## Final stable Linux/macOS/Windows run

- Same hosted run [37059942430](https://github.com/dbowm91/cargo-cleanme/actions/runs/37059942430): `checks (ubuntu-latest)`, `checks (macos-latest)`, `checks (windows-latest)` — **all success** (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`).
- Note: the preceding hosted run for `d04b9bb` ([37059587576](https://github.com/dbowm91/cargo-cleanme/actions/runs/37059587576)) failed only on Windows in the new nested-VCS regression test. The failure was a real boundary-mtime leak (pruned boundary directories still contributed their own mtime), fixed in `59de87b` by excluding pruned boundary entries from source evidence. The final run above is green on all three platforms. Ubuntu/macOS were green on both runs; MSRV was green on both runs.

## Traversal dependency/ownership summary

- New `src/traverse.rs` is the single traversal owner for discovery, source activity, target analysis/sizing, and post-clean measurement. It exposes only paths/sizes/timestamps (`worker_threads`, `TargetStats`, `measure_single_target`, `measure_many_targets`, `source_activity`, `SourceActivity`); `dua-core` types never appear in domain/CLI signatures.
- `discovery::scan` uses `traverse::worker_threads()` (≤8) with `dua-core` `ParentFirst` walks; symlink directories are never followed; errors are diagnostics.
- `analysis::analyze` (single) preserves eligibility semantics via `traverse::measure_single_target`; new `analysis::analyze_many` keeps source activity candidate-sequential (cheap early rejection) and sizes survivors together with `traverse::measure_many_targets` over one shared `walk_roots` pool, so candidate count grows work, not pools. `batched_and_single_analysis_agree` proves equivalence.
- `cleanup::measure_target` (post-clean) uses `traverse::measure_single_target`, giving pre/post size-metric equality by construction (`analysis_bytes_match_traverse_measure`).
- `walkdir` has no remaining production or test use (`grep -rn walkdir src tests` empty); removed from `[dependencies]` in `Cargo.toml` (lockfile pruned accordingly). No dev-dependency retention was needed.
- Size semantics preserved: per-file `symlink_metadata` + `filesize::file_real_size_fast`, entry counting excludes the root, any walk/metadata/sizing/overflow failure marks the candidate uncertain (never eligible).
- Cancellation/contention preserved: dropping any walk stops/joins bounded workers; no partial scan is presented as complete; destructive Cargo executions remain strictly sequential with per-project revalidation and ownership checks immediately before each operation.

## Synthetic and real-tree performance evidence

- Synthetic (release, local, `measure_*` directly):
  - wide: 512 files / 2,097,152 bytes / 512 entries in ~1.94 ms, `uncertain=false`;
  - deep: 15-level chain + leaf / 16 entries in ~0.80 ms;
  - batched 24 targets (192 entries / 786,432 bytes) with 8 workers in ~1.34 ms vs sequential singles in ~5.55 ms (~4× from the shared pool; worker count independent of candidate count at 32 roots).
  - Operation counters are asserted in `synthetic_wide_and_deep_trees_count_entries` and `candidate_count_does_not_grow_worker_pool`.
- Real tree (`/Users/davidbowman/projects`, release binary):
  - C001: 27 inactive candidates, 170.44 GiB estimated allocated, ~7.1 s wall (`scan`, explicit scope).
  - M003 baseline (recorded in its closure): 28 candidates, ~138.85 GiB, ~19.94 s on the same parent tree at an earlier date.
  - The tree is live (219 GiB now; file set and activity window changed), so bytes/counts are not directly comparable; the relevant C001 claim is no material regression with a faster elapsed time on a larger tree, consistent with bounded parallel sizing. No timing threshold is canonical.
- Before/after engine comparison is by construction plus equivalence tests rather than a retained sequential fallback: the sequential `walkdir` implementation was replaced wholesale after proving byte/entry equivalence (`single_and_batched_target_measures_agree`, `analysis_bytes_match_traverse_measure`), and `walkdir` was then deleted to prevent semantic drift.

## Confirmation M004 safety tests remain green

- Full suite on the final tree: `cargo test --all-targets --all-features` — 57 lib + 2 CLI-contract + 1 end-to-end, all pass; MSRV locked run identical.
- Cleanup/ownership subset (13 tests): absolute-root rejection, relative-to-absolute scope, nonexistent/symlink/`..` handling, missing marker, shared-workspace/redirected-target, build-dir config/environment, recent-source/missing-target revalidation, dry-run-only-Cargo-args, dry-run non-mutation, measured size decrease with fake Cargo (including repeated metadata preflight).
- Policy/activity subset: CLI-root precedence, configured-root precedence, activity cutoff/future-time, huge-window rejection, recent short-circuit, empty-target exclusion, nested-VCS exclusion, symlink non-following, disappearing-target uncertainty.

## Static production deletion review

- `grep -rn "remove_dir_all" src` — no hits.
- `grep -rn "remove_file" src` — only `cleanup.rs` fake-runner/test-fixture removals and `config.rs` temp-sibling cleanup on failed `--force` (`let _ = fs::remove_file(&temp)`); no production recursive deletion primitive.
- Production cleanup path (`SystemRunner`) invokes only `cargo metadata` / `cargo clean --dry-run --verbose --offline --locked --manifest-path … --target-dir …` via `Command::new("cargo")`; execution requires explicit `--yes` and repeats revalidation + ownership preflight.

## Documentation/registry reconciliation

- `README.md`: direct ≡ `cargo cleanme` contract, `config init --force` + single-template note, relative `clean ROOT` absolutization, bounded `dua-core` pool, sequential Cargo-delegated cleanup with no direct deletion.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: §3 current-state rewritten to the post-C001 implementation; §5 graph C001 `ready` → `closed`; §9A status `ready` → `closed` with implementation summary; M005 remains design-proposed/implementation-blocked (see below).
- `plans/registry.md`: C001 `ready` → `closed`; subsystem current milestone advanced past C001; M005 row updated (C001 baseline satisfied; only the ownership/ADR decision blocks implementation).
- `plans/implementation/…/c001-….md`: status `ready for handoff` → `closed`.
- Historical M001–M004 plans and closures were not rewritten; this record links backward to them and they link forward only via the registry/roadmap.

## Unresolved findings and disposition

- No known correctness or security findings remain within the C001 scope. The one hosted failure observed during C001 (`d04b9bb` Windows nested-VCS test) was investigated, fixed (`59de87b`), and re-qualified green on all platforms; it is recorded above, not left open.
- M005 redirected/shared-output implementation is **not unblocked for production work**: C001 closure satisfies the corrected-baseline half of its blocker, but the shared/redirected-output ownership model and any required ADR decision are still outstanding. M005 design research may continue against the post-C001 tree; an M005 implementation handoff must still wait for the ownership decision. Registry and roadmap reflect this split status.
- Future timestamps, concurrent filesystem races, and permission/disappearing-entry uncertainty retain their conservative M003/M004 dispositions (uncertain ⇒ ineligible/skipped with diagnostics).

## Verification run (final tree, before closure commit)

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (57 + 2 + 1).
- `cargo check --locked` — passed.
- `cargo +1.89 check --locked --all-targets` — passed.
- `cargo +1.89 test --locked --all-targets` — passed (57 + 2 + 1).
- `git diff --check` — passed.
- Focused: `cargo test argv` (direct/external/help/later-literal/equivalence), `cargo test config` (13 incl. template/force), `cargo test traverse` (6 incl. bounded pool/agreement/worker-bound/synthetic), `cargo test activity`/`sizing` (cutoff/nested/sizing), `cargo test ownership` (4), `cargo test clean` (15) — all passed.
- Hosted: run 37059942430 (implementation commit `59de87b`) — stable Ubuntu/macOS/Windows + MSRV 1.89 all success.

## Dependency disposition

C001 is closed. M005 design research may proceed against this corrected baseline; M005 production implementation remains blocked until the shared/redirected-output ownership model and any required ADR are decided and a new implementation handoff is registered. No other follow-on work was found blocked on C001.
