# Corrective C005 — Uncertainty-Aware Discovery-State Reconciliation and Config/Edit Hardening

Status: closing

Repository baseline: `cc37838b5a99298ae56e3d797b1ef2e4d345c4b3`

Corrects:

- M006E adaptive Routine/Full discovery and state publication
- M006E closure remains historical and is not rewritten
- M007 remains blocked until C005 closes

Authoritative references:

- `plans/adr/002-adaptive-routine-full-discovery-state.md`
- `plans/implementation/artifact-discovery-cleanup/006e-adaptive-routine-full-discovery-state.md`
- `plans/closure/artifact-discovery-cleanup/006e-status.md`

Primary class: corrective / persistent discovery state / conservative uncertainty handling / cross-platform config UX

## 1. Objective

Replace M006E's current all-or-nothing Full-state publication gate with coverage-aware reconciliation that can publish positive project evidence while conservatively retaining learned roots whose absence cannot be trusted.

Also close the remaining state/config-editor hardening gaps:

- persist containment-collapsed learned roots;
- surface corrupt/unsupported state diagnostics rather than silently swallowing them in Routine policy resolution;
- make state publication robust against same-process temporary-name collisions;
- qualify `config edit` with real fake-editor subprocesses across supported platforms;
- use a proven editor-command parser/resolver that preserves quoted arguments and Windows paths.

C005 does not change cleanup ownership or authorization. Discovery state remains a search optimization only.

## 2. Triggering findings

The reference Full scan completed traversal over about 4.67 million entries and found 1,803 Cargo manifests, but the command produced 741 diagnostics and 283 Cargo-resolution failures. Current code computes:

```text
uncertain_full = full && !diagnostics.is_empty()
```

and publishes no Full generation whenever any diagnostic exists.

That condition is too coarse for a real whole-machine scan. Permission errors, disappearing paths, malformed/old Cargo projects, offline metadata failures, and unrelated filesystem diagnostics can coexist with trustworthy positive evidence elsewhere.

The result is that a successful exhaustive traversal can become permanently unable to teach Routine discovery.

## 3. Safety model

C005 separates three concepts that are currently conflated:

1. **positive discovery evidence** — a Cargo.toml or Cargo-resolved workspace was actually observed;
2. **negative coverage evidence** — a path region was searched sufficiently to conclude that no project was observed there;
3. **cleanup ownership proof** — C003/C004 fresh Cargo resolution and physical output graph proof.

Only (1) and (2) participate in discovery-state reconciliation.

Discovery state MUST NOT be used for (3).

A Full scan may publish useful positive state even when some regions are uncertain. Uncertainty only limits what may be expired from learned state.

## 4. Project-observation model

State schema v2 SHOULD distinguish exact project observations from their resolution quality.

A project observation must record at least:

- canonical project/workspace or manifest-root path;
- canonical manifest path where available;
- observation timestamp;
- resolution status sufficient to distinguish:
  - manifest observed but Cargo workspace resolution failed;
  - workspace resolved authoritatively.

Exact type names are implementation choices.

A discovered regular non-symlink `Cargo.toml` is positive evidence that its containing directory is Rust/Cargo-bearing even if Cargo metadata later fails.

Successful Cargo metadata may replace/collapse member observations into canonical workspace identities for inventory presentation, but failed resolution MUST NOT erase the manifest observation.

Do not record manifests from trees pruned by the established Full policy.

## 5. Coverage uncertainty model

Introduce a Full-reconciliation coverage result that can answer whether absence beneath a learned root is trustworthy.

It should retain path-scoped uncertainty from at least:

- traversal permission errors;
- metadata/read failures that prevented descent or enumeration;
- disappearing directory boundaries where child coverage became unknown;
- fatal/cancelled traversal sections;
- platform-specific root enumeration failures.

Cargo workspace resolution failure alone does NOT mean the filesystem region was unsearched. It prevents an authoritative workspace record but does not invalidate the positive Cargo.toml observation.

Represent uncertainty as canonical path prefixes or an equivalent compact structure.

A learned root intersects uncertainty when the uncertain path is:

