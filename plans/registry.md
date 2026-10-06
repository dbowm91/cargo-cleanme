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
C025 planning baseline: `35905c5841effadcdb32e3ad4b5f396e4e9096ef`
Immutability policy enabled: `2026-10-05T13:34:38Z` (repository-level, not owner-enforced)

## Subsystem status

| Subsystem | Status | Roadmap | Current milestone | Blocker |
|---|---|---|---|---|
| Artifact discovery and cleanup | **released in 0.2.1; no open corrective** | `plans/subsystems/artifact-discovery-cleanup-roadmap.md` | none | Both destructive defects are fixed, qualified by tests shown to fail against the old behaviour, and published as 0.2.1. The one medium finding the requalification surfaced — the canonical-glob rewrite was a no-op on Windows — is fixed and **closed** by C024, unreleased. |
| Distribution, release, and update | **no active release corrective; M011C conditional** | `plans/subsystems/distribution-release-update-roadmap.md` | none | C023 is conditionally closed: v0.2.1 is published/attested/safety-qualified and crates.io 0.2.0 is yanked, but acceptance criterion 11 — a green **automatic** five-lane smoke — is not met, because the automatic v0.2.1 run fired and failed against the yank C023 itself performed. M011C remains conditional on the same missing evidence; the next release must produce a green automatic smoke to discharge it. |

## Open work

**There is no open implementation plan.** C024 — the one medium finding C023's
requalification surfaced — is **closed**: the canonical-glob rewrite now runs on
Windows, an `exclude` naming a bracketed directory matches it literally, and the
hosted Windows lane observed it. The fix is **unreleased**: it reaches users when
the next release is cut, which is a separate decision. What remains is not
implementation work but two conditional-evidence debts below, neither of which is
actionable from this repository.

**C023 is closed, conditionally.** Both destructive defects that shipped in
0.2.0 are fixed and published as 0.2.1, whose published binary passes both
safety fixtures; crates.io 0.2.0 is yanked and the defect is disclosed in four
user-facing documents. Ordinary roadmap work is unblocked.

The condition is WP-J, and it is a real one rather than a formality. The
automatic post-release smoke **fired** for v0.2.1 and **failed on all five
lanes**, because yanking 0.2.0 — the correct safety decision, and one WP-L
required — made the rehearsal's chosen predecessor uninstallable. The resolver
is now fixed and self-tested to skip yanked versions, and the green five-lane
evidence for `v0.1.6 -> v0.2.1` comes from the manual rehearsal surface. No
green *automatic* run for v0.2.1 exists and none can: `release: published`
fires once per release. The next release produces one with no action required.

**M011C is a conditional evidence dependency, not an open implementation
item.** Its own record now carries the v0.2.1 addendum, and it separates three
things that are easy to conflate: the automatic trigger is **proven** to fire,
the five-target transaction is **green via manual recovery**, and a green run
*under the automatic trigger* is **not yet proven**. Only the third was ever the
named qualification, and only a future publication can satisfy it. It is an
operational dependency per `plans/003-planning-process.md` §8 — it does not
justify writing a new M011C implementation plan, and it does not justify
upgrading the record now.

