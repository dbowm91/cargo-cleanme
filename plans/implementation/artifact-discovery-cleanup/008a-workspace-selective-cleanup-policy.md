# M008A — Workspace Selective Cleanup Policy

Status: closed

Repository baseline: `3ee9699a0b0d987287d08e427195284e08d079f7`

Source roadmap: Phase 9 — Selective cleanup and policy

Architecture dependencies:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
- `plans/adr/002-adaptive-routine-full-discovery-state.md`
- M007/C006 combined selected-root cleanup proof
- C007 discovery-state recovery and planning closure

Primary class: cleanup selection policy / destructive safety preservation

## 1. Objective

Add workspace-level selective cleanup policy without changing the ownership universe or Cargo mutation boundary established by C003/C004/C006.

This milestone adds:

- a minimum reclaimable-size threshold;
- a minimum inactivity-age threshold;
- canonical workspace include/exclude policy;
- typed policy dispositions suitable for later machine-readable reporting;
- fresh pre-spawn size/age policy revalidation using work already performed by the final cleanup proof.

The destructive unit remains one complete workspace `CleanupUnit` over the complete resolved `OutputSet`. Policy decides whether an already-proven unit is selected; policy does not redefine which manifests, workspaces, or physical outputs participate in ownership proof.

## 2. Readiness and dependencies

M008A is ready because:

- M007/C006 provide one combined selected-root manifest/ownership universe;
- Preview/Simulate/Execute already share one final `ExecutionProof`;
- every `CleanupUnit` already exposes deduplicated pre-clean bytes;
- output analysis already retains newest output mtime;
- traversal already computes newest quiet source mtime, although the workspace layer currently collapses that value to a boolean;
- config bootstrap/edit and deterministic CLI parsing are closed.

No Phase 10 distribution work is required.

## 3. Canonical invariants

M008A MUST preserve all existing destructive invariants.

In particular:

1. policy filtering happens only after complete selected-root manifest discovery and authoritative Cargo workspace/output resolution;
2. excluded or policy-rejected workspaces remain members of the ownership universe and can make another unit Shared or otherwise non-cleanable;
3. policy never converts `Shared`, `Uncertain`, `ExternalUnproven`, unauthorized, active, symlinked, marker-invalid, or unresolved output into cleanable output;
4. machine-learned state remains search input only and never policy/ownership authorization;
5. Preview, Simulate, and Execute use the same policy decision and the same final non-mutating proof for identical filesystem state;
6. dynamic size and age policy is re-evaluated from fresh observations immediately before Cargo Preview/Execute disposition;
7. no policy option may cause cargo-cleanme to parse Cargo-private target/build directory layout;
8. default configuration with no policy preserves current cleanup selection behavior.

Selection policy is strictly narrower than the safety envelope. It may reject a safe candidate; it may never admit an unsafe one.

## 4. User-facing contract

Preferred CLI direction:

~~~text
cargo-cleanme clean ROOT    [--min-size SIZE] [--older-than DURATION] [--include PATH_GLOB]... [--exclude PATH_GLOB]... [--dry-run|--dryrun|--yes]
cargo-cleanme clean --known [same policy flags]
cargo-cleanme clean --full  [same policy flags]
~~~

Exact clap field names may adapt to existing style, but the semantic contract is fixed.

Preferred config direction:

~~~toml
[cleanup]
allowed_output_roots = []

[cleanup.policy]
# Optional; absent/0 means no additional size threshold.
min_reclaimable_bytes = 0

# Optional additional inactivity threshold. The scan recency guard remains
# the minimum safety guard regardless of this value.
# min_inactive_seconds = 2592000

# Canonical workspace-root glob policy. Empty include means include all.
include = []
exclude = []
~~~

CLI policy overrides config for the current invocation. If both config and CLI include/exclude lists are accepted simultaneously, define whether CLI replaces or augments config; do not silently mix them. Preferred direction is replacement for any policy field explicitly supplied on CLI because it is easier to reason about and test.

Human-friendly parsers MAY accept binary size suffixes and duration suffixes, but configuration serialization should remain explicit and stable. Reject overflow and malformed units; do not guess.

## 5. Policy semantics

### 5.1 Minimum size

The threshold applies to the complete deduplicated physical output union affected by one `CleanupUnit`, not an individual directory and not a whole selected root.

Initial analysis may cheaply reject units whose measured `unit.bytes` is below the threshold before expensive final proof.

However, a unit that passes the initial threshold MUST pass it again using the fresh bytes observed by the final proof immediately before disposition.

A size decrease between initial analysis and final proof may therefore turn a selected unit into a policy skip.

### 5.2 Minimum inactivity age

Age is based on the newest trustworthy activity associated with the cleanup unit:

~~~text
last_activity = max(
    newest source mtime across all canonical workspace member source trees,
    newest output mtime across every affected physical covering root
)
~~~

