# C027 — POSIX Installer User-Local PATH Persistence Corrective

Status: **conditionally closed**

Closure record: `plans/closure/distribution-release-update/c027-status.md`

The remaining condition is a published release carrying this installer. It is
named in §14 below and C027 does not authorise one.

Repository baseline: `59c0be9422e52b23bbb771fe94943cb004670b59` (C026 closed on `main`).

Planning branch: `plans/c027-posix-installer-path-persistence`.

Source roadmap:

- `plans/subsystems/distribution-release-update-roadmap.md`
- Phase 10 / M010B bootstrap installer semantics.

Original plan and closure corrected:

- `plans/implementation/distribution-release-update/010b-bootstrap-installers-and-release-artifact-qualification.md`
- `plans/closure/distribution-release-update/010b-status.md`

Primary class: user-facing distribution corrective.

Hard dependencies: none.

Operational closure dependency: a future published release must carry the corrected
POSIX installer before C027 can be fully `closed`. Implementation may become
`conditionally closed` after repository and hosted-platform evidence is green,
but the published v0.2.2 installer remains immutable and retains the old behavior.

C027 does **not** authorize publication, select a release version, or reopen
M010B. It corrects forward from M010B's accepted evidence.

## 1. Objective

Make the normal non-root Linux/macOS bootstrap install behave like the current
Gregg user-local install UX: after a successful install to
`$HOME/.local/bin/cargo-cleanme`, a supported shell should have a safe,
idempotent PATH integration for future shells without requiring the user to
manually edit a startup file.

The installer must still explain that a piped child process cannot mutate the
already-running parent shell. When the current process PATH does not contain the
install directory, it must print a one-line export for immediate activation.

The scope is deliberately narrow:

- POSIX `packaging/install.sh`;
- the canonical non-root destination `$HOME/.local/bin`;
- zsh and bash startup-file integration;
- deterministic fixture and hosted macOS/Linux evidence;
- installer/help/operator documentation that describes the behavior.

Windows `install.ps1` PATH semantics are unchanged by C027.

## 2. Defect being corrected

M010B required the POSIX installer to install to `$HOME/.local/bin` for a
normal user and then "report the resulting path and PATH guidance." The current
wrapper implements that requirement literally:

- it detects whether `$INSTALL_DIR` is on the current PATH;
- if not, it prints `export PATH="$INSTALL_DIR:$PATH"`;
- it never persists that integration into a shell startup file.

`docs/INSTALLING.md` makes the weak behavior explicit: the wrappers "never
modify your PATH" and only print the command.

That is internally consistent with M010B, but it is a user-facing defect in the
current first-install experience. On a normal macOS zsh account,
`$HOME/.local/bin` is not guaranteed to be on PATH. The installer therefore
reports success while a newly opened terminal may still fail to resolve
`cargo cleanme` until the user manually performs the printed configuration
step.

The comparison point is Gregg's current installer, which treats shell-profile
integration as post-install UX rather than installation identity: it safely
persists the canonical user-local bin directory for future supported shells and
still prints the current-shell export.

## 3. Why previous verification missed it

This corrective is required because the earlier plan and tests verified the
weaker contract rather than the desired user outcome.

1. **M010B specified guidance, not persistence.** The implementation satisfied
   the plan exactly, so there was no failing requirement to expose.
2. **The happy-path fixture uses `--dir <temporary path>`.** It verifies
   download, identity, placement, and that some PATH guidance was emitted, but
   it does not exercise the canonical non-root destination or a shell profile.
3. **The PATH assertion is textual.** In
   `packaging/tests/test_installers.py`, the happy path passes if stdout
   contains "not on your PATH" or "PATH". A printed instruction is therefore
   indistinguishable from a persistent integration.
4. **Hosted macOS evidence stops at installer success.** It does not start a
   fresh zsh login/interactive shell with a clean HOME and assert that
   `command -v cargo-cleanme` resolves.
5. **Documentation encoded the omission as policy.** Since
   `docs/INSTALLING.md` explicitly said the wrappers never modify PATH, review
   had no contradiction to flag.

Regression evidence must therefore prove the *post-install shell result*, not
merely the presence of guidance text.

## 4. Invariants and safety boundaries

C027 must preserve all existing installer security and release invariants:

- release target mapping remains a projection of
  `release/eggpack/distribution.toml`;
- binary-first acquisition, SHA-256 verification, candidate identity checks,
  404-only Cargo fallback, TLS policy, atomic placement, and no internal
  privilege escalation remain unchanged;
- shell-profile work happens **only after the verified binary has been placed**;
- profile integration failure is non-fatal to an otherwise successful binary
  installation, but must produce bounded manual guidance;
- no profile file is sourced, evaluated, executed, or command-substituted by
  the installer;
