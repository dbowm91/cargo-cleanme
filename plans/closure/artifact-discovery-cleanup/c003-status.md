# Corrective C003 Closure — Workspace Cleanup-Unit Atomicity and Full Ownership-Graph Revalidation

Plan: `plans/implementation/artifact-discovery-cleanup/c003-workspace-cleanup-unit-atomicity.md`

Disposition: **closed**

Repository baseline: `5af882122b4781e8a927d34a3792cbfd3c0d12cb`

Implementation commits: `dfb325e` through `caa4a6d` on `main`
(`dfb325e` implements C003 work packages A–F: CleanupUnit domain and
workspace→physical-group mapping, unit-wide destructive authorization, the full
bounded ownership-universe final proof, one-Cargo-invocation/one-result
accounting with union pre/post measurement, final-proof `--stats` accumulation,
README reconciliation, and the full regression matrix;
`caa4a6d` fixes C003 test isolation for macOS symlinked temp roots, where
`/var/folders/...` is a symlink while covering roots are canonical by contract).
Hosted CI is green on the closure head.

Corrects (history preserved, not rewritten):

- M005B plan: `plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md`
- M005B closure: `plans/closure/artifact-discovery-cleanup/005b-status.md`
- C002 plan: `plans/implementation/artifact-discovery-cleanup/c002-m005-safety-progress-performance-reconciliation.md`
- C002 closure: `plans/closure/artifact-discovery-cleanup/c002-status.md`

Authoritative decision (unchanged, clarified):

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
  (C003 clarification section already present at the C003 plan baseline)

## Requirement-to-evidence matrix (C003-F1 through F4)

