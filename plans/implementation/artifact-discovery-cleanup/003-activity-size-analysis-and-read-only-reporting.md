# Artifact Discovery and Cleanup Milestone 003 — Activity, Size Analysis, and Read-Only Reporting

Status: conditionally closed

Repository baseline: 97dee9fa64868534cedf3a9310e7160335a91be2

Source roadmap:

- plans/subsystems/artifact-discovery-cleanup-roadmap.md#8-milestone-m003--activity-size-analysis-and-read-only-reporting
- plans/002-long-term-roadmap.md#phase-3--activity-analysis-sizing-and-v01-reporting

Canonical requirements:

- plans/000-long-term-specification.md#5-canonical-v01-scan-pipeline
- plans/000-long-term-specification.md#7-activity-semantics
- plans/000-long-term-specification.md#12-size-semantics
- plans/000-long-term-specification.md#13-output-contract
- plans/000-long-term-specification.md#14-error-and-partial-result-semantics
- plans/001-terminology-and-domain-model.md

Hard dependency:

- M002 closed.

Primary class: capability / invariant

## 1. Objective

Complete the first useful product slice: analyze discovered conventional Cargo targets, exclude projects with source or build activity inside the configured recency window, measure artifact size, and print a deterministic read-only reclaimable-space report.

M003 is the V0.1 capability closure boundary. It MUST still contain no cleanup/delete behavior.

## 2. Why this milestone is blocked

M003 consumes M002's candidate discovery, traversal boundary, scope semantics, diagnostics, and platform behavior.

Implementing activity/sizing before M002 closes would duplicate traversal logic and make safety semantics difficult to prove.

## 3. Current implementation evidence

At plan-writing baseline there is no Rust implementation.

Expected M003 starting point after M002:

- typed EffectiveScanPolicy;
- one fixed scan scope;
- DiscoveredProject records;
- dua-core-backed traversal abstraction;
- ignore/unignore discovery policy already resolved;
- no size/activity report yet.

Current external evidence:

- dua-core 4.1 stream_roots can reuse one fixed worker pool for newly submitted candidate roots;
- filesize 0.2 can measure platform-specific allocated size, with metadata-fast calculation on Unix and GetCompressedFileSizeW on Windows.

References:

- https://docs.rs/dua-core/4.1.0/dua_core/
- https://docs.rs/filesize/latest/filesize/

## 4. Invariants that must not regress

- scan_start_time is captured exactly once per scan.
- cutoff = scan_start_time - recency_seconds.
- an mtime >= cutoff makes the project active.
- a future mtime makes the project active.
- source activity and target activity both protect the project.
- user discovery ignore rules do not suppress activity evidence after a project is discovered.
- .git metadata and the candidate target are excluded from source activity.
- independent nested repository boundaries are excluded from the parent source-activity walk.
- recent source activity should short-circuit before expensive target sizing.
- an inability to establish trustworthy inactivity/size disqualifies the candidate rather than promoting it.
- reported bytes are target artifact bytes only.
- final ordering is deterministic regardless of traversal parallelism.
- no cargo clean, remove_dir_all, trash, or equivalent destructive path exists.

## 5. Scope

### In scope

- scan clock/cutoff;
- source activity gate;
- nested repository boundary behavior;
- target artifact-presence check;
- target newest-mtime calculation;
- allocated/on-disk size aggregation;
- size metric/fallback classification;
- candidate eligibility policy;
- deterministic human renderer;
- summary totals/diagnostic summary;
- optional JSON output if the domain records already make it low-risk;
- end-to-end fixtures;
- representative performance evidence.

### Explicitly out of scope

- destructive cleanup;
- dry-run cleanup;
- target-dir/build-dir config resolution;
- shared target ownership;
- selective package/profile cleanup;
- persistent cache/index;
- filesystem watcher;
- scheduling/background service;
- exact "bytes reclaimable" promise.

## 6. Required production changes

### 6.1 Scan clock

At application scan start:

~~~text
scan_start = SystemTime::now()
cutoff = scan_start - recency_seconds
~~~

Handle subtraction failure for absurd durations as a fatal policy/config error.

Pass the resulting immutable ScanClock/Cutoff through analysis. Do not repeatedly compute now - recency during a long scan.

Tests should inject a clock/time value rather than sleeping.

### 6.2 Source activity gate

For each DiscoveredProject:

- walk the project root;
- exclude the candidate target;
- exclude .git and other approved VCS metadata;
- do not enter independent nested repository boundaries;
- inspect directory/file mtimes as needed to catch content edits plus create/delete/rename activity;
- stop as soon as an entry mtime is at or newer than cutoff;
- treat mtime > scan_start as active.

The broad global discovery ignore/unignore rules MUST NOT be applied inside this activity gate.

If required metadata cannot be read and the missing evidence could hide recent activity, classify the candidate as uncertain/ineligible and emit a diagnostic.

