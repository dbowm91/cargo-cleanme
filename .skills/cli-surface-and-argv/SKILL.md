---
name: cli-surface-and-argv
description: Changing the cargo-cleanme command line — subcommand normalization, scope selectors, mode flags, hidden aliases, and the generated artifacts that drift with every edit
version: 1.0.0
tags:
  - cli
  - argv
  - clap
  - generated
---

# CLI Surface and Argv

The command line is the only part of this tool that is **published, remembered,
and quoted back to you by users**. Everything else can change without ceremony;
this cannot. A flag that changes meaning between releases is a breaking change to
a contract nobody versioned.

The reference is [`architecture/02-cli.md`](../../architecture/02-cli.md) and
[`architecture/13-orchestration.md`](../../architecture/13-orchestration.md). This
skill is about *how to change it without breaking it*.

## When to Load

- Any edit to `src/cli.rs`, or to the argv dispatch in `src/main.rs`
- Adding, renaming, or removing a flag, a subcommand, or a selector
- Changing what a flag *means* (as opposed to where it lives)
- Touching the completions, the man pages, or the root `long_about`

## The one thing to get right

> **A selector that is missing must be a usage error, never a silent default.**

The machine-wide Full reconciliation is the operation this tool can least afford
to run by accident, and it is the one whose absence-of-argument spelling has
changed twice. `cargo cleanme scan` with no selector is a **usage error (exit
2)** — not Full, not Routine.

It is enforced by one `ArgGroup`:

```rust
// src/cli.rs:244
#[command(group = clap::ArgGroup::new("scan-scope").required(true).args(["root", "known", "all", "full"]))]
```

Two properties come from that single declaration, and both are load-bearing:

1. the group is **required**, so `scan` alone is rejected;
2. the group's members are **mutually exclusive**, so `scan --known --all` is
   rejected.

Do not replace it with per-argument `required_unless_present_all`. In clap 4 a
*positional* ROOT does not satisfy `required_unless_present`, so `scan /tmp`
would still demand `--all`. That is why the group exists, and the comment above
it says so.

## `Invocation::UsageError` is not dead code

```rust
// src/cli.rs:426
UsageError(String),
```

Clap enforces the selector rule for `scan` alone, but the compiler cannot see
clap's invariant, and both available fallbacks were wrong: defaulting to Full
would silently run the machine-wide reconciliation the flag exists to gate,
and panicking would exit 101 — outside the `0`/`1`/`2` contract.

It was **unreachable** when clap enforced the whole `scan` selector rule. It is
**reachable now**: M019 turned three argv shapes that clap cannot express into
usage errors, and each lands here —

- a cleanup flag handed to `scan`, `config` or `update`
  (`src/cli.rs:637-639`);
- the same override given at both the root and on `clean`
  (`src/cli.rs:713-717`);
- the two mutually exclusive selectors given at *different* levels —
  `--profile` at the root and `--package` on `clean`
  (`src/cli.rs:728-737`). Clap's `conflicts_with` only sees one level, and
  merging them independently would discard one of the two: the operator would
  ask for a package and get a profile, which is a different destructive
  action;
- a scope selector given at both levels, which clap's `ArgGroup` cannot see
  across the root and the subcommand (`src/cli.rs:542-544`).

Do not "simplify" it away, and do not assume clap got there first. If you add
a selector, add the matching arm and keep the fail-closed fallback.

## The mode triple, and the two `--dry-run` flags

There are **two** `--dry-run` declarations, and they are not the same flag:

| Declaration | Where | Meaning |
|---|---|---|
| `dry_run` | the shared cleanup surface (`src/cli.rs:42-43`), carried by the root and by `clean` | honoured wherever it appears, but only `clean` and `update` act on it; `scan` and `config` ignore it |
| `dry_run` | `update` (`src/cli.rs:306-308`) | resolve the update plan, acquire nothing |

The canonical mapping is one function:

```rust
// src/cli.rs:434
pub fn clean_mode(dry_run: bool, cargo_preview: bool, dryrun_legacy: bool) -> CleanMode
```

It produces exactly three modes — `Execute`, `Simulate`, `CargoPreview` — and the
`debug_assert!` above it records that clap rejects conflicting flags before the
mapping runs. If you add a fourth mode, that assertion is what you must update
*and* what must keep failing when two modes are combined.

**A simulation at either level wins over a Cargo preview at either level.**
`src/cli.rs:676-681` checks the simulation flags *first*, because `--dry-run`
spawns nothing while `--cargo-preview` spawns a Cargo process. The safe mode
wins the tie. Clap cannot express this, because the two flags live at
different levels of the command tree and therefore never conflict with each
other.

This also exists because `cargo cleanme --dry-run clean ROOT` is the spelling
**Cargo itself** passes to an external subcommand, so a flag parsed *before* the
subcommand must never silently fall back to Execute.

