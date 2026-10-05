# Artifact Discovery and Cleanup C019 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c019-exact-unignore-sibling-containment-corrective.md`](../../implementation/artifact-discovery-cleanup/c019-exact-unignore-sibling-containment-corrective.md)

Corrects: `002-fast-project-discovery-and-scope-filters.md` and
`plans/closure/artifact-discovery-cleanup/002-status.md` (that record is
preserved unedited; this one corrects its forward).

Implementation commit: `f91c6ca` (`src/discovery.rs` only)
Release tag carrying the correction: **none yet** — see §11.

## 1. Executive finding

A literal `scan.ignore` rule names a directory, not its children. The
pre-C019 walk decided each directory from its own text alone, so it could
not tell that `/archive/other` was still underneath an ignored `/archive`.
An `unignore` entry anywhere below the ancestor forced the walk back in, and
the unrelated siblings came in with it.

Measured on `main` at `c448154`, release 0.1.6, with the plan's own canonical
configuration and two real projects:

```toml
[scan]
ignore   = ["/tmp/c019-base-Sjlu/projects/archive"]
unignore = ["/tmp/c019-base-Sjlu/projects/archive/keep"]
```

```console
$ HOME=$W cargo-cleanme scan --config $W/config.toml --format json --no-progress
scope: routine
discovered_manifests: 2      # keep AND other — one too many
```

Same tree, same binary, after the fix, with two exact exceptions
(`keep` and `nested/keep-b`) and a sibling `other` holding its own
4 KiB artifact directory:

```console
$ HOME=$W cargo-cleanme scan --config $W/config.toml --format json --no-progress --stats
scope: routine | discovered: 2 | workspaces: 2
pruned_directories=3  target_vcs_prunes=2  user_ignore_prunes=1
```

`discovered: 2` is `keep` and `nested/keep-b` — the two exact exceptions, and
only those. `other` is refused. Both survivors resolve as ordinary Cargo
workspaces (`workspaces=2`), which is the §12 requirement that the correction
removes a candidate earlier without creating a shortcut around later proof.
`groups: 0` in that run is the documented recency guard, not a regression: the
fixture was written seconds earlier, so its artifacts are *active* and
protected.

## 2. Why M002's verification missed it

M002 required all three of exact unignore, ignored-ancestor reachability, and
ignored-sibling pruning, and its closure recorded the first as a single "Pass"
(`002-status.md:17`). The two surviving tests both use the *wildcard* shape:

- `discovery_reaches_unignored_project_without_entering_ignored_sibling` —
  `ignore = <archive>/*`
- `filters_ignored_parent_keeps_exception_route` — `/tmp/archive/*`

With `/*`, the sibling matches the pattern **directly**, so the old stateless
test pruned it. Those tests prove a directly-matched sibling is refused; they
cannot prove that exclusion *inherited* from a literal ancestor survives. The
closure overclaimed exact-unignore coverage. Per the plan, M002 history is left
untouched and corrected here.

## 3. The fix

`Filters::ignored` and `Filters::exception_below` are deleted. One decision
replaces them.

### Work package A — an explicit disposition

`Filters::disposition` (`src/discovery.rs:68`) returns one of four states, and
`Disposition` (`:101`) is what makes the two excluded states distinguishable —
they lead to opposite walk decisions:

| State | Meaning | Walk |
|---|---|---|
| `Included` | no rule names this directory or anything above it | descend |
| `Pruned` | excluded here and no exception needs this directory | **refuse** |
| `PassThrough` | excluded, but an unignore path is at or below it | descend **only** as the route to that exception |
| `ReIncluded` | at or below an exact unignore path | descend, and clear exclusion for the subtree |

Inherited state is proven by `Filters::inherits_ignore` (`:52`), which tests the
path **and every ancestor** against the glob set. Cost is bounded by path depth
× rule count, never by tree size. The empty component a relative path would
contribute is skipped, so a bare `*` cannot match it.

### Work package B — traversal carries the exclusion

Both descent predicates now refuse only `Pruned` (`:542-550` global, `:831`
sequential). The configured pattern and the exact exception remain observable
as authored: the user's literal `/archive` is never rewritten into an
undocumented `/archive/**`.

### Work package C — the counter follows the real prune decision

`user_ignore_prunes` counts `Disposition::Pruned` and nothing else (`:649-653`,
`:873`). This is a deliberate counter-contract change and it fixed a false
count: the pre-C019 sequential counter used `filters.ignored(&path)` without the
exception check, so it counted exception **ancestors** as skips the walk had
not performed. Both sites now read one disposition, which is why
`directories_pruned == Σ` the five specific counters still holds.

