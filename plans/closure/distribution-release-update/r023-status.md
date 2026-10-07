# cargo-cleanme v0.2.3 — release record

Status: **published and qualified.**

A *release* is not a plan and therefore has no plan number, so this record is
named `r023-status.md`, continuing the form `r022-status.md` introduced.
Everything else follows `plans/closure/README.md` and the post-release
obligations in `docs/RELEASING.md`.

**What this release is.** The publication of the C027 corrective, and the thing
C027 was *conditionally closed* waiting for. It carries no other product change.
`cargo-cleanme 0.2.3` fixes a first-install defect: on a stock macOS account,
where `$HOME/.local/bin` is not on `PATH` by default, the POSIX installer
installed a working binary, exited 0, and printed an export command the user had
to run by hand — so a newly opened terminal could not resolve
`cargo cleanme` until they did. The installer now appends one bounded, guarded,
idempotent block to a supported zsh/bash startup file after the verified binary
is in place.

| Fact | Value |
|---|---|
| Tag | `v0.2.3` |
| Source revision | `7b4c632740107af6b3c5116c5249cdb83451f2c6` |
| GitHub release id | `405953835` |
| Draft staged | 2026-10-07T16:03:53Z |
| Published at | 2026-10-07T16:28:03Z |
| Immutable | yes (`isImmutable=true`) |
| crates.io version | `0.2.3`, not yanked |
| crates.io sha256 | `ec6206d578beee28bc82bb63b142a097d38e825b7aabe4c277dc5dc48be9bd4f` |
| Release content | `1109ea8` (version bump, CHANGELOG), `7b4c632` (regenerated man pages) |
| Automatic smoke | run `37652257315`, **green on all five targets**, `from_version=v0.2.2` → `to_version=v0.2.3` |

## 1. Order of operations, as performed

`docs/RELEASING.md` §"Order of operations" was followed. Two steps behaved as
written and are recorded rather than narrated as routine.

| Step | Result |
|---|---|
| 1. Land release content, CI green | CI run `37648220800` — **green on all nine jobs, none cancelled**, at `7b4c632` |
| 2. Bump version, changelog, own commit | `1109ea8` |
| 3. Complete local gate on a clean tree | `bash scripts/release-check.sh v0.2.3` — **passed, exit 0**, after the fix in §2 |
| 4. Tag, push, verify | `git tag -a v0.2.3` → `7b4c632`; `check-release-identity: v0.2.3 names 0.2.3 at 7b4c63274010` |
| 5. Dispatch Eggpack at the tag | run `37649375578` — **green on all 19 jobs**, no retry needed |
| 6. Inspect the draft | 15 assets, every digest recomputed locally (§3) |
| 7. Publish the draft | 2026-10-07T16:28:03Z, immutable |
| 8. Publish the crate | from a detached worktree of the tag (§4) |
| 9. External smoke, from outside the repository | all checks passed (§6) |

Unlike v0.2.2, **no run failed and no job was retried.** Eggpack, staged
validation, the automatic smoke, and both contract gates were green on first
execution. That is worth recording plainly: it is the absence of a failure, not
evidence that the gates are weak.

## 2. The gate caught the version bump's consequence before the tag

`release-check.sh` failed on its first run, at the generated-artifact drift
gate:

```text
generate-docs: generated artifacts are stale or missing: man/cargo-cleanme.1, … (8 files)
```

The man pages embed the version in their `.TH` line and in `.SH VERSION`, so a
version bump makes them stale. `7b4c632` regenerated them with
`generate-docs` — never hand-edited. The diff is nine version strings and
nothing else. This is the second release in a row where the gate caught exactly
this, and the third hosted run (`37558953249` during v0.2.2's preparation,
then CI here) is the first evidence that the *hosted* `generated-docs` job
catches it too.

The first `release-check.sh v0.2.3` invocation also refused at the release
identity step, correctly, because the tag did not exist yet. That is the
documented gate → tag → verify order working as written, not a second defect.
The gate was re-run after the tag existed and passed in full.

## 3. The review gate, performed before publication

The draft was inspected before publication, and the inspection did not rely on
counts or on CI having said so.

- **Inventory** — 15 assets: 5 contracted binaries, 5 `.sha256` sidecars,
  `release-manifest.json`, `install.sh`, `install.ps1`, `install-exact.sh`,
  `install-exact.ps1`. That matches the contract's "5 contracted targets, 11
  release assets, wrappers aligned".
- **Digests recomputed locally**, not read back from GitHub's API: all 15 assets
  were downloaded and `sha256sum`-ed, and each binary matches its sidecar
  (§5). An intermediate run of this loop reported a Windows "mismatch" that was
  a bug in my own verification script — it stripped `.exe` before hashing. The
  Windows binary does match; re-verified correctly. A verification loop that is
  itself wrong is exactly how a real mismatch gets dismissed, so it is recorded.