| Finding | Requirement | Evidence | Result |
|---|---|---|---|
| F1 Physical-group authorization narrower than Cargo cleanup scope | One Cargo clean invocation is authorized as one workspace CleanupUnit covering the complete resolved OutputSet; every affected physical group must independently be `PrivateBounded` + authorized + measured + non-symlink + inactive + marker-qualified; a private target cannot carry an `ExternalUnproven`/`Shared`/`Uncertain`/unauthorized sibling build group; no Cargo spawn for any subset; skip reason names the failing output kind, path, and class | `unit_block_reason` gates ownership → location → measured state over every `AffectedGroup` of the unit (`src/cleanup.rs:396`), called before the proof (`src/cleanup.rs:575`) and again as the first proof step (`src/cleanup.rs:869`); `covering_is_authorized` keeps C002's "ownership class is authoritative" rule (`src/cleanup.rs:299`); one `AffectedGroup` per group the OutputSet touches, with target/build kinds and `GroupSkipReason` (`src/workspace.rs:1268`, `src/workspace.rs:1241`); `clean_with` has no per-group dispatch path — it iterates `units` only (`src/cleanup.rs:558`); tests `private_target_with_external_unproven_build_skips_entire_unit`, `private_target_with_shared_build_skips_entire_unit`, `external_unproven_target_with_private_build_skips_entire_unit`, `uncertain_build_identity_skips_entire_unit`, `symlink_build_identity_skips_entire_unit` (all three modes, zero `clean` calls), `gate_blocks_mixed_class_units_in_every_combination` (all 6 mixed-class combinations), `gate_blocks_unmeasured_and_unmapped_unit_members`, `unit_with_unmeasured_sibling_is_not_fully_measured`; real Cargo: a private target with an external build dir inside `clean ROOT` is skipped whole with `build output … is external-unproven; external output ownership is unproven; configured authorization does not establish exclusivity` | Pass |
| F2 Result/accounting group-scoped while mutation is workspace-scoped | One Preview/Simulate/Execute result per CleanupUnit; one aggregated pre-clean estimate; one deduplicated pre/post union measurement; progress determinate totals count CleanupUnits; no second vanished/skipped row for a sibling group | `build_cleanup_units` returns one unit per resolved workspace with the deduplicated covering union and aggregated bytes (`src/workspace.rs:1333`); `clean_with` pushes exactly one `CleanResult` per unit in every branch and measures `proof.covering` once for Execute (`src/cleanup.rs:681`); `observer.units_total(cleanup_phase, units.len())` + one `unit_completed` per unit (`src/cleanup.rs:551`); `CleanResult.output_roots` renders the affected roots under the workspace row (`src/cleanup.rs` render); tests `distinct_private_target_and_build_is_one_cleanable_unit`, `one_invocation_one_result_progress_total_and_no_sibling_row` (Preview 1 dry-run, Simulate 0 clean + 1 Simulated, Execute 1 clean + 1 Cleaned, progress total 1, no sibling row), `equal_and_nested_target_build_are_one_unit_and_one_invocation`, `post_clean_disappearing_sibling_is_measured_normally`, `post_clean_measurement_uncertainty_fabricates_no_decrease` | Pass |
| F3 Final ownership revalidation sees only the current workspace | Immediately before each disposition, re-resolve every workspace discovered under clean ROOT, rebuild the complete fresh physical graph, verify candidate identity/member set/target/build identity, remap the complete candidate OutputSet, require the same expected covering union and `PrivateBounded` classification, reject when any other discovered workspace can reach the affected output, fail closed when a universe workspace cannot be re-resolved | `final_cleanup_proof` steps 1–12 over the whole universe (`src/cleanup.rs:858`): unit gate, full universe re-resolution with per-workspace identity match, `build_groups` on the fresh universe, candidate identity/member/target/build comparison, `map_workspace_groups` remap with exact covering-union equality and per-group `PrivateBounded` requirement, explicit cross-workspace reachability including symlink/unresolvable roots (`provable_output_paths`), covering-root existence/type/no-symlink, fresh source + output activity bounded by the fresh universe's output set, markers over the complete union, frozen env/args from the fresh OutputSet; no whole-machine rescan and no cross-candidate reuse of pre-boundary state; tests `cross_workspace_overlap_race_into_target_skips_candidate`, `cross_workspace_overlap_race_into_distinct_build_skips_candidate`, `cross_workspace_symlink_into_candidate_output_fails_closed`, `universe_workspace_that_cannot_reresolve_fails_closed`, `initial_shared_overlap_is_decided_conservatively_without_proof`, `candidate_target_or_build_change_still_skips`; real Cargo: two workspaces with one redirected target dir are both skipped as `[shared]` with zero `clean` invocations | Pass |
| F4 Cleanup stats omit final-proof Cargo work | `cleanup --stats` includes initial locate/metadata work, final-proof locate/metadata work, final-proof source/output activity timing, and total cleanup elapsed, in an unambiguous form; default output unchanged and quiet | `ScanCounters::merge_proof` merges proof counters into dedicated `proof_*` fields rather than hiding them in the initial counters (`src/domain.rs:277`), plus `proof_stats_line`/`proof_timings_line` (`src/domain.rs:299`, `src/domain.rs:309`); the proof takes `&mut ScanCounters` and merges each universe re-resolution (`src/cleanup.rs:858`); `main.rs` prints both lines only under `--stats` (`src/main.rs:66`); tests `stats_account_for_final_ownership_universe_proof_work` (2 initial metadata calls vs 4 proof locate + 4 proof metadata + 4 universe refreshes for two cleanable candidates, and initial counters stay unpolluted) and `stats_flag_parses_globally_in_direct_and_external_forms`; real run: `cleanup stats: mode=Simulate … proof_universes_refreshed=1 proof_locate=1 proof_metadata=1 …` | Pass |

## CleanupUnit domain layer

