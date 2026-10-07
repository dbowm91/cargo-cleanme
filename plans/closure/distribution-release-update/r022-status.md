# cargo-cleanme v0.2.2 — release record

Status: **published and qualified.**

A *release* is not a plan and therefore has no plan number, so this record is
named `r022-status.md` rather than `NNN-status.md`. That naming form is
introduced here; `plans/closure/README.md` records it. Everything else follows
`plans/closure/README.md` and the post-release obligations in
`docs/RELEASING.md`.

**What this release is.** The publication of the C024 corrective. It carries no
other product change. `cargo-cleanme 0.2.2` fixes a matching defect: a config
`exclude` naming a directory whose canonical path contains `[` or `]` did not
exclude it on Windows, because the pattern rewrite was a no-op on that platform.

| Fact | Value |
|---|---|
| Tag | `v0.2.2` |
| Source revision | `ddaf6f9fa37f28db39de1a38d269dac92123817a` |
| GitHub release id | `RE_kwDOU5PV0M4YKIbG` |
| Published at | 2026-10-07T02:21:14Z |
| Immutable | yes (`isImmutable=true`) |
| crates.io version | `0.2.2`, not yanked |
| crates.io sha256 | `acc488b5733a307408103d5207ac2c17b1a4b24099025617a2ea2dd780860050` |
| Release content | `9ba2154` (version bump, CHANGELOG, TROUBLESHOOTING), `ddaf6f9` (regenerated man pages) |

## 1. Order of operations, as performed

`docs/RELEASING.md` §"Order of operations" was followed, and two of its steps
behaved differently than written. Both are recorded rather than smoothed over.

| Step | Result |
|---|---|
| 1. Land release content, CI green | `9ba2154` + `ddaf6f9`; CI run `37558953249` — **one failure first**: `generated-docs` rejected the version bump because the man pages embed the version. Regenerated with `generate-docs` (never hand-edited) in `ddaf6f9`; CI re-run green |
| 2. Bump version, changelog, own commit | `9ba2154` |
| 3. Complete local gate on a clean tree | `bash scripts/release-check.sh` — **passed, exit 0** |
| 4. Tag and push | `git tag -a v0.2.2` → `ddaf6f9`; `check-release-identity: v0.2.2 names 0.2.2 at ddaf6f9fa37f` |
| 5. Dispatch Eggpack at the tag | run `37559299295` — **attempt 1 failed, attempt 2 passed** (§2) |
| 6. Inspect the draft | 15 assets, every digest recomputed locally (§3) |
| 7. Publish the draft | 2026-10-07 02:21:14Z, immutable |
| 8. Publish the crate | from a detached worktree of the tag (§4) |
| 9. External smoke, from outside the repository | all checks passed (§6) |

**Step 3 was run wrong the first time.** I passed `v0.2.2` to
`release-check.sh` before the tag existed, and it refused at the release
identity step: *"tag v0.2.2 does not exist in this repository"*. The documented
order is gate → tag → verify tag. Re-run untagged, it passed. Nothing was
skipped; one run of the script was wrong and was replaced.

## 2. The staging run failed once, and it was not the product

Eggpack run `37559299295`, attempt 1: **failure**. All five targets built
successfully; `qualify_build_x86_64_apple_darwin` failed in its *first* step —
downloading its own build artifact:

```text
Attempt 5 of 5 failed with error: Request timeout:
  /twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts
```

`stage` was skipped, so **no draft existed and nothing was public**. The
companion validator run `37560379580` failed for the same reason and is retained.

Attempt 2 re-ran only the failed job, so the four already-qualified binaries
kept their bytes and the fifth re-qualified the artifact that had been built in
attempt 1. `docs/RELEASING.md` forbids rebuilding after qualification; a re-run
of a *failed* job is not a rebuild, and no binary was rebuilt.

The evidence that this was infrastructure and not a defect: the failing step
never reached the binary, and the identical bytes qualified on attempt 2.

## 3. The review gate, performed before publication

The draft was inspected before it was published, and the inspection did not rely
on counts:

- **Inventory** — 15 assets: 5 contracted binaries, 5 `.sha256` sidecars,
  `release-manifest.json`, and 4 installer wrappers. That matches the contract's
  "5 contracted targets, 11 release assets, wrappers aligned".
- **Digests recomputed locally**, not read back from GitHub: all five binaries
  were downloaded and `sha256sum`-ed, and each matches both its sidecar and the
  manifest. GitHub's own served-byte `digest` field agrees with all five.
- **Manifest** — `source_revision: ddaf6f9fa37f28db39de1a38d269dac92123817a`,
  matching the tag target.
- **Staged Linux binary run directly**: `cargo-cleanme 0.2.2`.
- **Staged-release validation** `37561423879`, all six checks: inventory (15
  assets), sidecar integrity, `release-manifest.json` agreement, contract
  agreement, static Linux ABI evidence, and real installer qualification on the
  host. It re-proved the tag/source binding from a trusted checkout.
