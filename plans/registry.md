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
Phase 12 planning baseline: `e4e9d92673e5f10548248e23db788c0906760916`
C021 planning baseline: `f3dfd7a718d236a5a0b0b664fb74fe02ee7b0f62`
C022/M013 planning baseline: `f3d0d9c8c435063bb67940b8dd1c95605141e0e7`
C023 planning baseline: `eaa7bba1cd70513a7bf48a99d7d9ffe19334a539`
Immutability policy enabled: `2026-10-05T13:34:38Z` (repository-level, not owner-enforced)

## Subsystem status

| Subsystem | Status | Roadmap | Current milestone | Blocker |
|---|---|---|---|---|
| Artifact discovery and cleanup | **corrective required / C023 ready** | `plans/subsystems/artifact-discovery-cleanup-roadmap.md` | C023 v0.2.0 destructive-safety patch corrective | v0.2.0 contains two destructive defects: pre-subcommand `--dry-run` can execute cleanup, and a covering output root can contain another resolved workspace's sources. Fixes are on `main` but current CI is red on macOS and 0.2.1 is not yet published. |
| Distribution, release, and update | **corrective required / C023 ready** | `plans/subsystems/distribution-release-update-roadmap.md` | C023 v0.2.0 destructive-safety patch corrective | M013 published immutable v0.2.0 but did not meet its automatic M011C acceptance criterion, and post-release interrogation found two destructive product defects in those immutable bytes. C023 owns qualification and the 0.2.1 patch release. |

## Open work

**C023 is the immediate safety handoff.** C022 is closed and M013 did publish
v0.2.0, but M013's automatic M011C criterion was not met and v0.2.0 was later
proven to contain two destructive product defects. The immutable v0.2.0 bytes
cannot be repaired in place. C023 owns the current red-main correction,
destructive regression proof, planning reconciliation, crates.io 0.2.0 yank,
and the v0.2.1 immutable patch release. Ordinary roadmap work should wait.