| Record | Responsibility | Location |
|---|---|---|
| `GroupSkipReason` | why one physical group did not become a measured candidate (missing/empty/active-source/uncertain-source/uncertain-ownership/uncertain-measurement/active-output) | `src/workspace.rs:868` |
| `GroupOutcome` | per-group analysis result: measured candidate **or** blocking reason | `src/workspace.rs:898` |
| `analyze_groups_detailed` | fail-fast pipeline retaining the blocking reason per group | `src/workspace.rs:913` |
| `analyze_groups` | measured-only, size-descending wrapper (unchanged read-only scan contract) | `src/workspace.rs:1200` |
| `map_workspace_groups` | every physical group one workspace's complete OutputSet can touch, in display order | `src/workspace.rs:1241` |
| `AffectedGroup` | one affected group with covering roots, ownership, output kinds, measurement, skip reason | `src/workspace.rs:1268` |
| `CleanupUnit` | one Cargo workspace cleanup invocation: workspace identity, OutputSet, affected groups, deduplicated covering union, pre-clean union bytes, unmapped roots | `src/workspace.rs:1306` |
| `build_cleanup_units` | one deterministically ordered unit per resolved workspace; no reportable output ⇒ no unit | `src/workspace.rs:1333` |
| `unit_block_reason` | the unit-wide destructive gate (F1) | `src/cleanup.rs:396` |
| `ExecutionProof` | the final pre-spawn proof bound to one unit (F3) | `src/cleanup.rs:782` |

Core invariant enforced: a workspace is destructively cleanable only if every physical output group the `cargo clean` invocation can affect is independently private, authorized, inactive, non-symlink, marker-qualified, and stable under final full ownership-graph revalidation. No partial target-only/build-only cleanup exists.

## Mixed target/build ownership matrix

Rows are the plan §9 cases; each runs Preview, Simulate, and Execute against the same immutable fixture and asserts the disposition plus the Cargo call count.

| Target | Build | Expected | Observed | Test |
|---|---|---|---|---|
| PrivateBounded (distinct siblings) | PrivateBounded (distinct siblings) | one cleanable unit, one Cargo invocation, one result | `Previewed`/`Simulated`/`Cleaned`, 1 result row, 1 `clean` call (0 for Simulate), both output roots listed, aggregated pre-clean bytes | `distinct_private_target_and_build_is_one_cleanable_unit`, `one_invocation_one_result_progress_total_and_no_sibling_row` |
| PrivateBounded | ExternalUnproven (inside `clean ROOT`) | entire unit skipped | `Skipped [external-unproven]`, detail `build output … is external-unproven; external output ownership is unproven; configured authorization does not establish exclusivity`, 0 `clean` calls | `private_target_with_external_unproven_build_skips_entire_unit` |
| PrivateBounded | Shared (second discovered workspace owns it) | entire unit skipped | `Skipped [shared]`, detail names the build output, 0 `clean` calls | `private_target_with_shared_build_skips_entire_unit` |
| PrivateBounded | Uncertain (unprovable / symlink identity) | entire unit skipped | no reportable output ⇒ no unit, nothing cleanable; gate-level matrix asserts the mixed private+uncertain combination is blocked naming the build | `uncertain_build_identity_skips_entire_unit`, `symlink_build_identity_skips_entire_unit`, `gate_blocks_mixed_class_units_in_every_combination` |
| ExternalUnproven | PrivateBounded | entire unit skipped | `Skipped`, detail names the target output, 0 `clean` calls | `external_unproven_target_with_private_build_skips_entire_unit` |
| target == build | PrivateBounded | one group, one unit, one invocation | one group (`kinds == [Target, Build]`, `kind_label == "target+build"`), one `Cleaned` result, one `clean` call, one covering root | `equal_and_nested_target_build_are_one_unit_and_one_invocation`, `equal_and_nested_roots_collapse_to_one_physical_group` |
| build nested under target | PrivateBounded | deduplicated one physical union | one covering root, one invocation, one result | same two tests |
| target nested under build | PrivateBounded | deduplicated one physical union | one covering root, one invocation, one result | same two tests |
| distinct private siblings inside workspace | PrivateBounded | aggregated pre/post measurement includes both once | `before == measure(target) + measure(build)` (independently measured), `observed_decrease == before - after` | `union_accounting_counts_each_output_root_once` |
| PrivateBounded | unmeasured sibling (empty / never a candidate) | entire unit skipped | `fully_measured() == false`; gate blocks with `build output … is not cleanable: output directory contains no artifacts` | `unit_with_unmeasured_sibling_is_not_fully_measured`, `gate_blocks_unmeasured_and_unmapped_unit_members` |

