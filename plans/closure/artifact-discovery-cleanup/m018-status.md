# M018 — Proof-Scoped Full Cleanup Gate: closure record

Status: **closing — implemented and verified in the working tree; not yet
published.** The milestone stays **open**: publication is not closure, and no
release has carried this change yet.

Plan: `plans/implementation/artifact-discovery-cleanup/m018-proof-scoped-full-cleanup-gate.md`

Repository baseline: `main` at `8b96f86` (0.2.10, published).

Authority: **ADR 008** §5, route 5b-i — the Full cleanup's proof boundary
becomes the **learned root** instead of the platform root.

## The executive finding

`clean --full` refused to clean anything whenever the machine-wide traversal
was incomplete, and the traversal was almost always incomplete.

The refusal was not a bug in the sense of a wrong line of code. It was the
**whole-system fail-closed reservation** ADR 007 §1 made for the Full scope,
applied to a fact that does not support it: a traversal error somewhere under
`/` says something about *one region*, and the tool used it to refuse
*every* learned root it had proved perfectly well.

The evidence is in ADR 008's own record of the defect: 127 filesystem
diagnostics, each attributable to a specific region, and a Full cleanup that
cleaned **nothing** because of them. The platform walk was never the thing
that needed proving. The **learned root** was.

The shape of the fix follows the shape of the evidence. `dua-core`'s error
event carries a bare `io::Error` and a root index — no child path — so the
granularity of a traversal failure *is* the walk root. The existing
per-walk-root attribution already carried that granularity; ADR 008 §5 route
5b-i simply asks for it to be used.

**The first live run did not yet satisfy this.** It refused the operation
anyway — once because the unproven root was withheld from the cleanup roots
(the ADR 006 shortcut, which narrows the discovery universe), and once because
the engine could not attribute a traversal error to the root that produced it.
Both are fixed, and both are recorded under "What the first live run found".
The plan was amended on 2026-10-21 with the finding and the authority for the
resolution.

## What was built

Three pieces, and one deliberate decision about what did **not** change.

### 1. The verification pass — `discovery::verify_root_coverage`

When the platform walk could not complete, each learned root is re-walked **as
its own walk root**, and the existing per-walk-root attribution decides — per
root — whether that root's subtree was fully read. A root with no traversal
error and no unusable-root condition is **proven**; every other root is
**unproven** and carries the reason.

The pass uses the **same prune and filter sets as the platform walk**, because
a verdict computed under different rules is not comparable to the verdict it
stands in for. The traversal knobs are read once, by one function
(`global_walk_knobs`), by both walks, so the two cannot drift.

The pass runs **only** when the platform walk was incomplete. A completed
platform walk read every learned root's subtree, so every root is proven by
definition and a second traversal would buy nothing.

### 2. The gate — `policy::full_cleanup_roots`

The refusal on a non-zero reconciliation code is **replaced** by the per-root
verdict. The missing-generation refusal is **unchanged**: no fresh generation
means no cleanup, and `full_cleanup_consumes_only_a_fresh_in_memory_generation`
still pins it.

### 3. The exclusion — `cleanup::CleanupSelection::ExcludeRoots`

An **exclude-list** — "every unit the inner selection admits, except those
under an unproven learned root" — applied where every other selection is
applied: **after** ownership classification, where the effect can only be to
remove candidates.

That placement is the whole safety argument, and it was already the code's
shape: `resolve_cleanup_scope` classifies ownership across the complete
resolved universe *before* the selection filter runs. The unproven root's
workspaces stay in the universe — discovered, resolved, sized, classified — so
a shared output between a proven root and an unproven one stays `Shared` and
stays inventory-only.

**The forbidden shortcut is dropping the unproven root from *discovery*.** It
would hide the other claimant of a shared output and promote those bytes from
`Shared` to `PrivateBounded` — the only class that authorizes deletion. That
is the ADR 006 defect, and the test named below fails by construction against
it.

The reason code is new: `skipped_incomplete_coverage`. It is not
`skipped_outside_selection`, which says "you did not ask for this" — a
different fact, pointing the operator at a different place.

