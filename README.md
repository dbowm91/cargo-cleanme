# cargo-cleanme

Find reclaimable Cargo build artifacts and clean them **through Cargo** — never
by deleting directories.

`cargo cleanme` with no arguments **cleans**. Every deletion is delegated to
Cargo and gated on a fresh, complete ownership proof; `--dry-run` runs the whole
decision path and removes nothing. Scanning is the separate, always read-only
operation.

## Install

```sh
cargo install cargo-cleanme --locked
```

Or a verified prebuilt binary:

```sh
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh
```

Installing the package is enough for Cargo to expose the external subcommand,
so `cargo cleanme …` and `cargo-cleanme …` are equivalent.

→ [docs/INSTALLING.md](docs/INSTALLING.md) for pinned versions, Windows,
destination and PATH handling, and what the installer refuses to do.

## Quickstart

Every transcript below is **real output**, captured from a build of this source
tree against a project whose `target/` directory was left untouched for two
hours. The paths are abbreviated to `~/…`; the byte counts, the row order, and
the wording are the tool's.

**1. Look before you leap.** `--dry-run` runs the whole decision path —
discovery, resolution, ownership proof, authorization, sizing, revalidation,
reporting — and spawns nothing at all:

```console
$ cargo cleanme clean ~/projects/my-app --dry-run --no-progress
combined roots ~/projects/my-app
combined scope: 1 root(s) [~/projects/my-app], 1 manifest(s), 1 resolved workspace(s), 1 CleanupUnit(s), 0 unresolved ownership participant(s)
Simulated  ~/projects/my-app  [private]  output ~/projects/my-app/target  before   4.02 MiB  — simulation: no `cargo clean` command was invoked

simulation: 1 would-clean, 0 skipped, 0 failed; estimated would-clean   4.02 MiB; no `cargo clean` command was invoked; 0 filesystem diagnostics
```

**2. Scan is read-only.** `scan` needs an explicit scope selector; bare `scan`
is a usage error, so the machine-wide reconciliation is never one keystroke from
the front door:

```console
$ cargo cleanme scan ~/projects --no-progress
  4.02 MiB  ~/projects/my-app/target  [private]

  4.02 MiB inventory estimate across 1 inactive Cargo output groups
```

`[private]` means cargo-cleanme can prove it owns those bytes exclusively.
`[shared]`, `[external-unproven]`, and `[uncertain]` are **inventory only** and
are never cleaned.

> **If `scan` reports 0 groups on a project you just built, that is the safety
> guard working.** Anything touched within the last 300 seconds is *active* and
> protected. Wait, or backdate the sources, and it will appear.

**3. Clean it.** Same selection, same proof, and Cargo does the deleting:

```console
$ cargo cleanme clean ~/projects/my-app --no-progress
combined roots ~/projects/my-app
combined scope: 1 root(s) [~/projects/my-app], 1 manifest(s), 1 resolved workspace(s), 1 CleanupUnit(s), 0 unresolved ownership participant(s)
Cleaned  ~/projects/my-app  [private]  output ~/projects/my-app/target  before   4.02 MiB  after   0.00 B  observed decrease   4.02 MiB  — Removed 4 files, 4.0MiB total

cleanup: 1 cleaned, 0 skipped, 0 failed; pre-clean estimate   4.02 MiB, post-clean measured   0.00 B, observed decrease   4.02 MiB; 0 filesystem diagnostics
```

**4. Or parse it.** One versioned JSON document on stdout, nothing else:

```console
$ cargo cleanme clean ~/projects/my-app --dry-run --format json
{"schema_version":1,"cargo_cleanme_version":"0.2.9","operation":"clean","scope":"explicit","mode":"simulate","result":{"discovered_manifests":1,"resolved_workspaces":1,"units_considered":1,"scope_blocked":false,"scope_reason":null,"unresolved_ownership":0,"unresolved_participants":[],"selected_roots":["~/projects/my-app"],"effective_policy":{"min_reclaimable_bytes":0,"min_inactive_seconds":null,"include":[],"exclude":[]},"selector_kind":null,"selector_value":null,"state_generation_last_full_at":null,"units":[{"workspace_root":"~/projects/my-app","output_roots":["~/projects/my-app/target"],"ownership":"private","policy_disposition":"selected","outcome":"simulated","reason_code":"simulated","before_bytes":4210688,"selector_estimate_bytes":4210688,"output_union_before_bytes":4210688,"after_bytes":null,"observed_decrease_bytes":null,"detail":"simulation: no `cargo clean` command was invoked"}],"summary":{"previewed":0,"simulated":1,"cleaned":0,"skipped":0,"failed":0,"diagnostics":0}}}
```

