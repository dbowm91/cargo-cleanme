# M021 — Scan resolution and measurement cost

Status: **closing** — implemented in the working tree on `c333501` and
verified by the ladder and the 1.89 lane. **Not closed**: it has not been
carried by a published release.

Closure record:
`plans/closure/artifact-discovery-cleanup/m021-status.md`.

Date: 2026-11-09

Repository baseline: `c333501` (published v0.2.13; `CHANGELOG.md` has no
`[Unreleased]` section).

Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Source milestones: M006B (closed) and M006F (closed). C003 and C004 recorded
generic scan performance as a deferred line and did not claim it. This plan
claims that line. It is not a corrective: no closed milestone asserted this
work was done.

Primary class: performance / Cargo subprocesses / traversal hot path

Dependencies: soft on closed M006B and closed M006F. No dependency on M020,
C033, or C034. Nothing in those plans waits on this one.

## 1. Objective

Cut the wall time of a scan in the four places the current tree still spends
it, and leave the answer the scan produces the same.

1. `cargo locate-project` runs for a discovered manifest only when no
   successful `cargo metadata` has already named that manifest.
2. An explicit discovery walk (`scan ROOT`, and any other caller of
   `discover_manifests_root`) uses the same metadata policy the global walk
   already uses.
3. Inventory sizing of output trees uses the metadata the walker already
   collected, on platforms where that metadata is the same number the proof
   measurement reports.
4. Inventory source-activity walks share one worker pool.

The pre-delete proof does not take these shortcuts. `measure_single_target`
and the `workspace_member_activity` call in `cleanup.rs` keep today's
re-stat.

There is no wall-clock closure gate. M006F already rejected a 120-second
gate. Closure is semantic parity, plus one recorded `--stats` before/after
on a real tree.

## 2. Triggering evidence

`run_scan` (`src/main.rs`) spends its time in discovery, Cargo resolution,
source activity, and output sizing. `--stats` already prints that split
(`ScanCounters::timings_line` in `src/domain.rs`).

M006F's reference-host sample, recorded in
`plans/closure/artifact-discovery-cleanup/006f-status.md`: production
discovery of a full machine was 17.6 s for 4,670,246 entries; the full CLI
inventory on the same host was 97.6 s. Traversal was about a fifth of that
run. The same closure measured the worker cap: 4 threads 25.0 s, 8 threads
16.9 s, 16 threads 33.2 s, with 69.8 s system time against 10.3 s user time.
The walk is syscall-bound. Raising the cap is not this plan.

What the tree does today, re-read at the baseline:

- `resolve_workspaces_with_coverage` (`src/workspace.rs`) locates every
  manifest, then runs one `cargo metadata --offline --locked --no-deps` per
  located workspace root. Both phases are capped at
  `RESOLUTION_CONCURRENCY_CAP` (4). The comment on `resolve_workspaces` and
  the comment in `repeated_members_invoke_metadata_once` say why: the
  sequential loop could skip a locate after an earlier metadata call had
  named the member, and that skip serialized the phase. The parallel rewrite
  kept the metadata dedup and dropped the locate skip. A workspace whose
  root manifest is in the discovered set therefore pays one locate per
  member plus one metadata.
- `discover_manifests_root` walks with `dua_core::Options::default()`
  (metadata on) and then `symlink_metadata`s every `Cargo.toml`. The global
  walk already sets `skip_metadata` on Linux and macOS
  (`global_walk_knobs`). M018's verification pass shares those knobs; this
  plan does not change them.
- `measure_many_targets` and `measure_single_target` (`src/traverse.rs`)
  let the walker stat each entry, discard that stat, and
  `symlink_metadata` the path again. The comment names the type mismatch:
  macOS and Windows `dua_core::Metadata` are not `std::fs::Metadata`, so
  they cannot be passed to `filesize`. On macOS, `dua-core` 4.1
  `Metadata::allocated_size` is `blocks * 512` and `modified()` is the
  mtime — the same figure `filesize::file_real_size_fast` reports from
  `st_blocks`. On Linux the walker's metadata *is* `std::fs::Metadata`.
  On Windows, `filesize` calls `GetCompressedFileSizeW`, which is a
  different number from directory-enumeration size. The re-stat is also
  the vanish check the proof measurement is documented to perform.
