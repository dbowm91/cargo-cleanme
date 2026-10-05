# cargo-cleanme troubleshooting

## Check this first: known defects by version

Two defects in the `0.1.x` line affected the updater. Both were found by the
project's own release rehearsals, not by a test, and both are fixed. They are
listed first because they are the only cases where a correct-looking run did
the wrong thing.

### If you are on 0.1.1 through 0.1.4: check whether `update` replaced a Cargo-managed binary

**This is the one defect in the `0.1.x` line that could change a file it did
not own.** A binary installed with `cargo install --root DIR` — the form every
hermetic install script, CI job, and container image uses — was not recognised
as Cargo-managed, because detection only ever looked at `$CARGO_HOME/bin`. The
file was treated as a self-managed installation and **overwritten in place**,
with no warning and exit status `0`. Afterwards `cargo install --list` reported
the old version for a file that no longer had it, so `cargo upgrade` and
`cargo uninstall` could no longer do their jobs.

Nothing else about those releases is affected: scanning, sizing, reporting, and
cleanup are correct. To check whether you were hit:

```sh
cargo cleanme --version                      # the version the file reports now
cargo install --list | grep cargo-cleanme     # the version Cargo believes it owns
```

If those disagree, you are in the affected state. Repair it with:

```sh
cargo install cargo-cleanme --locked --force
```

Fixed in **0.1.5**. Versions 0.1.1 through 0.1.4 carry it; 0.1.5 and later
refuse a Cargo-managed installation, and also parse the `.crates.toml` schema
that current Cargo actually writes. None of these versions were yanked, because
their other behaviour is correct and yanking would misdescribe them.

### If you are on 0.1.1 or 0.1.2: `update` could not complete

The identity check ran the downloaded candidate with **no arguments**, so its
stdout could never equal a version string — and before failing, it scanned the
whole machine. The transaction then aborted safely every time, so nothing was
corrupted and no file was replaced. The advertised feature simply never once
worked in any published version.

Fixed in **0.1.3**. If you are on 0.1.1 or 0.1.2, upgrade with `cargo install
cargo-cleanme --locked --force` rather than `cargo cleanme update`.

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

They also never edit your `PATH`. If the install directory is not on `PATH`,
they print the exact command to add it.

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
