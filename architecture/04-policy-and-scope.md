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
(`main.rs:113-153`), and the user-facing scope *label* (`main.rs:501-507`).

**Actual dependency set.** A grep for `crate::` in `policy.rs` yields exactly
five modules:

| Dependency | Used at | For |
|---|---|---|
| `crate::config::ScanConfig` | `policy.rs:2`, `policy.rs:251` | input config |
| `crate::domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope}` | `policy.rs:3` | in/out vocabulary |
| `crate::error::AppError` | `policy.rs:4` | the only error type returned |
| `crate::discovery_state` | `policy.rs:88`, `:99`, `:106-107` | `load_default`, `StateLoad`, `now_seconds` |
| `crate::discovery::effective_rustup_home` | `policy.rs:243` | managed-tool prune list |

Plus the `directories` crate (`policy.rs:82`) and, on Windows only,
`windows_sys` (`policy.rs:166-169`).

**Boundary with `discovery.rs` prune helpers.** `discovery.rs` owns the
*environment-derived* prune sets: `effective_cargo_home` (`:102`),
`effective_rustup_home` (`:83`) and `cargo_home_prunes` (`:114`, i.e.
`$CARGO_HOME/registry` and `$CARGO_HOME/git`). `policy.rs` owns only the
*platform* and global-only prune sets. The division is by scope, not by subject:
`cargo_home_prunes()` applies to **every** scope (`discovery.rs:269`) because a
registry tree must never trigger Cargo resolution even under an explicit root,
whereas the rustup list applies **only** to `Global` or `Routine`
(`discovery.rs:270-276`) and is sourced from
`policy::global_discovery_policy().managed_tool_prunes`, populated by
`managed_rust_prunes()` (`policy.rs:242-246`). `policy.rs` reaches into
`discovery` exactly once (`policy.rs:243`), for a single path; everything else
flows the other way.

**I/O: it does perform I/O, read-only.** The module is a *decision* module, not a
*pure* one. Its logic is a total function of `(request, config)` for the Full and
Explicit branches; the Routine branch additionally consults the filesystem, the
process environment, the wall clock, and the persisted state file. All of these
calls are reads:

| Call | Location | Reads |
|---|---|---|
| `fs::symlink_metadata` | `policy.rs:32` | explicit root stat |
| `Path::is_dir` (seeds) | `policy.rs:103` | seed existence |
| `directories::BaseDirs::new()` | `policy.rs:82` | env / platform home |
| `discovery_state::load_default()` | `policy.rs:88` | state JSON file |
| `discovery_state::now_seconds()` | `policy.rs:107` | wall clock |
| `Path::new("/usr/local").is_dir()` | `policy.rs:131` | macOS filesystem |
| `GetLogicalDrives` / `GetDriveTypeW` | `policy.rs:170`, `:179` | Windows volumes |
| `fs::canonicalize` | `policy.rs:220` | root identity |
| `discovery::effective_rustup_home` | `policy.rs:243` | `RUSTUP_HOME`, home |
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
user-facing signal is a diagnostic emitted later by `discovery.rs:282-291` and
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
`canonical_dedup_roots` (`policy.rs:211`) collapses nothing here, so the scan
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
| `Active { ignore: Vec<String>, unignore: Vec<PathBuf> }` | Full (`:24-27`) and Routine (`:47-50`) | `Filters::new(ignore, unignore)` at `discovery.rs:268` |
| `Bypassed` | Explicit (`:42`) | `(&[][..], &[][..])` at `discovery.rs:266` |

`config.ignore` and `config.unignore` are cloned verbatim from `ScanConfig`
(`config.rs:44-47`); `resolve` adds no patterns of its own.

### Semantics

- **ignore globs prune.** Each `scan.ignore` string is compiled into a
  `globset::GlobSet` (`discovery.rs:18-30`) and matched against
  `path.to_string_lossy()` (`discovery.rs:39-40`). The lossy match is
  deliberate: a non-UTF-8 directory name must not escape a broad `*` rule on
  Linux (comment at `discovery.rs:35-38`, test
  `non_utf8_directory_still_matches_a_broad_ignore_pattern` at
  `discovery.rs:1114`).
