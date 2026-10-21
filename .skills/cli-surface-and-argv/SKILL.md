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
// src/cli.rs:89
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
// src/cli.rs:284
UsageError(String),
```

Clap enforces the selector rule, so this arm is unreachable through the parser.
It exists anyway, because **the compiler cannot see clap's invariant**, and both
available fallbacks were wrong: defaulting to Full would silently run the
machine-wide reconciliation the flag exists to gate, and panicking would exit
101 — outside the `0`/`1`/`2` contract.

Do not "simplify" it away. If you add a selector, add the matching arm and keep
the fail-closed fallback.

## The mode triple, and the two `--dry-run` flags

There are **three** `--dry-run` declarations, and they are not the same flag:

| Declaration | Where | Meaning |
|---|---|---|
| `dry_run` | root (`src/cli.rs:56-57`) | honoured wherever it appears, but only `clean` and `update` act on it; `scan` and `config` ignore it |
| `dry_run` | `clean` (`src/cli.rs:143-144`) | Simulate mode for cleanup |
| `dry_run` | `update` (`src/cli.rs:167-168`) | resolve the update plan, acquire nothing |

The canonical mapping is one function:

```rust
// src/cli.rs:284
pub fn clean_mode(dry_run: bool, cargo_preview: bool, dryrun_legacy: bool) -> CleanMode
```

It produces exactly three modes — `Execute`, `Simulate`, `CargoPreview` — and the
`debug_assert!` above it records that clap rejects conflicting flags before the
mapping runs. If you add a fourth mode, that assertion is what you must update
*and* what must keep failing when two modes are combined.

**The root-level `--dry-run` wins over the `clean`-level `--cargo-preview`.**
`src/cli.rs:392-396` checks the root flag *first*, because `--dry-run` spawns
nothing while `--cargo-preview` spawns a Cargo process. The safe mode wins the
tie. Clap cannot express this, because the two flags live at different levels
of the command tree and therefore never conflict with each other.

This also exists because `cargo cleanme --dry-run clean ROOT` is the spelling
**Cargo itself** passes to an external subcommand, so a flag parsed *before* the
subcommand must never silently fall back to Execute.

## Hidden aliases are aliases, not modes

| Alias | Hidden? | Maps to |
|---|---|---|
| `--dryrun` | yes | `--dry-run` (Simulate) |
| `--yes` | yes | the default (Execute) |
| `scan --full` | yes | `scan --all` |

They exist for the pre-1.0 migration period and must **never introduce a fourth
mode or a different safety path**. `--yes` is the sharpest trap: Execute is now
the *default*, so `--yes` is a spelling of doing nothing special. It prints
nothing.

## One cleanup code path, on purpose

`CleanupScope::Maintenance` (`src/cli.rs:186-195`) covers **both** bare
invocation and `clean` with no scope selector. The doc comment says why: bare
maintenance and every advanced cleanup spelling produce the same
`CleanupRequest`, so there is exactly one cleanup engine and one set of gates
behind all of them.

The corollary: a change to `clean --known` that does not also change bare
invocation is a bug, and a change that splits the two is a regression. Keep
`CleanupScope::Maintenance` as the shared arm.

Policy overrides (`--min-reclaimable-bytes`, `--older-than`, `--include`,
`--exclude`, `--profile`, `--package`) live **only on `clean`** today (M012A
work package D, `src/cli.rs:124-140`), because M012A deliberately declined to
clone them onto the root command.

**That decision has been superseded.** ADR 008 §4 was resolved to **4b** on
2026-10-21: the overrides become root-level, the root becomes the single
cleanup front door, and `clean` survives as a hidden compatibility alias. The
move is **not implemented** — it is M019's work, and M019 is *proposed*.

So: do not clone them onto the root as a symmetry exercise, and do not treat
their staying on `clean` as a settled design either. The decided destination
is the root; the implementation is M019's, and M019 may not be published
before M018 has shipped.

## The `long_about` lie — resolved, and the drift mechanism that hid it

`src/cli.rs:27-28` used to say that `cargo cleanme scan` with no ROOT is the
full read-only machine reconciliation. It is a usage error. Only
`cargo cleanme --help` showed that text — the man page overwrote the root
`long_about` with its own block in `xtask`, so the man page never saw the
lie and the drift gate could not catch it.

That was **C036 D1**, and it is **discharged**: C040 (ADR 009) rewrote the
root `long_about` so it states that `scan` needs a scope, and made the root
man page render from **that same `long_about`** (`src/main.rs:148-156`)
instead of from a generator-side block. The two surfaces can no longer drift.

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

## The `--full` spelling is decided; it is not implemented yet

ADR 008 (`plans/adr/008-…`) was **accepted 2026-10-21**. It decides that
`cargo cleanme --full` becomes the canonical Full-cleanup spelling, with
`cargo cleanme clean --full` retained as a **hidden** compatibility alias (it
has been published since 0.2.4), and that the policy overrides become
root-level too.

**Acceptance is not implementation.** The tree still has no root-level
`--full`, `clean` is still visible, and `clean --full` is still the
documented spelling. The implementation plans are M018 (the Full gate) and
M019 (the spelling), and both are *proposed* — they carry no implementation
authority yet.

Until M019 lands:

- do **not** add a root-level `--full` flag;
- do **not** remove or hide `clean --full`;
- if you touch the CLI, assume both spellings must keep working.

And note the sequencing rule ADR 008 makes binding: **M019 may not be
published before M018 has shipped**, because the canonical spelling must
never point at an operation that refuses.

See [`planning-and-closure`](../planning-and-closure/SKILL.md) before acting
on any of it.

## A flag can be accepted and ignored — and that is a defect shape

A root-level `--dry-run` passed to `scan` is **accepted by clap and then
silently ignored** (the `Invocation::Scan` arm never reads it). Harmless today,
because a scan is read-only, but it is exactly the shape of the discarded-flag
defects that were already fixed on the `Clean` and `Update` arms.

When you add a root-level flag, decide which arm reads it, and make the
ignoring arm explicit rather than accidental.

## Before you claim the change is safe

- `cargo cleanme scan` is still a **usage error** (exit 2) with a message that
  names the required selectors.
- `cargo cleanme scan --known --all` is still a usage error.
- `--dry-run`, `--cargo-preview`, and `--yes` still conflict pairwise, and the
  mode triple still maps onto exactly three modes.
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
  spelling is not yours to decide
