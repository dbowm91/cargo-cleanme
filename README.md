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
cargo-cleanme                  # scan platform-defined global roots
cargo-cleanme scan ./projects  # scan one explicit scope
cargo-cleanme scan ./projects --no-progress # scan without transient UI
cargo-cleanme clean ./projects # Cargo preview (default, `--dry-run`)
cargo-cleanme clean ./projects --dry-run # explicit Cargo preview
cargo-cleanme clean ./projects --dryrun # simulation: full path, no `cargo clean`
cargo-cleanme clean ./projects --yes # execute cleanup for verified workspaces
cargo-cleanme config path
cargo-cleanme config init
cargo-cleanme config init --force # replace the existing config file
cargo-cleanme config show
```

`--dry-run`, `--dryrun`, and `--yes` conflict pairwise and have intentionally distinct spellings:

- `--dry-run`: Cargo preview (may invoke `cargo clean --dry-run --verbose`).
- `--dryrun`: cargo-cleanme simulation/debug (runs discovery, resolution, filtering, sizing, authorization, revalidation, progress, and reporting but never invokes any `cargo clean` command, including `cargo clean --dry-run`).
- `--yes`: real Cargo cleanup.

If the distinction proves error-prone, the CLI contract will be reconciled rather than silently aliased.

Pass `--config PATH` to read a specific TOML config. Explicit CLI roots take precedence over `scan.root`; either explicit scope bypasses user ignore/unignore filters. Filters apply during global discovery only. On Unix, global discovery starts at `/`; Linux prunes `/proc`, `/sys`, `/dev`, and `/run`, and macOS prunes `/dev`. Cargo registry/git trees under the effective Cargo home (`$CARGO_HOME` or `~/.cargo`) are pruned and never trigger Cargo resolution. Windows enumerates fixed and removable local drive letters. Traversal does not follow symlinks. Filesystem access failures are summarized on stderr.

The repository ships a canonical `config.toml` template. `config init` writes exactly that file to the resolved platform config path and refuses to overwrite an existing file unless `--force` is given; `--force` writes a temporary sibling and renames it so a failed write leaves the previous config intact. The repository template is a distribution artifact and is never loaded implicitly from the working directory. The template includes a commented `[cleanup]` section:

```toml
[cleanup]
allowed_output_roots = [
  # "/absolute/cache/root",
]
```

The scanner is manifest-first: it discovers user `Cargo.toml` files without requiring a sibling `target/`, then asks Cargo itself (`locate-project --workspace`, `metadata --offline --locked --no-deps`) to resolve the workspace root, members, and `target-dir`/`build-dir` configuration. Cargo is never invoked for ignored/pruned paths, and metadata is resolved once per workspace. Equal/nested physical output is grouped and measured once, so `target == build` or nested `target`/`build` is never double-counted. Each group is labeled `[private]`, `[shared]`, `[external-unproven]`, or `[uncertain]`; shared and external-unproven output is inventory-only. Source edits anywhere in a workspace and recent output anywhere in a group protect that group. Cheap gates (existence, artifact presence, source activity) run before deep sizing, with counters on stderr. Unix and Windows report allocated bytes; other platforms report apparent bytes. Discovery, source-activity, target-sizing, and post-clean measurement share one bounded `dua-core` worker pool (at most eight threads); Cargo resolution is sequential. Final output is deterministic size-descending with a stable path tie-break and prints every reportable group plus a deduplicated inventory-estimate total, never recovered bytes.

Attended terminals show immediate inline progress on stderr: an indeterminate discovery bar (no wasteful pre-scan), determinate analysis once workspace/group counts are known, up to five largest reportable groups with sizes, capped at 10 Hz. Progress clears before the final stdout report and never changes results. `--no-progress` forces hidden output for benchmarks; `TERM=dumb` or non-TTY output is plain with no control sequences.

`clean ROOT` accepts a relative or absolute explicit existing directory and previews via Cargo by default. A relative root is resolved lexically against the invocation directory into an absolute sandbox root; the boundary still requires a real non-symlink directory and rejects uncertainty. A cleanable unit is one resolved workspace plus its authorized private physical output group(s); multi-member workspaces are supported when all groups to be cleaned are private/authorized. Before preview, simulation, or execution, workspace/output resolution, physical grouping, member source activity, output activity, and authorization roots are re-resolved and compared; any changed/uncertain state skips that workspace. Execute repeats the final preflight immediately before spawning Cargo. For cleanup, `CARGO_TARGET_DIR` is frozen to the resolved target (or explicit `--target-dir` arg) and, when runtime Cargo supports stable `build-dir`, `CARGO_BUILD_BUILD_DIR` is frozen to the resolved build; inherited conflicting variables never silently redirect the destination, and the command runs from the workspace/root-manifest context without package selectors or private cache parsing. Shared, uncertain, symlink, and unauthorized external output is never cleaned; a single observed external owner is not exclusivity proof. External private output inside `clean ROOT` is authorized by the sandbox; outside requires an explicit absolute `cleanup.allowed_output_roots` entry (directory roots, not globs; symlink roots invalid; empty preserves M004 containment). Output directories are never recursively deleted by cargo-cleanme; cleanup delegates to Cargo sequentially. Transient progress (determinate, at most five current/largest groups with sizes, distinct preview/simulate/execute labels) clears before final results. Execute lists every cleaned physical group with pre-clean size, post-clean size, observed decrease, plus a deduplicated total observed decrease and skipped/failed reasons. Preview lists authorized candidates with pre-clean estimates and Cargo preview text (verbose/detail when noisy) plus a total estimate and `no cleanup executed`. Simulate lists every would-clean group with estimates plus a total estimated would-clean bytes and `no cargo clean command was invoked`. Preview/simulate totals are estimates, never recovered bytes. No-argument invocation and `scan` remain read-only. No machine-wide destructive command exists, and shared-cache cleanup is not claimed.

## Planning

The repository starts planning-first. Canonical product direction, roadmap, bounded implementation plans, and active status are maintained under `plans/` using the same long-term/interim/closure separation used by CodeGG.

Cleanup is opt-in, Cargo-managed, and bounded by the M004 ownership checks described in its plan.
