# cargo-cleanme — Architecture Overview

A birds-eye view of how the crate is put together, and an index into the
per-component deep dives that follow.

This file has two jobs, deliberately kept separate:

1. **Orientation.** §1–§9 explain what the tool does, what each module owns,
   how the pieces fit, and which invariant each piece is responsible for. Read
   this to form a correct mental model.
2. **Index.** §12 links every discrete component to the deep dive that covers
   it in detail, and §13 gives a reading order.

Deep dives are where the line-level citations live. This file cites only the
anchors a reader needs to navigate. **Every citation here was re-derived from
the source**, not shifted arithmetically — see `AGENTS.md` §5 for why that
distinction matters.

- **What it is:** a Cargo subcommand (`cargo cleanme`) that finds inactive Cargo
  build artifacts on a machine and either reports or safely reclaims them.
- **Shape:** one binary (`src/main.rs`) over one library (`src/lib.rs`) of 16
  modules, 18 source files, 43,057 lines in `src/` (18,777 of them outside
  every `#[cfg(test)]` scope), no async runtime of its own, and exactly one
  binary dependency that matters for safety (`cargo metadata`).
- **Version:** 0.2.9 (unpublished; latest published tag `v0.2.8`) ·
  **Edition:** 2024 · **MSRV:** 1.89
- **Deep dives:** 16 documents, one per component, linked from §4 and §12.

---

## 1. The one-paragraph model

`cargo-cleanme` answers a deceptively hard question: *which build artifacts on
this machine are nobody using any more?* The naive answer — "anything whose
mtime is old" — is wrong, because a single `target/` directory is routinely
shared between several workspaces. Deleting it can destroy output another
project still depends on. So the architecture is built around one idea:

> **Nothing is ever deleted on the strength of a timestamp. Every candidate must
> first prove exclusive ownership of its physical bytes, and that proof must
> still hold at the moment of deletion.**

Everything else — the discovery state file, the two scan modes, the ownership
proof pass, the simulation mode, the fail-closed diagnostics — exists to
service that invariant.

---

## 2. What the tool can do

The tool exposes six operations: four that touch build output, plus
configuration and self-update. Every one of them resolves to exactly one
internal intent, so there is no path that behaves differently from its
documented twin.

| Operation | Spellings | What it does | Mutates bytes? |
|---|---|---|---|
| **Maintenance cleanup** | `cargo cleanme` (bare) | Routine scope: bounded seed roots plus active learned roots, **plus the working directory** when the Routine scope does not already reach it — or a configured `scan.root`, which is an exclusive override. Proves ownership per candidate and cleans eligible units. | Yes — via `cargo clean` |
| **Bounded cleanup** | `cargo cleanme ROOT` | One bounded root. Bypasses discovery filters. `cargo cleanme .` is a bounded cleanup of `.`. | Yes — via `cargo clean` |
| **Full cleanup** | `cargo cleanme --full` | Runs a complete Full reconciliation first, then cleans the learned roots it could **prove**. | Yes — via `cargo clean` |
| **Read-only scan** | `cargo cleanme scan --all`<br>`cargo cleanme scan ROOT`<br>`cargo cleanme scan --known` | The machine reconciliation, a bounded explicit scan, or the routine inventory. **A bare `cargo cleanme scan` is a usage error** (exit 2) — the `scan-scope` ArgGroup requires exactly one selector. | No |
| **Config** | `cargo cleanme config path\|show\|edit` | Resolve, print, or open the configuration file. `config --format json` is a **hard rejection** (exit 2); `--format log` is honoured. | No (except `edit`, which edits the file) |
| **Self-update** | `cargo cleanme update` | Resolve and apply the latest published release. | Yes — replaces the binary |

**The root is the single cleanup front door.** `cargo cleanme` (bare),
`cargo cleanme ROOT` and `cargo cleanme --full` are the three cleanup scopes;
`cargo cleanme clean …` is a **hidden** compatibility alias for the same
intents, kept working because it has been published since 0.2.4. The alias
carries its own hidden copies of the same flag names, and the two levels
*merge*: a value given at the root wins, a value given on `clean` is used when
the root did not supply one, and the **same** override at both levels is a
usage error (exit 2) rather than a silently discarded flag.

Root-level flags: `--config PATH`, `--no-progress`, `--stats`, `--verbose`,
`-i`/`--interactive`, and `--format human|json|log` are **global**;
`--dry-run` (hidden alias `--dryrun`) is root-level but **not** global, so
`clean` and `update` each declare their own `--dry-run`.

Root-level cleanup overrides, all **non**-global: `--full`, the ROOT
positional, `--min-reclaimable-bytes N`, `--older-than SECONDS` (the config key
is `cleanup.policy.min_inactive_seconds`), `--include GLOB` (repeatable),
`--exclude GLOB` (repeatable), `--profile NAME`, `--package NAME`,
`--cargo-preview` (hidden alias `--yes`).

A cleanup flag handed to a read-only or local-only subcommand — `scan`,
`config`, `update` — is a **usage error** (exit 2), never a silently
discarded flag.

`--include` and `--exclude` are matched against the **workspace root**, not
the physical output root — a pattern like `**/target` matches nothing.

Exactly two short forms exist in the whole CLI: `-i` (global) and `-a`
(`scan`-only), plus clap's own `-h` and `-V`.

Four spellings are deliberately **hidden** compatibility aliases rather than
removals: `--dryrun` (for `--dry-run`), `--yes` (for the now-default
Execute mode), `scan --full` (for `scan --all`), and the `clean` subcommand
itself. `man/cargo-cleanme-clean.1` still ships and still documents a working
spelling; it is generated from the same clap model, so it cannot drift.

> **One collision to know.** A directory whose name equals a subcommand name is
> read as that **subcommand**: `cargo cleanme scan` is the scan, not a cleanup
> of `./scan`. Spell such a directory `./scan` to name it.

---

## 3. The crate at a glance

| Property | Value | Where it comes from |
|---|---|---|
| Source files | 18 | `src/` |
| Library modules | 16 | `src/lib.rs` |
| Total lines in `src/` | 43,057 | `wc -l src/*.rs` |
| Production lines | 18,777 | total minus every `#[cfg(test)]` scope |
| Inline `#[test]` functions | 602 | `#[test]` attributes in `src/` |
| Integration test files | 2 | `tests/cli_contract.rs`, `tests/end_to_end.rs` |
| Contract-checker scripts | 17 | `scripts/` — the set is machine-gated, see §9 |
| GitHub workflows | 6 | `.github/workflows/` |
| Async runtime | none | the crate never awaits |

Runtime dependencies: `clap` 4.5, `crossterm` 0.28, `directories` 6,
`dua-core` 4.1, `filesize` 0.2, `globset` 0.4, `indicatif` 0.18.6, `serde` 1,
`serde_json` 1, `thiserror` 2, `toml` 0.8, and the `eggup-*` 0.1.2 family for
self-update. `clap_complete` and `clap_mangen` are optional behind the
`dev-tools` feature and exist only so the `generate-docs` binary can emit
`completions/` and `man/`.

`dua-core` and `eggup-*` are the only substantial third-party code in the
crate. `dua-core` supplies the parallel directory walk; `eggup-*` supplies
acquisition, transport, integrity, staging, and rollback for the updater. The
project's own statement is that generic acquisition machinery is *not*
reimplemented here.

### 3.1 Module sizes

`total` is the whole file; `prod` is everything outside every `#[cfg(test)]`
scope — including the *nested* test modules, which is why `output.rs` has 540
production lines rather than the 272 a naive "lines before the first
`#[cfg(test)]`" reading would report. `tests` counts `#[test]` attributes in
the file itself; the integration suites in `tests/` are counted separately in
§9.

| Module | total | prod | tests | Module | total | prod | tests |
|---|---:|---:|---:|---|---:|---:|---:|
| `cleanup.rs` | 12174 | 4536 | 138 | `output.rs` | 1113 | 543 | 22 
| `workspace.rs` | 6930 | 2418 | 87 | `progress.rs` | 1002 | 542 | 16 
| `update.rs` | 3342 | 1499 | 54 | `report.rs` | 387 | 190 | 11 
| `discovery.rs` | 2646 | 1125 | 42 | `traverse.rs` | 1098 | 770 | 12 
| `interactive.rs` | 3606 | 1817 | 76 | `domain.rs` | 500 | 471 | 1 
| `config.rs` | 1547 | 616 | 21 | `editor.rs` | 292 | 160 | 5 
| `main.rs` | 2513 | 2008 | 12 | `error.rs` | 16 | 16 | 0 
| `cli.rs` | 2874 | 980 | 61 | `lib.rs` | 16 | 16 | 0 
| `policy.rs` | 1666 | 825 | 26 | | | | 
| `discovery_state.rs` | 1335 | 636 | 18 | | | | 
| `cleanup.rs` | 11163 | 4241 | 126 | `output.rs` | 1065 | 540 | 21 |
| `workspace.rs` | 6301 | 2160 | 79 | `progress.rs` | 948 | 518 | 15 |
| `update.rs` | 2886 | 1289 | 47 | `report.rs` | 365 | 168 | 11 |
| `discovery.rs` | 2647 | 1146 | 41 | `traverse.rs` | 624 | 494 | 6 |
| `interactive.rs` | 2985 | 1493 | 67 | `domain.rs` | 480 | 451 | 1 |
| `config.rs` | 1499 | 613 | 20 | `editor.rs` | 215 | 155 | 2 |
| `main.rs` | 2244 | 1864 | 9 | `error.rs` | 16 | 16 | 0 |
| `cli.rs` | 1973 | 777 | 45 | `lib.rs` | 16 | 16 | 0 |
| `policy.rs` | 1660 | 875 | 24 | | | | |
| `discovery_state.rs` | 1215 | 634 | 17 | | | | |

Two observations a reader should carry forward. **Coverage is concentrated**:
`cleanup.rs` and `workspace.rs` alone hold 225 of the 602 inline tests, and
`main.rs` — where the two pipelines are stitched together — is where the
orchestration decisions live. And **the biggest files are the biggest risk**:
`cleanup.rs` is where bytes get deleted.

`main.rs` holding only twelve tests is not a neutral fact. They cover the
orchestrator's own decisions — the interactive surface's three endings, the
report verdict, and the M018 coverage gate's three endings — and nothing
else; the progress-bar ordering below is still unpinned. C023 established that the
cleanup progress bar is cleared *before* the fallible cleanup call in
`main.rs`, and that ordering is unreachable from `src/progress.rs`: reverting
it leaves every test green, because the bar only draws on an attended terminal
and the failing-cleanup contract test passes `--no-progress`. Pinning it needs
a pty harness. It is recorded as an open coverage gap in
[`c023-status.md`](../plans/closure/distribution-release-update/c023-status.md),
not as a claim that the ordering is verified.

---

## 4. Module index — the discrete overview

Every module below is a discrete component with one job, one set of
invariants, and one deep dive. The **Owns** line says what the module is
responsible for; the **Breaks** line says what a defect there would cost.

### 4.1 `domain.rs` — the shared vocabulary

- **Owns:** the types every other module speaks. `ScanScope` (`src/domain.rs:12`),
  `EffectiveScanPolicy` (`src/domain.rs:28`), `ScanReport` (`src/domain.rs:87`),
  and the ownership classification `OutputOwnershipClass`
  (`src/domain.rs:255`) that invariant 1 is literally written in. It also owns
  the seven diagnostic categories (`src/domain.rs:68-78`) and the four-field
  `UnresolvedOwnershipParticipant` (`src/domain.rs:194-199`), whose
  `capability` field is what decides whether an unresolvable manifest blocks
  the run or is merely reported.
- **Depends on:** nothing in this crate. It is a true leaf.
- **Breaks:** if the vocabulary is wrong, every other module is wrong in the
  same way and nothing can be checked against it. A missing enum variant is a
  silent contract change in the JSON output.
- **Deep dive:** [01 — Domain & errors](01-domain-and-errors.md)

### 4.2 `error.rs` — the fatal-error type

- **Owns:** `AppError` (`src/error.rs:3-4`) and its six variants — `Config`,
  `InvalidRoot`, `Io`, `Scan`, `Provenance`, `Update`. Every one of them maps
  to exit code 2.
- **Depends on:** nothing in this crate.
- **Breaks:** a new variant is a new exit-code mapping, and the log-mode
  `reason=` code table in `main.rs` has to learn it too.
- **Deep dive:** [01 — Domain & errors](01-domain-and-errors.md)

### 4.3 `lib.rs` — the module manifest

- **Owns:** the 16 `pub mod` declarations that define the public crate surface.
- **Depends on:** nothing. A published crate's public API is semver-visible,
  so this file is load-bearing.
- **Deep dive:** [01 — Domain & errors](01-domain-and-errors.md)

### 4.4 `cli.rs` — the argv surface

- **Owns:** the clap definition, the `cargo cleanme` → `cargo-cleanme` argv
  normalization, and the reduction of *every* accepted spelling to one
  `Invocation` (`src/cli.rs:326`) carrying a `CleanupRequest`
  (`src/cli.rs:296`), a `ScanIntent` (`src/cli.rs:262`), a `ConfigCommand`
  (`src/cli.rs:229`), or an update intent. The `scan-scope` `ArgGroup`
  (`src/cli.rs:139`) expresses "exactly one scope selector" once, so
  requiredness and mutual exclusion cannot drift apart.
- **Also owns:** the mode mapping `clean_mode` (`src/cli.rs:349`) that turns
  the `--dry-run` / `--cargo-preview` flag triple into one `CleanMode`, and
  the hidden-alias declarations.
- **Since M019:** the root **is** the cleanup front door.
  `root_cleanup_request` (`src/cli.rs:471-474`) resolves the bare, `ROOT`, and
  `--full` spellings; `merged_cleanup_request` (`src/cli.rs:499-504`) merges
  the hidden `clean` alias with the root's own flags.
  `reject_cleanup_flags_for` (`src/cli.rs:561-565`) turns a cleanup flag
  handed to `scan`, `config` or `update` into a usage error, and the same
  override at both levels is a usage error too (`src/cli.rs:646-661`).
  `Invocation::UsageError` is therefore **reachable**, not dead code.
- **Depends on:** `cleanup` (for `CleanMode` and `CleanupSelection`),
  `error`, `policy`.
- **Breaks:** a spelling that resolves to a different intent than its twin is
  the "worst failure mode this tool has" — a silently discarded root flag.
- **Deep dive:** [02 — CLI](02-cli.md)

### 4.5 `config.rs` — configuration

- **Owns:** loading, first-use creation, and validation of `config.toml`; the
  embedded template (the root `config.toml`, included with `include_str!`);
  the path resolver for `--config` and the platform default location; and the
  schema of every key the rest of the crate reads.
- **Depends on:** `error` only.
- **Breaks:** a default that changes silently alters every run on every
  machine. The template is a hard requirement of the published crate —
  `scripts/check-release-contract.py` fails the build if it is dropped from
  the `Cargo.toml` `include` allowlist.
