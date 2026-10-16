# Cleanup — authorization, ownership proof, and the three modes

> Component deep dive · part of the [architecture overview](overview.md)

Module under review: `src/cleanup.rs`, 6730 lines (2532 production, 4198 inline
tests). It is the largest module in the crate and the densest concentration of
safety logic. Every other module decides *what to look at*; this one decides
*what may be destroyed*, proves the decision still holds, and only then acts.

---

## 1. Responsibility

`cleanup.rs` owns the entire destructive path: mode selection semantics
(`CleanMode` `:36`, `main.rs:102-110`), the authorization gate
(`covering_is_authorized_roots` `:586`), the unit-wide destructive gate
(`unit_block_reason_roots` `:703`), the final ownership proof
(`final_cleanup_proof_roots` `:1986`), the frozen Cargo invocation
(`frozen_env` `:2338`, `clean_args` `:2385`), the spawn itself (`:1256-1258`),
post-clean accounting (`measure_union` `:2509`), and the human report
(`CleanReport::render` `:275`).

It explicitly does **not** own:

- **Activity measurement.** `traverse::workspace_member_activity` `:2226`,
  `traverse::measure_single_target` `:2246` and `:2520`. This module consumes
  timestamps; it never turns them into eligibility. See [Traverse](08-traverse.md).
- **Grouping and classification.** Every physical-group decision is delegated to
  `workspace.rs`: `resolve_workspaces_with_coverage` `:1292`, `build_groups`
  `:1309`/`:1852`, `analyze_groups_detailed` `:1314`, `build_cleanup_units`
  `:1324`, `refresh_workspace_from_root_manifest` `:1526`, `map_workspace_groups`
  `:1931`, `outermost` `:1951`, `workspace_member_roots` `:2015`.
- **Manifest discovery** — `discovery::discover_manifests` `:930`, `:1800`.
- **JSON rendering** — `output.rs:158` maps `CleanReport` to `CleanupV1`.
- **Progress presentation** — emits `ProgressObserver` events (`progress.rs:39`).

Actual crate imports (`cleanup.rs:14-21`): `discovery`, `domain::*`,
`error::AppError`, `progress::{ProgressObserver, ScanPhase}`, `traverse`,
`workspace::{self, CargoRunner}`. Additional fully-qualified uses:
`crate::report::format_bytes` (`:316`, `:324`, `:341`, `:348`, `:355-357`),
`crate::domain::{EffectiveScanPolicy, ScanScope, DiscoveryFilters, elapsed_nanos}`
(`:772-776`, `:1604-1608`, `:791`, `:2032`, `:2050`), `crate::progress::NoopObserver`
(`:671`, `:1531`, `:1627`). Line `:375` re-exports
`workspace::{ProcessOutput, SystemCargoRunner}` so tests can inject I/O through
this module's own seam.

**The "one-way edge" is test-only.** `workspace.rs:3743` calls
`crate::cleanup::covering_is_authorized`, but that line sits inside
`workspace.rs`'s own `#[cfg(test)] mod tests`, which begins at
`workspace.rs:2002`. There is no production edge from `workspace.rs` into
`cleanup.rs`. `overview.md:33-35` describes this as "a deliberate one-way call
into authorization helpers"; the substance (no cycle) holds, but the edge is a
test reference, not a production call.

---

## 2. Public surface

### Modes, outcomes, reporting

| Item | Signature (line) | Contract |
| --- | --- | --- |
| `CleanMode` | `enum { Preview, Simulate, Execute }` `:31` | Which semantics to run. `Preview` is `#[default]` (`:34`). |
| `CleanMode::as_str` | `-> &'static str` `:45` | `"preview"`/`"simulate"`/`"execute"`; JSON `mode` field (`output.rs:213`). |
| `CleanOutcome` | `enum { Previewed, Simulated, Cleaned, Skipped, Failed }` `:55` | Per-unit terminal state. |
| `CleanOutcome::as_str` | `-> &'static str` `:66` | `"previewed"`/`"simulated"`/`"cleaned"`/`"skipped"`/`"failed"`. Comment `:64-65`: the JSON contract must not depend on `Debug`. |
| `CleanResult` | `struct` `:78` | One row per workspace CleanupUnit: `display_path`, `workspace_roots`, `output_roots` (the dedup covering union one invocation can affect), `ownership`, `outcome`, `policy_disposition`, `reason_code`, `before_bytes`, `after_bytes`, `observed_decrease`, `detail`. |
| `CleanReport` | `struct` `:177` | Run-level result; field table in §8. |
| `CleanReport::render` | `-> String` `:275` | Human projection. On `scope_blocked` emits the block message, then — only for `OwnershipUnproven` with retained participants — a bounded listing (at most 5 `unresolved [stage] manifest: reason` rows plus an overflow line, `:288-341`) with a `[workspace]`-detach hint for the believes-in-workspace class. JSON (`unresolved_participants`) and the log line (count only) are unchanged. |
| `CleanupReasonCode` | 19-variant `enum` `:96` | The machine-facing reason vocabulary, tabled below. |
| `CleanupReasonCode::as_str` | `-> &'static str` `:140` | Stable snake_case, consumed at `output.rs:173`. |
| `ProofFailure` | `struct { code, detail }` `:122` | A failed final proof. |
| `ProofFailure::new` | constructor | Proof gates assign an explicit code at the failure site; new gates must use this. |
| `From<String>` / `From<&str>` | `:127` / `:133` | Legacy fallback for the Cargo-refresh boundary only, which passes through Cargo/discovery strings with no typed site; it derives `code` via `proof_skip_code`. Proof gates must not rely on it. |
| `PolicyDisposition` | 6-variant `enum` `:167` | `#[serde(rename_all="snake_case")]`; mapped at `output.rs:246-257`. |

Every `CleanupReasonCode` variant and wire name (`:140-162`):

| Variant | `as_str()` | Meaning |
| --- | --- | --- |
| `Previewed` | `previewed` | `cargo clean --dry-run` succeeded. |
| `Simulated` | `simulated` | Full proof passed; no `cargo clean` invoked. |
| `Cleaned` | `cleaned` | Real `cargo clean` succeeded; post-clean union measured. |
| `BelowMinimumSize` | `below_minimum_size` | Rejected by `min_reclaimable_bytes`. |
| `TooRecentForPolicy` | `too_recent_for_policy` | Rejected by `min_inactive_seconds`, or the required timestamp was unavailable (`:1011-1020`). |
| `NotIncluded` | `not_included` | Workspace root failed the `include` globs. |
| `Excluded` | `excluded` | Workspace root matched `exclude` globs. |
| `SkippedActive` | `skipped_active` | Source or output became/recent active. |
| `SkippedShared` | `skipped_shared` | A group in the unit is `Shared`. |
| `SkippedUncertain` | `skipped_uncertain` | `Uncertain` group, or an output root with unprovable physical identity (`:1188-1190`). |
| `SkippedUnauthorized` | `skipped_unauthorized` | Private unit outside the authorized boundary, or the gate fell through with no specific cause (`:1215`). An `ExternalUnproven` group reports `SkippedOwnershipUnproven`, not this. |
| `SkippedMarkerInvalid` | `skipped_marker_invalid` | Missing/malformed/unsigned `CACHEDIR.TAG`. |
| `SkippedChangedBeforeCleanup` | `skipped_changed_before_cleanup` | The workspace demonstrably changed between scan and delete. |
| `SkippedOwnershipUnproven` | `skipped_ownership_unproven` | Ownership could not be re-proven *at all* — Cargo refused, discovery incomplete, manifest did not re-resolve. Doc comment `:110-112` and the L9 test `:5773` both insist this is distinct from a real change. |
| `SkippedSafety` | `skipped_safety` | Fail-fast skip that is not active/uncertain (`:1211`) and the `proof_skip_code` catch-all (`:1242`). |
| `CargoFailed` | `cargo_failed` | Spawn error or non-zero exit (`:1071-1107`). |
| `MeasurementFailed` | `measurement_failed` | Cargo succeeded, post-clean union unmeasurable; outcome stays `Cleaned`, no bytes fabricated (`:1143-1162`). |
| `SelectorUnsupported` | `selector_unsupported` | Runtime Cargo lacks the selector, or selector-scoped bytes make a min-size policy unevaluable (`:901-947`). |
| `SelectorInvalid` | `selector_invalid` | `--package` spec unknown or ambiguous here (`:925-937`). |

**Verified gap:** `cleanup_reason_codes_have_stable_contract_names`
(`:2575-2601`) asserts 16 of 19 wire names — omitting `SkippedOwnershipUnproven`,
`SelectorUnsupported`, `SelectorInvalid`. The first is covered incidentally at
`:5796-5798`; the other two are covered nowhere.
`policy_dispositions_have_stable_json_names` (`:2555-2572`) covers 5 of 6
variants, omitting `SelectorEstimateUnavailable` (emitted as
`"selector_estimate_unavailable"` at `output.rs:254`).

### Policy, selection, runner seams, proof, authorization, entry points

| Item | Signature (line) | Contract |
| --- | --- | --- |
| `CleanupPolicy` | `struct { min_reclaimable_bytes: u64, min_inactive_seconds: Option<u64>, include: Vec<String>, exclude: Vec<String> }` `:216` | `Default` ⇒ no filtering. §8. |
| `CleanupSelector` | `enum { Profile(String), Package(String) }` `:196` | Narrows a unit's clean. |
| `CleanupSelector::kind` / `::value` | `-> &'static str` `:202` / `-> &str` `:208` | `"profile"`/`"package"`; the user spec. |
| `CleanupRunner` | `pub trait CleanupRunner: Sync` `:377` | The spawn seam: `run` `:378`, `run_with_env` `:379-384`. |
| `SystemCleanupRunner` | `pub struct SystemCleanupRunner;` `:387` | Real impl; also `impl CargoRunner` `:389` so one value serves resolution too. |
| `WorkspaceCleanupAdapter` | private `:1332` | Adapts `&dyn CleanupRunner` to `&dyn CargoRunner`. |
| `ProofRefreshMeter` | private `:1484` | Measures concurrent `cargo metadata` activity. |
| `ExecutionProof` | `struct` `:1366` | Frozen pre-spawn state of one unit; 14 fields, §6. |
| `ProofUniverse` | private `:1564` | One hoisted re-resolution of the complete universe. |
| `final_cleanup_proof` | `pub fn (unit, universe, clean_root, allowed_output_roots, runner, recency_seconds, counters) -> Result<ExecutionProof, ProofFailure>` `:1767` | Single-root wrapper, `refresh_candidate=false`. **No production caller** — §7. |
| `execute_pre_spawn_decision` | `pub fn (unit, universe, clean_root, allowed_output_roots, runner, recency_seconds) -> Result<ExecutionProof, ProofFailure>` `:2108` | Same gate set with throwaway counters. **No caller at all** — §7. |
| `is_authorized` | `pub fn (group: &workspace::RawGroup, clean_root: &Path, allowed_roots: &[PathBuf]) -> Result<bool, String>` `:453` | Wrapper. **Test-only callers** (`:2836-3080`). |
| `covering_is_authorized` | `pub fn (ownership, covering: &[PathBuf], clean_root: &Path, allowed_roots: &[PathBuf]) -> Result<bool, String>` `:470` | Single-root wrapper. **Test-only** (`workspace.rs:3743`). |
| `covering_is_authorized_roots` | `pub fn (ownership, covering, clean_roots: &[PathBuf], allowed_roots) -> Result<bool, String>` `:484` | The real gate. Production caller `:628`. |
| `ownership_skip_detail` | `pub fn (ownership: OutputOwnershipClass) -> String` `:565` | The "why wasn't this cleaned" sentence. |
| `unit_block_reason` | `pub fn (unit: &workspace::CleanupUnit, clean_root: &Path, allowed_output_roots: &[PathBuf]) -> Result<(), String>` `:589` | Single-root wrapper. **Test-only** (`:5263-5301`). |

