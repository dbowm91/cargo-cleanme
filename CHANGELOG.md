# Changelog

All notable changes to cargo-cleanme are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with a pre-1.0
stance: the `0.x` line may make breaking changes in any release, and the
command-line, JSON, and release-asset contracts are the stable surface.

## [0.1.5] - 2026-10-05

This release fixes the only defect in the 0.1.x line that could **change a file
it did not own**.

### Fixed

- **`cargo cleanme update` could replace a binary that Cargo owns.** A binary
  installed with `cargo install --root DIR` — the form every hermetic install
  script, CI job, and container image uses — was not recognised as
  Cargo-managed, because detection only ever looked at `$CARGO_HOME/bin`. The
  binary was therefore treated as a self-managed installation and **overwritten
  in place**, with no warning and exit status `0`. Afterwards `cargo install
  --list` reported the old version for a file that no longer had it, so
  `cargo upgrade` and `cargo uninstall` could no longer do their jobs.

  Reproduced against published v0.1.3:

  ```sh
  cargo install cargo-cleanme --version 0.1.3 --locked --root /tmp/root
  /tmp/root/bin/cargo-cleanme update --no-progress
  # cargo-cleanme: updated to 0.1.4      (exit 0; the file is now the release asset)
  # /tmp/root/.crates.toml still records 0.1.3
  ```

  `cargo cleanme update` on such an installation now refuses, leaves the bytes
  untouched, and prints the command that is actually correct:

  ```sh
  cargo install cargo-cleanme --locked --force
  ```

- **Cargo-managed installations were refused for the wrong reason.** The
  `.crates.toml` parser read a `[packages.<name>].vers` schema that no current
  cargo emits; cargo writes a flat `[v1]` table keyed by the full spec string. The
  parse therefore always failed, and a genuinely Cargo-managed binary was told to
  "reinstall with the published installer" — the exact provenance confusion this
  check exists to prevent.

  In this release a current Cargo-managed binary that is already up to date still
  reports "already at the latest stable version" instead of a refusal. Nothing is
  downloaded or written in that case, so it is safe; it is noted rather than
  changed.

### Known issue in earlier releases

v0.1.0 through v0.1.4 all carry both defects above. A Cargo-managed installation
on those versions can be silently replaced by `cargo cleanme update`. Upgrading
to 0.1.5 fixes it; `cargo install cargo-cleanme --locked` is the safe way to
move, since that route never overwrites a file it does not own.

## [0.1.4] - 2026-10-05

This release changes no product behavior. It exists so that the self-update
**commit** path can be exercised against two real published releases.

`0.1.3` fixed the updater, but proving a fix requires a released binary that
carries it *and* a newer published version to update to. Until 0.1.3 there was
no such pair, which is why the defect survived: the live commit path had never
succeeded in any published version, and no fixture could see it. 0.1.4 supplies
the missing half of that pair.

If you are on 0.1.3 or later, you can now reach this release with:

```sh
cargo cleanme update
```

## [0.1.3] - 2026-10-05

This release carries two defect fixes found after v0.1.2 was published. Both were
found by the post-release corrective line's own evidence work, not by a user
report: the relative-root defect by a release validator that had been green on
every CI lane, and the self-update defect by the live updater rehearsal.

### Fixed

- **`cargo cleanme update` could never complete a real commit.** The identity
  check ran the downloaded candidate with no arguments, so it compared the
  binary's default routine-scan report against a version string that can never
  match. The check therefore always failed — and, before failing, performed a
  filesystem scan of the whole machine. The transaction aborted safely every
  time: the live binary was left untouched and the error named the reason, so
  nothing was ever corrupted. But the feature had never worked in any published
  version, including `0.1.2`.

  Fixed by invoking the candidate with `--version`, which is bounded and
  produces exactly the identity the validator requires.

  The commit-path test fixture could not detect this: its candidate stub printed
  the identity for *any* invocation, so it could not tell a correct `--version`
  call from the argv-less one production actually made. The stub now answers only
  for `--version` and writes to stderr otherwise, exactly as the real binary
  does, so dropping the argument again turns the commit-path tests red. A fixture
  that accepts every input asserts nothing about the input.

- **A relative scan root resolved zero Cargo workspaces.** `cargo cleanme scan
  some/dir` reported no groups and exited 0, because the relative manifest path
  was passed to Cargo with the child's working directory set to that manifest's
  own parent, so the path did not resolve. The absolute spelling of the same
  directory was correct. The failure direction was fail-safe — a degraded scan
  reports *less* reclaimable space, so `clean` deleted less, never more — but it
  could read as a false all-clear.

  Relative and absolute roots now produce identical counters, groups, and bytes.
  The workaround noted in the `0.1.2` notes is no longer needed.

