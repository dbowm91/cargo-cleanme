# Artifact Discovery and Cleanup C020 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c020-windows-concurrent-config-creation-race-corrective.md`](../../implementation/artifact-discovery-cleanup/c020-windows-concurrent-config-creation-race-corrective.md)

Corrects: `config::create_initial` (`src/config.rs:254-341`). No prior closure
record claimed this path was race-correct on Windows.

Implementation commit: `408c2a8`
Release tag carrying the correction: **none yet** — see §9.

## 1. Executive finding

**Every concurrent first use of the tool could fail on Windows, and the plan's
own diagnosis of the cause was wrong.**

`create_initial` stages the template under `<name>.tmp.<pid>.<attempt>` and
publishes it with a single `fs::hard_link`, so exactly one racer wins and the
rest are expected to fail benignly. The staging-name step tolerated exactly one
spelling of "somebody already has that name":

```rust
Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
```

POSIX reports `AlreadyExists`, so that arm always matched. **Windows reports
`PermissionDenied` for a file another thread is still creating**, because the
name is reserved before its metadata is committed. That is os error 5, "Access
is denied" — not `AlreadyExists` — so the thread never advanced to its next
attempt and `create_initial` returned a hard error. One racer failing is enough
to fail the caller, so `scan` could exit non-zero because an editor, a shell
prompt, and a CI step reached the config at the same moment.

The plan inferred that the failing call was `fs::hard_link` and required
evidence before shipping. **The inference was wrong**, and the wrongness was
worth the detour — see §4.

## 2. Evidence, before the fix

Discovered while closing C019, on run `37335370419` at `13bf9ea`:

```text
thread '<unnamed>' panicked at src\config.rs:540:66:
called `Result::unwrap()` on an `Err` value:
  Io(Os { code: 5, kind: PermissionDenied, message: "Access is denied." })
test result: FAILED. 244 passed; 1 failed
```

Intermittent, and established rather than assumed: green on `9b4df98`, red on
`13bf9ea`, green on the re-run of the same commit. `13bf9ea` changes only
Markdown, so the code is identical across those runs.

## 3. Making the premise likely

One 8-thread round against one directory hit the failure roughly one run in
three — which means the defect was invisible on most runs. A guard cannot rest
on a premise the defect can miss, so the test became 16 workers over 24 rounds,
each round a fresh nested path. Premises and assertions are unchanged: every
racer must succeed, and the final bytes must be one complete template.

That change alone reproduced the defect on every Windows run, which is the
premise negative for §1:

```text
run 37344874226 @ 15a41b1 → checks (windows-latest) failure, 244 passed / 1 failed
  Io(Custom { kind: PermissionDenied,
              error: "creating a staging file failed: Access is denied. (os error 5)" })
```

Note the step name. Without it the diagnosis would have stayed a guess.

## 4. The diagnosis was wrong, and the first fix was worse

Three findings in sequence, none of which would have been visible locally. This
is the reason the plan refused to accept an inferred cause.

**The failing call was not `fs::hard_link`.** It was
`fs::OpenOptions::create_new` on the staging name. A `hard_link`-only fix would
have left the bug in place and shipped a plausible-sounding commit.

**The first fix regressed three platforms.** Replacing the `AlreadyExists` arm
with a probe for "is the name taken" — `Err(_) if temp.exists() => continue` —
is the shape the plan's requirement 2 asked for ("recognise the benign outcome
by what it is, not by a list of error codes"), and it was wrong:

```text
run 37345415865 @ 3ef2521 → msrv failure, checks (macos-latest) failure
  Io(Custom { kind: AlreadyExists,
              error: "creating a staging file failed: File exists (os error 17)" })
```

The probe answers whether the name is taken *now*; the syscall answered whether
it was taken *then*. The winner can publish and unlink its staging file in
between, so by the time the loser looks the name is free again and `EEXIST`
escaped as an error.

**A staging error hid behind a mis-amended commit.** `git commit --amend`
without `-a` reuses the existing index, so the commit re-recorded the *previous*
blob. Run `37346137043` therefore tested the regressed variant again and failed
identically, which looked like the corrected fix failing on macOS. The blob
diff is what caught it: `git show 28324ba:src/config.rs` had no `AlreadyExists`
arm, while the worktree did. Worth recording because the symptom — a fix that
appears not to work — pointed at the code, and the actual cause was the commit.

## 5. The fix

```rust
Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
Err(_) if temp.exists() => continue,
Err(e) => return Err(AppError::Io(step_error("creating a staging file", e))),
```

Two signals for one benign outcome, because the two platforms do not agree:

- `AlreadyExists` is the OS saying the name was taken **at the moment of the
  call**. Authoritative even though the file may be gone by the time we look,
  so the kind stays the discriminator here.
- `PermissionDenied` is Windows' spelling when the name is reserved by a thread
  that has not finished creating it. The kind says nothing, so the filesystem is
  the evidence.

