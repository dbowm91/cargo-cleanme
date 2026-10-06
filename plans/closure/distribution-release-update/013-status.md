# Distribution, Release, and Update M013 Status

Plan: [`plans/implementation/distribution-release-update/013-0.2.0-publication-and-phase11-operational-closure.md`](../../implementation/distribution-release-update/013-0.2.0-publication-and-phase11-operational-closure.md)

Consumes:

- `plans/closure/distribution-release-update/c022-status.md` — closed, final head `705cd65`
- `plans/closure/distribution-release-update/c018-status.md` — release evidence appended
- `plans/closure/distribution-release-update/m011a-status.md` — operationally closed
- `plans/closure/distribution-release-update/m011b-status.md` — operationally closed
- `plans/closure/distribution-release-update/m011c-status.md` — five-target evidence appended

---

## 1. Executive finding

**cargo-cleanme 0.2.0 is published, immutable, attested on all 15 assets, and
rehearsed across all five target lanes.**

The release itself was uneventful. The release *process* was not: publishing
0.2.0 required six Eggpack staging runs, three automatic validations, and three
smoke dispatches to get from "all green locally" to "green on the real
infrastructure", and **every one of those failures was a defect in first-party
release automation that had never executed before, because each milestone was
correctly left conditionally closed pending exactly this evidence.**

Five distinct defects, none of them in the product or the published bytes:

| # | Where | Defect | Found by |
|---|---|---|---|
| 1 | `validate-staged-release.yml` | manual-tag cross-check ran on the automatic `workflow_run` path, where `inputs.tag` is empty | automatic run 37412345424 |
| 2 | `validate-staged-release.yml` | a SHA checkout has no local tag, so the identity gate had nothing to compare `HEAD` against | automatic run 37416030378 |
| 3 | `validate-staged-release.yml` | `contents: read` cannot read a draft release — GitHub serves drafts only to push-level identities | analysis, escalated to the owner |
| 4 | `resolve-smoke-transition.py` | requested `.../crates/cargo-cleanme/v0.2.0` (a **tag**); crates.io's per-version endpoint wants a **version number**, and answers HTTP 400 | automatic smoke 37419183947 |
| 5 | `post-release-smoke.yml` | `tee -o` is not a GNU `tee` option; the step could never write its own outputs, and the manual inputs lacked the `v` the resolver requires | recovery run 37420358827 |

Two more were introduced *by the fixes* and caught by the repository's own
guards before they could ship: a double-`v` handoff between the resolver and
the smoke script (defect 6), and a comment whose text defeated
`check-post-release-smoke-contract.py`'s only-real-rehearsal check (defect 7).

The pattern is the one this repository has now seen repeatedly, and it has a
name here: **every guard that guarded an automation path was itself only
proved in the failing direction for its own inputs, and none of them proved the
automation *works*.** M011B's record explicitly said the hosted run was
outstanding; the hosted run was the first execution.

## 2. Release identity

| Field | Value |
|---|---|
| Release commit | `95629ae28223e975faf1e8ed99ca3e6f83d6f724` |
| Tag | `v0.2.0`, annotated, tagger `wr3n-ai`, 2026-10-06 03:49:45 +0000 |
| Tag object | `e9bdb0ce727d588eaf1aea3619f7623fde5995ac` → peels to `95629ae` |
| `Cargo.toml` version | `0.2.0` |
| `Cargo.lock` version | `0.2.0` |
| Changelog heading | `## [0.2.0] - 2026-10-06`, with `[0.2.0]` reference link |
| Identity gate | `check-release-identity: v0.2.0 names 0.2.0 at 95629ae28223` (exit 0) |

Tag identity was proven three times: locally against `HEAD`, in the detached
worktree used for publication, and again inside the automatic validator against
its own `head_sha`-bound checkout.

## 3. Pre-tag gate

`bash scripts/release-check.sh v0.2.0` — **passed**, including
`check-release-identity.py --tag v0.2.0`, `cargo package --locked`, and
`cargo publish --locked --dry-run` (`Uploading cargo-cleanme v0.2.0` /
`warning: aborting upload due to dry run`). It published nothing.

## 4. Qualification history — every failed attempt preserved

