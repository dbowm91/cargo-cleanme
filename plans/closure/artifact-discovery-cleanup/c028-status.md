# C028 Status — Stale Learned-Root Availability and Cleanup-Scope Corrective

Plan: `plans/implementation/artifact-discovery-cleanup/c028-stale-learned-root-availability-and-cleanup-scope-corrective.md`

Disposition: **closing** — implementation, regression evidence, documentation,
and local verification complete on this tree. **Not closed**: no release
carries the fix (the plan authorizes none), hosted Linux/macOS/Windows/MSRV
lanes have not run from this tree, and installed-release smoke verification
does not exist. v0.2.3 remains unfixed, as the registry states.

Implementation base: plan baseline `bd89fb1a5f8e1ba5ec5e41ff5586e082c2352149`
(`main`, 2026-10-08); implementation landed as commit `24a9e82` on the
handoff branch `plans/c028-stale-learned-roots-corrective`. Pre-push review
(2026-10-08) additionally removed a vacuous engine assertion, reverted an
unrelated CHANGELOG drive-by, and re-derived every citation the diff shifted
plus every citation the implementation added; pre-existing baseline-stale
citations are inventoried in §6 item 4 rather than repaired.

Date: 2026-10-08

---

## 1. What the corrective changed

A deleted, automatically learned scan location aborted otherwise safe Routine
maintenance: the stale advisory path reached the engine's strict explicit-root
validation and the whole bare `cargo cleanme` failed with exit 2, cleaning
nothing. Cleanup admission is now provenance-aware. Automatic (seed/learned)
roots are classified with `symlink_metadata` and the real `io::ErrorKind`
**before** any collapse: proven `NotFound`/non-directory roots are omitted
from the invocation with a bounded diagnostic, symlinks (never followed) and
unreadable/indeterminate roots block with a typed whole-scope report, and
explicit roots keep strict fatal semantics. The admitted set forms one fresh
combined ownership universe guarded by a late pre-spawn premise recheck;
Routine omission never rewrites state; complete certain Full reconciliation
prunes definitively nonexistent learned directories; `clean --full` consumes
the in-memory generation just proved, never a disk reload.

| File | Change |
|---|---|
| `src/policy.rs` | `RootProvenance`, `AutomaticOmission`, `AutomaticBlock`, `ClassifiedRoots`, `classify_cleanup_roots`, `recheck_admission_premise`, `CleanupCandidates`, `full_cleanup_roots`, `routine_cleanup_candidates`; 5 new unit tests |
| `src/discovery_state.rs` | narrow absence pruning in `reconcile_full` (positively `ENOENT`, no intersecting uncertainty, complete Full only); old-root fixture repaired to use existing dirs; 1 new unit test |
| `src/cleanup.rs` | `AdmissionPremise`, `clean_with_roots_policy_selector_guarded` with pre-proof and pre-spawn rechecks; unguarded entry point delegates with `None`; 1 new unit test |
| `src/main.rs` | `ResolvedCleanup` carries omissions/block/provenance; `admit_automatic`, `run_engine_guarded` (one fresh rebuild-or-block on the admission→validation race), `ScanOutcome` in-memory Full generation; explicit arms classify strict |
| `tests/cli_contract.rs` | `StaleLearnedFixture` (isolated `$HOME`/`$XDG_STATE_HOME`, POSIX Cargo stub, direct + staged-plugin runs); 7 new subprocess cases |
| docs | `CHANGELOG.md` Unreleased entry; `docs/TROUBLESHOOTING.md` 0.2.3 row + workaround section; `docs/USAGE.md` learned-state rules; `README.md` scope sentence; `docs/AUTOMATION.md` `incomplete_discovery` row |
| `architecture/` | `04-policy-and-scope.md` (admission seam, re-derived citations, new test table), `06-discovery-state.md` (absence probe, consumer table), `09-cleanup.md` (entry chain, admission premise, race defence), `13-orchestration.md` (pipelines, exit codes, coverage table), `14-testing-and-verification.md` + `overview.md` (327/325 counts, 39.8%) |

