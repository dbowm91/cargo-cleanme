# Discovery — finding Cargo manifests and accounting for what was missed

> Component deep dive · part of the [architecture overview](overview.md)

`src/discovery.rs` — 1370 lines; production code ends at `:824`, the inline
`#[cfg(test)]` module opens at `:826` and holds 18 `#[test]` functions. It is
step 3 of the scan pipeline: given the scope decided by
[`policy.rs`](04-policy-and-scope.md), walk it for `Cargo.toml` files and report
what could not be seen.

---

## 1. Responsibility

**Owns.** The filesystem walk, and the two outputs that matter: a *candidate
manifest list* (`ManifestDiscovery::manifests` — every `Cargo.toml` that is a real
file, sorted and deduplicated) and the *evidence of incompleteness* that
accompanies it (`diagnostics` — root-level refusals and unreadable subtrees,
categorised; `counters` — which directories were skipped and, by reason, why).

**Does not own.**

- **Workspace resolution.** `manifests` are raw paths; turning them into
  workspaces, members and physical groups is `workspace::resolve_workspaces`
  (`main.rs:347-353`, see [Workspace](07-workspace.md)). Discovery never parses a
  manifest, never calls `cargo metadata`, and never asks Cargo where an output
  directory is.
- **Sizing or activity.** No `mtime`, no byte counts —
  `traverse.rs`/`workspace::analyze_groups` (`main.rs:454-463`).
- **The inactivity decision.** `Active`/`Inactive`/`Uncertain`
  (`domain.rs:40-44`) is decided after physical grouping, on group ownership
  evidence.
- **The scope.** Roots, ignore globs and unignore paths arrive in the
  `EffectiveScanPolicy`; this module only resolves *env-derived* prune paths
  (§3) and applies what it is given.

**Actual `crate::` dependencies** (`discovery.rs:2`, plus one qualified use):

```rust
use crate::{domain::*, error::AppError, progress::ProgressObserver, traverse};
// :248  crate::policy::global_discovery_policy
```

`domain::*` supplies `ScanScope`, `DiscoveryFilters`, `EffectiveScanPolicy`,
`ScanDiagnostic`, `DiagnosticCategory`, `DiagnosticSeverity`, `ScanCounters`
([Domain & errors](01-domain-and-errors.md)). External crates: `globset` (`:3`),
`directories::BaseDirs` (`:86`, `:110`), `dua_core` 4.1 (`Cargo.toml:53`),
`tempfile` in tests only (`:830`). Note the inversion: `policy.rs:242-246` calls
back into `crate::discovery::effective_rustup_home`, so `policy` depends on this
module for exactly one function.

---

## 2. Public surface

| Item | Signature | Contract |
|---|---|---|
| `effective_rustup_home` | `() -> Option<PathBuf>` (`:83`) | `RUSTUP_HOME` if absolute, else `~/.rustup`; `None` if neither is knowable. No filesystem access. |
| `effective_cargo_home` | `() -> Option<PathBuf>` (`:102`) | `CARGO_HOME` if absolute, else `~/.cargo`; `None` if a relative `CARGO_HOME` is set or no home exists. No filesystem access. |
| `cargo_home_prunes` | `() -> Vec<PathBuf>` (`:114`) | exactly `<cargo home>/registry` and `<cargo home>/git`; empty when the cargo home is unknown. |
| `ManifestDiscovery` | struct, 6 public fields (`:221`) | the walk's whole result; see §6. |
| `discover_manifests` | `(&EffectiveScanPolicy, &dyn ProgressObserver) -> Result<ManifestDiscovery, AppError>` (`:234`) | convenience wrapper: delegates with `attribution = false` (`:238`). |
| `discover_manifests_with_attribution` | `(&EffectiveScanPolicy, &dyn ProgressObserver, bool) -> Result<ManifestDiscovery, AppError>` (`:241`) | the real function. `attribution` adds the `--stats` top-level map. |

`discover_manifests` is a one-line delegation (`:238`); all logic is in
`discover_manifests_with_attribution`, and `discover_manifests` exists so that
callers who do not want the per-entry attribution map do not have to pass a
`false`. Callers: `main.rs:321` (with `stats`), `cleanup.rs:786` and
`cleanup.rs:1609` (the destructive path, without), `tests/end_to_end.rs:135`,
`examples/traversal-qualification.rs:35`.

**`impl Filters` is private API.** `struct Filters` at `:13` has no `pub`, so
neither the type nor its methods are reachable from outside the module; the
block is listed here because it *is* the ignore engine:

