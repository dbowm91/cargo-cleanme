# AGENTS.md

Entry point for coding agents working on `cargo-cleanme`. Read this first, then
the one architecture deep dive your change touches.

## What this is

A single Rust crate that finds Cargo build output and cleans it **through
Cargo**, never by deleting directories. One binary over one library of 16
modules, 18 source files, 43,057 lines in `src/`. No async runtime of its own.
`cargo metadata` is the only binary dependency that matters for safety.

The working tree is **0.2.11, which is not published**; `CHANGELOG.md` has no
`[Unreleased]` section. Eighteen releases are published (v0.1.0 through v0.1.6,
then 0.2.0, 0.2.1, 0.2.2, 0.2.3, 0.2.4, 0.2.5, 0.2.6, 0.2.7, 0.2.8, 0.2.9,
0.2.10). **v0.2.0 is yanked; the other seventeen are not.** Bumping `Cargo.toml` is not a publication — see
`docs/RELEASING.md` and `scripts/release-check.sh`.

## Read this before your first change

1. [`architecture/overview.md`](architecture/overview.md) — shape, the
   cross-cutting safety invariants, and the index of 16 deep dives.
2. The deep dive for the module you are changing (table in §1 below).
3. The matching skill in [`.skills/`](.skills/) (table in §2 below).

The one thing to internalise before touching cleanup or discovery:

> **Nothing is deleted on the strength of a timestamp. Every candidate must
> first prove exclusive ownership of its physical bytes, and that proof must
> still hold at the moment of deletion.**

## 1. Architecture index

| Deep dive | Module(s) | Read it when |
|---|---|---|
| [01-domain-and-errors](architecture/01-domain-and-errors.md) | `domain.rs`, `error.rs` | You touch a DTO, a reason code, or error typing |
| [02-cli](architecture/02-cli.md) | `cli.rs` | You touch argv parsing, subcommand normalization, or root handling |
| [03-config-and-editor](architecture/03-config-and-editor.md) | `config.rs`, `editor.rs` | You touch `config.toml` or `config edit` |
| [04-policy-and-scope](architecture/04-policy-and-scope.md) | `policy.rs` | You touch the Routine/Full decision |
| [05-discovery](architecture/05-discovery.md) | `discovery.rs` | You touch manifest discovery or attribution |
| [06-discovery-state](architecture/06-discovery-state.md) | `discovery_state.rs` | You touch learned state or uncertainty |
| [07-workspace](architecture/07-workspace.md) | `workspace.rs` | You touch `cargo metadata`, grouping, or cleanup units |
| [08-traverse](architecture/08-traverse.md) | `traverse.rs` | You touch measurement, the worker pool, or activity |
| [09-cleanup](architecture/09-cleanup.md) | `cleanup.rs` | **You can delete bytes.** Read all of it |
| [10-reporting](architecture/10-reporting.md) | `report.rs`, `output.rs` | You touch rendered or JSON output |
| [11-progress](architecture/11-progress.md) | `progress.rs` | You touch the TTY renderer |
| [12-self-update](architecture/12-self-update.md) | `update.rs` | You touch `update` |
| [13-orchestration](architecture/13-orchestration.md) | `main.rs` | You touch composition, argv dispatch, or exit codes |
| [14-testing-and-verification](architecture/14-testing-and-verification.md) | tests, gates | **You are writing or reviewing a test** |
| [15-distribution-and-release](architecture/15-distribution-and-release.md) | installers, CI, packaging | You touch distribution |
| [16-interactive](architecture/16-interactive.md) | `interactive.rs` | You touch `-i`, the selection TUI, or raw-mode lifecycle |

### 1.1 The overview, by section

[`architecture/overview.md`](architecture/overview.md) is the birds-eye view **and**
the index. Cite it by **section number**, never by line — a line citation into
a file that is being rewritten rots immediately.

