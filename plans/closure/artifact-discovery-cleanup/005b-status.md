# Artifact Discovery and Cleanup M005B Closure

Plan: `plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md`

Disposition: **closed**

Implementation commits: `ef27399` through `b7b1f31` on `main`
(`ef27399` implements work packages A–G: cleanup config + CLI mode model,
workspace ownership/revalidation, frozen Cargo env, `--dryrun` simulation,
redirected private cleanup, progress/final-report integration, qualification;
`b7b1f31` corrects the CACHEDIR.TAG marker gate and macOS canonical
revalidation found by the first hosted Ubuntu run.)

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Multi-member private workspace with redirected output can be previewed and cleaned safely | `real_temp_project_preview_and_execute_with_redirected_output` (valid package, no local `target/`, `.cargo/config.toml` `target-dir` → external inside ROOT; Preview → `Previewed`, Simulate → `Simulated`, Execute → `Cleaned` with observed decrease, artifact removed); `multi_member_private_workspace_can_be_cleaned` (2 members → 1 group, Preview + Simulate); manual release `clean /tmp/m005b-exec --yes` (20 KiB → 0 B, `Removed 2 files`) | Pass |
| Distinct target/build modeled, frozen, measured correctly | `frozen_env_sets_target_and_build_dir` (Distinct → both `CARGO_TARGET_DIR` + `CARGO_BUILD_BUILD_DIR`; Unknown without support → error); workspace `distinct_sibling_*`, `nested_*` (covering counts, no double-count); manual distinct3 (`cargo +1.95.0 metadata` → distinct `/tmp/distinct3/my-target` vs `/tmp/distinct3/my-build`; `cargo 1.89` → target-only, no `build_directory`, capability `Unavailable`) | Pass |
| External private output requires explicit authorization | `private_redirected_outside_root_requires_allowed_root` (unit, outside ROOT → `false`); `external_outside_root_allowed_via_configured_root` (real Cargo: outside ROOT without allowed → `Skipped` external-unproven; with `allowed_output_roots=[outside]` → `Simulated`); manual `/tmp/m005b-auth` (without config → 0 would-clean/1 skipped; with config → 1 would-clean) | Pass |
| Shared/unproven/uncertain remains non-destructive | `symlink_allowed_root_rejected_and_shared_forbidden` (symlink allowed-root → `Err`; Shared/Uncertain → `false` even inside allowed); `is_authorized` rejects `Shared`/`Uncertain` before any Cargo invocation; `private_redirected_inside_root_allowed` still labels `ExternalUnproven` (inventory wording preserved, only authorized for cleanup) | Pass |
| `--dryrun` runs full path through revalidation but invokes no `cargo clean` | `preview_is_default_and_simulate_invokes_no_clean` (Simulate → 0 `clean` calls, files identical, render contains `no cargo clean`, no `recovered`); `simulation_candidate_set_matches_execute_preflight` (Simulate vs Preview same display); manual `/tmp/m005b-qual --dryrun` (1 would-clean, 100 KiB, artifact still exists, no `.rustc_info.json` from clean) | Pass |
| Progress transient and final execute lists every cleaned group + deduplicated total | `progress::tests::*` (M005A renderer reused, 5-row cap, determinate, clear); `final_report_lists_all_groups_and_deduped_total` (7 groups all printed, `cleanup: 7 cleaned`, no `would-clean`); manual execute (1 cleaned, `observed decrease 20 KiB`, total = union); `main.rs` calls `renderer.finish_and_clear()` before `println!` for both scan and clean | Pass |
| M004 conventional cleanup remains compatible | `real_temp_project_*` conventional via workspace path (single-member, local `target/` → `PrivateBounded`, Preview/Execute); `execution_reports_observed_decrease_deduped` (fake removing artifact, decrease>0, double preflight); `valid_fixture` backdated quiet fixtures still Preview/Simulate/Execute; full suite green | Pass |
| Real temp-project destructive integration passes on stable | `real_temp_project_preview_and_execute_with_redirected_target` (Preview dry-run non-mutating, Simulate non-mutating + matching candidate set, Execute removes `artifact.bin`, decrease>0); manual `/tmp/m005b-exec --yes` (real `cargo clean`, `Removed 2 files`) | Pass |
| Linux/macOS/Windows stable + Rust 1.89 CI green | `cargo fmt --check`, `clippy -- -D warnings`, `cargo test --all-targets --all-features` (97 lib + 2 CLI + 1 e2e), `cargo check --locked`, `cargo +1.89 check/test --locked --all-targets`, `git diff --check` — all passed locally + MSRV; hosted run 37069574126 — **success** | Pass |

