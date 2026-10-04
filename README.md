# cargo-cleanme

A focused Rust utility for finding reclaimable Cargo build artifacts without interrupting active development.

Scanning is read-only. Cleanup is a separate, explicitly scoped operation that defaults to Cargo's dry-run preview and requires `--yes` to execute.

The installed binary is `cargo-cleanme`, so it can be invoked directly or as a Cargo external subcommand. Both forms are equivalent:

```text
cargo-cleanme ...
cargo cleanme ...
```

## Initial goals

- fast recursive Cargo-project discovery;
- default 300-second activity guard;
- TOML configuration for recency and search scope;
- glob-style ignored directories with explicit re-included directories;
- optional sandbox/root scope that supersedes ignore/re-include filtering;
- no symlink traversal;
- allocated disk-usage reporting;
- deterministic, size-descending read-only output;
- separately invoked, Cargo-mediated `cargo clean` execution after fresh ownership and safety proof.

## Usage

```sh
cargo-cleanme                  # fast Routine scan over seed + learned roots
cargo-cleanme scan --full     # exhaustive reconciliation of platform roots
cargo-cleanme scan ./projects  # scan one explicit scope
cargo-cleanme scan ./projects --no-progress # scan without transient UI
cargo-cleanme scan ./projects --no-progress --stats # benchmark/debug: no UI, counters+timings on stderr
cargo-cleanme scan ./projects --format json # stable versioned JSON on stdout
cargo-cleanme clean ./projects # Cargo preview (default, `--dry-run`)
cargo-cleanme clean ./projects --format json --dryrun # machine-readable simulation
cargo-cleanme clean ./projects --dry-run # explicit Cargo preview
cargo-cleanme clean ./projects --dryrun # simulation: full path, no `cargo clean`
cargo-cleanme clean ./projects --yes # execute cleanup for verified workspaces
cargo-cleanme config path
cargo-cleanme config show
cargo-cleanme config edit
cargo-cleanme clean --known --dryrun # simulate bounded cleanup of Routine roots
cargo-cleanme clean --full --dryrun  # reconcile Full inventory, then simulate bounded scopes
cargo-cleanme clean ./projects --min-reclaimable-bytes 104857600
cargo-cleanme clean --known --older-than 2592000 --exclude '**/archived/**'
```

Global flags: `--no-progress` disables transient UI; `--stats` prints detailed semantic counters and phase/process timings to stderr (never stdout) for `scan` and `clean` (preview/simulate/execute). Default runs emit no debug counter line; stdout reports are byte-equivalent with and without `--stats`.

`--format human|json` selects the output format (human is the default). JSON emits one newline-terminated schema version 1 document to stdout; progress is disabled in JSON mode and `--stats` remains stderr-only. See [`plans/output-schema-v1.md`](plans/output-schema-v1.md) for field and compatibility details. Exit status is 0 when the requested operation completed, including safe per-unit skips; status 1 means cleanup scope was incomplete or one or more operations failed; status 2 means invocation/configuration failed before a report could be formed.

For unattended cleanup, an external scheduler can invoke an explicit bounded command such as `cargo-cleanme clean --known --older-than 2592000 --yes --format json`. `--yes` remains required for mutation. cargo-cleanme does not schedule or run in the background; callers should enforce cadence and resource limits.

`--dry-run`, `--dryrun`, and `--yes` conflict pairwise and have intentionally distinct spellings:

- `--dry-run`: Cargo preview (may invoke `cargo clean --dry-run --verbose`).
- `--dryrun`: cargo-cleanme simulation/debug (runs discovery, resolution, filtering, sizing, authorization, revalidation, progress, and reporting but never invokes any `cargo clean` command, including `cargo clean --dry-run`).
- `--yes`: real Cargo cleanup.

If the distinction proves error-prone, the CLI contract will be reconciled rather than silently aliased.

