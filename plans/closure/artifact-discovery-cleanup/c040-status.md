# C040 — Working-Directory Scope: closure record

Status: **closed — shipped in v0.2.10** (tag `v0.2.10` at `270cceb2be31`,
GitHub release published 2026-10-21T00:35:53Z and immutable with a valid
attestation, crates.io sha256 `0f2e471b…`, automatic smoke `39522277814`
green on five targets). Receipt:
`plans/closure/distribution-release-update/r030-status.md`.

Plan: `plans/implementation/artifact-discovery-cleanup/c040-working-directory-scope-corrective.md`

Decision: `plans/adr/009-working-directory-joins-the-maintenance-scope.md`
(ADR 009, accepted 2026-10-20, owner-directed).

Repository baseline: `main` at `b4de81f` (v0.2.9, unpublished; latest
published tag `v0.2.8`).

Implementation: `c9f7784` (`src/policy.rs`, `src/workspace.rs`,
`src/main.rs`, `src/cli.rs`, `xtask/src/main.rs`, the regenerated root man
page, and the tests).

Hosted-lane fixes: `cc27d5f` (the macOS path canonicalization, the Windows
symlink gate, the regression test that reproduces the macOS condition
anywhere, and the re-derived census).

Published: **v0.2.10**, tag `v0.2.10` at `270cceb2be31`, GitHub release
published 2026-10-21T00:35:53Z (immutable, valid attestation), crates.io
sha256 `0f2e471bcdf53b4c108b0ed0c1532fa30277b38bdb88854493f5b6da9248cd0b`,
automatic smoke `39522277814` green on five target lanes.

## The executive finding

The maintenance spelling had a **scope that could not contain the thing the
operator was pointing at**.

The working directory was applied as an `Ancestry` *filter* — a predicate
over the resolved cleanup units — and the resolved units came from exactly
the resolved roots. So the filter could only ever select from a universe the
working directory was not in. Stand anywhere the machine's known roots did
not reach, and the run had nothing to filter: it was a successful no-op over
an empty set, with the exit code `0` that says "there was nothing to clean".

The measurement, on the machine that prompted this, in the same directory at
the same moment:

| Spelling | Result |
|---|---|
| `cargo cleanme --dry-run` | **0 would-clean**, exit 0 |
| `cargo cleanme --dry-run clean ~/.codex` | **4 units, 107.15 GiB** |

Four workspaces, 107 GiB of reclaimable build output, and the front door
could not see any of it because the front door's own position was not in the
set it was filtering.

The root cause was a composition error, not a missing feature. ADR 006 made
the working directory an **anchor** — a *selection* filter applied after
classification — and it did so deliberately, because narrowing the
*discovery* universe would hide the out-of-scope claimants of a shared target
and promote `shared` bytes to `private`, the only class that authorizes
deletion. That reasoning was correct and remains correct. What was missing
is the other half: the anchor could filter, but nothing had ever put the
operator's position *into* the universe.

ADR 009 puts it in — as an **additional** root, never as a replacement.

## What changed

### 1. The working directory joins the maintenance scope (ADR 009)

`src/main.rs`, three functions and one call site:

- `working_directory_scope_paths` (`main.rs:882-889`) reads the working
  directory with the *same spelling the anchor uses* — `std::env::current_dir`,
  unresolved — so the scope root and the anchor agree on one path. An
  unreadable working directory contributes no root at all; the anchor's own
  resolution is what turns that into a usage error, so the two cannot
  disagree about whether the operator has a position.
- `working_directory_entries` (`main.rs:897-904`) turns those roots into
  candidates carrying `RootProvenance::Operator`.
- `working_directory_is_unreached` (`main.rs:931-938`) is the exclusion
  rule, and `roots_are_related` (`main.rs:941-946`) is its comparison:
  canonical-or-raw on both sides, so a symlinked prefix (macOS `/var` vs
  `/private/var`) can neither make two unrelated roots look related nor make
  related ones look unrelated.
- The `Maintenance` arm of `resolve_cleanup_roots` (`main.rs:850-861`)
  contributes them, and `admit_automatic` (`main.rs:952-1006`) classifies the
  Routine candidates **first**, asks the exclusion question, and only then
  classifies the operator's candidates and *extends* the admitted set
  (`main.rs:968-977`).

