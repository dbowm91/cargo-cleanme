# CLI — argument parsing and the cargo subcommand shim

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** the dead public API noted in §5 (`Cli::scan_root`,
> `absolutize_root_for_test`) was reviewed and **kept** — removing `pub` from a
> crates.io-published crate is a semver-visible change. See
> [overview §7.1](overview.md).

`src/cli.rs` is 506 lines: 201 production, 305 inside an inline `#[cfg(test)]`
module beginning at `src/cli.rs:200`. It is the entire user-facing contract of
the binary — the clap command model, the shim that makes `cargo cleanme …`
parse at all, and the lexical path helper that lets a relative `ROOT` reach the
cleanup safety boundary.

---

## 1. Responsibility

`cli.rs` owns three things: the **command model** (`Cli`, `Command`,
`ConfigCommand`, `OutputFormat`); **argv normalization**
(`normalize_cargo_argv`, `src/cli.rs:138`), which undoes Cargo's
external-subcommand argv injection; and **scan-root absolutization**
(`absolutize_root` `src/cli.rs:153`, `absolutize_root_for_test`
`src/cli.rs:166`, shared worker `absolutize_relative` `src/cli.rs:173`).

What it deliberately does **not** do:

- **No policy interpretation.** `clean` carries `known`/`full`/
  `min_reclaimable_bytes`/… as raw fields; the Routine-vs-Full decision lives
  in `policy::resolve`, called from `main.rs:317` and `main.rs:133`.
- **No filesystem discovery.** The only filesystem call is
  `std::env::current_dir()` (`src/cli.rs:157`); there is no `exists`, `is_dir`,
  or `read_dir`. Root existence/directory/symlink checks are later layers' job
  — e.g. `src/discovery.rs:385` refuses a symlinked scan root.
- **No config loading.** `Cli` holds `config: Option<PathBuf>`
  (`src/cli.rs:19`) and resolves a *path reference*, not a policy:
  `main.rs:17` hands it to `config::ConfigPathResolver::new(cli.config.clone())
  .path()?` (`src/config.rs:69-86`), which returns the override verbatim or
  derives the platform config dir via `ProjectDirs` and appends `config.toml`.
  `cli.rs` never calls it.
- **No exit-code handling.** It never touches `std::process::exit`.

**It is a leaf module.** `src/cli.rs:1-3` imports only `clap::{Parser,
Subcommand}`, `std::ffi::OsString`, and `std::path::{Component, Path, PathBuf}`;
a grep for `crate::` / `use cargo_cleanme` in the file returns nothing. A parser
change cannot be driven by, or drag along, any safety-critical module.

---

## 2. The command surface

### 2.1 Global options on `Cli` (`src/cli.rs:17-33`)

Declared with `global = true`, so each is accepted before *or* after the
subcommand name.

| Flag | Short | Type | Default | Maps to |
|---|---|---|---|---|
| `--config <CONFIG>` | — | `Option<PathBuf>` | `None` | `ConfigPathResolver::new` → `main.rs:17` |
| `--no-progress` | — | `bool` | `false` | `progress::should_show_progress(..)` → `main.rs:197`, `main.rs:328` |
| `--stats` | — | `bool` | `false` | `discover_manifests_with_attribution(.., stats)` `main.rs:338`; the `eprintln!` blocks at `main.rs:551`, `main.rs:243` |
| `--format <FORMAT>` | — | `OutputFormat` (value enum) | `human` | selects `output::scan` / `output::cleanup` / `report::render` → `main.rs:511`, `main.rs:216`, `main.rs:521` |
| `--help` / `-h` | `-h` | clap built-in | — | clap help path |
| `--version` / `-V` | `-V` | clap built-in (from `version`, `src/cli.rs:14`) | — | `cargo-cleanme 0.1.6` |

**No global flag has a short form** beyond clap's `-h`/`-V`; there is not one
`short = ` attribute in the file. `--config` carries no doc comment
(`src/cli.rs:18-19`), which is why `man/cargo-cleanme.1` renders it with an
empty description body. The other three state intent explicitly, and
`--stats`'s help text asserts the contract it must keep: *"Never changes
deterministic stdout"* (`src/cli.rs:24-25`).

`command: Option<Command>` (`src/cli.rs:31-32`) is **optional**, and that is
load-bearing: a bare invocation parses successfully and means *Routine scan*
(`main.rs:266` → `run_scan(None, false, …)`), pinned by
`empty_is_scan_intent` (`src/cli.rs:206`).

### 2.2 `scan` (`src/cli.rs:36-40`)

| Token | Kind | Notes |
|---|---|---|
| `[ROOT]` | optional positional `Option<PathBuf>` | No doc comment, so it is undocumented in the manpage. Passed **raw** (not absolutized) to `policy::resolve` as `ScanRequest.cli_root` → `main.rs:317-323` |
| `--full` | `bool` | `conflicts_with = "root"` (`src/cli.rs:38`) — `scan /x --full` is a parse error. No doc comment either |

