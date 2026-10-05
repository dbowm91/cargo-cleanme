# C016 — Live Self-Update Commit Path Never Invoked the Candidate Correctly

Status: closed

Discovered by: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`
(work package F, the real `v0.1.1` -> `v0.1.2` rehearsal)

Closed. The fix shipped in v0.1.3; v0.1.4 supplied the newer target it needed
and the `v0.1.3` -> `v0.1.4` rehearsal passed, re-confirmed as
`v0.1.5` -> `v0.1.6`.

Closure record: `plans/closure/distribution-release-update/c016-status.md`

Related: C011 (the updater transport corrective, which shipped in 0.1.1),
C012/C013 (the fixture-premise correctives), C015 (the relative-root defect).

Source roadmap: post-Phase-10 distribution/release/update corrective line

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: corrective / production defect

## 1. Objective

Make the self-update commit path actually work, and make the fixture suite able
to prove it.

`cargo cleanme update` has never once completed a real commit. It has always
failed at the last step, safely, with an error message that named the failure
precisely. The bytes were never wrong and nothing was ever deleted; the
transaction simply never reached its commit.

## 2. Evidence

The C014 rehearsal, run against two real public releases on
`x86_64-unknown-linux-gnu`:

~~~text
$ bash scripts/post-release-smoke.sh --target x86_64-unknown-linux-gnu \
    --from 0.1.1 --to 0.1.2
  published v0.1.1 cargo-cleanme-x86_64-unknown-linux-gnu sha256 96354b5e...
  published v0.1.2 cargo-cleanme-x86_64-unknown-linux-gnu sha256 42f09f72...
self-managed: update without --dry-run
post-release-smoke: FAILED: update failed on a self-managed installation:
  cargo-cleanme: self-update failed: the update transaction failed:
  the candidate did not identify itself as cargo-cleanme 0.1.2
  candidate execution failed: member cargo-cleanme did not produce the exact
  expected identity
~~~

The mechanism, measured against the published v0.1.2 binary:

`ExactIdentityValidator::new(member_id, expected)` configures
`args: Vec::new()` — the candidate is executed with **no arguments**
(`eggup-core-0.1.2/src/candidate.rs:250-256`, `309-330`). It then requires

- `stdout == b"cargo-cleanme 0.1.2\n"`, and
- `stderr` empty.

With no arguments, `cargo-cleanme` performs its **default routine scan**:

~~~text
$ ./cargo-cleanme-x86_64-unknown-linux-gnu > out 2> err   # the published v0.1.2 binary
exit=0
stdout: "  26.15 GiB /home/sugarwookie/projects/codegg/crates/..."
stderr: "6 filesystem diagnostics; rerun with a bounded root if needed"
stdout == expected identity: False
stderr empty:             False
validator verdict:         REJECT
~~~

So the identity check compared a machine-wide scan report against a version
string, while also **scanning the user's entire filesystem** on the way to
failing. Two defects in one invocation: it could never match, and it had a large
side effect where a bounded `--version` call belongs.

With `--version`, the same published binary satisfies the validator exactly:

~~~text
$ ./cargo-cleanme-x86_64-unknown-linux-gnu --version > v.out 2> v.err
exit=0
stdout: b"cargo-cleanme 0.1.2\n"
stderr empty:            True
stdout == expected identity: True
validator verdict:        PASS
~~~

## 3. Why the tests never caught it

The commit-path fixture's candidate stub was:

~~~sh
#!/bin/sh
echo 'cargo-cleanme 0.1.2'
~~~

It ignored argv. Any invocation produced the identity string, so the fixture
could not distinguish a correct `--version` invocation from the argv-less one
production actually performed. All seven commit-path tests were green while the
feature was non-functional — the same shape C011, C012, and C013 each paid for,
third time, in the same subsystem.

The C013 audit recorded this fixture as "correct — exact identity string" and
missed the blindness, because the audit asked whether the *assertion* was
meaningful and not whether the *fixture could fail for the right reason*. That is
the gap: a fixture that accepts every input asserts nothing about the input.

## 4. Severity: high, with a safety caveat

- The advertised feature has never worked, in any published version. `0.1.1` and
  `0.1.2` both ship an updater that always fails at the identity check.
- The failure is **safe**: Eggup refuses to commit, the live binary is left
  untouched, and the error names the reason. No wrong bytes, no partial state,
  no deletion.
- It is not silent: `update` exits non-zero with a specific message, so a user
  learns the feature does not work.
- The side effect is the part that is not safe by accident: the failed identity
  check ran a full routine scan of the user's machine. It read the filesystem and
  printed paths; it did not write or delete anything.

High because a shipped, documented, security-reviewed feature does not work at
all. Not critical: nothing is corrupted and the failure is loud.

## 5. Root cause

A one-line omission in `src/update.rs`. `ExactIdentityValidator` needs literal
argv; the call site never supplied it, and `ExactIdentityValidator::new` defaults
to no arguments rather than to a version query. Nothing in the type system, the
API, or the error message indicated that the default was wrong.

## 6. Required work

1. Supply `--version` to the validator.
2. Make the commit-path candidate stub argv-sensitive: print the identity for
   `--version` only, and write to stderr and exit non-zero for anything else, as
   the real binary does.
3. Keep a regression test that fails if the argv is ever dropped again. The
   strict stub is what makes this possible; the existing commit-path tests are
   the assertion.
4. Add a test that the identity check does not perform a filesystem scan, by
   asserting the candidate is invoked with a bounded argument. The argv-sensitive
   stub covers this once production drops `--version`, because the stub's
   no-argument branch is the scan-shaped branch.

## 7. Applied in this working tree

Not yet released. The fix and the strict stub are applied and gated locally:

~~~text
cargo test --lib update::        # 25 passed
~~~

Premise-negative evidence that the fixture can now detect the defect — the
product fix reverted, the strict stub kept:

~~~text
test update::tests::a_verified_candidate_replaces_the_live_binary ... FAILED
test update::tests::staging_is_cleaned_up_on_success_and_on_failure ... FAILED
update commits: Transaction { detail: "the candidate did not identify itself as
  cargo-cleanme 0.2.0\ncandidate execution failed: member cargo-cleanme did not
  produce the exact expected identity" }
test result: FAILED. 23 passed; 2 failed
~~~

With the fix restored, all 25 pass. The pre-fix tree would not have gone red,
which is the entire point.

## 8. What is still required to close

The live rehearsal cannot pass until a *published* release carries the fix. Both
`v0.1.1` and `v0.1.2` are immutable and both are broken, so:

1. bump to `0.1.3`, changelog, and the usual release-preparation gate;
2. tag, dispatch Eggpack, qualify, validate, publish GitHub then crates.io;
3. re-run `scripts/post-release-smoke.sh --from 0.1.2 --to 0.1.3` — the
   *published* 0.1.2 binary is the broken one, so a 0.1.2 -> 0.1.3 rehearsal
   still exercises the defect and must fail. The rehearsal that proves the fix
   is `--from 0.1.3 --to 0.1.4`, or an equivalent where the installing binary
   carries the fix;
4. dispatch the five-target post-release smoke workflow.

Step 3 is an important consequence: a 0.1.2 -> 0.1.3 rehearsal is expected to
fail by design, and that failure is itself worth recording as evidence that the
published 0.1.2 updater is broken.

## 9. What must not be done

- Do not replace `v0.1.2` bytes. crates.io versions and GitHub release assets are
  immutable; the fix ships as a new version.
- Do not yank `v0.1.2`. The 0.1.2 binary's scan, clean, config, and reporting
  behavior is correct, and its `update` fails loudly without mutating anything.
  Yanking would misdescribe a release that is safe to install. Annotate instead.
- Do not close C014 on the strength of publication. C014's own failure semantics
  forbid it, and the rehearsal is a C014 acceptance criterion.

## 10. Verification commands

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 packaging/tests/test_installers.py
python3 packaging/tests/test_installers.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-release-identity.py --self-test
bash scripts/release-check.sh v0.1.3
bash scripts/post-release-smoke.sh --target <host triple> --from 0.1.3 --to 0.1.4
~~~

## 11. Acceptance criteria

C016 closes only when:

- a real published release's updater completes a real self-update against a
  newer public release, with the post-update binary digest equal to the
  published asset digest;
- `update --dry-run` afterwards reports already-current;
- a Cargo-managed installation still refuses, with unchanged bytes;
- the premise-negative in §7 is reproduced from the closing tree;
- hosted Linux/macOS/Windows CI is green on the release commit;
- the published `0.1.2` updater is recorded as broken, not as qualified.

## 12. Closure evidence

Record the source revision, tag, both authorities' identifiers, the five-target
smoke results, the before/after identity-check output, the premise-negative, the
CI run ids, known limitations, and the disposition of `v0.1.1` and `v0.1.2` as
immutable releases carrying a non-functional updater.