- **Deep dive:** [03 — Config & editor](03-config-and-editor.md)

### 4.6 `editor.rs` — the `config edit` launcher

- **Owns:** resolving `$EDITOR` / `$VISUAL` for `config edit`, with the
  fallbacks and the refusal when neither is set.
- **Depends on:** nothing in this crate.
- **Deep dive:** [03 — Config & editor](03-config-and-editor.md)

### 4.7 `policy.rs` — the scope decision

- **Owns:** the Routine/Full decision. `resolve` (`src/policy.rs:19`) turns a
  `ScanRequest` plus config into an `EffectiveScanPolicy`: which roots, which
  filters, which recency window, which scope label. `ScanScope` has four
  variants (`src/dosrc/main.rs:11-18`) — `Explicit`, `ExplicitRoots`, `Global`,
  `Routine` — of which `resolve` produces three; `ExplicitRoots` is
  constructed only by the cleanup engine. The module also owns C028
  provenance admission for automatic roots, C030 maintenance-root hygiene, and
  the bounded C033 omission summaries, and the `full_cleanup_roots` gate that
  judges the Full cleanup's learned roots **per root**
  (`src/policy.rs:571-620`): it returns a `FullCleanupVerdict` carrying the
  proven roots, the unproven roots with their reasons, and the fresh
  generation. A missing fresh generation is still fatal (C028); a non-zero
  reconciliation code no longer refuses wholesale.
- **Depends on:** `config`, `domain`, `error`, `discovery`, `discovery_state`.
- **Breaks:** this module decides *what is in scope*. A wrong scope either
  misses eligible work or pulls unrelated work into a cleanup. It is the
  boundary between "the tool found something" and "the tool may act".
- **Deep dive:** [04 — Policy & scope](04-policy-and-scope.md)

### 4.8 `discovery.rs` — finding manifests

- **Owns:** the bounded walk that finds `Cargo.toml` files.
  `discover_manifests_with_attribution` (`src/discovery.rs:336`) returns the
  manifests *and* the reason anything was skipped, so coverage loss is
  recorded rather than silently swallowed. It also owns the platform
  prune lists — `effective_rustup_home` (`src/discovery.rs:145`),
  `effective_cargo_home` (`src/discovery.rs:164`), `cargo_home_prunes`
  (`src/discovery.rs:176`).
- **Since M018:** it also owns `verify_root_coverage`
  (`src/discovery.rs:586-592`), the pass that re-walks each learned root as
  its own walk root, under the same prune and filter sets as the platform
  walk, so the Full cleanup's proof boundary can be decided per root.
- **Depends on:** `domain`, `error`, `policy`, `progress`, and the leaf `traverse`.
- **Breaks:** an unrecorded walk error is an incomplete scan reported as
  complete — which would let an incomplete Full reconciliation authorize
  cleanup. The attribution is what makes coverage decidable.
- **Deep dive:** [05 — Discovery](05-discovery.md)

### 4.9 `discovery_state.rs` — learned roots

- **Owns:** the persisted state file: which roots have been learned, how they
  are reconciled, and when they may be retired. `reconcile_full`
  (`src/discovery_state.rs:237`) folds a Full scan's observations into the
  learned set; `publish` (`src/discovery_state.rs:527`) writes it atomically.
  It also owns `automatic_root_is_maintainable`
  (`src/discovery_state.rs:114`) — the C030 hygiene rule — and the
  uncertainty-intersection test.
- **Since M018:** `reconcile_full` takes a `ReconciliationMode`
  (`src/discovery_state.rs:211-235`) instead of a `complete` flag.
  `IncompleteDiscard` (the read-only `scan --all`) publishes nothing;
  `IncompleteRetain` (the Full cleanup) keeps every prior learned root in
  memory, because an incomplete traversal proves no absence — nothing expires
  and no root narrows. The retained generation is consumed in memory and
  never published.
- **Depends on:** `discovery`.
- **Breaks:** learned state is **advisory, never authority**. A persistence
  failure must never change an exit code, and a Full scan that could not
  observe a root must never be allowed to retire it.
- **Deep dive:** [06 — Discovery state](06-discovery-state.md)

### 4.10 `workspace.rs` — resolution and grouping

- **Owns:** everything Cargo-shaped. `resolve_workspaces`
  (`src/workspace.rs:459`) shells out to `cargo metadata` once per unique
  canonical workspace root, through the `CargoRunner` trait
  (`src/workspace.rs:29`) so tests can substitute a fake. It owns capability
  probing (which `cargo clean` features this Cargo supports),
  `attribute_unknown` (`src/workspace.rs:419`) for classifying a resolution
  failure, physical grouping — `build_groups` (`src/workspace.rs:1381`)
  collapses nested and duplicate output roots so bytes are counted once — and
  `analyze_groups` (`src/workspace.rs:1920`), which measures and classifies.
- **Depends on:** `domain`, `progress`, and the leaf `traverse` (plus `cleanup`
  test-only).
- **Breaks:** a group that double-counts or fails to collapse makes the
  reported reclaimable bytes wrong; a resolution failure that is not
  classified becomes an unattributable error instead of a typed reason.
- **Deep dive:** [07 — Workspace](07-workspace.md)

### 4.11 `traverse.rs` — measurement and activity

- **Owns:** how big an output directory is and whether it is still in use.
  `measure_many_targets` (`src/traverse.rs:185`) measures in parallel;
  `source_activity` (`src/traverse.rs:311`) and `workspace_member_activity`
  (`src/traverse.rs:411`) classify a target as active or idle.
- **Depends on:** nothing in this crate. It is a true leaf.
- **Breaks:** the activity check is the *only* thing standing between a
  workspace being built right now and its output being deleted. It checks the
  mtime of every entry it walks, **including directories** — backdating a
  workspace's files is not enough to make it look idle.
- **Deep dive:** [08 — Traverse](08-traverse.md)

### 4.12 `cleanup.rs` — authorization, proof, and execution

- **Owns:** the safety story. `CleanMode` (`src/cleanup.rs:36`) selects
  Execute, Simulate, or CargoPreview. `CleanupSelection`
  (`src/cleanup.rs:85`) says which units are candidates. `is_authorized`
  (`src/cleanup.rs:1117`) and `covering_is_authorized_roots`
  (`src/cleanup.rs:1155`) implement the rule that *authorization never
  manufactures proof* — a non-`PrivateBounded` class returns `Ok(false)`
  before the path is even inspected. `CleanupReasonCode`
  (`src/cleanup.rs:348`), `PolicyDisposition` (`src/cleanup.rs:455`), and
  `CleanReport` (`src/cleanup.rs:559`) are the report vocabulary, and
  `clean_with_roots_policy_selector_guarded`
  (`src/cleanup.rs:1688`) is the engine entry point.
- **Also owns:** the `CleanupRunner` trait (`src/cleanup.rs:1041`) — the spawn
  seam — and the admission-premise late recheck, so the premise that admitted
  a root is re-verified immediately before each spawn.
- **Since M018:** it also owns `CleanupSelection::ExcludeRoots`
  (`src/cleanup.rs:103-129`) — a narrowing filter meaning "every unit the
  inner selection admits, except those under an unproven learned root" — and
  its `excluding` (`src/cleanup.rs:198-219`) and `exclusion_reason`
  (`src/cleanup.rs:250-269`) methods. The exclusion is applied **after**
  ownership classification, so a shared output between a proven root and an
  unproven one stays `Shared`; it removes candidates only and can never add
  one or reclassify one. It is also the one filter shape that may move the
  **proof boundary** — it states a *proof* fact (that root's traversal did
  not complete), and ADR 008 §5 makes the Full cleanup's boundary the
  *proven* learned root, so an unreadable region inside an unproven root
  blocks that root and not the scope. Every other unreadable root still
  blocks the scope that contains it. Its typed reason is
  `skipped_incomplete_coverage`.
- **Depends on:** `discovery`, `domain`, `error`, `policy`, `progress`, `report`,
  `traverse`, `workspace` (plus `cli` and `output` test-only).
- **Breaks:** everything. A proof that passes when it should fail is the one
  defect this crate cannot survive.
- **Deep dive:** [09 — Cleanup](09-cleanup.md)

### 4.13 `report.rs` — human rendering

- **Owns:** the human-readable rendering of a `ScanReport` — `render`
  (`src/report.rs:134`) — and `format_bytes` (`src/report.rs:25`).
- **Depends on:** `domain` only.
- **Deep dive:** [10 — Reporting](10-reporting.md)

### 4.14 `output.rs` — the machine contract

- **Owns:** the versioned JSON contract. `EnvelopeV1<T>`
  (`src/output.rs:8`) wraps every machine document, and `scan`
  (`src/output.rs:105`) and `cleanup` (`src/output.rs:161`) map domain
  structs to DTOs **by hand**, so an internal refactor cannot silently change
  the CLI's machine contract. It also owns the bounded `--format log` line
  renderer (`src/output.rs:309-310`), whose lines are capped at
  `MAX_BYTES = 384` (`src/output.rs:319`), and the one non-envelope output,
  `update_json` (`src/output.rs:903`).
- **Depends on:** `cleanup`, `domain`, `update`.
- **Breaks:** the JSON envelope is a published contract. A field that changes
  type or disappears breaks every consumer that trusted it.
- **Deep dive:** [10 — Reporting](10-reporting.md)

### 4.15 `progress.rs` — the transient renderer

- **Owns:** the `ProgressObserver` trait (`src/progress.rs:39`) — the seam
  every long-running phase reports through — the `indicatif` renderer, and
  the terminal capability decision `should_show_progress`
  (`src/progress.rs:163`).
- **Depends on:** `error`. (`report` is **test-only**: until M020 the renderer
  wrote its candidate rows through `report::format_bytes`; the single-line
  renderer composes its own text, so the production edge is gone.)
- **Breaks:** progress writes to a terminal. If it writes when it should not,
  it corrupts a machine-readable stream; if it draws when the terminal is
  absent, it corrupts a log.
- **Deep dive:** [11 — Progress](11-progress.md)

### 4.16 `update.rs` — self-update

- **Owns:** everything about replacing the running binary. Provenance
  classification — `classify_provenance` (`src/update.rs:386`) answers *may
  this file be replaced at all* — version authority
  (`published_stable_version`, `src/update.rs:755`), the `UpdateEnvironment`
  trait (`src/update.rs:814`) that makes the transaction testable, and
  `update` (`src/update.rs:1285`), the top-level entry.
- **Depends on:** `error` only.
- **Breaks:** a self-update that replaces a file it does not own destroys a
  Cargo-managed installation. This module is **fail-closed** since C018: it
  refuses rather than guesses.
- **Deep dive:** [12 — Self-update](12-self-update.md)

### 4.17 `interactive.rs` — the selection TUI

- **Owns:** the operator-facing selection surface behind `-i`. `Session`
  (`src/interactive.rs:204`) is a pure state machine — mode, cursor, filter,
  selection, geometry — that renders itself to a `Vec<String>` and performs no
  IO. The `Terminal` trait (`src/interactive.rs:120`) is the only thing that
  touches a real terminal; `drive` (`src/interactive.rs:1120`) is the loop, and
  `run_interactive` (`src/interactive.rs:1280`) is the entry point.
- **Depends on:** `cleanup`, `report` (plus `domain` test-only).
- **Breaks:** a tool that can leave a terminal in raw mode is not shippable.
  The `RawModeGuard` restores on every exit path, including a panic.
- **Shape since C039:** the frame is a **bounded page** of at most fifteen
  candidate rows (`src/interactive.rs:354-358`), never more than the terminal
  has room for; a row is **named** by the shortest path suffix that identifies
  it (`src/interactive.rs:919-933`); the reclaimable size is right-aligned; the
  ownership column is dropped when it would say the same thing on every row;
  and the surface is the only one in the crate that renders colour
  (`src/interactive.rs:1100-1102`). The NOR/INS model and the published key
  table are unchanged.
- **Deep dive:** [16 — Interactive](16-interactive.md)

### 4.18 `main.rs` — the orchestration spine

- **Owns:** composition. `main` (`src/main.rs:11`) parses, resolves one
  intent, and dispatches. `run` (`src/main.rs:72`) is the switch over the
  four intents. `run_cleanup` (`src/main.rs:339`) is the cleanup pipeline,
  `run_scan` (`src/main.rs:1416`) is the read-only pipeline,
  `resolve_cleanup_roots` (`src/main.rs:919`) decides the root set,
  `collapse_roots` (`src/main.rs:1357`) canonicalizes and de-duplicates it,
  `run_engine_guarded` (`src/main.rs:683`) wraps the engine with the
  admission premise, and `emit_cleanup` (`src/main.rs:1235`) projects the
  result.
- **Since M018:** the Full cleanup branch of `resolve_cleanup_roots`
  (`src/main.rs:948-991`) runs the verification pass when the platform walk
  was incomplete (`src/main.rs:866-913`), applies the coverage exclusion to
  the declared selection, discloses each unproven root on stderr
  (`src/main.rs:389-401`), and exits 1 when any root was unproven
  (`src/main.rs:812-821`). When **no** root could be proven it blocks with
  the standard unproven-ownership scope block (`src/main.rs:776-806`) — it
  never exits 0 with zero rows.
- **Depends on:** the whole library. It is the only module with no tests.
- **Breaks:** it is the only place the two pipelines are stitched together, so
  a wiring error here is invisible in every module-level test.
- **Deep dive:** [13 — Orchestration](13-orchestration.md)

---

## 5. The layer map

Dependencies flow downward. The edges below are the **production** edges,
derived from the source — a reference inside a `#[cfg(test)]` module is not
listed here, and is called out separately in §5.2.

```text
                     ┌──────────────────────────────────────────┐
  binary entry       │  main.rs        orchestration spine      │
                     └────────────────┬─────────────────────────┘
                                      │
     ┌──────────┬──────────┬──────────┬┴────────┬───────────┬──────────────┐
     ▼          ▼          ▼          ▼         ▼           ▼              ▼
 ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌──────────┐  ┌──────────┐
 │  cli   │ │ config │ │ update │ │ output │ │progress│ │interactive│  │  policy  │
 │ (args) │ │(policy) │ │(selfupd│ │(JSON   │ │(render)│ │   (TUI)   │  │  (scope) │
 └───┬────┘ └───┬────┘ └───┬────┘ └───┬────┘ └───┬────┘ └────┬─────┘  └────┬─────┘
     │          │          │          │          │            │             │
     │          │          │          │          │            │             │
     ▼          ▼          ▼          ▼          │            │             │
 ┌────────┐ ┌──────┐  ┌──────┐  ┌───────┐      │            │             │
 │cleanup │ │disco-│  │error │  │report │      │            │             │
 │(auth,  │ │very  │  └──────┘  └───┬───┘      │            │             │
 │ proof, │ │state │               │          │            │             │
 │execute)│ └──┬───┘               │          │            │             │
 └───┬────┘    │                   │          │            │             │
     │         │                   │          │            │             │
     │         ▼                   ▼          ▼            │             │
     │   ┌──────────┐        ┌──────────┐ ┌───────────┐    │             │
     │   │ discovery│        │ progress │ │  domain   │◀───┘             │
     │   │(manifests│◀───────┼──────────┘ │  (vocab)  │                  │
     │   │ + prune) │        └──────────┘ └───────────┘                  │
     │   └────┬─────┘                                                ▲     │
     │        │                                                        │     │
     │        │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │     │
     │        └─▶│  policy  │─▶│ workspace│─▶│ traverse │─────────────┘     │
     │           └──────────┘  └──────────┘  └──────────┘                   │
     │                ▲                                                   ▲     │
     │                └──── discovery → policy → discovery_state → discovery ─┘    │
     └──────────────────────────────┐                                        │
                                    ▼                                        │
                             ┌──────────┐                                    │
                             │  error   │◀───────────────────────────────────┘
                             └──────────┘
```

