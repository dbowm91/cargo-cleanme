# AGENTS.md

Entry point for coding agents working on `cargo-cleanme`. Read this first, then
the one architecture deep dive your change touches.

## What this is

A single Rust crate that finds Cargo build output and cleans it **through
Cargo**, never by deleting directories. One binary over one library of 15
modules, 17 source files, 21,868 lines in `src/`. No async runtime of its own.
`cargo metadata` is the only binary dependency that matters for safety.

Eight releases are published (v0.1.0 through v0.1.6, plus 0.2.0), none yanked.

## Read this before your first change

1. [`architecture/overview.md`](architecture/overview.md) — shape, the
   cross-cutting safety invariants, and the index of 15 deep dives.
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

## 2. Skills

Load the one that matches your change. They carry the *how* and the traps; the
deep dives carry the *what*.

| Skill | Load it when |
|---|---|
| [planning-and-closure](.skills/planning-and-closure/SKILL.md) | You touch anything under `plans/`, or a closure claim |
| [cleanup-safety](.skills/cleanup-safety/SKILL.md) | You can delete bytes, or change what becomes `PrivateBounded` |
| [discovery-and-ownership](.skills/discovery-and-ownership/SKILL.md) | You change discovery, grouping, resolution, or learned state |
| [json-and-exit-contract](.skills/json-and-exit-contract/SKILL.md) | You touch `output.rs`, a reason code, or an exit code |
| [test-evidence](.skills/test-evidence/SKILL.md) | You write or review a test, or claim verification |
| [release-and-distribution](.skills/release-and-distribution/SKILL.md) | You release, or touch the updater or an installer |

## 3. Verification ladder

Run in order; stop at the first failure and do not report the rest.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
```

Focused test: `cargo test --all-features --test cli_contract <test_name>`
(`tests/` holds `cli_contract.rs` with 9 tests, `end_to_end.rs` with 1, plus
inline unit tests in `src/`).

If you change the CLI surface, regenerate derived artifacts or the drift gate
fails (`generate-docs -- --check` in CI):

```sh
cargo run --quiet --features dev-tools --bin generate-docs
```

Full pre-release gate: `scripts/release-check.sh` — requires a clean tree, the
`eggpack` binary, and toolchain 1.89 installed, so do not run it as routine
verification; the ladder above is the ordinary gate.

**Report what actually ran.** A record claiming a gate passed when it was not
run is worse than no record, because the next milestone trusts it.

## 4. The lesson this repository keeps re-learning

> **A green test proves only that its own premises hold.**

Seven milestones closed on a test that was passing for the wrong reason. Three
product defects (C015, C016, C017) were found by this repository's own evidence
work, and **none** was repaired under the plan that found it. A plan that finds
a defect writes a corrective; it does not fix it.

Three ways a test goes green without testing anything — and the guard that
exists for each:

1. A fixture that cannot run where it reports success →
   `scripts/check-fixture-portability.py`
2. A stub that is not the tool the subject resolves →
   `packaging/tests/test_installers.py --self-test`
3. A case whose pass condition is an unrelated failure → prefer a discriminating
   assertion (the specific reason code, not "the command failed")

Every guard takes a `--self-test` that feeds it a known-bad sample and requires
a failure. If you add a guard, the self-test is not optional: a guard that
stopped guarding is worse than no guard.

Before trusting a test, ask **"what input makes this fail?"** If you cannot
produce one, the test is decoration. See
[14-testing-and-verification §4](architecture/14-testing-and-verification.md).

## 5. Documentation conventions

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

Counts are not to be copied between documents. `architecture/overview.md` is the
source of truth for module line counts and the inline-test total; verify each
against `src/`, fix the overview first, then align the deep dives.

## 6. Planning and decision records

`plans/` is the decision record: canonical direction (`000`–`002`), the process
(`003`), accepted ADRs, subsystem roadmaps, milestone plans, and per-requirement
closure records.

- `plans/registry.md` is the compact control surface — what is open and what to
  read next. It is deliberately **not** a second copy of the roadmaps.
- Do not edit a closed plan or a closed closure record. Correct forward with a
  new corrective plan, as C001–C009 and C010–C017 all did.
- Compilation is not closure. Publication is not closure. A corrective that is
  fixed but not yet carried by a published release stays **open**.

## 7. Things that are already decided — do not relitigate

- Cleanup is **opt-in**, Cargo-mediated, and fail-closed. Deletion is always
  `cargo clean`, never direct filesystem removal.
- `allowed_output_roots` authorizes; it can never manufacture ownership proof.
- `--dryrun` spawns nothing, not even a preview dry-run, and is otherwise as
  strict as `--yes`.
- `ignore.unignore` is a literal relative path, not a glob. It predates glob
  support by design; changing it needs a plan.
- Learned state is advisory, never authority, and a persistence failure never
  changes an exit code.
- Upstream support gaps: musl and Windows ARM64/ARMv7 are Cargo-only. Do not
  add binaries without deciding that explicitly.
- `update_json` is the one non-envelope output and the only untested surface.
  Migrating it to `EnvelopeV1` is a contract change needing a plan and a version
  bump; adding a test for it needs neither.

## 8. Repository map

| Path | Contents |
|---|---|
| `src/` | the crate — 17 files; `main.rs` is the binary, `lib.rs` declares the 15 library modules |
| `tests/` | `cli_contract.rs` (32 tests), `end_to_end.rs` (2), `common/` |
| `architecture/` | `overview.md` + 15 deep dives |
| `docs/` | `RELEASING.md` (operator checklist), `TROUBLESHOOTING.md` (includes known defects by version) |
| `plans/` | decision records — see §6 |
| `.skills/` | the six skills — see §2 |
| `scripts/` | 17 scripts; `check-doc-citations.py` also gates that `architecture/14-testing-and-verification.md` §5 table is exactly these 17, so a new checker cannot ship undocumented. **11 of the 17 implement `--self-test`; the other 6 do not** — `release-check.sh` *invokes* five self-tests rather than providing one, and `gen-release-workflow-shape.py`, `release-benchmark.py`, `smoke-release-candidate.py`, `validate-staged-release.py`, and `post-release-smoke.sh` have no such mode. A new guard must ship with one; the existing six are inventoried in `architecture/14-testing-and-verification.md` §5 |
| `completions/`, `man/` | **generated** — do not hand-edit; run `generate-docs` |
| `xtask/` | `generate-docs` source (feature `dev-tools`); excluded from the published crate |
| `release/eggpack/` | `distribution.toml` is the authority for the release target matrix |
| `packaging/`, `release/` | installers, Eggpack policy, smoke and qualification scripts |
| `CHANGELOG.md` | released changes, including known defects per version |
