# Distribution, Release, and Update M010C Status

Plan: `plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md`

Disposition: **closed**

Implementation commit: `d25beeb` (`feat: add bounded self-update on the
published Eggup crates (M010C)`).

Repository baseline: `b4662af` (M010A implementation commit); M010B convention
established at `34a1d23`.

Date: 2026-10-04

## Executive finding

M010C is closed. `cargo cleanme update` resolves the latest stable release from
the registry, refuses to touch an installation it cannot prove it owns, and
replaces the running executable only when the digest evidence, the sidecar's
identity, and the candidate's own self-report all agree. Generic mechanics are
the published Eggup crates'; this milestone owns policy only.

Its stated hard dependency, Eggup acquisition M009 publishing `eggup-curl`, is
**satisfied**. `eggup-curl 0.1.2` resolves from the crates.io registry, proven
by a `cargo generate-lockfile` probe rather than by a search result.

No update was performed against a real release, because no release exists. The
live behavior of `update` today is a correct fail-closed report.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Version authority is crates.io stable metadata | `published_stable_version` reads exactly one bounded field, `crate.max_stable_version`, and rejects prereleases, non-`X.Y.Z` values, missing fields, non-JSON, and non-UTF-8 | 3 tests pass |
| Release tag is exact and never scraped from "latest" or a redirect | the tag is `format!("v{to_version}")`; a test feeds metadata carrying a hostile `evil.tag` field and asserts the plan tag is `v2.0.0` | pass |
| No downgrade, no nightly/beta target | `AlreadyCurrent` is a typed error, not a silent no-op; prereleases are refused upstream of the tag construction | 2 tests pass |
| Missing release is a typed failure, not a silent success | the transport reports a status and classifies it; `already_current` and `registry outage` tests cover both directions | 2 tests pass |
| Transport must be an already-published Eggup crate | `eggup-curl 0.1.2` + `eggup-acquisition` 0.1.2 + `eggup-core` 0.1.2, all from crates.io | pass |
| Prefer the lightweight curl path; measure both | measured: the curl path is the only production transport; `eggup-eggfetch` would add `eggfetch-core` plus rustls and was rejected on that basis | pass |
| `curl` unavailability is typed, never a second transport | `UpdateError::TransportUnavailable` from `discover_curl_executable`; there is no second transport in the dependency tree | pass |
| One destination, one version, one source, one release | `ArtifactSet::single` with one member; a single ReleaseId; no archive or multi-member path exists | pass |
| Candidate identity checked before replacement | `ExactIdentityValidator` requires exactly `cargo-cleanme X.Y.Z\n`; a test serves self-consistent registry and sidecar data with bytes that lie about their version and asserts rejection with the destination unchanged | pass |
| Destination ownership revalidated under the lock at commit | `LiveDigestVerifier` proves ownership by the live file's exact prior SHA-256 and is consulted by Eggup under `MutationLock`; `AbsentPolicy::DenyCreate` means only replacement of a proven-owned file is ever permitted | pass |
| Ownership never inferred from `~/.cargo/bin`, basename, or `PATH` order | a Cargo root grants nothing by itself; an unrecorded file there is `UnprovableOwnership`; a directory named like the binary is `UnprovableOwnership`; two tests | pass |
| Cargo-managed installs fail closed with guidance | a positive Cargo record produces `CargoManaged` plus `cargo install cargo-cleanme --locked --force`, and a run in that state is refused **before** any release byte is fetched | pass |
| Unprovable/foreign ownership fails closed | `UnprovableOwnership` with remediation to reinstall from the published installer | pass |
| Staging confined to an owned directory; existing artifacts preserved | a private `cargo-cleanme-update-<pid>-<nanos>` directory; Eggup backs up the live member and rolls back on failure | pass |
| Cleanup limited to this run's own temp data | `cleanup` removes exactly the directory `staging_dir` created; a test asserts no leftover on both the success and failure paths | pass |
| No privilege escalation, no PATH mutation, no service control | none of those exist in the module | pass |
| Explicit install provenance policy and documentation | `Provenance` with four states and stable codes; README "Self-update" and docs/TROUBLESHOOTING.md | pass |
| Footprint measured against the pre-update baseline | below | pass |

## Measured footprint

Measured on this host with the default release profile.

| Measurement | Baseline (`78aac6d`) | With updater (`d25beeb`) | Delta |
|---|---|---|---|
| Release binary | 4,970,912 bytes | 5,420,664 bytes | **+449,752 (+9.0%)** |
| Normal dependencies | 70 | 82 | +12 |
| New crates | — | `eggup-curl`, `eggup-acquisition`, `eggup-core` | 3 |
| Transitive deps of the transport | — | **0** (`eggup-acquisition` has none) | 0 |

