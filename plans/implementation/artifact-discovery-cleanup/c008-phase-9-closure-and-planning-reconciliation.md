# Corrective C008 — Phase 9 Closure and Planning Reconciliation

Status: ready

Repository baseline: `418a9876bb243f88e4403191c4ecaf9f5a2f6ef6`

Corrects:

- post-M008A/M008B/M008C/M008D Phase 9 closure status;
- stale canonical roadmap sequencing and dependency graph;
- stale durable specification language that still presents implemented cleanup/reporting capabilities as future/non-goals;
- registry/subsystem state after the final Phase 9 implementation handoff closed;
- user-facing README cleanup-selector wording that contradicts the implemented selector contract.

Authoritative evidence:

- `plans/closure/artifact-discovery-cleanup/008a-status.md`
- `plans/closure/artifact-discovery-cleanup/008b-status.md`
- `plans/closure/artifact-discovery-cleanup/008c-status.md`
- `plans/closure/artifact-discovery-cleanup/008d-status.md`
- hosted CI run `37178479978` on repository baseline HEAD
- `plans/output-schema-v1.md`
- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`
- `plans/adr/002-adaptive-routine-full-discovery-state.md`

Primary class: corrective / Phase 9 closure / canonical documentation reconciliation

## 1. Objective

Close Phase 9 as a coherent completed roadmap phase and make every active planning/user-facing document describe the behavior that actually exists on `main`.

C008 is documentation/planning reconciliation only. It does not add cleanup capability, widen selector support, change Cargo runtime allowlists, change output-schema semantics, refactor production modules, activate Phase 10, publish a release, or alter destructive safety behavior.

The completed state to reconcile is:

- M008A workspace selective cleanup policy — closed;
- M008B machine-readable reporting and automation contract — closed;
- M008C profile/package qualification — historical conditional closure after profile enablement and package deferral;
- M008D package-selector qualification and enablement — closed;
- package work deferred by M008C has now been satisfied by M008D for the exact qualified Cargo releases;
- Phase 9 has no remaining implementation handoff;
- Phase 10 distribution/operational polish remains deferred and has not become ready merely because Phase 9 closed.

## 2. Triggering findings

The repository implementation and closure evidence are ahead of several canonical documents.

### 2.1 Canonical roadmap is stale

`plans/002-long-term-roadmap.md` still presents Phase 9 as the pre-implementation M008A-M008C sequence and says:

- M008A is ready;
- M008B is blocked on M008A;
- M008C is blocked on M008A/M008B;
- the dependency graph ends at blocked M008C;
- M008D is absent.

That contradicts current closure evidence.

### 2.2 M008C closure state is no longer fully reconciled

`plans/closure/artifact-discovery-cleanup/008c-status.md` correctly records a period-accurate **conditionally closed** result: profile selection was qualified and package selection was intentionally deferred to M008D.

M008D is now closed and supplies the exact package qualification/enablement evidence that M008C deferred.

Active planning should therefore no longer present the umbrella Phase 9 selector work as still conditionally incomplete merely because its historical closure record was conditional.

Historical evidence MUST remain intact. Do not rewrite `008c-status.md` as if package support existed when that record was authored.

### 2.3 Registry/subsystem state is semantically awkward

The active registry currently labels the subsystem `active` while its “current milestone” is a deferred Phase 10 and explicitly states no later registered plan is eligible.

After C008 closes, the subsystem should have no active implementation handoff. Use the repository's existing `planning` state (or another already-defined state if the planning governance requires it) to represent “current objectives closed; next roadmap phase deferred,” rather than claiming active implementation.

While C008 is open, C008 itself is the only ready handoff.

### 2.4 Canonical specification still contains pre-cleanup assumptions

`plans/000-long-term-specification.md` is declared the durable product contract but still includes language such as:

- the initial release is intentionally read-only;
- removal of individual profiles/packages is a non-goal;
- redirected target/build output is deferred;
- machine-readable output is future/pre-destructive work;
- cleanup is a later capability.

Some of this is legitimate historical V0.1 context, but in a durable current contract it is now ambiguous or false unless explicitly scoped to the historical initial-release boundary.

The specification must distinguish historical staged-delivery constraints from current supported capability without deleting the safety rationale that motivated them.

### 2.5 Terminology may be missing current Phase 9 concepts

`plans/001-terminology-and-domain-model.md` defines the established ownership and CleanupUnit model but predates current policy/selector/output-contract terminology.

C008 must audit whether durable terms now need concise definitions for:

- cleanup selection policy;
- policy disposition;
- cleanup selector;
- selector capability qualification;
- selector estimate vs complete output-union measurement;
- stable machine-output reason code.

Only add terms that are part of the durable architecture. Do not duplicate implementation details.

### 2.6 README contains one direct contradiction

The README correctly documents qualified `--profile` and `--package` selectors, but the deeper cleanup description still says the Cargo command runs “without package selectors.”

Replace that stale statement with wording consistent with current behavior:

- ordinary cleanup has no selector;
- qualified profile/package cleanup passes the requested selector to Cargo;
- cargo-cleanme still does not infer or parse Cargo-private artifact/cache layout.

## 3. Historical-evidence preservation rule

C008 MUST distinguish active/canonical state from period-accurate historical records.

Do not rewrite closure records merely to make them look current.

In particular:

- `008c-status.md` remains a conditional closure record because package work was deferred at that point in time;
- `008d-status.md` remains the subsequent evidence satisfying the deferred package line for qualified releases;
- older M006 conditional closure records remain historical and are not converted to unconditional closures without evidence;
- implementation plans may have their top-level status reconciled when later evidence closes a dependency, but their original requirements and stop conditions remain intact.

The active registry, canonical roadmap, subsystem roadmap, and current plan headers should point to the chain of evidence rather than erasing it.

## 4. Work package A — Reconcile M008C/M008D closure semantics

Audit M008C's acceptance/stop conditions against M008D evidence.

If, as expected, M008D satisfies the only deferred package-selector portion of M008C:

1. update the M008C implementation-plan header/status to indicate it is now closed by subsequent M008D evidence;
2. retain `008c-status.md` unchanged as the historical conditional closure;
3. reference both `008c-status.md` and `008d-status.md` anywhere active planning states M008C is closed;
4. make clear that “closed” does not mean all Cargo versions are supported:
   - profile support remains exact-qualified releases;
   - package support remains exact Cargo 1.98.1 and 1.99.0;
   - future/unqualified Cargo versions remain fail-closed;
   - selector-specific reclaimable bytes remain unknown.

If the audit finds an M008C acceptance criterion not satisfied by M008D, do not paper over it. Keep M008C conditional and record the exact residual criterion in C008 closure evidence.

## 5. Work package B — Canonical long-term roadmap

Update `plans/002-long-term-roadmap.md`.

Required Phase 9 state:

~~~text
Phase 9 — Selective cleanup and policy
Status: closed for current objectives

M008A workspace selective cleanup policy [closed]
    |
    v
M008B machine-readable reporting/automation contract [closed]
    |
    v
M008C Cargo selector qualification [closed by subsequent M008D evidence,
                                    with historical conditional closure retained]
    |
    v
M008D Cargo package-selector qualification and enablement [closed]
    |
    v
Phase 10 distribution and operational polish [deferred]
~~~

The exact graph formatting may follow the existing document.

Update the Phase 9 body to describe implemented results rather than future plans:

- workspace min-size/min-age/include/exclude selection exists;
- selection happens after complete ownership resolution;
- fresh size/activity is rechecked before Cargo spawn;
- JSON schema v1 and typed reason codes exist;
- unattended execution is an external-scheduler contract, not an internal daemon;
- profile selection is enabled only on exact qualified Cargo releases;
- package selection is enabled only on exact Cargo 1.98.1/1.99.0;
- selector-specific byte estimates remain unknown;
- nonzero min-size with selectors fails closed;
- package cleanup may remove Cargo-shared dependency artifacts;
- future/unqualified Cargo releases fail closed until qualified.

Do not imply selector support is a stable Cargo-version range when implementation is an exact-release allowlist.

Update the dependency summary through M008D.

Keep Phase 10 status **deferred**. Do not mark it ready, active, or dependency-unblocked solely from Phase 9 closure.

## 6. Work package C — Durable product specification reconciliation

Audit `plans/000-long-term-specification.md` from top to bottom for statements that were deliberately true for V0.1 but are no longer the current product contract.

Preferred treatment:

- preserve a clearly labeled historical “initial read-only release boundary” where useful;
- move obsolete “future” wording into historical context or replace it with current behavior;
- keep all safety invariants that still apply.

At minimum reconcile:

### Initial release/read-only language

Clarify that the project was intentionally staged through a read-only V0.1, but the current product now includes separately invoked, Cargo-mediated destructive cleanup.

No-argument invocation and scan remain read-only.

### Profile/package non-goal

The old non-goal “remove individual profiles or packages from a target tree” must no longer be presented as a current blanket non-goal.

Replace with the current boundary:

- cargo-cleanme may request qualified profile/package cleanup through Cargo;
- cargo-cleanme does not directly delete inferred profile/package artifact paths;
- selector support is runtime-Cargo-qualified and fail-closed outside the allowlist;
- package cleanup may include Cargo-determined shared dependency artifacts.

### Redirected output

Reconcile any statement that redirected `target-dir`/`build-dir` support is deferred. The current contract resolves output through Cargo, classifies physical ownership, requires complete `PrivateBounded`/authorization/freshness proof, and supports distinct build-dir only where Cargo runtime capability is known.

### Machine-readable output

Replace future/SHOULD language with current schema-v1 behavior:

- `--format json`;
- one deterministic JSON document on stdout;
- dedicated DTO/schema versioning;
- typed policy/safety/action reason codes;
- progress/stats isolation from JSON stdout.

### Cleanup capability section

Convert “later cleanup milestone” wording to current requirements while preserving the existing proof model.

Add current Phase 9 requirements where canonical:

- selection policy only narrows candidates after complete ownership resolution;
- policy-rejected workspaces remain ownership participants;
- selector requests never narrow ownership proof;
- selector-specific estimates must not be fabricated;
- unsupported Cargo selector/runtime combinations fail before mutation.

Do not turn the specification into a release changelog or encode test-only implementation details.

## 7. Work package D — Terminology/domain-model audit

Audit `plans/001-terminology-and-domain-model.md`.

Add only durable missing concepts.

Recommended additions if absent:

### Cleanup selection policy

A user/config-supplied policy evaluated over an already-resolved complete CleanupUnit. It may reject a unit by minimum complete-output size, minimum inactivity age, or canonical workspace include/exclude policy. It never removes a workspace from the ownership universe.

### Policy disposition

A typed result describing whether a unit was selected or rejected by user policy, distinct from a safety/ownership skip.

### Cleanup selector

A Cargo-mediated mutation selector such as profile or package. A selector narrows the request passed to Cargo after complete workspace/output proof; it does not narrow the ownership universe or authorize direct filesystem deletion.

### Selector capability qualification

The fail-closed runtime-Cargo compatibility decision determining whether a selector may reach Cargo Preview/Execute.

### Selector estimate

A selector-specific reclaimable byte estimate. Current profile/package selectors have no trustworthy selector estimate, so it is unknown/null. Whole output-union measurements are context and must not be mislabeled as selector-reclaimable bytes.

### Stable cleanup reason code

A machine-output semantic code independent of human detail strings.

Do not rename existing `CleanupUnit`, `OutputSet`, ownership classes, or discovery-state terminology unless a real inconsistency exists.

## 8. Work package E — Subsystem roadmap and registry

### While C008 is open

Register C008 as the only ready handoff.

Registry target:

- subsystem status: `ready`;
- current milestone: C008 Phase 9 closure/planning reconciliation;
- no feature implementation handoff active;
- Phase 10 explicitly remains deferred.

Subsystem roadmap:

- append C008 after M008D;
- mark it ready;
- describe it as docs/planning reconciliation only.

### At C008 closure

Registry target state:

- C008: closed;
- M008A: closed;
- M008B: closed;
- M008C: closed by M008D evidence if the Work Package A audit passes;
- M008D: closed;
- subsystem status: `planning` (or the existing vocabulary's equivalent for no active implementation);
- current milestone: `none / Phase 10 deferred`;
- explicitly state there is no ready implementation handoff.

Subsystem roadmap target:

- Phase 9 closed for current objectives;
- no active implementation milestone;
- Phase 10 remains deferred pending separate research/planning decision.

Do not invent M009 or a Phase 10 implementation plan in this corrective.

## 9. Work package F — README/current user contract

Fix active README contradictions while preserving useful operational detail.

Required audit targets:

- selector support/version lists;
- minimum-size interaction with selectors;
- selector estimate vs complete output-union bytes;
- external scheduler semantics;
- package shared-dependency caveat;
- stale “without package selectors” wording;
- claims about release/distribution status.

The cleanup proof paragraph should say, in substance:

- Cargo runs with frozen resolved output environment;
- ordinary cleanup uses the whole-workspace request;
- a qualified requested profile/package selector is passed through to Cargo;
- cargo-cleanme never parses private Cargo cache/artifact layout or directly deletes selector artifacts.

Do not broaden support beyond closure evidence.

## 10. Work package G — Output-schema documentation consistency

Audit `plans/output-schema-v1.md` against `src/output.rs`, M008B closure evidence, and M008D selector behavior.

This is a documentation consistency audit, not a schema redesign.

Verify/document:

- schema_version = 1;
- selector kind/value fields;
- selector estimate null semantics;
- output-union context naming;
- stable reason codes including selector unsupported/invalid and policy selector-estimate-unavailable behavior;
- exit-status contract where documented;
- diagnostic detail text is not stable API.

If code and schema documentation disagree materially, C008 must stop and file a separate corrective implementation plan instead of changing public schema behavior inside a documentation pass.

## 11. Documentation consistency audit

Before closure, search active/canonical docs for stale phrases/concepts including:

- `M008A ... ready`;
- `M008B ... blocked`;
- `M008C ... blocked`;
- `M008D ... active` or `closing`;
- `package selector deferred` outside clearly historical M008C evidence;
- `Phase 9 ... planned`;
- `future selective cleanup`;
- `no implementation handoff is active` while C008 is registered;
- `without package selectors`;
- profile/package cleanup as a current non-goal;
- redirected target/build output described as unimplemented;
- machine-readable output described as future;
- destructive cleanup described as entirely future;
- Phase 10 described as ready/unblocked without a separate planning decision.

Historical implementation/closure documents may retain period-accurate language when clearly scoped as historical evidence.

## 12. Production-code audit boundary

C008 SHOULD result in no production Rust changes.

A narrow production-code change is allowed only if the documentation audit reveals a direct user-contract defect that cannot honestly be reconciled as documentation—for example, code contradicts the already accepted M008 closure evidence.

If such a defect is found:

1. do not silently fix it under C008;
2. classify severity and safety impact;
3. write/register a separate corrective implementation plan;
4. keep C008 focused on truthful documentation of the actual safe state.

The existing observation that `src/cleanup.rs` is large is maintenance architecture debt, not a C008 defect. Do not refactor it in this pass.

## 13. Verification

Because C008 is expected to be documentation/planning-only, required verification is:

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
rustup run 1.89 cargo check --locked --all-targets
rustup run 1.89 cargo test --locked --all-targets
git diff --check
~~~

Hosted Linux/macOS/Windows/MSRV CI must pass at the final C008 implementation commit even if only Markdown changed, because the branch/HEAD is the closure artifact.

Additionally:

- verify the final README examples against current CLI parsing;
- compare `plans/output-schema-v1.md` field/reason names with `src/output.rs` and cleanup reason enums;
- perform repository-wide documentation grep for the stale concepts in §11;
- verify no open implementation plan is accidentally marked ready after C008;
- verify Phase 10 remains deferred.

No new real-Cargo selector matrix is required unless C008 uncovers contradictory implementation evidence. M008C/M008D closure evidence remains authoritative.

## 14. Acceptance criteria

C008 may close only when:

- the canonical roadmap shows M008A-M008D in their actual terminal states and Phase 9 closed for current objectives;
- the dependency graph reaches closed M008D and then deferred Phase 10;
- active planning no longer leaves M008C conditionally incomplete if M008D evidence satisfies its deferred criterion;
- the historical M008C conditional closure record is preserved unchanged;
- the durable product specification distinguishes historical V0.1 staging from current cleanup/reporting/selector capability;
- current selector/version/accounting limitations are stated accurately;
- terminology covers durable Phase 9 policy/selector concepts where needed;
- README contains no contradiction about package/profile selectors;
- output-schema documentation matches code without changing schema semantics;
- registry/subsystem status no longer claims active implementation after C008 closure;
- no Phase 10 implementation handoff is invented or activated;
- production Rust behavior is unchanged;
- full test/MSRV/hosted CI gates pass;
- a C008 closure record maps each stale finding to its corrected document/evidence.

## 15. Non-goals

- no new selector capability;
- no expansion of Cargo version allowlists;
- no selector-specific byte-attribution algorithm;
- no direct artifact deletion;
- no ownership/proof changes;
- no JSON schema version bump;
- no output DTO redesign;
- no `cleanup.rs` refactor;
- no dependency upgrade;
- no release/version bump;
- no crates.io/GitHub release;
- no package-manager integration;
- no installer/self-update work;
- no Phase 10 research or implementation planning;
- no rewriting historical closure evidence to hide intermediate states.

## 16. Stop conditions

Stop and create a separate corrective plan if reconciliation finds:

- M008D does not actually satisfy a required M008C acceptance criterion;
- README/schema claims require changing selector behavior rather than wording;
- code emits machine-output semantics that contradict the accepted schema-v1 contract;
- current selector execution can bypass complete ownership/freshness proof;
- qualified runtime allowlists in docs and production disagree;
- Phase 9 closure requires a production safety change.

Do not mark Phase 9 fully closed merely to remove a conditional label if the evidence does not support it.

## 17. Closure evidence

Create `plans/closure/artifact-discovery-cleanup/c008-status.md` containing:

- implementation/documentation commit(s);
- exact repository baseline;
- stale-finding -> corrected-file mapping;
- M008C -> M008D evidence reconciliation decision;
- canonical roadmap/status before/after summary;
- specification sections reconciled;
- README/schema consistency findings;
- repository-wide stale-phrase audit result;
- full local verification results;
- hosted Linux/macOS/Windows/MSRV CI run;
- explicit statement that production Rust behavior did not change, or a pointer to any separately registered corrective if a code defect was found;
- final future-plan disposition: Phase 10 remains deferred with no ready implementation handoff.
