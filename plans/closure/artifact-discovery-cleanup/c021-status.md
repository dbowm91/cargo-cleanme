# Artifact Discovery and Cleanup C021 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c021-pre-release-test-and-verification-evidence-reconciliation.md`](../../implementation/artifact-discovery-cleanup/c021-pre-release-test-and-verification-evidence-reconciliation.md)

Corrects / closes findings from:

- `plans/closure/artifact-discovery-cleanup/c019-status.md` §12 findings 1 and 3
- `plans/closure/artifact-discovery-cleanup/c020-status.md` unresolved finding 1
- the unreleased 0.2.0 changelog surface, which described Phase 12 and C019 but
  not the user-observable C018 and C020 corrections

Those records are preserved unedited; this one corrects them forward.

Repository baseline: `f3dfd7a718d236a5a0b0b664fb74fe02ee7b0f62`
Implementation commit: `81af7f43ef24652b051c0f1262237ef1a74f7af8`
Release tag carrying the work: **none yet** — 0.2.0 is unreleased; see §10.

---

## 1. Executive finding

**The staging-cleanup flake was real, the diagnosis was right, and the test had
a worse problem than flakiness: it could not pass for its own reason either.**

`staging_is_cleaned_up_on_success_and_on_failure` asserted
`staging_leftovers().is_empty()`. That helper enumerated the **process-wide**
temp directory and reported every path whose basename began
`cargo-cleanme-update-test-` — the same prefix every fixture in the module mints
through `unique("cargo-cleanme-update-test")`. Because Rust runs test functions
in parallel, a sibling test's live staging directory satisfied that scan and
failed the assertion for a transaction that had removed its own directory
correctly.

The finding worth keeping is not "it was flaky". It is that the observation
surface was **wider than the subject**: the assertion could not fail for its own
reason, and it could not pass for its own reason. A test in that state reports
a property of the test binary, not of the updater.

The plan required proving the mechanism before changing the harness, and that
proof is in §3. It succeeded, so the harness changed. It did not require
believing the mechanism, so nothing had to be taken on faith.

A second finding, of the same class and found by the same work: **the
verification inventory had already drifted while reading confidently.** The
17-script table described 16 scripts and omitted
`check-doc-citations.py`. Nothing noticed, because a prose table is nobody's
obligation. The guard written to close that gap failed on its very first run
against the pre-corrective tree (§5).

---

## 2. The mechanism, proven deterministically

`a_foreign_staging_directory_is_not_evidence_of_a_leak` holds a second
fixture-owned staging directory with the same prefix live across the subject's
transaction. It asserts both halves:

- the **historical** process-wide scan sees the foreign directory (so the old
  premise had a real input to be wrong about), and
- the **fixture-owned** check passes, having found nothing of its own.

The failing direction was proven by temporarily restoring the old assertion and
running the same fixture:

```
thread '...' panicked at src/update.rs:2466:9:
staging left behind: ["/tmp/cargo-cleanme-update-test-1423423-1791234772039447552-0"]
test result: FAILED. 0 passed; 1 failed
```

A subject that completed successfully, removed its own staging directory, and
still failed — on a path it never created. That is the historical flake,
reproduced on demand rather than waited for.

---

## 3. What the corrected evidence asserts

`FixtureEnvironment` now records ownership rather than inferring it:

| Field | Purpose |
|---|---|
| `created_staging` | every staging path this fixture handed to production |
| `cleanup_calls` | how many times production reached the cleanup callback |
| `suppress_cleanup` | test-only: leave *this* fixture's own path behind |
| `leaked_staging()` | this fixture's recorded paths that still exist on disk |
| `assert_no_staging_leak()` | both claims, deliberately not collapsed |

The two claims are kept apart because they are different. `cleanup_calls > 0`
is satisfied by setting an in-memory `Option` to `None`; "the transaction left
no bytes behind" is satisfied only by the path being absent from the
filesystem. `a_deliberately_leaked_staging_path_is_still_reported` pins the
difference: it asserts `cleanup_calls > 0` **and** that the leaked path is
still reported. A test that accepted the former as the latter would pass there.

