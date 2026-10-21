# Traverse — parallel measurement and activity classification

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** the reachable panic described in §8 has been **fixed**. A repeated
> `candidate_index` is now measured once and every occurrence of that index
> reports the same measurement, covered by a new unit test. The dead
> `source_activity` noted in §6 was reviewed and **kept** (see
> [overview §10.1](overview.md)).

`src/traverse.rs` — 624 lines, 130 of them inline tests (`traverse.rs:495-624`).

## 1. Responsibility

`traverse.rs` is the only module in `src/` that answers two questions about a directory tree:
  **how many bytes does it occupy** and **when was anything in it last written**. It answers them
  by walking the tree and re-`stat`ing every entry. It is the leaf that discovery's candidate list
  and workspace's ownership graph both bottom out in.

**Owns:** the crate-wide worker-count policy (`:67-76`); the single-root size/recency measurement
  (`:107-175`); the batched multi-root measurement over one pool (`:185-291`); source-activity
  scanning in both its single-output and workspace-member forms (`:311-380`, `:411-471`); the
  VCS-exclusion predicate `.git`/`.hg`/`.svn` (`:97-99`, applied at `:337-343` and `:438-444`);
  the rule that *pruned boundary entries carry no evidence* (`:357-369`, mirrored at `:458`); and
  the only naming of the `filesize` types in the crate, plus the single ownership point for
  `dua-core` in the measurement path (`:78-79`, `:117`, `:159`, `:220`, `:267`).

**Does not own:**

- **Verdicts.** It emits `SourceActivity`, never an activity *state*. The
   domain's `ActivityState` (`domain.rs:40-44`) has **no construction site anywhere in `src/`** — a
    grep returns the definition and nothing else. Classification of evidence into a decision
    happens in `workspace.rs:1735-1750` and `cleanup.rs:3563-3582`.
- **Sizing policy.** It reports bytes and an mtime; `workspace.rs:1861` applies
   the recency window, `:1782` the emptiness gate, `:1812-1821` the metric label. The window is
    supplied by the caller (`main.rs:1348-1357`, `cleanup.rs:3542-3550`).
- **Workspaces, manifests, ownership, authorization.** No knowledge of Cargo,
   `Cargo.toml`, or who owns what.
- **Progress reporting.** It holds no observer. The `observer.group_measured` /
   `unit_completed` calls bracketing a measurement live in the caller (`workspace.rs:1876-1877`,
    `:1738`, `:1774`, `:1785`, `:1798`) — which is what lets this stay a leaf.
- **Cleanup.** The post-clean measurement referenced in the module doc
   (`:3-4`) is a re-measurement *supplied to* `cleanup.rs`; nothing here deletes.

**Why the separation is load-bearing.** Activity is *evidence*, not a verdict. A module whose
  output type can only be `Recent(t)` / `Quiet(Option<t>)` / `Err(())` has no vocabulary for "safe
  to delete" and therefore cannot be tempted into producing one. The safety decision stays one
  layer up, in `workspace.rs` and `cleanup.rs`, where the ownership proof also lives (invariant 1
  in [the overview](overview.md)).

**Leaf property: verified.** A grep for `crate::` and `use super` in `traverse.rs` yields exactly
one hit, inside the test module: `use super::*;` at `traverse.rs:497`. There is **no
`crate::progress` import** in this file at any point, test or otherwise; the only imports are
`std::collections::HashSet`, `std::fs`, `std::path`, `std::sync::{Arc, Mutex}`, and
`std::time::SystemTime` (`traverse.rs:18-24`). This matches the leaf claim in
[the overview](overview.md) §5.1.

> **History, retained because the doc table got this wrong twice.** The first
> `overview.md` module index listed `traverse.rs` as "544" with no inline-test
> annotation, while every other tested module in that table carried one. It
> *does* have an inline `#[cfg(test)]` module (`traverse.rs:495`). The 544 total
> was then correct; the implied "no inline tests" was not. It later went stale
> in the other direction when the duplicate-index fix added 55 lines, and the
> index now reads 624 / 494 / 6 ([the overview](overview.md) §3.1). See §9.

## 2. Public surface

```rust
pub fn worker_threads() -> usize;                                        // :72
pub fn measure_single_target(target: &Path) -> TargetStats;             // :107
pub fn measure_many_targets(targets: &[(usize, PathBuf)])
    -> Vec<(usize, TargetStats)>;                                       // :185
pub fn source_activity(project: &Path, target: &Path,
                       start: SystemTime, cutoff: SystemTime)
    -> Result<SourceActivity, ()>;                                      // :311
pub fn workspace_member_activity(member_root: &Path, output_roots: &[PathBuf],
                                 start: SystemTime, cutoff: SystemTime)
    -> Result<SourceActivity, ()>;                                      // :411
```

### `worker_threads()` — `traverse.rs:72-76`

Derivation, in order: `std::thread::available_parallelism()`, `map_or(1, ..)` on error, then
  `.clamp(1, 8)` — **available parallelism, hard-capped at 8, floor 1**. There is **no environment
  override on this function**; the crate does have `CARGO_CLEANME_PROFILE_THREADS`
  (`discovery.rs:385`) but that feeds discovery's profiling pool only. `discovery.rs:386` then
  applies `.min(GLOBAL_DISCOVERY_WORKER_CAP /* 8 */)` on top, redundant with the clamp already
  inside. Because this is a leaf, `discovery.rs` reaches it for its own walks (`:883`, `:1246`,
  `:1320`, `:1374`) — the single worker-count policy really is shared crate-wide, as
  `traverse.rs:67-71` claims.

### `TargetStats` — `traverse.rs:85-91`

| Field | Type | Meaning |
|---|---|---|
| `bytes` | `u64` | **Byte count.** Sum of allocated bytes over regular files only. Not a `usize`; no 32-bit overflow exposure. |
| `entries` | `u64` | **Count of entries, not bytes.** Every walked entry except the root — directories, symlinks, sockets, FIFOs, device nodes all count. |
| `newest` | `Option<SystemTime>` | **Maximum** mtime over the same entry set. `None` iff nothing but the root was walked. |
| `uncertain` | `bool` | `true` if any walk, metadata, or sizing step failed. Contract: "callers must treat the candidate as ineligible rather than promoting a partial measurement" (`:82-84`). |

