# C040 — Working-Directory Scope Corrective

Status: **active** — owner-directed 2026-10-20; implementation in progress.
Date: 2026-10-20.
Repository baseline: `main` at `075842f` (0.2.9, unpublished; latest published
tag `v0.2.8`).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Authority: `plans/adr/009-working-directory-joins-the-maintenance-scope.md`
(accepted 2026-10-20), which amends ADR 003 §1/§8 and ADR 006 §1/§3.
Corrects: M014 (CWD-anchored default), M012A (canonical maintenance CLI).
Related: ADR 007 §1/§3 (scoped proof boundary), ADR 008 §4 (root flag
placement — still open, not resolved here), C028, C032, C033, C036, C039.

## Objective

Make the maintenance cleanup spelling mean what an operator standing in a
directory expects it to mean: **it cleans what is here.**

Today the working directory is a *post-classification filter* over a universe
that does not contain it, so from a directory outside the seed and learned
roots the maintenance spelling proves nothing. From `~/.codex` on the
operator's machine it reported `0 would-clean`; the same directory under
`clean ~/.codex` reported four units and 107.15 GiB.

C040 adds the working directory to the maintenance scope as an additional
**discovery root**, so the anchor has something to filter. The discovery
universe may only ever grow, which keeps ADR 006's shared-output protection
structurally intact on this path.

## Non-goals

- **Not** the Full cleanup spelling. ADR 008 §1–§3 (canonical `--full`) and
  §5 (the Full gate, options 5a/5b/5c) remain open owner decisions and are
  untouched here.
- **Not** the root flag placement. ADR 008 §4 (4a minimal vs 4b coherent)
  remains open. C040 does not move `--older-than`, `--include`, `--exclude`,
  `--min-reclaimable-bytes`, `--profile`, or `--package` to the root, and does
  not add a root-level `--full`.
- **Not** a new spelling. `cargo cleanme --here` was considered and rejected
  in ADR 009; the default changes instead.
- **Not** a change to the selection anchor, the ownership proof, the
  deletion mechanism, the exit codes, the JSON envelope, or the reason codes.
- **Not** a change to `clean ROOT` or `clean --full`.
- **Not** the C036 documentation-drift corrective. C040 discharges C036's
  defect **D1** as a side effect, because it rewrites the same root
  `long_about` paragraph; the rest of C036 stays open.

## Current implementation evidence

Measured on the operator's machine on 2026-10-20, from `~/.codex`:

| Spelling | Roots resolved | Discovered | Outcome |
|---|---|---|---|
| `cargo cleanme --dry-run` | `~/m013-fixture`, `~/projects` | 335 manifests, 97 workspaces | `0 would-clean, 3 skipped` |
| `cargo cleanme --dry-run clean ~/.codex` | `~/.codex` | 303 manifests, 128 workspaces | `4 would-clean, 107.15 GiB` |

The relevant code path:

- `src/cli.rs:366-381` — the scope match (`Root` / `Full` / `Maintenance`) and
  the selection match (`WorkingDirectory` for Maintenance, `All` otherwise).
  `clean --known` is not in the scope match; it falls through to
  `Maintenance`, so it has always been the same command as bare invocation.
- `src/main.rs:820-844` — the Maintenance branch resolves the Routine scope
  through `policy::routine_cleanup_candidates` (`src/policy.rs:475`), which is
  the seed roots (`src/policy.rs:60`) plus age-eligible learned roots. The
  working directory is not a root.
- `src/cleanup.rs:1216-1220` — the discovery universe is
  `ScanScope::ExplicitRoots(roots)`. The classification universe *is* the
  resolved roots, so an anchor can only filter what was discovered.
- `src/cleanup.rs:101-113` — `CleanupSelection::Ancestry` matches a workspace
  root that equals, contains, or is contained by the anchor.
- `src/cleanup.rs:1258` — the ADR 007 boundary is decided with the same
  selection, so in-boundary unprovables still block and out-of-boundary ones
  remain diagnostics.
- `src/discovery_state.rs:114-207` — `automatic_root_is_maintainable`, whose
  forbidden list names `~/.codex/worktrees` at line 126 and whose comparison
  at line 192 is a path-prefix test.

The live learned state on that machine holds 17 roots, 14 of them at
forbidden origins. The tool already refuses them; C040 does not change that
for roots the tool chooses.

## Why previous verification missed it