The two features worth noticing in that picture: `discovery ⇄ policy` is the
core production cycle — and it is joined by a second one through
`discovery_state` — while `cleanup` sits **above** the resolution layer rather
than below it, because the engine calls into discovery and workspace rather
than the other way round.

### 5.1 The leaves

Four library modules import nothing from the crate: `domain.rs`,
`editor.rs`, `error.rs`, and `traverse.rs`. (`lib.rs` and `main.rs` are
outside the internal graph.) That is the load-bearing structural fact of this
codebase: **the safety vocabulary has no dependencies on the machinery that
could be tempted to misuse it.**

Two modules that a casual reading would expect to be leaves are not:
`discovery_state.rs` imports `discovery` (`src/discovery_state.rs:143`), and
`cli.rs` imports `cleanup` (`src/cli.rs:1`) for `CleanMode` and
`CleanupSelection`.

### 5.2 The two production cycles, and the test-only back edges

The production graph has exactly **two** elementary cycles, and they share an
edge:

```text
discovery → policy → discovery
discovery → policy → discovery_state → discovery
```

The edges that close them are:

| Edge | Line | Code |
|---|---|---|
| `policy → discovery` | `src/policy.rs:871` | `crate::discovery::effective_rustup_home()` |
| `discovery → policy` | `src/discovery.rs:343` | `crate::policy::global_discovery_policy()` |
| `policy → discovery_state` | `src/policy.rs:278` | the learned-root classification helpers |
| `discovery_state → discovery` | `src/discovery_state.rs:143` | `crate::discovery::effective_cargo_home()` |

Each side asks the other one question and neither recurses, so the cycles are
small and deliberate. But they are real: **any claim that the production graph
is a strict DAG is false**, and a reviewer who assumes it is a DAG will
mis-predict what a change to `policy.rs` can affect.

The **test-only** back edges are numerous and are not production dependencies:

| Module | Test-only references |
|---|---|
| `cleanup` | `cli`, `domain`, `output`, `policy`, `progress` |
| `output` | `cleanup`, `domain`, `update` |
| `policy` | `config`, `discovery_state`, `error` |
| `workspace` | `cleanup`, `progress` |
| `progress` | `error`, `report` |
| `discovery` | `progress` |
| `interactive` | `domain` |

Two of these close a production edge in the other direction and so form a
cycle once tests are included: `cli → cleanup` is production while
`cleanup → cli` is test-only, and `output → cleanup` is production while
`cleanup → output` is test-only. That is the standard Rust arrangement — a
test module may reach back into the layer below it — and it is why "no module
imports a module that imports it back" has to be stated as a claim about
*production* edges specifically.

`output.rs` is the only file with a nested test module inside production code
(`src/output.rs:273`), which is why its edges have to be checked by hand
rather than by boundary arithmetic. Its apparent `output → output`
self-reference (`src/output.rs:665`) is inside that nested test module, not a
production edge.

---

## 6. The scan pipeline

`run_scan` in `src/main.rs:1416` is the read-only pipeline. Nine steps, in order:

1. **Resolve config** — `config::load_or_create`.
2. **Resolve scope** — `policy::resolve` turns `{cli_root, full}` into concrete
   roots plus a recency window.
3. **Discover manifests** — `discovery::discover_manifests_with_attribution`
   walks the scope for `Cargo.toml`, recording *why* anything was skipped.
4. **Partition uncertainty** — permission-denied / metadata / vanished
   diagnostics are pulled out as `uncertainty`. This vector later decides
   whether learned state may be trusted.
5. **Resolve workspaces** — `workspace::resolve_workspaces` shells out to
   `cargo metadata` once per unique canonical workspace root.
6. **Reconcile state** — for a Full scan, `discovery_state::reconcile_full`
   folds observations into the learned-root set and publishes atomically. The
   reconciliation *mode* decides what an incomplete traversal may do: the
   read-only `scan --all` discards the generation; the Full cleanup retains
   every prior learned root in memory, because an incomplete traversal proves
   no absence (M018).
7. **Build physical groups** — `workspace::build_groups`; nested/duplicate
   output roots collapse to one group so bytes are counted once.
8. **Analyze** — `workspace::analyze_groups` measures each group in parallel
   via `traverse` and classifies activity.
9. **Render** — human (`report::render`) or JSON (`output::scan`).

Steps 3–5 and 7–8 are the expensive parts; step 6 is the part that makes the
next run cheap.

Step 4 is where the ADR 007 boundary is drawn. The filter at
`src/main.rs:1500-1512` pulls out exactly three of the seven
`DiagnosticCategory` variants — `PermissionDenied`, `Metadata`, and
`Vanished` — and leaves `OutOfScope` out on purpose, because an out-of-scope
unprovable is informational, not a coverage loss.

**The read-only maintenance inventory covers the same universe the cleanup
would act on (ADR 009).** `scan --known` is the inventory of the maintenance
scope, so it applies the same rule the cleanup path does: the working
directory joins the Routine scope when the Routine scope does not already
reach it (`src/main.rs:1450-1459`). An inventory that described a smaller
universe than the cleanup would not be an inventory of it.

## 7. The cleanup pipeline

A cleanup is not a scan plus a `rm`. It is a separate, guarded transaction. The
modes are `CleanMode` in `src/cleanup.rs:36`, mapped from flags by
`cli::clean_mode` (`src/cli.rs:349`):

| Mode | Flag | What it does |
|---|---|---|
| `Execute` | default | Proof, then actually removes through Cargo. |
| `Simulate` | `--dry-run` (hidden alias `--dryrun`) | Full ownership proof + selection, with no `cargo clean` process of any kind spawned. |
| `CargoPreview` | `--cargo-preview` | The same final proof, then Cargo's own `clean --dry-run --verbose`. Cargo *is* spawned here; simulation is the mode that spawns nothing. |

Note the deliberate spelling split. Since ADR 003, canonical `--dry-run` means
**simulation** and Cargo's own preview moved to `--cargo-preview`; `--dryrun`
is a hidden compatibility alias selecting the same simulation mode.

Deletion itself is always performed by `cargo clean`, never by direct
filesystem removal, so Cargo's own bookkeeping cannot be bypassed.

Cleanup roots come from three sources, resolved by `resolve_cleanup_roots`
(`src/main.rs:919`): an explicit `ROOT` argument, `--full` (runs a Full scan,
then uses learned roots), or `--known` (uses only the Routine scope). They are
canonicalized and de-duplicated by `collapse_roots` (`src/main.rs:1357`) so
nested roots cannot produce overlapping deletions.

**The Full cleanup's proof boundary is the learned root (M018).** When the
platform walk could not complete, each learned root is re-walked as its own
walk root and the per-root verdict decides whether that root's subtree was
fully read. A proven root is a cleanup candidate; an unproven root is
disclosed on stderr and its workspaces are removed from the *candidate* set,
each still appearing in the report as `skipped_incomplete_coverage`. If no
root could be proven, the operation blocks with the standard
unproven-ownership scope block; if some root was unproven, the exit code is
`1`. The old gate — refuse the whole Full cleanup because the reconciliation's
exit code was non-zero — is gone.

**The working directory joins the Routine scope (ADR 009).** A bare
`cargo cleanme` run also contributes the operator's position: the workspace
that encloses it, when there is one, and the directory itself
(`src/workspace.rs:655`). The two rules that make this safe are that the
universe may only ever **grow** — a wider universe can only reclassify
`shared` bytes to `private`, never the reverse — and that the working
directory joins **only** when the Routine scope does not already reach it
(`src/main.rs:1152-1167`). The second rule is a measurement, not a taste: without
it, a bare run from the home directory turned into a 65-second sweep of
264,183 directories that ended *blocked* on 1,357 unresolvable manifests in
`~/.local/share/Trash`. `cargo cleanme ROOT` is the spelling for that wider
sweep.

The order matters and is not incidental: the root set is resolved *before* the
candidate selection, because `-i` needs resolved roots to list anything at
all, and the proof universe stays wide even when the candidate set is narrow.

---

## 8. Cross-cutting invariants

These are the properties the architecture is shaped around. They are the
things a reviewer should check first in any change.

1. **Ownership proof, not timestamps.** A candidate is only actionable when it
   is classified as private-and-bounded under a resolved ownership universe.
   `OutputOwnershipClass` in `src/domain.rs:255` is the vocabulary; anything
   `Shared`, `Uncertain`, or `ExternalUnproven` is inventory-only, forever.
2. **Authorization never manufactures proof.** `allowed_output_roots` lets a
   user authorize a location for deletion; it cannot upgrade an unproven
   ownership class. `covering_is_authorized_roots` returns `Ok(false)` for any
   non-`PrivateBounded` class *before* it inspects a path. See
   [Cleanup §5](09-cleanup.md).
3. **Re-validate at the last moment.** Proof work is repeated immediately
   before each spawn, because a workspace can change between scan and delete.
4. **Fail closed on uncertainty.** Every uncertainty becomes a diagnostic, and
   uncertainty propagates *into* state reconciliation — an incomplete Full scan
   is never allowed to delete learned roots it could not observe.
5. **One spawn boundary.** All external `cargo` invocations go through the
   `CargoRunner` / `CleanupRunner` traits, so tests can substitute fakes. This
   is the project's main testability seam — and the source of most of its risk.
6. **Human and JSON are separate projections.** `output.rs` maps domain structs
   to versioned DTOs by hand, so internal refactors cannot silently change the
   CLI's machine contract.
7. **Exit codes are a contract.** `0` success, `1` partial failure or blocked
   scope, `2` fatal error — with the caveats in §9 below.
8. **Learned state is advisory, never authority.** A persistence failure never
   changes an exit code.
9. **The discovery universe may only ever grow.** A wider scope can only
   reclassify `shared` bytes to `private` — the class that authorizes
   deletion — never the reverse. ADR 009's working-directory scope is
   therefore *added* to the Routine roots and never substituted for them
   (`src/main.rs:1189-1198`). Anything that narrows the universe re-opens
   ADR 006's shared-output hazard.
10. **An unproven root is excluded from the *candidate* set, never from the
    *discovery* set.** M018's Full gate decides coverage per learned root. A
    root whose subtree could not be fully read has its workspaces removed
    from the candidate set — but they are still discovered, resolved, sized
    and classified, so a shared output between a proven root and an unproven
    one stays `Shared` and stays inventory-only. Narrowing the *candidates*
    cannot reclassify anything; narrowing the *universe* can, which is
    invariant 9's hazard in a new costume.

---

## 9. Tooling and verification surfaces

| Surface | What it is | Deep dive |
|---|---|---|
| `tests/cli_contract.rs` | 83 contract tests over the CLI surface, exit codes, and reason codes. | [14 — Testing](14-testing-and-verification.md) |
| `tests/end_to_end.rs` | 4 end-to-end tests over a real fixture. | [14 — Testing](14-testing-and-verification.md) |
| `tests/common/` | The shared fixture harness. | [14 — Testing](14-testing-and-verification.md) |
| `src/**/*.rs` inline tests | 602 `#[test]` functions, concentrated in `cleanup.rs` (138) and `workspace.rs` (87). | [14 — Testing](14-testing-and-verification.md) |
| `scripts/` (17) | The contract-checker set: release contract, release identity, staged-release validation, installer contract, doc citations, fixture portability, selector qualification, post-release smoke contract, and the release/benchmark tooling. The exact set is machine-gated — a new checker cannot ship undocumented. | [14 — Testing](14-testing-and-verification.md) §5 |
| `.github/workflows/` (6) | `ci.yml`, `post-release-smoke.yml`, `qualify-cargo-selectors.yml`, `release-binaries.yml`, `release-drift.yml`, `validate-staged-release.yml`. | [15 — Distribution](15-distribution-and-release.md) |
| `packaging/` | `install.sh` and `install.ps1` plus their qualification harness. | [15 — Distribution](15-distribution-and-release.md) |
| `release/eggpack/` | `distribution.toml` is the authority for the release target matrix. | [15 — Distribution](15-distribution-and-release.md) |
| `completions/`, `man/` | **Generated** — do not hand-edit. Regenerate with `cargo run --quiet --features dev-tools --bin generate-docs`. | [15 — Distribution](15-distribution-and-release.md) |
| `xtask/` | The `generate-docs` source (feature `dev-tools`); excluded from the published crate. | [15 — Distribution](15-distribution-and-release.md) |
| `docs/` | `USAGE.md`, `AUTOMATION.md`, `INSTALLING.md`, `RELEASING.md`, `TROUBLESHOOTING.md`, `UPDATE.md`. | referenced from the relevant deep dives |
| `examples/` | `traversal-qualification.rs` — the qualification harness. | referenced from the relevant deep dives |
| `plans/` | The decision record: canonical spec, terminology, roadmap, the planning process, ADRs, subsystem roadmaps, milestone plans, closure records. | [Registry](../plans/registry.md) |

Note that `architecture/` is itself outside the `Cargo.toml` `include`
allowlist, so none of this documentation ships in the published crate.

---

## 10. Findings from the deep dives, and their disposition

The deep dives surfaced the items below. Each was then **verified against the
source before any change was made** — three did not survive that check, and one
was materially misdescribed. Status is current as of the commit that fixed
items 1, 7, 8, 10, 11 and 22, as of the v0.2.10 release close-out, which
corrected item 50 and added item 61, and as of the ADR 008 acceptance of
2026-10-21, which added item 62. Items 17, 22, 24, 25, 38, 43, 44, 46, 47,
56, 58 and 62 were re-verified against the M018/M019 code, and item 65 was
added by the same pass.

Items 3-6 are re-verified as of the Phase-11 and Phase-12 closures (all of
C018/C019/C021-C025 are now closed). Their findings were real when recorded and
each is now closed by shipped code; the deep dives that carry them were updated in
the same pass, so these rows point at completed work rather than at a plan.

**Sixty-five** findings are recorded here. The count is re-derivable: it is the
number of numbered rows in the table below.

