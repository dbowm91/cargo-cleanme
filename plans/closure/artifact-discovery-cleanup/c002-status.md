# Corrective C002 Closure — M005 Ownership Safety, Simulation Parity, Progress, and Global-Scan Qualification

Plan: `plans/implementation/artifact-discovery-cleanup/c002-m005-safety-progress-performance-reconciliation.md`

Disposition: **closed**

Implementation commit: `35705a2` on `main`
(`35705a2` implements C002 work packages A–F: ownership safety repair,
unified final `ExecutionProof` with simulation parity, coordinated
`MultiProgress` renderer with semantic determinate totals, authoritative
member-cache resolver optimization, `--stats` instrumentation, README/config
reconciliation, and full regression coverage. No separate fixup commit was
required; hosted CI is green on the implementation commit.)

Corrects (history preserved, not rewritten):

- M005A plan: `plans/implementation/artifact-discovery-cleanup/005a-workspace-output-resolution-fail-fast-progress.md`
- M005A closure: `plans/closure/artifact-discovery-cleanup/005a-status.md`
- M005B plan: `plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md`
- M005B closure: `plans/closure/artifact-discovery-cleanup/005b-status.md`

Authoritative decision (unchanged, restored to compliance):

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`

## Requirement-to-evidence matrix (C002-F1 through F6)

| Finding | Requirement | Evidence | Result |
|---|---|---|---|
| F1 ExternalUnproven promoted to destructive | Only `PrivateBounded` may proceed to path authorization; `ExternalUnproven`/`Shared`/`Uncertain` remain inventory-only under any `clean ROOT` or `allowed_output_roots` configuration; authorization never manufactures ownership; skip detail states both facts | `is_authorized` early-returns `false` for non-`PrivateBounded` (`src/cleanup.rs:271`); `ownership_skip_detail` (`external output ownership is unproven; configured authorization does not establish exclusivity`); `final_cleanup_proof` ownership gate (`src/cleanup.rs:733`); tests `external_unproven_inside_root_remains_inventory_only` (inside ROOT/allowed/both → skipped for all three non-private classes), `private_bounded_inside_workspace_is_authorized` (conventional still eligible), `external_unproven_outside_root_never_authorized_by_allowed_root` (allowed does not promote; detail contains both facts) | Pass |
| F2 `--dryrun` skipped non-mutating gates | Simulate runs every read-only gate Execute runs (ownership, auth, fresh resolution, member/source activity, target/build identity, output activity, group stability, markers, frozen-env, final preflight); only diverges by not spawning `cargo clean`; zero `clean` invocations | `final_cleanup_proof` shared by Preview/Simulate/Execute (`src/cleanup.rs:712`); `clean_with` Simulate branch emits `Simulated` only after `Ok(proof)` with no `runner.run`/`run_with_env`; tests `preview_is_default_and_simulate_invokes_no_clean` (Simulate 0 clean calls), `simulation_parity_*` (missing/invalid markers, valid private, recent source/output, changed build-dir, unknown capability — Simulate and Preview/Execute-dry dispositions match), `execute_pre_spawn_decision` helper exposes Execute dry decision without mutation | Pass |
| F3 Final preflight narrower than contract | Single final proof binds canonical workspace root+manifest, canonical member set, canonical target, canonical build/capability, physical covering roots, ownership, auth decision, marker validity, fresh source/output activity, frozen env/args; re-resolves target AND build, rebuilds grouping, rejects shape/classification change; inspects every covering root for existence/dir/no-symlink; markers part of same proof; fail-closed TOCTOU minimization | `ExecutionProof` struct with all bound fields (`src/cleanup.rs`); `final_cleanup_proof` steps 1–8 (ownership, auth, fresh resolve+identity/member/target/build compare, covering physical checks, fresh source/output activity, group stability with exact covering equality, markers, frozen-env); consumed immediately to build `clean_args`/`frozen_env`/cwd with no intervening window; tests `race_*` (target replacement, covering disappears, output/source recent, marker disappears/changes, symlink/classification) all skip before clean | Pass |
| F4 Progress composition and determinacy unqualified | Main+5 rows share one `MultiProgress` (single cursor/draw owner); `ProgressObserver` gains `units_total`/`unit_completed`; scan analysis and cleanup set determinate totals via trait; renderer resets/re-scopes on phase changes; cleanup determinate through real observer path; in-memory composed frame qualified | `IndicatifRenderer` holds `Option<MultiProgress>` with `add` (bars intercept draw target) (`src/progress.rs`); `coordinated_bar_count()==6`, `has_shared_draw_target`; `TestObserver::{total_for,completed_count}`; `analyze_groups` emits `units_total(Analysis)` + per-group `unit_completed`; `clean_with` emits `units_total(CleanupPhase)` + per-candidate `unit_completed`; phase-change reset + total-set reset prevent leaks; tests `coordinated_renderer_owns_six_bars_on_one_draw_target`, `in_memory_frame_contains_at_most_six_progress_lines` (≤6 lines, no unbounded scroll, `InMemoryTerm`), `phase_total_resets_*`, `discovery_begins_indeterminate_*`, `cleanup_progress_is_determinate_through_observer_path` (Preview/Simulate/Execute total==1 done==1 via `TestObserver`), `scan_analysis_total_is_determinate_through_real_path` | Pass |
| F5 Avoidable locate calls + unqualified global scan | After first metadata, canonical member manifests seed member→workspace cache; later known manifests require no locate/metadata; N-member in any order → 1 locate+1 metadata; no ancestry/TOML guessing; synthetic/global-like + representative real scan evidence; further concurrency only with evidence (not introduced) | `resolve_workspaces` member cache (`src/workspace.rs`, `canonical_manifest_key`, `member_to_root` seeded from `ws.members`+root); timing fields `cargo_locate_nanos`/`cargo_metadata_nanos`/`source_activity_nanos`/`output_sizing_nanos`/`discovery_nanos`; tests `repeated_members_invoke_metadata_once` (1+1), `n_member_workspace_requires_one_locate_one_metadata_regardless_of_order` (3+root in both orders → 1+1), `two_independent_workspaces_require_two_locate_two_metadata` (2+2), `failed_first_locate_does_not_poison_unrelated_workspace`; synthetic fixture (34 manifests, 29 workspaces: baseline locate 34 vs C002 locate 29, metadata 29 both, deduped 5, 1.33 s → 1.17 s); representative `/Users/davidbowman/projects` scan (368 manifests, 96 workspaces, locate 115 metadata 96 deduped 253, 18.47 s, Cargo 35% of wall, no concurrency introduced — remaining per-workspace resolution is Cargo-authoritative and necessary) | Pass |
| F6 Unconditional debug counters | `--stats` global flag gates counters/timings on stderr; default emits no debug line; `--stats` includes semantic counters + phase/process timings for scan and clean (Preview/Simulate/Execute); stderr-only, stdout byte-equivalent; `--no-progress --stats` canonical, no control sequences | `Cli::stats` global (`src/cli.rs`); `run_scan`/`run` Clean print `counters.stats_line()`+`timings_line()` only when `stats`; default scan stderr empty (no counters); tests `stats_flag_parses_globally_in_direct_and_external_forms`; manual synthetic: default stdout == `--stats` stdout (`diff` identical), default stderr empty vs `--stats` stderr `scan stats: …` with no ESC, clean Simulate stdout identical with/without `--stats`, `cleanup stats: mode=Simulate …` on stderr | Pass |

## Ownership authorization matrix

| Ownership | Inside `clean ROOT` | Inside `allowed_output_roots` | Inside both | Result |
|---|---|---|---|---|
| `PrivateBounded` | authorized (if covering inside boundary) | authorized (if covering inside allowed) | authorized | May proceed to full proof; cleanable only if proof succeeds |
| `ExternalUnproven` | skipped (`external output ownership is unproven; configured authorization does not establish exclusivity`) | skipped (same detail) | skipped | Inventory-only (C002 correction; M005B tests that asserted promotion updated) |
| `Shared` | skipped (`shared output remains inventory-only`) | skipped | skipped | Inventory-only |
| `Uncertain` | skipped (`uncertain output remains inventory-only`) | skipped | skipped | Inventory-only |

`cleanup.allowed_output_roots` remains parse-compatible (absolute only) but is clarified as location authorization only; it cannot promote `ExternalUnproven`. Preserved for schema compatibility; currently operationally redundant because `PrivateBounded` is normally workspace-contained. No test describes an allowed root as ownership proof.

## Proof that ExternalUnproven never reaches Cargo clean

- `is_authorized` returns `false` for any non-`PrivateBounded` before path checks.
- `final_cleanup_proof` step 1 returns `Err(ownership_skip_detail)` for non-`PrivateBounded` before any resolution; step 2 re-checks `is_authorized`; step 6 requires fresh group `ownership == PrivateBounded` with exact covering equality.
- `clean_with` only spawns (`runner.run`/`run_with_env` with `clean` args) in `Preview|Execute` branches after `Ok(proof)`; `Simulate` branch never calls runner with `clean`.
- Static: `rg "remove_dir_all" src/` shows no production recursive deletion (only `config.rs` temp-file cleanup and test-only `remove_file`/`remove_dir_all` in `#[cfg(test)]` fixtures/race hooks); `rg "runner.run" src/cleanup.rs` shows spawns only after proof with `proof.frozen_env`/`proof.target`/`proof.workspace_root`.
- Tests: `external_unproven_*` (unit + real-Cargo `external_outside_root_allowed_via_configured_root` now asserts Skipped even with allowed root), `real_temp_project_*` redirected-inside-workspace (Private) still Previewed/Simulated/Cleaned, redirected-outside-workspace (External) Skipped.

