# M008C — Cargo Profile/Package Selective Cleanup Qualification

Status: closed by subsequent M008D evidence. Historical conditional closure: `plans/closure/artifact-discovery-cleanup/008c-status.md`; package-selector follow-up closure: `plans/closure/artifact-discovery-cleanup/008d-status.md`.

Repository baseline: `3ee9699a0b0d987287d08e427195284e08d079f7`

Source roadmap: Phase 9 — Selective cleanup and policy

Architecture dependencies:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
- C003/C004 complete ownership-universe proof
- C006 combined selected-root cleanup proof
- M008A workspace-level selective cleanup policy
- M008B stable machine-readable reporting/disposition contract

Primary class: Cargo-mediated mutation-footprint refinement / compatibility qualification

## 1. Objective

Research, qualify, and—only where Cargo behavior can be proven safe—add profile- and package-selective cleanup while preserving cargo-cleanme's ownership and revalidation guarantees.

Unlike M008A, this milestone changes what one Cargo invocation is asked to remove inside a workspace. Therefore it must not be implemented as a simple pre-filter over the existing full-workspace `CleanupUnit.bytes` estimate.

The milestone must establish a trustworthy selector-specific mutation and accounting contract before enabling Execute.

Candidate capabilities:

~~~text
cargo-cleanme clean ROOT --profile release ...
cargo-cleanme clean ROOT --package foo ...
cargo-cleanme clean --known --profile release ...
cargo-cleanme clean --known --package foo ...
~~~

Exact CLI shape may change based on qualification evidence.

## 2. Why this is separately gated

Current cargo-cleanme assumes one cleanup invocation can affect the complete resolved workspace `OutputSet`.

That assumption drives:

- `CleanupUnit` construction;
- ownership/authorization checks;
- pre-clean byte estimates;
- final physical covering-root proof;
- one result per workspace invocation;
- post-clean union measurement.

Cargo's selective clean flags narrow what Cargo removes inside those roots. The complete output roots still matter for ownership and safety, but the whole-root byte count is no longer a valid estimate of selector-specific reclaimable bytes.

Cargo's implementation and supported flags also vary by runtime version. The crate MSRV and the runtime Cargo capability must remain distinct concepts.

## 3. Research baseline to preserve

Known compatibility facts motivating the matrix:

- Cargo 1.89 exposes package selection and profile/release selection for `cargo clean`;
- Cargo 1.91 stabilizes `build.build-dir`, changing output-shape capability;
- later Cargo adds/changes workspace cleanup selection;
- package/profile/target cleanup behavior has received correctness fixes across recent Cargo releases;
- Cargo-private artifact-directory layout is not a stable API.

Implementation MUST re-verify these assumptions against the exact Cargo versions used for qualification. Do not encode this plan's research notes as runtime facts without tests.

## 4. Core safety invariant

Selector-aware cleanup may narrow Cargo's mutation request, but it MUST NOT narrow the ownership proof.

For a selected workspace:

1. discover and resolve the same complete selected-root manifest universe as existing cleanup;
2. build the complete physical output graph;
3. require the workspace's full affected `OutputSet` to satisfy ownership/path/authorization constraints required by the existing safety model;
4. revalidate the complete combined ownership universe immediately before disposition;
5. only then pass a qualified selector to Cargo.

An excluded package/profile is not an ownership exclusion.

If Cargo may affect output outside the proven covering union for the selector/runtime combination, Execute must remain disabled for that combination.

## 5. Qualification-first structure

M008C has two ordered stages.

### Stage 1 — Selector capability/behavior harness

Build a deterministic fixture harness that runs real Cargo versions against generated workspaces and records:

- command support;
- exit status;
- `--dry-run --verbose` output;
- pre/post filesystem state;
- which physical files/directories actually disappear;
- behavior with default target-dir;
- redirected target-dir;
- equal target/build output;
- distinct `build.build-dir` where supported;
- workspaces with multiple members;
- dependencies/shared build products;
- custom profiles;
- default dev/release profiles;
- configured/default build targets where relevant.