No CLI flag was added or renamed; `generate-docs` regeneration is not
required (clap surface unchanged). No state schema migration. No ADR change:
the absence rule was checked against ADR 002 §§5–6 and fits inside them
(positive absence is new evidence, not a reinterpretation of age).

## 2. Requirement-to-evidence matrix (plan §10)

| # | Acceptance criterion | Evidence |
|---|---|---|
| 1 | Stale worktree no longer returns fatal `InvalidRoot` from bare cleanup; valid roots still decided | `stale_learned_root_is_omitted_and_bare_routine_execute_cleans_the_sibling` (`tests/cli_contract.rs:2378`) direct + staged-plugin: exit 0, `scope=routine`, `cleaned=1`, one Cargo clean, stale path absent from `selected_roots`, `invalid scan root` absent from stderr |
| 2 | Explicit roots strict; auto missing/non-dir omittable; unreadable/symlink never benign | `missing_explicit_roots_stay_fatal_with_zero_cargo_clean_spawns` (`:2468`, exit 2 both forms, zero spawns); `automatic_file_root_is_omitted_and_symlink_root_blocks_without_traversal` (`:2535`, file omitted + sibling cleaned, symlink-to-dir and broken link exit 1 with zero `metadata` calls); `unreadable_automatic_root_blocks_instead_of_omitting` (`:2597`, exit 1, premise-checked skip under privilege); unit `automatic_roots_classify_by_provenance_and_explicit_roots_stay_strict` (`src/policy.rs:783`), `symlinked_automatic_roots_block_and_are_never_followed_or_omitted` (`:831`) |
| 3 | One fresh combined universe; reappearance/deletion cannot bypass completeness | `violated_admission_premise_blocks_with_zero_cargo_spawns` (`src/cleanup.rs:3414`, pre-proof block, zero cleans); `admission_premise_recheck_detects_reappearance_and_disappearance` (`src/policy.rs:868`, incl. surviving-file and vanished-file non-violations); `run_engine_guarded` rebuild-or-block (`src/main.rs:365`) |
| 4 | Routine omission persists nothing; complete certain Full prunes only definitively nonexistent dirs | `complete_certain_full_prunes_only_positively_absent_learned_roots` (`src/discovery_state.rs:660`, absence prunes, existing-but-unobserved retained, uncertainty vetoes, incomplete never publishes); Routine touch branch unchanged (`src/main.rs:889`) |
| 5 | `clean --full` cannot act on an older generation after a failed Full | `full_cleanup_consumes_only_a_fresh_in_memory_generation` (`src/policy.rs:913`, non-zero code and missing generation both refuse); `ScanOutcome::full_state` plumbing (`src/main.rs:727`); no live `clean --full` run exists — see §6 |
| 6 | Human/JSON/log tell the truth; contracts preserved | every new CLI case asserts `operation`/`scope`/`mode` plus one-envelope stdout; `stale_and_blocked_automatic_roots_keep_the_bounded_log_contract` (`:2631`, one bounded ASCII line, no paths, exit 0 vs `status=blocked` exit 1); `all_automatic_roots_missing_is_a_successful_empty_no_op` (`:2508`, exit 0, `units=[]`, zero spawns of any kind); `stale_learned_root_simulate_agrees_with_execute_and_spawns_no_cargo_clean` (`:2434`) |
| 7 | Negative controls + focused fixtures prove the defect and each gate; old proof fixtures green | §5 below; full suite green (§4); C003/C004/C006/C023 fixtures untouched and passing |
| 8 | Ordinary + hosted lanes pass with execution evidence | ordinary lanes pass (§4); hosted lanes **not run** — §6 |