| Item | Status | Plan / record | Note |
|---|---|---|---|
| M012A — canonical maintenance CLI and dry-run semantics | **closed** | `plans/closure/artifact-discovery-cleanup/m012a-status.md` | Bare `cargo cleanme` is Routine Execute through the existing combined-root proof; `--dry-run` is zero-Cargo-clean simulation; rootless `scan` is Full and ignores a configured `scan.root`; `clean` defaults to Execute. The destructive-default break shipped at the 0.2.0 minor boundary; current `main` is 0.2.1. Two defects were found and fixed inside the milestone: a configured `scan.root` made `clean --known` a silent no-op, and `clean --full` returned the scan's exit code silently. |
| M012B — bounded unattended log output and greggd integration | **closed** | `plans/closure/artifact-discovery-cleanup/m012b-status.md` | `--format log` emits one bounded deterministic ASCII line (<=384 bytes, no paths, no Cargo stderr) with a typed block-reason code; progress and ordinary diagnostic fan-out are suppressed in that mode; JSON and exit codes unchanged; `docs/AUTOMATION.md` documents the greggd integration. A stale `scan --deep` example in Gregg's docs is a non-blocking downstream handoff. |
| C019 — exact unignore sibling containment corrective | **closed** | `plans/closure/artifact-discovery-cleanup/c019-status.md` | Corrects M002: a literal ignored ancestor plus an exact unignore re-admitted unrelated siblings. Shipped in v0.2.0 and retained in the C023 v0.2.1 corrective line. |
| C020 — Windows concurrent first-use config creation race | **closed** | `plans/closure/artifact-discovery-cleanup/c020-status.md` | Corrects `config::create_initial`. Fixed with a process-wide nonce so concurrent first-use staging names cannot collide. Shipped in v0.2.0 and retained in the C023 v0.2.1 corrective line. |
| C021 — pre-release test and verification-evidence reconciliation | **closed** | `plans/closure/artifact-discovery-cleanup/c021-status.md` | Release-blocking cleanup pass for 0.2.0, implemented on `bf99aa4` from `f3dfd7a`. The updater staging-cleanup flake was **proven** and was worse than reported: the assertion scanned the process-wide temp directory, so a sibling test's live staging path could fail it and it could not pass for its own reason either. Evidence is now fixture-owned, with a deliberate-own-leak negative control and a live-foreign-path concurrency control; no sleeps, retries, or serialization; production update code unchanged. The 17-script inventory is now machine-checked by `check-doc-citations.py` for exact set parity plus uniqueness — it found the real drift on its first run. Changelog now covers C018 and C020. Hosted: CI 37376843721 all 9 jobs green, drift 37375751351 green, selector 37375754785 green including the exploratory lane. **The Windows lane caught a defect in C021's own change** (a unix-only test helper tripping `-D dead_code`) that local Linux verification could not see. |
| C022 — pre-release machine-contract hardening | **closed** | `plans/closure/distribution-release-update/c022-status.md` | Pins the update JSON machine contract (the one non-envelope surface, previously untested) and gives `check-installer-contract.py` a failing-direction self-test wired into every release gate, removing the last checker exception in the repository. No product behaviour changed. **Three hosted failures, all in C022's own new test code, none reproducible locally**: a CRLF assumption (Windows), an `ETXTBSY` staging race on overlayfs (`msrv`), and a read-only flush handle that made the second fix Unix-only (Windows again). It took four pushes to close. |
| M013 — 0.2.0 publication and Phase 11 operational closure | **closed / corrected by C023** | `plans/closure/distribution-release-update/013-status.md` | Historical receipt preserved and not reopened. Its automatic M011C criterion was not met, and post-release interrogation then found two destructive defects in the immutable bytes. An appended corrective note records this; no claim in the record is retracted. |
| C023 — v0.2.0 destructive-safety patch-release corrective | **closed (conditional)** | `plans/closure/distribution-release-update/c023-status.md` | Both published destructive defects reproduced against the immutable 0.2.0 binary and fixed; every shipped change requalified by a test shown to fail against the old behaviour; v0.2.1 published immutably and attested; the **published 0.2.1 binary** passes both safety fixtures; crates.io 0.2.0 yanked and disclosed in four documents. Five findings recorded rather than fixed, one handed to C024. **The single unmet criterion is WP-J**: no observed-green *automatic* five-lane smoke for v0.2.1, because the automatic run fired and failed against the yank C023 itself performed, and the event cannot be re-raised. The resolver that caused it is fixed and self-tested; green five-lane evidence exists from the manual rehearsal surface. |
| C025 — post-v0.2.1 planning/status reconciliation | **closed** | `plans/closure/distribution-release-update/c025-status.md` | Documentation/control-surface corrective. Closed as a control-surface repair, not a product milestone: C023 reconciled to conditional closure with a five-finding count derived from its own §11, M011C's v0.2.1 automatic-failure/manual-green addendum appended, and every false automatic-green summary corrected. **No Rust changed, no release performed, no failed run rewritten as passing**. C024 was left untouched and open at that point, and has since been closed (§C024 row). |
| C024 — Windows glob canonicalization corrective | **closed** | `plans/closure/distribution-release-update/c024-status.md` | Matching-semantics corrective. The canonical spelling is now `/`-normalized and stripped of a Win32 verbatim prefix before it is escaped, so `escape_glob_literal` no longer refuses every Windows path. Covered by a Windows-lane test and an every-lane test that pins the Windows spelling; both mutation-checked. **Fixed but unreleased** — the next release carries it. |
| C010 — `CurlConfig::user_agent` seam in `eggup-curl` | **proposed** | `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md` | Bounded upstream request. It cannot be closed from this repository, and nothing here waits on it: `eggup-eggfetch` already provides the seam. No closure record, by design. |
| C018 — self-update provenance uncertainty fail-closed | **closed** | `plans/closure/distribution-release-update/c018-status.md` | None outstanding. Published 0.1.6 refused in both the default Cargo home and `--root` with 0.2.0 available; bytes byte-identical before and after, bookkeeping still v0.1.6. |
| M011A — immutable release attestation + verification | **closed** | `plans/closure/distribution-release-update/m011a-status.md` | None outstanding. v0.2.0 verified immutable with a valid release attestation, and all 15 downloaded asset digests match it. v0.1.0–v0.1.6 predate the policy and remain mutable. |
| M011B — staged-release validation workflow gate | **closed** | `plans/closure/distribution-release-update/m011b-status.md` | None outstanding. Automatic run 37418179489 passed all six steps against the real draft, including a measured `GLIBC_2.17.0` floor. Its first three executions exposed three defects, now fixed. |
| M011C — published-release smoke automation | **closed (conditional)** | `plans/closure/distribution-release-update/m011c-status.md` | The automatic `release: published` trigger is now **demonstrated**: it fired unprompted for v0.2.1 (run 37507738147). It **failed**, on all five lanes, because C023 had just yanked 0.2.0 and the resolver chose that now-uninstallable version as its rehearsal source. Fixed in the resolver, which now picks the greatest *installable* predecessor. Five-lane green evidence for `v0.1.6 -> v0.2.1` comes from the manual rehearsal surface (run 37508262225). **There was no observed-green *automatic* run for v0.2.1 and there cannot be** — the event fires once per release. The next release gets one, automatically, from the fixed resolver. |
| M011D — Cargo selector qualification lifecycle | **closed** | `plans/closure/artifact-discovery-cleanup/m011d-status.md` | None outstanding. The hosted matrix is a recurring maintenance lane, not a release gate. **Addendum:** the gate had never actually run — both jobs passed a plural `toolchains:` input to an action that declares a singular `toolchain:`, and died on "'toolchain' is a required input". Fixed and re-verified. |

