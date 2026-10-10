# C032 — Cargo Workspace Resolution Diagnostics and Failure-Noise Corrective

Status: **active** — bounded stderr cause extraction and human grouping are implemented; a real-Cargo subprocess regression now covers 1,000 repeated failures, bounded human output, exact JSON diagnostics and observed locate counts, alongside a 1,000-failure unresolved-ledger test. The broader Cargo failure taxonomy, hosted qualification, and closure remain outstanding.
Baseline: `7c874ab7998bdac28a6236ec21ae59ec24b915f8` (`main`, v0.2.5, 2026-10-09).
Handoff branch: `plans/c032-c034-routine-cleanup-reliability`.
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: M005A, M006E, C004, C029, C030, C031 and their already closed records; do not rewrite historical closure claims.
Dependency: none. Interface dependency for C034's real-world end-to-end gate.
Read: `.skills/planning-and-closure`, `.skills/discovery-and-ownership`, `.skills/test-evidence`, `.skills/json-and-exit-contract`, and `.skills/cleanup-safety`.

## Incident and source-level diagnosis

User's Full `cargo cleanme scan` produces numerous near-identical warnings such as:

```text
cargo locate-project failed: failed searching for potential workspace
```

The actual spelling of a reported message may be shortened by the CLI or Cargo version. Confirm from a captured command before treating the literal text as a parser key. Full manifest discovery legitimately encounters dependency snapshots, disconnected projects, invalid manifests, workspace-inheritance declarations without parents, broken workspace membership, and transient worktrees. It must inventory what it can without pretending such manifests are valid Cargo workspaces.

`src/workspace.rs::resolve_workspaces_with_coverage` runs `cargo locate-project --workspace --manifest-path <manifest>` from the manifest's parent for each manifest not already covered by successful metadata. On failure, `cargo_failure_reason` retains only the **first nonempty stderr line** (currently bounded to 200 bytes), discarding error-chain details that could name the offending manifest/parent or remediation. It emits a separate `CandidateUncertain` diagnostic for each failure. `src/main.rs::run_scan` prints only the first ten of all scan diagnostics plus a count, typically burying the diversity and source paths of Cargo failures. Cargo failures are distinct from C029 traversal-coverage errors: a successful walk does not imply that every Cargo workspace resolves, while a failed workspace lookup is not proof that directory traversal was partial.

C030 deliberately retains unresolved manifests in Full read-only inventory without promoting them to automatic learned maintenance roots; C031 correctly counts unresolved ownership participants and blocks destructive cleanup under accepted C004/C006. Do **not** remove manifests or reinterpret a Cargo error as resolved simply to reduce log volume.

**Why verification missed this:** existing resolver tests assert an expected single failure string and stage; C029 bounded aggregation covered `dua_core` traversal errors, **not per-manifest Cargo subprocess failures**. Subprocess CLI tests exercise few fixtures with handcrafted runner stderr, not hundreds of heterogeneous malformed/partial Cargo manifests. Previously green tests therefore never asserted that users can identify the reason for a repeated workspace-search warning.

## Objective and invariant constraints

Deliver useful, bounded, deterministic diagnostics from real Cargo failures while preserving the manifest universe and an auditable per-manifest unresolved participant ledger. Distinguish workspace lookup failures, process spawn failures, Cargo metadata failures, parse failures, toolchain/env problems, and known Cargo workspace-structure failures. Do not parse Cargo configuration independently or infer membership/ownership from path ancestry. Never retry with weaker flags or omit `--workspace` to manufacture successful workspace identities without an explicit separate contract and proof.

Keep `scan` read-only and partial-result tolerant; maintain Full *traversal coverage* semantics independent of *Cargo resolution coverage*. Cleanup may use presentation summaries but must retain every unresolved participant for C004/C006 fail-closed checks. The current JSON v1, one-object stdout, `--format log` single <=384-byte ASCII line without paths, `--stats` stderr, Rust MSRV 1.89, and exit taxonomy remain binding. A machine schema change needs explicit approval, not an accidental `reason_code` rename.

## Ordered work packages

### A. Establish a real failure taxonomy and premise-negative reproduction

- Build a hermetic filesystem matrix: valid standalone crate, valid multi-member workspace, workspace member with `[package] workspace` inheritance, orphaned member after its root was removed, member absent from `workspace.members`, invalid TOML, detached nested Cargo project under a parent workspace, valid vendor snapshot (not necessarily maintained), and inaccessible Cargo executable/metadata where feasible.
- Invoke the *actual* Cargo toolchain from each manifest's parent with exactly `locate-project --workspace --manifest-path`; capture exit code, stderr **multiple lines**, and effective `cwd`, Cargo/Rust version and relevant environment, without recording secrets. Pin the error shape at the real Cargo version and separately with an injected CargoRunner. A warning's generic first line must be reproduced against baseline v0.2.5 and the useful next line must be shown.
- Measure counts, subprocess volume, elapsed time, report size, unique failure causes, and whether a complete scan still reconciles Full state appropriately. Do not conclude all user failures are orphaned worktrees solely from one generic message.