`scan` has no `--known`; the Routine scope is simply the no-argument case.

### 2.3 `config` (`src/cli.rs:41-44`) → `ConfigCommand` (`src/cli.rs:99-104`)

A container with no options of its own, only a required nested subcommand.

| Sub-subcommand | Handler | Notes |
|---|---|---|
| `config path` | `println!("{}", path.display())` → `main.rs:41` | prints the *resolved* path, so `config path` and `--config X config path` can differ |
| `config show` | `config::show(&config::load_or_create(&path)?)` → `main.rs:43` | creates the file if absent |
| `config edit` | `editor::resolve_editor()`, spawn, re-validate → `main.rs:45-80` | two fail-closed checks live here, not in `cli.rs` |

There is no `config init`: the variant does not exist. Asserted twice —
`src/cli.rs:219` and `src/cli.rs:428-431`.

### 2.4 `clean` (`src/cli.rs:45-97`)

Eleven tokens. The two orchestration selectors and the three clean modes are
disjoint sets by construction.

| Token | Kind | Default | Conflicts with | Maps to |
|---|---|---|---|---|
| `[ROOT]` | optional positional | `None` | `--known`, `--full` | `cli::absolutize_root(&root)` → `main.rs:113-114` |
| `--known` | `bool` | `false` | `ROOT`, `--full` | `policy::resolve(ScanRequest{cli_root:None,full:false})`, take `ScanScope::Routine` → `main.rs:132-143` |
| `--full` | `bool` | `false` | `ROOT`, `--known` | Full scan first (`main.rs:116`), then `discovery_state::load_default()` learned roots → `main.rs:120-131` |
| `--min-reclaimable-bytes <N>` | `Option<u64>` | `None` → `config.cleanup.policy.min_reclaimable_bytes` | — | `CleanupPolicy.min_reclaimable_bytes` via `.unwrap_or(configured…)` → `main.rs:157-158` |
| `--older-than <SECONDS>` | `Option<u64>` | `None` → `config.cleanup.policy.min_inactive_seconds` | — | `CleanupPolicy.min_inactive_seconds` via `.or(configured…)` → `main.rs:159` |
| `--include <GLOB>` | `Vec<String>`, repeatable | empty → `config.cleanup.policy.include` | — | `CleanupPolicy.include` → `main.rs:160-164` |
| `--exclude <GLOB>` | `Vec<String>`, repeatable | empty → `config.cleanup.policy.exclude` | — | `CleanupPolicy.exclude` → `main.rs:165-169` |
| `--profile <NAME>` | `Option<String>`, `NonEmptyStringValueParser` | `None` | `--package` | `CleanupSelector::Profile` → `main.rs:171-173` |
| `--package <SPEC>` | `Option<String>`, `NonEmptyStringValueParser` | `None` | `--profile` | `CleanupSelector::Package` → `main.rs:171-173` |
| `--dry-run` | `bool` | `false` (preview is the no-flag mode) | `--yes`, `--dryrun` | `CleanMode::Preview` |
| `--dryrun` | `bool` | `false` | `--yes`, `--dry-run` | `CleanMode::Simulate` |
| `--yes` | `bool` | `false` | `--dry-run`, `--dryrun` | `CleanMode::Execute` |

- **`--include`/`--exclude` replace, they do not merge.** An empty CLI vector
  selects the config value wholesale; a non-empty one *replaces* it
  (`main.rs:160-169` are `if … is_empty() { configured.clone() } else { cli }`,
  not an append). Adding one `--include` silently drops every configured one.
- **The numeric/glob flags fall through to config, not to a hardcoded
  default.** `cli.rs` declares no `default_value` for them, so a change to a
  `CleanupPolicy` default belongs in `config.rs`, not here.
- **`--profile`/`--package` are not free-standing.**
  `NonEmptyStringValueParser` rejects an empty value at parse time, and their
  mutual exclusion is enforced by clap, not by `main.rs` (`src/cli.rs:294-305`).

**`--dry-run` vs `--dryrun`.** Two distinct fields, `dry_run: bool`
(`src/cli.rs:76`) and `dryrun: bool` (`src/cli.rs:80`), mutually exclusive at
parse time, with the distinction stated in both doc comments
(`src/cli.rs:73-74`, `src/cli.rs:77-78`). The mapping is `main.rs:100-110`:

```rust
// Distinct spellings, distinct semantics:
// `--dry-run` (Cargo preview, default) vs `--dryrun` (simulation).
let mode = if yes {
    cargo_cleanme::cleanup::CleanMode::Execute
} else if dryrun {
    cargo_cleanme::cleanup::CleanMode::Simulate
} else {
    // Default and explicit `--dry-run` both preview via Cargo.
    let _ = dry_run;
    cargo_cleanme::cleanup::CleanMode::Preview
};
```