### 6.3 Nested repository detection

The source gate must not let activity in an independent nested repository protect the parent indefinitely.

A pragmatic V0.1 boundary is a nested directory that contains its own VCS metadata marker (especially .git) below the candidate project root.

Do not run Git.

Tests must cover a parent Cargo project containing an independently versioned nested Cargo/non-Cargo repository.

### 6.4 Target analysis

Only candidates that survive source activity enter target analysis.

For each target:

- verify it is still a real non-symlink directory;
- traverse without following symlinks;
- count artifact entries;
- compute newest trustworthy target mtime;
- aggregate allocated/on-disk size;
- classify target as recent if any mtime >= cutoff or > scan_start;
- produce a candidate-disqualifying diagnostic on errors that make size/activity untrustworthy.

An empty target does not become eligible.

### 6.5 Worker-pool reuse

Prefer dua-core stream_roots or equivalent bounded multi-root analysis so many target directories do not each allocate their own worker pool.

The implementation must keep per-root aggregation separate and produce a completion event/result per target.

Cancellation/early exit must not leak worker threads.

### 6.6 Size calculation

Primary metric: allocated/on-disk bytes.

Use traversal metadata when possible.

On Unix, filesize can use metadata block counts efficiently.

On Windows, the platform call may require the path. Account for this in performance tests.

If a platform cannot provide allocated bytes, fall back to apparent size only through an explicit SizeMetric enum and document it.

V0.1 does not promise hard-link-deduplicated physical reclaim. If hard links are counted using path-entry allocation semantics, document that the number is an inventory estimate and never label it recovered bytes.

### 6.7 Eligibility

A project is eligible only if all are true:

- source analysis is inactive;
- target exists and is artifact-bearing;
- target analysis is inactive;
- size is trustworthy;
- no disqualifying uncertainty occurred.

Conceptually:

~~~text
last_activity = max(source_newest, target_newest)
eligible = last_activity < cutoff
~~~

but the implementation should preserve early exits and uncertainty states rather than forcing missing times into sentinel epochs.

### 6.8 Human report

Default output:

- one eligible project per line;
- human-readable binary units;
- project root path;
- size descending;
- path lexical/OS-stable tie-breaker;
- final count and total.

Example:

~~~text
18.42 GiB  /home/me/projects/old-router
 7.81 GiB  /home/me/src/experiment

26.23 GiB reclaimable across 2 inactive Cargo projects
~~~

The word reclaimable is an estimate in read-only inventory context. Do not print recovered.

Paths that are not valid UTF-8 must still have a lossless-or-clearly-escaped output strategy on platforms that permit them.

### 6.9 Diagnostics/output separation

Normal stdout should remain script-friendly enough that diagnostics do not interleave unpredictably.

Prefer:

- eligible report on stdout;
- warnings/errors summary on stderr;
- --verbose for individual skipped/error paths.

If JSON output is added, emit structured diagnostics in the JSON model or stderr according to one documented contract. Do not add JSON if doing so materially delays read-only closure.

### 6.10 Exit status

Recommended initial behavior:

- 0: scan completed, including zero eligible projects and ordinary non-fatal skips;
- non-zero: configuration/scope failure or fatal scan failure that prevents meaningful report production.

Candidate-specific analysis errors do not necessarily make the whole scan fatal.

Document any threshold that promotes excessive/global IO failure to fatal status.

## 7. Ordered work packages

### Work package A — Scan clock and activity types

Intent:

Make time semantics deterministic and testable.

Required changes:

- injected/fixed scan clock;
- cutoff calculation;
- Active / Inactive / Uncertain states;
- future-time handling.

Acceptance evidence:

- boundary equality;
- one nanosecond/second before and after cutoff as supported;
- future mtime;
- huge recency validation.

### Work package B — Source activity gate

Intent:

Protect active development cheaply.

Required changes:

- source traversal;
- target/VCS/nested-repo pruning;
- early recent exit;
- uncertainty diagnostics.

Acceptance evidence:

- recent source edit prevents target sizing;
- old source proceeds;
- nested independent repo activity excluded from parent;
- user discovery ignores do not hide source activity.

### Work package C — Target analysis and sizing

Intent:

Produce trustworthy artifact facts.

Required changes:

- artifact presence;
- newest target mtime;
- size aggregation;
- metric classification;
- bounded multi-root worker reuse.

Acceptance evidence:

- empty target rejected;
- recent target rejected;
- inactive target sized;
- symlink entries not followed;
- analysis errors disqualify candidate.

### Work package D — Eligibility and rendering

Intent:

Turn facts into the user-facing V0.1 product.

Required changes:

- policy;
- deterministic sort;
- human formatter;
- total;
- stderr diagnostics summary;
- optional verbose mode.

Acceptance evidence:

- golden/snapshot tests where stable;
- equal-size deterministic ordering;
- zero-result output;
- non-UTF-8 path case where feasible.

### Work package E — End-to-end qualification

Intent:

Prove the complete read-only pipeline.

Required changes:

- multi-project fixture;
- platform CI;
- large-tree measurement;
- no-destructive static guard.

Acceptance evidence:

- recent source project absent;
- recent build project absent;
- old project present with correct size metric;
- filters/root scope from M002 still behave;
- no clean/delete symbols/commands in production surface.

## 8. Failure, cancellation, restart, and contention semantics

### Concurrent mutation

Files may change during analysis. Because V0.1 is read-only, a race that creates evidence of recent activity should bias toward exclusion.

If a target changes enough that size/activity analysis becomes inconsistent or errors, mark it uncertain rather than eligible.

### Cancellation

A cancelled scan stops accepting/processing new candidates and tears down traversal workers. It emits no claim that partial totals are a complete machine inventory unless an explicit future partial-report mode is designed.

### Restart

There is no persistent scan state. Restart begins a new scan with a new scan_start/cutoff.

### Contention

Do not lock Cargo projects or target directories during read-only scan. The later cleanup milestone owns revalidation and process contention semantics.

## 9. Compatibility and migration

No persistent data migration.

Human output should not be treated as a stable parser API. If JSON is added, document its stability level explicitly.

The config schema remains from M001/M002.

## 10. Required tests

### Time/activity unit tests

- exact cutoff active;
- just older than cutoff inactive;
- future mtime active;
- source newest vs target newest;
- uncertainty dominates eligibility.

### Source fixture tests

- edit src/main.rs recently;
- create/delete/rename represented by directory mtime;
- old source tree;
- nested .git boundary;
- target excluded;
- discovery ignore does not hide activity.

### Target fixture tests

- empty target;
- one old file;
- recent file;
- future file;
- nested directories;
- symlink inside target;
- target itself replaced by symlink between discovery/analysis;
- vanished file during walk.

### Size tests

- known regular-file allocation/apparent behavior with platform-aware assertions;
- sparse file where reliable;
- many files;
- overflow uses checked accumulation;
- platform fallback classification.

### End-to-end tests

- several projects with mixed activity;
- root/ignore/unignore from M002;
- stable sort;
- total;
- zero results;
- diagnostic summary;
- explicit root;
- permission failure where supported.

### Performance evidence

- source-recent fixture proves no target-size traversal;
- many target roots share bounded workers;
- large wide/deep target release-build scan recorded in closure;
- memory does not require full machine-tree retention.

## 11. Required verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test activity
cargo test sizing
cargo test report
cargo test end_to_end
~~~

If implementation introduces benchmarks/examples, record the actual release-mode command and result in closure.

## 12. Documentation updates

- README current V0.1 usage;
- config.example.toml;
- documented size metric;
- documented active-project semantics;
- documented platform/global-root semantics;
- explicit statement that cleanup is not yet implemented;
- closure record;
- registry reconciliation.

## 13. Acceptance criteria

M003 is complete when:

- cargo-cleanme / cargo cleanme can execute the full read-only scan;
- the same fixed cutoff is used for the full invocation;
- recent source activity excludes a project;
- recent target activity excludes a project;
- future timestamps exclude a project;
- inactive artifact-bearing conventional targets are reported;
- target sizes are measured using the documented disk-size metric;
- candidate uncertainty never promotes eligibility;
- results are deterministic and size-descending;
- ordinary system traversal errors are bounded diagnostics;
- Linux/macOS/Windows CI passes;
- representative large-tree performance evidence exists;
- no destructive command, Cargo cleanup invocation, or recursive deletion implementation exists.

## 14. Stop conditions

Stop and report if:

- the source-activity definition cannot be implemented without scanning user-excluded generated trees in a way that makes normal use impractical;
- nested-repository detection requires Git subprocesses to be correct;
- allocated-size measurement is prohibitively slow on a supported platform and changing the canonical metric is required;
- dua-core multi-root analysis cannot preserve per-target identity/cancellation safely;
- M002 closure has unresolved scope/filter correctness defects;
- any requested "cleanup while here" work appears.

## 15. Closure evidence required

The closure record must include:

- implementation commit/PR;
- fixed-cutoff tests;
- source and target recent-activity tests;
- future-mtime test;
- empty-target/artifact-presence test;
- size-metric evidence per platform;
- deterministic output evidence;
- bounded diagnostics evidence;
- proof recent-source short-circuit avoided target traversal;
- worker-bound evidence;
- representative large-tree performance measurement;
- Linux/macOS/Windows CI;
- static/CLI evidence that no destructive capability exists.

## 16. Handoff notes

This is the milestone to exercise on real developer machines before writing M004. Any false-positive "inactive" classification is a release-blocking safety defect because the next workstream will eventually rely on this eligibility logic as one input to destructive revalidation.
