# Distribution, Release, and Update M010B Status

Plan: `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`

Disposition: **closed**

Implementation commits: `c9a97c9` (installers, fixture suite, contract check),
`90e23f8` (host-aware fixture), `f92af90` (per-wrapper fixture assets),
`34a1d23` (compiled fixture candidate).

Repository baseline: `b4662af` (M010A implementation commit).

Date: 2026-10-04

## Executive finding

M010B is closed. The product owns its public bootstrap installers, the wrappers
are mechanically proven to be a projection of the Eggpack contract rather than a
second release schema, and 17 deterministic fixture cases run against both
wrappers on hosted Linux, macOS, and Windows. The suite found seven real
defects — four in the installers themselves and three in the harness that was
supposed to catch them — and all seven are fixed.

No release was staged and no publication occurred. The wrappers therefore have
no release to install yet; that is the M010D precondition, not a defect here.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Exactly one `packaging/install.sh` and one `packaging/install.ps1` | both present; `check-installer-contract.py` fails if either is missing | passed |
| Contract is the target-asset authority; wrappers consume it | `check-installer-contract.py` extracts each wrapper's host-to-target mapping and compares it to the contract: the POSIX wrapper must reach all five contracted triples and no others, and the PowerShell wrapper must reach exactly the contract's Windows set | passed |
| Latest and exact-version selection | `verified binary install` and `exact version install` cases for both wrappers | passed |
| Host-mapping branch exercised | `case_happy_path` asserts the installed candidate answers `--version` with the expected identity | passed |
| SHA-256 verification enforced | `missing checksum`, `malformed digest`, and `tampered payload` cases, each asserting a non-zero exit and that nothing was placed | passed |
| Candidate identity verified | `wrong product` and `wrong version` cases assert rejection before placement | passed |
| Cargo fallback is Cargo-only | `absent binary` case asserts the documented fallback runs; `transport failure` case asserts a 5xx is **not** a fallback; `absent binary with no cargo` and `a Cargo run that produces no binary` assert hard failures | passed |
| Non-escalation and atomic placement | `existing destination` refused without `--force` and replaced with it; `unwritable destination` is a hard failure; no wrapper calls sudo/su/doas or `Start-Process -Verb RunAs` (asserted by the contract check on comment-stripped source) | passed |
| Destination scope | `malformed --version` is rejected before any network use; `temporary state is cleaned up on failure` asserts no invocation-owned directory survives | passed |
| No second release schema | wrapper asset names are built from the product and target; install names are derived from the contract; check runs in the drift gate, `release-check.sh`, and the suite | passed |
| Hosted installer smoke on Linux, macOS, and Windows | CI `installers` job on all three OSes | passed |
| Actual release artifact bytes qualified for the five targets | **not obtained** | deferred, see below |
| Five targets including Windows with direct executable assets | contract declares all five | passed |

## Verification commands and results

Local, on `34a1d23`:

- `python3 packaging/tests/test_installers.py` — 17 passed, 0 failed.
- `python3 scripts/check-installer-contract.py` — pass.
- `python3 scripts/check-release-contract.py` — pass.
- `python3 scripts/gen-release-workflow-shape.py --check` — pass.
- `eggpack ci check` — pass, `ci check: match (55991 bytes)`.
- `sh -n packaging/install.sh` and a PowerShell tokenizer parse of
  `packaging/install.ps1` — pass.
- `cargo test --all-targets --all-features` — 222 + 7 + 1 pass.
- `cargo +1.89 check --locked --all-targets` and `cargo +1.89 test --locked
  --all-targets` — pass.

Hosted: CI runs `37226716539` (ubuntu and macOS installer lanes green),
`37227018648` (Windows lane reached the PowerShell block; 14/17, the three
failures were the harness defect fixed in `34a1d23`), and the final run on
`34a1d23` recorded in the planning reconciliation.

## Defects found and fixed

Four were real installer bugs that a reviewer reading the code would likely have
missed:

1. **`curl --fail` cannot distinguish absence from failure.** Both wrappers
   treated any non-zero exit as "asset absent" and entered the documented Cargo
   fallback. A release host returning 503 would therefore have silently produced
   a locally built binary — exactly the outcome the plan forbids. Both wrappers
   now ask curl / `HttpClient` for the status and classify it; only 404 is
   absence, and a connection failure is reported as 000.
2. **The POSIX latest-install path was always broken.** It compared the
   candidate's reported version against an empty expected version, so the
   default `install.sh` with no `--version` failed every time with a spurious
   "version authority disagreement". The check now runs only for a pinned
   request, where there is something to disagree with.
3. **The POSIX wrapper's own test escape hatch did not work.** The scheme check
   permitted `http://` under `CARGO_CLEANME_INSTALL_ALLOW_INSECURE`, but a
   hardcoded `--proto '=https'` rejected it anyway. The protocol allowlist is
   now derived from the scheme decision so the two cannot disagree.
