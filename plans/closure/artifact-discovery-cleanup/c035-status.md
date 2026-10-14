# C035 — Operator-Declared Independent Maintenance Scopes (Route B)

Status: **implemented on branch `plans/c035-operator-scoped-cleanup`; publication
pending — no release authorized by C035.** Baseline `6bad266` (`main`, v0.2.6,
2026-10-14). Work is committed on no commit yet: the branch holds the working
tree described here, uncommitted at the time of writing; the SHA below is the
baseline, not an implementation commit.

Plan: `plans/implementation/artifact-discovery-cleanup/c035-operator-declared-independent-maintenance-scopes-corrective.md`
(status `ready`, owner-directed 2026-10-14).
Accepts ADR 004 **in part** (operator-declared scopes; automatic certificates
stay rejected per C031-D) with the ADR 001 C035 clarification. C031's negative
result is not reopened; no closed plan or closure record was edited.

## What changed

- `src/config.rs` + `config.toml`: additive optional `[maintenance] scopes`
  (exact absolute dirs, absoluteness at load, strictness at admission).
- `src/policy.rs`: `declared_scope_partitions` — Explicit-strict validation,
  canonicalization, containment-merge, deterministic sort.
- `src/cleanup.rs`: `clean_with_declared_scopes` (combined discovery,
  resolution, and grouping; per-scope proof via the extracted shared tail
  `run_cleanup_units`; merge in `merge_scope_outcome`; legacy-combined
  fallback kill-switch), `ScopeOutcome` nesting under `CleanReport.scopes`,
  per-scope human sections in `render`.
- `src/domain.rs`: `ScanCounters::saturating_add_assign` for merged counters.
- `src/main.rs`: `ResolvedCleanup.declared_scopes`, Maintenance-arm dispatch
  (`scan.root` still wins with a warning), per-scope engine call.
- `src/output.rs`: additive `scopes: Vec<CleanupScopeV1>` with
  `skip_serializing_if` (undeclared JSON byte-identical); shared `summarize`.
- `tests/common/mod.rs`: stub answers each queried manifest from its own
  directory, honoring `CARGO_TARGET_DIR`; FIXTURE_ROOT queries byte-identical.