Pass `--config PATH` to read a specific TOML config. Explicit CLI roots take precedence over `scan.root`; either explicit scope bypasses user ignore/unignore search filters. Routine scans visit existing seed roots under Projects/projects, Developer, dev, Code/code, src, repos/Repos, GitHub/github, and workspace/workspaces, plus active learned roots. Routine scans remain recursive under those selected roots. A never-learned unusual location may remain undiscovered until `scan --full` or an Explicit scan visits it. Full discovery preserves the existing exhaustive platform roots and prunes. Full reconciliation learns workspace neighborhoods in machine-local JSON state and retains roots for 30 days by default (`scan.learned_root_retention_days`).

The repository ships a canonical `config.toml` template. Operational commands create it on first use without overwriting an existing file. `config path` only prints the resolved path. `config edit` opens that file in VISUAL, EDITOR, or an available terminal editor, then validates the result. The repository template is a distribution artifact and is never loaded implicitly from the working directory. The template includes a commented `[cleanup]` section:

```toml
[cleanup]
allowed_output_roots = [
  # "/absolute/cache/root",
]

[cleanup.policy]
min_reclaimable_bytes = 0
# min_inactive_seconds = 2592000
include = []
exclude = []
```

Cleanup policy matches canonical workspace roots and runs only after complete ownership resolution. Excluded workspaces remain in the ownership proof and can still make outputs shared. `--min-reclaimable-bytes` filters by the freshly measured complete workspace output union; `--older-than` is an additional quiet-age requirement and never replaces the recency safety guard. CLI policy values replace configured values of the same kind when provided. Include/exclude patterns use glob syntax and exclusion wins over inclusion.

Profile/package selectors do not have trustworthy selector-specific byte estimates. Their JSON estimate is `null`; complete output-union measurements are context only. Any nonzero `--min-reclaimable-bytes` threshold with a selector fails closed before Cargo mutation.

`clean --profile NAME` delegates profile selection to Cargo on exact qualified runtime releases: 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0. `clean --package SPEC` is enabled only on Cargo 1.98.1 and 1.99.0, the exact releases where package cleanup passed the configured-target matrix. Other/unknown Cargo versions get a typed `selector_unsupported` result before any Cargo clean. Selector-specific reclaimable bytes are unknown, so a nonzero `--min-reclaimable-bytes` safely skips that unit; JSON reports the selector estimate as `null` and labels full output-union measurements as context. Package specs are checked against Cargo-resolved workspace package identities; duplicate names are rejected before a clean process. Cargo may remove shared dependency artifacts. See [the M008D qualification plan](plans/implementation/artifact-discovery-cleanup/008d-cargo-package-selector-qualification-and-enablement.md).

Discovery state is machine-local and normally self-managed. Routine scans use seed/configured roots if the state is missing or unusable. A successful `scan --full` replaces corrupt or obsolete local state after reconciliation completes; an incomplete Full scan leaves it untouched. State written by a newer cargo-cleanme schema is preserved, and that version must be upgraded before it can be reconciled. Manually deleting discovery state is not the normal recovery procedure.

The scanner is manifest-first: it discovers user `Cargo.toml` files without requiring a sibling `target/`, then asks Cargo itself (`locate-project --workspace`, `metadata --offline --locked --no-deps`) to resolve the workspace root, members, and `target-dir`/`build-dir` configuration. Cargo is never invoked for ignored/pruned paths. After the first successful metadata resolution for a workspace, every member manifest returned by that authoritative metadata is registered, so later discovered manifests from the same workspace require neither another locate nor another metadata process (an N-member workspace needs exactly one locate and one metadata; membership is never guessed from path ancestry or TOML). Equal/nested physical output is grouped and measured once, so `target == build` or nested `target`/`build` is never double-counted. Each group is labeled `[private]`, `[shared]`, `[external-unproven]`, or `[uncertain]`; shared, external-unproven, and uncertain output is inventory-only. Source edits anywhere in a workspace and recent output anywhere in a group protect that group. Cheap gates (existence, artifact presence, source activity) run before deep sizing. Unix and Windows report allocated bytes; other platforms report apparent bytes. Discovery, source-activity, target-sizing, and post-clean measurement share one bounded `dua-core` worker pool (at most eight threads); Cargo resolution is sequential. Final output is deterministic size-descending with a stable path tie-break and prints every reportable group plus a deduplicated inventory-estimate total, never recovered bytes. Detailed scan/cleanup counters and phase/process timings (discovery, locate/metadata time and count, source-activity, output-sizing, render overhead, and for cleanup the final ownership-universe proof work) are opt-in via `--stats` on stderr and never appear on stdout; default scans emit no debug counter line. `--no-progress --stats` is the canonical benchmark/debug combination.