Production uses the private multi-root variants `unit_block_reason_roots`
(`:964`, `:1808`) and `covering_is_authorized_roots` (`:628`). The public
single-root wrappers are test/consumer surface and carry no production weight.

**Entry-point delegation chain.**

```text
clean_with_roots_policy_selector   :788   ← run_engine_guarded (main.rs:470) calls the guarded form
  └─ clean_with_roots_policy_selector_guarded :829   (premise = None for the unguarded path)
  └─ clean_with_roots_policy       :766   (selector = None)
       └─ clean_with_roots         :746   (policy = CleanupPolicy::default())
            └─ clean_with          :727   (roots = [root])
                 └─ clean          :710   (SystemCleanupRunner + NoopObserver)
```

Each layer supplies only the defaults its caller omitted. `clean` is the only
wrapper injecting a concrete runner and observer. Grepping `src/` finds no
in-crate caller of `clean`, `clean_with`, or `clean_with_roots` other than
tests — `main.rs` goes through `run_engine_guarded` and supplies
`SystemCleanupRunner` itself. The unguarded `clean_with_roots_policy_selector`
delegates with `premise = None` and exists so the public root contract stays
strict: every selected root is still validated, whatever the caller proved
beforehand.

---

## 3. The runner seams

```rust
// workspace.rs:29-31
pub trait CargoRunner {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
}

// cleanup.rs:499-507
pub trait CleanupRunner: Sync {
    fn run(&self, cwd: &Path, args: &[OsString]) -> io::Result<ProcessOutput>;
    fn run_with_env(&self, cwd: &Path, args: &[OsString],
                     env: &[(OsString, OsString)]) -> io::Result<ProcessOutput>;
}
```

`CleanupRunner: Sync` is what makes `std::thread::scope` legal in
`refresh_proof_universe_with_limit` (`:1676-1687`): one `&dyn CleanupRunner` is
shared across up to `PROOF_REFRESH_CONCURRENCY_CAP = 4` threads (`:1482`).
`CargoRunner` has no such bound because `resolve_workspaces_with_coverage` is
sequential and cached (`:1290`).

**What `SystemCleanupRunner` executes.** `run_with_env` (`:405-423`) builds
`Command::new("cargo").args(args).current_dir(cwd)`, applies each `(k, v)` with
`cmd.env(k, v)`, calls `.output()`. Args come from `clean_args` (`:2175-2201`):

```text
clean [--dry-run --verbose] --offline --locked
      --manifest-path <root_manifest> --target-dir <target>
      [--profile <p> | --package <q>]
```

**`CleanMode::Preview` versus `Execute` is exactly the `--dry-run --verbose`
pair** (`:2181-2184`); everything else is identical. This is ADR 001 `:152`
("may invoke `cargo clean --dry-run --verbose`") made literal. The target is
pinned twice — `--target-dir` (`:2190-2191`) and `CARGO_TARGET_DIR` (`:2147-2150`)
— with the comment at `:2189` calling the two forms "equivalent, Cargo-owned".

`frozen_env` (`:2128-2173`) pins the child's world: `CARGO_TARGET_DIR` always to
the proven target; `CARGO_BUILD_BUILD_DIR` **always** set — to the resolved
build dir when Cargo reported one (`:2151-2163`), or to the target when it did
not (`:2167-2170`), the comment at `:2164-2166` noting that pinning it to the
target asserts exactly the measured world and is a no-op if ignored. Two refusal
branches: a parent `CARGO_BUILD_BUILD_DIR` with an unprovable build dir
(`:2139-2146`), and a reported separate build dir under
`Unavailable`/`Unknown` capability (`:2157-2162`).

**Why an adapter.** The two traits do not convert. `workspace.rs` needs a
`&dyn CargoRunner` to resolve; cleanup needs `&dyn CleanupRunner` to run the
clean *with a frozen env*. `CleanupRunner` is the richer trait, so
`WorkspaceCleanupAdapter` (`:1332-1346`) is the widening direction: it copies
all four `ProcessOutput` fields (`workspace.rs:22-27`) and drops the env, which
resolution calls do not need. Constructed at `:1291`, `:1523`, `:1619`.
Production therefore makes every Cargo call through one injected seam regardless
of phase.

**Testability payoff and its risk.** The 75 tests need no real cargo binary for
decision logic. Eight fakes are defined in-module: `FakeCleanupRunner` `:2543`,
`ChangingTargetRunner` `:3496`, `PerPathFailRunner` `:3637` (function-local),
`RaceRunner` `:4312`, `BuildChangingRunner` `:4534` (function-local),
`StagedCargo` `:5012`, `ParallelProbeRunner` `:5223`, `MetadataMutationRunner`
`:5271`. Three tests do use `SystemCleanupRunner` against real Cargo: `:3785`,
`:3920`, and the `scan_analysis_total_is_determinate_through_real_path` path at
`:4953`.

The risk is the false-green class this project has already been bitten by
(`overview.md:221-225`). Concrete instances here: every fake ignores
`--offline --locked`; `StagedCargo`'s clean block (`:5188-5195`) deletes only files
the test told it to, so a wrong `--target-dir` would not surface unless a test
asserts the arg vector; `FakeCleanupRunner::unresolved_manifest` (`:2550`,
`:2583-2593`) models `locate-project` failure only for a manifest whose last arg
canonicalizes to the configured path; and no fake models a `cargo clean` that
actually fails on a real target tree.

---

## 4. Modes and the clean entry points

| Variant | `as_str()` | CLI | Guarantees | Does not guarantee |
| --- | --- | --- | --- | --- |
| `Preview` (`#[default]`) | `preview` | default or `--dry-run` | Every gate through the final proof ran; Cargo reported what it *would* remove. | Nothing removed by us — but Cargo *was* spawned. |
| `Simulate` | `simulate` | `--dryrun` | Identical gate set; the unit reached the spawn point. | No `cargo clean` of any kind. Reported as `Simulated`, never as recovered space (`:1052`; test asserts no "recovered" in the render, `:2698`). |
| `Execute` | `execute` | `--yes` | Every gate, a real `cargo clean` with frozen env, then a post-clean union measurement. | — |

The mapping lives at `main.rs:100-110`, with the comment "Distinct spellings,
distinct semantics" at `:100-101`. Note `main.rs:108`: `let _ = dry_run;` — the
`--dry-run` flag is *explicitly discarded*, since it and the default already mean
`Preview`. `cli.rs:111-116` makes the flags mutually exclusive in both directions
and gives each a doc comment naming the other; `cli.rs:453` asserts
`--dry-run --dryrun` is a parse error.

**Is the distinction adequately surfaced?** Documented in `cli.rs` and the
variant docs, and `render()` emits a distinguishable footer per mode (`:337-360`):
"no cleanup executed" for `Preview` versus "no `cargo clean` command was invoked"
for `Simulate`. But nothing in the runtime output names the *mode* up front — a
user who typed `--dryrun` (easily a typo of `--dry-run`) only learns which one
they got from a footer, after a full run. Risk is low and the failure direction
is safe (`Simulate` is stricter than `Preview`), but a mode line, or a
"simulation is an estimate, not Cargo's own dry run" note in the `Simulate`
footer, would close it.

**Where roots come from** (`main.rs:559-698`, C028), all funnelled into
`clean_with_roots_policy_selector_guarded` via `run_engine_guarded`
(`main.rs:470`):

| Source | Code | Roots |
| --- | --- | --- |
| Explicit `ROOT` | `main.rs:570-588` | `[cli::absolutize_root(&root)]` classified `Explicit` — relative input accepted for ergonomics, absolutized before becoming the safety boundary. Any defect is fatal. |
| `--full` | `main.rs:589-610` | The in-memory generation the just-finished Full reconciliation proved (`policy::full_cleanup_roots`, `policy.rs:430`), classified `Automatic` by `admit_automatic` (`main.rs:609`). No disk reload, so a failed save cannot resurrect stale roots. |
| Maintenance / `--known` | `main.rs:615-697` | Raw Routine candidates (`policy::routine_cleanup_candidates`, `policy.rs:465`) classified `Automatic` before any collapse; a configured `scan.root` takes the strict explicit path instead. |
| None | — | `AppError` (unknown scope) or a typed `ScopeBlock` (symlink/unreadable automatic root) — never a silent subset. |

`admit_automatic` collapses admitted paths only (`main.rs:734`), remembers
omissions for the late recheck, and converts symlink/uncertain automatic
roots into a whole-scope `ScopeBlock` (exit 1) rather than a fatal error.
`run_engine_guarded` additionally defends the admission→validation race: a
fatal `InvalidRoot` over an automatic scope triggers one fresh
reclassification, and only a now-proven-omittable offender rebuilds the
universe from scratch (`main.rs:495-530`); anything else becomes a typed
block, never a sibling cleanup against a stale proof.
`cleanup.rs:981` applies `collapse_roots` again and re-validates the collapsed
set at `:908-910` with the comment that collapsing only narrows the set, so
survivors still need checking. An empty admitted set short-circuits in
`run_cleanup` and never reaches this module; a blocked set reports without
traversing anything.

### The admission premise and its late recheck (C028)

`AdmissionPremise` (`cleanup.rs:957`) carries the safely omitted automatic
roots alongside the admitted set. The engine re-examines the premise with
fresh `symlink_metadata` — no timers, sleeps, or cached metadata — at two
moments: before the first candidate reaches final proof (`cleanup.rs:1244`)
and before every later Cargo spawn (`cleanup.rs:1420`). A violated premise
returns the completed results plus a whole-scope `IncompleteDiscovery` block
instead of spawning. Policy-skipped units never reach proof and need no
guard. `violated_admission_premise_blocks_with_zero_cargo_spawns`
(`cleanup.rs:4213`) proves the block with zero spawns in both directions.

---

## 5. Authorization

Authorization is a *location* gate, kept strictly separate from the *ownership*
gate. `is_authorized`'s doc comment (`:436-452`) states the rule set;
`covering_is_authorized_roots` (`:484-559`) implements it.

**What makes a location authorized.** (1) Ownership first, as a hard filter
(`:490-495`). (2) Each allowed root is validated then canonicalized (`:498-532`):
non-absolute → `Err` `:499-504`; missing → `continue`, non-matching but not
fatal `:505-510`; other fs error → `Err` `:511-516`; symlink → `Err`
`:517-522`; not a directory → `continue` `:523-525`; else canonicalize and push
`:528-531`. (3) Clean roots are canonicalized or the call fails `:533-539`.
(4) Every covering root must sit inside *some* boundary — `clean_roots` or
`canonical_allowed`, compared with `==` or `starts_with`; one miss → `Ok(false)`
(`:541-556`).

**Absolute directory roots, not globs.** `config.toml:29-30` requires absolute
directory roots and invalidates symlink roots. Verified: the absoluteness `Err`
is `:499-504`, the symlink `Err` is `:517-522`, and there is no glob machinery
anywhere in the function. **Precision worth recording:** the "not globs" rule is
enforced by construction, not validation. A mistyped glob such as `/tmp/*/out`
*is* absolute, so it passes `:499-504`; `symlink_metadata` returns `NotFound`,
`:505-510` `continue`s, and the entry silently authorizes nothing — outcome
`Ok(false)` with the message at `:636-641`, not a configuration error.
Fail-closed but silent.