`Default` is derived, so an all-zero `TargetStats` is indistinguishable from a genuinely empty
  directory. Callers must read `uncertain` to tell them apart — `workspace.rs:1817-1831` does
  exactly that, treating a *missing* map entry as uncertain too.

### `SourceActivity` — `traverse.rs:294-302`

| Variant | Payload | Caller should |
|---|---|---|
| `Recent(SystemTime)` | The **first** recent timestamp encountered, not the maximum (`:376`, `:467`) | Treat the candidate as active; do not size it. `workspace.rs:2153`, `cleanup.rs:3564`. |
| `Quiet(Option<SystemTime>)` | Newest observed source timestamp, `None` if nothing could be stat'ed (`:379`, `:470`) | Not-active is *permitted*, not *required* — the timestamp is evidence for the caller's own bookkeeping. `cleanup.rs:3570-3574` folds it into `newest_activity`. |

`Err(())` is a third, implicit outcome: any walk or metadata failure is conservative
  (`traverse.rs:354`, `:454`). `cleanup.rs:3575-3580` turns it into a hard block;
  `workspace.rs:1708-1717` into a workspace-level uncertainty that later becomes
  `GroupSkipReason::UncertainSource` (`:1679-1682`).

### Private helpers worth explaining

| Helper | Line | Why it exists |
|---|---|---|
| `contains_vcs_marker()` | `:31-39` | One definition of "does this directory hold a nested checkout?", shared by the descend decision and the evidence rule. |
| `NestedRepoMarkers` | `:49-65` | `Arc<Mutex<HashSet<PathBuf>>>` memo of directories already known to hold a marker. Only *positives* are recorded, so it is bounded by the number of nested repositories rather than by tree size. |
| `walk_options()` | `:78-80` | Single choke point returning `dua_core::Options::default()`. Note this keeps `skip_metadata` **off**. |
| `newest_time()` | `:93-95` | `old.max(next)` fold, used by every accumulator in the module. |
| `is_vcs_name()` | `:97-99` | The `.git`/`.hg`/`.svn` set, in one place. |
| `is_excluded()` | `:382-402` | The evidence rule for the single-output form. |
| `is_excluded_multi()` | `:473-493` | The same rule, parameterised over N output roots. |

## 3. Threading model

**`measure_many_targets` does not use `std::thread::scope` and does not manage threads at all.**
  It calls `dua_core::walk_roots(roots, worker_threads(), ..)` (`traverse.rs:220-226`). Thread
  creation, work distribution, and joining are entirely the engine's (`dua-core` 4.1.0,
  `start_pool` at `lib.rs:784`).

- **How many threads:** `worker_threads()`, i.e. ≤ 8, independent of candidate
   count — one pool for the whole batch, the point of `traverse.rs:183-184` and the property
    asserted by `traverse.rs:570-584` (32 roots, pool still ≤ 8).
- **How work is distributed:** a **work-stealing pool**, not chunking.
   `PoolShared` (`dua-core lib.rs:283-301`) holds a crossbeam `Injector<Job>` plus per-worker
    `Stealer`s, an `idle` flag per worker, and a round-robin `next_wake` cursor with an atomic
    "claim one idle worker" step. Directories are pushed as `Job::ReadDir` and claimed
    dynamically. No static partition, no chunk size, no chunk-size heuristic anywhere in
    `traverse.rs`.
- **`Order::ParentFirst`** on all four walks (`:120`, `:223`, `:328`, `:427`),
   so a parent always precedes its descendants. The engine's `Order::Completion` is used by
    `discovery.rs` in profiling mode (`discovery.rs:220-224`) and by the qualification example
    (`examples/traversal-qualification.rs:78`); no walk in this module uses it.
- **Not chunked because roots are not partitioned:** every root enters one
   shared queue, so a 40 GB `target/` cannot starve nine 4 KB ones.

### Order preservation — definitive answer

**The returned `Vec` is in ascending-`usize` order, not completion order.** `traverse.rs:287-290`
  rebuilds the output by iterating `ordered` (stable-sorted by index at `:186-187`) and looking
  each index up in `stats_by_index`. The engine's arrival order is discarded entirely. This
  satisfies the doc contract at `traverse.rs:180` ("Results are returned in input order") *only
  because* callers pass ascending indices.

**Do callers depend on it? The production caller does not.** `workspace.rs:1790-1793` re-keys
  every result into a `HashMap<usize, TargetStats>` and looks up by explicit position at
  `:1750-1751`. Order is irrelevant there. The test at `traverse.rs:521` does depend
  on it (`batched.iter().zip(singles.iter())`, positional) and is the only order-sensitive
  consumer.

**The nuance:** the implementation guarantees *index-sorted* order; the doc promises *input*
  order. Those differ for unsorted indices. `workspace.rs:1776-1780` uses `.enumerate()`, so they
  coincide today. Duplicated indices are a second, worse hazard — see §8.

**Open directory handles.** There is **no explicit bound in this module**, and the module makes
  exactly one `fs::read_dir` call — `contains_vcs_marker` (`:32`) — which consumes its iterator
  inside the `is_some_and` closure and drops it there, so no handle outlives the callback that
  opened it. The effective bound is the engine's worker count (≤ 8) plus buffered-entry depth,
  not a number anyone chose. If the OS refuses an open — EMFILE, or a vanished directory — the
  engine surfaces it as `RootEvent::Entry(Err(_))` (`:276-280`) or an `Err` item from `walk`
  (`:125-131`); the module marks that root `uncertain` and never reports a partial size as
  complete.

## 4. Measuring a target

`measure_single_target(target: &Path) -> TargetStats` — `traverse.rs:107-175`.

### Algorithm

