# Cleanup — authorization, ownership proof, and the three modes

> Component deep dive · part of the [architecture overview](overview.md)

Module under review: `src/cleanup.rs`, 11938 lines (4471 production, 7467 lines
of test code, 136 `#[test]` functions). It is the largest module in the crate
and the densest concentration of safety logic. Every other module decides *what
to look at*; this one decides *what may be destroyed*, proves the decision
still holds, and only then acts.

---

## 1. Responsibility

`cleanup.rs` owns the entire destructive path: mode selection semantics
(`CleanMode` `cleanup.rs:36`, mapped by `cli::clean_mode` at `cli.rs:435`), the
authorization gate (`covering_is_authorized_roots` `cleanup.rs:1194`), the unit-wide
destructive gate (`unit_block_reason_roots` `cleanup.rs:1433`), the final ownership proof
(`final_cleanup_proof_roots` `cleanup.rs:3689`), the frozen Cargo invocation
(`frozen_env` `cleanup.rs:4183`, `clean_args` `cleanup.rs:4237`), the spawn itself
(`cleanup.rs:2422`), post-clean accounting (`measure_union` `cleanup.rs:4452`), and the
human report (`CleanReport::render` `cleanup.rs:720`).

It explicitly does **not** own:

- **Activity measurement.** `traverse::workspace_member_activity`
  `traverse.rs:624`, `traverse::measure_single_target` `traverse.rs:305`. This
  module consumes timestamps; it never turns them into eligibility. See
  [Traverse](08-traverse.md).
- **Grouping and classification.** Every physical-group decision is delegated to
  `workspace.rs`: `resolve_workspaces_with_coverage` `workspace.rs:798`,
  `build_groups` `workspace.rs:1537`, `analyze_groups_detailed`
  `workspace.rs:1805`, `build_cleanup_units` `workspace.rs:2321`,
  `refresh_workspace_from_root_manifest` `workspace.rs:59`,
  `map_workspace_groups` `workspace.rs:2227`, `outermost` `workspace.rs:1460`,
  `workspace_member_roots` `workspace.rs:1751`.
- **Manifest discovery** — `discovery::discover_manifests`, called at `:1695`,
  `:2030`, and `:3428`.
- **JSON rendering** — `output::cleanup` `output.rs:161` maps `CleanReport` to
  `CleanupV1` `output.rs:49`.
- **Progress presentation** — emits `ProgressObserver` events
  (`progress.rs:39`).

Actual crate imports (`cleanup.rs:15-21`): `discovery`, `domain::*`,
`error::AppError`, `progress::{ProgressObserver, ScanPhase}`, `traverse`,
`workspace::{self, CargoRunner}`. Additional fully-qualified uses:
`crate::report::format_bytes` (`:818`, `:915`, `:924`, `:1024`, `:1032`, `:1037-1039`),
`crate::domain::{EffectiveScanPolicy, ScanScope, DiscoveryFilters, elapsed_nanos}`
(`:1739-1742`, `:1778`, `:2295-2298`, `:3448-3451`, `:4029`, `:4068`),
`crate::progress::NoopObserver` (`:1542`, `:2302`, `:3356`, `:3453`, `:3488`).
Line `:1077` re-exports
`workspace::{ProcessOutput, SystemCargoRunner}` so tests can inject I/O through
this module's own seam.

**The "one-way edge" is test-only.** `workspace.rs:4846` and `workspace.rs:5125`
call `crate::cleanup::covering_is_authorized`, but both lines sit inside
`workspace.rs`'s own `#[cfg(test)] mod tests`, which begins at
`workspace.rs:2440`. There is no production edge from `workspace.rs` into
`cleanup.rs`.

**Test-only back edges.** `cleanup.rs` reaches back into `cli`, `domain`,
`output`, `policy`, and `progress` — but only from the `#[cfg(test)]` module
that begins at `cleanup.rs:4403`: `crate::cli::clean_mode` (`:4914`, `:4919`,
`:4663`, `:4663`), `crate::domain::OutputRootKind` (`:4663`),
`crate::output::cleanup` (`:6375`, `:6538`),
`crate::policy::{AutomaticOmission, recheck_admission_premise}` (`:7644`,
`:7667`, `:7692`), `crate::progress::{ScanPhase, TestObserver}` (`:9247`,
`:9282`, `:9850`), `crate::progress::NoopObserver` (`:4479`). None of these are
production dependencies of the module, and none should be read as an import
edge.

---

## 2. Public surface

### Modes, outcomes, reporting

| Item | Signature (line) | Contract |
| --- | --- | --- |
| `CleanMode` | `enum { Execute, Simulate, CargoPreview }` `:36` | Which semantics to run. **No `Preview` variant and no `#[default]`** — the doc comment at `:36-46` says "every cleanup entry point states its mode explicitly". |
| `CleanMode::as_str` | `-> &'static str` `:50` | `"preview"`/`"simulate"`/`"execute"`; JSON `mode` field. Note `CargoPreview.as_str()` is `"preview"`: the machine label is stable, the variant name is not. |
| `CleanOutcome` | `enum { Previewed, Simulated, Cleaned, Skipped, Failed }` `:314` | Per-unit terminal state. |
| `CleanOutcome::as_str` | `-> &'static str` `:325` | `"previewed"`/`"simulated"`/`"cleaned"`/`"skipped"`/`"failed"`. Comment `:325-756`: the JSON contract must not depend on `Debug`. |
| `CleanResult` | `struct` `:337-419` | One row per workspace CleanupUnit: `display_path`, `workspace_roots`, `output_roots` (the dedup covering union one invocation can affect), `ownership`, `outcome`, `policy_disposition`, `reason_code`, `before_bytes`, `after_bytes`, `observed_decrease`, `detail`. |
| `CleanReport` | `struct` `:589-627` | Run-level result; field table in §8. |
| `CleanReport::render` | `-> String` `:720` | Human projection. On `scope_blocked` emits the block message, then — only for `OwnershipUnproven` with retained participants — a bounded listing (at most 5 `unresolved [stage] manifest: reason` rows plus an overflow line, `append_unresolved_listing` `:663`) with a `[workspace]`-detach hint for the believes-in-workspace class. |
| `CleanupReasonCode` | 21-variant `enum` `:355` | The machine-facing reason vocabulary, tabled below. |
| `CleanupReasonCode::as_str` | `-> &'static str` `:417` | Stable snake_case, consumed at `output.rs:176`. |
| `ProofFailure` | `struct { code, detail }` `:391` | A failed final proof. |
| `ProofFailure::new` | constructor `:416` | Proof gates assign an explicit code at the failure site; new gates must use this. |
| `From<String>` / `From<&str>` | `:435` / `:449` | Legacy fallback retained for external callers only. It derives `code` via `proof_skip_code`. Proof gates must not rely on it. |
| `PolicyDisposition` | 6-variant `enum` `:291` | `#[serde(rename_all="snake_case")]`; mapped at `output.rs:249-260`. |
| `ScopeBlockReason` | 2-variant `enum` `:476-490` | `IncompleteDiscovery` or `OwnershipUnproven`; the only whole-scope reason taxonomy in the crate (`:482-488`). `as_str` `:50`. |
| `ScopeBlock` | `struct { reason, message }` `:524` | A typed whole-scope block plus the prose a human reads; `new` `:530`. |

Every `CleanupReasonCode` variant and wire name (`:355-395`, `as_str` `:456-481`):

| Variant | `as_str()` | Meaning |
| --- | --- | --- |
| `Previewed` | `previewed` | `cargo clean --dry-run` succeeded. |
| `Simulated` | `simulated` | Full proof passed; no `cargo clean` invoked. |
| `Cleaned` | `cleaned` | Real `cargo clean` succeeded; post-clean union measured. |
| `BelowMinimumSize` | `below_minimum_size` | Rejected by `min_reclaimable_bytes`. |
| `TooRecentForPolicy` | `too_recent_for_policy` | Rejected by `min_inactive_seconds`, or the required timestamp was unavailable (`:657-668`). |
| `NotIncluded` | `not_included` | Workspace root failed the `include` globs (`:658`). |
| `Excluded` | `excluded` | Workspace root matched `exclude` globs (`:659`). |
| `SkippedActive` | `skipped_active` | Source or output became/recent active. |
| `SkippedShared` | `skipped_shared` | A group in the unit is `Shared`. |
| `SkippedUncertain` | `skipped_uncertain` | `Uncertain` group, or an output root with unprovable physical identity (`:440`). |
| `SkippedUnauthorized` | `skipped_unauthorized` | Private unit outside the authorized boundary, or the gate fell through with no specific cause (`:441`). An `ExternalUnproven` group reports `SkippedOwnershipUnproven`, not this. |
| `SkippedMarkerInvalid` | `skipped_marker_invalid` | Missing/malformed/unsigned `CACHEDIR.TAG`. |
| `SkippedChangedBeforeCleanup` | `skipped_changed_before_cleanup` | The workspace demonstrably changed between scan and delete. |
| `SkippedOwnershipUnproven` | `skipped_ownership_unproven` | Ownership could not be re-proven *at all* — Cargo refused, discovery incomplete, manifest did not re-resolve. Doc comment `:444-447` insists this is distinct from a real change. |
| `SkippedSafety` | `skipped_safety` | Fail-fast skip that is not active/uncertain, and the `proof_skip_code` catch-all (`:2793`). |
| `SkippedOutsideSelection` | `skipped_outside_selection` | The unit was discovered, resolved, sized and proven, but the operator's selection removed it (M014 anchor, or an interactive selection). Emitted at `:446`. Deliberately not an ownership or safety code. |
| `CargoFailed` | `cargo_failed` | Spawn error (`:448`) or non-zero exit (`:448`). |
| `MeasurementFailed` | `measurement_failed` | Cargo succeeded, post-clean union unmeasurable; outcome stays `Cleaned`, no bytes fabricated (`:310-319`). |
| `SelectorUnsupported` | `selector_unsupported` | Runtime Cargo lacks the selector (`:450`, `:450`), or selector-scoped bytes make a min-size policy unevaluable (`:450`). |
| `SelectorInvalid` | `selector_invalid` | `--package` spec unknown or ambiguous here (`:451-470`). |

**Verified gap:** `cleanup_reason_codes_have_stable_contract_names`
(`:365-388`) asserts 16 of 20 wire names — omitting `SkippedOwnershipUnproven`,
`SkippedOutsideSelection`, `SelectorUnsupported`, `SelectorInvalid`. None of
the four is covered anywhere else in the module's own tests.
`policy_dispositions_have_stable_json_names` (`:4934-4951`) covers 5 of 6
variants, omitting `SelectorEstimateUnavailable` (emitted as
`"selector_estimate_unavailable"` at `output.rs:257`).

### Policy, selection, runner seams, proof, authorization, entry points

| Item | Signature (line) | Contract |
| --- | --- | --- |
| `CleanupPolicy` | `struct { min_reclaimable_bytes: u64, min_inactive_seconds: Option<u64>, include: Vec<String>, exclude: Vec<String> }` `:611-697` | `Default` ⇒ no filtering. §8. |
| `CleanupSelector` | `enum { Profile(String), Package(String) }` `:591` | Narrows a unit's clean. |
| `CleanupSelector::kind` / `::value` | `-> &'static str` `:597` / `-> &str` `:597` | `"profile"`/`"package"`; the user spec. |
| `CleanupSelection` | `enum { All, WorkingDirectory, Ancestry(PathBuf), Roots(Vec<PathBuf>) }` `:85` | Which resolved units are candidates. `admits` `:140`, `resolved` `:172`, `describe` `:223`. §8. |
| `CleanupRunner` | `pub trait CleanupRunner: Sync` `:1089` | The spawn seam: `run` `:1090`, `run_with_env` `:1091-1108`. |
| `SystemCleanupRunner` | `pub struct SystemCleanupRunner;` `:1099` | Real impl; also `impl CargoRunner` `:1099` so one value serves resolution too. |
| `WorkspaceCleanupAdapter` | private `:3106` | Adapts `&dyn CleanupRunner` to `&dyn CargoRunner`. |
| `ProofRefreshMeter` | private `:3309` | Measures concurrent `cargo metadata` activity. |
| `ExecutionProof` | `struct` `:3140` | Frozen pre-spawn state of one unit; 12 fields, §6. |
| `ProofUniverse` | private `:3389` | One hoisted re-resolution of the complete universe. |
| `final_cleanup_proof` | `pub fn (unit, universe, clean_root, allowed_output_roots, runner, recency_seconds, counters) -> Result<ExecutionProof, ProofFailure>` `:3653` | Single-root wrapper, `refresh_candidate=false`. **No production caller** — §7. |
| `execute_pre_spawn_decision` | `pub fn (unit, universe, clean_root, allowed_output_roots, runner, recency_seconds) -> Result<ExecutionProof, ProofFailure>` `:4163` | Same gate set with throwaway counters. **No caller at all** — §7. |
| `is_authorized` | `pub fn (group: &workspace::RawGroup, clean_root: &Path, allowed_roots: &[PathBuf]) -> Result<bool, String>` `:1156` | Wrapper. **Test-only callers** (`:7132`, `:7247`, `:7382`). |
| `covering_is_authorized` | `pub fn (ownership, covering: &[PathBuf], clean_root: &Path, allowed_roots: &[PathBuf]) -> Result<bool, String>` `:1180` | Single-root wrapper. **Test-only** (`workspace.rs:4846`, `workspace.rs:5125`). |
| `covering_is_authorized_roots` | `pub fn (ownership, covering, clean_roots: &[PathBuf], allowed_roots) -> Result<bool, String>` `:1194` | The real gate. Production caller `:1186`. |
| `ownership_skip_detail` | `pub fn (ownership: OutputOwnershipClass) -> String` `:1362` | The "why wasn't this cleaned" sentence. |
| `unit_block_reason` | `pub fn (unit: &workspace::CleanupUnit, clean_root: &Path, allowed_output_roots: &[PathBuf]) -> Result<(), BlockReason>` `:1421` | Single-root wrapper. **Test-only** (`:10210`, `:10228`, `:11388`). |
| `list_cleanup_candidates` | `pub fn (roots, recency_seconds, mode, runner, observer, selection) -> Result<CandidateListing, AppError>` `:2664` | The interactive listing. Runs the *same* `discover_and_resolve_universe` pipeline, spawns `cargo metadata` and never `cargo clean`. |
| `CleanupCandidate` | `struct` `:2537` | One offered row; `provably_private` `:2557-2559`. |

