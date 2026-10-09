# C031 — Routine Cleanup Happy-Path, Ownership Scope, and Partial-Failure Corrective

Status: **closing** for the safe whole-scope path; **blocked** for partial-scope cleanup pending an accepted ownership decision.
Planning branch: `plans/c029-c031-permission-discovery-cleanup-reliability`.
Repository baseline: `f08705e77716e445e50097333a0313cab9574cdd` (`main`, 2026-10-09).
Source: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: C003 workspace atomicity, C004 manifest coverage, C006 combined roots, M007 learned cleanup, M012A canonical Routine Execute, M012B automation, and C028 availability semantics.
Hard implementation dependency: C029 (coverage/error provenance), C030 (effective Routine root hygiene), and an **accepted** decision resolving proposed ADR 004 before any narrowing of C004/C006's whole-scope fail-closed boundary. Research may begin now.
Normative: ADR 001 remains authoritative. ADR 004 is **proposed only**. Read the repository planning, discovery, cleanup-safety, JSON/exit, and test-evidence skills before implementation.

## 1. Incident and exact failure

On ordinary `cargo cleanme` after a Full scan, selected roots included Codex worktrees, Trash, Minimax/OpenClaw nested `node_modules`, Go modules, `/tmp`, and the developer's `~/projects`. Seven nonexistent learned Codex directories were correctly omitted (C028) before the engine, but the selected ten roots still contained 2,014 discovered manifests. Engine reported:

```text
combined scope: 10 root(s), 2014 manifest(s), 0 resolved workspace(s),
0 CleanupUnit(s), 0 unresolved ownership participant(s)
combined cleanup ownership universe is incomplete:
10 discovery diagnostic(s); no cleanup commands were run
```

`src/cleanup.rs` unconditionally treats `!discovered.diagnostics.is_empty()` as incomplete and **returns before** `resolve_cleanup_scope`. The zeros mean *not attempted*, not 2,014 failed resolutions. The next gate, `resolve_cleanup_scope`/C004, also conservatively blocks the entire selected universe if **any** discovered manifest stays unresolved, even after diagnostics are reduced. This second gate can still make the normal command unusable on mixed developer trees. `src/main.rs` prints the entire comma-joined selected root set and a state-generation prefix without useful concise blocking detail.

Prior C004/C006 tests deliberately validate that a bad manifest in root B blocks good root A and that all modes produce zero `cargo clean` spawns. This regression therefore exposes a *product expectation versus accepted safety design* gap, **not** a test that accidentally forgot to check a working partial-cleanup outcome. C028 tests demonstrate the different `NotFound` admission defect; they do not prove real user multi-root no-error completion. Earlier happy-path fixtures contain one or two clean, isolated workspaces and have little mixed-tool/cache content.

## 2. Two-track objectives

**Safe, immediately implementable track (after C029/C030):** make the existing complete-ownership pipeline reachable on the natural healthy scope, avoid false diagnostic blockage, and report real stage outcomes accurately, **without relaxing** any accepted destructive safety invariant. If the proven selected combined scope is fully traversed and fully resolved, at least one inactive private workspace should qualify. If any relevant discovered manifest or traversal region is uncertain, the current whole-scope block remains until a new safety decision is accepted.

**Conditional track (ADR 004 acceptance required):** determine whether independently proven cleanup components permit an unaffected scope to proceed while a disjoint uncertain region remains blocked. The target is not 'always clean good-looking workspaces'; it is 'clean only those with complete auditable ownership evidence'. Never build independent proof by deleting participants from the original universe.

## 3. Work packages, in order

### A. Premise-negative realistic default-path integration fixture (ready)

Build a disposable home-state fixture with valid `projects` workspace(s), nested npm/Go manifests (valid and broken), ephemeral deleted Codex worktrees, an inaccessible sibling, a sample Cargo `target`, recent/old source and output timestamps, and an explicitly recorded state generation. Run the real CLI as bare `cargo cleanme`, `--dry-run`, `scan --known`, `scan`, explicit `clean ROOT`, and (where feasible) an actual staged external `cargo cleanme` invocation.

