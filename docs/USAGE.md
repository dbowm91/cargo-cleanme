# Using cargo-cleanme

Full command reference and behavior. The [README](../README.md) has the
quickstart; this document has the detail.

## Contents

- [Commands](#commands)
- [Flags](#flags)
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
| *(none)* | **Cleanup** of the maintenance scope: the machine's known roots plus the working directory (Execute) |
| `--dry-run` | Same selection simulated end to end, zero `cargo clean` processes |
| `-i` / `--interactive` | Terminal selection list over the resolved units, then clean |
| `cargo cleanme scan --all` | Full read-only reconciliation |
| `cargo cleanme scan ROOT` | One explicit read-only scope |
| `cargo cleanme scan --known` | Routine read-only inventory of the maintenance scope |
| `cargo cleanme clean [ROOT] [--known\|--full]` | advanced cleanup: execute (default), simulate, or Cargo preview, proven within the selected scope (unprovable manifests outside the scope are reported, never blocking) |
| `cargo cleanme config path\|show\|edit` | locate, print, or edit the config |
| `cargo cleanme update [--dry-run]` | self-update to the latest stable release |

`cargo cleanme …` and `cargo-cleanme …` are the same binary and behave
identically. `scan --all` and `clean --full` take **no** `ROOT`; the
scan form walks the platform roots, and the cleanup form reconciles them first.

## Flags

| Flag | Effect |
|---|---|
| `--format human\|json\|log` | interactive detail; one schema-1 JSON document; or one bounded ASCII line for an unattended scheduler |
| `--dry-run` | simulate: the whole decision path, zero `cargo clean` processes |
| `--no-progress` | disable the transient TTY UI (stderr only) |
| `--stats` | counters and phase timings **on stderr**; stdout stays byte-identical |
| `--config PATH` | use a specific `config.toml` |
| `--min-reclaimable-bytes N` | only clean units above this measured size (`clean`) |
| `--older-than SECONDS` | extra quiet-age requirement; never replaces the recency guard (`clean`) |
| `--include GLOB` | include canonical workspace roots matching this glob (`clean`) |
| `--exclude GLOB` | exclude canonical workspace roots matching this glob (`clean`) |
| `--profile NAME` | ask Cargo to clean one named profile (`clean`) |
| `--package SPEC` | clean one validated workspace package (`clean`) |
| `--cargo-preview` | after the same proof, run Cargo's own `clean --dry-run --verbose` (`clean`) |
| `--yes` | hidden alias for the default Execute mode (`clean`) |
| `--known` | Routine roots (`clean`, `scan`) |
| `--full` | Full reconciliation before cleanup (`clean`) |
| `-a` / `--all` | Full machine reconciliation (`scan`; `--full` is a hidden alias) |
| `-i` / `--interactive` | choose cleanup units from a terminal list (needs a TTY) |

`--dry-run`, `--cargo-preview`, and `--yes` conflict pairwise **on `clean`**, on
purpose: they select three different modes. A root-level `--dry-run` — the
`cargo cleanme --dry-run clean ROOT` spelling Cargo itself passes to an
external subcommand — is a **different flag** from the clean-level one and does
**not** conflict with `--cargo-preview`: the root flag wins and selects
Simulation, because simulation is the mode that spawns nothing.

`--dryrun` and `--yes` are hidden aliases that print nothing.

### The maintenance scope

Bare invocation, `clean --known`, and `scan --known` all resolve the scope the
same way:

1. a configured legacy `scan.root`, if set, is an **exclusive explicit
   override** — nothing else is searched and cleanup targets exactly that root;
2. otherwise the **Routine scope**: bounded seed roots under your home plus
   active learned roots from prior Full reconciliations, **plus the working
   directory** — the workspace that encloses it and the directory itself.

The working directory is part of the scope, so a bare run cleans what is under
where you are standing, wherever that is. The scope only ever *grows*: the
working directory is added to the roots, never substituted for them. A narrower
scope would hide the out-of-scope claimants of a shared target and promote
`shared` bytes to `private` — the only class that authorizes deletion — so
ownership is still proven across the whole resolved universe.

The working directory joins only when the Routine scope does not already reach
it. Standing inside a known root, or standing in a directory that contains
one, leaves the bounded machine-wide sweep bounded: sweeping *everything*
under your home would also sweep your Trash and your package caches, whose
abandoned manifests cannot be resolved and would block the run. To sweep a
whole directory on purpose, name it: `cargo cleanme clean ~`.

Bare invocation and `clean --known` then anchor the **selection** to the working
directory: a resolved unit is a candidate when its canonical workspace root is
the directory you are standing in, contains it, or is contained by it.
`clean ROOT` and `clean --full` skip this filter, because their roots already
bound the scope. The anchor narrows candidates only — ownership is proven across
the whole resolved universe first, so a target shared with a workspace outside
the anchor stays `shared` and stays inventory-only. Units the anchor excludes
are reported as typed `skipped_outside_selection` rows rather than dropped, so
"nothing was selected" never looks like "nothing was eligible". `scan --known`
is not anchored: it is the read-only inventory over the whole maintenance scope.

`scan --all` is **always** Full and deliberately ignores `scan.root`, so a
configured root can never silently narrow the canonical reconciliation command.
`scan ROOT` remains the explicit per-invocation override, and any explicit
effective scope bypasses the configured `ignore`/`unignore` filters.

Full means an exhaustive **attempt** across the platform roots available to
the current user, not elevated access. If traversal cannot read a subtree,
the scan keeps positive manifest results, reports bounded unknown-location
coverage loss, exits 1, and does not publish a complete learned-state
generation. Use `scan ROOT` when you need a bounded result for one accessible
tree.

Learned roots are **search hints only**. Complete manifest coverage and fresh
ownership proof are rebuilt before anything is cleaned.

New learned roots come only from successfully Cargo-resolved workspaces in
durable developer locations. Full inventory still includes unusual accessible
Cargo manifests, including package-manager content; cache, Trash, temporary,
`node_modules`, and transient Codex worktree paths are not automatically
selected for Routine maintenance. Explicit roots remain available and keep
their existing strict safety checks.

Standing in one of those locations is a different matter: the working
directory joins the maintenance scope whatever it is, because you chose it by
standing there. `clean ROOT` permits the same subtree and has always done so.

### scan

```sh
cargo cleanme scan --all              # Full: exhaustive platform-root walk
cargo cleanme scan ~/projects      # one explicit scope
cargo cleanme scan --known         # Routine read-only inventory
cargo cleanme scan --known --format json
cargo cleanme scan ~/p --no-progress --stats
```

`scan` requires exactly one scope selector — `ROOT`, `--known`, or `--all`
(`-a`) — and bare `scan` is a usage error, so the machine-wide reconciliation
is never one keystroke from the front door. `scan --full` remains accepted as a
hidden alias of `--all`.
`ROOT` with `--known` or `--all`, and `--known` with `--all`, are usage errors.

Output is deterministic: size-descending, with a stable path tie-break, and a
deduplicated inventory total. It is an **estimate of reclaimable bytes**, never
a claim about bytes already recovered.

A group appears only if it is *inactive*. Anything with source activity inside
the recency window (`recency_seconds`, default 300) is protected and reported
as skipped. `--stats` shows this as `active_skipped=N`.

### clean

```sh
cargo cleanme                                     # cleanup anchored at $PWD (Execute)
cargo cleanme --dry-run                           # same selection, zero cargo clean
cargo cleanme -i                                  # choose units interactively, then clean
cargo cleanme -i --dry-run                        # choose units, simulate, clean nothing
cargo cleanme clean ~/projects                    # explicit root (Execute)
cargo cleanme clean ~/projects --dry-run          # simulation, no cargo clean
cargo cleanme clean ~/projects --cargo-preview    # Cargo's own dry-run, after the same proof
cargo cleanme clean --known --dry-run             # $PWD-anchored selection, simulated
cargo cleanme clean --full --dry-run              # reconcile Full, then simulate
cargo cleanme clean ~/projects --min-reclaimable-bytes 104857600
cargo cleanme clean --known --older-than 2592000 --exclude '**/archived'
```

`clean --full` refuses with exit `2` when the Full reconciliation it depends
on is incomplete or stale — `full reconciliation reported an incomplete scan
(exit N); no cleanup commands were run against its unproven learned roots`, or
`full reconciliation produced no fresh generation; no cleanup commands were run
against stale learned roots`. A Full cleanup runs against the learned roots the
reconciliation just proved, so an unproven reconciliation refuses the cleanup
rather than using those roots.

`--dry-run`, `--cargo-preview`, and `--yes` conflict pairwise **on `clean`**:
they select three different modes. A root-level `--dry-run` — the
`cargo cleanme --dry-run clean ROOT` spelling Cargo itself passes to an
external subcommand — is a different flag from the clean-level one and does
**not** conflict with `--cargo-preview`: the root flag wins and selects
Simulation, because simulation is the mode that spawns nothing.

`clean` with no scope selector resolves the maintenance scope, exactly as bare
invocation does — there is one cleanup code path, not two. Both anchor the
selection to the working directory; only `clean ROOT` and `clean --full` clean
every resolved unit.

### Interactive selection

`-i` / `--interactive` resolves the cleanup units read-only, presents one row
per unit with its reclaimable bytes, and cleans only what is checked. A
selection can remove candidates but never add one: ownership is still proven
across the complete universe and the engine still re-proves it per spawn.

| Key | Mode | Effect |
|---|---|---|
| `↑`/`k`, `↓`/`j` | NOR | move the cursor |
| `PgUp`, `PgDn` | NOR | move by one page of visible rows |
| `Home`, `End` | NOR | move to the first / last candidate |
| `space` | NOR | toggle the row under the cursor |
| `i` | NOR | enter INS (filter) |
| `Esc` | INS | back to NOR, keeping the filter |
| `Esc` then `Esc` | NOR | exit |
| `q`, `Ctrl-C` | either | exit immediately |
| `Enter` | NOR | clean the selected units |

Row 0 is an **all** toggle that ignores the filter. Typing in INS filters live
without discarding the existing selection. `Enter` on an empty selection does
not commit. Combine with `--dry-run` to review a selection without invoking
`cargo clean`. The session refuses to start when stdin or stdout is not a
terminal, and it refuses machine formats (`--format json` / `--format log`),
because a selection list is an operator surface, not a report. An **empty
candidate list** is not a refusal: the scope resolved and proved no cleanup
unit, so the run prints one plain line, `Nothing to clean.`, and exits `0` —
the same exit the same scope cleaned *without* `-i` produces.

The list is a **bounded page**, not the whole screen: it shows at most fifteen
rows whatever the scope holds, and a row is named by the shortest part of its
path that tells it apart from every other candidate. The reclaimable size is
right-aligned so the big ones line up, and the ownership annotation is omitted
entirely when every candidate is private. The surface uses colour; set
`NO_COLOR` (or run with `TERM=dumb`) to turn it off.

### Migration from 0.1.x

The canonical table is
[README.md §Upgrading from 0.1.x](../README.md#upgrading-from-01x) — it is the
one this repository's release contract checks, and it is the only copy. The
five rows that matter:

| 0.1.x | Now | Same thing? |
|---|---|---|
| `cargo cleanme` (read-only Routine scan) | `cargo cleanme scan --known` | yes |
| `cargo cleanme scan --full` | `cargo cleanme scan --all` | yes |
| `cargo cleanme clean R --dry-run` (Cargo preview) | `cargo cleanme clean R --cargo-preview` | yes |
| `cargo cleanme clean R --dryrun` (simulation) | `cargo cleanme clean R --dry-run` | yes |
| `cargo cleanme clean R --yes` (execute) | `cargo cleanme clean R` | yes |

The one row whose *meaning* changed rather than its spelling: bare
`cargo cleanme` was a read-only Routine scan in 0.1.x and is Routine **cleanup**
now, and `--dry-run` was Cargo preview and is **simulation** now.

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
| Routine | `scan --known` | seed roots under Projects/projects, Developer, dev, Code/code, src, repos/Repos, GitHub/github, workspace/workspaces, **plus** active learned roots, **plus** the working directory when the Routine scope does not already reach it |
| Full | `scan --all` | the full platform roots, exhaustively |

Explicit CLI roots take precedence over `scan.root`. Either explicit scope
bypasses the user ignore/unignore search filters. Routine and Full honor them.

A never-learned unusual location stays undiscovered until `scan --all` (Full) or an
explicit scan visits it.

## Discovery model

The scanner is **manifest-first**: it finds user `Cargo.toml` files without
requiring a sibling `target/`, then asks Cargo itself to resolve the facts.

Cargo is invoked with `locate-project --workspace` and
`metadata --offline --locked --no-deps` — never for ignored or pruned paths.
Resolution runs in bounded-parallel bulk phases: every manifest is located
once, manifests group by located workspace root, and one metadata runs per
group. After the first successful metadata resolution for a workspace, every
member manifest that authoritative metadata returned is registered, so later
manifests from the same workspace need no further metadata processes. An
N-member workspace costs one locate per manifest and one metadata. Membership
is never guessed from path ancestry or TOML.

Full discovery can find manifests Cargo cannot resolve as workspaces, such as
an orphaned member or a broken manifest. Such a manifest stays in the
read-only inventory as unresolved; it does not become a learned maintenance
root. Cleanup remains blocked while any discovered manifest *inside the selected scope* is unresolved,
because its Cargo-configured output ownership is unknown. A manifest outside
the scope (for example, outside the working-directory anchor) is reported in
diagnostics and never blocks. To reproduce the
underlying Cargo error for one path, run from its parent directory:

```sh
cargo locate-project --workspace --manifest-path /path/to/Cargo.toml
```

The diagnostic includes a short, sanitized excerpt of Cargo's error chain.
Cargo's exact wording can vary by version and failure type.

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

### Unresolved ownership blocks its own scope

Before any output analysis or cleanup qualification, every `Cargo.toml` under
the selected scope must be accounted for by either a direct successful
resolution or successful workspace metadata listing it as a member or root.
An earlier failed member attempt is cleared only if later authoritative
metadata covers it.

Any remaining unresolved manifest **inside the selected scope** blocks that
scope in Preview, Simulate, and Execute, reports `unresolved_ownership`
under `--stats`, and runs **no** `cargo clean` command — including Cargo's
own preview dry-run. A manifest Cargo itself refused (a scope note or a
refused-manifest passenger) is reported, never blocking: it owns no bytes
to overlap. A manifest outside the scope is reported in diagnostics and
never blocks. Read-only scans stay partial-result tolerant; that asymmetry
is deliberate.

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

On Windows the canonical path may come back as `\\?\C:\…`; that prefix is
stripped before matching, so it never has to appear in a pattern.

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

Four entries are absolute-checked, and a relative one is rejected with exit
`2`: `scan.root`, `scan.ignore`, `scan.unignore`, and
`cleanup.allowed_output_roots`. The real messages are `configuration error:
ignore pattern must be absolute: <value>`, `configuration error: unignore must
be absolute: <value>`, `configuration error: scan.root must be absolute:
<value>`, and `configuration error: cleanup.allowed_output_roots must be
absolute: <value>`.

`include` and `exclude` are **not** absolute-checked. They are globs matched
against the canonical workspace root and may be relative; only a malformed
glob is rejected there, also with exit `2`.

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
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#unignore-re-admitted-siblings-fixed-after-016)
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

> **That schema document is currently behind the code in two specific ways.**
> It omits the reason code `skipped_outside_selection` (the code carries 20
> reason codes), and it does not disclose that the JSON envelope drops two
> fields the safety model depends on: `capability` (present on the
> unresolved-participant domain type, absent from the serialized participant)
> and `out_of_scope_unresolved` (the count of manifests that did not resolve
> outside the selected scope). Read the code alongside the schema document.

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

`update --format json` is the one exception to the envelope; see
[docs/UPDATE.md](UPDATE.md). It differs from the envelope in four concrete
ways: it carries its own `schema_version` literal rather than the envelope's
contract, it has **no** `scope` and **no** `mode` (an update resolves no
cleanup scope and runs no cleanup mode), its `result` object is a different
projection entirely, and it is assembled from a hand-rolled
`serde_json::Map` rather than a typed struct — so a change to an unrelated
struct cannot move a published field.

It is not an untested surface: four unit tests in `mod update_tests`
(`src/output.rs`) pin its schema version, its identity fields, its provenance
tokens, and the one-document-plus-one-newline stream contract, and a CLI
contract case covers the JSON refusal path.

### Output formats

| `--format` | Shape | Use it when |
|---|---|---|
| `human` (default) | tables, per-unit rows, diagnostic detail | a person is reading |
| `json` | exactly one schema-1 document on stdout | software needs complete, per-unit data |
| `log` | exactly one bounded ASCII line on stdout | a scheduler retains a short history tail |

`config` is the one command with **no** JSON shape at all. `config path` and
`config show` are a raw path and TOML text, and emitting those on a
`--format json` stdout would break the one-JSON-document automation contract,
so `cargo cleanme config show --format json` is refused with exit `2` —
`configuration error: config command does not support --format json; use human
or log output`. `--format log` is accepted and exits `0`.

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

→ [docs/AUTOMATION.md](AUTOMATION.md) for the common field list, the scheduling
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

The qualified-version lists above are a **hand-maintained copy** of the
runtime capability table in `src/workspace.rs` — they are prose here, not
generated, and they must be kept in sync with that table by hand.

`release/selector-qualification.json` is the single machine-readable
authority, and four things are proved to agree with it by
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
- A successful `scan --all` (Full) replaces corrupt or obsolete state **after**
  reconciliation completes. An incomplete Full scan leaves it untouched.
- State written by a **newer** schema is preserved, and that version must be
  upgraded before it can be reconciled.
- A persistence failure is reported on stderr and never changes the exit code.
- Deleting discovery state is **not** the normal recovery procedure.
- A learned or seed location that provably vanished (`ENOENT`) or is
  positively not a directory is **omitted from the current cleanup run** with
  a bounded diagnostic; the remaining roots still receive complete ownership
  proof. The omission is never written back — only a complete Full
  reconciliation can change learned state.
- A learned or seed location that is a symlink (never followed, including
  broken links) or that cannot be read is **not** omitted: the run blocks
  with a typed report and exit `1`, because omitting it could
  conceal an expected scan tree. Explicit roots (`scan.root`, `scan ROOT`,
  `clean ROOT`) are never omitted; any defect in one is a fatal usage error.
- A complete, certain Full reconciliation prunes learned directories that are
  positively confirmed nonexistent, independent of retention. An existing
  directory with no projects keeps the age rule below, and uncertainty,
  partial/failed Full passes, newer schemas, and unpublishable state never
  shrink the learned scope.

A Full `scan --all` on a typical workstation reports permission-denied diagnostics
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
