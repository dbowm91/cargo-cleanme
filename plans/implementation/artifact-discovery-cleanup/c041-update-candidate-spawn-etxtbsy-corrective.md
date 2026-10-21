# C041 — Update-Path Candidate Spawn Can Fail With ETXTBSY: corrective

Status: **proposed** — found by the v0.2.10 release gate. The mechanism is
narrowed but not yet proven; not implemented.

Repository baseline: `main` at `270cceb` (v0.2.10, published).

Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.

Related: `docs/RELEASING.md` §"The complete local gate"; the update
subsystem's own deep dive (`architecture/12-self-update.md`); the release
record `plans/closure/distribution-release-update/r030-status.md` §11; and
`architecture/overview.md` §10 finding **61**, which this plan records.

This is a **new** finding. It is not a fix for any existing §10 row, and the
§10 rows 19 and 37 — the negative-result defects C038 closed — have nothing
to do with it.

## The finding

The v0.2.10 release gate failed on its **first** run and passed on its second,
with no change to the tree between the two runs. The failure was in
`update::tests::a_staging_cleanup_failure_is_reported_not_swallowed`
(`src/update.rs:2603`):

```
---- update::tests::a_staging_cleanup_failure_is_reported_not_swallowed stdout ----
cargo-cleanme: staging directory /tmp/cargo-cleanme-update-test-2043996-…-26 could not be
removed after a failed update: Directory not empty

thread 'update::tests::a_staging_cleanup_failure_is_reported_not_swallowed' panicked at
src/update.rs:2619:9:
the failure must name the directory and the reason it survived: the candidate did not
identify itself as cargo-cleanme 0.2.0
spawning bounded candidate: Text file busy (os error 26)
```

`ETXTBSY` is `execve` refusing to run a file that some process still holds
open for writing. The spawn is Eggup's `run_bounded`
(`eggup-core-0.1.2/src/candidate.rs:141`), and the program it runs is the
**staged candidate** — the freshly downloaded, freshly copied, freshly
chmod-ed copy of the new release binary, which the transaction is about to
execute to prove it really is `cargo-cleanme <version>` before replacing the
live executable with it.

## Why this matters, and how much

**It is not a safety defect.** The transaction aborts *before* the commit, the
error is reported rather than swallowed, and nothing is deleted. The
ownership proof and the pre-spawn re-validation are untouched.

**It is a reliability defect, and it is reachable by a user.** The same code
path runs in production: `cargo cleanme update` downloads a candidate, stages
it, and executes it. If the staged file can be observed open for writing at
the moment it is executed, an update can fail spuriously with a
`Transaction` error the operator cannot act on — and the retry may hit the
same race.

**It is pre-existing.** `src/update.rs` is untouched by C039 and C040. The
release gate has passed on every previous release. This is the first time the
gate has caught it.

## What is known, and what is not

Measured on the release host:

| Condition | Result |
|---|---|
| The test alone, 12 consecutive runs | 12 pass, 0 fail |
| The test alone, 40 consecutive runs (re-measured for this record) | 40 pass, 0 fail |
| The whole lib suite, quiet host, 37 consecutive runs | 37 pass, 0 fail |
| The whole lib suite, 8-way CPU load, 8 runs | 8 pass, 0 fail |
| The whole lib suite, one burst | **1 of 6 failed** |
| The release gate, first run | **failed** |
| The release gate, second run, same tree | passed |

So it is **load- and timing-dependent**, and rare. It has never reproduced
on the hosted CI matrix, on any of ubuntu/macos/windows, across many runs.

**The root cause is not established.** What *has* been established, by reading
the sources rather than guessing at them, is the following — and it is
narrower than "something is holding the file open somewhere".

### The candidate is a shell script, and the shell is the hazard

The staged candidate in this path is a `#!/bin/sh` script
(`src/update.rs:1582-1604`, `candidate_bytes`), and it is executed with
`current_dir` set to the stage root. `run_bounded` spawns it through
`std::process::Command` with `env_clear()`.

`ETXTBSY` on `execve` of a **script** is a different failure from `ETXTBSY`
on a binary. The kernel's `execve` of a `#!` script opens the *interpreter*
and passes the *script path* as an argument; the interpreter then opens the
script itself. The kernel's "text file busy" check applies to the file being
`execve`'d. For a script, the file the kernel is asked to execute is the
script; the interpreter is a separate `execve`.

So the failing `execve` is of the **staged script**, and the condition is
that the staged file had an open write descriptor at that instant.

### The write handle that could still be open

The staging write is Eggup's, in `eggup-core-0.1.2/src/stage.rs`:

```rust
let mut staged = fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .open(destination)
    .map_err(|source| Error::io("creating bound stage destination", source))?;
…
std::io::copy(&mut handle, &mut staged)
    .map_err(|source| Error::io("staging bound source into private stage", source))?;
```

