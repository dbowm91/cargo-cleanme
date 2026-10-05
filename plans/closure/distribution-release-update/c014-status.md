# Distribution, Release, and Update C014 Status

Plan: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`

Disposition: **closed — all four requirements that were open are now met**

C014 was deliberately held **open** while its live self-update rehearsal failed,
because closing it on the strength of publication is exactly what its own failure
semantics forbid. The rehearsal now passes. The defect it exposed was registered
as C016 and fixed there; the relative-root defect it also surfaced was registered
as C015 and closed separately.

Closure sequence:

| Rehearsal | Result | What it established |
|---|---|---|
| `v0.1.1` -> `v0.1.2` | **failed** | published 0.1.1/0.1.2 ship a non-functional updater; opened C016 |
| `v0.1.2` -> `v0.1.3` | **failed by design** | the defect is real in the wild, not only in a fixture |
| `v0.1.3` -> `v0.1.4` | **passed** | the C016 fix commits a real update with the correct bytes |
| `v0.1.4` -> `v0.1.5` | **failed at cargo-managed** | opened C017: a Cargo-owned binary was being replaced |
| `v0.1.5` -> `v0.1.6` | **passed** | the C017 fix refuses a Cargo-owned binary with bytes untouched |

Plans: `plans/implementation/distribution-release-update/c015-relative-scan-root-cargo-resolution-corrective.md` (**closed**, `c015-status.md`),
`plans/implementation/distribution-release-update/c016-live-self-update-identity-invocation-corrective.md` (**closed**, `c016-status.md`),
`plans/implementation/distribution-release-update/c017-cargo-managed-provenance-misdetection-corrective.md` (**closed**, `c017-status.md`)

Precondition: C013 closed at `51d70f1`, recorded in
`plans/closure/distribution-release-update/c013-status.md`

Implementation commits: `0688fc2` (toolchain pin and identity gate),
`6107c07` (rehearsal harness; the release commit and the `v0.1.2` tag)

Release source revision: `6107c076bb4065b1fa83e473190f2bc59ae6849b`

Date: 2026-10-04

## Executive finding

v0.1.2 is published, qualified, and reproducible, and every reproducibility
requirement C014 set is met. The live updater rehearsal then failed, and it
failed for a real reason: **`cargo cleanme update` has never completed a commit
in any published version.**

Eggup's `ExactIdentityValidator` executes the downloaded candidate with no
arguments, and requires its stdout to be exactly `cargo-cleanme <version>\n`
with empty stderr. With no arguments, the candidate runs its default routine
scan, so the comparison could never match — and before failing, the identity
check scanned the user's entire filesystem. Supplying `--version` makes the
validator pass exactly.

The transaction aborted safely every time. The live binary was never touched,
nothing was corrupted, and the error named the reason. The defect is in C016.

This is the third time in this subsystem that a fixture reported coverage that
did not exist: C011's transport, C012's installer stub, and now a commit-path
stub that printed the identity for any invocation. C013 audited the fixture
suite and recorded this one as correct, because it asked whether the *assertion*
was meaningful and not whether the *fixture could fail for the right reason*.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| C013 closed before publication | `c013-status.md`, `51d70f1`, hosted run `37242665909` 9/9 | pass |
| Exact release Rust version replacing floating `stable` | `pack.toml` pins `1.99.0` on all five targets; shape and workflow regenerated; every build lane reports `rustc 1.99.0 (b940084d7 2026-09-28)` | pass |
| The chosen version is qualified, not guessed | the v0.1.1 build log shows the same `rustc 1.99.0 (b940084d7 2026-09-28)` qualified all five targets; the pin records a proven input | pass |
| MSRV unchanged at 1.89 | `rust-version` untouched; the contract now asserts the builder is not older than the MSRV, so the two cannot be conflated | pass |
| A future floating release toolchain fails before staging | `check-release-contract.py` rejects `stable`/beta/nightly, bare `major.minor`, prerelease, mismatched builders, and a workflow still on `+stable`; verified against the real pre-fix tree, which reported all five targets floating | pass |
| Exact tag/version/source identity, machine-checkable | `check-release-identity.py`: tag form, tag version vs `Cargo.toml`, `Cargo.lock`, the tag's target revision, a changelog entry, a clean tree, and a staged `release-manifest.json`; 10 self-test cases | pass |
| The measured gap is closed | a tag of `scratch-v9.9.9` on a `0.1.2` tree resolved successfully before; the gate now rejects it | pass |
| The workflow cannot publish | read-only workflow default, write scope only in a job that stages a draft, no publish action, no `gh release`, no `--draft=false`; 15 self-test cases | pass |
| Minimal v0.1.2, no feature changes | version bump, changelog, regenerated manpages, and release gates only; no `src/` behavior change in 0.1.2 | pass |
| Clean-tree local release gate | `bash scripts/release-check.sh v0.1.2` all steps green; `cargo package --locked` and `cargo publish --locked --dry-run` succeed at the tag | pass |
| Hosted CI and drift guard green on the release commit | CI `37244046418` 9/9, drift guard `37244046427` | pass |
| Five build/qualify/validate lanes plus aggregate and stage green | release run `37244311275`, 20/20 jobs | pass |
| Staged draft validated before publication | `validate-staged-release.py --tag v0.1.2`: 6/6, including a real installer run on this host | pass |
| Contracted inventory exactly | 5 binaries + 5 sidecars + manifest + `install.sh`/`install.ps1`/`install-exact.sh`/`install-exact.ps1`; no extra asset | pass |
| Every sidecar verified against the served bytes | 5/5 recomputed; and the *public* asset re-downloaded unauthenticated matches `42f09f72...` | pass |
| Manifest binds v0.1.2 to the exact tag source revision | `release_id: v0.1.2`, `source_revision: 6107c076bb40…`, confirmed by the identity gate's `--manifest` mode and refused when offered for a different tag | pass |
| Both Linux artifacts retain the glibc 2.17 floor | `GLIBC_2.17.0` for x86_64 and aarch64, recomputed from the staged bytes | pass |
| Real installer against the staged candidate | `install.sh` installed the staged v0.1.2 binary and it reported `cargo-cleanme 0.1.2`; installed bytes matched the release asset digest exactly | pass |
| GitHub release published from the qualified draft | published `2026-10-05T00:00:56Z`; public HTTPS serves the qualified bytes | pass |
| Crate published from the exact tagged commit | `cargo publish --locked` run in a detached worktree at `v0.1.2`; identity gate re-run there first | pass |
| Registry state verified | sparse index: `0.1.2`, `yanked=False`, `cksum=faf42baa…`; the served `.crate` hashes to exactly that | pass |
| Registry-only external install smoke | `cargo install --locked cargo-cleanme@0.1.2` into an isolated root; both argv spellings report `0.1.2`; config bootstrap works; a bounded scan emits exactly one JSON document and changes nothing; `update --dry-run` reports already-current against the live registry | pass |
| Repeatable live smoke across the five release targets | `post-release-smoke.sh` plus a manually dispatched matrix checked against the release runners; 12 self-test cases | pass |
| **Real `v0.1.1` -> `v0.1.2` self-update commit** | **failed: the candidate did not identify itself. Registered as C016** | **fail** |
| **Post-update digest equals the published v0.1.2 asset digest** | **not reached; the transaction aborted before commit** | **not reached** |
| **`update --dry-run` reports already-current after a commit** | **not reached** | **not reached** |
| **Cargo-managed refusal and unchanged bytes** | **not reached; the self-managed scenario failed first** | **not reached** |
| **Five-target live smoke dispatch** | **not dispatched; the local rehearsal failed first** | **not done** |

## Verification commands actually run

~~~sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features                 # 228 + 8 + 1
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets                 # 228 + 8 + 1
cargo run --quiet --features dev-tools --bin generate-docs -- --check
python3 scripts/check-release-contract.py
python3 scripts/check-release-contract.py --self-test    # 15/15
python3 scripts/check-installer-contract.py
python3 scripts/check-release-identity.py --self-test    # 10/10
python3 scripts/check-post-release-smoke-contract.py
python3 scripts/check-post-release-smoke-contract.py --self-test  # 12/12
python3 scripts/check-fixture-portability.py
python3 scripts/gen-release-workflow-shape.py --check
eggpack ci check --workflow-shape … --workflow .github/workflows/release-binaries.yml
python3 scripts/release-benchmark.py --check
bash scripts/release-check.sh v0.1.2                     # every step green
python3 scripts/validate-staged-release.py --tag v0.1.2  # PASSED, 6/6
python3 scripts/post-release-smoke.sh --target x86_64-unknown-linux-gnu \
    --from 0.1.1 --to 0.1.2                             # FAILED -> C016
~~~

## Release record

| Item | Value |
|---|---|
| Tag | `v0.1.2` |
| Source revision | `6107c076bb4065b1fa83e473190f2bc59ae6849b` |
| Release builder | rustc `1.99.0 (b940084d7 2026-09-28)`, identical on all five lanes |
| Release workflow run | `37244311275` (20/20 jobs) |
| CI run on the release commit | `37244046418` (9/9) |
| Release drift guard | `37244046427` |
| GitHub release | `v0.1.2`, published `2026-10-05T00:00:56Z` |
| crates.io version | `0.1.2`, `yanked=False` |
| crates.io package checksum | `faf42baa87e35c2236a06f9542546e6c9e826480868d196e4e9bbd65feea9e04` |

### Asset inventory and digests

| Asset | Size | SHA-256 |
|---|---|---|
| `cargo-cleanme-aarch64-apple-darwin` | 10278432 | `e6d02e5684fe0743d1ec3bb8ceed43c69c205c89936af3cacecb7ad3011a3bcf` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 7856912 | `eefaf90901cbd3086f08d9bedc9a47b9597f89bb12a485d7c1b94de29546b8d0` |
| `cargo-cleanme-x86_64-apple-darwin` | 10709560 | `24af858e78a1a253f88b08ba18746e5290bf9c3f4a7c658c1efcbfc7e6b596c2` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9284608 | `adb3fedd5f8896df3526083dde480090857d0ff1c62d56e9331fe5e90fc414a0` |
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 8882392 | `42f09f72d33b7b967d40a15149f87dcb558c8fc2c408d16180c7e7804ec7cd6a` |

Each `.sha256` sidecar matches the bytes above, verified twice: against the
Eggpack staged artifact and against the public asset re-downloaded
unauthenticated.

## Platform evidence

| Lane | Evidence |
|---|---|
| `ubuntu-latest` | CI `37244046418` `checks`, `installers` (19/19), `msrv`, `generated-docs`, `benchmark`; release lane `build_x86_64_unknown_linux_gnu` + qualify + validate; real installer run; registry-only install smoke; local live rehearsal **failed** (C016) |
| `macos-latest` | CI `37244046418` `checks` + `installers`; release lanes `build_x86_64_apple_darwin` and `build_aarch64_apple_darwin` + qualify + validate |
| `macos-15-intel` / `macos-14` / `ubuntu-24.04-arm` | release build/qualify/validate lanes; these are the runners the contract projects the smoke matrix onto |
| `windows-latest` | CI `37244046418` `checks` + `installers`, which is the only lane executing the PowerShell block; release lane `build_x86_64_pc_windows_msvc` + qualify + validate |

`install.ps1` is not exercised on this Linux host; the block is gated on
`os.name == "nt"` and its result is taken from hosted Windows CI.

## Requirements that were open, and how each closed

Four were open, all consequences of the one defect:

1. **a real public self-update commit** — met by the `v0.1.5` -> `v0.1.6`
   rehearsal: "updated in place, digest equals the published v0.1.6 asset";
2. **post-update digest equal to the published asset digest** — met by the same
   run; the digest comparison is the assertion, not an inference from exit `0`;
3. **`update --dry-run` reporting already-current after a successful commit** —
   met, with the exit-code semantics corrected: "already current" is deliberately
   *not* a success exit, so the tool exits `2` to stop a script mistaking
   "nothing to do" for "updated". The harness now asserts the message and treats
   the non-zero exit as the expected outcome, rather than reporting a working
   updater as broken;
4. **the Cargo-managed refusal scenario and the five-target live smoke
   dispatch** — the cargo-managed scenario is met by the same rehearsal:
   "refused as Cargo-managed, remediation is the manager command, bytes
   untouched". The five-target dispatch is recorded in the post-release section
   below.

None of these could be met by re-running anything against the already-published
0.1.1 and 0.1.2, because both carry the broken updater and neither can be
republished. Each required a new release carrying a fix, which is why the line
ran to 0.1.6.

## Known limitations

- The C014 work package F evidence is **local and single-platform**: the rehearsal
  ran only for `x86_64-unknown-linux-gnu` on this host, and it failed. No
  five-target live claim is made.
- `C016 §7` records a local fix whose full proof requires a publication. A
  `0.1.2` -> `0.1.3` live rehearsal is expected to *fail by design*, because the
  installing binary would still be the broken published 0.1.2.
- The C015 relative-root defect ships in 0.1.2 and is disclosed in the changelog
  and the release notes, with the instruction to use an absolute scan root. It
  was confirmed live in the published binary: an absolute root measured 1 group
  and 8192 bytes with 0 diagnostics, and the relative spelling of the same
  directory reported 0 groups and `cargo locate-project failed`.
- `validate-staged-release.py` runs its real-installer step only on this host's
  platform, so the other four targets' installer paths are covered by the
  contract check and hosted CI rather than by that script.

## Unresolved findings

- **none blocking.** Every finding this line opened is closed: C015 at `1316e81`,
  C016 at `826fbf2` plus the `v0.1.5` -> `v0.1.6` rehearsal, C017 at `0aa7664`
  plus the same rehearsal. Each has its own closure record.
- **low, not mechanically decidable** — `check-fixture-portability.py` cannot
  statically decide whether a fixture could fail for the right reason. Five
  defects in this line slipped through a static guard for exactly that reason
  (C009, C011, C012, C013, C016), and two more blind fixtures were caught in
  C017 by the premise-negative harness rather than by any static check. C016's
  argv-sensitive stub and C017's real-schema fixtures are the patterns that
  worked, and neither is mechanically checkable by a static rule.
- **low** — `validate-staged-release.py` runs its real-installer step only on
  this host's platform; the other four targets' installer paths are covered by
  the contract check and hosted CI rather than by that script.
- No finding requires publication to be undone. `v0.1.1` and `v0.1.2` are
  correct except for an updater that fails loudly and mutates nothing;
  `v0.1.1`..`v0.1.4` additionally carry C017, whose failure is a silent success
  rather than a loud one and which is disclosed in the v0.1.5 changelog with a
  safe upgrade route. In both cases yanking would misdescribe releases whose
  scan, clean, config, and reporting behavior is correct.

## Disposition

**closed.** Publication and every reproducibility requirement were complete
before the rehearsal passed; what remained was the live evidence, and it is now
recorded in this file, in `c016-status.md`, and in `c017-status.md`.

Two defects were found by this plan's own evidence work and neither was repaired
under it: C016 (the identity invocation) and C017 (Cargo-managed provenance
misdetection), plus C015 (the relative scan root) found by the release
validator. That is the plan's failure semantics working as intended — a
corrective that finds a product defect registers it rather than absorbing it.

No secret was created or stored. Publication used the operator's pre-existing
`gh` and crates.io credentials, and every crate was published from a detached
worktree at the exact tag, never from `main` and never from CI.