- `tests/cli_contract.rs`: 9 C035 subprocess cases + fixture.
- Docs: `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `architecture/04, 09, 10,
  13, 14`, `architecture/overview.md` counts; ADR 001 clarification + ADR 004
  accepted-in-part (filename keeps `proposed` to avoid breaking references).

## Requirement-to-evidence matrix (plan §6)

| # | Requirement | Evidence | Result |
|---|---|---|---|
| 1 | Partial clean: healthy scope cleans (real argv, bytes), poisoned scope typed block, exit 1 | `declared_scopes_clean_the_healthy_scope_and_block_only_the_poisoned_scope` (direct + staged-plugin spellings): cleaned 1, `scope_b` block names its scope, 1 clean call, artifact A gone / B present, unresolved stage `locate` | Pass (macOS local) |
| 2 | Simulation parity: same split, zero spawns, no mutation | `declared_scopes_simulate_the_same_split_with_zero_spawns`: simulated 1, 0 clean calls, artifact present | Pass |
| 3 | Cross-scope redirect: output outside own root is `ExternalUnproven`, nothing spawns | `redirected_output_is_external_unproven_and_spawns_nothing`: `external-unproven` / `skipped_ownership_unproven`, exit 0, 0 clean calls, both artifacts present | Pass |
| 4 | Overlap: shared output skips both sides, never `PrivateBounded` | `shared_output_across_scopes_skips_both_sides`: both `shared` / `skipped_shared`, exit 0, 0 clean calls | Pass, and **discriminating** (see below) |
| 5 | Late appearance / config mutation re-blocks the scope | Mechanism-shared: per-scope H-2 rediscovery over scope roots and per-scope hoisted-universe refresh reuse the unchanged `run_cleanup_units` gates; scope attribution covered by `scope_attribution_is_first_containing_scope_or_none`. No deterministic mid-run trigger test exists — same as legacy H-2, which also has none | Partial (mechanism shared + reviewed; no trigger test) |
| 6 | Declaration hygiene: merge, fatal defects (exit 2), undeclared identity | `declared_scopes_merge_overlap_and_sort_disjoint`, `declared_scope_defects_are_fatal_not_skipped`, `declared_symlink_scopes_are_fatal` (unit) + `bad_scope_declarations_are_fatal_usage_errors`, `configured_scan_root_overrides_declared_scopes_with_a_warning` (subprocess); v0.2.6-config load test | Pass |
| 7 | Contract: additive JSON, log bound, exit pins, human/JSON parity | `undeclared_runs_emit_no_scopes_key` (no `scopes` key undeclared), `multi_scope_log_line_is_one_bounded_blocked_line` (`status=blocked reason=ownership_unproven`, ≤384 B), `multi_scope_human_report_groups_by_scope`; exit pins in items 1–4/6 | Pass |
| 8 | Heterogeneity shape with real Cargo | Items 1–4 run direct + plugin spellings under the stub; **plus** a live real-Cargo run on macOS (`cargo new` project + garbage-manifest scope, debug binary): Simulate predicted 1 would-clean + 1 scoped block, Execute reclaimed the real `target/` while the poison scope stood untouched, JSON `scope=routine mode=execute` with per-scope entries, exit 1. Hosted lanes: **CI green** (run `38513116448`, all 9 jobs: ubuntu/macos/windows checks, MSRV 1.89, generated-docs, benchmark, 3 installer lanes) | Pass locally and hosted |

## Discrimination evidence (what makes the load-bearing test fail)

With grouping temporarily restricted to one scope's workspaces
(`build_groups(&workspaces_all[..1])`, reverted after),
`shared_output_across_scopes_skips_both_sides` fails with the first unit
reported `"ownership":"private", "outcome":"cleaned"` — the scope's shared
output received a real `cargo clean` — instead of `shared`/`skipped_shared`.
That is exactly the Route B hazard the combined grouping exists to prevent,
and the test guards it. Pre-C035 `main` also fails the new cases (the
`[maintenance]` section is rejected by `deny_unknown_fields`).

## Verification actually run

2026-10-14, macOS (darwin), rustc 1.89.0 (MSRV toolchain), branch
`plans/c035-operator-scoped-cleanup` on baseline `6bad266`:

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — 367 lib + 60 cli_contract
  (51 pre-existing + 9 new) + 4 end_to_end, 0 failures.
- `python3 scripts/check-fixture-portability.py --self-test` + checker —
  passed, 36 fixture files clean.
- `python3 scripts/check-doc-citations.py --self-test` + checker — passed,
  16 documents clean.
- `cargo run --quiet --features dev-tools --bin generate-docs -- --check` —
  13 artifacts match (no CLI surface change, as designed).
- `git diff --check` — passed.
- Hosted CI on branch `plans/c035-operator-scoped-cleanup`: first run
  (`38512575806`) went red on **one** case, `checks (windows-latest)` —
  `config::tests::maintenance_scopes_require_absolute_paths` interpolated a
  backslash path raw into TOML, which does not parse (`\U` reads as a unicode
  escape). Fixed in commit `832a9ee` (spell through `toml_string`, plus a
  lane-independent round-trip pin); re-run (`38513116448`) green on all 9
  jobs. The Cargo selector qualification workflow passed on the branch as
  well (`38512576013`). A later docs-only tip run (`38513464752`) first
  failed `json_update_refusal_emits_no_document_and_exits_non_zero` on ubuntu
  with `ETXTBSY` at spawn — the known staging flake documented at
  `tests/cli_contract.rs:stage_runnable_binary`, unrelated to C035 (no C035
  code in that path; prior run green on identical test code); `--failed`
  re-run green on all jobs.

## Stop-condition compliance

None of the plan §7 stop conditions fired: no auto-narrowing after combined
failure (fallback re-runs the *combined* scope, never a subset); no promotion
of `Shared`/`Uncertain`/`ExternalUnproven` (all stay per-unit skips via the
unchanged gate); no unresolved manifest dropped (attributed per scope, merged
into the top-level list); no schema drift (additive `scopes` key only,
`skip_serializing_if`); no direct filesystem deletion (one atomic
workspace-unit `cargo clean` only); no `sudo`; permission-denied still blocks;
every new test names its failing input (see discrimination row); all fixture
mutation under disposable temp dirs.

Two plan-text corrections made during implementation, recorded in plan §8:
hygiene failure is exit 2, not 17 (no exit 17 exists in the 0/1/2 contract);
no dedicated kill-switch flag (operator rollback is deleting the section).

## Known limitations (plan §7, unchanged)

- An unresolvable manifest's unknown output is the irreducible residual,
  carried by the operator's declaration.
- Poison *inside* a declared scope still blocks that scope, by design.
- In-scope exclusions rejected for v1 (excluding a manifest from discovery
  cannot exclude it from owning bytes).
- Human output prints both the legacy `combined roots` union line and the new
  `declared scopes` summary line; intentional (roots walked vs boundaries
  proven), not drift.
- Item 5 has no deterministic trigger test (same standing as legacy H-2).
- Hosted OS/MSRV evidence complete via CI (run `38513116448`).

## Observed documentation debt (pre-existing, not caused by C035)

Re-deriving C035 citations surfaced rot the C032–C034 line left behind (all
verified against baseline `6bad266`, where the cited lines already did not
match): e.g. `architecture/09-cleanup.md` cited the guarded engine at
`cleanup.rs:829` (a parameter line at baseline) and `resolve_cleanup_scope`
at `:1281-1330` (a unit-loop region); `architecture/13-orchestration.md`
cited `main.rs:365/457/639`; `architecture/04-policy-and-scope.md` cited
`policy.rs:437` for `routine_cleanup_candidates` (at `:465` at baseline).
C035 fixed the cites in every section it added or rewrote and every table row
covering code it changed, all re-derived from source — not shifted
arithmetically. The untouched sections keep their old numbers; a follow-up
corrective should re-derive the remainder the same way.
