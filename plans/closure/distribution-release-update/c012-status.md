# Distribution, Release, and Update C012 Status

Plan: `plans/implementation/distribution-release-update/c012-windows-installer-fixture-corrective.md`

Disposition: **closed**

Corrects: `plans/closure/distribution-release-update/010b-status.md`, whose
installer fixture matrix recorded this case as passing on all three hosted
platforms when it had not in fact executed on Windows.

Implementation commit: `ab4b080` (`test: the Windows installer lane was green
for the wrong reason`)

Repository baseline: `67e98a31ab6b854855d15c6512995808752f17ce`

Date: 2026-10-04

## Executive finding

One installer fixture case had never run on the hosted Windows lane. It
reported green on every CI run for the entire pre-release period while testing
nothing, and it was disclosed only because the product started working.

The harness's fake `cargo` was an extensionless `#!/bin/sh` file. PowerShell
resolves `cargo` through PATHEXT, so the stub was invisible and the real
`cargo.exe` was used instead. The PATH was also merged with `:` instead of
`os.pathsep`, which on Windows left the real cargo reachable. The case therefore
exercised a real `cargo install cargo-cleanme`, not `install.ps1`.

It passed anyway, because before 0.1.0 was published that real `cargo install`
failed. A real failing cargo and a correct wrapper rejecting an empty Cargo run
produce the same observable outcome — non-zero exit, no binary placed — so the
assertion was satisfied by an unrelated failure. When 0.1.0 reached crates.io,
the real install started succeeding, the installer exited 0, and the case
turned red on a commit that had changed only `plans/**`.

Nothing shipped is wrong. `install.ps1` behaved correctly throughout; the defect
was entirely in the test harness. v0.1.0 and v0.1.1 ship a correct installer.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| The fake cargo is invocable by the wrapper under test | `fake_cargo` writes `cargo.cmd` with `@exit /b 0` on `os.name == "nt"`; `install.ps1` invokes cargo via `& cargo`, which resolves through PATHEXT | pass — hosted Windows |
| The case's premise is asserted, not assumed | new `case_fake_cargo_is_actually_resolved` probes `command -v cargo` / `(Get-Command cargo).Source` and fails unless the resolved cargo's parent is the stub directory | pass — 18/18 POSIX locally, green on Windows |
| PATH is merged with the platform separator | `os.pathsep` replaces the literal `":"` | pass |
| The wrapper's own guard is what is now under test | with the stub genuinely reached, `& cargo` succeeds, `$LASTEXITCODE` is 0, the expected binary is absent, and `install.ps1` must fire `Stop-Install "Cargo reported success but $source was not produced"` | pass |
| The outcome assertion is unchanged | `result.returncode == 0 or installed_binary(dest).exists()` is untouched; only the harness reaching it was fixed | pass |
| Nothing in the published releases is wrong | the defect was in `packaging/tests/test_installers.py` only; `packaging/install.ps1` was not modified | pass |

## Verification commands actually run

~~~sh
python3 packaging/tests/test_installers.py
#   ok   [posix] the cargo stub is the cargo the wrapper will resolve
#   ok   [posix] a Cargo run that produces no binary is a hard failure
#   18 passed, 0 failed

python3 -c "import ast; ast.parse(open('packaging/tests/test_installers.py').read())"
python3 scripts/check-installer-contract.py
cargo fmt --all -- --check
cargo test --all-targets      # 228 + 7 + 1 passed
~~~

## Platform and fixture evidence

| Lane | Evidence |
|---|---|
| `ubuntu-latest` | 18/18 installer cases pass, including the new premise case |
| `macos-latest` | green in CI run `37235169841` |
| `windows-latest` | green in CI run `37235169841`; this is the **only** place the PowerShell block executes, and it is the sole evidence for this corrective |

The PowerShell block is gated on `os.name == "nt"`. `pwsh` 7.6.6 is present on
this Linux host but the block does not run there, so the Windows result is
taken from hosted CI and is not claimed from a local run.

## Unclosed requirement

None. The case now executes on every lane it claims to.

## Known limitations

- The premise check proves the *stub* is what cargo resolves to. It does not
  prove the wrapper's branch selection is correct for every platform nuance of
  PATHEXT ordering. That would need a case per extension, which is not
  justified by anything observed.
- The same class of defect — a stub the subject cannot invoke — could exist in
  other fixture suites. The two instances found in Phase 10 (this one and C009)
  were both POSIX assumptions in cross-platform fixtures. A mechanical sweep for
  `#!/bin/sh` stubs and literal `":"` PATH joins outside the installer suite is
  not recorded as done.

## Unresolved findings

- **low** — other fixture suites have not been swept for the same pattern. Named
  rather than dismissed, because naming it is what makes it cheap to pick up.
- No finding here requires a further corrective.

## Disposition

**closed.** The case executes on Windows, its premise is asserted, and the
Windows lane is green.

M010B's closure record is not edited to imply its matrix was correct. It gains
an addendum recording that this row was green without executing, and naming
this corrective as the discharge.