| Method | Signature | Contract |
|---|---|---|
| `new` | `(&[String], &[PathBuf]) -> Result<Self, AppError>` (`:18`) | compiles the ignore globs into one `GlobSet`; a bad glob is a fatal `AppError::Config` (`:23`). |
| `ignored` | `(&self, &Path) -> bool` (`:31`) | `true` when the lossy string form matches any glob **and** no unignore entry is a prefix of the path (`:40`). Short-circuits to `false` on an empty glob set (`:32`). |
| `exception_below` | `(&self, &Path) -> bool` (`:42`) | `true` when some unignore entry lives *under* the path — the reason a pruned parent is still descended into. |

---

## 3. Environment resolution

### `effective_rustup_home` (`:83-99`)

Precedence: `RUSTUP_HOME` → `~/.rustup`. A configured value is returned **only
if absolute** (`:96` `.is_absolute().then_some(path)`); a relative `RUSTUP_HOME`
yields `None` rather than a guess. If unset, `directories::BaseDirs::new()`'s
`home_dir()` joined with `.rustup` (`:98`), or `None` when there is no home
directory. No syscall: the string/prefix logic is factored into the pure
`effective_rustup_home_from` (`:90-99`) precisely so the precedence table is
testable without touching the process environment (`:833-852`).

### `effective_cargo_home` (`:102-111`)

Same precedence — `CARGO_HOME` → `~/.cargo` — with the same fail-closed handling
of a relative value (`:107-108`): a *relative* `CARGO_HOME` returns `None` and
does **not** fall back to `~/.cargo`. That is the right direction. An unusable
`CARGO_HOME` disables the prune; a fallback would prune the wrong directory.
`cargo_home_prunes` then returns an empty `Vec` (`:116-117`) — the same "prune
nothing rather than prune wrong" outcome, expressed as a no-op instead of an
error.

### `cargo_home_prunes` and why it is correct (`:114-123`)

Exactly two absolute paths: `<cargo home>/registry` and `<cargo home>/git`.
Matching is a component-wise prefix test (`is_cargo_home_pruned`, `:121-123`), so
`/…/registry-old` is not caught by a rule meant for `/…/registry`.

Pruning Cargo's own home is not an optimisation, it is a correctness rule. Both
subtrees hold *real, buildable* `Cargo.toml` files that are not user projects:
`registry/src/*/*/` (extracted third-party crate sources) and
`git/checkouts/*/*/` (git-dependency checkouts). Admitting them would (a) inflate
the candidate set by thousands of entries, each of which becomes a `cargo
metadata` subprocess in `workspace::resolve_workspaces` (`main.rs:347-353`), and
(b) let the tool attribute `target/` output built *by* a dependency to a "project"
it does not own. Test: `manifest_discovery_prunes_cargo_home_registry`
(`:1254-1279`).

### Relationship to `policy::global_discovery_policy`

They do not overlap and are not redundant. The division is by *scope*, not by
subject:

| Prune set | Owner | Consulted when | Applied in |
|---|---|---|---|
| `<cargo home>/{registry,git}` | **this module** (`:269`) | always | every scope |
| `global_only_prunes` (`/System /dev /bin /sbin /usr` on macOS; `/proc /sys /dev /run` on Linux — `policy.rs:143-153`) | `policy.rs` | only when the `Global` scope's roots match the policy's own roots (`:249-256`) | global path only (`:314-316`) |
| `managed_tool_prunes` (the rustup home) | `policy.rs:242-246`, *computed from* `effective_rustup_home` | `global` is true (`:270-276`) | global path only (`:476`, `:573`) |

`policy` is authoritative for *what to walk*; this module for the env-derived
paths. The `canonical_roots(roots) == canonical_roots(candidate.roots)` test at
`:249-256` is a guard: a caller that builds its own `Global` root list (as several
tests do) gets `None` for the policy and therefore no platform or rustup prunes —
the module never silently reinterprets a caller-supplied scope as "the machine".

There is a third, hardcoded system prune for the non-global path: `system_prune`
(`:67-80`) — `/proc /sys /dev /run` on Linux, `/dev` on macOS, always `false` on
Windows. It duplicates the Linux list from `policy.rs:151` in a second place and
is reachable only when a user explicitly points a scan at a system directory.

---

## 4. The walk

The traversal strategy is chosen by one boolean:

```rust
let global = matches!(policy.scope, ScanScope::Global(_) | ScanScope::Routine(_));  // :246
```

