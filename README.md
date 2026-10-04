# cargo-cleanme

A focused Rust utility for finding reclaimable Cargo build artifacts without interrupting active development.

Scanning is read-only. Cleanup is a separate, explicitly scoped operation that defaults to Cargo's dry-run preview and requires `--yes` to execute.

The installed binary is `cargo-cleanme`, so it can be invoked directly or as a Cargo external subcommand. Both forms are equivalent:

```text
cargo-cleanme ...
cargo cleanme ...
```

## Initial goals

- fast recursive Cargo-project discovery;
- default 300-second activity guard;
- TOML configuration for recency and search scope;
- glob-style ignored directories with explicit re-included directories;
- optional sandbox/root scope that supersedes ignore/re-include filtering;
- no symlink traversal;
- allocated disk-usage reporting;
- deterministic, size-descending read-only output;
- later, separately qualified `cargo clean` execution with space-recovery reporting.

## Usage

```sh
cargo-cleanme                  # fast Routine scan over seed + learned roots
cargo-cleanme scan --full     # exhaustive reconciliation of platform roots
cargo-cleanme scan ./projects  # scan one explicit scope
cargo-cleanme scan ./projects --no-progress # scan without transient UI
cargo-cleanme scan ./projects --no-progress --stats # benchmark/debug: no UI, counters+timings on stderr
cargo-cleanme clean ./projects # Cargo preview (default, `--dry-run`)
cargo-cleanme clean ./projects --dry-run # explicit Cargo preview
cargo-cleanme clean ./projects --dryrun # simulation: full path, no `cargo clean`
cargo-cleanme clean ./projects --yes # execute cleanup for verified workspaces
cargo-cleanme config path
cargo-cleanme config show
cargo-cleanme config edit
cargo-cleanme clean --known --dryrun # simulate bounded cleanup of Routine roots
cargo-cleanme clean --full --dryrun  # reconcile Full inventory, then simulate bounded scopes
cargo-cleanme clean ./projects --min-reclaimable-bytes 104857600
cargo-cleanme clean --known --older-than 2592000 --exclude '**/archived/**'
```

Global flags: `--no-progress` disables transient UI; `--stats` prints detailed semantic counters and phase/process timings to stderr (never stdout) for `scan` and `clean` (preview/simulate/execute). Default runs emit no debug counter line; stdout reports are byte-equivalent with and without `--stats`.

`--dry-run`, `--dryrun`, and `--yes` conflict pairwise and have intentionally distinct spellings:

- `--dry-run`: Cargo preview (may invoke `cargo clean --dry-run --verbose`).
- `--dryrun`: cargo-cleanme simulation/debug (runs discovery, resolution, filtering, sizing, authorization, revalidation, progress, and reporting but never invokes any `cargo clean` command, including `cargo clean --dry-run`).
- `--yes`: real Cargo cleanup.

If the distinction proves error-prone, the CLI contract will be reconciled rather than silently aliased.

Pass `--config PATH` to read a specific TOML config. Explicit CLI roots take precedence over `scan.root`; either explicit scope bypasses user ignore/unignore search filters. Routine scans visit existing seed roots under Projects/projects, Developer, dev, Code/code, src, repos/Repos, GitHub/github, and workspace/workspaces, plus active learned roots. Routine scans remain recursive under those selected roots. A never-learned unusual location may remain undiscovered until `scan --full` or an Explicit scan visits it. Full discovery preserves the existing exhaustive platform roots and prunes. Full reconciliation learns workspace neighborhoods in machine-local JSON state and retains roots for 30 days by default (`scan.learned_root_retention_days`).

The repository ships a canonical `config.toml` template. Operational commands create it on first use without overwriting an existing file. `config path` only prints the resolved path. `config edit` opens that file in VISUAL, EDITOR, or an available terminal editor, then validates the result. The repository template is a distribution artifact and is never loaded implicitly from the working directory. The template includes a commented `[cleanup]` section:

```toml
[cleanup]
allowed_output_roots = [
  # "/absolute/cache/root",
]

[cleanup.policy]
min_reclaimable_bytes = 0
# min_inactive_seconds = 2592000
include = []
exclude = []
```

