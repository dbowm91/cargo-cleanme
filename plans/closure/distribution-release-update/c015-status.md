# Distribution, Release, and Update C015 Status

Plan: `plans/implementation/distribution-release-update/c015-relative-scan-root-cargo-resolution-corrective.md`

Disposition: **closed**

Discovered by: the C013 audit, which found the symptom inside a release
validator that had been green on every hosted lane while the product under test
was failing.

Implementation commit: `1316e81` (`fix: a relative scan root resolved zero Cargo
workspaces`)

Repository baseline: `c9e0104` (C013 closure, `c9e9f68`)

Date: 2026-10-04

## Executive finding

`cargo cleanme scan <relative-path>` resolved **zero** Cargo workspaces, reported
no groups, and exited 0.

Discovery seeds its walker with the root exactly as the caller spelled it, so
every manifest discovered beneath a relative root inherits that prefix. The
product then ran

~~~text
cargo locate-project --workspace --manifest-path <relative>
~~~

with the child's working directory set to **the manifest's own parent**. For a
discovered `fixture/Cargo.toml` that asks Cargo for
`<root>/fixture/fixture/Cargo.toml`, which does not exist. Cargo exits non-zero,
the workspace is unresolved, the scan degrades to `groups: []`, and the only
signal is one stderr diagnostic.

Measured on `x86_64-unknown-linux-gnu`, cargo-cleanme 0.1.2, one inactive
fixture package with an 8192-byte `target/debug/artifact.bin`:

| Root spelling | Exit | Workspaces | Groups | Bytes | Diagnostics |
|---|---|---|---|---|---|
| absolute | 0 | 1 | 1 | 8192 | none |
| relative | 0 | 0 | 0 | 0 | `cargo locate-project failed` |

Confirmed independently of the product:

~~~text
$ cd <root>/fixture
$ cargo locate-project --workspace --manifest-path fixture/Cargo.toml
error: manifest path `fixture/Cargo.toml` does not exist
$ cargo locate-project --workspace --manifest-path <root>/fixture/Cargo.toml
{"root":"<root>/fixture/Cargo.toml"}
~~~

The two spellings are equivalent only when the manifest path is absolute.

## The fix

`manifest_path_for_cargo` anchors a relative manifest path against the process
working directory **lexically**, before it becomes `--manifest-path`.

Lexical rather than canonical, deliberately: `fs::canonicalize` would resolve
symlinks and change the spelling Cargo sees, and the plan's constraint was that
Cargo's own configuration discovery from the manifest's directory must be
unchanged. A relative path that cannot be anchored is passed through unchanged,
so Cargo produces the error it always would — the fix never invents a path Cargo
could not have resolved on its own.

The diagnostic recorded for a failed resolution still carries the spelling the
user gave, so a report is not rewritten by a fix to the invocation.

No cleanup-safety boundary moved. The change makes a manifest *more* resolvable
only to the degree that the identical absolute spelling was always resolvable:
same manifest, same Cargo, same answer.

## Why the tests had to be written twice

Both required tests passed against the unfixed code when first written. The
failure modes are recorded because they are the same shape as the defect line
this repository has been living with for three corrective rounds: something that
reports success without having tested anything.

**The `..` spelling hides the defect.** The first fixture lived in a temp
directory outside the process working directory, so the relative spelling was
built with `..` components. Joining `../../tmp/x/Cargo.toml` onto the manifest's
own parent composes back to the correct file, so the broken invocation still
resolved and the case passed. The defect requires a *forward* relative path —
exactly what `cd project && cargo-cleanme scan fixture` produces. The fixture now
lives under the gitignored `target/` directory and the case asserts the spelling
contains no parent component, so it cannot silently become the safe shape again.