## Hidden aliases are aliases, not modes

| Alias | Hidden? | Maps to |
|---|---|---|
| `--dryrun` | yes | `--dry-run` (Simulate) |
| `--yes` | yes | the default (Execute) |
| `scan --full` | yes | `scan --all` |
| the `clean` subcommand | yes | the root front door |
| `clean --full` | yes | `cargo cleanme --full` |

They exist for the pre-1.0 migration period and must **never introduce a fourth
mode or a different safety path**. `--yes` is the sharpest trap: Execute is now
the *default*, so `--yes` is a spelling of doing nothing special. It prints
nothing.

## One cleanup code path, on purpose

`CleanupScope::Maintenance` (`src/cli.rs:326`) covers **both** bare
invocation and `clean` with no scope selector. The doc comment says why: bare
maintenance and every advanced cleanup spelling produce the same
`CleanupRequest`, so there is exactly one cleanup engine and one set of gates
behind all of them.

The corollary: a change to `clean --known` that does not also change bare
invocation is a bug, and a change that splits the two is a regression. Keep
`CleanupScope::Maintenance` as the shared arm.

Policy overrides (`--min-reclaimable-bytes`, `--older-than`, `--include`,
`--exclude`, `--profile`, `--package`) are **root-level** now, and `clean`
carries the *same* declaration hidden. M019 shipped ADR 008 §4b: the root is
the single cleanup front door, `clean` survives as a hidden compatibility
alias, and the overrides exist at both levels. C045 collapsed the two
declarations into one — `CleanupFlags` (`src/cli.rs:35-83`), flattened into
the root (`src/cli.rs:167-168`) and into `clean` (`src/cli.rs:286-288`) — so a
new override costs one edit, and
`the_alias_accepts_every_cleanup_flag_the_root_declares` (`src/cli.rs:1247`)
fails if the two levels ever diverge.

The two levels **merge**, they do not stack:

- A value given at the **root** wins.
- A value given on `clean` is used when the root did not supply one.
- The **same** override at both levels is a **usage error** (exit 2)
  (`src/cli.rs:728-751`), because silently preferring one of two values is the
  discarded-flag failure mode ADR 008 §3 exists to prevent.

## The `long_about` lie — resolved, and the drift mechanism that hid it

`src/cli.rs:104` is the root `long_about`. It used to say that
`cargo cleanme scan` with no ROOT is the full read-only machine
reconciliation. It is a usage error. Only `cargo cleanme --help` showed that
text — the man page overwrote the root `long_about` with its own block in
`xtask`, so the man page never saw the lie and the drift gate could not catch
it.

That was **C036 D1**, and it is **discharged**: C040 (ADR 009) rewrote the
root `long_about` so it states that `scan` needs a scope, and made the root
man page render from **that same `long_about`** (`xtask/src/main.rs:141-156`)
instead of from a generator-side block. The two surfaces can no longer drift.
M019 then extended that same `long_about` to name the canonical spellings —
`cargo cleanme ROOT` and `cargo cleanme --full` — so the front door is stated
in the one surface every user reads.

The lesson survives the fix: a surface that is rendered from a *different*
source than the one being edited is invisible to the gate. When you change a
user-visible string, check which artifacts render it.

## The generated artifacts drift with every edit

`completions/` (5 files) and `man/` (8 files) — **13 artifacts** — are generated
from the clap model by `xtask` (feature `dev-tools`). They are **not**
hand-editable and CI fails on drift.

```sh
cargo run --quiet --features dev-tools --bin generate-docs          # regenerate
cargo run --quiet --features dev-tools --bin generate-docs -- --check # drift gate
```

The generated artifacts document the **`cargo cleanme`** spelling, not
`cleanme`, because that is the spelling that resolves after a registry install.

## The `--full` spelling is shipped; the root is the front door

ADR 008 (`plans/adr/008-…`) was **accepted 2026-10-21** and M019 has landed
it. `cargo cleanme --full` is the canonical Full-cleanup spelling;
`cargo cleanme clean --full` survives as a **hidden** compatibility alias (it
has been published since 0.2.4), and the policy overrides are root-level too.

The root is the **single cleanup front door** (`src/cli.rs:539-623`):

| Spelling | Scope |
|---|---|
| `cargo cleanme` | the maintenance scope |
| `cargo cleanme ROOT` | one bounded root |
| `cargo cleanme --full` | the Full cleanup |

