# ADR 005 (proposed) — Targeted Retirement of Positively Unavailable Automatic Learned Roots

Status: **proposed; not accepted; no implementation authority for state mutation**.
Date: 2026-10-09.
Baseline: `7c874ab7998bdac28a6236ec21ae59ec24b915f8`.
Related: C033 learned-root reconciliation corrective; ADR 002 adaptive Full/Routine state; C028 missing-root admission; C029 partial Full coverage.
Scope: machine-local **advisory learned-root hints only**, not Cargo resolution, cleanup ownership, or user-authored configuration.

## Problem

An ordinary unprivileged Full scan of `/` often yields partial coverage, which (correctly) cannot complete an ADR 002 negative reconciliation. Separately, Routine `cargo cleanme` may definitively observe that one previously learned ephemeral worktree directory no longer exists (`symlink_metadata -> ENOENT`). C028 omits that automatic root from *this invocation*, but its persisted hint can reappear on every later run until expiry/complete Full; C030's policy excludes known transient locations only after some admission work, and the existing terminal output may emit hundreds of redundant lines.

ADR 002 presently permits expiry/absence pruning only after complete Full discovery; it does not authorize Routine to retire state even if a specific path's absence is directly established. An automatic, positive path-absence observation is narrower than inferring that a subtree has no Rust projects from an incomplete Full walk. This is a proposed **forward exception**, not an assertion that C028/C030 already changed that authority.

## Proposed decision (requires acceptance)

Allow **only a particular previously persisted automatic learned-root hint** to be retired from a supported-version state generation during Routine when either:

1. the exact raw root path has a verified `NotFound` result from `symlink_metadata` and every relevant ancestor path identity/permissions needed to interpret it are known, without following symlinks; or
2. the exact path is independently, positively classified as a known ephemeral/non-maintainable origin by stable, location-sensitive policy and its exclusion does not remove an explicit/configured seed.

No negative project-discovery inference, no expiration by age outside a complete Full, no arbitrary parent-container collapse, and no rewriting `ProjectRecord` inventory from partial scans. Do not touch `last_full_at` or claim a new Full generation. A deleted tree may reappear later; the next Full can rediscover it and re-learn it only if it then satisfies the approved durable-learning policy. Existing seed/configured/explicit search roots never retire automatically.

### Concurrency, path identity, and cleanup

- Retire only by compare-and-swap or equivalent generation-checked atomic publication that preserves all unrelated state and refuses newer schema/unreadable files; a failed publication leaves the old state intact with a bounded diagnostic.
- Revalidate the exact absence/provenance condition before commitment; a reappearance blocks retirement. Concurrent writers and stale snapshots must not lose recent positive observations.
- The current **invocation's** omitted-root ledger remains frozen and is checked again before *every* Cargo cleanup spawn (C028). State retirement **cannot** authorize a clean, widen a source root, or relax complete-universe C004/C006 proof.
- Do not infer `NotFound` from `Path::exists` returning false, from failed canonicalization, or from failure to read a directory. Permission denied, symlink loops and indeterminate I/O remain uncertainty and block destructive admission.
- No source-tree search suppression when the user selected that path explicitly; an eventual Full scan still inventories accessible unusual locations.

### User-visible compatibility

Existing CLI, `--format log` bounded single line, machine JSON v1 fields, and simulated cleanup semantics remain unchanged unless a separately approved versioned contract says otherwise. Human omission diagnostics are bounded independently of whether state mutation succeeds. State writes are optimization-only: a failed save must not turn successful read-only inventory into a nonzero exit or turn a blocked cleanup into a permitted one.

## Alternatives

- **Keep all stale state until Full/age expiration:** safe and ADR 002-compliant, but can repeat diagnostics indefinitely when Full remains partial.
- **Clear entire discovery-state.json every Routine:** rejected; loses useful project inventory and violates forward-schema/data-preservation guarantees.
- **Silently classify every inaccessible root as nonexistent:** rejected; hides potential owners and weakens destructive safety.
- **Prune absent known transient hints in-memory but never persist:** valid immediate UX mitigation; repeated retrieval/omission cost remains.
- **Allow arbitrary negative learned-root pruning every Routine:** rejected because missing projects under inaccessible subtrees are not proven absent.

## Acceptance and evidence gate

Before accepting, demonstrate with real path/permission/race fixtures that a particular entry's positive absence can be established without following symlinks, and that its retirement changes only advisory hint selection while C028 pre-spawn proof remains intact. Prove 100-path repeated-run quietness and state idempotence, atomic compare-and-write under competing writers, future-schema refusal, symlink/permission counterexamples, late reappearance zero-clean-spawn, and post-Full re-discovery.

If the concurrency and proof requirements cannot be met, **leave this ADR proposed**, ship only bounded omission reporting, and record state-recovery liveness as open. C033 may not implement automatic persistent pruning before explicit acceptance. Historical accepted ADR 002/closed C028–C030 receipts are not edited.