The `let _ = dry_run;` at `main.rs:108` is the honest core: **`--dry-run`
carries no information the mode dispatch uses.** It exists so the default mode
can be named explicitly and validated against the other two. Because the two
flags differ by a single hyphen, both parse and do different work; pairing them
is rejected rather than silently resolved (`src/cli.rs:243-246`). This is a
deliberate ergonomic/clarity trade recorded in the parser's own help text and
in `man/cargo-cleanme-clean.1` — renaming or aliasing either flag is a
documented-contract change, not a typo fix.

### 2.5 `update` (`src/cli.rs:85-97`)

Exactly one option: `--dry-run` (`bool`) → `update::update(dry_run)` at
`main.rs:23`, which also selects between the `update_json` document and the
human lines at `main.rs:24-36`. Its long doc comment (`src/cli.rs:85-92`) is
the provenance/safety contract — registry is the version authority, the tag is
constructed not scraped, `.sha256` must verify, the binary must self-identify
as the target version, and a Cargo-managed install is refused. That comment is
reproduced verbatim in `man/cargo-cleanme-update.1`, so editing it changes
generated documentation.

### 2.6 `OutputFormat` (`src/cli.rs:4-9`)

`#[derive(clap::ValueEnum)]`, `#[default] Human`:

| Variant | Spelling | Downstream effect |
|---|---|---|
| `Human` (default) | `human` | `report::render` / `CleanReport::render` on stdout (`main.rs:521`, `main.rs:240`); inline progress also permitted (`main.rs:328`, `main.rs:196`); the `state generation last_full_at=…` line prints (`main.rs:211-215`) |
| `Json` | `json` | exactly one line of `output::scan` / `output::cleanup`, or the hand-built `update_json` object (`main.rs:584`); inline progress suppressed by the `format == Human` guard |

`--format` is orthogonal to `--stats`: `--stats` always writes to stderr
(`main.rs:562`, `main.rs:247`) and never changes stdout. Pinned end to end by
`json_scan_is_one_versioned_document_and_stats_stay_on_stderr`
(`tests/cli_contract.rs:177`), which asserts `plain.stdout == with_stats.stdout`.

### 2.7 `Cli::scan_root()` and its precedence rule

```rust
pub fn scan_root(&self) -> Option<PathBuf> {          // src/cli.rs:106
    match &self.command {
        None => None,
        Some(Command::Scan { root, .. }) => root.clone(),
        _ => None,
    }
}
```

The rule: **only an explicit `scan ROOT` is a scan root.** Everything else is
`None` — including `None` (no subcommand), which *means* scan but has no root,
and including `clean ROOT`, a cleanup root belonging to a different pipeline.
It is a narrow accessor, not a general "what root did the user mean" query. It
does **not** absolutize, and it currently has **no production caller** — the
only references in `src/` are its definition (`src/cli.rs:106`) and its own
test (`src/cli.rs:213`); `main.rs` destructures
`Some(Command::Scan { root, full })` directly at `main.rs:263-265`. It is public
surface only its own test exercises, which is the shape a future refactor
quietly deletes.

---

## 3. Argv normalization

### 3.1 Why it is needed

Cargo executes an *external subcommand* by locating `cargo-cleanme` on `PATH`
and invoking it with the subcommand name injected as the **second** argv
element, so `cargo cleanme scan /x` reaches the process as:

```text
argv[0] = "cargo-cleanme"   (path Cargo resolved, may be anything)
argv[1] = "cleanme"         (injected by Cargo)
argv[2] = "scan"
argv[3] = "/x"
```

clap's first positional is the subcommand name, so it would try to match
`cleanme` against `scan`/`config`/`clean`/`update` and fail. The doc comment at
`src/cli.rs:127-137` gives the full argument: `cleanme` is *not* a
`cargo-cleanme` subcommand, so the only valid first arguments after
normalization are `scan`/`config`/`clean`/help/version flags, or no argument at
all — which is what makes stripping position 1 safe.

### 3.2 The algorithm, quoted

```rust
pub fn normalize_cargo_argv(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = args.into_iter().collect();
    if argv.len() >= 2 && argv[1] == "cleanme" {
        argv.remove(1);
    }
    argv
}                                                       // src/cli.rs:138-144
```

1. **Collect** the whole iterator into a `Vec<OsString>`. It must own the
   buffer because the next step mutates it; `args_os()` is only an iterator.
2. **Guard on length.** `argv.len() >= 2` means a bare invocation
   (`["cargo-cleanme"]`) returns unchanged; `args_os()` always yields at least
   argv[0], so a no-argument invocation can never be mistaken for an injected
   one.
