# cargo-cleanme v0.2.10 — release record

Release record for the **eighteenth** published release. It carries **C039**
(the compact interactive selection surface) and **C040** (ADR 009 — the working
directory joins the maintenance scope) to the public, and it discharges the
publication condition on **C038** (the negative-result verdict), which was
already implemented and hosted-green.

| | |
|---|---|
| Version | 0.2.10 |
| Tag | `v0.2.10` at `270cceb2be318ab6e2f7da87a90d5ce73f7f292a` |
| GitHub release | published 2026-10-21T00:35:53Z, immutable, 15 assets, release id `419004194` |
| crates.io | published 2026-10-21T00:36:36Z, sha256 `0f2e471bcdf53b4c108b0ed0c1532fa30277b38bdb88854493f5b6da9248cd0b`, not yanked |
| Automatic smoke | `39522277814` — green on all five target lanes, `from_version=v0.2.9` → `to_version=v0.2.10` |
| Closed by this release | **C040**, **C039**, **C038** (publication condition discharged) |
| Implementation commits | `382192d` (C039), `462a996` (C038), `c9f7784` (C040), `270cceb` (the release commit) |

Prior record: `r029` (v0.2.9). The v0.2.7 and v0.2.8 receipts are still
missing; that debt is recorded in the registry and is **not** reopened here.

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The tree's version was already 0.2.9 and 0.2.9 **was published** | `AGENTS.md` and the release skill still claimed otherwise; both were corrected in this release |
| 2 | The version was bumped to 0.2.10 | `Cargo.toml`, `Cargo.lock`; `## [0.2.10] - 2026-10-20` in `CHANGELOG.md` |
| 3 | The generated artifacts were regenerated | 8 man pages carry the version; `generate-docs -- --check` is green |
| 4 | **The complete local gate** (no tag yet) | `bash scripts/release-check.sh` → exit 0, "release-check: passed; no publication was performed" |
| 5 | The tag was created locally and the gate re-run **with** it | `bash scripts/release-check.sh v0.2.10` → exit 0, "v0.2.10 names 0.2.10 at 270cceb2be31" |
| 6 | The tag and the branch were pushed | `git push origin main` and `git push origin v0.2.10` |
| 7 | CI on the release commit | `39517670642` (push) and `39517674568` (tag push) — all nine jobs green on both |
| 8 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.10` → run `39518072322`, all 19 jobs green |
| 9 | The staged draft was inspected | 15 assets, every digest recomputed locally — §3 |
| 10 | The staged-release validation was inspected and awaited | run `39520043716` — green; its retained log is quoted in §3 |
| 11 | The draft was published | `gh release edit v0.2.10 --draft=false --notes-file …` → published 2026-10-21T00:35:53Z, immutable |
| 12 | The crate was published from a detached worktree of the tag | `git worktree add --detach /tmp/cargo-cleanme-publish v0.2.10` → `cargo publish --locked` → "Published cargo-cleanme v0.2.10 at registry `crates-io`" |
| 13 | The attestation was verified, twice | §6 |
| 14 | The automatic published-release smoke was awaited | run `39522277814` — §7 |
| 15 | The external smoke was run from outside the repository | §8 — **and it failed on the first attempt; §8.1 says why, and §8.2 says what was re-run** |

The crate was published from a **detached worktree of the tag**, as
`docs/RELEASING.md` requires — not from `main`.

## 2. Where this release deviated, and why each is stated

### 2.1 The release notes were curated, not left as the Eggpack placeholder

This follows **v0.2.8**, the most recent precedent: the notes were written
while the release was still a draft, and were read back before the draft was
flipped public. The placeholder body is a staging artefact, not release
notes, and leaving it in place is the weaker choice.

The notes describe the three shipped changes in user terms — the working
directory joining the scope, the compact selection surface, and the
negative-result verdict — and state the one thing a reader needs to act on:
the working directory joins only when the known roots do not already reach
it, so sweeping everything under a home directory stays the explicit
`clean ROOT` spelling.

### 2.2 The gate ran **twice**, and the second run carried the identity check

`scripts/release-check.sh` requires the tag to exist locally — the identity
checker resolves `v0.2.10` and asserts it names 0.2.10 at the current commit.
The gate was therefore run **once before the tag** (self-test mode, identity
check skipped) and **once after the tag existed locally**, so the real
identity check ran *inside* the gate. This is stronger than the v0.2.9
precedent, which recorded "the local gate ran after the tag was created" as a
deviation. The order was: commit → gate (no tag) → tag → gate (with tag) →
push.

### 2.3 The `Changed — BREAKING` heading

The changelog section is headed `Changed — BREAKING`, because the bare
invocation's *effect* changes — it now covers the working directory — even
though the spelling, the JSON contract, the log line and every
ownership-proof gate are unchanged. A 0.2.9→now mapping table sits beside
it. No script parses that heading; it exists for the reader.

### 2.4 The release gate failed once, and the tree was not changed

`scripts/release-check.sh` failed on its **first** run, on one test
(`update::tests::a_staging_cleanup_failure_is_reported_not_swallowed`), and
passed on the second run with an **identical tree**. The failure was
`spawning bounded candidate: Text file busy (os error 26)` — a
load-dependent flake in the update path, in code this release did not touch.
It was **not** fixed inside the release: a change to the update path is
safety-adjacent and deserves its own review, and the intermittency was not
understood well enough to fix responsibly. It is recorded as
`plans/implementation/artifact-discovery-cleanup/c041-update-candidate-spawn-etxtbsy-corrective.md`
and in §11.

The second run is the one recorded as the gate's result. Both facts are
stated here rather than only the passing one.

## 3. The review gate, performed before publication

The publication gate requires four things, and all four were satisfied before
the draft was flipped:

1. **The Eggpack candidate build was green.** Run `39518072322`, all 19 jobs
   green (5 builds, 5 qualifications, 5 validations, plus `preflight`,
   `resolve`, `required_gate`, `aggregate`, `stage`).
2. **The staged-release validation was green for the same tag and the same
   source SHA.** Run `39520043716`. Its retained log
   (`staged-release-validation-v0.2.10`) reads:

   ```text
   Validating staged draft v0.2.10
   [1/6] inventory (15 assets)
     … 5 contracted binaries + 5 sidecars + manifest + 4 installers
   [2/6] sidecar integrity
     ok cargo-cleanme-aarch64-apple-darwin
     ok cargo-cleanme-aarch64-unknown-linux-gnu
     ok cargo-cleanme-x86_64-apple-darwin
     ok cargo-cleanme-x86_64-pc-windows-msvc.exe
     ok cargo-cleanme-x86_64-unknown-linux-gnu
   [3/6] release-manifest.json agreement
     manifest binds source revision 270cceb2be31 to tag v0.2.10
     manifest agrees with the bytes for 5 asset(s)
   [4/6] contract agreement
     … 5 targets → asset names, sizes and digests
   [5/6] static Linux ABI evidence (not runtime)
     aarch64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
     x86_64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
   [6/6] real installer qualification on this host
     installed the real cargo-cleanme-x86_64-unknown-linux-gnu and it reports 'cargo-cleanme 0.2.10'
     the installed bytes match the release asset digest exactly
   PASSED: the staged draft matches the release contract
   ```

3. **The validator's summary was read by a human.** Exact tag `v0.2.10`;
   source SHA `270cceb2be31`; upstream run `39518072322`; inventory count
   15; measured glibc floors 2.17.0 on both Linux targets; installer result:
   the real binary installed and reported its version.
4. **The crate version and the release tag agreed**, and no public release
   existed for this version beforehand: `check-release-identity.py --tag
   v0.2.10` exit 0 ("v0.2.10 names 0.2.10 at 270cceb2be31"), and crates.io
   returned 404 for `0.2.10` before the upload.

### The independent local inspection

The draft's 15 assets were downloaded and **every** digest recomputed from
the bytes, then compared four ways: the local recomputation, the
`release-manifest.json`, each binary's `.sha256` sidecar, and GitHub's own
reported asset digest. Sizes were compared against the manifest. All fifteen
agreed, for all five binaries, and the inventory was exactly the contracted
set (15 of 15, no extra, none missing) — `armv7-unknown-linux-gnueabihf`
absent by contract. The inspector's own verdict was "INSPECTION PASSED: 15
assets, every digest agrees four ways".

The staged `cargo-cleanme-x86_64-unknown-linux-gnu` was also run directly:
`cargo-cleanme 0.2.10`, a stripped PIE ELF, with a measured glibc floor of
`GLIBC_2.17`.

## 4. Crate publication

`check-release-identity.py --tag v0.2.10` was run first ("v0.2.10 names 0.2.10
at 270cceb2be31", exit 0). `cargo publish --locked` uploaded and reported:

```text
Published cargo-cleanme v0.2.10 at registry `crates-io`
```

crates.io confirms version `0.2.10`, `yanked: false`, sha256
`0f2e471bcdf53b4c108b0ed0c1532fa30277b38bdb88854493f5b6da9248cd0b`. This is
the **eighteenth** published release and the **seventeenth** installable one;
`0.2.0` remains the only yanked release.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes, and each binary
digest was checked against its sidecar, against the release manifest, and
against GitHub's reported digest. All agree.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9263200 | `79594e01b390e8f6ab04d4b2d0ae111308000b94b4d311020b3c1d0c5c6d531b` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8171824 | `565da2b6ed25383aaad9a3caa73d2a348ff2e9ba74a4421b2b62a78e3002e44f` |
| `cargo-cleanme-x86_64-apple-darwin` | 11151440 | `44cc493ed28162d8a937b646b87842c0ac61cdac3537d732d0a4b87c77e98457` |
| `cargo-cleanme-aarch64-apple-darwin` | 10662704 | `8a793fc4bee2af87d21fcbbc0c2a5c5830bed2b67386d0ec7857fd4330b19fa9` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9720320 | `ec3b944faaa7b8d5c915bd9caef1c73f087a3973d37e8fca72c2376bfdfe3b50` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6900 | `57fccd879903ccbbea7f4a716a1cb37722aee73a201794440d71947342fdf8e7` |
| `install-exact.ps1` | 4710 | `7f1aaa07db36e0d368ae1c737490df82592d3fbea1b1344135b4f83cba5f25e6` |
| `release-manifest.json` | 1340 | binds `270cceb2be318ab6e2f7da87a90d5ce73f7f292a` to `v0.2.10` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are **byte-identical to v0.2.9's**
(`cc59833b…`, `e8268523…`) — the machine-checkable form of "installer
behaviour is unchanged by this release", which touched `src/` only for the
scope composition and the two selection/negative-result surfaces.
`install-exact.sh` / `install-exact.ps1` differ from v0.2.9's **only** in the
pinned release version in their download URLs; that is their designed
per-release change.

## 6. Public-release evidence

The release is **immutable** and carries a valid signed attestation:

```text
verify-release-attestation: v0.2.10 is immutable and carries a valid release attestation
  attested release subject: pkg:github/dbowm91/cargo-cleanme@v0.2.10 (15 asset digest(s))