Root guard → `dua_core::walk` with `Order::ParentFirst` and a constant `|_| true` descend
  predicate (`:109-123`) → linear fold over the yielded entries. The recursion lives in the engine's
  heap-allocated job queue, so **this module contains no recursion of its own** and no explicit
  stack. Every entry is then re-`stat`ed from scratch (`fs::symlink_metadata` at `:136`),
  deliberately — `traverse.rs:9-12` gives the reason as keeping platform behaviour, error
  conservatism, and the allocated-bytes metric from drifting with the engine change.

### What it counts

- The root is yielded then **explicitly skipped** (`:132-135`), so a target with
   one file reports `entries == 1`, not 2.
- Every other entry increments `entries` and folds into `newest` (`:150-157`).
- **Only `meta.is_file()` contributes bytes** (`:158-172`).

### The cases that make a naive walk wrong

**Symlinks — not followed, and no loop detection is needed.** Two independent guards.
  `traverse.rs:109-115` rejects a root whose `symlink_metadata` says symlink, and the engine never
  follows symlinks during descent (`dua-core lib.rs:432`, `:593`: *"Walk `root` without following
  symlinks"*). The mechanism is structural: descent is gated on `entry.file_type.is_dir()`
  (`dua-core lib.rs:1288`), and on Unix a symlink's `d_type` reports `is_symlink()`, not
  `is_dir()` — so a symlink-to-directory is never queued as a `ReadDir` job. Since a hard link to
  a directory cannot exist on Unix or Windows, **no cycle is constructible and no visited-set is
  required.** A symlink still counts in `entries` and contributes its own mtime; it contributes 0
  bytes. (`discovery.rs:891` carries the same note.)

**Hard links — counted once per directory entry, no dedup.** `st_blocks` is a property of the
  inode, so N dirents pointing at one inode each report the same allocation and the walk sums them
  N times: **a file with N hard links contributes N× its size.** This is conservative-by-direction
  rather than by-design — `bytes` is shown to the user as reclaimable space, and under-counting
  would over-promise. It is *not* `du` semantics (§7).

**Sparse files — allocated size, not apparent size.** `bytes` comes from
  `filesize::file_real_size_fast` (`:159`), which on Unix is literally `metadata.blocks() * 512`
  (`filesize-0.2.0/src/lib.rs:68-73`). A 1 GiB file with one 4 KiB block reports ~4 KiB — the
  correct number for a reclaimable-bytes estimate, and why `workspace.rs:1879-1888` labels the
  metric `SizeMetric::Allocated` on unix/windows.

**Unreadable entries — the information is genuinely lost, and that is the architectural cost of
  this design.** There is no `ScanDiagnostic` parameter and no error return on the size path: a
  permission error, a vanished file, and a failed `modified()` all collapse to the same two-word
  outcome — `stats.uncertain = true; break` (`:127-130`, `:138-141`, `:145-148`, `:167-170`). The
  *cause* is discarded. The consequence is contained because `uncertain` is fail-closed:
  `workspace.rs:1833-1848` drops the group and emits a generic `CandidateUncertain` error naming
  only the group display path. A user learns *that* a root could not be measured, never *why*.
  That is a deliberate trade — measurement must not be able to abort a scan — but it is the
  sharpest cost in the module.

**Special files are safe.** Sockets, FIFOs, and device nodes fail `is_file()` (`:158`): one entry,
  one mtime, zero bytes. Critically, **the module never opens an entry for reading** — `read_dir`
  is called only on paths already known to be directories: the single `fs::read_dir` at `:32` is
  reached only through `NestedRepoMarkers::probe`, and every call site guards on `is_dir` first
  (`:344`, `:401`, `:445`, `:492`). A FIFO in a `target/` tree cannot block the walk.

### Why `newest` is the *maximum* mtime

`newest` is `max` over every entry's mtime (`:93-95`, `:150`). For an inactivity test the question
  is "has *anything* here been touched since the cutoff?", and the max answers exactly that: the
  tree is inactive iff `max(mtimes) < cutoff`. The obvious alternative — the root directory's own
  mtime — is wrong in the common case: a `target/` root's mtime tracks only direct-child
  create/rename, not writes deeper inside, so `target/debug/deps/` churning for an hour leaves it
  untouched. The minimum is worse: one ancient file would make an actively-built project look
  inactive. The cost of the max is that it is a *lower bound on activity* — it proves a tree was
  touched, never that it was not. That is precisely why it is evidence rather than a verdict. The
  root is excluded from the max (`:132-135`), so `newest` describes the tree's contents, not the
  directory entry pointing at it.

## 5. Measuring many targets

`measure_many_targets(targets: &[(usize, PathBuf)]) -> Vec<(usize, TargetStats)>` —
  `traverse.rs:185-291`.

Sequence: copy + stable-sort by index (`:186-187`) → drop repeated indices, then classify each
  remaining path with `symlink_metadata`, pushing only real directories into `roots` and
  pre-seeding `stats_by_index` for the rest (`:191-212`) → one `walk_roots` over `roots`
  (`:220-226`) → re-emit in sorted index order (`:287-290`). The empty-input short circuit
  (`:213-218`) exists so a zero-valid-root batch never constructs a pool at all.

**Why the index is threaded through.** Roots must be identified across a shared, interleaved
  stream, and `PathBuf` is the wrong key: the same physical output root can be a `covering` entry
  of several groups, and `workspace.rs` emits one entry per *group position*, not per path. The
  `usize` lets the caller address results positionally while the engine addresses them by root id.
  `workspace.rs:1776-1780` produces indices with `.enumerate()`, which is why index order equals
  input order there. **The index must be unique** — the engine asserts it — and this module now
  enforces that itself rather than inheriting the engine's panic; see §8.

### Divergence risk — flagging for review

**The batch path duplicates the single path; it does not share it.**

| Step | Single | Batch |
|---|---|---|
| root guard | `:109-115` | `:202-211` |
| root skip | `:132-135` | `:241-243` |
| `symlink_metadata` | `:136-142` | `:244-250` |
| `modified()` | `:143-149` | `:251-257` |
| `newest` fold | `:150` | `:258` |
| `entries` `checked_add` | `:151-157` | `:259-265` |
| `filesize` + `checked_add` | `:158-172` | `:266-274` |

Seven near-identical steps, differing in one respect: the single path `break`s on error
  (`:127-130`, `:138-141`, `:145-148`, `:151-157`, `:160-166`, `:167-170`) while the batch path
  `continue`s (`:245-249`, `:252-256`, `:259-265`). That difference is *deliberate and correct* —
  the single path can stop its own private pool, whereas in a batch a failure must not abandon
  the other roots — and the `if stats.uncertain { continue; }` guard at `:233-235` restores the
  single path's semantics at root granularity. (The batch's two *sizing* arms do not `continue`
  at all: they set `uncertain` and fall through, and the same guard stops the root on the next
  iteration.) So the two agree *behaviourally*, and `traverse.rs:507-526` asserts it on a
  three-root fixture. But nothing structural keeps them in agreement: the arithmetic is written
  out twice, so a change to the size metric in one place and not the other would still pass every
  test in the file. That is the divergence risk.