**The critical property: authorization cannot promote an ownership class.
Definitive.** `covering_is_authorized_roots` returns `Ok(false)` for any class
other than `PrivateBounded` at `:493-495`, *before* inspecting a single path.
`allowed_output_roots` is consulted only at `:549-554`, unreachable for
`ExternalUnproven`, `Shared`, and `Uncertain`. The same rule is re-asserted at
the unit gate (`:618`, comment "Ownership class is authoritative; authorization
never promotes") and again in the proof (`:1940-1948`, where a fresh group no
longer `PrivateBounded` fails regardless of location).

This matches three independent sources: `config.toml:33-35` ("authorization never
manufactures ownership proof and cannot promote ExternalUnproven to private
(ADR 001, C002)"); `overview.md:189-192` invariant 2; ADR 001 `:101-105` and
`:109-115`. Tests close it from both sides:
`external_unproven_outside_root_never_authorized_by_allowed_root` (`:2953-3008`)
asserts `!is_authorized` with and without an allowed root, and
`external_outside_root_allowed_via_configured_root` (`:3624-3673`) runs the
*real* Cargo path and asserts `Skipped` in both cases.

**One naming trap.** The test at `:3624` is named
`external_outside_root_allowed_via_configured_root`, which reads as "an allowed
root makes external output cleanable". Its own comments (`:3653-3656`) and
assertions say the opposite — the unit is skipped *because* authorization cannot
promote. Rename before someone "fixes" it to match the name.

**`ownership_skip_detail`** (`:565-576`) exists to state both halves of the rule
(doc comment `:561-564`): the class is unproven *and* configuration does not fix
it. `Shared` and `Uncertain` → "…remains inventory-only";
`ExternalUnproven` → "external output ownership is unproven; configured
authorization does not establish exclusivity" (asserted `:3005-3007`). The
`PrivateBounded` arm — "external output requires explicit cleanup authorization"
— is **unreachable in production**: its only two call sites (`:624`, `:1945`) sit
inside branches already guarded by `ownership != PrivateBounded` (`:618`,
`:1940`), and its text is a *location* message that cannot apply to a group that
passed the location check. Harmless, but a trap for a future caller.

**`unit_block_reason`** (`:589-657`) runs three ordered checks over one
`CleanupUnit`: (a) `unit.unmapped` non-empty → `Err` naming root kind and logical
path (`:606-615`) — any output root with unprovable physical identity blocks the
whole unit because its footprint cannot be bounded; (b) for every group the
invocation can affect, ownership must be `PrivateBounded` (`:617-626`) then
`covering_is_authorized_roots` must return `Ok(true)` for that group alone
(`:627-644`); (c) every group must have `measured.is_some()` (`:645-654`),
otherwise the fail-fast verdict and its `GroupSkipReason::detail()`
(`workspace.rs:1412-1422`) become the reason. The doc comment `:578-588` states
the C003 invariant — every affected group qualifies independently, and "No Cargo
process is spawned for any subset of a unit" — which is what stops a
`PrivateBounded` target smuggling an `ExternalUnproven` sibling build group into
one invocation (ADR 001 `:208-210`).

**Is `allowed_output_roots` actually redundant?** `config.toml:36-38` claims it
is "currently operationally redundant because PrivateBounded output is normally
workspace-contained." **Assessment: consistent with the code, with a caveat.** The
reason is the classification rule at `workspace.rs:1217-1220`: `PrivateBounded`
means "every covering root **strictly below the workspace root**";
`ExternalUnproven` means "output **outside workspace root**." Classification
depends on the *workspace root*, not on the clean root or allowed roots —
`build_groups` (`workspace.rs:1221`) takes only `&[ResolvedWorkspace]`. So a
`PrivateBounded` group's covering roots are always beneath the workspace root and
are already authorized by the `canonical_clean` branch at `:543-548` whenever
the workspace root is inside a clean root. That is the ordinary case.

The caveat: if `cargo locate-project` resolves a workspace root *above* the clean
root — legal for a member manifest whose workspace root is an ancestor — then
`PrivateBounded` covering roots can fall outside every clean root, and
`allowed_output_roots` becomes the only route to cleanability. The code
anticipates this: the error at `:636-641` tells the user to "add a
`cleanup.allowed_output_roots` entry or use a clean ROOT that contains it". I
found no end-to-end test of that path; `workspace.rs:2310` covers roots above a
declared directory, but the cleanup-side allowed-root-then-cleaned case is
untested. An under-verified property, not a defect.

**A separate divergence.** ADR 001 `:92` defines `PrivateBounded` as "every
destructive output path is contained by an *authorized cleanup boundary*" — a
predicate over authorization roots. The code defines it over the *workspace root*
(`workspace.rs:1217-1219`). Reconciled by the argument above, but not the same
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
{ manifest, stage, reason }` (`domain.rs:171-175`) is a discovered manifest
without authoritative Cargo coverage, and `ResolutionCoverage { workspaces,
unresolved, discovered_manifest_count }` (`domain.rs:179-183`) is resolution
output *plus* explicit accounting of the discovered set, so "I resolved some of
them" cannot be confused with "I resolved all of them".

`cleanup.rs` obtains coverage from `workspace::resolve_workspaces_with_coverage`
(`:1292`) and builds no group until `unresolved` is empty (`:1302-1308`). It
re-derives the guarantee at proof time in `refresh_combined_universe`
(`:1598-1653`):

| Line | Failure | Error prefix |
| --- | --- | --- |
| `:1613-1618` | Combined-root rediscovery produced diagnostics | "ownership could not be re-proven: combined selected-root discovery is incomplete" |
| `:1641-1646` | A manifest failed to re-resolve | "…combined selected-root universe has N unresolved manifest(s)" |
| `:1649-1651` | An initial workspace id is absent from the fresh set | "…a workspace changed identity or disappeared from the initial combined universe" |
| `:1705-1725` | Per-workspace refresh returned `None`, or a different id/root | "…workspace X did not re-resolve" / "…changed identity" |

Two layers: `refresh_combined_universe` rediscovers *and* re-resolves from the
roots, then calls `refresh_proof_universe` (`:1652`) to refresh each workspace a
second time — catching a change landing between the snapshot and the per-candidate
gate.

### `ProofUniverse` and hoisting

```rust
// cleanup.rs:2081-2084
struct ProofUniverse { fresh: Option<Vec<ResolvedWorkspace>>, failure: Option<String> }
```

`get()` (`:1779-1785`) returns the fresh set or the reason; the `(None, None)`
arm is a defensive "ownership could not be re-proven: no refreshed universe".
Hoisting happens at `:1149-1151` via `get_or_insert_with`, so it is **lazy**: a run
where every unit is skipped by policy still spawns nothing. That is the mechanism
behind the zero-spawn property, not an accident of ordering.

Hoisting does not weaken the per-candidate guarantee. Each candidate is
*additionally* re-resolved immediately before its own gates
(`refresh_candidate = true` at `:988`, implemented `:1814-1850`). The comment at
`:1561-1563` states it: "Each candidate still re-resolves its *own* workspace
through Cargo, so a workspace that changed after this snapshot is still caught
before anything is cleaned."

### `ProofRefreshMeter` and the concurrency cap

`PROOF_REFRESH_CONCURRENCY_CAP = 4` (`:1482`). `ProofRefreshMeter`
(`:1484-1517`) counts in-flight `cargo metadata` only — it checks
`args.first() == "metadata"` at `:1492`, and `run_with_env` (`:1509-1516`) passes
straight through un-metered, correctly, since env-carrying calls are the clean
itself. `refresh_proof_universe` (`:1540-1553`) sizes the pool as
`available_parallelism().clamp(1, 4)`. Work runs in batches
(`universe.chunks(concurrency)` `:1665`); a single-element batch skips thread
spawning (`:1668-1674`); results are joined in input order (`:1688-1695`) and
sorted by root at `:1737`, so output is deterministic regardless of completion
order. A thread panic degrades to `(ScanCounters::default(), None)` (`:1693`),
treated by the caller as a re-resolve failure — fail-closed. Peak concurrency
lands in `counters.proof_metadata_peak_concurrency` (`:1728-1736`) and reaches
the user through `main.rs:445-447`.

This apparatus is an efficiency concern the project tracked explicitly, not a
safety mechanism: it keeps the *cost* of proving linear. Pinned by
`final_proof_cost_is_linear_in_the_number_of_workspaces` (`:5737-5770`), which
asserts exactly 16 proof spawns for 4 workspaces and 32 for 8, with the comment
"Quadratic growth would be 4 → 64, not 4 → 16".

### `ExecutionProof` field by field

`cleanup.rs:1764-1779`. The doc comment `:1539-1555` is the contract: it "binds
the complete frozen state of ONE workspace CleanupUnit … all re-proven against
the complete bounded ownership universe discovered under clean ROOT", and is
"consumed with no intervening mutation window beyond the spawn itself. This
minimizes (but does not eliminate) TOCTOU races and fails closed."

| Field | Asserts |
| --- | --- |
| `workspace_roots: Vec<PathBuf>` | The unit's workspace roots; set to `vec![unit.root]` (`:2074`). |
| `workspace_root: PathBuf` | Workspace root; becomes the spawn `cwd` (`:1063`). |
| `root_manifest: PathBuf` | The manifest for `--manifest-path` (`:2187-2188`). |
| `canonical_members: Vec<PathBuf>` | Sorted, deduped canonical member roots; compared at `:1869-1883`. |
| `target: PathBuf` | Proven target; becomes `--target-dir` and `CARGO_TARGET_DIR`. |
| `build: Option<PathBuf>` | Proven distinct build dir, or `None` when build == target (`:2061-2072`). |
| `capability: CargoCapabilities` | `CargoBuildDirCapability` + env evidence; drives `frozen_env`'s refusals. |
| `covering: Vec<PathBuf>` | Complete dedup union of every affected physical covering root. The blast radius. |
| `pre_bytes: u64` | Pre-clean bytes of *that same union* — not a whole-workspace total. |
| `newest_activity: Option<SystemTime>` | Newest activity observed during the proof; feeds `min_inactive_seconds`. |
| `ownership: OutputOwnershipClass` | Hard-coded `PrivateBounded` at `:2093` — a postcondition, not a measurement: the proof only completes for private groups. |
| `frozen_env: Vec<(OsString, OsString)>` | The exact env for the child; built `:2096`, attached `:2097-2100`. |

`type RevalidatedContext = ExecutionProof` (`:1384`) is a compatibility alias
still used by `frozen_env` and `clean_args`.

### `final_cleanup_proof` — the walkthrough

The doc comment `:1741-1765` enumerates twelve gates. Execution order in
`final_cleanup_proof_roots` (`:1795-2101`):

1. `:1808` — `unit_block_reason_roots` re-run "defensive: the caller gates before
   dispatch, and the final proof must be the single authority consumed by any
   spawn."
2. `:1813` — `hoisted.get()?`. If the universe could not be re-proven, `?` lifts
   the `&str` into a `ProofFailure` via `From<&str>` and the candidate dies.
3. `:1814-1850` — re-resolve *this* candidate, looked up by identity in the
   refreshed universe (`:1821`); missing entry, id change, root change all fail.
4. `:1852` — rebuild the complete fresh physical output graph.
5. `:1869-1928` — member set (canonicalized, sorted `:1869-1883`), package
   identity set (`:1884-1888`), target physical identity (`:1889-1920`), build-dir
   identity (`:1901-1928`). Comparisons are canonicalized so `/tmp` vs
   `/private/tmp` on macOS does not read as a change (`:1811-1812`).
6. `:1931-1956` — remap the fresh OutputSet onto the fresh graph; affected-group
   count must match (`:1932`), every fresh group `PrivateBounded` (`:1940-1948`),
   and `workspace::outermost` + `sorted_same` must reproduce exactly the same
   covering union (`:1951-1956`).
7. `:1957-1978` — no *other* workspace in the fresh universe may overlap the
   candidate's affected output in either direction. This covers symlink and
   unresolvable roots the physical graph cannot represent, via
   `provable_output_paths` (`:1467-1472`) and `resolve_as_far_as_possible`
   (`:1434-1458`). Fails closed.
8. `:1979-1997` — every covering root must exist, be a real directory, not be a
   symlink; "preflight changed: disappeared / became symlink / changed type".
9. `:1998-2047` — fresh source activity per member (`:2015-2029`), with the
   exclusion set built from the *complete fresh universe's* output set
   (`:2004-2012`), and fresh output activity + sizing per covering root
   (`:2035-2047`). Both `t >= cutoff` and `t > start` are rejected (`:2040`) — a
   future-dated mtime reads as active, not as old.
10. `:2051-2053` — `require_cargo_markers` (`:2280-2297`): every covering root
    needs a `CACHEDIR.TAG` that is a regular non-symlink file starting with
    Cargo's signature bytes.
11. `:2054-2072` — target/build resolution and the build==target collapse.
12. `:2073-2100` — assemble the provisional proof, compute `frozen_env`, attach.

**Why the work is repeated rather than reusing the scan's result: TOCTOU.** The
initial analysis (`resolve_cleanup_scope:1281-1330`) ran at `scan_start`; the
proof runs immediately before the spawn. In between a workspace can be
re-pointed, gain a member, have its target redirected, gain a second owner, or be
touched. The comment `:1058-1060` is the honest statement of the residual risk:
the proof "was generated immediately before spawn and is consumed here with no
intervening mutation window beyond the spawn itself (fail-closed TOCTOU
minimization, not elimination)."

**On a conflict the proof aborts the candidate. It does not narrow, and it does
not continue.** Every gate above returns `Err(...)`; there is no partial-success
path and no "clean the subset that still looks safe." The rationale is
structural: one `cargo clean` affects the whole covering union, so no narrower
destructive action exists. Confirmed at the call site (`:979-996`): an `Err`
becomes a `Skipped` row via `failure.code` and the loop `continue`s.

### How a proof failure becomes observable

`ProofFailure { code, detail }` → the `skipped` closure `:863-880` → a
`CleanResult { outcome: Skipped, reason_code: failure.code, before_bytes:
Some(unit.bytes), detail: failure.detail }`. `code` is assigned explicitly at
each proof gate via `ProofFailure::new`, so rewording an error string cannot
reclassify a machine-facing reason code; the unit-wide gate derives its code
from the unit itself (`unit_skip_code`). Only the Cargo-refresh boundary still
derives `code` from text through `proof_skip_code` (`:1218-1244`), a substring
matcher kept as a fallback for pass-through Cargo/discovery strings with no
typed site. The comment `:1231-1233` records the L9 correction: "changed
identity", "vanished", "disappeared" classify as *change*, while a genuine
Cargo/discovery failure classifies as *unproven*. The test `:5773-5799` pins
those five mappings.

---

## 7. The pre-spawn decision

`execute_pre_spawn_decision` (`:2108-2126`) is a 19-line wrapper: allocate
`ScanCounters::default()`, call `final_cleanup_proof`. Its doc comment
(`:2103-2107`) says it exposes "the same non-mutating gate set Execute runs
before spawn so tests can compare candidate dispositions with Simulate on
identical state without invoking Cargo clean."

**Verified finding: it has no callers.** `grep -rn 'execute_pre_spawn_decision'
src/` returns exactly one hit — its own definition at `:2108`. Not `main.rs`,
not `lib.rs`, not the test module. The public `final_cleanup_proof` is likewise
production-dead: its only references are `execute_pre_spawn_decision:2117` (itself
dead) and the test at `:3812`.

The production pre-spawn authority is the **private**
`final_cleanup_proof_roots`, called at `:979` with `refresh_candidate = true`
(`:988`). This is *stricter* than the public wrappers, which pass
`refresh_candidate = false` (`:1790`, `:1804`): production re-resolves the
candidate itself; the public path does not.

So, corrected against the common framing: the last gate before any external
process is spawned is `final_cleanup_proof_roots` as invoked at `:979`, not
`execute_pre_spawn_decision`. The latter is an unused public affordance —
possibly useful to an external consumer, but a reviewer must know it is not what
protects a real run, and that `overview.md:193-195` cites both function names as
if they were co-equal production gates.

### Why the zero-spawn property matters

For a blocked or empty cleanup the correct behaviour is to spawn no processes at
all — not "spawn, then decide not to delete." `plans/registry.md:65` (M008A
closure) lists "all-mode zero-spawn assertions" among the milestone evidence. The
concrete assertions in this module:

| Test | Lines | Assertion |
| --- | --- | --- |
| `unresolved_discovered_manifest_blocks_every_cleanup_mode_before_clean` | `:2813` | All three modes; `clean_calls().is_empty()` at `:2745-2748` ("zero clean and dry-run invocations"); `results.is_empty()` `:2730-2733`; exact `scope_blocked` string `:2734-2740`. |
| `initial_shared_overlap_is_decided_conservatively_without_proof` | `:5738` | All three modes; `proof_workspaces_refreshed == 0` `:5575-5578` ("gate blocked before any ownership proof"); `clean_calls().is_empty()` `:5579`. |
| `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` | `:4381` | All three modes; policy rejection precedes any clean. |
| `fresh_source_and_output_age_policy_rejects_before_any_cargo_clean` | `:4431` | All three modes. |
| `preview_is_default_and_simulate_invokes_no_clean` | `:2778` | Simulate: `clean_calls().is_empty()` `:2688-2691`. |
| `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls` | `:3913` | Simulate reaches cleanable with zero clean calls; the same unit proves `Ok` through `final_cleanup_proof` with a fake runner. |
| `combined_roots_share_one_ownership_proof_and_simulation_never_cleans` | `:6061` | Two roots, one shared universe proof: 6 metadata + 2 locate; zero cleans. |
| `simulation_parity_missing_and_invalid_markers_both_skip` | `:3853` | Marker failures skip identically in Simulate and Preview. |

**Precision required:** "zero-spawn" here means **zero `clean` invocations**, not
zero processes. `cargo locate-project` and `cargo metadata` still run during
discovery and resolution (`:1292`, `:1609`, `:1526`), and
`runtime_clean_capabilities` (`:2203-2215`) spawns `cargo --version` when a
selector is present. No test in this module asserts that *no* subprocess is
created.

**Relationship between the two functions.** `execute_pre_spawn_decision` is
`final_cleanup_proof` with fresh counters; `final_cleanup_proof` is
`final_cleanup_proof_roots` with a locally built universe and
`refresh_candidate = false`. The gate logic is identical in all three — same
twelve checks, same `unit_block_reason_roots` first line. The only differences
are who supplies the universe and whether the candidate is re-resolved
individually. `final_cleanup_proof_roots` at `:979` is the single authority
consumed by any spawn, and `:1806-1807` says so.

---

## 8. Selection policy and outcome

### `CleanupPolicy`

`cleanup.rs:283-289`, `#[derive(Default)]`:

| Field | Default | Enforced at | Notes |
| --- | --- | --- | --- |
| `min_reclaimable_bytes: u64` | `0` | `:948` (scan `unit.bytes`) and `:997` (fresh `proof.pre_bytes`) | Checked **twice** — cheaply before the proof, authoritatively after. |
| `min_inactive_seconds: Option<u64>` | `None` | `:1010-1036` | Only **after** the proof, against `proof.newest_activity`. A missing timestamp is treated as too recent (`:1011-1020`), not as "no data, allow". |
| `include: Vec<String>` | empty | `:881-890` | Empty ⇒ no include filtering. |
| `exclude: Vec<String>` | empty | `:891-900` | Always applied; an empty `GlobSet` never matches. |

Config interaction is `main.rs:321-338`. CLI overrides configured per field, and
for the two list fields an **empty CLI list falls back to config**:
`min_reclaimable_bytes: min_reclaimable_bytes.unwrap_or(configured.min_reclaimable_bytes)`,
`min_inactive_seconds: older_than.or(configured.min_inactive_seconds)`,
`include`/`exclude`: `if … .is_empty() { configured.… .clone() } else { … }`.
Consequence: a user cannot express "ignore the configured exclude list" from the
CLI, because omitting the flag is indistinguishable from passing an empty list.

### Where selection sits relative to proof

`config.toml:38` says the policy is "applied after complete ownership
resolution." **Verified — and the real ordering is more granular.** Within the
unit loop (`:862-1168`):

```text
881  include globs           ─┐
891  exclude globs            │
901  selector capability      ├─ BEFORE the unit gate and BEFORE the proof
925  package spec resolvable  │   (no clean call, no proof)
938  selector + min-size      │
948  min_reclaimable_bytes    ─┘   (uses scan-time unit.bytes)
964  unit_block_reason_roots  ─┐
976  ProofUniverse::refresh    │
979  final_cleanup_proof_roots│  proof
997  min_reclaimable_bytes     │   (uses FRESH proof.pre_bytes)
1010 min_inactive_seconds    ─┘   (needs proof.newest_activity)
1038 mode dispatch → spawn
```

Ownership resolution does complete before the loop (`:814-857`, with the
incomplete-universe aborts at `:795-810` and `:839-853`), so `config.toml:38` is
accurate. But selection is *split*: glob and capability filtering plus the first
size check are pre-proof; the authoritative size check and the age check are
post-proof. No path lets a selector narrow a proof — a selector only reaches
`clean_args` (`:2192-2199`) and affects the spawn.

### `CleanupSelector` and globset

`Profile(String)` / `Package(String)` filter what the *invocation* cleans, not
which units are considered: `--profile <p>` / `--package <q>` (`:2192-2199`).
Package specs are resolved against the workspace's own package list before any
spawn, via `resolve_package_spec` (`:2217-2246`): exact name, exact id,
`name@version`, and `name@1.2`-style partial versions through
`semver_spec_matches` (`:2260-2278`, which accepts a 1- or 2-component numeric
prefix). Ambiguity is rejected twice — multiple matches (`:2235`) and, even with
one match, a duplicate package *name* anywhere in the workspace (`:2237-2244`). A
spec that fails becomes `SelectorInvalid` (`:925-937`).

Selectors interact with policy in a way that is easy to miss: a selector plus a
non-zero `min_reclaimable_bytes` skips the unit with
`SelectorEstimateUnavailable` (`:938-947`), because `before_bytes` is the whole
output union, not the selector's share. There is also a runtime capability gate:
`runtime_clean_capabilities` (`:2203-2215`) parses `cargo --version` and asks
`workspace::clean_capabilities_from_version`; an unsupported runtime yields
`SelectorUnsupported` before the unit is considered (`:901-924`).

Matching uses `globset`. `compile_policy_globs` (`:1175-1185`) builds a
`GlobSet` per field, mapping a bad pattern to
`AppError::Config("invalid cleanup policy glob ...")`. Patterns match
`unit.root` — the **workspace root**, not the output directory (`:881`, `:891`).
Full `globset` syntax is available (`*`, `**`, `?`, `[…]`, `{a,b}`), and because
`Glob::new` is called without `literal_separator(true)`, a single `*` crosses `/`
— `**` is the correct way to say "any depth".

### Disposition, outcome, result, report

`PolicyDisposition` (`:165-174`) is serde snake_case, mapped at
`output.rs:246-257`. It is set only on rows that reached the policy stage:
`Selected` on a row that passed every gate and spawned (`:1047`, `:1079`, `:1098`,
`:1115`, `:1135`), or one of the five dispositions on a policy skip. A row
blocked by the ownership gate or the proof carries `None` (`:865` default) — so
**`policy_disposition` is not a "was it considered" flag**; it means "policy made
this call."

`CleanResult` (`:77-93`) is one row per unit. `CleanReport` (`:176-193`):

| Field | Set at | Meaning |
| --- | --- | --- |
| `results` | loop `:862-1168` | One `CleanResult` per unit that produced a row. |
| `diagnostics` | `:795-808`, `:1161` | Diagnostic count; also incremented on post-clean measurement failure (`:1161`). |
| `failed` | `:1072`, `:1091` | Cargo spawn/exit failures. Drives exit code 1. |
| `mode` | `:798`, `:829` | The run's `CleanMode`. |
| `counters` | `:808`, `:851`, `:1171` | `ScanCounters` including all `proof_*` fields. |
| `scope_blocked` | `:804`, `:841` | `Some` only for scan-time incompleteness. Renders alone and returns early (`:237-240`). |
| `unresolved_ownership` | `:849` | `UnresolvedOwnershipParticipant` list, for audit. |
| `selected_roots` | `:799`, `:830` | The collapsed root set actually cleaned. |
| `discovered_manifests` / `resolved_workspaces` / `units_considered` | `:800`/`:832`/`:833` | Counts for the combined scope header. |
| `effective_policy` | `:803`, `:836` | The config/CLI merge result, echoed so JSON consumers see what applied. |
| `selector` | `:837` | The active selector, if any. |

`render()` (`:223-363`) sorts rows size-descending with a stable path tie-break
(`:252-258`, deliberate per `:249-251`), prints the combined scope header when
roots are set (`:226-236`), and closes with a mode-specific footer
(`:337-360`). With a selector active, size labels switch from "before"/"after" to
"output-union before"/"output-union after" (`:311-324`) because the numbers are
the whole union — the same distinction `output.rs:174-184` makes by routing
`before_bytes` to a separate `selector_estimate_bytes` field.

---

## 9. Invariants and edge cases

### Can any code path reach a clean spawn or a deletion without a fresh proof?

**No.** The only two `runner` calls in the production module are at `:1256-1258`,
inside the `CleanMode::CargoPreview | CleanMode::Execute` arm, and `proof` is bound at
`:1152` in the immediately preceding statement with no `continue`, no loop
boundary, and no intervening mutation. The module header comment (`:9-11`)
claims "No earlier RawGroup is consulted for mutation decisions after proof
generation", and the structure backs it: the `CleanResult` literals in the spawn
arm (`:1264-1351`) copy from `proof.*`, never `unit.*`. `unit_block_reason_roots`
is re-run as gate 1 of the proof itself (`:1999`) precisely so the proof is
self-sufficient.

**Caveat stated precisely:** non-clean spawns happen without a proof and
legitimately so — `cargo locate-project` and `cargo metadata` during resolution
(`:1483`, `:1717`, `:1813`), and `cargo --version` for selector capability
(`:912`). The invariant is about the *destructive* spawn.

### Is `Simulate` guaranteed to spawn nothing destructive?

**Structurally, not by convention.** The `match mode` at `:1211` routes
`CleanMode::Simulate` to an arm (`:1212-1229`) containing no `runner` call at
all — it pushes a `CleanResult` and moves on. `clean_args` is reachable only from
the `CargoPreview | Execute` arm (`:1253`) and adds `--dry-run --verbose` only for
`CargoPreview` (`:2391-2393`). So `Simulate` is prevented twice over: it never reaches
the dispatch, and if it somehow did, the arg builder would not produce a
non-dry-run clean. No flag combination turns `Simulate` into a clean; `cli.rs:119-122`
makes `--dryrun` conflict with `--yes`. `Simulate` does run the *full* proof
(`:1152` is mode-independent) — it is a rehearsal of the destructive path, not a
cheaper one.

### What happens when the ownership universe is incomplete?

**Two different answers depending on when incompleteness is detected. This is the
most important distinction in the module.**

**(a) Incomplete at scan time → the whole transaction aborts.** Two sites, both
*before* the unit loop, both returning early with `scope_blocked` and zero
results: discovery diagnostics `:1336-1356` ("combined cleanup ownership universe is
incomplete: N discovery diagnostic(s); resolved W workspace(s), considered U
cleanup unit(s); no cleanup commands were run") and
unresolved manifests `:1359-1384` ("cleanup ownership could not be proven: N
discovered Cargo manifest(s) did not resolve; no cleanup commands were run", with
the full participant list retained and rendered bounded at `:510-560`).
Neither proceeds partially:
`resolve_cleanup_scope` returns `units: Vec::new()` when unresolved is non-empty
(`:2561-2568`), so no unit even exists. Pinned by
`unresolved_discovered_manifest_blocks_every_cleanup_mode_before_clean` (`:4738`,
all three modes), `unresolved_cargo_manifest_blocks_entire_cleanup_scope`
(`:6491`), `combined_roots_classify_cross_root_output_as_shared_in_every_mode`
(`:9408`) and `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes`
(`:9451`).

**(b) Incomplete at proof time → the transaction does *not* abort; every
candidate is skipped instead, and the run exits 0.** The hoisted universe is
built once (`:1149-1151`) and shared, so a `ProofUniverse.failure` makes
`hoisted.get()` (`:2004`) return `Err` for *every* candidate. The result is a
report where every row is `Skipped` with `SkippedOwnershipUnproven` or
`SkippedChangedBeforeCleanup` — but `report.scope_blocked` stays `None` and
`report.failed` stays `0`, because neither early-return site is reached.
`main.rs:452-456` therefore returns exit code **0**. Tested at
`universe_workspace_that_cannot_reresolve_fails_closed` (`:6013-6039`): the row
is `Skipped` with "could not be re-proven" and `clean_calls()` is empty —
safety fully preserved, the *signal* is a normal-looking skip. Fail-closed is
correct and consistent; the reporting is inconsistent with case (a). A run where
the universe cannot be re-proven and nothing was cleaned is reported like a run
where every workspace was legitimately inactive, and exits 0.

### The report-only proof-island partition (M015)

Both scan-time block sites also compute a proof-island partition
(`:1350-1355`, `:1376-1381`) and carry it on the report
(`CleanReport.proof_partition`, `:446`), rendered as a bounded advisory section
(`partition_section`, `:2196-2322`) after the participant listing. The machine
surfaces never see it: no JSON field, no log-line field, no exit-code change
(locked by `cleanup_json_result_key_set_is_stable`, `src/output.rs:658`, and the
subprocess case `tests/cli_contract.rs:1930`).

The partition answers ADR 004's question — *what constitutes independently
proven non-overlap?* — without enforcing any answer. Its sharp edge is
byte-capability (`domain.rs:178-195`): only unknowns Cargo itself refused
(`locate` exit nonzero) own provably no bytes and merge nothing; every other
class (transport, unparseable output, lost workers, metadata, identity,
discovery holes) merges every island, because unproven outputs could overlap
any island's. Attribution (`workspace::attribute_unknown`, `:419-438` in
`workspace.rs`) assigns a refused manifest to the island of the workspace Cargo
named, or to a scope-level note when nothing resolved was named; the union-find
(`compute_proof_partition`, `:2023-2159`) merges islands on proven physical
output overlap (equality or canonical-prefix, component-wise) and collapses to
one blocked island on any merge cause (`PartitionMergeCause`, `:380-394`). The
environment relevant to output placement is snapshotted with the partition
(`snapshot_output_env`, `:357-362`); the mismatch detector is proven by tests
(`output_env_matches`, `:367-372`) for M016 to wire as a merge.

Parity is the load-bearing test:
`partition_reports_islands_but_engine_still_blocks_with_zero_spawns_in_every_mode`
(`:5026`) stages two output-disjoint workspaces plus a refused orphan — the
certificate reports two would-proceed islands — and asserts the engine still
blocks with empty results and zero clean spawns in all three modes. The hostile
controls (`:5078-5390`) each fail against a naive skip (no union edges,
unknowns dropped): cross-root redirect, nested-output prefix overlap,
metadata-failure overlap with a zero-spawn engine assertion, symlinked output
(unix), build-dir-only env moves, and H-2 growth both as predicate
(`universe_manifests_stable`, `:2330`) and end to end
(`mid_run_manifest_growth_blocks_remaining_scope`, `:5326`). The one listed
shape that is *not* hostile — a target nested under another workspace's
*sources* — splits under both and ships as a documented positive case, not a
control. Activation (per-island proof and spawn) is explicitly deferred to
M016, gated on ADR 004 acceptance; live island-hit data for that decision is
attached to the M015 closure record, not to this section.

### Per-island activation (M016)

`clean --partitioned` (`cli.rs:149`, `CleanupRequest.partitioned` `:260`,
threaded through `run_engine_guarded` into
`clean_with_roots_policy_selector_guarded` `:1365`) dispatches
proceed-islands instead of returning at the scan-time block sites
(`:1454`, `:1485`, via `retain_proceed_island_units` `:1304`).
Everything else is unchanged: grouping, classification, overlap evidence,
the twelve-gate final proof, and the hoisted combined universe — which
still contains held islands' workspaces — read the full universe, and the
refresh still re-proves against it. Only the spawn decision partitions.

Three load-bearing details:

1. **Merge still means hold.** The certificate semantics are M015's: any
   merge cause collapses to one blocked island, so a held island is always
   the merged one and proceed islands never overlap. Shared-output
   workspaces merge into one *proceed* island whose units the unit gate
   then skips (`SkippedShared`): the island says go, the gate says no, zero
   spawns. Held islands never become units or rows; they live on the
   islands axis (JSON `islands`, human island sections).
2. **The scan-time unknowns are tolerated by the refresh, not forgotten.**
   `refresh_combined_universe` (`:3406`) tolerates exactly the manifests
   already unresolved at scan time (`:3454`); anything newly unresolvable
   still fails the run. Flag-off runs never reach the refresh with
   unresolved present, so the tolerance preserves today's behavior exactly.
3. **The environment is re-verified per island before its first dispatch.**
   The partition's provenance snapshot is re-compared
   (`output_env_matches` `:370`, check at `:1860-1872`); any mismatch
   merges the remaining islands into one blocked scope before any further
   spawn, in every mode (Simulate rehearses the check so it predicts
   Execute).

The scan-time block message keeps its pinned substrings with an additive
counts-only suffix naming acted-on islands
(`append_partitioned_suffix` `:1329`, applied at `:2053`). The human
render shows completed rows, the block, and per-island outcomes together
(`:571`, `partition_outcome_section` `:2529`). The machine surface gains
one additive field (`CleanupV1.islands`, `output.rs:57`; `IslandV1`
`output.rs:70` with roots/verdict/cause/passengers, cause rendered in
`partition_section` vocabulary via `partition_merge_cause_label`
`:2364`). Exit aggregation is untouched: `scope_blocked` stays set, so
exit 1 falls out structurally. `-i` offers proceed-island rows with held
islands as non-selectable context (`list_cleanup_candidates` `:2669`,
`held_context`, `island_held_view` `:2779`); a merged universe refuses
exactly as before.

Tests (all `#[test]` in `cleanup.rs` unless noted): flag-off parity
`partitioned_flag_off_matches_unpartitioned_engine` (`:5860`); the
activation kennel `partitioned_execute_cleans_every_provable_island`
(`:5939`) plus Simulate/CargoPreview twins; the passenger rule
(`partitioned_passenger_rides_along_without_merging`); hostile twins
(redirect, nested, metadata-failure, symlinked-output unix, mid-run
growth); the gate-8 control
(`partitioned_proof_refuses_where_partition_would_proceed`); the
passenger-redirect control; the environment child-process probe
(`partitioned_environment_change_merges_remaining_islands` `:6540`,
which spawns the test binary so the process-env mutation cannot race
parallel tests); the listing pair (offer + refusal with held context);
the JSON key-set update (deliberate, `output.rs`); the log-line contract
test; subprocess G1/G1c surfaces (`tests/cli_contract.rs:2008`,
`:2083`, `run_island_scenario` `:1373`). Every engine twin asserts zero
cross-island cleans and exact per-island verdicts, and each fails against
a dispatch-everything naive by construction.

### What happens when the proof detects a mid-transaction change?

The candidate is skipped; the run continues to the next unit.

| Change | Line | Message |
| --- | --- | --- |
| Member set changed | `:1881-1883` | "workspace changed before cleanup; skipped: member set changed" |
| Package identity set changed | `:1884-1888` | "…Cargo package identity set changed" |
| Target redirected | `:1913-1920` | "…target changed (was X, now Y)" |
| Build dir changed | `:1921-1928` | "…build-dir changed (was X, now Y)" |
| Group count changed | `:1932-1936` | "…physical output group set changed" |
| Ownership degraded | `:1940-1948` | "ownership changed before cleanup; skipped: output X is now Y" |
| Another workspace now overlaps | `:1961-1978` | "…another discovered workspace (W) now overlaps cleaned output P" |
| Covering root gone/type/symlink | `:1981-1997` | "preflight changed: …" |
| Source became active | `:2017-2020` | "…source became active" |
| Output became active | `:2040-2042` | "…output became active" |
| Marker invalid | `:2053` → `:2280-2297` | "Cargo CACHEDIR.TAG marker is missing…" |

Covered by `RaceRunner`-based tests: `race_target_replacement_skips_before_clean`
`:4426`, `race_covering_root_disappears_skips` `:4630`,
`race_output_becomes_recent_skips` `:4645`, `race_source_becomes_recent_skips`
`:4667`, `race_marker_disappears_and_changes_skip` `:4861`,
`race_covering_becomes_symlink_skips_and_changes_classification` `:4891`,
`cross_workspace_overlap_race_into_target_skips_candidate` `:5829`,
`cross_workspace_overlap_race_into_distinct_build_skips_candidate` `:5873`,
`candidate_target_or_build_change_still_skips` `:6081`. `BuildChangingRunner`
covers the build-dir capability flip (Equal → Distinct) in
`simulation_parity_changed_build_dir_and_unsupported_capability_both_skip`
`:4522`. `MetadataMutationRunner` covers a workspace-root change, a newly added
member, and malformed metadata JSON from the second metadata call onward, in
`direct_metadata_refresh_detects_workspace_root_and_member_changes` `:4453`.

### Are two units sharing physical bytes prevented from racing?

**Yes, by construction rather than by locking.** Three independent mechanisms:

1. **Classification.** Two workspaces pointing at the same physical output produce
   one group with two owners, classified `Shared`, failing
   `unit_block_reason_roots:618` for every unit that touches it.
2. **Initial-graph decision.** Overlap present in the *initial* complete graph is
   decided conservatively from that graph alone, before any proof work —
   `initial_shared_overlap_is_decided_conservatively_without_proof` (`:5549`)
   asserts `proof_workspaces_refreshed == 0`, i.e. the gate blocks before the
   proof is even built. A shared pair cannot reach a spawn via either unit.
3. **Fresh re-check.** Proof step 7 (`:1957-1978`) re-derives the graph and
   re-checks that *no other* workspace overlaps the candidate's covering union in
   either direction, including symlink and unresolvable roots the graph cannot
   represent. The unit loop at `:862` is strictly sequential — no unit spawns
   until the previous spawn has returned.

`PerPathFailRunner` (`:3211`) tests a different property: that one unresolvable
manifest blocks a *sibling* workspace that is perfectly clean — the completeness
property of the third question, not a byte race. The byte-race property is
covered by `StagedCargo`'s multi-workspace fixtures (`:4939`, `:5529`, `:5559`,
`:6014`).