```

Re-run against the downloaded bytes, the attestation and the bytes agree:

```text
verify-release-attestation: v0.2.10 is immutable and its 15 asset digest(s) match GitHub's signed attestation
```

## 7. The automatic published-release smoke

`post-release-smoke.yml` ran automatically on the `release: published` event as
run `39522277814`: transition resolution (`from_version=v0.2.9`,
`to_version=v0.2.10`) plus **all five target lanes green**, public releases
only. No lane was retried and none was skipped; per the project rule, a
skipped smoke and a passing smoke must never be confusable, and this one is
unambiguously the latter.

This is the **tenth** consecutive green automatic smoke. The two historical
failures (v0.2.0 `37507097075`, v0.2.1 `37507738147`) stay recorded as
failures in their own records.

## 8. External smoke, from outside the repository

Run from a scratch directory outside the repository, against the published
artifacts only.

**§8.1 — the first attempt failed, and the failure was in the harness.** The
first script resolved the binary under test with `command -v cargo-cleanme`.
On this host there is a **stale 0.2.9** at `~/.local/bin/cargo-cleanme`,
which precedes `~/.cargo/bin/cargo-cleanme` on `PATH`. The freshly installed
0.2.10 was therefore never executed: the smoke printed `cargo-cleanme 0.2.9`
under a heading that said 0.2.10 and still reported `ok`, because the check
asserted an **exit code** and never the version. Two further harness defects
were found in the same pass:

- The "exactly one JSON document" check computed `len(json.load(stdout))`,
  which on an object returns its **top-level key count** (6), not a document
  count. The check was vacuous in the wrong direction — it failed for a
  reason that had nothing to do with the claim.
- The `update --dry-run` check looked for the substring `already`, which the
  correct already-current message does contain ("already at the latest stable
  version") but which a *wrong* binary offering a newer version would also
  satisfy elsewhere in its output.

This is worth recording rather than quietly re-running: **the first external
smoke of this release tested the wrong binary and reported a failure that was
not a product defect.** The fix was to install into an isolated Cargo root,
invoke the binary by absolute path, assert the reported version, count JSON
documents by decoding one value and requiring only whitespace to remain, and
assert the already-current message and the version by name.

**§8.2 — the corrected smoke, and its result.** `EXTERNAL SMOKE PASSED`.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --version 0.2.10 --locked --root <isolated>` | resolved registry-only and installed `cargo-cleanme v0.2.10` |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.10` — **asserted by value, not by exit code** |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.10` — same assertion |
| `config show` in an isolated config home | prints the effective config; the first-use template was written; exit 0 |
| `config show --format json` | **refused**, exit `2`, `configuration error: config command does not support --format json; use human or log output`; stdout empty, so no config text leaked |
| `scan --format json --no-progress <absolute path>` | **exactly one** JSON document on stdout, a top-level object; exit 0; the scanned tree unchanged |
| `clean --full --dry-run` | **refused with exit 2**, naming one of the two documented reasons ("full reconciliation reported an incomplete scan"), on 1,232 filesystem diagnostics outside the scope — a correct outcome, and `--dry-run` spawned nothing |
| `update --dry-run` (Cargo-managed install) | `already at the latest stable version (0.2.10; published is 0.2.10)`, **exit 0** |
| `update` (no `--dry-run`, Cargo-managed install) | **refused**, exit 2, remediation exactly `cargo install cargo-cleanme --locked --force`; the managed bytes' digest **unchanged**; `cargo install --list --root` still reports `0.2.10` |
| The published **release asset** (downloaded from GitHub, digest-verified against its `.sha256` sidecar, placed outside any Cargo root) | digest matches the sidecar exactly; reports `cargo-cleanme 0.2.10`; `update --dry-run` reports already-current, exit 0 |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report
no groups while looking healthy.

