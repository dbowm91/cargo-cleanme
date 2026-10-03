# M006E — Adaptive Routine Discovery and Full Reconciliation State

Status: conditionally closed

Closure: `plans/closure/artifact-discovery-cleanup/006e-status.md`. The Routine/Full/Explicit split and state/config surfaces landed. Full traversal completed on the reference host, but a complete error-free reconciliation did not publish a state generation; qualification remains open for reliable uncertainty-scoped reconciliation and state publication.

Repository baseline: `52018219860d3a5412929a848a142236de78ca58`

Source milestone: M006 performance hardening

Architecture decision: `plans/adr/002-adaptive-routine-full-discovery-state.md`

Primary class: performance / discovery policy / persistent local state / configuration UX

## 1. Objective

Replace no-argument full-machine traversal with a fast adaptive Routine scan while preserving an explicit exhaustive Full scan that refreshes a machine-local Rust-project inventory and learned developer roots.

M006E also removes the separate config-init workflow, self-provisions the canonical config on first operational use, and adds `config edit` with the editor behavior already used by snip-it/eggpool.

This milestone changes discovery and configuration UX only. It does not change Cargo cleanup authority.

## 2. Triggering evidence

M006D's native reference run exceeded 120 seconds after visiting about 4.64 million entries. About 1,800 Cargo.toml candidates were revalidated in roughly 0.13 seconds. Increasing worker count and switching metadata/order variants did not close the gap. The remaining cost is exhaustive traversal breadth.

The repository already provides:

- dua-core 4.1.0 multi-root bounded traversal;
- platform exhaustive roots and OS/tool-managed prunes;
- Cargo-authoritative workspace resolution;
- deterministic output and progress;
- config path/bootstrap primitives;
- serde/serde_json and directories dependencies.

M006E should reuse those boundaries rather than introduce an index/database service.

## 3. Invariants

- Routine, Full, and Explicit scan intents are distinguishable in the domain/policy layer.
- Explicit scan roots remain authoritative and bypass adaptive root selection.
- Full scan remains capable of rediscovering a project in a location absent from learned state.
- No directory symlink is followed.
- Existing platform/Cargo/rustup/target/VCS safety prunes remain intact for Full discovery.
- Routine root retention never narrows Full or Explicit scope.
- Learned state never authorizes cleanup or substitutes for Cargo resolution.
- C003/C004 cleanup safety is unchanged.
- A partial/failed Full scan cannot expire learned roots.
- Existing malformed config is never silently replaced.
- Rust 1.89 remains the MSRV.

## 4. Work package A — Domain and policy split

Introduce explicit internal scan intent, for example:

- `ScanIntent::Routine`
- `ScanIntent::Full`
- `ScanIntent::Explicit(PathBuf)`

Exact names may differ.

CLI contract:

- `cargo-cleanme` -> Routine
- `cargo-cleanme scan` -> Routine
- `cargo-cleanme scan --full` -> Full
- `cargo-cleanme scan ROOT` -> Explicit
- `scan ROOT --full` must be rejected as contradictory.

Preserve the existing `scan.root` compatibility contract as an exclusive configured root during this milestone. If present and no CLI root/full flag is supplied, it yields Explicit intent rather than being reinterpreted as additive.

Routine roots are assembled separately from the Full global policy.

## 5. Work package B — Conservative platform seed roots

Add a testable routine-seed policy.

Seed roots are existing directories only and should be conservative developer-location conventions rather than broad filesystem domains.

Expected home-relative candidates include a bounded set such as:

- Projects/projects
- Developer
- dev
- Code/code
- src
- repos/Repos
- GitHub/github
- workspace/workspaces

Windows may additionally include conventional user source/repository directories when they can be derived without scanning a broad profile root.

Do not seed:

- filesystem/drive roots;
- the user home directory itself;
- /Users or /home;
- /Applications, /Library, /opt, /Volumes;
- arbitrary Documents/Desktop trees.

The exact seed set must be documented and fixture-tested. Missing seeds are simply omitted.

## 6. Work package C — Versioned discovery state

Add a dedicated state module.

Resolve the state path with `ProjectDirs`:

1. `state_dir()` when available;
2. otherwise `data_local_dir()`;
3. if no suitable path exists, operate without learned state and emit a bounded diagnostic.

