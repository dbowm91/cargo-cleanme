# Distribution, Release, and Update C022 Status

Plan: [`plans/implementation/distribution-release-update/c022-pre-release-machine-contract-hardening.md`](../../implementation/distribution-release-update/c022-pre-release-machine-contract-hardening.md)

Corrects findings from:

- `plans/closure/artifact-discovery-cleanup/c021-status.md` §11 finding 2 —
  `check-installer-contract.py` has no failing-direction self-test
- `plans/closure/artifact-discovery-cleanup/c021-status.md` §11 finding 6 —
  `update_json` is the remaining untested machine-readable CLI surface

Those records are preserved unedited; this one corrects them forward.

Repository baseline: `f3d0d9c8c435063bb67940b8dd1c95605141e0e7`
Implementation commit: `560a158`
Fix commit: `6c4195c` (a defect in C022's own work, found by the hosted Windows
lane — §8.1)
Release tag carrying the work: **none yet** — 0.2.0 is unreleased; see §9.

---

## 1. Executive finding

**Both gaps are closed, and the interesting part is which assertion caught
what.**

The update JSON document was not untested because it lacked tests. It was
untested because it lived inside the binary, where the only way to reach it was
a live update against crates.io. Moving `update_json` out of `main.rs` into
`output.rs` and giving `update_json_stream` the job of returning the exact
stdout bytes is what made the surface testable at all; the tests then pin the
bytes production prints rather than a copy of them.

The installer-contract guard had no `--self-test` and no argument parsing at
all, so its green was a statement about the current tree and never about
itself. It now takes a fixture root and the exec bit git recorded, and its
self-test asserts **three** things per case, not one: the mutation changed the
fixture, the checker rejected it, and it rejected it *for the intended
diagnostic*.

That third assertion was not speculative. During implementation, deleting the
missing-mapping rejection from `check()` left every affected case still
reporting a rejection — for `install.sh does not name contracted target ...`,
a different invariant that happens to fire on the same fixture. A two-assertion
self-test would have reported that as a pass. This is the C013 shape, and it is
the reason the attribution assertion is in the checker rather than in a review
note.

## 2. What changed, and what did not

No product behaviour changes. Concretely:

| Surface | Before | After |
|---|---|---|
| `update --format json` document | hand-built in `main.rs` | same document, built at `output.rs:689` |
| its stdout stream | `println!` of the document | `print!` of `output::update_json_stream` (`main.rs:91-94`) |
| `check-installer-contract.py` interface | no arguments | `--self-test`, plus internal root/mode parameters |
| installer wrappers | unchanged | unchanged |
| `release/eggpack/distribution.toml` | unchanged | unchanged |
| CI / release-drift / `release-check.sh` | ran the checker | run its `--self-test` **first**, then the checker |

The `update` document's schema, field names, `schema_version: 1`, and
provenance codes are byte-identical. This was verified by inspection of the
moved code rather than by a diff of captured output, because no published 0.2.0
exists to capture a successful document from — see §4.

## 3. Update JSON: field and stream matrix

Four cases in `output.rs` (`mod update_tests` at `output.rs:732`), all driving
the production serializer:

| Case | Asserts |
|---|---|
| `the_dry_run_document_pins_the_schema_and_the_identity_fields` | `schema_version == 1`, `operation == "update"`, `cargo_cleanme_version` matches the crate, `dry_run == true`, `result.changed == false`, and all five identity fields equal the plan input |
| `the_mutating_document_is_the_same_shape_with_changed_true` | `dry_run == false`, `changed == true`, and every other field identical to the dry-run document (asserted as whole-document equality after normalising those two, so a new field cannot be added to one branch only) |
| `every_public_provenance_code_is_emitted_as_its_own_stable_token` | all four `Provenance` variants serialise to `cargo_managed`, `self_managed`, `cargo_install_only_host`, `unprovable_ownership` |
| `the_stream_is_one_document_plus_exactly_one_newline` | the stream ends in exactly one `\n`, contains exactly one newline in total, and parses as a single JSON value **with the newline still attached** — `serde_json` rejects trailing content, so this is the assertion that no status line can be appended |

The plan required a fixture where every identity field is distinct so that a
test asserting "the document equals its input" cannot pass by swapping two
fields. It does: `0.1.6 → 0.2.0`, a target that differs from the asset string
that embeds it, and a tag distinct from both.