## Simulate-vs-Execute pre-spawn parity matrix (identical immutable fixture state)

| State | Simulate disposition | Execute dry decision (`final_cleanup_proof`/`execute_pre_spawn_decision`) | Zero `clean` for Simulate |
|---|---|---|---|
| Missing `CACHEDIR.TAG` | Skipped (`CACHEDIR.TAG …`) | Err (same proof) | Yes (`clean_calls().is_empty()`) |
| Invalid marker signature | Skipped (marker) | Err | Yes |
| Changed target (second metadata returns different target) | Skipped (`… target changed`) | Err | Yes |
| Changed build-dir (second metadata returns distinct build) | Skipped or filtered (no cleanable) | Err | Yes |
| Recent source (future mtime) | Filtered (no rows) or Skipped; never Simulated/Previewed | Err or filtered | Yes |
| Recent output (future mtime) | Filtered or Skipped; never Simulated | Err or filtered | Yes |
| Changed ownership/group shape (symlink/disappears/classification) | Skipped; never Simulated | Err | Yes |
| Unsupported/unknown build capability (`CARGO_BUILD_BUILD_DIR` set with `Unavailable`) | Skipped (frozen-env) | Err | Yes |
| Valid private (conventional, marker present, quiet) | Simulated (`no cargo clean …`) | Ok(proof) | Yes |