Attended terminals show immediate inline progress on stderr: one coordinated `MultiProgress` owns the main bar plus up to five candidate rows (a single cursor/draw owner, so bars never fight for terminal state), with an indeterminate discovery bar (no wasteful pre-scan) and determinate analysis/cleanup once group/candidate counts are known through the `ProgressObserver::units_total` / `unit_completed` contract. Totals are re-scoped on phase changes so analysis counts never leak into cleanup counts; both scan analysis and cleanup set determinate totals via the trait (callers never assume the concrete renderer). Up to five largest reportable groups with sizes are shown, capped at 10 Hz, with no alternate screen or raw mode, stderr only. Progress clears before the final stdout report and never changes results. `--no-progress` forces hidden output for benchmarks; `TERM=dumb` or non-TTY output is plain with no control sequences.

`clean ROOT` accepts a relative or absolute explicit existing directory and previews via Cargo by default. A relative root is resolved lexically against the invocation directory into an absolute sandbox root; the boundary still requires a real non-symlink directory and rejects uncertainty. Before output analysis or cleanup qualification, every discovered `Cargo.toml` under `clean ROOT` must be accounted for by direct successful resolution or successful workspace metadata listing it as a member/root. An earlier failed member attempt is cleared only if later authoritative metadata covers it. Any remaining unresolved manifest blocks the entire cleanup scope in Preview, Simulate, and Execute, reports `unresolved_ownership` under `--stats`, and runs no `cargo clean` command, including Cargo preview dry-run. Read-only scans remain partial-result tolerant. The destructive unit is one workspace cleanup unit: exactly one Cargo workspace cleanup invocation, containing one resolved workspace, its complete resolved `OutputSet` (target and build), and every physical output group that one `cargo clean` can affect. Multi-member workspaces are supported. A unit is authorized only when *every* group it can affect is `PrivateBounded`, authorized, inactive, a real non-symlink directory, and marker-qualified, so a private `target` can never carry an `ExternalUnproven`, `Shared`, `Uncertain`, or unauthorized sibling `build` directory into the same invocation; such a unit is skipped whole, with the failing output kind, path, and class in the skip reason. Equal/nested `target`/`build` collapse into one physical group and one deduplicated union, so a distinct private target/build pair produces one Cargo invocation, one result, and one pre/post union measurement. `cleanup` progress totals and final report rows count cleanup units, not physical groups; each row names the workspace and the output roots the single invocation affects. The read-only scan report stays physical-group oriented. Only `PrivateBounded` may reach Cargo cleanup; `ExternalUnproven`, `Shared`, and `Uncertain` remain inventory-only under any authorization configuration. `cleanup.allowed_output_roots` is location authorization only for already-private groups: it MUST NOT convert `ExternalUnproven` into a private group and does not establish exclusivity (a single observed external owner is not exclusivity proof). The field is preserved for schema compatibility but is currently operationally redundant because `PrivateBounded` output is normally workspace-contained; conventional and redirected-inside-workspace private output inside `clean ROOT` remains eligible. A single final cleanup proof (`ExecutionProof`) binds canonical workspace root and root manifest, canonical member set, canonical target, canonical build and capability, the complete deduplicated physical covering union the invocation can affect, pre-clean union bytes, ownership class, authorization decision, cache-marker validity, fresh source and output activity, and frozen environment/arguments immediately before spawn. The proof re-resolves *every* workspace discovered under `clean ROOT` (the bounded cleanup ownership universe) through fresh `cargo metadata --offline --locked --no-deps` calls against each known root manifest, rebuilds the complete fresh physical output graph from those fresh workspaces, rejects any change in workspace identity, member set, target/build identity, group shape, or classification, rejects the unit when any other discovered workspace (including symlink or unresolvable output roots) can reach the affected output, and inspects every affected covering root for existence, directory type, no-symlink state, activity, and cache markers. A universe workspace that cannot be re-resolved fails the candidate closed. `cleanup --stats` reports this final-proof Cargo work separately from the initial scan counters; direct metadata refresh avoids redundant `cargo locate-project` calls, and independent refreshes run with a fixed concurrency cap of four (`proof_metadata_peak`). Preview, simulation, and execution share all non-mutating pre-spawn gates (ownership, authorization, fresh resolution, member/source activity, target/build identity, output activity, group stability, markers, frozen-env, final preflight); on unchanged state they reach the same cleanable/skipped dispositions, and `--dryrun` invokes no `cargo clean` command (including Cargo dry-run). For cleanup, `CARGO_TARGET_DIR` is frozen to the resolved target and, when runtime Cargo supports stable `build-dir`, `CARGO_BUILD_BUILD_DIR` is frozen to the resolved build; inherited conflicting variables never silently redirect the destination, and the command runs from the workspace/root-manifest context using the ordinary whole-workspace request unless a qualified profile/package selector was requested, which is passed through to Cargo; cargo-cleanme never parses Cargo-private cache layout. Output directories are never recursively deleted by cargo-cleanme; cleanup delegates to Cargo sequentially. Transient progress (determinate via the observer contract, at most five current/largest groups with sizes, distinct preview/simulate/execute labels) clears before final results. Execute lists every cleaned workspace unit with its output roots, pre-clean union size, post-clean union size, and observed decrease, plus a deduplicated total observed decrease and skipped/failed reasons; post-clean measurement uncertainty is reported as a diagnostic and never fabricates recovered bytes. Preview lists authorized candidates with pre-clean estimates and Cargo preview text plus a total estimate and `no cleanup executed`. Simulate lists every would-clean unit with estimates plus a total estimated would-clean bytes and `no cargo clean command was invoked`. Preview/simulate totals are estimates, never recovered bytes. No-argument invocation and `scan` remain read-only. No machine-wide destructive command exists, and shared-cache cleanup is not claimed.

