# Discovery — finding Cargo manifests and accounting for what was missed

> Component deep dive · part of the [architecture overview](overview.md)

`src/discovery.rs` — 2647 lines; production code ends at `:1145`, the inline
`#[cfg(test)]` module opens at `:1147` and holds 41 `#[test]` functions. It is
step 3 of the scan pipeline: given the scope decided by
[`policy.rs`](04-policy-and-scope.md), walk it for `Cargo.toml` files and report
what could not be seen.

---

## 1. Responsibility

**Owns.** The filesystem walk, and the outputs that matter: a *candidate
manifest list* (`ManifestDiscovery::manifests` — every `Cargo.toml` that is a real
file, sorted and deduplicated) and the *evidence of incompleteness* that
accompanies it (`diagnostics` — root-level refusals and unreadable subtrees,
categorised; `counters` — which directories were skipped and, by reason, why;
`coverage_complete` — whether the walk saw everything it was asked to see).
Since M018 it also owns the **per-root coverage ledger**
(`ManifestDiscovery::root_coverage`, `:323-328`) and the **verification pass**
(`verify_root_coverage`, `:586-622`) that measures it — see §6.

**Does not own.**

- **Workspace resolution.** `manifests` are raw paths; turning them into
  workspaces, members and physical groups is `workspace::resolve_workspaces`
  (`main.rs:1552`, see [Workspace](07-workspace.md)). Discovery never parses a
  manifest, never calls `cargo metadata`, and never asks Cargo where an output
  directory is.
- **Sizing or activity.** No `mtime`, no byte counts —
  `traverse.rs`/`workspace::analyze_groups` (`main.rs:1701`).
- **The inactivity decision.** `Active`/`Inactive`/`Uncertain`
  (`domain.rs:40-44`) is decided after physical grouping, on group ownership
  evidence.
- **The scope.** Roots, ignore globs and unignore paths arrive in the
  `EffectiveScanPolicy`; this module resolves the *env-derived* prune paths (§3),
  applies the platform policy's own prune lists when the scope is the machine's
  `Global` scope, and applies what it is given.

**Actual `crate::` dependencies** (`discovery.rs:2`, plus one qualified use):

```rust
use crate::{domain::*, error::AppError, progress::ProgressObserver, traverse};
// :337  crate::policy::global_discovery_policy
```

`domain::*` supplies `ScanScope`, `DiscoveryFilters`, `EffectiveScanPolicy`,
`ScanDiagnostic`, `DiagnosticCategory`, `DiagnosticSeverity`, `ScanCounters`
([Domain & errors](01-domain-and-errors.md)). External crates: `globset` (`:3`),
`directories::BaseDirs` (`:148`, `:172`), `dua_core` 4.1 (`Cargo.toml:67`),
`tempfile` in tests only (`:1005`). Note the inversion: `policy.rs:848-852` calls
back into `crate::discovery::effective_rustup_home`, so `policy` depends on this
module for exactly one function.

---

## 2. Public surface

| Item | Signature | Contract |
|---|---|---|
| `effective_rustup_home` | `() -> Option<PathBuf>` (`:145`) | `RUSTUP_HOME` if absolute, else `~/.rustup`; `None` if neither is knowable. No filesystem access. |
| `effective_cargo_home` | `() -> Option<PathBuf>` (`:164`) | `CARGO_HOME` if absolute, else `~/.cargo`; `None` if a relative `CARGO_HOME` is set or no home exists. No filesystem access. |
| `cargo_home_prunes` | `() -> Vec<PathBuf>` (`:176`) | exactly `<cargo home>/registry` and `<cargo home>/git`; empty when the cargo home is unknown. |
| `ManifestDiscovery` | struct, 8 public fields (`:307-324`) | the walk's whole result; see §6. |
| `verify_root_coverage` | `(&[PathBuf], &EffectiveScanPolicy, &dyn ProgressObserver) -> Result<(Vec<RootCoverage>, Vec<ScanDiagnostic>), AppError>` (`:586-622`) | M018: re-walks each root as its own walk root and reports a per-root coverage verdict plus the diagnostics that pass produced. It collects **no manifests** — it is a coverage measurement, not a second discovery. |
| `discover_manifests` | `(&EffectiveScanPolicy, &dyn ProgressObserver) -> Result<ManifestDiscovery, AppError>` (`:329``) | convenience wrapper: delegates with `attribution = false` (`:329`). |
| `discover_manifests_with_attribution` | `(&EffectiveScanPolicy, &dyn ProgressObserver, bool) -> Result<ManifestDiscovery, AppError>` (`:336``) | the real function. The third argument is `attribution` (`:336`); it adds the `--stats` top-level map. |

`discover_manifests` is a one-line delegation (`:329`); all logic is in
`discover_manifests_with_attribution`, and `discover_manifests` exists so that
callers who do not want the per-entry attribution map do not have to pass a
`false`. Callers: `main.rs:1510-1513` (with `options.stats`),
`cleanup.rs:1543`, `cleanup.rs:2039` and `cleanup.rs:3196` (the destructive
paths, without), `tests/end_to_end.rs:135`,
`examples/traversal-qualification.rs:35`.

`verify_root_coverage` has exactly one caller: `main.rs:831-858`, the M018
verification pass, and it runs **only** when the platform walk was incomplete.

**`impl Filters` is private API.** `struct Filters` at `:13` has no `pub`, so
neither the type nor its methods are reachable from outside the module; the
block is listed here because it *is* the ignore engine:

| Method | Signature | Contract |
|---|---|---|
| `new` | `(&[String], &[PathBuf]) -> Result<Self, AppError>` (`:18`) | compiles the ignore globs into one `GlobSet`; a bad glob is a fatal `AppError::Config` (`:18`), a `GlobSet` build failure likewise (`:18`). |
| `directly_ignored` | `(&self, &Path) -> bool` (`:38`) | `true` when the lossy string form of *this* path matches any glob. The only glob match site, so it also pins the lossy-match policy. Short-circuits to `false` on an empty glob set (`:617-619`). |
| `inherits_ignore` | `(&self, &Path) -> bool` (`:52`) | `true` when `p` **or any ancestor** is matched directly (`:1749-1751`). This is what carries a literal ignore rule down to the children it does not textually name — the C019 correction. |
| `disposition` | `(&self, &Path) -> Disposition` (`:68`) | the whole filter decision: `ReIncluded` if `p` is at/below an unignore entry (`:1749-1751`), else `Pruned`/`PassThrough` by whether an unignore entry lies at or below `p` (`:1749-1751`), else `Included` (`:103-105`). Bounded by path depth × rule count, never by tree size. |

`enum Disposition` (`:102-107`) has exactly **four** variants, and the
distinction between the two excluded ones is the point of the type (`:89-101`):

