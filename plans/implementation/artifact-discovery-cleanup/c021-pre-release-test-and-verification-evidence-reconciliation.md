# C021 — Pre-Release Test and Verification-Evidence Reconciliation

Status: ready

Repository baseline: `f3dfd7a718d236a5a0b0b664fb74fe02ee7b0f62`

Corrects / closes findings from:

- `plans/closure/artifact-discovery-cleanup/c019-status.md` §12:
  - stale/incomplete `architecture/14-testing-and-verification.md` verification inventory;
  - the intermittent updater staging-cleanup test failure.
- `plans/closure/artifact-discovery-cleanup/c020-status.md` unresolved finding 1:
  - the same stale verification documentation.
- the unreleased 0.2.0 release-evidence surface:
  - `CHANGELOG.md` currently describes Phase 12 and C019, but does not yet describe
    the user-observable C018 provenance correction or C020 Windows concurrent
    first-use configuration correction.

Source roadmap:

- post-Phase-12 corrective / pre-release qualification
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Primary class: test-harness corrective / verification-premise hardening / release-documentation reconciliation

Hard dependencies: none.

Release dependency: **C021 must close before the first 0.2.0 release candidate is
treated as publication-ready.** This plan does not publish a release.

## 1. Objective

Enter the 0.2.0 release boundary with a test suite and verification narrative
whose premises are as strong as the product code they are meant to qualify.

C021 has three tightly related jobs:

1. prove and correct the intermittent updater staging-cleanup test without
   weakening what the test claims;
2. reconcile the architecture verification inventory with the actual
   `scripts/` and workflow wiring, and add a mechanical premise so the inventory
   cannot silently become incomplete again;
3. reconcile the 0.2.0 changelog against all user-observable changes on
   `v0.1.6..main`, especially C018 and C020, without claiming release evidence
   that cannot exist before publication.

This is a verification/evidence corrective. It is not authorization to change
cleanup semantics, updater transaction semantics, release policy, or the
canonical Phase 12 CLI.

## 2. Current repository state

At the baseline:

- `Cargo.toml` is already `0.2.0`; 0.2.0 is unreleased.
- Phase 12 M012A/M012B is implemented and closed.
- C018, C019, C020, M011A-M011D implementation is on `main`.
- the next release is the first one eligible for immutable-release attestation
  and the first release that can supply the remaining operational evidence for
  C018/M011A/M011B/M011C.
- `CHANGELOG.md [Unreleased]` already documents the Phase 12 breaking CLI
  transition, bounded `--format log`, and C019 exact-unignore correction.
- it does not currently document C018 or C020.
- `scripts/` contains exactly 17 current files at the baseline.
- `architecture/14-testing-and-verification.md` says there are 17 scripts, but
  its primary table contains only 16 rows and omits
  `check-doc-citations.py`.
- the same architecture document later retains a pre-Phase-11 statement that
  `validate-staged-release.py` and `qualify-cargo-selectors.sh` are invoked by
  no workflow, even though M011B/M011D now wire those surfaces.
- its `check-release-contract.py` description is also partly historical and
  does not accurately describe the current Eggpack distribution-contract /
  published-target alignment surface.

The latest hosted runs on the baseline are not acceptable closure evidence:

- CI #250 concluded failure with some jobs cancelled and no identified failing
  Rust test in the completed lanes;
- Release drift guard #78 concluded failure with its job cancelled;
- Cargo selector qualification #5 concluded failure with the supported matrix
  green and the exploratory lane cancelled.

C021 must obtain a fresh, complete hosted result from its final head. A
cancelled workflow is not a pass, and must not be converted into one by prose.

## 3. The flaky staging-cleanup test

The historical finding names
`staging_is_cleaned_up_on_success_and_on_failure`. The test currently lives in
`src/update.rs`, not `src/cleanup.rs`.

The test's subject is important: after both a failed update transaction and a
successful update transaction, the staging directory owned by that transaction
must no longer exist.

The current test does not inspect only its own directory. It calls
`staging_leftovers()`, which enumerates the process-wide temporary directory
and reports every path whose basename starts with:

