# C035 — Operator-Declared Independent Maintenance Scopes (Route B)

Status: **implemented** on branch `plans/c035-operator-scoped-cleanup`
(baseline `6bad266`); publication pending — no release authorized by C035.
Closure: `plans/closure/artifact-discovery-cleanup/c035-status.md`.
Planning branch: `plans/c035-operator-scoped-cleanup`.
Baseline: `6bad266` (`main`, v0.2.6 + release record, 2026-10-14).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects forward: C031 (research baseline that retained the whole-scope
block), C034-D (the blocked destructive-partition gate), ADR 004 (proposed).
No closed plan or closure record is edited; C031's negative result stands as
the reason automatic partitioning is out of scope here.
Hard dependency: C034 safe track (existing-proof qualification) for the
engine this plan reuses unchanged inside each scope.
Skills: planning-and-closure; cleanup-safety; discovery-and-ownership;
test-evidence; json-and-exit-contract.

## 1. Incident and why a new plan is needed

On a real developer Mac (2026-10-14, v0.2.6 debug build), bare Routine
Simulate reported:

```text
combined scope: 7 root(s), 1253 manifest(s), 499 resolved workspace(s),
0 CleanupUnit(s), 288 unresolved ownership participant(s)
cleanup ownership could not be proven: 288 discovered Cargo manifests did not
resolve; no cleanup commands were run
simulation: 0 would-clean … exit 1
```

while read-only `scan` on the same machine inventoried **48 inactive groups,
70.22 GiB**. The 288 are locate-stage failures in unrelated subtrees (helix
grammar `Cargo.toml`s with unparsable `edition` keys, Go-module testdata,
workspace-mismatched fuzz crates). Even `/Users/davidbowman/projects` alone
is blocked (672 manifests, 219 resolved, 30 unresolved: codegg worktree fuzz
crates, a broken rustwaf2 workspace, synvoid third-party manifests). One
narrow explicit root (`clean --dry-run /Users/davidbowman/projects/eggpack`:
13 manifests, 4 resolved, 1 CleanupUnit, exit 0) proves the engine works —
the product failure is scope granularity, not resolution.

A workaround attempt is itself evidence: eleven recursive `[scan] ignore`
patterns in a `--config` file changed nothing (289 unresolved before and
after), because cleanup hardcodes
`discovery_filters: DiscoveryFilters::Bypassed` (`src/cleanup.rs:995`)
and explicit roots bypass filters by design (`src/policy.rs:42`). Per-unit
`--exclude` is evaluated inside the unit loop, which the whole-scope block
(`src/cleanup.rs:1132-1148`) never reaches. **There is no operator knob that
unblocks a poisoned scope today.** That is the defect this plan corrects, and
why previous verification missed it is recorded in §2.

## 2. Corrective-pass fields

1. **Original plans/records corrected:** C031 (whole-scope block retained,
   §D research), C034-D (partition gate), ADR 004 (proposed, unaccepted).
2. **Unclosed requirement:** bare `cargo cleanme` must be practical on a
   heterogeneous developer machine: poison in one subtree must block only
   that subtree's scope, never the whole machine.
