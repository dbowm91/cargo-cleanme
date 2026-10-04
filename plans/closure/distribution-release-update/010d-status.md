# Distribution, Release, and Update M010D Status

Plan: `plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md`

Disposition: **closed** — both the repository-internal and the publication
halves are complete. v0.1.0 and v0.1.1 are published and publicly installable.

Internal implementation commit: `3e5f3ab` (`docs: add generated completions,
manpages, release checklist, and benchmark baseline (M010D, part 1)`).

Publication commits: `8b39313` (v0.1.0, tag), `95bb38e` (v0.1.1, tag), `d478257`
(publication-evidence planning; see the recorded deviation below).

Repository baseline: M010A `b4662af`, M010B `34a1d23`, M010C `d25beeb`.

Date: 2026-10-04

## Executive finding

M010D is closed. Its **repository-internal** scope is complete and verified:
generated completions and manpages with a working drift gate, a benchmark
baseline that separates hard evidence from trend evidence, the operator release
checklist, a support policy that states its own limits, and complete
troubleshooting documentation.

Its **publication** scope is also complete. The user authorized publication and
supplied authentication. `v0.1.0` and `v0.1.1` are public GitHub releases, and
`cargo-cleanme 0.1.0` and `0.1.1` are on crates.io, not yanked, installable
registry-only with zero non-registry sources. All five release targets were
built, qualified, and validated in one green workflow run, and the staged draft
was validated against the release contract before anything was published.

Publication was not a formality. The external smoke of v0.1.0 found a defect
that made `cargo cleanme update` fail on every host, and that defect is
recorded and fixed in corrective C011 rather than smoothed over here.

## Requirement-to-evidence matrix — internal scope

| Requirement | Evidence | Result |
|---|---|---|
| Completions generated from the clap model, not hand-written | `xtask/src/main.rs` builds them from `cargo_cleanme::cli::Cli::command()`; five shells: Bash, Zsh, Fish, PowerShell, Elvish | pass |
| `cargo-cleanme(1)` from the same model | `man/cargo-cleanme.1` plus one page per subcommand (`scan`, `clean`, `config`, `path`, `show`, `edit`, `update`) | pass |
| Conventional layout, checked in | `completions/` and `man/` | pass |
| CI regeneration/diff gate | `generate-docs --check` in the new `generated-docs` job and in `release-check.sh`; verified to fail on both a tampered page and a deleted page | pass |
| No runtime completion subcommand | none added | pass |
| Benchmark uses existing `--no-progress --stats` instrumentation | `scripts/release-benchmark.py` parses the real `scan stats:` line | pass |
| Semantic counters are hard regression evidence | 9 counters gated; `--check` exits 1 on drift, verified by injecting `manifests: 99` | pass |
| Wall-clock timings are trend evidence only | printed and diffed, never gated; two runs of an unchanged tree differ ~10x on the locate phase | pass |
| Zero-spawn expectations for filtered/simulated paths | `locate` and `metadata` process counts are gated counters | pass |
| Release binary sizes and updater dependency deltas recorded | baseline records 5,420,664 bytes and 82 normal dependencies; the +449,752 / +12 delta is in the M010C record | pass |
| Support policy: MSRV, target matrix, selector policy, provenance policy, integrity limits, pre-1.0 cadence | README "Support and release policy" | pass |
| Signing/notarization state documented | new README table: unsigned and un-notarized on all platforms, with the user-visible consequence | pass |
| Release process / operator checklist | `docs/RELEASING.md` | pass |
| Install, update, and uninstall guidance | README "Installing", "Self-update", "Uninstalling" | pass |
| Target/support matrix and source-of-truth explanation | README "Prebuilt release targets"; contract named as the single authority | pass |
| Library API stability statement | README: not a stable third-party API before 1.0 | pass |
| Troubleshooting for curl/Cargo fallback and managed installs | `docs/TROUBLESHOOTING.md` | pass |
| CHANGELOG entry | `CHANGELOG.md` records M010A-M010C plus what deliberately does not exist | pass |
| Package-manager interaction policy documented | README: Cargo-managed installs are refused by `update` by design | pass |
| Homebrew/formulae not a closure blocker | none added; documented as absent | pass |

## Requirement-to-evidence matrix — publication scope: OBTAINED

Publication was authorized by the user and completed. v0.1.0 and v0.1.1 are
both live; the evidence below is v0.1.1 unless stated otherwise.

