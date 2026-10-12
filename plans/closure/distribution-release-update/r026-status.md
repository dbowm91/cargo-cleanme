# cargo-cleanme v0.2.6 — release record

Status: **published and qualified, with two stated deviations (§2).**

A *release* is not a plan and therefore has no plan number, so this record is
named `r026-status.md`, continuing the form `r025-status.md`. Everything else
follows `plans/closure/README.md` and the post-release obligations in
`docs/RELEASING.md`.

**What this release is.** The publication of the bugs.md interrogation fixes
(config glob labels, update-variant messages, trailing-slash symlink guards,
blocked-scope row preservation, selector union parity, equal-missing identity,
recorded unresolved stages, stale-temp sweep) plus the C032–C034 diagnostics
and hygiene work already on `main` (bounded Cargo cause excerpts, Routine
hygiene narrowing, parallel resolution phases). `cargo-cleanme 0.2.6`:

- Symlink checks no longer miss the trailing-slash spelling across clean
  roots, scan roots, authorization roots, and declared output roots;
- cleanup change detection no longer falls back to a lexical compare, and two
  absent identities for the same logical output compare equal instead of
  unproven;
- proof skip codes are assigned at the proof site with stages recorded where
  the lookup fails, so message edits cannot change the machine contract;
- a mid-run block keeps completed unit rows in human output, and log
  `reclaimed_bytes` counts executed units only;
- `output_union_before_bytes` is populated under a selector as whole-output
  context (matching human output) while `before_bytes`/`selector_estimate_bytes`
  stay null;
- `update --dry-run` distinguishes already-current from newer-local builds,
  and both benign no-op states exit **0** (changed from 0.2.5's exit 2; see
  §7 and §9.4).

No CLI flag and no release-asset contract changed. The one JSON shape change
is the union field above, which aligns JSON with the already-shipped human
line; `plans/output-schema-v1.md` already describes the union as
whole-output context, never the selector's estimate.

| Fact | Value |
|---|---|
| Tag | `v0.2.6` |
| Source revision | `bfa976ff8ce3092f0ceb073d523f4f4d2ae147a0` |
| GitHub release id | `410964339` |
| Draft staged | 2026-10-12T20:14Z (Eggpack run completed) |
| Published at | 2026-10-12T20:17:34Z |
| Immutable | yes |
| crates.io version | `0.2.6`, not yanked |
| crates.io sha256 | `bcc4e42e870b9653f178fca4950e34082a953158048ecd8937c15c49f2d60100` |
| Release content | `bfa976f` (version bump, CHANGELOG, regenerated man pages — one commit) |
| Automatic smoke | run `38303253453`, **green on all five targets**, `from_version=v0.2.5` → `to_version=v0.2.6` |

## 1. Order of operations, as performed

`docs/RELEASING.md` §"Order of operations" was followed, except §2.2.

