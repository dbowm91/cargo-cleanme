# Policy & Scope — deciding what to look at

> Component deep dive · part of the [architecture overview](overview.md)

`src/policy.rs` — 1,623 lines, 838 of them production; the inline test
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

**Owns:** scope-mode selection and its precedence (`policy.rs:20-59`); the
per-scope recency window (`:23`, `:55`); per-scope filter activation (`:25-28`,
`:43`, `:48-51`); existence / real-directory validation of an explicit root
(`:33-42`); the provenance vocabulary — `RootProvenance`, `AutomaticOmission`,
`AutomaticBlock`, `ClassifiedRoots` (`:98-142`); the admission classifier
(`:220-355`); the late premise recheck (`:357-480`); the Full cleanup verdict and gate — `FullCleanupVerdict`,
`full_cleanup_roots`, `RootVerdict` (`:493-602`); the raw Routine cleanup
candidates (`:613-643`); Routine root assembly (`:645-690`); platform global
root and prune enumeration (`:691-778`); root normalization, dedup and
containment collapse (`:780-831`); the rustup managed-tool prune list
(`:833-837`).

**Does not own:** filesystem traversal (`discovery.rs`), manifest → workspace
resolution (`workspace.rs`), activity classification (`traverse.rs`), state
reconciliation and atomic publish (`discovery_state.rs`), the cleanup root list
(`main.rs:934-1120`), the per-root coverage *measurement* that feeds the Full
gate (`discovery.rs:551-601`), and the user-facing scope *label*
(`main.rs:1455-1470`).

**Actual dependency set.** A grep for `crate::` in `policy.rs` yields exactly
five modules:

| Dependency | Used at | For |
|---|---|---|
| `crate::config::ScanConfig` | `policy.rs:2`, `policy.rs:20` | input config |
| `crate::domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope}` | `policy.rs:3` | in/out vocabulary |
| `crate::domain::{RootCoverage, UnprovenRoot}` | `policy.rs:497`, `:549`, `:580` | M018's per-root coverage verdict and the unproven-root report |
| `crate::error::AppError` | `policy.rs:4` | the only error type returned |
| `crate::discovery_state` | `policy.rs:253`, `:299`, `:389`, `:548`, `:616`, `:624`, `:625`, `:652`, `:663`, `:670`, `:671`, `:677` | `load_default`, `StateLoad`, `now_seconds`, `DiscoveryState`, `automatic_root_is_maintainable` |
| `crate::discovery::effective_rustup_home` | `policy.rs:834` | managed-tool prune list |

Plus the `directories` crate (`policy.rs:221`, `:375`, `:646`) and, on Windows only,
`windows_sys` (`policy.rs:754-778`).

**Boundary with `discovery.rs` prune helpers.** `discovery.rs` owns the
*environment-derived* prune sets: `effective_cargo_home` (`discovery.rs:169`),
`effective_rustup_home` (`discovery.rs:150`) and `cargo_home_prunes`
(`discovery.rs:181`, i.e. `$CARGO_HOME/registry` and `$CARGO_HOME/git`).
`policy.rs` owns only the *platform* and global-only prune sets. The division
is by scope, not by subject: `cargo_home_prunes()` applies to **every** scope
(`discovery.rs:369`) because a registry tree must never trigger Cargo
resolution even under an explicit root, whereas the rustup list applies
**only** to `Global` or `Routine` (`discovery.rs:370-376`) and is sourced from
`policy::global_discovery_policy().managed_tool_prunes`, populated by
`managed_rust_prunes()` (`policy.rs:833`). `policy.rs` reaches into
`discovery` exactly once (`policy.rs:834`), for a single path; everything else
flows the other way.

**I/O: it does perform I/O, read-only.** The module is a *decision* module, not a
*pure* one. Its scan-side logic is a total function of `(request, config)` for
the Full and Explicit branches; the Routine branch additionally consults the
filesystem, the process environment, the wall clock, and the persisted state
file. The admission seam reads metadata for every candidate root. All of these
calls are reads:

| Call | Location | Reads |
|---|---|---|
| `fs::symlink_metadata` | `policy.rs:33` | explicit root stat (resolve) |
| `fs::symlink_metadata` | `traverse.rs:48`, `:55` | symlink identity, trailing separators stripped — the shared `path_is_symlink` predicate, imported at `policy.rs:5` |
| `fs::symlink_metadata` | `policy.rs:231` | admission classification |
| `fs::symlink_metadata` | `policy.rs:387`, `:448` | late premise recheck |
| `fs::symlink_metadata` | `policy.rs:620` | raw seed existence (cleanup candidates) |
| `fs::read_dir` | `policy.rs:255` | coverage proof for a non-maintainable root |
| `Path::is_dir` (seeds) | `policy.rs:610` | seed existence |
| `directories::BaseDirs::new()` | `policy.rs:221`, `:375`, `:646` | env / platform home |
| `discovery_state::load_default()` | `policy.rs:652` | state JSON file |
| `discovery_state::now_seconds()` | `policy.rs:625`, `:671` | wall clock |
| `Path::new("/usr/local").is_dir()` | `policy.rs:696` | macOS filesystem |
| `GetLogicalDrives` / `GetDriveTypeW` | `policy.rs:757`, `:769` | Windows volumes |
| `fs::canonicalize` | `policy.rs:562`, `traverse.rs:144` | root identity |
| `discovery::effective_rustup_home` | `policy.rs:834` | `RUSTUP_HOME`, home |
| `eprintln!` | `policy.rs:655` | one stderr warning line |

It never writes to disk and never spawns a process. The single `eprintln!` at
`policy.rs:655` is the module's only side effect, and it exists so a degraded
Routine root set is never silent.

## 2. The scope decision

```rust
pub fn resolve(request: ScanRequest, config: &ScanConfig)
    -> Result<EffectiveScanPolicy, AppError>          // policy.rs:19
```