- **unignore re-includes a subtree, not a single entry.** `Filters::ignored`
  is `globs.is_match(s) && !unignore.iter().any(|u| p.starts_with(u))`
  (`discovery.rs:40`) — so the unignore path *and everything beneath it*
  escapes the prune. `Filters::exception_below` (`discovery.rs:42-44`) is the
  other half: if any unignore entry is *below* `p`, the walker keeps descending
  through `p` so it can reach the exception. Test
  `discovery_reaches_unignored_project_without_entering_ignored_sibling`
  (`discovery.rs:1307`) pins both halves.
- **a CLI scan root takes precedence over configured scope.** Verified at
  `policy.rs:30`. The stronger form is what the code actually does: an explicit
  root does not merely win the root slot, it switches filters to `Bypassed`, so
  configured `scan.ignore` / `scan.unignore` do not apply to it at all. The
  inline test helper comment states the same intent
  (`discovery.rs:1057-1058`).

### What must be absolute, and why

`scan.root` must be absolute (`require_absolute(p, "scan.root")`,
`config.rs:110-112`); every `scan.ignore` pattern must be absolute
(`config.rs:113-119`, message `"ignore pattern must be absolute: {p}"`); every
`scan.unignore` entry must be absolute (`config.rs:120-122`).

Absoluteness is not cosmetic: patterns are matched against the **canonical**
walk path, so a relative or symlink-spelled pattern would silently match
nothing. `config::load` rewrites the glob-free literal prefix of every pattern
to its canonical spelling via `canonical_pattern_prefix` (`config.rs:146-164`,
`:190-214`), leaving the glob tail verbatim. A pattern naming a non-existent
path is left untouched — it still means exactly what it says, i.e. it matches
nothing. Glob *syntax* is validated at load time too (`config.rs:146-149`), so
a bad pattern fails at `config::load_or_create` rather than at the first walk.

**What `globset` is for:** it compiles the ignore list once into a `GlobSet`
(`discovery.rs:19-27`) so the per-directory prune test is a set lookup rather
than a re-parse. The unignore side uses no globbing at all — it is exact
`Path::starts_with` prefix comparison on absolute `PathBuf`s, which is why
`config.toml:24` calls them "Exact absolute directories".

**One divergence to be aware of.** `config.toml:21` says these filters "apply
only to global discovery". The code applies them to Routine scope as well
(`policy.rs:47-50`), and `discovery.rs` honours them for both
(`discovery.rs:264-268`, `discovery.rs:281-327` vs `:334-350`). The comment is
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
`is_manifest_name` (`discovery.rs:46-63`).

**This function is pure and does no filtering.** It is a literal `join` over a
fixed array: no existence check, no config read, no env read, no state read.
Existence filtering happens one level up, in `routine_roots_from_state`
(`policy.rs:101-104`: `.filter(|p| p.is_dir())`).

**Who supplies `home`:** `routine_roots` (`policy.rs:81-84`) builds it itself
from `directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())` — the
same crate and accessor used by `discovery.rs:86` and `discovery.rs:110`, but
obtained independently of `main.rs`. `policy::resolve` cannot inject a home
directory; `routine_seed_candidates` is the only seam, and it exists so the list
can be asserted without touching the real home directory. If
`BaseDirs::new()` returns `None`, `routine_roots` returns an empty `Vec`
(`policy.rs:82-84`) and Routine silently degrades to "no roots", which
`discovery.rs:282-291` reports as `Info` / `PlatformRoot`.

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

**Assembly.** `routine_roots_from_state` (`policy.rs:96-125`) concatenates
existing seeds first, then each learned root passing the retention test, then
hands the vector to `canonical_dedup_roots`. Retention (`policy.rs:112-117`):

```rust
retention_days == 0
    || now < r.last_project_seen_at               // future timestamp: keep
    || now.saturating_sub(r.last_project_seen_at) <= u64::from(retention_days) * 86400
```