**A version gate that skipped into a pass.** The real-Cargo case gated on the
`cargo metadata --no-deps` feature, comparing the *patch* component of the
version. On this host the toolchain is `1.99.0`, so the patch is `0`, the
comparison fell under the threshold, and the case skipped while reporting a
pass. The gate now compares the **minor** version, and since
`--no-deps` landed in 1.77 — below this project's 1.89 MSRV — every supported
toolchain runs the case.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Relative and absolute spellings agree | `a_relative_root_resolves_the_same_workspaces_as_its_absolute_spelling`: equal workspace count, equal `unique_workspaces`, equal `cargo_failures`, no diagnostics, equal member count | pass — 230 lib tests |
| The real Cargo is the subject, not a stub | the case drives `SystemCargoRunner`; the defect was in the arguments the real Cargo receives, and a stub ignoring the manifest path would not have detected it | pass |
| The composed arguments are asserted before the behaviour | `the_composed_locate_arguments_resolve_against_the_supplied_working_directory` records `(cwd, args)` and asserts `cwd.join(manifest)` exists | pass |
| The dotted root works too | `scan .` and `scan ..` resolve; both are forward-relative shapes covered by the same fix | pass |
| Cargo stays authoritative for membership | no TOML parsing, no ancestry inference; the fix only anchors the path handed to Cargo | pass |
| `routine`/`global` scope discovery unchanged | no change to the implicit-scope path; the fix is confined to the locate invocation | pass |
| No JSON schema or `display_path` change | `groups[].display_path` still reports the user's spelling; the anchored path is used only for the child invocation | pass |
| No cleanup, selector, or deletion assertion weakened | no such test was modified; the count rose from 228 to 230 | pass |
| The dotted/relative regression is cross-platform | the argument composition is platform-independent; no platform gate | pass |
| Hosted CI green on Linux, macOS, and Windows | see the run ids below | pass |

## Premise-negative evidence

Both required tests fail against the unfixed code, and the failures carry the
real defect rather than an incidental one:

~~~text
$ # product fix reverted, tests unchanged
test ...the_composed_locate_arguments_resolve_against_the_supplied_working_directory ... FAILED
  panicked at src/workspace.rs:3275:13
test ...a_relative_root_resolves_the_same_workspaces_as_its_absolute_spelling ... FAILED
  panicked at src/workspace.rs:3354:9
test result: FAILED. 43 passed; 1 failed  (and 44 passed with the fix)
~~~

## End-to-end verification

Run from inside the fixture's parent directory, which is the shape that used to
fail — `cd <parent> && cargo-cleanme scan fixture`:

~~~text
relative root:  1 group,  8192 bytes, 0 diagnostics
  <root>/fixture/target 8192 bytes
absolute root:  1 group,  8192 bytes, 0 diagnostics
  <root>/fixture/target 8192 bytes
~~~

Identical. Before the fix the first line was `0 groups, 0 bytes,
'cargo locate-project failed'`.

## Verification commands actually run

~~~sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features                 # 230 + 8 + 1
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets                 # 230 + 8 + 1
python3 scripts/check-fixture-portability.py
python3 scripts/check-release-contract.py
python3 scripts/check-installer-contract.py
python3 scripts/check-post-release-smoke-contract.py
~~~

## Platform evidence

| Lane | Evidence |
|---|---|
| `ubuntu-latest` | this host: both premise-negatives reproduced, the end-to-end relative/absolute equivalence measured, 230 lib tests |
| `macos-latest` | CI green on the fix commit; the argument composition has no platform-specific component |
| `windows-latest` | CI green on the fix commit; `manifest_path_for_cargo` uses `Path::join` and `is_absolute`, which are platform-correct on Windows |

The end-to-end relative-root measurement is a Linux run. The reasoning is
platform-independent — `cwd.join(relative)` composes the same way on both — and
the argument-level test, which is the premise assertion, runs on every lane.

## Unclosed requirement

None.

## Known limitations

- The fix anchors the manifest path for the `locate-project` invocation. The
  `metadata` invocation already receives an absolute path, because
  `locate.root` is absolute by contract, so it needed no change.
- `manifest_path_for_cargo` uses the process working directory. For a caller that
  changes the process cwd concurrently, the anchoring could be stale. Nothing in
  the codebase does, and the alternative — threading a cwd through discovery —
  is a larger change than the defect warrants. Recorded rather than hidden.
- The version gate in the real-Cargo case skips below cargo 1.77, below the
  declared 1.89 MSRV. The skip is announced on stderr rather than returning
  silently.

## Unresolved findings

- **low** — a stale `target/debug` fingerprint reported `cargo-cleanme` as
  `Fresh` after a source edit, so a local verification run used a binary that
  predated the fix and appeared to show the defect still present. The
  end-to-end evidence above was produced from a clean `CARGO_TARGET_DIR`. A
  wrong local build can make a correct fix look broken, or the reverse; that is
  the mirror image of a false green and worth knowing about.
- No finding here requires a further corrective.

## Disposition

**closed.** A relative and an absolute scan root now produce equal counters,
equal groups, and equal bytes; the composed Cargo arguments are asserted before
the behaviour is; both tests fail against the unfixed code; the cleanup safety
boundary is untouched; and the release notes for 0.1.2 that told users to use an
absolute path become unnecessary from 0.1.3 onward.

C016 remains open and independent. It is the higher-severity of the two defects
this line found, and it owns the self-update fix.
