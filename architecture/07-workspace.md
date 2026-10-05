# Workspace — Cargo resolution, physical grouping, and cleanup units

> Component deep dive · part of the [architecture overview](overview.md)

`src/workspace.rs` — 3771 lines: 1446 production, 2325 inline `#[cfg(test)]`,
44 `#[test]` functions. The test module begins at `workspace.rs:1446`.

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
`measure_many_targets` (`:1088`) and `workspace_member_activity` (`:1437`).
Plus one fully-qualified `crate::domain::elapsed_nanos` (`:1003`, `:1092`).

**Does not own:**

- **Activity / inactivity.** Source recency comes from `traverse::workspace_member_activity`
  (`workspace.rs:1437`); byte sizing and output recency come from
  `traverse::measure_many_targets` (`workspace.rs:1088`). This module decides
  *whether to ask* and *what the answer means for a group*, never the answer.
- **Deletion.** Nothing here removes a byte. `cleanup.rs` invokes `cargo clean`.
- **Cargo configuration semantics.** The module doc states it outright
  (`workspace.rs:1-6`): it never reimplements Cargo's config merge rules, it asks
  Cargo. The only parsing of Cargo output is JSON.
- **Reporting.** This module produces `PhysicalOutputGroup`; the report-facing
  `EligibleOutputGroup` is built in `main.rs:471-484`.

**The workspace → cleanup edge is one-way, and in this file it is test-only.**
The production half of `workspace.rs` contains *zero* references to
`crate::cleanup`. The single call is at `workspace.rs:2327`
(`crate::cleanup::covering_is_authorized`), inside the test module, asserting
that a source-overlapping group is refused even by the most permissive caller.
So the dependency direction that [the overview](overview.md) calls out is
`cleanup → workspace` in production (`cleanup.rs:1292`, `1292-1324`, `1526`,
`1622`, `1852`, `1931`, `1951`) plus a test-only back-reference. There is no
cycle and no production import from `workspace` into `cleanup`.

Call sites in the read-only scan: `main.rs:346` (runner), `main.rs:347`
(`resolve_workspaces`), `main.rs:446` (`build_groups`), `main.rs:454`
(`analyze_groups`).

---

## 2. Public surface

Twenty-two public items. Nothing else in the file is reachable from outside.

### Process / IO

| Item | Signature | Contract |
|---|---|---|
| `ProcessOutput` | `struct { success: bool, code: Option<i32>, stdout: Vec<u8>, stderr: Vec<u8> }` (`:21-27`) | A cargo subprocess result. Raw `Vec<u8>` on both streams, so a non-UTF-8 filename in cargo's output cannot break the decoder. `stderr` is carried but **never read** by the production half — see §9. |
| `CargoRunner` | `trait { fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput> }` (`:29-31`) | The project's only external-process boundary. No supertraits, no associated types. |
| `SystemCargoRunner` | `struct SystemCargoRunner;` (`:78-93`) | The real implementation. Zero-sized. |
| `refresh_workspace_from_root_manifest` | `(root_manifest: &Path, runner: &dyn CargoRunner, counters, diagnostics, observer) -> Option<ResolvedWorkspace>` (`:59-76`) | Re-resolve one *known* root directly through `cargo metadata`: one process, no `locate-project`. Never discovers anything. Used by the cleanup final proof (`cleanup.rs:1526`). |

### Resolution

| Item | Signature | Contract |
|---|---|---|
| `resolve_workspaces` | `(manifests: &[PathBuf], runner, counters, diagnostics, observer) -> Vec<ResolvedWorkspace>` (`:301-309`) | Thin wrapper: `resolve_workspaces_with_coverage(..).workspaces`. Read-only scan entry point. |
| `resolve_workspaces_with_coverage` | same params `-> ResolutionCoverage` (`:315-487`) | The real implementation. Also returns the *unresolved* set, which is what makes the destructive scope fail-closed. |
| `capability_from_metadata` | `(had_build_field: bool, target: &str, build: Option<&str>, env_set: bool) -> CargoCapabilities` (`:124-142`) | Pure state machine over metadata evidence. No IO. |
| `clean_capabilities_from_version` | `(version: &str) -> CargoCleanCapabilities` (`:146-165`) | Exact-string allowlist of Cargo versions whose `cargo clean` selector behaviour was qualified. |

### Grouping

| Item | Signature | Contract |
|---|---|---|
| `outermost` | `(roots: &[PathBuf]) -> Vec<PathBuf>` (`:686-704`) | Collapse a root set to its minimal non-nested antichain, sorted. The primitive that makes "counted once" possible. |
| `build_groups` | `(workspaces: &[ResolvedWorkspace]) -> Vec<RawGroup>` (`:721-838`) | Union-find over canonical physical roots; assigns an `OutputOwnershipClass` to each connected component. |
| `RawGroup` | `struct { physicals, covering, display, owners: Vec<usize>, ownership, source_overlap }` (`:840-851`) | Pre-measurement group. `owners` are indices into the caller's `workspaces` slice, not IDs. |
| `has_artifact_entries` | `(path: &Path) -> Option<bool>` (`:855-858`) | `Some(true)` if the directory has ≥1 entry, `Some(false)` if empty, `None` if unreadable. One `read_dir` + one `next()`. |
| `GroupAnalysisInput` | `struct { group: RawGroup, owner_workspaces: Vec<ResolvedWorkspace> }` (`:860-863`) | **Dead.** Declared, never constructed, never referenced anywhere in `src/` or `tests/`. `analyze_groups_detailed` does not take it — see §7. |
| `workspace_member_roots` | `(ws: &ResolvedWorkspace) -> Vec<PathBuf>` (`:865-870`) | Member `source_root`s, deduped, passed through `outermost`. Also used by `cleanup.rs:2015`. |
| `GroupSkipReason` | 7-variant enum (`:873-882`) + `detail(self) -> &'static str` (`:885-895`) | Why a group produced no candidate. Full table in §7. |
| `GroupOutcome` | `struct { group: RawGroup, measured: Option<PhysicalOutputGroup>, skip: Option<GroupSkipReason> }` (`:903-908`) | One per input group, in input order. Exactly one of `measured`/`skip` is `Some`. |
| `analyze_groups_detailed` | 9 params `-> Vec<GroupOutcome>` (`:918-1217`) | **The real implementation.** Fail-fast gate pipeline. |
| `analyze_groups` | identical params `-> Vec<PhysicalOutputGroup>` (`:1222-1253`) | **Delegates** to `analyze_groups_detailed`, drops outcomes with no `measured`, sorts by bytes descending then `display_path`. Used by the read-only scan. |
| `map_workspace_groups` | `(ws: &ResolvedWorkspace, groups: &[RawGroup]) -> Vec<usize>` (`:1264-1287`) | The inverse direction: which group indices this workspace's `OutputSet` can touch. |

### Units