- `analyze_groups_detailed` calls `workspace_source_activity`, which calls
  `workspace_member_activity` once per outermost member root. Each call
  builds and joins its own pool (`dua_core::walk`). The function collects
  walker metadata and then calls `fs::metadata`, which follows symlinks.
  `dua_core::stream_roots` (4.1) already accepts roots into one live pool.
  Sizing of many output roots already shares one pool
  (`measure_many_targets`). Source activity does not.
- A quiet tree has no early exit. Writing a file does not update the parent
  directory mtime on Unix, so an old directory can still hold a recently
  written file. The early return in `workspace_member_activity` helps only
  an active tree. This plan does not try to prune quiet trees by directory
  mtime.

`analyze_groups` is the inventory used by the read-only scan and by
cleanup's first pass. The bytes and the activity verdict a person sees come
from it. The spawn-time proof in `cleanup.rs` measures again with
`measure_single_target` and `workspace_member_activity`. Those two calls
stay on the re-stat path, so a shortcut in the inventory cannot authorize a
delete the proof would refuse, and it cannot report a byte count the proof
measurement would disagree with on a stable tree: the acceptance test is
agreement between the two measurements.

## 3. Invariants

- Cargo remains the authority for workspace membership. Path ancestry
  chooses which locate to spawn first. It never marks a manifest covered.
- A manifest is covered without a locate only when a *successful* metadata
  result names it, compared with the same canonical identity the resolver
  already uses (`canonical_or_absolute`). A metadata failure, a transport
  failure, or an unparseable result covers nothing.
- Unresolved manifests stay in the coverage ledger. Read-only scan stays
  partial-result tolerant. Cleanup stays fail-closed on an in-boundary
  unresolved participant (ADR 007).
- Diagnostic order, diagnostic text, workspace order, and the unresolved
  ledger stay those of the sequential emulation. The allowed delta is which
  locate processes actually spawn, and therefore `cargo_locate_calls` and
  `cargo_locate_nanos`.
- `cargo metadata` stays one spawn per distinct workspace root, with
  `--offline --locked --no-deps --format-version 1`.
- `RESOLUTION_CONCURRENCY_CAP` stays 4 unless work package E records a
  matched gain and changes it in the same milestone. The proof-refresh cap
  stays aligned with it if it changes, and stays put if it does not.
- Manifest sets, prune counters, and `coverage_complete` are unchanged by
  the explicit walk's metadata policy.
- `global_walk_knobs` is unchanged. M018's verification pass keeps the
  knobs it shares with the platform walk.
- Inventory `TargetStats` (`bytes`, `entries`, `newest`, `uncertain`,
  `package_dir`) agree with `measure_single_target` on a stable tree.
- Inventory source-activity verdicts agree with today's
  `workspace_member_activity` on a stable tree: `Recent`, `Quiet`, and
  `Err(())`, including a followed symlink, a nested-repository boundary,
  and an excluded output root.
- The pre-delete proof keeps the re-stat. `cleanup.rs`'s calls to
  `measure_single_target` and `workspace_member_activity` stay on that
  path. M006B's removal of locate from the proof refresh is untouched.
  No snapshot is reused across cleanup candidates.
- `worker_threads()` stays clamped at 8.
- No new flag, config key, reason code, exit code, or JSON field.
- Rust 1.89 remains the MSRV.

## 4. Non-goals

- A hardcoded `node_modules` / `.cache` / hidden-directory prune. M006A
  rejected performance blacklists. `scan.ignore` is the operator control.
- Pruning a source tree because its directory mtime is old.
- Raising the discovery or measurement worker cap. M006F measured 16 as
  slower than 8 on the reference host.
- Replacing `dua-core`, or switching walk order. M006F kept `ParentFirst`.
- Speeding the pre-delete proof by trusting walker metadata there.
- Removing the N-candidate × N-workspace proof refresh. That cost is the
  one C003 recorded and M006B narrowed; it stays out of this plan.
- Canonicalizing macOS member roots whose path contains a symlink. C003
  recorded that as a separate follow-up.
- A daemon, a filesystem index, or a watcher.
- A version bump or a changelog entry. Those belong to the release that
  carries the change.

## 5. Work package A — Locate only what metadata has not named

Replace phase 1 of `resolve_workspaces_with_coverage`.

A discovered manifest is a **wave-1** manifest when no other discovered
manifest's directory is a strict ancestor of its directory. Wave 1 is
located in parallel, under the existing cap, joining so the emulation still
sees outcomes in input order. Metadata then runs once per distinct located
root, as it does today.

