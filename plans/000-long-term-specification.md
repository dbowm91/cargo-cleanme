# cargo-cleanme Long-Term Product and Architecture Specification

Status: canonical long-term implementation directive

Companion documents:

- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

This document defines the durable product contract for cargo-cleanme. The initial V0.1 delivery was intentionally read-only; the current product also provides separately invoked, Cargo-mediated destructive cleanup after complete ownership and freshness proof. No-argument invocation and scan remain read-only.

The keywords MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, and MAY are normative.

## 1. Product definition

cargo-cleanme is a Rust-specific filesystem utility that inventories reclaimable Cargo build artifacts across a machine or an explicitly bounded project tree.

The installed executable is cargo-cleanme and MUST be usable both directly and as a Cargo external subcommand:

~~~text
cargo-cleanme
cargo cleanme
~~~

With no destructive subcommand, the default operation is a read-only scan.

## 2. Primary goals

The product MUST:

1. discover Cargo project/workspace roots and resolve target/build output through Cargo itself;
2. avoid presenting projects that appear to be in active development;
3. default the activity guard to 300 seconds;
4. allow the activity guard to be changed in config.toml;
5. support ignored search directories using absolute glob-style paths;
6. support explicit re-included directories under otherwise ignored trees;
7. support an explicit sandbox/root path that becomes the sole search scope;
8. make explicit scope take precedence over ignore/re-include search filters;
9. avoid following symbolic links while traversing;
10. report deterministic project paths and artifact sizes;
11. remain fast enough for routine developer use while retaining explicit exhaustive Full reconciliation;
12. treat filesystem permission and transient IO errors as bounded diagnostics rather than whole-scan failures where safe;
13. keep cleanup execution separate from read-only eligibility analysis.

## 3. Historical V0.1 boundary and current non-goals

The initial V0.1 release did not delete files, invoke `cargo clean`, resolve redirected output for cleanup, or support profile/package selectors. Those were staged-delivery constraints, not the current product boundary. The current product:

- keeps no-argument invocation and scan read-only;
- offers cleanup only as a separate explicit command, using Cargo after complete workspace/output ownership, authorization, activity, marker, and freshness proof;
- resolves target/build output through Cargo and permits cleanup only for completely proven private bounded output;
- delegates qualified profile/package selection to Cargo and never directly deletes inferred selector paths.

Current non-goals are:

- clean Cargo's global registry/git cache;
- directly delete inferred profile/package artifact paths or parse Cargo-private artifact/cache layout;
- claim selector-specific reclaimable bytes where Cargo does not provide a trustworthy estimate;
- infer arbitrary historical CARGO_TARGET_DIR environment values;
- clean shared, uncertain, or externally unproven output;
- follow symlinks;
- use Git commit timestamps as the definition of project activity;
- require a daemon, index database, filesystem watcher, or background service;
- require a full-screen/raw-mode TUI. A later milestone may provide a transient inline terminal progress display.

Cargo permits `build.target-dir` and `build.build-dir` to redirect output. Current support resolves these paths through Cargo, classifies their physical ownership, and cleans only when the complete output set is `PrivateBounded`, authorized, fresh, and safe. A distinct build-dir is supported only where runtime Cargo capability is known.

## 4. Architectural principles

### 4.1 Read-only before destructive

Discovery, activity classification, sizing, and reporting MUST remain separately testable from cleanup execution. No-argument invocation and scan MUST remain read-only.

### 4.2 Filesystem state is authoritative for activity

Recent uncommitted edits and recent builds matter even when Git history has not changed. Project activity therefore derives from filesystem modification times, not commit timestamps.

### 4.3 Conservative eligibility

Ambiguity favors keeping a project out of the reclaimable set. Future timestamps, incomplete metadata needed to establish inactivity, and activity inside the recency window MUST NOT produce an eligible result.

### 4.4 Prune early

Traversal MUST avoid descending into trees that cannot contribute useful results. In particular:

- symbolic links are not followed;
- VCS metadata is not scanned for project activity;
- target trees are not recursively searched for nested projects;
- ignored discovery subtrees are pruned unless traversal is required to reach an explicit re-included path;
- once recent activity is proven, that project's activity scan may terminate early.

### 4.5 Traversal implementation is replaceable

Filesystem walking MUST sit behind a small internal boundary. dua-core 4.1 is the preferred initial engine because it provides parallel work-stealing traversal, directory descent predicates, multi-root streaming, cancellation, error-as-item behavior, and non-following symlink semantics. The domain model MUST NOT expose dua-core-specific types.

### 4.6 Report disk use, not source-tree size

The reported size represents the candidate artifact tree, not the entire repository. V0.1 SHOULD report allocated/on-disk bytes where the platform provides them and MUST document any platform fallback to apparent bytes.

### 4.7 Fail fast before expensive analysis

Workspace/output-aware scanning MUST order its gates so paths that cannot become reportable are rejected before Cargo metadata, source traversal, or deep output sizing whenever safety permits. Ignore/safety pruning occurs before Cargo subprocesses; absent/empty resolved output is rejected before source/deep-size analysis; recent source activity rejects a workspace before deep output sizing.

A progress display MUST NOT require a second filesystem traversal merely to compute a percentage.

### 4.8 Interactive status is transient

When stderr is an attended terminal, the default CLI MAY render a small inline progress/status surface while scanning. It remains on the main screen, MUST NOT require raw mode, is rate-limited, and is cleared before deterministic final output. Non-terminal output MUST contain no progress-control sequences.

## 5. Canonical V0.1 scan pipeline

The logical pipeline is:

~~~text
effective scan scope
      |
      v
project discovery
Cargo.toml + sibling target/
      |
      v
source activity gate
exclude .git, target, nested VCS roots
short-circuit if recent
      |
      v
target analysis
size + newest target mtime + artifact presence
      |
      v
eligibility
max(source activity, target activity) older than cutoff
      |
      v
stable sort + report
~~~

Discovery MAY overlap with analysis internally, but externally observable semantics MUST remain equivalent to this pipeline.

## 6. Cargo project and artifact eligibility

A V0.1 Cargo project is a directory containing Cargo.toml.

A conventional artifact tree is a real, non-symlink direct child directory named target.

A project becomes a reportable candidate only when:

- Cargo.toml exists at the project root;
- target exists as a real directory;
- target contains at least one filesystem entry representing build output;
- no disqualifying recent activity is found;
- target analysis completes sufficiently to produce a trustworthy size.

Workspace members that use only the workspace-root target directory do not become duplicate candidates merely because they contain their own Cargo.toml. A nested independent Cargo project with its own direct target MAY be a distinct candidate.

## 7. Activity semantics

Let cutoff = scan_start_time - recency_seconds.

Project activity is the maximum trustworthy modification time observed from:

1. project content beneath the project root, excluding the candidate target tree, VCS metadata, and independent nested repository boundaries; and
2. entries in the candidate target tree while it is sized.

An entry at or newer than cutoff makes the project active.

Timestamps later than scan_start_time are conservatively active.

The source-side walk SHOULD terminate as soon as recent activity is established. The target walk still computes size only for projects that survive the source gate.

Directory mtimes MAY participate because create/remove/rename operations can update a directory without modifying an existing file.

## 8. Configuration contract

The canonical user configuration is named config.toml under the platform-appropriate cargo-cleanme configuration directory. On first operational use, a missing config MUST be created atomically from the checked-in canonical template before loading. Existing malformed configuration is an actionable error and MUST NOT be replaced automatically. A path-inspection command such as `config path` remains side-effect free.

The CLI MUST provide a `config edit` command that ensures the effective config exists, opens it in the user's editor, waits for editor completion, and validates the resulting TOML. Editor resolution follows `VISUAL`, then `EDITOR`, then bounded executable fallbacks; editor specifications with arguments are parsed without invoking a shell.

