# cargo-cleanme v0.2.4 — release record

Status: **published and qualified.**

A *release* is not a plan and therefore has no plan number, so this record is
named `r024-status.md`, continuing the form `r023-status.md` introduced.
Everything else follows `plans/closure/README.md` and the post-release
obligations in `docs/RELEASING.md`.

**What this release is.** The publication of the C028 corrective, and the thing
C028 was *closing* waiting for. It carries no other product change.
`cargo-cleanme 0.2.4` fixes a scope-integrity defect: a deleted automatically
learned scan location (for example a removed worktree) aborted bare
`cargo cleanme` with `invalid scan root … No such file or directory`, and no
other root was processed. Automatically seeded and learned roots are now
admitted by provenance — positively absent (`ENOENT`) and positively
non-directory roots are omitted with a bounded diagnostic, while symlinks
(never followed, including broken links) and unreadable/indeterminate roots
block the run with a typed whole-scope report. Explicit roots keep strict fatal
semantics, every destructive candidate is still proven against one fresh
combined ownership universe with a late pre-spawn recheck, Routine omission
never rewrites learned state, and a complete certain Full reconciliation prunes
definitively nonexistent learned directories independent of retention.

| Fact | Value |
|---|---|
| Tag | `v0.2.4` |
| Source revision | `24ec57aba00eab421646ed4aa135c265289990a2` |
| GitHub release id | `407294986` |
| Draft staged | 2026-10-08T16:08:40Z |
| Published at | 2026-10-08T21:59:46Z |
| Immutable | yes |
| crates.io version | `0.2.4`, not yanked |
| crates.io sha256 | `61e97a6e13dcbf364e15513971c1a973558431dca63b99637ed032465c8c02f7` |
| Release content | `9a0736a` (version bump, CHANGELOG), `24ec57a` (regenerated man pages) |
| Automatic smoke | run `37850582892`, **green on all five targets**, `from_version=v0.2.3` → `to_version=v0.2.4` |

## 1. Order of operations, as performed

`docs/RELEASING.md` §"Order of operations" was followed. Two steps behaved as
the document says they should, and are recorded rather than narrated as
routine.