Production uses the private multi-root variants `unit_block_reason_roots`
(`:1208`, `:1208`) and `covering_is_authorized_roots` (`:1208`, `:1208`). The
public single-root wrappers are test/consumer surface and carry no production
weight.

**Entry-point delegation chain.**

```text
clean_with_roots_policy_selector   :1108   ← run_engine_guarded (main.rs:682) calls the guarded form
  └─ clean_with_roots_policy_selector_guarded :1288   (premise = None, selection = All for the unguarded path)
  └─ clean_with_roots_policy       :1086   (selector = None)
       └─ clean_with_roots         :1066   (policy = CleanupPolicy::default())
            └─ clean_with          :1047   (roots = [root])
                 └─ clean          :1030   (SystemCleanupRunner + NoopObserver)
```

Each layer supplies only the defaults its caller omitted. `clean` is the only
wrapper injecting a concrete runner and observer. Grepping `src/` finds no
in-crate caller of `clean`, `clean_with`, or `clean_with_roots` other than
tests — `main.rs` goes through `run_engine_guarded` and supplies
`SystemCleanupRunner` itself. The unguarded `clean_with_roots_policy_selector`
delegates with `premise = None` and `selection = CleanupSelection::All`
(`:1371-1372`) and exists so the public root contract stays strict: every
selected root is still validated, whatever the caller proved beforehand.

---

## 3. The runner seams

```rust
// workspace.rs:29-31
pub trait CargoRunner: Sync {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
}

// cleanup.rs:1279-1287
pub trait CleanupRunner: Sync {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
    fn run_with_env(&self, cwd: &Path, args: &[OsString],
                     env: &[(OsString, OsString)]) -> io::Result<ProcessOutput>;
}
```

`CleanupRunner: Sync` is what makes `std::thread::scope` legal in
`refresh_proof_universe_with_limit` (`cleanup.rs:3534-3620`): one `&dyn CleanupRunner` is
shared across up to `PROOF_REFRESH_CONCURRENCY_CAP = 4` threads (`cleanup.rs:3307`).
`CargoRunner` has no such bound because `resolve_workspaces_with_coverage` is
sequential and cached (`workspace.rs:798`).

**What `SystemCleanupRunner` executes.** `run_with_env` (`cleanup.rs:1107-1125`) builds
`Command::new("cargo").args(args).current_dir(cwd)`, applies each `(k, v)` with
`cmd.env(k, v)`, calls `.output()`. Args come from `clean_args` (`cleanup.rs:4237-4263`):

```text
clean [--dry-run --verbose] --offline --locked
      --manifest-path <root_manifest> --target-dir <target>
      [--profile <p> | --package <q>]
```

**`CleanMode::CargoPreview` versus `Execute` is exactly the `--dry-run
--verbose` pair** (`cleanup.rs:4243-4246`); everything else is identical. This is ADR 001
`:206` ("invokes Cargo rather than recursively deleting output") made literal.
The target is pinned twice — `--target-dir` (`cleanup.rs:4252-4253`) and
`CARGO_TARGET_DIR` (`cleanup.rs:4209-4212`) — with the comment at `cleanup.rs:4251` calling the two
forms "equivalent, Cargo-owned".

`frozen_env` (`cleanup.rs:4183-4235`) pins the child's world: `CARGO_TARGET_DIR` always to
the proven target; `CARGO_BUILD_BUILD_DIR` **always** set — to the resolved
build dir when Cargo reported one (`cleanup.rs:4213-4218`), or to the target when it did
not (`cleanup.rs:4229-4232`), the comment at `cleanup.rs:4226-4228` noting that pinning it to the
target asserts exactly the measured world and is a no-op if ignored. Two refusal
branches: a parent `CARGO_BUILD_BUILD_DIR` with an unprovable build dir
(`cleanup.rs:4201-4208`), and a reported separate build dir under
`Unavailable`/`Unknown` capability (`cleanup.rs:4219-4224`).

**Why an adapter.** The two traits do not convert. `workspace.rs` needs a
`&dyn CargoRunner` to resolve; cleanup needs `&dyn CleanupRunner` to run the
clean *with a frozen env*. `CleanupRunner` is the richer trait, so
`WorkspaceCleanupAdapter` (`cleanup.rs:3106-3120`) is the widening direction: it copies
all four `ProcessOutput` fields (`workspace.rs:22-26`) and drops the env, which
resolution calls do not need. Constructed at `cleanup.rs:3025`, `cleanup.rs:3348`, `cleanup.rs:3480`.
Production therefore makes every Cargo call through one injected seam regardless
of phase.

**Testability payoff and its risk.** The 126 tests that need no real cargo binary for
decision logic. Eight fakes are defined in-module: `FakeCleanupRunner` `cleanup.rs:4690`,
`ChangingTargetRunner` `cleanup.rs:7634`, `PerPathFailRunner` `cleanup.rs:7781` (function-local),
`RaceRunner` `cleanup.rs:8679`, `BuildChangingRunner` `cleanup.rs:8935` (function-local),
`StagedCargo` `cleanup.rs:9431`, `ParallelProbeRunner` `cleanup.rs:9697`, `MetadataMutationRunner`
`cleanup.rs:9736`. Ten tests do use `SystemCleanupRunner`, the real Cargo runner:
`real_temp_project_preview_and_execute_with_redirected_target` `cleanup.rs:8144`,
`real_cargo_package_selector_preserves_sibling_workspace_artifacts` `cleanup.rs:8283`, and
`external_outside_root_allowed_via_configured_root` `cleanup.rs:8427` among them.
`scan_analysis_total_is_determinate_through_real_path` `cleanup.rs:9354` exercises the
same analysis pipeline over a real filesystem path, but through `FakeCleanupRunner`.

The risk is the false-green class this project has already been bitten by
([the overview](overview.md) §11 — "*a green test proves only that its own premises hold*").
Concrete instances here: every fake ignores
`--offline --locked`; `StagedCargo`'s clean block deletes only files the test
told it to, so a wrong `--target-dir` would not surface unless a test asserts
the arg vector; `FakeCleanupRunner::unresolved_manifest` models
`locate-project` failure only for a manifest whose last arg canonicalizes to
the configured path; and no fake models a `cargo clean` that actually fails on
a real target tree.

---

## 4. Modes and the clean entry points

| Variant | `as_str()` | CLI | Guarantees | Does not guarantee |
| --- | --- | --- | --- | --- |
| `Execute` | `execute` | **default** (no mode flag) | Every gate through the final proof ran; a real `cargo clean` with a frozen env, then a post-clean union measurement. | — |
| `Simulate` | `simulate` | `--dry-run` (hidden alias `--dryrun`) | Identical gate set; the unit reached the spawn point. | No `cargo clean` of any kind, and no Cargo process for the clean at all. Reported as `Simulated`, never as recovered space (`:327-332`). |
| `CargoPreview` | `preview` | `--cargo-preview` | Every gate through the final proof ran; Cargo itself reported what it *would* remove. | Nothing removed by us — but Cargo *was* spawned. |

The mapping lives in `cli::clean_mode` (`cli.rs:352-364`): `--dry-run` (or the
hidden `--dryrun`) ⇒ `Simulate`; `--cargo-preview` ⇒ `CargoPreview`; no mode
flag ⇒ `Execute`. Clap makes the three flags mutually exclusive
(`cli.rs:151-167`), and `clean_mode` carries a `debug_assert!` that the
conflicting pair never reaches it (`cli.rs:353-356`). The `clean` subcommand
resolves the same way at `cli.rs:400-404`, and a root-level `--dry-run` is
honoured on the bare front door too (`cli.rs:337`), so
`cargo cleanme --dry-run clean ROOT` never silently falls back to Execute.

**`--yes` is now a hidden no-op alias.** It used to be the only way to ask for
Execute; Execute is the default, so the flag carries no intent of its own
(`cli.rs:209-212`, and the comment at `cli.rs:425-428`). The comment there is explicit that
printing nothing about it is what keeps unattended runs quiet.

**Bare `cargo cleanme` is Routine Execute** (ADR 003), with `--dry-run` the
canonical zero-mutation preview. That is a behavioural change from the earlier
design in which the default was a Cargo-spawning preview: a reader who assumes
"the default does not clean" will be wrong.

**Where roots come from** (C028), all funnelled into
`clean_with_roots_policy_selector_guarded` via `run_engine_guarded`
(`main.rs:737`):

| Source | Roots |
| --- | --- |
| Explicit `ROOT` | `cli::absolutize_root` — relative input accepted for ergonomics, absolutized before becoming the safety boundary. Any defect is fatal. |
| `--full` | The in-memory generation the just-finished Full reconciliation proved (`policy::full_cleanup_roots`), so a failed save cannot resurrect stale roots. |
| Maintenance / `--known` | Raw Routine candidates classified `Automatic` before any collapse; a configured `scan.root` takes the strict explicit path instead. |
| None | `AppError` (unknown scope) or a typed `ScopeBlock` (symlink/unreadable automatic root) — never a silent subset. |

`run_engine_guarded` (`main.rs:760-849`) additionally defends the
admission→validation race: a fatal `InvalidRoot` over an automatic scope
triggers one fresh reclassification, and only a now-proven-omittable offender
rebuilds the universe from scratch (`main.rs:788-827`); anything else becomes a
typed block, never a sibling cleanup against a stale proof.
`cleanup.rs:3264` applies `collapse_roots` again and re-validates the collapsed
set at `:1713-1715` with the comment at `:1711-1712` that collapsing only
narrows the set, so survivors still need checking. The roots are validated
*before* any observable work at `:1698-1703`, so a malformed root can neither
spawn a process nor be masked by the runtime capability verdict. An empty admitted set short-circuits in
`run_cleanup` and never reaches this module; a blocked set reports without
traversing anything.

### The admission premise and its late recheck (C028)

`AdmissionPremise` (`cleanup.rs:1645`) carries the safely omitted automatic
roots alongside the admitted set. The engine re-examines the premise with
fresh `symlink_metadata` — no timers, sleeps, or cached metadata — at two
moments: before the first candidate reaches final proof (`cleanup.rs:2445-2449`)
and before every later Cargo spawn (`cleanup.rs:2643-2648`). A violated premise
returns the completed results plus a whole-scope `IncompleteDiscovery` block
instead of spawning. Policy-skipped units never reach proof and need no
guard. `violated_admission_premise_blocks_with_zero_cargo_spawns`
(`cleanup.rs:7619`) proves the block with zero spawns in both directions.

---

## 5. Authorization

Authorization is a *location* gate, kept strictly separate from the *ownership*
gate. `is_authorized`'s doc comment (`:1138-1155`) states the rule set;
`covering_is_authorized_roots` (`:1194-1202`) implements it through
`AuthorizationContext::admits` (`:1257-1287`).

**What makes a location authorized.** (1) Ownership first, as a hard filter
(`:1262-1264`). (2) Each allowed root is validated then canonicalized
(`:1301-1343`): non-absolute → `Err` `:1302-1307`; missing → `continue`,
non-matching but not fatal `:1309-1313`; other fs error → `Err` `:1314-1319`;
symlink → `Err` `:1320-1325`; not a directory → `continue` `:1326-1328`; else
canonicalize and push `:1335-1343`. (3) Clean roots are canonicalized or the
call fails `:1345-1351`. (4) Every covering root must sit inside *some* boundary
— `roots.clean` (`:1269-1274`) or `roots.allowed` (`:1276-1282`), compared with
`==` or `starts_with`; one miss → `Ok(false)` (`:1283`).

