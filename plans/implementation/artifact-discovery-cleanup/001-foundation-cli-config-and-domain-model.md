# Artifact Discovery and Cleanup Milestone 001 — Foundation, CLI, Config, and Domain Model

Status: closed

Repository baseline: 97dee9fa64868534cedf3a9310e7160335a91be2

Source roadmap:

- plans/subsystems/artifact-discovery-cleanup-roadmap.md#6-milestone-m001--foundation-cli-config-and-domain-contracts
- plans/002-long-term-roadmap.md#phase-1--foundation-cli-configuration-and-domain-contracts

Canonical requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/003-planning-process.md

Primary class: infrastructure / invariant

## 1. Objective

Create the minimal Rust production foundation for cargo-cleanme: package/toolchain metadata, CLI dispatch, typed configuration, canonical scan-policy resolution, domain records, error/diagnostic types, and baseline CI/tests.

This milestone MUST NOT implement real recursive machine scanning or any destructive cleanup behavior.

## 2. Why this milestone is ready

There are no hard implementation dependencies.

The repository is planning-only at the baseline. Product semantics, terminology, roadmap ordering, and the read-only boundary are already specified.

Current dependency research is sufficient to establish the foundation without deciding M002 traversal mechanics inside CLI/config code.

## 3. Current implementation evidence

At the repository baseline:

- README.md exists;
- canonical planning documents exist;
- there is no Cargo.toml;
- there is no src/ tree;
- there is no config parser;
- there is no CI;
- there is no deletion/cleanup code.

The implementation therefore has no legacy API or migration burden.

## 4. Invariants that must not regress

- Default operation is read-only scan intent.
- Default recency_seconds is exactly 300.
- scan_start_time and cutoff belong to scan execution, not configuration parsing.
- CLI root > configured scan.root > global platform scope.
- If an explicit scope is effective, user ignore/unignore discovery filters are inactive.
- User discovery filters do not redefine project-activity semantics.
- No cleanup/delete command, recursive deletion primitive, or cargo clean process invocation is introduced.
- Path handling uses Path/PathBuf/OsString semantics rather than assuming UTF-8.
- Malformed config is actionable and not silently replaced with defaults.
- Core domain types do not depend on clap, terminal formatting, or dua-core.

## 5. Scope

### In scope

- Cargo package scaffold using Rust 2024 edition.
- Initial rust-version of 1.89 unless a direct dependency proves incompatible.
- Binary name cargo-cleanme.
- Library crate for testable domain/config/scan orchestration boundaries.
- clap-based CLI.
- serde + toml configuration.
- platform configuration directory resolution, preferably through a small wrapper over the directories crate.
- repository-shipped config.example.toml.
- config path/init/show or equivalent bounded config inspection surface.
- typed path/scope/filter configuration.
- typed domain records for later M002/M003 consumers.
- top-level application error classification.
- formatting, linting, unit-test CI on Linux/macOS/Windows.
- README updates for current implemented surface.

### Explicitly out of scope

- dua-core integration;
- real recursive discovery;
- artifact sizing;
- mtime activity walking;
- JSON report stability;
- cargo metadata invocation;
- Cargo target-dir/build-dir discovery;
- cargo clean;
- deletion;
- confirmation prompts/TUI;
- release packaging.

## 6. Required production changes

### 6.1 Package and module structure

Prefer a small layout resembling:

~~~text
Cargo.toml
config.example.toml
src/
  lib.rs
  main.rs
  cli.rs
  config.rs
  domain.rs
  policy.rs
  error.rs
~~~

Exact filenames may change, but dependency direction MUST remain:

~~~text
domain <- policy/config
   ^         ^
   |         |
 scanner   CLI/app
~~~

The domain layer must not import clap or the later filesystem traversal engine.

### 6.2 CLI contract

The installed binary is cargo-cleanme, enabling both:

~~~text
cargo-cleanme
cargo cleanme
~~~

Required initial commands:

