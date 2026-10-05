# Domain & Errors — the vocabulary everything else speaks

> Component deep dive · part of the [architecture overview](overview.md)

Covers three files: `src/domain.rs` (381 lines), `src/error.rs` (16 lines),
`src/lib.rs` (15 lines).

---

## 1. Responsibility

**What these modules own.** `domain.rs` is the single place where the crate's
nouns are defined: what was asked for (`ScanRequest`, `EffectiveScanPolicy`),
what a workspace is (`ResolvedWorkspace`, `OutputSet`, `OutputRoot`), what a
candidate for deletion is (`PhysicalOutputGroup`), and — most importantly — the
vocabulary of *safety* (`OutputOwnershipClass`, `DiagnosticCategory`,
`ScanDiagnostic`, `UnresolvedOwnershipParticipant`). `error.rs` owns the one
error type the binary propagates, `AppError`, whose six variants are the
complete set of ways a run can fail fatally. `lib.rs` owns nothing but the list
of `pub mod` declarations. Together they are the vocabulary; `discovery`,
`workspace`, `traverse`, `cleanup`, `output` and `report` are the verbs.

**What they deliberately do not own.** There is no behaviour here beyond
formatting and merging: no filesystem access, no subprocess spawning, no policy
decision, no I/O, no serialization. `domain.rs`'s only imports are
`std::path::PathBuf` and `std::time::{Duration, SystemTime}` (`domain.rs:1-4`);
`error.rs`'s only import is `thiserror::Error` (`error.rs:1`). Neither file
contains `use crate::` or `use super::` — this is a verifiable leaf, and the
load-bearing structural claim in [the overview](overview.md) §2. The safety
vocabulary has no compile-time dependency on the machinery that could be tempted
to misuse it. Three absences are conspicuous: the cleanup-side report type
`CleanReport` lives in `cleanup.rs:170+`, not here; Cargo version → capability
*derivation* lives in `workspace.rs:124-165` even though the capability *types*
live here; and `Serialize` is derived on no domain type — the JSON contract
belongs to `output.rs`, which maps domain structs to DTOs by hand.

---

## 2. Public surface

### 2.1 `domain.rs` — request, scope and policy

| Item | Kind | Meaning |
|---|---|---|
| `ScanRequest` | struct | Raw user intent for `policy::resolve`: `cli_root: Option<PathBuf>`, `full: bool` (`domain.rs:7-10`). |
| `ScanScope` | enum | The *resolved* discovery boundary (`domain.rs:12-18`). |
| `DiscoveryFilters` | enum | Whether ignore rules apply to the walk (`domain.rs:20-26`). |
| `EffectiveScanPolicy` | struct | `recency: Duration` + `scope` + `discovery_filters` (`domain.rs:28-32`). |

| Enum | Variant | Meaning |
|---|---|---|
| `ScanScope` | `Explicit(PathBuf)` | One user-named root; forces `DiscoveryFilters::Bypassed` (`policy.rs:42`). |
| | `ExplicitRoots(Vec<PathBuf>)` | Several explicit roots forming one boundary; doc comment restricts it to *cleanup only* (`domain.rs:14`). Constructed only at `cleanup.rs:774` and `cleanup.rs:1606`. |
| | `Global(Vec<PathBuf>)` | Machine-wide discovery roots (`policy.rs:23`). |
| | `Routine(Vec<PathBuf>)` | The learned-root set from the state file (`policy.rs:46`). |
| `DiscoveryFilters` | `Active { ignore, unignore }` | `Vec<String>` patterns and `Vec<PathBuf>` re-inclusions, handed to the walker (`discovery.rs:327`). |
| | `Bypassed` | No filtering; used for explicit and cleanup-internal scans. |

### 2.2 `domain.rs` — project and activity

| Item | Kind | Meaning |
|---|---|---|
| `DiscoveredProject` | struct | Manifest + logical/target paths (`domain.rs:34-38`). **Never constructed or referenced anywhere in `src/` or `tests/`.** |
| `ActivityState` | enum | `Active` / `Inactive` / `Uncertain` (`domain.rs:40-44`). **Never referenced outside its own definition** — inactivity is enforced by a cutoff comparison plus `GroupSkipReason` (`workspace.rs:1157-1170`). |
| `SizeMetric` | enum | `Allocated` (block-rounded) or `Apparent` (logical length) (`domain.rs:46-49`). Selected by `#[cfg]`: `Allocated` on unix/windows (`workspace.rs:1180-1188`). |

### 2.3 `domain.rs` — diagnostics (the uncertainty channel)

| Item | Kind | Meaning |
|---|---|---|
| `DiagnosticSeverity` | enum | `Info` / `Warning` / `Error` (`domain.rs:51-55`). |
| `ScanDiagnostic` | struct | `severity`, `category`, `path: Option<PathBuf>`, `message` (`domain.rs:77-82`). |
| `DiagnosticCategory` | enum | The closed set of *why we might be wrong* (`domain.rs:68-75`). |

| `DiagnosticCategory` | Meaning |
|---|---|
| `PermissionDenied` | The walk could not read a directory. |
| `Vanished` | A path disappeared between discovery and inspection. |
| `Metadata` | Metadata could not be established. |
| `InvalidEntry` | A directory entry is malformed (bad name, wrong file type). |
| `PlatformRoot` | A root is a filesystem root or otherwise not a real project root; at `Error` severity this makes a Full scan incomplete (`main.rs:384-386`, `main.rs:466-469`). |
| `CandidateUncertain` | A *candidate* was dropped or refused: source-tree overlap, filesystem-root target, unmeasurable output. |

