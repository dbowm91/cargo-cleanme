# Distribution, Release, and Update Roadmap

Status: **published through v0.2.4; C027 closed.** C022 is closed; M013's v0.2.0 publication was corrected by C023; C023 published immutable v0.2.1, attested it, and yanked crates.io 0.2.0. Its acceptance criterion 11 and M011C's operational condition were both **met on 2026-10-07** by the v0.2.2 release: run `37561575727` was green on all five automatic lanes with `from_version=v0.2.1`, the first green automatic smoke in the project's history. **C023 and M011C are both closed.** C024 is **closed and shipped in v0.2.2**. C025 and C026 are closed. **C027 is closed and shipped in v0.2.3:** it corrected the POSIX installer's first-install UX — a default non-root install now safely persists the canonical user-local bin directory for supported zsh/bash shells instead of only printing PATH guidance. Implementation and hosted Linux/macOS/Windows evidence were complete before release; v0.2.3 carries the fix, its automatic five-target smoke (`37652257315`) is green, and the published installer was verified end to end from outside the repository. **v0.2.4 carries the C028 stale-learned-root fix** (tag `v0.2.4` at `24ec57a`, immutable; Eggpack `37847719817` and staged validation `37850233649` green on first execution; automatic five-target smoke `37850582892` green, v0.2.3 → v0.2.4; crates.io `0.2.4` sha256 `61e97a6e…`, not yanked; C028 closed). C010 remains an independent upstream request, `proposed` and non-blocking. No Phase 14 feature milestone has been accepted.

Repository audit baseline: `85b5d4adee81f363c788505aa2f7d0136eb5ff0b`

