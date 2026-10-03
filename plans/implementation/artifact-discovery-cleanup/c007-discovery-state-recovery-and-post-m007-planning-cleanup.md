# Corrective C007 — Discovery-State Recovery and Post-M007 Planning Cleanup

Status: closing

Repository baseline: `cf7c1a39fa01d19dd23874b884056556d4384a16`

Corrects:

- post-C005 discovery-state recovery behavior
- post-C006/M007 roadmap and registry status
- stale implementation/planning branch hygiene after PR #6 merge

Authoritative references:

- `plans/adr/002-adaptive-routine-full-discovery-state.md`
- `plans/closure/artifact-discovery-cleanup/c005-status.md`
- `plans/closure/artifact-discovery-cleanup/c006-status.md`
- `plans/registry.md`
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Primary class: corrective / operational recovery / documentation reconciliation / repository hygiene

## 1. Objective

Close the remaining post-M007 operational and planning debt without reopening the completed adaptive-discovery or combined-cleanup architecture.

C007 has two bounded responsibilities:

1. make an explicit successful Full scan capable of self-healing unusable discovery state from older/corrupt local state formats while continuing to protect newer unsupported schemas from downgrade overwrite;
2. reconcile planning/documentation so the subsystem no longer advertises a closed milestone as the active handoff and stale superseded branches are clearly retired.

No cleanup ownership, authorization, traversal scope, learned-root retention, or config-edit semantics change in this corrective.

## 2. Triggering findings

C005 intentionally makes corrupt, schema-0, and unsupported-newer discovery state fail soft for Routine scans. Routine correctly warns and falls back to seed/configured roots.

However, the current Full path obtains prior state through `load_default()`. Any state-load error produces:

```text
discovery state was not changed
```

and bypasses reconciliation entirely.

That is too conservative for state that is clearly unusable and older than the current schema. A user carrying an invalid schema-0/corrupt file can run a complete trustworthy `scan --full` and still remain permanently on seed-only Routine discovery until manually deleting the state file.

The same behavior is correct for a state file with a schema newer than the running binary: an older binary MUST NOT overwrite state it cannot understand.

Separately, the active registry still names M007 as the current milestone even though M007 and both corrective passes are closed, and obsolete branches remain visible after the adaptive line merged.

## 3. State recovery policy

Classify discovery-state load outcomes explicitly:

- Absent
- Valid/migratable supported state
- Recoverable-invalid state
- Unsupported-newer state
- Unavailable/I/O error

### Recoverable-invalid state

Includes:

- schema 0;
- malformed/corrupt JSON;
- supported-schema data that fails structural validation and cannot be migrated.

Routine behavior remains fail-soft:

- emit one bounded warning;
- ignore the unusable state;
- use seed/configured Routine roots.

Full behavior:

- run exhaustive discovery normally;
- if Full traversal completes sufficiently for C005 reconciliation, build a new state generation from an empty prior state plus current positive observations/coverage;
- atomically replace the recoverable-invalid file with the new current-schema state;
- emit a concise diagnostic that prior unusable state was replaced after successful Full reconciliation.

A failed/incomplete Full scan MUST leave the unusable file untouched.

### Unsupported-newer state

Routine behavior:

- warn;
- do not use it;
- fall back to seed/configured roots.

Full behavior:

- MAY perform read-only inventory;
- MUST NOT replace, migrate, truncate, quarantine, or rename the newer state;
- report that state reconciliation was skipped because the running binary is older than the state schema.

### Unavailable/I/O error

Do not assume recoverability. If the path cannot be read due permissions or I/O failure, do not overwrite it opportunistically.

## 4. State-load API

Replace string-only state-load errors with a typed internal classification, for example:

```text
StateLoad {
    Missing,
    Loaded(DiscoveryState),
    RecoverableInvalid(StateProblem),
    UnsupportedNewer { schema, current },
    Unavailable(StateProblem),
}
```

Exact names are implementation choices.

Requirements:

- callers must not parse human-readable error strings to decide overwrite safety;
- error rendering remains concise and actionable;
- tests can directly assert recovery category;
- current v1 migration behavior remains supported;
- writes always use current schema.

## 5. Full reconciliation changes

The Full path should choose prior state as follows:

- Loaded -> reconcile against loaded state;
- Missing -> reconcile against empty state;
- RecoverableInvalid -> reconcile against empty state and mark replacement reason;
- UnsupportedNewer -> perform scan/report only; no state publication;
- Unavailable -> perform scan/report only; no state publication unless a separately proven safe path exists.

The existing C005 path-scoped uncertainty rules remain unchanged.

Successful replacement of recoverable-invalid state must still require:

- Full traversal completion;
- no fatal platform-root failure;
- successful construction of the next state generation;
- successful atomic publication.

