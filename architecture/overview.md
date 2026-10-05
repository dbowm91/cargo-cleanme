# cargo-cleanme — Architecture Overview

A birds-eye view of how the crate is put together, and an index into the
per-component deep dives that follow.

- **What it is:** a Cargo subcommand (`cargo cleanme`) that finds inactive Cargo
  build artifacts on a machine and either reports or safely reclaims them.
- **Shape:** one binary (`src/main.rs`) over one library (`src/lib.rs`), 15
  modules, ~18.3k lines of Rust in `src/`, no async runtime of its own, and
  exactly one binary dependency that matters for safety (`cargo metadata`).
- **Version:** 0.1.6 · **Edition:** 2024 · **MSRV:** 1.89
- **Deep dives:** 15 documents, one per component, linked from §3 and §9.

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

## 2. Layer map

Dependencies flow strictly downward, and no module imports a module that
imports it back. The one apparent exception (`workspace` → `cleanup`) is a
single test-only call into authorization helpers, not a production edge.

```text
                    ┌──────────────────────────────────────────┐
  binary entry      │  main.rs        orchestration spine      │
                    └───────────────┬──────────────────────────┘
                                    │
        ┌───────────────┬───────────┼─────────────┬──────────────┐
        │               │           │             │              │
        ▼               ▼           ▼             ▼              ▼
   ┌─────────┐   ┌──────────┐ ┌──────────┐ ┌───────────┐  ┌──────────┐
   │  cli    │   │  config  │ │  update  │ │  output   │  │ progress │
   │  (args) │   │ (policy) │ │(self-upd)│ │ (JSON DTO)│  │ (render) │
   └────┬────┘   └────┬─────┘ └────┬─────┘ └─────┬─────┘  └────┬─────┘
        │             │            │             │             │
        └─────────────┴──────┬─────┴─────────────┘             │
                             ▼                                   ▼
                    ┌────────────────┐                  ┌──────────────┐
                    │  policy::resolve│                  │  traverse    │
                    │  scope decision │                  │ (measurement)│
                    └────────┬───────┘                  └──────────────┘
                             │
        ┌────────────────────┴─────────────────────┐
        ▼                                          ▼
 ┌─────────────┐   ┌──────────────────┐   ┌──────────────────┐
 │  discovery  │   │ discovery_state  │   │    workspace     │
 │ (find Cargo │   │ (learned roots,  │   │ (cargo metadata, │
 │  manifests) │   │  reconciliation) │   │  groups, units)  │
 └──────┬──────┘   └──────────────────┘   └────────┬─────────┘
        │                                            │
        └────────────────┬───────────────────────────┘
                         ▼
                ┌─────────────────┐        ┌──────────┐
                │     cleanup     │───────▶│  report  │
                │ (authorize,     │        │ (human)  │
                │  prove, execute)│        └──────────┘
                └─────────────────┘
                     │            │
                     ▼            ▼
              ┌────────────┐  ┌──────────┐
              │   domain   │  │  error   │
              │ (vocabular)│  │(AppError)│
              └────────────┘  └──────────┘
```

`domain.rs`, `error.rs`, `lib.rs`, `editor.rs`, `traverse.rs`,
`discovery_state.rs` and `cli.rs` are leaves — they import nothing from the
crate. That is the load-bearing structural fact of this codebase: **the safety
vocabulary has no dependencies on the machinery that could be tempted to misuse
it.**

---

## 3. Module index

Line counts are `total`; where a module carries an inline `#[cfg(test)]`
module, the `prod / test` split is given. The crate has **236 inline `#[test]`
functions**, and **48.7% of them live in the two largest modules**
(`cleanup.rs` 71 + `workspace.rs` 44). Five modules have **zero** tests:
`main.rs`, `output.rs`, `domain.rs`, `error.rs`, `lib.rs`.