### Precedence, in code order

1. **`request.full == true` → `Global`, unconditionally** (`policy.rs:21-30`).
   The function returns before `request.cli_root` and `config.root` are ever
   read. `--full` therefore beats *both* a CLI root and a configured
   `scan.root`: if all three are set, Global wins.
2. **Otherwise `request.cli_root` beats `config.root`**
   (`policy.rs:31`: `request.cli_root.or_else(|| config.root.clone())`).
3. **If a root survived step 2 → `Explicit(root)`** (`policy.rs:43`), with
   `DiscoveryFilters::Bypassed`.
4. **Otherwise → `Routine(routine_roots(...))`** (`policy.rs:47`), with
   `DiscoveryFilters::Active`.

The specific case of an explicit CLI root *and* `--full` *and* a configured
`scan.root` resolves to **`ScanScope::Global`**, because the `full` early-return
at `policy.rs:21` precedes the `or_else` at `policy.rs:31`. That combination
is unreachable from the command line: `scan` requires exactly one of ROOT /
`--known` / `--all` through an `ArgGroup` (`cli.rs:244`), so `scan --all` and
`scan ROOT` are mutually exclusive, and the hidden `clean` alias carries
`conflicts_with_all = ["root", "full"]` on `--known` (`cli.rs:290`), while the
root-level `--full` carries `conflicts_with = "root"` (`cli.rs:51`). No in-repo
caller constructs the conflicting pair either — `tests/end_to_end.rs:124` and
`examples/traversal-qualification.rs:26` both pass `cli_root: None`. The
precedence therefore matters only for a future programmatic caller.

Note the asymmetry with cleanup, which is decided in `cli.rs` rather than here:
for `clean`, `--full` wins over a ROOT argument, because the merged scope match
tests `full` first (`cli.rs:576-611`) — although clap has already refused the
pair outright.

### Fields populated on `EffectiveScanPolicy`

| Field | Full (`policy.rs:21-30`) | Explicit (`policy.rs:43`) | Routine (`policy.rs:44-53`) |
|---|---|---|---|
| `recency` | `config.recency_seconds` | same | same |
| `scope` | `Global(global_discovery_policy().roots)` | `Explicit(cli_root \| config.root)` | `Routine(routine_roots(retention_days))` |
| `discovery_filters` | `Active { ignore, unignore }` from config | `Bypassed` | `Active { ignore, unignore }` from config |

All three paths share one construction site (`policy.rs:54-58`). `recency` is
scope-independent — it is `Duration::from_secs(config.recency_seconds)` on
every path (`policy.rs:23`, `:55`). The type is
`domain::EffectiveScanPolicy` (`domain.rs:27-32`); `recency` is captured at
`main.rs:1591` and turned into the activity cutoff at `main.rs:1788`, not by
the walker.

### Errors returned

`resolve` returns exactly one error variant, from two sites, both in the
Explicit branch:

| Site | Condition | Message |
|---|---|---|
| `policy.rs:33-36` | `fs::symlink_metadata` fails | `AppError::InvalidRoot { path, reason: e.to_string() }` — includes the OS error (missing, permission denied) |
| `policy.rs:37-42` | not a directory, **or** is a symlink | `AppError::InvalidRoot { path, reason: "expected a real directory" }` |

The Full and Routine branches of `resolve` are infallible. A Routine scope
with no available roots returns `Ok(ScanScope::Routine(vec![]))`, never an
error; the user-facing signal is a diagnostic emitted later by
`discovery.rs:384-391` and the one stderr line at `policy.rs:655`.

The **module** as a whole has three more error sites, all on the cleanup side:

| Site | Condition | Message |
|---|---|---|
| `policy.rs:234-237` | an *explicit* admission candidate is not a real directory | `AppError::InvalidRoot { …, reason: "expected a real directory" }` |
| `policy.rs:292-295` | an *explicit* admission candidate cannot be stat'ed | `AppError::InvalidRoot { …, reason: e.to_string() }` |
| `policy.rs:551-557` | the Full reconciliation produced no fresh in-memory generation | `AppError::Config("full reconciliation produced no fresh generation; no cleanup commands were run against stale learned roots")` |

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
| `Explicit(PathBuf)` | yes, `policy.rs:43` | one user-named root |
| `ExplicitRoots(Vec<PathBuf>)` | **no** | multiple explicit roots forming one cleanup boundary; constructed only in `cleanup.rs:1741`, `cleanup.rs:2297` and `cleanup.rs:3450` |
| `Global(Vec<PathBuf>)` | yes, `policy.rs:24` | exhaustive platform roots |
| `Routine(Vec<PathBuf>)` | yes, `policy.rs:47` | bounded adaptive roots |

Which spelling reaches which scope is decided in `cli.rs` and `main.rs`, not
here: `scan ROOT` → Explicit, `scan --known` → Routine, `scan --all` → Global
(`cli.rs:475-490`, `main.rs:86-93`); `cargo cleanme ROOT` → Explicit,
`cargo cleanme --full` → Global, and `clean --known` plus the bare front door →
the maintenance scope (`cli.rs:463-474`, `cli.rs:576-611`), which resolves to
Routine **or** to Explicit when a `scan.root` is configured (§7.8).

### `Explicit(root)` — the user named a place

One directory, no `scan.ignore` / `scan.unignore` at all
(`DiscoveryFilters::Bypassed`, `policy.rs:43`). Worked example: `scan ~/work/api`
walks `~/work/api` recursively and reports every `Cargo.toml` beneath it, even
under a directory the user has globally ignored in `config.toml`. It still
enforces the symlink and real-directory rule (`policy.rs:37-42`), and it is the
only branch of `resolve` that can fail. A symlink *to* a directory is rejected.

### `Routine(roots)` — the maintenance scope, bounded

