# Artifact Discovery and Cleanup M005A Closure

Plan: `plans/implementation/artifact-discovery-cleanup/005a-workspace-output-resolution-fail-fast-progress.md`

Disposition: **closed**

Implementation commits: `0dbe619` through `91b3e26` on `main`
(`0dbe619` implements work packages A–F: domain/capability, manifest
discovery + workspace resolver, fail-fast pipeline + deduped sizing, progress
observer, indicatif renderer + `--no-progress`, deterministic report + docs;
`91b3e26` corrects the Windows redirected-target fixture TOML escaping found
by the first hosted Windows run.)

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Discover project even with no direct local target | `discovery::tests::manifest_discovery_finds_project_without_target`; real redirected fixture `real_manifest_without_local_target_but_redirected_output` (valid package, no local `target/`, `.cargo/config.toml` `target-dir` → external, resolved target equals canonical external) | Pass |
| Cargo itself resolves workspace/output configuration; no manual `.cargo/config.toml` merge | `workspace::SystemCargoRunner` uses only `cargo locate-project --workspace --manifest-path` + `cargo metadata --offline --locked --no-deps --format-version 1`; `grep -rn "config.toml" src/workspace.rs` shows no manual merge (only test fixture writes); real multi-member + redirected tests use system Cargo | Pass |
| Multi-member workspaces represented once | `repeated_members_invoke_metadata_once` (2 manifests → 1 metadata, `deduped_workspace_hits==1`); `real_multi_member_workspace_maps_to_one` (2 members → 1 workspace, 1 metadata) | Pass |
| Target/build physical overlap deduplicated | `output_grouping_equal_nested_shared` (equal → 1 group, 1 covering); `nested_target_build_deduped` (build under target → 1 covering); `target_nested_under_build_deduped` (target under build → 1 covering); `distinct_sibling_target_build_measured_once_per_root` (siblings → 2 disjoint groups, each measured once, total entries 2) | Pass |
| Shared output recognized and inventory-only; no new destructive path | `output_grouping_equal_nested_shared` shared case (2 owners → `Shared`); `cross_overlap_target_build_is_shared`; `real_manifest_without_local_target_but_redirected_output` → `ExternalUnproven`; `cleanup.rs` unchanged for M004 conventional path; `grep -rn "remove_dir_all" src` no hits; production `SystemRunner` still only `metadata` / `clean --dry-run --verbose` for conventional private | Pass |
| Fail-fast counters prove expensive work skipped | `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned` (ignored → 0 manifests, pruned>0); `missing_and_empty_output_skips_without_source_or_deep_work` (missing/empty → `groups_measured==0`); `recent_member_source_skips_deep_sizing` (recent source → `groups_measured==0`, `active_skipped>0`); `recent_shared_output_protects_group`; `manifest_discovery_prunes_*` (target children, cargo-home registry) | Pass |
| Attended-terminal inline status, indeterminate discovery, determinate analysis | `progress::tests::discovery_begins_indeterminate_then_analysis_determinate` (discovery total 0, analysis sets total 7); `main.rs` creates `IndicatifRenderer` before traversal, `phase(Discovery)` before `discover_manifests`, `set_determinate_total(groups.len())` before analysis | Pass |
| Transient display capped at five sized groups and clears before final output | `renderer_caps_rows_at_five_largest_first` (10 inputs → 5, largest-first); `sizes_render_in_top_rows_and_clear_before_report` (`finish_and_clear` twice, no panic); `main.rs` calls `renderer.finish_and_clear()` before `println!(report)` | Pass |
| Non-TTY output deterministic/plain; `--no-progress` forces hidden | `should_show_progress_respects_flag_and_dumb_term`; `noop_observer_is_free_and_hidden_renderer_emits_nothing` (hidden refresh 0 despite 1000 events); `cli::tests::no_progress_flag_parses_globally`; release runs piped (non-TTY) show no control noise, same semantic results with/without flag (see perf below) | Pass |
| Final scan prints every group plus non-double-counted total, deterministic order | `report::tests::groups_render_every_group_with_ownership_and_deduped_total` (3 groups, `/a` before `/z` at equal size, all ownership labels, `inventory estimate`, no `recovered`); `groups_total_does_not_double_count_equal_roots`; `workspace::analyze_groups` sorts size-desc + path tie-break | Pass |
| Existing conventional scan semantics remain green | `real_conventional_workspace_is_private_bounded` (conventional → `PrivateBounded`); `discovery::tests::discovers_real_target_and_prunes_target`, `discovery_reaches_unignored_*`, `discovery_rejects_symlink_target` (M004 wrapper preserved); full suite 94 lib + 2 CLI + 1 e2e green; MSRV green | Pass |
| No new destructive cleanup path | `cleanup::clean` still M004-bounded (single-member, conventional `target/`, marker, no build-dir); M005A adds no `cargo clean` invocation for redirected/shared; `cargo test clean` (15 tests incl. dry-run non-mutation, ownership rejections) green | Pass |

