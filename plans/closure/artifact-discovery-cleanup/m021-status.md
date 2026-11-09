# M021 — Scan resolution and measurement cost: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**. No release has carried this
change, and this repository does not call an unpublished milestone closed.

Plan: `plans/implementation/artifact-discovery-cleanup/m021-scan-resolution-and-measurement-cost.md`

Repository baseline: `c333501` (published v0.2.13).

Implementation commit: **none.** The change is uncommitted on `main` at
`c333501`. No commit was requested.

## Requirement-to-evidence matrix

| Acceptance criterion | Evidence |
|---|---|
| A workspace whose root was discovered pays one locate, and members metadata names are not located | `n_member_workspace_requires_one_metadata_regardless_of_order` (`src/workspace.rs:3989`) asserts `cargo_locate_calls == 1`, the only locate path is the root manifest, and `cargo_metadata_calls == 1`, for both input orders. `repeated_members_invoke_metadata_once` (`src/workspace.rs:3362`) stays at 2 locates and 1 metadata: neither sibling directory is an ancestor, so both are still located. |
| A nested manifest metadata does not name is still located | `nested_non_member_is_located_when_metadata_does_not_name_it` (`src/workspace.rs:4071`). Locate count is 2. The non-member is located. The members are not. Two workspaces, unresolved empty. |
| A failed metadata result does not cover its members | `metadata_failure_does_not_cover_member_manifests` (`src/workspace.rs:4192`). All three manifests are located and all three stay unresolved at stage `metadata`. |
| Failed sibling locates are all still issued | `thousand_failed_lookups_keep_every_unresolved_manifest` (`src/workspace.rs:3789`) still asserts 1,000 locates. `two_independent_workspaces_require_two_locate_two_metadata` (`src/workspace.rs:4515`) still asserts 2 and 2. The `proof_cargo_locate_calls` assertions in `src/cleanup.rs` were not edited and passed in the full suite. |
| Explicit discovery with the platform `skip_metadata` bit matches metadata-on | `explicit_walk_manifest_and_prune_counts_agree_across_metadata_modes` (`src/discovery.rs:2321`). Nested manifest, `target/` prune, and on Unix a symlink directory and a symlink `Cargo.toml`. Sorted manifests, prune total, and `coverage_complete` agree with the bit off. The bit is `explicit_walk_skips_metadata` (`src/discovery.rs:558`): Linux and macOS skip, Windows does not. `global_walk_knobs` is unchanged. |
| Linux and macOS inventory sizing agrees with `measure_single_target`, including a sparse file. Windows inventory sizing still re-stats | `single_and_batched_target_measures_agree` (`src/traverse.rs:997`) now includes `sparse/hole.bin` created with `File::set_len(1 << 20)`. Entries, bytes, and newest agree. On Unix the test also asserts apparent size is greater than allocated size. `measure_single_target` (`src/traverse.rs:354`) is unchanged and re-stats on every platform. The Windows branch of `measure_many_targets` still calls `symlink_metadata` and `filesize::file_real_size_fast`. |
| Inventory source activity agrees with `workspace_member_activity`, and the proof calls still re-stat | `inventory_source_activity_agrees_with_the_proof_walk` (`src/workspace.rs:6246`): quiet, a recent file, a symlink whose target is the only recent mtime (Unix), a recent file inside a nested repository, and a missing directory. Quiet timestamps are compared. Recent timestamps are not. `cleanup.rs:4087` still calls `traverse::workspace_member_activity`. `workspace_source_activity` calls `inventory_member_roots_activity` (`src/traverse.rs:844`). |
| `RESOLUTION_CONCURRENCY_CAP` is still 4, with the sample recorded, or a higher cap whose matched sample is in this record | Still 4 (`src/workspace.rs:527`). `PROOF_REFRESH_CONCURRENCY_CAP` is still 4 (`src/cleanup.rs:3362`). Neither constant was edited. The sample is below. The cap-8 pair was not run. |
| The ladder and the 1.89 lane were run, and their results are in this record | The command log below. Both exited 0. |
| No cleanup decision, exit code, or JSON field changed | No flag, config key, reason code, exit code, or JSON field was added. `generate-docs --check` reported 13 artifacts matching the clap model. The eggfetch scan printed the same stdout and the same byte count before and after. The allowed counter delta is `locate` 18 → 11. |

`failed_member_first_is_cleared_by_later_authoritative_workspace_metadata`
(`src/workspace.rs:3688`) was rewritten in the same change. It used to
require `cargo_failures == 1` from a member locate. That locate is the
spawn package A removes: the root is the member's ancestor and successful
metadata names the member. The test now asserts `cargo_locate_calls == 1`,
`cargo_failures == 0`, and an empty unresolved ledger.

## Verification commands

Run on this host (macOS), with `~/.cargo/bin/cargo` (rustup). `CARGO_TARGET_DIR`
was unset for the suite and the sample. An earlier `cargo test` inside the
sandbox failed `cleanup::tests::real_cargo_package_selector_preserves_sibling_workspace_artifacts`
because the sandbox's `CARGO_TARGET_DIR` sent `cargo build` outside the
fixture. That test passed once the variable was unset, and it passed again
in the ladder run below. Clippy had one prior failure,
`clippy::io_other_error` in `src/traverse.rs`, fixed before this run.

```text
~/.cargo/bin/cargo fmt --all -- --check
~/.cargo/bin/cargo clippy --all-targets --all-features -- -D warnings
~/.cargo/bin/cargo test --all-targets --all-features
```

