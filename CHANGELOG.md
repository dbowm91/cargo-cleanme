# Changelog

All notable changes to cargo-cleanme are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with a pre-1.0
stance: the `0.x` line may make breaking changes in any release, and the
command-line, JSON, and release-asset contracts are the stable surface.

## [Unreleased]

### Fixed — discovery and maintenance scope

- Full traversal errors without a child path are now reported as bounded,
  unknown-location coverage loss instead of being attributed to the selected
  root. A partial Full inventory keeps positive manifests but does not publish
  a complete learned-state generation and exits 1.
- Only Cargo-resolved workspaces in durable developer locations contribute new
  automatic Routine roots. Recognized Cargo/npm/Go caches, Trash, temporary
  trees, `node_modules`, and transient Codex worktrees remain visible to Full
  discovery but are omitted from Routine selection; explicit scans remain
  available.
- Cleanup now resolves discovered manifests before returning an incomplete
  discovery block, so reported workspace and cleanup-unit counts reflect work
  actually attempted. Discovery uncertainty still blocks the whole cleanup
  scope and produces no `cargo clean` spawn.

## [0.2.4] - 2026-10-08

### Fixed — discovery and cleanup scope

- **A deleted, automatically learned scan location no longer aborts otherwise
  safe Routine maintenance.** Bare `cargo cleanme` failed with
  `invalid scan root <path>: No such file or directory` when a previously
  learned directory — for example a removed worktree — no longer existed, and
  no other root was processed. Automatically seeded and learned roots are now
  admitted by provenance before any collapse: positively absent (`ENOENT`) and
  positively non-directory roots are omitted from the current invocation with
  a bounded diagnostic, while symlinks (never followed, including broken
  links) and unreadable/indeterminate roots block the run with a typed
  whole-scope report instead of a fatal error. Explicit roots (`scan.root`,
  `scan ROOT`, `clean ROOT`) keep strict fatal semantics, every destructive
  candidate is still proven against one fresh combined ownership universe with
  a late pre-spawn recheck, Routine omission never rewrites learned state,
  and a complete certain Full reconciliation now prunes definitively
  nonexistent learned directories independent of retention.

## [0.2.3] - 2026-10-07

### Fixed — distribution

- **The POSIX installer reported success while a new terminal could not find the
  binary.** `packaging/install.sh` installs a normal user account's copy into
  `$HOME/.local/bin`, which is *not* on `PATH` by default on a stock macOS
  account. The installer detected that, printed `export PATH=…` for you to run
  yourself, and exited 0 — so a first install left a working binary that a newly
  opened terminal could not resolve until the user performed the printed step by
  hand.

  The installer now appends one bounded, guarded block to a single startup file
  **after** the verified binary is in place, so a future shell works without
  manual work:

  ```sh
  # >>> cargo-cleanme installer PATH >>>
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) export PATH="$HOME/.local/bin:$PATH" ;;
  esac
  # <<< cargo-cleanme installer PATH <<<
  ```

  It is idempotent, so re-running the installer adds nothing. The block is
  removed by deleting the markers and everything between them.

  The installer **cannot** change the `PATH` of the shell that invoked it — a
  piped child cannot mutate its already-running parent — so it still prints the
  export line for the current shell. Open a new terminal, or paste it yourself.

  Which file is used: `$ZDOTDIR/.zshrc` when `ZDOTDIR` is an absolute path and
  `$HOME/.zshrc` otherwise for zsh; `$HOME/.bashrc` for bash on Linux; and on
  macOS the first existing file among `$HOME/.bash_profile`, `$HOME/.bash_login`,
  `$HOME/.profile`, else a new `$HOME/.bash_profile`, because Terminal there
  starts a login shell.

  Nothing is written when it should not be: a `--dir` destination (your explicit
  choice), a root/system install, a shell that is not zsh or bash, an unset or
  invalid `ZDOTDIR`, or a target that is a symlink, a directory, a FIFO, or
  unwritable. The installer never sources, evaluates, or command-substitutes the
  file it writes, never creates a directory to drop a dotfile into, and never
  follows a symlink. A failed write prints the manual step and leaves the
  already-verified binary installed — it never rolls a working installation back
  because a dotfile could not be appended.

- **`--no-shell-profile` is new, and `--no-path` now suppresses more than it
  did.** `--no-shell-profile` installs the binary and prints the manual PATH
  guidance but writes no startup file. `--no-path` is the broad opt-out: no
  startup-file change *and* no PATH output. That is a deliberate strengthening
  for existing scripted callers — one that already opted out of PATH handling
  will not acquire a new startup-file mutation after upgrading.

  `install.ps1` and all Windows environment-variable handling are unchanged.

  Qualified by a fixture suite that asserts the state transition rather than the
  presence of guidance text (the earlier assertion passed on a printed
  instruction), each case shown to fail against a mutated wrapper, plus a hosted
  macOS lane that starts a fresh `zsh` with a fixture-owned `HOME` and asserts
  it resolves the exact installed binary — and asserts the same shell does *not*
  resolve it when the profile entry is removed.

