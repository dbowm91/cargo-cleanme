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

## Subsystem status

| Subsystem | Status | Roadmap | Current milestone | Blocker |
|---|---|---|---|---|
| Artifact discovery and cleanup | ready / Phase 11 hardening | `plans/subsystems/artifact-discovery-cleanup-roadmap.md` | M011D Cargo selector qualification lifecycle | none; evidence-lifecycle hardening only, no selector support expansion |
| Distribution, release, and update | ready / Phase 11 hardening | `plans/subsystems/distribution-release-update-roadmap.md` | C018 provenance uncertainty fail-closed | C018 is the immediate safety handoff; M011A-M011C are also ready and can proceed in parallel, with operational closure tied to a future release |

## Open work

| Item | Status | Plan | Note |
|---|---|---|---|
| C018 — self-update provenance uncertainty fail-closed | **ready** | `plans/implementation/distribution-release-update/c018-self-update-provenance-uncertainty-fail-closed.md` | Immediate safety handoff. Positive Cargo-manager evidence that cannot be interpreted must become `UnprovableOwnership`, never self-managed ownership; mutating refusals move before remote acquisition. |
| M011A — immutable release attestation + verification | **ready** | `plans/implementation/distribution-release-update/011a-immutable-release-attestation-and-verification.md` | Enable/verify immutable GitHub releases and their release attestations without hand-editing Eggpack-generated CI. Operational closure requires the first covered future release. |
| M011B — staged-release validation workflow gate | **ready** | `plans/implementation/distribution-release-update/011b-staged-release-validation-workflow-gate.md` | Product-owned read-only workflow runs `validate-staged-release.py` automatically after a trusted Eggpack candidate-build/stage run. Stop and move upstream to Eggpack if exact source/tag binding cannot be made safe. |
| M011C — published-release smoke automation | **ready** | `plans/implementation/distribution-release-update/011c-published-release-smoke-automation.md` | Add release-published automation for the real five-target updater rehearsal while retaining manual dispatch; boundedly wait for crates.io because GitHub publication precedes crate publication. |
| M011D — Cargo selector qualification lifecycle | **ready** | `plans/implementation/artifact-discovery-cleanup/011d-cargo-selector-qualification-lifecycle.md` | Guard the exact runtime selector allowlist with real-Cargo qualification evidence and a dedicated compatibility workflow; exploratory future Cargo versions never auto-promote support. |
| C010 — `CurlConfig::user_agent` seam in `eggup-curl` | **proposed** | `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md` | Bounded upstream request. It cannot be closed from this repository, and nothing here waits on it: `eggup-eggfetch` already provides the seam. No closure record, by design. |

C018 is the next safety-critical handoff. M011A-M011D are independently
implementation-ready and may proceed in parallel where they do not consume the
same release event. The first future release after these changes is expected to
supply operational evidence for C018 and M011A-M011C; no release number is
pre-authorized by planning.

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
