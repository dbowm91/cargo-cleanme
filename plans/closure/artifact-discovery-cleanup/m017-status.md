# M017 Status — Scoped Proof Default (ADR 007 implementation)

Plan: `plans/implementation/artifact-discovery-cleanup/m017-scoped-proof-default.md`
(status active, owner-directed, ADR 007 accepted).

Disposition: **conditionally closed.** Implementation, local verification,
live-shape fixtures, and hosted OS/MSRV lanes are complete. Three items
remain open, all release-time or external rather than code: (a) publication
of a release carrying it (tree at 0.2.9, unpublished; the flag and islands
axis were removed before any publication, so nothing shipped needs
migrating); (b) the ADR 004 item (e) distinct release gate, transferred to
M017's release; (c) hosted `cli_contract`/`end_to_end` evidence, which did
not execute on hosted lanes because `cargo test` stops scheduling further
targets behind the carried pre-existing lib failure (same structural
situation as M015/M016). Per planning-and-closure, a milestone not yet
carried by a published release stays open — hence conditional, not closed.

Implementation (PR #14 branch
`plans/m015-proof-island-certificate-and-partition-report`, atop the M016
stack):

- `4b1fa29` feat: boundary rule in `resolve_cleanup_scope` + refresh
  re-application + per-dispatch env re-verification, `--partitioned`
  removal, `islands` axis removal, boundary-scoped exits + zero-unit
  fail-closed edge, render/contract updates, test rewrite, ADR 007,
  ADR 001 + 000 amendments, M017 plan, docs/changelog/regen.
- `904fc17` fix(lint): doc-comment blank-line the hosted 1.99 clippy
  rejects (local toolchain predates the lint — the lane caught what local
  could not, recorded below).
- `0bf1b95` fix(test): component-wise manifest comparisons for macOS
  symlinked TMPDIR and Windows separators (two new tests caught by the
  macOS/Windows lanes; engine correct on all lanes, test spelling wrong).

## Requirement-to-evidence map (plan §Contract + §Engine + §Tests)

| Requirement | Evidence |
|---|---|
| Boundary rule: out-of-boundary never blocks; in-boundary MergeAll blocks; passengers/notes reported | `split_unresolved_for_boundary` + `boundary_admits` + `blocking_unresolved_count`; early return only on blocking; out-of-scope becomes `Info`/`out_of_scope` diagnostics; `refresh_combined_universe` re-applies the split against fresh workspaces |
| Zero-unit edge fails closed | Engine + listing block when zero units are proven with any unproven participant; unselected units do not count (M014 elsewhere case stays exit 0; empty scope stays exit 0) |
| Env re-verified per dispatch | Unconditional per-unit check (M016 proved per-island; retained without islands); child-process probe `environment_change_blocks_remaining_units` |
| No flag, no islands | `--partitioned` gone from clap/`CleanupRequest`/threading/docs/completions/man; `IslandV1`/`CleanupV1.islands` gone; key-set test back to 14 keys, deliberately |
| Human + log contract | Rows + block as before; new bounded out-of-scope line on both paths; reported-note listing on unblocked runs; log line unchanged (counts only) |
| New discriminating tests | `out_of_anchor_poison_does_not_block_anchor` (lib, engine level) + `out_of_anchor_poison_does_not_block_bare_routine_cleanup` (subprocess, CWD anchor end to end) — both fail pre-change as a whole-scope block; `empty_proof_universe_blocks_listing` + engine zero-unit edge; rewritten G1/G1c/Simulate/human/`-i` surfaces (exit 0, note named, no islands) |
| Preserved hostile controls | Redirect/nested skip without blocking; metadata-failure blocks; symlinked-output unit absent while sibling cleans; mid-run growth blocks remainder; gate-8 per-unit refusal; passenger-redirect never cleans alone; audit of every old whole-scope-block test (each kept only if its poison is in-boundary) |

## Verification actually run