## Pre-C003 defect reproduction (real Cargo, same fixtures)

Fixture: one workspace with a conventional private `target/` and a distinct build directory outside the workspace root but inside `clean ROOT` (`CARGO_BUILD_BUILD_DIR` set so Cargo itself reports it).

Baseline `5af8821`, `clean ROOT --no-progress --yes`:

~~~text
Cleaned  /tmp/.../ws/target  [private]  before 8.78 MiB  after 0.00 B  observed decrease 8.78 MiB  — Removed 25 files, 8.8MiB total
Skipped  /tmp/.../extbuild   [external-unproven]  before 68.00 KiB  — external output ownership is unproven; configured authorization does not establish exclusivity

cleanup: 1 cleaned, 1 skipped, 0 failed …
$ ls /tmp/.../extbuild
/usr/bin/ls: cannot access '/tmp/.../extbuild': No such file or directory
~~~

The baseline reported the external build directory as *skipped / external-unproven* and then **deleted it** with the one authorized `cargo clean`: the C003-F1 destructive boundary violation, reproduced end to end.

C003 head, same fixture, same command:

~~~text
Skipped  /tmp/.../ws  [external-unproven]  outputs /tmp/.../extbuild, /tmp/.../ws/target  before 8.85 MiB  — build output /tmp/.../extbuild is external-unproven; external output ownership is unproven; configured authorization does not establish exclusivity

cleanup: 0 cleaned, 1 skipped, 0 failed; pre-clean estimate 0.00 B, post-clean measured 0.00 B, observed decrease 0.00 B
extbuild contents: CACHEDIR.TAG, blob.bin
target contents: debug/, .rustc_info.json, CACHEDIR.TAG
~~~

Nothing was mutated and the reason is actionable.

## Proof that one Cargo invocation maps to one CleanupUnit result

- Production structure: `clean_with` iterates `units`; each iteration can reach at most one `runner.run` / `runner.run_with_env` with `clean` args, immediately after `Ok(proof)`, and pushes exactly one `CleanResult`. There is no loop over physical groups anywhere in the cleanup path.
- `Preview` records exactly one `cargo clean --dry-run --verbose` call per unit; `Simulate` records zero `clean` calls and one `Simulated` row; `Execute` records exactly one `clean` call and one `Cleaned` row. `one_invocation_one_result_progress_total_and_no_sibling_row` asserts all three plus `ProgressObserver::units_total == 1` and `unit_completed == 1` and that the report contains no `Skipped`/vanished sibling row.
- Real Cargo, distinct private target + build: one row `Simulated /tmp/.../ws [private] outputs /tmp/.../ws/build, /tmp/.../ws/target before 8.85 MiB`.
- Progress determinacy: `CleanupPreview`/`CleanupSimulate`/`CleanupExecute` totals equal the CleanupUnit count, never the group count.

## Simulate zero-clean trace

- `CleanMode::Simulate` branch in `clean_with` contains no `runner.run`/`run_with_env`; it only pushes `Simulated` with `detail = "simulation: no `cargo clean` command was invoked"` after a successful proof.
- `distinct_private_target_and_build_is_one_cleanable_unit` asserts `runner.clean_calls().is_empty()` for Simulate in the mixed target/build fixture, with a proof that succeeded.
- `preview_is_default_and_simulate_invokes_no_clean`, `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls`, `simulation_parity_*`, `cargo_failure_isolated_per_workspace` retain the C002 zero-clean coverage.
- Real Cargo: `simulation: 8 would-clean, 0 skipped, 0 failed; … no `cargo clean` command was invoked` for an 8-workspace root, with the target directories intact afterwards.

## Two-workspace overlap-race evidence from full fresh graph revalidation

