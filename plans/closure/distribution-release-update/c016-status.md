# Distribution, Release, and Update C016 Status

Plan: `plans/implementation/distribution-release-update/c016-live-self-update-identity-invocation-corrective.md`

Disposition: **closed**

Discovered by: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`
(work package F, the real `v0.1.1` -> `v0.1.2` rehearsal)

Precondition: C013 closed at `51d70f1` (`c013-status.md`)

Implementation commit: `826fbf2` (the fix and the argv-sensitive candidate stub)

Release source revisions: `76e672fe2d5eb20e581bac5f2d36ba7537f6a64e` (tag `v0.1.3`,
carries the fix) and `404d871634ca07eae5b35b00ec8c412d848e21a4` (tag `v0.1.4`,
supplies the newer target)

Proving rehearsal: `v0.1.3` -> `v0.1.4`, repeated and re-confirmed as
`v0.1.5` -> `v0.1.6`

Date: 2026-10-05

## Executive finding

`cargo cleanme update` had **never once completed a commit** in any published
version. It always failed at the last step, safely, with an error message that
named the failure precisely.

`ExactIdentityValidator::new(member_id, expected)` leaves `args` empty, and the
candidate was therefore executed with **no arguments**. The validator requires
`stdout == b"cargo-cleanme <version>\n"` and empty stderr, so with no arguments
the candidate ran its **default routine scan** and the comparison could never
match — and before failing, it scanned the user's entire filesystem.

The fix is one line at the call site: pass `--version`. The recorded version is
the input to a self-update, so demanding a version query is the only sane
default, but nothing in the type system, the API, or the error message said so.

## Why the tests never caught it

The commit-path candidate stub was:

~~~sh
#!/bin/sh
echo 'cargo-cleanme 0.1.2'
~~~

It ignored argv. Any invocation produced the identity string, so the fixture
could not distinguish a correct `--version` call from the argv-less one
production actually performed. All seven commit-path tests were green while the
feature was non-functional.

This is the shape C011, C012, and C013 had each already paid for in this
subsystem, and C013's audit recorded this fixture as "correct — exact identity
string" because it asked whether the *assertion* was meaningful and not whether
the *fixture could fail for the right reason*.

The fix made the stub argv-sensitive: it prints the identity for `--version`
only, and writes to stderr and exits non-zero for anything else, as the real
binary does.

## Premise-negative evidence

Product fix reverted, strict stub kept:

~~~text
test update::tests::a_verified_candidate_replaces_the_live_binary ... FAILED
test update::tests::staging_is_cleaned_up_on_success_and_on_failure ... FAILED
update commits: Transaction { detail: "the candidate did not identify itself as
  cargo-cleanme 0.2.0\ncandidate execution failed: member cargo-cleanme did not
  produce the exact expected identity" }
test result: FAILED. 23 passed; 2 failed
~~~

With the fix restored, all 25 passed at the time. The pre-fix tree would not
have gone red, which is the entire point.

## Proving rehearsal

`v0.1.3` -> `v0.1.4` on `x86_64-unknown-linux-gnu`, and again
`v0.1.5` -> `v0.1.6` after the C017 fix:

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

The post-update digest equalling the published asset digest is the criterion
that matters: the transaction committed, and committed the *right* bytes.

## Expected-failure rehearsal, recorded as evidence

`v0.1.2` -> `v0.1.3` was run deliberately. Published v0.1.2 carries the broken
updater, so it must fail, and its failure is the evidence that the defect was
real in the wild rather than only in a fixture. It failed with:

~~~text
the candidate did not identify itself as cargo-cleanme 0.1.2
candidate execution failed: member cargo-cleanme did not produce the exact
expected identity
~~~

## Releases that carried the fix

| Version | Revision | CI | Eggpack | GitHub published | crates.io cksum |
|---|---|---|---|---|---|
| v0.1.3 | `76e672f` | `37250022529` 9/9 | `37250296062` 20/20 | `2026-10-05T01:33:57Z` | `d76c4fbd2b368ad52cff2942a55f6648a7b83286023b9258f71f30f4b2fbd200` |
| v0.1.4 | `404d871` | `37252028141` 9/9 | `37252232330` 20/20 | `2026-10-05T01:55:48Z` | `783dd7a6a8a141521774552aefec7616391363ddde067c06f23602ab86b9470a` |
| v0.1.5 | `d9e0219` | `37254633448` 9/9 | `37254890208` 20/20 | `2026-10-05T02:37:30Z` | `f6b57f0aab89f1330cd2500692f47868b6658ca816b6e5ecb28d425ec3df86d7` |
| v0.1.6 | `383b64e` | — | `37256235897` 20/20 | `2026-10-05T02:56:29Z` | `d89be4bac1096ef5b073858c7c2370b58064a130ba39828c6e567e6fcfa7134b` |

## Acceptance criteria

| Criterion | Evidence | Result |
|---|---|---|
| A published release's updater completes a real self-update against a newer public release, post-update digest equal to the published asset digest | `v0.1.3` -> `v0.1.4` and `v0.1.5` -> `v0.1.6`, "updated in place, digest equals the published asset" | pass |
| `update --dry-run` afterwards reports already-current | both rehearsals; exit 2 is the documented "nothing to do" status | pass |
| A Cargo-managed installation still refuses, with unchanged bytes | `v0.1.5` -> `v0.1.6`, "refused as Cargo-managed ... bytes untouched" | pass |
| The premise-negative is reproduced from the closing tree | 2 commit tests red with the fix reverted | pass |
| Hosted Linux/macOS/Windows CI green on the release commit | `37250022529`, `37252028141`, `37254633448`, all 9/9 | pass |
| The published 0.1.2 updater is recorded as broken, not as qualified | the `v0.1.2` -> `v0.1.3` expected-failure rehearsal above | pass |

## Five-target hosted smoke

Run `37257527651` (`from_version=0.1.5`, `to_version=0.1.6`), conclusion
**success**, 5/5 across `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, and `x86_64-unknown-linux-gnu`.

This is what discharges the "Windows commit-path evidence" limitation recorded
below: the commit path executes the candidate, so a `#!/bin/sh` fixture stub
cannot run on Windows at all. The hosted rehearsal is the only honest source of
that evidence, and it now confirms the commit path on `windows-latest`.

## Known limitations

1. **Provenance is not surfaced when there is nothing to do.** The `Provenance`
   refusal is raised at `src/update.rs:834-839`, after the already-current check
   at `789-795`, so a current Cargo-managed binary reports "already at the latest
   stable version" and is never told it is Cargo-owned. Safe — nothing is
   acquired or written — and left unchanged. This is C017's documented
   limitation, and it is the reason a version behind the published one is needed
   to observe the refusal.
2. **Windows commit-path evidence is live, not fixture.** Eggup's
   `ExactIdentityValidator` *executes* the candidate, and a `#!/bin/sh` stub is
   not a Windows executable. The commit-path cases are therefore `#[cfg(unix)]`,
   and Windows evidence comes from the hosted rehearsal rather than from a
   fixture that could not have run.

## Disposition of earlier releases

`v0.1.1` and `v0.1.2` are immutable and both ship an updater that cannot
complete a commit. Neither is yanked: their scan, clean, config, and reporting
behavior is correct, and the updater fails loudly without mutating anything, so
yanking would misdescribe releases that are safe to install and would be a
breaking change for no compensating benefit. Recorded rather than hidden.