`DiagnosticSeverity::as_str(self) -> &'static str` (`domain.rs:59-65`) maps to
`"info"` / `"warning"` / `"error"`. Its doc comment gives the reason it exists:
the JSON contract must not depend on `Debug` formatting.

### 2.4 `domain.rs` — workspace and output identity

| Item | Kind | Meaning |
|---|---|---|
| `WorkspaceId(pub PathBuf)` | newtype | Stable identity: the canonical workspace root. Derives `Hash`/`Ord` so it can key maps and sort (`domain.rs:94`). |
| `WorkspaceMember` | struct | `manifest_path` + `source_root` of one member (`domain.rs:97-100`). |
| `WorkspacePackage` | struct | Package identity for selector validation; doc comment forbids resolving specs by filesystem substring or target-dir inference (`domain.rs:102-110`). |
| `CargoBuildDirCapability` | enum | `Unavailable` / `Equal` / `Distinct` / `Unknown` (`domain.rs:113-122`). |
| `CargoCapabilities` | struct | `build_dir` + evidence flags `metadata_had_build_directory`, `env_build_dir_set` (`domain.rs:125-129`). |
| `CargoCleanCapabilities` | struct | `profile_selector`, `package_selector`, both defaulting `false` (`domain.rs:132-135`). |
| `OutputRootKind` | enum | `Target` or `Build` (`domain.rs:138-141`). |
| `OutputRoot` | struct | One probed output root: `kind`, `logical_path`, `physical_path: Option`, `exists`, `is_symlink` (`domain.rs:144-150`). |
| `OutputSet` | struct | Exactly the pair `target` + `build` (`domain.rs:153-156`). |
| `ResolvedWorkspace` | struct | A workspace proven by successful `cargo metadata`: id, root, root manifest, members, packages, `OutputSet`, `CargoCapabilities` (`domain.rs:159-167`). |

`CargoBuildDirCapability` variants carry their justification in doc comments
(`domain.rs:114-121`): `Unavailable` = pre-1.91 metadata with no
`build_directory` and no env evidence; `Equal` = modern Cargo reports
target == build; `Distinct` = reports them separately; `Unknown` = malformed,
unknown or env-only evidence, handled conservatively. Derivation:
`workspace.rs:130-136`.

### 2.5 `domain.rs` — coverage ("we could not prove everything")

| Item | Kind | Meaning |
|---|---|---|
| `UnresolvedOwnershipParticipant` | struct | `manifest: PathBuf`, `stage: &'static str`, `reason: String` — a discovered manifest no successful Cargo response covered (`domain.rs:171-175`). `stage` ∈ `"locate"` / `"metadata"` / `"identity"` (`workspace.rs:465-469`). |
| `ResolutionCoverage` | struct | `workspaces` + `unresolved` + `discovered_manifest_count: usize` (`domain.rs:179-183`). `Default`-able, so an empty coverage is constructible. |

### 2.6 `domain.rs` — ownership, report, counters

| Item | Kind | Meaning |
|---|---|---|
| `OutputOwnershipClass` | enum | `PrivateBounded` / `ExternalUnproven` / `Shared` / `Uncertain` (`domain.rs:186-191`). |
| `PhysicalOutputGroup` | struct | A measured group, pre-report: `covering_roots`, `physical_paths`, `display_path`, `owners: Vec<WorkspaceId>`, `ownership`, `bytes`, `metric`, `newest_mtime`, `artifact_entries`, `uncertain` (`domain.rs:205-219`). |
| `EligibleOutputGroup` | struct | The report projection: same identity/size fields, but `workspace_roots: Vec<PathBuf>` instead of `owners`, and **no `covering_roots`, no `uncertain`** (`domain.rs:222-231`). |
| `ScanReport` | struct | `groups: Vec<EligibleOutputGroup>`, `diagnostics`, `discovered: u64`, `visited_entries: u64`, `counters: ScanCounters` (`domain.rs:84-90`). Rendered by mutable reference (`report.rs:44`). |
| `ScanCounters` | struct | 34 `u64` fields spanning prune attribution, Cargo call counts, gate exits, per-phase timings (`domain.rs:238-277`). |
| `elapsed_nanos(Duration) -> u64` | free fn | Saturating `Duration` → nanosecond conversion (`domain.rs:284-286`). |

`impl` blocks in `domain.rs`:

| Method | Signature | Notes |
|---|---|---|
| `DiagnosticSeverity::as_str` | `self -> &'static str` | `domain.rs:59`. Used at `output.rs:124`. |
| `OutputOwnershipClass::label` | `self -> &'static str` | `domain.rs:194`: `private` / `external-unproven` / `shared` / `uncertain`. Used at `report.rs:60`, `cleanup.rs:294`, `cleanup.rs:623`, `cleanup.rs:1944`, `output.rs:117`, `output.rs:170`. |
| `ScanCounters::stats_line` | `&self -> String` | `domain.rs:290`. The `key=value` scan line. It *fuses* two fields into one key: `empty_skipped` = `empty_no_output_skipped + missing_output_skipped` (`domain.rs:306`). |
| `ScanCounters::timings_line` | `&self -> String` | `domain.rs:317`. Five phases as `ms`, 2 decimals. |
| `ScanCounters::merge_proof` | `&mut self, proof: &ScanCounters` | `domain.rs:339`. Six `saturating_add`s into `proof_*` fields only. |
| `ScanCounters::proof_stats_line` | `&self -> String` | `domain.rs:361`. Adds `proof_workspaces_refreshed` and `proof_metadata_peak`. |
| `ScanCounters::proof_timings_line` | `&self -> String` | `domain.rs:372`. Four proof phases as `ms`. |

