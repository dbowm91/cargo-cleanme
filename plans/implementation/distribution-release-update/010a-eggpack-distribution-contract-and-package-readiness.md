# M010A — Eggpack Distribution Contract and Package Readiness

Status: closed

Repository baseline: `78aac6d` (the plan's stated `85b5d4a` is an earlier planning baseline)

Closure: `plans/closure/distribution-release-update/010a-status.md`

Source roadmap: Phase 10 — Distribution and operational polish

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Hard dependency: Phase 9 / C008 closure — satisfied.

External interface dependency: current `eggstack/eggpack` producer contracts and generated-CI surface. No Eggpack production change is authorized by this plan.

## 1. Objective

Make cargo-cleanme release-ready without publishing it yet. Establish Eggpack as the single producer authority for binary release layout and generated release CI, and make the Cargo package intentionally publishable rather than relying on repository defaults.

## 2. Current evidence

At the baseline:

- the crate is version 0.1.0, Rust 1.89, edition 2024, and already supports both direct and Cargo external-subcommand invocation;
- ordinary CI covers Stable Linux/macOS/Windows plus Rust 1.89 on Linux;
- no GitHub binary release workflow exists;
- no release assets or tags have been published;
- Cargo metadata lacks a complete public package/distribution contract;
- `src/config.rs` embeds repository-root `config.toml` with `include_str!("../config.toml")`, so package filtering must retain that file;
- Eggpack already supports direct-asset contracts, Cargo/cargo-zigbuild planning, qualification bindings, generated release CI, release manifests, public-wrapper composition, draft staging, and drift checking through existing real consumers.

## 3. Invariants

- Do not change cleanup eligibility, ownership proof, selector allowlists, or destructive behavior.
- Do not add cargo-dist as a parallel production release authority.
- Release filenames, target aliases, install identity, checksums, and generated workflow must derive from Eggpack inputs.
- Generated workflow changes must be reproducible and drift-checked.
- Release staging must bind an exact source revision.
- SHA-256 is integrity evidence only.
- No publication occurs under M010A.

## 4. Production/package changes

Add a cargo-cleanme Eggpack configuration surface, expected under `release/eggpack/`, modeled on the qualified Eggsact direct-release consumer:

- `distribution.toml`
- `pack.toml`
- `build-bindings.toml`
- `qualification-bindings.toml`
- `consumer-validators.json` where product-level runtime smoke adds value
- `github-policy.json`
- `github-template.json`
- `install-policy.toml`
- `installer-presentation.json`
- `workflow-shape.json`

Generate/check in the Eggpack release workflow and a drift guard. Pin the Eggpack tool by immutable Git revision in the workflow policy unless/until its CLI has a separately qualified registry publication.

Required target contract:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Use direct artifacts with version-independent release filenames. Evaluate cargo-zigbuild for the two Linux targets and record whether a glibc 2.17 floor is actually qualified. Do not advertise an ABI floor merely because it appears in configuration.

## 5. Cargo package readiness

Reconcile `Cargo.toml` for public publication:

- repository/homepage/documentation fields as appropriate;
- useful keywords/categories, including Cargo-plugin/command-line classification;
- explicit package include/exclude policy;
- dual-license text files matching the existing SPDX expression;
- README packaging;
- CHANGELOG;
- support/release policy.

The package allowlist/exclude policy must include every compile-time input, especially root `config.toml`, and exclude repository-only planning/closure/tooling material unless intentionally user-facing.

Inspect the resulting `.crate` contents rather than assuming Cargo filters are correct.

Document the Rust library support stance before publication. The existing public modules were introduced for testability; M010A must not silently promise a stable third-party library API. Prefer documenting the pre-1.0 API as unstable unless a deliberate API stabilization pass is separately justified.

## 6. Ordered work packages

1. Freeze the release target/asset/install-name contract in Eggpack inputs.
2. Define build and qualification strategies per target.
3. Add exact runtime smoke bindings: at minimum `--version`; add `--help`, bounded read-only scan, and JSON smoke where practical without filesystem side effects.
4. Generate the release workflow from Eggpack and add drift checking.
5. Reconcile Cargo package metadata, license files, changelog, support text, and package file selection.
6. Run `cargo package --locked` and inspect package contents.
7. Build/test the packaged source, not only the repository checkout.
8. Run `cargo publish --locked --dry-run`.
9. Re-run normal Stable/MSRV/platform CI.
10. Record exact Eggpack revision, generated workflow digest/diff result, package file inventory, and target contract in closure evidence.

## 7. Compatibility effects

No CLI behavior should change except release/documentation metadata.

The package remains installable as `cargo-cleanme`, giving Cargo the external `cargo cleanme` entry point through its existing argv normalization.

Additional target families are out of scope unless they can be added without weakening qualification. ARMv7 remains a potential Cargo/source-fallback platform rather than an implied prebuilt target.

## 8. Tests and verification commands

At minimum:

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
cargo package --locked
cargo publish --locked --dry-run
~~~

Also run the checked-in Eggpack CI drift command against the generated workflow and exercise the generated release plan for all five required targets.

## 9. Documentation updates

Update README installation/release sections only to describe capabilities actually established by this milestone. Public one-curl installer and `update` commands belong to M010B/M010C and must not be advertised early.

## 10. Acceptance criteria

- Eggpack is the sole binary-release layout/workflow authority.
- Five required targets have explicit build + qualification policy.
- Generated release workflow is checked in and drift-guarded.
- Cargo package metadata and license material are complete.
- Packaged source compiles/tests and includes root `config.toml`.
- `cargo publish --dry-run` succeeds from a clean tree.
- No release or crates.io publication has occurred.
- Existing cleanup and CI contracts remain green.

## 11. Stop conditions

Stop rather than publish or weaken checks if:

- package contents omit a compile-time input;
- Eggpack cannot express the required direct-release contract without a producer change;
- a required target cannot produce/execute the candidate under its declared support policy;
- Linux ABI-floor evidence is unavailable;
- package metadata would imply a stable library API that has not been intentionally accepted.

If an Eggpack primitive is genuinely missing, author a bounded Eggpack upstream plan rather than implementing parallel producer machinery in cargo-cleanme.

## 12. Closure evidence

Record implementation commit, exact Eggpack pin, generated workflow path/digest, target matrix, package file list, `cargo package`/publish-dry-run results, hosted CI runs, ABI-floor evidence, and any deferred target/package-manager work.
