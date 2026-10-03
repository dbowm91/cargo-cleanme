# Corrective C003 — Workspace Cleanup-Unit Atomicity and Full Ownership-Graph Revalidation

Status: ready

Repository baseline: 5af882122b4781e8a927d34a3792cbfd3c0d12cb

Corrects:

- M005B authorized redirected cleanup and simulation
- C002 M005 ownership safety, simulation parity, progress, and global qualification
- their historical closure records remain unchanged

Authoritative references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

Primary class: corrective / destructive safety / accounting

## 1. Objective

Repair the remaining mismatch between the unit cargo-cleanme currently authorizes and reports (one PhysicalOutputGroup) and the unit one Cargo clean invocation can affect (the complete resolved workspace OutputSet).

C003 must make a destructive decision atomic across every physical output group that the same Cargo clean invocation can affect.

It must also revalidate ownership against the complete discovered cleanup-scope ownership graph immediately before mutation, rather than rebuilding a one-workspace graph that cannot detect another workspace moving into the candidate's output region.

This corrective does not add any new cleanup capability. It narrows destructive eligibility where the current group-by-group implementation can over-authorize a workspace-level Cargo cleanup.

Generic no-argument scan performance remains a separate future optimization line and is not part of C003.

## 2. Findings

### C003-F1 — Physical-group authorization is narrower than Cargo cleanup scope

Severity: high / destructive boundary violation.

At the baseline, clean_with iterates RawGroup values independently. final_cleanup_proof receives one RawGroup and can approve it when that group is PrivateBounded.

For a workspace with distinct non-overlapping target and build roots, build_groups produces separate physical groups.

Example:

- workspace A target: /repo/A/target -> PrivateBounded
- workspace A build: /shared/cargo-build -> ExternalUnproven

The ExternalUnproven build group is correctly skipped when considered by itself.

However, processing the private target group re-resolves the complete workspace OutputSet. ExecutionProof then carries both the target and the distinct build directory, and frozen Cargo context can direct Cargo at both outputs.

Current Cargo build-dir design specifies that cargo clean cleans both target-dir and build-dir. A single Cargo clean therefore has a larger mutation footprint than the one physical group used to authorize it.

Result: one PrivateBounded group can carry a second ExternalUnproven, Shared, Uncertain, or otherwise unauthorized output into the same Cargo invocation.

This violates ADR 001's ownership boundary even though C002 correctly prevents directly authorizing an ExternalUnproven RawGroup.

### C003-F2 — Cleanup result/accounting is group-scoped while mutation is workspace-scoped

Severity: medium/high / correctness and reporting.

When one workspace has distinct sibling target/build physical groups, clean_with may process each group as if each represented a separate cleanup action.

The first successful Cargo clean can remove or modify both output roots. Post-clean measurement then measures only the current RawGroup covering roots.

Consequences can include:

- incomplete observed-decrease accounting;
- a second group later appearing vanished/skipped even though it was affected by the first clean;
- duplicate preview/simulation intentions for one Cargo invocation;
- progress candidate counts that do not correspond to actual cleanup units;
- final reports that describe physical-group operations that never occurred independently.

One Cargo invocation must correspond to one cleanup result.

### C003-F3 — Final ownership revalidation sees only the current workspace

Severity: high / race-hardening defect.

C002's final_cleanup_proof rebuilds group classification from:

workspace::build_groups(std::slice::from_ref(fresh_ws))

This proves only that the current workspace's own output roots still look internally private.

It cannot detect another workspace in the original explicit cleanup scope changing its target/build configuration after the initial scan so that it now overlaps the candidate's output region.

Example:

1. A and B initially resolve to separate PrivateBounded output.
2. Before A is cleaned, B's Cargo configuration changes to point B at A's target/build region.
3. A's one-workspace fresh graph still has one owner and reports PrivateBounded.
4. Cargo clean can proceed against output that is now shared.

