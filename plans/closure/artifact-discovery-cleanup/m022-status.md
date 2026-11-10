# M022 — bounded output claim for an unresolved manifest: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**. No release has carried this
change, and this repository does not call an unpublished milestone closed.
Acceptance of ADR 010 is not a release. This record does not authorize one.

Plan: `plans/implementation/artifact-discovery-cleanup/m022-bounded-output-claim-cleanup.md`
Authority: ADR 010, accepted 2026-11-09, owner-directed by the instruction
to complete this plan. The amendment is in ADR 007 item 4, dated
2026-11-09.
Repository baseline: `main` at `5ad76cf` (published v0.2.13).
Date: 2026-11-09.
Implementation commit: none. The change is in the working tree and has not
been committed.

## Requirement-to-evidence matrix

| Acceptance criterion | Evidence |
|---|---|
| ADR 010 is accepted and ADR 007 item 4 carries the amendment before the work is called complete | `plans/adr/007-accepted-scoped-proof-boundary.md`, amendment 2026-11-09. `plans/adr/010-proposed-bounded-output-claim-for-unresolved-manifests.md` status is accepted and points at that block. The production code was written in the same working tree, before the acceptance block was typed into ADR 007. The block exists now, and this record does not claim the block preceded the first edit. |
| A disjoint conventional pair performs one `cargo clean` of the healthy workspace and none of the failed one. The failed manifest stays discovered | `output_claim_probe` (`src/cleanup.rs:13532`), run by `output_claim_hostile_cases` (`src/cleanup.rs:13493`) in a child with `M022_CLAIM_PROBE=1`, a private `CARGO_HOME`, and the output-placement variables removed. The loop covers `CleanupSelection::All`, `PerLearnedRoot`, and `Ancestry`. Each asserts one clean of the healthy workspace, no scope block, and the broken manifest still in `unresolved_ownership`. `metadata_failure_with_a_disjoint_claim_cleans_the_sibling` is the same contract for the three clean modes. |
| A config `target-dir` redirect holds the healthy unit. Zero cleans | The second case in `output_claim_probe`. `skipped_output_claim` is present. `clean_calls` is empty. |
| An unreadable config blocks the boundary. Zero cleans | `.cargo/config.toml` created as a directory. `scope_blocked` is `OwnershipUnproven`. `clean_calls` is empty. |
| A symlink `target-dir` holds the healthy unit | Unix-only arm in the same probe. `clean_calls` is empty and a row is `skipped_output_claim`. |
| A `target` inside another source tree is not cleaned because of this plan | The shared `broken/src/inside` case. No row is `Cleaned`. `clean_calls` is empty. |
| A claim that changes before the spawn performs no further clean | `ClaimRewriteRunner` writes the redirect on the third metadata call, which is the refresh. `clean_calls` is empty. `scope_blocked` is `OwnershipUnproven`. The message is the claim-change block (`src/cleanup.rs:2904-2909`). A late `CARGO_TARGET_DIR` is still the existing `output_env_matches` block; `environment_change_blocks_remaining_units` covers that path and was not rewritten. |
| `RefusedByCargo` does not block and does not create a claim | `a_refused_manifest_does_not_block_its_learned_root` (`src/cleanup.rs:13409`). Locate refusal is `RefusedByCargo`. One clean. |
| JSON gains `skipped_output_claim` and no other field | `the_new_reason_codes_do_not_add_a_json_field_or_a_log_path` (`src/cleanup.rs:13453`) asserts the sibling code adds no JSON field. `CleanupReasonCode::as_str` (`src/cleanup.rs:820-846`) spells `skipped_output_claim`. `plans/output-schema-v1.md` lists it. The log adds `output_claims=N` only when the count is greater than zero, and the line carries no path (`src/output.rs:413-420`). |
| Exit is 1 when a unit was held | `cleanup_exit_code` (`src/main.rs:896-912`). `an_unproven_learned_root_makes_the_exit_code_non_zero` asserts `SkippedOutputClaim` is exit 1. |
| Docs state the claim, the unbounded block, and the narrowed residual | `docs/USAGE.md` section "Unresolved ownership blocks its own scope". `docs/TROUBLESHOOTING.md` sections on unresolved manifests and the Full per-root skip. `docs/AUTOMATION.md` operational limits. |

The hostile cases were not recorded red against baseline `5ad76cf` before
the engine change. They were written with the implementation. The two
tests that pinned the old whole-boundary block failed on this tree first,
with `scope_blocked == None` where they expected `OwnershipUnproven`, and
were then rewritten to the disjoint-claim contract. That is the safe
direction of the old assertion going red. It is not a separate baseline
run of the six ADR cases.

`output_claim_probe` returns immediately unless `M022_CLAIM_PROBE` is set,
so a direct run of that test name proves nothing. The suite entry is
`output_claim_hostile_cases`, which sets the variable and requires the
child to exit 0.

## Verification commands

Same host and same ladder as `c047-status.md`. One shell, exit 0, after
the exit-code assertions:

| Step | Command | Result |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | clean, after `cargo fmt --all` |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| 3 | `cargo test --all-targets --all-features` | **605 lib + 12 bin + 2 generate-docs + 81 cli_contract + 4 end_to_end + 0 example = 704 passed, 0 failed** |
| 4 | `python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py` | self-test passed; 37 fixture files clean |
| 5 | `python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py` | self-test passed; 17 documents clean |
| 6 | `python3 scripts/check-selector-qualification.py --self-test && python3 scripts/check-selector-qualification.py` | self-test passed; 9 profile-qualified and 2 package-qualified releases agree |
| 7 | `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | 13 artifacts match the clap model |

`cargo` on `PATH` is Homebrew rustc 1.98.1. The claim reader is pinned to
the Cargo 1.89 sources named in ADR 010 item 5.

**MSRV.** `cargo +1.89` is not a command here. The lane was:

| Command | Result |
|---|---|
| `rustup run 1.89 cargo test --locked --all-targets` | **exit 0. 605 lib + 12 bin + 81 cli_contract + 4 end_to_end + 0 example = 702 passed, 0 failed.** `dev-tools` is off. |

**The hosted Linux, macOS, and Windows lanes were not run.** The plan
requires them before closure. The symlink arm is `#[cfg(unix)]`. This
host is macOS and that arm ran inside the local suite. That is not the
hosted matrix.

## Known limitation

An unresolved manifest in learned root A can still point its output at a
path inside learned root B when that path is not one of the directories
the claim reader named. A finite named redirect holds the intersecting
unit. An unnamed or unreadable redirect blocks the boundary that contains
the unknown. The failed workspace's `target` is not deleted. Learned-root
selection is unchanged.

## What deliberately did not change

- No new flag, config key, or JSON field other than the additive
  `reason_code` value.
- `RefusedByCargo` passengers and scope notes do not grow a claim.
- C047's per-root block remains. A finite claim narrows a block. It does
  not restore one boundary over every proven learned root.

## Open

- Publication. The plan stays open until a published release carries it.
- The hosted OS lanes, as above.
- The six ADR hostile cases were not captured red on `5ad76cf` before the
  change, as recorded above.