Prove exactly which gate stopped work: validate diagnostic count, `cargo_locate_calls` and `cargo_metadata_calls` **zero** when blocked pre-resolution; resolve failures counted only after Cargo actually ran. Add an instrumented fake Cargo runner that asserts real command identity, argument paths and no unintended `cargo clean` spawns. Negative controls must fail on `f08705e` *for the reported gate*, not an unrelated invalid-root exit.

### B. Stage-correct reporting and operator contracts (ready)

Use a typed reason for `discovery_incomplete`, `ownership_unresolved`, and `no_eligible_units` without falsely describing 'zero workspaces' as a measured result. Consider an explicit `not_attempted` representation internally; if changing JSON v1 field values/shape is needed, register a versioned contract decision and test compatibility. Human output should show succinct selected-root summary by count and optional bounded illustrative roots; avoid printing long comma-joined paths twice. Keep `--format log` exactly one bounded ASCII line <=384 bytes, no absolute paths/Cargo stderr; JSON exactly one output object. Preserve dry-run's zero mutation and machine-readable stable reason codes. Distinguish filtered-out roots from discovery diagnostics, and actual Cargo metadata failures from coverage not attempted.

No reporting-only refactor may change eligibility or silently convert an exit 1 to 0.

### C. Existing proof-engine happy-path qualification (after C029/C030)

For a complete, healthy Routine scope prove that one or more normal workspaces can make it through admission, discovery, authoritative Cargo resolution, complete physical ownership grouping, inactivity, final pre-spawn proof, and Execute. `--dry-run` simulates the same decision with **zero** `cargo clean` spawns; Cargo preview runs only once complete proof exists. Protect redirected target/build, Shared/Uncertain/ExternalUnproven, source-tree containment, symlinked outputs, recent activity, and C028 reappearance/TOCTOU. Use a controlled profile/environment matrix so the test cannot pass merely because a real Cargo operation failed early. Ordinary default-path eligibility and failures are measured against baseline performance and memory.

### D. Independence proof research and explicit decision gate (research ready, execution blocked)

Read proposed ADR 004, current C004/C006 and final `ProofUniverse`/refresh code. Model actual Cargo target/build config/environment influence and whether any finite selected-root partition can prove an unresolved manifest outside it cannot share physical output. Record concrete counterexamples. Reproduce:
- healthy workspace A in `~/projects`, broken Cargo manifest B under a genuinely separate root; whether B can redirect into A's target is unknown;
- successfully resolved B with explicit or environment selected output overlapping A;
- unreadable B with unknown path in a broad Full walk; and
- late B appearance or config mutation before A's Cargo spawn.

Decide explicitly whether to (1) retain whole-scope fail-closed and prioritize hygiene/diagnostic usability, or (2) **accept a rigorous ADR 004 boundary and implement only certified partitioning**. If no proof stronger than filesystem location can be provided, record option (1); a 'per-root fallback' without proof is prohibited. If (2) is accepted, write a forward accepted ADR amendment naming which C004/C006 guarantee changes and why, and define proof/late-refresh/cancellation semantics *before modifying `cleanup.rs`*.

**Research result (2026-10-09): retain option (1).** A workspace can select
`target-dir` through Cargo configuration and environment; a successfully
resolved workspace can therefore redirect output into another root. An
unresolved manifest supplies no authoritative output set at all, and an
unreadable subtree supplies no manifest paths to resolve. A late workspace or
configuration change can also alter the participant graph; the current final
refresh is sound because it revalidates the complete frozen combined universe.
Filesystem disjointness between scan roots is not an independence certificate.
ADR 004 remains proposed and unaccepted, so the C004/C006 whole-scope block is
retained. No partitioning code or partial Execute path was added.

### E. Conditional independent-scope implementation (blocked)

Only following D/ADR acceptance: implement certificate-driven graph partitions, discovery provenance and partial block propagation. Every Cargo invocation remains one atomic complete `CleanupUnit`. Any cross-root overlap, unknown location, unresolved potential owner, environment ambiguity, or TOCTOU expands the blocked component. Evaluate full multi-root coverage before executing any component, and re-prove freshness against frozen effective roots and the exact participant universe before each spawn. Do not optimize by caching an incomplete universe. Preview/Simulate/Execute must agree on eligible/blocked components; Simulate spawns no Cargo clean, Preview cannot preview blocked units; Execute is Cargo-mediated only.

