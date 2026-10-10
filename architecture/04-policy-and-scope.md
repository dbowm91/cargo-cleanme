# Policy & Scope — deciding what to look at

> Component deep dive · part of the [architecture overview](overview.md)

`src/policy.rs` — 349 lines, 101 of them inline tests.

## 1. Responsibility

`policy.rs` converts an *intent* (`ScanRequest`) plus *user configuration*
(`ScanConfig`) into the *concrete scope* (`EffectiveScanPolicy`) that
`discovery.rs` then walks. It is the only place in the crate where the
Routine / Full / Explicit distinction is decided.

**Owns:** scope-mode selection and its precedence (`policy.rs:20-58`); the
per-scope recency window (`:22`, `:54`); per-scope filter activation (`:24-27`,
`:42`, `:47-50`); existence / real-directory validation of an explicit root
(`:32-41`); Routine root assembly (`:60-125`); platform global root and prune
enumeration (`:126-188`); root normalization, dedup and containment collapse
(`:194-240`); the rustup managed-tool prune list (`:242-246`).

**Does not own:** filesystem traversal (`discovery.rs`), manifest → workspace
resolution (`workspace.rs`), activity classification (`traverse.rs`), state
reconciliation and atomic publish (`discovery_state.rs`), the cleanup root list
(`main.rs:113-153`), and the user-facing scope *label* (`main.rs:515-521`).

**Actual dependency set.** A grep for `crate::` in `policy.rs` yields exactly
five modules:

| Dependency | Used at | For |
|---|---|---|
| `crate::config::ScanConfig` | `policy.rs:2`, `policy.rs:19` | input config |
| `crate::domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope}` | `policy.rs:3` | in/out vocabulary |
| `crate::error::AppError` | `policy.rs:4` | the only error type returned |
| `crate::discovery_state` | `policy.rs:402`, `:413`, `:420-421` | `load_default`, `StateLoad`, `now_seconds` |
| `crate::discovery::effective_rustup_home` | `policy.rs:595` | managed-tool prune list |

Plus the `directories` crate (`policy.rs:396`) and, on Windows only,
`windows_sys` (`policy.rs:166-169`).

**Boundary with `discovery.rs` prune helpers.** `discovery.rs` owns the
*environment-derived* prune sets: `effective_cargo_home` (`:102`),
`effective_rustup_home` (`:83`) and `cargo_home_prunes` (`:114`, i.e.
`$CARGO_HOME/registry` and `$CARGO_HOME/git`). `policy.rs` owns only the
*platform* and global-only prune sets. The division is by scope, not by subject:
`cargo_home_prunes()` applies to **every** scope (`discovery.rs:331`) because a
registry tree must never trigger Cargo resolution even under an explicit root,
whereas the rustup list applies **only** to `Global` or `Routine`
(`discovery.rs:332-338`) and is sourced from
`policy::global_discovery_policy().managed_tool_prunes`, populated by
`managed_rust_prunes()` (`policy.rs:594`). `policy.rs` reaches into
`discovery` exactly once (`policy.rs:595`), for a single path; everything else
flows the other way.

**I/O: it does perform I/O, read-only.** The module is a *decision* module, not a
*pure* one. Its logic is a total function of `(request, config)` for the Full and
Explicit branches; the Routine branch additionally consults the filesystem, the
process environment, the wall clock, and the persisted state file. All of these
calls are reads:

| Call | Location | Reads |
|---|---|---|
| `fs::symlink_metadata` | `policy.rs:32` | explicit root stat |
| `Path::is_dir` (seeds) | `policy.rs:417` | seed existence |
| `directories::BaseDirs::new()` | `policy.rs:396` | env / platform home |
| `discovery_state::load_default()` | `policy.rs:402` | state JSON file |
| `discovery_state::now_seconds()` | `policy.rs:421` | wall clock |
| `Path::new("/usr/local").is_dir()` | `policy.rs:445` | macOS filesystem |
| `GetLogicalDrives` / `GetDriveTypeW` | `policy.rs:170`, `:179` | Windows volumes |
| `fs::canonicalize` | `policy.rs:571` | root identity |
| `discovery::effective_rustup_home` | `policy.rs:595` | `RUSTUP_HOME`, home |
| `eprintln!` | `policy.rs:91` | one stderr warning line |

It never writes to disk and never spawns a process. The single `eprintln!` at
`policy.rs:91` is the module's only side effect, and it exists so a degraded
Routine root set is never silent.

## 2. The scope decision