### Invariants honoured

- No sleeps, no retries, no `RUST_TEST_THREADS`, no suite serialization. The
  fix is ownership scoping, not timing.
- Isolation is achieved by *not reading another test's state*, never by deleting
  it. The foreign directory in the concurrency control is left in place and
  removed by the test that created it.
- Production `RealEnvironment` / `execute` transaction mechanics are
  **unchanged**. `environment.cleanup(&staging)` remains unconditional at
  `src/update.rs:1187`, after the transaction closure binds its result at
  `:1185`.
- No cleanup/ownership/freshness invariant from ADR 001, C003, C004 or C006 was
  touched. The change is confined to `#[cfg(test)]`.

### Regression matrix

| Case | Asserts |
|---|---|
| `staging_is_cleaned_up_on_success_and_on_failure` (`:2399`) | failed transaction: one staging dir, callback reached, own path absent |
| `staging_is_cleaned_up_on_success_and_on_failure` (`:2399`) | successful transaction: same three claims |
| `a_deliberately_leaked_staging_path_is_still_reported` (`:2429`) | **negative control** — a deliberate leak of its *own* path is reported, and the report equals the recorded paths |
| `a_foreign_staging_directory_is_not_evidence_of_a_leak` (`:2460`) | **concurrency control** — a live same-prefix foreign path does not fail the subject, and the old scan would have |
| `process_wide_staging_paths()` (`:2383`) | retained solely as the premise-negative; documented as not-a-leak-check |

---

## 4. Requirement-to-evidence matrix

| # | Requirement | Evidence |
|---|---|---|
| 1 | Prove or reject the flake mechanism | §2 — deterministic reproduction of the historical failure; mechanism **confirmed**, not assumed |
| 2 | Staging assertion is fixture-owned, not process-global | `leaked_staging()` at `src/update.rs:1310`, scoped to `created_staging` |
| 3 | A deliberately leaked own path still fails the test | `:2429`, negative control |
| 4 | A concurrent foreign same-prefix path cannot fail the subject | `:2460`, concurrency control |
| 5 | No sleeps, retries, or global serialization | `git show 81af7f4 -- src/update.rs` contains none; 25 stress runs green (§6) |
| 6 | Production cleanup unchanged | `execute` untouched; cleanup still at `:1187` |
| 7 | Inventory has exact parity with `scripts/` | `check-doc-citations.py` on `81af7f4`: 17 disk, 17 rows, clean |
| 8 | The inventory guard has premise-negative self-tests | §5 |
| 9 | Stale M011B/M011D unwired claims removed | §7 |
| 10 | Stale release-contract prose removed | §7, `check-release-contract.py` row rewritten from its current source |
| 11 | 0.2.0 changelog covers C018, C019, C020, M012A, M012B | `CHANGELOG.md` `[Unreleased]` |
| 12 | No changelog claim of future publication/attestation/smoke evidence | §9 |
| 13 | Local verification and MSRV green | §6 |
| 14 | Complete hosted evidence from the final head | §8 |
| 15 | No medium-or-higher unresolved finding inside C021's scope | §11 |

---

## 5. The inventory guard, and its failing directions

The guard was added to `scripts/check-doc-citations.py` rather than as an 18th
script, exactly as the plan preferred: a new script would have immediately
changed the inventory it exists to guard. It enumerates `scripts/*.py` and
`scripts/*.sh`, extracts basenames from the deep dive's §5 table rows, and
requires exact set parity plus uniqueness.

Its `--self-test` covers both directions and a duplicate row, and each negative
asserts **the mutation changed the subject** — the failure this repository has
already hit once, where a self-test reported "rejected" for a tree containing no
defect because its mutation had silently become a no-op.