## CLI/mode tests

- `cli::tests::clean_requires_root_and_accepts_explicit_execution` (root required, `--yes` ok, `--yes --dry-run` rejected).
- `cli::tests::clean_modes_conflict_pairwise_and_default_is_preview` (default/`--dry-run`/`--dryrun`/`--yes` parse; all three pairwise conflicts rejected; `cargo cleanme clean …` external form works for all four spellings).
- `cleanup::tests::cli_modes_map_to_preview_simulate_execute` (`Preview` default, three variants distinct).
- Mode mapping in `main.rs`: default/`--dry-run` → `Preview` (Cargo `clean --dry-run --verbose`), `--dryrun` → `Simulate` (no clean), `--yes` → `Execute` (real clean). Distinct spellings documented in README and `--help`; no silent aliasing.

## Authorization tests

- `private_redirected_inside_root_allowed` (external-unproven inside `clean ROOT` → `true`).
- `private_redirected_outside_root_requires_allowed_root` (outside → `false`; inside allowed → `true`).
- `symlink_allowed_root_rejected_and_shared_forbidden` (symlink allowed-root → `Err`; Shared/Uncertain → `false` even inside allowed).
- `external_outside_root_allowed_via_configured_root` (real Cargo, outside without allowed → `Skipped`; with allowed → `Simulated`).
- Authorization roots validated absolute at config load (`malformed_and_relative_paths_fail` includes `cleanup.allowed_output_roots`); symlink/missing handled at cleanup time (missing → non-matching, symlink → `Err`, non-dir → non-matching). No traversal of allowed roots (only `symlink_metadata`/`canonicalize`; discovery walks only `clean ROOT`).

## Runtime Cargo matrix

- Local `cargo 1.89.0`: no `build_directory` (pre-1.91 `Unavailable`); redirected `target-dir` private fixture passes (`real_temp_*` on MSRV job).
- Local `cargo +1.90.0`: no `build_directory`.
- Local `cargo +1.95.0`: `build_directory == target_directory` when equal; distinct `my-target` vs `my-build` when `build-dir` configured (manual distinct3).
- Unit: `capability_*` (Unavailable/Equal/Distinct/Unknown), `frozen_env_*` (Distinct sets both vars; Unknown/Unavailable with separate build → error, never clean).
- Integration: `real_multi_member_*` (2 members → 1 workspace), `real_conventional_*` (`PrivateBounded`), `real_manifest_without_local_target_*` (`ExternalUnproven`), `real_temp_project_*` (redirected inside ROOT, Preview/Simulate/Execute).
- Hosted toolchain matrix (stable latest + 1.89) runs all above; stable-only `build-dir` distinct behavior qualified via `frozen_env` unit + manual distinct3 (1.95 distinct, 1.89 target-only), clearly separated from MSRV-compatible target-only tests.

## Simulation invariant (recording runner)

- `preview_is_default_and_simulate_invokes_no_clean`: Preview invokes `clean --dry-run` (artifact preserved); Simulate invokes 0 `clean` calls (any `clean` would fail the assertion), files byte-identical, render contains `no cargo clean`, no `recovered`.
- `simulation_candidate_set_matches_execute_preflight`: Simulate vs Preview same `display_path` (same authorized candidate set before mutation).
- Manual `/tmp/m005b-qual --dryrun`: 1 would-clean, 100 KiB, `artifact.bin` still exists, no `cargo clean` in trace (only `locate`/`metadata` would appear with verbose runner; system run shows no `Removed` output).