A manifest that was not in wave 1 is located in wave 2 only when no
successful metadata result from wave 1 names it. Wave 2 uses the same cap.
A metadata failure covers nothing, so the manifests under that root are
located and then fail closed through the existing emulation.

Sibling members whose workspace-root manifest was **not** discovered stay
in wave 1. Neither is an ancestor of the other, so both are located. That
is `repeated_members_invoke_metadata_once`: two member manifests, no root
manifest in the input, two locates, one metadata. Keep that count. The
comment on the test currently says the bulk resolver locates every manifest
up front; rewrite the comment so it states the ancestor rule, or the next
change will "fix" the test back to a locate of every member.

The discriminating case is the one the current test pins the other way.
`n_member_workspace_requires_one_metadata_regardless_of_order` discovers the
root manifest and three members and asserts `cargo_locate_calls == 4`.
After this package the root is the only ancestor, metadata names the three
members, and the locate count is 1 in both input orders. Metadata count
stays 1. Rewrite that assertion as part of the change, in the same commit
as the production change. A test that still expects four locates would be
green only by pinning the cost this package removes.

These counts stay:

- `thousand_failed_lookups_keep_every_unresolved_manifest` — 1,000 failed
  locates, 1,000 unresolved manifests. They are siblings, and a failed
  locate covers nothing.
- `two_independent_workspaces_require_two_locate_two_metadata` — two
  locates, two metadata calls.
- Every `proof_cargo_locate_calls` assertion. Those count the proof
  refresh, which this package does not enter.

Add one fixture the current suite does not have: a root manifest, two
members, and a nested `Cargo.toml` that metadata does **not** name (a
non-member sitting under the workspace). Locate count is 2. The non-member
is unresolved or resolved as its own workspace according to what the fake
Cargo returns for its locate. Path ancestry must not swallow it.

The emulation's fast path (`member_to_root` hit) already skips metadata for
a covered member. Extend it so a covered member that was never located does
not increment `cargo_locate_calls` and does not require a `LocateOutcome`.
Diagnostic positions for manifests that *were* located stay in input order.

## 6. Work package B — Explicit discovery uses the global metadata policy

`discover_manifests_root` currently walks with
`dua_core::Options::default()`. Build its options from the same
`skip_metadata` bit `global_walk_knobs` already computes for this platform
(Linux and macOS skip; Windows follows whatever the global knob does
today). Do not read the profiling environment here, and do not change
`global_walk_knobs`.

The manifest check already re-stats with `symlink_metadata` and requires a
regular non-symlink file. It does not read the walker's metadata. Prune
decisions read `file_type`, which `dua-core` still supplies when
`skip_metadata` is set (it stats an entry whose type the directory
enumeration did not provide).

Required parity, on a fixture with a nested manifest, a `target/` directory,
a symlink directory, and a symlink `Cargo.toml`: the same sorted manifest
vector, the same prune total, the same `coverage_complete`, with and
without the bit. The existing global-walk parity across metadata modes is
the model (`synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps`).

## 7. Work package C — Inventory sizing trusts the walk on Linux and macOS

`measure_many_targets` is the inventory measurement
(`analyze_groups_detailed`). On Linux, use the walker's
`std::fs::Metadata` with `filesize::file_real_size_fast`. On macOS, use
`dua_core`'s `allocated_size()` and `modified()`. On Windows, keep the
re-stat: compressed size is what `filesize` reports, and directory
enumeration does not.

A walker metadata error, a missing mtime, or an entry the walker did not
stat marks that root `uncertain` and stops accumulating it, which is the
rule the re-stat path uses today. `package_dir` still trips on a
manifest-shaped file name; the full path is needed only for that entry.

`measure_single_target` keeps the re-stat. It is the proof measurement
(`cleanup.rs` calls it immediately before `cargo clean`).

`single_and_batched_target_measures_agree` must keep passing, and it must
gain a sparse file. Today every size fixture writes dense files, so a
divergence between `st_blocks` and apparent length would not fail the
suite (`architecture/08-traverse.md` §9). The new case asserts the batched
inventory and the single-target proof report the same `bytes` for a sparse
file. If they disagree on any platform, that platform keeps the re-stat.
Do not "fix" the disagreement by changing `measure_single_target`.

