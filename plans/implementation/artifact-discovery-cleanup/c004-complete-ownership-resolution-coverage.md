# Corrective C004 — Complete Ownership-Universe Resolution Coverage

Status: closing

Repository baseline: 819f71cd50bebb3b68ae54ef22e6951f08edcbbe

Corrects:

- C003 workspace cleanup-unit atomicity and full ownership-graph revalidation
- C003 closure remains historical and is not rewritten

Authoritative references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

Primary class: corrective / destructive safety / fail-closed ownership

## 1. Objective

Close the remaining gap between the discovered Cargo-manifest set under clean ROOT and the resolved workspace set used as the destructive ownership universe.

C003 re-resolves every workspace that successfully entered the initial universe. However, a discovered Cargo.toml whose cargo locate-project or cargo metadata resolution fails is currently diagnosed and then dropped. Other successfully resolved CleanupUnits may still proceed.

C004 must make destructive qualification fail closed whenever the bounded ownership universe is incomplete.

Read-only scan behavior remains tolerant: unresolved Cargo manifests remain diagnostics and do not fail the whole scan.

Preview, Simulate, and Execute must share the same completeness requirement so the clean command has one safety model.

Generic scan performance, C003's O(N²) final-proof cost, and the macOS source-root canonicalization optimization remain separate work.

## 2. Finding

### C004-F1 — discovered but unresolved manifests disappear from the ownership universe

Severity: high / destructive ownership incompleteness.

At the baseline:

1. discovery returns a set of Cargo.toml manifests under explicit clean ROOT;
2. workspace::resolve_workspaces attempts cargo locate-project and cargo metadata;
3. a failed locate/metadata/parse path records a diagnostic and continues;
4. resolve_cleanup_scope builds groups and CleanupUnits only from the successfully returned Vec<ResolvedWorkspace>;
5. final_cleanup_proof re-resolves only that successful workspace universe.

A discovered manifest that never becomes covered by a resolved workspace therefore has no representation in the final ownership graph.

Example:

- A/Cargo.toml resolves to A with target A/target;
- B/Cargo.toml is discovered;
- B's Cargo resolution fails under the conservative offline/locked metadata invocation;
- B may nevertheless be configured by Cargo to use A/target;
- B is absent from the ownership universe;
- A may be proven PrivateBounded and cleaned.

Because cargo-cleanme intentionally delegates Cargo configuration semantics to Cargo, it cannot independently prove that unresolved B is irrelevant to A's output.

The safe conclusion is that destructive ownership is incomplete.

## 3. Why prior verification missed it

- C003 tested failure to re-resolve a workspace that had already entered the initial ownership universe; that path fails closed correctly.
- It did not test a manifest that was discovered but failed before producing any initial ResolvedWorkspace.
- resolve_workspaces returns only successes, so callers cannot distinguish “all discovered manifests are accounted for” from “some disappeared after a diagnostic.”
- C003's phrase “complete bounded ownership universe” therefore described the successful-resolution set rather than resolution coverage of the discovered-manifest set.

## 4. Resolution-coverage model

Introduce a cleanup-scope resolution result equivalent in responsibility to:

ResolutionCoverage {
    workspaces: Vec<ResolvedWorkspace>,
    unresolved: Vec<UnresolvedOwnershipParticipant>,
    discovered_manifest_count: usize,
}

An unresolved ownership participant records enough stable evidence for diagnostics/tests, at minimum:

- discovered manifest path;
- resolution stage (locate, metadata, parse/identity as applicable);
- concise reason/category.

The exact public/internal type names are implementation choices.

### Coverage rule

A discovered manifest is considered accounted for when either:

1. it directly resolves successfully to a workspace; or
2. a successfully resolved workspace's authoritative Cargo metadata lists that manifest as a member/root manifest.

This matters for member-first discovery and transient/redundant resolution attempts.

A failed locate/metadata attempt MUST NOT permanently mark a manifest unresolved if later successful authoritative workspace metadata covers it in the same resolution pass.

At the end of workspace resolution:

unresolved = discovered manifests - authoritative manifest coverage

using canonical/absolute manifest identity consistent with the existing member cache.

Do not infer coverage from path ancestry or hand-parsed Cargo.toml.

## 5. Destructive safety rule

For clean ROOT:

- Scan/read-only inventory may proceed with unresolved manifests as diagnostics.
- Clean Preview, Simulate, and Execute MUST NOT invoke any cargo clean command for any CleanupUnit while unresolved ownership participants remain in the bounded scope.
- No successfully resolved unit may be treated as privately owned when the ownership universe is incomplete.
- cleanup.allowed_output_roots does not override this rule.
- An unresolved manifest outside clean ROOT is irrelevant to this bounded proof; C004 does not perform a whole-machine ownership search.
- External output remains governed independently by ExternalUnproven/Shared/Uncertain rules from ADR 001.