| | global/routine | explicit / explicit-roots |
|---|---|---|
| entry point | `discover_global_roots` (`:407`) | `discover_manifests_root` (`:712`), once per root |
| walker | `dua_core::walk_roots` (`:455`) | `dua_core::walk` (`:734`) |
| workers | `traverse::worker_threads().min(8)` (`:296`) | `traverse::worker_threads()` (`:736`) |
| order | `ParentFirst` by default (`:158-164`) | `ParentFirst` (`:737`) |
| options | `skip_metadata` on Unix by default (`:303`) | full metadata (`:738`) |
| attribution | per-entry map (`:537-555`) | never (`top_level_entries` empty, `:365`) |

**Not the standard library.** `std::fs` appears only for `symlink_metadata`
(`:385`, `:601`, `:808`) and `canonicalize` (`:372`, `:430`); every directory
read is `dua_core`'s. Inside dua-core 4.1 a crossbeam work-stealing pool
(`Injector`/`Stealer`/`Unparker`) reads directories, hands newly found
subdirectories to whichever worker is free, and funnels every event into one
channel — effectively a parallel breadth-first traversal with a bounded work
queue (`ENTRY_CHUNK_SIZE = 4`, `MAX_QUEUED_STAT_JOBS = 64`,
`dua-core-4.1.0/src/lib.rs:137,140` keep a wide directory from being read into
memory at once).

**Order.** `Order::ParentFirst` publishes a directory's batch *before* scheduling
its children (`dua-core lib.rs:1409-1421`); `Order::Completion` schedules first
(`:1423-1435`); "sibling order is unspecified in both modes" (`lib.rs:6`). The
stream is parent-before-descendant but otherwise scheduler-dependent — §7.

**Descent decision.** The `descend` predicate (`:464-485` global, `:739-759`
sequential) returns `false` to refuse a subtree. Two properties matter: a refused
directory is **still yielded** (`dua-core lib.rs:8`), which is why discovery can
count the directory it just decided to skip; and the predicate runs for the root
entry too, where `entry_is_within` answers `root.starts_with(prefix)`
(`:181-184`) so a root that *is itself* inside a pruned subtree is not descended.

**Bounds.** No depth limit, no entry-count budget, no time budget — `depth` is
only ever compared against `0` and `1`. The only bounds are the prune predicate,
dua-core's 64-job back-pressure, and the 8-worker cap.
`GLOBAL_DISCOVERY_WORKER_CAP = 8` (`:126`) and
`MAX_DISCOVERY_PROFILE_WORKERS = 32` (`:125`) are *worker* caps, not scan caps.
Termination is the pool's final `Finished` event (`dua-core lib.rs:267-268`);
`RootEvent::Finished` per root only bumps `finished_roots` (`:531-534`). There is
no early exit: an unusable root is skipped (`:335-337`, `:426-428`) while the
remaining roots are still walked.

**Undocumented profiling knobs** (`:292-308`): `CARGO_CLEANME_PROFILE_SUBTREES`
arms them all, then `CARGO_CLEANME_PROFILE_THREADS`,
`CARGO_CLEANME_PROFILE_SKIP_METADATA` and `CARGO_CLEANME_PROFILE_ORDER` apply
`discovery_profile_workers` / `discovery_profile_skip_metadata` /
`discovery_profile_order` (`:128-164`). They are inert unless the arming variable
is set (`discovery_profile_workers(false, Some("2"), 4) == 4`, test `:940`), and
none appears in `docs/`, `man/`, `config.toml` or any `*.md` in the repository.

**Candidates and nested projects.** Nothing stops the walk at a directory that
itself contains a `Cargo.toml`; every nested manifest is returned as its own
candidate, which is correct, because a workspace member is a distinct manifest
that `workspace::resolve_workspaces` must see. The test tree drives a manifest ten
levels deep and finds it (`:975-978`), and the only place nested manifests vanish
is `target/` (`:1184-1197`). A file entry whose name passes `is_manifest_name`
(`:46-63` — case-insensitive on macOS/Windows because those volumes are
case-insensitive by default, exact on Linux) is re-validated with
`fs::symlink_metadata` and must be a regular, non-symlink file before
`observer.manifests_found(1)` (`:597-611` global, `:806-815` sequential).

**Progress batching** (observer contract: no per-entry allocation,
`progress.rs:28-31`): `dirs_visited` every 512 entries (`:563-566`, `:786-789`),
`dirs_pruned` every 64 (`:612-615`, `:801-804`), residuals flushed at `:640-645`
and `:817-822`.

---

## 5. Pruning

Every skip the code implements, with the reason it is auditable:

| Reason | Rule | Global path | Sequential path | Counter | Diagnostic |
|---|---|---|---|---|---|
| Platform system dir | `global_only_prunes` / `system_prune` (`:67-80`) | `:474`, `:571` | `:752`, `:795` | `platform_system_prunes` | none |
| Cargo home `registry`/`git` | `cargo_home_prunes()` | `:475`, `:572` | `:755`, `:796` | `cargo_home_prunes` | none |
| Rustup home (managed tool) | `managed_tool_prunes` | `:476`, `:573` | **not applied** | `rustup_home_prunes` | none |
| `target/`, `.git/`, `.hg/`, `.svn/` | hardcoded, `vcs()` `:64-66` | `:471`, `:574` | `:748`, `:794` | `target_vcs_prunes` | none |
| `scan.ignore` globs | `Filters::ignored` + `exception_below` | `:480-484`, `:575-580` | `:758`, `:797` | `user_ignore_prunes` | none |
| Root is a symlink | `root_is_usable` (`:384-391`) | `:426` | `:335` | — | **Error + `PlatformRoot`** (`:395-400`) |
| Root missing / not a directory | same | `:426` | `:335` | — | **Error + `PlatformRoot`** |
| No Routine roots available | `:282-291` | — | — | — | **Info + `PlatformRoot`, `path: None`** |
| Any symlinked directory | descend refuses (`:465`, `:741-747`) | `:465` | `:745` | not counted as pruned | none |
| `scan.unignore` exception | re-enables descent | `:484`, `:577` | `:758` | — | none |

**Counted vs invisible.** A pruned directory increments `directories_pruned`
(`:592-595`) and exactly one specific counter, chosen by a fixed `else if`
priority: system → cargo → rustup → target/VCS → ignore (`:581-591`). That makes
the five specific counters sum to `directories_pruned` exactly — the invariant the
tests assert (`:884-891`), and the only thing that makes a skip auditable rather
than merely asserted. A directory matching several reasons is attributed to the
first in that order, so a `target/` inside a cargo home counts as a cargo-home
prune.

**What is *not* skipped.** There is no hidden-directory rule — the synthetic test
asserts that `.hidden-project/Cargo.toml` **is** found (`:979-982`). There is no
`node_modules`, no `.cache`, no mount-point and no cross-device rule: the string
`node_modules` does not occur anywhere in `src/`. Do not assume any of them.

**Three honest asymmetries:**

1. **The non-global path keeps only 3 of 8 counters.** `discover_manifests_root`
   maintains per-reason classification logic, but the sequential `ScanCounters`
   literal (`:353-358`) fills only `directories_visited`, `directories_pruned` and
   `manifests_found`. On `scan <dir>` the `--stats` line reports
   `rustup_prunes=0 target_vcs_prunes=0 user_ignore_prunes=0` even when
   directories were pruned: the total is honest, the breakdown is not produced.
2. **A whole root inside a pruned subtree under-reports.** The counter guards
   require `entry.depth > 0` (`:571-573`) while the descend predicate does not
   (`:474-479`, via the `depth == 0` arm of `entry_is_within`). A root that is
   itself inside `/proc` or a cargo-home prune is walked as a single refused
   entry: 1 visited, 0 pruned. The walk is right; the accounting loses the skip.
3. **The rustup prune is global-only** (`:270-276`), so a Routine scan gets none.
   Routine roots are `$HOME/<Projects|src|repos|…>` (`policy.rs:60-79`) plus
   learned roots, so this only bites if a learned root points into a toolchain —
   where `lib/rustlib/src/rust/library/**` holds hundreds of real manifests.

**Explicit scope bypasses `scan.ignore` twice over.** `explicit ||` short-circuits
the descend predicate (`:758`) *and* the counter condition repeats `!explicit`
(`:797`); in addition `policy.rs:42` hands `ScanScope::Explicit` the
`DiscoveryFilters::Bypassed` variant, which becomes empty slices at `:266`.

---

## 6. Attribution and diagnostics

`ManifestDiscovery` field by field (`:221-229`):

| Field | Filled from | Notes |
|---|---|---|
| `manifests: Vec<PathBuf>` | every accepted manifest | `sort()` + `dedup()` before return (`:351-352`, `:662-665`) |
| `visited_entries: u64` | `saturating_add(1)` per yielded entry (`:536`, `:779`) | counts **files and directories**; the traversal total (L6, comment `:559-560`) |
| `pruned_dirs: u64` | the same accumulator as `counters.directories_pruned` (`:592-595` → `:675`, `:704`) | a duplicate of the counter, kept for the report |
| `diagnostics: Vec<ScanDiagnostic>` | three shapes, below | see §5 |
| `counters: ScanCounters` | per-reason accumulation | 8 of the 38 fields are ever set here (`domain.rs:238-246`) |
| `top_level_entries: BTreeMap<PathBuf,u64>` | per-root, per-first-component counts (`:537-555`, `:685-693`) | `--stats` only; empty otherwise (`:365`, `:692`) |