## 6. Source activity

This is a conceptually different question from §4. `TargetStats` answers "is this *output*
  stale?". `SourceActivity` answers "is anyone still working on this *project*?" A project can
  have two-year-old artifacts and a source file touched five minutes ago — the first question says
  delete, the second says no. The recency *window* is the same in both cases; the *evidence* is
  different.

### `source_activity(project, target, start, cutoff)` — `traverse.rs:311-380`

The recency rule is `time >= cutoff || time > start` (`:319`). Both clauses matter: `cutoff` is
  the inactivity window, and `time > start` catches **future-dated** timestamps (clock skew, a
  restored backup, a deliberately touched file). Production constructs `start` as
  `SystemTime::now()` (`cleanup.rs:3542`); the `+1s` future-clock that keeps the future
  test live exists only in test (`cleanup.rs:9849`) alongside `traverse.rs:614-616`.

Evidence is the **maximum** mtime over *non-excluded* entries, with early exit at the first recent
  one (`:374-377`). Excluded from evidence, including their own boundary mtime (`:357-369` and
  `is_excluded` at `:382-402`):

1. `target` and everything under it (`:392-394`);
2. any entry named `.git` / `.hg` / `.svn` (`:395-400`);
3. any directory that *contains* a nested VCS marker, i.e. a nested checkout
   (`:401`).

Rule 3's exclusion of the boundary directory's own mtime is the subtle one, commented as such at
  `:357-360`: a directory's mtime updates when its children change, so without this a nested
  repository's activity would leak into the parent through the boundary directory's timestamp and
  permanently protect it. The predicate is expressed twice — as the descent closure (`:330-348`)
  and as `is_excluded` (`:382-402`) — because the engine's callback decides *where to go* while
  the loop body decides *what counts as evidence*. Both are needed, and both must agree.

Timestamps come from `fs::metadata` (`:370`), which **follows** symlinks, in contrast to the size
  path's `symlink_metadata`. `traverse.rs:309-310` states this preserves the previous
  source-activity semantics for included entries. It is a genuine asymmetry between the two halves
  of the module, and a reviewer should confirm it is intentional rather than a leftover.

### What the guard actually measures — directories count

**This is the load-bearing detail for anyone writing a fixture.** The activity guard reads the
  mtime of **every entry it walks, including directories**: `fs::metadata(&path)` is called on
  each yielded entry with no file-type filter (`:370-377` in the single-output form, `:461-468` in
  the member form), and the walk root itself is *not* excluded — `is_excluded` returns `false` for
  `path == project` (`:389-391`) and `is_excluded_multi` does the same for `path == member_root`
  (`:480-482`). So:

- a member root's own directory mtime is source activity;
- a subdirectory's mtime is source activity, and a directory's mtime moves whenever its
  *children* are created, renamed, or removed.

