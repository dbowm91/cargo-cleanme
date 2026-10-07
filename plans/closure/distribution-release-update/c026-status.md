# C026 — Post-v0.2.2 repository reconciliation, cleanup, and polish — status

Status: **closed.**

Plan: `plans/implementation/distribution-release-update/c026-post-v0.2.2-repository-reconciliation-cleanup-and-polish.md`

Repository baseline: `aac5b685e116b67aadda06bacac5788c483b1499`
Implementation branch: `plans/c026-post-v0.2.2-reconciliation-cleanup`

## 1. What this record is, and what it is not

C026 was a **control-surface and repository-hygiene corrective**. It changed no
Rust source, no release bytes, no test, no glob semantics, no updater
behaviour, no yank state, no workflow behaviour, no version, and no
acceptance-matrix cell. `src/` is byte-identical to the baseline;
`Cargo.toml` reads `0.2.2` as it did before. `git diff --stat` for the
reconciliation commit touches documentation only.

What it changed is the **agreement between the planning/architecture record and
the evidence already in it**, plus the set of remote branches the repository
carries. At the baseline, present-tense current-state sentences across the
registry, both subsystem roadmaps, the canonical roadmap, `architecture/`, and
the agent/operator entry points contradicted the accepted v0.2.2 evidence — and
several cited source lines no longer said what the sentence claimed. That drift
is the defect.

Because the change carries no product qualification and no acceptance criterion
that depends on a future event, this is closed rather than conditionally closed.

## 2. Implementation commits

| Commit | Work |
|---|---|
| `129ac21` | the C026 plan itself |
| `aafe234` | register C026 as the post-v0.2.2 handoff |
| `8a6258e` | register C026 in the distribution roadmap |
| `2fd54af` | cross-register C026 in the cleanup roadmap |
| `4f6f1e8` | register C026 in the canonical long-term roadmap |
| `cc40459154f002ea5c7455a85ce16d527498bd1e` | **the reconciliation itself** — work packages B, C, D, F |
| (this record) | formal closure, work package G |

`cc40459` is the substantive commit. The five commits before it registered the
plan; they are cited for completeness rather than because they carried work.

**Repository mutations outside the commit history**, performed during work
package E and recorded here because `git log` cannot show them: four stale
remote branches were deleted (§8).

## 3. Requirements mapped to evidence

Each acceptance criterion in §11 of the plan, in the plan's order.