| Item | Signature | Contract |
|---|---|---|
| `AffectedGroup` | `struct { display, covering, ownership, kinds: Vec<OutputRootKind>, measured, skip }` (`:1290-1300`) | One physical group as seen *through one workspace's* output set. |
| `AffectedGroup::kind_label` | `(&self) -> String` (`:1304-1318`) | `"target"`, `"build"`, `"target+build"`, or `"output"` when there are no kinds. |
| `CleanupUnit` | `struct { workspace_idx, id, root, root_manifest, output, capability, groups, unmapped, covering, bytes }` (`:1328-1342`) | One `cargo clean` invocation's full footprint. |
| `CleanupUnit::fully_measured` | `(&self) -> bool` (`:1346-1348`) | `unmapped.is_empty() && groups.iter().all(\|g\| g.measured.is_some())`. |
| `build_cleanup_units` | `(workspaces, outcomes: &[GroupOutcome]) -> Vec<CleanupUnit>` (`:1356-1426`) | One unit per workspace that has ≥1 measured affected group. |

---

## 3. The CargoRunner seam

```rust
pub trait CargoRunner {                                        // workspace.rs:29
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
}
```

One method. No associated types. **No `Send`, no `Sync` supertrait** — verified
against all five production use sites, which all take `&dyn CargoRunner`
(`:61`, `:303`, `:317`, `:492`). The concurrency requirement lives on the *other*
trait: `CleanupRunner: Sync` (`cleanup.rs:377`) is what makes `std::thread::scope`
legal in the parallel proof refresh (`cleanup.rs:1676`).
`WorkspaceCleanupAdapter` (`cleanup.rs:1332-1346`) implements `CargoRunner` over
a `&dyn CleanupRunner` so `cleanup.rs` can reuse this module's resolver without
inheriting its unbounded trait. Identical method signatures, different bounds —
that asymmetry is deliberate and load-bearing.

Because `run` takes `&self`, stateful fakes need interior mutability: this file's
own doubles use `AtomicU64` (`CountingRunner`, `:1571`) and `Mutex`
(`RecordingRunner`, `:3225`).

### What `SystemCargoRunner` actually runs

`:80-93`. It builds argv by handing the caller's `&[OsString]` straight to
`std::process::Command::new("cargo")`, sets `.current_dir(cwd)`, and calls
`.output()` — which captures both streams as `Vec<u8>` and blocks. A
non-zero exit is **not** an error: it becomes `ProcessOutput { success: false,
code: Some(n), .. }` and the caller decides. An `io::Result::Err` (cargo absent
from `PATH`, permission denied on the binary) is the only transport-level failure.

Two subcommands, both built by the module:

| Subcommand | argv | Built at |
|---|---|---|
| locate | `locate-project --workspace --manifest-path <manifest>` | `:340-349` |
| metadata | `metadata --offline --locked --no-deps --format-version 1 --manifest-path <root_manifest>` | `:498-510` |

`cwd` is the manifest's own parent in both cases (`:352`, `:511`). Because
`--manifest-path` is resolved by the *child*, and the child's cwd is the
manifest's parent, a relative manifest would be composed against the wrong
directory — the `fixture/Cargo.toml` + cwd `fixture/` defect documented at
`workspace.rs:33-45` and fixed by `manifest_path_for_cargo` (`:46-54`), which
lexically absolutizes against the process cwd without resolving symlinks.
`cleanup.rs` adds `--profile` / `--package` and a frozen environment via
`run_with_env` on its own trait; `cargo clean` never goes through this module.

### Why this trait costs 2325 lines of tests

Every external process crosses this one method, so a fake can substitute for all
of Cargo. The six doubles in this file — `FakeCargo` (`:1451`, impl `:1461`),
`CountingRunner` (`:1569`), `MemberFirstFails` (`:1831`), `TwoWsRunner` (`:2003`),
`FailFirstRunner` (`:2072`), `RecordingRunner` (`:3224`) — plus ~25 hand-built
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
(`:3291`) uses `SystemCargoRunner` and the *real* Cargo, with the reason stated
in its own doc comment (`:3284-3289`): a stub that ignores the manifest path
would not have detected the C015 defect. The test module's most recent
deliberate design choice is therefore to add a *real*-Cargo case and a premise
assertion (`RecordingRunner` asserts `!seen.is_empty()` at `:3265-3268` with the
message "the locate path was never exercised, so nothing was proven") rather
than to trust the fake.

---

## 4. Metadata resolution

`resolve_workspaces_with_coverage` (`:315-487`) is a single sequential `for`
loop over `manifests`. There is no parallelism anywhere in this module's
resolution, and no batching — cargo has no batch metadata mode, so batching means
in-process deduplication.

### Per-manifest algorithm

For each discovered manifest, in input order:

1. **Fast path (`:332-339`).** Canonicalize the manifest; if it is already in
   `member_to_root` *and* that root is in `cache`, take it as covered:
   `deduped_workspace_hits += 1`, add to `authoritative_coverage`, `continue`.
   **Zero subprocesses.**
2. **`locate-project`** (`:340-397`). `cargo_locate_calls += 1`; wall time
   accumulated into `cargo_locate_nanos` with a saturating add. Three failure
   modes, each a `Warning` / `CandidateUncertain` diagnostic and `continue`:
   process could not start (`:355-368`), non-zero exit (`:373-383`), unparseable
   stdout (`:384-397`).
3. **Workspace cache (`:399-413`).** If the located root's canonical key is
   already in `cache`, count a dedup hit, register the manifest in
   `member_to_root`, and stop. **No metadata process.**
4. **`cargo metadata`** (`:415`, `:498-655`) once per unique workspace root.
5. **Seeding (`:427-442`).** Every member manifest returned by that metadata,
   plus the canonical root manifest, is inserted into `authoritative_coverage`
   and `member_to_root`. This is the C002 §7.6 mechanism the doc comment at
   `:294-300` describes.

### Is `cargo metadata` deduplicated?

Yes, by two independent keys. `cache` is keyed on the canonical workspace root
manifest and guards the metadata call (`:407`); `member_to_root` is keyed on the
canonical *member* manifest and eliminates the locate. Consequence, and the
testable claim at `:1956-1998`: **an N-member workspace costs exactly one
`locate-project` and one `cargo metadata`, in either discovery order**.

### Does a member manifest short-circuit a nested metadata call?

Yes — but the short-circuit is *discovery-order dependent*, and that is the point
of the `MemberFirstFails` test. If a member is seen first, step 2 runs a locate
for it, and `locate-project --workspace` returns the *workspace root*, so step 4
issues metadata against the **root** manifest, not the member (`:415` passes
`&root_manifest`). The member never gets its own metadata. If the member's locate
*fails*, nothing is seeded and the later root manifest's success repairs the
coverage (`:430-440`): `failed_member_first_is_cleared_by_later_authoritative_workspace_metadata`
(`:1822`) asserts `coverage.unresolved.is_empty()` **and** `cargo_failures == 1`
— the failed attempt's diagnostic is deliberately *not* retracted.

A member can cause a second, independent resolution only when `locate-project`
reports a genuinely different root, in which case two workspaces and two metadata
calls are correct. When membership *conflicts*, `member_to_root.entry(key).or_insert(...)`
(`:433`, `:438`) is **first-writer-wins** and the conflict is neither detected
nor reported; combined with the fast path at `:332-339`, a manifest mapped to the
wrong root is never re-located in that pass. No test covers a genuine conflict.

### Failure handling

