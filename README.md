# cargo-cleanme

Find reclaimable Cargo build artifacts and clean them **through Cargo** —
never by deleting directories.

```console
$ cargo cleanme --dry-run
combined roots /home/you/Projects

Cleaned  …/demo-app  [private]  output …/demo-app/target  before   8.78 MiB  — simulation: no `cargo clean` command was invoked

simulation: 1 would-clean, 0 skipped, 0 failed; estimated would-clean   8.78 MiB; no `cargo clean` command was invoked; 0 filesystem diagnostics
```

`cargo cleanme` with no arguments **cleans**. Every deletion is delegated to
Cargo and gated on a fresh complete ownership proof; `--dry-run` runs the whole
decision path and removes nothing. Scanning is the separate, always read-only
operation.

## Install

> **Do not install 0.2.0.** It is yanked, and two destructive defects were found
> in it after release: `--dry-run clean ROOT` — the spelling Cargo itself passes —
> fell through to a real cleanup, and a workspace whose declared output directory
> contained another project's source tree was cleaned as `private`, deleting that
> neighbour's files. Both report success and exit 0. See
> [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md#known-defects-by-version).

```sh
cargo install cargo-cleanme --locked
```

Or install a verified prebuilt binary from a GitHub release:

```sh
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh
```

Installing the package is enough for Cargo to expose the external subcommand,
so `cargo cleanme …` and `cargo-cleanme …` are equivalent.

→ [docs/INSTALLING.md](docs/INSTALLING.md) for pinned versions, Windows,
destination and PATH handling, and what the installer refuses to do.

### Prebuilt release targets

[`release/eggpack/distribution.toml`](release/eggpack/distribution.toml) is the
single authority for this matrix and the asset names; it is not maintained by
hand.

| Target triple | Install name |
|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme.exe` |

Each asset ships a `.sha256` sidecar. `armv7-unknown-linux-gnueabihf` is a
recognized host with **no** prebuilt binary: it is Cargo-install-only, as are
musl, Windows ARM64/ARMv7, and any unlisted OS or architecture.

The Linux targets are built against a glibc 2.17 floor, but that floor is
**not yet claimed** as support — no runtime evidence for the exact release bytes
exists yet, so nothing here advertises a minimum glibc. See
[the ABI floor section](docs/INSTALLING.md#linux-abi-floor).

## Quickstart

```sh
cargo cleanme                               # Routine maintenance: clean (Execute)
cargo cleanme --dry-run                     # Routine maintenance: simulate, zero `cargo clean`
cargo cleanme scan                          # Full read-only reconciliation of the machine
cargo cleanme scan --known                  # Routine read-only inventory
cargo cleanme scan ROOT                     # one explicit bounded scope
cargo cleanme --format json                 # one versioned JSON document on stdout
```

### The surface

| Command | What it does | Runs `cargo clean`? |
|---|---|---|
| `cargo cleanme` | Routine cleanup over the maintenance scope | yes |
| `cargo cleanme --dry-run` | Simulation: every gate runs, nothing is removed | **never**, not even preview |
| `cargo cleanme scan` | Full read-only reconciliation | never |
| `cargo cleanme scan --known` | Routine read-only inventory | never |
| `cargo cleanme scan ROOT` | One bounded read-only scope | never |
| `cargo cleanme clean ROOT` | Cleanup of one explicit root (Execute by default) | yes |
| `cargo cleanme clean --known` / `--full` | Advanced orchestration | yes |
| `cargo cleanme clean … --cargo-preview` | Cargo's own `clean --dry-run --verbose`, after the same proof | yes, as a dry run |

**The maintenance scope** is a configured legacy `scan.root` when one is set
(exclusive, explicit), otherwise bounded seed roots plus active learned roots.
Rootless `scan` is always Full and ignores `scan.root`, so configuration can
never silently narrow the reconciliation command.

### Scan output

Scan finds inactive output. It never deletes.

```console
$ cargo cleanme scan ~/projects
  8.78 MiB  /home/you/projects/demo-app/target  [private]

  8.78 MiB inventory estimate across 1 inactive Cargo output groups
```

A group is reported as **`[private]`** only when cargo-cleanme can prove it
owns those bytes exclusively. `[shared]`, `[external-unproven]`, and
`[uncertain]` groups are **inventory only** and are never cleaned.

> **If `scan` reports 0 groups on a project you just built, that is the safety
> guard working.** Anything whose source changed within the last 300 seconds is
> *active* and protected. Backdate the sources, or wait, and it will appear.

### Clean modes

Three modes, and `dry-run` means one specific thing:

| Flag | Mode | Runs `cargo clean`? |
|---|---|---|
| *(none)* | Execute | yes |
| `--dry-run` | Simulation | **never**, not even preview |
| `--cargo-preview` | Cargo preview | yes, as `cargo clean --dry-run --verbose` |

```console
$ cargo cleanme clean ~/projects/demo-app --cargo-preview --no-progress
combined scope: 1 root(s) […], 1 manifest(s), 1 resolved workspace(s), 1 CleanupUnit(s), 0 unresolved ownership participant(s)
Previewed  …/demo-app  [private]  output …/demo-app/target  before   8.78 MiB  — … /warning: no files deleted due to --dry-run/

cargo preview: 1 previewed, 0 skipped, 0 failed; pre-clean estimate   8.78 MiB; no cleanup executed; 0 filesystem diagnostics

$ cargo cleanme clean ~/projects/demo-app --dry-run --no-progress
Simulated  …/demo-app  [private]  output …/demo-app/target  before   8.78 MiB  — simulation: no `cargo clean` command was invoked

simulation: 1 would-clean, 0 skipped, 0 failed; estimated would-clean   8.78 MiB; no `cargo clean` command was invoked; 0 filesystem diagnostics
```

`--dry-run` still runs discovery, resolution, ownership proof, authorization,
sizing, revalidation, and reporting — it differs *only* in that it stops before
spawning anything. That makes it the mode to use when testing a change to the
gates. `--cargo-preview` is the lower-level debugging view that hands the
mutation decision to Cargo itself.

Real cleanup, made explicit:

```console
$ cargo cleanme clean ~/projects/demo-app --no-progress
Cleaned  …/demo-app  [private]  output …/demo-app/target  before   8.78 MiB  after   0.00 B  observed decrease   8.78 MiB  — Removed 22 files, 8.7MiB total

cleanup: 1 cleaned, 0 skipped, 0 failed; pre-clean estimate   8.78 MiB, post-clean measured   0.00 B, observed decrease   8.78 MiB; 0 filesystem diagnostics
```

Every candidate must prove exclusive ownership of its bytes, and the proof is
repeated immediately before each `cargo clean`. If any manifest under the
scope cannot be resolved, **no** cleanup command runs at all and you get a
typed block instead:

```console
$ cargo cleanme --format json; echo "exit $?"
{
  "scope_blocked": true,
  "scope_reason": "cleanup ownership could not be proven: 1 discovered Cargo manifest did not resolve; no cleanup commands were run",
  "unresolved_participants": [
    { "manifest": "…/broken/Cargo.toml", "stage": "locate", "reason": "cargo locate-project failed" }
  ],
  "units_considered": 0
}
exit 1
```

→ [docs/USAGE.md](docs/USAGE.md#cleanup-safety-model) for the full safety
model, and [docs/USAGE.md](docs/USAGE.md#exit-codes) for exit codes.

### Unattended

```sh
cargo cleanme --format log                # one bounded line: status, scope, mode, counts
cargo cleanme scan --format log           # same, read-only Full reconciliation
```

```console
$ cargo cleanme --format log
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0
```

`log` exists for schedulers that keep a short history pane: one ASCII line, at
most 384 bytes, no paths, no progress output. **Anything parsing output should
parse `--format json`**, which remains the complete versioned contract.

→ [docs/AUTOMATION.md](docs/AUTOMATION.md) for the field list, a worked greggd
configuration, scheduler-neutral alternatives, and exit-code triage.

### Config

```sh
cargo cleanme config path   # where the config lives
cargo cleanme config show   # effective settings
cargo cleanme config edit   # $VISUAL, then $EDITOR, then a common editor
```

Created on first use, never overwritten, and validated after every edit.

```toml
[scan]
recency_seconds = 300          # protect anything touched in the last 5 minutes
ignore = []                    # literal path segments
unignore = []                  # literal relative paths, NOT globs
learned_root_retention_days = 30

[cleanup]
allowed_output_roots = []      # authorizes a location; never grants ownership

[cleanup.policy]
min_reclaimable_bytes = 0
# min_inactive_seconds = 2592000
include = []
exclude = []
```

`allowed_output_roots` can widen where cleanup is *permitted*. It can never
turn a shared or unproven group into a cleanable one.

## Common flags

| Flag | Effect |
|---|---|
| `--format human\|json\|log` | interactive detail; one schema-1 JSON document; or one bounded ASCII line for an unattended scheduler |
| `--dry-run` | simulate: the whole decision path, zero `cargo clean` processes |
| `--no-progress` | disable the transient TTY UI (stderr only) |
| `--stats` | counters and phase timings **on stderr**; stdout stays byte-identical |
| `--config PATH` | use a specific `config.toml` |
| `--min-reclaimable-bytes N` | only clean units above this measured size (`clean`) |
| `--older-than SECONDS` | extra quiet-age requirement; never replaces the recency guard (`clean`) |
| `--known` / `--full` | Routine roots / Full reconciliation, on `clean` and `scan` |

`--dry-run`, `--cargo-preview`, and `--yes` conflict pairwise on purpose.

### Upgrading from 0.1.x

Bare `cargo cleanme` changed from a read-only Routine scan to Routine cleanup,
and `--dry-run` changed from Cargo preview to simulation.

| 0.1.x | Now | Meaning |
|---|---|---|
| `cargo cleanme` | `cargo cleanme scan --known` | same old read-only Routine inventory |
| `cargo cleanme scan --full` | `cargo cleanme scan` | Full reconciliation |
| `cargo cleanme clean R --dry-run` | `cargo cleanme clean R --cargo-preview` | same Cargo preview |
| `cargo cleanme clean R --dryrun` | `cargo cleanme clean R --dry-run` | same simulation |
| `cargo cleanme clean R --yes` | `cargo cleanme clean R` | Execute is now the default |

`--dryrun` and `--yes` still work on `clean` as hidden aliases for the
pre-1.0 migration period. They print nothing.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | the operation completed, including safe per-unit skips |
| `1` | scope incomplete, ownership unproven, or an operation failed |
| `2` | invocation or configuration error, before a report could be formed |

`1` on `clean` with `scope_blocked: true` is a *success-shaped* result with a
typed blocker, not a crash. Read the block.

## Keep it current

```sh
cargo cleanme update --dry-run   # report what would change
cargo cleanme update
```

`update` refuses to touch a Cargo-managed install and will not downgrade or
accept a prerelease. → [docs/UPDATE.md](docs/UPDATE.md).

## Documentation

| Document | What it covers |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | full command reference, discovery model, cleanup safety model, config, JSON schema, selectors |
| [docs/AUTOMATION.md](docs/AUTOMATION.md) | bounded log output, scheduler integration, unattended limits and exit-code triage |
| [docs/INSTALLING.md](docs/INSTALLING.md) | install paths, prebuilt targets, destinations, uninstalling |
| [docs/UPDATE.md](docs/UPDATE.md) | self-update rules, what it refuses, integrity vs authenticity |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | known defects by version, installer and updater failure modes |
| [docs/RELEASING.md](docs/RELEASING.md) | maintainer release checklist |
| [CHANGELOG.md](CHANGELOG.md) | released changes, including known defects per version |

For contributors and coding agents: [AGENTS.md](AGENTS.md) and
[architecture/overview.md](architecture/overview.md).

## Support and policy

- **MSRV** 1.89. **Prebuilt** for the five targets in
  [Prebuilt release targets](#prebuilt-release-targets). Musl, Windows ARM64, and
  ARMv7 are Cargo-only.
- **Integrity, not authenticity.** Published `.sha256` files prove the bytes
  match what the release built. There is no signature and no authenticity
  claim. macOS binaries are unsigned and not notarized — Gatekeeper will
  refuse a downloaded binary on first launch.
- **No glibc floor is advertised yet.** See the
  [ABI floor note](#prebuilt-release-targets).
- **Pre-1.0.** Breaking changes to the CLI, JSON schema, or asset names can
  occur in any `0.x` release. Read [CHANGELOG.md](CHANGELOG.md) before
  upgrading. `1.0.0` is when those are declared stable.
- **Library API is not stable.** The crate ships a library because the tests
  and tooling need one. The CLI, the versioned JSON schema, and the
  release-asset names are the stable surfaces.
- cargo-cleanme never installs a service or daemon, and never modifies your
  `PATH`.

## Uninstall

```sh
cargo uninstall cargo-cleanme     # if installed with cargo
rm -f "$(cargo cleanme config path)"
```

→ [docs/INSTALLING.md](docs/INSTALLING.md#uninstalling).

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
