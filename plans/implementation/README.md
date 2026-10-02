# Milestone Implementation Plans

This directory contains bounded plans intended for direct handoff to implementation agents.

Implementation plans are tied to a repository baseline and may be corrected, superseded, or archived without rewriting canonical long-term documents.

## Layout

~~~text
implementation/<subsystem>/NNN-short-title.md
~~~

## Required plan content

Every handoff plan should include:

1. status and repository baseline;
2. source roadmap/milestone;
3. canonical requirements;
4. objective;
5. readiness/dependencies;
6. current implementation evidence;
7. invariants;
8. in-scope and out-of-scope work;
9. required production changes;
10. ordered work packages;
11. failure/cancellation/restart/contention semantics where relevant;
12. compatibility/configuration effects;
13. focused and broad tests;
14. verification commands;
15. documentation updates;
16. acceptance criteria;
17. stop conditions;
18. closure evidence required;
19. handoff notes.

Implementation agents may adapt file-level mechanics to repository reality but may not weaken canonical safety invariants or silently add destructive behavior.

## Corrective plans

A material defect after implementation receives a new corrective plan in the same subsystem directory. The corrective plan references the original plan and closure record, enumerates unclosed findings, and adds regression evidence that would have detected them.