One bad manifest never aborts anything. Every failure path is `continue`, so the
loop keeps going and the scan degrades to a partial result. Diagnostics are
`Warning` + `CandidateUncertain` and accumulate in the caller's vector; the
counters split the concepts — `cargo_failures` counts subprocess problems,
`unresolved_ownership` counts uncovered manifests (`domain.rs:251-253`).

Metadata-parse failures mirror the locate ones (`:539-565`). Two further
`return None` cases inside `resolve_one_workspace_cached`: a relative
`workspace_root` from Cargo (`:573-584`) and a member whose physical source root
cannot be established (`:587-604`) — the latter drops the *whole workspace*, not
just the member, because a member without a source root would make the
`PrivateBounded` source-disjointness test meaningless.

### `ResolutionCoverage` and `UnresolvedOwnershipParticipant`

`ResolutionCoverage` (`domain.rs:177-183`) = `workspaces` + `unresolved` +
`discovered_manifest_count`. `UnresolvedOwnershipParticipant`
(`domain.rs:169-175`) = `{ manifest, stage: &'static str, reason: String }`,
where `stage` is one of `"locate"`, `"metadata"`, `"identity"`.

Populated at `:449-481`: every discovered manifest not in `authoritative_coverage`
becomes one entry. The `reason` is found by scanning `diagnostics` **backwards**
for the last one whose `path` equals the manifest (`:456-463`), falling back to
"no successful Cargo workspace metadata covered this manifest". Sorted by path,
then deduped on the canonical key (`:478-481`).

**This is the safety hinge.** Per ADR 001's C004 clarification
(`plans/adr/001-...md:217-229`): because cargo-cleanme delegates configuration
semantics to Cargo, it cannot prove that an uncovered manifest does not overlap
another workspace's output. So Preview, Simulate and Execute **fail closed for
the entire cleanup scope** while any participant remains — no `cargo clean`, not
even `--dry-run`. `resolve_workspaces` (`:301-309`) discards the `unresolved`
vector, which is exactly why the doc comment at `:311-314` says its
partial-result behaviour is "intentionally unchanged" and the read-only scan
stays tolerant.

**Documented fragility:** `stage` is not carried by the diagnostic. It is
re-inferred by substring-matching the message (`:464-470`): contains
`"locate-project"` → `locate`, contains `"metadata"` → `metadata`, else
`identity`. Today every message that should classify correctly does (e.g.
`"cannot establish physical workspace member source root for …"` contains
neither and correctly yields `identity`), but the field is coupled to human
strings. A copy edit that drops a subcommand name silently reclassifies a
participant's stage. `stage` is a display/labelling convenience only — the
fail-closed decision keys on `unresolved.is_empty()`, not on `stage`.

### Progress

Resolution reports through the observer only, and only sparsely:
`observer.workspaces_resolved(1)` per unique workspace (`:425`) and
`observer.cargo_failure()` on every failure (`:360`, `:376`, `:388`, `:526`,
`:542`, `:556`). `main.rs:344` sets the phase to `ScanPhase::Resolution` before
the call. There is no `units_total` for the resolution phase — only analysis
announces a determinate total (`:932`).

---

## 5. Capability probing

Two independent probes answer two different questions.

### `capability_from_metadata` (`:124-142`) — "what did this Cargo tell me?"

`CargoBuildDirCapability` (`domain.rs:112-122`) is a four-state answer about
whether a *separate* build directory exists:

| `had_build_field` | `build` | `env_set` | Result | Reading |
|---|---|---|---|---|
| false | any | false | `Unavailable` | Pre-1.91 stable Cargo: `build_directory` did not exist, and no env evidence contradicts it. |
| false | any | true | `Unknown` | Field absent **but** `CARGO_BUILD_BUILD_DIR` is set in our environment. Absence is not proof. |
| true | `None` | any | `Unknown` | Field present but empty/null — malformed. |
| true | `Some(b)`, `b == target` | any | `Equal` | Modern Cargo; build and target are the same directory. |
| true | `Some(_)` | any | `Distinct` | Modern Cargo; two real directories. |

The two raw booleans travel alongside in `CargoCapabilities`
(`:137-141`, `domain.rs:124-129`) so downstream code never has to re-derive them.

`had_build_field` is detected by a substring search over the whole stdout
(`:551`, `String::from_utf8_lossy(...).contains("build_directory")`) rather than
by serde. That is crude but deliberate: serde's `Option<String>`
(`RawMetadata::build_directory`, `:107-108`) collapses "field absent" and
"field null" into the same value, and the two cases need different answers.

The `Unknown` arm exists because of ADR 001 §3
(`plans/adr/001-...md:54-64`): *"Absence of `build_directory` is never blindly
treated as proof that a configured nightly build directory does not exist."*
The env probe (`cargo_env_build_dir_set`, `:120-122`) is the only evidence that
can contradict absence, and it reads **our** process environment, not the child
cargo's. If the two differ the answer is wrong in the `Unknown` direction, which
is the safe direction. Four tests cover the table: `:1535`, `:1541`, `:1549`,
`:1555`.

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
per M008D's closure record ("Exact Cargo 1.98.1/1.99.0 behavior … hosted
platform/MSRV gates pass", `plans/registry.md:68`).

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
`exploratory_toolchains_are_never_promoted_by_accident`, and the hosted
exploratory job is `continue-on-error` so a stable-Cargo regression is reported
without reading as a product regression.

`read_cargo_record`'s sibling here is the same idea in the updater: a support
claim that cannot fail its own check is decoration.

The asymmetry between the two selectors is the substantive part.
`package_selector` is gated to exactly the two releases where the
*configured* `build.target` discrepancy was resolved — the comment at `:161-162`
bounds it to "exact tested releases where configured and explicit target
dry-runs agree". `profile_selector` covers a wider, older set.

Consequences, all verified in `cleanup.rs:2605-2617`: `1.96.0` → both false;
`1.99.1` → both false; `1.99.0-nightly` → both false; `"unknown"` → both false.
`runtime_clean_capabilities` (`cleanup.rs:2203-2215`) shells `cargo --version`,
takes whitespace token 1, and **returns `CargoCleanCapabilities::default()` —
all false — on any failure whatsoever**. So a user on an old, new, nightly, or
absent Cargo gets the reduced behaviour: the clean still runs, just without
`--profile` / `--package` selectors, i.e. at whole-target scope rather than a
narrower one. Safe and boring, never broken. That is the whole design intent of
this function.

### The build-directory version boundary

