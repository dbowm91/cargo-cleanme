# Workspace — Cargo resolution, physical grouping, and cleanup units

> Component deep dive · part of the [architecture overview](overview.md)

`src/workspace.rs` — 6301 lines: 2160 production, 4141 inline `#[cfg(test)]`,
79 `#[test]` functions. The test module begins at `workspace.rs:2311`.

It is the bridge between "we found some `Cargo.toml` files" and "we know which
physical bytes we are allowed to touch". Everything downstream of it — grouping,
measurement, cleanup units, and the ownership vocabulary `cleanup.rs` authorizes
against — is downstream of the picture it builds.

---

## 1. Responsibility

**Owns:** turning a flat list of discovered manifest paths into a *proven* set of
physical output groups with an ownership class, measured once each, assembled
into per-workspace cleanup units.

The production half imports exactly three things from the crate
(`workspace.rs:8-12`): `domain::*` for the whole shared vocabulary,
`progress::{ProgressObserver, ScanPhase}` to report gate transitions without
knowing the renderer, and `traverse` — used at exactly two call sites,
`measure_many_targets` (`:1897`) and `workspace_member_activity` (`:2302`).
Plus four fully-qualified `crate::domain::elapsed_nanos` uses (`:564`, `:694`,
`:1799`, `:1901`).

**Does not own:**

- **Activity / inactivity.** Source recency comes from `traverse::workspace_member_activity`
  (`workspace.rs:2302`); byte sizing and output recency come from
  `traverse::measure_many_targets` (`workspace.rs:1897`). This module decides
  *whether to ask* and *what the answer means for a group*, never the answer.
- **Deletion.** Nothing here removes a byte. `cleanup.rs` invokes `cargo clean`.
- **Cargo configuration semantics.** The module doc states it outright
  (`workspace.rs:1-6`): it never reimplements Cargo's config merge rules, it asks
  Cargo. The only parsing of Cargo output is JSON — with exactly one narrow
  parse of a Cargo *message* (§4, `believed_workspace_manifest`).
- **Reporting.** This module produces `PhysicalOutputGroup`; the report-facing
  `EligibleOutputGroup` is built in `main.rs:1664-1677`.

**The workspace → cleanup edge is one-way, and in this file it is test-only.**
The production half of `workspace.rs` contains *zero* references to
`crate::cleanup`. The two calls are at `workspace.rs:4539` and `:4818`
(`crate::cleanup::covering_is_authorized`), both inside the test module,
asserting that source-overlapping groups are refused even by the most
permissive caller.
So the dependency direction that [the overview](overview.md) calls out is
`cleanup → workspace` in production (`cleanup.rs:2710` resolves,
`cleanup.rs:2752` groups, `cleanup.rs:2757` analyses, `cleanup.rs:2767` builds
units, `cleanup.rs:3103` refreshes a root manifest, `cleanup.rs:3589` re-maps
groups, `cleanup.rs:3621` reduces the covering union, `cleanup.rs:3700` reads
member roots) plus a test-only back-reference. There is no cycle and no
production import from `workspace` into `cleanup`.

Call sites in the read-only scan: `main.rs:1501` (runner), `main.rs:1502`
(`resolve_workspaces`), `main.rs:1635` (`build_groups`), `main.rs:1651`
(`analyze_groups`).

---

## 2. Public surface

Twenty-nine public items: twenty-six types and functions, plus three public
methods. Nothing else in the file is reachable from outside.

### Process / IO

| Item | Signature | Contract |
|---|---|---|
| `ProcessOutput` | `struct { success: bool, code: Option<i32>, stdout: Vec<u8>, stderr: Vec<u8> }` (`:21-27`) | A cargo subprocess result. Raw `Vec<u8>` on both streams, so a non-UTF-8 filename in cargo's output cannot break the decoder. Failed locate/metadata stderr is converted to a bounded causal excerpt (`:199-228`). |
| `CargoRunner` | `trait { fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> }` (`:29-31`) | The project's only external-process boundary. No supertraits, no associated types. |
| `SystemCargoRunner` | `struct SystemCargoRunner;` (`:78-93`) | The real implementation. Zero-sized. |
| `refresh_workspace_from_root_manifest` | `(root_manifest: &Path, runner: &dyn CargoRunner, counters, diagnostics, observer) -> Option<ResolvedWorkspace>` (`:59-76`) | Re-resolve one *known* root directly through `cargo metadata`: one process, no `locate-project`. Never discovers anything. Used by the cleanup final proof (`cleanup.rs:3103`). |

### Resolution

| Item | Signature | Contract |
|---|---|---|
| `resolve_workspaces` | `(manifests: &[PathBuf], runner, counters, diagnostics, observer) -> Vec<ResolvedWorkspace>` (`:459-467`) | Thin wrapper: `resolve_workspaces_with_coverage(..).workspaces`. Read-only scan entry point. |
| `resolve_workspaces_with_coverage` | same params `-> ResolutionCoverage` (`:738-1057`) | The real implementation: bulk-parallel locate plus grouped metadata with a sequential emulation (§4). Also returns the *unresolved* set, which is what makes the destructive scope fail-closed. |
| `enclosing_workspace_root` | `(dir: &Path, runner: &dyn CargoRunner) -> Option<PathBuf>` (`:617-635`) | **ADR 009.** The workspace that encloses `dir`, by asking Cargo: `cargo locate-project --workspace` run *in* the directory, with **no** `--manifest-path`. `None` for "there is no enclosing workspace" — which Cargo signals by failing, and which is a normal answer here, not an error. |
| `working_directory_scope_roots` | `(dir: &Path, runner: &dyn CargoRunner) -> Vec<PathBuf>` (`:655-664`) | **ADR 009.** The two roots the operator's position contributes to the maintenance scope: the enclosing workspace, when there is one, and the directory itself. |
| `capability_from_metadata` | `(had_build_field: bool, target: &str, build: Option<&str>, env_set: bool) -> CargoCapabilities` (`:124-142`) | Pure state machine over metadata evidence. No IO. |
| `clean_capabilities_from_version` | `(version: &str) -> CargoCleanCapabilities` (`:146-165`) | Exact-string allowlist of Cargo versions whose `cargo clean` selector behaviour was qualified. |
| `UnknownDisposition` | 3-variant enum (`:399-409`) | The M015 proof-island assignment for one unresolved manifest: `PassengerOf { named_manifest }`, `ScopeNote`, `MergeAll`. |
| `attribute_unknown` | `(participant: &UnresolvedOwnershipParticipant, workspaces: &[ResolvedWorkspace]) -> UnknownDisposition` (`:419-438`) | Pure failure-class attribution. No spawns, no mutation, no counter or diagnostic changes. |

### Grouping

| Item | Signature | Contract |
|---|---|---|
| `outermost` | `(roots: &[PathBuf]) -> Vec<PathBuf>` (`:1389-1407`) | Collapse a root set to its minimal non-nested antichain, sorted. The primitive that makes "counted once" possible. |
| `build_groups` | `(workspaces: &[ResolvedWorkspace]) -> Vec<RawGroup>` (`:1466-1622`) | Union-find over canonical physical roots; assigns an `OutputOwnershipClass` to each connected component. |
| `RawGroup` | `struct { physicals, covering, display, owners: Vec<usize>, ownership, source_overlap, nested_source_overlap }` (`:1624-1642`) | Pre-measurement group. `owners` are indices into the caller's `workspaces` slice, not IDs. |
| `has_artifact_entries` | `(path: &Path) -> Option<bool>` (`:1646-1649`) | `Some(true)` if the directory has ≥1 entry, `Some(false)` if empty, `None` if unreadable. One `read_dir` + one `next()`. |
| `GroupAnalysisInput` | `struct { group: RawGroup, owner_workspaces: Vec<ResolvedWorkspace> }` (`:1651-1654`) | **Dead.** Declared, never constructed, never referenced anywhere in `src/` or `tests/`. `analyze_groups_detailed` does not take it — see §7. |
| `workspace_member_roots` | `(ws: &ResolvedWorkspace) -> Vec<PathBuf>` (`:1656-1661`) | Member `source_root`s, deduped, passed through `outermost`. Also used by `cleanup.rs:3700`. |
| `GroupSkipReason` | 7-variant enum (`:1623-1632`) + `detail(self) -> &'static str` (`:1635-1645`) | Why a group produced no candidate. Full table in §7. |
| `GroupOutcome` | `struct { group: RawGroup, measured: Option<PhysicalOutputGroup>, skip: Option<GroupSkipReason> }` (`:1653-1658`) | One per input group, in input order. Exactly one of `measured`/`skip` is `Some`. |
| `analyze_groups_detailed` | 8 params `-> Vec<GroupOutcome>` (`:1669-1678`) | **The real implementation.** Fail-fast gate pipeline. |
| `analyze_groups` | identical params `-> Vec<PhysicalOutputGroup>` (`:1991-2000`) | **Delegates** to `analyze_groups_detailed`, drops outcomes with no `measured`, sorts by bytes descending then `display_path`. Used by the read-only scan. |
| `map_workspace_groups` | `(ws: &ResolvedWorkspace, groups: &[RawGroup]) -> Vec<usize>` (`:2126-2128`) | The inverse direction: which group indices this workspace's `OutputSet` can touch. |

### Units

| Item | Signature | Contract |
|---|---|---|
| `AffectedGroup` | `struct { display, covering, ownership, kinds: Vec<OutputRootKind>, measured, skip }` (`:2154-2164`) | One physical group as seen *through one workspace's* output set. |
| `AffectedGroup::kind_label` | `(&self) -> String` (`:2168-2182`) | `"target"`, `"build"`, `"target+build"`, or `"output"` when there are no kinds. **Not compile-checked** — the final `_` arm at `:2180` catches any future kind instead of failing the build. |
| `CleanupUnit` | `struct { workspace_idx, id, root, root_manifest, output, capability, groups, unmapped, covering, bytes }` (`:2192-2206`) | One `cargo clean` invocation's full footprint. |
| `CleanupUnit::fully_measured` | `(&self) -> bool` (`:2210-2212`) | `unmapped.is_empty() && groups.iter().all(\|g\| g.measured.is_some())`. |
| `build_cleanup_units` | `(workspaces, outcomes: &[GroupOutcome]) -> Vec<CleanupUnit>` (`:2220-2291`) | One unit per workspace that has ≥1 measured affected group. |