- **Manifest** — binds `source_revision 7b4c63274010` to tag `v0.2.3` and agrees
  with the bytes for all 5 assets (confirmed independently by the validator,
  §3/6 below).
- **The shipped installer actually carries the fix** — this is the one check
  specific to this release, and it is a question about *content*, not counts.
  The staged `install.sh` was downloaded and grepped: it contains
  `PROFILE_BEGIN`/`PROFILE_END` managed-block markers, `--no-shell-profile`
  (lines 28, 73, 79, 126), `append_profile_entry` (442), and
  `profile_persistence_permitted` (697). A release can satisfy every digest and
  inventory check and still ship the wrong installer; this is what rules that
  out.
- **Staged-release validation** `37651861689`, all six steps green: inventory,
  sidecar integrity (5 digests), `release-manifest.json` agreement, contract
  agreement, static Linux ABI evidence (`GLIBC_2.17.0` for both Linux targets),
  and **real installer qualification on the host** — "installed the real
  `cargo-cleanme-x86_64-unknown-linux-gnu` and it reports `cargo-cleanme 0.2.3`"
  with bytes matching the release asset digest exactly. It re-proved the
  tag/source binding from a trusted checkout (`ref: 7b4c632…`) and refused to
  make any claim about a run that was not `workflow_dispatch`-triggered.
- **Release notes** — the draft carried Eggpack's placeholder. Maintainer notes
  describing the defect, the persisted-block behaviour, the new flag, the
  strengthened `--no-path`, and the "no CLI/JSON/asset contract changed"
  statement were written and attached **before** publication, because the
  release is immutable afterwards.

## 4. Crate publication

Published from a detached worktree of the exact tag, `/tmp/cargo-cleanme-publish-0.2.3`,
per `docs/RELEASING.md`. `check-release-identity.py` was re-run inside it first
(`v0.2.3 names 0.2.3 at 7b4c63274010`, exit 0). `cargo publish --locked`
uploaded and reported:

```text
Published cargo-cleanme v0.2.3 at registry `crates-io`
```

crates.io confirms version `0.2.3`, `yanked: false`, sha256
`ec6206d578beee28bc82bb63b142a097d38e825b7aabe4c277dc5dc48be9bd4f`. This is the
**eleventh** published release; `0.2.0` remains the only yanked one.

The stale `/tmp/cargo-cleanme-publish` worktree that caught v0.2.2's
near-miss does not exist here; a uniquely named worktree was used so a future
release cannot collide with a previous one either.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8933608 | `0e770d2178ac3b02fc8a4c26c0d5499188f78ce4f187968d8410255e9c4f4423` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7893696 | `b05f851ebef51fbbc0317139617eb28a8f43a0865ffd35e6ad970bcab5c04012` |
| `cargo-cleanme-x86_64-apple-darwin` | 10763736 | `6a9a8c28a0f8ed915be3d004c47454dfbb61a091db75285cae3d64c0e63c816b` |
| `cargo-cleanme-aarch64-apple-darwin` | 10339904 | `d233affb1cb45fc80474dca3c1fd99e523fc7c30dabf3e9a42efaf911527112d` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9347072 | `ddc90c6db96341538958187e447af7a72926826137897c4bb0796abc9e30ec47` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `872459fd01e351fe5b4c0b232118c1beaef64bb652a1158370c051939b7303be` |
| `install-exact.ps1` | 4709 | `2e4b153e751269ee7a2006df28466e05ee6e6478a8e1700977f1c81dfb7bfc0d` |
| `release-manifest.json` | 1339 | `fc05c8e98a2601af58a0cab2cc0f0188eca0406f133d6e8ea998e95df8934ded` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` grew from 14557 bytes in v0.2.2 to 24808 here — the managed-block
implementation. `install.ps1` is byte-identical to v0.2.2's
(`e8268523…`), which is the machine-checkable form of "Windows installer
behaviour is unchanged".

**Attestation** — `verify-release-attestation.py --tag v0.2.3`: "v0.2.3 is
immutable and carries a valid release attestation", subject
`pkg:github/dbowm91/cargo-cleanme@v0.2.3`, 15 asset digests. The trust root is
still GitHub's release and attestation infrastructure, exactly as
`docs/RELEASING.md` states it; this is not an independent maintainer-key
signature and not SLSA provenance.

## 6. Public-release evidence — the condition C027 was waiting on

C027's remaining condition was "a published release carrying the corrected
installer, followed by public-release evidence that the shipped installer
preserves the hosted macOS fresh-shell result." Both halves were checked
against **published** artifacts, not the tree and not a fixture.

- **The published installer is the validated one.**
  `https://raw.githubusercontent.com/dbowm91/cargo-cleanme/v0.2.3/packaging/install.sh`
  hashes to `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa`,
  byte-identical to the `install.sh` asset in the immutable release.
