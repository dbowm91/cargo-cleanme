# Changelog

All notable changes to cargo-cleanme are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with a pre-1.0
stance: the `0.x` line may make breaking changes in any release, and the
command-line, JSON, and release-asset contracts are the stable surface.

## [0.1.0] - 2026-10-04

Initial distribution-ready release candidate. Nothing in this entry is
published yet.

### Added

- Eggpack producer contract under `release/eggpack/`: the static distribution
  contract, build/qualification bindings, per-target consumer validator, GitHub
  runner policy, draft template, install policy, installer presentation, and a
  *derived* reusable workflow shape. The shape is generated from the other
  inputs by `scripts/gen-release-workflow-shape.py`, so it cannot drift into a
  second copy of producer facts.
- Eggpack-generated `release-binaries.yml` release workflow covering
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `x86_64-apple-darwin`, `aarch64-apple-darwin`, and
  `x86_64-pc-windows-msvc`, with a draft-only staging job. The workflow stages
  a draft; it never publishes, clobbers, or requests `id-token`.
- `release-drift.yml` CI job that installs Eggpack at a pinned Git revision and
  fails on either a generated-workflow drift or a hand-edited workflow shape.
- `scripts/check-release-contract.py`, which keeps the product wrappers,
  packaging payload, README target matrix, and Cargo package metadata aligned
  with the Eggpack contract instead of duplicating them.
- `scripts/smoke-release-candidate.py`, a portable per-target release smoke
  (identity, command surface, `cargo cleanme --version` argv normalization, and
  a bounded read-only JSON scan proven not to mutate the scanned tree).
- Explicit Cargo package contract: repository/homepage/documentation metadata,
  keywords, crates.io categories, a deliberate `include` allowlist that
  retains the embedded `config.toml` template, and dual `LICENSE-MIT` /
  `LICENSE-APACHE` files.
- `scripts/release-check.sh`, the single local gate for formatting, lint,
  tests, MSRV, package, and publish dry-run.

### Documentation

- Documented the prebuilt release target matrix, the deliberate Cargo-only
  hosts, and the current installation options.
- Documented the Rust library API stance: the library is published for
  tooling and testability but is **not** a stable third-party API before 1.0.
- Documented SHA-256 as integrity evidence only, not authenticity.

### Added (M010B)

- `packaging/install.sh` and `packaging/install.ps1`: product-owned,
  binary-first, fail-closed public installers. Latest or exact `X.Y.Z`
  selection, mandatory SHA-256 sidecar verification, `--version` identity check
  before placement, re-verification of the placed bytes, user-local default
  destinations, and no internal privilege escalation.
- Cargo source fallback that runs *only* when the host has no published binary
  or the selected release genuinely lacks it. Verification, identity, and
  transport failures never fall back. The fallback builds into a private
  temporary Cargo root, validates the built binary, and cleans up only its own
  state.
- `packaging/tests/`: a local fixture release server and 17 deterministic
  installer cases covering the verified install, exact-version install,
  documented Cargo fallback, and the negative paths for missing checksum,
  malformed digest, tampered payload, wrong product, wrong version, transport
  failure, existing destination, `--force` replacement, unwritable destination,
  malformed version syntax, absent Cargo, an empty Cargo result, and temporary
  state cleanup.
- `scripts/check-installer-contract.py`, which extracts the host-to-target
  mapping each wrapper actually implements and proves it is a projection of the
  Eggpack contract rather than a second release schema. Wired into the release
  drift gate, the release check, and the installer suite.

### Not in this release

- No crates.io publication and no public GitHub release have occurred, so the
  installers have no release to install yet.
- `cargo cleanme update` does not exist yet.
- No runtime release-artifact qualification has run; the exact release bytes
  are qualified when the first release workflow is dispatched.

[0.1.0]: https://github.com/dbowm91/cargo-cleanme/releases/tag/v0.1.0