- no system-wide startup file is modified;
- root/system installs never modify a user's profile;
- arbitrary `--dir` values are never auto-persisted into a profile;
- unsupported shells fall back to manual guidance;
- a profile symlink is not followed by the installer;
- a non-regular or unwritable profile target is not rewritten;
- an existing active PATH integration is respected rather than duplicated;
- comments, prose, aliases, `echo` statements, or unrelated variables that
  merely mention `.local/bin` do not count as active integration;
- repeated installs are idempotent and must not append duplicate managed blocks.

C027 must not change Rust cleanup, discovery, update, configuration, JSON, exit
codes, Cargo ownership semantics, or Windows installer behavior.

## 5. Required POSIX PATH contract

### 5.1 Canonical user-local install

For a non-root install using the default destination:

```text
$HOME/.local/bin/cargo-cleanme
```

the installer should integrate `$HOME/.local/bin` for future supported shells
when it is absent from the current PATH and no active equivalent already exists.

The managed entry must be bounded and idempotent. A Gregg-style conditional
block is acceptable:

```sh
# added by cargo-cleanme installer: ensure user-local binaries are on PATH
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
```

Equivalent mechanics are acceptable if the tests prove the same behavior and
the installer remains POSIX-`sh` compatible.

### 5.2 zsh

For `$SHELL` resolving to zsh:

- if `ZDOTDIR` is a valid absolute path without a newline, target
  `$ZDOTDIR/.zshrc`;
- otherwise target `$HOME/.zshrc`;
- do not source the file;
- do not follow a symlink;
- create `.zshrc` only when its parent is an expected writable user-owned
  location.

The macOS closure evidence must include a fresh zsh process using the fixture
HOME that resolves the installed command from the persisted entry.

### 5.3 bash

For bash:

- Linux targets `$HOME/.bashrc`;
- macOS follows login-shell startup precedence: existing
  `$HOME/.bash_profile`, then `$HOME/.bash_login`, then
  `$HOME/.profile`; if none exists, create `$HOME/.bash_profile`;
- do not source or rewrite the selected file; append only a bounded managed
  entry when necessary.

Hosted macOS evidence must exercise at least the default create path and one
existing-profile precedence path.

### 5.4 Unsupported shells

For fish, nushell, elvish, unknown shells, or an unset/invalid `SHELL`, do not
guess syntax or startup-file semantics. Leave the binary installed and emit the
manual export guidance unless PATH output was explicitly disabled.

### 5.5 Custom install directories and system scope

A user-supplied `--dir` does not authorize startup-file mutation, even when it
is under HOME. The wrapper reports the exact directory and prints the immediate
export when needed.

An already-root/system install keeps the existing `/usr/local/bin` destination
policy and never modifies a user profile. If that directory is absent from PATH,
print manual guidance only.

## 6. CLI compatibility

The existing `--no-path` option currently means "do not print PATH guidance."
C027 must not cause an invocation that already opted out of PATH handling to
begin mutating a shell profile.

Required compatibility behavior:

- default: safe profile persistence for the canonical user-local destination,
  plus current-shell guidance when necessary;
- `--no-shell-profile`: suppress profile persistence but retain manual PATH
  guidance;
- `--no-path`: legacy broad opt-out; suppress both profile persistence and
  PATH guidance.

This intentionally strengthens `--no-path` from "no output" to "no PATH
side-effect and no output". That is the conservative compatibility direction:
an existing scripted caller that opted out will not acquire a new startup-file
mutation after upgrading.

Help text and installer documentation must state the distinction.

## 7. Work package A — Add bounded shell-profile integration

Refactor the final PATH handling into explicit helpers rather than growing the
current trailing case statement.

The implementation should have separable operations for:

- checking whether the destination is already on PATH;
- selecting a supported startup file;
- recognizing an existing active canonical user-local PATH entry;
- appending the managed entry safely and idempotently;
- printing current-shell/manual guidance;
- deciding whether profile persistence is permitted for this invocation.

Keep these helpers shell-only. Do not introduce a Rust helper binary or make
installation depend on cargo-cleanme being runnable before PATH integration.

## 8. Work package B — Strengthen installer fixture premises and cases

Extend `packaging/tests/test_installers.py` so PATH behavior is tested as a
state transition rather than as text output.

Required POSIX cases:

1. canonical user-local zsh install with a temporary HOME creates the selected
   `.zshrc` entry;
2. a repeated `--force` install does not duplicate the managed block;
3. a pre-existing active user-authored canonical PATH entry suppresses the
   managed append;
4. a commented-out entry or prose mentioning `.local/bin` does **not**
   suppress the append;
5. valid absolute `ZDOTDIR` selects its `.zshrc`;
6. invalid/relative `ZDOTDIR` falls back to HOME;
7. Linux bash selects `.bashrc`;
8. hosted macOS bash obeys
   `.bash_profile -> .bash_login -> .profile -> create .bash_profile`;
9. `--no-shell-profile` leaves the profile untouched and emits guidance;
10. `--no-path` leaves the profile untouched and emits no PATH guidance;
11. `--dir <custom>` leaves profiles untouched;
12. unsupported/unset `SHELL` leaves profiles untouched and emits guidance;
13. a symlink/non-regular/unwritable profile target is not followed/rewritten;
    installation remains successful and guidance names the manual action;