## Cargo runtime fixtures

- Local `cargo 1.89.0` metadata: no `build_directory` key (pre-1.91 path).
- Local `cargo +1.90.0` metadata: no `build_directory` key.
- Local `cargo +1.95.0` metadata: `build_directory == target_directory` (equal path).
- Unit fixtures:
  - `capability_without_build_field_is_unavailable` (no field, no env → `Unavailable`);
  - `capability_equal_and_distinct` (equal vs distinct);
  - `capability_env_only_is_unknown` (no field + `CARGO_BUILD_BUILD_DIR` → `Unknown`);
  - `capability_malformed_is_unknown` (field present but null → `Unknown`);
  - `malformed_and_failure_isolation` (bad JSON → diagnostic, no workspace).
- Real Cargo integration (system runner, `--offline --locked --no-deps`):
  - multi-member (2 members → 1 workspace);
  - redirected external `target-dir` via `.cargo/config.toml` → `ExternalUnproven`;
  - conventional local `target/` → `PrivateBounded`.

## Output-graph fixtures

- `target == build` → 1 group, 1 covering, `PrivateBounded`.
- Distinct siblings `target/` + `build/` → 2 groups, each 1 covering, both `PrivateBounded`, measured separately, total = sum (no double-count within group).
- Build nested under target → 1 covering.
- Target nested under build → 1 covering.
- Two workspaces identical target → `Shared`.
- Cross-overlap (ws1 target == ws2 build) → `Shared`.
- Missing output → 0 groups, `missing_output_skipped`, no source/deep work.
- Empty output → 1 group pre-sizing, then skipped (`empty_no_output_skipped`), `groups_measured==0`.
- Symlink output → 0 physical nodes, `Uncertain` owner, no groups.
- Non-UTF-8 (Linux-only) → 1 group, 1 entry measured.

## Fail-fast operation counters

Representative explicit-root run (`/tmp/perf-proj`, 1 manifest, 100 KiB artifact, backdated quiet):

```text
scan counters: visited=5 pruned=1 manifests=1 workspaces=1 locate=1 metadata=1
failures=0 empty_skipped=0 active_skipped=0 groups_measured=1 bytes=102400
reportable=1 elapsed=0.04s
```

- Ignored/pruned trees: 0 manifests → 0 Cargo subprocesses (`ignored_subtree_*`).
- Empty/missing: 0 source walks, 0 deep walks (`missing_and_empty_*`).
- Recent source: 0 deep walks (`recent_member_*`, `recent_shared_*`).
- N manifests in one workspace: 1 metadata (`repeated_members_*`, real multi-member).
- Progress disabled vs enabled: identical eligible sets/bytes (`progress_disabled_vs_enabled_same_results`; release runs below).
- Renderer refresh: hidden 0 refreshes for 10k entries (`renderer_refresh_bounded_*`); visible throttled to 10 Hz via `stderr_with_hz(10)` + 100 ms `maybe_draw` gate.

## Progress renderer tests

- `noop_observer_is_free_and_hidden_renderer_emits_nothing`
- `renderer_caps_rows_at_five_largest_first`
- `test_observer_records_semantic_events`
- `should_show_progress_respects_flag_and_dumb_term`
- `renderer_refresh_bounded_independently_of_entry_count`
- `discovery_begins_indeterminate_then_analysis_determinate`
- `sizes_render_in_top_rows_and_clear_before_report`
- `renderer_error_falls_back_without_failing_scan`
- `cli::tests::no_progress_flag_parses_globally`

Renderer uses `indicatif 0.18.6` (declares Rust 1.85, fits MSRV 1.89); no ratatui/raw-mode/alternate-screen. `ProgressDrawTarget::stderr_with_hz(10)`, `enable_steady_tick(100 ms)`, `MultiProgress`-free single bar + 5 rows, `finish_and_clear` before stdout.

## Representative before/after timing

- C001 baseline (recorded in its closure, `/Users/davidbowman/projects`, release `scan`, explicit scope): 27 inactive candidates, 170.44 GiB allocated, ~7.1 s wall. Tree is live (file set changed), so bytes/counts not directly comparable.
- M005A explicit-root (`/tmp/perf-proj`, release, 1 group, 100 KiB):
  - `--no-progress`: 0.04 s, 0.05 s, 0.05 s (3 runs).
  - progress enabled (non-TTY auto-hidden): 0.05 s, 0.04 s, 0.05 s (3 runs).
  - Semantic results identical (1 `private` group, 102400 bytes). Renderer overhead negligible (<5%; within noise, both auto-hidden non-TTY).
- M005A own repo (`./`, release, active tree): 0 groups, `elapsed=0.09 s`, `active_skipped=1` (recent build protects).
- Generic global scan (`/`): not run (would traverse entire machine + spawn Cargo per workspace; explicit-root evidence above satisfies the milestone; CI covers correctness, not wall-clock promises).