| § | Section | Use it for |
|---|---|---|
| 1 | The one-paragraph model | Orientation in thirty seconds |
| 2 | What the tool can do | The capability surface, in user terms |
| 3 | The crate at a glance | Module sizes and the **authoritative counts** — verify against `src/` before copying |
| 4 | Module index | Per module: what it owns, what it depends on, what it breaks |
| 5 | The layer map | The dependency graph, the two production cycles, the test-only back edges |
| 6 | The scan pipeline | What happens on `scan`, end to end |
| 7 | The cleanup pipeline | What happens on `clean`, end to end |
| 8 | Cross-cutting invariants | The eight rules that span modules — read before changing any of them |
| 9 | Tooling and verification surfaces | The gates, and what each one actually proves |
| 10 | Findings from the deep dives | 110 known defects and their dispositions — **check here before adding a new one** |
| 11 | How the code got this way | The plan corpus |
| 12 | Index of deep dives | The sixteen deep dives |
| 13 | Reading order | If you read one thing, read this |

## 2. Skills

Load the one that matches your change. They carry the *how* and the traps; the
deep dives carry the *what*.

| Skill | Load it when | Deep dive |
|---|---|---|
| [planning-and-closure](.skills/planning-and-closure/SKILL.md) | You touch anything under `plans/`, or a closure claim | — |
| [cleanup-safety](.skills/cleanup-safety/SKILL.md) | You can delete bytes, or change what becomes `PrivateBounded` | [09](architecture/09-cleanup.md) |
| [discovery-and-ownership](.skills/discovery-and-ownership/SKILL.md) | You change discovery, grouping, resolution, or learned state | [05](architecture/05-discovery.md) · [06](architecture/06-discovery-state.md) · [07](architecture/07-workspace.md) |
| [json-and-exit-contract](.skills/json-and-exit-contract/SKILL.md) | You touch `output.rs`, a reason code, or an exit code | [10](architecture/10-reporting.md) |
| [cli-surface-and-argv](.skills/cli-surface-and-argv/SKILL.md) | You touch argv, a flag, a selector, or a generated artifact | [02](architecture/02-cli.md) · [13](architecture/13-orchestration.md) |
| [terminal-and-interactive](.skills/terminal-and-interactive/SKILL.md) | You touch `-i`, the TUI, raw mode, the cursor, or the TTY renderer | [16](architecture/16-interactive.md) · [11](architecture/11-progress.md) |
| [test-evidence](.skills/test-evidence/SKILL.md) | You write or review a test, or claim verification | [14](architecture/14-testing-and-verification.md) |
| [release-and-distribution](.skills/release-and-distribution/SKILL.md) | You release, or touch the updater or an installer | [12](architecture/12-self-update.md) · [15](architecture/15-distribution-and-release.md) |

> **The skills are not machine-gated.** `scripts/check-doc-citations.py` resolves
> citations inside `architecture/` only — not in `.skills/`, `docs/`, or
> `README.md`. Three of the six original skills carried fabricated facts until
> the re-derivation pass (an exit-code table for codes that do not exist, four
> reason codes that do not exist, a `Preview` mode that does not exist, and a
> `[package] ignore` feature that does not exist); the two newer skills were
> written from source. **Re-derive a skill's claims from the source; do not
> copy them forward.** See `architecture/overview.md` §10, findings 53 and 54.

## 3. Verification ladder

Run in order; stop at the first failure and do not report the rest.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-selector-qualification.py --self-test && python3 scripts/check-selector-qualification.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

The last step is the **generated-artifact drift gate**. It only matters if you
touched the CLI surface, but it is cheap, and a CLI change that leaves the
completions and man pages stale fails CI rather than you. Regenerate with
`cargo run --quiet --features dev-tools --bin generate-docs`.

The **selector-qualification gate** is the one that keeps
`release/selector-qualification.json`, the CI runtime allowlist, the hosted
qualification matrix, `scripts/check-selector-qualification.py` and
`clean_capabilities_from_version` in exact agreement — it decides whether
`--profile` and `--package` are usable at all. It is enforced in CI
(`.github/workflows/ci.yml`) and in `scripts/release-check.sh`; a change to
any of those files passes the rest of the ladder and fails it.

