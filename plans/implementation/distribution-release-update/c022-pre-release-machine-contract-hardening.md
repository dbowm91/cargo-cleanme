# C022 - Pre-Release Machine-Contract Hardening

Status: closed

Closure record:
[`plans/closure/distribution-release-update/c022-status.md`](../../closure/distribution-release-update/c022-status.md)

Repository baseline: `f3d0d9c8c435063bb67940b8dd1c95605141e0e7`
Implementation commits: `560a158`, `6c4195c`, `3bf8f3a`
Final head: `3bf8f3a`

Corrects findings from:

- `plans/closure/artifact-discovery-cleanup/c021-status.md` section11 finding 2:
  `check-installer-contract.py` has no failing-direction self-test.
- `plans/closure/artifact-discovery-cleanup/c021-status.md` section11 finding 6:
  `update_json` is the remaining untested machine-readable CLI surface.

Source roadmap:

- post-Phase-12 / pre-0.2.0 release hardening
- `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: machine-contract / verification-premise corrective

Hard dependencies: C021 closed.

Downstream hard dependency:

- M013 - 0.2.0 publication and Phase 11 operational closure must not begin
  staging until C022 is closed.

## 1. Objective

Close the two bounded, release-adjacent verification gaps that remain after
C021 without changing product semantics.

C022 must:

1. give `cargo cleanme update --format json` the same kind of explicit
   contract evidence already present for scan/cleanup JSON and log output; and
2. make `scripts/check-installer-contract.py` prove that it can reject broken
   installer/contract shapes before the release gate trusts a green result.

This plan is intentionally small. It does not redesign the updater, installer
UX, JSON schema, Eggpack contract, or release process.

## 2. Current implementation evidence

### 2.1 Update JSON

`src/main.rs::run_update` routes JSON output through:

```rust
fn update_json(plan: &cargo_cleanme::update::UpdatePlan, dry_run: bool) -> String
```

The current object contains:

- `schema_version = 1`;
- `cargo_cleanme_version`;
- `operation = "update"`;
- `dry_run`;
- `result.from_version`;
- `result.to_version`;
- `result.target`;
- `result.asset`;
- `result.tag`;
- `result.provenance`;
- `result.changed = !dry_run`.

`run_update` prints that serialization with `println!`, so the intended stream
shape is one JSON document followed by one trailing newline.

C021 established that this surface has no focused contract test. That is
material because C016/C017/C018 all lived on the self-update path, and every
other machine-readable surface now has stream/schema assertions.

### 2.2 Installer contract checker

`scripts/check-installer-contract.py` proves that the POSIX and PowerShell
wrappers are projections of `release/eggpack/distribution.toml`:

- contracted targets remain reachable;
- wrappers do not invent extra targets;
- Windows wrapper target selection is bounded to Windows;
- asset-name expansion matches the contract;
- Cargo-only fallback families are intentional/documented;
- wrapper files/modes are present.

It is wired into CI, release drift, and `scripts/release-check.sh`.

Unlike the other contract/premise checkers, it has no argument parsing and no
`--self-test`. A green result therefore proves only that the current tree
passes, not that the checker still detects the defect classes it claims to
guard.

## 3. Invariants

- JSON schema version remains 1 unless the implementation evidence proves a
  breaking schema change is actually required.
- Existing field names and meanings are not silently changed.
- Human update output remains unchanged.
- JSON output is exactly one JSON object on stdout, one trailing newline, and no
  ordinary human/progress text on stdout.
- A JSON-formatted updater error remains an error; C022 does not convert
  transport/provenance/update failures into success-shaped documents.
- Dry-run must keep `changed=false`; a successful mutating update must keep
  `changed=true`.
- `result.provenance` must remain the stable provenance code emitted by the
  updater domain model, not human prose.
- Installer wrappers and Eggpack producer authority remain unchanged unless a
  self-test exposes an actual contract inconsistency.
- Self-tests must prove both that their mutation changed the subject and that
  the checker rejected because of that mutation.
- No release is staged or published under C022.

## 4. Work package A - Extract update JSON into a testable contract seam

Prefer the smallest seam that lets tests exercise the actual production
serialization rather than duplicating expected JSON in a second helper.

Acceptable shapes include:

- moving `update_json` into a crate module used by `main.rs` and tests; or
- keeping it in the binary while adding unit coverage in a binary-local test
  module if the repository's current test organization supports that cleanly.

Do not create a second serializer used only by tests.

The contract seam must consume a real `UpdatePlan`/provenance value and emit
the same string production prints.

## 5. Work package B - Pin the update JSON schema and stream contract

Add focused tests that cover at minimum:

### Successful dry-run document

Assert:

- object parses as JSON;
- `schema_version == 1`;
- `operation == "update"`;
- `dry_run == true`;
- `result.changed == false`;
- all version/target/tag/asset fields equal the plan input;
- provenance code equals the domain value.

### Successful mutating document

Assert:

- `dry_run == false`;
- `result.changed == true`;
- all identity fields remain the same shape.

### Provenance variants

Exercise every public updater provenance code that can appear in a successful
plan. Do not hard-code human diagnostics as schema.

### CLI stream shape

Through `tests/cli_contract.rs` or an equivalent integration seam, prove a JSON
update invocation emits:

- exactly one JSON document;
- exactly one trailing newline;
- no human status line mixed into stdout;
- expected exit code.

If a fully successful live update cannot be represented without network/public
release state, use the repository's established fixture injection seam rather
than contacting external services. If there is no bounded way to exercise the
binary entry point without adding production-only test hooks, stop and record
that boundary instead of inventing a hidden network bypass.

### Failure shape

At minimum prove that a representative update failure returns non-zero and does
not emit a success-shaped JSON document.

C022 must not invent a new error JSON schema merely to make this test easy.

## 6. Work package C - Give check-installer-contract.py a real self-test

Refactor the checker only enough to support controlled fixture roots/inputs.

The preferred shape is:

```text
python3 scripts/check-installer-contract.py
python3 scripts/check-installer-contract.py --self-test
```

The normal invocation must continue to check the repository tree exactly as it
does today.

The self-test must create isolated temporary subjects and prove rejection of at
least these defect classes:

1. missing contracted POSIX target mapping;
2. extra POSIX target not present in the Eggpack contract;
3. PowerShell target-set drift;
4. wrong asset-name expansion;
5. undocumented or unexpected Cargo-only fallback family;
6. missing POSIX installer;
7. missing PowerShell installer;
8. non-executable POSIX installer / recorded mode premise where testable without
   depending on the host filesystem alone.

For each mutation:

- assert the fixture was actually changed;
- run the checker against the mutated fixture;
- require non-zero;
- require a diagnostic attributable to the intended invariant rather than an
  unrelated parse/setup failure.

At least one coherent fixture must pass as the positive control.

## 7. Work package D - Wire the new self-test everywhere the checker is trusted

Update all relevant gates so the failing direction is proved before the normal
check:

- `.github/workflows/ci.yml`;
- `.github/workflows/release-drift.yml`;
- `scripts/release-check.sh`.

Ordering:

```text
check-installer-contract.py --self-test
check-installer-contract.py
```

Do not move release authority or add secrets.

Update `architecture/14-testing-and-verification.md` so
`check-installer-contract.py` is no longer documented as the sole checker
without self-test coverage.

Because C021 now guards exact script inventory, do not add a new checker file
unless necessary.

## 8. Work package E - Update release/checker documentation

Reconcile:

- `architecture/14-testing-and-verification.md`;
- `docs/RELEASING.md` if it still states that every static guard self-tests
  while the installer contract checker does not;
- `plans/registry.md` only at closure time.

Do not rewrite the large
`architecture/12-self-update.md` opaque-citation debt here. That is a separate
documentation reconstruction problem and is not a prerequisite for proving the
0.2.0 release bytes.

## 9. Required tests

At minimum:

```text
cargo test --all-targets --all-features
cargo test --doc

