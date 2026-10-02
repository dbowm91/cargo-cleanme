# M005B — Authorized Redirected Cleanup and Full Simulation Mode

Status: ready

Repository planning baseline: `50461442c0d4d783a5a4f1e86a35001f68f8d8db`

Source roadmap milestone: M005 redirected/shared Cargo output awareness

Accepted decision:

- `plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md`

Hard dependency:

- M005A closure and its final workspace/output/progress interfaces.

Primary class: destructive capability / safety / UX

## 1. Objective

Extend Cargo-mediated cleanup from M004's conventional one-package target to M005A's multi-member workspace/output model only where physical output ownership and cleanup authorization are exclusive and bounded.

Add an explicit `--dryrun` application-simulation mode that exercises the same discovery, workspace resolution, filtering, sizing, ownership authorization, final revalidation, progress events, and final report as execution while never invoking any `cargo clean` command.

Shared and externally unproven output remains inventory-only.

## 2. Invariants

- Scanning remains read-only.
- `--yes` remains the only mode that may perform real Cargo cleanup.
- `--dryrun` MUST NOT invoke `cargo clean`, including `cargo clean --dry-run`.
- Cargo metadata/locate subprocesses are allowed in `--dryrun`; the prohibition is specifically cleanup invocation.
- Existing default safe preview may continue to use `cargo clean --dry-run --verbose`.
- Cleanup is workspace-scoped and Cargo-mediated.
- Shared, uncertain, symlink, and unauthorized external output is never cleaned.
- A single observed external owner is not exclusivity proof.
- Target/build paths and ownership are re-resolved immediately before execution/simulation disposition.
- Cleanup Cargo processes remain sequential.
- Output directories are never recursively deleted by cargo-cleanme.
- Progress clears before final results.
- Final execute report lists every physical directory/group cleaned and its measured size change plus a total observed decrease.
- Simulation reports estimated/would-clean bytes, never recovered bytes.

## 3. CLI mode contract

Model the command explicitly rather than with loosely coupled booleans:

- `cargo-cleanme clean ROOT` or `--dry-run` => `CargoPreview`: invoke Cargo's own dry-run for authorized candidates.
- `cargo-cleanme clean ROOT --dryrun` => `Simulate`: run the full cargo-cleanme path but invoke no Cargo clean command.
- `cargo-cleanme clean ROOT --yes` => `Execute`: real Cargo cleanup.

`--dryrun`, `--dry-run`, and `--yes` conflict pairwise.

Document the intentionally distinct spellings because they have distinct semantics:

- `--dry-run`: Cargo preview.
- `--dryrun`: cargo-cleanme simulation/debug.

If usability testing shows this distinction is too error-prone, stop and reconcile the CLI contract rather than silently aliasing them.

## 4. Cleanup authorization configuration

Add a bounded configuration section such as:

~~~toml
[cleanup]
allowed_output_roots = [
  "/absolute/cache/root",
]
~~~

Rules:

- paths must be absolute;
- entries are directory roots, not globs;
- symlink authorization roots are invalid;
- authorization does not imply ownership;
- `Shared` and `Uncertain` remain forbidden even inside an allowed root;
- the explicit `clean ROOT` source sandbox automatically authorizes private output physically contained inside ROOT;
- output outside ROOT must be inside one configured allowed output root to become `PrivateBounded`;
- an empty list preserves M004-style containment safety.

No machine-wide destructive cleanup command is added.

## 5. Required production changes

### 5.1 Evolve cleanup ownership to workspace/output groups

Replace package-only `Ownership` with the M005A resolved workspace/output contract.

A cleanable unit is one resolved workspace plus its authorized private physical output group(s).

Multi-member workspaces are supported when all output groups to be cleaned are private/authorized.

### 5.2 Revalidate the complete ownership graph

Before Cargo preview, simulation disposition, or execute:

1. rerun Cargo workspace/output resolution;
2. rerun output physical identity/grouping;
3. rerun member source activity;
4. rerun output activity;
5. re-evaluate authorization roots;
6. compare workspace identity, member set as needed for safety, target/build logical paths, physical groups, and ownership class.

Any changed/uncertain state skips that workspace.

Execute repeats the final preflight immediately before spawning Cargo, preserving M004's double-check behavior.

### 5.3 Freeze Cargo output context

Extend `ProcessRunner` so the caller can supply an explicit child environment.

For cleanup:

- set/override `CARGO_TARGET_DIR` to the resolved target directory or pass the equivalent explicit Cargo argument;
- when runtime Cargo supports stable build-dir, set/override `CARGO_BUILD_BUILD_DIR` to the resolved build directory;
- prevent inherited conflicting output variables from silently changing the cleanup destination;
- invoke from the resolved workspace/root-manifest context.

Do not pass unsupported unstable flags merely to force compatibility with Cargo 1.89.

### 5.4 Delegate cleanup to Cargo

For a private workspace, run Cargo against the workspace/root manifest without package selectors unless a later selective-cleanup milestone says otherwise.

Do not parse/delete build-cache internals.

When target and build are distinct, rely on the supported Cargo cleanup behavior for that runtime; qualify it with integration fixtures before enabling execution.

### 5.5 Implement `--dryrun` simulation

Simulation executes the same cargo-cleanme stages as Execute through the final authorization/revalidation boundary.

At the point Execute would spawn `cargo clean`:

- emit a simulated/would-clean result;
- do not call the process runner with any argv whose Cargo subcommand is `clean`;
- do not mutate target/build output;
- retain measured pre-clean bytes for final reporting.

A fake runner test MUST fail if simulation attempts any Cargo clean invocation.

### 5.6 Integrate cleanup progress status

Use M005A's observer/renderer rather than adding a second terminal implementation.

Cleanup phase becomes determinate because the number of authorized candidate workspaces/groups is known.

Transient rows show at most five current/largest cleanup groups with sizes.

Simulation and preview use visibly distinct phase labels.

Clear progress before writing final results.

### 5.7 Final reports

Execute:

- list every cleaned physical output group/directory;
- include pre-clean size, post-clean measured size, observed decrease;
- total observed decrease once, with overlapping roots deduplicated;
- list skipped/failed workspaces with concise reasons after or alongside results without double-counting totals.

CargoPreview:

- list every authorized preview candidate and pre-clean estimate;
- include Cargo preview text only in verbose/detail output if normal output would become noisy;
- total estimated bytes;
- explicitly state no cleanup executed.

Simulate:

- list every group that would have been cleaned and its estimated size;
- total estimated would-clean bytes;
- explicitly state simulation and that no `cargo clean` command was invoked.

Do not label Preview/Simulate totals as recovered bytes.

## 6. Ordered work packages

### A — Cleanup config and CLI mode model

Add `cleanup.allowed_output_roots`, explicit mode enum, and pairwise flag conflicts.

### B — Workspace ownership/revalidation

Port M004 preflight/double-check semantics to M005A workspace/output groups.

### C — Frozen Cargo process context

Add environment-aware process runner and capability-conditioned target/build freezing.

### D — Simulation mode

Implement `--dryrun` and no-clean subprocess invariant.

### E — Redirected private cleanup

Enable Cargo preview/execute for private authorized redirected output; keep shared/unproven inventory-only.

### F — Progress/final report integration

Use M005A renderer, clear it, then print complete cleanup results and totals.

### G — Qualification/closure

Cross-version Cargo fixtures, real temp projects, CI/MSRV, docs, closure.

## 7. Required tests

### CLI/mode

- default clean => CargoPreview;
- explicit `--dry-run` => CargoPreview;
- `--dryrun` => Simulate;
- `--yes` => Execute;
- all conflicting pairs rejected;
- Cargo external-subcommand argv works for all modes.

### Authorization