| Plan §11 criterion | Where satisfied | How it was checked |
|---|---|---|
| Registry C023/M011C conditional state reconciled to closed | `plans/registry.md` §Open work | The two "conditionally closed" paragraphs are replaced by a paragraph stating both closed with the v0.2.2 run id, and explicitly pointing at the two failed runs as failures |
| Registry stale "read this before the next release" block bounded or removed | `plans/registry.md` §Release discipline | Replaced by an eight-item note headed "Release discipline for the next release, derived from the v0.2.2-closed process", labelled a handoff note rather than an open plan. The seven generic v0.2.2-closed items are kept; the four v0.2.0/v0.2.1-specific conditional ones are gone |
| Registry `0.2.1` version prose current | `plans/registry.md` M012A row | Reads `0.2.2`. `Cargo.toml` verified at `0.2.2` |
| Registry published-state yank sentence verified live | `plans/registry.md` §7; §7 below | crates.io rechecked during implementation: 10 versions, only `0.2.0` yanked |
| Canonical roadmap Phase 11 status current | `002-long-term-roadmap.md` Phase 11 | Status is **closed** with the run id; the pre-v0.2.2 conditional paragraph is retained below a dated *Historical* marker |
| Canonical roadmap v0.2.1/current-main prose current | `002-long-term-roadmap.md` Phase 12 | `it is 0.2.2` |
| Canonical roadmap C026 status current, Phase 14 undefined | `002-long-term-roadmap.md` §C026 | Status **closed** with a pointer to this record, plus an explicit statement that C026 is not Phase 14 and authorises nothing |
| Canonical roadmap dependency diagram extended | `002-long-term-roadmap.md` §Dependency | `v0.2.1 -> C024 -> v0.2.2 -> automatic M011C green [37561575727] -> C025 -> C026 -> no accepted milestone` |
| Distribution roadmap stale 12.4 "four outstanding evidences" paragraph corrected | §12.4 | Replaced by what actually happened per milestone; the C025 resolution text is preserved verbatim with a dated later-reconciliation paragraph after it |
| Distribution roadmap C026 status current | §12.6 | Status **closed** with an Outcome paragraph |
| Distribution roadmap header current | header | "closed through v0.2.2"; "No distribution corrective is open"; Phase 14 explicitly not accepted |
| Cleanup roadmap C026 cross-registration current | header and §C026 | Both read closed; §C026 states the audit found no cleanup/discovery defect needing a separate corrective |
| `architecture/overview.md` version/status facts current | `overview.md` header, §3, §7, §8 | See §5 |
| `architecture/overview.md` Phase-11 finding rows reconciled | `overview.md` §7 rows 3-6 | Rows 3-6 now report C018/M011A/M011C/M011B+M011D closed, each naming the shipped work; rows 9, 12, 13 left open with their reasons intact |
| Agent-entry release/yank counts corrected | `AGENTS.md` §What this is | Ten releases; v0.2.0 yanked |
| Agent-entry ADR-003 CLI wording reconciled | `AGENTS.md` §7 | "Cleanup is opt-in" removed — it is not; bare invocation is Routine Execute per ADR 003. The `--dry-run`/`--cargo-preview` split recorded. `update_json` no longer described as untested |
| Operator docs audited | `README.md`, `docs/*.md` | Read; no stale current-state claim found. `docs/TROUBLESHOOTING.md`'s per-version defect tables and `docs/RELEASING.md`'s checklist are correct as written — see §4 |
| Stale branch history proved before retirement | §8 below | Three branches proved 0 unique commits; the fourth proved content-preserved by a 34-path blob comparison |
| Branch deletion not performed without proof | §8 | Deletion happened only after the comparisons were run and recorded |
| Every surviving `file.rs:N` citation re-derived from source | §5 | 20 citations re-derived line by line; each new line was read, not shifted arithmetically |
| `cargo fmt`, `clippy`, `test`, fixture-portability, doc-citation all green | §6 | All five ladder rungs run and green |
| No product behaviour changed | §1 | `git diff --stat` touches documentation only |
| No closed record edited | §4 | None; verified by path list |
| No failed run recorded as passing | §4 | The two failed automatic runs are named as failures in every surface that mentions them |
| No release published or publication advanced | §1 | No tag, no crates.io write, no workflow dispatch |
| Registry returns to no open local implementation plan | `plans/registry.md` §Open work | "No cargo-cleanme-local implementation plan is open" |
| Historical status not restated as current | §4 | C025's 12.4 resolution text is preserved rather than overwritten, and labelled by date |
| A substantive product defect was not absorbed | §9 | Audit found none; §9 records what was examined |

## 4. What was deliberately **not** edited

The distinction that matters most in this record, because getting it wrong would
have made this pass look more thorough than it was while corrupting the evidence
base.

- **Every closure record**, including `c023-status.md` and `m011c-status.md`,
  which C025 had already reconciled correctly. Both already read `closed` with
  the v0.2.1 addendum and the v0.2.2 discharge. Nothing needed changing, and a
  closed record is not edited by a later corrective.
- **C025's 12.4 resolution text** in the distribution roadmap. It accurately
  describes the state on 2026-10-06, which is not the state now. It is left
  verbatim and followed by a dated later-reconciliation paragraph, so a reader
  sees both what C025 fixed and what has happened since.
- **Every run id, result, and conclusion.** Runs `37419183947` and
  `37507738147` are recorded as **failed** at every occurrence; `37561575727` is
  recorded as green on all five lanes. Nothing converts the two failures.
- **`docs/TROUBLESHOOTING.md`'s known-defect tables** and `docs/RELEASING.md`'s
  operator checklist. Both are correct as written, including the "0.2.1 and
  earlier" defect rows and the 0.2.0 yank rows.