3. **Why previous verification missed it:** C031's fixtures asserted
   known-good scopes and zero-spawn whole-scope blocks (C031 §1: "a product
   expectation versus accepted safety design gap, not a test that forgot a
   working outcome"); C034's positive control uses an isolated staged
   `$HOME/projects` with no poison. No test ever put a healthy project and a
   broken manifest in the *same default Routine invocation* and required a
   partial clean. The ignore-bypass interaction above (`cleanup.rs:995` vs
   operator expectation) was never a test case.
4. **Regression evidence required:** §6, items 1–8. Every new test must name
   the input that makes it fail (test-evidence skill): the per-scope block
   tests fail against current `main` by exhibiting whole-scope zero-unit
   reports.

## 3. Decision: what "accept ADR 004" means here

C031-D's negative result stands: **automatic** independence certificates are
rejected — filesystem disjointness is not proof, an unresolved manifest
supplies no authoritative output set, and Cargo/env redirection plus late
appearance defeat any static partition. This plan does **not** reopen that.

What is accepted instead is **operator-declared independence**: the operator
names authoritative maintenance scopes in configuration; each scope receives
the *complete* combined-universe proof — frozen roots, rebuilt discovery
inside the boundary (ADR 004 §5), full manifest coverage, hoisted-universe
refresh, per-spawn revalidation — identical gates to today's explicit
`clean ROOT`, which C034 §D already concedes is authoritative within its
boundary. The operator's declaration is the independence assertion, recorded
as such; the tool enforces proof within each boundary and stays fail-closed
on everything provable. Cross-scope hazard model:

- A resolved workspace whose Cargo output leaves its scope is
  `ExternalUnproven` → unit skipped (existing classification, unchanged).
- Two workspaces in different scopes resolving to overlapping output → each
  scope's grouping sees one owner plus an outside claim → not
  `PrivateBounded` → skipped. Per-scope grouping must therefore treat
  out-of-scope resolved output as an external claim, never as absence.
- An *unresolvable* manifest's unknown output remains the irreducible
  residual risk (C031-D): disclosed as a known limitation (§7), carried by
  the operator's declaration exactly as `allowed_output_roots` carries
  boundary authorization today. Authorization never manufactures proof; the
  declaration narrows *where proof is attempted*, never *what counts as
  proven*.

Out of scope, explicitly: automatic per-root fallback after combined failure
(prohibited, ADR 004 §"Alternatives"); in-scope ignore/exclusion pruning
before proof (universe-narrowing — rejected for v1 with rationale in §7;
revisit only as a separate risk-acceptance decision, never silently);
`clean --full` semantics; learned-state promotion (state stays advisory per
AGENTS.md §7).

## 4. Objectives and hard invariants

Bare `cargo cleanme` with declared scopes cleans every provable scope and
reports a typed per-scope block for each poisoned one, in one invocation,
all modes. Unchanged invariants: Cargo-mediated deletion only, one atomic
workspace CleanupUnit over its complete OutputSet, `Shared`/`Uncertain`/
`ExternalUnproven` inventory-only forever, authorization never manufactures
proof, Simulate spawns no Cargo clean, pre-spawn revalidation per scope,
exit/JSON/log contract discipline (§5).

## 5. Ordered work packages

### A. ADR record (ready, no code)

Append a dated "C035 clarification" to ADR 001 beside the C003/C004
clarifications: operator-declared scopes are independent proof boundaries;
C004/C006 completeness applies *within* each boundary. Flip ADR 004 to
**accepted-in-part** (operator-declared scopes; automatic certificates stay
rejected, citing C031-D). This is an intentional product change by the owner,
the one case AGENTS.md permits touching canonical direction — record the
owner decision and date in both files.

### B. Configuration: declared scopes (after A)

New additive `[maintenance]` section: `scopes = [...]` — exact absolute
directories (precedent: `allowed_output_roots` exact roots, `unignore`
literal paths; NOT globs — glob boundaries reintroduce matching ambiguity).
Validation at load: absolute, real directory, non-symlink (mirror explicit
root validation, `src/policy.rs:32-41`); overlapping/duplicate declarations
are containment-collapsed into one scope (mirror `collapse_roots`), never
silently dropped. Empty/missing section = today's behavior byte-for-byte
(this is the rollback: delete the section). Config-compat assessment
required; additive optional section must not need a state/schema bump — prove
with a load test on a v0.2.6 config.

### C. Policy: scope resolution (after B)

When scopes are declared, bare/`--known` Routine resolves to exactly the
declared set (declared wins over seeds/learned, same precedence shape as
`cli_root.or_else(config.root)` at `src/policy.rs:30`); seeds/learned remain
scan inputs and the fallback when undeclared. Each declared scope resolves
with `DiscoveryFilters::Bypassed` *within its boundary* (explicit-root
semantics, `src/policy.rs:42`) — declared means it. Unit tests for
precedence, collapse, and the undeclared-fallback identity.

### D. Engine: per-scope proof (after C, hard-depends C034 safe track)

Refactor `clean_with_roots_policy_selector_guarded` (`src/cleanup.rs:956`)
to iterate declared scopes: per scope, freeze roots → rebuild discovery
inside the boundary → `resolve_cleanup_scope` (`src/cleanup.rs:2021`, whose
unresolved-early-return at `:2042-2048` becomes per-scope) → per-scope
hoisted `ProofUniverse` refresh (`src/cleanup.rs:2428-2465`, scoped roots in
place of combined roots) → per-unit `ExecutionProof` unchanged. The
admission-premise recheck and H-2 manifest-growth gate run per scope.
Kill-switch: any internal partitioning error (unattributable diagnostic,
manifest, workspace, or participant) falls back to the legacy single
combined-scope run with a stderr note — the failure mode is today's
behavior, never a silent partial clean. No new CLI flag: the operator-level
kill-switch is deleting the `[maintenance]` section, which restores the
legacy path exactly (rollback identity test). Simulate/Preview/Execute agree per
scope; Simulate still spawns nothing anywhere.

### E. Reporting and contracts (with D)

Human: per-scope result groups; a scope blocked by unresolved ownership
names the manifest and stage (C034-B diagnosis, using C032 cause chains).
JSON: additive per-scope breakdown; top-level `scope_blocked` stays true if
*any* scope is blocked (fail-closed aggregation, no new status value —
assess against `plans/output-schema-v1.md`; if a `partial` status is needed
it is a versioned migration with its own decision, not a quiet addition).
`--format log` stays one ≤384-byte ASCII line (aggregate worst reason; exact
rule is an open product detail to close in review). Exit codes: 0 all clean,
1 any scope blocked or unit failed (C031 §E recommendation), 2 usage —
pin with `tests/cli_contract.rs` subprocess cases (exit codes are only
observable out-of-process, test-evidence skill).

### F. Docs and generated artifacts (with E)

`docs/USAGE.md`, `TROUBLESHOOTING.md` (poisoned-scope diagnosis → declare
scopes workflow), `architecture/04-policy-and-scope.md`,
`architecture/09-cleanup.md`, `architecture/13-orchestration.md`,
`architecture/10-reporting.md` as semantics change; re-derive every touched
`file.rs:N` citation from source (no arithmetic shifts); `generate-docs` if
and only if CLI surface changes (none planned).

## 6. Required regression/evidence matrix

1. **Partial clean:** two declared scopes, one healthy inactive private
   workspace + one scope containing an unresolvable manifest; Execute cleans
   exactly the healthy scope (real `cargo clean` argv asserted, bytes
   reclaimed, exit 1 with typed per-scope block for the other). Fails on
   current `main` as whole-scope zero-unit.
2. **Simulation parity:** same unit/block set under `--dry-run`, zero Cargo
   clean spawns, no byte mutation.
3. **Cross-scope redirect:** healthy A with `CARGO_TARGET_DIR` into scope B →
   A skipped `ExternalUnproven`, nothing spawns. The control that keeps Route
   B honest.
4. **Overlap:** scopes resolving to shared output → both sides skip, never
   `PrivateBounded`.
5. **Late appearance/config mutation:** manifest or env change between scope
   proof and spawn re-blocks that scope; sibling scope results stand.
6. **Declaration hygiene:** overlapping declarations merge; symlink/non-dir
   declaration is a load-time fatal (exit 17 path, not a silent skip);
   undeclared config behaves exactly as v0.2.6 (rollback identity test).
7. **Contract:** one JSON object, additive fields only, log bound held,
   exit-code pins, human/JSON parity on scope blocks.
8. **Heterogeneity:** the §1 machine shape (healthy project + grammar/fuzz
   poison in another scope) in a disposable fixture with real Cargo —
   staged-plugin spelling and direct binary, non-TTY, Linux/macOS/Windows,
   MSRV 1.89, real non-root Linux run.

## 7. Known limitations and stop conditions

- The irreducible residual (§3): an unresolvable manifest's unknown output
  cannot be disproven; operator declaration carries it. Disclosed, not fixed.
- Poison *inside* a declared scope still blocks that scope — by design. The
  operator narrows declarations to clean subtrees; in-scope exclusions are
  rejected for v1 because excluding a manifest from discovery cannot exclude
  it from owning bytes.
- **Stop** for: any auto-narrowing after combined failure; any promotion of
  `Shared`/`Uncertain`/`ExternalUnproven`; unresolved manifests silently
  dropped; JSON/log schema drift without decision; direct filesystem
  deletion; `sudo`; permission-denied read as absence; a test green via an
  unrelated failure; deletion in fixtures outside disposable trees.
- Closure: `plans/closure/artifact-discovery-cleanup/c035-status.md`
  (requirement→evidence matrix, commands actually run, hosted runs,
  limitations); no release authorized here — publication is a separate gate.

## 8. Handoff

Hard dependency on C034 safe track; C032 publication-pending and C033/ADR 005
proceed independently. Owner review of §§3/5-E open details is resolved as
follows (owner direction 2026-10-14: proceed on plan defaults):

- **Log-line aggregation rule: closed.** First block in sorted scope order
  wins; the existing `log::cleanup` renderer needs no change (blocked +
  reason code, prose never emitted). Deterministic, no new code path.
- **Declaration-merge vs block: confirmed as merge.** Overlapping/duplicate
  declarations containment-collapse into one scope, never silently dropped.
- **Correction — §5-B/§6 item 6 "exit 17": wrong.** There is no exit 17 in
  the 0/1/2 contract (exit 17 was a `cargo clean` subprocess status in a
  test fix, not our code). Declaration-hygiene failure is a load-time fatal
  `AppError::InvalidRoot` → **exit 2**, never a silent skip. Strict
  existence/type checks run at admission (explicit-root precedent), not in
  `config::load` (which checks absoluteness only, like `scan.root`); every
  invocation including `config show` must survive an unmounted scope disk.
- **`scan.root` precedence: closed.** A configured `scan.root` stays the
  exclusive Explicit override and wins over declared scopes, with a stderr
  warning when both are set. Declared scopes win over seeds/learned only.
- **JSON breakdown shape: closed.** Per-scope entries carry counts, block
  state, reason, and unresolved participants; no per-scope units array in
  v1 (top-level `units` stays the full list). Key emitted only when scopes
  are declared (`skip_serializing_if`), so undeclared JSON is byte-identical.
- **Analyze-phase diagnostics are global.** Discovery diagnostics attribute
  per scope by path; diagnostics from combined resolution/analysis (and any
  unattributable diagnostic) count at the merged top level. Unattributable
  *discovery* diagnostics fall back to the legacy combined run (kill-switch).
- **`policy::resolve` stays untouched.** Declared-wins resolution lives in
  the `resolve_cleanup_roots` Maintenance arm; the partition helper lives in
  `policy.rs` beside `collapse` precedent. No new `ScanScope` variant: the
  engine reuses `ExplicitRoots` per scope and for the combined union.
- **Proof threading (source study):** `final_cleanup_proof_roots` looks up
  the candidate by workspace identity, but the package-selector check indexes
  `unit.workspace_idx` — so the unit loop keeps the combined workspaces
  slice for indexing while the hoisted universe and proof take the in-scope
  slice. `resolve_cleanup_scope` itself is not modified (its unresolved
  early-return feeds legacy block reports); the scoped driver replicates its
  tail (~25 lines) to build units from resolved workspaces despite
  out-of-scope unresolved participants.

Next: implement B→F in order, then the §6 matrix, ladder, and
`c035-status.md`. No release authorized here.