The doc comment calls `top_level_entries` "bounded" (`:227`). The bound is
structural, not a cap: `top_level_component` (`:166-179`) keys each entry by the
first component of its root-relative path (or the entry's own name at
`depth == 1`), so the map holds at most `roots × distinct top-level names` rows.
Small, but not a limit the code enforces.

**Three diagnostic shapes, five sites.**

| Shape | Sites | Notes |
|---|---|---|
| `Info` + `PlatformRoot` + `path: None` | `:282-291` | Routine scope with no available roots; report-only |
| `Error` + `PlatformRoot` + `path: Some(root)` | `:392-403` | root is a symlink (`:386`) or unavailable/not a dir (`:388`) |
| `Warning` + `PermissionDenied` or `Metadata` + `path: Some(root)` | `:519-528`, `:770-776` | an entry the walker could not read; category chosen by `io::ErrorKind` (`:521-525`, `:765-769`) |

**Attribution honesty.** dua-core reports an I/O error *without* the entry it
applies to, so the path recorded is the scan root — the finest honest attribution
available, with the OS error carried in the message (comment `:512-515`, L3). The
resulting coarseness is deliberate: one unreadable subtree marks its whole root
uncertain, and `discovery_state::uncertainty_intersects`
(`discovery_state.rs:107-109`) then protects every learned root intersecting it.

**Diagnostic-stream determinism.** The global path buffers per root and sorts by
`(root index, message)` (`:646-651`, comment `:503-505`, L8), so its stream is
byte-identical across runs. That comment's claim is module-wide but the guarantee
is not: the sequential path pushes in encounter order (`:770-776`) with no
buffering, so two or more errors under one explicit root can be reported in either
order. Manifests and counters are unaffected.

### The `main.rs` contract, and how it can break

```rust
// main.rs:327-339 — the three-category uncertainty filter
let uncertainty: Vec<PathBuf> = discovered.diagnostics.iter()
    .filter(|d| matches!(d.category,
        DiagnosticCategory::PermissionDenied
        | DiagnosticCategory::Metadata | DiagnosticCategory::Vanished))
    .filter_map(|d| d.path.clone()).collect();
```

That vector is what stops a Full scan from retiring a learned root
(`main.rs:387-397` → `reconcile_full`, which retains every old learned root
intersecting `uncertainty`: `discovery_state.rs:113-124`, `:176-181`). Three facts
to hold onto:

- The names are exactly `PermissionDenied`, `Metadata`, `Vanished`
  (`domain.rs:68-75`). This module emits only the first two — `Vanished` appears
  nowhere in it; it is an output/workspace-side category (`output.rs:127`), so its
  presence in the list is defensive breadth, not a discovery responsibility.
- `filter_map(|d| d.path.clone())` silently drops pathless diagnostics, so the
  `Info`/`PlatformRoot` "no Routine roots" note (`:283-290`) contributes nothing
  to uncertainty.
- **The coupling.** A new category meaning "I could not see here" that is not in
  that list is treated as *certainty*: the corresponding learned root becomes
  eligible for retirement, invisibly. The same applies to the two
  `PlatformRoot && Error` checks (`main.rs:383-386` for reconciliation
  completeness, `main.rs:465-469` → exit 1 at `main.rs:560-562`). Three places
  must be edited together.

There is a **stricter** consumer the scan path does not show: `cleanup.rs:795-809`
returns early with `scope_blocked` if the discovery diagnostics vector is
*non-empty at all*, and `cleanup.rs:1613-1618` refuses to re-prove ownership if
the re-walk produced any diagnostic. So on the destructive path a new *benign*
warn-level diagnostic here would silently disable `clean` entirely — the sharpest
edge in the module.

---

## 7. Concurrency

`discovery.rs` contains no `std::thread::spawn` and no `std::thread::scope`.
Threads belong to `dua_core::start_pool`; this module passes a worker count and
consumes one iterator on the calling thread. Every accumulator
(`manifests`, `diagnostics`, the `u64`s, the attribution maps) is a plain `&mut`
parameter, which is why no lock appears.

**Work division.** Directory reads are pool jobs; each finished read schedules
its subdirectories into the shared queue and other workers steal them
(`dua-core lib.rs:4-6`). `traverse::worker_threads()` is
`available_parallelism().clamp(1, 8)` (`traverse.rs:29-33`) — a budget *shared
with* the measurement traversal, capped at 8.

