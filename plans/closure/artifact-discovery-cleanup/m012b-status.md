# Artifact Discovery and Cleanup M012B Status

Plan: `plans/implementation/artifact-discovery-cleanup/012b-unattended-log-output-and-greggd-integration.md`

Decision: `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Disposition: **closed** — implementation, contract, documentation, and
verification evidence complete. One optional external rehearsal was not
performed; §8 records why that is not a blocker.

Repository baseline at implementation: `d6ea491` (Phase 12 planning baseline
`e4e9d92673e5f10548248e23db788c0906760916`)

Date: 2026-10-05

---

## 1. What the milestone added

`--format log` emits **one ASCII line on stdout** per scan or cleanup report:

```text
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0
cargo-cleanme op=scan status=ok scope=full manifests=184 groups=31 bytes=51335569408 diagnostics=2
cargo-cleanme op=clean status=blocked scope=routine mode=execute cleaned=0 skipped=0 failed=0 reason=ownership_unproven diagnostics=1
```

The renderer lives in `output::log` (`src/output.rs:274`), projects only state
that already exists in `ScanReport`/`CleanReport`, and shares no serialization
path with the JSON envelope.

## 2. The design decisions worth defending

### The bound is enforced by dropping a field, not by truncating

`output::log::line` (`src/output.rs:397`) emits structural fields
unconditionally, then appends optional fields while the result fits and **drops
the field whole** once it does not. Nothing is ever cut in half, so ASCII
integrity is a property of the construction rather than a check that could be
forgotten. A `debug_assert!` covers the structural head against the worst case.

The alternative — truncate at 384 bytes — would produce a line whose last field
is a lie. `an_overlong_optional_value_is_dropped_whole_and_the_head_survives`
feeds a 4 KiB reason source and asserts the structural fields survive, the
flood does not appear at all, and no trailing optional field sneaks in.

### `reclaimed_bytes` is printed only when bytes were actually recovered

A simulation and a Cargo preview both remove nothing. Printing
`reclaimed_bytes=0` for either would be a measured claim about work that did not
happen. The field is emitted only for an executed, unblocked cleanup.

A blocked scope is excluded too. `a_blocked_scope_never_reports_reclaimed_bytes`
pins it: a run refused before mutation has no measurement to report, and
`reclaimed_bytes=0` would read as "we checked and there was nothing", which is
not what happened.

### Status is derived from typed state, never from prose

`ScopeBlockReason` and `ScopeBlock` were introduced in `cleanup.rs` so the log
line can carry `reason=ownership_unproven` instead of matching English.
`cleanup_line_reports_a_typed_reason_when_blocked` asserts the prose never
reaches the line. This is the plan's "minimum typed reason" requirement, and it
is the crate's only reason taxonomy.

### The line carries no paths and no Cargo stderr

A retained history pane is a convenience, not a directory listing. Log mode
emits counts and typed codes only. `log_mode_failed_cleanup_…` asserts Cargo's
own error text does not escape onto stdout.

### Fatal errors get one bounded line, not prose

`main()` now parses argv before calling `run(cli)` so a failure raised before any
report exists knows the format. In log mode that is
`op=<operation> status=error reason=<code>` on stderr with **empty stdout** —
there is no report to summarise, and writing a summary line would be inventing
one.

## 3. Requirement-to-evidence matrix

| # | Requirement | Evidence |
|---|---|---|
| 1 | `--format log` parses globally | `format_log_parses_globally` (`src/cli.rs`) — five argv shapes incl. the external form; also asserts `Human` remains the default, so a non-TTY never selects log |
| 2 | Routine Execute summary: one line, ASCII, deterministic, ≤384 bytes | `log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr` (`tests/cli_contract.rs:1258`) with `assert_bounded_log_line`, which re-checks ASCII, length, single newline, prefix, no `\r`/ESC, and key shape on every case |
| 3 | Simulate reports `mode=simulate` and zero Cargo-clean spawns | `log_mode_distinguishes_simulate_from_cargo_preview` — `fixture.clean_calls() == 0` |
| 4 | CargoPreview reports `mode=preview` without claiming reclaimed bytes | same test — `mode=preview`, no `reclaimed_bytes`, artifact still present, `--dry-run` present in the stub's argv |
| 5 | Full scan reports `scope=full` | `scan_line_reports_failed_for_an_incomplete_full_scan` and `scan_line_is_one_bounded_ascii_line` (`src/output.rs`) assert the exact literal line. **Unit-level only — see §6** |
| 6 | Explicit scan reports `scope=explicit` | `log_mode_scan_reports_the_resolved_scope` — process level, both `scan ROOT` and a configured root |
| 7 | `scan --known` reports `scope=routine` | same test, and it also asserts `manifests=1 groups=1` so the label cannot be right for the wrong reason |
| 8 | Blocked cleanup: exit 1, one stdout line, typed reason | `log_mode_scope_block_is_one_line_with_a_typed_reason_and_exit_one` — also asserts stderr stays empty, i.e. a blocked report is still a report |
| 9 | Failed cleanup: exit 1, `status=failed` | `log_mode_failed_cleanup_is_one_line_with_status_failed_and_exit_one`, driven by a real Cargo stub failure (`CARGO_FAIL_CLEAN`) rather than a hand-built failure |
| 10 | Successful zero-result maintenance is `status=ok` | `log_mode_zero_result_maintenance_is_status_ok` |
| 11 | No progress/control sequences in log mode | `assert_bounded_log_line` rejects `\r` and ESC on **every** log case, and progress is disabled by the `format == Human` gate. **No attended-TTY case is exercised — see §6** |
| 12 | Diagnostic fan-out suppressed | `log_mode_suppresses_normal_diagnostic_fan_out` — and it first proves the *human* path does fan out, so the case is discriminating in both directions |
| 13 | `--stats` stays opt-in stderr and does not alter the line | `log_mode_stats_is_opt_in_stderr_and_leaves_the_single_line_alone` — byte-identical stdout with and without |
| 14 | JSON unchanged | `json_and_human_output_are_unchanged_by_log_mode` — parses the JSON envelope and checks the human report keeps its own shape |
| 15 | Human unchanged | same test |
| 16 | Synthetic worst case proves the bound without cutting structure | `the_structural_head_fits_at_the_worst_case` (`src/output.rs`) — `u64::MAX` in every numeric field |
| 17 | Premise-negative: an overlong reason source stays bounded | `an_overlong_optional_value_is_dropped_whole_and_the_head_survives` — 4 KiB flood, distinctive character, asserts omission not truncation |

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
| `cargo run --features dev-tools --bin generate-docs` | 13 artifacts regenerated |
| `cargo run --features dev-tools --bin generate-docs -- --check` | 13 artifacts match |

### Manual exercise with byte counts

Run against a temporary config and fixture, as the plan's §12 requires:

```text
$ cargo cleanme --no-progress --format log            # bare Routine Execute
exit=0
stdout(122 bytes): cargo-cleanme op=clean status=ok scope=explicit mode=execute cleaned=0 skipped=1 failed=0 reclaimed_bytes=0 diagnostics=0
stderr(0 bytes):