## 3. Verification actually run

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean (after one `cargo fmt --all` pass) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean (after fixing `cloned_ref_to_slice_refs`, one complex-type alias, one dead-code removal) |
| `cargo test --all-targets --all-features` | 372 passed, 0 failed (325 lib + 43 `cli_contract` + 4 `end_to_end`) |
| `python3 scripts/check-fixture-portability.py --self-test` | self test passed |
| `python3 scripts/check-fixture-portability.py` | 36 fixture files clean |
| `python3 scripts/check-doc-citations.py --self-test` | self test passed |
| `python3 scripts/check-doc-citations.py` | 16 documents clean |
| `git diff --check` | clean |
| `scripts/release-check.sh` | **not run** (requires clean tree, `eggpack`, toolchain 1.89; forbidden as routine verification) |
| hosted Linux/macOS/Windows + MSRV | **green** — CI `37801034285`, all 9 jobs success (checks ×3 OS, installers ×3 OS, generated-docs, benchmark, MSRV 1.89); local MSRV 1.89 check + full suite also green (372 passed) |

## 4. Premise-negative evidence

Against the plan baseline (unfixed tree), the exact reported shape was
reproduced with a disposable fixture: `$HOME` holding a seed sibling project,
`discovery-state.json` under an overridden `$XDG_STATE_HOME` learning a
worktree path, the worktree then deleted, and bare
`cargo cleanme --config <cfg> --no-progress --format json --dry-run`:

```text
EXIT: 2
STDERR: cargo-cleanme: invalid scan root /tmp/c028reprowlia8i6s/stale-worktree: No such file or directory (os error 2)
```

The fixed tree on the same shape exits 0, prints
`cargo-cleanme: omitting unavailable learned root …` on stderr, traverses the
surviving root, and reports the same decision in Simulate and Execute.

Every new test states the input that makes it fail: the primary CLI case
fails on the baseline with exit 2 (not exit 0 with `cleaned == 1`); the
symlink cases fail if classification used `is_dir` (which follows the link)
or canonicalized before classifying (which resolves it into its target); the
absence-pruning case fails if pruning keyed on age instead of `ENOENT`; the
guard case fails if the recheck is removed (simulation proceeds); the Full
case fails if the code reads the disk generation instead of the in-memory
one. Two of these were observed failing during implementation (the canonicalization
ordering and the file-omission recheck), and both failures changed the design:
see §7.

## 5. Commands run, including failures

- Baseline reproduction (exit 2, quoted above) — before any production change.
- `cargo test --lib -- policy discovery_state` — one failure:
  `zero_retention_disables_expiration_and_nested_roots_collapse` used
  fictitious `/work` paths that absence pruning now removes. Repaired by
  giving the retention half real directories; absence gets its own test.
  This was the plan working as intended: the old test's premise (nonexistent
  paths retained by age) contradicts the new specified behaviour.
- `cargo test --test cli_contract` — two failures, both real design bugs
  found by the new tests (§7), repaired in production code, not in the tests.
- Full ladder (§3) — green after the repairs above; clippy required three
  fix classes (listed in §3).

## 6. Known limitations and what still blocks `closed`

1. **No live `clean --full` subprocess run exists.** A Full cleanup walks the
   platform roots, which is unsuitable for a hermetic test. The Full-only
   properties are covered one level down: generation freshness
   (`src/policy.rs:913`), absence pruning under complete/certain Full
   (`src/discovery_state.rs:660`), and the shared `admit_automatic` path the
   live Routine cases exercise. A hosted full-machine run remains the only
   way to observe the composed path.
2. **Hosted lanes ran green — after catching a real defect.** The first push's
   macOS lane failed one new unit test (`cleanup_candidates_...`, `blocked`
   2 vs 1): on case-insensitive APFS the seed candidates `Projects` and
   `projects` resolve to the same directory, so the symlinked seed blocked
   twice and the CLI fixture would have admitted a duplicate root. Local
   Linux could not see it. Fixed in `a25b4e8` by using the case-twin-free
   seed name `Developer` in the unit test and the `StaleLearnedFixture`
   (with the rationale at the call site), re-verified locally (43 CLI + 325
   lib), and re-pushed to full green. The unreadable-root CLI case runs (not
   skipped) on the macOS lane: runners are non-root, so the premise holds.