| Scenario | Staged change between initial analysis and final proof | Result |
|---|---|---|
| B moves onto A's target | B's 2nd metadata call (the ownership-universe refresh) returns A's target dir | A `Skipped`: `ownership changed before cleanup; skipped: output … is now shared; shared output remains inventory-only`; 0 `clean` calls in Simulate/Preview/Execute; test also asserts B was genuinely re-resolved (`metadata_for(1) >= 2`) |
| B moves onto A's distinct build dir | B's 2nd metadata call returns A's build dir | A `Skipped` naming the now-shared output; 0 `clean` calls — the proof covers the complete OutputSet, not only the authorizing target group |
| B's target becomes a symlink to A's target | B's 2nd metadata call returns a symlink path | A `Skipped`: `another discovered workspace (…) now overlaps cleaned output …`; the fresh physical graph cannot represent that overlap, so the explicit cross-workspace check fails closed |
| B cannot be re-resolved | B's metadata fails from its 2nd call | A `Skipped`: `ownership could not be re-proven: workspace … did not re-resolve`; fail closed (C003 §10) |
| Initial overlap, later non-overlap | B already points at A's target in the initial graph; B's refresh no longer overlaps | Both units `Skipped [shared]` from the initial complete graph, `proof_workspaces_refreshed == 0`: fresh state is never used to *enable* cleanup, and stale Shared state is not relied upon |
| Candidate A changes its own target | A's 2nd metadata call returns another target dir | A `Skipped`: `workspace changed before cleanup; skipped: target changed (was …, now …)` (C002 behavior preserved) |
| Real Cargo, two workspaces sharing one redirected target dir | none (static) | Both `Skipped [shared] output /tmp/.../shared-target`; 0 `clean` invocations |

## target/build union pre/post accounting evidence

- `union_accounting_counts_each_output_root_once`: pre-clean bytes equal `traverse::measure_single_target(target) + measure_single_target(build)` measured independently of the tool, post-clean measures the same union, `observed_decrease == before.saturating_sub(after)`, and both output roots are listed exactly once.
- Equal/nested roots are counted once: `equal_and_nested_roots_collapse_to_one_physical_group` asserts `covering.len() == 1` and `bytes == that single group's bytes`; `distinct_target_and_build_become_one_unit_with_deduped_union` asserts `unit.bytes == sum of the two group measurements` and `covering.len() == 2`.
- `post_clean_disappearing_sibling_is_measured_normally`: one sibling directory is removed by Cargo; the surviving union is measured normally and the observed decrease equals `before - after`.
- `post_clean_measurement_uncertainty_fabricates_no_decrease`: a sibling output path changes type after a successful Cargo run; the result is `Cleaned` with `after_bytes == None`, `observed_decrease == None`, detail `Cargo succeeded; post-clean measurement failed: …`, and `diagnostics >= 1`.
- Real Cargo, one `cargo clean` with a distinct build dir: `Cleaned /tmp/.../ws [private] outputs … before 8.85 MiB after 0.00 B observed decrease 8.85 MiB` — one row, one union measurement.

## cleanup progress total evidence

- `one_invocation_one_result_progress_total_and_no_sibling_row` asserts `TestObserver::total_for(phase) == Some(1)` and `completed_count(phase) == 1` for Preview, Simulate, and Execute against a distinct target+build workspace.
- `cleanup_progress_is_determinate_through_observer_path` (C002) still passes unchanged.
- Renderer totals remain re-scoped per phase in `src/progress.rs`; the cleanup total is the CleanupUnit count.

## --stats proof-work accounting

- Test: `stats_account_for_final_ownership_universe_proof_work` — two cleanable candidates in a two-workspace ownership universe ⇒ initial `cargo_metadata_calls == 2` while `proof_workspaces_refreshed == 4`, `proof_cargo_locate_calls == 4`, `proof_cargo_metadata_calls == 4`; proof timings are non-zero; `proof_stats_line()` contains `proof_metadata=4` / `proof_locate=4`; the initial counters remain unpolluted (`stats_line()` contains `metadata=2`).
- Real Cargo, 8 private workspaces under one clean ROOT (all simulated, nothing mutated):
  - baseline `5af8821`: `locate=8 metadata=8 … elapsed=0.49s` (per-candidate single-workspace proof, hidden inside the initial counters);
  - C003 head: `locate=8 metadata=8 … proof_universes_refreshed=64 proof_locate=64 proof_metadata=64 proof_locate=937.23ms proof_metadata=942.42ms proof_source_activity=4.36ms proof_output_sizing=4.25ms elapsed=2.16s`.
  - The bounded ownership-universe refresh costs N resolutions per cleanable candidate (8 × 8 = 64). This is the C003 §11 accepted cost at the destructive boundary, is now visible in `--stats`, and was not traded away for speed.
