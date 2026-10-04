# Artifact Discovery and Cleanup C009 Status

Plan: `plans/implementation/artifact-discovery-cleanup/c009-windows-test-fixture-portability.md`

Disposition: **closed**

Implementation commits: `ab33874` (F1/F2 fix) and `212cb1e` (F3 fix).

Repository baseline: `b4662afdd02a4fdbb6ac065b372ea43836687df6` (M010A implementation commit).

Date: 2026-10-04

## Executive finding

C009 is closed. The hosted Windows lane is green for the first time in the
Phase 9/10 sequence: CI run `37225697173` passed `checks (windows-latest)`,
`checks (ubuntu-latest)`, `checks (macos-latest)`, and `msrv`. All three
findings were test-fixture portability defects. No production Rust file
changed, no cleanup safety behavior changed, and no test was skipped,
deleted, or relaxed.

## Finding-to-correction map

| Finding | Defect | Correction |
|---|---|---|
| C009-F1 | `config::tests::invalid_scan_ignore_glob_is_reported_at_load` hardcoded the POSIX path `/tmp/[unclosed`, which is not absolute on Windows, so the absolute-path check fired before the glob compiler the test targets. | The pattern is derived from the platform temporary directory and the assertion now names the glob-compilation error (`invalid scan.ignore glob`) directly. |
| C009-F2 | `workspace::tests::nested_build_inside_target_is_counted_once` opened directories as files to backdate them, which is permitted on Unix and denied on Windows. | A test-scoped `set_path_modified` helper sets a path's mtime for files and directories on every platform. |
| C009-F3 | `tests/end_to_end.rs` and `tests/cli_contract.rs` carried the identical directory-as-file defect, masked because `cargo test --all-targets` stops after the first failing target. | Both suites now include `tests/common/mod.rs` and backdate through the same helper. |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| F1 asserts glob compilation, not absolute-path rejection | fixture builds the pattern from `tempfile::tempdir()`; assertion is `err.contains("invalid scan.ignore glob")` | passed on all four lanes |
| F2 keeps the inactive-group precondition genuine | `set_path_modified` uses `CreateFileW` with `FILE_FLAG_BACKUP_SEMANTICS` + `FILE_WRITE_ATTRIBUTES` and `SetFileTime` on Windows; the workspace root mtime participates in `traverse::workspace_member_activity` and is really backdated | passed on all four lanes |
| F2 union-measured-once assertion unchanged | no assertion in `nested_build_inside_target_is_counted_once` was altered; only the timestamp mechanism changed | passed |
| F3 both integration suites pass on Windows | run `37225697173` Windows lane: 190 lib + 6 `cli_contract` + 1 `end_to_end` tests, 0 failed | passed |
| No test skipped/deleted/relaxed | no `#[ignore]`, no deletion, no assertion weakening; diff touches only fixture timestamp plumbing | passed |
| No production Rust file changed | `src/config.rs` and `src/workspace.rs` changes are confined to `#[cfg(test)]` code except the `config.rs` test itself, which is also `#[cfg(test)]` | passed |
| No production dependency added | `windows-sys` was already a Windows-only dependency; only the `Win32_Security` feature that `CreateFileW` requires was added | passed |
| Published library surface unchanged | helper lives in the `src/workspace.rs` unit-test module and in `tests/common/mod.rs`; no new `pub` item | passed |
| MSRV preserved | `cargo +1.89 check --locked --all-targets` and `cargo +1.89 test --locked --all-targets` pass | passed |

## Production implementation evidence

- `src/workspace.rs` test module: new `set_path_modified(path, when)`. The
  Windows branch opens the path with
  `CreateFileW(FILE_WRITE_ATTRIBUTES, FILE_SHARE_READ|WRITE|DELETE,
  OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL|FLAG_BACKUP_SEMANTICS)` and applies
  `SetFileTime` for the write time only, closing the handle on every path.
  `FILE_WRITE_ATTRIBUTES` is the minimum right that permits setting a
  timestamp and the one right a directory handle can be granted. The
  non-Windows branch keeps `File::set_modified` for files and uses
  `OpenOptions::write(true)` for regular files.