- **All `src/`, `tests/`, `completions/`, `man/`, `packaging/`, `release/`,
  `xtask/`, and `.github/workflows/`.** Byte-identical to the baseline.

## 5. Citation re-derivation — the part a checker cannot do

`check-doc-citations.py` verifies that every cited file exists, that the line is
in range, and that a range is ordered. It cannot verify that the line still says
what the sentence claims. **Every citation touched was therefore re-read from
`src/` before being rewritten.** The mechanical gate passed at the baseline
*and* these citations were wrong, which is the concrete demonstration that the
gate is only half the job.

| Location | Was | Now | How it was derived |
|---|---|---|---|
| `overview.md` header | `0.2.0 (unreleased)` | `0.2.2 (published)` | `Cargo.toml:3` |
| §4 step 0 | `run_scan` at `main.rs:302` | `main.rs:485` | `grep -n 'fn run_scan'` |
| §4 steps 1-9 | `main.rs:316,317,338,344,364,403,475,483,535/528` | `main.rs:496,506,527,537,557,596/608,654,662,721/709` | Each step's call site read and confirmed; `reconcile_full` at `:596` and its `publish` at `:608` cited separately |
| §5 mode table | `main.rs:102` and a `Preview`/`Simulate`/`Execute` table where `--dry-run` meant *Cargo preview* and `--dryrun` meant simulation | `CleanMode` at `cleanup.rs:36`, mapping at `cli.rs:224`, Execute-is-default, and the ADR-003 spelling | `grep -n 'enum CleanMode'` and `cli::clean_mode`; the **old table was not just stale but pre-ADR-003 wrong** — it described a CLI that no longer exists |
| §5 roots | `main.rs:113` and `main.rs:269` | `resolve_cleanup_roots` at `main.rs:314`, `collapse_roots` at `main.rs:440` | Function definitions located by name |
| §3 invariant 2 | `cleanup.rs:493-495` | `cleanup.rs:544-546` | The `ownership != PrivateBounded` early return, re-read |
| §7 row 2, row 12 | `cleanup.rs:795` | `cleanup.rs:846` | `if !diagnostics.is_empty()`, re-read |
| §7.2 item 2 | `cleanup.rs:976` / `:979` / `:991-995` | `cleanup.rs:1049` and `:1061-1064` | Re-read. The old citation described a hoisted-universe mechanism that **no longer exists in that form**; the real path is the `ProofFailure` → `skipped` row |
| `15-distribution` §4.4, §9, §10 | `ci.yml:64` | `ci.yml:72` (and `02-cli.md` `64-76` → `72-84`) | `generated-docs:` job key re-read |
| `15-distribution` §2 | "current version is 0.1.6 … nothing has been yanked" | Rewritten around both yanking decisions | crates.io rechecked (§7) |

Verified unchanged and therefore left alone: `domain.rs:186` (`OutputOwnershipClass`),
`cleanup.rs:493-495`'s neighbours, `post-release-smoke.yml:6-7`,
`packaging/tests/test_installers.py:786-791`, `check-release-identity.py:91`,
`check-release-contract.py:527`, `install.sh:305`, `install.ps1:125`.

## 6. Verification actually run

Reported in the order run. Nothing here is claimed that was not executed.

```text
cargo fmt --all -- --check                                      → clean
cargo clippy --all-targets --all-features -- -D warnings         → finished, no warnings
cargo test --all-targets --all-features                          → 358 passed, 0 failed
                                                                 (320 inline + 36 cli_contract + 4 end_to_end)
python3 scripts/check-fixture-portability.py --self-test         → all four rules, both directions
python3 scripts/check-fixture-portability.py                     → 35 fixture files clean
python3 scripts/check-doc-citations.py --self-test               → all rules, both directions
python3 scripts/check-doc-citations.py                           → 16 documents clean
```

The full ladder ran even though C026 changed no Rust. C025's record skipped it
and justified the skip; that was defensible for a registry-only change. C026
touched `architecture/` and `AGENTS.md`, which are consumed by agents and are
therefore operational surface, and the cheap rungs remove a class of "I only
changed docs" surprise. `358 = 320 + 36 + 4` was counted, not assumed; it
matches `overview.md` §3's inline count exactly.