| Variant | Meaning | Effect on the walk |
|---|---|---|
| `Included` | in scope | descended; children inherit the state |
| `Pruned` | excluded, and no exception lies below | subtree refused |
| `PassThrough` | excluded, but an exact `unignore` path lies at or below | descended *only* as the route to that exception |
| `ReIncluded` | in scope because it is at or below an exact `unignore` path | descended; the whole subtree is in scope |

`Pruned` and `PassThrough` are both "excluded" and must not be merged: only
`PassThrough` is entered, and only far enough to reach the exception.

---

## 3. Environment resolution

### `effective_rustup_home` (`:145-150`)

Precedence: `RUSTUP_HOME` → `~/.rustup`. A configured value is returned **only
if absolute** (`:158` `.is_absolute().then_some(path)`); a relative `RUSTUP_HOME`
yields `None` rather than a guess. If unset, `directories::BaseDirs::new()`'s
`home_dir()` joined with `.rustup` (`:160`), or `None` when there is no home
directory. No syscall: the string/prefix logic is factored into the pure
`effective_rustup_home_from` (`:152-161`) precisely so the precedence table is
testable without touching the process environment (`:1423-1442``).

### `effective_cargo_home` (`:164-173`)

Same precedence — `CARGO_HOME` → `~/.cargo` — with the same fail-closed handling
of a relative value (`:167-170`): a *relative* `CARGO_HOME` returns `None` and
does **not** fall back to `~/.cargo`. That is the right direction. An unusable
`CARGO_HOME` disables the prune; a fallback would prune the wrong directory.
`cargo_home_prunes` then returns an empty `Vec` (`:370-372`) — the same "prune
nothing rather than prune wrong" outcome, expressed as a no-op instead of an
error.

### `cargo_home_prunes` and why it is correct (`:176-181`)

Exactly two absolute paths: `<cargo home>/registry` and `<cargo home>/git`.
Matching is a component-wise prefix test (`is_cargo_home_pruned`, `:183-185`), so
`/…/registry-old` is not caught by a rule meant for `/…/registry`.

Pruning Cargo's own home is not an optimisation, it is a correctness rule. Both
subtrees hold *real, buildable* `Cargo.toml` files that are not user projects:
`registry/src/*/*/` (extracted third-party crate sources) and
`git/checkouts/*/*/` (git-dependency checkouts). Admitting them would (a) inflate
the candidate set by thousands of entries, each of which becomes a `cargo
metadata` subprocess in `workspace::resolve_workspaces` (`main.rs:1502-1508`), and
(b) let the tool attribute `target/` output built *by* a dependency to a "project"
it does not own. Test: `manifest_discovery_prunes_cargo_home_registry`
(`:2070-2095``).

### Relationship to `policy::global_discovery_policy`

The division is by *provenance* and *scope*, not by subject — and the two
system lists overlap on Linux:

| Prune set | Owner | Consulted when | Applied in |
|---|---|---|---|
| `<cargo home>/{registry,git}` | **this module** (`:364``) | always | every scope |
| `global_only_prunes` (`/System /dev /bin /sbin /usr` on macOS; `/proc /sys /dev /run` on Linux — `policy.rs:736-746`) | `policy.rs` | only when the scope is `Global` **and** its roots match the policy's own roots (`:336-345`) | global path only (`:597`, `:679`) |
| `managed_tool_prunes` (the rustup home) | `policy.rs:848-852`, *computed from* `effective_rustup_home` | same guard (`:359-365`) | global path only (`:599`, `:681`) |
| `system_prune` (hardcoded: `/proc /sys /dev /run` on Linux, `/dev` on macOS, `false` on Windows) | **this module** (`:617-630`) | always | sequential path only (`:617`, `:617`) |

`policy` is authoritative for *what to walk*; this module for the env-derived
paths. The `canonical_roots(roots) == canonical_roots(&candidate.roots)` test at
`:346`` is a guard: a caller that builds its own `Global` root list (as several
tests do) gets `None` for the policy and therefore no platform or rustup prunes —
the module never silently reinterprets a caller-supplied scope as "the machine".

There is a fourth, hardcoded system prune for the non-global path:
`system_prune` (`:129-142`). It duplicates the Linux list from `policy.rs:744`
in a second place and is reachable only when a user explicitly points a scan at
a system directory. The two lists are not the same rule: macOS's `/usr` and
`/System` are pruned only by the platform policy (and therefore only in a
matching `Global` scope), while the hardcoded rule is never consulted in the
global path at all.

---

## 4. The walk

The traversal strategy is chosen by one boolean:

```rust
let global = matches!(policy.scope, ScanScope::Global(_) | ScanScope::Routine(_));  // :335
```

| | global/routine | explicit / explicit-roots |
|---|---|---|
| entry point | `discover_global_roots` (`:625``) | `discover_manifests_root` (`:625`), once per root |
| walker | `dua_core::walk_roots` (`:706``) | `dua_core::walk` (`:1237`) |
| workers | `traverse::worker_threads().min(GLOBAL_DISCOVERY_WORKER_CAP)` (`:392``) | `traverse::worker_threads()` (`:1029``) |
| order | `Order::ParentFirst` by default (`:224-230`) | `Order::ParentFirst` (`:1030``) |
| options | `skip_metadata` on Linux and macOS by default (`:410-416`) | full metadata (`:410`) |
| attribution | per-entry map (`:777-792``) | never (`top_level_entries` empty, `:453``) |

**Not the standard library.** `std::fs` appears only for `symlink_metadata`
(`:476``, `:490``, `:844``, `:1095``) and `canonicalize` (`:464``, `:669``); every
directory read is `dua_core`'s. (One test premise reads a directory directly to
*establish* that a child is unreadable, `:1182-1186``.) Inside dua-core 4.1 a
crossbeam work-stealing pool (`Injector`/`Stealer`/`Unparker`) reads directories,
hands newly found subdirectories to whichever worker is free, and funnels every
event into one channel — effectively a parallel breadth-first traversal with a
bounded work queue (`ENTRY_CHUNK_SIZE = 4`, `MAX_QUEUED_STAT_JOBS = 64`,
`dua-core-4.1.0/src/lib.rs:137,140` keep a wide directory from being read into
memory at once).

**Order.** `Order::ParentFirst` publishes a directory's batch *before* scheduling
its children (`dua-core lib.rs:1409-1421`); `Order::Completion` schedules first
(`:1423-1435`); "sibling order is unspecified in both modes" (`lib.rs:6`). The
stream is parent-before-descendant but otherwise scheduler-dependent — §7.

**Descent decision.** The `descend` predicate (`:715-740`` global, `:1076-1096`
sequential) returns `false` to refuse a subtree. Two properties matter: a refused
directory is **still yielded** (`dua-core lib.rs:8-9`), which is why discovery can
count the directory it just decided to skip; and the predicate runs for the root
entry too, where `entry_is_within` answers `root.starts_with(prefix)`
(`:243-253`) so a root that *is itself* inside a pruned subtree is not descended.

The global closure's checks run in a fixed order — not a directory or a symlink
(`:716-718``), then `target`/VCS at `depth > 0` (`:722-724``), then the system,
cargo and rustup prunes (`:725-730``), then the user filter, which refuses only a
`Pruned` disposition (`:104-112`). The sequential closure is the same order with
the hardcoded `system_prune` in place of the platform list (`:129-142`).

**The global-only system prune re-opens for a root inside it.**
`entry_is_system_pruned` (`:279-283`) exempts a walk root that sits *strictly
inside* a global-only system prune, so macOS's `/usr/local` exception root is
walked instead of being swallowed by `/usr` — `entry_is_within` matches on
`parent_path`, so every entry under `/usr/local` is "within" `/usr`. Only
global-only prunes get the exemption: a Cargo or rustup home stays refused even
when a root sits inside it (`:725-730``). Tests: `a_root_inside_a_global_prune_reopens_that_prune`
(`:1270``) and the production-walk form `global_walk_descends_into_a_root_that_reopens_a_system_prune`
(`:1358``).

**Bounds.** No depth limit, no entry-count budget, no time budget — `depth` is
only ever compared against `0` and `1`. The only bounds are the prune predicate,
dua-core's 64-job back-pressure, and the 8-worker cap.
`GLOBAL_DISCOVERY_WORKER_CAP = 8` (`:188`) and
`MAX_DISCOVERY_PROFILE_WORKERS = 32` (`:187`) are *worker* caps, not scan caps.
Termination is the pool's final `Finished` event, which ends the iterator rather
than being yielded (`dua-core lib.rs:267-268`); `dua_core::RootEvent::Finished`
is *per root* and only bumps `finished_roots` (`:772-775`). There is no early
exit: an unusable root is skipped (`:420-424``, `:648-667``) while the remaining
roots are still walked.

**Undocumented profiling knobs** (`:388-398``): `CARGO_CLEANME_PROFILE_SUBTREES`
arms them all, then `CARGO_CLEANME_PROFILE_THREADS`,
`CARGO_CLEANME_PROFILE_SKIP_METADATA` and `CARGO_CLEANME_PROFILE_ORDER` apply
`discovery_profile_workers` / `discovery_profile_skip_metadata` /
`discovery_profile_order` (`:220-226`). They are inert unless the arming variable
is set (`discovery_profile_workers(false, Some("2"), 4) == 4`, test `:1532``), and
none appears in `docs/`, `man/`, `config.toml` or `CHANGELOG.md` — the only
mention in the repository is a cross-reference in the
[traverse deep dive](08-traverse.md). Armed, they also print per-phase timing
lines to stderr (`:868-892``, `:944-952``, `:953-964``, `:983-989``).

**Candidates and nested projects.** Nothing stops the walk at a directory that
itself contains a `Cargo.toml`; every nested manifest is returned as its own
candidate, which is correct, because a workspace member is a distinct manifest
that `workspace::resolve_workspaces` must see. The test tree drives a manifest ten
levels deep and finds it (`:1567-1570``), and the only place nested manifests
vanish is `target/` (`:1824-1837``). A file entry whose name passes
`is_manifest_name` (`:115-125` — case-insensitive on macOS/Windows because those
volumes are case-insensitive by default, exact on Linux) is re-validated with
`fs::symlink_metadata` and must be a regular, non-symlink file before
`observer.manifests_found(1)` (`:840-861`` global, `:1093-1112`` sequential).

**The manifest's parent must be in scope (H-1).** A manifest directly under a
`PassThrough` directory is *excluded*, even though the walk entered that
directory: only `Included` and `ReIncluded` parents are in scope
(`:850-858`` global, `:1101-1109`` sequential). A `PassThrough` directory is entered
solely as the route to an exact `unignore` path below it, and a manifest beside
that exception is not the exception.

**Progress batching** (observer contract: no per-entry allocation,
`progress.rs:28-31`): `dirs_visited` every 512 entries (`:671-674`, `:924-927`),
`dirs_pruned` every 64 (`:865-868`, `:1089-1092`), residuals flushed at `:898-903`
and `:1115-1120``.

---

## 5. Pruning

Every skip the code implements, with the reason it is auditable:

| Reason | Rule | Global path | Sequential path | Counter | Diagnostic |
|---|---|---|---|---|---|
| Platform system dir | `global_only_prunes` (`policy.rs:736-746`) | descend `:597`, count `:679` | **not applied** | `platform_system_prunes` | none |
| Platform system dir (hardcoded) | `system_prune` (`:129-142`) | **not applied** | descend `:129`, count `:129` | — (the sequential literal has no such field) | none |
| Cargo home `registry`/`git` | `cargo_home_prunes()` (`:180`) | descend `:180`, count `:180` | descend `:180`, count `:180` | `cargo_home_prunes` | none |
| Rustup home (managed tool) | `managed_tool_prunes` (`:368-374`) | descend `:607`, count `:607` | **not applied** | `rustup_home_prunes` | none |
| `target/`, `.git/`, `.hg/`, `.svn/` | hardcoded, `vcs()` (`:126-128`) | descend `:722``, count `:810`` | descend `:1041``, count `:1081`` | `target_vcs_prunes` | none |
| `scan.ignore` globs | `Filters::disposition` == `Pruned` | descend `:104`, count `:104-108` | descend `:104`, count `:104` | `user_ignore_prunes` | none |
| Root is a symlink | `root_is_usable` (`:502-515`) | `:502` | `:502` | — | **Error + `PlatformRoot`** (`:502-515`) |
| Root missing / not a directory | same | `:651`` | `:421`` | — | **Error + `PlatformRoot`** |
| No Routine roots available | `:378-387`` | — | — | — | **Info + `PlatformRoot`, `path: None`** |
| Any symlinked directory | descend refuses (`:716-718``, `:1038-1040``) | `:716`` | `:1038`` | not counted as pruned | none |
| `scan.unignore` exception | `ReIncluded` subtree, or `PassThrough` ancestry | `:105`, `:105-109` | `:105`, `:105` | — | none |

**Counted vs invisible.** A pruned directory increments `directories_pruned`
(`:835-838``) and exactly one specific counter, chosen by a fixed `else if`
priority: system → cargo → rustup → target/VCS → ignore (`:824-834``). That makes
the five specific counters sum to `directories_pruned` exactly — the invariant the
tests assert (`:1475-1482``, `:2386-2394``), and the only thing that makes a skip
auditable rather than merely asserted. A directory matching several reasons is
attributed to the first in that order, so a `target/` inside a cargo home counts
as a cargo-home prune.

**What is *not* skipped.** There is no hidden-directory rule — the synthetic test
includes `.hidden-project/Cargo.toml` in its fixture (`:1571-1574``) and its
assertion requires it in the result (`:1636-1639``), so a hidden project **is**
found. There is no
`node_modules`, no `.cache`, no mount-point and no cross-device rule *in this
module*: the string `node_modules` never occurs in `discovery.rs` (it occurs
elsewhere in the crate — `discovery_state.rs:202` refuses to admit such a root as
learned state — which is a different rule at a different layer). Do not assume
any of them here.

**Three honest asymmetries:**

1. **The non-global path keeps only 3 of 8 counters.** `discover_manifests_root`
   maintains per-reason classification logic (`:1079-1092``), but the sequential
   `ScanCounters` literal (`:461-466``) fills only `directories_visited`,
   `directories_pruned` and `manifests_found`. On `scan <dir>` the `--stats` line
   (`domain.rs:381`) reports `rustup_prunes=0 target_vcs_prunes=0
   user_ignore_prunes=0` even when
   directories were pruned: the total is honest, the breakdown is not produced.
2. **A whole root inside a pruned subtree under-reports — except for a user
   ignore, and only in the global path.** In the global path the counter guards
   for the four internal reasons require `entry.depth > 0` (`:807-810``) while the
   descend predicate does not (`:725-730``, via the `depth == 0` arm of
   `entry_is_within`). A root that is itself inside a cargo-home prune is walked
   as a single refused entry: 1 visited, 0 pruned. The walk is right; the
   accounting loses the skip. The user-ignore reason deliberately does **not**
   carry that guard (`:819-823``). Since C019 a scan root can inherit exclusion
   from a literal ignored ancestor and be refused outright, and a scan that
   returned nothing while reporting zero prunes would be unexplainable — so that
   one reason is counted at `depth == 0` as well. The **sequential** path does
   not make that exception: its `path != root_path` guard (`:1080``) covers the
   ignore reason too, so the same tree reports 0 prunes there. The two paths
   therefore disagree, and only the global path is tested (`:2616-2634``).
3. **The rustup prune needs the machine's own `Global` policy** (`:341-350`,
   `:365-371``), so a Routine scan gets none. Routine roots are
   `$HOME/<Projects|projects|Developer|dev|Code|code|src|repos|Repos|GitHub|github|workspace|workspaces>`
   (`policy.rs:60-79`) plus learned roots, so this only bites if a learned root
   points into a toolchain — where `lib/rustlib/src/rust/library/**` holds
   hundreds of real manifests.

**What counts as a user-ignore prune.** Only `Disposition::Pruned`
(`:102-107`, decided at `:68-87`). A `PassThrough` ancestor was *entered* — it
is the route to an exception — and a `ReIncluded` directory is in scope, so
neither is a skip. The sibling that stays excluded beneath the same ancestor is,
because refusing it is the whole correction (C019). Before C019 this counter
also counted exception ancestors, which made it report skips the walk had not
performed; the invariant `directories_pruned ==` the sum of the five specific
counters (`:1475-1482``, `:2386-2394``) is unchanged and is the reason the two
sites are derived from one disposition rather than two predicates.

**Explicit scope bypasses `scan.ignore` at three levels.** `explicit ||`
short-circuits the descend predicate (`:1051``) *and* the counter condition repeats
`!explicit` (`:938`); in addition `policy.rs:42` hands `ScanScope::Explicit` the
`DiscoveryFilters::Bypassed` variant, which becomes empty slices at `:361``.

---

## 6. Attribution and diagnostics

`ManifestDiscovery` field by field (`:307-324`):

| Field | Filled from | Notes |
|---|---|---|
| `manifests: Vec<PathBuf>` | every accepted manifest | `sort()` + `dedup()` before return (`:439-440``, `:954-957``) |
| `visited_entries: u64` | `saturating_add(1)` per yielded entry (`:776``, `:1063``) | counts **files and directories**; the traversal total (L6, comment `:795-796``) |
| `pruned_dirs: u64` | the same accumulator as `counters.directories_pruned` (`:835-838`` → `:967``, `:993``) | a duplicate of the counter, kept for the report |
| `diagnostics: Vec<ScanDiagnostic>` | three shapes, below | see the table below |
| `counters: ScanCounters` | per-reason accumulation | 8 of the 34 fields are ever set here (`domain.rs:308-315`) |
| `top_level_entries: BTreeMap<PathBuf,u64>` | per-root, per-first-component counts (`:777-792``, `:979-982``) | `--stats` only; empty otherwise (`:473``) |
| `coverage_complete: bool` | `false` on any unusable root or any walk error (`:617`, `:617`, `:617`, `:617`) | the fail-closed coverage flag (C029) |
| `root_coverage: Vec<RootCoverage>` | one entry per walk root, built **before** the walk (`:644-678`) and corrected by the walk's error events (`:915-927`) | M018: the per-root coverage verdict; see the subsection below |

The doc comment calls `top_level_entries` "bounded" (`:996`). The bound is
structural, not a cap: `top_level_component` (`:228-241`) keys each entry by the
first component of its root-relative path (or the entry's own name at
`depth == 1`), so the map holds at most `roots × distinct top-level names` rows.
Small, but not a limit the code enforces.

**Three diagnostic shapes, four sites.**

| Shape | Sites | Notes |
|---|---|---|
| `Info` + `PlatformRoot` + `path: None` | `:381-390` | Routine scope with no available roots; report-only |
| `Error` + `PlatformRoot` + `path: Some(root)` | `:508-518` | root is a symlink (`:508-512`) or unavailable/not a dir (`:381-382`) |
| `Warning` + `PermissionDenied` / `Vanished` / `Metadata` + `path: None` | `:935-949`, `:1131-1149` | aggregated traversal errors; dua-core supplies no failing child path |

**Attribution honesty (C029).** dua-core 4.1's `RootEvent::Entry(io::Result<Entry>)`
carries a bare `io::Error` with **no path** (`dua-core-4.1.0/src/lib.rs:276-281`).
A walk error therefore cannot be attributed to the subtree that produced it, and
recording the selected root as the diagnostic's path would falsely say that the
whole root was unreadable. The module's answer is attribution at the **root**
level: `record_walk_error` (`:261-268`) accumulates `(events, denied, vanished)`
per root index into `walk_errors: HashMap<usize, (u64, u64, u64)>` (`:760``), and
the drain at `:900-943`` emits **at most one** aggregate diagnostic per selected
root, sorted by root index (`:901``), with `path: None` and the root named in the
message text (`:907-910``). Diagnostic memory is bounded by the number of selected
roots, not by the number of errors: the test
`more_than_one_thousand_walk_errors_stay_one_bounded_root_record` (`:1154-1164``)
pins 1001 errors into one record.

Unknown-location loss fails closed. `coverage_complete` is set `false` at the
moment the error is observed (`:638`, `:912`), so `main.rs:1523-1527` refuses to
treat the walk as complete, `discovery_state.rs:220-222` refuses to publish a
Full generation from it, and `main.rs:1712` with `main.rs:1883` makes
`scan --full` exit 1. An incomplete walk is positive inventory only.

**Diagnostic-stream determinism.** Both walkers emit at most one summary per
selected root, and the roots are visited in a fixed order, so the diagnostic
vector is deterministic on **both** paths. The global path additionally sorts its
walk-error summaries by root index (`:901``); the sequential path emits each
root's summary as that root's walk finishes, in root order.

### The M018 coverage ledger and verification pass

`coverage_complete` is a single boolean for the whole walk. M018 needed a
finer question: *which* root was unread? The answer is the **coverage ledger**,
`ManifestDiscovery::root_coverage` (`:323-328`).

| Field of `RootCoverage` (`domain.rs:219-236`) | Meaning |
|---|---|
| `root` | the root as it was spelled for the walk |
| `canonical_root` | its canonical identity, used to match a learned root to its verdict |
| `complete` | whether this root's **own** traversal completed |
| `error_events` | traversal errors attributed to this root |
| `permission_denied` | of those, the permission-denied ones |
| `vanished` | of those, the "vanished" (not found) ones |
| `detail` | why the root is not complete, in operator-facing words; `None` when it is |

**The ledger is built before the walk** (`:644-678`), not after it, so a root
that never reaches the walker at all still has a verdict. An unusable root —
one that is a symlink or is not a directory — is pushed as an *incomplete*
entry with a `detail` naming the reason (`:660-676`) and never enters the walk
(`:677`). A usable root enters as a complete entry with `detail: None`
(`:668-676`). The walk's error events then correct the entries they belong to
(`:915-927`).

**The ledger is keyed by the root's own identity, not the walk's index.** The
walk reports an error by *walk-root index*; the ledger is keyed by the root.
`walk_to_coverage` (`:690-695`) maps one to the other, and the mapping is
built from the de-duplicated canonical root list (`:679-693`) so a root that
canonicalizes onto an already-present root does not get two entries.

**The granularity of the verdict is the root, and that is a property of the
error event, not a choice.** `dua-core`'s error carries a bare `io::Error` with
no child path — only the root index — so a subtree's incompleteness *cannot* be
attributed below the root that contains it. The comment at `:911-914` says so
explicitly.

**`verify_root_coverage` (`discovery.rs:586-622`) is the measurement.** It
re-walks each root as its **own** walk root, so the existing per-walk-root
attribution decides — per root — whether that root's subtree was fully read.
It is a *coverage measurement*, not a second discovery: it collects no
manifests, because the platform walk already discovered them (`:581-585`).

**The pass uses the same prune and filter sets as the platform walk** —
`global_discovery_policy()`, `cargo_home_prunes()`, the policy's managed-tool
and global-only prunes, and the compiled `Filters` (`:13-16`). The knobs
come from `global_walk_knobs()` (`:549-570`), a **single** function shared by
both walks (`:597`), so the two cannot drift. The doc comment states the
reasoning: *"a verdict computed under different rules is not comparable to the
verdict it stands in for"* (`:581-584`).

The pass is **not** free: it costs a second traversal of the learned roots'
subtrees. It therefore runs **only** when the platform walk was incomplete —
a completed platform walk proves every learned root by definition
(`main.rs:931-938`).

### The `main.rs` contract, and how it can break

```rust
// main.rs:1532-1544 — the three-category uncertainty filter
let uncertainty: Vec<PathBuf> = discovered
    .diagnostics
    .iter()
    .filter(|d| {
        matches!(d.category,
            domain::DiagnosticCategory::PermissionDenied
                | domain::DiagnosticCategory::Metadata
                | domain::DiagnosticCategory::Vanished)
    })
    .filter_map(|d| d.path.clone()).collect();
```

That vector is **always empty in production**, and the reason is worth stating
precisely because it is easy to assume otherwise. All three uncertainty
categories — `PermissionDenied`, `Metadata`, `Vanished` — are emitted with
`path: None` (`:928-942``, `:1124-1142``), because `dua_core`'s error carries no
failing child path. The `.filter_map(|d| d.path.clone())` above therefore drops
every one of them, and the vector that was supposed to carry known,
path-specific uncertainty into `reconcile_full` carries nothing.

The path-carrying protection described below is therefore **unreachable in
production**; it is the shape the code would take if the diagnostics ever grew
a path. Unknown-location traversal errors have no path and therefore cannot be
represented in the vector; `ManifestDiscovery::coverage_complete`
separately gates both reconciliation and Full exit status. An incomplete walk
does not publish any generation, so unknown loss cannot be interpreted as
negative evidence.

> **Two traps live here.** First, the `matches!` filter above covers **three of
> the seven** `DiagnosticCategory` variants and has **no `_` arm**
> (`src/main.rs:1535-1542`), so a newly-added category is *silently* excluded.
> Second, the filter itself is correct and should not be "fixed" by adding a
> catch-all that admits pathless diagnostics — that would make the vector
> non-empty with values that carry no location, which is worse than empty.
> See `architecture/06-discovery-state.md` §5 and `architecture/overview.md`
> finding #30.

- The diagnostic categories distinguish `PermissionDenied`, `Vanished`, and
  other metadata errors. Traversal summaries count each class, although a mixed
  aggregate uses one representative category for the row (`:930-936``, `:1126-1132``).
- *Known localized diagnostics would* use their paths to protect intersecting
  old roots — but no category currently emits a path, so this branch is
  unreachable in production. Pathless informational diagnostics remain
  non-uncertain.
- The coverage flag, rather than a duplicated category predicate, decides
  whether Full can publish state and whether it exits 1.

**Cleanup, under ADR 007.** The old "any discovery diagnostic blocks the whole
scope" rule is gone. Cleanup blocks only on *in-boundary* discovery diagnostics
(`cleanup.rs:1577-1578`, `cleanup.rs:2661-2674`): a diagnostic whose path lies
outside the selected boundary is reported and never blocks. Pathless diagnostics
cannot be attributed, so they count as in-boundary (fail-closed,
`cleanup.rs:2669-2670`) — which is exactly why discovery's path-aggregated walk
errors still block the scope they occurred in. The block is raised only after
metadata resolution, so workspace and unit counts describe attempted work
(`cleanup.rs:1561-1570`, `cleanup.rs:1666-1677`). The final proof re-walk is
unchanged: it re-discovers over the *selected* roots and refuses any diagnostic
(`cleanup.rs:2039-2061`), and the ownership refresh does the same
(`cleanup.rs:3196-3208`).

---

## 7. Concurrency

`discovery.rs` contains no `std::thread::spawn` and no `std::thread::scope`.
Threads belong to dua-core's internal pool (`dua-core lib.rs:784`); this module
passes a worker count and consumes one iterator on the calling thread. Every
accumulator
(`manifests`, `diagnostics`, the `u64`s, the attribution maps) is a plain `&mut`
parameter, which is why no lock appears.

**Work division.** Directory reads are pool jobs; each finished read schedules
its subdirectories into the shared queue and other workers steal them
(`dua-core lib.rs:3-6`). `traverse::worker_threads()` is
`available_parallelism().clamp(1, 8)` (`traverse.rs:72-76`) — a budget *shared
with* the measurement traversal, capped at 8.

**One pool for all roots, in the global path** (`:706``): N roots share ≤8
workers. **One pool per root, in the sequential path** (`:1027``): `walk` starts
a fresh pool, and dropping the iterator stops and joins its workers
(`dua-core lib.rs:11`). `ScanScope::ExplicitRoots` from `cleanup.rs:1531`
therefore pays one pool startup per selected root.

**Result ordering, honestly.** Results are *not* merged from several threads —
the pool funnels all events into one channel that a single thread drains, so there
is no merge to order. But the *stream* order is scheduler-dependent (sibling order
is unspecified, `dua-core lib.rs:6`). The return value is nevertheless
deterministic because every output is order-insensitive by construction:

| Output | Mechanism |
|---|---|
| `manifests` | `sort()` then `dedup()` (`:448-449`, `:448-451`) |
| `top_level_entries` | `BTreeMap` (`:996`, `:996`) |
| counters | commutative sums, one increment per event |
| `diagnostics` (global) | per-root buffer, then `sort_by_key(root index)` (`:451-452`) |
| `diagnostics` (sequential) | one summary per root, in root order (§6) |

`examples/traversal-qualification.rs:35` (stage 6) is the harness that runs the
production global path with attribution for benchmarking, and
`synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps` (`:1551-1565`)
asserts the determinism claim across 6 worker counts × 2 metadata modes × 2
orders, and that `top_level_entries` sums to `visited_entries` (`:309-313`).
The consequence for report stability: the report is stable; the progress
bar's tick order is not.

---

## 8. Invariants and edge cases

**Duplicate-free?** Yes, by `sort()` + `dedup()` on the `PathBuf` (`:2176-2177`,
`:954-957``) — a *spelling* comparison, not canonicalization and not a `HashSet`.
The global path additionally dedups *roots* by canonical identity (`:643-680``)
while keeping each root's user spelling for globs and diagnostics (`:681-687``).
The sequential path canonicalizes nothing, so two spellings of one directory in a
single `ExplicitRoots` scope would produce two differently-spelled manifest paths
that `dedup()` cannot merge. Not reachable from the CLI as wired (`cli`
absolutizes the single root; cleanup passes canonical workspace roots) — but that
is a caller contract, not an invariant of this module.

**Deterministic order?** `manifests` and `top_level_entries`: yes.
Global-path diagnostics: yes, by the sort at `:901``. Sequential-path
diagnostics: yes — one summary per root, in root order. Counters are
order-independent.

**Root missing, a file, or a symlink?** All three are refused by `root_is_usable`
(`:502-622``) with `Error` + `PlatformRoot` + the root's path; the root is skipped
(`:421-424``, `:651-667``) and the scan continues with `Ok`. The symlink refusal is
deliberate (comment `:497-501``, L7): `Path::is_dir` follows symlinks while the
walker does not, so a symlinked root used to produce a *successful empty* scan for
a directory the user asked for. `path_is_symlink` (`:475-495`) additionally
strips a trailing `/` or `/.`, because `symlink_metadata("link/")` follows the
final component on POSIX and would otherwise hide the symlink (B1).

A root that vanishes between `root_is_usable` and the walk becomes a `Warning`
(`:1057-1061``, `:1121-1142``), **not** `Error` + `PlatformRoot`. Because C029 it
still sets `coverage_complete = false` (`:1058``), so a Full scan neither
publishes (`main.rs:1544` → `discovery_state.rs:220-222`) nor exits 0
(`main.rs:1662`, `main.rs:1833-1839`). It never reaches the path-carrying
`uncertainty` vector, because the diagnostic is pathless.

**Root vs deep-tree permission errors: distinguished, and it matters.** An
unstat-able root is `Error` + `PlatformRoot` — the only thing that makes
`scan --full` exit 1 through the `PlatformRoot && Error` predicate
(`main.rs:1466-1469` → `main.rs:1662` → `main.rs:1833-1839`) and `complete` false
(`main.rs:1544` → `discovery_state.rs:220-222`). An unreadable subtree is
`Warning` + `PermissionDenied`/`Vanished`/`Metadata`, pathless, named by root in
the message. It reaches the scan through the other predicate,
`coverage_complete`. A platform-level failure and a deep hole both make the scan
incomplete; they differ in *which* predicate says so.

**TOCTOU.** Every walker error degrades to a `Warning` and the walk continues
(`:765-770``, `:1057-1061``) — never a panic, never an `Err`. A directory removed
between enumeration and descent looks like a permission error to the counter,
because both share the same accumulator and are separated only by `ErrorKind`
(`:261-268`: `PermissionDenied` → denied, `NotFound` → vanished, anything else →
"other"). A `Cargo.toml` removed before the `symlink_metadata` re-validation is
dropped **silently**: `is_ok_and` is false, so it is neither counted nor
reported (`:844-845``, `:1095-1098``) — a real unrecorded incompleteness, bounded by
the fact that a project which just vanished cannot be reaped anyway.

**Hostile or unusual paths.** No `unwrap`, `expect` or `panic!` anywhere in the
production half (`:1-1145``). Non-UTF-8 directory names are handled deliberately:
`path.to_string_lossy()` feeds the glob match (`:42`, comment `:31-37`, L4) so a
broad `*` cannot be escaped by an invalid name, while a pattern that spells the
valid part still matches only what it spells (test `:1720-1743``). Over-long paths
surface as `ENAMETOOLONG`, i.e. a `Metadata` diagnostic. Case collisions are why
`is_manifest_name` exists (`:115-125`). All containment tests use component-wise
`Path::starts_with`, so `/usr` never captures `/usrfoo` (`:184`, `:245-248`).

**Fatal vs diagnostic.** `Result` is nearly vestigial here. The only reachable
`Err` is `AppError::Config` from a malformed `scan.ignore` glob (`:23`) or a
`GlobSet` build failure (`:14`) — those are the only two `AppError::`
constructions in the file. `discover_manifests_root` is declared
`Result<(), AppError>` but contains no `?` and always returns `Ok(())` (`:1144``), so
the `?` at `:437`` cannot fire today; `AppError::Scan` is never constructed here.
**Fatal = bad glob syntax; every filesystem condition degrades into a
diagnostic.** (Contrast `policy.rs:32-41`, where an unusable *root* is a fatal
`AppError::InvalidRoot` before discovery ever runs.)

---

## 9. Testing

39 `#[test]` functions (verified `grep -c '#\[test\]' src/discovery.rs`), all in
`discovery.rs:1147-2647`. Seven are compile-time platform-gated — `:1166``
(`unix`), `:1698`` (`macos`/`windows`), `:1718`` (`linux`), and `:1762``, `:1794``,
`:2115``, `:2554`` (all `unix`) — so a Linux or macOS run executes 38 and a
Windows run 33.

| Test | Line | Invariant protected |
|---|---|---|
| `more_than_one_thousand_walk_errors_stay_one_bounded_root_record` | `:1154` | C029: 1001 walk errors collapse into one per-root record |
| `unreadable_child_keeps_accessible_inventory_and_unknown_coverage_bounded` | `:1168` | C029: an unreadable child keeps the readable inventory, sets `coverage_complete = false`, and yields one pathless aggregate per root |
| `a_root_inside_a_global_prune_reopens_that_prune` | `:1270` | the `/usr/local` exception root is not swallowed by `/usr`; cargo and rustup homes get no exemption |
| `global_walk_descends_into_a_root_that_reopens_a_system_prune` | `:1358` | the same shape through the *production* walk, with non-empty system prunes and honest prune counters |
| `rustup_home_uses_absolute_override_or_platform_home_only` | `:1423` | precedence table via the pure `effective_rustup_home_from`; relative `RUSTUP_HOME` → `None` |
| `global_rustup_prune_keeps_adjacent_user_project_reachable` | `:1445` | the rustup prune spares an adjacent sibling project; the five per-reason counters sum to `directories_pruned`; an explicit scope overrides the global prune |
| `global_multi_root_walk_deduplicates_equivalent_roots_and_manifests` | `:1502` | duplicate identical roots collapse by canonical identity |
| `profiling_worker_override_is_opt_in_and_bounded` | `:1531` | worker override inert unless profiling; 32-worker cap; `completion` order opt-in only |
| `synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps` | `:1551` | 288-leaf + deep + hidden + unignore + symlink fixture is identical across 6×2×2 configurations; `top_level_entries` sums to `visited_entries` |
| `manifest_name_is_matched_case_insensitively_where_the_volume_is` | `:1700` | case-insensitive manifest match (macOS/Windows) |
| `non_utf8_directory_still_matches_a_broad_ignore_pattern` | `:1720` | L4: a non-UTF-8 name cannot escape `*` (Linux) |
| `directories_visited_counts_directories_not_files` | `:1746` | L6: 4 directories vs 8 entries |
| `symlinked_scan_root_is_refused_instead_of_scanning_nothing` | `:1764` | L7: a symlinked root yields an `Error` diagnostic, not a silent empty scan |
| `symlinked_scan_root_with_trailing_slash_is_refused` | `:1796` | B1: the trailing `/` and `/.` spellings are refused too |
| `discovers_real_manifest_and_prunes_target` | `:1824` | `target/` descent refused and a nested `target/` manifest excluded |
| `global_attribution_is_unallocated_when_not_requested` | `:1840` | `attribution = false` leaves the map empty while manifests and counters are unchanged |
| `verification_pass_attributes_coverage_to_the_root_that_failed` | `:1866` | M018: `verify_root_coverage` re-walks each root as its own walk root, and the failure is attributed to the root that produced it — the readable sibling root stays complete |
| `a_real_traversal_produces_the_code_the_gate_judges` | `:1933` | M018: a real traversal produces the coverage codes the Full gate judges (`complete`, `error_events`, `permission_denied`, `vanished`) |
| `manifest_discovery_finds_project_without_target` | `:1996` | a project with no `target/` is still a candidate |
| `manifest_discovery_prunes_target_children` | `:2012` | a nested `target/` manifest is excluded |
| `manifest_discovery_prunes_cargo_home_registry` | `:2032` | `$CARGO_HOME/registry/src` sources are not user projects |
| `ignored_subtree_never_invokes_cargo_manifest_count_zero_for_pruned` | `:2059` | an ignored tree contributes prunes and zero manifests |
| `discovery_reaches_unignored_project_without_entering_ignored_sibling` | `:2085` | the `/*` half of the unignore mechanism, against canonical spellings |
| `discovery_rejects_symlink_target` | `:2117` | a symlinked `target` does not stop manifest discovery (eligibility is decided later) |
| `filters_ignored_parent_keeps_exception_route` | `:2138` | the re-enable path in `Filters` |
| `literal_ignored_ancestor_becomes_pass_through_and_not_inherited_by_siblings` | `:2168` | the disposition state model: `PassThrough` / `Pruned` / `ReIncluded` |
| `disposition_of_a_recursive_ignore_pattern_is_still_pass_through` | `:2218` | the documented `/**` workaround still reaches the exception |
| `literal_ignored_ancestor_does_not_readmit_ignored_siblings` | `:2243` | C019's premise negative |
| `pass_through_directory_manifest_itself_stays_excluded` | `:2270` | H-1: a manifest directly under a `PassThrough` directory is excluded |
| `explicit_walk_respects_explicit_flag_for_pass_through_manifest` | `:2293` | H-1's explicit half: `explicit = true` bypasses ignore entirely |
| `user_ignore_prune_counts_only_the_subtrees_the_walk_refused` | `:2367` | the prune-counter contract: the pass-through ancestor and the re-included directory are not skips |
| `wildcard_and_recursive_ignore_patterns_keep_their_documented_behavior` | `:2398` | `*` and `**` admit only the exception |
| `several_exceptions_under_one_ignored_ancestor_are_each_reachable` | `:2421` | multiple and nested exceptions, at both levels |
| `a_deeply_nested_exception_never_visits_a_pruned_siblings_subtree` | `:2448` | the refused siblings' subtrees are never read |
| `an_exact_unignore_reincludes_the_whole_subtree_at_its_root` | `:2484` | re-inclusion covers the subtree at the exception root |
| `an_explicit_root_still_bypasses_a_literal_ignore_entirely` | `:2506` | a named root means it |
| `an_exact_unignore_does_not_reopen_an_internal_prune` | `:2522` | internal prunes are decided before the user filters |
| `an_exact_unignore_does_not_reopen_a_symlinked_directory` | `:2556` | a symlinked directory stays refused even when unignored by name |
| `a_broad_ignored_region_is_pruned_before_its_siblings_are_read` | `:2583` | 200 refused siblings, each refused exactly once |
| `an_explicit_unignore_of_the_scan_root_prunes_the_whole_root_visibly` | `:2616` | a root refused by an inherited ignore is counted at `depth == 0` (global path) |
| `an_empty_policy_includes_everything` | `:2637` | the explicit-scope case has no exclusion to inherit and nothing to re-include |

**Not covered — the gaps a reader should worry about.**

1. **The global path's walk-error branch has no test at all** (`:765-770``,
   `:900-943``): its per-root drain, its `coverage_complete = false`, and its
   `sort_by_key` ordering. The sequential path's equivalent *is* tested
   (`:1022`). This is the branch `main.rs:1523-1527` depends on. M018's
   `verification_pass_attributes_coverage_to_the_root_that_failed` (`:1904`)
   covers the *per-root attribution* of the same failures through the
   verification pass, not the platform walk's own drain.
2. **`root_is_usable`'s "unavailable" branch is untested** (`:502-515`, the
   missing-or-not-a-directory case) — the branch that makes `scan --full` exit 1.
   Only the symlink sibling is covered. Since M018 that branch also produces an
   *incomplete coverage entry* with a `detail` (`:660-676`), so the untested
   branch now has a second consequence.
3. **`is_manifest_name` has no negative test on Linux**: nothing asserts a
   lowercase `cargo.toml` is *not* found on a case-sensitive volume. And the
   macOS/Windows test's comment claims "on a case-insensitive volume"
   (`:1701-1702``) while its assertion holds on a case-*sensitive* volume too — it
   verifies the rule, not its own stated premise. That is exactly the hazard the
   project's lesson names: *a green test proves only that its own premises hold.*
4. **The `CARGO_CLEANME_PROFILE_*` wiring is unexercised** — only the pure helpers
   are tested (`:1532-1547``), never `discover_manifests_with_attribution`'s use of
   them (`:388-398``). Likewise `top_level_entry_map` with multiple or nested
   roots is unasserted (`:285-305`; the accounting sum is checked for a single
   root only, `:1641-1645``).
5. **The two paths disagree about a root refused by an inherited ignore** — the
   global path counts it (`depth == 0`, `:819-823``), the sequential path does not
   (`path != root_path`, `:1080``). Only the global path is tested (`:2616``).
6. **Process-global environment mutation.** `manifest_discovery_prunes_cargo_home_registry`
   writes `CARGO_HOME` with `unsafe { std::env::set_var }` and restores it after
   the call (`:2041-2054``). The harness runs tests concurrently, so this races any
   test in the same binary that reads `CARGO_HOME` or depends on
   `cargo_home_prunes()` — a flakiness risk, not a false green, since the
   assertion is still meaningful. `scripts/check-fixture-portability.py` scans
   `src/**/*.rs` and applies four rules (literal PATH separator assembly, an
   unjustified POSIX shebang fixture, raw byte names, platform-specific entry
   literals); none of them is about environment mutation, so nothing catches
   this statically. The `#[cfg]` guards elsewhere in this module
   are the right form — compile-time, not a runtime `if` that silently passes.
7. **Windows is the thinnest lane**: 33 of 41 tests, including *neither* symlink
   test (both are `unix`-gated), with `system_prune` (`:129-142`)
   unconditionally `false` and `skip_metadata` always `false` (`:617`).

---

## 10. Review checklist

1. **Does any new `DiagnosticCategory` mean "I could not see here"?** Then it must
   be added to `main.rs:1532-1544` *and*, if it should block Full reconciliation,
   to the `PlatformRoot && Error` check (`main.rs:1524-1527`) and to the
   `coverage_complete` flag (`:454`, `:454`). Missing the first silently
   converts a hole in the scan into certainty about learned roots. Before adding
   anything at all, check `cleanup.rs:2661-2674`: under ADR 007 a discovery
   diagnostic blocks only when it is in-boundary, and a pathless one always
   counts as in-boundary.
2. **Does a new prune reason have a `ScanCounters` field and a position in the
   `else if` chain at `:824-834``?** That chain's ordering plus
   `directories_pruned` is what makes the identity asserted at `:443-450` hold; a
   prune counted in `pruned` but absent from the chain breaks it silently. The
   sequential literal (`:441-446``) must be widened in the same change, or the
   reason is invisible on `scan <dir>`.