4. **The installer-override `--proto` was stricter than the documented
   override.** Fixed together with the above.

Three were harness bugs that made cases pass without testing their subject:

5. **Control files were found with `with_suffix`.** For an asset named
   `....sha256` that rewrites the asset name itself, so the control file was
   never found and checksum cases passed vacuously. Control paths are now built
   by explicit name concatenation.
6. **The PowerShell negative cases ran on Linux.** `install.ps1` refuses at its
   own platform guard, so all of them "passed" without reaching the logic under
   test. The block is now skipped off-Windows with an explicit message and
   qualified for real by the hosted Windows lane.
7. **The fixture published the wrong asset.** It published only the Linux
   asset, so an arm64 macOS runner's correct request 404'd and every case
   silently exercised the Cargo fallback; and on Windows the asset name
   resolved to empty, so the write landed on the release directory. The fixture
   is now built per block from the contract, and the candidate is a compiled
   executable so the PowerShell identity check can really run.

An additional harness correction: the exec-bit assertion in the contract check
used the filesystem, but a Windows checkout has no exec bit. It now reads the
recorded mode from `git ls-files -s`, which is the durable fact.

## Runtime release-artifact qualification: obtained later, in M010D

The plan requires qualifying the actual release bytes for the five M010A
targets, which requires dispatching `release-binaries.yml` for a real tag. That
was not done at M010B closure and was named as the explicit M010D precondition.

**It is now discharged.** Release workflow run `37232595211` built, qualified,
and validated all five targets green, `scripts/validate-staged-release.py`
validated the staged draft, and both Linux binaries were confirmed to require at
most `GLIBC_2.17.0` by static ELF evidence. See
`plans/closure/distribution-release-update/010d-status.md` for the full
inventory and digests. The passage below is retained as written at M010B
closure.

What is qualified: the *wrappers* against a fixture that reproduces the release
layout, including the version-free asset names that let one URL serve both
`releases/latest/download` and `releases/download/vX.Y.Z`.

What is not qualified: the real Eggpack-produced artifacts, the real glibc 2.17
ABI floor on `x86_64` and `aarch64` Linux, and the end-to-end agreement between
what Eggpack stages and what the wrappers and the M010C updater consume.

The README therefore still refuses to advertise the ABI floor, and
`check-release-contract.py` fails the build if it starts to.

## Known limitations

- `install.sh` is POSIX `sh` and is exercised on Linux and macOS only. It
  refuses a Windows host by design.
- The `armv7` family is recognized and routed to Cargo; it is not a
  configurable Cargo-fallback host. Both are registered follow-ups.
- No pre-1.0 package-manager formulae, musl targets, or Windows ARM64 binaries
  exist. The wrappers map those hosts to Cargo, which is documented.
- The `--force` replacement path does not preserve the previous file's mode;
  the installed candidate is placed with the wrapper's own permissions.

## Future-plan review

M010B closes and stabilizes the release convention that M010C depends on, so
M010C is unblocked on its interface dependency. M010C's remaining hard
dependency, Eggup acquisition M009, is satisfied: `eggup-curl 0.1.2` resolves
from the crates.io registry, verified by a `cargo generate-lockfile` probe on
2026-10-04. M010D remained blocked at the time on M010B's runtime
release-artifact qualification and M010C's closure, both of which require a real
release. Both are discharged: see the addendum above and
`plans/closure/distribution-release-update/c011-status.md`.

## Addendum (2026-10-04, after the v0.1.1 publication)

**One row in the installer matrix above was green without executing.** The case
"a Cargo run that produces no binary is a hard failure" is listed as passing on
the hosted Windows lane. It had never run there.

The harness's fake `cargo` was an extensionless `#!/bin/sh` file.
`install.ps1` resolves cargo through PATHEXT, so the stub was invisible and the
real `cargo.exe` was used. The PATH was additionally merged with `:` rather than
`os.pathsep`, which on Windows left the real cargo reachable. The case was
exercising a real `cargo install cargo-cleanme`, not the wrapper.

It passed regardless, because before 0.1.0 was published that real install
failed — and a real failing cargo is observationally identical to a correct
wrapper rejecting an empty Cargo run. When 0.1.0 reached crates.io the real
install began succeeding and the case turned red, on a commit that changed only
`plans/**`. The defect was disclosed by the product working correctly.

`install.ps1` itself is correct and is unchanged. Nothing in v0.1.0 or v0.1.1 is
affected; the defect was in the test harness alone.

Discharged by corrective
`plans/implementation/distribution-release-update/c012-windows-installer-fixture-corrective.md`
(implementation `ab4b080`), closure
`plans/closure/distribution-release-update/c012-status.md`. The fixture now
writes a `cargo.cmd` shim on Windows, merges PATH with `os.pathsep`, and asserts
its own premise — that the cargo the wrapper resolves is the stub — before any
case depends on it. Hosted CI run `37235169841` is green on all nine jobs
including `installers (windows-latest)`.
