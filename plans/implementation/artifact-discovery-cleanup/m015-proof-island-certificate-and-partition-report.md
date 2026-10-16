# M015 — Proof-Island Certificate Design and Partition Reporting (no destructive change)

Status: **conditionally closed** — see `plans/closure/artifact-discovery-cleanup/m015-status.md`.
Implementation, local verification, and live-data collection are complete;
hosted OS/MSRV lanes and VCS landing are outstanding. No publication is
required (zero disposition change). Research + reporting milestone only. It designs the
independence certificate ADR 004 requires, computes the partition it implies,
and reports it — without changing any Execute/Simulate/Preview disposition.
Destructive activation is explicitly **not** this milestone; it is a named,
blocked successor (M016, §12) gated on ADR 004 acceptance.

Baseline: `0a5a6e6` (`main`, release 0.2.8).
Roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Proposed decision under test: `plans/adr/004-proposed-bounded-independent-cleanup-proof-islands.md`
(remains **proposed**; this milestone neither accepts it nor amends ADR 001).
Corrects nothing; extends C031/C034 evidence. M014's cross-root redirect
control is reused as a negative fixture.
Skills: planning-and-closure; cleanup-safety; discovery-and-ownership;
test-evidence; json-and-exit-contract (review-only: no contract change ships).

## 1. Objective and non-goals

Objective: answer ADR 004's question to decide — *"what constitutes
independently proven non-overlap?"* — with a certificate definition, a
partition computation over the existing combined universe, per-island
dispositions shown in human reporting, negative controls proving hostile cases
still merge or block, and live data on how often real runs would actually
partition. All of it observable with zero change to what any mode cleans.

Non-goals (explicit; these belong to blocked M016, §12):

- No new CLI flag that cleans anything. No change to `scope_blocked`,
  `resolve_cleanup_scope`'s early return, unit dispositions, exit codes, or the
  JSON envelope.
- No acceptance of ADR 004 and no amendment to ADR 001 C004/C006. The
  whole-scope block stays binding throughout.
- No second deletion path, no direct filesystem deletion, no Cargo-config
  reimplementation (forbidden input to any attribution rule, §5).

## 2. Current implementation evidence

The block this milestone studies, verified against `0a5a6e6` plus the
in-flight working tree noted below:

- `cleanup::resolve_cleanup_scope` returns `units: Vec::new()` while any
  `coverage.unresolved` entry remains, so no unit even exists
  (`src/cleanup.rs`, `resolve_cleanup_scope` tail). The engine then reports
  `ScopeBlockReason::OwnershipUnproven` with zero results and zero spawns in
  all three modes (`clean_with_roots_policy_selector_guarded`).
- Discovery diagnostics do the same one step earlier
  (`ScopeBlockReason::IncompleteDiscovery`).
- `list_cleanup_candidates` applies the identical gates to `-i`, so the
  interactive session refuses to present a selection from an unproven universe
  (M014, ADR 006).
- Per-candidate final proof already reasons about unresolvable roots: no other
  workspace in the fresh universe may overlap the candidate's covering union in
  either direction, including symlink/unresolvable roots via
  `resolve_as_far_as_possible` (`architecture/09-cleanup.md` §6, proof step 7).
  The scan-time block is therefore strictly more conservative than the
  proof-time machinery — that gap is exactly what a certificate must justify
  closing, per island, with evidence.
- Pinned behavior that must keep passing unchanged: `PerPathFailRunner`
  (one unresolvable manifest blocks a clean sibling),
  `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes`,
  `combined_roots_classify_cross_root_output_as_shared_in_every_mode`.
- M014's measured hostile control (two workspaces, `other` redirecting output
  into `ws/target` through `CARGO_TARGET_DIR`, shared at full width, private
  at narrowed width) is the reuse-ready cross-root redirect fixture.

In-flight, uncommitted working-tree delta at plan time (anchor listing,
`blocked_listing_detail`, believed-workspace pass-through in
`src/{main,cleanup,cli,workspace}.rs` + two architecture dives): M015's
implementer must first give it a disposition — commit as its own corrective
with tests green, or re-derive it — and record which. M015 packages that touch
`resolve_workspaces_with_coverage` build on whatever lands; the plan does not
assume its content beyond the pattern it establishes (class attribution with
positive Cargo-emitted evidence).

## 3. Invariants (unchanged; violated by no work package)

Ownership proof not timestamps; authorization never manufactures proof;
re-validation at the last moment per spawn; fail closed on uncertainty; the
destructive unit is one workspace `CleanupUnit` over its complete `OutputSet`;
deletion is always `cargo clean`; unresolved ownership blocks the whole scope
for every mode (still binding — M015 reports partitions, it does not enforce
them).