Focused test: `cargo test --all-features --test cli_contract <test_name>`
(`tests/` holds `cli_contract.rs` and `end_to_end.rs`, plus inline unit tests in
`src/`). Current counts live in `architecture/overview.md` §3, which is the source
of truth — verify them there rather than copying them from here).

Full pre-release gate: `scripts/release-check.sh` — **24 steps**, requires a
clean tree, the `eggpack` binary, and toolchain 1.89 installed, so do not run
it as routine verification; the ladder above is the ordinary gate.

**Report what actually ran.** A record claiming a gate passed when it was not
run is worse than no record, because the next milestone trusts it.

### 3.1 Gates the ladder does not cover

- **MSRV.** CI runs a separate lane on toolchain **1.89**
  (`cargo +1.89 check --locked --all-targets` and
  `cargo +1.89 test --locked --all-targets`). A stdlib or dependency API newer
  than 1.89 compiles on your stable toolchain and breaks that lane. `Cargo.lock`
  is committed and the MSRV lane uses `--locked`; do not regenerate it casually.
- **Cross-platform.** The CI matrix is ubuntu/macos/windows, and the
  platform-specific fixtures only run there: backdating a workspace root's mtime
  needs the `windows-sys` dev-dependency on Windows, and macOS's symlinked
  `TMPDIR` broke path comparison until the manifests were compared
  component-wise. A change touching path comparison, mtimes, symlinks, or
  `TMPDIR` is **not** verified until the hosted lanes run.
- **Release contract.** `python3 scripts/check-release-contract.py` guards the
  `Cargo.toml` `include` allowlist: it fails if `/config.toml` is dropped or if
  `xtask` is added. It runs in the CI `generated-docs` job, not in the ladder.

## 4. Runtime facts that are not obvious from the filenames

- **One spawn boundary.** Every external `cargo` invocation goes through the
  `CargoRunner` / `CleanupRunner` traits (`src/workspace.rs:82` for
  `cargo metadata`, `src/cleanup.rs:1013` for `cargo clean`) so tests can
  substitute fakes. Do not add a bare `Command::new("cargo")` — that seam is the
  project's main testability mechanism and its main risk.
- **`config.toml` is a compile-time input.** `src/config.rs:12` embeds it with
  `include_str!("../config.toml")`, so editing the template changes what
  first-use config creation writes. Validate with `cargo test config_template`.
  The file is also a hard requirement of the package allowlist, so it cannot be
  deleted or excluded.
- **The `include` allowlist in `Cargo.toml` is deliberate.** The published crate
  is a curated subset: `architecture/`, `plans/`, `docs/`, `scripts/`, `xtask/`,
  and `tests/` do **not** ship. A new top-level file will not ship unless you add
  it to the allowlist — and `check-release-contract.py` fails if `xtask` slips in.
- **`dev-tools` is off by default.** `clap_complete` and `clap_mangen` are
  optional, so a normal build and `cargo install` never compile the generator.
  `--all-features` does compile it, which is why the ladder uses that form.

## 5. The lesson this repository keeps re-learning

> **A green test proves only that its own premises hold.**

Seven milestones closed on a test that was passing for the wrong reason. Three
product defects (C015, C016, C017) were found by this repository's own evidence
work, and **none** was repaired under the plan that found it. A plan that finds
a defect writes a corrective; it does not fix it.

Ways a test goes green without testing anything — and the guard that exists for
each:

1. A fixture that cannot run where it reports success →
   `scripts/check-fixture-portability.py`
2. A stub that is not the tool the subject resolves →
   `packaging/tests/test_installers.py --self-test`
3. A case whose pass condition is an unrelated failure → prefer a discriminating
   assertion (the specific reason code, not "the command failed")
