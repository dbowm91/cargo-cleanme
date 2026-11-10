# cargo-cleanme Active Planning Registry

This file answers one question: what is open right now. It is the compact
control surface from `plans/003-planning-process.md` §11. Requirements stay
in the source plans. History stays in `plans/subsystems/` and
`plans/closure/`. Closed plans are not restated here.

Line citations into an older copy of this file are historical. The file was
compacted on 2026-11-10 because the copy had become a second, contradictory
roadmap. The smoke counts and release digests it used to repeat live in
`CHANGELOG.md` and `plans/closure/distribution-release-update/`.

| Read this | For |
|---|---|
| `plans/000-long-term-specification.md` | canonical product direction |
| `plans/001-terminology-and-domain-model.md` | the shared vocabulary |
| `plans/002-long-term-roadmap.md` | roadmap ordering |
| `plans/003-planning-process.md` | the planning rules this registry obeys |
| `plans/adr/` | accepted decisions that constrain implementation |
| `plans/subsystems/*-roadmap.md` | per-milestone history |
| `plans/closure/<subsystem>/<NNN>-status.md` | the requirement-to-evidence record |
| `architecture/overview.md` | how the code actually fits together |

## Published baseline

`Cargo.toml` and `CHANGELOG.md` say **0.2.14** is the published version.
crates.io carries v0.1.0 through v0.1.6 and v0.2.0 through v0.2.14.
**v0.2.0 is yanked.** v0.2.12 is on crates.io and has no GitHub release, so
self-update into it fails closed; 0.2.13 is its replacement. 0.2.14 carries
C047, C048, M021, and M022; its receipt is
`plans/closure/distribution-release-update/r033-status.md`. This file does
not re-query crates.io. The next release that changes the count must change
this paragraph in the same pass; nothing gates the sentence.

Publication is not closure. A plan below stays open until a published
release carries it.

## Open work

| Item | Status | Plan | Why it is still here |
|---|---|---|---|
| C048 | closing | `plans/implementation/artifact-discovery-cleanup/c048-idle-artifact-cleanup.md` | Idle `Shared` and `ExternalUnproven` artifact directories are cleanable. `Uncertain` paths are not. One active owner protects the directory. Published in v0.2.14; the closure record still needs the post-release evidence. |
| C047 | closing | `plans/implementation/artifact-discovery-cleanup/c047-full-cleanup-per-learned-root-block.md` | On `--full`, a blocking unknown is attributed to the proven learned roots that contain it. Other proven roots still run `cargo clean`. Published in v0.2.14; the closure record still needs the post-release evidence. |
| M022 | closing | `plans/implementation/artifact-discovery-cleanup/m022-bounded-output-claim-cleanup.md` | A metadata failure whose output directories can be named holds only units that intersect that claim. An unnamed claim still blocks the boundary. Published in v0.2.14; the closure record still needs the post-release evidence. |
| M021 | closing | `plans/implementation/artifact-discovery-cleanup/m021-scan-resolution-and-measurement-cost.md` | Fewer locates, and Unix inventory sizing trusts walker metadata. The proof path is unchanged. `RESOLUTION_CONCURRENCY_CAP` stays 4. Published in v0.2.14; the closure record still needs the post-release evidence. |
| M020 | closing | `plans/implementation/artifact-discovery-cleanup/m020-quiet-human-output-surface.md` | The default human surface is one progress line, one row per cleaned directory, and one tally. `--verbose` restores the detail. Implemented; not published. |
| C033 | active | `plans/implementation/artifact-discovery-cleanup/c033-learned-root-reconciliation-and-unavailable-noise-corrective.md` | Omission summaries are grouped. Durable retirement of vanished learned roots is gated on proposed ADR 005, which has no implementation authority. |
| C034 | active | `plans/implementation/artifact-discovery-cleanup/c034-default-routine-cleanup-live-happy-path-and-ownership-corrective.md` | A Unix real-Cargo happy path is proven. The remaining qualification named in the plan is outstanding. The plan's `--partitioned` sentence is historical; ADR 007 superseded that mechanism. |
| C041 | open | `plans/implementation/artifact-discovery-cleanup/c041-update-candidate-spawn-etxtbsy-corrective.md` | `update` can fail with `ETXTBSY` before the commit. A bounded retry shipped in v0.2.11. The root cause, a deterministic reproduction, and a green gate under load are unmet. |
| C036 | proposed | `plans/implementation/artifact-discovery-cleanup/c036-cli-surface-truthfulness-and-documentation-drift-corrective.md` | No implementation authority. It is the home for a newly found documentation defect. Do not open a second one. |
| C010 | proposed | `plans/implementation/distribution-release-update/c010-eggup-curl-user-agent-seam.md` | Upstream. It cannot be closed from this repository, and nothing here waits on it. |

## Not open

M018 and M019 shipped in v0.2.11, and M019 was not published before M018.
C042, C043, C044, C045, and C046 shipped in that same release. M014 through
M017 and C037 through C040 shipped by v0.2.10. C035 is the rejected
mechanism: do not merge that branch. No Phase 14 milestone exists. Shared
cache collection, and stronger provenance than the shipped attestation, each
need their own plan. An idle roadmap is not permission to start one.

The distribution line has no open milestone. v0.2.13 replaced the
unreleasable v0.2.12, and v0.2.14 carries C047, C048, M021, and M022. Its
GitHub release is immutable with a valid attestation, crates.io sha256 is
`ee472f81…`, automatic smoke `42885742223` is green on five targets, and the
receipt is `plans/closure/distribution-release-update/r033-status.md`.

## Current cleanup rule

ADR 001, amended 2026-11-10, is the rule. The older "only `PrivateBounded`
is cleanable" sentences in the long-term spec, the terminology, ADR 003 §6,
and M005 are historical and are marked that way.

- Deletion is `cargo clean`. This crate does not remove a directory itself.
- An idle `PrivateBounded`, `Shared`, or `ExternalUnproven` artifact
  directory is cleanable. `Uncertain` — a source tree, a symlink, or a path
  that could not be identified — is never cleaned.
- One active owner protects the whole physical directory.
- The discovery universe may only grow. Narrowing it can hide a co-owner.
- A stale unresolved manifest does not block. A recent or unstattable one
  does. A named output claim holds only intersecting units (ADR 010).
- `allowed_output_roots` authorizes. It cannot make an `Uncertain` path
  cleanable, and it does not manufacture ownership proof.
- Learned state is advisory. A persistence failure does not change the exit
  code.
- There are three exit codes: `0`, `1`, and `2`.

## The lesson this repository keeps re-learning

A green test proves only that its own premises hold. A plan that finds a
defect writes a corrective. It does not fix the defect under the plan that
found it, and it does not edit a closed plan or a closed closure record.
