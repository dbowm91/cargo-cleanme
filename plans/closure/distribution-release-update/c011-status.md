# Distribution, Release, and Update C011 Status

Plan: `plans/implementation/distribution-release-update/c011-self-update-transport-qualification-corrective.md`

Disposition: **closed**

Corrects: `plans/closure/distribution-release-update/010c-status.md`
(implementation `d25beeb`), whose transport-selection requirement is recorded
here as **not satisfied at the time it was written**.

Implementation commit: `95bb38e2bf2b335211bff4498089016f6d4239eb` (tag `v0.1.1`)

Repository baseline: `8b393135bbaff41c553207dcb198fb8bf43f5533` (tag `v0.1.0`)

Date: 2026-10-04

## Executive finding

Two defects made `cargo cleanme update` fail in production while its entire
test suite was green. Both were found by the 0.1.0 release smoke, and neither
could have been found by a test.

The first is fatal to the feature. M010C selected `eggup-curl` 0.1.2 and
recorded that choice as a satisfied requirement. `eggup-curl` cannot set a
`User-Agent`, and crates.io answers HTTP 403 to any non-descriptive agent, so
`update` could never read the version authority on any host. The second was
latent behind the first: `update` had no version ordering, so the moment 0.1.1
was published it would have offered every 0.1.1 install a "downgrade" to 0.1.0.

Both are fixed, both are covered by tests that assert the requirement rather
than the preference, and the live behavior is now verified against the real
registry.

The footprint cost is real and is recorded rather than minimized: the release
binary goes from 5,420,664 to 12,125,624 bytes and from 82 to 177 normal
dependencies, because the working transport is an embedded HTTP/TLS stack.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| The transport can read the version authority | `USER_AGENT` is `cargo-cleanme/0.1.1 (+https://github.com/dbowm91/cargo-cleanme)`, set via `EggfetchConfig::strict().user_agent(...)`; **verified live**: `update --dry-run` against crates.io returns a registry answer, not a 403 | pass |
| The requirement is asserted, not the preference | `the_production_transport_identifies_itself_descriptively` asserts the *wired* transport's effective agent names the product and is neither empty nor a bare tool name. It would fail for any transport that cannot satisfy the requirement, including curl | pass |
| No downgrade | `UpdateError::NewerThanPublished`; `a_newer_local_build_is_never_downgraded` drives `0.1.1`-running against `0.1.0`-published and asserts the refusal | pass |
| Equal versions are already current, not an offer | `an_older_published_version_is_not_offered_to_a_matching_build` | pass |
| Version ordering is numeric | `versions_order_numerically_not_lexically`: `0.1.10 > 0.1.9`, `0.10.0 > 0.9.9`, equal, less, and `None` for `1.2` vs `1.2.0`, `1.2.3-rc.1` vs `1.2.3`, `x` vs `1.2.3` | pass |
| An unorderable pair is refused, not guessed | the `None` arm in `run` returns `NoPublishedStableVersion` | pass |
| Exactly one production transport | `eggup-eggfetch` only; `eggup-curl` is absent from `Cargo.toml`, `Cargo.lock`, and the tree. A curl fallback is deliberately **not** retained | pass |
| Transport unavailability stays typed | `UpdateError::TransportUnavailable`; the message no longer names a specific tool | pass |
| Footprint is measured and recorded, not hidden | `release/baseline-benchmark.json` records 12,125,624 bytes and 177 dependencies; `CHANGELOG.md` and `Cargo.toml` both carry the delta and the reasoning | pass |
| Docs and generated artifacts match the new behavior | `docs/TROUBLESHOOTING.md` gains entries for the unreachable-registry and newer-than-published cases; `generate-docs --check` reports 13/13 artifacts in sync | pass |
| The build itself is sound | `bash scripts/release-check.sh` passes end to end on the clean tree at `95bb38e`: fmt, clippy `-D warnings`, 228 lib + 7 + 1 tests, doc tests, MSRV 1.89 check and test, benchmark counters, docs drift, Eggpack workflow-shape derivation and drift, release contract, installer contract, 17 installer fixture cases, `cargo package`, `cargo publish --dry-run` | pass |

