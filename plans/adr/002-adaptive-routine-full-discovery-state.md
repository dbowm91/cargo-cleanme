# ADR 002 — Adaptive Routine Discovery, Exhaustive Reconciliation, and Machine-Local Discovery State

Status: accepted

Date: 2026-10-03

Repository baseline: `ca224407188e39eb7fc1ac5f83d4f9c8b82a2d01`

Scope: M006 performance hardening and subsequent full/known cleanup orchestration

## Context

M006A, M006C, and M006D preserved cargo-cleanme's historical no-argument full-machine discovery contract while improving platform pruning, path identity, worker bounds, and traversal overhead. The exact reference macOS scan still exceeded 120 seconds. M006D measured roughly 4.64 million visited entries at the timeout while validating about 1,800 Cargo.toml candidates consumed only about 0.13 seconds. Traversal breadth, not Cargo manifest validation or report finalization, is the remaining dominant cost.

Routine developer use and exhaustive machine reconciliation have different requirements:

- routine use should quickly revisit the places where a developer actually keeps Rust work;
- exhaustive discovery should remain capable of finding projects in unusual locations;
- exhaustive discovery is useful even when expensive because it can refresh the machine's Rust-project inventory and teach later routine scans where to look;
- mutable learned discovery state is not user-authored policy and must not be mixed into config.toml;
- learned discovery state must never become evidence for destructive ownership or cleanup authorization.

The current proposed M006E framed the remaining choice as full scope versus latency. This ADR resolves that choice by supporting both as explicit product modes.

## Decision

### 1. Three discovery intents are distinct

cargo-cleanme recognizes three discovery intents:

1. **Routine** — the no-argument/default scan over a bounded adaptive root set.
2. **Full** — explicit exhaustive machine reconciliation, requested by `scan --full`.
3. **Explicit** — a user-provided scan root; it remains authoritative and bypasses routine-root learning.

"Routine" is shallow in scope only. Once a routine root is selected, recursive discovery beneath it remains complete and has no arbitrary depth limit.

### 2. Routine roots are seeds plus learned developer roots

The routine root set is formed from:

- a conservative set of existing platform/user seed directories;
- user-configured additive routine roots when configured;
- active learned roots from machine-local discovery state.

Roots are normalized once, canonical-equivalent roots are deduplicated, and contained roots collapse beneath an already-selected ancestor.

Broad roots such as filesystem roots, the user's home directory itself, /Users, /home, or equivalent platform domains are not learned merely because they contain a project.

### 3. Full discovery preserves exhaustive reachability

`scan --full` uses the existing platform exhaustive-discovery policy and safety prunes established by M006A-M006D. Learned-root retention does not remove locations from Full scope.

A location expired from Routine scope can therefore be rediscovered by a later Full scan if a Rust project appears there again.

Explicit roots remain unaffected by routine/full retention policy.

### 4. Full discovery records exact projects and derives learned roots

A successfully resolved Cargo workspace contributes:

- its canonical workspace/project identity to the exact project inventory;
- a candidate learned search container, normally the workspace root's direct parent.

Learned-root derivation must refuse promotion across broad/platform boundaries. Where promoting the parent would make Routine scope unreasonably broad, the workspace root itself is retained instead.

Derived learned roots are canonicalized, sorted, deduplicated, and containment-collapsed.

Exact project inventory and learned roots are separate state concepts.

### 5. Learned roots carry positive-observation timestamps

Each learned root records at least `last_project_seen_at`.

The default retention is 30 days and is configurable through config.toml.

Positive observations during Routine, Full, or Explicit read-only scans may advance the last-seen timestamp for a matching learned root.

Only a successfully completed Full reconciliation may expire learned roots from absence/age. Routine or Explicit scans never remove learned roots from negative evidence.

Future timestamps fail conservative: they do not cause expiration.

### 6. Partial or uncertain Full scans do not prune learned state

A cancelled, timed-out, failed, or otherwise incomplete Full scan must not publish a pruned discovery-state generation.