**And it places the unproven root outside the proof boundary.** That is the
one place a filter may unblock a scope, and it is what ADR 008's acceptance
record decides: *"an unreadable region inside a learned root blocks **that
root**"*. The exclusion therefore states a **proof** fact — that root's
traversal did not complete — and an unprovable participant inside it is
reported (`out_of_scope`), not blocking. See "What deliberately did not
change" for the two things that did *not* move with it: discovery does not
narrow, and every other unreadable root still blocks the scope that contains
it.

## The safety argument, and the test for each property

| Property | Claim | Test |
|---|---|---|
| **The universe does not narrow.** | Every learned root's manifests are discovered, resolved and classified, including the unproven roots'. A shared output between a cleaned root and an unproven root stays `Shared`. | `a_shared_output_with_an_unproven_root_stays_shared_and_is_not_cleaned` (`src/cleanup.rs`) and `an_unproven_root_is_admitted_as_a_root_and_excluded_only_as_a_candidate` (`src/main.rs`) |
| **A cleaned root is fully known.** | A root is cleaned only when its own traversal completed, so no unknown participant inside that root can claim its bytes. | `verification_pass_attributes_coverage_to_the_root_that_failed` (`src/discovery.rs`) |
| **An unproven root blocks itself, not the scope.** | The engine attributes a traversal error to the root that produced it; a root the gate judged and disclosed does not block the scope, and every other unreadable root still does. | `an_unproven_roots_unreadable_region_does_not_block_the_scope` and `an_unreadable_region_inside_the_proof_boundary_still_blocks` (`src/cleanup.rs`) |
| **The residual is disclosed, not hidden.** | An unknown participant hidden in an unreadable region — inside an unproven learned root, or outside every learned root — cannot be disproven. Under the withdrawn whole-system rule such a region blocked the entire operation; under this milestone it blocks the root that contains it, and the exclusion is disclosed on stderr and in the report. | `a_unit_under_an_unproven_root_is_reported_as_incomplete_coverage` (`src/cleanup.rs`) and the live run above |

The first property's test is the load-bearing one. Its fixture gives the
readable root's workspace and the unproven root's workspace **one shared
output root**, runs the engine in `Execute` mode with the exclusion applied,
and asserts that the readable root's unit is classified `Shared` and that the
artifact is **still on disk**. Drop the unproven root from discovery and the
same fixture classifies the unit `PrivateBounded` and deletes the bytes.

## The exit-code contract

| Exit | Meaning |
|---|---|
| `0` | Every learned root was proven, and the usual policy skips did not change that. |
| `1` | At least one learned root was unproven (the skipped roots are reported, and the boundary was not fully cleaned), or a unit failed, or the usual block conditions apply. |
| `2` | Usage errors and fatal errors, unchanged. |

**Fail closed at zero.** If no learned root could be proven, the operation
proves nothing: it blocks with the standard unproven-ownership scope block
and exits 1. It never exits 0 with zero rows — that is the M012A silent
no-op, where an operator script reads "success" for a cleanup that never ran.

## What deliberately did not change

- **The read-only `scan --all`.** It still exits 1 on an incomplete traversal,
  and learned roots still expire only on a complete reconciliation. It uses
  `ReconciliationMode::IncompleteDiscard`.
- **The platform prune list**, the **learned-state schema**, and the **JSON
  envelope's shape** — one reason code was added, nothing else moved.
- **The ownership proof itself.** Cargo resolution, unit construction, the
  per-spawn revalidation, the hoisted-universe refresh and the output
  environment re-verification are unchanged; the gate decides *which roots
  may be cleaned*, not how ownership is proven.

**One thing did change, and it is a proof-boundary change, not an ownership
one.** The unproven root is outside the **proof boundary**, so an unprovable
participant inside it is reported (`out_of_scope`) rather than blocking the
whole scope. ADR 008's acceptance record decides this — *"an unreadable
region inside a learned root blocks **that root**"* — and ADR 007's amendment
of the same date says it from the other side: *"`clean --full` no longer
blocks whole-scope on any unprovable participant."* The plan's own quote of
the selection invariant was amended on 2026-10-21 to record the exception and
its limit.