The Cargo-managed refusal path **was** exercised here, which the v0.2.9
record could not do: 0.2.10 is the newest release, so `update --dry-run`
short-circuits on an empty plan, but the *mutating* `update` reaches the
provenance gate and refuses.

## 9. Completions, manpages, benchmark, size

The 13 generated artefacts (5 completions, 8 man pages) matched the clap model
at the release commit — `generate-docs --check` reported "13 artifacts match
the clap model", and the `generated-docs` CI job was green. The eight man
pages carry the new version.

## 10. Effect on closures

- **C040 — closed.** The corrective is carried by a published release, which
  was its only remaining condition. See
  `plans/closure/artifact-discovery-cleanup/c040-status.md`.
- **C039 — closed.** The compact selection surface shipped in v0.2.10. See
  `plans/closure/artifact-discovery-cleanup/c039-status.md`.
- **C038 — publication condition discharged.** The negative-result verdict
  shipped in v0.2.10. See
  `plans/closure/artifact-discovery-cleanup/c038-status.md`.
- **C036 — D1 discharged by C040.** The root `long_about` no longer claims a
  bare `cargo cleanme scan` is the full machine reconciliation, and the root
  man page now renders from that same `long_about` so the two surfaces
  cannot drift again. C036's remaining items stay open.
- **M017 — unchanged.** Its scoped-proof default shipped in v0.2.9; v0.2.10
  carries it again unchanged.