The decision is made against the **admitted** routine roots, so a root the
maintainability rule refuses — an agent worktree, say — does not count as
the Routine scope reaching the operator's directory.

### 2. The enclosing workspace is Cargo's answer, not ours

`src/workspace.rs`:

- `enclosing_workspace_root` (`workspace.rs:617-635`) runs
  `cargo locate-project --workspace` **in** the operator's directory, with
  **no** `--manifest-path`. The child's working directory is the operator's
  directory and Cargo walks up from there to the nearest manifest, so the
  answer is "the workspace that owns the place I am standing".
  `--manifest-path` would name a manifest to report on rather than a place
  to search from, and would answer for a manifest instead of for the
  operator's position.
- `working_directory_scope_roots` (`workspace.rs:655-664`) returns two roots:
  the enclosing workspace, when there is one, and the directory itself.

The enclosing workspace is **not optional**. A member directory's manifest
sits *above* the member, so a root at the member cannot discover it, and an
operator standing in `workspace/crates/member/src` expects that workspace's
output to be reclaimed exactly as `cargo clean` would. The anchor already
admits that workspace in both directions; this is the discovery half of the
same rule.

Every failure mode — a transport error, a Cargo refusal, an unparseable
answer — yields "no enclosing workspace", and the working directory remains a
scope root on its own. The worst case is therefore that the enclosing
workspace is not cleaned, never that something is cleaned without proof.

### 3. Operator provenance (I4)

`src/policy.rs`:

- `RootProvenance::Operator` (`policy.rs:100`) is the third variant. The
  operator chose these roots by standing where they stand, so the
  maintainability rule does not apply to them: that rule stops the tool from
  *choosing* a transient origin on its own, and `clean ROOT` already permits
  the same subtree. **Every other defect keeps the automatic root's
  handling** — a symlink blocks the scope rather than being followed, an
  absent or non-directory root is omitted, an indeterminate filesystem
  failure blocks. A scope that quietly reduced itself is worse than one that
  stops.
- `AutomaticOmission::OperatorUnavailable` / `OperatorNonDirectory`
  (`policy.rs:119-121`) carry the cause in the variant rather than reusing
  `NotFound` / `NonDirectory`. The pre-spawn recheck reads the variant to
  know which premise must still hold; the diagnostic reads it to name the
  root truthfully. An operator root was never a *learned* one, and saying it
  was would misreport what the tool did.
- `omission_diagnostics` groups by `(reason, noun)` (`policy.rs:166-210`), so
  the published **"learned root(s)"** wording is byte-identical to what it
  was before ADR 009 and an operator root's omission reads "operator
  root(s)". A closed record's quoted message cannot be invalidated by adding
  a bucket.

### 4. The `long_about`, and the man page that can no longer disagree with it

The root `long_about` said that *"`cargo cleanme scan` with no ROOT is the
full read-only machine reconciliation"* — which has been a clap **usage
error** since M014 (`scan` requires one of ROOT / `--known` / `--all`). It
now says that `scan` **needs a scope**, and that `--all` is the full
read-only machine reconciliation (`src/cli.rs:29-32`).

The reason the lie survived is worth recording, because it is a structural
gap and not a typo. The root man page did **not** carry `cli.rs`'s
`long_about` at all: the generator *overwrote* it with its own `INVOCATION`
block built from `about`. The stale sentence therefore never reached a
generated artifact, and no drift gate ever saw it. `xtask/src/main.rs:148-156`
now uses the model's own `long_about`, falling back to `about`, as the page
base — so the man page and `--help` cannot drift apart again, and the
`generated-docs` job now covers that surface.

This discharges **C036's D1** (and D2 with it).