The tests assert the *shape* of the scope, never its *reach*. The
maintenance scope was verified as "seed roots plus learned roots, anchored at
the working directory", and the anchor was verified as "a unit whose root
relates to the working directory is a candidate". Both are true, and together
they are consistent with a run that cleans nothing, because nothing asserted
that the working directory's own tree is ever *discovered*.

The one test that would have caught it needs a directory outside the seed and
learned roots, which no fixture provided: every fixture in the suite lives
under a temporary directory that the Routine scope never claims, so the
maintenance spelling was never exercised against one.

## Invariants

- **I1 — The universe may only grow.** The maintenance scope's roots are a
  superset of the pre-C040 roots (seed ∪ learned). Adding the working
  directory can only add participants, so it can only reclassify `private` as
  `shared`. Machine-checked. The working-directory exclusion below removes
  only a candidate the pre-C040 scope never had, so the scope is never
  narrowed either.
- **I2 — The anchor is unchanged.** Selection for the maintenance spelling
  stays `Ancestry(working directory)` and is still applied after
  classification. It cannot add, reclassify, or unblock a candidate.
- **I3 — The ADR 007 boundary is unchanged.** In-boundary unprovables block;
  out-of-boundary ones are reported diagnostics. The boundary follows the
  scope, and the scope grew, so *less* is inside the boundary than before,
  never more.
- **I4 — The operator-direct exemption is scoped to the maintainability
  rule only.** The working directory's roots are exempt from
  `automatic_root_is_maintainable`; they are **not** exempt from symlink
  blocking. The configured `scan.ignore` / `scan.unignore` filters apply to
  the read-only scan path and have never applied to a cleanup — every cleanup
  scope bypasses the discovery filters (ADR 007 §1) — so C040 adds no filter
  behaviour on either path.
- **I5 — No new spawn site.** The enclosing workspace is resolved through the
  existing `CargoRunner` seam. No bare `Command::new("cargo")`.
- **I6 — The machine contract's shape is unchanged.** The `scope` label stays
  `routine`; `selected_roots` carries the actual root set.

## Production changes

### A. Operator provenance

`src/policy.rs` — add `RootProvenance::Operator`. Defect handling matches
`Automatic` (a symlinked root blocks the scope; an absent or non-directory
root is omitted), but the maintainability rule does not apply.

### B. The enclosing workspace

`src/workspace.rs` — add a function that asks Cargo, through the
`CargoRunner` seam, which workspace encloses a directory:
`cargo locate-project --workspace` run with the directory as the child's
working directory. Its `{"root": "<manifest>"}` answer yields the workspace
root; any failure yields "no enclosing workspace". The invocation shape
matches `src/workspace.rs:548`.

### C. The maintenance scope

`src/main.rs` — the Maintenance branch of `resolve_cleanup_roots` adds the
working directory's scope roots (the enclosing workspace, when there is one,
and the working directory itself) with operator provenance. The scan path's
Maintenance scope (`scan --known`) gains the same roots, so the read-only
inventory stays a true inventory of what the maintenance spelling would act
on.

**The working directory joins only when the Routine scope does not already
reach it** — no routine root contains it, and it contains no routine root.
Measured on the operator's machine, adding it unconditionally turned a bare
run from the home directory from a nine-second bounded sweep into a
sixty-five-second sweep of 264,183 directories that ended **blocked** on
1,357 unresolvable manifests in `~/.local/share/Trash`. The decision is made
against the **admitted** routine roots, so a root the maintainability rule
refuses does not count as the Routine scope reaching the operator's
directory.

## Work packages

1. **W1 — Operator provenance.** `RootProvenance::Operator` and its
   classification arms.
2. **W2 — Enclosing workspace.** The `CargoRunner`-backed resolver and its
   unit tests against a fake runner.
3. **W3 — Scope composition.** The Maintenance branch in `main.rs`, and the
   `scan --known` twin.
4. **W4 — Tests.** The required tests below, including the machine-checked
   I1 invariant.
5. **W5 — Artifacts and documentation.** Regenerate `man/` and
   `completions/`; update the root `long_about`; reconcile the deep dives,
   `docs/USAGE.md`, `CHANGELOG.md`, the registry, the skills, and `AGENTS.md`;
   write the closure record.

## Required tests

- **T1** — The maintenance scope's roots are a superset of the seed and
  learned roots (I1). Fails if the working-directory roots ever replace the
  Routine roots.
- **T2** — A maintenance run from a directory outside the seed and learned
  roots discovers and proves the workspace underneath it. This is the
  headline case and the one no fixture previously exercised.
