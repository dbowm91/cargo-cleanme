# M019 — Canonical Full Cleanup Spelling and the Root Cleanup Front Door (ADR 008 §§1–4)

Status: **closing** — implemented and verified in the working tree; **not
yet published.** The milestone stays **open**: publication is not closure, and
no release has carried this change yet. M019 may not be published before M018
has shipped (ADR 008's acceptance record).
Date: 2026-10-21.
Repository baseline: `main` at `8b96f86` (0.2.10 published).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Implements: **ADR 008 §§1–4, option 4b** — the canonical Full cleanup spelling
becomes `cargo cleanme --full`, the policy overrides become root-level, `clean`
survives as a hidden compatibility alias, and a scope selector or policy
override handed to a read-only subcommand is a usage error.
Related: ADR 008 (accepted 2026-10-21; §4 resolved to 4b; **amended 2026-10-21
with the owner's ROOT-positional decision**), ADR 003 §4 (amended by ADR 008 on
2026-10-21, and **amended again on 2026-10-21** by the owner's ROOT-positional
decision), ADR 007 §3 (a discarded flag is the
worst failure mode this tool has), **M018** (the gate — this milestone may not
be **published** before M018 has shipped), ADR 009 / C040 (the root man page
renders from the root `long_about`), M012A, M014, M017.
Publication: **gated on M018 shipping first** — see "Sequencing".

## Objective

Make the Full cleanup operation one word long, and make the root command the
single cleanup front door:

- `cargo cleanme --full` is the **canonical** spelling of the Full cleanup
  operation;
- `cargo cleanme clean --full` remains accepted as a **hidden compatibility
  alias** — published since 0.2.4, so hidden, never deleted;
- the **ROOT positional moves to the root**: `cargo cleanme ~/projects` is the
  canonical bounded-root spelling, and `cargo cleanme clean ~/projects`
  remains accepted and hidden;
- the policy overrides (`--min-reclaimable-bytes`, `--older-than`,
  `--include`, `--exclude`, `--profile`, `--package`) become root-level;
- `--full`, and any policy override, handed to a read-only or local-only
  subcommand is a **usage error (exit 2)** — never a silently discarded flag.

## Non-goals

- **Not** the gate. M018 owns the Full cleanup's proof boundary; this
  milestone changes no proof, no discovery, no ownership rule, and no exit
  code.
- **Not** a change to the cleanup engine, the scope resolution, the policy
  evaluation, or the JSON envelope. The spelling changes; the semantics do
  not.
- **Not** a removal of any published spelling. `clean ROOT`, `clean --known`
  and `clean --full` all keep working.
- **Not** a change to `scan`. `scan --all`, `scan ROOT`, `scan --known` and
  the hidden `scan --full` alias are untouched; `scan --full` stays hidden
  because ADR 006 §5 demoted it.
- **Not** a change to the read-only `update` surface.

## Sequencing — binding

ADR 008's acceptance record: **the spelling change (§§1–§4) and the gate
change (§5) are sequenced together, never separately released.** The canonical
spelling must never point at an operation that refuses.

Therefore **this milestone may not be published before M018 has shipped.**
Implementation may proceed in parallel; the release is gated. M018 ships first
with `clean --full` still the published spelling — the capability lands
before the ergonomics.

## Contract (decided, under ADR 008 §§1–4)

### Flag placement

| Flag | Root | `clean` | Notes |
|---|---|---|---|
| `--full` | **canonical** | hidden alias | root-level, **non-global** |
| `--min-reclaimable-bytes` | **canonical** | hidden alias | policy override |
| `--older-than` | **canonical** | hidden alias | policy override |
| `--include` | **canonical** | hidden alias | repeatable, policy override |
| `--exclude` | **canonical** | hidden alias | repeatable, policy override |
| `--profile` | **canonical** | hidden alias | Cargo target selector |
| `--package` | **canonical** | hidden alias | Cargo package selector |
| `--cargo-preview` | **canonical** | hidden alias | mode, not an override — see below |
| `--dry-run` / `--dryrun` | already global | already present | unchanged |
| `--yes` | — | hidden alias | carries no intent (Execute is the default) |
| ROOT positional | **canonical** | hidden alias | bounded root — moved to the root by the owner decision, 2026-10-21 |
| `--known` | — | accepted | inert for cleanup scope, and it stays on `clean`: it resolves to the bare invocation (`src/cli.rs:383-389` against `src/cli.rs:332-338`), so hiding `clean` orphans nothing by it |

**`--full` is root-level and non-global.** It must not be `global = true`:
`scan` already owns a `full` argument id (`src/cli.rs:155-156`) and `clean`
owns one (`src/cli.rs:130-131`), and a blanket global collides with both —
the same collision `architecture/02-cli.md:92-96` records for `--dry-run`
versus `update --dry-run`. Non-global root args and identically-named
subcommand args coexist, exactly as `dry_run` already does
(`src/cli.rs:70-71`).

**`--cargo-preview` moves too.** It is a mode rather than a policy override,
but ADR 008 §4b's rationale is that the root is the *single cleanup front
door*: a front door that cannot express every mode is not a front door. The
root already carries `--dry-run` as a global (`src/cli.rs:64-71`); Cargo
preview is the one mode the root cannot express today.

**Scope resolution is unchanged.** `Cli::invocation()` resolves
`root.full || clean.full` to `CleanupScope::Full` at the existing arm
(`src/cli.rs:383-389`), and the policy overrides merge the same way: a value
given at the root wins, a value given on `clean` is used when the root did
not supply one. **The same override given at both levels is a usage error**
(exit 2) — it is ambiguous, and silently preferring one of two values is the
discarded-flag failure mode ADR 008 §3 exists to prevent.

### Usage errors, not discarded flags

`cargo cleanme --full scan --all`, `cargo cleanme --full config path`, and
`cargo cleanme --full update` are **usage errors (exit 2)**, not no-ops.
So is any policy override handed to a read-only or local-only subcommand.

The home is the existing `Invocation::UsageError` arm
(`src/cli.rs:333-341`), whose own doc comment already explains that
"defaulting to Full would silently run the machine-wide reconciliation the
flag exists to gate". It maps to `AppError::Config` (`src/main.rs:97`), so
the exit code is 2 — the same as any other usage error.

This is deliberately **not** the `--dry-run` precedent: `scan` and `config`
ignore a global `--dry-run` because for those operations a simulation *is*
the same read-only behaviour (`src/cli.rs:64-71`). Full cleanup is not the
same behaviour as a Full scan, and `--profile` / `--package` select Cargo
targets — they are meaningless for a read-only scan.

### The published spelling survives

`clean ROOT`, `clean --known` and `clean --full` keep working, unannounced.
`clean` becomes a **hidden** subcommand: accepted, invisible in
`cargo cleanme --help`.

## Engine

- `Cli` (`src/cli.rs:35-86`) gains `--full` and the six policy overrides, all
  **non-global**, mirroring the existing `--dry-run` structure.
- `Command::Clean` (`src/cli.rs:122-165`) keeps every flag it has today and
  becomes hidden; its flags become hidden aliases. The `CleanupOverrides`
  struct (`src/cli.rs:281-288`) is reused rather than duplicated — the root
  and `clean` must produce the same value type, or the two spellings can
  drift.
- `Cli::invocation()` (`src/cli.rs:339-417`) resolves the merged flags and
  raises the usage errors. It stays pure: no IO, no filesystem access, and
  every resolution unit-testable.
- The `CleanupRequest`, `CleanupScope`, `CleanupSelection` and
  `CleanupOverrides` types are **unchanged**. The spelling changes; the
  resolved intent does not. This is what makes the change safe to review:
  the diff in the cleanup engine is zero.

### The root `long_about`

The root `long_about` (`src/cli.rs:21-33`) must describe the canonical Full
cleanup spelling. The root man page renders from that same `long_about`
(`src/main.rs:156-164`), so `--help` and `man cargo-cleanme` cannot drift —
the property C040 established when it rewrote the same text.

The sentence "`cargo cleanme scan` needs a scope: `--all` is the full
read-only machine reconciliation…" stays true and stays.

### Generated artifacts

`generate-docs` must be re-run and the drift gate must be green. Two things
need checking rather than assuming:

- The generator renders every subcommand returned by `get_subcommands()`
  (`xtask/src/main.rs:183`), and clap includes **hidden** subcommands in that
  list — so hiding `clean` does not automatically remove
  `man/cargo-cleanme-clean.1`. Whether that page keeps shipping (a
  compatibility alias with its own page) is a decision this milestone must
  make explicitly and record; a stale page that documents a command the root
  no longer lists is drift.
- The five completions change, because the root's flag set changed. Whether
  the completion for the hidden `clean` subcommand changes with it is a fact
  to observe from the generated output, not to predict.

Whatever the outcome: **no stale artifact may ship**, and
`docs/INSTALLING.md`'s man-page count must be re-derived from the tree.

## The ROOT positional — owner decision, 2026-10-21: **option A**

ADR 008 §4b decides that `clean` becomes hidden. It did not decide what
happens to the **ROOT positional**, which is how a bounded root is selected.

Hiding `clean` without moving ROOT to the root would remove the documented
spelling for a published, non-deprecated operation
(`cargo cleanme clean ~/projects`): the bounded-root scope would become
reachable only through an undocumented alias. That is a usability regression
ADR 008 does not ask for, and it is the one place where ADR 008's *rationale*
("the root is the single cleanup front door") implies more than its literal
text.

Two coherent resolutions were put to the owner:

- **A (recommended) — move ROOT to the root.** `cargo cleanme ~/projects`
  becomes the canonical bounded-root spelling; `cargo cleanme clean ~/projects`
  remains accepted and hidden. The root is then a complete front door for all
  three scopes and both modes. It requires a root positional alongside
  subcommands, which needs clap's positional/subcommand conflict handling to
  be explicit, and it turns today's `cargo cleanme .` (a usage error) into a
  bounded cleanup of `.` — a behaviour change for an argv shape that was
  previously rejected.
- **B — keep `clean` visible.** The root gains `--full` and the policy
  overrides, `clean` stays documented for the bounded-root and `--known`
  spellings, and ADR 008 §4b's "hidden" is narrowed to the *flags* rather
  than the subcommand.

**The owner chose A on 2026-10-21.** The decision is recorded in ADR 003 §4
and ADR 008 as dated amendment blocks; this plan implements it.

### What the decision settles

- **The canonical bounded-root spelling is `cargo cleanme ROOT`.** The
  compatibility spelling `cargo cleanme clean ROOT` is accepted and hidden —
  published since 0.2.8, so hidden, never deleted.
- **`cargo cleanme .` changes from a usage error (exit 2) to a bounded cleanup
  of `.`.** This is a deliberate behaviour change to an argv shape that was
  previously rejected, and it is the one user-visible change the decision
  makes that is not merely a relocation of a spelling.
- **`clean --known` does not move.** It resolves to the bare invocation
  (`src/cli.rs:383-389` against `src/cli.rs:332-338`), and the bare invocation
  is ADR 003's canonical maintenance operation, so hiding `clean` orphans
  nothing by it. Adding a root-level `--known` would be a second spelling for
  an operation the root already performs.
- **The root now carries a positional alongside subcommands.** The
  subcommand-versus-positional precedence must be **explicit**, not a default:
  `cargo cleanme scan` must still parse as the subcommand, and
  `cargo cleanme ~/projects` as the ROOT positional. The one edge this creates
  is a directory whose name collides with a subcommand name (`scan`, `clean`,
  `config`, `update`); the resolution must be stated in `--help` and the
  behaviour documented, not left to whichever way the parser happens to fall.

### What the decision does not change

The resolved intent is untouched. The scope, the selection, the policy
evaluation, and the engine are unchanged; `CleanupRequest`, `CleanupScope`,
`CleanupSelection` and `CleanupOverrides` are unchanged. Only the command
that *carries* the intent moves. The `--full` spelling, the policy overrides,
the usage-error rule for read-only subcommands, and the M018 sequencing all
stand exactly as ADR 008 §4b and ADR 008 §5 set them.

## Tests

Every test names the input that makes it fail.

1. **The canonical spelling resolves to the Full scope.**
   `cargo cleanme --full` resolves to `CleanupScope::Full` — the *same*
   value `cargo cleanme clean --full` resolves to. Assert the resolved
   `Invocation`, not the exit code. *Fails if:* the root flag is parsed but
   not bound.
2. **The compatibility alias still resolves.** `cargo cleanme clean --full`
   resolves to the same value, and `clean` is absent from
   `cargo cleanme --help`. *Fails if:* the alias is dropped or the hidden
   subcommand reappears in the help text.
3. **The policy overrides work at the root.** `cargo cleanme --older-than
   3600` produces the same `CleanupOverrides` as `cargo cleanme clean
   --older-than 3600`. *Fails if:* the root value is parsed but discarded.
4. **A duplicate override is a usage error.** `cargo cleanme --older-than 3600
   clean --older-than 60` exits **2** with a usage message, and cleans
   nothing. *Fails if:* one of the two values is silently preferred.
5. **A scope selector on a read-only subcommand is a usage error.**
   `cargo cleanme --full scan --all` and `cargo cleanme --full config path`
   and `cargo cleanme --full update` each exit **2**. *Fails if:* the flag is
   accepted and discarded — the C034/"worst failure mode" shape.
6. **A policy override on a read-only subcommand is a usage error.**
   `cargo cleanme --profile release scan --all` exits **2**. *Fails if:* the
   override is ignored, as `--dry-run` is on `scan`.
7. **The scope resolution is unchanged.** The existing `invocation()` unit
   tests keep passing unchanged — the same `CleanupScope`, the same
   `CleanupSelection`, the same `CleanupMode`. *Fails if:* the spelling
   change altered the intent.
8. **The one-document invariant survives.** A root-level `--full` run emits
   exactly one JSON document on stdout, and the intervening Full scan report
   is still suppressed (`EmitReport::No`, `src/main.rs:771`).
9. **The generated artifacts are current.** `generate-docs -- --check` is
   green, and no artifact in `man/` or `completions/` is stale.
10. **The root help describes the Full cleanup.** `cargo cleanme --help`
    names the canonical Full cleanup spelling.
11. **The root positional resolves to the bounded-root scope.**
    `cargo cleanme ~/projects` resolves to `CleanupScope::Root("~/projects")`
    — the *same* value `cargo cleanme clean ~/projects` resolves to. Assert
    the resolved `Invocation`, not the exit code. *Fails if:* the root
    positional is parsed but not bound, or is bound to the maintenance scope.
12. **A subcommand still wins over a positional of the same shape.**
    `cargo cleanme scan --all` resolves to the scan invocation, not to a ROOT
    positional named `scan`. *Fails if:* the positional/subcommand precedence
    is left to a default that resolves the other way — the case that would
    make `cargo cleanme scan` silently become a bounded cleanup of a directory
    called `scan`.
13. **The deliberate behaviour change is pinned.** `cargo cleanme .` no longer
    exits **2** with *"unrecognized subcommand"*: it resolves to
    `CleanupScope::Root(".")`. *Fails if:* the root positional is added but the
    old argv shape is still rejected, or if the new shape is accepted but
    resolves to the wrong scope.
14. **The compatibility alias for the bounded root still resolves.**
    `cargo cleanme clean ~/projects` resolves to the same value as test 11,
    and `clean` is absent from `cargo cleanme --help`. *Fails if:* the alias is
    dropped or the hidden subcommand reappears in the help text.
15. **`clean --known` is not orphaned by the hiding.** `cargo cleanme clean
    --known` still resolves to `CleanupScope::Maintenance` with
    `CleanupSelection::WorkingDirectory` — the same value as the bare
    invocation. *Fails if:* hiding `clean` silently changes what the alias
    resolves to, or if the behaviour becomes unreachable.

## Documentation updates

- `src/cli.rs`'s root `long_about` and the `Clean` subcommand's doc comment.
- `docs/USAGE.md` — the canonical spellings; `clean --full` becomes the
  documented alias, and the Full cleanup section is reconciled with M018's
  new behaviour.
- `README.md`, `docs/AUTOMATION.md`, `docs/TROUBLESHOOTING.md` — every
  transcript and example that spells the operation.
- `docs/INSTALLING.md` — the man-page count, re-derived.
- `architecture/02-cli.md`, `04`, `06`, `09`, `10`, `13`, `16` — the CLI
  surface deep dives, with citations re-derived from the new source rather
  than shifted arithmetically.
- `.skills/cli-surface-and-argv/SKILL.md` — the "the `--full` spelling
  question is open" section and the "policy overrides live only on `clean`"
  section become descriptions of the shipped surface.
- `AGENTS.md` §8 — the ADR 008 bullet changes from an open decision to an
  implemented one.

## Planning review (`plans/003-planning-process.md` §12)

Confirmed on 2026-10-21, when the plan moved from `proposed` to `ready`:

1. **Durable behavior is unambiguous.** ADR 008 §§1–4, option 4b, plus the
   owner's ROOT-positional decision of 2026-10-21 (option A) fix the whole
   surface: every spelling, its canonical form, its alias, and the
   usage-error rule.
2. **Destructive behavior is not introduced ahead of its dependency.** The
   change moves *spellings*. The scope, the selection, the proof and the
   engine are untouched, and the release is gated on M018 shipping first.
3. **Scope/filter precedence is explicit.** `ROOT` wins over `--full`, as
   today (`src/cli.rs:383-389`); the same override given at both levels is a
   usage error, not a silent preference.
4. **Filesystem failure semantics are explicit.** Unchanged — the plan
   changes no discovery, no traversal and no cleanup.
5. **Concurrency/parallelism does not weaken correctness.** Unchanged.
6. **Cross-platform behavior is testable.** The change is argv parsing only;
   the hosted matrix still gates it.
7. **Configuration compatibility is defined.** No configuration key changes.
8. **Verification is specific.** Fifteen tests, each naming the input that
   makes it fail, plus the ordinary ladder.
9. **Closure evidence is measurable.** The closure-evidence requirements below
   name the live runs, the artifact inventory, the scope-resolution
   equivalence and the ladder's quoted results.
10. **Stop conditions prevent silent scope expansion.** The stop conditions
    below cover the ambiguous-argv-shape case, the resolved-intent case, the
    policy-override case and the artifact case.

## Verification commands

The ordinary ladder, in order, stopping at the first failure:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

The last step **must fail before the artifacts are regenerated** — that is
the drift gate proving it detects this milestone's change. Regenerate with
`cargo run --quiet --features dev-tools --bin generate-docs`.

MSRV (`cargo +1.89 check --locked --all-targets`) and the hosted matrix are
not covered by the ladder.

## Acceptance criteria

- ADR 008 evidence item **4** (the artifacts regenerated, the drift gate
  green) and item **5** (`--help` describes the canonical Full cleanup
  spelling, and the stale `scan`-with-no-ROOT sentence is gone — it was
  already retired by C040) are met.
- The fifteen tests above exist, pass, and each names its failing input.
- The scope resolution is provably unchanged: the existing `invocation()`
  tests pass without modification, and `CleanupRequest`, `CleanupScope`,
  `CleanupSelection` and `CleanupOverrides` are untouched.
- The usage-error surface is complete: every read-only and local-only
  subcommand rejects `--full` and every policy override with exit 2.
- No published spelling is removed; `clean` still accepts everything it
  accepts today.
- **The ROOT positional is implemented at the root**, with the
  subcommand-versus-positional precedence made explicit and the
  subcommand-name collision documented in `--help`.
- The documentation set above is reconciled, and the man-page count is
  re-derived from the tree.
- The ladder is green.

## Stop conditions

Stop and write a corrective rather than continuing if:

- the ROOT positional cannot be made to coexist with the subcommands without
  an ambiguous argv shape — the decision is made, but if the precedence cannot
  be made explicit then the spelling must not ship until it can;
- the change cannot be made without touching the resolved intent (the scope,
  the selection, the policy evaluation, or the engine);
- a policy override turns out to be needed by a read-only subcommand, which
  would mean the rejection rule is wrong rather than the implementation;
- the generated artifact set cannot be made consistent, or a stale artifact
  would ship;
- any test would have to assert an unrelated failure to pass.

## Closure evidence requirements

A closure record is written under `plans/closure/artifact-discovery-cleanup/`,
and it must carry:

1. the **live** post-implementation runs: `cargo cleanme --full --dry-run`
   and `cargo cleanme --full` (on a non-root machine), plus the usage-error
   cases, with their exit codes;
2. the **artifact inventory** after regeneration — which files exist, which
   were removed, and the man-page count, re-derived;
3. the **scope-resolution equivalence** evidence: the root and `clean`
   spellings resolving to the same `Invocation`;
4. the ladder's results, quoted.

Publication is a separate decision, and it is **gated on M018 having
shipped**. A corrected milestone that is fixed but not yet carried by a
published release stays **open**.

## Related open items this milestone does not close

- **C041** — the update path's candidate-spawn `ETXTBSY` flake.
- The hidden-cursor finding (overview §10 finding 36), the absent pty
  harness, and finding 59's load-sensitive assertions.
- **ADR 008 §5c** — rejected at acceptance; not revived here.
