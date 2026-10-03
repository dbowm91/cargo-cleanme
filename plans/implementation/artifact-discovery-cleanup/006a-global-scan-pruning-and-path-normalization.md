# M006A — Global Scan Pruning, Traversal Policy, and Path Normalization

Status: conditionally closed

Closure: `plans/closure/artifact-discovery-cleanup/006a-status.md`. The policy/traversal/path-normalization work is implemented, but the required reference-host no-argument scan exceeded 120 seconds; traversal-throughput follow-up M006C is proposed. M006A is not fully qualified closed.

Repository baseline: 47574fa8a087ac2f9111e29821c23b13657e7bbe

Source milestone: M006 performance hardening and routine machine-wide qualification

Primary class: performance / traversal / conservative semantic reconciliation

## 1. Objective

Make the no-argument global scan complete in a routine, bounded amount of time on representative developer machines without weakening discovery correctness, activity protection, Cargo ownership rules, or explicit-scope behavior.

M006A addresses three measured/current problems:

1. Unix global discovery begins at "/" while macOS currently hard-prunes only "/dev", causing traversal through large OS-managed trees that cannot contain ordinary user source projects.
2. Tool-managed Rust trees such as RUSTUP_HOME can contain many Cargo.toml files that are installed toolchain sources rather than user projects and should not trigger user-project Cargo resolution.
3. Workspace member source roots are not normalized to the same physical path identity used for output exclusions, so a symlinked path component on macOS can prevent target/build output from being pruned during source-activity traversal.

The milestone is read-only/performance focused. It must not weaken C002-C004 cleanup safety.

## 2. Baseline evidence

Recorded pre-M006 evidence:

- representative developer tree: 381,467 entries, 368 manifests, 96 workspaces, 32 reportable groups / 309.82 GiB, about 18.47 s;
- full no-argument macOS "/" scan exceeded 120 s;
- current Unix global root policy is one root: "/";
- current macOS system prune is only "/dev";
- Cargo registry/git source/cache trees are already pruned;
- C003 closure records the macOS source-root/output-exclusion identity mismatch as eligibility-neutral wasted traversal;
- traversal uses bounded dua-core workers, but discovery invokes a separate walk for each configured root.

M006A must record a fresh baseline on the same reference host before changing traversal policy.

## 3. External implementation evidence

- Apple documents "/System" as the system domain and protected OS content; modern macOS separates a read-only system volume from mutable data.
- Apple documents "/usr" as system-restricted except "/usr/local".
- Rustup documents RUSTUP_HOME, defaulting to "~/.rustup" (or the platform equivalent), as the root containing installed toolchains and rustup configuration.
- Cargo project/workspace discovery remains Cargo-authoritative after manifests survive filesystem pruning.

References:

- https://developer.apple.com/documentation/technologyoverviews/files-and-directories
- https://developer.apple.com/library/archive/documentation/Security/Conceptual/System_Integrity_Protection_Guide/FileSystemProtections/FileSystemProtections.html
- https://support.apple.com/guide/security/role-of-apple-file-system-seca6147599e/web
- https://rust-lang.github.io/rustup/devel/environment-variables.html

These references justify only narrow OS/tool-managed exclusions. They do not justify pruning arbitrary writable directories.

## 4. Invariants

- No-argument scan remains read-only.
- Explicit scan roots remain authoritative and are not silently narrowed by new global-performance exclusions.
- No directory symlink is followed.
- Target and VCS pruning remain unchanged.
- Cargo registry/git pruning remains unchanged.
- A new global prune is allowed only when the tree is OS/tool managed and cannot reasonably be a user project location under the product contract.
- Do not prune broad writable domains such as "/Users", "/Library", "/Applications", "/private", "/Volumes", "/opt", or arbitrary hidden directories merely because they are large.
- Do not treat absence of measurement as permission to prune.
- New pruning must never trigger more Cargo subprocesses than the baseline for the same discovered user-project set.
- Source-root normalization must preserve the activity contract: source traversal excludes resolved target/build output, but no non-output source path is dropped.
- Any change that makes a previously false-active workspace eligible because its own output is now correctly excluded must receive explicit cross-platform activity/cleanup qualification.
- C004 ownership coverage and C003 final CleanupUnit proof are unchanged.
- Rust 1.89 remains the MSRV.

## 5. Non-goals

