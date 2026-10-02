# cargo-cleanme

A focused Rust utility for finding reclaimable Cargo build artifacts without interrupting active development.

The first release is intentionally read-only. It discovers Cargo projects with conventional local `target/` build artifacts, excludes projects with recent source or build activity, measures their on-disk artifact footprint, and prints a deterministic report. Cleanup execution is deferred until discovery and activity semantics have been qualified on real systems.

The installed binary is `cargo-cleanme`, so it can be invoked directly or as a Cargo external subcommand:

```text
cargo-cleanme
cargo cleanme
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
cargo-cleanme config path
cargo-cleanme config init
cargo-cleanme config show
```

Pass `--config PATH` to read a specific TOML config. Explicit CLI roots take precedence over `scan.root`; either explicit scope bypasses user ignore/unignore filters. Filters apply during global discovery only. On Unix, global discovery starts at `/`; Linux prunes `/proc`, `/sys`, `/dev`, and `/run`, and macOS prunes `/dev`. Windows enumerates fixed and removable local drive letters. Traversal does not follow symlinks. Filesystem access failures are summarized on stderr.

The scanner reports only conventional direct-child `target/` directories. Source edits and target entries newer than the fixed 300-second default window protect a project. Unix and Windows report allocated bytes through platform metadata; other platforms report apparent file bytes. This is a read-only inventory estimate. Cleanup is not implemented.

## Planning

The repository starts planning-first. Canonical product direction, roadmap, bounded implementation plans, and active status are maintained under `plans/` using the same long-term/interim/closure separation used by CodeGG.

No destructive cleanup behavior belongs in the initial implementation milestone set.