### Is deletion by `cargo clean`, by `std::fs::remove_dir_all`, or both?

**By `cargo clean` only.** Verified three ways: `grep -n 'remove_dir_all' src/*.rs`
finds no occurrence in `cleanup.rs` production — the four hits (`:4189`, `:4444`,
`:5650`, `:5671`) are all inside the test module, and the module's own trailing
comment `:2319-2321` says "direct deletion remains prohibited (no `remove_dir_all`
in production)"; the only mutation path is the child process at `:1064-1068`;
and ADR 001 `:133-136` requires "invokes Cargo rather than recursively deleting
output… Cargo-cleanme never deletes Cargo output trees directly."

**Can a direct filesystem removal bypass Cargo's bookkeeping? No — because there
is none.** Cargo's invariants (lock files, `CACHEDIR.TAG` handling, fingerprint
bookkeeping) stay consistent by construction, and the tool cannot accidentally
delete a `target/` tree Cargo believes is live. The cost is inheriting Cargo's
deletion semantics wholesale, including future changes to what `cargo clean`
removes — the intended trade (ADR 001 `:162-165`). The module does read the
filesystem (measures, reads `CACHEDIR.TAG` `:2288`, touches mtimes through
measurement) but never writes.

### Worst case if the process is killed mid-transaction

