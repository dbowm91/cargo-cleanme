# Artifact Discovery and Cleanup Corrective C001 — Public Contract, Planning, MSRV, and Traversal Reconciliation

Status: closed

Closure: `plans/closure/artifact-discovery-cleanup/c001-status.md`

Repository baseline: `811e7d0833cda3d9eec35b2c77bbc3deb17aa379`

Corrects:

- `plans/implementation/artifact-discovery-cleanup/001-foundation-cli-config-and-domain-model.md`
- `plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md`
- `plans/implementation/artifact-discovery-cleanup/003-activity-size-analysis-and-read-only-reporting.md`
- `plans/implementation/artifact-discovery-cleanup/004-revalidated-cleanup-execution.md`

Closure evidence reviewed:

- `plans/closure/artifact-discovery-cleanup/001-status.md`
- `plans/closure/artifact-discovery-cleanup/002-status.md`
- `plans/closure/artifact-discovery-cleanup/003-status.md`
- `plans/closure/artifact-discovery-cleanup/004-status.md`

Source roadmap:

- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Canonical requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/003-planning-process.md#7-corrective-passes`

Primary class: corrective / invariant / polish

## 1. Objective

Reconcile the implemented M001-M004 product with its public CLI/configuration contracts, declared Rust MSRV, traversal/performance architecture, README examples, and active planning state before redirected/shared Cargo-output work becomes implementation-ready.

This pass MUST preserve the safety boundary established by M004. It is not a vehicle for broadening cleanup ownership to shared workspaces, redirected `target-dir`, or separate `build-dir`.

## 2. Why this corrective pass is required

The M001-M004 implementation is functional and its current Linux/macOS/Windows CI matrix passes, but post-merge review found several closure gaps that were not represented in the original closure records.

### C001-F1 — Cargo external-subcommand invocation is advertised but not normalized

The README and M001 contract state that both of these forms are supported:

~~~text
cargo-cleanme ...
cargo cleanme ...
~~~

Current `src/main.rs` calls `Cli::parse()` directly. The production CLI does not normalize Cargo's external-subcommand argv form before clap parses it, and current tests exercise only `cargo-cleanme ...` argv.

The earlier M001 planning contract explicitly required direct/Cargo-external argv equivalence. The closure record did not include an integration test demonstrating the actual external-subcommand argv contract.

Severity: correctness / public CLI contract.

### C001-F2 — Configuration template has two sources and the shipped filename drifted

The canonical/M001 contract converged on a checked-in top-level `config.toml` used as the same source emitted by `config init`.

Current implementation instead has:

- checked-in `config.example.toml`;
- a separately maintained hard-coded `CONFIG_TEMPLATE` string in `src/config.rs`;
- no byte-for-byte synchronization guard;
- no `config init --force` path even though the tightened M001 handoff called for explicit overwrite behavior.

This is maintenance and public-contract drift.

Severity: maintainability / configuration correctness.

### C001-F3 — README cleanup example conflicts with the runtime safety contract

README currently documents:

~~~text
cargo-cleanme clean ./projects
~~~

but `cleanup::clean_with` rejects non-absolute roots.

The safety invariant that the cleanup subsystem ultimately operates on an absolute sandbox root is sound. The user-facing CLI should either resolve a supplied relative path to an absolute path before crossing the cleanup boundary, or documentation must require an absolute path. The preferred behavior is to accept a relative CLI path and absolutize it without weakening the internal invariant.

Severity: usability / documentation correctness.

### C001-F4 — Declared Rust 1.89 MSRV is not continuously qualified

`Cargo.toml` declares `rust-version = "1.89"`.

Current CI installs stable Rust only. The hosted cross-platform matrix proves portability on current stable but does not prove the declared MSRV.

Severity: maintenance / release-contract evidence.

### C001-F5 — Traversal architecture is split between dua-core discovery and sequential walkdir analysis

M002 discovery uses `dua-core`, but M003 activity/target analysis and M004 post-clean measurement use `walkdir`. Candidate analysis in `main.rs` is sequential.

This creates:

- two traversal implementations with duplicated no-follow/error semantics;
- separate behavior to maintain for filesystem edge cases;
- no reuse of dua-core's fixed worker-pool/multi-root machinery for target sizing, which is typically the most expensive scan phase;
- weaker evidence for the original performance direction that candidate targets share bounded traversal workers.

The recorded real-tree run (~28 candidates / ~138.85 GiB / ~19.94 s) is acceptable baseline evidence, so C001 MUST use measurement to justify changes rather than blindly replacing working code.

Severity: maintenance / performance / architecture drift.

### C001-F6 — Planning state is stale in the subsystem roadmap

