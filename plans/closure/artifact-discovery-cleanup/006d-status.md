# Artifact Discovery and Cleanup M006D Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006d-global-scan-completion-qualification.md`

Disposition: **corrective required** (plan status: conditionally closed)

Implementation commit: `8a9a4df` (`Optimize global scan path and attribution costs`)

Date: 2026-10-03

M006D removed avoidable path materialization from global prune and filter checks, reduced per-entry subtree attribution allocations, and enabled type-only traversal by default on macOS as well as Linux. The exact full-scope scan still did not finish within 120 seconds on the native Apple Silicon host. The plan's stop condition is met: the remaining decision concerns the product's global scan scope/latency contract. No user project domains, hidden trees, or symlinks were silently excluded.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Profile traversal versus manifest validation and finalization | Native arm64 type-only profiling reached about 4.64 million visited entries by 120 seconds; one of two roots had finished. It had 1,800 manifest candidates, all 1,800 passed the existing `symlink_metadata` regular-file/non-symlink revalidation, in about 0.13 seconds. The walker did not complete, so final manifest sort/dedup and report finalization were not reached. | Traversal is the remaining measured phase; finalization is not the cause |
| Compare bounded traversal strategies | Native arm64 and Rosetta runs compared metadata and type-only traversal, parent-first and completion ordering, and worker caps 8 and 16. Type-only and completion ordering reached roughly 4.64 million visits at 120 seconds but did not finish the root walk. Increasing the cap to 16 did not materially improve a 40-second type-only sample. | No compared variant met the time bound |
| Preserve all global project-bearing domains and safety | Production keeps the existing root selection and pruning contract, does not follow symlinks, retains manifest revalidation, filters, deterministic sorting, and the bounded worker pool. The existing synthetic parity test covers worker caps, traversal modes/order, filters, platform pruning, explicit roots, and a symlink to an outside manifest. | Preserved and locally tested |
| Exact unprofiled no-argument native release qualification within 120 seconds | `target/native-arm64/aarch64-apple-darwin/release/cargo-cleanme --no-progress --stats`, under `/usr/bin/time -l gtimeout 120`, returned at 120.03 seconds with no final `--stats` output. The wrapper reported 6.24 seconds user CPU and 61.88 seconds system CPU. The measured invocation did not produce a trustworthy scanner-only peak-memory figure, so none is claimed. | **Miss** |
| Keep user-specific paths out of retained profiling output | Temporary local second-level and home-subtree profiling used to locate the traversal tail was removed before commit. Retained opt-in profiling reports only the existing bounded first-level subtree totals and aggregate completion/manifest counters. | Pass |
| Rust, lint, format, and regression checks | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features` (165 tests), `rustup run 1.89.0 cargo check --locked --all-targets`, `rustup run 1.89.0 cargo test --locked --all-targets` (165 tests), and `git diff --check`. | Pass locally |
| Hosted Linux/macOS/Windows CI and Rust 1.89 | CI run [37109498135](https://github.com/dbowm91/cargo-cleanme/actions/runs/37109498135) passed Linux, macOS, Windows, and Rust 1.89 jobs on closure commit `eb7ef42`. | Pass |

## Matched measurement context

M006C's exact reference run is the closest prior unprofiled baseline: the Darwin x86_64/Rosetta command timed out at 120.03 seconds without final stats (recorded in `006c-status.md`). M006D's exact run used the same macOS installation on the native arm64 executable and likewise timed out at 120.03 seconds. These runs are not a controlled before/after comparison because the process architecture differs. No speedup is claimed. Profiled full-scope runs on both architectures consistently left a root unfinished; type-only traversal did not close the gap.

## Findings and downstream disposition

- The full-scope 120-second target remains unmet despite measured implementation changes. A partial entry count is not reported as a completed scan.
- The residual traversal tail includes user-controlled and system-wide project-bearing locations. Pruning those locations, hidden directories, or following symlinks would change the product's discovery contract and is outside M006D authorization.
- M006D is conditionally closed with corrective work required. M006E is proposed to obtain an explicit scope-versus-latency product decision before implementing a changed default or adding user scope controls.
- M006 performance hardening remains active. M006E is not ready until that policy decision is supplied.
- M005 destructive release qualification and C003/C004 cleanup proof safety are unaffected; no cleanup authorization or proof behavior changed.