**Absolute directory roots, not globs.** `config.toml:32-33` requires absolute
directory roots and invalidates symlink roots. Verified: the absoluteness `Err`
is `:1302-1307`, the symlink `Err` is `:1320-1325`, and there is no glob machinery
anywhere in the function. **Precision worth recording:** the "not globs" rule is
enforced by construction, not validation. A mistyped glob such as `/tmp/*/out`
*is* absolute, so it passes `:1309-1313`; `symlink_metadata` returns `NotFound`,
`:1309-1313` `continue`s, and the entry silently authorizes nothing — outcome
`Ok(false)` with the message at `:1474-1479`, not a configuration error.
Fail-closed but silent.

**The critical property: authorization cannot promote an ownership class.
Definitive.** `covering_is_authorized_roots` returns `Ok(false)` for any class
other than `PrivateBounded` at `:1262-1264`, *before* inspecting a single path.
`allowed_output_roots` is consulted only at `:1276-1282`, unreachable for
`ExternalUnproven`, `Shared`, and `Uncertain`. The same rule is re-asserted at
the unit gate (`:1460`, comment "Ownership class is authoritative; authorization
never promotes") and again in the proof (`:3881-3891`, where a fresh group no
longer `PrivateBounded` fails regardless of location).

**There are five such ownership gates in production, not four:**

| Line | Site | Form |
| --- | --- | --- |
| `:1262` | `AuthorizationContext::admits` | `ownership != PrivateBounded` ⇒ `Ok(false)` |
| `:1461` | `unit_block_reason_roots_with` | `group.ownership != PrivateBounded` ⇒ `Err` |
| `:2557-2558` | `CleanupCandidate::provably_private` | `self.ownership == PrivateBounded` (the only positive form; a presentation hint, not an eligibility verdict) |
| `:2827` | `unit_blocking_class` | `group.ownership != PrivateBounded` ⇒ that class decides the skip |
| `:3881` | `final_cleanup_proof_roots` | `g.ownership != PrivateBounded` ⇒ proof fails |

This matches three independent sources: `config.toml:35-36` ("authorization
never manufactures ownership proof and cannot promote ExternalUnproven to
private (ADR 001, C002)"); the crate-wide invariant in `overview.md`; ADR 001
`:132-133`. Tests close it from both sides:
`external_unproven_outside_root_never_authorized_by_allowed_root` (`:7251`)
asserts `!is_authorized` with and without an allowed root, and
`external_outside_root_allowed_via_configured_root` (`:8427`) runs the *real*
Cargo path and asserts `Skipped` in both cases.

**One naming trap.** The test at `:8214` is named
`external_outside_root_allowed_via_configured_root`, which reads as "an allowed
root makes external output cleanable". Its own assertions say the opposite —
the unit is skipped *because* authorization cannot promote.

**`ownership_skip_detail`** (`:1362-1373`) exists to state both halves of the rule
(doc comment `:1358-1361`): the class is unproven *and* configuration does not fix
it. `Shared` and `Uncertain` → "…remains inventory-only";
`ExternalUnproven` → "external output ownership is unproven; configured
authorization does not establish exclusivity". The `PrivateBounded` arm —
"external output requires explicit cleanup authorization" — is **unreachable in
production**: its only two call sites (`:1467`, `:3888`) sit inside branches
already guarded by `ownership != PrivateBounded` (`:1461`, `:3881`), and its text
is a *location* message that cannot apply to a group that passed the location
check. Harmless, but a trap for a future caller.

**`unit_block_reason`** (`:1421-1431`) is a single-root wrapper; `unit_block_reason_roots`
(`:1433-1442`) runs three ordered checks over one
`CleanupUnit` (`:1445-1497`): (a) `unit.unmapped` non-empty → `Err` naming root
kind and logical path (`:1449-1458`) — any output root with unprovable physical
identity blocks the whole unit because its footprint cannot be bounded; (b) for
every group the invocation can affect, ownership must be `PrivateBounded`
(`:1459-1469`) then `AuthorizationContext::admits` must return `Ok(true)` for that
group alone (`:1470-1484`); (c) every group must have `measured.is_some()`
(`:1485-1494`), otherwise the fail-fast verdict and its `GroupSkipReason::detail()`
(`workspace.rs:1771`) become the reason. The doc comment `:1410-1420` states the
C003 invariant — every affected group qualifies independently, and "No Cargo
process is spawned for any subset of a unit" — which is what stops a
`PrivateBounded` target smuggling an `ExternalUnproven` sibling build group into
one invocation.

**Authorization-infrastructure errors are scope blocks, not per-unit skips.**
The verdict is typed, not parsed out of prose: `BlockReason::is_authorization_infrastructure`
(`:1405-1407`) separates the two, and `ProofFailure::authorization_infrastructure`
(`:426-432`) carries the verdict across the proof boundary. `Ok(false)` — private
but outside the boundary — stays a per-unit
`SkippedUnauthorized` row, so authorization still cannot promote. The
infrastructure path raises `scope_blocked` and exits 1 (`:2128-2135`),
pinned by `authorization_infrastructure_failure_scope_blocks_instead_of_per_unit_skip`
(`:11328`).

**Is `allowed_output_roots` actually redundant?** `config.toml:37-38` claims it
is "currently operationally redundant because PrivateBounded output is normally
workspace-contained." **Assessment: consistent with the code, with a caveat.** The
reason is the classification rule at `workspace.rs:1688-1702`:
`PrivateBounded` means "every covering root **strictly below the workspace
root**"; `ExternalUnproven` means "output **outside workspace root**."
Classification depends on the *workspace root*, not on the clean root or allowed
roots — `build_groups` (`workspace.rs:1537`) takes only `&[ResolvedWorkspace]`. So
a `PrivateBounded` group's covering roots are always beneath the workspace root
and are already authorized by the `canonical_clean` branch at `:1279-1284` whenever
the workspace root is inside a clean root. That is the ordinary case.

The caveat: if `cargo locate-project` resolves a workspace root *above* the clean
root — legal for a member manifest whose workspace root is an ancestor — then
`PrivateBounded` covering roots can fall outside every clean root, and
`allowed_output_roots` becomes the only route to cleanability. The code
anticipates this: the error at `:1474-1479` tells the user to "add a
`cleanup.allowed_output_roots` entry or use a clean ROOT that contains it".
`missing_and_file_allowed_roots_authorize_nothing_but_are_not_fatal` (`:7601`)
covers the *negative* branches of the allowed-root validation, but I found no
end-to-end test of the positive path — a `PrivateBounded` group with covering
roots outside every clean root becoming `Cleaned` once an allowed root is
configured. An under-verified property, not a defect.

**A separate divergence.** ADR 001 `:7` defines `PrivateBounded` as "every
destructive output path is contained by an *authorized cleanup boundary*" — a
predicate over authorization roots. The code defines it over the *workspace root*
(`workspace.rs:1688-1694`). Reconciled by the argument above, but not the same
predicate, and the ADR's phrasing cannot be read literally alongside
`workspace.rs`.

---

## 6. Ownership and the proof universe

### The universe concept

A proof of exclusive ownership is meaningful only relative to a **complete set of
claimants**. If a second workspace under the same root cannot be resolved, you do
not know what it points at, so you do not know the first workspace's output is
really private. That is why the module carries a `ProofUniverse`: it holds the
complete set of output roots in play, and an incomplete universe makes every
proof inside it untrustworthy.

The mechanism flows in through `domain.rs`: `UnresolvedOwnershipParticipant
{ manifest, stage, reason, capability }` (`domain.rs:194-199`) is a discovered
manifest without authoritative Cargo coverage, and `ResolutionCoverage
{ workspaces, unresolved, discovered_manifest_count }` (`domain.rs:203-207`) is
resolution output *plus* explicit accounting of the discovered set, so "I
resolved some of them" cannot be confused with "I resolved all of them".

`cleanup.rs` obtains coverage from `workspace::resolve_workspaces_with_coverage`
(`workspace.rs:798`) and re-derives the guarantee at proof time in `refresh_combined_universe`
(`:3432-3532`). The refresh has four failure shapes: combined-root rediscovery
produced diagnostics; a manifest failed to re-resolve; an initial workspace id is
absent from the fresh set; a per-workspace refresh returned `None`, or a
different id/root. Each is a `ProofFailure` carrying an explicit code — the
`ProofFailure::new` discipline at `:412-422` exists so a prose edit cannot
silently reclassify the machine contract.

Two layers: `refresh_combined_universe` rediscovers *and* re-resolves from the
roots, then calls `refresh_proof_universe` (`:3365-3378`) to refresh each workspace a
second time — catching a change landing between the snapshot and the
per-candidate gate.

### `ProofUniverse` and hoisting

```rust
// cleanup.rs:3389-3392
struct ProofUniverse {
    fresh: Option<Vec<ResolvedWorkspace>>,
    failure: Option<ProofFailure>,
}
```

`get()` (`:3419-3428`) returns the fresh set or the typed reason it could not
be re-proven; the `(None, None)` arm is a defensive "ownership could not be
re-proven: no refreshed universe". Hoisting happens at `:2174-2180` via
`get_or_insert_with`, so it is **lazy**: a run where every unit is skipped by
policy still spawns nothing. That is the mechanism behind the zero-spawn
property, not an accident of ordering.

Hoisting does not weaken the per-candidate guarantee. Each candidate is
*additionally* re-resolved immediately before its own gates
(`refresh_candidate = true` at `:2202`, implemented `:3725-3761`). The comment at
`:3386-3388` states it: "Each candidate still re-resolves its *own* workspace
through Cargo, so a workspace that changed after this snapshot is still caught
before anything is cleaned."

### `ProofRefreshMeter` and the concurrency cap

`PROOF_REFRESH_CONCURRENCY_CAP = 4` (`:3307`). `ProofRefreshMeter`
(`:3309-3342`) counts in-flight `cargo metadata` only — it checks
`args.first() == "metadata"` at `:3317`, and `run_with_env` (`:3334`) passes
straight through un-metered, correctly, since env-carrying calls are the clean
itself. `refresh_proof_universe` (`:3365-3378`) sizes the pool as
`available_parallelism().clamp(1, 4)`. Work runs in batches
(`universe.chunks(concurrency)`); results are joined in input order and sorted
by root, so output is deterministic regardless of completion order. A thread
panic degrades to `(ScanCounters::default(), None)`, treated by the caller as a
re-resolve failure — fail-closed. Peak concurrency lands in
`counters.proof_metadata_peak_concurrency` and reaches the user through the
`--stats` line in `main.rs:605-616`.

This apparatus is an efficiency concern the project tracked explicitly, not a
safety mechanism: it keeps the *cost* of proving linear. Pinned by
`final_proof_cost_is_linear_in_the_number_of_workspaces` (`:10848`), which
asserts the proof spawn count grows linearly, not quadratically.

### `ExecutionProof` field by field

`cleanup.rs:3140-3155`. The doc comment `:3122-3138` is the contract: it "binds
the complete frozen state of ONE workspace CleanupUnit … all re-proven against
the complete bounded ownership universe discovered under clean ROOT", and is
"consumed with no intervening mutation window beyond the spawn itself. This
minimizes (but does not eliminate) TOCTOU races and fails closed."

| Field | Asserts |
| --- | --- |
| `workspace_roots: Vec<PathBuf>` | The unit's workspace roots; set to `vec![unit.root.clone()]` (`:4092`). |
| `workspace_root: PathBuf` | Workspace root; becomes the spawn `cwd` (`:2418`). |
| `root_manifest: PathBuf` | The manifest for `--manifest-path` (`:4249-4250`). |
| `canonical_members: Vec<PathBuf>` | Sorted, deduped canonical member roots. |
| `target: PathBuf` | Proven target; becomes `--target-dir` and `CARGO_TARGET_DIR`. |
| `build: Option<PathBuf>` | Proven distinct build dir (`:4079-4085`), or `None` when build == target (`:4086-4090`). |
| `capability: CargoCapabilities` | `CargoBuildDirCapability` + env evidence; drives `frozen_env`'s refusals. |
| `covering: Vec<PathBuf>` | Complete dedup union of every affected physical covering root. The blast radius. |
| `pre_bytes: u64` | Pre-clean bytes of *that same union* — not a whole-workspace total. |
| `newest_activity: Option<SystemTime>` | Newest activity observed during the proof; feeds `min_inactive_seconds`. |
| `ownership: OutputOwnershipClass` | Hard-coded `PrivateBounded` at `:4111` — a postcondition, not a measurement: the proof only completes for private groups. |
| `frozen_env: Vec<(OsString, OsString)>` | The exact env for the child; the provisional proof carries an empty vec (`:4112`), `frozen_env` is computed from it (`:4114`) and attached (`:4115-4118`). |

`type RevalidatedContext = ExecutionProof` (`:3158`) is a compatibility alias
still used by `frozen_env` and `clean_args`.

### `final_cleanup_proof` — the walkthrough

The doc comment `:3122-3138` enumerates the frozen state. Execution order in
`final_cleanup_proof_roots` (`:3689-4119`):

1. `:3700-3708` — `unit_block_reason_roots` re-run "defensive: the caller gates before
   dispatch, and the final proof must be the single authority consumed by any
   spawn."
2. `:3720` — `hoisted.get()?`. If the universe could not be re-proven, the
   candidate dies with the typed `ProofFailure`.
3. `:3721-3761` — re-resolve *this* candidate, looked up by identity in the
   refreshed universe (`:3726-3734`); a changed identity (`:3742-3750`) and a
   failed re-resolution (`:3751-3759`) both fail.
4. `:3762-3763` — rebuild the complete fresh physical output graph; the
   candidate is then re-located in the fresh universe (`:3765-3776`) and the
   original candidate re-located in the scan-time universe (`:3777-3785`).
5. `:3786-3821` — member set (canonicalized with `canon_strict`, sorted,
   compared `:3816-3821`), package identity set (`:3822-3827`), target physical
   identity (`:3828-3849`), build-dir identity (`:3834-3859`). Comparisons are
   canonicalized so `/tmp` vs `/private/tmp` on macOS does not read as a change
   (`:3716-3719`).
6. `:3860-3862` — remap the fresh OutputSet onto the fresh graph; affected-group
   count must match (`:3863-3868`), every fresh group `PrivateBounded` (`:3881-3891`),
   and `workspace::outermost` + `sorted_same` must reproduce exactly the same
   covering union (`:3894-3900`).
7. `:3901-3947` — no *other* workspace in the fresh universe may overlap the
   candidate's affected output in either direction, and no other workspace's
   *source* tree may lie inside the cleaned region. This covers symlink and
   unresolvable roots the physical graph cannot represent, via
   `provable_output_paths` (`:3292`) and `resolve_as_far_as_possible`
   (`workspace.rs`). Fails closed.
8. `:3981-3983` — every covering root must exist, be a real directory, not be a
   symlink; "preflight changed: disappeared / became symlink / changed type"
   (`prove_covering_roots` `:4128`).
9. `:3986-4065` — the recency window (`:3986-3994`), fresh source activity per
   member (`:4006-4026`), and fresh output activity + sizing per covering root
   (`:4030-4065`). Both `t >= cutoff` and `t > start` are rejected
   (`:4055`, refusing at `:4056-4060`) — a future-dated mtime reads as active, not as old.
10. `:4071` — `require_cargo_markers` (`:4421`): every covering root
    needs a `CACHEDIR.TAG` that is a regular non-symlink file starting with
    Cargo's signature bytes.
11. `:4072-4090` — target/build resolution and the build==target collapse.
12. `:4091-4113` — assemble the provisional proof (`:4091-4113`), compute `frozen_env` (`:4114`), attach (`:4115-4118`).

**Why the work is repeated rather than reusing the scan's result: TOCTOU.** The
initial analysis (`resolve_cleanup_scope:3014`) ran at `scan_start`; the proof
runs immediately before the spawn. In between a workspace can be re-pointed,
gain a member, have its target redirected, gain a second owner, or be touched.
The comment `:3134-3138` is the honest statement of the residual risk: the proof
"was generated immediately before spawn and is consumed here with no
intervening mutation window beyond the spawn itself (fail-closed TOCTOU
minimization, not elimination)."

**On a conflict the proof aborts the candidate. It does not narrow, and it does
not continue.** Every gate above returns `Err(...)`; there is no partial-success
path and no "clean the subset that still looks safe." The rationale is
structural: one `cargo clean` affects the whole covering union, so no narrower
destructive action exists. Confirmed at the call site (`:2204-2224`): an `Err`
becomes a `Skipped` row via `failure.code` and the loop `continue`s.

### How a proof failure becomes observable

`ProofFailure { code, detail }` → the `skipped` closure `:2007-2024` → a
`CleanResult { outcome: Skipped, reason_code: failure.code, before_bytes:
Some(unit.bytes), detail: failure.detail }`. `code` is assigned explicitly at
each proof gate via `ProofFailure::new`, so rewording an error string cannot
reclassify a machine-facing reason code; the unit-wide gate derives its code
from the unit itself (`unit_skip_code` `:2758`). Only the Cargo-refresh boundary
still derives `code` from text through `proof_skip_code` (`:2793-2819`), a
substring matcher kept as a fallback for pass-through Cargo/discovery strings
with no typed site. The comment at `:2806-2808` records the L9 correction:
"changed identity", "vanished", "disappeared" classify as *change*, while a
genuine Cargo/discovery failure classifies as *unproven*. The test
`proof_failures_distinguish_a_change_from_an_unproven_ownership` (`:10884-11041`)
pins those mappings.

---

## 7. The pre-spawn decision

`execute_pre_spawn_decision` (`:4163-4178`) is a thin wrapper: allocate
`ScanCounters::default()`, call `final_cleanup_proof`. Its doc comment
(`:3914-3921`) says it exposes "the same non-mutating gate set Execute runs
before spawn so tests can compare candidate dispositions with Simulate on
identical state without invoking Cargo clean."

**Verified finding: it has no callers.** `grep -rn 'execute_pre_spawn_decision'
src/` returns exactly one hit — its own definition at `:3922`. Not `main.rs`,
not `lib.rs`, not the test module. The public `final_cleanup_proof` is likewise
production-dead: its only references are `execute_pre_spawn_decision:3522`
(itself dead) and the tests at `:7379` and `:8410`.

The production pre-spawn authority is the **private**
`final_cleanup_proof_roots`, called at `:3689` with `refresh_candidate = true`
(`:2001`). This is *stricter* than the public wrappers, which pass
`refresh_candidate = false` (`:3453`, `:3488`): production re-resolves the
candidate itself; the public path does not.

So, corrected against the common framing: the last gate before any external
process is spawned is `final_cleanup_proof_roots` as invoked at `:3689`, not
`execute_pre_spawn_decision`. The latter is an unused public affordance —
possibly useful to an external consumer, but a reviewer must know it is not what
protects a real run.

**Two more refusals sit in that gate, and they re-prove shapes the
classification cannot see.** The classification refuses a group whose covering
root holds a package directory, or lies inside a workspace's source tree (see
[07 — Workspace](07-workspace.md)); both shapes are invisible to the resolved
universe, which is what the classification works from. The proof re-derives them
from the filesystem at the moment of the spawn: a covered root that lies inside
a source tree is refused at `:3963-3980`, and a covered root that holds a
package directory is refused at `:4045-4054`. Both refusals are
`SkippedSafety` — the unit is withheld, not failed, and the scope continues.