4. A test that **pins the defect** — it asserts the wrong thing, including the
   thing you are about to fix. `the_driver_shows_the_hardware_cursor_only_in_insert_mode`
   asserted the terminal cursor's last recorded visibility was `Some(false)`; the
   interactive session never restored the cursor, so the test was green *because*
   it asserted the bug. **Before you fix a defect, check whether a test is
   currently pinning it.** C042 closed that one, and the rule is now a worked
   example rather than a warning: the test was rewritten **first**, so the fix
   could not leave a green test pinning the *new* bug.

Every **new** guard takes a `--self-test` that feeds it a known-bad sample and
requires a failure. If you add a guard, the self-test is not optional: a guard
that stopped guarding is worse than no guard. (Eleven of the seventeen scripts
already have one; the six that do not are inventoried in
`architecture/14-testing-and-verification.md` §5.)

Before trusting a test, ask **"what input makes this fail?"** If you cannot
produce one, the test is decoration. See
[14-testing-and-verification §4](architecture/14-testing-and-verification.md).

## 6. Documentation conventions

`architecture/` cites source by line, which is what makes it checkable and what
makes it rot. A fix that adds or removes lines invalidates every citation past
that point while the prose keeps reading as true. This happened: one commit
added 38 lines to `main.rs` and 55 to `traverse.rs` and the deep dives kept the
old numbers.

`scripts/check-doc-citations.py` gates the mechanically decidable half — the
cited file exists, the line is in range, the range is ordered. The semantic
half (does the line still say what the sentence claims?) is a reviewer's job and
no tool can do it. When you shift a citation, re-derive it from the source; do
not shift it arithmetically unless you have read the new line.

Three limits of that gate, all of which have bitten:

- It resolves **only** the `file.rs:N` form. The bare `:N` form is the dominant
  convention inside the deep dives (2,742 of them) and is **not** machine-gated.
- It covers `architecture/` **only**. `.skills/`, `docs/`, and `README.md` are
  ungated, and all three drifted (findings 53 and 54 in
  `architecture/overview.md` §10).
- It cannot judge meaning. A citation can resolve perfectly and say something
  the code no longer does.

Counts are not to be copied between documents. `architecture/overview.md` is the
source of truth for module line counts and the inline-test total; verify each
against `src/`, fix the overview first, then align the deep dives.

The same rule applies to **per-platform test counts**, which have disagreed
across three documents: they are 602 declared, 600 on Linux, 547 on Windows,
596 on macOS. Those three figures count `src/` — the **lib target and the bin
target together** — while a CI lane prints one number per target, so the Linux
lane's library line reads 588 and the Windows lane's 536. Derive them with a
brace-tracking parser that evaluates each
test's *effective* gate set against `rustc --print cfg --target <triple>`, and
cross-check against `cargo test --lib --bins -- --list`. **Count the tests a gate
excludes, not the gates.** A naive "the `#[cfg(unix)]` immediately above a
`#[test]`" grep finds 38 of the **50** tests a `#[cfg(unix)]` gate actually
excludes, because Rust allows `#[test]` to precede the gate — twelve of the
50 have it that way. The remaining gates in test scope sit on helpers and on
blocks *inside* a test function, which exclude no test at all: a block inside
a test is removed on Windows, and the test still compiles and still runs. A
derivation that subtracts gate counts instead of test counts lands 16 high on
Windows, which is exactly how the census was wrong for so long — and the
figure it produced, 500, was **also** wrong, by 16 the other way.

The Windows figure can be checked without a Windows host: cross-compile the
test binary for `x86_64-pc-windows-gnu` (the `windows-gnu` std is
installable, and `mingw-w64` supplies the C toolchain `ring` needs) and
confirm it carries exactly the derived set of tests and no others. That check
is what settled the figure at 547.

## 7. Planning and decision records

`plans/` is the decision record: canonical direction (`000`–`002`), the process
(`003`), accepted ADRs, subsystem roadmaps, milestone plans, and per-requirement
closure records.

- `plans/registry.md` is the compact control surface — what is open and what to
  read next. It is deliberately **not** a second copy of the roadmaps.
- Do not edit a closed plan or a closed closure record. Correct forward with a
  new corrective plan, as C001–C009 and C010–C017 all did.
