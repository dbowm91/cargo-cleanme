# Artifact Discovery and Cleanup M012A Status

Plan: `plans/implementation/artifact-discovery-cleanup/012a-canonical-maintenance-cli-and-dry-run-semantics.md`

Decision: `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Disposition: **closed** — implementation, contract, documentation, and
verification evidence complete. The breaking invocation change is unreleased; see
§8 for what that does and does not block.

Repository baseline at implementation: `d6ea491` (Phase 12 planning baseline
`e4e9d92673e5f10548248e23db788c0906760916`)

Date: 2026-10-05

---

## 1. What the milestone changed

ADR 003 moved the front door without weakening the proof boundary. Before, a
bare `cargo cleanme` fell through `match cli.command { None => run_scan(None,
false, …) }` — one line decided the meaning of the front door. After, argv
collapses into exactly one `Invocation` value and `main.rs` dispatches on that
value alone.

| | 0.1.x | Now |
|---|---|---|
| `cargo cleanme` | Routine read-only scan | **Routine Execute cleanup** |
| `cargo cleanme --dry-run` | usage error | Routine Simulate, zero `cargo clean` |
| `cargo cleanme scan` | Routine read-only scan | **Full reconciliation** |
| `cargo cleanme clean R --dry-run` | Cargo preview | Simulate |
| `cargo cleanme clean R --dryrun` | Simulate | Simulate (`--dry-run`) |
| `cargo cleanme clean R --yes` | Execute | Execute (default) |
| `cargo cleanme clean R --cargo-preview` | usage error | Cargo preview |
| `cargo cleanme clean` (no selector) | exit 2 | Routine maintenance cleanup |
| `cargo cleanme scan --full` | Full | hidden alias for `scan` |

## 2. The structural decisions

### One internal operation model, not two code paths

`cli.rs` gained `CleanupScope`, `ScanIntent`, `CleanupOverrides`,
`CleanupRequest`, `Invocation`, `Cli::invocation`, and `clean_mode`. Bare
invocation and `clean` with no selector produce the **same**
`CleanupScope::Maintenance` value and reach the same `run_cleanup`. That is the
milestone's central claim made mechanical: they cannot drift apart in root
selection, policy, proof, or reporting, because there is one cleanup engine
behind both.

### One mode, one definition site

Three parallel mode taxonomies (two CLI booleans plus an enum, mapped in
`main.rs`) collapsed into `cleanup::CleanMode` plus a single mapping function.
`CleanMode` lost its `Default` and `CleanReport` lost its derived `Default`, so
no future call site can silently inherit the weaker mode: every report states
its mode in full or does not compile.

The cost is real and stated in `architecture/02-cli.md` §1: `cli.rs` no longer
imports nothing but clap, so it cannot be reviewed in isolation from
`cleanup.rs`. The dependency is one-way.

### Rootless `scan` is Full, by construction

`cli.rs` cannot consult `scan.root` — it maps a rootless `scan` to
`ScanIntent::Full` and nothing more. `policy::resolve` returns `ScanScope::Global`
before configuration is read, so configuration cannot narrow the canonical
reconciliation command even by accident.

## 3. Requirement-to-evidence matrix

| # | Requirement | Evidence |
|---|---|---|
| 1 | Bare `cargo cleanme` reaches Routine cleanup Execute, not scan | `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` (`tests/cli_contract.rs:856`) — `operation == "clean"`, `result.units` is an array, exactly one `clean` call, artifact deleted |
| 2 | Bare `--dry-run` is Simulate with zero `cargo clean` processes | `bare_dry_run_simulates_with_zero_cargo_clean_processes` (`:893`) — `mode=simulate`, zero `clean`, `metadata` called, artifact intact |
| 3 | Bare cleanup with no known roots is a successful no-op | `bare_cleanup_with_no_known_roots_is_a_successful_no_op` (`:935`) — exit 0, `scope=routine`, `units == []`; platform-neutral counterpart `json_scan_within_an_empty_root_is_a_successful_zero_result_report` (`:1155`) |
| 4 | Bare cleanup with unresolved ownership blocks, exit 1, zero Cargo clean | `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` (`:969`) |
| 5 | Rootless `scan` is Full and ignores `scan.root` | `scan_scope_selection_is_full_explicit_or_maintenance` (`src/cli.rs:432`) plus `full_resolution_ignores_a_configured_root` (`src/policy.rs:326`). **Argument, not observation — see §6** |
| 6 | `scan ROOT` stays Explicit | `scan_scope_selection_is_full_explicit_or_maintenance`; `log_mode_scan_reports_the_resolved_scope` (`:1189`) at process level |
| 7 | `scan --known` is the read-only Routine inventory | `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` (`:1098`) — `scope=routine` **and** `discovered_manifests == 1`, so the label is not vacuously right |
| 8 | Scope conflicts fail at invocation time | `scan_scope_conflicts_are_rejected_before_any_traversal` (`:268`) — exit 2, empty stdout, clap's conflict message; `clean_scope_selectors_stay_mutually_exclusive` (`src/cli.rs:482`) |
| 9 | `clean` defaults to Execute; preview/simulate reachable and distinct | `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` (`:1002`) — Execute deletes, Cargo preview spawns Cargo with `--dry-run` and deletes nothing, simulation adds no third invocation |
| 10 | Aliases map exactly to canonical modes, print nothing | `cleanup_modes_default_to_execute_and_expose_cargo_preview` (`src/cli.rs:517`), `compatibility_aliases_are_hidden_but_accepted` (`src/cli.rs:570`), `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` (`:1060`) |
| 11 | Direct and external argv equivalent | `direct_and_external_forms_parse_equivalently` (`src/cli.rs:781`) compares `invocation()`, not `Debug`; process-level `cargo_external_subcommand_help_matches_direct_help` |
| 12 | JSON names the resolved operation/scope/mode | every M012A integration case asserts all three; `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` (`:219`) |
| 13 | Selector and min-size policies unchanged | `json_unattended_yes_executes_through_cargo_and_emits_typed_result` (`:379`) — unchanged and still passing |
| 14 | Ownership/atomicity/final-proof regressions stay green | the full `cleanup.rs` suite (71 tests) and the C003/C004/C006 property tests; none modified |
| 15 | `--dry-run` semantics, not merely spelling, reach the implementation | `assert_eq!(fixture.clean_calls(), 0, …)` in case 2 — the assertion is about spawned processes, not a JSON label |
| 16 | Breaking change crosses a minor pre-1.0 boundary | `Cargo.toml` `version = "0.2.0"`; `CHANGELOG.md` has a `Changed — BREAKING` section and a 0.1.x→0.2.0 mapping table; `README.md` and `docs/TROUBLESHOOTING.md` carry the same table |
| 17 | Generated artifacts regenerated | `cargo run --features dev-tools --bin generate-docs`; `-- --check` reports `13 artifacts match the clap model` |
| 18 | Premise-negative: the tests fail for the old behaviour | §5 below |

## 4. Verification actually run

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-targets --all-features` | 285 lib + 30 `cli_contract` + 2 `end_to_end`, 0 failed |
| `python3 scripts/check-fixture-portability.py --self-test` | self test passed |
| `python3 scripts/check-fixture-portability.py` | 35 fixture files clean |
| `python3 scripts/check-doc-citations.py --self-test` | self test passed |
| `python3 scripts/check-doc-citations.py` | 16 documents clean |
| `cargo run --features dev-tools --bin generate-docs -- --check` | 13 artifacts match |