---

## 3. The CargoRunner seam

```rust
pub trait CargoRunner: Sync {                              // workspace.rs:29
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
}
```

One method. No associated types. The `: Sync` bound is what makes
`std::thread::scope` legal in the bulk resolution phases below: one
`&dyn CargoRunner` is shared across worker threads, exactly as
`CleanupRunner: Sync` (`cleanup.rs:999`) already allows for the parallel proof
refresh (`cleanup.rs:3284`). All six production `&dyn CargoRunner` sites
(`:61`, `:461`, `:547`, `:674`, `:740`, `:1062`) share through the bound; the
pre-existing `WorkspaceCleanupAdapter` (`cleanup.rs:2790-2804`) stays valid
because the trait it adapts from is already `Sync`.
`WorkspaceCleanupAdapter` (`cleanup.rs:2790-2804`) implements `CargoRunner` over
a `&dyn CleanupRunner` so `cleanup.rs` can reuse this module's resolver without
inheriting its unbounded trait. Identical method signatures, matching bounds.

Because `run` takes `&self`, stateful fakes need interior mutability: this file's
own doubles use `AtomicU64` (`CountingRunner`, `:2879`) and `Mutex`
(`RecordingRunner`, `:5848`). Every test double is field-read-only or
synchronized, so all of them satisfy the bound with no changes.

### What `SystemCargoRunner` actually runs

`:80-93`. It builds argv by handing the caller's `&[OsString]` straight to
`std::process::Command::new("cargo")`, sets `.current_dir(cwd)`, and calls
`.output()` — which captures both streams as `Vec<u8>` and blocks. A
non-zero exit is **not** an error: it becomes `ProcessOutput { success: false,
code: Some(n), .. }` and the caller decides. An `io::Result::Err` (cargo absent
from `PATH`, permission denied on the binary) is the only transport-level failure.

Three subcommands, all built by the module:

| Subcommand | argv | Built at |
|---|---|---|
| locate | `locate-project --workspace --manifest-path <manifest>` | `:548-556` |
| metadata | `metadata --offline --locked --no-deps --format-version 1 --manifest-path <root_manifest>` | `:675-687` |
| locate-in-place | `locate-project --workspace` — **no** `--manifest-path` | `:618-621` |

The third shape is ADR 009's, and the missing option is the point. The
child's working directory is the operator's directory and Cargo walks up from
there to the nearest manifest, so the answer is "the workspace that owns the
place I am standing". `--manifest-path` would name a manifest to report on
rather than a place to search from, so it would answer for a manifest instead
of for the operator's position (`workspace.rs:607-610`).

`cwd` is the manifest's own parent in the first two cases (`:559-562`,
`:689-692`), and the operator's directory in the third.
Because `--manifest-path` is resolved by the *child*, and the child's cwd is the
manifest's parent, a relative manifest would be composed against the wrong
directory — the `fixture/Cargo.toml` + cwd `fixture/` defect documented at
`workspace.rs:33-45` and fixed by `manifest_path_for_cargo` (`:46-54`), which
lexically absolutizes against the process cwd without resolving symlinks.
`cleanup.rs` adds `--profile` / `--package` and a frozen environment via
`run_with_env` on its own trait; `cargo clean` never goes through this module.

### Why this trait costs 4141 lines of tests

Every external process crosses this one method, so a fake can substitute for all
of Cargo. The sixteen doubles in this file — `FailingCargo` (`:2469`),
`RelativeMemberRunner` (`:2700`), `FakeCargo` (`:2761`, impl `:2771`),
`CountingRunner` (`:2879`), `MemberFirstFails` (`:3143`),
`CountingFailRunner` (`:3499`), `SleepRunner` (`:3568`), `OrderRunner` (`:3644`),
`TwoWsRunner` (`:3729`), `FailFirstRunner` (`:3798`), `BelievesRunner` (`:3903`),
`BelievesOrphanRunner` (`:4004`), `RefusedRunner` (`:4236`),
`MetadataFailRunner` (`:4281`), `ForwardFixture` (`:5804`),
`RecordingRunner` (`:5848`, impl `:5852`) — plus a large number of hand-built
`ResolvedWorkspace` values exercise resolution, grouping, and units with no
subprocess at all. That is the seam paying for itself, and it is the same seam
[the overview](overview.md) names as cross-cutting invariant 5.

**The honest cost.** A trait that broad is a contract with a large surface, and
`FakeCargo` is a *model* of Cargo, not Cargo. It answers `locate-project` with a
fixed root for every input, so it cannot represent a member and a root resolving
differently; it cannot represent a real `cargo metadata` failure mode, cargo's
config merge, or a `build_directory` that only appears under a nightly flag. A
green test against `FakeCargo` proves the module's logic over the premises
`FakeCargo` encodes, and nothing else — the project's own standing lesson, from
`plans/registry.md`, is that *a green test proves only that its own premises
hold*.

That risk is not hypothetical in this file, and the module has already been
corrected for it. `a_relative_root_resolves_the_same_workspaces_as_its_absolute_spelling`
(`:5915`) uses `SystemCargoRunner` and the *real* Cargo, with the reason stated
in its own doc comment (`:5908-5913`): a stub that ignores the manifest path
would not have detected the C015 defect. The test module's most recent
deliberate design choice is therefore to add a *real*-Cargo case and a premise
assertion (`RecordingRunner` asserts `!seen.is_empty()` at `:5889-5892` with the
message "the locate path was never exercised, so nothing was proven") rather
than to trust the fake.

---

## 4. Metadata resolution

`resolve_workspaces_with_coverage` (`:738-1057`) resolves in bulk phases, not
in one sequential loop: locate runs bounded-parallel over all manifests
(`:755-757`), manifests group by located root first-seen-first (`:762-788`),
one metadata runs bounded-parallel per group (`:793-804`), and a sequential
emulation replays manifest order over the precomputed results (`:825-996`).
Cargo has no batch metadata mode, so batching still means in-process
deduplication — but the subprocess wall time, not the spawn sequence, is now
parallel. There is exactly one deliberate semantic delta from the old
sequential loop, and it is spawn accounting only (see below); coverage sets,
diagnostic order and content, workspace order, and the unresolved ledger are
identical by construction.

### The enclosing workspace of the operator's directory (ADR 009)

`enclosing_workspace_root` (`:617-635`) and `working_directory_scope_roots`
(`:655-664`) answer a question the bulk resolver never asked: *which
workspace owns the directory the operator is standing in?* It is not the bulk
resolver's job — that one resolves a manifest it was given — so it is a
separate, single-purpose call.

The enclosing workspace is **not optional**. A member directory's manifest
sits *above* the member, so a root at the member cannot discover it, and an
operator standing in `workspace/crates/member/src` expects that workspace's
output to be reclaimed exactly as `cargo clean` would. The anchor already
admits that workspace in both directions (`CleanupSelection::Ancestry`); this
is the discovery half of the same rule.

Every failure mode — a transport error, a Cargo refusal, an unparseable
answer — yields "no enclosing workspace", and the working directory remains a
scope root on its own. The worst case is therefore that the enclosing
workspace is not cleaned, never that something is cleaned without proof.

### The four phases

1. **Locate, in parallel (`:755-757`, worker `locate_one` at `:547-664`).**
   Every manifest gets exactly one `locate-project` spawn, in input-order
   chunks of [`resolution_concurrency`] (`:476-480`, capped at
   `RESOLUTION_CONCURRENCY_CAP = 4`, `:474`), joining in order. The worker
   only builds argv, spawns, and classifies into `LocateOutcome`
   (`:512-533`): transport failure, Cargo-reported failure, unparseable
   stdout, located root, or lost worker slot. A lost slot degrades to a
   deterministic spawn-failure diagnostic — never a panic or a lost manifest.
2. **Group, sequential (`:762-788`).** Outcomes classify in input order;
   manifests locating to the same canonical root form a `ResolutionGroup`
   (`:725-727`) in first-seen order. Group order is the order the old loop
   would have discovered workspaces in.
3. **Metadata, in parallel (`:793-804`, worker `metadata_one` at
   `:674-705`).** One spawn per group — the same spawn set the old loop
   issued, since every group has a first manifest that would have triggered
   metadata. Parsing and member validation stay sequential.
4. **Emulation, sequential (`:825-996`).** The manifest loop replays
   per-manifest decisions over the precomputed results:
   - **Fast path (`:839-845`).** Canonicalize the manifest; if it is already
     in `member_to_root` *and* that root is in `cache`, take it as covered:
     `deduped_workspace_hits += 1`, add to `authoritative_coverage`,
     `continue`. The locate for this manifest already ran in phase 1, so the
     hit saves metadata but not the locate.
   - **Locate outcomes (`:847-905`).** Transport (`could not start`),
     Cargo-reported failure, unparseable stdout, and a lost worker slot each
     push the same `Warning` / `CandidateUncertain` diagnostic at the same
     position as the old loop, with identical text.
   - **Workspace cache (`:906-921`).** Located root already known: count a
     dedup hit, register the manifest, stop. **No metadata process.**
   - **Metadata trigger (`:922-995`).** First manifest per group: account one
     metadata call plus the recorded wall time, then run the shared
     post-spawn processor (`complete_workspace_from_metadata`, `:1093`)
     with this manifest as the diagnostic path. On success the workspace is
     registered and members seeded exactly as before (`:973-986`).
   - **Repeat triggers of a failed root** reuse the recorded outcome via
     `failed_templates` (`:938-956`): one metadata call, the recorded wall
     time, one failure (when the outcome counts as one), and a per-manifest
     diagnostic cloned from the path-independent template. The old loop re-spawned here;
     accounting matches a fresh spawn while one pass cannot fork the ledger
     on concurrent mutation. Repeat triggers of a *successful* root never
     reach this code — the caches above divert them first.