Current schema direction:

~~~toml
[scan]
recency_seconds = 300
learned_root_retention_days = 30

# Optional exclusive scope retained for compatibility.
# root = "/path/to/projects"

ignore = [
  "/path/to/ignore/*",
]

unignore = [
  "/path/to/ignore/except-this-one",
]
~~~

Validation rules:

- recency_seconds MUST be a non-negative integer;
- learned_root_retention_days MUST be a bounded non-negative integer with a documented default of 30;
- root, ignore entries, and unignore entries MUST be absolute after supported home/environment expansion;
- ignore entries are glob-style search exclusions;
- unignore entries are explicit directory paths, not arbitrary inverse globs;
- malformed configuration is an actionable error, not silently ignored configuration.

Machine-learned project/root inventory is NOT stored in config.toml. It is versioned machine-local application state under the platform state/data-local directory and may be rebuilt by Full discovery.

## 9. Scope and filter precedence

Discovery intent is resolved in this order:

~~~text
explicit CLI scan root
    >
explicit --full request
    >
configured scan.root (legacy exclusive scope)
    >
Routine adaptive roots
~~~

A CLI root and `--full` are mutually exclusive.

If an explicit CLI root or configured scan.root is effective, ignore/unignore search filters are bypassed and the explicit scope itself is the search sandbox.

Routine and Full discovery may apply configured discovery filters. An explicit unignore path takes precedence over an ignore pattern. The walker MUST retain enough ancestry to reach an unignored descendant even when its ancestor matches an ignore rule.

Internal safety pruning such as no symlink traversal, VCS metadata exclusion, and target-tree pruning is not a user filter and remains active in every scope.

## 10. Routine and Full scan semantics

A no-argument scan is a **Routine** scan over a bounded adaptive root set. It is shallow in scope but recursively complete beneath each selected root.

Routine roots consist of conservative existing platform/user seed directories plus active learned developer roots from machine-local discovery state. Broad roots such as the filesystem root, a drive root, the user's home directory itself, /Users, or /home MUST NOT become routine roots merely because they contain a Rust project.

`scan --full` is the explicit **Full** reconciliation operation. Full discovery retains the exhaustive platform reachability established by M006A-M006D:

- Unix-like systems may begin from the filesystem root, with tested safety/performance exclusions for OS-managed trees that cannot represent ordinary user project locations.
- A platform policy MAY split a writable exception such as macOS /usr/local into a separate Full root when its protected parent is pruned.
- Tool-managed Rust trees such as Cargo registry/git caches and rustup-installed toolchains MAY be pruned during Full discovery when their location is established from authoritative environment/platform rules.
- Performance-only Full prunes MUST NOT silently narrow an explicit user-provided scan root.
- Windows enumerates suitable local filesystem volumes.
- permission-denied and disappearing entries are recorded and skipped unless they prevent trustworthy analysis of a discovered candidate.

A successfully completed Full scan records the exact Cargo workspaces it resolves and derives learned Routine roots for future scans. Learned roots record the last time a Rust project was positively observed beneath them. The default learned-root retention is 30 days and is configurable.

Only a successfully completed Full reconciliation may expire learned roots based on absence/age. Routine or Explicit scans may advance positive last-seen timestamps but MUST NOT remove roots from negative evidence. A failed/cancelled/partial Full scan MUST NOT publish a pruned state generation. Uncertain Full coverage retains affected learned roots.

Learned-state retention NEVER narrows Full or Explicit discovery. A location expired from Routine scope can be rediscovered by a later Full scan.

Broad writable domains are not eligible for permanent Full pruning merely because they are expensive.

Network filesystems and removable volumes MAY be policy-controlled later. The platform adapter MUST document exactly what Full discovery traverses.

## 11. Symlink and filesystem-boundary policy

cargo-cleanme MUST NOT follow directory symlinks in V0.1.

A symlink named target is not a V0.1 artifact tree.