The subsystem roadmap still has initial audit text claiming the repo contains only README/no Rust implementation, and its dependency graph labels M004 active even though M004 is closed.

Severity: planning/documentation debt.

## 3. Invariants that must not regress

- No-argument invocation and `scan` remain read-only.
- Mutation remains available only under the explicit `clean` command and only with `--yes`.
- Cleanup continues to require a bounded explicit sandbox scope internally.
- Every cleanup candidate is freshly activity-revalidated.
- Cargo ownership is checked before execution.
- Shared workspaces, redirected targets, and separate build directories remain rejected.
- Production cleanup delegates deletion to Cargo; cargo-cleanme does not recursively delete target trees itself.
- No filesystem symlink directory is followed during discovery/activity/target traversal.
- Filesystem uncertainty cannot promote a candidate to eligible.
- The 300-second default and root/filter precedence remain unchanged.
- Rust 1.89 remains the declared MSRV unless C001 proves a current direct dependency cannot support it; do not silently raise it.
- M005 redirected/shared-output implementation remains out of scope.

## 4. Scope

### In scope

- direct/Cargo-external argv normalization and integration tests;
- configuration template single-source reconciliation;
- explicit `config init --force` overwrite behavior;
- relative user cleanup-root normalization to an absolute sandbox root, or a documented stop if that cannot be made unambiguous;
- Rust 1.89 CI qualification;
- traversal architecture consolidation where measurement supports it;
- bounded parallel target/source analysis without changing eligibility semantics;
- README corrections;
- subsystem roadmap/current-state reconciliation;
- corrective closure record and registry update.

### Explicitly out of scope

- redirected `target-dir` cleanup;
- separate `build-dir` cleanup;
- multi-member/shared workspace cleanup;
- global Cargo cache cleanup;
- JSON output redesign;
- release packaging or crates.io publication;
- daemon/index/watcher architecture;
- general-purpose disk usage features;
- changes to the M004 destructive ownership model beyond preserving it during refactor.

## 5. Required production changes

### 5.1 Normalize Cargo external-subcommand argv before clap

Introduce one testable argv-normalization function at the CLI/app boundary.

Required semantics:

- `cargo-cleanme` => unchanged;
- `cargo-cleanme scan /x` => unchanged;
- Cargo external-subcommand argv containing exactly the injected `cleanme` token in the documented position => strip that token once before clap;
- user arguments containing a later literal `cleanme` path/value MUST NOT be stripped;
- direct invocation behavior must not become dependent on executable filename string matching alone.

Prefer parsing from an explicitly constructed iterator rather than hiding normalization in `main`.

Add tests for the exact argv vectors used by direct and Cargo-external invocation, including help and config/clean commands.

If practical in CI, add an integration test that installs/builds the binary into a temporary Cargo command path and runs `cargo cleanme --help` or equivalent without mutating user Cargo state.

### 5.2 Make the checked-in config template authoritative

Replace the split `config.example.toml` + hard-coded template model with one checked-in canonical template.

Preferred outcome:

~~~text
config.toml
src/config.rs -> const CONFIG_TEMPLATE: &str = include_str!("../config.toml");
~~~

The checked-in file is a template/distribution artifact; runtime config discovery still uses the platform application config directory and MUST NOT automatically load the repository/current-working-directory file.

If retaining the name `config.example.toml` is strongly preferred for packaging reasons, the implementation may do so only by reconciling the canonical planning docs in the same pass and proving that `config init` embeds that exact file. Do not maintain two independent copies.

Add a test that the bytes emitted by `config init` exactly match the checked-in template.

### 5.3 Implement explicit safe overwrite for config initialization

Add:

~~~text
cargo-cleanme config init --force
~~~

Default init continues to use create-new semantics and refuses overwrite.

`--force` MUST replace only the resolved cargo-cleanme config path. Prefer atomic same-directory replacement or a write-temp + rename strategy where supported. It MUST NOT truncate an existing config until replacement content is fully written.

Tests must cover:

- initial creation;
- refusal without force;
- successful force replacement;
- failure leaves the previous file intact where the platform permits deterministic fault injection.

### 5.4 Normalize user cleanup roots before the safety boundary

Keep `cleanup::clean_with`'s absolute-root invariant.

At the CLI/application boundary, resolve a relative `clean ROOT` argument against the invocation current directory into an absolute lexical path before cleanup.

Do not require canonicalization merely to make the path absolute, because canonicalization can erase useful path intent or fail before the explicit root is validated by policy. The cleanup/policy layer already verifies that the effective root exists, is a real directory, and is not a symlink.

Tests:

- `clean ./project` reaches cleanup with an absolute root;
- absolute root remains unchanged;
- nonexistent relative root fails as an invalid root;
- symlink root remains rejected after normalization;
- paths containing `..` cannot escape the path actually selected by the caller through surprising normalization semantics.