### What changed about spawn counts, and what did not

Metadata is still deduplicated by the canonical workspace root: one spawn per
unique root, plus re-accounting (not re-spawning) for repeat triggers of
failed roots. The `cargo_metadata_calls` counter therefore matches sequential
evaluation exactly, as do `cargo_failures`, `unique_workspaces,
deduped_workspace_hits`, workspace order, diagnostic order and content, and
the unresolved set.

Locate is not: an N-member workspace now costs N `locate-project` spawns plus
one `cargo metadata`, in either discovery order, where the old loop cost one
plus one once the first metadata had seeded the member cache. The old
fast path skipped locates, and skipping locates serializes the whole phase on
member order — there is no order-independent way to know two manifests share
a root without asking Cargo, and ancestry inference is forbidden. The two
tests that pinned the old count were updated with this justification
(`n_member_workspace_requires_one_metadata_regardless_of_order`,
`:3437`; `repeated_members_invoke_metadata_once`, `:2871`), and
`shared_failing_root_reaccounts_metadata_per_manifest_from_one_spawn`
(`:3489`) pins the clone rule from the other side: two manifests, one actual
metadata spawn, two accounted calls with per-manifest diagnostics.
`resolution_overlaps_locate_spawns` (`:3567`) proves spawns actually overlap
(peak concurrency ≥ 2 on multi-core hosts), and
`mixed_locate_and_metadata_failures_keep_manifest_order` (`:3635`) pins the
interleaved diagnostic order.

### Does a member manifest short-circuit a nested metadata call?

Yes — through the group structure rather than through discovery order. All of
a workspace's manifests locate (possibly to the same root), exactly one group
forms, and one metadata runs against the **root** manifest. If a member's
locate *fails*, nothing is seeded for it and the later root manifest's success
still repairs the coverage for the members it lists:
`failed_member_first_is_cleared_by_later_authoritative_workspace_metadata`
(`:3134`) asserts `coverage.unresolved.is_empty()` **and**
`cargo_failures == 1` — the failed attempt's diagnostic is deliberately *not*
retracted.

A member can cause a second, independent resolution only when `locate-project`
reports a genuinely different root, in which case two workspaces and two metadata
calls are correct. When membership *conflicts*, first-writer-wins in group
first-seen order applies and the conflict is neither detected nor reported;
combined with the fast path, a manifest mapped to the wrong root is never
re-located in that pass. No test covers a genuine conflict.

### Failure handling

One bad manifest never aborts anything. Every failure path is `continue`, so the
loop keeps going and the scan degrades to a partial result. Diagnostics are
`Warning` + `CandidateUncertain` and accumulate in the caller's vector; the
counters split the concepts — `cargo_failures` counts subprocess problems,
`unresolved_ownership` counts uncovered manifests (`domain.rs:320-322`).

Metadata-parse failures mirror the locate ones (`complete_workspace_from_metadata`
(`:1093-1273`) handles transport at `:1103-1120`, Cargo-reported failure at
`:1122-1141`, and parse failure at `:1147-1166`). Two further `None` cases inside
the same function: a relative
`workspace_root` from Cargo (`:1175-1192`) and a member whose physical source root
cannot be established (`:1195-1219`) — the latter drops the *whole workspace*, not
just the member, because a member without a source root would make the
`PrivateBounded` source-disjointness test meaningless. Only the first three
kinds count as Cargo failures; the last two record diagnostics without one —
and the repeat-trigger template (`MetadataFailure`, `:717-720`) carries that distinction so
replays account identically.

For non-zero Cargo exits, `cargo_failure_reason` (`:199-228`) keeps up to three
non-empty stderr lines in a 480-byte UTF-8-safe excerpt. Line breaks are
collapsed, terminal controls are removed, and truncation is marked. This keeps
the useful `Caused by:` context Cargo often places after its generic first line
without embedding unbounded stderr in diagnostics. Failure presentation never
changes the exact unresolved-manifest set.

### `ResolutionCoverage` and `UnresolvedOwnershipParticipant`

`ResolutionCoverage` (`domain.rs:203-207`) = `workspaces` + `unresolved` +
`discovered_manifest_count`. `UnresolvedOwnershipParticipant`
(`domain.rs:194-199`) = `{ manifest, stage: &'static str, reason: String,
capability: UnknownCapability }`, where `stage` is one of `"locate"`,
`"metadata"`, `"identity"`, and `capability` is `RefusedByCargo` or `Unproven`
(`domain.rs:181-190`).

Populated at `:1001-1042`: every discovered manifest not in `authoritative_coverage`
becomes one entry. The `reason` is found by scanning `diagnostics` **backwards**
for the last one whose `path` equals the manifest (`:1008-1015`), falling back to
"no successful Cargo workspace metadata covered this manifest". Sorted by path,
then deduped on the canonical key (`:1031-1042`).

**Stage and capability are recorded where the diagnostic is pushed, not parsed
back out of the message.** Each failure branch inserts its own
`(stage, capability)` pair into `unresolved_stage` as it pushes the diagnostic:
transport, unparseable output, and a lost worker record `("locate", Unproven)`
(`:857`, `:884`, `:897`); a Cargo-reported locate failure records
`("locate", RefusedByCargo)` (`:869-872`); the unreachable "no resolution group"
branch records `("identity", Unproven)` (`:935`); metadata failures record
`("metadata", Unproven)` (`:954`, `:994`). The ledger reads that map back with
`("identity", Unproven)` as the fallback (`:1016-1028`). The rationale is in the
comment at `:1016-1018`: a copy-edit to a diagnostic must not silently
reclassify the stage or the merge behavior. Pinned by
`unresolved_stage_is_recorded_not_parsed_from_message` (`:2696`), whose premise
is a diagnostic containing neither "locate-project" nor "metadata" — a message
substring match would have labelled it "identity".

**This is the safety hinge.** Per ADR 001's C004 clarification
(`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:217-229`):
because cargo-cleanme delegates configuration
semantics to Cargo, it cannot prove that an uncovered manifest does not overlap
another workspace's output. So CargoPreview, Simulate and Execute **fail closed
for the proof boundary** while an in-boundary participant remains — no `cargo
clean`, not even `--dry-run`.

> **The boundary is the selection, not the discovered universe.** ADR 007 §1–§4
> and M017 replaced the earlier whole-scope block: an unresolved manifest
> **outside** the boundary is reported as an `out_of_scope` diagnostic and
> **never blocks**, while an **in-boundary** unknown owner still blocks the
> boundary. `src/cleanup.rs:2619-2642` implements the three-way split, and
> `out_of_anchor_poison_does_not_block_anchor` (`src/cleanup.rs:5728`) fails by
> construction against the old whole-scope rule. This passage is the one place
> in the deep dives that still read as the superseded rule; it is corrected
> here rather than left to rot.

`resolve_workspaces` (`:459-467`) discards the `unresolved`
vector, which is exactly why the doc comment at `:729-732` says its
partial-result behaviour is "intentionally unchanged" and the read-only scan
stays tolerant.

**Failure-class attribution is report-only (M015); there is no enforcement
pass-through.** A manifest Cargo refuses with `current package believes it's
in a workspace when it's not` stays an unresolved participant — and under ADR
007 it is an **out-of-boundary** participant that is reported, never blocking;
an in-boundary one blocks the boundary. Withholding it from `unresolved` would
narrow the block without ADR cover, which is exactly what
M015's non-goals forbid (`src/workspace.rs:1043-1051`). What M015 adds is a
pure decision function over the failure classes,
`attribute_unknown` (`src/workspace.rs:419-438`), returning
`UnknownDisposition` (`src/workspace.rs:399-409`): only `RefusedByCargo`
unknowns (Cargo ran and refused the manifest, so it owns no bytes) avoid the
merge — as a passenger of the island Cargo named
(`believed_workspace_manifest`, `src/workspace.rs:372-387`) when that
workspace resolved here, else a scope-level note. Every other class
(transport, unparseable output, lost workers, metadata, identity) is
`MergeAll`, today's block; the capability is recorded where the failure is
classified, never re-parsed from message text. The one narrow parse of a Cargo
*message* lives in `believed_workspace_manifest` (`:372-387`) — the last one in
the module — and it **fails closed**: when the parse returns `None` (wrong
class, or a truncated reason) the disposition is `ScopeNote`, never a passenger
of an unproven island (`:413-418`, `:426-436`).
Permission, transport, parse,
and metadata failures are untouched. Pinned by
`misconfigured_member_with_resolved_parent_still_blocks_but_attributes_as_passenger`
and `misconfigured_member_with_unresolved_parent_still_blocks`
(`src/workspace.rs:3895-4001`), the per-class table
`attribute_unknown_merges_every_unproven_class` (`src/workspace.rs:4077`),
`attribute_unknown_reports_refused_manifest_as_scope_note_without_naming_evidence`
(`src/workspace.rs:4128`),
`attribute_unknown_assigns_refused_believed_manifest_to_resolved_island`
(`src/workspace.rs:4151`),
`attribute_unknown_with_unresolved_named_parent_is_scope_note_not_passenger`
(`src/workspace.rs:4200`),
`attribute_unknown_truncated_believed_reason_is_scope_note`
(`src/workspace.rs:4216`), and the resolution-level capability pins
`resolution_records_refused_capability_for_cargo_refused_locate`
(`src/workspace.rs:4235`) and
`resolution_records_unproven_capability_for_metadata_failure`
(`src/workspace.rs:4280`). Enforcement of the passenger shape arrived with
M016 under the accepted ADR 004 amendment and became the default under
M017/ADR 007: a refused passenger rides along while staying named in
`unresolved_ownership` (never a unit row); see [Cleanup](09-cleanup.md)
§ "Scoped proof is the default (M017)". The three-way split that consumes
`attribute_unknown` lives in `cleanup.rs:2619-2643` — blocking (`MergeAll`),
reported in-boundary (`PassengerOf` / `ScopeNote`), and out-of-boundary are
three separate lists, and only the first blocks. Resolution itself is
unchanged — still report-only.

### Progress

