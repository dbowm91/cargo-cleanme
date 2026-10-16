# M016 Status — Per-Island Cleanup Activation behind `--partitioned`

Plan: `plans/implementation/artifact-discovery-cleanup/m016-per-island-cleanup-activation.md`
(status flipped proposed → active on gate clearance, implementation complete).

Disposition: **conditionally closed.** Implementation, local verification,
live-shape fixtures, and hosted OS/MSRV lanes are complete. Two items remain
open, both release-time rather than code: (a) publication of a release
carrying the flag (tree is at 0.2.9, unpublished; a behavior change even
flag-gated requires a version bump, done — but no release carries it yet),
and (b) the ADR 004 item (e) distinct release gate (disposable fixtures +
published-artifact smoke), which belongs to the distribution subsystem at
release time. Per planning-and-closure, a milestone not yet carried by a
published release stays open — hence conditional, not closed.

Implementation (PR #14 branch
`plans/m015-proof-island-certificate-and-partition-report`, atop M015
`49bcab1` + ADR 004 acceptance `1c56e9f`):

- `1217a7f` feat: engine (partition at both scan-time block sites with the
  block retained, proceed-island unit filtering, hoisted-universe refresh
  that tolerates scan-time-known unresolved, per-island environment
  re-verification before first dispatch, scan-message suffix, human
  rows+block+island-outcomes render, additive JSON `islands` field,
  `-i` proceed-offer with held context, `--partitioned` flag threaded
  `cli.rs` → `CleanupRequest` → `run_engine_guarded` → engine, version
  0.2.9, CHANGELOG entry, regenerated completions/man).
- `ee6feb6` docs: 09-cleanup activation section, 07-workspace passenger
  enforcement note, 02-cli flag row, 10-reporting islands fields, overview
  counts, USAGE.md flag row + example, plan status active.
- `c1a645a` fix(test): Windows path-separator assertion found by the hosted
  lane (see below).

## Requirement-to-evidence map

| Requirement (plan §) | Evidence |
|---|---|
| A. Per-island scope resolution, whole-universe grouping, island-filtered dispatch (§6A) | `retain_proceed_island_units`; G1 subprocess Execute/Simulate/human cases; units_considered counts proceed units; held islands never rows |
| B. Per-island proof with combined evidence (§6B) | `final_cleanup_proof_roots` unchanged over full `workspaces`; hoisted universe holds all islands; gate-8 control (staged overlap from call 2 refused where the partition said go); hoisted refresh tolerates scan-time-known unresolved only (§7 rule, see below) |
| B4. Environment re-verification per island first dispatch | `output_env_matches` promoted to production; child-process probe `partitioned_environment_change_merges_remaining_islands` (own process, marker-armed, premise-checked) merges with zero spawns |
| C. Sequential island loop, existing unit loop inside (§6C) | Loop untouched; spawn/parse failures `continue` per unit; mid-run blocks keep their accurate messages and stop the run |
| D. `-i` interplay (§6D) | Listing filters to proceed islands, `held_context` channel, refusal unchanged when merged; subprocess refusal test; listing unit pair (offer + refusal) |
| E. Kill-switch and flag-off parity (§6E) | Wrappers default false; `partitioned_flag_off_matches_unpartitioned_engine` (identical block/results/spawns, no suffix, all modes) + the three M015 whole-scope pins verbatim |
| F. Render restructure (§6F) | Rows + block + `partition_outcome_section` + footer; pinned substrings intact; suffix counts-only |
| G. Documentation (§6G) | Deep dives + USAGE.md updated; citations re-derived; `generate-docs --check` 13/13 |
| Contract: JSON additive-only, exits unchanged (§9) | `IslandV1` (roots/verdict/cause/passengers), key-set test updated deliberately, log-line contract test, no new reason code, `update_json` untouched |
| Tests: twins, controls, surfaces, fixtures (§10) | 15 lib + 1 cli + 1 log + 5 subprocess; every hostile twin discriminating (documented naive failure); repo-owned G1/G1c fixtures in `tests/cli_contract.rs` |
| Version bump (§11 + ADR item (e)) | `Cargo.toml` 0.2.9, CHANGELOG entry; release gate itself outstanding (release-time, above) |