`CargoBuildDirCapability` splits at Cargo 1.91 (`domain.rs:114-116`:
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

### `outermost` (`:686-704`)

Sort, dedup, then sweep: skip a candidate already contained in a kept root;
otherwise evict any kept root the candidate contains; push; final sort. Result
is the minimal antichain under path-prefix containment — a deterministic,
idempotent reduction used in four places: per-group `covering` (`:792`),
`workspace_member_roots` (`:869`), and the cleanup unit's deduplicated union
(`:1401`), plus `cleanup.rs:1951`.

### `build_groups` (`:721-838`)

1. **Node collection (`:728-748`).** Per workspace, for each of `output.target` and
   `output.build`: `is_symlink` → mark the workspace uncertain, no node (`:732`);
   `!exists` → nothing, and *not* marked (`:736-737`), because a missing directory
   has no bytes to share; `physical_path.is_none()` (exists, uncanonicalizable) →
   mark uncertain (`:744-746`); otherwise push `Node { ws, physical }`.

2. **Union-find (`:750-773`).** O(n²) pairwise scan, union `i,j` when
   `contains(pi, pj) || contains(pj, pi)`. `contains` (`:659-661`) is a pure
   `starts_with` component test — equality counts, per the doc at `:658`.

3. **Components → groups (`:774-834`).** `physicals` sorted+deduped (`:786-788`),
   `owners` sorted+deduped (`:789-791`), `covering = outermost(physicals)` (`:792`),
   `display = covering[0]` (`:794-797`).

4. **Classification (`:799-825`),** in this exact order:

| Condition | Class |
|---|---|
| `owners.len() > 1` | `Shared` |
| owner is in `uncertain_ws`, **or** a covering root is (or contains) a member `source_root`, **or** a covering root is named `src`/`tests`/`benches`/`examples` | `Uncertain` |
| single owner, every covering root `strictly_below(ws.root)`, and `!exclusivity_unproven` | `PrivateBounded` |
| single owner, inside the root, but `exclusivity_unproven` | `Shared` |
| single owner, some covering root outside the workspace root | `ExternalUnproven` |

5. **Sort by `display` (`:836`).**

**The load-bearing detail: `exclusivity_unproven` (`:784`).** A root that
contributes no node — a symlink, or one with no physical identity — is invisible
to the union-find, yet it can still point into any group. So the graph is
incomplete, and the code refuses to let *anyone* claim `PrivateBounded` while
that is true: `let exclusivity_unproven = !uncertain_ws.is_empty();`. A
workspace with one provable `target/` and one symlinked `build/` therefore loses
its `PrivateBounded` claim entirely — exactly the case
`another_workspaces_unrepresentable_root_blocks_a_private_claim` (`:2361`,
unix-only) exists to catch. Note the residual asymmetry: a single-owner group
whose covering root lies *outside* the workspace root still gets
`ExternalUnproven` even when `exclusivity_unproven` is set, because the third
arm is guarded by `inside_root`. That is safe by construction —
`ExternalUnproven` is inventory-only, and `covering_is_authorized_roots`
(`cleanup.rs:493-495`) returns `Ok(false)` for anything that is not
`PrivateBounded`.

`strictly_below` (`:681-683`) exists because `contains` is inclusive: grouping
*needs* equality, but an artifact must sit strictly inside its workspace, so the
workspace root itself can never be an output root. `is_conventional_source_dir`
(`:670-674`) is a name-based backstop for `target-dir = "src"`; its doc comment
states the design rule: it "can only ever *withhold* a cleanup, never authorize
one."

### `has_artifact_entries` (`:855-858`)

`fs::read_dir(path).ok()?` then `entries.next().is_some()`. One syscall pair. It
exists because `analyze_groups_detailed` must reject empty and missing output
*before* any source walk or deep sizing (`:933-963`) — sizing a tree is the
expensive step, and a `read_dir` returning no entries is a complete negative
answer for the price of one directory read. The trade-off is in the signature:
`Option<bool>` cannot distinguish "empty" from "unreadable", and the caller folds
the `None` case into `missing_all` (`:950`), so an **unreadable** covering root is
reported as `MissingOutput` rather than as a diagnostic. That is a real, if
narrow, regression against `output_root`'s own care ("Unreadable is not absent",
`:229-243`); the window is small, since `read_dir` needs only read permission on
the directory itself, but it is the one place where fail-closed reporting is
weaker than the rest of the module.

### `workspace_member_roots` (`:865-870`)

Member `source_root`s, sorted, deduped, `outermost`-reduced. Two reasons members
participate, not just the workspace root:

1. **Source-activity coverage.** ADR §9 (`plans/adr/001-...md:117-123`): "A
   workspace is protected by recent source activity in any workspace member."
   `workspace_source_activity` (`:1428-1444`) walks every member root and returns
   `Ok(true)` on the first `Recent`, `Err(())` on the first uncertainty.
   `out_of_tree_member_is_included_and_protects` (`:3034`) is the test — a member
   outside the workspace directory still protects.
2. **Source/output disjointness.** `build_groups:808` tests every member's
   `source_root` against every covering root. A target dir that contains a
   member's sources is a misconfiguration that would delete the project.

The `outermost` reduction matters for a nested layout: a workspace whose members
are `ws/` and `ws/inner/` must not have `ws/inner/` walked twice, and `inner`'s
own entries are already covered by the `ws/` walk.

---

## 7. Group analysis

### `GroupAnalysisInput` is not an input

`analyze_groups_detailed` (`:918-928`) takes `workspaces: &[ResolvedWorkspace]`
and `groups: Vec<RawGroup>` as separate parameters. `GroupAnalysisInput` (`:860`)
is never constructed and never referenced in `src/` or `tests/`. The real inputs
are the two slice parameters plus `clock_start`, `clock_cutoff`, `recency`,
`counters`, `diagnostics`, `observer` — the last four of which exist to keep
`clippy::too_many_arguments` satisfied (`:918`) and to thread cross-phase state.

`analyze_groups` (`:1223`) is the delegating wrapper: same nine parameters,
calls the detailed form, keeps only `measured`, and sorts by
`(bytes desc, display_path)`.

### Gate order

Documented at `:910-913` as existence/type → artifact presence → source
activity → deep sizing. The implementation, in order:

**Stage 0** — announce the total (`:932`):
`observer.units_total(ScanPhase::Analysis, groups.len())`.

**Stage 1 — cheap presence (`:936-963`).** Probe every `covering` root per group,
building `group_has_entries` and `group_missing_all`. All-empty groups bump
`empty_no_output_skipped` + `observer.empty_skipped()`; all-missing groups bump
`missing_output_skipped`. Nothing expensive runs for either.

**Stage 2 — source activity, once per workspace (`:965-1024`).** Two fail-fast
skips before any walk: a workspace with no existing output (`:983-988`), and one
whose every group is already empty (`:990-998`). Otherwise
`workspace_source_activity(ws, &all_outputs, clock_start, clock_cutoff)`:
`Ok(true)` → `active_skipped += 1` + `observer.active_skipped()`; `Err(())` →
mark `ws_source_uncertain` and push an `Error` / `CandidateUncertain`
diagnostic (`:1016-1021`) — note the severity, unlike almost everything else in
the module. `all_outputs` (`:970-979`) is every workspace's `physical_path` (or
`logical_path` when there is none), so a source walk never counts the project's
own artifacts as source activity.

**Stage 3 — pre-skip survivors (`:1030-1077`),** first match wins:

| Order | Condition | `GroupSkipReason` | Counter |
|---|---|---|---|
| 1 | no covering root had an entry, and all were missing/unreadable | `MissingOutput` | `missing_output_skipped` (stage 1) |
| 2 | no covering root had an entry, at least one was readable-and-empty | `EmptyOutput` | `empty_no_output_skipped` (stage 1) |
| 3 | any owner's source is recent | `ActiveSource` | `active_skipped` (per workspace, stage 2) |
| 4 | any owner's source is uncertain | `UncertainSource` | `uncertain_skipped += 1` |
| 5 | `ownership == Uncertain` | `UncertainOwnership` | `uncertain_skipped += 1` |

