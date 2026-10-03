# cargo-cleanme Terminology and Domain Model

Status: canonical terminology

This document defines terms used by the specification, roadmap, implementation plans, tests, and future cleanup reports.

## 1. Scan

One invocation that captures a scan start time, resolves configuration, enumerates discovery roots, discovers Cargo projects, analyzes activity/artifacts, and emits a report.

The scan start time is fixed for the invocation. Recency decisions MUST NOT drift because a long scan repeatedly samples wall clock time.

## 2. Scan scope

The set of filesystem roots from which project discovery begins.

A scan scope is either:

- explicit: a CLI root or configured scan.root; or
- global: platform-defined system roots.

## 3. Explicit scope / sandbox root

A user-supplied root that limits the entire scan to one tree.

Effective precedence:

~~~text
CLI root > configured scan.root > global platform roots
~~~

When an explicit scope is active, user ignore/unignore search filters are bypassed. Internal safety pruning still applies.

## 4. Discovery filter

A path rule used only while finding Cargo projects during global scanning.

### Ignore pattern

An absolute glob-style pattern identifying search subtrees that should normally be pruned.

### Unignore path

An explicit absolute directory path that re-enters a tree otherwise excluded by an ignore pattern.

V0.1 unignore values are exact directory paths. They are not inverse glob expressions.

Discovery filters do not weaken the activity check inside a project that has already been discovered.

## 5. Internal prune rule

A traversal rule owned by cargo-cleanme rather than user configuration, such as:

- never follow directory symlinks;
- do not descend into .git metadata;
- do not search within target for nested projects.

Internal prune rules remain active even when scan.root is configured.

## 6. Cargo project root

A directory containing Cargo.toml.

V0.1 does not require the directory to be a Git repository.

## 7. Workspace root

A Cargo project root that owns the conventional target directory shared by workspace members.

A member manifest without its own direct target is not independently reportable.

## 8. Independent nested project

A Cargo project beneath another scanned directory that has its own project boundary and conventional direct target. It MAY be reported independently.

A nested VCS root is treated as an activity-boundary for its parent so independent repository activity does not accidentally classify the parent.

## 9. Conventional artifact tree

A real, non-symlink directory named target directly beneath a Cargo project root.

Redirected target-dir/build-dir output is not a conventional artifact tree in V0.1.

## 10. Artifact-bearing candidate

A discovered Cargo project whose conventional target directory contains at least one artifact entry and is therefore worth activity/size analysis.

Empty target directories are not reportable.

## 11. Recency window

The duration in seconds during which modification activity protects a project from being reported.

Default: 300 seconds.

## 12. Cutoff

For one scan:

~~~text
cutoff = scan_start_time - recency_seconds
~~~

The same cutoff is used throughout that scan.

## 13. Source activity

The newest trustworthy modification time observed under the project root while excluding:

- its conventional target tree;
- VCS metadata;
- independent nested repository boundaries.

The activity walker may terminate as soon as it observes a time at or newer than the cutoff.

## 14. Target activity

The newest trustworthy modification time observed while analyzing the conventional target tree.

## 15. Project activity

The maximum of source activity and target activity.

A future timestamp is treated as active.

## 16. Active project

A project for which source or target activity is at or newer than the cutoff, or where timestamp ambiguity requires conservative exclusion.

Active projects do not appear in the default reclaimable report.

## 17. Inactive project

A candidate for which all required activity evidence is older than the cutoff.

Inactive does not mean abandoned; it means no filesystem activity was observed within the configured guard window.

## 18. Artifact size

The measured size of the conventional artifact tree.

Allocated/on-disk bytes are preferred. A fallback metric, if required by a platform, MUST be explicit in implementation/documentation.

## 19. Eligible result

A project record that has:

- valid discovery evidence;
- artifact presence;
- inactive classification;
- trustworthy size analysis.

Only eligible results are printed in the normal V0.1 report.

## 20. Scan diagnostic

Structured information about an IO, metadata, configuration, or platform event that did not necessarily fail the whole scan.

Diagnostics have severity sufficient to distinguish:

- informational skips;
- non-fatal errors;
- candidate-disqualifying errors;
- fatal scan errors.

## 21. Scan report

The deterministic collection of eligible results plus totals and diagnostic summary.

Future JSON output should serialize this model rather than re-parsing human text.

## 22. Cleanup candidate

A future destructive-operation input produced by revalidation, not merely by copying an earlier eligible result.

No V0.1 type named cleanup candidate should imply cleanup is currently available.

## 23. Recovered bytes

A future post-clean measurement of storage no longer occupied after a successful cleanup operation.

Pre-clean artifact size MUST NOT be labeled recovered bytes.


## 24. Resolved workspace

A Cargo workspace identity resolved by Cargo itself from one or more discovered manifests.

It includes the workspace root/root manifest, workspace members needed for source-activity analysis, Cargo runtime capability information, and the resolved output set.

Multiple discovered member manifests may map to one resolved workspace.

## 25. Output root

One logical Cargo output location for a resolved workspace.

Kinds:

- target output root — final/user-facing Cargo output;
- build output root — intermediate Cargo/rustc output when Cargo distinguishes it.

Target and build roots may be equal.

## 26. Output set

The target/build output roots associated with one resolved workspace under one Cargo resolution context.

The output set is logical. Physical overlap is resolved separately.

## 27. Physical output group

One connected physical filesystem region formed by canonical equal/ancestor/descendant relationships among output roots.

A physical output group is the unit for deduplicated size/activity measurement. Equal or nested logical roots are counted once.

A group may have one or many workspace owners.

## 28. Output ownership class

The cleanup-safety classification of a physical output group:

- PrivateBounded — one workspace owner and all destructive paths lie within an authorized cleanup boundary;
- ExternalUnproven — one observed owner but external exclusivity/authorization is not proven;
- Shared — multiple workspaces resolve to equal/overlapping physical output;
- Uncertain — resolution/path/activity/ownership evidence is incomplete.

Only PrivateBounded is eligible for M005 destructive cleanup.

## 29. Cleanup output authorization root

An explicit absolute directory root under which otherwise external private Cargo output may be cleaned.

Authorization is not ownership proof. It never makes a Shared or Uncertain group cleanable.

The explicit clean source sandbox implicitly authorizes private output physically contained within that sandbox.

## 30. Inline progress status

Transient attended-terminal status rendered while a scan/cleanup is executing.

It is not a report and has no effect on eligibility. It is written to stderr, rate-limited, limited to a small number of visible candidate rows, and cleared before final deterministic output.

## 31. Cargo preview

A non-mutating cleanup mode that may invoke `cargo clean --dry-run --verbose` for an otherwise authorized candidate.

## 32. Simulation / --dryrun

A cargo-cleanme debugging/performance mode that executes cargo-cleanme's own discovery, Cargo resolution/metadata, filtering, sizing, authorization, revalidation, progress, and final reporting but invokes no Cargo `clean` command.

Simulation reports would-clean/estimated bytes, never recovered bytes.


## 33. Cleanup unit

The destructive-operation unit corresponding to exactly one Cargo workspace cleanup invocation.

A cleanup unit contains one resolved workspace, its complete OutputSet, and every PhysicalOutputGroup the Cargo invocation can affect.

PhysicalOutputGroup remains the unit for physical deduplication and ownership classification. CleanupUnit is the unit for destructive authorization, progress, Cargo invocation, and cleanup reporting.

A cleanup unit is eligible only when every affected physical group independently satisfies the destructive ownership and authorization boundary. No partial target-only/build-only cleanup is implied.

One cleanup unit produces at most one Cargo clean invocation and one cleanup result.

## 34. Cleanup ownership universe

The complete set of Cargo workspaces discovered within the explicit clean ROOT for one cleanup invocation.

Immediately before destructive execution, cargo-cleanme may re-resolve this bounded set and rebuild the physical output graph to prove that the candidate CleanupUnit has not become shared with another discovered workspace.

This bounded universe does not prove that an external cache has no owners outside clean ROOT; such output remains ExternalUnproven under ADR 001.