### 2.7 `error.rs` — `AppError`

`#[derive(Debug, Error)]` from `thiserror` (`error.rs:2-16`).

| Variant | Fields | `Display` | Constructed at |
|---|---|---|---|
| `Config` | `String` | `configuration error: {0}` | `config.rs` ×15, `main.rs` ×9, `cleanup.rs` ×4, `discovery.rs` ×2 |
| `InvalidRoot` | `{ path: String, reason: String }` | `invalid scan root {path}: {reason}` | `cleanup.rs` ×4, `policy.rs` ×2 |
| `Io` | `#[from] std::io::Error` | `I/O error: {0}` | `config.rs` ×2 (via `?`) |
| `Scan` | `String` | `scanner failed: {0}` | **never constructed** |
| `Provenance` | `String` | `self-update refused: {0}` | `update.rs` ×1 |
| `Update` | `String` | `self-update failed: {0}` | `update.rs` ×1 |

`Io` is the only variant with `#[from]`, so it is the only one that
`?`-converts implicitly from `std::io::Error`. `AppError` implements no other
trait the crate relies on — notably no `Clone`/`PartialEq` — so it cannot be
embedded in a comparable struct or asserted on directly. Text is printed by
`main.rs:10-12`, prefixed `cargo-cleanme: `, mapped to exit code `2`.

### 2.8 `lib.rs` — the module manifest

Fifteen `pub mod` declarations, one per line, alphabetical (`lib.rs:1-15`):
`cleanup`, `cli`, `config`, `discovery`, `discovery_state`, `domain`, `editor`,
`error`, `output`, `policy`, `progress`, `report`, `traverse`, `update`,
`workspace`. Implications:

- **Everything is public API.** There is no `pub(crate)`, no private module and
  no re-export layer, so every type in §2 is part of the library's surface
  whether or not it is meaningfully consumable. (The
  [architecture overview](overview.md) §3 counts 17 modules including the
  binary `main.rs`; `lib.rs` itself declares 15.)
- **`main.rs` is not a 16th module.** It is the binary crate root and reaches
  the library via the external-crate path `cargo_cleanme::domain::…`, which is
  why the same symbol appears in `src/` greps both as `domain::X` and
  `cargo_cleanme::domain::X`.
- **Adding a module is a one-line change here**, but every `pub mod` also
  implicitly promises that module's types to downstream users.

---

## 3. Core types

### 3.1 `OutputOwnershipClass` and `OutputRootKind`

`OutputOwnershipClass` is the crate's central safety token
(`domain.rs:186-191`); its meaning is fixed by the classifier at
`workspace.rs:800-825`.

| Variant | Label | Assigned when | Permitted at cleanup time |
|---|---|---|---|
| `PrivateBounded` | `private` | Single owner, every covering root strictly below the workspace root, no source overlap, and *no* unresolved workspace anywhere in the universe (`workspace.rs:816-818`) | **The only actionable class** — still subject to path authorization and a fresh ownership proof at the spawn boundary. |
| `Shared` | `shared` | More than one owner maps to the group (`workspace.rs:801`), *or* it is inside the root but not provably exclusive because some root in the universe is unrepresentable (`workspace.rs:820-822`) | Inventory-only: `cleanup.rs:1193` returns `CleanupReasonCode::SkippedShared`. |
| `ExternalUnproven` | `external-unproven` | Inside no single workspace root — e.g. a `CARGO_TARGET_DIR` outside the project (`workspace.rs:823-824`) | Inventory-only *even inside a clean ROOT or a configured `allowed_output_roots`*: `cleanup.rs:1195`, plus the gate at `cleanup.rs:493-494` which fires before any root validation. |
| `Uncertain` | `uncertain` | The owning workspace is itself uncertain, or a covering root *is or contains* a member source root — cleaning would delete the project (`workspace.rs:813-815`) | Never. Dropped before sizing with an explicit `CandidateUncertain` warning (`workspace.rs:1053-1071`). |

The `Uncertain` source-overlap case deserves attention: a user who sets
`target-dir` to their source tree would otherwise get silence, so the group is
dropped with a warning explaining why nothing is reported or cleaned.

The *order* of checks in `covering_is_authorized_roots` (`cleanup.rs:485-500`) is
load-bearing: ownership class is tested **first**, before allowed roots are
validated as absolute and non-symlink. Its comment states the rule — ownership
class is authoritative, and `ExternalUnproven` is inventory-only even inside a
clean ROOT or a configured allowed root. That is what makes
`allowed_output_roots` unable to upgrade an unproven class. Three more gates
repeat the test: `cleanup.rs:618` (per group), `cleanup.rs:1940` (per unit), and
`unit_blocking_class` (`cleanup.rs:1250-1256`), which reduces a workspace unit
to its worst member's class.

