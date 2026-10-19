# Artifact Discovery and Cleanup C037 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c037-host-cargo-dependent-selector-premise-corrective.md`](../../implementation/artifact-discovery-cleanup/c037-host-cargo-dependent-selector-premise-corrective.md)

Corrects / closes findings from:

- `plans/closure/artifact-discovery-cleanup/m017-status.md` — "Known limitations" and
  "Unresolved findings by severity", both of which carried the failing test as a
  low-severity item awaiting "its own corrective". That record is preserved
  unedited; this one corrects it forward, per `plans/003-planning-process.md` §6.
- `architecture/14-testing-and-verification.md` finding **R7**.
- `architecture/overview.md` §10 finding **26**.

Repository baseline: `main` at 0.2.9 (unpublished; latest published tag `v0.2.8`),
commit `6224d07`.
Implementation commit: *recorded in §1 once the commit exists.*
Release tag carrying the work: **none yet** — 0.2.9 is unreleased and no release
is authorized. See §9.

---

## 1. Executive finding

**The CI red was a test with an unstated premise, not a product defect — and the
premise had a known expiry date that nobody was watching.**

`cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
drove the **real** host Cargo for its profile-selector step and asserted
`CleanOutcome::Previewed`. The product offers the profile selector only for the
nine exact Cargo releases the qualification policy claims, and refuses
everything else *without invoking a Cargo clean* — the safe direction. On a host
whose Cargo is outside that list, the engine reported `Skipped` with
`SelectorUnsupported` and the **test** failed.

Because `clean_capabilities_from_version` matches exact `X.Y.Z` strings, the
**first patch release of any qualified minor** is outside the list. The case
therefore had an expiry date built into it, and nothing alarmed anyone when it
expired.

## 2. The hosted timeline

| Run | Date (UTC) | `checks (ubuntu-latest)` | Host toolchain | Selector case |
|---|---|---|---|---|
| `38283059324` | 2026-10-12 | **success** | rustc 1.99.0 | `ok` |
| `38681699087` | 2026-10-15 | **success** | rustc 1.99.0 | `ok` |
| `38912914854` | 2026-10-16 | **failure** | rustc **1.99.1** | **FAILED** |
| `39307925648` | 2026-10-19 | **failure** | rustc **1.99.1** | **FAILED** |

The break is exactly the toolchain move, quoted from the run log of
`39307925648`:

```
stable-x86_64-unknown-linux-gnu updated - rustc 1.99.1 (58cf8f9c3 2026-10-13) (from rustc 1.99.0 (b940084d7 2026-09-28))
```

`1.99.0` is in `release/selector-qualification.json`'s `profile_selector` list;
`1.99.1` is not, and is asserted *not* to be at `src/cleanup.rs:4213`.

**The red was worse than one red.** `cargo test` stops scheduling the remaining
targets behind a failing `--lib` target, so from 2026-10-16 the **entire**
62-case `cli_contract` and 4-case `end_to_end` integration surface was never
executed on CI. The m017 closure record had already named this consequence and
still classified the failure as low severity.

## 3. What changed

**No production behaviour changed.** `src/workspace.rs` and
`release/selector-qualification.json` are byte-identical to the baseline. No
Cargo version was added to the allowlist — that would be a support *claim*
requiring hosted qualification evidence, which the policy's promotion rule
forbids.

Two changes, both inside the test module of `src/cleanup.rs`:

1. **The premise is now stated, and the assertion follows it.** The
   profile-selector step calls `assert_profile_preview_matches_host_premise`
   (`src/cleanup.rs:9901`), which derives the expected outcome from
   `runtime_clean_capabilities()` — the product's own premise function — and
   asserts a *specific* outcome per premise: a qualified host must report
   `Previewed` and the output-union line; an unqualified one must report
   `Skipped` with `SelectorUnsupported`. It is deliberately **not** a
   disjunction.
2. **The host-independent part of the selector path is now covered.**
   `a_selector_reaches_the_cargo_command_line` (`src/cleanup.rs:9925`) pins that
   a `--profile` selector reaches `cargo clean` as `--profile <name>` and a
   `--package` selector as `--package <spec>`, with neither implying the other.
   Before this, the supported branch was only reachable on a qualified host —
   and CI runs on whatever `stable` currently is.

The repair is **line-count neutral above the end of the test module**, so every
cited line in `architecture/` kept its number. Only lines after the end of the
test module moved, and nothing is cited there.

## 4. The repair is discriminating — proven by mutation

Every mutation below was applied, run, observed, and reverted.

| # | Mutation | Result |
|---|---|---|
| M1 | `clean_capabilities_from_version` returns `profile_selector: true` for every version | the case **passed** — and that is the finding, not a pass: the case's expectation is derived from the same table the product consults, so it cannot see table drift. The table's authority is `workspace::tests::runtime_allowlist_agrees_with_the_selector_qualification_policy` (`src/workspace.rs:2146`), which asserts the table agrees with `release/selector-qualification.json` exhaustively, over every `major.minor.patch` in `0..3 × 0..110 × 0..3`. A table that drifts from the policy fails **there**. The chain is: policy → table → gate, with an independent witness at each link. |
| M2 | The gate stops consulting the capability (`&& !selector_capabilities.profile_selector` → `&& false`) | the case **FAILED**: `left: Previewed, right: Skipped` — the product wrongly offered the selector on an unqualified host, and the case caught it. |
| M3 | The gate always refuses (`&& true`), with the table made to qualify the host (simulating a qualified host) | the case **FAILED**: `left: Skipped, right: Previewed` — the product wrongly refused a selector it had qualified, and the case caught it. |
| M4 | `--profile` is dropped from the built command line | the new composition case **FAILED**: `--profile is missing from ["clean", "--dry-run", "--verbose", "--offline", "--locked", "--manifest-path", "/ws/Cargo.toml", "--target-dir", "/cache/target"]`. |

M2 and M3 are the two directions of the gate, on the two sides of the premise.
M4 is the new coverage. M1 records the one direction this case *cannot* see,
and names the test that can.

## 5. Requirement-to-evidence matrix

| Plan requirement | Evidence |
|---|---|
| I1 — no production behaviour change | `git diff 6224d07 -- src/workspace.rs` is empty; `release/selector-qualification.json` unchanged; the diff touches only `src/cleanup.rs`'s test module |
| I2 — no support-claim change | the allowlist still names exactly the nine policy versions; `1.99.1` still asserted excluded at `src/cleanup.rs:4213` |
| I3 — the test states its premise | `src/cleanup.rs:9901` derives the expectation from `runtime_clean_capabilities()` and asserts a specific outcome per premise |
| I4 — no citation rot | the repair is line-count neutral above the end of the test module; `check-doc-citations.py` is green (17 documents) |
| I5 — host-independent coverage | `src/cleanup.rs:9925`, mutation-checked by M4 |
| WP-1 — premise stated | the amended case at `src/cleanup.rs:6722-6723` |
| WP-2 — host-independent selector coverage | the new case at `src/cleanup.rs:9925` |
| WP-3 — the repair is discriminating | M2, M3, M4 above |
| WP-4 — the record is reconciled | this record; R7 in `architecture/14-testing-and-verification.md`; finding 26 in `architecture/overview.md`; the C037 row in `plans/registry.md` |

## 6. Verification actually run

All on the repaired tree, Linux, host Cargo **1.99.1** — the unqualified case.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --all-targets --all-features -- -D warnings` | **PASS** |
| `cargo test --all-targets --all-features` | **PASS** — 449 lib (was 448), 62 `cli_contract`, 4 `end_to_end`; **0 failed** |
| `python3 scripts/check-fixture-portability.py --self-test` | **PASS** — "all four rules verified in both directions" |
| `python3 scripts/check-fixture-portability.py` | **PASS** — "37 fixture file(s) clean" |
| `python3 scripts/check-doc-citations.py --self-test` | **PASS** |
| `python3 scripts/check-doc-citations.py` | **PASS** — "17 document(s) clean" |
| `python3 scripts/check-release-contract.py --self-test` | **PASS** |
| `python3 scripts/check-release-contract.py` | **PASS** — "5 contracted targets, 11 release assets, wrappers aligned" |
| `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | **PASS** — "13 artifacts match the clap model" |

**The census was re-derived, not copied.** The added case is
platform-independent, so the per-platform counts move by exactly +1. The
derivation used a brace-tracking parser whose attribute association is by
*contiguity* (a `#[cfg]` governs a test only when it is in the contiguous run of
attribute lines immediately above or below the `#[test]` line), evaluated
against `rustc --print cfg --target <triple>` for `x86_64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc` and `x86_64-apple-darwin`, with `test` and
`debug_assertions` added to each cfg set. The parser was **validated against
the real build** first: its Linux set is identical, name for name, to
`cargo test --lib -- --list` (449 = 449).