## In-flight finding (engine semantic, fixed before closure)

The first partitioned runs died in the hoisted refresh: `refresh_combined_universe`
failed on the scan-time-known orphan ("a workspace did not re-resolve").
The refresh could not distinguish tolerated unknowns from new ones. Fix:
the refresh takes the scan-time unresolved set and tolerates exactly those
manifests (canonical compare); anything newly unresolvable still fails the
run. Flag-off runs never reach the refresh with unresolved present, so the
tolerance preserves today's behavior exactly. Covered by every G1-shape
test (proceed requires surviving the refresh) and by the gate-8 control
(stale partition says go, fresh proof refuses).

## Verification actually run

| Command or run | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed on final tree. |
| `cargo clippy --all-targets --all-features -- -D warnings` | No issues on final tree. |
| `cargo test --all-targets --all-features` (lib, stable Linux) | 467 passed, 1 failed: the known pre-existing `real_temp_project_preview_and_execute_with_redirected_target` (host cargo outside the profile-selector allowlist; fails identically on pristine HEAD; recorded, not fixed — needs its own corrective) |
| `cargo test --all-features --test cli_contract` | 61 passed (56 + 5 new G1/G1c/`-i`/Simulate/human surfaces) |
| `cargo test --all-features --test end_to_end` | 4 passed |
| `cargo +1.89 check/test --locked --all-targets` (exact CI MSRV commands, local) | check clean; 468 + 61 + 4 passed, 0 failed |
| Hosted lanes, PR #14 (post-fix tree `c1a645a`) | `checks` lib: **ubuntu 467+1, macOS 464+1, Windows 429+1** — the single failure on every OS is the same pre-existing environment test; every M016-owned test green on all three OSes. `msrv` job green on hosted ubuntu. `installers`, `generated-docs`, `benchmark` green |
| Windows-only M016 defect found by the lane | `ends_with("orphan/Cargo.toml")` on a JSON string with backslash separators; fixed component-wise in `c1a645a`; post-fix Windows lane shows only the pre-existing failure |
| `python3 scripts/check-fixture-portability.py --self-test` + run | Self-test passed; 37 files clean (new subprocess tests unix-gated, stub POSIX) |
| `python3 scripts/check-doc-citations.py --self-test` + run | Self-test passed; 17 documents clean |
| `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | 13 artifacts match the clap model |
| `scripts/release-check.sh` | Not run (requires clean tree + `eggpack` + 1.89; release-time gate, outstanding above) |

## Known limitations

- The one red test on every lane is carried, not fixed: the profile-selector
  allowlist does not qualify host stable cargo. Needs its own corrective
  with hosted evidence — out of M016 scope; M016 neither fixes nor masks it.
  Because CI's `cargo test` stops at the lib-target failure,
  `cli_contract`/`end_to_end` did not execute on hosted lanes; they are
  green locally on stable and MSRV 1.89 (61 + 4).
- No publication yet: 0.2.9 carries the flag in-tree, but no release
  publishes it and the item (e) release gate has not run. Both are
  release-time work for the distribution subsystem.
- Mid-run blocks keep their own accurate messages without the acted-on
  suffix (they say "no FURTHER cleanup commands were run", which stays
  true); the suffix applies to the scan-time block message only. The
  island outcomes still render in that case.
- `09-cleanup.md` rows older than the M015/M016 rows still cite pre-M015
  line numbers (noted in-dive); a full re-derivation pass is follow-up
  work, not claimed here.

## Unresolved findings by severity

- **Low (test-env/toolchain drift, pre-existing, carried from M015):** as
  above; own corrective still needed.
- **Low (M016-owned, found and fixed pre-closure):** the Windows
  path-separator assertion (`c1a645a`). No product change; the engine was
  correct on Windows, the test spelling was not.
- **None medium or higher.** Per the corrective-pass rule, any future
  material defect gets a new plan, not an edit to this record.

## Deferred

- Passenger enforcement beyond the blessed rule, default-on activation,
  and any broader partition semantics are separate decisions with their own
  plans — this milestone activates exactly what ADR 004 blessed, nothing more.
