# C027 — POSIX Installer User-Local PATH Persistence Corrective — status

Status: **conditionally closed.**

Plan: `plans/implementation/distribution-release-update/c027-posix-installer-user-local-path-persistence-corrective.md`

Repository baseline: `59c0be9422e52b23bbb771fe94943cb004670b59`
Implementation branch: `plans/c027-posix-installer-path-persistence`

Disposition: **conditionally closed** — the single remaining condition is a
published release carrying this installer, which C027 does not authorise. §1
states it precisely.

Implementation branch: `plans/c027-posix-installer-path-persistence`
Pull request: [#9](https://github.com/dbowm91/cargo-cleanme/pull/9)

Implementation commits:

- `b011c85` — the installer, the 19 POSIX PATH cases, the self-test additions,
  the macOS qualification script, and the documentation;
- `a114367`, `17b91ca`, `9d5b4f6` — three evidence repairs, each forced by a red
  hosted run. See §6.

Original plan and closure corrected, and deliberately not edited:

- `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`
- `plans/closure/distribution-release-update/010b-status.md`

Date: 2026-10-07

## 1. Why this disposition and not `closed`

Every acceptance criterion in §12 of the plan is satisfied except the standing
condition the plan itself wrote: **a published immutable release must carry the
corrected `install.sh`, followed by public-release evidence that the shipped
installer preserves the hosted macOS fresh-shell result.**

The published v0.2.2 wrapper is immutable and retains the old behaviour by
design. Nothing C027 did changes that, and C027 did not authorise, select, or
advance a release (§13 of the plan forbids publishing solely because the
implementation landed). So the correct maximum disposition is
`conditionally closed`, and the plan's §14 wording is honoured literally: "Until
that last item exists, the correct maximum disposition is `conditionally
closed`."

The operational closure dependency named in the plan header is discharged
differently than expected in one respect and not at all in the other. The
*hosted-platform* evidence exists and is green. The *public-release* evidence
does not exist because no release has been published. A future release that
carries this installer discharges the second half; it is a release-discipline
item, not a code item, and no future plan is blocked on it (§9).

## 2. What changed

### `packaging/install.sh`

A default non-root install now appends one bounded, guarded block to a single
startup file, **after** the verified binary is in place:

```sh
# >>> cargo-cleanme installer PATH >>>
# added by the cargo-cleanme installer; safe to delete
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
# <<< cargo-cleanme installer PATH <<<
```

The trailing `case` that previously only printed guidance is gone; its logic is
now separable helpers, as §7 of the plan required:

| Helper | Responsibility |
|---|---|
| `path_has_dir` | is the destination already on `PATH` |
| `shell_profile_target` | which supported startup file, or none |
| `profile_target_is_safe` | is the target a writable regular file, or a creatable file in an existing directory |
| `profile_has_active_entry` | does the profile already integrate the directory *for real* |
| `append_profile_entry` | the guarded, idempotent write |
| `print_path_guidance` | the current-shell and manual export lines |
| `profile_persistence_permitted` | may this invocation persist at all |

Helpers are shell-only. No Rust helper binary was added and nothing in PATH
integration requires cargo-cleanme to be runnable first.

### Flags

- `--no-shell-profile` is new: install the binary, print the manual guidance,
  touch no startup file.
- `--no-path` is now the broad opt-out: no startup-file mutation **and** no PATH
  output. This is the conservative direction for an existing scripted caller
  that already opted out — the plan called it out explicitly (§6), and it is the
  reason the change is safe to ship without asking anyone to re-audit a flag they
  may already be passing.

`install.ps1` and all Windows environment handling are untouched. §12 of the plan
requires the Windows lane to stay green to prove that, and it is (§5).

### Test seams

Two environment overrides exist so a fixture on one host can exercise the other
platform's *PATH-integration policy*:

- `CARGO_CLEANME_INSTALL_PATH_PROFILE_OS` — which startup file bash targets.
- `CARGO_CLEANME_INSTALL_SYSTEM_SCOPE` — refuse persistence, as a root install
  must, on a host that cannot become root.

Both are read by `shell_profile_target` and
`profile_persistence_permitted` only. Transport, digest, candidate identity, and
placement all run before any of it and read no override; §8 of the plan required
exactly this and the mutation runs in §6 are what prove it.

## 3. Requirement-to-evidence matrix

| §12 requirement | Evidence | Result |
|---|---|---|
| 1. a default non-root macOS zsh install persists `$HOME/.local/bin` safely and a fresh shell resolves the exact installed fixture binary | `case_profile_zsh_user_local`; hosted macOS run `37586963589`, job `112679225627`: `zsh -l -i -c 'command -v cargo-cleanme'` resolved `/var/folders/…/home/.local/bin/cargo-cleanme`, the exact fixture path | passed |
| 2. Linux zsh/bash behavior is deterministic and covered | `case_profile_zsh_user_local`, `case_profile_linux_bash`, `case_profile_macos_bash_precedence`; Ubuntu lane `112679225543` green | passed |
| 3. profile integration is idempotent | `case_profile_repeat_is_idempotent` asserts the profile is byte-identical after a `--force` re-install; mutation "append is not idempotent" fails 4 assertions | passed |
| 4. an existing active integration is respected without duplicate mutation | `case_profile_existing_active_entry_suppresses` asserts the user's file is byte-identical afterwards; `case_profile_commented_entry_does_not_suppress` is the negative direction | passed |
| 5. custom directories, system installs, unsupported shells, and unsafe profile targets are not auto-mutated | `case_profile_custom_dir_untouched`, `case_profile_system_scope_untouched`, `case_profile_unsupported_shell` (fish, unknown, unset), `case_profile_unsafe_targets` (symlink, directory, unwritable), `case_profile_non_regular_target` (FIFO), `case_profile_zdotdir_missing_directory` | passed |
| 6. `--no-shell-profile` and `--no-path` semantics are pinned | `case_profile_no_shell_profile_flag` (untouched profile, guidance still present), `case_profile_no_path_flag` (untouched profile, `PATH` absent from stdout entirely) | passed |
| 7. profile-integration failure cannot roll back or misreport a verified binary placement | `case_profile_append_failure_keeps_binary` asserts the binary is still installed *and* that the append genuinely failed *and* that the failure was reported; mutations "profile failure removes the binary" and "write failure reported as success" each fail 2 assertions | passed on Linux; **skipped on macOS with a reason** — see §7 |
| 8. existing installer integrity/fallback/identity/security tests remain green | the 15-case cross-platform roster is unchanged and green on all three lanes | passed |
| 9. hosted Linux/macOS/Windows installer lanes are green | run `37586963589`: all three `installers` jobs success; `Release drift guard` `37586963594` success | passed |
| 10. documentation no longer claims the POSIX wrapper is manual-PATH-only | `docs/INSTALLING.md` gains a "PATH integration" section; `docs/TROUBLESHOOTING.md` gains "installed but a new terminal cannot find it"; `README.md` and `docs/AUTOMATION.md` no longer say the tool "never modifies your PATH"; `architecture/15` gains §3.1a; `CHANGELOG.md` has an `[Unreleased]` entry | passed |
| final: a published release carries the fix | **none** | **not satisfied** — §1 |

## 4. The fixture cases, and what each one actually measures

19 POSIX cases in `POSIX_PATH_CASES`
(`packaging/tests/test_installers.py:1547-1567`). Every one isolates `HOME`,
`SHELL`, and `ZDOTDIR` into a fixture-owned directory; the CI account's real
dotfiles are never read or written. Every one runs the wrapper with a curated
`PATH` containing no user-local bin directory, so "the destination is already on
`PATH`" is a real condition rather than an accident of the runner's environment.

The plan enumerated 14 cases. All 14 are present, plus 5 that the implementation
surfaced and that the plan's list would not have predicted:

- **a missing `ZDOTDIR` directory** — the installer must not create a directory
  tree to drop a dotfile into. Mutation: `mkdir -p "$_parent"` → 2 failing
  assertions.
- **a FIFO profile target** — the regular-file guard is what keeps the installer
  off a named pipe. Appending to a FIFO blocks forever, so this case asserts the
  wrapper returns promptly; a wrapper without the guard hangs until the run's
  timeout. Mutation: dropping `-f` → 4 failing assertions.
- **the `case` guard itself** — sourcing the profile twice must leave the
  directory on `PATH` exactly once. An unguarded `export PATH=…` still resolves
  the command on the *first* source, so the fresh-shell case cannot see the
  difference. Mutation → 2 failing assertions.
- **the fresh-shell resolution**, inside the suite as well as in the standalone
  macOS script.
- **`file_limit_is_enforced`**, the premise probe for requirement 7, discussed in
  §6 and §7.

Both rosters are tuples and the driver counts what it ran
(`packaging/tests/test_installers.py:1786-1803`), because a case quietly dropped
from a roster would shrink coverage while the run stayed green — the failure mode
this suite exists to prevent.

### The premise guards, and the failing direction

The original defect survived because the assertion was "stdout contains `PATH`".
Every new assertion is about filesystem and shell state instead. §8 of the plan
required a failing-direction premise in the self-test, and `self_test()` now
includes:

- the block-count guard rejecting 0 and 2 managed blocks and accepting 1;
- the untouched-profile guard reporting a *created* file as touched, and
  accepting absence when absence was expected.

The mutants below are the stronger form of the same requirement: each removes one
guard from `packaging/install.sh` and requires the suite to go red.

## 5. Hosted evidence

Run `37586963589` (`CI`, pull request), all nine jobs success:

| Job | Conclusion |
|---|---|
| `installers (ubuntu-latest)` | success — 47 passed, 0 failed, 0 skipped |
| `installers (macos-latest)` | success — 46 passed, 0 failed, 1 skipped |
| `installers (windows-latest)` | success — 18 passed, 0 failed, 1 skipped |
| `checks (ubuntu/macos/windows-latest)` | success |
| `msrv`, `benchmark`, `generated-docs` | success |

`Release drift guard` `37586963594` (pull request) — success.

The macOS fresh-shell step, verbatim from job `112679225627`:

```
--- fresh zsh -l -i -c resolves the installed binary ---
  zsh -l -i -c 'command -v cargo-cleanme' -> '/var/folders/36/…/home/.local/bin/cargo-cleanme'
  ok   the fresh shell resolves the exact installed fixture binary

--- without the profile, the same shell must not resolve it ---
  zsh -l -i -c 'command -v cargo-cleanme' -> ''
  ok   the resolution depends on the persisted entry, not on ambient PATH
```

The second block is the negative direction in the same environment. Without it,
a `PATH` that happened to contain the directory would make the first result
meaningless, and "the profile contains the expected text" would be the only
evidence — which §9 of the plan explicitly forbade as the sole proof.

The Windows lane's count is 18 rather than 34 because the PowerShell block runs
alone and the 19 POSIX PATH cases do not apply to `install.ps1`. That is the
expected shape, not a reduction in Windows coverage: the wrapper is untouched
and its 15 cross-platform cases are unchanged.

## 6. Three red runs, and what each one found

The evidence was wrong three times before it was right. Recording that is the
point of this section; all three runs failed and are named as failures.

### `37584787476` — the macOS installer lane failed (2 assertions)

Both failures were in the **evidence**, not the installer:

1. the fresh-shell probe passed the curated tool-only `PATH` to the probe shell.
   On macOS, `/etc/zprofile` calls `path_helper`, which is not resolvable on that
   `PATH`, so startup stops before `.zshrc` is read — and the case reported a
   failure while proving nothing about the installer. The curated `PATH` exists
   for the *installer's* own check; handing it to the reader of the profile
   conflates two different subjects.
2. the same probe passed `ZDOTDIR=""`, believing it meant "unset". zsh consults
   `ZDOTDIR` in place of `HOME` whenever it is present at all, so an empty value
   sends it looking for `/.zshrc`.

Fixed in `a114367` and `17b91ca`: the probe keeps the host `PATH` and `ZDOTDIR`
is *removed* rather than emptied.

### `37585797197` — the macOS installer lane failed (2 assertions)

Same two, not yet both corrected, plus the append-failure case. This run is what
exposed the third defect.

### `37586422088` — the macOS installer lane failed (the qualification step)

`/bin/zsh: can't open input file: command -v cargo-cleanme` — zsh saying it had
no `-c`. Without it, the argument is a script file name, not a command string.
The suite's copy of this probe already passed `-c`; the standalone macOS
qualification script's flags did not, because the two were written separately and
only one of them was corrected. Fixed in `9d5b4f6`.

### The mutation runs, and the one that was missed first

Each mutant removes one guard from `packaging/install.sh` and requires the suite
to go red. All of the following were caught:

| Mutant | Failing assertions |
|---|---|
| persistence disabled entirely | the canonical-zsh case, plus dependents |
| symlink target followed | 2 |
| non-regular (FIFO) target written into | 4 |
| unwritable existing target rewritten | 2 |
| any mention of the dir counted as active | 2 |
| comment/prose counted as active | 2 |
| custom `--dir` auto-persisted | 4 |
| system scope persisted | 2 |
| `--no-path` gate removed (call site) | 2 |
| `--no-path` gate removed (both inner gates) | 2 |
| `--no-shell-profile` parsed as a no-op | 2 |
| `--no-path` still printed guidance | 2 |
| unsupported shells given a guessed `.zshrc` | 4 |
| macOS bash precedence ignored | 8 |
| `ZDOTDIR` validity ignored | 10 |
| invalid `ZDOTDIR` accepted | 10 |
| managed append not idempotent | 4 |
| duplicate block written twice | 4 |
| entry written but syntactically inert | 4 (and the macOS harness) |
| managed block written with no entry | 22 |
| profile selection broken (zsh writes `.bashrc`) | 20 |
| unsafe target refused but not named | 10 |
| write failure reported as success | 2 |
| profile failure removes the binary | 2 |
| parent directory created if missing | 2 |
| `PATH` entry prepended without a guard | 2 |

Five mutants initially passed the suite. Each is worth stating because in four of
them the suite was right to be suspicious of itself:

1. **"commented entry treated as active"** — an early version of the mutant
   changed `grep -v '^[[:space:]]*#' | grep -E …PATH=` to a plain `cat`, which is
   not actually a behavioural change; the subsequent filter still rejected the
   comment. The mutant was invalid, not missed. The real mutant (`grep -qF` on the
   whole file) is caught with 2 failing assertions.
2. **"custom dir auto-persisted"** and **"system scope persisted"** — removing one
   guard is not enough, because two independent guards cover each case. Removing
   both is caught.
3. **"non-regular target written into"** and **"unwritable existing target
   rewritten"** — passed because the test could not *tell* the two refusal
   messages apart. Both said "add it yourself". The two constants
   `PROFILE_UNSAFE_TARGET` and `PROFILE_WRITE_FAILED` now exist so the cases can
   distinguish "refused before writing" from "tried and the filesystem said no",
   and both mutants are caught.
4. **"`PATH` entry prepended unconditionally"** — passed because idempotency
   happened to survive it and the fresh shell still resolved. That is exactly
   what `case_profile_entry_is_guarded` was then written for; it sources the
   profile twice and counts occurrences, and catches it.

The general lesson is the one this repository keeps re-learning, in a new shape:
**a green assertion may be green because the thing it is trying to distinguish
does not exist in the observable surface.** Four of the five were found by asking
"what mutant would make this pass for the wrong reason?", not by reading the
wrapper.

## 7. Known limitations

1. **Requirement 7 cannot be exercised on macOS.** The append-failure case needs
   a write that fails after the binary is verified in place. The fixture provokes
   it with `RLIMIT_FSIZE`, and macOS does not enforce that limit on writes to
   regular files the same way Linux does. `file_limit_is_enforced()` probes the
   platform with the *same shape of write the wrapper performs* — a brace group
   appending with `>>` to a pre-filled regular file with `SIGXFSZ` ignored — and
   the case **skips with an explicit reason** where enforcement cannot be
   provoked. The Ubuntu lane runs it (47 passed, 0 skipped); the macOS lane
   reports the skip. This is recorded rather than hidden: a case that skips
   loudly on the platform that cannot run it is honest, and a case that passed
   vacuously would not be.

   The first version of that probe inferred enforcement from a plain `> file`
   write and got it wrong on macOS, where such a write succeeds while the
   wrapper's own append still fails. The probe was rewritten to run the real
   thing. This is the "guard that stopped guarding" shape, caught by asking what
   the guard actually measures.

2. **`--no-path` now suppresses more than it used to.** A caller that passed
   `--no-path` and separately relied on the startup file being untouched gains
   nothing, and a caller that passed it expecting no output still gets none. No
   caller can acquire a mutation they did not previously have, which is the
   reason this is the conservative direction; it is documented in `CHANGELOG.md`,
   `docs/INSTALLING.md`, and `--help`.

3. **The managed block uses the literal path**, not `$HOME`. A user who later
   changes `HOME` must re-run the installer. The literal spelling was chosen
   because the entry stays valid and readable when a later shell sources it, and
   because `HOME` is not guaranteed to be set in every context that reads a
   startup file.

4. **fish, nushell, elvish, csh, and every other shell are unsupported**, and get
   manual guidance rather than a guess. §13 of the plan forbids expanding the
   shell set, so this is by design, not an oversight.

5. **The macOS evidence is against the fixture release, not the public one.** The
   fixture server is local and no test in the suite contacts GitHub. The
   public-release half of the closure condition is therefore genuinely
   outstanding — see §1.

6. **`architecture/15` line citations were re-derived, not shifted.** The
   mechanical half of `check-doc-citations.py` does not cover `install.sh`
   citations at all (it resolves `src/` files), so those were re-derived by
   reading the new source. §5 of `AGENTS.md` applies and was followed.

## 8. Findings, classified

| Finding | Severity | Disposition |
|---|---|---|
| The shipped POSIX installer reported success while a new terminal could not find the binary on a stock macOS account | **user-facing distribution defect** | Corrected by C027. The published v0.2.2 wrapper remains immutable; a future release carries the fix |
| The fixture asserted textual guidance rather than a state transition, which is why the defect survived a release cycle | **harness defect** | Corrected: 19 state assertions, all mutation-checked |
| Three hosted macOS runs failed on the *evidence* before it was right | harness defect | Corrected in `a114367`, `17b91ca`, `9d5b4f6`. Recorded as failures, not rewritten |
| The first `RLIMIT_FSIZE` probe inferred enforcement from a probe that did not match the wrapper's write | harness defect | Corrected in `17b91ca`; the case now skips loudly where it cannot run |
| Two distinct profile-failure diagnostics were indistinguishable to the tests | harness defect (latent) | Corrected: `PROFILE_UNSAFE_TARGET` vs `PROFILE_WRITE_FAILED` |
| `install.sh --help` gained two flags without regenerating completions/manpages | non-issue | Correct. Those artefacts are a projection of the Rust clap surface, which C027 does not touch; `generate-docs -- --check` confirms 13 artefacts still match |
| The POSIX/PowerShell PATH contract is now asymmetric | intentional | Recorded in `architecture/15` §9. The plan scoped C027 to `install.sh`; Windows environment-variable persistence was explicitly out of scope |

No finding requires a separate corrective plan. None of them is a product defect
in `src/` — C027 changed no Rust at all.

## 9. What future work this unblocks, and what it does not

**No plan is blocked by C027.** Stated explicitly because "conditionally closed"
is easy to read as "something is waiting":

- The product and release line remains published through **v0.2.2**. C027 does
  not authorise a release and does not select a version.
- A future release that carries this installer discharges the remaining
  operational condition. That is release discipline, not an open implementation
  plan.
- `C010` remains `proposed`, upstream, and non-blocking, exactly as before.
- **No Phase 14 feature milestone has been accepted and none was activated by
  this pass.**

The registry now reads: the one open cargo-cleanme-local corrective is
*conditionally closed*, with its only remaining condition named as a publication
event rather than as engineering work.

## 10. Verification actually run

Local, on the implementation branch, in the plan's §11 order:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass, no warnings |
| `cargo test --all-targets --all-features` | pass — 318 unit + 36 `cli_contract` + 4 `end_to_end`, 0 failed |
| `python3 scripts/check-fixture-portability.py --self-test` | pass (four rules, both directions) |
| `python3 scripts/check-fixture-portability.py` | pass — 36 fixture files clean |
| `python3 scripts/check-doc-citations.py --self-test` | pass |
| `python3 scripts/check-doc-citations.py` | pass — 16 documents clean |
| `sh -n packaging/install.sh` | pass (also `bash -n` and `dash -n`) |
| `python3 packaging/tests/test_installers.py --self-test` | pass |
| `python3 packaging/tests/test_installers.py` | pass — 47 passed, 0 failed, 0 skipped |
| `python3 packaging/tests/qualify_macos_fresh_shell.py` | skip on Linux (exit 0); `--require-macos` correctly fails on Linux (exit 1) |
| `python3 packaging/tests/qualify_macos_fresh_shell.py --require-macos` in CI | pass on macOS |
| `qualify()` driven locally with bash | pass, including the negative direction |
| `python3 scripts/check-installer-contract.py --self-test` | pass |
| `python3 scripts/check-installer-contract.py` | pass — 5 contracted targets, 1 Cargo-only family, install names match |
| `python3 scripts/check-release-contract.py --self-test` | pass |
| `python3 scripts/check-release-contract.py` | pass — 5 targets, 11 assets, wrappers aligned |
| `cargo run --quiet --features dev-tools --bin generate-docs -- --check` | pass — 13 artefacts match the clap model |

Hosted, run `37586963589` — all nine jobs success (§5). `Release drift guard`
`37586963594` success.

Not run, deliberately: `scripts/release-check.sh`. Per `AGENTS.md` §3 it
requires a clean tree, the `eggpack` binary, and toolchain 1.89, and is not
routine verification. C027 published nothing, so its release gate has no trigger.

`docs/RELEASING.md` was not executed as a procedure, because no release was cut.
That is a real gap in the *release* line and not in C027, which is why §1 stops
at `conditionally closed`.

## 11. Documentation surfaces updated

| Surface | Change |
|---|---|
| `packaging/install.sh` `--help` | both flags documented; a "Shell PATH integration" section states the destination, the shell support, and the fact the current shell still needs the export |
| `docs/INSTALLING.md` | new "PATH integration" section with the exact managed block, the idempotency and removal instructions, a table of when the profile is left alone, and the per-shell file selection |
| `docs/TROUBLESHOOTING.md` | new entry "installed but a new terminal cannot find it", replacing the manual-only claim; both failure messages explained |
| `README.md` | the guarantee no longer says the tool never modifies `PATH`; it now describes the bounded block and the two flags |
| `docs/AUTOMATION.md` | same correction, scoped to the tool vs the bootstrap installer |
| `architecture/15-distribution-and-release.md` | new §3.1a documenting the state-transition contract, the four load-bearing properties, and the two seams; §3.1 table row; §3.4 fixture description; §9 records the intentional POSIX/PowerShell asymmetry |
| `CHANGELOG.md` | `[Unreleased]` entry describing the defect, the fix, the flags, and the qualification |

Not edited, deliberately: M010B's closed plan and closure record (C027 corrects
forward), `completions/` and `man/` (generated from the Rust clap surface, which
did not change), and `AGENTS.md` §7 (its decided-forever list is unaffected — this
is a distribution-surface change, not a product decision).

## 12. Disposition

**C027 is conditionally closed.**

Ten of the plan's eleven acceptance criteria are satisfied and evidenced; the
eleventh is a publication event that C027 explicitly does not authorise. The
implementation is complete, the evidence is green on all three hosted platforms,
and the one platform-limited case skips loudly rather than passing vacuously.

The corrective worked as a corrective is supposed to: it found the defect, it
found three defects in its own evidence, and it recorded all four rather than
repairing the last three quietly. Nothing in this record waits on code that
someone still has to write.