Two things did **not** change with it, and both are load-bearing:

- **Discovery does not narrow.** The unproven root's manifests are still
  discovered, resolved, sized and classified. The universe stays whole, so a
  shared output between a proven root and an unproven one stays `Shared`.
- **Every other unreadable root still blocks the scope that contains it.** An
  unreadable automatic root is indeterminate coverage, not absence:
  `unreadable_automatic_root_blocks_instead_of_omitting` keeps saying so, and
  it is the test that fails if the per-root attribution is ever generalised
  into "a pathless diagnostic never blocks".

## The learned-state consequence, stated plainly

`reconcile_full` learned that an incomplete traversal proves **no absence**,
so the Full cleanup's reconciliation retains every prior learned root — and
narrows none of them — and the resulting generation is consumed **in memory**
and never published.

This was a change the plan did not spell out and the implementation required:
without it there is no fresh generation when the platform walk is incomplete,
and the gate would have nothing to judge. The properties it preserves:

- nothing is expired, so a root that was merely behind an unreadable region
  is not forgotten;
- no root is narrowed, so an out-of-scope claimant of a shared output is not
  hidden (narrowing would promote `shared` bytes to `private`);
- the persisted state keeps describing the last **complete** reconciliation,
  so an incomplete pass can never advance the generation a later run trusts.

The read-only scan keeps the strict behaviour, so an incomplete `scan --all`
still publishes nothing.

## The live run

Three live runs on the development machine (non-root, uid 1000, Linux), all
against the **real** home directory, with the learned state isolated
(`XDG_STATE_HOME=/tmp/m018state`) so the machine's own state was never
rewritten. All three are `--dry-run`: a machine-wide *Execute* cleanup would
delete the operator's own build artifacts without an instruction to do so, and
the dry run exercises the identical decision, proof and reporting path — the
only difference is that no `cargo clean` is spawned.

**Fixture.** Two directories under the real home:

- `~/m018-live-readable/proj-a` — a Cargo workspace with a `target/` holding
  real bytes, fully readable;
- `~/m018-live-unproven/proj-b` — a Cargo workspace with a `target/`, plus
  `~/m018-live-unproven/locked`, a mode-`000` directory the walk cannot enter.

Both were backdated two hours, so the default 300-second recency treats them
as inactive and they are eligible for cleanup.

