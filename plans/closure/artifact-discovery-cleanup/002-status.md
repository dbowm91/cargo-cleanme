# Artifact Discovery and Cleanup M002 Closure

Plan: `plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md`

Disposition: **conditionally closed**

Implementation commits: `64b13a7b7746bd4ed60f8d8f6c481517b50bcb9a` through `9a4e7af9f88b9dbe866131a104877500975bb4df` (branch `implementation/artifact-discovery-cleanup`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Traversal adapter uses dua-core 4.1; no engine types leak into domain | `Cargo.toml`, `src/discovery.rs`; `cargo tree -i dua-core` resolved `dua-core v4.1.0` | Pass |
| Bounded traversal workers | Discovery caps `available_parallelism()` at 8; a pool is dropped/joined after each root walk | Pass; Windows drive roots are processed sequentially |
| Unix global root and safety pruning | `src/policy.rs`, `src/discovery.rs`; macOS root implementation starts at `/` and prunes `/dev`; Linux additionally prunes `/proc`, `/sys`, `/run` | Implemented; Linux CI pending |
| Windows local volumes | `GetLogicalDrives` + `GetDriveTypeW` includes fixed/removable volumes only | Implemented; hosted Windows CI pending |
| Ignore, exact unignore, ignored-ancestor reachability, explicit bypass | Filter unit tests, discovery exception fixture, policy tests | Pass |
| Cargo manifest + direct real target; target subtree pruned | Discovery fixture asserts the target subtree is absent from entry count; nested target manifest is not discovered | Pass |
| Symlink targets rejected; directory symlinks are not traversed | Unix symlink-target fixture; dua-core's documented non-following walker semantics | Pass on macOS |
| Structured bounded error category; no target sizing during discovery | `ScanDiagnostic`, discovery walker error conversion; candidate discovery creates only placeholder records | Pass |
| Representative release-tree observation | Release scan of `/Users/davidbowman/projects`: 28 candidates, 138.85 GiB estimated allocated size, 19.94 s elapsed | Pass; single local developer tree |

## Verification run

- `cargo test discovery` — passed (3 tests).
- `cargo test filters` — passed (2 tests).
- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (20 tests in final source tree).

## Limitations and unresolved findings

- Linux and Windows hosted CI have not run yet. Windows root enumeration is therefore implemented but not runtime-qualified here.
- Discovery currently starts one bounded dua-core pool per root; Windows roots are visited sequentially. It does not start a pool per directory or project.
- Diagnostics are retained as structured values and summarized by the CLI; individual-path verbose rendering is not implemented.

Severity: no known correctness or security findings. Hosted platform qualification remains open.

## Dependency disposition

M002 is conditionally closed and M003 proceeded against its domain output. M003's closure remains subject to the platform qualification below.
