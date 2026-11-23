# cargo-cleanme

Find inactive Cargo build output and clean it through Cargo. cargo-cleanme never deletes directories itself.

**Cleanup runs by default.** If you are unsure what will be selected, preview the same command with `--dry-run` first. For example, `cargo cleanme ROOT --dry-run` checks a bounded root without invoking `cargo clean`.

## Install

Install from crates.io with Cargo:

```sh
cargo install cargo-cleanme --locked
```

Prebuilt installers and platform support are covered in [Installing](docs/INSTALLING.md). Cargo makes the command available as `cargo cleanme`; you can also run `cargo-cleanme` directly.

## Quick start

```sh
cargo cleanme scan ~/projects/my-app
cargo cleanme ~/projects/my-app --dry-run
cargo cleanme ~/projects/my-app
```

The first command inventories the named directory and does not clean build output. The second simulates cleanup. The third performs eligible cleanup through Cargo.

> **Upgrading from 0.1.x?** The bare `cargo cleanme` command now cleans by default. Use `cargo cleanme --dry-run` to preview or `cargo cleanme scan` to inventory without cleaning.

## Choose a command

| Command | Purpose |
|---|---|
| `cargo cleanme` | Run routine maintenance, with selection anchored to the working directory |
| `cargo cleanme ROOT` | Clean one explicitly bounded root |
| `cargo cleanme ROOT --dry-run` | Simulate cleanup for one bounded root |
| `cargo cleanme --full` | Recheck and clean learned roots; does not start a new machine-wide scan |
| `cargo cleanme scan` | Reconcile learned roots with a read-only machine scan |
| `cargo cleanme scan ROOT` | Inventory one directory and update learned roots for it |
| `cargo cleanme config path\|show\|edit` | Locate, view, or edit configuration |
| `cargo cleanme update --dry-run` | Check the latest release without replacing the binary |

Run `cargo cleanme scan` before `--full` if you have not discovered your projects yet. A configured `scan.root` replaces the routine maintenance scope. The `clean` subcommand remains accepted as a hidden compatibility alias.

A directory named `scan`, `config`, `update`, or `clean` is parsed as a command. Prefix a colliding directory with `./`, for example `cargo cleanme ./scan`.

## Safety and reports

- Cleanup always runs through Cargo. Ownership is rechecked immediately before cleanup.
- Only proven build output is eligible, and every known workspace using a shared output directory must be idle. The default recent-activity window is 300 seconds.
- Unresolved ownership inside the selected scope can block cleanup. Problems outside that scope are reported but do not block it.
- `--dry-run` is cargo-cleanme's simulation and invokes no `cargo clean`. `--cargo-preview` instead asks Cargo to run its own `clean --dry-run --verbose` after the safety checks.

Use `--format json` for structured reports or `--format log` for a short scheduler summary. See [Using cargo-cleanme](docs/USAGE.md) for the safety model, configuration, output formats, and exit codes, and [Unattended operation](docs/AUTOMATION.md) for scheduler guidance.

## Configuration

Configuration is created automatically when needed. Use `cargo cleanme config path`, `config show`, or `config edit` to manage it. The default activity window is 300 seconds. Ignore filters use absolute glob patterns; unignore entries are literal absolute paths, not globs. See the [configuration reference](docs/USAGE.md#configuration).

## Compatibility and support

The project is pre-1.0, so CLI, JSON, and release-asset changes may occur in any 0.x release. Check the [changelog](CHANGELOG.md) before upgrading. See [Installing](docs/INSTALLING.md) for prebuilt targets and checksum limitations.

## More help

- [Using cargo-cleanme](docs/USAGE.md)
- [Unattended operation](docs/AUTOMATION.md)
- [Installing](docs/INSTALLING.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Self-update](docs/UPDATE.md)
- [Changelog](CHANGELOG.md)

## Uninstall

For a Cargo installation, run:

```sh
cargo uninstall cargo-cleanme
```

For a prebuilt or manual installation, remove the binary from its destination. To remove configuration too, run `cargo cleanme config path` before removing the binary, then delete the reported file. Keep it to preserve your settings.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
