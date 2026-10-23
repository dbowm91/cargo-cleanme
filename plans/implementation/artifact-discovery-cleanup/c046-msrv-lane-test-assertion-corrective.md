# C046 — The MSRV lane asserts a rendering the qualified-host case never produces

Status: **closing** — implemented and verified; the MSRV lane
(`cargo +1.89 test --locked --all-targets`) exits 0. This plan carried
**implementation authority** for the work it describes; it authorises **no
release**.
Date: 2026-10-22.
Repository baseline: `main` at `22bbfd2` (0.2.10 published; latest published tag
`v0.2.10`).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Found by: **C045** — `plans/implementation/artifact-discovery-cleanup/c045-clean-alias-single-source-of-truth-corrective.md`
(status: **closing**, unpublished), while running the MSRV lane that C045's
acceptance criteria name.
Related: `architecture/overview.md` §10 item **70**; `AGENTS.md` §3.1 (the MSRV
lane is a gate the ordinary ladder does not cover).

## Objective

Make the repository's **MSRV lane** (`cargo +1.89 test --locked --all-targets`)
green again, by correcting the one assertion whose expectation does not match
what the code renders when the host Cargo qualifies the profile selector.

## Non-goals

- **Not** a change to any production behaviour. The rendering is correct: the
  summary says *the selector's reclaimable size is not measured (output union …)*
  because a selector run's byte figure is the output union, not the selector's
  share, and claiming the union as "would be reclaimed" would over-claim by the
  whole union (`src/cleanup.rs:868-878`). Only the **assertion** is wrong.
- **Not** a change to the qualification policy, to
  `runtime_clean_capabilities`, or to what any host is allowed to do.
- **Not** a change to the cleanup engine, the discovery pipeline, or ownership
  proof.
- **Not** a change to the selector's *premise*: the test derives its
  expectation from `runtime_clean_capabilities()`, which is the right premise
  and stays the premise.

## Current implementation evidence

### D1 — the assertion expects the row renderer's label; the summary renders another branch

`cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
asserts, for a qualified host:

```rust
assert!(
    report
        .render(&crate::report::HumanOptions::default())
        .contains("output-union before")
);
```

`output-union before` is the **row** renderer's label for a selector run's
byte figure (`src/cleanup.rs:982`). The **summary** renderer takes a different
branch when the mode is not Execute *and* a selector is present
(`src/cleanup.rs:868-878`) and prints *the selector's reclaimable size is not
measured (output union …)* — which is the honest statement, and the one the
JSON projection makes too.

The test's premise is derived from the host's own Cargo:
`runtime_clean_capabilities()` (`src/cleanup.rs:4158`) runs `cargo --version`
and asks `workspace::clean_capabilities_from_version` whether that Cargo
qualifies the profile selector. The qualification policy
(`release/selector-qualification.json`) qualifies Cargo 1.89 — so **on the MSRV
lane the selector is qualified**, the preview runs, and the summary branch is
the one that renders.

### D2 — the failure is pre-existing, and the host Cargo's version is the variable

Three measurements, all taken with **C045's source changes stashed**, so the
corrective is excluded as a cause:

| Run | Result |
|---|---|
| `cargo test --lib cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` (current stable) | **passes** |
| `cargo +1.89 test --locked --lib cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` | **fails** — `assertion failed: … contains("output-union before")` |
| the 1.89 compiler with a **stable `cargo` first on `PATH`** (`PATH=/tmp/stablebin:$PATH ~/.rustup/toolchains/1.89-*/bin/cargo test --locked --lib …`) | **passes** |

The third row is the decisive one: the *compiler* is 1.89 in both the second
and third rows, and only the **host Cargo's version** differs. It is therefore
the host Cargo, not the toolchain, that selects the branch.

The CI MSRV lane runs `cargo +1.89 test --locked --all-targets`, so it uses
Cargo 1.89 as the host Cargo — and therefore fails at `main`, independently of
C045.

### D3 — why previous verification missed it

The stable lane passes, and the stable lane is what runs locally and in the
bulk of CI. The failing assertion is inside a test that *passes on the
machine that runs it*, so nothing is red until the MSRV lane runs — and the
MSRV lane is a separate job (AGENTS.md §3.1) whose failure is easy to
attribute to "the old toolchain" rather than to the assertion's premise.

## Invariants

- **I1.** No production behaviour changes. The rendered text, the JSON
  projection, the reason codes and the exit codes are untouched.
- **I2.** The test keeps deriving its premise from `runtime_clean_capabilities()`
  — a host that qualifies the selector must preview, and a host that does not
  must refuse. Only the *rendering* the qualified case is expected to produce
  changes.
- **I3.** The test stays a disjunction-free assertion about the qualified
  case and the unqualified case separately. The repository's own rule applies:
  a case that accepts either outcome is green for the wrong reason.
- **I4.** No new test depends on the *host's* Cargo version, except this one,
  which derives its expectation from that version deliberately.

## Production changes

**None.** This corrective changes a test assertion and nothing else.

## Work packages

- **WP-1 — correct the assertion.** In
  `assert_profile_preview_matches_host_premise`
  (`src/cleanup.rs:10978-10999`), the qualified-host branch must assert the
  text the qualified case actually renders: the summary's *"the selector's
  reclaimable size is not measured (output union …)"*, and **not** the row
  renderer's `output-union before` label — which belongs to a different
  renderer and is asserted where that renderer is used.
- **WP-2 — assert the row label where it belongs.** If no case asserts
  `output-union before` after WP-1, add one that renders the *row* surface
  (the verbose human projection) and asserts the label there, so the label
  stays covered rather than quietly losing its only assertion.
- **WP-3 — re-run the MSRV lane.** `cargo +1.89 test --locked --all-targets`
  must be green, and the stable lane must stay green.

## Failure, cancellation and contention semantics

Unchanged. The corrective touches no production path, spawns no process, and
removes nothing.

## Compatibility and configuration effects

- **No configuration effect.**
- **No user-visible change.** The rendered text is unchanged; only an
  assertion about it changes.
- **No published spelling is affected.**

## Required tests

1. **The MSRV lane is green.** `cargo +1.89 test --locked --all-targets` passes.
   *Fails if:* the assertion still expects the branch the qualified host does
   not render — the input is a host Cargo the policy qualifies.
2. **The qualified case asserts the summary.** The qualified branch asserts
   the *summary*'s wording. *Fails if:* the branch is changed back to the row
   label — the input is the same qualified host.
3. **The unqualified case still refuses.** The unqualified branch still
   asserts `Skipped` and `SelectorUnsupported`. *Fails if:* the correction
   weakens the refusal — the input is a host Cargo the policy does not
   qualify.
4. **The row label stays covered.** A case renders the row surface and
   asserts `output-union before`. *Fails if:* the label loses its only
   assertion — the input is a removal of that assertion.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

## Documentation updates

- `architecture/overview.md` §10 item **70**: move from *Open — owned by
  C046* to *Fixed in C046* when the fix lands, with the evidence.
- `plans/registry.md`: C046's status.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`: a C046 section.

