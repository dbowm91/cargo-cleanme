# cargo-cleanme v0.2.9 — release record

Release record for the **seventeenth** published release. It closes the
publication condition on C037 and M017, and it is the record that the scoped
proof default (M017 / ADR 007) reached the public.

| | |
|---|---|
| Version | 0.2.9 |
| Tag | `v0.2.9` at `8c95c2ba416b298c9aa7ebe541115240feccd4b0` |
| GitHub release | published 2026-10-19T20:31:06Z, immutable, 15 assets |
| crates.io | published 2026-10-19T20:32:08Z, sha256 `9c1461bcf6806f6511bee89a20cf16c472e4849f825b426315fab29b0d923d7d`, not yanked |
| Automatic smoke | `39319974151` — green on all five target lanes, `from_version=v0.2.8` → `to_version=v0.2.9` |
| Closed by this release | **C037**, **M017** (publication condition discharged) |
| Implementation commit | `27ab9e3` (the C037 repair) and `8c95c2b` (the release commit) |

Prior records: `r026` (v0.2.6). There is no `r027` or `r028`; v0.2.7 and
v0.2.8 were published without a receipt. That is pre-existing debt, recorded
in the registry, and **not** reopened here — but this release does not repeat
it.

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The tree's version was already 0.2.9; the changelog did not say so | `Cargo.toml` `version = "0.2.9"`, `Cargo.lock` 0.2.9, `CHANGELOG.md` still `[Unreleased]` |
| 2 | The changelog was converted to a release entry | commit `8c95c2b`, `## [0.2.9] - 2026-10-19` |
| 3 | CI on that commit | `39314578772` — all nine jobs green |
| 4 | The tag was created and the identity check run | `check-release-identity.py --tag v0.2.9` → "v0.2.9 names 0.2.9 at 8c95c2ba416b", exit 0 |
| 5 | **The complete local gate** | `bash scripts/release-check.sh v0.2.9` → exit 0, "release-check: passed; no publication was performed" |
| 6 | The tag was pushed | `git push origin v0.2.9` → `v0.2.9 -> v0.2.9` |
| 7 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.9` → run `39315647877` |
| 8 | The staged draft was inspected | 15 assets, every digest recomputed locally — §3 |
| 9 | The staged-release validation was inspected and awaited | run `39319286668` — green; the retained validation log is quoted in §3 |
| 10 | The draft was published | `gh release edit v0.2.9 --draft=false` → published 2026-10-19T20:31:06Z, immutable |
| 11 | The crate was published from a detached checkout of the tag | `git worktree add --detach /tmp/cargo-cleanme-publish v0.2.9` → `cargo publish --locked` → "Published cargo-cleanme v0.2.9 at registry `crates-io`" |
| 12 | The attestation was verified, twice | §6 |
| 13 | The automatic published-release smoke was awaited | run `39319974151` — §7 |
| 14 | The external smoke was run from outside the repository | §8 |

The crate was published from a **detached worktree of the tag**, as
`docs/RELEASING.md` requires — not from `main`. `git worktree remove --force
/tmp/cargo-cleanme-publish` afterwards; `git worktree list` shows only the
main worktree.

## 2. Where this release deviated, and why each is stated

### 2.1 The release notes were curated, not left as the Eggpack placeholder

v0.2.6 and v0.2.7 were published with Eggpack's placeholder body ("Draft
binary release staged by Eggpack…"); v0.2.8 was published with a curated
body. This release follows **v0.2.8**, the most recent precedent: the notes
were written while the release was still a draft, and were read back before
the draft was flipped public. The placeholder body is a staging artefact, not
release notes, and leaving it in place is the weaker choice.

### 2.2 The local gate ran *after* the tag was created

`scripts/release-check.sh` requires the tag to exist locally — the identity
checker resolves `v0.2.9` and asserts it names 0.2.9 at the current commit
(`scripts/check-release-identity.py:91-147`). The tag was therefore created
locally, the gate was run against it, and only then was the tag pushed. A
local tag that was never pushed is not a publication; the ordering keeps the
gate's identity check meaningful instead of skipping it.

### 2.3 The release notes claim is scoped to what shipped

The notes state that the selector test's premise was unstated and that the
tool was never wrong. That is what the evidence shows: the tool refuses an
unqualified Cargo *in the safe direction*, and the repair changed no
production behaviour. The notes do not claim the tool gained a capability.

## 3. The review gate, performed before publication

The publication gate requires four things, and all four were satisfied before
the draft was flipped:

1. **The Eggpack candidate build was green.** Run `39315647877`, all 19 jobs
   green (5 builds, 5 qualifications, 5 validations, plus `preflight`,
   `resolve`, `required_gate`, `aggregate`, `stage`).
2. **The staged-release validation was green for the same tag and the same
   source SHA.** Run `39319286668`. Its retained log
   (`staged-release-validation-v0.2.9`, 1021 bytes) reads:

   ```text
   Validating staged draft v0.2.9
   [1/6] inventory (15 assets)
     … 5 contracted binaries + 5 sidecars + manifest + 2 installers
   [2/6] sidecar integrity
     ok cargo-cleanme-aarch64-apple-darwin  3324cbfe202d4bef…
     ok cargo-cleanme-aarch64-unknown-linux-gnu  abe23c5db64459da…
     ok cargo-cleanme-x86_64-apple-darwin  d83e54dc407a6b1f4e6d…
     ok cargo-cleanme-x86_64-pc-windows-msvc.exe  94a5257c1702c8c1ef…
     ok cargo-cleanme-x86_64-unknown-linux-gnu  c382668db129f97213…
   [3/6] release-manifest.json agreement
     manifest binds source revision 8c95c2ba416b to tag v0.2.9
     manifest agrees with the bytes for 5 asset(s)
   [4/6] contract agreement
     … 5 targets → asset names, sizes and digests
   [5/6] static Linux ABI evidence (not runtime)
     aarch64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
     x86_64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
   [6/6] real installer qualification on this host
     installed the real cargo-cleanme-x86_64-unknown-linux-gnu and it reports 'cargo-cleanme 0.2.9'
     the installed bytes match the release asset digest exactly
   PASSED: the staged draft matches the release contract
   ```

3. **The validator's summary was read by a human.** Exact tag `v0.2.9`; source
   SHA `8c95c2ba416b`; upstream run `39315647877`; inventory count 15; measured
   glibc floors 2.17.0 on both Linux targets; installer result: the real binary
   installed and reported its version.
4. **The crate version and the release tag agreed**, and no public release
   existed for this version beforehand: `check-release-identity.py` exit 0, and
   crates.io returned 404 for `0.2.9` before the upload.

### The independent local inspection

The draft's 15 assets were downloaded and **every** digest recomputed from
the bytes, then compared four ways: the local recomputation, the
`release-manifest.json`, each binary's `.sha256` sidecar, and GitHub's own
reported asset digest. Sizes were compared against the manifest. All fifteen
agreed, for all five binaries, and the inventory was exactly the contracted
set (15 of 15, no extra, none missing) — `armv7-unknown-linux-gnueabihf`
absent by contract.

The staged `cargo-cleanme-x86_64-unknown-linux-gnu` was also run directly:
`cargo-cleanme 0.2.9`, a stripped PIE ELF, with a measured glibc floor of
`GLIBC_2.17`.

## 4. Crate publication

`check-release-identity.py --tag v0.2.9` was run first ("v0.2.9 names 0.2.9 at
8c95c2ba416b", exit 0). `cargo publish --locked` uploaded and reported:

```text
Published cargo-cleanme v0.2.9 at registry `crates-io`
```

crates.io confirms version `0.2.9`, `yanked: false`, sha256
`9c1461bcf6806f6511bee89a20cf16c472e4849f825b426315fab29b0d923d7d`. This is
the **seventeenth** published release and the **sixteenth** installable one;
`0.2.0` remains the only yanked release.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes, and each binary
digest was checked against its sidecar, against the release manifest, and
against GitHub's reported digest. All agree.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9234704 | `c382668db129f972131e4792400853753a09aa823ce79a571d1f9197325d0485` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8145496 | `abe23c5db64459da7c1f221df6ecf4eea44d68ac679d752e7a0d4777534808bc` |
| `cargo-cleanme-x86_64-apple-darwin` | 11121200 | `d83e54dc407a6b1f4e6d30ad3459595af173d750291eb4b55fc1b4f6b98605a0` |
| `cargo-cleanme-aarch64-apple-darwin` | 10640624 | `3324cbfe202d4bef58042f3c4671b5e0da1752039714aebdacca3e7eff8b25dc` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9673728 | `94a5257c1702c8c1efedca060c4bd09bc0c60ea20b02a0dd95d29e51665afef4` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `c63f89be3b3dd2b7578043d4fdfac0b090a3e9a0832919f8ee8a02ce5bc13a73` |
| `install-exact.ps1` | 4709 | `633bdfd9e7a699e2d8702e89f3eefd966e3cf05039cb8355af5cdaf83720ea53` |
| `release-manifest.json` | 1339 | `998f07e9f4aad8a2b998d571e410b22b2a3ae40f08763559d0ab396ece5f6052` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are **byte-identical to v0.2.8's**
(`cc59833b…`, `e8268523…`) — the machine-checkable form of "installer
behaviour is unchanged by this release", which touched `src/` only for the
scoped-proof default and the test repair.
`install-exact.sh`/`install-exact.ps1` differ from v0.2.8's **only** in the
pinned release version in their download URLs (`…/download/v0.2.8/…` →
`…/download/v0.2.9/…`); that is their designed per-release change, and the
diff is exactly those lines.

## 6. Public-release evidence

The release is **immutable** and carries a valid signed attestation:

```text
verify-release-attestation: v0.2.9 is immutable and carries a valid release attestation
  attested release subject: pkg:github/dbowm91/cargo-cleanme@v0.2.9 (15 asset digest(s))