`retention_days == 0` disables expiration and a future timestamp fails
conservative — both match ADR 002 §5. A `StateLoad` that is not `Loaded`
(`Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable`) yields
`load.diagnostic()` as a warning and **keeps the seeds** (`policy.rs:105-123`),
matching ADR 002 §7's fail-soft rule for a corrupt state file.

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
| `policy.rs:126-138` | `#[cfg(unix)]` | delegates to `unix_policy(...)`, then fills `roots` through `canonical_dedup_roots` and `managed_tool_prunes` through `managed_rust_prunes()` |
| `policy.rs:164-188` | `#[cfg(windows)]` | enumerates drive letters with `GetLogicalDrives` / `GetDriveTypeW` |

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
| macOS | `["/"]`, plus `/usr/local` when that directory exists | `/System`, `/dev`, `/bin`, `/sbin`, `/usr` | `policy.rs:142-149` |
| Linux | `["/"]` | `/proc`, `/sys`, `/dev`, `/run` | `policy.rs:150-153` |
| Other unix | `["/"]` | none | `policy.rs:154-156` |
| Windows | `X:\` for each letter where `GetLogicalDrives` reports a bit and `GetDriveTypeW` is `DRIVE_FIXED` or `DRIVE_REMOVABLE` | none | `policy.rs:170-182` |

The macOS row is the interesting one: `/usr` is pruned while `/usr/local` is
enumerated as a *root*, so the Homebrew toolchain subtree stays reachable
inside an otherwise-pruned `/usr`. Test
`macos_global_policy_prunes_protected_usr_and_enumerates_usr_local`
(`policy.rs:256`) pins exactly that, and also asserts `/Library` and
`/Applications` are *not* pruned. `managed_tool_prunes` is `Some` only via
`managed_rust_prunes()` → `discovery::effective_rustup_home()`
(`policy.rs:242-246`), i.e. `$RUSTUP_HOME` when absolute, else `~/.rustup`
(`discovery.rs:90-99`).

### Consumers

| Consumer | Behaviour |
|---|---|
| `discovery.rs:248` | re-derives the policy for `Global` scope only |
| `discovery.rs:249-256` | **adopts it only if `canonical_roots(roots) == canonical_roots(&candidate.roots)`**; otherwise treats the scope as a plain root list with no platform prunes |
| `discovery.rs:260` | when adopted, walks `p.roots` — which is why the `roots` carried in `ScanScope::Global` and in `GlobalDiscoveryPolicy` are kept in agreement |
| `discovery.rs:273` | `managed_tool_prunes`, gated on `global` (Global **or** Routine) |
| `discovery.rs:316` | `global_only_prunes`, passed only to `discover_global_roots` |
| `examples/traversal-qualification.rs:50` | reads `.roots` for the standalone qualification harness |

The equality guard at `discovery.rs:249-256` is why a *caller-constructed*
`ScanScope::Global` (as several `discovery.rs` tests do) does **not** silently
acquire platform prunes it never asked for.

### Root normalization

`canonical_dedup_roots` (`policy.rs:211-240`) is the shared normalizer for all
three roots lists:

1. For each root, try `fs::canonicalize`. Success → identity is the canonical
   path *and* the retained spelling is canonical. Failure → identity is
   `lexical_identity(root)` but the retained spelling stays as the caller wrote
   it (so an unavailable root still names what the user asked for).
2. Sort by identity and `dedup_by` identity equality.
3. Drop any root whose identity `starts_with` an already-kept root's identity
   (containment collapse).

`lexical_identity` (`policy.rs:194-209`) resolves `.` and `..` textually and
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
   `state.learned_roots[].path` (`policy.rs:118`) with a recency test and
   nothing else — no containment check is performed here. The "no
   broad/platform roots are learned" guarantee is a *derivation-time* rule in
   `discovery_state.rs` (`is_broad_root` at `discovery_state.rs:220`, the
   home-equality filter at `discovery_state.rs:151`), i.e. ADR 002 §2, and it is
   not re-asserted here. A hand-edited or stale state file can therefore inject a
   root outside the home directory into Routine scope, and `clean --known` will
   use it.
5. **A stale configured root is a hard error, not a fallback.** `config::load`
   validates only that `scan.root` is absolute (`config.rs:110-112`), never
   that it exists. A deleted `scan.root` makes *every* scan and every bare
   invocation fail with `AppError::InvalidRoot` → exit code 2 (`main.rs:9-12`).
   There is no degradation path back to Routine.
6. **Empty Routine roots are not an error.** `resolve` returns
   `Ok(Routine(vec![]))`; `discovery.rs:282-291` emits `Info` / `PlatformRoot`
   "no Routine roots are available; run `scan --full` or scan an explicit root".
7. **`learned_root_retention_days as u16` (`policy.rs:46`) is lossless only
   because of the caller's validation.** `config::load` range-checks it to
   0–3650 (`config.rs:105-109`), which fits in `u16`; a `ScanConfig` built
   directly — as every test does — bypasses that check, and a value above 65535
   would wrap silently. Same class of issue for `recency_seconds`: `load`
   rejects 0 (`config.rs:97-101`) because 0 would disable the inactivity guard,
   but `resolve` does not (`policy.rs:22`, `:54`).
8. **A configured `scan.root` silently disables `clean --known`.**
   `main.rs:133-143` takes only the `Routine` arm and yields `Vec::new()` for any
   other scope; `main.rs:174-194` then prints "no bounded cleanup roots are
   known; no cleanup commands were run" and returns **0**. Setting `scan.root`
   is therefore not orthogonal to `--known`.
9. **The JSON `scope` field is derived from CLI flags, not from the resolved
   policy** (`main.rs:496-508`): `full ? "full" : explicit_scope ? "explicit" :
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
`policy.rs:248-349` — 101 of its 349 lines. The five tests:

