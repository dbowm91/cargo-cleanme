# cargo-cleanme

Find reclaimable Cargo build artifacts and clean them **through Cargo** —
never by deleting directories.

```console
$ cargo cleanme --dry-run
combined roots /home/you/Projects

Simulated  …/demo-app  [private]  output …/demo-app/target  before   8.78 MiB  — simulation: no `cargo clean` command was invoked

simulation: 1 would-clean, 0 skipped, 0 failed; estimated would-clean   8.78 MiB; no `cargo clean` command was invoked; 0 filesystem diagnostics
```

The transcript above is **illustrative**: the paths and byte counts are
placeholders, and the real human output prints two scope lines — `combined
roots …` followed by `combined scope: N root(s) …` — before the per-unit rows.

`cargo cleanme` with no arguments **cleans**. Every deletion is delegated to
Cargo and gated on a fresh complete ownership proof; `--dry-run` runs the whole
decision path and removes nothing. Scanning is the separate, always read-only
operation.

## Install

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
single authority for this matrix and the asset names. The tables here and in
[docs/INSTALLING.md](docs/INSTALLING.md) are hand-copied transcriptions of it
and must be kept in sync with it.

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
cargo cleanme                               # clean anchored at the working directory (Execute)
cargo cleanme --dry-run                     # same selection, simulated, zero `cargo clean`
cargo cleanme -i                            # choose units from a terminal list, then clean
cargo cleanme scan --all                    # Full read-only reconciliation of the machine
cargo cleanme scan --known                  # Routine read-only inventory
cargo cleanme scan ROOT                     # one explicit bounded scope
cargo cleanme --format json                 # one versioned JSON document on stdout
```

### The surface

| Command | What it does | Runs `cargo clean`? |
|---|---|---|
| `cargo cleanme` | Cleanup anchored at the working directory (Execute) | yes |
| `cargo cleanme --dry-run` | Simulation: every gate runs, nothing is removed | **never**, not even preview |
| `cargo cleanme -i` / `--interactive` | Choose cleanup units from a terminal list, then clean | yes |
| `cargo cleanme scan --all` | Full read-only reconciliation | never |
| `cargo cleanme scan --known` | Routine read-only inventory | never |
| `cargo cleanme scan ROOT` | One bounded read-only scope | never |
| `cargo cleanme clean ROOT` | Cleanup of one explicit root (Execute by default) | yes |
| `cargo cleanme clean --known` / `--full` | Advanced orchestration | yes |
| `cargo cleanme clean … --cargo-preview` | Cargo's own `clean --dry-run --verbose`, after the same proof | yes, as a dry run |

`scan --full` remains accepted as a hidden alias of `scan --all`.

**The maintenance scope** is a configured legacy `scan.root` when one is set
(exclusive, explicit), otherwise bounded seed roots plus active learned roots.
Bare `cargo cleanme` and `clean --known` resolve that scope and then keep only
the units anchored at the working directory: a unit qualifies when its workspace
root is the directory you are standing in, contains it, or is contained by it.
`clean ROOT` and `clean --full` keep every resolved unit, because their roots
already bound the scope. Units outside the anchor are reported as typed
`skipped_outside_selection` rows, never silently dropped. Ownership is still
proven across the whole resolved universe before the anchor narrows anything, so
a target shared with a project elsewhere stays shared and stays inventory-only.
`scan --all` is the Full reconciliation and ignores `scan.root`, so
configuration can never silently narrow it; bare `scan` is a usage error.
Learned locations that
provably vanished are omitted for the run with a diagnostic; symlinked or
unreadable locations block instead, and explicit roots stay strict.

`-i` / `--interactive` presents one row per resolved cleanup unit with its
reclaimable bytes and cleans only what is checked. It is a selection surface,
not a second deletion path: the choice only narrows the candidates, and the
engine re-proves ownership per spawn exactly as in a non-interactive run. It
needs a terminal on stdin and stdout; combine with `--dry-run` to review a
selection without invoking `cargo clean`.

Full scans are best-effort for inventory: protected or unreadable subtrees can
leave coverage partial. Such scans retain discovered projects but do not
publish a complete learned-state generation. Routine learning uses only
Cargo-resolved workspaces in durable developer locations; package caches,
Trash, temporary trees, `node_modules`, and transient Codex worktrees stay out
of automatic maintenance while remaining discoverable in Full scans.
If Cargo cannot resolve one discovered manifest, scans preserve it as an
unresolved participant. Cleanup fails closed **only for the proof boundary** —
the scope you selected: the working-directory anchor, the explicit roots, or
the Full set. A manifest Cargo cannot resolve *inside* that boundary blocks
it. A manifest *outside* it is reported as a bounded `out_of_scope` diagnostic
and never blocks: cleaning a healthy anchor while poison exists elsewhere is
the intended behaviour, so a boundary clean is never read as a system clean.
See
[Troubleshooting](docs/TROUBLESHOOTING.md#a-manifest-is-found-but-its-cargo-workspace-cannot-be-resolved)
for reproducing the exact Cargo lookup.

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

The transcripts in this section are **illustrative**: the paths and byte counts
are placeholders, and the real human output prints `combined roots …` before
the `combined scope: N root(s) …` line.

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
repeated immediately before each `cargo clean`. Cleanup fails closed **only
for the proof boundary** — the scope you selected: the working-directory
anchor, the explicit roots, or the Full set. A manifest Cargo cannot resolve
*inside* that boundary blocks it, so **no** cleanup command runs at all and
you get a typed block instead. A manifest *outside* it is reported as a
bounded `out_of_scope` diagnostic and never blocks: cleaning a healthy anchor
while poison exists elsewhere is the intended behaviour, so a boundary clean
is never read as a system clean. The blocked shape looks like this:

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
cargo cleanme scan --all --format log     # same, read-only Full reconciliation
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
ignore = []                    # absolute glob patterns
unignore = []                  # absolute literal paths, NOT globs
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
| `--include GLOB` | include canonical workspace roots matching this glob (`clean`) |
| `--exclude GLOB` | exclude canonical workspace roots matching this glob (`clean`) |
| `--profile NAME` | ask Cargo to clean one named profile (`clean`) |
| `--package SPEC` | clean one validated workspace package (`clean`) |
| `--cargo-preview` | after the same proof, run Cargo's own `clean --dry-run --verbose` (`clean`) |
| `--yes` | hidden alias for the default Execute mode (`clean`) |
| `--known` | Routine roots (`clean`, `scan`) |
| `--full` | Full reconciliation before cleanup (`clean`) |
| `-a` / `--all` | Full machine reconciliation (`scan`; `--full` is a hidden alias) |
| `-i` / `--interactive` | choose cleanup units from a terminal list (needs a TTY) |

`--dry-run`, `--cargo-preview`, and `--yes` conflict pairwise **on `clean`**, on
purpose: they select three different modes. A root-level `--dry-run` — the
`cargo cleanme --dry-run clean ROOT` spelling Cargo itself passes to an
external subcommand — is a different flag from the clean-level one and does
**not** conflict with `--cargo-preview`: the root flag wins and selects
Simulation, because simulation is the mode that spawns nothing.

### Upgrading from 0.1.x

Bare `cargo cleanme` changed from a read-only Routine scan to Routine cleanup,
and `--dry-run` changed from Cargo preview to simulation.

| 0.1.x | Now | Meaning |
|---|---|---|
| `cargo cleanme` | `cargo cleanme scan --known` | same old read-only Routine inventory |
| `cargo cleanme scan --full` | `cargo cleanme scan --all` | Full reconciliation |
| `cargo cleanme clean R --dry-run` | `cargo cleanme clean R --cargo-preview` | same Cargo preview |
| `cargo cleanme clean R --dryrun` | `cargo cleanme clean R --dry-run` | same simulation |
| `cargo cleanme clean R --yes` | `cargo cleanme clean R` | Execute is now the default |

`--dryrun` and `--yes` still work on `clean` as hidden aliases for the
pre-1.0 migration period. They print nothing.

### Changes in 0.2.8

- Bare `cargo cleanme` and `clean --known` clean only the units anchored at the
  working directory. `clean ROOT` and `clean --full` are unfiltered.
- `scan` requires an explicit selector: `scan --all` (Full), `scan --known`
  (Routine), or `scan ROOT` (explicit). Bare `scan` is a usage error.
- `-i` / `--interactive` adds a terminal selection list over the resolved units.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | the operation completed, including safe per-unit skips |
| `1` | scope incomplete, ownership unproven, or an operation failed |
| `2` | invocation or configuration error, before a report could be formed |

`1` on `clean` with `scope_blocked: true` is a *success-shaped* result with a
typed blocker, not a crash. Read the block.

Two refusals exit `2` before any report is formed:

| Command | Refusal |
|---|---|
| `cargo cleanme config show --format json` | `configuration error: config command does not support --format json; use human or log output` — `config` has no JSON envelope, so it refuses loudly rather than emitting non-JSON. `--format log` is accepted and exits `0`. |
| `cargo cleanme clean --full` | `full reconciliation reported an incomplete scan (exit N); no cleanup commands were run against its unproven learned roots`, or `full reconciliation produced no fresh generation; no cleanup commands were run against stale learned roots` — a Full cleanup runs against the learned roots the reconciliation just proved, so an incomplete or stale reconciliation refuses the cleanup instead of using unproven roots. |

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
- **Integrity, not full authenticity.** Published `.sha256` files prove the
  bytes match what the release built; they do not prove *who* built them.
  There is no maintainer-key signature and no transparency log. Each release
  additionally carries a GitHub **release attestation** that the release
  pipeline verifies — see
  [docs/RELEASING.md](docs/RELEASING.md#release-authenticity-immutable-releases-and-attestations)
  — but the **shipped updater does not verify attestations**, so the trust
  root for an install is still the `.sha256` sidecar plus HTTPS. macOS binaries
  are unsigned and not notarized — Gatekeeper will refuse a downloaded binary
  on first launch.
- **No glibc floor is advertised yet.** See the
  [ABI floor note](#prebuilt-release-targets).
- **Pre-1.0.** Breaking changes to the CLI, JSON schema, or asset names can
  occur in any `0.x` release. Read [CHANGELOG.md](CHANGELOG.md) before
  upgrading. `1.0.0` is when those are declared stable.
- **Library API is not stable.** The crate ships a library because the tests
  and tooling need one. The CLI, the versioned JSON schema, and the
  release-asset names are the stable surfaces.
- cargo-cleanme never installs a service or daemon. The POSIX installer appends
  one guarded, idempotent block to your zsh or bash startup file so a new shell
  can find the binary — see [docs/INSTALLING.md](docs/INSTALLING.md#path-integration).
  It never touches a system-wide startup file, and `--no-shell-profile` or
  `--no-path` suppresses it.

## Uninstall

```sh
cargo uninstall cargo-cleanme     # if installed with cargo
rm -f "$(cargo cleanme config path)"
```

→ [docs/INSTALLING.md](docs/INSTALLING.md#uninstalling).

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