## [0.2.2] - 2026-10-07

### Fixed — matching

- **A config `exclude` naming a directory with `[` or `]` in its canonical path
  did not exclude it on Windows.** The canonical rewrite that makes a
  symlink-spelled pattern work was gated on an escape it could not produce:
  `escape_glob_literal` rejected `\`, and every canonical Windows path contains
  one, so on Windows the rewrite never ran at all. A directory named
  `real[abc]` — legal on NTFS — was therefore spliced in unescaped and read as a
  character class, so the pattern stopped matching the tree the user named.

  **Read this if you use `exclude` on Windows:** an exclusion that silently
  fails is the direction that matters. A project you excluded could still be
  cleaned. Every other guard is unchanged — cleanup is still opt-in, still
  `cargo clean` through Cargo, still gated on an ownership proof — but your
  stated boundary was not being applied. This has been true since before 0.1.0.

  Three things were wrong together, and all three are fixed. The canonical
  spelling is now normalized to `/` before it is escaped, because that is how
  `globset` spells a candidate on every platform: `Candidate::new` normalizes
  candidate separators, and with `backslash_escape` off a `\` in a pattern is
  compiled to a `/` as well. A Win32 verbatim prefix (`\\?\C:\…`), which
  `fs::canonicalize` can return, is stripped — it is an API artefact rather than
  part of the path, and its `?` is a glob metacharacter. And a trailing `\` is
  treated as a dangling escape only where `\` actually escapes, so the ordinary
  `C:\dev\proj\*` is rewritten instead of refused.

  A `{` or `}` in a canonical name still has no escapable form and is left as
  written. `scan.unignore` is a literal path by design and is still spliced
  unescaped. Existing patterns that contain no backslash and no brace are
  byte-identical after the rewrite, so no configuration needs changing.

  Covered by a Windows-lane test that builds a real bracketed directory, a
  junction alias and a `config.toml`, plus an every-lane test that pins the
  Windows spelling and asserts — in the same test — that the unescaped reading
  misses the named tree and matches a sibling. `docs/USAGE.md` now states the
  rule and its platform scope.

## [0.2.1] - 2026-10-06

### Security

- **0.2.0 is yanked.** Two destructive defects shipped in 0.2.0 and were found
  by post-release interrogation, not by the tests. It could delete files it did
  not own, and it reported `status=ok` with exit `0` when it did. Yanking stops
  new `cargo install` resolution from selecting it; existing installations and
  lockfiles are unaffected. Every earlier release remains unyanked — their other
  behaviour was correct, and a yank would have misdescribed them. See
  [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md#known-defects-by-version)
  for what to check if you ran 0.2.0.

### Fixed — safety

- **`--dry-run` before the subcommand no longer deletes.** `cargo cleanme
  --dry-run clean ROOT` — the spelling Cargo itself passes, and the one users
  type — parsed cleanly, discarded the flag, and ran a real `cargo clean`,
  reporting `status=ok` and exit 0. The root-level flag is now honoured on the
  `Clean` and `Update` arms wherever it appears. `cargo cleanme --dry-run update`
  reached `run_update(false)` by the same route. The root flag is still not
  `global = true`, because that would collide with `update --dry-run`'s own
  argument id; `invocation()` reconciles the two instead. When a root
  `--dry-run` meets `clean --cargo-preview`, simulation wins: it is the mode
  that spawns no Cargo process at all.

- **A second workspace's source tree inside a covering root is no longer
  deleted.** The source-disjointness check consulted only the group's own
  owner's members. An outer workspace whose output directory was not literally
  named `target`, containing a real nested project, was classified `PrivateBounded`
  and cleaned — destroying the inner project's `Cargo.toml`, `Cargo.lock`,
  `src/`, and `.cargo/config.toml` with exit 0 and no diagnostic. The check now
  walks every resolved workspace's members, and the preflight proof independently
  refuses when another workspace's source tree lies inside the cleaned region.
  The diagnostic text is now "contains a workspace source tree" rather than "is
  the workspace source tree", because the tree need not be the owner's.

### Fixed — correctness and reporting

- **On macOS, the `/usr/local` root is actually walked again.** The platform
  policy lists `/usr/local` as an exception to the `/usr` prune, and both halves
  of that exception were missing: the canonical collapse folded `/usr/local`
  into `/`, and — had it survived — `entry_is_within` matched on `parent_path`,
  so the `/usr` prune covered every entry below the `/usr/local` root.
  Composition now protects a root that sits strictly inside a global-only prune,
  and the prune exempts a walk root strictly inside it. The certifying test
  asserted on a private pre-collapse helper and so passed while production
  walked nothing; it now calls the production composition.

- **Cargo's stderr is reported on the two failure paths.** A manifest that
  `cargo locate-project` or `cargo metadata` refused produced a bare label —
  `cargo locate-project failed` — with no way to learn why a project was missing
  from every report. Both now carry a bounded first line of Cargo's stderr, the
  same way the adjacent "could not start" and "cannot parse" paths already did.
  No safety impact: these projects were refused, never deleted.

- **`--format log` emits a line for `config` and `update`.** `op` is documented
  to admit both, but neither had a `Log` branch, so `--format log config path`
  printed a raw path on stdout and `--format log config show` printed the config
  file — exactly what the format promises never to retain. Both now report the
  action; the content is still available in human and JSON form.

- **Config glob metacharacters can no longer be injected by the canonical
  rewrite.** `load` validated each pattern and then spliced the canonical
  spelling of its prefix in unescaped, so a directory named `real[abc]` reached
  through a symlink turned `link/*` into a character class: the rule stopped
  excluding the tree the user named and began excluding two they never wrote
  down. The canonical spelling is now bracket-escaped, spellings with no portable
  escape are left untouched, and the rewritten patterns are re-validated.
  `scan.unignore` is a literal path by design and is still not escaped.

- **Duplicate learned roots are folded.** A workspace with N members
  contributed N observations resolving to one root, and the containment collapse
  started at the parent, so the identical path was published N times. Behaviour
  was inert (both consumers dedup first); the state file is what an operator
  reads to judge whether the tool "knows" their projects.

- **`active_skipped` counts what it claims to.** It counted *workspaces* while
  `stats_line` compares it against `groups_measured`, so a workspace owning two
  skipped groups reported `active_skipped=1`; every skip counter is now
  per-group, like the rest of that line. `has_artifact_entries` returning `None`
  for EACCES/ELOOP is still folded into `MissingOutput` rather than reported as
  unreadable — fail-safe (the group is skipped, never deleted), already recorded
  as a deliberate trade-off in `architecture/07-workspace.md`, and changing it
  needs a new reason code.

### Changed — internal

- Four `u128 → u64` truncating casts on user-visible timing counters now call
  `domain::elapsed_nanos`, the last inconsistent users of a rule the crate states
  in `domain.rs`.
- `update`'s `cleanup` returns its failure instead of discarding it. A staging
  directory that survives removal (up to 128 MiB) is now reported rather than
  papered over with `updated to <v>` and exit 0.
- `staging_dir` uses `fs::create_dir`, so "this never adopts an existing
  directory, it fails instead" is now what the code does rather than what the
  comment says; the test fixture no longer pre-cleans the hazard its subject
  should refuse.
- First-use config creation falls back to an exclusive `create_new` publish when
  the filesystem has no hard links (FAT32/exFAT, some CIFS mounts, WSL `/mnt/c`),
  which previously made every command fail at startup. The never-overwrite-a-
  concurrent-winner guarantee is kept; crash-atomicity of the contents is the
  thing traded away, and a truncated config is refused rather than read as
  defaults.
- The cleanup progress bar is cleared on the failure path too: `BarState::drop`
  finishes a bar rather than clearing it, so a `?` that returned first left a
  retained line above the error message.
- Traversal: the nested-repository probe is one shared, memoised definition
  instead of four duplicated `read_dir` closures; the physical-group index is
  built once per group set instead of once per workspace; and the
  owns-a-non-empty-group test is a single pass instead of a scan per workspace.
- Corrected stale documentation: `AGENTS.md`'s line count, release count, and
  test counts; its blanket "all 17 scripts take `--self-test`" claim, which was
  true of 11; the second provenance gate in `update.rs`, which is unreachable
  defence in depth rather than the gate that decides; a comment citing a test
  that does not exist; `HostUnsupported`'s doc comment; the
  `distribution.toml` test, which panicked on a published crate that does not
  ship that file; and the `TERM=dumb` progress test, which asserted on a
  function that returns false in CI regardless and so passed with the branch it
  claimed to cover deleted.

## [0.2.0] - 2026-10-06

### Changed — BREAKING

- **`cargo cleanme` is now the canonical maintenance command, and it cleans.**
  Bare invocation was a read-only Routine scan; it is now a Routine cleanup over
  the maintenance scope through the same combined-root safety engine
  `clean --known` already used. Every candidate must still prove exclusive
  ownership of its bytes, and that proof is repeated immediately before each
  `cargo clean`. Deletion is still always `cargo clean`; cargo-cleanme never
  removes a directory itself.

  | 0.1.x | Now | Same behaviour? |
  |---|---|---|
  | `cargo cleanme` | `cargo cleanme scan --known` | yes |
  | `cargo cleanme scan --full` | `cargo cleanme scan` | yes |
  | `cargo cleanme clean R --dry-run` | `cargo cleanme clean R --cargo-preview` | yes |
  | `cargo cleanme clean R --dryrun` | `cargo cleanme clean R --dry-run` | yes |
  | `cargo cleanme clean R --yes` | `cargo cleanme clean R` | yes |

  `--dry-run`, `--dryrun`, and `--yes` conflict pairwise on purpose.

- **`--dry-run` now means simulation, not Cargo preview.** `cargo cleanme
  clean --dry-run` used to invoke `cargo clean --dry-run --verbose`; it now runs
  the complete decision, proof, and reporting path and spawns **no**
  `cargo clean` process of any kind. Cargo's own preview is reachable as
  `cargo cleanme clean --cargo-preview`. Simulation is not a weaker mode: it
  passes the same completeness and final-proof gates as a real cleanup, so it
  remains the right mode for testing a change to those gates.

- **`clean` now defaults to Execute.** `--yes` is retained as a hidden alias
  that prints nothing.

- **Rootless `scan` is now Full and no longer needs `--full`.** A configured
  `scan.root` is *not* consulted by a rootless scan, so configuration cannot
  silently narrow the canonical reconciliation command. `scan --full` is
  retained as a hidden alias for the old spelling. `scan --known` is the
  read-only Routine inventory, and `scan ROOT` remains the explicit bounded
  scope. `ROOT` with `--known`/`--full`, and `--known` with `--full`, are usage
  errors.

- **`--dryrun` and `--yes` are hidden.** Both remain accepted on `clean` as
  aliases for the canonical `--dry-run` and default Execute modes. They create
  no fourth mode and print nothing, so an unattended run stays quiet.

### Added

- **`--format log`: one bounded ASCII summary line for unattended runs.** A
  scan or cleanup emits exactly one line on stdout — `op`, `status`, `scope`,
  `mode`, a typed `reason=` code when blocked, and counts — at most 384 bytes,
  with no paths, no Cargo stderr, and no progress output. Progress is disabled
  and per-diagnostic stderr fan-out is suppressed; `--stats` remains an opt-in
  stderr override. This is an operational summary, not a machine contract:
  `--format json` remains the complete versioned one.

  → [docs/AUTOMATION.md](docs/AUTOMATION.md) for the field list, a worked
  greggd configuration, scheduler-neutral alternatives, and exit-code triage.

- **A typed reason code for a scope block.** A blocked cleanup scope now carries
  `ownership_unproven` or `incomplete_discovery` alongside the prose a human
  reads, so an unattended log line can be grepped and alerted on without
  matching English text. JSON is unchanged and still reports the prose.

### Fixed

- **A configured `scan.root` made `clean --known` a silent no-op.** With
  `scan.root` set, the maintenance scope resolved to that root as an explicit
  override, and cleanup then discarded it and ran with no roots at all,
  reporting a successful zero-result cleanup of nothing. It now cleans exactly
  the root the scope resolved to. Bare maintenance, `clean --known`, and
  `scan --known` all share one scope resolution.

- **`clean --full` returned the scan's exit code silently.** An incomplete Full
  reconciliation now aborts cleanup with a typed configuration error instead of
  returning a bare non-zero code that looked like a cleanup failure.

- **`clean --full` could emit two JSON documents on stdout.** The scan run to
  refresh learned state wrote its report ahead of the cleanup report. It is
  suppressed again, so one invocation still emits exactly one document.

- **`scan.unignore` re-admitted every sibling under a literally ignored
  directory.** With

  ```toml
  [scan]
  ignore   = ["/home/you/projects/archived"]
  unignore = ["/home/you/projects/archived/keepme"]
  ```

  and both `archived/keepme` and `archived/other` present, discovery reported
  **2** manifests instead of 1. The walk decided each directory on its own text
  alone, so an `unignore` below an ignored directory forced the walk back in —
  and a sibling that did not itself match the ignore pattern came in with it.
  Append `/**` to the `ignore` pattern to work around it.

  The walk now carries the excluded state a directory inherits from an ignored
  ancestor: the ancestor is entered only as far as the exact `unignore` path
  requires, and every other branch beneath it is refused. `keepme` and any
  projects nested beneath it are discovered; `other` is not.

  **0.1.6 and earlier are affected** and keep the broader behaviour. The `/*`
  and `/**` forms work on every release and are unchanged. No configuration
  migration is required — configurations using a literal ignored ancestor with an
  exact `unignore` simply become narrower, which is what they asked for.

  Scope selection is the only thing that changed. Ownership resolution,
  authorization, activity, marker, and final cleanup proof are untouched, and an
  explicit scan root still bypasses `ignore`/`unignore` entirely.

- **`cargo cleanme update` could replace a binary that Cargo owns, because an
  unreadable Cargo record read as "no record".** The updater asked its Cargo-home
  bookkeeping for the installed version and got back `None`, which meant four
  different things at once: no record, unreadable record, malformed record, and a
  good record that simply does not mention this package. It could not tell "Cargo
  says nothing here" from "I could not read what Cargo would have said" — so a
  record that failed to parse fell through to hashing the binary, and the
  installation was classified **self-managed**, which is the classification that
  authorizes replacement. `cargo install --list` and uninstall bookkeeping were
  then left describing a version the file no longer had.

  A state that means *I could not find out* is never read as a state that means
  *there is nothing there*. Unreadable, malformed, unsupported, and
  non-UTF-8 evidence is now reported as **uncertain** and refuses the update,
  naming the reason. A mutating run on forbidden provenance now refuses **before**
  any registry request is made, so a refused update contacts crates.io zero times.
  Positive self-managed evidence — exact SHA-256 of the current bytes, revalidated
  under Eggup's mutation lock immediately before replacement — is unchanged.

  **0.1.6 and earlier are affected**; C017 is the same defect reached by a
  different route (a `.crates.toml` schema no current Cargo writes), disclosed
  there and in `docs/TROUBLESHOOTING.md`. If your binary was replaced
  unexpectedly, reinstall it with `cargo install cargo-cleanme --locked`.

- **Concurrent first use could fail on Windows with "Access is denied."** On the
  first run that creates `config.toml`, the file is staged under a temporary name
  and published with a single hard link, so exactly one racer should win. The
  retry logic tolerated exactly one spelling of "somebody already has that name":
  `AlreadyExists`. POSIX always reports that. **Windows reports `PermissionDenied`
  for a file another thread is still creating** — the name is reserved before its
  metadata is committed — so the losing threads never advanced to their next
  attempt and the first run returned a hard error. One racer failing is enough to
  fail the caller, so `scan` could exit non-zero because an editor, a shell
  prompt, and a CI step reached the config at the same instant.

  The staging name is now unique **within the process**, so the collision is
  removed rather than tolerated: production code no longer has to treat an
  ambiguous `PermissionDenied` as "try again". Every racer succeeds and the
  published bytes are one complete template.

### Fixed (test and release evidence only)

- **`cargo cleanme update --format json` had no contract evidence at all.**
  It is the one machine-readable surface that does not use the shared envelope,
  and nothing asserted its schema, its dry-run/changed semantics, its provenance
  code, or its stdout shape. The document is unchanged — this is the test and
  the seam, not a schema change — but it is now pinned, and the binary prints
  the same bytes the tests hold.

- **The installer-contract release guard could not prove it could fail.** Every
  other contract checker shipped a `--self-test` that requires it to reject a
  broken input; this one had no argument parsing at all, so its green was a
  statement about the current tree and never about itself. It now runs against
  isolated fixtures and must reject each defect class it claims to guard, for
  the intended reason. CI, the release-drift guard, and `release-check.sh` all
  run that self-test before the check. No installer or release contract changed.

## [0.1.6] - 2026-10-05

This release changes no product behavior. It exists so that the fix shipped in
0.1.5 can be exercised against two real published releases.

0.1.5 refuses to replace a binary that Cargo owns, but proving that a published
binary does so requires an installation that is *behind* the published version:
when a Cargo-managed binary is already current, the tool reports "already at the
latest stable version" and never reaches the ownership check, because there is
nothing to do. 0.1.6 supplies that missing condition, exactly as 0.1.4 supplied
it for the 0.1.3 updater fix.

If you are on 0.1.3 or later, you can reach this release with:

```sh
cargo cleanme update
```

## [0.1.5] - 2026-10-05

This release fixes the only defect in the 0.1.x line that could **change a file
it did not own**.

### Fixed

- **`cargo cleanme update` could replace a binary that Cargo owns.** A binary
  installed with `cargo install --root DIR` — the form every hermetic install
  script, CI job, and container image uses — was not recognised as
  Cargo-managed, because detection only ever looked at `$CARGO_HOME/bin`. The
  binary was therefore treated as a self-managed installation and **overwritten
  in place**, with no warning and exit status `0`. Afterwards `cargo install
  --list` reported the old version for a file that no longer had it, so
  `cargo upgrade` and `cargo uninstall` could no longer do their jobs.

  Reproduced against published v0.1.3:

  ```sh
  cargo install cargo-cleanme --version 0.1.3 --locked --root /tmp/root
  /tmp/root/bin/cargo-cleanme update --no-progress
  # cargo-cleanme: updated to 0.1.4      (exit 0; the file is now the release asset)
  # /tmp/root/.crates.toml still records 0.1.3
  ```

  `cargo cleanme update` on such an installation now refuses, leaves the bytes
  untouched, and prints the command that is actually correct:

  ```sh
  cargo install cargo-cleanme --locked --force
  ```

- **Cargo-managed installations were refused for the wrong reason.** The
  `.crates.toml` parser read a `[packages.<name>].vers` schema that no current
  cargo emits; cargo writes a flat `[v1]` table keyed by the full spec string. The
  parse therefore always failed, and a genuinely Cargo-managed binary was told to
  "reinstall with the published installer" — the exact provenance confusion this
  check exists to prevent.

  In this release a current Cargo-managed binary that is already up to date still
  reports "already at the latest stable version" instead of a refusal. Nothing is
  downloaded or written in that case, so it is safe; it is noted rather than
  changed.

### Known issue in earlier releases

v0.1.0 through v0.1.4 all carry both defects above. A Cargo-managed installation
on those versions can be silently replaced by `cargo cleanme update`. Upgrading
to 0.1.5 fixes it; `cargo install cargo-cleanme --locked` is the safe way to
move, since that route never overwrites a file it does not own.

## [0.1.4] - 2026-10-05

This release changes no product behavior. It exists so that the self-update
**commit** path can be exercised against two real published releases.

`0.1.3` fixed the updater, but proving a fix requires a released binary that
carries it *and* a newer published version to update to. Until 0.1.3 there was
no such pair, which is why the defect survived: the live commit path had never
succeeded in any published version, and no fixture could see it. 0.1.4 supplies
the missing half of that pair.

If you are on 0.1.3 or later, you can now reach this release with:

```sh
cargo cleanme update
```

## [0.1.3] - 2026-10-05

This release carries two defect fixes found after v0.1.2 was published. Both were
found by the post-release corrective line's own evidence work, not by a user
report: the relative-root defect by a release validator that had been green on
every CI lane, and the self-update defect by the live updater rehearsal.

### Fixed

- **`cargo cleanme update` could never complete a real commit.** The identity
  check ran the downloaded candidate with no arguments, so it compared the
  binary's default routine-scan report against a version string that can never
  match. The check therefore always failed — and, before failing, performed a
  filesystem scan of the whole machine. The transaction aborted safely every
  time: the live binary was left untouched and the error named the reason, so
  nothing was ever corrupted. But the feature had never worked in any published
  version, including `0.1.2`.

  Fixed by invoking the candidate with `--version`, which is bounded and
  produces exactly the identity the validator requires.

  The commit-path test fixture could not detect this: its candidate stub printed
  the identity for *any* invocation, so it could not tell a correct `--version`
  call from the argv-less one production actually made. The stub now answers only
  for `--version` and writes to stderr otherwise, exactly as the real binary
  does, so dropping the argument again turns the commit-path tests red. A fixture
  that accepts every input asserts nothing about the input.

- **A relative scan root resolved zero Cargo workspaces.** `cargo cleanme scan
  some/dir` reported no groups and exited 0, because the relative manifest path
  was passed to Cargo with the child's working directory set to that manifest's
  own parent, so the path did not resolve. The absolute spelling of the same
  directory was correct. The failure direction was fail-safe — a degraded scan
  reports *less* reclaimable space, so `clean` deleted less, never more — but it
  could read as a false all-clear.

  Relative and absolute roots now produce identical counters, groups, and bytes.
  The workaround noted in the `0.1.2` notes is no longer needed.

### Upgrading

Self-update works from this release onward. If you are on `0.1.1` or `0.1.2`,
`cargo cleanme update` cannot reach any version, so upgrade with Cargo:

```sh
cargo install cargo-cleanme --locked --force
```

or download the `0.1.3` asset for your platform from the release page.

## [Unreleased]

- **`cargo cleanme update` could never complete a real commit.** The identity
  check ran the downloaded candidate with no arguments, so it compared the
  binary's default routine-scan report against a version string that can never
  match. The check therefore always failed — and, before failing, performed a
  filesystem scan of the whole machine. The transaction aborted safely every
  time: the live binary was left untouched and the error named the reason, so
  nothing was ever corrupted. But the feature had never worked in any published
  version, including `0.1.2`.

  Fixed by invoking the candidate with `--version`, which is bounded and
  produces exactly the identity the validator requires.

  The commit-path test fixture could not detect this, and the reason is worth
  recording: its candidate stub printed the identity for *any* invocation, so it
  could not tell a correct `--version` call from the argv-less one production
  actually made. The stub now answers only for `--version` and writes to stderr
  otherwise, exactly as the real binary does, so dropping the argument again
  turns the commit-path tests red. A fixture that accepts every input asserts
  nothing about the input.

  Tracked as C016. The live rehearsal that proves the fix requires a published
  release carrying it; see §8 of that plan for why a `0.1.2` -> `0.1.3`
  rehearsal is expected to fail by design.

## [0.1.2] - 2026-10-04

This is a deliberately small qualification release. It changes no product
behavior: every change is to test evidence, release reproducibility, and the
release record. It exists so the live self-update path can be rehearsed against
a real public release, which was impossible while `0.1.1` was both the newest
publication and the first version containing a working updater.

### Changed

- The release builder is pinned to one exact Rust release, `1.99.0`, on all five
  contracted targets. It was `stable`, so each rebuild of a tag could have used a
  different compiler. `1.99.0` is the version that built and qualified `v0.1.1`,
  so the pin records an already-proven input rather than changing one. This is
  **not** an MSRV change: the declared MSRV remains `1.89`.

- A release tag is now bound to the source identity in the tree. The tag's
  version, `Cargo.toml`, `Cargo.lock`, the revision the tag points at, and a
  changelog entry must all agree before a release is staged. A stale or mistyped
  tag previously resolved cleanly and would have staged a draft whose tag and
  asset names disagreed with the version the binary reports.

- `scripts/release-check.sh` accepts the release tag as an argument and runs the
  identity gate, so the pre-publication gate and the tag are checked together.

### Fixed (test and release evidence only)

- The release-candidate smoke validator accepted a scan that discovered zero
  manifests and ignored diagnostics, so it reported a healthy candidate while the
  product's own Cargo integration was failing. It now asserts the exact manifest
  count, rejects any Cargo diagnostic, and probes the real Cargo first so a
  broken lane is reported as a lane failure rather than a product failure.

- The installer fixture cases for a missing binary, a missing Cargo, and a Cargo
  run that produces nothing asserted only a non-zero exit. A dead fixture server
  or an unrelated failure satisfied them. They now assert the wrapper's own
  diagnostic, so they prove the branch under test is the branch that ran.

- The two `cargo cleanme` cases no longer assume that Cargo resolves the binary
  they staged on PATH. The staged binary is resolved and compared before the
  behaviour assertions, and the resolution probe reads the child's environment
  rather than the test process's.

- A case that could not run — an unwritable-directory case running as root — is
  now reported as a skip instead of `ok`, and a host on which no installer block
  can run exits non-zero rather than reporting a green run that qualified
  nothing.

- A release validator with no `readelf` available no longer lets the glibc floor
  pass as "no version info found", and a failed `cargo tree` no longer records a
  dependency count of zero.

### Known limitations

- A known defect is **not** fixed in this release: an explicitly relative scan
  root (`cargo-cleanme scan some/dir`) silently resolves zero Cargo workspaces
  and reports no groups, because the relative manifest path is passed to Cargo
  with the child's working directory set to that manifest's own parent. The
  absolute spelling of the same directory is correct. The failure direction is
  fail-safe — a degraded scan reports *less* reclaimable space, so `clean` deletes
  less, never more — but the result can read as a false all-clear. Tracked as
  C015; use an absolute path until it is fixed.

## [0.1.1] - 2026-10-04

### Fixed

- `cargo cleanme update` could not reach the version authority. The production
  transport was `eggup-curl`, whose `CurlConfig` in 0.1.2 exposes no
  User-Agent seam and whose adapter clears the child environment, so every
  request went out as `curl/x.y`. The crates.io registry answers **HTTP 403** to
  any non-descriptive User-Agent, so `update` always ended in a transport
  failure. It now reports already-current correctly against the live registry.

  This was found by the 0.1.0 release smoke, not by the test suite: the fixture
  suite never talks to the registry, so no unit or fixture test could have
  caught it. The 19 updater tests were all green while the feature was
  non-functional in production.

  The transport is now `eggup-eggfetch`, the already-published Eggup transport
  whose `EggfetchConfig` has the `user_agent` seam the requirement needs. The
  curl path is not kept as a fallback: a fallback that cannot reach the
  authority is a second way to fail for no benefit.

- `cargo cleanme update` could offer to **downgrade** a build that is newer than
  the published release. Once 0.1.1 was published, a 0.1.1 install would have
  compared 0.1.1 against 0.1.0, taken the "newer version available" branch, and
  replaced itself with the older binary. The registry is the version authority
  for releases, not for a build that was never published, so a build newer than
  the published stable version now stops with a refusal instead of an offer.

  This was also found by the release smoke, for the same reason as the 403: the
  fixture suite drives a transport that always answers, so it could not express
  a registry that reports an older version than the one running.

- The release consumer validator now runs on every CI lane. It previously ran
  only inside the release workflow, so a Windows-only defect in the validator
  stayed invisible until a release was attempted.

### Changed

- The self-update transport is now an embedded HTTP/TLS stack rather than the
  external `curl` executable. This is a real cost and is not hidden:

  | Measurement | 0.1.0 (curl) | 0.1.1 (eggfetch) |
  |---|---|---|
  | Release binary | 5,420,664 bytes | 12,125,624 bytes |
  | Normal dependencies | 82 | 177 |

  `curl` was originally chosen because it is much smaller, and that reasoning
  was correct about size and wrong about qualification: a transport that cannot
  perform the required transaction is not a cheaper transport, it is a broken
  one. The original footprint argument should not have been used to close the
  question before a live registry request had ever been made.

### Known limitations

- `update` requires no external HTTP client now, but the binary is roughly twice
  the size it was. On a size-constrained platform this is the wrong trade and
  the updater should be a separate optional binary instead; that is registered
  as a follow-up, not resolved here.
- The bounded upstream gap is recorded rather than worked around: `eggup-curl`
  0.1.2 cannot set a User-Agent, so curl cannot be a viable production
  transport for any registry-backed updater. That request is named in the M010C
  closure record.

## [0.1.0] - 2026-10-04

Initial distribution-ready release candidate. Nothing in this entry is
published yet.

### Added

- Eggpack producer contract under `release/eggpack/`: the static distribution
  contract, build/qualification bindings, per-target consumer validator, GitHub
  runner policy, draft template, install policy, installer presentation, and a
  *derived* reusable workflow shape. The shape is generated from the other
  inputs by `scripts/gen-release-workflow-shape.py`, so it cannot drift into a
  second copy of producer facts.
- Eggpack-generated `release-binaries.yml` release workflow covering
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `x86_64-apple-darwin`, `aarch64-apple-darwin`, and
  `x86_64-pc-windows-msvc`, with a draft-only staging job. The workflow stages
  a draft; it never publishes, clobbers, or requests `id-token`.
- `release-drift.yml` CI job that installs Eggpack at a pinned Git revision and
  fails on either a generated-workflow drift or a hand-edited workflow shape.
- `scripts/check-release-contract.py`, which keeps the product wrappers,
  packaging payload, README target matrix, and Cargo package metadata aligned
  with the Eggpack contract instead of duplicating them.
- `scripts/smoke-release-candidate.py`, a portable per-target release smoke
  (identity, command surface, `cargo cleanme --version` argv normalization, and
  a bounded read-only JSON scan proven not to mutate the scanned tree).
- Explicit Cargo package contract: repository/homepage/documentation metadata,
  keywords, crates.io categories, a deliberate `include` allowlist that
  retains the embedded `config.toml` template, and dual `LICENSE-MIT` /
  `LICENSE-APACHE` files.
- `scripts/release-check.sh`, the single local gate for formatting, lint,
  tests, MSRV, package, and publish dry-run.

### Documentation

- Documented the prebuilt release target matrix, the deliberate Cargo-only
  hosts, and the current installation options.
- Documented the Rust library API stance: the library is published for
  tooling and testability but is **not** a stable third-party API before 1.0.
- Documented SHA-256 as integrity evidence only, not authenticity.

### Added (M010B)

- `packaging/install.sh` and `packaging/install.ps1`: product-owned,
  binary-first, fail-closed public installers. Latest or exact `X.Y.Z`
  selection, mandatory SHA-256 sidecar verification, `--version` identity check
  before placement, re-verification of the placed bytes, user-local default
  destinations, and no internal privilege escalation.
- Cargo source fallback that runs *only* when the host has no published binary
  or the selected release genuinely lacks it. Verification, identity, and
  transport failures never fall back. The fallback builds into a private
  temporary Cargo root, validates the built binary, and cleans up only its own
  state.
- `packaging/tests/`: a local fixture release server and 17 deterministic
  installer cases covering the verified install, exact-version install,
  documented Cargo fallback, and the negative paths for missing checksum,
  malformed digest, tampered payload, wrong product, wrong version, transport
  failure, existing destination, `--force` replacement, unwritable destination,
  malformed version syntax, absent Cargo, an empty Cargo result, and temporary
  state cleanup.
- `scripts/check-installer-contract.py`, which extracts the host-to-target
  mapping each wrapper actually implements and proves it is a projection of the
  Eggpack contract rather than a second release schema. Wired into the release
  drift gate, the release check, and the installer suite.

### Added (M010C)

- `cargo cleanme update`: bounded self-update built on the published Eggup
  crates. The registry is the version authority, the release tag is
  constructed as `vX.Y.Z` rather than scraped, and the candidate is replaced
  only after its `.sha256` sidecar verifies, the sidecar is confirmed to be
  evidence for that asset, and the candidate prints exactly `cargo-cleanme
  X.Y.Z`.
- Install provenance policy: a Cargo-managed installation is refused with the
  exact `cargo install cargo-cleanme --locked --force` command, because Cargo
  owns that file and its bookkeeping. Any other installation must be proven
  owned by its exact prior SHA-256, re-verified under a mutation lock
  immediately before replacement. Unprovable ownership fails closed.
- `eggup-curl` is the single production transport, so the updater adds no
  embedded HTTP/TLS stack. Its cost is measured: the release binary grows from
  4,970,912 to 5,420,664 bytes (+449,752, +9.0%) with three new crates and
  `eggup-acquisition` contributing zero transitive dependencies. The
  `eggup-eggfetch` alternative would add `eggfetch-core` plus rustls and was
  rejected on that measurement.
- 19 fixture-driven updater tests covering the version authority, tag
  construction, already-current, registry outage, all three provenance
  states, a successful verified replacement, checksum mismatch, wrong candidate
  identity, a sidecar naming a different asset, an absent asset, and staging
  cleanup on both the success and failure paths.

### Fixed

- The candidate was staged with `PermissionsIntent::Preserve`, which carries a
  freshly downloaded non-executable artifact through as non-executable. Every
  real update would have failed identity validation with a permission error.
  The intent is now `Executable`, which Eggup applies at staging time.

### Not in this release

- No crates.io publication and no public GitHub release have occurred. Because
  `cargo-cleanme` is not published, `cargo cleanme update` currently reports
  that the registry has no stable version, which is the correct fail-closed
  behavior rather than a defect.
- Release-manifest projection via `eggup-eggpack` is deferred: every target is
  a single direct artifact, so the tag plus the asset plus its sidecar digest
  is the whole update input. A real release workflow run is still required to
  prove end-to-end agreement with what Eggpack stages.
- No runtime release-artifact qualification has run; the exact release bytes
  are qualified when the first release workflow is dispatched.

[0.2.0]: https://github.com/dbowm91/cargo-cleanme/releases/tag/v0.2.0
[0.1.0]: https://github.com/dbowm91/cargo-cleanme/releases/tag/v0.1.0
