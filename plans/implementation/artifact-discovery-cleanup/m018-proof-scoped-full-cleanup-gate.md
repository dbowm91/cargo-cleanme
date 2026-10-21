# M018 — Proof-Scoped Full Cleanup Gate (ADR 008 §5, route 5b-i)

Status: **proposed; not active; no implementation authority**.
Date: 2026-10-21.
Repository baseline: `main` at `8b96f86` (0.2.10 published).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Implements: **ADR 008 §5, route 5b-i** — a verification pass over the learned
roots, so the Full cleanup's proof boundary is the learned root rather than
the platform root.
Related: ADR 008 (accepted 2026-10-21; §5 resolved to 5b-i), ADR 007 §1 and
*Residuals* (amended by ADR 008 on 2026-10-21, effective when this milestone
ships), ADR 006 (the shared-output measurement this milestone must not
reintroduce), ADR 008 §§1–§4 (**M019** — the spelling; it may not be
published before this milestone ships), ADR 009 / C040, M017, C028, C029,
C032, C036 D7.
Publication: **gated on this milestone shipping first** — see "Sequencing".

## Objective

Make `clean --full` usable for an ordinary non-root operator, without
weakening any ownership proof.

Today the operation is **unreachable in practice**: a platform-root walk by a
non-root user is never complete, so `clean --full` refuses before it cleans
anything. The live run recorded in ADR 008's acceptance record (2026-10-21,
uid 1000) exits **2** with 128 permission-denied traversal errors under `/`,
all of them in system trees that contain no Rust development.

This milestone replaces the whole-system gate with a **per-root** gate:

- a learned root whose own traversal **completed** is proven and cleaned;
- a learned root whose traversal did **not** complete is **skipped with a
  typed reason**, and the operation proceeds with the roots it can prove;
- if **no** learned root is proven, the operation proves nothing and fails
  closed.

## Non-goals

- **Not** the canonical spelling. `cargo cleanme --full`, the root-level
  policy flags, and the hidden `clean` alias are **M019**. This milestone
  changes no flag, no spelling, and no generated artifact.
- **Not** ADR 008 §5c (`--full --allow-partial`), which ADR 008 rejected.
- **Not** a change to the platform prune list. The ADR explicitly rejected
  "make the platform walk skip unreadable system trees instead of recording
  them as coverage errors" as a silent narrowing; the coverage record stays
  truthful about what could not be read.
- **Not** a change to ownership proof, Cargo resolution, unit construction,
  per-spawn revalidation, the hoisted-universe refresh, or the learned-state
  schema.
- **Not** a change to the read-only `scan --all`. It keeps exiting 1 on an
  incomplete traversal, and it keeps expiring learned roots only on a complete
  reconciliation.
- **Not** a change to the JSON envelope's shape. One **additive** reason code
  is added; per the `json-and-exit-contract` skill's rule, an additive field
  needs no envelope version bump.

## Sequencing — binding

ADR 008's acceptance record makes this binding: **the spelling change (§§1–4)
and the gate change (§5) are sequenced together, never separately released.**
The canonical spelling must never point at an operation that refuses.

Therefore:

- **M019 may not be published before M018 has shipped.** M019's
  implementation may proceed in parallel; its *release* is gated.
- M018 ships alone, with `clean --full` unchanged as the published spelling.
  The operation becomes usable; the spelling is still two words. That is the
  correct order: the *capability* lands before the *ergonomics*, so the
  canonical spelling never advertises a refusal.

## Contract (decided, under ADR 008 §5)

- **Proof boundary.** The Full cleanup's proof boundary is the **learned
  root**. A learned root is *proven* when a verification walk of that root
  completed with no traversal error and no unusable-root condition; it is
  *unproven* otherwise.
- **Selection.** Proven learned roots are cleanup candidates. Unproven learned
  roots are **unselected, never dropped**: their manifests are still
  discovered, resolved and classified, so their shared-output claims are
  still counted, and they are reported as rows with the new typed reason.
- **New reason code.** `skipped_incomplete_coverage` — a unit whose learned
  root's traversal did not complete, so its ownership could not be proven for
  that root. Additive to the `CleanupReasonCode` vocabulary
  (`src/cleanup.rs:192-225`).