- **Release notes** — the draft carried Eggpack's placeholder
  ("Maintainer review and publication are required…"). Maintainer notes
  describing the defect, its direction, and the upgrade path were written and
  attached *before* publication, because the release is immutable afterwards.

## 4. Crate publication, and a guard that earned its place

`docs/RELEASING.md` requires publishing from a detached checkout **of the tag**,
and names the path `/tmp/cargo-cleanme-publish`. That path **already existed**,
holding the v0.2.0 release checkout at revision `95629ae`.

`check-release-identity.py` refused, and would have caught a real disaster:

```text
tag v0.2.2 declares 0.2.2 but Cargo.toml declares 0.2.0
tag v0.2.2 points at ddaf6f9fa37f, not the revision being released (95629ae28223)
```

`cargo publish` from that directory would have uploaded **0.2.0's bytes labelled
0.2.2**. A clean worktree was created at `/tmp/cargo-cleanme-publish-0.2.2`
instead. The stale 0.2.0 worktree was left in place — it was not created by this
release and removing it was not required to publish.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8933608 | `dbf58754d96a417aea4f976fe4bb4eac9166b1401e7a01db5b7987e4b3d7d0c6` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7893696 | `3780b90c17f7b3569dcd8b18b15d1c57b606824990749c8e7f1566ed9763f401` |
| `cargo-cleanme-x86_64-apple-darwin` | 10763568 | `34efd73a4011992b01bf4e16d03b859a342065127a5f7193feeaf62535978cdf` |
| `cargo-cleanme-aarch64-apple-darwin` | 10339904 | `79b319021fd109a8be2494f42d32bf4b105522f0e77289d06aa7db9df17305fa` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9347072 | `913566da528fd95cafa77c9eb5033caaba2f24c0832a84e5a0979a14443ba547` |
| 5 × `.sha256` sidecar | 100–107 each | match their binary |
| `release-manifest.json` | 1339 | `1c3377292b93022e3222200e530c7146e9ea30e8c0156e7d100fd2d2b84b33e6` |
| `install.sh` | 14557 | `5675992d09b2f7eb76d2e16713c1d1c342521f472f32ba4e9b27af18d561b1a3` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6894 | `c8183adbb90fac699f4ba135e6249f47550de70ecfa1dd1de63a690ac215e258` |
| `install-exact.ps1` | 4709 | `93374f5a9f7814c42c2e273af3a8ae5a8206d218c448a50c1fc0233618617378` |

Release builder: Rust **1.99.0**, pinned in `release/eggpack/pack.toml`, on all
five targets. MSRV remains **1.89**.

## 6. Verification actually run

| Check | Result |
|---|---|
| `bash scripts/release-check.sh` (untagged) | **passed, exit 0** — fmt, clippy, tests, doc tests, MSRV 1.89 check *and* test, semantic counters, completions/manpage drift, Eggpack workflow drift, release contract, installer contract, smoke resolver, staged-validation contract, selector qualification, attestation helper, installer fixtures, fixture portability, doc citations, release-candidate smoke, `cargo package --locked`, `cargo publish --locked --dry-run` |
| Package contents | 26 files, the intended subset, `config.toml` present (required — `src/config.rs` embeds it) |
| CI on the release commit | run `37558953249`, nine jobs `success` |
| Eggpack stage | run `37559299295` attempt 2, `success` |
| Staged-release validation | run `37561423879`, six checks, `success` |
| Attestation | `verify-release-attestation.py --tag v0.2.2 --assets-dir …` — **15 asset digests match GitHub's signed attestation**; immutability policy confirmed enabled |
| crates.io | `max_version: 0.2.2`, 10 versions, 0.2.2 not yanked, checksum as above; 0.2.0 still the only yanked version |
| Benchmark | no semantic counter drift; release binary 12125624 → **12190392 bytes** |

### External smoke, from outside the repository

Run from `/tmp/smoke-ext`, with no local path or git override in effect:

| Check | Result |
|---|---|
| `cargo install cargo-cleanme --locked` | resolved registry deps only; installed 0.2.2 (replacing an older cargo-installed 0.1.6) |
| `cargo-cleanme --version` | `cargo-cleanme 0.2.2` |
| `cargo cleanme config show` (isolated config home) | bootstrapped defaults |
| Bounded read-only scan, absolute path, `--format json --no-progress` | exit 0; **exactly one JSON document** on stdout, stderr empty |
| `update --dry-run` | `0.2.0 -> 0.2.2 available`, names the right asset, `install: not changed (--dry-run)` |
| Cargo-owned refusal | refused, **exit 2**, bytes **unchanged** (`657cb2d8…` before and after), `cargo install --list` still reports 0.2.2 |