C003 must re-resolve a bounded ownership universe sufficient to detect this change before each destructive invocation.

### C003-F4 — Cleanup stats omit final-proof Cargo work

Severity: low/medium / debug and qualification accuracy.

clean_with accumulates initial discovery/resolution counters in CleanReport.

final_cleanup_proof creates fresh local ScanCounters for its Cargo re-resolution calls and discards them.

Therefore cleanup --stats wall elapsed is correct, but locate/metadata call counts and timings under-report the work actually performed during preview/simulation/execute qualification.

C003 changes the final proof substantially, so the proof's re-resolution counters/timings must be accumulated into the cleanup report.

## 3. Why prior verification missed these defects

- M005/C002 correctly reasoned about ownership at PhysicalOutputGroup granularity but did not explicitly model the mutation footprint of one workspace-level Cargo clean.
- Distinct target/build tests verified path identity and freezing but did not construct a mixed-class OutputSet where one root was private and another was external/shared/uncertain.
- C002 race tests mutated the current workspace or its physical roots; they did not mutate a second discovered workspace into overlap after the initial graph was built.
- Final reporting tests treated each RawGroup as an independent cleanup result and therefore did not assert one-workspace/one-Cargo-invocation/one-result cardinality.
- --stats tests covered the initial scan counters, not the internal counters used by final proof re-resolution.

## 4. Correct destructive abstraction

Introduce a cleanup-oriented domain record equivalent in responsibility to CleanupUnit.

A CleanupUnit represents exactly one Cargo workspace cleanup invocation and contains:

- stable workspace identity and root manifest;
- the complete resolved OutputSet;
- every PhysicalOutputGroup touched by that OutputSet;
- the union of physical covering roots that Cargo clean can affect;
- ownership/authorization state for every touched group;
- the set of other discovered workspaces relevant to overlap proof;
- pre-clean deduplicated bytes across the complete union.

Core invariant:

A workspace is destructively cleanable only if every physical output group that the Cargo invocation can affect is independently PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under final ownership-graph revalidation.

If any affected group fails, skip the entire CleanupUnit.

No partial workspace clean is introduced.

## 5. Ownership universe

For clean ROOT, the ownership universe is the complete set of Cargo workspaces discovered from that explicit cleanup scope for the current invocation.

Before each Preview, Simulate, or Execute disposition that would otherwise be cleanable:

1. re-resolve every workspace in that bounded ownership universe through Cargo;
2. rebuild the complete physical output graph from those fresh workspaces;
3. locate the candidate workspace and all physical groups touched by its fresh OutputSet;
4. require those groups to have exactly the expected physical shape and ownership;
5. reject the CleanupUnit if another discovered workspace now overlaps any affected output.

This does not claim knowledge of arbitrary workspaces outside clean ROOT. ADR 001 already handles that limitation by making externally unproven output inventory-only.

Do not perform a whole-machine rescan during cleanup.

Caching may be used within one final preflight only if it does not reuse state from before the destructive revalidation boundary.

## 6. Invariants

- Workspace remains the logical ownership and Cargo invocation unit.
- PhysicalOutputGroup remains the unit for physical deduplication, activity, and ownership classification.
- CleanupUnit is the unit for destructive authorization, Cargo invocation, progress, and cleanup reporting.
- One CleanupUnit produces at most one Cargo clean subprocess.
- One Cargo clean subprocess produces exactly one cleanup result.
- Every output group affected by that Cargo clean must pass the destructive boundary.
- Private target plus ExternalUnproven build => entire unit skipped.
- Private target plus Shared build => entire unit skipped.
- Private target plus Uncertain build => entire unit skipped.
- Equal/nested target/build remain one physical group and are counted once.
- Preview, Simulate, and Execute consume the same final CleanupUnit proof.
- Simulate invokes no Cargo clean command.
- Cargo remains authoritative for workspace/output resolution.
- No direct recursive deletion is added.
- Rust 1.89 remains the compile-time MSRV.
- Generic scan behavior and progress UI semantics remain unchanged except where cleanup totals now count CleanupUnits instead of RawGroups.

