# Artifact Discovery and Cleanup M006F Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006f-exhaustive-traversal-hot-path-qualification.md`

Disposition: **conditionally closed**

Implementation commit: recorded in the enclosing M006F/M007 implementation commit.

Date: 2026-10-03

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Staged dua-core callback harness | `examples/traversal-qualification.rs` runs Full roots/prunes with entry counting, prune counters, manifest recognition, batched progress, attribution, and production discovery. | Pass |
| Remove attribution from ordinary hot path | Attribution HashMaps and final map materialization are created only for `--stats` or explicit profiling. Regression test verifies empty attribution and result/counter parity when disabled. | Pass |
| Qualify delivery order | Matched release eight-worker samples returned identical sorted manifest sets (1,803), with ParentFirst around 16.9–17.1s and Completion around 17.0–17.1s. No material Completion gain; retain ParentFirst. | Pass |
| Revalidate worker count | Matched stage-1 native samples: 4 workers 25.00s; 8 workers 16.92s; 16 workers 33.24s. Keep the existing bounded eight-worker cap. | Pass on reference host |
| Record Full baseline and host wait evidence | Production discovery stage 6: 17.56s, 4,670,246 entries, 1,803 manifests. `/usr/bin/time -lp` reported 10.32s user, 69.76s system, maximum RSS 174,776,320 bytes. The full CLI inventory invocation took 97.59s, completed traversal, but exited 1 due diagnostics and did not publish M006E state. | Traversal baseline complete; reconciliation remains M006E qualification |
| Preserve exhaustive scope and safety | Harness uses platform Full roots/prunes. Existing discovery parity tests and complete suite pass locally; no production scope narrowing or cleanup proof change was introduced. | Pass locally |
| Rust, lint, format, and MSRV | `rtk cargo fmt --all`; `rtk cargo test --all-targets --all-features` (168 passed across five suites); `rtk cargo clippy --all-targets --all-features -- -D warnings`; `rtk rustup run 1.89.0 cargo check --locked --all-targets --examples`; `rtk rustup run 1.89.0 cargo test --locked --all-targets` (164 library tests plus binary/integration targets); `rtk git diff --check`. | Pass locally |

## Matched callback stage samples

Native release runs used the same Full roots, type-only metadata mode, and eight workers. ParentFirst/Completion stage pairs were: stage 1 count-only 16.92/17.02s; stage 2 prune counters 17.03/16.77s; stage 3 manifest recognition/sort 17.09/17.08s; stage 4 batched progress 16.89/17.06s; stage 5 attribution 16.81/16.86s. Production discovery (stage 6, ParentFirst) took 17.56s. Each is a single run and is reported as a qualification sample, not a statistical speedup claim.

## Verification commands

Hosted Linux/macOS/Windows CI has not yet run for this commit and remains the named qualification before this milestone can be marked closed.

## Limitations and findings

- The native Full scan completed traversal but Cargo resolution produced 283 failures and 741 diagnostics; no complete state generation was published. This is tracked in M006E closure, not hidden as a completed reconciliation.
- Hosted Linux/macOS/Windows evidence is required before release qualification.