~~~text
cargo-cleanme-update-test-
~~~

But every updater fixture creates staging directories with the same prefix via
`unique("cargo-cleanme-update-test")`, and Rust tests execute in parallel.

That makes this a plausible false-positive mechanism:

~~~text
test A creates its own staging path
test B creates a legitimate in-flight staging path using the same prefix
test A finishes and cleans its own path
test A scans the entire temp directory
test A sees B's still-live path and reports "staging left behind"
~~~

This diagnosis is strong but is not closure evidence. C021 must prove the
premise before changing the harness.

## 4. Invariants

- A successful update fixture must prove its own staging directory was removed.
- A failed update fixture must prove its own staging directory was removed.
- Another parallel test's live staging directory is not evidence that this
  transaction leaked.
- Test isolation must not be achieved by deleting another test's temporary
  state.
- Do not "fix" this by globally serializing the Rust test suite, setting
  `RUST_TEST_THREADS=1`, adding sleeps, or retrying a failed assertion.
- A premise-negative control must prove that the corrected assertion still
  fails when **its own** staging directory is intentionally left behind.
- Product `RealEnvironment::cleanup` and Eggup transaction mechanics remain
  unchanged unless C021 reproduces an actual product-owned staging leak.
- Every cleanup/ownership/freshness invariant from ADR 001 and
  C003/C004/C006 remains untouched.
- Documentation must describe implemented evidence, not intended evidence.
- The changelog may describe code present on `main`, but must not say an
  immutable release, attestation, staged-draft run, or published-release smoke
  has passed before that event actually exists.

## 5. Work package A — Prove the flaky-test premise

Build a deterministic premise-negative around the current leak detector.

Required evidence:

1. create a fixture-owned staging directory for the test subject;
2. create a second, foreign/in-flight directory with the same
   `cargo-cleanme-update-test-` prefix;
3. clean the subject's directory;
4. demonstrate that the baseline `staging_leftovers()` still reports the
   foreign directory and would fail the subject's assertion.

Also stress the current updater test module under ordinary parallel execution.
The deterministic premise-negative is the authority; a probabilistic stress run
is supporting evidence, not a prerequisite for reproducing the race.

Record whether the historical flake matches this mechanism. If the flake occurs
without any foreign matching staging path, stop and investigate rather than
forcing this diagnosis.

## 6. Work package B — Make staging cleanup evidence invocation-scoped

Replace the process-global prefix scan with evidence bound to the
`FixtureEnvironment` / transaction being tested.

Acceptable designs include:

- retaining the exact staging path(s) created by one fixture in test-only
  history and checking those paths after `run`;
- recording create/cleanup events per fixture and independently checking the
  created path no longer exists;
- another equally local ownership mechanism.

The corrected test must prove both:

- the fixture's cleanup callback was reached where expected; and
- the actual fixture-owned staging path is absent from the filesystem after the
  transaction.

It is not enough to set an in-memory `Option` to `None` and call that
filesystem cleanup evidence.

Required negative control:

- use a test-only environment that intentionally leaves its own staging path
  behind, or otherwise suppresses its cleanup;
- assert the new leak check rejects that state.

Required concurrency control:

- while the subject completes correctly, keep a different fixture-owned
  same-prefix staging path live;
- the subject must pass, proving the check does not consume another test's
  state.

Do not add production cleanup behavior to satisfy a test-harness ownership
problem.

## 7. Work package C — Reconcile the verification inventory

Audit the repository's **actual** verification surfaces from the final C021
tree, not from the historical Phase 10/11 narrative.

At minimum:

1. enumerate every regular script under `scripts/`;
2. classify its purpose as outcome check, premise check, generator,
   qualification harness, release operator tool, or composite gate;
3. identify every workflow that invokes it;
4. identify whether `scripts/release-check.sh` invokes it;
5. identify intentionally event-specific/manual-only surfaces explicitly.

Correct `architecture/14-testing-and-verification.md` so:

- every current script appears exactly once in the primary inventory;
- `check-doc-citations.py` is no longer omitted;
- `check-release-contract.py` describes the current M010A/Eggpack
  distribution-contract and package/release support assertions it actually
  performs;