Consequence: **backdating a workspace's files is not sufficient to make it eligible.** The
  containing directories' mtimes count too and must be backdated as well. A fixture that backdates
  only files will always observe a `Recent` verdict, and a `Quiet` result is unreachable — which
  reads as a bug in the tool when it is the fixture that is wrong. The repository's own end-to-end
  fixture does this correctly (`tests/end_to_end.rs:50-70` backdates the project directory and
  `src/`, `target/`, and `target/debug/` alongside the files, with the comment "The workspace
  directory's own mtime is part of the recency verdict"), and the portable in-repo helper that
  does it is `workspace.rs:4876-4977` (`set_path_modified` plus `backdate_tree`).

> **Known defect (C036 D4, proposed — not fixed).** `README.md:138-140` tells a user whose scan
> reports zero groups to "backdate the sources", which is incomplete advice: the guard also keys
> on directory mtimes. The corrective
> ([C036](../plans/implementation/artifact-discovery-cleanup/c036-cli-surface-truthfulness-and-documentation-drift-corrective.md)
> D4) records it as open; this deep dive states the behaviour the README omits.

### `workspace_member_activity(member_root, output_roots, start, cutoff)` — `traverse.rs:411-471`

Structurally identical; the only differences are (a) one output root becomes N (`:429-449`,
  `is_excluded_multi` at `:473-493`) and (b) it excludes *all* of them, including any outside the
  member tree.

**Why a member-aware version is required.** In a Cargo workspace the source activity of the
  *virtual* root is meaningless — the root typically has only `Cargo.toml` and member directories.
  Worse, all members may share one `target/`, so excluding only "the" output root is not
  expressible. `workspace.rs:2150-2157` therefore loops over `workspace_member_roots(ws)` and ORs
  the members: any one recent member protects the whole workspace. `cleanup.rs:3562-3582` repeats
  the loop during revalidation. The single-output `source_activity` is the one-project shape.

### Which function is actually live

**`source_activity` has no production caller.** A repo-wide grep returns the definition
  (`traverse.rs:311`) and exactly one call site: the inline test at `:619`. Production reaches
   source activity only through `workspace_member_activity`, via `workspace.rs:2152` and
   `cleanup.rs:3563`. `measure_single_target` *is* live in production (`cleanup.rs:3589`,
   `:3734`). `source_activity` is `pub`, so nothing warns about it; it is recorded as
   deliberately-kept dead public API in [overview §10.1](overview.md).

### Combining with a size measurement

The cutoff never enters this module's arithmetic; it arrives as a parameter (`:314-315`) and is
  applied per entry (`:375`). Composition lives upstream:

```text
main.rs:1348        clock_cutoff = scan_start.checked_sub(recency)
workspace.rs:1692   workspace_source_activity(ws, &all_outputs, clock_start, clock_cutoff)
workspace.rs:1738   any owner source-recent -> GroupSkipReason::ActiveSource
workspace.rs:1746   any owner uncertain     -> GroupSkipReason::UncertainSource
workspace.rs:1785   traverse::measure_many_targets(&measure_targets)
workspace.rs:1861   output gate: newest >= clock_cutoff || newest > clock_start
```

Source activity is the **cheap gate that runs before the expensive one**: a recent member
  short-circuits at `workspace.rs:1735-1745` and the group's output roots are never walked at all.
  A candidate the source gate clears is then re-checked in the opposite direction by the output
  gate (`workspace.rs:1861`) — *both* must agree the group is idle.

### What this signal can and cannot prove

**Can prove:** at least one non-excluded entry under a member root has an mtime at or after the
  cutoff, or in the future. A direct observation, and sufficient to refuse action.

**Cannot prove:** that a project is *inactive*. `Quiet` means "no source entry carried a recent
  mtime" — not "no one is working here". A project edited only through a symlinked or out-of-tree
  source directory, one whose edits preserve mtimes, one edited while its member root was
  momentarily unreadable, and one whose only recent file sits inside a nested repo all report
  `Quiet`. A pruned subtree is invisible by construction (`:357-369`). The signal is a **one-sided
  safety test**: `Recent` is authoritative, `Quiet` is merely permissive. Both production callers
  treat it that way (`workspace.rs:1696-1718` skips a group only on `true`; `cleanup.rs:3564-3569`
  aborts only on `true`).

## 7. Dependencies and trade-offs

### `dua-core = "4.1"` — `Cargo.toml:67`

**Verified: used here, and also in `discovery.rs`.** Grep for `dua_core` in `*.rs` returns
  `traverse.rs` (`:78-79`, `:117`, `:120`, `:220`, `:223`, `:229`, `:276`, `:281`, `:325`, `:328`,
  `:424`, `:427`) and `discovery.rs` (`:220-224`, `:228`, `:243`, `:255`, `:279`, `:544`, `:578`,
  `:583-585`, `:636-643`, `:755`, `:881-885`, `:1089-1099`). So `traverse.rs` is the *single
  ownership point* for `dua-core` in the **measurement** path (as `traverse.rs:3-5` claims), not
  the only user of the crate. `examples/traversal-qualification.rs` also uses it directly.

What it buys: a work-stealing parallel directory-read pool with a `ParentFirst` ordering
  guarantee, per-root identity, and a hard no-follow-symlink rule — all of which would otherwise
  be a few hundred lines of hand-rolled concurrency. It is the single biggest reason a
  whole-machine scan is tractable.

**What it does *not* buy — and the module's own doc is careful about this:** `dua-core` supplies
  **no disk-usage semantics at all**. It is a traversal engine. Per-file size comes from
  `filesize` (`:159`, `:267`) and timestamps from `std::fs`. `traverse.rs:8-14` says exactly this,
  and the code matches: `entry.metadata()` is never read; every entry is re-`stat`ed from the
  path.

**The `dua-core` boundary and error attribution.** `dua_core::RootEvent::Entry` carries a bare
  `io::Result<Entry>` (`dua-core lib.rs:276-278`): the payload is an `io::Error` with **no path**,
  and the accompanying root index identifies the *root*, not the failing subtree. Per-subtree
  error attribution is therefore impossible without re-walking a root as its own walk root. This
  module does not attempt one: a `RootEvent::Entry(Err(_))` marks the whole root `uncertain`
  (`:276-280`) and the cause is discarded, exactly as in the single-root path (`:127-130`). The
  pinned version is `dua-core` 4.1.0 (`Cargo.lock`).

**`filesize = "0.2"` — `Cargo.toml:68`. Verified: used *only* here.** A repo-wide grep for
  `filesize` in `*.rs` returns exactly two call sites, both in `traverse.rs` (`:159`, `:267`). If
  this module were deleted the dependency would be orphaned. All three platform branches are real:

| Platform | Implementation | Reported number |
|---|---|---|
| unix | `metadata.blocks() * 512` (`filesize-0.2.0/src/lib.rs:68-73`) | **Allocated.** Sparse files shrink; empty files report 0. |
| windows | `GetCompressedFileSizeW` on a **canonicalized** path (`:85-109`) | On-disk, compression-aware. |
| other | `metadata.len()` (`:120-125`) | **Apparent.** |

**The accuracy story, stated precisely — and its limit.** This is real allocated-bytes accounting,
  not `len()`: sparse files, compression, and block rounding are all reflected, which is what
  makes "bytes you would reclaim" honest. **It is not `du`, and the difference matters.** `du`
  dedups inodes; this sums `st_blocks` per directory entry, so hard-linked trees are over-counted
  and shared/reflinked blocks (APFS clones, btrfs) are counted once per dirent rather than once
  per physical extent. The figure is a *per-entry allocated-bytes estimate*, not a filesystem-wide
  disk-usage figure. For Cargo `target/` trees — overwhelmingly non-hard-linked, non-reflinked
  regular files — it is close; for anything else it is an upper bound in the safe direction.
  `workspace.rs:1879-1888` independently re-derives the same label from `cfg(any(unix, windows))`.
  The two cfg pairs agree today, which is what makes the label true, but it *is* a duplicated
  claim about a dependency's internals.

One Windows-specific hazard, verifiable and worth stating: that path calls `std::fs::canonicalize`
  (`filesize-0.2.0/src/lib.rs:86`) before querying size. Canonicalize fails on a file that exists
  but is unopenable, and fails if the file vanishes between walk and sizing — turning a race into
  `Err` → `uncertain`, i.e. fail-closed, not a miscount. A symlinked *file* inside a target tree
  would be canonicalized to its target and sized there; that is a plausible but unverified Windows
  path, and this environment has no Windows fixture to test it.

### Performance trade-offs visible in the code

| Cost | Where | Note |
|---|---|---|
| **Two metadata syscalls per entry.** The engine collects `entry.metadata()` eagerly because `skip_metadata` is off (`dua-core lib.rs:776`), then `traverse.rs:136`/`:244` throws it away and re-`stat`s. | `:78-80` + `:136` | The single largest avoidable cost in the module. `discovery.rs:583` uses `.skip_metadata()`; `traverse.rs` cannot, because it needs the `modified()` value — but it does not need the *rest* of the metadata. |
| `PathBuf` allocated per entry. `entry.path()` joins parent + filename and returns an owned `PathBuf` (`dua-core lib.rs:748`). | `:132`, `:236` | O(entries) heap allocations. Unavoidable given the re-stat design. |
| **Double directory read in source activity.** The descend closure and the entry classification used to ask the same question of the same directory, each costing a `fs::read_dir`. | `:344` + `:401`, and `:445` + `:492` | **Fixed.** One definition, `contains_vcs_marker` (`:31-39`), behind a `NestedRepoMarkers` memo (`:49-65`) shared by both. Only *positives* are recorded, so the memo is bounded by the number of nested repositories rather than by the size of the tree, and a miss costs exactly what it cost before — the entry loop degrades to the old behaviour rather than to a wrong answer. |
| O(entries) with two passes for roots. `symlink_metadata` runs once in the batch pre-pass and again per entry inside the walk. | `:202`, `:244` | |
| `root_paths` clones every path. | `:219` | O(roots) `PathBuf` clones per batch, for a comparison that only ever needs the root's own `path()`. |
| Sizing is O(bytes-of-inodes), not O(bytes). | — | Measuring a 40 GB tree costs one `st_blocks` lookup per file, not a read. This is the property that makes whole-machine sizing affordable. |

Hot spots for a whole-machine scan, in order: the double `stat` per entry; and — outside
  this module — the sheer number of
  entries, which is why `worker_threads()`'s cap of 8 is a *deliberate* limit (`:67-71`:
  "candidate count never spawns unbounded worker pools") rather than a missing feature.