Canonical references:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`
- `plans/003-planning-process.md`

External architecture references:

- Eggpack producer authority: `eggstack/eggpack`
- Eggup consumer/update authority: `eggstack/eggup`
- Gregg install/update UX reference: `eggstack/gregg`
- Eggsact Eggpack + Eggup reference consumer: `eggstack/eggsact`

## 1. Purpose and ownership boundary

This subsystem owns cargo-cleanme's public distribution contract, release artifacts, product-owned bootstrap wrappers, self-update policy, publication gates, support matrix, completion/manpage generation, release documentation, and release-oriented benchmark evidence.

It does not own generic release construction, generic verified-update transactions, a second package manager, service lifecycle, background updating, or arbitrary software installation.

Ownership is deliberately split:

- Eggpack owns producer-side release contracts, build/qualification planning, release manifests, generated release CI, exact-release bootstrap evidence, and GitHub draft staging.
- Eggup owns consumer-side bounded acquisition, integrity verification, local staging, ownership revalidation, replacement, rollback, and recovery evidence.
- cargo-cleanme owns release/version authority, target selection, install/update destination policy, Cargo fallback policy, CLI presentation, public `install.sh` / `install.ps1` wrappers, and publication decisions.

cargo-dist is not a production dependency for this roadmap. Eggpack is the canonical Eggstack producer path.

## 2. Invariants

- Distribution work must not weaken any existing cleanup safety invariant.
- Published binaries must be built from an exact source revision and qualified as the same bytes that are staged for release.
- SHA-256 is integrity evidence, not an authenticity claim.
- Public installers and `cargo-cleanme update` must fail closed on checksum, manifest, candidate-identity, TLS/transport, timeout, or destination-ownership failures.
- Cargo/source fallback is permitted only by explicit cargo-cleanme policy for unsupported targets or an actually absent selected binary. Verification failure is never a fallback signal.
- No installer or updater invokes `sudo` internally.
- Self-update must never infer ownership from basename alone.
- The application remains a Cargo external subcommand: both `cargo-cleanme ...` and `cargo cleanme ...` stay supported.
- Release automation stages drafts/evidence; public publication remains an explicit release action.
- Package-manager integration must not silently mutate package-manager-owned metadata.
- The current Rust library surface is not implicitly declared stable merely because the crate is published; the support stance must be documented before first publication.

## 3. Target release contract

Initial required binary targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

The preferred public asset convention is version-independent within a release:

- `cargo-cleanme-{target}`
- `cargo-cleanme-{target}.sha256`
- Windows executable suffix `.exe`

The Linux build strategy should hold an explicit glibc compatibility floor when the current Eggpack Cargo/cargo-zigbuild path can qualify it. A glibc 2.17 floor is the initial target because it matches established Eggstack release practice; implementation must record real qualification rather than claim it from configuration alone.

ARMv7, musl, Windows ARM64, package-manager formulae, and additional targets are follow-up capabilities unless evidence justifies adding them without delaying the initial distribution closure.

## 4. Dependency graph

~~~text
Phase 9 + C008 [closed]
        |
        v
M010A Eggpack distribution contract + package readiness [CLOSED]
        |
        v
C009 Windows test-fixture portability [CLOSED: restores per-platform evidence]
        |
        +-----------------------------+
        |                             |
        v                             v
M010B bootstrap installers      Eggup acquisition M009
+ release artifact             eggup-curl publication
qualification                  [external; VERIFIED UNPUBLISHED 2026-10-04,
        |                       so M010C must select from what is
        |                       actually published]
        +-------------+---------------+
                      |
                      v
M010C Eggup self-update + install provenance
                      |
                      v
M010D publication + operational polish + release closure
~~~

M010B may proceed while Eggup M009 is implemented. M010C must not vendor/copy Gregg update machinery if Eggup M009 is delayed.

## 5. M010A — Eggpack distribution contract and package readiness

Plan: `plans/implementation/distribution-release-update/010a-eggpack-distribution-contract-and-package-readiness.md`

Closure: `plans/closure/distribution-release-update/010a-status.md`

Status: closed at implementation `b4662af`. No publication occurred. Its single
green-CI acceptance criterion was a pre-existing Windows test-fixture defect,
discharged by corrective C009.

Primary outcomes:

- make Eggpack the canonical producer authority for cargo-cleanme release artifacts;
- add the static release contract, build bindings, qualification bindings, workflow shape/policy, draft template, install policy, installer presentation, and consumer validator inputs required by Eggpack;
- generate and drift-check the release workflow;
- define the supported binary target matrix and Linux ABI floor evidence;
- make the crates.io package intentional: metadata, licenses, package include/exclude policy, README/CHANGELOG/support text, and explicit library-API support stance;
- require `cargo package --locked` and `cargo publish --locked --dry-run` from a clean tree.

M010A does not publish anything and does not add a self-updater.

## 6. M010B — Bootstrap installers and release artifact qualification

Plan: `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`

Closure: `plans/closure/distribution-release-update/010b-status.md`

Status: closed. The release contract, both public wrappers, and the per-platform
fixture qualification are in place. Runtime qualification of the real release
bytes was the open item; M010D discharged it, and both the workflow's own
`qualify_build_*`/`validate_build_*` jobs and a real installer install of the
real asset now cover it. Static glibc 2.17.0 evidence is confirmed on both
Linux targets.

Primary outcomes:

- product-owned `packaging/install.sh` and `packaging/install.ps1`;
- latest and exact-version selection;
- binary-first installation with Cargo fallback only for unsupported/missing binary;
- manifest/checksum and candidate `--version` validation before placement;
- no internal privilege escalation;
- user-local default installation, with explicit elevated/system behavior only when already running elevated;
- runtime qualification of generated release artifacts on the five required targets;
- Eggpack exact-release bootstrap output used as conformance evidence, not as a substitute for the public wrappers.

## 7. M010C — Eggup self-update and install provenance

Plan: `plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md`

Closure: `plans/closure/distribution-release-update/010c-status.md`

Status: closed.

Dependency discharged. `eggup-curl 0.1.2` **is** published and resolvable; this
was proven with a `cargo generate-lockfile` probe, not a search result. An
earlier check in this session reported it unpublished, and that was correct at
the time — the sparse-index CDN lagged the registry API, so the state genuinely
changed mid-session. The lesson for any later transport work: confirm a
publication by resolving it, because both the index CDN and the API can
disagree with each other and with `cargo search` during propagation.

Primary outcomes:

- add `cargo-cleanme update` / `cargo cleanme update`;
- consume published Eggup primitives rather than copying `gregg-update`;
- prefer the lightweight `eggup-curl` acquisition path once registry-published;
- consume Eggpack ReleaseManifest evidence through `eggup-eggpack` where that keeps release facts single-source;
- validate the staged executable identity exactly before replacement;
- define a truthful install-provenance policy for Cargo-managed, installer-managed, and unknown installations;
- preserve Cargo fallback as cargo-cleanme policy, never as an Eggup transport fallback;
- fail closed when manager ownership cannot be proven safely.

M010C must measure binary-size/dependency impact against the pre-update baseline. If the published curl path is unexpectedly larger or less portable than the native Eggfetch path, record the measurement and choose the smaller qualified path; do not add two production transports without need.

## 8. M010D — Publication, operational polish, and release closure

Plan: `plans/implementation/distribution-release-update/010d-publication-operational-polish-and-release-closure.md`

Closure: `plans/closure/distribution-release-update/010d-status.md`

Status: closed in both halves. The repository-internal scope — generated
completions and manpages with a CI drift gate, the benchmark baseline with
counters gated and timings not, the operator release checklist, the support
policy, and troubleshooting — landed at `3e5f3ab`.

The publication half completed after the user authorized it and supplied
authentication. `v0.1.0` and `v0.1.1` are public releases; `cargo-cleanme 0.1.0`
and `0.1.1` are on crates.io and neither is yanked. Release workflow run
`37232595211` built, qualified, and validated all five targets green
(20/20 jobs), and the staged draft was validated against the release contract
before publication.

Publication exposed a defect that no test could catch, and it is recorded as
corrective **C011** rather than smoothed over: the `v0.1.0` updater could not
read the version authority at all. See
`plans/closure/distribution-release-update/c011-status.md`.

One requirement was closed partial: the live updater **commit** path had no
end-to-end rehearsal, because `v0.1.1` was both the newest published version and
the first containing a working updater.

That gap is now closed as *evidence*, and the rehearsal it produced is why the
commit path is not closed as *behavior*. C014 published v0.1.2 and then ran the
real `v0.1.1` -> `v0.1.2` commit rehearsal, which failed: the identity check ran
the downloaded candidate with no arguments, so its output could never equal a
version string, and before failing it scanned the user's whole filesystem. The
defect is C016.

Primary outcomes:

- first crates.io publication and registry-only install verification;
- first binary-bearing GitHub release with checksums, manifest, installers, and required target evidence;
- move subsequent crates.io publication to trusted/OIDC publishing when the registry bootstrap permits it;
- shell completions and manpage generated from the clap command model with CI drift checks;
- release/support documentation and explicit selector/Cargo compatibility policy;
- release benchmark tracking built from existing `--no-progress --stats` and deterministic fixtures;
- smoke installation and self-update from externally resolved release artifacts;
- closure record with source revision, package checksum, release asset inventory, hosted platform runs, known limitations, and rollback/recovery evidence.

## 8A. C013 — Cross-platform fixture premise audit

Plan: `plans/implementation/distribution-release-update/c013-cross-platform-fixture-premise-audit.md`

Status: **closed**. Implementation `51d70f1`; closure record
`plans/closure/distribution-release-update/c013-status.md`.

C013 closed the low-severity evidence debt left by C012: audit every fixture that substitutes or resolves an external executable, PATH entry, shell/editor command, or platform-specific process and prove that the executable the test intends to exercise is the executable the subject actually resolves. The audit may keep intentionally Unix-only tests platform-scoped; it must not claim cross-platform coverage from a fixture that cannot execute on that platform.

Primary outcomes:

- complete external-process fixture inventory;
- platform-correct PATH/stub semantics;
- premise assertions for substituted tools;
- explicit scoping for deliberately platform-only fixtures;
- no medium-or-higher fixture-evidence finding remaining.

C013 did not reopen Phase 10 and changed no production behavior. The audit
recorded a 25-row fixture inventory, corrected nine evidence defects, and proved
every new premise guard in the failing direction.

Its most serious finding was not a test: the release-candidate smoke validator
that C009 added to CI was green on all three hosted lanes while the subject under
test was failing. Tightening that validator exposed a real product defect, which
by C013's own failure semantics became a separate corrective — see §8C.

## 8B. C014 — v0.1.2 live update and release reproducibility corrective

Plan: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`