The `staged` handle is a local binding: it is dropped when `stage_bound_source`
returns, and `apply_permissions` runs *after* it. So on the **bound-source**
path the write handle is closed before the candidate is executed.

The **unbound** path — which is the path this crate takes, because it calls
`install_plan.prepare()` and never `prepare_with_bound_sources` — goes
through `fs::copy(member.source(), &destination)` instead
(`eggup-core-0.1.2/src/stage.rs:87`). `fs::copy` opens the destination,
copies, and closes it, but **it closes it in a `Drop` that the standard
library performs**, and `std::fs::copy` on Linux is implemented on top of
`copy_file_range`, whose write side is the destination descriptor.

That is a plausible mechanism, and it is a *different* mechanism from the
bound-source path, which is consistent with the observation that the failure
is rare and load-dependent: it is a race between the kernel's write-descriptor
teardown and the subsequent `execve`, and it is only reachable when the
descriptor's teardown is delayed.

**This has not been proven.** It is the leading hypothesis, and it is stated
here as a hypothesis because the plan's own rule is that a mechanism which has
not been demonstrated must not be written down as if it had been. The
distinguishing evidence — which descriptor holds the staged file open, and in
which process — is exactly what the reproduction must produce.

### Upstream `eggup-core` 0.1.3 does **not** fix this

The registry carries `eggup-core` 0.1.3 (and `eggup-acquisition` 0.1.3,
`eggup-eggfetch` 0.1.3). The two crates were diffed, source against source,
for this plan:

- `run_bounded` — the function that performs the spawn and produces the
  error string — is **byte-identical** between 0.1.2 and 0.1.3.
- `stage_bound_source` is byte-identical between the two versions.
- 0.1.3's changes are: `StagePlacement` (where the stage directory is
  created), reading the executable intent from the source *handle's* metadata
  rather than from a later stat, `commit_with_stale_lock_recovery`,
  `current_exe.rs`, and a Windows-only `self-replace` dependency for the
  kept-installed self-update.

So "pin upstream until the fix lands" is **not** available as a mitigation
here: 0.1.3 does not carry the fix. That is a material change to this plan's
options, and it was worth the diff to establish.

## Non-goals

- **Not** a change to the update transaction's shape, the digest binding, the
  ownership proof, or the commit order.
- **Not** a suppression of the error. A failed update must keep reporting
  that it failed.
- **Not** a retry. A retry would mask a race rather than find it.
- **Not** a change to what the candidate is asked to prove.
- **Not** a bump to `eggup-core` 0.1.3 as a *fix*. It is not one.

## Required work

1. **Reproduce it deterministically.** The current rate is too low to
   instrument. The candidate is a `#!/bin/sh` script, and the spawn is the
   only exec in the path; a harness that holds a descriptor open on the
   staged file while the transaction runs would turn the flake into a
   deterministic test and *also* explain the mechanism. The harness must be
   built to fail **against the current code** — a reproduction that only
   passes on a fixed build proves nothing.
2. **Find the holder.** Instrument the spawn path to report the offending
   descriptor's target and the process that holds it (`/proc/<pid>/fd`).
   Until that is known, the fix is a guess.
3. **Fix the cause, not the symptom.** If the holder is in this crate, fix it
   here. If it is in `eggup-core`, the fix is upstream — **file it against
   `eggup-core`, and note that 0.1.3 does not carry it** (established above),
   so the workaround here must be a pin to a version that *does*, not a pin to
   the newest one. The dependency versions are `eggup-acquisition`,
   `eggup-core` and `eggup-eggfetch` 0.1.2.
4. **Prove the fix.** The gate must be green over a run of the whole suite
   large enough that the pre-fix rate would have shown. A fix that turns
   "1 in 6" into "0 in 6" is not evidence.

## Acceptance criteria

- The failure is reproducible on demand, and the mechanism is named — with the
  descriptor identified, not inferred.
- The fix removes the race, and the gate is green over at least 50 whole-suite
  runs.
- The update transaction's shape, its error reporting, and the ownership
  proof are unchanged.

## Closure evidence requirements

- The reproduction, and the descriptor that was holding the file open.
- The fix, and where it lives (this crate, or upstream).
- The gate's result over at least 50 whole-suite runs before and after.
- The hosted matrix's result.

## Note on the v0.2.10 release

The v0.2.10 release was published with this finding open. The release gate
failed once and passed on the second run of the **same tree**; the second run
is the one recorded in the release record. The finding is recorded here
rather than fixed inside the release, because a fix to the update path is a
safety-adjacent change that deserves its own review, and because the
intermittency is not understood well enough to fix responsibly.
