# Artifact Discovery and Cleanup M008D Status

Plan: `plans/implementation/artifact-discovery-cleanup/008d-cargo-package-selector-qualification-and-enablement.md`

Disposition: **closed**

Implementation commits: `0eb0b67` (`Qualify and enable Cargo package cleanup selectors`) and `f3c49dd` (`Make package cleanup fixture executable check portable`).

Date: 2026-10-04

## Decision

Package-selective cleanup is enabled only for exact Cargo releases 1.98.1 and 1.99.0. The workspace's complete ownership, authorization, activity, marker, output-root and freshness proofs remain unchanged. Package identities are obtained from Cargo metadata and revalidated before mutation. Unknown Cargo versions, unresolved/ambiguous package specs, and selectors with a nonzero minimum reclaimable byte threshold fail closed before a clean process.

Cargo may remove shared dependency artifacts as part of package cleanup. Selector-specific reclaimable bytes remain unknown; complete output-union measurements are context only. No Cargo-private artifact layout is parsed and cargo-cleanme performs no direct artifact deletion.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Preserve complete workspace and output ownership proof while applying a selective package request | Existing C003/C004/C006 proof/freshness suite; package integration execution resolves full Cargo metadata and compares package identity sets at final revalidation | Pass |
| Validate a request against authoritative workspace package identities and reject invalid/ambiguous identities before clean | `WorkspacePackage` metadata; unit coverage for unique name, abbreviated/full name-version, exact Cargo package ID, unknown package, and duplicate-name rejection; fake-Cargo CLI contract verifies no clean invocation for invalid specs | Pass |
| Gate package Execute on qualified Cargo versions and fail closed for unknown releases | Central capability table; unit boundaries include Cargo 1.98.1/1.99.0 support and 1.99.1/1.100.0/prerelease/unknown rejection; fake-Cargo Cargo 1.97 test confirms no clean invocation | Pass |
| Delegate package mutation to Cargo; preserve Preview, Simulate, Execute and minimum-size semantics | Package `--package` argument path; selector mode contracts and zero-clean Simulate tests; dry-run nonmutation in qualification harness; real Cargo Execute integration; nonzero minimum-size selector skip | Pass |
| Report selector bytes honestly and retain shared dependency effects | JSON v1 keeps selector estimate null, labels whole output-union measurements as context, and exposes `selector_invalid`/`selector_unsupported`; real Cargo fixture observes shared dependency outputs removed | Pass |
| Qualify supported package runtime behavior around configured-target boundary | Real-Cargo matrix below; configured `build.target` and explicit `--target` package dry-run outputs agree for 1.98.1 and 1.99.0; prior M008C qualification documents discrepancy through 1.95.0 | Pass for exact enabled releases |
| Hosted Linux/macOS/Windows and Rust 1.89 gates | GitHub Actions run `37178319318` on final implementation commit `f3c49dd` | Pass |

## Real-Cargo qualification matrix

Ran `rtk bash scripts/qualify-cargo-selectors.sh <toolchain>` on macOS using disposable virtual workspaces with app-a, app-b, a shared dependency imported through a renamed dependency alias, dev/release/custom profiles, redirected `target-dir`, and a distinct build directory where supported.

| Cargo | Package mutation | Other package/profile outputs | Shared dependency | Package dry-run | Configured vs explicit target paths |
|---|---|---|---|---|---|
| 1.95.0 (boundary comparator; M008C evidence) | app-a selected | app-b and release preserved | removed | nonmutating | 0 vs 18; not enabled |
| 1.98.1 (minimum enabled) | app-a selected | app-b and release preserved | removed | nonmutating | 18 vs 18 |
| 1.99.0 (current stable) | app-a selected | app-b and release preserved | removed | nonmutating | 18 vs 18 |

For both enabled releases the package fixture reported two removed target paths, and Cargo's dry-run warning was not treated as a stable deletion manifest. Cargo rejected the duplicate-name fixture during authoritative dependency resolution before `cargo clean`; no ambiguous request reached a destructive clean. Exact `app-a@0.1.0` dry-run parsing succeeded. The harness also rechecked release/custom profile boundaries and default-target package cleanup. The crate's platform integration test verifies the selected executable disappears while the sibling executable survives, using the host executable suffix.

## Verification

- `rtk bash scripts/qualify-cargo-selectors.sh 1.98.1` — pass on macOS; package, profile, duplicate-identity, and configured-target fixture matrix.
- `rtk bash scripts/qualify-cargo-selectors.sh stable` — pass on macOS; Cargo 1.99.0 matrix.
- `rtk cargo fmt --check` — pass.
- `rtk cargo clippy --all-targets --all-features -- -D warnings` — pass.
- `rtk cargo test --all-targets --all-features` — pass, 199 tests.
- `rtk rustup run 1.89 cargo check --locked --all-targets` — pass.
- `rtk rustup run 1.89 cargo test --locked --all-targets --quiet` — pass, 199 tests across targets.
- `rtk git diff --check` — pass.
- `rtk gh run watch 37178319318 --exit-status` — pass: Ubuntu, macOS, Windows, and MSRV 1.89.

An initial hosted run (`37178200662`) exposed a Windows-only fixture assertion that omitted `.exe`. The fixture was corrected to use `std::env::consts::EXE_SUFFIX`; the final hosted run above passed all jobs.

## Known limitations and unresolved findings

- Cargo package cleanup may remove shared dependency artifacts; the behavior is surfaced and no package-unique reclaim amount is promised.
- Package selector bytes are unknown. Any nonzero minimum-size threshold skips the selected unit with a typed reason.
- Package support is an exact release allowlist: Cargo 1.98.1 and 1.99.0. Other versions, including future Cargo versions, remain unsupported until separately qualified.
- Duplicate package names are rejected even if version-qualified because Cargo reports duplicate-name ambiguity for the observed clean behavior.
- No unresolved findings remain for M008D. These deliberate restrictions are severity: informational/compatibility limitations.

## Future-plan disposition

M008D was the final registered Phase 9 implementation handoff. Registry review found no subsequent ready or blocked implementation plan whose dependency can be updated by this closure. Phase 10 distribution remains deferred and is not unblocked by package-selector qualification; no future plan status was changed.