### Why the zero-spawn property matters

For a blocked or empty cleanup the correct behaviour is to spawn no processes at
all — not "spawn, then decide not to delete." The concrete assertions in this
module:

| Test | Lines | Assertion |
| --- | --- | --- |
| `unresolved_scope_note_is_reported_without_blocking_any_mode` | `:5844` | All three modes; a reported in-boundary note never blocks and no clean runs. |
| `initial_shared_overlap_is_decided_conservatively_without_proof` | `:10534` | `proof_workspaces_refreshed == 0` — the gate blocks before any ownership proof. |
| `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` | `:9086` | All three modes; policy rejection precedes any clean. |
| `fresh_source_and_output_age_policy_rejects_before_any_cargo_clean` | `:9136` | All three modes. |
| `preview_is_default_and_simulate_invokes_no_clean` | `:5800` | Simulate: no clean call. |
| `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls` | `:8583` | Simulate reaches cleanable with zero clean calls. |
| `combined_roots_share_one_ownership_proof_and_simulation_never_cleans` | `:10975` | Two roots, one shared universe proof; zero cleans. |
| `simulation_parity_missing_and_invalid_markers_both_skip` | `:8523` | Marker failures skip identically in Simulate and CargoPreview. |
| `the_listing_never_spawns_a_clean` | `:5687` | The interactive listing never spawns a clean. |
| `metadata_failure_blocks_boundary_with_zero_spawns` | `:6638` | A metadata failure inside the boundary blocks with zero spawns. |
| `nested_outputs_skip_at_unit_gate_with_zero_spawns` | `:6599` | A nested output is rejected at the unit gate with zero spawns. |

**Precision required:** "zero-spawn" here means **zero `clean` invocations**, not
zero processes. `cargo locate-project` and `cargo metadata` still run during
discovery and resolution, and `runtime_cargo_version` (`:4322-4334`) runs
`cargo --version` — through the `CleanupRunner` seam, so a test can observe it —
when a selector is present. No test in this module asserts
that *no* subprocess is created.

**Relationship between the two functions.** `execute_pre_spawn_decision` is
`final_cleanup_proof` with fresh counters; `final_cleanup_proof` is
`final_cleanup_proof_roots` with a locally built universe and
`refresh_candidate = false`. The gate logic is identical in all three — the same
numbered gate set (1–12 in the source comments, `:3506`–`:3831`), same
`unit_block_reason_roots` first line. The only differences
are who supplies the universe and whether the candidate is re-resolved
individually. `final_cleanup_proof_roots` at `:3689` is the single authority
consumed by any spawn, and `:3507-3512` says so.

---

## 8. Selection policy and outcome

### `CleanupSelection` — which units are candidates (M014, M018)

`cleanup.rs:98-137`:

| Variant | Meaning |
| --- | --- |
| `All` | Every resolved unit is a candidate. `clean ROOT` and `clean --full`, which are already bounded by their own root selection. |
| `WorkingDirectory` | An **unresolved** anchor on the process working directory. Deliberately fail-closed: `admits` returns `false` until `resolved` has been called, so a caller that forgets to resolve it cleans nothing rather than everything. |
| `Ancestry(PathBuf)` | Units whose canonical workspace root is the anchor, an ancestor of it, or a descendant of it. |
| `Roots(Vec<PathBuf>)` | Units whose canonical workspace root is exactly one of these. The interactive selection produces this. |
| `ExcludeRoots { inner, roots }` | M018: every unit the **inner** selection admits, **except** those under (or equal to) an unproven learned root. |

`admits` (`:147-171`) matches `Ancestry` in **both** directions on purpose: an
operator standing inside a member of a virtual workspace must still select that
enclosing workspace, and an operator standing above several projects must be
able to select one of them. A descendant-only rule would make the common
virtual-workspace layout a silent no-op. `resolved` (`:179-197`) turns
`WorkingDirectory` into `Ancestry(cwd)`; an unresolvable working directory is
an `AppError::InvalidRoot` usage error, never a silent fallback to `All`.

`cli.rs` resolves argv but must not perform IO, so the maintenance spellings
emit the *unresolved* `CleanupSelection::WorkingDirectory` (through
`selection_for`, `cli.rs:648-653`), and `main.rs` calls
`CleanupSelection::resolved` before the engine runs. The `CleanupRequest`
doc comment says so at `cli.rs:303-313`. `clean ROOT` and `clean --full`
use `CleanupSelection::All` because they already bound their own scope.

**Units removed by the selection are reported, never dropped.** The engine
emits one `Skipped` row per excluded unit with `SkippedOutsideSelection`
(`:1771-1797`), carrying the unit's measured bytes and a detail naming the
anchor (`selection.describe()` `:205-221`). Silence here would be the M012A
silent-no-op defect in a new place. `outside_selection_is_reported_rather_than_silently_dropped`
pins it.

#### `ExcludeRoots` — the coverage exclusion (M018)

`ExcludeRoots` (`cleanup.rs:125-130`) is a **narrowing filter**, not an
allow-list: "every unit the inner selection admits, except those under an
unproven learned root". The doc comment states the asymmetry with the
`Roots` variant that `-i` produces — *"the verdict's granularity is the whole
learned root"*, so an exclude-list is the honest shape.

Four properties are load-bearing, and each is a separate guarantee:

1. **It is applied AFTER ownership classification.** The selection is consulted
   at `cleanup.rs:3342`, in the loop that partitions the resolved units into
   `units` and `unselected` — *after* `build_groups` has classified ownership
   across the complete resolved universe (`cleanup.rs:3334-3347`). A shared
   output between a proven root and an unproven one therefore stays `Shared`
   and stays inventory-only. Applying the exclusion earlier — to the roots, or
   to discovery — would be the ADR 006 defect.
2. **It removes candidates only.** It cannot add one or reclassify one.
   `admits` (`:147-171`) is the *inner* selection's answer AND-ed with "not
   under an unproven root"; there is no path by which an excluded unit becomes
   a candidate. It is also the one filter shape that may move the **proof
   boundary**, and that is a deliberate exception with its own authority, not
   a loophole — see the note below.
3. **It composes with any declared selection.** `excluding` (`:205-226`) wraps
   the current selection in an `ExcludeRoots`; if the selection is *already* an
   `ExcludeRoots`, the two root lists are merged and the inner selection is
   kept (`:323-333`). An empty root list is a no-op (`:319-321`).
4. **The comparison is canonical on both sides.** `path_is_within`
   (`:265-267`) canonicalizes both the unit's root and the unproven root, so a
   symlinked prefix (macOS `/var` vs `/private/var`) cannot fail open and leave
   an unproven workspace a candidate.

`resolved` recurses into the inner selection (`:179-197`), so an unresolved
`WorkingDirectory` inside an `ExcludeRoots` is still resolved before the engine
runs — the exclusion does not become a way to skip the resolution.

**The one exception, and why it is not the ADR 006 shortcut.** The exclusion
also places the unproven root outside the **proof boundary**: an unreadable
region inside it blocks *that root*, not the whole scope. ADR 008's acceptance
record decides this — *"an unreadable region inside a learned root blocks
**that root**"* — and ADR 007's amendment of the same date says it from the
other side: *"`clean --full` no longer blocks whole-scope on any unprovable
participant."* The exclusion states a **proof** fact (that root's traversal did
not complete), not an operator's narrowing, which is why it is the one shape
that may move the boundary.