## 8. Invariants and edge cases

**Are results deterministic given identical filesystem state?** **Yes for all measured values,
  with one exception.** `bytes` and `entries` are `checked_add` sums over the entry set and
  `newest` is a `max` fold (`:93-95`, `:151-157`, `:259-265`). All three are commutative and
  associative, so the traversal interleaving cannot change them. The `ParentFirst` order plus the
  `|| _` `Finished` no-op (`:281`) means no event is counted twice. `Ordering` is restored by
  index at `:287-290`, so even the `Vec` is deterministic.

**The exception:** `SourceActivity::Recent(t)` carries the **first** recent timestamp found, not
  the maximum (`:376`, `:467`), so `t` depends on which worker reached the entry first. In
  practice no production consumer reads the payload — `workspace.rs:2153` and `cleanup.rs:3564`
  both match `Recent(_)`. `Quiet(Option<SystemTime>)` *is* read (`cleanup.rs:3570-3574`) and *is*
  deterministic, because it is the `max`. If a future caller starts reading the `Recent` payload,
  it inherits scheduling nondeterminism.

**Does thread count change measured values, or only how fast they arrive?** **Only the speed.**
  `worker_threads()` (`:72-76`) reaches the engine as `threads` and nothing else; it appears in no
  accumulator, no threshold, no comparison. `traverse.rs:570-584` asserts the *pool* does not grow
  with candidate count, and `traverse.rs:507-526` asserts single/batch agreement — but neither
  runs the same measurement at two different thread counts. The
  `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition` test
  (`cleanup.rs:9799-9875`) compares 1 vs 4 workers in `refresh_proof_universe_with_limit`, which
  is a *different* concurrency knob; it proves parity for the workspace-graph refresh, not for
  `traverse`.

**32-bit overflow.** **No risk, and the premise in the brief is wrong.** Both accumulators are
  `u64` (`:87-88`), not `usize`, and both additions go through `checked_add`, which sets
  `uncertain` rather than wrapping (`:151`, `:160`, `:259`, `:268`). The only
  `usize` in the public surface is `worker_threads()`'s return. The one width-sensitive value is
  internal to `dua-core`: `DirectoryId` is `NonZeroU32` and its constructor carries
  `.expect("directory identifier overflow")` (`dua-core lib.rs:188`, `:1389`) — reachable only
  past 2^32 directories in one walk.

**Edge cases:**

| Input | Result | Location |
|---|---|---|
| Nonexistent path | `TargetStats { bytes: 0, entries: 0, newest: None, uncertain: true }` | `:109-115` |
| A regular file, not a directory | Identical to nonexistent — `is_dir()` fails the match | `:109-115` |
| An empty directory | `entries: 0, bytes: 0, newest: None, **uncertain: false**` | `:132-135` |
| A path the process cannot open | Root guard passes, walk yields `Err` → `uncertain: true` | `:125-131` |
| An unreadable *sub*directory | Entry error → `uncertain`, `break` | `:125-131` |
| A directory of only empty files | `bytes: 0` but `entries: N`, `uncertain: false` — not "empty" | `:158-172` |
| A symlink as the target root | `uncertain: true` (rejected by the guard) | `:109-115` |

The empty-directory row is why `workspace.rs:1849` gates on `entries == 0` rather than `bytes == 0`: a target containing 10 000 zero-byte files is a real build artifact tree with real
  reclaimable inodes, and gating on bytes would silently classify it as empty.

**Max depth or file-count budget?** **None, in this module.** There is no depth cap, no entry cap,
  and no timeout anywhere in `traverse.rs`. A pathological tree is bounded only by the filesystem,
  by the error-to-`uncertain` path, and — for the activity functions only — by the early exit at
  `:375-377`, which is an emergent budget for *active* projects rather than a designed one.
  Nothing truncates silently, because there is no truncation; a run that cannot finish simply runs
  longer. Note the asymmetry: the size walks have no early exit at all, so a single enormous
  output root costs its full traversal no matter how many siblings are measured concurrently.