**No cleanup.rs operation is non-atomic in a way that can corrupt a workspace,
because cleanup.rs performs no mutation of its own.** The only mutation happens
inside the `cargo clean` child. Killed before the spawn: nothing happened.
Killed during `cargo clean`: the observable state is a partially emptied target
directory, which Cargo treats as a cache miss and rebuilds; no workspace *sources*
are damaged, because a source tree used as an output root is classified
`Uncertain` upstream and can never be a cleanup target
(`workspace.rs:1217-1220`; test `source_tree_as_output_root_is_never_cleaned`
`:6242`). Killed after the spawn but before `measure_union` (`:1126`): the
cleanup happened but no `after_bytes`/`observed_decrease` is reported —
under-reporting, never over-reporting.

The two variables that could redirect a child to the wrong directory are both
pinned: `--target-dir` (`:2190-2191`), `CARGO_TARGET_DIR` (`:2147-2150`),
`CARGO_BUILD_BUILD_DIR` (`:2151-2170`). `--locked` (`:2186`) prevents a lockfile
update. I found no non-atomic window here that could leave a workspace in an
unintended state.

### Can a symlinked output root redirect a deletion outside the intended scope?

**Within one process, no. Across the proof→spawn window the race is real and the
code says so.**

| Site | Check |
| --- | --- |
| `validate_cleanup_root:1541` | A clean ROOT must be a real, non-symlink directory. |
| `covering_is_authorized_roots:517-522` | A symlink allowed root is `Err`. |
| `workspace.rs:1249-1251` | A symlink output root marks the whole workspace `uncertain_ws` upstream. |
| Proof `:1989-1991` | Every covering root re-checked with `symlink_metadata`; a symlink is "preflight changed: covering root became symlink". |
| Proof `:1961-1978` | Symlink/unresolvable *other* roots compared via `resolve_as_far_as_possible` so they cannot be silently ignored. |
| `measure_union:2305-2307` | Post-clean measurement refuses a path that changed type or became a symlink. |
| `resolve_as_far_as_possible:1434-1458` | Canonicalizes the deepest existing ancestor, so a symlinked ancestor (macOS `/var`) is resolved rather than compared lexically. |