Status: **closed**. Closure: `plans/closure/distribution-release-update/c014-status.md`.

C014 began with v0.1.2 as a deliberately small qualification release and was
deliberately held open when that release's live updater rehearsal failed. The
evidence line subsequently opened C016 and C017 rather than absorbing those
defects. C014 finally closed after the published v0.1.5 -> v0.1.6 rehearsal
proved both the self-managed commit path and Cargo-managed refusal, with the
five-target hosted smoke green.

Required outcomes:

- replace floating `rust = "stable"` release inputs with one exact qualified Rust release while preserving Rust 1.89 MSRV;
- mechanically enforce exact tag/version/source identity for crate publication;
- publish v0.1.2 only after the normal Eggpack staged-release gates and the tag/source identity gate (the C013 precondition is closed);
- exercise the released v0.1.1 updater through a real self-managed v0.1.1 -> public v0.1.2 networked commit;
- require the installed post-update binary digest to equal the public v0.1.2 asset digest;
- prove a Cargo-managed v0.1.1 install refuses self-update without mutation and emits the correct v0.1.2 manager remediation;
- make the live smoke repeatable on the supported release platform matrix to the extent claimed.

A failure after immutable publication must be handled by yanking/annotating and a new corrective/version, never by replacing v0.1.2 bytes.

C014's first live rehearsal produced the C016 defect: `v0.1.1` and `v0.1.2`
ship an updater that cannot complete a commit, fixed in v0.1.3. A later
Cargo-managed rehearsal produced C017, fixed in v0.1.5. The proving
v0.1.5 -> v0.1.6 rehearsal and five-target hosted smoke close C014's original
live-evidence requirement. Historical affected releases remain recorded rather
than rewritten.

## 8C. C015 — Relative scan root Cargo resolution

Plan: `plans/implementation/distribution-release-update/c015-relative-scan-root-cargo-resolution-corrective.md`

Status: **closed**. Implementation `1316e81`; closure record
`plans/closure/distribution-release-update/c015-status.md`. Discovered by C013.

An explicitly relative scan root — `cargo-cleanme scan fixture` — silently
resolves **zero** Cargo workspaces. Discovery preserves the caller's spelling, and
the product then passes that relative manifest path to `cargo locate-project` with
the child's working directory set to the manifest's own parent, so the path does
not resolve. The scan still exits 0 with a valid JSON document reporting no
groups.

Severity is medium. The failure direction is fail-safe: a degraded scan reports
*less* reclaimable space, so `clean` deletes less and never more. But a user can
read "nothing to clean" as a false all-clear, and the difference is invisible in
the machine-readable channel.

C015 did not block v0.1.2 publication for that reason. It is **closed**: the
manifest path is anchored for Cargo lexically against the process working
directory, the two spellings now produce identical counters, groups, and bytes,
and both required tests fail against the unfixed code. It ships in v0.1.3, so
the 0.1.2 release-note instruction to use an absolute scan root becomes
unnecessary from 0.1.3 onward.

