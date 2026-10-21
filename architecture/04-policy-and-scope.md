# Policy & Scope — deciding what to look at

> Component deep dive · part of the [architecture overview](overview.md)

`src/policy.rs` — 1,660 lines, 875 of them production; the inline test
module is the remaining 785.

## 1. Responsibility

`policy.rs` converts an *intent* (`ScanRequest`) plus *user configuration*
(`ScanConfig`) into the *concrete scope* (`EffectiveScanPolicy`) that
`discovery.rs` then walks. It is the only place in the crate where the
Routine / Full / Explicit distinction is decided.

It does a second, separate job: it is the **cleanup admission seam**. The
scan path decides *what to look at*; the admission seam decides *what may be
cleaned*, and it does so by provenance, before any canonical collapse can hide
a symlinked root. The two jobs share the module but not the decision.

**Owns:** scope-mode selection and its precedence (`policy.rs:19-58`); the
per-scope recency window (`:22`, `:54`); per-scope filter activation (`:24-27`,
`:42`, `:47-50`); existence / real-directory validation of an explicit root
(`:32-41`); the provenance vocabulary — `RootProvenance`, `AutomaticOmission`,
`AutomaticBlock`, `ClassifiedRoots` (`:130-141`); the admission classifier
(`:219-380`); the late premise recheck (`:382-505`); the Full cleanup verdict and gate — `FullCleanupVerdict`,
`full_cleanup_roots`, `RootVerdict` (`:518-664`); the raw Routine cleanup
candidates (`:630-668`); Routine root assembly (`:670-715`); platform global
root and prune enumeration (`:716-803`); root normalization, dedup and
containment collapse (`:805-868`); the rustup managed-tool prune list
(`:870-874`).

**Does not own:** filesystem traversal (`discovery.rs`), manifest → workspace
resolution (`workspace.rs`), activity classification (`traverse.rs`), state
reconciliation and atomic publish (`discovery_state.rs`), the cleanup root list
(`main.rs:878-1064`), the per-root coverage *measurement* that feeds the Full
gate (`discovery.rs:572-622`), and the user-facing scope *label*
(`main.rs:1351-1357`).

**Actual dependency set.** A grep for `crate::` in `policy.rs` yields exactly
five modules:

| Dependency | Used at | For |
|---|---|---|
| `crate::config::ScanConfig` | `policy.rs:2`, `policy.rs:19` | input config |
| `crate::domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope}` | `policy.rs:3` | in/out vocabulary |
| `crate::domain::{RootCoverage, UnprovenRoot}` | `policy.rs:522`, `:574`, `:605` | M018's per-root coverage verdict and the unproven-root report |
| `crate::error::AppError` | `policy.rs:4` | the only error type returned |
| `crate::discovery_state` | `policy.rs:649`, `:649`, `:649`, `:649`, `:649`, `:649`, `:649`, `:649`, `:649` | `load_default`, `StateLoad`, `now_seconds`, `DiscoveryState`, `automatic_root_is_maintainable` |
| `crate::discovery::effective_rustup_home` | `policy.rs:871` | managed-tool prune list |

Plus the `directories` crate (`policy.rs:220`, `:400`, `:671`) and, on Windows only,
`windows_sys` (`policy.rs:781-805`).

**Boundary with `discovery.rs` prune helpers.** `discovery.rs` owns the
*environment-derived* prune sets: `effective_cargo_home` (`discovery.rs:164`),
`effective_rustup_home` (`discovery.rs:145`) and `cargo_home_prunes`
(`discovery.rs:176`, i.e. `$CARGO_HOME/registry` and `$CARGO_HOME/git`).
`policy.rs` owns only the *platform* and global-only prune sets. The division
is by scope, not by subject: `cargo_home_prunes()` applies to **every** scope
(`discovery.rs:364`) because a registry tree must never trigger Cargo
resolution even under an explicit root, whereas the rustup list applies
**only** to `Global` or `Routine` (`discovery.rs:365-371`) and is sourced from
`policy::global_discovery_policy().managed_tool_prunes`, populated by
`managed_rust_prunes()` (`policy.rs:870`). `policy.rs` reaches into
`discovery` exactly once (`policy.rs:871`), for a single path; everything else
flows the other way.

**I/O: it does perform I/O, read-only.** The module is a *decision* module, not a
*pure* one. Its scan-side logic is a total function of `(request, config)` for
the Full and Explicit branches; the Routine branch additionally consults the
filesystem, the process environment, the wall clock, and the persisted state
file. The admission seam reads metadata for every candidate root. All of these
calls are reads:

| Call | Location | Reads |
|---|---|---|
| `fs::symlink_metadata` | `policy.rs:32` | explicit root stat (resolve) |
| `fs::symlink_metadata` | `policy.rs:229`, `:243` | symlink identity, trailing separators stripped |
| `fs::symlink_metadata` | `policy.rs:256` | admission classification |
| `fs::symlink_metadata` | `policy.rs:412`, `:473` | late premise recheck |
| `fs::symlink_metadata` | `policy.rs:645` | raw seed existence (cleanup candidates) |
| `fs::read_dir` | `policy.rs:280` | coverage proof for a non-maintainable root |
| `Path::is_dir` (seeds) | `policy.rs:635` | seed existence |
| `directories::BaseDirs::new()` | `policy.rs:220`, `:400`, `:671` | env / platform home |
| `discovery_state::load_default()` | `policy.rs:677` | state JSON file |
| `discovery_state::now_seconds()` | `policy.rs:650`, `:696` | wall clock |
| `Path::new("/usr/local").is_dir()` | `policy.rs:721` | macOS filesystem |
| `GetLogicalDrives` / `GetDriveTypeW` | `policy.rs:782`, `:794` | Windows volumes |
| `fs::canonicalize` | `policy.rs:587`, `:847` | root identity |
| `discovery::effective_rustup_home` | `policy.rs:871` | `RUSTUP_HOME`, home |
| `eprintln!` | `policy.rs:680` | one stderr warning line |

It never writes to disk and never spawns a process. The single `eprintln!` at
`policy.rs:680` is the module's only side effect, and it exists so a degraded
Routine root set is never silent.

## 2. The scope decision

```rust
pub fn resolve(request: ScanRequest, config: &ScanConfig)
    -> Result<EffectiveScanPolicy, AppError>          // policy.rs:19
```

### Precedence, in code order

1. **`request.full == true` → `Global`, unconditionally** (`policy.rs:23-32`).
   The function returns before `request.cli_root` and `config.root` are ever
   read. `--full` therefore beats *both* a CLI root and a configured
   `scan.root`: if all three are set, Global wins.
2. **Otherwise `request.cli_root` beats `config.root`**
   (`policy.rs:30`: `request.cli_root.or_else(|| config.root.clone())`).
