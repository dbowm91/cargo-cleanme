# cargo-cleanme

Find inactive Cargo build artifacts and clean them **through Cargo**, never by
removing directories directly.

The default command performs cleanup. Use `--dry-run` to review the same
selection and safety checks without invoking cleanup. Use `scan` to inventory
output without cleaning it.

## Install

From crates.io:

```sh
cargo install cargo-cleanme --locked
```

Prebuilt installers are available for Linux, macOS, and Windows. See
[Installing](docs/INSTALLING.md) for platform support, install options, and
verification details.

Cargo exposes the binary as `cargo cleanme`; you can also run it directly as
`cargo-cleanme`.

## Quick start

Inspect one project, preview its cleanup, then clean it:

```sh
cargo cleanme scan ~/projects/my-app
cargo cleanme ~/projects/my-app --dry-run
cargo cleanme ~/projects/my-app
```

`scan` does not clean build output. A machine scan updates cargo-cleanme's
local learned-root state. `--dry-run` runs the cleanup checks without invoking
`cargo clean`. The final command uses Cargo to remove eligible build output.

> **Important for users upgrading from 0.1.x:** bare `cargo cleanme` now
> performs cleanup. Use `cargo cleanme --dry-run` to preview, or `cargo cleanme
> scan` to inventory without cleaning.

## Common commands

| Command | Purpose |
|---|---|
| `cargo cleanme` | Clean the maintenance scope, anchored to the working directory |
| `cargo cleanme --dry-run` | Simulate maintenance without invoking `cargo clean` |
| `cargo cleanme ROOT` | Clean one bounded root |
| `cargo cleanme --full` | Clean learned project roots that can be proven |
| `cargo cleanme scan` | Inventory the machine and reconcile learned roots; does not clean build output |
| `cargo cleanme scan ROOT` | Inventory one directory and update its learned roots; does not clean build output |
| `cargo cleanme --interactive` | Choose cleanup units in a terminal |
| `cargo cleanme config path\|show\|edit` | Locate, view, or edit configuration |
| `cargo cleanme update --dry-run` | Check for an update without replacing the binary |
| `cargo cleanme update` | Update a supported self-managed installation |

The `clean` subcommand remains as a hidden compatibility alias. The canonical
cleanup forms are `cargo cleanme`, `cargo cleanme ROOT`, and
`cargo cleanme --full`.

A bare cleanup uses the maintenance scope and selects workspaces related to the
current directory. A configured `scan.root` replaces that scope. `ROOT` names
one bounded scope. `--full` cleans learned roots and does not perform a new
machine-wide scan. Run `cargo cleanme scan` first to discover and learn roots.

A directory named `scan`, `config`, `update`, or `clean` is parsed as a
subcommand. Use `./scan` to name a directory called `scan`.

## Safety in brief

- Cleanup is always performed by `cargo clean`; cargo-cleanme does not delete
  directories itself.
- Recently active projects are protected. The default activity window is 300
  seconds.
- A shared output directory is eligible only when every known owner is idle.
- Source trees, symlinks, and paths whose ownership cannot be proven are never
  cleaned.
- Ownership is checked again immediately before cleanup. An unresolved
  manifest inside the selected scope can block that scope; unresolved manifests
  outside it are reported without blocking it.

`--dry-run` is the application simulation and invokes no `cargo clean` command.
`--cargo-preview` is different: after the same safety checks, it invokes
Cargo's own `clean --dry-run --verbose` for inspection.

## Output and exit status

Use `--format json` for complete, versioned machine-readable reports. Use
`--format log` for a bounded one-line scheduler summary. The log format is not a
machine contract; scripts should parse JSON. See
[Unattended operation](docs/AUTOMATION.md).

| Exit code | Meaning |
|---|---|
| `0` | Completed, including when there was nothing to clean or safe units were skipped |
| `1` | Partial or blocked scan/cleanup, unproven ownership, or an operation failure |
| `2` | Usage, configuration, or other fatal error |

## Configuration

Configuration is created automatically when needed. View or edit it with:

```sh
cargo cleanme config path
cargo cleanme config show
cargo cleanme config edit
```

The default activity window is 300 seconds. `scan.ignore` accepts absolute glob
patterns; `scan.unignore` accepts literal absolute paths, not globs. Cleanup
policies can set a minimum size, an additional quiet period, and workspace-root
include or exclude patterns. `cleanup.allowed_output_roots` authorizes a
location but never proves ownership. See the
[configuration reference](docs/USAGE.md#configuration).

## Prebuilt release targets

| Target triple | Install name |
|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme.exe` |

ARMv7 Linux (`armv7-unknown-linux-gnueabihf`), musl, Windows ARM64, and
Windows ARMv7 are Cargo-install-only. No minimum glibc version is currently
advertised. The prebuilt installers verify SHA-256 checksums, but the checksum
is not a publisher signature. Details are in [Installing](docs/INSTALLING.md).

## Documentation

- [Using cargo-cleanme](docs/USAGE.md): commands, scopes, safety, configuration,
  and output formats
- [Unattended operation](docs/AUTOMATION.md): scheduler and log-format guidance
- [Installing](docs/INSTALLING.md): install methods, platform support, and
  uninstalling
- [Troubleshooting](docs/TROUBLESHOOTING.md): common problems and fixes
- [Self-update](docs/UPDATE.md): update behavior and limitations
- [Changelog](CHANGELOG.md): changes and known issues by release

## Compatibility

The project is pre-1.0. Breaking changes to the CLI, JSON schema, or release
assets may occur in any 0.x release. Read the changelog before upgrading.

## Uninstall

For a Cargo installation:

```sh
cargo uninstall cargo-cleanme
```

For a prebuilt or manual installation, delete the binary from its destination.
To remove configuration too, run `cargo cleanme config path` before removing the
binary, then delete the file it reports. Keep that file if you want to preserve
your settings.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