- `validate-staged-release.py` is described as wired through
  `validate-staged-release.yml` under M011B;
- `qualify-cargo-selectors.sh` is described as wired through the M011D
  qualification workflow / self-test path;
- the later "Is every script wired into CI?" subsection no longer carries the
  stale claim that those two scripts are unwired;
- CI job counts, test counts, and workflow ownership statements are reconciled
  to the current Phase 12 tree rather than retained because they were once true.

Historical false-green examples remain historical and must not be rewritten to
look cleaner.

## 8. Work package D — Add a mechanical script-inventory premise

The architecture inventory has already drifted once while still reading
confidently. Add a bounded premise guard so a future added/removed script cannot
silently leave §5 incomplete.

Prefer extending the existing documentation guard rather than creating a new
18th script whose existence immediately changes the inventory it guards.

A suitable implementation is an extension of
`scripts/check-doc-citations.py` or another already-wired checker that:

- enumerates the intended `scripts/*.py` and `scripts/*.sh` inventory;
- extracts the script basenames from the canonical §5 inventory table;
- requires exact set equality and uniqueness;
- fails when a script exists on disk but is absent from the table;
- fails when the table names a script that no longer exists;
- ships `--self-test` premise negatives for both directions.

Do not parse arbitrary natural-language wiring descriptions as a release
contract merely to automate them. Exact script-set parity is the mechanical
minimum; workflow/wiring semantics still receive direct review.

If a machine-readable inventory already exists elsewhere by implementation
time, consume that authority instead of adding a second schema.

## 9. Work package E — Reconcile the 0.2.0 changelog from evidence

Audit `v0.1.6..HEAD` plus the accepted closure records and classify every
user-observable change.

The 0.2.0 `[Unreleased]` entry must, at minimum, accurately cover:

- M012A canonical maintenance invocation and breaking `--dry-run` semantics;
- M012B bounded `--format log`;
- C019 exact-unignore sibling containment;
- **C018 provenance uncertainty fail-closed behavior**:
  positive Cargo-manager evidence that is unreadable/malformed/unsupported no
  longer silently becomes self-managed ownership, and a mutating locally
  forbidden provenance refuses before remote update acquisition;
- **C020 Windows concurrent first-use config creation**:
  same-process first-use racers no longer share one staging name; a
  process-wide nonce removes the collision rather than teaching production code
  to accept ambiguous `PermissionDenied`.

Review M011A-M011D separately:

- release-process/qualification improvements may be documented if useful;
- do not state that 0.2.0's attestation, staged validation, C018
  released-binary refusal, or automatic post-publication smoke has passed until
  0.2.0 is actually staged/published and those operational evidences exist;
- do not describe v0.1.0-v0.1.6 as immutable or attested.

The changelog audit is content-driven, not a requirement to mention internal
refactors that have no user/operator effect.

## 10. Work package F — Re-establish a green pre-release baseline

The final C021 head must have complete hosted evidence.

Required:

- normal CI, all jobs completed successfully;
- Release drift guard completed successfully;
- Cargo selector qualification supported matrix completed successfully;
- exploratory lane either completed according to its defined non-promotion
  contract or has an explicitly documented infrastructure disposition that
  does not masquerade as qualification evidence.

The current baseline's cancelled jobs are not a product diagnosis. Determine
why they were cancelled before deciding whether any code change belongs here.

If a rerun exposes a substantive new product defect unrelated to the C021 test
harness or documentation surfaces, stop and register a separate corrective.
Do not absorb it merely to get 0.2.0 green.

## 11. Required tests

### Staging premise

- foreign same-prefix staging path makes the **old/global** premise fail;
- corrected fixture-scoped check ignores the foreign path;
- corrected check catches a deliberately leaked **own** path;
- success transaction removes its own path;
- failure transaction removes its own path;
- repeated parallel updater-module execution remains green without sleeps or
  suite serialization.

### Verification inventory premise

- disk script missing from §5 table -> guard fails;
- stale table-only script -> guard fails;
- duplicate script row -> guard fails;
- unchanged exact inventory -> guard passes;
- guard self-test proves each mutation really changed the subject before
  accepting rejection.