Contract guards beyond the ladder, all run individually:

```text
check-release-contract.py --self-test / run                     → pass / 5 targets, 11 assets, wrappers aligned
check-installer-contract.py --self-test / run                    → pass / 5 targets reachable, install names match
check-release-identity.py --self-test                            → pass
check-selector-qualification.py --self-test / run                → pass / 9 profile + 2 package-qualified agree
check-post-release-smoke-contract.py --self-test / run           → pass / 5 targets match runners
check-staged-validation-contract.py --self-test / run            → pass / read-only upstream-bound enforcement point
resolve-smoke-transition.py --self-test                          → pass
verify-release-attestation.py --self-test                        → pass
gen-release-workflow-shape.py --check                            → matches derived inputs (6135 bytes)
packaging/tests/test_installers.py --self-test                   → every premise guard rejects a broken setup
generate-docs -- --check                                         → 13 artifacts match the clap model
```

**Deliberately not run, and why:**

- `scripts/release-check.sh`. Per `AGENTS.md` §3 it requires a clean tree, the
  `eggpack` binary, and toolchain 1.89, and is explicitly not routine
  verification. Every documentation gate it invokes was run individually above.
- Hosted CI. Cannot be run locally; §10 records what is pending on it.

## 7. External state rechecked rather than asserted

The plan forbade repeating any yank or release claim unless it was rechecked
during implementation. Two were rechecked live on **2026-10-07**.

- **crates.io API** `cargo-cleanme/versions` → `total: 10`. Exactly one yanked:
  **`0.2.0`**. `0.1.0`–`0.1.6`, `0.2.1`, `0.2.2` all `yanked: false`. This
  confirms `AGENTS.md` ("ten releases, v0.2.0 yanked") and the rewritten §2 of
  `15-distribution-and-release.md`.
- **GitHub branches API** → six branches before work package E, two after.
  Full inventory in §8.

## 8. Work package E — branch proof and retirement

The plan required proof of preservation **before** deletion, and explicitly
prohibited deleting a branch because its commit message resembled a mainline
commit. Three of four branches have **zero** unique commits; the fourth does not,
and was proved by content comparison rather than by resemblance.

### Proof

| Branch | Head | vs `origin/main` | Verdict |
|---|---|---|---|
| `c019-unignore-containment` | `9b4df98b` | 92 behind, **0 ahead** | Every commit reachable from `main`. No unique work. |
| `c020-windows-config-race` | `fcc9eee1` | 86 behind, **0 ahead** | Same. |
| `c021-pre-release-evidence-reconciliation` | `f3d0d9c8` | 61 behind, **0 ahead** | Same. |
| `phase11-hardening` | `877a3f29` | 101 behind, **1 ahead** | **One unique commit.** Needed real proof. |

`git cherry origin/main origin/phase11-hardening` reports `+`, meaning the patch
text is not in `main`. That is exactly the case the plan warned about, so the
commit's 34 touched paths were compared blob-for-blob between the branch head and
its mainline counterpart `c4481540692e848cc88bdf57d48404b0ff794130` (verified an
ancestor of `origin/main`):

```text
git diff --stat 877a3f2 c448154 -- <34 paths>
 → 32 of 34 paths byte-identical
 → plans/registry.md                          (+11 −11)
 → plans/subsystems/artifact-discovery-cleanup-roadmap.md   (+53)
```

The two residuals were read in full. They are **concurrent C019 registration
text that `main` carries and the branch does not**: `main`'s version registers
C019 as the current handoff and moves M011D to a later-corrected status, while
the branch predates that. In other words `main` is a **strict superset** of the
branch on both paths. Nothing the branch holds is absent from `main`; the
difference is that `main` additionally has newer, correct text.

This is the distinction the plan drew, and it is why the comparison had to be
per-path: the headline `git cherry` signal was `+` — "not in main" — and a
reviewer who stopped there would either have refused to retire a stale branch or
retired it anyway on a hunch. Neither would have been evidence.

