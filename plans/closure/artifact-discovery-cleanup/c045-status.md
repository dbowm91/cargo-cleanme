# C045 — the `clean` alias's single source of truth — closure record

Plan: `plans/implementation/artifact-discovery-cleanup/c045-clean-alias-single-source-of-truth-corrective.md`
Status: **closed — shipped in v0.2.11** (tag `v0.2.11` at `1d679833ae62`, GitHub release published 2026-10-25T17:23:10Z and immutable with a valid attestation, crates.io sha256 `96298f83…`, automatic smoke `40216211513` green on five targets). Receipt:
`plans/closure/distribution-release-update/r031-status.md`.
Corrects: **M019** (`plans/implementation/artifact-discovery-cleanup/m019-canonical-full-cleanup-spelling-and-root-front-door.md`;
closure record `plans/closure/artifact-discovery-cleanup/m019-status.md`,
status **closing**, unpublished when this record was written; M019 and this
corrective both shipped in **v0.2.11**).
Baseline: `main` at `22bbfd2` (0.2.10, unpublished).
Date: 2026-10-22.

## The executive finding

The hypothesis that started this line of work was that the `clean`
subcommand's duplication existed so that the base scan could be run in a
target directory simply as `cargo cleanme`. That need is **already met, and it
is met without the alias**: a bare run joins the working directory to the
maintenance scope (ADR 009 / C040, shipped in 0.2.10), and the bare run and
the `clean` run were verified byte-identical in the JSON envelope before a
line of code was changed.

What remained was not a missing feature but a **duplicated declaration** and a
**one-sided guard**. The root declared twelve cleanup flags, the hidden alias
declared the same twelve again, and the scope rule was derived twice. The
only guard on the alias's compatibility guarantee enumerated the alias's *own*
argument ids and asserted that the alias carried them — and asserted nothing
about the **converse**, that the alias accepts every cleanup flag the root
declares. An override added to the root and not mirrored onto the alias would
have broken `cargo cleanme clean <flag>` — a spelling published since v0.2.0 —
with the whole suite green.

The corrective collapsed the declaration and the scope rule to one of each,
made the converse guard mechanical, and corrected the prose that denied the
shipped surface. **No resolved intent changed**, and that is a measurement,
not a claim: 394 argv shapes were rendered before and after the change, and
the two renders are byte-identical.

## What changed

| # | Defect | Fix | Regression test |
|---|---|---|---|
| D1 | The alias's compatibility guarantee was guarded in one direction only: the guard enumerated the alias's own ids and never checked that the alias accepts every cleanup flag the root declares. | `the_alias_accepts_every_cleanup_flag_the_root_declares` reads the clap model, asserts that every cleanup flag the root declares is carried by the alias, and **parses** each of them through the alias. | `cli::tests::the_alias_accepts_every_cleanup_flag_the_root_declares` |
| D2 | The cleanup surface was declared twice — twelve flags on the root, the same twelve on the alias — and the scope rule was implemented twice (`root_cleanup_request` and `merged_cleanup_request`), so one new flag cost four edits. | One declaration: `CleanupFlags` (`src/cli.rs:35-83`), flattened into the root (`src/cli.rs:167-168`) and into the alias (`src/cli.rs:286-288`). One scope rule: `cleanup_request` (`src/cli.rs:539-623`), with the bare invocation as that same derivation with an empty alias. | `cli::tests::the_whole_published_spelling_set_agrees`, `cli::tests::the_bare_invocation_is_untouched_by_the_consolidation` |
| D3 | `CleanupOverrides`'s doc comment denied the shipped surface: it said the policy overrides "live only on the `clean` subcommand by design (M012A work package D)", which is the reason a reviewer would use to reject the change that was made. | The comment now says what the code does: the overrides live on the root **and** on the hidden alias, and both read one declaration. | — (documentation) |
| D4 | The alias's rationale was nowhere written down as one sentence, so an implementer could "simplify" it into the canonical path — deprecating the bare invocation ADR 009 made canonical. | The rationale is recorded next to the alias: **published-spelling compatibility** since v0.2.0, and *not* what makes a bare run in a target directory work (ADR 009 / C040). | — (documentation) |