## 7. Required production changes

### 7.1 Build cleanup units after initial analysis

After discovery, workspace resolution, physical grouping, and fail-fast activity/sizing:

- construct one CleanupUnit candidate per resolved workspace, not per RawGroup;
- map target and build OutputRoots to every physical group they touch;
- deduplicate equal/nested roots;
- aggregate pre-clean size as the union of all affected physical groups;
- keep a deterministic order, preferably descending aggregated bytes then workspace path.

A workspace with no reportable/inactive affected output should not become a CleanupUnit.

A workspace whose target/build groups cannot be mapped unambiguously is skipped/uncertain.

### 7.2 Require all affected groups to be cleanable

Replace the current single-RawGroup authorization decision with a unit-wide gate.

For every affected physical group:

- ownership must be PrivateBounded;
- location authorization must pass;
- output must be a real non-symlink directory;
- activity must remain outside the recency window;
- required Cargo cache markers must be valid.

If any affected group fails, return one skipped result for the workspace CleanupUnit with an actionable reason naming the failing output kind/path/class.

Do not spawn Cargo for any subset.

### 7.3 Revalidate the complete bounded ownership graph

Refactor final_cleanup_proof into a CleanupUnit proof that receives:

- the candidate workspace/unit;
- the complete initially discovered workspace set for clean ROOT;
- clean ROOT and configured authorization;
- the Cargo runner and recency policy;
- mutable counters/timings for stats accumulation.

Immediately before disposition:

1. re-resolve every workspace in the bounded ownership universe;
2. rebuild the complete fresh physical graph;
3. verify the candidate workspace identity/member set is unchanged;
4. verify target and build logical/physical identities are unchanged;
5. remap the complete candidate OutputSet to fresh physical groups;
6. require the same expected physical covering union;
7. require every affected fresh group to be PrivateBounded and authorized;
8. reject if any second discovered workspace now overlaps the candidate's output;
9. repeat source activity for the candidate workspace;
10. repeat output activity for every affected physical covering root;
11. validate root existence/type/no-symlink and Cargo markers;
12. freeze Cargo environment/arguments from the fresh OutputSet.

The proof object should bind the complete output union and all data needed for the Cargo spawn so no earlier RawGroup is consulted for mutation decisions after proof generation.

### 7.4 Make post-clean measurement unit-wide

After successful Execute:

- measure the complete deduplicated covering-root union bound by the proof;
- compare against the CleanupUnit pre-clean union;
- report one observed decrease for that workspace invocation.

If one output root disappears and another remains, measure the surviving union normally.

Measurement uncertainty after Cargo success is diagnostic but must not fabricate recovered bytes.

### 7.5 Align Preview and Simulate reporting with CleanupUnit

Preview:

- one Cargo dry-run invocation per CleanupUnit;
- one preview result per workspace;
- one aggregated pre-clean estimate.

Simulate:

- one would-clean result per CleanupUnit;
- no Cargo clean invocation;
- one aggregated estimated would-clean value.

Execute:

- one cleaned/failed/skipped result per CleanupUnit;
- one pre/post union measurement.

Progress determinate totals for cleanup phases count CleanupUnits, not physical groups.

The read-only scan report remains physical-group oriented and does not change.

### 7.6 Accumulate final-proof stats

Pass or merge a ScanCounters-like accumulator through final ownership-universe re-resolution.

cleanup --stats must include:

- initial locate/metadata work;
- final-proof locate/metadata work;
- final-proof source/output activity timing where practical;
- total cleanup elapsed.

If useful for debugging, add explicit proof_locate/proof_metadata counts or timings rather than hiding the extra work inside the initial counters. Either design is acceptable if the output is unambiguous.

Default output remains unchanged and quiet.

## 8. Ordered work packages

### A — CleanupUnit domain and mapping