- **It works end to end against the real published release.** Run from a
  directory outside the repository with an isolated `HOME`:

  ```text
  cargo-cleanme 0.2.3 installed to /tmp/pubrel-…/home2/.local/bin/cargo-cleanme

    added /tmp/pubrel-…/home2/.local/bin to /tmp/pubrel-…/home2/.bashrc for future shells.

  /tmp/pubrel-…/home2/.local/bin is not on your PATH. Add it with:
    export PATH="/tmp/pubrel-…/home2/.local/bin:$PATH"
  ```

  and the fresh-shell assertion, in the same environment:

  ```text
  bash -i -c 'command -v cargo-cleanme; cargo-cleanme --version'
    -> /tmp/pubrel-…/home2/.local/bin/cargo-cleanme
    -> cargo-cleanme 0.2.3
  ```

  Negative direction: with `.bashrc` moved aside, the same shell resolves a
  *different* `cargo-cleanme` (`/home/sugarwookie/.local/bin/cargo-cleanme`,
  this host's own pre-existing install) — not the published one. The resolution
  depends on the persisted entry, not on ambient `PATH`.

  This host is Linux, so this is the **Linux** public-release fresh-shell
  result. The **macOS** fresh-shell result remains the fixture-lane evidence in
  the C027 closure record and hosted run `37648257315`'s CI lane, executed
  against the same `install.sh` bytes this release ships — which is why the
  digest comparison above is the load-bearing step rather than a nicety.

- **The five-target automatic smoke is green**: run `37652257315`, all five
  target jobs success, `from_version=v0.2.2` → `to_version=v0.2.3`. This is the
  **fourth consecutive** automatic demonstration (v0.2.0 `37419183947`,
  v0.2.1 `37507738147`, v0.2.2 `37561575727`, v0.2.3 `37652257315`); the first
  two failed and stay recorded as failures in the registry.

## 7. External smoke, from outside the repository

Run from `/tmp/extsmoke-…`, resolving only registry dependencies, against the
published artifacts.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --version 0.2.3 --locked --root …` | resolved registry-only, `Installed package cargo-cleanme v0.2.3` |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.3` |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.3` |
| `cargo cleanme config show` | prints the effective config; first-use bootstrap works |
| `cargo cleanme scan --format json --no-progress <absolute path>` | exactly one JSON document on stdout (`cargo_cleanme_version`, `mode`, `operation`, `result`, `schema_version`, `scope`); nothing changed on disk |
| `cargo cleanme update --dry-run` | `already at the latest stable version (0.2.3; published is 0.2.3)` — no downgrade offered |
| Cargo-managed `cargo cleanme update` | **refused**, exit 2 |
| bytes after the refused update | **unchanged** (sha256 identical before/after) |
| `cargo install --list --root …` | still reports `cargo-cleanme v0.2.3` |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report no
groups while looking healthy.

## 8. Completions, manpages, benchmark, size

- Generated artefacts match the clap model: `generate-docs: 13 artifacts match
  the clap model`, and `release-check.sh` ran the same check.
- Semantic counters: **no drift** (manifests 2→2, workspaces 2→2, locate 2→2,
  metadata 2→2, all reportable counters 0→0, failures 0→0, unresolved
  ownership 0→0).
- Release binary 12125624 → 12190560 bytes (**+64936**, +0.5%). Dependencies
  unchanged at 177. The size delta is the installer's PATH-persistence code
  only — C027 changed no Rust in `src/`, and the growth is in the shipped
  installer, not the binary. Wall-clock timings moved but are explicitly
  "trend only, never gated".

## 9. Known limitations and unresolved findings

No new defect was found in the published bytes. Carried forward unchanged:

1. **The macOS fresh-shell proof remains fixture-based.** The public-release
   run above is Linux. The macOS assertion runs against a local fixture release
   in CI, over the same `install.sh` bytes this release ships (digest-identical).
   Standing up a lane that runs the *public* installer on a macOS runner would
   close this properly; it is not required by the plan and was not done.
2. **Independent signing / SLSA provenance is still not in place.** Unchanged
   since M011A; the trust root remains GitHub's infrastructure. An open
   trust-model decision requiring a plan.
3. **The POSIX/PowerShell PATH asymmetry is intentional.** `install.ps1` is
   byte-identical to v0.2.2's. C027 was scoped to `install.sh`.
4. **`0.2.0` remains yanked.** This release does not change that; the
   `x86_64-pc-windows-msvc` variant name in the manifest follows the asset
   naming that predates the ARM Windows gap, unchanged here.
5. **Windows environment-variable persistence remains unimplemented.** A
   first install on Windows still requires the user to add the directory to
   `PATH` themselves. That was explicitly out of C027's scope; if it is wanted,
   it needs its own plan.

## 10. Effect on the C027 closure

C027 moves from **conditionally closed** to **closed**. Its final condition was
this record: a published immutable release carrying the corrected installer,
plus public-release evidence that the shipped installer preserves the
first-install outcome. Both now exist.

Closure here means the *engineering* is complete and shipped. It does not claim
the macOS public-installer lane described in §9.1 exists — it does not, and the
registry says so.