The Routine *scan* is `scan --known`
(`main.rs:86-93` → `run_scan(ScanIntent::Maintenance, …)`); the Routine *cleanup* is the
bare front door and `clean --known`, which resolve their scope through
`policy::resolve` at `main.rs:1067-1074`. Roots are existing seed
directories under `$HOME` plus retained learned roots from the state file (§5).
Worked example: a developer with `~/Code/api` and `~/Code/web` plus a learned
root at `~/scratch/lab`, where the state file's `last_project_seen_at` for
`~/scratch/lab` is 10 days old and retention is 30.
`canonical_dedup_roots` (`policy.rs:782`) collapses nothing here, so the scan
walks all three, applying `scan.ignore` / `scan.unignore` normally. A Rust
project in `~/Documents/sidequest` is **not** found — that is the deliberate
blind spot, and `scan --all` is the documented way to teach the tool about it.

### `Global(roots)` — `--all`

`ScanScope::Global(global_discovery_policy().roots)` (`policy.rs:24`). Worked
example on Linux: roots is `["/"]` and the platform prunes are
`/proc`, `/sys`, `/dev`, `/run` (`policy.rs:740-743`), so the walk is
filesystem-root breadth minus pseudo-filesystems. `scan.ignore` / `scan.unignore`
apply. This is also the only scope that runs
`discovery_state::reconcile_full` afterwards (`main.rs:1704`, reached only when
`full` is true — `main.rs:1557-1558`).

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
(`main.rs:1494`), described in [the overview's scan pipeline](overview.md) §6 as
"Resolve scope — `policy::resolve` turns `{cli_root, full}` into concrete roots
+ a recency window".

## 4. Pruning inputs

`DiscoveryFilters` (`domain.rs:19-26`) has two states, and `resolve` picks
between them — it never merges them:

| State | Set by | Effect in `discovery.rs` |
|---|---|---|
| `Active { ignore: Vec<String>, unignore: Vec<PathBuf> }` | Full (`policy.rs:25-28`) and Routine (`policy.rs:48-51`) | `Filters::new(ignore, unignore)` at `discovery.rs:368` |
| `Bypassed` | Explicit (`policy.rs:43`) | `(&[][..], &[][..])` at `discovery.rs:366` |

`config.ignore` and `config.unignore` are cloned verbatim from `ScanConfig`
(`config.rs:47-48`); `resolve` adds no patterns of its own.

### Semantics

- **ignore globs prune, and the exclusion is inherited.** Each `scan.ignore`
  string is compiled into a `globset::GlobSet` (`discovery.rs:23-35`) and matched
  against `path.to_string_lossy()` (`discovery.rs:43-48`). The lossy match is
  deliberate: a non-UTF-8 directory name must not escape a broad `*` rule on
  Linux (comment at `discovery.rs:36-42`, test
  `non_utf8_directory_still_matches_a_broad_ignore_pattern` at
  `discovery.rs:1699`). The match is also consulted for every **ancestor** of the
  candidate (`Filters::inherits_ignore`, `discovery.rs:57-60`), because a literal
  ignore rule names a directory, not its children: `/archive` is not a match for
  `/archive/other`, but `other` is still under an ignored directory and must
  stay excluded. This is the C019 correction, and the walk descends into
  `/archive` only as far as an exception requires.
- **`unignore` re-includes a subtree, not a single entry.** One function decides
  all of it: `Filters::disposition` (`discovery.rs:73-92`) returns
  `Included` / `Pruned` / `PassThrough` / `ReIncluded` (enum at
  `discovery.rs:107-112`). `ReIncluded` applies when
  `p.starts_with(u)` for some unignore entry, so the unignore path *and everything
  beneath it* escapes the prune. `PassThrough` is the other half: excluded, but
  with an unignore entry at or below it, so the walker enters far enough to reach
  the   exception — and only that route. Tests:
  `filters_ignored_parent_keeps_exception_route` (`discovery.rs:2116`) for the
  `/*` shape, `literal_ignored_ancestor_becomes_pass_through_and_not_inherited_by_siblings`
  (`discovery.rs:2146`) for the literal shape, and
  `literal_ignored_ancestor_does_not_readmit_ignored_siblings`
  (`discovery.rs:2221`) for the same case through a real walk.
- **a CLI scan root takes precedence over configured scope.** Verified at
  `policy.rs:31`. The stronger form is what the code actually does: an explicit
  root does not merely win the root slot, it switches filters to `Bypassed`, so
  configured `scan.ignore` / `scan.unignore` do not apply to it at all. The
  inline test helper comment states the same intent
  (`discovery.rs:1629-1630`).

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
(`discovery.rs:24-32`) so the per-directory prune test is a set lookup rather
than a re-parse. The unignore side uses no globbing at all — it is exact
`Path::starts_with` prefix comparison on absolute `PathBuf`s, which is why
`config.toml:27` calls them "Exact absolute directories".

**The template and the code now agree.** `config.toml:21-24` says these
filters "apply to global discovery and to the Routine scope", which is exactly
what the code does: `resolve` activates them for both Full (`policy.rs:20-59`)
and Routine (`policy.rs:48-51`), and `discovery.rs` hands the compiled filter
set to the global branch (`discovery.rs:400`) and to the per-root branch
(`discovery.rs:432`) alike. The earlier divergence — a template that claimed
"global discovery" only while the code also applied the filters to Routine —
has been closed.

## 5. Seed candidates

```rust
pub fn routine_seed_candidates(home: &Path) -> Vec<PathBuf>   // policy.rs:60
```

Returns `home.join(name)` for this exact ordered list of 13 names
(`policy.rs:62-76`):

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
`is_manifest_name` (`discovery.rs:113-130`).

**This function is pure and does no filtering.** It is a literal `join` over a
fixed array: no existence check, no config read, no env read, no state read.
Existence filtering happens one level up, in `routine_roots_from_state`
(`policy.rs:645`: `.filter(|p| p.is_dir())`).

**Who supplies `home`:** `routine_roots` (`policy.rs:645-684`) builds it itself
from `directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())` — the
same crate and accessor used by `discovery.rs:153` and `discovery.rs:177`, but
obtained independently of `main.rs`. `policy::resolve` cannot inject a home
directory; `routine_seed_candidates` is the only seam, and it exists so the list
can be asserted without touching the real home directory. If
`BaseDirs::new()` returns `None`, `routine_roots` returns an empty `Vec`
(`policy.rs:646-648`) and Routine silently degrades to "no roots", which
`discovery.rs:383-392` reports as `Info` / `PlatformRoot`.

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
the call sites are `policy.rs:618` (`routine_cleanup_candidates`) and
`policy.rs:665` (`routine_roots_from_state`). It is public surface without an external
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
(`policy.rs:655-659`)

`retention_days == 0` disables expiration and a future timestamp fails
conservative — both match ADR 002 §5. A `StateLoad` that is not `Loaded`
(`Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable`) yields
`load.diagnostic()` as a warning and **keeps the seeds** (`policy.rs:669-689`),
matching ADR 002 §7's fail-soft rule for a corrupt state file.

### Cleanup admission is provenance-aware, scan assembly is not (C028)

`resolve` above decides what a *scan* walks. Cleanup admission is a separate
seam that runs before any collapse, because collapsing first would resolve a
symlinked root into its target and hide the invalid identity:

- `RootProvenance` (`policy.rs:98`) labels each candidate `Explicit`,
  `Automatic`, or `Operator`; `ClassifiedRoots` (`policy.rs:138`) carries the
  admitted, safely omitted (`AutomaticOmission::NotFound` / `NonDirectory` /
  `NotMaintainable` / `OperatorUnavailable` / `OperatorNonDirectory`,
  `policy.rs:115`), and blocking (`AutomaticBlock`, `policy.rs:131`) sets.
- `classify_cleanup_roots` (`policy.rs:220`) uses `symlink_metadata` and the
  real `io::ErrorKind` — never `Path::exists`/`is_dir`, which conflate errors
  and follow symlinks — and has no catch-all `Err(_) => continue`. A missing
  automatic root is checked for provenance *before* availability: a missing
  path at a known-transient origin omits as `NotMaintainable`, a missing
  durable location omits as `NotFound`; symlinks never reach that arm and
  every other failure still blocks.
- `path_is_symlink` (`src/traverse.rs:47-59`) closes the trailing-separator
  bypass: `symlink_metadata("link/")` follows the final component on POSIX, so
  a trailing slash (or a trailing `/.`) would otherwise hide a symlink from the
  `is_symlink` check. The stripped spelling is judged too.
- `routine_cleanup_candidates` (`policy.rs:613`) supplies the raw Routine
  candidates (existing seeds in any form plus age-eligible learned roots)
  without canonicalization, so a symlinked seed reaches classification as a
  link. `full_cleanup_roots` (`policy.rs:546`) supplies the Full roots from
  the in-memory generation the reconciliation just proved, or refuses.
- `recheck_admission_premise` (`policy.rs:371`) re-examines the premise with
  fresh metadata before the first final proof and before every later spawn;
  `CleanupCandidates` (`policy.rs:484`) is the shared candidate type.

Omission diagnostics are grouped by typed reason and sorted deterministically
(`ClassifiedRoots::omission_diagnostics`, `policy.rs:167-211`). Each summary
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

`RootProvenance::Operator` (`policy.rs:270`) is the third variant, added by
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
`policy.rs:120-122`) rather than reusing `NotFound` / `NonDirectory`. Two
things depend on that. The pre-spawn recheck reads the variant to know which
premise must still hold; and the diagnostic reads it to name the root
truthfully. An operator root was never a *learned* one, and saying it was
would misreport what the tool did.

