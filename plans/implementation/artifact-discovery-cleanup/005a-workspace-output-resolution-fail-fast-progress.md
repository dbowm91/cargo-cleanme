# M005A — Workspace Output Resolution, Fail-Fast Scan, and Inline Progress

Status: ready

Repository baseline: `50461442c0d4d783a5a4f1e86a35001f68f8d8db`

Source roadmap milestone: M005 redirected/shared Cargo output awareness

Accepted decision:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`

Hard dependencies:

- M004 closed;
- C001 closed;
- ADR 001 accepted.

Primary class: capability / performance / UX foundation

## 1. Objective

Replace the M004 assumption `project -> direct target/` with a workspace-aware read-only output model that can discover redirected target/build output, deduplicate shared/overlapping physical output, reject work as early as safely possible, and show immediate low-overhead scan status on attended terminals.

M005A ends at inventory/reporting. It MUST NOT broaden destructive cleanup beyond M004.

## 2. Current implementation evidence

At the baseline:

- discovery only emits a candidate when a `Cargo.toml` has a direct real `target/`;
- cleanup ownership parses `cargo metadata` but rejects multi-member workspaces and redirected/separate output;
- `src/traverse.rs` owns bounded dua-core traversal and batched target sizing;
- no progress renderer exists;
- final scan output is deterministic and written after the scan;
- stable Linux/macOS/Windows and Rust 1.89 CI are green.

These are strong primitives, but discovery must become manifest/workspace-first before redirected output can be represented.

## 3. Invariants

- No-argument scan and `scan` remain read-only.
- Cargo configuration precedence is resolved by Cargo, not cargo-cleanme.
- Symlink directories are never followed.
- Filesystem/Cargo uncertainty never promotes eligibility.
- No output tree is deeply sized before cheaper exclusion gates have run.
- Shared/equal/nested physical output is never double-counted.
- Recent source activity in any workspace member protects the workspace.
- Recent activity anywhere in one physical output group protects that group.
- Final report ordering remains deterministic and independent of progress-event timing.
- Progress rendering never changes scan results.
- Progress output is transient stderr only; final report remains stdout.
- Non-TTY output receives no cursor/control-sequence noise.
- Rust 1.89 remains the compile-time MSRV.
- M004 cleanup behavior remains available for its existing conventional-private case.

## 4. In scope

- manifest-first discovery;
- Cargo workspace resolution;
- runtime Cargo capability model;
- target/build output resolution;
- multi-member workspace representation;
- physical output grouping and deduplicated sizing;
- shared/external/unproven classification for reporting;
- fail-fast exclusion and instrumentation;
- structured scan-progress observer events;
- inline progress/status renderer;
- final read-only report updates;
- documentation and qualification.

## 5. Out of scope

- cleaning redirected external output;
- cleaning shared output;
- explicit external cleanup authorization;
- `--dryrun` simulation mode;
- selective `cargo clean -p`;
- parsing Cargo's private build-cache layout;
- full-screen/raw-mode TUI;
- persistent scan index/database;
- background filesystem watcher.

Those destructive/simulation items are M005B.

## 6. Required production changes

### 6.1 Decouple manifest discovery from local artifact discovery

Broad traversal MUST discover plausible user `Cargo.toml` manifests without requiring a sibling `target/`.

Preserve/prioritize cheap prune decisions before manifest handling:

- user ignore/unignore policy;
- platform pseudo-filesystem prunes;
- symlink refusal;
- VCS metadata;
- conventional `target/` subtrees;
- Cargo registry/git source/cache trees under the effective Cargo home when their path is known;
- already identified nested independent-repository boundaries where current semantics require it.

Do not invoke Cargo for ignored/pruned paths.

Do not add an unconditional per-directory `CACHEDIR.TAG` read if benchmark evidence shows it costs more than it saves. A cache-marker prune MAY be added only with measured justification.

### 6.2 Add Cargo workspace resolver/cache

Introduce a small Cargo-owned adapter, separate from cleanup execution, with test-injectable process I/O.

For each manifest that survives discovery:

1. resolve workspace root using `cargo locate-project --workspace --manifest-path ...`;
2. canonicalize/cache the resulting root manifest;
3. do not repeat metadata resolution for additional members mapping to the same workspace/configuration context;
4. resolve the workspace using `cargo metadata --offline --locked --no-deps --format-version 1`.

Cargo failure creates a bounded diagnostic and no eligible output.

The resolver MUST NOT manually merge `.cargo/config.toml`.

### 6.3 Add runtime Cargo capability model

Record Cargo runtime capabilities separately from Rust MSRV.

At minimum model whether the running Cargo can reliably report/use a distinct build directory.

Tests need fixtures for:

- Cargo 1.89/1.90-style metadata without `build_directory`;
- Cargo 1.91+ metadata with equal target/build;
- Cargo 1.91+ metadata with distinct build;
- malformed/unknown metadata;
- environment evidence of a separate build directory when metadata cannot prove it.

Unknown capability means conservative inventory/cleanup classification.

### 6.4 Introduce workspace/output domain records

Add domain types equivalent in responsibility to:

- `WorkspaceId`;
- `ResolvedWorkspace`;
- `WorkspaceMember`;
- `OutputRoot { kind, logical_path, physical_path? }`;
- `OutputSet`;
- `PhysicalOutputGroup`;
- `OutputOwnershipClass`.

Do not expose serde/Cargo-specific JSON types from the domain layer.

### 6.5 Build physical-output groups

After workspace resolution:

- canonicalize existing output roots at the root boundary;
- reject symlink roots from destructive eligibility;
- identify equal and ancestor/descendant overlaps;
- form connected physical groups;
- associate all workspace owners;
- classify `PrivateBounded`, `ExternalUnproven`, `Shared`, or `Uncertain` per ADR 001.

In M005A, `PrivateBounded` is informational only; no new destructive path is added.

### 6.6 Fail fast before expensive analysis

Required gate ordering for resolved workspaces:

1. path/config prune during discovery;
2. workspace deduplication;
3. Cargo resolution;
4. cheap output-root existence/type check;
5. cheap artifact-presence check using bounded directory inspection;
6. source activity analysis excluding every resolved output root;
7. deep output activity/allocated-size analysis only for survivors.

If target/build output is absent or contains no artifact entries, do not scan the source tree or deeply size output.

If source activity is recent, do not deeply size output.

If an output group is already known uncertain, do not perform expensive work solely to make it look eligible.

Instrumentation MUST count how many workspaces exit at each gate.

### 6.7 Workspace source activity

Source activity is evaluated across all workspace member roots from Cargo metadata, including an explicitly declared member outside the normal workspace-root subtree.

Avoid duplicate source traversal when member roots overlap.

Exclude:

- all resolved output roots contained in a scanned member tree;
- VCS metadata;
- independent nested-repository boundaries per existing semantics.

Any recent/future member activity protects the workspace.

### 6.8 Deduplicated output analysis

Extend `src/traverse.rs` or a nearby output-analysis adapter to measure a physical group once.

For equal/nested logical roots:

- count physical files once;
- compute one allocated-byte total;
- compute one newest trustworthy output timestamp;
- propagate uncertainty to the group.

Do not parse Cargo-private subdirectory names.

### 6.9 Add structured progress events

Add a narrow observer interface so hot-path/domain code emits state, not terminal formatting.

Events/counters should cover at least:

- phase start/change;
- directories visited;
- directories pruned;
- manifests found;
- unique workspaces resolved;
- Cargo resolution failures;
- empty/no-output workspaces skipped;
- active workspaces skipped;
- output groups measured;
- bytes measured;
- reportable groups discovered.

Avoid one heap allocation/string format per visited filesystem entry. Directory counters SHOULD be aggregated/batched.

The no-op observer must be effectively free and be used by tests/non-interactive calls.

### 6.10 Add the inline progress renderer

Prefer `indicatif 0.18.6` or a later version that still supports Rust 1.89; 0.18.6 declares Rust 1.85 and therefore fits the current MSRV.

Do not add ratatui or raw-mode terminal handling.

Interactive behavior:

- initialize immediately before scan traversal so the user sees status at once;
- render on stderr;
- discovery uses an indeterminate bar because computing a total would require a wasteful pre-scan;
- once the number of unique workspaces/output groups is known, analysis may switch to determinate progress;
- show counters such as visited/pruned/manifests/workspaces/eligible;
- show at most five reportable directories/output groups beneath the main bar, including formatted sizes;
- prefer the largest reportable groups found so far rather than an unbounded scrolling log;
- cap refresh to at most 10 Hz;
- never redraw once per filesystem entry;
- call clear/finish-and-clear before final stdout report;
- restore a clean terminal on normal error/cancellation paths.

Add `--no-progress` to force the no-op renderer for benchmarks/debugging.

If stderr is not an attended terminal or `TERM=dumb`, progress is hidden automatically.

### 6.11 Preserve deterministic final reporting

After progress clears:

- print every eligible physical output group, not only the five transient UI rows;
- show its measured size and a useful workspace/output path identity;
- show whether a group is private, shared, external-unproven, or uncertain when that affects actionability;
- print one total without double-counting overlapping output;
- keep deterministic size-descending ordering with a stable path tie-break.

M005A's total is an inventory estimate, never recovered bytes.

## 7. Ordered work packages

### A — Domain/ADR plumbing

Add workspace/output/capability/ownership types and fixture parsers.

### B — Manifest discovery and workspace resolver

Separate `Cargo.toml` discovery from direct-target discovery, add locate/metadata caching, and retain M004 compatibility adapters.

### C — Fail-fast analysis pipeline

Add cheap output-presence gates, workspace-wide source activity, and deduplicated physical-group sizing.

### D — Progress observer contract

Introduce no-op/test observers and counters without terminal dependencies in traversal/domain APIs.

### E — Inline renderer

Add indicatif-based attended-terminal UI, top-five rows, `--no-progress`, clear-before-report behavior.

### F — Report/qualification

Update deterministic report, performance evidence, docs, and closure.

## 8. Failure/cancellation/contention semantics

- Cargo resolution failure affects only that workspace unless process startup itself is globally unavailable.
- A disappearing output root becomes uncertain/skipped, not zero-sized eligible.
- Progress-render failure must degrade to hidden/no-op rendering rather than fail a scan.
- Ctrl-C/default process termination may leave the program without a final report; no filesystem mutation occurs in M005A.
- Worker count remains bounded independently of manifest/workspace count.
- Do not run an unbounded number of Cargo resolver processes concurrently. Start with sequential resolution or a small explicit bound and measure before increasing it.

## 9. Compatibility/configuration effects

- No existing user config key changes in M005A.
- `--no-progress` is additive.
- Existing conventional direct-target scans must remain semantically compatible.
- Existing library callers may receive richer domain/report records but must not be forced to depend on indicatif.
- Rust 1.89 CI remains mandatory.

## 10. Required tests

### Discovery/resolution

- manifest with no local target but redirected output;
- multi-member workspace maps to one workspace;
- repeated member manifests invoke metadata once;
- explicit `package.workspace` / out-of-tree member fixture;
- ignored subtree never invokes Cargo;
- Cargo-home registry/git sources are pruned;
- malformed locate/metadata output;
- offline/locked resolution failure isolation.

### Output graph

- target == build;
- distinct sibling target/build;
- build nested under target;
- target nested under build;
- two workspaces share identical target;
- two workspaces share identical build;
- cross-overlap between one workspace target and another build;
- missing output;
- empty output;
- symlink output;
- non-UTF-8 output where platform/Cargo permits observation.

### Activity/sizing

- recent member source skips deep sizing;
- recent shared output protects the group;
- equal/nested roots counted once;
- sizing error -> uncertain;
- conventional M003 fixtures remain equivalent.

### Fail-fast/performance

Instrument fixtures proving:

- ignored/pruned trees cause zero Cargo subprocesses;
- empty/missing output causes zero source walks and zero deep output walks;
- recent source causes zero deep output-size walks;
- N manifests in one workspace produce one metadata resolution;
- progress disabled vs enabled does not change semantic results;
- renderer refresh count is bounded independently of visited entry count.

### Progress UI

Use indicatif's in-memory/term-like testing support or a renderer abstraction:

- begins before first traversal event;
- discovery phase is indeterminate;
- analysis phase can be determinate;
- at most five candidate rows;
- sizes render;
- final clear occurs before report write;
- hidden/non-TTY renderer emits no terminal control output;
- `--no-progress` emits no progress output;
- renderer error falls back without failing scan.

## 11. Performance qualification

Record both semantic counters and wall time.

At minimum compare the post-C001 real developer tree with M005A using:

- progress enabled;
- `--no-progress`;
- explicit bounded project root;
- generic scan where feasible.

Record:

- directories visited/pruned;
- manifests found;
- unique workspaces;
- Cargo locate calls;
- Cargo metadata calls;
- empty-output skips;
- recent-source skips;
- groups deeply sized;
- measured bytes;
- elapsed wall time.

The inline renderer SHOULD add negligible wall-time overhead; a sustained regression over roughly 5% on the representative explicit-root scan requires investigation or justification before closure.

Do not add a pre-scan merely to make the progress bar determinate.

## 12. Verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
git diff --check
~~~

Hosted stable Linux/macOS/Windows plus the Rust 1.89 job must pass.

## 13. Documentation updates

Update:

- README scan/output/progress behavior;
- canonical terminology if implementation reveals a naming correction;
- subsystem roadmap;
- registry;
- M005A closure record.

Do not mark redirected cleanup supported after M005A.

## 14. Acceptance criteria

M005A closes only when:

- a project can be discovered even with no direct local target;
- Cargo itself resolves workspace/output configuration;
- multi-member workspaces are represented once;
- target/build physical overlap is deduplicated;
- shared output is recognized and inventory-only;
- fail-fast counters prove expensive work is skipped by earlier gates;
- generic attended-terminal scans display immediate inline status;
- the transient display is capped at five sized groups and clears before final output;
- non-TTY output remains deterministic/plain;
- final scan prints every reportable group plus a non-double-counted total;
- existing conventional scan semantics remain green;
- no new destructive cleanup path exists.

## 15. Stop conditions

Stop and report rather than improvise if:

- Cargo workspace/output resolution cannot be made stable across the supported runtime matrix without reproducing Cargo config logic;
- `cargo locate-project` invocation context changes output resolution relative to metadata in a way the resolver cannot preserve;
- physical overlap cannot be deduplicated without parsing Cargo-private cache internals;
- progress instrumentation measurably dominates traversal time;
- an implementation requires destructive handling of shared/external output to complete inventory;
- Rust 1.89 cannot compile the selected progress dependency.

## 16. Closure evidence

Create:

`plans/closure/artifact-discovery-cleanup/005a-status.md`

Include requirement-to-evidence mapping, hosted CI/MSRV runs, Cargo runtime fixtures, output-graph fixtures, fail-fast operation counters, progress renderer tests, representative before/after timing, and explicit confirmation that destructive support remains M004-bounded.