The implementation must include an explicit capability/feature kill-switch or rollback strategy so a failure in partition proof conservatively falls back to the original blocked combined-scope behavior. Document whether the operator-visible partial-success exit is 1 (recommended) and whether any new `partial` JSON status needs schema migration. Explicit `clean ROOT` and `clean --full` retain their existing safety model unless separately decided.

## 4. Required safety/contract matrix

| Case | Default allowed behavior under currently accepted ADR 001 |
|---|---|
| Healthy complete Routine union, normal inactive private target | Execute only after current full proof; Simulate mirrors with zero Cargo clean |
| Routine discovered manifest unresolved anywhere in selected union | Scope-wide block, zero Cargo clean |
| Routine inaccessible subtree anywhere in selected union | Scope-wide block, zero Cargo clean |
| Automatic missing learned root proven absent at admission | C028 omit + late check; other healthy roots may be proven and cleaned |
| Symlink/unreadable automatic admitted candidate | Typed scope block; no silent omission |
| Uncertain cross-root output or unproven redirected output | No Cargo clean; do not promote to PrivateBounded |
| Purely read-only scan with partial traversal | Report positives and typed uncertainty; negative state pruning prohibited according to C029 |
| Metadata resolution never reached | Report stage 'not attempted', not a misleading 0-success/0-failure claim |
| Invalid explicit root | Existing strict fatal behavior; no automatic narrowing |

**Any proposal to allow Execute in the second or third row is blocked until ADR 004 is explicitly accepted and the particular partition satisfies an independence certificate.** A diagnostic dedup alone cannot change those rows.

## 5. Verification and closure

Tests must exercise realistic Linux non-root permissions, synthetic platform-independent errors, Windows/macOS equivalents, multiple selected roots, redirected build/target and Cargo environment changes, hard links/symlinks as relevant, C003/C004/C006 negative controls, present/absent and reappearing learned roots, exact JSON/log/exit semantics, and actual end-to-end recovered bytes (reported only after successful Cargo clean). Assert no direct filesystem deletion. Add a test that fails if the cleanup engine returns pre-resolution zero workspaces when the selected scope is otherwise clean and fully valid.

Run repository's fmt/clippy/test, fixture-portability and doc-citation checker self-test/check ladder, `git diff --check`, and complete hosted Linux/macOS/Windows/Rust 1.89 verification. Record real command outputs, toolchain version, permissions/UID premise, branch SHA, and run links. Update `architecture/09`, `13`, `14`, USAGE/TROUBLESHOOTING/AUTOMATION/README, Unreleased CHANGELOG, and machine-contract docs only when those semantics actually change.

Closure path: `plans/closure/artifact-discovery-cleanup/c031-status.md`. Mark conditional research as completed but **do not close the destructive correctness claim** until either (a) the unchanged C006 safety model is retained and the Routine happy path passes at current scope, with documented residual blocks, or (b) a new accepted ADR plus independently qualified destructive proof passes all negative and published-artifact tests.

## 6. Stop conditions and handoff

Stop destructive work for any unresolved ADR status, missing proof of cross-root independence, a negative test that can pass from an unrelated failure, uncertain Cargo/environment output identities, incomplete pre-spawn refresh, machine-contract ambiguity, or any observed unexpected deletion. A clean-looking output is not proof. Never weaken `Shared`/`Uncertain`/`ExternalUnproven` or skip an unresolved manifest merely to make the observed command exit 0.

**Safe track completed in implementation commits `e58825b` and its preceding
C029/C030 commits:** cleanup now resolves discovered manifests and constructs
workspace/unit counts before returning an incomplete-discovery block. A
premise-checked subprocess test proves those counts are measured, metadata was
attempted, and `cargo clean` was not spawned. The complete healthy Routine
execute/simulate path remains qualified by the existing `bare_invocation_*`
subprocess cases. No JSON v1 shape or exit-code contract changed. The
independent-scope track remains blocked as recorded above.
