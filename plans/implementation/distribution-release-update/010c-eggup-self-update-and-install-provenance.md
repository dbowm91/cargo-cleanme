# M010C — Eggup Self-Update and Install Provenance

Status: closed, with a recorded defect in the transport-selection requirement
discharged by corrective C011 (see the addendum in
`plans/closure/distribution-release-update/010c-status.md`)

Repository baseline: M010A closure commit plus the registered Eggup acquisition M009 publication dependency.

Source roadmap: Phase 10 — Distribution and operational polish

Hard dependencies:

- M010A closed.
- Eggup acquisition M009 publishes the qualified `eggup-curl` package to crates.io.

Integration dependency:

- M010B release asset/installer convention must be stable before closure, though fixture-first updater implementation may proceed against M010A's written release contract.

## 1. Objective

Add a safe `cargo-cleanme update` command with Gregg-like user semantics while using Eggup for generic acquisition and replacement mechanics and Eggpack release evidence for producer facts. Do not copy `gregg-update`.

Both invocation forms must work:

~~~text
cargo-cleanme update
cargo cleanme update
~~~

## 2. Ownership split

cargo-cleanme owns:

- current/latest version policy;
- stable release authority;
- GitHub repository/origin and tag selection;
- canonical target selection;
- Cargo fallback policy;
- destination/install-provenance policy;
- CLI messages/exit semantics.

Eggup owns:

- bounded acquisition transport;
- private staging;
- size/digest verification inputs;
- destination ownership/revalidation;
- locking;
- replacement;
- rollback/recovery receipts.

`eggup-eggpack` may project `release-manifest.json` into acquisition/install facts after cargo-cleanme has selected the exact authorized release.

## 3. Transport choice

Use published `eggup-curl` as the default candidate because this CLI should not add an embedded HTTP/TLS stack merely for infrequent self-update if the external-curl path is materially smaller.

Before locking that choice:

- record stripped release binary size and dependency tree for the pre-update baseline;
- record the same with the curl adapter;
- optionally compare the already-published `eggup-eggfetch` path using the same release profile;
- select one production transport.

Do not ship both transports by default without a measured reason. If curl is not available on the host, a typed adapter-unavailable result may enter an explicitly chosen cargo-cleanme fallback policy; ordinary TLS/5xx/timeout errors must not.

## 4. Release/version authority

Use one stable-version authority consistently. Prefer crates.io stable package metadata once cargo-cleanme is published, matching the established Gregg semantics, then select the exact GitHub tag `vX.Y.Z`.

The GitHub release must contain a matching Eggpack release manifest/artifact inventory.

Rules:

- installed version >= latest -> report already current, no mutation;
- prerelease metadata is not silently selected as stable;
- malformed authority response -> hard failure;
- exact selected GitHub release evidence must bind the same product/version.

## 5. Candidate path

For a supported binary target:

1. select exact release/tag;
2. fetch bounded `release-manifest.json`;
3. project the canonical target;
4. acquire exact artifact through the selected Eggup transport;
5. verify exact size + SHA-256;
6. stage privately;
7. execute staged candidate with a bounded environment and timeout;
8. require exact `cargo-cleanme X.Y.Z`;
9. revalidate current destination identity/ownership;
10. commit through Eggup;
11. report from/to/source and recovery disposition.

A binary 404 may enter source fallback only if release policy explicitly treats that target as fallback-capable. Missing checksum/manifest evidence for an otherwise declared artifact is not fallback.

## 6. Cargo fallback

Cargo fallback is application policy above Eggup, not transport fallback.

Build the exact selected version into a private temporary Cargo root, validate the produced executable identity, then feed the staged candidate through the same destination/replacement policy where that preserves manager semantics.

Never treat a failed checksum/version/TLS request as a reason to run Cargo.

## 7. Install provenance decision gate

cargo-cleanme is commonly installed through `cargo install`, so M010C must make installation provenance explicit before mutation.

Classify at least:

- positively Cargo-managed;
- positively cargo-cleanme installer/direct-binary managed, if an ownership marker/receipt is introduced by M010B;
- unknown/foreign.

Do not infer ownership solely from `~/.cargo/bin`, basename, or PATH order.

Before coding the final commit path, qualify two strategies:

### Strategy A — exact invoked binary

Replace the exact current executable through Eggup after ownership proof, matching Gregg semantics. This is simple and cross-platform but can leave Cargo's package metadata describing an older install.

### Strategy B — manager-aware Cargo update

For positively Cargo-managed installs, delegate the version transition through Cargo in a way that preserves Cargo bookkeeping and works on Windows running-image semantics; use Eggup direct replacement for installer/unmanaged installs.

Select Strategy B only if it can be proved bounded and cross-platform without editing Cargo-owned metadata directly. Otherwise use Strategy A with truthful documentation that self-update changes the executable independently of Cargo install metadata, or refuse self-update for positively Cargo-managed installs and print the exact manager command.

Unknown/foreign ownership must fail closed with actionable guidance.

## 8. CLI behavior

Add an `Update` subcommand with concise deterministic outcomes such as:

- already current;
- updated X -> Y from verified GitHub binary;
- updated X -> Y via Cargo fallback;
- manager-owned install requires manager update;
- unsupported target / no safe update path.

Progress/network diagnostics go to stderr. Final human outcome follows existing CLI conventions. If JSON support is extended to update, it must use a separately versioned DTO rather than reusing scan/cleanup shapes accidentally; JSON update output is optional for this milestone.

## 9. Tests

Fixture-test:

- latest == current;
- current newer than authority;
- malformed/prerelease version metadata;
- supported/unsupported target mapping;
- manifest product/release/target mismatch;
- exact artifact 404;
- checksum/digest/size mismatch;
- transport unavailable vs ordinary transport failure;
- candidate timeout/nonzero exit/wrong version/wrong product;
- ownership changes between plan and commit;
- foreign/unknown destination;
- interrupted commit and recovery receipt;
- Cargo fallback exact-version behavior;
- positive/negative Cargo-managed detection;
- both direct and `cargo cleanme update` argv forms;
- Windows running-image behavior.

No test should require the public GitHub service for correctness.

## 10. Verification

Run normal Stable/MSRV/platform CI plus focused updater fixtures on Linux/macOS/Windows. Record binary-size and dependency deltas for the chosen transport.

Perform an end-to-end update rehearsal using a local or draft release containing older/newer fixture versions before public publication.

## 11. Documentation

Document:

- what `update` considers the version authority;
- exact fallback conditions;
- integrity vs authenticity claims;
- required external curl if selected;
- manager/provenance behavior;
- how to update when self-update refuses a managed/foreign destination;
- recovery behavior if replacement fails.

## 12. Acceptance criteria

- generic update mechanics come from published Eggup crates;
- no copied Gregg updater module exists;
- exact candidate identity is verified before mutation;
- destination ownership is revalidated at commit;
- fallback is absence/unsupported-target driven, not error driven;
- install provenance has a tested, explicit policy;
- both command spellings work;
- chosen transport footprint is measured;
- hosted Linux/macOS/Windows updater fixtures pass.

## 13. Stop conditions

Stop and author a corrective/upstream plan rather than weaken policy if Eggup cannot represent the required destination transaction, if Cargo-managed provenance cannot be handled truthfully, if Windows replacement cannot be made safe, or if the selected release evidence cannot bind exact product/version/target bytes.

## 14. Closure evidence

Record Eggup crate versions, target/release policy, transport footprint table, install-provenance decision and fixtures, end-to-end rehearsal, hosted runs, recovery evidence, and any refused manager/install scopes.