No pre-scan is added to make the bar determinate; determinate total is set only after workspace/group count is known.

## Verification run (final tree, before closure commit)

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (94 lib + 2 CLI-contract + 1 end-to-end).
- `cargo check --locked` — passed.
- `cargo +1.89 check --locked --all-targets` — passed.
- `cargo +1.89 test --locked --all-targets` — passed (94 + 2 + 1).
- `git diff --check` — passed.
- Focused: `cargo test workspace` (23 tests incl. real Cargo multi-member/redirected/conventional, grouping, fail-fast, non-UTF-8 Linux), `cargo test discovery` (7 incl. manifest-without-target, target prune, cargo-home prune, ignored-zero-Cargo), `cargo test progress` (8 incl. indeterminate/determinate, 5-row cap, hidden, fallback), `cargo test report` (4 incl. ownership labels, deduped total), `cargo test clean` (15, M004 safety still green) — all passed.
- Hosted stable Linux/macOS/Windows + MSRV 1.89: run 37067511675 (implementation commits `0dbe619` + `91b3e26`) — **success** (all four jobs green).
- Note: preceding hosted run for `0dbe619` (37067001800) failed only on Windows in the new redirected-target integration test. The failure was a real TOML-escaping bug (absolute Windows paths with backslashes broke `.cargo/config.toml` parsing, so metadata resolved 0 workspaces), fixed in `91b3e26` by writing the fixture `target-dir` with forward slashes. Final run above is green on all three platforms plus MSRV. Ubuntu/macOS were green on both runs; MSRV was green on both runs.
- Manual release: `scan /tmp/perf-proj --no-progress` and `scan /tmp/perf-proj` both report `100.00 KiB ... [private]` + `inventory estimate`, same counters, no control noise when piped.

## Confirmation destructive support remains M004-bounded

- `src/cleanup.rs` still requires absolute sandbox root, single-member workspace, conventional `target/`, `CACHEDIR.TAG` marker, no `build.build-dir` config/env, exact target match; redirected/shared/build-dir skipped.
- `grep -rn "remove_dir_all" src` — no hits.
- `grep -rn "remove_file" src` — only test fixtures/fake-runner and `config.rs` temp-sibling cleanup; no production deletion.
- M005A scan path invokes only `cargo locate-project` / `cargo metadata`; never `cargo clean`.
- `cargo test clean` green; `cargo_dry_run_leaves_fixture_untouched` proves non-mutation.

## Documentation/registry reconciliation (in closure commit)

- `README.md`: manifest-first + Cargo-authoritative resolution, grouping/dedup, ownership labels, fail-fast counters, inline progress + `--no-progress`, inventory-estimate wording; cleanup section explicitly notes redirected cleanup still deferred to M005B.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: M005A `ready` → `closed`, M005B `blocked` → `ready`.
- `plans/registry.md`: M005A `ready` → `closed`, M005B `blocked` → `ready`, subsystem current milestone `M005A ready` → `M005B ready`.
- `plans/implementation/.../005a-....md`: `ready` → `closed` + closure link.
- `plans/implementation/.../005b-....md`: `blocked` → `ready` (hard dependency satisfied; ADR 001 already accepted).
- Historical M001–M004/C001 untouched.

## Known limitations

- Large-machine scans now invoke one `cargo locate-project` per manifest plus one `cargo metadata` per workspace (sequential, bounded workers for traversal only). This is required by ADR 001 but is slower than the pre-M005A Cargo-free scan on trees with many workspaces. Fail-fast prunes/dedup mitigate, but do not eliminate, the subprocess cost.
- `--locked` metadata fails conservatively (diagnostic, no eligible output) for workspaces whose lockfile state cannot be resolved offline. Valid conventional projects with no deps (or with lock) resolve fine; exotic lock states are inventory-skipped, never promoted.
- Non-UTF-8 output paths are measured on Linux where the platform permits; macOS APFS rejects the synthetic `0xFF` fixture, so that test is Linux-gated.
- Generic `/` scans were not wall-clock qualified (explicit-root evidence above is the representative gate for renderer overhead).

## Unresolved findings

- No known correctness or security findings remain within M005A scope. The one hosted failure observed during M005A (`0dbe619` Windows redirected-target test) was investigated, fixed (`91b3e26`), and re-qualified green on all platforms; it is recorded above, not left open.
- Severity: no correctness/security findings; performance cost above is an accepted ADR 001 trade-off, not a defect.

## Dependency disposition

M005A is closed. M005B (`authorized redirected cleanup + --dryrun`) is **unblocked**: its hard dependency (M005A closure + final workspace/output/progress interfaces) is satisfied, and ADR 001 already satisfies the ownership-policy decision. No other follow-on work was found blocked on M005A.