- **T3** — Standing in a member directory still cleans the enclosing
  workspace (the enclosing-workspace root is present).
- **T4** — Operator provenance admits a root that `automatic_root_is_maintainable`
  refuses, and the same path as an *automatic* root is still refused.
- **T5** — The configured ignore list still prunes the working directory's
  subtree in the read-only maintenance inventory (I4). Asserted on the scan
  path, where the filters apply; every cleanup scope bypasses them
  (ADR 007 §1), so the cleanup path has no filter behaviour to preserve.
- **T6** — A symlinked working directory still blocks the scope rather than
  being followed (I4).
- **T7** — `clean ROOT` and `clean --full` are byte-identical in behaviour
  before and after C040.
- **T8** — The existing regression `out_of_anchor_poison_does_not_block_anchor`
  still passes.
- **T9** — The JSON contract's `scope` label is unchanged and
  `selected_roots` carries the working directory's roots.
- **T10** — A working directory that already contains a routine root does not
  widen the scope (I1). The bounded machine-wide sweep stays bounded.
- **T11** — An explicit root keeps its own scope: `clean ROOT` does not gain
  the working directory.

## Verification commands

The ladder in `AGENTS.md` §3, in order, plus:

```sh
cargo test --all-features --test cli_contract <each new case>
```

`scripts/release-check.sh` is the pre-release gate and is **not** part of
routine verification.

## Documentation updates

- `src/cli.rs` — the root `long_about`: describe the maintenance scope's new
  composition. This rewrite also discharges C036's defect **D1** (the stale
  "`cargo cleanme scan` with no ROOT is the full read-only machine
  reconciliation" sentence), which is recorded in C036's plan.
- `architecture/02-cli.md`, `04-policy-and-scope.md`,
  `06-discovery-state.md`, `09-cleanup.md`, `10-reporting.md`,
  `13-orchestration.md`, `16-interactive.md` — re-derive the citations that
  moved; do not shift them arithmetically.
- `architecture/overview.md` — the module index, the counts, and a finding
  for the defect C040 corrects.
- `docs/USAGE.md` — the maintenance scope, the scheduler recipe change
  (ADR 003 §8), and the migration note.
- `CHANGELOG.md` — the `[Unreleased]` entry.
- `plans/registry.md` — the C040 row and the subsystem row.
- `.skills/discovery-and-ownership/SKILL.md` — the scope composition.
- `.skills/cli-surface-and-argv/SKILL.md` — the spelling table.
- `AGENTS.md` — §8's "things that are already decided", the scheduler recipe,
  and the counts.
- `plans/adr/003-*.md` and `plans/adr/006-*.md` — amended by reference to
  ADR 009, with the amendment dated and attributed, not edited to imply the
  original decisions were never made.

## Rejected alternatives

Recorded in ADR 009 §Rejected alternatives: narrowing the scope to the
working directory (ADR 006's measurement), a `--here` flag, making the
maintenance spelling identical to `clean <cwd>`, applying the maintainability
rule to the working directory, a new scope label, and a walk-up resolution of
the enclosing workspace.

## Acceptance criteria

1. `cargo cleanme --dry-run` from a directory outside the seed and learned
   roots reports the units under it, with the same proof and the same
   reporting as every other spelling.
2. The I1 invariant is machine-checked and green.
3. Every required test above exists and is green, and each has been shown to
   fail against the pre-C040 behaviour.
4. The ladder is green, the generated artifacts match the clap model, and
   the hosted lanes are green.
5. The documentation states the new composition, the scheduler migration, and
   the pre-1.0 version boundary.

## Stop conditions

- If the hosted lanes show the I1 invariant cannot be established across
  platforms, stop and re-derive the scope composition rather than widening
  the exemption.
- If the working directory's scope roots make the maintenance scope
  unbounded — for example if resolving the enclosing workspace can return a
  filesystem root — stop; a scope root must be a bounded directory.
- If ADR 008 is accepted with §4b before C040 lands, fold C040's spelling
  work into that ADR and do not ship two overlapping front doors.

## Closure evidence requirements

- The implementation commit or PR.
- A requirement-to-evidence matrix over I1–I6 and T1–T9.
- The verification commands actually run, with their results.
- Hosted evidence: the CI run and the Release drift guard, with the hosted
  census re-derived name for name.
- Known limitations and unresolved findings, classified by severity.
- Disposition: closed, conditionally closed, corrective required, or blocked.
- A statement that the corrective remains **open** until a published release
  carries it.