3. **Strip exactly one token, at index 1, only if it equals the literal
   `cleanme`.** The comparison is `OsString == &str`, a byte-exact match on a
   single name. `argv[0]` — the resolved executable `Path`, which Cargo sets to
   whatever it found and which on Windows carries an `.exe` suffix — is never
   inspected, compared, or removed. Index 1 is the only position considered.

Everything after index 1 is left byte-for-byte alone, so `scan cleanme` keeps
its root and `clean ./cleanme --yes` keeps its path
(`later_literal_cleanme_values_are_preserved`, `src/cli.rs:366-379`).

**Accepted trade-off.** Because the strip is unconditional on position, a
*direct* invocation of `cargo-cleanme cleanme` also loses the token and parses
as *no subcommand* — a Routine scan — rather than erroring. That is the one
shape where normalization changes a literal reading of the user's command. The
doc comment's answer is that the shape cannot be a valid direct command, so
treating it as a scan beats rejecting a no-argument `cargo cleanme`. It is a
real ambiguity, not an oversight; do not "fix" the condition without deciding
which behaviour is wanted.

### 3.3 The two entry points

```rust
pub fn parse_normalized() -> Self {                    // src/cli.rs:115
    Self::parse_from(normalize_cargo_argv(std::env::args_os()))
}

pub fn try_parse_normalized_from(                        // src/cli.rs:120
    args: impl IntoIterator<Item = OsString>,
) -> Result<Self, clap::Error> {
    Self::try_parse_from(normalize_cargo_argv(args))
}
```

`parse_normalized` is the **only** production parse entry point (`main.rs:16`).
It differs from `Cli::parse` in exactly one respect: `std::env::args_os()` is
piped through `normalize_cargo_argv` first. Everything else —
`#[command(name = "cargo-cleanme", version, about = …)]` at
`src/cli.rs:12-16` — is unchanged, which is why `cargo cleanme --help` and
`cargo-cleanme --help` print byte-identical text (`tests/cli_contract.rs:120`).

`try_parse_normalized_from` is the fallible, argv-injecting twin. It exists so a
caller can assert the *real* production path — normalize-then-parse — over a
synthetic argv, instead of reaching past the parser and calling
`try_parse_from` on a hand-normalized vector. It has exactly one caller in the
tree, `src/cli.rs:317`, where
`orchestration_roots_are_mutually_exclusive_and_external_forms_match` compares
the `Debug` rendering of the direct parse against the external parse. It is
`pub` and not `#[cfg(test)]`, so it is public surface even though only an
inline test currently consumes it.

---

## 4. Scan-root absolutization

### 4.1 The two public functions

```rust
pub fn absolutize_root(root: &Path) -> PathBuf {         // src/cli.rs:153
    if root.is_absolute() {
        return root.to_path_buf();
    }
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    absolutize_relative(&base, root)
}
```

- An **absolute** input is returned **completely untouched** — not normalized,
  not `..`-folded, not validated. `absolutize_root("/a/b/../c")` returns
  `/a/b/../c` (`absolute_root_is_stable`, `src/cli.rs:434-441`).
- A **relative** input resolves against the process current directory. If
  `current_dir()` fails (deleted cwd, permission), the base silently degrades
  to `/`: the result is still absolute, so the boundary invariant holds, but it
  is absolute relative to the filesystem root rather than to where the user
  was. Nothing reports the fallback.
- The function is **purely lexical**. Beyond that `current_dir` lookup it makes
  no syscall, so it never fails on a non-existent target and can never follow a
  symlink.

`absolutize_root_for_test(base, root)` (`src/cli.rs:166`) is the same function
with the base injected. Its doc comment gives the reason: production uses the
current directory, tests use the helper *"to avoid mutating the process-global
current directory"* — correct, because `set_current_dir` is process-global and
races any parallel test. It is `#[doc(hidden)]` and, in the current tree, has
**no call sites**: the inline tests reach `absolutize_relative` directly for
the `..` cases (`src/cli.rs:449-454`) and call `absolutize_root` for the
cwd-based ones (`src/cli.rs:440`, `:461`).

### 4.2 `absolutize_relative` — the lexical algorithm

```rust
pub(crate) fn absolutize_relative(base: &Path, relative: &Path) -> PathBuf {  // src/cli.rs:173
    let mut out = if base.is_absolute() { base.to_path_buf() }
                  else { PathBuf::from("/").join(base) };
    for component in relative.components() {
        match component {
            Component::Prefix(part) => { out = PathBuf::from(part.as_os_str()); }
            Component::RootDir      => { out.push(component.as_os_str()); }
            Component::CurDir       => {}
            Component::ParentDir    => { let _ = out.pop(); }
            Component::Normal(part) => { out.push(part); }
        }
    }
    out
}
```

- A relative *base* is anchored at `/` first, so the output is absolute
  regardless of what it is handed. That is the invariant which makes it safe
  for the cleanup boundary.