Qualification must cover at least:

- project MSRV Cargo 1.89;
- Cargo 1.91;
- the first stable version relied upon for any newer selector behavior;
- current stable Cargo at implementation time.

Add intermediate versions when research shows a semantic boundary or known correctness fix.

The harness can be a test/helper script or Rust integration fixture. It must not become production layout-parsing code.

### Stage 2 — Guarded production implementation

Only selector/runtime combinations whose mutation behavior and accounting can be bounded by Stage 1 may be enabled.

Unsupported or unqualified combinations must produce a typed unsupported/deferred disposition before any Cargo clean process is spawned.

## 6. Runtime Cargo capability model

Extend the existing Cargo capability model rather than checking version strings ad hoc at CLI parsing sites.

Possible capability concepts:

~~~text
CargoCleanCapabilities
PackageCleanCapability
ProfileCleanCapability
WorkspaceSelectorCapability
BuildDirCapability
DryRunDetailCapability
~~~

Prefer behavior/capability probing where reliable and inexpensive. If a version check is required, parse Cargo's version once per invocation and centralize the mapping with fixtures.

Do not conflate:

- rust-version/MSRV for compiling cargo-cleanme;
- runtime Cargo version executing metadata/clean;
- metadata schema capabilities.

Unknown versions should fail conservatively for selector Execute until the relevant behavior can be proven or safely treated as compatible.

Preview may be allowed for a broader set only if it cannot create a misleading “safe to execute” contract.

## 7. Package selector semantics

Package selection must use Cargo package identity, not filesystem path substring matching.

Requirements:

- resolve the target workspace through Cargo first;
- validate requested package spec/name against the resolved workspace;
- handle duplicate/ambiguous names according to Cargo's own accepted package-spec semantics;
- pass the selector through Cargo rather than mapping package names to target subdirectories;
- never delete package artifact directories directly;
- never infer dependency artifact ownership from filenames.

Workspace-level source activity remains protective. A future refinement may define per-package activity only with a separate proof model; it is out of scope here.

Because package builds share dependency/incremental artifacts, do not promise that package cleanup reclaims only bytes uniquely attributable to that package.

## 8. Profile selector semantics

Profile selection must pass Cargo's symbolic profile through Cargo.

Requirements:

- support only profiles Cargo accepts for the runtime;
- preserve `--release` as a Cargo alias only if exposed by cargo-cleanme intentionally;
- do not assume profile directory names;
- do not infer custom profile output paths;
- account for Cargo's evolving built-in profile set via runtime behavior rather than hardcoded filesystem names.

Profile policy is a mutation selector, distinct from M008A's workspace inactivity policy.

## 9. Selector-specific byte accounting

This is the hardest part of M008C.

The existing full-unit `bytes` is an upper bound on selector cleanup, not the selector estimate.

Preferred accounting hierarchy:

1. if Cargo's dry-run output for the qualified runtime can be parsed through a stable enough contract to enumerate deletion targets safely, use it only after proving the format/semantics across the matrix;
2. otherwise compute selector result as “unknown estimate” before Execute and rely on post-clean observed decrease, rather than inventing a byte number;
3. never parse Cargo-private artifact layout to manufacture package/profile byte attribution.

If selector-specific pre-clean bytes cannot be established robustly, machine/human reports must say so explicitly (`null`/unknown) while still reporting whole-output bytes separately only if clearly labeled as context, not “would clean.”

M008A minimum-size policy must not be applied to a selector using whole-workspace bytes as though they were selector bytes. Either:

- disable/minimize incompatible threshold combinations;
- evaluate minimum size against a qualified selector-specific estimate;
- or reject that combination with a typed unsupported-policy disposition.

This interaction must be decided and tested before implementation is marked ready for Execute.

## 10. Cargo dry-run use