- **Exits.**
  - **0** — every learned root was proven, and the usual policy skips
    (too recent, below minimum size, not included, excluded, shared) do not
    change that. Derived from ADR 007 §5: the requested boundary was fully
    cleaned.
  - **1** — at least one learned root was unproven (the skipped roots are
    reported, and the boundary was not fully cleaned), or a unit failed, or
    the usual block conditions apply. This mirrors the Full *scan*'s exit 1
    for the same underlying condition — an incomplete traversal.
  - **2** — usage errors and fatal errors, unchanged.
  - **Fail closed at zero.** If **no** learned root is proven, the operation
    proves nothing: it fails closed with the standard unproven-ownership
    block, exactly as ADR 007 §5's zero-unit edge requires. It must never
    exit 0 with zero rows — that is the M012A silent-no-op defect.
- **Human output.** The skipped roots appear as ordinary rows with the typed
  reason and a detail that names the cause and the alternative. The refusal
  message of ADR 008 §5a survives in this form: when a root is skipped, the
  message says *why* (its traversal did not complete) and *what the operator
  can do* (`cargo cleanme scan --all` for the read-only inventory,
  `cargo cleanme clean ROOT` for a bounded scope).
- **JSON output.** One additive `reason_code` value. No field added or
  removed; no envelope version bump.

## Engine

### 1. The verification pass

After the Full reconciliation produces the fresh in-memory generation, and
**only if the platform walk was incomplete**, re-walk each learned root of
that generation as **its own walk root**.

- **Reuse, do not reimplement.** `discovery::discover_global_roots`
  (`src/discovery.rs:528`) already accepts a list of roots and already keeps
  per-walk-root attribution: `walk_errors: HashMap<usize, (u64, u64, u64)>`
  (`src/discovery.rs:632`) is drained into **one diagnostic per walk root**
  (`src/discovery.rs:772-783`), naming the root in the message. A learned
  root with no entry in `walk_errors`, and no unusable-root condition, is
  proven. The existing test
  `more_than_one_thousand_walk_errors_stay_one_bounded_root_record`
  (`src/discovery.rs:1008`) covers the bounded-diagnostic property this
  depends on.
- **No dependency change.** The upstream obstacle is unchanged and still
  documented: `dua-core`'s error event carries a bare `io::Error` with no
  child path, only the root index (`src/discovery.rs:775-778`). That is why
  the granularity of a verdict is the **whole learned root** and not the
  unreadable sub-path — the same limitation the ADR records, and the reason
  route 5b-i was chosen over 5b-ii.
- **Same semantics as the platform walk.** The verification pass must use the
  same prune and filter sets as the platform walk, or its coverage verdict
  is not comparable to the verdict it is standing in for.
- **Cost.** The pass runs only when the platform walk was incomplete: when
  the platform walk *completed*, every learned root is proven by definition
  and no second walk happens. The cost is therefore bounded by the union of
  the learned roots' subtrees — smaller than the platform walk, but not
  free. **Measure it** (see "Closure evidence requirements"); if the
  measured cost is not acceptable, that is a stop condition, not something
  to paper over.

### 2. The gate

`policy::full_cleanup_roots` (`src/policy.rs:519-543`) currently refuses any
non-zero reconciliation code (`src/policy.rs:523-528`) and any missing fresh
generation (`src/policy.rs:529-535`). Under this milestone:

- the **missing-generation refusal is unchanged** — no fresh generation means
  no cleanup, and `full_cleanup_consumes_only_a_fresh_in_memory_generation`
  (`src/policy.rs:1137`) keeps pinning it;
- the **reconciliation-code refusal is replaced** by the per-root verdict:
  the function takes the verification result and returns the learned roots
  together with the set of unproven ones;
- the caller is the Full branch in `src/main.rs:753-773`, which runs the
  reconciliation (`src/main.rs:765`) and then calls the gate
  (`src/main.rs:771-772`).

### 3. The exclusion is a post-classification filter

The unproven roots must be excluded from the **candidate** set, never from
the **discovery** set.

**The forbidden shortcut.** Dropping the unproven learned roots from the
cleanup scope — so their manifests are never discovered, resolved or
classified — is the ADR 006 defect (narrowing discovery silently promotes
`Shared` to `PrivateBounded`). It would let a shared output between a
cleaned root and an unproven root be deleted while another workspace is
using it. This shortcut is forbidden, and the test in "Tests" that fails
without it exists to keep it forbidden.