Arm 5 additionally emits the source-overlap diagnostic when `g.source_overlap`
(`:1057-1070`): *"output root … is the workspace source tree, not a build
artifact; it is not sized and not eligible for cleanup"*. The comment above it
(`:1058-1060`) gives the reason — the user configured `target-dir` that way and
must be told, rather than being met with silence. Test: `:2389`.

**Stage 4 — deep sizing (`:1078-1096`).** One bounded pool, one call:
`traverse::measure_many_targets` over `(survivor_position, covering_path)` pairs.
Wall time → `output_sizing_nanos`. Results are re-indexed into a
`HashMap<usize, TargetStats>` (`:1093-1096`), so parallel completion order is
irrelevant.

**Stage 5 — aggregate per group (`:1104-1215`).** Sum `bytes` and `entries` with
`saturating_add`, take the max `newest` mtime, and — critically — **break and
mark uncertain if any contributing root is uncertain or missing from the result
map** (`:1121-1135`). A partial measurement is never promoted. Then, in order:

| Condition | Outcome |
|---|---|
| any contributor uncertain | `UncertainMeasurement` + `Error` diagnostic (`:1136-1151`) |
| `entries == 0` | `EmptyOutput` (`:1152-1162`) |
| `newest.is_some_and(\|t\| t >= clock_cutoff \|\| t > clock_start)` | `ActiveOutput` (`:1163-1175`) |
| otherwise | **survivor** → `PhysicalOutputGroup` (`:1196-1207`) |

Every branch, including the early-skipped ones at `:1105-1114`, calls
`observer.unit_completed(ScanPhase::Analysis)` exactly once, which is what makes
the analysis progress bar determinate. The survivor branch also fires
`observer.group_measured(bytes)` (`:1179`) and
`observer.reportable_group(&g.display, bytes)` (`:1208`).

### Clock handling

`clock_start` and `clock_cutoff` come from the caller; `main.rs:450-452` computes
`clock_cutoff = scan_start - recency` and turns an overflow into
`AppError::Config`.

**The `recency: Duration` parameter is accepted and deliberately ignored**
(`:1192-1195`):

```rust
// `recency` is deliberately not consulted here: the inactivity window is
// applied by the caller through `clock_cutoff`. Accepting a duration and
// ignoring it invited the belief that the policy lived here.
let _ = recency;
```

`analyze_groups` forwards it unchanged (`:1237`). A deliberate API wart, and the
comment explains why it was kept.

The output-recency predicate is `t >= clock_cutoff || t > clock_start` (`:1164`).
The second disjunct protects against future-dated timestamps — a skewed clock or
a restored backup must not be silently treated as ancient. The identical
predicate lives in `traverse.rs:369`, so source and output agree on what
"recent" means. ADR §9 (`plans/adr/001-...md:121-123`): "A physical output group
is protected by recent activity anywhere in that group. When a shared physical
group is recent, all owners are protected" — which is what arm 3 and this stage
jointly implement for multi-owner groups.

### `PhysicalOutputGroup` vs `EligibleOutputGroup`

**This module produces only `PhysicalOutputGroup`** (`:1196-1207`), with
`covering_roots`, `owners: Vec<WorkspaceId>`, and `uncertain`. It never
constructs `EligibleOutputGroup`; that flattening happens in `main.rs:471-484`,
which drops `covering_roots` and `uncertain` and maps `owners` to
`workspace_roots`. The distinction is the scan's report-facing projection versus
the internal working type.

`SizeMetric` is chosen at compile time, not runtime (`:1182-1191`):
`Allocated` on `unix`/`windows`, `Apparent` everywhere else.

### `GroupSkipReason` — the "why is my project not in the report" table

`detail()` (`:885-895`) is the user-facing text.

| Variant | `detail()` | Produced when |
|---|---|---|
| `MissingOutput` | `output directory is missing` | No covering root yielded a dir entry, and every root was missing or unreadable (`:1033-1034`). |
| `EmptyOutput` | `output directory contains no artifacts` | Every covering root was readable and empty (`:1035-1036`), **or** a measured group aggregated to `entries == 0` (`:1152-1160`). |
| `ActiveSource` | `workspace source activity is recent` | Any owner's member source was touched at/after the cutoff (`:1042-1048`). |
| `UncertainSource` | `workspace source activity could not be established` | Any owner's member walk returned `Err(())` (`:1049-1053`). |
| `UncertainOwnership` | `output ownership could not be established` | `g.ownership == Uncertain` — symlinked root, unresolvable identity, or source overlap (`:1054-1071`). |
| `UncertainMeasurement` | `output activity or size could not be established` | A covering root's stats were uncertain or absent from the result map (`:1136-1150`). |
| `ActiveOutput` | `output activity is recent` | Newest mtime in the group is at/after the cutoff, or in the future (`:1163-1175`). |

### `GroupOutcome` and `map_workspace_groups`

`GroupOutcome` (`:903-908`) is one entry per input group, in input order, with
exactly one of `measured` / `skip` set. Its reason for existing is in the comment
at `:915-917` and ADR C003 (`plans/adr/001-...md:207-212`): a workspace-level
cleanup decision must be able to *name* the sibling physical group that blocks
the whole unit, rather than silently dropping one group from a multi-group
`OutputSet`.

`map_workspace_groups` (`:1264-1287`) is the many-to-many inverse. Why
many-to-many:

- **one group → many workspaces.** A `Shared` group's single `cargo clean` from
  workspace A's perspective affects bytes workspace B also claims.
- **many groups → one workspace.** A workspace with a distinct `target/` and
  `build/` yields two groups (`:3550`, `distinct_sibling_target_build_measured_once_per_root`).
- **nested/equal roots collapse.** Both kinds pointing at the same physical path
  resolve to one group index, deduplicated by `!idxs.contains(&gi)` (`:1280`).

It skips roots that are symlinks, missing, or have no `physical_path` (`:1273-1278`)
— precisely the roots `build_groups` never made a node for, so the mapping is
total over the graph. Output is sorted by group display path (`:1285`).
`cleanup.rs:1931` uses it during final revalidation to compute the
`fresh_covering` union that `outermost` then reduces (`cleanup.rs:1951`).

---

## 8. Cleanup units

Unit-of-atomicity is the concept: **one `cargo clean` invocation is the
transaction**, so the unit must describe everything that invocation can touch.
ADR C003 (`plans/adr/001-...md:201-214`): "One Cargo clean invocation is
authorized as a workspace-level CleanupUnit covering the complete resolved
OutputSet that the invocation can affect. Every physical group touched by that
OutputSet must independently satisfy the destructive ownership and authorization
boundary before Cargo clean may run. A PrivateBounded target group cannot
authorize a sibling build group that is ExternalUnproven, Shared, Uncertain, or
unauthorized."

### `AffectedGroup` (`:1290-1300`)