- `CurDir` is a no-op — how `./project` becomes `<base>/project`, asserted by
  `relative_root_becomes_absolute` (`src/cli.rs:444-448`).
- `ParentDir` is `PathBuf::pop` with its failure discarded via `let _ =`, so a
  `..` that would rise above the root **saturates at the root** rather than
  escaping: `absolutize_relative("/a", "../../x") == "/x"`
  (`src/cli.rs:453-454`). Intermediate `..` collapse lexically: `("c/../d")`
  against `/a/b` gives `/a/b/d` (`src/cli.rs:449-450`).
- `Prefix` and `RootDir` exist for Windows correctness: `Prefix` **replaces**
  the accumulator, `RootDir` pushes onto it. This is the only Windows-specific
  code in the file.

### 4.3 Why absolutize at all

`clean` accepts a relative `ROOT` for ergonomics, but the cleanup layer's
authorization and ownership proof assume an absolute root. `main.rs:111-114` is
the whole bridge:

```rust
// Accept a relative CLI root for ergonomics, but keep the
// cleanup safety boundary absolute.
let (roots, state_generation) = if let Some(root) = root {
    (vec![cli::absolutize_root(&root)], None)
```

The asymmetry is real: **`scan ROOT` is not absolutized.** `main.rs:317-323`
passes the parsed `PathBuf` straight into
`policy::resolve(ScanRequest { cli_root: root, full })`. Only the `clean` path
calls `absolutize_root`. That is defensible — scan is read-only and
`policy::resolve` is the scope authority — but "relative roots behave
identically for `scan` and `clean`" is not true, and a change to absolutization
must be reasoned about on the cleanup side only.

### 4.4 Absolutize vs canonicalize, and `collapse_roots`

`absolutize_root` is applied at `main.rs:114`, and then `main.rs:154` calls
`collapse_roots`, whose first act is a *different* operation:
`.map(|p| std::fs::canonicalize(&p).unwrap_or(p))` (`main.rs:272`).

| | `absolutize_root` (`src/cli.rs:153`) | `canonicalize` (`main.rs:272`) |
|---|---|---|
| Symlinks in the path | left intact | resolved to their targets |
| `..` | folded lexically | folded *after* symlink resolution, so it means something different |
| Non-existent path | succeeds (string work) | fails → `.unwrap_or(p)` keeps the absolutized form |
| Failure mode | none | silently falls back |

- **A symlinked relative root** is absolutized to a path still containing the
  symlink; `collapse_roots` then canonicalizes it to the target, and the two
  values genuinely differ. Downstream a symlinked *root* is refused outright by
  discovery (`src/discovery.rs:385-386`), so the divergence is caught — by a
  later layer, not by `cli.rs`.
- **A non-existent relative root** survives `absolutize_root`, then survives
  `collapse_roots` via `.unwrap_or(p)`, reaching cleanup as an absolute path
  that does not exist. Nothing in `cli.rs` rejects it.
- **A relative root containing `..` through a symlink** is the genuinely
  ambiguous case: lexical `..` and post-resolution `..` can name different
  directories. `cli.rs` picks lexical, `collapse_roots` picks resolved, and no
  test covers the disagreement.

---

## 5. Internals worth knowing

### 5.1 `parse_normalized` uses clap's own exit path

`Cli::parse_normalized` returns `Self`, not `Result<Self, clap::Error>`, because
it delegates to `Parser::parse_from` (`src/cli.rs:116`), which is
`try_parse_from(…).unwrap_or_else(|e| e.exit())`. On a usage error clap
**prints its own message and exits the process**; it never returns to `main()`.
There is no funnel from parse failure into `AppError`, which is visible in
`main.rs:6-14`:

```rust
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("cargo-cleanme: {e}");     // main.rs:10 — runtime errors only
            std::process::exit(2)
        }
    }
}
```

The `Err` arm only ever sees an `AppError` raised *after* parsing succeeded.

**Exit-code implications.** The project contract is `0` success, `1` partial
failure or blocked scope, `2` fatal error. clap's usage-error code is also `2`
and its help/version path is `0`, so the numbers line up — by coincidence of two
independent defaults, not by design in this file. Two observable consequences:
a usage error is **not** prefixed with `cargo-cleanme: ` the way every
`AppError` message is (`main.rs:10`), so scripts parsing stderr face two
formats; and because clap exits directly, any future attempt to catch a parse
error and route it through `AppError` would be a behaviour change, not a
refactor. `try_parse_normalized_from` is the escape hatch for callers that want
a `Result` — which is why it exists alongside `parse_normalized` rather than
the latter returning a `Result` that `main()` would immediately re-bury.

### 5.2 `clean`'s root requirement is not modelled in clap