```rust
pub fn resolve(request: ScanRequest, config: &ScanConfig)
    -> Result<EffectiveScanPolicy, AppError>          // policy.rs:19
```

### Precedence, in code order

1. **`request.full == true` → `Global`, unconditionally** (`policy.rs:20-29`).
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
at `policy.rs:20` precedes the `or_else` at `policy.rs:30`. That combination is
unreachable from the command line — `scan --full` carries
`conflicts_with = "root"` (`cli.rs:38`), `clean --full` carries
`conflicts_with_all = ["root", "known"]` (`cli.rs:53`) — so the precedence
matters only for programmatic callers (`tests/end_to_end.rs:124`,
`examples/traversal-qualification.rs:26`). Note the asymmetry with cleanup, which
is decided in `main.rs` rather than here: for `clean`, a `ROOT` argument wins
over `--full` (`main.rs:113-115`).

### Fields populated on `EffectiveScanPolicy`

| Field | Full (`policy.rs:21-28`) | Explicit (`policy.rs:53-57`) | Routine (`policy.rs:53-57`) |
|---|---|---|---|
| `recency` | `config.recency_seconds` | same | same |
| `scope` | `Global(global_discovery_policy().roots)` | `Explicit(cli_root \| config.root)` | `Routine(routine_roots(retention_days))` |
| `discovery_filters` | `Active { ignore, unignore }` from config | `Bypassed` | `Active { ignore, unignore }` from config |

`recency` is scope-independent — it is `Duration::from_secs(config.recency_seconds)`
on every path (`policy.rs:22`, `policy.rs:54`). The type is
`domain::EffectiveScanPolicy` (`domain.rs:27-32`); `recency` is consumed by
`main.rs:307` for the activity cutoff, not by the walker.

### Errors returned

Only one variant, from two sites, both in the Explicit branch:

| Site | Condition | Message |
|---|---|---|
| `policy.rs:32-35` | `fs::symlink_metadata` fails | `AppError::InvalidRoot { path, reason: e.to_string() }` — includes the OS error (missing, permission denied) |
| `policy.rs:36-41` | not a directory, **or** is a symlink | `AppError::InvalidRoot { path, reason: "expected a real directory" }` |

The Full and Routine branches are infallible. A Routine scope with no
available roots returns `Ok(ScanScope::Routine(vec![]))`, never an error; the
user-facing signal is a diagnostic emitted later by `discovery.rs:344-353` and
the one stderr line at `policy.rs:91`. A propagated `AppError` reaches
`main.rs:9-12` and becomes exit code 2.

## 3. The three scope modes

`ScanScope` (`domain.rs:11-18`) has **four** variants, but `resolve` produces
only three of them:

| Variant | Produced by `resolve`? | Meaning |
|---|---|---|
| `Explicit(PathBuf)` | yes, `policy.rs:42` | one user-named root |
| `ExplicitRoots(Vec<PathBuf>)` | **no** | multiple explicit roots forming one cleanup boundary; constructed only in `cleanup.rs:774` and `cleanup.rs:1606` |
| `Global(Vec<PathBuf>)` | yes, `policy.rs:23` | exhaustive platform roots |
| `Routine(Vec<PathBuf>)` | yes, `policy.rs:46` | bounded adaptive roots |

### `Explicit(root)` — the user named a place

One directory, no `scan.ignore` / `scan.unignore` at all
(`DiscoveryFilters::Bypassed`, `policy.rs:42`). Worked example: `scan ~/work/api`
walks `~/work/api` recursively and reports every `Cargo.toml` beneath it, even
under a directory the user has globally ignored in `config.toml`. It still
enforces the symlink and real-directory rule (`policy.rs:36-41`), and it is the
only branch that can fail. A symlink *to* a directory is rejected.

### `Routine(roots)` — the default, bounded

The default `scan` with no root (`main.rs:266` → `run_scan(None, false, …)`).
Roots are existing seed directories under `$HOME` plus retained learned roots
from the state file (§5). Worked example: a developer with `~/Code/api` and
`~/Code/web` plus a learned root at `~/scratch/lab`, where the state file's
`last_project_seen_at` for `~/scratch/lab` is 10 days old and retention is 30.
`canonical_dedup_roots` (`policy.rs:236`) collapses nothing here, so the scan
walks all three, applying `scan.ignore` / `scan.unignore` normally. A Rust
project in `~/Documents/sidequest` is **not** found — that is the deliberate
blind spot, and `scan --full` is the documented way to teach the tool about it.

### `Global(roots)` — `--full`