A `RawGroup` re-expressed through *one* workspace's lens: the same
`RawGroup.display` / `covering` / `ownership`, plus `kinds` — the
`OutputRootKind`s of this workspace whose `physical_path` appears in
`o.group.physicals` (`:1379-1386`). Both kinds appear when target and build
collapse into one physical group; the test asserts
`kinds == [Target, Build]` and `kind_label() == "target+build"` (`:3637-3642`).
`kind_label()` (`:1304-1318`) sorts Target before Build, dedups, and falls back to
`"output"` for the empty case.

### `CleanupUnit` (`:1328-1342`)

| Field | Meaning |
|---|---|
| `workspace_idx`, `id`, `root`, `root_manifest` | identity, copied from `ResolvedWorkspace` (`:1412-1416`) |
| `output`, `capability` | the full `OutputSet` and Cargo capability evidence, needed to rebuild the command and the proof |
| `groups: Vec<AffectedGroup>` | every physical group this workspace's output set can touch, each with its own `measured`/`skip` |
| `unmapped: Vec<OutputRoot>` | existing, non-symlink roots with `physical_path == None` (`:1365-1372`) — an unbounded cleanup footprint |
| `covering: Vec<PathBuf>` | `outermost` of the union of all affected `covering` roots (`:1401-1406`) — the deduplicated footprint |
| `bytes: u64` | sum of the affected groups' measured bytes (`:1407-1410`) |

### `fully_measured()` (`:1346-1348`)

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

`unit_with_unmeasured_sibling_is_not_fully_measured` (`:3651-3681`) is the exact
case: a measured 4 KiB `target/` beside an existing-but-never-populated `build/`
directory. The unit exists (so the user sees it and learns why it is blocked) but
`fully_measured()` is false, and the unmeasured sibling carries
`skip == Some(EmptyOutput)` and `kinds == [Build]`.

### `build_cleanup_units` (`:1356-1426`)

Per workspace: map to groups (`:1363`), collect `unmapped` (`:1365-1372`), skip
entirely if both are empty (`:1373-1375`), build `AffectedGroup`s, then **skip
again if no affected group is measured** (`:1396-1400`) — "a workspace with no
reportable affected output is not a cleanup unit at all." Then the deduplicated
union and byte sum, and finally a deterministic sort by
`(bytes desc, root)` (`:1424`).

Note the consequence, verified by `unprovable_output_identity_makes_the_group_uncertain`
(`:3732-3769`): one unprovable output root makes the workspace `Uncertain`
through `build_groups:744-746`, every one of its groups is skipped, and
`build_cleanup_units` produces **no unit at all** — so the unprovable root never
even reaches `unmapped`, because `affected` is empty first.

### Ownership classes that can appear in a unit

`AffectedGroup.ownership` is copied from `RawGroup.ownership`
(`domain.rs:185-202`), so a unit can carry any of the four classes across
different groups. The safety property is not that a unit is uniformly
`PrivateBounded`; it is that `cleanup.rs` re-checks each group independently
through `covering_is_authorized_roots` (`cleanup.rs:484-495`), which returns
`Ok(false)` for anything that is not `PrivateBounded` — `ExternalUnproven`,
`Shared`, and `Uncertain` are all inventory-only, permanently
(ADR §7, `plans/adr/001-...md:101-107`). And even a `PrivateBounded` group is
still gated by the location check, so user authorization can never manufacture
proof.

---

## 9. Invariants and edge cases

**Is physical grouping a true partition?** **Yes over output-root nodes and
canonical directory trees; no over inodes or mount points.** The union-find is a
connected-components computation over a symmetric relation, so the components
partition `nodes` by construction (`:750-778`) and no path is emitted in two
groups. The stronger property holds too: if two *different* components held
overlapping bytes, one path would have to be an ancestor of the other — and
`contains` is checked in both directions (`:767-768`) — so they would have been
unioned. Equal and ancestor–descendant roots therefore cannot straddle a group
boundary, which is what makes "counted and acted on exactly once" true, and what
ADR §5 (`plans/adr/001-...md:76-86`) requires. Three ways a byte can still count
twice: **hard links** between two target directories (the walker sums
`file_real_size_fast` per path, `traverse.rs:212`, so each name counts);
**mount points** inside a target tree (`dua_core` descends into them; nothing here
or in `traverse.rs` checks for a mount boundary); and **case-insensitive
aliasing**, defended only by the single `fs::canonicalize` per root at `:266`,
not per entry. I did not empirically verify canonicalization on case-insensitive
macOS volumes; on Windows `canonicalize` returns verbatim paths, which is the
expected mitigation — reasoning, not a measurement in this repo.

**Can two different workspaces share one `target/`?** Yes, and that is a
first-class supported case, not an error. Two independent workspaces with the
same redirected `target-dir` produce two nodes at the same path, the union-find
merges them, and `owners.len() > 1` makes the group `Shared`
(`:800-801`) — measurable, reportable, never cleanable
(ADR §7). Asserted by `output_grouping_equal_nested_shared` (`:2208-2235`).
Within one workspace, the same physics gives a `PrivateBounded` group with
`kinds == [Target, Build]`. Note the *deliberate* asymmetry: a missing root is
invisible to the graph (`:736-737`) and does not block exclusivity, because no
bytes exist to share.

**Is the report order deterministic and independent of `cargo metadata`
completion order?** **Yes, unconditionally — and the premise is stronger than
expected: there is no concurrent metadata call in this module at all.** Resolution
is one sequential `for` loop (`:328-444`) with a synchronous `runner.run`, so
"completion order" cannot vary. Downstream, every source of potential
nondeterminism is erased before return:

| Site | Non-determinism | How it is removed |
|---|---|---|
| `:322-327` | `cache` / `locate_cache` / `member_to_root` HashMaps | Lookup only; never iterated |
| `:774-785` | `groups_map` **is** a `HashMap` **and is iterated** | Every group's `physicals` (`:786-788`), `owners` (`:789-791`) and `covering` (`outermost` → sorted, `:702`) are individually sorted, and the group list is sorted by `display` (`:836`) |
| `:449-477` | `unresolved` | Sorted by manifest (`:478`), deduped on the canonical key (`:479-481`) |
| `:1088` | `traverse::measure_many_targets` is parallel | Results are re-indexed into a `HashMap<usize, _>` keyed by input position (`:1093-1096`); completion order is discarded |
| `:1247-1251` | `analyze_groups` output | Sorted `(bytes desc, display_path)` |
| `:1285` | `map_workspace_groups` | Sorted by group display |
| `:1424` | `build_cleanup_units` | Sorted `(bytes desc, root)` |

One residual worth stating precisely: `resolve_workspaces` returns workspaces in
**first-seen discovery order** (`:426`), not sorted. The doc comment's "in
deterministic order" (`:291`) means "deterministic given a fixed input list", and
it holds because `ordered_keys` is a `Vec` appended in iteration order. No
consumer depends on it. The one *parallel* resolution in the project lives in
`cleanup.rs` (`refresh_proof_universe_with_limit`, `cleanup.rs:1660-1700`); it
threads `batch` with `std::thread::scope`, zips outcomes back against the input
batch in order (`:1700`), and `cleanup.rs:6110-6111` asserts sequential ==
parallel. So the determinism claim survives the parallel path too.