Both C015 tests had to be written twice before they proved anything: the first
relative spelling used `..` components, which compose back to the right file and
so hide the defect, and the first real-Cargo case gated on the cargo *patch*
component, so `1.99.0` read as patch 0 and the case skipped into a pass. A test
that skips into a pass is the same false green as a fixture that accepts any
input.

## 8D. C016 — Live self-update identity invocation

Plan: `plans/implementation/distribution-release-update/c016-live-self-update-identity-invocation-corrective.md`

Status: **closed**. Discovered by C014's live rehearsal.

`cargo cleanme update` has never completed a commit in any published version.
Eggup's `ExactIdentityValidator` executes the staged candidate with no arguments
and requires its stdout to be exactly `cargo-cleanme <version>\n` with empty
stderr. With no arguments the candidate performs its default routine scan, so
the comparison could never match — and the identity check performed a filesystem
scan of the whole machine before failing.

The transaction aborted safely every time: the live binary was untouched, the
error named the reason, and nothing was corrupted. Severity is high because a
shipped, documented feature does not work at all, and not critical because
nothing is damaged and the failure is loud.

The fixture could not see it, and the reason generalises: the commit-path
candidate stub printed the identity for *any* invocation, so it could not
distinguish a correct `--version` call from the argv-less one production made.
The stub now answers only for `--version` and writes to stderr otherwise, so
dropping the argument turns the commit-path tests red. A fixture that accepts
every input asserts nothing about the input.

**Closed.** The fix shipped in v0.1.3; v0.1.4 existed only to give the fixed
updater a real newer target, and the `v0.1.3` -> `v0.1.4` rehearsal passed with
the post-update digest equal to the published asset digest. The `v0.1.2` ->
`v0.1.3` rehearsal was run deliberately first and **failed by design**, which is
the evidence that the defect was real in the wild and not only in a fixture.
Re-confirmed as `v0.1.5` -> `v0.1.6`.

Closure record: `plans/closure/distribution-release-update/c016-status.md`.

## 8E. C017 — Cargo-managed provenance misdetection

Plan: `plans/implementation/distribution-release-update/c017-cargo-managed-provenance-misdetection-corrective.md`

Status: **closed**. Discovered by C014's `v0.1.3` -> `v0.1.4` rehearsal, cargo-managed
scenario.

`cargo install --root DIR` places the binary in `ROOT/bin` and the record in
`ROOT/.crates.toml`. Detection compared the running executable's parent against
`$CARGO_HOME/bin` and nothing else, so any install whose root was not the process
`CARGO_HOME` was classified self-managed and **replaced in place**, silently, with
exit `0`. Afterwards `cargo install --list` reported a version the file no longer
had, so `cargo upgrade` and `cargo uninstall` could not work.

This is the only defect in this corrective line that mutated. C011, C015, and
C016 all failed loudly and wrote nothing, which is the distinction that makes
this one worth its own plan.

A second bug was hiding behind it: the `.crates.toml` parser read
`[packages.<name>].vers`, which no cargo emits any more, so the refusal seen in
the `$CARGO_HOME/bin` case was an accident of two bugs rather than a decision.
The test covering it hand-wrote the obsolete shape, so the parser was checked
against its own assumption — the blind-fixture pattern from C009, C011, C012,
C013, and C016, now for the fifth time in this subsystem, and missed by C013's
audit for the same reason as the others.

**Closed.** The fix shipped in v0.1.5 and v0.1.6 supplied the target it needed.
v0.1.6 is required because the refusal is raised *after* the already-current
check, so observing it needs an installation behind the published version. The
`v0.1.5` -> `v0.1.6` rehearsal passed end to end: the self-managed path committed
the correct bytes, and the Cargo-managed path refused with the manager command and
the binary untouched.

Two of the five premise-negatives in C017 **failed to bite on the first attempt**
and had to be rewritten before the harness could prove anything — the
over-detection test and the malformed-entry test were both blind fixtures of the
exact kind they were meant to catch. The second is the same mistake the plan
exists to document, caught this time by the premise-negative rather than by a user.

That rehearsal also exposed a latent harness bug: the cargo-managed assertion
required the remediation to name the target version, which no correct refusal can
do, because `cargo install cargo-cleanme --locked --force` resolves to the newest
release on its own. It was latent rather than obvious because the scenario had
never reached that line. Worse, it would have *accepted* a message claiming the
tool had updated — the exact outcome the scenario forbids. Replaced at `3548fc2`
with an assertion of the exact manager command, verified in both directions.

Closure record: `plans/closure/distribution-release-update/c017-status.md`.

## 9. Verification strategy

Every milestone keeps the existing Stable + Rust 1.89 + Linux/macOS/Windows checks green.

Release-specific verification adds:

- clean-tree package/publish simulation;
- generated-workflow drift checking;
- package-content inspection including embedded `config.toml`;
- exact release artifact inventory validation;
- per-target `--version` and `--help` smoke;
- bounded scan and JSON-output smoke on built release candidates;
- installer negative tests for missing checksum, malformed digest, wrong candidate identity, wrong requested version, unavailable Cargo fallback, existing/foreign destination, and unsupported platform;
- updater fixture tests for current/latest comparison, missing asset, verification failure, transport failure, candidate mismatch, destination ownership failure, interrupted replacement, and recovery receipt;
- no destructive cleanup command should be necessary to qualify distribution.

## 10. Completion definition

Phase 10 is complete when cargo-cleanme can be installed from crates.io or a verified prebuilt release on the required platforms, the public bootstrap wrappers and self-update path share the same release contract, Eggpack and Eggup own the generic producer/consumer mechanisms respectively, release workflow drift is mechanically detected, support limitations are explicit, and the first public release has closure evidence rather than only a successful build.


## 11. Phase 11 — Hardening, release trust, and qualification automation

Phase 11 is a hardening line over the closed Phase 10 distribution surface. It
does not reopen M010A-M010D or rewrite C013-C017. Its purpose is to close the
remaining provenance, release-trust, and live-evidence gaps identified by the
post-release rehearsals and architecture review.

Dependency/parallelism:

~~~text
C013-C017 [closed]
      |
      v
C018 provenance uncertainty fail-closed [CLOSED]
      |
      +----------------------+----------------------+
      |                      |                      |
      v                      v                      v
M011A immutable          M011B staged           M011C published
release attestation      validation workflow    smoke automation
[CLOSED]                 [CLOSED]               [CLOSED]

All three closed outright. M011A and M011B reached that when their evidence
existed; M011C reached it on 2026-10-07, when run `37561575727` produced the
first green **automatic** `release: published` smoke on all five lanes for the
v0.2.2 release, discharging the condition it had held open.

The paragraph that previously sat here described four outstanding *hosted*
evidences supplied by "the first release published from `main` after
2026-10-05". That release happened: v0.2.0 supplied C018/M011A/M011B, v0.2.1
carried the C023 safety fixes, and v0.2.2 supplied the automatic-smoke evidence
and shipped C024. None of the four is outstanding.


### 11.1 C018 — Self-update provenance uncertainty fail-closed

Plan: `plans/implementation/distribution-release-update/c018-self-update-provenance-uncertainty-fail-closed.md`

Status: **closed.** Published 0.1.6 refused in both the default Cargo home and
`--root` with 0.2.0 available, bytes byte-identical before and after. The fix
is carried by every published release from 0.1.6 onward.

C018 corrects the residual C017 ownership boundary. Cargo-manager evidence that
is present but unreadable, malformed, or otherwise ambiguous must not collapse
to `VerifiableSelfManaged`. The implementation must characterize real Cargo
layouts, replace the current `Option`-shaped manager-record evidence with typed
outcomes, preserve genuine installer-managed layouts, and refuse mutating
manager/unprovable provenance before remote release acquisition.

Closure requires a public release carrying the fix plus real default/`--root`
Cargo evidence that bytes and manager bookkeeping remain unchanged.

### 11.2 M011A — Immutable release attestation and verification

Plan: `plans/implementation/distribution-release-update/011a-immutable-release-attestation-and-verification.md`

Status: **closed.** v0.2.0's real immutable release was verified with a valid attestation and all 15 downloaded asset digests matching it, and v0.2.2 repeated that verification (`verify-release-attestation.py`, 15 digests). The trust root is GitHub's release and attestation infrastructure, and the doc says so.

M011A enables GitHub immutable releases for future publications and makes the
platform's immutable-release attestation/asset verification part of release
closure. Existing SHA-256 sidecars, `release-manifest.json`, source identity,
and candidate checks remain required.

This is deliberately the product-owned first step. It does not hand-edit
Eggpack-generated CI or request OIDC there. Independent signing, SLSA/per-build
provenance, SBOM attestation, or updater enforcement crosses into Eggpack Phase
12 and its authenticity-ADR threshold.

### 11.3 M011B — Staged-release validation workflow gate

Plan: `plans/implementation/distribution-release-update/011b-staged-release-validation-workflow-gate.md`

Status: **closed.** The automatic staged-draft validator has run against real drafts: run `37418179489` passed all six steps for v0.2.0 including a measured `GLIBC_2.17.0` floor, and run `37561423879` passed all six for v0.2.2 (inventory 15 assets, sidecar integrity, manifest agreement, contract agreement, static Linux ABI, real installer qualification).

M011B makes `scripts/validate-staged-release.py` a hosted publication
prerequisite. A product-owned, read-only workflow observes successful trusted
Eggpack candidate-build runs, re-binds the exact source/tag, downloads the
actual draft, and runs the existing six-part validator. Human publication
remains separate.

If GitHub's workflow-run context cannot safely bind the exact tag/source,
implementation stops and registers an Eggpack post-stage validation seam rather
than modifying generated YAML.

### 11.4 M011C — Published release smoke automation

Plan: `plans/implementation/distribution-release-update/011c-published-release-smoke-automation.md`

Closure: `plans/closure/distribution-release-update/m011c-status.md`

