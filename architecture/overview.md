# cargo-cleanme — Architecture Overview

A birds-eye view of how the crate is put together, and an index into the
per-component deep dives that follow.

- **What it is:** a Cargo subcommand (`cargo cleanme`) that finds inactive Cargo
  build artifacts on a machine and either reports or safely reclaims them.
- **Shape:** one binary (`src/main.rs`) over one library (`src/lib.rs`) of 15
  modules, 17 source files, 23,468 lines of Rust in `src/`, no async runtime
  of its own, and exactly one binary dependency that matters for safety
  (`cargo metadata`).
- **Version:** 0.2.3 (published) · **Edition:** 2024 · **MSRV:** 1.89
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
module, the `prod` half of the split is given. The crate has **320 inline
`#[test]` functions**, and **40.3% of them live in the two largest modules**
(`cleanup.rs` 72 + `workspace.rs` 57). Three modules have **zero** tests:
`main.rs`, `error.rs`, `lib.rs`.

Test counts in this table are **declared**, not "how many ran on my machine",
and both the counts and the line counts are re-derived from `src/` rather than
carried forward. A count copied between documents goes stale silently, and this
file is the source of truth the deep dives align to. Read the total as coverage
concentrated in two modules, not as a balance — see
[14-testing-and-verification](14-testing-and-verification.md) §2.

**`main.rs` still has no tests, and C023 established that this is not a
neutral fact.** The cleanup progress bar is cleared *before* the fallible
cleanup call in `main.rs`, and that ordering is unreachable from
`src/progress.rs`: reverting it leaves every test green, because the bar only
draws on an attended terminal and the failing-cleanup contract test passes
`--no-progress`. Pinning it needs a pty harness. It is recorded as an open
coverage gap in
[`c023-status.md`](../plans/closure/distribution-release-update/c023-status.md),
not as a claim that the ordering is verified.

| Module | Lines | Role | Deep dive |
|---|---:|---|---|
| [`main.rs`](13-orchestration.md) | 782 | Binary entry: argv → resolved invocation → dispatch → exit code. The only place the scan and cleanup pipelines are stitched together. | [Orchestration](13-orchestration.md) |
| [`cli.rs`](02-cli.md) | 1025 / 398 | Clap surface, resolved-invocation model, `cargo`-subcommand argv normalization, scan-root absolutization. | [CLI](02-cli.md) |
| [`config.rs`](03-config-and-editor.md) | 1443 / 575 | Load/create/validate `config.toml`; embedded template; path resolution. | [Config & editor](03-config-and-editor.md) |
| [`policy.rs`](04-policy-and-scope.md) | 464 / 285 | Turns a `ScanRequest` + config into an `EffectiveScanPolicy` — the Routine/Full scope decision. | [Policy & scope](04-policy-and-scope.md) |
| [`discovery.rs`](05-discovery.md) | 2079 / 916 | Finds `Cargo.toml` manifests across a bounded walk, with attribution for what was pruned. | [Discovery](05-discovery.md) |
| [`discovery_state.rs`](06-discovery-state.md) | 702 / 405 | Persisted "learned roots", uncertainty-aware reconciliation, atomic publish. | [Discovery state](06-discovery-state.md) |
| [`workspace.rs`](07-workspace.md) | 4517 / 1524 | `cargo metadata` resolution, capability probing, physical grouping, cleanup-unit construction. | [Workspace](07-workspace.md) |
| [`traverse.rs`](08-traverse.md) | 624 / 494 | Parallel size/recency measurement and source-activity classification. | [Traverse](08-traverse.md) |
| [`cleanup.rs`](09-cleanup.md) | 6426 / 2411 | Authorization, ownership proof, pre-spawn decision, three clean modes. The heart of the safety story. | [Cleanup](09-cleanup.md) |
| [`report.rs`](10-reporting.md) | 195 / 71 | Human-readable rendering of a `ScanReport`; byte formatting. | [Reporting](10-reporting.md) |
| [`output.rs`](10-reporting.md) | 941 | Versioned machine-readable DTOs (`EnvelopeV1`); the stable JSON contract, including the non-envelope `update` document and its stdout stream; the bounded `--format log` line renderer. | [Reporting](10-reporting.md) |
| [`progress.rs`](11-progress.md) | 840 / 508 | `ProgressObserver` trait + indicatif renderer; terminal capability detection. | [Progress](11-progress.md) |
| [`update.rs`](12-self-update.md) | 2773 / 1234 | Eggup-based self-update: provenance classification, version authority, staged replace. | [Self-update](12-self-update.md) |
| [`domain.rs`](01-domain-and-errors.md) | 411 | The shared vocabulary: reports, ownership classes, counters, diagnostics. Pure data. | [Domain & errors](01-domain-and-errors.md) |
| [`error.rs`](01-domain-and-errors.md) | 16 | `AppError` — the six variants the binary can fail with. | [Domain & errors](01-domain-and-errors.md) |
| [`lib.rs`](01-domain-and-errors.md) | 15 | Module manifest. | [Domain & errors](01-domain-and-errors.md) |
| [`editor.rs`](03-config-and-editor.md) | 215 / 155 | Resolves `$EDITOR`/`$VISUAL` for `config edit`. | [Config & editor](03-config-and-editor.md) |