Two things did **not** move with it, and both are load-bearing:

- **Discovery does not narrow.** The excluded root's manifests are still
  discovered, resolved, sized and classified — property 1 above — so a shared
  output between a proven root and an unproven one stays `Shared`.
- **Every other unreadable root still blocks the scope that contains it.** An
  unreadable automatic root is indeterminate coverage, not absence, and
  `unreadable_automatic_root_blocks_instead_of_omitting` keeps saying so.

The engine can only implement the per-root rule because it walks **per root**:
a traversal error carries **no path** — the upstream walk's error event has no
child path — so the root that produced it is the only attribution available.
`CleanupSelection::excludes_coverage_root` is the predicate that says "this
root's incompleteness was already judged and disclosed"; a pathless diagnostic
from any other root is in-boundary and blocks.

**The removal has a reason of its own.** `exclusion_reason`
(`cleanup.rs:257-275`) returns the typed reason a unit was removed, when the
removal has one: `CleanupReasonCode::SkippedIncompleteCoverage`
(`cleanup.rs:390`, spelled `skipped_incomplete_coverage` at `cleanup.rs:474`).
Every other variant returns `None`, and the report then uses the ordinary
`SkippedOutsideSelection`. The two facts are deliberately not conflated: one
says "you did not ask for this", the other says "this could not be proven".
The engine reads it at `cleanup.rs:1966-1975`, where the excluded unit's row is
built.

**It is a filter, not a block.** The unproven root's workspaces are still
*discovered, resolved, sized and classified* — they are removed from the
candidate set, not from the discovery set. That is why the exclusion carries a
typed *skip* reason rather than an ownership failure.

### `CleanupPolicy`

`cleanup.rs:637-643`, `#[derive(Default)]`:

| Field | Default | Enforced at | Notes |
| --- | --- | --- | --- |
| `min_reclaimable_bytes: u64` | `0` | `:2105-2118` (scan `unit.bytes`) and `:2227-2240` (fresh `proof.pre_bytes`) | Checked **twice** — cheaply before the proof, authoritatively after. |
| `min_inactive_seconds: Option<u64>` | `None` | `:2240-2266` | Only **after** the proof, against `proof.newest_activity`. A missing timestamp is treated as too recent (`:2241-2250`), not as "no data, allow". |
| `include: Vec<String>` | empty | `:2025-2033` | Empty ⇒ no include filtering. |
| `exclude: Vec<String>` | empty | `:2035-2043` | Always applied; an empty `GlobSet` never matches. |

Config interaction is `main.rs:440-457`. CLI overrides configured per field, and
for the two list fields an **empty CLI list falls back to config**:
`min_reclaimable_bytes: min_reclaimable_bytes.unwrap_or(configured.min_reclaimable_bytes)`,
`min_inactive_seconds: older_than.or(configured.min_inactive_seconds)`,
`include`/`exclude`: `if … .is_empty() { configured.… .clone() } else { … }`.
Consequence: a user cannot express "ignore the configured exclude list" from the
CLI, because omitting the flag is indistinguishable from passing an empty list.

### Where selection sits relative to proof

`config.toml:41` says the policy is "applied after complete ownership
resolution." **Verified — and the real ordering is more granular.** Within the
unit loop (`:2006-2524`):

```text
2025  include globs           ─┐
2035  exclude globs            │
2040  package capability        ├─ BEFORE the unit gate and BEFORE the proof
2057  profile capability        │   (no clean call, no proof)
2069  package spec resolvable   │
2095  selector + min-size       │
2105  min_reclaimable_bytes     ─┘   (uses scan-time unit.bytes)
2126  unit_block_reason_roots  ─┐
2179  ProofUniverse::refresh    │
2193  final_cleanup_proof_roots│  proof
2227  min_reclaimable_bytes     │   (uses FRESH proof.pre_bytes)
2240  min_inactive_seconds     ─┘   (needs proof.newest_activity)
2354  mode dispatch → spawn
```

Ownership resolution does complete before the loop, so `config.toml:41` is
accurate. But selection is *split*: glob and capability filtering plus the first
size check are pre-proof; the authoritative size check and the age check are
post-proof. No path lets a selector narrow a proof — a selector only reaches
`clean_args` (`:4237-4263`) and affects the spawn.

### `CleanupSelector` and globset

`Profile(String)` / `Package(String)` filter what the *invocation* cleans, not
which units are considered: `--profile <p>` / `--package <q>`. Package specs
are resolved against the workspace's own package list before any spawn, via
`resolve_package_spec` (`:4358-4388`): exact name, exact id, `name@version`, and
`name@1.2`-style partial versions through `semver_spec_matches` (`:4401`, which
accepts a 1- or 2-component numeric prefix). Ambiguity is rejected twice —
multiple matches, and, even with one match, a duplicate package *name* anywhere
in the workspace. A spec that fails becomes `SelectorInvalid` (`:2084-2093`).

Selectors interact with policy in a way that is easy to miss: a selector plus a
non-zero `min_reclaimable_bytes` skips the unit with
`SelectorEstimateUnavailable` (`:485-491`), because `before_bytes` is the whole
output union, not the selector's share. There is also a runtime capability gate:
`runtime_cargo_release` (`:4343-4350`, via `runtime_cargo_version` `:4322-4334`)
asks the `CleanupRunner` seam for `cargo --version` and passes the answer to
`workspace::clean_capabilities_from_version` (`workspace.rs:160`); an
unsupported runtime yields `SelectorUnsupported` before the unit is considered
(`:2040-2068`). The test-only shortcut `runtime_clean_capabilities`
(`:4353-4356`) wraps the same release lookup.

Matching uses `globset`. `compile_policy_globs` (`:2746-2756`) builds a
`GlobSet` per field, mapping a bad pattern to
`AppError::Config("invalid cleanup policy glob ...")`. Patterns match
`unit.root` — the **workspace root**, not the output directory (`:2025`,
`:2035`). Full `globset` syntax is available (`*`, `**`, `?`, `[…]`, `{a,b}`), and
because `Glob::new` is called without `literal_separator(true)`, a single `*`
crosses `/` — `**` is the correct way to say "any depth".

### Disposition, outcome, result, report

`PolicyDisposition` (`:485-491`) is serde snake_case, mapped at
`output.rs:249-260`. It is set only on rows that reached the policy stage:
`Selected` on a row that passed every gate and spawned, or one of the five
other dispositions on a policy skip. A row blocked by the ownership gate or the
proof carries `None` — so **`policy_disposition` is not a "was it considered"
flag**; it means "policy made this call."

`CleanResult` (`:337-419`) is one row per unit. `CleanReport` (`:589-615`):

| Field | Set at | Meaning |
| --- | --- | --- |
| `results` | loop `:2006-2524` | One `CleanResult` per unit that produced a row. |
| `diagnostics` | `:1872`, `:2516` | Diagnostic count; also incremented on post-clean measurement failure (`:2516`). |
| `failed` | `:1873`, `:2427`, `:2446` | Cargo spawn/exit failures. Drives exit code 1. |
| `mode` | `:1874` | The run's `CleanMode`. |
| `counters` | `:1888`, `:2135`, `:2526` | `ScanCounters` including all `proof_*` fields. |
| `scope_blocked` | `:1899`, `:1913`, `:1937`, `:2128`, `:2157`, `:2186`, `:2322`, `:2331`, `:2346`, `:2385` | `Some` only for a typed whole-scope block. Renders alone and returns early. |
| `unresolved_ownership` | `:1877` | `UnresolvedOwnershipParticipant` list — ADR 007: boundary-relevant only. |
| `out_of_scope_unresolved` | `:1878` | Count of manifests that did not resolve **outside** the selected scope. Reported, never blocking. |
| `selected_roots` | `:1879` | The collapsed root set actually cleaned. |
| `discovered_manifests` / `resolved_workspaces` / `units_considered` | `:1880`/`:1881`/`:1882` | Counts for the combined scope header. |
| `effective_policy` | `:1883` | The config/CLI merge result, echoed so JSON consumers see what applied. |
| `selector` | `:1884` | The active selector, if any. |

`render()` (`:720-931`) sorts rows size-descending with a stable path tie-break
(`:777-783`), prints the combined scope header when roots are set
(`:936-946`), emits the out-of-scope line when the count is non-zero
(`:737-742`), and closes with a mode-specific footer (`:876-890`). With a
selector active, size labels switch from
"before"/"after" to "output-union before"/"output-union after" because the
numbers are the whole union — the same distinction `output.rs:177-190` makes: with a
selector active, `before_bytes` and `selector_estimate_bytes` are both `None`
and `output_union_before_bytes` carries the union.

---

## 9. Invariants and edge cases

### Can any code path reach a clean spawn or a deletion without a fresh proof?

**No.** The only two `runner` calls in the production module are at `:2420` and
`:2422`, inside the `CleanMode::CargoPreview | CleanMode::Execute` arm
(`:2373`), and `proof` is bound at `:2193` in the immediately preceding
statement with no `continue`, no loop boundary, and no intervening mutation.
The module header comment (`:3122-3138` on `ExecutionProof`) claims "No earlier
RawGroup is consulted for mutation decisions after proof generation", and the
structure backs it: the `CleanResult` literals in the spawn arm copy from
`proof.*`, never `unit.*`. `unit_block_reason_roots` is re-run as gate 1 of the
proof itself (`:3708`) precisely so the proof is self-sufficient.

Two gates sit *between* the proof and the spawn, both re-verifying rather than
re-proving: `prove_covering_roots` (`:2404-2408`) and `require_cargo_markers`
(`:2412-2416`). Both are the same functions the proof uses, so both refuse the
same states with the same codes. A failure in either skips the unit instead of
spawning.

**Caveat stated precisely:** non-clean spawns happen without a proof and
legitimately so — `cargo locate-project` and `cargo metadata` during resolution,
and `cargo --version` for selector capability. The invariant is about the
*destructive* spawn.

### Is `Simulate` guaranteed to spawn nothing destructive?

**Structurally, not by convention.** The `match mode` at `:2354` routes
`CleanMode::Simulate` to an arm (`:2355-2372`) containing no `runner` call at
all — it pushes a `CleanResult` and moves on. `clean_args` is reachable only from
the `CargoPreview | Execute` arm (`:2417`) and adds `--dry-run --verbose` only
for `CargoPreview` (`:4243-4246`). So `Simulate` is prevented twice over: it
never reaches the dispatch, and if it somehow did, the arg builder would not
produce a non-dry-run clean. No flag combination turns `Simulate` into a clean;
`cli.rs:151-167` makes the mode flags mutually exclusive. `Simulate` does run
the *full* proof — it is a rehearsal of the destructive path, not a cheaper one.

### What happens when the ownership universe is incomplete?

**Three different answers depending on when incompleteness is detected, and
which side of the boundary it sits on. This is the most important distinction in
the module.**

**(a) Incomplete *inside* the boundary at scan time → the whole transaction
aborts.** `boundary_discovery_diagnostics > 0` raises
`ScopeBlockReason::IncompleteDiscovery` and returns early (`:1898-1908`);
`blocking_unresolved > 0` raises `ScopeBlockReason::OwnershipUnproven` and
returns early (`:1911-1926`). Neither proceeds partially. Pinned by
`scope_note_does_not_block_in_boundary_execute` (`:6309`) and its Simulate /
CargoPreview twins, `metadata_failure_blocks_boundary_with_zero_spawns`
(`:6638`), and `symlinked_output_produces_no_unit_while_sibling_cleans`
(`:6674`).

**(b) Incomplete *outside* the boundary → reported, never blocking.** This is
ADR 007's central correction. The three-way split is computed by
`split_unresolved_for_boundary` (`:2914-2938`) over `boundary_admits`
(`:2867-2893`):

| Bucket | Predicate | Effect |
| --- | --- | --- |
| Out-of-boundary | `!boundary_admits(selection, &participant.manifest)` | Informational `out_of_scope` diagnostic; **never blocks** (`:2927-2930`). |
| In-boundary `MergeAll` | `workspace::attribute_unknown` returns `UnknownDisposition::MergeAll` (`:2931-2932`) | **Blocks the boundary** (`:2931-2932`). |
| In-boundary `PassengerOf` / `ScopeNote` | `attribute_unknown` names an owner or a scope note (`:2933-2934`) | Reported, never blocking (`:2933-2934`). |

The counts that drive the decision are: `blocking_unresolved_count` (`:2943-2951`),
`boundary_discovery_diagnostic_count` (`:2965-2990`), and the out-of-scope
diagnostic count (`:1801-1804`). The count of out-of-boundary unprovables
travels to the report as `out_of_scope_unresolved` (`:1878`) and is rendered as
one bounded human line (`:737-742`) so "the boundary is clean" can never read as
"the system is clean". The flagship twin is
`out_of_anchor_poison_does_not_block_anchor` (`:6407`), which fails against the
old whole-scope rule by construction; the twin
`scope_note_in_second_root_does_not_block_first_root` (`:11096`) covers the
cross-root case.

