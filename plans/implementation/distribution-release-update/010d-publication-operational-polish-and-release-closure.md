# M010D — Publication, Operational Polish, and Release Closure

Status: closed (internal and publication halves)

Repository baseline: M010A `b4662af`, M010B `34a1d23`, M010C `d25beeb`.

Closure: `plans/closure/distribution-release-update/010d-status.md` (blocked, with the residual sequence enumerated)

Source roadmap: Phase 10 — Distribution and operational polish

Hard dependencies: M010A, M010B, and M010C closed.

## 1. Objective

Perform the first public distribution closure only after package, binary release, installer, and updater contracts are qualified. Add the remaining operational surfaces needed to support a real release without turning cargo-cleanme into its own package manager.

## 2. Crates.io publication

Before publication:

- clean working tree;
- version/tag/release metadata agree;
- `cargo package --locked` succeeds;
- inspect packaged files;
- build/test from the packaged source;
- `cargo publish --locked --dry-run` succeeds;
- verify the crate name/ownership state through the actual registry workflow;
- record the package checksum after publication.

The initial package publication may require the registry's bootstrap/manual authentication path. After the crate exists, configure trusted/OIDC publishing if available and appropriate; do not retain a long-lived token merely for convenience when trusted publishing is supported.

## 3. GitHub release

Run the Eggpack-generated release workflow against an exact source revision and stage the release as a draft first.

Required public release inventory:

- five required cargo-cleanme binaries;
- corresponding checksum evidence;
- `release-manifest.json`;
- product public `install.sh`;
- product public `install.ps1`;
- any Eggpack exact-release bootstrap evidence required by the generated workflow;
- release notes with supported targets and known limitations.

Before publish, validate exact inventory and candidate digests. Publication must not rebuild different bytes after qualification.

## 4. Registry-only and release-only smoke

From a clean external fixture/environment:

- `cargo install cargo-cleanme --locked` resolves only registry dependencies and produces the expected binary;
- direct installer installs the public release binary;
- `cargo-cleanme --version` matches the published version;
- `cargo cleanme --version` works;
- config bootstrap works in an isolated config home;
- bounded read-only scan works;
- JSON output remains one deterministic stdout document;
- self-update can identify already-current state from the public authorities.

Where practical, perform a rehearsal from the previous draft/release version to the newly staged candidate before publication.

## 5. Shell completions and manpage

Generate from the clap command model rather than maintaining syntax manually.

Initial completion targets:

- Bash
- Zsh
- Fish
- PowerShell
- Elvish if the selected clap completion tooling supports it without disproportionate maintenance.

Generate `cargo-cleanme(1)` from the same command model.

Check generated artifacts into a conventional `completions/` and `man/` or `generated/` layout and add a CI regeneration/diff gate. Do not add a runtime completion subcommand unless there is a user need.

## 6. Package-manager integration

Do not make Homebrew/other formulae a Phase 10 closure blocker unless an existing Eggstack mechanism can generate and qualify them cheaply.

At minimum document:

- crates.io source install;
- GitHub installer/direct binary install;
- Cargo-binstall compatibility expectations if its release-asset discovery works against the chosen metadata;
- package-manager/self-update interaction policy.

Never self-update a clearly manager-owned installation behind the manager's back unless M010C explicitly qualified that behavior.

## 7. Benchmark tracking

Use the existing `--no-progress --stats` instrumentation and deterministic fixtures as the release baseline.

Track semantic counters as hard regression evidence where deterministic:

- manifests discovered/resolved;
- Cargo process counts;
- output groups/cleanup units;
- sizing/proof work;
- zero-spawn expectations for filtered/simulated paths.

Track wall-clock phase timings as trend evidence, not tight hosted-run pass/fail thresholds, unless a fixture produces a sufficiently stable bound.

Record release binary sizes for all targets and dependency/feature deltas introduced by the updater.

## 8. Support/release policy

Document:

- MSRV;
- required prebuilt target matrix;
- Linux ABI floor if qualified;
- macOS signing/notarization state;
- Windows signing state;
- Cargo selector exact-version support policy;
- difference between cargo-cleanme application support and fine-grained selector qualification;
- update/install provenance policy;
- security/integrity limitations: SHA-256 is not authenticity/signing;
- release cadence/versioning expectations for pre-1.0.

## 9. Release documentation

Add/finish:

- CHANGELOG entry;
- installation/update/uninstall or manual removal guidance;
- release process/operator checklist;
- target/support matrix;
- crates.io/GitHub release source-of-truth explanation;
- library API stability statement;
- troubleshooting for curl/Cargo fallback and managed installs.

## 10. Verification commands

Run the complete existing gate plus M010A-C focused suites. Repeat package/publish dry-run immediately before publication.

Verify generated completions/manpage are clean by regeneration.

Verify Eggpack release drift check and exact release inventory.

After publication, run external registry-only and release-only smoke from a directory that does not resolve local path/git overrides.

## 11. Acceptance criteria

- crates.io release exists and registry-only install succeeds;
- public GitHub release contains exactly the required qualified inventory;
- installer and updater point at the same release contract;
- completion/manpage drift is checked;
- support limitations are explicit;
- benchmark/release-size baseline is recorded;
- no long-lived publication secret is retained if trusted publishing can replace it;
- closure evidence names exact source revision, package checksum, asset digests, workflow run(s), and known limitations.

## 12. Stop conditions

Do not publish if package bytes differ from reviewed package contents, release artifacts were rebuilt after qualification, any required target lacks runtime evidence, installer/update tests are failing, crate/release version identity differs, or the release process requires bypassing Eggpack's no-clobber/source-binding rules.

## 13. Closure evidence

Create a Phase 10 closure record containing:

- source and release commit/tag;
- crates.io package checksum and registry smoke;
- Eggpack release workflow/draft/public release IDs;
- asset inventory + SHA-256 values;
- installer smoke per platform;
- updater current/update rehearsal;
- completions/manpage drift result;
- benchmark and binary-size table;
- explicit unresolved findings and disposition.

Phase 10 closes only on this evidence; a successful upload alone is not closure.
