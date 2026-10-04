# Artifact Discovery and Cleanup M008C Status

Plan: `plans/implementation/artifact-discovery-cleanup/008c-cargo-profile-package-selective-cleanup-qualification.md`

Disposition: **conditionally closed**

Implementation commit: `d70c18e6374ab4a34ac3acfb0f49292a33c33009` (`Qualify and enable Cargo profile cleanup`).

Date: 2026-10-04

## Decision

Profile-selective cleanup is qualified and enabled. Package-selective cleanup is intentionally deferred to M008D. This is the split explicitly allowed by M008C §21: the roadmap and registry identify the deferred selector and its follow-up instead of claiming package support is complete.

## Real-Cargo qualification matrix

Run with `bash scripts/qualify-cargo-selectors.sh <toolchain> stable` on macOS against generated, disposable workspaces containing app-a, app-b, their shared dependency, dev/release/custom profiles, redirected target output, and (where available) a distinct build directory.

| Cargo | Package selector observation | Profile selectors | `--workspace` | Distinct build-dir | Configured `build.target` package dry-run |
|---|---|---|---|---|---|
| 1.89.0 | app-a removed; app-b/release/custom preserved; shared dependency preserved | release/custom each removed only their observed profile output | unsupported | unavailable | not qualified |
| 1.91.1 | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | unsupported | created | not qualified |
| 1.92.0 | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | unsupported | created | not qualified |
| 1.93.1 | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | supported | created | configured target: 0 paths; explicit target: 44 |
| 1.94.1 | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | supported | created | configured target: 0 paths; explicit target: 18 |
| 1.95.0 | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | supported | created | configured target: 0 paths; explicit target: 18 |
| 1.99.0 (current stable at implementation) | app-a and shared dependency removed; app-b and other profiles preserved | release/custom scopes correct | supported | created | configured and explicit target: 18 paths each |

Package `--dry-run --verbose` was non-mutating and did not provide a stable deletion-path list (Cargo printed a no-files-deleted warning summary). Selector-specific byte attribution cannot be derived from its output. Cargo package cleanup also removes shared dependency outputs in the qualified fixture. The configured target discrepancy reproduced through 1.95 and was absent in 1.99.

## Implemented contract

- `clean --profile NAME` passes the profile symbol to Cargo after full-workspace ownership proof. Runtime Cargo `--version` is queried once per invocation; profile cleanup is enabled only for the qualified 1.89–1.99 range. Unknown/out-of-range versions fail closed with `selector_unsupported` before a Cargo clean command.
- Profile selector estimates serialize as `null`. Whole output-union before/after measurements are explicitly labeled as context. A nonzero minimum-size policy produces `selector_estimate_unavailable` and does not invoke Cargo clean.
- Preview delegates `--dry-run --verbose` to Cargo after proof. Simulate runs no Cargo clean process. Execute invokes Cargo with `--profile NAME` after the unchanged complete ownership and freshness proof.
- `--package NAME` is accepted as a CLI request but returns typed `selector_unsupported` before any Cargo clean spawn. It is not mapped to filesystem paths and no Cargo-private artifact layout is parsed.
- Profile/package selector conflicts and empty selector values are rejected by the CLI.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Generated real-Cargo matrix, including MSRV/current Cargo and intermediate boundaries | `scripts/qualify-cargo-selectors.sh`; matrix above | Pass |
| Profile cleanup delegates to Cargo and works with default/custom profiles | Real-Cargo release/custom matrix; cross-platform real-Cargo profile-preview fixture; fake-Cargo Execute verifies `--profile` argument | Pass |
| Package selector unsupported disposition before mutation | CLI contract test checks typed skip and verifies clean invocation count remains unchanged | Pass; package capability intentionally deferred |
| Shared output/ownership proof remains full-workspace | Existing C003/C004/C006 tests plus M008A excluded shared-owner regression | Pass |
| Selector bytes are not mislabeled; size policy fails closed | JSON fixture asserts selector estimate and `before_bytes` null, output-union context separate; min-size typed skip | Pass |
| Preview/Simulate/Execute command behavior | Real-Cargo Preview fixture; fake-Cargo profile Execute; profile Simulate and no-clean count assertion | Pass |
| Current-Cargo hosted Linux/macOS/Windows and Rust 1.89 gates | GitHub Actions CI run `37176614679` | Pass on Ubuntu, macOS, Windows, and Rust 1.89 |

## Verification

- `rtk bash scripts/qualify-cargo-selectors.sh 1.89 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh 1.91 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh 1.92 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh 1.93 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh 1.94 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh 1.95 stable` — pass.
- `rtk bash scripts/qualify-cargo-selectors.sh stable stable` — pass (Cargo 1.99.0).
- `rtk cargo fmt --check`, `rtk cargo clippy --all-targets --all-features -- -D warnings`, `rtk cargo test --all-targets --all-features` — pass, 197 tests.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — pass.
- `rtk rustup run 1.89 cargo test --locked --all-targets` — pass, 197 tests.
- `rtk git diff --check` — pass.

## Deferred work and handoff

M008D owns package-selector enablement. It must gate package cleanup on a tested Cargo runtime capability, preserve Cargo package-spec semantics for duplicate/ambiguous names, validate package identity against resolved workspace metadata, and repeat the configured-target matrix on the minimum supported package-clean runtime. It must retain unknown selector byte estimates and the zero-minimum-policy restriction unless a stable, non-private accounting contract is demonstrated. M008D is ready after this record lands.

Known limitation: the capability window deliberately fails closed for future Cargo versions until their selector behavior is qualified. The package selector currently skips even on Cargo 1.99. No direct deletion or dependency-artifact ownership inference is performed.
