# cargo-cleanme troubleshooting

## Check this first: known defects by version

Every row below is a case where a correct-looking run did the wrong thing, and
none of them was found by a test — each was found by the project's own release
rehearsals or post-release interrogation. The `0.1.x` updater defects are the
oldest; the 0.2.0 defects are the reason 0.2.0 is yanked; the `exclude` defect
is the reason the table now leads with a matching rule rather than with an
action.

| If you are on | Check | Fixed in |
|---|---|---|
| **0.2.3 and earlier** | A deleted automatically learned scan location (for example a removed worktree) aborts bare `cargo cleanme` with `invalid scan root … No such file or directory` | **0.2.4** (C028; see below) |
| **0.2.1 and earlier** | An `exclude` naming a directory with `[` or `]` in its canonical path **did not exclude it** on Windows | **0.2.2** |
| **0.2.0** | `--dry-run clean ROOT` **performed a real cleanup** | **0.2.1** (0.2.0 yanked) |
| **0.2.0** | A region containing another project's **source tree was deleted** | **0.2.1** (0.2.0 yanked) |
| **0.1.1 – 0.1.4** | `update` could replace a binary **Cargo owns**, silently, with exit `0` | **0.1.5** |
| **0.1.1 – 0.1.2** | `update` could never complete a commit | **0.1.3** |

**0.2.0 is yanked, and it is the one release where that is the right call.**
Every earlier defect in this table was a *wrong or incomplete* action; the
program's other behaviour was correct, and yanking would have misdescribed
them. 0.2.0 could delete files it did not own, and it did so while reporting
success and exiting `0`. There is no honest description of 0.2.0 that also says
"everything else was fine", so it is yanked. Existing installations and
lockfiles are unaffected — a yank stops new resolution, it does not break what
you already have.

### If you are on 0.2.3 or earlier: a deleted learned location aborts bare cleanup

Bare `cargo cleanme` fails before doing anything:

```text
cargo-cleanme: invalid scan root /home/you/.codex/worktrees/1749: No such file or directory (os error 2)
```

That path is a transient developer tree the tool learned on an earlier Full
reconciliation and that has since been deleted. Every other root would have
been safe to process, but the stale advisory path reaches the strict
explicit-root validation and aborts the whole run with exit `2`. Nothing is
cleaned, and nothing is corrupted — the failure is availability, not safety.

Until you are on the release carrying the C028 corrective, work around it by
naming an explicit scope, which never consults learned state:

```sh
cargo cleanme clean ~/Projects --dry-run   # preview first
cargo cleanme clean ~/Projects              # then run it
```

Deleting `discovery-state.json` also unblocks the run (learned roots are
rebuilt by the next Full `scan`), at the cost of forgetting every learned
location. With the corrective in place, the stale location is omitted with a
diagnostic, symlinked or unreadable automatic locations block with a typed
report instead of a fatal error, and explicit roots stay strict.

## Full scans report partial coverage for unreadable subtrees

Full scans run with the current user's permissions. They continue inventorying
accessible paths, but `dua-core` does not attach the failing child path to its
traversal error event. The report therefore describes the skipped region as
unknown-location coverage loss rather than labeling the selected root as
unreadable. A partial Full scan exits `1` and does not publish a new complete
learned-state generation. Use an explicit accessible root to narrow the scan;
do not run as root just to suppress the diagnostic.

Full inventory and Routine maintenance have different purposes. A valid Cargo
manifest in a cache remains visible during Full discovery, but only a
Cargo-resolved workspace in a durable developer location can create a new
automatic Routine root. Existing cache/runtime roots are filtered during
Routine admission; explicitly named roots retain their strict semantics.

### If you are on 0.2.0: two things it could do that it should never have done

Both were found by the project's own post-release interrogation and confirmed by
reproducing them against the published binary. Both are fixed in **0.2.1**.

**1. `cargo cleanme --dry-run clean ROOT` deleted your build artifacts.**
`--dry-run` before the subcommand is the spelling Cargo itself passes to an
external subcommand, and it was accepted — then discarded. Cleanup fell through
to its default, ran `cargo clean` for real, and printed `Cleaned …` with exit
`0`. The words `--dry-run` were in your command line the whole time.

If you ran 0.2.0 in that form, the damage is bounded to build output; no source
file was touched. Rebuild with `cargo build`.