~~~text
cargo-cleanme
cargo-cleanme scan
cargo-cleanme scan /explicit/root
cargo-cleanme config path
cargo-cleanme config init
cargo-cleanme config show
~~~

No arguments MUST normalize to scan intent.

The M001 scan handler MAY terminate with a clearly classified "scanner not implemented in M001" internal path in focused tests, but the handoff should prefer a clean orchestration seam that M002 can fill without rewriting CLI parsing.

Support a global --config PATH override so tests and operators can avoid platform config directories.

Do not add a clean subcommand.

### 6.3 Configuration contract

Initial serde model:

~~~toml
[scan]
recency_seconds = 300

# root = "/path/to/projects"

ignore = [
  "/path/to/ignore/*",
]

unignore = [
  "/path/to/ignore/except-this-one",
]
~~~

The config file is optional. Absence means defaults.

The repository ships config.example.toml with commented root and explanatory precedence.

The platform config file is named config.toml under the cargo-cleanme application config directory.

M001 should use a dedicated ConfigPathResolver boundary so tests can supply temporary locations.

### 6.4 Path validation

Normalize only what is necessary for policy comparison.

Required rules:

- root must resolve to an absolute path;
- ignore patterns must be absolute-pattern expressions;
- unignore paths must resolve to absolute paths;
- ~ expansion MAY be supported if implemented deterministically;
- environment-variable interpolation is optional and MUST NOT be half-implemented;
- path existence is not required while deserializing config, because removable paths can legitimately be absent;
- CLI explicit root existence/type validation belongs to scan-policy resolution.

Do not require UTF-8 paths on Unix.

### 6.5 Effective scan policy

Introduce a typed policy object sufficient for later milestones, conceptually:

~~~text
EffectiveScanPolicy
  recency: Duration
  scope: Explicit(PathBuf) | Global
  discovery_filters: Active(...) | Bypassed
~~~

Rules:

1. a CLI positional root wins over configured scan.root;
2. configured scan.root wins over global scanning;
3. any explicit effective root bypasses ignore/unignore search filters;
4. global scope carries compiled-or-uncompiled filter input for M002;
5. recency defaults to 300 seconds and is independent of scope.

M001 does not need to compile globs; it must freeze the precedence contract.

### 6.6 Domain records

Define internal types sufficient for later milestones without over-modeling:

- ScanRequest / EffectiveScanPolicy;
- DiscoveredProject { project_root, manifest_path, target_path };
- ActivityState or ActivityAnalysis;
- ArtifactAnalysis { target_path, bytes, metric, newest_mtime, artifact_entries };
- EligibleProject;
- ScanDiagnostic;
- ScanReport.

Prefer newtypes/enums where they prevent unit or state confusion.

Do not expose a CleanupCandidate production type in this milestone.

### 6.7 Errors and diagnostics

Separate fatal application/config errors from non-fatal scan diagnostics.

The CLI should be able to return conventional non-zero exit status for invalid configuration or invalid explicit root.

No scan warning should be represented only as preformatted terminal text in the domain layer.

### 6.8 Dependency constraints

Keep dependencies modest.

Expected candidates:

- clap;
- serde;
- toml;
- directories;
- thiserror or equivalent small error helper.

Do not add async runtimes.

Do not add dua-cli itself. M002 may add dua-core directly.

## 7. Ordered work packages

### Work package A — Rust package and quality baseline

Intent:

Establish a boring, portable crate before feature work.

Required changes:

- Cargo.toml and lockfile;
- Rust 2024 + rust-version;
- lib/bin split;
- rustfmt/clippy policy;
- .gitignore;
- GitHub Actions matrix for Linux/macOS/Windows;
- unit-test command.

Acceptance evidence:

- cargo fmt --check;
- cargo clippy --all-targets --all-features -- -D warnings;
- cargo test --all-targets --all-features;
- CI workflow parses and exercises the same broad checks.

### Work package B — CLI dispatch

Intent:

Freeze command names and no-argument semantics.

Required changes:

- clap parser;
- default-to-scan dispatch;
- positional explicit root;
- --config override;
- config path/init/show.