Cleanup policy matches canonical workspace roots and runs only after complete ownership resolution. Excluded workspaces remain in the ownership proof and can still make outputs shared. `--min-reclaimable-bytes` filters by the freshly measured complete workspace output union; `--older-than` is an additional quiet-age requirement and never replaces the recency safety guard. CLI policy values replace configured values of the same kind when provided. Include/exclude patterns use glob syntax and exclusion wins over inclusion.

Discovery state is machine-local and normally self-managed. Routine scans use seed/configured roots if the state is missing or unusable. A successful `scan --full` replaces corrupt or obsolete local state after reconciliation completes; an incomplete Full scan leaves it untouched. State written by a newer cargo-cleanme schema is preserved, and that version must be upgraded before it can be reconciled. Manually deleting discovery state is not the normal recovery procedure.

The scanner is manifest-first: it discovers user `Cargo.toml` files without requiring a sibling `target/`, then asks Cargo itself (`locate-project --workspace`, `metadata --offline --locked --no-deps`) to resolve the workspace root, members, and `target-dir`/`build-dir` configuration. Cargo is never invoked for ignored/pruned paths. After the first successful metadata resolution for a workspace, every member manifest returned by that authoritative metadata is registered, so later discovered manifests from the same workspace require neither another locate nor another metadata process (an N-member workspace needs exactly one locate and one metadata; membership is never guessed from path ancestry or TOML). Equal/nested physical output is grouped and measured once, so `target == build` or nested `target`/`build` is never double-counted. Each group is labeled `[private]`, `[shared]`, `[external-unproven]`, or `[uncertain]`; shared, external-unproven, and uncertain output is inventory-only. Source edits anywhere in a workspace and recent output anywhere in a group protect that group. Cheap gates (existence, artifact presence, source activity) run before deep sizing. Unix and Windows report allocated bytes; other platforms report apparent bytes. Discovery, source-activity, target-sizing, and post-clean measurement share one bounded `dua-core` worker pool (at most eight threads); Cargo resolution is sequential. Final output is deterministic size-descending with a stable path tie-break and prints every reportable group plus a deduplicated inventory-estimate total, never recovered bytes. Detailed scan/cleanup counters and phase/process timings (discovery, locate/metadata time and count, source-activity, output-sizing, render overhead, and for cleanup the final ownership-universe proof work) are opt-in via `--stats` on stderr and never appear on stdout; default scans emit no debug counter line. `--no-progress --stats` is the canonical benchmark/debug combination.

Attended terminals show immediate inline progress on stderr: one coordinated `MultiProgress` owns the main bar plus up to five candidate rows (a single cursor/draw owner, so bars never fight for terminal state), with an indeterminate discovery bar (no wasteful pre-scan) and determinate analysis/cleanup once group/candidate counts are known through the `ProgressObserver::units_total` / `unit_completed` contract. Totals are re-scoped on phase changes so analysis counts never leak into cleanup counts; both scan analysis and cleanup set determinate totals via the trait (callers never assume the concrete renderer). Up to five largest reportable groups with sizes are shown, capped at 10 Hz, with no alternate screen or raw mode, stderr only. Progress clears before the final stdout report and never changes results. `--no-progress` forces hidden output for benchmarks; `TERM=dumb` or non-TTY output is plain with no control sequences.

