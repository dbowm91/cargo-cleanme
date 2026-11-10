# cargo-cleanme v0.2.14 — release record

Release record for the **twenty-second** published crate version. It carries
four product changes since v0.2.13: C048 (idle shared and external artifact
directories), C047 (per-learned-root Full blocks), M022 (bounded output
claims), and M021 (cheaper scan resolution).

| | |
|---|---|
| Version | 0.2.14 |
| Tag | `v0.2.14` at `200d31c4def821d14d3ae3500f55e4d0f2de7002` |
| GitHub release | published 2026-11-10T19:29:14Z, immutable, 15 assets, release id `440812106` |
| crates.io | published 2026-11-10, sha256 `ee472f813c9be5ecc74fc79fca93c583c50914fc729a413facbaa497f897651e`, 476,485 bytes, not yanked |
| Automatic smoke | `42885742223` — green on all five targets |
| Eggpack candidate build | `42881836118` — success, `head_sha=200d31c` |
| Staged-release validation | `42885555757` — success for the same tag and source SHA |
| CI on the release commit | `42881829199` (tag push) and `42881827687` (main push) — all nine jobs green |
| Release commit | `200d31c` (version bump, changelog, module sizes and test census re-derived, regenerated man pages, release prose) |

Prior record: `r032` (v0.2.13). The v0.2.7 and v0.2.8 receipts are still
missing; that debt is recorded in the registry and is **not** reopened here.

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The release content was already on `main` | `ea08442` (Windows fixture backdating), `5217f82`, `b897228`, `a29b4a0`, `fb7c501`, `5ad76cf`, `6fdde56`, `c333501` |
| 2 | The version was bumped to 0.2.14 with changelog + regenerated artifacts | `200d31c`: `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `README.md` examples, 8 man pages, `AGENTS.md` / `INSTALLING.md` / architecture version prose |
| 3 | The module sizes and test census were re-derived from `src/` | `46,269` total, `20,689` production, `625` declared tests; **623 Linux / 569 Windows / 619 macOS**; Windows cross-compiled for `x86_64-pc-windows-gnu` and confirmed to carry exactly the derived 569 names and none of the 56 excluded ones |
| 4 | **The complete local gate** | `bash scripts/release-check.sh` → exit 0, "release-check: passed; no publication was performed". An earlier `bash scripts/release-check.sh v0.2.14` stopped at the identity step because the tag did not exist yet; the gate was re-run without the tag argument, as the script's own help describes |
| 5 | The tag was created, verified, and pushed | `git tag v0.2.14` → `check-release-identity.py --tag v0.2.14` → "v0.2.14 names 0.2.14 at 200d31c4def8", exit 0; `git push origin main` then `git push origin v0.2.14` |
| 6 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.14` → run `42881836118`, `headSha=200d31c`, all 20 jobs green |
| 7 | The staged draft was inspected locally | 15 assets; `sha256sum -c *.sha256` 5/5 OK; `validate-staged-release.py --tag v0.2.14` → "PASSED: the staged draft matches the release contract"; installer leg: the real `x86_64-unknown-linux-gnu` binary reports `cargo-cleanme 0.2.14` and its bytes match the asset digest; identity with `--manifest` binds `v0.2.14` to `200d31c` |
| 8 | The hosted staged-release validation was awaited | run `42885555757` — green, identical tag and source SHA |
| 9 | The draft was published, with curated notes | `gh release edit v0.2.14 --notes-file … --draft=false` → published 2026-11-10T19:29:14Z, immutable, 15 assets |
| 10 | The crate was published from a **detached worktree of the tag** | `/tmp/cargo-cleanme-publish-014` at `200d31c` → `cargo publish --locked` → "Published cargo-cleanme v0.2.14"; worktree removed afterwards |
| 11 | The attestation was verified, twice | `verify-release-attestation.py --tag v0.2.14` → immutable, valid, 15 digests; with `--assets-dir` → all 15 match GitHub's signed attestation |
| 12 | External smoke from outside the repository | `cargo install cargo-cleanme --version 0.2.14 --locked --root …`; both spellings report 0.2.14; config bootstrap works; `config show --format json` refuses with exit 2; one scan JSON document is emitted; `--full --dry-run` simulates and spawns nothing (exit 1 because four learned roots were skipped); `update --dry-run` reports already-current; `update` refuses the Cargo-managed install with exit 2 and names `cargo install … --force --root …`; `cargo install --list --root` still reports 0.2.14 |
| 13 | A live self-managed rehearsal ran on every contracted target | `post-release-smoke.yml` run `42885742223`: 0.2.13 → 0.2.14, final digest verified, dry run re-run, and a Cargo-managed refusal left the bytes unchanged |