### Inventory

Before (2026-10-07T03:41:28Z):

```text
c019-unignore-containment                    9b4df98bd0571a1e60306dd23665b94b1f8076b9
c020-windows-config-race                     fcc9eee1d4ffee455a2f073904938cdf7f5498b4
c021-pre-release-evidence-reconciliation     f3d0d9c8c435063bb67940b8dd1c95605141e0e7
main                                        aac5b685e116b67aadda06bacac5788c483b1499
phase11-hardening                            877a3f29567c9f4290314943406cfd43b6c15054
plans/c026-post-v0.2.2-reconciliation-cleanup 4f6f1e8f65ad68c9651ad9bc2aec4953eeb39b1f
```

After: `main` and `plans/c026-post-v0.2.2-reconciliation-cleanup`. Four deleted.

`origin/implementation/artifact-discovery-cleanup` was already gone from the
remote before this work and was pruned on `git fetch --prune`; its PRs (#1, #2)
are merged, so it was previously retired, not by this plan.

**Local branches were left in place.** The plan mandated remote retirement and
the local pointers are harmless. Deleting `phase11-hardening` locally would
remove the only local reference to `877a3f2`, which — despite being content-
preserved — would become garbage-collectible. Keeping it costs nothing and
preserves the commit object for as long as the working copy exists.

## 9. Discrepancies found and how they were classified

The audit did not find a product defect. It found a large volume of
documentation drift, which is what it was chartered to find.

| # | Discrepancy | Classification | Resolution |
|---|---|---|---|
| 9.1 | Registry described C023 as conditionally closed and M011C as a conditional evidence dependency, after both were discharged by v0.2.2 | **stale current state** | Corrected, with the two failed runs still recorded as failures |
| 9.2 | Registry's "read this before the next release" block was built on v0.2.0/v0.2.1-specific conditions that no longer apply | **stale current state, wrong genre** | Replaced with a bounded release-discipline note. A checklist of retired conditions is not a handoff note |
| 9.3 | Distribution roadmap §12.4 said four hosted evidences were outstanding and named "the first release published from `main` after 2026-10-05" | **stale current state** | Replaced by what actually happened; C025's own resolution text preserved with a dated follow-on |
| 9.4 | `architecture/overview.md` header said `0.2.0 (unreleased)` | **stale status field** | `0.2.2 (published)` |
| 9.5 | `overview.md` §4/§5/§7 citations pointed at lines that no longer say what the sentences claimed | **semantic citation rot** | All 20 re-derived from source (§5). The mechanical gate passed throughout; this is why it is only half the job |
| 9.6 | `overview.md` §5 mode table described `--dry-run` as Cargo preview and `--dryrun` as simulation — the pre-ADR-003 CLI | **superseded design documented as current** | Rewritten against `CleanMode` and `cli::clean_mode`. This was the most consequential single find: an agent reading the overview would have implemented the wrong safety mode |
| 9.7 | `overview.md` §7 rows 3-6 said "ready"/"Open hardening" for plans that shipped months of releases ago | **stale status** | Closed against shipped work |
| 9.8 | `overview.md` §8 said "2 accepted ADRs" when ADR 003 exists | **count drift** | 3, with ADR 003 added and its load-bearing role stated |
| 9.9 | `overview.md` said `scripts/` held 13 checkers and 3 release workflows; the repo has 17 scripts and 6 workflows | **count drift** | Corrected |
| 9.10 | `15-distribution-and-release.md` §2 said "current version is 0.1.6 … nothing has been yanked" | **stale, and contradicted by a yank that had happened** | Rewritten around both decisions, including why 0.2.0's case differs in kind from 0.1.1/0.1.2 |
| 9.11 | `15-distribution-and-release.md` §8.2 asserted in present tense that the rehearsal is manual-only and "a defect of the C016 or C017 class can ship again" | **current-state claim contradicted by §8.3 and §9 of the same document** | §8.2 explicitly reframed as historical, with new §8.2.1 recording the three-run scoreboard |
| 9.12 | `15-distribution-and-release.md` had §8.2 then §8.4, with no §8.3 | **numbering defect** (pre-existing) | Automation section renumbered to §8.3; cross-refs updated |
| 9.13 | `15-distribution-and-release.md` checklist items 12-13 said the rehearsal is `workflow_dispatch`-only and must be dispatched manually | **stale** | Corrected, with a new item on the predecessor-installability failure |
| 9.14 | `AGENTS.md` said "cleanup is opt-in" | **contradicted by ADR 003 since 0.2.0** | Corrected. This one had operational consequence: it told agents the opposite of the shipped safety design |
| 9.15 | `AGENTS.md` said `update_json` is "the only untested surface" | **stale; C022 added tests** | Corrected, with the migration cost preserved |
| 9.16 | `AGENTS.md` said eight releases, none yanked | **stale** | Ten releases, 0.2.0 yanked |
| 9.17 | `overview.md` said `cleanup.rs` has 71 inline tests in one place (registry did) | **count drift** | Registry corrected to 72, verified against `src/` |
| 9.18 | `docs/`, `README.md` carried no stale current-state claim | **no defect** | Audited and left unchanged — see §4 |

Two of these deserve comment because they are the same bug class as the seven
milestones this repository already knows about.

**9.6 and 9.14 are one defect wearing two hats**: ADR 003 changed the front door,
and both the architecture overview and the agent entry point still described the
old one. Neither is a citation and no checker reads either. An agent obeying
`overview.md` §5 would have chosen `CargoPreview` where `Simulate` is the safe
mode; an agent obeying `AGENTS.md` §7 would have believed cleanup required
consent. This is exactly the "documentation is operational surface" case, and it
is why the full test ladder ran for a documentation-only change.

**9.5 is the strongest available evidence that the citation gate is half a
gate.** `check-doc-citations.py` reported 16 documents clean at the baseline,
while §5's mode table documented a CLI that had been removed, and §7.2
described a mechanism (`hoisted.get()?`) that no longer existed. The gate is
correct about what it checks and the plan was right to require re-derivation
rather than arithmetic shifting. §5 above is the record of the re-derivation.

## 10. Unresolved findings and hand-offs

**No corrective is required.** Nothing found needed a separate plan, and nothing
is carried forward to a future milestone.

Two items are deliberately left as they are, with the reasoning recorded so a
future reader does not "fix" them by accident:

1. **Independent signing / SLSA build provenance is still not in place.** Row 4
   of `overview.md` §7 says "closed for the planned scope" and names what was
   deliberately deferred. M011A closed immutable releases plus GitHub-issued
   attestation; the trust root is still GitHub's release infrastructure, and a
   compromise of the release account still yields a self-consistent artefact
   every check accepts. This is an open trust-model decision requiring a plan,
   not documentation drift — it was open before C026 and remains open after.
2. **The symlinked-output-root diagnostic divergence (row 12) and the
   uncertainty-threshold asymmetry (row 13)** remain open design observations.
   Both are recorded with the reason they were not simply fixed: row 12 would
   widen a fail-closed block condition, and row 13 is arguably intentional.

`C010` remains `proposed`, upstream, and non-blocking, exactly as before.

## 11. Disposition

**C026 is closed.** Every acceptance criterion in §11 of the plan is satisfied.
The verification in §6 was executed, not assumed. The branch retirements in §8
were preceded by the proof that §8 records. The closure is unconditional because
nothing in it depends on a future event: this pass changed no product behaviour
and no acceptance criterion waits on a release.

**What the registry looks like afterwards, stated plainly: no cargo-cleanme-local
implementation plan is open.** The product and release line is closed through
v0.2.2 and reconciled. `C010` is upstream and non-blocking. **No Phase 14
milestone has been accepted and none was activated by this pass.**

That last point is the one most likely to be quietly undone by whoever reads
this next. A repository with an empty registry and a clean tree looks like a
repository waiting for work, and the cheapest way to fill it is to pick the next
phase from the roadmap's own hints — shared-cache GC, provenance work, selector
expansion. None of those has research behind it. Idleness here is the correct
end state, and it is a decision, not a gap.