**One pool for all roots, in the global path** (`:455`): N roots share ≤8
workers. **One pool per root, in the sequential path** (`:734-760`): `walk` starts
a fresh pool, and dropping the iterator stops and joins its workers
(`dua-core lib.rs:10`). `ScanScope::ExplicitRoots` from `cleanup.rs:774` therefore
pays one pool startup per selected root.

**Result ordering, honestly.** Results are *not* merged from several threads —
the pool funnels all events into one channel that a single thread drains, so there
is no merge to order. But the *stream* order is scheduler-dependent (sibling order
is unspecified, `dua-core lib.rs:6`). The return value is nevertheless
deterministic because every output is order-insensitive by construction:

| Output | Mechanism |
|---|---|
| `manifests` | `sort()` then `dedup()` (`:662-665`, `:351-352`) |
| `top_level_entries` | `BTreeMap` (`:203`, `:228`) |
| counters | commutative sums, one increment per event |
| `diagnostics` (global) | per-root buffer, then `sort_by(root, message)` (`:646-651`) |
| `diagnostics` (sequential) | encounter order — **not** stable, see §6 |

`examples/traversal-qualification.rs:35` (stage 6) is the harness that runs the
production global path with attribution for benchmarking, and
`synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps` (`:1010-1055`)
asserts the determinism claim across 6 worker counts × 2 metadata modes × 2
orders. The consequence for report stability: the report is stable; the progress
bar's tick order is not; and the explicit-scope diagnostic order is not.

---

## 8. Invariants and edge cases

**Duplicate-free?** Yes, by `sort()` + `dedup()` on the `PathBuf` (`:351-352`,
`:662-665`) — a *spelling* comparison, not canonicalization and not a `HashSet`.
The global path additionally dedups *roots* by canonical identity (`:424-435`)
while keeping each root's user spelling for globs and diagnostics (`:436-441`).
The sequential path canonicalizes nothing, so two spellings of one directory in a
single `ExplicitRoots` scope would produce two differently-spelled manifest paths
that `dedup()` cannot merge. Not reachable from the CLI as wired (`cli`
absolutizes the single root; cleanup passes canonical workspace roots) — but that
is a caller contract, not an invariant of this module.

**Deterministic order?** `manifests` and `top_level_entries`: yes.
Global-path diagnostics: yes, by the sort at `:650`. Sequential-path
diagnostics: **no** (unspecified sibling order, no buffering). Counters are
order-independent.

**Root missing, a file, or a symlink?** All three are refused by `root_is_usable`
(`:384-404`) with `Error` + `PlatformRoot` + the root's path; the root is skipped
(`:336`, `:427`) and the scan continues with `Ok`. The symlink refusal is
deliberate (comment `:379-383`, L7): `Path::is_dir` follows symlinks while the
walker does not, so a symlinked root used to produce a *successful empty* scan for
a directory the user asked for. A root that vanishes between `root_is_usable` and
the walk becomes `Warning` + `Metadata`/`PermissionDenied` (`:511-530`), **not**
`Error` + `PlatformRoot` — it lands in `uncertainty` but does not set
`full_incomplete` (`main.rs:465-469`) nor stop reconciliation
(`main.rs:383-386`).

**Root vs deep-tree permission errors: distinguished, and it matters.** An
unstat-able root is `Error` + `PlatformRoot` — the only thing that makes
`scan --full` exit 1 (`main.rs:465-469` → `:560-562`) and `complete` false
(`main.rs:383-386` → `discovery_state.rs:122-124`). An unreadable subtree is
`Warning` + `PermissionDenied`/`Metadata` attributed to the root, which only feeds
`uncertainty` and the diagnostics summary (`main.rs:517-538`). A platform-level
failure blocks Full reconciliation; a deep hole merely makes the scan uncertain.

**TOCTOU.** Every walker error degrades to a `Warning` and the walk continues
(`:511-530`, `:764-777`) — never a panic, never an `Err`. A directory removed
between enumeration and descent looks exactly like a permission error, because both
share the same two categories and are separated only by `ErrorKind` (`:521-525`).
A `Cargo.toml` removed before the `symlink_metadata` re-validation is dropped
**silently**: `is_ok_and` is false, so it is neither counted nor reported
(`:601-602`) — a real unrecorded incompleteness, bounded by the fact that a project
which just vanished cannot be reaped anyway.