- **C033, C034 — unchanged.** Their retirement and safe-track qualification
  are separate tracks, not publication-blocked.

## 11. Known limitations and unresolved findings

- **One medium reliability finding, open: C041.** The release gate's
  `update::tests::a_staging_cleanup_failure_is_reported_not_swallowed` failed
  once with `spawning bounded candidate: Text file busy (os error 26)`
  (`ETXTBSY`) and passed on the second run of the same tree. It is **not a
  safety defect**: the transaction aborts before the commit, the error is
  reported, and nothing is deleted. It **is** reachable by a user, because
  the same path stages and executes a candidate during a real
  `cargo cleanme update`. Re-measured for this record: the test alone passed
  40 of 40 consecutive runs; an 8-way parallel full-lib-suite burst passed
  8 of 8. The mechanism is narrowed but **not proven**, and upstream
  `eggup-core` 0.1.3 was checked and does **not** carry the fix
  (`run_bounded` is byte-identical between 0.1.2 and 0.1.3). Recorded as
  `plans/implementation/artifact-discovery-cleanup/c041-update-candidate-spawn-etxtbsy-corrective.md`.
- **Medium (evidence shape, not a defect): the interactive terminal
  lifecycle.** The cursor is not restored on exit, and the repository still
  has no pty harness for the CLI-level proof of the rendered surface. Both
  are carried from C039 and remain open.
- **Low (coverage, not a defect):** the *supported* branch of the profile
  selector is still only executed against a real Cargo on a qualified host.
  The hosted `stable` is 1.99.1 and the `msrv` lane's 1.89.0 is the nearest
  qualified toolchain, so the qualified-host execution of the amended case is
  not in the hosted evidence. Recorded in
  `plans/closure/artifact-discovery-cleanup/c037-status.md` §8.
- **ADR 008 §4 and §5 — open owner decisions.** Deliberately not resolved by
  this release: no root-level `--full` was added, and the published
  `clean --full` spelling was not removed.
- **Debt carried forward:** v0.2.7 and v0.2.8 were published without a
  receipt. This release has one.

## 12. Disposition

**Closed.** The release was published on both channels — the GitHub release is
public and immutable with a valid attestation, and the crate is on crates.io
and not yanked — and every gate the project defines for a publication was
executed and recorded above. The publication conditions on C040, C039 and
C038 are discharged.

One finding is carried forward **open** rather than closed with the release:
C041, the update-path `ETXTBSY` flake. It was found by this release's own
gate, it is not a safety defect, and it was deliberately not fixed inside the
release.
