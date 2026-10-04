# Distribution, Release, and Update Roadmap

Status: active planning; M010A is ready. Later Phase 10 milestones are dependency-ordered behind the distribution contract and upstream Eggup curl publication.

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
M010A Eggpack distribution contract + package readiness [READY]
        |
        +-----------------------------+
        |                             |
        v                             v
M010B bootstrap installers      Eggup acquisition M009
+ release artifact             eggup-curl 0.1.2 publication
qualification                  [external hard dependency for
        |                       lightweight updater path]
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

Status: ready.

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

Status: blocked on M010A.

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

Status: blocked on M010A and Eggup acquisition M009; integration closure also needs the M010B release convention.

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

Status: blocked on M010A-M010C.

Primary outcomes:

- first crates.io publication and registry-only install verification;
- first binary-bearing GitHub release with checksums, manifest, installers, and required target evidence;
- move subsequent crates.io publication to trusted/OIDC publishing when the registry bootstrap permits it;
- shell completions and manpage generated from the clap command model with CI drift checks;
- release/support documentation and explicit selector/Cargo compatibility policy;
- release benchmark tracking built from existing `--no-progress --stats` and deterministic fixtures;
- smoke installation and self-update from externally resolved release artifacts;
- closure record with source revision, package checksum, release asset inventory, hosted platform runs, known limitations, and rollback/recovery evidence.

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
