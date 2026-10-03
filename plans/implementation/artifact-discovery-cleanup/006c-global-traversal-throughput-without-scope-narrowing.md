# M006C — Global Traversal Throughput Without Scope Narrowing

Status: ready

Repository baseline: `755eb66` (M006A/M006B implementation and cross-platform test correction); current trigger measurements are recorded below and in `plans/closure/artifact-discovery-cleanup/006a-status.md`.

Source milestone: M006 performance hardening and routine machine-wide qualification

Primary class: performance / filesystem traversal

## 1. Objective

Finish the no-argument macOS scan within the M006A 120-second qualification bound while preserving the global scan's user-project search scope. Improve bounded traversal throughput using measured traversal and filesystem-enumeration work; do not remove writable project-bearing roots or trees from the scan.

## 2. Triggering evidence

M006A applied the allowed OS-managed and rustup-managed prunes, but the release no-argument scan still exceeded 120 seconds on the reference macOS host. A 40-second opt-in subtree profile observed 4,203,630 visited entries, including:

- `/Users`: 3,105,324 entries;
- `/Library`: 438,855 entries;
- `/Applications`: 360,321 entries;
- `/opt`: 181,982 entries;
- `/private`: 117,129 entries.

These domains can contain user projects and are intentionally not pruned. The largest measured cost is user-controlled `/Users`; M006C must improve traversal cost without weakening reachability.

## 3. Scope and invariants

- Preserve global root selection and current explicit-root precedence.
- Preserve no-follow symlink behavior, manifest completeness, filter semantics, deterministic ordering, and Cargo-call behavior.
- Keep one bounded worker pool per global discovery operation.
- Preserve category counters and bounded first-level subtree attribution.
- Any engine-specific fast path must remain testable on Linux, macOS, and Windows.
- Do not prune `/Users`, `/Library`, `/Applications`, `/private`, `/Volumes`, `/opt`, or hidden directories by default.
- Preserve Rust 1.89 and CLI/config compatibility.

## 4. Non-goals

- Changing the default scan scope to a home directory or another reduced root.
- Adding arbitrary skip lists, a daemon, filesystem watcher, or persistent index.
- Weakening Cargo ownership or cleanup proof behavior.
- Following symlinks to increase reachability.

## 5. Investigation and implementation work packages

1. Capture release `--stats` and subtree profiles on the reference host, separating directory enumeration, metadata/type work, filtering, and manifest handling where practical.
2. Compare bounded worker caps and supported `dua-core` enumeration options using synthetic deep/wide trees and the reference host. Include CPU, wall time, and memory; retain a single bounded pool.
3. Implement only measured traversal improvements that preserve every discovered manifest in unchanged fixtures.
4. Re-run the exact M006A no-argument, explicit-root, platform, and cleanup-activity qualifications.

Do not start a second worker pool per root. Do not increase process/thread creation without a fixed measured cap.

## 6. Required tests and verification

- Synthetic deep/wide tree manifest parity and deterministic ordering.
- No symlink following, ignore/unignore precedence, and platform prune policy parity.
- Explicit-root stdout equivalence on unchanged fixtures.
- Linux/macOS/Windows hosted CI and Rust 1.89.
- Release no-argument macOS scan finishes within 120 seconds; record counters, subtree profile, wall time, and memory.
- Confirm no extra Cargo locate/metadata calls for the same discovered user-project set.

## 7. Acceptance and stop conditions

M006C can close only when traversal throughput meets the no-argument bound without reducing the discovered user-project set or weakening the activity/cleanup safety checks.

Stop and report if the remaining cost can only be removed by narrowing writable user-project domains or following symlinks. Present any proposed product-scope change separately for an explicit policy decision.

## 8. Closure evidence

Create `plans/closure/artifact-discovery-cleanup/006c-status.md` with implementation commit, before/after host measurements, subtree attribution, fixture parity, platform/MSRV CI, unresolved findings, and disposition.