- equal to the root;
- an ancestor of the root; or
- a descendant of the root whose failed traversal means part of that root was not covered.

Tests must lock down the exact containment rule.

## 6. Full reconciliation algorithm

A completed Full scan constructs the next state generation in memory.

For each positive project observation:

1. record/refresh its exact project entry;
2. derive its learned container using ADR 002 broad-root guards;
3. refresh that learned root's `last_project_seen_at`.

For each previously learned root not positively observed:

- if it intersects uncertain coverage, retain it unchanged;
- else if its timestamp is in the future, retain it;
- else if retention is disabled, retain it;
- else expire it only when its age is greater than the configured retention.

The generation MAY be published when the Full traversal itself completed, even if unrelated diagnostics or Cargo resolution failures exist.

A cancelled, timed-out, fatal-root-enumeration, or otherwise incomplete Full traversal MUST NOT publish retention pruning. It may either publish no generation or merge positive observations without deletions; choose one behavior and test it explicitly. The simpler initial contract is no publication on incomplete traversal.

## 7. Retention value semantics

Define the config contract explicitly:

- default `learned_root_retention_days = 30`;
- allowed range remains bounded;
- `0` means **disable learned-root expiration**, not immediate deletion.

This avoids an accidental destructive interpretation of a zero-duration retention setting.

Update template comments and validation tests.

## 8. Learned-root derivation and persistence

Derivation continues to prefer the project/workspace parent when that parent is an acceptable developer container.

Broad-root guards must include:

- filesystem/drive roots;
- user home itself;
- generic multi-user home parents;
- established OS-managed broad domains from ADR 002.

Before persistence:

1. canonicalize where possible;
2. sort;
3. deduplicate;
4. containment-collapse.

If `/projects` is stored, do not persist redundant `/projects/foo` or `/projects/foo/bar` learned roots.

Positive timestamps merged into a containing learned root use the maximum observed timestamp.

Add fixtures for multiple sibling projects, nested containers, home-direct projects, symlink aliases, and Windows drive roots.

## 9. State read/write hardening

### 9.1 State diagnostics

Routine policy resolution currently tolerates `load_default().ok().flatten()`, which suppresses corrupt/newer-schema errors.

Refactor so callers can distinguish:

- no state file;
- usable supported state;
- corrupt state;
- unsupported newer schema;
- state-path/read error.

Routine scanning remains fail-soft, but emits one bounded actionable diagnostic on stderr for corrupt/unavailable state and falls back to seed/configured roots.

An unsupported newer schema MUST NOT be overwritten by an older binary.

### 9.2 Atomic publication

Current temp naming uses the process ID only. Replace it with bounded create-new unique sibling attempts so concurrent/same-process publications do not fail merely because one temporary file already exists.

Publication requirements:

- complete bytes written before replacement;
- file sync before replacement;
- no truncated destination on failure;
- no stray temp files after handled failure;
- destination replacement remains same-filesystem atomic where supported.

Advisory locking is optional because discovery state is not safety proof. If no lock is introduced, document last-writer-wins semantics and ensure timestamps never regress within one reconciliation.

## 10. Config edit hardening

Retain the existing command contract:

`cargo-cleanme config edit`

Editor resolution:

1. non-empty `VISUAL`;
2. non-empty `EDITOR`;
3. first runnable fallback: `hx`, `vim`, `vi`, `nano`.

Replace the hand-written command splitter with a proven shell-word parser consistent with snip-it, or provide equivalent fixture-complete parsing.

Requirements:

- no shell execution;
- preserve editor arguments such as `code --wait`, `nvim -f`;
- preserve quoted paths containing spaces;
- preserve Windows path backslashes correctly;
- resolve bare program names via PATH;
- on Windows honor executable suffixes/PATHEXT or an equivalent platform resolver;
- reject missing/non-file explicit editor paths clearly;
- wait for editor completion;
- propagate non-zero exit;
- validate config after zero exit;
- invalid edited TOML remains on disk with an actionable error and path;
- `config path` remains side-effect free;
- `config edit` bootstraps the config if absent.

## 11. Required production changes

### A — Discovery evidence result

