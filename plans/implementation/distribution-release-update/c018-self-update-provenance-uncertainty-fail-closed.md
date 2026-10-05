# C018 — Self-Update Provenance Uncertainty Fail-Closed Corrective

Status: closed

Closure record: `plans/closure/distribution-release-update/c018-status.md`

Repository baseline: `c3feae01fe5b684826b77b4d32afcb2e3bd4e3d0`

Corrects:

- `plans/implementation/distribution-release-update/c017-cargo-managed-provenance-misdetection-corrective.md`
- `plans/closure/distribution-release-update/c017-status.md`
- the self-update provenance contract originally introduced by M010C

Source roadmap: Phase 11 — hardening, release trust, and qualification automation

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: corrective / ownership safety

Hard dependencies: none. C017 is closed and v0.1.6 is the current qualified release.

## 1. Objective

Eliminate avoidable fail-open cases in self-update provenance classification.

The current updater correctly refuses Cargo-managed installations when Cargo's install record is well formed, but the architecture review found ambiguity paths that can collapse to `VerifiableSelfManaged`. That is the dangerous direction: an ownership false negative can overwrite a file Cargo still believes it owns.

C018 must make provenance a typed evidence decision rather than an `Option`-shaped best effort. Positive Cargo-root evidence that cannot be interpreted must become `UnprovableOwnership`, never self-managed ownership.

The corrective must also make the non-dry-run refusal occur before remote version-authority acquisition when the local provenance already proves that mutation is forbidden.

## 2. Current evidence

At the baseline:

- `classify_provenance_in` positively handles the explicit `CARGO_HOME/bin` case and the normal `cargo install --root ROOT` case.
- `cargo_recorded_version` returns only `Option<String>`, collapsing at least "not this package", "record unreadable", "record malformed", and "record shape unsupported" into `None`.
- ancestor probing then falls through to hashing the executable and can classify it as `VerifiableSelfManaged`.
- `binary_name` turns a non-UTF-8 file name into the empty string.
- the ancestor probe has a fixed search-depth bound rather than an evidence-typed outcome.
- `run` classifies provenance before contacting crates.io, but for a mutating update it does not refuse a non-self-managed installation until after fetching and comparing registry metadata.

The architecture review records four fail-open classes to investigate rather than assume reachable:

1. absent, unreadable, or unparseable Cargo install metadata;
2. the bounded ancestor search;
3. non-UTF-8 executable names;
4. malformed record keys/value shapes.

Some of these may be unreachable for a genuine current Cargo install. C018 must distinguish "not reproducible against Cargo" from "safe to ignore"; it must not convert hypothetical findings into broad path-based ownership guesses.

## 3. Invariants

- Never infer Cargo ownership from basename, PATH order, or a directory merely named `bin`.
- Never infer self-managed ownership from a failure to parse positive package-manager evidence.
- An arbitrary installer-managed `.../bin/cargo-cleanme` with no Cargo evidence must remain eligible for self-managed classification; the corrective must not make every `bin` directory manager-owned.
- An explicit Cargo home whose `bin` contains the running executable is manager-sensitive. Missing/unreadable/malformed ownership metadata there is `UnprovableOwnership`.
- If an ancestor is positively identified as a Cargo install root, inability to interpret its ownership record is `UnprovableOwnership`.
- A well-formed Cargo record that definitely names a different package/binary is not ownership evidence for this executable.
- A mutating update must refuse `CargoManaged`, `UnprovableOwnership`, and unsupported-host provenance before any release asset is acquired; where local provenance alone makes mutation impossible, do not perform a registry request merely to discover a newer version.
- `--dry-run` remains non-mutating and may retain version-authority lookup when needed to report a candidate, but it must report the actual provenance classification.
- C017's successful normal default-root and `--root` behavior must not regress.
- No change to cleanup/discovery semantics is authorized.

## 4. Work package A — Characterize real Cargo ownership layouts

Use real Cargo, not only fixtures, for:

- the project's MSRV Cargo/Rust 1.89 toolchain;
- the exact current release-builder toolchain (currently Rust/Cargo 1.99.0);
- default Cargo home installation;
- `cargo install --root <isolated-root>`;
- a relocated `CARGO_HOME`;
- supported Unix and Windows layouts where hosted evidence is available.

Record which metadata files Cargo actually writes, the exact relevant `.crates.toml` schema, and whether any secondary manager marker is stable enough to be evidence.

Then inject failure states in isolated copies:

- unreadable metadata where the platform permits it;
- malformed TOML;
- malformed package entry;
- malformed executable list;
- missing metadata after a proven Cargo install;
- an executable with an unusual/non-UTF-8 name on Unix where representable.

Do not mutate the user's real Cargo home.

## 5. Work package B — Replace `Option` ownership parsing with typed evidence

Refactor the package-manager record path so callers can distinguish at least:

- positively managed by Cargo, with recorded version;
- valid Cargo record that does not claim this package/binary;
- no positive Cargo-root evidence;
- positive Cargo-root evidence but unreadable/unparseable/unsupported ownership record.