Add the cleanup-specific workspace-to-physical-group mapping and deduplicated union helpers.

### B — Unit-wide authorization

Require every target/build physical group affected by one Cargo invocation to pass ownership and authorization.

### C — Full ownership-universe final proof

Re-resolve the bounded workspace set, rebuild the complete fresh graph, and detect another workspace moving into overlap.

### D — Cargo invocation and accounting alignment

Make one unit correspond to one Preview/Simulate/Execute result and one post-clean union measurement.

### E — Stats/instrumentation repair

Accumulate final-proof re-resolution work into --stats.

### F — Regression qualification and documentation

Run mixed target/build ownership fixtures, two-workspace race fixtures, hosted CI/MSRV, README/planning reconciliation, and C003 closure.

## 9. Required tests

### Mixed target/build ownership matrix

At minimum:

- target PrivateBounded + build PrivateBounded, distinct siblings => one cleanable unit, one Cargo invocation, one result;
- target PrivateBounded + build ExternalUnproven => entire unit skipped;
- target PrivateBounded + build Shared => entire unit skipped;
- target PrivateBounded + build Uncertain => entire unit skipped;
- target ExternalUnproven + build PrivateBounded => entire unit skipped;
- target == build PrivateBounded => one group, one unit, one invocation;
- build nested under target => deduplicated one physical union;
- target nested under build => deduplicated one physical union;
- distinct private target/build both inside workspace => aggregated pre/post measurement includes both once.

Run the relevant cases in Preview, Simulate, and Execute decision paths.

### One invocation / one result

For a workspace with distinct target and build:

- Preview records exactly one Cargo clean --dry-run call;
- Simulate records zero Cargo clean calls and exactly one Simulated result;
- Execute records exactly one Cargo clean call and exactly one Cleaned result;
- cleanup progress total is one;
- no second vanished/skipped row is emitted for the sibling output group.

### Cross-workspace final ownership race

Use a staged runner/config fixture:

1. initial A target/build and B target/build do not overlap;
2. after initial analysis but before A final proof, B re-resolves to overlap A target;
3. fresh full graph classifies the region Shared;
4. A is skipped before Cargo clean.

Repeat for B moving into A's distinct build directory.

Also cover:

- B changes from overlap to non-overlap: do not rely on stale Shared state; fresh proof decides conservatively from the complete graph;
- a fresh workspace fails to resolve during final universe refresh: candidate cleanup fails closed because ownership cannot be proven;
- candidate A itself changes target/build: existing C002 behavior remains;
- B becomes symlink/uncertain: fail closed when it affects A's ownership proof.

### Accounting

- pre-clean bytes equal deduplicated union of all target/build groups in the unit;
- execute post-clean bytes measure the same union;
- observed decrease equals saturating pre minus post;
- equal/nested roots never double-count;
- distinct sibling roots are both counted;
- post-clean disappearance of one sibling is handled;
- measurement uncertainty yields no fabricated observed decrease.

### Stats

- --stats includes final-proof Cargo re-resolution calls;
- one candidate in a two-workspace ownership universe shows the expected extra bounded re-resolution count;
- default stdout/stderr remains unchanged without --stats;
- --dryrun --no-progress --stats remains the canonical non-mutating qualification path.

## 10. Failure and contention semantics

- Failure to re-resolve any workspace that could affect the bounded ownership graph makes the candidate CleanupUnit non-cleanable for that pass.
- One candidate failure does not abort unrelated CleanupUnits unless the ownership universe itself becomes globally unavailable.
- Cargo cleanup remains sequential.
- The full ownership-universe refresh may be more expensive than C002's one-workspace proof; this is acceptable at the destructive boundary.
- Do not add parallel destructive proof resolution unless later evidence shows it is needed and deterministic/fail-closed behavior is preserved.
- Ctrl-C before Cargo spawn causes no mutation; after spawn, Cargo process semantics remain the authority.

## 11. Performance boundary