**Hostile or unusual paths.** No `unwrap` on a path. Non-UTF-8 directory names
are handled deliberately: `path.to_string_lossy()` feeds the glob match (`:39`,
comment `:35-38`, L4) so a broad `*` cannot be escaped by an invalid name, while a
pattern that spells the valid part still matches only what it spells (test
`:1112-1132`). Over-long paths surface as `ENAMETOOLONG`, i.e. a `Metadata`
diagnostic. Case collisions are why `is_manifest_name` exists (`:46-63`). All
containment tests use component-wise `Path::starts_with`, so `/usr` never captures
`/usrfoo` (`:122`, `:186`).

**Fatal vs diagnostic.** `Result` is nearly vestigial here. The only reachable
`Err` is `AppError::Config` from a malformed `scan.ignore` glob (`:23`) or a
`GlobSet` build failure (`:27`) — those are the only two `AppError::`
constructions in the file. `discover_manifests_root` is declared
`Result<(), AppError>` but contains no `?` and always returns `Ok(())` (`:823`), so
the `?` at `:349` cannot fire today; `AppError::Scan` is never constructed here.
**Fatal = bad glob syntax; every filesystem condition degrades into a
diagnostic.** (Contrast `policy.rs:32-41`, where an unusable *root* is a fatal
`AppError::InvalidRoot` before discovery ever runs.)

---

## 9. Testing

18 `#[test]` functions (verified `grep -c '#\[test\]' src/discovery.rs`), all in
`discovery.rs:826-1370`. Four are compile-time platform-gated — `:1092`
(`macos`/`windows`), `:1112` (`linux`), `:1151` and `:1337` (`unix`) — so a Linux
or macOS run executes 17 and a Windows run 15.

| Test | Line | Invariant protected |
|---|---|---|
| `rustup_home_uses_absolute_override_or_platform_home_only` | `:833` | precedence table via the pure `effective_rustup_home_from`; relative `RUSTUP_HOME` → `None` |
| `global_rustup_prune_keeps_adjacent_user_project_reachable` | `:855` | the rustup prune spares an adjacent sibling project; the five per-reason counters sum to `directories_pruned`; an explicit scope overrides the global prune |
| `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests` | `:911` | duplicate identical roots collapse by canonical identity |
| `profiling_worker_override_is_opt_in_and_bounded` | `:939` | worker override inert unless profiling; 32-worker cap; `completion` order opt-in only |
| `synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps` | `:959` | 288-leaf + deep + hidden + unignore + symlink fixture is identical across 6×2×2 configurations; `top_level_entries` sums to `visited_entries` |
| `manifest_name_is_matched_case_insensitively_where_the_volume_is` | `:1094` | case-insensitive manifest match |
| `non_utf8_directory_still_matches_a_broad_ignore_pattern` | `:1114` | L4: a non-UTF-8 name cannot escape `*` |
| `directories_visited_counts_directories_not_files` | `:1135` | L6: 4 directories vs 8 entries |
| `symlinked_scan_root_is_refused_instead_of_scanning_nothing` | `:1153` | L7: a symlinked root yields an `Error` diagnostic, not a silent empty scan |
| `discovers_real_manifest_and_prunes_target` / `manifest_discovery_prunes_target_children` / `manifest_discovery_finds_project_without_target` | `:1184`, `:1234`, `:1218` | `target/` descent refused and a nested `target/` manifest excluded, while a project with no `target/` is still a candidate |
| `global_attribution_is_unallocated_when_not_requested` | `:1200` | `attribution = false` leaves the map empty while manifests and counters are unchanged |
| `manifest_discovery_prunes_cargo_home_registry` | `:1254` | `$CARGO_HOME/registry/src` sources are not user projects |
| `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned` | `:1281` | an ignored tree contributes prunes and zero manifests |
| `discovery_reaches_unignored_project_without_entering_ignored_sibling` / `filters_ignored_parent_keeps_exception_route` | `:1307`, `:1360` | both halves of the unignore mechanism, against canonical spellings, and the re-enable path in `Filters` |
| `discovery_rejects_symlink_target` | `:1339` | a symlinked `target` does not stop manifest discovery (eligibility is decided later) |

**Not covered — the gaps a reader should worry about.**

1. **The diagnostic branches of both walkers have no test at all** (`:511-530`,
   `:764-777`). Nothing asserts that an unreadable subtree yields
   `PermissionDenied`/`Metadata` with the root's path, or that the walk survives
   it; the only diagnostic assertion in the module is the symlink-root message
   (`:1175-1180`). This is precisely the code `main.rs:327-339` and
   `cleanup.rs:795-809` depend on.
2. **`root_is_usable`'s "unavailable" branch is untested** (`:387-388`, the
   missing-or-not-a-directory case) — the branch that makes `scan --full` exit 1.
   Only the symlink sibling is covered.
