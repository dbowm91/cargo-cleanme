# ADR 003 — Canonical Maintenance Invocation, Full Scan Front Door, and Unattended Output

Status: accepted

Date: 2026-10-05

Repository baseline: `e4e9d92673e5f10548248e23db788c0906760916`

Scope: Phase 12 maintenance UX, cleanup-mode reconciliation, and external scheduler integration

Supersedes: ADR 002 §§1, 3, and 11 only where they define the public invocation used to select Routine versus Full discovery. ADR 002's learned-state, retention, uncertainty, and "learned state is never cleanup proof" decisions remain authoritative.

## Context

cargo-cleanme began as a read-only inventory tool. Later milestones added Cargo-mediated cleanup, complete ownership coverage, fresh final-proof revalidation, adaptive Routine discovery, Full reconciliation, cleanup policy, machine-readable output, and distribution/update support.

The public command shape still reflects that history:

- bare `cargo cleanme` means a Routine read-only scan;
- `cargo cleanme scan --full` is Full reconciliation;
- destructive work requires `clean ... --yes`;
- `--dry-run` means Cargo's own preview while `--dryrun` means cargo-cleanme's true zero-mutation simulation.

That surface is safe but no longer matches the product's mature maintenance role. The common operator intent is now:

1. periodically reconcile the machine to learn unusual Rust development locations;
2. routinely clean inactive, privately owned Cargo output under bounded seed + learned roots;
3. let an external scheduler such as greggd decide when maintenance may run based on system load;
4. retain explicit commands for bounded/manual/selector cleanup and detailed inspection.

Gregg's scheduler already executes direct argv arrays, serializes jobs globally, can defer on cached load averages, and captures bounded stdout/stderr history. cargo-cleanme therefore does not need a daemon or scheduler. It needs a canonical maintenance command and a concise unattended output mode.

The current CLI also has a usability defect: two near-identical spellings, `--dry-run` and `--dryrun`, mean materially different things. The safer and more conventional meaning for `--dry-run` is cargo-cleanme simulation: run every decision/proof/report step but invoke no `cargo clean` command.

Changing bare invocation from read-only to destructive is intentionally breaking. It must not ship as an unnoticed patch-level behavior change.

## Decision

### 1. Bare invocation is the canonical Routine maintenance operation

The canonical command is:

~~~text
cargo cleanme
~~~

It selects the current Routine root set (bounded seed roots plus active learned roots, with configured policy), constructs the complete combined cleanup ownership universe, performs the same C003/C004/C006 fresh proof used by advanced cleanup, and executes eligible Cargo cleanup units.

Bare invocation does not delete directories directly. It delegates deletion only through Cargo after every existing ownership, authorization, activity, marker, policy, and final-proof gate passes.

Learned discovery state selects where to search. It remains advisory and never supplies cleanup proof.

### 2. Bare `--dry-run` is the canonical zero-mutation preview

~~~text
cargo cleanme --dry-run
~~~

runs the complete Routine cleanup decision path but invokes no `cargo clean` command, including no Cargo dry-run process.

This is the operation previously exposed as `--dryrun`.

The term **dry run** therefore means application simulation throughout the canonical CLI.

### 3. Cargo's own preview remains available under an explicit name

Cargo's `cargo clean --dry-run --verbose` behavior remains useful for debugging and compatibility qualification, but it is not the canonical dry-run meaning.

The advanced cleanup surface exposes it as:

~~~text
cargo cleanme clean ... --cargo-preview
~~~

A Cargo preview still passes the same complete ownership/freshness proof before any Cargo process is invoked.

### 4. Advanced cleanup executes by default

The explicit cleanup command remains for bounded roots, Full/known orchestration, policy overrides, and profile/package selectors:

~~~text
cargo cleanme clean ROOT
cargo cleanme clean --known
cargo cleanme clean --full
~~~

Absent a mode flag, these execute eligible cleanup units. `--dry-run` selects application simulation and `--cargo-preview` selects Cargo preview.

For migration compatibility, the implementation may accept the historical `--yes` as a hidden execute alias and `--dryrun` as a hidden simulation alias through the pre-1.0 line. They must not remain the documented canonical spellings.

### 5. `scan` becomes the canonical Full read-only reconciliation command

~~~text
cargo cleanme scan
~~~

with no ROOT performs Full read-only machine reconciliation under the existing platform policy.

~~~text
cargo cleanme scan ROOT
~~~

remains an Explicit bounded read-only scan.

The old no-root Routine read-only inventory remains available for advanced inspection as:

~~~text
cargo cleanme scan --known
~~~

The historical `scan --full` spelling may remain as a hidden compatibility alias for no-root `scan` during the pre-1.0 migration period.

Full reconciliation remains the only operation allowed to expire learned roots from absence/age. Routine cleanup and Routine read-only inspection may advance positive observations but do not prune learned roots.