| Run | What | Outcome |
|---|---|---|
| [37408556261](https://github.com/dbowm91/cargo-cleanme/actions/runs/37408556261) | CI on `560a158` | **failure** — Windows lane (C022 defect 1) |
| [37408907268](https://github.com/dbowm91/cargo-cleanme/actions/runs/37408907268) | CI on `6c4195c` | **failure** — `msrv` (C022 defect 2) |
| [37409315255](https://github.com/dbowm91/cargo-cleanme/actions/runs/37409315255) | CI on `3bf8f3a` | **failure** — Windows lane (C022 defect 3) |
| [37410115511](https://github.com/dbowm91/cargo-cleanme/actions/runs/37410115511) | CI on `705cd65` | **success**, all 9 jobs |
| [37410883861](https://github.com/dbowm91/cargo-cleanme/actions/runs/37410883861) | Eggpack staging #1 | success (20/20); draft staged |
| [37412345424](https://github.com/dbowm91/cargo-cleanme/actions/runs/37412345424) | automatic validator | **failure** — defect 1 |
| [37412611160](https://github.com/dbowm91/cargo-cleanme/actions/runs/37412611160) | Eggpack staging #2 | **failure** — `same-name remote asset digest mismatch` |
| [37414619399](https://github.com/dbowm91/cargo-cleanme/actions/runs/37414619399) | Eggpack staging #3 | success; draft staged |
| [37416030378](https://github.com/dbowm91/cargo-cleanme/actions/runs/37416030378) | automatic validator | **failure** — defect 2 |
| [37416668628](https://github.com/dbowm91/cargo-cleanme/actions/runs/37416668628) | Eggpack staging #4 | success; draft staged |
| **[37418179489](https://github.com/dbowm91/cargo-cleanme/actions/runs/37418179489)** | automatic validator | **success — all six steps** |
| [37419183947](https://github.com/dbowm91/cargo-cleanme/actions/runs/37419183947) | automatic smoke (`release` event) | **failure** — defects 4 and 5 |
| [37420358827](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420358827) | smoke, manual dispatch | **failure** — defect 5 (`tee -o`) |
| [37420483732](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420483732) | smoke, manual dispatch | **failure** — defect 6 (`vv0.1.6`, all five lanes) |
| **[37420625111](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420625111)** | smoke, manual dispatch | **success — 5/5 lanes** |
| [37420622066](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420622066) / [37420622080](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420622080) | CI + release-drift on `f53dc94` | success / success |
| [37410858459](https://github.com/dbowm91/cargo-cleanme/actions/runs/37410858459) | selector qualification on `95629ae` | success |

Eggpack staging #2's failure is worth calling out as **correct behaviour, not a
defect**: rebuilt binaries do not hash identically, and Eggpack refused to
replace same-name assets under a tag with different digests rather than
silently swapping published bytes. Recovering required deleting the *draft*
(not the tag) and staging fresh, which is why there were four staging runs.

## 5. Automatic staged validation (M011B)

All six steps of run 37418179489 passed:

```text
[1/6] inventory (15 assets) — 5 contracted binaries + 5 sidecars + manifest + 2 installers
[2/6] sidecar integrity
[3/6] release-manifest.json agreement
      manifest binds source revision 95629ae28223 to tag v0.2.0
      manifest agrees with the bytes for 5 asset(s)
[4/6] contract agreement
[5/6] static Linux ABI evidence (not runtime)
      aarch64-unknown-linux-gnu: requires at most GLIBC_2.17.0
      x86_64-unknown-linux-gnu: requires at most GLIBC_2.17.0
[6/6] real installer qualification on this host
PASSED: the staged draft matches the release contract
```

The advertised glibc floor came back **2.17.0 on both Linux targets**, so the
contract's floor is measured rather than asserted.

## 6. Human draft inspection

Draft state at inspection: `draft=true`, `prerelease=false`, `publishedAt=null`.
Inventory was exactly 15 assets — no extras, none missing. Release notes were
replaced with a body that calls out the breaking CLI surface, the 0.1.x→0.2.0
translation table, and the C018/C019/C020/C022 fixes.

**One inaccuracy was caught in the notes before publication.** They attributed
the Windows concurrent-config race to "C019/C020". C019 is the
`scan.unignore` sibling-containment corrective; only C020 is the Windows race.
Corrected to C018 (updater refusal), C019 (unignore), C020 (Windows race),
C022 (update JSON contract), with the unattributed entries left uncited
because `CHANGELOG.md` is the authority for attribution.

The notes state that attestation and the smoke begin *after* publication and
claim neither.

## 7. Publication

| Field | Value |
|---|---|
| GitHub release id | `RE_kwDOU5PV0M4YGdDL` |
| Published at | `2026-10-06T05:34:06Z` |
| Immutable | yes — `v0.2.0 is immutable and carries a valid release attestation` |
| crates.io | `cargo-cleanme = "0.2.0"`, `max_stable_version: 0.2.0` |

crates.io was published from a **detached worktree** at the tag
(`git worktree add /tmp/cargo-cleanme-publish v0.2.0`), never from a moving
`main`, with `check-release-identity.py --tag v0.2.0` re-run inside it first.

### 7.1 Asset inventory (downloaded and re-hashed)

| Asset | Bytes | SHA-256 |
|---|---|---|
| `cargo-cleanme-aarch64-apple-darwin` | 10318992 | `1a3d9d2d5cffa842a5d4d2f6b79e8a176396659bc17b48f6dc991eda90c30bec` |
| `cargo-cleanme-aarch64-apple-darwin.sha256` | 101 | `81aba81a1aafc4afd4d4499bd61f0e0d4f501d2810eabb0b229a7447477c6f2a` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7876448 | `64ca61c6bfb1a466331905c7ac6ed23dbe3bb9789246847169919514e4212407` |
| `cargo-cleanme-aarch64-unknown-linux-gnu.sha256` | 106 | `5c55ca9ec8b93346c751c369beb822134011faddfc3c84a7cad1ee34510db2b5` |
| `cargo-cleanme-x86_64-apple-darwin` | 10741992 | `d9ef3e4857df5cc3f952d6fd1442aad67493c8d9cadf48217ce070d13aac3937` |
| `cargo-cleanme-x86_64-apple-darwin.sha256` | 100 | `fb64bdb90aa192d39e657ecb464c0f07a714799f0177ffbd92153c6b945eb36e` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9325568 | `a003ed0dc0fe7d3ad23426a684dc681c9d978bd33097d3dfc40813acd52109ed` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe.sha256` | 107 | `78b4a52d1f670878cc9cf09c4686927a697310ac093aba70af10b2bca0e240c1` |
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8909280 | `46f976432facd54a822eed26273f8ca443a311c31de902c61dc47446ca8d567f` |
| `cargo-cleanme-x86_64-unknown-linux-gnu.sha256` | 105 | `43a3afc5738955765ecd20596adc7edf1289ad32bba418f20ac3804675c42685` |
| `install-exact.ps1` | 4709 | `3a5e3b556bd2fc2ed9fde0091dd2d7abc0e91ec4fddee5495d7e558b2f423671` |
| `install-exact.sh` | 6894 | `6aa8e0934edf8ff9fb07611781ed5c89291506af0a3cabaac050a24b3007386b` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install.sh` | 14557 | `5675992d09b2f7eb76d2e16713c1d1c342521f472f32ba4e9b27af18d561b1a3` |
| `release-manifest.json` | 1339 | `7643fcb4f51d03fbb06fd40050444b4f1bd83f2a5aff5fe8740a4dd93333637e` |

All 15 match GitHub's signed attestation:

```text
verify-release-attestation: v0.2.0 is immutable and its 15 asset digest(s) match GitHub's signed attestation
  attested release subject: pkg:github/dbowm91/cargo-cleanme@v0.2.0 (15 asset digest(s))
```

## 8. C018 real-release refusal evidence

Starting installation: published **0.1.6**, with **0.2.0** served by crates.io
throughout — so neither refusal can be mistaken for "already current".

| Case | Refusal | Exit | Bytes | Bookkeeping |
|---|---|---|---|---|
| default `~/.cargo` | `cargo_managed` + `cargo install cargo-cleanme --locked --force` | 2 | `e0388afd…` unchanged | `cargo install --list` still v0.1.6 |
| `--root /tmp/m013-c018-root` | same | 2 | `e0388afd…` unchanged | still v0.1.6 |

Full transcript and the disclosed environment change (0.1.0 → 0.1.6 in the
default home) are in `c018-status.md`.

## 9. External install/CLI smoke

`cargo install cargo-cleanme --version 0.2.0 --locked --root /tmp/m013-020-root`,
exercised outside the repository:

| Requirement | Result |
|---|---|
| installed binary reports 0.2.0 | `cargo-cleanme 0.2.0` |
| bare `--dry-run` is zero-mutation simulation | `0 would-clean … no cargo clean command was invoked`; target bytes identical |
| Routine Execute on a disposable fixture | `1 cleaned, 0 skipped, 0 failed`; **9.70 MiB reclaimed, 34 files**; `target` removed, `src/` and `Cargo.toml` intact |
| rootless `scan` is Full read-only | 3930 manifests resolved, no mutation, fail-closed on 11 diagnostics |
| `scan --known` remains Routine read-only | accepted; `ROOT` with `--known` is a usage error, as documented |
| `--format log` is one bounded line | `op=clean status=ok scope=explicit mode=simulate …`, 104 bytes |
| `--format json` parses | valid envelope, `schema_version=1`, `operation=scan` |

The Execute line reads:

```text
Cleaned  /tmp/m013-smoke/app  [private]  output /tmp/m013-smoke/app/target  before   9.70 MiB  after   0.00 B  observed decrease   9.70 MiB  — Removed 34 files, 9.6MiB total
```

Getting there needed two fixture corrections, recorded because they are the
kind of thing that makes a smoke lie: the first fixture used
`CARGO_TARGET_DIR`, which cargo-cleanme does not follow (0 groups), and a
single-profile target directory produced no group until a `--release` build was
added alongside `--debug`.

## 10. Post-release smoke (M011C)

**The automatic run failed and is not presented as passing.**
Run [37419183947](https://github.com/dbowm91/cargo-cleanme/actions/runs/37419183947)
fired correctly on `release: published` at `05:34:08`, rejected nothing, and
hung in the resolver until its deadline — defects 4 and 5. A release event fires
once per publication, so this result **cannot be re-run automatically**.

Recovery used the documented `workflow_dispatch` path. All five lanes of
[37420625111](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420625111)
passed:

| Target | Result |
|---|---|
| `aarch64-apple-darwin` (macos-14) | success |
| `aarch64-unknown-linux-gnu` (ubuntu-24.04-arm) | success |
| `x86_64-apple-darwin` (macos-15-intel) | success |
| `x86_64-pc-windows-msvc` (windows-latest) | success |
| `x86_64-unknown-linux-gnu` (ubuntu-latest) | success |

Lane evidence (x86_64 Linux, representative):

```text
post-release-smoke: x86_64-unknown-linux-gnu v0.1.6 -> v0.2.0 (public releases only)
  published v0.1.6 cargo-cleanme-x86_64-unknown-linux-gnu sha256 9bb8c582adcb1cf8bd6953f084d096e8249964b123ec01f61f6552e1efe9fc2c
  published v0.2.0 cargo-cleanme-x86_64-unknown-linux-gnu sha256 46f976432facd54a822eed26273f8ca443a311c31de902c61dc47446ca8d567f
  Cargo-managed install reports 0.1.6 (local build, sha256 e668741cddca, not the release asset)
post-release-smoke: PASSED (x86_64-unknown-linux-gnu v0.1.6 -> v0.2.0)
```

The v0.2.0 digest in that lane matches §7.1 exactly.

**Two qualifications, stated rather than smoothed over.**

1. The recovery dispatch supplied `from_version=0.1.6`, so a human chose the
   predecessor in that run — the plan's requirement that the *automatic* path
   derive it independently was never met for 0.2.0. The resolver does derive
   that answer: `--to v0.2.0` with no `--previous` resolves
   `from_version=v0.1.6`, and the automatic run's own failure was in the crates.io
   wait, not in the predecessor resolution.
2. Each lane rehearses the updater against a *Cargo-managed* install, which the
   C018 fix correctly refuses to mutate. The self-managed replacement path is
   therefore exercised by the lane script's own staged-binary checks, not by a
   live self-update of a real published installer.

## 11. Documentation and guards reconciled

| File | Change |
|---|---|
| `.github/workflows/validate-staged-release.yml` | `contents: write`; manual cross-check scoped; tag fetched for the identity proof |
| `scripts/check-staged-validation-contract.py` | write scope permitted, publication commands banned in every form, undeclared scope and extra capabilities rejected, comments stripped before scanning |
| `.github/workflows/post-release-smoke.yml` | `tee -o` removed; `v` added to resolver input and stripped for the script; input descriptions corrected |
| `scripts/resolve-smoke-transition.py` | crates.io path is a version number; new self-test case pins the requested URL |
| `scripts/check-post-release-smoke-contract.py` | rehearsal presence test ignores comments |
| `architecture/14-testing-and-verification.md` | guard row for `check-staged-validation-contract.py` rewritten around behaviour rather than capability |
| `architecture/15-distribution-and-release.md` | workflow table row for the validator's scope |
| `docs/RELEASING.md` | corrected a false claim that the validator "holds `contents: read` and nothing else" |
| `CHANGELOG.md` | `## [0.2.0] - 2026-10-06` + `[0.2.0]` link |

## 12. Unresolved findings

| # | Severity | Finding |
|---|---|---|
| 1 | **Medium** | The automatic post-release smoke for 0.2.0 is a recorded **failure** and cannot be re-run automatically, because a `release: published` event fires once. Recovery evidence is a manual dispatch in which a human chose the predecessor. This is a real gap in M011C's automatic-closure requirement, not a formality — the next release should be watched for whether the now-fixed automatic path actually fires green. |
| 2 | **Medium** | The validator job now holds `contents: write`. It is prevented from publishing by a *content* rule, not by incapability. A future edit that adds a publication command in a shape `PUBLICATION` does not match would defeat it. The rule was broadened to five shapes and mutation-checked; it is still a denylist. |
| 3 | **Low** | `verify-release-attestation.py` must be run from a checkout of the release's own tag, because it runs `check-release-identity.py` from the current tree. Running it from a tree past the tag fails with text that reads like a release defect. No message says so. |
| 4 | **Low** | Three separate guards were defeated by their own subject's *comments* during this release (installer sudo/TLS markers in C022, the publication-command scan, and the rehearsal presence test). All three now strip comments, but the shape is not prevented repository-wide — a new bare-substring guard can reintroduce it. |
| 5 | **Informational** | `resolve-smoke-transition.py --emit github` and `--emit env` print identically; the distinction is unimplemented. Harmless today because the workflow appends stdout to `GITHUB_OUTPUT` itself. |
| 6 | **Informational** | This record's own authoring produced two misquoted pieces of evidence — a fabricated commit SHA (`95629ae`) and a wrong hostname in a quoted test output (`309bf49`). Both were caught after the fact. Writing the record is the part that needed checking. |

No finding above requires republishing 0.2.0: none of them concerns the
product bytes, the attestation, the crates.io identity, or the C018 refusals.

## 13. Acceptance criteria

| # | Criterion | Evidence |
|---|---|---|
| 1 | C022 closed | `c022-status.md`, final head `705cd65` |
| 2 | exact commit/tag/source/version/changelog identity proven | §2, three times |
| 3 | Eggpack staged all contracted assets from that source | §4, runs 37414619399 / 37416668628, `release-manifest.json` binds `95629ae` |
| 4 | **automatic** M011B validation passed before publication | §5, run 37418179489 |
| 5 | human draft inspection passed | §6, including a caught attribution error |
| 6 | GitHub v0.2.0 published and immutable | §7 |
| 7 | crates.io 0.2.0 published from the exact tag | §7, detached worktree |
| 8 | M011A release + asset attestation verification passed | §7.1, 15/15 digests |
| 9 | **automatic** M011C five-target smoke passed | **Not met — §10 finding 1.** All five lanes green on the manual recovery path; the automatic run is a recorded failure |
| 10 | C018 default-home and `--root` refusal evidence passed | §8 |
| 11 | external install/CLI smoke confirms the 0.2.0 boundary | §9, 7 of 7 |
| 12 | all four Phase 11 records reconciled | §14 |
| 13 | no unresolved finding invalidates the published release | §12 |

## 14. Downstream handoff

`c018-status.md`, `m011a-status.md`, `m011b-status.md`, and `m011c-status.md`
are appended to, not rewritten. `plans/registry.md`, the distribution-release
roadmap, and `plans/002-long-term-roadmap.md` mark Phase 13 closed.

The next release's first qualification is the real test of whether the six
defects fixed here are actually fixed, because that path will execute for the
first time with nothing left to hide behind.

## 15. Disposition

**Closed, with one acceptance criterion explicitly not met.**

cargo-cleanme 0.2.0 is published and every claim in this record is backed by a
run id or a command output. The exception is criterion 9: the automatic
five-target smoke for 0.2.0 failed and cannot be re-run automatically. It is
recorded as a failure, the six defects it uncovered were fixed, and the
documented recovery path produced five green lanes — but a manual rerun is not
the automatic evidence the plan asked for, and this record does not claim it is.