- Canonical non-mutating qualification path `clean ROOT --no-progress --stats` (default Cargo preview) and `--dryrun --no-progress --stats` both exercised manually; default output without `--stats` emits no counter line on either stream.

## Hosted CI and MSRV

- `cargo fmt --check` — pass.
- `cargo clippy --all-targets --all-features -- -D warnings` — pass.
- `cargo test --all-targets --all-features` — 149 pass (123 before C003; 26 new C003 tests).
- `cargo check --locked` — pass.
- `cargo +1.89 check --locked --all-targets` — pass.
- `cargo +1.89 test --locked --all-targets` — pass (149 lib+integration tests).
- `git diff --check` — pass.
- Hosted CI run `37096527542` on `caa4a6d`: `checks (ubuntu-latest)`, `checks (macos-latest)`, `checks (windows-latest)`, `msrv` (Rust 1.89) — all SUCCESS.
- The first C003 CI run (`37096327696`) failed only in three new macOS tests, all caused by `/var/folders/...` temp roots being symlinks while covering roots are canonical by contract; fixed in `caa4a6d` (canonical authorization root in the synthetic gate fixtures, and an unmeasured-sibling fixture that does not depend on output-pruning behavior). No production code changed in the fixup. M005A/M005B/C002 closures were untouched.

## Static review

- No production recursive output deletion: a per-file scan of every `src/*.rs` outside `#[cfg(test)]` finds only `config.rs:181` (`fs::remove_file` of a config-init temporary file) and one *comment* in `cleanup.rs` documenting the prohibition. All `remove_dir_all`/`remove_file` calls in `cleanup.rs` are inside `#[cfg(test)]` fixtures and race hooks.
- No `cargo clean` spawn is reachable from a partially authorized OutputSet: the only two spawn sites are inside the `Preview | Execute` branch after `Ok(proof)`, and `final_cleanup_proof` step 1 re-runs the unit gate.
- Every `cargo clean` spawn consumes a CleanupUnit proof: `clean_args(&proof, mode)`, `proof.workspace_root`, `proof.frozen_env`, and `proof.covering` are taken only from the `Ok(proof)` value; no `RawGroup` is consulted for a mutation decision after proof generation.
- `Simulate` never spawns Cargo clean: its branch has no spawn call.
- One workspace cleanup invocation cannot produce multiple independent cleaned results: one iteration of the `units` loop yields at most one result.
- No partial target-only/build-only cleanup path was added; no Cargo-private cache internals are parsed; no whole-machine rescan is performed during cleanup.

## Documentation/planning updates

- `README.md` (in `dfb325e`): cleanup semantics are now workspace-CleanupUnit oriented; explains distinct target/build mixed ownership and the whole-unit skip, the deduplicated union and one invocation/result/measurement, the bounded ownership-universe proof and its fail-closed cases, per-row output roots, union pre/post accounting with no fabricated recovered bytes, and the separate `--stats` proof accounting. Read-only scan description unchanged.
- `plans/implementation/.../c003-....md`: `ready` → `closed` + closure link.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: C003 `ready` → `closed`; M005 `corrective required` → `closed` (M005A + M005B + C002 + C003 closed, M005 again release-qualified); dependency graph updated; deferred macOS output-exclusion observation recorded.
- `plans/registry.md`: C003 `ready` → `closed`; subsystem current milestone updated; immediate handoff and dependency order updated; baseline `caa4a6d`.
- M005A/M005B/C002 closure records untouched (C003 is the historical correction).