Resolution reports through the observer only, and only sparsely:
`observer.workspaces_resolved(1)` per unique workspace (`:971`) and
`observer.cargo_failure()` for every failed Cargo command. The TTY renderer
keeps this failure total separate from filesystem discovery diagnostics; it
does not display diagnostic text or paths. There is no `units_total` for the
resolution phase — only analysis announces a determinate total.

---

## 5. Capability probing

Two independent probes answer two different questions.

### `capability_from_metadata` (`:124-142`) — "what did this Cargo tell me?"

`CargoBuildDirCapability` (`domain.rs:116-125`) is a four-state answer about
whether a *separate* build directory exists:

| `had_build_field` | `build` | `env_set` | Result | Reading |
|---|---|---|---|---|
| false | any | false | `Unavailable` | Pre-1.91 stable Cargo: `build_directory` did not exist, and no env evidence contradicts it. |
| false | any | true | `Unknown` | Field absent **but** `CARGO_BUILD_BUILD_DIR` is set in our environment. Absence is not proof. |
| true | `None` | any | `Unknown` | Field present but empty/null — malformed. |
| true | `Some(b)`, `b == target` | any | `Equal` | Modern Cargo; build and target are the same directory. |
| true | `Some(_)` | any | `Distinct` | Modern Cargo; two real directories. |

The two raw booleans travel alongside in `CargoCapabilities`
(`:137-141`, `domain.rs:128-132`) so downstream code never has to re-derive them.

`had_build_field` is detected by a **JSON field pattern** over the whole stdout
(`:1144-1146`, `stdout_str.contains("\"build_directory\":")`) rather than by
serde. It cannot be a serde field, because serde's `Option<String>`
(`RawMetadata::build_directory`, `:107-108`) collapses "field absent" and
"field null" into the same value, and the two cases need different answers.
The pattern is deliberately *not* a bare `build_directory` substring: a package
name or path containing that text must not flip `Unavailable` to `Unknown`
(the reason is recorded at `:1144-1145`, defect L-5).

The `Unknown` arm exists because of ADR 001 §3
(`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:54-64`): *"Absence of `build_directory` is never blindly
treated as proof that a configured nightly build directory does not exist."*
The env probe (`cargo_env_build_dir_set`, `:120-122`) is the only evidence that
can contradict absence, and it reads **our** process environment, not the child
cargo's. If the two differ the answer is wrong in the `Unknown` direction, which
is the safe direction. Four tests cover the table: `:2845`, `:2851`, `:2859`,
`:2865`.

### `clean_capabilities_from_version` (`:146-165`) — "may I pass selectors to `cargo clean`?"

```rust
let profile_selector = matches!(version,
    "1.89.0" | "1.90.0" | "1.91.1" | "1.92.0" | "1.93.1"
  | "1.94.1" | "1.95.0" | "1.98.1" | "1.99.0");
let package_selector = matches!(version, "1.98.1" | "1.99.0");
```

**Why version sniffing exists at all.** This is feature detection for Cargo
*behaviour*, not a floor check. The comment at `:144-145` is explicit:
"Selector capabilities are qualified against real Cargo behavior, not the
cargo-cleanme crate MSRV. Unknown/future versions are deliberately false." The
crate's own MSRV is 1.89 (`Cargo.toml:5`), yet `1.88.0` → false and `2.0.0` →
false: the gates track what was empirically observed from a real `cargo clean`,
per M008D's closure record
(`plans/closure/artifact-discovery-cleanup/008d-status.md:26-27`: configured
`build.target` and explicit `--target` package dry-run outputs agree for 1.98.1
and 1.99.0, and the hosted Linux/macOS/Windows and Rust 1.89 gates passed).

**The qualification lifecycle (M011D).** This table used to be a bare allowlist
whose only evidence was a characterization script that defaulted to
`1.89 1.91 1.92 stable` and was wired into nothing. Nothing connected it to the
hosted matrix, so a version could be added, removed, or left behind with no
signal at all — and because the table fails *closed*, no signal was needed for
correctness. That is precisely why the drift was invisible: correctness and
verifiability are different properties, and only the second one was missing.

`release/selector-qualification.json` is now the single machine-readable
authority, holding three deliberately separate lists: `profile_selector`,
`package_selector`, and `exploratory`. Four artifacts are proved to agree by
`scripts/check-selector-qualification.py`:

| Artifact | Role |
| --- | --- |
| `release/selector-qualification.json` | the authority; profile, package, and exploratory held apart |
| `clean_capabilities_from_version` | what the product actually does, and therefore what gets proved |
| `.github/workflows/qualify-cargo-selectors.yml` | the hosted matrix, naming toolchains explicitly so a dropped version is detectable |
| `scripts/qualify-cargo-selectors.sh` | the real-Cargo assertions, consuming the policy rather than carrying a second copy |

Two properties are worth stating because both are the non-obvious direction. The
matrix names its toolchains **literally** rather than reading the policy at run
time — reading it would make a missing version impossible to detect, which is the
defect the gate exists to catch. And `exploratory` is structurally incapable of
becoming a claim: a leaked entry is rejected by both the checker and
`exploratory_toolchains_are_never_promoted_by_accident` (`:2436`), and the hosted
exploratory job is `continue-on-error` so a stable-Cargo regression is reported
without reading as a product regression.

`read_cargo_record`'s sibling here is the same idea in the updater: a support
claim that cannot fail its own check is decoration.

The asymmetry between the two selectors is the substantive part.
`package_selector` is gated to exactly the two releases where the
*configured* `build.target` discrepancy was resolved — the comment at `:161-162`
bounds it to "exact tested releases where configured and explicit target
dry-runs agree". `profile_selector` covers a wider, older set.

Consequences, all verified in `cleanup.rs:4582-4597`: `1.96.0` → both false;
`1.99.1` → both false; `1.99.0-nightly` → both false; `"unknown"` → both false.
`runtime_clean_capabilities` (`cleanup.rs:3992-4004`) shells `cargo --version`,
takes whitespace token 1, and **returns `CargoCleanCapabilities::default()` —
all false — on any failure whatsoever**. So a user on an old, new, nightly, or
absent Cargo gets the reduced behaviour: the clean still runs, just without
`--profile` / `--package` selectors, i.e. at whole-target scope rather than a
narrower one. Safe and boring, never broken. That is the whole design intent of
this function.

### The build-directory version boundary

`CargoBuildDirCapability` splits at Cargo 1.91 (`domain.rs:117`:
"Pre-1.91 metadata without `build_directory`"; ADR §3 lines 60-62). Note this
1.91 boundary is expressed as a *metadata field* observation, not a version
match — the code never parses a version to decide it, which is why it survives
nightlies and future releases. Only the clean-selector gates use version
strings.

---

## 6. Physical grouping

The central idea: many *logical* output roots (one per workspace, up to two
kinds each) collapse into few *physical* groups, and each physical group is
measured once.

### `outermost` (`:1389-1407`)

Sort, dedup, then sweep: skip a candidate already contained in a kept root;
otherwise evict any kept root the candidate contains; push; final sort. Result
is the minimal antichain under path-prefix containment — a deterministic,
idempotent reduction used in four places: per-group `covering` (`:1555`),
`workspace_member_roots` (`:1660`), and the cleanup unit's deduplicated union
(`:2266`), plus `cleanup.rs:3621`.

### `build_groups` (`:1466-1622`)

1. **Node collection (`:1480-1512`).** Per workspace, for each of `output.target` and
   `output.build`: `is_symlink` → mark the workspace uncertain, no node (`:1494-1498`);
   `!exists` → nothing, and *not* marked (`:1499-1501`), because a missing directory
   has no bytes to share; `physical_path.is_none()` (exists, uncanonicalizable) →
   mark uncertain (`:1507-1510`); otherwise push `Node { ws, physical }`.

2. **Union-find (`:1514-1537`).** O(n²) pairwise scan, union `i,j` when
   `contains(pi, pj) || contains(pj, pi)`. `contains` (`:1277-1279`) is a pure
   `starts_with` component test — equality counts, per the doc at `:1276`.

3. **Components → groups (`:1538-1542`, `:1548-1618`).** `physicals` sorted+deduped
   (`:1549-1551`), `owners` sorted+deduped (`:1552-1554`),
   `covering = outermost(physicals)` (`:1555`),
   `display = covering[0]` (`:1557-1560`).

4. **Classification (`:1561-1608`),** in this exact order:

| Condition | Class |
|---|---|
| `owners.len() > 1` | `Shared` |
| owner is in `uncertain_ws`, **or** a covering root is (or contains) a member `source_root`, **or** a covering root is nested inside a package directory, **or** a covering root is named `src`/`tests`/`benches`/`examples` | `Uncertain` |
| single owner, every covering root `strictly_below(ws.root)`, and the group overlaps **none** of the unrepresentable roots' known aliases | `PrivateBounded` |
| single owner, inside the root, but the group **does** overlap an unrepresentable root's known aliases | `Shared` |
| single owner, some covering root outside the workspace root | `ExternalUnproven` |

5. **Sort by `display` (`:1620`).**

**The load-bearing detail: exclusivity is decided per group (M-1).** A root that
contributes no node — a symlink, or one with no physical identity — is invisible
to the union-find, yet it can still point into a group. So the graph is
incomplete, and the code refuses to let a group that *could* contain those bytes
claim `PrivateBounded`.

The mechanism is two small functions plus one comparison:

- `uncertain_root_aliases` (`:1418-1438`) enumerates the *known aliases* of a
  root with no provable identity: always the declared logical spelling, plus
  the symlink's resolved target when the link can be read and the target
  canonicalized. A link *into* a group's covering must block that group, and a
  purely logical check would miss it.
- `group_overlaps_uncertain` (`:1444-1448`) returns true when any known alias
  contains, or sits inside, a covering root — either direction means
  `cargo clean` on the covering could destroy the other workspace's bytes.
- The classification consumes it at `:1598`:
  `inside_root && !group_overlaps_uncertain(&covering, &uncertain_aliases)`.

