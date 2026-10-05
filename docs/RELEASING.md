# Releasing cargo-cleanme

Operator checklist for a release. Everything here is a precondition or an
explicit, manual action — no step in this document is automated away.

Read this before changing anything under `release/eggpack/`. Those files are
producer authority: the release workflow, the asset names, the installer
targets, and the updater's compiled-in target table are all projections of
them, and the drift gate fails the build if they disagree.

## Order of operations

The order matters. Publication is last because it is the only irreversible step.

1. Land the release content on `main` and get CI green.
2. Bump `version` in `Cargo.toml`, update `CHANGELOG.md`, and land that as its
   own commit. The version in the tree must equal the version you will publish.
3. Run the complete local gate (below). It must be green on a clean tree.
4. Tag the release commit and push the tag.
5. Dispatch `Release drift guard` / `Eggpack candidate builds`
   (`.github/workflows/release-binaries.yml`) against that exact tag.
6. Inspect the **draft** release Eggpack staged. Verify the inventory and every
   asset digest. This is a human review gate.
7. Publish the draft.
8. Publish the crate to crates.io.
9. Run the external smoke below, from outside the repository.

## The complete local gate

```sh
rustup toolchain install 1.89
cargo install --git https://github.com/eggstack/eggpack --rev <revision> --locked eggpack-cli
scripts/release-check.sh
```

`release-check.sh` runs, in order: `cargo fmt --check`, clippy with
`-D warnings`, the full test suite, doc tests, the Rust 1.89 check and test,
the semantic-counter benchmark, the completions/manpage drift gate, the derived
workflow-shape check, the Eggpack `ci check` workflow drift check, the release
contract check, the installer-contract check, the installer fixture suite,
the fixture-portability and architecture-citation guards, `cargo package
--locked`, and `cargo publish --locked --dry-run`.

Each static guard runs its own `--self-test` first. That ordering is
deliberate: a guard that has quietly stopped detecting its defect shape is
worse than no guard, so the gate proves each one can still fail before trusting
it to pass.

The Eggpack revision is pinned in `release/eggpack/github-policy.json`. Install
that exact revision; a different one may render a different workflow.

If `git status` is dirty, `release-check.sh` refuses to run. A release must be
built from a committed tree.

## What Eggpack does and does not do

`release-binaries.yml` is generated, and it stages a **draft**. It never
publishes, never clobbers an existing release, and never requests an
`id-token`. It refuses to build from a tag whose commit does not match the
dispatched revision, and it re-verifies every asset's size and SHA-256 against
the resolved plan.

Publishing the draft is a separate, manual, human-reviewed action. That is
deliberate: a compromised CI token must not be enough to publish a release.

## Publication gate: both workflows must be green

Publication requires **all four** of these, in order:

1. **Eggpack candidate build/stage green.** `release-binaries.yml`, generated
   by Eggpack. Draft staged, nothing published.
2. **Staged-release validation green for the same tag and source SHA.**
   `validate-staged-release.yml` subscribes to the Eggpack run's completion,
   checks out the upstream `head_sha`, re-proves the tag/source binding with
   `check-release-identity.py`, and runs `validate-staged-release.py` against
   the actual staged bytes.
3. **Human inspection** of the validator's summary: exact tag, source SHA,
   upstream run id, inventory count, measured glibc floors, installer result.
4. **Then publish the draft.**

**The validation workflow does not publish.** It holds `contents: read` and
nothing else, and contains no publication command. A compromised or buggy
validation job must never hold publication authority — and equally, its failure
must never be worked around by a job that can publish.

**There is no bypass.** If the validation workflow runs late, or GitHub has an
outage, the release waits. Do not publish on the grounds that validation was
"probably fine" or "already passed earlier": a rerun after the draft bytes change
must re-download and re-validate everything, because cached prior success is not
evidence about bytes that may have moved.

Once a release is public and immutable, staged validation is no longer
pre-publication evidence. Use the attestation verification
(`scripts/verify-release-attestation.py`) and the published-release smoke
(`post-release-smoke.yml`) instead.

## Release authenticity: immutable releases and attestations

Immutable releases are **enabled** for `dbowm91/cargo-cleanme` (repository
policy, not owner-enforced; enabled 2026-10-05). From the first release
published after that point:

- The published release's assets and its tag cannot be replaced. That also means
  a broken release **cannot be repaired by replacing an asset** — the remedy is
  a new version. Decide before staging, not after.
- GitHub issues a signed attestation binding the release to its repository, tag,
  commit, and every asset digest.

**State the trust root exactly.** The root is still GitHub's release and
attestation infrastructure. This is **not** an independent maintainer-key
signature, and it is **not** SLSA build provenance. It upgrades
*authenticity* from "HTTPS to a host that also serves the checksum" to "a
signature from GitHub over the artifact digests", and it is deliberately
scoped no wider. A trust claim that overstates its own root is worse than none.

The `.sha256` sidecar is now a **corruption and mis-download** check rather than
the authenticity boundary, and the sidecar check is retained because
`src/update.rs` performs it as the product's fail-closed integrity requirement
and the updater is not yet attestation-aware. See C018 for that gap.

Verify before you install or recommend a download:

```sh
python3 scripts/verify-release-attestation.py --settings-only
python3 scripts/verify-release-attestation.py --tag v0.1.7
python3 scripts/verify-release-attestation.py --tag v0.1.7 --assets-dir ./downloads
```

The helper fails closed on every non-pass state — policy off, release mutable,
attestation missing or unparseable, attestation for another repository or tag,
an asset the attestation does not name, local bytes that do not match the
attested digest, an inventory that is not the contracted set, and a missing or
too-old `gh` client. **Releases v0.1.0 through v0.1.6 predate the policy and
remain mutable. Do not recreate them, and do not describe them as attested.**

## After you publish: the automatic published-release smoke

Once the GitHub release **and** the crates.io version both exist,
`post-release-smoke.yml` runs automatically on the `release: published` event
across all five contracted targets. It performs a real self-managed update from
the previous stable release to the new one, verifies the final digest, re-runs
the dry run, and proves Cargo-managed refusal leaves bytes unchanged.

Two behaviours are deliberate and should not be "fixed":

- **It waits for crates.io.** Publication is ordered GitHub-release-first and
  `cargo publish`-second, so the event can fire before the version the updater
  resolves actually exists. The wait is bounded; **exhausting it is a failure,
  not a skip.** A closure record that shows a skipped smoke and one that shows a
  passing smoke must never be confusable.
- **It does not fail fast.** One platform's network or runner failure lets the
  other four produce evidence.

This path found C016 and C017 while every fixture suite was green. If it finds a
defect in immutable published bytes, follow the patch-release corrective process
in `docs/TROUBLESHOOTING.md`. **Never replace the published asset** — the
release is immutable, and the remedy is a new version.

Release closure records the smoke run id and all five target results. A manual
re-run may prove transient infrastructure recovery, but it does not erase the
failed original run: the closure record includes both.

The attestation verification from M011A runs post-publication too; its failure
stays separately visible and does not merge into the smoke's result.



| Item | Count | Source |
|---|---|---|
| release binary per contracted target | 5 | `release/eggpack/distribution.toml` |
| `.sha256` sidecar per binary | 5 | the contract's checksum template |
| `release-manifest.json` | 1 | Eggpack |
| `install-exact.sh` / `install-exact.ps1` | 2 | Eggpack bootstrap, from `packaging/` |
| release notes | 1 | maintainer |

