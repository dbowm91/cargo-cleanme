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

## Planning

The repository starts planning-first. Canonical product direction, roadmap, bounded implementation plans, and active status are maintained under `plans/` using the same long-term/interim/closure separation used by CodeGG.

No destructive cleanup behavior belongs in the initial implementation milestone set.