`clean ROOT` accepts a relative or absolute explicit existing directory and previews via Cargo by default. A relative root is resolved lexically against the invocation directory into an absolute sandbox root; the boundary still requires a real non-symlink directory and rejects uncertainty. Before output analysis or cleanup qualification, every discovered `Cargo.toml` under `clean ROOT` must be accounted for by direct successful resolution or successful workspace metadata listing it as a member/root. An earlier failed member attempt is cleared only if later authoritative metadata covers it. Any remaining unresolved manifest blocks the entire cleanup scope in Preview, Simulate, and Execute, reports `unresolved_ownership` under `--stats`, and runs no `cargo clean` command, including Cargo preview dry-run. Read-only scans remain partial-result tolerant. The destructive unit is one workspace cleanup unit: exactly one Cargo workspace cleanup invocation, containing one resolved workspace, its complete resolved `OutputSet` (target and build), and every physical output group that one `cargo clean` can affect. Multi-member workspaces are supported. A unit is authorized only when *every* group it can affect is `PrivateBounded`, authorized, inactive, a real non-symlink directory, and marker-qualified, so a private `target` can never carry an `ExternalUnproven`, `Shared`, `Uncertain`, or unauthorized sibling `build` directory into the same invocation; such a unit is skipped whole, with the failing output kind, path, and class in the skip reason. Equal/nested `target`/`build` collapse into one physical group and one deduplicated union, so a distinct private target/build pair produces one Cargo invocation, one result, and one pre/post union measurement. `cleanup` progress totals and final report rows count cleanup units, not physical groups; each row names the workspace and the output roots the single invocation affects. The read-only scan report stays physical-group oriented. Only `PrivateBounded` may reach Cargo cleanup; `ExternalUnproven`, `Shared`, and `Uncertain` remain inventory-only under any authorization configuration. `cleanup.allowed_output_roots` is location authorization only for already-private groups: it MUST NOT convert `ExternalUnproven` into a private group and does not establish exclusivity (a single observed external owner is not exclusivity proof). The field is preserved for schema compatibility but is currently operationally redundant because `PrivateBounded` output is normally workspace-contained; conventional and redirected-inside-workspace private output inside `clean ROOT` remains eligible. A single final cleanup proof (`ExecutionProof`) binds canonical workspace root and root manifest, canonical member set, canonical target, canonical build and capability, the complete deduplicated physical covering union the invocation can affect, pre-clean union bytes, ownership class, authorization decision, cache-marker validity, fresh source and output activity, and frozen environment/arguments immediately before spawn. The proof re-resolves *every* workspace discovered under `clean ROOT` (the bounded cleanup ownership universe) through fresh `cargo metadata --offline --locked --no-deps` calls against each known root manifest, rebuilds the complete fresh physical output graph from those fresh workspaces, rejects any change in workspace identity, member set, target/build identity, group shape, or classification, rejects the unit when any other discovered workspace (including symlink or unresolvable output roots) can reach the affected output, and inspects every affected covering root for existence, directory type, no-symlink state, activity, and cache markers. A universe workspace that cannot be re-resolved fails the candidate closed. `cleanup --stats` reports this final-proof Cargo work separately from the initial scan counters; direct metadata refresh avoids redundant `cargo locate-project` calls, and independent refreshes run with a fixed concurrency cap of four (`proof_metadata_peak`). Preview, simulation, and execution share all non-mutating pre-spawn gates (ownership, authorization, fresh resolution, member/source activity, target/build identity, output activity, group stability, markers, frozen-env, final preflight); on unchanged state they reach the same cleanable/skipped dispositions, and `--dryrun` invokes no `cargo clean` command (including Cargo dry-run). For cleanup, `CARGO_TARGET_DIR` is frozen to the resolved target and, when runtime Cargo supports stable `build-dir`, `CARGO_BUILD_BUILD_DIR` is frozen to the resolved build; inherited conflicting variables never silently redirect the destination, and the command runs from the workspace/root-manifest context without package selectors or private cache parsing. Output directories are never recursively deleted by cargo-cleanme; cleanup delegates to Cargo sequentially. Transient progress (determinate via the observer contract, at most five current/largest groups with sizes, distinct preview/simulate/execute labels) clears before final results. Execute lists every cleaned workspace unit with its output roots, pre-clean union size, post-clean union size, and observed decrease, plus a deduplicated total observed decrease and skipped/failed reasons; post-clean measurement uncertainty is reported as a diagnostic and never fabricates recovered bytes. Preview lists authorized candidates with pre-clean estimates and Cargo preview text plus a total estimate and `no cleanup executed`. Simulate lists every would-clean unit with estimates plus a total estimated would-clean bytes and `no cargo clean command was invoked`. Preview/simulate totals are estimates, never recovered bytes. No-argument invocation and `scan` remain read-only. No machine-wide destructive command exists, and shared-cache cleanup is not claimed.

## Planning

The repository starts planning-first. Canonical product direction, roadmap, bounded implementation plans, and active status are maintained under `plans/` using the same long-term/interim/closure separation used by CodeGG.

Cleanup is opt-in and Cargo-managed. Its current destructive boundary is defined by M004 plus ADR 001 and the C002-C004 ownership/revalidation corrections; M006 performance work must not weaken those checks.
