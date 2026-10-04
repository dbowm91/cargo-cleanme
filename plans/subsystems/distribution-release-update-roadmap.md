# Distribution, Release, and Update Roadmap

Status: Phase 10 remains **closed**, with a post-release corrective line now registered. C013 cross-platform fixture-premise audit is **ready**. C014 v0.1.2 live-update/release-reproducibility corrective is **blocked on C013 for publication**; its non-publication toolchain/source-identity preparation may proceed in parallel. C010 remains an upstream request (`proposed`) that nothing here waits on.

Repository audit baseline: `85b5d4adee81f363c788505aa2f7d0136eb5ff0b`

Canonical references:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`
- `plans/003-planning-process.md`

External architecture references:

- Eggpack producer authority: `eggstack/eggpack`
- Eggup consumer/update authority: `eggstack/eggup`
- Gregg install/update UX reference: `eggstack/gregg`
- Eggsact Eggpack + Eggup reference consumer: `eggstack/eggsact`

## 1. Purpose and ownership boundary

This subsystem owns cargo-cleanme's public distribution contract, release artifacts, product-owned bootstrap wrappers, self-update policy, publication gates, support matrix, completion/manpage generation, release documentation, and release-oriented benchmark evidence.

It does not own generic release construction, generic verified-update transactions, a second package manager, service lifecycle, background updating, or arbitrary software installation.

Ownership is deliberately split:

- Eggpack owns producer-side release contracts, build/qualification planning, release manifests, generated release CI, exact-release bootstrap evidence, and GitHub draft staging.
- Eggup owns consumer-side bounded acquisition, integrity verification, local staging, ownership revalidation, replacement, rollback, and recovery evidence.
- cargo-cleanme owns release/version authority, target selection, install/update destination policy, Cargo fallback policy, CLI presentation, public `install.sh` / `install.ps1` wrappers, and publication decisions.

cargo-dist is not a production dependency for this roadmap. Eggpack is the canonical Eggstack producer path.

## 2. Invariants

- Distribution work must not weaken any existing cleanup safety invariant.
- Published binaries must be built from an exact source revision and qualified as the same bytes that are staged for release.
- SHA-256 is integrity evidence, not an authenticity claim.
- Public installers and `cargo-cleanme update` must fail closed on checksum, manifest, candidate-identity, TLS/transport, timeout, or destination-ownership failures.
- Cargo/source fallback is permitted only by explicit cargo-cleanme policy for unsupported targets or an actually absent selected binary. Verification failure is never a fallback signal.
- No installer or updater invokes `sudo` internally.
- Self-update must never infer ownership from basename alone.
- The application remains a Cargo external subcommand: both `cargo-cleanme ...` and `cargo cleanme ...` stay supported.
- Release automation stages drafts/evidence; public publication remains an explicit release action.
- Package-manager integration must not silently mutate package-manager-owned metadata.
- The current Rust library surface is not implicitly declared stable merely because the crate is published; the support stance must be documented before first publication.

## 3. Target release contract

Initial required binary targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

The preferred public asset convention is version-independent within a release:

- `cargo-cleanme-{target}`
- `cargo-cleanme-{target}.sha256`
- Windows executable suffix `.exe`

The Linux build strategy should hold an explicit glibc compatibility floor when the current Eggpack Cargo/cargo-zigbuild path can qualify it. A glibc 2.17 floor is the initial target because it matches established Eggstack release practice; implementation must record real qualification rather than claim it from configuration alone.

ARMv7, musl, Windows ARM64, package-manager formulae, and additional targets are follow-up capabilities unless evidence justifies adding them without delaying the initial distribution closure.

## 4. Dependency graph

~~~text
Phase 9 + C008 [closed]
        |
        v
M010A Eggpack distribution contract + package readiness [CLOSED]
        |
        v
C009 Windows test-fixture portability [CLOSED: restores per-platform evidence]
        |
        +-----------------------------+
        |                             |
        v                             v
M010B bootstrap installers      Eggup acquisition M009
+ release artifact             eggup-curl publication
qualification                  [external; VERIFIED UNPUBLISHED 2026-10-04,
        |                       so M010C must select from what is
        |                       actually published]
        +-------------+---------------+
                      |
                      v
M010C Eggup self-update + install provenance
                      |
                      v
M010D publication + operational polish + release closure
~~~

M010B may proceed while Eggup M009 is implemented. M010C must not vendor/copy Gregg update machinery if Eggup M009 is delayed.

## 5. M010A — Eggpack distribution contract and package readiness

Plan: `plans/implementation/distribution-release-update/010a-eggpack-distribution-contract-and-package-readiness.md`

Closure: `plans/closure/distribution-release-update/010a-status.md`

Status: closed at implementation `b4662af`. No publication occurred. Its single
green-CI acceptance criterion was a pre-existing Windows test-fixture defect,
discharged by corrective C009.

Primary outcomes:

- make Eggpack the canonical producer authority for cargo-cleanme release artifacts;
- add the static release contract, build bindings, qualification bindings, workflow shape/policy, draft template, install policy, installer presentation, and consumer validator inputs required by Eggpack;
- generate and drift-check the release workflow;
- define the supported binary target matrix and Linux ABI floor evidence;
- make the crates.io package intentional: metadata, licenses, package include/exclude policy, README/CHANGELOG/support text, and explicit library-API support stance;
- require `cargo package --locked` and `cargo publish --locked --dry-run` from a clean tree.

M010A does not publish anything and does not add a self-updater.

## 6. M010B — Bootstrap installers and release artifact qualification

Plan: `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`

Closure: `plans/closure/distribution-release-update/010b-status.md`

Status: closed. The release contract, both public wrappers, and the per-platform
fixture qualification are in place. Runtime qualification of the real release
bytes was the open item; M010D discharged it, and both the workflow's own
`qualify_build_*`/`validate_build_*` jobs and a real installer install of the
real asset now cover it. Static glibc 2.17.0 evidence is confirmed on both
Linux targets.

Primary outcomes:

- product-owned `packaging/install.sh` and `packaging/install.ps1`;
- latest and exact-version selection;
- binary-first installation with Cargo fallback only for unsupported/missing binary;
- manifest/checksum and candidate `--version` validation before placement;
- no internal privilege escalation;
- user-local default installation, with explicit elevated/system behavior only when already running elevated;
- runtime qualification of generated release artifacts on the five required targets;
- Eggpack exact-release bootstrap output used as conformance evidence, not as a substitute for the public wrappers.

## 7. M010C — Eggup self-update and install provenance

Plan: `plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md`

Closure: `plans/closure/distribution-release-update/010c-status.md`

Status: closed.

Dependency discharged. `eggup-curl 0.1.2` **is** published and resolvable; this
was proven with a `cargo generate-lockfile` probe, not a search result. An
earlier check in this session reported it unpublished, and that was correct at
the time — the sparse-index CDN lagged the registry API, so the state genuinely
changed mid-session. The lesson for any later transport work: confirm a
publication by resolving it, because both the index CDN and the API can
disagree with each other and with `cargo search` during propagation.

Primary outcomes:

- add `cargo-cleanme update` / `cargo cleanme update`;
- consume published Eggup primitives rather than copying `gregg-update`;
- prefer the lightweight `eggup-curl` acquisition path once registry-published;
- consume Eggpack ReleaseManifest evidence through `eggup-eggpack` where that keeps release facts single-source;
- validate the staged executable identity exactly before replacement;
- define a truthful install-provenance policy for Cargo-managed, installer-managed, and unknown installations;
- preserve Cargo fallback as cargo-cleanme policy, never as an Eggup transport fallback;
- fail closed when manager ownership cannot be proven safely.

M010C must measure binary-size/dependency impact against the pre-update baseline. If the published curl path is unexpectedly larger or less portable than the native Eggfetch path, record the measurement and choose the smaller qualified path; do not add two production transports without need.

## 8. M010D — Publication, operational polish, and release closure

Plan: `plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md`

Closure: `plans/closure/distribution-release-update/010d-status.md`

Status: closed in both halves. The repository-internal scope — generated
completions and manpages with a CI drift gate, the benchmark baseline with
counters gated and timings not, the operator release checklist, the support
policy, and troubleshooting — landed at `3e5f3ab`.

The publication half completed after the user authorized it and supplied
authentication. `v0.1.0` and `v0.1.1` are public releases; `cargo-cleanme 0.1.0`
and `0.1.1` are on crates.io and neither is yanked. Release workflow run
`37232595211` built, qualified, and validated all five targets green
(20/20 jobs), and the staged draft was validated against the release contract
before publication.

Publication exposed a defect that no test could catch, and it is recorded as
corrective **C011** rather than smoothed over: the `v0.1.0` updater could not
read the version authority at all. See
`plans/closure/distribution-release-update/c011-status.md`.

One requirement was closed partial: the live updater **commit** path has no
end-to-end rehearsal, because `v0.1.1` is both the newest published version and
the first containing a working updater. That gap is now explicitly owned by
post-release corrective C014, with C013 as its publication precondition.

Primary outcomes:

- first crates.io publication and registry-only install verification;
- first binary-bearing GitHub release with checksums, manifest, installers, and required target evidence;
- move subsequent crates.io publication to trusted/OIDC publishing when the registry bootstrap permits it;
- shell completions and manpage generated from the clap command model with CI drift checks;
- release/support documentation and explicit selector/Cargo compatibility policy;
- release benchmark tracking built from existing `--no-progress --stats` and deterministic fixtures;
- smoke installation and self-update from externally resolved release artifacts;
- closure record with source revision, package checksum, release asset inventory, hosted platform runs, known limitations, and rollback/recovery evidence.

## 8A. C013 — Cross-platform fixture premise audit

Plan: `plans/implementation/distribution-release-update/c013-cross-platform-fixture-premise-audit.md`

Status: ready.

C013 closes the low-severity evidence debt left by C012: audit every fixture that substitutes or resolves an external executable, PATH entry, shell/editor command, or platform-specific process and prove that the executable the test intends to exercise is the executable the subject actually resolves. The audit may keep intentionally Unix-only tests platform-scoped; it must not claim cross-platform coverage from a fixture that cannot execute on that platform.

Primary outcomes:

- complete external-process fixture inventory;
- platform-correct PATH/stub semantics;
- premise assertions for substituted tools;
- explicit scoping for deliberately platform-only fixtures;
- no medium-or-higher fixture-evidence finding remaining.

C013 does not reopen Phase 10 and should not change production behavior. A product defect discovered by the audit gets a separate corrective.

## 8B. C014 — v0.1.2 live update and release reproducibility corrective

Plan: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`