One asymmetry was removed on purpose. The four internal reasons keep their
`entry.depth > 0` guard, but the user-ignore reason no longer does. Since C019 a
scan root can inherit exclusion from a literal ignored ancestor and be refused
outright; a scan that returned nothing while reporting zero prunes would be
unexplainable. `an_explicit_unignore_of_the_scan_root_prunes_the_whole_root_visibly`
pins this, and it is the only reason for which the counter and the descend
predicate disagree about the root.

### Work package D — documentation

`docs/USAGE.md` no longer presents `/**` as required semantics; the literal form
is now documented as the intended meaning, with the 0.1.6-and-earlier behaviour
and the `/**`/`/*` workaround retained as historical. `docs/TROUBLESHOOTING.md`
carries the same split. `CHANGELOG.md` gains an `## [Unreleased]` entry. No
version number is named for the fix, because the release that carries it does
not exist yet.

## 4. Premise-negative evidence

The traversal decision was reverted to its pre-C019 stateless form — `directly_
ignored(p)` instead of `inherits_ignore(p)` — and the new tests were run against
it. **9 of 31** discovery tests failed; the mandatory one:

```text
---- literal_ignored_ancestor_does_not_readmit_ignored_siblings ----
  left:  ["…/archive/keep/Cargo.toml", "…/archive/other/Cargo.toml"]
  right: ["…/archive/keep/Cargo.toml"]
```

and the counter contract:

```text
---- user_ignore_prune_counts_only_the_subtrees_the_walk_refused ----
  left: 0
  right: 2
```

The end-to-end test is a premise negative too, with the same left/right pair.
The profile is the important part: the tests that are *not* about the literal
case — `/*` and `/**` behaviour, explicit-root bypass, symlink refusal, and all
19 pre-existing discovery tests — stayed **green**. These are not tests that
fail for everything.

The reversion was applied by patching one line, so the premise negative
isolates the traversal decision rather than the test scaffolding.

## 5. Regression matrix

| Case | Configuration | Expected | Test |
|---|---|---|---|
| Literal ignored ancestor | `ignore=archive`, `unignore=archive/keep` | `keep` only | `literal_ignored_ancestor_does_not_readmit_ignored_siblings` |
| Wildcard | `ignore=archive/*` | `keep` only | `wildcard_and_recursive_ignore_patterns_keep_their_documented_behavior` |
| Recursive | `ignore=archive/**` | `keep` subtree only | same test |
| Multiple exceptions | `keep-a` + `nested/keep-b` | both, siblings pruned | `several_exceptions_under_one_ignored_ancestor_are_each_reachable` |
| Nested exception, 3 deep | `a/b/c/keep` | only that, siblings at every level pruned | `a_deeply_nested_exception_never_visits_a_pruned_siblings_subtree` |
| Re-included subtree | `keep` + `keep/nested` | both | `an_exact_unignore_reincludes_the_whole_subtree_at_its_root` |
| Explicit scope bypass | `scan <archive>` | both | `an_explicit_root_still_bypasses_a_literal_ignore_entirely` |
| Internal-prune precedence | unignore aimed at `target` and `.git` | both still refused | `an_exact_unignore_does_not_reopen_an_internal_prune` |
| Symlink precedence | unignore aimed at a symlink | still refused | `an_exact_unignore_does_not_reopen_a_symlinked_directory` (unix) |
| Prune-before-read | 200 ignored siblings | `visited_entries == 204` | `a_broad_ignored_region_is_pruned_before_its_siblings_are_read` |
| Root inherits exclusion | root under a literal ignored ancestor | refused **and** counted | `an_explicit_unignore_of_the_scan_root_prunes_the_whole_root_visibly` |
| End to end | full production pipeline | 1 workspace, `PrivateBounded` | `tests/end_to_end.rs` |

The nested-exception test is the strongest containment claim in the set: each
pruned sibling holds its own project, so finding any of them would prove the
subtree was walked rather than refused. The 200-sibling test is the cost claim
(plan §14): the traversal total is exactly `SIBLINGS + 4`, so a refused sibling
yields its directory entry and nothing beneath it.

Cross-platform: every fixture is built from `tempdir()` with `Path::join` and is
canonicalised, and no new test is POSIX-shaped — the only gated one is the
symlink case, which is `#[cfg(unix)]` by nature. `check-fixture-portability.py`
reports 35 fixture files clean. Hosted macOS and Windows lanes are the proof,
not an assumption.