Extend discovery/Full orchestration so positive Cargo.toml observations remain available independently of successful workspace resolution.

Avoid changing ordinary ScanReport output unless useful; this evidence may be internal to state reconciliation.

### B — Coverage uncertainty

Preserve path-scoped traversal uncertainty from the walker into Full reconciliation.

Do not use one global `diagnostics.is_empty()` boolean as the publication/expiration gate.

### C — State schema v2 and migration

Support reading v1 state and migrating it in memory to v2.

Do not require users to delete existing state.

A v1 exact workspace record can be imported as a successfully resolved project observation.

Write only the current schema on successful publication.

### D — Reconciliation engine

Move reconciliation out of `main.rs` into a testable library function/module.

The engine should accept:

- prior state;
- positive project observations;
- uncertainty prefixes;
- Full completion status;
- current timestamp;
- retention config.

It returns either:

- a next publishable generation; or
- an explicit no-publication disposition with reason.

### E — Routine state loading

Return state diagnostics through policy/main rather than silently swallowing errors.

Routine root selection stays bounded and fail-soft.

### F — Editor resolver

Move editor parsing/resolution out of `main.rs` into a testable module or config helper.

Use fake editor executables/scripts for integration tests.

## 12. Required tests

### Full state reconciliation

- completed Full + zero diagnostics -> publish;
- completed Full + unrelated permission diagnostic outside learned roots -> publish positives and expire only trustworthy stale roots;
- completed Full + uncertainty intersecting old learned root -> retain root;
- completed Full + Cargo metadata failure for observed Cargo.toml -> record positive manifest observation and learned root;
- malformed Cargo.toml observed -> positive manifest/root evidence is retained with unresolved status;
- cancelled/incomplete Full -> no pruning generation;
- future learned-root timestamp -> retained;
- retention 0 -> never expires;
- retention 30 -> expires only after age >30 days with trustworthy negative coverage;
- sibling projects -> one collapsed parent learned root;
- persisted nested roots -> containment-collapsed;
- v1 state -> read/migrate/write v2;
- newer schema -> warning/fallback, no overwrite;
- corrupt JSON -> warning/fallback, no panic.

### State publication

- create initial state;
- replace state atomically;
- concurrent/same-process temp collision uses another unique temp;
- replacement failure preserves prior destination bytes;
- no temp residue on handled failure.

### Editor

- VISUAL wins over EDITOR;
- EDITOR used when VISUAL absent;
- deterministic fallback order;
- editor arguments preserved;
- quoted path with spaces;
- Windows-style quoted backslash path;
- PATH lookup;
- Windows executable suffix/PATHEXT lookup;
- missing editor error;
- non-zero editor exit;
- successful no-op edit;
- successful valid edit;
- successful editor exit + invalid TOML returns validation error and preserves edit;
- missing config is bootstrapped before editor launch.

### Regression

- Routine/Full/Explicit CLI semantics remain green;
- no symlink directory traversal;
- M006F traversal counters/manifest parity remain green;
- C003/C004 cleanup suites unchanged;
- Linux/macOS/Windows hosted CI;
- Rust 1.89 check/test.

## 13. Acceptance criteria

C005 closes when:

- a real reference Full traversal with ordinary localized diagnostics publishes a useful state generation;
- exact positive project evidence survives Cargo workspace-resolution failures;
- learned-root expiration is path-scoped and uncertainty-aware;
- containment-collapsed learned roots are persisted;
- retention 0 has documented/tested disable-expiration semantics;
- corrupt/newer state is visible but fail-soft;
- config edit has fake-editor cross-platform qualification;
- no learned state participates in cleanup ownership/authorization;
- hosted Linux/macOS/Windows and Rust 1.89 gates pass.

On closure, M006E may move from conditionally closed to closed if no other M006E acceptance item remains.

## 14. Stop conditions

Stop and write a narrower follow-up rather than weakening guarantees if:

- traversal diagnostics cannot be associated with useful path coverage;
- positive manifest observation cannot be retained without changing cleanup proof types;
- schema migration risks overwriting newer state;
- editor resolution would require shell execution;
- implementation attempts to use state observations as destructive ownership proof.