Tests: `symlink_allowed_root_rejected_and_shared_forbidden` `:3351`,
`symlink_build_identity_skips_entire_unit` `:5639`,
`cross_workspace_symlink_into_candidate_output_fails_closed` `:5912`,
`cross_workspace_symlinked_ancestor_into_candidate_output_fails_closed` `:5986`,
`race_covering_becomes_symlink_skips_and_changes_classification` `:4891`. The
honest limit is `:1058-1060`: between the symlink check at `:1989` and the
child's own `open()` there is a window, minimized rather than eliminated. Closing
it entirely would require `cargo clean` to re-verify, outside this design.

### Does cleanup ever delete anything outside a workspace's own `target/`?

**No, and the code is stricter than the ADR's framing.** ADR 001 `:92-93` defines
`PrivateBounded` relative to "authorized cleanup boundaries". The implementation
is narrower: `workspace.rs:1217-1219` requires every covering root to be **strictly
below the workspace root**, disjoint from every member source root, with every
universe root represented in the graph. Anything outside the workspace root is
`ExternalUnproven` (`:720`) and inventory-only, permanently (§5).

Two secondary facts: the *invocation* is still
`cargo clean --manifest-path <workspace root>`, so what Cargo removes is whatever
Cargo believes belongs to that workspace — the module's job is to ensure that
belief is safe, which it does by proving the union before the spawn and freezing
the target via two independent mechanisms. And `build` output is a distinct root
from `target` inside the same unit (`:2061-2072`), covered by the same
workspace-root rule, with the union deduplicated so an equal build==target counts
once (`equal_and_nested_target_build_are_one_unit_and_one_invocation` `:5787`).

### Is the exit-code behaviour correct?

`main.rs:452-456` returns `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`report.failed` is incremented only at `:1072` (spawn error) and `:1091` (non-zero
exit). `scope_blocked` is set only at `:804` and `:841` — the two scan-time
incompleteness sites.

**Is `scope_blocked` the right signal for "partial failure"? Partly.** It is
right for *scope-level* blocking and correctly forces exit 1 there. Two gaps:

1. **Proof-time universe failure is not a scope block.** Every row is `Skipped`
   because the shared universe could not be re-proven, nothing was cleaned, and
   the exit code is 0 (third question above). Safety intact; signal not.
2. **A run where every unit is legitimately `Skipped` also exits 0** — correct,
   since skipping is not failure. But a machine consumer then cannot distinguish
   "nothing to clean" from "could not prove, so cleaned nothing" without
   inspecting `reason_code` per row. The data is available, since
   `CleanupReasonCode` is a stable vocabulary; only the aggregate conflates it.

I found no case where exit 0 follows an unsafe clean, and no case where a Cargo
failure goes uncounted. The gap is consistency of the *blocked* signal, not a
safety hole.

---

## 10. Testing

**Numbers (verified by grep):** 133 `#[test]` functions; the test module opens at
`cleanup.rs:4295` (`#[cfg(test)]` at `:4294`), so production is lines 1-4293 and tests 4295-11229 — 6936 lines, 62% of the file. No fixture files are
needed: workspaces are built on disk (`ws_root` `:9590`, `cargo_output` `:9603`,
`sibling_ws_fixture` `:9638`, `valid_fixture` `:4614`) and mtimes backdated
(`backdate` `:4646`, `touch_future` `:8318`) so recency gates run without
sleeping.

Line numbers in the group table below were re-derived for the M015 row only.
Older rows still cite pre-M015 numbering (the module grew ~490 production
lines under M015 plus the uncommitted delta beneath it); a full
re-derivation pass over this dive is recorded as follow-up work in the M015
closure record, not silently claimed here.

### Grouped by invariant protected