**`cargo` not on `PATH`?** A **diagnostic, not a hard error.** `Command::new("cargo")
.output()` returns `io::ErrorKind::NotFound`, which becomes `Err` from `run` and
is caught at `:353-368` (locate) and `:514-534` (metadata): `cargo_failures += 1`,
`observer.cargo_failure()`, a `Warning` / `CandidateUncertain` diagnostic saying
"cargo metadata could not start" or "cargo locate-project could not start", then
`continue` / `None`. The scan exits 0 with zero workspaces, and — via
`cleanup.rs`'s use of `resolve_workspaces_with_coverage` — every discovered
manifest lands in `unresolved`, failing the destructive scope closed. Note the
trade-off this creates: "cargo is not installed" and "nothing to clean" are
indistinguishable in a bare scan, which is precisely the C015-class silent-zero
shape the module's own comments at `:33-45` and `:3265-3268` guard against.
(`cleanup.rs`'s `runtime_clean_capabilities` returns all-false on the same
condition, `cleanup.rs:2205-2207`.)

**Warnings on stderr with a successful exit?** Ignored. `ProcessOutput.stderr`
is written by `SystemCargoRunner` (`:90`) and never read anywhere in the
production half — verified: the only two `stderr` occurrences in lines 1-1446 are
the field declaration and the assignment. A successful parse wins; cargo's
warning text cannot affect resolution. The `ProcessOutput` field is still carried
because the fakes populate it and `CleanupRunner` shares the type
(`cleanup.rs:375`).

**Malformed or unexpected metadata JSON?** A warning diagnostic, the workspace is
dropped, resolution continues (`:552-565`). Same for unparseable `locate-project`
output (`:384-397`) and for a relative `workspace_root` (`:573-584`). The
`UnresolvedOwnershipParticipant.stage` for the first two is `"metadata"` /
`"locate"`, recovered by substring match on the message (`:464-470`) — see §4.
Because `serde_json` is non-strict about *unknown* fields, a future Cargo that
adds fields still parses; only a *changed* `target_directory`/`workspace_root`
contract would break it.

**Is a symlinked `target/` followed, and can it escape the scope?** Not followed,
and it cannot escape — this module is a consistent no-op on symlinks.
`output_root` uses `fs::symlink_metadata` (`:221`), detects `file_type().is_symlink()`
and records `is_symlink: true` with `physical_path: None` **without canonicalizing**
(`:252-258`) — deliberately, because canonicalizing is what would resolve the
link. From there: `build_groups` contributes no node and marks the workspace
uncertain (`:732-735`); `map_workspace_groups` skips it (`:1273`);
`build_cleanup_units` skips it for both the mapped *and* the `unmapped` list
(`:1366`); and `traverse` never follows links either
(`traverse.rs:8-10`, `symlink_metadata` guards at `traverse.rs:67`, `148-149`,
`190`, plus `Options::default()` with no follow flag at `traverse.rs:35-37` — I
read this module's usage, not dua-core's default). Consequences worth naming: a
workspace whose *only* output is a symlink produces **no groups and no
diagnostic** — `symlink_output_is_uncertain` (`:2553-2589`) asserts exactly
`groups.is_empty()`. ADR §5 says symlink roots "are inventory diagnostics"
(`plans/adr/001-...md:86`); the code makes them *inert*, not *reported*. The
safety property still holds (the workspace is in `uncertain_ws`, so
`exclusivity_unproven` blocks every `PrivateBounded` claim machine-wide, `:784`),
but the user gets no message. **This is a real gap between ADR wording and
implementation**, and it is the one I would file. What the *spawned* `cargo
clean` does with such a root is outside this module; `cleanup.rs`'s final proof
(`cleanup.rs:1366-1381`) is the only thing that could catch a change between scan
and clean, and I did not audit that path in full.

**Can a member's manifest produce a disagreeing nested resolution?** Only by
being in a *different* workspace, which is correct behaviour. Within one
workspace, a member never gets its own metadata call (step 4 above). The
untested case is a *conflicting* mapping: `member_to_root.entry(..).or_insert(..)`
(`:433`, `:438`) is first-writer-wins and a conflict is neither detected nor
diagnosed. Combined with the fast path at `:332-339`, a manifest mapped to the
wrong root is never re-located within that pass. `TwoWsRunner` (`:2003`) proves
the *legitimate* two-workspace case; no test constructs a genuine conflict.

**Bounded, or O(manifests)?** **Bounded by unique workspaces, and asymptotically
O(manifests) — with no parallelism.**

- `cargo metadata`: exactly **one per unique canonical workspace root**, guarded
  by `cache.contains_key` at `:407` and counted at `:512`. So ≤ O(unique
  workspaces) ≤ O(manifests).
- `cargo locate-project`: **at most one per discovered manifest**, eliminated to
  zero for any manifest already covered by `member_to_root` (`:332-339`) or
  already cached (`:407-413`). Worst case is O(manifests): a machine of N
  unrelated single-crate projects costs N locate + N metadata.
- `refresh_workspace_from_root_manifest` (`:59-76`) costs one metadata and zero
  locate per call; `cleanup.rs:1526` calls it per candidate at the destructive
  boundary, tracked in the separate `proof_*` counters so `--stats` cannot hide
  it (`domain.rs:267-276`, `merge_proof` at `domain.rs:339-358`).

Performance implication: the cost is **N sequential subprocess round-trips**, and
`--locked` (`:500`) means a stale `Cargo.lock` fails metadata for that workspace
outright. The `cargo_locate_nanos` / `cargo_metadata_nanos` split
(`:370-372`, `:516-518`) exists so the user can see which half dominates.

---

## 10. Testing

**44 `#[test]` functions** — confirmed by count. The inline module starts at
`workspace.rs:1446` (`#[cfg(test)] mod tests {`).

| Area | Count | Tests (line) |
|---|---:|---|
| Capability probing | 4 | `:1535`, `:1541`, `:1549`, `:1555` |
| Metadata resolution & coverage | 15 | `:1561`, `:1622`, `:1677`, `:1822`, `:1895`, `:1925`, `:1956`, `:2001`, `:2071`, `:2138`, `:3223`, `:3291`, `:3382`, `:3424`, `:3473` |
| Grouping & ownership classification | 9 | `:2165`, `:2302`, `:2339`, `:2361`, `:2389`, `:2425`, `:2511`, `:2553`, `:2592` |
| Analysis, gates, measurement, progress | 10 | `:2605`, `:2790`, `:2839`, `:2861`, `:2889`, `:2945`, `:2982`, `:3034`, `:3080`, `:3125` |
| Cleanup units | 6 | `:3533`, `:3585`, `:3651`, `:3684`, `:3706`, `:3732` |

Most significant: `n_member_workspace_requires_one_locate_one_metadata_regardless_of_order`
(`:1956`) — the performance contract, proved in both discovery orders;
`another_workspaces_unrepresentable_root_blocks_a_private_claim` (`:2361`) — the
`exclusivity_unproven` safety net; `failed_member_first_is_cleared_by_later_authoritative_workspace_metadata`
(`:1822`) — coverage completeness under partial failure;
`unit_with_unmeasured_sibling_is_not_fully_measured` (`:3651`) — the atomicity
gate; `the_composed_locate_arguments_resolve_against_the_supplied_working_directory`
(`:3223`) — the C015 regression.