### Upgrading

Self-update works from this release onward. If you are on `0.1.1` or `0.1.2`,
`cargo cleanme update` cannot reach any version, so upgrade with Cargo:

```sh
cargo install cargo-cleanme --locked --force
```

or download the `0.1.3` asset for your platform from the release page.

## [Unreleased]

- **`cargo cleanme update` could never complete a real commit.** The identity
  check ran the downloaded candidate with no arguments, so it compared the
  binary's default routine-scan report against a version string that can never
  match. The check therefore always failed — and, before failing, performed a
  filesystem scan of the whole machine. The transaction aborted safely every
  time: the live binary was left untouched and the error named the reason, so
  nothing was ever corrupted. But the feature had never worked in any published
  version, including `0.1.2`.

  Fixed by invoking the candidate with `--version`, which is bounded and
  produces exactly the identity the validator requires.

  The commit-path test fixture could not detect this, and the reason is worth
  recording: its candidate stub printed the identity for *any* invocation, so it
  could not tell a correct `--version` call from the argv-less one production
  actually made. The stub now answers only for `--version` and writes to stderr
  otherwise, exactly as the real binary does, so dropping the argument again
  turns the commit-path tests red. A fixture that accepts every input asserts
  nothing about the input.

  Tracked as C016. The live rehearsal that proves the fix requires a published
  release carrying it; see §8 of that plan for why a `0.1.2` -> `0.1.3`
  rehearsal is expected to fail by design.

## [0.1.2] - 2026-10-04

This is a deliberately small qualification release. It changes no product
behavior: every change is to test evidence, release reproducibility, and the
release record. It exists so the live self-update path can be rehearsed against
a real public release, which was impossible while `0.1.1` was both the newest
publication and the first version containing a working updater.

### Changed

- The release builder is pinned to one exact Rust release, `1.99.0`, on all five
  contracted targets. It was `stable`, so each rebuild of a tag could have used a
  different compiler. `1.99.0` is the version that built and qualified `v0.1.1`,
  so the pin records an already-proven input rather than changing one. This is
  **not** an MSRV change: the declared MSRV remains `1.89`.

- A release tag is now bound to the source identity in the tree. The tag's
  version, `Cargo.toml`, `Cargo.lock`, the revision the tag points at, and a
  changelog entry must all agree before a release is staged. A stale or mistyped
  tag previously resolved cleanly and would have staged a draft whose tag and
  asset names disagreed with the version the binary reports.

- `scripts/release-check.sh` accepts the release tag as an argument and runs the
  identity gate, so the pre-publication gate and the tag are checked together.

### Fixed (test and release evidence only)

- The release-candidate smoke validator accepted a scan that discovered zero
  manifests and ignored diagnostics, so it reported a healthy candidate while the
  product's own Cargo integration was failing. It now asserts the exact manifest
  count, rejects any Cargo diagnostic, and probes the real Cargo first so a
  broken lane is reported as a lane failure rather than a product failure.

- The installer fixture cases for a missing binary, a missing Cargo, and a Cargo
  run that produces nothing asserted only a non-zero exit. A dead fixture server
  or an unrelated failure satisfied them. They now assert the wrapper's own
  diagnostic, so they prove the branch under test is the branch that ran.

- The two `cargo cleanme` cases no longer assume that Cargo resolves the binary
  they staged on PATH. The staged binary is resolved and compared before the
  behaviour assertions, and the resolution probe reads the child's environment
  rather than the test process's.

- A case that could not run — an unwritable-directory case running as root — is
  now reported as a skip instead of `ok`, and a host on which no installer block
  can run exits non-zero rather than reporting a green run that qualified
  nothing.

- A release validator with no `readelf` available no longer lets the glibc floor
  pass as "no version info found", and a failed `cargo tree` no longer records a
  dependency count of zero.

### Known limitations

- A known defect is **not** fixed in this release: an explicitly relative scan
  root (`cargo-cleanme scan some/dir`) silently resolves zero Cargo workspaces
  and reports no groups, because the relative manifest path is passed to Cargo
  with the child's working directory set to that manifest's own parent. The
  absolute spelling of the same directory is correct. The failure direction is
  fail-safe — a degraded scan reports *less* reclaimable space, so `clean` deletes
  less, never more — but the result can read as a false all-clear. Tracked as
  C015; use an absolute path until it is fixed.

## [0.1.1] - 2026-10-04

### Fixed