### 6. The destructive safety boundary does not change

This ADR changes command intent and ergonomics, not what is cleanable.

All existing invariants remain:

- every discovered ownership participant in the selected bounded roots must be authoritatively covered;
- unresolved ownership blocks the complete cleanup scope;
- one CleanupUnit corresponds to one Cargo workspace invocation over its complete OutputSet;
- every affected physical group must be PrivateBounded, authorized, inactive, non-symlink, marker-qualified, and stable under fresh complete ownership-graph revalidation;
- Shared, ExternalUnproven, and Uncertain output remain inventory-only;
- selectors may narrow Cargo's mutation request only after complete ownership proof;
- no filesystem directory is recursively deleted by cargo-cleanme.

### 7. Add a bounded unattended log format

The public output selector gains:

~~~text
--format log
~~~

alongside `human` and `json`.

`log` is for cron, greggd scheduler history, journals, CI summaries, and other unattended operator surfaces. It is not a replacement for schema-versioned JSON.

For scan and cleanup operations, log mode must:

- emit exactly one deterministic ASCII summary line on stdout when an operation report exists;
- terminate it with exactly one newline;
- stay within a documented hard byte bound small enough to survive greggd's retained output tail without truncating the summary;
- emit no transient progress UI;
- emit no per-workspace/per-group rows;
- use stable short keys and typed reason/status codes rather than free-form paths;
- suppress normal diagnostic fan-out; report diagnostic counts in the summary instead;
- keep stdout empty for pre-report fatal errors and emit one bounded diagnostic line on stderr;
- preserve process exit semantics.

`json` remains the stable automation contract for software that needs complete typed detail.

### 8. External schedulers own time/load policy

cargo-cleanme does not gain an internal daemon, cron parser, load sampler, retry queue, or history store.

Documentation should show greggd as a first-class integration example:

- daily Routine cleanup: absolute path to `cargo-cleanme --format log`;
- weekly Full reconciliation: absolute path to `cargo-cleanme scan --format log`;
- load threshold, load window, retry interval, and max wait owned by greggd;
- direct argv, no shell wrapper;
- user-owned/rootless greggd for developer-home maintenance, because the system service account/sandbox generally cannot access a developer's home tree.

Other schedulers remain supported because the contract is ordinary process execution plus exit status/stdout/stderr.

### 9. This requires a breaking pre-1.0 release boundary

The first published release containing the new bare-invocation semantics must not be a `0.1.x` patch release.

It must make the behavioral break prominent in the changelog, README/usage migration notes, generated manpage/completions, and release notes.

No version number beyond "at least the next minor pre-1.0 boundary" is reserved by this ADR.

## Consequences

Positive:

- the shortest command performs the normal maintenance task;
- Full reconciliation becomes the obvious periodic discovery command;
- Routine learned roots are useful automatically rather than requiring an advanced cleanup spelling;
- one conventional `--dry-run` now means zero mutation;
- Cargo preview remains available without overloading that spelling;
- cron/greggd history receives concise useful output instead of a multi-row interactive report;
- cargo-cleanme remains scheduler-agnostic.

Costs:

- bare invocation changes from read-only to destructive and therefore requires a deliberate migration/release boundary;
- existing scripts using bare `cargo cleanme` for inventory must move to `cargo cleanme scan --known`, explicit `scan ROOT`, or JSON equivalents;
- scripts using `--dry-run` for Cargo preview must move to `--cargo-preview`;
- generated CLI artifacts, tests, docs, and architecture references require broad reconciliation;
- log mode adds another output contract that must remain bounded and deterministic.

## Rejected alternatives

### Keep bare invocation read-only and document `clean --known --yes` as the cron command

Rejected because it preserves historical ceremony in the canonical maintenance path and makes the adaptive Routine root machinery unnecessarily difficult to use.

### Make bare invocation clean but retain no-root `scan` as Routine and require `scan --full`

Rejected because the two canonical periodic jobs would remain asymmetrical and the expensive discovery operation would keep an unnecessary expert flag. `scan` is the natural name for reconciliation when cleaning owns the root command.

### Introduce a separate `cargo scan` external subcommand

Rejected because that would require a second installed Cargo external-subcommand binary/package namespace and would obscure that discovery state belongs to cargo-cleanme. The canonical spelling is `cargo cleanme scan`.

### Preserve `--dry-run` as Cargo preview and rename simulation

Rejected because users reasonably expect dry-run to mean "do not mutate anything." Cargo preview is a lower-level diagnostic mode and receives the explicit `--cargo-preview` name.

### Auto-select log output whenever stdout is non-TTY

Rejected because redirection must not silently change the output contract. `--format log` is explicit.

### Add scheduler/load semantics directly to cargo-cleanme

Rejected because greggd and other schedulers already own that responsibility, and duplicating it would add daemon/state/concurrency surface unrelated to Cargo cleanup safety.