The transport decision is measured, not asserted. `eggup-curl` is
`eggup-acquisition` plus a `Command::new("curl")` wrapper, and
`eggup-acquisition` declares zero dependencies, so the curl path adds no
embedded HTTP/TLS stack. The alternative, the already-published
`eggup-eggfetch 0.1.2`, depends on `eggfetch-core` (0.2.x) with rustls and would
add a TLS stack plus a materially larger tree to a CLI that self-updates
rarely. Per the plan's own selection rule, the smaller qualified path was
chosen and only one production transport is wired.

An intermediate measurement is worth recording because it is a trap: simply
adding the three dependencies without using them moved the binary by 40 bytes,
because the linker drops unreferenced code. The +449,752 figure is from a build
that actually links the updater.

## Defect found and fixed by the fixture suite

**The candidate was staged non-executable.** `PermissionsIntent::Preserve` maps
an executable source to `0700` and a non-executable source to `0600`. A
freshly downloaded release artifact is neither executable nor marked as such, so
the staged candidate came out `0600` and Eggup's bounded candidate execution
failed with `EACCES` on *every* real update. The intent is now
`PermissionsIntent::Executable`.

This would not have been caught by any test that only checked argument parsing
or a dry run; it required executing the acquired bytes, which is exactly what
`ExactIdentityValidator` does. It is the clearest argument in this milestone for
the fixture suite existing.

Three harness defects were also fixed: a shared temp-file name that raced
between parallel tests, a `fetch_metadata` path that ignored injected
transport failures, and provenance that could only be reached by mutating
process-global environment state. The last one changed the design:
`classify_provenance_in` now takes the Cargo home as a parameter, and
`UpdateEnvironment` exposes `cargo_home`, so the Cargo-managed refusal is
reachable from a test without `set_var` (unsafe in edition 2024) and without
test ordering.

## Verification commands and results

Local, on `d25beeb` (and re-verified on `3e5f3ab`):

- `cargo test --all-targets --all-features` — 222 lib + 7 `cli_contract` + 1
  `end_to_end` pass, including the 19 `update::tests` cases.
- `cargo clippy --all-targets --all-features -- -D warnings` — pass.
- `cargo clippy --target x86_64-pc-windows-msvc --all-targets --all-features
  -- -D warnings` — pass.
- `cargo +1.89 check --locked --all-targets` — pass.
- `cargo +1.89 test --locked --all-targets` — pass, 222 + 7 + 1.
- `cargo package --locked` — pass, 26 files.
- Extracted `.crate` → `cargo build --locked --release` — pass, then
  `release smoke passed for cargo-cleanme 0.1.0`.
- `./target/release/cargo-cleanme update --dry-run` against the live registry —
  fails closed with
  `could not read https://crates.io/api/v1/crates/cargo-cleanme: Transport("HTTP 403 ...")`.
  This is the correct behavior: `cargo-cleanme` is not published, so there is no
  stable version to resolve.

Hosted CI on `d25beeb` (`37227583886`) failed only in the Windows **installer**
lane, for the fixture-stub reason fixed in `34a1d23`; every lane on `34a1d23`
(`37227663867`) is green.

## Known limitations and deferred work

- **No real update was performed.** There is no published release and no staged
  release, so the only end-to-end evidence is the fixture suite plus the real
  fail-closed report above. A real `update` requires M010D's publication.
- **Release-manifest projection is deferred.** Every target is a single direct
  artifact, so the update input is the constructed tag, the contracted asset,
  and the asset's own `.sha256` sidecar. `eggup-eggpack` would add
  `eggpack-manifest` and the archive crates for no input the sidecar does not
  already carry. The plan explicitly permits this deferral, and the honest
  statement is recorded: a real release workflow run is still required to prove
  end-to-end agreement between what Eggpack stages and what this consumes.
- **The committed binary is owner-private (`0700`).** Eggup deliberately makes
  staged files owner-private and never inherits broad source modes, so a
  self-update in a shared multi-user location tightens the mode. Preserving
  broad modes is a bounded upstream Eggup question and is registered as a
  follow-up rather than worked around by post-commit permission edits, which
  would run outside the lock and outside rollback.
- **Strategy A only.** The plan's §7 gate is satisfied by the explicitly
  permitted third option: self-update *refuses* positively Cargo-managed
  installations and prints the manager command. Manager-aware Cargo transition
  was not selected because it could not be proved bounded and cross-platform
  here; Strategy B would edit Cargo-owned bookkeeping.
- **The compiled-in target table is a copy of the contract.**
  `scripts/check-release-contract.py` and a unit test both assert it matches
  `release/eggpack/distribution.toml` exactly, so the two cannot drift silently.
- **No dry-run rehearsal across a real draft release.** M010D owns it.

## Future-plan review

M010C closes. Both of its hard dependencies are now discharged: the M010B
release convention is stable, and `eggup-curl` is published. M010D's remaining
blockers are therefore purely its own: registry authentication and an
irreversible publication, neither of which can be completed from this session
without explicit authorization and credentials.
