# Corrective C005 Closure — Uncertainty-Aware State Reconciliation and Config/Edit Hardening

Plan: `plans/implementation/artifact-discovery-cleanup/c005-uncertainty-aware-state-reconciliation-and-config-edit-hardening.md`

Disposition: **closed**

Implementation commits: `e6f883a` (C005/C006 implementation), `e7c4f91` (generation reporting and Windows editor fixture), `656dcd2` (combined-root parity fixture).

Date: 2026-10-03

## Requirement-to-evidence mapping

| C005 requirement | Evidence |
|---|---|
| Publish positive manifest observations independently of Cargo resolution | State schema v2 stores manifest identity and `resolved` / `manifest_observed_unresolved`; `discovery_state::reconcile_full` retains unresolved manifest evidence and learns its containing root. Reconciliation fixtures cover unresolved observations. |
| Reconcile absence using path-scoped uncertainty | `uncertainty_intersects` tests equality, ancestor, descendant, and component-boundary behavior. Full reconciliation retains intersecting prior roots and expires only trustworthy stale roots. |
| Preserve generations on incomplete Full and implement retention semantics | `reconcile_full` explicitly returns `NoPublication` for incomplete traversal; tests cover retention `0`, expiration, and root containment collapse. Config template documents `0` as expiration disabled. |
| Migrate v1 state and protect corrupt/newer state | `load_at` migrates v1 in memory, rejects schema 0/corrupt data/newer schemas, and writes only schema 2. A real local Full attempt left the existing invalid schema-0 state untouched. |
| Persist canonical, containment-collapsed learned roots | Reconciliation canonicalizes, sorts, collapses nested roots, and keeps the newest timestamp in a containing root. Broad user, drive, and OS-managed roots are guarded. |
| Harden atomic state publication | Tests cover same-process temporary-name collision, replacement failure preserving the destination, cleanup of failed temp files, and v1-to-v2 round trip. Windows uses `MoveFileExW` replace/write-through semantics. |
| Make Routine state errors visible and fail-soft | Routine policy emits one bounded actionable diagnostic and falls back to seed/configured roots; unsupported state is never overwritten by an older binary. |
| Qualify config edit with subprocess editors | Parser fixtures cover arguments, quoted spaces, and Windows backslashes. Fake editor integration covers VISUAL precedence, config bootstrap, nonzero exit propagation, valid edit, invalid TOML preservation/actionable error, and no-op edit. |
| Keep discovery state separate from cleanup proof | Cleanup continues to rediscover and freshly resolve selected roots; no state workspace/output record is passed into cleanup ownership proof. |

## Full traversal evidence

The local reference `scan --full` completed in 128.83 seconds: 4,670,392 entries, 1,804 manifests, 742 localized diagnostics, and 283 Cargo-resolution failures. The normal machine state file had invalid schema 0 and was correctly left unchanged. Repeating under an isolated HOME completed and published schema 2 with 1,804 project observations and six learned roots despite the same localized diagnostic/resolution failures.

## Verification

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets` — passed, 178 tests across five suites (final local run).
- `rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rustup run 1.89 cargo test --locked --all-targets` — passed, 177 tests across five suites.
- `git diff --check` — passed.
- Hosted CI run [37146797895](https://github.com/dbowm91/cargo-cleanme/actions/runs/37146797895) — passed on Windows, macOS, Ubuntu, and Rust 1.89.

## Unresolved findings and disposition

- No C005 correctness finding remains open.
- The user's pre-existing invalid schema-0 state remains untouched by design; Routine scanning reports it and falls back. The isolated Full run proves completed localized-error reconciliation can publish a useful generation.
- The historical M006E closure record remains unchanged; C005 supplies the corrective evidence that closes its outstanding state/config requirements.
- Disposition: **closed**.