A unit passes an `older-than` threshold only when every required activity observation is trustworthy and `last_activity < now - threshold`.

The existing `scan.recency_seconds` activity guard remains a hard safety gate. Policy may impose a longer quiet interval but must never shorten or bypass the recency guard.

Future timestamps are active. Unreadable/uncertain required timestamps are non-cleanable, not “old.”

### 5.3 Include/exclude workspace policy

Matching is against canonical resolved workspace identity/root, not arbitrary discovered directories, member source paths, target paths, or learned-root records.

Precedence:

- an empty include set means all resolved workspaces are provisionally included;
- a non-empty include set requires at least one include match;
- any exclude match rejects the workspace even when it also matches include;
- policy matching affects selection only, never ownership-universe membership.

Patterns should use the existing glob machinery where practical, but compilation must occur once per invocation rather than inside per-unit hot paths.

Path behavior must be explicitly tested on Unix and Windows. Matching must not depend on lossy UTF-8 conversion where the current path model can preserve native paths.

## 6. Required domain changes

Introduce a typed policy layer rather than embedding decisions in rendered strings.

Likely concepts:

~~~text
CleanupPolicy
CleanupPolicyDecision
CleanupPolicyDisposition
ActivityObservation / WorkspaceActivity
~~~

Required disposition distinctions include at least:

- Selected;
- BelowMinimumSize;
- TooRecentForPolicy;
- NotIncluded;
- Excluded.

Safety skips remain separate from policy skips. Do not collapse `Shared` or unresolved ownership into “excluded.”

Preserve the newest quiet source timestamp currently produced by `traverse::SourceActivity::Quiet(Option<SystemTime>)` through workspace analysis instead of reducing it to `Result<bool, ()>`.

The exact structs may differ, but the model must be serializable later without parsing human text.

## 7. Work package A — Config and CLI

1. Add a typed `CleanupPolicyConfig` under `cleanup.policy`.
2. Preserve backward compatibility for existing configs containing only `cleanup.allowed_output_roots`.
3. Validate size/duration ranges and policy patterns during config load.
4. Add CLI policy overrides to all three cleanup selectors: ROOT, `--known`, and `--full`.
5. Preserve existing `--dry-run` / `--dryrun` / `--yes` pairwise conflicts and root/known/full conflicts.
6. Update the checked-in config template and first-use bootstrap fixtures.
7. Ensure `config show` and `config edit` round-trip the new fields.

No policy flag is allowed on read-only scan unless a separate product decision explicitly extends reporting behavior.

## 8. Work package B — Preserve activity timestamps

Refactor workspace activity analysis so it returns both recency classification and the newest observed source timestamp.

Requirements:

- retain current early-exit behavior when a recent/future source timestamp is found;
- retain uncertainty semantics;
- do not add a second source traversal merely to compute age;
- preserve exclusion of resolved output roots, VCS boundaries, and independent nested repositories;
- aggregate newest source activity across workspace members deterministically;
- store enough activity information in the measured candidate/unit path for policy evaluation.

The normal no-policy path must not materially regress traversal cost.

## 9. Work package C — Initial policy selection

After complete workspace/output resolution, complete physical grouping, and CleanupUnit construction:

1. evaluate canonical workspace include/exclude;
2. evaluate initial minimum size from `CleanupUnit.bytes`;
3. evaluate initial age from retained source/output activity;
4. record a typed policy decision;
5. avoid final ownership-proof work for policy-rejected units.

This optimization MUST NOT remove rejected units/workspaces from the combined ownership universe passed to later proofs.

Progress totals/reporting should distinguish units considered from units selected, without treating a policy skip as an error.

## 10. Work package D — Final proof freshness

Extend the final proof to produce fresh policy inputs while performing its existing traversal.

For each candidate:

- source revalidation already walks all canonical member roots; retain the newest quiet timestamp during this walk;
- output revalidation already calls `measure_single_target` for every covering root; accumulate both fresh bytes and newest output timestamp from those exact calls;
- derive fresh `last_activity`;
- re-evaluate minimum size and age after all safety gates needed to trust those values;
- if policy no longer passes, return a typed policy skip without spawning Cargo.

Do not perform an additional size traversal solely for policy.

`ExecutionProof.pre_bytes` should use the freshly measured byte union rather than stale initial `unit.bytes` once this work is complete.

Include/exclude matching may be rechecked defensively against the fresh canonical workspace identity, but it need not trigger another filesystem walk.

## 11. Work package E — Reporting

Human reports must make policy skips understandable without overwhelming normal output.

Required summary distinctions:

- units considered;
- units selected by policy;
- units policy-skipped;
- units safety-skipped/failed;
- previewed/simulated/cleaned totals.

Per-unit output may include policy skip reasons where useful, but stable machine-readable representation is M008B.