Supporting surfaces, outside `src/`:

| Surface | Deep dive |
|---|---|
| `tests/` (2 suites + shared harness), `scripts/` (17 scripts), `.github/workflows/ci.yml` | [Testing & verification](14-testing-and-verification.md) |
| `packaging/`, `completions/`, `man/`, `release/`, `xtask/`, the 6 GitHub workflows | [Distribution & release](15-distribution-and-release.md) |
| `plans/`, `examples/`, `docs/` — decision records, qualification harness, user docs | referenced from the relevant deep dives |

Note that `architecture/` is itself outside the `Cargo.toml` `include`
allowlist, so none of this documentation ships in the published crate.

---

## 4. The scan pipeline

`run_scan` in `main.rs:485` is the read-only pipeline. Nine steps, in order:

1. **Resolve config** — `config::load_or_create` (`main.rs:496`).
2. **Resolve scope** — `policy::resolve` turns `{cli_root, full}` into concrete
   roots + a recency window (`main.rs:506`).
3. **Discover manifests** — `discovery::discover_manifests_with_attribution`
   walks the scope for `Cargo.toml`, recording *why* anything was skipped
   (`main.rs:527`).
4. **Partition uncertainty** — permission-denied / metadata / vanished
   diagnostics are pulled out as `uncertainty` (`main.rs:537`). This vector
   later decides whether learned state may be trusted.
5. **Resolve workspaces** — `workspace::resolve_workspaces` shells out to
   `cargo metadata` once per unique canonical workspace root (`main.rs:557`).
6. **Reconcile state** — for a Full scan, `discovery_state::reconcile_full`
   folds observations into the learned-root set and publishes atomically
   (`main.rs:596`, publishing at `main.rs:608`).
7. **Build physical groups** — `workspace::build_groups`; nested/duplicate
   output roots collapse to one group so bytes are counted once (`main.rs:654`).
8. **Analyze** — `workspace::analyze_groups` measures each group in parallel
   via `traverse` and classifies activity (`main.rs:662`).
9. **Render** — human (`report::render`, `main.rs:721`) or JSON
   (`output::scan`, `main.rs:709`).

Steps 3–5 and 7–8 are the expensive parts; step 6 is the part that makes the
next run cheap.

## 5. The cleanup pipeline

`clean` is not a scan plus a `rm`. It is a separate, guarded transaction. The
modes are `CleanMode` in `cleanup.rs:36`, mapped from flags by
`cli::clean_mode` (`cli.rs:224`):

| Mode | Flag | What it does |
|---|---|---|
| `Execute` | default | Proof, then actually removes through Cargo. |
| `Simulate` | `--dry-run` (hidden alias `--dryrun`) | Full ownership proof + selection, with no `cargo clean` process of any kind spawned. |
| `CargoPreview` | `--cargo-preview` | The same final proof, then Cargo's own `clean --dry-run --verbose`. Cargo *is* spawned here; simulation is the mode that spawns nothing. |