Tests: `simulation_parity_missing_and_invalid_markers_both_skip`, `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls`, `simulation_parity_recent_source_and_output_both_skip`, `simulation_parity_changed_build_dir_and_unsupported_capability_both_skip`, `race_target_replacement_skips_before_clean` (Simulate/Preview/Execute all Skipped with `changed`), `preview_is_default_and_simulate_invokes_no_clean`, `simulation_candidate_set_matches_execute_preflight` (retained).

## Zero-clean subprocess trace for Simulate

- `preview_is_default_and_simulate_invokes_no_clean`: Simulate → 1 `Simulated`, `clean_calls().is_empty()`, artifact preserved, render contains `no cargo clean`, no `recovered`.
- `simulation_parity_valid_private_*`: Simulate `Simulated` with `clean_calls().is_empty()` while Execute dry `Ok(proof)` (no spawn in dry helper).
- Manual synthetic `clean … --dryrun` (6 would-clean): `simulation: 6 would-clean … no cargo clean command was invoked`, stderr `cleanup stats` shows locate/metadata but no `Removed` output; artifacts preserved.

## Staged race/preflight tests (between initial scan and final proof)

Hook: `RaceRunner` mutates filesystem or returns different target on second metadata call (i.e., inside `final_cleanup_proof` after initial scan). Each case asserts Skipped/Filtered (never Simulated/Previewed/Cleaned) and no `cargo clean` when skipped:

- Target replacement (`race_target_replacement_skips_before_clean`): second metadata returns different target → Skipped `… target changed` for Simulate/Preview/Execute.
- Build-dir replacement (parity test staged build change): second metadata returns distinct build → Skipped/filtered.
- Build/target becomes symlink (`race_covering_becomes_symlink_skips_and_changes_classification`): replace covering with symlink → Skipped (covering symlink + classification change to `Uncertain`/vanished).
- Covering root disappears (`race_covering_root_disappears_skips`): `remove_dir_all` covering → Filtered/Skipped.
- Output becomes recent (`race_output_becomes_recent_skips`): touch artifact to future → Filtered/Skipped.
- Member source becomes recent (`race_source_becomes_recent_skips`): touch `src/main.rs` to future → Filtered/Skipped.
- Marker disappears/changes (`race_marker_disappears_and_changes_skip`): delete → Skipped `CACHEDIR`; corrupt → Skipped.
- Physical group changes classification: symlink case also covers `PrivateBounded` → `Uncertain`/vanished (`physical group vanished`/`changed classification`); `revalidation_detects_changed_target_and_skips` (retained) covers target identity.

Every case skips before `cargo clean` (Simulate zero clean calls; Preview/Execute also skip, so no spawn).

## Attended-terminal / in-memory composed progress evidence

- Production: `MultiProgress::with_draw_target(stderr_with_hz(10))`, `add` for main+5 rows (draw target intercepted, single owner); no alternate screen, no raw mode, stderr only, 10 Hz, `finish_and_clear` via `multi.clear()` + bars before stdout; hidden when `--no-progress`/`TERM=dumb`/non-TTY (no control sequences).
- Tests: `coordinated_renderer_owns_six_bars_on_one_draw_target` (6 bars, shared target), `in_memory_frame_contains_at_most_six_progress_lines` (`InMemoryTerm(24,120)` + `term_like_with_hz`, 6 reportable groups → ≤6 non-empty lines; 20 refreshes → still ≤6, no unbounded scroll), `renderer_caps_rows_at_five_largest_first`, `sizes_render_in_top_rows_and_clear_before_report`, `renderer_refresh_bounded_*` (hidden 0 refreshes for 10k entries; visible throttled to 10 Hz), `should_show_progress_*`, `renderer_error_falls_back_*`.
- Manual: `--no-progress --stats` stderr contains no ESC (`python3` check: `ESC in stats stderr: False`).

## Cleanup determinate-total evidence through real observer path

- `cleanup_progress_is_determinate_through_observer_path`: `TestObserver` drives real `clean_with` for Preview/Simulate/Execute on valid fixture; asserts `total_for(CleanupPreview/Simulate/Execute)==1` and `completed_count==1`.
- `scan_analysis_total_is_determinate_through_real_path`: real `resolve→build_groups→analyze_groups` with `TestObserver`; asserts `total_for(Analysis)==1`, `completed_count==1`.
- `phase_total_resets_so_cleanup_cannot_reuse_analysis_counts` and `discovery_begins_indeterminate_*`: phase changes reset totals; `group_measured` alone does not advance `done` (only `unit_completed` does).

## N-member locate/metadata call counts before/after

Synthetic/global-like fixture (`/tmp/.../c002-perf`: 200 irrelevant dirs, 20 empty manifests, 3 active, 5 inactive, 1 big 5-member workspace with shared target; 34 manifests total, backdated quiet except active):

- Baseline (pre-C002 `8177d82`, release, `--no-progress`): `scan counters: visited=549 pruned=9 manifests=34 workspaces=29 locate=34 metadata=29 … elapsed=1.33s` (6 locates for bigws).
- C002 (`35705a2`, release, `--no-progress --stats`): `scan stats: visited=549 pruned=9 manifests=34 workspaces=29 locate=29 metadata=29 … deduped_hits=5 … discovery=3.49ms locate=612.72ms metadata=548.81ms source_activity=2.89ms output_sizing=0.23ms elapsed=1.17s` (1 locate for bigws, 5 saved).
- Hard criterion met: N-member workspace produces 1 locate + 1 metadata after first authoritative resolution, in any discovery order (unit tests cover member-before-root and root-before-member).