| Test | Line | Covers |
|---|---|---|
| `macos_global_policy_prunes_protected_usr_and_enumerates_usr_local` | `policy.rs:256` | macOS prune list, `/usr/local` root promotion, `/Library` + `/Applications` *not* pruned (`#[cfg(unix)]`) |
| `linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes` | `policy.rs:274` | the four Linux pseudo-filesystem prunes, and `/usr` absent (`#[cfg(unix)]`) |
| `cli_root_wins_and_bypasses_filters` | `policy.rs:282` | CLI root over configured root over a configured `ignore`/`unignore`; asserts `ScanScope::Explicit`, `DiscoveryFilters::Bypassed`, `recency == 300` |
| `configured_root_wins_over_global` | `policy.rs:305` | configured root yields `ScanScope::Explicit` rather than Global/Routine |
| `routine_state_errors_fall_back_to_seeds_with_one_warning` | `policy.rs:326` | `RecoverableInvalid` and `UnsupportedNewer` both fall back to existing seeds and produce a warning |

Scope behaviour is covered mostly **one level down**, in `discovery.rs`, by
constructing `EffectiveScanPolicy` directly rather than through `resolve` — via
the `explicit_manifests` (`:1059`) and `routine_manifests` (`:1073`) helpers:

- `discovery_reaches_unignored_project_without_entering_ignored_sibling`
  (`discovery.rs:1307`) — the unignore `exception_below` half, negative control
  inline.
- `filters_ignored_parent_keeps_exception_route` (`discovery.rs:1360`).
- `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned`
  (`discovery.rs:1281`) — `ScanScope::Global` + `Active` filters, and
  `user_ignore_prunes` attribution.
- `non_utf8_directory_still_matches_a_broad_ignore_pattern`
  (`discovery.rs:1114`, Linux only) and
  `manifest_name_is_matched_case_insensitively_where_the_volume_is`
  (`discovery.rs:1094`).
- `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests`
  (`discovery.rs:911`), `global_rustup_prune_keeps_adjacent_user_project_reachable`
  (`discovery.rs:855`).

