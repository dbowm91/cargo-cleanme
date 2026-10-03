# Corrective C006 Closure — Combined-Root Cleanup Ownership Universe

Plan: `plans/implementation/artifact-discovery-cleanup/c006-combined-root-cleanup-ownership-universe.md`

Disposition: **closed**

Implementation commits: `e6f883a` (combined cleanup engine), `e7c4f91` (state-generation reporting), `656dcd2` (multi-root mode-parity regression).

Date: 2026-10-03

## Requirement-to-evidence mapping

| C006 requirement | Evidence |
|---|---|
| Canonical, sorted, deduplicated, containment-collapsed selected roots | `clean_with_roots` validates real absolute roots, canonicalizes, sorts, deduplicates, and collapses them before discovery. `clean ROOT` delegates to the same entry point. |
| Discover all manifests in one selected-root boundary | `ScanScope::ExplicitRoots` discovers every explicit root with existing cleanup traversal policy; manifests deduplicate before one C004 coverage calculation. |
| Block all modes if any selected-root manifest is unresolved | `unresolved_manifest_in_second_root_blocks_first_root_for_all_modes` verifies an unresolved manifest in B blocks A in Preview, Simulate, and Execute with zero clean invocations and no candidate totals. |
| Build one physical output graph across selected roots | `combined_roots_classify_cross_root_output_as_shared_in_every_mode` uses two selected roots sharing one output and verifies scope-wide Shared classification. |
| Refresh the full combined universe before each unit disposition | `refresh_combined_universe` rediscovers manifests in all roots, repeats complete Cargo coverage, rejects incomplete/changed participants, then runs the existing fresh workspace proof. State records are not consulted. |
| Keep selected-root authorization bounded to the union | Authorization checks every covering root against the canonical selected-root union or explicit configured roots; ownership class remains authoritative and configuration cannot promote Shared/Uncertain/ExternalUnproven. Existing C003/C004 authorization tests remain green. |
| Keep Preview, Simulate, and Execute safety dispositions aligned | `combined_private_roots_keep_preview_simulate_execute_parity` proves two private roots are eligible in every mode; `combined_roots_classify_cross_root_output_as_shared_in_every_mode` proves shared output is skipped in every mode. Simulation makes zero Cargo clean calls. |
| Allow safe multi-root Execute after combined proof | The parity fixture executes two independently private selected roots successfully; the previous `roots.len() > 1` Execute rejection is removed. |
| Preserve Full/known orchestration and deterministic reporting | `clean --known` and `clean --full` call the same combined engine; no known roots stays a successful no-op; Full must produce a reconciled state generation before phase 2. Reports include the ordered root set, manifest/workspace/unit/unresolved counts, and `last_full_at` for known/full state generations. Counters aggregate combined discovery and each final proof. |

## Verification

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets` — passed, 178 tests across five suites (final local run).
- `rustup run 1.89 cargo check --locked --all-targets` — passed.
- `rustup run 1.89 cargo test --locked --all-targets` — passed, 177 tests across five suites.
- `git diff --check` — passed.
- Hosted CI run [37146797895](https://github.com/dbowm91/cargo-cleanme/actions/runs/37146797895) — passed on Windows, macOS, Ubuntu, and Rust 1.89.

## Platform and fixture evidence

Combined-root fixtures exercise two independent private roots, shared cross-root output, an unresolved participant in the second root, zero-clean simulation, Preview/Simulate/Execute parity, and complete proof refresh before disposition. The full C003/C004 cleanup regression suite remains green.

## Unresolved findings and disposition

- No C006 safety finding remains open.
- Final hosted Windows, macOS, Linux, and Rust 1.89 qualification is linked above; the first hosted attempt found only a Windows `.cmd` fake-editor test harness exit-code issue, which was replaced with a PowerShell subprocess fixture before this final run.
- Corrective C006 satisfies M007's combined-scope proof, mode parity, aggregate reporting, and state-generation reporting handoff. Historical M007 blocked evidence remains unchanged; the registry records the corrective closure.
- Disposition: **closed**.