This prevents accidental traversal into shared caches, cycles, remote mounts reached through links, and cleanup ownership that cannot later be proven from the project root.

## 12. Size semantics

The primary V0.1 size is allocated/on-disk bytes when available.

The implementation SHOULD use metadata already obtained by traversal when possible. The current filesize crate exposes platform-specific allocated-size measurement and fast metadata-aware paths on Unix; on unsupported platforms an apparent-size fallback is acceptable if reported consistently.

Inventory size is an estimate, not a promise of exact bytes reclaimed. Cleanup reports MUST measure post-clean state rather than treating a pre-clean estimate as recovered bytes.

## 13. Output contract

Human output is size-descending by default and deterministic for equal sizes.

Interactive progress, when enabled, is ephemeral stderr state only. It is cleared before this final report and is never part of the machine-readable or deterministic human-output contract.

A successful scan prints one line per eligible Cargo project and a summary, for example:

~~~text
18.42 GiB  /home/me/projects/old-router
 7.81 GiB  /home/me/src/experiment

26.23 GiB reclaimable across 2 inactive Cargo projects
~~~

The default path is the project root, not only target/.

The current CLI supports `--format json` using a dedicated schema-v1 DTO, one deterministic newline-terminated document on stdout, and typed policy/safety/action reason codes. Progress and `--stats` remain stderr-only. Diagnostic detail text is not stable API. External schedulers may invoke bounded unattended cleanup with `--yes`; cargo-cleanme does not include an internal scheduler or daemon.

## 14. Error and partial-result semantics

The scan MUST distinguish:

- fatal configuration/scope errors;
- non-fatal traversal errors outside candidates;
- candidate-analysis errors that make one project ineligible because its inactivity or size cannot be established;
- successful zero-result scans.

Diagnostics SHOULD be summarized without flooding normal output. A verbose mode MAY expose individual skipped paths and IO failures.

## 15. Performance requirements

The design SHOULD:

- use bounded parallelism;
- avoid spawning one thread pool per target directory;
- reuse a fixed worker pool across candidate target roots when possible;
- perform discovery without recursively sizing target directories;
- short-circuit recent projects before expensive target analysis;
- avoid canonicalizing every filesystem entry;
- avoid retaining a complete filesystem tree in memory.

Performance qualification MUST include representative synthetic deep/wide trees and at least one real developer tree. Absolute timing gates are secondary to regression comparisons because storage hardware varies substantially.

Routine-scan performance SHOULD scale with the selected developer root set rather than whole-host filesystem breadth. Full-scan performance work SHOULD attribute traversal/prune/Cargo-resolution costs before adding exclusions, SHOULD avoid constructing one worker pool per Full root, and MUST preserve deterministic discovery semantics.

Machine-learned discovery state is a search optimization only. It MUST NOT be used as Cargo workspace/output ownership proof or cleanup authorization.

Workspace member roots and resolved output exclusions SHOULD use compatible physical path identity so output trees are not redundantly traversed as source activity on platforms with path aliases/symlinked ancestors.

Cleanup-proof performance work MAY remove redundant Cargo subprocesses or use bounded parallel read-only refresh within one proof, but MUST NOT reuse stale workspace/output state across destructive candidates unless an independently reviewed invalidation model preserves C003/C004 freshness guarantees.

## 15A. Workspace/output-aware extension

After the conventional V0.1 scanner is qualified, redirected/shared-output support uses the Cargo workspace as the logical ownership unit.

Cargo itself is authoritative for workspace/output resolution. cargo-cleanme does not reproduce Cargo configuration-merging semantics.

Target and build output are separate logical roots that may be equal, nested, external, or shared. Existing roots are grouped by physical identity/containment before activity, sizing, or cleanup so bytes are not double-counted.

Shared, uncertain, or externally unproven output MAY be inventoried but MUST NOT be destructively cleaned. External private output requires an explicit cleanup-output authorization boundary in addition to ownership evidence.