### Run A — one unproven root (exit 1)

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --dry-run --stats --no-progress --format json
cargo-cleanme: discovery state was not updated: the Full traversal was incomplete; the learned roots were retained and verified in memory
140 filesystem diagnostics; rerun with a bounded root if needed
cargo-cleanme: learned root /home/sugarwookie/m018-live-unproven was skipped: 1 traversal error(s) under /home/sugarwookie/m018-live-unproven: 1 permission-denied, 0 vanished, 0 other; the skipped region's location is unknown
exit 1
```

The JSON result (`result` object):

```json
{
  "scope_blocked": false,
  "selected_roots": [
    "/home/sugarwookie/m013-fixture",
    "/home/sugarwookie/m018-live-readable",
    "/home/sugarwookie/m018-live-unproven",
    "/home/sugarwookie/projects"
  ],
  "units": [
    {"reason_code": "skipped_incomplete_coverage", "workspace_root": "/home/sugarwookie/m018-live-unproven/proj-b", "detail": "the learned root /home/sugarwookie/m018-live-unproven could not be proven: 1 traversal error(s) under /home/sugarwookie/m018-live-unproven: 1 permission-denied, 0 vanished, 0 other; the skipped region's location is unknown; the workspace was left untouched — run `cargo cleanme scan --all` for the read-only inventory, or `cargo cleanme ROOT` for a bounded scope"},
    {"reason_code": "simulated", "workspace_root": "/home/sugarwookie/m018-live-readable/proj-a"},
    {"reason_code": "simulated", "workspace_root": "/home/sugarwookie/projects/i2pr"},
    {"reason_code": "simulated", "workspace_root": "/home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit"},
    {"reason_code": "simulated", "workspace_root": "/home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/crates/eggwork-test-node"}
  ],
  "summary": {"previewed": 0, "simulated": 4, "cleaned": 0, "skipped": 1, "failed": 0, "diagnostics": 3}
}
```

Four facts are load-bearing here:

1. **`scope_blocked` is `false`.** The unreadable region under the unproven
   root blocks **that root**, not the scope. This is the per-root rule ADR
   008 §5 adopted, and it is the difference between a usable operation and
   the whole-system refusal the ADR withdrew.
2. **The unproven root is in `selected_roots`.** It is a *root*: its manifests
   were discovered, resolved and classified. That is what keeps the universe
   whole — see the safety table above.
3. **The unproven root's own unit is a row with
   `skipped_incomplete_coverage`**, and the detail names the root, the cause
   and the alternative.
4. **The proven root's unit is a candidate** (`simulated`), so the operation
   did what it could rather than nothing.

The same run in the human format:

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --dry-run --stats --no-progress
  reported unprovable participant(s), not blocking:
  unresolved [locate] /home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/crates/codegg-core/fuzz/Cargo.toml: cargo locate-project failed: ...
  unresolved [locate] /home/sugarwookie/projects/codegg/crates/codegg-core/fuzz/Cargo.toml: cargo locate-project failed: ...
  hint: a nested crate that believes it is in a workspace needs an empty `[workspace]` table or a parent `workspace.exclude` entry; otherwise clean a healthy workspace explicitly, e.g. `cargo cleanme ROOT`
Simulated  /home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/target                           100.11 GiB
Simulated  /home/sugarwookie/projects/i2pr/target                                                          41.49 GiB
Simulated  /home/sugarwookie/projects/codegg/.worktrees/m005-writer-audit/crates/eggwork-test-node/target  1.26 GiB
Simulated  /home/sugarwookie/m018-live-readable/proj-a/target                                              16.00 KiB

142.86 GiB would be reclaimed from 4 directories (simulation only — no `cargo clean` command was invoked, 1 skipped)
exit 1
```

### Run B — every learned root proven (exit 0)

With `~/m018-live-unproven` removed, the same command exits **0**: every
learned root's own traversal completed, so every root is proven, no root is
skipped, and the boundary was fully cleaned.

```console
$ XDG_STATE_HOME=/tmp/m018state cargo-cleanme --full --dry-run --stats --no-progress --format json
exit 0
```

```json
{
  "scope_blocked": false,
  "selected_roots": [
    "/home/sugarwookie/m013-fixture",
    "/home/sugarwookie/m018-live-readable",
    "/home/sugarwookie/projects"
  ],
  "summary": {"previewed": 0, "simulated": 4, "cleaned": 0, "skipped": 0, "failed": 0, "diagnostics": 2}
}
```

Both halves of the exit contract are therefore exercised on a real machine:
exit **1** with a skipped root, exit **0** with none. (The plan's
closure-evidence wording asked for a single run that exits 0 *with* skipped
rows; that is not reachable under this milestone's own exit contract — a
skipped row means an unproven root, and an unproven root means exit 1. The
plan was amended on 2026-10-21 to say so.)

### What the first live run found

The first live run of the pre-amendment implementation **refused the
operation** with exit 1 and `scope_blocked: true`:

```text
scope_reason: "cleanup ownership universe is incomplete in the selected scope: 1 discovery diagnostic(s); resolved 99 workspace(s), considered 4 cleanup unit(s); no cleanup commands were run"
```

The unproven root was absent from `selected_roots`, and its workspace produced
no row at all. Two defects were behind it, and both are fixed:

1. **The unproven root was withheld from the cleanup roots.** That narrowed
   the discovery universe — the ADR 006 forbidden shortcut. The fix admits
   every learned root, proven or not, as a root
   (`FullCleanupVerdict::admitted_roots`), and excludes the unproven ones from
   the *candidate* set instead.
2. **The engine's boundary rule could not attribute a traversal error.** A
   traversal error carries no path, so it was counted as in-boundary,
   fail-closed — and the scope blocked. The engine now walks **per root**, so
   the error is attributed to the root that produced it, and a root the gate
   judged and disclosed does not block.

The plan was amended on 2026-10-21 with both findings and the authority for
the resolution.

## The measured verification cost