## 2. Asset inventory

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-aarch64-apple-darwin` | 10,996,912 | `28ea1c399f75a34a6864771b9f6c24093485f80fcc3d96a5ac8dae287bf064ea` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8,404,712 | `b4c2b49f2ac5d6a201059f3e188b51d6e56ac6167f3b8e98a620b794b04e501d` |
| `cargo-cleanme-x86_64-apple-darwin` | 11,475,792 | `20346403c34d99394f48c3ca4169e712cff82195a4eac13db4ba7383a4d59b82` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 10,017,280 | `397da004e03d64c74965d7a2beb0e8372ef727aa2fc15dc39bc5227be00ca95f` |
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9,545,984 | `f27f0ab71745360a806b01e6f571ad811d6297ffa3256c83f1d36c5e4833fd73` |
| `cargo-cleanme-aarch64-apple-darwin.sha256` | 101 | `d2737c777069f6d733754244cef9651783ffd2be97dff697c5001c86f6b937ce` |
| `cargo-cleanme-aarch64-unknown-linux-gnu.sha256` | 106 | `79c90c93e2edbeaa6687695795f1ca14a178ce94539d4483f7e1cc3036d96e26` |
| `cargo-cleanme-x86_64-apple-darwin.sha256` | 100 | `72f80128ccb40a668e2ac036dda3bb7eafbf8ddbff1e25c7252bb5ae373057d3` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe.sha256` | 107 | `b779142ea85a719560a4555b540a4cd88d10940ac5ab34f4d5d7fcc8da9ccd12` |
| `cargo-cleanme-x86_64-unknown-linux-gnu.sha256` | 105 | `6bdec5f3e8326290008b32711ac6356f7081deab7a0140586c1eb098940385a1` |
| `install-exact.sh` | 6,901 | `1bb45418ed3722957b878ebe13344c7eb807ef29fc922df882ccc7e5141e02cd` |
| `install-exact.ps1` | 4,711 | `10726f064ad1e3b03a43a7977ca5328fc1ff55ef973ba4a6902784593a54b9c1` |
| `install.sh` | 24,808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13,046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `release-manifest.json` | 1,341 | `05977fe841696e22d5e5053b7a7fde793ba041f6d5e8c40a678b8212a2e2404e` |

Static Linux ABI evidence: both Linux binaries require at most
**GLIBC_2.17.0** (static ELF evidence, not a runtime result on an old
distribution).

## 3. Gate and benchmark evidence

`scripts/release-check.sh` passed with no semantic counter drift:

```text
manifests: 2 -> 2        workspaces: 2 -> 2       locate: 2 -> 2
metadata: 2 -> 2         groups_measured: 0 -> 0  reportable: 0 -> 0
bytes: 0 -> 0            failures: 0 -> 0          unresolved_ownership: 0 -> 0
release binary: 12,125,624 -> 12,980,208 bytes
normal dependencies: 177 -> 191
```

Wall-clock timings are trend-only and were recorded by the gate
(discovery 0.0006 s → 0.0006 s, locate 0.0262 s → 0.0279 s, metadata
0.0270 s → 0.0303 s, elapsed 0.0700 s → 0.0500 s). Completions and man pages:
13 artifacts match the clap model.

## 4. Hosted lanes

Both CI runs on the release commit are green, including `checks` on
ubuntu-latest, macos-latest, and windows-latest, the `msrv` lane on 1.89, the
installer lanes on all three platforms, the benchmark, and generated-docs.
This is the hosted evidence the C047/M022 closure records named as missing
before closure; it does not itself close them.

## 5. Unresolved findings and follow-ups

- **C047, C048, M021, M022 remain open.** They are published in v0.2.14 and
  their hosted lanes are green, but their closure records have not been
  updated with the post-release evidence. Publication is not closure.
- **M020's registry row is stale.** It says "Implemented; not published",
  while the quiet human surface shipped in v0.2.12. This predates this
  release and is left for the closure update; the home for a documentation
  defect remains C036.
- **The published-release count is still hand-maintained.** `AGENTS.md`,
  `docs/INSTALLING.md`, and the release skill were updated to 22 published /
  21 installable; no gate enforces the count. Home: C036 D29.
- **The `v0.2.12` draft remains as evidence.** It can never be published and
  is not served.
- The updater still verifies the `.sha256` sidecar rather than the GitHub
  release attestation; that is C018's open gap, unchanged by this release.