- Compilation is not closure. Publication is not closure. A corrective that is
  fixed but not yet carried by a published release stays **open**.

## 8. Things that are already decided — do not relitigate

- Cleanup is **Cargo-mediated and fail-closed**; deletion is always
  `cargo clean`, never direct filesystem removal. It is *not* opt-in: since
  [ADR 003](plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md)
  bare `cargo cleanme` is Routine **Execute**, with `--dry-run` the canonical
  zero-mutation preview. The safety boundary is ownership proof, not user
  rememberment.
- **Bare `cargo cleanme` and `clean --known` are anchored at the working
  directory** (M014, shipped 0.2.8). The Routine scope is still discovered and
  classified in full, but only units whose workspace root equals, contains, or
  is contained by the CWD become candidates. `clean ROOT` and `clean --full`
  keep every resolved unit. Units the anchor excludes are reported as
  `skipped_outside_selection` rows, not dropped — "nothing was selected" must
  never look like "nothing was eligible".
- **The working directory joins the maintenance scope** (ADR 009, C040 — not
  yet published). A bare run's scope is the Routine scope **plus** the
  operator's position — the enclosing workspace and the working directory
  itself — but only when the Routine scope does not already reach it, and
  never *instead of* it: the universe may only grow, so a wider universe can
  only reclassify `shared` → `private`. The exclusion is a measurement, not a
  taste: an unconditional join turned a bare run from `$HOME` into a 65-second
  sweep of 264,183 directories that ended **blocked** on 1,357 unresolvable
  manifests in `~/.local/share/Trash`. `clean ROOT` is the spelling for that
  wider sweep. The operator's roots carry `RootProvenance::Operator`, exempt
  from `automatic_root_is_maintainable` only.
- **Fail-closed means the proof boundary, not the discovered universe.** Under
  [ADR 007](plans/adr/007-accepted-scoped-proof-boundary.md)
  and M017, an unresolved manifest *inside* the selected boundary blocks it; an
  unresolved manifest *outside* it is reported as an `out_of_scope` diagnostic
  and **never blocks**. Do not reintroduce a whole-scope block; the regression
  test `out_of_anchor_poison_does_not_block_anchor` fails by construction
  against it.
- `allowed_output_roots` authorizes; it can never manufacture ownership proof.
- `--dry-run` (hidden alias `--dryrun`) spawns nothing, not even a Cargo preview,
  and is otherwise as strict as `--yes`. Cargo's own `clean --dry-run` lives
  behind the separate `--cargo-preview` flag (`cli::clean_mode`). The clean-mode
  spellings conflict pairwise **on `clean`**; a root-level `--dry-run` takes
  precedence over `--cargo-preview`, because simulation spawns nothing.
- `ignore.unignore` is a literal absolute path, not a glob. It predates glob
  support by design; changing it needs a plan.
- Learned state is advisory, never authority, and a persistence failure never
  changes an exit code.
- Upstream support gaps: musl and Windows ARM64/ARMv7 are Cargo-only. Do not
  add binaries without deciding that explicitly.
- `update_json` is the one non-envelope output. It is **no longer untested**:
  C022 pinned the machine contract with tests. Migrating it to `EnvelopeV1` is
  still a contract change needing a plan and a version bump.
- There are **three** exit codes: `0`, `1`, `2`. Every `AppError` exits `2`,
  usage errors included (`src/main.rs:34`). The log-format `reason=` codes
  (`config`, `io`, `scan`, `provenance`, `update`) are a **different** axis from
  the exit status.
- `cargo cleanme scan` with no selector is a **usage error** (exit `2`), not a
  Full scan. `scan --full` is a hidden alias of `scan --all`.

### Open decisions — do not resolve them yourself

