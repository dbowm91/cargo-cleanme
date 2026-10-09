# C030 — Learned-root provenance and Routine scope hygiene

Status: **closing** — local implementation and verification are complete; hosted qualification is pending.

Implementation commits: `e7a9a82` (state and policy provenance filters) and `1a900bb` (platform temporary-root boundary correction).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Unresolved manifests remain inventoried but do not create Routine roots | `discovery_state::tests::unresolved_and_cache_manifests_never_create_routine_roots` keeps all four project records and verifies the unresolved entry is not learned | Pass |
| Resolved durable developer locations remain learnable | Same test verifies a resolved `home/projects/app` workspace contributes `home/projects` | Pass |
| Known caches and transient locations stay out of automatic Routine scope | Same test covers `.npm/node_modules` and `.codex/worktrees`; shared classifier also excludes Cargo registry/git, Go module cache, Trash and temporary roots | Pass |
| Existing disallowed learned roots are handled only on complete certain Full reconciliation | `reconcile_full` checks uncertainty first, then prunes only positively classified disallowed locations; incomplete Full returns `NoPublication` | Pass |
| Routine read-only scan and cleanup use compatible effective sets | Both use `automatic_root_is_maintainable`; cleanup still classifies raw paths before canonical collapse and rechecks omitted scope premises | Pass |
| Explicit roots and Full inventory remain broad | Discovery filters were not changed; explicit paths bypass maintenance-root classification | Pass |
| Preserve C028 and schema-forward behavior | Existing C028 absence/reappearance and state schema tests pass; no schema migration was introduced | Pass |

## Verification actually run

On 2026-10-09, local uid 1000 (`sugarwookie`), Rust `1.99.0`:

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — 376 passed on Linux.
- `python3 scripts/check-fixture-portability.py --self-test` and the checker — passed; 36 fixture files clean.
- `python3 scripts/check-doc-citations.py --self-test` and the checker — passed; 16 documents clean.
- `git diff --check` — passed.

## Platform and fixture evidence

Policy fixtures use a temporary home root, not an account-specific absolute path. They retain Full inventory records while asserting the exact Routine root set. Hosted Linux/macOS/Windows and Rust 1.89 CI qualification is pending the pushed branch run.

## Limitations and findings

- Provenance is derived from canonical location and known cache/runtime roots; it is not persisted as an authority field. Unknown legacy roots remain unless a complete certain Full scan positively classifies them as disallowed.
- A synthetic home under the system temporary directory is allowed as a fixture namespace; temp descendants outside the selected home remain excluded.
- No new config key or schema version was added. Explicit opt-in beyond existing explicit roots remains outside this corrective.
- No release or published-artifact qualification was authorized by this plan.

Unresolved findings: no known correctness defect. Hosted matrix evidence remains an external closure requirement.

Disposition: **closing** pending hosted qualification.
