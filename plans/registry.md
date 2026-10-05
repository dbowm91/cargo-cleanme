# cargo-cleanme Active Planning Registry

This file is the compact control surface for planning, per
`plans/003-planning-process.md` §11. It states **what is open and what to read
next**. It deliberately does not restate requirements or repeat the
milestone-by-milestone history: that lives in the source plans and the closure
records, and duplicating it here is how a control surface goes stale.

| Read this | For |
|---|---|
| `plans/000-long-term-specification.md` | canonical product direction |
| `plans/001-terminology-and-domain-model.md` | the shared vocabulary |
| `plans/002-long-term-roadmap.md` | roadmap ordering |
| `plans/003-planning-process.md` | the planning rules this registry obeys |
| `plans/adr/` | accepted decisions that constrain implementation |
| `plans/subsystems/*-roadmap.md` | per-milestone status, dependency graph, closure mapping |
| `plans/closure/<subsystem>/<NNN>-status.md` | the requirement-to-evidence record for one milestone |
| `architecture/overview.md` | how the code actually fits together |

## Baselines

Planning framework baseline: `97dee9fa64868534cedf3a9310e7160335a91be2`
M006 implementation baseline: `47574fa8a087ac2f9111e29821c23b13657e7bbe`
M008 planning baseline: `3ee9699a0b0d987287d08e427195284e08d079f7`
Phase 11 implementation baseline: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`
Immutability policy enabled: `2026-10-05T13:34:38Z` (repository-level, not owner-enforced)

## Subsystem status

| Subsystem | Status | Roadmap | Current milestone | Blocker |
|---|---|---|---|---|
| Artifact discovery and cleanup | ready | `plans/subsystems/artifact-discovery-cleanup-roadmap.md` | none open | none. C019 closed, and its two blocking baseline defects (the README/release-contract drift and the M011D gate that had never run) were reconciled in the same pass. The correction is **unreleased**: 0.1.6 and earlier keep the over-broad behavior. |
| Distribution, release, and update | closed / Phase 11 complete, pending a published release | `plans/subsystems/distribution-release-update-roadmap.md` | M011A, M011B, M011C, C018 all closed | No plan is blocked. **The next release from `main` is the first covered by immutability**, and it is the first thing that can supply the outstanding hosted evidence for M011A, M011B, M011C, and C018. |

## Open work

Two kinds of thing are open. The Phase 11 hardening milestone set is closed; its
outstanding items are **evidences that require a published release**, which no
plan in this repository can supply without a human publishing one. Separately,
two pre-existing documentation defects were found and left open while closing
C019, and one of them is the reason `architecture/14-testing-and-verification.md`
§5 under-describes the guard suite.

| Item | Status | Plan / record | Note |
|---|---|---|---|
| C019 — exact unignore sibling containment corrective | **closed** | `plans/closure/artifact-discovery-cleanup/c019-status.md` | Corrects M002: a literal ignored ancestor plus an exact unignore re-admitted unrelated siblings, so `ignore = […/archived]` with one `unignore` beneath it discovered every project in the archived subtree. Now one `Filters::disposition` decides `Included`/`Pruned`/`PassThrough`/`ReIncluded`, and inherited exclusion is proven by testing the path's ancestry. Unreleased. |
| `architecture/14-testing-and-verification.md` §5 contract-checker table | **open — needs a plan** | recorded in `plans/closure/artifact-discovery-cleanup/c019-status.md` §12 | Characterises 4 of 17 scripts, and its `check-release-contract.py` row describes a contract M010A replaced. Documentation-only, low severity; the guards it under-describes are wired into CI and each take a `--self-test`. |
| C010 — `CurlConfig::user_agent` seam in `eggup-curl` | **proposed** | `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md` | Bounded upstream request. It cannot be closed from this repository, and nothing here waits on it: `eggup-eggfetch` already provides the seam. No closure record, by design. |
| C018 — self-update provenance uncertainty fail-closed | **closed** | `plans/closure/distribution-release-update/c018-status.md` | Outstanding: work package 9 — a default-Cargo-home and `--root` real installation refused by a *released* binary, bytes unchanged. |
| M011A — immutable release attestation + verification | **closed (conditional)** | `plans/closure/distribution-release-update/m011a-status.md` | Outstanding: verification of a real attested `cargo-cleanme` release. Policy enabled and verified live; v0.1.0–v0.1.6 remain mutable and are not described as attested. |
| M011B — staged-release validation workflow gate | **closed (conditional)** | `plans/closure/distribution-release-update/m011b-status.md` | Outstanding: a real Eggpack draft run triggering the workflow automatically, with a green validator against that draft. |
| M011C — published-release smoke automation | **closed (conditional)** | `plans/closure/distribution-release-update/m011c-status.md` | Outstanding: a real `release: published` event launching the five-target workflow, all five lanes green after crates.io exposes the version. |
| M011D — Cargo selector qualification lifecycle | **closed** | `plans/closure/artifact-discovery-cleanup/m011d-status.md` | None outstanding. The hosted matrix is a recurring maintenance lane, not a release gate. **Addendum:** the gate had never actually run — both jobs passed a plural `toolchains:` input to an action that declares a singular `toolchain:`, and died on "'toolchain' is a required input". Fixed and re-verified. |

**Read this before the next release.** The next release from `main` is the first
that will carry an attestation, and it is the first chance to supply four
outstanding evidences at once. Two constraints make the order matter:

1. Publication requires **both** `release-binaries.yml` and
   `validate-staged-release.yml` green, then human inspection, then publication
   (`docs/RELEASING.md`).
2. The release is **immutable once published**. A defect found in published bytes
   cannot be repaired by replacing an asset; it needs a patch version. The
   post-publication smoke is the last point at which that is cheap.

## Published state

Seven releases are published as GitHub releases and on crates.io, none yanked:
**v0.1.0, v0.1.1, v0.1.2, v0.1.3, v0.1.4, v0.1.5, v0.1.6**. Phase 11 now
requires a future qualification release for C018/M011A-M011C closure evidence,
but planning deliberately does not assign its version before implementation
selects the release boundary.

Not every published version is defect-free, and the registry is where that is
recorded rather than hidden:

- **v0.1.1 and v0.1.2** ship an updater that cannot complete a commit (C016).
  The transaction aborts safely every time, so nothing is corrupted.
- **v0.1.1 through v0.1.4** can replace a binary that Cargo owns (C017). This
  is the only defect in the line that mutated a file it did not own.
- Both are fixed (0.1.3 and 0.1.5 respectively) and disclosed to users in
  `docs/TROUBLESHOOTING.md`. None is yanked, because their other behaviour is
  correct and yanking would misdescribe them.

## Current destructive safety boundary

Stable surface. Changing any of these is a canonical-direction change, not an
implementation task.

- Cargo resolves workspace/output configuration; cargo-cleanme does not infer
  unresolved output paths.
- Every Cargo manifest discovered under the selected cleanup root set must be
  authoritatively covered before any Cargo clean command may run.
- Unresolved ownership participants block Preview, Simulate, and Execute for the
  complete selected-root scope.
- The destructive unit is one workspace `CleanupUnit` over its complete
  `OutputSet`.
- Every affected physical group must be `PrivateBounded`, authorized, inactive,
  non-symlink, marker-qualified, and stable under fresh complete combined
  ownership-graph revalidation.
- `cleanup.allowed_output_roots` never manufactures ownership proof.
- `--dryrun` remains zero-clean and shares the same completeness and final-proof
  gates as Execute.
- `--stats` is the qualification and debug surface.

Defined in ADR 001 plus milestones M004, M005, M007, M008A and correctives
C002–C004, C006. Performance work must not weaken any of it.

## The lesson this repository keeps re-learning

From C009, C011, C012, C013, C015, C016, and C017: **a green test proves only
that its own premises hold.** Several milestones were closed by a test that was
passing for the wrong reason — a POSIX-only fixture passing on no platform, a
transport that could not reach its version authority, a Windows case that had
never executed and was green only because an unrelated real `cargo install`
failed, and a smoke validator that was green on all three hosted lanes while its
subject was failing.

The general form: a fixture that stubs a tool the subject cannot invoke is
indistinguishable from no stub, and a case whose pass condition is an unrelated
failure reports coverage that does not exist. Three product defects (C015, C016,
C017) were found by this repository's own evidence work, and none was repaired
under the plan that found it.

The defences, all of which are self-tested so a guard cannot quietly stop
detecting its defect shape:

| Guard | Defect class |
|---|---|
| `scripts/check-fixture-portability.py` | a fixture that cannot run on the lane reporting it green |
| `packaging/tests/test_installers.py --self-test` | a stub that is not the tool the subject resolves |
| `scripts/smoke-release-candidate.py` | a smoke case that passes while its subject fails |
| `scripts/check-doc-citations.py` | documentation citing lines that no longer exist |

The same reasoning applies to prose. `architecture/` cites source by line, so a
fix that adds or removes lines silently invalidates every citation past that
point while the sentences keep reading as true. `check-doc-citations.py` gates
the mechanically decidable half (the file exists, the line is in range, the
range is ordered); the semantic half — whether the line still says what the
sentence claims — is a reviewer's job, and
[`overview.md` §7.2](../architecture/overview.md) records the general
hazard from the other side: a finding stated confidently enough to be believed,
and wrong.