**This used to be a global flag.** An earlier shape computed
`exclusivity_unproven = !uncertain_ws.is_empty()` and let one unrepresentable
root anywhere demote *every* disjoint group to `Shared`. M-1 made the check
per group: a group disjoint from every known alias of every unrepresentable
root keeps its claim, and only an *overlapping* group loses it. Both shapes
are pinned: `another_workspaces_unrepresentable_root_blocks_a_private_claim`
(`:4831`, unix-only) is the overlapping case, and
`disjoint_groups_stay_private_despite_a_far_away_unrepresentable_root`
(`:4859`) is the disjoint case — the latter fails on the old global flag and
passes on per-group overlap.

Note the residual asymmetry: a single-owner group whose covering root lies
*outside* the workspace root still gets `ExternalUnproven` even when an
unrepresentable root overlaps it, because that arm is guarded by `inside_root`
(`:1593-1606`). That is safe by construction — `ExternalUnproven` is
inventory-only, and `covering_is_authorized_roots`
(`cleanup.rs:1113-1127`) returns `Ok(false)` for anything that is not
`PrivateBounded`.

`strictly_below` (`:1384-1386`) exists because `contains` is inclusive: grouping
*needs* equality, but an artifact must sit strictly inside its workspace, so the
workspace root itself can never be an output root. `is_conventional_source_dir`
(`:1288-1292`) is a name-based backstop for `target-dir = "src"`; its doc comment
states the design rule: it "can only ever *withhold* a cleanup, never authorize
one."

**That backstop has a containment twin, and the name test alone was not
enough.** `is_conventional_source_dir` sees only the covering root's *final
component*, so `target-dir = "src/foo"` — an output root inside the crate's own
source tree — slips past it, and `cargo clean --target-dir <root>` removes the
whole root, taking the crate's own `.rs` files with it. `inside_source_tree`
(`:1316-1337`) is the containment form of the same guard: it asks whether the
covering root lies inside any member's conventional source directory (`src`,
`tests`, `benches`, `examples`) or inside the workspace root's. It fails the
same way — it can only ever *withhold* a cleanup.

**A package directory *inside* the output root is the mirror shape, and the
discovery cannot see it.** The discovery does not descend into an output root,
so a package checked in *under* the root (`target/sub/Cargo.toml`) never enters
the resolved universe — and the classification, which works from that universe,
has nothing to exclude it. The filesystem is where the shape is caught:
`manifest_package_dir` (`traverse.rs:107-116`) reports the first package
directory the traversal finds under the root, `analyze_groups_detailed`
(`:1961-2000`) turns that into an `Uncertain` group with a diagnostic instead
of a measured candidate, and the pre-spawn proof refuses the unit again
(`cleanup.rs:3934-3948`).

**One manifest rule for the whole tool.** `is_package_directory`
(`:1303-1311`) used to test two exact file names, while the discovery tested
the platform's own rule, so the two modules could disagree about what a
package directory is. Both now use `domain::is_manifest_shaped_name`
(`domain.rs:474-486`): the discovery asks *would Cargo open this file?* and
follows the platform's resolution rules; the ownership proof asks *would a
cleanup lose this file?* and is deliberately broader, because it may only ever
withhold a cleanup.

### `has_artifact_entries` (`:1646-1649`)

`fs::read_dir(path).ok()?` then `entries.next().is_some()`. One syscall pair. It
exists because `analyze_groups_detailed` must reject empty and missing output
*before* any source walk or deep sizing (`:1727-1754`) — sizing a tree is the
expensive step, and a `read_dir` returning no entries is a complete negative
answer for the price of one directory read. The trade-off is in the signature:
`Option<bool>` cannot distinguish "empty" from "unreadable", and the caller folds
the `None` case into `missing_all` (`:1741`), so an **unreadable** covering root is
reported as `MissingOutput` rather than as a diagnostic. That is a real, if
narrow, regression against `output_root`'s own care ("Unreadable is not absent",
`:306-333`); the window is small, since `read_dir` needs only read permission on
the directory itself, but it is the one place where fail-closed reporting is
weaker than the rest of the module.

### `workspace_member_roots` (`:1656-1661`)

Member `source_root`s, sorted, deduped, `outermost`-reduced. Two reasons members
participate, not just the workspace root:

1. **Source-activity coverage.** ADR §9 (`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:117-123`): "A
   workspace is protected by recent source activity in any workspace member."
   `workspace_source_activity` (`:2293-2309`) walks every member root and returns
   `Ok(true)` on the first `Recent`, `Err(())` on the first uncertainty.
   `out_of_tree_member_is_included_and_protects` (`:5658`) is the test — a member
   outside the workspace directory still protects.
2. **Source/output disjointness.** `build_groups:1424-1429` tests every member's
   `source_root` against every covering root. A target dir that contains a
   member's sources is a misconfiguration that would delete the project.

The `outermost` reduction matters for a nested layout: a workspace whose members
are `ws/` and `ws/inner/` must not have `ws/inner/` walked twice, and `inner`'s
own entries are already covered by the `ws/` walk.

---

## 7. Group analysis

### `GroupAnalysisInput` is not an input

`analyze_groups_detailed` (`:1710-1719`) takes `workspaces: &[ResolvedWorkspace]`
and `groups: Vec<RawGroup>` as separate parameters. `GroupAnalysisInput` (`:1651`)
is never constructed and never referenced in `src/` or `tests/`. The real inputs
are the two slice parameters plus `clock_start`, `clock_cutoff`, `recency`,
`counters`, `diagnostics`, `observer` — six more, which is why the function
carries `#[allow(clippy::too_many_arguments)]` (`:1709`) and threads cross-phase
state explicitly.

`analyze_groups` (`:2070`) is the delegating wrapper: same eight parameters,
calls the detailed form, keeps only `measured`, and sorts by
`(bytes desc, display_path)`.

### Gate order

Documented at `:1703-1704` as existence/type → artifact presence → source
activity → deep sizing. The implementation, in order:

**Stage 0** — announce the total (`:1723`):
`observer.units_total(ScanPhase::Analysis, groups.len())`.

**Stage 1 — cheap presence (`:1727-1754`).** Probe every `covering` root per group,
building `group_has_entries` and `group_missing_all`. All-empty groups bump
`empty_no_output_skipped` + `observer.empty_skipped()`; all-missing groups bump
`missing_output_skipped`. Nothing expensive runs for either.

**Stage 2 — source activity, once per workspace (`:1758-1823`).** Two fail-fast
skips before any walk: a workspace with no existing output (`:1781-1788`), and one
whose every group is already empty (`:1789-1794`). Otherwise
`workspace_source_activity(ws, &all_outputs, clock_start, clock_cutoff)`:
`Ok(true)` → `active_skipped += 1` + `observer.active_skipped()`; `Err(())` →
mark `ws_source_uncertain` and push an `Error` / `CandidateUncertain`
diagnostic (`:1812-1821`) — note the severity, unlike almost everything else in
the module. `all_outputs` (`:1760-1770`) is every workspace's `physical_path` (or
`logical_path` when there is none), so a source walk never counts the project's
own artifacts as source activity.

**Stage 3 — pre-skip survivors (`:1829-1886`),** first match wins:

| Order | Condition | `GroupSkipReason` | Counter |
|---|---|---|---|
| 1 | no covering root had an entry, and all were missing/unreadable | `MissingOutput` | `missing_output_skipped` (stage 1) |
| 2 | no covering root had an entry, at least one was readable-and-empty | `EmptyOutput` | `empty_no_output_skipped` (stage 1) |
| 3 | any owner's source is recent | `ActiveSource` | `active_skipped` (per group, stage 3) |
| 4 | any owner's source is uncertain | `UncertainSource` | `uncertain_skipped += 1` |
| 5 | `ownership == Uncertain` | `UncertainOwnership` | `uncertain_skipped += 1` |

Arm 5 additionally emits the source-overlap diagnostic when `g.source_overlap`
(`:1858-1879`): *"output root … contains a workspace source tree, not a build
artifact; it is not sized and not eligible for cleanup"*, or, when the root is
nested inside a package directory rather than around one, *"output root … lies
inside a package directory, not a build artifact; it is not sized and not
eligible for cleanup"*. The comment above it gives the reason — the user
configured `target-dir` that way and must be told, rather than being met with
silence.

The overlap test walks **every** resolved workspace's members, not just the
group's own owner. `cargo clean` removes whatever is under the covering root, so
a *second* resolved project whose sources live inside it is destroyed just as
surely as the owner's own; consulting only the owner's `members` ratified exactly
the shape the check exists to refuse — an outer workspace whose output directory
was not literally named `target`, containing a real nested project, lost four
files with exit 0 and no diagnostic. The preflight proof closes the same gap
independently at `cleanup.rs:3653-3673`.

**Stage 4 — deep sizing (`:1888-1905`).** One bounded pool, one call:
`traverse::measure_many_targets` over `(survivor_position, covering_path)` pairs.
Wall time → `output_sizing_nanos`. Results are re-indexed into a
`HashMap<usize, TargetStats>` (`:1902-1905`), so parallel completion order is
irrelevant.

**Stage 5 — aggregate per group (`:1913-2062`).** Sum `bytes` and `entries` with
`saturating_add`, take the max `newest` mtime, and — critically — **break and
mark uncertain if any contributing root is uncertain or missing from the result
map** (`:1930-1944`). A partial measurement is never promoted. Then, in order:

| Condition | Outcome |
|---|---|
| any contributor uncertain | `UncertainMeasurement` + `Error` diagnostic (`:1945-1998`) |
| `entries == 0` | `EmptyOutput` (`:1999-2009`) |
| `newest.is_some_and(\|t\| t >= clock_cutoff \|\| t > clock_start)` | `ActiveOutput` (`:2010-2022`) |
| otherwise | **survivor** → `PhysicalOutputGroup` (`:1964-1975`) |

Every branch, including the early-skipped ones at `:1914-1923`, calls
`observer.unit_completed(ScanPhase::Analysis)` exactly once, which is what makes
the analysis progress bar determinate. The survivor branch also fires
`observer.group_measured(bytes)` (`:2026`) and
`observer.reportable_group(&g.display, bytes)` (`:2055`).