| Module | Lines | Role | Deep dive |
|---|---:|---|---|
| [`main.rs`](13-orchestration.md) | 590 | Binary entry: argv → command dispatch → exit code. The only place the two scan pipelines are stitched together. | [Orchestration](13-orchestration.md) |
| [`cli.rs`](02-cli.md) | 506 / 201 | Clap surface, `cargo`-subcommand argv normalization, scan-root absolutization. | [CLI](02-cli.md) |
| [`config.rs`](03-config-and-editor.md) | 548 / 309 | Load/create/validate `config.toml`; embedded template; path resolution. | [Config & editor](03-config-and-editor.md) |
| [`policy.rs`](04-policy-and-scope.md) | 349 / 248 | Turns a `ScanRequest` + config into an `EffectiveScanPolicy` — the Routine/Full scope decision. | [Policy & scope](04-policy-and-scope.md) |
| [`discovery.rs`](05-discovery.md) | 1370 / 826 | Finds `Cargo.toml` manifests across a bounded walk, with attribution for what was pruned. | [Discovery](05-discovery.md) |
| [`discovery_state.rs`](06-discovery-state.md) | 656 / 402 | Persisted "learned roots", uncertainty-aware reconciliation, atomic publish. | [Discovery state](06-discovery-state.md) |
| [`workspace.rs`](07-workspace.md) | 3771 / 1446 | `cargo metadata` resolution, capability probing, physical grouping, cleanup-unit construction. | [Workspace](07-workspace.md) |
| [`traverse.rs`](08-traverse.md) | 544 / 456 | Parallel size/recency measurement and source-activity classification. | [Traverse](08-traverse.md) |
| [`cleanup.rs`](09-cleanup.md) | 6161 / 2323 | Authorization, ownership proof, pre-spawn decision, three clean modes. The heart of the safety story. | [Cleanup](09-cleanup.md) |
| [`report.rs`](10-reporting.md) | 195 / 72 | Human-readable rendering of a `ScanReport`; byte formatting. | [Reporting](10-reporting.md) |
| [`output.rs`](10-reporting.md) | 265 | Versioned machine-readable DTOs (`EnvelopeV1`); the stable JSON contract. **No tests.** | [Reporting](10-reporting.md) |
| [`progress.rs`](11-progress.md) | 703 / 500 | `ProgressObserver` trait + indicatif renderer; terminal capability detection. | [Progress](11-progress.md) |
| [`update.rs`](12-self-update.md) | 1971 / 1021 | Eggup-based self-update: provenance classification, version authority, staged replace. | [Self-update](12-self-update.md) |
| [`domain.rs`](01-domain-and-errors.md) | 381 | The shared vocabulary: reports, ownership classes, counters, diagnostics. Pure data. | [Domain & errors](01-domain-and-errors.md) |
| [`error.rs`](01-domain-and-errors.md) | 16 | `AppError` — the six variants the binary can fail with. | [Domain & errors](01-domain-and-errors.md) |
| [`lib.rs`](01-domain-and-errors.md) | 15 | Module manifest. | [Domain & errors](01-domain-and-errors.md) |
| [`editor.rs`](03-config-and-editor.md) | 215 / 156 | Resolves `$EDITOR`/`$VISUAL` for `config edit`. | [Config & editor](03-config-and-editor.md) |

Supporting surfaces, outside `src/`:

| Surface | Deep dive |
|---|---|
| `tests/` (2 suites + shared harness), `scripts/` (12 contract checkers), `.github/workflows/ci.yml` | [Testing & verification](14-testing-and-verification.md) |
| `packaging/`, `completions/`, `man/`, `release/`, `xtask/`, the 3 release workflows | [Distribution & release](15-distribution-and-release.md) |
| `plans/`, `examples/`, `docs/` — decision records, qualification harness, user docs | referenced from the relevant deep dives |

Note that `architecture/` is itself outside the `Cargo.toml` `include`
allowlist, so none of this documentation ships in the published crate.

---

## 4. The scan pipeline

`run_scan` in `main.rs:284` is the read-only pipeline. Nine steps, in order:

1. **Resolve config** — `config::load_or_create` (`main.rs:299`).
2. **Resolve scope** — `policy::resolve` turns `{cli_root, full}` into concrete
   roots + a recency window (`main.rs:300`).
3. **Discover manifests** — `discovery::discover_manifests_with_attribution`
   walks the scope for `Cargo.toml`, recording *why* anything was skipped
   (`main.rs:320`).
4. **Partition uncertainty** — permission-denied / metadata / vanished
   diagnostics are pulled out as `uncertainty` (`main.rs:327`). This vector
   later decides whether learned state may be trusted.
5. **Resolve workspaces** — `workspace::resolve_workspaces` shells out to
   `cargo metadata` once per unique canonical workspace root (`main.rs:347`).