`omission_diagnostics` groups by `(reason, noun)` (`policy.rs:167-211`), so
the published "learned root(s)" wording is byte-identical to what it was
before ADR 009 and an operator root's omission reads "operator root(s)". A
closed record's quoted message cannot be invalidated by adding a bucket.

### The Full cleanup gate is a *verdict*, not a refusal (ADR 008 §5b, M018)

`full_cleanup_roots` (`policy.rs:546-593`) is the only place where a scan
result gates a destructive operation. It no longer refuses on a non-zero
reconciliation code; it returns a typed verdict:

```rust
pub struct FullCleanupVerdict {          // policy.rs:518-525
    pub proven: CleanupCandidates,
    pub unproven: Vec<crate::domain::UnprovenRoot>,
    pub generation: Option<u64>,
}
```

Three inputs decide it (`policy.rs:546-550`): the reconciliation exit code, the
in-memory state the traversal just proved, and the per-root coverage verdict.

- **A missing fresh generation refuses** (`policy.rs:551-557`):
  `AppError::Config("full reconciliation produced no fresh generation; …")`,
  which becomes exit code 2 through `main.rs:21-37`. The roots come from the
  generation the traversal just proved, never from a disk reload that could be
  an older generation after a failed or unsupported publication. A *failed
  save* is not a refusal — it stays a stderr warning for the caller
  (`main.rs:982-986`).
- **A non-zero reconciliation code no longer refuses.** The platform walk's
  incompleteness is not evidence that *every* learned root is unproven — it is
  evidence that the platform walk did not finish. When the code is zero the
  platform walk read every learned root's subtree, so every root is proven by
  definition and no measurement is needed (`policy.rs:563-566`). When it is
  non-zero the root is judged on its own coverage entry
  (`policy.rs:567-577`): an entry with `complete == true` is proven, an
  incomplete entry is unproven *with that entry's detail*, and a root with no
  coverage entry at all is unproven with no detail — an unmeasured root is an
  unproven root.
