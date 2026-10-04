# M008D — Cargo Package-Selector Qualification and Enablement

Status: ready

Repository baseline: M008C closure commit (see `plans/closure/artifact-discovery-cleanup/008c-status.md`).

Source roadmap: Phase 9 — Selective cleanup and policy

Hard dependencies:

- M008A workspace selective cleanup policy — closed.
- M008B machine-readable reporting/automation contract — closed.
- M008C profile/package selector qualification — conditionally closed.

## Objective

Complete Cargo package selection after M008C's real-Cargo evidence exposed two constraints: package cleaning may remove shared dependency artifacts, and configured `build.target` package dry-runs differed from explicit `--target` through Cargo 1.95. M008D must decide and implement an evidence-backed runtime capability window or retain the typed deferred disposition.

## Required behavior

- Keep the complete discovered-manifest, workspace identity, output ownership, activity, marker, authorization, and final combined-universe proof unchanged. Package filtering must never narrow the ownership proof.
- Resolve the requested Cargo package spec against the complete resolved workspace package identity set. Preserve Cargo's accepted spec and duplicate/ambiguous-name behavior; reject invalid/ambiguous requests before `cargo clean`.
- Pass the accepted package spec through Cargo. Never infer package output directories or directly delete artifacts.
- Gate Execute on a centralized runtime Cargo capability model. Unknown, unparseable, or unqualified versions return `selector_unsupported` before Cargo clean.
- Preserve unknown selector-specific bytes. Whole output-union bytes are context only. Any nonzero M008A minimum-size threshold must fail closed unless a trustworthy selector estimate is established.
- Simulate must invoke zero Cargo clean processes. Preview may invoke Cargo dry-run after proof and must not claim its text is a stable deletion manifest. Execute delegates mutation to Cargo exactly once after proof.
- Retain package cleanup's observed shared dependency effects as Cargo behavior; do not promise that only package-unique bytes are reclaimed.

## Qualification and tests

- Re-run the checked-in M008C harness on the chosen minimum supported package-clean runtime, current stable, and versions around any configured-target correctness boundary.
- Include workspace member identity, duplicate package names, renamed/dependency package specs, virtual workspaces, redirected target-dir, distinct build-dir, custom profiles, configured/default targets, and shared dependencies.
- Assert selector dry-run is non-mutating and compare observed execute mutations; all mutations must stay within the complete proven output union.
- Add Preview/Simulate/Execute tests, unsupported/unknown runtime zero-clean tests, package validation tests, size-policy interaction tests, and JSON v1 result tests.
- Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, Rust 1.89 locked check/tests, and hosted Linux/macOS/Windows CI.

## Stop conditions

Do not enable package Execute if package identity is ambiguous, Cargo may mutate outside the complete proven output union, configured target behavior is not bounded for the enabled runtime range, or reporting/minimum-size behavior would imply a package-specific byte amount that cannot be established.

If those conditions cannot be satisfied, close M008D by retaining package selection as deferred, with the matrix and reason codes recorded. Do not weaken the M008A/M008B contracts.
