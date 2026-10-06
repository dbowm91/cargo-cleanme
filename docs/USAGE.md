# Using cargo-cleanme

Full command reference and behavior. The [README](../README.md) has the
quickstart; this document has the detail.

## Contents

- [Commands](#commands)
- [Scope: Routine vs Full vs explicit](#scope-routine-vs-full-vs-explicit)
- [Discovery model](#discovery-model)
- [Cleanup safety model](#cleanup-safety-model)
- [Exit codes](#exit-codes)
- [Configuration](#configuration)
- [JSON output](#json-output)
- [Cleanup selectors](#cleanup-selectors)
- [Learned state](#learned-state)
- [Progress UI](#progress-ui)

## Commands

| Command | Effect |
|---|---|
| *(none)* | **Routine cleanup** over the maintenance scope |
| `--dry-run` | Routine cleanup simulated end to end, zero `cargo clean` processes |
| `cargo cleanme scan [ROOT]` | Full read-only reconciliation (no ROOT), or one explicit scope |
| `cargo cleanme scan --known` | Routine read-only inventory of the maintenance scope |
| `cargo cleanme clean [ROOT] [--known\|--full]` | advanced cleanup: execute (default), simulate, or Cargo preview |
| `cargo cleanme config path\|show\|edit` | locate, print, or edit the config |
| `cargo cleanme update [--dry-run]` | self-update to the latest stable release |

`cargo cleanme …` and `cargo cleanme-cleanme …` are the same binary and behave
identically. `scan` with no `ROOT` and `clean --full` take **no** `ROOT`; the
scan form walks the platform roots, and the cleanup form reconciles them first.

### The maintenance scope

Bare invocation, `clean --known`, and `scan --known` all resolve the scope the
same way:

1. a configured legacy `scan.root`, if set, is an **exclusive explicit
   override** — nothing else is searched and cleanup targets exactly that root;
2. otherwise the **Routine scope**: bounded seed roots under your home plus
   active learned roots from prior Full reconciliations.

Rootless `scan` is **always** Full and deliberately ignores `scan.root`, so a
configured root can never silently narrow the canonical reconciliation command.
`scan ROOT` remains the explicit per-invocation override, and any explicit
effective scope bypasses the configured `ignore`/`unignore` filters.

Learned roots are **search hints only**. Complete manifest coverage and fresh
ownership proof are rebuilt before anything is cleaned.

### scan

```sh
cargo cleanme scan                 # Full: exhaustive platform-root walk
cargo cleanme scan ~/projects      # one explicit scope
cargo cleanme scan --known         # Routine read-only inventory
cargo cleanme scan --known --format json
cargo cleanme scan ~/p --no-progress --stats
```

`scan --full` remains accepted as a hidden alias for no-root `scan`.
`ROOT` with `--known` or `--full`, and `--known` with `--full`, are usage errors.

Output is deterministic: size-descending, with a stable path tie-break, and a
deduplicated inventory total. It is an **estimate of reclaimable bytes**, never
a claim about bytes already recovered.

A group appears only if it is *inactive*. Anything with source activity inside
the recency window (`recency_seconds`, default 300) is protected and reported
as skipped. `--stats` shows this as `active_skipped=N`.

### clean

```sh
cargo cleanme                                     # Routine cleanup (Execute)
cargo cleanme --dry-run                           # Routine cleanup, zero cargo clean
cargo cleanme clean ~/projects                    # explicit root (Execute)
cargo cleanme clean ~/projects --dry-run          # simulation, no cargo clean
cargo cleanme clean ~/projects --cargo-preview    # Cargo's own dry-run, after the same proof
cargo cleanme clean --known --dry-run             # maintenance scope only
cargo cleanme clean --full --dry-run              # reconcile Full, then simulate
cargo cleanme clean ~/projects --min-reclaimable-bytes 104857600
cargo cleanme clean --known --older-than 2592000 --exclude '**/archived'
```

`--dry-run`, `--cargo-preview`, and `--yes` conflict pairwise: they select
three different modes and never silently precedence-resolve.

`clean` with no scope selector resolves the maintenance scope, exactly as bare
invocation does — there is one cleanup code path, not two.

### Migration from 0.1.x

| 0.1.x | Now | Same thing? |
|---|---|---|
| `cargo cleanme` (read-only Routine scan) | `cargo cleanme scan --known` | yes |
| `cargo cleanme scan --full` | `cargo cleanme scan` | yes |
| `cargo cleanme clean R --dry-run` (Cargo preview) | `cargo cleanme clean R --cargo-preview` | yes |
| `cargo cleanme clean R --dryrun` (simulation) | `cargo cleanme clean R --dry-run` | yes |
| `cargo cleanme clean R --yes` (execute) | `cargo cleanme clean R` | yes |

`--dryrun` and `--yes` remain accepted on `clean` as **hidden** aliases for the
pre-1.0 migration period. They select the same canonical modes as `--dry-run`
and the default respectively, print nothing, and create no fourth mode.

### config

```sh
cargo cleanme config path    # prints the resolved path only
cargo cleanme config show    # effective settings
cargo cleanme config edit    # $VISUAL, then $EDITOR, then a common editor
```

The config is created on first use and never overwritten. The repository ships a
canonical `config.toml` template as a distribution artifact; it is never loaded
implicitly from the working directory. `config edit` validates the result after
the editor exits and rejects an invalid file.

## Scope: Routine vs Full vs explicit

| Scope | How to ask | What it walks |
|---|---|---|
| explicit | `clean ROOT` / `scan ROOT` | that directory, recursively |
| Routine | `scan` with no ROOT | seed roots under Projects/projects, Developer, dev, Code/code, src, repos/Repos, GitHub/github, workspace/workspaces, **plus** active learned roots |
| Full | `--full` | the full platform roots, exhaustively |

Explicit CLI roots take precedence over `scan.root`. Either explicit scope
bypasses the user ignore/unignore search filters. Routine and Full honor them.

A never-learned unusual location stays undiscovered until `scan` (Full) or an
explicit scan visits it.

## Discovery model

The scanner is **manifest-first**: it finds user `Cargo.toml` files without
requiring a sibling `target/`, then asks Cargo itself to resolve the facts.

Cargo is invoked with `locate-project --workspace` and
`metadata --offline --locked --no-deps` — never for ignored or pruned paths.
After the first successful metadata resolution for a workspace, every member
manifest that authoritative metadata returned is registered, so later manifests
from the same workspace need no further processes. An N-member workspace costs
exactly one locate and one metadata. Membership is never guessed from path
ancestry or TOML.

What that buys:

- **Output roots are never inferred.** `target-dir` / `build-dir` configuration
  is read from Cargo, not assumed.
- **Equal and nested physical output is grouped and measured once**, so
  `target == build`, or a nested `target`/`build`, is never double-counted.
- **Symlinks are never traversed**, and a symlinked scan root is refused rather
  than silently scanning nothing.

Two independent bounded walks cover the filesystem — one for discovery, one for
measurement — so a root can be discoverable yet unmeasurable, and the report
distinguishes those cases instead of hiding one behind the other.

Cheap gates (existence, artifact presence, source activity) run before deep
sizing. Unix and Windows report **allocated** bytes; other platforms report
apparent bytes.

### Ownership classes

| Class | Cleanable | Meaning |
|---|---|---|
| `private` | yes | this workspace exclusively owns these bytes |
| `shared` | **no** | another known workspace shares them |
| `external-unproven` | **no** | a participant exists that cannot be attributed |
| `uncertain` | **no** | ownership could not be decided |

Only `private` is ever a cleanup candidate. The other three are inventory-only
everywhere — in the scan report and in the cleanup decision alike.

## Cleanup safety model

Deletion is **always** `cargo clean`. cargo-cleanme never removes directories
directly, so Cargo's own bookkeeping cannot be bypassed.

The destructive unit is **one workspace `CleanupUnit` over its complete
`OutputSet`** — exactly one Cargo workspace cleanup invocation, containing one
resolved workspace, its complete resolved `target` and `build` roots, and every
physical group that one invocation can affect. Multi-member workspaces are
supported. One invocation produces one result and one pre/post measurement.

A unit is authorized only when **every** group it can affect is `private`,
authorized, inactive, a real non-symlink directory, and marker-qualified. A
private `target` can therefore never carry a `shared`, `external-unproven`,
`uncertain`, or unauthorized sibling `build` directory into the same
invocation. Such a unit is **skipped whole**, and the skip reason names the
failing output kind, path, and class.

Ownership is re-proven **immediately before each spawn**, because a workspace
can change between scan and delete. Nothing is ever deleted on the strength of a
timestamp.

### Unresolved ownership blocks the whole scope

Before any output analysis or cleanup qualification, every `Cargo.toml` under
`clean ROOT` must be accounted for by either a direct successful resolution or
successful workspace metadata listing it as a member or root. An earlier failed
member attempt is cleared only if later authoritative metadata covers it.

Any remaining unresolved manifest **blocks the entire cleanup scope** in
Preview, Simulate, and Execute, reports `unresolved_ownership` under `--stats`,
and runs **no** `cargo clean` command — including Cargo's own preview dry-run.
Read-only scans stay partial-result tolerant; that asymmetry is deliberate.

### Mode semantics

| Mode | Flag | Machine `mode` | What runs |
|---|---|---|---|
| Execute | *(default)*; `--yes` is a hidden alias | `execute` | everything, then `cargo clean` |
| Simulate | `--dry-run`; `--dryrun` is a hidden alias | `simulate` | everything, then **nothing** — no `cargo clean` at all |
| Cargo preview | `--cargo-preview` | `preview` | everything, then `cargo clean --dry-run --verbose` |

**dry run means simulation.** One conventional spelling, one meaning: nothing is
mutated and nothing is spawned that could mutate.

Simulation is not a weaker mode. It shares the same completeness and final-proof
gates as Execute, so use it when testing a change to those gates. Cargo preview
is the lower-level debugging view: it hands the decision to Cargo itself, so its
`mode` stays `preview` and it never reports reclaimed bytes.

### Policy filters

`--min-reclaimable-bytes` filters on the freshly measured, complete workspace
output union. `--older-than` is an *additional* quiet-age requirement and never
replaces the recency safety guard. CLI policy values replace configured values
of the same kind. Excluded workspaces remain in the ownership proof and can
still make outputs `shared` — excluding a workspace does not make its output
private.

`include` and `exclude` are `globset` patterns matched against the **canonical
workspace root** — one path per workspace, such as
`/home/you/projects/demo-app`. They are *not* matched against output directories
or anything beneath the root, which has one consequence worth knowing:

```sh
--exclude '**/archived/**'   # does NOT match a workspace root that IS .../archived
--exclude '**/archived'      # does match
--exclude '*archived*'       # does match
```

A pattern with a trailing `/**` requires a path segment *after* the name, so it
never matches the directory itself. Exclusion wins over inclusion, and a
malformed glob is a configuration error (exit `2`), not a silent no-match.

### Patterns are matched against canonical paths, and canonicalized first

A pattern is matched against the **canonical** path, so a pattern spelled the
way your shell shows it — through a symlink, or with an 8.3 short name on
Windows — would silently match nothing. Each pattern's leading literal run is
therefore resolved to its canonical spelling when the config is loaded, and
the glob tail after it is kept exactly as you wrote it.

The rule that follows from that is the one worth knowing, because it is the
opposite of what a path normally does:

> **Directory names in a pattern are matched literally, not as globs.**

A directory really named `real[abc]` is matched by `.../real[abc]/*`, not by a
character class. You do **not** escape the brackets yourself — the canonical
spelling is escaped for you, and a pattern you write against the directory's
real name matches it. The rule is the same on Windows, where `[` and `]` are
legal file-name characters, and it is enforced by escaping at load time rather
than by asking you to spell patterns differently.

Two consequences that are genuinely yours:

| Situation | What happens |
|---|---|
| The canonical directory name contains `{` or `}` | the rewrite is **skipped** for that pattern and it keeps your spelling — so it may not match the canonical path |
| The pattern is relative, or names a path that does not exist | kept as written, and matches nothing (exit `2` only if it is not a valid glob) |

`globset` compiles a `\` in a Windows pattern to `/` and compares against a
`/`-normalized candidate, so `C:\dev\proj\*` and `C:/dev/proj/*` are the same
pattern. Matching is case-sensitive, and `*` crosses path separators — both are
`globset` defaults that this tool does not override.

Profile and package selectors are the exception: they have no trustworthy
selector-specific byte estimate, so a nonzero `--min-reclaimable-bytes` with any
selector **fails closed** before Cargo mutation. See
[Cleanup selectors](#cleanup-selectors).

`cleanup` progress totals and report rows count **cleanup units**, not physical
groups. The read-only scan report stays physical-group oriented.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | the requested operation completed, including safe per-unit skips |
| `1` | scope incomplete, ownership unproven, or one or more operations failed |
| `2` | invocation or configuration error, before a report could be formed |

`2` covers every `AppError` — an invalid or unreadable config, an invalid scan
root, an I/O error, a scan failure, a self-update refusal, and any Clap usage
error.

`config edit` is the one exception: it **propagates the editor's own exit
status**, so its code is whatever the editor chose. A nonzero editor exit is
reported on stderr and the config is *not* reverted.

`1` is overloaded across commands on purpose, and `clean` reports it **with** a
well-formed JSON document. When `scope_blocked` is `true`, that is a
success-shaped result carrying a typed blocker — read the block rather than
inferring from the status.

A failure to persist learned state does **not** affect the exit code. State is
an optimization only.

## Configuration

```toml
[scan]
recency_seconds = 300
# root = "/path/to/projects"      # optional exclusive scope; a CLI root wins
ignore = []
unignore = []
learned_root_retention_days = 30

[cleanup]
allowed_output_roots = []

[cleanup.policy]
min_reclaimable_bytes = 0
# min_inactive_seconds = 2592000
include = []
exclude = []
```

| Key | Effect |
|---|---|
| `recency_seconds` | activity guard; source touched within this window is protected |
| `root` | an exclusive scope. A CLI root takes precedence over it |
| `ignore` | **absolute** glob patterns pruned from discovery |
| `unignore` | **absolute** paths re-included beneath an ignored tree |
| `learned_root_retention_days` | how long a learned root is retained |
| `allowed_output_roots` | authorizes a location for cleanup; never grants ownership |
| `min_reclaimable_bytes` | floor for a unit to be considered |
| `min_inactive_seconds` | quiet-age floor |
| `include` / `exclude` | globs on canonical workspace roots; exclusion wins |

A relative `ignore`, `unignore`, `include`, `exclude`, `root`, or
`allowed_output_roots` entry is rejected: `configuration error: ignore pattern
must be absolute: <value>`, exit `2`.

### The filters are bypassed by an explicit root

`ignore` and `unignore` apply to **global discovery and the Routine scope** only.
An explicit scan root — a CLI `ROOT` argument *or* this file's `root` — bypasses
them entirely, because a caller who names one directory means it.

```sh
cargo cleanme scan .                                  # filters do NOT apply
cargo cleanme scan --config ./filters.toml            # Routine: filters apply
```

This surprises people who test an `ignore` rule with `scan .` and see no effect.

### An `ignore` pattern and an `unignore` exception

A literal directory in `ignore` prunes the whole subtree beneath it. An
`unignore` entry re-includes **that one path and its subtree**, and nothing
else: the walk comes back in far enough to reach the exception, and every other
directory under the ignored ancestor stays excluded.

```toml
ignore   = ["/home/you/projects/archived"]
unignore = ["/home/you/projects/archived/keepme"]
```

With `archived/keepme` and `archived/other` both present, this discovers **1**
manifest — `keepme` — plus any projects nested beneath it.

This is the intended meaning of the pair, and it is what `ignore` has always
been documented to mean. **0.1.6 and earlier did not do this**: the walk
forgot that a sibling was still under the ignored directory, so a literal
`ignore` plus any `unignore` beneath it re-admitted every sibling, and the
above discovered **2** manifests. Appending `/**` (or `/*`) to the ignore
pattern worked around it and is still accepted:

```toml
# Also correct, on every release including 0.1.6
ignore   = ["/home/you/projects/archived/**"]
unignore = ["/home/you/projects/archived/keepme"]
```

Use the plain literal form above. If a scan on 0.1.6 or earlier returns more
projects than you expect from an ignored region, that is a known defect fixed
after 0.1.6 — upgrade rather than tuning the pattern. See
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#unignore-re-admitted-siblings-fixed-after-0-1-6)
for the historical detail, and
[An `ignore` rule seems to do nothing](TROUBLESHOOTING.md#an-ignore-rule-seems-to-do-nothing)
for the explicit-root case, which is a different thing entirely.

`unignore` is a literal path, not a glob. A `*` in it is not expanded, so
`archived/keep*` names a literal directory that does not exist and re-includes
nothing. Glob patterns belong in `ignore`, `include`, and `exclude`.

`allowed_output_roots` entries must be absolute and non-symlink. They authorize;
they cannot manufacture ownership proof. A group that is `shared` or
`external-unproven` stays inventory-only no matter what is configured.

## JSON output

`--format json` emits **exactly one** newline-terminated schema-version-1
document on stdout. Not NDJSON, not pretty-printed, not followed by a summary
line. Field-level detail is normative in
[`plans/output-schema-v1.md`](../plans/output-schema-v1.md).

- Diagnostics go to **stderr**, never into the document.
- `--stats` goes to **stderr**. stdout is byte-identical with and without it.
- Progress is disabled in JSON mode; `TERM=dumb` and non-TTY output are plain.
- `scope` is an **open string**, not a closed enum. It is derived from the
  resolved policy, so a configured `scan.root` reports `explicit`. Treat it as
  open and do not add a closed-enum assumption downstream.
- Absent means "does not apply"; `0` asserts a measured value. Do not fill a gap
  with a zero.
- `bytes` are platform-independent integers. Display may abbreviate; the JSON
  value never carries a unit.

`update --format json` is the one exception to the envelope and is the only
untested output surface; see [docs/UPDATE.md](UPDATE.md).

### Output formats

| `--format` | Shape | Use it when |
|---|---|---|
| `human` (default) | tables, per-unit rows, diagnostic detail | a person is reading |
| `json` | exactly one schema-1 document on stdout | software needs complete, per-unit data |
| `log` | exactly one bounded ASCII line on stdout | a scheduler retains a short history tail |

**Parse JSON, not log.** Log is an operational summary: it drops per-unit detail
entirely, prints no paths, and omits optional fields from the tail to stay
inside a 384-byte bound. It is never selected automatically — a non-TTY stdout
still gets `human`.

```console
$ cargo cleanme --format log
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0
```

In log mode a successful run writes that one line to stdout and nothing to
stderr: progress is disabled regardless of TTY, and per-diagnostic fan-out is
replaced by the `diagnostics=` count. `--stats` stays an opt-in stderr
override. A failure raised *before* any report exists writes no stdout and one
bounded stderr line carrying a typed `reason=` code.

→ [docs/AUTOMATION.md](AUTOMATION.md) for the full field list, the scheduling
integration, and exit-code triage.

## Cleanup selectors

`clean --profile NAME` delegates profile selection to Cargo on exact qualified
runtime releases: 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0,
1.98.1, and 1.99.0.

`clean --package SPEC` is enabled only on Cargo **1.98.1 and 1.99.0** — the exact
releases where package cleanup passed the configured-target matrix. Any other or
unknown Cargo version gets a typed `selector_unsupported` result *before* any
Cargo clean.

### How a version gets added to those lists

The lists above are not hand-maintained prose, and they are not a bare allowlist
inside the binary. `release/selector-qualification.json` is the single
machine-readable authority, and four things are proved to agree with it by
`scripts/check-selector-qualification.py`:

- the runtime capability table in `src/workspace.rs` — what the product actually
  does, and therefore what gets proved rather than what gets to define the truth;
- the policy's `profile_selector`, `package_selector`, and `exploratory` lists,
  held separately because package selection has stricter preconditions;
- the hosted qualification matrix in
  `.github/workflows/qualify-cargo-selectors.yml`;
- the real-Cargo assertions in `scripts/qualify-cargo-selectors.sh`.

**Adding a version is a plan, not an edit.** Putting a release in
`profile_selector` or `package_selector` requires an implementation plan or
closure record carrying exact real-Cargo evidence for that version, checked by
`rustup` against that pinned toolchain. A newly released Cargo version is **not**
supported because the exploratory lane happened to pass: an exploratory result is
a research signal, and both the static gate and a Rust test reject an exploratory
toolchain that reaches the runtime table.

To requalify the current matrix against real Cargo:

~~~sh
bash scripts/qualify-cargo-selectors.sh --evidence /tmp/qualification.json
~~~

Every claimed capability produces a named assertion with an explicit verdict, and
a claimed version that produced **no** assertion is a failure rather than a pass.
Three failure classes are deliberately kept apart: `semantic_regression` (a
product finding), `toolchain_unavailable` (an operational blocker — reported as
one, and never a reason to drop a version from the list), and `premise_failed`
(a fixture that did not build, which is not evidence about Cargo).

Selector-specific byte estimates are not trustworthy, so the JSON estimate is
`null` and complete output-union measurements are context only. A nonzero
`--min-reclaimable-bytes` with any selector therefore **fails closed** before
Cargo mutation.

Package specs are checked against Cargo-resolved workspace package identities,
and duplicate names are rejected before a clean process starts. Cargo may remove
shared dependency artifacts even for a `--package` selector.

Application support for a Cargo version is broader than selector qualification:
an unqualified release fails closed rather than guessing.

## Learned state

Discovery state is machine-local JSON and is normally self-managed.

- Routine scans fall back to seed/configured roots when state is missing or
  unusable.
- A successful rootless `scan` (Full) replaces corrupt or obsolete state **after**
  reconciliation completes. An incomplete Full scan leaves it untouched.
- State written by a **newer** schema is preserved, and that version must be
  upgraded before it can be reconciled.
- A persistence failure is reported on stderr and never changes the exit code.
- Deleting discovery state is **not** the normal recovery procedure.

A rootless `scan` (Full) on a typical workstation reports permission-denied diagnostics
for platform roots it cannot read. Those are expected, and they are why a
Full scan can be "incomplete" while still returning a usable report.

## Progress UI

Attended terminals show inline progress on **stderr** only: one coordinated
`MultiProgress` owns the main bar plus up to five candidate rows, so bars never
fight for terminal state.

- Discovery is indeterminate — there is no wasteful pre-scan.
- Analysis and cleanup become determinate once counts are known, via the
  `ProgressObserver::units_total` / `unit_completed` contract. Callers never
  assume the concrete renderer.
- Totals are re-scoped on phase changes, so analysis counts never leak into
  cleanup counts.
- Output is capped at 10 Hz, with no alternate screen and no raw mode.
- Progress clears before the final stdout report and never changes results.
- `--no-progress` forces hidden output; `TERM=dumb` and non-TTY are plain.

`--no-progress --stats` is the canonical benchmark and debug combination.