6. **Reconcile state** — for a Full scan, `discovery_state::reconcile_full`
   folds observations into the learned-root set and publishes atomically
   (`main.rs:387`).
7. **Build physical groups** — `workspace::build_groups`; nested/duplicate
   output roots collapse to one group so bytes are counted once (`main.rs:446`).
8. **Analyze** — `workspace::analyze_groups` measures each group in parallel
   via `traverse` and classifies activity (`main.rs:454`).
9. **Render** — human (`report::render`) or JSON (`output::scan`) (`main.rs:470`+).

Steps 3–5 and 7–8 are the expensive parts; step 6 is the part that makes the
next run cheap.

## 5. The cleanup pipeline

`clean` is not a scan plus a `rm`. It is a separate, guarded transaction with
three modes (`main.rs:102`):

| Mode | Flag | What it does |
|---|---|---|
| `Preview` | default, or `--dry-run` | Asks Cargo what it *would* remove. No deletion. |
| `Simulate` | `--dryrun` | Full ownership proof + selection, but the deletion step is a no-op. |
| `Execute` | `--yes` | Proof, then actually removes. |

Note the deliberate spelling split: `--dry-run` (Cargo preview) and `--dryrun`
(simulation) are different flags with different meanings (`main.rs:100-110`).
Deletion itself is always performed by `cargo clean`, never by direct
filesystem removal, so Cargo's own bookkeeping cannot be bypassed.

Cleanup roots come from three sources, in priority order (`main.rs:113`): an
explicit `ROOT` argument, `--full` (runs a Full scan, then uses learned roots),
or `--known` (uses only the Routine scope). They are canonicalized and
de-duplicated by `collapse_roots` (`main.rs:269`) so nested roots cannot produce
overlapping deletions.

---

## 6. Cross-cutting invariants

These are the properties the architecture is shaped around. They are the
things a reviewer should check first in any change.

1. **Ownership proof, not timestamps.** A candidate is only actionable when it
   is classified as private-and-bounded under a resolved ownership universe.
   `OutputOwnershipClass` in `domain.rs:186` is the vocabulary; anything
   `Shared`, `Uncertain`, or `ExternalUnproven` is inventory-only, forever.
2. **Authorization never manufactures proof.** `allowed_output_roots` lets a
   user authorize a location for deletion; it cannot upgrade an unproven
   ownership class. `covering_is_authorized_roots` returns `Ok(false)` for any
   non-`PrivateBounded` class *before* it inspects a path
   (`cleanup.rs:493-495`). See [Cleanup §5](09-cleanup.md).
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
   scope, `2` fatal error — with the caveats in §7 below.

---

## 7. Findings from the deep dives

The deep dives surfaced the items below. They are **observations from reading
the code, not confirmed defects** — several are judgement calls a maintainer
should resolve — and each links to the deep dive that establishes it with
`file:line` evidence. Nothing here has been fixed; this section exists so the
next reviewer does not have to rediscover them.

