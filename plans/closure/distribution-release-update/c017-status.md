# Distribution, Release, and Update C017 Status

Plan: `plans/implementation/distribution-release-update/c017-cargo-managed-provenance-misdetection-corrective.md`

Disposition: **closed**

Discovered by: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`
(work package F, the `v0.1.3` -> `v0.1.4` rehearsal, cargo-managed scenario)

Precondition: C013 closed at `51d70f1` (`c013-status.md`); C015 closed at
`1316e81` (`c015-status.md`)

Implementation commit: `0aa7664` (the fix, the real-schema fixtures, the plan)

Release source revision: `d9e0219b999cc28a0fd2af3dcacf5a351af17e4c` (tag `v0.1.5`)

Date: 2026-10-05

## Executive finding

`cargo cleanme update` **replaced a binary that Cargo owns**, silently, and
exited `0`.

`cargo install --root DIR` — the form every hermetic install script, CI job, and
container image uses — puts the binary in `ROOT/bin` and the record in
`ROOT/.crates.toml`. `cargo_bin_root_containing` compared the running
executable's parent against `$CARGO_HOME/bin` and nothing else, so any install
whose root was not the process `CARGO_HOME` was classified
`VerifiableSelfManaged` and overwritten in place.

This is the only defect in the post-release corrective line that **mutated**.
C011, C015, and C016 all failed loudly and wrote nothing.

A second, independent bug was hiding behind the first.
`cargo_recorded_version` read `[packages.<name>].vers`, which no cargo has
emitted for years; cargo writes a flat `[v1]` table keyed by the full spec
string. The parse therefore always returned `None`, and the refusal observed in
the `$CARGO_HOME/bin` case was an accident of two bugs rather than a decision.

## Evidence, before the fix

Case A — `cargo install --root`, against published v0.1.3:

~~~text
$ cargo install cargo-cleanme --version 0.1.3 --locked --root /tmp/c017-repro/root
   Installed package `cargo-cleanme v0.1.3` (executable `cargo-cleanme`)

$ /tmp/c017-repro/root/bin/cargo-cleanme update --no-progress
cargo-cleanme: updated to 0.1.4
exit=0

binary now reports:     cargo-cleanme 0.1.4
cargo still records:    "cargo-cleanme 0.1.3 (registry+...)" = ["cargo-cleanme"]
cargo install --list:   cargo-cleanme v0.1.3
post-update sha256:     fb8608a30f2b259e0d7d2e9e277b8a4d5eba3f96703919c3a2e2b888c5470fdc
published v0.1.4 asset: fb8608a30f2b259e0d7d2e9e277b8a4d5eba3f96703919c3a2e2b888c5470fdc
~~~

The digest equalling the published asset is the point: the file was replaced
with release bytes, not merely diverged. Cargo's bookkeeping then lied in both
directions, so `cargo upgrade` could not offer the right target and
`cargo uninstall` was operating on a package whose contents it did not know.

Case B — `$CARGO_HOME/bin`, the location test passing and the parse failing:

~~~text
$ "$CARGO_HOME/bin/cargo-cleanme" update --no-progress
cargo-cleanme: self-update refused: cargo-cleanme is unprovable_ownership; run this instead:
  Reinstall with the published installer, then retry: https://github.com/dbowm91/cargo-cleanme
exit=2
~~~

Safe, but the remediation is wrong: the correct answer for a Cargo-installed
binary is `cargo install cargo-cleanme --locked --force`, not "reinstall with the
published installer" — which is the provenance confusion this subsystem exists
to prevent.

## Why the tests never caught it

Two invented assumptions.

1. **Location.** `cargo_bin_root_containing` enumerated exactly one bin root, the
   process `CARGO_HOME`. Every test constructed `$CARGO_HOME/bin` by hand and
   passed it to `classify_provenance_in` — the seam the test needed is the seam
   that removed the only case that fails.
2. **Schema.** `src/update.rs:1294` hand-wrote
   `[packages.cargo-cleanme]\nvers = "0.1.0"\n`, so the parser was checked
   against its own assumption and agreed with itself.

That is the blind-fixture pattern from C009, C011, C012, C013, and C016 — the
fifth occurrence in this subsystem. C013's cross-platform fixture audit reviewed
this file and recorded the assertion as sound, because it asked whether the
*assertion* was meaningful and not whether the *fixture could fail for the right
reason*.

## The fix

- `cargo_recorded_version` parses the real `[v1]` schema, requires the record's
  value list to name the running binary, and skips a malformed entry rather
  than aborting the scan.
- `cargo_bin_roots_above` walks a bounded number of ancestor directories, which
  is what finds `cargo install --root`.
- The explicit `CARGO_HOME` branch still runs first and still refuses an
  unrecorded file, so the pre-existing safety property is unchanged.

Detection is a negative test, so the fix is biased toward over-detection:
claiming a Cargo root requires a record naming **both** this package and **this**
binary, and a false positive only costs a refusal that names the manager
command, while a false negative hands a Cargo-owned file to the path that writes.

## Premise-negative evidence

Each guard was broken on purpose, one at a time, and the suite re-run. A guard
that cannot fail is not a guard.

~~~text
A. schema fix reverted (parse obsolete [packages]/vers)   26 passed; 4 failed
B. ancestor walk reverted (only $CARGO_HOME/bin)          27 passed; 3 failed
C. binary-identity requirement dropped                    29 passed; 1 failed
D. package-name filter dropped                            29 passed; 1 failed
E. malformed-entry resilience removed (? aborts scan)     29 passed; 1 failed
~~~

Two of the five **failed to bite on the first attempt**, and both had to be
rewritten before the harness could prove anything:

- The over-detection test planted a record for an unrelated package and asserted
  the binary stayed self-managed. Both the correct code and the broken code
  return self-managed there, so the test could never fail. It now plants the one
  near miss that discriminates: a record for a *different package* that installs
  a binary named `cargo-cleanme`. That is what makes break D go red.
- The malformed-entry test wrote the bad key *above* the `[v1]` header, making
  it a root-level key the parser never reads; it passed against code that
  aborted the scan. The bad key is now spliced explicitly inside the table, with
  an assertion that the fixture really has that shape.

The second is the same blind-fixture mistake, caught this time by the
premise-negative rather than by a user.

## Local verification

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features          # 235 + 8 + 1 (was 230 + 8 + 1)
cargo +1.89 check --locked --all-targets
cargo +1.89 test  --locked --all-targets         # 235 + 8 + 1
python3 scripts/check-fixture-portability.py     # 30 files
python3 scripts/check-release-contract.py (+ --self-test)
python3 scripts/check-release-identity.py --self-test
python3 scripts/check-post-release-smoke-contract.py (+ --self-test)
python3 packaging/tests/test_installers.py (+ --self-test)   # 19 passed
bash scripts/release-check.sh v0.1.5
~~~

Live, against a real `cargo install --root` layout with a real record and an
unrelated `CARGO_HOME`:

~~~text
before fix:  exit=0  "cargo-cleanme: updated to 0.1.4"   bytes: CHANGED
after fix:   exit=2  bytes unchanged
~~~

## Hosted CI

| Run | Scope | Result |
|---|---|---|
| `37254633448` | CI on the fix commit `0aa7664` | 9/9 green |
| `37254633505` | Release drift guard on `0aa7664` | green |

## v0.1.5

| Fact | Value |
|---|---|
| Source revision | `d9e0219b999cc28a0fd2af3dcacf5a351af17e4c` |
| Tag | `v0.1.5` |
| Eggpack run | `37254890208`, 20/20 green |
| Staged validation | `scripts/validate-staged-release.py --tag v0.1.5`, 6/6 |
| GitHub published | `2026-10-05T02:37:30Z` |
| crates.io | `cksum=f6b57f0aab89f1330cd2500692f47868b6658ca816b6e5ecb28d425ec3df86d7`, not yanked |
| Public asset sha256 (x86_64-unknown-linux-gnu) | `bbaa2dc71923a1b9f0f024951ee500ac0a40d094a9c21027e9ec3a31305f9248` |
| Linux ABI | both Linux artifacts `GLIBC_2.17.0` |

crates.io was published from a detached worktree at the exact tag, never from
`main` and never from CI.

## Expected-failure rehearsal: `v0.1.4` -> `v0.1.5`

Published v0.1.4 carries the defect, so this rehearsal **must** fail at the
cargo-managed scenario. It did, on a harness assertion written before the
defect was known:

~~~text
  published v0.1.4 cargo-cleanme-x86_64-unknown-linux-gnu sha256 fb8608a30f2b259e...
  published v0.1.5 cargo-cleanme-x86_64-unknown-linux-gnu sha256 bbaa2dc71923a1b9...
self-managed: update without --dry-run
  updated in place, digest equals the published v0.1.5 asset
self-managed: update --dry-run must report already-current
  already-current reported (exit 2, which is the documented 'nothing to do' status)
cargo-managed: install exact v0.1.4 into an isolated root
  Cargo-managed install reports 0.1.4 (local build, sha256 c2879276d632, not the release asset)
post-release-smoke: FAILED: update succeeded on a Cargo-managed installation; it must refuse
exit=1
~~~

This is the defect reproduced end to end by the harness, and it simultaneously
re-confirms the C016 fix: the same published 0.1.4 binary updates itself
correctly when the installation really is self-managed.

## v0.1.6, the target the fix needed

| Fact | Value |
|---|---|
| Source revision | `383b64e7c5f23916c697139099c97f79679f2314` |
| Tag | `v0.1.6` |
| Eggpack run | `37256235897`, 20/20 green |
| Staged validation | `scripts/validate-staged-release.py --tag v0.1.6`, 6/6 |
| GitHub published | `2026-10-05T02:56:29Z` |
| crates.io | `cksum=d89be4bac1096ef5b073858c7c2370b58064a130ba39828c6e567e6fcfa7134b`, not yanked |
| Public asset sha256 (x86_64-unknown-linux-gnu) | `9bb8c582adcb1cf8bd6953f084d096e8249964b123ec01f61f6552e1efe9fc2c` |
| Linux ABI | both Linux artifacts `GLIBC_2.17.0` |

v0.1.6 changes no product behavior. It exists because the refusal is raised
after the already-current check, so observing it needs an installation that is
*behind* the published version — the same gap 0.1.4 filled for C016.

## Proving rehearsal: `v0.1.5` -> `v0.1.6`

~~~text
  published v0.1.5 cargo-cleanme-x86_64-unknown-linux-gnu sha256 bbaa2dc71923a1b9...
  published v0.1.6 cargo-cleanme-x86_64-unknown-linux-gnu sha256 9bb8c582adcb1cf8...
self-managed: update without --dry-run
  updated in place, digest equals the published v0.1.6 asset
self-managed: update --dry-run must report already-current
  already-current reported (exit 2, which is the documented 'nothing to do' status)
cargo-managed: install exact v0.1.5 into an isolated root
  Cargo-managed install reports 0.1.5 (local build, sha256 133edfe9abbc, not the release asset)
  refused as Cargo-managed, remediation is the manager command, bytes untouched

post-release-smoke: PASSED (x86_64-unknown-linux-gnu v0.1.5 -> v0.1.6)
~~~

The refusal, verbatim:

~~~text
cargo-cleanme: self-update refused: cargo-cleanme is cargo_managed; run this instead:
  cargo install cargo-cleanme --locked --force
~~~

### A latent harness bug this rehearsal exposed

The first run of this rehearsal **failed on the harness, not the product**. The
refusal above is exactly what C017 requires, and the harness rejected it with:

~~~text
post-release-smoke: FAILED: the remediation does not name v0.1.6: ...
~~~

The assertion required the remediation to name `v0.1.6`, which no correct
refusal can do: `Provenance::CargoManaged::remediation` returns `cargo install
cargo-cleanme --locked --force` and deliberately carries no version, because
that command resolves to the newest published release on its own. Handing a
Cargo user back to their package manager is the point; a version-pinned
reinstall would be the wrong instruction for someone who ran `update` in order
to get the newer version.

The bug was latent rather than obvious because the cargo-managed scenario had
**never previously reached that line**: with the pre-fix binary the update
succeeded and the harness failed one assertion earlier.

Worse, the assertion was not merely unsatisfiable but actively wrong. Checked
against plausible output it would have **accepted**:

~~~text
cargo-cleanme: updated to 0.1.6, see v0.1.6 release notes
~~~

— a message claiming the tool performed the update, which is the exact outcome
the scenario exists to forbid and which would accompany an overwritten binary.

Replaced at `3548fc2` with an assertion of the exact manager command: stricter
than a version substring, satisfiable by correct behavior, and verified in both
directions. It matches the real refusal and rejects all four wrong
remediations, including the success message the old check waved through.

This is the third harness bug this rehearsal line has found — after the
already-current exit-2 semantics and `cargo install --no-progress` being an
invalid flag — and the first that was hiding a check weaker than it appeared.

## Acceptance criteria

| Criterion | Evidence | Result |
|---|---|---|
| A published release's updater **refuses** a `cargo install --root` installation with unchanged bytes, and names `cargo install cargo-cleanme --locked --force` | `v0.1.5` -> `v0.1.6`, "refused as Cargo-managed, remediation is the manager command, bytes untouched" | pass |
| The same release refuses a `$CARGO_HOME/bin` installation, not `unprovable_ownership` | `cargo_recorded_version` parses the real schema; `the_crates_toml_we_parse_is_the_one_cargo_writes` and the two refusal tests | pass |
| Both premise-negatives are reproduced from the closing tree | five deliberate breaks, 4/3/1/1/1 red (§ Premise-negative evidence) | pass |
| Hosted Linux/macOS/Windows CI green on the release commit | `37254633448` 9/9, drift `37254633505` | pass |
| `v0.1.1`..`v0.1.4` recorded as immutable releases carrying this defect | see Disposition | pass |

## Five-target hosted smoke

Run `37257527651`, dispatched as `post-release-smoke.yml` with
`from_version=0.1.5`, `to_version=0.1.6`, across every contracted target:

| Target | Runner | Result |
|---|---|---|
| `aarch64-apple-darwin` | `macos-14` | pass |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | pass |
| `x86_64-apple-darwin` | `macos-15-intel` | pass |
| `x86_64-pc-windows-msvc` | `windows-latest` | pass |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | pass |

Run conclusion: **success**, 5/5.

The `x86_64-unknown-linux-gnu` lane log, verbatim:

~~~text
self-managed: update without --dry-run
  updated in place, digest equals the published v0.1.6 asset
self-managed: update --dry-run must report already-current
  already-current reported (exit 2, which is the documented 'nothing to do' status)
cargo-managed: install exact v0.1.5 into an isolated root
  refused as Cargo-managed, remediation is the manager command, bytes untouched
post-release-smoke: PASSED (x86_64-unknown-linux-gnu v0.1.5 -> v0.1.6)
~~~

This is the evidence that C017's refusal holds against **real published
binaries on real hosted runners**, not only against a locally built binary and a
fixture. It also supplies the Windows commit-path evidence C016 could not get
from a fixture, since Eggup's identity validator executes the candidate and a
`#!/bin/sh` stub is not a Windows executable.

## Known limitation

The `Provenance` refusal is raised at `src/update.rs:834-839`, *after* the
already-current check at `789-795`. A Cargo-managed binary already at the
published version therefore reports "already at the latest stable version" and
is never told it is Cargo-owned. Nothing is acquired and nothing is written in
that case, so it is safe; the ordering is left alone because changing it would
alter the meaning of a correct no-op message for reasons unrelated to this
defect. It is disclosed in the v0.1.5 changelog rather than hidden.

## Disposition of earlier releases

`v0.1.0` through `v0.1.4` all carry the defect and none is yanked. Their scan,
clean, config, reporting, and self-managed behavior is correct, and the failure
is a silent success rather than a loud one, so yanking would misdescribe
releases that are otherwise safe to install and would be a breaking change for
users with no compensating benefit. The defect is disclosed in the v0.1.5
changelog under "Known issue in earlier releases", with the safe upgrade route
(`cargo install cargo-cleanme --locked`) named.