Use a versioned JSON document. Initial state should represent at least:

- schema version;
- last successful Full reconciliation timestamp;
- exact Cargo workspace/project records;
- learned roots;
- per-learned-root `last_project_seen_at`;
- enough provenance/coverage information to reconcile retention safely.

Do not store output ownership or cleanup authorization in this state.

State writes must be atomic: write/flush a sibling temporary generation then replace the destination. Avoid adding SQLite or a daemon.

A newer unsupported schema version is read-only/unavailable to the older binary and must not be overwritten. Corrupt supported-version state is fail-soft for read-only scanning: warn and fall back to seed/configured roots.

## 7. Work package D — Learned-root derivation

After Cargo-authoritative workspace resolution, record canonical workspace roots as exact project inventory.

Derive a routine learned root normally from each workspace root's direct parent, then apply broad-root guards.

Never promote a learned root to:

- filesystem/drive root;
- the user's home directory itself;
- a generic multi-user home parent;
- an OS-managed/protected domain that routine scanning intentionally excludes.

When parent promotion is unsafe/broad, retain the workspace root itself.

Canonicalize once, sort, deduplicate, and containment-collapse learned roots before persisting and before routine traversal.

Tests must include:

- many projects under /projects -> one /projects learned root;
- nested workspace neighborhoods collapse deterministically;
- one project directly beneath home does not cause the whole home to be learned;
- canonical/symlink aliases do not duplicate roots;
- Windows drive/path normalization.

## 8. Work package E — Positive observation and 30-day retention

Add config:

```toml
[scan]
learned_root_retention_days = 30
```

Validate it with a bounded integer range and a default of 30 days.

Semantics:

- positive project observations from Routine, Full, or Explicit scans may advance matching learned-root last-seen timestamps;
- Routine and Explicit scans never delete/expire roots from negative evidence;
- only a successfully completed Full reconciliation may expire a learned root;
- expiration requires `now - last_project_seen_at > retention`;
- future timestamps retain the root;
- roots intersecting uncertain Full coverage are retained;
- setting retention to zero, if supported, must have an explicit documented meaning rather than accidental immediate deletion.

Build the next Full state generation in memory and publish it only after discovery/resolution/reconciliation completes successfully.

A cancelled, timed-out, or fatal Full scan leaves the previous generation intact.

## 9. Work package F — Routine traversal assembly

Routine roots = existing seed roots + active learned roots, plus any future additive user roots introduced by a later config migration.

Canonicalize/deduplicate/containment-collapse before walking.

Use the existing shared bounded multi-root traversal.

Routine traversal remains recursively complete under selected roots; do not impose a directory depth limit.

If no Routine roots exist, return a successful empty inventory with a diagnostic suggesting `scan --full` rather than falling back to a machine-wide walk.

## 10. Work package G — Automatic config bootstrap

Refactor config loading into explicit semantics:

- `config path`: resolve/print only; no filesystem mutation;
- operational commands requiring config: ensure canonical config exists, then load;
- `config show`: ensure/load and print;
- `config edit`: ensure, edit, validate;
- existing config: never overwritten automatically;
- malformed existing config: fail with the current actionable parse error.

Use create-new/atomic semantics so concurrent first launches cannot truncate or replace each other's config.

Remove the public `config init` command, tests, README examples, and normative references. The repository template remains the single source of truth for first-run creation.

## 11. Work package H — `config edit`

Add:

`cargo-cleanme config edit`

Behavior:

1. resolve the effective config path, honoring `--config PATH`;
2. ensure the canonical config exists;
3. resolve an editor;
4. launch it directly with the config path as the final argument;
5. wait for completion;
6. on successful editor exit, reload/validate the edited config;
7. return success only if the editor exited successfully and the config validates.

Editor precedence:

1. non-empty `VISUAL`;
2. non-empty `EDITOR`;
3. first runnable fallback from `hx`, `vim`, `vi`, `nano`.

Support editor arguments such as `code --wait` and `nvim -f` using shell-word parsing only to split the environment specification. Never invoke a shell.

Absolute, relative-with-directory, and PATH-resolved editor programs should receive clear not-found/not-file errors. Cross-platform PATH resolution must account for Windows executable suffixes.

A non-zero editor exit is an error. If the editor modified the file and exits non-zero, do not silently restore old bytes.