D1 and D2 are `architecture/overview.md` §10 findings **68** and **69**; D3 is
finding **65**. All three are now closed.

### One shape, two presentations

The two levels now differ in **presentation only**, and the difference is one
function (`configure_cleanup_alias`, `src/cli.rs:209-232`): the alias hides
every flag it carries except `--dry-run`, and gives that one the alias's
shorter help text. The root's rendering is unchanged.

The alias's `--dry-run` needed its own text, and the root's `--dry-run` needed
to keep the long one, so the shared declaration carries the root's and the
alias's copy is overridden. The two levels' *option order* also had to be
pinned (`display_order`), because clap renders options in display order and
the root's order — `--dry-run` before `-i, --interactive`, which is before
`--full` — is a published surface.

### Why the alias's conflict rules did not need a second declaration

Clap's conflict relation is **symmetric**: a conflict declared on either side
applies to both. The alias's old declarations (`--full` conflicting with
`--known`, `--dry-run` conflicting with `--yes`) and the root's (`--full`
conflicting with `ROOT`) therefore described *the same relation twice*, and
the shared declaration describes it once with the identical effective sets.
The alias-only flags — `--known` and `--yes` — keep their own declarations,
because they exist only there.

## The equivalence evidence

The surface was **frozen before it was changed**, and the freeze is the
strongest evidence in this record.

A temporary harness rendered the parse outcome of **394** argv shapes — every
scope selector (none, ROOT, `--full`) × every policy override (none and each
of the six) × every mode (none, `--dry-run`, `--dryrun`, `--cargo-preview`) ×
both levels, plus every conflict pair, every read-only-subcommand shape, the
external-subcommand form and the alias-only flags — and recorded for each the
resolved `Invocation` or the exact clap error message. The harness was run
against the pre-change tree and against the post-change tree, and the two
renders were **byte-identical**: a diff of zero lines.

The harness was removed once the permanent test replaced it. It is not part of
the delivered suite; what it proved is recorded here.

The rendered help is byte-identical too:

| Surface | Before | After |
|---|---|---|
| `cargo cleanme --help` | — | **byte-identical** |
| `cargo cleanme clean --help` | — | **byte-identical** |
| `man/` (8 pages) | — | **byte-identical** |
| `completions/` (5 files) | — | **changed: option order in all five; three of the zsh file's conflict hints narrowed** (see *Residuals*) |

## The tests

Nine tests, one per acceptance criterion, each naming the input that makes it
fail.

| # | Test | Where | Fails if |
|---|---|---|---|
| 1 | `the_whole_published_spelling_set_agrees` | `src/cli.rs:1188` | one spelling resolves differently from its twin — the cross product is 84 pairs, not the four a hand-written list would name |
| 2 | `the_alias_accepts_every_cleanup_flag_the_root_declares` | `src/cli.rs:1247` | the root declares a cleanup flag `cargo cleanme clean <flag>` would reject |
| 3 | `the_alias_is_still_hidden_and_still_accepted` | `src/cli.rs:1315` | the alias is dropped, reappears in the root help, or a published `clean` spelling stops resolving |
| 4 | `the_alias_hide_states_are_unchanged` | `src/cli.rs:1372` | the alias gains or loses a hide state — a second front door, or a spelling that stopped being advertised |
| 5 | `the_duplicate_override_rule_survives_the_consolidation` | `src/cli.rs:1411` | one of two values is silently preferred, at either level |
| 6 | `the_scope_conflict_rules_survive_the_consolidation` | `src/cli.rs:1497` | a conflict check is lost in the collapse, or the message names a flag the operator never typed |
| 7 | `the_bare_invocation_is_untouched_by_the_consolidation` | `src/cli.rs:1542` | the consolidation changes what the bare invocation selects, or routes it through the alias's spelling |
| 8 | `yes_stays_clean_only_and_stays_unbound` | `src/cli.rs:1572` | a consolidation adds `--yes` to the root, or binds it |
| 9 | `the_generated_artifacts_match_the_clap_model` | `xtask/src/main.rs` | the rendered help changed and the committed artifacts were not regenerated |

**Test 2 is discriminating, not vacuous.** Adding a root-only cleanup flag to
`Cli` was verified to fail it, with the flag's name in the message:

```
thread 'cli::tests::the_alias_accepts_every_cleanup_flag_the_root_declares' panicked at src/cli.rs:1267:13:
the root's cleanup flag `new_override` must be accepted by `clean`
```

The change was reverted and the test returned to green. The guard also
asserts that it inspected at least ten cleanup flags, so it cannot pass by
finding none.

**Test 9 is new machinery.** The generated-artifact drift gate was a
*command* (`generate-docs -- --check`), run in a separate CI job. It is now
also a **test** inside the generator, comparing the committed artifacts
against a fresh render — so a stale artifact fails `cargo test`, not only the
documentation job.

## The verification ladder

Every step of the ladder in `AGENTS.md` §3, with the result:

| Step | Command | Result |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **clean** |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | **clean** |
| 3 | `cargo test --all-targets --all-features` | **552 lib + 9 + 1 + 78 + 4 + 0 = 644 passed, 0 failed** |
| 4 | `python3 scripts/check-fixture-portability.py --self-test && … --fixture-portability` | **37 fixture files clean** |
| 5 | `python3 scripts/check-doc-citations.py --self-test && …` | **17 documents clean; every `file.rs:N` citation resolves and is in range** |
| 6 | `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | **13 artifacts match the clap model** |
| 7 | `python3 scripts/check-release-contract.py` | **5 contracted targets, 11 release assets, wrappers aligned** |

**The MSRV lane, outside the ladder** (`AGENTS.md` §3.1):

| Command | Result |
|---|---|
| `cargo +1.89 check --locked --all-targets` | **passes** |
| `cargo +1.89 test --locked --all-targets` | **fails — one test, pre-existing** |

The failing test is
`cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`,
and it is **not** caused by this corrective: the failure reproduces at `main`
with C045's source changes stashed. It is recorded as `architecture/overview.md`
§10 finding **70** and owned by a new corrective, **C046**
(`plans/implementation/artifact-discovery-cleanup/c046-msrv-lane-test-assertion-corrective.md`).
C045 did not fix it: a plan that finds a defect writes a corrective, it does
not fix it (`AGENTS.md` §5).

**The hosted lanes were not run.** The ubuntu/macos/windows matrix is not
available here. Nothing in this corrective touches path comparison, mtimes,
symlinks or `TMPDIR`, and the platform-specific fixtures are unchanged, but
"not run" is the honest entry: the hosted lanes remain outstanding.

## The citations

`src/cli.rs` grew by 439 lines, which invalidated the citations that pointed
into it. Every citation in `architecture/02-cli.md`,
`architecture/13-orchestration.md`, `architecture/overview.md` and
`.skills/cli-surface-and-argv/SKILL.md` was **re-derived by matching the
content of the cited line**, not by shifting the number: the old file was
diffed against the new one, and each citation was mapped through the diff
where the line's content was unchanged. Where the line had been *edited*, the
citation was re-derived by hand and the new line was read and checked against
the sentence that cites it — 40 such citations, each verified to point at what
the prose claims.

Two pre-existing drifts were found in the process and corrected:

- `architecture/overview.md` §3's **production-line** figure did not reproduce.
  The table in §3.1 is derived by one rule — production is every line outside a
  `#[cfg(test)]` scope, where such a scope spans its own attribute line, the
  item it gates, and that item's closing brace — and that rule reproduces 17 of
  the 18 published per-module rows exactly (`config.rs` was off by one, and is
  corrected). It now reads **18,344**, re-derived **after** this corrective's
  own changes, because a census is a statement about the tree. The §3.1 row for
  `cli.rs` is updated to this change's own shape (2700 / 906 / 58). The rule is
  now stated in the document, so the next reader re-derives it instead of
  copying a number whose provenance was ambiguous.