**(c) Incomplete at *proof* time → the transaction aborts.** The hoisted
universe is built once (`:2174-2180`) and shared, so a `ProofUniverse.failure`
makes `hoisted.get()` (`:2185`) fail for *every* candidate, and the engine
raises a typed `ScopeBlock` and returns (`:2186-2192`) rather than emitting N
skipped rows. This is the M-2 correction: an unprovable universe is a scope
block (exit 1), not a run of silent per-unit skips. Pinned by
`universe_workspace_that_cannot_reresolve_fails_closed` (`:10488`).

**(d) The zero-unit edge.** A run that proves zero units while anything is
unproven proves nothing: `units.is_empty() && unselected.is_empty() &&
!unresolved_ownership.is_empty()` raises the standard unproven-ownership block
(`:1592-1606`). Units merely *unselected* do not count — the M014 elsewhere case
reports skipped rows and stays exit 0.

**The residual asymmetry.** The *human* surface reports the out-of-scope count
(`:688-693`), but `CleanupV1` has no field for it: the JSON DTO
(`output.rs:49-64`) carries `unresolved_ownership` as a *count* (`output.rs:55`)
and `unresolved_participants` (`output.rs:56`), but nothing for the
out-of-scope population. "the system is not clean, the boundary is" exists in
prose only. The JSON *does* carry the block itself — `scope_blocked` and
`scope_reason` (`output.rs:53-54`) — so the block/no-block distinction survives
JSON even though the out-of-scope count does not.

### Scoped proof is the default (M017, ADR 007)

Out-of-scope blocking was the product defect, so the proof boundary is the
selected scope and there is no opt-in. Discovery, resolution, grouping, and unit
construction still run combined over the whole selected-root scope (M014
measurement: narrowing discovery would promote `Shared` to `PrivateBounded`);
cross-boundary claims still force `Shared`/skip at grouping time. Only the
block decision is boundary-scoped.

Mechanically, in `clean_with_roots_policy_selector_guarded` (`:1833`):

1. `resolve_cleanup_scope` (`:3014`) splits every unresolved participant three
   ways (`:2914-2938`): out-of-boundary manifests become informational
   `out_of_scope` diagnostics; in-boundary passengers and scope notes join
   `unresolved_ownership` as reported-but-never-blocking; only in-boundary
   `MergeAll` participants early-return the run with the existing typed block.
   Grouping still sees every resolved workspace.
2. The scan-time block sites check only the in-boundary counts
   (`boundary_discovery_diagnostics`, `blocking_unresolved` on the universe):
   discovery uncertainty and MergeAll unknowns inside the boundary block;
   everything out-of-boundary is already a diagnostic.
3. `refresh_combined_universe` (`:3432-3532`) re-applies the boundary rule against
   the fresh workspaces instead of carrying a scan-time tolerated list:
   out-of-boundary and in-boundary passenger/note unknowns stay tolerated; only a
   fresh in-boundary `MergeAll` invalidates the refresh. H-2 rediscovery stays
   combined (mid-run change detection is not poison reporting) — see
   `universe_manifests_stable` (`:2629`) and
   `mid_run_manifest_growth_blocks_remaining_scope` (`:6104`).
4. The output environment is re-verified before every unit dispatch
   (`output_env_matches` `:564`, against a snapshot taken at `:551`); any
   mismatch blocks the remaining units before any further spawn, in every mode
   (Simulate rehearses it).

The machine surface *lost* a field: `CleanupV1.islands` / `IslandV1` are gone.
The key-set test was updated deliberately rather than deleted
(`output.rs:703-713`), so a future field addition fails that test and forces a
conscious contract decision. `unresolved_ownership` is boundary-relevant only
and out-of-boundary poison travels in `diagnostics` as `out_of_scope`
(`DiagnosticCategory::OutOfScope`, `domain.rs`). Exit aggregation is
untouched: `scope_blocked` stays set, so exit 1 falls out structurally.
`-i` lists the selected units and refuses on a boundary block exactly as before
(`list_cleanup_candidates` `:2664`, `listing_offers_rows_with_no_block` `:7003`,
`listing_refuses_blocked_boundary` `:7029`).

**What M015/M016 left behind.** The M015 proof-island certificate and the M016
`--partitioned` flag were removed before publication; the flag and the `islands`
axis never shipped. What carries forward is the certificate's load-bearing
remainder, now the default rule: byte-capability is still the sharp edge; the
environment snapshot and mismatch detector (`snapshot_output_env` `:551`,
`output_env_matches` `:564`) are still re-verified before every unit dispatch;
and the "from the Nth metadata call" race-injection idiom survives as the
module's core test mechanism. History (M015 parity proof, hostile controls) is
in the M015/M016 closure records; the computation, the flag, and the human/JSON
island sections are gone.

### What happens when the proof detects a mid-transaction change?

The candidate is skipped; the run continues to the next unit.

| Change | Line | Message |
| --- | --- | --- |
| Member set changed | `:3816-3821` | "workspace changed before cleanup; skipped: member set changed" |
| Package identity set changed | `:3822-3827` | "…Cargo package identity set changed" |
| Target redirected | `:3840-3849` | "…target changed (was X, now Y)" |
| Build dir changed | `:3850-3859` | "…build-dir changed (was X, now Y)" |
| Group count changed | `:3863-3868` | "…physical output group set changed" |
| Ownership degraded | `:3881-3891` | "ownership changed before cleanup; skipped: output X is now Y" |
| Another workspace now overlaps | `:3910-3925` | "…another discovered workspace (W) now overlaps cleaned output P" |
| Another workspace's source inside the region | `:3930-3946` | "…source tree S of another discovered workspace (W) lies inside cleaned output C" |
| Covering root gone/type/symlink | `:4130-4153` → `:4128-4156` | "preflight changed: …" |
| Source became active | `:4006-4013` | "…source became active" |
| Output became active | `:4055-4060` | "…output became active" |
| Marker invalid | `:4424-4429` → `:4421-4430` | "Cargo CACHEDIR.TAG marker is missing…" |

Covered by `RaceRunner`-based tests: `race_target_replacement_skips_before_clean`
`:8793`, `race_covering_root_disappears_skips` `:9031`,
`race_output_becomes_recent_skips` `:9046`,
`race_source_becomes_recent_skips` `:9068`,
`race_marker_disappears_and_changes_skip` `:9262`,
`race_covering_becomes_symlink_skips_and_changes_classification` `:9292`.
The cross-workspace variants use `StagedCargo` instead:
`cross_workspace_overlap_race_into_target_skips_candidate` `:10304`,
`cross_workspace_overlap_race_into_distinct_build_skips_candidate` `:10348`,
`candidate_target_or_build_change_still_skips` `:10573`.
`BuildChangingRunner` covers the build-dir capability flip (Equal → Distinct) in
`simulation_parity_changed_build_dir_and_unsupported_capability_both_skip` `:8923`.
`MetadataMutationRunner` covers a workspace-root change, a newly added member,
and malformed metadata JSON from the second metadata call onward, in
`direct_metadata_refresh_detects_workspace_root_and_member_changes` `:8820`.

### Are two units sharing physical bytes prevented from racing?

**Yes, by construction rather than by locking.** Three independent mechanisms:

1. **Classification.** Two workspaces pointing at the same physical output produce
   one group with two owners, classified `Shared`, failing
   `unit_block_reason_roots:1433` for every unit that touches it.
2. **Initial-graph decision.** Overlap present in the *initial* complete graph is
   decided conservatively from that graph alone, before any proof work —
   `initial_shared_overlap_is_decided_conservatively_without_proof` (`:10534`)
   asserts `proof_workspaces_refreshed == 0`, i.e. the gate blocks before the
   proof is even built. A shared pair cannot reach a spawn via either unit.
3. **Fresh re-check.** Proof step 7 (`:3901-3947`) re-derives the graph and
   re-checks that *no other* workspace overlaps the candidate's covering union in
   either direction, including symlink and unresolvable roots the graph cannot
   represent. The unit loop at `:2006` is strictly sequential — no unit spawns
   until the previous spawn has returned.

`PerPathFailRunner` (`:7781`) tests a different property: that one unresolvable
manifest blocks a *sibling* workspace that is perfectly clean — the completeness
property of the third question, not a byte race. The byte-race property is
covered by `StagedCargo`'s multi-workspace fixtures (`:10304`, `:10348`, `:10573`).

### Is deletion by `cargo clean`, by `std::fs::remove_dir_all`, or both?

**By `cargo clean` only.** Verified three ways: `grep -n 'remove_dir_all'
src/cleanup.rs` finds only six hits — the module's own trailing comment
(`:4474`) and five inside the test module (`:6279`, `:9035`, `:9298`, `:10639`,
`:10660`) — and that comment (`:4472-4474`)
says "direct deletion remains prohibited (no `remove_dir_all` in production)";
the only mutation path is the child process at `:2419-2423`; and ADR 001
`:188-220` requires "invokes Cargo rather than recursively deleting output…
Cargo-cleanme never deletes Cargo output trees directly."

**Can a direct filesystem removal bypass Cargo's bookkeeping? No — because there
is none.** Cargo's invariants (lock files, `CACHEDIR.TAG` handling, fingerprint
bookkeeping) stay consistent by construction, and the tool cannot accidentally
delete a `target/` tree Cargo believes is live. The cost is inheriting Cargo's
deletion semantics wholesale, including future changes to what `cargo clean`
removes — the intended trade (ADR 001 `:317`). The module does read the
filesystem (measures, reads `CACHEDIR.TAG` `:4423`, touches mtimes through
measurement) but never writes.

### Worst case if the process is killed mid-transaction

**No cleanup.rs operation is non-atomic in a way that can corrupt a workspace,
because cleanup.rs performs no mutation of its own.** The only mutation happens
inside the `cargo clean` child. Killed before the spawn: nothing happened.
Killed during `cargo clean`: the observable state is a partially emptied target
directory, which Cargo treats as a cache miss and rebuilds; no workspace *sources*
are damaged, because a source tree used as an output root is classified
`Uncertain` upstream and can never be a cleanup target. The same holds for
the mirror shape: an output root *nested inside* another package's directory
is refused for the same reason — `cargo clean` removes whatever is under the
root it is given, so a target directory pointed at another package's tree
takes that package's checked-in files with it.
(`workspace.rs:1689-1692`; test `source_tree_as_output_root_is_never_cleaned` `cleanup.rs:10792`). Killed after the spawn but before `measure_union` (`:4452`): the
cleanup happened but no `after_bytes`/`observed_decrease` is reported —
under-reporting, never over-reporting.

The two variables that could redirect a child to the wrong directory are both
pinned: `--target-dir` (`:4252-4253`), `CARGO_TARGET_DIR` (`:4209-4212`),
`CARGO_BUILD_BUILD_DIR` (`:4213-4233`). `--locked` (`:4248`) prevents a
lockfile update. I found no non-atomic window here that could leave a workspace
in an unintended state.

### Can a symlinked output root redirect a deletion outside the intended scope?

**Within one process, no. Across the proof→spawn window the race is real and the
code says so.**

| Site | Check |
| --- | --- |
| `validate_cleanup_root:3244-3262` | A clean ROOT must be absolute (`:3245-3250`) and a real, non-symlink directory (`:3255-3260`). |
| `resolve_authorization_roots:1320-1325` | A symlink allowed root is `Err`. |
| `workspace.rs:1689-1692` | A symlink output root marks the whole workspace `Uncertain` upstream. |
| Proof `:3981-3983` → `prove_covering_roots:4128-4156` | Every covering root re-checked with `symlink_metadata`; a symlink is "preflight changed: covering root became symlink". |
| Proof `:3901-3947` | Symlink/unresolvable *other* roots compared via `provable_output_paths` so they cannot be silently ignored. |
| `measure_union:4452-4470` | Post-clean measurement refuses a path that changed type or became a symlink (`:4458-4460`). |
| Pre-spawn `:2404-2408` | The covering-root proof is re-run at the last moment, immediately before the spawn. |

Tests: `symlink_allowed_root_rejected_and_shared_forbidden` `:7310`,
`symlink_build_identity_skips_entire_unit` `:10105`,
`cross_workspace_symlink_into_candidate_output_fails_closed` `:10387`,
`cross_workspace_symlinked_ancestor_into_candidate_output_fails_closed` `:10461`,
`race_covering_becomes_symlink_skips_and_changes_classification` `:9292`,
`prove_covering_roots_refuses_symlink_swap_with_change_code` `:7536`. The
honest limit is `:2404-2408`: between the symlink check and the child's own
`open()` there is a window, minimized rather than eliminated. Closing it entirely
would require `cargo clean` to re-verify, outside this design.

### Does cleanup ever delete anything outside a workspace's own `target/`?

**No, and the code is stricter than the ADR's framing.** ADR 001 `:131-132` defines
`PrivateBounded` relative to "authorized cleanup boundaries". The implementation
is narrower: `workspace.rs:1688-1694` requires every covering root to be **strictly
below the workspace root**, disjoint from every member source root, with every
universe root represented in the graph. Anything outside the workspace root is
`ExternalUnproven` (`workspace.rs:1701`) and inventory-only, permanently (§5).

