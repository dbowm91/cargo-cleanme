# C037 — Host-Cargo-Dependent Selector Premise Corrective

Status: **conditionally closed** — implementation landed and hosted-verified;
closure evidence in
[`plans/closure/artifact-discovery-cleanup/c037-status.md`](../../closure/artifact-discovery-cleanup/c037-status.md).
**Still open until a published release carries the fix.**

Date: 2026-10-19.
Repository baseline: `main` at 0.2.9 (unpublished; latest published tag `v0.2.8`),
commit `6224d07`.
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: M017 (scoped proof default) — whose closure record carried this
failure as a known limitation and named the missing corrective.
Corrects: finding **R7** in `architecture/14-testing-and-verification.md`.
Related: `plans/implementation/artifact-discovery-cleanup/008c-cargo-profile-package-selective-cleanup-qualification.md`,
`011d-cargo-selector-qualification-lifecycle.md`.

## Objective

Remove the **unstated host premise** from the real-Cargo selector test, so the
test states what it assumes and asserts the behaviour the product owes *for
that assumption* — and so the CI `checks` lane is not hostage to the next
Cargo patch release.

The product is **not** at fault and this plan changes **no production
behaviour**. The profile selector is offered only for the exact Cargo releases
the qualification policy claims, and every other Cargo is refused by design.
The *test* was the thing that failed, and it failed because its premise was
never written down.

## Non-goals

- **Qualifying Cargo 1.99.1 (or 1.100.x) for the profile selector.** That is a
  support *claim*, and per
  `release/selector-qualification.json` `_comment_promotion` it requires hosted
  real-Cargo evidence and a dedicated plan. Adding a version to the allowlist
  to make a test pass would be a claim without evidence, and the next patch
  release would break it again.
- Changing `clean_capabilities_from_version` (`src/workspace.rs:146`), the
  qualification policy, or the selector gate.
- Changing the runtime capability *shape* (adding a test-only override hook to
  production code) — see "Rejected alternatives" §4.

## Current implementation evidence

| # | Defect | Evidence |
|---|---|---|
| D1 | `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` asserted `CleanOutcome::Previewed` for a **profile-selector** preview driven against the **host's** real Cargo. The product offers the profile selector only for the nine exact releases the policy claims; `1.99.1` is explicitly excluded and asserted excluded. On such a host the engine takes the fail-closed branch and reports `Skipped` + `SelectorUnsupported` **without invoking a Cargo clean** — the safe direction. The *test* failed. | `src/cleanup.rs:6722` (pre-fix); `src/cleanup.rs:1499-1509`; `src/workspace.rs:146-158`; `src/cleanup.rs:4213` |
| D2 | The failure was **not visible as a small signal**: `cargo test` stops scheduling the remaining targets behind a failing `--lib` target, so the whole `cli_contract` (62) and `end_to_end` (4) integration surface was never executed on CI. | `plans/closure/artifact-discovery-cleanup/m017-status.md` "Known limitations" |
| D3 | The failure was **carried, never owned**: `m017-status.md` recorded it as a known limitation needing "its own corrective", and `architecture/14-testing-and-verification.md` recorded it as finding R7 — but no corrective plan existed, so nothing tracked it to closure. | `plans/closure/artifact-discovery-cleanup/m017-status.md`; `architecture/14-testing-and-verification.md` R7 |

### Timeline (hosted evidence)

| Run | Date (UTC) | `checks (ubuntu-latest)` | Host toolchain | Selector test |
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

`1.99.0` is in the policy's `profile_selector` list; `1.99.1` is not, and is
asserted *not* to be (`src/cleanup.rs:4213`). The allowlist is an **exact
version** list by design, so every Cargo patch release is outside it until it
is separately qualified — a structural time bomb, not a one-off.

## Why previous verification missed it

1. **The premise was discovered, not stated.** The assertion hard-coded the
   outcome of a branch whose precondition is a property of the *host*. Nothing
   in the test said "this requires a profile-qualified Cargo", so nothing
   reviewed the test could see the dependency, and the case could not fail for
   its own reason — it failed only when the world changed.
2. **The qualification allowlist makes drift the default.** Because
   `clean_capabilities_from_version` matches exact `X.Y.Z` strings, the *first
   patch release of any qualified minor* is outside the set. A test written
   against "the host happens to be qualified" therefore has a known expiry date
   and no alarm before it.
3. **The red was carried as a note, not owned as a defect.** `m017-status.md`
   classified it "Low (test-env/toolchain drift, pre-existing, carried)" and
   said an own corrective was still needed — but no plan was opened, so the
   "known limitation" became a permanent resident. The corrective-pass rule
   (`plans/003-planning-process.md` §6) requires a plan for a defect found
   after implementation; the gap was that no plan was ever written.
4. **447 of 448 lib cases still passed.** The red was one assertion inside a
   long real-Cargo case, and the surrounding green made it read as noise rather
   than as a broken premise.

## Invariants