| Item | Status | Plan / record | Note |
|---|---|---|---|
| M012A — canonical maintenance CLI and dry-run semantics | **closed** | `plans/closure/artifact-discovery-cleanup/m012a-status.md` | Bare `cargo cleanme` is Routine Execute through the existing combined-root proof; `--dry-run` is zero-Cargo-clean simulation; rootless `scan` is Full and ignores a configured `scan.root`; `clean` defaults to Execute. `main` is at 0.2.0, so the destructive-default break now structurally cannot ship as a 0.1.x patch. Two defects were found and fixed inside the milestone: a configured `scan.root` made `clean --known` a silent no-op, and `clean --full` returned the scan's exit code silently. |
| M012B — bounded unattended log output and greggd integration | **closed** | `plans/closure/artifact-discovery-cleanup/m012b-status.md` | `--format log` emits one bounded deterministic ASCII line (<=384 bytes, no paths, no Cargo stderr) with a typed block-reason code; progress and ordinary diagnostic fan-out are suppressed in that mode; JSON and exit codes unchanged; `docs/AUTOMATION.md` documents the greggd integration. A stale `scan --deep` example in Gregg's docs is a non-blocking downstream handoff. |
| C019 — exact unignore sibling containment corrective | **closed** | `plans/closure/artifact-discovery-cleanup/c019-status.md` | Corrects M002: a literal ignored ancestor plus an exact unignore re-admitted unrelated siblings, so `ignore = […/archived]` with one `unignore` beneath it discovered every project in the archived subtree. Now one `Filters::disposition` decides `Included`/`Pruned`/`PassThrough`/`ReIncluded`, and inherited exclusion is proven by testing the path's ancestry. Unreleased. |
| C020 — Windows concurrent first-use config creation race | **closed** | `plans/closure/artifact-discovery-cleanup/c020-status.md` | Corrects `config::create_initial`. The staging name was never unique within a process — threads shared the pid and the starting `attempt` — so every concurrent first use collided, and Windows spells that collision `PermissionDenied` rather than `AlreadyExists`. Fixed with a process-wide nonce, so the collision is removed rather than tolerated. Unreleased. |
| C021 — pre-release test and verification-evidence reconciliation | **closed** | `plans/closure/artifact-discovery-cleanup/c021-status.md` | Release-blocking cleanup pass for 0.2.0, implemented on `bf99aa4` from `f3dfd7a`. The updater staging-cleanup flake was **proven** and was worse than reported: the assertion scanned the process-wide temp directory, so a sibling test's live staging path could fail it and it could not pass for its own reason either. Evidence is now fixture-owned, with a deliberate-own-leak negative control and a live-foreign-path concurrency control; no sleeps, retries, or serialization; production update code unchanged. The 17-script inventory is now machine-checked by `check-doc-citations.py` for exact set parity plus uniqueness — it found the real drift on its first run. Changelog now covers C018 and C020. Hosted: CI 37376843721 all 9 jobs green, drift 37375751351 green, selector 37375754785 green including the exploratory lane. **The Windows lane caught a defect in C021's own change** (a unix-only test helper tripping `-D dead_code`) that local Linux verification could not see. |
| C022 — pre-release machine-contract hardening | **closed** | `plans/closure/distribution-release-update/c022-status.md` | Pins the update JSON machine contract (the one non-envelope surface, previously untested) and gives `check-installer-contract.py` a failing-direction self-test wired into every release gate, removing the last checker exception in the repository. No product behaviour changed. **Three hosted failures, all in C022's own new test code, none reproducible locally**: a CRLF assumption (Windows), an `ETXTBSY` staging race on overlayfs (`msrv`), and a read-only flush handle that made the second fix Unix-only (Windows again). It took four pushes to close. |
| M013 — 0.2.0 publication and Phase 11 operational closure | **corrective required / superseded by C023 for current state** | `plans/closure/distribution-release-update/013-status.md` | Historical receipt preserved: v0.2.0 was published and attested, but its automatic M011C criterion was not met. Post-release interrogation then found two destructive defects in the immutable bytes. C023 is the corrective; do not rewrite M013 history as a full pass. |
| C023 — v0.2.0 destructive-safety patch-release corrective | **ready / immediate** | `plans/implementation/distribution-release-update/c023-v0.2.0-destructive-safety-patch-release-corrective.md` | High-severity corrective. Reproduce both published destructive defects; qualify the already-landed fixes; repair current macOS CI; publish immutable v0.2.1; require automatic five-target smoke; yank crates.io 0.2.0; reconcile M013/M011C without rewriting history. |
| C010 — `CurlConfig::user_agent` seam in `eggup-curl` | **proposed** | `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md` | Bounded upstream request. It cannot be closed from this repository, and nothing here waits on it: `eggup-eggfetch` already provides the seam. No closure record, by design. |
| C018 — self-update provenance uncertainty fail-closed | **closed** | `plans/closure/distribution-release-update/c018-status.md` | None outstanding. Published 0.1.6 refused in both the default Cargo home and `--root` with 0.2.0 available; bytes byte-identical before and after, bookkeeping still v0.1.6. |
| M011A — immutable release attestation + verification | **closed** | `plans/closure/distribution-release-update/m011a-status.md` | None outstanding. v0.2.0 verified immutable with a valid release attestation, and all 15 downloaded asset digests match it. v0.1.0–v0.1.6 predate the policy and remain mutable. |
| M011B — staged-release validation workflow gate | **closed** | `plans/closure/distribution-release-update/m011b-status.md` | None outstanding. Automatic run 37418179489 passed all six steps against the real draft, including a measured `GLIBC_2.17.0` floor. Its first three executions exposed three defects, now fixed. |
| M011C — published-release smoke automation | **closed (conditional)** | `plans/closure/distribution-release-update/m011c-status.md` | Five-target evidence satisfied: run 37420625111, all lanes green on `v0.1.6 -> v0.2.0`. **Still outstanding: an observed-green *automatic* `release: published` trigger.** The automatic run fired, hit three defects in its own path, and cannot be re-raised. |
| M011D — Cargo selector qualification lifecycle | **closed** | `plans/closure/artifact-discovery-cleanup/m011d-status.md` | None outstanding. The hosted matrix is a recurring maintenance lane, not a release gate. **Addendum:** the gate had never actually run — both jobs passed a plural `toolchains:` input to an action that declares a singular `toolchain:`, and died on "'toolchain' is a required input". Fixed and re-verified. |