**Read this before the next release.** C023's safety patch, **v0.2.1**, is
published, immutable, attested, and passes both destructive safety fixtures.
Its automatic five-lane `release: published` smoke **fired but failed** because
the resolver selected yanked v0.2.0; that resolver is fixed on `main`
(`cfdc910`). The next release is the first opportunity to obtain the
still-missing green automatic M011C evidence, so:

- **Start from a green full hosted baseline** — all three OS lanes observed
  green, none cancelled, and the release-drift and selector gates green.
- **Use the existing immutable release pipeline unchanged.** Eggpack ->
  automatic staged validation -> human publish -> crates.io from a detached
  checkout of the exact tag -> attestation verification.
- **The publication will trigger M011C automatically.** Nothing has to be
  remembered or dispatched; that part is proven, twice.
- **The resolver must select an installable, non-yanked predecessor.** It now
  consults crates.io's yanked set; a release that yanks its own immediate
  predecessor is exactly the case that broke it, and the next release should
  not assume a predecessor is installable without that check.
- **Record a green automatic five-target run before M011C is upgraded.**
  Manual recovery does not discharge this.

1. **Do not tag while current CI is red, and do not accept a cancelled lane as
   evidence.** C023's baseline had Ubuntu and Windows cancelled behind a macOS
   compile error; two of the three defects it found in that state were invisible
   to Linux and only became visible when all three lanes actually ran.
2. Reproduce a destructive defect only in a disposable fixture, against the
   immutable published binary rather than a build of the tree.
3. Run the complete release gate and require complete Linux/macOS/Windows hosted
   green evidence before tagging.
4. Publish through the same Eggpack -> automatic staged validation -> human
   publish -> crates.io exact-tag -> attestation path.
5. **Require a green automatic M011C `release: published` smoke.** Manual
   recovery does not close that requirement.
6. v0.2.0 is immutable and was never rewritten. It is yanked on crates.io and its
   defects are documented in `CHANGELOG.md`, `README.md`, `docs/INSTALLING.md`,
   and `docs/TROUBLESHOOTING.md`.
7. **C024 is closed and unreleased.** The next release carries the fix; nothing
   about it is release-blocking, and folding it into a *safety* patch would have
   been the failure mode C023 exists to prevent — which is why it waited until
   after 0.2.1 rather than inside it.

## Published state

Nine releases are published as GitHub releases and on crates.io. **v0.2.0 is
yanked**; the other eight are not.

Rechecked live against the crates.io API on 2026-10-06: `total: 9`, and
`yanked` is `true` for `0.2.0` alone — `0.2.1` through `0.1.0` all report
`yanked: false`. The yank is visible in that record as an explicit `yank`
audit action at `2026-10-06T16:21:49Z`. GitHub reports nine corresponding
releases, with v0.2.1 and v0.2.0 both `immutable: true`.

**v0.2.0 is the first immutable release and it carried two destructive defects:**
pre-subcommand `--dry-run` may execute a real cleanup, and a covering output
root may include another resolved workspace's source tree. It carried a valid
attestation for the bytes that shipped — the attestation proves identity, not
safety. It is yanked rather than rewritten because the release is immutable and
because deleting files it did not own is not something a "the rest was fine"
note can honestly describe. **v0.2.1 carries both fixes.** v0.1.0–v0.1.6 predate
the immutable-release policy and remain mutable.

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
default cannot ship as a `0.1.x` patch: `Cargo.toml` has read `0.2.0` or later
ever since (it is `0.2.1`), so the boundary is structural rather than a promise
in a document. C021, the pre-release evidence corrective that had to close
before that 0.2.0 tree was staged, is closed.

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
