# Self-update

`cargo cleanme update` updates supported self-managed binary installations. A Cargo-managed installation must be updated through Cargo.

`--dry-run` contacts crates.io to resolve the latest stable version and reports the candidate. It does not download the release binary or replace the running file.

```sh
cargo cleanme update --dry-run
cargo cleanme update
```

## What an update checks

For a supported host and a verifiable self-managed installation, the updater:

1. Resolves the latest stable version from crates.io.
2. Selects the matching GitHub release asset for the host.
3. Downloads the asset and its `.sha256` sidecar, then checks the digest.
4. Confirms the candidate reports exactly the target version.
5. Replaces the existing binary only after its identity and update eligibility are verified.

The updater does not downgrade or replace a running build newer than the latest published release.

## When an update is refused

| Situation | What to do |
|---|---|
| Cargo-managed installation | Update with `cargo install cargo-cleanme --locked --force`. |
| Installation cannot be positively identified | Reinstall with Cargo or the [release installer](INSTALLING.md). |
| No release binary for the host | Install from crates.io with `cargo install cargo-cleanme --locked`. |
| Registry or release host cannot be reached | Check network access and retry later. |
| Checksum is missing or invalid | Stop and retry later. The updater will not replace the binary. |
| Already current or running a newer build | No change is made. |

A mutating update refuses Cargo-managed or unprovable installations rather than guessing from the binary's name or location. An update refusal exits `2`. Already current and newer-than-published builds are no-change results and exit `0`.

## Integrity limits

The SHA-256 sidecar checks that the downloaded asset matches the digest published with it. It does not prove who produced the asset. The updater does not verify a publisher signature or provenance attestation.

## Machine-readable output

`update --format json` uses a separate schema-version-1 document, not the scan and cleanup JSON envelope. See [Output formats](USAGE.md#output-and-exit-codes) for details.
