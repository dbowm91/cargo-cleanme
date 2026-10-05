---
name: planning-and-closure
description: How to write and close a cargo-cleanme milestone — plans/ governance, the corrective-pass rule, closure evidence, and what must never be edited
version: 1.0.0
tags:
  - planning
  - closure
  - governance
  - roadmap
---

# Planning and Closure

`plans/` is the decision record. This skill covers how to add to it without
corrupting it. The rules are normative in
[`plans/003-planning-process.md`](../../plans/003-planning-process.md); this is
the operational version, including the traps that actually cost time here.

## When to Load

- Writing or editing anything under `plans/`
- Deciding whether work needs a new plan, a corrective plan, or neither
- Deciding whether a milestone is genuinely closed
- Asked to "update the docs" in a way that touches planning state

## The one thing to get right

**Compilation is not closure, and publication is not closure.** Both of those
mistakes actually happened in this repository:

- C014 published `v0.1.2` with every reproducibility requirement met and was
  still held **open**, because its live updater rehearsal failed. Publication
  alone does not close a plan whose point is live behavior.
- C016 and C017 were each *fixed and gated locally* but could not close until a
  published release carried the fix, because the defect was only observable
  against a real registry and a real installation.

A corrective that is fixed but not yet carried by a published release stays
open. Say so in the registry.

## Handoff authority order

When sources disagree, resolve in this order and record the discrepancy rather
than inventing new behavior to make a checklist pass:

1. `plans/000-long-term-specification.md` and `plans/001-terminology-and-domain-model.md`
2. Accepted ADRs in `plans/adr/`
3. The subsystem roadmap in `plans/subsystems/`
4. The milestone implementation plan
5. Current repository evidence

## What must never be edited

| Document | Why |
|---|---|
| `plans/000-*.md`, `001-*.md`, `002-*.md` | canonical direction; only edit on demonstrated contradiction or an intentional product change |
| A **closed** milestone plan | history. Correct it forward with a corrective plan |
| A **closed** closure record | same. Add an addendum naming the unsatisfied requirement, as M010C's record does |

C001–C009 and C010–C017 are all corrective passes. That is the pattern: a
material defect found after a milestone gets a **new** plan, and the original
closure record keeps the original verdict.

## The corrective-pass rule

A new corrective plan MUST identify:

1. the original plan and closure record it corrects,
2. each unclosed requirement or defect,
3. **why previous verification missed it** — the hardest and most valuable field,
4. regression evidence required to prevent recurrence.

Field 3 is where the real content is. C016's answer was: the identity check ran
the candidate with no arguments, so its stdout could never match a version
string — a premise that was never asserted. C012's answer was: the Windows
fixture had never executed, and was green only because an unrelated real
`cargo install` failed. If you cannot answer field 3, the defect is not yet
understood and the plan is not ready.

## Writing a closure record

`plans/closure/<subsystem>/<NNN>-status.md` must contain:

- implementation commit or PR
- a requirement-to-evidence matrix
- verification commands **actually run**, with results
- platform/fixture evidence the milestone required
- known limitations
- unresolved findings classified by severity
- a disposition: `closed`, `conditionally closed`, `corrective required`, or `blocked`

Do not write a success record because the crate compiles. See
[the closure README](../../plans/closure/README.md).

## Registering work

`plans/registry.md` is a **control surface**, not a second copy of the roadmaps.
Per §11 it holds only: active roadmaps, non-closed plans, immediate blockers,
and handoff notes. Per-milestone history belongs in
`plans/subsystems/*-roadmap.md`, which already carries a section per milestone
with its status and closure path.

When you close something, update **both** the roadmap section and the registry,
or they will disagree — which is the staleness this rule exists to prevent.

## Status vocabulary

Defined once, in §7 of the planning process: `proposed`, `ready`, `active`,
`blocked`, `closing`, `closed`, `conditionally closed`, `superseded`,
`archived`. Use those words and no others; do not redefine them locally.

Note that `conditionally closed` is not a softer `closed`. M006A, M006C, and
M006D hold real unclosed requirements — the exact native no-argument scan is
still over 120 seconds — and their closure records preserve that. Do not
quietly promote them.

## Sizing a milestone

Per §9, a milestone is a vertical slice one agent can complete coherently:
production code, focused tests, broad verification, docs, and a
closure-oriented report. Too large means it combines separately releasable
behavior or unresolved safety decisions. Too small means it only reshuffles
internals — unless it is a corrective.

## Related

- [`cleanup-safety`](../cleanup-safety/SKILL.md) — when the plan touches
  anything that can delete bytes
- [`test-evidence`](../test-evidence/SKILL.md) — what counts as verification
  evidence for a closure record