| # | Finding | Status |
|---|---|---|
| 1 | **JSON `scope` label was derived from CLI flags, not the resolved policy.** A scan configured with `scan.root` resolves to `ScanScope::Explicit` but reported `"routine"`. | **Fixed.** Now derived from `policy.scope` via `main::scope_label`, with a regression test that fails against the old code. |
| 2 | **A failed ownership proof at cleanup time is recorded as a per-candidate skip, so the process can exit `0`.** | **Kept deliberately.** Per-candidate ineligibility is a legitimate skip with a typed reason code; the scan-time path still blocks. See the correction below. |
| 3 | **Self-update provenance classification fails *open* in four paths.** | **Closed (C018).** Fail-closed typed provenance shipped: manager-evidence ambiguity is gone and real self-managed installs are preserved. Every published release from 0.1.6 onward refuses without remote acquisition or mutation when local provenance forbids replacement. |
| 4 | **Release integrity rests on HTTPS plus a self-published checksum — no signature verification anywhere.** | **Closed for the planned scope (M011A).** Immutable-release attestation and verification shipped, enabled, and verified live. Independent signing/SLSA was deliberately deferred and remains an open Eggpack trust-model decision; see [Distribution §7.5](15-distribution-and-release.md). |
| 5 | **The live self-update rehearsal is manual** (`workflow_dispatch`-only), and it is the only thing that has ever caught a self-update defect.** | **Closed (M011C).** Automatic on `release: published`, with deterministic predecessor selection and a bounded crates.io-authority wait. Demonstrated four times; run `37561575727` (v0.2.2) was the first green automatic five-lane run and `37652257315` (v0.2.3) is the fourth. `workflow_dispatch` remains the manual recovery path. |
| 6 | **Two `scripts/` files are wired into nothing** — `validate-staged-release.py` (the sole "published bytes == qualified bytes" proof) and `qualify-cargo-selectors.sh`.** | **Closed (M011B, M011D).** Both are now wired: `validate-staged-release.py` runs automatically on a real Eggpack staged draft before human publication, and `qualify-cargo-selectors.sh` runs on a recurring hosted real-Cargo matrix. |
| 7 | **Ten divergences between `plans/output-schema-v1.md` and the code, in both directions.** | **Fixed.** The code is authoritative; the schema document was corrected — `selector_estimate_bytes` nullity, the 19th `reason_code`, the 6th policy disposition, `effective_policy`'s four fields, `units[].detail` always-present, the `outcome` value set, `mode` always emitted, and the scan exit-code case. |
| 8 | **Exit codes responded to only some stderr warnings**, and a failed discovery-state save contradicted the module's own "state is an optimization only" invariant. | **Fixed** for the state-save case: it no longer changes the exit code, which also makes the Full and Routine branches agree. The remaining warning/exit-code asymmetry is documented, not changed. |
| 9 | **Dead public API** in a published crate. | **Kept, documented** in §10.1 below. |
| 10 | **A reachable panic in `traverse::measure_many_targets`**: duplicate indices are forwarded into `dua_core::walk_roots`, whose `assert_eq!` is not a debug assert. | **Fixed.** A repeated index is now measured once and every occurrence of that index reports the same measurement, with a unit test. |
| 11 | **`config.toml` said the ignore/unignore filters apply "only to global discovery"; the code applies them to Routine too.** | **Fixed.** The template comment now states the real rule: filters apply to global and Routine, and an explicit root bypasses them. |
| 12 | **ADR 001 describes symlinked output roots as producing an "inventory diagnostic"; the code makes them inert — no group and no diagnostic.** | **Open.** Adding a diagnostic is not a free change: the scan-time path blocks on *any* non-empty discovery diagnostic, so a new one here widens the block condition. |
| 13 | **The uncertainty contract is enforced at two different thresholds** — `main.rs` extracts three diagnostic categories, `cleanup.rs` blocks on any diagnostic. | **Open, and arguably intentional:** the cleanup path being stricter is fail-closed. Recorded because the asymmetry is invisible from either module alone. |
| 14 | **`discovery_state`'s `covered` skip can drop a root that also intersects uncertainty, before the uncertainty veto is reached.** | **Not a bug — checked and dismissed.** The collapse always replaces an ancestor with a *descendant*, so the remembered set only ever narrows. Narrowing means less cleanup on the next run, which is fail-safe. |
| 15 | **The JSON contract drops the `capability` of an unresolvable manifest.** `UnresolvedOwnershipParticipant` carries `capability: UnknownCapability` (`src/domain.rs:198`), and `workspace.rs` uses it to decide whether the participant *blocks* the run or is merely reported — but `UnresolvedParticipantV1` (`src/output.rs:68-72`) has only `manifest` / `stage` / `reason`. A JSON consumer cannot tell a Cargo-refused manifest from one that merges everything. | **Open — not yet planned.** This is a contract decision, not a documentation fix: the fix is to add the field, which is a schema change. |
| 16 | **`CleanReport.out_of_scope_unresolved` (`src/cleanup.rs:578`) is rendered in the human report (`src/cleanup.rs:699-704`) but has no field in the JSON cleanup DTO.** ADR 007's "the system is not clean, the boundary is" signal exists only in prose. | **Open — not yet planned.** Same shape as item 15: a missing JSON projection. |
| 17 | **`main.rs`'s uncertainty filter is a silent `matches!` over three of the seven diagnostic categories** (`src/main.rs:1503-1510`) — no `_` arm, so it is not compile-checked. `OutOfScope` is correctly excluded, but a future eighth category would silently reclassify an incomplete scan as complete. | **Open — not yet planned.** The right shape is an exhaustive match or an explicit allow-list with a test that fails on a new variant. |
| 18 | **`OutputRootKind`'s human label builder has a `_` arm** (`src/workspace.rs:2025-2031`), so it is *not* compile-checked the way the sort key at `src/workspace.rs:2020-2023` is. A new kind renders as `"output"` rather than failing to build. | **Open — not yet planned.** |
| 19 | **`AutomaticOmission::NotMaintainable` conflated two different causes** — "the path does not exist at a transient origin" and "the path exists but is a non-maintainable location" — and the consequence was that `omission_diagnostics` printed "omitting N *non-maintainable* learned root(s)" for roots that are simply absent. | **Fixed in C038.** The by-design bucket is no longer reported at all: a root omitted because it sits at a known-transient origin is omitted *by design* — the tool would never have cleaned it — so it is not a diagnostic and never reaches the operator. A durable root that has vanished, and a root that is no longer a directory, are still reported. |
| 20 | **`--include` / `--exclude` match the workspace root, not the physical output root** (`src/cleanup.rs:1876`, `src/cleanup.rs:1886`), and neither the flag help nor the config key name says so. A user writing `include = ["**/target"]` gets no match and no warning. | **Open — not yet planned.** |
| 21 | **A configured `scan.root` is silently ignored by a rootless `scan`** — `policy::resolve` returns from its `full` arm before the configured root is read. The behaviour is deliberate (`src/main.rs:1429-1432` says configuration must not narrow the canonical reconciliation command), but the config key's own documentation does not mention it. | **Open — not yet planned.** Documentation, not behaviour. |
| 22 | **The root `long_about` claimed a bare `cargo cleanme scan` is the full machine reconciliation** — the `long_about` at `src/cli.rs:21` has since been rewritten. It was a usage error (exit 2) — the `scan-scope` ArgGroup requires one of ROOT / `--known` / `--all`. Only `cargo cleanme --help` showed the lie; the manpage was unaffected because `xtask` overwrote the root `long_about` with its own block. | **Fixed by C040** (ADR 009). The root `long_about` now states that `scan` **needs a scope** and that `--all` is the full read-only machine reconciliation (`src/cli.rs:34-37`), and the root manpage is generated from that same `long_about` rather than from a generator-side block, so the two surfaces can no longer drift. Discharges [C036](../plans/implementation/artifact-discovery-cleanup/c036-cli-surface-truthfulness-and-documentation-drift-corrective.md) D1. |
| 23 | **A user-facing diagnostic points the operator at `scan --full`** (`src/discovery.rs:379-386`) — a *hidden* alias that does not appear in `scan --help`. The canonical spelling is `scan --all`. | **Covered by C036 D3** (proposed, not accepted). |
| 24 | **`cli::absolutize_root` and `main::collapse_roots` disagree about `..`.** The CLI folds `..` lexically; `src/main.rs:1360` canonicalizes, which resolves symlinks first. A relative root whose `..` crosses a symlink names different directories under the two. No test covers the disagreement. | **Open — not yet planned.** |
| 25 | **A root-level `--dry-run` is accepted alongside `scan` and then silently ignored** — the root flag is not global, clap accepts it, and the `Invocation::Scan` arm never reads it. Harmless today because a scan is read-only, but it is the same shape as the discarded-flag defect the `Clean` and `Update` arms were fixed for. | **Open for `--dry-run`; closed for the cleanup flags.** M019 closed the half that mattered: `--full`, ROOT, the policy overrides and `--cargo-preview` handed to `scan`, `config` or `update` are now a usage error (exit 2) rather than a silently discarded flag. `--dry-run` and `--dryrun` are still ignored by `scan` and `config`, which is deliberate — those operations are read-only or local-only, so the flag changes no behaviour. |
| 26 | **One lib test fails on any host whose Cargo version is outside an exact allowlist.** `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` expected `Previewed` and got `Skipped`, because `clean_capabilities_from_version` (`src/workspace.rs:146-164`) enables `profile_selector` only for nine exact versions — `1.89.0`, `1.90.0`, `1.91.1`, `1.92.0`, `1.93.1`, `1.94.1`, `1.95.0`, `1.98.1`, `1.99.0` — and the host here is `1.99.1`. The tool skips in the **safe** direction; the *test* was what failed. The defect was the test's **unstated host premise**, and the exact-version allowlist made it structural: the first patch release of any qualified minor is outside the set, so the case had a known expiry date with no alarm before it. | **Corrected by C037** (`plans/implementation/artifact-discovery-cleanup/c037-host-cargo-dependent-selector-premise-corrective.md`); the hosted release is outstanding. The case now derives its expectation from the product's own premise function and asserts a *specific* outcome per premise — a qualified host must preview, an unqualified one must refuse with `SelectorUnsupported` — rather than a disjunction. Proven by mutation in both directions, and the selector-to-Cargo composition is now covered by a host-independent case. No production behaviour changed: `src/workspace.rs` and `release/selector-qualification.json` are byte-identical. |
| 27 | **`traverse::tests::source_scan_short_circuits_recent_files` does not discriminate what it claims.** It writes one fresh file into a *freshly created* temp dir and walks that dir as the root. The root's own mtime is fresh, the root is not excluded from the walk, and `Order::ParentFirst` yields the root first — so `Recent` is returned on the **directory's** mtime before the file is ever examined. The test would still pass if the file were deleted, if the guard ignored files entirely, or if directories were excluded from the evidence. | **Open — not yet planned.** This is the AGENTS.md §4 "green test" shape exactly: a passing test that cannot fail for the reason it claims. It needs a discriminating fixture pair. |
| 28 | **The C028 narrow-absence rule has *zero* discriminating coverage.** Verified by mutation on a scratch copy: deleting the probe (`src/discovery_state.rs:362-371`) leaves **all 17** `discovery_state` unit tests green and **all 62** `cli_contract` integration tests green. The nominal test `complete_certain_full_prunes_only_positively_absent_learned_roots` (`src/discovery_state.rs:1036`) stages roots under `tempfile::tempdir()` with `home: None`, so the **C030** rule at `src/discovery_state.rs:352-354` drops them first — the temp root is forbidden when `home` is `None` (`src/discovery_state.rs:165-177`). The symmetric mutation (deleting C030 instead) fails two tests, confirming C030 is the rule actually under test. | **Open — not yet planned.** The rule is live in production and untested; only the rule that shadows it is tested. |
| 29 | **`discovery_state::tests::zero_retention_disables_expiration_and_nested_roots_collapse` no longer demonstrates its name.** Same fixture problem: the assertion is now `learned_roots.is_empty()`, so the zero-retention path and the nested-root containment collapse are both unexercised. Only the exact-duplicate fold (`src/discovery_state.rs:1150`) is covered. | **Open — not yet planned.** |
| 30 | **Production `uncertainty` is always empty.** All three uncertainty categories now emit `path: None` (`src/discovery.rs:937`, `src/discovery.rs:937`); `main.rs:1455` drops them, and the same events clear `coverage_complete` (`src/discovery.rs:652`, `src/discovery.rs:652`), so the scan is incomplete and never publishes. The fail-open `filter_map` edge is therefore the only path — and it is covered by the completeness gate, not by the uncertainty veto. The unit tests at `src/discovery_state.rs:942` and `src/discovery_state.rs:1115` assert a behaviour the pipeline cannot produce. | **Open — not yet planned.** |
| 31 | **`discovery_state::sweep_own_stale_temps` (`src/discovery_state.rs:560-563`) has no test at all.** Its own-pid and one-hour scoping is a safety property: it must never delete another publisher's in-flight temp file. | **Open — not yet planned.** |
| 32 | **An unreadable output root is reported as *missing*, not as a diagnostic.** `workspace::has_artifact_entries` returns `Option<bool>`, which cannot distinguish "empty" from "unreadable", so the `None` case folds into `missing_all` (`src/workspace.rs:1637`). An unreadable covering root is therefore reported as absent rather than as a coverage loss — weaker than `output_root`'s own careful "unreadable is not absent" handling (`src/workspace.rs:306-318`). The window is narrow (read permission on the directory only), but it is a fail-closed-reporting hole. | **Open — not yet planned.** |
| 33 | **No test covers a conflicting member→root mapping.** `member_to_root.entry(..).or_insert(..)` (`src/workspace.rs:974-980`) is first-writer-wins; a conflict is neither detected nor diagnosed, and the fast path (`src/workspace.rs:839-845`) means a mis-mapped manifest is never re-located in that pass. `TwoWsRunner` (`src/workspace.rs:3579`) covers only the legitimate two-workspace case. | **Open — not yet planned.** |
| 34 | **`workspace::GroupAnalysisInput` (`src/workspace.rs:1547`) is dead** — declared, never constructed, never referenced in `src/` or `tests/`. It is `pub`, so it is semver-visible. | **Open — not yet planned.** It joins the §10.1 dead-API list if the owner decides to record it there. |
| 35 | **The doc-citation gate checks only the `file.rs:N` form, not the bare `:N` form** — and the bare form is the *dominant* convention inside the deep dives: **2,742** bare `:N` citations across the set, against **2,836** explicit `file.rs:N` citations (the checker's own regex). `architecture/09-cleanup.md` alone has 689, `architecture/07-workspace.md` 448, `architecture/13-orchestration.md` 328, `architecture/05-discovery.md` 327. So most in-file citations are **not** machine-gated; only their file existence and range are. | **Open — not yet planned.** The reviewers compensated by resolving the bare citations against their anchored file in a separate pass and converting script citations to an explicit "script lines N" form. The gate itself was not changed. |
| 48 | **The `-i` selection surface is covered only by its usage-error refusals.** The `Terminal` trait (`src/interactive.rs:120`) is an injectable seam and its `restore` is asserted on unwind (`src/interactive.rs:2042`), but the integration suite runs without a terminal, so the selection session itself has no integration coverage — only the refusals do. | **Open — not yet planned.** A pty-backed integration case would close it; the repo already knows a pty harness is the missing piece (C023 §11.4). |
| 49 | **`scripts/validate-staged-release.py` discards the evidence it collects.** `report()` is `for message in notes: pass` (`scripts/validate-staged-release.py:410-412`). The gate still fails closed and the exit code is right, but the measured glibc floor per Linux target, the manifest-agreement summary, and the "installed bytes match the release asset digest exactly" confirmation never reach the operator or the CI log. | **Open — not yet planned.** The verdict is trustworthy; the supporting numbers are invisible. |
| 50 | **The published-release count is hand-maintained in three places, and drifts.** `docs/INSTALLING.md`, `AGENTS.md`, and `.skills/release-and-distribution/SKILL.md` each state "N releases are published; M are installable" in prose, with no gate over any of them. The count has now drifted at least three times: it read "Fifteen" when there were sixteen, it read "Sixteen" when there were seventeen, and it read "Seventeen" when there were eighteen. The three copies can also disagree with *each other*, and the number is not derivable from the tree — the tags and crates.io are the only authorities, and nothing resolves the prose against them. | **Open — tracked as C036 D29.** Corrected at the v0.2.10 publication (all three now read eighteen/seventeen, re-derived from `git tag` and crates.io), but the *mechanism* is untouched: nothing prevents the next release from drifting the same way. |
| 51 | **`scripts/check-post-release-smoke-contract.py` has a stale docstring.** It still says the smoke workflow "is manually dispatched only, so it cannot run on a push" (`scripts/check-post-release-smoke-contract.py:15`), but M011C made the automatic `release: published` trigger the primary path. The *code* (`scripts/check-post-release-smoke-contract.py:174-208`) was correctly updated and requires exactly `workflow_dispatch` **and** `release: types: [published]` — so the guard is right and only its prose is wrong. | **Open — not yet planned.** The same shape as D2: a guard that is correct while its explanation is not. |
| 52 | **Nothing enforces that the installer destination is never inside a Cargo root** — the property the self-update path depends on. It holds by convention and by reading the source, not by a check. | **Open — not yet planned.** |
| 53 | **The six agent skills in `.skills/` carried fabricated facts, and no gate could have caught them.** `json-and-exit-contract/SKILL.md` asserted exit codes `17` and `18` — neither exists anywhere in `src/`; every `AppError` exits `2` (`src/main.rs:34`). The same file listed four reason codes (`skipped_permission_denied`, `skipped_io_error`, `skipped_never_authoritative`, `skipped_dryrun`) with **zero** occurrences in `src/`, `tests/`, `plans/output-schema-v1.md`, or `docs/`. `cleanup-safety/SKILL.md` named a `Preview` mode (there are three — `Execute`, `Simulate`, `CargoPreview`, `src/cleanup.rs:36-46`) and described the superseded ADR 007 whole-scope block. `discovery-and-ownership/SKILL.md` asserted a `[package] ignore` feature that does not exist and described the duplicate-index panic as live (it was fixed at `src/traverse.rs:191-200`). | **Fixed in this pass.** All six skills were re-derived from source and corrected; two new skills (`cli-surface-and-argv`, `terminal-and-interactive`) cover the trap classes that had none. Root cause is item 54. |
| 54 | **The doc-citation gate covers `architecture/*.md` and nothing else.** `scripts/check-doc-citations.py` resolves the `file.rs:N` form inside `architecture/` only. `.skills/`, `docs/`, and `README.md` are **ungated**, so a skill or a user-facing document can drift indefinitely with no mechanical signal — and it did, in both places. | **Open — not yet planned.** A citation gate over `.skills/` and `docs/` is the mechanical fix; it is not one, because those files are prose-heavy and their citation forms are inconsistent. Until it exists, re-derive skill and doc claims from source rather than copying them forward. |
| 55 | **`architecture/07-workspace.md` carried the superseded ADR 007 whole-scope block** in two places — naming a `Preview` mode that does not exist and saying an unresolved manifest "blocks the whole scope", while `architecture/09-cleanup.md` correctly documented the scoped boundary in the same repository. | **Fixed in this pass.** Corrected to the ADR 007 three-way split, with the boundary rule and the regression test named. |
| 56 | **`architecture/05-discovery.md` asserted that the uncertainty vector "still carries known, path-specific uncertainty into `reconcile_full`"** — while `architecture/06-discovery-state.md` correctly documented that it is always empty. All three uncertainty categories are emitted with `path: None` (`src/discovery.rs:379-386`, `:1121-1143`), so `filter_map(|d| d.path.clone())` (`src/main.rs:1500-1512`) drops every one of them. | **Fixed in this pass.** |
| 57 | **The per-platform inline-test census disagreed across three documents and with itself.** `test-evidence/SKILL.md` claimed 320 declared / 318 Linux / 296 Windows / 317 macOS; `architecture/14-testing-and-verification.md` claimed 450 / 448 / 420 Windows; and a figure of 443/415 was in circulation. The truth is **450 declared, 448 Linux, 411 Windows, 445 macOS** — 35 `#[cfg(unix)]` and 4 Linux-only gates. A naive grep for `#[cfg(unix)]` immediately above `#[test]` finds 26, because nine of the 35 have the gate *after* the `#[test]`. | **Fixed in this pass.** All three documents now state the current census — **531 declared / 529 Linux / 487 Windows / 526 macOS** after M018, M019, and C042 — and the derivation method (a brace-tracking evaluation against `rustc --print cfg --target <triple>`, cross-checked against `cargo test --lib -- --list`) is recorded so the next reader can re-derive rather than copy. The naive grep finds 30 of the 40 `#[cfg(unix)]` gates, because ten have the gate *after* the `#[test]`. **Re-derived 2026-10-24**: the census is **602 declared / 600 Linux / 547 Windows / 596 macOS** — 50 `#[cfg(unix)]` and 5 Linux-only gates — and the naive grep finds **38** of the 50 `#[cfg(unix)]` gates, because twelve have the gate *after* the `#[test]`. The Windows figure is build-verified again: the three test binaries cross-compiled for `x86_64-pc-windows-gnu` carry exactly the 547 derived test names and none of the 55 excluded ones. |
| 58 | **Nine source comments still describe the pre-ADR 007 whole-scope block.** `src/cleanup.rs:464`, `src/cleanup.rs:492`, `src/cleanup.rs:1496`, `src/cleanup.rs:2477`, `src/workspace.rs:394`, `src/main.rs:312`, `src/main.rs:406`, `src/main.rs:679`, `src/main.rs:1172` say "whole-scope" where the behaviour is boundary-scoped. `tests/cli_contract.rs:1938` carries the same wording on a test whose *behaviour* is correct (its unresolved manifest is inside the anchor, so it does block). | **Open — not yet planned.** Deliberately **not** fixed in this pass: the diff is documentation-only by design, and editing `src/` shifts line numbers that 2,742 bare citations depend on — which `AGENTS.md` §5 forbids doing casually. The correction needs a corrective that re-derives the affected citations in the same change. Recorded as C036 D27. |
| 59 | **Several inline tests assert on wall-clock and thread-scheduling observations, so the suite is load-sensitive.** `cleanup::tests::proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` asserts the observed peak proof-refresh concurrency equals the reported counter, and `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` asserts a recency-window reason code. Under CPU contention the observations move and the assertions fail. Measured: on the C038 tree, 8 consecutive `cargo test --all-targets --all-features` runs with an idle host all passed, and the same tree run concurrently with a build failed; on the **unmodified baseline** (`426cb72`) under the same contention, three tests failed — the same two plus `workspace::tests::real_conventional_workspace_is_private_bounded`. | **Open — not yet planned.** Pre-existing, and not a C038 regression: the baseline fails under the same load. The lesson is the repository's own — *a green test proves only that its own premises hold*, and here one of the premises is an unloaded host. A fix belongs with the real-Cargo fixtures, not with a corrective that changed no fixture. |
| 60 | **The `-i` selection surface was a *list*, not a *finder*, and it was built that way on purpose.** The frame was exactly the terminal height (`list_rows()` returned `height - CHROME_LINES`), so the list filled the screen instead of being a window; there was no scroll-position indicator, no paging key (`Key` had no PageUp/PageDown/Home/End), and the absolute path of every row occupied most of the width while the part that distinguishes one workspace from another sat at its far end. Every row repeated the word `private`, whether or not that was news. The surface was **functionally correct** — every defect here is a presentation defect — but the module's own tests asserted the *shape* rather than the *usability*, and one of them (`the_frame_is_exactly_the_terminal_height_and_never_overflows_the_width`) **pinned the defect as the contract**. | **Fixed in C039.** The frame is now a bounded page of at most fifteen rows that never outgrows the terminal; a row is named by the shortest path suffix that identifies it uniquely among the candidates; the reclaimable size is right-aligned; the ownership column is omitted when it would say the same thing on every row; the gutter marks the window edges and the cursor; and the surface renders colour, opt-out via `NO_COLOR`. The NOR/INS model and the published key table are unchanged. The test that pinned the old shape was rewritten to pin the new property, and the evidence is a pty capture of the real binary, not a string comparison — the repository has no pty harness (finding **48**), so the rendered surface had never been observed by a test. |
| 61 | **The update path's candidate spawn can fail with `ETXTBSY` under load, and the failure is a *spawn* failure rather than a *proof* failure.** `run_bounded` (`eggup-core-0.1.2/src/candidate.rs:141`) executes the staged candidate to prove its identity before the commit; when the staged file is still open for writing at that instant, `execve` refuses it with `Text file busy (os error 26)`. The transaction aborts before the commit and the error is reported, so **nothing is deleted and the ownership proof is untouched** — but the same path runs in production during a real `cargo cleanme update`, so a user can be handed a failure they cannot act on. Found by the v0.2.10 release gate: it failed on the first run and passed on the second with an identical tree. `src/update.rs` is untouched by C039 and C040, so the defect is **pre-existing** and not a regression. | **Open — tracked as C041.** The mechanism is narrowed but **not proven**: the next step is instrumentation, not more reading, and the plan requires a reproduction that fails against the current code. Upstream `eggup-core` 0.1.3 was diffed against 0.1.2 and does **not** carry the fix (`run_bounded` is byte-identical), so "pin upstream until it lands" is not available. Re-measured: the test alone passed 40 of 40 consecutive runs, and an 8-way parallel full-suite burst passed 8 of 8. |
| 36 | **`cargo cleanme -i` leaves the terminal cursor hidden when the session ends in NOR mode.** `session_loop` hides the cursor at `src/interactive.rs:1145` and the per-frame call at `src/interactive.rs:1155` sets it to `mode == Insert`. The commit and quit arms (`src/interactive.rs:1159-1163`) rewind and clear **without restoring it**, and `set_cursor_visible(true)` is never called anywhere in the crate. `RawModeGuard` restores the terminal *mode* (`disable_raw_mode`) but cursor visibility is a separate escape sequence; `main.rs` has no cursor handling at all. The existing test `the_driver_shows_the_hardware_cursor_only_in_insert_mode` (`src/interactive.rs:1959-1971`) asserts the *last* recorded value is `Some(false)` — it **pins** the behaviour rather than catching it. | **Open — not yet planned. Verified live.** Driven over a raw pty: the tool emitted `ESC[?25l` (Hide) twice and `ESC[?25h` (Show) **zero** times, then exited 0 with `cargo-cleanme: interactive selection cancelled; no cleanup commands were run`. The operator's shell is left with no visible caret. The module's own argument — "a tool that can leave a terminal in raw mode is not shippable" — applies verbatim to the cursor. |
| 37 | **`-i` and non-`-i` disagreed on the exit code for a scope that has roots but no cleanup units.** `run_interactive` refused an empty candidate list (`src/interactive.rs:1281-1283`), and the orchestrator mapped that to `AppError::Config` (`src/main.rs:644-660` at the time) → **exit 2**. The same scope without `-i` reported zero units and exited 0. | **Fixed in C038.** The empty surface is now a plain negative result: `SessionResult::NothingToClean` → one line, `Nothing to clean.`, on stdout, exit 0. The two spellings of one scope now agree. |
| 38 | **`plans/output-schema-v1.md` is missing a reason code the code actually emits.** The schema listed 19 `reason_code` values while the code emitted **20**; the missing one was `skipped_outside_selection` — the value M014 actually emits for a workspace outside the selected scope (`src/cleanup.rs:1828`). M018 added a twenty-first, `skipped_incomplete_coverage`. | **Closed.** The schema document now carries all twenty-one `reason_code` values, matching `CleanupReasonCode::as_str` (`src/cleanup.rs:426-450`) exactly. |
| 39 | **`plans/output-schema-v1.md` describes a non-UTF-8 path as using "the platform escaped debug form".** `output::path` (`src/output.rs:262-266`) now uses `to_string_lossy()`, so both the human and the machine output render `U+FFFD` and **agree** — the schema's description of a divergence that no longer exists is stale. The schema's own wording ("are UTF-8 strings when representable") is also wrong: a path is always a UTF-8 `String`, representable or not. | **Open — not yet planned.** Note the consequence: `output::path` is lossy and therefore **not injective**, so a JSON path string must not be used as a join key. |
| 40 | **`plans/registry.md` contradicts itself about the self-update smoke.** The M011C row says the automatic smoke is "demonstrated three times" while the evidence table below it lists **seven** green automatic smokes. | **Open — not yet planned.** A registry is a control surface; a self-contradicting one is worse than a stale one, because a reader cannot tell which number to trust. |
| 41 | **`progress::set_determinate_total` (`src/progress.rs:322`) is dead *and* internally inconsistent.** It sets the bar's total without zeroing `determinate_done`, unlike `apply_total` (`src/progress.rs:335`). It is the one path that can leave the displayed total and done-count mismatched. It has zero callers. | **Recorded in §10.1** — M020 confirmed zero call sites and removed its `set_length` call, so the inconsistency is now state only. The public item is kept deliberately (see §10.1's semver note). |
| 42 | **The progress bar's message template hardcoded `"analyzing {done}/{total} …"` whenever a total existed**, so a `CleanupExecute` bar read *"analyzing 3/12 …"*. The root cause was the duplicated counter set, not the label. | **Closed by M020.** The renderer now composes the whole line from the phase label (`src/progress.rs:348`) and the fraction (`src/progress.rs:365`), so every phase names itself: *"scanning…"*, *"resolving workspaces…"*, *"analyzing output 3/12…"*, *"cleaning…"*. The phase also moved out of the bar's *prefix* into its message, because indicatif's default spinner template (`{spinner} {msg}`) never renders the prefix — so the transitions had never been visible at all. |
| 43 | **`--format json` on an attended terminal gets no progress at all.** The format gate at `src/main.rs:1464-1465` suppresses the renderer on anything other than `OutputFormat::Human`, regardless of whether stdout is a pipe. Progress is on **stderr**, so a `jq` consumer is unaffected by it — and the log mode's suppression is independently justified, but the JSON mode's is not. A minutes-long `--format json` scan gives the operator nothing. | **Open — not yet planned.** |
| 44 | **The contract tests do not cover the whole contract.** `cleanup_reason_codes_have_stable_contract_names` (`src/cleanup.rs:4672-4698`) covers **16 of 21** reason codes — it omits `skipped_ownership_unproven`, `skipped_outside_selection`, `skipped_incomplete_coverage`, `selector_unsupported`, and `selector_invalid`. `policy_dispositions_have_stable_json_names` (`src/cleanup.rs:4652-4669`) covers **5 of 6** dispositions — it omits `selector_estimate_unavailable`. | **Open — not yet planned.** The stable-name tests are the only guard on the published string vocabulary, so the uncovered five and one are unguarded — and M018 added a twenty-first code to the unguarded set. |
| 45 | **The positive `allowed_output_roots` path is untested end-to-end.** `config.toml` calls the field "operationally redundant" and the code advertises the remediation at `src/cleanup.rs:1318-1325`, but no test shows a `PrivateBounded` group outside every clean root becoming `Cleaned` through an allowed root. | **Open — not yet planned.** This is the one path where *authorization* — not proof — is what makes a group cleanable, so it is the path most worth an end-to-end case. |
| 46 | **Two error paths in `main.rs` report an unmeasured zero.** `src/main.rs:415` and `src/main.rs:415` build a `CleanReport` after the engine returned `Err`, so the out-of-scope count it had computed was discarded and `out_of_scope_unresolved` is set to `0` — an unmeasured zero on a path that *did* traverse. The other two report sites (`src/main.rs:415`, `src/main.rs:415`) are correct. | **Open — not yet planned.** A machine consumer reading `out_of_scope_unresolved: 0` cannot tell "nothing was out of scope" from "we never got to count". |
| 47 | **The one-JSON-document guard is asserted by a string check, not behaviourally.** `tests/cli_contract.rs:95-3147` asserts that `src/main.rs` contains the text `"enum EmitReport"`. Renaming the enum breaks the test — good — but keeping the enum while every call site passes `EmitReport::Yes` leaves it green, and the two-JSON-documents bug returns. A source check on the *call site* (`src/main.rs:86-93`) would be a stronger, equally cheap assertion. | **Open — not yet planned.** AGENTS.md §4's own lesson in miniature. |
| 62 | **The Full cleanup operation is unreachable for an ordinary user.** `cargo cleanme clean --full` runs a Full reconciliation first (`src/main.rs:1405`), and a platform-root walk by a non-root user is **never** complete, so `policy::full_cleanup_roots` (`src/policy.rs:571-620`) refuses the non-zero reconciliation code (`src/policy.rs:571-620`) and the operation exits **2** before it cleans anything. The refusal is unconditional in practice, not merely common: it happens because regions that are irrelevant to the cleanup — `/var`, `/snap`, `/run` — are unreadable, not because anything about the learned roots is unproven. **Verified live** on 2026-10-21 (uid 1000): exit `2`, stdout empty, 128 permission-denied traversal errors under `/`. The gate is unit-tested, but only with a literal `1` (`src/policy.rs:1352`), so the test proved its own premise and never that a real machine produces that code. | **Closed by M018.** The Full cleanup's proof boundary is now the **learned root**: a root whose traversal completed is proven and cleaned, one whose traversal did not complete is skipped with a typed reason (`skipped_incomplete_coverage`), and the operation proceeds with the roots it can prove. The gate returns a `FullCleanupVerdict` (`src/policy.rs:518-525`) instead of refusing wholesale; the C028 property (no fresh in-memory generation, no cleanup) is unchanged. A run that could not prove every root exits `1`, and a run that proved none blocks rather than exiting 0 with zero rows. M019 then made `cargo cleanme --full` the canonical spelling. The disclosed residual — an unknown participant in an unreadable region outside every learned root cannot be disproven — is the same shape as ADR 007's out-of-boundary residual, and it is disclosed rather than fixed. |
| 63 | **The cleanup's human surface wrote control characters in a path raw.** `cleanup::escaped_path` returned a valid-UTF-8 path unchanged, so a directory whose name contained a newline or an escape sequence wrote that sequence straight into the terminal — the exact hazard `report::path_text` was written to prevent. The machine surface already escaped, so the two surfaces disagreed about the same path. | **Closed by M020.** Both human surfaces now render paths through `HumanOptions::path` (`src/report.rs:50`), which escapes control characters and degrades non-UTF-8 to U+FFFD; `escaped_path` is gone. |
| 64 | **The citation gate cannot detect the defect it exists to prevent, and the corpus had drifted.** `scripts/check-doc-citations.py` resolves the `file.rs:N` form and checks that the file exists and the line is **in range** — it never checks that the line still *says* what the sentence claims. So a citation whose content has moved is silently green. Measured on 2026-10-21: **412 citations across 24 live documents** pointed at lines whose content had moved since the citing line was last written — 70 in `architecture/01-domain-and-errors.md`, 44 in `architecture/overview.md`, 36 in `architecture/14-testing-and-verification.md`, 33 in `architecture/07-workspace.md`, 30 in `architecture/03-config-and-editor.md`, 23 in `architecture/16-interactive.md`, and 19 each in `architecture/08-traverse.md`, `architecture/09-cleanup.md` and C036's own D-table. A reader following a pointer landed on unrelated code. The drift was **pre-existing**: it is the accumulated cost of the rule in `AGENTS.md` §6 that a line-shifting fix invalidates every citation past that point while the prose keeps reading as true. | **The drift is corrected; the gate gap is not.** All 412 were re-derived against the current source — 409 mechanically and 3 by hand — each accepted only when the mapped lines carried **identical** content — never shifted arithmetically — and the mechanical passes' diff was verified to contain **no prose change**: every changed line differed only inside a citation's numbers. The gate gap is tracked as **C036 D34 / WP-10**, which proposes a checked-in citation manifest and a content check; the 224 corrected citations are its baseline. Historical records are exempt by design: closed and superseded plans keep their own baseline citations, and ADR 004 pins its citations explicitly ("as of the M015 landing"). |
| 65 | **The `CleanupOverrides` doc comment still denies the shipped surface.** `src/cli.rs:275-279` says the policy overrides "live only on the `clean` subcommand by design (M012A work package D): bare maintenance intentionally uses the configured/default policy rather than cloning the whole advanced option surface onto the root command." M019 put them on the root — that is the whole point of ADR 008 §4b — and the type's own fields are now read from the root's flags. The comment is not a doc nit: it states the *reason* a reviewer would use to reject the change that was made. Forty-two sibling source comments still spell the pre-M019 invocations (`clean ROOT`, `clean --full`, `clean --known`) where the canonical spelling is now the bare root; those are stale but not false, because the alias still works. | **Open — not yet planned.** The same shape as item 58: the code changed and the prose that describes it did not. It is a one-line source comment fix, but it belongs to whoever owns `src/`, not to a documentation pass. |
| 67 | **The interactive frame did not fit the terminal it was painted on, in either axis, and the test that should have caught it asserted a *clamped* geometry.** The frame's width was `max(40, terminal_width)` — clamped **up** — so in a terminal narrower than forty columns every frame line wrapped: the frame occupied twice the rows it claimed, and the erase — which moves up by the frame's *line count*, not by the rows it occupies — left the mangled frame standing above the operator's prompt. The surface was **not cleaned up**. The frame's height was bounded in *lines* against the terminal's height, but the frame *occupies* one row more than its line count, because every line ends with a newline and the last one moves the cursor onto the row below; a frame whose lines equalled the terminal's height therefore **scrolled** the terminal, pushing its own title line — the arithmetic `N selected of M, X reclaimable` — off the top in any terminal shorter than ten rows, and pushing the operator's scrollback up with it. The test `the_frame_is_bounded_and_never_overflows_the_width` asserted that the frame fitted `max(w, MIN_WIDTH)`: it was green *because* it blessed the clamp. | **Fixed in C043.** The frame adopts the terminal's geometry exactly, and the layout degrades so that it fits: the reclaimable column goes first, then the ownership column's wording, and the name keeps what is left. The frame's *rows* are bounded by the terminal's height, and the chrome gives way — the key-hint bar, then the column header, then the all-toggle row — so the title, the status line and a candidate row survive in a six-row terminal. A terminal too small for the surface to be useful is refused with an actionable message, before the listing spawns anything. The two tests that pinned the defect were rewritten **first**, and all eight mutations of the fix are caught. |
| 66 | **The interactive surface was painted with a bare newline, which in raw mode is a linefeed *without* a carriage return.** `crossterm 0.28`'s `enable_raw_mode` calls `cfmakeraw`, which clears `OPOST`, so every frame line started at the column the previous one ended in. The frame occupied **twice** the rows it claimed, every line lost its first character to the row above, and the rewind — which moved up by the frame's *line count* rather than by the rows it actually occupied — erased most of the frame it was meant to keep. On a 30-row terminal with four candidates, the operator saw one candidate, a broken key-hint bar, and the hint bar's leading bracket gone. The defect was invisible to the suite because the `Terminal` seam records *lines*, not bytes: a fake cannot see a byte-level defect, and a frame is bytes. | **Fixed in C042.** The byte stream is now a pure function (`frame_bytes`, `rewind_bytes`, `clear_bytes`), every line begins with an explicit carriage return, and the rewind moves to column zero before it moves up. The tests replay the stream through an in-test terminal emulator that models xterm's pending-wrap rule, so the rendered geometry is asserted for the first time. Verified live over a raw pty: the frame now lands on the nine rows it claims, each at column zero, with every character intact. |
| 68 | **The `clean` alias's compatibility guarantee is guarded in one direction only.** The only guard on the alias's surface, `compatibility_aliases_are_hidden_but_accepted` (`src/cli.rs:2422-2464`), enumerates the alias's own ids — twelve hidden and two visible — and asserts that the alias *carries* them. It asserts nothing about the **converse**: that the alias accepts every cleanup flag the root declares. The converse does not even hold structurally today: `interactive` is declared only on the root (`src/cli.rs:162-163`) and reaches the alias solely because the flag is `global = true`, so the guard's list is already a hand-maintained approximation rather than a derivation. M019's design makes the root the canonical home for every future policy override; an override added to the root and not mirrored onto the alias yields `cargo cleanme clean --new-override` → *"unexpected argument"* → exit `2`, and a spelling published since v0.2.0 stops working with the whole suite green. M019's acceptance criterion *"No published spelling is removed; `clean` still accepts everything it accepts today"* is satisfied today but discharged by reading a diff — nothing enumerates "everything it accepts today", so the guarantee can only be broken quietly. |  **Fixed in C045.** The converse guard now exists: `the_alias_accepts_every_cleanup_flag_the_root_declares` (`src/cli.rs:1319`) reads the clap model, asserts that every cleanup flag the root declares is carried by the alias, and *parses* each of them through the alias — so a flag that exists on one level and not the other fails the suite. It is discriminating, not vacuous: adding a root-only cleanup flag was verified to fail it, with the flag's name in the message. The hand-maintained id list is gone; the guard derives the list from the model.
| 69 | **The cleanup surface is declared twice, and the scope rule is implemented twice.** The root declared twelve cleanup flags and the alias declared the same surface again. The scope rule was derived twice as well: `root_cleanup_request` and `merged_cleanup_request` each matched on three arms from a different view, joined by a bespoke merge layer (`merge_overrides`, `merge_one`, `merge_many`, `merged_clean_mode`). One new flag therefore costs **four** edits — the root declaration, the alias declaration, the merge layer, and the guard's hand-list — and **item 68** is the consequence. The duplication is invisible at runtime: both spellings resolve to the same `Invocation`, so no behavioural test can observe it. |  **Fixed in C045.** The cleanup surface is declared once — `CleanupFlags` (`src/cli.rs:35-83`), flattened into both the root (`src/cli.rs:167-168`) and the alias (`src/cli.rs:286-288`) — and the scope rule is implemented once: `cleanup_request` (`src/cli.rs:540-629`) folds the two levels into one `CleanupRequest`, and the bare invocation is that same derivation with an empty alias. One new flag now costs one edit. The merge layer (`merge_overrides` `src/cli.rs:803-827`, `merge_one` `:830-838`, `merge_many` `:840-847`, `merged_clean_mode` `:706-716`) still exists — it merges *two levels*, and no longer declares a second copy of the surface. |
| 70 | **The MSRV lane is red at `main`: a test asserts a rendering the qualified-host case never produces.** `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` asserts that a profile-scoped preview's report contains `output-union before`. That string is the **row** renderer's label (`src/cleanup.rs:1020`); the **summary** renderer takes a different branch when a selector is present in a non-Execute mode (`src/cleanup.rs:906-916`) and prints *"the selector's reclaimable size is not measured (output union …)"* instead. The test's premise is derived from the host's own Cargo (`runtime_clean_capabilities`, `src/cleanup.rs:4386`), and the qualification policy qualifies Cargo 1.89 — so on the MSRV lane the selector **is** qualified, the preview runs, and the summary branch is the one that renders. The assertion therefore expects a string the qualified case does not produce. Reproduced at `main` with C045's changes stashed, so the corrective did not cause it: `cargo +1.89 test --locked --lib cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` fails, the same command on the current stable toolchain passes, and running the 1.89 compiler with a **stable** `cargo` on `PATH` passes — which is the decisive isolation of the host Cargo's version as the variable. The CI MSRV lane (`cargo +1.89 test --locked --all-targets`) therefore fails at `main`, independently of this corrective. | **Fixed by C046** (`plans/implementation/artifact-discovery-cleanup/c046-msrv-lane-test-assertion-corrective.md`). The assertion is derived from the premise that *is* consulted; it is the rendering it expects that was wrong. The test now asserts the **quiet** surface's summary wording — *"the selector's reclaimable size is not measured (output union …"* — and a second test pins the **verbose** row label `output-union before`, so both renderings are pinned and neither is asserted by accident. The production rendering is unchanged. `cargo +1.89 test --locked --all-targets` now exits 0. |
| 71 | **The published per-platform test census was wrong on Windows by 16, and the correction that fixed it was never re-derived.** Item 57 corrected the census to **563 declared / 561 Linux / 500 Windows / 558 macOS** and recorded the method. The Windows figure was still wrong: the derivation counted `#[cfg(unix)]` *gates* rather than the *tests* those gates exclude, and the premise it rested on — "a gated block holds further tests" — is false, because every gated block in test scope sits **inside** a test function, where removing it removes no test. The true figure was **516**. The error then survived M018, M019, C042, C043 and C045 unchanged, and was published in four documents. | **Fixed.** The census is re-derived from the source by the method `AGENTS.md` §6 prescribes, and the Windows figure is now **build-verified** rather than asserted: the test binary cross-compiled for `x86_64-pc-windows-gnu` carries exactly the derived set of tests and no others, and the Linux figure reproduces `cargo test --lib -- --list` name for name. The current census is **585 declared / 583 Linux / 536 Windows / 580 macOS** (the Windows number is 536 after this pass's own new tests). **Re-derived 2026-10-23** (the 2026-10-23 defect sweep): **594 declared / 592 Linux / 542 Windows / 588 macOS** — the Windows figure was re-verified by cross-compiling the test binary for `x86_64-pc-windows-gnu` and confirming that all 542 derived test names are in the binary and none of the 52 excluded ones are. All four documents state it, and the derivation — including the cross-compile check — is recorded so the next reader can re-derive rather than copy. **Re-derived 2026-10-24**: **602 declared / 600 Linux / 547 Windows / 596 macOS** — 50 `#[cfg(unix)]` and 5 Linux-only gates. The Windows figure was re-verified by cross-compiling the three test binaries for `x86_64-pc-windows-gnu` and confirming that all 547 derived test names are in the binaries and none of the 55 excluded ones are; the Linux figure reproduces `cargo test --lib -- --list` name for name. |
| 72 | **The cleanup engine's authorization verdict was a prose substring match.** The engine decided whether a unit was blocked by searching a human-readable message for the word "authorization". A directory whose *name* contained that word — `cleanup authorization root` — turned the match on, and the run refused with exit `1` instead of completing with exit `0`. The decision that governs deletion was taken against English text, and the text was written for humans. | **Fixed.** The verdict is a typed `BlockReason` (`src/cleanup.rs`), and `ProofFailure` carries an `authorization_infrastructure` flag rather than being inferred from its detail string. The message is now a *rendering* of the verdict, not its source. |
| 73 | **The interactive preconditions were checked after the scope was resolved, so a refused `-i` still spawned Cargo.** A `-i` run in `--format json`, or with no terminal, was refused — but only after the scope resolution had already run `cargo metadata` across every resolved root. The refusal existed to avoid that work. | **Fixed.** `check_interactive_preconditions` (`src/main.rs:377-428`) is called at the top of `run_cleanup`, **before** the scope is resolved, and `run_cleanup` now takes a `&dyn CargoRunner` so a `CountingRunner` can prove the refusal path spawns nothing. |
| 74 | **The H-2 re-discovery comment described a mechanism the code does not use.** The comment claimed the post-proof refresh "only re-resolves *self*", implying a per-candidate re-resolution. The re-discovery is in fact a **whole-scope** manifest walk over **every** root, run **once per unit**, because the claimant set is global: a manifest appearing under any root after one unit's proof must block the units that follow. | **Comment fixed; the shape is unchanged.** The comment now states what the walk does and why the scope is global. The *shape* is load-bearing and is pinned by a regression test — a manifest planted during one unit's cleanup must block the next — so it is **not** changed. A cheaper form would either trust a timestamp (the direction this crate forbids) or narrow the check (a change of safety semantics). The finding's own measurement of "24 `cargo locate-project` spawns" for the re-discovery is **wrong**: the bounded discovery path spawns no Cargo at all. |
| 75 | **The locate phase costs one `cargo locate-project` per manifest, not per workspace.** A fifty-member workspace costs fifty locate spawns plus one metadata call. | **Not a defect — a documented decision.** `architecture/07-workspace.md` §4 records this as the price of the parallel, order-independent locate phase: skipping locates serializes the phase on member order, and there is no order-independent way to know two manifests share a root without asking Cargo. **Ancestry inference is forbidden**: re-implementing Cargo's workspace resolution by walking up directories is exactly the thing the crate does not do. The published invariant — *exactly one locate per discovered manifest* — and the `cargo_locate_calls` counter are part of the machine contract. |
| 76 | **The progress surface lists at most five groups.** | **Not a defect — a documented decision.** `architecture/11-progress.md` records the top-5 list as an observation-only mechanism: it is a *progress* surface, not a report, and a progress line that grew with the candidate count would itself be a defect. The finding itself calls it "a candidate for a cost/benefit call rather than a defect". |
| 77 | **Four root-collapse helpers duplicated the mechanism and disagreed about failure.** `main.rs`, `cleanup.rs`, `policy.rs` and `workspace.rs` each canonicalised, deduplicated and dropped nested roots, with three different answers to "what if this root cannot be resolved": one swallowed the failure, one failed the whole run, one compared the unresolved root *lexically* against canonicalised siblings (unsound across a symlinked prefix), and one did not canonicalise at all. | **Fixed.** The mechanism is one implementation — `traverse::collapse_outermost` — and each root resolves into an *identity* (its physical location, resolved as far as the filesystem allows) and a *spelling* (canonical when it resolves, the caller's own spelling when it does not). The comparison is by identity, so a symlinked prefix can no longer make one physical root look like two. The **policy** stays where it belongs: the cleanup surface still fails closed, and the scan and display surfaces still keep the spelling. |
| 78 | **Two clauses in the resolution path could never change the verdict.** The recency test carried a `|| t > clock_start` disjunct that the `t >= clock_cutoff` arm already subsumes, because the cutoff is `clock_start - recency` with an *unsigned* recency; and the coverage bookkeeping re-inserted into a set whose contents were already implied by the map it read. | **Fixed.** The subsumed disjunct is gone, with a comment stating the relation that made it redundant. The redundant insert is gone, and a new test — `a_repeated_manifest_stays_covered_by_the_workspace_that_claimed_it` — pins the invariant that makes it redundant; it was **verified to fail** when the invariant is broken, so it is a guard and not decoration. **Amended 2026-10-24**: the same disjunct survived in **three** other places — the two `is_recent` closures at `src/traverse.rs:587` and `src/traverse.rs:696`, and the proof-time recency test at `src/cleanup.rs:4102` — where the `t > start` arm could never change the verdict for the same reason. All three are gone; the two closures now read `time >= cutoff` outright, and the `cleanup.rs` site carries the comment that states the relation. |
| 79 | **The shared symlink predicate trimmed a trailing backslash on Unix, where a backslash is an ordinary filename character.** `path_is_symlink` (`src/traverse.rs`) stripped `\` with `/` as separators, so a real directory named `link\` was re-spelled as `link` and the predicate answered about a *different* path. Every caller treats a true answer as "this is a symlink", so a real directory was refused as one — in the safety path. | **Fixed.** The trimming is done with `Path`'s own component handling, which knows the platform's separators, so a Unix path that legitimately ends in `\` is left alone. A regression test (`a_unix_directory_named_with_a_trailing_backslash_is_not_a_symlink`) pins it. |
| 80 | **The same predicate's spelling arithmetic went through a lossy `String`, so a name that is not valid UTF-8 became a path that does not exist.** `to_string_lossy` replaced the invalid bytes with U+FFFD; the trimmed probe then stat'ed a path that was not there and reported "not a symlink" — about exactly the spelling the guard exists to catch. | **Fixed.** The trimming never decodes the bytes: `Path::file_name`/`parent` carry the `OsStr` through unchanged. A regression test (`a_non_utf8_link_spelled_with_a_trailing_separator_is_still_a_symlink`) pins it. |
| 81 | **`workspace.rs` carried a second copy of the symlink predicate**, byte-identical to the shared one and with the same two defects. | **Fixed.** The copy is deleted; `workspace.rs` calls `traverse::path_is_symlink`. One definition, one set of defects to fix. |
| 82 | **`policy.rs` re-implemented the root collapse while its doc comment claimed it called the shared one.** `canonical_dedup_roots_protecting` carried its own sort/dedup/nest loop — the mechanism that `traverse::collapse_outermost` exists to hold in one place — and the doc named the shared function it did not call. | **Fixed.** The collapse is now `traverse::collapse_outermost_protecting`: the shared mechanism, with the protection — the only part specific to this surface — as a parameter. The doc comment names the function it actually calls. |
| 83 | **`ResolvedRoot::error` was written and never read.** The field carries the reason a root did not resolve; the cleanup surface made its own `fs::canonicalize` call to learn the same thing, and nobody ever read the field the resolver had already filled in. | **Fixed.** The cleanup surface reads `ResolvedRoot::error` and phrases the refusal from it, so the field has one reader and the filesystem is asked once (see **92**). |
| 84 | **`merge_walk_counters` merged two of the seven counters.** `directories_visited` and `directories_pruned` were merged; the five prune *reasons* (`platform_system_prunes`, `cargo_home_prunes`, `rustup_home_prunes`, `target_vcs_prunes`, `user_ignore_prunes`) were dropped, so the report said how much was skipped but not why. | **Fixed.** All seven counters merge. `manifests_found` is still deliberately not merged: it counts the manifests the run knows about, and a re-walk rediscovers the same ones. |
| 85 | **The source-tree-set optimisation landed in one call site only.** `workspace.rs` computed the conventional source directories once per scan; `cleanup.rs` rebuilt the set for every covering root of every unit, in the final ownership proof. | **Fixed.** The set is computed once per unit from the fresh universe and passed in, the way the scan path already did it. `inside_source_tree` — the form that rebuilt the set on every call — is gone; the two functions it wrapped are now the API. |
| 86 | **The environment lock covered the writers and not the readers.** `progress::test_env::EnvRestore` serialised the *writers*; every `std::env::var` reader — `CARGO_HOME`, `VISUAL`, `EDITOR`, `TERM` — was outside the lock, and `setenv`/`getenv` are not safe against each other. `std::env::set_var` is `unsafe` for exactly this reason. | **Fixed.** The three environment-mutating tests now mutate in a **child process** — the repository's own `M017_ENV_PROBE_ARMED` pattern — so the parent's environment is never rewritten and no reader can race. The guard's lock is gone with it. |
| 87 | **`escape_path_text`'s doc claimed every human surface routes path text through it, and two did not.** The binary's stderr surfaces and `policy.rs`'s example rendering each had their own filter, and the binary's was a *different* filter: it dropped control characters rather than escaping them, so the two surfaces rendered the same path differently. | **Fixed.** The escaper is public, and every human surface — the report, the selection TUI, the binary's stderr, and the policy diagnostics — routes through it. The bound is applied to the *escaped* text, so a path of control characters cannot push the visible text past it by rendering shorter than it is. |
| 88 | **`is_filesystem_root("")` was true.** The predicate asked only whether the path had a parent, and the empty path has none, so an empty path was refused as a filesystem root. | **Fixed.** The empty path is not a root. The refusal still names a real filesystem root. |
| 89 | **`duplicate_dry_run` said "the two values are ambiguous" about a boolean flag.** `--dry-run` carries no value: both levels mean the same thing, and the refusal is about the invocation, not the value. The message pointed the operator at a conflict that does not exist. | **Fixed.** The message names the conflict that does exist: the flag was given at two levels, and belongs at one. The value-carrying `duplicate_override` keeps its wording, because there the ambiguity is real. |
| 90 | **The two version predicates disagreed about `+1.0.0`.** `is_release_version` refused it; `compare_release_versions` parsed it, because `"+1"` parses as the number one — so a build-metadata version ordered *above* `0.9.0` and the updater could believe a local build was newer. | **Fixed.** `compare_release_versions` uses `is_release_version`, so the two cannot disagree. A test asserts the agreement over a table of versions, and the build-suffix case is in the existing bad-version list. |
| 91 | **`scripts/check-selector-qualification.py` was in neither the verification ladder nor §3.1's "gates the ladder does not cover".** The gate decides whether `--profile` and `--package` are usable at all, and it is enforced in CI and in `scripts/release-check.sh` — but a change to the policy file, the CI allowlist, or `clean_capabilities_from_version` passed the documented ladder and failed CI. | **Fixed.** The gate is a step of the ladder in `AGENTS.md` §3, with a note saying what it keeps in agreement. |
| 92 | **`collapse_roots` canonicalised every root and then the shared collapse canonicalised it again.** The cleanup surface resolved each root to check that it still resolved, then handed the *paths* to a collapse that resolved them again — two `fs::canonicalize` calls per root, and a window in which the two answers could differ. | **Fixed.** The roots are resolved once, the refusal is phrased from that resolution, and the collapse takes the already-resolved roots. |
| 93 | **The shared symlink predicate had no unit test in the module that owns it.** The only assertion in the repository was in `policy.rs`, and it covered one of the three spellings the predicate exists to handle. Both defects (**79**, **80**) were in the untested branch. | **Fixed.** `src/traverse.rs` carries the full matrix: `link`, `link/`, `link/.`, a Unix `link\`, a plain directory, a non-UTF-8 link spelled `link/`, and a path that does not exist. |
| 94 | **Nothing pinned `policy.rs`'s collapse to the shared one**, so **82** and **85** were invisible to the suite: the two implementations were in different modules and would have agreed until one changed. | **Fixed.** A test asserts that the policy collapse *is* the shared collapse, and the protection is covered by its own case. |
| 95 | **The CLI-contract harness's own `ETXTBSY` mitigation had the opposite of the claimed effect.** `stage_runnable_binary` reopened the staged binary for **writing** immediately before executing it, to flush it. `execve` consults the inode's write count, and `close()` does not synchronously clear it — an `O_WRONLY` open-and-close placed just before an `execve` is the canonical way to hit the failure. Measured: **479 failures in 960** staging-and-exec cycles under load, against 47 for a plain write and 41 for `fs::copy`. The full `--all-targets` command failed **twice in 8** invocations, each time failing two CLI-contract cases with a message that pointed at the product. | **Fixed.** The flush is a read-only reopen on Unix — `fsync()` on an `O_RDONLY` descriptor is accepted by Linux, macOS and the BSDs — and **0 in 960** failures, same machine, same load. The write-mode reopen stays on Windows, where `FlushFileBuffers` requires write access and there is no `ETXTBSY` to cause. |
| 96 | **The recorded `ETXTBSY` re-measurement was contradicted, and the tracked scope named only the product's update path.** Finding **61** recorded "the test alone passed 40 of 40 consecutive runs, and an 8-way parallel full-suite burst passed 8 of 8" — both taken on an *unloaded* host, while the defect is contention-dependent by construction. Under contention the same tree failed a full suite in 1 of 6 runs. And C041's scope was the product's update path, so the repository's own, larger window (**95**) was untracked. | **Fixed.** Finding **61** is amended: the figure is a *rate under load*, not a count of consecutive passes, and the "8 of 8" was a miss. C041 is amended in the same pass — the measurement method, and the widening of the scope to the repository's own staging helpers. C041 remains **open**; the product's window is unfixed. |
| 97 | **`cargo cleanme ROOT` lost the operator's root.** `normalize_cargo_argv` removed the Cargo-injected token by *spelling* — the literal `cleanme` at argv[1]. The ROOT positional is spelled exactly like that token, so a direct `cargo-cleanme cleanme` had its own root removed and became a machine-wide Routine cleanup: the scope label changed from `explicit` to `routine`, and the operator's project trees joined the selected roots. The two spellings are indistinguishable from the argv alone. | **Fixed.** The token is identified by launch *context* instead: Cargo exports `CARGO` to the external subcommand it runs, and a direct invocation does not (`src/cli.rs:916-918`), and the removal is gated on it (`src/cli.rs:905`). The context is a parameter (`normalize_cargo_argv_with`, `src/cli.rs:900-909`) so the unit test is deterministic without mutating the process environment. Three cases pin it: `a_direct_cleanme_root_is_never_stripped` and `a_direct_cleanme_root_resolves_to_an_explicit_scope` (`src/cli.rs`), and `a_direct_cleanme_root_stays_a_bounded_scope` (`tests/cli_contract.rs:377`) — the last one `#[cfg(unix)]`, because the Cargo stub it drives is a POSIX `/bin/sh` script and a lane that could not run it would prove nothing. |
| 98 | **The command line's `--include` and `--exclude` were never canonicalised.** `config::load` canonicalises the policy patterns it reads from `config.toml`, but the CLI's own patterns went straight into the policy, so a pattern spelled through a symlink matched nothing — the operator asked to exclude a tree and the tree was not excluded. | **Fixed.** `config::canonical_policy_patterns` (`src/config.rs:349-354`) is now the single canonicaliser, and the CLI overrides route through it (`src/main.rs:460`). Two cases pin it: `a_command_line_policy_pattern_is_canonicalized_like_a_configured_one` (`src/config.rs`) and `a_command_line_exclude_spelled_through_a_symlink_still_excludes` (`tests/cli_contract.rs`). |
| 99 | **Two human surfaces wrote path text raw.** The scan's non-Cargo diagnostics and the binary's own `config path`, `config edit` and combined-roots messages put a path on the terminal without escaping it, so a directory name carrying a control byte could rewrite the line the operator was reading. The escaper existed; these surfaces did not use it. | **Fixed.** Every one of them now routes through the shared escaper (`src/main.rs:1275-1277`, which delegates to `report::escape_path_text`): the scan's diagnostic paths (`src/main.rs:1923`), the blocked-unit paths (`src/main.rs:1326`), the unproven-root paths (`src/main.rs:493`), the combined roots (`src/main.rs:1406`), and the `config path` / `config edit` messages (`src/main.rs:264`, `src/main.rs:300-324`). Two cases pin it, both `#[cfg(target_os = "linux")]`: `a_control_character_in_a_directory_name_never_reaches_the_scan_diagnostics_raw` and `a_control_character_in_a_directory_name_never_reaches_the_cleanup_preamble_raw` (`tests/cli_contract.rs`). |
| 100 | **`before_bytes` contradicted the row's own `detail`.** The fresh-size skip published a byte count that the row's `detail` sentence did not quote, so the machine field and the human field described two different sizes for the same unit. | **Fixed.** The size the decision used is published (`proof.pre_bytes`, `src/cleanup.rs:2254-2268`), and the detail wording now says "current size" rather than "fresh size" (`src/cleanup.rs:2258`). One case pins it: `a_size_skip_publishes_the_size_its_own_detail_quotes` (`src/cleanup.rs`). |
| 101 | **A stale `VISUAL` hid a usable `$EDITOR`.** `resolve_program` accepted any *existing* path, so a `VISUAL` pointing at a file that is not executable shadowed a perfectly good `$EDITOR` and the operator was told the editor could not be found. | **Fixed.** An explicit path must be *runnable*, not merely present (`src/editor.rs:39-50`, with the predicate at `src/editor.rs:87`). One case pins it: `an_unexecutable_visual_falls_through_to_the_next_candidate` (`src/editor.rs`). |
| 102 | **The learned-root retention window was truncated by an `as u16` cast.** A retention above 65 535 days became a small number, so a learned root was forgotten long before the operator's retention window said it should be. | **Fixed.** The window is carried as a `u32` end to end (`src/discovery_state.rs:243`, `src/policy.rs`), so the truncation is inexpressible rather than merely guarded. One case pins it: `a_retention_window_larger_than_a_u16_is_not_truncated` (`src/policy.rs`). |
| 103 | **A case failed on a single-CPU host.** The cleanup engine's worker pool is sized by `available_parallelism`, so on a one-CPU machine the pool has one worker and the case's premise — that the fan-out is observable — does not hold. The suite was green on the development host and red on a constrained runner. | **Fixed.** The case now guards its own premise with `available_parallelism() > 1` (`src/cleanup.rs:11374`), so a single-CPU host skips it rather than failing it. Verified on a pinned single-CPU host: the library suite passes at 588/588 with `taskset -c 0`. |
| 104 | **`SelectorInvalid` was unreachable by any test.** The branch that reports an unknown package specifier as invalid rather than unsupported existed, but every case that reached it asserted the *unsupported* code, so the branch was never distinguished from its neighbour. | **Fixed.** The branch is now exercised: `an_unknown_package_spec_on_a_qualified_cargo_is_selector_invalid` (`src/cleanup.rs:11709`) names the Cargo premise the branch depends on — a version the selector qualification recognises — and the older case was tightened to assert `SelectorUnsupported` exactly, so the two codes can no longer be confused. |
| 105 | **The corpus's citations had drifted.** Deep dives cited line numbers that no longer said what the sentence claimed, and a re-derivation pass that mapped citations by *content* could not see the drift: a citation that had drifted onto a generic line matched a generic line again and reported itself clean. | **Fixed.** Every function-named citation is now anchored to the function it names — the `fn` line, or the `#[test]` attribute where the table's convention says so — and each table's convention is voted from that table's own still-correct rows. The content-based mapping is kept only where a citation names no function and the span is shifted, and then only when the cited content is byte-identical at the shifted range. The mechanical half is gated by `scripts/check-doc-citations.py`; the semantic half remains a reviewer's job, as `AGENTS.md` §6 already says. |
| 106 | **Fifteen citations in this register pointed at the wrong lines.** The entries named a file and a line, but the line did not carry the claim — including two that pointed at the *same* line twice for two different error paths, and one that cited a function's own definition as the place its behaviour lives. | **Fixed.** Each is re-derived against the source and each was checked to land on the content the sentence claims. The register's own rows are not exempt from the rule they state: the citations in items 62, 63 and 64 now resolve to the code they describe. |
| 107 | **The published module sizes and test census were stale.** `architecture/overview.md` §3 and §3.1 publish the module sizes, and `architecture/14-testing-and-verification.md` publishes the per-file distribution and the per-platform census. The figures in them described a much earlier tree: `cleanup.rs` was published as 11,163 lines with 126 tests when it carried 12,174 with 138, and the census was published as 531 declared when it was 602. | **Fixed.** The figures are recomputed from `src/` and the three measures the corpus uses are now named apart: `overview.md` §3.1's `prod` is everything outside *every* `#[cfg(test)]` scope, while the per-file table's `mod tests opens at` is the `mod … {` line the attribute gates. The census is **602 declared / 600 Linux / 547 Windows / 596 macOS**, and the Windows figure is build-verified rather than asserted: the three test binaries cross-compiled for `x86_64-pc-windows-gnu` carry exactly the 547 derived test names and none of the 55 excluded ones. |
| 108 | **`plans/output-schema-v1.md` described the path escaping wrongly.** It said that an undecodable byte was dropped from the path, so a reader would expect the machine document to carry a shorter spelling than the filesystem has. | **Fixed.** The schema now states what actually happens: each undecodable byte becomes the Unicode replacement character `U+FFFD`, so the spelling is lossless in length and a reader can tell it apart from a path that was genuinely shorter. |
### 10.1 Dead public API, recorded not removed

`cargo_cleanme` is published on crates.io, so dropping `pub` is a semver-visible
change. These items have no production callers and are kept deliberately:

| Item | State |
|---|---|
| `cleanup::execute_pre_spawn_decision` (~200 lines) | Zero callers, including tests. The real pre-spawn authority is the private `cleanup::final_cleanup_proof_roots`, which refreshes more strictly. |
| `cleanup::final_cleanup_proof` | Reached only by the dead function above and one test. Correct in its own right — it builds its own universe. |
| `cli::absolutize_root_for_test` | Zero call sites anywhere, despite the name promising otherwise. |
| `cli::Cli::scan_root` | Test-only; `main.rs` destructures `Command::Scan` directly. |
| `traverse::source_activity` | Test-only; production uses `workspace_member_activity`. |
| `progress::set_determinate_total` | Zero call sites anywhere. It stores the total without zeroing `determinate_done`, unlike the private `apply_total` — the one path that can leave the displayed total and the done-count mismatched. M020 removed its `set_length` call, so the inconsistency is now state only. |

### 10.2 Corrections verification made to the original findings

Recorded because the original wording was wrong and would mislead a future
reviewer:

- **Item 1 was misdescribed.** The claim that `tests/end_to_end.rs:202` "encodes
  the bug as expected" is false: that test calls `output::scan(&report, "routine")`
  with a hand-supplied label to exercise the DTO projection. It never touches
  `main.rs`'s derivation, so the label logic was simply **untested** — which is
  why it was wrong, and why the fix adds a test rather than correcting one.
- **Item 2's mechanism was unreachable.** The reported cause — a shared hoisted
  universe failing via `hoisted.get()?` — cannot occur: the universe is
  initialised immediately before the call that consumes it. The real path is
  the per-candidate skip, where a `ProofFailure` becomes a `skipped` row and
  the unit is abandoned.
- **Item 3 is a documented decision, not an oversight**, and the doc comment
  states the reasoning explicitly.

---

## 11. How the code got this way

The shape above is not accidental. `plans/` holds the decision record — the
canonical spec, the terminology, the roadmap, the planning process, the
accepted ADRs, the implementation plans, and a matching closure record for
each. As of this pass `plans/` contains:

| Artifact | Count | Notes |
|---|---:|---|
| ADRs | 8 | 001, 002, 003, 004 (with an amendment), 006, and 007 are accepted; **005 and 008 are proposed, not accepted** |
| Numbered milestone plans | 29 | `001`…`013` across the two subsystems |
| `M`-prefixed milestone plans | 4 | M014–M017 |
| Corrective plans | 35 | C001–C034 plus C036. **C035 is absent on purpose** — it is superseded and must not be merged; it stays as the record of the rejected mechanism. |
| Closure records | 68 | one `*-status.md` per plan, in `plans/closure/` |

The load-bearing decisions are:

- **ADR 001 — workspace output ownership and cleanup authorization.** Defines the
  ownership classification that invariant 1 is built on.
- **ADR 002 — adaptive Routine/Full discovery.** Defines the two scan modes, the
  learned-root state file, and the uncertainty rule in invariant 4.
- **ADR 003 — canonical maintenance invocation and unattended output.** Defines the
  current front door: bare `cargo cleanme` is Routine *Execute*, `--dry-run`
  is the zero-spawn simulation preview, and the updater's JSON output is the
  unattended surface. Read it before changing anything in §7's mode table, or you
  will be reasoning from the pre-0.2.0 CLI.
- **ADR 004 — per-island activation**, accepted with an amendment.
- **ADR 006 — working-directory-anchored selection.** Defines the CWD-anchored
  default and the `-i` selection surface.
- **ADR 007 — scoped proof is the default.** The proof boundary is the selected
  scope (anchor / explicit roots / `--full`); out-of-boundary unprovables are
  reported, never blocking. It deliberately *reserves* whole-system fail-closed
  for `clean --full`.

The project's own summary of its recurring lesson (from `plans/registry.md`) is
worth carrying into any review: *a green test proves only that its own premises
hold.* Several milestones were closed by a test that was passing for the wrong
reason — a POSIX-only fixture passing on no platform, a transport that could not
reach its version authority, a Windows case that had never executed, and a
smoke validator that was green on all three hosted lanes while its subject was
failing. `scripts/` now contains explicit *premise guards* to catch that class
of false green, and [Testing & verification](14-testing-and-verification.md)
assesses which classes are now guarded and which remain open.

---

## 12. Index of deep dives

| # | Component | Covers |
|---|---|---|
| 01 | [Domain & errors](01-domain-and-errors.md) | `domain.rs`, `error.rs`, `lib.rs` — the shared vocabulary |
| 02 | [CLI](02-cli.md) | `cli.rs` — parsing, argv normalization, root absolutization |
| 03 | [Config & editor](03-config-and-editor.md) | `config.rs`, `config.toml`, `editor.rs` |
| 04 | [Policy & scope](04-policy-and-scope.md) | `policy.rs` — the Routine/Full decision |
| 05 | [Discovery](05-discovery.md) | `discovery.rs` — manifest walk, pruning, attribution |
| 06 | [Discovery state](06-discovery-state.md) | `discovery_state.rs` — learned roots, uncertainty, publish |
| 07 | [Workspace](07-workspace.md) | `workspace.rs` — metadata, capabilities, grouping, units |
| 08 | [Traverse](08-traverse.md) | `traverse.rs` — parallel measurement, activity |
| 09 | [Cleanup](09-cleanup.md) | `cleanup.rs` — authorization, proof, modes |
| 10 | [Reporting](10-reporting.md) | `report.rs`, `output.rs` — human and machine output |
| 11 | [Progress](11-progress.md) | `progress.rs` — observer trait, renderer, gating |
| 12 | [Self-update](12-self-update.md) | `update.rs` — provenance, version authority, replace |
| 13 | [Orchestration](13-orchestration.md) | `main.rs` — dispatch, both pipelines, exit codes |
| 14 | [Testing & verification](14-testing-and-verification.md) | `tests/`, `scripts/`, CI, the false-green problem |
| 15 | [Distribution & release](15-distribution-and-release.md) | installers, generated docs, packaging, release gates |
| 16 | [Interactive](16-interactive.md) | `interactive.rs` — NOR/INS selection state machine, terminal seam, raw-mode lifecycle |

---

## 13. Reading order

For a first pass over the system:

1. This file.
2. [Domain & errors](01-domain-and-errors.md) — the vocabulary everything else speaks.
3. [Orchestration](13-orchestration.md) — the two pipelines end to end.
4. [Cleanup](09-cleanup.md) — where the safety invariants actually live.
5. [Workspace](07-workspace.md) and [Discovery](05-discovery.md) — how candidates are found and grouped.
6. §10 above, then the deep dive for whichever area you are changing.
7. Everything else as needed.

For a review that is specifically about *the CLI surface*, read
[CLI](02-cli.md) → [Policy & scope](04-policy-and-scope.md) →
[Orchestration](13-orchestration.md) → [Reporting](10-reporting.md) in that
order; the spelling decisions, the scope decision, the dispatch, and the
machine contract are the four places a CLI change can break a promise.
