# M015 Status — Proof-Island Certificate Design and Partition Reporting (no destructive change)

Plan: `plans/implementation/artifact-discovery-cleanup/m015-proof-island-certificate-and-partition-report.md`

Disposition: **closed.** Implementation, local verification, live-data
collection, and hosted OS/MSRV lanes are complete. Landed as commit
`49bcab1` (fix commits `eb82ad2`, `1993301` below); the one remaining
red is the pre-existing profile-selector environment failure, which fails
identically on pristine HEAD, is unrelated to M015, and is carried as an
unresolved finding with its own corrective need. Unlike behavior
correctives, **no publication is required to close**: M015 changes zero
dispositions by construction (parity test + JSON key-set lock below), so no
release must carry it. M015 closure clears M016 §2 gate 1 (gate 2 cleared
by ADR 004 acceptance `1c56e9f`; gate 3 was already satisfied).

Implementation (working tree atop baseline `0a5a6e6`, release 0.2.8;
landed in `49bcab1`, plus `eb82ad2` and `1993301` on the closure PR):

- `src/domain.rs` — `UnknownCapability` (`RefusedByCargo` vs `Unproven`) on
  `UnresolvedOwnershipParticipant`, recorded where each failure is classified.
- `src/workspace.rs` — `believed_workspace_manifest`, `UnknownDisposition`,
  `attribute_unknown` (pure failure-class table), capability recording in
  `resolve_workspaces_with_coverage`, per-class + resolution-level tests.
- `src/cleanup.rs` — `OutputProvenanceSnapshot`, `PartitionMergeCause`,
  `ProofIsland`, `ProofPartition`, `CleanReport.proof_partition` (human render
  only), `compute_proof_partition` (pure union-find), `discovery_holes`
  (locatable + pathless remainder), `partition_section` (bounded advisory
  render), `universe_manifests_stable` (extracted H-2 predicate), parity test,
  premise-negative hostile controls, `blocked_listing_detail` for `-i`.
- `src/main.rs`, `src/cli.rs` (comment/anchor only), `src/output.rs`
  (JSON key-set stability test only), `tests/cli_contract.rs` (blocked-human
  subprocess case), `architecture/09-cleanup.md` (certificate section +
  M015 test row), `architecture/07-workspace.md` (report-only attribution
  section), `architecture/16-interactive.md` (anchor listing),
  `architecture/overview.md` (re-derived counts).
- New files: the M015 plan itself and this record.

## Plan §2 disposition (in-flight delta)

The plan required a disposition for the uncommitted delta named at plan
time (anchor listing, `blocked_listing_detail`, believed-workspace
pass-through). Recorded here:

- **Kept as report-only foundation:** working-directory-anchored `-i`
  listing (`main.rs`, `cli.rs` comment, `16-interactive.md`) and
  `blocked_listing_detail` change no disposition — the proof universe stays
  wide, the engine still blocks identically, only offered rows and human
  error text change. Pinned by `ancestry_selection_matches_in_both_directions`
  and `blocked_listing_detail_names_participants_and_hints_workspace_membership`.
- **Enforcement pass-through REMOVED.** The tree contained a
  `unresolved.retain(...)` block withholding believed-workspace claimants
  from `unresolved` when the named parent resolved. That narrows the ADR 001
  C004/C006 whole-scope block — a disposition change M015's non-goals
  explicitly forbid, and one no corrective plan or ADR acceptance covers.
  It was deleted (`src/workspace.rs` now states why at the removal site);
  what ships is report-only attribution (`attribute_unknown` →
  `PassengerOf`), which never merges, never unblocks, never spawns. The
  removed shape is preserved below under "Deferred enforcement" so a future
  ADR-004-gated corrective can resurrect it with cover. This is the §2
  "re-derive it" disposition, and the reason previous verification would have
  missed the overreach is recorded: the pass-through arrived bundled with
  report-only work and its tests asserted the *relaxed* behavior as
  expected, so green meant "enforcement works", not "research is pure".

## Requirement-to-evidence map