## Installing

There is no published release yet. The two supported installation paths today
are:

```sh
cargo install cargo-cleanme --locked          # from the crates.io registry
cargo install cargo-cleanme --locked --version 0.1.1  # pin an exact release
```

Installing the package is enough for Cargo to expose the external
subcommand, so `cargo cleanme ...` works after `cargo install`.

### One-command install from a release

`packaging/install.sh` (Linux, macOS) and `packaging/install.ps1` (Windows)
install a verified prebuilt binary from a GitHub release:

```sh
curl -fsSL https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.sh | sh
sh install.sh --version 0.1.0     # pin an exact release
sh install.sh --dir "$HOME/bin"   # choose the destination
```

```powershell
irm https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.ps1 | iex
./install.ps1 -Version 0.1.0
```

The install path is binary-first and fail-closed:

1. resolve latest stable, or the exact `X.Y.Z` you asked for;
2. map the host to the published target below;
3. download the asset and its `.sha256` sidecar;
4. refuse to continue unless the digest matches;
5. run the candidate and require exactly `cargo-cleanme X.Y.Z`;
6. place it, then re-verify the placed bytes;
7. report the path, and print PATH guidance if the directory is not on `PATH`.

Default destinations are `$HOME/.local/bin` when not root and
`/usr/local/bin` when already root. On Windows it is a per-user
`%LOCALAPPDATA%\cargo-cleanme\bin`, or `%ProgramFiles%\cargo-cleanme\bin` when
already elevated. **The wrappers never call `sudo`, `su`, `doas`, or
`Start-Process -Verb RunAs`.** For a machine-wide install, run the wrapper
yourself in an elevated shell. The wrappers never modify your `PATH`; they only
print the command you would need.