$ cargo cleanme --no-progress --format log --dry-run   # bare Routine Simulate
cargo-cleanme op=clean status=ok scope=explicit mode=simulate cleaned=0 skipped=1 failed=0 diagnostics=0
exit=0

$ cargo cleanme --no-progress --format log scan --known
cargo-cleanme op=scan status=ok scope=explicit manifests=1 groups=1 bytes=8192 diagnostics=0
exit=0

$ cargo cleanme --config ./broken.toml --no-progress --format log   # fatal
cargo-cleanme op=clean status=error reason=config
exit=2
```

Note the `reclaimed_bytes` field present in the Execute line and absent from
the Simulate line — the property case 4 asserts, visible in real output.

## 5. Documentation delivered

| Document | Content |
|---|---|
| `docs/AUTOMATION.md` (new) | the log field list and guarantees; a worked greggd configuration with every required caveat; cron and systemd equivalents; what an unattended run will not do; exit-code triage; the two typed block reasons |
| `README.md` | quickstart pointer, format table row, documentation table row |
| `docs/USAGE.md` | an output-format table under the JSON section, with "parse JSON, not log" stated plainly |
| `docs/TROUBLESHOOTING.md` | a scheduler section: empty scope, `status=blocked`, exit 2, and a missing field |
| `plans/output-schema-v1.md` | log is explicitly *not* part of the JSON contract |
| `CHANGELOG.md` | the added format and the typed block reason |

The greggd integration prose covers every item the plan requires: absolute path,
argv-not-shell, daily Routine vs weekly Full, Full is read-only but expensive,
load gating/retry/max-wait belong to greggd, the system service's `ProtectHome`
problem and the rootless recommendation, bounded in-memory history, no
credentials in argv, and no authenticity claim. No Gregg internal beyond what
the integration needs is reproduced.

## 6. Known limitations

1. **Requirements 5 and 11 are not exercised against a real Full scan or an
   attended TTY.** A rootless `scan` walks the whole filesystem; an attended
   TTY needs a pty. Both are covered at the level where the behaviour is
   decided — `assert_bounded_log_line` rejects control characters on every
   process-level log case, and `show` is `format == Human && …`, so the renderer
   is a no-op for `Log` by construction. A reader should treat this as an
   argument about the code path, not an observation of a scheduled run.
2. **No greggd rehearsal was performed.** The plan makes it optional and says
   closure must not depend on an unavailable external binary. No greggd is
   installed here, and the retained-history claim is therefore derived from the
   format's 384-byte bound, not observed.
3. **`output.rs` gained 11 inline tests**, so it is no longer a zero-test module.
   The remaining zeros are `main.rs`, `domain.rs`, `error.rs`, and `lib.rs`.

## 7. Deliberate judgement calls

**JSON was not extended.** The plan says to update
`plans/output-schema-v1.md` "only to clarify that log is separate from JSON, not
to redefine JSON". Adding `scope_reason_code` would have redefined v1, so the
typed code is exposed by log mode only. This is the same call M012A recorded:
one taxonomy in `cleanup.rs`, exposed by the surface that needs it, with any
JSON change left as an explicit future schema decision.

**Log is never selected automatically.** A non-TTY stdout still gets `human`.
`format_log_parses_globally` asserts the default is `Human`, because a mode that
appeared by itself would be a mode nobody chose.

## 8. Downstream handoff (non-blocking)

Gregg's `docs/daemon.md` currently shows `cargo cleanme scan --deep`. That
spelling is stale — it has never been a cargo-cleanme option — and this milestone
does not mutate another repository. **After 0.2.0 is published**, Gregg should
replace that example with the canonical daily/weekly commands and may link to
`docs/AUTOMATION.md`.

This note deliberately carries no dependency: cargo-cleanme's closure does not
wait on a commit in another project, per the plan's §10.

## 9. Unresolved findings, by severity

| Severity | Finding | Disposition |
|---|---|---|
| none blocking | Requirements 5 and 11 are argued from the code path, not observed against a Full scan or a pty. | Recorded (§6). Not correctable inside a bounded suite. |
| none blocking | No greggd rehearsal. | Optional in the plan (§12). Not a closure blocker. |
| low | A stale `scan --deep` example persists in Gregg's documentation. | Non-blocking downstream handoff (§8). |
| low | Gregg's repository URL is not recorded in `docs/AUTOMATION.md`; the prose names the tool without linking. | Deliberate: the plan records no canonical URL and inventing one would be a fabricated source. |

## 10. Correctives

None arising from M012B's own implementation.

**One finding against M012A, fixed before publication.** The documentation pass
over `architecture/13-orchestration.md` found that M012A's removal of the scan
report's `emit_output` guard made `clean --full --format json` emit two JSON
documents on one stream. It was fixed in the same change by restoring the guard
as a named `EmitReport` parameter, and `scan_emits_exactly_one_json_document`
was added. Recorded here rather than in M012A's record because it was *found*
here, but it is an M012A defect and the fix ships with both milestones.