### Clock handling

`clock_start` and `clock_cutoff` come from the caller; `main.rs:1639-1649` computes
`clock_cutoff = scan_start - recency` and turns an overflow into
`AppError::Config`.

**The `recency: Duration` parameter is accepted and deliberately ignored**
(`:2039-2042`):

```rust
// `recency` is deliberately not consulted here: the inactivity window is
// applied by the caller through `clock_cutoff`. Accepting a duration and
// ignoring it invited the belief that the policy lived here.
let _ = recency;
```

`analyze_groups` forwards it unchanged (`:2085`). A deliberate API wart, and the
comment explains why it was kept.

The output-recency predicate is `t >= clock_cutoff || t > clock_start` (`:2011`).
The second disjunct protects against future-dated timestamps — a skewed clock or
a restored backup must not be silently treated as ancient. The identical
predicate lives in `traverse.rs:362`, so source and output agree on what
"recent" means. ADR §9 (`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:121-123`): "A physical output group
is protected by recent activity anywhere in that group. When a shared physical
group is recent, all owners are protected" — which is what arm 3 and this stage
jointly implement for multi-owner groups.

### `PhysicalOutputGroup` vs `EligibleOutputGroup`

**This module produces only `PhysicalOutputGroup`** (`:1964-1975`), with
`covering_roots`, `owners: Vec<WorkspaceId>`, and `uncertain`. It never
constructs `EligibleOutputGroup`; that flattening happens in `main.rs:1664-1677`,
which drops `covering_roots` and `uncertain` and maps `owners` to
`workspace_roots`. The distinction is the scan's report-facing projection versus
the internal working type.

`SizeMetric` is chosen at compile time, not runtime (`:1950-1959`):
`Allocated` on `unix`/`windows`, `Apparent` everywhere else.

### `GroupSkipReason` — the "why is my project not in the report" table

`detail()` (`:1676-1686`) is the user-facing text.

| Variant | `detail()` | Produced when |
|---|---|---|
| `MissingOutput` | `output directory is missing` | No covering root yielded a dir entry, and every root was missing or unreadable (`:1832-1833`). |
| `EmptyOutput` | `output directory contains no artifacts` | Every covering root was readable and empty (`:1834-1835`), **or** a measured group aggregated to `entries == 0` (`:1999-2009`). |
| `ActiveSource` | `workspace source activity is recent` | Any owner's member source was touched at/after the cutoff (`:1842-1848`). |
| `UncertainSource` | `workspace source activity could not be established` | Any owner's member walk returned `Err(())` (`:1850-1854`). |
| `UncertainOwnership` | `output ownership could not be established` | `g.ownership == Uncertain` — symlinked root, unresolvable identity, or source overlap (`:1855-1880`). |
| `UncertainMeasurement` | `output activity or size could not be established` | A covering root's stats were uncertain or absent from the result map (`:1945-1998`). |
| `ActiveOutput` | `output activity is recent` | Newest mtime in the group is at/after the cutoff, or in the future (`:2010-2022`). |

### `GroupOutcome` and `map_workspace_groups`

`GroupOutcome` (`:1694-1699`) is one entry per input group, in input order, with
exactly one of `measured` / `skip` set. Its reason for existing is in the comment
at `:1689-1693` and ADR C003 (`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:207-212`): a workspace-level
cleanup decision must be able to *name* the sibling physical group that blocks
the whole unit, rather than silently dropping one group from a multi-group
`OutputSet`.

`map_workspace_groups` (`:2126-2128`, with `map_workspace_groups_indexed` at
`:2130-2151`) is the many-to-many inverse. Why
many-to-many:

- **one group → many workspaces.** A `Shared` group's single `cargo clean` from
  workspace A's perspective affects bytes workspace B also claims.
- **many groups → one workspace.** A workspace with a distinct `target/` and
  `build/` yields two groups (`:5324`, `distinct_sibling_target_build_measured_once_per_root`).
- **nested/equal roots collapse.** Both kinds pointing at the same physical path
  resolve to one group index, deduplicated by `!idxs.contains(&gi)` (`:2144`).

It skips roots that are symlinks, missing, or have no `physical_path` (`:2136-2142`)
— precisely the roots `build_groups` never made a node for, so the mapping is
total over the graph. Output is sorted by group display path (`:2149`).
`cleanup.rs:3589` uses it during final revalidation to compute the
`fresh_covering` union that `outermost` then reduces (`cleanup.rs:3621`).

---

## 8. Cleanup units

Unit-of-atomicity is the concept: **one `cargo clean` invocation is the
transaction**, so the unit must describe everything that invocation can touch.
ADR C003 (`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:201-214`): "One Cargo clean invocation is
authorized as a workspace-level CleanupUnit covering the complete resolved
OutputSet that the invocation can affect. Every physical group touched by that
OutputSet must independently satisfy the destructive ownership and authorization
boundary before Cargo clean may run. A PrivateBounded target group cannot
authorize a sibling build group that is ExternalUnproven, Shared, Uncertain, or
unauthorized."

### `AffectedGroup` (`:2154-2164`)

A `RawGroup` re-expressed through *one* workspace's lens: the same
`RawGroup.display` / `covering` / `ownership`, plus `kinds` — the
`OutputRootKind`s of this workspace whose `physical_path` appears in
`o.group.physicals` (`:2165-2172`). Both kinds appear when target and build
collapse into one physical group; the test asserts
`kinds == [Target, Build]` and `kind_label() == "target+build"`.
`kind_label()` (`:2089-2103`) sorts Target before Build, dedups, and falls back to
`"output"` for the empty case.

**The label builder is not compile-checked.** The sort key at `:2170-2173`
*is* exhaustive over `OutputRootKind` — a new variant breaks the build there.
The `match kinds.as_slice()` that produces the label ends in a `_` arm
(`:2101`), so a *third* kind would silently render as `"output"` rather than
failing to compile. This is recorded as an open finding in
[the overview](overview.md) § 10 (finding 18); it is not a safety issue, only a
label-accuracy one.

### `CleanupUnit` (`:2192-2206`)

| Field | Meaning |
|---|---|
| `workspace_idx`, `id`, `root`, `root_manifest` | identity, copied from `ResolvedWorkspace` (`:2198-2202`) |
| `output`, `capability` | the full `OutputSet` and Cargo capability evidence, needed to rebuild the command and the proof |
| `groups: Vec<AffectedGroup>` | every physical group this workspace's output set can touch, each with its own `measured`/`skip` |
| `unmapped: Vec<OutputRoot>` | existing, non-symlink roots with `physical_path == None` (`:2150-2158`) — an unbounded cleanup footprint |
| `covering: Vec<PathBuf>` | `outermost` of the union of all affected `covering` roots (`:2266-2271`) — the deduplicated footprint |
| `bytes: u64` | sum of the affected groups' measured bytes (`:2272-2275`) |

### `fully_measured()` (`:2210-2212`)

```rust
self.unmapped.is_empty() && self.groups.iter().all(|g| g.measured.is_some())
```

**Confirmed: a partially-measured unit cannot be acted on.** The two halves fail
closed for different reasons. A missing `measured` means some physical group the
invocation can affect was skipped for a *reason* — empty, active, uncertain, or
unclassified ownership — and ADR C003 requires *every* touched group to clear the
bar independently, so one bad sibling withholds the whole. A non-empty `unmapped`
means there is existing output whose physical identity could not be proven, so
the deletion footprint cannot even be enumerated.

`unit_with_unmeasured_sibling_is_not_fully_measured` (`:6275`) is the exact
case: a measured 4 KiB `target/` beside an existing-but-never-populated `build/`
directory. The unit exists (so the user sees it and learns why it is blocked) but
`fully_measured()` is false, and the unmeasured sibling carries
`skip == Some(EmptyOutput)` and `kinds == [Build]`.

### `build_cleanup_units` (`:2220-2291`)

Per workspace: map to groups (`:2228`), collect `unmapped` (`:2229-2237`), skip
entirely if both are empty (`:2238-2240`), build `AffectedGroup`s, then **skip
again if no affected group is measured** (`:2261-2265`) — "a workspace with no
reportable affected output is not a cleanup unit at all." Then the deduplicated
union and byte sum, and finally a deterministic sort by
`(bytes desc, root)` (`:2289`).

Note the consequence, verified by `unprovable_output_identity_makes_the_group_uncertain`
(`:6356`): one unprovable output root makes the workspace `Uncertain`
through `build_groups:1355-1358`, every one of its groups is skipped, and
`build_cleanup_units` produces **no unit at all** — so the unprovable root never
even reaches `unmapped`, because `affected` is empty first.

### Ownership classes that can appear in a unit

`AffectedGroup.ownership` is copied from `RawGroup.ownership`
(`domain.rs:255-260`), so a unit can carry any of the four classes across
different groups. The safety property is not that a unit is uniformly
`PrivateBounded`; it is that `cleanup.rs` re-checks each group independently
through `covering_is_authorized_roots` (`cleanup.rs:1113-1127`), which returns
`Ok(false)` for anything that is not `PrivateBounded` — `ExternalUnproven`,
`Shared`, and `Uncertain` are all inventory-only, permanently
(ADR §7, `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:101-107`). And even a `PrivateBounded` group is
still gated by the location check, so user authorization can never manufacture
proof.

---

## 9. Invariants and edge cases

**Is physical grouping a true partition?** **Yes over output-root nodes and
canonical directory trees; no over inodes or mount points.** The union-find is a
connected-components computation over a symmetric relation, so the components
partition `nodes` by construction (`:1538-1542`) and no path is emitted in two
groups. The stronger property holds too: if two *different* components held
overlapping bytes, one path would have to be an ancestor of the other — and
`contains` is checked in both directions (`:1531-1532`) — so they would have been
unioned. Equal and ancestor–descendant roots therefore cannot straddle a group
boundary, which is what makes "counted and acted on exactly once" true, and what
ADR §5 (`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:76-86`) requires. Three ways a byte can still count
twice: **hard links** between two target directories (the walker sums
`file_real_size_fast` per path, `traverse.rs:310`, so each name counts);
**mount points** inside a target tree (`dua_core` descends into them; nothing here
or in `traverse.rs` checks for a mount boundary); and **case-insensitive
aliasing**, defended only by the single `fs::canonicalize` per root at `:380`,
not per entry. I did not empirically verify canonicalization on case-insensitive
macOS volumes; on Windows `canonicalize` returns verbatim paths, which is the
expected mitigation — reasoning, not a measurement in this repo.