## 6. Cleanup-safety regression evidence

`tests/end_to_end.rs::a_literal_ignored_ancestor_yields_one_workspace_while_an_explicit_root_yields_two`
drives the real production chain — `discover_manifests` → `resolve_workspaces` →
`build_groups` → `analyze_groups` → `report::render` — and asserts:

- the ignored sibling is not a discovered manifest, so it cannot become a
  candidate, let alone be authorized;
- the surviving project reaches `OutputOwnershipClass::PrivateBounded`, the
  same class the explicit-scope run gives it;
- an explicit root still returns both projects, both `PrivateBounded`;
- both projects' `target/debug/app` files still exist afterwards — C019 removed
  a candidate from scope and deleted nothing.

No ownership, authorization, activity, marker, or final-proof code was touched.
`src/discovery.rs` is the only changed source file; the diff cannot reach the
cleanup module. The full suite is the regression evidence for everything
downstream: 265 lib + 9 CLI contract + 2 end-to-end.

## 7. Operational-closure dependency

The plan made green hosted evidence a precondition, because "only the C019
tests passed" is not acceptable from an already-red baseline. At `c448154`
**three hosted workflows were red**, and all three reasons were pre-existing —
none was introduced by C019. Each was verified pre-existing against the
baseline rather than assumed.

### 7.1 README/release-contract drift — reconciled here

`scripts/check-release-contract.py` was red: the README documented neither a
prebuilt target matrix, nor the five contracted triples, nor the Cargo-only
`armv7-unknown-linux-gnueabihf`. This single drift was failing **two** jobs
(`CI / generated-docs` and `Release drift guard / Eggpack drift + contract`) plus
`CI / installers (ubuntu-latest)`, which reported
`check-installer-contract: README does not document the Cargo-only armv7
family`.

The drift is **reconciled** here, which the plan explicitly permits as the
alternative to fixing it. `README.md` gained a `### Prebuilt release targets`
section naming all five triples and the Cargo-only host, compiled from
`release/eggpack/distribution.toml` and pointing at `docs/INSTALLING.md` as the
longer form. The glibc floor is stated as **not yet claimed**, satisfying the
checker's non-claim rule.

This is a small, factual documentation addition, not a contract change: the
content was already authoritative in `docs/INSTALLING.md:104-127` and in
`distribution.toml`. It is called out because the plan listed the README drift
as a non-goal, and a reader deserves to know one non-goal was crossed and why.

### 7.2 The M011D qualification gate had never run — fixed as its own corrective

`.github/workflows/qualify-cargo-selectors.yml` passed its matrix to
`dtolnay/rust-toolchain` as a plural `toolchains:` input. That action declares a
single `toolchain:` input; GitHub Actions does not reject an unknown `with:`
key, so the action received an empty value and exited with
`'toolchain' is a required input`. Both jobs failed that way on run
`37328157746`, and the workflow had been red on every push since it landed.

**This is the failure mode M011D was created to prevent, reproduced by M011D
itself**: a gate that cannot run reports no drift, which is indistinguishable
from a gate that finds none. C019's plan made a green baseline a precondition,
so this had to be fixed to close honestly. It is committed separately
(`7973ed2`) against M011D, with an addendum on that closure record, because the
gate and its evidence are M011D's.

### 7.3 A C018 fixture whose premise does not hold on APFS — fixed as its own corrective

`update::tests::a_non_utf8_executable_name_in_a_cargo_root_is_unprovable` was
gated `#[cfg(unix)]`, but its premise is that a file name which is not valid
UTF-8 can exist on disk. APFS rejects one, so `CI / checks (macos-latest)` failed
at `std::fs::write` — before `classify_provenance_in` ran and before any
provenance assertion executed. The lane reported a product failure for a premise
the product never touched. Committed separately (`7a4dc5e`) with an addendum on
C018's record.

`discovery.rs` already had the identical fixture correctly narrowed to
`#[cfg(target_os = "linux")]`. The convention existed in this repository; the
C018 test did not follow it.

`check-fixture-portability.py` exists to catch precisely this class and did not,
because it only knew about path separators and POSIX shebangs. It gained a third
rule — a function that builds a raw-byte name **and writes it** must be gated to
Linux, not `unix` — with four self-test cases. Requiring a write in the same
function matters: the first version of the rule flagged
`report.rs::report_escapes_non_utf8_paths`, which builds a raw-byte `PathBuf` in
memory and formats it, which is genuinely portable. The rule fires on the
reverted `update.rs` gate and stays quiet on the fixed tree.