## Execution safety

- `revalidation_detects_changed_target_and_skips` (second metadata returns different target → `Skipped`, 0 `clean` calls).
- Canonicalized revalidation (`/tmp` vs `/private/tmp` macOS symlink): members/targets compared via `canonicalize`, so stable physical identity does not falsely report change (fixed during M005B afterhi local macOS repro; `real_temp_*` would otherwise skip as `workspace changed`).
- `cargo_failure_isolated_per_workspace` (bad manifest `locate` fails → diagnostic, good workspace still `Simulated`).
- Fresh source/output activity rechecked in `revalidate_group` + `final_preflight` (double-check before spawning); recent/future → `Skipped`; uncertain → `Err` → `Skipped`; symlink/missing fresh output → `build_groups` empty → `Ok(None)` → `Skipped` (never promoted).
- `execution_reports_observed_decrease_deduped` (fake removes artifact, `before > after`, `decrease>0`, `observed decrease` wording, no `would-clean`).
- `real_temp_project_*` Execute removes `artifact.bin` (real `cargo clean`, `Removed 2 files`), `after` measured via `measure_union` (missing → 0, no double-count; nested covering is outermost-only by construction).
- `measure_union` sums covering roots only (outermost, disjoint); equal/nested → 1 covering → counted once (workspace `nested_*` tests).

## Reporting/UI

- `final_report_lists_all_groups_and_deduped_total` (7 groups, all printed, `cleanup: 7 cleaned`, totals from `Cleaned` only, skipped/failed excluded).
- Totals: Preview → `cargo preview: … pre-clean estimate … no cleanup executed`; Simulate → `simulation: … estimated would-clean … no cargo clean command was invoked`; Execute → `cleanup: … pre-clean … post-clean … observed decrease …`. Preview/Simulate never labeled `recovered`; totals sum only relevant outcomes (skipped/failed excluded; fixed during M005B after manual `/tmp/m005b-auth` showed skipped inflating simulate total).
- Non-TTY deterministic/plain: `should_show_progress_*`, hidden renderer 0 refreshes, `TERM=dumb`/piped runs show no control noise (manual piped runs above).
- Progress: M005A `IndicatifRenderer` reused (`stderr_with_hz(10)`, 100 ms gate, 5 rows largest-first, distinct `CleanupPreview`/`CleanupSimulate`/`CleanupExecute` labels, `finish_and_clear` before stdout in both `run_scan` and `run` Clean).

## Performance/debug qualification (`--dryrun`)

Representative explicit-root (`/tmp/m005b-qual`, 1 workspace, 100 KiB, backdated quiet, release):

```text
Simulated  /private/tmp/m005b-qual/proj/target  [private]  before 100.00 KiB
simulation: 1 would-clean, 0 skipped, 0 failed; estimated would-clean 100.00 KiB; no `cargo clean` command was invoked
--no-progress: elapsed 0.33 s (locate=1 metadata=1 by construction, 1 group measured, 102400 bytes)
progress (non-TTY auto-hidden): elapsed 0.10 s, identical 1 would-clean/100 KiB
```

- Simulation candidate set matches adjacent Preview preflight on unchanged fixture (unit `simulation_candidate_set_*` + manual fresh-fixture runs: both 1 group, same display/bytes).
- No filesystem mutation (`artifact.bin` still exists, no `Removed` output).
- Progress enabled vs `--no-progress` identical semantics, overhead negligible (both auto-hidden non-TTY; 0.1–0.3 s noise on 100 KiB fixture, no sustained >5% regression).
- No pre-scan for determinism; cleanup phase determinate (candidate count known after grouping).