`cargo cleanme clean …` is the same intent reached through the hidden alias
(`src/cli.rs:284`, `src/cli.rs:513-517`). `clean` is *hidden*, not removed:
`man/cargo-cleanme-clean.1` still ships and still documents a working
spelling, and it is generated from the same clap model, so it cannot drift.
The **five generated completions advertise it too** — `clap_complete`
enumerates every subcommand, hidden ones included, and has no way to suppress
one — so `cargo cleanme <TAB>` offers `clean` beside `scan`, `config` and
`update`. That is a recorded decision (C036 D49): the alias works, and
excluding it from the completions would mean either removing it from the clap
model, which breaks the single-model property C045 established, or editing the
generated text per shell.
`cargo cleanme .` is a bounded cleanup of `.`.

Two rules make the alias safe, and both are **usage errors (exit 2)**:

- A **cleanup flag handed to a read-only or local-only subcommand** — `scan`,
  `config`, `update` — is refused, never silently discarded
  (`src/cli.rs:637-639`).
- The **same** override at both levels is ambiguous, and is refused
  (`src/cli.rs:713-717`).

A **simulation at either level wins over a Cargo preview at either level**
(`src/cli.rs:676-681`): `--dry-run` says "invoke no `cargo clean` at all", a
Cargo preview does invoke one, so the safe mode takes the tie.

One collision to know: a directory whose name equals a subcommand name is read
as that **subcommand** — `cargo cleanme scan` is the scan, not a cleanup of
`./scan`. Spell such a directory `./scan`.

ADR 008's sequencing rule still binds *publication*: M019 may not be
published before M018 has shipped, because the canonical spelling must never
point at an operation that refuses. Both are implemented in the tree; neither
has shipped in a published release yet, so do not describe them as released.

See [`planning-and-closure`](../planning-and-closure/SKILL.md) before acting
on any of it.

## `--verbose` selects a rendering shape; it is not a mode

`--verbose` (`src/cli.rs:146`) is a **global** flag on the root `Cli`, like
`--config`, `--no-progress`, `--stats` and `--format`. It is global because it
selects a rendering shape and every report-producing subcommand renders one.

It changes the **human** format only. `--format json` and `--format log` are
byte-for-byte identical with and without it, and it changes no exit code, no
proof, no scope, and no cleaned set. That is the property to preserve when you
touch it: a rendering flag that moves a byte in the versioned machine contract
is a contract change, and it needs a plan and a version bump.

The **default** human surface names only what the run acted on: one line per
directory cleaned, one tally, one progress line. The skipped workspaces are
counted, not listed. The ADR 007 safety disclosures — the scope block, the
unresolved-participant listing, the out-of-scope count — are **never**
withheld: they say what the report *means*, not how long it is.

If you add a flag that selects output, ask the same question `--verbose`
answers: does it change the machine contract? If yes, it is not a rendering
flag.

## A flag can be accepted and ignored — and that is a defect shape

A root-level `--dry-run` passed to `scan` or `config` is **accepted by clap and
then silently ignored** — those operations are read-only or local-only, so the
flag changes no behaviour. It is harmless today, but it is exactly the shape
of the discarded-flag defects that were already fixed on the `Clean` and
`Update` arms.

Every **cleanup** flag is different, and M019 closed that shape: `--full`,
ROOT, `--min-reclaimable-bytes`, `--older-than`, `--include`, `--exclude`,
`--profile`, `--package` and `--cargo-preview` handed to `scan`, `config` or
`update` are a **usage error** (exit 2), never a silently discarded flag
(`src/cli.rs:571-575`). Discarding them would look exactly like the flag was
honoured.

When you add a root-level flag, decide which arm reads it, and make the
ignoring arm explicit rather than accidental.

## Before you claim the change is safe

- `cargo cleanme scan` is still a **usage error** (exit 2) with a message that
  names the required selectors.
- `cargo cleanme scan --known --all` is still a usage error.
- `--dry-run`, `--cargo-preview`, and `--yes` still conflict pairwise **on
  `clean`**, and the mode triple still maps onto exactly three modes.
- `cargo cleanme`, `cargo cleanme ROOT` and `cargo cleanme --full` all work,
  and `cargo cleanme clean …` still works: the alias is hidden, not removed.
- A cleanup flag handed to `scan`, `config` or `update` is still a **usage
  error** (exit 2), and so is the same override given at both levels.
- The hidden aliases still print nothing and still select the canonical modes.
- `cargo run --quiet --features dev-tools --bin generate-docs -- --check` passes.
- The contract suite in `tests/cli_contract.rs` still covers the spelling you
  changed — and it pins exit codes `0`, `1`, **and** `2`, so a usage error that
  silently became exit `1` or `0` fails the build.

## Related

- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md) — the shape of
  what the argv selects, once it has been selected
- [`terminal-and-interactive`](../terminal-and-interactive/SKILL.md) — the one
  argv flag that drives a terminal
- [`planning-and-closure`](../planning-and-closure/SKILL.md) — why the `--full`
  spelling is not yours to re-decide