The verification pass re-walks each learned root as its own walk root, so it
is a second traversal of the learned roots' subtrees. It runs **only** when the
platform walk was incomplete, and the product now prints its cost on the
cleanup stats line (`verification=<secs>`, or `verification=skipped` when the
pass was not needed).

Measured on the machine above, same run, same `--stats` line:

| Quantity | Run A (unproven root) | Run B (all roots proven) |
|---|---|---|
| Platform walk discovery (`discovery=` in the scan stats) | 6,868 ms | 6,128 ms |
| **Verification pass (`verification=` in the cleanup stats)** | **0.14 s** | **0.15 s** |
| Ratio to the platform walk | **2.0 %** | **2.4 %** |
| Whole reconciliation (`elapsed=` in the scan stats) | 18.70 s | 18.19 s |
| Ratio to the whole reconciliation | 0.7 % | 0.8 % |
| Cleanup phase (`elapsed=` in the cleanup stats) | 10.18 s | 10.45 s |

The ratio is small because the learned roots' subtrees are a small fraction
of the platform tree: the platform walk visited 468,593 directories, the
cleanup phase 27,275.

An earlier, **cold-cache** measurement of the same pass on the same machine
was 1.82 s against a 7.63 s platform walk — 23.9 %. The warm figures above are
the steady state; the cold figure is the worst one observed, and both are
reported here rather than only the flattering one.

The cost is therefore bounded by the learned roots rather than by the
platform, it is *measured in the product* rather than assumed, and it is
within the plan's stop condition ("the measured verification cost is not
bounded and acceptable on a real machine"). It is not free, and the record
says so.

## The disclosure text

The residual is disclosed in three places, and all three name the cause and
the alternative:

1. **The report row.** A unit under an unproven root carries the reason code
   `skipped_incomplete_coverage` and a detail that reads:

   > the learned root `<root>` could not be proven: `<cause>`; the workspace
   > was left untouched — run `cargo cleanme scan --all` for the read-only
   > inventory, or `cargo cleanme ROOT` for a bounded scope

2. **The stderr note.** One line per unproven root, in every format but
   `log` (the log format carries no paths, by contract):

   ```console
   cargo-cleanme: learned root <root> was skipped: <cause>
   ```

3. **The user documentation.** `docs/USAGE.md`'s Full cleanup section states
   the rule and the residual; `docs/TROUBLESHOOTING.md` carries the
   no-root-could-be-proven case.

## The tests

Nine cases, each naming the input that makes it fail.

| # | Test | Where |
|---|---|---|
| 1 | `verification_pass_attributes_coverage_to_the_root_that_failed` | `src/discovery.rs` |
| 2 | `a_shared_output_with_an_unproven_root_stays_shared_and_is_not_cleaned` | `src/cleanup.rs` |
| 3 | `a_real_traversal_produces_the_code_the_gate_judges` | `src/discovery.rs` |
| 4 | `a_full_cleanup_that_proved_nothing_blocks_instead_of_exiting_zero` + `an_unproven_learned_root_makes_the_exit_code_non_zero` + `a_scope_with_no_roots_dispatches_no_cleanup` | `src/main.rs`, `src/cleanup.rs` |
| 5 | `a_complete_platform_walk_needs_no_verification_pass` | `src/main.rs` |
| 6 | `the_full_cleanup_alias_still_resolves_after_the_root_moved` | `src/cli.rs` |
| 7 | `full_cleanup_consumes_only_a_fresh_in_memory_generation` | `src/policy.rs` |
| 8 | `an_unproven_root_is_admitted_as_a_root_and_excluded_only_as_a_candidate` + `a_gate_that_proved_nothing_blocks_even_though_the_roots_are_admitted` | `src/main.rs` |
| 9 | `an_unproven_roots_unreadable_region_does_not_block_the_scope` + `an_unreadable_region_inside_the_proof_boundary_still_blocks` | `src/cleanup.rs` |

Cases 8 and 9 were added after the first live run, and each was checked against
the defect it exists for: case 8 fails when the Full branch passes the proven
roots only, and case 9 fails in **both** directions — when a pathless
traversal diagnostic is treated as always blocking (the whole-system refusal
ADR 008 withdrew) and when it is treated as always harmless (a fail-open).