- **A missing coverage verdict fails closed.** `coverage` is `None` only when
  the platform walk completed. When the walk was incomplete it *must* carry the
  verification result; `coverage.and_then(…)` yields `None` for a root that is
  absent from the slice, and that root is unproven
  (`policy.rs:575`). The contract is stated in the function's own doc comment
  (`policy.rs:538-545`).

The proven roots carry `RootProvenance::Automatic` (`policy.rs:579`), so the
shared admission path applies to them exactly as it does to every other
automatic root. The unproven roots carry a typed reason:
`crate::domain::UnprovenRoot { root, detail }` (`policy.rs:580-587`), with the
default detail naming the reason the root could not be measured
(`policy.rs:582-586`).

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
(`policy.rs:1188`) — an incomplete platform walk with a per-root verdict
yields the proven roots and reports the unproven ones;
`full_cleanup_gate_proves_every_root_when_the_platform_walk_completed`
(`policy.rs:1244`) — a zero exit code proves every learned root without a
coverage verdict; and
`full_cleanup_gate_proves_nothing_when_the_coverage_was_never_measured`
(`policy.rs:1266`) — the fail-closed arm.
`full_cleanup_consumes_only_a_fresh_in_memory_generation`
(`policy.rs:1298`) still pins the C028 property: a missing in-memory
generation refuses (`policy.rs:551-557`), while a present one yields the
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
| `policy.rs:691-698` | `#[cfg(unix)]` | delegates to `unix_policy(...)`, then hands the result to `compose_global_policy` (`policy.rs:707-718`), which fills `roots` through `canonical_dedup_roots_protecting` and `managed_tool_prunes` through `managed_rust_prunes()` |
| `policy.rs:754-778` | `#[cfg(windows)]` | enumerates drive letters with `GetLogicalDrives` / `GetDriveTypeW` |

**They are not duplicates and neither is dead.** They are mutually exclusive
`#[cfg]` variants of the same public function: exactly one is compiled per
platform, and a name collision never occurs. On any other target neither
exists, and the `resolve` call at `policy.rs:20` would not compile — which is
consistent with the crate shipping a single binary for unix and Windows only.

A third, different function shares the family: `unix_policy`
(`policy.rs:730-752`, `#[cfg(unix)]`, private) is a *pure* helper whose three
booleans are supplied from `cfg!` plus one filesystem probe
(`policy.rs:671-674`). It exists so the macOS and Linux tables can be unit
tested on any host. It always returns `managed_tool_prunes: Vec::new()`
(`policy.rs:747-750`); the `#[cfg(unix)]` wrapper supplies that field
(`policy.rs:692-697`).

### Platform tables