There is no `ArgGroup` and no `required_unless_present_any` in `cli.rs`; the
grep returns nothing. "ROOT or `--known` or `--full`" is enforced at
`main.rs:149-153` as
`AppError::Config("clean requires ROOT, --known, or --full")` → exit **2**. The
*pairwise* exclusions (ROOT vs `--known`, `--known` vs `--full`, mode flags
against each other) **are** modelled as `conflicts_with*` attributes and produce
clap's exit 2. So `clean --known --full` is a clap usage error while bare
`clean` is an `AppError`: same code, different message provenance.

Relatedly, `main.rs:17` resolves the config path *before* the `match` at
`main.rs:21`, so the path is resolved for every subcommand — including
`config path`, `config edit`, and `update` — and nothing checks existence at
that point.

---

## 6. Invariants and edge cases

1. **Parser and generated docs must agree.** Completions and manpages are
   generated from this clap model, and CI fails on drift: `generated-docs`
   (`.github/workflows/ci.yml:64-76`) runs
   `cargo run --quiet --features dev-tools --bin generate-docs -- --check`,
   comparing regenerated output against the checked-in `man/` and
   `completions/` trees. `xtask/src/main.rs:96` builds the model from
   `cargo_cleanme::cli::Cli::command()`, wrapped in a synthetic `cargo` parent
   (`xtask/src/main.rs:100-104`) so completions offer the typeable
   `cargo cleanme` spelling. **Any flag add, rename, or doc-comment edit fails
   this job until `man/*.1` and `completions/*` are regenerated.**
2. **Direct and external forms must parse identically.** Unit guard:
   `direct_and_external_forms_parse_equivalently` (`src/cli.rs:398`), comparing
   `format!("{a:?}")` against `format!("{b:?}")`. Process guard:
   `tests/cli_contract.rs:120` and `:154`, diffing real stdout. A `PathBuf` or
   ordering-sensitive `Vec` value is where a normalization bug would hide.
3. **The `cleanme` strip is positional.** A new subcommand or positional that
   could legitimately be the literal string `cleanme` at index 1 would collide
   with the injection token. Only index ≥ 2 is currently reachable, which is
   why the `cleanme`-as-a-root tests pass.
4. **The `--` separator is clap's default, not a project decision.**
   `src/cli.rs` has no `trailing_var_arg`, no `allow_hyphen_values`, no
   `ArgGroup`. So the first `--` is consumed and all later tokens become
   positional values: `cleanme clean -- --yes` treats `--yes` as the `ROOT`
   path, not as the execute flag. (Derived from the absence of those attributes
   rather than an executed run; no test pins `--`.)
5. **Unknown flags fail closed.** Clap rejects them at parse time with exit 2
   before any filesystem or config access. No prefix-matching or
   suggestion-suppression is configured, so clap's did-you-mean output is
   available — a user-facing string that shifts with the flag set.
6. **Path-like values are unvalidated here.** `ROOT`, `--config`, the
   `--include`/`--exclude` globs and the `--package` spec are taken as typed. A
   value starting with `-` needs `--` or `=` (`--package=--weird`).
   `--profile`/`--package` additionally reject the empty string at parse time
   (`src/cli.rs:68`, `src/cli.rs:71`); nothing else does.
7. **Platform behaviour.** `absolutize_relative`'s `Prefix`/`RootDir` arms are
   the Windows path logic; `relative_root_becomes_absolute` mixes POSIX-only
   expectations (`/a/b`) with one `cfg!(windows)`-conditional assertion
   (`C:\projects`, `src/cli.rs:435-439`).
   `tests/cli_contract.rs:284` scopes its stub-Cargo case to `#[cfg(unix)]`
   because a `#!/bin/sh` file named `cargo` is not a Windows executable.
8. **Threading a new flag** touches, in order: the parser field here → the
   destructuring pattern and branch in `main.rs` (the `Clean` arm at
   `main.rs:85-98` enumerates every field explicitly, so a new field is a
   compile error until handled) → `CleanupPolicy` / `CleanupSelector`
   construction at `main.rs:156-173`, which is also where the config fallback
   is decided → `output.rs` if it changes the machine contract, since
   `output::cleanup`/`output::scan` are the versioned `schema_version: 1`
   boundary and the overview's invariant 6 forbids letting an internal change
   reach the JSON shape implicitly.

---

## 7. Testing

### 7.1 Inline module: 17 `#[test]` functions

`src/cli.rs` has **17** `#[test]` functions. `#[cfg(test)]` is at
`src/cli.rs:200`, `mod tests {` opens at `src/cli.rs:201`, and the module runs
to end of file (305 of 506 lines).