Note the deliberate spelling split. Since ADR 003, canonical `--dry-run` means
**simulation** and Cargo's own preview moved to `--cargo-preview`; `--dryrun`
is a hidden compatibility alias selecting the same simulation mode
(`cli.rs:224-237`).
Deletion itself is always performed by `cargo clean`, never by direct
filesystem removal, so Cargo's own bookkeeping cannot be bypassed.

Cleanup roots come from three sources, resolved by `resolve_cleanup_roots`
(`main.rs:314`): an explicit `ROOT` argument, `--full` (runs a Full scan, then
uses learned roots), or `--known` (uses only the Routine scope). They are
canonicalized and de-duplicated by `collapse_roots` (`main.rs:440`) so nested
roots cannot produce overlapping deletions.

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
   (`cleanup.rs:544-546`). See [Cleanup §5](09-cleanup.md).
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

## 7. Findings from the deep dives, and their disposition

The deep dives surfaced the items below. Each was then **verified against the
source before any change was made** — three did not survive that check, and one
was materially misdescribed. Status is current as of the commit that fixed
items 1, 7, 8, 10 and 11.

Items 3-6 are re-verified as of the Phase-11 and Phase-12 closures (all of
C018/C019/C021-C025 are now closed). Their findings were real when recorded and
each is now closed by shipped code; the deep dives that carry them were updated in
the same pass, so these rows point at completed work rather than at a plan.

| # | Finding | Status |
|---|---|---|
| 1 | **JSON `scope` label was derived from CLI flags, not the resolved policy.** A scan configured with `scan.root` resolves to `ScanScope::Explicit` but reported `"routine"`. | **Fixed.** Now derived from `policy.scope` via `main::scope_label`, with a regression test that fails against the old code. |
| 2 | **A failed ownership proof at cleanup time is recorded as a per-candidate skip, so the process can exit `0`.** | **Kept deliberately.** Per-candidate ineligibility is a legitimate skip with a typed reason code; the scan-time path at `cleanup.rs:846` still blocks. See the correction below. |
| 3 | **Self-update provenance classification fails *open* in four paths.** | **Closed (C018).** Fail-closed typed provenance shipped: manager-evidence ambiguity is gone and real self-managed installs are preserved. Every published release from 0.1.6 onward refuses without remote acquisition or mutation when local provenance forbids replacement. |
| 4 | **Release integrity rests on HTTPS plus a self-published checksum — no signature verification anywhere.** | **Closed for the planned scope (M011A).** Immutable-release attestation and verification shipped, enabled, and verified live. Independent signing/SLSA was deliberately deferred and remains an open Eggpack trust-model decision; see [Distribution §7.5](15-distribution-and-release.md). |
| 5 | **The live self-update rehearsal is manual** (`workflow_dispatch`-only), and it is the only thing that has ever caught a self-update defect.** | **Closed (M011C).** Automatic on `release: published`, with deterministic predecessor selection and a bounded crates.io-authority wait. Demonstrated four times; run `37561575727` (v0.2.2) was the first green automatic five-lane run and `37652257315` (v0.2.3) is the fourth. `workflow_dispatch` remains the manual recovery path, and [Distribution §8.2](15-distribution-and-release.md) is the historical record of what that gap used to cost. |
| 6 | **Two `scripts/` files are wired into nothing** — `validate-staged-release.py` (the sole "published bytes == qualified bytes" proof) and `qualify-cargo-selectors.sh`.** | **Closed (M011B, M011D).** Both are now wired: `validate-staged-release.py` runs automatically on a real Eggpack staged draft before human publication, and `qualify-cargo-selectors.sh` runs on a recurring hosted real-Cargo matrix. Neither was forced into ordinary PR execution. |
| 7 | **Ten divergences between `plans/output-schema-v1.md` and the code, in both directions.** | **Fixed.** The code is authoritative; the schema document was corrected — `selector_estimate_bytes` nullity, the 19th `reason_code`, the 6th policy disposition, `effective_policy`'s four fields, `units[].detail` always-present, the `outcome` value set, `mode` always emitted, and the scan exit-code case. |
| 8 | **Exit codes responded to only some stderr warnings**, and a failed discovery-state save contradicted the module's own "state is an optimization only" invariant. | **Fixed** for the state-save case: it no longer changes the exit code, which also makes the Full and Routine branches agree. The remaining warning/exit-code asymmetry is documented, not changed. |
| 9 | **Dead public API** in a published crate. | **Kept, documented** in §7.1 below. |
| 10 | **A reachable panic in `traverse::measure_many_targets`**: duplicate indices are forwarded into `dua_core::walk_roots`, whose `assert_eq!` is not a debug assert. | **Fixed.** A repeated index is now measured once and every occurrence of that index reports the same measurement, with a unit test. |
| 11 | **`config.toml` said the ignore/unignore filters apply "only to global discovery"; the code applies them to Routine too.** | **Fixed.** The template comment now states the real rule: filters apply to global and Routine, and an explicit root bypasses them. |
| 12 | **ADR 001 describes symlinked output roots as producing an "inventory diagnostic"; the code makes them inert — no group and no diagnostic.** | **Open.** Adding a diagnostic is not a free change: `cleanup.rs:846` blocks on *any* non-empty discovery diagnostic, so a new one here widens the block condition. |
| 13 | **The uncertainty contract is enforced at two different thresholds** — `main.rs` extracts three diagnostic categories, `cleanup.rs` blocks on any diagnostic. | **Open, and arguably intentional:** the cleanup path being stricter is fail-closed. Recorded because the asymmetry is invisible from either module alone. |
| 14 | **`discovery_state`'s `covered` skip can drop a root that also intersects uncertainty, before the uncertainty veto is reached.** | **Not a bug — checked and dismissed.** The collapse always replaces an ancestor with a *descendant*, so the remembered set only ever narrows. Narrowing means less cleanup on the next run, which is fail-safe. |