If the edited TOML is invalid, leave the user's file untouched and report the parse/validation error plus the config path so the user can reopen it.

Tests must use fake editor executables/scripts and never launch an interactive editor in CI.

## 12. Work package I — State inspection surface

Add a minimal non-mutating inspection surface if it remains small:

- `state path`
- `state show`

or an equivalent config-adjacent command group.

It should expose exact projects, learned roots, timestamps, and last successful Full scan without exposing cleanup proof internals.

If adding this surface materially expands M006E, defer it, but tests must still make state contents inspectable through library APIs.

## 13. Required tests

### CLI/policy

- no command and `scan` resolve Routine;
- `scan --full` resolves Full;
- `scan ROOT` resolves Explicit;
- ROOT + --full conflicts;
- existing scan.root remains exclusive/explicit;
- direct and `cargo cleanme` external-subcommand forms remain equivalent.

### Seeds/routine roots

- each platform seed policy is fixture-testable;
- missing seeds are omitted;
- broad roots are never seeded;
- seed + learned overlap collapses to one walk root;
- routine walk is recursively complete under a selected root.

### State

- empty/missing state;
- valid v1 round-trip;
- atomic replacement;
- malformed state fail-soft;
- newer schema not overwritten;
- exact projects and learned roots remain distinct;
- positive observation advances timestamp;
- Routine negative observation does not expire;
- successful Full expires only old/absent roots;
- partial/failing Full publishes no pruned generation;
- uncertain coverage retains intersecting learned roots;
- future timestamp retains root;
- canonical alias deduplication.

### Config/bootstrap/editor

- first operational run creates exact template bytes;
- concurrent create race loads the winner without truncation;
- config path does not create;
- malformed existing config is not replaced;
- config init no longer parses;
- config edit creates missing config;
- VISUAL wins over EDITOR;
- EDITOR works when VISUAL absent;
- fallback order is deterministic;
- editor args are preserved without a shell;
- non-zero exit propagates;
- successful invalid TOML reports validation failure and preserves bytes;
- Windows fake-editor coverage.

### Regression

- all M001-M006D discovery/filter/symlink parity fixtures remain green where semantically applicable;
- clean ROOT Preview/Simulate/Execute behavior remains byte/decision compatible;
- Rust 1.89 check/test and hosted Linux/macOS/Windows CI.

## 14. Performance qualification

Define separate measurements:

### Routine

Use a representative learned/seed root set. Record:

- roots selected;
- visited entries;
- manifests/workspaces found;
- wall/user/system time;
- memory.

The target should be interactive/routine rather than reusing the old 120-second whole-machine acceptance. Establish the numeric gate from the post-implementation matched baseline and record it in closure rather than guessing before the new scope exists.

### Full

Require successful completion and state reconciliation on the reference host. Record wall/user/system time and visited entries, but do not preserve the old 120-second limit as a product requirement.

M006F owns further exhaustive walker optimization/qualification.

## 15. Documentation

Update:

- canonical specification;
- terminology/domain model;
- long-term roadmap;
- subsystem roadmap;
- registry;
- README;
- config.toml comments/examples.

Document clearly that a Routine scan can miss a project in a never-learned unusual location until `scan --full` or an Explicit scan discovers it.

## 16. Acceptance criteria

M006E closes when:

- Routine/Full/Explicit contracts are implemented and fixture-qualified;
- first-run config creation and `config edit` work cross-platform;
- Full completion transactionally refreshes exact project + learned-root state;
- 30-day default retention is configurable and conservative;
- Routine scans use only bounded adaptive roots and never silently fall back to whole-machine traversal;
- learned state cannot enter cleanup ownership/authorization decisions;
- matched Routine performance evidence is materially faster than the former global default;
- Full scan completes successfully on the reference host and records its state;
- Linux/macOS/Windows and Rust 1.89 gates pass.

## 17. Stop conditions

Stop and write a corrective plan instead of weakening semantics if:

- reliable Full completion cannot be distinguished from partial traversal;
- root-coverage uncertainty cannot be represented well enough to avoid unsafe expiration;
- platform state/config paths cannot be kept distinct;
- implementation pressure suggests using cached state as cleanup authority;
- Routine root derivation requires scanning broad domains just to decide where to scan.