Status: blocked on C013 for publication. Toolchain/source-identity preparation may proceed in parallel.

C014 uses v0.1.2 as a deliberately small qualification release. It closes M010D's partial live-updater evidence and the two release-process weaknesses recorded during v0.1.1 publication.

Required outcomes:

- replace floating `rust = "stable"` release inputs with one exact qualified Rust release while preserving Rust 1.89 MSRV;
- mechanically enforce exact tag/version/source identity for crate publication;
- publish v0.1.2 only after C013 closure and the normal Eggpack staged-release gates;
- exercise the released v0.1.1 updater through a real self-managed v0.1.1 -> public v0.1.2 networked commit;
- require the installed post-update binary digest to equal the public v0.1.2 asset digest;
- prove a Cargo-managed v0.1.1 install refuses self-update without mutation and emits the correct v0.1.2 manager remediation;
- make the live smoke repeatable on the supported release platform matrix to the extent claimed.

A failure after immutable publication must be handled by yanking/annotating and a new corrective/version, never by replacing v0.1.2 bytes.

## 9. Verification strategy

Every milestone keeps the existing Stable + Rust 1.89 + Linux/macOS/Windows checks green.

Release-specific verification adds:

- clean-tree package/publish simulation;
- generated-workflow drift checking;
- package-content inspection including embedded `config.toml`;
- exact release artifact inventory validation;
- per-target `--version` and `--help` smoke;
- bounded scan and JSON-output smoke on built release candidates;
- installer negative tests for missing checksum, malformed digest, wrong candidate identity, wrong requested version, unavailable Cargo fallback, existing/foreign destination, and unsupported platform;
- updater fixture tests for current/latest comparison, missing asset, verification failure, transport failure, candidate mismatch, destination ownership failure, interrupted replacement, and recovery receipt;
- no destructive cleanup command should be necessary to qualify distribution.

## 10. Completion definition

Phase 10 is complete when cargo-cleanme can be installed from crates.io or a verified prebuilt release on the required platforms, the public bootstrap wrappers and self-update path share the same release contract, Eggpack and Eggup own the generic producer/consumer mechanisms respectively, release workflow drift is mechanically detected, support limitations are explicit, and the first public release has closure evidence rather than only a successful build.