| Requirement | Status | Evidence |
|---|---|---|
| A GitHub release exists and is public | **done** | `v0.1.1`, `isDraft: false`, `isPrerelease: false`, published `2026-10-04T20:56:31Z`; `v0.1.0` published `2026-10-04T20:21:19Z` |
| crates.io release exists and is not yanked | **done** | `cargo-cleanme 0.1.1`, `yanked: false`, crate checksum `8dfb3d18f64896e81093efaf4ffca3498b7b19f4688597ec93c0832fea76f3c5` |
| Registry-only install succeeds | **done** | `cargo install cargo-cleanme --version 0.1.1 --locked` in an isolated `HOME`/`CARGO_HOME`; `.crates2.json` records `cargo-cleanme 0.1.1 (registry+https://github.com/rust-lang/crates.io-index)` — zero non-registry sources |
| Both command spellings work after a registry install | **done** | `cargo-cleanme --version` and `cargo cleanme --version` both print `cargo-cleanme 0.1.1` |
| Public release carries exactly the required inventory | **done** | 15 assets: 5 contracted binaries, 5 `.sha256` sidecars, `release-manifest.json`, and 4 installer wrappers. `scripts/validate-staged-release.py` fails on any missing *or* extra asset |
| Every sidecar matches the bytes actually served | **done** | all 5 verified against the downloaded bytes, not against the manifest |
| The manifest binds the tag to the exact source revision | **done** | `source_revision 95bb38e2bf2b` ↔ tag `v0.1.1` |
| Contract agreement between installer, updater, and artifacts | **done** | all 5 asset names are exactly the expansions of `release/eggpack/distribution.toml`, including the `.exe` suffix on Windows |
| Static Linux ABI evidence | **done** | both Linux binaries require at most `GLIBC_2.17.0`, read statically from the ELF. Labelled static evidence, not runtime execution on an old distribution |
| Runtime qualification of the real release bytes | **done** | `qualify_build_*` and `validate_build_*` jobs green for all 5 targets in run `37232595211`, plus the validator's own real installer install of the real asset |
| A real installer install of the real asset | **done** | the validator installed `cargo-cleanme-x86_64-unknown-linux-gnu`, it reported `cargo-cleanme 0.1.1`, and the installed bytes matched the release digest exactly |
| External release-only smoke | **done** | `install.sh` fetched over real HTTPS from the public release and installed a binary whose SHA-256 equals the published release digest `96354b5e…` |
| Version-free asset names resolve both ways | **done** | `releases/download/v0.1.1/<asset>` and `releases/latest/download/<asset>` both return HTTP 200 |
| No long-lived publication secret retained | **done** | publication used the operator's pre-existing `gh` and crates.io credentials. No secret was created, stored, or echoed |
| Updater rehearsal across a real release | **partial** | see the known limitation below. The registry read, the provenance check, and the real-asset digest path are all proven live; the commit path is not, and cannot be until a version newer than 0.1.1 exists |

## Publication evidence

**Release workflow run `37232595211` — 20/20 jobs green.** `preflight`,
`resolve`, five `build_*`, five `qualify_build_*`, five `validate_build_*`,
`required_gate`, `aggregate`, `stage`.