`scripts/release-check.sh` was **not** run: it requires a clean tree, the
`eggpack` binary, and toolchain 1.89, and AGENTS.md §3 forbids running it as
routine verification.

## 5. Premise-negative evidence

Requirement 18 is the one that cannot be discharged by assertion. The pre-M012A
dispatch was temporarily restored in `src/main.rs` — routing
`CleanupScope::Maintenance` to `run_scan(ScanIntent::Maintenance, …)` — and the
suite was re-run. Four tests failed, with the discriminating failure:

```
---- bare_invocation_executes_routine_cleanup_and_is_not_a_scan stdout ----
assertion `left == right` failed: bare invocation must reach cleanup, not scan:
{"operation":"scan","result":{"groups":[{"bytes":16384,…}],"summary":{"group_count":1,…}},…}
  left: String("scan")
 right: "clean"
```

- `bare_invocation_executes_routine_cleanup_and_is_not_a_scan`
- `bare_dry_run_simulates_with_zero_cargo_clean_processes`
- `bare_cleanup_with_no_known_roots_is_a_successful_no_op`
- `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean`

All **271** lib tests still passed under the reverted dispatch (that was the
lib count at the time of the demonstration, before M012B added `output.rs`
tests). That is worth stating plainly: the lib suite does **not** detect this
regression, because the regression lives in `main.rs`, which has no inline
tests at all. Only the process-level cases do — which is an argument for
`tests/cli_contract.rs` existing, not evidence that the lib suite is sufficient.