Two secondary facts: the *invocation* is still
`cargo clean --manifest-path <workspace root>`, so what Cargo removes is whatever
Cargo believes belongs to that workspace — the module's job is to ensure that
belief is safe, which it does by proving the union before the spawn and freezing
the target via two independent mechanisms. And `build` output is a distinct root
from `target` inside the same unit (`:4213-4233`), covered by the same
workspace-root rule, with the union deduplicated so an equal build==target counts
once (`equal_and_nested_target_build_are_one_unit_and_one_invocation` `:10262`).

### Is the exit-code behaviour correct?

`main.rs:883-892` returns `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`report.failed` is incremented only at `:2427` (spawn error) and `:2446` (non-zero
exit). `scope_blocked` is set only at the typed block sites listed in §8 — the
two scan-time boundary sites, the zero-unit edge, and the mid-run block sites
(the premise recheck, the universe refresh failure, the H-2 rediscovery, and the
environment mismatch).

**Is `scope_blocked` the right signal for "partial failure"? Partly.** It is
right for *scope-level* blocking and correctly forces exit 1 there. Two gaps:

1. **A run where every unit is legitimately `Skipped` also exits 0** — correct,
   since skipping is not failure. But a machine consumer then cannot distinguish
   "nothing to clean" from "could not prove, so cleaned nothing" without
   inspecting `reason_code` per row. The data is available, since
   `CleanupReasonCode` is a stable vocabulary; only the aggregate conflates it.
2. **The out-of-scope count is not in the JSON.** `out_of_scope_unresolved` is
   rendered in the human report but has no field in `CleanupV1`, so a machine
   consumer cannot tell "the boundary is clean" from "the system is clean".

I found no case where exit 0 follows an unsafe clean, and no case where a Cargo
failure goes uncounted. The gap is consistency of the *blocked* signal, not a
safety hole.

---

## 10. Testing

**Numbers (verified by grep):** 126 `#[test]` functions; the test module opens at
`cleanup.rs:4476` (`#[cfg(test)]`), so production is lines 1–4241 (4241 lines)
and the test region is 4242–11163 (6922 lines, 62% of the file). No fixture
files are needed: workspaces are built on disk and mtimes backdated so recency
gates run without sleeping.

### Grouped by invariant protected

| Group | Tests (line) |
| --- | --- |
| Authorization cannot promote | `external_unproven_inside_root_remains_inventory_only` `:7067`; `private_bounded_inside_workspace_is_authorized` `:7199`; `external_unproven_outside_root_never_authorized_by_allowed_root` `:7251`; `symlink_allowed_root_rejected_and_shared_forbidden` `:7310`; `external_outside_root_allowed_via_configured_root` `:8427`; `missing_and_file_allowed_roots_authorize_nothing_but_are_not_fatal` `:7601`; `authorization_infrastructure_failure_scope_blocks_instead_of_per_unit_skip` `:11328`; `external_unproven_skip_code_names_ownership_not_authorization` `:7510`; `ownership_degraded_code_matches_scan_time_vocabulary` `:11439`; `covering_inspect_failure_maps_permission_denied_to_uncertain` `:11423` |
| Ownership gating / unit gate | `gate_blocks_mixed_class_units_in_every_combination` `:10195`; `private_target_with_external_unproven_build_skips_entire_unit` `:9960`; `private_target_with_shared_build_skips_entire_unit` `:10002`; `external_unproven_target_with_private_build_skips_entire_unit` `:10035`; `uncertain_build_identity_skips_entire_unit` `:10064`; `symlink_build_identity_skips_entire_unit` `:10105`; `gate_blocks_unmeasured_and_unmapped_unit_members` `:10232`; `distinct_private_target_and_build_is_one_cleanable_unit` `:9868`; `equal_and_nested_target_build_are_one_unit_and_one_invocation` `:10262` |
| Proof universe completeness | `universe_workspace_that_cannot_reresolve_fails_closed` `:10488`; `cross_workspace_symlink_into_candidate_output_fails_closed` `:10387`; `cross_workspace_unresolvable_root_into_candidate_output_fails_closed` `:10423`; `cross_workspace_symlinked_ancestor_into_candidate_output_fails_closed` `:10461`; `combined_roots_classify_cross_root_output_as_shared_in_every_mode` `:11049`; `scope_note_in_second_root_does_not_block_first_root` `:11096`; `refused_empty_manifest_is_reported_without_blocking_scope` `:7780`; `proof_fails_closed_when_a_member_source_no_longer_resolves` `:7560` |
| Pre-spawn / proof refresh | `revalidation_detects_changed_target_and_skips` `:7675`; `direct_metadata_refresh_detects_workspace_root_and_member_changes` `:8820`; `initial_shared_overlap_is_decided_conservatively_without_proof` `:10534`; `candidate_target_or_build_change_still_skips` `:10573`; `missing_marker_skips_instead_of_failing` `:8480`; `race_marker_disappears_and_changes_skip` `:9262`; `race_covering_root_disappears_skips` `:9031`; `race_covering_becomes_symlink_skips_and_changes_classification` `:9292`; `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` `:9086`; `fresh_source_and_output_age_policy_rejects_before_any_cargo_clean` `:9136`; `race_output_becomes_recent_skips` `:9046`; `race_source_becomes_recent_skips` `:9068`; `race_target_replacement_skips_before_clean` `:8793`; `new_manifest_after_proof_blocks_remaining_scope` `:11512`; `prove_covering_roots_refuses_symlink_swap_with_change_code` `:7536`; `missing_output_identity_pair_compares_equal_not_unproven` `:4662` |
| Coverage exclusion (M018) | `a_unit_under_an_unproven_root_is_reported_as_incomplete_coverage` `:5473`; `a_shared_output_with_an_unproven_root_stays_shared_and_is_not_cleaned` `:5548`; `the_coverage_exclusion_removes_candidates_and_names_the_cause` `:5599` |
| Selection policy | `profile_selector_capability_is_bounded_to_qualified_runtime_versions` `:4983`; `package_specs_require_one_authoritative_workspace_identity` `:5000`; `package_selector_with_valid_workspace_index_evaluates_spec` `:11478`; `excluded_workspace_still_makes_nonexcluded_shared_output_non_cleanable` `:9189`; `simulation_parity_changed_build_dir_and_unsupported_capability_both_skip` `:8923`; `policy_dispositions_have_stable_json_names` `:4934`; `ancestry_selection_matches_in_both_directions` `:4589`; `roots_selection_matches_exactly` `:4629`; `outside_selection_is_reported_rather_than_silently_dropped` `:4642` |
| Mode semantics / parity | `preview_is_default_and_simulate_invokes_no_clean` `:5800`; `simulation_candidate_set_matches_execute_preflight` `:7055`; `simulation_parity_missing_and_invalid_markers_both_skip` `:8523`; `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls` `:8583`; `simulation_parity_recent_source_and_output_both_skip` `:8632`; `combined_private_roots_keep_preview_simulate_execute_parity` `:11009`; `combined_roots_share_one_ownership_proof_and_simulation_never_cleans` `:10975`; `cli_modes_map_to_preview_simulate_execute` `:4903` |
| Concurrency / cost | `proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` `:11162`; `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition` `:11248`; `final_proof_cost_is_linear_in_the_number_of_workspaces` `:10848` |
| Accounting / reporting | `execution_reports_observed_decrease_deduped` `:7428`; `final_report_lists_all_groups_and_deduped_total` `:8094`; `union_accounting_counts_each_output_root_once` `:10593`; `post_clean_disappearing_sibling_is_measured_normally` `:10631`; `post_clean_measurement_uncertainty_fabricates_no_decrease` `:10652`; `stats_account_for_final_ownership_universe_proof_work` `:10937`; `cleanup_progress_is_determinate_through_observer_path` `:9319`; `scan_analysis_total_is_determinate_through_real_path` `:9354`; `blocked_scope_render_lists_unresolved_participants_with_workspace_hint` `:5925`; `blocked_scope_render_bounds_unresolved_participant_listing` `:5986`; `one_invocation_one_result_progress_total_and_no_sibling_row` `:9919` |
| Real Cargo integration | `real_temp_project_preview_and_execute_with_redirected_target` `:8144`; `real_cargo_package_selector_preserves_sibling_workspace_artifacts` `:8283`; `multi_member_private_workspace_can_be_cleaned` `:8400` |
| Selector composition (host-independent) | `a_selector_reaches_the_cargo_command_line` `:11668`; `profile_selector_capability_is_bounded_to_qualified_runtime_versions` `:4983` |
| Root validation / frozen env | `clean_requires_absolute_root` `:7451`; `frozen_env_sets_target_and_build_dir` `:7387`; `frozen_env_pins_the_build_directory_so_an_inherited_redirect_cannot_apply` `:10913`; `collapse_roots_fails_closed_when_a_root_no_longer_resolves` `:7494`; `violated_admission_premise_blocks_with_zero_cargo_spawns` `:7697`; `cleanup_root_symlink_with_trailing_slash_is_refused` `:7469` (`#[cfg(unix)]`) |
| Misc | `cleanup_reason_codes_have_stable_contract_names` `:4954`; `source_tree_as_output_root_is_never_cleaned` `:10792`; `another_workspaces_source_tree_inside_the_cleaned_region_is_never_cleaned` `:10688`; `proof_failures_distinguish_a_change_from_an_unproven_ownership` `:10884`; `cross_workspace_overlap_race_into_target_skips_candidate` `:10304`; `cross_workspace_overlap_race_into_distinct_build_skips_candidate` `:10348` |
| Scoped proof default (M017 / ADR 007) | `scope_note_does_not_block_in_boundary_execute` `:6309`; `simulate_and_preview_predict_without_spawning_or_blocking` `:6462`; `out_of_anchor_poison_does_not_block_anchor` `:6407`; `refused_passenger_rides_along_without_blocking` `:6499`; `redirected_outputs_skip_both_sides_without_blocking` `:6556`; `nested_outputs_skip_at_unit_gate_with_zero_spawns` `:6599`; `metadata_failure_blocks_boundary_with_zero_spawns` `:6638`; `symlinked_output_produces_no_unit_while_sibling_cleans` `:6674`; `mid_run_growth_blocks_remaining_scope_with_reported_note` `:6728`; `proof_refuses_where_stale_universe_would_proceed` `:6776`; `passenger_host_never_cleans_alone_after_redirect` `:6828`; `environment_change_blocks_remaining_units` `:6890` + `environment_change_probe` `:6915`; `listing_offers_rows_with_no_block` `:7003`; `listing_refuses_blocked_boundary` `:7029`; `empty_proof_universe_blocks_listing` `:5721`; `unresolved_scope_note_is_reported_without_blocking_any_mode` `:5844`; `the_candidate_listing_is_the_engines_own_unit_set` `:5105`; `the_listing_never_spawns_a_clean` `:5687`; `a_candidate_row_states_when_it_cannot_be_cleaned` `:5757`; `blocked_listing_detail_names_participants_and_hints_workspace_membership` `:4521`; `mid_run_manifest_growth_blocks_remaining_scope` `:6104`; `universe_stability_predicate_detects_growth_that_naive_stub_misses` `:6075`; `build_dir_move_detected_by_full_env_check_but_missed_by_target_only_naive` `:6034` |

### What each fake runner exists to catch

The highest-value mapping in the module: each runner is a purpose-built
counterexample generator.

| Fake | Mechanism | Unique failure mode it catches |
| --- | --- | --- |
| `FakeCleanupRunner` `:4690` | Synthetic `locate-project`/`metadata` JSON; deletes `artifact.bin` on non-dry-run cleans; `unresolved_manifest` fails one manifest. | The baseline: that a real clean changes the filesystem and the reported decrease matches. The only fake making post-clean measurement non-trivial. `unresolved_manifest` drives the scope-block path. |
| `ChangingTargetRunner` `:7634` | First `metadata` returns the base target; **every later** one a different `target_directory`. | A target redirected by Cargo's *own resolution* between scan and proof — the TOCTOU failure at the module's heart, caught by a proof that trusted the scan's cached `OutputSet`. |
| `RaceRunner` `:8679` | Arbitrary mutation closure on the **second and later** `metadata` calls — precisely the scan→proof window. Optional `second_target`/`second_build`. | Filesystem changes landing *between* two different Cargo invocations. The general race injector: covering-root deletion (`:8686`), mtime bump to now (`:8686`, `:8686`), marker removal or content change (`:8686`), covering root replaced by a symlink (`:8686`), target replacement (`:8686`). |
| `PerPathFailRunner` `:7781` | `locate-project` fails for any manifest path containing `"bad"`; everything else resolves cleanly. | **Partial resolution** — the dangerous shape is one unresolvable manifest plus one perfectly cleanable workspace, and the temptation to clean the cleanable one. Asserts `unresolved_ownership == 1`, `results.is_empty()`, and a `scope_blocked` render. The direct test of §9's incomplete-universe answer. |
| `BuildChangingRunner` `:8935` | `build_directory` starts equal to `target_directory`, becomes distinct on later `metadata` calls. Explicitly does *not* consume a metadata call for `clean`. | A build-dir capability flip (`Equal` → `Distinct`) mid-transaction, where the *shape* of the affected output changes rather than its location. Also guards a subtle fake bug: a fake counting a `clean` as a `metadata` call would corrupt the call-index arithmetic. |
| `StagedCargo` `:9431` | Multi-workspace Cargo; `stage(index, from_call, target, build)` changes output from the Nth `metadata` onward; `fail_metadata_from`; `on_clean` hook mutates the filesystem *during* the clean; `removes_on_clean`; `dry_run_calls()` separates preview from real clean. | The general N-workspace instrument. Cross-workspace contamination (one workspace's change affecting another's proof), per-workspace metadata failure at a chosen call index, and mutation *between* proof and spawn. The `on_clean` hook is what makes the post-spawn window testable at all. |
| `ParallelProbeRunner` `:9697` | Wraps `StagedCargo` with an active/peak counter and a 4 ms sleep on `metadata`. | **Unbounded parallelism** — if the pool were unbounded or mis-chunked, `peak` would exceed `PROOF_REFRESH_CONCURRENCY_CAP`. Also asserts the peak equals what is reported in `counters`, i.e. that the *meter* is honest, not just bounded. The sleep is what makes `peak >= 2` non-flaky. |
| `MetadataMutationRunner` `:9736` | Wraps `StagedCargo`; from the **second** `metadata` call onward mutates the returned JSON: change `workspace_root`, append a member to `packages`+`workspace_members` (also writing a real manifest file), or emit non-JSON. | The three ways a workspace's *identity* changes without its paths changing: root moved, membership grew, Cargo's own output unparseable. A proof comparing only paths, or trusting cached JSON, misses all three. |