| Group | n | Tests (line) |
| --- | --- | --- |
| Authorization cannot promote | 4 | `external_unproven_inside_root_remains_inventory_only` `:3113`; `external_unproven_outside_root_never_authorized_by_allowed_root` `:3293`; `external_outside_root_allowed_via_configured_root` `:4061`; `symlink_allowed_root_rejected_and_shared_forbidden` `:3351` |
| Ownership gating / unit gate | 11 | `gate_blocks_mixed_class_units_in_every_combination` `:5729`; `private_target_with_external_unproven_build_skips_entire_unit` `:5494`; `private_target_with_shared_build_skips_entire_unit` `:5536`; `external_unproven_target_with_private_build_skips_entire_unit` `:5569`; `uncertain_build_identity_skips_entire_unit` `:5598`; `symlink_build_identity_skips_entire_unit` `:5639`; `gate_blocks_unmeasured_and_unmapped_unit_members` `:5760`; `private_bounded_inside_workspace_is_authorized` `:3242` |
| Proof universe completeness | 8 | `unresolved_discovered_manifest_blocks_every_cleanup_mode_before_clean` `:2934`; `unresolved_cargo_manifest_blocks_entire_cleanup_scope` `:3636`; `universe_workspace_that_cannot_reresolve_fails_closed` `:6013`; `cross_workspace_symlink_into_candidate_output_fails_closed` `:5912`; `cross_workspace_unresolvable_root_into_candidate_output_fails_closed` `:5948`; `cross_workspace_symlinked_ancestor_into_candidate_output_fails_closed` `:5986`; `combined_roots_classify_cross_root_output_as_shared_in_every_mode` `:6499`; `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes` `:6542` |
| Pre-spawn / proof refresh | 11 | `revalidation_detects_changed_target_and_skips` `:3537`; `direct_metadata_refresh_detects_workspace_root_and_member_changes` `:4453`; `initial_shared_overlap_is_decided_conservatively_without_proof` `:6042`; `candidate_target_or_build_change_still_skips` `:6081`; `missing_marker_skips_instead_of_failing` `:4114`; `race_marker_disappears_and_changes_skip` `:4861`; `race_covering_root_disappears_skips` `:4630`; `race_covering_becomes_symlink_skips_and_changes_classification` `:4891`; `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` `:4685`; `fresh_source_and_output_age_policy_rejects_before_any_cargo_clean` `:4735`; `race_output_becomes_recent_skips` `:4645` |
| Selection policy | 5 | `profile_selector_capability_is_bounded_to_qualified_runtime_versions` `:2836`; `package_specs_require_one_authoritative_workspace_identity` `:2853`; `excluded_workspace_still_makes_nonexcluded_shared_output_non_cleanable` `:4788`; `simulation_parity_changed_build_dir_and_unsupported_capability_both_skip` `:4522`; `policy_dispositions_have_stable_json_names` `:2787` |
| Mode semantics / parity | 8 | `preview_is_default_and_simulate_invokes_no_clean` `:2899`; `simulation_candidate_set_matches_execute_preflight` `:3101`; `simulation_parity_missing_and_invalid_markers_both_skip` `:4157`; `simulation_parity_valid_private_reaches_cleanable_with_zero_clean_calls` `:4217`; `simulation_parity_recent_source_and_output_both_skip` `:4265`; `combined_private_roots_keep_preview_simulate_execute_parity` `:6459`; `combined_roots_share_one_ownership_proof_and_simulation_never_cleans` `:6425`; `cli_modes_map_to_preview_simulate_execute` `:2756` |
| Concurrency / cost | 3 | `proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` `:6581`; `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition` `:6652`; `final_proof_cost_is_linear_in_the_number_of_workspaces` `:6298` |
| Accounting / reporting | 10 | `execution_reports_observed_decrease_deduped` `:3466`; `final_report_lists_all_groups_and_deduped_total` `:3740`; `union_accounting_counts_each_output_root_once` `:6101`; `post_clean_disappearing_sibling_is_measured_normally` `:6139`; `post_clean_measurement_uncertainty_fabricates_no_decrease` `:6160`; `stats_account_for_final_ownership_universe_proof_work` `:6387`; `cleanup_progress_is_determinate_through_observer_path` `:4918`; `scan_analysis_total_is_determinate_through_real_path` `:4953`; `blocked_scope_render_lists_unresolved_participants_with_workspace_hint` `:3000`; `blocked_scope_render_bounds_unresolved_participant_listing` `:3058` |
| Real Cargo integration | 3 | `real_temp_project_preview_and_execute_with_redirected_target` `:3785`; `real_cargo_package_selector_preserves_sibling_workspace_artifacts` `:3920`; `multi_member_private_workspace_can_be_cleaned` `:4034` |
| Root validation / frozen env | 3 | `clean_requires_absolute_root` `:3480`; `frozen_env_sets_target_and_build_dir` `:3425`; `frozen_env_pins_the_build_directory_so_an_inherited_redirect_cannot_apply` `:6363` |
| Misc | 9 | `cleanup_reason_codes_have_stable_contract_names` `:2807`; `source_tree_as_output_root_is_never_cleaned` `:6242`; `one_invocation_one_result_progress_total_and_no_sibling_row` `:5453`; `distinct_private_target_and_build_is_one_cleanable_unit` `:5403`; `equal_and_nested_target_build_are_one_unit_and_one_invocation` `:5787`; `proof_failures_distinguish_a_change_from_an_unproven_ownership` `:6334`; `cross_workspace_overlap_race_into_target_skips_candidate` `:5829`; `cross_workspace_overlap_race_into_distinct_build_skips_candidate` `:5873`; `another_workspaces_source_tree_inside_the_cleaned_region_is_never_cleaned` `:6196` |
| Proof-island partition, report-only (M015) | 23 | Pure partition: `proof_partition_splits_disjoint_workspaces_with_no_unknowns` `:5509`; `proof_partition_merges_equal_targets_into_one_island` `:5527`; `proof_partition_merges_nested_outputs_by_prefix_overlap` `:5544`; `proof_partition_does_not_merge_string_prefix_siblings` `:5565`; `proof_partition_is_deterministic_across_input_orders` `:5581`; `proof_partition_merges_unproven_unknown_globally_with_named_cause` `:5601`; `proof_partition_assigns_refused_believed_manifest_as_passenger_without_merging` `:5632`; `proof_partition_reports_refused_manifest_without_owner_as_scope_note` `:5661`; `proof_partition_merges_discovery_holes_globally` `:5680`; `proof_partition_with_only_notes_has_no_islands` `:5704`; `proof_partition_merges_pathless_holes_with_counted_cause` `:5767`; holes extractor `discovery_holes_takes_discovery_prefix_with_pathless_remainder` `:5720`; render `partition_section_renders_islands_passengers_notes_and_provenance` `:4936`, `partition_section_bounds_islands_and_causes` `:4984`; parity `partition_reports_islands_but_engine_still_blocks_with_zero_spawns_in_every_mode` `:5026`; hostile controls `redirect_outputs_merge_under_certificate_but_split_under_naive_skip` `:5078`, `nested_outputs_merge_under_certificate_but_split_under_equality_naive` `:5114`, `unresolved_manifest_overlap_merges_and_engine_blocks_with_zero_spawns_in_every_mode` `:5152`, `symlinked_output_merges_under_certificate_but_splits_under_naive_skip` `:5211` (unix), `build_dir_move_detected_by_full_env_check_but_missed_by_target_only_naive` `:5256`, `universe_stability_predicate_detects_growth_that_naive_stub_misses` `:5297`, `mid_run_manifest_growth_blocks_remaining_scope` `:5326`; documented non-control `target_nested_under_foreign_sources_splits_under_both_and_is_not_a_control` `:5392` (pre-M016 numbering; the module grew ~1300 lines under M016, mostly tests) |
| Per-island activation (M016) | 15 | flag-off parity `partitioned_flag_off_matches_unpartitioned_engine`; activation kennel `partitioned_execute_cleans_every_provable_island` + Simulate/CargoPreview twins `partitioned_simulate_and_predict_without_spawning`; passenger rule `partitioned_passenger_rides_along_without_merging`; hostile twins (redirect, nested, metadata-failure, symlinked-output unix, mid-run growth `partitioned_mid_run_manifest_growth_blocks_remaining_scope`); gate-8 control `partitioned_proof_refuses_where_partition_would_proceed`; passenger-redirect control `partitioned_passenger_island_never_cleans_alone_after_redirect`; environment child-process probe `partitioned_environment_change_merges_remaining_islands` + `partitioned_environment_change_probe`; listing pair (offer + refusal with held context). Subprocess surfaces in `tests/cli_contract.rs`: G1 Execute/Simulate/human + G1c passenger + `-i` refusal. |

### What each fake runner exists to catch

The highest-value mapping in the module: each runner is a purpose-built
counterexample generator.

| Fake | Mechanism | Unique failure mode it catches |
| --- | --- | --- |
| `FakeCleanupRunner` `:2543` | Synthetic `locate-project`/`metadata` JSON; deletes `artifact.bin` on non-dry-run cleans (`:2639-2648`, `:2675-2684`); `unresolved_manifest` fails one manifest (`:2550`, `:2583-2593`). | The baseline: that a real clean changes the filesystem and the reported decrease matches. The only fake making post-clean measurement non-trivial. `unresolved_manifest` drives the scope-block path. |
| `ChangingTargetRunner` `:3496` | First `metadata` returns the base target; **every later** one a different `target_directory` (`:3504-3516`). | A target redirected by Cargo's *own resolution* between scan and proof — the TOCTOU failure at the module's heart, caught by a proof that trusted the scan's cached `OutputSet`. |
| `RaceRunner` `:4312` | Arbitrary mutation closure on the **second and later** `metadata` calls (`:4367-4368`) — precisely the scan→proof window. Optional `second_target`/`second_build`. | Filesystem changes landing *between* two different Cargo invocations. The general race injector: covering-root deletion (`:4630`), mtime bump to now (`:4645`, `:4667`), marker removal or content change (`:4861`), covering root replaced by a symlink (`:4891`), target replacement (`:4426`). |
| `PerPathFailRunner` `:3637` | `locate-project` fails for any manifest path containing `"bad"`; everything else resolves cleanly (`:3642-3682`). | **Partial resolution** — the dangerous shape is one unresolvable manifest plus one perfectly cleanable workspace, and the temptation to clean the cleanable one. Asserts `unresolved_ownership == 1`, `results.is_empty()`, and a `scope_blocked` render (`:3730-3736`). The direct test of §9's incomplete-universe answer. |
| `BuildChangingRunner` `:4534` | `build_directory` starts equal to `target_directory`, becomes distinct on later `metadata` calls (`:4565-4569`). Explicitly does *not* consume a metadata call for `clean` (`:4551-4560`). | A build-dir capability flip (`Equal` → `Distinct`) mid-transaction, where the *shape* of the affected output changes rather than its location. Also guards a subtle fake bug: a fake counting a `clean` as a `metadata` call would corrupt the call-index arithmetic. |
| `StagedCargo` `:5012` | Multi-workspace Cargo; `stage(index, from_call, target, build)` changes output from the Nth `metadata` onward (`:5062-5070`); `fail_metadata_from` (`:5072-5074`); `on_clean` hook mutates the filesystem *during* the clean (`:5076-5078`); `removes_on_clean` (`:5080-5082`); `dry_run_calls()` separates preview from real clean (`:5095-5100`). | The general N-workspace instrument. Cross-workspace contamination (one workspace's change affecting another's proof), per-workspace metadata failure at a chosen call index, and mutation *between* proof and spawn. The `on_clean` hook is what makes the post-spawn window testable at all (`:5188-5195` deletes only the listed paths). |
| `ParallelProbeRunner` `:5223` | Wraps `StagedCargo` with an active/peak counter and a 4 ms sleep on `metadata` (`:5231-5240`). | **Unbounded parallelism** — if the pool were unbounded or mis-chunked, `peak` would exceed `PROOF_REFRESH_CONCURRENCY_CAP`. Also asserts the peak equals what is reported in `counters` (`:6610-6613`), i.e. that the *meter* is honest, not just bounded. The sleep is what makes `peak >= 2` non-flaky. |
| `MetadataMutationRunner` `:5271` | Wraps `StagedCargo`; from the **second** `metadata` call onward mutates the returned JSON (`:5287-5316`): change `workspace_root`, append a member to `packages`+`workspace_members` (also writing a real manifest file, `:5295-5296`), or emit non-JSON. | The three ways a workspace's *identity* changes without its paths changing: root moved, membership grew, Cargo's own output unparseable. A proof comparing only paths, or trusting cached JSON, misses all three. |