| Count | Before | After |
|---|---:|---:|
| Declared `#[test]` functions in `src/` | 450 | **451** |
| Compile and run on Linux | 448 | **449** |
| Compile and run on Windows | 411 | **412** |
| Compile and run on macOS | 445 | **446** |
| `src/cleanup.rs` inline tests | 113 | **114** |
| `src/cleanup.rs` lines | 9,885 | **9,983** (production unchanged at 3,746) |
| `src/` lines | 32,680 | **32,778** (production unchanged at 14,908) |

Two earlier derivations of these numbers were wrong and are corrected here: a
`#[cfg(target_os = "linux")]` written as `key = "value"` must be matched
against the cfg set's `key="value"` spelling, and a ±6-line attribute window
attributes the *next* item's `#[cfg]` to the previous test. Both undercounted
the Linux set by 4. Every document carrying the census was updated:
`AGENTS.md`, `.skills/test-evidence/SKILL.md`, `architecture/overview.md`,
`architecture/01-domain-and-errors.md`, `architecture/09-cleanup.md`,
`architecture/14-testing-and-verification.md`.

## 7. Hosted evidence

- **Outstanding at the time of writing:** the `checks` lane on ubuntu, macOS and
  Windows for the implementation commit, plus the release-drift guard. Recorded
  here when the runs complete.