A genuine permissions problem leaves the name free and still hard-fails on both
platforms.

Two supporting changes, both of which earn their place:

- `step_error` (`:245-253`) names the call that failed while preserving
  `ErrorKind` **exactly**. Everything below decides by inspecting the kind, so a
  "helpful" wrapper that changed the kind would silently stop the race handling
  recognising its own outcome. It also turned an unactionable `Io(Os { … })`
  into a diagnostic that names the step, which is what made §3 and §4 possible.
- The test premise, as above.

## 6. What this did not change

The single-winner publication mechanism is untouched: `hard_link` is still the
primitive, a config is still never replaced, the template is still verbatim, the
non-file-destination refusal (`:329-334`) still refuses, and
`malformed_config_is_not_replaced_automatically` still holds. No parsing,
retention, path-validation, template, discovery, or cleanup behaviour changed.

One thing the evidence **did not** establish, recorded so nobody inherits it as
a fact: across 24 rounds of 16 threads on `windows-latest`, `hard_link` never
reported `PermissionDenied`. That is an observation, not a proof the platform
cannot do it. `architecture/03-config-and-editor.md` now says what to do if a
future run sees `publishing the config failed: Access is denied` — discriminate
on "a regular-file destination now exists", never on an error-kind list.

## 7. Local verification

```text
cargo fmt --all -- --check                                     PASS
cargo clippy --all-targets --all-features -- -D warnings       PASS
cargo test --all-features --lib config::    14 passed           PASS
cargo +1.89 test --locked --lib config::    14 passed           PASS
python3 scripts/check-doc-citations.py        16 documents clean PASS
```

## 8. Hosted CI

Run `37346722132` at `408c2a8` — all nine jobs green:

| Lane | Result |
|---|---|
| `checks (ubuntu-latest)` | success |
| `checks (macos-latest)` | success |
| `checks (windows-latest)` | success |
| `msrv` (1.89) | success |
| `installers` (ubuntu / macos / windows) | success |
| `generated-docs` | success |
| `benchmark` | success |

### 8.1 Premise negative

| Run | Commit | What it tested | Result |
|---|---|---|---|
| `37344874226` | `15a41b1` | step names + strengthened premise, **no fix** | windows failure — `PermissionDenied` at the staging step |
| `37345415865` | `3ef2521` | `temp.exists()` probe only | msrv + macOS failure — `AlreadyExists` escaping |
| `37346137043` | `28324ba` | mis-amended blob (identical to `3ef2521`) | msrv + macOS failure — identical signature |
| `37346722132` | `408c2a8` | the fix | all lanes green |

The premise negative is a Windows run, not a Linux surrogate, which is what the
plan required. Rows 2 and 3 are the regressed variant, kept because "the obvious
fix makes it worse on three platforms" is part of the finding.

## 9. Release status

The correction is on `main` and **unreleased**. Like C019, no published artifact
is needed to prove it: the defect is observable entirely within the repository
and on a hosted runner. 0.1.6 and earlier carry the Windows race until the next
release from `main`.

## 10. Findings recorded along the way

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | `architecture/14-testing-and-verification.md` §5 characterises 4 of 17 scripts and its `check-release-contract.py` row predates M010A | low | **Open, needs a corrective.** Unrelated; recorded in C019's closure §12. |
| 2 | `staging_is_cleaned_up_on_success_and_on_failure` (`cleanup.rs`) flaked once under parallel execution in the preceding milestone and passed on 3 reruns | low | **Open, pre-existing.** Untouched. |
| 3 | `check-fixture-portability.py` could not see a raw-byte-name fixture, so it missed the C018 macOS failure | low | **Closed in C019's pass** — third rule added, verified both directions. |

None is in `config.rs`, and none is caused by this change.

## 11. Acceptance criteria

| Criterion | Status |
|---|---|
| Failing call identified on Windows from evidence, not inference | met — §3, `creating a staging file failed: Access is denied. (os error 5)` |
| Lost race treated as success, expressed as filesystem state rather than an enumerated error list | met, with one correction — §4, §5. The kind is authoritative for `AlreadyExists`; the filesystem is the evidence only where the kind says nothing |
| Non-file destination still refused | met — `:329-334`, untouched |
| Windows exercised under a premise that makes the race likely, shown to change the outcome | met — §3 (fails before) and §8 (green after) |
| Green on Linux, macOS, and Windows | met — §8 |
| `architecture/03-config-and-editor.md` describes the actual mechanism and failure mode | met — rewritten, including the case this evidence did *not* establish |

## 12. Disposition

**Closed.**

The defect is confirmed, the failing call is known from a Windows run rather than
an argument, the fix is narrow, and the premise negative is a failure of this
code on the platform where it fails. Two intermediate states are recorded because
both would otherwise have shipped: a fix aimed at the wrong call, and a fix that
met the plan's stated requirement while regressing three lanes.

The correction is unreleased; finding 2 of C019 remains the reason the next
release from `main` matters.