| Command or run | Result |
|---|---|
| `cargo fmt --all --check` | Passed on final tree. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Clean locally (1.89 toolchain); hosted 1.99 lane caught one `empty-line-after-doc-comments` miss, fixed in `904fc17`. Local toolchain cannot see that lint — recorded so the next agent does not trust a local green alone. |
| `cargo test --all-targets --all-features` (lib, stable Linux) | 447 passed, 1 failed: the known pre-existing `real_temp_project_preview_and_execute_with_redirected_target` (host cargo outside the profile-selector allowlist). Verified identical on pristine HEAD via `git stash` + rerun: pre-existing, unrelated, needs its own corrective. |
| `cargo test --all-features --test cli_contract` | 62 passed with `--test-threads=4` (61 pre-existing + rewritten + 1 new flagship). At full parallelism one disk-heavy real-cargo test flakes with ENOSPC: this machine's disk is 100% full (868G/915G used, unrelated to the repo). Passes in isolation; full-suite green at reduced parallelism. Environmental, not product. |
| `cargo test --all-features --test end_to_end` | 4 passed. |
| `python3 scripts/check-fixture-portability.py --self-test` + run | Self-test passed; 37 files clean. |
| `python3 scripts/check-doc-citations.py --self-test` + run | Self-test passed; 17 documents clean. |
| `git diff --check` | Clean (`git diff --check -- .`; bare `git diff --check` exits 2 under the rtk proxy with no output — tooling quirk, not a finding). |
| `generate-docs --check` | 13 artifacts match the clap model. |
| Hosted lanes, PR #14 head `0bf1b95` (run `38976428485`) | `checks` lib: **ubuntu 447+1, macOS 444+1, Windows 429+1** — the single failure on every OS is the same pre-existing toolchain-drift test; every M017-owned test green on all three OSes. `msrv` green. `benchmark`, `generated-docs`, `installers` (3 OSes), Eggpack drift + contract, selector Supported + Exploratory: all green. `cli_contract`/`end_to_end` did not execute on hosted (cargo stops scheduling targets behind the lib failure). |
| `scripts/release-check.sh` | Not run (requires clean tree + `eggpack` + 1.89; release-time gate, outstanding above). |

## In-flight findings (fixed pre-closure)

- **Zero-unit gap (design, found by test audit):** the first M017 cut let a
  fully-unresolvable scope exit 0 with zero units (the M012A silent
  no-op). Fixed before any lane ran: zero proven units plus any unproven
  participant fails closed with the standard block; unselected units do not
  count. ADR 007 item 5 and the M017 plan record the rule. Three old
  whole-scope tests that looked broken by M017 (`json_scope_block`,
  `log_mode_scope_block`, `bare_cleanup_with_unresolved_ownership`) pass
  unchanged under it — the rule distinguishes "nothing proven" from
  "healthy units proceed".
- **Two lane-caught test defects (test-side, no product change):** hosted
  1.99 clippy's `empty-line-after-doc-comments` (local toolchain lacks the
  lint); macOS symlinked-TMPDIR + Windows separator path comparisons in
  two new assertions (the M015 lesson, re-learned once). Engine correct on
  all lanes in both cases.

## Known limitations

- The one red test on every lane is carried, not fixed: the profile-selector
  allowlist does not qualify host stable cargo. Own corrective still
  needed; it also suppresses hosted `cli_contract`/`end_to_end` execution
  (cargo stops scheduling targets behind the lib failure), so hosted
  integration evidence for M017 is outstanding through no fault of M017.
- No publication yet: 0.2.9 carries the change in-tree; item (e) release
  gate runs at release time. No release authorized here.
- Machine surface for out-of-boundary poison is the `summary.diagnostics`
  count; per-manifest paths are visible via `scan --all` (whose diagnostics
  array carries them) and the human report carries only the bounded count
  line, no paths. Deliberate (log-line/ASCII discipline), recorded so a
  future machine consumer knows where paths live.
- H-2 rediscovery and refresh discovery stay combined-scope (mid-run change
  detection is not poison reporting): a mid-run manifest-set change or
  discovery failure blocks even if out-of-boundary. Documented residual,
  same class as C035's.

## Unresolved findings by severity

- **Low (test-env/toolchain drift, pre-existing, carried):** the
  allowlist/host-cargo mismatch; own corrective still needed.
- **Low (M017-owned, found and fixed pre-closure):** the 1.99-only clippy
  lint and the two path-comparison test defects (`904fc17`, `0bf1b95`).
- **None medium or higher.** Per the corrective-pass rule, any future
  material defect gets a new plan, not an edit to this record.

## Deferred

- The out-of-boundary `diagnostics` array on the cleanup JSON (paths on the
  machine surface) was considered and declined for M017: the count plus the
  human line plus `scan --all` coverage is sufficient, and a new array is
  a contract addition needing its own decision.
- In-scope exclusion (`clean` a subtree while ignoring a nested manifest)
  stays rejected for the C035 reason: excluding a manifest from discovery
  cannot exclude it from owning bytes.
