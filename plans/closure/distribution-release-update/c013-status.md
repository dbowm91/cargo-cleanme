# Distribution, Release, and Update C013 Status

Plan: `plans/implementation/distribution-release-update/c013-cross-platform-fixture-premise-audit.md`

Disposition: **closed**

Discharges the open item in `plans/closure/distribution-release-update/c012-status.md`:
"the same class of defect ... could exist in other fixture suites ... a
mechanical sweep for `#!/bin/sh` stubs and literal `":"` PATH joins outside the
installer suite is not recorded as done."

Implementation commit: `51d70f1` (`test: make every external-process fixture
prove its own premise`)

Repository baseline: `911804b` (`plans: record post-Phase-10 corrective sequence`)

Date: 2026-10-04

## Executive finding

The sweep is done, across the whole repository, and it found the class in three
more places — including one that mattered more than the original.

The most serious finding was not in a test. The release-candidate smoke
validator that C009 added to CI — the step whose stated purpose is to prove the
validator is portable — was green on all three hosted lanes while the subject
under test was failing. It accepted `discovered_manifests` of any integer,
including `0`, and never looked at diagnostics. Tightening it exposed a real
product defect, registered as C015 rather than repaired here.

The same validator's root spelling was a second, independent source of the false
green, and it is a product defect too: the product passes the discovered
*relative* manifest path to Cargo with the child's working directory set to that
manifest's own parent, so `cargo locate-project` cannot find the file. The
validator's fixture is corrected to use an absolute root, and the C015 finding
is registered rather than absorbed into C013.

No production defect was repaired under C013, per the plan's failure semantics.

## Fixture inventory

Every surface that substitutes, discovers, or resolves an external executable.
Dispositions are from the audit, not from assumption.