The five targets are `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
and `x86_64-pc-windows-msvc`.

Verify the inventory exactly. A missing target, an extra asset, or a digest
that does not match the plan means **do not publish**.

## Before you publish the crate

### Breaking-invocation gate (0.2.0 and later)

**0.2.0 changes what a bare `cargo cleanme` does.** It was a read-only Routine
scan on the 0.1.x line; it is a Routine **cleanup**. Any release carrying
ADR 003 must satisfy all three, and a human must confirm them — they are not
checkable by a script in this repository:

1. **`Cargo.toml` `version` is `>= 0.2.0`.** The destructive default cannot ship
   as a `0.1.x` patch. If the tree still says `0.1.x` while the front door
   cleans, stop.
2. **`CHANGELOG.md` has a `Changed — BREAKING` section** naming the bare
   invocation change and the `--dry-run` semantics change, with a 0.1.x → new
   mapping table a user can act on.
3. **The migration table is visible outside the changelog.** A breaking default
   that only appears in a changelog is a breaking default users hit before they
   read it. `README.md` and `docs/TROUBLESHOOTING.md` must both carry it, and
   `docs/TROUBLESHOOTING.md` must keep its "Upgrading from 0.1.x" section.

Verify locally before publishing:

```sh
grep -n '^version' Cargo.toml | head -1
grep -c 'Changed — BREAKING' CHANGELOG.md
grep -c 'Upgrading from 0.1.x' docs/TROUBLESHOOTING.md README.md
```

### The package check

`cargo package --locked` and `cargo publish --locked --dry-run` must both
succeed, and you must have looked at what is inside the package:

```sh
tar tzf target/package/cargo-cleanme-<version>.crate | sort
```

The published crate is an intentional subset: `src/`, `config.toml`,
`Cargo.toml`, `Cargo.lock`, `README.md`, `CHANGELOG.md`, and the two license
files. Planning, tests, scripts, `release/`, `packaging/`, and `xtask/` are
excluded by the `include` allowlist.

`config.toml` **must** be in that list. `src/config.rs` embeds it with
`include_str!("../config.toml")`, and first-use config creation is impossible
without it. `scripts/check-release-contract.py` fails the build if it is ever
dropped.

Then build and test the packaged source, not the checkout:

```sh
mkdir -p /tmp/pkgcheck && tar xzf target/package/cargo-cleanme-<version>.crate -C /tmp/pkgcheck
cd /tmp/pkgcheck/cargo-cleanme-<version>
cargo build --locked --release
python3 scripts/smoke-release-candidate.py ./target/release/cargo-cleanme
```

## Publishing to crates.io

The initial publication needs registry authentication. Prefer trusted
publishing (OIDC) from the release workflow once the crate exists; do not
retain a long-lived token for convenience when trusted publishing is available.
If a token *is* used, it belongs in your credential store, not in the
repository.

Record the resulting package checksum in the closure record. A successful
upload is not release closure.

## External smoke, from outside the repository

Run this from a directory that does not resolve local path or git overrides,
against the published artifacts only:

```sh
cargo install cargo-cleanme --locked
cargo-cleanme --version
cargo cleanme --version
cargo cleanme config show
cargo cleanme scan --format json --no-progress <some path>
cargo cleanme update --dry-run
```

Check each of these:

- `cargo install` resolved only registry dependencies and produced the binary.
- `--version` matches the published version, under **both** spellings.
- Config bootstrap works in an isolated config home.
- A bounded read-only scan works and changes nothing.
- `--format json` emits exactly one document on stdout, with progress and
  diagnostics on stderr.
- `update --dry-run` reports already-current rather than offering a downgrade.

A binary installed by Cargo must be **refused**, not updated. `cargo cleanme
update` replaces the running executable in place when it believes the
installation is self-managed, so a misdetected Cargo root means overwriting a
file `cargo install --list` still believes Cargo owns. Check the `--root` form
specifically, not only the default `CARGO_HOME`:

```sh
cargo install cargo-cleanme --version <published> --locked --root /tmp/root
/tmp/root/bin/cargo-cleanme update          # must refuse, exit non-zero
cargo install --list --root /tmp/root       # must still report <published>
```

From 0.1.5 onward the refusal is correct and this check passes as written. It
failed on every release through v0.1.4, where the refusal named no version in
its remediation (`cargo install cargo-cleanme --locked --force` resolves to the
newest release by itself), so assert the *command*, not a version string.
`scripts/post-release-smoke.sh` covers this and fails the release if the
binary's bytes change.

Use an **absolute** path for the bounded scan. An explicitly relative root
resolved zero Cargo workspaces through 0.1.2 — tracked as C015, fixed in 0.1.3 —
so on an older version the scan reports no groups while looking healthy. The two
spellings are equivalent from 0.1.3 onward, but the absolute form costs nothing.

Then exercise the public installers on all three platforms against the real
release, and exercise `cargo cleanme update` from a draft release to the new
candidate as a rehearsal before you publish.

## What you must not do

- Do not rebuild release artifacts after qualification. If the bytes change,
  the qualification is void.
- Do not bypass Eggpack's source-binding or no-clobber rules to make a release
  go through.
- Do not publish if any required target lacks runtime evidence.
- Do not publish if `installer` or `updater` tests are failing.
- Do not publish if the crate version and the release tag disagree. This is now
  machine-checked; do not work around a failure from
  `scripts/check-release-identity.py`.
- Do not publish the crate from `main`. Publish it from a detached checkout or
  worktree of the exact tag.
- Do not self-update a manager-owned installation behind the manager's back.
  `update` already refuses this; do not work around it.

## The release builder is pinned

`release/eggpack/pack.toml` pins an exact Rust release, `1.99.0`, on all five
contracted targets. It used to be `stable`, which meant re-running a release for
the same tag could produce different bytes, so "do not rebuild after
qualification" had no reproducibility guarantee to fall back on. That is fixed.

Two facts must not be confused:

- `1.99.0` is the **release builder** — the compiler that qualified the shipped
  bytes. It is pinned in `release/eggpack/pack.toml` and rendered into the
  generated workflow.
- `1.89` is the **MSRV** — the oldest compiler cargo-cleanme claims to build
  with. It is `rust-version` in `Cargo.toml` and it is unchanged.

The pin only holds if it is regenerated. After editing `pack.toml`:

```sh
python3 scripts/gen-release-workflow-shape.py                       # rewrite the shape
eggpack ci generate \
  --github-policy release/eggpack/github-policy.json \
  --workflow-shape release/eggpack/workflow-shape.json \
  --contract release/eggpack/distribution.toml \
  --output .github/workflows/release-binaries.yml
