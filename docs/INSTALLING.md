# Installing cargo-cleanme

## Install with Cargo

This works on any host supported by Cargo:

```sh
cargo install cargo-cleanme --locked
```

To install a specific release, add `--version X.Y.Z`. Installing the crate makes
`cargo cleanme` available. You can also run the binary directly as
`cargo-cleanme`.

## Install a prebuilt binary

On the prebuilt path, the release installers verify the downloaded binary
before placing it.

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.ps1 | iex
```

To use installer options, save the script locally and run it with the desired
arguments. The installers accept an exact release version and a destination
directory. They refuse to replace an existing binary unless you explicitly use
`--force` (PowerShell: `-Force`). For example:

```sh
sh install.sh --version 0.2.15 --dir "$HOME/bin"
```

```powershell
./install.ps1 -Version 0.2.15 -Directory "$env:LOCALAPPDATA\bin"
```

Use the latest release unless you need a pin. Release `0.2.0` is yanked due to
serious cleanup defects. See [Troubleshooting](TROUBLESHOOTING.md#known-issues-in-older-releases).

## Installer behavior

On the prebuilt path, the installer downloads the release binary and its
`.sha256` sidecar, checks the digest, verifies the candidate's version, places
it, and checks the placed file. A missing or mismatched checksum is a hard
failure. It does not fall back to Cargo when release verification fails.

If the host has no prebuilt binary, the installers may use Cargo as a
source-build fallback. You can also install from crates.io with
`cargo install cargo-cleanme --locked`.

### Destinations

| Host | Default destination |
|---|---|
| Linux or macOS, non-root | `$HOME/.local/bin` |
| Linux or macOS, root | `/usr/local/bin` |
| Windows, non-elevated | `%LOCALAPPDATA%\cargo-cleanme\bin` |
| Windows, elevated | `%ProgramFiles%\cargo-cleanme\bin` |

The installers do not elevate privileges for you. Run them from an elevated
shell if you want a system-wide installation.

### PATH

On Linux and macOS, the installer may add a guarded entry to a zsh or bash
startup file for a default user install. It also prints the command needed to
update PATH in the current shell, since a child installer cannot change its
parent shell. A new terminal reads the saved entry.

- `--no-shell-profile` skips the startup-file edit but still prints PATH
  guidance.
- `--no-path` skips both the startup-file edit and PATH guidance.
- `--dir PATH` uses your chosen directory and does not persist it to a shell
  profile.

The PowerShell installer does not modify PATH. Add its destination to PATH if
needed.

## Prebuilt targets

Prebuilt release binaries are published for these targets:

| Target triple | Binary |
|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme.exe` |

ARMv7 Linux, musl targets, Windows ARM64, Windows ARMv7, and other unlisted
platforms do not have prebuilt binaries. Use Cargo to install on those hosts.
No minimum glibc version is currently advertised for the Linux binaries.

## Integrity and signing

The `.sha256` sidecar checks that the downloaded bytes match the digest
published with the release. It does not prove who produced the bytes. GitHub
release attestations are available, but neither the installer nor
`cargo cleanme update` verifies them. The binaries are not signed with macOS
Developer ID or Windows Authenticode certificates, and the macOS binaries are
not notarized.

## Updating

`cargo cleanme update` can update a supported self-managed binary installation.
It refuses to replace a Cargo-managed installation; update those with:

```sh
cargo install cargo-cleanme --locked --force
```

See [Self-update](UPDATE.md) for details.

## Uninstall

For a Cargo installation:

```sh
cargo uninstall cargo-cleanme
```

For an installer or manual installation, delete the binary from the destination
reported by the installer. To remove configuration too, run `cargo cleanme config path` before removing
the binary, then delete the file it reports. Keep the config file if you may
reinstall and want to preserve your settings.