## Synthetic/global-like performance counters/timings

Fixture above (broad: irrelevant, ignored-like empty, active, inactive, large multi-member):

```text
C002 scan --no-progress --stats:
scan stats: visited=549 pruned=9 manifests=34 workspaces=29 locate=29 metadata=29 failures=0 empty_skipped=20 active_skipped=3 groups_measured=6 bytes=53248 reportable=6 deduped_hits=5 uncertain_skipped=0 discovery=3.49ms locate=612.72ms metadata=548.81ms source_activity=2.89ms output_sizing=0.23ms elapsed=1.17s
stdout: 6 inactive groups (bigws 12 KiB + 5×8 KiB), 52 KiB inventory estimate
```

- Discovery traversal 3.5 ms, Cargo locate+metadata ~1.16 s (99% of wall — inherent per-workspace Cargo resolution, not redundant work after member-cache fix), activity 2.9 ms, sizing 0.23 ms.
- No additional concurrency introduced: remaining Cargo time is necessary (one locate+metadata per unique workspace, `--offline --locked --no-deps` preserved, deterministic ordering preserved, no guessing). Member-cache already removed the only avoidable amplification (per-member locates).

## Representative real generic scan (with conditional-closure reason for full `/`)

- Attempted release no-argument scan (`cargo-cleanme --no-progress --stats`, global root `/` on macOS dev host, read-only): exceeded 120 s timeout with no output (host filesystem too broad for full `/` in CI-like time). Recorded as `exit 124`, empty stdout/stderr. This environment cannot safely/meaningfully run full `/` to completion; C002 is therefore qualified via synthetic/global-like fixture above plus representative real tree below, not solely via one-workspace explicit-root fixture.
- Representative real developer-tree scan (release, `/Users/davidbowman/projects`, `--no-progress --stats`, read-only):

```text
scan stats: visited=381467 pruned=72 manifests=368 workspaces=96 locate=115 metadata=96 failures=19 empty_skipped=62 active_skipped=2 groups_measured=32 bytes=332670468096 reportable=32 deduped_hits=253 uncertain_skipped=0 discovery=1459.54ms locate=3779.81ms metadata=2640.59ms source_activity=4502.20ms output_sizing=6059.47ms elapsed=18.47s
stdout: 32 inactive groups, 309.82 GiB inventory estimate (largest: 66.99 GiB codegg-experiment target [private], …)
stderr diagnostics: 19 filesystem diagnostics; rerun with a bounded root if needed
```

- Member-cache saving: 368 manifests → 115 locates (253 saved, `deduped_hits=253`); baseline would have been 368 locates. Cargo resolution 35% of wall (6.42 s / 18.47 s); discovery 1.46 s, source 4.50 s, sizing 6.06 s. No material regression vs baseline (synthetic baseline 1.33 s → C002 1.17 s on same tree); remaining Cargo cost is per-unique-workspace and Cargo-authoritative.
- No universal wall-clock threshold required (host/storage topology varies); no material regression unexplained.

## Default-vs-`--stats` stdout/stderr evidence (synthetic fixture)

- Default `scan … --no-progress`: stdout report (6 groups, 52 KiB), stderr empty (no debug counter line, no diagnostics when none).
- `--stats` `scan … --no-progress --stats`: stdout byte-identical (`diff` identical), stderr `scan stats: …` with counters+timings, no ESC control sequences.
- Clean Simulate `--dryrun --no-progress` vs `--dryrun --no-progress --stats`: stdout identical (6 would-clean, `no cargo clean …`), `--stats` stderr `cleanup stats: mode=Simulate …` with counters+timings.
- `--stats` parses in direct (`cargo-cleanme --stats scan …`, `clean … --stats`) and external (`cargo cleanme --stats …`) forms (unit `stats_flag_parses_globally_in_direct_and_external_forms`); `--no-progress --stats` emits no terminal control sequences (manual ESC check).

