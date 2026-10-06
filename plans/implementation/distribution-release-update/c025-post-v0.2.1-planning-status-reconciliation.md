# C025 — Post-v0.2.1 Planning and Status Reconciliation

Status: ready

Repository baseline: `35905c5841effadcdb32e3ad4b5f396e4e9096ef`

Corrects planning/control-surface drift following:

- C023 — v0.2.0 destructive-safety patch-release corrective;
- M013 — v0.2.0 publication and Phase 11 operational closure;
- M011C — published-release smoke automation;
- the v0.2.1 publication and subsequent smoke-resolver correction.

Source roadmaps:

- `plans/subsystems/distribution-release-update-roadmap.md`
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`
- `plans/002-long-term-roadmap.md`

Primary class: planning/documentation reconciliation only

Hard dependencies: none.

Parallel substantive handoff:

- C024 — Windows glob canonicalization corrective remains **open / ready** and
  may proceed in parallel. C025 must not change C024's matching semantics.

## 1. Objective

Make every active planning/control surface describe the same repository state
after v0.2.1 without rewriting historical evidence.

The durable current state is:

- v0.2.0 is published, immutable, affected by two destructive defects, and
  yanked on crates.io;
- v0.2.1 is published, immutable, attested, and is the current safe/recommended
  release;
- the published v0.2.1 binary passes both C023 destructive safety fixtures;
- C023 is **conditionally closed**, not a clean pass;
- C023 acceptance criterion 11 — a green automatic
  `release: published` five-target smoke — is **not met**;
- the automatic v0.2.1 smoke did fire unprompted but failed on all five lanes
  because it selected the newly yanked v0.2.0 predecessor;
- the smoke resolver is fixed and self-tested to skip yanked predecessors;
- a manual five-target rehearsal from v0.1.6 -> v0.2.1 is green;
- M011C therefore remains **conditionally closed** pending a future release
  whose automatic smoke is observed green;
- C024 is the only active product corrective identified by C023;
- current `main` CI and release-drift are green.

C025 exists because several summaries contradict that evidence even though the
underlying closure record mostly states it correctly.

## 2. Current contradictions to reconcile

### 2.1 Registry distribution row falsely discharges M011C

`plans/registry.md` currently says the distribution subsystem:

> recorded an observed-green automatic five-lane `release: published` smoke

and says M011C's conditional closure is discharged.

The same registry later correctly states that the automatic v0.2.1 run failed
on all five lanes and that no green automatic v0.2.1 run exists.

The subsystem row is false and must be corrected.

### 2.2 Registry next-release banner falsely says the automatic smoke is green

The current "Read this before the next release" section says:

> v0.2.1 ... automatic five-lane `release: published` smoke is green

while item 5 immediately below still requires a green automatic M011C smoke.

The banner must instead state:

- the automatic trigger is proven to fire;
- the v0.2.1 automatic run failed;
- the resolver defect is fixed on `main`;
- the next release is the first opportunity to obtain the missing automatic
  green evidence.

### 2.3 Distribution roadmap header repeats the false automatic-green claim

`plans/subsystems/distribution-release-update-roadmap.md` currently says C023
recorded an observed-green automatic five-lane smoke and discharged M011C.

That must match the C023 closure record: automatic fired-and-failed, manual
five-target recovery green, M011C still conditional.

### 2.4 Historical C023 sections still say "ready / immediate"

Both subsystem roadmaps retain C023 milestone sections with a pre-implementation
status such as `ready / immediate`.

Historical narrative may remain, but the section status must point to the
accepted closure record and say **conditionally closed**.

Do not delete the original plan or turn the section into a new implementation
plan.

### 2.5 Canonical long-term roadmap remains pre-publication in several places

`plans/002-long-term-roadmap.md` still contains language such as:

- M011C's automatic trigger being carried into C023/v0.2.1 as future work;
- Phase 13 being "published with corrective required — C023 ready";
- C023 being "ready / immediate";
- a future path through v0.2.1 publication/yank as though those events have not
  happened.

Those passages must be reconciled to actual evidence:

- v0.2.1 publication is complete;
- C023 is conditionally closed;
- M011C remains conditional;
- C024 is the active product corrective;
- the next release owns the first opportunity for a green automatic smoke.

### 2.6 C023 closure header and reconciliation section contradict its disposition

`plans/closure/distribution-release-update/c023-status.md` currently has:

- header: `Status: closed`;
- disposition: "**Closed, with acceptance criterion 11 explicitly not met.**"
  followed by an explicit statement that the record claims **conditional
  closure**;
- reconciliation §13 falsely says "M011C upgraded to a confirmed-green
  automatic trigger."

The header must use the repository's normative vocabulary:
**conditionally closed**.

§13 must say M011C remains conditional and that the automatic v0.2.1 run fired
but failed.

Do not alter the run IDs, failure history, or acceptance matrix.

### 2.7 C023 finding counts are inconsistent

The C023 header says "three findings recorded rather than fixed in §11."

§11 actually contains five numbered findings:

1. Windows glob canonicalization no-op -> C024;
2. unreachable post-rewrite glob re-validation loop;
3. production updater staging-dir exclusivity untested;
4. main.rs progress-clear ordering untested;
5. config fallback partial-file removal untested.

C025 must count from the actual §11 headings and reconcile every summary that
names a number.

Only C024 is currently an open medium corrective. The other recorded findings
must not silently become release blockers or active implementation plans unless
their existing severity/evidence warrants opening one separately.

### 2.8 M011C closure record lacks the v0.2.1 addendum

`plans/closure/distribution-release-update/m011c-status.md` ends with the
v0.2.0 automatic failure/manual recovery evidence.

Append a v0.2.1 addendum that records:

- automatic run `37507738147` fired without operator action;
- all five lanes failed because crates.io v0.2.0 was yanked;
- resolver root cause: predecessor selection consulted GitHub release
  suitability but not crates.io installability;
- resolver fix commit `cfdc910afc732a50353158cd0ca6bf074b9ff29d`;
- associated self-test correction
  `b0b41fcb34da0a0bece4f7763a9781a3c1123a2f`;
- manual rehearsal run `37508262225` green on all five targets for
  v0.1.6 -> v0.2.1;
- no green automatic run for v0.2.1 exists or can be recreated;
- disposition remains **conditionally closed** until a future publication
  produces an observed-green automatic run.

Do not upgrade the milestone merely because the trigger itself was proven.

## 3. Authoritative evidence order

When wording conflicts, C025 must derive current status from evidence in this
order:

1. immutable GitHub release/tag/publication facts;
2. named GitHub Actions runs and their actual conclusions;
3. C023 acceptance matrix and disposition;
4. M011C closure record plus the new v0.2.1 addendum;
5. active registry;
6. subsystem and long-term roadmap summaries.

A summary sentence must never override a failed named run.

## 4. Work package A — Reconcile C023 closure record

Update
`plans/closure/distribution-release-update/c023-status.md` without rewriting
history.

Required changes:

- header status -> **conditionally closed**;
- finding count -> exact count derived from §11;
- §13 M011C bullet -> conditional, not confirmed-green;
- disposition stays explicit that criterion 11 is not met;
- preserve all publication, attestation, safety-fixture, yank, and failure
  evidence;
- preserve the distinction between automatic failed run and manual green
  rehearsal;
- preserve C024 as the one opened medium corrective.

Add a short "C025 reconciliation" note only if needed to explain why the summary
changed after the closure commit. Do not fabricate new evidence.

## 5. Work package B — Append v0.2.1 evidence to M011C

Append a dated v0.2.1 operational-evidence section to
`plans/closure/distribution-release-update/m011c-status.md`.

The resulting status must distinguish three independent properties:

1. automatic trigger wiring: **proven**;
2. five-target product/update transaction: **green via manual recovery**;
3. green five-target run under the automatic trigger: **not yet proven**.

The disposition remains conditionally closed because property 3 is the named
qualification from the original milestone.

The next release is an operational dependency, not a reason to create a new
M011C implementation plan.

## 6. Work package C — Reconcile the active registry

Make `plans/registry.md` once again a compact authoritative control surface.

Required state:

### Subsystems

Artifact discovery and cleanup:

- current release safety line closed through v0.2.1;
- current substantive handoff: C024;
- no destructive-safety blocker from C023 remains.

Distribution/release/update:

- C023 conditionally closed;
- no active release corrective;
- M011C conditional evidence remains;
- next release must produce a green automatic smoke to discharge it.

### Open work

State in this order:

1. C025 — planning/status reconciliation (**ready** while this plan is open);
2. C024 — Windows glob canonicalization (**open / ready**, substantive);
3. M011C — conditional evidence dependency on the next release.

After C025 closure, remove C025 from active work and leave C024 as the next
implementation handoff.

### Next-release note

Correct the false automatic-green statement.

The note must say the next release should:

- run from a green full hosted baseline;
- use the existing immutable release pipeline;
- automatically trigger M011C;
- require the resolver to choose an installable, non-yanked predecessor;
- record a green automatic five-target run before M011C is upgraded.

### Published state

State:

- nine public GitHub releases through v0.2.1;
- v0.2.0 yanked on crates.io;
- v0.2.1 current and not yanked;
- v0.2.0 immutable historical evidence remains untouched.

Do not say "every other version is not yanked" unless the current crates.io
evidence is actually rechecked during implementation; prefer the bounded facts
needed for this repository state.

## 7. Work package D — Reconcile both subsystem roadmaps

### Distribution roadmap

Correct:

- top-level status;
- C023 section status;
- M011C current evidence;
- Phase 13/C023 terminal state;
- any text saying automatic-green evidence already exists.

End state:

- C023 conditionally closed;
- v0.2.1 published/attested/safety-qualified;
- v0.2.0 yanked;
- M011C conditional;
- C024 is not distribution behavior, but is cross-listed because it was opened
  by C023 and currently lives in this plan directory.

### Artifact discovery/cleanup roadmap

Correct:

- C023 section status from ready -> conditionally closed;
- terminal/current-state prose that still calls C023 active;
- active handoff -> C024;
- retain the exact destructive-safety invariants C023 restored.

Do not move C024 between directories as part of a status reconciliation.

## 8. Work package E — Reconcile the canonical long-term roadmap

Update only status/order prose made stale by actual publication.

Required current sequence:

~~~text
Phase 12 implementation [closed]
        |
        v
C021/C022 release-readiness [closed]
        |
        v
M013 v0.2.0 publication [historical; corrected by C023]
        |
        v
C023 v0.2.1 safety patch [conditionally closed]
        |
        +--> published/immutable/attested v0.2.1 [done]
        +--> v0.2.0 crates.io yank [done]
        +--> public safety fixtures [done]
        +--> automatic M011C trigger fired [done]
        +--> automatic M011C green result [outstanding]
        |
        v
C024 Windows glob semantics [ready]
~~~

Remove future-tense text that says v0.2.1 still needs to be tagged/published or
0.2.0 still needs to be yanked.

Do not change canonical product behavior.

## 9. Work package F — Reconcile version/current-release prose

Audit planning/control documents for stale current-version statements such as:

- "main is at 0.2.0";
- "0.2.0 staging is unblocked";
- "the next release is 0.2.1";
- "C023 is ready";
- "automatic smoke is green."

For every hit, classify it as:

- historical statement whose date/context should remain;
- stale current-state statement to correct;
- deliberate future acceptance criterion that should remain future tense.

Do not globally search/replace version strings. Historical release evidence must
continue naming the version it actually exercised.

## 10. Work package G — Close C025 with a consistency check

Create:

- `plans/closure/distribution-release-update/c025-status.md`.

Before closure, mechanically/search-audit the active surfaces for contradictory
status phrases.

At minimum inspect:

~~~text
plans/registry.md
plans/002-long-term-roadmap.md
plans/subsystems/distribution-release-update-roadmap.md
plans/subsystems/artifact-discovery-cleanup-roadmap.md
plans/closure/distribution-release-update/c023-status.md
plans/closure/distribution-release-update/m011c-status.md
plans/implementation/distribution-release-update/c024-windows-glob-canonicalization-corrective.md
Cargo.toml
CHANGELOG.md
~~~

Required invariants after reconciliation:

- no active/current sentence calls C023 ready;
- no active/current sentence claims a green automatic v0.2.1 smoke;
- no active/current sentence says M011C is fully discharged;
- no active/current sentence says v0.2.1 is still unpublished;
- no active/current sentence says current `main` is 0.2.0;
- C024 remains ready/open;
- historical run outcomes remain unchanged.

## 11. Tests and verification

C025 changes planning/docs only, but the repository has machine-enforced
documentation contracts.

At minimum:

~~~text
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test
python3 scripts/check-release-contract.py
python3 scripts/check-post-release-smoke-contract.py --self-test
python3 scripts/check-post-release-smoke-contract.py

bash scripts/release-check.sh
~~~

If any edited planning document is consumed by another repository checker, run
that checker too.

Hosted CI/release-drift should remain green if a documentation edit triggers
them. C025 does not require a new publication, staged draft, attestation, or
network smoke.

## 12. Non-goals

C025 does not:

- change Rust source;
- change glob semantics;
- implement C024;
- change the smoke resolver beyond documenting its already-landed fix;
- publish a release;
- un-yank v0.2.0;
- upgrade M011C to closed;
- reopen M013;
- rewrite failed automatic smoke runs as passing;
- create plans for C023 findings 11.2-11.5 unless a separate severity/design
  review justifies doing so;
- fix the staged-validator "2 installers" summary arithmetic unless it is
  independently assigned to a separate implementation corrective.

## 13. Acceptance criteria

C025 closes only when:

- C023's closure header says conditionally closed and agrees with its
  disposition;
- C023's finding count agrees with §11;
- C023 §13 no longer falsely upgrades M011C;
- M011C contains the v0.2.1 automatic-failure/manual-green addendum and remains
  conditionally closed;
- registry subsystem rows, open-work text, next-release note, and published
  state agree with the evidence;
- distribution roadmap no longer claims observed-green automatic v0.2.1 smoke;
- artifact roadmap no longer marks C023 ready/current;
- long-term roadmap treats v0.2.1 publication and v0.2.0 yank as completed;
- C024 remains the next substantive product corrective;
- current release/version prose reflects `Cargo.toml = 0.2.1`;
- all historical failed run IDs/results remain intact;
- doc/release/post-smoke contract guards pass;
- no contradictory active status remains in the audited surfaces.

## 14. Stop conditions

Stop and open a separate implementation corrective if reconciliation discovers
that a supposedly stale sentence actually reflects:

- a new product defect;
- a release/attestation mismatch;
- crates.io yank state different from the C023 receipt;
- an automatic workflow result that differs from the named GitHub Actions run;
- a C024 severity change that makes it release-blocking.

Do not resolve factual disagreements by choosing the nicer status.

## 15. Closure evidence

Record:

- implementation commit(s);
- exact baseline and final head;
- each contradictory statement corrected;
- final C023 status/disposition wording;
- final M011C disposition and v0.2.1 evidence addendum;
- final C024 handoff state;
- grep/search audit showing no stale active "C023 ready",
  "automatic smoke green", or "main is 0.2.0" claims;
- documentation/release checker results;
- hosted CI/release-drift result if triggered;
- unresolved findings by severity;
- disposition.

C025 is a control-surface repair, not a product milestone. Its purpose is to
make the next agent read one coherent truth before working on C024 or preparing
another release.
