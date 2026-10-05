# C020 — Windows concurrent first-use config creation can fail closed

Status: ready

Subsystem: artifact discovery and cleanup
Corrects: `config::create_initial` (`src/config.rs:240-302`), first shipped in the
initial configuration work. No prior closure record claimed this path was
race-correct on Windows.

## 1. Problem

`config::create_initial` is written to be safe when several processes or threads
reach a missing config at the same time. It stages a uniquely named temporary
file, and publishes it with a single `fs::hard_link`:

```rust
// Creating the final link is atomic and never overwrites a
// concurrent winner. Both names are siblings on the same volume.
fs::hard_link(&temp, path)?;
fs::remove_file(&temp)?;
```

The design is right: `link(2)` on POSIX fails with `EEXIST` for a second
challenger, so exactly one thread wins and the rest fall through. The published
claim in that comment, "never overwrites a concurrent winner", holds. What does
not hold is the error handling around it, which tolerates exactly one spelling
of "somebody else won":

```rust
if let Err(e) = write_result {
    let _ = fs::remove_file(&temp);
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return Ok(());
    }
    ...
    return Err(AppError::Io(e));
}
```

On Windows the losing `CreateHardLinkW` can report `ERROR_ACCESS_DENIED`
(os error 5) rather than `ERROR_ALREADY_EXISTS`, because the winning file is
still open by the thread that just created it. `ERROR_ACCESS_DENIED` is not
`AlreadyExists`, so a perfectly correct race is reported as a hard error.

## 2. Evidence

Discovered while closing C019, on run `37335370419` at commit `13bf9ea`, in
`CI / checks (windows-latest)`:

```text
thread '<unnamed>' panicked at src\config.rs:540:66:
called `Result::unwrap()` on an `Err` value:
  Io(Os { code: 5, kind: PermissionDenied, message: "Access is denied." })
test result: FAILED. 244 passed; 1 failed
```

`config.rs:540` is `load_or_create(&path).unwrap()` inside the spawned closure
of `config::tests::concurrent_first_use_creates_one_complete_template`, which
races 8 threads at a config under a directory that does not yet exist.

This is **intermittent**, and that is established rather than assumed:

| Run | Commit | `checks (windows-latest)` |
|---|---|---|
| `37334733986` | `9b4df98` | success |
| `37335370419` attempt 1 | `13bf9ea` | **failed** (244 passed, 1 failed) |
| `37335370419` attempt 2 | `13bf9ea` | success |
| `37334039683` | `7a4dc5e` | success |

Same bytes on `13bf9ea` in both attempts. `13bf9ea` changes only Markdown, so it
cannot have introduced the failure; the code is identical to the `9b4df98` run
that passed.

`src/config.rs` is not in C019's change set, and C019's own hosted evidence is
unaffected: its closing runs on `9b4df98` were green, and the two commits since
touch no code.

## 3. Why this is worth a plan rather than a rerun

A rerun is exactly the wrong response, and the repository already knows why: *a
green test proves only that its own premises hold.* This lane was green twice
and red once, and the next reader would have no way to tell that the red was
intermittent rather than causal. Leaving it is how a flake becomes an accepted
baseline.

The user-visible consequence is not theoretical. First use of the tool races
against itself whenever a machine starts several `cargo-cleanme` invocations at
once — an editor calling `config edit`, a shell prompt calling `scan`, a CI step
calling `clean`. On Windows, one of them can be told the config is unavailable
when it is being created by its own sibling, and `scan` exits non-zero.

## 4. What the fix must establish

1. **Which call returns code 5.** The diagnosis above narrows it to
   `fs::hard_link`, but it is inferred from the single `unwrap` in the path, not
   observed. The plan must capture the failing call before changing behavior.
   Do not ship a fix that assumes the inference was right.
2. **The benign outcome must be recognised by what it is, not by a list of
   error codes.** "The destination now exists and is a regular file" is the
   condition that means a concurrent writer won. Matching on `AlreadyExists`
   alone is the same class of defect as this one: a correct platform difference
   read as a failure.
3. **A premise negative.** Windows must be shown to produce the failure before
   the fix and to stop producing it after. A test that only ever runs on Linux
   cannot carry this.
4. **No weakening of the guard next door.** The adjacent check refuses to
   replace a non-file destination, and it must keep refusing. Accepting
   "exists and is a file" must not become accepting "exists".

## 5. Non-goals

- Changing the single-winner publication mechanism. `hard_link` is the right
  primitive; the defect is in how a lost race is read.
- Changing config parsing, retention, path validation, or the template.
- Touching discovery, cleanup, or the C019 filter work.
- Broadening `check-fixture-portability.py`, which cannot decide this: the test
  and its premise are fine, and the bug is in what the code does under a race.

## 6. Acceptance criteria

- [ ] The failing call under the 8-thread race is identified on Windows from
      evidence, not inference.
- [ ] A lost publication race is treated as success on every supported platform,
      expressed as "a regular-file destination now exists" rather than as an
      enumerated set of error kinds.
- [ ] A non-file destination is still refused.
- [ ] The Windows lane is exercised under a precondition that makes the race
      likely, and the fix is shown to change the outcome on Windows specifically.
- [ ] `cargo test` is green on Linux, macOS, and Windows.
- [ ] `architecture/03-config-and-editor.md` describes the actual mechanism and
      the actual failure mode.

## 7. Review questions

- Is `ErrorKind::PermissionDenied` on Windows ambiguous here — can it also mean a
  genuine permissions problem that must stay an error? If so, the destination
  check has to be the discriminator, not the error kind. This is the whole
  design question and it should be answered explicitly.
- Does the same tolerance belong in `ensure_exists`, which reaches the same
  `create_initial`? Verify rather than assume; a shared helper is not
  automatically a shared answer.
- Should a persistent failure be distinguishable from a lost race in the
  diagnostic? A user who sees "config is unavailable" deserves to know whether
  something else created the file first.

## 8. Evidence to attach at closure

- A Windows run showing the race failing on the pre-fix code.
- The same scenario on the fixed code.
- The full three-platform test matrix.
- The hosted run IDs on `main`.
