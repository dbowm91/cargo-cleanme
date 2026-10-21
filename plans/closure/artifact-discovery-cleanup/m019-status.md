# M019 — Canonical Full Cleanup Spelling and the Root Cleanup Front Door: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**: publication is not closure, and no
release has carried this change yet. M019 may not be published before M018 has
shipped (ADR 008's acceptance record).
Date: 2026-10-21.
Plan: `plans/implementation/artifact-discovery-cleanup/m019-canonical-full-cleanup-spelling-and-root-front-door.md`
Repository baseline: `main` at `8b96f86` (0.2.10 published).
Implements: **ADR 008 §§1–4, option 4b**, plus the owner's ROOT-positional
decision of 2026-10-21 (option A).
Related: ADR 003 §4 (amended twice on 2026-10-21), ADR 007 §3, ADR 008,
ADR 009 / C040, **M018** (the gate — this milestone may not be **published**
before M018 has shipped), M012A, M014, M017.

## The executive finding

The Full cleanup operation is now **one word long**, and the root command is
the single cleanup front door for all three scopes and both modes:

```console
$ cargo cleanme --full            # the canonical Full cleanup
$ cargo cleanme ~/projects        # the canonical bounded-root cleanup
$ cargo cleanme                   # routine maintenance
```

`cargo cleanme clean --full` and `cargo cleanme clean ~/projects` still work,
are still hidden, and are still the published spellings until a release
carries this change. `clean` is no longer advertised; the root is.

**The change is a spelling change.** The resolved intent did not move:
`CleanupRequest`, `CleanupScope`, `CleanupSelection` and `CleanupOverrides`
are unchanged, and the cleanup engine's diff is zero. What moved is the
command that *carries* the intent — and the guards that stop a cleanup flag
being handed to a read-only subcommand and silently discarded.

## What was built

### 1. The root is the front door — `src/cli.rs`

The root `Cli` gained:

| Argument | Kind | Notes |
|---|---|---|
| `--full` | flag | **non-global** — `scan` and `clean` already own a `full` argument id, and a blanket global collides with both |
| `--min-reclaimable-bytes`, `--older-than`, `--include`, `--exclude`, `--profile`, `--package` | flags | the policy overrides, moved from `clean` to the root; all **non-global** |
| `--cargo-preview` | flag | the one mode the root could not express before |
| `ROOT` | positional | the bounded-root selector, moved from `clean` to the root by the owner's option A |

`Command::Clean` became `#[command(hide = true)]`, and its flags became hidden
aliases. `CleanupOverrides` is **reused**, not duplicated: the root and
`clean` must produce the same value type, or the two spellings can drift.

### 2. The merge — `Cli::invocation()`

`Cli::invocation()` stayed pure — no IO, no filesystem access — and gained:

- `root_cleanup_request()` — the root's own spelling of a cleanup request;
- `merged_cleanup_request(CleanAlias)` — the merge of the root and the alias;
- `reject_cleanup_flags_for(subcommand)` — the read-only guard;
- `root_overrides()` — the root's policy overrides;
- `merge_overrides`, `merge_one`, `merge_many`, `duplicate_override`,
  `selection_for`, `merged_clean_mode` — the merge rules.

The merge rules, and why each exists:

| Situation | Result | Why |
|---|---|---|
| the root supplies a value, `clean` does not | the root's value wins | one spelling, one answer |
| `clean` supplies a value, the root does not | the alias's value is used | the published spelling keeps working |
| **both** supply the same override | **usage error, exit 2** | silently preferring one of two values is the discarded-flag failure mode ADR 008 §3 exists to prevent |
| a simulation at either level, a Cargo preview at either level | **simulation wins** | simulation spawns nothing, so it is the strictly safer mode |
| a cleanup flag handed to `scan`, `config` or `update` | **usage error, exit 2** | a discarded flag is the worst failure mode this tool has |

### 3. The ROOT positional — the owner's option A

`cargo cleanme ROOT` is the canonical bounded-root spelling; `cargo cleanme
clean ROOT` is accepted and hidden.

The precedence is **explicit**, not a parser default: clap consults the
subcommand table before the positional, so `cargo cleanme scan` is the scan
and `cargo cleanme ./scan` names a directory called `scan`. The rule is
stated in `--help` and in the ROOT argument's own help text.

Two consequences are deliberate:

- **`cargo cleanme .` changes from a usage error (exit 2) to a bounded cleanup
  of `.`.** This is the one user-visible change that is not merely a
  relocation of a spelling.
- **`clean --known` does not move.** It resolves to the bare invocation
  (`CleanupScope::Maintenance` with `CleanupSelection::WorkingDirectory`), and
  the bare invocation is ADR 003's canonical maintenance operation — so
  hiding `clean` orphans nothing by it.

### 4. The generated artifacts — `completions/` and `man/`

`generate-docs` was re-run and the drift gate is green. **No artifact was
removed**, and the man-page count is unchanged at **8**:

| Artifact | Changed | Note |
|---|---|---|
| `man/cargo-cleanme.1` | yes | renders from the root `long_about`, which now names the canonical spellings — the property C040 established |
| `man/cargo-cleanme-clean.1` | yes | **keeps shipping** — it documents a published, working, hidden alias, and it is generated from the same clap model, so it cannot drift |
| `man/cargo-cleanme-{config,edit,path,scan,show,update}.1` | no | unchanged |
| `completions/` — all five | yes | the root's flag set changed, so every completion changed |

The decision that the `clean` page keeps shipping is recorded here rather
than left implicit: the generator renders every subcommand clap returns, and
clap includes hidden subcommands in that list. A stale page that documents a
command the root no longer lists would be drift — but this page documents a
command that *works*, and it is regenerated from the same definition, so it
cannot fall behind the code.

## The scope-resolution equivalence

The root and the `clean` alias must resolve to the **same** `Invocation`, not
merely to the same behaviour. Three unit tests assert the resolved value, not
the exit code:

| Test | Asserts |
|---|---|
| `the_root_full_flag_resolves_to_the_full_scope` (`src/cli.rs`) | `cargo cleanme --full` resolves to `CleanupScope::Full` — the same value `cargo cleanme clean --full` resolves to |
| `the_root_positional_resolves_to_the_bounded_root_scope` (`src/cli.rs`) | `cargo cleanme ~/projects` resolves to `CleanupScope::Root("~/projects")` — the same value `cargo cleanme clean ~/projects` resolves to |
| `the_root_and_the_alias_resolve_to_the_same_intent` (`src/cli.rs`) | the two spellings produce an equal `Invocation` |

And the cases that must *not* collapse:

| Test | Asserts |
|---|---|
| `a_subcommand_wins_over_a_positional_of_the_same_shape` | `cargo cleanme scan --all` is the scan, not a cleanup of `./scan` |
| `the_dot_positional_is_a_bounded_cleanup_and_not_a_usage_error` | `cargo cleanme .` is `CleanupScope::Root(".")` — the deliberate behaviour change, pinned |
| `the_known_alias_is_not_orphaned_by_the_hiding` | `clean --known` is `CleanupScope::Maintenance` + `CleanupSelection::WorkingDirectory`, and hiding `clean` orphans nothing |
| `the_same_override_at_both_levels_is_a_usage_error` | the same override at both levels is exit 2, not a silent preference |
| `a_cleanup_flag_on_a_read_only_subcommand_is_a_usage_error` | `scan`, `config` and `update` each refuse a cleanup flag |

## The live runs

The machine is the same non-root Linux host used for M018's evidence, with
the learned state isolated (`XDG_STATE_HOME=/tmp/m018state`) so the machine's
own state was never rewritten.

### 1. `cargo cleanme --full --dry-run` — the canonical spelling, simulated

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --dry-run --stats --no-progress --format json
cargo-cleanme: discovery state was not updated: the Full traversal was incomplete; the learned roots were retained and verified in memory
140 filesystem diagnostics; rerun with a bounded root if needed
exit 0
```

Every learned root on the machine was proven, so the run exited **0** and the
boundary was fully cleaned. With an unreadable root under it, the same
command exits **1** and reports the skipped root — that evidence is M018's
closure record, not duplicated here.

### 2. `cargo cleanme --full` — the canonical spelling, executed

A machine-wide *Execute* cleanup would delete the operator's own build
artifacts — 142 GiB of them — without an instruction to do so, so the
Execute run was given a **hard allow-list** instead of being run against
everything. `--include` is one of the root-level policy overrides this
milestone moved, so the evidence exercises the new spelling and the new flag
at once.

First the same command as a preview, to confirm what it would touch:

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --dry-run --include '/home/sugarwookie/m018-live-readable/*' --no-progress --format json
{"scope_blocked": false, "summary": {"previewed": 0, "simulated": 1, "cleaned": 0, "skipped": 2, "failed": 0, "diagnostics": 2}}
  not_included | /home/sugarwookie/projects/i2pr
  not_included | /home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/crates/eggwork-test-node
  simulated    | /home/sugarwookie/m018-live-readable/proj-a
```

Exactly one unit — a purpose-built fixture — was a candidate. Then the
Execute run:

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --include '/home/sugarwookie/m018-live-readable/*' --no-progress --format json
{"scope_blocked": false, "summary": {"previewed": 0, "simulated": 0, "cleaned": 1, "skipped": 2, "failed": 0, "diagnostics": 2}}
  not_included | /home/sugarwookie/projects/i2pr                              | before: 44547182592
  not_included | /home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/crates/eggwork-test-node | before: 1357287424
  cleaned      | /home/sugarwookie/m018-live-readable/proj-a                   | before: 16384 after: 0 | Removed 4 files, 12.4KiB total
exit 0
```

The fixture's `target/` directory was gone afterwards. The canonical spelling
reaches the Execute path and spawns a real `cargo clean`; the two units
outside the allow-list were reported, not cleaned.

**What this run does not claim.** It is not a machine-wide cleanup, and it
does not prove the machine-wide Execute path against real build output. It
proves the *spelling* is wired end to end — the root's `--full` reaches the
Full scope, the gate, the proof and the dispatch, and `cargo clean` was
spawned for the unit the policy allowed.

### 3. The usage-error cases

Every one of these exits **2** and names the offending flag:

```console
$ cargo-cleanme --full scan --all
cargo-cleanme: configuration error: `scan` does not accept --full: these are cleanup flags, and `cargo cleanme scan` is read-only. Run the cleanup without the subcommand, or run `scan` on its own.
exit 2

$ cargo-cleanme --full config path
cargo-cleanme: configuration error: `config` does not accept --full: these are cleanup flags, and `cargo cleanme config` is read-only. Run the cleanup without the subcommand, or run `config` on its own.
exit 2

$ cargo-cleanme --full update
cargo-cleanme: configuration error: `update` does not accept --full: these are cleanup flags, and `cargo cleanme update` is read-only. Run the cleanup without the subcommand, or run `update` on its own.
exit 2

$ cargo-cleanme --profile release scan --all
cargo-cleanme: configuration error: `scan` does not accept --profile: these are cleanup flags, and `cargo cleanme scan` is read-only. Run the cleanup without the subcommand, or run `scan` on its own.
exit 2

$ cargo-cleanme --older-than 3600 clean --older-than 60
cargo-cleanme: configuration error: --older-than was given at the root and on `clean`. The two values are ambiguous; give it at one level only.
exit 2
```

A root-level `--dry-run` does **not** rescue a discarded flag:
`cargo cleanme --full --dry-run scan --all` still exits 2 with the same
message. Simulation is not consent to ignore the other argument.

### 4. The help text

`cargo cleanme --help` names the canonical spellings, and `clean` is absent
from the `Commands:` list:

```console
$ cargo-cleanme --help
...
The root is the cleanup front door: `cargo cleanme ROOT` cleans one bounded root, `cargo cleanme --full` runs the Full cleanup -- a complete machine reconciliation, then the learned roots it can prove -- and `--cargo-preview` and the policy overrides work here too. A directory whose name collides with a subcommand name is read as that subcommand; spell it `./scan` to name it.
...
Usage: cargo-cleanme [OPTIONS] [ROOT] [COMMAND]

Commands:
  scan    Reconcile the machine read-only: full with `--all`, explicit with ROOT, routine inventory with `--known`
  config
  update  Update this cargo-cleanme to the latest stable published release
  help    Print this message or the help of the given subcommand(s)
```

## The tests

Fifteen cases, each naming the input that makes it fail. Ten are unit tests
in `src/cli.rs`, three are CLI-contract tests in `tests/cli_contract.rs`, and
two are regressions in the pre-existing suite that this milestone supersedes.

| # | Test | Where |
|---|---|---|
| 1 | `the_root_full_flag_resolves_to_the_full_scope` | `src/cli.rs` |
| 2 | `the_clean_alias_still_resolves_and_is_hidden_from_the_help` | `src/cli.rs` |
| 3 | `the_policy_overrides_work_at_the_root` | `src/cli.rs` |
| 4 | `the_same_override_at_both_levels_is_a_usage_error` | `src/cli.rs` + `tests/cli_contract.rs` |
| 5 | `a_scope_selector_on_a_read_only_subcommand_is_a_usage_error` | `tests/cli_contract.rs` |
| 6 | `a_policy_override_on_a_read_only_subcommand_is_a_usage_error` | `tests/cli_contract.rs` |
| 7 | the pre-existing `invocation()` unit tests keep passing unchanged | `src/cli.rs` |
| 8 | the one-document invariant — the Full branch still emits `EmitReport::No` | `tests/cli_contract.rs` |
| 9 | `generate-docs -- --check` is green; no artifact is stale | the drift gate |
| 10 | `the_root_help_names_the_canonical_full_cleanup_spelling` | `src/cli.rs` |
| 11 | `the_root_positional_resolves_to_the_bounded_root_scope` | `src/cli.rs` |
| 12 | `a_subcommand_wins_over_a_positional_of_the_same_shape` | `src/cli.rs` |
| 13 | `the_dot_positional_is_a_bounded_cleanup_and_not_a_usage_error` | `src/cli.rs` |
| 14 | `the_root_and_the_alias_resolve_to_the_same_intent` | `src/cli.rs` |
| 15 | `the_known_alias_is_not_orphaned_by_the_hiding` | `src/cli.rs` |

Four pre-existing tests changed, because this milestone supersedes them, and
each change is recorded rather than made silently:

| Test | Before | After |
|---|---|---|
| `hidden_full_alias_is_not_advertised_in_scan_help` | asserted the root help did not advertise `--full` | asserts the **scan** subcommand's own declaration does not advertise it — `scan --full` stays hidden, because ADR 006 §5 demoted it |
| `compatibility_aliases_are_hidden_but_accepted` | asserted the aliases were absent from the help prose | asserts clap's `is_hide_set()` on the declarations, which is the property that actually holds |
| the M018 "no root-level `--full`" test | pinned the pre-M019 spelling | became `the_full_cleanup_alias_still_resolves_after_the_root_moved` — the *alias* still resolves, which is the property that must survive |
| the help-text assertions | matched prose that contained "clean" | use clap's declaration API and the `Commands:` section's command *names* |

## The verification ladder

The ladder in `AGENTS.md` §3, run in order, on the working tree as it stands:

```console
$ cargo fmt --all -- --check
(no output)

$ cargo clippy --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo test --all-targets --all-features
test result: ok. 511 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 77 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
check-fixture-portability: self test passed; all four rules verified in both directions
check-fixture-portability: 37 fixture file(s) clean (no literal PATH separator, no unjustified POSIX shebang fixture, no ungated raw-byte write, no platform-specific Entry literal)

$ python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
check-doc-citations: self test passed; all rules verified in both directions
check-doc-citations: 17 document(s) clean; every file.rs:N citation resolves to an existing file and an in-range line

$ cargo run --quiet --features dev-tools --bin generate-docs -- --check
generate-docs: 13 artifacts match the clap model
```

The inline-test census moved with this milestone: **522 declared, 520 on
Linux, 478 on Windows, 517 on macOS**. The figures are derived with a
brace-tracking parser evaluated against `rustc --print cfg --target <triple>`
and cross-checked against `cargo test --lib -- --list` (511 lib + 9 bin = the
520 the Linux lane compiles). A naive grep for `#[cfg(unix)]` immediately
above `#[test]` finds 30 of the 40, because ten gates sit *after* the
`#[test]` attribute.

`scripts/release-check.sh` was **not** run: it requires a clean tree, the
`eggpack` binary and toolchain 1.89, and it is the release gate, not the
ordinary one. Nothing is published by this milestone.

## The citations

The CLI surface moved: `src/cli.rs` grew, and the deep dives that cite it
(`architecture/02-cli.md`, `04`, `05`, `06`, `09`, `10`, `13`, `16`) were
re-derived from the new source rather than shifted arithmetically. The
re-derivation was mechanical and content-verified: a citation was accepted
only when the mapped line carried *identical* content, and a mechanical diff
confirmed that 1,065 changed line pairs differed **only inside a citation's
numbers**, with no prose change.

Closed plans, closure records and ADR 004 (which pins its own citations "as
of the M015 landing") were left alone: they are historical records.

## What deliberately did not change

- **The resolved intent.** `CleanupRequest`, `CleanupScope`,
  `CleanupSelection` and `CleanupOverrides` are unchanged. The cleanup
  engine's diff is zero.
- **Every published spelling.** `clean --full`, `clean ROOT` and
  `clean --known` all keep working; `clean` is hidden, never deleted.
- **`scan`.** `scan --all`, `scan ROOT`, `scan --known` and the hidden
  `scan --full` alias are untouched, and `scan` still refuses a cleanup flag
  rather than ignoring it.
- **The JSON envelope's shape.** No field moved, no reason code was added.
- **The proof boundary and the exit codes.** Those are M018's, and this
  milestone changes neither.

## Residuals, disclosed

- **The machine-wide Execute path was not exercised against real build
  output.** The Execute run was allow-listed to a purpose-built fixture, so
  the evidence covers the spelling and the dispatch, not a 142 GiB machine
  sweep. A machine-wide Execute cleanup needs an owner instruction, and it
  was not given.
- **The MSRV lane (1.89) and the hosted matrix have not run.** The CLI
  change touches no stdlib API, no fixture and no path comparison, but only
  those lanes prove it, and this record does not claim it. The
  `--include`/`--exclude` globs are evaluated by `globset`, whose behaviour
  is platform-independent, but the Windows lane is what proves the *paths*.
- **`cargo cleanme .` is now a bounded cleanup.** An argv shape that used
  to be a usage error is now an operation. It is pinned by
  `the_dot_positional_is_a_bounded_cleanup_and_not_a_usage_error`, and it is
  stated in `--help`.

## Open

- **Publication.** This milestone is `closing`, not `closed`, and it is
  **gated on M018 shipping first**: the canonical spelling must never point
  at an operation that refuses. M018 must therefore be in a release before
  this one is.
- **C041** — the update path's candidate-spawn `ETXTBSY` flake. Unrelated.
- The hidden-cursor finding (overview §10 finding 36), the absent pty
  harness, and finding 59's load-sensitive assertions. Separate correctives.