**v0.1.1 asset inventory**, as staged, validated, and published:

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-aarch64-apple-darwin` | 10,276,688 | `3a0dbcf0168b9aa9cf63ae28cd693ef54c788e9ba9ff232b2e166c9f1975598d` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7,856,912 | `7582516add97e2f44147a47922f3c496ea0e3a138c8d07ac5838b2921d9215d9` |
| `cargo-cleanme-x86_64-apple-darwin` | 10,709,600 | `37940bdac253cc9435a14a5a5121f39c9f24ba7c157c492acbb5e79e2b6db218` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9,284,608 | `f932126ac0743f983e9cb2a301e04f9e2bcac934c4f60079afd20d6881712abb` |
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8,882,392 | `96354b5ee69da01ed7a308e9e69640b0caa46c2453590f00070863f777cba2ff` |

**Ordering.** The GitHub release was made public *before* the crate was
published to crates.io. crates.io is the version authority the updater reads,
so publishing the crate first would have let a user resolve 0.1.1 and then
fail to download assets that were still a draft. The reverse order has no such
window.

## External smoke — what was actually observed

~~~text
$ cargo install cargo-cleanme --version 0.1.1 --locked
  Installed package `cargo-cleanme v0.1.1` (executable `cargo-cleanme`)

$ cargo-cleanme --version          ->  cargo-cleanme 0.1.1
$ cargo cleanme --version          ->  cargo-cleanme 0.1.1

$ cargo-cleanme update --dry-run        # cargo-managed 0.1.1
cargo-cleanme: self-update failed: already at the latest stable version
(0.1.1; published is 0.1.1)

$ cargo-cleanme update --dry-run        # self-managed 0.1.1
cargo-cleanme: self-update failed: already at the latest stable version
(0.1.1; published is 0.1.1)

$ sh install.sh --dir <prefix>          # real HTTPS, real public release
cargo-cleanme 0.1.1 installed to <prefix>/cargo-cleanme
installed sha256: 96354b5ee69da01ed7a308e9e69640b0caa46c2453590f00070863f777cba2ff
release  sha256: 96354b5ee69da01ed7a308e9e69640b0caa46c2453590f00070863f777cba2ff

$ cargo-cleanme scan --format json --no-progress
{"schema_version":1,"cargo_cleanme_version":"0.1.1","operation":"scan",...}
~~~

The updater reaching `AlreadyCurrent` rather than a 403 is the direct proof
that corrective C011's User-Agent fix works against the live registry. The
identical message from a Cargo-managed and a self-managed install is the proof
that both provenance paths behave as designed.

## The v0.1.0 defect, reproduced live

A real user of v0.1.0 cannot self-update, because the broken updater is in the
version they are running:

~~~text
$ /tmp/smoke/old/cargo-cleanme update          # real v0.1.0 release binary
cargo-cleanme: self-update failed: could not read
https://crates.io/api/v1/crates/cargo-cleanme:
Transport("HTTP 403 from https://crates.io/api/v1/crates/cargo-cleanme")
~~~

This is why v0.1.0 was published rather than yanked, and why 0.1.1 could not be
reached by `update` from 0.1.0. Users of 0.1.0 upgrade with
`cargo install cargo-cleanme --locked --force` or the installer. The defect is
confined to one subcommand, fails closed with an actionable message, and does
not affect the product's actual function.

## Deviation from the publication precondition — recorded, not glossed

The crate was published from `d478257`, not from the tagged `95bb38e`. The
precondition is that the crate is built from the exact tagged commit, so this
is recorded rather than quietly passed over.

Verified to be benign, with evidence:

- The published `.crate` was downloaded back from crates.io and unpacked. It
  contains 26 files.
- Unpacking `cargo package` run at the tagged `95bb38e` and diffing the two
  trees recursively produces **exactly one** difference: `.cargo_vcs_info.json`,
  which records `d478257` in the published copy and `95bb38e` in the tagged
  one. Every other file is identical, and the file inventories match exactly.
- The reason is that `d478257` differs from `95bb38e` only in `plans/**` —
  five planning documents, none of which is in the package `include` allowlist.
  The code, docs, completions, and manpages shipped are the tagged commit's
  bytes.
- The release *binaries* were built by run `37232595211` from the tag, and
  `release-manifest.json` binds them to `95bb38e`. So the binaries and the
  crate agree on content; only the crate's VCS provenance stamp points one
  planning-only commit later.

The process correction is to publish from the tagged commit with no
subsequent commits, or to amend the tag. Recorded here so the next release
does not repeat it.

## Known limitations

- **The updater's commit path has no live evidence.** The version authority
  read, the provenance decision, the sidecar fetch, the asset fetch, and the
  digest match are all proven live or by real-asset install. The
  fetch → verify → commit transaction is covered by fixture tests against the
  real Eggup commit machinery, but not by a real network transaction. The
  structural reason is specific: 0.1.1 is simultaneously the newest published
  version and the first one containing a working updater, so there is no
  installed build that both has a working updater and has a newer release
  available. Rehearsing it requires a v0.1.2.
- **No automated live smoke.** The updater check above is manual and was run by
  an operator. Adding CI that reaches a real registry and mutates a real
  installation is a different risk class and was not in scope.
- **`pack.toml` pins Rust as `stable`, not an exact version.** Re-running a
  release for the same tag can therefore produce different bytes, so "do not
  rebuild after qualification" has no reproducibility guarantee behind it.
  This is unchanged from 0.1.0 and is noted in `docs/RELEASING.md`.
- **Nothing is signed.** `.sha256` sidecars prove the bytes match the release,
  not who produced them. There is no signature, notarization, or transparency
  log. Unchanged and stated in `docs/TROUBLESHOOTING.md`.
- **The release binary more than doubled** with the 0.1.1 transport fix, from
  5,420,664 to 12,125,624 bytes. See the C011 closure record.

## Defects found while building the internal scope

Three, all in the benchmark harness, all of which would have produced a
meaningless baseline:

1. **`locate` and `metadata` are ambiguous.** Each appears twice in the `scan
   stats:` line — once as a Cargo process count, once as a phase duration. A
   single key/value map let the duration overwrite the count, so a hard counter
   was recorded as a float and `--check` reported spurious drift on every run.
   Counters and timings are now parsed in separate passes with distinct names.
2. **Config isolation broke Cargo.** Isolating the app config with `HOME` or
   `XDG_CONFIG_HOME` overrides made this Cargo build emit nothing for
   `locate-project`, so every fixture project was reported as uncertain and the
   baseline recorded `failures: 2`. The app config is now isolated with
   `--config`, which leaves Cargo's own configuration inputs untouched. The
   first baseline was measuring a broken scan, not the real one.
3. **The ABI-floor guard forbade the honest explanation.** The README's
   non-claim statement mentions "glibc 2.17", which the previous guard rejected
   outright. The invariant is now "mentioning the floor requires an explicit
   non-claim nearby", so the document may explain *why* the floor is not
   advertised yet while still failing if it starts being advertised.

## The generation drift gate is real, not decorative

It was verified by deliberately breaking the tree three ways and confirming the
check failed each time, then confirming it passed after regeneration:

| Tamper | `--check` exit |
|---|---|
| a `\n.TH TAMPERED` line appended to `man/cargo-cleanme-update.1` | 1 |
| `man/cargo-cleanme-scan.1` deleted | 1 |
| semantic counter drifted to `manifests: 99` | 1 |
| none | 0 |

Both new gates are also hosted: the `generated-docs` and `benchmark` jobs in
CI run `37228509633` on `19bd128` are green, alongside the five pre-existing
lanes, so all nine jobs pass.

One earlier attempt to tamper appeared to pass, which was a flaw in the *test*
of the gate rather than in the gate: the string `"dry-run"` does not appear
literally in roff output, so the replacement was a no-op. That is recorded
because "the drift gate passed" is only meaningful once it has been shown to
fail.

## The publication sequence, as it actually ran

The plan's ordered sequence is preserved because it worked, with the one
reordering noted and recorded.

1. Bump the version, update `CHANGELOG.md` and the docs, land, get CI green.
2. `scripts/release-check.sh` on a clean tree.
3. Tag the release commit and push the tag.
4. Dispatch `.github/workflows/release-binaries.yml` at that tag.
5. Inspect the staged draft: exactly 15 assets, every digest verified against
   the bytes actually served, and `release-manifest.json` binding the tag to the
   exact source revision.
6. Per-target runtime qualification by the workflow's own `qualify_build_*` and
   `validate_build_*` jobs, plus static glibc 2.17.0 evidence on both Linux
   targets.
7. `scripts/validate-staged-release.py` against the staged draft — including a
   real installer install of the real asset.
8. **Reordered:** make the GitHub release public, *then* publish the crate, so
   crates.io never advertises a version whose assets are not yet downloadable.
9. Re-run the external smoke against the published release.
10. Record the evidence above.

Step 7 is the one the 0.1.0 release should have run and did not. The staged
draft existed and was not smoke-tested, and the defect shipped. That is now
written into `docs/RELEASING.md` as a requirement rather than a suggestion.

## Known limitations — product surface

- `install.sh` is POSIX `sh` and is exercised on Linux and macOS only.
- The `armv7` family is recognized and routed to Cargo but is not yet a
  configurable Cargo-fallback host.
- No musl targets, Windows ARM64 binaries, ARMv7 binaries, package-manager
  formulae, or `.deb`/`.rpm` packages exist.
- macOS binaries are un-notarized and Windows binaries are unsigned, so both
  platforms will show a first-run security warning for a downloaded binary.
  This is documented rather than solved; it needs an Apple Developer identity
  and a Windows code-signing certificate respectively, both of which are
  external.
- The published crate's Rust library is not a stable third-party API before
  1.0. This is stated in the README and the changelog.

## Future-plan review

M010D was the final Phase 10 milestone and is now closed in full. The
publication half that was blocked on external authority was authorized and
completed.

Two follow-ups are registered rather than left implicit:

- **C010** — the bounded upstream request for a `User-Agent` seam on
  `eggup-curl`, status `proposed`. It is an upstream matter; nothing in this
  repository waits on it.
- **C011** — the corrective for the v0.1.0 updater defect, status `closed`.

The next release should be rehearsed by publishing v0.1.2 and running a real
`cargo cleanme update` from a v0.1.1 self-managed install. That is the one
piece of Phase 10 evidence that no amount of local testing can substitute for,
and it needs a newer published version to exist.

## Disposition

**closed.** Both the internal and the publication halves of M010D are complete.
v0.1.0 and v0.1.1 are published, publicly installable from both the registry
and the release, and their artifacts are qualified and digest-verified.

The publication half is closed on a partial for one requirement only — the
live updater commit-path rehearsal — and that limitation is stated above with
its specific structural reason and its specific remedy. It does not block
closure, because the transaction machinery it would exercise is the published
Eggup crates' and is covered by fixture tests; what is unproven is the
network path to them.

The v0.1.0 defect and its corrective are recorded in
`plans/closure/distribution-release-update/c011-status.md`, not resolved here
by editing M010C's history.