- no daemon/index/database;
- no filesystem watcher;
- no user-project heuristic based on Git presence;
- no pruning of all caches or all dot-directories;
- no network/removable-volume policy redesign;
- no cleanup-proof optimization (M006B);
- no Cargo config parser;
- no change to recency semantics;
- no new destructive capability.

## 6. Work package A — Measurement and cost attribution

Extend the existing --stats instrumentation so global discovery can explain where traversal work went without logging every path.

Required counters should distinguish at least:

- platform-system prunes;
- Cargo-home prunes;
- rustup/toolchain prunes;
- target/VCS prunes;
- user-config ignore prunes;
- visited entries;
- discovered manifests;
- Cargo locate/metadata calls.

Add a benchmark/debug-only way to attribute first-level global traversal cost by root/top-level subtree. This may be a test/benchmark helper rather than stable CLI output.

Do not allocate a string/map entry per filesystem entry. Aggregate by a bounded top-level key.

Before implementing new prunes, capture a reference-host no-argument scan or a bounded timed profile showing which top-level trees dominate.

## 7. Work package B — Platform discovery policy boundary

Replace ad-hoc global_roots + system_prune branching with a typed internal policy, for example:

- GlobalDiscoveryPolicy
  - roots
  - global_only_prunes
  - managed_tool_prunes

Exact names are implementation choices.

The policy must be testable without traversing the real host.

### macOS initial safe candidates

Implement only exclusions backed by OS ownership/protection and measurements.

Expected first candidates:

- /System;
- /dev;
- /bin and /sbin where they are real directories rather than already skipped symlinks;
- /usr system content, while preserving /usr/local as a separately enumerable project-bearing root.

If "/usr" is pruned from the "/" traversal, "/usr/local" MUST be added as an explicit global root when present so developer-managed content remains discoverable.

Do not prune /Library, /Applications, /private, /Volumes, /opt, or user homes wholesale.

### Linux

Keep existing /proc, /sys, /dev, /run safety prunes. Add only measured tool-managed prunes unless separate evidence supports more.

### Windows

Preserve fixed/removable local-volume enumeration. M006A may add managed Rust-toolchain pruning but should not redesign drive selection.

## 8. Work package C — Rust tool-managed tree pruning

Add effective_rustup_home analogous to effective_cargo_home:

1. absolute RUSTUP_HOME if set;
2. otherwise platform home + ".rustup";
3. if identity is unavailable, do not guess.

For global discovery, prune the rustup-managed toolchain/configuration tree before manifest handling.

This prune is global-discovery policy, not a general "directory named .rustup" rule.

Tests must cover:

- default home;
- absolute RUSTUP_HOME;
- relative/invalid RUSTUP_HOME => no unsafe guessed prune;
- a Cargo.toml beneath the managed rustup tree never reaches Cargo resolution;
- a neighboring user project still does.

## 9. Work package D — One bounded traversal across multiple global roots

If platform policy produces multiple global roots (for example "/" plus "/usr/local"), do not create an independent worker pool per root.

Use dua-core multi-root streaming/shared traversal facilities or an equivalent one-pool abstraction.

Requirements:

- one bounded worker pool for the discovery operation;
- deterministic final manifest ordering after traversal;
- duplicate/canonical-equivalent roots deduplicated before walking;
- no duplicate manifest resolution when roots overlap;
- progress counters remain monotonic and exact.

Explicit single-root scans may keep the simpler single-root path if it is measurably cheaper.

## 10. Work package E — Canonical workspace member source roots

Normalize WorkspaceMember.source_root to a physical/canonical path when the manifest exists and can be resolved.

Preserve the Cargo-reported manifest path separately where needed for diagnostics/identity.

The source-activity exclusion set and member roots must use compatible physical identity so target/build output is actually pruned when the clean/scan root contains a symlinked ancestor such as macOS /tmp -> /private/tmp-style layouts.

Fail closed when member-root physical identity required for activity cannot be established.

Do not canonicalize every filesystem entry during traversal; normalize once at workspace resolution.

### Eligibility qualification

Because this fix can remove false source activity caused by walking the workspace's own output tree, tests must prove:

- recent source outside output still protects the workspace;
- recent target/build output still protects through the output-activity gate;
- old source + old output remains eligible;
- recent output is not accidentally hidden from both gates;
- symlinked ancestor spelling produces the same final eligibility as the canonical spelling;
- Preview/Simulate/Execute remain protected by fresh output activity in final proof.