**Can this module panic? Not any more — the one reachable path is closed.** It used to.
  `dua_core::walk_roots` asserts root-index uniqueness:
  `assert_eq!(indices.len(), roots.len(), "root indices must be unique")` (`dua-core lib.rs:659`).
  `measure_many_targets` used to copy indices straight from its input (`:186-187`) into `roots`
  (`:204`) and pass them to `walk_roots` (`:220-226`) with no dedup, so **a caller passing two
  targets with the same `usize` panicked inside the engine.** The fix is a `seen_indices` set
  (`:191`) and a `continue` on a repeat (`:198-200`): the first occurrence wins, later duplicates
  are dropped, and every occurrence of that index reports the one measurement. The behaviour is
  stated in the function's own doc comment (`:180-184`) and pinned by
  `duplicate_candidate_indices_do_not_panic_and_are_measured_once` (`:529-567`).

  The invariant that matters now is enforced *in this module* rather than in the engine: the
  caller may repeat an index, and the module guarantees one result per input with identical
  statistics for identical indices. What it no longer guarantees is that a repeated index means
  two distinct physical targets — the second path is silently not measured. A caller that passes
  the same index for two distinct roots and expects both to be sized now gets the first one's
  numbers twice.

Otherwise: no `unwrap`, no `expect`, no slicing, and no integer casts in the production half of
  `traverse.rs` — every fallible step is a `match` that sets `uncertain`. The `unwrap_or_default()`
  at `:216` and `:289` is a `HashMap` miss fallback that yields a default `TargetStats`; it is
  reached only if a root was somehow neither walked nor pre-seeded, and the caller treats the
  resulting `uncertain: false, entries: 0` as empty rather than failed. The only `panic!` in the
  file is a test assertion (`:621`).

**Filesystem changes during the walk.** The walk **skips and marks uncertain**; it neither aborts
  nor silently miscounts. A file deleted between `readdir` and `symlink_metadata` yields `Err` at
  `:138-141` (single) or `:245-249` (batch) → `uncertain = true`. In the batch path the `if
  stats.uncertain { continue; }` guard at `:233-235` then stops that root from accumulating
  anything further, so **no partial measurement is ever presented as a complete one** — the
  property `traverse.rs:13-14` claims. On Windows the extra `fs::canonicalize` inside `filesize`
  (§7) is a second TOCTOU window that also resolves to `Err` → `uncertain`.

**This is measurement, not authorization.** A measurement taken at scan time says nothing about
  the state at delete time — a build can start between the two. The architecture handles that one
  layer up: `cleanup.rs:3537-3605` re-runs `workspace_member_activity` *and*
  `measure_single_target` immediately before deletion and converts any `Recent` verdict, any fresh
  `newest`, and any `uncertain` into a hard `Err` that blocks the transaction
  (`cleanup.rs:3564-3569`, `:3384-3389`, `:3399-3404`, `:3405-3410`). That is invariant 3 in [the
  overview](overview.md) implemented, and it is the reason this module can be as trusting as it
  is.

## 9. Testing

**The briefing's premise is wrong: `traverse.rs` does have an inline test module.**
  `traverse.rs:495-624` — 130 lines, six tests, all under `#[cfg(test)] mod tests`:

| Test | Line | What it establishes |
|---|---|---|
| `worker_pool_is_bounded` | `:501` | `worker_threads()` is in `1..=8` |
| `single_and_batched_target_measures_agree` | `:507` | Batch and single agree on `entries` and `bytes` for 3 roots — the cross-check for §5's duplication |
| `duplicate_candidate_indices_do_not_panic_and_are_measured_once` | `:529` | A repeated `candidate_index` is measured once and every occurrence reports that one measurement, instead of tripping `dua_core::walk_roots`' `assert_eq!` |
| `candidate_count_does_not_grow_worker_pool` | `:570` | 32 roots still measure, pool still ≤ 8 |
| `synthetic_wide_and_deep_trees_count_entries` | `:587` | 64 flat files → 64 entries; 12 nested dirs + 1 file → 13 entries (root excluded) |
| `source_scan_short_circuits_recent_files` | `:611` | A fresh file yields `Recent`, not `Quiet` |

`tests/end_to_end.rs` and `tests/cli_contract.rs` give indirect end-to-end coverage:
  `end_to_end.rs:205-210` asserts a real artifact is *sized* through the JSON contract (`bytes >= 8192`, and the human formatter agrees), and `end_to_end.rs:50-70` backdates both files and
  directories — with a `windows`-specific helper, because directory mtime is part of the verdict
  (`workspace.rs:4876-4977`).

`scripts/release-benchmark.py` is a genuine indirect gate: `COUNTER_KEYS` (`:60-70`) includes
  `groups_measured`, `reportable`, and `bytes`, and `TIMING_KEYS` (`:42-49`) includes
  `source_activity` and `output_sizing`. `bytes` and `reportable` are produced entirely by this
  module, so a semantic change to sizing or to the uncertainty rule moves those counters and fails
  `.github/workflows/ci.yml:156`. Wall-clock timings are recorded but never gated
  (`release-benchmark.py:12-14`).

`examples/traversal-qualification.rs` is **not** a test of this module. It re-implements its own
  walk over `policy::global_discovery_policy()`'s roots (`:50-104`) to qualify the *engine and the
  prune callback*, and its stage 6 calls `discovery::discover_manifests_with_attribution`
  (`:35-40`). It does not call into `traverse.rs` at all — it re-uses the same `dua-core` engine
  over the same production roots. It is run manually —
  `cargo run --release --example traversal-qualification -- <stage> [workers] [completion]` (`:3`) —
  and a repo-wide grep finds
  **no CI reference to it**. Its `workers` argument is clamped to 32 (`:76`) and defaults to 8
  (`:23`), so it can explore thread-count effects production cannot; nothing asserts anything
  about its output.

### Under-verified properties

