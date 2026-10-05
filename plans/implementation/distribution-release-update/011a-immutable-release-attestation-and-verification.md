# M011A — Immutable Release Attestation and Verification

Status: closed (conditionally — hosted evidence pending the first attested release)

Closure record: `plans/closure/distribution-release-update/m011a-status.md`

Repository baseline: `c3feae01fe5b684826b77b4d32afcb2e3bd4e3d0`

Source roadmap: Phase 11 — hardening, release trust, and qualification automation

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: supply-chain hardening / publication evidence

Dependencies:

- no code dependency on C018;
- operational closure requires the first new release published after this plan is implemented.

## 1. Objective

Move the public GitHub release trust story beyond "HTTPS plus a checksum served by the same release host" without hand-editing the Eggpack-generated build workflow.

The first hardening step is GitHub immutable releases plus their release attestation:

- future release tags and assets become immutable after publication;
- GitHub generates a cryptographically verifiable release attestation binding the release tag, commit, and release assets;
- consumers/operators can verify the release and local assets with GitHub's verification tooling.

This milestone does **not** claim independent publisher-key trust or SLSA build provenance. Those are broader producer capabilities and belong to Eggpack Phase 12, whose planning rules require an authenticity/signing ADR before enforcement.

## 2. Current evidence

At the baseline:

- release binaries are qualified through the Eggpack-generated five-target workflow;
- each binary has a SHA-256 sidecar;
- `release-manifest.json` binds source revision, asset identity, size, and digest;
- release publication is manual after draft inspection;
- actions are pinned by immutable revision;
- `docs/RELEASING.md` correctly says SHA-256 is integrity evidence, not authenticity;
- the generated Eggpack workflow deliberately has no `id-token: write` permission and statically rejects OIDC-token minting;
- the architecture review leaves release authenticity open.

GitHub's current immutable-release model automatically locks the tag/assets and generates a release attestation at publication. This is compatible with the existing "stage complete draft, inspect, then publish" process and does not require adding OIDC permissions to generated build jobs.

## 3. Invariants

- The Eggpack-generated workflow remains generated and drift-checked; do not hand-edit it to add signing steps.
- The human publication boundary remains explicit.
- All required assets must be present before publication because an immutable release cannot be repaired by replacing an asset.
- SHA-256 sidecars remain required for the updater/installers and local integrity checks.
- Release attestation supplements those checks; it does not replace the product contract or candidate identity validation.
- Do not describe GitHub release attestation as an independent maintainer signature. The trust root remains GitHub's release/attestation infrastructure.
- Historical mutable releases remain historical; do not recreate tags or assets to make old releases appear attested.
- No long-lived signing key or new repository secret is introduced by this milestone.

## 4. Work package A — Enable and prove release immutability

Enable immutable releases for `dbowm91/cargo-cleanme` using the repository setting or the supported GitHub API.

Record:

- the setting state after enablement;
- whether policy is repository- or owner-enforced;
- the exact date/commit at which future releases became covered.

Add a release-process check that fails closure if the target release is not immutable after publication.

Do not attempt to make an already-published historical release immutable retroactively if GitHub does not support that operation.

## 5. Work package B — Product-owned release-attestation verification

Add a bounded verification helper or extend an existing release-validation helper so an operator can verify:

- the release itself is immutable and its release attestation is valid;
- the release tag/commit identity matches cargo-cleanme's exact-source rules;
- every contracted public release asset downloaded locally is accepted by GitHub's release-asset verification;
- the exact asset inventory still matches the product contract.

Prefer the supported `gh release verify` / `gh release verify-asset` surfaces for immutable release verification. Keep calls bounded and non-mutating.

The helper must distinguish:

- verification unavailable/tool missing;
- release is mutable;
- attestation invalid/unavailable;
- local asset differs from the attested release;
- product-contract inventory drift.

None of those is a pass.

## 6. Work package C — Integrate verification into release closure

After publication, but before the release is declared closed:

1. verify immutable-release status;
2. verify the release attestation;
3. verify all required assets against the immutable release;
4. retain existing SHA-256 and manifest checks;
5. record the attestation/verification evidence in the closure record.

Update the post-release operator flow so "GitHub release published" is not by itself sufficient release evidence.

This may be called from M011C's automated post-release workflow once that milestone lands; M011A must not depend on M011C to implement the verification helper.

## 7. Why explicit build attestations are deferred

GitHub Artifact Attestations can create per-build provenance with OIDC-backed attestations, but cargo-cleanme's release workflow is generated by Eggpack and Eggpack currently forbids `id-token: write`.

Adding product-specific OIDC steps directly to the generated workflow would violate the producer boundary and the drift contract.

Therefore:

- M011A closes on immutable-release attestation and verification;
- per-build/SBOM/manifest artifact attestations are **not** silently added here;
- if cargo-cleanme requires SLSA-style build provenance or independent signing, stop and open the corresponding Eggpack Phase 12 ADR/implementation line.

## 8. Failure semantics

If a future release is published while immutability is unexpectedly disabled:

- do not describe that release as satisfying M011A;
- do not recreate or move its tag;
- record the deviation and decide whether a new patch release is required.

If immutable publication succeeds but attestation verification fails, release closure fails. Do not replace immutable assets to repair evidence.

## 9. Required tests

- verification helper self-tests with good/bad fixture output;
- exact inventory mismatch;
- mutable-release result is a failure;
- mismatched local asset is a failure;
- missing attestation/verification capability is not silently skipped;
- tag/source mismatch still fails through the existing release-identity gate.

No unit test should fake a successful cryptographic verification by merely returning exit 0 without asserting the command/result shape.

## 10. Verification

~~~text
python3 scripts/check-release-identity.py --self-test
python3 scripts/check-release-contract.py --self-test
bash scripts/release-check.sh
~~~

Plus, for the first post-M011A release:

~~~text
gh release verify <tag>
gh release verify-asset <tag> <each-downloaded-required-asset>
~~~

Record the exact GitHub CLI version used for verification.

## 11. Documentation

Update:

- `docs/RELEASING.md` with immutable-release enablement and post-publication verification;
- `docs/TROUBLESHOOTING.md` or README security/support text with consumer verification commands;
- `architecture/overview.md` finding 4;
- `architecture/15-*` distribution/release documentation if present;
- the release closure template/process.

Keep the wording explicit that attestation links artifacts to GitHub release/source identity; it is not proof that the code is safe.

## 12. Acceptance criteria

M011A closes only when:

- immutable releases are enabled for future cargo-cleanme releases;
- the first new release after enablement is visibly immutable;
- its GitHub release attestation verifies;
- every contracted local release asset verifies against the immutable release;
- exact tag/source identity and existing SHA-256/manifest checks remain green;
- operator/user documentation describes the new verification path and its trust boundary;
- no generated Eggpack workflow drift was introduced;
- no long-lived signing secret was added.

## 13. Stop conditions

Stop and open an Eggpack Phase 12 ADR/plan if the desired claim becomes any of:

- independent maintainer-key signatures;
- SLSA build provenance;
- per-build OIDC artifact attestations generated inside Eggpack CI;
- SBOM attestations;
- offline trust rooted outside GitHub;
- updater enforcement of attestations rather than current digest/candidate identity.

Those change the producer trust model and must not be smuggled into a product workflow.

## 14. Closure evidence

Record:

- immutability setting evidence;
- first covered release/tag/source SHA;
- release-attestation verification output;
- per-asset verification results;
- inventory/manifest/SHA results;
- GitHub CLI version;
- hosted workflow evidence if M011C invokes the verifier;
- unresolved findings;
- disposition.