## Verification commands actually run

~~~sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features          # 228 + 7 + 1 passed, 0 failed
cargo run --features dev-tools --bin generate-docs -- --check   # 13 artifacts match
python3 scripts/release-benchmark.py --check     # no semantic counter drift
bash scripts/release-check.sh                    # passed; no publication performed
cargo build --release
./target/release/cargo-cleanme --version         # cargo-cleanme 0.1.1
./target/release/cargo-cleanme update --dry-run
~~~

## Live evidence

The only evidence that matters for this corrective is a real request. Run
against the live crates.io registry with a 0.1.1 binary:

~~~text
$ ./target/release/cargo-cleanme --version
cargo-cleanme 0.1.1

$ ./target/release/cargo-cleanme update --dry-run
cargo-cleanme: self-update failed: this build (0.1.1) is newer than the
published release (0.1.0); refusing to downgrade it
$ echo $?
2
~~~

Before this corrective, the same command returned HTTP 403 from crates.io and
could never reach the comparison. The registry answer it now receives proves
the User-Agent seam works; the refusal it now produces proves the ordering
guard works. Both are visible in one command.

## Platform and fixture evidence

- The transport change is platform-independent: `eggup-eggfetch` is a pure
  Rust transport with no external executable, so there is no per-platform
  `curl` discovery to qualify. Hosted Windows and macOS evidence comes from CI
  run for `95bb38e` rather than from local cross-compilation, which cannot
  link `ring 0.17.14` without an MSVC-capable C toolchain.
- Fixture coverage: 19 updater fixture tests plus the 3 new regression tests
  above, all on the deterministic fixture transport.
- The 5 release targets were built, qualified, and validated by the v0.1.1
  release workflow; see the M010D closure record for the run, the asset
  inventory, and the digests.

## Defects this corrective exists to fix

| ID | Severity | Defect | Why verification missed it |
|---|---|---|---|
| C011-F1 | critical | the production transport could not read the version authority, so `update` was non-functional in production | the suite drove a fixture transport that always answers; no fixture talks to the registry |
| C011-F2 | critical (latent) | no version ordering; any differing version pair was treated as "newer available", which would have downgraded every 0.1.1 install | masked entirely behind C011-F1, so the comparison was never reached in production; the fixtures only ever published a *newer* version |
| C011-F3 | medium | the M010C acceptance row asserted a preference ("prefer the lightweight curl path") rather than the requirement it stood in for | the preference was measurable, so it produced evidence that looked like closure evidence |

C011-F3 is the one that let C011-F1 be recorded as `pass`. It is a defect in
the acceptance criteria, not in the code.

## Known limitations

- The release binary is roughly twice the size it was in 0.1.0. On a
  size-constrained platform this is the wrong trade, and the updater should be
  a separate optional binary instead. That is a follow-up, not resolved here.
- The updater is still not exercised end-to-end against a real release in
  CI. The live check above is manual and was run by a human operator. There is
  no automated live smoke, deliberately: CI that mutates a real installation
  and reaches a real registry is a different risk class from the rest of this
  suite, and adding it was not in scope for this corrective.
- `eggup-curl` remains unusable as a transport for any registry-backed updater.
  The bounded upstream request is
  `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md`
  and is `proposed`. Nothing in this repository is waiting on it.

## Unresolved findings

- **medium** — no automated live smoke for the updater. See known limitations.
  Tracked as a follow-up, not as a Phase 10 blocker: the manual live check is
  recorded above and is repeatable from the release checklist.
- **low** — `release/eggpack/pack.toml` still pins Rust as `stable` rather than
  an exact version, so re-running a release for the same tag can produce
  different bytes. This was recorded before 0.1.0 and is unchanged. It is
  noted in `docs/RELEASING.md`.
- No finding here is `corrective required` against this repository.

## Disposition

**closed.** Both critical defects are fixed, each by a test that asserts the
requirement rather than the preference, and the live behavior is verified
against the real registry.

M010C's own closure record is **not** edited to imply it succeeded. It keeps
its original content and carries an addendum recording that its
transport-selection requirement was not satisfied when it was written, and
naming this corrective as the discharge.