## 8. Work package D — One pool for inventory source activity

`workspace_source_activity` walks each outermost member root by calling
`workspace_member_activity`, and each call starts a pool.

Add an inventory entry point that submits those roots to one
`dua_core::stream_roots` pool (or one `walk_roots` if the roots are known
before the walk starts). The descend predicate stays per root: output roots
relevant to that member (`outputs_relevant_to`), VCS names, and the nested
repository marker. A `Recent` verdict for one workspace stops that
workspace from contributing further members and does not stop any other
workspace. Workers may already have read ahead, which is the behavior of
dropping today's iterator.

For a non-symlink entry, the inventory walk uses the walker's `modified()`.
For a symlink, it keeps `fs::metadata` so the followed mtime is the one the
proof function uses. An error from either is `Err(())`.

`workspace_member_activity` itself keeps the re-stat. `cleanup.rs` and any
other proof caller keep calling it. The inventory entry point is what
`analyze_groups_detailed` calls.

Required parity on one fixture: inventory verdict and
`workspace_member_activity` verdict agree for a quiet tree, a tree with a
recent file, a tree whose recent mtime is only on a symlink target, a tree
whose recent mtime is only inside a nested repository, and a tree that
cannot be read. `Quiet`'s timestamp payload is the max and must agree.
`Recent`'s timestamp payload is scheduling-dependent today and no
production caller reads it; do not assert the payload.

## 9. Work package E — Concurrency cap, measured or left alone

After package A, run one matched pair on a real tree with `--stats`: the
baseline binary and the new binary, same roots, same machine, release
build. Record `locate`, `metadata`, `source_activity`, `output_sizing`, and
wall time in the closure record.

Change `RESOLUTION_CONCURRENCY_CAP` only if that sample shows metadata still
dominating **and** a second matched pair at a higher cap (the candidate is
8, the traversal cap) is faster on the same tree. If the higher cap is
flat or slower, the cap stays 4 and the closure record says so. The
proof-refresh cap is the same constant's sibling
(`PROOF_REFRESH_CONCURRENCY_CAP`); a change updates both and the comment
that says they stay aligned. A non-change touches neither.

This package does not block A–D. A milestone that finishes A–D and records
"cap left at 4" has finished E.

## 10. Failure and contention

- A locate or metadata worker that panics degrades to the existing
  `WorkerFailed` / failed-spawn outcome. No manifest disappears.
- Wave 2 runs only after wave 1 metadata has been classified. A wave-1
  metadata error is not a reason to skip wave 2.
- Inventory walks keep per-root `uncertain` / `Err(())`. One root's failure
  does not fail a sibling root.
- Sharing a source-activity pool must not change the measured byte count or
  the activity verdict on a stable tree. Contention that only changes which
  `Recent` timestamp is observed first is acceptable, because no caller
  reads that timestamp.
- Dropping the inventory iterator still joins the pool. A cancelled scan
  must not leave walker threads behind. The existing `dua-core` drop
  behavior is the mechanism; a test that starts an inventory walk and drops
  it asserts the call returns.

## 11. Compatibility

No CLI, config, reason-code, exit-code, or JSON change. `--stats` counters
that count spawned locates will be lower on a workspace whose root manifest
was discovered. That counter is stderr, not the machine contract. The
timings line keeps its five fields.

`scan.ignore` / `scan.unignore` are unchanged. Explicit scope still bypasses
them.

## 12. Required tests

Rewrite in the same change as package A, so the suite cannot stay green by
pinning the old locate count:

- `n_member_workspace_requires_one_metadata_regardless_of_order` —
  `cargo_locate_calls` becomes 1; `cargo_metadata_calls` stays 1; both
  input orders.
- The comment on `repeated_members_invoke_metadata_once`. The count stays
  2 locates and 1 metadata.

Keep, unchanged:

- `thousand_failed_lookups_keep_every_unresolved_manifest` (1,000 locates).
- `two_independent_workspaces_require_two_locate_two_metadata`.
- Every assertion on `proof_cargo_locate_calls`.

Add:

- Root plus members plus a nested non-member: the non-member is located;
  the members are not.
- Metadata failure on the root: member manifests are still located and
  still unresolved. Covered-without-locate happens only after success.
- Explicit-walk manifest and prune parity with `skip_metadata` on and off.
- `single_and_batched_target_measures_agree` extended with a sparse file.
- Inventory source-activity versus `workspace_member_activity` on the five
  trees in package D.