**The required shape.** Exclude the unproven roots the way every other
selection is applied in this codebase: **after** ownership classification,
where the effect can only be to remove candidates.
`cleanup::CleanupSelection` (`src/cleanup.rs:59-91`) is that mechanism, and
its own documentation states the invariant the implementation must preserve:
*"A selection is applied after ownership classification and can only remove
candidates. It cannot add one, reclassify one, or unblock a scope."* The
seam is `resolve_candidate_selection` (`src/main.rs:527-535`), which resolves
the selection for both the engine and the interactive listing.

The exclusion therefore needs a selection shape that means *"every unit,
except those under these roots"* — an **exclude-list**, not the allow-list
`Roots(Vec<PathBuf>)` that `-i` produces. Add it to `CleanupSelection` as
its own variant rather than adding a second, less-documented filter
parameter: one filter concept, one place, with the same "removes candidates
only" guarantee. Its predicate is "the unit's canonical workspace root is
under (or equal to) an unproven learned root", because the verdict's
granularity is the whole learned root.

The reporting follows the existing pattern for units removed from the
selection (`src/cleanup.rs:1420-1437`): **one row per excluded unit**, not a
synthetic summary row, because the report's row identity is a workspace path
and the operator's next question is always "which one?". Those rows carry
`skipped_incomplete_coverage`.

## Safety argument

This is the load-bearing part of the plan. Three properties must hold, and
each has a test.

