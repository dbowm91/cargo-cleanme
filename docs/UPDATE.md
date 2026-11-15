# Self-update

```sh
cargo cleanme update --dry-run   # check the latest stable release
cargo cleanme update             # download and install it
```

The dry run contacts the registry to resolve the latest stable version. It does
not download the release binary or replace the running file.

## What update does

For a supported host and a self-managed installation, the updater:

1. Resolves the latest stable version from crates.io.
2. Selects the matching GitHub release asset for the host.
3. Downloads the asset and its `.sha256` sidecar, then verifies the digest.
4. Checks that the candidate identifies itself as exactly the target version.
5. Replaces the running binary only after confirming the existing file can be
   safely updated.

The updater does not downgrade or replace a build whose version is newer than
the latest published release.

## When update refuses

| Situation | What to do |
|---|---|
| The binary is Cargo-managed | Update it through Cargo: `cargo install cargo-cleanme --locked --force` |
| The file cannot be positively identified | Reinstall with Cargo or the [release installer](INSTALLING.md) |
| The host has no published binary | Install from crates.io with `cargo install cargo-cleanme --locked` |
| The registry or release host cannot be reached | Check network access and retry later |
| The release checksum is missing or invalid | Stop and retry later; the updater will not replace the binary |

Cargo-managed installations are refused so Cargo's install records remain
accurate. The updater also refuses an unprovable installation rather than
guessing from its filename or location.

An update refusal exits `2`. Already being current, or running a build newer
than the published release, is a no-change result and exits `0`.

## Integrity and authenticity

The `.sha256` sidecar verifies that the downloaded asset matches the published
digest. It does not prove who produced the asset. GitHub release attestations
are available, but the shipped updater does not verify them. Releases are not
signed with a maintainer key, and the binaries are not signed with macOS
Developer ID or Windows Authenticode certificates. macOS binaries are not
notarized.

## Machine-readable output

`update --format json` uses a separate schema-version-1 document rather than
the scan and cleanup JSON envelope. See [Output formats](USAGE.md#output-formats)
for the distinction.
