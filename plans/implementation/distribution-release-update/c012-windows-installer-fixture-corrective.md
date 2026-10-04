# C012 — Windows Installer-Fixture Corrective

Status: closed

Repository baseline: `67e98a31ab6b854855d15c6512995808752f17ce`

Closure: `plans/closure/distribution-release-update/c012-status.md`

Original plan: `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`

Original closure record: `plans/closure/distribution-release-update/010b-status.md`

Source roadmap: Phase 10 — Distribution, release, and update

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Implementation commit: `ab4b080` (`test: the Windows installer lane was green for
the wrong reason`)

Related: C009 (the earlier Windows test-fixture portability corrective),
C011 (the updater transport corrective).

## 1. Objective

One installer fixture case — "Cargo reports success but produces no binary is a
hard failure" — had never actually run on the hosted Windows lane. It reported
green for the entire pre-release period while testing nothing.

This corrective makes the case real on Windows and, more importantly, makes the
case assert its own premise so the failure mode cannot recur silently.

## 2. How the defect presented

CI run `37234522042`, on commit `67e98a3`, failed:

~~~text
FAIL [powershell] a Cargo run that produces no binary is a hard failure: exit 0
~~~

A commit that changed only `plans/**` cannot break an installer test. That
mismatch is the whole diagnosis: the case had been passing for an unrelated
reason, and something in the world changed.

## 3. The two defects

### C012-F1 — the fake cargo stub is not invocable on Windows

`fake_cargo` wrote an extensionless file named `cargo` whose contents were
`#!/bin/sh\nexit 0\n`. `install.ps1` locates cargo with `Get-Command cargo`
and invokes it with `& cargo`, both of which resolve through **PATHEXT**
(`.COM;.EXE;.BAT;.CMD;…`). An extensionless file is not in that list.

So on Windows the stub was invisible: `Get-Command cargo` skipped it and found
the real `cargo.exe` later on PATH. The case was not exercising
`install.ps1`'s fallback logic at all — it was exercising a real
`cargo install cargo-cleanme`.

### C012-F2 — the PATH was merged with the wrong separator

The case built its PATH as:

~~~python
{"PATH": f"{stub_dir}:{os.environ.get('PATH', '')}"}
~~~

`:` is the POSIX separator. On Windows the separator is `;`, so this glued the
stub directory and the inherited PATH into a single non-existent entry, and
left the real cargo reachable through the remainder.

This is the same defect as C009 in kind: a POSIX assumption embedded in a
fixture that runs on a platform it was not written for.

## 4. Why it was green for so long

Before `cargo-cleanme 0.1.0` existed on crates.io, the real `cargo install` that
C012-F1 and C012-F2 let through **failed**. `install.ps1` exited non-zero, no
binary was placed, and the assertion `result.returncode == 0 or
installed_binary(dest).exists()` evaluated false — so the case passed.

The pass condition was an unrelated failure. The case would have failed just as
happily if the wrapper were completely broken, because a real failing cargo
produced the same observable outcome as a correct wrapper rejecting an empty
Cargo run.

The moment 0.1.0 was actually published, the real `cargo install` began
succeeding, a binary was produced, the installer exited 0, and the case turned
red. **The defect was disclosed by the product working correctly.**

A green test whose pass condition is an unrelated failure is worse than a
missing test, because it reports coverage that does not exist and suppresses the
signal that would have found the next defect in the same area.

## 5. Resolution

1. `fake_cargo` writes `cargo.cmd` containing `@exit /b 0` when
   `os.name == "nt"`, and the existing `#!/bin/sh` script otherwise. `cargo.cmd`
   is what PATHEXT resolution finds, and `@exit /b 0` is the batch form of a
   successful exit. `.cmd` rather than `.bat` because `cmd.exe` never
   auto-executes a bare `.bat` from a path lookup.
2. The PATH merge uses `os.pathsep` instead of a literal `":"`.

With the stub genuinely reached, `install.ps1` takes the real branch under test:
`& cargo` succeeds, `$LASTEXITCODE` is 0, the expected binary is absent, and
the wrapper's own `Stop-Install "Cargo reported success but $source was not
produced"` fires. That is the behavior the case was written to require.

## 6. The regression guard: assert the premise

The real lesson is not the shim's extension. It is that the case's premise —
"the cargo the wrapper finds is the stub" — was assumed rather than checked.

A new case, `case_fake_cargo_is_actually_resolved`, runs before the fallback
cases and probes the host the same way the wrapper will:

- POSIX: `command -v cargo`
- Windows: `(Get-Command cargo).Source`

It fails if no cargo resolves at all, and fails if the resolved cargo's parent
directory is not the stub directory. With the pre-fix stub, this fails loudly on
Windows instead of quietly testing nothing.

Every fixture case that stubs an external tool should carry this kind of
premise check. A stub that the subject cannot invoke is indistinguishable from
no stub at all, and the case will still produce a pass/fail verdict.

## 7. Out of scope

- Changing `install.ps1`. The wrapper behaved correctly throughout; the defect
  was entirely in the harness. The wrapper's own guard, the `Test-Path` check
  after a successful `cargo install`, is exactly the behavior the case was
  supposed to be proving, and it is now actually being proven on Windows.
- Changing any assertion's expected outcome.
- Re-qualifying the published releases. The defect was in a test, not in
  `install.ps1`, so v0.1.0 and v0.1.1 ship a correct installer. Nothing about
  the published artifacts is wrong.

## 8. Verification

Local, POSIX lane:

~~~sh
python3 packaging/tests/test_installers.py
# 18 passed, 0 failed
~~~

The PowerShell block is gated on `os.name == "nt"` and is only exercised by the
hosted Windows lane. That is the evidence for this corrective, and it is not
claimed from a Linux host.

Hosted:

~~~sh
gh run view <run-id> --json jobs --jq '.jobs[] | select(.name=="installers (windows-latest)")'
~~~