| Platform | `roots` | `global_only_prunes` | Source |
|---|---|---|---|
| macOS | `["/"]`, plus `/usr/local` when that directory exists | `/System`, `/dev`, `/bin`, `/sbin`, `/usr` | `policy.rs:733-739` |
| Linux | `["/"]` | `/proc`, `/sys`, `/dev`, `/run` | `policy.rs:740-743` |
| Other unix | `["/"]` | none | `policy.rs:744-746` |
| Windows | `X:\` for each letter where `GetLogicalDrives` reports a bit and `GetDriveTypeW` is `DRIVE_FIXED` or `DRIVE_REMOVABLE` | none | `policy.rs:754-778` |

The macOS row is the interesting one: `/usr` is pruned while `/usr/local` is
enumerated as a *root*, so the Homebrew toolchain subtree stays reachable
inside an otherwise-pruned `/usr`. That exception takes **two** steps to work,
and both were once missing:

1. **Composition.** `global_discovery_policy` (`policy.rs:692-718`) delegates to
   `compose_global_policy` (`policy.rs:707-718`), which collapses redundant
   roots — and the collapse folds `/usr/local` into `/`, because
   `"/usr/local".starts_with("/")`. `compose_global_policy` therefore computes
   the *protected* set first: every root sitting strictly inside a global-only
   prune is that prune's exception, so `canonical_dedup_roots_protecting`
   (`policy.rs:801-818`) keeps it as a walk root of its own.
2. **Application.** A root that survived composition would still be unwalked:
   `entry_is_within` matches on `parent_path`, so with root `/usr/local` every
   entry's parent `starts_with("/usr")` and the subtree read as pruned.
   `discovery::entry_is_system_pruned` (`discovery.rs:284-297`) exempts a walk
   root strictly inside a system prune. The exemption is scoped to *global-only*
   prunes on purpose: a Cargo or rustup home stays refused even when a root sits
   inside it.

Test `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local`
(`policy.rs:846`) pins this — and it now calls `compose_global_policy`, so it
asserts the roots **production** walks rather than the ones a private
pre-collapse helper declares. That distinction was the defect: the old test
called `unix_policy` directly and passed while production collapsed the root
away. `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root`
(`policy.rs:873`) covers the collapse alone, including that an ordinary nested
root is still folded; `a_root_inside_a_global_prune_reopens_that_prune`
(`discovery.rs:1249`) covers the prune. `managed_tool_prunes` is `Some` only via
`managed_rust_prunes()` → `discovery::effective_rustup_home()`
(`policy.rs:833-837`), i.e. `$RUSTUP_HOME` when absolute, else `~/.rustup`
(`discovery.rs:150-166`).

### Consumers

| Consumer | Behaviour |
|---|---|
| `discovery.rs:347-348` | re-derives the policy for `Global` scope only |
| `discovery.rs:349-356` | **adopts it only if `canonical_roots(roots) == canonical_roots(&candidate.roots)`**; otherwise treats the scope as a plain root list with no platform prunes |
| `discovery.rs:360` | when adopted, walks `p.roots` — which is why the `roots` carried in `ScanScope::Global` and in `GlobalDiscoveryPolicy` are kept in agreement |
| `discovery.rs:370` | `managed_tool_prunes`, gated on `global` (Global **or** Routine) |
| `discovery.rs:403` | `global_only_prunes`, passed only to `discover_global_roots` |
| `examples/traversal-qualification.rs:50` | reads `.roots` for the standalone qualification harness |

The equality guard at `discovery.rs:349-356` is why a *caller-constructed*
`ScanScope::Global` (as several `discovery.rs` tests do) does **not** silently
acquire platform prunes it never asked for.

### Root normalization

`canonical_dedup_roots` (`policy.rs:782-784`) is the shared normalizer for all
three roots lists; it delegates to `traverse::collapse_outermost`
(`src/traverse.rs:189-191`), which resolves each root through
`traverse::resolve_root` (`src/traverse.rs:155-...`):

1. For each root, try `fs::canonicalize`. Success → identity is the canonical
   path *and* the retained spelling is canonical. Failure → identity is
   `lexical_identity` of the root's physical location as far as the filesystem
   allows, but the retained spelling stays as the caller wrote it (so an
   unavailable root still names what the user asked for).
2. Sort by spelling and `dedup_by` spelling equality.
3. Drop any root whose identity `starts_with` an already-kept root's identity
   (containment collapse).

`lexical_identity` (`src/traverse.rs:61-83`) resolves `.` and `..` textually and
never touches the filesystem; its doc comment states that a path which does not
exist "still has to be comparable with its siblings". The rationale comment at
`policy.rs:800-805` records the bug it fixed (L16): an unresolvable root
previously kept its raw spelling and so was neither deduped nor collapsed,
causing an overlapping subtree to be scanned twice.

## 7. Invariants and edge cases

1. **`full` beats everything.** The `full` early-return (`policy.rs:21-30`) sits
   above every root consideration. Reordering it would silently convert
   `--full` into a single-root scan.
2. **An explicit root wins monotonically.** `policy.rs:31` is a single
   `or_else`; no branch lets a configured root displace a CLI root, so adding
   roots to `ScanConfig` can never override a CLI root.
3. **Explicit scope bypasses filters — completely.** Not "precedence": a
   configured `scan.ignore` has no effect at all under `ScanScope::Explicit`
   (`policy.rs:43`). Pinned by `cli_root_wins_and_bypasses_filters`
   (`policy.rs:902`).
4. **A Routine root is *not* guaranteed to be under `$HOME` by this module.**
    Seeds are (`policy.rs:78`), but learned roots are taken verbatim from
    `state.learned_roots[].path` (`policy.rs:673-683`) with a recency test and a
    maintainability test (`policy.rs:676-682`) and nothing else — no containment
    check is performed here. The "no broad/platform roots are learned"
    guarantee is a *derivation-time* rule in `discovery_state.rs`
    (`is_broad_root` at `discovery_state.rs:417`, the home-equality filter at
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
   `Ok(Routine(vec![]))`; `discovery.rs:384-391` emits `Info` / `PlatformRoot`
   "no Routine roots are available; run `scan --full` or scan an explicit root".
7. **`learned_root_retention_days as u16` (`policy.rs:47`) is lossless only
   because of the caller's validation.** `config::load` range-checks it to
   0–3650 (`config.rs:107-111`), which fits in `u16`; a `ScanConfig` built
   directly — as every test does — bypasses that check, and a value above 65535
   would wrap silently. Same class of issue for `recency_seconds`: `load`
   rejects 0 (`config.rs:99-103`) because 0 would disable the inactivity guard,
   but `resolve` does not (`policy.rs:20`, `:20`).
8. **A configured `scan.root` redirects the maintenance scope; it does not
   disable it.** `clean --known` and the bare front door resolve their scope
   through `policy::resolve` with `cli_root: None, full: false`
   (`main.rs:1023-1029`), and the resulting `ScanScope::Explicit` arm is handled
   explicitly (`main.rs:1034-1055`): the configured root becomes the scope that
   gets cleaned, and the machine label is `"explicit"`. `ScanScope::Routine`
   (`main.rs:1074-1109`) is the no-configured-root case, and `ScanScope::Global`
   is refused rather than treated as an empty scope (`main.rs:1112-1116`).
   Setting `scan.root` is therefore not orthogonal to `--known`: it changes
   what `--known` cleans.
9. **The JSON `scope` field is derived from the resolved policy, not from the
   CLI flags** (`main.rs:1455-1470`): `Global` → `"full"`, `Explicit` and
   `ExplicitRoots` → `"explicit"`, `Routine` → `"routine"`. The function's own
   doc comment records the earlier defect — deriving it from `root.is_some()`
   reported `"routine"` for a scan that was in fact pinned to one explicit root
   (`main.rs:1504-1510`). The contract test is
   `json_scope_label_follows_the_resolved_scope_not_the_cli_flags`
   (`tests/cli_contract.rs:214`), and `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one`
   (`tests/cli_contract.rs:2535`) pins the same label for `scan --known` with
   and without a configured root.
10. **The safety-envelope coupling.** `clean --known` routes its cleanup roots
    through `policy::resolve` (`main.rs:1068-1074`), so any change to scope resolution
    changes the set of roots a destructive command may operate on. A scope
    change is a safety change and belongs in the same review as the proof work
    in [the cleanup deep dive](09-cleanup.md).
11. **State degradation is visible but not fatal.** A corrupt or newer-schema
    state file degrades Routine to seeds plus one stderr line
    (`policy.rs:632-633`, `policy.rs:647-667`); it never expands scope and never
    fails the run.
12. **The Full cleanup's proof boundary is the learned root.** Since M018,
    `full_cleanup_roots` (`policy.rs:546-593`) no longer refuses an incomplete
    Full reconciliation: it returns a `FullCleanupVerdict`
    (`policy.rs:493-500`) whose `proven` roots are candidates and whose
    `unproven` roots are reported, never candidates. The only refusal left is
    a missing fresh generation (`policy.rs:551-557`), and a root whose coverage
    was never measured is unproven (`policy.rs:575`). Every other scope is
    proof-scoped under ADR 007: an out-of-boundary unprovable participant is
    reported, never blocking. A change to this gate is a change to the safety
    boundary, not a refactor.
13. **The operator's roots are added to the Routine scope, never substituted
    for it.** ADR 009's working directory joins the maintenance scope through
    `admit_automatic` (`main.rs:1199-1288`), which classifies the Routine
    candidates first, asks `working_directory_is_unreached` (`main.rs:1178-1185`)
    whether the Routine scope already reaches the operator's directory, and
    only then classifies the operator's candidates and *extends* the admitted
    set (`main.rs:1221-1224`). The universe may only ever grow, so a wider
    universe can only reclassify `shared` bytes to `private` — never the
    reverse, which is ADR 006's shared-output hazard. The decision is made
    against the **admitted** roots, so a root the maintainability rule
    refuses — an agent worktree, say — does not count as the Routine scope
    reaching the operator's directory.

## 8. Testing

`policy.rs` **does** have an inline `#[cfg(test)] mod tests` at
`policy.rs:839-840` — the tests:

| Test | Line | Covers |
|---|---|---|
| `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local` | `policy.rs:846` | macOS prune list, `/usr/local` root promotion, `/Library` + `/Applications` *not* pruned (`#[cfg(unix)]`) |
| `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root` | `policy.rs:873` | a prune-contained exception root survives composition; ordinary nesting still folds (`#[cfg(unix)]`) |
| `linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes` | `policy.rs:894` | the four Linux pseudo-filesystem prunes, and `/usr` absent (`#[cfg(unix)]`) |
| `cli_root_wins_and_bypasses_filters` | `policy.rs:902` | CLI root over configured root over a configured `ignore`/`unignore`; asserts `ScanScope::Explicit`, `DiscoveryFilters::Bypassed`, `recency == 300` |
| `configured_root_wins_over_global` | `policy.rs:925` | configured root yields `ScanScope::Explicit` rather than Global/Routine |
| `full_resolution_ignores_a_configured_root` | `policy.rs:946` | `full: true` returns Global before `config.root` is consulted |
| `maintenance_scope_is_routine_without_a_configured_root` | `policy.rs:974` | no configured root resolves to `ScanScope::Routine` regardless of machine seeds |
| `routine_state_errors_fall_back_to_seeds_with_one_warning` | `policy.rs:993` | `RecoverableInvalid` and `UnsupportedNewer` both fall back to existing seeds and produce a warning |
| `automatic_roots_classify_by_provenance_and_explicit_roots_stay_strict` | `policy.rs:1021` | C028 decision table: automatic `NotFound`/non-directory omit, explicit defects stay fatal |
| `symlinked_automatic_roots_block_and_are_never_followed_or_omitted` | `policy.rs:1072` | C028: symlink-to-dir and broken-link automatic roots block (`#[cfg(unix)]`) |
| `trailing_slash_symlink_roots_classify_like_plain_symlinks` | `policy.rs:1111` | B1: `link/` and `link/.` classify exactly like a plain symlink — fatal for explicit, blocking for automatic (`#[cfg(unix)]`) |
| `admission_premise_recheck_detects_reappearance_and_disappearance` | `policy.rs:1139` | C028 late recheck: reappearance/disappearance invalidates the premise; surviving files and vanished non-directories do not |
| `full_cleanup_consumes_only_a_fresh_in_memory_generation` | `policy.rs:1298` | C028: a missing in-memory generation refuses; a fresh one yields the automatic roots plus the generation |
| `full_cleanup_gate_proves_the_roots_whose_own_traversal_completed` | `policy.rs:1188` | M018: an incomplete platform walk plus a per-root coverage verdict yields the proven roots and reports the unproven ones |
| `full_cleanup_gate_proves_every_root_when_the_platform_walk_completed` | `policy.rs:1244` | M018: a zero reconciliation code proves every learned root without a coverage verdict |
| `full_cleanup_gate_proves_nothing_when_the_coverage_was_never_measured` | `policy.rs:1266` | M018: the fail-closed arm — an unmeasured root is an unproven root |
| `cleanup_candidates_preserve_symlinked_seed_identity_for_admission` | `policy.rs:1339` | C028: raw candidates carry the link itself, and classification blocks it (`#[cfg(unix)]`) |
| `missing_transient_roots_classify_as_non_maintainable_not_unavailable` | `policy.rs:1370` | C033: a missing path at a known-transient origin omits as `NotMaintainable`, a missing durable location as `NotFound`; neither blocks |
| `omission_diagnostics_group_reasons_and_bound_path_samples` | `policy.rs:1415` | C033: one summary per omission reason, exact counts, at most three sanitized examples, bounded line length |

Scope behaviour is covered mostly **one level down**, in `discovery.rs`, by
constructing `EffectiveScanPolicy` directly rather than through `resolve` — via
the `explicit_manifests` (`discovery.rs:1631`) and `routine_manifests`
(`discovery.rs:1658`) helpers:

- `discovery_reaches_unignored_project_without_entering_ignored_sibling`
  (`discovery.rs:2063`) — the unignore `exception_below` half, negative control
  inline.
- `filters_ignored_parent_keeps_exception_route` (`discovery.rs:2116`).
- `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned`
  (`discovery.rs:2037`) — `ScanScope::Global` + `Active` filters, and
  `user_ignore_prunes` attribution.
- `non_utf8_directory_still_matches_a_broad_ignore_pattern`
  (`discovery.rs:1699`, Linux only) and
  `manifest_name_is_matched_case_insensitively_where_the_volume_is`
  (`discovery.rs:1679`).