Test 3 is the one that replaces the old shape. The corpus asserted
`full_cleanup_roots(1, Some(state())).is_err()` — a **literal** `1` — which
proved only that a non-zero code was refused, never that a real traversal
produces the code the gate sees. ADR 008 names that as the reason the defect
survived: nothing in the corpus ever showed the real path. The replacement
runs the **production traversal** over a bounded fixture, confirms it reports
that it could not complete, and drives the gate with the code that traversal
produced.

## The verification ladder

The ladder in `AGENTS.md` §3, run in order, on the working tree as it stands:

```console
$ cargo fmt --all -- --check
(no output)

$ cargo clippy --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo test --all-targets --all-features
test result: ok. 511 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 77 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
check-fixture-portability: self test passed; all four rules verified in both directions
check-fixture-portability: 37 fixture file(s) clean (no literal PATH separator, no unjustified POSIX shebang fixture, no ungated raw-byte write, no platform-specific Entry literal)

$ python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
check-doc-citations: self test passed; all rules verified in both directions
check-doc-citations: 17 document(s) clean; every file.rs:N citation resolves to an existing file and an in-range line

$ cargo run --quiet --features dev-tools --bin generate-docs -- --check
generate-docs: 13 artifacts match the clap model
```

**The two new tests are `#[cfg(unix)]`-gated**, so the 511 lib tests above are
the Linux count. The per-platform counts in `architecture/overview.md` §3 are
derived with a brace-tracking parser against `rustc --print cfg --target
<triple>`, not by grep — and they have to be re-derived for this milestone,
because two gated cases were added.

`scripts/release-check.sh` was **not** run: it requires a clean tree, the
`eggpack` binary and toolchain 1.89, and it is the release gate, not the
ordinary one. Nothing is published by this milestone.

## The citations

The implementation moved 860 lines across seven source files, so every
citation into them moved with it. The re-derivation was done the way
`AGENTS.md` §6 requires: **re-derived from the source, never shifted
arithmetically**. A citation was accepted only when the mapped line carried
*identical* content — the comparison is against the content the citation
pointed at when the citing line was last written, not against a number. 1,263
citations across 30 live documents were re-derived; every changed line
differed **only inside a citation's numbers**, verified mechanically.

Closed plans, closure records and ADR 004 (which pins its own citations "as
of the M015 landing") were left alone: they are historical records.

## Residuals, disclosed

- **An unknown participant in an unreadable region cannot be disproven.** The
  region hides what is inside it, and only the root it was found under is
  known — the region's own location is not. So a Cargo workspace hidden there
  may name an output root that a cleaned root also owns, and the shared output
  would not be recognised as shared. This covers an unreadable region **inside
  an unproven learned root** as well as one **outside every learned root**.
  Under the withdrawn whole-system rule such a region blocked the entire
  operation; under this milestone it blocks **the root that contains it**, and
  the operator is told which roots were skipped, with the cause and the
  alternative. It is disclosed, not fixed, and it is the same shape as ADR
  007's disclosed out-of-boundary residual.
- **The verification pass costs a second traversal of the learned roots'
  subtrees.** It is measured (above), it runs only when the platform walk was
  incomplete, and it is bounded by the learned roots rather than by the
  platform.
- **The MSRV lane (1.89) and the hosted matrix have not run.** Nothing in
  this milestone touches the stdlib API, fixtures, path comparison, mtimes or
  symlink handling — but only those lanes prove it, and this record does not
  claim it. The two new boundary tests are `#[cfg(unix)]`-gated, because a
  mode-`000` directory is how the unreadable region is made; the Windows and
  macOS lanes run the rest.

## Open

- **Publication.** This milestone is `closing`, not `closed`. It must ship
  **before** M019, because the canonical spelling must never point at an
  operation that refuses.
- **C041** — the update path's candidate-spawn `ETXTBSY` flake. Unrelated.
- The hidden-cursor finding (overview §10 finding 36), the absent pty
  harness, and finding 59's load-sensitive assertions. Separate correctives.
