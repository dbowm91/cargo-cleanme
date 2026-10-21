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
  modules, 18 source files, 35,190 lines in `src/` (16,113 of them outside
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
| **Explicit cleanup** | `cargo cleanme clean ROOT` | One bounded root. Bypasses discovery filters. | Yes — via `cargo clean` |
| **Full cleanup** | `cargo cleanme clean --full` | Runs a complete Full reconciliation first, then cleans the learned roots it retained. | Yes — via `cargo clean` |
| **Read-only scan** | `cargo cleanme scan --all`<br>`cargo cleanme scan ROOT`<br>`cargo cleanme scan --known` | The machine reconciliation, a bounded explicit scan, or the routine inventory. **A bare `cargo cleanme scan` is a usage error** (exit 2) — the `scan-scope` ArgGroup requires exactly one selector. | No |
| **Config** | `cargo cleanme config path\|show\|edit` | Resolve, print, or open the configuration file. `config --format json` is a **hard rejection** (exit 2); `--format log` is honoured. | No (except `edit`, which edits the file) |
| **Self-update** | `cargo cleanme update` | Resolve and apply the latest published release. | Yes — replaces the binary |

Root-level flags: `--config PATH`, `--no-progress`, `--stats`, and
`--format human|json|log` are **global**; `--dry-run` (hidden alias
`--dryrun`) and `-i`/`--interactive` are root-level but **not** global, so
`clean` and `update` each declare their own `--dry-run`.

`clean`-only flags: `--known`, `--full`, `--min-reclaimable-bytes N`,
`--older-than SECONDS` (the config key is `cleanup.policy.min_inactive_seconds`),
`--include GLOB` (repeatable), `--exclude GLOB` (repeatable), `--profile NAME`,
`--package NAME`, `--dry-run`, `--cargo-preview` (hidden alias `--yes`).

`--include` and `--exclude` are matched against the **workspace root**, not
the physical output root — a pattern like `**/target` matches nothing.

Exactly two short forms exist in the whole CLI: `-i` (global) and `-a`
(`scan`-only), plus clap's own `-h` and `-V`.

Two spellings are deliberately **hidden** compatibility aliases rather than
removals: `--dryrun` (for `--dry-run`) and `--yes` (for the now-default
Execute mode). `scan --full` is a hidden alias of `scan --all`.

> **There is no `cargo cleanme --full`.** `--full` exists only on `scan` (as a
> hidden alias of `--all`) and on `clean`. A proposed ADR
> ([ADR 008](../plans/adr/008-accepted-canonical-full-cleanup-spelling-and-proof-scoped-full-gate.md),
> status **proposed — not accepted, no implementation authority**) proposes
> making `cargo cleanme --full` the canonical spelling. Nothing in this file
> describes that as shipped.

---

## 3. The crate at a glance

| Property | Value | Where it comes from |
|---|---|---|
| Source files | 18 | `src/` |
| Library modules | 16 | `src/lib.rs` |
| Total lines in `src/` | 35,190 | `wc -l src/*.rs` |
| Production lines | 16,113 | total minus every `#[cfg(test)]` scope |
| Inline `#[test]` functions | 493 | `#[test]` attributes in `src/` |
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
| `cleanup.rs` | 10403 | 3937 | 120 | `output.rs` | 1065 | 540 | 21 |
| `workspace.rs` | 6301 | 2160 | 79 | `progress.rs` | 945 | 515 | 15 |
| `update.rs` | 2886 | 1289 | 47 | `report.rs` | 365 | 168 | 11 |
| `discovery.rs` | 2363 | 1000 | 39 | `traverse.rs` | 624 | 494 | 6 |
| `interactive.rs` | 2426 | 1311 | 58 | `domain.rs` | 435 | 406 | 1 |
| `config.rs` | 1499 | 613 | 20 | `editor.rs` | 215 | 155 | 2 |
| `main.rs` | 1758 | 1621 | 4 | `error.rs` | 16 | 16 | 0 |
| `cli.rs` | 1293 | 501 | 32 | `lib.rs` | 16 | 16 | 0 |
| `policy.rs` | 1451 | 791 | 21 | | | | |
| `discovery_state.rs` | 1129 | 580 | 17 | | | | |

Two observations a reader should carry forward. **Coverage is concentrated**:
`cleanup.rs` and `workspace.rs` alone hold 193 of the 481 inline tests, and
`main.rs` — where the two pipelines are stitched together — is where the
orchestration decisions live. And **the biggest files are the biggest risk**:
`cleanup.rs` is where bytes get deleted.

`main.rs` holding only four tests is not a neutral fact. They cover the
orchestrator's own decisions — the interactive surface's three endings and the
report verdict — and nothing else; the progress-bar ordering below is still
unpinned. C023 established that the
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
  (`src/domain.rs:210`) that invariant 1 is literally written in. It also owns
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

- **Owns:** `AppError` (`src/error.rs:3-16`) and its six variants — `Config`,
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
  `Invocation` (`src/cli.rs:269`) carrying a `CleanupRequest`
  (`src/cli.rs:239`), a `ScanIntent` (`src/cli.rs:205`), a `ConfigCommand`
  (`src/cli.rs:172`), or an update intent. The `scan-scope` `ArgGroup`
  (`src/cli.rs:89`) expresses "exactly one scope selector" once, so
  requiredness and mutual exclusion cannot drift apart.
- **Also owns:** the mode mapping `clean_mode` (`src/cli.rs:292`) that turns
  the `--dry-run` / `--cargo-preview` flag triple into one `CleanMode`, and
  the hidden-alias declarations.
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
  variants (`src/domain.rs:11-18`) — `Explicit`, `ExplicitRoots`, `Global`,
  `Routine` — of which `resolve` produces three; `ExplicitRoots` is
  constructed only by the cleanup engine. The module also owns C028
  provenance admission for automatic roots, C030 maintenance-root hygiene, and
  the bounded C033 omission summaries, and the `full_cleanup_roots` gate that
  refuses a Full cleanup whose reconciliation was incomplete.
- **Depends on:** `config`, `domain`, `error`, `discovery`, `discovery_state`.
- **Breaks:** this module decides *what is in scope*. A wrong scope either
  misses eligible work or pulls unrelated work into a cleanup. It is the
  boundary between "the tool found something" and "the tool may act".
- **Deep dive:** [04 — Policy & scope](04-policy-and-scope.md)

### 4.8 `discovery.rs` — finding manifests

- **Owns:** the bounded walk that finds `Cargo.toml` files.
  `discover_manifests_with_attribution` (`src/discovery.rs:330`) returns the
  manifests *and* the reason anything was skipped, so coverage loss is
  recorded rather than silently swallowed. It also owns the platform
  prune lists — `effective_rustup_home` (`src/discovery.rs:145`),
  `effective_cargo_home` (`src/discovery.rs:164`), `cargo_home_prunes`
  (`src/discovery.rs:176`).
- **Depends on:** `domain`, `error`, `policy`, `progress`, and the leaf `traverse`.
- **Breaks:** an unrecorded walk error is an incomplete scan reported as
  complete — which would let an incomplete Full reconciliation authorize
  cleanup. The attribution is what makes coverage decidable.
- **Deep dive:** [05 — Discovery](05-discovery.md)

### 4.9 `discovery_state.rs` — learned roots

- **Owns:** the persisted state file: which roots have been learned, how they
  are reconciled, and when they may be retired. `reconcile_full`
  (`src/discovery_state.rs:211`) folds a Full scan's observations into the
  learned set; `publish` (`src/discovery_state.rs:473`) writes it atomically.
  It also owns `automatic_root_is_maintainable`
  (`src/discovery_state.rs:114`) — the C030 hygiene rule — and the
  uncertainty-intersection test.
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
  (`src/cleanup.rs:73`) says which units are candidates. `is_authorized`
  (`src/cleanup.rs:762`) and `covering_is_authorized_roots`
  (`src/cleanup.rs:800`) implement the rule that *authorization never
  manufactures proof* — a non-`PrivateBounded` class returns `Ok(false)`
  before the path is even inspected. `CleanupReasonCode`
  (`src/cleanup.rs:193`), `PolicyDisposition` (`src/cleanup.rs:291`), and
  `CleanReport` (`src/cleanup.rs:395`) are the report vocabulary, and
  `clean_with_roots_policy_selector_guarded`
  (`src/cleanup.rs:1288`) is the engine entry point.
- **Also owns:** the `CleanupRunner` trait (`src/cleanup.rs:686`) — the spawn
  seam — and the admission-premise late recheck, so the premise that admitted
  a root is re-verified immediately before each spawn.
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
  renderer (`src/output.rs:309-885`), whose lines are capped at
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
  candidate rows (`src/interactive.rs:354-361`), never more than the terminal
  has room for; a row is **named** by the shortest path suffix that identifies
  it (`src/interactive.rs:919-944`); the reclaimable size is right-aligned; the
  ownership column is dropped when it would say the same thing on every row;
  and the surface is the only one in the crate that renders colour
  (`src/interactive.rs:1100-1116`). The NOR/INS model and the published key
  table are unchanged.
- **Deep dive:** [16 — Interactive](16-interactive.md)

### 4.18 `main.rs` — the orchestration spine

- **Owns:** composition. `main` (`src/main.rs:11`) parses, resolves one
  intent, and dispatches. `run` (`src/main.rs:70`) is the switch over the
  four intents. `run_cleanup` (`src/main.rs:318`) is the cleanup pipeline,
  `run_scan` (`src/main.rs:1131`) is the read-only pipeline,
  `resolve_cleanup_roots` (`src/main.rs:726`) decides the root set,
  `collapse_roots` (`src/main.rs:1072`) canonicalizes and de-duplicates it,
  `run_engine_guarded` (`src/main.rs:629`) wraps the engine with the
  admission premise, and `emit_cleanup` (`src/main.rs:1012`) projects the
  result.
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
| `policy → discovery` | `src/policy.rs:787` | `crate::discovery::effective_rustup_home()` |
| `discovery → policy` | `src/discovery.rs:337` | `crate::policy::global_discovery_policy()` |
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
(`src/output.rs:509`), which is why its edges have to be checked by hand
rather than by boundary arithmetic. Its apparent `output → output`
self-reference (`src/output.rs:665`) is inside that nested test module, not a
production edge.

---

## 6. The scan pipeline

`run_scan` in `main.rs:1131` is the read-only pipeline. Nine steps, in order:

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
   folds observations into the learned-root set and publishes atomically.
7. **Build physical groups** — `workspace::build_groups`; nested/duplicate
   output roots collapse to one group so bytes are counted once.
8. **Analyze** — `workspace::analyze_groups` measures each group in parallel
   via `traverse` and classifies activity.
9. **Render** — human (`report::render`) or JSON (`output::scan`).

Steps 3–5 and 7–8 are the expensive parts; step 6 is the part that makes the
next run cheap.

Step 4 is where the ADR 007 boundary is drawn. The filter at
`src/main.rs:1218-1223` pulls out exactly three of the seven
`DiagnosticCategory` variants — `PermissionDenied`, `Metadata`, and
`Vanished` — and leaves `OutOfScope` out on purpose, because an out-of-scope
unprovable is informational, not a coverage loss.

**The read-only maintenance inventory covers the same universe the cleanup
would act on (ADR 009).** `scan --known` is the inventory of the maintenance
scope, so it applies the same rule the cleanup path does: the working
directory joins the Routine scope when the Routine scope does not already
reach it (`src/main.rs:1164-1173`). An inventory that described a smaller
universe than the cleanup would not be an inventory of it.

## 7. The cleanup pipeline

`clean` is not a scan plus a `rm`. It is a separate, guarded transaction. The
modes are `CleanMode` in `cleanup.rs:36`, mapped from flags by
`cli::clean_mode` (`src/cli.rs:292`):

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
(`src/main.rs:726`): an explicit `ROOT` argument, `--full` (runs a Full scan,
then uses learned roots), or `--known` (uses only the Routine scope). They are
canonicalized and de-duplicated by `collapse_roots` (`src/main.rs:1072`) so
nested roots cannot produce overlapping deletions.

**The working directory joins the Routine scope (ADR 009).** A bare
`cargo cleanme` run also contributes the operator's position: the workspace
that encloses it, when there is one, and the directory itself
(`src/workspace.rs:655`). The two rules that make this safe are that the
universe may only ever **grow** — a wider universe can only reclassify
`shared` bytes to `private`, never the reverse — and that the working
directory joins **only** when the Routine scope does not already reach it
(`src/main.rs:931`). The second rule is a measurement, not a taste: without
it, a bare run from the home directory turned into a 65-second sweep of
264,183 directories that ended *blocked* on 1,357 unresolvable manifests in
`~/.local/share/Trash`. `clean ROOT` remains the spelling for that wider
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
   `OutputOwnershipClass` in `domain.rs:210` is the vocabulary; anything
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
   (`main.rs:856-861`). Anything that narrows the universe re-opens
   ADR 006's shared-output hazard.

---

## 9. Tooling and verification surfaces

| Surface | What it is | Deep dive |
|---|---|---|
| `tests/cli_contract.rs` | 74 contract tests over the CLI surface, exit codes, and reason codes. | [14 — Testing](14-testing-and-verification.md) |
| `tests/end_to_end.rs` | 4 end-to-end tests over a real fixture. | [14 — Testing](14-testing-and-verification.md) |
| `tests/common/` | The shared fixture harness. | [14 — Testing](14-testing-and-verification.md) |
| `src/**/*.rs` inline tests | 493 `#[test]` functions, concentrated in `cleanup.rs` (120) and `workspace.rs` (79). | [14 — Testing](14-testing-and-verification.md) |
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
2026-10-21, which added item 62.

Items 3-6 are re-verified as of the Phase-11 and Phase-12 closures (all of
C018/C019/C021-C025 are now closed). Their findings were real when recorded and
each is now closed by shipped code; the deep dives that carry them were updated in
the same pass, so these rows point at completed work rather than at a plan.

**Sixty-three** findings are recorded here. The count is re-derivable: it is the
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
| 16 | **`CleanReport.out_of_scope_unresolved` (`src/cleanup.rs:414`) is rendered in the human report (`src/cleanup.rs:518-521`) but has no field in the JSON cleanup DTO.** ADR 007's "the system is not clean, the boundary is" signal exists only in prose. | **Open — not yet planned.** Same shape as item 15: a missing JSON projection. |
| 17 | **`main.rs`'s uncertainty filter is a silent `matches!` over three of the seven diagnostic categories** (`src/main.rs:1218-1223`) — no `_` arm, so it is not compile-checked. `OutOfScope` is correctly excluded, but a future eighth category would silently reclassify an incomplete scan as complete. | **Open — not yet planned.** The right shape is an exhaustive match or an explicit allow-list with a test that fails on a new variant. |
| 18 | **`OutputRootKind`'s human label builder has a `_` arm** (`src/workspace.rs:2025-2030`), so it is *not* compile-checked the way the sort key at `src/workspace.rs:2020-2023` is. A new kind renders as `"output"` rather than failing to build. | **Open — not yet planned.** |
| 19 | **`AutomaticOmission::NotMaintainable` conflated two different causes** — "the path does not exist at a transient origin" and "the path exists but is a non-maintainable location" — and the consequence was that `omission_diagnostics` printed "omitting N *non-maintainable* learned root(s)" for roots that are simply absent. | **Fixed in C038.** The by-design bucket is no longer reported at all: a root omitted because it sits at a known-transient origin is omitted *by design* — the tool would never have cleaned it — so it is not a diagnostic and never reaches the operator. A durable root that has vanished, and a root that is no longer a directory, are still reported. |
| 20 | **`--include` / `--exclude` match the workspace root, not the physical output root** (`src/cleanup.rs:1467`, `src/cleanup.rs:1477`), and neither the flag help nor the config key name says so. A user writing `include = ["**/target"]` gets no match and no warning. | **Open — not yet planned.** |
| 21 | **A configured `scan.root` is silently ignored by a rootless `scan`** — `policy::resolve` returns from its `full` arm before the configured root is read. The behaviour is deliberate (`src/main.rs:1143-1146` says configuration must not narrow the canonical reconciliation command), but the config key's own documentation does not mention it. | **Open — not yet planned.** Documentation, not behaviour. |
| 22 | **The root `long_about` claimed a bare `cargo cleanme scan` is the full machine reconciliation** (`src/cli.rs:27-28`, since rewritten). It was a usage error (exit 2) — the `scan-scope` ArgGroup requires one of ROOT / `--known` / `--all`. Only `cargo cleanme --help` showed the lie; the manpage was unaffected because `xtask` overwrote the root `long_about` with its own block. | **Fixed by C040** (ADR 009). The root `long_about` now states that `scan` **needs a scope** and that `--all` is the full read-only machine reconciliation (`src/cli.rs:29-32`), and the root manpage is generated from that same `long_about` rather than from a generator-side block, so the two surfaces can no longer drift. Discharges [C036](../plans/implementation/artifact-discovery-cleanup/c036-cli-surface-truthfulness-and-documentation-drift-corrective.md) D1. |
| 23 | **A user-facing diagnostic points the operator at `scan --full`** (`src/discovery.rs:373-380`) — a *hidden* alias that does not appear in `scan --help`. The canonical spelling is `scan --all`. | **Covered by C036 D3** (proposed, not accepted). |
| 24 | **`cli::absolutize_root` and `main::collapse_roots` disagree about `..`.** The CLI folds `..` lexically; `main.rs:1075` canonicalizes, which resolves symlinks first. A relative root whose `..` crosses a symlink names different directories under the two. No test covers the disagreement. | **Open — not yet planned.** |
| 25 | **A root-level `--dry-run` is accepted alongside `scan` and then silently ignored** — the root flag is not global, clap accepts it, and the `Invocation::Scan` arm never reads it. Harmless today because a scan is read-only, but it is the same shape as the discarded-flag defect the `Clean` and `Update` arms were fixed for. | **Open — not yet planned.** |
| 26 | **One lib test fails on any host whose Cargo version is outside an exact allowlist.** `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` expected `Previewed` and got `Skipped`, because `clean_capabilities_from_version` (`src/workspace.rs:146-165`) enables `profile_selector` only for nine exact versions — `1.89.0`, `1.90.0`, `1.91.1`, `1.92.0`, `1.93.1`, `1.94.1`, `1.95.0`, `1.98.1`, `1.99.0` — and the host here is `1.99.1`. The tool skips in the **safe** direction; the *test* was what failed. The defect was the test's **unstated host premise**, and the exact-version allowlist made it structural: the first patch release of any qualified minor is outside the set, so the case had a known expiry date with no alarm before it. | **Corrected by C037** (`plans/implementation/artifact-discovery-cleanup/c037-host-cargo-dependent-selector-premise-corrective.md`); the hosted release is outstanding. The case now derives its expectation from the product's own premise function and asserts a *specific* outcome per premise — a qualified host must preview, an unqualified one must refuse with `SelectorUnsupported` — rather than a disjunction. Proven by mutation in both directions, and the selector-to-Cargo composition is now covered by a host-independent case. No production behaviour changed: `src/workspace.rs` and `release/selector-qualification.json` are byte-identical. |
| 27 | **`traverse::tests::source_scan_short_circuits_recent_files` does not discriminate what it claims.** It writes one fresh file into a *freshly created* temp dir and walks that dir as the root. The root's own mtime is fresh, the root is not excluded from the walk, and `Order::ParentFirst` yields the root first — so `Recent` is returned on the **directory's** mtime before the file is ever examined. The test would still pass if the file were deleted, if the guard ignored files entirely, or if directories were excluded from the evidence. | **Open — not yet planned.** This is the AGENTS.md §4 "green test" shape exactly: a passing test that cannot fail for the reason it claims. It needs a discriminating fixture pair. |
| 28 | **The C028 narrow-absence rule has *zero* discriminating coverage.** Verified by mutation on a scratch copy: deleting the probe (`src/discovery_state.rs:308-317`) leaves **all 17** `discovery_state` unit tests green and **all 62** `cli_contract` integration tests green. The nominal test `complete_certain_full_prunes_only_positively_absent_learned_roots` (`src/discovery_state.rs:966`) stages roots under `tempfile::tempdir()` with `home: None`, so the **C030** rule at `src/discovery_state.rs:298-300` drops them first — the temp root is forbidden when `home` is `None` (`src/discovery_state.rs:165-166`). The symmetric mutation (deleting C030 instead) fails two tests, confirming C030 is the rule actually under test. | **Open — not yet planned.** The rule is live in production and untested; only the rule that shadows it is tested. |
| 29 | **`discovery_state::tests::zero_retention_disables_expiration_and_nested_roots_collapse` no longer demonstrates its name.** Same fixture problem: the assertion is now `learned_roots.is_empty()`, so the zero-retention path and the nested-root containment collapse are both unexercised. Only the exact-duplicate fold (`src/discovery_state.rs:1064`) is covered. | **Open — not yet planned.** |
| 30 | **Production `uncertainty` is always empty.** All three uncertainty categories now emit `path: None` (`src/discovery.rs:792`, `src/discovery.rs:987`); `main.rs:1225` drops them, and the same events clear `coverage_complete` (`src/discovery.rs:638`, `src/discovery.rs:912`), so the scan is incomplete and never publishes. The fail-open `filter_map` edge is therefore the only path — and it is covered by the completeness gate, not by the uncertainty veto. The unit tests at `src/discovery_state.rs:880` and `src/discovery_state.rs:1029` assert a behaviour the pipeline cannot produce. | **Open — not yet planned.** |
| 31 | **`discovery_state::sweep_own_stale_temps` (`src/discovery_state.rs:506-534`) has no test at all.** Its own-pid and one-hour scoping is a safety property: it must never delete another publisher's in-flight temp file. | **Open — not yet planned.** |
| 32 | **An unreadable output root is reported as *missing*, not as a diagnostic.** `workspace::has_artifact_entries` returns `Option<bool>`, which cannot distinguish "empty" from "unreadable", so the `None` case folds into `missing_all` (`src/workspace.rs:1637`). An unreadable covering root is therefore reported as absent rather than as a coverage loss — weaker than `output_root`'s own careful "unreadable is not absent" handling (`src/workspace.rs:306-333`). The window is narrow (read permission on the directory only), but it is a fail-closed-reporting hole. | **Open — not yet planned.** |
| 33 | **No test covers a conflicting member→root mapping.** `member_to_root.entry(..).or_insert(..)` (`src/workspace.rs:974-986`) is first-writer-wins; a conflict is neither detected nor diagnosed, and the fast path (`src/workspace.rs:839-845`) means a mis-mapped manifest is never re-located in that pass. `TwoWsRunner` (`src/workspace.rs:3579`) covers only the legitimate two-workspace case. | **Open — not yet planned.** |
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
| 56 | **`architecture/05-discovery.md` asserted that the uncertainty vector "still carries known, path-specific uncertainty into `reconcile_full`"** — while `architecture/06-discovery-state.md` correctly documented that it is always empty. All three uncertainty categories are emitted with `path: None` (`src/discovery.rs:783-797`, `:978-996`), so `filter_map(|d| d.path.clone())` (`src/main.rs:1216-1224`) drops every one of them. | **Fixed in this pass.** |
| 57 | **The per-platform inline-test census disagreed across three documents and with itself.** `test-evidence/SKILL.md` claimed 320 declared / 318 Linux / 296 Windows / 317 macOS; `architecture/14-testing-and-verification.md` claimed 450 / 448 / 420 Windows; and a figure of 443/415 was in circulation. The truth is **450 declared, 448 Linux, 411 Windows, 445 macOS** — 35 `#[cfg(unix)]` and 4 Linux-only gates. A naive grep for `#[cfg(unix)]` immediately above `#[test]` finds 26, because nine of the 35 have the gate *after* the `#[test]`. | **Fixed in this pass.** All three documents now state the current census — 493/491/453/488 after M020 — and the derivation method (a brace-tracking evaluation against `rustc --print cfg --target <triple>`, cross-checked against `cargo test --lib -- --list`) is recorded so the next reader can re-derive rather than copy. |
| 58 | **Thirteen source doc comments still describe the pre-ADR 007 whole-scope block.** `src/cleanup.rs:300`, `:328`, `:1141`, `:2044`, `src/workspace.rs:394`, `:978`, and `src/main.rs:304`, `:366`, `:590`, `:831` say "whole-scope" where the behaviour is boundary-scoped; `src/cleanup.rs:315-316` says "coverage of the selected scope is unproven" for the same reason. `tests/cli_contract.rs:1935-1936` carries the same wording on a test whose *behaviour* is correct (its unresolved manifest is inside the anchor, so it does block). | **Open — not yet planned.** Deliberately **not** fixed in this pass: the diff is documentation-only by design, and editing `src/` shifts line numbers that 2,742 bare citations depend on — which `AGENTS.md` §5 forbids doing casually. The correction needs a corrective that re-derives the affected citations in the same change. Recorded as C036 D27. |
| 59 | **Several inline tests assert on wall-clock and thread-scheduling observations, so the suite is load-sensitive.** `cleanup::tests::proof_refresh_parallelism_is_bounded_and_results_stay_deterministic` asserts the observed peak proof-refresh concurrency equals the reported counter, and `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target` asserts a recency-window reason code. Under CPU contention the observations move and the assertions fail. Measured: on the C038 tree, 8 consecutive `cargo test --all-targets --all-features` runs with an idle host all passed, and the same tree run concurrently with a build failed; on the **unmodified baseline** (`426cb72`) under the same contention, three tests failed — the same two plus `workspace::tests::real_conventional_workspace_is_private_bounded`. | **Open — not yet planned.** Pre-existing, and not a C038 regression: the baseline fails under the same load. The lesson is the repository's own — *a green test proves only that its own premises hold*, and here one of the premises is an unloaded host. A fix belongs with the real-Cargo fixtures, not with a corrective that changed no fixture. |
| 60 | **The `-i` selection surface was a *list*, not a *finder*, and it was built that way on purpose.** The frame was exactly the terminal height (`list_rows()` returned `height - CHROME_LINES`), so the list filled the screen instead of being a window; there was no scroll-position indicator, no paging key (`Key` had no PageUp/PageDown/Home/End), and the absolute path of every row occupied most of the width while the part that distinguishes one workspace from another sat at its far end. Every row repeated the word `private`, whether or not that was news. The surface was **functionally correct** — every defect here is a presentation defect — but the module's own tests asserted the *shape* rather than the *usability*, and one of them (`the_frame_is_exactly_the_terminal_height_and_never_overflows_the_width`) **pinned the defect as the contract**. | **Fixed in C039.** The frame is now a bounded page of at most fifteen rows that never outgrows the terminal; a row is named by the shortest path suffix that identifies it uniquely among the candidates; the reclaimable size is right-aligned; the ownership column is omitted when it would say the same thing on every row; the gutter marks the window edges and the cursor; and the surface renders colour, opt-out via `NO_COLOR`. The NOR/INS model and the published key table are unchanged. The test that pinned the old shape was rewritten to pin the new property, and the evidence is a pty capture of the real binary, not a string comparison — the repository has no pty harness (finding **48**), so the rendered surface had never been observed by a test. |
| 61 | **The update path's candidate spawn can fail with `ETXTBSY` under load, and the failure is a *spawn* failure rather than a *proof* failure.** `run_bounded` (`eggup-core-0.1.2/src/candidate.rs:141`) executes the staged candidate to prove its identity before the commit; when the staged file is still open for writing at that instant, `execve` refuses it with `Text file busy (os error 26)`. The transaction aborts before the commit and the error is reported, so **nothing is deleted and the ownership proof is untouched** — but the same path runs in production during a real `cargo cleanme update`, so a user can be handed a failure they cannot act on. Found by the v0.2.10 release gate: it failed on the first run and passed on the second with an identical tree. `src/update.rs` is untouched by C039 and C040, so the defect is **pre-existing** and not a regression. | **Open — tracked as C041.** The mechanism is narrowed but **not proven**: the next step is instrumentation, not more reading, and the plan requires a reproduction that fails against the current code. Upstream `eggup-core` 0.1.3 was diffed against 0.1.2 and does **not** carry the fix (`run_bounded` is byte-identical), so "pin upstream until it lands" is not available. Re-measured: the test alone passed 40 of 40 consecutive runs, and an 8-way parallel full-suite burst passed 8 of 8. |
| 36 | **`cargo cleanme -i` leaves the terminal cursor hidden when the session ends in NOR mode.** `session_loop` hides the cursor at `src/interactive.rs:1145` and the per-frame call at `src/interactive.rs:1155` sets it to `mode == Insert`. The commit and quit arms (`src/interactive.rs:1159-1168`) rewind and clear **without restoring it**, and `set_cursor_visible(true)` is never called anywhere in the crate. `RawModeGuard` restores the terminal *mode* (`disable_raw_mode`) but cursor visibility is a separate escape sequence; `main.rs` has no cursor handling at all. The existing test `the_driver_shows_the_hardware_cursor_only_in_insert_mode` (`src/interactive.rs:1959-1974`) asserts the *last* recorded value is `Some(false)` — it **pins** the behaviour rather than catching it. | **Open — not yet planned. Verified live.** Driven over a raw pty: the tool emitted `ESC[?25l` (Hide) twice and `ESC[?25h` (Show) **zero** times, then exited 0 with `cargo-cleanme: interactive selection cancelled; no cleanup commands were run`. The operator's shell is left with no visible caret. The module's own argument — "a tool that can leave a terminal in raw mode is not shippable" — applies verbatim to the cursor. |
| 37 | **`-i` and non-`-i` disagreed on the exit code for a scope that has roots but no cleanup units.** `run_interactive` refused an empty candidate list (`src/interactive.rs:1281-1283`), and the orchestrator mapped that to `AppError::Config` (`src/main.rs:603-616` at the time) → **exit 2**. The same scope without `-i` reported zero units and exited 0. | **Fixed in C038.** The empty surface is now a plain negative result: `SessionResult::NothingToClean` → one line, `Nothing to clean.`, on stdout, exit 0. The two spellings of one scope now agree. |
| 38 | **`plans/output-schema-v1.md` is missing a reason code the code actually emits.** The schema lists 19 `reason_code` values; the code has **20** (`src/cleanup.rs:263-286`). The missing one is `skipped_outside_selection` — the value M014 actually emits for a workspace outside the selected scope (`src/cleanup.rs:1431`). | **Open — not yet planned.** Fits the C036 drift remit, but C036 is `proposed` and not accepted. |
| 39 | **`plans/output-schema-v1.md` describes a non-UTF-8 path as using "the platform escaped debug form".** `output::path` (`src/output.rs:262-266`) now uses `to_string_lossy()`, so both the human and the machine output render `U+FFFD` and **agree** — the schema's description of a divergence that no longer exists is stale. The schema's own wording ("are UTF-8 strings when representable") is also wrong: a path is always a UTF-8 `String`, representable or not. | **Open — not yet planned.** Note the consequence: `output::path` is lossy and therefore **not injective**, so a JSON path string must not be used as a join key. |
| 40 | **`plans/registry.md` contradicts itself about the self-update smoke.** The M011C row says the automatic smoke is "demonstrated three times" while the evidence table below it lists **seven** green automatic smokes. | **Open — not yet planned.** A registry is a control surface; a self-contradicting one is worse than a stale one, because a reader cannot tell which number to trust. |
| 41 | **`progress::set_determinate_total` (`src/progress.rs:322`) is dead *and* internally inconsistent.** It sets the bar's total without zeroing `determinate_done`, unlike `apply_total` (`src/progress.rs:335`). It is the one path that can leave the displayed total and done-count mismatched. It has zero callers. | **Recorded in §10.1** — M020 confirmed zero call sites and removed its `set_length` call, so the inconsistency is now state only. The public item is kept deliberately (see §10.1's semver note). |
| 42 | **The progress bar's message template hardcoded `"analyzing {done}/{total} …"` whenever a total existed**, so a `CleanupExecute` bar read *"analyzing 3/12 …"*. The root cause was the duplicated counter set, not the label. | **Closed by M020.** The renderer now composes the whole line from the phase label (`src/progress.rs:348`) and the fraction (`src/progress.rs:365`), so every phase names itself: *"scanning…"*, *"resolving workspaces…"*, *"analyzing output 3/12…"*, *"cleaning…"*. The phase also moved out of the bar's *prefix* into its message, because indicatif's default spinner template (`{spinner} {msg}`) never renders the prefix — so the transitions had never been visible at all. |
| 43 | **`--format json` on an attended terminal gets no progress at all.** The format gate at `src/main.rs:1178-1179` suppresses the renderer on anything other than `OutputFormat::Human`, regardless of whether stdout is a pipe. Progress is on **stderr**, so a `jq` consumer is unaffected by it — and the log mode's suppression is independently justified, but the JSON mode's is not. A minutes-long `--format json` scan gives the operator nothing. | **Open — not yet planned.** |
| 44 | **The contract tests do not cover the whole contract.** `cleanup_reason_codes_have_stable_contract_names` (`src/cleanup.rs:4177-4203`) covers **16 of 20** reason codes — it omits `skipped_ownership_unproven`, `skipped_outside_selection`, `selector_unsupported`, and `selector_invalid`. `policy_dispositions_have_stable_json_names` (`src/cleanup.rs:4157-4174`) covers **5 of 6** dispositions — it omits `selector_estimate_unavailable`. | **Open — not yet planned.** The stable-name tests are the only guard on the published string vocabulary, so the uncovered four and one are unguarded. |
| 45 | **The positive `allowed_output_roots` path is untested end-to-end.** `config.toml` calls the field "operationally redundant" and the code advertises the remediation at `src/cleanup.rs:963-970`, but no test shows a `PrivateBounded` group outside every clean root becoming `Cleaned` through an allowed root. | **Open — not yet planned.** This is the one path where *authorization* — not proof — is what makes a group cleanable, so it is the path most worth an end-to-end case. |
| 46 | **Two error paths in `main.rs` report an unmeasured zero.** `src/main.rs:683` and `src/main.rs:708` build a `CleanReport` after the engine returned `Err`, so the out-of-scope count it had computed was discarded and `out_of_scope_unresolved` is set to `0` — an unmeasured zero on a path that *did* traverse. The other two report sites (`src/main.rs:381`, `src/main.rs:406`) are correct. | **Open — not yet planned.** A machine consumer reading `out_of_scope_unresolved: 0` cannot tell "nothing was out of scope" from "we never got to count". |
| 47 | **The one-JSON-document guard is asserted by a string check, not behaviourally.** `tests/cli_contract.rs:3141-3144` asserts that `src/main.rs` contains the text `"enum EmitReport"`. Renaming the enum breaks the test — good — but keeping the enum while every call site passes `EmitReport::Yes` leaves it green, and the two-JSON-documents bug returns. A source check on the *call site* (`src/main.rs:765`) would be a stronger, equally cheap assertion. | **Open — not yet planned.** AGENTS.md §4's own lesson in miniature. |
| 62 | **The Full cleanup operation is unreachable for an ordinary user.** `cargo cleanme clean --full` runs a Full reconciliation first (`src/main.rs:765`), and a platform-root walk by a non-root user is **never** complete, so `policy::full_cleanup_roots` (`src/policy.rs:519-543`) refuses the non-zero reconciliation code (`src/policy.rs:523-528`) and the operation exits **2** before it cleans anything. The refusal is unconditional in practice, not merely common: it happens because regions that are irrelevant to the cleanup — `/var`, `/snap`, `/run` — are unreadable, not because anything about the learned roots is unproven. **Verified live** on 2026-10-21 (uid 1000): exit `2`, stdout empty, 128 permission-denied traversal errors under `/`. The gate is unit-tested, but only with a literal `1` (`src/policy.rs:1150`), so the test proved its own premise and never that a real machine produces that code. | **Decided — ADR 008 §5, route 5b-i (accepted 2026-10-21).** The Full cleanup's proof boundary becomes the **learned root**: a root whose traversal completed is proven and cleaned, one whose traversal did not complete is skipped with a typed reason, and the operation proceeds with the roots it can prove. Implementation is planned in **M018** (the gate) and **M019** (the canonical spelling), both *proposed*. The disclosed residual — an unknown participant in an unreadable region outside every learned root cannot be disproven — is the same shape as ADR 007's out-of-boundary residual, and it is disclosed rather than fixed. ADR 007 §1 and *Residuals* are amended accordingly. |
| 63 | **The cleanup's human surface wrote control characters in a path raw.** `cleanup::escaped_path` returned a valid-UTF-8 path unchanged, so a directory whose name contained a newline or an escape sequence wrote that sequence straight into the terminal — the exact hazard `report::path_text` was written to prevent. The machine surface already escaped, so the two surfaces disagreed about the same path. | **Closed by M020.** Both human surfaces now render paths through `HumanOptions::path` (`src/report.rs:50`), which escapes control characters and degrades non-UTF-8 to U+FFFD; `escaped_path` is gone. |

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