```

Re-run against the downloaded bytes, the attestation and the bytes agree:

```text
verify-release-attestation: v0.2.9 is immutable and its 15 asset digest(s) match GitHub's signed attestation
```

## 7. The automatic published-release smoke

`post-release-smoke.yml` ran automatically on the `release: published` event as
run `39319974151`: transition resolution (`from_version=v0.2.8`,
`to_version=v0.2.9`, confirmed in the resolve job's log) plus **all five
target lanes green**, public releases only. No lane was retried and none was
skipped; per the project rule, a skipped smoke and a passing smoke must never
be confusable, and this one is unambiguously the latter.

This is the **ninth** consecutive green automatic smoke. The two historical
failures (v0.2.0 `37507097075`, v0.2.1 `37507738147`) stay recorded as
failures in their own records.

## 8. External smoke, from outside the repository

Run from `/tmp/smoke029`, with an isolated `HOME`/`CARGO_HOME`
(the `HOME` override hides rustup's toolchain directory, so `RUSTUP_HOME` was
pointed at the operator's real one) so the operator's own installation is
untouched, resolving only registry dependencies, against the published
artifacts.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --locked` | resolved registry-only and installed `cargo-cleanme v0.2.9` |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.9` |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.9` |
| `cargo cleanme config show` | prints the effective config; first-use bootstrap works in the isolated home; exit 0 |
| `cargo cleanme scan --format json --no-progress <absolute path>` | exactly one JSON document on stdout (`schema_version: 1`, `operation: scan`); exit 0 |
| `cargo cleanme update --dry-run` | `already at the latest stable version (0.2.9; published is 0.2.9)`, **exit 0** — this also proves the live crates.io version-authority path works from the published binary |
| bytes after the update check | **unchanged** (sha256 `5e0eb2f2…` identical before and after) |
| Cargo bookkeeping after the run | the isolated root's `.crates.toml` records `"cargo-cleanme 0.2.9 (registry+…)" = ["cargo-cleanme"]` |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report
no groups while looking healthy.

