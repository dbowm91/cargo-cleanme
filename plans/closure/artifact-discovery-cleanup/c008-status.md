# Artifact Discovery and Cleanup C008 Status

Plan: `plans/implementation/artifact-discovery-cleanup/c008-phase-9-closure-and-planning-reconciliation.md`

Disposition: **closed**

Implementation commit: `f823798` (`Reconcile and close Phase 9 planning state`).

Repository baseline: `50de189c4720b1049a63d18e22802999e785d592` (`Register C008 in subsystem roadmap`). The plan's stated `418a9876bb243f88e4403191c4ecaf9f5a2f6ef6` is the M008D implementation baseline; subsequent commits registered and prepared C008, so reconciliation was applied to the actual baseline HEAD above.

Date: 2026-10-04

## M008C/M008D reconciliation

M008D satisfies the only deferred M008C line: package-selector qualification and enablement. M008C is now closed by subsequent M008D evidence in active planning; its original `conditionally closed` record at `plans/closure/artifact-discovery-cleanup/008c-status.md` is preserved unchanged. M008D records package support only for exact Cargo 1.98.1 and 1.99.0, with package identity validation, unchanged complete-workspace proof, unknown selector estimates, shared-dependency effects, and fail-closed behavior outside those releases. Profile support remains limited to its exact qualified release list. Future/unqualified Cargo releases remain fail-closed.

## Stale finding to correction map

| Finding | Corrected document / evidence |
|---|---|
| Phase 9 graph stopped at blocked M008C and omitted M008D | `plans/002-long-term-roadmap.md`: Phase 9 state/body and dependency graph now show M008A-M008D closed and Phase 10 deferred. |
| M008C remained active-planning conditional after its follow-up closed | M008C implementation-plan status, `plans/registry.md`, and `plans/subsystems/artifact-discovery-cleanup-roadmap.md` now say closed by M008D while preserving the period-accurate M008C closure record. |
| Durable contract called cleanup, redirected output, and machine output future/non-goals | `plans/000-long-term-specification.md` now distinguishes the historical V0.1 boundary from current Cargo-mediated cleanup, policy, JSON schema v1, selector qualification, and exact limitations. |
| Durable vocabulary lacked Phase 9 concepts | `plans/001-terminology-and-domain-model.md` adds cleanup selection policy, policy disposition, cleanup selector, selector capability qualification, selector estimate, and stable cleanup reason code. |
| README cleanup proof paragraph excluded selectors | `README.md` now states ordinary cleanup uses the whole-workspace request and qualified requested profile/package selectors pass through to Cargo; no private artifact/cache parsing or direct deletion is implied. |
| README omitted selector estimate/size-policy limitation | `README.md` states selector estimate is unknown/null, union measurement is context, and nonzero minimum-size policy fails closed with selectors. Runtime release allowlists and package shared-dependency caveat remain explicit. |
| Output-schema reason-code list was incomplete | `plans/output-schema-v1.md` now lists current cleanup reason codes and identifies `selector_estimate_unavailable` as a typed policy disposition; field names and schema version remain unchanged. |
| Registry claimed active implementation after Phase 9 work ended | `plans/registry.md` enters the closing state for C008; final target is subsystem `planning`, no implementation handoff, Phase 10 deferred. |

## Canonical roadmap/status before and after

- Before: Phase 9 was described as planned; M008A ready, M008B blocked, M008C blocked/conditional, and M008D absent from the canonical roadmap. Registry had C008 as the only ready handoff.
- After: M008A, M008B, M008C (closed by subsequent M008D evidence), M008D, and C008 are closed. The dependency graph reaches Phase 10, which remains deferred. M006A/C/D historical conditional records remain unchanged and are not blockers.
- Future-plan review: no other ready/blocked implementation plan depends on C008 or Phase 9 closure. M008C was the only active status reconciled by M008D evidence. Phase 10 is not unblocked by this work and no new Phase 10 plan was created.

## Specification and consistency audit

Reconciled specification sections: product boundary, primary goals, historical V0.1/current non-goals, read-only architecture principle, workspace/output gates, output contract, workspace/output extension, cleanup requirements and current policy/selector contract, and deferred distribution direction.

The README selector/version and shared-dependency statements match the M008C/M008D closure evidence. Its cleanup proof description reflects whole-workspace ordinary cleanup and Cargo-mediated qualified selectors. CLI help was inspected via `cargo run -- clean --help`; the documented command, policy, JSON, mode, and selector options are accepted by the current parser. The installed `cargo-cleanme` command on PATH is stale and lacks `clean`; it was not used for this check.

The schema audit compared `plans/output-schema-v1.md` with `src/output.rs` and cleanup reason enums. `schema_version` remains 1; selector kind/value fields, null selector estimate, output-union context, policy disposition, selector unsupported/invalid outcomes, and process status descriptions are consistent. Diagnostic detail remains non-stable API. No schema or production behavior changed.

Repository-wide stale-phrase audit over active/canonical README, specification, terminology, roadmap, registry, and subsystem roadmap found no remaining claims that M008A is ready, M008B/M008C are blocked, M008D is active, Phase 9 is planned, selectors are a current non-goal, redirected output or machine output is unimplemented, cleanup is entirely future, or Phase 10 is ready/unblocked. Period-accurate wording remains only in historical closure/implementation records and in C008's description of the findings it corrects.

## Verification

All local commands passed on baseline `50de189c4720b1049a63d18e22802999e785d592` with documentation changes present:

- `rtk cargo fmt --check` — pass.
- `rtk cargo clippy --all-targets --all-features -- -D warnings` — pass.
- `rtk cargo test --all-targets --all-features` — pass, 199 tests.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — pass.
- `rtk rustup run 1.89 cargo test --locked --all-targets` — pass, 199 tests.
- `rtk git diff --check` — pass.
- CLI help inspection (`rtk cargo run -- clean --help`) — pass; listed options match the README examples and selector contract.

Hosted Linux/macOS/Windows and Rust 1.89 CI: GitHub Actions run `37179572356` passed on implementation commit `f823798` (Ubuntu, macOS, Windows, and MSRV 1.89 jobs).

## Production behavior and final disposition

No production Rust file changed. The C008 diff is documentation and planning state only.

Phase 9 is closed for current objectives. Phase 10 distribution/operational polish remains deferred pending a separate planning decision, with no ready implementation handoff.