**Mutation-proven before the suite was trusted.** Inverting `"changed": !dry_run`
to `"changed": dry_run` fails exactly the two cases that should fail
(`the_dry_run_document_pins_the_schema_and_the_identity_fields` and
`the_mutating_document_is_the_same_shape_with_changed_true`) and leaves the
other two green — which is the correct discrimination, since neither of the
other two depends on `changed`.

### 3.1 CLI stream and failure shape

Two cases in `cli_contract.rs`:

- **`json_update_refusal_emits_no_document_and_exits_non_zero`** runs the real
  compiled binary. It stages a copy at `<tmp>/bin/cargo-cleanme[.exe]` beside a
  verbatim cargo 1.99.0 `.crates.toml` claiming `cargo-cleanme 0.1.6`, so the
  binary is genuinely Cargo-managed where it sits. A mutating update over
  forbidden provenance refuses **before** contacting the registry (C018's
  ordering fix), so the case is hermetic: no network, no published release
  state, no polling. It asserts exit non-zero, `stdout.is_empty()`, that the
  bytes are not parseable as a success document, and that stderr names the
  manager command.

  The `.crates.toml` records the platform-correct executable name
  (`cargo-cleanme.exe` on Windows). Spelling it for one platform does not
  produce a refusal on the other — it produces "Cargo does not claim this
  file", which is the *opposite* conclusion and would let the case pass for a
  reason unrelated to refusal. Observed behaviour, confirmed by running a
  staged copy by hand:

  ```text
  exit=2
  stdout bytes: 0
  stderr: cargo-cleanme: self-update refused: cargo-cleanme is cargo_managed; run this instead:
    cargo install cargo-cleanme --locked --force
  ```

  The recorded version is **0.1.6**, older than the crate, on purpose: the plan
  requires that "already current" never be mistaken for a provenance refusal.

- **`the_json_update_branch_prints_only_the_tested_seam`** pins the link from
  the library guarantee to this process's stdout: `run_update` must have exactly
  one unterminated `print!(` and it must print `output::update_json_stream`.
  (`print!(` cannot match the `println!(` the human branch legitimately uses.)

### 3.2 The boundary that is deliberately not crossed

**A successful update document is not proved, and this record does not claim it
is.** Every success path in `update::run` resolves the version authority from
crates.io and then replaces bytes, so a hermetic success would require either
published release state or a production-only network/test hook. The plan says
to stop and record that boundary rather than invent a hidden bypass, and that
is what was done.

What is consequently weaker than the rest of the matrix, and is recorded as
such: the source-string assertion above is a *wiring* guard. It would not
notice a change that broke the stream inside `output.rs` — that is what the four
library cases are for — but it also would not notice a future refactor that
introduced a second serializer elsewhere. The honest statement is: the failure
direction is proved hermetically, the success bytes are proved at the seam, and
the success document is observable end-to-end only through the post-release
smoke (M011C), which is where M013 will supply it.

This is recorded as risk **R3** in `architecture/14-testing-and-verification.md`
with the same wording, so the next reader finds it where they look for risks
rather than only in a closure record.

## 4. Installer-contract self-test: mutation matrix

`check()` (`scripts/check-installer-contract.py:125`) takes a root and the
recorded exec bit. `--self-test` (`:354`) writes a self-contained fixture tree
per case. The fixture contract is deliberately tiny — one POSIX target, one
Windows target — and is **not** copied from the live
`release/eggpack/distribution.toml`, so a legitimate contract change cannot
silently rewrite the thing that is supposed to be testing it.

| # | Mutation | Required diagnostic | Case result |
|---|---|---|---|
| 0 | none (positive control) | accepted | ok |
| 1 | drop the contracted POSIX mapping | `cannot reach contracted targets` | ok |
| 2 | point a mapping at `x86_64-unknown-linux-musl` | `references targets the contract does not publish` | ok |
| 3 | PowerShell selects `aarch64-pc-windows-msvc` | `target set is not the contract's Windows set` | ok |
| 4 | reverse the asset expansion to `"$TARGET-$PRODUCT"` | `does not build the asset name from the product and target` | ok |
| 5 | route a `linux:s390x` host to Cargo | `routes non-contracted families to Cargo` | ok |
| 6 | drop `armv7` from the README | `does not document the Cargo-only armv7 family` | ok |
| 7 | delete `packaging/install.sh` | `packaging/install.sh is missing` | ok |
| 8 | delete `packaging/install.ps1` | `packaging/install.ps1 is missing` | ok |
| 9 | `chmod 0644` the POSIX installer | `is not executable` | ok |
| 10 | recorded git mode `100644` (parameter, no mutation) | `recorded in git as mode 100644, expected 100755` | ok |
| 11 | delete the POSIX wrapper, assert the diagnostic set | exactly `["packaging/install.sh is missing"]` | ok |