`cargo clean --dry-run --verbose` is useful qualification evidence but is not ownership proof.

If production code consumes Cargo dry-run output:

- treat it as Cargo-provided mutation preview only;
- validate every reported path remains within the already-proven authorized output union;
- reject paths escaping the union;
- do not assume output text formatting is stable without a tested compatibility layer;
- do not use dry-run to replace final combined-universe revalidation.

If stable parsing cannot be justified across supported runtime versions, do not parse it in production.

## 11. Work package A — Real-Cargo qualification fixtures

Create generated fixture workspaces covering:

- single package;
- virtual workspace;
- multi-member workspace;
- two packages sharing dependencies;
- custom profile;
- dev/release;
- redirected `target-dir`;
- distinct build-dir on supported Cargo;
- package with build script/proc macro;
- multiple target triples where practical;
- no artifacts;
- partially populated artifacts.

For each qualified Cargo version, record pre/post trees or stable hashes/manifests of paths and compare selective cleanup behavior.

Keep fixture outputs bounded and temporary.

## 12. Work package B — Capability gating

Add runtime capability detection and typed unsupported results.

Required behavior:

- unsupported selector -> no Cargo clean spawn;
- unknown/unqualified runtime selector -> no Execute spawn;
- normal full-workspace cleanup remains available when its existing capability proof passes;
- selector support failure does not weaken ordinary cleanup behavior.

Expose capability failure through M008B stable reason codes.

## 13. Work package C — Selector-aware proof context

Extend `ExecutionProof` or introduce a selector-aware execution descriptor carrying:

- workspace identity;
- complete proven output union;
- selector kind/value;
- runtime Cargo capability evidence;
- frozen Cargo output environment;
- selector-specific estimate if trustworthy;
- policy compatibility result.

The selector is consumed only after the same final combined-universe ownership/activity/path proof as ordinary cleanup.

If selector-specific Cargo dry-run is required as an additional gate, it runs after safety proof and before Execute, and any discrepancy must fail closed.

## 14. Work package D — Command construction

Centralize selector-aware Cargo argument construction.

Required properties:

- `--manifest-path` and frozen output configuration remain explicit;
- package/profile arguments are passed through without shell interpretation;
- `--offline`/`--locked` behavior remains compatible with existing cleanup unless qualification shows a reason to adjust;
- Preview uses Cargo dry-run;
- Simulate invokes no Cargo clean command, including selector dry-run;
- Execute invokes exactly the qualified selective Cargo command.

Simulate must still evaluate every non-mutating proof/capability gate available without invoking Cargo clean.

If selector mutation semantics can only be determined by `cargo clean --dry-run`, Simulate cannot claim exact selector mutation parity; report that distinction explicitly or keep the feature blocked.

## 15. Work package E — Reporting and M008A policy interaction

Use M008B schema and typed codes.

Per-unit selector output must identify:

- selector kind/value;
- runtime support state;
- policy disposition;
- safety disposition;
- selector-specific pre-clean estimate or unknown;
- Execute post-clean measured union;
- observed decrease;
- Cargo failure.

Do not report full-output bytes as selector reclaimable bytes.

M008A include/exclude and age policy remain workspace-level and can compose naturally.

Minimum-size policy composes only when selector-specific size is trustworthy; otherwise reject/disable that combination as defined in the final contract.

## 16. Required tests

### Qualification harness

For every supported runtime band:

- package selector removes only Cargo's observed selector footprint;
- profile selector behavior matches observed fixture expectations;
- redirected output remains inside proven roots;
- distinct build-dir behavior is characterized;
- custom profiles work or are explicitly unsupported;
- dry-run/execute mutation sets agree to the degree Cargo promises.

### Safety regression

- shared output remains blocked even if selecting one package;
- excluded package does not remove another workspace from ownership proof;
- unresolved participant blocks selector Execute;
- output path mutation/race fixtures remain fail-closed;
- target/build redirect change before spawn skips;
- symlink/marker checks remain active.

