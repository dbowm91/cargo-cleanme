# C047 — a Full-cleanup unknown blocks its learned root: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**. No release has carried this
change, and this repository does not call an unpublished milestone closed.
This record does not authorize a release.

Plan: `plans/implementation/artifact-discovery-cleanup/c047-full-cleanup-per-learned-root-block.md`
Corrects: M018 (`plans/closure/artifact-discovery-cleanup/m018-status.md`).
Repository baseline: `main` at `5ad76cf` (published v0.2.13).
Date: 2026-11-09.
Implementation commit: none. The change is in the working tree and has not
been committed.

## Requirement-to-evidence matrix

| Acceptance criterion | Evidence |
|---|---|
| A blocking unknown on `--full` blocks only the proven learned roots that contain it, and the healthy root is cleaned once | `an_unbounded_unknown_blocks_only_its_learned_root` (`src/cleanup.rs:13186`). One `cargo clean` of the healthy root. The poisoned root's row is `skipped_root_blocked`. |
| The blocked root stays discovered, and a shared output with it is not cleaned | `a_shared_output_with_a_blocked_learned_root_is_not_cleaned` (`src/cleanup.rs:13242`). Zero cleans. The shared unit is not private. |
| A pathless diagnostic blocks the root whose walk failed, and not the sibling | `a_pathless_diagnostic_blocks_only_the_root_whose_walk_failed` (`src/cleanup.rs:13280`), Unix, `SystemCleanupRunner`, Simulate. |
| A manifest inside two overlapping retained roots blocks both | `a_manifest_inside_overlapping_learned_roots_blocks_both` (`src/cleanup.rs:13321`). |
| Every proven root blocked is the existing scope block, zero rows, zero spawns | `every_blocked_learned_root_is_a_scope_block` (`src/cleanup.rs:13362`). |
| An explicit root containing the same poison is still one boundary | `an_explicit_root_with_an_unbounded_unknown_still_blocks` (`src/cleanup.rs:13391`). `CleanupSelection::All`. Scope block, zero spawns. |
| `RefusedByCargo` inside a proven root does not block that root | `a_refused_manifest_does_not_block_its_learned_root` (`src/cleanup.rs:13409`). One clean. No `skipped_root_blocked` row. |
| An unattributable discovery diagnostic blocks the whole run | `an_unattributable_discovery_diagnostic_is_a_whole_run_block` (`src/cleanup.rs:13439`). Discovery count 1 and empty walk ranges return `Err`. |
| Exit is 1 when any proven root was blocked | `cleanup_exit_code` (`src/main.rs:896-912`) returns 1 when `roots_blocked > 0` or a row is `SkippedRootBlocked`. `an_unproven_learned_root_makes_the_exit_code_non_zero` (`src/main.rs:2269`) asserts both. |
| JSON gains the reason code and no other field. The log gains a count and no path | `the_new_reason_codes_do_not_add_a_json_field_or_a_log_path` (`src/cleanup.rs:13453`). `roots_blocked=1` is on the log line. The path is not. JSON has no `roots_blocked` field. The unit's `reason_code` is `skipped_root_blocked`. |
| The troubleshooting doc states the cross-root residual | `docs/TROUBLESHOOTING.md`, section "A Full cleanup skips one proven learned root and cleans the others". |

The package-1 fixture was not recorded red against baseline `5ad76cf`
before the engine change. The tests and the engine landed in one working
tree. Two existing tests were pinning the old whole-boundary block and
went red on this tree before they were rewritten:
`metadata_failure_blocks_boundary_with_zero_spawns` (now
`metadata_failure_with_a_disjoint_claim_cleans_the_sibling`) and
`listing_refuses_blocked_boundary` (now
`listing_offers_the_sibling_of_a_finite_claim_and_refuses_an_unbounded_one`).
That red is the M022 contract, not a separate C047 baseline run. The C047
spawn counts above are asserted on the implemented tree.

## Verification commands

Run on this host (macOS). `cargo` on `PATH` is Homebrew rustc 1.98.1.
`CARGO_TARGET_DIR` was set by the sandbox to its cargo-target cache. The
ladder uses that cache. The real-Cargo package-selector fixture strips
`CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR`, and `CARGO_BUILD_BUILD_DIR`
on its own `cargo build`, because the cleanup runner strips the same
variables and the assertion path is `workspace/target`.

| Step | Command | Result |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | clean, after `cargo fmt --all` |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| 3 | `cargo test --all-targets --all-features` | **605 lib + 12 bin + 2 generate-docs + 81 cli_contract + 4 end_to_end + 0 example = 704 passed, 0 failed** |
| 4 | `python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py` | self-test passed; 37 fixture files clean |
| 5 | `python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py` | self-test passed; 17 documents clean |
| 6 | `python3 scripts/check-selector-qualification.py --self-test && python3 scripts/check-selector-qualification.py` | self-test passed; 9 profile-qualified and 2 package-qualified releases agree |
| 7 | `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | 13 artifacts match the clap model |

The seven steps were one shell, exit 0, after the exit-code assertions
were added to `an_unproven_learned_root_makes_the_exit_code_non_zero`.

**MSRV, outside the ladder** (`AGENTS.md` §3.1). `cargo +1.89` is not a
command on this host because `PATH` cargo is Homebrew, not rustup. The
lane was:

| Command | Result |
|---|---|
| `rustup run 1.89 cargo test --locked --all-targets` | **exit 0. 605 lib + 12 bin + 81 cli_contract + 4 end_to_end + 0 example = 702 passed, 0 failed.** `dev-tools` is off, so the generate-docs tests are absent. |

**The hosted Linux, macOS, and Windows lanes were not run.** The plan
requires them before closure because the containment check is the one
macOS symlinked temp roots have bitten before. This host is macOS and the
local suite passed. That is not the hosted matrix.

## Known limitation

An unresolved manifest in learned root A can still have configured its
output at a path inside learned root B. Resolved cross-root claims still
force `Shared`. C047 does not clean on source-tree disjointness. M022
narrows the residual only where the claim is a finite named directory:
that redirect holds the intersecting unit. An unnamed or unreadable
redirect still blocks the boundary that contains the unknown.

## What deliberately did not change

- No new flag, config key, or learned-state field.
- Explicit `ROOT` and the maintenance anchor stay one boundary.
- `RefusedByCargo` does not block and does not grow a claim.
- Discovery still includes the blocked root. A shared output stays shared.

## Open

- Publication. The plan stays open until a published release carries it.
- The hosted OS lanes, as above.