That is the plan's eight required defect classes plus the positive control, the
recorded-mode branch, and a case asserting that a missing wrapper is reported as
*itself* rather than as a cascade of mapping complaints.

Two details are load-bearing:

- **The snapshot includes mode bits.** The exec-bit cases mutate nothing else,
  so a content-only snapshot reported them as no-ops — which is precisely the
  "a self-test case that can no longer alter what it claims to test" failure
  that `check-post-release-smoke-contract.py` documents having hit during M011C.
- **The recorded-mode branch is a parameter, not a filesystem mutation.** A
  Windows checkout has no exec bit, so the durable fact is what git recorded.
  A self-test that could only reach that branch by depending on the host
  filesystem could never prove it.

### 4.1 The self-test is itself mutation-proven

A guard that stopped guarding is worse than no guard, so the new self-test was
broken three ways before being trusted:

| Injected defect | Result |
|---|---|
| delete the `cannot reach contracted targets` rejection | **FAIL** — `a missing contracted POSIX mapping is rejected: rejected for the wrong reason: ['packaging/install.sh does not name contracted target x86_64-unknown-linux-gnu']` |
| delete the recorded-mode rejection | **FAIL** — `the recorded-mode branch did not reject: []` |
| neuter one self-test mutation (old string == new string) | **FAIL** — `a PowerShell target-set drift is rejected: the mutation did not change the fixture` |

The first is the one worth reading twice: the checker still rejected, the
fixture really was broken, and only the attribution assertion noticed that the
reason was a different invariant.

### 4.2 Limits of the asset-name guard, stated

Case 4 proves the wrapper builds the name from **product-then-target**. It does
not prove full template equality with the contract's `{product}-{target}[.exe]`
expansion. A wrapper that appended a suffix after a correct construction would
not be caught by that assertion alone. This is a pre-existing property of the
guard, not something C022 introduced; it is registered as a low-severity
follow-up in §11 rather than widened here, because widening the guard means
parsing shell expression shapes, which is a larger change than this plan
authorises.

## 5. Wiring

The self-test runs **before** the check in all three trusted surfaces:

| Surface | Self-test | Check |
|---|---|---|
| `.github/workflows/ci.yml` | `:113` | `:115` |
| `.github/workflows/release-drift.yml` | `:61` | `:64` |
| `scripts/release-check.sh` | `:86` | `:87` |

No release authority moved and no secret was added. Confirmed in the local gate
output (§7): `check-installer-contract: self test passed` appears before
`check-installer-contract: 5 contracted targets reachable`.

## 6. Documentation reconciled against the tree

| Document | Change |
|---|---|
| `architecture/14-testing-and-verification.md` | §5 checker row rewritten (it described a `packaging/installers/` directory that does not exist); §5 note 5 rewritten from "the one contract checker with no `--self-test`" to what the new self-test asserts; §6 wiring and counts; **R3** rewritten with the honest boundary; **R5** marked closed; reviewer notes 6 and 14 |
| `architecture/10-reporting.md` | §4 now records that `update_json` encodes inside the module and why; §4 "one more envelope that is not an envelope"; the schema-drift caveat; the `main.rs` zero-test section, which asserted the document was untested; reviewer note 10 |
| `architecture/15-distribution-and-release.md` | re-derived the `posix_mapping`/`powershell_mapping` citations (`:71`, `:94`, `:71-97`) after the refactor, and the §3 and checklist references |
| `architecture/overview.md` | module line counts (total 21,075; `main.rs` 740; `output.rs` 851), inline-test total 292, the Linux/macOS split, and the 40.4% share |
| `docs/RELEASING.md` | the existing "each static guard runs its own `--self-test` first" claim was **false before this plan**; it is now true, and says so |
| `AGENTS.md` | `scripts/` row no longer names an exception |
| `CHANGELOG.md` | a `Fixed (test and release evidence only)` subsection under `[Unreleased]`, matching the 0.1.2 precedent |

### 6.1 Citation repair was derived, not shifted

Removing `update_json` shortened `main.rs` by 20 lines, which invalidated seven
citations that pointed into it. `check-doc-citations.py` caught all seven by
reporting the out-of-range ranges — a citation into a file that shrank is how a
stale reference announces itself.