`ScanScope::Global(global_discovery_policy().roots)` (`policy.rs:23`). Worked
example on Linux: roots is `["/"]` and the platform prunes are
`/proc`, `/sys`, `/dev`, `/run` (`policy.rs:150-153`), so the walk is
filesystem-root breadth minus pseudo-filesystems. `scan.ignore` / `scan.unignore`
apply. This is also the only scope that runs
`discovery_state::reconcile_full` afterwards (`main.rs:387`).

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
(`main.rs:284`), described in [the overview's scan pipeline](overview.md) §4 as
"Resolve scope — `policy::resolve` turns `{cli_root, full}` into concrete roots
+ a recency window".

## 4. Pruning inputs

`DiscoveryFilters` (`domain.rs:19-26`) has two states, and `resolve` picks
between them — it never merges them:

| State | Set by | Effect in `discovery.rs` |
|---|---|---|
| `Active { ignore: Vec<String>, unignore: Vec<PathBuf> }` | Full (`:24-27`) and Routine (`:47-50`) | `Filters::new(ignore, unignore)` at `discovery.rs:330` |
| `Bypassed` | Explicit (`:42`) | `(&[][..], &[][..])` at `discovery.rs:328` |

`config.ignore` and `config.unignore` are cloned verbatim from `ScanConfig`
(`config.rs:47-49`); `resolve` adds no patterns of its own.

### Semantics

- **ignore globs prune, and the exclusion is inherited.** Each `scan.ignore`
  string is compiled into a `globset::GlobSet` (`discovery.rs:18-30`) and matched
  against `path.to_string_lossy()` (`discovery.rs:38-45`). The lossy match is
  deliberate: a non-UTF-8 directory name must not escape a broad `*` rule on
  Linux (comment at `discovery.rs:30-37`, test
  `non_utf8_directory_still_matches_a_broad_ignore_pattern` at
  `discovery.rs:1203`). The match is also consulted for every **ancestor** of the
  candidate (`Filters::inherits_ignore`, `discovery.rs:52-59`), because a literal
  ignore rule names a directory, not its children: `/archive` is not a match for
  `/archive/other`, but `other` is still under an ignored directory and must
  stay excluded. This is the C019 correction, and the walk descends into
  `/archive` only as far as an exception requires.
- **`unignore` re-includes a subtree, not a single entry.** One function decides
  all of it: `Filters::disposition` (`discovery.rs:68-99`) returns
  `Included` / `Pruned` / `PassThrough` / `ReIncluded` (enum at
  `discovery.rs:101-110`). `ReIncluded` applies when
  `p.starts_with(u)` for some unignore entry, so the unignore path *and everything
  beneath it* escapes the prune. `PassThrough` is the other half: excluded, but
  with an unignore entry at or below it, so the walker enters far enough to reach
  the exception — and only that route. Tests:
  `filters_ignored_parent_keeps_exception_route` (`discovery.rs:1454`) for the
  `/*` shape, `literal_ignored_ancestor_becomes_pass_through_and_not_inherited_by_siblings`
  (`discovery.rs:1484`) for the literal shape, and
  `literal_ignored_ancestor_does_not_readmit_ignored_siblings`
  (`discovery.rs:1559`) for the same case through a real walk.
- **a CLI scan root takes precedence over configured scope.** Verified at
  `policy.rs:30`. The stronger form is what the code actually does: an explicit
  root does not merely win the root slot, it switches filters to `Bypassed`, so
  configured `scan.ignore` / `scan.unignore` do not apply to it at all. The
  inline test helper comment states the same intent
  (`discovery.rs:1133-1134`).

### What must be absolute, and why

`scan.root` must be absolute (`require_absolute(p, "scan.root")`,
`config.rs:112-114`); every `scan.ignore` pattern must be absolute
(`config.rs:115-121`, message `"ignore pattern must be absolute: {p}"`); every
`scan.unignore` entry must be absolute (`config.rs:122-124`).

Absoluteness is not cosmetic: patterns are matched against the **canonical**
walk path, so a relative or symlink-spelled pattern would silently match
nothing. `config::load` rewrites the glob-free literal prefix of every pattern
to its canonical spelling via `canonical_pattern_prefix` (`config.rs:155-187`,
`:323-366`), leaving the glob tail verbatim. A pattern naming a non-existent
path is left untouched — it still means exactly what it says, i.e. it matches
nothing. Glob *syntax* is validated at load time too (`config.rs:137-151`), so
a bad pattern fails at `config::load_or_create` rather than at the first walk.

**What `globset` is for:** it compiles the ignore list once into a `GlobSet`
(`discovery.rs:19-27`) so the per-directory prune test is a set lookup rather
than a re-parse. The unignore side uses no globbing at all — it is exact
`Path::starts_with` prefix comparison on absolute `PathBuf`s, which is why
`config.toml:24` calls them "Exact absolute directories".

**One divergence to be aware of.** `config.toml:21` says these filters "apply
only to global discovery". The code applies them to Routine scope as well
(`policy.rs:47-50`), and `discovery.rs` honours them for both
(`discovery.rs:326-330`, `discovery.rs:343-389` vs `:334-350`). The comment is
narrower than the behaviour.

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
(`policy.rs:417`: `.filter(|p| p.is_dir())`).

**Who supplies `home`:** `routine_roots` (`policy.rs:395-397`) builds it itself
from `directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())` — the
same crate and accessor used by `discovery.rs:148` and `discovery.rs:172`, but
obtained independently of `main.rs`. `policy::resolve` cannot inject a home
directory; `routine_seed_candidates` is the only seam, and it exists so the list
can be asserted without touching the real home directory. If
`BaseDirs::new()` returns `None`, `routine_roots` returns an empty `Vec`
(`policy.rs:396-397`) and Routine silently degrades to "no roots", which
`discovery.rs:344-353` reports as `Info` / `PlatformRoot`.

**Heuristic vs exact:**

| Part | Status |
|---|---|
| The 13 directory names | **Heuristic.** A guess at where developers keep Rust work. Nothing verifies it. |
| `home.join(name)` | **Exact.** |
| The `.is_dir()` filter | **Exact**, one stat per candidate. |
| `canonical_dedup_roots` normalization | **Exact** for roots that canonicalize; lexical for the rest (§6). |
| The `last_project_seen_at` recency filter | **Exact** against the persisted state and the wall clock. |
| Whether the seeds *cover* the user's work | **Not a property of this code** — the accepted cost in ADR 002 ("the 30-day default limits stale routine roots without creating permanent blind spots"). |

`routine_seed_candidates` is `pub` but has **no non-test caller in `src/`** —
the only call site is `policy.rs:101`. It is public surface without an internal
consumer.

**Assembly.** `routine_roots_from_state` concatenates existing seeds first,
then learned roots that pass both retention and the automatic-maintenance
location policy, then hands the vector to `canonical_dedup_roots`. The shared
`discovery_state::automatic_root_is_maintainable` rule excludes known cache,
Trash, temporary, node_modules, and transient Codex worktree locations from
Routine selection. It is not a Full discovery filter and does not affect an
explicit root. Retention remains:

```rust
automatic_root_is_maintainable(path, home)
    && (retention_days == 0
    || now < r.last_project_seen_at               // future timestamp: keep
    || now.saturating_sub(r.last_project_seen_at) <= u64::from(retention_days) * 86400)
```

`retention_days == 0` disables expiration and a future timestamp fails
conservative — both match ADR 002 §5. A `StateLoad` that is not `Loaded`
(`Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable`) yields
`load.diagnostic()` as a warning and **keeps the seeds** (`policy.rs:419-437`),
matching ADR 002 §7's fail-soft rule for a corrupt state file.

### Cleanup admission is provenance-aware, scan assembly is not (C028)

`resolve` above decides what a *scan* walks. Cleanup admission is a separate
seam that runs before any collapse, because collapsing first would resolve a
symlinked root into its target and hide the invalid identity:

- `RootProvenance` (`policy.rs:88`) labels each candidate `Explicit` or
  `Automatic`; `ClassifiedRoots` (`policy.rs:115`) carries the admitted,
  safely omitted (`AutomaticOmission::NotFound` / `NonDirectory`,
  `policy.rs:97`), and blocking (`AutomaticBlock`, `policy.rs:108`) sets.
- `classify_cleanup_roots` (`policy.rs:156`) uses `symlink_metadata` and the
  real `io::ErrorKind` — never `Path::exists`/`is_dir`, which conflate errors
  and follow symlinks — and has no catch-all `Err(_) => continue`.
- `routine_cleanup_candidates` (`policy.rs:363`) supplies the raw Routine
  candidates (existing seeds in any form plus age-eligible learned roots)
  without canonicalization, so a symlinked seed reaches classification as a
  link. `full_cleanup_roots` (`policy.rs:328`) supplies the Full roots from
  the in-memory generation the reconciliation just proved, or refuses.
- `recheck_admission_premise` (`policy.rs:229`) re-examines the premise with
  fresh metadata before the first final proof and before every later spawn;
  `CleanupCandidates` (`policy.rs:318`) is the shared candidate type.

Omission diagnostics are grouped by typed reason and sorted deterministically
(`ClassifiedRoots::omission_diagnostics`, `policy.rs:135-169`). Each summary
includes the exact omitted-root count and at most three sanitized paths. The
omission vector remains complete for `recheck_admission_premise`; reporting does
not retire persisted state or alter the selected cleanup universe.

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
| `policy.rs:126-133` | `#[cfg(unix)]` | delegates to `unix_policy(...)`, then hands the result to `compose_global_policy` (`policy.rs:142-163`), which fills `roots` through `canonical_dedup_roots_protecting` and `managed_tool_prunes` through `managed_rust_prunes()` |
| `policy.rs:189-213` | `#[cfg(windows)]` | enumerates drive letters with `GetLogicalDrives` / `GetDriveTypeW` |

**They are not duplicates and neither is dead.** They are mutually exclusive
`#[cfg]` variants of the same public function: exactly one is compiled per
platform, and a name collision never occurs. On any other target neither
exists, and the `resolve` call at `policy.rs:23` would not compile — which is
consistent with the crate shipping a single binary for unix and Windows only.

A third, different function shares the family: `unix_policy`
(`policy.rs:140-162`, `#[cfg(unix)]`, private) is a *pure* helper whose three
booleans are supplied from `cfg!` plus one filesystem probe
(`policy.rs:128-132`). It exists so the macOS and Linux tables can be unit
tested on any host. It always returns `managed_tool_prunes: Vec::new()`
(`policy.rs:160`); the `#[cfg(unix)]` wrapper supplies that field
(`policy.rs:136`).

### Platform tables

| Platform | `roots` | `global_only_prunes` | Source |
|---|---|---|---|
| macOS | `["/"]`, plus `/usr/local` when that directory exists | `/System`, `/dev`, `/bin`, `/sbin`, `/usr` | `policy.rs:166-182` |
| Linux | `["/"]` | `/proc`, `/sys`, `/dev`, `/run` | `policy.rs:175-178` |
| Other unix | `["/"]` | none | `policy.rs:179-181` |
| Windows | `X:\` for each letter where `GetLogicalDrives` reports a bit and `GetDriveTypeW` is `DRIVE_FIXED` or `DRIVE_REMOVABLE` | none | `policy.rs:190-212` |

The macOS row is the interesting one: `/usr` is pruned while `/usr/local` is
enumerated as a *root*, so the Homebrew toolchain subtree stays reachable
inside an otherwise-pruned `/usr`. That exception takes **two** steps to work,
and both were once missing:

1. **Composition.** `global_discovery_policy` (`policy.rs:126-133`) delegates to
   `compose_global_policy` (`policy.rs:142-163`), which collapses redundant
   roots — and the collapse folds `/usr/local` into `/`, because
   `"/usr/local".starts_with("/")`. `compose_global_policy` therefore computes
   the *protected* set first: every root sitting strictly inside a global-only
   prune is that prune's exception, so `canonical_dedup_roots_protecting`
   (`policy.rs:248-278`) keeps it as a walk root of its own.
2. **Application.** A root that survived composition would still be unwalked:
   `entry_is_within` matches on `parent_path`, so with root `/usr/local` every
   entry's parent `starts_with("/usr")` and the subtree read as pruned.
   `discovery::entry_is_system_pruned` (`discovery.rs:270-283`) exempts a walk
   root strictly inside a system prune. The exemption is scoped to *global-only*
   prunes on purpose: a Cargo or rustup home stays refused even when a root sits
   inside it.

Test `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local`
(`policy.rs:294`) pins this — and it now calls `compose_global_policy`, so it
asserts the roots **production** walks rather than the ones a private
pre-collapse helper declares. That distinction was the defect: the old test
called `unix_policy` directly and passed while production collapsed the root
away. `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root`
(`policy.rs:321`) covers the collapse alone, including that an ordinary nested
root is still folded; `a_root_inside_a_global_prune_reopens_that_prune`
(`discovery.rs:940`) covers the prune. `managed_tool_prunes` is `Some` only via
`managed_rust_prunes()` → `discovery::effective_rustup_home()`
(`policy.rs:280-284`), i.e. `$RUSTUP_HOME` when absolute, else `~/.rustup`
(`discovery.rs:145-186`).

### Consumers

| Consumer | Behaviour |
|---|---|
| `discovery.rs:310` | re-derives the policy for `Global` scope only |
| `discovery.rs:311-318` | **adopts it only if `canonical_roots(roots) == canonical_roots(&candidate.roots)`**; otherwise treats the scope as a plain root list with no platform prunes |
| `discovery.rs:322` | when adopted, walks `p.roots` — which is why the `roots` carried in `ScanScope::Global` and in `GlobalDiscoveryPolicy` are kept in agreement |
| `discovery.rs:335` | `managed_tool_prunes`, gated on `global` (Global **or** Routine) |
| `discovery.rs:378` | `global_only_prunes`, passed only to `discover_global_roots` |
| `examples/traversal-qualification.rs:50` | reads `.roots` for the standalone qualification harness |

The equality guard at `discovery.rs:311-318` is why a *caller-constructed*
`ScanScope::Global` (as several `discovery.rs` tests do) does **not** silently
acquire platform prunes it never asked for.

### Root normalization

`canonical_dedup_roots` (`policy.rs:236-246`) is the shared normalizer for all
three roots lists:

1. For each root, try `fs::canonicalize`. Success → identity is the canonical
   path *and* the retained spelling is canonical. Failure → identity is
   `lexical_identity(root)` but the retained spelling stays as the caller wrote
   it (so an unavailable root still names what the user asked for).
2. Sort by identity and `dedup_by` identity equality.
3. Drop any root whose identity `starts_with` an already-kept root's identity
   (containment collapse).

`lexical_identity` (`policy.rs:219-234`) resolves `.` and `..` textually and
never touches the filesystem; the doc comment states it "is only ever compared
against other roots, never walked". The rationale comment at `policy.rs:212-217`
records the bug it fixed (L16): an unresolvable root previously kept its raw
spelling and so was neither deduped nor collapsed, causing an overlapping
subtree to be scanned twice.

## 7. Invariants and edge cases

1. **`full` beats everything.** The `full` early-return (`policy.rs:20`) sits
   above every root consideration. Reordering it would silently convert
   `--full` into a single-root scan.
2. **An explicit root wins monotonically.** `policy.rs:30` is a single
   `or_else`; no branch lets a configured root displace a CLI root, so adding
   roots to `ScanConfig` can never override a CLI root.
3. **Explicit scope bypasses filters — completely.** Not "precedence": a
   configured `scan.ignore` has no effect at all under `ScanScope::Explicit`
   (`policy.rs:42`). Pinned by `cli_root_wins_and_bypasses_filters`
   (`policy.rs:282`).
4. **A Routine root is *not* guaranteed to be under `$HOME` by this module.**
    Seeds are (`policy.rs:77`), but learned roots are taken verbatim from
    `state.learned_roots[].path` (`policy.rs:432`) with a recency test and
    nothing else — no containment check is performed here. The "no
    broad/platform roots are learned" guarantee is a *derivation-time* rule in
    `discovery_state.rs` (`is_broad_root` at `discovery_state.rs:241`, the
    home-equality filter at `discovery_state.rs:151`), i.e. ADR 002 §2, and it is
    not re-asserted here. A hand-edited or stale state file can therefore inject a
    root outside the home directory into Routine scope, and `clean --known` will
    use it.
5. **A stale configured root is a hard error, not a fallback.** `config::load`
    validates only that `scan.root` is absolute (`config.rs:112-114`), never
    that it exists. A deleted `scan.root` makes *every* scan and every bare
    invocation fail with `AppError::InvalidRoot` → exit code 2 (`main.rs:9-12`).
    There is no degradation path back to Routine. Since C028 this is true only
    of *explicit* roots: a deleted automatically learned/seed root is omitted
    (proven absence/non-directory) or blocks with a typed report (symlink or
    unreadable), via the admission seam above — a stale advisory path can no
    longer reach explicit-root validation and abort the run.
6. **Empty Routine roots are not an error.** `resolve` returns
   `Ok(Routine(vec![]))`; `discovery.rs:344-353` emits `Info` / `PlatformRoot`
   "no Routine roots are available; run `scan --full` or scan an explicit root".
7. **`learned_root_retention_days as u16` (`policy.rs:46`) is lossless only
   because of the caller's validation.** `config::load` range-checks it to
   0–3650 (`config.rs:107-111`), which fits in `u16`; a `ScanConfig` built
   directly — as every test does — bypasses that check, and a value above 65535
   would wrap silently. Same class of issue for `recency_seconds`: `load`
   rejects 0 (`config.rs:99-103`) because 0 would disable the inactivity guard,
   but `resolve` does not (`policy.rs:22`, `:54`).
8. **A configured `scan.root` silently disables `clean --known`.**
   `main.rs:133-143` takes only the `Routine` arm and yields `Vec::new()` for any
   other scope; `main.rs:174-194` then prints "no bounded cleanup roots are
   known; no cleanup commands were run" and returns **0**. Setting `scan.root`
   is therefore not orthogonal to `--known`.
9. **The JSON `scope` field is derived from CLI flags, not from the resolved
   policy** (`main.rs:510-522`): `full ? "full" : explicit_scope ? "explicit" :
   "routine"`, where `explicit_scope = root.is_some()` (`main.rs:296`). A scan
   driven by a *configured* `scan.root` resolves to `ScanScope::Explicit` but
   reports `"routine"`. Do not use the JSON label to infer the resolved scope.
10. **The safety-envelope coupling.** `clean --known` routes its cleanup roots
    through `policy::resolve` (`main.rs:133`), so any change to scope resolution
    changes the set of roots a destructive command may operate on. A scope
    change is a safety change and belongs in the same review as the proof work
    in [the cleanup deep dive](09-cleanup.md).
11. **State degradation is visible but not fatal.** A corrupt or newer-schema
    state file degrades Routine to seeds plus one stderr line
    (`policy.rs:90-92`, `policy.rs:105-123`); it never expands scope and never
    fails the run.

## 8. Testing

`policy.rs` **does** have an inline `#[cfg(test)] mod tests` at
`policy.rs:601` — the tests:

| Test | Line | Covers |
|---|---|---|
| `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local` | `policy.rs:608` | macOS prune list, `/usr/local` root promotion, `/Library` + `/Applications` *not* pruned (`#[cfg(unix)]`) |
| `a_root_inside_a_global_prune_survives_the_collapse_as_its_own_root` | `policy.rs:635` | a prune-contained exception root survives composition; ordinary nesting still folds (`#[cfg(unix)]`) |
| `linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes` | `policy.rs:656` | the four Linux pseudo-filesystem prunes, and `/usr` absent (`#[cfg(unix)]`) |
| `cli_root_wins_and_bypasses_filters` | `policy.rs:664` | CLI root over configured root over a configured `ignore`/`unignore`; asserts `ScanScope::Explicit`, `DiscoveryFilters::Bypassed`, `recency == 300` |
| `configured_root_wins_over_global` | `policy.rs:687` | configured root yields `ScanScope::Explicit` rather than Global/Routine |
| `full_resolution_ignores_a_configured_root` | `policy.rs:708` | `full: true` returns Global before `config.root` is consulted |
| `maintenance_scope_is_routine_without_a_configured_root` | `policy.rs:736` | no configured root resolves to `ScanScope::Routine` regardless of machine seeds |
| `routine_state_errors_fall_back_to_seeds_with_one_warning` | `policy.rs:755` | `RecoverableInvalid` and `UnsupportedNewer` both fall back to existing seeds and produce a warning |
| `automatic_roots_classify_by_provenance_and_explicit_roots_stay_strict` | `policy.rs:783` | C028 decision table: automatic `NotFound`/non-directory omit, explicit defects stay fatal |
| `symlinked_automatic_roots_block_and_are_never_followed_or_omitted` | `policy.rs:831` | C028: symlink-to-dir and broken-link automatic roots block (`#[cfg(unix)]`) |
| `cleanup_candidates_preserve_symlinked_seed_identity_for_admission` | `policy.rs:943` | C028: raw candidates carry the link itself, and classification blocks it (`#[cfg(unix)]`) |
| `admission_premise_recheck_detects_reappearance_and_disappearance` | `policy.rs:868` | C028 late recheck: reappearance/disappearance invalidates the premise; surviving files and vanished non-directories do not |
| `full_cleanup_consumes_only_a_fresh_in_memory_generation` | `policy.rs:913` | C028: non-zero reconciliation or missing generation refuses; fresh state yields automatic roots plus generation |

Scope behaviour is covered mostly **one level down**, in `discovery.rs`, by
constructing `EffectiveScanPolicy` directly rather than through `resolve` — via
the `explicit_manifests` (`:1059`) and `routine_manifests` (`:1073`) helpers:

- `discovery_reaches_unignored_project_without_entering_ignored_sibling`
  (`discovery.rs:1401`) — the unignore `exception_below` half, negative control
  inline.
- `filters_ignored_parent_keeps_exception_route` (`discovery.rs:1454`).
- `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned`
  (`discovery.rs:1375`) — `ScanScope::Global` + `Active` filters, and
  `user_ignore_prunes` attribution.
- `non_utf8_directory_still_matches_a_broad_ignore_pattern`
  (`discovery.rs:1203`, Linux only) and
  `manifest_name_is_matched_case_insensitively_where_the_volume_is`
  (`discovery.rs:1183`).
- `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests`
  (`discovery.rs:987`), `global_rustup_prune_keeps_adjacent_user_project_reachable`
  (`discovery.rs:931`).

Integration: `tests/end_to_end.rs:124` is the only test that calls
`policy::resolve` end to end, and it uses a **configured** root with
`cli_root: None` (`end_to_end.rs:120-123`). `tests/cli_contract.rs:202` asserts
the JSON `scope` field equals `"explicit"` — but per §7.9 that string comes from
`main.rs:517`, so it does not exercise the resolved scope.

**Honest gaps** — scope behaviour a reader should not assume is covered:

- **The Full branch has no test at all.** Nothing constructs
  `ScanRequest { full: true }`, so `ScanScope::Global` from `resolve` and the
  `global_discovery_policy()` call at `policy.rs:23` are never executed by a
  test; the platform tables are covered only through the pure `unix_policy`
  helper. The `#[cfg(windows)]` body (`policy.rs:164-188`) is untested too.
- `canonical_dedup_roots` and `lexical_identity` (`policy.rs:533-592`) are
  covered only indirectly, via the discovery tests above — the
  unresolvable-root branch (the L16 fix) has no direct test. The `other unix`
  arm (`policy.rs:493-495`) and the `BaseDirs::new() == None` branch
  (`policy.rs:398-400`) are uncovered.
- The retention filter (`policy.rs:427-430`) is not directly tested:
  `retention_days == 0`, boundary equality, and the future-timestamp case.
- `clean --known` root derivation (`main.rs:133-148`), including the
  configured-root interaction in §7.8, has no integration test in
  `tests/cli_contract.rs` or `tests/end_to_end.rs`.

## 9. Review checklist

1. **The `full` early-return is still first.** `policy.rs:20-29` must return
   before `policy.rs:30` is reached. Anything that reorders it changes what
   `--full` means.
2. **An explicit root still bypasses filters, not merely outranks config.**
   `policy.rs:42` must keep `DiscoveryFilters::Bypassed`; `discovery.rs:328`
   turns that into empty ignore/unignore slices.
3. **Every new `GlobalDiscoveryPolicy` field needs a `#[cfg]`-paired
   definition.** The two `global_discovery_policy` bodies are
   `policy.rs:441` (`#[cfg(unix)]`) and `policy.rs:503` (`#[cfg(windows)]`); a
   field added to one and not the other is a platform-specific compile error
   that only one CI job will catch.
4. **New seed names must be absolute-joinable and case variants must be
   deliberate.** `policy.rs:61-75`; the pairs exist because of volume
   case-sensitivity (`discovery.rs:108-125`).
5. **A new retention rule must fail conservative.** `policy.rs:427-430` keeps
   the root when `retention_days == 0` or the timestamp is in the future; ADR
   002 §5 requires the same for any new expiry path.
6. **Do not add a containment check to learned roots here without reading
   `discovery_state.rs:241` first** — `is_broad_root` already exists at
   derivation time; `policy.rs:434` deliberately consumes learned paths
   verbatim.
7. **A `u16`/`u32` or range assumption must be traced to `config::load`.**
   `policy.rs:46` narrows `u32 → u16` on the strength of
   `config.rs:107-111`; `policy.rs:22` relies on `config.rs:99-103` for the
   non-zero recency guard. Test-constructed `ScanConfig` values bypass both.
8. **Any new `AppError` here becomes exit code 2.** `policy.rs:32` and
   `policy.rs:37` are the only sources today; `main.rs:9-12` maps every
   `AppError` to exit 2, and a missing `scan.root` is therefore fatal rather
   than degrading to Routine.
9. **Scope changes are safety changes for `clean --known`.** `main.rs:133-148`
   and `main.rs:174-194` turn this module's output into the bounded root list
   a destructive command receives; re-verify §7.8 and §7.10 before landing.
10. **Add a test for any branch you touch.** The Full branch
    (`policy.rs:20-29`), the Windows body (`policy.rs:503-528`), and the
    retention filter (`policy.rs:427-430`) are currently untested; use
    `unix_policy`'s injected booleans (`policy.rs:480`) for platform tables
    and `routine_roots_from_state`'s injected `StateLoad`
    (`policy.rs:410-414`) for state behaviour — those are the two seams the
    module already provides.
