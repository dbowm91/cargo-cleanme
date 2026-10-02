# cargo-cleanme Planning and Agent-Handoff Process

Status: normative planning governance

This repository uses a scaled version of the CodeGG planning convention: stable long-term direction is separated from repository-baseline implementation handoffs and from closure evidence.

## 1. Planning horizons

Long-term planning defines product behavior, terminology, invariants, safety boundaries, and roadmap ordering.

Interim planning defines bounded implementation work against a specific repository baseline.

Implementation evidence may reveal that long-term direction should change, but an implementation agent MUST NOT silently rewrite durable requirements to fit an easier implementation.

## 2. Canonical documents

Canonical long-term documents are:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

Ordinary implementation work should not edit these unless repository evidence demonstrates a contradiction/material omission or the product direction is intentionally changed.

## 3. Subsystem roadmaps

A subsystem roadmap owns one coherent workstream and MUST state:

- ownership boundary;
- invariants and non-goals;
- repository/current-state evidence;
- dependency graph;
- ordered milestones;
- verification direction;
- risks and deferred work.

cargo-cleanme is intentionally small, so the initial scanner and later cleanup share one subsystem roadmap until evidence justifies a split.

## 4. Milestone implementation plans

Each implementation plan is a bounded coding-agent handoff.

A plan MUST contain:

- repository baseline;
- source roadmap/milestone;
- objective and non-goals;
- current implementation evidence;
- invariants;
- production changes;
- ordered work packages;
- failure/cancellation/contention semantics as applicable;
- compatibility/configuration effects;
- required tests;
- verification commands;
- documentation updates;
- acceptance criteria;
- stop conditions;
- closure evidence requirements.

Plans may name likely modules but SHOULD preserve implementation freedom where several mechanics satisfy the contract.

## 5. Closure records

After implementation, plans/closure/<subsystem>/<NNN>-status.md determines whether the milestone is actually closed.

A closure record MUST contain:

- implementation commit or PR;
- requirement-to-evidence matrix;
- verification commands actually run and their results;
- platform/fixture evidence required by the milestone;
- known limitations;
- unresolved findings classified by severity;
- disposition: closed, conditionally closed, corrective required, or blocked.

Compilation alone is not closure.

## 6. Corrective passes

A material defect found after a milestone implementation receives a separate corrective plan. Do not edit history to imply the original milestone fully succeeded.

Corrective plans MUST identify:

- the original plan and closure record;
- each unclosed requirement/defect;
- why previous verification missed it;
- regression evidence required to prevent recurrence.

## 7. Status vocabulary

Use:

- proposed — direction exists but is not implementation-ready;
- ready — dependencies/contracts are satisfied;
- active — implementation is in progress;
- blocked — a named dependency/evidence requirement prevents progress;
- closing — implementation landed and closure evidence is being gathered;
- closed — closure record accepted;
- conditionally closed — substantial work landed but a named qualification remains;
- superseded — replaced by a newer plan;
- archived — retained only for history.

## 8. Dependency types

Milestones may declare:

- hard dependency — implementation cannot correctly begin before closure;
- interface dependency — work can proceed against a stable written contract;
- soft dependency — parallel work is possible but integration depends on another milestone;
- operational dependency — code can land but release/claim requires external evidence.

## 9. Milestone sizing

Prefer a vertical slice one implementation agent can complete coherently: production code, focused tests, broad verification, docs, and a closure-oriented report.

A milestone is too large when it combines separately releasable behavior or unresolved safety decisions.

A milestone is too small when it only renames/reshuffles internals without closing a useful contract, unless it is a corrective action.

## 10. Handoff authority order

Implementation agents should resolve conflicts in this order:

1. canonical specification and terminology;
2. accepted ADRs, if any;
3. subsystem roadmap;
4. milestone implementation plan;
5. current repository evidence.

When current code contradicts the plan, preserve canonical invariants and record the discrepancy. Do not invent a new destructive behavior to complete a checklist.

## 11. Active registry

plans/registry.md is the compact control surface and should contain only:

- active subsystem roadmap(s);
- ready/active/blocked implementation plans;
- closure records/corrective work when they exist;
- immediate blockers and handoff notes.

Detailed requirements belong in source plans, not duplicated in the registry.

## 12. Required planning review

Before a plan is marked ready, confirm:

1. durable behavior is unambiguous;
2. destructive behavior is not introduced ahead of its dependency;
3. scope/filter precedence is explicit;
4. filesystem failure semantics are explicit;
5. concurrency/parallelism does not weaken correctness;
6. cross-platform behavior is testable;
7. configuration compatibility is defined;
8. verification is specific;
9. closure evidence is measurable;
10. stop conditions prevent silent scope expansion.