**The Cargo-managed refusal path was not exercised, and could not be.** The
refusal fires when a *newer* published version exists to update to; 0.2.9 is
the newest release, so `update --dry-run` short-circuits on an empty plan
before the provenance gate. The v0.2.6 record exercised this path precisely
because 0.2.7 had been published. This is a property of being the newest
release, not a defect, and the path is covered by the crate's own tests
(`src/update.rs:1866`).

## 9. Completions, manpages, benchmark, size

The 13 generated artefacts (5 completions, 8 man pages) matched the clap model
at the release commit — `generate-docs --check` reported "13 artifacts match
the clap model", and the `generated-docs` CI job was green. The release
builder is pinned to 1.99.0; the 1.99 clippy lint that would have broken it
(`904fc17`) is fixed and shipped in this release.

## 10. Effect on closures

- **C037 — closed.** The corrective is carried by a published release, which
  was its only remaining condition. See
  `plans/closure/artifact-discovery-cleanup/c037-status.md`.
- **M017 — publication condition discharged.** The scoped proof default
  (ADR 007) shipped in v0.2.9. Its two stated limitations are both
  resolved: the carried red test is fixed by C037, and the hosted
  `cli_contract`/`end_to_end` integration evidence — which the failing
  `--lib` target had been suppressing — is now in hand (the ubuntu lane
  reports 449 lib, 62 `cli_contract` and 4 `end_to_end`, none failed). See
  `plans/closure/artifact-discovery-cleanup/m017-status.md`.
- **C032 — publication condition discharged.** v0.2.7 already carried the
  C032–C034 diagnostics/reporting line; v0.2.9 carries it again. See
  `plans/closure/artifact-discovery-cleanup/c032-status.md`.
- **M016 — unchanged.** It was superseded by M017 before any publication, and
  0.2.9 confirms it: no `--partitioned` flag and no `result.islands` field
  exist in the published binary. Its record stays *conditionally closed*,
  never published.
- **C033, C034 — unchanged.** Their retirement and safe-track qualification
  are separate tracks, not publication-blocked.

## 11. Known limitations and unresolved findings

- **None medium or higher.** The release gate, the staged-release
  validation, the attestation, the automatic smoke and the external smoke
  were all green, and the independent local inspection of all 15 assets
  agreed four ways.
- **Low (evidence shape, not a defect):** the Cargo-managed refusal path was
  not exercised from the published binary, because 0.2.9 is the newest
  published version (§8). It will be exercisable from the next release.
- **Low (coverage, not a defect):** the *supported* branch of the profile
  selector is still only executed against a real Cargo on a qualified host.
  The hosted `stable` is 1.99.1 and the `msrv` lane's 1.89.0 is the nearest
  qualified toolchain, so the qualified-host execution of the amended case is
  not in the hosted evidence. The composition is covered host-independently
  and the gate is covered by mutation in both directions. This is a property
  of the qualification policy, recorded in
  `plans/closure/artifact-discovery-cleanup/c037-status.md` §8.
- **Debt carried forward:** v0.2.7 and v0.2.8 were published without a
  receipt. This release has one.

## 12. Disposition

**Closed.** The release was published on both channels — the GitHub release is
public and immutable with a valid attestation, and the crate is on crates.io
and not yanked — and every gate the project defines for a publication was
executed and recorded above. The publication conditions on C037, M017 and
C032 are discharged.