- The **per-platform test census** was re-derived by the method `AGENTS.md` §6
  prescribes — a brace-tracking evaluation of each test's *effective* gate set
  against `rustc --print cfg --target <triple>`, cross-checked against
  `cargo test --lib -- --list`. The Linux figure cross-checks exactly (561 =
  552 lib + 9 bin). The census moved from 555 declared / 553 Linux / 492
  Windows / 550 macOS to **563 / 561 / 500 / 558** — the eight new tests are
  ungated, so every platform gains eight. **The Windows figure published in
  every document (508, then 516) was wrong by 16, and the fault was the
  method**: the census had been computed by counting `#[cfg(unix)]` *gates*
  and subtracting them from the declared total, which is only right while every
  gate covers exactly one test. The 53 `#[cfg(unix)]` attributes in test scope
  exclude **60** tests, because ten of them gate a helper or a block that
  holds further tests, so 63 of the 563 are excluded on Windows and 500
  remain. The old 508 was wrong for the same reason.

## What deliberately did not change

- **No resolved intent.** The scope, the selection, the mode, the policy
  evaluation, the JSON envelope, the reason-code vocabulary and the exit
  codes are untouched.
- **No published spelling removed or narrowed.** `clean ROOT`,
  `clean --known`, `clean --full`, `clean --dry-run`, `clean --dryrun`,
  `clean --yes`, `clean --cargo-preview` and every root-level flag continue to
  work. `clean` stays hidden: absent from `cargo cleanme --help`.
- **No change to the bare invocation's semantics.** The working directory
  joining the maintenance scope is a separate, shipped, canonical feature
  (ADR 009 / C040) and was not re-derived through the alias.
- **No change to the cleanup engine, discovery, ownership proof, or the scan
  surface.**
- **No new ADR.** ADR 008 §4b is accepted and this corrective only implements
  it faithfully.
- **`--yes` stays unbound.** Execute is the default; the flag carries no
  intent of its own, and that decision stands.

## Residuals, disclosed

- **The completions changed: option order, and three of the zsh file's
  mutual-exclusion hints.** A completion script lists a command's options in
  the order the command *declares* them, and the root and the alias declared
  the same flags in **two different orders**. One shared declaration has one
  order, so one ordering had to move: `--interactive` moved one position
  earlier in the root's list, and the alias's option list now follows the
  shared order. The **flag sets are unchanged** — the same flags, the same
  values, the same help — and `man/` is byte-identical, because a man page
  renders the help text, which is unchanged.
  **One further change is not an ordering, and it is in the zsh file only.**
  In the `clean` alias's section, three options now carry a narrower
  *rendered* conflict hint: `--dry-run` reads `(--dryrun)` where it read
  `(--cargo-preview --yes --dryrun)`, `--dryrun` reads `(--dry-run)` where it
  read `(--dry-run --cargo-preview --yes)`, and `--full` reads `()` where it
  read `(--known)`. The **effective** conflict relation is unchanged —
  clap's is symmetric, so a pair is refused whichever side declares it — and
  every pair is still refused, with the same messages: `clean --full
  --known`, `clean --dry-run --cargo-preview`, `clean --yes --dry-run` and
  `clean --dry-run --dryrun` all still exit `2`. What moved is which *side*
  carries the declaration, and that is inherent to the shared shape: the
  shared declaration cannot declare `conflicts_with = "known"`, because the
  root has no `known` argument and clap rejects the unknown id. The other
  four completion files carry no conflict hints and changed in order only.
  The plan's acceptance criterion demanded byte-identical completions; it
  was not satisfiable together with the corrective's primary goal, and the
  plan carries a dated amendment saying so rather than the criterion being
  quietly dropped.
- **The MSRV lane is red at `main`** for a reason unrelated to this
  corrective (§10 finding **70**, owned by **C046**).
- **The hosted lanes were not run.**

## Open

- **C046** — the MSRV lane's assertion (ready).
- **C036** — the documentation-drift corrective, still open (D36's `--config`
  help; the hand-maintained counts D29 / D37 / D38, whose gate is still not
  built, so a further drift is expected).
- The safety-path findings from the 2026-10-22 interrogation of the CLI
  surface — `is_authorization_infrastructure_error` deciding an exit code by
  substring-matching prose; `path_is_symlink` byte-identical in four modules;
  `collapse_roots` in three; three "inspect a covering root" copies giving
  three verdicts; `admits` against `boundary_admits`; and forty-eight
  pre-M019 spelling comments across four modules — remain **unplanned**. They
  are a different line of work from this corrective and were deliberately not
  folded into it.