`OutputRootKind` (`domain.rs:138-141`) is the smaller, purely descriptive
sibling: `Target` and `Build`. Since Cargo 1.91 these can be distinct
directories, and one `cargo clean` can affect both, so the kind is carried on
every `OutputRoot` and ordered `Target` before `Build` in unit construction
(`workspace.rs:1307-1308`). Four exhaustive matches over it live outside
`domain.rs`: `workspace.rs:208`, `workspace.rs:238`, `cleanup.rs:610`, and the
label builder at `workspace.rs:1313-1315` whose `[Target, Build]` arm renders
`"target+build"`.

### 3.2 `OutputRoot`, `OutputSet`, `ResolvedWorkspace`

`OutputRoot` is the *probed* form of one output directory. The distinction
between `logical_path` and `physical_path` is what makes symlinked output roots
safe: grouping runs over canonical physical paths, so two logical roots pointing
at the same bytes collapse into one group instead of being counted twice.
`exists: false` is not an error state — it is the normal answer for a never-built
project, and also for a filesystem root (refused with a `CandidateUncertain`
warning, `workspace.rs:199-217`) and for an unreadable directory, where the code
insists that *unreadable is not absent* (`workspace.rs:227`).

`OutputSet` is simply the mandatory pair. `ResolvedWorkspace` is the proof
object: a workspace plus everything needed to reason about its output without
touching disk again. It only exists if a `cargo metadata` call succeeded;
manifests that did not produce one are represented by
`UnresolvedOwnershipParticipant` instead.

### 3.3 `PhysicalOutputGroup` vs `EligibleOutputGroup`

Same idea at two stages, and the field differences are the point.
`PhysicalOutputGroup` is produced by `workspace::analyze_groups` for every group
that survived the gates, and is the *candidate* form: it carries `covering_roots`
(the minimal outermost canonical roots, used for measurement and per-group
authorization, `domain.rs:206-207`) and an `uncertain: bool`.
`EligibleOutputGroup` is the *report* form, built by the map at `main.rs:474-484`
and the equivalent in `tests/end_to_end.rs:174-183`.

Two asymmetries are worth stating. First, `owners: Vec<WorkspaceId>` becomes
`workspace_roots: Vec<PathBuf>`; the `main.rs` map is
`g.owners.iter().map(|w| w.0.clone())`, the only place `WorkspaceId`'s field is
projected. Second, `covering_roots` and `uncertain` are **dropped**. Dropping
`covering_roots` is consistent with keeping authorization a cleanup-side concern
(`cleanup.rs:462`); dropping `uncertain` is sound only because a group is never
constructed as uncertain (§5.6).

"Eligible" means *included in the inventory*, not eligible for deletion. An
`EligibleOutputGroup` can carry `ownership: ExternalUnproven`; only
`PrivateBounded` is actionable, and even then only after proof.

### 3.4 The input half: `ScanRequest` → `EffectiveScanPolicy`

`ScanRequest` is the smallest possible encoding of user intent — a root and a
`full` flag — and deliberately contains no recency window, no scope and no
filters. `policy::resolve` is its only real consumer, converting intent into
three decisions: scope, filters, recency (`policy.rs:23-47`). The visible design
rule is that *explicit scope bypasses filtering*: `policy.rs:42` pairs
`ScanScope::Explicit` with `DiscoveryFilters::Bypassed` — if the user named the
directory, cargo-cleanme does not second-guess them with ignore rules.

`cleanup.rs` constructs `EffectiveScanPolicy` directly, twice
(`cleanup.rs:772-776`, `cleanup.rs:1604-1608`), always as
`ScanScope::ExplicitRoots(roots)` with `DiscoveryFilters::Bypassed`. That is why
`ExplicitRoots` is documented as cleanup-only: a scan is driven by
`policy::resolve` and would never produce it. `Bypassed` is the safe default for
internal callers — `discovery.rs:327-328` destructures it to empty slices.

### 3.5 The output half: `DiscoveredProject`, `ActivityState`, `SizeMetric`

`DiscoveredProject` and `ActivityState` are the two types in this file the rest
of the crate does not use. Discovery actually returns `Vec<PathBuf>` manifests
plus a parallel `Vec<ScanDiagnostic>`; the manifest→project structuring never
happened, or was superseded. `ActivityState` is the same story: recency is
enforced as a cutoff comparison at `workspace.rs:1157-1170` and surfaced as
`GroupSkipReason::ActiveOutput` plus `counters.active_skipped`, never as a value
on a group.

`SizeMetric` is load-bearing in the output contract. `output.rs:112-115` maps it
exhaustively to `"allocated"` / `"apparent"`, and `tests/end_to_end.rs:203-209`
asserts the *allocated* metric by checking the byte figure is block-rounded
(`bytes >= 8192`) rather than assuming an exact filesystem value. The field
exists because allocated and apparent sizes differ by orders of magnitude on
sparse files; a report that did not say which it meant would be meaningless.

### 3.6 `ScanDiagnostic` and the uncertainty channel

A `ScanDiagnostic` is the answer to "I could not be sure, and here is where".
`path: Option<PathBuf>` is optional because some uncertainties are about a whole
scan rather than a location. Consumers treat diagnostics four different ways,
which is why `category` is a closed enum:

1. **Rendering** — every diagnostic reaches the human report and `DiagnosticV1`
   (`output.rs:121-134`).