Two patterns deserve naming. The **"from the Nth metadata call"** idiom
(`stage`, `fail_metadata_from`, the `MetadataMutation` threshold) is the module's
core race-injection mechanism: it pins the mutation to the exact scan→proof
boundary rather than to wall-clock time, which is what makes these tests
deterministic. And `BuildChangingRunner` documents a fake-implementation hazard
in a comment — good practice, and rare.

### Platform guards

`scripts/check-fixture-portability.py` scans `SCANNED_GLOBS`
(`scripts/check-fixture-portability.py:70-75`), which covers `src/**/*.rs`, so
**it does cover `src/cleanup.rs`**. Its four rules — no literal `:`/`;` PATH
separator assembly (`:335`), no unjustified POSIX-shebang executable written
into a fixture (`:369`), no raw byte file name that needs Linux (`:470`), no
platform-specific `dua_core::Entry` literal (`:514`) — are not triggered by
anything here: no PATH is built, no shebang is used, no byte-literal file name
is created, and all fixtures are constructed through `std::fs` in Rust.

Within `cleanup.rs` the platform guards are narrow and correct: `#[cfg(unix)]`
appears eight times — `:6479`, `:7115`, `:7262`, `:7339`, `:9093`, `:9897`,
`:10170`, `:10250` — all on symlink-manipulating tests, which is right since
creating a symlink and asserting `symlink_metadata(...).file_type().is_symlink()`
are the platform-sensitive operations. The three real-Cargo tests (`:7747`,
`:7882`, `:9148`) carry **no platform guard** and depend on the host Cargo
supporting the flags cleanup passes.

### Honest gaps

1. **The positive `allowed_output_roots` path is unverified end to end.** The
   only test supplying a non-empty `allowed_output_roots` for a group that passes
   the ownership filter is `external_outside_root_allowed_via_configured_root`
   (`:8214`) — and it asserts the group is **skipped**. No test shows a
   `PrivateBounded` group with covering roots outside every clean root becoming
   `Cleaned` once an allowed root is configured. Given `config.toml:37-38` calls
   the field "operationally redundant" this is consistent, but it leaves the
   positive branch of `:1079-1084` unverified while `:1166-1173` advertises the
   capability to users.
2. **`execute_pre_spawn_decision` is untested and unused** — no caller at all
   (§7). No test validates the function its name refers to. Small in production
   terms (production uses the stricter `final_cleanup_proof_roots`), but the
   naming misleads a reviewer about coverage.
3. **The reason-code contract test covers 16 of 20 variants**, and the
   disposition test 5 of 6 (§2). For a vocabulary documented as the
   machine-facing contract, that is thin. `SkippedOutsideSelection` — the code
   that makes a selection non-silent — is one of the four uncovered.
4. **`ownership_skip_detail`'s `PrivateBounded` arm is unreachable** (§5) and
   untested — consistent with unreachability, but a future caller would get a
   misleading message with no test to catch it.
5. **The out-of-scope count has no machine surface.** `out_of_scope_unresolved`
   is rendered in the human report (`:720-725`) but has no field in `CleanupV1`
   (§9). A JSON consumer cannot distinguish "the boundary is clean" from "the
   system is clean".

**The installer-fixture question.** [The overview](overview.md) §11 records the project's
recurring lesson — "a green test proves only that its own premises hold" — and
`check-fixture-portability.py`'s docstring
(`scripts/check-fixture-portability.py:39-47`) names the same failure: whether
"a substituted tool is actually the one the subject resolves" and whether
"a case's pass condition proves the intended branch" are both *not* checked
statically. Does that risk exist here? **Partly, in a narrower form.**

- The *class* of risk is present in the real-Cargo tests. A test like `:7747`
  uses `SystemCleanupRunner` against host cargo; if that invocation failed for an
  unrelated reason (lockfile semantics, a network-less index, an unsupported
  flag), the setup assertions could pass while the intended branch went
  unexercised. `check-fixture-portability.py` cannot catch this — its own
  docstring concedes it "deliberately does **not** check… whether a substituted
  tool is actually the one the subject resolves" and "whether a case's pass
  condition proves the intended branch"
  (`scripts/check-fixture-portability.py:39-47`).
- The *specific* installer failure cannot recur here: no installer, no PATH
  substitution, no shell fixture. The portability rules apply to
  `src/cleanup.rs` and currently find nothing.
- What is **not** defended: no test asserts a *premise* about the host cargo
  before relying on it. The closest is
  `profile_selector_capability_is_bounded_to_qualified_runtime_versions`
  (`:4790`), which tests the version parser in isolation rather than the host
  version. Treat the three real-Cargo tests as environment-dependent and confirm
  they are not silently short-circuiting on the CI lanes.

---

## 11. Review checklist

1. **Any change reaching `:2193-2204` must have a live `proof` from `:2204`.**
   Nothing may read `unit.*` for a mutation decision afterwards; the spawn-arm
   `CleanResult` literals should cite `proof.*` exclusively.
2. **`AuthorizationContext::admits:1262-1264` must remain the first thing that
   inspects anything.** Moving the ownership check after path validation would
   let a future edit return `true` for `ExternalUnproven`. Guarded by `:1262`
   and `:7251`.
3. **The five `PrivateBounded` gates (`:1262`, `:1461`, `:2557-2558`, `:2827`, `:4111`)
   must stay ahead of their `AuthorizationContext::admits` calls** (`:1471` is a
   per-group call that assumes private). This is the C003 unit invariant.
4. **Error strings feed the machine contract only at the refresh boundary.**
   `proof_skip_code` (`:2793`) classifies pass-through Cargo/discovery strings
   by substring; proof gates themselves carry explicit codes
   (`ProofFailure::new`), so rewording a `format!` in
   `final_cleanup_proof_roots` cannot reclassify a `CleanupReasonCode`. When
   touching the refresh boundary or the "changed identity"/"vanished" cases,
   check `:2806-2808`.
5. **A new `CleanupReasonCode` or `PolicyDisposition` variant must be added to
   its contract test** (`:4934`, `:4954`) *and* to `output.rs:176` / `:249-260`.
   Four reason codes and one disposition are currently missing from their tests.
6. **The universe is shared, so a `ProofUniverse` failure is a run-wide scope
   block, not a unit failure.** Touching `hoisted.get()` (`:2185`) or the
   `get_or_insert_with` at `:2174` flips §9's answer from a whole-run
   block to per-unit skips.
7. **`refresh_candidate` is the difference between production and the public
   wrappers.** `:2202` passes `true`; `:3684` passes `false` (via
   `final_cleanup_proof`; `execute_pre_spawn_decision` delegates through it). A
   refactor unifying the call sites must not drop the per-candidate
   re-resolution.
8. **`cargo --version` is a real spawn at `:4324`, outside the proof**, gated on
   `selector.is_some()` (`:1847-1851`). Any new runtime probe should follow the
   same gating so a zero-selector run keeps its spawn count at
   locate+metadata only.
9. **`min_reclaimable_bytes` is checked twice on different numbers** — scan bytes
   at `:2105`, fresh proof bytes at `:2227`. Removing the second check would let a
   unit that *shrank* between scan and spawn pass a size policy.
   `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` (`:9086`)
   covers exactly this.
10. **`frozen_env` (`:4183`) is the only thing between an inherited
    `CARGO_BUILD_BUILD_DIR` and a silent redirect.** Both refusal branches
    (`:4201-4208`, `:4219-4224`) are load-bearing; covered by
    `frozen_env_sets_target_and_build_dir` (`:7387`) and the M5/M9 test
    `frozen_env_pins_the_build_directory_so_an_inherited_redirect_cannot_apply`
    (`:10913`). Likewise `--offline --locked` (`:4247-4248`) are unconditional
    and must not become conditional; a networked cleanup path is a behaviour
    change, not a fix.
11. **Post-clean measurement failure must not fabricate bytes.** The `Err` branch
    at `:2498-2517` sets `after_bytes: None`, increments `diagnostics`, and keeps
    `outcome: Cleaned` with `MeasurementFailed`. Guarded by
    `post_clean_measurement_uncertainty_fabricates_no_decrease` (`:10652`).
12. **Adding a filesystem deletion to this module would be a contract-level
    regression.** There is none in production (`:4472-4474`) and ADR 001
    `:188-220` forbids it. Any `fs::remove_*` in cleanup.rs production code
    should fail review outright.
13. **The parallelism cap (`:3307`) and the meter's honesty.** If
    `ProofRefreshMeter` stops counting, `proof_metadata_peak_concurrency` becomes
    a fiction while the refresh still works.
    `proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` (`:11162`)
    asserts meter-equals-reported.
14. **Determinism under parallel refresh is load-bearing for report ordering.**
    The refresh sorts by root; `render()` re-sorts by size then path.
    `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition`
    (`:11248`) keeps both honest.
15. **`config.toml:37-38`'s "operationally redundant" claim should be revisited.**
    It holds because `PrivateBounded` requires covering roots below the
    *workspace* root (`workspace.rs:1688-1694`), but the positive
    `allowed_output_roots` branch (`:1276-1282`, resolved at `:1301-1343`) and
    its advertised remediation (`:1474-1479`) have no end-to-end test. Add one
    or soften the comment.
16. **The public wrappers with no production caller are a growing dead-code
    surface** — `clean` `:1530`, `is_authorized` `:1156`,
    `covering_is_authorized` `:1180`, `unit_block_reason` `:1421`, plus
    `final_cleanup_proof` `:3653` and `execute_pre_spawn_decision` `:4163`. Each
    is a place where a future edit can diverge from the live path with no compile
    error. Mark them clearly as test/consumer surface or delete them.
17. **The ADR 007 boundary rule has three homes that must not drift:**
    `split_unresolved_for_boundary` (`:2914`), `blocking_unresolved_count`
    (`:2943`), and `boundary_discovery_diagnostic_count` (`:2965`). The engine
    and the interactive listing both consume the same counts (`:1800`, `:1805`,
    `:2677`, `:2687`). A new block site must ask the same predicate.
18. **The mode surface changed under the reader's feet.** `CleanMode` has no
    `Preview` variant, `--dry-run` now means `Simulate` (not "discarded"), and
    bare `cargo cleanme` is `Execute`. Any prose, test name, or downstream
    script that says "the default is a preview" or "the default does not
    clean" is wrong and must be corrected, not preserved.
19. **The coverage exclusion removes candidates and nothing else (M018).**
    `CleanupSelection::ExcludeRoots` (`:125-130`) is a *narrowing* filter: it
    composes with any declared selection (`:164-169`), it is applied after
    ownership classification (`:1965-1975`), and it can only remove. A refactor
    that applies it earlier — to the roots, or to discovery — is the ADR 006
    defect: an out-of-scope workspace that shares an output root would become
    invisible, and its bytes would be promoted from `Shared` to `PrivateBounded`.
    The reason code it produces (`SkippedIncompleteCoverage`, `:390`) is
    distinct from `SkippedOwnershipUnproven` on purpose — one says "you did
    not ask for this", the other "this could not be proven" — so a consumer can
    tell which question it is looking at.

---

## Related

- [Architecture overview](overview.md) — cross-cutting invariants
- [Domain & errors](01-domain-and-errors.md) — `OutputOwnershipClass`, `OutputSet`, `ScanCounters`
- [Workspace](07-workspace.md) — grouping, `CleanupUnit`, `RawGroup`, ownership classification
- [Traverse](08-traverse.md) — activity and sizing measurement
- [Config and editor](03-config-and-editor.md) — `allowed_output_roots` schema
- [Testing & verification](14-testing-and-verification.md) — the false-green discipline this module is subject to