- **I1 — No production behaviour change.** The product continues to offer the
  profile selector only for policy-qualified Cargo releases, and continues to
  refuse everything else without spawning a Cargo clean.
- **I2 — No support-claim change.** `release/selector-qualification.json` and
  the runtime allowlist are untouched.
- **I3 — The test states its premise.** Every host-dependent expectation is
  derived from the product's own premise function, and the case asserts a
  *specific* outcome per premise — never a disjunction that accepts whichever
  outcome the host produces.
- **I4 — No citation rot.** The fix is line-count neutral above the end of the
  test module, so the 2,836 `file.rs:N` citations and the 2,742 bare `:N`
  citations inside `architecture/` keep pointing at the same lines.
- **I5 — The new coverage is host-independent.** The composition of a selector
  into Cargo's command line must be provable on any host.

## Production changes

**None.** `src/workspace.rs` is byte-identical to the baseline.

## Work packages

- **WP-1 — State the premise in the real-Cargo selector case.** Replace the
  hard-coded `Previewed` assertion with a call to a helper that derives the
  expected outcome from `runtime_clean_capabilities()` — the product's own
  premise function — and asserts the matching outcome *and* its reason code in
  each branch. The helper lives at the end of the test module so that the
  replacement is line-count neutral at the call site.
- **WP-2 — Add host-independent coverage of the selector composition.** A new
  case pins that a `--profile` selector reaches `cargo clean` as
  `--profile <name>` and a `--package` selector as `--package <spec>`, and
  that neither implies the other. This is the property that the capability
  gate otherwise hides on an unqualified host.
- **WP-3 — Prove the repair is discriminating.** Mutate the gate in both
  directions and confirm the case fails; mutate the argument builder and
  confirm the new case fails. A repair that cannot fail for its own reason is
  not a repair.
- **WP-4 — Reconcile the record.** Update finding R7 in
  `architecture/14-testing-and-verification.md`, and open this plan's closure
  record with the evidence actually run.

## Required tests

- `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
  — amended, premise-stated.
- `cleanup::tests::a_selector_reaches_the_cargo_command_line` — new,
  host-independent.
- `workspace::tests::runtime_allowlist_agrees_with_the_selector_qualification_policy`
  — unchanged, and still the independent authority that the runtime allowlist
  agrees with the policy file. It is what makes the amended case's derivation
  from the runtime table a *chain* rather than a tautology: a table that drifts
  from the policy fails there, and a gate that stops consulting the table
  fails in the amended case.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test && python3 scripts/check-release-contract.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

## Documentation updates

- `architecture/14-testing-and-verification.md` — finding R7 becomes the
  closed record of this corrective, with the mutation evidence.
- `plans/closure/artifact-discovery-cleanup/m017-status.md` — **not edited**;
  it is a closed record. This plan corrects it forward, per
  `plans/003-planning-process.md` §6.

## Rejected alternatives

- **Add `1.99.1` to the allowlist.** Makes the test pass by making a support
  claim without hosted qualification evidence, and re-breaks on the next patch
  release. The policy's promotion rule forbids it.
- **Add a test-only capability override to production code.** It would give
  the supported branch a deterministic home, but it changes
  `src/cleanup.rs` above the end of the test module and therefore shifts
  every later citation in the deep dives — dozens of ungated bare `:N`
  citations that would have to be re-derived one by one (invariant I4). The
  composition case (WP-2) covers the host-independent part of the selector
  path, and the gate itself is covered by mutation (WP-3), so the override
  buys coverage that the cheaper route already provides.
- **Accept either outcome (`Previewed` or `Skipped`).** It is a disjunction:
  it stays green on a host where the selector was wrongly refused *and* on a
  host where it was wrongly offered. It would have been the one-line fix, and
  it is exactly the "green that proves nothing" shape this repository keeps
  finding.

## Acceptance criteria

1. `cargo test --all-targets --all-features` is green on a host whose Cargo is
   **outside** the policy (1.99.1) and on one whose Cargo is **inside** it.
2. The amended case fails when the selector gate stops consulting the
   capability, in both directions.
3. The new composition case fails when the selector no longer reaches Cargo's
   command line.
4. `src/workspace.rs` and `release/selector-qualification.json` are unchanged.
5. The deep-dive citations resolve, and no cited line moved.
6. The `checks` lane is green on all three hosted platforms.

## Stop conditions

- If the repair requires a support-claim change (adding a Cargo version to the
  allowlist), **stop**: that is a qualification decision for a plan with
  hosted evidence, not a repair.
- If making the case deterministic requires a production seam that shifts
  cited lines, **stop and re-plan** rather than accept silent citation rot.

## Closure evidence requirements

- The mutation evidence from WP-3, with the exact mutated code and the exact
  failure each mutation produced.
- The verification commands above, actually run, with their results.
- The hosted CI result for the amended commit on all three platforms.
- **A published release carrying the fix.** Until one exists, this corrective
  is fixed but **open**: compilation is not closure, and publication is not
  closure.