| # | Finding | Where |
|---|---|---|
| 1 | **JSON `scope` label is derived from CLI flags, not the resolved policy.** A scan configured with `scan.root` resolves to `ScanScope::Explicit` but reports `"routine"`. `tests/end_to_end.rs:202` hardcodes `"routine"` while its own fixture sets `ScanConfig::root`, so the test encodes the behaviour rather than catching it. | [Orchestration §8](13-orchestration.md) |
| 2 | **An incomplete ownership universe at *proof* time fails closed but reports success.** Every candidate becomes `Skipped`, yet `scope_blocked` stays `None` and `failed` stays `0`, so the process exits `0`. At *scan* time the same condition does abort. Safety is fail-closed; the signal is not. | [Cleanup §9](09-cleanup.md) |
| 3 | **Self-update provenance classification fails *open* in four paths** — unreadable/unparseable `.crates.toml`, a root deeper than `CARGO_ROOT_SEARCH_DEPTH = 4`, a non-UTF-8 filename, and malformed records — each defaulting to "self-managed, replaceable". This is the narrowed form of the shipped C017 defect. | [Self-update §3](12-self-update.md) |
| 4 | **Release integrity rests on HTTPS plus a self-published checksum.** Installers enforce a mandatory SHA-256 sidecar, but there is no signature verification anywhere — no GPG, sigstore, minisign, or notarisation. A compromised publish step yields a self-consistent artefact every check accepts. | [Distribution §3](15-distribution-and-release.md) |
| 5 | **The live self-update rehearsal is manual.** `post-release-smoke.yml` is `workflow_dispatch`-only. A C016- or C017-class defect can ship again, because a live rehearsal is the only thing that has ever caught one. | [Distribution §8](15-distribution-and-release.md), [Testing §7](14-testing-and-verification.md) |
| 6 | **Two `scripts/` files are wired into nothing.** `validate-staged-release.py` — the sole "published bytes == qualified bytes" proof — is invoked by no workflow and not even chained by `release-check.sh`; `qualify-cargo-selectors.sh` likewise. No record states whether this is by design. | [Testing §6](14-testing-and-verification.md) |
| 7 | **Ten divergences between `plans/output-schema-v1.md` and the code**, in both directions. Most significant: `selector_estimate_bytes` is null on exactly the runs where a selector is active (inverted vs the doc), and the code emits a 19th `reason_code` the doc does not list. | [Reporting §7](10-reporting.md) |
| 8 | **Exit codes respond to only 3 of 10 stderr warnings.** The Full-branch state warnings change the exit code; the Routine branch's byte-identical "state was not saved" warning is ignored, as are the diagnostics summary and policy's Routine warning. A failed state save also contradicts the module's own stated "state is an optimization only" invariant. | [Orchestration §7](13-orchestration.md) |
| 9 | **Dead public API.** `cleanup::execute_pre_spawn_decision` and `cleanup::final_cleanup_proof` have no production callers (the private `final_cleanup_proof_roots` is the real authority); `cli::Cli::scan_root` and `cli::absolutize_root_for_test` have no production callers; `traverse::source_activity` is test-only. | [Cleanup §7](09-cleanup.md), [CLI §5](02-cli.md), [Traverse §6](08-traverse.md) |
| 10 | **A reachable panic in `traverse::measure_many_targets`**: duplicate `usize` indices are forwarded into `dua_core::walk_roots`, which asserts uniqueness. Latent today because the only production caller uses `.enumerate()`. | [Traverse §8](08-traverse.md) |
| 11 | **`config.toml` says the ignore/unignore filters apply "only to global discovery"; the code applies them to Routine too.** | [Policy §4](04-policy-and-scope.md) |
| 12 | **ADR 001 describes symlinked output roots as producing an "inventory diagnostic"; the code makes them fully inert — no group and no diagnostic.** | [Workspace §9](07-workspace.md) |
| 13 | **The uncertainty contract is enforced by two different thresholds.** `main.rs` extracts three diagnostic categories into `uncertainty`, but `cleanup.rs` treats *any* non-empty diagnostic vector from discovery as a hard block. Stricter in one place, looser in another, on the same signal. | [Discovery §6](05-discovery.md), [Cleanup §9](09-cleanup.md) |
| 14 | **`discovery_state` uncertainty is a per-root retention veto, not a publication gate.** A root that is both an ancestor of a fresh observation *and* intersecting uncertainty is dropped by the `covered` skip before the uncertainty test is reached. | [Discovery state §5](06-discovery-state.md) |

---

## 8. How the code got this way

The shape above is not accidental. `plans/` holds the decision record — 2
accepted ADRs, ~30 milestone plans, and a matching closure record for each. Two
are load-bearing for the architecture:

- **ADR 001 — workspace output ownership and cleanup authorization.** Defines the
  ownership classification that invariant 1 is built on.
- **ADR 002 — adaptive Routine/Full discovery.** Defines the two scan modes, the
  learned-root state file, and the uncertainty rule in invariant 4.

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

## 9. Index of deep dives

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

## 10. Reading order

For a first pass over the system:

1. This file.
2. [Domain & errors](01-domain-and-errors.md) — the vocabulary everything else speaks.
3. [Orchestration](13-orchestration.md) — the two pipelines end to end.
4. [Cleanup](09-cleanup.md) — where the safety invariants actually live.
5. [Workspace](07-workspace.md) and [Discovery](05-discovery.md) — how candidates are found and grouped.
6. §7 above, then the deep dive for whichever area you are changing.
7. Everything else as needed.
