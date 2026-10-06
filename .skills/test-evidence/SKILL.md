---
name: test-evidence
description: Proving a cargo-cleanme change works — the false-green problem, fixture guards, self-tests, and how to write a test that can actually fail
version: 1.0.0
tags:
  - testing
  - evidence
  - fixtures
  - verification
---

# Test Evidence

This repository's single most expensive recurring lesson:

> **A green test proves only that its own premises hold.**

Seven milestones closed on a test that was passing for the wrong reason. Three
product defects (C015, C016, C017) were found by the project's own evidence
work, and **none** was repaired under the plan that found it. The plan that
found a defect does not fix it; it writes a corrective.

Reference: `architecture/14-testing-and-verification.md`, and
[`plans/registry.md`](../../plans/registry.md) § *The lesson this repository
keeps re-learning*.

## When to Load

- Writing or reviewing any test
- Choosing what a closure record may claim as evidence
- A reviewer's pass over a diff
- Before asserting in a plan or record that something is verified

## The three ways a test goes green without testing anything

1. **A fixture that cannot run where it reports success.** A POSIX-only fixture
   passing on no platform. Guarded by
   `scripts/check-fixture-portability.py`, which every gate runs.
2. **A stub that is not the tool the subject resolves.** If the subject invokes
   a real binary through `PATH` and the test substitutes a differently-named
   shim, the test is indistinguishable from no stub — and it is green only
   because the real tool failed for an unrelated reason. This is exactly how the
   Windows installer case was green while its subject failed. Guarded by
   `packaging/tests/test_installers.py --self-test`.
3. **A case whose pass condition is an unrelated failure.** Asserting "the
   command failed" when what the test needs is "this specific thing failed". The
   command failing for another reason satisfies the assertion. Prefer a
   discriminating assertion: the specific reason code, the specific diagnostic,
   the specific message.

Two more that recur:

4. **A tautological test.** Asserting that a sort produced sorted output, by
   checking the comparator. Assert that the code under test called the mock
   without checking anything about the result.
5. **A guard that stopped guarding.** A checker with no negative sample reports
   green forever. Every guard in `scripts/` and `packaging/tests/` therefore
   takes a `--self-test` that feeds it a known-bad sample and requires a
   failure. If you add a guard, the self-test is not optional.

## The worked example worth remembering

C013's own test **could only be failed, never passed**. It asserted a property
of the code that was true for every input, so a regression would have gone
unnoticed while the test stayed green. The test asserted "the port is in
range"; the test needed was "the process really was the one under test".

Generalised: before trusting a test, ask **"what input makes this fail?"** If
you cannot produce one, the test is decoration. `cleanup.rs` contains an
`all_dryrun_modes_emit_zero_clean_spawns` test whose comment is worth reading
before writing anything in that module.

## What only a subprocess can prove

Five modules have **zero** inline tests: `main.rs`, `output.rs`, `domain.rs`,
`error.rs`, `lib.rs`. That is a real gap, not an oversight, and it is deliberate
in part: the composition, argv, and exit-code paths are only observable from
outside the process.

So:

| Claim | Needs |
|---|---|
| a module's internal logic | an inline `#[test]` |
| argv normalization, exit code, stdout/stderr split, one-JSON-object | a `#[test]` in `tests/cli_contract.rs` or `tests/end_to_end.rs` |
| an installer, transport, or toolchain version | the fixture suite / external smoke |

Do not write an inline `#[test]` for `main.rs` composition. Add a subprocess
case, and note that the gate only has 2 pinned exit codes; widening that is
valuable.

## Coverage shape, honestly

320 inline `#[test]` functions are **declared**; **318 compile and run on Linux**.
Two are excluded there: one `#[cfg(windows)]`
(`config.rs:930`) and one `#[cfg(any(target_os = "macos", windows))]`
(`discovery.rs:1394`). A further 21 carry `#[cfg(unix)]`, 3 carry
`#[cfg(target_os = "linux")]` or an `all(linux, …)` form, so the three hosted
lanes compile **318 of 320 on Linux**, **296 of 320 on Windows**, and **317 of 320
on macOS** — `#[cfg(unix)]` is true *on* macOS, which is the half people get
wrong. A green local run is not a green run elsewhere — that is what the OS
matrix in `ci.yml` is for, and why `check-fixture-portability.py` exists to catch
a *new* ungated fixture before a lane disagrees about it.

State which number you mean. "320 tests" (declared) and "318 passing" (on Linux)
are both true, and a record that gives one while meaning the other is ambiguous in
the direction that matters.

Of the 320, **40.3% live in two modules** (`cleanup.rs` 72, `workspace.rs` 57).
That concentration is a direct consequence of the architecture: cleanup and
workspace resolution are where the invariants are. It is not a balanced suite,
and a flat count would hide that. A change to a small module can be genuinely
under-covered; count the module, not the crate.

The same discipline applies to documentation. `architecture/` cites source by
line, so a fix that adds or removes lines silently invalidates every citation
past that point while the prose keeps reading as true. This happened: a commit
added 38 lines to `main.rs` and 55 to `traverse.rs`, and the deep dives went on
citing the old numbers. `scripts/check-doc-citations.py` now gates the
mechanically decidable half. The semantic half — whether the line still says
what the sentence claims — is a reviewer's job, and no tool can do it.

## The verification ladder

Run in this order; stop at the first failure and do not report the rest.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
```

For a full pre-release gate use `scripts/release-check.sh`, which adds the
completions/manpage drift check, the contract checkers, `cargo package
--locked`, and `cargo publish --locked --dry-run`.

**Report what actually ran.** If a command was skipped, or a suite is red for an
unrelated reason, say so. A closure record that claims a gate passed when it was
not run is worse than no record, because the next milestone trusts it.

## Before you claim verification

- You can name the input that makes each new test fail, and you have seen it.
- Every new guard has a `--self-test` that fails on a known-bad sample.
- If a test stubs a tool, it asserts **which** tool was invoked, not merely that
  the invocation succeeded.
- External-process fixtures prove their own premise — the port is free, the
  binary exists, the fixture is reachable — before asserting the behaviour under
  test. `check-fixture-portability.py` exists because this was skipped.
- You have run the self-tests of the guards you touched, not just the code.

## Related

- [`planning-and-closure`](../planning-and-closure/SKILL.md) — turning this
  evidence into a closure record
- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md)