## Full hosted CI/MSRV evidence

Local (implementation commit `35705a2`, macOS arm64, Rust 1.89 active):

- `cargo fmt --check` — PASS
- `cargo clippy --all-targets --all-features -- -D warnings` — PASS
- `cargo test --all-targets --all-features` — PASS (118 lib + 2 CLI-contract + 1 end-to-end)
- `cargo check --locked` — PASS
- `cargo +1.89 check --locked --all-targets` — PASS
- `cargo +1.89 test --locked --all-targets` — PASS (118 + 2 + 1)
- `git diff --check` — PASS

Hosted (push of `35705a2`, `.github/workflows/ci.yml`: stable Linux/macOS/Windows + 1.89 MSRV):

- Run 37093199244 — SUCCESS (all four jobs green; see `gh run view 37093199244`).

Static review (also in implementation):

- No production `remove_dir_all` / arbitrary recursive deletion (`rg` shows only `config.rs` temp-file cleanup and `#[cfg(test)]` fixture/race helpers; `src/cleanup.rs:1082` documents prohibition).
- `ExternalUnproven` cannot reach `cargo clean` (ownership gates + `is_authorized` + proof steps 1/2/6; tests above).
- `Simulate` cannot reach `cargo clean` (Simulate branch has no `runner.run`; zero-clean tests).
- All `cargo clean` spawns consume final validated proof (`clean_args(&proof)`, `proof.workspace_root`, `proof.frozen_env` immediately after `Ok(proof)`).

## Documentation/planning updates (in closure commit)

- `README.md` (in implementation commit `35705a2`): ownership/authorization semantics (only `PrivateBounded` cleanable, allowed roots do not establish exclusivity, currently redundant), normal-vs-`--stats` output, coordinated `MultiProgress` + observer determinate contract, simulation parity + final proof binding, member-cache 1+1, `--no-progress --stats` canonical, usage examples.
- `config.toml` (in `35705a2`): `[cleanup]` comments include `ExternalUnproven`, non-promotion, redundancy note; schema unchanged.
- `plans/implementation/.../c002-....md`: `ready` → `closed` + closure link (in closure commit).
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: C002 `ready` → `closed`; M005 `corrective required` → `closed` (M005A historically closed + M005B historically closed + C002 closed; M005 again release-qualified for destructive use).
- `plans/registry.md`: C002 `ready` → `closed`; subsystem current milestone `C002 ready` → `M005 complete (C002 closed)`; immediate handoff `Implement C002` → `C002 closed; M005 release qualification unblocked`.
- M005A/M005B closure records untouched (C002 is the historical correction).

## Unresolved findings and final disposition

- No open correctness or safety defects remain within C002 scope. The five pre-C002 M005B tests that encoded promotion (`private_redirected_*`, `external_outside_root_allowed_via_configured_root` expecting `Simulated` with allowed root, `real_temp_project_*` with outside-workspace redirect, `cargo_failure_isolated_*` without marker) were corrected to assert ADR 001 compliance; all now pass.
- Performance: full `/` no-argument scan exceeds 120 s on this representative macOS host (conditional reason above); synthetic/global-like + representative tree qualify the requirement. No further Cargo concurrency introduced (optional per plan; evidence shows remaining cost is necessary per-workspace resolution, not redundant amplification).
- `allowed_output_roots` preserved for compatibility but documented as currently redundant; a future ADR/milestone would be required to define explicit external-cache exclusivity — not reinterpreted here (per §6 out-of-scope).
- Disposition: **closed**. M005 (M005A + M005B historical + C002) is complete and again release-qualified. No other implementation plans are blocked on C002; future selective `cargo clean -p`, shared-cache GC, or daemon work would need new milestones.

## Dependency disposition

C002 is closed. M005 release qualification, blocked on C002 per registry (`C002 ownership/simulation/preflight/progress/performance correction -> ready; C002 -> closure; M005 release qualification -> only after C002 closure`), is now unblocked. No other `ready`/`active`/`blocked` plans depend on C002.
