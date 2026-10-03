# M006D — Global Scan Completion Qualification

Status: conditionally closed

Closure: `plans/closure/artifact-discovery-cleanup/006d-status.md`. Full-scope traversal optimizations and parity checks landed, but the exact 120-second scan acceptance remains unmet; resolving the scope/latency policy is proposed as M006E.

Repository baseline: `bbfff4b` (M006C bounded traversal improvements and parity tests).

Source milestone: M006 performance hardening and routine machine-wide qualification

Primary class: performance / filesystem traversal

## 1. Objective

Complete the reference macOS no-argument global scan within 120 seconds while preserving every currently reachable project-bearing root, no-follow behavior, filters, ordering, and cleanup safety. M006C reduced traversal overhead and established fixture parity, but repeated current-host release runs still exceeded the bound.

## 2. Triggering evidence

On Darwin 25.6 x86_64, the current release no-argument command was terminated by a 120-second timeout without final stats. A 120-second profile reached approximately 4.63 million visited entries; `/Users` dominated, while `/Library`, `/Applications`, `/opt`, and `/private` contributed material counts. Parent-first and completion-order profiles both approached the total traversal set but neither produced a completed report within the bound. M006C closure evidence is in `plans/closure/artifact-discovery-cleanup/006c-status.md`.

## 3. Scope and invariants

- Keep global roots and all writable/project-bearing domains in scope.
- Preserve no-follow symlink behavior, complete manifest discovery, ignore/unignore precedence, deterministic final ordering, and categorized counters.
- Use one bounded traversal pool per global discovery operation.
- Preserve explicit-root behavior, cleanup proof completeness, and Cargo call semantics.
- Preserve Rust 1.89 and Linux/macOS/Windows behavior.

## 4. Non-goals

- Pruning `/Users`, `/Library`, `/Applications`, `/private`, `/Volumes`, `/opt`, hidden directories, or arbitrary user trees.
- Following symlinks, adding persistent indexes/watchers/daemons, or changing cleanup safety.
- Claiming success from partial entry-rate extrapolation.

## 5. Work packages

1. Separate traversal, manifest validation, counter finalization, sorting, and report emission timings on the reference host.
2. Identify and measure the dominant completion tail, including manifest revalidation and output sorting, without weakening race or symlink checks.
3. Compare only bounded, fixture-proven strategies against the same full scope; capture wall time, CPU, peak memory, manifest set, and categorized counters.
4. Implement the smallest measured full-scope improvement and prove manifest parity, deterministic ordering, filtering, symlink, and explicit-root behavior.
5. Repeat the exact unprofiled release no-argument qualification and run platform/MSRV CI.

## 6. Acceptance

- Exact no-argument Darwin release scan completes with final `--stats` output within 120 seconds.
- Full manifest set, filter behavior, symlink policy, global root policy, and cleanup proof semantics remain intact.
- Synthetic deep/wide, explicit-root, platform-prune, and ignore/unignore tests pass.
- Linux/macOS/Windows hosted CI and Rust 1.89 pass.

## 7. Stop condition

If the time bound cannot be met while preserving global reachability and safety invariants, stop implementation and record the remaining product policy decision explicitly. Do not characterize partial traversal as a completed scan.

## 8. Closure evidence

Create `plans/closure/artifact-discovery-cleanup/006d-status.md` with matched before/after measurements, full-scan counters, subtree attribution, manifest parity, verification commands, unresolved findings, and disposition.