### Test doubles and what each uniquely proves

| Double | Line | Uniquely proves |
|---|---|---|
| `FakeCargo` | `:1451` | The general scripted shape: one locate root, N synthetic members, optional `build_directory`, three failure switches (`fail_locate`, `fail_metadata`, `malformed`). It **records nothing**, so it structurally cannot prove call counts. |
| `CountingRunner` | `:1569` | *Call counts.* Wraps `FakeCargo` with an `AtomicU64` on `argv[0] == "metadata"`, supplying the one observation `FakeCargo` cannot make — that 1 locate + 1 metadata is *achievable* for an N-member workspace. |
| `MemberFirstFails` | `:1831` | The failure mode where a **member-first** locate failure is cached as permanently unresolved and never cleared. It discriminates on `args.last()` so only the member's locate fails, then asserts `unresolved.is_empty()` **and** `cargo_failures == 1` (`:1888-1891`): coverage is repaired, the diagnostic is not retracted. Without this test, a plausible cache-seeding bug would make resolution order-dependent. |
| `TwoWsRunner` | `:2003` | The *opposite* failure — over-eager member caching. Routes locate by substring on the manifest path to two independent roots and requires 2 locate + 2 metadata. If `member_to_root` were seeded wrongly, workspace B would be silently folded into A. |
| `FailFirstRunner` | `:2072` | A poisoned cache: one workspace's locate failure must not suppress an unrelated workspace's resolution. |
| `RecordingRunner` | `:3224` | *Argument composition*, with no Cargo at all. Records `(cwd, argv)` and returns failure; asserts `cwd.join(args.last()).exists()`. Carries an explicit anti-false-green assertion at `:3265-3268`: "the locate path was never exercised, so nothing was proven." |

### Platform risk — and it is real in this file

The brief's premise that "the CI never needs a real `cargo`" is **not true for
`workspace.rs`**. Four tests drive `SystemCargoRunner` against real fixtures
(`:3291`, `:3382`, `:3424`, `:3473`) and one calls `Command::new("cargo")`
directly (`:3294`).

That is defensible and deliberate — `:3284-3289` states the reason: a stub that
ignores the manifest path would not have caught C015. But it puts this file in
the same class as the project's documented false-green incidents. `plans/registry.md:76`
records C012: "The Windows case had never executed; it was green on an unrelated
real-cargo failure." The exact shape of that hazard is reproduced in this file's
own history: the version gate at `:3310-3318` once compared the **patch**
component, so `1.99.0` read as patch 0, fell under the threshold, and the case
"silently skipped -- passing without proving anything, **twice**". The gate now
compares the **minor** component against 77, which is below the 1.89 MSRV, so on
any supported toolchain it cannot fire. That is a real, verifiable mitigation.

Residual risks, honestly stated:

1. The skip is a bare `return` inside a passing test (`:3317`). It is
   self-documenting via `eprintln!`, but nothing makes it *fail*. A toolchain
   older than 1.77 would quietly convert four real-Cargo tests into skips.
2. `:3424` writes a `.cargo/config.toml` with a forward-slashed path and a
   `Cargo.lock`, with the Windows rationale in a comment (`:3435-3436`) — the lane
   risk is at least partly handled by hand, and `--offline --locked` makes a
   network-dependent false green impossible.
3. `scripts/check-fixture-portability.py` **does** cover this file:
   `SCANNED_GLOBS` includes `src/**/*.rs`, with production explicitly *not*
   excluded — "a cleanup tool that shells out to cargo is not a fixture"
   (script lines 55-60). So the literal-`PATH`-separator and unjustified-shebang
   rules apply here. But the script's own docstring disclaims the check this
   module most needs: "whether a case's pass condition proves the intended branch
   … Reviewing that is the point of the C013 inventory" (script lines 25-27).

**Honest gaps:** no test for a *conflicting* member→root mapping (`:433`);
no test for an unreadable covering root reaching `MissingOutput` via the `None`
branch of `has_artifact_entries` (`:950`); no test asserting a diagnostic for a
symlink-only output root, because none is emitted; and
`observer_counts_progress_without_allocation_per_entry` (`:2605`) asserts three
atomics on a `TestObserver` without exercising this module at all.

---

## 11. Review checklist

1. **Every new failure path is a diagnostic, never a `return` from the loop.**
   Any `?`/`?`-like early exit added to `resolve_workspaces_with_coverage`
   (`:328-444`) would silently shrink the ownership universe. Compare `:353-368`
   and `:514-534`.
2. **Any new output root must be added to all four consuming sites** or it will
   be invisible to some: `build_groups` node collection (`:731-747`),
   `all_outputs` exclusion (`:971-979`), `map_workspace_groups` (`:1272-1284`),
   `build_cleanup_units` `unmapped` (`:1365-1372`).
3. **`exclusivity_unproven` (`:784`) is the only thing standing between an
   incomplete graph and a `PrivateBounded` claim.** Any change that adds a way for
   a root to contribute no node must preserve or widen this flag. Test:
   `:2361`.
4. **Never canonicalize a symlink.** `output_root` (`:221-258`) checks
   `symlink_metadata` *before* `canonicalize`; reordering those two lines would
   turn a refused root into a followed one. Also keep `traverse`'s
   `symlink_metadata` guards (`traverse.rs:67`, `148-149`, `190`).
5. **Keep the `stage` substring mapping (`:464-470`) in sync with the diagnostic
   strings it matches**, or `UnresolvedOwnershipParticipant.stage` will
   misclassify. Better: carry the stage on the diagnostic.
6. **Do not start consulting the `recency: Duration` parameter** (`:1192-1195`).
   The window is the caller's `clock_cutoff`; the `let _ = recency;` is a
   deliberate scar.
7. **Preserve determinism at every `HashMap` iteration point.** `groups_map` is
   iterated at `:785` and neutralized by the sort at `:836`; the
   `bytes_by_survivor` map (`:1093-1096`) is what makes the parallel
   `measure_many_targets` order-independent. Removing either sort reintroduces
   nondeterminism into the report.
8. **Any new `GroupSkipReason` variant needs a `detail()` arm (`:885-895`) and a
   route through `analyze_groups_detailed`** — and the detail string is the user
   answer to "why isn't my project listed".
9. **Keep `bytes` a deduplicated union, never a sum of overlapping groups**
   (`:1401-1410`, and the aggregation at `:1116-1135` which must keep breaking on
   the first uncertain contributor). Test: `:3573-3580`.
10. **Keep `fully_measured()` the sole atomicity gate** (`:1346-1348`). Adding a
    per-group cleanable flag would let a `PrivateBounded` target authorize a
    sibling `Shared` build root, which ADR C003 forbids.
11. **Version-gated capability changes are empirical, not documented.** The
    allowlist at `:147-163` may only gain a release after real `cargo clean`
    qualification with a *configured* `build.target`; unknown versions stay
    `false` (tests `cleanup.rs:2605-2617`).
12. **New real-Cargo tests need a premise assertion, not a stub.** Follow
    `:3265-3268` (the case ran) and `:3294-3302` (cargo is present and
    responded) — and check the new file against
    `scripts/check-fixture-portability.py`.