| Requirement (plan §6/§11) | Evidence |
|---|---|
| A. Certificate definition (§4, four proofs + merge-by-default + provenance format) | Code types + doc comments (`cleanup.rs:338-427`, `workspace.rs:400-438`); reviewable spec in `architecture/09-cleanup.md` §"report-only proof-island partition". |
| B. Failure-class table (§5) implemented as pure decision fn with per-row tests | `attribute_unknown` + `attribute_unknown_merges_every_unproven_class` (8 merge rows incl. believed-reason-with-Unproven-capability, proving classification follows the recorded branch never the prose), 4 refused-shape tests, 2 resolution-level capability pins (`src/workspace.rs:3860-4116`). |
| C. Partition computation: union-find, deterministic, bounded | `compute_proof_partition` (`cleanup.rs:2023-2159`); split/merge/nesting/string-prefix-sibling/determinism/degenerate tests (`:5509-5719`); bounds (5 islands + overflow, 5 causes + overflow). |
| D. Human-only partitionability report on blocked runs; no JSON/exit change | `partition_section` renders islands/passengers/notes/causes/provenance; `cleanup_json_result_key_set_is_stable` (`output.rs:658`) fails on any additive `CleanupV1` change; subprocess `blocked_human_cleanup_report_carries_advisory_partition_and_json_stays_one_object` (`tests/cli_contract.rs:1930`) asserts one JSON object + advisory section + zero clean calls. |
| E. Disposition parity proof | `partition_reports_islands_but_engine_still_blocks_with_zero_spawns_in_every_mode` (`cleanup.rs:5026`): two disjoint workspaces + refused orphan → certificate reports 2 would-proceed islands, engine still blocks with empty results and zero clean spawns in all three modes. |
| F. Premise-negative hostile controls, each failing against a naive skip | Redirect (`:5078` vs per-workspace naive), nested-output prefix (`:5114` vs equality-only naive), metadata-failure overlap + zero-spawn engine assertion (`:5152`), symlinked output unix (`:5211`), build-dir-only env move (`:5256` vs target-only naive), H-2 growth as predicate (`:5297` vs always-stable stub) and end-to-end (`:5326`); documented non-control target-under-foreign-sources (`:5392`) splits under both and ships as a positive case, not a control. |
| G. Live-data collection (island-hit rate for M016 go/no-go) | Real-Cargo fixtures below; S1 evaluated: islands DO occur (G1 2 islands, G1c passenger island) → S1 rejection not triggered. |
| H. Documentation with re-derived citations; ADR 004 untouched | `09-cleanup.md` certificate section + M015 test row (new citations verified below); `07-workspace.md` report-only attribution section; `overview.md` counts re-derived (32,607 lines; 453 inline tests: cleanup 118 + workspace 76). ADR 004 still proposed, unedited. |
| Whole-scope-block pins from §2 unchanged | `PerPathFailRunner` analogue, `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes`, `combined_roots_classify_cross_root_output_as_shared_in_every_mode` all green in the final full run. |

## Live data (package G, real Cargo 1.99.x, final tree)

Fixtures under `/tmp/m015-live` (ephemeral; outputs captured here):
mixed pair + refused edition-orphan (G1), ws + nested edition orphan (G1b),
ws + genuine believes-orphan naming the resolved parent (G1c), healthy +
workspace-with-broken-member (G2), cross-root `CARGO_TARGET_DIR` redirect
pair (G3), permission-hole under `chmod 000` (G4).

| Fixture | Exit | Block | Partition shown (advisory) |
|---|---|---|---|
| G1 | 1 | `OwnershipUnproven` | 2 islands, both would-proceed + 1 scope note |
| G1b | 1 | `OwnershipUnproven` | 1 island would-proceed + 1 scope note |
| G1c | 1 | `OwnershipUnproven` | 1 island would-proceed + **passenger** (believed claimant attributed, block retained — the M015 headline shape) |
| G2 | 1 | `OwnershipUnproven` | 1 island, **blocked**, merged by named `UnprovenUnknown` (broken root) + 1 scope note (member) |
| G3 | 0 | none | none (no block → no section); both units `Skipped … shared`, combined classification intact |
| G4 | 1 | `IncompleteDiscovery` | 1 island, **blocked**, merged by counted `UnnamedDiscoveryHoles(1)` — pathless diagnostics merge rather than rendering would-proceed |

JSON twin (G1, exit 1): exactly one object, `scope_blocked: true`,
`units: []`, 14 result keys (no partition field), stderr empty.
Stop conditions: **S1 not triggered** (islands occur in real-shaped runs);
**S2 not triggered** (every hostile shape merges; the one split-under-both
shape is documented as a non-control); **S3 not triggered** (parity green;
the one tree element that would have tripped its spirit — the enforcement
pass-through — was removed above).

## Verification actually run

