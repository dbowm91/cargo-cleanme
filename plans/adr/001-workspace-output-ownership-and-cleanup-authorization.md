# ADR 001 — Workspace Output Ownership and Cleanup Authorization

Status: accepted

Date: 2026-10-02

Repository baseline: `50461442c0d4d783a5a4f1e86a35001f68f8d8db`

Scope: M005 redirected/shared Cargo output awareness

## Context

M004 safely cleans only a one-member Cargo workspace whose output is the conventional direct `target/` directory. Cargo supports substantially broader output configurations:

- a workspace normally owns one shared target directory for all members;
- `build.target-dir` / `CARGO_TARGET_DIR` can redirect final output;
- Cargo 1.91+ supports a distinct `build.build-dir` / `CARGO_BUILD_BUILD_DIR` for intermediate output;
- target and build directories can be equal, nested, external to the workspace, or intentionally shared by multiple workspaces;
- Cargo configuration resolution depends on invocation context, environment, ancestor configuration, included configuration, and command-line overrides;
- cargo-cleanme supports Rust/Cargo 1.89+, so runtime Cargo capability cannot be assumed from cargo-cleanme's compile-time MSRV.

A path returned by Cargo is evidence of where a workspace writes output. It is not, by itself, proof that the entire physical directory is exclusively owned by that workspace.

M005 must also remain safe when scanning a large machine quickly. It must not size output trees, run Cargo metadata, or render terminal updates more often than necessary.

## Decision

### 1. Workspace is the logical ownership unit

M005 groups discovered manifests into Cargo workspaces before output ownership is decided.

A resolved workspace record contains at least:

- canonical workspace root and root manifest;
- workspace member manifests/source roots;
- Cargo runtime capability information;
- resolved target directory;
- resolved build directory when Cargo reports one;
- the Cargo invocation/configuration context used for resolution.

Multi-member workspaces are normal M005 inputs and are not rejected merely for containing more than one package.

### 2. Cargo is authoritative for workspace and output resolution

cargo-cleanme MUST NOT reimplement Cargo's configuration merge rules.

The resolver uses Cargo itself, preferring:

1. `cargo locate-project --workspace --manifest-path ...` to collapse member manifests to a workspace identity without dependency resolution;
2. one cached `cargo metadata --offline --locked --no-deps --format-version 1` resolution per unique workspace/configuration context.

Cargo subprocess failures are conservative diagnostics. A workspace whose output configuration cannot be resolved reliably is not destructively eligible.

### 3. Runtime capabilities are detected

The binary remains Rust 1.89 compatible.

M005 records enough Cargo runtime capability to distinguish at least:

- pre-1.91 stable behavior where a separate stable build directory is unavailable;
- Cargo 1.91+ behavior where `build-dir` is stable;
- unknown/nightly cases where metadata fields or environment configuration cannot be interpreted safely.

Absence of `build_directory` is never blindly treated as proof that a configured nightly build directory does not exist.

### 4. Target and build directories are separate logical outputs

A workspace owns an `OutputSet` containing:

- one target output root;
- one build output root, which may equal target;
- output-root kind and resolved path identity.

Cargo internal build-directory layout is opaque. cargo-cleanme MUST NOT parse fingerprints, hashed filenames, `deps/`, incremental directories, or other Cargo-private layout to infer ownership.

### 5. Physical output is grouped before sizing or cleanup

Resolved existing output roots are canonicalized at the output-root boundary, not per filesystem entry.

Equal or ancestor/descendant output roots are grouped into a `PhysicalOutputGroup`. Connected overlapping groups are measured once as a physical union so:

- target == build is not double-counted;
- nested target/build paths are not double-counted;
- two workspaces resolving to the same/overlapping output region are recognized as shared.

Symlink output roots are inventory diagnostics and are not destructively eligible.

### 6. Ownership/authorization is classified, not boolean

Each physical group has one of these safety classes:

- `PrivateBounded` — exactly one resolved workspace owns the group and every destructive output path is contained by an authorized cleanup boundary.
- `ExternalUnproven` — only one workspace was observed, but some output lies outside authorized cleanup boundaries.
- `Shared` — multiple resolved workspaces own equal/overlapping physical output.
- `Uncertain` — path identity, Cargo resolution, activity, or ownership could not be proven.

Conventional workspace-local output is normally `PrivateBounded`.

A scan finding only one user of an external cache does NOT prove exclusive ownership.

### 7. Shared and externally unproven output is inventory-only

M005 may discover, measure, deduplicate, and report `Shared` and `ExternalUnproven` output.

It MUST NOT invoke Cargo clean against those groups.

Selective package/workspace cleanup of shared caches remains a later policy milestone unless new Cargo evidence provides a stronger ownership primitive.

### 8. External cleanup requires explicit authorization

The explicit `clean ROOT` source sandbox authorizes private output physically contained within that root.

Private redirected output outside that source sandbox requires an explicit configured cleanup-output authorization root before destructive cleanup can occur.

Authorization roots are exact absolute directory roots, not glob patterns, and do not turn a shared/uncertain group into a private group.

### 9. Activity is workspace/output-group wide