| Test | Line | What it pins |
|---|---|---|
| `empty_is_scan_intent` | 206 | `command.is_none()` for a bare argv — "no subcommand means Routine scan" |
| `root_parses` | 211 | `scan /tmp` → `scan_root() == Some("/tmp")` |
| `config_command_parses` | 216 | `config path`/`edit` ok; `config init` **err** |
| `clean_requires_root_and_accepts_explicit_execution` | 222 | `clean` alone parses (the requirement is runtime, not clap); `--yes` ok; `--yes --dry-run` err |
| `clean_modes_conflict_pairwise_and_default_is_preview` | 231 | all three modes parse individually; all three *pairs* are errors; external form works for four argv shapes |
| `cleanup_policy_options_are_repeatable_and_mode_independent` | 261 | `--min-reclaimable-bytes 42`, `--older-than 60`, **two** `--include` values accumulate, `--exclude` lands; `--profile` + `--package` is an error |
| `orchestration_roots_are_mutually_exclusive_and_external_forms_match` | 309 | `--known`/`--full`/`ROOT` exclusions; **the only caller of `try_parse_normalized_from`** — direct vs external `Debug` equality |
| `direct_argv_is_unchanged` | 334 | no-arg and `scan /x` argv returned byte-identical |
| `cargo_external_argv_strips_exactly_one_documented_token` | 346 | one token removed for `cleanme`, `cleanme scan`, `cleanme config init`, `cleanme clean --yes` |
| `later_literal_cleanme_values_are_preserved` | 366 | `scan cleanme`, `cleanme scan cleanme`, `clean ./cleanme --yes` — the literal survives at every position ≥ 2 |
| `help_and_version_forms_normalize` | 382 | `--help`, `--version`, `clean --help` all normalize identically |
| `direct_and_external_forms_parse_equivalently` | 398 | `Debug` equality for four direct/external pairs |
| `config_init_is_removed_and_edit_parses` | 428 | re-asserts `config init` err, `config edit` ok |
| `absolute_root_is_stable` | 434 | `absolutize_root` returns an absolute path unchanged (POSIX and Windows spellings) |
| `relative_root_becomes_absolute` | 444 | `./project` → `cwd/project`; three `absolutize_relative` `..` cases: `c/../d` → `/a/b/d`, `../outside` → `/a/outside`, `../../x` from `/a` → `/x` (root saturation) |
| `no_progress_flag_parses_globally` | 458 | global before the subcommand, default false, legal with **no** subcommand |
| `stats_flag_parses_globally_in_direct_and_external_forms` | 469 | global before the subcommand; combinable with `--no-progress`; accepted after `clean` in all three modes and in external form |

### 7.2 Integration: `tests/cli_contract.rs`

7 test functions, all driving the real binary through
`env!("CARGO_BIN_EXE_cargo-cleanme")`.

| Test | Line | Contract asserted |
|---|---|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 80 | **The premise of the external-form cases, as its own test.** Copies the built binary into a temp dir, prepends it to `PATH`, isolates `CARGO_HOME`, asserts via `assert_staged_is_resolved` (`:63`) that the staged file is the first `cargo-cleanme<EXE_SUFFIX>` Cargo would resolve, *then* asserts `cargo cleanme --version` stdout == direct `--version` stdout. Its comment records why: an unrelated `cargo-cleanme` on `PATH` would otherwise satisfy every equality assertion while the product under test never ran |
| `cargo_external_subcommand_help_matches_direct_help` | 120 | `direct_stdout == external_stdout` for `--help`, and the output contains `cargo-cleanme` |
| `cargo_external_config_edit_help_matches_direct` | 154 | same equality for `config edit --help`, plus the text contains `edit` — catches a lost `ConfigCommand::Edit` |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 177 | drives `--config <path> --no-progress [--stats] --format json scan <root>`; asserts `schema_version == 1`, `operation == "scan"`, `scope == "explicit"`, `--stats` leaves stdout byte-identical, stderr contains `scan stats:`, and stdout is exactly **one** newline |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 208 | `clean <root> --dryrun --format json` → `json["mode"] == "simulate"` and `result.summary.simulated == 0`. The only process-level proof that the hyphenless spelling selects `CleanMode::Simulate` |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 246 | an invalid `Cargo.toml` gives `status.code() == Some(1)` together with `result.scope_blocked == true` |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` (`#[cfg(unix)]`) | 286 | against a `#!/bin/sh` Cargo stub: `--yes --profile dev` → `reason_code == "cleaned"`, `selector_kind == "profile"`, `selector_value == "dev"`, exactly **one** `clean` logged, `--profile` in the stub argv; `--dryrun` → `outcome == "simulated"`, count still 1; `--min-reclaimable-bytes 1 --yes` with a profile → `reason_code == "selector_unsupported"` / `policy_disposition == "selector_estimate_unavailable"`, count still 1; `--package missing-package` → `selector_invalid`; `--package fixture` with `CARGO_VERSION=1.97.0` → `selector_unsupported`; `--package fixture@0.1.0` → `cleaned`, `selector_kind == "package"`, count reaching 2 |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 582 | `--config "config path with spaces.toml" config edit` against a fake editor: exit `17` when the editor fails; non-zero exit plus stderr containing `is invalid` on a malformed edit, and the bad file is *not* reverted; success plus `edited` on stdout when valid; a no-op edit leaves `recency_seconds == 42` |