## 4. The certificate to design (work package A)

A proof island is a subset of the selected scope carrying four attached
proofs. The plan requires the implementer to write this definition down
before code, because ADR 004 §"Proposed conditional direction" items 2–4
stand or fall on its exactness:

1. **Component completeness.** Every manifest discovered under the island's
   roots is resolved or attributed *inside* the island, or the island reports
   blocked. Same rule as today, smaller boundary.
2. **Resolved non-overlap.** No resolved workspace outside the island shares
   or canonically overlaps island outputs. Computed from the *combined* graph
   the engine already builds — M015 keeps combined classification and only
   partitions the blocking effect, so M014's shared-stays-shared control keeps
   passing verbatim.
3. **Unknown quarantine.** Every unresolved manifest and every discovery
   diagnostic hole is assigned to exactly one island. An island holding an
   unassigned or unassignable unknown reports blocked. Assignment is allowed
   only by a failure-class attribution rule with stated positive evidence
   (§5); the default for every class is merge-with-everything (i.e. today's
   global block).
4. **Environment/config provenance snapshot.** The certificate records the
   process environment relevant to output placement (`CARGO_TARGET_DIR`,
   `CARGO_BUILD_BUILD_DIR`) and each island's per-workspace effective
   target/build provenance as Cargo reported it (metadata `target_directory`,
   `build_directory` capability — Cargo-emitted, never re-derived). Any
   cross-island redirect evidence merges the affected islands.

## 5. Failure-class table (work package B)

For each resolution/discovery failure class, M015 specifies exactly one of:
the attribution rule plus the evidence that makes it sound, or
merge-with-everything. Initial table (implementer extends only with evidence):