C003 is a cleanup-safety corrective, not the generic-scan optimization milestone.

The final ownership-universe refresh adds bounded Cargo work proportional to discovered workspaces under clean ROOT. Record this cost with --stats.

Do not weaken the proof to recover speed.

The previously observed full-root scan exceeding 120 seconds remains a separate future performance/pruning line. No C003 acceptance criterion requires improving no-argument scan latency.

## 12. Compatibility effects

- CLI flags remain unchanged.
- Config schema remains unchanged.
- Scan output remains physical-group oriented.
- Cleanup output becomes workspace/CleanupUnit oriented where distinct target/build previously produced multiple group rows.
- Some configurations previously allowed to clean will now skip when any sibling output group is not independently safe. This is an intentional safety correction.
- allowed_output_roots remains authorization only and cannot establish ownership.
- Rust 1.89 remains unchanged.

## 13. Verification commands

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
git diff --check

Hosted stable Linux/macOS/Windows plus Rust 1.89 must pass.

Static review must confirm:

- no production recursive output deletion;
- no Cargo clean spawn is reachable from a partially authorized OutputSet;
- every Cargo clean spawn consumes a CleanupUnit proof;
- Simulate never spawns Cargo clean;
- one workspace cleanup invocation cannot produce multiple independent cleaned results.

## 14. Documentation/planning updates

Update:

- README cleanup semantics from physical-group action to workspace CleanupUnit action;
- README explanation for distinct target/build mixed ownership;
- subsystem roadmap and registry;
- C003 closure record.

Do not rewrite M005B or C002 closure history.

ADR 001's core decision remains valid: workspace is the logical ownership unit, OutputSet contains target/build, physical groups deduplicate storage, and shared/external-unproven output is inventory-only. The C003 clarification added to ADR 001 makes the previously implicit workspace-wide destructive atomicity explicit.

## 15. Acceptance criteria

C003 closes only when:

- one Cargo clean invocation is authorized as one CleanupUnit covering the complete workspace OutputSet;
- every physical group affected by that invocation must be PrivateBounded and authorized;
- a private target cannot carry an ExternalUnproven/Shared/Uncertain build directory into Cargo clean;
- distinct private target/build directories produce one Cargo invocation and one cleanup result;
- post-clean measurement covers the complete deduplicated output union;
- final proof re-resolves the complete bounded ownership universe and detects another discovered workspace moving into overlap;
- failure to prove the fresh ownership graph fails closed;
- Preview/Simulate/Execute share the same unit-wide proof;
- Simulate invokes zero Cargo clean commands;
- cleanup progress totals count CleanupUnits;
- cleanup --stats includes final-proof work;
- full hosted CI/MSRV is green;
- no direct recursive output deletion exists.

## 16. Stop conditions

Stop and report rather than improvise if:

- current Cargo behavior can mutate an output path that cannot be represented in the resolved OutputSet;
- safe cleanup would require parsing Cargo-private cache internals;
- a partial target-only/build-only cleanup is needed to satisfy the plan;
- proving shared ownership would require scanning outside the explicit cleanup scope and treating absence as exclusivity proof;
- Cargo cannot reliably re-resolve the candidate OutputSet immediately before spawn;
- workspace-level atomic cleanup conflicts with a current canonical decision rather than clarifying it.

## 17. Closure evidence

Create:

plans/closure/artifact-discovery-cleanup/c003-status.md

The closure record must include:

- implementation commit/PR range;
- C003-F1 through F4 requirement-to-evidence matrix;
- mixed target/build ownership matrix;
- proof that one Cargo invocation maps to one CleanupUnit result;
- Simulate zero-clean trace;
- two-workspace overlap-race evidence from full fresh graph revalidation;
- target/build union pre/post accounting evidence;
- cleanup progress total evidence;
- --stats proof-work accounting;
- hosted Linux/macOS/Windows + Rust 1.89 results;
- static no-direct-deletion review;
- unresolved findings and final disposition.