## The requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| **I1** — the universe may only grow | `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots` (`tests/cli_contract.rs:4265`) asserts the Routine roots are still present in the resolved scope alongside the working directory's roots. Mutation **M2** replaces the routine roots with the operator's and the test fails. |
| **I2** — the anchor is unchanged | The anchor is `CleanupSelection::Ancestry` in `cleanup.rs`, untouched by C040. `the_working_directory_scope_adds_the_working_directory_to_the_routine_roots` asserts the resolved *scope* grew while the *selection* stayed anchored. |
| **I3** — the ADR 007 boundary is unchanged | `out_of_anchor_poison_does_not_block_anchor` (`src/cleanup.rs:5080`) is untouched and green. The boundary follows the scope, and the scope grew. |
| **I4a** — the exemption is the maintainability rule only | `the_working_directory_is_exempt_from_the_maintainability_rule` (`tests/cli_contract.rs:4341`) and `operator_provenance_admits_what_the_maintainability_rule_refuses` (`src/policy.rs:1327`) — the same path as an *automatic* root is still refused. Mutation **M4** applies the maintainability rule to the operator's root and both fail. |
| **I4b** — a symlinked working directory still blocks | `an_operator_root_that_is_a_symlink_blocks_the_scope` (`src/policy.rs:1430`). |
| **I4c** — the configured filters still apply to the scan path | `a_configured_ignore_still_covers_the_working_directory` (`tests/cli_contract.rs:4409`), asserted on the read-only path, where the filters apply; every cleanup scope bypasses them (ADR 007 §1). Mutation **M5** removes the working directory from the read-only inventory and the test fails. |
| **I5** — no new spawn site | The enclosing workspace is resolved through the existing `CargoRunner` seam. `the_enclosing_workspace_is_asked_for_in_the_working_directory` (`src/workspace.rs:6147`) asserts the *argument construction* (`locate-project --workspace`, **no** `--manifest-path`, cwd = the operator's directory), not merely that a spawn happened. |
| **I6** — the machine contract's shape is unchanged | The JSON `scope` label stays `routine`; `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots` asserts it, and `selected_roots` carries the actual roots. |
| **T1** — the scope is a superset of the seed and learned roots | `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots`. Mutation **M2** (the I1 hazard) fails it. |
| **T2** — the headline case: a run from outside the known roots cleans what is under it | `a_maintenance_run_from_a_directory_outside_the_known_roots_cleans_the_project_underneath_it` (`tests/cli_contract.rs:4221`). Mutation **M1** fails it, and two more. |
| **T3** — standing in a member directory still cleans the enclosing workspace | `standing_in_a_member_directory_still_cleans_the_enclosing_workspace` (`tests/cli_contract.rs:4311`). Mutations **M1** and **M3** fail it. |
| **T4** — operator provenance admits what the rule refuses | `the_working_directory_is_exempt_from_the_maintainability_rule`; `operator_provenance_admits_what_the_maintainability_rule_refuses`. Mutation **M4** fails both. |
| **T5** — the configured ignore list still prunes the working directory's subtree | `a_configured_ignore_still_covers_the_working_directory`. |
| **T6** — a symlinked working directory blocks the scope | `an_operator_root_that_is_a_symlink_blocks_the_scope`. |
| **T7** — `clean ROOT` and `clean --full` are unchanged | `an_explicit_root_keeps_its_own_scope` (`tests/cli_contract.rs:4485`) asserts `clean ROOT` does not gain the working directory. |
| **T8** — the ADR 007 regression still passes | `out_of_anchor_poison_does_not_block_anchor`, green, untouched. |
| **T9** — the JSON `scope` label is unchanged | `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots`. |
| **T10** — a working directory that already contains a routine root does not widen the scope | `a_working_directory_that_already_contains_a_routine_root_does_not_widen_the_scope` (`tests/cli_contract.rs:4564`). Mutation **M7** (removing the skip rule) fails it. |
| **T11** — an explicit root keeps its own scope | `an_explicit_root_keeps_its_own_scope`. |
| **C036 D1** — the `long_about` no longer lies, and cannot drift | The rewrite is in `src/cli.rs:21-33`; `xtask/src/main.rs:148-156` renders the man page from it; `generate-docs -- --check` reports 13 artifacts matching the clap model. |

### The supporting unit tests

Three in `src/workspace.rs` — `the_enclosing_workspace_is_asked_for_in_the_working_directory`
(`:6147`), `a_refused_enclosing_workspace_is_not_an_error` (`:6206`),
`the_working_directory_contributes_the_enclosing_workspace_and_itself`
(`:6261`) — and four in `src/policy.rs` — `operator_provenance_admits_what_the_maintainability_rule_refuses`
(`:1327`), `an_operator_omission_is_not_reported_as_a_learned_root` (`:1377`),
`the_learned_root_diagnostic_is_unchanged` (`:1402`),
`an_operator_root_that_is_a_symlink_blocks_the_scope` (`src/policy.rs:1430`),
unix-gated for the reason every other symlink test in the module is.

The last two are the ones that protect the *published* wording:
`the_learned_root_diagnostic_is_unchanged` asserts the "learned root(s)"
message is byte-identical to what it was before ADR 009, and
`an_operator_omission_is_not_reported_as_a_learned_root` asserts the
operator's bucket does not borrow the learned noun. Mutation **M6** breaks
both.

## The mutation evidence

Every mutation reverts one part of ADR 009's implementation and asserts that
a named test **fails**. A mutation that leaves the suite green is a test that
proves nothing. The harness (`/tmp/mutate_c040.sh`) backs every file up with
`cp` and restores from the backup; it never uses `git checkout --`, which
once wiped uncommitted work in this repository.

| Mutation | What it reverts | Caught by |
|---|---|---|
| **M1** | the working directory never joins the maintenance scope | 3 tests (`a_maintenance_run_from_a_directory_outside_the_known_roots_cleans_the_project_underneath_it`, `standing_in_a_member_directory_still_cleans_the_enclosing_workspace`, `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots`) |
| **M2** | the working directory **replaces** the routine roots — the I1 hazard | `the_maintenance_scope_adds_the_working_directory_to_the_routine_roots` |
| **M3** | the enclosing workspace is never resolved | 2 tests |
| **M4** | the maintainability rule is applied to the operator's root | 2 tests |
| **M5** | the read-only inventory does not cover the working directory | `a_configured_ignore_still_covers_the_working_directory` |
| **M6** | an operator omission is reported as a learned root's | `an_operator_omission_is_not_reported_as_a_learned_root`, `the_learned_root_diagnostic_is_unchanged` |
| **M7** | the "already reached" skip rule is removed | `a_working_directory_that_already_contains_a_routine_root_does_not_widen_the_scope` |

**All seven mutations were caught.** M2 is the one worth naming: it is the
only mutation that does not remove a feature but *inverts* one, and it is the
mutation that would have produced the shared-output hazard ADR 006 exists to
prevent.

The harness itself is a scratch artifact, not a shipped script: it lives at
`/tmp/mutate_c040.sh` and backs every file up with `cp` before mutating
(it never uses `git checkout --`, which once wiped uncommitted work in this
repository). **The table above is the specification** — each row says what to
revert and which test must fail, which is enough to re-create the harness
from scratch. The mutations were re-run against the committed tree after the
portability fix, and all seven were still caught.

## The live evidence

On the machine that prompted this, before the change and after:

| Run | Before | After |
|---|---|---|
| `cargo cleanme --dry-run` from `~/.codex` | **0 would-clean**, exit 0 | **1 would-clean** (`worktrees/fe7e/codegg`, 2.74 GiB), exit 0, 18 s |
| `cargo cleanme --dry-run` from `$HOME` | — | **1 would-clean** (1005.71 MiB), exit 0, **9.1 s** — the bounded sweep, not blocked |

The `$HOME` figure is the load-bearing one. With the skip rule removed, the
same run became a **65-second sweep of 264,183 directories that exited 1
(blocked)** on 1,357 unresolvable manifests in `~/.local/share/Trash`. The
bounded machine-wide sweep must stay bounded, and the published
daily-scheduler recipe — a bare run from the home directory — keeps working
unchanged.

The live runs are `Simulate` mode. No `cargo clean` process was invoked by
any of them.

## What the hosted lanes caught

The first hosted run of the implementation (`c9f7784`, CI `39507383193`)
failed on **macOS and Windows** and passed on Linux. Both failures were in
C040's *tests*, not in its production code, and both were invisible on the
platform they were developed on.

**macOS — five `cli_contract` tests failed.** The reported path is
canonical; the expectation was not:

```
left:  "/private/var/folders/…/sandbox/project"
right: "/var/folders/…/sandbox/project"
```

Production canonicalizes every path it reports, and macOS puts the
temporary directory under a symlinked prefix (`/var` → `/private/var`). The
fixture built its paths on the raw spelling, so it asserted against a path
production never produces. This is the repository's oldest known trap, and
`AGENTS.md` records it: *"macOS's symlinked `TMPDIR` broke path comparison
until the manifests were compared component-wise."* The fix is the one
`tests/end_to_end.rs` already makes: canonicalize the directory the fixture
builds in.

**Windows — one test failed.** `an_operator_root_that_is_a_symlink_blocks_the_scope`
created its symlink under `#[cfg(unix)]` but ran everywhere, so on Windows
it asserted against a path that was simply absent: no symlink, so no
"blocked", and the scope was *omitted* rather than blocked. The test is now
unix-gated, as every other symlink test in the module already is — creating
a symlink on Windows needs a privilege the CI runner does not have. The
*classification* is the same on both platforms; only the creation is
platform-specific.

Neither failure is a product defect, and both are now covered by a test
that can be run anywhere: `the_reported_path_is_canonical_even_behind_a_symlinked_prefix`
(`tests/cli_contract.rs:4523`) builds the fixture under a symlinked prefix
of its own, asserts that the fixture really canonicalized, and then asserts
that the reported path is the canonical one. It was checked against a
mutated build with the canonicalization removed, and it fails there.

The lesson is the one this repository keeps re-learning, in a new costume:
**a green local run is not a green run elsewhere**, and the hosted matrix
is the only thing that proves it. A test that can only fail on the platform
it was written for is a test whose failure arrives at the worst possible
moment.

## The citation re-derivation

C040 changed four source files, so 1,556 citations in `architecture/` moved.
They were re-pointed through a **content-exact** map built by aligning the
baseline and the current tree with `difflib.SequenceMatcher` and mapping
only lines that are byte-identical on both sides. A line whose content
changed was deliberately left unmapped, because a citation pointing at it
has to be re-derived by hand — shifting it arithmetically is exactly the
failure this repository keeps re-learning (`AGENTS.md` §6).

Twenty-three citations needed the hand treatment, and they are worth
enumerating because they are the ones the machine could not do:

- **Six** named `src/cli.rs:27` or `src/cli.rs:28` — the `long_about`
  sentence that was rewritten. These are not re-pointed at all: the claim
  they carried ("the `long_about` still claims…") is now false, so the
  prose was rewritten and the finding discharged.
- **Twelve** named `main.rs:1045` or `main.rs:766` — the `policy::resolve`
  call site and the `admit_automatic` call. Both lines changed *content*
  (`let` → `let mut`; a new parameter), so the map refused them. Each was
  re-derived by reading the new source: `main.rs:1045` → `main.rs:1153`,
  `main.rs:766` → `main.rs:773`.
- **Two** named bare `:2578` and `:3892` in `13-orchestration.md`. These are
  `cli_contract.rs` references, not `main.rs` ones — the bare form in that
  deep dive is not uniformly about its subject file. `cli_contract.rs` was
  only appended to, so those line numbers did not move and the citations are
  correct as they stand.

The bare `:N` form is the dominant convention inside the deep dives (2,764
citations) and is **not** machine-gated. The distribution was checked before
the re-point: in each of the four affected deep dives every bare citation
falls inside its subject file's line range, with exactly the two exceptions
above, which are demonstrably `cli_contract.rs` references.

`xtask/src/main.rs` needed a separate pass: the citation gate's lookbehind
refuses a `src/main.rs` preceded by `/`, so that surface has **no mechanical
guard at all**. Thirteen citations there were re-pointed by hand.

The `src/` and `tests/` directory prefixes were preserved throughout. The
first pass dropped them, which would have restyled 1,556 citations for no
reason and made each document internally inconsistent; the prefixes were
restored by a diff-aligned second pass, and the counts now match the baseline
exactly.

## The census re-derivation

The overview's counts were re-derived from `src/`, not copied:

| Figure | Value |
|---|---|
| Declared `#[test]` in `src/` | **481** |
| Compile on Linux x86_64 | **479** (475 lib + 4 bin) |
| Compile on macOS | **476** |
| Compile on Windows | **441** |
| `#[cfg(unix)]`-gated | 36 |
| Linux-only | 4 |
| Total lines in `src/` | **34,438** (15,760 production) |

The parser that derives `prod` was **validated against the unchanged
modules first**: it reproduces the four changed files' previous values
(`cli.rs` 1276/484/32, `main.rs` 1562/1425/4, `policy.rs` 1238/712/17,
`workspace.rs` 6068/2093/76) exactly before being applied to the new tree.
The Linux figure is cross-checked against `cargo test --lib -- --list`, which
lists 475 — the library target — and the binary target's 4 make 479.

The gate breakdown was derived by **set intersection** across the three
target configurations, not by grep: 439 compile on all three lanes, 36 are
unix-gated, 4 are Linux-only, 1 is Windows-only, and 1 is macOS+Windows.
That is 481 declared, 479 Linux, 476 macOS, 441 Windows.

## The hosted evidence

Two hosted runs, and the first one failed. Both are recorded.

| Run | Tree | Result |
|---|---|---|
| CI `39507383193` | `c9f7784` (the implementation) | **failure** — macOS and Windows; see "What the hosted lanes caught" |
| Release drift guard `39507383163` | `c9f7784` | success |
| CI `39509077691` | `cc27d5f` (the portability fix) | **success** — all nine jobs |
| Release drift guard `39509077681` | `cc27d5f` | success |
| Cargo selector qualification `39509083087` | `cc27d5f` | success |
| CI `39509567027` | `3a33431` (this record) | **success** — all nine jobs |
| Release drift guard `39509567152` | `3a33431` (this record) | success |

All nine jobs of `39509077691` are green: `checks (ubuntu-latest)`,
`checks (macos-latest)`, `checks (windows-latest)`, `msrv`,
`generated-docs`, `benchmark`, `installers (ubuntu-latest)`,
`installers (macos-latest)`, `installers (windows-latest)`.

### The hosted census, re-derived name for name

The source-derived census is confirmed by the hosted lanes, target by target:

| Target | Hosted `src/` tests | Census says |
|---|---|---|
| ubuntu-latest | 475 lib + 4 bin = **479** | 479 ✓ |
| macos-latest | 472 lib + 4 bin = **476** | 476 ✓ |
| windows-latest | 437 lib + 4 bin = **441** | 441 ✓ |
| declared | — | **481** |

The `tests/` targets are 71 `cli_contract` and 4 `end_to_end` on Linux and
macOS, and 19 `cli_contract` on Windows — the `cli_contract` suite is
unix-gated, which is why its Windows count is a fifth of the others.

This is the first time the per-platform census has been *build-verified*
rather than source-derived, and the derivation was correct on all three
targets. That is worth saying plainly, because the previous three
per-platform counts had each disagreed with the others (finding 57).

## The verification actually run

Every command below was run on this tree, in this order.

```sh
cargo fmt --all -- --check                               # ok
cargo clippy --all-targets --all-features -- -D warnings # ok, no warnings
cargo test --all-targets --all-features                 # 475 lib + 4 bin + 71 cli_contract + 4 end_to_end, 0 failed
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
                                                        # 37 fixture file(s) clean
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
                                                        # 17 document(s) clean
python3 scripts/check-release-contract.py --self-test && python3 scripts/check-release-contract.py
                                                        # 5 contracted targets, 11 release assets, wrappers aligned
cargo run --quiet --features dev-tools --bin generate-docs -- --check
                                                        # 13 artifacts match the clap model
```

One clippy failure was found and fixed during the ladder: the new
`BrokenRunner` test double used `io::Error::new(io::ErrorKind::Other, …)`,
which clippy's `io_other_error` lint rejects. It now uses `io::Error::other`.
The lint is on by default in the toolchain on this host; it would have
failed the CI lane.

`scripts/release-check.sh` was **not** run: it requires a clean tree, the
`eggpack` binary, and toolchain 1.89, and it is the pre-release gate rather
than routine verification.

## Known limitations

- **The per-platform lane counts are now build-verified.** The derivation
  (a brace-tracking evaluation against `rustc --print cfg --target <triple>`,
  cross-checked against `cargo test --lib -- --list` on the host platform)
  was confirmed against the hosted lanes target by target: 479 on Linux, 476
  on macOS, 441 on Windows, against a derived 479/476/441. The *per-file*
  breakdown remains source-derived; the totals are not.
- **A symlinked *working directory* is covered by a unit test, not an
  integration one.** `std::env::current_dir()` on Linux resolves symlinks, so
  a symlinked CWD is unreachable from a subprocess test. The behaviour is
  covered where it can be: `an_operator_root_that_is_a_symlink_blocks_the_scope`
  exercises the classification arm directly. (The *reported path* is a
  different question, and it now has an integration test that runs
  everywhere: `the_reported_path_is_canonical_even_behind_a_symlinked_prefix`.)
- **The live runs are Simulate mode.** No `cargo clean` process was invoked
  by any of them, so the evidence is about *selection and proof*, not about
  deletion.
- **The enclosing workspace is resolved by a Cargo spawn per maintenance
  run.** It is one extra `locate-project --workspace` on a path that already
  spawns Cargo for metadata, and it runs only when the working directory
  joins the scope.
- **The ADR 007 boundary grew with the scope.** The scope is wider, so
  *less* is inside the proof boundary than before, never more. A manifest
  that used to be outside the boundary can now be inside it — the
  unresolvable manifests in a Trash directory are the case that made this
  visible. This is the reason for the "already reached" rule, not a defect.

## The unresolved findings, by severity

| Finding | Severity | State |
|---|---|---|
| 20 — `--include` / `--exclude` match the workspace root, not the physical output root | low | **Open.** Not C040's scope. |
| 23 — a user-facing diagnostic points at the hidden `scan --full` alias | low | **Open.** Carried by C036 D3. |
| 24 — `absolutize_root` and `collapse_roots` disagree about `..` | low | **Open.** Not C040's scope. |
| 25 — a root-level `--dry-run` is accepted alongside `scan` and ignored | low | **Open.** Not C040's scope. |
| The terminal cursor is not restored on exit | medium | **Open.** Pre-existing; pinned by a test; carried from C039. |
| No pty harness | medium | **Open.** Carried from C039. |
| ADR 008 §4 and §5 | — | **Open owner decisions.** Deliberately not resolved by C040; see below. |

## The interaction with ADR 008

ADR 008 proposes making `cargo cleanme --full` the canonical Full-cleanup
spelling. Its §4 (policy-flag placement) and §5 (the Full gate) carry two
owner decisions that are still open, and C040 deliberately did **not**
resolve them: it did not add a root-level `--full`, and it did not remove
the published `clean --full` spelling.

C040's direction — the root as the single front door — is compatible with
ADR 008 §4b. If §4b is accepted before this ships, C040's spelling work
should be folded into that ADR rather than shipping two overlapping front
doors. That is recorded as a stop condition in the plan.

## The disposition

**Closed.** The implementation landed, the hosted lanes are green, the
documentation is re-derived, and the publication that was the only remaining
condition has happened: **v0.2.10** carries ADR 009 to the public.

| | |
|---|---|
| Tag | `v0.2.10` at `270cceb2be318ab6e2f7da87a90d5ce73f7f292a` |
| GitHub release | published 2026-10-21T00:35:53Z, immutable, 15 assets, valid attestation |
| crates.io | published 2026-10-21T00:36:36Z, sha256 `0f2e471b…`, not yanked |
| Automatic smoke | `39522277814` — green on all five target lanes |
| External smoke | passed against the published crate and the published release asset |
| Release record | `plans/closure/distribution-release-update/r030-status.md` |

The publication gate was satisfied in full: the Eggpack candidate build
(`39518072322`) was green, the staged-release validation (`39520043716`) was
green for the same tag and the same source SHA, the draft's 15 assets were
independently inspected and every digest agreed four ways, and the crate
version and the release tag agreed before the upload.

**Publication is what closes a corrective in this repository** — *compilation
is not closure, publication is not closure* — and the publication happened.

Two things are **not** closed by this, and both stay open on their own
merits:

- **The interactive terminal lifecycle** — the cursor is not restored on exit
  — and the **absence of a pty harness**. Both are carried from C039.
- **ADR 008 §4 and §5**, the two open owner decisions. C040 deliberately did
  not resolve them: it added no root-level `--full` and removed no published
  spelling.

One further finding was discovered by this release's own gate and is recorded
as **C041** (open, proposed): the update path's candidate spawn can fail
spuriously with `ETXTBSY` under load. It is not a safety defect, it is not
C040's code, and it was not fixed inside the release.

## Reproduction

```sh
cd /home/sugarwookie/projects/cargo-cleanme
cargo build
cd <a directory that holds a Cargo project but sits outside the machine's known roots>
/home/sugarwookie/projects/cargo-cleanme/target/debug/cargo-cleanme --dry-run --no-progress
# the units under the current directory are now reported
```
