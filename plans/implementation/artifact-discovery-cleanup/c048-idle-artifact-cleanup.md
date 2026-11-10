# C048 — Idle artifact directories are cleanable

Status: **closing** (implemented, not published)

Date: 2026-11-10

Owner-directed the same day. Amends ADR 001 decisions 6–8, ADR 007 item 4,
and ADR 010.

## Decision

A target directory is rebuildable. Cleanup authorizes it when it is an
artifact directory and every project that uses it is idle.

- `PrivateBounded`, `Shared`, and `ExternalUnproven` are artifact classes.
- `Uncertain` stays uncleaned: source trees, conventional source directory
  names, symlinks, and paths whose identity could not be established.
- One active owner protects a shared directory.
- A stale unresolved manifest does not block and does not hold an
  intersecting unit. A recent or unstattable one still does.
- Deletion stays `cargo clean`. One physical directory is cleaned once.

## Closure

Not closed. Publication is not closure, and no release is authorized.