- private redirected output inside ROOT allowed;
- private redirected output outside ROOT skipped without configured authorization;
- same output allowed when inside a configured absolute allowed root;
- symlink allowed-root rejected;
- shared output skipped even inside allowed root;
- uncertain group skipped;
- authorization root itself does not cause filesystem traversal outside what output resolution requires.

### Runtime Cargo matrix

On Rust/Cargo 1.89:

- redirected `target-dir` private fixture;
- no assumption of stable separate build-dir.

On current stable:

- target == build;
- distinct target/build;
- multi-member workspace;
- external authorized target/build.

Where practical, use the hosted toolchain matrix rather than JSON mocks alone.

### Simulation invariant

With a recording process runner:

- locate/metadata calls may occur;
- no call whose subcommand is `clean` occurs;
- result set matches Execute's authorized candidate set before mutation;
- files remain byte-identical;
- progress/final output is produced.

### Execution safety

- re-resolution detects changed target/build path;
- changed owner/share classification skips;
- recent member activity between scan and execution skips;
- output replacement/symlink race skips;
- Cargo process failure isolated per workspace;
- real temp-project Cargo cleanup removes private redirected output as expected;
- post-clean union measurement does not double-count nested target/build.

### Reporting/UI

- progress clears before final report;
- execute prints all cleaned groups, not only transient top five;
- total observed decrease equals deduplicated per-group union;
- simulation/preview totals are estimates and wording is distinct;
- non-TTY output remains plain/deterministic.

## 8. Performance/debug qualification

Use `--dryrun` specifically to qualify the end-to-end decision path without filesystem mutation.

Record on a representative tree:

- total elapsed time;
- Cargo locate/metadata counts;
- gate/filter counts from M005A;
- authorized/skipped/shared groups;
- measured bytes;
- progress enabled vs `--no-progress`.

The simulation candidate set should match an immediately adjacent Execute/Preview preflight on an unchanged fixture.

## 9. Verification commands

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
git diff --check
~~~

Hosted Linux/macOS/Windows stable and Rust 1.89 jobs must pass. Stable-only integration tests for `build-dir` must be clearly separated from MSRV-compatible tests.

## 10. Documentation updates

Update:

- README mode distinction and progress/final output examples;
- canonical config template with commented cleanup authorization;
- subsystem roadmap/registry;
- M005B closure.

Do not claim shared-cache cleanup support.

## 11. Acceptance criteria

M005B closes only when:

- a multi-member private workspace with redirected output can be Cargo-previewed and cleaned safely;
- distinct supported target/build directories are modeled, frozen, and measured correctly;
- external private output requires explicit authorization;
- shared/unproven/uncertain output remains non-destructive;
- `--dryrun` runs the full cargo-cleanme path through final revalidation but invokes no Cargo clean command;
- progress remains transient and final execute output lists every cleaned physical directory/group and total observed decrease;
- M004 conventional cleanup remains compatible;
- real temp-project destructive integration passes on supported stable Cargo;
- Linux/macOS/Windows stable and Rust 1.89 CI are green.

## 12. Stop conditions

Stop and report if:

- Cargo cleanup cannot be constrained to the exact resolved target/build outputs on a supported runtime;
- Cargo's cleanup behavior for distinct build-dir is not stable enough to qualify safely;
- an external private group cannot be distinguished from shared use without scanning unrelated filesystem state;
- simulation would need to invoke `cargo clean --dry-run` to determine the candidate set;
- enabling multi-member cleanup requires package-selective cache parsing;
- an implementation weakens M004's revalidation or direct-deletion prohibition.

## 13. Closure evidence

Create:

`plans/closure/artifact-discovery-cleanup/005b-status.md`

Include an ownership/authorization matrix, Cargo runtime matrix, simulation no-clean call trace, destructive temp-fixture evidence, progress/final report evidence, before/after deduplicated size measurements, hosted CI/MSRV links, and unresolved limitations.
