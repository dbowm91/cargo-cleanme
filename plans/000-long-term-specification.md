# cargo-cleanme Long-Term Product and Architecture Specification

Status: canonical long-term implementation directive

Companion documents:

- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

This document defines the durable product contract for cargo-cleanme. The initial release is intentionally read-only. Destructive cleanup is a later capability and MUST NOT be enabled merely because discovery appears functional.

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

1. discover Cargo project/workspace roots and, initially, conventional local target/ output; later milestones may resolve redirected target/build output through Cargo itself;
2. avoid presenting projects that appear to be in active development;
3. default the activity guard to 300 seconds;
4. allow the activity guard to be changed in config.toml;
5. support ignored search directories using absolute glob-style paths;
6. support explicit re-included directories under otherwise ignored trees;
7. support an explicit sandbox/root path that becomes the sole search scope;
8. make explicit scope take precedence over ignore/re-include search filters;
9. avoid following symbolic links while traversing;
10. report deterministic project paths and artifact sizes;
11. remain fast enough for routine machine-wide use;
12. treat filesystem permission and transient IO errors as bounded diagnostics rather than whole-scan failures where safe;
13. keep cleanup execution separate from read-only eligibility analysis.

## 3. Initial non-goals

The first release does not:

- delete files or directories;
- invoke cargo clean;
- clean Cargo's global registry/git cache;
- remove individual profiles or packages from a target tree;
- infer arbitrary historical CARGO_TARGET_DIR environment values;
- clean shared external target directories;
- clean Cargo build-dir locations that differ from the conventional local target directory;
- follow symlinks;
- use Git commit timestamps as the definition of project activity;
- require a daemon, index database, filesystem watcher, or background service;
- require a full-screen/raw-mode TUI. A later milestone may provide a transient inline terminal progress display.

Cargo currently permits build.target-dir and build.build-dir to redirect output. Those paths are deliberately deferred until cleanup ownership and shared-cache semantics can be proven safely.

## 4. Architectural principles

### 4.1 Read-only before destructive

Discovery, activity classification, sizing, and reporting MUST be separately testable and releasable before cleanup execution exists.

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

Later workspace/output-aware scanning MUST order its gates so paths that cannot become reportable are rejected before Cargo metadata, source traversal, or deep output sizing whenever safety permits. Ignore/safety pruning occurs before Cargo subprocesses; absent/empty resolved output is rejected before source/deep-size analysis; recent source activity rejects a workspace before deep output sizing.

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

The product MUST work without a configuration file. When present, the canonical user configuration is named config.toml under the platform-appropriate cargo-cleanme configuration directory. The repository SHOULD ship an example/default template, and the CLI SHOULD provide a way to print the config path and initialize that template.

Initial schema:

~~~toml
[scan]
recency_seconds = 300

# Optional exclusive scope.
# root = "/path/to/projects"

ignore = [
  "/path/to/ignore/*",
]

unignore = [
  "/path/to/ignore/except-this-one",
]
~~~

Initial validation rules:

- recency_seconds MUST be a non-negative integer;
- root, ignore entries, and unignore entries MUST be absolute after supported home/environment expansion;
- ignore entries are glob-style search exclusions;
- unignore entries are explicit directory paths in V0.1, not arbitrary inverse globs;
- malformed configuration is an actionable error, not silently ignored configuration.

## 9. Scope and filter precedence

Effective scope precedence is:

~~~text
explicit CLI scan root
    >
configured scan.root
    >
platform system roots
~~~

If either explicit CLI root or configured scan.root is effective, ignore and unignore search filters are not applied. The explicit scope itself is the search sandbox.

Without an explicit scope, ignore filters apply to global discovery. An explicit unignore path takes precedence over an ignore pattern. The walker MUST retain enough ancestry to reach an unignored descendant even when its ancestor matches an ignore rule.

Internal safety pruning such as no symlink traversal, VCS metadata exclusion, and target-tree pruning is not a user filter and remains active in every scope.

## 10. System scan semantics

A no-argument scan is intended to discover user-accessible projects across the host rather than only the current directory.

Platform root enumeration MUST be explicit and tested:

- Unix-like systems begin from the filesystem root and apply platform safety exclusions for pseudo-filesystems that cannot contain durable project data or are unsafe to traverse.
- Windows enumerates suitable local filesystem volumes.
- permission-denied and disappearing entries are recorded and skipped unless they prevent trustworthy analysis of a discovered candidate.

Network filesystems and removable volumes MAY be policy-controlled later. V0.1 MUST document exactly what the platform adapter traverses.

## 11. Symlink and filesystem-boundary policy

cargo-cleanme MUST NOT follow directory symlinks in V0.1.

A symlink named target is not a V0.1 artifact tree.

This prevents accidental traversal into shared caches, cycles, remote mounts reached through links, and cleanup ownership that cannot later be proven from the project root.

## 12. Size semantics

The primary V0.1 size is allocated/on-disk bytes when available.

The implementation SHOULD use metadata already obtained by traversal when possible. The current filesize crate exposes platform-specific allocated-size measurement and fast metadata-aware paths on Unix; on unsupported platforms an apparent-size fallback is acceptable if reported consistently.

V0.1 is an inventory estimate, not a promise of exact bytes eventually reclaimed. The future cleanup report MUST measure post-clean state rather than treating the pre-clean estimate as recovered bytes.

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

V0.1 SHOULD include a machine-readable output mode before destructive automation is added, but human-readable output is the initial required interface.

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

## 15A. Workspace/output-aware extension

After the conventional V0.1 scanner is qualified, redirected/shared-output support uses the Cargo workspace as the logical ownership unit.

Cargo itself is authoritative for workspace/output resolution. cargo-cleanme does not reproduce Cargo configuration-merging semantics.

Target and build output are separate logical roots that may be equal, nested, external, or shared. Existing roots are grouped by physical identity/containment before activity, sizing, or cleanup so bytes are not double-counted.

Shared, uncertain, or externally unproven output MAY be inventoried but MUST NOT be destructively cleaned. External private output requires an explicit cleanup-output authorization boundary in addition to ownership evidence.

Source activity in any workspace member and output activity anywhere in a physical output group protect the workspace/group.

## 16. Cleanup capability requirements

A later cleanup milestone MAY invoke Cargo only after V0.1 discovery is closed.

Before cleaning a candidate it MUST:

1. revalidate project activity against a new cutoff;
2. verify the project/artifact relationship still matches the analyzed candidate;
3. refuse ambiguous or shared/redirected output ownership;
4. invoke Cargo against the intended manifest/artifact contract rather than blindly deleting arbitrary directories;
5. capture process failure independently per candidate;
6. measure disk state after cleanup;
7. report attempted, cleaned, skipped, failed, and actually recovered bytes separately;
8. preserve physical target/build output identity so overlapping roots are measured once;
9. refuse shared, uncertain, or unauthorized external output.

A later `--dryrun` simulation mode MAY exercise discovery, resolution, filtering, sizing, authorization, revalidation, progress, and final reporting while prohibiting every `cargo clean` subprocess. Such simulation output is estimated/would-clean space, not recovered bytes.

The existence of cargo clean --dry-run MAY support later qualification but does not replace ownership validation.

## 17. Platform and release direction

The initial implementation target is Rust 1.89+ with Rust 2024 edition unless implementation evidence requires a narrowly justified revision.

Initial supported development platforms are Linux, macOS, and Windows. CI SHOULD cover all three before V0.1 is described as portable.

Distribution, shell installers, self-update, package-manager publishing, and release signing are roadmap concerns after the read-only core closes.

## 18. Research basis

Current ecosystem assumptions used by this specification:

- dua-core 4.1.0: https://docs.rs/dua-core/4.1.0/dua_core/
- Cargo configuration target-dir/build-dir: https://doc.rust-lang.org/cargo/reference/config.html
- Cargo build cache layout: https://doc.rust-lang.org/cargo/reference/build-cache.html
- filesize allocated-size semantics: https://docs.rs/filesize/latest/filesize/

These are implementation evidence, not substitutes for the normative contracts above.

## 19. Initial product closure

The read-only V0.1 foundation is closed only when one command can scan the intended scope, apply exact configuration precedence, avoid active projects, report stable artifact sizes, tolerate ordinary filesystem errors, and pass Linux/macOS/Windows CI plus fixture-based behavioral tests without containing any destructive code path.