2. **Uncertainty extraction** — `main.rs:328-339` filters *exactly*
   `PermissionDenied | Metadata | Vanished` and maps them to a `Vec<PathBuf>`
   via `filter_map` on `d.path.clone()`, so a diagnostic with `path: None`
   contributes nothing. This vector decides whether the learned-root state file
   may be trusted and reconciled.
3. **Scan incompleteness** — a `PlatformRoot` diagnostic at `Error` severity
   makes a Full scan `full_incomplete` (`main.rs:384-386`, `main.rs:466-469`),
   suppressing state-destroying reconciliation.
4. **Durable uncertainty** — the `Uncertain` ownership class exists so
   candidate-level uncertainty becomes a *property attached to a group*, not
   just a transient log line.

`main.rs:328-339` is the single most important thing to know about this enum, and
it is a `matches!` listing three of six variants. It compiles whether or not a
fourth exists. That asymmetry — compile-checked in `output.rs:126-131`, silently
exhaustive in `main.rs` — is review item 1 in §7.

### 3.7 `ScanCounters` — scan counters vs proof counters

34 `u64` fields in five families:

| Family | Fields | Purpose |
|---|---|---|
| Prune attribution | `directories_visited`, `directories_pruned`, `platform_system_prunes`, `cargo_home_prunes`, `rustup_home_prunes`, `target_vcs_prunes`, `user_ignore_prunes` | "Why was this subtree not searched?" — the fail-fast instrumentation named in the struct doc (`domain.rs:233-236`). |
| Cargo call counts | `cargo_locate_calls`, `cargo_metadata_calls`, `cargo_failures`, `unique_workspaces`, `manifests_found` | Cost and success of the subprocess boundary. |
| Ownership honesty | `unresolved_ownership` | Discovered cleanup-scope manifests *not* authoritatively covered by Cargo metadata. Its doc comment insists this is kept separate from subprocess failures (`domain.rs:251-253`): a manifest no Cargo response covered is a different event from a Cargo call that failed. |
| Gate exits | `empty_no_output_skipped`, `missing_output_skipped`, `active_skipped`, `uncertain_skipped`, `groups_measured`, `bytes_measured`, `reportable_groups`, `deduped_workspace_hits` | Where candidates died. |
| Timings (ns) | `discovery_nanos`, `cargo_locate_nanos`, `cargo_metadata_nanos`, `source_activity_nanos`, `output_sizing_nanos` | Wall time per phase, as `u64` specifically to preserve `Eq` (`domain.rs:235-236`). |

The **`proof_*` fields are a separate accounting universe** — a deliberate
decision, not duplication. The proof pass re-resolves the ownership universe at
the destructive boundary (`cleanup.rs:1702` and `cleanup.rs:1829` call
`merge_proof`), and that work is real cost. The `proof_*` doc comment gives the
reason: proof work is kept separate from initial discovery counters "so
`--stats` cannot hide it" (`domain.rs:267-268`). Accordingly `main.rs:250-255`
prints **four** lines for `cleanup --stats` — `stats_line`, `timings_line`,
`proof_stats_line`, `proof_timings_line` — while `main.rs:545-549` prints two
for a plain scan.

`merge_proof` is one-directional: six specific fields are copied into `proof_*`
and **no** base counter is ever touched. Two `proof_*` fields are not merged
because they are not additive — `proof_workspaces_refreshed` is incremented at
`cleanup.rs:1704` and `cleanup.rs:1830`, and `proof_metadata_peak_concurrency`
is a `.max()` (`cleanup.rs:1730`, `cleanup.rs:1736`). `proof_workspaces_refreshed`
counts *workspaces*, not processes, because the universe is refreshed once per
run and each candidate re-resolves its own workspace (`domain.rs:335-338`).

### 3.8 `UnresolvedOwnershipParticipant` and `ResolutionCoverage`

These are the crate's admission that a scan can be incomplete. A participant
records one discovered manifest for which no successful `cargo metadata`
response gave authoritative coverage, plus the `stage` at which resolution gave
up and a human-readable `reason`. `stage` is derived by substring-matching the
diagnostic message (`workspace.rs:465-469`) — brittle, and worth remembering
when reviewing. `reason` is a copy of that message, or the literal
`"no successful Cargo workspace metadata covered this manifest"`.

They drive the most fail-closed behaviour in the crate. `cleanup.rs:1289-1309`
calls `resolve_workspaces_with_coverage`, sets
`counters.unresolved_ownership = unresolved.len() as u64`, and — crucially — if
`unresolved` is non-empty returns a `CleanupScope` with **zero units**: a
whole-scope safety block raised *before* any sizing or per-unit proof. One
manifest Cargo could not account for stops the entire run. The structured
participants are retained on `CleanReport.unresolved_ownership`
(`cleanup.rs:186`) for audit and projected to JSON as `unresolved_participants`
(`output.rs:65-69`).

---

## 4. How the pieces are used by the rest of the crate

From `grep -rho "domain::[A-Za-z_]*" src/`: **37 qualified references** plus
**5 import sites**. Per file: `main.rs` 14, `output.rs` 9, `cleanup.rs` 8,
`workspace.rs` 3, `report.rs` 1, `policy.rs` 1, `discovery.rs` 1. Four files
import by glob (`cleanup.rs:16`, `discovery.rs:2`, `report.rs:1`,
`workspace.rs:9`), one by explicit list (`policy.rs:3`); everything else
qualifies the path inline.