### 7.3 What is not covered

- **No test for the `--` separator**, in `cli.rs` or `cli_contract.rs`. The
  behaviour asserted in §6.4 rests on the absence of
  `trailing_var_arg`/`allow_hyphen_values`, not on a test.
- **No test for invalid values**: `--format yaml`, or a non-numeric / negative /
  overflowing `--min-reclaimable-bytes` or `--older-than`.
  `NonEmptyStringValueParser` is exercised for the `--profile`/`--package`
  *conflict*, not for an actually-empty value.
- **No test for `cargo cleanme clean` with no root selector.**
  `src/cli.rs:222` asserts it *parses*; the `AppError` and exit 2 from
  `main.rs:149-153` are not asserted anywhere I could find. Likewise nothing
  covers `update --dry-run`, `config path`, or `config show` output — the whole
  `Update` arm and two of three `ConfigCommand` arms are untested at the
  integration level.
- **No test for `absolutize_root` under symlinks or a non-existent relative
  root**, and none for the absolutize-vs-`collapse_roots` disagreement in §4.4.
  `absolutize_root_for_test` has no call sites at all.
- **No negative test for a path-shaped flag value**, e.g.
  `cleanme --config --stats scan`, where `--stats` would be consumed as the
  config path.

---

## 8. Review checklist

1. **Did you regenerate the generated docs?** Any flag add, rename, or
   doc-comment edit changes `man/*.1` and `completions/*`, and
   `.github/workflows/ci.yml:75-76` fails the build without it. The comparison
   model comes from `cargo_cleanme::cli::Cli::command()` at
   `xtask/src/main.rs:96`.
2. **Does the flag work in both invocation forms?** Add it to
   `direct_and_external_forms_parse_equivalently` (`src/cli.rs:398`), or use
   the production `try_parse_normalized_from` path as `src/cli.rs:309` does. A
   `Vec` or `PathBuf` value is where an order-sensitive mismatch appears.
3. **Is `normalize_cargo_argv` still correct after your change?** Its whole
   safety argument is that no valid command can have `cleanme` at argv[1]
   (`src/cli.rs:127-137`). If a new subcommand or positional could produce that
   shape, the shim must change with it;
   `later_literal_cleanme_values_are_preserved` (`src/cli.rs:366`) encodes the
   current position rule.
4. **Did you handle the field in `main.rs`?** The `Clean` arm
   (`main.rs:85-98`) destructures every field explicitly, so a new field is a
   compile error — verify you added the *semantics* at `main.rs:156-173`, not
   just the binding. Decide whether the new flag **replaces** or **merges**
   with the config value: `--include`/`--exclude` replace (`main.rs:160-169`)
   while `--min-reclaimable-bytes`/`--older-than` fall back
   (`main.rs:157-159`).
5. **Did you disturb the two-spelling dry-run contract?** `dry_run`
   (`src/cli.rs:76`) and `dryrun` (`src/cli.rs:80`) must stay distinct and
   mutually exclusive, and the `let _ = dry_run;` at `main.rs:108` must stay
   honest. `tests/cli_contract.rs:208` is the only process-level proof that
   `--dryrun` → `mode: "simulate"`.
6. **Is a new exclusion enforced the way you intend?** `clean`'s "ROOT or
   `--known` or `--full`" rule is a runtime `AppError` (`main.rs:149-153`), not
   an `ArgGroup`, so it exits 2 with an `AppError`-shaped message while the
   pairwise conflicts exit 2 with a clap message. Pick a mechanism
   deliberately rather than adding a third style.
7. **Does the error path match your expectation?** Parse failures never reach
   `AppError`; clap prints and exits (`src/cli.rs:116` → `main.rs:6-14`).
   Anything that must read `cargo-cleanme: …` with exit 2 has to be raised from
   `main.rs`, not from a `value_parser` or a `conflicts_with`.
8. **Have you checked the root path arithmetic?** If you touch
   `absolutize_relative` (`src/cli.rs:173-199`), re-assert the three
   properties: output is always absolute even for a relative `base`; `..`
   saturates at the root (`let _ = out.pop()`); symlinks are never followed.
   And remember `absolutize_root` leaves an already-absolute input entirely
   alone (`src/cli.rs:154-156`), `..` components included.
9. **Does your change alter the machine contract?** If it changes what
   `output::scan`/`output::cleanup`/`update_json` emit, it needs a
   `schema_version` decision in `output.rs`; the single-newline,
   one-document assertions at `tests/cli_contract.rs:204` and `:242` are the
   guard.
10. **Is there a test that asserts your new behaviour, or only one that
    exercises it?** A flag added without a case in `cli.rs` or
    `cli_contract.rs` is covered by nothing — §7.3 is the precedent for what
    silently rots.