## 8. Local verification

Run on this host, on the implementation commit.

```text
cargo fmt --all -- --check                                     PASS
cargo clippy --all-targets --all-features -- -D warnings       PASS
cargo test --all-targets --all-features   265 lib + 9 + 2      PASS
cargo test --doc                                                0 tests, PASS
cargo +1.89 check --locked --all-targets                        PASS
cargo +1.89 test  --locked --all-targets   265 lib + 9 + 2      PASS
python3 scripts/check-fixture-portability.py --self-test         PASS
python3 scripts/check-fixture-portability.py     35 files clean PASS
python3 scripts/check-doc-citations.py --self-test               PASS
python3 scripts/check-doc-citations.py        16 documents clean PASS
python3 scripts/check-release-contract.py     5 targets aligned PASS
```

## 9. Hosted CI

Two workflows, both green on `7a4dc5e` (branch `c019-unignore-containment`).

Run `37334039683` — `CI`:

| Lane | Result |
|---|---|
| `checks (ubuntu-latest)` | **success** |
| `checks (macos-latest)` | **success** |
| `checks (windows-latest)` | **success** |
| `msrv` (1.89) | **success** |
| `installers` (ubuntu / macos / windows) | **success**, all three |
| `generated-docs` | **success** |
| `benchmark` | **success** |

Run `37334039970` — `Cargo selector qualification`: `Supported matrix`
**success**, `Exploratory current stable` **success**.

For comparison, the same workflows on `main` at `c448154` were red:
`CI` failed `generated-docs`, `checks (macos-latest)`, and
`installers (ubuntu-latest)`; `Cargo selector qualification` failed both jobs;
`Release drift guard` failed `Eggpack drift + contract`.

### 9.1 `main` after the merge

Merged to `main` as `9b4df98`. All three workflows are green there, which is
the first time that has been true on this branch:

| Workflow | Run | Result |
|---|---|---|
| `CI` (10 jobs, incl. ubuntu/macos/windows and `msrv`) | `37334733986` | **success** |
| `Cargo selector qualification` | `37334733493` | **success** |
| `Release drift guard` | `37334733502` | **success** |

`scripts/release-check.sh` was also run against the clean merged tree and
passed end to end, including packaging and a publish dry run.

### 9.2 The qualification gate is no longer a paper gate

The point of §7.2 is only worth making if the fixed gate produces real evidence,
so this was checked rather than assumed. The hosted log:

```text
qualify-cargo-selectors: 9 toolchain(s) to qualify
evidence: 79 assertion(s) across 9 toolchain(s) -> evidence/qualification.json
  1.89.0: pass   1.90.0: pass   1.91.1: pass   1.92.0: pass   1.93.1: pass
  1.94.1: pass   1.95.0: pass   1.98.1: pass   1.99.0: pass
qualify-cargo-selectors: passed; every claimed capability was asserted against real Cargo
```

Nine toolchains were installed by nine separate action invocations — each step
shows its own `rustc +<version> --version --verbose` — and the assertion count
matches the 79 recorded locally in M011D's closure. A green lane that printed
"0 toolchains" would have been the same false green in a new place.

## 10. Documentation and version disposition

| Surface | State |
|---|---|
| `docs/USAGE.md` | literal form documented as the intended meaning; 0.1.6-and-earlier behaviour and the `/**` workaround retained as historical |
| `docs/TROUBLESHOOTING.md` | defect entry rewritten as a fixed-after-0.1.6 note, with a three-row table covering both forms |
| `CHANGELOG.md` | `## [Unreleased]` entry naming the defect, the mechanism, the affected versions, and the unchanged downstream gates |
| `README.md` | prebuilt target matrix added (§7) |
| Released behaviour | **unchanged**. 0.1.6 and earlier still exhibit the broader behaviour, and the docs say so |

No version number is claimed for the fix. The next release from `main` is the
first covered by it.

## 11. Release status

The correction is on `main` and **unreleased**. Unlike C016 and C017, C019
needs no published artifact to prove itself: the defect is a scope-selection
error observable entirely within the repository, and every acceptance criterion
is met by in-repo and hosted evidence. Publication is therefore not a
precondition for closure here — but it *is* a precondition for users seeing the
fix, and 0.1.6 remains affected until then.