1. **The universe does not narrow.** Every learned root's manifests are
   discovered, resolved and classified, including the unproven roots'. A
   shared output between a cleaned root and an unproven root therefore stays
   `Shared` and stays inventory-only. *Source: ADR 007 §2 ("discovery and
   grouping stay whole-system") and ADR 006's measurement.*
2. **A cleaned root is fully known.** A root is cleaned only when its own
   traversal completed, so no unknown participant *inside that root* can
   claim its bytes.
3. **The residual is disclosed, not hidden.** An unknown participant hidden
   in an unreadable region **outside every learned root** cannot be
   disproven. Under the withdrawn whole-system rule such a region blocked the
   entire operation; under this milestone it blocks nothing. This is the same
   shape as the residual ADR 007's *Residuals* already disclose for
   out-of-boundary unprovables, and it must appear in the closure record and
   in the user documentation — not only in this plan.

Property 3 is the price of the decision, and it was accepted with the ADR,
not discovered by this plan.

## Tests

Every test names the input that makes it fail. A test that cannot fail is
decoration.

1. **Per-root attribution (ADR 008 evidence item 3).** Two learned roots
   under a bounded fixture: one fully readable, one containing a
   permission-denied subtree. Assert: the readable root's unit is a
   candidate; the unreadable root's unit is skipped with
   `skipped_incomplete_coverage`; and the diagnostic names **the correct
   root**, not the other one. *Fails if:* the verdict is global instead of
   per root, or the roots are swapped.
2. **The shared-output hazard (the ADR 006 measurement).** A workspace in
   the readable root whose target directory is shared with a workspace in the
   unreadable root. Assert: the readable root's unit is classified
   `Shared` (inventory-only) and **no `cargo clean` spawn is issued for it**.
   *Fails if:* the unproven root is dropped from discovery — the forbidden
   shortcut — because the shared bytes would then be classified `PrivateBounded`
   and deleted.
3. **A real non-zero reconciliation code (ADR 008 evidence item 2, C036 D7).**
   A bounded root containing a permission-denied path, exercised through the
   production traversal, so the refusal path runs on a code produced by a
   real traversal rather than a literal. This replaces the literal-`1` shape
   that ADR 008 identifies as the reason the defect survived: the existing
   assertion `full_cleanup_roots(1, Some(state())).is_err()`
   (`src/policy.rs:1150`) proves only its own premise.
4. **Zero proven roots fails closed.** Every learned root unproven. Assert:
   the standard unproven-ownership block, a non-zero exit, and **no cleanup
   spawn**. *Fails if:* the operation exits 0 with zero rows — the M012A
   silent no-op.
5. **The complete platform walk needs no second walk.** Assert that the
   verification pass is not entered when the platform traversal completed.
   *Fails if:* the cost control regresses into an unconditional re-walk.
6. **The published spelling is unchanged by this milestone.** `clean --full`
   still exists, still works, and no root-level `--full` is introduced —
   M019 owns that. *Fails if:* M018 grows a flag.
7. **Regression.** `full_cleanup_consumes_only_a_fresh_in_memory_generation`
   (`src/policy.rs:1137`) keeps passing: a missing fresh generation still
   refuses, and stale learned roots are never cleaned.

## Documentation updates

- `plans/output-schema-v1.md` — add `skipped_incomplete_coverage`, and fix
  the reason-code list's existing omission of `skipped_outside_selection`
  (C036 D11) in the same pass.
- `docs/USAGE.md` — the Full cleanup section must state the new rule: the
  operation cleans the learned roots it can prove and reports the ones it
  cannot, with the reason; and it must disclose the residual.
- `docs/TROUBLESHOOTING.md` — the "clean --full refuses with exit 2" entry
  is obsolete for the normal case; it becomes the *no root could be proven*
  case.
- `architecture/04-policy-and-scope.md`, `architecture/13-orchestration.md`,
  `architecture/overview.md` — the gate's description and the §10 findings,
  with citations re-derived from the new source rather than shifted
  arithmetically.
- `.skills/cleanup-safety/SKILL.md`, `.skills/json-and-exit-contract/SKILL.md`
  — the Full-gate rule and the reason-code vocabulary.

## Verification commands

The ordinary ladder, in order, stopping at the first failure:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

`generate-docs -- --check` is expected to be **unchanged** by this milestone:
it moves no flag and no spelling. If it fails, the milestone has grown a CLI
surface it was not authorised to grow.

MSRV (`cargo +1.89 check --locked --all-targets`) and the hosted matrix
(ubuntu/macos/windows) are not covered by the ladder; a change touching
traversal, mtimes or path comparison is **not verified** until they run.

## Acceptance criteria

- ADR 008 evidence item **2** (a real non-zero code from a real traversal)
  and item **3** (the verification pass showing a provable root beside a
  skippable one, with the error attributed to the correct root) are met by
  tests 1 and 3.
- The seven tests above exist, pass, and each names its failing input.
- The safety argument's three properties are each covered by a test, and
  test 2 fails if the forbidden shortcut is taken.
- The exit-code contract is implemented exactly as specified, including the
  zero-proven-roots fail-closed case.
- `plans/output-schema-v1.md` lists every reason code the code can emit.
- The documentation updates above are made, and the residual is disclosed in
  the user documentation, not only in this plan.
- The ladder is green, and the generated artifacts are unchanged.

## Stop conditions

Stop and write a corrective rather than continuing if:

- the verification pass cannot attribute a traversal error to a specific
  learned root — the verdict would be global, which is the defect ADR 008
  exists to fix;
- the forbidden shortcut (dropping unproven roots from discovery) appears to
  be the only way to implement the gate;
- the measured verification cost is not bounded and acceptable on a real
  machine — the cost is a real cost, and hiding it behind a default timeout
  is not a fix;
- the implementation would need to change the platform prune list, the
  learned-state schema, the envelope shape, or the read-only `scan`;
- any test would have to assert an unrelated failure to pass.

## Closure evidence requirements

A closure record is written under `plans/closure/artifact-discovery-cleanup/`,
and it must carry:

1. the **live** post-implementation run on a non-root unix machine:
   `clean --full` exiting **0** with rows cleaned and rows skipped, and the
   skipped rows naming the unreadable root — the transcript, not a summary;
2. the **measured** verification cost on that machine, against the platform
   walk's cost, with the ratio stated;
3. the **disclosure text** for the residual, as it appears in the user
   documentation;
4. the evidence that the shared-output hazard is still blocked — the test
   that fails if the shortcut is taken, named;
5. the ladder's results, quoted.

Publication is a separate decision: a corrected milestone that is fixed but
not yet carried by a published release stays **open**.

## Related open items this milestone does not close

- **C041** — the update path's candidate-spawn `ETXTBSY` flake. Unrelated to
  this milestone; it stays open.
- The hidden-cursor finding (overview §10 finding 36), the absent pty
  harness, and finding 59's load-sensitive assertions. Separate correctives.
- **M019** — the canonical spelling. Gated on this milestone shipping.
