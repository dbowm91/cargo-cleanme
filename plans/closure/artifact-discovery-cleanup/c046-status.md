# C046 — the MSRV lane asserts a rendering the qualified-host case never produces — closure record

Plan: `plans/implementation/artifact-discovery-cleanup/c046-msrv-lane-test-assertion-corrective.md`
Status: **closed — shipped in v0.2.11** (tag `v0.2.11` at `1d679833ae62`, GitHub release published 2026-10-25T17:23:10Z and immutable with a valid attestation, crates.io sha256 `96298f83…`, automatic smoke `40216211513` green on five targets). Receipt:
`plans/closure/distribution-release-update/r031-status.md`.
Closes: `architecture/overview.md` §10 finding **70**.
Baseline: `main` at `48cba7a` (0.2.10, unpublished).
Date: 2026-10-22.

## The executive finding

One test asserted a string that the **quiet** human surface does not render.
`cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
asserted that a profile-scoped preview's report contains `output-union before` —
but that is the **row** renderer's label, and the row renderer only runs on the
**verbose** surface. The quiet surface takes a different branch when a selector
is present in a non-Execute mode, and prints the honest sentence instead:

> the selector's reclaimable size is not measured (output union …)

The test's *premise* was right and stayed right: the expectation is derived
from the host's own Cargo (`runtime_clean_capabilities`), and the qualification
policy qualifies Cargo 1.89 — so on the MSRV lane the selector **is**
qualified, the preview runs, and the summary branch is the one that renders.
The test asked the quiet render for a string only the verbose render contains.
It failed on **every** qualified host, and the MSRV lane is one.

The defect was not caused by C045: the failure reproduces at `main` with
C045's source changes stashed, and running the 1.89 compiler with a **stable**
`cargo` on `PATH` passes — which isolates the host Cargo's version as the
variable, not the compiler.

## What changed

| # | Defect | Fix | Regression test |
|---|---|---|---|
| D1 | The qualified-host case asserted the **row** renderer's `output-union before` label against the **quiet** surface, so it failed on every qualified host — including the MSRV lane's. | The assertion now reads the quiet surface (`HumanOptions::default()`) and expects the summary's own sentence, *the selector's reclaimable size is not measured (output union …*. The premise is unchanged: the branch is still chosen by `runtime_clean_capabilities(runner).profile_selector`, and the unqualified arm still asserts `SelectorUnsupported`. | `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` (helper `assert_profile_preview_matches_host_premise`) |
| D2 | Correcting D1 would have left the `output-union before` label with **no assertion at all** — a label nothing asserts is a label nothing keeps. | A second test renders the **verbose** surface and pins the row label. Both renderings are now asserted; neither is asserted by accident. | `cleanup::tests::the_row_surface_names_a_selector_runs_figure_as_the_output_union` |

**The production rendering is unchanged.** No line of `report.rs` or `cleanup.rs`
that renders the summary or the row was touched; the change is in the tests
and in the helper's comment, which now says which surface it is asserting.

## The tests

The corrected case is a **disjunction-free** assertion: on a qualified host it
asserts the outcome is `Previewed` **and** that the quiet render says the
figure is the output union; on an unqualified host it asserts the outcome is
`Skipped` with the reason code `SelectorUnsupported`. Accepting either
outcome would leave the case green on a host where the selector was wrongly
refused and on a host where it was wrongly offered — the "green that proves
nothing" shape this repository keeps finding.

The new test is a plain rendering assertion: a Cargo-preview report with a
profile selector and one unit row, rendered with `verbose: true`, must contain
`output-union before`.

## The verification ladder

Every step of the ladder in `AGENTS.md` §3, with the result:

| Step | Command | Result |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **clean** |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | **clean** |
| 3 | `cargo test --all-features --all-targets` | **583 lib + 12 + 1 + 79 + 4 + 0 = 679 passed, 0 failed** |
| 4 | `python3 scripts/check-fixture-portability.py --self-test && …` | **clean** |
| 5 | `python3 scripts/check-doc-citations.py --self-test && …` | **17 documents clean** |
| 6 | `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | **13 artifacts match the clap model** |
| 7 | `python3 scripts/check-release-contract.py` | **5 contracted targets, 11 release assets, wrappers aligned** |

**The MSRV lane, outside the ladder** (`AGENTS.md` §3.1) — the lane this
corrective exists for:

| Command | Result |
|---|---|
| `cargo +1.89 test --locked --all-targets` | **passes — 0 failed** |

The lane was red at `main` before this corrective and is green after it. The
qualification policy was **not** changed: it remains exact-release, and the
host's Cargo version is still the premise the assertion is derived from.

**The hosted lanes were not run.** The ubuntu/macos/windows matrix is not
available here. Nothing in this corrective touches path comparison, mtimes,
symlinks or `TMPDIR`, and the platform-specific fixtures are unchanged, but
"not run" is the honest entry: the hosted lanes remain outstanding.

## What deliberately did not change

- **The qualification policy.** The finding that opened this line was that
  the *tool's statements* about qualification were wrong — it named the host's
  capability where it should have named the qualification — not that the
  policy was wrong. The policy stays exact-release; the tool's claims about
  it are now truthful.
- **The production rendering.** The summary's sentence and the row's label
  are both correct as they stand. Only the assertions changed.
- **The premise.** `runtime_clean_capabilities(runner)` remains the thing the
  expectation is derived from, and the unqualified arm remains a distinct
  assertion rather than an "either will do".

## Open

- **None.** The change shipped in **v0.2.11** (tag `v0.2.11` at `1d679833ae62`,
  GitHub release immutable and attested, crates.io sha256 `96298f83…`,
  automatic smoke `40216211513` green on five targets; receipt
  `plans/closure/distribution-release-update/r031-status.md`). The MSRV lane
  was green before publication and is green now.
- **The hosted lanes remain outstanding**, as above.