3. **No release carries the fix.** Publication, installed-command smoke, and
   the five-target automatic smoke are future release work under the
   distribution subsystem, not this plan.
4. **Pre-existing doc drift observed but out of scope.** `04-policy-and-scope.md`
   §7.8/§7.9 still describe pre-M012A behaviour (configured-root scope label,
   `clean --known` empty-set interaction) that M012A already fixed in code.
   Untouched because the plan scopes doc edits to affected sections; flagged
   here so the next reader does not mistake those paragraphs for current
   behaviour. The same review pass verified by baseline (`bd89fb1a`) that the
   deep dives' older `main.rs`/`policy.rs`/`cleanup.rs` line citations were
   already stale before C028 — e.g. `06-discovery-state.md`'s `main.rs:383-386`
   / `:402-406` / `:448-452` / `:479-483` / `:590-592` cluster,
   `09-cleanup.md`'s `main.rs:100-110` CleanMode mapping (the mapping lives in
   `cli.rs:224`), and `04-policy-and-scope.md`'s `main.rs:174-194` /
   `main.rs:510-522` scope-label cites — and left them for a C026-class
   reconciliation corrective, which is the repository's established pattern
   for citation sweeps (C026 re-derived 20 citations with no Rust changed).
   Every citation C028 added or rewrote was re-derived from the post-change
   source instead, including the previously-correct cites this diff shifted
   (`09-cleanup.md`'s exit-code sites, `06` §10's retention/version/publish
   review checklist, `04` §2's policy anatomy table, `13-orchestration.md`'s
   §8/`§10`-item-2).

## 7. Deliberate judgement calls

1. **Classification before collapse, with raw candidates.** The first
   implementation classified `policy::resolve`'s canonicalized Routine set
   and the symlink CLI case passed vacuously — the link had already become
   its target. The failing CLI test forced `routine_cleanup_candidates`
   (`src/policy.rs:365`), which carries raw paths. Scan policy (`resolve`)
   is unchanged; only cleanup admission uses the raw seam.
2. **Omission-kind-aware recheck.** The first recheck treated any existing
   omitted path as a violation, which blocked a run whose omitted path was a
   surviving regular file — the premise ("no tree here") still held. The
   premise now carries the omission kind: `NotFound` tolerates nothing,
   `NonDirectory` tolerates a surviving or vanished non-directory.
3. **Full generation carried in memory, not re-read.** The old code reloaded
   state from disk after the Full scan, so a failed save silently substituted
   the older generation. `ScanOutcome::full_state` makes the fresh generation
   the only input; `full_cleanup_roots` was extracted into `policy.rs` so the
   refusal is unit-testable without a machine-wide walk.
4. **No new reason codes or schema version.** The symlink/uncertain block
   reuses `ScopeBlockReason::IncompleteDiscovery` and the absence probe adds
   no JSON field, so the v1 envelope and log contract are unchanged.
5. **Symlinked seeds block rather than omit.** Safe omission of a symlink
   that points at a real directory cannot be proven (the tree exists, the
   walker would never descend), so the run blocks. The symlinked-seed case is
   pinned at `src/policy.rs:943`.

## 8. Residual risks

- A symlink or unreadable automatic root now blocks the whole automatic scope
  (exit 1) where the baseline errored (exit 2). Operators who scripted on
  exit 2 for *any* root problem will see a different code for the automatic
  subset; explicit roots still exit 2. Documented in `docs/AUTOMATION.md`.
- Absence pruning keys on `symlink_metadata` `ENOENT` at reconciliation
  time. A root on a transiently unmounted volume reads as absent and is
  pruned; it is relearned by the next Full scan that observes it. This matches
  the plan's "positively confirmed nonexistent" rule and ADR 002's
  rediscoverability, but it is a behaviour change for flaky mounts.
- The admission→validation race rebuild runs the engine twice in the worst
  case (first attempt fails strict validation, second runs the rebuilt set).
  Both attempts are full fresh runs; no partial proof is reused.