**One environment caveat, recorded because it looks like a product bug and is
not.** This host has a self-managed `cargo-cleanme` **0.2.0** at
`/home/sugarwookie/.local/bin/`, which precedes `~/.cargo/bin` in `PATH`. So
`cargo cleanme --version` resolves *that* binary and prints 0.2.0, while the
cargo-installed binary prints 0.2.2. Cargo resolves `cargo-cleanme` from `PATH`,
so the two spellings can differ on a host with a shadowing install. That is a
host `PATH` condition; the 0.2.2 binary reports 0.2.2 under every spelling when
it is the one resolved.

### The automatic `release: published` smoke — green on all five lanes

Run **`37561575727`**, `success`, on the `release: published` event:

| Lane | Result |
|---|---|
| Resolve the exact transition | `from_version=v0.2.1` |
| `x86_64-unknown-linux-gnu` | PASSED (v0.2.1 → v0.2.2) |
| `aarch64-unknown-linux-gnu` | PASSED (v0.2.1 → v0.2.2) |
| `x86_64-apple-darwin` | PASSED (v0.2.1 → v0.2.2) |
| `aarch64-apple-darwin` | PASSED (v0.2.1 → v0.2.2) |
| `x86_64-pc-windows-msvc` | PASSED (v0.2.1 → v0.2.2) |

This is the **first green automatic smoke in the project's history**. Both
previous automatic runs failed: v0.2.0 (`37419183947`, three automation defects)
and v0.2.1 (`37507738147`, all five lanes, because the resolver chose the
yanked v0.2.0). The resolver fix is visible in the evidence rather than asserted:
the run records `from_version=v0.2.1`, the correct previous stable, not the
yanked 0.2.0.

## 7. What this release changes for a user

One matching defect, in the protective direction. An `exclude` naming a
bracketed directory now excludes it on Windows; previously the exclusion
silently did not apply, so a project the user had excluded could still be
cleaned. Nothing can newly *become* cleanable, and existing patterns are
byte-identical after the rewrite — no configuration needs changing.

`{` and `}` in a canonical directory name still have no escapable form and are
left as written; `docs/USAGE.md` documents that case.

## 8. Unresolved findings

| # | Finding | Class | Disposition |
|---|---|---|---|
| 1 | `docs/RELEASING.md` names `/tmp/cargo-cleanme-publish` as the publish worktree path, but that path was already occupied by the v0.2.0 release checkout | **Process defect; guard caught it** | The identity guard refused and prevented publishing 0.2.0's bytes as 0.2.2 (§4). The stale worktree is still there. The documented path should either be unique per release or the step should say "a path that does not already exist" |
| 2 | Eggpack's artifact handoff can fail on a GitHub artifact-API timeout, after five attempts, with no retry at the workflow level | **Infrastructure fragility** | Recovered by re-running the failed job (§2). `docs/RELEASING.md` says a failure "must never be worked around"; a re-run of a job that never inspected the bytes is not a workaround, and no binary was rebuilt |
| 3 | `gh release publish` does not exist in the `gh` available here; publishing a draft is `gh release edit --draft=false` | **Documentation gap** | Not recorded anywhere in this repository. Worth adding to `docs/RELEASING.md` |
| 4 | The `cargo cleanme` subcommand spelling can resolve a different binary than `cargo-cleanme` when a self-managed install shadows `PATH` | **Product observation, not a 0.2.2 defect** | Recorded in §6. It is inherent to Cargo's `PATH` resolution. No corrective opened |
| 5 | No separate M011A attestation job runs after publication on this event | **Observation** | The attestation was verified from the operator host (`verify-release-attestation.py`, 15 digests matched). `docs/RELEASING.md` describes the attestation verification as running post-publication; on this event it did not run hosted. Recorded, not repaired |

## 9. Effect on other plans

- **C024 is carried by this release.** Its closure record said the fix would
  reach users when the next release was cut. That has happened; C024 itself is
  unchanged, because its disposition does not depend on it.
- **C023's acceptance criterion 11 is now met.** It asked for a green automatic
  `release: published` five-lane smoke, and `37561575727` is one. C023 moves
  from *conditionally closed* to *closed*, recorded in its own §13B.
- **M011C's operational condition is met.** Its disposition required "a real
  `release: published` event" with a green automatic run, which is what
  `37561575727` is. M011C moves from *conditionally closed* to *closed*.
- **Nothing here unblocks C010**, which remains `proposed` and blocked on an
  upstream `eggup-curl` change.
- **No other plan is affected.** No defect was found by this release.

## 10. Disposition

**Closed.** The release is published, immutable, attested, and verified on both
registries; the local gate, the hosted gates, the review gate, the external
smoke and the automatic five-lane smoke all passed, and the two runs that failed
on the way are recorded above with their causes rather than omitted.

This record claims no closure of anything that was not observed. In particular it
does not claim per-platform *installer* evidence against the published assets on
macOS and Windows: the validator's real installer qualification ran on its host
(ubuntu), and the `installers` CI jobs exercise the repository's installer copies,
not the released bytes. That gap is finding #5's neighbour and is stated here
rather than papered over.