Cargo is used only when the host has **no** published binary, or when the
selected release genuinely does not contain that binary. A checksum mismatch,
a missing checksum, a wrong product or version, a TLS or transport failure, and
a destination conflict are all hard failures — they never fall back to Cargo.
When the fallback does run, it builds into a private temporary Cargo root and
validates the produced binary with the same `--version` check before placing
it, and it cleans up only its own temporary state.

### Self-update

```sh
cargo cleanme update --dry-run   # resolve and report; change nothing
cargo cleanme update             # update to the latest stable release
```

`update` is deliberately narrow. It will not guess and it will not clean up
after itself:

- The **registry is the version authority**. The published stable version is
  read from crates.io, and the GitHub release tag is *constructed* as
  `vX.Y.Z` from it — never scraped from a page, a redirect, or a "latest" link.
  A prerelease is not an update target, because `update` cannot express "I
  accept that tradeoff" the way an explicit `cargo install` can.
- **A Cargo-managed install is refused.** If the running executable lives in a
  Cargo bin root that Cargo's own `.crates.toml` records, `update` prints
  `cargo install cargo-cleanme --locked --force` and changes nothing. Cargo
  owns that file and its bookkeeping; replacing it here would make
  `cargo install --list` and uninstall lie.
- **Ownership is proven, not inferred.** Anything else is updated only after
  the live file's exact SHA-256 is observed and then re-verified under a
  mutation lock immediately before replacement. The file name, the directory,
  and `PATH` order prove nothing. A file that cannot be positively identified
  fails closed.
- **A host with no published binary** is reported as such, with the Cargo
  command to use instead.
- Bytes are replaced only after the asset's `.sha256` sidecar verifies, the
  sidecar is confirmed to be evidence for *that* asset, and the candidate is
  executed and required to print exactly `cargo-cleanme X.Y.Z`.
- The transport is the published `eggup-eggfetch` adapter. Exactly one
  production transport is wired, and the run never silently switches to
  another. Note that 0.1.0 used the external `curl` executable instead and
  could not reach the registry at all: crates.io answers HTTP 403 to a
  non-descriptive User-Agent, and that adapter had no way to set one. The
  embedded HTTP/TLS stack is the measured cost of a transport that works — the
  release binary is roughly twice the size it was in 0.1.0.
- Verified replacement, rollback, and staging are Eggup's, not a second copy
  here. A commit that is not cleanly `Committed` is reported as such, including
  when recovery is required.
- The SHA-256 evidence is **integrity only**. There is no signature check, and
  `update` makes no authenticity claim.


## Prebuilt release targets

The release contract in [`release/eggpack/distribution.toml`](release/eggpack/distribution.toml)
is the single authority for the published target matrix and asset names. It is
compiled from that file; the mapping is not maintained by hand anywhere else.

| Target triple | Release asset | Install name |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `cargo-cleanme-x86_64-unknown-linux-gnu` | `cargo-cleanme` |
| `aarch64-unknown-linux-gnu` | `cargo-cleanme-aarch64-unknown-linux-gnu` | `cargo-cleanme` |
| `x86_64-apple-darwin` | `cargo-cleanme-x86_64-apple-darwin` | `cargo-cleanme` |
| `aarch64-apple-darwin` | `cargo-cleanme-aarch64-apple-darwin` | `cargo-cleanme` |
| `x86_64-pc-windows-msvc` | `cargo-cleanme-x86_64-pc-windows-msvc.exe` | `cargo-cleanme.exe` |

Each asset ships a `.sha256` sidecar.

Two further hosts are recognized but have **no** prebuilt binary and are
Cargo-install-only:

- `armv7-unknown-linux-gnueabihf` (32-bit ARMv7 Linux)
- any other unlisted OS or architecture

Musl targets, Windows ARM64, ARMv7 prebuilts, and package-manager formulae are
follow-up capabilities, not current support. The Linux binaries are built with
a pinned cross toolchain, but no glibc floor is advertised as support until the
exact release bytes have been runtime-qualified on every target.

## Support and release policy