| Step | Result |
|---|---|
| 1. Land release content, CI green | C028 branch fast-forward-merged to `main` (`bd89fb1` → `6f8126b`); CI run `37805115776` — **green on all nine jobs** at the merge head |
| 2. Bump version, changelog, own commit | `9a0736a` (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` only — the exact 3-file shape of the 0.2.3 release commit) |
| 3. Complete local gate on a clean tree | `bash scripts/release-check.sh v0.2.4` — **passed, exit 0**, after the fix in §2 |
| 4. Tag, push, verify | `git tag -a v0.2.4` → `24ec57a`; `check-release-identity: v0.2.4 names 0.2.4 at 24ec57aba00e` |
| 5. Dispatch Eggpack at the tag | run `37847719817` — **green on all 20 jobs**, no retry needed |
| 6. Inspect the draft | 15 assets, every digest recomputed locally (§3) |
| 7. Publish the draft | 2026-10-08T21:59:46Z, immutable |
| 8. Publish the crate | from a detached worktree of the tag (§4) |
| 9. External smoke, from outside the repository | all checks passed (§7) |

No run failed and no job was retried at any stage: Eggpack, staged
validation, the automatic smoke, and both contract gates were green on first
execution. As in r023, that is recorded as the absence of a failure, not as
evidence that the gates are weak.

CI on the release commit itself (`24ec57a`): run `37806192700`, green on all
nine jobs.

## 2. The gate caught the version bump's consequence before the tag

`release-check.sh` failed on its first run twice over, both times as designed:

1. **Stale man pages.** The man pages embed the version in their `.TH` line
   and in `.SH VERSION`, so the bump made all 8 stale. `24ec57a` regenerated
   them with `generate-docs` — never hand-edited. The diff is version strings
   and nothing else. This is the third release in a row where the gate caught
   exactly this.
2. **Missing tag at the identity step.** The first invocation refused at
   `release identity` with `v0.2.4 is not publishable — tag v0.2.4 does not
   exist in this repository`. That is the documented gate → tag → verify order
   working as written (r023 §2 records the same refusal for v0.2.3), not a
   defect. The gate was re-run after the tag existed and passed in full,
   ending in `release-check: passed; no publication was performed`.

## 3. The review gate, performed before publication

The draft was inspected before publication, and the inspection did not rely on
counts or on CI having said so.

- **Inventory** — 15 assets: 5 contracted binaries, 5 `.sha256` sidecars,
  `release-manifest.json`, `install.sh`, `install.ps1`, `install-exact.sh`,
  `install-exact.ps1`. That matches the contract's "5 contracted targets, 11
  release assets, wrappers aligned".
- **Digests recomputed locally**, not read back from GitHub's API: all 5
  binaries were downloaded and `sha256sum`-ed; each matches its sidecar (§5)
  and the manifest entry.
- **Manifest** — binds `source_revision 24ec57aba00e…` to release `v0.2.4`
  and agrees with the bytes for all 5 assets (confirmed independently by the
  validator below).
- **The shipped binary actually carries the fix** — the one check specific to
  this release, and a question about *content*, not counts. The staged
  `cargo-cleanme-x86_64-unknown-linux-gnu` (reporting `cargo-cleanme 0.2.4`)
  was run bare against an isolated `HOME` whose learned state named a deleted
  project directory alongside a surviving inactive sibling, with real Cargo on
  `PATH`. Result: exit 0 (v0.2.3 fails this shape with `invalid scan root`,
  exit 2), stderr carries `cargo-cleanme: omitting unavailable learned root
  …`, the sibling's `target/` was cleaned through Cargo (8.78 MiB, 22 files),
  and the state file still names the stale root afterwards — Routine omission
  did not rewrite learned state, which is the designed Full-only pruning
  boundary. A release can satisfy every digest and inventory check and still
  ship the wrong behaviour; this is what rules that out.
- **Staged-release validation** `37850233649`, green: it re-proved the
  tag/source binding and ran inventory, sidecar integrity, manifest agreement,
  contract agreement, static ABI evidence, and real installer qualification
  against the staged bytes.

**Attestation** — `verify-release-attestation.py --tag v0.2.4`: "v0.2.4 is
immutable and carries a valid release attestation", subject
`pkg:github/dbowm91/cargo-cleanme@v0.2.4`, 15 asset digests; re-run with
`--assets-dir` against the full 15-asset download: "immutable and its 15 asset
digest(s) match GitHub's signed attestation". The trust root is still GitHub's
release and attestation infrastructure, exactly as `docs/RELEASING.md` states
it; this is not an independent maintainer-key signature and not SLSA
provenance. (One self-inflicted note: pointing `--assets-dir` at a scratch
directory containing non-asset files fails closed with `inventory_drift`, as
designed — the guard counts extra files as drift, not as ignorable clutter.)

## 4. Crate publication

Published from a detached worktree of the exact tag,
`/tmp/opencode/publish-0.2.4`, per `docs/RELEASING.md`.
`check-release-identity.py` was re-run inside it first (`v0.2.4 names 0.2.4 at
24ec57aba00e`, exit 0). `cargo publish --locked` uploaded and reported:

```text
Published cargo-cleanme v0.2.4 at registry `crates-io`
```

crates.io confirms version `0.2.4`, `yanked: false`, sha256
`61e97a6e13dcbf364e15513971c1a973558431dca63b99637ed032465c8c02f7`. This is the
**twelfth** published release; `0.2.0` remains the only yanked one.

The worktree was removed after publication. (Two stale `prunable` worktree
entries from earlier releases remain registered; they are not this release's
and were left alone.)

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes. The published
bytes were additionally re-downloaded after publication and compared byte for
byte against the inspected draft (`cmp`: identical), so "the bytes may have
moved" is answered by evidence, not by the immutability flag alone.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8968656 | `5644116ecf03fd164bdf03c0fad044466c39b486e5bb3d5c3d5fb947e14d23aa` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7928376 | `4d38ed5b95471305b0e0c8136d2b468107c92e1a6f2e46f06cf9fea15cb3c9ef` |
| `cargo-cleanme-x86_64-apple-darwin` | 10808928 | `1825232c0bbbf7e1d9466d2b695cd94ce1622ae50412d8b9eb686c88635e5790` |
| `cargo-cleanme-aarch64-apple-darwin` | 10381648 | `8a627d8727c141ac87ddf512776eaa54f05037e181f85e45ec4eab8f332f6909` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9387008 | `1701f608d83532a600223419eddf4d5c69e2d5e06269d8981a42b5eb1e457fb6` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `c9dce503a525f2830372b5716ed345b60f6b627b4990f7d1087e80aae43fc888` |
| `install-exact.ps1` | 4709 | `024322460fb2220f131393bb28e05ed231a4f6e9dede16c3dbc23abe952f856a` |
| `release-manifest.json` | 1339 | `914e22772160e5fc2fcab14312eb44edc4065002a152d6514e1e0c64ff70b76d` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are byte-identical to v0.2.3's
(`cc59833b…`, `e8268523…`) — the machine-checkable form of "installer
behaviour is unchanged by this release", which touched `src/` only.
`install-exact.sh`/`install-exact.ps1` differ because they pin the release
version in their download URLs (`…/download/v0.2.4/…`); that is their designed
per-release change, verified by grep rather than assumed from the digest.

## 6. Public-release evidence — the condition C028 was waiting on

`post-release-smoke.yml` ran automatically on the `release: published` event
as run `37850582892`: transition resolution plus **all five target lanes
green**, `from_version=v0.2.3` → `to_version=v0.2.4`, public releases only.
The Linux lane's log shows the rehearsal explicitly: published v0.2.3 digest
`0e770d21…`, published v0.2.4 digest `5644116e…`, "updated in place, digest
equals the published v0.2.4 asset", "`--dry-run` must report already-current:
already-current reported (exit 2, which is the documented 'nothing to do'
status)". No lane was retried and none was skipped; per the project rule, a
skipped smoke and a passing smoke must never be confusable, and this one is
unambiguously the latter.

This is the fifth consecutive green automatic smoke (v0.2.0 `37507097075`
failed, v0.2.1 `37507738147`, v0.2.2 `37561575727`, v0.2.3 `37652257315`,
v0.2.4 `37850582892` — the earlier failures stay recorded as failures in
their own records).

## 7. External smoke, from outside the repository

Run from `/tmp`, with an isolated `CARGO_HOME`/config/state home so the
operator's own installation is untouched, resolving only registry
dependencies, against the published artifacts.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --locked --root …` | resolved registry-only, compiled 149 dependencies, `cargo-cleanme v0.2.4` installed |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.4` |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.4` |
| `cargo cleanme config show` | prints the effective config; first-use bootstrap works |
| `cargo cleanme scan --format json --no-progress <absolute path>` | exactly one JSON document on stdout (`operation: scan`, `scope: explicit`); stderr carries no document |
| `cargo cleanme update --dry-run` | `already at the latest stable version (0.2.4; published is 0.2.4)` — no downgrade offered; this also proves the live crates.io version-authority path (descriptive User-Agent and all) works from the published binary |
| Cargo-managed `cargo cleanme update` (`--version 0.2.4 --root …`) | **refused**, exit 2, naming `cargo install cargo-cleanme --locked --force` |
| bytes after the refused update | **unchanged** (sha256 identical before/after) |
| `cargo install --list --root …` | still reports `cargo-cleanme v0.2.4` |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report no
groups while looking healthy.

## 8. Completions, manpages, benchmark, size

- Generated artefacts match the clap model: `generate-docs: 13 artifacts match
  the clap model`, and `release-check.sh` ran the same check.
- Semantic counters: **no drift** (failures 0→0, unresolved ownership 0→0).
- Release binary 12125624 → 12236672 bytes (**+111048**, +0.9%). Dependencies
  unchanged at 177. Unlike v0.2.3 (installer-only change), this delta is the
  C028 product code itself — provenance admission, the guarded orchestration,
  Full-generation freshness, and absence pruning. Wall-clock timings moved but
  are explicitly "trend only, never gated".

## 9. Known limitations and unresolved findings

No new defect was found in the published bytes. Carried forward unchanged:

1. **Independent signing / SLSA provenance is still not in place.** Unchanged
   since M011A; the trust root remains GitHub's infrastructure. An open
   trust-model decision requiring a plan.
2. **`0.2.0` remains yanked.** This release does not change that; twelve
   releases are published, ten not yanked.
3. **Windows environment-variable persistence remains unimplemented.** A
   first install on Windows still requires the user to add the directory to
   `PATH` themselves. Explicitly out of scope; a future plan if wanted.
4. **The automatic smoke exercises the updater, not the C028 fix.** The
   fix-specific evidence on released bytes is the staged-binary rehearsal in
   §3 (omission diagnostic, sibling cleaned through Cargo, exit 0, state
   untouched). If a future release wants the public smoke to cover scope
   admission, that is a smoke-matrix change needing its own plan.

## 10. Effect on the C028 closure

C028's stated closing conditions were hosted lanes, a release carrying the
fix, and installed-release smoke. All three are now evidence rather than
intent: hosted lanes green before release (CI `37801034285`/`37802097231` on
the branch, `37805115776` on `main` at the merge head, `37806192700` at the
release commit), `v0.2.4` carries exactly the C028 tree plus the version bump
and regenerated man pages, and the installed-release smoke is green on all
five targets (`37850582892`, v0.2.3 → v0.2.4). C028 is therefore **closed**;
its closure record (`plans/closure/artifact-discovery-cleanup/c028-status.md`)
is updated to point here.