Acceptance evidence:

- parser tests for direct/default/scan/config cases;
- no clean/delete command exists.

### Work package C — Configuration and path resolution

Intent:

Make config behavior deterministic before filesystem discovery begins.

Required changes:

- defaults;
- optional file load;
- config init template;
- absolute path validation;
- explicit-root precedence;
- filter bypass semantics.

Acceptance evidence:

- absent config;
- valid config;
- malformed TOML;
- invalid path forms;
- CLI root vs config root;
- configured root vs ignore/unignore;
- config init does not overwrite an existing file without an explicit safe policy.

### Work package D — Domain and diagnostics

Intent:

Give M002/M003 a stable non-CLI model.

Required changes:

- core records/enums;
- byte-size metric enum;
- diagnostic severity/category;
- report record;
- no traversal dependency.

Acceptance evidence:

- compile-time/domain tests;
- CLI formatting not embedded in records.

### Work package E — Documentation

Intent:

Make implemented behavior match planning.

Required changes:

- README usage/config section;
- config.example.toml;
- module-level comments only where ownership is non-obvious.

## 8. Failure, cancellation, restart, and contention semantics

M001 performs no long-running concurrent work.

Config init MUST handle pre-existing files safely. It should fail without overwriting unless the CLI explicitly grows a future force option.

Interrupted writes to newly created config should use create-new/write semantics or atomic replacement where practical; do not truncate an existing config accidentally.

## 9. Compatibility and migration

There is no previous runtime configuration.

The V0.1 config schema begins at versionless simple TOML. Do not add a schema-version field unless implementation evidence requires one.

Unknown fields SHOULD fail clearly during the initial release rather than silently doing nothing, unless serde flatten/forward-compatibility is intentionally documented.

## 10. Required tests

### Focused unit tests

- CLI no-arg => scan;
- cargo-cleanme scan path parsing;
- config default recency = 300;
- config root precedence;
- filter bypass when explicit root is effective;
- invalid recency/path inputs;
- config path resolver;
- config init non-overwrite;
- non-UTF-8 PathBuf domain behavior on Unix where feasible.

### Integration tests

- invoke built binary help/version;
- invoke config path with temporary override;
- load temporary config and inspect effective policy.

### Security/negative tests

- malformed TOML;
- relative configured root;
- relative unignore path;
- impossible explicit root;
- existing config not overwritten.

## 11. Required verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
~~~

Hosted CI should run the broad set on Linux, macOS, and Windows, with reasonable duplication reduction allowed after the first qualification.

## 12. Documentation updates

- README.md
- config.example.toml
- plans/registry.md only through closure/status reconciliation, not ad-hoc implementation edits
- closure record after implementation

## 13. Acceptance criteria

M001 is complete when:

- the repository is a compiling Rust package;
- cargo cleanme is a valid installed-subcommand shape;
- no arguments resolve to read-only scan intent;
- config.toml defaults and precedence are represented by typed tested policy;
- the 300-second default is test-covered;
- config root bypasses ignore/unignore filters in effective policy;
- domain types exist without traversal/CLI leakage;
- Linux/macOS/Windows CI is green;
- there is no destructive command or deletion implementation.

## 14. Stop conditions

Stop and report rather than improvising if:

- the chosen config-directory dependency forces a materially different user-visible path policy;
- Rust 1.89 cannot satisfy a required dependency;
- supporting cargo cleanme would require Cargo-specific argument behavior not captured here;
- path-filter semantics cannot be represented without changing canonical terminology;
- implementation begins to require recursive scanning.

## 15. Closure evidence required

The closure record must include:

- implementation commit/PR;
- Cargo.toml dependency/MSRV summary;
- CLI parser evidence;
- config precedence test evidence;
- config init safety evidence;
- Linux/macOS/Windows CI results;
- grep/static evidence that no clean/delete command/path was introduced;
- known deviations, if any.

## 16. Handoff notes

Keep this milestone deliberately boring. M001 should make M002 easy to implement without converting config/CLI files into a filesystem engine.