| Command or run | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed on final tree. |
| `cargo clippy --all-targets --all-features -- -D warnings` | No issues on final tree. |
| `cargo test --all-targets --all-features` (lib) | 450 passed, 1 failed: `real_temp_project_preview_and_execute_with_redirected_target` expects `Previewed`, got `Skipped`. **Pre-existing environment failure, not M015**: fails identically on a pristine-HEAD worktree (`/tmp/head-check`, since removed); mechanism is the version allowlist — host cargo is 1.99.1, `clean_capabilities_from_version("1.99.1").profile_selector` is false by construction (exact 1.89.0/1.90.0/1.95.0/1.99.0 only), so the profile-selector preview skips in the safe direction. Recorded below as an unresolved finding; no M015 file touches capability code. |
| `cargo test --all-features --test cli_contract` | 56 passed (55 + new blocked-human case). |
| `cargo test --all-features --test end_to_end` | 4 passed. |
| `python3 scripts/check-fixture-portability.py --self-test` + run | Self-test passed; 37 files clean. |
| `python3 scripts/check-doc-citations.py --self-test` + run | Self-test passed; 17 documents clean. |
| `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | 13 artifacts match the clap model (CLI surface unchanged). |
| Hosted lanes, PR #14 (`plans/m015-proof-island-certificate-and-partition-report`), post-fix tree `eb82ad2` + CI fix `1993301` | `checks` lib target per OS: **ubuntu 450 passed + 1 failed, macOS 447 passed + 1 failed, Windows 413 passed + 1 failed** — the single failure on every OS is the same pre-existing `real_temp_project_preview_and_execute_with_redirected_target` environment failure (below). fmt, clippy, and all other lib tests — including every M015 certificate/partition/hostile-control test — green on all three OSes. `msrv` job green on hosted ubuntu. `installers`, `generated-docs`, `benchmark` green. |
| MSRV 1.89 lane (`cargo +1.89 check/test --locked --all-targets`, exact CI commands, local) | check clean; **lib 451 passed, 0 failed; cli_contract 56 passed; end_to_end 4 passed.** The allowlist covers 1.89.0, so the pre-existing failure passes here — confirming it is pure toolchain drift, not product behavior. |
| macOS canonicalization defect found by the hosted lane (M015-owned) | Pre-fix macOS run: 445 passed, 3 failed — two M015 tests (`partition_reports_islands_…`, `unresolved_manifest_overlap_…`) asserted raw-TMPDIR paths while discovery canonicalizes (`/var` vs `/private/var`). Fixed in `eb82ad2` (canonicalize the tempdir base, established repo pattern); reproduced on Linux with a symlinked TMPDIR (fail before, pass after); post-fix macOS lane shows only the pre-existing failure. Product code needed no change (canonical throughout). |
| CI structural finding + fix (`1993301`) | The `checks` matrix ran with default fail-fast, so the first red lane (ubuntu, fastest) cancelled macOS/Windows before they reported — repeated cancellations made hosted evidence unobtainable on any branch while any lane is red. `fail-fast: false` added to the `checks` matrix with rationale comment (installers matrix untouched). Post-fix, all three OS lanes report independently. |
| `scripts/release-check.sh` | Not run (requires clean tree + `eggpack` + 1.89; not routine verification). |

## Known limitations

- Landed in `49bcab1` on closure PR #14 (branch
  `plans/m015-proof-island-certificate-and-partition-report`, unmerged);
  fix commits `eb82ad2` (macOS canonicalization) and `1993301` (CI
  fail-fast) ride the same PR.
- The one red test on every hosted lane is carried, not fixed: the
  profile-selector allowlist does not qualify host stable cargo, so
  `real_temp_project_preview_and_execute_with_redirected_target` skips
  where it expects preview. Fail-closed direction; needs its own
  corrective (below). Because CI stops at the lib-target failure,
  `cli_contract`/`end_to_end` did not execute on hosted lanes; they are
  green locally on stable and MSRV 1.89 (56 + 4).
- The operator's original single-bad-manifest case (e.g. the codegg
  `fuzz/Cargo.toml` shape, reproduced as G1c) **still blocks by design**:
  the partition shows the would-proceed island and names the passenger, but
  the engine does not act on it. Relief is now authorized (ADR 004
  accepted `1c56e9f`) and designed (M016 plan `b55a5e9`); all three M016
  gates are cleared.
- Live fixtures lived in `/tmp` (ephemeral); their outputs are captured in
  the table above, not re-runnable from the repo.
- `09-cleanup.md` group-table rows older than the M015 row still cite
  pre-M015 line numbers (noted in-dive); a full re-derivation pass is
  follow-up work, not claimed here.

## Unresolved findings by severity

- **Low (test-env/toolchain drift, pre-existing):** profile-selector version
  allowlist does not qualify host stable cargo, so one real-Cargo test
  skips where it expects preview. Fail-closed direction (skip, zero spawns);
  product behavior safe. Needs its own corrective (qualify current stable
  with hosted evidence) — explicitly out of M015 scope; M015 neither fixes
  nor masks it (recorded, not reclassified).
- **Low (M015-owned, found and fixed pre-closure):** two M015 tests
  asserted raw-TMPDIR paths and failed on the macOS hosted lane
  (`/var` vs `/private/var`). Fixed in `eb82ad2` by canonicalizing the
  tempdir base; root cause was test-side only (product canonicalizes
  throughout), reproduced on Linux with a symlinked TMPDIR, and the
  post-fix macOS lane confirms. No corrective needed — the milestone was
  still open, and the hosted lane did exactly the job plan §10 required.
- **None medium or higher.** Per the corrective-pass rule, any future
  material defect gets a new plan, not an edit to this record.

## Deferred enforcement (for a future ADR-004-gated corrective, not authorized here)

The removed block withheld a `locate`-stage believed-workspace claimant
from `unresolved` when `believed_workspace_manifest` named a workspace
whose canonical root manifest resolved in-run (old
`src/workspace.rs:976-1003` + test
`misconfigured_member_attributed_to_resolved_parent_does_not_block`). Its
safety sketch — Cargo refused the claimant so no Cargo command places
output for it; any output falls under the named parent's proven target; the
diagnostic stays visible — is plausible but unreviewed and amends ADR 001
C004/C006 in effect. Do not resurrect without ADR cover, negative controls
over the enforcement (not just the report), and hosted-lane evidence.
