# C024 — Windows glob canonicalization corrective — status

Status: **closed.**

Plan: `plans/implementation/distribution-release-update/c024-windows-glob-canonicalization-corrective.md`

Opened by: [`c023-status.md`](c023-status.md) §11.1, from the 0.2.1
patch-release requalification.

Implementation commits:

| Commit | Work |
|---|---|
| `4fba120a61cc18e2de2cbced47b8928a2c8408ef` | the fix: `glob_spelling`, the `\` arm of `escape_glob_literal`, and the tests |
| `33d9772` | refusal 2 found by the Windows lane: strip a Win32 verbatim prefix |
| `0aba2ea` | fixture repair: `cmd` reads `/` as a switch, so `mklink` needs backslashes |
| `0a7a596` | refusal 3 found by the Windows lane: a trailing `\` is a dangling escape only where it escapes |

Repository baseline at plan registration: `96801ebb0baac27116d26e8c27eed31193f47972`
(the C025 reconciliation).

## 1. What changed, and what did not

This was a **matching-semantics corrective**, not a safety patch — the defect
never deleted the wrong bytes, it failed to *match* what the user wrote. It
changed one function's contract in `src/config.rs` and nothing else:

- `escape_glob_literal` (`src/config.rs:244-276`) no longer rejects `\`. It is
  now reachable only where a `\` is a *file-name* character rather than a
  separator, and it doubles it.
- A new `glob_spelling` / `glob_spelling_with` (`src/config.rs:218-242`)
  normalizes the canonical spelling to `/` **before** the escape.
- `canonical_pattern_prefix` (`src/config.rs:292-335`) calls them in that order
  for `PatternKind::Glob`, and splices `PatternKind::Literal` (`scan.unignore`)
  exactly as before.

It changed **no** CLI surface, **no** JSON or exit-code contract, **no**
ownership or authorization rule, **no** CI workflow, **no** release process,
and **no** release bytes. `cargo clean` is still the only thing that deletes.

### The safety direction, stated plainly

The defect failed *under*-matching: an `exclude` that did not match meant the
named tree was cleaned as though no `exclude` had been written. That is the
default, so nothing became newly destructive. The fix moves in the opposite,
protective direction — a written `exclude` now matches — so it **narrows** what
can be cleaned in the one case that was previously unprotected. It cannot widen
any cleanup: the only patterns whose meaning changes are ones that previously
matched *nothing at all* (a Windows prefix that differed from its canonical
spelling was left un-rewritten, so the brackets stayed live; a Unix directory
named `back\slash` was refused and left un-rewritten too).

The one behaviour that is *not* strictly a widening is worth naming: on Windows,
a pattern whose canonical prefix previously matched nothing can now match. If
that pattern was an `include`, a workspace that was previously skipped as
`not included` can now become eligible. That is the pattern doing what it says,
and eligibility still requires the full ownership proof and authorization
(`architecture/09-cleanup.md`); no authorization or ownership rule was touched.

## 2. The decision the plan required (plan §4)

Plan §4 listed three options and required the corrective to state which platform
each applies to. The premise option 1 rested on — "requires establishing that
candidates are matched against `/`-spelled paths too — which is a separate,
currently unverified premise" — was **verified before the choice**, from the
dependency's source rather than from memory:

| Question | Answer | Source |
|---|---|---|
| What spelling does a candidate reach `globset` in? | `/` on every platform | `globset-0.4.20` `Candidate::new` → `normalize_path`, which rewrites every `std::path::is_separator` byte to `/` on non-Unix and is the identity on Unix |
| What spelling does a *pattern* compile to on Windows? | also `/` — `backslash_escape` defaults to `!is_separator('\\')`, and with it off `parse_backslash` pushes `Literal('/')` for every `\` | `globset-0.4.20` `GlobBuilder` defaults and `parse_backslash` ("normalize all patterns to use / as a separator") |

**Option 1 was chosen**, and it is correct *because* of that second row, not in
spite of it: a `\` left in a spliced Windows spelling cannot match the candidate
it names, so normalizing is not cosmetic — it is what makes the escape
reachable at all.

**Option 2 was rejected, and the plan's stated reason for it was wrong.** The
plan rejected `backslash_escape(false)` + escaping `\` as a literal separator
because "a separator cannot be expressed in the escaped form". The stronger and
accurate reason is that on Windows the candidate has **no** `\` in it at all —
globset normalized it away — so a literal `\` could never match. Enabling
`backslash_escape(false)` on Windows is a **no-op**, because it is already the
default there.

**Option 3 was rejected as the plan says**, and is worth restating because the
evidence changed: documenting `\` as unavailable would have been wrong. A `\` in
a *user's* Windows pattern compiles to `/`, so `C:\dev\proj\*` and
`C:/dev/proj/*` are the same pattern and both work. There is nothing to
document as unavailable. Had the premise above been assumed rather than
verified, this plan would have written a user-facing restriction that is not
real.

### Platform scope, as the plan required

| Platform | Canonical spelling contains | Escaped as |
|---|---|---|
| Windows | `\` separators (every path) | `/`, then `* ? [ ]`; `\` cannot be in a file name, so the `\` arm is unreachable there |
| macOS / Linux | `/` separators; `\` legal in a file name | `/` unchanged, then `* ? [ ]`, and a literal `\` doubled as `\\` |

`{` and `}` remain the `None` case on both, because no bracket form of an
alternation exists. That is the whole of the remaining `None` arm, it is
documented at the function, and it is tested individually by the existing Unix
test's `a{b}c` case.

## 3. Requirements mapped to evidence (plan §5, five criteria)

| Criterion | Where satisfied | Evidence |
|---|---|---|
| **1.** A Windows-lane test proves a bracketed directory name is matched literally and its unescaped reading is not | `a_bracketed_canonical_spelling_is_matched_literally_on_windows`, `src/config.rs:860` | Builds a real `real[abc]` directory and `reala`/`realb` siblings, a **junction** alias (`cmd /c mklink /J`, no elevation), writes a `\`-spelled `config.toml`, reads it through `load`, and asserts (a) the rewritten pattern, (b) the named tree matches, (c) both siblings do not, and (d) the *unescaped* reading fails both — the premise is asserted in the same test, not assumed. See §5 for the lane result |
| **2.** Unix behaviour is unchanged; the existing Unix test still passes and is still discriminating | `a_canonical_spelling_cannot_inject_glob_characters`, `src/config.rs:643` | **Unchanged file bytes** — the diff to that test is empty. It still passes, and mutation C (§4) shows it fails the moment bracket escaping is removed. Its `#[cfg(unix)]` comment, which recorded *why* the gate is honest, still holds and now has a Windows case of its own |
| **3.** `escape_glob_literal`'s `None` arm is unreachable on every supported platform, **or** its remaining inputs are documented and tested individually | `src/config.rs:246-252` (doc), `src/config.rs:250` (`'{' \| '}'`) | The remaining input pair is `{`/`}`, documented in the function's doc comment as the reason `None` exists at all, and tested by the pre-existing Unix case at `:699-705`. The `\` input is no longer in the `None` set — it moved to the escape, and the Unix case that proves the doubled spelling compiles to a literal `\` is `a_backslash_in_a_canonical_directory_name_is_spliced_as_a_escape` (`:817`) |
| **4.** `docs/USAGE.md` states the rule and its platform scope | "Patterns are matched against canonical paths, and canonicalized first" | States that directory names are matched **literally, not as globs**, that the user does not escape brackets themselves, the `{`/`}` case where the rewrite is skipped, and the two `globset` defaults that do apply (case-sensitive; `*` crosses separators; a `\` in a Windows pattern is a `/`) |
| **5.** All three hosted lanes green, with the Windows lane **observed** rather than cancelled | §4, §6 | Run `37516727455` on `0a7a596`: every job `success`, including `checks (windows-latest)`, which ran `a_bracketed_canonical_spelling_is_matched_literally_on_windows` and reported `292 passed; 0 failed`. Three earlier runs failed and are recorded in §4 rather than omitted |

## 4. What the Windows lane found, and what it cost

**The rewrite had three separate Windows-only refusals, and reading the code
found none of them.** All three were found by running the hosted Windows lane,
one per iteration, and each one was a case where the rewrite silently declined to
run — the same failure direction as the defect C024 was opened for.

| # | Refusal | Why it is wrong on Windows | Found by |
|---|---|---|---|
| 1 | `escape_glob_literal` returned `None` for any `\` | every canonical Windows path contains one, so the splice never ran | the defect itself (C023 §11.1) |
| 2 | `fs::canonicalize` answers `\\?\C:\…`, whose `?` is a glob metacharacter | the split at the first `* ? [ {` landed **inside** the prefix, so the rewrite declined again — and a prefix that survived would demand a candidate no walk produces, because `discovery` deliberately walks each root's own spelling | run `37514729530`, `config.rs:754` and `:904` |
| 3 | `prefix.ends_with('\\')` refused the pattern | on Unix that is a dangling escape; on Windows `\` is a **separator**, so `C:\dev\proj\*` has a prefix ending in one, and the guard refused the rewrite for every `\`-spelled pattern | run `37516147002`, `config.rs:985` |

Refusal 2 was a **real defect in the fix itself**, not only in the fixture: it
would have made the rewrite a no-op on exactly the inputs that exhibit it, which
is the failure mode C024 exists to remove. It is fixed by `dereference`
(`src/config.rs:265-275`), which drops the verbatim prefix — a property of how
the platform spells a path rather than of anything the user wrote, so removing it
cannot change a user's pattern.

Two of the three iterations were also **fixture** failures, and both are recorded
because a fixture that cannot run is the failure mode this repository keeps
paying for:

- `mklink /J` received `C:/Users/…` and answered `Invalid switch - "Users"`,
  because `cmd` reads a `/` as a switch introducer. Run `37514729530` passed the
  same step only because its paths were still verbatim `\\?\C:\…`.
- Both fixtures rooted themselves at `fs::canonicalize`'s verbatim answer, whose
  `?` broke the pattern split before the subject was consulted.

The rule that came out of this is now in the test code rather than in a person's
head: **the Windows answer is asserted on every lane.** `glob_spelling_with`
takes the separator as an argument and `dangling_trailing_escape` takes the
escape rule as one, so the two platform-dependent decisions can be pinned
without a Windows filesystem — which is what the third iteration lacked.

## 5. Test evidence: what each new test fails on

A green test proves only that its own premises hold, so each new test was run
against deliberately reverted implementations — five of them, covering every part
of the change rather than one representative slice. The production file was
restored from a byte-identical copy after each mutation and the suite re-run.

| Mutation | What was reverted | Tests that failed |
|---|---|---|
| **A — the pre-fix splice** | `'\\'` back into the `None` set, and the native spelling spliced instead of the normalized one | `a_backslash_in_a_canonical_directory_name_is_spliced_as_a_escape` (17 passed / 1 failed) — the pattern came back as `/tmp/.tmpXXX/bslink/*` instead of `…/back\\slash/*` |
| **B — separator normalization reverted** | `glob_spelling_with` made the identity | `a_spliced_canonical_spelling_matches_the_directory_it_names` on the Windows assertion: `C:\\Users\\…` instead of `C:/Users/…` |
| **C — metacharacter escaping removed** | `'*' \| '?' \| '[' \| ']'` pushed raw | **2 failures**: the new every-lane test *and* the pre-existing `a_canonical_spelling_cannot_inject_glob_characters` — which is the direct evidence that criterion 2's test is still discriminating |
| **D — the trailing-escape guard reverted** | `dangling_trailing_escape` back to "any trailing `\` refuses", which is what the Windows lane found | `a_spliced_canonical_spelling_matches_the_directory_it_names` (17 passed / 1 failed) |
| **E — de-verbatim reverted** | `dereference` no longer called, leaving `//?/C:/…` | `a_spliced_canonical_spelling_matches_the_directory_it_names` on the first verbatim input (17 passed / 1 failed) |

Two further checks, because a Windows-only test cannot run on this machine:

- The Windows test body was compiled **with its `#[cfg(windows)]` gate removed**
  under `cargo clippy --all-targets --all-features -- -D warnings`: clean. A
  type error or a dead-code warning inside a lane-only test is invisible to
  every other lane, and this is what makes the Windows lane the first place it
  would have surfaced.
- The Windows *answer* is asserted on every lane. `glob_spelling_with` takes
  the separator as an argument precisely so the Windows spelling can be pinned
  without a Windows filesystem; the every-lane test compiles
  `C:/Users/runneradmin/real[[]abc[]]/*` and asserts it matches
  `…/real[abc]/proj` and not `…/reala/proj`.

## 6. Platform and fixture evidence

| Surface | Result |
|---|---|
| Linux (local) | `cargo test --all-targets --all-features`: **318** inline + **36** CLI-contract + **4** end-to-end, 0 failed. The Windows case is `#[cfg(windows)]` and is not among them |
| Windows lane | `checks (windows-latest)` on run **`37516727455`** — `success`, 292 inline tests, and the log names `a_bracketed_canonical_spelling_is_matched_literally_on_windows ... ok` rather than showing it skipped. It could not be made vacuous: the junction is made with `cmd /c mklink /J` and the assertion carries its stderr, so a lane where the fixture cannot run **fails** — which is exactly what two runs in a row did (§4) |
| macOS lane | `checks (macos-latest)` on run `37516727455` — `success`. The two new Unix-only cases ran there |
| Fixture-portability guard | `python3 scripts/check-fixture-portability.py --self-test` → "all four rules verified in both directions"; the run → "35 fixture file(s) clean". The new Windows fixture carries an explicit `fixture-scope:` justification **and** a `#[cfg(windows)]` gate, so it satisfies the guard by being genuinely scoped rather than by being exempted |
| MSRV | Not run locally: toolchain 1.89 is not installed on this machine (only 1.80, stable, and nightlies). The hosted `msrv` job runs `cargo +1.89 check --locked --all-targets` and `cargo +1.89 test --locked --all-targets` on every push, and is `success` on `37516727455`. No API newer than what the file already used was introduced (`str::replace`, `format!`, `Path`, `Cow`) |

## 7. Verification actually run

Every command below was run on this tree; none is inferred.

```text
cargo fmt --all -- --check                                   pass
cargo clippy --all-targets --all-features -- -D warnings     pass
cargo test --all-targets --all-features                      318 + 36 + 4, 0 failed
python3 scripts/check-fixture-portability.py --self-test     pass (4 rules, both directions)
python3 scripts/check-fixture-portability.py                 pass (35 files clean)
python3 scripts/check-doc-citations.py --self-test           pass
python3 scripts/check-doc-citations.py                       pass (16 documents clean)
python3 scripts/check-release-contract.py (+ --self-test)    pass
python3 scripts/check-post-release-smoke-contract.py (+ --self-test)  pass
python3 scripts/check-staged-validation-contract.py (+ --self-test)   pass
python3 scripts/check-installer-contract.py (+ --self-test)  pass
python3 scripts/check-selector-qualification.py (+ --self-test)  pass
cargo run --features dev-tools --bin generate-docs -- --check   13 artifacts match
CI run 37516727455 on 0a7a596                                all 9 jobs success, incl. windows-latest
```

**Not run, deliberately:**

- `scripts/release-check.sh` — `AGENTS.md` §3 says it needs a clean tree, the
  `eggpack` binary and toolchain 1.89, and is not routine verification. This is
  not a release, and every documentation gate it would invoke was run
  individually above instead.
- `cargo package` / `cargo publish --dry-run` — no release is being made. The
  published crate is unchanged until the next release, and the CHANGELOG entry
  is `## [Unreleased]`, not a version heading.

## 8. Documentation consequences, and a finding this produced

C024 added 264 lines to `src/config.rs`, and `architecture/` cites that file by
line. Re-deriving the citations was therefore part of the work, not an
afterthought — and the re-derivation found that **the citations were already
rot before this change**:

- Every `src/config.rs` citation in `architecture/03` and `architecture/04`
  below `load` was **off by one** (e.g. `recency_seconds == 0` cited as
  `:97-101`, actually `:98-102`), and `cleanup.rs:1178` in `03` did not point at
  `compile_policy_globs` at all (it is `:1248`).
- `architecture/14-testing-and-verification.md`'s per-module table claimed
  **292** inline tests in a crate that declared **317** at the baseline: nine
  modules' rows were stale, and `domain.rs` was listed as having 0 tests while it
  has 1.
- `architecture/overview.md`'s stated total (23,068) did not equal the sum of
  its own rows (22,853).

`check-doc-citations.py` passes on all of these: it gates the *mechanically*
decidable half (the file exists, the line is in range, the range is ordered),
and a citation one line off satisfies all three. This is the semantic half
`AGENTS.md` §5 assigns to a reviewer, and it was not being reviewed.

**All of it was re-derived from `src/` and corrected**, per `AGENTS.md` §5 and
the rule that `overview.md` is the source of truth:

- `overview.md`: 23,068 → **23,357** lines; 317 → **320** inline tests;
  40.7% → **40.3%**; `config.rs` 1055/480 → **1332/528**; `workspace.rs` and
  `update.rs` row totals corrected (4510 → 4517, 2768 → 2773).
- `AGENTS.md`: the same `src/` total.
- `.skills/test-evidence/SKILL.md`: 266/265 → **320 declared / 318 on Linux**,
  with the per-lane counts corrected to 318 / 296 / 317 and the note that
  `#[cfg(unix)]` is true *on* macOS — the half the old text got wrong.
- `architecture/03`: every `config.rs` citation, plus the whole §8 test table,
  rebuilt for 19 tests with their current lines and what each one pins.
- `architecture/04`, `06`, `13`, `15`: the `config.rs` citations each of them
  carries.

**This is a finding, not a repair claim.** The guard's blind spot is unchanged
by this work: nothing in the repository now prevents the same drift next time,
and the numbers will rot again unless someone adds a check that re-derives
counts from `src/`. That is a proposal, not a plan; see §9.

## 9. Unresolved findings, classified

| # | Finding | Class | Disposition |
|---|---|---|---|
| 1 | `check-doc-citations.py` cannot detect a citation that resolves but points at the wrong statement — an off-by-one passes every rule it has | **Unresolved; no plan opened** | Recorded here. The semantic half is currently a reviewer's job and this change shows what that costs. Opening a plan is a judgement for the next milestone, not something this corrective decides for itself |
| 1a | **Three Windows-only refusals in one function, none of them visible by reading the code** | **Fixed here; no plan needed** | All three are in §4 with the run that found each. The durable answer is not a plan but a shape: the two platform-dependent decisions are parameters, so the Windows answer is asserted on every lane rather than only on a Windows runner |
| 1b | Nothing prevents the same Unix assumption from being written again in a function with no Windows lane | **Unresolved; structural** | `check-fixture-portability.py` guards fixtures, not production branches. `ci.yml` proves the Windows lane *ran*; it cannot prove a branch is exercised. Recorded, not solved |
| 2 | The inline-test count is maintained in **four** documents (`overview.md` §3, `14`, `.skills/test-evidence`, and this record's own §5) | **Unresolved; mitigated** | Re-derived here. Mitigated, not solved: four copies of one number is exactly the shape that rots |
| 3 | A `{` or `}` in a canonical directory name still blocks the rewrite, so such an `exclude` can silently under-match on any platform | **Accepted limitation, documented and tested** | Unchanged by this plan and now stated in `docs/USAGE.md` as a named consequence. Escaping it would require a bracket form of an alternation, which `globset` does not have |
| 4 | The Windows test needs an alias, and on Windows that means a junction made through `cmd` | **Accepted, with the failure loud** | `mklink /J` needs no elevation and fails loudly if it cannot run — it did, twice, for reasons of its own (§4). A test that skips would have been the false-green this repository keeps paying for |
| 5 | 0.2.1 and earlier remain affected on Windows | **Not fixable retroactively** | Recorded in `CHANGELOG.md` under `## [Unreleased]` with the affected versions named, and in `docs/TROUBLESHOOTING.md`'s "An `exclude` pattern does not match" with a workaround for anyone still on 0.2.1. The published 0.2.1 CHANGELOG text is **not** rewritten — it was an accurate description of 0.2.1's behaviour |

## 10. Effect on other plans

- **C023 §11.1 is discharged.** `c023-status.md` recorded it as "recorded, not
  fixed" and assigned it here. It is now fixed. The C023 record is left
  otherwise untouched — it is a closed record, and its history (the finding
  existed, was assigned, and was then closed elsewhere) is the correct history.
- **The registry has no open implementation plan left** once this closes. C023
  and M011C stay conditionally closed on their own separate evidence — a green
  *automatic* five-lane smoke — which this corrective neither supplies nor
  affects.
- **Nothing here unblocks M011C.** Its missing evidence is an automatic release
  smoke, produced by a release, not by a matching fix.
- **No release is implied.** `Cargo.toml` still reads `0.2.1` and the CHANGELOG
  entry is `## [Unreleased]`. Per `plans/003-planning-process.md`, a corrective
  that is fixed but not yet carried by a published release is closed as a
  *corrective*, not as a *shipped capability*; the fix reaches users when the
  next release is cut, and that release is a separate decision.

## 11. Disposition

**Closed.** All five acceptance criteria are met, with criterion 5 met by an
observed Windows lane rather than by an argument that one would have passed.
Criterion 1 is the only one that needed a platform this plan could not run on
directly, and it was covered three ways: a real Windows-lane test, an every-lane
assertion of the Windows answer, and a compile-check of the lane-only body.

It took four hosted runs to get there, and that is the substance of this record
rather than a footnote to it. The rewrite C024 exists to restore had **three**
Windows-only refusals; the first push fixed one, the second was found by the
lane and was a defect in the fix itself, the third was found by the lane again.
A reviewer reading the function would not have found them, and a Unix-only suite
could not have failed on them. The plan's own §3 said the defect was untested
because "the path is unreachable" — which was true of the *product* and false
of the *fix*: making the path reachable is what exposed the other two.

The unresolved findings in §9 are documentation-hygiene, structural and
pre-existing limitations, none of which bears on the acceptance criteria and none
of which is silently dropped: #3 and #4 are accepted and now documented, #5 is
disclosed to users on the versions it affects, #1a is fixed and captured as a
code shape, and #1, #1b and #2 are recorded for whoever next touches the
guards.

This record makes no claim that any release has been performed.