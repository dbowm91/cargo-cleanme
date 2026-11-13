# cargo-cleanme v0.2.15 — release record

Release record for the **twenty-third** published crate version. It carries
the scan / `--full` split (bare `scan` learns the machine; `--full` cleans
learned roots only) and markerless proven-target cleanup.

| | |
|---|---|
| Version | 0.2.15 |
| Tag | `v0.2.15` at `467377c10a4dcecbea2a51c9b48b318fbb0307b9` |
| GitHub release | published 2026-11-13T19:26:14Z, immutable, 15 assets, release id `444643300` |
| crates.io | published 2026-11-13T19:46:04Z, sha256 `e01cddd93f45d9c8544d0ad8f93d208a8cc083e3e69615d8f9fbe7a791d416be`, 479,233 bytes, not yanked |
| Automatic smoke | `43492187343` — **failed**: crates.io did not advertise 0.2.15 within 900s |
| Manual smoke re-run | `43495428163` — green on all five targets (0.2.14 → 0.2.15) |
| Eggpack candidate build | `43487639554` — success, `head_sha=467377c` |
| Staged-release validation | `43491618684` — success, `PASSED: the staged draft matches the release contract` |
| CI on the release commit | `43487617048` / `43487614818` (main) and tag-push lanes — green |
| Release commit | `467377c` (version bump, changelog, user-doc alignment for the scan CLI, regenerated man pages) |

Prior record: `r033` (v0.2.14).

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The release content was already on `main` | PR #18 merge `1051fa6` |
| 2 | The version was bumped to 0.2.15 with changelog + user-doc polish | `467377c` |
| 3 | **The complete local gate** | `bash scripts/release-check.sh` → exit 0, "release-check: passed; no publication was performed" |
| 4 | The tag was created, verified, and pushed | `git tag -a v0.2.15` → `check-release-identity.py --tag v0.2.15` → "v0.2.15 names 0.2.15 at 467377c10a4d"; `git push origin main` then `git push origin v0.2.15` |
| 5 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.15` → run `43487639554` |
| 6 | The staged draft was inspected locally | 15 assets; `sha256sum -c *.sha256` 5/5 OK; identity with `--manifest` binds `v0.2.15` to `467377c`; linux-gnu binary reports `cargo-cleanme 0.2.15` |
| 7 | The hosted staged-release validation was awaited | run `43491618684` — green |
| 8 | The draft was published, with curated notes | `gh release edit v0.2.15 --notes-file … --draft=false` → published 2026-11-13T19:26:14Z, immutable, 15 assets |
| 9 | The crate was published | crates.io carries 0.2.15 (sha256 `e01cddd9…`, published_by `dbowm91`); a later `cargo publish --locked` from a detached worktree of the tag refused with "already exists" |
| 10 | The attestation was verified, twice | `verify-release-attestation.py --tag v0.2.15` → immutable, valid; with `--assets-dir` → all 15 digests match |
| 11 | Automatic post-release smoke | run `43492187343` failed at the crates.io wait (900s); recorded, not erased |
| 12 | Manual post-release smoke re-run | `gh workflow run post-release-smoke.yml -f from_version=0.2.14 -f to_version=0.2.15` → run `43495428163`, green on all five targets |

## 2. Asset inventory

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-aarch64-apple-darwin` | 11,022,032 | `fb25d337f43b1d6944c7d902e91645e11a749ab5709c872dc4596e484a84e766` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8,415,848 | `dfd59524276ddb4530c28eea054a98f461a28b3336487452ad25469a401e339f` |
| `cargo-cleanme-x86_64-apple-darwin` | 11,500,536 | `6e8ccedf301454a73f72bfa51d42f9c964cc799e1deac411c8b5eca2103d6c2f` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 10,052,096 | `5963882a9c2ae76eb4e8b2aa0d34bb9c24fdd4b93f84802ed4e6b73118a7ef71` |
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9,558,896 | `291594a9a063af3773dafea8b5358420a3b5c2909996d31f58a29df42554be68` |
| `cargo-cleanme-aarch64-apple-darwin.sha256` | 101 | `c55fe96b3c741cf7769e0030ea245e4151d03fff151834c1c1ce9686eac5d045` |
| `cargo-cleanme-aarch64-unknown-linux-gnu.sha256` | 106 | `f296aac217ac149ce480295e2fcd4adf940b9f87d91feae2856cd1d9f1e51975` |
| `cargo-cleanme-x86_64-apple-darwin.sha256` | 100 | `1a295e673792a99ee1394ad4b4e30b372fedf694bed71b6a7581abb7fef183e2` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe.sha256` | 107 | `807f47211c3d816af78c49f419704a92a12e1273ec763093991d05f3570567a9` |
| `cargo-cleanme-x86_64-unknown-linux-gnu.sha256` | 105 | `36d45eee9aa5ed56096ad7a408b0d03ce254e4a615839394183f71d331d0575b` |
| `install-exact.ps1` | 4,711 | `1460e467b4954163029efeab5030478227a4ad6d666830eeaa0f14a75e9baaef` |
| `install-exact.sh` | 6,901 | `d2facef37614661d63e24d909186be80aadaf1354b41c20d9026fff28f01d539` |
| `install.ps1` | 13,046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install.sh` | 24,808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `release-manifest.json` | 1,341 | `adb4edc1f164cc9bc25de143564d2a7b0c8c8195450f4865b42c191354df4526` |

## 3. What this release carries

- Bare `cargo cleanme scan` is the machine-wide learn/expire walk; `scan ROOT`
  updates the global learned list for that directory only; `scan --all` /
  `--full` / `--known` are rejected.
- `cargo cleanme --full` cleans learned roots only (no platform walk), with a
  cold-start nudge toward `scan` when none are known.
- A proven `target/` without `CACHEDIR.TAG` is cleaned via `CARGO_TARGET_DIR`
  rather than deferred on Cargo's `--target-dir` marker check.

## 4. Open after publication

Publication is not closure for plans that still need post-release evidence
beyond this record. The failed automatic smoke run remains part of the
receipt; the manual re-run is recovery evidence, not an erasure of the first
failure.