python3 scripts/gen-release-workflow-shape.py --check               # prove no drift
```

`scripts/check-release-contract.py` refuses a `stable`/beta/nightly or bare
`major.minor` pin, a required target that disagrees with the others, and a
rendered workflow that still says `+stable`. Run it with `--self-test` to see
those refusals actually fire.

## The tag must name the version in the tree

`v0.1.1` shipped from a tag whose bytes were the reviewed ones, but the
`cargo publish` ran from a later commit on `main`. The tag and the publication
disagreed, and nothing machine-checkable noticed.

`scripts/check-release-identity.py` closes that. It binds the tag's form, the
tag's version, `Cargo.toml`, `Cargo.lock`, the revision the tag points at, and a
changelog entry, and it can also check a staged `release-manifest.json` against
the same revision:

```sh
python3 scripts/check-release-identity.py --tag v0.1.2
python3 scripts/check-release-identity.py --tag v0.1.2 \
  --manifest /path/to/staged/release-manifest.json
```

`bash scripts/release-check.sh v0.1.2` runs it as part of the gate.

The publishing order that makes the deviation impossible:

1. Commit the release content, including the version bump and changelog.
2. `bash scripts/release-check.sh v0.1.2` on a **clean** tree at the release
   commit. It refuses a dirty tree.
3. Tag that exact commit: `git tag v0.1.2`.
4. Verify the tag: `python3 scripts/check-release-identity.py --tag v0.1.2`.
5. Dispatch Eggpack at `v0.1.2`, qualify, and stage the draft.
6. Publish the crate **from a detached checkout of the tag**, not from `main`:

   ```sh
   git worktree add /tmp/cargo-cleanme-publish v0.1.2
   cd /tmp/cargo-cleanme-publish
   python3 scripts/check-release-identity.py --tag v0.1.2
   cargo publish --locked
   ```

7. Only after the crate is published, return to `main` and continue planning or
   closure documentation there.

Publishing from `main` after tagging is the specific deviation to avoid. If the
tree on `main` has moved on, the crate and the release would describe different
commits while carrying the same version.

The 0.1.0 release also shipped a defect that no test could catch, because the
feature depended on a live third-party service and every test was a fixture.
Run the external smoke in `docs/RELEASING.md` against a *staged draft* before
publishing anything, not only after. The registry's User-Agent policy is not
something a fixture can model.

## Post-release

Record the exact source revision, the tag, the crates.io package checksum, the
workflow run id, the draft and public release ids, the full asset inventory with
every SHA-256, per-platform installer smoke results, the updater rehearsal
result, the completions/manpage drift result, the benchmark and binary-size
table, and every unresolved finding with its disposition.

Write it to `plans/closure/<subsystem>/<NNN>-status.md` and reference it from
`plans/registry.md`. A corrective that is fixed but not yet carried by a
published release stays **open**, not closed: `v0.1.2` was held open precisely
because publication alone is not evidence that the live updater works, and
closing it on publication would have hidden a real defect. Closure needs a
published release *and* a rehearsal of the previous release into it.