If a completed Full scan has permission/I/O uncertainty intersecting an existing learned root, absence beneath that uncertain coverage is not proof that the root is empty of Rust projects; the affected learned root is retained.

The next state generation is built in memory and atomically replaces the old state only after reconciliation succeeds.

### 7. Learned state is separate from configuration

config.toml contains user-authored policy.

Machine-learned project/root inventory is stored in a versioned machine-local state file using `ProjectDirs::state_dir()` where available and `data_local_dir()` as the platform fallback.

The initial state format is JSON because it is internal/versioned state, not a hand-edited contract.

A corrupt supported-version state file is fail-soft for read-only scanning: warn and fall back to seed/configured roots. A state file with a newer unsupported schema version is not overwritten by an older binary.

### 8. Learned state is never cleanup proof

A learned root or exact project record only answers where cargo-cleanme should search.

It does not establish:

- current Cargo workspace membership;
- current output roots;
- exclusive output ownership;
- authorization;
- inactivity;
- cleanup completeness.

C003/C004 fresh complete-universe proof remains authoritative before every destructive candidate disposition.

### 9. Configuration self-provisions on first operational use

A missing config.toml is atomically created from the checked-in canonical template when an operational command needs configuration.

An existing malformed config is an actionable error and is never overwritten automatically.

`config path` remains side-effect free.

The standalone `config init` command is removed rather than retaining two initialization mechanisms.

### 10. `config edit` opens the effective config in the user's editor

`cargo-cleanme config edit` ensures the effective config exists, then opens that exact path and waits for the editor to exit.

Editor resolution follows the established snip-it/eggpool convention:

1. non-empty `$VISUAL`;
2. non-empty `$EDITOR`;
3. first available fallback from `hx`, `vim`, `vi`, `nano`.

Editor command specifications may contain arguments (for example `code --wait`). They are parsed into a program/argument vector and launched directly without shell evaluation.

A non-zero editor exit is an error. After a successful editor exit, cargo-cleanme validates the resulting TOML and reports an actionable parse/config error if invalid; it does not silently replace or discard the user's edits.

### 11. Exhaustive cleanup is a separate capability

M006E changes discovery only. Existing `clean ROOT` semantics remain bounded and authoritative.

A later milestone may use learned/full discovery to orchestrate multiple bounded cleanup scopes. That milestone must preserve C003/C004 proof freshness and may not authorize one machine-wide Cargo cleanup from cached discovery state.

`--dryrun` retains its existing meaning: full cargo-cleanme cleanup simulation with zero Cargo clean processes. Read-only `scan --full` does not gain a redundant `--dryrun` spelling.

## Consequences

Positive:

- routine latency scales with the developer's actual working set rather than host filesystem size;
- exhaustive discovery remains available and becomes useful as periodic reconciliation;
- unusual project locations teach later routine scans automatically;
- the 30-day default limits stale routine roots without creating permanent blind spots;
- configuration remains human-owned while learned state can evolve independently;
- destructive safety is unchanged.

Costs:

- cargo-cleanme gains a small persistent state format and migration/versioning responsibility;
- Full scans remain expensive by design;
- root derivation and uncertainty-aware retention need cross-platform fixtures;
- the CLI/config contract changes before 0.1.0 release.

## Rejected alternatives

### Keep no-argument full-machine traversal and only relax the latency target

Rejected because it leaves routine interaction proportional to unrelated filesystem breadth and ignores the measured traversal bottleneck.

### Narrow global discovery permanently

Rejected because unusual/new project locations would become invisible unless manually configured.

### Rewrite learned roots into config.toml

Rejected because machine observations are mutable state, not user-authored policy, and automatic config rewrites would create surprising diffs and migration conflicts.

### Cache project paths only

Rejected because exact paths cannot discover a newly created sibling project. Learned parent containers preserve neighborhood discovery.

### Let routine scans expire roots after no match

Rejected because a root could disappear from the search set before the scan that would have rediscovered a new project beneath it.

### Treat learned state as cleanup ownership evidence

Rejected because cached discovery cannot satisfy C003/C004 freshness, completeness, or output-ownership proof.