Status: **closed.** The automatic trigger is **proven** — it has fired
unprompted on three real publications (v0.2.0 run `37419183947`, v0.2.1 run
`37507738147`, v0.2.2 run `37561575727`) — and on the third it was **green on all
five lanes under the automatic trigger itself**, which is what this section was
waiting for. The first two automatic runs failed and remain recorded as
failures; the manual recoveries (`37420625111`, `37508262225`) are still the
evidence that the *transaction* works, but they are no longer standing in for it.
The v0.2.1 failure was C023's own yank reaching the harness —
`select_predecessor` consulted GitHub release suitability but not crates.io
installability, so it chose a yanked v0.2.0 that no lane could install. Fixed in
`cfdc910`, self-test corrected in `b0b41fc`, both landed. No automatic run for
v0.2.1 can be recreated — `release: published` fires once per release — and the
missing evidence was supplied by the v0.2.2 release instead.

M011C preserves the existing five-target real updater rehearsal but adds an
automatic stable `release.published` path. It deterministically selects the
previous stable public release and waits with a finite deadline for crates.io to
expose the newly published target version before exercising the updater.

The workflow stays read-only, credential-free beyond public/repository reads,
isolated to temporary directories, and `fail-fast: false`. Manual dispatch
remains available for recovery/historical transitions.

### 11.5 Cross-cutting release hardening constraints

- Generated Eggpack CI remains generated and drift-checked.
- Product evidence workflows never gain publication authority.
- A release is not fully closed until the staged validator, immutable-release
  verification, and automatic post-release smoke all produce the evidence their
  plans require. v0.2.2 is the first release for which all three are green.
- Failure after immutable publication creates a corrective/new patch release;
  bytes/tags are not rewritten.
- SHA-256 remains local integrity evidence even after release attestation is
  added.
- C010 remains independent; no Phase 11 item waits for `eggup-curl`.

## 12. Phase 13 — 0.2.0 pre-release hardening and publication

Status: **published; corrected by C023, which is now closed.**

Phase 13 does not add a new product feature. It takes the already-implemented
0.2.0 tree through the last bounded verification hardening and then through the
first immutable/attested publication.

Dependency:

~~~text
C021 pre-release evidence reconciliation [closed]
                    |
                    v
C022 machine-contract hardening [closed]
                    |
                    v
M013 v0.2.0 publication + Phase 11 operational closure [closed]
                    |
                    |    published 0.2.0 from tag 95629ae; six release-
                    |    automation defects found and fixed on the way,
                    |    all previously unexecuted
                    |
                    +--> C018 released Cargo provenance evidence [satisfied]
                    +--> M011A immutable attestation evidence [satisfied]
                    +--> M011B automatic staged-validation evidence [satisfied]
                    +--> M011C automatic five-target smoke evidence
                         [OUTSTANDING: automatic trigger fired for v0.2.0 and
                         failed; v0.2.1's automatic run also fired and failed.
                         Green evidence is manual-recovery only]
~~~

### 12.1 C022 — Pre-release machine-contract hardening

Plan:

- `plans/implementation/distribution-release-update/c022-pre-release-machine-contract-hardening.md`
- closure: `plans/closure/distribution-release-update/c022-status.md`

Status: **closed** (`560a158`, `6c4195c`, `3bf8f3a`, `705cd65`; CI 37410115511, drift 37410115515).

C022 closed the two bounded release-edge gaps C021 intentionally carried
forward:

- `update --format json` had a production serializer and no contract/stream
  test. `update_json` moved from a private function of the binary into
  `output.rs`, and `update_json_stream` now returns the exact bytes the JSON
  path writes, so four cases pin the production document and its stream and a
  real-binary case pins the failure shape. **No product behaviour changed.**
- `check-installer-contract.py` was the only contract checker without a
  failing-direction `--self-test`. It now takes a fixture root and the recorded
  exec bit, and its self-test requires a rejection *for the intended
  diagnostic* across twelve cases, wired into CI, release-drift, and
  `release-check.sh` before the check itself.

Three hosted failures landed in C022's own work before it closed — a CRLF
assumption in a test that reads `main.rs`, an `ETXTBSY` staging race on the
`msrv` runner, and a read-only flush handle that made the second of those fixes
Unix-only. None reproduced locally in 23 attempts, and the local ladder was
green throughout; it took four pushes to close. All are recorded in the closure
record rather than fixed quietly. C022 published nothing.

### 12.2 M013 — 0.2.0 publication and Phase 11 operational closure

Plan:

- `plans/implementation/distribution-release-update/013-0.2.0-publication-and-phase11-operational-closure.md`

Status: **historical closure recorded; current state corrected by C023 (closed)** — receipt in `plans/closure/distribution-release-update/013-status.md`, with an appended corrective note. The receipt itself records that automatic M011C criterion 9 was not met; the post-release safety findings that followed are closed by v0.2.1. No claim in the receipt is retracted.

M013 is the canonical operator handoff for the 0.2.0 release. It consumes the
existing release machinery rather than reimplementing it:

1. freeze and qualify one exact 0.2.0 release commit;
2. tag `v0.2.0` and prove tag/source/version/changelog identity;
3. dispatch Eggpack and stage the draft;
4. require the automatic M011B staged validator on that exact source;
5. human-inspect the draft;
6. publish the immutable GitHub release;
7. publish crates.io from a detached checkout of the exact tag;
8. verify M011A release+asset attestation;
9. require the automatic M011C five-target `v0.1.6 -> v0.2.0` smoke;
10. gather C018 real default-Cargo-home and `--root` refusal evidence;
11. reconcile all closure records and the registry.

Publication remains the irreversible boundary. A pre-publication failure stops
the release. A post-publication defect produces a corrective/new patch release;
published immutable bytes/tags are never replaced.

### 12.3 Deferred findings that do not block M013

The following remain explicit follow-ups rather than hidden release blockers:

- the 257 opaque bare line citations in `architecture/12-self-update.md`;
- hosted-job cancellation/concurrency policy;
- C010's optional upstream `eggup-curl` User-Agent seam;
- independent signing/SLSA/SBOM work, which remains an Eggpack authenticity
  roadmap concern.

None may be used to waive a failed release qualification gate.

### 12.4 C023 — v0.2.0 destructive-safety patch-release corrective

Status: **closed** — accepted closure record:
`plans/closure/distribution-release-update/c023-status.md`. Acceptance criterion
11, a green **automatic** M011C five-target smoke, was **not met at v0.2.1** and
**was met on 2026-10-07** by the v0.2.2 release, run `37561575727` — green on all
five lanes with `from_version=v0.2.1`. See that record's §13B and
`r022-status.md`.

Plan:

- `plans/implementation/distribution-release-update/c023-v0.2.0-destructive-safety-patch-release-corrective.md`

Affected release:

- immutable `v0.2.0`, source `95629ae28223e975faf1e8ed99ca3e6f83d6f724`.

*The text below is the plan as it was written before implementation. It is kept
as history. What actually happened is in the closure record, not here.*

Post-release interrogation proved two destructive defects in those bytes:

1. `cargo cleanme --dry-run clean ROOT` could discard the root-level dry-run
   and execute a real `cargo clean`.
2. an output covering root owned by one workspace could contain another
   resolved workspace's source tree and still be treated as `PrivateBounded`,
   allowing Cargo cleanup to remove the neighbour's source.

The fixes are present on `main`, but the current baseline
`eaa7bba1cd70513a7bf48a99d7d9ffe19334a539` is not release-qualified:
CI #284 fails on macOS because a new discovery test helper passes
`std::fs::FileType` where `dua_core::FileType` is required; the Ubuntu and
Windows check lanes were cancelled after that failure.

C023 requires published-0.2.0 premise reproduction in disposable fixtures,
cross-platform qualification of the fixes, an immutable v0.2.1 patch release,
a green **automatic** M011C five-target smoke, public-binary safety
requalification, crates.io 0.2.0 yank, and explicit reconciliation of the M013
historical closure without erasing its failed evidence.

Ordinary roadmap work is unblocked. C024, the one medium finding C023 recorded, has been taken, is closed, and **shipped in v0.2.2**.

**What the plan asked for versus what happened.** Everything below was
delivered: both defects were reproduced against the immutable 0.2.0 binary,
cross-platform qualification, an immutable and attested v0.2.1, public-binary
safety requalification, and the crates.io 0.2.0 yank. One requirement was not:
the green **automatic** M011C smoke, because WP-L's yank removed the rehearsal
source the resolver had already chosen. v0.2.1 is published, attested, and
safety-qualified; v0.2.0 is yanked and disclosed in four user-facing documents;
the automatic smoke fired and failed; the resolver that failed it is fixed and
self-tested. See §12.6 of the closure record and §11.4 above.

### 12.5 C025 — Post-v0.2.1 planning and status reconciliation

Status: **closed** — closure record:
`plans/closure/distribution-release-update/c025-status.md`.

Plan:

- `plans/implementation/distribution-release-update/c025-post-v0.2.1-planning-status-reconciliation.md`

C025 was a documentation/control-surface corrective. It changed no Rust code,
release bytes, glob semantics, updater behavior, yank state, or workflow
behavior.

It existed because current summaries disagreed with the accepted evidence:

- C023 was conditionally closed, but several current-status sentences called it
  simply closed or still ready;
- the automatic v0.2.1 smoke fired and failed, but several summaries called it
  observed-green and said M011C was discharged;
- the M011C closure record had not incorporated the v0.2.1
  automatic-failure/manual-green evidence;
- the canonical roadmap still described completed v0.2.1 publication/yank work
  in future tense.

**Resolution (as C025 recorded it, on 2026-10-06):** C023's header read
conditionally closed and agreed with its own disposition; its finding count was
derived from §11's headings rather than remembered; §13 made no M011C upgrade
claim; M011C carried the v0.2.1 addendum and was still conditionally closed;
this roadmap, the cleanup roadmap, `plans/registry.md`, and
`plans/002-long-term-roadmap.md` all stated the same current state. No failed
run was rewritten as passing and no historical evidence was edited.

C024 was left untouched by that reconciliation and was taken afterwards: it is
now closed, and its fix **shipped in 0.2.2** (release record
`plans/closure/distribution-release-update/r022-status.md`).