Filesystem measurement is the easiest thing in this project to test against a fixture that does
  not represent reality, and the project's own `plans/registry.md` lesson applies verbatim: *a
  green test proves only that its own premises hold* ([the overview](overview.md)
  §11). Four gaps:

1. **Allocated-vs-apparent is never asserted, on any platform.** Every size
   test writes dense non-sparse files (`:513`, `:540-541`, `:576`, `:592`, `:603`) and compares batch against single,
     never against an expected number. No fixture creates a sparse file, a compressed NTFS file,
     an APFS clone, or a hard link. So the §7 claim — sparse files report allocated bytes, hard
     links are counted N times, `SizeMetric::Allocated` is the truthful label — is supported by
     reading `filesize-0.2.0` and `workspace.rs:1879-1888`, **not by any test in this
     repository.** A `filesize` change to its Unix branch, or a divergence between its cfg and
     `workspace.rs`'s duplicate cfg, would pass the suite.
2. **The uncertainty paths are almost entirely untested.** No test in
   `traverse.rs` constructs a permission error, a mid-walk deletion, an EMFILE, or a vanished
     file; the batch-vs-single parity test (`:507-526`) runs on a healthy temp dir where every
  `stat` succeeds. The *consequences* of uncertainty are well covered
   (`workspace.rs:1833-1848`, `cleanup.rs:3590-3595`), but the module's ability to *produce*
     `uncertain` from each failure mode (walk error `:127-130`, `symlink_metadata` error
     `:138-141`, `modified()` error `:145-148`, `filesize` error `:167-170`, `entries` overflow
     `:151-157`, `bytes` overflow `:160-166`) is untested per mode. The two overflow branches are
     unreachable in practice and untestable without injection.
3. **The nested-repository boundary rule** — specifically the decision to
   exclude the boundary directory's own mtime (`:357-360`) — is verified only indirectly, by
   `workspace.rs:2950-2980`, which tests that a *recent output file* does not protect a
   workspace and a *recent source file* does. The nested-repo case the comment describes is not
   covered by any test I could find.
4. **The directory-mtime rule is untested, and the one test that looks like it covers the rule
   does not discriminate it.** `source_scan_short_circuits_recent_files` (`:611-623`) writes one
   fresh file into a freshly created temp directory, and the walk root is the temp directory
   itself — whose own mtime is fresh and is *not* excluded (`:389-391`). The walk yields the root
   first, so `Recent` is returned on the root's directory mtime before the file is ever examined.
   The test would still pass if the file were deleted, if the guard ignored files entirely, or
   if directories were excluded from the evidence. What would discriminate the behaviour is a
   pair of fixtures with opposite signs: a backdated file under a *fresh* directory (expect
   `Recent`) and a fresh file under a *backdated* directory (expect `Recent` as well, because the
   directory's mtime moved when the file was written). Neither exists.

## 10. Review checklist

1. **`traverse.rs:174` / `:287-290`** — the two exits. Any change that can
   return a `TargetStats` without `uncertain` being set is a safety regression:
    `traverse.rs:13-14` promises no partial scan is presented as complete, and `cleanup.rs:3590`
      turns `uncertain` into a block.
2. **`traverse.rs:191` + `:198-200`** — the dedup guard that keeps caller indices unique before
   `walk_roots` (`:220-226`) can assert uniqueness (`dua-core lib.rs:659`). If you add a second
     producer of indices, keep the dedup — and remember that the *first* occurrence of an index
     wins, so a repeated index now silently drops the second path rather than measuring it.
3. **`traverse.rs:186-187` vs `:180`** — the doc promises "input order", the
   code delivers "index-sorted order". They agree only because `workspace.rs:1776-1780` uses
     `.enumerate()`. Either fix the doc or document the uniqueness + ascending precondition.
4. **`traverse.rs:136` + `:244`** — the re-`stat` that duplicates the engine's
   eager `entry.metadata()` (`dua-core lib.rs:776`). This is the module's largest avoidable
     syscall cost, and it exists for a stated reason (`traverse.rs:9-12`). If anyone "optimises"
     it away by reading `entry.metadata()`, the allocated-bytes metric and the error-conservatism
     guarantee both move.
5. **`workspace.rs:1879-1888` vs `filesize-0.2.0/src/lib.rs:58-126`**
   — the `SizeMetric` label is a second, hand-maintained statement about which platform branch of
     a dependency actually ran. Change one cfg, change both.
6. **`traverse.rs:330-348` vs `:382-402`** (and `:429-449` vs `:473-493`) —
   the exclusion rule is written twice per function, once for descent and once for evidence. Any
     change to one copy must be made to the other, and nothing enforces it.
7. **`traverse.rs:370` vs `:136`** — `fs::metadata` (follows symlinks) in the
   activity path against `fs::symlink_metadata` (does not) in the sizing path. The asymmetry is
     documented at `traverse.rs:309-310`; confirm it is still wanted rather than inherited.
8. **`traverse.rs:127-130`, `:138-141`, `:145-148`, `:167-170`** — the four
   `uncertain` sites in the single path. They discard the error, so the diagnostic a user can
     eventually see (`workspace.rs:1835-1839`) names the group but not the cause. If the project
     ever wants actionable "permission denied on X", this is the place to thread it.
9. **`traverse.rs:158`** — `meta.is_file()` is the only byte-contributing
   branch. Adding sockets, or trusting `entry.file_type` from the walk instead of the fresh
     `symlink_metadata`, changes the byte total and the Windows behaviour of `filesize`'s
     `canonicalize` in ways nothing in the suite would catch.
10. **`traverse.rs:376` / `:467`** — `Recent(t)` carries the first recent
    timestamp, not the max. Every current caller discards it (`workspace.rs:2153`,
      `cleanup.rs:3564`), which is the only reason this is not a determinism bug. A caller that
      starts reading `t` inherits scheduling nondeterminism.
11. **`traverse.rs:370-377` / `:461-468`** — the activity guard reads the mtime of every entry
    it walks, **including directories**, and the walk root is not excluded. Any change that
    filters the guard down to regular files silently changes which projects count as active, and
    the existing test (`traverse.rs:611-623`) would not notice.
