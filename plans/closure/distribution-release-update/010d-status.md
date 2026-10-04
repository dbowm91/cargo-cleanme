# Distribution, Release, and Update M010D Status

Plan: `plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md`

Disposition: **blocked** — the publication half cannot be completed, and the
blocker is external and explicit rather than a design gap.

Internal implementation commit: `3e5f3ab` (`docs: add generated completions,
manpages, release checklist, and benchmark baseline (M010D, part 1)`).

Repository baseline: M010A `b4662af`, M010B `34a1d23`, M010C `d25beeb`.

Date: 2026-10-04

## Executive finding

M010D's **repository-internal** scope is complete and verified: generated
completions and manpages with a working drift gate, a benchmark baseline that
separates hard evidence from trend evidence, the operator release checklist, a
support policy that states its own limits, and complete troubleshooting
documentation.

M010D's **publication** scope is not done and cannot be done from here. It
requires two things this session does not have and the user did not
authorize: crates.io registry authentication, and an irreversible public
publication of a crate and a GitHub release. The honest disposition is
therefore `blocked`, with the residual work enumerated precisely below.

Nothing was published, no release was staged, no tag was created, and no
secret was created or stored.

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

## Requirement-to-evidence matrix — publication scope: NOT OBTAINED

| Requirement | Status |
|---|---|
| crates.io release exists and registry-only install succeeds | **not done** — no publication |
| Public GitHub release with exactly the required qualified inventory | **not done** — no release staged |
| Installer and updater point at the same release contract | satisfied structurally (`check-release-contract.py`), not by a real release |
| Runtime qualification of the five release artifacts | **not done** — requires a dispatched release workflow |
| `release-manifest.json` in the release inventory | **not done** — produced by the release run |
| External registry-only and release-only smoke | **not done** — nothing is published to smoke against |
| Updater rehearsal across a real draft release | **not done** |
| No long-lived publication secret retained | satisfied trivially: no secret was created |

## Why this is blocked, specifically

1. **Registry authentication.** Publishing requires a crates.io token or a
   configured trusted-publishing workflow. No such credential exists in this
   environment, and creating or using one is not something to do unprompted.
2. **Publication is irreversible and outward-facing.** A crates.io version and a
   public GitHub release cannot be withdrawn. The request was to implement the
   plans and commit/push, not to publish a package on the user's behalf.
3. **Runtime qualification depends on publication.** §3 of the plan requires
   running the generated release workflow against a real tag to produce a draft.
   That draft is the artifact M010B was to qualify, and qualifying it is a
   precondition for publishing it. The loop cannot be short-circuited: the plan
   forbids publishing a target that lacks runtime evidence, and the evidence
   only exists once a release run happens.

Point 3 is the load-bearing one: even with credentials and authorization, doing
§2-§4 correctly means a multi-step dance (release workflow run → draft →
per-target runtime evidence → external smoke → then publish). Each step's
output is the next step's input, and step 1 requires a real tag on `main`.

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

## Residual work for the next attempt

Ordered, and each step is the input to the next.

1. Obtain crates.io authentication and decide trusted publishing vs. a
   short-lived token. Prefer trusted publishing; do not retain a long-lived
   token.
2. Bump to the release version, update `CHANGELOG.md`, land, and get CI green.
3. Run `scripts/release-check.sh` on a clean tree.
4. Tag the release commit and push the tag.
5. Dispatch `.github/workflows/release-binaries.yml` at that exact tag.
6. Inspect the staged draft: exactly 5 binaries, 5 `.sha256` sidecars,
   `release-manifest.json`, `install-exact.sh`, `install-exact.ps1`, and release
   notes. Verify every digest against the plan.
7. Extract the artifacts and run the per-target runtime qualification that
   M010B left open — in particular the glibc 2.17 ABI floor on both Linux
   targets, which is the last thing preventing that floor from being advertised.
8. Run `docs/RELEASING.md`'s external smoke against the staged artifacts,
   including a `cargo cleanme update` rehearsal from the previous version.
9. Publish the draft, then publish the crate.
10. Re-run the external smoke against the published release, and only then
    update the README to claim the ABI floor.
11. Write the Phase 10 closure record with the exact source revision, tag,
    package checksum, workflow run id, draft and public release ids, full asset
    inventory with SHA-256 values, per-platform installer smoke, updater
    rehearsal, drift results, and the benchmark and binary-size table.

## Known limitations

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

M010D is the final Phase 10 milestone, and it is blocked only on external
authority and credentials, not on any remaining repository work. Every
repository-local deliverable it owns is complete and gated. There is no
follow-on plan in this subsystem to activate; the residual work is a
single named sequence above, gated on a human decision to publish.
