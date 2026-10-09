# cargo-cleanme v0.2.5 — release record

Status: **published and qualified.**

A *release* is not a plan and therefore has no plan number, so this record is
named `r025-status.md`, continuing the form `r024-status.md` introduced.
Everything else follows `plans/closure/README.md` and the post-release
obligations in `docs/RELEASING.md`.

**What this release is.** The publication of the C029–C031 safe-track
correctives, which were already closed before the tag. It carries no other
product change. `cargo-cleanme 0.2.5`:

- Full traversal errors without a child path are reported as bounded,
  unknown-location coverage loss instead of being attributed to the selected
  root; a partial Full inventory keeps positive manifests but does not publish
  a complete learned-state generation and exits 1 (C029);
- only Cargo-resolved workspaces in durable developer locations contribute new
  automatic Routine roots — recognized caches, Trash, temporary trees,
  `node_modules`, and transient worktrees stay visible to Full discovery but
  are omitted from Routine selection (C030);
- cleanup resolves discovered manifests before returning an incomplete
  discovery block, so reported workspace and cleanup-unit counts reflect work
  actually attempted, while discovery uncertainty still blocks the whole
  cleanup scope with no `cargo clean` spawn (C031 safe track).

ADR 004 (independent proof before partial-scope cleanup) remains proposed;
partial-scope Execute remains blocked. This release changes no CLI flag, no
JSON schema field, and no release-asset contract.

| Fact | Value |
|---|---|
| Tag | `v0.2.5` |
| Source revision | `c4ec29811c2ae68d7c608c2b8fd93adcda5db81c` |
| GitHub release id | `408326919` |
| Draft staged | 2026-10-09T20:39:04Z (Eggpack run completed) |
| Published at | 2026-10-09T20:40:48Z |
| Immutable | yes |
| crates.io version | `0.2.5`, not yanked |
| crates.io sha256 | `4466f06336d81a24d256c7321139f686098ecc37facd38a9e191ad9631da30b9` |
| Release content | `c4ec298` (version bump, CHANGELOG, regenerated man pages — one commit) |
| Automatic smoke | run `37988595432`, **green on all five targets**, `from_version=v0.2.4` → `to_version=v0.2.5` |

## 1. Order of operations, as performed

`docs/RELEASING.md` §"Order of operations" was followed.

