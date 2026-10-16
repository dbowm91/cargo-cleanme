# M016 — Per-Island Cleanup Activation (active implementation plan)

Status: **active**. All three §2 gates cleared 2026-10-16: M015 closed
(`plans/closure/artifact-discovery-cleanup/m015-status.md`), ADR 004
accepted with C004/C006 amendment (`1c56e9f`), islands occur live. This
plan now authorizes implementation of exactly what §1–§12 specify.

Baseline: `0a5a6e6` (`main`, release 0.2.8) **plus M015 landed**. M015 is
currently conditionally closed with an uncommitted tree (see
`plans/closure/artifact-discovery-cleanup/m015-status.md`); M016's
implementer re-derives every `src/` line cited below against the landed
tree and records any drift before touching behavior. Citations below were
re-derived against the M015 working tree (cleanup.rs 9868 lines); where
`architecture/09-cleanup.md` §6 still cites pre-M015 numbers (m015-status
"Known limitations"), this plan cites `src/` directly and says so.

Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Decision under test:
`plans/adr/004-proposed-bounded-independent-cleanup-proof-islands.md`
(remains **proposed**; this plan neither accepts it nor amends ADR 001).
Corrects nothing; activates M015's certificate. Extends C031/C034 evidence.
Skills: planning-and-closure; cleanup-safety; discovery-and-ownership;
test-evidence; json-and-exit-contract (contract change ships here, §9).

## 1. Objective and non-goals

Objective: the operator goal, verbatim — run the command; it cleans every
eligible directory it can prove, holds what it cannot prove with a typed
reason, and no aberrant directory stalls the run. Behind an explicit
opt-in flag (§5), a blocked scope partitions into proof islands (M015's
`compute_proof_partition`, `src/cleanup.rs:2023-2159`); each would-proceed
island is proven and spawned independently; each held island is reported,
not cleaned; the exit code is nonzero iff anything was held or failed (§9).

Non-goals (explicit; a plan that ships any of these is outside its
authority):

- No change to default (flag-off) behavior. Every whole-scope-block pin in
  M015 §2 (`PerPathFailRunner` struct `src/cleanup.rs:6492`,
  `combined_roots_classify_cross_root_output_as_shared_in_every_mode`
  `:9408`,
  `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes`
  `:9451`) keeps passing verbatim with the flag off. Flipping the default
  is a separate product/CLI decision needing its own plan and release
  vehicle.
