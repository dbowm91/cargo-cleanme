# Distribution, Release, and Update M011A Status

Plan: `plans/implementation/distribution-release-update/011a-immutable-release-attestation-and-verification.md`

Disposition: **conditionally closed — implementation and policy complete; operational closure requires the first attested release**

Implementation commit: the `phase11-hardening` branch commit that adds
`release/selector-qualification.json` and
`scripts/verify-release-attestation.py`.

Repository baseline at implementation: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Date: 2026-10-05

## Executive finding

This plan closed the one gap `architecture/15-distribution-and-release.md` called
"the single largest gap in the release surface": **release artefacts were verified
for integrity but not for authenticity.** The `.sha256` sidecar is served by the
same host as the binary it describes, so whoever could replace the binary could
replace its checksum. Every existing check accepted a self-consistent,
correctly-checksummed malicious artefact.

## Work package A — immutable releases enabled

| Field | Value |
|---|---|
| Repository | `dbowm91/cargo-cleanme` |
| Endpoint | `PUT /repos/dbowm91/cargo-cleanme/immutable-releases` |
| State after enablement | `{"enabled":true,"enforced_by_owner":false}` |
| Enablement timestamp | `2026-10-05T13:34:38Z` |
| Policy scope | repository-level, **not** owner/organisation-enforced |
| Releases covered | those published **after** this timestamp |
| Historical releases | v0.1.0 – v0.1.6 remain `immutable: false` and are **not** covered |

Recorded finding worth keeping: the ordinary `PATCH /repos/{owner}/{repo}`
endpoint **silently ignores** `immutable_releases`. It returns HTTP 200 with the
field absent from the response body, so an operator following the obvious path
would believe the setting was on. The state is only readable and writable
through the dedicated `immutable-releases` endpoint, which
`verify-release-attestation.py --settings-only` uses.

Consequence, and it is a real operational cost: **a broken published release can
no longer be repaired by replacing an asset.** The remedy is a new version. This
is recorded in `docs/RELEASING.md` as a pre-staging decision, not a discovery.

## Work package B — verification helper

`scripts/verify-release-attestation.py` verifies a published release and fails
closed on ten distinct classified states, each with its own code:

`verification_unavailable`, `settings_unavailable`, `release_mutable`,
`attestation_unavailable`, `identity_mismatch`, `inventory_drift`,
`asset_not_attested`, `source_identity`, plus a non-`pass` scope distinction for
`--settings-only`.

The security property it actually checks is a **digest comparison against
GitHub's signed in-toto statement**, not an exit status. `gh release verify
--format json` is parsed into
`verificationResult.statement.{subject,predicate}`; every asset's local SHA-256
is compared to the attested digest, and the release subject's
`pkg:github/OWNER/REPO@tag` URI is compared to the expected repository and tag.

Three guards that are easy to get wrong and are implemented deliberately:

- A **successful exit with an unparseable body is a failure.** A verification
  step that reports success because a command returned 0 proves nothing, and
  that is the exact failure mode the self test is built to catch.
- A **missing or too-old `gh`** is `verification_unavailable`, distinct from
  `attestation_unavailable`, because the two demand different operator actions:
  one needs a tool, the other needs an attested release.
- A **response larger than 8 MiB** is refused rather than parsed, because reading
  a release-sized object unbounded is how a verification step becomes a memory
  problem.

## Work package C — product and docs

- `docs/RELEASING.md` gains a "Release authenticity" section stating the trust
  root **exactly**: GitHub's release and attestation infrastructure. Not an
  independent maintainer-key signature, not SLSA build provenance. The sidecar is
  reclassified as a corruption and mis-download check that the product still
  enforces because `src/update.rs` is not yet attestation-aware (see C018).
- The release workflow path is unchanged. It stages a draft and never publishes,
  which remains correct under immutability.
- The updater is deliberately **not** changed here. Attestation-aware `update` is
  C018's follow-on, and this milestone's invariant was to add no
  user-visible behaviour change to a shipped binary.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Immutable releases enabled for the repository | `gh api repos/dbowm91/cargo-cleanme/immutable-releases` → `{"enabled":true,"enforced_by_owner":false}` | Pass |
| Setting state recorded after enablement | Table above; `verify-release-attestation.py --settings-only` reads it live | Pass |
| Trust root stated precisely, no overreach | `docs/RELEASING.md` "Release authenticity" section | Pass |
| Helper distinguishes unavailable / mutable / unattested / mismatched | `--self-test`, 22 cases | Pass |
| Every contracted local asset checked against the attested digest | `check_asset_digests`; `every local asset matches the attested digest` | Pass |
| Historic releases not recreated and not described as attested | `v0.1.6` verified live → fails `release_mutable`, as intended | Pass |
| Normal CI / `release-check.sh` wiring | `scripts/release-check.sh` step added | Pass |

## Verification run

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (261 tests).
- `python3 scripts/verify-release-attestation.py --self-test` — **passed**, 22
  cases, each in the failing direction, driven by a `gh` stand-in that emits
  real-shaped JSON. No case treats an exit code as evidence on its own.
- `python3 scripts/verify-release-attestation.py --settings-only` — **passed
  live** against the real repository.
- `python3 scripts/verify-release-attestation.py --tag v0.1.6` — **failed
  correctly** with `release_mutable`. This is the intended result and is the
  negative control for the whole milestone: a pre-policy release is refused
  rather than described as attested.
- `python3 scripts/check-doc-citations.py` — passed, 16 documents clean.

## Limitations and unresolved findings

1. **No attested release exists yet**, so the byte-level path
   (`--assets-dir`) has been exercised only against synthetic attestations
   produced by the self test. The parsing, classification, and digest
   comparison are all proven; what is unproven is a real
   `gh release verify` document for *this* repository. The schema was read from
   a real attested public release rather than guessed.
2. **The trust root is GitHub.** A compromise of the release account still
   yields a self-consistent artefact every check accepts. That is a property of
   the mechanism, not a defect in the implementation, and it is stated as such
   rather than papered over.
3. `enforced_by_owner: false`. An owner-level policy would be stronger; it was
   not requested and not set.
4. On Windows the self test's stub-based cases are **skipped**, and the script
   says so in its output rather than exiting 0 silently. `check-fixture-portability.py`
   enforces the `fixture-scope` marker that makes the skip explicit.

## Closure status and unblocking

**Conditionally closed.** Every work package is implemented, the policy is live
and verified, and the guard is proved in the failing direction. What cannot be
claimed yet is the milestone's own stop condition:

> If a future release is published while immutability is unexpectedly disabled:
> do not describe that release as satisfying M011A.

The verification of a *real* attested `cargo-cleanme` release cannot be
performed until one is published, and this branch publishes nothing. The next
release from `main` is the first that will carry an attestation. Closure must
not be upgraded before that run's evidence exists — a corrective that is fixed
but not yet carried by a published release stays open, which is the rule
C010–C017 all followed.

**Downstream effect:** this unblocks nothing that was blocked, and M011C is
already implemented. It does **not** unblock C018's release evidence, which
needs a published release carrying the provenance fix.