Integration: `tests/end_to_end.rs:124` is the only test that calls
`policy::resolve` end to end, and it uses a **configured** root with
`cli_root: None` (`end_to_end.rs:120-123`). `tests/cli_contract.rs:202` asserts
the JSON `scope` field equals `"explicit"` — but per §7.9 that string comes from
`main.rs:503`, so it does not exercise the resolved scope.

**Honest gaps** — scope behaviour a reader should not assume is covered:

- **The Full branch has no test at all.** Nothing constructs
  `ScanRequest { full: true }`, so `ScanScope::Global` from `resolve` and the
  `global_discovery_policy()` call at `policy.rs:23` are never executed by a
  test; the platform tables are covered only through the pure `unix_policy`
  helper. The `#[cfg(windows)]` body (`policy.rs:164-188`) is untested too.
- `canonical_dedup_roots` and `lexical_identity` (`policy.rs:194-240`) are
  covered only indirectly, via the discovery tests above — the
  unresolvable-root branch (the L16 fix) has no direct test. The `other unix`
  arm (`policy.rs:154-156`) and the `BaseDirs::new() == None` branch
  (`policy.rs:82-84`) are uncovered.
- The retention filter (`policy.rs:112-117`) is not directly tested:
  `retention_days == 0`, boundary equality, and the future-timestamp case.
- `clean --known` root derivation (`main.rs:133-148`), including the
  configured-root interaction in §7.8, has no integration test in
  `tests/cli_contract.rs` or `tests/end_to_end.rs`.

## 9. Review checklist

1. **The `full` early-return is still first.** `policy.rs:20-29` must return
   before `policy.rs:30` is reached. Anything that reorders it changes what
   `--full` means.
2. **An explicit root still bypasses filters, not merely outranks config.**
   `policy.rs:42` must keep `DiscoveryFilters::Bypassed`; `discovery.rs:266`
   turns that into empty ignore/unignore slices.
3. **Every new `GlobalDiscoveryPolicy` field needs a `#[cfg]`-paired
   definition.** The two `global_discovery_policy` bodies are
   `policy.rs:127` (`#[cfg(unix)]`) and `policy.rs:165` (`#[cfg(windows)]`); a
   field added to one and not the other is a platform-specific compile error
   that only one CI job will catch.
4. **New seed names must be absolute-joinable and case variants must be
   deliberate.** `policy.rs:61-75`; the pairs exist because of volume
   case-sensitivity (`discovery.rs:46-63`).
5. **A new retention rule must fail conservative.** `policy.rs:112-117` keeps
   the root when `retention_days == 0` or the timestamp is in the future; ADR
   002 §5 requires the same for any new expiry path.
6. **Do not add a containment check to learned roots here without reading
   `discovery_state.rs:220` first** — `is_broad_root` already exists at
   derivation time; `policy.rs:118` deliberately consumes learned paths
   verbatim.
7. **A `u16`/`u32` or range assumption must be traced to `config::load`.**
   `policy.rs:46` narrows `u32 → u16` on the strength of
   `config.rs:105-109`; `policy.rs:22` relies on `config.rs:97-101` for the
   non-zero recency guard. Test-constructed `ScanConfig` values bypass both.
8. **Any new `AppError` here becomes exit code 2.** `policy.rs:32` and
   `policy.rs:37` are the only sources today; `main.rs:9-12` maps every
   `AppError` to exit 2, and a missing `scan.root` is therefore fatal rather
   than degrading to Routine.
9. **Scope changes are safety changes for `clean --known`.** `main.rs:133-148`
   and `main.rs:174-194` turn this module's output into the bounded root list
   a destructive command receives; re-verify §7.8 and §7.10 before landing.
10. **Add a test for any branch you touch.** The Full branch
    (`policy.rs:20-29`), the Windows body (`policy.rs:164-188`), and the
    retention filter (`policy.rs:112-117`) are currently untested; use
    `unix_policy`'s injected booleans (`policy.rs:141`) for platform tables
    and `routine_roots_from_state`'s injected `StateLoad`
    (`policy.rs:96-100`) for state behaviour — those are the two seams the
    module already provides.