Proven against the real tree, not only against synthetic samples:

| Mutation | Result |
|---|---|
| `scripts/zz-temp-probe.py` created, absent from the table | FAILED — `` `scripts/zz-temp-probe.py` exists but is absent from the ## 5 inventory `` |
| a `retired-probe.sh` row added for a nonexistent script | FAILED — `` lists `retired-probe.sh`, which does not exist in `scripts/` `` |
| unchanged inventory (17 disk / 17 rows) | passed |

**It found the real defect on first run.** Against the pre-corrective tree it
failed with `` `scripts/check-doc-citations.py` exists but is absent from the
inventory `` — the exact drift C019 recorded as finding 1, detected by a
mechanical rule rather than by a reader noticing.

Deliberate limit: the guard stops at set parity. Which workflow invokes a script
is natural language, and automating it means parsing English for "wired into",
which would encode one reviewer's phrasing as a release contract and then fail on
a correct reword. Wiring receives direct review; the table is shaped for it.

---

## 6. Local verification — commands actually run

Run on `81af7f4`, Linux, at the repository root. **Every command below was
executed; the result column is what it printed.**

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-targets --all-features` | **287** inline + **30** `cli_contract` + **2** `end_to_end`, 0 failed |
| `cargo test --doc` | 0 doctests, clean |
| `cargo +1.89 check --locked --all-targets` | clean |
| `cargo +1.89 test --locked --all-targets` | **287** + **30** + **2**, 0 failed |
| `cargo test --all-features --lib update::tests:: -- --test-threads=8` | 44 passed |
| the same, repeated **25×** | 25/25 green, 0 failures |
| `cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features -- -D warnings` | clean — see §12 finding 3 |
| `python3 scripts/check-fixture-portability.py --self-test` / checker | pass / pass |
| `python3 scripts/check-doc-citations.py --self-test` / checker | pass / pass |
| `python3 scripts/check-release-contract.py --self-test` / checker | pass / pass — *5 contracted targets, 11 release assets, wrappers aligned* |
| `python3 scripts/check-selector-qualification.py --self-test` / checker | pass / pass |
| `python3 scripts/check-staged-validation-contract.py --self-test` / checker | pass / pass |
| `python3 scripts/check-post-release-smoke-contract.py --self-test` / checker | pass / pass |
| `python3 scripts/check-installer-contract.py` | pass (no `--self-test` exists — see §11) |
| `python3 scripts/check-release-identity.py --self-test` | pass |
| `python3 scripts/resolve-smoke-transition.py --self-test` | pass |
| `python3 scripts/verify-release-attestation.py --self-test` / `--settings-only` | pass / pass — *immutable releases are enabled* |
| `python3 scripts/gen-release-workflow-shape.py --check` | *workflow shape matches derived inputs (6135 bytes)* |
| `cargo run --features dev-tools --bin generate-docs -- --check` | *13 artifacts match the clap model* |
| `bash scripts/qualify-cargo-selectors.sh --self-test` | pass |

**287 on Linux is a confirmation, not a coincidence.** The corrected counts in
`architecture/` were derived by measurement before the suite was re-run, and
`288 declared − 1 macOS/Windows-only gate = 287` is what the suite reports.

### Argument-gated checkers, and why they "fail" bare

`check-release-identity.py`, `resolve-smoke-transition.py` and
`verify-release-attestation.py` exit non-zero when invoked with no arguments,
because each requires a release context (`--tag`, `--to`). That is correct
behaviour, not a failure: `release-check.sh` invokes them as
`--tag "$RELEASE_TAG"` (`:76`), `--self-test` (`:79`, `:93`, `:115`) and
`--settings-only` (`:116`), and each of those forms passed. Recorded here so the
next reader does not mistake a usage error for a red gate.

---

## 7. Documentation reconciled against the tree

`architecture/14-testing-and-verification.md` was audited from the final tree,
not from the historical narrative. Corrections:

1. **The §5 inventory** now lists all 17 scripts with current wiring, with a
   `Class` column (premise / outcome / generator / qualification / composite) so
   the distinctive category is visible rather than implied.
2. **`check-release-contract.py`**, whose row predated M010A, now describes what
   it actually asserts: comparison against Eggpack producer authority, exact
   Rust release pinning, the draft-not-publish rule, and the retained
   cargo-cleanme-owned `include` allowlist and `/config.toml` invariants.
3. **The "Two scripts are invoked by no workflow" claim is deleted.** It was the
   most misleading sentence in the file. M011B wired `validate-staged-release.py`
   to `validate-staged-release.yml`; M011D wired `qualify-cargo-selectors.sh` to
   its qualification workflow and to CI's `--self-test`. All 17 are reachable
   from an automated surface; 15 from a workflow.
4. **CI job descriptions corrected.** `ci.yml` has five jobs; six of the ten
   Python guards it runs live in `generated-docs`, not `checks`. Several rows
   attributed a script to `checks` or to `installers` when it runs elsewhere, and
   one claimed `release-check.sh` is wired into `checks` when it is wired
   nowhere by design.
5. **Residual risks R1/R2 rewritten**, because they were built on the false
   premise. The narrower true claim replaces them: event-specific verification
   has **wiring but no receipts** — nothing records that a given script ran for
   a given tag. New R5 and R6 recorded (§11).
6. **`AGENTS.md`**: `scripts/` said 13 scripts (17), and `src/` said 18,327 lines
   (20,906). Both were already wrong before C021.
7. **`architecture/overview.md` is the source of truth for counts per
   `AGENTS.md` §5, so it was corrected first**: 286 → 288 inline tests, and
   `update.rs` 2474 → 2635 lines. The deep dives were aligned to it afterwards.

### Citation repair, and why it was not arithmetic

`check-doc-citations.py` gates `file.rs:N`. It cannot see a bare `` `:N` ``,
which is how `architecture/12-self-update.md` cites its own test catalogue.

Measuring rather than guessing found that **all 30 test-name citations in that
file were already stale at the baseline**, by −191 and −503 lines — non-uniform,
and predating C021. The deltas being non-uniform is exactly why the rule is
"re-derive from the source, never shift arithmetically". Each was re-derived by
resolving the test's `fn` and reading the line it lands on, plus the
test-module structural references. The remaining opaque bare citations are
recorded as finding 1 in §11 rather than mass-rewritten.

---

## 8. Hosted evidence

Final head: `81af7f43ef24652b051c0f1262237ef1a74f7af8`.

| Workflow | Run | Result |
|---|---|---|
| CI | 37375658241 | see §8.1 |
| Release drift guard | 37375751351 | see §8.1 |
| Cargo selector qualification | 37375754785 | see §8.1 |

### 8.1 The baseline cancellations, diagnosed

The plan recorded the baseline hosted runs as unacceptable and required
determining **why** they were cancelled before deciding whether any code change
belongs here. They were investigated rather than assumed, and the cause is
infrastructure, not the tree:

- Every cancelled job reports **`steps = 0`** — not one step ever executed.
  They were cancelled while still **queued for a runner**.
- Every cancelled job sits on a hosted OS lane; every job that *obtained* a
  runner completed successfully. **No job that ran a step ever failed**, in any
  of the three workflows the plan cited.
- Neither `ci.yml` nor `release-drift.yml` declares a `concurrency` group, so no
  in-repo configuration can cancel a sibling run.
- The four commits preceding this work were pushed within **30 seconds** of each
  other (20:53:49, 20:53:51, 20:53:53, 20:54:18), spawning CI, drift and
  selector runs concurrently against one shared runner pool.

Diagnosis: **runner-capacity contention, not a product or test failure.** The
evidence was incomplete because jobs never started, not because anything failed.

No code change is made on this basis. Adding a `concurrency` group to `ci.yml`
would be a release/CI policy change, which §1 of the plan explicitly does not
authorize. It is registered as a follow-up in §11 instead.

### 8.2 Final-head results

<!-- FINAL-HEAD-RESULTS -->

---

## 9. Changelog audit — `v0.1.6..HEAD`

Audited against the accepted closure records, not against commit subjects.

| Change | User-observable? | Disposition |
|---|---|---|
| M012A canonical maintenance invocation | yes | **documented** (breaking table + `--dry-run` semantics) |
| M012B bounded `--format log` | yes | **documented** |
| C019 exact-unignore sibling containment | yes | **documented**, with the affected-version disclosure |
| **C018 provenance uncertainty fail-closed** | yes | **added by C021** — previously absent |
| **C020 Windows concurrent first-use config race** | yes | **added by C021** — previously absent |
| C021 staging-test evidence | no | correctly omitted — internal test harness |
| C021 script-inventory guard | no | correctly omitted — repository-internal |
| C021 documentation reconciliation | no | correctly omitted |
| M011A–M011D release/qualification work | operator-facing | deliberately **omitted** from a user-facing changelog; recorded here and in the release checklist instead |

The audit was content-driven: internal refactors with no user or operator
effect were left out rather than padded in.

### No forward-dated evidence

The `[Unreleased]` entry states no attestation, staged-draft validation,
released-binary refusal, or post-publication smoke result. Those cannot exist
before 0.2.0 is staged and published, and M011A/M011B/M011C/C018 still carry
that publication evidence as outstanding. The entry also does not describe
v0.1.0–v0.1.6 as immutable or attested; they remain mutable.

---

## 10. Release status

**Unreleased.** `Cargo.toml` is `0.2.0` and 0.2.0 is not published.

C021 authorizes no publication action. It makes the tree ready for the path
already documented in `docs/RELEASING.md`: prepare and tag the exact 0.2.0
commit, Eggpack candidate build and stage, require the automatic
staged-release validator green on the same tag/source, human inspection,
publication, attestation and asset verification, then the automatic five-target
post-release smoke. C018/M011A/M011B/M011C closure evidence updates with the
real results.

---

## 11. Unresolved findings

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | `architecture/12-self-update.md` carries **257 opaque bare `` `:N` `` citations** invisible to `check-doc-citations.py`. Measured drift at baseline was −191 to −503 lines, non-uniform, so it cannot be shifted arithmetically. Several are genuinely ambiguous — `` `cleanup` (`:N`) `` may mean the trait method, the impl, or the call site inside `execute` | medium | **Open, pre-existing, not C021's to absorb.** C021 re-derived what was mechanically re-derivable (30 test-name citations, verified against source) and added a prominent citation-health note to the document so no reader trusts a bare number. The remainder needs a corrective that reads each one. Extending the guard to symbol-anchored references is the natural fix and is *not* the brittle-natural-language parsing §15 warns against — a symbol name is a machine contract |
| 2 | `check-installer-contract.py` has no `--self-test` and no argument parsing, so it cannot be required to reject a broken setup | medium | **Open.** Every other checker self-tests. Recorded as R5 and §5 note 5. Out of C021's scope (a new checker surface, not a stale one) |
| 3 | `release-check.sh` and `verify-release-attestation.py` are reachable from no workflow | low | **Open by design**, now documented as such rather than as an omission. Operator surfaces: the first refuses a dirty tree and needs `eggpack` and 1.89; the second needs a published immutable release and a live `gh` |
| 4 | Hosted runs can be cancelled while queued for a runner, and 4 commits pushed in 30 seconds is enough to trigger it | low | **Open as infrastructure.** Diagnosed in §8.1. No product change. A `concurrency` group on `ci.yml` is the candidate remedy and is registered as follow-up, not adopted here — CI/release policy is outside C021's authorization |
| 5 | `architecture/12-self-update.md`'s test catalogue names 32 of 44 tests | low | **Open, now stated.** The 12 unlisted are C018's provenance-uncertainty group, named individually in the document. They are the evidence behind the new C018 changelog entry |
| 6 | `update_json` (`main.rs:755`) remains the one machine-readable surface with no test | medium | **Open, pre-existing, deliberately unchanged.** Noted here because C021 touched `update.rs` and this is the adjacent gap a reviewer will ask about. Migrations and tests both need their own plan |

No medium-or-higher finding remains **inside C021's scope**. Findings 1, 2, 4
and 6 are each outside the surfaces this plan authorizes, and each is recorded
rather than absorbed — the mistake this repository has made before.

---

## 12. Defects found in C021's own work

| # | Defect | Disposition |
|---|---|---|
| 1 | The inventory guard's first self-test draft asserted that a prose mention of a script *should* fail parity. It should not — prose is not a table row. The case is now inverted to assert the extractor **ignores** prose, which is the property that actually matters | fixed during implementation, before any commit |
| 2 | Extending `check-doc-citations.py` created a second `check_file` definition, shadowing the first. Python would have taken the later one and worked, which is worse than an error | fixed before commit |
| 3 | **`assert_no_staging_leak` failed the Windows CI lane.** It is called only from the two `#[cfg(unix)]` cases, so on Windows it is unreachable, and `clippy -D warnings` rejected it: `error: method assert_no_staging_leak is never used` at `src/update.rs:1324`, `= note: -D dead-code implied by -D warnings`. Linux was green throughout | fixed with the module's existing `#[cfg_attr(not(unix), allow(dead_code))]` convention — the same attribute `absent` already carries for the same reason — then **verified by running `cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features -- -D warnings` locally, which now finishes clean** |

