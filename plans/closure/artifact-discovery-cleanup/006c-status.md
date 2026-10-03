# Artifact Discovery and Cleanup M006C Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006c-global-traversal-throughput-without-scope-narrowing.md`

Disposition: **corrective required** (plan status: conditionally closed)

Implementation commit: `bbfff4b` (`Improve bounded global traversal throughput`)

Date: 2026-10-03

M006C landed bounded, fixture-tested traversal improvements without narrowing global scope. The reference Darwin x86_64 no-argument scan still exceeded 120 seconds. This record closes M006C with its acceptance miss visible; M006D is ready to diagnose and qualify the remaining full-scan completion cost. M006 performance hardening remains active.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Compare bounded workers and traversal options | Synthetic deep/wide tree compares worker caps 1, 2, 4, 8, 16, 32; metadata modes; and parent-first/completion order. Every variant yields the identical sorted manifest set and top-level attribution sums exactly to visited entries. Reference-host samples informed an eight-worker cap, retained native metadata mode on macOS, and retained parent-first production order. | Pass for fixture parity; profiling samples are not matched benchmark claims |
| Reduce measured per-entry overhead | Global attribution aggregates by root/component before materializing sorted paths; global event handling avoids unnecessary path construction; VCS names compare as `OsStr`; empty user filters return without path-string conversion. Linux requests type-only enumeration; macOS retains default metadata behavior. | Implemented; performance target remains unmet |
| Preserve scope, filters, symlink policy, counters, and ordering | Synthetic fixture includes hidden project manifests, ignored and unignored archive children, target pruning, and (on Unix) a symlink to an outside manifest. Manifest lists match exactly across all variants; existing platform policy and explicit-scope tests remain in the suite. | Pass locally |
| Complete exact no-argument Darwin release scan in 120 seconds | `target/release/cargo-cleanme --no-progress --stats`, run under `/usr/bin/time -l gtimeout 120`, ended at 120.03s with no final stats. It used 14.06s user CPU, 97.54s system CPU, and reported 59,510,784 bytes maximum RSS. A separate unbounded native-mode run was stopped at 165.86s (13.55s user, 89.54s system, 30,265,344 bytes maximum RSS) without completion. | **Miss** |
| Attribute work by root without pruning project-bearing domains | Bounded parent-first profile at 120s reported approximately 4.14 million visited entries; `/Users` was still progressing, while `/Library` (438,855), `/Applications` (360,321), and `/opt` (181,982) had completed. Completion-order profiling approached 4.63 million entries by 120s but did not emit final stats. `/Users`, `/Library`, `/Applications`, `/private`, `/opt`, and other project-bearing domains remain reachable. | Pass for attribution; scan bound remains missed |
| Linux/macOS/Windows hosted CI and Rust 1.89 | Hosted CI run [37106407982](https://github.com/dbowm91/cargo-cleanme/actions/runs/37106407982) passed Linux, macOS, Windows, and Rust 1.89 jobs on implementation commit `bbfff4b`. Local Rust 1.89.0 tests and check also passed. | Pass |
| Preserve cleanup proof call behavior | M006C does not change Cargo locate/metadata resolution or cleanup proof code. Existing full regression suite passed locally. | Pass locally |

## Verification

- `cargo fmt --check` — passed.
- `cargo test --all-targets --all-features` — passed, 165 tests across 4 suites.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rustup run 1.89.0 cargo test --all-targets --all-features` — passed, 165 tests.
- `rustup run 1.89.0 cargo check --all-targets --all-features` — passed.
- `cargo test synthetic_wide_and_deep_manifest_set_is_stable_across_worker_caps -- --nocapture` — passed across all worker/order/metadata variants.
- `git diff --check` — passed before closure documentation.
- Hosted Linux/macOS/Windows and Rust 1.89 CI — run [37106407982](https://github.com/dbowm91/cargo-cleanme/actions/runs/37106407982), all jobs passed on `bbfff4b`.

## Findings and downstream disposition

- No known correctness or cleanup-safety regression was found in the implemented traversal changes.
- M006C's 120-second full-scan acceptance criterion is unmet. Partial entry counts are not treated as completed scan results.
- M006D (`plans/implementation/artifact-discovery-cleanup/006d-global-scan-completion-qualification.md`) is **ready**. It is unblocked by the exact timeout and near-complete profiles and owns timing traversal versus manifest/report finalization, then qualifying one full-scope improvement.
- No other future implementation plan is blocked on M006C; M005 and C001-C004 remain closed and unchanged.
- The artifact discovery and cleanup subsystem remains active; M006 remains active until a reference-host full scan completes within the bound or the product policy decision is explicitly resolved.
- M005/C001-C004 remain closed and unchanged. M006C did not weaken Cargo proof freshness, cleanup authorization, or fail-closed behavior.