Update README examples to match implemented behavior.

### 5.5 Add real MSRV CI

Keep the existing Linux/macOS/Windows stable matrix.

Add one dedicated Rust 1.89 job, preferably Linux, that at minimum runs:

~~~bash
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
~~~

If a dependency activates features that cannot compile on 1.89, stop and identify the dependency/version before changing `rust-version`.

The closure record must link the successful hosted MSRV job.

### 5.6 Reconcile filesystem traversal ownership

Create one small cargo-cleanme-owned traversal abstraction that can support:

- discovery;
- source activity;
- target analysis/sizing;
- post-clean measurement.

Do not expose dua-core types from domain APIs.

The implementation SHOULD prefer one engine where it materially reduces duplication and improves bounded parallelism. Given the existing code, first evaluate moving M003/M004 walks to dua-core or a common adapter rather than keeping independent `WalkDir` behavior.

Required semantic preservation:

- symlinks never followed;
- source activity can short-circuit;
- target analysis records newest mtime, entry count, and allocated bytes;
- metadata/permission errors remain conservative;
- nested VCS source boundaries remain excluded;
- post-clean measurement uses the same size semantics as pre-clean measurement.

### 5.7 Bounded candidate analysis

If the traversal consolidation permits it, use one bounded worker pool/multi-root mechanism for target analysis rather than one serial full-tree traversal per project.

Do not parallelize destructive Cargo executions in C001. Cleanup execution remains sequential unless a later plan explicitly changes contention semantics.

Source activity may remain candidate-sequential if it preserves cheap early rejection and measurement shows target traversal dominates. The implementation must choose based on measured evidence, not aesthetics.

### 5.8 Remove dead/duplicated traversal dependency only when safe

If all `walkdir` production use disappears after consolidation:

- remove `walkdir` from direct dependencies;
- retain it only in dev-dependencies if fixture helpers still justify it.

If it remains because a bounded use is simpler/faster, document the ownership split and why it is not semantic duplication.

### 5.9 Reconcile documentation and planning state

Update:

- README to match direct/Cargo-external invocation, relative cleanup root behavior, config template/init force semantics, and actual safety limitations;
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md` current-state evidence and dependency graph;
- `plans/registry.md`;
- any M001-M004 closure limitation section only by additive corrective references; do not rewrite historical closure evidence as if C001 had existed then.

Historical closure records SHOULD link forward to the corrective closure once C001 completes, or C001 closure may link backward if the project convention prefers immutable historical records.

## 6. Ordered work packages

### Work package A — CLI/config public-contract repair

Intent:

Close the externally visible correctness gaps before internal optimization.

Required changes:

- Cargo argv normalization;
- Cargo-external invocation tests;
- canonical config template;
- `config init --force`;
- relative cleanup-root normalization;
- README examples.

Acceptance evidence:

- direct and Cargo-external forms produce equivalent parsed commands;
- config init output exactly matches repository template;
- overwrite behavior is explicit and safe;
- README commands execute as documented.

### Work package B — MSRV qualification

Intent:

Turn `rust-version = "1.89"` from metadata into a continuously tested contract.

Required changes:

- CI job/toolchain setup;
- locked check/test;
- dependency correction if needed.

Acceptance evidence:

- successful hosted Rust 1.89 job on the final corrective commit.

### Work package C — Traversal consolidation

Intent:

Reduce duplicate filesystem semantics and make expensive target analysis use bounded traversal resources.

Required changes:

- common traversal boundary;
- source/target/post-clean migration as justified;
- shared size accounting;
- remove redundant dependency if possible.

Acceptance evidence:

- existing eligibility and cleanup fixtures remain unchanged;
- no symlink/uncertainty regression;
- bounded worker evidence;
- before/after traversal measurements.

### Work package D — Performance qualification

Intent:

Verify that cleanup/refactor work is neutral or beneficial on the original use case.

Required evidence:

1. synthetic wide/deep tree operation counters;
2. candidate-count worker-bound test;
3. repeat the prior representative developer-tree scan, or another comparably documented tree;
4. record elapsed time, candidate count, and measured artifact bytes.

No hardware-independent timing threshold is required, but a material regression requires explanation before closure.

### Work package E — Planning/documentation reconciliation

Intent:

Leave the repository control plane truthful.

Required changes:

- update subsystem current-state section;
- update dependency graph with C001;
- registry C001 status;
- keep M005 design proposed/deferred for implementation;
- create C001 closure record when implementation/evidence is complete.

## 7. Failure, cancellation, restart, and contention semantics

### CLI normalization

Malformed or ambiguous argv must fail through clap rather than being guessed into a command.

### Config force replacement

A failed forced write must not intentionally destroy a previously valid configuration. File replacement must be scoped to the resolved cargo-cleanme config path.

### Traversal cancellation

Dropping/cancelling a scan must stop accepting work and join bounded worker resources. No partial scan may be presented as a complete inventory.

### Cleanup contention

C001 does not add parallel cleanup. Revalidation and Cargo ownership checks remain immediately before each sequential Cargo clean operation.

## 8. Compatibility and migration

No config schema change is intended.

The repository template filename is not the user's runtime config location. Renaming `config.example.toml` to `config.toml` has no user-file migration effect.

Accepting relative cleanup roots is backward-compatible: previously accepted absolute roots keep the same semantics.

Cargo-external argv normalization fixes an advertised invocation path; direct invocation must remain unchanged.

Traversal refactoring must not alter human output ordering, size metric, activity cutoff, or eligibility semantics.

## 9. Required tests

### CLI tests

- direct no-arg;
- direct scan/config/clean;
- Cargo-injected `cleanme` no-arg;
- Cargo-injected scan/config/clean;
- help/version forms;
- a literal later argument named `cleanme` is preserved;
- relative cleanup root conversion;
- absolute cleanup root stability.

### Config tests

- checked-in template parses;
- runtime defaults match template;
- init bytes == checked-in template;
- init refuses overwrite;
- force overwrite succeeds;
- malformed existing config can be deliberately replaced only with force.

### Traversal/regression tests

- all existing discovery fixtures;
- all existing activity fixtures;
- all target size fixtures;
- non-UTF-8 path behavior on Unix;
- symlink target/internal symlink behavior;
- nested VCS activity boundary;
- permission/disappearing-entry uncertainty;
- source recent short-circuit;
- post-clean size metric equals pre-clean metric semantics.

### Cleanup tests

- dry-run remains default;
- `--yes` remains the only execute opt-in;
- relative user root still becomes a bounded absolute internal scope;
- shared workspace / redirected target / build-dir rejection remains;
- no direct production deletion primitive appears.

### CI tests

- stable Linux/macOS/Windows;
- Rust 1.89 locked check/test.

## 10. Required verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
git diff --check
~~~

Also run focused tests for argv normalization, config init, traversal, activity, sizing, and cleanup ownership after their final test names exist.

Static source review must confirm that production cleanup still contains no direct recursive deletion implementation.

## 11. Acceptance criteria

C001 closes only when:

- the advertised `cargo cleanme` invocation path is proven equivalent to direct invocation;
- repository config template and `config init` have one source of truth;
- forced config replacement is explicit and safe;
- README cleanup examples work as written;
- relative cleanup roots become absolute before the internal cleanup safety boundary without weakening symlink/existence validation;
- Rust 1.89 passes a hosted locked build/test job;
- traversal ownership is reconciled and documented;
- expensive target analysis uses bounded resources and has before/after evidence;
- M004 safety/ownership tests remain green;
- Linux/macOS/Windows stable CI remains green;
- subsystem roadmap and registry match the implemented repository state;
- M005 implementation has not been pulled into the corrective pass.

## 12. Stop conditions

Stop and report rather than improvise if:

- Cargo's real external-subcommand argv contract differs materially from the planned normalization;
- making the config template single-source would require build-time generation or packaging machinery disproportionate to this repo;
- safe relative-root handling requires canonicalization that changes the M004 ownership/symlink model;
- any direct dependency fails Rust 1.89 and cannot be pinned/fixed without a material architecture change;
- moving analysis to the common traversal engine changes eligibility or error semantics;
- performance work suggests parallel Cargo cleanup;
- a fix requires supporting shared/redirected Cargo output—that belongs to M005.

## 13. Closure evidence required

Create:

`plans/closure/artifact-discovery-cleanup/c001-status.md`

It MUST include:

- implementation commit(s);
- defect-to-regression-test matrix for C001-F1 through C001-F6;
- direct vs Cargo-external invocation evidence;
- config template/init byte-equivalence evidence;
- config force-write safety evidence;
- relative-root CLI evidence;
- hosted Rust 1.89 run;
- final stable Linux/macOS/Windows run;
- traversal dependency/ownership summary;
- synthetic and real-tree before/after performance evidence;
- confirmation M004 safety tests remain green;
- static production deletion review;
- documentation/registry reconciliation;
- unresolved findings and disposition.

## 14. Handoff notes

Implement public-contract correctness before traversal optimization so performance work cannot obscure CLI/config regressions.

Do not reopen M004's deliberately narrow cleanup ownership. C001 should make the existing product more truthful, maintainable, and efficient; M005 remains the place to design broader Cargo output ownership.