Source activity in any workspace member and output activity anywhere in a physical output group protect the workspace/group.

## 16. Cleanup capability requirements

A cleanup command MAY invoke Cargo only after current workspace/output ownership and safety proof succeeds. Phase 9 adds policy and selector behavior within that proof boundary.

Before cleaning a candidate it MUST:

1. revalidate project activity against a new cutoff;
2. verify the project/artifact relationship still matches the analyzed candidate;
3. refuse ambiguous or shared/redirected output ownership;
4. invoke Cargo against the intended manifest/artifact contract rather than blindly deleting arbitrary directories;
5. capture process failure independently per candidate;
6. measure disk state after cleanup;
7. report attempted, cleaned, skipped, failed, and actually recovered bytes separately;
8. preserve physical target/build output identity so overlapping roots are measured once;
9. refuse shared, uncertain, or unauthorized external output;
10. authorize destructive cleanup at the workspace invocation boundary: every physical output group that one Cargo clean invocation can affect through the resolved OutputSet MUST pass ownership, authorization, activity, and path-safety checks before any mutation;
11. produce one cleanup result and one deduplicated pre/post union measurement per Cargo workspace cleanup invocation;
12. establish complete cleanup-scope ownership coverage: every Cargo manifest discovered under the explicit clean root MUST either be authoritatively covered by a successfully resolved Cargo workspace or remain an unresolved ownership participant, and any unresolved participant MUST block all Cargo cleanup commands for that scope.

A later `--dryrun` simulation mode MAY exercise discovery, resolution, filtering, sizing, authorization, revalidation, progress, and final reporting while prohibiting every `cargo clean` subprocess. Such simulation output is estimated/would-clean space, not recovered bytes.

The existence of cargo clean --dry-run MAY support later qualification but does not replace ownership validation.

### 16.1 Current cleanup policy and selectors

Workspace minimum-size, minimum-inactivity-age, include, and exclude policy MUST be evaluated only after complete ownership resolution. Policy-rejected workspaces remain in the ownership universe. Size and age inputs MUST be refreshed in the final proof immediately before Cargo spawn. Profile/package selectors narrow the Cargo request only; they MUST NOT narrow workspace, manifest, or output ownership proof. Selector-specific estimates are unknown and MUST be represented as such; whole output-union measurements are context only. Any nonzero minimum-size threshold with a selector MUST fail closed. Unsupported or unqualified runtime Cargo/selector combinations MUST fail before mutation.

Profile selection is enabled only for exact Cargo releases 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0. Package selection is enabled only for exact Cargo 1.98.1 and 1.99.0. Other and future releases remain fail-closed until separately qualified. Cargo package cleanup may remove shared dependency artifacts; cargo-cleanme does not promise package-unique reclamation.

## 17. Platform and release direction

The initial implementation target is Rust 1.89+ with Rust 2024 edition unless implementation evidence requires a narrowly justified revision.

Initial supported development platforms are Linux, macOS, and Windows. CI SHOULD cover all three before V0.1 is described as portable.

Distribution, shell installers, self-update, package-manager publishing, and release signing remain deferred Phase 10 roadmap concerns. Phase 9 closure does not activate them.

## 18. Research basis

Current ecosystem assumptions used by this specification:

- dua-core 4.1.0: https://docs.rs/dua-core/4.1.0/dua_core/
- Cargo configuration target-dir/build-dir: https://doc.rust-lang.org/cargo/reference/config.html
- Cargo build cache layout: https://doc.rust-lang.org/cargo/reference/build-cache.html
- filesize allocated-size semantics: https://docs.rs/filesize/latest/filesize/

These are implementation evidence, not substitutes for the normative contracts above.

## 19. Initial product closure

The read-only V0.1 foundation is closed only when one command can scan the intended scope, apply exact configuration precedence, avoid active projects, report stable artifact sizes, tolerate ordinary filesystem errors, and pass Linux/macOS/Windows CI plus fixture-based behavioral tests without containing any destructive code path.