### Accounting

- no whole-workspace byte value is mislabeled selector reclaimable;
- known selector estimate matches qualified dry-run/deletion set within defined metric semantics;
- unknown estimate serializes/renders as unknown;
- post-clean observed decrease remains measured across complete affected covering roots.

### Mode parity

- Simulate spawns zero Cargo clean commands;
- Preview invokes selector dry-run only after proof;
- Execute invokes qualified selector command;
- unsupported runtime spawns none.

### CLI

- package/profile conflicts and repeatability are explicit;
- malformed profile/package input rejected;
- composition with ROOT/known/full and M008A flags is covered.

### Platform/MSRV

- cargo-cleanme still compiles/tests on Rust 1.89;
- production unit/integration tests hosted on Linux/macOS/Windows;
- real-Cargo version matrix may run on Linux if installing all historical toolchains cross-platform is disproportionate, but platform-specific selector behavior must still receive current-Cargo coverage.

## 17. Verification commands

Baseline:

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
rustup run 1.89 cargo check --locked --all-targets
rustup run 1.89 cargo test --locked --all-targets
git diff --check
~~~

Qualification evidence must additionally record explicit real-Cargo commands/toolchain versions and fixture outcomes.

Do not close M008C from fake-runner tests alone.

## 18. Documentation updates

Document:

- runtime Cargo support matrix;
- distinction between cargo-cleanme MSRV and runtime Cargo selector capability;
- selector-specific accounting limitations;
- interaction with M008A minimum-size/age/include/exclude;
- unsupported/deferred reason codes;
- statement that cargo-cleanme delegates selective deletion to Cargo and does not parse Cargo artifact layout.

## 19. Acceptance criteria

M008C may close only when:

- a real-Cargo compatibility matrix exists and is checked into closure evidence;
- enabled package/profile selectors have bounded mutation behavior under the supported runtime capability model;
- ownership proof remains complete/full-workspace even when mutation is selective;
- selector-specific reclaimable bytes are either trustworthy or explicitly unknown;
- M008A minimum-size is never evaluated against a misleading whole-workspace estimate;
- Preview/Simulate/Execute semantics are explicit for selectors;
- unsupported/unknown combinations fail before Cargo mutation;
- C003/C004/C006 safety regressions remain green;
- M008B reason/schema contracts cover selector outcomes;
- hosted platform/MSRV gates pass.

## 20. Non-goals

- per-file direct deletion;
- parsing Cargo fingerprint/deps/incremental internals;
- dependency-graph garbage collection;
- Cargo global cache `clean gc`;
- per-package source-activity semantics;
- interactive package picker;
- target-triple/doc-only cleanup unless qualification shows they can be added without expanding the milestone materially;
- changing Cargo's own selector semantics.

## 21. Stop conditions

Do not enable selector Execute if any of the following remains true:

- Cargo may mutate outside the proven output union;
- selector behavior cannot be made deterministic enough for the supported runtime band;
- implementation requires parsing private artifact-directory layout;
- package/profile byte estimates would knowingly mislabel whole-workspace bytes;
- Simulate would need to run a mutating Cargo command;
- selector support would require weakening C003/C004/C006 ownership freshness;
- runtime capability detection cannot distinguish supported from unqualified behavior.

A valid closure may intentionally leave one selector (for example package selection) deferred while closing another only if the roadmap/registry is split or corrected so the unimplemented selector is not falsely marked complete.

## 22. Closure evidence

The closure record must include:

- exact Cargo versions/toolchains tested;
- fixture matrix and observed mutation sets;
- selector capability table;
- accounting decision and evidence;
- M008A policy-composition matrix;
- Preview/Simulate/Execute subprocess evidence;
- unsupported-runtime fail-closed fixtures;
- C003/C004/C006 regression results;
- hosted Linux/macOS/Windows CI;
- Rust 1.89 gates;
- any selector intentionally deferred with explicit follow-up disposition.