3. **Are the `depth > 0` guards at `:807-810`` still right for a new reason?**
   They are why a root inside a pruned subtree reports 0 prunes (§5). Fixing
   that means touching both the counter and the descend predicate, which use
   different guards today (`:725-730`` vs `:807-810``). The sequential path's
   guard is a different shape entirely — `path != root_path` (`:1080``) — and it
   covers the ignore reason as well.
4. **Is a new prune reachable in both paths?** The sequential `ScanCounters`
   literal (`:461-466``) fills only 3 fields, the platform prune lists are
   global-only (`:342-351``, `:385-391``), and the hardcoded `system_prune` is
   sequential-only (`:1045``). A reason added to one path only looks implemented
   in `--stats` on `scan <dir>` and reports zeros.
5. **Is every prune still silent?** Only the two `root_is_usable` outcomes and the
   empty-Routine notice emit diagnostics (`:378-387``, `:506-511``). A prune needing
   a user-visible explanation belongs in the report, not in a counter.
6. **Did a change break root canonicalization?** Only the global path dedups roots
   by canonical identity before keeping the user spelling (`:643-687``); the
   sequential path canonicalizes nothing, so its dedup is spelling-based (§8).
   Either change alters duplicate behaviour for `ExplicitRoots`.
7. **Is a diagnostic stream still deterministic on both paths?** Both emit at most
   one summary per selected root, and the roots are visited in a fixed order
   (§6). A new diagnostic emitted per traversal *event* would destroy that
   property and the bounded memory with it — keep the per-root accumulator
   (`:261-268`, `:760``).
8. **Does a new manifest test assert a fixture property it does not establish?**
   The case-insensitivity test (`:1700-1714``) is the current example of a green
   test that does not prove its stated premise. Relatedly: did a change touch
   `is_manifest_name` or `Filters::directly_ignored`? Both encode platform
   assumptions with compile-time-gated tests (`:1698``, `:1718``); a runtime `cfg!`
   in their place turns a skipped test into a passing one.
9. **Are the four undocumented `CARGO_CLEANME_PROFILE_*` knobs still inert by
   default?** They are gated on `CARGO_CLEANME_PROFILE_SUBTREES` (`:388-398``,
   `:190-226`) and appear in no operator-facing document — so making one
   unconditional changes production traversal with no documented way back.
10. **Does a change to the walk invalidate the coverage verdict?** The M018
    ledger is built **before** the walk (`:644-678`) and corrected by the walk's
    error events (`:915-927`). A new prune reason, a new root filter, or a new
    "unusable root" condition must be reflected in `root_unusable_reason`
    (`:522-530`), or a root will be reported complete when it was not read. The
    verification pass must also keep using the *same* prune and filter sets as
    the platform walk — `global_walk_knobs()` (`:549-570`) is the shared knob
    reader precisely so the two cannot drift.
