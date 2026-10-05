# Artifact Discovery and Cleanup C020 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c020-windows-concurrent-config-creation-race-corrective.md`](../../implementation/artifact-discovery-cleanup/c020-windows-concurrent-config-creation-race-corrective.md)

Corrects: `config::create_initial` (`src/config.rs:254-341`). No prior closure
record claimed this path was race-correct on Windows.

Implementation commits: `408c2a8` (diagnostics + first fix),
`4e2e3f6` (the fix), `fcc9eee` (architecture rewrite)
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

## 5. The first fix, and why it was not enough

The obvious repair was to recognise the taken name by asking the filesystem
rather than by matching an error kind — the shape the plan's requirement 2 asked
for. It was wrong twice.

Run `37346722132` attempt 1 was green on all nine lanes; attempt 2 — the same
commit, `408c2a8`, Windows only — failed at the same step with the same
signature. `Err(_) if temp.exists() => continue` is racy for **both** spellings:
the winner of the publication unlinks its staging file, so a loser that probes
afterwards reads the name free and the real failure escapes. Attempt 1 being
green is exactly the evidence a single run cannot provide.

That made the underlying defect legible. The staging name was never unique among
everything that can create a config at once:

```rust
temp_name.push(format!(".tmp.{}.{}", std::process::id(), attempt));
```

The pid separates *processes*. Threads inside one process shared both the pid
and the starting `attempt`, so **every** concurrent first use was a collision —
on all platforms, and only Windows reported it as a hard error, because only
Windows spells the collision `PermissionDenied`.

## 6. The fix

Remove the collision rather than recognise a lost one:

- `STAGING_NONCE` (`src/config.rs:253`), a process-wide `AtomicU64` folded into
  the staging name (`:293-296`). Two threads no longer contend for a name at all,
  so the intra-process race disappears instead of being tolerated. With nothing
  else competing, `PermissionDenied` once again means what it says, and a genuine
  permissions problem hard-fails.
- The `attempt` suffix stays for the case the nonce cannot cover: a stale
  `.tmp.` file left by a crashed process whose pid the OS later reuses. That is a
  real name collision, and `AlreadyExists` is the authoritative answer (`:318`).
- The **publish** step is the mirror image, and is decided by the filesystem
  (`:350-352`): `path.is_file()`. The destination is never unlinked, so unlike
  the staging name this probe is stable — and it is the only file that can exist
  at that path, published solely by hard-linking a written and synced staging
  file, so a regular file there is always a complete template. The non-file
  refusal (`:355-359`) is untouched: "exists" must not become "acceptable".

Supporting change: `step_error` (`:255-257`) names the call that failed while
preserving `ErrorKind` **exactly**, because every branch below decides by
inspecting the kind — a wrapper that changed it would silently stop the race
handling recognising its own outcome. It turned an unactionable
`Io(Os { code: 5 })` into
`creating a staging file failed: Access is denied. (os error 5)`, which is what
made §3 and §4 possible at all.

## 6a. What was not changed

The single-winner publication mechanism is untouched: `hard_link` is still the
primitive, a config is still never replaced, the template is still verbatim, and
the non-file-destination refusal still refuses. No parsing, retention,
path-validation, template, discovery, or cleanup behaviour changed.

One thing the evidence did **not** establish, recorded so nobody inherits it as
fact: `hard_link` losing the publication race was never observed reporting
`PermissionDenied`. It is handled anyway, because the discriminator there is
reliable — but "handled defensively" is not "observed to happen", and
`architecture/03-config-and-editor.md` says so.

## 7. Local verification

```text
cargo fmt --all -- --check                                     PASS
cargo clippy --all-targets --all-features -- -D warnings       PASS
cargo test --all-features --lib config::    14 passed           PASS
cargo +1.89 test --locked --lib config::    14 passed           PASS
python3 scripts/check-doc-citations.py        16 documents clean PASS
```

## 8. Hosted CI

Run `37347758658` at `4e2e3f6` (the nonce fix) — all nine lanes green. Run
`37347945989` at `fcc9eee` (the documentation rewrite; identical code) — all
nine lanes green.

| Lane | Result |
|---|---|
| `checks (ubuntu-latest)` | success |
| `checks (macos-latest)` | success |
| `checks (windows-latest)` | success |
| `msrv` (1.89) | success |
| `installers` (ubuntu / macos / windows) | success |
| `generated-docs` | success |
| `benchmark` | success |

### 8.1 One green run is not the evidence; seven are

`37346722132` attempt 1 was green on all nine lanes and attempt 2 — same commit,
Windows only — failed. That is the whole reason §5 exists, so the fixed code was
re-run rather than trusted:

| Run | Commit | `checks (windows-latest)` |
|---|---|---|
| `37346722132` attempt 1 | `408c2a8` | success |
| `37346722132` attempt 2 | `408c2a8` | **failed** — first fix incomplete |
| `37347945989` attempt 1 | `fcc9eee` | success |
| `37347945989` attempts 2–7 | `fcc9eee` | success ×6 |

Seven consecutive green Windows runs of the strengthened premise — 16 workers ×
24 rounds each, 2 688 racing thread-pairs per run — against the one failure that
preceded them. The contrast is the evidence: the same premise, the same runner,
the same commit shape, before and after.

### 8.2 Premise negative

| Run | Commit | What it tested | Result |
|---|---|---|---|
| `37344874226` | `15a41b1` | step names + strengthened premise, **no fix** | windows failure — `creating a staging file failed: Access is denied. (os error 5)` |
| `37345415865` | `3ef2521` | `temp.exists()` probe only | msrv + macOS failure — `AlreadyExists` escaping |
| `37346137043` | `28324ba` | mis-amended blob, identical to `3ef2521` | msrv + macOS failure — identical signature |
| `37346722132` a2 | `408c2a8` | kind + probe | windows failure — `PermissionDenied` leaking through the probe |
| `37347758658` | `4e2e3f6` | the nonce fix | all lanes green |

The premise negative is a Windows run, not a Linux surrogate. Rows 2–4 are the
two intermediate fixes, kept because "the obvious repair regresses three lanes
and then leaks on Windows anyway" is part of the finding.

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
| Lost race treated as success, and decided by the right evidence | met — §5, §6. The kind is authoritative where the syscall answered "taken *then*"; the filesystem is the evidence only where a probe is stable. The staging name is removed from the question by the nonce, and the destination is probed because it is never unlinked |
| Non-file destination still refused | met — `:329-334`, untouched |
| Windows exercised under a premise that makes the race likely, shown to change the outcome | met — §3 (fails before) and §8 (green after) |
| Green on Linux, macOS, and Windows | met — §8, and seven consecutive Windows runs at §8.1 |
| `architecture/03-config-and-editor.md` describes the actual mechanism and failure mode | met — rewritten, including the case this evidence did *not* establish |

## 12. Disposition

**Closed.**

The defect is confirmed, the failing call is known from a Windows run rather than
an argument, and the fix removes the collision instead of tolerating it. Three
intermediate states are recorded because each would otherwise have shipped, each
disguised as progress:

1. a fix aimed at the wrong call (`fs::hard_link`, from an inference);
2. a fix that met the plan's stated requirement — recognise the benign outcome by
   filesystem state — while regressing Linux, macOS, and the 1.89 lane;
3. a fix that passed all nine lanes on its first hosted run and then failed on
   the repeat, proving that one green run of a strengthened premise is not
   evidence.

The premise negative is a failure of this code on the platform where it fails,
and the fix is green on seven consecutive Windows runs of that same premise.

The correction is unreleased; finding 2 of C019 remains the reason the next
release from `main` matters.