### 7.1 Dead public API, recorded not removed

`cargo_cleanme` is published on crates.io, so dropping `pub` is a semver-visible
change. These items have no production callers and are kept deliberately:

| Item | State |
|---|---|
| `cleanup::execute_pre_spawn_decision` (~200 lines) | Zero callers, including tests. The real pre-spawn authority is the private `cleanup::final_cleanup_proof_roots`, which refreshes more strictly. |
| `cleanup::final_cleanup_proof` | Reached only by the dead function above and one test. Correct in its own right — it builds its own universe. |
| `cli::absolutize_root_for_test` | Zero call sites anywhere, despite the name promising otherwise. |
| `cli::Cli::scan_root` | Test-only; `main.rs` destructures `Command::Scan` directly. |
| `traverse::source_activity` | Test-only; production uses `workspace_member_activity`. |

### 7.2 Corrections verification made to the original findings

Recorded because the original wording was wrong and would mislead a future
reviewer:

- **Item 1 was misdescribed.** The claim that `tests/end_to_end.rs:202` "encodes
  the bug as expected" is false: that test calls `output::scan(&report, "routine")`
  with a hand-supplied label to exercise the DTO projection. It never touches
  `main.rs`'s derivation, so the label logic was simply **untested** — which is
  why it was wrong, and why the fix adds a test rather than correcting one.
- **Item 2's mechanism was unreachable.** The reported cause — a shared hoisted
  universe failing via `hoisted.get()?` — cannot occur: the universe is
  initialised immediately before the call that consumes it (`cleanup.rs:1049`).
  The real path is the per-candidate skip, where a `ProofFailure` becomes a
  `skipped` row and the unit is abandoned (`cleanup.rs:1061-1064`).
- **Item 3 is a documented decision, not an oversight**, and the doc comment
  states the reasoning explicitly.

## 8. How the code got this way

The shape above is not accidental. `plans/` holds the decision record — 3
accepted ADRs, ~30 milestone plans, and a matching closure record for each. All
three are load-bearing for the architecture:

- **ADR 001 — workspace output ownership and cleanup authorization.** Defines the
  ownership classification that invariant 1 is built on.
- **ADR 002 — adaptive Routine/Full discovery.** Defines the two scan modes, the
  learned-root state file, and the uncertainty rule in invariant 4.
- **ADR 003 — canonical maintenance invocation and unattended output.** Defines
  the current front door: bare `cargo cleanme` is Routine *Execute*, `--dry-run`
  is the zero-spawn simulation preview, and the updater's JSON output is the
  unattended surface. Read it before changing anything in §5's mode table, or you
  will be reasoning from the pre-0.2.0 CLI.

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