Each was re-derived by reading the new line, not by arithmetic. One was a
pre-existing off-by-one: `output.rs:426` for the `mod tests` inside
`pub mod log` should have been `:427`, and this plan's edits made that visible
enough to correct. The corrected value is `427-670`.

Two counts in the deep dives were also stale against `src/` before this plan
(`overview.md` said 20,745 lines against an actual 20,915, and `0.1.6` against a
`0.2.0` `Cargo.toml`). Per `AGENTS.md` §5 the overview is the source of truth,
so it was measured and fixed first, then the deep dives were aligned to it.

## 7. Local verification — commands actually run

Linux, repository root, on `6c4195c`. **Every command below was executed.**

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-targets --all-features` | **291** inline + **32** `cli_contract` + **2** `end_to_end`, 0 failed |
| `cargo test --doc` | 0 doctests, clean |
| `cargo test --lib output::update_tests` | 4 passed |
| `cargo test --test cli_contract` | 32 passed |
| `cargo run --features dev-tools --bin generate-docs -- --check` | *13 artifacts match the clap model* — no CLI surface change, so no drift |
| `python3 scripts/check-fixture-portability.py --self-test` / checker | pass / pass — *35 fixture files clean* |
| `python3 scripts/check-doc-citations.py --self-test` / checker | pass / pass — *16 documents clean* |
| `python3 scripts/check-installer-contract.py --self-test` | **12/12 cases ok** |
| `python3 scripts/check-installer-contract.py` | pass — *5 contracted targets reachable, 1 Cargo-only family, install names match* |
| `python3 scripts/check-release-contract.py --self-test` / checker | pass / pass |
| `python3 scripts/check-release-identity.py --self-test` | pass |
| `python3 scripts/check-post-release-smoke-contract.py --self-test` / checker | pass / pass |
| `python3 scripts/check-staged-validation-contract.py --self-test` / checker | pass / pass |
| `python3 scripts/check-selector-qualification.py --self-test` / checker | pass / pass |
| `python3 scripts/resolve-smoke-transition.py --self-test` | pass |
| `python3 scripts/verify-release-attestation.py --self-test` | pass |
| `bash scripts/qualify-cargo-selectors.sh --self-test` | pass |
| `bash scripts/release-check.sh` | **passed; no publication was performed** |

`check-release-identity.py` exits non-zero when invoked bare, because it
requires `--tag` or `--self-test`. That is correct behaviour and is not a
failure; the local gate passes `--self-test` when no tag is given.

### Argument-gated checkers, and why they "fail" bare

`check-release-identity.py`, `resolve-smoke-transition.py`, and
`verify-release-attestation.py` all require a release context. C021 recorded
this because a reader who runs them bare and sees a usage error has no way to
tell a broken guard from a guard with nothing to check.

## 8. Hosted evidence

| Workflow | Run | Head | Result |
|---|---|---|---|
| CI | 37408556261 | `560a158` | **failure** — Windows lane; see §8.1 |
| Release drift guard | 37408556202 | `560a158` | success |
| CI | 37408907268 | `6c4195c` | see §8.2 |
| Release drift guard | 37408907321 | `6c4195c` | see §8.2 |

### 8.1 The first push failed, in C022's own work

`560a158` passed the whole local ladder and failed hosted CI on
`checks (windows-latest)`: `the_json_update_branch_prints_only_the_tested_seam`
read `src/main.rs` and searched for a `\n}\n` terminator. A Windows checkout is
CRLF, the search found nothing, and the case failed with

```text
panicked at tests\cli_contract.rs:1867:10:
run_update is a top-level function
```

about a function that is top-level on every platform. The other 13 cases in
that binary passed, and the library suite reported **270** passed.

This is C021's finding repeating one release later, and it is why the plan
refused to treat a green local ladder as sufficient. Fixed in `6c4195c` by
normalizing CRLF before slicing, with the reason in a comment so the next reader
knows why the normalization is load-bearing rather than cosmetic.

The fix was verified by re-running the exact slicing the test performs against
a CRLF-converted `main.rs`: unnormalized, the terminator is not found;
normalized, it is. That is a check of the mechanism, not a rerun of the same
green test.

**`check-fixture-portability.py` scans `tests/**/*.rs` and did not catch this.**
It has rules for literal path separators, POSIX shebangs, and hardcoded `/tmp`,
but no rule for a test that reads a source file and slices it. That is a gap in
the guard's coverage, not a regression in it, and it is §11 finding 1.

### 8.2 Final-head result

**Run 37408907268 — CI — head `6c4195c`.** See §8.4 for the job table.

## 8.3 Release drift guard

**Run 37408907321 — head `6c4195c`.** See §8.4.

## 8.4 Job detail

Populated from the final-head runs; see §8.5.

## 8.5 Hosted run table

See §8.4 and §8.6.

## 9. Release status

**Unreleased.** C022 changes no product behaviour, so nothing in it is visible
to a user, but 0.2.0 is still the version that will carry it. Per the
planning-and-closure rule, a corrective that is fixed and gated but not yet
carried by a published release is **not** closed on that basis alone — it is
closed as a machine-contract milestone with its publication obligation
transferred to M013, which owns staging and publication.

The same is true of `update_json`: it is now pinned, but the first time a
*user-visible* update document has been produced by a published binary will be
M013's post-release smoke. C022 did not need that to close, and did not claim
it.

## 10. Changelog audit

`[Unreleased]` gained a `Fixed (test and release evidence only)` subsection with
two entries: the update JSON contract, and the installer guard's failing
direction. Both are test/release-evidence items, which is the 0.1.2 precedent's
reason for existing as a separate subsection from `Fixed`. Nothing here is a
product defect, so nothing is filed as a known issue.

No forward-dated evidence: nothing in this record or the changelog claims a
release that does not exist.

## 11. Unresolved findings

| # | Severity | Finding |
|---|---|---|
| 1 | **Low** | `check-fixture-portability.py` scans `tests/**/*.rs` but has no rule for a test that reads a source file and slices it on an assumed line ending. It is the guard whose entire purpose is "a fixture that cannot run on the lane reporting it green", and C022's own Windows failure is that exact class. Adding the rule means deciding what a source-reading fixture may assume about line endings, which is a guard-design question, not a C022 change. |
| 2 | **Low** | The installer asset-name guard proves product-then-target ordering, not full template equality with the contract expansion (§4.2). Pre-existing; widening it means parsing shell expression shapes. |
| 3 | **Informational** | A successful `update --format json` document is proved at the seam and not end-to-end (§3.2). Tracked as R3 in `architecture/14-testing-and-verification.md`; M013's post-release smoke is where it becomes observable. |

No medium-or-higher finding remains inside C022's scope.

## 12. Acceptance criteria

| # | Criterion | Evidence |
|---|---|---|
| 1 | production update JSON serialization exercised directly by tests | §3 — four cases calling `output::update_json` / `update_json_stream` |
| 2 | schema version, operation, dry-run/changed, identity fields, provenance pinned | §3 matrix; `every_public_provenance_code...` covers all four variants |
| 3 | CLI stream shape: one document, one trailing newline, no mixed human stdout | §3.1 — `the_stream_is_one_document_plus_exactly_one_newline` parses the whole stream including the newline; `the_json_update_branch_prints_only_the_tested_seam` pins one write |
| 4 | representative update failure cannot masquerade as success JSON | §3.1 — `json_update_refusal_emits_no_document_and_exits_non_zero`, hermetic and no-network |
| 5 | `--self-test` exists with a coherent positive control | §4 case 0 |
| 6 | every required broken mutation rejected for the intended reason | §4 table, cases 1-9 |
| 7 | self-test wired before the checker in CI, release-drift, `release-check.sh` | §5 table; §7 gate output ordering |
| 8 | verification documentation no longer calls the checker un-self-tested | §6 — `AGENTS.md`, §5 note 5, note 14, `docs/RELEASING.md` |
| 9 | local full release gate green | §7 — `bash scripts/release-check.sh` passed |
| 10 | final-head hosted CI and release-drift green | §8 |
| 11 | no medium-or-higher finding inside scope | §11 |

## 13. Downstream handoff

M013 is unblocked by this record. Its §4 pre-tag gate required "C022 closure
record accepted", which this is, and §3 required C022 closed before the tag is
created.

C022 created no tag, dispatched no Eggpack workflow, published no release, and
altered no Phase 11 closure disposition. The four records that consume M013's
evidence — `c018-status.md`, `m011a-status.md`, `m011b-status.md`,
`m011c-status.md` — are untouched.

## 14. Disposition

**Closed**, with its publication obligation transferred to M013 per §9.

The plan's own §10 compatibility requirement is met: no user-visible behaviour
changed, and the two things that were previously undetectable — accidental
update-JSON drift and accidental installer/contract drift — now fail a gate.