| Class | Attribution or merge |
|---|---|
| `believes-it's-in-a-workspace` with named workspace resolved in-run | Attribute to the named workspace's island (pattern established in-tree; Cargo names the owner, parent resolution proves the output location) |
| Same, named workspace not resolved here | Merge (parent's outputs unproven) |
| Locate transport / spawn / worker failure | Merge (nothing is known) |
| Locate parse failure, manifest otherwise present | Merge, unless a narrower Cargo-emitted rule is proven; file readability alone is not ownership evidence |
| Metadata failure / unparseable metadata | Merge |
| Identity (internal group-assignment failure) | Merge and file as defect — this edge must be unreachable |
| Discovery hole with bounded path (`Untraversable`/`Unreadable` with `path`) | Assign to the containing root's island; island blocked unless a class rule clears it (a hole is an unknown participant, ADR 004 item 2) |
| Discovery hole without path | Merge |

Hard input constraint from discovery-and-ownership: attribution rules may use
Cargo-emitted evidence (locate/metadata stdout/stderr, workspace member
lists) and filesystem identity facts (canonical paths) only. Reimplementing
Cargo's config merge to "compute" an unknown manifest's effective target-dir
is forbidden; any rule needing it is merge by construction.

## 6. Ordered work packages

- **A. Certificate definition.** §4 written as a reviewable spec with the four
  proofs, the merge-by-default rule, and the provenance snapshot format.
- **B. Failure-class table.** §5 implemented as a pure decision function over
  `(UnresolvedOwnershipParticipant, ResolutionCoverage, diagnostics)` with
  unit tests per row, including merge-default tests for every class.
- **C. Partition computation.** Union-find over resolved workspaces +
  assigned unknowns + bounded holes, with union edges from the combined output
  graph (shared/canonical-prefix overlap) and containment without attribution.
  Pure function; deterministic ordering; bounded size.
- **D. Partitionability report (human only).** When the engine raises a
  whole-scope block, the human report additionally shows the computed
  partition: per-island unit counts/bytes, which islands would proceed, which
  are blocked and by which named unknowns. Bounded like the existing blocked
  listing (at most 5 participant rows + overflow). No JSON field, no exit-code
  change — the contract review in M016 decides the machine surface with full
  context, not as a side effect here.
- **E. Disposition parity proof.** A test asserting that computing partitions
  changes nothing: Execute/Simulate/Preview results, reason codes, exit codes,
  and spawn counts identical with the computation on or off. This is the test
  that lets M015 land without ADR acceptance.
- **F. Negative controls (premise-negative).** Each control ships with a naive
  skip implementation it must fail against: cross-root `CARGO_TARGET_DIR`
  redirect (M014 fixture), unresolved-manifest overlap, symlink output into a
  clean island, env change between discovery and pre-spawn, target under
  another source tree, parallel-mutation TOCTOU. Every control asserts the
  hostile island merges or blocks with zero cross-island cleans. A control
  that passes against the naive skip is decoration — discard it (lesson:
  "what input makes this fail?").
- **G. Live-data collection.** Run the partition report over heterogeneous
  fixtures (cargo-clean-all-style mixed trees: caches, fixtures, worktrees,
  permission noise, one redirect pair) and record the island-hit rate: how
  often real-shaped runs would split into >1 island, and which classes block.
  This data is M016's go/no-go input and the check on stop condition S1.
- **H. Documentation.** Architecture deep-dive section for the certificate +
  partition computation (09-cleanup; 07-workspace for the class table),
  citations re-derived from source. ADR 004 itself is not edited (still
  proposed); M015's evidence lands in the closure record.

## 7. Failure, cancellation, and contention semantics

Partition computation is infallible by construction: any internal error
(unassigned unknown, unreachable edge) degrades to a single island — today's
block — never to a finer partition. Reporting failures (render errors) must
not alter the report's machine fields. Concurrency: packages C–E are pure
functions with no threads; fixture G runs sequentially unless the existing
bounded pools are reused without changing their caps.

## 8. Compatibility and configuration effects

None shipped. No flags, no config keys, no JSON fields, no exit-code changes,
no MSRV change. The human blocked-scope report gains bounded lines (same
treatment as the existing participant listing). M016's contract decisions
(opt-in flag name, partial-success exit code, envelope versioning) are
explicitly deferred, with the M015 live data attached as input.

## 9. Required tests

Per-class attribution unit tests with discriminating assertions (exact reason
codes / island assignments, never "the command failed"); union-find merge
tests including merge-by-default for every class; disposition-parity test (E);
all negative controls (F) with naive-skip failing direction; existing suite
green, notably the three whole-scope-block pins in §2.

## 10. Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
```

Plus hosted Linux/macOS/Windows + MSRV 1.89 lanes for the fixture-bearing
packages (F, G), since C021/C024 proved platform-only defects are the norm in
this area, not the exception.

## 11. Acceptance criteria and stop conditions

Acceptance: certificate spec reviewed; class table fully implemented per §5;
partition computation deterministic and bounded; human report shows islands on
blocked runs; parity test green (zero disposition change); every negative
control fails against naive skip and passes against the certificate; live-data
report (G) attached to the closure record; closure record
`plans/closure/artifact-discovery-cleanup/m015-status.md` written with
requirement-to-evidence matrix, commands actually run, and known limitations.

Stop conditions (any one ends the line, recorded as rejection, block retained):

- **S1.** The class table resolves to merge-by-default for every failure
  class observed live (permission holes, transports, metadata failures admit
  no sound attribution) — islands would never unblock a real run. This is
  ADR 004 item 8's anticipated outcome; a rejection is a completed milestone,
  not a failure.
- **S2.** Any negative control in F cannot be contained by the certificate
  (e.g. a redirect shape that partitions cleanly under the rules but shares
  bytes). Stop and retain the block; do not narrow the control.
- **S3.** The parity test (E) fails — partitioning leaks into dispositions.
  The milestone is not landable without ADR acceptance; stop, do not proceed
  to activation under a research plan.

## 12. Deferred successor: M016 (blocked, not a plan yet)

Destructive activation — per-island proof, per-island spawn behind an explicit
opt-in flag, partial-success JSON/exit contract, kill-switch (flag removal
restores today's behavior), disposable-fixture release gate plus
published-artifact smoke — requires **all three**: M015 closed, ADR 004
accepted with an explicit C004/C006 amendment, and M015's live data showing
islands occur in real runs. Until then M016 must not be written as an
implementation plan; this section is its only sanctioned existence.

## 13. Planning review (§12 of the process)

1. Durable behavior unambiguous: yes — nothing durable changes; §8.
2. No destructive behavior ahead of its dependency: yes — activation is
   structurally separated into blocked M016.
3. Scope/filter precedence explicit: partition applies after classification,
   same position as M014's selection; discovery scope untouched.
4. Filesystem failure semantics explicit: §5 table; default merge.
5. Concurrency does not weaken correctness: §7; sequential unit loop untouched.
6. Cross-platform testable: §10 hosted lanes required.
7. Configuration compatibility: no new keys (§8).
8. Verification specific: §9–§10.
9. Closure evidence measurable: §11 + closure record format per skill.
10. Stop conditions prevent silent scope expansion: §11 S1–S3; S1 names
    rejection as success.