A workspace is protected by recent source activity in any workspace member.

A physical output group is protected by recent activity anywhere in that group.

When a shared physical group is recent, all owners are protected.

### 10. Cleanup remains Cargo-mediated and revalidated

For an authorized private group, execution:

1. re-resolves the workspace and output set;
2. repeats source/output activity checks;
3. compares physical output identities and ownership classification;
4. freezes the resolved output environment/arguments passed to the Cargo subprocess where Cargo capabilities allow it;
5. invokes Cargo rather than recursively deleting output;
6. measures the physical output union after success.

Cargo-cleanme never deletes Cargo output trees directly.

### 11. Progress rendering is presentation only

Traversal/resolution emits structured progress events through an internal observer boundary.

Interactive rendering is not part of eligibility or ownership logic.

The default attended-terminal renderer is an inline, main-screen status display on stderr. It never enters raw mode or the alternate screen and clears itself before the deterministic final report.

Non-terminal/CI output contains no progress control sequences.

### 12. `--dryrun` is an application simulation mode

M005 distinguishes:

- Cargo preview: current default / explicit `--dry-run`, which may invoke `cargo clean --dry-run --verbose`;
- simulation: `--dryrun`, which runs cargo-cleanme discovery, Cargo resolution/metadata, filtering, sizing, authorization, final revalidation, progress UI, and final reporting but MUST NOT invoke any `cargo clean` command;
- execution: `--yes`, which invokes real Cargo cleanup after the same gates.

Simulation exists for output/performance testing and is not labeled recovered space.

## Consequences

Positive:

- Cargo remains the source of truth for its own configuration semantics.
- Multi-member workspaces and redirected output become representable.
- Shared caches can be reported without pretending cargo-cleanme can prove per-workspace byte ownership.
- Cargo internal cache layout can evolve without cargo-cleanme parsing it.
- Destructive authority remains narrower than discovery authority.
- M005 can support Cargo 1.89-1.90 while taking advantage of newer runtime Cargo fields.
- Progress/status work cannot alter safety decisions.

Costs:

- Cargo subprocess resolution is required for manifests that survive cheap discovery filters.
- Output identity/grouping adds a domain layer before analysis.
- External private cleanup needs a user-visible authorization concept.
- Shared caches remain inventory-only in M005.
- Runtime capability fixtures must cover more than one Cargo generation.

## Rejected alternatives

### Parse Cargo configuration ourselves

Rejected because configuration search/merge rules, environment overrides, included configs, relative path resolution, and workspace invocation context are Cargo behavior and evolve independently.

### Treat one observed workspace as exclusive ownership

Rejected because a machine scan is not proof that an external directory has no other users.

### Parse Cargo build-cache internals to assign bytes to packages

Rejected because Cargo explicitly treats the build-directory layout as an implementation detail.

### Delete redirected directories directly

Rejected because it would bypass Cargo locking/cleanup semantics and weaken M004's established safety boundary.

### Full-screen ratatui/crossterm UI for progress

Rejected for this milestone. The requirement is transient status, not an interactive application. A lightweight inline renderer has lower dependency, terminal-state, and performance risk.


## C003 clarification — cleanup atomicity across the complete OutputSet

Clarification date: 2026-10-02.

ADR 001 already establishes the workspace as the logical ownership unit and requires cleanup to re-resolve the workspace and OutputSet. C003 makes the destructive consequence explicit:

- PhysicalOutputGroup remains the unit for physical deduplication, activity, and ownership classification.
- One Cargo clean invocation is authorized as a workspace-level CleanupUnit covering the complete resolved OutputSet that the invocation can affect.
- Every physical group touched by that OutputSet must independently satisfy the destructive ownership and authorization boundary before Cargo clean may run.
- A PrivateBounded target group cannot authorize a sibling build group that is ExternalUnproven, Shared, Uncertain, or unauthorized.
- Final revalidation must rebuild ownership against the complete bounded workspace set discovered under clean ROOT, not only the current workspace, so another discovered workspace moving into overlap is detected.
- One Cargo invocation produces one cleanup result and one deduplicated pre/post union measurement.

This clarifies the original decision; it does not authorize new external/shared cleanup behavior.


## C004 clarification — complete discovered-manifest coverage

Clarification date: 2026-10-03.

The bounded cleanup ownership universe is complete only when every Cargo.toml discovered under clean ROOT is accounted for by authoritative Cargo workspace resolution.

A discovered manifest is accounted for when it either resolves directly or is listed as a member/root by a successfully resolved workspace's Cargo metadata. A failed direct attempt may therefore be cleared by later authoritative metadata coverage in the same pass.

Any discovered manifest that remains uncovered is an unresolved ownership participant. Because cargo-cleanme delegates Cargo configuration semantics to Cargo, it cannot prove that participant does not share or overlap another workspace's output.

Therefore Preview, Simulate, and Execute fail closed for the entire cleanup scope while unresolved ownership participants remain. No cargo clean command, including cargo clean --dry-run, may run in that state.

Read-only scan remains partial-result tolerant; this completeness requirement is specific to destructive ownership qualification.