## 8. Known limitations

- **The supported branch is still only *executed* against a real Cargo on a
  qualified host.** The composition case proves the selector reaches Cargo's
  command line on every host, and the gate is covered by mutation in both
  directions, but no hosted lane currently runs on a Cargo that the policy
  qualifies — the hosted `stable` is 1.99.1, and the `msrv` lane's 1.89.0 is
  the nearest qualified toolchain. A qualified-host run of the amended case is
  therefore **not** in the hosted evidence. This is a property of the
  qualification policy, not of the repair.
- **No publication.** 0.2.9 is unreleased and no release is authorized. This
  corrective is fixed but **open** until a published release carries it.

## 9. Disposition

**Conditionally closed — implementation landed and locally verified; the
published release is outstanding.**

Per `plans/003-planning-process.md` §5, compilation is not closure and
publication is not closure. This record is complete on the implementation side:
the defect is corrected, the repair is proven discriminating in both
directions, and the full verification ladder is green on the repaired tree. It
is **not** closed, because no published release carries the correction, and the
hosted evidence in §7 must show all three `checks` lanes green.

## 10. Unresolved findings by severity

- **None medium or higher.**
- **Low (host coverage, not defect):** the qualified-host execution noted in
  §8. Not a defect in the repair; a property of the qualification policy.
- Per the corrective-pass rule, any future material defect gets a new plan, not
  an edit to this record.