**Can two different workspaces share one `target/`?** Yes, and that is a
first-class supported case, not an error. Two independent workspaces with the
same redirected `target-dir` produce two nodes at the same path, the union-find
merges them, and `owners.len() > 1` makes the group `Shared`
(`:1564-1565`) — measurable, reportable, never cleanable
(ADR §7). Asserted by `output_grouping_equal_nested_shared` (`:4377`).
Within one workspace, the same physics gives a `PrivateBounded` group with
`kinds == [Target, Build]`. Note the *deliberate* asymmetry: a missing root is
invisible to the graph (`:1499-1501`) and does not block exclusivity, because no
bytes exist to share.

**Is the report order deterministic and independent of `cargo metadata`
completion order?** **Yes, unconditionally.** Resolution spawns run on worker
threads, but every spawn result joins in input order and every decision —
diagnostics, coverage inserts, workspace registration — replays sequentially
in the emulation pass (`:825-996`), so "completion order" cannot leak into any
output. Downstream, every source of potential
nondeterminism is erased before return:

| Site | Non-determinism | How it is removed |
|---|---|---|
| `:745-750` | `cache` / `locate_cache` / `member_to_root` / `authoritative_coverage` HashMaps | Lookup only; never iterated |
| `:1538-1542` | `groups_map` **is** a `HashMap` **and is iterated** (`:1548`) | Every group's `physicals` (`:1549-1551`), `owners` (`:1552-1554`) and `covering` (`outermost` → sorted, `:1405`) are individually sorted, and the group list is sorted by `display` (`:1620`) |
| `:1001-1042` | `unresolved` | Sorted by manifest (`:1031`, `:1039`), deduped on the canonical key (`:1040-1042`) |
| `:1888-1905` | `traverse::measure_many_targets` is parallel | Results are re-indexed into a `HashMap<usize, _>` keyed by input position (`:1902-1905`); completion order is discarded |
| `:755-757`, `:793-804` | locate/metadata spawns run on worker threads | Outcomes join in input order and every decision replays sequentially (`:825-996`); completion order is discarded |
| `:2093-2098` | `analyze_groups` output | Sorted `(bytes desc, display_path)` |
| `:2149` | `map_workspace_groups` | Sorted by group display |
| `:2289` | `build_cleanup_units` | Sorted `(bytes desc, root)` |

One residual worth stating precisely: `resolve_workspaces` returns workspaces in
**first-seen discovery order** (`:972`, collected at `:997-1000`), not sorted. The doc comment's "in
deterministic order" (`:442-443`) means "deterministic given a fixed input list", and
it holds because `ordered_keys` is a `Vec` appended in iteration order. No
consumer depends on it. The *other* parallel resolution in the
project lives in
`cleanup.rs` (`refresh_proof_universe_with_limit`, `cleanup.rs:3263`); it
threads `batch` with `std::thread::scope` (`cleanup.rs:3284`), zips outcomes back
against the input batch in order, and `cleanup.rs:10343` asserts sequential ==
parallel. So the determinism claim survives the parallel path too.

**`cargo` not on `PATH`?** A **diagnostic, not a hard error.** `Command::new("cargo")
.output()` returns `io::ErrorKind::NotFound`, which becomes `Err` from `run` and
is caught at `:567-572` (locate transport) and `:700-704` (metadata transport),
then surfaced as `Warning` / `CandidateUncertain` diagnostics at `:848-859` and
`:1103-1120`: `cargo_failures += 1`,
`observer.cargo_failure()`, a message saying
"cargo metadata could not start" or "cargo locate-project could not start", then
`continue` / `None`. The scan exits 0 with zero workspaces, and — via
`cleanup.rs`'s use of `resolve_workspaces_with_coverage` — every discovered
manifest lands in `unresolved`, failing the destructive scope closed. Note the
trade-off this creates: "cargo is not installed" and "nothing to clean" are
indistinguishable in a bare scan, which is precisely the C015-class silent-zero
shape the module's own comments at `:33-45` and `:5680-5684` guard against.
(`cleanup.rs`'s `runtime_clean_capabilities` returns all-false on the same
condition, `cleanup.rs:3992-4004`.)

**Warnings on stderr with a successful exit?** Ignored. `ProcessOutput.stderr`
is inspected only after a non-zero locate/metadata exit; successful stdout remains
authoritative and warning text cannot affect resolution. The same process-output
type is shared with `CleanupRunner` (`cleanup.rs:997`).

**Malformed or unexpected metadata JSON?** A warning diagnostic, the workspace is
dropped, resolution continues (`:1147-1166`). Same for unparseable `locate-project`
output (`:592-595`) and for a relative `workspace_root` (`:1175-1192`). The
`UnresolvedOwnershipParticipant.stage` for the first two is `"metadata"` /
`"locate"` — **recorded at the failure site**, not recovered by substring match
on the message (`:857`, `:869-872`, `:884`, `:897`, `:935`, `:954`, `:994`;
read back at `:1016-1028`) — see §4. Because `serde_json` is non-strict about
*unknown* fields, a future Cargo that
adds fields still parses; only a *changed* `target_directory`/`workspace_root`
contract would break it.

**Is a symlinked `target/` followed, and can it escape the scope?** Not followed,
and it cannot escape — this module is a consistent no-op on symlinks.
`output_root` uses `fs::symlink_metadata` (`:289`), detects
`file_type().is_symlink()` — and, because `symlink_metadata("link/")` follows
the final component on POSIX, also re-checks the spelling with its trailing
separators and dot components stripped (`path_is_symlink`, `:234-254`) — and
records `is_symlink: true` with `physical_path: None` **without canonicalizing**
(`:299-305`) — deliberately, because canonicalizing is what would resolve the
link. From there: `build_groups` contributes no node and marks the workspace
uncertain (`:1494-1498`); `map_workspace_groups` skips it (`:2137`);
`build_cleanup_units` skips it for both the mapped *and* the `unmapped` list
(`:2231-2233`); and `traverse` never follows links either
(`traverse.rs:7-8`, `symlink_metadata` guards at `traverse.rs:137`, `136`,
`202`, `244`, plus `Options::default()` with no follow flag at `traverse.rs:83` — I
read this module's usage, not dua-core's default). Consequences worth naming: a
workspace whose *only* output is a symlink produces **no groups and no
diagnostic** — `symlink_output_is_uncertain` (`:4893`) asserts exactly
`groups.is_empty()`. ADR §5 says symlink roots "are inventory diagnostics"
(`plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md:86`); the code makes them *inert*, not *reported*.
The safety property still holds — the symlinked workspace is in `uncertain_ws`,
so its own groups are `Uncertain`, and under M-1 an unrepresentable root blocks
only the groups whose covering overlaps its known aliases (`:1598`), never a
disjoint group — but the user gets no message. **This is a real gap between ADR
wording and implementation**, and it is the one I would file. What the *spawned* `cargo
clean` does with such a root is outside this module; `cleanup.rs`'s final proof
(`cleanup.rs:3381`) is the only thing that could catch a change between scan
and clean, and I did not audit that path in full.

**Can a member's manifest produce a disagreeing nested resolution?** Only by
being in a *different* workspace, which is correct behaviour. Within one
workspace, a member never gets its own metadata call (step 4 above). The
untested case is a *conflicting* mapping: `member_to_root.entry(..).or_insert(..)`
(`:974-986`) is first-writer-wins in group first-seen order and a conflict is neither detected nor
diagnosed. Combined with the fast path at `:839-845`, a manifest mapped to the
wrong root is never re-located within that pass. `TwoWsRunner` (`:3729`) proves
the *legitimate* two-workspace case; no test constructs a genuine conflict.

**Bounded, or O(manifests)?** **Bounded by unique workspaces for spawns that
matter, and asymptotically O(manifests) overall — with bounded parallelism.**

- `cargo metadata`: exactly **one spawn per unique canonical workspace root**
  (`:793-804`), counted at `:957` (trigger) and `:943` (repeat-trigger
  re-accounting, which replays the recorded outcome without re-spawning). So
  ≤ O(unique workspaces) ≤ O(manifests) spawns.
- `cargo locate-project`: **exactly one per discovered manifest** (`:755-757`),
  including manifests the fast path later covers without metadata. Worst case
  is O(manifests): a machine of N unrelated single-crate projects costs N
  locate + N metadata.
- `refresh_workspace_from_root_manifest` (`:59-76`) costs one metadata and zero
  locate per call; `cleanup.rs:3103` calls it per candidate at the destructive
  boundary, tracked in the separate `proof_*` counters so `--stats` cannot hide
  it (`domain.rs:338-345`, `merge_proof` at `domain.rs:428`).

Performance implication: the cost is **N subprocess round-trips at
`RESOLUTION_CONCURRENCY_CAP = 4` lanes** (`:474`, `:476-480`), and
`--locked` (`:678`) means a stale `Cargo.lock` fails metadata for that workspace
outright. The `cargo_locate_nanos` / `cargo_metadata_nanos` split
(`:564`, `:694`) exists so the user can see which half dominates.

---

## 10. Testing

**76 `#[test]` functions** — confirmed by count. The inline module starts at
`workspace.rs:2311` (`#[cfg(test)] mod tests {`).