**Later reconciliation (2026-10-07).** The v0.2.2 release produced the first
green *automatic* `release: published` smoke (run `37561575727`, all five lanes,
`from_version=v0.2.1`). That discharged the condition C023 and M011C were both
holding open, so **both are now closed**. The two earlier automatic runs remain
recorded as failures; a later green run does not convert them into passes. The
resolution text above is left as C025 wrote it, because it accurately describes
the state on 2026-10-06.



### 12.6 C026 — Post-v0.2.2 repository reconciliation, cleanup, and polish

Status: **closed** — closure record:
`plans/closure/distribution-release-update/c026-status.md`.

Plan:

- `plans/implementation/distribution-release-update/c026-post-v0.2.2-repository-reconciliation-cleanup-and-polish.md`

C026 was the bounded handoff after the v0.2.2 release closed the remaining
C023/M011C operational evidence. It did not reopen Phase 11, Phase 12, Phase
13, C023, C024, C025, or the v0.2.2 release record.

Its scope was deliberately non-product:

- reconcile stale *current-state* registry/roadmap/architecture/operator prose
  against the accepted v0.2.2 evidence while preserving historical records;
- repair agent-entry facts such as release/yank counts and ADR-003 CLI wording;
- audit architecture findings whose statuses predate the Phase-11 closure;
- prove stale branch history is already preserved or superseded before
  retirement;
- leave the next product milestone undefined.

Repository baseline: `aac5b685e116b67aadda06bacac5788c483b1499`.
Implementation branch:
`plans/c026-post-v0.2.2-reconciliation-cleanup`.

**Outcome.** The reconciliation completed with no product, release, schema, or
configuration change, and found no medium-or-higher implementation defect that
needed a separate corrective. Four stale remote branches were retired after
proving that three had zero unique commits and that the fourth's unique commit
was content-preserved on `main`. **No distribution corrective remains open, and
no Phase 14 was defined or activated.**

That sentence was true on 2026-10-07 and was superseded the same day by the
acceptance of C027, which was in flight while this section was written. It is
left verbatim rather than restated: C026's own outcome is a closed record's
content, and rewriting it to sound current would make the timeline lie. The
current state is §12.7 below — **no distribution corrective is open, and no
Phase 14 was defined or activated.**

### 12.7 C027 — POSIX installer user-local PATH persistence corrective

Status: **closed — shipped in v0.2.3.** Closure record:
`plans/closure/distribution-release-update/c027-status.md`; release record:
`plans/closure/distribution-release-update/r023-status.md`.

Implementation landed in `b011c85` plus three evidence repairs (`a114367`,
`17b91ca`, `9d5b4f6`), each forced by a red hosted macOS run and each recorded as
a finding rather than quietly absorbed. Hosted run `37586963589` is green on all
nine jobs; the macOS lane's fresh `zsh -l -i -c` resolved the exact installed
fixture binary, and the same shell did not resolve it once the profile entry was
removed.

The publication condition was discharged by **v0.2.3** (tag `v0.2.3` at
`7b4c632`, immutable, automatic smoke `37652257315` green on five targets). The
status above therefore reads "closed".

Plan:

- `plans/implementation/distribution-release-update/c027-posix-installer-user-local-path-persistence-corrective.md`

Repository baseline: `59c0be9422e52b23bbb771fe94943cb004670b59`.

C027 corrects M010B forward. M010B required a normal POSIX install to report the
installed path and PATH guidance; the shipped wrapper satisfies that literal
contract but leaves `$HOME/.local/bin` unpersisted. That is especially visible
on a fresh macOS zsh account, where the verified binary may install successfully
and still not resolve in a newly opened terminal until the user manually edits
their shell configuration.

The corrective adopts the bounded Gregg-style model without copying unrelated
daemon/update behavior: profile integration is post-install UX, only for the
canonical non-root `$HOME/.local/bin` destination, only for supported zsh/bash
profiles, idempotent, non-evaluating, non-system-wide, and non-fatal after a
successful binary placement. A current-shell export remains necessary because a
piped installer cannot mutate its parent environment.

The existing fixture missed the defect because its happy path uses a custom
temporary `--dir` and passes when textual PATH guidance is emitted. C027
therefore requires state-transition tests plus hosted macOS evidence that a
fresh zsh process resolves the exact fixture-installed binary.

Implementation may close locally only conditionally. Final closure requires a
published immutable release carrying the corrected installer and public-release
macOS fresh-shell evidence. C027 does not itself authorize that publication.

**What happened.** The hosted-platform half is discharged; the public-release half
is not, because no release was cut. Nineteen POSIX cases assert the state
transition rather than the presence of guidance text, and every one was shown to
fail against a wrapper with the corresponding guard removed. Five mutants
initially passed the suite — the most instructive being two distinct
profile-failure diagnostics the tests could not tell apart, which is the same
defect shape as the original (an assertion that cannot distinguish the states it
claims to). One case, the profile-append failure, **skips on macOS with an
explicit reason**: it needs `RLIMIT_FSIZE` to bite, and macOS does not enforce it
on regular-file writes the same way Linux does. The skip is recorded in the closure
record rather than converted into a vacuous pass.