**2. A project could have another project's source tree inside its output
directory.** If workspace A declares an output directory (a redirected
`build.target-dir`, for instance) and an independently resolved workspace B has
its sources inside it, A's region was classified `private` and cleaned. B's
`Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, and `src/` were deleted along
with A's artifacts.

Check whether you have this shape before cleaning anything:

```sh
# every Cargo.toml on this machine, to find neighbours living in someone's output dir
find ~ -name Cargo.toml -not -path '*/.cargo/registry/*' 2>/dev/null
```

If a nested project does not exist in your layout, there is nothing to repair.

Until you are on 0.2.1, do not run `cargo cleanme` without `--dry-run`.

One breaking change arrived in **0.2.0**, listed separately below, because it
changes what a bare `cargo cleanme` *does* rather than fixing a defect. Detection
and repair steps for the `0.1.x` defects, plus the reasoning, are in
[UPDATE.md](UPDATE.md#known-defects-by-version).

Quick check for the first one, if you installed with `cargo install --root`:

```sh
cargo cleanme --version                      # the version the file reports now
cargo install --list | grep cargo-cleanme     # the version Cargo believes it owns
```

If those disagree, run `cargo install cargo-cleanme --locked --force`.

## Upgrading from 0.1.x: bare `cargo cleanme` now cleans

**Applies to 0.1.x only.** Bare `cargo cleanme` was a read-only Routine scan on
0.1.x. On 0.2.0 it is a Routine **cleanup**, and `--dry-run` on `clean` changed
from Cargo's own preview to application simulation.

| You wrote | On 0.2.0 it | Same behaviour? |
|---|---|---|
| `cargo cleanme` | `cargo cleanme scan --known` | yes |
| `cargo cleanme scan --full` | `cargo cleanme scan` | yes |
| `cargo cleanme clean R --dry-run` | `cargo cleanme clean R --cargo-preview` | yes |
| `cargo cleanme clean R --dryrun` | `cargo cleanme clean R --dry-run` | yes |
| `cargo cleanme clean R --yes` | `cargo cleanme clean R` | yes |

`--dryrun` and `--yes` still work on `clean`, hidden, mapping to the canonical
modes. Nothing in your config needs to change.

**If you want the old read-only behaviour unconditionally**, use
`cargo cleanme scan --known` and put it in the same place you used to run the
bare command. A configured legacy `scan.root` still wins as an explicit
override for bare maintenance, so a config that pins `scan.root` keeps cleaning
exactly what it always pinned.

## Unattended / scheduler runs

→ [docs/AUTOMATION.md](AUTOMATION.md) for the full integration.

### A scheduled run reports `cleaned=0` and nothing happens

Almost always **scope, not a bug**. Check in this order:

1. **Is the scheduler running as you?** The normal Linux greggd service runs as
   `greggd` with `ProtectHome=true` and generally cannot see your home
   directory, where the build output is. A successful run with an empty scope
   looks exactly like a successful run that cleaned something. Use a
   user-owned/rootless greggd.
2. **Is the path absolute?** `command = ["cargo-cleanme", …]` does not inherit
   your interactive `PATH`. Use the full path to the binary.
3. **Is `scan.root` set?** A configured `scan.root` is an exclusive override: it
   replaces the Routine scope entirely rather than adding to it.
4. **Is everything too recent?** Anything modified inside `recency_seconds`
   (default 300) is protected. A nightly job is far outside that window.

### A scheduled run exits `1` with `status=blocked`

The scope was refused **before** anything was deleted, by design. The `reason=`
code tells you which:

| `reason=` | Cause | Retry? |
|---|---|---|
| `ownership_unproven` | a discovered manifest could not be resolved, so coverage of the scope is unproven | usually no — fix the workspace |
| `incomplete_discovery` | discovery diagnostics made the ownership universe incomplete | usually no |

Both are *success-shaped* results: a typed blocker, not a crash. See
[USAGE.md §The three post-edit checks](USAGE.md) for the underlying safety
model.

### A scheduled run exits `2` with `status=error` and no summary line

The run failed before a report existed — almost always an invalid or unreadable
`config.toml`, or a root that cannot be used. There is no summary line because
there is no report to summarise. Re-run it in the foreground without
`--format log` to get the full message.

### A log line is missing a field you expected

Optional fields are dropped from the tail of the line when including them would
exceed 384 bytes. `op`, `status`, `scope`, and `mode` are structural and never
dropped. If you need complete data, use `--format json`.

## Discovery filters

### `unignore` re-admitted siblings (fixed after 0.1.6)

**Applies to 0.1.6 and earlier.** On those releases, a literal `ignore` plus an
`unignore` beneath it re-admitted the *whole* ignored subtree, so projects you
never meant to re-included showed up in the report — and a `clean` could then
consider them.

| `ignore` | `unignore` | 0.1.6 and earlier | fixed |
|---|---|---|---|
| `/home/you/projects/archived` | `…/archived/keepme` | **2** — over-broad | **1** — correct |
| `/home/you/projects/archived/**` | `…/archived/keepme` | **1** | **1** |
| `/home/you/projects/archived/*` | `…/archived/keepme` | **1** | **1** |

**Fix.** Upgrade past 0.1.6. The walk now carries the exclusion it inherits from
an ignored ancestor, so it enters the ancestor only far enough to reach the
exact `unignore` path and refuses the siblings. Nothing in your config needs to
change, and the `/**` and `/*` forms above keep working unchanged.

**If you cannot upgrade yet**, make the ignore pattern cover its own children by
appending `/**` (or `/*`):

```toml
[scan]
ignore   = ["/home/you/projects/archived/**"]
unignore = ["/home/you/projects/archived/keepme"]
```

This is correct on every release, but it is a workaround for a defect that no
longer exists — prefer the plain literal form once you are past 0.1.6.

### An `ignore` rule seems to do nothing

If you are testing with an explicit root, that is the reason:

```sh
cargo cleanme scan .                          # filters are BYPASSED
cargo cleanme scan --config ./filters.toml    # Routine scope: filters apply
```

`ignore` and `unignore` apply to global discovery and the Routine scope. An
explicit root — a CLI `ROOT` argument or the config's `root` key — bypasses
them, because a caller who names one directory means it.

### A relative path in your config is rejected

`configuration error: ignore pattern must be absolute: <value>`, exit `2`. Every
path in `ignore`, `unignore`, `root`, `include`, `exclude`, and
`allowed_output_roots` must be absolute.

### An `exclude` pattern does not match

`include` and `exclude` are matched against the **canonical workspace root**,
not against output directories or anything beneath it. A trailing `/**` requires
a segment after the name, so it never matches the directory itself:

```toml
exclude = ["**/archived/**"]   # does not match a workspace root that IS .../archived
exclude = ["**/archived"]      # matches
```

On **0.2.1 and earlier** an `exclude` naming a directory whose canonical path
contains `[` or `]` — legal in a Windows file name — silently failed to match
it, because the canonical rewrite refused every Windows spelling and so spliced
the name in unescaped: `real[abc]` was read as a character class. A project you
excluded could therefore still be cleaned. **Fixed in 0.2.2**: directory names in
a pattern are matched literally on every platform, and `docs/USAGE.md` documents
the rule. Before the fix, work around it with a prefix that resolves to a name
without brackets, or with patterns written against the canonical path.

## Installation

### `install.sh` says the host has no prebuilt binary

Musl targets, Windows ARM64, and ARMv7 have no prebuilt cargo-cleanme binary.
Build from the registry instead:

```sh
cargo install cargo-cleanme --locked
```

`install.ps1` behaves the same way on a non-x64 Windows host. This is
intentional: those hosts are Cargo-only, and the wrappers say so rather than
silently installing something else.

### `install.sh` says it could not reach the release host

The wrappers distinguish *absence* from *failure*. A `404` on the binary means
the release genuinely has no artifact and Cargo is used instead. A `5xx`, a TLS
error, or a timeout is a **hard failure** and will never fall back to Cargo,
because a release host outage must not silently become a locally built binary
with a different provenance. Retry later, or install from crates.io.

### `install.sh` says the checksum is missing or malformed

Every release asset ships a `.sha256` sidecar, and it is mandatory. A missing,
truncated, or non-hex digest stops the install. This is not a bug to work
around: it means the release evidence is incomplete, and installing bytes whose
integrity cannot be checked is exactly what the sidecar exists to prevent.

If the *payload* hash mismatches, the download is corrupt or was tampered with.
Nothing is installed. Re-run the installer.

### `install.sh` needs `curl` and a SHA-256 tool

Both `curl` and (`sha256sum` or `shasum`) must be present for a prebuilt
install. Without them, use `cargo install cargo-cleanme --locked`.

### The installer says the destination already exists

Re-run with `--force` (`-Force` in PowerShell) to replace it. Without that
flag the installer refuses rather than overwriting a file you did not ask it to
touch. Note that `--force` does not preserve the previous file's permissions.

### I want a system-wide install

The wrappers never call `sudo`, `su`, `doas`, or `Start-Process -Verb RunAs`.
Run the installer yourself from an elevated shell and it will use
`/usr/local/bin`, `%ProgramFiles%\cargo-cleanme\bin`, or the platform default.

A system-wide install never edits a user profile.

### `cargo-cleanme` is installed but a new terminal cannot find it

If the install directory is not on `PATH`, the POSIX installer prints the exact
command to add it for the current shell. It additionally appends a guarded block
to your zsh or bash startup file so new shells find it — see
[PATH integration](INSTALLING.md#path-integration). Two caveats:

- the block takes effect in **new** shells. The shell you ran the installer from
  cannot be changed by it, which is why the export line is printed too;
- if you see `could not add … (it is not a writable regular file)` or `could not
  write …`, nothing was changed and the binary is still installed. Add the
  directory to `PATH` yourself, or re-run with `--dir` pointing at a directory
  already on `PATH`.

## `cargo cleanme update`

### `update` refuses and prints a `cargo install` command

Your installation is Cargo-managed: the running executable lives in a Cargo bin
root that Cargo's own `.crates.toml` records. Cargo owns that file and its
bookkeeping, so replacing it from inside the tool would make
`cargo install --list` and uninstall describe a state that no longer exists.

Run the printed command instead:

```sh
cargo install cargo-cleanme --locked --force
```

This is the intended path for registry installs, not a limitation to work
around. It also means an update through Cargo picks up the full dependency
graph, which a single-binary replacement does not.

### `update` could not reach the registry

Current releases identify themselves to the registry with a descriptive
User-Agent, which crates.io requires. In `0.1.0` the updater used your system's
`curl` executable, which has no way to set one, so crates.io answered HTTP 403
and `update` always failed. `0.1.1` replaced it with a transport that can, and
every release since then uses that transport — so this failure mode is a
network or proxy problem on your side, not a known defect here.

If you see a transport failure, check outbound HTTPS and any proxy
configuration, then retry. It will not silently switch to a different transport,
and it prints the manager command to use instead.

### `update` says this host has no published binary

Same as the installers: musl, Windows ARM64, and ARMv7 are Cargo-only. Use
`cargo install cargo-cleanme --locked`.

### `update` says the installation is not provably ours to replace

`update` refuses to touch a file it cannot positively identify. It never infers
ownership from the file name, the directory it lives in, or `PATH` order. If
this happened to a real installation, reinstall it with the published installer
or via Cargo, then retry.

### `update` says it is already current

That is success. `update` only offers strictly newer stable releases, and it
refuses prereleases, because it cannot express "I accept that tradeoff" the way
an explicit `cargo install` can.

### `update` says this build is newer than the published release

Also correct. If you are running a build from `main` or
`cargo install --git`, its version is ahead of the latest published release.
The registry is the version authority for *releases*, not for a build that was
never published, so `update` refuses to replace it with the older one instead of
reporting a bogus "newer version available".

Install the published release directly to go back to a released version:

```sh
cargo install cargo-cleanme --locked --force
```

## Integrity and security expectations

**`.sha256` files are integrity evidence, not authenticity.** They prove that
the bytes you downloaded are the bytes the release built. They are not a
signature and make no claim about *who* produced them. An attacker who can
replace both the binary and its sidecar passes the check.

There is currently no signature, no notarization, and no transparency log.
Everything cargo-cleanme publishes is unsigned. Treat the download channel as
part of your trust boundary.

A successfully committed update is also not a guarantee of behavior: the
candidate is verified to be exactly `cargo-cleanme X.Y.Z` and to match the
release digest, and that is the full extent of the check.

## Shell completions and the manpage

Both are generated from the command model and checked into the repository. If
you add a flag, regenerate them or the drift gate fails:

```sh
cargo run --quiet --features dev-tools --bin generate-docs
```

They document the `cargo cleanme` spelling, because that is what works after a
registry install. A directly downloaded binary is also invokable as
`cargo-cleanme`.

## Getting help

If something here is wrong or a case is missing, that is a bug worth
reporting with the exact command, the exact output, and your platform.