- A proof-path test that still observes a re-stat: the existing cleanup
  proof tests stay, and a focused test (or an extension of one) shows
  `measure_single_target` reports `uncertain` when an entry vanishes. If
  that vanish case cannot be built portably, say so in the closure record
  rather than claiming it.

Linux, macOS, and Windows. The Windows inventory-sizing path is the re-stat
path; the sparse-file parity test is what keeps an accidental Windows
shortcut from shipping a different byte count.

## 13. Verification

The ordinary ladder, in order, stopping at the first failure:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-selector-qualification.py --self-test && python3 scripts/check-selector-qualification.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

Plus the MSRV lane, because this plan touches library code the 1.89 job
compiles with `--locked`:

```sh
cargo +1.89 test --locked --all-targets
```

No new dependency. `Cargo.lock` is not regenerated. `generate-docs` is
expected to be a no-op because the CLI surface does not change; if it is
not, the surface change was out of scope and the milestone stops.

Package E's real-tree sample is qualification evidence, recorded in the
closure. It is not a CI gate. `scripts/release-benchmark.py` records wall
timings and does not fail on them; do not add a timing threshold.

## 14. Documentation

Re-derive, from the source after the change, the paragraphs that currently
describe the old cost. Do not shift their line numbers arithmetically.

- `architecture/07-workspace.md` §4, "The four phases". It says every
  manifest gets exactly one `locate-project` spawn, and that a member-cache
  hit saves metadata but not the locate. Both sentences become false.
- `architecture/05-discovery.md` §4, the options row that says the
  sequential path uses full metadata.
- `architecture/08-traverse.md` §7, the row that calls the second stat the
  single largest avoidable cost. After package C that sentence is true of
  the proof measurement and of Windows inventory, and false of Linux and
  macOS inventory. Say which.

`architecture/overview.md` §6 step 5 ("`cargo metadata` once per unique
canonical workspace root") stays true. Leave it unless the implementation
makes it false.

No `CHANGELOG.md` edit. No `config.toml` edit. No generated completion or
man page.

## 15. Acceptance criteria

M021 closes when:

- A workspace whose root manifest was discovered pays one locate for that
  root, and its members are not located when metadata names them.
- A nested manifest metadata does not name is still located.
- A failed metadata result does not cover its members.
- Failed sibling locates are all still issued
  (`thousand_failed_lookups_keep_every_unresolved_manifest`).
- Explicit discovery with the platform `skip_metadata` bit returns the same
  manifest set and prune total as metadata-on.
- Linux and macOS inventory sizing agrees with `measure_single_target`,
  including a sparse file. Windows inventory sizing still re-stats.
- Inventory source activity agrees with `workspace_member_activity` on the
  package D fixtures, and the proof calls in `cleanup.rs` still re-stat.
- `RESOLUTION_CONCURRENCY_CAP` is either still 4, with the sample recorded,
  or is a higher cap whose matched sample is in the closure record.
- The ladder and the 1.89 lane were run and their results are in the
  closure record.
- No cleanup decision, exit code, or JSON field changed.

## 16. Stop conditions

Stop the package, and say so in the closure record, if:

- Skipping a locate would require treating path ancestry as membership.
- Inventory bytes disagree with `measure_single_target` on a stable tree.
  That platform keeps the re-stat.
- Sharing the source-activity pool changes a `Recent` / `Quiet` / `Err`
  verdict on a stable tree.
- A higher Cargo concurrency cap is not faster on the measured tree.
- The remaining time is the host filesystem or Cargo itself, and the
  `--stats` sample shows these four packages are no longer the cost.
- A change would touch `global_walk_knobs`, the pre-delete proof, or the
  proof-refresh locate elimination from M006B.

## 17. Closure evidence

`plans/closure/artifact-discovery-cleanup/m021-status.md` must contain:

- the implementation commit;
- a requirement-to-evidence matrix, one row per acceptance criterion;
- the verification commands actually run, with results, including any
  command that was skipped;
- the package E sample: tree, binary, and the five timing fields, before
  and after;
- the decision on `RESOLUTION_CONCURRENCY_CAP`;
- platform notes for the Windows re-stat and for any parity case that
  could not be built;
- unresolved findings, if a stop condition fired;
- disposition.