For a scheduler, `--format log` is one bounded ASCII line on stdout — no paths,
no progress, at most 384 bytes:

```console
$ cargo cleanme clean ~/projects/my-app --dry-run --format log
cargo-cleanme op=clean status=ok scope=explicit mode=simulate cleaned=0 skipped=0 failed=0 diagnostics=0
```

**Anything parsing output should parse `--format json`**, which is the complete
versioned contract. → [docs/AUTOMATION.md](docs/AUTOMATION.md) for the log
field list and scheduler integration.

## Commands

| Command | What it does | Runs `cargo clean`? |
|---|---|---|
| `cargo cleanme` | Cleanup anchored at the working directory (Execute) | yes |
| `cargo cleanme --dry-run` | Simulation: every gate runs, nothing is removed | **never**, not even preview |
| `cargo cleanme -i` / `--interactive` | Choose cleanup units from a terminal list, then clean | yes |
| `cargo cleanme scan --all` | Full read-only reconciliation of the machine | never |
| `cargo cleanme scan --known` | Routine read-only inventory | never |
| `cargo cleanme scan ROOT` | One bounded read-only scope | never |
| `cargo cleanme clean ROOT` | Cleanup of one explicit root (Execute by default) | yes |
| `cargo cleanme clean --known` / `--full` | Advanced orchestration | yes |
| `cargo cleanme clean … --cargo-preview` | Cargo's own `clean --dry-run --verbose`, after the same proof | yes, as a dry run |
| `cargo cleanme config path\|show\|edit` | Locate, print, or edit the config | never |
| `cargo cleanme update [--dry-run]` | Self-update to the latest stable release | never |

`scan --full` remains accepted as a hidden alias of `scan --all`.

The **maintenance scope** is a configured legacy `scan.root` when one is set
(exclusive), otherwise bounded seed roots under your home plus active learned
roots. Bare `cargo cleanme` and `clean --known` keep only the units anchored at
the working directory; `clean ROOT` and `clean --full` keep every resolved unit,
because their roots already bound the scope. Ownership is proven across the
whole resolved universe *before* the anchor narrows anything, so a target
shared with a project elsewhere stays shared and stays inventory-only.

→ [docs/USAGE.md](docs/USAGE.md) for the full command and flag reference, the
discovery model, and the cleanup safety model.

## Clean modes

| Flag | Mode | Runs `cargo clean`? |
|---|---|---|
| *(none)* | Execute | yes |
| `--dry-run` | Simulation | **never**, not even preview |
| `--cargo-preview` | Cargo preview | yes, as `cargo clean --dry-run --verbose` |

`--dry-run` differs from Execute *only* in that it stops before spawning
anything, which makes it the mode to use when testing a change to the gates.
`--cargo-preview` is the lower-level debugging view that hands the mutation
decision to Cargo itself.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | the operation completed, including safe per-unit skips |
| `1` | scope incomplete, ownership unproven, or an operation failed |
| `2` | invocation or configuration error, before a report could be formed |

`1` on `clean` with `scope_blocked: true` is a *success-shaped* result with a
typed blocker, not a crash. Read the block — it is inside `result`:

```console
$ cargo cleanme clean ~/projects/broken --format json; echo "exit $?"
{"schema_version":1,"cargo_cleanme_version":"0.2.9","operation":"clean","scope":"explicit","mode":"execute","result":{"discovered_manifests":1,"resolved_workspaces":0,"units_considered":0,"scope_blocked":true,"scope_reason":"cleanup ownership could not be proven: 1 discovered Cargo manifest did not resolve; no cleanup commands were run","unresolved_ownership":1,"unresolved_participants":[{"manifest":"~/projects/broken/Cargo.toml","stage":"locate","reason":"cargo locate-project failed: error: unclosed table, expected `]` | --> Cargo.toml:1:9 | |"}],"selected_roots":["~/projects/broken"],"effective_policy":{"min_reclaimable_bytes":0,"min_inactive_seconds":null,"include":[],"exclude":[]},"selector_kind":null,"selector_value":null,"state_generation_last_full_at":null,"units":[],"summary":{"previewed":0,"simulated":0,"cleaned":0,"skipped":0,"failed":0,"diagnostics":1}}}
exit 1
```

Two refusals exit `2` before any report is formed:

| Command | Refusal |
|---|---|
| `cargo cleanme config show --format json` | `configuration error: config command does not support --format json; use human or log output` — `config` has no JSON envelope, so it refuses loudly rather than emitting non-JSON. `--format log` is accepted and exits `0`. |
| `cargo cleanme clean --full` | `full reconciliation reported an incomplete scan (exit N); no cleanup commands were run against its unproven learned roots` — a Full cleanup runs against the learned roots the reconciliation just proved, so an incomplete or stale reconciliation refuses the cleanup instead of using unproven roots. |

→ [docs/USAGE.md](docs/USAGE.md#exit-codes) for the full exit-code table and
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) for symptom-first triage.

A run that finds **nothing to clean** is not a refusal. It prints one plain
line, `Nothing to clean.`, and exits `0` — the same for both spellings, and the
same whether the scope was empty or the interactive surface had nothing to
offer.

## Configuration

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
include = []
exclude = []
```

`allowed_output_roots` can widen where cleanup is *permitted*. It can never turn
a shared or unproven group into a cleanable one.

→ [docs/USAGE.md](docs/USAGE.md#configuration) for every key, its default, and
its validation rules.

## Prebuilt release targets

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

## Upgrading from 0.1.x

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

## Documentation

| Document | What it covers |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | full command and flag reference, discovery model, cleanup safety model, config, JSON schema, selectors |
| [docs/AUTOMATION.md](docs/AUTOMATION.md) | bounded log output, scheduler integration, unattended limits and exit-code triage |
| [docs/INSTALLING.md](docs/INSTALLING.md) | install paths, prebuilt targets, destinations, PATH handling, uninstalling |
| [docs/UPDATE.md](docs/UPDATE.md) | self-update rules, what it refuses, integrity vs authenticity |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | known defects by version, symptom-first triage, installer and updater failure modes |
| [docs/RELEASING.md](docs/RELEASING.md) | maintainer release checklist |
| [CHANGELOG.md](CHANGELOG.md) | released changes, including known defects per version |

For contributors and coding agents: [AGENTS.md](AGENTS.md) and
[architecture/overview.md](architecture/overview.md).

## Support and policy

- **MSRV** 1.89. **Prebuilt** for the five targets in
  [Prebuilt release targets](#prebuilt-release-targets). Musl, Windows ARM64, and
  ARMv7 are Cargo-only. **No glibc floor is advertised yet.**
- **Integrity, not full authenticity.** Published `.sha256` files prove the
  bytes match what the release built; they do not prove *who* built them.
  There is no maintainer-key signature and no transparency log. Each release
  additionally carries a GitHub **release attestation** that the release
  pipeline verifies — but the **shipped updater does not verify attestations**,
  so the trust root for an install is still the `.sha256` sidecar plus HTTPS.
  macOS binaries are unsigned and not notarized.
- **Pre-1.0.** Breaking changes to the CLI, JSON schema, or asset names can
  occur in any `0.x` release. Read [CHANGELOG.md](CHANGELOG.md) before
  upgrading. `1.0.0` is when those are declared stable.

→ [docs/INSTALLING.md](docs/INSTALLING.md#support-and-release-policy) for the
full policy, and [docs/INSTALLING.md](docs/INSTALLING.md#signing-and-notarization)
for what an install can and cannot prove.

## Uninstall

```sh
cargo uninstall cargo-cleanme     # if installed with cargo
rm -f "$(cargo cleanme config path)"
```

→ [docs/INSTALLING.md](docs/INSTALLING.md#uninstalling).

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
