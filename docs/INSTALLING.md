# Installing cargo-cleanme

## Install with Cargo

Install from crates.io on any host supported by Cargo:

```sh
cargo install cargo-cleanme --locked
```

To pin a release, add `--version X.Y.Z`. Cargo makes `cargo cleanme` available. You can also run the installed binary directly as `cargo-cleanme`.

## Install a prebuilt binary

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.ps1 | iex
```

These commands install the latest stable release. To use installer options, save the script and run it locally. Both installers accept an exact release version and destination directory, and refuse to replace an existing binary unless you pass `--force` (PowerShell: `-Force`). The POSIX installer also supports `--no-shell-profile` and `--no-path`.

The installer verifies the downloaded SHA-256 digest and checks the candidate version before placement. A missing or mismatched checksum, or a network failure, stops the install. Cargo is used as a source-build fallback only when the installer recognizes a host or release with no prebuilt binary.

### Destinations and PATH

| Host | Default destination |
|---|---|
| Linux or macOS, non-root | `$HOME/.local/bin` |
| Linux or macOS, root | `/usr/local/bin` |
| Windows, non-elevated | `%LOCALAPPDATA%\\cargo-cleanme\\bin` |
| Windows, elevated | `%ProgramFiles%\\cargo-cleanme\\bin` |

The installers do not elevate privileges. Run from an elevated shell for a system-wide install.

On Linux and macOS, the installer can add a guarded PATH entry to a zsh or bash startup file for a default non-root install. It also prints guidance for the current shell, which a child process cannot update. A custom `--dir` is not written to a shell profile. `--no-shell-profile` suppresses the profile edit but keeps PATH guidance; `--no-path` suppresses both. PowerShell does not edit PATH and prints instructions if its destination is not on PATH.

## Prebuilt targets

Prebuilt binaries are published for these targets:

| Target triple | Binary |
|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme.exe` |

ARMv7 Linux, musl targets, Windows ARM64, Windows ARMv7, and other unlisted targets do not have prebuilt releases. Install those with Cargo. No minimum glibc version is advertised for the Linux binaries.

## Integrity limits

The SHA-256 sidecar checks that the downloaded file matches the digest published with it. It does not prove who produced the file. The installers do not verify a publisher signature or provenance attestation.

## Updating and uninstalling

`cargo cleanme update` can update supported self-managed binary installations. Update a Cargo installation with:

```sh
cargo install cargo-cleanme --locked --force
```

For a Cargo installation, uninstall with `cargo uninstall cargo-cleanme`. For a prebuilt or manual install, remove the binary from its destination. To remove configuration too, run `cargo cleanme config path` first and delete the reported file. Keep it if you want to preserve your settings.

See [Self-update](UPDATE.md) for update limits.