## Unresolved findings and final disposition

1. **Deduplicated ownership-universe refresh cost (accepted, measured).** Each cleanable candidate re-resolves every workspace discovered under `clean ROOT`, so a root with N cleanable workspaces performs N² bounded Cargo resolutions (8 workspaces ⇒ 64 locate + 64 metadata, 2.16 s wall in `--dryrun` on this host, versus 0.49 s for the weaker baseline). C003 §11 accepts this cost at the destructive boundary, forbids weakening the proof for speed, and forbids parallel destructive proof resolution without evidence. It is fully visible in `cleanup --stats`. A future milestone may reduce it (for example a per-invocation refresh whose reuse is proven not to cross the revalidation boundary) but must not weaken the proof.
2. **macOS output-exclusion observation during source activity (low, eligibility-neutral, pre-existing).** `resolve_workspaces` canonicalizes `workspace_root` and output physical paths, but member `source_root`s come from Cargo verbatim. On macOS, when a path component of the clean root is a symlink (`/var/folders/...`, `/tmp` on some setups), the source-activity walk starts from a non-canonical member root while output exclusions are canonical, so output directories are not pruned during that walk. Effect: wasted traversal and different `active_skipped` attribution; **no eligibility or safety difference**, because the per-group output activity gate fires on exactly the same recency predicate, so a workspace with recent output is skipped either way. The C003 final proof is unaffected (it uses canonicalized comparisons and requires the affected output to be old). C003 did not change this production behavior because doing so would widen destructive eligibility on macOS — a change this corrective is not chartered to make. It was found because a new C003 test depended on output pruning; the test was made platform-independent instead (`caa4a6d`). Candidate follow-up: canonicalize member source roots at resolution time, qualified as its own corrective with cross-platform measurement.
3. **Unmeasured-sibling silence.** When a workspace has no reportable affected output at all (for example its only output root is `Uncertain` because a sibling root's identity cannot be proven), no CleanupUnit is created and no result row is emitted, per C003 §7.1 ("a workspace with no reportable/inactive affected output should not become a CleanupUnit"). The actionable mixed-class reason is provided whenever a unit does exist (`gate_blocks_mixed_class_units_in_every_combination`). This preserves the M005A/C002 decision to avoid a noisy skip row for every pruned/empty workspace.
4. **Compatibility narrowing (intended).** Configurations previously allowed to clean will now skip when any sibling output group of the same workspace is not independently safe. This is the intentional C003 safety correction and is exercised by `private_target_with_external_unproven_build_skips_entire_unit`, `private_target_with_shared_build_skips_entire_unit`, `external_unproven_target_with_private_build_skips_entire_unit`, and `uncertain_build_identity_skips_entire_unit`. CLI flags, config schema, Rust 1.89 MSRV, and the read-only scan report are unchanged.
5. **Generic scan performance.** The previously observed full-root no-argument scan exceeding 120 s remains a separate future performance/pruning line and is explicitly out of C003 scope; no C003 acceptance criterion depends on it.
6. Disposition: **closed**. All C003-F1 through F4 findings are repaired with regression coverage, the destructive boundary now covers the complete workspace OutputSet, and the bounded ownership graph is revalidated from fresh Cargo state immediately before mutation. M005 destructive release qualification, reopened until C003 closed, is unblocked.

## Dependency disposition

C003 is closed. Per `plans/registry.md`, the order was `… C002 -> historically closed; C003 ready; C003 -> closure; M005 destructive release qualification -> only after C003 closure`. With C003 closed, M005 destructive release qualification is unblocked and M005 is closed on this roadmap (M005A + M005B + C002 + C003). No other `ready`/`active`/`blocked` implementation plan exists in this subsystem, so no further plan status changes are required; finding 2 above is a candidate future corrective rather than a blocked plan. Future selective `cargo clean -p`, shared-cache GC, external-cache exclusivity policy, and generic scan pruning remain unclaimed and would need new milestones.