### B. Preserve an actionable bounded Cargo error chain

- Replace one-line-only `cargo_failure_reason` with a bounded causal excerpt retaining the *most relevant* Cargo error context: first line plus a small number of causally informative `Caused by:`, `error:`, path and workspace-membership lines. Prefer a structured failure object (`stage`, `manifest`, `exit_status`, normalized diagnostic class, bounded human sample) while retaining raw diagnostic causality to the extent allowed by existing DTOs.
- Bound diagnostic line count and UTF-8 byte length before storage/rendering, redact no user-provided local path needed for troubleshooting but do not leak Cargo stderr into `--format log`; sanitize control sequences/newlines and never allow terminal injection. Preserve deterministic ordering; do not accidentally split a multi-line cause into hundreds of report diagnostics.
- Treat truly unknown errors as unknown, not speculative classifications inferred from substring matching. Preserve original manifest identity, failure stage and reproducible exact argv under `--stats` or a documented diagnostics flag if one exists; do not add a new CLI option without updating completions/man pages and contract tests.

### C. Bounded deduplicated human reporting, complete internal coverage

- Aggregate presentation by normalized *error class/cause and stage*, with count, at most a small fixed number of representative manifest paths, and a reproducible means to inspect all failures through structured read-only output or bounded explicit scans. Never merge unlike errors simply because their first line matches; never retain unbounded raw Cargo stderr for repeated errors.
- Preserve the complete `ResolutionCoverage.unresolved` list and every `UnresolvedOwnershipParticipant` for cleanup. Summary compression must not change the set of manifests, workspaces, output ownership, or block reason.
- If deduplicating diagnostics in `ScanReport` changes JSON `diagnostics` counts, `cargo_failures`, exit or log semantics, first define and test the compatibility behavior. Prefer a separate *presentation* summary derived from exact typed failure records; keep existing output schema stable until explicitly versioned.
- Ensure progress reports a bounded, meaningful Cargo failure total and separates global traversal errors from workspace-resolution problems.

### D. Docs and operator-facing outcome

Update `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `architecture/07-workspace.md`, `architecture/13-orchestration.md`, `architecture/14-testing-and-verification.md`, applicable README and Unreleased CHANGELOG. Explain why Full can discover Cargo.toml with unresolved workspace, how to reproduce exact Cargo errors on one manifest, why no learned root is created from unresolved manifests, and why cleanup remains blocked on unresolved ownership. Re-derive source citations, not shifted old line numbers.

## Required discriminating tests

1. Real Cargo fixture whose stderr first line is generic but second/third line distinguishes an orphaned workspace: after the fix the bounded warning includes meaningful cause (not simply the string `failed`). Include Cargo version constraints rather than assuming English text across all toolchains.
2. Two distinct Cargo failures sharing the same first line are not silently merged into the same cause bucket; 1,000 similar failures emit bounded human summaries without losing per-manifest unresolved coverage.
3. Valid sibling workspace is resolved and read-only inventoried even with unrelated broken manifest; count actual `cargo_locate_calls`, `cargo_metadata_calls`, `cargo_failures` accurately.
4. Simulate/Preview/Execute remain scope-blocked, zero `cargo clean` spawns when a discovered manifest is unresolved, regardless of fewer displayed warnings. Unknown or malformed subprocess failures also fail closed.
5. Full traversal complete while some Cargo resolutions fail has explicitly tested positive-state and missing-resolution semantics; C029 traversal partiality and C030 learning from resolved workspaces only remain intact.
6. UTF-8/control sequence sanitation, bounded stderr, deterministic non-TTY output, JSON v1 and <=384-byte log invariants, Linux/macOS/Windows, Rust 1.89.

## Verification, stop conditions and closure

Follow the actual repository ladder: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, fixture-portability checker self-test/check, doc-citation checker self-test/check, `git diff --check`, and the full hosted OS/MSRV matrix. Record elapsed time and process count on a synthetic 1,000-failure case; do not measure host-wide Full scans in CI. Prove new tests fail on v0.2.5 for the intended reason.

**Stop** if failure presentation is used to drop an unresolved owner, if a Cargo invocation loses its real manifest/cwd context, if output log path privacy is breached, or if an accepted machine contract changes without a decision. Any broader authoritative Cargo resolution fallback is out of scope until separately proved.

Acceptance: usable root-cause diagnostics for the reported Cargo workspace failures with deterministic bounded human output; complete unresolved proof ledger preserved; no change to cleanup eligibility. Closure evidence: `plans/closure/artifact-discovery-cleanup/c032-status.md`. Plan does not authorize a release.
