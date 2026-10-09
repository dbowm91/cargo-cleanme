# C030 — Learned-root provenance and Routine scope hygiene

Status: **conditionally closed** — local implementation and verification are complete; hosted macOS checks and installer jobs remain queued.

Implementation commits: `e7a9a82` (state and policy provenance filters), `1a900bb` (platform temporary-root boundary correction), and `fc673fd` (canonical synthetic-home handling for Windows fixtures).

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

Policy fixtures use a temporary home root, not an account-specific absolute path. They retain Full inventory records while asserting the exact Routine root set. Hosted run `37953385660` at `60ee214` exposed a Windows-only fixture mismatch: canonical learned paths differed from the expected path spelling, and the temporary-home boundary compared long/short Windows paths. Both sides are canonicalized in `fc673fd`. The prior Windows failure is retained. Corrected-head run `37953997910` passes Windows, Ubuntu, MSRV, generated docs, benchmark, and Linux/Windows installers; macOS checks and installer jobs remain queued.

## Limitations and findings

- Provenance is derived from canonical location and known cache/runtime roots; it is not persisted as an authority field. Unknown legacy roots remain unless a complete certain Full scan positively classifies them as disallowed.
- A synthetic home under the system temporary directory is allowed as a fixture namespace; temp descendants outside the selected home remain excluded.
- No new config key or schema version was added. Explicit opt-in beyond existing explicit roots remains outside this corrective.
- No release or published-artifact qualification was authorized by this plan.

Unresolved findings: no known correctness defect. The initial Windows fixture failure is corrected and retained above; macOS hosted qualification remains outstanding.

Disposition: **conditionally closed** pending the two macOS jobs in run `37953997910`.