- **MSRV:** 1.89.
- **Release artifacts:** built by Eggpack from an exact source revision,
  qualified per target, and staged as a GitHub *draft*. Publication is a
  separate, explicit maintainer action; no automation publishes.
- **Integrity, not authenticity:** published `.sha256` files prove that
  downloaded bytes match the bytes the release built. They are not a signature
  and make no claim about who produced them.
- **Library API:** the crate publishes a Rust library because the test suite
  and tooling need one. It is **not** a stable third-party API before 1.0 and
  may change in any release. Treat the command-line interface, the versioned
  JSON schema, and the release-asset names as the stable surfaces.
- **Cargo selector support is exact-version:** `clean --profile` and
  `clean --package` work only on the exact Cargo releases listed above.
  Application support for a Cargo version is broader than selector
  qualification, and an unqualified Cargo release fails closed with a typed
  result rather than guessing.
- **Pre-1.0 versioning:** breaking changes to the CLI, JSON schema, or asset
  names can occur in any `0.x` release. Read `CHANGELOG.md` before upgrading.
  There is no published release schedule; the sequence is
  `0.1.0` -> `0.2.0` -> `1.0.0`, and `1.0.0` is when the CLI, JSON schema, and
  asset names are declared stable.

### Signing and notarization state

| Platform | State |
|---|---|
| macOS | **unsigned, not notarized.** Gatekeeper will refuse a downloaded binary on first launch; see [troubleshooting](docs/TROUBLESHOOTING.md). |
| Windows | **unsigned.** SmartScreen will warn on a downloaded binary. |
| Linux | unsigned; no packages, formulae, or distribution packages are published. |

No signature, transparency log, or reproducible-build claim is made for any
target. A `.sha256` sidecar proves the bytes match what the release built; it
does not prove who built them.

### Linux ABI floor

**Not yet claimed.** The two Linux targets are built with a pinned
cross-toolchain that produces a glibc 2.17 floor, but no runtime evidence for
the exact release bytes exists yet, so no floor is advertised. Once a release
has been qualified on both Linux targets, this section will name the floor
explicitly and `scripts/check-release-contract.py` will start requiring that
the README states it.

## Uninstalling

There is nothing to uninstall beyond the binary and its config file. cargo-cleanme
never installs a service, a daemon, or a background process.

| How it was installed | How to remove it |
|---|---|
| `cargo install cargo-cleanme` | `cargo uninstall cargo-cleanme` |
| `install.sh` / `install.ps1` | delete the binary it reported installing |
| manually downloaded | delete the binary |

Then remove the config, if you want a clean slate. Its path is printed by
`cargo cleanme config path`:

```sh
rm -f "$(cargo cleanme config path)"
```

`cargo cleanme update` never runs against a Cargo-managed installation, so
there is no state to reconcile between the package manager and the binary.

## Shell completions and manpage

Both are generated from the command model, not written by hand, and a CI job
fails if they drift from the model:

```sh
cargo run --quiet --features dev-tools --bin generate-docs
```

| Shell | File |
|---|---|
| Bash | `completions/cargo-cleanme.bash` |
| Zsh | `completions/_cargo-cleanme` |
| Fish | `completions/cargo-cleanme.fish` |
| PowerShell | `completions/_cargo-cleanme.ps1` |
| Elvish | `completions/cargo-cleanme.elv` |

Man pages: `man/cargo-cleanme.1` plus one page per subcommand. They document
the `cargo cleanme` spelling, which is what works after a registry install.

## Documentation

- [docs/RELEASING.md](docs/RELEASING.md) — the operator release checklist.
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) — installer and updater
  failure modes, and the limits of the integrity evidence.
- [CHANGELOG.md](CHANGELOG.md) — what changed, and what deliberately does not
  exist yet.

## Planning

The repository starts planning-first. Canonical product direction, roadmap, bounded implementation plans, and active status are maintained under `plans/` using the same long-term/interim/closure separation used by CodeGG.

Cleanup is opt-in and Cargo-managed. Its current destructive boundary is defined by M004 plus ADR 001 and the C002-C004 ownership/revalidation corrections; M006 performance work must not weaken those checks.