| Area | Count | Tests (line) |
|---|---|---|
| Selector qualification | 3 | `:2363`, `:2436`, `:2453` |
| Bounded Cargo failure causes | 6 | `:2518`, `:2536`, `:2562`, `:2593`, `:2623`, `:2659` |
| Capability probing | 4 | `:2845`, `:2851`, `:2859`, `:2865` |
| Metadata resolution & coverage | 16 | `:2696`, `:2871`, `:2934`, `:2989`, `:3134`, `:3207`, `:3237`, `:3274`, `:3310`, `:3379`, `:3437`, `:3489`, `:3567`, `:3635`, `:3727`, `:3797` |
| Failure-class attribution (M015) | 10 | `:3864`, `:3895`, `:4002`, `:4077`, `:4128`, `:4151`, `:4200`, `:4216`, `:4235`, `:4280` |
| Grouping & ownership classification | 13 | `:4336`, `:4350`, `:4377`, `:4514`, `:4551`, `:4766`, `:4831`, `:4859`, `:4923`, `:4959`, `:5045`, `:5087`, `:5126` |
| Analysis, gates, measurement, progress | 11 | `:5139`, `:5324`, `:5373`, `:5395`, `:5423`, `:5479`, `:5521`, `:5606`, `:5658`, `:5704`, `:5749` |
| Real-Cargo & premise qualification | 5 | `:5847`, `:5915`, `:6006`, `:6048`, `:6097` |
| Cleanup units & output-root probing | 8 | `:6157`, `:6209`, `:6275`, `:6308`, `:6330`, `:6356`, `:6398`, `:6437` |

Most significant: `n_member_workspace_requires_one_metadata_regardless_of_order`
(`:3437`) — one metadata per workspace in both discovery orders, with one
locate per manifest under bulk resolution;
`another_workspaces_unrepresentable_root_blocks_a_private_claim` (`:4831`) and
`disjoint_groups_stay_private_despite_a_far_away_unrepresentable_root` (`:4859`) —
the two halves of the per-group exclusivity rule;
`failed_member_first_is_cleared_by_later_authoritative_workspace_metadata`
(`:3134`) — coverage completeness under partial failure;
`unit_with_unmeasured_sibling_is_not_fully_measured` (`:6275`) — the atomicity
gate; `the_composed_locate_arguments_resolve_against_the_supplied_working_directory`
(`:5847`) — the C015 regression.

### Test doubles and what each uniquely proves

| Double | Line | Uniquely proves |
|---|---|---|
| `FakeCargo` | `:2761` | The general scripted shape: one locate root, N synthetic members, optional `build_directory`, three failure switches (`fail_locate`, `fail_metadata`, `malformed`). It **records nothing**, so it structurally cannot prove call counts. |
| `CountingRunner` | `:2879` | *Call counts.* Wraps `FakeCargo` with an `AtomicU64` on `argv[0] == "metadata"`, supplying the one observation `FakeCargo` cannot make — that one metadata per workspace holds while every manifest locates. |
| `MemberFirstFails` | `:3143` | The failure mode where a **member-first** locate failure is cached as permanently unresolved and never cleared. It discriminates on the manifest path so only the member's locate fails, then asserts `unresolved.is_empty()` **and** `cargo_failures == 1`: coverage is repaired, the diagnostic is not retracted. Without this test, a plausible cache-seeding bug would make resolution order-dependent. |
| `TwoWsRunner` | `:3729` | The *opposite* failure — over-eager member caching. Routes locate by substring on the manifest path to two independent roots and requires 2 locate + 2 metadata. If `member_to_root` were seeded wrongly, workspace B would be silently folded into A. |
| `FailFirstRunner` | `:3798` | A poisoned cache: one workspace's locate failure must not suppress an unrelated workspace's resolution. |
| `RecordingRunner` | `:5848` | *Argument composition*, with no Cargo at all. Records `(cwd, argv)` and returns failure; asserts `cwd.join(args.last()).exists()`. Carries an explicit anti-false-green assertion at `:5889-5892`: "the locate path was never exercised, so nothing was proven." |
| `BelievesRunner` / `BelievesOrphanRunner` | `:3903` / `:4004` | The M015 *believed workspace* message shapes: one whose named parent workspace resolved here (passenger) and one whose named parent did not (scope note). |
| `RefusedRunner` / `MetadataFailRunner` | `:4236` / `:4281` | The two capability classes at the resolution boundary: a Cargo-refused locate records `RefusedByCargo`; a metadata failure records `Unproven`. |

### Platform risk — and it is real in this file

The brief's premise that "the CI never needs a real `cargo`" is **not true for
`workspace.rs`**. Four tests drive `SystemCargoRunner` against real fixtures
(`:5915`, `:6006`, `:6048`, `:6097`), one drives a recording runner against the
real argument shape (`:5847`), and one calls `Command::new("cargo")`
directly (`:5918`).

That is defensible and deliberate — `:5908-5913` states the reason: a stub that
ignores the manifest path would not have caught C015. But it puts this file in
the same class as the project's documented false-green incidents. `plans/registry.md:275-277`
records C012: "a Windows case that had
never executed and was green only because an unrelated real `cargo install`
failed". The exact shape of that hazard is reproduced in this file's
own history: the version gate at `:5934-5942` once compared the **patch**
component, so `1.99.0` read as patch 0, fell under the threshold, and the case
"silently skipped -- passing without proving anything, **twice**". The gate now
compares the **minor** component against 77, which is below the 1.89 MSRV, so on
any supported toolchain it cannot fire. That is a real, verifiable mitigation.

Residual risks, honestly stated:

1. The skip is a bare `return` inside a passing test (`:5939-5942`). It is
self-documenting via `eprintln!`, but nothing makes it *fail*. A toolchain
older than 1.77 would quietly convert the real-Cargo tests into skips.
2. `:6048` writes a `.cargo/config.toml` with a forward-slashed path and a
   `Cargo.lock`, with the Windows rationale in a comment (`:6059-6061`) — the lane
   risk is at least partly handled by hand, and `--offline --locked` makes a
   network-dependent false green impossible.
3. `scripts/check-fixture-portability.py` **does** cover this file:
   `SCANNED_GLOBS` includes `src/**/*.rs` (script lines 70-75), with production
   explicitly *not* excluded — "a cleanup tool that shells out to cargo is not a
   fixture" (script lines 67-69). So the literal-`PATH`-separator and
   unjustified-shebang
   rules apply here. But the script's own docstring disclaims the check this
   module most needs: "whether a case's pass condition proves the intended branch
   … Reviewing that is the point of the C013 inventory" (script lines 46-47).

**Honest gaps:** no test for a *conflicting* member→root mapping (`:974-986`);
no test asserting a diagnostic for a
symlink-only output root, because none is emitted; and
`observer_counts_progress_without_allocation_per_entry` (`:5139`) asserts one
atomic on a `TestObserver` without exercising this module at all.
The unreadable-covering-root → `MissingOutput` path through the `None` branch of
`has_artifact_entries` (`:1741`) is now *partly* covered from the other side —
`unreadable_output_root_is_uncertain_never_absent` (`:6437`) proves the probe
reports `exists: true` with no physical identity — but the `read_dir` failure
inside `has_artifact_entries` itself is still unpinned.

---

## 11. Review checklist

1. **Every new failure path is a diagnostic, never a `return` from the loop.**
   Any `?`/`?`-like early exit added to `resolve_workspaces_with_coverage`
   (`:738-1057`) would silently shrink the ownership universe. Compare `:848-859`
   and `:1103-1120`.
2. **Any new output root must be added to all four consuming sites** or it will
   be invisible to some: `build_groups` node collection (`:1480-1512`),
   `all_outputs` exclusion (`:1760-1770`), `map_workspace_groups` (`:2136-2149`),
   `build_cleanup_units` `unmapped` (`:2229-2237`).
3. **Exclusivity is per group, and it is the only thing standing between an
   incomplete graph and a `PrivateBounded` claim.** A root that contributes no
   node must contribute its known aliases to `uncertain_aliases`
   (`:1496`, `:1509`), and the classification must consult
   `group_overlaps_uncertain` (`:1598`) before granting `PrivateBounded`.
   Widening the alias set is safe; narrowing it is not. Tests: `:4831` and
   `:4859`.
4. **Never canonicalize a symlink.** `output_root` (`:289-357`) checks
   `symlink_metadata` *before* `canonicalize`; reordering those two lines would
   turn a refused root into a followed one. Also keep `traverse`'s
   `symlink_metadata` guards (`traverse.rs:137`, `136`, `202`, `244`).
5. **Record the stage and capability where the diagnostic is pushed**, not by
   parsing the message back. Every new failure branch needs its own
   `unresolved_stage.insert(..)` entry (`:857`, `:869-872`, `:884`, `:897`,
   `:935`, `:954`, `:994`); a copy-edit to a diagnostic string must never
   reclassify a participant.
6. **Do not start consulting the `recency: Duration` parameter** (`:1960-1963`).
   The window is the caller's `clock_cutoff`; the `let _ = recency;` is a
   deliberate scar.
7. **Preserve determinism at every `HashMap` iteration point.** `groups_map` is
   iterated at `:1548` and neutralized by the sort at `:1620`; the
   `bytes_by_survivor` map (`:1902-1905`) is what makes the parallel
   `measure_many_targets` order-independent. Removing either sort reintroduces
   nondeterminism into the report.
8. **Any new `GroupSkipReason` variant needs a `detail()` arm (`:1676-1686`) and a
   route through `analyze_groups_detailed`** — and the detail string is the user
   answer to "why isn't my project listed".
9. **Keep `bytes` a deduplicated union, never a sum of overlapping groups**
   (`:2266-2275`, and the aggregation at `:1925-1944` which must keep breaking on
   the first uncertain contributor). Test: `:6157`.
10. **Keep `fully_measured()` the sole atomicity gate** (`:2210-2212`). Adding a
    per-group cleanable flag would let a `PrivateBounded` target authorize a
    sibling `Shared` build root, which ADR C003 forbids.
11. **Version-gated capability changes are empirical, not documented.** The
    allowlist at `:147-163` may only gain a release after real `cargo clean`
    qualification with a *configured* `build.target`; unknown versions stay
    `false` (tests `cleanup.rs:4582-4597`).
12. **New real-Cargo tests need a premise assertion, not a stub.** Follow
    `:5728-5731` (the case ran) and `:5757-5765` (cargo is present and
    responded) — and check the new file against
    `scripts/check-fixture-portability.py`.
