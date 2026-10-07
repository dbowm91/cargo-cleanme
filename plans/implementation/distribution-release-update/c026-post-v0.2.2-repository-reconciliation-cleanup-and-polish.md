# C026 — Post-v0.2.2 Repository Reconciliation, Branch Cleanup, and Polish

Status: ready

Repository baseline: `aac5b685e116b67aadda06bacac5788c483b1499`

Source roadmaps:

- `plans/subsystems/distribution-release-update-roadmap.md`
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`
- `plans/002-long-term-roadmap.md`

Primary class: corrective planning/documentation/repository-hygiene pass

Hard dependencies: none.

Product/release dependency: none. v0.2.2 is already published, immutable, attested,
and qualified. C026 does not authorize another release and does not define Phase
14 product work.

Historical plans/records whose closed facts this pass must propagate correctly:

- C023 — `c023-v0.2.0-destructive-safety-patch-release-corrective.md` /
  `plans/closure/distribution-release-update/c023-status.md`;
- C024 — `c024-windows-glob-canonicalization-corrective.md` /
  `plans/closure/distribution-release-update/c024-status.md`;
- C025 — `c025-post-v0.2.1-planning-status-reconciliation.md` /
  `plans/closure/distribution-release-update/c025-status.md`;
- M011C — `011c-published-release-smoke-automation.md` /
  `plans/closure/distribution-release-update/m011c-status.md`;
- v0.2.2 release receipt —
  `plans/closure/distribution-release-update/r022-status.md`.

Closed plans and closure records remain historical evidence. C026 corrects
forward from them; it does not rewrite their original verdicts or run results.

## 1. Objective

Reconcile the repository after the v0.2.2 publication so every *current-state*
surface describes one coherent truth, retire stale development branches only
after their unique history is proven preserved, and leave a clean planning and
architecture baseline from which a future Phase 14 can be researched.

The durable current state C026 must converge on is:

- `Cargo.toml` is `0.2.2`;
- v0.2.2 is published on GitHub and crates.io, immutable and attested;
- v0.2.0 remains yanked and its historical destructive defects remain disclosed;
- v0.2.1 carries the two C023 destructive-safety fixes;
- C023 is **closed** because its final operational criterion was discharged by
  the v0.2.2 automatic post-release smoke;
- M011C is **closed** because run `37561575727` was green automatically on all
  five lanes with `from_version=v0.2.1`;
- C024 is **closed and shipped in v0.2.2**;
- C025 is closed as the earlier post-v0.2.1 control-surface reconciliation;
- there is no open cargo-cleanme product corrective other than this bounded
  repository-reconciliation pass;
- C010 remains merely `proposed`, upstream, and non-blocking;
- no Phase 14 product milestone has been accepted yet.

The pass is complete only when an agent entering through `AGENTS.md`,
`architecture/overview.md`, `plans/registry.md`, either subsystem roadmap,
or the canonical long-term roadmap cannot reasonably infer a contradictory
present-tense repository state.

## 2. Defects and debt being corrected

### 2.1 Registry current-state contradictions

The top of `plans/registry.md` correctly records v0.2.2, C023/M011C closure,
and no open implementation plan, while lower current-state prose still says or
implies:

- C023 is conditionally closed;
- M011C is still a conditional evidence dependency;
- a future release is needed for the first green automatic smoke;
- current `main` is 0.2.1;
- C024 is closed but unreleased.

Those statements were accurate before v0.2.2 but are no longer valid as active
handoff text.

### 2.2 Canonical roadmap contains mixed pre- and post-v0.2.2 status

`plans/002-long-term-roadmap.md` correctly contains later reconciliation text,
but Phase 11/13 opening status and some exit/sequence prose still describe
M011C/C023 as conditional or future work.

Historical blocks may remain when clearly marked as historical. Active section
status, dependency summaries, exit conditions, and "what remains" prose must
describe the current state.

### 2.3 Distribution roadmap still carries stale current-state paragraphs

The distribution roadmap has later text closing M011C, but also retains active
status/dependency wording such as:

- C018 "published-release evidence outstanding";
- M011C evidence marked outstanding in Phase 13 dependency diagrams;
- future-publication language after the required publication has happened.

This is distinct from preserving a pre-implementation plan body as history.

### 2.4 Architecture overview is semantically stale

`architecture/overview.md` still labels Phase-11 findings such as C018,
M011A, M011B/M011D, and M011C as open hardening even though implementation and
release evidence have closed them.

The existing citation checker can prove a cited line exists; it cannot prove a
sentence still expresses the repository's current state. C026 must therefore
perform a semantic architecture audit, not only a line-range audit.

### 2.5 Entry-point/operator documentation contains stale repository facts

At baseline, `AGENTS.md` still reports an older public-release count and says
none are yanked. It also contains historical CLI wording that must be checked
against ADR 003 and the v0.2.x public surface.

C026 must audit entry-point/operator documents for volatile current-state facts,
including at minimum:

- `AGENTS.md`;
- `README.md`;
- `docs/INSTALLING.md`;
- `docs/RELEASING.md`;
- `docs/TROUBLESHOOTING.md`;
- `docs/USAGE.md`;
- `docs/AUTOMATION.md`;
- `architecture/overview.md`;
- `architecture/15-distribution-and-release.md`.

Historical defect tables and release-specific receipts must continue to name
the release they actually describe.

### 2.6 Stale remote branches obscure the real development state

Current remote branches include:

- `c019-unignore-containment`;
- `c020-windows-config-race`;
- `c021-pre-release-evidence-reconciliation`;
- `phase11-hardening`;
- `main`.

At the C026 baseline:

- `c019-unignore-containment` is 0 commits ahead / 92 behind `main`;
- `c020-windows-config-race` is 0 ahead / 86 behind;
- `c021-pre-release-evidence-reconciliation` is 0 ahead / 61 behind;
- `phase11-hardening` is 1 ahead / 101 behind.

The first three are already proven to have no unique commit history by the
GitHub compare result. The Phase-11 branch requires an additional proof:
its unique head `877a3f29567c9f4290314943406cfd43b6c15054` has the same
closure commit message as the mainline commit
`c4481540692e848cc88bdf57d48404b0ff794130`, but identical messages alone do
not establish patch equivalence. Its unique diff must be shown to be preserved
or superseded on `main` before deletion.

### 2.7 The repository has reached a roadmap boundary but does not state it cleanly everywhere

The accepted roadmap has no M014/Phase 14 product milestone. C026 must leave
that boundary explicit:

- current objectives are closed through Phase 13/v0.2.2;
- C026 is cleanup of the completed line, not Phase 14;
- future shared-cache GC, stronger provenance/SBOM/SLSA work, or other feature
  expansion requires separate research and planning;
- no future feature is silently activated merely because the repository is
  otherwise idle.

## 3. Why previous verification missed this

This corrective exists because the previous checks validated *local truth* but
did not define a repository-wide current-state closure invariant.

1. C025 intentionally reconciled the repository to the **2026-10-06 v0.2.1**
   state. Its closure is not wrong; v0.2.2 happened later.
2. The v0.2.2 release closure correctly updated the high-value release/control
   surfaces needed to close C023/M011C, but it did not perform an exhaustive
   semantic sweep of every lower current-state paragraph, architecture finding,
   entry-point document, and branch reference.
3. `check-doc-citations.py` proves mechanical citation validity, not semantic
   freshness. A stale statement can point to a valid line and remain green.
4. Release/workflow contract guards correctly test workflows and release shape;
   they do not assert that prose stops saying "next release" after that release
   occurs.
5. Branch hygiene is not represented by a source-tree test. A merged or
   superseded branch can remain visible indefinitely while every CI job is green.
6. Historical and active text intentionally coexist. Without an explicit
   classification pass, a sentence preserved as accurate history can be
   mistaken for a present-tense handoff, or present-tense prose can accidentally
   survive beside a later correction.

The regression evidence for C026 is therefore a combination of explicit
surface inventory, classification, machine checks for mechanically decidable
facts, and branch ancestry/equivalence evidence. Do not invent a brittle
release-version linter solely to make this plan green.

## 4. Invariants

C026 must preserve all of the following:

- no Rust product behavior changes;
- no cleanup/discovery/update semantics change;
- ADR 001 ownership/freshness proof remains untouched;
- ADR 003 CLI semantics remain untouched;
- JSON schema and exit-code contracts remain untouched;
- release assets, tags, attestations, and crates.io yank state remain untouched;
- closed plans and closure records remain historical evidence;
- failed historical workflow runs remain failed;
- v0.2.0 remains immutable/yanked rather than rewritten;
- C010 remains proposed/non-blocking unless independent upstream evidence
  changes it;
- branch deletion never substitutes for proving history is preserved;
- no force-push is used to "clean up" old branches;
- C026 does not define or activate Phase 14.

Any implementation discovery that violates one of these boundaries requires a
separate corrective/product plan.

## 5. Work package A — Build a current-state contradiction inventory

Before editing, enumerate every current-state reference to the release and
closure facts that changed with v0.2.2.

At minimum search for:

- `0.2.1`, `0.2.2`, `current main`, `current release`, `next release`;
- `conditionally closed`, `conditional evidence`, `outstanding`;
- `C023`, `C024`, `C025`, `M011C`;
- `first green automatic`, `automatic smoke`;
- `unreleased`, `not yet published`, `future publication`;
- old release-count/yank statements;
- "open hardening", "ready", or "active" references to closed Phase-11 work.

Classify each hit as one of:

1. **historical immutable context** — retain;
2. **historical but ambiguous** — retain and label/date more clearly;
3. **active/current and stale** — correct;
4. **future acceptance criterion still genuinely future** — retain;
5. **new defect/contradiction requiring a separate plan** — stop and register
   separately.

The closure record must include this classification inventory or a compact
machine-readable/search-derived equivalent.

## 6. Work package B — Reconcile the active registry

Make `plans/registry.md` a compact current control surface again.

Required changes:

- register C026 as the sole `ready` cargo-cleanme-local implementation plan;
- keep C010 `proposed` and explicitly non-blocking;
- remove active/current prose that still treats C023 or M011C as conditional;
- correct the M012A row/current-version statement to `0.2.2`;
- replace the stale "read this before the next release" block with a bounded
  release note derived from the v0.2.2-closed process, or remove it if it no
  longer belongs in the compact registry;
- state C024 as closed and shipped, not closed/unreleased;
- preserve the historical v0.2.0/v0.2.1 failure facts where they are relevant
  to future release discipline;
- do not copy detailed C026 requirements into the registry.

After C026 closure, the registry should return to no open local implementation
plan unless implementation discovers and separately registers a new defect.

## 7. Work package C — Reconcile canonical and subsystem roadmaps

### 7.1 Long-term roadmap

Correct only demonstrated current-state contradictions in
`plans/002-long-term-roadmap.md`.

Required end state:

- Phase 11 status is closed, including M011C's real v0.2.2 evidence;
- Phase 12 remains closed;
- Phase 13 remains historical publication/corrective work and is fully closed
  through v0.2.2;
- current dependency/exit prose no longer says a future release is required to
  close M011C/C023;
- pre-v0.2.2 blocks retained as history are explicitly recognizable as such;
- no Phase 14 milestone is invented.

### 7.2 Distribution roadmap

Reconcile current status/dependency sections so they agree with:

- C018 closed with release evidence;
- M011A/B/C/D closed;
- C023 closed;
- C024 closed and shipped in v0.2.2;
- C025 closed;
- r022 is the release receipt that discharged the final M011C/C023 condition;
- no active distribution corrective remains except C026's documentation/hygiene
  registration while this pass is open.

Historical plan bodies may remain unchanged when explicitly marked historical.

### 7.3 Artifact-discovery roadmap

Audit only the current-state/terminal sections affected by C023/C024/v0.2.2.
Do not churn closed milestone history merely for stylistic consistency.

## 8. Work package D — Reconcile architecture and agent entry points

### 8.1 Architecture overview

Re-read every finding/status row in `architecture/overview.md` against current
code and closure evidence.

At minimum:

- convert C018/M011A/M011B/M011C/M011D rows from "open hardening" to their real
  terminal states;
- ensure findings intentionally retained as open design observations are not
  accidentally closed just because neighboring Phase-11 work closed;
- refresh module/test/script counts only from source evidence, using the
  overview as the count authority;
- re-derive any source line citations touched by edits rather than shifting
  line numbers arithmetically.

If the audit finds a real medium-or-higher implementation defect, stop that item
and open a separate corrective; do not bury a product fix inside C026.

### 8.2 AGENTS.md

Correct volatile repository facts:

- published release count/range;
- v0.2.0 yank state;
- current CLI semantics where ADR 003 superseded earlier wording;
- any stale "already decided" line that conflicts with current canonical
  invocation.

Keep the file an agent entry point rather than turning it into a release log.

### 8.3 Deep dives and user/operator docs

Audit the minimum set in §2.5 for current-state language. Update only stale
present-tense facts, generated examples that genuinely changed, and citations
invalidated by the edits.

Do not rewrite historical known-defect tables or release receipts into present
tense.

## 9. Work package E — Branch-history reconciliation and safe retirement

Treat remote branch deletion as a repository mutation with explicit proof.

For each stale branch:

1. record branch head SHA;
2. confirm there is no open PR targeting/from it;
3. compare it with current `main`;
4. classify every unique commit as:
   - already reachable from `main`;
   - patch-equivalent/cherry-picked on `main`;
   - intentionally superseded with permanent plan/closure evidence;
   - genuinely unique and therefore **not deletable** yet;
5. delete only after the first three safe classes are proven.

Baseline expectations:

- `c019-unignore-containment`: 0 commits ahead — eligible after recheck;
- `c020-windows-config-race`: 0 ahead — eligible after recheck;
- `c021-pre-release-evidence-reconciliation`: 0 ahead — eligible after
  recheck;
- `phase11-hardening`: one unique commit
  `877a3f29567c9f4290314943406cfd43b6c15054`; compare it against mainline
  `c4481540692e848cc88bdf57d48404b0ff794130` and the current tree. The branch
  is deletable only if its unique content is proven preserved/superseded.

Do not:

- force-update an old branch to make it compare cleanly;
- merge a stale branch solely to eliminate divergence;
- delete a branch because its commit message resembles a mainline commit;
- delete the active C026 branch as part of its own implementation. Its normal
  merge/cleanup happens after the plan is accepted.

Record before/after branch inventory in the closure evidence.

## 10. Work package F — Polish current documentation without expanding scope

After factual reconciliation, perform a narrow consistency/polish pass:

- normalize status vocabulary to the normative planning terms;
- remove duplicated current-state paragraphs when a compact pointer to a
  closure record is sufficient;
- ensure "current", "next", and "future" statements have an unambiguous temporal
  referent;
- keep version-specific historical narrative version-specific;
- make current release/support statements derive from one nearby authority
  rather than multiple manually synchronized copies where practical;
- repair stale counts/citations found while touching a document.

Do not perform broad prose rewrites, reformatting, or terminology changes
unrelated to the v0.2.2/current-state cleanup.

## 11. Failure, contention, and cancellation semantics

C026 has no runtime concurrency changes. Repository-operation semantics are:

- if `main` advances materially while reconciliation is in progress, rebase or
  refresh the evidence inventory before claiming closure;
- if a stale branch gains new commits or a PR after the baseline audit, stop
  deletion and reclassify it;
- if a branch comparison is inconclusive, preserve the branch;
- if live GitHub/crates.io release facts disagree with r022, stop and open a
  release-state corrective rather than editing prose to the nicer answer;
- if current-state edits expose a real product defect, register a separate
  corrective and keep C026 focused on reconciliation;
- a cancelled/failed hosted check is not green evidence.

## 12. Compatibility and configuration effects

None expected.

C026 must not change:

- CLI parsing or aliases;
- config schema/defaults;
- learned discovery state schema;
- cleanup policy;
- update protocol;
- machine-readable output schema;
- release target matrix.

If implementation requires any of those, C026 is too broad and must stop.

## 13. Required verification

Because this pass is documentation/planning/repository hygiene, verification
must prove both "no product behavior changed" and that the repository's
machine-checked documentation/release contracts remain valid.

Run the ordinary repository ladder on the final tree:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
~~~

Also run the relevant release/control guards:

~~~text
python3 scripts/check-release-contract.py --self-test
python3 scripts/check-release-contract.py
python3 scripts/check-post-release-smoke-contract.py --self-test
python3 scripts/check-post-release-smoke-contract.py
python3 scripts/check-staged-validation-contract.py --self-test
python3 scripts/check-staged-validation-contract.py
python3 scripts/check-selector-qualification.py --self-test
python3 scripts/check-selector-qualification.py
bash scripts/qualify-cargo-selectors.sh --self-test
cargo run --quiet --features dev-tools --bin generate-docs -- --check
~~~

If `scripts/release-check.sh` is runnable on the implementation host with its
documented prerequisites, run it on a clean final tree. If it is not runnable,
record the missing prerequisite explicitly and require hosted CI plus the
release-drift guard to be green instead of claiming the local release gate ran.

Required repository-state checks:

- final `git diff --name-only` contains no unintended `src/`, workflow,
  packaging, release policy, or generated CLI artifact change;
- no open PR is associated with a branch proposed for deletion;
- branch compare/equivalence evidence is recorded before deletion;
- current branch inventory is recorded after deletion;
- hosted CI and release-drift guard triggered by the final push are green.

## 14. Documentation updates

Expected edits may include:

- `plans/registry.md`;
- `plans/002-long-term-roadmap.md`;
- both subsystem roadmaps where current status is stale;
- `architecture/overview.md`;
- affected architecture deep dives;
- `AGENTS.md`;
- current user/operator docs identified by the audit.

Create the closure record at:

- `plans/closure/distribution-release-update/c026-status.md`.

Closed historical implementation plans and closure records are not edited merely
to make their old status read like today's status.

## 15. Non-goals

C026 does not:

- implement Phase 14;
- add a new cleanup feature;
- change destructive eligibility or ownership proof;
- change scanner performance;
- change Cargo selector support;
- add shared-cache GC;
- add SBOM/SLSA/independent signing;
- modify Eggpack or Eggup;
- publish v0.2.3;
- alter crates.io yank state;
- rewrite v0.2.0/v0.2.1 failed evidence;
- edit closed plans/closure records to erase their historical state;
- delete a branch with unpreserved unique work;
- add a new semantic-doc linter unless implementation demonstrates a
  mechanically decidable recurring defect worth guarding.

## 16. Acceptance criteria

C026 closes only when all are true:

1. `Cargo.toml = 0.2.2` and every active/current repository status surface
   agrees that v0.2.2 is the current published release.
2. No active/current registry text calls C023 or M011C conditionally closed.
3. No active/current text says the first green automatic post-release smoke is
   still future; run `37561575727` is the named discharge evidence.
4. C024 is consistently described as closed and shipped in v0.2.2.
5. C010 remains proposed and non-blocking.
6. Phase 11/12/13 current statuses agree across the long-term and subsystem
   roadmaps.
7. `architecture/overview.md` no longer marks already-closed Phase-11 work as
   open, while genuinely open observations remain open.
8. `AGENTS.md` no longer reports the pre-v0.2.x release/yank state and its CLI
   safety summary agrees with ADR 003.
9. Historical release/failure evidence remains intact and clearly historical.
10. The contradiction inventory has no unexplained active/current stale hit.
11. `c019-unignore-containment`, `c020-windows-config-race`, and
    `c021-pre-release-evidence-reconciliation` are retired after re-proving
    zero unique commits.
12. `phase11-hardening` is retired only after its unique commit is proven
    content-preserved or superseded on main; otherwise it remains and the
    closure record explains why.
13. No product-code/config/schema/release-workflow behavior changed under C026.
14. The ordinary verification ladder and all relevant documentation/release
    contract guards pass.
15. Final hosted CI and release-drift are green.
16. A C026 closure record maps every acceptance criterion to evidence and
    classifies unresolved findings by severity.
17. After closure, the registry has no open local implementation plan unless
    C026 discovered and separately registered a new corrective.
18. No Phase 14 feature direction is implied or activated by the cleanup.

## 17. Stop conditions

Stop the affected work package and open/register a separate plan if the audit
finds:

- a real cleanup/discovery/update correctness defect;
- a medium-or-higher security/safety issue;
- GitHub release/attestation state inconsistent with r022;
- crates.io state inconsistent with the recorded 0.2.0 yank / 0.2.2 publication;
- an unpreserved unique stale-branch commit;
- a need to alter an accepted ADR or machine-readable schema;
- a need to change Eggpack/Eggup;
- a substantive Phase 14 product decision.

Do not resolve factual disagreement by editing the evidence source or deleting
the branch that contains it.

## 18. Closure evidence

`plans/closure/distribution-release-update/c026-status.md` must record:

- implementation branch, baseline, and final head;
- complete current-state contradiction inventory and disposition;
- every current-state file changed and why;
- explicit list of historical statements deliberately retained;
- final registry/roadmap Phase-11/12/13 status;
- architecture findings reconciled and any still-open observations;
- AGENTS/user-doc volatile facts corrected;
- pre-deletion branch heads;
- compare/ahead/behind results for every retired branch;
- patch/content-equivalence evidence for `phase11-hardening` or the reason it
  was retained;
- post-cleanup branch inventory;
- exact verification commands actually run and their results;
- hosted CI/release-drift run IDs and conclusions;
- unresolved findings by severity;
- final disposition using the normative vocabulary.

C026 is complete when the repository has one coherent present tense, preserved
historical evidence, and no stale development line masquerading as active work.
It deliberately leaves the next product milestone undefined.