This is a scope-wide fail-closed gate, not a per-unit path-overlap heuristic: without Cargo resolution, cargo-cleanme has no sound output path to compare.

## 6. User-visible behavior

When cleanup-scope resolution is incomplete:

- do not spawn cargo clean or cargo clean --dry-run;
- do not enter per-unit final ownership proof;
- preserve diagnostics for each unresolved participant internally;
- emit a concise actionable summary such as:
  “cleanup ownership could not be proven: 2 discovered Cargo manifests did not resolve; no cleanup commands were run”;
- under --stats, include unresolved_ownership=<N>;
- verbose/per-diagnostic output may name individual manifests according to existing diagnostics policy.

If CleanupUnits were otherwise reportable, they may be rendered as skipped with a common scope-level reason, or the report may carry one scope-level blocked state. Choose one representation and keep it deterministic.

No output should imply recovered/would-clean bytes when the entire scope is blocked before destructive qualification.

## 7. Required production changes

### 7.1 Preserve resolution coverage

Refactor workspace resolution so cleanup callers can observe:

- successful workspaces;
- authoritative manifest coverage;
- unresolved discovered manifests after the complete pass.

Prefer preserving the existing resolve_workspaces convenience API for read-only callers if that minimizes churn, while adding a richer resolver result for cleanup.

The existing member_to_root cache is the natural coverage source:

- seed root/member manifest identities from successful Cargo metadata;
- after all manifests are processed, compare the canonical discovered-manifest set against that authoritative coverage;
- retain failure details for manifests still uncovered.

### 7.2 Make cleanup scope completeness explicit

resolve_cleanup_scope must return an ownership-universe completeness record rather than only (workspaces, units).

Before announcing a cleanable unit total or entering unit_block_reason/final_cleanup_proof:

- test completeness;
- if unresolved participants remain, block the entire cleanup scope.

No Cargo clean subprocess is allowed in this state in Preview, Simulate, or Execute.

Cargo locate-project/metadata subprocesses are still allowed because they are resolution, not cleanup.

### 7.3 Preserve C003 final proof

When initial coverage is complete, C003's full bounded ownership-universe final proof remains unchanged in strength:

- re-resolve every workspace;
- rebuild the complete physical graph;
- fail if any previously resolved workspace no longer resolves;
- prove complete CleanupUnit OutputSet ownership;
- then allow Preview/Simulate/Execute disposition.

C004 is an additional initial-universe completeness gate, not a replacement for C003's final refresh.

### 7.4 Stats and diagnostics

Extend cleanup statistics with a clearly named count, e.g.:

unresolved_ownership=<N>

Do not merge it into cargo_failures in a way that makes the ownership safety state invisible; existing cargo failure counts may remain for process-level instrumentation.

If resolution coverage becomes complete because a later authoritative workspace metadata response covers a manifest whose earlier direct attempt failed, unresolved_ownership must be zero.

### 7.5 Keep read-only scan semantics unchanged

The normal scan command remains partial-result tolerant.

A manifest resolution failure may produce a diagnostic and omit that workspace/output from inventory, as today.

Do not make a machine-wide scan fail because one Cargo project is malformed, locked inconsistently, offline-incomplete, or otherwise unresolved.

The stricter rule applies only to cleanup-scope ownership proof.

## 8. Ordered work packages

### A — ResolutionCoverage domain/internal model

Add unresolved ownership participant and authoritative manifest-coverage tracking.

### B — Resolver reconciliation

Make member/root metadata coverage clear earlier failed member attempts and produce the final unresolved set.

### C — Cleanup scope fail-closed gate

Block Preview/Simulate/Execute before any Cargo clean invocation when unresolved ownership remains.

### D — Stats/reporting

Expose unresolved ownership count and concise deterministic blocked output.

### E — Regression qualification

Add duplicate-member, initial-failure, all-modes, and hosted cross-platform tests.

### F — Documentation/planning closure

Update README safety semantics, roadmap/registry, and create C004 closure evidence.

## 9. Required tests

### Coverage accounting

- one discovered root manifest resolves successfully => complete;
- N member manifests in one workspace, first member locate fails, later root resolves and metadata lists all members => complete, unresolved count zero;
- member resolves first and metadata covers root/later members => complete;
- one manifest locate fails and no successful metadata covers it => unresolved count one;
- locate succeeds but metadata fails and no later workspace covers it => unresolved count one;
- malformed locate/metadata output => unresolved unless later authoritative coverage clears it;
- two independent workspaces, one resolves and one fails => one resolved workspace + one unresolved participant;
- canonical/symlinked spelling of the same manifest does not create a false unresolved duplicate where platform semantics permit canonicalization.

### Cleanup fail-closed behavior

For a scope containing cleanable A plus unresolved B:

- Preview => zero cargo clean calls, including zero cargo clean --dry-run;
- Simulate => zero cargo clean calls;
- Execute => zero cargo clean calls;
- A is not reported Previewed/Simulated/Cleaned;
- report states ownership universe is incomplete;
- --stats reports unresolved_ownership=1.

Repeat with:

- B locate failure;
- B metadata failure;
- B malformed metadata;
- B later covered by successful workspace metadata (cleanup is allowed if all other gates pass).

### Interaction with C003

- complete initial universe + B fails only during final refresh => existing C003 fail-closed behavior still blocks A;
- complete initial universe + B moves into A overlap => existing C003 overlap test remains green;
- mixed target/build ownership tests remain green;
- one-invocation/one-result tests remain green;
- Simulate zero-clean invariant remains green.

### Read-only scan compatibility

- same unresolved B under scan produces diagnostics/partial results, not a fatal scan error;
- scan stdout ordering/result semantics for successfully resolved workspaces remain unchanged.

## 10. Failure semantics

- Incomplete cleanup ownership coverage is a safety block, not permission to fall back to conventional-target assumptions.
- Do not attempt to parse Cargo config to recover the missing output path.
- Do not guess workspace identity from directory structure.
- Do not ignore an unresolved manifest because it has no local target directory; redirected output is exactly why that inference is unsound.
- A transient Cargo-resolution failure requires the user to retry after the underlying condition is corrected.
- One unresolved participant blocks all cleanup units in that explicit clean ROOT because its output relationship is unknown.

## 11. Compatibility effects

- CLI flags and config schema remain unchanged.
- Read-only scan remains partial-result tolerant.
- Preview becomes stricter: it will not invoke Cargo dry-run if ownership coverage is incomplete.
- --dryrun remains fully non-mutating and now reflects the same global ownership-completeness gate as Execute.
- Some cleanup scopes that previously cleaned other resolvable workspaces despite an unrelated unresolved Cargo manifest will now skip all cleanup. This is intentional fail-closed narrowing.
- Rust 1.89 remains unchanged.

## 12. Performance boundary

The added coverage set comparison is in-memory and should be negligible relative to Cargo resolution.

Do not use C004 to optimize C003's N² final ownership refresh.

Do not broaden into global scan pruning or macOS member-root canonicalization.

If preserving failure detail materially increases per-manifest memory, retain only bounded structured information necessary for deterministic diagnostics.

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

- every discovered cleanup-scope manifest is either authoritatively covered or retained unresolved;
- unresolved ownership blocks every Cargo clean spawn;
- Preview cannot invoke cargo clean --dry-run when coverage is incomplete;
- Simulate still invokes zero Cargo clean commands;
- C003 CleanupUnit proof remains the only path to a cleanup spawn after completeness is established;
- no production recursive output deletion exists.

## 14. Documentation/planning updates

Update:

- README cleanup ownership proof to mention complete manifest-resolution coverage;
- registry and subsystem roadmap;
- C004 closure record.

Do not rewrite C003 closure history.

ADR 001 receives a clarification that an ownership universe is complete only when every discovered manifest is accounted for by authoritative Cargo workspace metadata; unresolved participants fail destructive qualification closed.

## 15. Acceptance criteria

C004 closes only when:

- cleanup retains explicit resolution coverage for every discovered manifest under clean ROOT;
- a failed direct manifest attempt can be cleared only by later authoritative Cargo metadata that covers that manifest;
- any remaining unresolved ownership participant blocks Preview, Simulate, and Execute for the whole cleanup scope;
- incomplete coverage produces zero Cargo clean invocations;
- complete coverage continues into the full C003 ownership-universe final proof;
- C003 mixed-output and overlap-race tests remain green;
- read-only scan remains partial-result tolerant;
- --stats exposes unresolved ownership count;
- hosted Linux/macOS/Windows and Rust 1.89 are green;
- no direct recursive output deletion exists.

## 16. Stop conditions

Stop and report rather than weakening the contract if:

- complete discovered-manifest coverage cannot be tracked without reimplementing Cargo workspace/config semantics;
- implementation requires treating a failed manifest as irrelevant based only on path layout or lack of local target;
- a per-unit heuristic is proposed that allows cleanup while an unresolved participant's output paths remain unknown;
- cleanup completeness can only be achieved by dropping offline/locked safety semantics without a separately approved policy change;
- read-only scan would need to become globally fatal to satisfy the cleanup-only rule.

## 17. Closure evidence

Create:

plans/closure/artifact-discovery-cleanup/c004-status.md

The closure record must include:

- implementation commit/PR range;
- C004-F1 requirement-to-evidence mapping;
- manifest coverage matrix;
- member-fails-first/later-metadata-covers regression;
- all-mode zero-clean evidence for incomplete ownership;
- C003 final-refresh regression evidence;
- read-only scan compatibility evidence;
- unresolved_ownership stats evidence;
- full hosted CI/MSRV results;
- static no-direct-deletion review;
- unresolved findings and final disposition.