## 11. Work package F — Evidence-driven second-pass pruning

After A-E, rerun the no-argument profile.

Only if the reference scan still fails the M006A qualification target may implementation add more global-only prune rules.

Every additional rule needs:

- measured material cost;
- a documented ownership/semantic reason the tree is not a user project domain;
- fixture coverage proving nearby project-bearing paths remain reachable;
- README documentation.

Do not turn a performance milestone into an arbitrary blacklist.

Potential large writable domains must remain scanned unless a separate policy/ADR changes product semantics.

## 12. Required tests

### Policy

- macOS policy preserves /usr/local while excluding protected /usr content;
- global-only prunes do not narrow an explicit /usr/local or explicit user-provided root;
- root/prune identity handles canonical aliases safely;
- duplicate global roots do not duplicate manifests.

### Managed Rust trees

- Cargo registry/git remain pruned;
- RUSTUP_HOME is pruned globally;
- user project adjacent to RUSTUP_HOME is discovered;
- relative RUSTUP_HOME does not create an unsafe prune.

### Source-root normalization

- canonical and symlinked ancestor fixtures have identical source/output activity disposition;
- output tree is excluded from source walk;
- recent output still skips at output gate;
- recent true source still skips at source gate;
- Windows path normalization fixtures remain green.

### Traversal/performance

- one bounded pool across multi-root global scan;
- no pre-scan;
- no duplicate manifest resolution;
- --stats counters sum consistently;
- explicit-root scan result stdout is unchanged.

## 13. Performance qualification

Record release-build results with:

cargo-cleanme --no-progress --stats

Required evidence:

1. same reference developer tree used by C002/C003 where available;
2. no-argument scan on the representative macOS host;
3. synthetic deep/wide global-like fixture;
4. at least one Linux host/global-like fixture;
5. explicit-root regression comparison.

M006A closure requires the reference macOS no-argument scan to finish inside the prior 120-second timeout.

Target, not canonical product promise: <=60 seconds on that same host.

Also record:

- entries visited/pruned by category;
- manifests;
- locate/metadata calls;
- discovery, Cargo resolution, activity, sizing, total time;
- peak/representative memory where practical.

If the full scan still exceeds 120 seconds after safe measured pruning, do not close M006A. Record the dominant remaining tree and write a narrower follow-up rather than silently pruning writable user domains.

## 14. Compatibility

- CLI/config schema unchanged.
- Read-only result semantics unchanged except the intended source-root physical-identity correction.
- New global-only system/tool prunes may reduce irrelevant diagnostics and Cargo subprocesses.
- Explicit roots remain user-authoritative.
- Cleanup safety unchanged.
- Rust 1.89 unchanged.

## 15. Acceptance criteria

M006A closes only when:

- global discovery uses a testable platform policy;
- measured OS/tool-managed trees are pruned before manifest handling;
- /usr/local or equivalent developer-writable exceptions remain reachable when parent system trees are pruned;
- RUSTUP_HOME cannot generate user-project Cargo resolution during global scan;
- multiple global roots share one bounded traversal/pool or equivalent measured implementation;
- member source roots and output exclusions use compatible physical identity;
- source/output activity regression matrix is green;
- reference no-argument macOS scan finishes within 120 seconds;
- explicit-root output does not regress;
- hosted Linux/macOS/Windows + Rust 1.89 CI is green.

## 16. Stop conditions

Stop and report rather than broadening prunes if:

- the remaining dominant directory is writable and can legitimately contain user projects;
- achieving the target requires silently dropping /Volumes, /Library, /Applications, /private, /opt, or user homes;
- source-root canonicalization makes recent real source activity invisible;
- multi-root traversal requires unbounded workers;
- performance requires following symlinks;
- a change would weaken C002-C004 safety.

## 17. Closure evidence

Create:

plans/closure/artifact-discovery-cleanup/006a-status.md

Include:

- before/after reference scan measurements;
- prune-category counters;
- global-root/policy matrix per platform;
- RUSTUP_HOME evidence;
- source-root canonicalization activity matrix;
- multi-root worker-pool evidence;
- no-argument macOS result;
- explicit-root regression result;
- CI/MSRV;
- unresolved performance findings.