- `backdate_tree` and the F2 fixture route through the helper. `backdate_tree`
  keeps its lenient `let _ =` behavior so an unreadable entry still cannot fail
  an unrelated fixture.
- `tests/common/mod.rs` provides the same helper plus a `backdate_file` that
  panics with the offending path, so a future Windows fixture failure names the
  file instead of failing obscurely.
- `tests/cli_contract.rs` now backdates `target` and `target/debug` explicitly;
  previously it backdated only `target`, relying on the directory mtime that
  could not be set on Windows. The file's own comment already recorded that the
  workspace directory mtime is source activity.
- `Cargo.toml` Windows dependency comment records why `Win32_Security` is
  required.

## Scope revision recorded during implementation

The plan scoped the helper to a single test location. C009-F3 disproved that
scope: integration tests cannot observe `#[cfg(test)]` items, so the same
capability was needed in a second location. The invariant that actually
mattered — do not widen the published library surface — was preserved by
providing the helper in exactly two test-scoped copies rather than introducing
a `#[doc(hidden)]` public module. The plan file records this revision.

## Why the previous verification missed these

- F1: the fixture was written in terms of a POSIX-only path literal, so it is
  green wherever POSIX paths are absolute.
- F2: every other fixture in the file ignores the `set_modified` error with
  `let _ =`, so the Windows no-op was invisible; only the F2 fixture unwrapped,
  converting a silent no-op into a visible panic.
- F3: `cargo test --all-targets` stops after the first failing test target, so
  while the `lib` target failed on Windows the two integration binaries never
  ran. Expect further Windows fixture findings to arrive one target at a time.

## Regression evidence required to prevent recurrence

- Any new fixture that needs a backdated timestamp must use
  `set_path_modified` / `common::backdate_file`, never `File::open` on a
  directory.
- `cargo test --all-targets` must be the qualification command, never
  `cargo test --lib`, when the question is "does this pass on Windows".
- The Windows lane is the regression gate; it is green at run `37225697173` and
  any future red lane is a new finding, not an expected baseline.

## Verification

All local commands passed on commit `212cb1e`:

- `cargo fmt --check` — pass.
- `cargo clippy --all-targets --all-features -- -D warnings` — pass.
- `cargo clippy --target x86_64-pc-windows-msvc --all-targets --all-features -- -D warnings` — pass.
- `cargo check --target x86_64-pc-windows-msvc --all-targets` — pass; this is
  the only local evidence available for the Windows FFI branch.
- `cargo test --all-targets --all-features` — pass, 203 lib + 7 `cli_contract`
  + 1 `end_to_end` tests.
- `cargo +1.89 check --locked --all-targets` — pass.
- `cargo +1.89 test --locked --all-targets` — pass, 203 lib + 7 + 1 tests.

Hosted CI run `37225697173` on commit `212cb1e`: `checks (windows-latest)`,
`checks (ubuntu-latest)`, `checks (macos-latest)`, and `msrv` all passed.
Hosted release drift guard run `37225697157` passed.

## Disposition of the pre-M010A failures

Baseline run `37223413669` on pre-M010A commit `78aac6d` and M010A run
`37224962781` both failed `checks (windows-latest)` with F1 and F2. The first
C009 push, run `37225436204`, failed with F3. Run `37225697173` is green on all
four lanes. The historical red runs are preserved unchanged; this record is
the corrective evidence that closes them.

## Known limitations

- The Windows FFI branch of `set_path_modified` cannot be executed locally; its
  only execution evidence is the hosted Windows lane. The Windows-target
  compile check proves it builds, not that it behaves.
- A very old NTFS setting could in principle reject a backdated directory
  timestamp; the helper surfaces that as an error, and the affected fixtures
  fail loudly rather than silently changing meaning.
