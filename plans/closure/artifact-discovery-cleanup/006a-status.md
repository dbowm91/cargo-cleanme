# Artifact Discovery and Cleanup M006A Closure

Plan: `plans/implementation/artifact-discovery-cleanup/006a-global-scan-pruning-and-path-normalization.md`

Disposition: **conditionally closed**

Implementation commit: `2d2a37f` (`Implement M006 global discovery and proof optimizations`)

Date: 2026-10-03

M006A's policy, traversal, Rustup-pruning, accounting, and source-root path-normalization work is implemented. Its explicit global-scan qualification gate remains unmet, so this record does not claim full closure. The plan's stop condition requires a measured throughput follow-up instead of narrowing writable project-bearing domains; M006C is now **ready**.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Record a fresh baseline and post-policy no-argument macOS release scan | Darwin 25.6, x86_64 reference host. Before policy: `cargo-cleanme --no-progress --stats` terminated by the 120-second timeout (120.03s, exit 124, no final output). After policy: same command also exceeded 120s (exit 124). | Miss; retained as explicit stop condition |
| Replace ad-hoc global pruning with typed platform policy | `policy::tests::macos_global_policy_prunes_protected_usr_and_enumerates_usr_local`; `policy::tests::linux_global_policy_keeps_the_existing_pseudo_filesystem_prunes`; Windows drive enumeration remains in the platform policy. Global-only rules do not affect explicit roots. | Implemented; host CI qualification pending |
| Prune the effective Rustup home only in global discovery | `discovery::tests::rustup_home_uses_absolute_override_or_platform_home_only`; `global_rustup_prune_keeps_adjacent_user_project_reachable` checks category-counter sum and explicit-scope authority. | Pass locally |
| Use one bounded multi-root traversal and deterministic deduplication | `discovery::tests::global_multi_root_walk_deduplicates_equivalent_roots_and_manifests`; one `dua_core::walk_roots` call handles the typed global roots. | Pass locally |
| Keep categorized prune counters coherent | Rustup fixture asserts total pruned directories equal the sum of exclusive prune categories; `--stats` identifies `visited_entries` and each prune category. | Pass locally |
| Normalize source member identity for output exclusion through symlinked ancestors | `workspace::tests::resolution_normalizes_member_roots_through_symlinked_ancestor` and source-activity/output-exclusion fixture coverage. | Pass locally |
| Preserve user/project-bearing global search scope | A 40-second opt-in profile observed 4,203,630 visited entries: `/Users` 3,105,324; `/Library` 438,855; `/Applications` 360,321; `/opt` 181,982; `/private` 117,129. None were pruned; these remain in global scope. | Preserved; explains qualification miss |
| Finish no-argument scan under 120 seconds | Both reference runs exceeded 120 seconds after the allowed OS/tool-managed pruning. | Not met; M006C is ready to investigate throughput without scope narrowing |

## Verification

- `cargo fmt --check` — passed.
- `cargo test --all-targets --all-features` — passed, 163 tests across 4 suites.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `rustup run 1.89.0 cargo test --all-targets --all-features` — passed, 163 tests.
- `rustup run 1.89.0 cargo check --all-targets --all-features` — passed.
- Hosted Linux, macOS, and Windows CI plus MSRV CI — run `37103172868`, pending when this record was written; final outcome is recorded in the follow-up closure commit.

## Unresolved findings and downstream disposition

- No correctness or safety finding remains known in M006A's implemented policy/path work.
- The no-argument scan target remains unmet. This is a performance qualification finding, not a reason to omit user projects from writable/project-bearing domains.
- M006C (`plans/implementation/artifact-discovery-cleanup/006c-global-traversal-throughput-without-scope-narrowing.md`) is unblocked and **ready** based on the measured miss, bounded profile, defined tests, and acceptance criteria. It has no dependency on M006B.
- M006B proceeded after this disposition; C003/C004 safety semantics remain required.
- M005B and earlier milestones are already closed; this evidence does not unblock or reopen them.