| # | Site | Tool | Platforms | Resolution | Premise asserted | Branch asserted | Disposition |
|---|---|---|---|---|---|---|---|
| 1 | `cli_contract.rs::cargo_external_subcommand_help_matches_direct_help` | real `cargo`, staged binary prepended | all three | PATH discovery by cargo | no (was) | stdout equality only | **corrected** — premise now asserted |
| 2 | `cli_contract.rs::cargo_external_config_edit_help_matches_direct` | same | all three | same | no (was) | stdout equality only | **corrected** — premise now asserted |
| 3 | `cli_contract.rs::json_unattended_yes_executes_through_cargo_and_emits_typed_result` | fake `cargo` `#!/bin/sh` | `#[cfg(unix)]` | PATH prepend | yes — stub's own log file and argument file | yes | correct; scope now documented at source |
| 4 | `cli_contract.rs::config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | `VISUAL` editor | all three | exact path in `VISUAL` (bypasses discovery) | n/a by construction | yes — exit 17, invalid retained, valid parsed | correct |
| 5 | `cleanup.rs::real_cargo_package_selector_preserves_sibling_workspace_artifacts` | real `cargo` | all three | PATH discovery | **no (was)** — an unreadable `cargo --version` made the capability gate false and returned green | yes when it runs | **corrected** — the version probe must now succeed, and the capability skip is announced |
| 6 | `cleanup.rs` production `SystemCargoRunner` | real `cargo` | — | production | — | — | out of scope; no production defect asserted |
| 7 | `update.rs` commit-path cases (7) | candidate `#!/bin/sh` bytes executed by `ExactIdentityValidator` | `#[cfg(unix)]` | staged by Eggup, executed by absolute path | yes — exact identity string | yes | correct; scope now documented at source |
| 8 | `update.rs` non-commit cases (16) | none — trait-injected transport | all three | no process | n/a | yes | correct |
| 9 | `editor.rs::unresolvable_visual_falls_through_to_the_next_candidate` | `ed` file | all three | absolute path, `is_file()` only, never executed | n/a — resolution is the only assertion | yes | correct; the inert `#!` is now labelled as inert |
| 10 | `test_installers.py::case_cargo_produces_nothing` | fake `cargo` / `cargo.cmd` | all three | PATH + PATHEXT | yes (premise case) | **no (was)** — any non-zero exit | **corrected** — asserts the wrapper's own guard |
| 11 | `test_installers.py::case_cargo_missing` | curated PATH without cargo | all three | PATH | yes | **no (was)** | **corrected** — asserts the cargo-absent diagnostic |
| 12 | `test_installers.py::case_binary_404_falls_back` | curated PATH without cargo | all three | PATH | yes | **no (was)** — `"cargo"` appearing anywhere | **corrected** — asserts the fallback-entered diagnostic |
| 13 | `test_installers.py::case_checksum_absent_is_fatal`, `case_transport_failure` | — | all three | — | yes | **no (was)** — `"cargo"` and `"build"` anywhere | **corrected** — asserts the fallback was *not* entered |
| 14 | `test_installers.py::case_unwritable_destination` under root | — | root hosts | — | — | — | **corrected** — was recorded as `ok` while skipped; now a reported skip |
| 15 | `test_installers.py` driver with no runnable block | — | any | — | — | — | **corrected** — exited 0 having qualified nothing; now exits 1 and asserts the executed case count |
| 16 | `test_installers.py::_compile_candidate_stub` | real `rustc` | all three | `shutil.which("rustc")`; POSIX falls back to a `#!/bin/sh` stub, Windows hard-fails | yes — the compiled identity is asserted | yes | correct |
| 17 | `smoke-release-candidate.py` | real `cargo`, via the subject | all three (CI) | subject's own lookup | **no (was)** | **no (was)** — `isinstance(int)` accepted 0 | **corrected** — cargo premise probe, exact manifest count, no Cargo diagnostic |
| 18 | `validate-staged-release.py::glibc_floor` | `readelf` | release host | `shutil.which("readelf")` | **no (was)** — absent `readelf` printed a note that read as a pass | n/a | **corrected** — an unmeasurable floor is now a failure |
| 19 | `validate-staged-release.py::qualify_installer` | `sh`, fixture server | POSIX only, explicitly gated | exact paths | yes | yes | correct |
| 20 | `check-installer-contract.py` | `git` | all three | discovery, tolerant of absence with a filesystem fallback | n/a | yes | correct |
| 21 | `release-benchmark.py::dependency_count` | real `cargo tree` | ubuntu (CI) | discovery | **no (was)** — a failed `cargo tree` recorded a count of `0` | n/a — recorded-only figure | **corrected** — records `null` and says unavailable |
| 22 | `qualify-cargo-selectors.sh` | real `cargo` via rustup | POSIX, not in CI | real dependency integration | yes — asserts artifact states | yes | correct, explicitly POSIX-scoped |
| 23 | `.github/workflows/release-binaries.yml` | pinned eggpack, cargo-zigbuild 0.23.3, Zig 0.14.1 | release runners | real installs, prepended to PATH | — | — | correct (C014 pins the Rust half) |
| 24 | `packaging/install.sh`, `install.ps1` | `curl`, `cargo` | — | production | — | — | out of scope; no production defect asserted |
| 25 | `xtask/src/main.rs`, `tests/end_to_end.rs` | none | — | no external process | n/a | n/a | correct — nothing to substitute |