**Read this before the next release.** The next release must be the C023 safety
patch, expected as **v0.2.1**.

1. **Do not tag while current CI is red.** Baseline CI #284 fails on macOS in
   `src/discovery.rs:931` (`std::fs::FileType` vs `dua_core::FileType`);
   Ubuntu/Windows check lanes were cancelled and are not evidence.
2. Reproduce both destructive v0.2.0 defects only in disposable fixtures and
   prove the fixed candidate rejects/makes them non-mutating.
3. Run the complete release gate and require complete Linux/macOS/Windows hosted
   green evidence before tagging.
4. Publish v0.2.1 through the same Eggpack -> automatic staged validation ->
   human publish -> crates.io exact-tag -> attestation path.
5. **Require a green automatic M011C `release: published` smoke.** Manual
   recovery does not close that requirement.
6. v0.2.0 is immutable and must never be rewritten. C023 requires a crates.io
   yank for 0.2.0 and explicit documentation of the affected behavior.

## Published state

Eight releases are published as GitHub releases and on crates.io, none yanked:
**v0.1.0, v0.1.1, v0.1.2, v0.1.3, v0.1.4, v0.1.5, v0.1.6, v0.2.0**, none
yanked.

**v0.2.0 is the first immutable release and is affected by C023.** v0.1.0–v0.1.6
predate the immutable-release policy and remain mutable; v0.2.0 carries a valid
attestation for the bytes that shipped, but those bytes contain two destructive
defects: pre-subcommand `--dry-run` may execute cleanup, and a covering output
root may include another resolved workspace's source tree. The attestation proves
identity, not safety. The corrective target is v0.2.1; v0.2.0 is to be yanked on
crates.io under C023.

C018, M011A, and M011B remain closed on v0.2.0's release-process evidence.
M011C remains conditional on an observed-green automatic trigger. M013's
historical receipt remains useful but its current "closed" state is corrected by
C023: one acceptance criterion was unmet and the published product was later
shown to contain destructive defects.

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

The proof boundary is stable; ADR 003 changed the public invocation front door, not these safety properties. M012A has landed: the canonical spellings changed and every property below is unchanged, with the 71-test `cleanup.rs` suite and the C003/C004/C006 property tests still green.

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
- zero-mutation Simulation shares the same completeness and final-proof gates as Execute; canonical `--dry-run` carries that meaning and `--cargo-preview` is the distinct Cargo-level view.
- `--stats` is the qualification and debug surface.

Defined in ADR 001 plus milestones M004, M005, M007, M008A and correctives
C002–C004, C006. Performance work must not weaken any of it.

## Phase 12 accepted direction (implemented)

ADR 003 intentionally superseded the historical front-door semantics without
weakening cleanup safety. All eight points below are implemented and closed in
`m012a-status.md` and `m012b-status.md`:

- bare `cargo cleanme` -> Routine Execute;
- bare `cargo cleanme --dry-run` -> Routine Simulate with zero `cargo clean` subprocesses;
- `cargo cleanme scan` -> Full read-only reconciliation;
- `cargo cleanme scan ROOT` -> Explicit read-only scan;
- `cargo cleanme scan --known` -> the non-Full maintenance scope (Routine normally; Explicit when legacy configured `scan.root` wins);
- Cargo's own dry-run moves to explicit `--cargo-preview` on advanced cleanup;
- `--format log` is a bounded operator-history surface; JSON remains the complete machine contract;
- scheduling/load policy stays external, with greggd documented as a first-class example.

C003/C004/C006 ownership and freshness proof is preserved. The bare destructive
default cannot ship as a `0.1.x` patch: `Cargo.toml` reads `0.2.0`, so the
boundary is structural rather than a promise in a document. C021, the
pre-release evidence corrective that had to close before that 0.2.0 tree was
staged, is closed.

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