14. a profile-append failure after verified placement does not remove the
    installed binary.

Tests must isolate HOME/ZDOTDIR and must never touch the CI account's real shell
configuration.

If a deterministic test seam is needed to exercise system-scope or Darwin-only
selection from another host, it must affect only PATH-integration policy. It
must not bypass transport, digest, candidate identity, or placement checks.

The fixture's self-test must gain a failing-direction premise if the new cases
could otherwise pass without touching the intended profile.

## 9. Work package C — Prove the macOS user outcome

The hosted macOS installer lane must contain a real fresh-shell assertion using
a fixture-owned HOME.

At minimum, after the canonical user-local fixture install:

```text
zsh -lic 'command -v cargo-cleanme'
```

or an equivalent clean zsh invocation must resolve the exact installed fixture
binary through the profile entry written by the installer.

The case must fail if:

- the profile was not selected;
- the managed entry was not written;
- the entry is syntactically inert;
- a different `cargo-cleanme` wins PATH precedence.

Do not accept "the profile contains the expected text" as the only macOS proof.

## 10. Work package D — Documentation and contract reconciliation

Update the current user-facing and architecture surfaces that intentionally
describe the old behavior:

- `packaging/install.sh --help`;
- `docs/INSTALLING.md`;
- README installer guidance if it states or implies manual-only PATH setup;
- `docs/TROUBLESHOOTING.md` if a PATH-resolution troubleshooting entry needs
  the new profile semantics;
- `architecture/15-distribution-and-release.md`;
- `CHANGELOG.md` under the unreleased section.

Do not edit M010B's closed plan or closure record. C027 is the forward
correction.

Do not hand-edit Cargo CLI completions/manpages: C027 changes installer flags,
not the Rust clap surface.

## 11. Verification

Run the ordinary repository ladder in order:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
```

Then run the distribution-specific gates:

```sh
sh -n packaging/install.sh
python3 packaging/tests/test_installers.py --self-test
python3 packaging/tests/test_installers.py
python3 scripts/check-installer-contract.py --self-test
python3 scripts/check-installer-contract.py
python3 scripts/check-release-contract.py --self-test
python3 scripts/check-release-contract.py
```

Hosted evidence must include Linux and macOS installer lanes. Windows must remain
green to prove C027 did not regress the untouched PowerShell path.

If implementation changes a static checker, its failing-direction self-test is
mandatory before the positive run.

## 12. Acceptance criteria

C027 implementation is ready for conditional closure only when all of the
following are true:

1. a default non-root macOS zsh install persists `$HOME/.local/bin` safely and
   a fresh shell resolves the exact installed fixture binary;
2. Linux zsh/bash behavior is deterministic and covered;
3. profile integration is idempotent;
4. an existing active integration is respected without duplicate mutation;
5. custom directories, system installs, unsupported shells, and unsafe profile
   targets are not auto-mutated;
6. `--no-shell-profile` and `--no-path` semantics are pinned by tests;
7. profile-integration failure cannot roll back or misreport a successfully
   verified binary placement;
8. existing installer integrity/fallback/identity/security tests remain green;
9. hosted Linux/macOS/Windows installer lanes are green;
10. documentation no longer claims the POSIX wrapper is manual-PATH-only.

Full closure additionally requires a published immutable release carrying the
corrected `install.sh`, followed by public-release evidence that the shipped
installer preserves the hosted macOS fresh-shell result. Until then the
published v0.2.2 wrapper remains the old behavior and C027 must not be marked
`closed`.

## 13. Stop conditions

Stop and register a separate plan rather than expanding C027 if implementation
requires any of the following:

- changing Windows environment-variable persistence;
- modifying arbitrary system shell startup policy;
- auto-integrating arbitrary `--dir` values;
- changing the install destination to `$CARGO_HOME/bin`;
- changing update provenance or self-update behavior;
- adding support for shells beyond zsh/bash;
- changing the release target matrix;
- weakening installer integrity, TLS, identity, or fallback rules;
- publishing a release solely because the implementation landed without the
  normal release qualification process.

## 14. Closure evidence requirements

The C027 closure record must contain:

- implementation commit/PR range;
- requirement-to-evidence matrix for §12;
- exact POSIX fixture cases and their results;
- the hosted macOS fresh-shell command and the path it resolved;
- hosted Linux/macOS/Windows installer lane results;
- proof that a duplicate managed block is rejected by the idempotency case;
- proof that `--no-path`, custom `--dir`, and unsafe-profile cases leave the
  fixture profile unchanged;
- every verification command actually run;
- documentation surfaces updated;
- known limitations;
- disposition of any new finding;
- the published release/tag carrying the fix before a final `closed`
  disposition.

Until that last item exists, the correct maximum disposition is
`conditionally closed`.