- `cargo cleanme update` could not reach the version authority. The production
  transport was `eggup-curl`, whose `CurlConfig` in 0.1.2 exposes no
  User-Agent seam and whose adapter clears the child environment, so every
  request went out as `curl/x.y`. The crates.io registry answers **HTTP 403** to
  any non-descriptive User-Agent, so `update` always ended in a transport
  failure. It now reports already-current correctly against the live registry.

  This was found by the 0.1.0 release smoke, not by the test suite: the fixture
  suite never talks to the registry, so no unit or fixture test could have
  caught it. The 19 updater tests were all green while the feature was
  non-functional in production.

  The transport is now `eggup-eggfetch`, the already-published Eggup transport
  whose `EggfetchConfig` has the `user_agent` seam the requirement needs. The
  curl path is not kept as a fallback: a fallback that cannot reach the
  authority is a second way to fail for no benefit.

- `cargo cleanme update` could offer to **downgrade** a build that is newer than
  the published release. Once 0.1.1 was published, a 0.1.1 install would have
  compared 0.1.1 against 0.1.0, taken the "newer version available" branch, and
  replaced itself with the older binary. The registry is the version authority
  for releases, not for a build that was never published, so a build newer than
  the published stable version now stops with a refusal instead of an offer.

  This was also found by the release smoke, for the same reason as the 403: the
  fixture suite drives a transport that always answers, so it could not express
  a registry that reports an older version than the one running.

- The release consumer validator now runs on every CI lane. It previously ran
  only inside the release workflow, so a Windows-only defect in the validator
  stayed invisible until a release was attempted.

### Changed

- The self-update transport is now an embedded HTTP/TLS stack rather than the
  external `curl` executable. This is a real cost and is not hidden:

  | Measurement | 0.1.0 (curl) | 0.1.1 (eggfetch) |
  |---|---|---|
  | Release binary | 5,420,664 bytes | 12,125,624 bytes |
  | Normal dependencies | 82 | 177 |

  `curl` was originally chosen because it is much smaller, and that reasoning
  was correct about size and wrong about qualification: a transport that cannot
  perform the required transaction is not a cheaper transport, it is a broken
  one. The original footprint argument should not have been used to close the
  question before a live registry request had ever been made.

### Known limitations

- `update` requires no external HTTP client now, but the binary is roughly twice
  the size it was. On a size-constrained platform this is the wrong trade and
  the updater should be a separate optional binary instead; that is registered
  as a follow-up, not resolved here.
- The bounded upstream gap is recorded rather than worked around: `eggup-curl`
  0.1.2 cannot set a User-Agent, so curl cannot be a viable production
  transport for any registry-backed updater. That request is named in the M010C
  closure record.

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

### Added (M010C)

- `cargo cleanme update`: bounded self-update built on the published Eggup
  crates. The registry is the version authority, the release tag is
  constructed as `vX.Y.Z` rather than scraped, and the candidate is replaced
  only after its `.sha256` sidecar verifies, the sidecar is confirmed to be
  evidence for that asset, and the candidate prints exactly `cargo-cleanme
  X.Y.Z`.
- Install provenance policy: a Cargo-managed installation is refused with the
  exact `cargo install cargo-cleanme --locked --force` command, because Cargo
  owns that file and its bookkeeping. Any other installation must be proven
  owned by its exact prior SHA-256, re-verified under a mutation lock
  immediately before replacement. Unprovable ownership fails closed.
- `eggup-curl` is the single production transport, so the updater adds no
  embedded HTTP/TLS stack. Its cost is measured: the release binary grows from
  4,970,912 to 5,420,664 bytes (+449,752, +9.0%) with three new crates and
  `eggup-acquisition` contributing zero transitive dependencies. The
  `eggup-eggfetch` alternative would add `eggfetch-core` plus rustls and was
  rejected on that measurement.
- 19 fixture-driven updater tests covering the version authority, tag
  construction, already-current, registry outage, all three provenance
  states, a successful verified replacement, checksum mismatch, wrong candidate
  identity, a sidecar naming a different asset, an absent asset, and staging
  cleanup on both the success and failure paths.

### Fixed

- The candidate was staged with `PermissionsIntent::Preserve`, which carries a
  freshly downloaded non-executable artifact through as non-executable. Every
  real update would have failed identity validation with a permission error.
  The intent is now `Executable`, which Eggup applies at staging time.

### Not in this release

- No crates.io publication and no public GitHub release have occurred. Because
  `cargo-cleanme` is not published, `cargo cleanme update` currently reports
  that the registry has no stable version, which is the correct fail-closed
  behavior rather than a defect.
- Release-manifest projection via `eggup-eggpack` is deferred: every target is
  a single direct artifact, so the tag plus the asset plus its sidecar digest
  is the whole update input. A real release workflow run is still required to
  prove end-to-end agreement with what Eggpack stages.
- No runtime release-artifact qualification has run; the exact release bytes
  are qualified when the first release workflow is dispatched.

[0.1.0]: https://github.com/dbowm91/cargo-cleanme/releases/tag/v0.1.0