The two real-tool integration sites (#5, #22) use the genuine dependency
deliberately, which the plan permits, and both now announce a capability skip
instead of returning green silently.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Audit every external-process fixture in the repository | 25-row inventory above, from a mechanical sweep of `tests/**`, `src/**` test modules, `packaging/tests/**`, `scripts/**`, `xtask/**`, and both workflows; no inference from a single search mechanism | pass |
| A substituted tool must be the tool the subject resolves | #1–#2, #10, #17 corrected; premise asserted before the behaviour assertion in every dependent case | pass |
| A pass condition must prove the branch, not an exit code | #10–#13 corrected to assert the wrapper's own diagnostics; the three form a verified chain | pass |
| A skip must never be recorded as a pass | #14, #15 corrected; skips are counted and printed separately | pass |
| A suite that ran nothing must not report success | #15 corrected: zero runnable blocks exits 1, and the executed case count is asserted against the roster | pass |
| A measurement tool that is absent must not pass silently | #18, #21 corrected | pass |
| Platform-scoped fixtures must record the scope at source | #3, #7, #9 now carry explicit source-level scope or `fixture-scope:` justification | pass |
| A statically decidable guard, if precise | `scripts/check-fixture-portability.py`, two rules, self-tested in both directions (9 samples), zero findings across 29 fixture files | pass |
| A Windows-executed substitution must be invocable on Windows | #16 already carried the correct PATHEXT treatment; the new guard prevents a reintroduction | pass |
| No production defect repaired under C013 | C015 registered and left unimplemented; `src/**` production paths unmodified | pass |
| Cleanup safety boundary unchanged | no cleanup, selector, or deletion-boundary assertion was modified | pass |
| Hosted CI green on Linux, macOS, and Windows | run `37242665909`, 9/9 jobs | pass |

## Premise-negative evidence

A premise assertion only observed to pass has not been shown to work. Each new
guard was exercised in the direction that matters.

**Installer branch assertions** — `packaging/tests/test_installers.py --self-test`, 6/6:

~~~text
ok   a non-zero exit with no wrapper diagnostic is not accepted as the branch
ok   exit 0 is not accepted even when the wording matches
ok   the real diagnostic is accepted
ok   the stub is accepted when it is the first PATH entry
ok   the stub is rejected when PATH precedence is reversed
ok   the premise is rejected when no cargo is resolvable at all
~~~

**Installer suite, real tree** — 19 passed, 0 failed, 0 skipped, including
`[suite] every declared case ran for every block`.

**The smoke validator's new guards, against the live C015 defect.** The
pre-C013 validator on the real tree:

~~~text
$ python3 <pre-C013 smoke-release-candidate.py> target/debug/cargo-cleanme
release smoke passed for cargo-cleanme 0.1.1     # exit 0
~~~

while the subject under that same fixture reported:

~~~text
discovered_manifests = 1
workspaces resolved = 0
diagnostics = ['cargo locate-project failed']
~~~

The corrected validator, with the defect reintroduced by hand, rejects it:

~~~text
$ python3 <corrected validator, relative root> target/debug/cargo-cleanme
release smoke failed for cargo-cleanme: the scan reported Cargo failures:
['cargo locate-project failed']                 # exit 1
~~~

and passes on the honest fixture. The before/after is the evidence that the
guard detects the defect class rather than merely existing.

**Fixture portability guard** — 9/9 samples, four of which must be flagged:
floating Python `:` PATH, floating Rust `":{}"` PATH, ungated shebang fixture
are rejected; `os.pathsep`, `std::env::join_paths`, `os.name == "nt"`,
`#[cfg(unix)]`, a `fixture-scope:` marker, and a shebang mentioned in prose are
accepted.

**External-subcommand premise.** `the_staged_binary_is_the_external_subcommand_cargo_will_run`
failed on first run for a real reason: the probe was reading the *test process's*
PATH and found a real installed `cargo-cleanme` at
`/home/sugarwookie/.cargo/bin/cargo-cleanme`. The probe was corrected to read
the child's PATH, which is the environment Cargo actually searches. Both
revisions are recorded because the first is itself evidence that the resolution
is not guaranteed by construction.

## Verification commands actually run

~~~sh
cargo fmt --all -- --check                                     # clean
cargo clippy --all-targets --all-features -- -D warnings       # clean
cargo test --all-targets --all-features                         # 228 + 8 + 1 passed
cargo +1.89 check --locked --all-targets                        # clean
cargo +1.89 test --locked --all-targets                         # 228 + 8 + 1 passed
python3 packaging/tests/test_installers.py                      # 19 passed, 0 failed, 0 skipped
python3 packaging/tests/test_installers.py --self-test          # 6/6
python3 scripts/check-fixture-portability.py --self-test        # 9/9
python3 scripts/check-fixture-portability.py                    # 29 files clean
python3 scripts/smoke-release-candidate.py target/debug/cargo-cleanme
python3 scripts/check-release-contract.py                       # 5 targets, 11 assets, wrappers aligned
python3 scripts/check-installer-contract.py                     # 5 targets reachable, names match
python3 scripts/gen-release-workflow-shape.py --check           # shape matches derived inputs
cargo run --quiet --features dev-tools --bin generate-docs -- --check
bash scripts/release-check.sh                                   # all steps green
~~~

## Platform and fixture evidence

| Lane | Evidence |
|---|---|
| `ubuntu-latest` | run `37242665909`, `checks` + `installers` + `msrv` + `generated-docs` + `benchmark` green; 19/19 installer cases; all 6 premise negatives execute here |
| `macos-latest` | run `37242665909`, `checks` + `installers` green; POSIX installer block runs on this lane as well |
| `windows-latest` | run `37242665909`, `checks` + `installers` green. The PowerShell block executes **only** here, and the 3 branch-assertion negatives in `--self-test` execute on this lane. The two POSIX premise negatives are announced as skipped by the tool rather than reported as passes |

`install.ps1` is not exercised on this Linux host: the block is gated on
`os.name == "nt"`. Its result is taken from hosted Windows CI and is not
claimed from a local run. `pwsh` is present on this host but the block does not
run there.

## Unclosed requirement

None of C013's own requirements.

## Known limitations

- The two `update.rs` commit-path groups remain `#[cfg(unix)]` because the
  candidate fixture is a `#!/bin/sh` script, which is not a Windows executable.
  A Windows counterpart would need a compiled stub, not a different extension.
  Windows commit-path evidence therefore comes from C014's live self-update
  rehearsal against a real published release, and the scope is now recorded in
  `src/update.rs` so it cannot be mistaken for coverage that exists.
- `cli_contract.rs` and `smoke-release-candidate.py` integrate with the *real*
  `cargo` rather than a stub. That is deliberate — they test the real external
  subcommand mechanism — and it means a broken local cargo can make them fail for
  an environmental reason. The new cargo premise probe reports that as a lane
  environment failure rather than a candidate failure.
- The static guard covers only the two mechanically decidable shapes. Whether a
  substituted tool is the one the subject resolves, and whether a pass condition
  proves the intended branch, are not statically decidable without false
  positives; the guard says so in its own docstring, and those two are carried
  by runtime premise assertions and by this inventory.
- `scripts/check-fixture-portability.py` excludes itself from its scan, because
  its self-test corpus is deliberately full of violations. The exclusion is
  recorded in the code rather than hidden.
- The guard's quote pairing is exact for well-formed source; an escaped quote
  inside a literal would end a run early, which surfaces as a finding to review
  rather than as a silent miss.

## Unresolved findings

- **medium** — C015: an explicitly relative scan root silently resolves zero
  Cargo workspaces. The product passes the discovered relative manifest path to
  `cargo locate-project` with the child's working directory set to the
  manifest's own parent, so the path does not resolve. Exit 0, valid JSON,
  `groups: []`, one stderr diagnostic. Fail-safe in direction — a degraded scan
  reports *less* reclaimable space, so `clean` deletes less, never more — but it
  can report a false all-clear. Registered at
  `plans/implementation/distribution-release-update/c015-relative-scan-root-cargo-resolution-corrective.md`.
  Not repaired here, by the plan's failure semantics.
- **low** — the same sweep covered test/fixture/CI-script surfaces. Build scripts
  invoked only inside release runners (`release-binaries.yml` build jobs) were
  read but their per-run shell is generated, not maintained here, so a sweep of
  generated shell is not claimed.
- No other finding requires a further corrective.

## Disposition

**closed.** The sweep is complete and recorded, the nine evidence defects are
corrected, the statically decidable subset is now guarded, every new guard is
proven to fail on a broken setup, hosted Linux/macOS/Windows CI is green, and
the one product defect the sweep exposed is registered as C015 instead of being
silently absorbed.

C012's closure record is not edited. Its open item is discharged here.
