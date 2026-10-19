# Installing cargo-cleanme

Seventeen releases are published as GitHub releases and on crates.io: v0.1.0
through v0.1.6, then v0.2.0 through v0.2.9. **Sixteen of them are
installable.** **0.2.0 is yanked**: it could delete files it did not own, and
`cargo install` will no longer select it for a new installation. See
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#check-this-first-known-defects-by-version)
before installing anything at or below that version.

## Contents

- [From crates.io](#from-cratesio)
- [One-command install from a release](#one-command-install-from-a-release)
- [What the installer guarantees](#what-the-installer-guarantees)
- [Destinations](#destinations)
- [When Cargo is used instead](#when-cargo-is-used-instead)
- [Prebuilt release targets](#prebuilt-release-targets)
- [Support and release policy](#support-and-release-policy)
- [Signing and notarization](#signing-and-notarization)
- [Linux ABI floor](#linux-abi-floor)
- [Shell completions and manpage](#shell-completions-and-manpage)
- [Uninstalling](#uninstalling)

## From crates.io

```sh
cargo install cargo-cleanme --locked                    # newest release
cargo install cargo-cleanme --locked --version 0.1.6    # pin an exact release
```

The pinned examples on this page use `0.1.6` — the oldest non-yanked release —
to show the *syntax* of a pin. It is not the current release; drop `--version`
(or `--version <X.Y.Z>`) to take the newest one.

Installing the package is enough for Cargo to expose the external subcommand,
so `cargo cleanme …` works after `cargo install`. The binary is named
`cargo-cleanme`, and the two spellings are equivalent.

This is the only supported path on hosts with no prebuilt binary — musl, any
Windows ARM target, ARMv7, and any unlisted OS or architecture.

## One-command install from a release

`packaging/install.sh` (Linux, macOS) and `packaging/install.ps1` (Windows)
install a verified prebuilt binary from a GitHub release.

```sh
# Linux, macOS
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh

# pin a version
sh install.sh --version 0.1.6

# choose the destination
sh install.sh --dir "$HOME/bin"
```

```powershell
# Windows
irm https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.ps1 | iex

./install.ps1 -Version 0.1.6
./install.ps1 -Directory "$env:LOCALAPPDATA\bin"
```

As above, `0.1.6` is a pin example, not the current release: omit the version
argument to take the newest release.

`irm … | iex` is idiomatic PowerShell and is intentional.

The Windows wrapper's destination parameter is `-Directory`. `-Dir` is not a
parameter name; the POSIX wrapper's equivalent is `--dir`.

## What the installer guarantees

The path is binary-first and fail-closed:

1. resolve the latest stable release, or the exact `X.Y.Z` requested;
2. map the host to a published target;
3. download the asset and its `.sha256` sidecar;
4. **refuse to continue unless the digest matches**;
5. run the candidate and require it to print exactly `cargo-cleanme X.Y.Z`;
6. place it, then **re-verify the placed bytes**;
7. report the path, and make sure the directory is on `PATH` for future shells
   (see below).

The installer checks the `.sha256` sidecar and nothing else — it does **not**
verify the release attestation — so this is integrity evidence, not
authenticity. See [Signing and notarization](#signing-and-notarization).

## Destinations

| Host | Default destination |
|---|---|
| Linux / macOS, not root | `$HOME/.local/bin` |
| Linux / macOS, already root | `/usr/local/bin` |
| Windows, not elevated | `%LOCALAPPDATA%\cargo-cleanme\bin` |
| Windows, already elevated | `%ProgramFiles%\cargo-cleanme\bin` |

**The wrappers never call `sudo`, `su`, `doas`, or `Start-Process -Verb
RunAs`.** For a machine-wide install, run the wrapper yourself in an elevated
shell.

## PATH integration

`install.sh` makes the install directory usable from a new shell. It appends one
small guarded block to a single startup file, and only after the verified binary
is already in place:

```sh
# >>> cargo-cleanme installer PATH >>>
# added by the cargo-cleanme installer; safe to delete
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
# <<< cargo-cleanme installer PATH <<<
```

The block is idempotent: re-running the installer finds it and writes nothing.
Delete the markers and everything between them to remove it.

**The installer cannot change the `PATH` of the shell that invoked it.** A piped
child process cannot mutate its already-running parent, so the installer always
prints the export line for the current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

That takes effect in the shell you run it from. Open a new terminal for the
persisted entry, or paste the export yourself.

### When the profile is left alone

The startup file is **not** modified when:

| Situation | What happens |
|---|---|
| you passed `--dir PATH` | your explicit choice is reported, never auto-persisted |
| the install is a root/system install | there is no user profile to edit |
| the shell is not zsh or bash | manual guidance only; no syntax is guessed |
| the target is a symlink, a directory, a FIFO, or unwritable | refused, with the manual step printed |
| you passed `--no-shell-profile` | binary installed, manual guidance printed |
| you passed `--no-path` | binary installed, **no** PATH guidance and no profile change |

Which file is used:

- **zsh** — `$ZDOTDIR/.zshrc` when `ZDOTDIR` is an absolute path, otherwise
  `$HOME/.zshrc`.
- **bash on Linux** — `$HOME/.bashrc`.
- **bash on macOS** — the first of `$HOME/.bash_profile`, `$HOME/.bash_login`,
  `$HOME/.profile` that exists; `$HOME/.bash_profile` if none does. Terminal
  starts a login shell there, so that is the file it reads.

The installer never sources, evaluates, or command-substitutes the file it
writes, never creates a directory to put a dotfile in, and never follows a
symlink. If the write fails, it says so and leaves the installed binary alone.

## When Cargo is used instead

Cargo is used only when the host has **no** published binary, or when the
selected release genuinely does not contain one.

A checksum mismatch, a missing checksum, a wrong product or version, a TLS or
transport failure, and a destination conflict are all **hard failures** — none
of them falls back to Cargo.

When the fallback does run, it builds into a private temporary Cargo root,
validates the produced binary with the same `--version` check before placing it,
and cleans up only its own temporary state.

## Prebuilt release targets

[`release/eggpack/distribution.toml`](../release/eggpack/distribution.toml) is
the single authority for the published target matrix and asset names. The
release workflow is generated from that file, and
`scripts/check-installer-contract.py` proves the installer wrappers are a
projection of it. The tables in this file and in [`README.md`](../README.md) are
**hand-copied transcriptions** of it, and must be kept in sync with it — they
are not maintained by hand *instead* of it.

| Target triple | Release asset | Install name |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme-x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme-aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme-x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme-aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme-x86_64-pc-windows-msvc.exe` | `cargo-cleanme.exe` |

Each asset ships a `.sha256` sidecar.

The hosts with **no** prebuilt binary are Cargo-install-only:

- `armv7-unknown-linux-gnueabihf` (32-bit ARMv7 Linux) — a host the wrappers
  recognize and route to Cargo; the release contract omits it on purpose
- musl targets
- Windows ARM64
- Windows ARMv7

Any other unlisted OS or architecture is Cargo-install-only as well, but the
wrappers do not recognize it as a host at all.

Musl prebuilts, Windows ARM64/ARMv7 prebuilts, and package-manager formulae
are follow-up capabilities, not current support.

## Support and release policy

- **MSRV:** 1.89.
- **Release artifacts** are built by Eggpack from an exact source revision,
  qualified per target, and staged as a GitHub *draft*. Publication is a
  separate, explicit maintainer action; no automation publishes.
- **Library API** is **not** stable before 1.0. The crate publishes a library
  because the test suite and tooling need one, and it may change in any release.
  Treat the command-line interface, the versioned JSON schema, and the
  release-asset names as the stable surfaces.
- **Cargo selector support is exact-version.** `clean --profile` and
  `clean --package` work only on the exact releases listed in
  [USAGE.md](USAGE.md#cleanup-selectors). An unqualified Cargo release fails
  closed with a typed result rather than guessing.
- **Pre-1.0 versioning:** breaking changes to the CLI, JSON schema, or asset
  names can occur in any `0.x` release. Read [CHANGELOG.md](../CHANGELOG.md)
  before upgrading. There is no published release schedule; the sequence is
  `0.1.0` → `0.2.0` → `1.0.0`, and `1.0.0` is when the CLI, JSON schema, and
  asset names are declared stable.
- cargo-cleanme never installs a service, daemon, or background process.

## Signing and notarization

| Platform | State |
|---|---|
| macOS | **unsigned, not notarized.** Gatekeeper refuses a downloaded binary on first launch — see [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
| Windows | **unsigned.** SmartScreen warns on a downloaded binary |
| Linux | unsigned; no packages or formulae are published |

"Unsigned" above means *not signed with a platform code-signing key*: there is
no macOS Developer ID, no Windows Authenticode certificate, and no
maintainer-key signature or transparency log for any target.

What a published release does carry is a **GitHub release attestation**.
Immutable releases are enabled for the repository, so a published release's
assets and tag cannot be replaced, and GitHub issues a signed attestation
binding the release to its repository, tag, commit, and every asset digest. The
release pipeline checks this: `scripts/release-check.sh` runs the attestation
helper's self-test and its `--settings-only` mode (which proves the
repository's immutability policy is still on), and
`scripts/verify-release-attestation.py --tag <X.Y.Z>` verifies a specific
release's attestation and every asset digest against it.

Two limits worth stating plainly:

- **The shipped updater does not verify attestations.** `cargo cleanme update`
  fetches the asset and its `.sha256` sidecar and checks the digest; it does not
  consult the attestation. The sidecar check is the updater's fail-closed
  integrity requirement, not a substitute for the attestation.
- **The trust root is GitHub's release and attestation infrastructure** — not an
  independent maintainer key, and not SLSA build provenance.

A `.sha256` sidecar proves the bytes match what the release built; it does
**not** prove who built them. Releases v0.1.0 through v0.1.6 predate the
immutable-release policy, remain mutable, and carry no attestation.

## Linux ABI floor

**Not yet claimed.** The two Linux targets are built with a pinned
cross-toolchain that produces a glibc 2.17 floor, but no runtime evidence for
the exact release bytes exists yet, so no floor is advertised as support. Once a
release has been qualified on both Linux targets, this section will name the
floor explicitly and `scripts/check-release-contract.py` will start requiring
that the README state it.

## Shell completions and manpage

Both are generated from the command model rather than written by hand, and CI
fails if they drift from the model.

| Shell | File |
|---|---|
| Bash | `completions/cargo-cleanme.bash` |
| Zsh | `completions/_cargo-cleanme` |
| Fish | `completions/cargo-cleanme.fish` |
| PowerShell | `completions/_cargo-cleanme.ps1` |
| Elvish | `completions/cargo-cleanme.elv` |

There are **eight** man pages: the root page `man/cargo-cleanme.1`, one page per
top-level subcommand (`clean`, `config`, `scan`, `update`), and one page per
`config` sub-subcommand (`edit`, `path`, `show`). They document the
`cargo cleanme` spelling, which is what resolves after a registry install.

To regenerate after a CLI change:

```sh
cargo run --quiet --features dev-tools --bin generate-docs
```

## Uninstalling

There is nothing to uninstall beyond the binary and its config file.

| How it was installed | How to remove it |
|---|---|
| `cargo install cargo-cleanme` | `cargo uninstall cargo-cleanme` |
| `install.sh` / `install.ps1` | delete the binary it reported installing |
| manually downloaded | delete the binary |

Then remove the config for a clean slate. Its path is printed by
`cargo cleanme config path`:

```sh
rm -f "$(cargo cleanme config path)"
```

`cargo cleanme update` never runs against a Cargo-managed installation, so there
is no state to reconcile between the package manager and the binary.