3. **If a root survived step 2 → `Explicit(root)`** (`policy.rs:42`), with
   `DiscoveryFilters::Bypassed`.
4. **Otherwise → `Routine(routine_roots(...))`** (`policy.rs:46`), with
   `DiscoveryFilters::Active`.

The specific case of an explicit CLI root *and* `--full` *and* a configured
`scan.root` resolves to **`ScanScope::Global`**, because the `full` early-return
at `policy.rs:20` precedes the `or_else` at `policy.rs:30`. That combination
is unreachable from the command line: `scan` requires exactly one of ROOT /
`--known` / `--all` through an `ArgGroup` (`cli.rs:139`), so `scan --all` and
`scan ROOT` are mutually exclusive, and the hidden `clean` alias carries
`conflicts_with_all = ["root", "full"]` on `--known` (`cli.rs:175`) and
`conflicts_with_all = ["root", "known"]` on `--full` (`cli.rs:178`). No in-repo
caller constructs the conflicting pair either — `tests/end_to_end.rs:124` and
`examples/traversal-qualification.rs:26` both pass `cli_root: None`. The
precedence therefore matters only for a future programmatic caller.

Note the asymmetry with cleanup, which is decided in `cli.rs` rather than here:
for `clean`, `--full` wins over a ROOT argument, because the merged scope match
tests `full` first (`cli.rs:511-529`) — although clap has already refused the
pair outright.

### Fields populated on `EffectiveScanPolicy`

| Field | Full (`policy.rs:20-29`) | Explicit (`policy.rs:42`) | Routine (`policy.rs:43-52`) |
|---|---|---|---|
| `recency` | `config.recency_seconds` | same | same |
| `scope` | `Global(global_discovery_policy().roots)` | `Explicit(cli_root \| config.root)` | `Routine(routine_roots(retention_days))` |
| `discovery_filters` | `Active { ignore, unignore }` from config | `Bypassed` | `Active { ignore, unignore }` from config |

All three paths share one construction site (`policy.rs:53-57`). `recency` is
scope-independent — it is `Duration::from_secs(config.recency_seconds)` on
every path (`policy.rs:22`, `:54`). The type is
`domain::EffectiveScanPolicy` (`domain.rs:27-32`); `recency` is captured at
`main.rs:1430` and turned into the activity cutoff at `main.rs:1627`, not by
the walker.

### Errors returned

`resolve` returns exactly one error variant, from two sites, both in the
Explicit branch:

| Site | Condition | Message |
|---|---|---|
| `policy.rs:32-35` | `fs::symlink_metadata` fails | `AppError::InvalidRoot { path, reason: e.to_string() }` — includes the OS error (missing, permission denied) |
| `policy.rs:36-41` | not a directory, **or** is a symlink | `AppError::InvalidRoot { path, reason: "expected a real directory" }` |

The Full and Routine branches of `resolve` are infallible. A Routine scope
with no available roots returns `Ok(ScanScope::Routine(vec![]))`, never an
error; the user-facing signal is a diagnostic emitted later by
`discovery.rs:379-386` and the one stderr line at `policy.rs:680`.

The **module** as a whole has three more error sites, all on the cleanup side:

| Site | Condition | Message |
|---|---|---|
| `policy.rs:259-262` | an *explicit* admission candidate is not a real directory | `AppError::InvalidRoot { …, reason: "expected a real directory" }` |
| `policy.rs:317-320` | an *explicit* admission candidate cannot be stat'ed | `AppError::InvalidRoot { …, reason: e.to_string() }` |
| `policy.rs:576-582` | the Full reconciliation produced no fresh in-memory generation | `AppError::Config("full reconciliation produced no fresh generation; no cleanup commands were run against stale learned roots")` |

The third site is the C028 property, and it is the **only** Full-side refusal
left: an incomplete Full reconciliation is no longer an error here. It is
carried as a *verdict* instead — see §5, *The Full cleanup gate is a verdict,
not a refusal*.

A propagated `AppError` reaches `main.rs:21-37` and becomes exit code 2.

## 3. The three scope modes

`ScanScope` (`domain.rs:12-17`) has **four** variants, but `resolve` produces
only three of them:

| Variant | Produced by `resolve`? | Meaning |
|---|---|---|
| `Explicit(PathBuf)` | yes, `policy.rs:42` | one user-named root |
| `ExplicitRoots(Vec<PathBuf>)` | **no** | multiple explicit roots forming one cleanup boundary; constructed only in `cleanup.rs:1531`, `cleanup.rs:2036` and `cleanup.rs:3193` |
| `Global(Vec<PathBuf>)` | yes, `policy.rs:23` | exhaustive platform roots |
| `Routine(Vec<PathBuf>)` | yes, `policy.rs:46` | bounded adaptive roots |

Which spelling reaches which scope is decided in `cli.rs` and `main.rs`, not
here: `scan ROOT` → Explicit, `scan --known` → Routine, `scan --all` → Global
(`cli.rs:386-407`, `main.rs:1403-1408`); `cargo cleanme ROOT` → Explicit,
`cargo cleanme --full` → Global, and `clean --known` plus the bare front door →
the maintenance scope (`cli.rs:471-489`, `cli.rs:511-529`), which resolves to
Routine **or** to Explicit when a `scan.root` is configured (§7.8).

### `Explicit(root)` — the user named a place

One directory, no `scan.ignore` / `scan.unignore` at all
(`DiscoveryFilters::Bypassed`, `policy.rs:42`). Worked example: `scan ~/work/api`
walks `~/work/api` recursively and reports every `Cargo.toml` beneath it, even
under a directory the user has globally ignored in `config.toml`. It still
enforces the symlink and real-directory rule (`policy.rs:36-41`), and it is the
only branch of `resolve` that can fail. A symlink *to* a directory is rejected.

### `Routine(roots)` — the maintenance scope, bounded

The Routine *scan* is `scan --known`
(`main.rs:1403-1408` → `run_scan(None, false, …)`); the Routine *cleanup* is the
bare front door and `clean --known`, which resolve their scope through
`policy::resolve` at `main.rs:967-973`. Roots are existing seed
directories under `$HOME` plus retained learned roots from the state file (§5).
Worked example: a developer with `~/Code/api` and `~/Code/web` plus a learned
root at `~/scratch/lab`, where the state file's `last_project_seen_at` for
`~/scratch/lab` is 10 days old and retention is 30.
`canonical_dedup_roots` (`policy.rs:826`) collapses nothing here, so the scan
walks all three, applying `scan.ignore` / `scan.unignore` normally. A Rust
project in `~/Documents/sidequest` is **not** found — that is the deliberate
blind spot, and `scan --all` is the documented way to teach the tool about it.

### `Global(roots)` — `--all`