3. **`is_manifest_name` has no negative test on Linux**: nothing asserts a
   lowercase `cargo.toml` is *not* found on a case-sensitive volume. And the
   macOS/Windows test's comment claims "on a case-insensitive volume"
   (`:1095-1096`) while its assertion holds on a case-*sensitive* volume too — it
   verifies the rule, not its own stated premise. That is exactly the hazard the
   project's lesson names: *a green test proves only that its own premises hold.*
4. **The `CARGO_CLEANME_PROFILE_*` wiring is unexercised** — only the pure helpers
   are tested (`:939-955`), never `discover_manifests_with_attribution`'s use of
   them (`:292-308`). Likewise `top_level_entry_map` with multiple or nested
   roots is unasserted (`:199-219`; the accounting sum is checked for a single
   root only, `:1048-1052`).
5. **Process-global environment mutation.** `manifest_discovery_prunes_cargo_home_registry`
   writes `CARGO_HOME` with `unsafe { std::env::set_var }` and restores it after
   the call (`:1263-1276`). The harness runs tests concurrently, so this races any
   test in the same binary that reads `CARGO_HOME` or depends on
   `cargo_home_prunes()` — a flakiness risk, not a false green, since the
   assertion is still meaningful. `scripts/check-fixture-portability.py` scans
   `src/**/*.rs` but rejects only literal PATH separators and unjustified POSIX
   shebang fixtures; it has no rule about env mutation or platform guards, so
   nothing catches this statically. The `#[cfg]` guards elsewhere in this module
   are the right form — compile-time, not a runtime `if` that silently passes.
6. **Windows is the thinnest lane**: 15 of 18 tests, including both symlink tests,
   with `system_prune` (`:67-80`) unconditionally `false` and `skip_metadata`
   always `false` (`:303`).

---

## 10. Review checklist

1. **Does any new `DiagnosticCategory` mean "I could not see here"?** Then it must
   be added to `main.rs:331-336` *and*, if it should block Full reconciliation, to
   both `PlatformRoot && Error` checks (`main.rs:383-386`, `main.rs:465-469`).
   Missing the first silently converts a hole in the scan into certainty about
   learned roots. Before adding anything at all, check `cleanup.rs:795-809` and
   `cleanup.rs:1613-1618` — those treat *any* non-empty diagnostic vector as a hard
   block on cleanup.
2. **Does a new prune reason have a `ScanCounters` field and a position in the
   `else if` chain at `:581-591`?** That chain's ordering plus
   `directories_pruned` is what makes the identity asserted at `:884-891` hold; a
   prune counted in `pruned` but absent from the chain breaks it silently.
3. **Are the `depth > 0` guards at `:571-573` and `:793` still right for a new
   reason?** They are why a root inside a pruned subtree reports 0 prunes (§5).
   Fixing that means touching both the counter and the descend predicate, which
   use different guards today (`:474-479` vs `:571-573`).
4. **Is a new prune reachable in both paths?** The sequential `ScanCounters`
   literal (`:353-358`) fills only 3 fields and `managed_tool_prunes` is
   global-only (`:270-276`), so a reason added to one path only looks implemented
   in `--stats` on `scan <dir>` and reports zeros.
5. **Is every prune still silent?** Only the two `root_is_usable` outcomes and the
   empty-Routine notice emit diagnostics (`:283-291`, `:395-400`). A prune needing
   a user-visible explanation belongs in the report, not in a counter.
6. **Did a change break root canonicalization?** Only the global path dedups roots
   by canonical identity before keeping the user spelling (`:424-441`); the
   sequential path canonicalizes nothing, so its dedup is spelling-based (§8).
   Either change alters duplicate behaviour for `ExplicitRoots`.
7. **Is a diagnostic stream still deterministic on both paths?** The global path
   sorts per root (`:646-651`, L8); the sequential path does not, so the L8
   comment at `:503-505` overstates the guarantee module-wide.
8. **Does a new manifest test assert a fixture property it does not establish?**
   The case-insensitivity test (`:1094-1108`) is the current example of a green
   test that does not prove its stated premise. Relatedly: did a change touch
   `is_manifest_name` or `Filters::ignored`? Both encode platform assumptions with
   compile-time-gated tests (`:1094`, `:1114`); a runtime `cfg!` in their place
   turns a skipped test into a passing one.
9. **Are the four undocumented `CARGO_CLEANME_PROFILE_*` knobs still inert by
   default?** They are gated on `CARGO_CLEANME_PROFILE_SUBTREES` (`:292-308`,
   `:128-141`) and appear in no doc, man page, or config comment — so making one
    unconditional changes production traversal with no documented way back.