## Acceptance criteria

- The MSRV lane is green: `cargo +1.89 test --locked --all-targets` passes.
- The stable lane stays green: `cargo test --all-targets --all-features`
  passes.
- No production code changed: `git diff --stat src/` shows only `src/cli.rs`
  and `xtask/src/main.rs` from C045, and nothing from this corrective.
- The qualified and unqualified branches are each asserted separately, and
  neither is a disjunction.
- The row label `output-union before` keeps an assertion.
- `architecture/overview.md` §10 item 70 is updated to *Fixed in C046*.

## Stop conditions

- **If** the qualified-host case cannot be made to assert a *specific*
  rendering — that is, if the only green form of the assertion is one that
  accepts either outcome — **stop** and escalate: the case must be re-derived
  from the premise, not weakened.
- **If** the fix requires changing production rendering, **stop** and open a
  separate corrective: this one is a test-only correction by construction.
- **If** the qualification policy would have to change for the test to pass,
  **stop**: the policy is a product decision, and the test follows it.
- **If** the stable lane regresses, **stop** and re-derive.
- **If** the MSRV lane still fails after the correction, **stop** and record
  the remaining failure as a new finding rather than widening this plan.

## Closure evidence requirements

A closure record at `plans/closure/artifact-discovery-cleanup/c046-status.md`
with: the commit or PR; a requirement-to-evidence matrix; the verification
commands **actually run**, with their output; the platform evidence (the MSRV
lane's output, and the stable lane's); the known limitations; the unresolved
findings by severity; and the disposition.

## Planning review (`plans/003-planning-process.md` §12)

- The objective is one assertion, and the objective is stated as a *test*
  correction: no production change is authorised.
- The defect was reproduced on the failing lane **and** on the passing one,
  and the variable was isolated with a third run.
- The evidence names the exact lines in `src/cleanup.rs`.
- The invariants are narrow and checkable.
- The stop conditions cover the three ways this could go wrong (weakening the
  assertion, changing production code, changing the policy).
- No ADR is proposed: nothing durable is decided here.