Exit 0. Results: lib 594 passed; bin 12 passed; `generate-docs` 2 passed;
`cli_contract` 81 passed; `end_to_end` 4 passed; the traversal example 0 tests.

```text
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-selector-qualification.py --self-test && python3 scripts/check-selector-qualification.py
~/.cargo/bin/cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

Exit 0. Fixture portability: 37 files clean. Citations: 17 documents clean.
Selector qualification: 9 profile-qualified and 2 package-qualified releases
agree. `generate-docs`: 13 artifacts match.

```text
~/.cargo/bin/cargo +1.89 test --locked --all-targets
```

Exit 0. Results: lib 594 passed; bin 12 passed; `cli_contract` 81 passed;
`end_to_end` 4 passed; the traversal example 0 tests. This command does not
pass `--all-features`, so the `generate-docs` target is not in the lane.
`Cargo.lock` was not regenerated.

Not run: `scripts/release-check.sh`. It is the pre-release gate, not the
ordinary ladder, and it needs a clean tree.

## Package E sample

Tree: `/Users/davidbowman/projects/eggfetch`.

Command, both binaries: `--no-progress --stats scan /Users/davidbowman/projects/eggfetch`.
`CARGO_TARGET_DIR` unset. Release builds.

| | Baseline | After, warm |
|---|---|---|
| Binary | `/tmp/cargo-cleanme-m021-baseline` (11,275,476 bytes, built 2026-11-09 21:49 from the pre-change tree) | `/tmp/cargo-cleanme-m021-after` (11,305,188 bytes, built 2026-11-09 22:16 from this tree) |
| stdout | `eggfetch/target  369.57 MiB  [private]` and `369.57 MiB inventory estimate across 1 inactive Cargo output groups` | identical |
| manifests / workspaces / failures / bytes | 18 / 11 / 0 / 387526656 | identical |
| locate calls | 18 | 11 |
| metadata calls | 11 | 11 |
| discovery | 74.37 ms | 57.59 ms |
| locate | 1628.16 ms | 936.58 ms |
| metadata | 1079.79 ms | 1166.00 ms |
| source_activity | 28.61 ms | 14.11 ms |
| output_sizing | 22.26 ms | 5.86 ms |
| tool elapsed | 0.90 s | 0.90 s |
| `/usr/bin/time -p` real | 0.92 | 0.93 |

The phase fields are sums across workers and can exceed wall time. A first
after-run, before the warm row, reported tool elapsed 0.81 s and `time` real
1.69 s; the second run is the one in the table. An earlier baseline from
before this matched pair (locate 1746.88 ms, metadata 1066.86 ms, real 1.88 s)
is the same shape and is not the pair: the two binaries above were run
sequentially in one environment.

`deduped_hits` stayed 7. The seven manifests that are no longer located are
the ones successful metadata already named.

### Cap decision

`RESOLUTION_CONCURRENCY_CAP` stays **4**. Metadata is the largest phase sum
on the after binary, and locate is still close to it. Wall time on this tree
is about one second and moved by hundreds of milliseconds between runs of
the same binary. That is not a sample on which a higher cap is a measured
win, so the cap-8 pair was not run. `PROOF_REFRESH_CONCURRENCY_CAP` was not
touched.

## Platform notes

This host is macOS. The Windows inventory path was not executed here. It
still re-stats: `measure_many_targets` uses `symlink_metadata` and
`filesize::file_real_size_fast`, and on Windows that function calls
`GetCompressedFileSizeW`, which the walker metadata does not. The sparse
file on this APFS temp directory was a hole (`set_len`); apparent size was
greater than allocated size, and both measurement paths agreed. A
seek-and-write at a 1 MiB offset was dense on this volume and was not used.

The vanish-during-walk case for `measure_single_target` was **not built**.
The function re-stats after the walker yields the entry, and there is no
portable hook between those two points. A test that deletes the file in
between would be a race. The plan says to record that rather than claim it.

The symlink-target activity fixture and the symlink discovery fixtures are
`#[cfg(unix)]`. The missing-directory activity case, the nested-repository
case, and the nested-manifest / `target/` prune comparison are not. None of
this was run on Windows.

## Known limitations

The published inline-test census was not re-derived. This change adds four
ungated `#[test]` functions (`nested_non_member_is_located_when_metadata_does_not_name_it`,
`metadata_failure_does_not_cover_member_manifests`,
`explicit_walk_manifest_and_prune_counts_agree_across_metadata_modes`,
`inventory_source_activity_agrees_with_the_proof_walk`). The figures in
`architecture/overview.md` (602 declared / 600 Linux / 547 Windows / 596
macOS) are each four short until the next census pass. They were not
incremented by arithmetic: that is how the census was wrong before.

No stop condition fired. Ancestry schedules locates and is not membership.
Inventory bytes agreed with `measure_single_target` on the stable fixture,
including the sparse file. The activity verdicts agreed on the five
fixtures. The higher cap was not faster because it was not tried, and the
sample does not justify trying it. `global_walk_knobs`, the pre-delete
proof, and the proof-refresh locate elimination were not changed.

## Unresolved findings

None that block this milestone. The census note above is documentation
drift of low severity. It does not change a scan answer.

## Disposition

**closing.** Implemented, ladder-green, and not published. `RESOLUTION_CONCURRENCY_CAP`
stays 4.