Two patterns deserve naming. The **"from the Nth metadata call"** idiom
(`stage`, `fail_metadata_from`, the `MetadataMutation` threshold) is the module's
core race-injection mechanism: it pins the mutation to the exact scan→proof
boundary rather than to wall-clock time, which is what makes these tests
deterministic. And `BuildChangingRunner:4551-4560` documents a
fake-implementation hazard in a comment — good practice, and rare.

### Platform guards

`scripts/check-fixture-portability.py` scans `SCANNED_GLOBS = ("tests/**/*.rs",
"src/**/*.rs", "packaging/tests/**/*.py", "scripts/**/*.py")`, so **it does cover
`src/cleanup.rs`**. Its two rules — no literal `:`/`;` PATH separator assembly,
and no unjustified POSIX-shebang executable stand-in — are not triggered by
anything here: no PATH is built, no shebang is used, all fixtures are constructed
through `std::fs` in Rust.

Within `cleanup.rs` the platform guards are narrow and correct: `#[cfg(unix)]`
appears eight times — `:5209` (the M015 symlinked-output control), `:6038`,
`:6183`, `:6258`, `:7789`, `:8529`, `:8802`, `:8876` — all on
symlink-manipulating tests, which is right since creating a symlink and
asserting `symlink_metadata(...).file_type().is_symlink()` are the
platform-sensitive operations. `symlink_allowed_root_rejected_and_shared_forbidden`
(`:3351`) guards only its *body* at `:3352`, so on Windows the test still runs
but does less. The three real-Cargo tests (`:3785`, `:3920`, `:4953`) carry **no
platform guard** and depend on the host Cargo supporting the flags cleanup
passes.

### Honest gaps

1. **The positive `allowed_output_roots` path is unverified end to end.** The
   only test supplying a non-empty `allowed_output_roots` for a group that passes
   the ownership filter is `external_outside_root_allowed_via_configured_root`
   (`:3624`) — and it asserts the group is **skipped**. No test shows a
   `PrivateBounded` group with covering roots outside every clean root becoming
   `Cleaned` once an allowed root is configured. Given `config.toml:36-38` calls
   the field "operationally redundant" this is consistent, but it leaves the
   positive branch of `:549-554` unverified while `:636-641` advertises the
   capability to users.
2. **`execute_pre_spawn_decision` is untested and unused** — no caller at all.
   Worse, `simulation_candidate_set_matches_execute_preflight` (`:2761`) compares
   `Simulate` against **`CleanMode::Preview`** (`:2767`), not against
   `execute_pre_spawn_decision`, despite its name. No test validates the function
   the name refers to. Small in production terms (production uses the stricter
   `final_cleanup_proof_roots`), but the naming misleads a reviewer about
   coverage.
3. **Proof-time universe failure is not asserted to affect the exit code**
   (§9). `universe_workspace_that_cannot_reresolve_fails_closed` (`:5520`) asserts
   the row is `Skipped` and no cleans happened — correct, but silent about the
   `scope_blocked == None` / exit-0 consequence.
4. **The reason-code contract test covers 16 of 19 variants**, and the
   disposition test 5 of 6 (§2). For a vocabulary documented as the machine-facing
   contract, that is thin.
5. **`ownership_skip_detail`'s `PrivateBounded` arm is unreachable** (§5) and
   untested — consistent with unreachability, but a future caller would get a
   misleading message with no test to catch it.

**The installer-fixture question.** `overview.md:221-225` records the project's
recurring lesson — "a green test proves only that its own premises hold" — and
`check-fixture-portability.py`'s docstring names C009/C011/C012 as the same
failure: "a fixture that did not run, in a lane that reported green", with a
Windows installer fixture green only because an unrelated real cargo command
failed. Does that risk exist here? **Partly, in a narrower form.**

- The *class* of risk is present in the real-Cargo tests. A test like `:3348`
  uses `SystemCleanupRunner` against host cargo; if that invocation failed for an
  unrelated reason (lockfile semantics, a network-less index, an unsupported
  flag), the setup assertions could pass while the intended branch went
  unexercised. `check-fixture-portability.py` cannot catch this — its own
  docstring concedes it "deliberately does not check… whether a substituted tool
  is actually the one the subject resolves" and "whether a case's pass condition
  proves the intended branch."
- The *specific* installer failure cannot recur here: no installer, no PATH
  substitution, no shell fixture. The portability rules apply to
  `src/cleanup.rs` and currently find nothing.
- The module does defend itself once, at `:4180-4181`: "Global
  `CARGO_BUILD_BUILD_DIR` is never set in tests because it races with parallel
  real-Cargo tests" — exactly the premise discipline the lesson calls for.
- What is **not** defended: no test asserts a *premise* about the host cargo
  before relying on it. The closest is
  `profile_selector_capability_is_bounded_to_qualified_runtime_versions`
  (`:2604`), which tests the version parser in isolation rather than the host
  version. Treat the three real-Cargo tests as environment-dependent and confirm
  they are not silently short-circuiting on the CI lanes.

---

## 11. Review checklist

1. **Any change reaching `:1256-1258` must have a live `proof` from `:1152`.**
   Nothing may read `unit.*` for a mutation decision afterwards; the spawn-arm
   `CleanResult` literals (`:1264-1351`) should cite `proof.*` exclusively.
2. **`covering_is_authorized_roots:595-597` must remain the first thing that
   inspects anything.** Moving the ownership check after path validation would
   let a future edit return `true` for `ExternalUnproven`. Guarded by `:3293` and
   `:4061`.
3. **The `ownership != PrivateBounded` returns at `:720` and `:2131` must stay
   ahead of their `covering_is_authorized_roots` calls** (`:730` is a per-group
   call that assumes private). This is the C003 unit invariant.
4. **Error strings feed the machine contract only at the refresh boundary.**
   `proof_skip_code` (`:1218`) classifies pass-through Cargo/discovery strings
   by substring; proof gates themselves carry explicit codes
   (`ProofFailure::new`), so rewording a `format!` in
   `final_cleanup_proof_roots` cannot reclassify a `CleanupReasonCode`. When
   touching the refresh boundary or the "changed identity"/"vanished" cases,
   check `:1231-1233`.
5. **A new `CleanupReasonCode` or `PolicyDisposition` variant must be added to
   its contract test** (`:2807`, `:2787`) *and* to `output.rs:173` / `:243-254`.
   Three reason codes and one disposition are currently missing from their tests.
6. **The universe is shared, so a `ProofUniverse` failure is a run-wide skip, not
   a unit failure.** Touching `hoisted.get()` (`:2004`) or the
   `get_or_insert_with` at `:1149-1151` flips §9's answer from per-unit to
   whole-run. Consider whether `scope_blocked` should be set there.
7. **`refresh_candidate` is the difference between production and the public
   wrappers.** `:1161` passes `true`; `:1981` passes `false` (via
   `final_cleanup_proof`; `execute_pre_spawn_decision` delegates through it). A refactor
   unifying the call sites must not drop the per-candidate re-resolution.
8. **`cargo --version` is a real spawn at `:2414`, outside the proof**, gated on
   `selector.is_some()` (`:911-912`). Any new runtime probe should follow the same
   gating so a zero-selector run keeps its spawn count at locate+metadata only.
9. **`min_reclaimable_bytes` is checked twice on different numbers** — scan bytes
   at `:1101`, fresh proof bytes at `:1170`. Removing the second check would let a
   unit that *shrank* between scan and spawn pass a size policy.
   `fresh_size_policy_rejects_shrunk_output_before_any_cargo_clean` (`:4685`)
   covers exactly this.
10. **`frozen_env` (`:2338`) is the only thing between an inherited
    `CARGO_BUILD_BUILD_DIR` and a silent redirect.** Both refusal branches
    (`:2349-2356`, `:2367-2372`) are load-bearing; covered by
    `frozen_env_sets_target_and_build_dir` (`:3425`) and the M5/M9 test (`:6363`).
    Likewise `--offline --locked` (`:2395-2396`) are unconditional and must not
    become conditional; a networked cleanup path is a behaviour change, not a fix.
11. **Post-clean measurement failure must not fabricate bytes.** The `Err` branch
    at `:1334-1353` sets `after_bytes: None`, increments `diagnostics`, and keeps
    `outcome: Cleaned` with `MeasurementFailed`. Guarded by
    `post_clean_measurement_uncertainty_fabricates_no_decrease` (`:6160`).
12. **Adding a filesystem deletion to this module would be a contract-level
    regression.** There is none in production (`:2530-2531`) and ADR 001
    `:133-136` forbids it. Any `fs::remove_*` in cleanup.rs production code
    should fail review outright.
13. **The parallelism cap (`:1673`, `:1852`) and the meter's honesty.** If
    `ProofRefreshMeter` stops counting, `proof_metadata_peak_concurrency` becomes
    a fiction while the refresh still works. `:6610-6613` asserts
    meter-equals-reported.
14. **Determinism under parallel refresh is load-bearing for report ordering.**
    `:1928` sorts by root; `render()` re-sorts by size then path (`:354-360`).
    `sequential_and_parallel_refreshes_produce_the_same_workspace_graph_and_disposition`
    (`:6652`) keeps both honest.
15. **`config.toml:36-38`'s "operationally redundant" claim should be revisited.**
    It holds because `PrivateBounded` requires covering roots below the *workspace*
    root (`workspace.rs:1217-1220`), but the positive `allowed_output_roots` branch
    (`:651-656`) and its advertised remediation (`:738-743`) have no end-to-end
    test. Add one or soften the comment.
16. **The public wrappers with no production caller are a growing dead-code
    surface** — `clean` `:761`, `is_authorized` `:555`,
    `covering_is_authorized` `:572`, `unit_block_reason` `:691`, plus
    `final_cleanup_proof` `:1958` and `execute_pre_spawn_decision` `:2318`. Each is
    a place where a future edit can diverge from the live path with no compile
    error. Mark them clearly as test/consumer surface or delete them — and note
    that `overview.md:193-195` cites `execute_pre_spawn_decision` as a production
    gate, which it is not.

---

## Related

- [Architecture overview](overview.md) — cross-cutting invariants
- [Domain & errors](01-domain-and-errors.md) — `OutputOwnershipClass`, `OutputSet`, `ScanCounters`
- [Workspace](07-workspace.md) — grouping, `CleanupUnit`, `RawGroup`, ownership classification
- [Traverse](08-traverse.md) — activity and sizing measurement
- [Config and editor](03-config-and-editor.md) — `allowed_output_roots` schema
- [Testing & verification](14-testing-and-verification.md) — the false-green discipline this module is subject to