## 6. Known limitations

1. **No hosted integration case runs a rootless `scan`.** It walks the whole
   filesystem; every case in the suite is bounded on purpose. Requirement 5 is
   discharged by an argument at the two places the decision is made
   (`policy::resolve` returns `Global` before reading config; `Cli::invocation`
   never supplies a root for the Full shape), not by observation. A reader
   should weigh it as an argument.
2. **The maintenance-scope tests pin the variant, not the root list.** A learned
   root surviving retention is covered by `discovery_state`'s own tests, not
   through the front door.
3. **`clean --full` is not exercised at any format**, so the single-document
   invariant is pinned only indirectly (§7). A cheap end-to-end case would need a
   way to bound a Full scan; that is a design question for a future plan, not a
   defect in this one.

## 7. Deliberate judgement calls

**A configured `scan.root` now actually gets cleaned.** `clean --known`
discarded the resolved `ScanScope::Explicit(root)` and ran with no roots,
reporting a successful zero-result cleanup of nothing. M012A makes bare
invocation destructive, so a silent no-op for anyone with `scan.root` set is
untenable: it would report `status=ok … cleaned=0` for work that was never
attempted. The fix is in the plan's own invariant ("a configured legacy
`scan.root` remains an exclusive Explicit override"), and it now runs the same
proof path over one root that `clean ROOT` runs.

**`clean --full` with an incomplete scan now exits 2 instead of returning the
scan's exit code silently.** The old behaviour gave a caller a bare non-zero
code with no message, in a position where no cleanup had run.

**`emit_output` was deleted and had to be restored.** M012A removed the scan
report's `emit_output: bool`, reasoning that it was only ever passed `false`
once. That made `clean --full` emit the intervening Full scan's report ahead of
the cleanup report — and in `--format json`, **two JSON documents on one stream**,
breaking the one-document-per-invocation property the automation contract rests
on. The flag is back as a named `EmitReport` enum parameter
(`src/main.rs:432-435`) with the reason documented at the definition.

**This regression was found by reading `architecture/13-orchestration.md` during
M012B's documentation pass, not by a test.** No case runs `clean --full` at any
format, because that is a full filesystem walk. `scan_emits_exactly_one_json_document`
(`tests/cli_contract.rs`) now pins the single-document property for a scan and
asserts the guard exists in the source — weaker than an end-to-end case, and
recorded as such in §6. The lesson is the repository's own: a flag that exists
for exactly one call site reads as dead code, and deleting it reads as
simplification.

**JSON was not extended with the typed block reason.** M012B needed a stable
code for the log line, so `ScopeBlockReason` and `ScopeBlock` were introduced
in `cleanup.rs`. JSON schema v1 keeps the prose `scope_reason` and does **not**
gain a code field: adding one would redefine v1, which the plan explicitly
forbids. The typed code is the only reason taxonomy in the crate; only the log
surface exposes it today. Exposing it in JSON is a future schema decision, not
an accident.

**Cleanup `scope` value `"known"` became `"routine"`/`"explicit"`.** No field
changed meaning and none was added or removed, so `schema_version` stays 1. The
old value described argv; the new one describes the resolved scope, matching
what the scan envelope already did.

## 8. Unresolved findings, by severity

| Severity | Finding | Disposition |
|---|---|---|
| none blocking | The breaking change is unreleased. `Cargo.toml` is `0.2.0`; no release has been cut. | Publication is a human action (`docs/RELEASING.md`). The 0.1.6 → 0.2.0 boundary is now structural, not a promise: a `0.1.x` patch cannot contain this commit. |
| low, recorded | `output.rs` JSON exposes the prose block reason only, while log mode exposes the typed code. | Deliberate (§7). No corrective plan; a schema change needs its own plan. |
| low, recorded | No process-level case exercises a rootless Full `scan`. | Deliberate (§6.1). Not a defect; a bounded suite cannot. |

## 9. Correctives

None. No material defect was found in this milestone's implementation. The two
behaviour corrections in §7 were found *during* implementation, before closure,
and are part of this milestone rather than correctives to it.