| Step | Result |
|---|---|
| 1. Land release content, CI green | C029–C031 merged to `main` (`57e6585`); CI `37984239079` and drift `37984239124` green at that head |
| 2. Bump version, changelog, own commit | `c4ec298` (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` + 8 regenerated man pages — one commit, see §2) |
| 3. Complete local gate on a clean tree | `bash scripts/release-check.sh` (no tag arg) — **passed, exit 0**, first execution |
| 4. Tag, push, verify | `git tag v0.2.5` → `c4ec298`; `check-release-identity: v0.2.5 names 0.2.5 at c4ec29811c2a` |
| 5. Dispatch Eggpack at the tag | run `37986193662` — **green on all 20 jobs**, no retry needed |
| 6. Inspect the draft | 15 assets, every digest recomputed locally (§3) |
| 7. Publish the draft | 2026-10-09T20:40:48Z, immutable |
| 8. Publish the crate | from a detached worktree of the tag (§4) |
| 9. External smoke, from outside the repository | all checks passed (§7) |

No run failed and no job was retried at any stage: Eggpack, staged
validation, the automatic smoke, and the local gate were green on first
execution. That is recorded as the absence of a failure, not as evidence that
the gates are weak.

CI on the release commit itself (`c4ec298`): run `37985793257` green;
drift `37985793075` green.

## 2. The gate behaved differently this time, for a stated reason

The 0.2.3 and 0.2.4 releases each needed two commits — the version bump, then
a man-page regeneration after the gate caught the stale `.TH`/`.SH VERSION`
strings. This release folded the regeneration into the bump commit
(`cargo run --quiet --features dev-tools --bin generate-docs` run before
committing), so `release-check.sh` passed on its first execution with nothing
to catch. The single-commit shape is therefore not a shortcut around the
drift gate: `generate-docs -- --check` ran inside the gate and confirmed
`13 artifacts match the clap model` on the committed tree.

The gate ran without a tag argument (the tag did not exist yet), so the
identity step ran in self-test mode only. Tag identity was proven separately
after tagging (§1 step 4), before anything was dispatched.

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
- **Manifest** — binds `source_revision c4ec29811c2a…` to release `v0.2.5`
  and agrees with the bytes for all 5 assets (confirmed independently by the
  validator below).
- **Staged-release validation** `37988403248`, green: it re-proved the
  tag/source binding (exact tag `v0.2.5` derived from the trusted checkout,
  upstream run `37986193662` success) and ran inventory (15 assets), sidecar
  integrity, manifest agreement, contract agreement, static ABI evidence
  (`GLIBC_2.17.0` floors on both Linux targets), and real installer
  qualification against the staged bytes.
- **No separate staged-binary fixture rehearsal was run for the C029–C031
  content.** r024's record contains one (stale-root omission against the
  staged Linux binary); this release's fix-specific evidence on released
  bytes is the automatic smoke's real-asset rehearsal plus the external
  smoke in §7. That is a thinner content check than r024's, stated plainly
  so a future release does not cite this one as precedent for skipping it.

**Attestation** — `verify-release-attestation.py --tag v0.2.5`: "v0.2.5 is
immutable and carries a valid release attestation", subject
`pkg:github/dbowm91/cargo-cleanme@v0.2.5`, 15 asset digests. The trust root
is still GitHub's release and attestation infrastructure, exactly as
`docs/RELEASING.md` states it; this is not an independent maintainer-key
signature and not SLSA provenance.

## 4. Crate publication

Published from a detached worktree of the exact tag,
`/tmp/cargo-cleanme-publish`, per `docs/RELEASING.md`.
`check-release-identity.py` was re-run inside it first (`v0.2.5 names 0.2.5
at c4ec29811c2a`, exit 0). `cargo publish --locked` uploaded and reported:

```text
Published cargo-cleanme v0.2.5 at registry `crates-io`
```

crates.io confirms version `0.2.5`, `yanked: false`, sha256
`4466f06336d81a24d256c7321139f686098ecc37facd38a9e191ad9631da30b9`. This is the
**thirteenth** published release; `0.2.0` remains the only yanked one.

The worktree was removed after publication.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8975360 | `932327aaf784ad6e2acc63d4cf519c99b6d20672cbfbfbadfd3b442ca6e4fc9a` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7930392 | `0f6ec1dc28716742b67ad60fa452e419b26360b7067f0ebc88f71271af1fde2c` |
| `cargo-cleanme-x86_64-apple-darwin` | 10810632 | `bb0fc34c681aef353342dcf16f44e302cdf8dc2165abc5468b2e6f30f27a59d1` |
| `cargo-cleanme-aarch64-apple-darwin` | 10379584 | `dc93fe70526404979054dc7ea8793cd2871e2e6f2518f332687962e72351c81e` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9403904 | `7a5f7633243fbd3ab88d074b53803b892306d6f05258f47f98701833a9a5555d` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `5fda82710c94617fdeb4bca753734cfe23338457042d1de2d3b7483d9b30380a` |
| `install-exact.ps1` | 4709 | `ccb160cce94df34d5d455d78f62c7881cfae3a19666542192cc010899f1a56be` |
| `release-manifest.json` | 1339 | `4b9e6a9b7ba27474a9492ee605623c8fc516cb94c5d2647aa5a69017d805698c` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are byte-identical to v0.2.3's and v0.2.4's
(`cc59833b…`, `e8268523…`) — the machine-checkable form of "installer
behaviour is unchanged by this release", which touched `src/` only.
`install-exact.sh`/`install-exact.ps1` differ only in the pinned release
version in their download URLs (`…/download/v0.2.5/…`); that is their
designed per-release change.

## 6. Public-release evidence

`post-release-smoke.yml` ran automatically on the `release: published` event
as run `37988595432`: transition resolution plus **all five target lanes
green**, `from_version=v0.2.4` → `to_version=v0.2.5`, public releases only.
No lane was retried and none was skipped; per the project rule, a skipped
smoke and a passing smoke must never be confusable, and this one is
unambiguously the latter.

This is the sixth consecutive green automatic smoke (v0.2.0 `37507097075`
failed, v0.2.1 `37507738147` failed, v0.2.2 `37561575727`, v0.2.3
`37652257315`, v0.2.4 `37850582892`, v0.2.5 `37988595432` — the two failures
stay recorded as failures in their own records).

## 7. External smoke, from outside the repository

Run from `/tmp/ext-smoke`, with an isolated `CARGO_HOME`/`XDG_CONFIG_HOME`/
`XDG_DATA_HOME` so the operator's own installation is untouched, resolving
only registry dependencies, against the published artifacts.

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --version 0.2.5 --locked` | resolved registry-only, compiled 149 dependencies, installed |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.5` |
| `cargo cleanme --version` (via Cargo) | `cargo-cleanme 0.2.5` |
| `cargo cleanme config show` | prints the effective config; first-use bootstrap works |
| `cargo cleanme scan --format json --no-progress <absolute path>` | exactly one JSON document on stdout (`operation: scan`, `scope: explicit`); stderr carries no document |
| `cargo cleanme update --dry-run` | `already at the latest stable version (0.2.5; published is 0.2.5)` (exit 2, the documented nothing-to-do status) — this also proves the live crates.io version-authority path works from the published binary |
| Cargo-managed `cargo cleanme update` (`--version 0.2.5 --root …`) | **refused**, exit 2, naming `cargo install cargo-cleanme --locked --force` |
| bytes after the refused update | **unchanged** (sha256 identical before/after) |
| Cargo bookkeeping after the refusal | isolated root's `.crates.toml` still records `cargo-cleanme 0.2.5` |

The absolute-path form of the bounded scan was used deliberately: a relative
root resolved zero Cargo workspaces through 0.1.2 (C015), so it would report no
groups while looking healthy.

## 8. Completions, manpages, benchmark, size

- Generated artefacts match the clap model: `generate-docs: 13 artifacts match
  the clap model`, and `release-check.sh` ran the same check.
- Semantic counters: **no drift** (`release-benchmark.py --check` passed
  inside the gate; failures 0, unresolved ownership 0).
- Linux x86_64 binary 8968656 (0.2.4) → 8975360 bytes (**+6704**, +0.07%).
  Dependencies unchanged at 149 compiled for the registry install (r024
  recorded the same 149).

## 9. Known limitations and unresolved findings

No new defect was found in the published bytes. Carried forward unchanged:

1. **Independent signing / SLSA provenance is still not in place.** Unchanged
   since M011A; the trust root remains GitHub's infrastructure. An open
   trust-model decision requiring a plan.
2. **`0.2.0` remains yanked.** This release does not change that; thirteen
   releases are published, twelve not yanked.
3. **Windows environment-variable persistence remains unimplemented.** A
   first install on Windows still requires the user to add the directory to
   `PATH` themselves. Explicitly out of scope; a future plan if wanted.
4. **The automatic smoke exercises the updater, not the C029–C031 scope
   paths.** Fix-specific evidence on released bytes is §7 plus the hosted
   qualification recorded in the C029–C031 closure records. If a future
   release wants the public smoke to cover scope admission, that is a
   smoke-matrix change needing its own plan.
5. **ADR 004 is still proposed.** Partial-scope Execute remains blocked;
   C031's combined-universe fail-closed behaviour is retained in this
   release's bytes.

## 10. Effect on closures

None of C029, C030, or C031 was waiting on this release: all three were
closed before the tag, with hosted qualification recorded in their own
closure records. This record is therefore the publication receipt for
already-closed work, not a closing condition. No plan status changes. The
registry's published-state section is updated to thirteen releases with
v0.2.5 current.