Deterministic ordering remains size-descending with stable path tie-break unless a stronger existing contract applies.

`--stats` remains stderr-only and must not change stdout.

## 12. Failure, cancellation, and contention semantics

- malformed policy config/CLI is a startup error before discovery;
- pattern compilation failure is a startup error;
- activity uncertainty remains a safety/eligibility failure, never a policy “too recent” classification;
- a unit falling below threshold between initial analysis and final proof is skipped cleanly;
- a unit becoming active between initial analysis and final proof is a safety skip under the existing activity rule;
- cancellation stops launching new Cargo work exactly as before;
- policy evaluation creates no new shared mutable state and must remain deterministic under concurrent analysis.

## 13. Required tests

### Config/CLI

- old config files deserialize unchanged;
- default policy is behaviorally neutral;
- malformed/overflow size and duration reject;
- include/exclude pattern validation;
- CLI precedence over config;
- ROOT/known/full mode matrix with policy flags;
- Cargo external-subcommand argv normalization.

### Selection semantics

- below-minimum size skips;
- exact boundary semantics are explicit and tested;
- source-old/output-old passes age;
- recent source with old output fails;
- old source with recent output fails;
- future source/output fails safely;
- missing/uncertain required activity fails safely;
- include-only match passes;
- missing include match skips;
- exclude wins over include;
- canonical workspace identity is matched rather than member/target paths.

### Ownership preservation

- excluded workspace sharing output with an included workspace still makes the physical group Shared;
- policy-rejected workspace remains present during complete ownership refresh;
- unresolved participant blocks scope even when it would have been excluded by policy;
- allowed_output_roots still cannot manufacture ownership;
- C003/C004/C006 race and combined-root fixtures remain green.

### Fresh proof

- unit passes initial min-size, shrinks below threshold, and no Cargo clean is spawned;
- unit passes initial age, receives a fresh source edit, and no Cargo clean is spawned;
- fresh proof byte total becomes Preview/Execute pre-clean measurement;
- Simulate/Preview/Execute produce the same policy disposition on identical immutable fixtures;
- `--dryrun` still invokes zero Cargo clean commands.

### Platform/MSRV

- Linux/macOS/Windows hosted CI;
- Rust 1.89 check/test;
- Windows path-pattern fixtures;
- non-UTF-8 Unix paths do not crash or silently broaden selection.

## 14. Verification commands

At minimum:

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
rustup run 1.89 cargo check --locked --all-targets
rustup run 1.89 cargo test --locked --all-targets
git diff --check
~~~

Hosted CI must pass on Linux, macOS, and Windows.

## 15. Documentation updates

Update:

- README cleanup examples;
- canonical config template comments;
- Phase 9/subsystem status;
- CLI distinction between scan filtering and cleanup selection policy;
- explanation that excluded workspaces still participate in ownership proof;
- explanation that `older-than` is stricter than, not a replacement for, the recency safety guard.

## 16. Acceptance criteria

M008A may close only when:

- default no-policy cleanup behavior remains regression-compatible;
- minimum size, minimum inactivity age, and canonical workspace include/exclude work for ROOT/known/full;
- source age is preserved without an extra source traversal;
- fresh final proof recomputes byte/activity inputs without an extra dedicated policy traversal;
- excluded/policy-rejected workspaces remain in the ownership universe;
- Preview/Simulate/Execute share one policy decision path;
- policy cannot weaken C003/C004/C006 safety;
- all focused, broad, hosted platform, and Rust 1.89 gates pass.

## 17. Non-goals

- package-selective cleanup;
- profile-selective cleanup;
- target-triple/doc-only cleanup;
- Cargo global cache GC;
- direct filesystem deletion;
- interactive full-screen selection UI;
- daemon/scheduler implementation;
- machine-readable public schema (M008B);
- changing Routine/Full discovery policy.

## 18. Stop conditions

Stop and require a corrective/architecture review if implementation would require:

- filtering manifests/workspaces out before ownership graph construction;
- parsing Cargo-private target/build layout;
- weakening unresolved-participant or shared-output blocking;
- performing a second deep target traversal solely for policy;
- treating missing/uncertain timestamps as old;
- changing the CleanupUnit destructive boundary.

## 19. Closure evidence

The closure record must include:

- implementation commit(s);
- policy requirement-to-test matrix;
- evidence that excluded workspaces remain in ownership proof;
- fresh-size/fresh-age race fixtures;
- Preview/Simulate/Execute parity evidence;
- no-policy compatibility evidence;
- hosted Linux/macOS/Windows CI;
- Rust 1.89 gates;
- any observed performance delta for cleanup selection/final proof.

Current disposition and remaining qualification are recorded in `plans/closure/artifact-discovery-cleanup/008a-status.md`. The typed policy disposition is frozen in the M008B output DTO; M008B may proceed against that interface while M008A's hosted and race-specific qualification remains outstanding.