- No second deletion path. Deletion stays one `cargo clean` per workspace
  `CleanupUnit` over its complete `OutputSet`
  (`architecture/09-cleanup.md` §5–§6). The only mutation path is the
  child process built by `SystemCleanupRunner::run_with_env`
  (`src/cleanup.rs:727-733`, `Command::new("cargo")` at `:733`; the
  `CargoRunner::run` spelling at `:711-713`). No `fs::remove_*` in
  production (review-fail per 09-cleanup §9 "Is deletion by `cargo clean`
  … or both?").
- No Cargo-config reimplementation. Attribution rules may use Cargo-emitted
  evidence and filesystem identity facts only (M015 §5 hard constraint).
  Any rule needing a computed effective target-dir for an unresolved
  manifest is merge-by-construction.
- No authorization-promotes-proof. `covering_is_authorized_roots`
  (`src/cleanup.rs:813-900`) keeps the ownership-first ordering — the
  non-`PrivateBounded` early `Ok(false)` at `:822-827` runs before any
  path handling; the flag does not consult `allowed_output_roots` any
  earlier.
- No narrowing of the evidence. Activation partitions the *spawn
  decision*, never the *graph*: grouping, classification, and overlap
  checks still read the full combined universe (proof gate 8,
  `src/cleanup.rs:3423-3469`). A per-island universe that forgets the
  held islands is the naive skip M015's controls kill.
- No `update_json` change (still the non-envelope surface; out of scope).

## 2. Activation gates (all three, in order)

1. **M015 fully closed** (hosted OS/MSRV lanes + VCS landing outstanding
   per its closure record). M016 builds on the certificate code; it does
   not re-litigate the certificate.
2. **ADR 004 accepted with an explicit C004/C006 amendment** covering
   exactly two things: (a) per-island activation behind an explicit
   opt-in flag, and (b) the refused-manifest passenger rule (§4 rule 3).
   Without (b), only zero-unknown islands may proceed (§4 fallback).
3. **Live data shows islands occur** — already satisfied (M015 package G:
   S1 not triggered; G1 2 islands, G1c passenger island).

If ADR 004 is rejected, §12 S3 applies: this plan stays blocked and the
rejection is recorded as the completed decision. Implementation work
started before gate 2 clears is out of authority, however green its
tests.

Handoff authority order (planning-and-closure): canonical spec, then
accepted ADRs (ADR 001 fail-closed is binding until amended — this plan
does not reinterpret it), then roadmap, then this plan, then evidence.

## 3. Invariants (unchanged; violated by no work package)

Ownership proof not timestamps; authorization never manufactures proof;
re-validation at the last moment per spawn; fail closed on uncertainty;
one workspace `CleanupUnit` over its complete `OutputSet`; deletion is
always `cargo clean`; `Simulate` spawns nothing (structurally,
09-cleanup §7 "The pre-spawn decision" and §9 zero-spawn property). Plus
two activation-local invariants: **flag-off parity** (§6E) and
**combined evidence** (§1, §6B).

## 4. The activation proof rule (what the ADR amendment must bless)

An island may spawn iff **all** hold:

1. **Component completeness** — every manifest under the island's roots
   is resolved or attributed inside the island (M015 §4.1; same rule as
   today, smaller boundary).
2. **Resolved non-overlap** — no resolved workspace outside the island
   shares or overlaps island outputs, computed from the *combined* graph
   (M015 §4.2; union edges as in `compute_proof_partition`).
3. **Unknown quarantine** — the island holds no unknowns **except**
   `RefusedByCargo`-capability passengers
   (`src/domain.rs:178-187`) attributed to a resolved in-island
   workspace Cargo itself named (`attribute_unknown → PassengerOf`,
   `src/workspace.rs:419-438`; parse at `:372-387`). Every other class
   (transport, unparseable output, lost workers, metadata, identity,
   discovery holes) merges-or-blocks exactly as today. This is the
   deferred sketch from the M015 closure record ("Cargo refused the
   claimant so no Cargo command places output for it; any output falls
   under the named parent's proven target") promoted to a reviewable
   rule: it is sound only because the refusal is Cargo-emitted evidence
   of non-ownership-by-Cargo, never because the manifest "looks valid".
4. **Environment match** — `output_env_matches`
   (`src/cleanup.rs:367-372`, currently `#[cfg(test)]` with no
   production callers) is promoted to production and wired as a merge:
   the partition's `provenance` snapshot (`snapshot_output_env`,
   `:357-362`, recorded on the partition at `:2029`) is re-compared
   before each island's first spawn; any mismatch merges the remaining
   islands into one blocked scope before any further spawn.
5. **Universe stability** — `universe_manifests_stable`
   (`src/cleanup.rs:2330-2332`) gates growth mid-run: the per-unit
   H-2 rediscovery gate (`:1681-1710`) already blocks the remaining
   scope on any manifest-set change, and held-island manifests are part
   of the compared `initial_manifests` set by construction (they are
   discovered manifests). No new mechanism; retained and covered by the
   existing end-to-end control (`:5326`) extended to a held-island run.

**Fallback if the amendment blesses (a) but not (b):** only
zero-unknown islands proceed; passenger islands stay blocked with the
M015 report. The engine code must make this a one-predicate difference,
not a fork, so the fallback is reviewable in the same diff.

Per-island proof reuses `final_cleanup_proof_roots`
(`src/cleanup.rs:3212` onward; the twelve-gate contract is enumerated
in its doc comment at `:3164-3177`, implementation gates 1–12 at
`:3223`, `:3238`, `:3243`, `:3284`, `:3286-3341`, `:3342-3422`,
`:3423-3469`, `:3470`, `:3473`, `:3543`, `:3546`) with
`refresh_candidate = true` — which is already what production passes
at the unit-loop call site (`:1599-1609`; the `false` at `:3207`
belongs to the single-candidate test wrapper only). Gate 8
(no-other-workspace-overlaps, `:3423-3469`) reads the **full fresh
universe including held islands** — this is the load-bearing
difference from the naive skip and must carry a dedicated test (§10).
The hoisted universe (`ProofUniverse::refresh`, `:2958-2997`, built
per run at `:1584-1586`) is built from **all** resolved workspaces,
including held islands' — they are resolved, and gate 8 must see
them. The admission-premise late recheck (`:1557-1570` before the
loop, `:1739-1752` before every spawn) is retained per spawn
unchanged.

## 5. Flag design (decided)

The flag is `--partitioned`: one explicit opt-in long flag on the
`clean` subcommand (beside `dry_run`/`cargo_preview` at
`src/cli.rs:135-149`, inside the `Clean` variant at `:107-150`).
Decided, not deferred, on the following evidence:

- **Collision-free.** No `partition*` long flag, alias, or prefix
  exists anywhere in `src/cli.rs`, `completions/`, or `man/` (verified
  by empty grep at plan time; the implementer re-verifies).
- **Orthogonal to `CleanMode`.** It conflicts with nothing and composes
  with all three modes: Simulate predicts per-island spawns with zero
  spawns, CargoPreview runs `cargo clean --dry-run` per proven unit in
  proceed-islands only, Execute cleans. Parity required across all
  three (§10). `clean_mode` (`src/cli.rs:281-292`) is untouched.
- **No short flag, no hidden alias, no config-file mirror.** Absence of
  the flag is today's behavior, which keeps the kill-switch trivial
  (§6E). Bare maintenance (`CleanupRequest` at `src/cli.rs:228-250`,
  built at `:312-317` and `:379-395`) does not set it.
- **Threading.** New `partitioned: bool` field on `CleanupRequest`,
  through `run_engine_guarded` (`src/main.rs:594-618`) into
  `clean_with_roots_policy_selector_guarded`
  (`src/cleanup.rs:1278-1290`, already `#[allow(too_many_arguments)]`).
  Documented in `cli.rs` with a doc comment naming its interaction
  with `--dry-run`; completions/man regenerated via `generate-docs`
  (drift gate, §11).

## 6. Ordered work packages

- **A. Per-island scope resolution.** At today's two scan-time block
  sites — IncompleteDiscovery (`src/cleanup.rs:1336-1357`, partition
  call at `:1350-1355`) and OwnershipUnproven (`:1359-1385`, partition
  call at `:1376-1381`) — flag-on keeps the blocks' messages, keeps
  `scope_blocked` set (§9), but additionally builds units for
  proceed-islands only: grouping (`build_groups`) and classification
  stay whole-universe (combined evidence, §1); only unit
  construction/dispatch is island-filtered, reusing the existing
  `build_cleanup_units` + `selection.admits` shape from
  `resolve_cleanup_scope` (`:2539-2605`, selection-after-classification
  at `:2585-2598`). Held islands produce typed held-island rows (§9),
  never units. Discovery-hole merge causes
  (`PartitionMergeCause::DiscoveryHole/UnnamedDiscoveryHoles`,
  `:380-394`) collapse to one blocked island exactly as today — the
  IncompleteDiscovery path therefore almost always behaves exactly as
  today, which is the honest picture, not a silent widening.
  Flag-off path byte-identical to today.
- **B. Per-island proof with combined evidence.** Promote
  `output_env_matches` to production and wire the §4-rule-4 merge;
  gate 8 over the full fresh universe including held islands (§4);
  admission-premise late recheck retained per spawn (`:1557-1570`,
  `:1739-1752`); H-2 growth blocks remaining scope (`:1681-1710`,
  `:2330-2332`). `clean_args` (`:3711`) and `frozen_env` (`:3657`)
  unchanged — island dispatch never alters spawn argv or env.
- **C. Sequential island loop.** The existing unit loop
  (`for unit in &units`, `:1422`; proof-failure skip+continue at
  `:1611-1629`; spawn-failure `Failed`+continue at `:1782-1820`) runs
  unchanged *inside* each island; islands run strictly sequentially
  (no unit spawns until the previous spawn returned, per 09-cleanup
  §9 "Are two units sharing physical bytes prevented from racing?").
  A per-island spawn failure becomes a `Failed` row via
  `failure.code`/`CargoFailed` and the loop `continue`s to the next
  island — this, at island granularity after proof, is the sanctioned
  form of "skip and go to the next". Policy gates
  (include/exclude/min-bytes/inactivity, `:1441-1670`) and selector
  capability gates apply per unit inside islands exactly as today.
- **D. `-i` interplay (decided).** `list_cleanup_candidates` applies
  the identical gates (`src/cleanup.rs:2365-2418`, block construction
  at `:2375-2400`); candidates come from proceed-islands only, and
  held islands are shown as **non-selectable context lines, never as
  cleanable rows** (ADR 006 anchoring; selection can remove
  candidates, never add proof). `CandidateListing.scope_blocked`
  (`:2344`, documented at `:2338-2343`) stays set whenever any island
  is held, so the session still refuses a fully-blocked universe
  exactly as today; a partially-proceedable universe offers the
  proven rows with the held context visible.
- **E. Kill-switch and flag-off parity.** The flag gates one code
  path; removing it restores today's behavior structurally. Parity
  test extended from M015's (`:5026`): flag-off Execute/Simulate/
  Preview results, reason codes, exit codes, and spawn counts
  identical to the pre-M016 engine on the same fixtures.
- **F. Render restructure for partial success.** Today a set
  `scope_blocked` returns the human report early
  (`src/cleanup.rs:558-561`), so completed rows never render in
  human (JSON `units` already carries them — comment at `:500-502`).
  Partial success requires both: render completed rows *and* the
  block *and* per-island sections. The block message keeps its
  pinned substrings (`unresolved_manifest…` path keeps
  "cleanup ownership could not be proven", `:1364`;
  discovery path keeps "combined cleanup ownership universe is
  incomplete", `:1340`) with an additive suffix naming cleaned
  islands — substring pins in `tests/cli_contract.rs` keep passing,
  and the message no longer claims "no cleanup commands were run"
  when commands ran. Counts: `resolved_workspaces` counts all
  resolved workspaces including held islands'; `units_considered`
  counts proceed-island units; `discovered_manifests` unchanged.
- **G. Documentation.** `09-cleanup.md` activation section (per-island
  proof, combined-evidence rule, flag), `07-workspace.md` passenger
  rule if §2 gate 2(b) is blessed, `02-cli.md` §2.4 `clean` surface,
  `10-reporting.md` islands fields, `overview.md` counts (source of
  truth — re-derive, never copy; current: 453 inline tests,
  cleanup 118 + workspace 76). Citations re-derived from source,
  never shifted arithmetically. ADR 004 itself is amended only
  through its own acceptance, not by this plan.

## 7. Failure, cancellation, and contention semantics

Partition computation stays infallible by construction (M015 §7): any
internal error degrades to a single island — today's block — never to
a finer partition. Reporting failures must not alter machine fields.
Concurrency: packages A–E reuse existing bounded pools without
changing caps (`PROOF_REFRESH_CONCURRENCY_CAP`, covered at
09-cleanup §6 "`ProofUniverse` and hoisting"); island loop is
sequential. TOCTOU minimization statement (spawn site `:1732-1734`;
contract at `:2635-2639`) extends per spawn unchanged. Mid-run
rediscovery/diagnostic/premise failures keep their existing
scope-block-and-return shape (`:1591-1598`, `:1615-1625`,
`:1702-1708`, `:1743-1752`) — under the flag they name the islands
already cleaned, but still stop the run.

## 8. Compatibility and configuration effects

One CLI flag (§5); no config keys; no MSRV change (1.89); no change to
`update_json`. The JSON/exit surface below (§9) is the one deliberate
machine-surface change in this plan, and it is additive-only by
assessment recorded here — not deferred.

## 9. Contract: JSON, exit codes, human report (json-and-exit-contract)

Assessed with full context now (deferred from M015 §6D deliberately);
the assessment *is* the decision, recorded here so the implementer does
not re-decide it:

- **Exit codes: unchanged code, clarified meaning.**
  `src/main.rs:474-478` (`failed > 0 || scope_blocked.is_some() → 1`)
  is untouched. Partial success keeps `scope_blocked` set (held
  islands are named in it), so exit 1 falls out of the existing
  aggregation with no new code and no new exit value — satisfying
  "nonzero iff anything was held or failed" structurally. What
  changes is documentary: `scope_blocked: true` no longer implies
  "nothing ran"; the block message's additive suffix (§6F) and the
  13-orchestration exit-code account record that. No exit-0-with-held-
  islands: the proof-time-failure signal gap (09-cleanup §9, items
  2–3) is closed in the same direction for islands — held-but-zero
  is a typed block, and the aggregate never conflates "nothing to
  clean" with "could not prove".
- **JSON (`EnvelopeV1`): additive fields only, no version bump.**
  `CleanupV1` (`src/output.rs:48-64`) gains exactly one field:
  `islands: Vec<IslandV1>`, where `IslandV1 { roots: Vec<String>,
  verdict: "proceeded" | "held", cause: Option<String>,
  passengers: Vec<String> }`. `cause` renders the merge/held reason
  in `partition_section` vocabulary (`merged by …`, never a new
  reason-code axis); `passengers` names manifests already present in
  `unresolved_participants`. Per-unit rows join to islands on the
  workspace-root path, so no unit field changes. Reason codes:
  **no new code** — held islands are not units, so no
  `CleanupReasonCode` applies; collapsing island-held into
  `skipped_ownership_unproven` unit rows would be a contract
  degradation (distinct operator responses), so held islands stay on
  the islands axis, not the units axis. The key-set stability test
  (`src/output.rs:658`) is updated deliberately, not deleted;
  human/JSON parity tested per the skill (same result, two
  renderings; non-TTY stderr discipline unchanged). `update_json`
  untouched.
- **Human:** completed rows render alongside the block (§6F);
  per-island sections extend the `partition_section` vocabulary
  (`src/cleanup.rs:2196-2322`) with per-island outcome (cleaned /
  simulated / previewed / held + bytes), bounded like the existing
  participant listing (`PARTITION_MAX_*`).
- **`--format log` stays counts-only** (no paths, no island detail):
  cleaned/simulated/failed counts flow through the existing line;
  the blocked marker is set whenever `scope_blocked` is set,
  including partial runs. Pinned by a dedicated log-line test (§10).

## 10. Required tests

- **Flag-off parity** (§6E) plus the three M015 whole-scope pins
  verbatim (`:6492`, `:9408`, `:9451`).
- **Enforcement twins of every M015 hostile control** (flag-on,
  all three modes incl. Execute-with-fake): redirect
  (`src/cleanup.rs:5078`), nested prefix (`:5114`), metadata-failure
  overlap with zero-spawn engine assertion (`:5152`), symlinked
  output unix (`:5211`), build-dir env move (`:5256`), H-2 growth
  predicate (`:5297`) and end-to-end (`:5326`). Each asserts zero
  cross-island cleans and exact per-island dispositions
  (`proceeded` only where the certificate says so; every hostile
  island `held`). A twin that passes against the naive skip
  (`naive_islands`, `:5480`; `naive_equality_islands`, `:5487`) is
  decoration — discard it.
- **New activation-specific controls:** passenger-parent target
  redirected under another island mid-run (must merge/block, never
  clean the passenger island alone); env change between partition
  and spawn (must merge remaining — the promoted
  `output_env_matches` as detector, target-only naive as the
  decoration check); gate 8 reading held islands (staged overlap
  *from a held island's direction* fails the proceed-island's
  proof — the test that distinguishes this plan from the naive
  skip at the proof level, not just the partition level).
- **Partial-success surfaces:** human render carries completed rows
  + block + island outcomes; JSON `islands` array joins to
  `units` on workspace root; log line shows counts + blocked
  marker with no paths; `-i` offers proceed rows with held
  context non-selectable (subprocess case extending
  `tests/cli_contract.rs:1930`).
- **Discriminating assertions only**: exact island verdicts,
  `cause` strings, reason codes, spawn arg vectors
  (StagedCargo-style fakes, 09-cleanup §6 test table and §10);
  never "the command failed".
- **Repo-owned disposable fixtures** for the G1/G1c shapes (M015's
  lived in ephemeral `/tmp`; M016's must be re-runnable and pass
  `check-fixture-portability.py`, whose globs cover `src/` and
  `tests/`).
- Hosted Linux/macOS/Windows + MSRV 1.89 lanes for all
  fixture-bearing packages (C021/C024: platform-only defects are the
  norm here).

## 11. Verification commands and release gate

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
```

Plus `cargo run --quiet --features dev-tools --bin generate-docs -- --check`
(CLI surface changes: one new flag → regenerated completions/man).
Plus ADR 004 item (e): a distinct release gate with disposable fixtures
and published-artifact smoke, and a version bump carrying the opt-in
flag (behavior change, even flag-gated). `scripts/release-check.sh`
is the pre-release gate, not routine verification.

ADR 004 evidence mapping (item (e) above; (a)–(d) in §2/§4/§9/§6E):
(b) fresh per-island proof design + every §10 negative control with
failing-direction evidence; (c) the §9 assessment (additive-only, no
envelope bump); (d) the kill-switch is flag absence (§6E) plus the
flag-off parity test — rollback is not deploying the flag.

## 12. Acceptance criteria and stop conditions

Acceptance: gates §2 cleared in order; activation rule §4 implemented
as specified (or §4 fallback, recorded); flag §5 shipped with
regenerated artifacts and zero collisions; per-island proof with
combined evidence (§6B) with the hoisted universe provably
held-inclusive; contract §9 implemented as assessed (no re-decision);
render restructure §6F with pinned substrings intact; every §10
control green with failing-direction evidence against the naive skip;
hosted lanes green; closure record with requirement-to-evidence
matrix, commands actually run, and known limitations (per
planning-and-closure: implementation commit, matrix, verification
actually run, platform/fixture evidence, limitations, findings by
severity, disposition).

Stop conditions (any one ends the line, block retained, recorded as
rejection — a completed decision, not a failure):

- **S1.** Any hostile shape escapes: a cross-island clean, a spawn
  against a merged island, a held island's bytes touched, or a
  proceed-island spawn whose argv/env differs from what the
  whole-scope engine would have spawned for that unit. Stop; do
  not narrow the control.
- **S2.** Flag-off parity fails — activation leaks into default
  dispositions. Not landable; stop.
- **S3.** ADR 004 is rejected, or (b) is refused while the
  implementation needs it. M016 stays blocked; record the rejection.
- **S4.** Implementation discovers the §9 assessment was wrong
  (e.g. an envelope bump is unavoidable). Hold for a release
  vehicle; the plan stays open, not conditionally closed.

## 13. Planning review (§12 of the process)

1. Durable behavior unambiguous: yes — §1 objective, §8 no durable
   side effects beyond `cargo clean` itself.
2. No destructive behavior ahead of its dependency: yes — §2 gates
   implementation on ADR acceptance; this document is design only.
3. Scope/filter precedence explicit: partition applies after
   classification at the block sites; grouping stays
   whole-universe (§6A); discovery scope untouched; -i interplay
   decided in §6D.
4. Filesystem failure semantics explicit: §4 rules + merge default;
   mid-run growth/env rules in §4 rules 4–5, §6B, §7.
5. Concurrency does not weaken correctness: §7; sequential island
   loop, unchanged caps.
6. Cross-platform testable: §10 hosted lanes required, MSRV pinned.
7. Configuration compatibility: no new keys (§8); one flag (§5).
8. Verification specific: §10–§11.
9. Closure evidence measurable: §12 + closure record format per skill.
10. Stop conditions prevent silent scope expansion: §12 S1–S4; S3
    names rejection as success.