### Finding 3 is the one worth reading twice

It is a defect in C021's own change, found by the hosted lane the plan
insisted on, and **no amount of local Linux verification could have found it** —
the method is genuinely used, on the only platform the author was running. That
is the repository's standing lesson arriving as a personal one: *a green test
proves only that its own premises hold*, and the premise here was "the CI lane I
did not watch".

Two things followed from it. First, the fix followed the module's existing
convention rather than inventing one, so the attribute now carries a comment
recording why. Second, the cross-target clippy run that would have caught it in
30 seconds is recorded below as a command C021 should have run *before*
pushing, and it is cheap enough to be worth a future plan's attention.

It is also the reason this record does not simply cite "local verification was
green". Local verification was green, and the lane was still red.

---

## 13. Acceptance criteria

| Criterion | Met |
|---|---|
| Flake mechanism proven or explicitly rejected | **Proven** — §2, deterministic |
| Staging assertion fixture-owned, not process-global | **Yes** — `:1310` |
| A deliberately leaked own path still fails the test | **Yes** — `:2429` |
| A concurrent foreign same-prefix path cannot fail the subject | **Yes** — `:2460` |
| No sleeps, retries, or global serialization as the fix | **Yes** — none present |
| Production update cleanup unchanged | **Yes** — `execute` untouched |
| Inventory has exact parity with `scripts/` | **Yes** — 17/17 |
| Inventory guard has premise-negative self-tests | **Yes** — §5 |
| Stale M011B/M011D unwired claims removed | **Yes** — §7.3 |
| Stale release-contract prose removed | **Yes** — §7.2 |
| 0.2.0 changelog covers C018, C019, C020, M012A, M012B | **Yes** — §9 |
| No changelog claim of future publication evidence | **Yes** — §9 |
| Local verification and MSRV green | **Yes** — §6 |
| Complete acceptable hosted evidence from final head | **See §8.2** |
| No medium-or-higher unresolved finding inside scope | **Yes** — §11 |

---

## 14. Disposition

**See §15.** The disposition depends on the final-head hosted result, because
the plan is explicit that a cancelled workflow is not a pass and must not be
converted into one by prose. This record will not claim closure on local
evidence alone.