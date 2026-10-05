# M011D — Cargo Selector Qualification Lifecycle

Status: ready

Repository baseline: `c3feae01fe5b684826b77b4d32afcb2e3bd4e3d0`

Source roadmap: Phase 11 — hardening, release trust, and qualification automation

Subsystem roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Primary class: compatibility-evidence hardening

Dependencies: none.

## 1. Objective

Turn `scripts/qualify-cargo-selectors.sh` from an unwired historical characterization script into a repeatable evidence lifecycle for the exact Cargo releases cargo-cleanme claims to support for `--profile` and `--package`.

The product already fails closed for unknown Cargo versions. The hardening goal is to keep the explicit allowlist synchronized with real-Cargo evidence so future maintenance cannot silently preserve a stale capability claim.

## 2. Current evidence

At the baseline:

- `workspace::clean_capabilities_from_version` enables profile selection for exact Cargo 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0;
- package selection is enabled only for exact 1.98.1 and 1.99.0;
- unknown/future versions fail closed;
- `qualify-cargo-selectors.sh` exercises real Cargo behavior but defaults to `1.89 1.91 1.92 stable`, which is not the current exact support matrix;
- the script is wired into no CI, release gate, or recurring compatibility job;
- architecture review records that gap explicitly.

## 3. Invariants

- Real Cargo behavior, not fixtures or assumptions, is authority for selector enablement.
- The runtime allowlist remains exact-version, fail-closed.
- A newly released Cargo version is **not** automatically promoted to supported because an exploratory job happens to pass.
- Profile/package evidence remains distinct; package support has stricter configured-target/shared-dependency requirements.
- Complete workspace/output ownership proof remains unchanged.
- Selector-specific reclaimable bytes remain unknown unless separately designed.
- Qualification runs only inside private temporary fixtures and must not clean a developer workspace.
- A failed/absent qualification never broadens support.

## 4. Work package A — Establish one machine-checkable qualification matrix

Remove the current drift between the runtime allowlist and the script's default version list.

Choose one bounded design:

- a checked-in machine-readable selector qualification policy consumed/validated by both the maintenance script and Rust support table; or
- a precise self-tested drift checker that compares the exact runtime allowlist to the exact workflow/script matrix.

Do not rely on duplicated prose lists.

The authority must express separately:

- profile-qualified versions;
- package-qualified versions;
- exploratory versions, if any.

## 5. Work package B — Make the real-Cargo script assertion-oriented

Refactor `qualify-cargo-selectors.sh` only as needed so each claimed capability produces an unambiguous pass/fail result.

For every profile-qualified version, require:

- profile dry-run/non-mutation behavior;
- release/custom profile isolation;
- configured target/output behavior required by the existing support contract.

For every package-qualified version, additionally require:

- package identity/ambiguity behavior;
- selected package removal while sibling profile/package outputs remain as expected;
- shared-dependency behavior is observed and recorded;
- configured `build.target` and explicit `--target` behavior matches the condition M008D used to authorize package selection.

If a historical qualified Cargo toolchain is no longer installable from rustup, report an operational blocker; do not drop it from the product allowlist silently.

## 6. Work package C — Add a dedicated qualification workflow

Add a least-privilege workflow dedicated to real-Cargo selector qualification.

Trigger policy:

- `workflow_dispatch` for explicit evidence/requalification;
- a low-frequency schedule (for example weekly) to detect ecosystem/toolchain availability drift;
- optionally a path-filtered push/PR job only when selector capability code/policy/script changes, if runtime cost is acceptable.

Do not run the whole historical matrix on every unrelated push by default.

The workflow must:

- install exact required toolchains;
- run the assertion-oriented qualification script;
- retain a bounded text/JSON evidence artifact;
- run exploratory current-stable evidence separately from the supported matrix;
- never edit the support allowlist automatically.

Pin actions by immutable revision.

## 7. Work package D — Gate support-table changes

Any PR/change that modifies:

- `clean_capabilities_from_version`;
- the machine-readable selector policy;
- selector command construction;
- `qualify-cargo-selectors.sh`;

must fail normal CI if the static matrix/contract no longer agrees.

Actual real-Cargo hosted qualification may be a separate workflow, but a support-table change cannot merge with an internally inconsistent matrix.

Document the operator rule: promoting a Cargo version requires a dedicated implementation/closure record or amendment with exact hosted evidence, not merely editing the allowlist.

## 8. Work package E — Future Cargo observation without accidental promotion

The scheduled/exploratory lane may test current stable Cargo and report:

- version;
- whether profile behavior matches;
- whether package behavior matches;
- any changed output/ambiguity semantics.

Passing exploratory evidence is a research signal only. Runtime support remains false until explicitly registered and reviewed.

This preserves the product's fail-closed stance while making future qualification cheap.

## 9. Failure semantics

- One supported Cargo version fails: qualification workflow red; do not silently remove or widen support.
- Toolchain unavailable: classify separately from semantic regression.
- Exploratory stable fails: report, no product regression.
- Matrix drift: normal CI red before real qualification.
- Script produces no assertion/evidence for a claimed version: failure, not pass.
- A fixture fails for an unrelated prerequisite: premise guard must identify it rather than treating nonzero as selector evidence.

## 10. Required tests

- static matrix/drift checker self-test;
- missing supported version from workflow/script;
- unsupported exploratory version accidentally placed in runtime allowlist;
- profile/package lists disagree with evidence policy;
- script premise failures for missing Cargo/toolchain/fixture build;
- exact-version parsing (1.99.0 is not conflated with 1.99 or 1.9);
- current M008C/M008D selector tests remain green.

## 11. Verification

~~~text
cargo test --all-targets --all-features
cargo +1.89 test --locked --all-targets
scripts/qualify-cargo-selectors.sh <exact-supported-toolchains>
# new matrix checker --self-test and normal mode
bash scripts/release-check.sh
~~~

Hosted qualification must record every currently supported exact Cargo release.

## 12. Documentation

Update:

- `plans/subsystems/artifact-discovery-cleanup-roadmap.md` with the evidence lifecycle;
- selector support/reference documentation;
- architecture testing/verification deep dive;
- contributor/release guidance for adding a new Cargo version.

Do not describe exploratory stable as supported.

## 13. Acceptance criteria

M011D closes only when:

- every runtime-enabled selector version is represented in one guarded qualification matrix;
- the real-Cargo qualification script asserts the current support contract rather than merely printing observations;
- a hosted workflow runs the complete supported matrix successfully;
- matrix/policy drift fails ordinary CI;
- future stable observation is separated from support promotion;
- no selector support was broadened merely to make the workflow green;
- M008 ownership/freshness safety regressions remain green.

## 14. Stop conditions

Stop and create a new selector capability plan if:

- a currently supported Cargo version now behaves materially differently;
- package/profile semantics require product behavior changes;
- maintaining exact old toolchains becomes impossible and support policy must change;
- qualification reveals selector-specific accounting can no longer honestly remain "unknown" under existing policy.

Do not bury a capability change in test-maintenance work.

## 15. Closure evidence

Record:

- implementation commit;
- exact supported matrix;
- static guard/self-test results;
- hosted workflow run id;
- per-Cargo profile/package outcomes;
- exploratory current-stable outcome;
- toolchain availability notes;
- unresolved findings;
- disposition.