| Type | Qualified `domain::` refs | Consuming modules |
|---|---:|---|
| `DiagnosticCategory` | 11 | `main.rs`, `output.rs`, `workspace.rs`, `discovery.rs` (glob) |
| `elapsed_nanos` | 6 | `main.rs`, `cleanup.rs`, `workspace.rs` |
| `ScanRequest` | 2 | `policy.rs`, `main.rs`, `tests/end_to_end.rs` |
| `ScanReport` | 2 | `main.rs`, `output.rs`, `tests/end_to_end.rs` |
| `EffectiveScanPolicy` | 2 | `policy.rs`, `cleanup.rs` |
| `DiscoveryFilters` | 2 | `policy.rs`, `cleanup.rs` |
| `SizeMetric` | 2 | `output.rs`, `workspace.rs`, `report.rs`, `cleanup.rs` (all via glob) |
| `DiagnosticSeverity` | 2 | `main.rs`, `workspace.rs`, `discovery.rs` (glob); also used *unnamed* by `output.rs:124` via `as_str()` |
| `ScanScope` | 1 | `policy.rs`, `main.rs`, `cleanup.rs`, `discovery.rs` (glob) |
| `ScanDiagnostic` | 1 | `main.rs`, `workspace.rs`, `discovery.rs`, `cleanup.rs` (glob) |
| `EligibleOutputGroup` | 1 | `main.rs`, `report.rs` (glob), `tests/end_to_end.rs` |
| `OutputOwnershipClass` | 0 (glob) | `workspace.rs`, `cleanup.rs`, `report.rs`, `tests/end_to_end.rs` |
| `PhysicalOutputGroup` | 0 (glob) | `workspace.rs`, `cleanup.rs` |
| `OutputRoot`, `OutputSet`, `ResolvedWorkspace`, `WorkspaceId`, `WorkspaceMember`, `WorkspacePackage` | 0 (glob) | `workspace.rs`, `cleanup.rs` |
| `CargoBuildDirCapability`, `CargoCapabilities`, `CargoCleanCapabilities` | 0 (glob) | `workspace.rs`, `cleanup.rs` |
| `OutputRootKind` | 0 (glob) | `workspace.rs`, `cleanup.rs` |
| `UnresolvedOwnershipParticipant`, `ResolutionCoverage` | 0 (glob) | `workspace.rs`, `cleanup.rs` |
| `ScanCounters` | 0 (glob) | `workspace.rs`, `cleanup.rs`, `discovery.rs` (glob) |
| `DiscoveredProject`, `ActivityState` | 0 | **none** — no consumer in `src/` or `tests/` |

`AppError` is referenced 40 times across six files: `config.rs` 17, `main.rs` 9,
`cleanup.rs` 8, `discovery.rs` 2, `policy.rs` 2, `update.rs` 2. `workspace.rs`
and `cli.rs` never construct it.

**No type is used only from tests.** The four test-referenced types
(`ScanRequest`, `ScanReport`, `EligibleOutputGroup`, `OutputOwnershipClass`) are
all used in production too; `tests/end_to_end.rs` names them because it drives
the library directly rather than the binary.

---

## 5. Invariants and edge cases

**5.1 The pure-data rule.** No type in `domain.rs` may acquire behaviour that
touches the filesystem, spawns `cargo`, or makes a policy decision. The moment a
`domain` method needed to know about Cargo versions it would stop being a leaf,
and `CargoCapabilities` would become a place where safety logic hides. All
domain types derive `Clone + Debug`, and all but the `Copy` enums derive
`Eq + PartialEq` — which is what lets `assert_eq!` be the primary testing
technique of every other module.

**5.2 Exhaustiveness is the compiler's job — except where it is not.** Verified
match sites outside `domain.rs`:

| Adding a variant to | Consequence |
|---|---|
| `DiagnosticCategory` | `output.rs:126-131` has no `_` arm → **compile error** |
| `OutputOwnershipClass` | `cleanup.rs:566-574` and `cleanup.rs:1192-1199` both list all four → **compile error** |
| `OutputRootKind` | `workspace.rs:208-210`, `workspace.rs:238-240`, `workspace.rs:1313-1315`, `cleanup.rs:610-612` → **compile error** |
| `SizeMetric` | `output.rs:112-115` → **compile error** |
| `DiagnosticSeverity` | `as_str()` at `domain.rs:60-64` is exhaustive → **compile error** |
| **`DiagnosticCategory` at `main.rs:333-336`** | **nothing — compiles silently.** A new uncertainty class would default to *not* feeding the `uncertainty` vector, so an incomplete scan would be treated as complete. The sharpest edge in this file. |
| **`OutputOwnershipClass` at `cleanup.rs:493`, `cleanup.rs:618`, `cleanup.rs:1252`, `cleanup.rs:1940`** | **nothing — all are `==`/`!=` comparisons.** A new class falls through as "not private", i.e. fails closed. Safe direction, but safe by accident rather than by construction. |

**5.3 `ScanCounters` merge semantics.** `merge_proof` is additive, saturating,
and one-directional into `proof_*`. It is called twice — once for the
whole-universe refresh (`cleanup.rs:1702`), once per candidate
(`cleanup.rs:1829`) — so repeated calls are expected, but it is *not* an
idempotent replace: calling it twice with the same proof double-counts. The doc
claim that proof work "is merged into separate `proof_*` fields (never into the
initial-scan counters)" (`domain.rs:331-333`) is accurate and is the property to
preserve.