| Step | Result |
|---|---|
| 1. Land release content, CI green | bugs.md fixes and C032–C034 work merged to `main`; CI red on windows-latest only (§2.1), everything else green |
| 2. Bump version, changelog, own commit | `bfa976f` (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` + 8 regenerated man pages — one commit) |
| 3. Complete local gate on a clean tree | `bash scripts/release-check.sh` (no tag arg) — **passed, exit 0**, first execution |
| 4. Tag, push, verify | `git tag v0.2.6` → `bfa976f`; `check-release-identity: v0.2.6 names 0.2.6 at bfa976ff8ce3` |
| 5. Dispatch Eggpack at the tag | run `38300637475` — **success**, no retry needed |
| 6. Inspect the draft | 15 assets, every digest recomputed locally (§3, §5) |
| 7. Publish the draft | 2026-10-12T20:17:34Z, immutable, after explicit human review of the staged inventory |
| 8. Publish the crate | `cargo publish --locked` reported `Published cargo-cleanme v0.2.6 at registry crates-io` (§4, with deviation §2.2) |
| 9. External smoke, from outside the repository | all checks passed (§7) |

Eggpack, staged validation, the automatic smoke, and the local gate were
green on first execution with no retries. CI on the release commit was not
(§2.1); that is recorded as a failure with its fix, not folded into the
green runs.

## 2. Where this release deviated, and why each is stated

### 2.1. CI was red on windows-latest at the tag

`CI 38300320955` failed only on `checks (windows-latest)`:
`config_edit_uses_fake_editor_process_and_keeps_invalid_edits` asserts stderr
contains `"exit status: 17"`, but `ExitStatus`'s `Display` is `"exit code: 17"`
on Windows — the exact string in the failure log. Every Unix lane passed, so
the test was green everywhere its author ran it and red where the product
ships. The assertion (added in `d7bcd76`) was failing since that commit; the
release content did not cause it.

This is the defect class `scripts/check-fixture-portability.py` guards for
fixtures, but no guard covers platform-specific `std` renderings inside test
assertions. The fix (`d1d8da3`, on `main` after the tag) accepts either
spelling with a comment naming the platform difference — test-only, no
product byte affected, so the published artifacts are not invalidated by it.
A guard for platform-dependent assertions is follow-up work, not claimed
here.

### 2.2. The crate was published from `main`, not a detached worktree

`docs/RELEASING.md` requires publishing from a detached checkout of the tag.
The worktree was created (`/tmp/cargo-cleanme-publish` at `bfa976f`), the
identity check passed inside this checkout, but `cargo publish` ran in the
`main` checkout. The tree was clean at `bfa976f`, which is exactly what the
tag names (`bfa976ff8ce3`), so the published bytes are the tagged bytes —
the agreement the rule exists to guarantee holds. The letter of the rule did
not. The worktree was removed afterwards. Future releases must run the
publish command inside the worktree; this record is not precedent for
skipping it.

## 3. The review gate, performed before publication

The draft was inspected before publication, and the inspection did not rely on
counts or on CI having said so.

- **Inventory** — 15 assets: 5 contracted binaries, 5 `.sha256` sidecars,
  `release-manifest.json`, `install.sh`, `install.ps1`, `install-exact.sh`,
  `install-exact.ps1`. That matches the contract's "5 contracted targets, 11
  release assets, wrappers aligned".
- **Digests recomputed locally**, not read back from GitHub's API: all 5
  binaries were downloaded and `sha256sum`-ed; each matches its sidecar (§5).
- **Manifest** — `check-release-identity.py --tag v0.2.6 --manifest`
  confirms it binds `v0.2.6` to `bfa976ff8ce3` and agrees with the bytes.
- **Staged-release validation** `38302863906`, green: it re-proved the
  tag/source binding (exact tag `v0.2.6` derived from the trusted checkout,
  upstream run `38300637475` success) and ran inventory (15 assets), sidecar
  integrity, manifest agreement, contract agreement, static ABI evidence, and
  real installer qualification against the staged bytes.
- **No separate staged-binary fixture rehearsal was run for the interrogation
  fixes.** As in r025, fix-specific evidence on released bytes is the
  automatic smoke's real-asset rehearsal plus the external smoke in §7 —
  stated plainly so a future release does not cite this one as precedent for
  skipping it.

**Attestation** — `verify-release-attestation.py --tag v0.2.6`: "v0.2.6 is
immutable and carries a valid release attestation", subject
`pkg:github/dbowm91/cargo-cleanme@v0.2.6`, 15 asset digests. The trust root
is still GitHub's release and attestation infrastructure, exactly as
`docs/RELEASING.md` states it; this is not an independent maintainer-key
signature and not SLSA provenance.

## 4. Crate publication

`check-release-identity.py` was run first (`v0.2.6 names 0.2.6 at
bfa976ff8ce3`, exit 0). `cargo publish --locked` uploaded and reported:

```text
Published cargo-cleanme v0.2.6 at registry `crates-io`
```

crates.io confirms version `0.2.6`, `yanked: false`, sha256
`bcc4e42e870b9653f178fca4950e34082a953158048ecd8937c15c49f2d60100`. This is the
**fourteenth** published release; `0.2.0` remains the only yanked one. See
§2.2 for the worktree deviation.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes, and each
binary digest was checked against its sidecar (all five match).

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9089096 | `1f12198b886d15d1f5ba35ecc3f1a09d0bc2be6da13b3534f52ca4ff9ffc5019` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8021272 | `bd94c66de649019010055de191b96d5a79edf7e37f09a8159f2983621cf4e459` |
| `cargo-cleanme-x86_64-apple-darwin` | 10949432 | `d0b4ac182c59767d4d398fb5785d15fc3a680622069627803cebd02edad4b7f6` |
| `cargo-cleanme-aarch64-apple-darwin` | 10487776 | `5b767a9122fd4d0286c5eb33f8f8656d96dfa0017f424c33145c8f252bf500f2` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9554944 | `1b5f2237e070573eeeea6c4a33893129afe5e6bc2688bb521a00b2f8b85bbb98` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `53b5781e3bf73c80db7edee56a1f4d1079751b844aafabadb5be8e0856f80cc7` |
| `install-exact.ps1` | 4709 | `61b19fd6fad23e00f6c7a15aad8fe06681cc2c51680df1a1860e247d4588dcff` |
| `release-manifest.json` | 1339 | `3718f69e6f1ee2b19d5e3776d5dce071c661e2fce74fc2adb45e3a33ab89aaa2` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are byte-identical to v0.2.3's, v0.2.4's, and
v0.2.5's (`cc59833b…`, `e8268523…`) — the machine-checkable form of
"installer behaviour is unchanged by this release", which touched `src/`
only. `install-exact.sh`/`install-exact.ps1` differ only in the pinned
release version in their download URLs (`…/download/v0.2.6/…`); that is their
designed per-release change.

## 6. Public-release evidence

`post-release-smoke.yml` ran automatically on the `release: published` event
as run `38303253453`: transition resolution (`from_version=v0.2.5`,
`to_version=v0.2.6`, confirmed in the resolve job's log) plus **all five
target lanes green**, public releases only. No lane was retried and none was
skipped; per the project rule, a skipped smoke and a passing smoke must never
be confusable, and this one is unambiguously the latter.

This is the seventh consecutive green automatic smoke (v0.2.0 `37507097075`
failed, v0.2.1 `37507738147` failed, v0.2.2 `37561575727`, v0.2.3
`37652257315`, v0.2.4 `37850582892`, v0.2.5 `37988595432`, v0.2.6
`38303253453` — the two failures stay recorded as failures in their own
records).

## 7. External smoke, from outside the repository

Run from `/tmp/smoke026`, with an isolated `HOME`/`CARGO_HOME`/`RUSTUP_HOME`
(the `HOME` override hides rustup's toolchain directory, so `RUSTUP_HOME` was
pointed at the operator's real one) so the operator's own installation is
untouched, resolving only registry dependencies, against the published
artifacts.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --locked` | resolved registry-only and installed `cargo-cleanme v0.2.6` |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.6` |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.6` |
| `cargo cleanme config show` | prints the effective config; first-use bootstrap works in the isolated home |
| `cargo cleanme scan --format json --no-progress <absolute path>` | exactly one JSON document on stdout (`schema_version: 1`, `operation: scan`); exit 0 |
| `cargo cleanme update --dry-run` | `already at the latest stable version (0.2.6; published is 0.2.6)`, **exit 0** — this also proves the live crates.io version-authority path works from the published binary |
| Cargo-managed `cargo cleanme update` (`--version 0.2.6 --root …`) | **refused**, exit 2, naming `cargo install cargo-cleanme --locked --force` |
| bytes after the refused update | **unchanged** (sha256 `d7c828fa…` identical before/after) |
| Cargo bookkeeping after the refusal | isolated root's `.crates.toml` still records `cargo-cleanme 0.2.6` |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report no
groups while looking healthy.

The `update --dry-run` exit code changed from 0.2.5 (exit 2, as r025
recorded) to 0. This is the intended new behaviour, not a regression: the
benign no-op arms return `Ok(0)` with a comment stating schedulers must not
alert on it, and `docs/TROUBLESHOOTING.md` ("That is success") agrees. But
`architecture/13-orchestration.md` still documented exit 2 as "still wrong",
so the doc was corrected on `main` alongside this record; the `config edit`
exit-code row in the same table (0/1/2, editor status in prose) was corrected
at the same time.

## 8. Completions, manpages, benchmark, size

- Generated artefacts match the clap model: `generate-docs: 13 artifacts match
  the clap model`, and `release-check.sh` ran the same check.
- Semantic counters: **no drift** (`release-benchmark.py --check` passed
  inside the gate).
- Linux x86_64 binary 8975360 (0.2.5) → 9089096 bytes (**+113736**, +1.3%).

## 9. Known limitations and unresolved findings

No new defect was found in the published bytes. Carried forward unchanged,
plus two documentation findings from release prep:

1. **Independent signing / SLSA provenance is still not in place.** Unchanged
   since M011A; the trust root remains GitHub's infrastructure. An open
   trust-model decision requiring a plan.
2. **`0.2.0` remains yanked.** This release does not change that; fourteen
   releases are published, thirteen not yanked.
3. **Windows environment-variable persistence remains unimplemented.** A
   first install on Windows still requires the user to add the directory to
   `PATH` themselves. Explicitly out of scope; a future plan if wanted.
4. **The automatic smoke exercises the updater, not the interrogation-fix
   paths.** Fix-specific evidence on released bytes is §7 plus the hosted
   qualification in CI (Unix lanes) and the closure records of the originating
   fixes. If a future release wants the public smoke to cover scope admission,
   that is a smoke-matrix change needing its own plan.
5. **ADR 004 is still proposed.** Partial-scope Execute remains blocked; the
   combined-universe fail-closed behaviour is retained in this release's
   bytes.
6. **`update --dry-run` exit 0 is now the behaviour but was exit 2 in 0.2.5.**
   Intended (see §7), covered by the prose contract tests but not by an
   exit-code-pinning test; the deep-dive doc is corrected, the exit-code pin
   is not added here.
7. **The module line-count table in `architecture/overview.md` §3 is stale
   independent of this release.** Its per-module totals exceed `wc -l` even
   for files this release did not touch, so the counting method is not
   recoverable locally (`tokei` is not installed). Re-deriving it needs its
   own corrective with deep-dive realignment, not a release-record edit.

## 10. Effect on closures

C032 was conditionally closed with publication pending, and v0.2.6 carries
its implementation (bounded Cargo cause excerpts, Routine hygiene narrowing,
parallel resolution phases, recorded unresolved stages). The publication
condition is therefore met; the actual status change stays with the plans
process and is **not** made here. C033 and C034 remain active under their own
gates. No other plan waited on this release. The registry's published-state
section is updated to fourteen releases with v0.2.6 current.