Localized diagnostics remain eligible for C005 uncertainty-aware publication.

## 6. Recovery tests

Required fixtures:

- missing state + successful Full -> creates current schema;
- valid v1 + successful Full -> migrates/reconciles to current schema;
- valid current state + successful Full -> normal reconciliation;
- schema 0 + successful Full -> replaced with current schema;
- malformed JSON + successful Full -> replaced with current schema;
- schema 0 + incomplete Full -> original bytes preserved;
- malformed JSON + incomplete Full -> original bytes preserved;
- newer schema + successful Full -> original bytes preserved exactly;
- newer schema + incomplete Full -> original bytes preserved exactly;
- permission/read I/O failure -> no overwrite attempt;
- replacement failure -> prior bytes/path remain intact;
- Routine with recoverable-invalid/newer state -> seed fallback with one warning.

Where feasible, cover the real CLI path rather than only the reconciliation helper.

## 7. User-visible documentation

Update README/config/state documentation to state:

- discovery state is machine-local and normally self-managed;
- Routine scans fall back safely if state is unreadable;
- a successful `scan --full` repairs corrupt/obsolete local state when it is safe to do so;
- a newer unsupported state is intentionally preserved and requires upgrading cargo-cleanme rather than forcing a downgrade rewrite;
- manual state deletion should not be the normal recovery procedure.

Do not expose the JSON schema as a user-editable contract.

## 8. Planning reconciliation

The subsystem has no active implementation milestone after C005/C006 closure.

Update the active registry:

- current milestone -> `none / roadmap planning` or equivalent;
- mark C007 ready as the only immediate implementation handoff while it exists;
- after C007 closes, state explicitly that no implementation handoff is active;
- retain M006A/C/D historical conditional closure records without presenting them as blockers;
- keep M006E/M006F/M007/C005/C006 closed.

Update the subsystem roadmap:

- add C007 after C006 as a bounded post-M007 corrective;
- state that Phase 8/M007 is closed after C006;
- separate future Phase 9 selective cleanup from active work;
- remove stale language implying M007 remains current/blocked.

Update the long-term roadmap only where necessary to reflect:

- Phase 7/M006 closed for current objectives;
- Phase 8/M007 closed;
- next roadmap work is future selective-cleanup/policy planning, not an active implementation dependency.

Do not invent an M008 implementation plan unless separate research justifies it.

## 9. Branch/repository hygiene

After C007 is implemented and merged, repository maintainers should retire branches whose work is fully merged or superseded.

Candidates currently include:

- `plans/m006e-adaptive-discovery` — merged by PR #6;
- `implementation/m006e-global-scan-scope-latency-policy` — superseded by ADR 002 and must never be merged;
- completed M006A/M006C/M006D implementation branches where no unmerged evidence is intentionally retained.

Before deleting any branch:

1. confirm its unique commits are either merged, intentionally superseded, or preserved in permanent planning/closure history;
2. confirm no open PR targets it;
3. never force-update or merge the superseded old M006E policy merely to eliminate divergence.

Branch deletion itself is repository hygiene, not a product acceptance criterion if permissions/workflow keep historical branches.

## 10. Documentation consistency audit

Before closure, search canonical/planning docs for stale phrases including:

- `M007 ... blocked`
- `M007 ... current milestone`
- `M006E ... conditionally closed` where corrective closure should be referenced
- `ready under ADR 002`
- `ready after M006E`
- old 120-second no-argument target presented as current
- old 15-minute/full-machine default proposal presented as authoritative
- `config init` as a supported command

Historical closure documents may retain period-accurate language when clearly labeled historical.

## 11. Verification

Required:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`
- Rust 1.89 check/test
- hosted Linux/macOS/Windows CI
- `git diff --check`
- documentation grep/audit proving no active-status contradictions remain.

No new performance qualification is required unless state recovery changes scan hot paths materially.

## 12. Acceptance criteria

C007 closes when:

- recoverable-invalid local discovery state self-heals after a trustworthy successful Full scan;
- incomplete Full scans preserve invalid prior state bytes;
- newer unsupported state is never overwritten;
- Routine fallback remains fail-soft;
- typed state-load classification replaces overwrite decisions based on generic string errors;
- README/state recovery behavior is documented;
- registry and roadmaps no longer present M007 as active;
- no stale active-handoff contradictions remain in canonical planning;
- hosted platform/MSRV gates pass.

## 13. Non-goals

- no discovery-state database/SQLite conversion;
- no filesystem watcher;
- no change to Routine seed roots;
- no change to learned-root retention policy;
- no cleanup proof or authorization changes;
- no selective-cleanup feature work;
- no release/distribution implementation;
- no automatic deletion of Git branches from runtime code.