- `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests`
  (`discovery.rs:1481`), `global_rustup_prune_keeps_adjacent_user_project_reachable`
  (`discovery.rs:1424`).

Integration: `tests/end_to_end.rs:124` is the only *integration* test that
calls `policy::resolve` end to end, and it uses a **configured** root with
`cli_root: None` (`end_to_end.rs:120-123`). `tests/cli_contract.rs:197`
asserts the JSON `scope` field equals `"explicit"`, and per §7.9 that string
comes from the resolved policy (`main.rs:1429-1435`), so the assertion does
exercise the resolved scope rather than the flags.

**Honest gaps** — scope behaviour a reader should not assume is covered:

- **The Full branch is exercised, but only for its *variant*.**
  `full_resolution_ignores_a_configured_root` (`policy.rs:946`) does construct
  `ScanRequest { full: true }`, so `resolve` takes the early return and
  `global_discovery_policy()` is called on the host platform — but the
  assertion is `matches!(p.scope, ScanScope::Global(_))`, not the root set.
  The platform *tables* are therefore still covered only through the pure
  `unix_policy` helper, and the `#[cfg(windows)]` body (`policy.rs:754-778`)
  is untested on any lane.
- `canonical_dedup_roots` (`policy.rs:782-784`) and `lexical_identity`
  (`src/traverse.rs:80-102`) now have
  a direct caller in a test (`policy.rs:881` and `policy.rs:888`), but the
  unresolvable-root branch (the L16 fix, `src/traverse.rs:162-169`) still has no direct
  test. The `other unix` arm (`policy.rs:744-746`) and the
  `BaseDirs::new() == None` branch (`policy.rs:646-648`) are uncovered.
- The retention filter (`policy.rs:676-682`) is not directly tested:
  `retention_days == 0`, boundary equality, and the future-timestamp case. The
  only `routine_roots_from_state` test (`policy.rs:660`) exercises the
  *degraded* load arms, never the `Loaded` arm that owns the filter.
- `clean --known` root derivation (`main.rs:934-1120`), including the
  configured-root interaction in §7.8, has no test in `src/policy.rs`.
  `main.rs` has four inline tests, none of which reach root derivation; the
  nearest coverage is
  `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one`
  (`tests/cli_contract.rs:2535`) on the *scan* side and
  `bare_cleanup_with_no_known_roots_is_a_successful_no_op`
  (`tests/cli_contract.rs:1973`) on the empty-scope side.

## 9. Review checklist

1. **The `full` early-return is still first.** `policy.rs:21-30` must return
   before `policy.rs:31` is reached. Anything that reorders it changes what
   `--full` means.
2. **An explicit root still bypasses filters, not merely outranks config.**
   `policy.rs:43` must keep `DiscoveryFilters::Bypassed`; `discovery.rs:366`
   turns that into empty ignore/unignore slices.
3. **Every new `GlobalDiscoveryPolicy` field needs a `#[cfg]`-paired
   definition.** The two `global_discovery_policy` bodies are
   `policy.rs:692` (`#[cfg(unix)]`) and `policy.rs:755` (`#[cfg(windows)]`); a
   field added to one and not the other is a platform-specific compile error
   that only one CI job will catch.
4. **New seed names must be absolute-joinable and case variants must be
   deliberate.** `policy.rs:62-76`; the pairs exist because of volume
   case-sensitivity (`discovery.rs:113-130`).
5. **A new retention rule must fail conservative.** `policy.rs:676-682` keeps
   the root when `retention_days == 0` or the timestamp is in the future; ADR
   002 §5 requires the same for any new expiry path.
6. **Do not add a containment check to learned roots here without reading
   `discovery_state.rs:417` first** — `is_broad_root` already exists at
   derivation time; `policy.rs:683` deliberately consumes learned paths
   verbatim.
7. **A `u16`/`u32` or range assumption must be traced to `config::load`.**
   `policy.rs:47` narrows `u32 → u16` on the strength of
   `config.rs:107-111`; `policy.rs:23` relies on `config.rs:99-103` for the
   non-zero recency guard. Test-constructed `ScanConfig` values bypass both.
8. **Any new `AppError` here becomes exit code 2.** `policy.rs:33` and
   `policy.rs:38` are the only sources today in `resolve`; `main.rs:21-37`
   maps every `AppError` to exit 2, and a missing `scan.root` is therefore
   fatal rather than degrading to Routine. The admission seam adds
   `policy.rs:234` and `policy.rs:292`, and the Full gate adds
   `policy.rs:551-557` — the *intended* exit-2 refusal, not a defect. The
   removed `policy.rs:499-502` refusal is gone by decision, not by accident:
   see §5.
9. **Scope changes are safety changes for `clean --known`.** `main.rs:934-1120`
   and `main.rs:517-549` turn this module's output into the bounded root list
   a destructive command receives; re-verify §7.8 and §7.10 before landing.
10. **Do not widen the Full gate by accident.** `policy.rs:563-588` is the
    ADR 008 §5b route 5b-i rule: the proof boundary is the learned root, and a
    root whose own traversal did not complete is reported, never a candidate.
    A root with no coverage entry at all is unproven (`policy.rs:575`) — the
    fail-closed arm. Changing any of this needs a decision record, not a patch.
11. **Add a test for any branch you touch.** The Windows body
    (`policy.rs:754-778`), the `other unix` arm (`policy.rs:744-746`), the
    unresolvable-root branch (`src/traverse.rs:150-157`) and the retention filter
    (`policy.rs:676-682`) are currently untested; use `unix_policy`'s injected
    booleans (`policy.rs:731`) for platform tables and
    `routine_roots_from_state`'s injected `StateLoad`
    (`policy.rs:660-664`) for state behaviour — those are the two seams the
    module already provides.