`ScanScope::Global(global_discovery_policy().roots)` (`policy.rs:23`). Worked
example on Linux: roots is `["/"]` and the platform prunes are
`/proc`, `/sys`, `/dev`, `/run` (`policy.rs:765-768`), so the walk is
filesystem-root breadth minus pseudo-filesystems. `scan.ignore` / `scan.unignore`
apply. This is also the only scope that runs
`discovery_state::reconcile_full` afterwards (`main.rs:1543`, reached only when
`full` is true — `main.rs:1405`).

### Cost / benefit

Routine is bounded by the developer's actual working set; Full is bounded by
the machine. ADR 002 records the measurement behind that split: the exact
reference macOS scan still exceeded 120 seconds, and M006D measured roughly
4.64 million visited entries at the timeout while validating about 1,800
`Cargo.toml` candidates consumed only about 0.13 seconds — traversal breadth,
not Cargo validation, is the dominant cost (ADR 002, *Context*). The ADR's
decision is to keep exhaustive reachability as an explicit product mode rather
than narrow it permanently, and lists "Full scans remain expensive by design" as
an accepted cost (*Consequences*).

Cross-reference: this module is pipeline step 2 of `run_scan`
(`main.rs:1386`), described in [the overview's scan pipeline](overview.md) §6 as
"Resolve scope — `policy::resolve` turns `{cli_root, full}` into concrete roots
+ a recency window".

## 4. Pruning inputs

`DiscoveryFilters` (`domain.rs:19-26`) has two states, and `resolve` picks
between them — it never merges them:

| State | Set by | Effect in `discovery.rs` |
|---|---|---|
| `Active { ignore: Vec<String>, unignore: Vec<PathBuf> }` | Full (`policy.rs:24-27`) and Routine (`policy.rs:47-50`) | `Filters::new(ignore, unignore)` at `discovery.rs:363` |
| `Bypassed` | Explicit (`policy.rs:42`) | `(&[][..], &[][..])` at `discovery.rs:361` |

`config.ignore` and `config.unignore` are cloned verbatim from `ScanConfig`
(`config.rs:47-48`); `resolve` adds no patterns of its own.

### Semantics

- **ignore globs prune, and the exclusion is inherited.** Each `scan.ignore`
  string is compiled into a `globset::GlobSet` (`discovery.rs:18-30`) and matched
  against `path.to_string_lossy()` (`discovery.rs:38-43`). The lossy match is
  deliberate: a non-UTF-8 directory name must not escape a broad `*` rule on
  Linux (comment at `discovery.rs:31-37`, test
  `non_utf8_directory_still_matches_a_broad_ignore_pattern` at
  `discovery.rs:1720`). The match is also consulted for every **ancestor** of the
  candidate (`Filters::inherits_ignore`, `discovery.rs:52-55`), because a literal
  ignore rule names a directory, not its children: `/archive` is not a match for
  `/archive/other`, but `other` is still under an ignored directory and must
  stay excluded. This is the C019 correction, and the walk descends into
  `/archive` only as far as an exception requires.
- **`unignore` re-includes a subtree, not a single entry.** One function decides
  all of it: `Filters::disposition` (`discovery.rs:68-87`) returns
  `Included` / `Pruned` / `PassThrough` / `ReIncluded` (enum at
  `discovery.rs:102-107`). `ReIncluded` applies when
  `p.starts_with(u)` for some unignore entry, so the unignore path *and everything
  beneath it* escapes the prune. `PassThrough` is the other half: excluded, but
  with an unignore entry at or below it, so the walker enters far enough to reach
  the   exception — and only that route. Tests:
  `filters_ignored_parent_keeps_exception_route` (`discovery.rs:2138`) for the
  `/*` shape, `literal_ignored_ancestor_becomes_pass_through_and_not_inherited_by_siblings`
  (`discovery.rs:2168`) for the literal shape, and
  `literal_ignored_ancestor_does_not_readmit_ignored_siblings`
  (`discovery.rs:2243`) for the same case through a real walk.
- **a CLI scan root takes precedence over configured scope.** Verified at
  `policy.rs:30`. The stronger form is what the code actually does: an explicit
  root does not merely win the root slot, it switches filters to `Bypassed`, so
  configured `scan.ignore` / `scan.unignore` do not apply to it at all. The
  inline test helper comment states the same intent
  (`discovery.rs:1650-1651`).

### What must be absolute, and why

`scan.root` must be absolute (`require_absolute(p, "scan.root")`,
`config.rs:112-114`); every `scan.ignore` pattern must be absolute
(`config.rs:115-121`, message `"ignore pattern must be absolute: {p}"`); every
`scan.unignore` entry must be absolute (`config.rs:122-124`).

Absoluteness is not cosmetic: patterns are matched against the **canonical**
walk path, so a relative or symlink-spelled pattern would silently match
nothing. `config::load` rewrites the glob-free literal prefix of every pattern
to its canonical spelling via `canonical_pattern_prefix` (`config.rs:155-187`,
`config.rs:353-403`), leaving the glob tail verbatim. A pattern naming a non-existent
path is left untouched — it still means exactly what it says, i.e. it matches
nothing. Glob *syntax* is validated at load time too (`config.rs:147-151` for
`scan.ignore`, `config.rs:137-146` for the cleanup include/exclude globs), so
a bad pattern fails at `config::load_or_create` rather than at the first walk.

**What `globset` is for:** it compiles the ignore list once into a `GlobSet`
(`discovery.rs:19-27`) so the per-directory prune test is a set lookup rather
than a re-parse. The unignore side uses no globbing at all — it is exact
`Path::starts_with` prefix comparison on absolute `PathBuf`s, which is why
`config.toml:27` calls them "Exact absolute directories".

**The template and the code now agree.** `config.toml:21-24` says these
filters "apply to global discovery and to the Routine scope", which is exactly
what the code does: `resolve` activates them for both Full (`policy.rs:19-58`)
and Routine (`policy.rs:47-50`), and `discovery.rs` hands the compiled filter
set to the global branch (`discovery.rs:395`) and to the per-root branch
(`discovery.rs:427`) alike. The earlier divergence — a template that claimed
"global discovery" only while the code also applied the filters to Routine —
has been closed.

## 5. Seed candidates

```rust
pub fn routine_seed_candidates(home: &Path) -> Vec<PathBuf>   // policy.rs:60
```

Returns `home.join(name)` for this exact ordered list of 13 names
(`policy.rs:61-75`):

| Position | Name | Position | Name | Position | Name |
|---:|---|---:|---|---:|---|
| 1 | `Projects` | 6 | `code` | 11 | `github` |
| 2 | `projects` | 7 | `src` | 12 | `workspace` |
| 3 | `Developer` | 8 | `repos` | 13 | `workspaces` |
| 4 | `dev` | 9 | `Repos` | — | — |
| 5 | `Code` | 10 | `GitHub` | — | — |

The order is the literal array order, not a priority order. The paired
capitalizations `Projects`/`projects`, `Code`/`code`, `Repos`/`repos`,
`GitHub`/`github` exist because macOS and Windows volumes are case-insensitive
by default while Linux is not — the same reasoning that drives
`is_manifest_name` (`discovery.rs:108-125`).

**This function is pure and does no filtering.** It is a literal `join` over a
fixed array: no existence check, no config read, no env read, no state read.
Existence filtering happens one level up, in `routine_roots_from_state`
(`policy.rs:670`: `.filter(|p| p.is_dir())`).

**Who supplies `home`:** `routine_roots` (`policy.rs:670-709`) builds it itself
from `directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())` — the
same crate and accessor used by `discovery.rs:148` and `discovery.rs:172`, but
obtained independently of `main.rs`. `policy::resolve` cannot inject a home
directory; `routine_seed_candidates` is the only seam, and it exists so the list
can be asserted without touching the real home directory. If
`BaseDirs::new()` returns `None`, `routine_roots` returns an empty `Vec`
(`policy.rs:671-673`) and Routine silently degrades to "no roots", which
`discovery.rs:378-387` reports as `Info` / `PlatformRoot`.

**Heuristic vs exact:**

| Part | Status |
|---|---|
| The 13 directory names | **Heuristic.** A guess at where developers keep Rust work. Nothing verifies it. |
| `home.join(name)` | **Exact.** |
| The `.is_dir()` filter | **Exact**, one stat per candidate. |
| `canonical_dedup_roots` normalization | **Exact** for roots that canonicalize; lexical for the rest (§6). |
| The `last_project_seen_at` recency filter | **Exact** against the persisted state and the wall clock. |
| Whether the seeds *cover* the user's work | **Not a property of this code** — the accepted cost in ADR 002 ("the 30-day default limits stale routine roots without creating permanent blind spots"). |

`routine_seed_candidates` is `pub` but has **no non-test caller outside
`src/policy.rs` itself** —
the call sites are `policy.rs:643` (`routine_cleanup_candidates`) and
`policy.rs:690` (`routine_roots_from_state`). It is public surface without an external
consumer.

**Assembly.** `routine_roots_from_state` concatenates existing seeds first,
then learned roots that pass both retention and the automatic-maintenance
location policy, then hands the vector to `canonical_dedup_roots`. The shared
`discovery_state::automatic_root_is_maintainable` rule
(`discovery_state.rs:114-207`) excludes known cache (`~/.cache`,
`~/Library/Caches`, pnpm stores), Trash, temporary locations (platform temp
root plus the `/tmp`, `/private/tmp`, `/var/tmp` aliases), system software
locations (`/usr/local`, `/opt`), `node_modules`, the `~/Downloads` drop zone,
and transient Codex worktree locations from Routine selection. It is not a Full
discovery filter and does not affect an explicit root. Retention remains:

```rust
automatic_root_is_maintainable(path, home)
    && (retention_days == 0
    || now < r.last_project_seen_at               // future timestamp: keep
    || now.saturating_sub(r.last_project_seen_at) <= u64::from(retention_days) * 86400)
```
(`policy.rs:680-684`)

`retention_days == 0` disables expiration and a future timestamp fails
conservative — both match ADR 002 §5. A `StateLoad` that is not `Loaded`
(`Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable`) yields
`load.diagnostic()` as a warning and **keeps the seeds** (`policy.rs:694-714`),
matching ADR 002 §7's fail-soft rule for a corrupt state file.

### Cleanup admission is provenance-aware, scan assembly is not (C028)

`resolve` above decides what a *scan* walks. Cleanup admission is a separate
seam that runs before any collapse, because collapsing first would resolve a
symlinked root into its target and hide the invalid identity:

- `RootProvenance` (`policy.rs:97`) labels each candidate `Explicit`,
  `Automatic`, or `Operator`; `ClassifiedRoots` (`policy.rs:137`) carries the
  admitted, safely omitted (`AutomaticOmission::NotFound` / `NonDirectory` /
  `NotMaintainable` / `OperatorUnavailable` / `OperatorNonDirectory`,
  `policy.rs:114`), and blocking (`AutomaticBlock`, `policy.rs:130`) sets.
- `classify_cleanup_roots` (`policy.rs:219`) uses `symlink_metadata` and the
  real `io::ErrorKind` — never `Path::exists`/`is_dir`, which conflate errors
  and follow symlinks — and has no catch-all `Err(_) => continue`. A missing
  automatic root is checked for provenance *before* availability: a missing
  path at a known-transient origin omits as `NotMaintainable`, a missing
  durable location omits as `NotFound`; symlinks never reach that arm and
  every other failure still blocks.
- `path_is_symlink` (`policy.rs:228-248`) closes the trailing-separator
  bypass: `symlink_metadata("link/")` follows the final component on POSIX, so
  a trailing slash (or a trailing `/.`) would otherwise hide a symlink from the
  `is_symlink` check. The stripped spelling is judged too.
- `routine_cleanup_candidates` (`policy.rs:638`) supplies the raw Routine
  candidates (existing seeds in any form plus age-eligible learned roots)
  without canonicalization, so a symlinked seed reaches classification as a
  link. `full_cleanup_roots` (`policy.rs:571`) supplies the Full roots from
  the in-memory generation the reconciliation just proved, or refuses.
- `recheck_admission_premise` (`policy.rs:396`) re-examines the premise with
  fresh metadata before the first final proof and before every later spawn;
  `CleanupCandidates` (`policy.rs:509`) is the shared candidate type.

Omission diagnostics are grouped by typed reason and sorted deterministically
(`ClassifiedRoots::omission_diagnostics`, `policy.rs:166-210`). Each summary
includes the exact omitted-root count and at most three sanitized paths. Only
the reasons the operator can act on are reported: a root omitted because it
sits at a known-transient origin — an agent worktree, a package cache, a
temporary directory — is omitted *by design*, because the tool would never
have cleaned it, so its omission narrows nothing the operator was relying on
and is not news (C038). A durable root that has vanished, and a root that is
no longer a directory, are reported. The omission vector remains complete for
`recheck_admission_premise`; reporting does not retire persisted state or
alter the selected cleanup universe.

### Operator provenance — the working directory's roots (ADR 009)

`RootProvenance::Operator` (`policy.rs:295`) is the third variant, added by
ADR 009. These are the roots the working directory contributes to the
maintenance scope, and the operator chose them by standing where they stand.
The maintainability rule therefore does not apply to them: that rule stops
the tool from *choosing* a transient origin on its own — an agent worktree, a
package cache, a temporary directory — and `clean ROOT` already permits the
same subtree. Every other defect keeps the automatic root's handling exactly:
a symlink blocks the scope rather than being followed, an absent or
non-directory root is omitted, and an indeterminate filesystem failure
blocks. A scope that quietly reduced itself is worse than one that stops.

The omission vocabulary carries the cause in the variant
(`AutomaticOmission::OperatorUnavailable` / `OperatorNonDirectory`,
`policy.rs:119-121`) rather than reusing `NotFound` / `NonDirectory`. Two
things depend on that. The pre-spawn recheck reads the variant to know which
premise must still hold; and the diagnostic reads it to name the root
truthfully. An operator root was never a *learned* one, and saying it was
would misreport what the tool did.

`omission_diagnostics` groups by `(reason, noun)` (`policy.rs:166-210`), so
the published "learned root(s)" wording is byte-identical to what it was
before ADR 009 and an operator root's omission reads "operator root(s)". A
closed record's quoted message cannot be invalidated by adding a bucket.

### The Full cleanup gate is a *verdict*, not a refusal (ADR 008 §5b, M018)

`full_cleanup_roots` (`policy.rs:571-618`) is the only place where a scan
result gates a destructive operation. It no longer refuses on a non-zero
reconciliation code; it returns a typed verdict:

```rust
pub struct FullCleanupVerdict {          // policy.rs:518-525
    pub proven: CleanupCandidates,
    pub unproven: Vec<crate::domain::UnprovenRoot>,
    pub generation: Option<u64>,
}
```

Three inputs decide it (`policy.rs:571-575`): the reconciliation exit code, the
in-memory state the traversal just proved, and the per-root coverage verdict.

- **A missing fresh generation refuses** (`policy.rs:576-582`):
  `AppError::Config("full reconciliation produced no fresh generation; …")`,
  which becomes exit code 2 through `main.rs:21-37`. The roots come from the
  generation the traversal just proved, never from a disk reload that could be
  an older generation after a failed or unsupported publication. A *failed
  save* is not a refusal — it stays a stderr warning for the caller
  (`main.rs:926-930`).
- **A non-zero reconciliation code no longer refuses.** The platform walk's
  incompleteness is not evidence that *every* learned root is unproven — it is
  evidence that the platform walk did not finish. When the code is zero the
  platform walk read every learned root's subtree, so every root is proven by
  definition and no measurement is needed (`policy.rs:588-591`). When it is
  non-zero the root is judged on its own coverage entry
  (`policy.rs:592-602`): an entry with `complete == true` is proven, an
  incomplete entry is unproven *with that entry's detail*, and a root with no
  coverage entry at all is unproven with no detail — an unmeasured root is an
  unproven root.
- **A missing coverage verdict fails closed.** `coverage` is `None` only when
  the platform walk completed. When the walk was incomplete it *must* carry the
  verification result; `coverage.and_then(…)` yields `None` for a root that is
  absent from the slice, and that root is unproven
  (`policy.rs:600`). The contract is stated in the function's own doc comment
  (`policy.rs:563-570`).

The proven roots carry `RootProvenance::Automatic` (`policy.rs:604`), so the
shared admission path applies to them exactly as it does to every other
automatic root. The unproven roots carry a typed reason:
`crate::domain::UnprovenRoot { root, detail }` (`policy.rs:605-612`), with the
default detail naming the reason the root could not be measured
(`policy.rs:607-611`).

This is the **ADR 008 §5b route 5b-i** rule, and it narrows the ADR 007
reservation: ADR 007 §1 reserved whole-system fail-closed blocking for the
system-wide reconciliation operation. Under M018 the Full cleanup's proof
boundary is the **learned root**, not the platform root — a root whose own
traversal completed is a candidate, a root whose traversal did not complete is
reported and never a candidate. ADR 008 is **accepted** (2026-10-21); it
resolved §4 to 4b (the canonical spelling is the root-level `--full`, with
`clean --full` retained as a hidden alias) and §5 to 5b, route 5b-i.

The unit tests pin the three branches:
`full_cleanup_gate_proves_the_roots_whose_own_traversal_completed`
(`policy.rs:1225`) — an incomplete platform walk with a per-root verdict
yields the proven roots and reports the unproven ones;
`full_cleanup_gate_proves_every_root_when_the_platform_walk_completed`
(`policy.rs:1281`) — a zero exit code proves every learned root without a
coverage verdict; and
`full_cleanup_gate_proves_nothing_when_the_coverage_was_never_measured`
(`policy.rs:1303`) — the fail-closed arm.
`full_cleanup_consumes_only_a_fresh_in_memory_generation`
(`policy.rs:1335`) still pins the C028 property: a missing in-memory
generation refuses (`policy.rs:576-582`), while a present one yields the
automatic roots plus the generation.

## 6. Global discovery policy

```rust
pub struct GlobalDiscoveryPolicy {              // policy.rs:13-18
    pub roots: Vec<PathBuf>,
    pub global_only_prunes: Vec<PathBuf>,
    pub managed_tool_prunes: Vec<PathBuf>,
}
```

### The two `global_discovery_policy` definitions — the truth

| Lines | Gate | Body |
|---|---|---|
| `policy.rs:716-723` | `#[cfg(unix)]` | delegates to `unix_policy(...)`, then hands the result to `compose_global_policy` (`policy.rs:732-743`), which fills `roots` through `canonical_dedup_roots_protecting` and `managed_tool_prunes` through `managed_rust_prunes()` |
| `policy.rs:779-803` | `#[cfg(windows)]` | enumerates drive letters with `GetLogicalDrives` / `GetDriveTypeW` |

**They are not duplicates and neither is dead.** They are mutually exclusive
`#[cfg]` variants of the same public function: exactly one is compiled per
platform, and a name collision never occurs. On any other target neither
exists, and the `resolve` call at `policy.rs:19` would not compile — which is
consistent with the crate shipping a single binary for unix and Windows only.

A third, different function shares the family: `unix_policy`
(`policy.rs:755-777`, `#[cfg(unix)]`, private) is a *pure* helper whose three
booleans are supplied from `cfg!` plus one filesystem probe
(`policy.rs:696-699`). It exists so the macOS and Linux tables can be unit
tested on any host. It always returns `managed_tool_prunes: Vec::new()`
(`policy.rs:772-775`); the `#[cfg(unix)]` wrapper supplies that field
(`policy.rs:717-722`).

### Platform tables

| Platform | `roots` | `global_only_prunes` | Source |
|---|---|---|---|
| macOS | `["/"]`, plus `/usr/local` when that directory exists | `/System`, `/dev`, `/bin`, `/sbin`, `/usr` | `policy.rs:758-764` |
| Linux | `["/"]` | `/proc`, `/sys`, `/dev`, `/run` | `policy.rs:765-768` |
| Other unix | `["/"]` | none | `policy.rs:769-771` |
| Windows | `X:\` for each letter where `GetLogicalDrives` reports a bit and `GetDriveTypeW` is `DRIVE_FIXED` or `DRIVE_REMOVABLE` | none | `policy.rs:783-806` |

The macOS row is the interesting one: `/usr` is pruned while `/usr/local` is
enumerated as a *root*, so the Homebrew toolchain subtree stays reachable
inside an otherwise-pruned `/usr`. That exception takes **two** steps to work,
and both were once missing:

1. **Composition.** `global_discovery_policy` (`policy.rs:717-743`) delegates to
   `compose_global_policy` (`policy.rs:732-743`), which collapses redundant
   roots — and the collapse folds `/usr/local` into `/`, because
   `"/usr/local".starts_with("/")`. `compose_global_policy` therefore computes
   the *protected* set first: every root sitting strictly inside a global-only
   prune is that prune's exception, so `canonical_dedup_roots_protecting`
   (`policy.rs:838-868`) keeps it as a walk root of its own.
2. **Application.** A root that survived composition would still be unwalked:
   `entry_is_within` matches on `parent_path`, so with root `/usr/local` every
   entry's parent `starts_with("/usr")` and the subtree read as pruned.
   `discovery::entry_is_system_pruned` (`discovery.rs:279-292`) exempts a walk
   root strictly inside a system prune. The exemption is scoped to *global-only*
   prunes on purpose: a Cargo or rustup home stays refused even when a root sits
   inside it.

Test `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local`
(`policy.rs:883`) pins this — and it now calls `compose_global_policy`, so it
asserts the roots **production** walks rather than the ones a private
pre-collapse helper declares. That distinction was the defect: the old test
called `unix_policy` directly and passed while production collapsed the root
away. `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root`
(`policy.rs:910`) covers the collapse alone, including that an ordinary nested
root is still folded; `a_root_inside_a_global_prune_reopens_that_prune`
(`discovery.rs:1270`) covers the prune. `managed_tool_prunes` is `Some` only via
`managed_rust_prunes()` → `discovery::effective_rustup_home()`
(`policy.rs:870-874`), i.e. `$RUSTUP_HOME` when absolute, else `~/.rustup`
(`discovery.rs:145-161`).

### Consumers

| Consumer | Behaviour |
|---|---|
| `discovery.rs:342-343` | re-derives the policy for `Global` scope only |
| `discovery.rs:344-351` | **adopts it only if `canonical_roots(roots) == canonical_roots(&candidate.roots)`**; otherwise treats the scope as a plain root list with no platform prunes |
| `discovery.rs:355` | when adopted, walks `p.roots` — which is why the `roots` carried in `ScanScope::Global` and in `GlobalDiscoveryPolicy` are kept in agreement |
| `discovery.rs:365` | `managed_tool_prunes`, gated on `global` (Global **or** Routine) |
| `discovery.rs:398` | `global_only_prunes`, passed only to `discover_global_roots` |
| `examples/traversal-qualification.rs:50` | reads `.roots` for the standalone qualification harness |

The equality guard at `discovery.rs:344-351` is why a *caller-constructed*
`ScanScope::Global` (as several `discovery.rs` tests do) does **not** silently
acquire platform prunes it never asked for.

### Root normalization

`canonical_dedup_roots` (`policy.rs:826-851`) is the shared normalizer for all
three roots lists:

1. For each root, try `fs::canonicalize`. Success → identity is the canonical
   path *and* the retained spelling is canonical. Failure → identity is
   `lexical_identity(root)` but the retained spelling stays as the caller wrote
   it (so an unavailable root still names what the user asked for).
2. Sort by identity and `dedup_by` identity equality.
3. Drop any root whose identity `starts_with` an already-kept root's identity
   (containment collapse).

`lexical_identity` (`policy.rs:809-819`) resolves `.` and `..` textually and
never touches the filesystem; the doc comment states it "is only ever compared
against other roots, never walked". The rationale comment at `policy.rs:839-843`
records the bug it fixed (L16): an unresolvable root previously kept its raw
spelling and so was neither deduped nor collapsed, causing an overlapping
subtree to be scanned twice.

## 7. Invariants and edge cases

1. **`full` beats everything.** The `full` early-return (`policy.rs:952`) sits
   above every root consideration. Reordering it would silently convert
   `--full` into a single-root scan.
2. **An explicit root wins monotonically.** `policy.rs:30` is a single
   `or_else`; no branch lets a configured root displace a CLI root, so adding
   roots to `ScanConfig` can never override a CLI root.
3. **Explicit scope bypasses filters — completely.** Not "precedence": a
   configured `scan.ignore` has no effect at all under `ScanScope::Explicit`
   (`policy.rs:42`). Pinned by `cli_root_wins_and_bypasses_filters`
   (`policy.rs:917`).
4. **A Routine root is *not* guaranteed to be under `$HOME` by this module.**
    Seeds are (`policy.rs:77`), but learned roots are taken verbatim from
    `state.learned_roots[].path` (`policy.rs:686`) with a recency test and a
    maintainability test (`policy.rs:680`) and nothing else — no containment
    check is performed here. The "no broad/platform roots are learned"
    guarantee is a *derivation-time* rule in `discovery_state.rs`
    (`is_broad_root` at `discovery_state.rs:415`, the home-equality filter at
    `discovery_state.rs:288`), i.e. ADR 002 §2, and it is not re-asserted here.
    A hand-edited or stale state file can therefore inject a
    root outside the home directory into Routine scope, and `clean --known` will
    use it.
5. **A stale configured root is a hard error, not a fallback.** `config::load`
    validates only that `scan.root` is absolute (`config.rs:112-114`), never
    that it exists. A deleted `scan.root` makes *every* scan and every bare
    invocation fail with `AppError::InvalidRoot` → exit code 2 (`main.rs:21-37`).
    There is no degradation path back to Routine. Since C028 this is true only
    of *explicit* roots: a deleted automatically learned/seed root is omitted
    (proven absence/non-directory) or blocks with a typed report (symlink or
    unreadable), via the admission seam above — a stale advisory path can no
    longer reach explicit-root validation and abort the run.
6. **Empty Routine roots are not an error.** `resolve` returns
   `Ok(Routine(vec![]))`; `discovery.rs:379-386` emits `Info` / `PlatformRoot`
   "no Routine roots are available; run `scan --full` or scan an explicit root".
7. **`learned_root_retention_days as u16` (`policy.rs:46`) is lossless only
   because of the caller's validation.** `config::load` range-checks it to
   0–3650 (`config.rs:107-111`), which fits in `u16`; a `ScanConfig` built
   directly — as every test does — bypasses that check, and a value above 65535
   would wrap silently. Same class of issue for `recency_seconds`: `load`
   rejects 0 (`config.rs:99-103`) because 0 would disable the inactivity guard,
   but `resolve` does not (`policy.rs:19`, `:19`).
8. **A configured `scan.root` redirects the maintenance scope; it does not
   disable it.** `clean --known` and the bare front door resolve their scope
   through `policy::resolve` with `cli_root: None, full: false`
   (`main.rs:967-973`), and the resulting `ScanScope::Explicit` arm is handled
   explicitly (`main.rs:978-999`): the configured root becomes the scope that
   gets cleaned, and the machine label is `"explicit"`. `ScanScope::Routine`
   (`main.rs:1018-1053`) is the no-configured-root case, and `ScanScope::Global`
   is refused rather than treated as an empty scope (`main.rs:1056-1060`).
   Setting `scan.root` is therefore not orthogonal to `--known`: it changes
   what `--known` cleans.
9. **The JSON `scope` field is derived from the resolved policy, not from the
   CLI flags** (`main.rs:1351-1357`): `Global` → `"full"`, `Explicit` and
   `ExplicitRoots` → `"explicit"`, `Routine` → `"routine"`. The function's own
   doc comment records the earlier defect — deriving it from `root.is_some()`
   reported `"routine"` for a scan that was in fact pinned to one explicit root
   (`main.rs:1343-1350`). The contract test is
   `json_scope_label_follows_the_resolved_scope_not_the_cli_flags`
   (`tests/cli_contract.rs:214`), and `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one`
   (`tests/cli_contract.rs:2464`) pins the same label for `scan --known` with
   and without a configured root.
10. **The safety-envelope coupling.** `clean --known` routes its cleanup roots
    through `policy::resolve` (`main.rs:967`), so any change to scope resolution
    changes the set of roots a destructive command may operate on. A scope
    change is a safety change and belongs in the same review as the proof work
    in [the cleanup deep dive](09-cleanup.md).
11. **State degradation is visible but not fatal.** A corrupt or newer-schema
    state file degrades Routine to seeds plus one stderr line
    (`policy.rs:657-658`, `policy.rs:672-692`); it never expands scope and never
    fails the run.
12. **The Full cleanup's proof boundary is the learned root.** Since M018,
    `full_cleanup_roots` (`policy.rs:571-618`) no longer refuses an incomplete
    Full reconciliation: it returns a `FullCleanupVerdict`
    (`policy.rs:518-525`) whose `proven` roots are candidates and whose
    `unproven` roots are reported, never candidates. The only refusal left is
    a missing fresh generation (`policy.rs:576-582`), and a root whose coverage
    was never measured is unproven (`policy.rs:600`). Every other scope is
    proof-scoped under ADR 007: an out-of-boundary unprovable participant is
    reported, never blocking. A change to this gate is a change to the safety
    boundary, not a refactor.
13. **The operator's roots are added to the Routine scope, never substituted
    for it.** ADR 009's working directory joins the maintenance scope through
    `admit_automatic` (`main.rs:1143-1203`), which classifies the Routine
    candidates first, asks `working_directory_is_unreached` (`main.rs:1122-1129`)
    whether the Routine scope already reaches the operator's directory, and
    only then classifies the operator's candidates and *extends* the admitted
    set (`main.rs:1165-1168`). The universe may only ever grow, so a wider
    universe can only reclassify `shared` bytes to `private` — never the
    reverse, which is ADR 006's shared-output hazard. The decision is made
    against the **admitted** roots, so a root the maintainability rule
    refuses — an agent worktree, say — does not count as the Routine scope
    reaching the operator's directory.

## 8. Testing

`policy.rs` **does** have an inline `#[cfg(test)] mod tests` at
`policy.rs:876-877` — the tests:

| Test | Line | Covers |
|---|---|---|
| `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local` | `policy.rs:883` | macOS prune list, `/usr/local` root promotion, `/Library` + `/Applications` *not* pruned (`#[cfg(unix)]`) |
| `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root` | `policy.rs:910` | a prune-contained exception root survives composition; ordinary nesting still folds (`#[cfg(unix)]`) |
| `linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes` | `policy.rs:931` | the four Linux pseudo-filesystem prunes, and `/usr` absent (`#[cfg(unix)]`) |
| `cli_root_wins_and_bypasses_filters` | `policy.rs:939` | CLI root over configured root over a configured `ignore`/`unignore`; asserts `ScanScope::Explicit`, `DiscoveryFilters::Bypassed`, `recency == 300` |
| `configured_root_wins_over_global` | `policy.rs:962` | configured root yields `ScanScope::Explicit` rather than Global/Routine |
| `full_resolution_ignores_a_configured_root` | `policy.rs:983` | `full: true` returns Global before `config.root` is consulted |
| `maintenance_scope_is_routine_without_a_configured_root` | `policy.rs:1011` | no configured root resolves to `ScanScope::Routine` regardless of machine seeds |
| `routine_state_errors_fall_back_to_seeds_with_one_warning` | `policy.rs:1030` | `RecoverableInvalid` and `UnsupportedNewer` both fall back to existing seeds and produce a warning |
| `automatic_roots_classify_by_provenance_and_explicit_roots_stay_strict` | `policy.rs:1058` | C028 decision table: automatic `NotFound`/non-directory omit, explicit defects stay fatal |
| `symlinked_automatic_roots_block_and_are_never_followed_or_omitted` | `policy.rs:1109` | C028: symlink-to-dir and broken-link automatic roots block (`#[cfg(unix)]`) |
| `trailing_slash_symlink_roots_classify_like_plain_symlinks` | `policy.rs:1148` | B1: `link/` and `link/.` classify exactly like a plain symlink — fatal for explicit, blocking for automatic (`#[cfg(unix)]`) |
| `admission_premise_recheck_detects_reappearance_and_disappearance` | `policy.rs:1176` | C028 late recheck: reappearance/disappearance invalidates the premise; surviving files and vanished non-directories do not |
| `full_cleanup_consumes_only_a_fresh_in_memory_generation` | `policy.rs:1335` | C028: a missing in-memory generation refuses; a fresh one yields the automatic roots plus the generation |
| `full_cleanup_gate_proves_the_roots_whose_own_traversal_completed` | `policy.rs:1225` | M018: an incomplete platform walk plus a per-root coverage verdict yields the proven roots and reports the unproven ones |
| `full_cleanup_gate_proves_every_root_when_the_platform_walk_completed` | `policy.rs:1281` | M018: a zero reconciliation code proves every learned root without a coverage verdict |
| `full_cleanup_gate_proves_nothing_when_the_coverage_was_never_measured` | `policy.rs:1303` | M018: the fail-closed arm — an unmeasured root is an unproven root |
| `cleanup_candidates_preserve_symlinked_seed_identity_for_admission` | `policy.rs:1376` | C028: raw candidates carry the link itself, and classification blocks it (`#[cfg(unix)]`) |
| `missing_transient_roots_classify_as_non_maintainable_not_unavailable` | `policy.rs:1407` | C033: a missing path at a known-transient origin omits as `NotMaintainable`, a missing durable location as `NotFound`; neither blocks |
| `omission_diagnostics_group_reasons_and_bound_path_samples` | `policy.rs:1452` | C033: one summary per omission reason, exact counts, at most three sanitized examples, bounded line length |

Scope behaviour is covered mostly **one level down**, in `discovery.rs`, by
constructing `EffectiveScanPolicy` directly rather than through `resolve` — via
the `explicit_manifests` (`discovery.rs:1652`) and `routine_manifests`
(`discovery.rs:1679`) helpers:

- `discovery_reaches_unignored_project_without_entering_ignored_sibling`
  (`discovery.rs:2085`) — the unignore `exception_below` half, negative control
  inline.
- `filters_ignored_parent_keeps_exception_route` (`discovery.rs:2138`).
- `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned`
  (`discovery.rs:2059`) — `ScanScope::Global` + `Active` filters, and
  `user_ignore_prunes` attribution.
- `non_utf8_directory_still_matches_a_broad_ignore_pattern`
  (`discovery.rs:1720`, Linux only) and
  `manifest_name_is_matched_case_insensitively_where_the_volume_is`
  (`discovery.rs:1700`).
- `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests`
  (`discovery.rs:1502`), `global_rustup_prune_keeps_adjacent_user_project_reachable`
  (`discovery.rs:1445`).

Integration: `tests/end_to_end.rs:124` is the only *integration* test that
calls `policy::resolve` end to end, and it uses a **configured** root with
`cli_root: None` (`end_to_end.rs:120-123`). `tests/cli_contract.rs:197`
asserts the JSON `scope` field equals `"explicit"`, and per §7.9 that string
comes from the resolved policy (`main.rs:1325-1331`), so the assertion does
exercise the resolved scope rather than the flags.

**Honest gaps** — scope behaviour a reader should not assume is covered:

- **The Full branch is exercised, but only for its *variant*.**
  `full_resolution_ignores_a_configured_root` (`policy.rs:983`) does construct
  `ScanRequest { full: true }`, so `resolve` takes the early return and
  `global_discovery_policy()` is called on the host platform — but the
  assertion is `matches!(p.scope, ScanScope::Global(_))`, not the root set.
  The platform *tables* are therefore still covered only through the pure
  `unix_policy` helper, and the `#[cfg(windows)]` body (`policy.rs:756-763`)
  is untested on any lane.
- `canonical_dedup_roots` and `lexical_identity` (`policy.rs:809-819`) now have
  a direct caller in a test (`policy.rs:918` and `policy.rs:925`), but the
  unresolvable-root branch (the L16 fix, `policy.rs:852`) still has no direct
  test. The `other unix` arm (`policy.rs:769-771`) and the
  `BaseDirs::new() == None` branch (`policy.rs:649-650`) are uncovered.
- The retention filter (`policy.rs:681-684`) is not directly tested:
  `retention_days == 0`, boundary equality, and the future-timestamp case. The
  only `routine_roots_from_state` test (`policy.rs:685`) exercises the
  *degraded* load arms, never the `Loaded` arm that owns the filter.
- `clean --known` root derivation (`main.rs:878-1064`), including the
  configured-root interaction in §7.8, has no test in `src/policy.rs`.
  `main.rs` has four inline tests, none of which reach root derivation; the
  nearest coverage is
  `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one`
  (`tests/cli_contract.rs:2464`) on the *scan* side and
  `bare_cleanup_with_no_known_roots_is_a_successful_no_op`
  (`tests/cli_contract.rs:1902`) on the empty-scope side.

## 9. Review checklist

1. **The `full` early-return is still first.** `policy.rs:952-961` must return
   before `policy.rs:30` is reached. Anything that reorders it changes what
   `--full` means.
2. **An explicit root still bypasses filters, not merely outranks config.**
   `policy.rs:42` must keep `DiscoveryFilters::Bypassed`; `discovery.rs:361`
   turns that into empty ignore/unignore slices.
3. **Every new `GlobalDiscoveryPolicy` field needs a `#[cfg]`-paired
   definition.** The two `global_discovery_policy` bodies are
   `policy.rs:717` (`#[cfg(unix)]`) and `policy.rs:780` (`#[cfg(windows)]`); a
   field added to one and not the other is a platform-specific compile error
   that only one CI job will catch.
4. **New seed names must be absolute-joinable and case variants must be
   deliberate.** `policy.rs:61-75`; the pairs exist because of volume
   case-sensitivity (`discovery.rs:108-125`).
5. **A new retention rule must fail conservative.** `policy.rs:681-684` keeps
   the root when `retention_days == 0` or the timestamp is in the future; ADR
   002 §5 requires the same for any new expiry path.
6. **Do not add a containment check to learned roots here without reading
   `discovery_state.rs:415` first** — `is_broad_root` already exists at
   derivation time; `policy.rs:686` deliberately consumes learned paths
   verbatim.
7. **A `u16`/`u32` or range assumption must be traced to `config::load`.**
   `policy.rs:46` narrows `u32 → u16` on the strength of
   `config.rs:107-111`; `policy.rs:22` relies on `config.rs:99-103` for the
   non-zero recency guard. Test-constructed `ScanConfig` values bypass both.
8. **Any new `AppError` here becomes exit code 2.** `policy.rs:32` and
   `policy.rs:37` are the only sources today in `resolve`; `main.rs:21-37`
   maps every `AppError` to exit 2, and a missing `scan.root` is therefore
   fatal rather than degrading to Routine. The admission seam adds
   `policy.rs:259` and `policy.rs:317`, and the Full gate adds
   `policy.rs:576-582` — the *intended* exit-2 refusal, not a defect. The
   removed `policy.rs:524-527` refusal is gone by decision, not by accident:
   see §5.
9. **Scope changes are safety changes for `clean --known`.** `main.rs:878-1064`
   and `main.rs:426-456` turn this module's output into the bounded root list
   a destructive command receives; re-verify §7.8 and §7.10 before landing.
10. **Do not widen the Full gate by accident.** `policy.rs:588-613` is the
    ADR 008 §5b route 5b-i rule: the proof boundary is the learned root, and a
    root whose own traversal did not complete is reported, never a candidate.
    A root with no coverage entry at all is unproven (`policy.rs:600`) — the
    fail-closed arm. Changing any of this needs a decision record, not a patch.
11. **Add a test for any branch you touch.** The Windows body
    (`policy.rs:779-803`), the `other unix` arm (`policy.rs:769-771`), the
    unresolvable-root branch (`policy.rs:852`) and the retention filter
    (`policy.rs:681-684`) are currently untested; use `unix_policy`'s injected
    booleans (`policy.rs:756`) for platform tables and
    `routine_roots_from_state`'s injected `StateLoad`
    (`policy.rs:663-666`) for state behaviour — those are the two seams the
    module already provides.
