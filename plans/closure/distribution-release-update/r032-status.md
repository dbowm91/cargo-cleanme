# cargo-cleanme v0.2.13 — release record

Release record for the **twenty-first** published crate version. It carries
**no product milestone**: it exists because `v0.2.12` cannot have a GitHub
release, and the updater resolves the newest stable release, so every
`update` from an older version failed closed until a complete release
existed again.

## The 0.2.12 incident, briefly

`0.2.12` (tag `b7ab909`, 2026-11-06T18:44:38Z) was published to crates.io at
18:46:56Z with no GitHub release. Its Eggpack draft (15 assets, manifest
source `b7ab909`, every sidecar verified locally against the bytes) was
published prematurely at ~18:47 — the automatic smoke run `42273848060`
fired on the `release` event and was then cancelled — and reverted to
draft. Under the repository's immutable-release policy that publication
burned the tag: re-publishing the corrected draft fails with `HTTP 422:
tag_name was used by an immutable release`. The staged `v0.2.12` bytes were
good; they are unpublishable by design. The remedy, per `docs/RELEASING.md`
("a broken release cannot be repaired by replacing an asset — the remedy is
a new version"), is this release. The stale `v0.2.12` draft is left in
place as evidence and documented in `CHANGELOG.md`; it is not served.

| | |
|---|---|
| Version | 0.2.13 |
| Tag | `v0.2.13` at `3409bd3bfa0aac1f745c39f9151605d91ad398e1` |
| GitHub release | published 2026-11-06T23:07:25Z, immutable, 15 assets, release id `437149669` |
| crates.io | published 2026-11-06, sha256 `19e4c551d7884902b1bc7552afa1a39905f5d45ceea65a01d5faeffd25b1be45`, not yanked |
| Automatic smoke | `42309706425` — green on all five targets, §5 |
| Closed by this release | none (release-process corrective; product code unchanged since 0.2.12) |
| Release commit | `3409bd3` (version bump, changelog, regenerated man pages, smoke-runner fix below) |

Prior record: `r031` (v0.2.11). The v0.2.7 and v0.2.8 receipts are still
missing; that debt is recorded in the registry and is **not** reopened here.
No receipt is written for `v0.2.12`: there is no published release to
record, and this file is its record.

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The release content was already on `main` (`cf26c2f`, `ad90cf3`) | macos-latest Eggpack runner fix; rustls 0.23.45 → 0.23.46 |
| 2 | The version was bumped to 0.2.13 with changelog + regenerated artifacts | `3409bd3`: `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` (new entry plus a `0.2.12` errata note), `README.md` examples, 8 man pages, `AGENTS.md` / `INSTALLING.md` / architecture version prose |
| 3 | **The complete local gate** | `bash scripts/release-check.sh` → exit 0, "release-check: passed; no publication was performed" (`/tmp/gate2.log` on the release host). It caught one real defect first: `cf26c2f` moved the release runner to `macos-latest` without moving the smoke matrix, so `check-post-release-smoke-contract.py --self-test` refused the pair; fixed in the release commit, gate re-run green |
| 4 | The tag was created, pushed, and bound | `git tag v0.2.13` → `check-release-identity.py --tag v0.2.13` → "v0.2.13 names 0.2.13 at 3409bd3bfa0a", exit 0; `git push origin v0.2.13` |
| 5 | `main` was pushed **before** dispatch | First dispatch (`42307063552`) ran with `head_sha=cf26c2f` while the tag pointed at `3409bd3` — the exact mismatch shape that blocked `v0.2.12`'s validation. `git push origin main`, re-dispatch `42307577394` (`head_sha=3409bd3`, tag commit); the stale run cancelled via the workflow's `cancel-in-progress` |
| 6 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.13` → run `42307577394`, success |
| 7 | The staged draft was inspected locally from a detached tag worktree | 15 assets; `sha256sum -c *.sha256` 5/5 OK; `validate-staged-release.py --tag v0.2.13` → "PASSED: the staged draft matches the release contract" (installer leg: real binary reports `cargo-cleanme 0.2.13`, bytes match digest); identity with `--manifest` binds `v0.2.13` to `3409bd3` |
| 8 | The hosted staged-release validation was awaited | run `42309540961` — green for `head_sha=3409bd3` |
| 9 | The draft was published, with curated notes | `gh release edit v0.2.13 --notes-file … --draft=false` → published 2026-11-06T23:07:25Z, immutable, 15 assets |
| 10 | The crate was published from a **detached worktree of the tag** | `/tmp/cargo-cleanme-publish-013` at `3409bd3` → `cargo publish --locked` → "Published cargo-cleanme v0.2.13"; worktree removed afterwards |
| 11 | The attestation was verified, twice | `verify-release-attestation.py --tag v0.2.13` → immutable, valid, 15 digests; with `--assets-dir` → all 15 match GitHub's signed attestation |
| 12 | External smoke from outside the repository | `cargo install cargo-cleanme --locked` → 0.2.13; `update --dry-run` on 0.2.13 → already-current, exit 0 |
| 13 | Live updater rehearsal of the reported failure path | self-managed 0.2.10 (`~/.local/bin`) → `cargo-cleanme update` → "updated to 0.2.13", exit 0; live bytes `5791d9c8…` equal the staged manifest digest for `x86_64-unknown-linux-gnu` |

## 2. Why the first dispatch was wrong, and why the gate did not catch it

Step 5's stale dispatch is worth one paragraph because it is the same
defect shape as the `v0.2.12` incident one level down: the Eggpack run's
`head_sha` is fixed at dispatch from the branch, while the build itself
checks out `inputs.release_tag`. Dispatching before `main` carries the tag
commit stages bytes the validation workflow will refuse to bind — and the
local gate cannot see it, because it runs on the tree, not on the push
state. The observable that saves this is the run's `headSha` matching the
tag commit (§1 step 5); a dispatch whose `headSha` is not the tag commit
must be re-dispatched, and `cancel-in-progress` makes that safe.

## 3. Follow-ups (not blockers)

- The `v0.2.12` draft remains as evidence. If it ever confuses a future
  validation pairing, delete it — it can never be published.
- `docs/UPDATE.md` "Known defects by version" covers only the `0.1.x`
  updater defects; the `0.2.12` absent-sidecar shape is disclosed in
  `CHANGELOG.md` but not there. Home: corrective C036.
- The hand-maintained published-release counts (`AGENTS.md`,
  `docs/INSTALLING.md`, the release skill) drifted again around this
  incident and were corrected by hand again. Home: C036 D29 (the gate).
