# C009 — Windows Test-Fixture Portability Corrective

Status: ready

Repository baseline: `b4662afdd02a4fdbb6ac065b372ea43836687df6` (M010A implementation commit)

Source roadmap: Phase 10 — Distribution and operational polish

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Related: `plans/closure/distribution-release-update/010a-status.md` (finds the defect), CI runs `37223413669` (baseline `78aac6d`) and `37224962781` (M010A).

## 1. Objective

Restore a green hosted Windows lane. Two pre-existing unit tests pass on Linux
and macOS and fail on Windows for platform reasons unrelated to the behavior
they assert, so no hosted Windows evidence is currently obtainable and every
Phase 10 milestone that requires per-platform closure evidence is blocked on
it.

## 2. Current evidence

Both failures reproduce identically on the pre-M010A baseline `78aac6d` and on
the M010A implementation commit, so neither is a distribution regression.

### Finding C009-F1 — `config::tests::invalid_scan_ignore_glob_is_reported_at_load`

The fixture writes `ignore = ["/tmp/[unclosed"]` and asserts the load error
mentions `scan.ignore`. `/tmp/[unclosed` is a POSIX absolute path but **not** a
Windows absolute path, so on Windows the absolute-path check fires first and
the observable error becomes `ignore pattern must be absolute`, never reaching
the glob compiler the test is about.

Why previous verification missed it: the assertion is written in terms of a
POSIX-only path literal, so it is green wherever POSIX paths are absolute.

### Finding C009-F2 — `workspace::tests::nested_build_inside_target_is_counted_once`

The fixture backdates the `target`, `build`, and workspace-root directories with

~~~rust
std::fs::File::open(directory).unwrap().set_modified(old).unwrap();
~~~

Opening a *directory* as a file is permitted on Unix and denied on Windows, so
the test panics with `PermissionDenied` at `src/workspace.rs:2418` before any
assertion runs.

This is not only a panic. `traverse::workspace_member_activity` includes the
member root directory entry in its recency verdict, so the backdated workspace
root is load-bearing for the "inactive group" precondition. Silently dropping
the `set_modified` call (the pattern other fixtures use with `let _ =`) would
let the root look freshly written, and the test would start passing or failing
for the wrong reason. The directory mtime must actually be set.

Why previous verification missed it: every other fixture in the file ignores
the `set_modified` error with `let _ =`, so a Windows no-op was invisible; only
this fixture unwrapped, converting the silent no-op into a visible panic.

### Finding C009-F3 — the same defect in both integration suites

`tests/end_to_end.rs::backdate` and the `cli_contract` fixture both apply the
identical `fs::File::open(directory).unwrap().set_modified(old).unwrap()`
pattern to the workspace root, `src`, `target`, and `target/debug`. The
`cli_contract` fixture already carried a comment stating that the workspace
directory's mtime is source activity and must be backdated, so the requirement
was understood; only the platform capability was missing.

This finding was **masked**, not absent. `cargo test --all-targets` stops
after the first failing test target, so while the `lib` target failed on
Windows the two integration binaries never ran. Fixing F1 and F2 turned the
lane far enough to expose F3. Any future Windows-fixture failure in this
repository should be expected to arrive in batches of one per target.

Why previous verification missed it: fail-fast target ordering hid the
integration suites behind the library test failure.

## 2a. Scope revision

The original plan scoped the helper to the `src/workspace.rs` unit-test
module on the grounds that only that call site failed. F3 disproves that:
integration tests cannot see `#[cfg(test)]` items, so the same platform
capability is required in a second location.

The helper is therefore provided in exactly two test-scoped copies: one in the
`src/workspace.rs` unit-test module and one in `tests/common/mod.rs`, included
by each integration suite with `mod common;`. The invariant is unchanged: no
production item is added, and the published library surface is not widened. A
single shared helper would have required a `#[doc(hidden)]` public module,
which is a worse trade than a contained duplicate.

## 3. Invariants

- No production behavior changes. Both findings are in test fixtures.
- No cleanup eligibility, ownership, proof, or destructive boundary changes.
- The activity/precondition semantics the C009-F2 test asserts must be preserved
  on every platform, not weakened to accommodate Windows.
- No new production dependency. `windows-sys` is already a Windows-only
  dependency with the `Win32_Storage_FileSystem` feature.
- Any helper introduced for this purpose stays test-local; the published
  library surface is not widened.

## 4. Production changes

None. The corrective is confined to test code under `#[cfg(test)]`.

## 5. Ordered work packages

1. Add a test-scoped `set_path_modified` helper that sets a path's modification
   time for files *and* directories on every platform. On Windows it must open
   the path with `FILE_FLAG_BACKUP_SEMANTICS` and `FILE_WRITE_ATTRIBUTES` and
   call `SetFileTime`; on other platforms it keeps the current `set_modified`
   behavior.
2. Provide the helper in the two test scopes that need it: the
   `src/workspace.rs` unit-test module and a `tests/common/mod.rs` module
   included by each integration suite.
3. Route `backdate_tree`, the C009-F2 fixture, and both integration fixtures
   through the helper, keeping the existing lenient behavior for files where no
   special handle is required.
4. Make the C009-F1 fixture's invalid glob path absolute on every platform by
   deriving it from the platform temporary directory instead of hardcoding a
   POSIX literal, and keep asserting the glob-compilation error.
5. Run the full local gate plus a Windows-target compile check.
6. Obtain green hosted Windows, Linux, macOS, and Rust 1.89 evidence, allowing
   for the fail-fast masking described in C009-F3.

## 6. Compatibility effects

None. The MSRV, CLI, JSON schema, and release contract are untouched.

## 7. Required tests

The two named tests must pass on all four CI lanes. No test may be deleted,
`#[ignore]`d, or relaxed to make the lanes green.

## 8. Verification commands

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --target x86_64-pc-windows-msvc --all-targets
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
~~~

Plus the hosted `checks (windows-latest)` lane on the implementation commit.

## 9. Documentation updates

None required. The finding is recorded in the C009 closure record and
referenced from the M010A closure record.

## 10. Acceptance criteria

- `config::tests::invalid_scan_ignore_glob_is_reported_at_load` passes on
  Windows and still proves an invalid `scan.ignore` glob is reported at load.
- `workspace::tests::nested_build_inside_target_is_counted_once` passes on
  Windows and still proves the nested union is measured once, with the
  backdated-inactive precondition genuinely established.
- `tests/end_to_end.rs` and `tests/cli_contract.rs` pass on Windows, proving
  C009-F3 is closed rather than merely un-masked.
- Hosted `checks (windows-latest)` is green.
- No production Rust file changed; `Cargo.toml` gains only the `Win32_Security`
  feature that `CreateFileW` requires.

## 11. Stop conditions

Stop and record a finding rather than:

- skipping, deleting, or `#[ignore]`ing either test;
- weakening either assertion;
- replacing the backdated-directory precondition with a future-clock shortcut,
  which would make the test stop exercising the inactivity path;
- adding a production dependency or a public test-support module.

## 12. Closure evidence

Record the implementation commit, the exact helper used per platform, the two
test outcomes, the Windows-target compile check, and the hosted run id that
closes the Windows lane.