# Focused update JSON tests
cargo test update_json
cargo test --test cli_contract update

python3 scripts/check-installer-contract.py --self-test
python3 scripts/check-installer-contract.py

python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test
python3 scripts/check-release-contract.py

bash scripts/release-check.sh
```

Exact Rust test filters may change to match final test names; the coverage
requirements may not.

Hosted CI and Release drift guard must both be green on the final head.

## 10. Compatibility

C022 is not intended to change user-visible behavior.

Expected externally observable behavior:

- none, except that accidental future JSON or installer-contract drift becomes
  detectable.

No version bump beyond the already-selected `0.2.0` is required.

If implementation discovers that the existing update JSON schema is already
internally inconsistent with documented schema-v1 conventions, stop and open a
schema corrective rather than silently redefining version 1 under this plan.

## 11. Acceptance criteria

C022 closes only when:

- production update JSON serialization is exercised directly by tests;
- schema version, operation, dry-run/changed semantics, identity fields, and
  provenance code are pinned;
- CLI stream shape proves one JSON document and one trailing newline without
  mixed human stdout;
- representative update failure cannot masquerade as success JSON;
- `check-installer-contract.py --self-test` exists and has a coherent positive
  control;
- every required broken installer/contract mutation is rejected for the
  intended reason;
- the self-test is wired before the normal checker in CI, release drift, and
  `release-check.sh`;
- verification documentation no longer calls the checker un-self-tested;
- local full release gate is green;
- final-head hosted CI and release-drift evidence are green;
- no medium-or-higher finding remains inside C022's scope.

## 12. Stop conditions

Stop and open a separate corrective if:

- exercising update JSON requires changing updater network/transaction
  semantics rather than exposing an existing test seam;
- the current JSON document cannot be preserved without a schema-version change;
- the installer self-test reveals a real disagreement between wrappers and
  Eggpack authority;
- hosted verification exposes a product defect outside C022's machine-contract
  scope.

Do not absorb unrelated release or product behavior changes merely to reach a
green release gate.

## 13. Closure evidence

Record:

- implementation commit(s);
- update JSON field/stream test matrix;
- provenance variants covered;
- installer self-test mutation matrix and exact rejection diagnostics;
- CI/release-drift wiring changes;
- local `release-check.sh` result;
- final hosted CI run id;
- final Release drift guard run id;
- unresolved findings by severity;
- disposition.

## 14. Downstream handoff

On C022 closure, M013 becomes ready.

C022 itself does not create `v0.2.0`, dispatch Eggpack, publish a GitHub
release, publish crates.io, or alter any existing Phase 11 closure disposition.