## 12. Unresolved findings

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | `architecture/14-testing-and-verification.md` §5 characterises 4 of the 17 scripts, and its `check-release-contract.py` row describes a contract M010A replaced (a list of required files, not a comparison against `release/eggpack/distribution.toml`) | low | **Open — needs a corrective.** Not repaired here: it is unrelated documentation drift, and the guards it under-describes are wired into CI and each take a `--self-test`. The §2 note now says so rather than claiming coverage the table does not have. |
| 2 | The published 0.1.6 binary retains the over-broad behaviour | medium | **Open by nature.** Fixes in the next release. Documented in all three user-facing surfaces. |
| 3 | `staging_is_cleaned_up_on_success_and_on_failure` (`cleanup.rs`) flaked once under parallel execution during the preceding milestone's verification and passed on 3 reruns | low | **Open, pre-existing, not C019's.** Untouched here. |
| 4 | `check-fixture-portability.py` could not see a raw-byte-name fixture, so it missed the C018 macOS failure (§7.3) | low | **Closed in this pass** — third rule added, verified in both directions. |

No medium-or-higher filter-scope finding remains open in the code.

## 13. Defects found in C019's own work

Recorded because the repository's own evidence work has a poor record of
repairing what it finds, and because these were all *test* defects rather than
implementation defects:

1. Four of the new tests failed on first run. All four were bugs in the tests,
   not the fix: a wildcard pattern built as `<archive>/archive/*`; two
   traversal totals miscounted by hand (8 for 10, and `SIBLINGS + 2` for
   `SIBLINGS + 4`); and a symlink-precedence fixture whose target directory
   sat *inside* the scan root, so the walk reached it directly and the
   assertion was not about the symlink at all. The last one is the dangerous
   shape — it would have passed for the wrong reason if the target had been
   reachable another way.
2. An intermediate version of the end-to-end test asserted `json["schema_version"] == 1`
   on an empty report. It was a placeholder that tested nothing; it was replaced
   with a real ownership-class assertion.
3. Three of the `architecture/05-discovery.md` citations written by hand were
   wrong on first pass (`:592` for the counter priority chain, `:353-358` for
   the sequential `ScanCounters` literal, `:1035` for the invariant assertion).
   They were found by resolving each one and reading the line, not by trusting
   the checker — `check-doc-citations.py` passed throughout, because it only
   decides the mechanical half.

## 14. Citation repair

`src/discovery.rs` grew by 496 lines, which invalidated every downstream
citation in 8 documents. Rather than shift numbers, the old and new files were
compared by content with `difflib`: **1352 of 1370 old lines map exactly**, and
the 18 that do not are precisely the lines this change edited. 66 citations were
rewritten from that map, and the 7 that reference the deleted
`Filters::ignored`/`exception_below` were rewritten by hand, because the prose
named methods that no longer exist.

Counts were recomputed from `src/` rather than copied: `src/` is 19,467 lines,
266 inline `#[test]` (265 run on Linux, 246 on Windows and macOS), 17 scripts.

## 15. Acceptance criteria

| Criterion | Status |
|---|---|
| Literal ignored ancestor discovers only the exception subtree | met — §1 live, §5 premise negative |
| Premise-negative demonstrably fails against the baseline | met — §4, 9 lib + 1 e2e |
| `/*` and `/**` retain intended behaviour | met — §5 |
| Multiple and nested exceptions do not admit unrelated siblings | met — §5 |
| Exact unignore does not override internal safety prunes | met — §5, including `target`, `.git`, and symlink |
| Explicit-root bypass unchanged | met — §5, §6 |
| User-ignore instrumentation truthful | met — §3 WP-C; the pre-C019 counter also over-counted |
| Cleanup authority, ownership, freshness proof not broadened | met — §6; `src/discovery.rs` is the only source change |
| Documentation distinguishes historical releases from corrected behaviour | met — §10 |
| Linux/macOS/Windows hosted CI and Rust 1.89 green | met — §9 |
| No medium-or-higher filter-scope finding remains | met — §12 |

## 16. Disposition

**Closed.**

The correction is complete, its premise negative is demonstrated rather than
asserted, the downstream safety gates are provably untouched, and the
documentation no longer tells users to work around a defect that no longer
exists. The fix is unreleased, which §11 states plainly: 0.1.6 and earlier keep
the broader behaviour until the next release from `main`, and finding 2 in §12
stays open on exactly that account.

One low-severity documentation finding (§12 #1) is handed forward as an open
corrective rather than absorbed here.
