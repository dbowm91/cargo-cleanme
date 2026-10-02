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