### Existing regression surface

Retain all C018/C019/C020, M011D, M012A/M012B, updater, cleanup, CLI-contract,
installer, fixture-portability, release-contract, generated-doc, and MSRV tests.

## 12. Verification

At minimum:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets

# Focused updater/test-premise stress; exact filter may be adjusted to the
# final test names without weakening parallel execution.
cargo test update::tests:: -- --test-threads=8

python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test
python3 scripts/check-release-contract.py
python3 scripts/check-selector-qualification.py --self-test
python3 scripts/check-selector-qualification.py

bash scripts/release-check.sh
~~~

If `release-check.sh` is given a release tag, do so only after the tag/source
premise actually exists. C021 itself does not create or publish the 0.2.0 tag.

## 13. Compatibility and release effects

No CLI, JSON, cleanup, discovery, config schema, or updater wire contract change
is authorized by C021.

Expected observable effects:

- flaky false-positive test evidence becomes invocation-scoped;
- verification documentation becomes accurate;
- the documentation guard gains a premise for script inventory completeness;
- the 0.2.0 changelog becomes complete enough to qualify the release boundary.

No version bump beyond the already-selected 0.2.0 is required by this plan.

## 14. Acceptance criteria

C021 closes only when:

- the historical staging-test flake mechanism is proven or explicitly rejected
  by evidence;
- the staging-cleanup assertion is fixture-owned rather than process-global;
- a deliberately leaked own staging path still fails the test;
- a concurrent foreign same-prefix path cannot fail the subject's test;
- no sleeps, retries, or global test serialization are used as the fix;
- production update cleanup code is unchanged unless a real product leak is
  independently reproduced;
- the canonical architecture verification table has exact parity with the
  current `scripts/` inventory;
- the inventory guard has premise-negative self-tests;
- stale M011B/M011D unwired-script claims and stale release-contract prose are
  removed;
- the 0.2.0 changelog covers C018, C019, C020, M012A, and M012B accurately;
- no changelog statement claims future publication/attestation/smoke evidence;
- local full verification and MSRV are green;
- final-head CI, Release drift guard, and selector qualification produce
  complete acceptable hosted evidence;
- no medium-or-higher unresolved finding remains inside C021's scope.

## 15. Stop conditions

Stop and open a separate corrective if:

- a fixture-owned staging directory actually survives after
  `RealEnvironment::cleanup` / the production transaction should have removed
  it;
- the flake reproduces with no foreign matching staging path and the current
  diagnosis is therefore incomplete;
- final hosted runs reveal a substantive product defect outside the test
  harness/documentation surfaces;
- the changelog audit discovers an unplanned user-visible behavior change whose
  correctness needs design/implementation work;
- mechanically validating wiring semantics would require a brittle parser of
  natural-language documentation rather than a bounded machine contract.

Do not turn a pre-release cleanup pass into an unreviewed production behavior
change.

## 16. Closure evidence

Record:

- implementation commit(s);
- exact baseline/final head;
- deterministic old-premise false-positive reproduction;
- fixture-scoped success/failure/intentional-leak/concurrent-foreign matrix;
- parallel stress command and result;
- final `scripts/` inventory and architecture-table parity result;
- documentation-guard self-test negatives;
- `v0.1.6..HEAD` changelog audit disposition;
- local full verification/MSRV output;
- final hosted CI run id;
- final Release drift guard run id;
- final selector qualification run id;
- any baseline cancellation/infrastructure explanation;
- unresolved findings by severity;
- disposition.

## 17. Release handoff

After C021 closes, the repository returns to the release path already documented
in `docs/RELEASING.md`:

1. prepare/tag the exact 0.2.0 release commit;
2. run Eggpack candidate build/stage;
3. require the automatic staged-release validator green on the same tag/source;
4. human-inspect and publish;
5. verify immutable-release attestation/assets;
6. allow the automatic five-target post-release smoke to finish after crates.io
   exposes 0.2.0;
7. update C018/M011A/M011B/M011C closure evidence with the real release results.

C021 authorizes none of those publication actions; it only makes the tree ready
for them.