Names are implementation detail; semantics are not.

Malformed entries unrelated to cargo-cleanme may still be skipped if the parser can prove they cannot claim this binary. A malformed entry that could be the cargo-cleanme record must not be silently treated as absence.

The parser must remain bounded. Reject path traversal, unbounded file reads, and manager metadata outside the candidate root.

## 6. Work package C — Make root detection evidence-aware

Revisit `cargo_bin_roots_above` and `CARGO_ROOT_SEARCH_DEPTH` against the real layouts from Work Package A.

Acceptable outcomes include:

- replacing the depth heuristic with a smaller exact-layout derivation;
- retaining a bound but proving it cannot miss a supported Cargo layout;
- using positive Cargo metadata presence as the root marker.

Do not solve the problem by treating every ancestor `bin` directory as Cargo-managed.

If a claimed fail-open path is proven unreachable under supported Cargo layouts, record that as evidence and add a regression test for the premise rather than adding speculative production behavior.

## 7. Work package D — Refuse forbidden mutation before network work

For non-`check_only` updates, refuse locally known non-self-managed provenance before registry metadata or release asset acquisition.

Required observable effects:

- Cargo-managed: immediate manager refusal/remediation;
- unprovable ownership: immediate reinstall/refusal guidance;
- self-managed: proceed to version authority and normal update planning;
- unsupported host: existing fail-closed semantics preserved.

If preserving current `--dry-run` output requires a different ordering, keep the distinction explicit and tested.

Correct stale comments that claim "before acquiring anything" only if the implementation now makes that statement true.

## 8. Regression evidence

Add tests that would fail against the baseline implementation:

- unreadable/unparseable positive Cargo metadata cannot become `self_managed`;
- malformed cargo-cleanme record cannot be skipped into self-managed ownership;
- unrelated malformed entries do not hide a later valid cargo-cleanme record;
- explicit Cargo-home bin with missing record is unprovable;
- a genuinely self-managed installer layout remains self-managed;
- non-mutating/dry-run semantics remain zero-write;
- mutating manager-owned/unprovable runs make zero registry/release requests;
- default Cargo home and `--root` real installations are refused with bytes unchanged.

Where platform permissions make "unreadable" unreliable (notably elevated Windows runners), use a deterministic failing read fixture and separately record live platform evidence.

## 9. Release evidence

Per `docs/RELEASING.md`, a corrective that changes published behavior is not closed until a release carries it.

The first release containing C018 must prove, from an isolated real Cargo installation of that same published version:

- `cargo-cleanme update` refuses before mutation;
- Cargo bookkeeping remains unchanged;
- binary digest remains unchanged;
- malformed/unusable positive manager metadata refuses rather than self-manages where the state can be reproduced safely;
- a published self-managed installation still follows the normal version/update path.

Because the refusal is moved before the version-authority comparison, the Cargo-managed proof does not require a newer target release merely to exercise the refusal.

## 10. Verification

At minimum:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
bash scripts/release-check.sh
~~~

Hosted Linux, macOS, and Windows CI is required. Record real-Cargo provenance tests separately from pure unit fixtures.

## 11. Documentation

Update:

- `docs/TROUBLESHOOTING.md` if refusal wording or recovery guidance changes;
- `architecture/12-self-update.md` to remove resolved fail-open findings and accurately describe remaining irreducible ambiguity;
- `architecture/overview.md` finding disposition;
- `CHANGELOG.md` in the release that carries the fix.

Do not claim that every absent metadata file proves Cargo ownership. State the evidence boundary precisely.

## 12. Acceptance criteria

C018 closes only when:

- manager-record parsing is typed and parse uncertainty cannot silently become self-managed ownership;
- every positively identified Cargo-root ambiguity fails closed;
- normal self-managed installer layouts still classify correctly;
- the supported default/`--root` Cargo layouts are proven with real Cargo;
- non-dry-run forbidden provenance refuses before remote release/version acquisition;
- premise-negative tests demonstrate the new guards actually fail against the old behavior;
- hosted platform/MSRV gates are green;
- a published release carries the corrective and the real manager-owned refusal leaves both bytes and Cargo bookkeeping unchanged;
- no medium-or-higher provenance finding remains.

## 13. Stop conditions

Stop and write a narrower follow-up if:

- eliminating an ambiguity requires treating ordinary self-managed `bin` layouts as Cargo-owned without manager evidence;
- Cargo versions in the supported range use materially incompatible ownership schemas;
- the fix requires changing Eggup transaction mechanics rather than cargo-cleanme provenance policy;
- a real Cargo layout cannot be distinguished safely from a product-owned install.

Do not trade an ownership false negative for a broad path-name heuristic.

## 14. Closure evidence

Record:

- implementation/release commits and tag;
- real Cargo metadata/layout matrix;
- typed record-parser behavior matrix;
- premise-negative tests;
- zero-network evidence for refused mutating provenance;
- default Cargo home and `--root` byte/bookkeeping evidence;
- hosted CI run IDs;
- release smoke result;
- unresolved findings by severity;
- disposition.