- **ADR 008** (`plans/adr/008-*.md`, *accepted 2026-10-21*) decides that
  `cargo cleanme --full` becomes the canonical Full-cleanup spelling, with
  `cargo cleanme clean --full` retained as a hidden alias (published since
  0.2.4), and that the Full cleanup's proof boundary becomes the **learned
  root** instead of the platform root. §4 resolved to 4b (the policy flags
  become root-level); §5 resolved to 5b, route 5b-i (a verification pass over
  the learned roots). The one point ADR 008 left open — where the **ROOT
  positional** lives — was resolved by the owner on 2026-10-21 in favour of
  **option A**: the ROOT positional **moves to the root**, so
  `cargo cleanme ~/projects` is the canonical bounded-root spelling and
  `cargo cleanme clean ~/projects` remains accepted and hidden; `clean --known`
  does not move; and `cargo cleanme .` changes from a usage error to a bounded
  cleanup of `.`. The decision is recorded as dated amendment blocks in ADR 003
  §4 and ADR 008. **Both milestones are implemented** — the gate in M018
  (`plans/implementation/artifact-discovery-cleanup/m018-*.md`) and the
  spelling in M019 (`m019-*.md`) — and both are **`closing`**: implemented,
  ladder-green, and **not yet published**, so they stay open (publication is
  not closure). The root-level `--full` and the root-level policy overrides
  now exist; `clean --full` and `clean ROOT` remain accepted and hidden, and
  `clean --known` still resolves to the bare invocation. **M019 may not be
  published before M018 has shipped**, because the canonical spelling must
  never point at an operation that refuses.
- **C036** (`plans/implementation/artifact-discovery-cleanup/c036-*.md`,
  *proposed*) is the open documentation-drift corrective. It is the right home
  for a newly-found doc or message defect — extend it rather than opening a
  second one. **Its D1 is discharged by C040**, which rewrote the root
  `long_about` so it no longer claims a bare `cargo cleanme scan` is the full
  machine reconciliation, and made the root manpage render from that same
  `long_about` so the two surfaces cannot drift again. **Its D29 was added on
  2026-10-21**: the published-release count is hand-maintained prose in three
  documents with no gate over it, and it has now drifted three times. The
  three copies were corrected in the same pass; the *gate* is not built, so
  the drift is expected to recur until it is.

## 9. Repository map

| Path | Contents |
|---|---|
| `src/` | the crate — 18 files; `main.rs` is the binary, `lib.rs` declares the 16 library modules |
| `tests/` | `cli_contract.rs` (83 tests), `end_to_end.rs` (4), `common/` |
| `architecture/` | `overview.md` + 16 deep dives |
| `docs/` | `USAGE.md`, `AUTOMATION.md`, `INSTALLING.md`, `RELEASING.md`, `TROUBLESHOOTING.md`, `UPDATE.md` |
| `plans/` | decision records — see §7 |
| `.skills/` | the eight skills — see §2 |
| `scripts/` | 17 scripts; `check-doc-citations.py` also gates that `architecture/14-testing-and-verification.md` §5 table is exactly these 17, so a new checker cannot ship undocumented. **11 of the 17 implement `--self-test`; the other 6 do not** — `release-check.sh` *invokes* twelve self-tests rather than providing one, and `gen-release-workflow-shape.py`, `release-benchmark.py`, `smoke-release-candidate.py`, `validate-staged-release.py`, and `post-release-smoke.sh` have no such mode. A new guard must ship with one; the existing six are inventoried in `architecture/14-testing-and-verification.md` §5 |
| `completions/`, `man/` | **generated** — 5 completions + 8 man pages = 13; do not hand-edit; run `generate-docs` |
| `xtask/` | `generate-docs` source (feature `dev-tools`); excluded from the published crate |
| `examples/` | `traversal-qualification.rs` — the M006F traversal harness, run with `cargo run --release --example traversal-qualification -- <stage>` |
| `release/eggpack/` | `distribution.toml` is the authority for the release target matrix; the tables in `README.md` and `docs/INSTALLING.md` are hand-copied transcriptions of it |
| `packaging/`, `release/` | installers, Eggpack policy, smoke and qualification scripts |
| `CHANGELOG.md` | released changes, including known defects per version |