**5.4 `elapsed_nanos` and inconsistent siblings.** `elapsed_nanos`
(`domain.rs:284-286`) uses `u64::try_from(...).unwrap_or(u64::MAX)`, with a doc
comment explaining that a `Duration` cannot actually reach u64 nanoseconds
(~584 years) and that this is defensive: a truncating cast on a user-visible
counter is "a silent-wrong-number bug waiting for a slow machine"
(`domain.rs:279-283`).

**That reasoning is not applied consistently elsewhere.** Six call sites use the
helper (`main.rs:326`, `cleanup.rs:791`, `cleanup.rs:2032`, `cleanup.rs:2050`,
`workspace.rs:1003`, `workspace.rs:1092`), but four use a truncating
`elapsed().as_nanos() as u64` on the very same `*_nanos` fields —
`workspace.rs:358`, `workspace.rs:372`, `workspace.rs:518`, `workspace.rs:524`,
feeding `cargo_locate_nanos` / `cargo_metadata_nanos`, which `merge_proof` then
saturating-adds. Treat those four as defects in the making, not as precedent.

**5.5 Saturating vs checked arithmetic.** `saturating_add` appears on 39 lines
in `src/`, 6 of them in `merge_proof` (`domain.rs:342-357`).
`saturating_sub` appears 3 times: `policy.rs:115` and `discovery_state.rs:183`
(both `SystemTime`, where clamping to the epoch is intended) and
`cleanup.rs:1128`, `proof.pre_bytes.saturating_sub(after)` — clamping to `0` when
output *grew* between proof and verification, the only safe reading.
`checked_sub` appears 20 times but only **4** in production code (`main.rs:451`,
`cleanup.rs:1022`, `cleanup.rs:1312`, `cleanup.rs:2002`); the other 16 are
`start.checked_sub(…).unwrap()` inside test modules. The production uses are all
`SystemTime` cutoff computations mapping to `AppError::Config` — see
`main.rs:450-452`, "recency window exceeds system time range". `checked_add`
appears only at `traverse.rs:108`, `117`, `205`, `214`, where overflow is not
silently clamped but sets `stats.uncertain = true` and aborts the walk. Byte
accumulation in hot paths is `saturating_add` (`workspace.rs:1126-1127`,
`workspace.rs:1178`, `report.rs:53`, `output.rs:138`).

**5.6 `PhysicalOutputGroup.uncertain` is structurally always `false`.** The
field exists (`domain.rs:218`) but the single production construction site sets
it to `false` (`workspace.rs:1206`). That is not a bug: the real gate is a
*local* `uncertain` flag computed at `workspace.rs:1119-1134`, which when set
drops the group with an `Error`/`CandidateUncertain` diagnostic
(`workspace.rs:1136-1145`). `EligibleOutputGroup` then has no `uncertain` field
to lose. A reader who assumes the field carries live information will
misunderstand the pipeline.

**5.7 `ScanReport` is mutable and `render` re-sorts it.** `report::render` takes
`&mut ScanReport` and sorts `groups` in place by descending bytes then display
path (`report.rs:44-49`). Rendering is therefore not order-preserving; a caller
needing the pre-sort order must copy first.

**5.8 `ActivityState` and `DiscoveredProject` are dead public API.** Both are
`pub`, reachable from outside the crate, and used by nothing in this repository.
They are a trap: a reader who greps for how inactivity is represented finds
`ActivityState` and concludes it is the mechanism, when the mechanism is a
`clock_cutoff` comparison plus `GroupSkipReason` in `workspace.rs`.

---

## 6. Testing

**`domain.rs` and `error.rs` contain no tests at all.** Verified by grep: zero
`#[test]` attributes and zero `#[cfg(test)]` modules in either file. The crate
has **236** `#[test]` functions in `src/` — `cleanup.rs` 71, `workspace.rs` 44,
`update.rs` 30, `discovery.rs` 18, `cli.rs` 17, `config.rs` 14, `progress.rs` 12,
`discovery_state.rs` 11, `report.rs` 7, `policy.rs` 5, `traverse.rs` 5,
`editor.rs` 2 — and `domain.rs`, `error.rs`, `lib.rs`, `main.rs` and `output.rs`
contribute none.

**The consequence: this layer is covered only indirectly**, by tests that live in
the modules that consume it. That is the right place for them — a test of
`format_bytes` belongs in `report.rs`, not in a data module. Concretely, the only
domain-owned behaviour (`stats_line`, `timings_line`, `proof_stats_line`,
`proof_timings_line`, `merge_proof`, `label`, `as_str`, `elapsed_nanos`) is
asserted in `cleanup.rs:5851-5860`, three lines, one of which only checks that
`proof_timings_line()` contains `proof_metadata=`. That is thin. **Do not
overstate the coverage:** there is no unit test pinning the full `stats_line` key
list, none for `elapsed_nanos` saturation, and none for the
`unresolved_ownership`-vs-`cargo_failures` separation the doc comment at
`domain.rs:251-253` insists on.

The integration surfaces that exercise this vocabulary:

- **`tests/cli_contract.rs`** (658 lines, 8 `#[test]`s) drives the real binary via
  `CARGO_BIN_EXE_cargo-cleanme` and asserts the JSON/stderr contract that
  `output.rs` and the counter lines produce.
  `json_scan_is_one_versioned_document_and_stats_stay_on_stderr`
  (`tests/cli_contract.rs:176-205`) asserts `schema_version == 1`,
  `operation == "scan"`, `scope == "explicit"`, exactly one newline on stdout,
  byte-identical stdout with and without `--stats`, and `"scan stats:"` on
  stderr — a direct end-to-end test of `stats_line` and `timings_line` plus the
  stdout/stderr split. `json_scope_block_is_emitted_with_nonzero_exit_status`
  (`tests/cli_contract.rs:245`) covers the whole-scope block that
  `UnresolvedOwnershipParticipant` feeds.
- **`tests/end_to_end.rs`** (211 lines, 1 `#[test]`,
  `end_to_end_reports_only_inactive_artifact_projects`,
  `tests/end_to_end.rs:113-211`) is the only test that constructs the domain
  types directly. It runs policy → discovery → resolution → grouping → analysis →
  report with a fake `cargo metadata`, then asserts both physical groups are
  `OutputOwnershipClass::PrivateBounded` (`tests/end_to_end.rs:151-156`), that
  only the backdated project is reported, and that the machine projection agrees
  with the human one — including
  `json["result"]["groups"][0]["ownership"] == "private"`
  (`tests/end_to_end.rs:200`), which is a test of `label()`.

---

## 7. Review checklist

1. **Adding a `DiagnosticCategory` variant.** Three things must happen; only one
   is compiler-enforced: fix the exhaustive match at `output.rs:126-131`; decide
   whether it belongs in the uncertainty filter at `main.rs:333-336` (a
   `matches!` that will *not* warn); and decide whether it makes a scan
   incomplete in the `PlatformRoot` + `Error` sense at `main.rs:384-386` /
   `main.rs:466-469`. If it is a new way for the walk to be incomplete and it is
   absent from the `main.rs` filter, an incomplete Full scan is treated as
   complete and may delete learned roots it never observed.
2. **Adding an `OutputOwnershipClass` variant.** It needs a `label()` arm at
   `domain.rs:194-201`, and the exhaustive matches at `cleanup.rs:566-574` and
   `cleanup.rs:1192-1199` will fail to compile — but the four *inequality* gates
   (`cleanup.rs:493`, `cleanup.rs:618`, `cleanup.rs:1252`, `cleanup.rs:1940`)
   will silently accept it as "not private". Confirm the new class fails closed
   in all four, and that `workspace::build_groups` (`workspace.rs:800-825`)
   cannot produce it.
3. **Changing `ScanCounters`.** A new field must be classified: is it in
   `stats_line` or `proof_stats_line`? If it is proof-phase work it must be a
   `proof_*` field merged by `merge_proof` (`domain.rs:339-358`) so
   `cleanup --stats` (`main.rs:250-255`) cannot hide it. If it is a maximum
   rather than a sum (like `proof_metadata_peak_concurrency`, set by `.max()` at
   `cleanup.rs:1730`), it must *not* be added to `merge_proof`.
4. **Any change to `elapsed_nanos` usage.** The helper exists to avoid a silent
   truncation, and four sites still do it by hand (`workspace.rs:358`, `372`,
   `518`, `524`). Do not copy those; if you touch the helper, fix those four in
   the same change.
5. **Is `merge_proof` still one-directional and non-idempotent-safe?** Any edit
   that lets proof work land in a base counter silently re-breaks the property
   stated at `domain.rs:331-333`. Also confirm the repeated calls at
   `cleanup.rs:1702` and `cleanup.rs:1829` remain intentional.
6. **Does a change to `ScanReport` / `EligibleOutputGroup` reach the JSON
   contract?** Neither type is `Serialize`. A new field changes nothing on the
   wire until `output.rs` maps it (`output.rs:104-154`), and that mapping is the
   only place a stable name can be given. Separately, `visited_entries` is
   written at `main.rs:487` and read nowhere — if you add a report field, decide
   who reads it.
7. **Is `PhysicalOutputGroup.uncertain` still meaningful?** It is `false` at the
   only construction site (`workspace.rs:1206`) because uncertainty is decided by
   a local flag at `workspace.rs:1119-1134`. If a new path can construct a group
   without passing that gate, the field becomes load-bearing and
   `EligibleOutputGroup`'s lack of it becomes real information loss.
8. **Adding a module to `lib.rs` is not free.** Every `pub mod` is public API
   (`lib.rs:1-15`); there is no `pub(crate)` anywhere, so a new module's types
   join the library contract immediately. If the module is internal, consider
   whether it belongs under an existing public module instead.
9. **Adding an `AppError` variant.** Check whether it needs `#[from]`
   (`error.rs:9` is the only conversion), and remember `AppError` is neither
   `Clone` nor `PartialEq`, so it cannot be compared or stored in a test
   fixture. `Scan` is currently never constructed (`error.rs:11`) — if a new
   error is genuinely a scan failure, reuse that variant rather than adding a
   seventh.
10. **Two dead types are still public.** `DiscoveredProject` (`domain.rs:34`) and
    `ActivityState` (`domain.rs:40`) have no consumer in `src/` or `tests/`. A
    change that starts using them is a behaviour change, not a refactor; a
    change that does not is a chance to remove them from the public surface
    deliberately rather than leaving them as misleading vocabulary.