## Verification run (final tree, before closure commit)

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (98 lib + 2 CLI-contract + 1 end-to-end; 97 on macOS without Linux-only non-UTF-8).
- `cargo check --locked` — passed.
- `cargo +1.89 check --locked --all-targets` — passed.
- `cargo +1.89 test --locked --all-targets` — passed (98 + 2 + 1).
- `git diff --check` — passed.
- Focused: `cargo test cleanup` (16 incl. mode/auth/frozen/marker/revalidation/failure-isolation/report/real-temp/multi-member/external-allowed), `cargo test cli` (mode conflicts + external forms + `--no-progress`), `cargo test config` (template + `allowed_output_roots` absolute), `cargo test workspace`/`discovery`/`progress`/`report` (M005A still green) — all passed.
- Hosted Linux/macOS/Windows stable + MSRV 1.89: run 37070213730 (implementation commits `ef27399` + `b7b1f31`) — **success** (all four jobs green).
- Note: preceding hosted run for `ef27399` (37069574126) failed only on Ubuntu in the new real-temp redirected Preview test. The failure was a real safety gap: newer Cargo (stable) enforces `CACHEDIR.TAG` for `cargo clean` (`missing or invalid CACHEDIR.TAG`, exit 101), while fixtures had only `artifact.bin`. Fixed in `b7b1f31` by requiring the marker (clean `Skipped`, not `Failed`) and adding the marker to valid/real fixtures, plus canonicalized revalidation for macOS `/tmp` vs `/private/tmp`. Final run above is green on all platforms plus MSRV.
- Manual release: `clean /tmp/m005b-qual --dryrun` (1 would-clean, no mutation), `clean /tmp/m005b-auth/root --dryrun` without/with allowed (0/1 would-clean), `clean /tmp/m005b-exec --yes` (1 cleaned, 20 KiB → 0 B, target removed).

## Documentation updates (in closure commit)

- `README.md`: mode distinction (`--dry-run` vs `--dryrun` vs `--yes`), pairwise conflicts, simulation invariant, authorization (`clean ROOT` sandbox + `allowed_output_roots`), frozen env, sequential Cargo-delegated cleanup, no direct deletion, transient determinate progress with distinct labels + clear-before-final, final report examples (every group + deduped totals, estimate vs observed wording), no shared-cache claim, canonical `[cleanup]` template snippet.
- `config.toml`: `[cleanup] allowed_output_roots = []` (commented, absolute roots only, symlink invalid, empty preserves M004 containment).
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: M005B `ready` → `closed`.
- `plans/registry.md`: M005B `ready` → `closed`, subsystem current milestone `M005B ready` → `M005 complete`.
- `plans/implementation/.../005b-....md`: `ready` → `closed` + closure link.
- Historical M001–M004/C001/M005A untouched.

## Known limitations

- Large-machine cleanups still invoke one `locate` per manifest + one `metadata` per workspace (sequential) plus revalidation `metadata` per owner workspace and a final preflight `metadata` before each `cargo clean`. This is required for safety but is subprocess-heavy on trees with many workspaces; `--dryrun` qualifies the decision path without mutation.
- `--locked` metadata fails conservatively (diagnostic, skip) for exotic lock states; valid no-dep/with-lock projects resolve fine.
- `cargo clean --dry-run --verbose` (Preview) may create `target/.rustc_info.json` (recent output) as a Cargo side effect, causing an immediately adjacent real run on the same fixture to correctly skip as active. Unit Simulate-vs-Preview matching uses a non-mutating fake (no `.rustc_info.json`); real adjacent Preview→Simulate matching requires a fresh backdated fixture (documented above).
- Generic `/` cleanups remain prohibited by design (explicit `clean ROOT` required); no machine-wide destructive command exists.

## Unresolved findings

- No known correctness or security findings remain within M005B scope. The one hosted failure observed during M005B (`ef27399` Ubuntu real-temp Preview) was investigated, fixed (`b7b1f31`), and re-qualified green on all platforms; it is recorded above, not left open.
- Severity: no open defects; subprocess cost above is an accepted safety trade-off.

## Dependency disposition

M005B is closed. M005 (`redirected/shared Cargo output awareness`) is complete: M005A (resolution/fail-fast/progress) + M005B (authorized redirected cleanup/`--dryrun`) both closed, ADR 001 accepted. No other implementation plans are blocked on M005B. Future selective `cargo clean -p`, shared-cache GC, or TUI work (explicitly out of scope in M005A/B) would need new roadmap milestones, not unblocked implicitly here.
