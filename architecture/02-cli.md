# CLI — argument parsing, invocation intent, and the cargo subcommand shim

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** the dead public API noted in §4.1 (`absolutize_root_for_test`) was
> reviewed and **kept** — removing `pub` from a crates.io-published crate is a
> semver-visible change. See [overview §10.1](overview.md).
>
> **M019 / ADR 008 §4b.** The **root is the canonical cleanup front door**.
> `cargo cleanme` is the maintenance scope, `cargo cleanme ROOT` is a bounded
> root, and `cargo cleanme --full` is the Full cleanup. The `clean` subcommand
> survives as a **hidden alias** with the same intent. See §2.4 and §2.5.
>
> **M014 / ADR 006.** Bare invocation is anchored to the working directory,
> `scan` demands an explicit scope selector, and `-i` opens a selection
> session. **M017 / ADR 007.** Scoped proof is the default; M016's
> `--partitioned` flag was removed before publication and has no alias
> (`src/cli.rs:1773`).
>
> **The root `long_about` was rewritten by C040.** It used to say that
> *"`cargo cleanme scan` with no ROOT is the full read-only machine
> reconciliation"* — since M014 that shape is a **usage error**, `scan`
> requiring one of ROOT / `--known` / `--all` (`src/cli.rs:139`). The
> `long_about` at `src/cli.rs:21-38` now says that `scan` **needs a scope**,
> and that `--all` is the full read-only machine reconciliation
> (`src/cli.rs:34-37`). The manpage is generated from the same `long_about`,
> so the help output and the manpage can no longer disagree. See §2.1.

`src/cli.rs` is 1973 lines: 777 production, 1196 inside an inline
`#[cfg(test)]` module beginning at `src/cli.rs:823`. It is the entire
user-facing contract of the binary — the clap command model, the
resolved-invocation model, the shim that makes `cargo cleanme …` parse at all,
and the lexical path helper that lets a relative `ROOT` reach the cleanup
safety boundary.

---

## 1. Responsibility

`cli.rs` owns four things: the **command model** (`Cli`, `Command`,
`ConfigCommand`, `OutputFormat`); the **resolved-invocation model**
(`CleanupScope`, `ScanIntent`, `CleanupOverrides`, `CleanupRequest`,
`Invocation`, `Cli::invocation`, `clean_mode`, `merged_clean_mode`); **argv
normalization** (`normalize_cargo_argv`, `src/cli.rs:754`), which undoes Cargo's
external-subcommand argv injection; and **scan-root absolutization**
(`absolutize_root` `src/cli.rs:773`, `absolutize_root_for_test`
`src/cli.rs:789`, shared worker `absolutize_relative` `src/cli.rs:796`).

The resolved-invocation model exists so that "no subcommand" is an *explicit*
Routine cleanup intent rather than a fallthrough. Before M012A, `main.rs`
matched `None => run_scan(None, false, …)`, and that one line decided what the
front door meant. Now the decision is data: `Cli::invocation()` maps argv onto
exactly one of **five** `Invocation` values, and `main.rs:83-98` dispatches on
that value alone. A bare invocation and `clean` with no scope selector produce
the *same* `CleanupRequest` (`src/cli.rs:299-318` and `src/cli.rs:299-318`),
so they cannot drift apart in root selection, policy, proof, or reporting.

What it deliberately does **not** do:

- **No policy interpretation.** `clean` carries `known`/`full`/
  `min_reclaimable_bytes`/… as raw fields; the Routine-vs-Full decision lives in
  `policy::resolve`, called from `main.rs:967` (maintenance scope) and
  `main.rs:1404` (scan scope).
- **No filesystem discovery.** The only filesystem call is
  `std::env::current_dir()` (`src/cli.rs:777`); there is no `exists`, `is_dir`,
  or `read_dir`. Root existence/directory/symlink checks are later layers' job
  — e.g. `src/discovery.rs:506` refuses a symlinked scan root.
- **No config loading.** `Cli` holds `config: Option<PathBuf>`
  (`src/cli.rs:42`) and resolves a *path reference*, not a policy:
  `main.rs:73` hands it to `config::ConfigPathResolver::new(cli.config.clone())
  .path()?` (`src/config.rs:71-88`), which returns the override verbatim or
  derives the platform config dir via `ProjectDirs` and appends `config.toml`.
  `cli.rs` never calls it.
- **No exit-code handling.** It never touches `std::process::exit`.

**It is no longer a leaf module.** `src/cli.rs:1-4` imports
`crate::cleanup::{CleanMode, CleanupSelection}`, `clap::{Parser, Subcommand}`,
`std::ffi::OsString`, and `std::path::{Component, Path, PathBuf}`. M012A
collapsed the three parallel mode taxonomies (two CLI booleans plus an enum)
into `cleanup::CleanMode`, which is the enum the cleanup engine already
executes on; M014 added `cleanup::CleanupSelection` for the same reason — the
working-directory anchor is a *cleanup* concept, so it is defined once, next
to the engine that has to honour it. That is a deliberate coupling and it is
worth being honest about what it costs: `cli.rs` can no longer be reviewed in
complete isolation from `cleanup.rs`. The gain is that a fourth mode, if one is
ever added, has exactly one definition site instead of three, and `cli.rs`
cannot name a mode the engine does not implement. `cleanup` does not depend on
`cli`, so the dependency is one-way.

---

## 2. The command surface

### 2.1 Global options on `Cli` (`src/cli.rs:40-140`)

`config`, `no_progress`, `stats`, `format`, `verbose`, and `interactive` are
`global = true`, so each is accepted before *or* after the subcommand name.
`--dry-run` and the hidden `--dryrun` at the root are **not** global: a blanket
`global = true` collides with `update --dry-run`'s own argument id, so the two
have to be reconciled deliberately rather than by making one of them
disappear.

That is why `invocation()` re-reads the root flag on the `Update` arm
(`src/cli.rs:416-423`) and *merges* it on the `Clean` arm
(`src/cli.rs:442-460`) rather than trusting clap to have done it. Being
non-global used to mean **silently discarded**: clap happily accepts
`cargo cleanme --dry-run clean ROOT` — the spelling Cargo itself passes — and
the `Clean` arm read only its *own* three flags, so the mode fell through to
`Execute` and a real `cargo clean` ran, reported `status=ok`, and exited 0. A
dry run that deletes and confirms everything was fine is the worst failure mode
this tool has.

Because clap cannot make a root flag mutually exclusive with a subcommand's
flags, `--dry-run` plus `clean --cargo-preview` reaches `invocation()` together.
It resolves to `Simulate` (`src/cli.rs:635-645`): simulation is the mode that
spawns no Cargo process at all, and a user who asked for it before the
subcommand must not be handed a Cargo invocation instead. It is resolved *before*
`clean_mode`, so that function's `debug_assert!` keeps its premise — clap really
does reject conflicting mode flags *on the subcommand*.

**The root's cleanup flags are not global.** M019 moved the cleanup surface to
the root: `--full` (`src/cli.rs:89-94`), the ROOT positional
(`src/cli.rs:95-102`), and the seven policy overrides
(`src/cli.rs:103-124`) are all declared on `Cli` **without** `global = true`.
They are therefore accepted only *before* a subcommand name, which is exactly
what "the root is the front door" means: `cargo cleanme --full` and
`cargo cleanme --include 'glob' ROOT` are root spellings, and the hidden
`clean` alias carries its own copies as hidden flags
(`src/cli.rs:165-216`).

| Flag | Short | Type | Default | Maps to |
|---|---|---|---|---|
| `--config <CONFIG>` | — | `Option<PathBuf>` | `None` | `ConfigPathResolver::new` → `main.rs:73` |
| `--no-progress` | — | `bool` | `false` | `progress::should_show_progress(..)` → `main.rs:1435`, `main.rs:475` |
| `--stats` | — | `bool` | `false` | `discover_manifests_with_attribution(.., options.stats)` `main.rs:1448`; the `eprintln!` blocks at `main.rs:1796`, `main.rs:512` |
| `--format <FORMAT>` | — | `OutputFormat` (value enum) | `human` | selects `output::scan` / `output::cleanup` / `report::render` → `main.rs:1688`, `main.rs:1189`, `main.rs:1700-1701`; the bounded `output::log::*` line → `main.rs:1697`, `main.rs:1199` |
| `--verbose` | — | `bool` | `false` | the detailed human surface → `report::HumanOptions::verbose()` at `main.rs:1266`, chosen by `human_options` (`main.rs:1261-1271`); the JSON document and the log line are identical with and without it (`src/cli.rs:55-61`) |
| `--dry-run` | — | `bool` | `false` | `CleanMode::Simulate` for a bare invocation *and* for `clean` / `update`, whichever side of the subcommand it was given on |
| `--dryrun` (hidden) | — | `bool` | `false` | same, as a compatibility alias |
| `-i`, `--interactive` | `-i` | `bool` | `false` | `CleanupRequest::interactive` (`src/cli.rs:320`, set at `src/cli.rs:485` and `src/cli.rs:548`) → read at `main.rs:578`; the session itself is `interactive::run_interactive` (`main.rs:647`) |
| `--full` | — | `bool` | `false` | `CleanupScope::Full` (`src/cli.rs:471-472`); conflicts with the ROOT positional (`src/cli.rs:93`) |
| `ROOT` (positional) | — | `Option<PathBuf>` | `None` | `CleanupScope::Root(root)` (`src/cli.rs:504-505`); conflicts with `--full` (`src/cli.rs:504`) |
| `--min-reclaimable-bytes <N>` | — | `Option<u64>` | `None` | `CleanupOverrides::min_reclaimable_bytes` (`src/cli.rs:555`) |
| `--older-than <SECONDS>` | — | `Option<u64>` | `None` | `CleanupOverrides::older_than` (`src/cli.rs:556`) |
| `--include <GLOB>` | — | `Vec<String>` | empty | `CleanupOverrides::include` (`src/cli.rs:557`) — repeatable |
| `--exclude <GLOB>` | — | `Vec<String>` | empty | `CleanupOverrides::exclude` (`src/cli.rs:558`) — repeatable |
| `--profile <NAME>` | — | `Option<String>` | `None` | `CleanupOverrides::profile` (`src/cli.rs:559`) |
| `--package <SPEC>` | — | `Option<String>` | `None` | `CleanupOverrides::package` (`src/cli.rs:560`) |
| `--cargo-preview` | — | `bool` | `false` | `CleanMode::CargoPreview` (`src/cli.rs:1395-1396`); conflicts with the root's own `--dry-run` / `--dryrun` (`src/cli.rs:1395`) |
| `--help` / `-h` | `-h` | clap built-in | — | clap help path |
| `--version` / `-V` | `-V` | clap built-in (from `version`, `src/cli.rs:19`) | — | `cargo-cleanme 0.2.10` |

**`--verbose` is a rendering shape, not a mode.** It is `global = true`
(`src/cli.rs:62`), so every report-producing subcommand honours it, and it
collides with no existing argument id. Its doc comment (`src/cli.rs:55-61`) is
the contract: the detailed human surface — every workspace considered
(including the ones that were skipped), the scope census, the per-phase
counters, the diagnostic detail — against a default surface that names only
what the run acted on.

**It changes the *human* format only.** The flag is lifted into `RunOptions`
(`main.rs:78`) and consumed at the human render call sites
(`main.rs:1236-1253`, `main.rs:1700-1701`) and by the progress renderer
(`main.rs:476`, `main.rs:605`, `main.rs:1436`); it never reaches
`output::scan`, `output::cleanup`, or `output::log::*`. The JSON document and
the log line are byte-identical with and without it — pinned by
`the_machine_contract_is_identical_with_and_without_verbose`
(`tests/cli_contract.rs:4630`), which drives both formats with and without the
flag and compares stdout and the exit code.

**Two short forms exist, and only one of them is global.** `-i` is the global
`--interactive` (`src/cli.rs:87`); `-a` is the `scan`-only short for `--all`
(`src/cli.rs:158`). Clap's own `-h`/`-V` complete the set. The root command has
no other short form.

`--config` carries no doc comment (`src/cli.rs:41-42`), which is why
`man/cargo-cleanme.1` renders it with an empty description body. The `--stats`
help text asserts the contract it must keep: *"Never changes deterministic
stdout"* (`src/cli.rs:46-48`).

`command: Option<Command>` (`src/cli.rs:125-126`) is **optional**, and that is
load-bearing: a bare invocation parses successfully and produces
`Invocation::Cleanup(CleanupRequest { scope: CleanupScope::Maintenance, mode:
Execute, .. })` (`src/cli.rs:469-487`), pinned by
`bare_invocation_is_routine_execute_maintenance_not_scan`
(`src/cli.rs:1262`). That test's doc comment names the exact production code it
guards: before M012A, `Cli::command` being `None` sent `main.rs` to
`run_scan(None, false, …)`.

> **The root `long_about` was rewritten (C040).** It used to advertise
> "`cargo cleanme scan` with no ROOT is the full read-only machine
> reconciliation". The `ArgGroup` at `src/cli.rs:139` makes that shape a usage
> error, and `scan --all` (`src/cli.rs:158`) is the canonical Full spelling.
> The text now says that `scan` **needs a scope** (`cli.rs:21-38`), so the
> help output, the manpage, and the parser agree. This discharges C036's D1;
> the behaviour, the group, and the test that pins it (`cli.rs:1401`) were
> always correct. M019 extended the same text to name the canonical cleanup
> spellings (`cli.rs:21-33`).

### 2.2 `scan` (`src/cli.rs:389-402`)

> **M014.** `scan` with no selector is **no longer Full**. It is a usage error.
> Full now requires an explicit `-a`/`--all`, because bare `cargo cleanme` is
> anchored to the working directory (ADR 006) and the machine-wide
> reconciliation — the only operation that may expire learned roots — must not
> sit one keystroke away from the front door.

| Token | Kind | Notes |
|---|---|---|
| `[ROOT]` | optional positional `Option<PathBuf>` | Doc-commented and rendered in the manpage. Passed **raw** (not absolutized) to `policy::resolve` as `ScanRequest.cli_root` → `main.rs:1404-1408` |
| `--known` | `bool` | Routine read-only inventory |
| `-a`, `--all` | `bool` | Full machine reconciliation. The canonical Full selector |
| `--full` (hidden) | alias of `--all` | Historical spelling of `--all`; retained as a hidden alias of the same argument |

**Exactly one selector is required**, expressed as a
`clap::ArgGroup::new("scan-scope").required(true).args(["root", "known", "all"])`
on the variant (`src/cli.rs:139`). The rendered usage line reads
`cargo-cleanme scan [OPTIONS] <ROOT|--known|--all>`.

The `ArgGroup` is not decoration. The obvious spelling —
`required_unless_present_all = ["root", "full"]` on `--all` — **does not work
for a positional**: `scan /tmp` still failed with `MissingRequiredArgument`,
because a positional ROOT does not register as "present" for that check in
clap 4. The group also makes the selectors mutually exclusive as a consequence
of the same declaration, so the conflict rules cannot drift from the requiredness
rules — `scan /tmp --known`, `scan --known --all` and `scan --all --full` are
all rejected by one declaration.

`cli.rs` cannot consult `scan.root`, so it cannot narrow the canonical
reconciliation command by accident — `policy::resolve` returns
`ScanScope::Global` before it ever looks at configuration
(`full_resolution_ignores_a_configured_root`, `src/policy.rs:961`). `Cli::invocation`
turns the three shapes into `ScanIntent::Full` / `Explicit(root)` /
`Maintenance` at `src/cli.rs:255-266`.

### 2.3 `config` (`src/cli.rs:42-45`) → `ConfigCommand` (`src/cli.rs:232-318`)

A container with no options of its own, only a required nested subcommand.

| Sub-subcommand | Handler | Notes |
|---|---|---|
| `config path` | `println!("{}", path.display())` → `main.rs:253` | prints the *resolved* path, so `config path` and `--config X config path` can differ |
| `config show` | `config::show(&config::load_or_create(path)?)` → `main.rs:257` | creates the file if absent |
| `config edit` | `editor::resolve_editor()`, spawn, re-validate → `main.rs:264-300` | two fail-closed checks live here, not in `cli.rs` |

`ConfigCommand` is `Copy` (`src/cli.rs:232`), so `Cli::invocation` hands it over
by value (`src/cli.rs:410`) and `main.rs:85` matches on it directly.

There is no `config init`: the variant does not exist. Asserted twice —
`src/cli.rs:1826` and `src/cli.rs:2031`.

### 2.4 `clean` — now a hidden alias (`src/cli.rs:685-744`)

> **M019 / ADR 008 §4b.** The `clean` subcommand is **hidden**
> (`#[command(hide = true)]` at `src/cli.rs:171`) and the **root is the
> canonical spelling**. `cargo cleanme` is the maintenance scope,
> `cargo cleanme ROOT` is a bounded root, `cargo cleanme --full` is the Full
> cleanup. `cargo cleanme clean …` remains accepted and produces the same
> intent.

The two orchestration selectors, the advanced policy flags, and the three clean
modes are disjoint sets by construction.

| Token | Kind | Default | Conflicts with | Maps to |
|---|---|---|---|---|
| `[ROOT]` | optional positional (hidden) | `None` | `--known`, `--full` | `CleanupScope::Root` → `cli::absolutize_root` → `main.rs:890` |
| `--known` | `bool` (visible) | `false` | ROOT, `--full` | `CleanupScope::Maintenance` → `policy::resolve` → `main.rs:967` |
| `--full` | `bool` (hidden) | `false` | ROOT, `--known` | `CleanupScope::Full` → Full scan first (`main.rs:919-925`), then the coverage verdict (`main.rs:939-960`) |
| *(no selector)* | — | — | — | `CleanupScope::Maintenance`, identical to bare invocation (`src/cli.rs:538-542`) |
| `--min-reclaimable-bytes <N>` | `Option<u64>` (hidden) | `None` → `config.cleanup.policy.min_reclaimable_bytes` | — | `CleanupOverrides.min_reclaimable_bytes` → `CleanupPolicy.min_reclaimable_bytes` via `.unwrap_or(configured…)` → `main.rs:356-358` |
| `--older-than <SECONDS>` | `Option<u64>` (hidden) | `None` → `config.cleanup.policy.min_inactive_seconds` | — | `CleanupPolicy.min_inactive_seconds` via `.or(configured…)` → `main.rs:359` |
| `--include <GLOB>` | `Vec<String>`, repeatable (hidden) | empty → `config.cleanup.policy.include` | — | `CleanupPolicy.include` → `main.rs:360-364` |
| `--exclude <GLOB>` | `Vec<String>`, repeatable (hidden) | empty → `config.cleanup.policy.exclude` | — | `CleanupPolicy.exclude` → `main.rs:365-369` |
| `--profile <NAME>` | `Option<String>`, `NonEmptyStringValueParser` (hidden) | `None` | `--package` | `CleanupSelector::Profile` → `main.rs:371-380` |
| `--package <SPEC>` | `Option<String>`, `NonEmptyStringValueParser` (hidden) | `None` | `--profile` | `CleanupSelector::Package` → `main.rs:371-380` |
| *(no mode flag)* | — | — | — | `CleanMode::Execute` |
| `--dry-run` | `bool` (visible) | `false` | `--cargo-preview`, `--yes`, `--dryrun` | `CleanMode::Simulate` |
| `--cargo-preview` | `bool` (hidden) | `false` | `--dry-run`, `--yes`, `--dryrun` | `CleanMode::CargoPreview` |
| `--yes` (hidden) | `bool` | `false` | `--dry-run`, `--cargo-preview`, `--dryrun` | nothing — Execute is already the default |
| `--dryrun` (hidden) | `bool` | `false` | `--dry-run`, `--cargo-preview`, `--yes` | `CleanMode::Simulate` |

- **The root carries the same policy flags, and they are not global.** The root
  declares `--min-reclaimable-bytes`, `--older-than`, `--include`, `--exclude`,
  `--profile`, `--package`, and `--cargo-preview` itself
  (`src/cli.rs:103-124`), so `cargo cleanme --include 'glob' ROOT` is a root
  spelling rather than a `clean` spelling. The advanced flags are *not*
  `global = true`, so they are accepted only before a subcommand name; a
  `scan`/`config`/`update` invocation that carries one is a **usage error**
  (`reject_cleanup_flags_for`, `src/cli.rs:571-573`), never a silently
  discarded flag.
- **`--include`/`--exclude` replace, they do not merge.** An empty CLI vector
  selects the config value wholesale; a non-empty one *replaces* it
  (`main.rs:360-369` are `if … is_empty() { configured.clone() } else { cli }`,
  not an append). Adding one `--include` silently drops every configured one.
- **The numeric/glob flags fall through to config, not to a hardcoded
  default.** `cli.rs` declares no `default_value` for them, so a change to a
  `CleanupPolicy` default belongs in `config.rs`, not here.
- **`--profile`/`--package` are not free-standing.**
  `NonEmptyStringValueParser` rejects an empty value at parse time, and their
  mutual exclusion is enforced by clap (`src/cli.rs:116-120`).

**The root and `clean` are one intent, merged.** `merged_cleanup_request`
(`src/cli.rs:497-550`) folds the root's flags and the alias's flags into one
`CleanupRequest`:

- **A value given at the root wins**; a value given on `clean` is used when the
  root did not supply one (`merge_one`, `src/cli.rs:710-721`; `merge_many`,
  `src/cli.rs:720-727`).
- **The same override at both levels is a usage error**
  (`duplicate_override`, `src/cli.rs:644-799`). Silently preferring one of two
  values is the discarded-flag failure mode ADR 008 §3 exists to prevent.
- **The scope selectors are exclusive too**: `--full` at both levels, ROOT at
  both levels, and `--cargo-preview` at both levels are each a duplicate
  (`src/cli.rs:500-508`).
- **A simulation at either level wins over a Cargo preview at either level**
  (`merged_clean_mode`, `src/cli.rs:631-679`): `--dry-run` says "invoke no
  `cargo clean` at all"; a Cargo preview does invoke one.
- **`clean --known` and `--full` are mutually exclusive** (`src/cli.rs:514-522`,
  `src/cli.rs:525-533`) — a Full scope and `--known` select different scopes.

**A directory whose name collides with a subcommand name is read as that
subcommand.** The ROOT positional is declared on the root
(`src/cli.rs:95-102`) and clap resolves a bare token that names a subcommand to
that subcommand, so `cargo cleanme scan` is the `scan` subcommand, not a
directory called `scan`. The doc comment says so explicitly and points at the
escape: spell such a directory `./scan`.

> **`--full` is now a root-level flag.** `cargo cleanme --full` is the
> canonical Full cleanup spelling (ADR 008 §4b, accepted 2026-10-21), and
> `cargo cleanme clean --full` remains accepted as a hidden alias
> (`src/cli.rs:180-182`). The root's `--full` conflicts with the root's ROOT
> positional (`src/cli.rs:93`, `src/cli.rs:101`), so the pair is a clap usage
> error rather than a silent precedence.

### 2.5 The three modes, and the two functions that map them

```rust
pub fn clean_mode(dry_run: bool, cargo_preview: bool, dryrun_legacy: bool) -> CleanMode {
    debug_assert!(
        !((dry_run || dryrun_legacy) && cargo_preview),
        "clap rejects conflicting mode flags before this mapping runs"
    );                                                  // src/cli.rs:353-356
    if dry_run || dryrun_legacy {
        CleanMode::Simulate
    } else if cargo_preview {
        CleanMode::CargoPreview
    } else {
        CleanMode::Execute
    }
}
```

This is the **single-level** mapping: one level's three mode flags onto one
mode. It is used where clap has already rejected the conflicting pair, so the
`debug_assert!` premise holds.

M019 added a second mapping, `merged_clean_mode`
(`src/cli.rs:631-641`), because the root and the `clean` alias each carry
their own mode flags and a combination across the two levels is reachable:

```rust
fn merged_clean_mode(root: (bool, bool, bool), clean: (bool, bool, bool)) -> CleanMode {
    let (root_dry_run, root_cargo_preview, root_dryrun_legacy) = root;
    let (clean_dry_run, clean_cargo_preview, clean_dryrun_legacy) = clean;
    if root_dry_run || root_dryrun_legacy || clean_dry_run || clean_dryrun_legacy {
        CleanMode::Simulate
    } else if root_cargo_preview || clean_cargo_preview {
        CleanMode::CargoPreview
    } else {
        CleanMode::Execute
    }
}
```

**A simulation at either level wins over a Cargo preview at either level.**
`--dry-run` says "invoke no `cargo clean` at all"; a Cargo preview does invoke
one. The single-level `clean_mode` keeps its stricter contract, because there
clap has already rejected the combination.

`Cli::invocation` calls `merged_clean_mode` for the bare front door
(`src/cli.rs:480-483`) and for the `clean` alias (`src/cli.rs:685-696`).
`cleanup.rs`'s inline test asserts the mapping against the stable labels rather
than against a `Default` (`cli_modes_map_to_preview_simulate_execute`,
`src/cleanup.rs:4470`).

Three properties are load-bearing:

1. **Execute is the `else` branch, not a flag.** `--yes` is accepted and
   deliberately *not* bound in `Cli::invocation` (`src/cli.rs:424-427`): it used
   to be the only way to ask for Execute, so accepting it keeps scripts working
   while printing nothing. That is the whole reason the `try_parse_normalized_from`
   caller and the alias test exist.
2. **`--dry-run` and `--dryrun` are the same mode, `--cargo-preview` is a
   different one.** Simulation spawns nothing; Cargo preview runs the same
   complete proof and then hands the decision to Cargo. Collapsing them would
   have been the ergonomic fix and would have lost the one property an operator
   debugging a gate needs.
3. **The conflicting pairs are rejected by clap, not by precedence.** All six
   mode-flag pairs are `conflicts_with_all` declarations, asserted in
   `cleanup_modes_default_to_execute_and_expose_cargo_preview`
   (`src/cli.rs:1675`). The *cross-level* combinations are not a clap concern,
   which is why `merged_clean_mode` has no `debug_assert!`.

`CleanMode` has **no `Default`** (`src/cleanup.rs:30-47`), and `CleanReport` does
not derive one either (`src/cleanup.rs:511-516`). That is the structural guard
against a future call site inheriting the weaker mode by accident: every report
states its mode in full.

### 2.6 `update` (`src/cli.rs:417-429`)

Exactly one option: `--dry-run` (`bool`) → `update::update(dry_run)` at
`main.rs:102`, which also selects between the `update_json` document and the
human lines at `main.rs:194-207`. Its long doc comment (`src/cli.rs:217-224`)
is the provenance/safety contract — registry is the version authority, the tag
is constructed not scraped, `.sha256` must verify, the binary must self-identify
as the target version, and a Cargo-managed install is refused. That comment is
reproduced verbatim in `man/cargo-cleanme-update.1`, so editing it changes
generated documentation.

`update --dry-run` is *unrelated* to cleanup `--dry-run` and is not routed
through `clean_mode`. It resolves and reports a plan without acquiring or
replacing bytes; `update_keeps_its_own_dry_run_meaning` pins that it still
parses and reaches `Invocation::Update`.

It is, however, **not** immune to the root-level flag: the arm ORs the two
(`src/cli.rs:416-423`), so `cargo cleanme --dry-run update` is also a plan-only
run. The same discarding bug reached this arm by the same route; whether a
replacement actually committed could not be exercised in the sandbox that found
it, so it is fixed by construction and by test rather than by observation.

**A cleanup flag handed to `update` is a usage error.**
`reject_cleanup_flags_for("update")` (`src/cli.rs:417-419`) rejects `--full`,
ROOT, and the seven policy overrides before the arm runs. `update` is
read-only; discarding the flag would look exactly like it was honoured.

### 2.7 `OutputFormat` (`src/cli.rs:6-14`)

`#[derive(clap::ValueEnum)]`, `#[default] Human`:

| Variant | Spelling | Downstream effect |
|---|---|---|
| `Human` (default) | `human` | `report::render` / `CleanReport::render` on stdout (`main.rs:1700-1701`, `main.rs:1236-1252`); inline progress also permitted (`main.rs:481`, `main.rs:1435`); the `state generation last_full_at=…` line prints **only** under `--verbose` (`main.rs:503-510`) |
| `Json` | `json` | exactly one line of `output::scan` / `output::cleanup`, or the hand-built `update_json` object (`main.rs:200`) |
| `Log` | `log` | one bounded ASCII `key=value` line per report (`output::log::*`, `main.rs:1697`, `main.rs:1225`); **not** a machine contract — the doc comment says so at `src/cli.rs:11` |

`--format` is orthogonal to `--stats`: `--stats` always writes to stderr
(`main.rs:1796`, `main.rs:512`) and never changes stdout. Pinned end to end by
`json_scan_is_one_versioned_document_and_stats_stay_on_stderr`
(`tests/cli_contract.rs:172`), which asserts `plain.stdout == with_stats.stdout`.

`--format` is orthogonal to `--verbose` in the same way: the flag changes the
*human* projection only. The `Json` and `Log` rows above are unaffected by it —
the JSON document and the log line are byte-identical with and without
`--verbose`. Within the human row the flag selects between two shapes, and
the difference is visible on the surface: the `combined roots …` preamble
(`main.rs:1240-1248`) and the `state generation last_full_at=…` line
(`main.rs:503-510`) belong to the verbose shape only, and the quiet shape
prints the report alone (`main.rs:1250`).

`log` is opt-in and never inferred: a non-TTY stdout does not select it
(`format_log_parses_globally`, `src/cli.rs:1796`). The `config` subcommand
**rejects** `--format json` outright rather than emitting non-JSON on stdout
(`main.rs:238-242`).

### 2.8 M012A: what moved, and what stayed

| 0.1.x spelling | Post-M012A spelling | Same behaviour |
|---|---|---|
| `cargo cleanme` (read-only Routine scan) | `cargo cleanme scan --known` | yes |
| `cargo cleanme scan --full` | `cargo cleanme scan --all` | yes (M014 made the selector explicit) |
| `cargo cleanme clean R --dry-run` (Cargo preview) | `cargo cleanme clean R --cargo-preview` | yes |
| `cargo cleanme clean R --dryrun` (simulation) | `cargo cleanme clean R --dry-run` | yes |
| `cargo cleanme clean R --yes` (execute) | `cargo cleanme clean R` | yes |
| `cargo cleanme clean --full` | `cargo cleanme --full` | yes (M019 made the root the canonical spelling; `clean --full` stays a hidden alias) |
| `cargo cleanme clean R` | `cargo cleanme R` | yes (M019 moved the ROOT positional to the root) |

What is **not** an alias: the `scan` front door. `scan` now requires an
explicit selector; `scan --known` is the Routine inventory and `scan --all` is
the Full reconciliation. `scan --full` survives as a hidden alias for `--all`,
and `hidden_full_alias_is_not_advertised_in_scan_help`
(`src/cli.rs:1578`) asserts the alias does not appear in `scan --help`, so a
reader of the help text is never offered two spellings for one operation.

Since M014 the bare front door also *changed meaning*: it is no longer "the
0.1.x read-only scan". It is a Routine **Execute** cleanup, anchored to the
working directory. `cargo cleanme scan --known` is the read-only counterpart.
Since ADR 009 the scope itself has grown to include the operator's position:
the working directory joins the maintenance scope when the Routine scope does
not already reach it, so a bare run cleans what is under it. The CLI's own
contribution is still only the `CleanupScope::Maintenance` variant — the scope
rule lives in `main.rs`, not here (`src/main.rs:1041-1052`).

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
`src/cli.rs:743-753` gives the full argument: `cleanme` is *not* a
`cargo-cleanme` subcommand, so the only valid first arguments after
normalization are `scan`/`config`/`clean`/`update`/help/version flags or no
argument at all — which is what makes stripping position 1 safe.

### 3.2 The algorithm, quoted

```rust
pub fn normalize_cargo_argv(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = args.into_iter().collect();
    if argv.len() >= 2 && argv[1] == "cleanme" {
        argv.remove(1);
    }
    argv
}                                                       // src/cli.rs:754-760
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
(`later_literal_cleanme_values_are_preserved`, `src/cli.rs:1961`).

**Accepted trade-off.** Because the strip is unconditional on position, a
*direct* invocation of `cargo-cleanme cleanme` also loses the token and parses
as *no subcommand* — which since M012A is a **Routine Execute cleanup**. That is
the one shape where normalization changes a literal reading of the user's
command, and M012A made it more consequential than it was: before, the
fallthrough was read-only; now it deletes. The doc comment's answer is unchanged
(the shape cannot be a valid direct command, so treating it as the bare
invocation beats rejecting a no-argument `cargo cleanme`), but the cost of being
wrong about it is now deletion rather than a wasted scan. Do not "fix" the
condition without deciding which behaviour is wanted — and if you do, the
argument must account for the delete, not just the parse.

### 3.3 The two entry points

```rust
pub fn parse_normalized() -> Self {                    // src/cli.rs:368
    Self::parse_from(normalize_cargo_argv(std::env::args_os()))
}

pub fn try_parse_normalized_from(                        // src/cli.rs:373
    args: impl IntoIterator<Item = OsString>,
) -> Result<Self, clap::Error> {
    Self::try_parse_from(normalize_cargo_argv(args))
}
```

`parse_normalized` is the **only** production parse entry point (`main.rs:15`).
It differs from `Cli::parse` in exactly one respect: `std::env::args_os()` is
piped through `normalize_cargo_argv` first. Everything else —
`#[command(name = "cargo-cleanme", version, about = …)]` at
`src/cli.rs:17-39` — is unchanged, which is why `cargo cleanme --help` and
`cargo-cleanme --help` print byte-identical text (`tests/cli_contract.rs:115`).

`try_parse_normalized_from` is the fallible, argv-injecting twin. It exists so a
caller can assert the *real* production path — normalize-then-parse — over a
synthetic argv, instead of reaching past the parser and calling
`try_parse_from` on a hand-normalized vector. It is `pub` and not
`#[cfg(test)]`, so it is public surface even though only inline tests and one
integration test consume it (`src/cli.rs:1912`, `tests/cli_contract.rs:2444`).

---

## 4. Scan-root absolutization

### 4.1 The two public functions

```rust
pub fn absolutize_root(root: &Path) -> Result<PathBuf, crate::error::AppError> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }
    let base = std::env::current_dir().map_err(|e| crate::error::AppError::InvalidRoot {
        path: root.display().to_string(),
        reason: format!("cannot resolve current directory: {e}"),
    })?;
    Ok(absolutize_relative(&base, root))
}                                                       // src/cli.rs:773-782
```

- An **absolute** input is returned **completely untouched** — not normalized,
  not `..`-folded, not validated.
- A **relative** input resolves against the process current directory. If
  `current_dir()` fails (deleted cwd, permission), the function now returns
  `AppError::InvalidRoot` — a usage error, exit 2. It **used to** degrade
  silently to `/`, which rewrote `clean ./p` into `clean /p`; the doc comment at
  `src/cli.rs:770-772` records that the fallback was the defect.
- The function is **purely lexical**. Beyond that `current_dir` lookup it makes
  no syscall, so it never fails on a non-existent target and can never follow a
  symlink.

`absolutize_root_for_test(base, root)` (`src/cli.rs:789`) is the same function
with the base injected. Its doc comment gives the reason: production uses the
current directory, tests use the helper *"to avoid mutating the process-global
current directory"* — correct, because `set_current_dir` is process-global and
races any parallel test. It is `#[doc(hidden)]` and, in the current tree, has
**no call sites**: the inline tests reach `absolutize_relative` directly for the
`..` cases and call `absolutize_root` for the cwd-based ones.

### 4.2 `absolutize_relative` — the lexical algorithm

```rust
pub(crate) fn absolutize_relative(base: &Path, relative: &Path) -> PathBuf {   // src/cli.rs:796
    let mut out = if base.is_absolute() {
        base.to_path_buf()
    } else {
        PathBuf::from("/").join(base)
    };
    for component in relative.components() {
        match component {
            Component::Prefix(part) => {
                out = PathBuf::from(part.as_os_str());
            }
            Component::RootDir => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Lexical pop only: never follows symlinks. A `..` that would
                // rise above the filesystem root stays at the root; otherwise
                // it resolves against the actual caller-selected directory
                // stack without escaping it through surprising semantics.
                let _ = out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}
```

- A relative *base* is anchored at `/` first, so the output is absolute
  regardless of what it is handed. That is the invariant which makes it safe
  for the cleanup boundary.
- `CurDir` is a no-op — how `./project` becomes `<base>/project`, asserted by
  `relative_root_becomes_absolute` (`src/cli.rs:2047`).
- `ParentDir` is `PathBuf::pop` with its failure discarded via `let _ =`, so a
  `..` that would rise above the root **saturates at the root** rather than
  escaping: `absolutize_relative("/a", "../../x") == "/x"`. Intermediate `..`
  collapse lexically: `("c/../d")` against `/a/b` gives `/a/b/d`.
- `Prefix` and `RootDir` exist for Windows correctness: `Prefix` **replaces**
  the accumulator, `RootDir` pushes onto it. This is the only Windows-specific
  path code in the file.

### 4.3 Why absolutize at all

`clean` accepts a relative `ROOT` for ergonomics, but the cleanup layer's
authorization and ownership proof assume an absolute root. `main.rs:886-905` is
the whole bridge:

```rust
CleanupScope::Root(root) => {                                // main.rs:886
    // Explicit roots stay authoritative: no omission, no fallback to
    // a smaller implicit set. The engine's strict validation reports
    // the defect as a fatal usage error.
    let absolute = cli::absolutize_root(root)?;              // main.rs:890
    let classified = cargo_cleanme::policy::classify_cleanup_roots(vec![(
        absolute,
        cargo_cleanme::policy::RootProvenance::Explicit,
    )])?;
    Ok(ResolvedCleanup {
        roots: collapse_roots(classified.admitted),           // main.rs:896
        …
    })
}
```

(The call site is `main.rs:890`; the roots it produces are then classified and
collapsed, so the `roots:` field in the real code is
`collapse_roots(classified.admitted)` at `main.rs:896`.)

The asymmetry is real: **`scan ROOT` is not absolutized.** `main.rs:1404-1408`
passes the parsed `PathBuf` straight into
`policy::resolve(ScanRequest { cli_root: root, full })`. Only the cleanup path
calls `absolutize_root`. That is defensible — scan is read-only and
`policy::resolve` is the scope authority — but "relative roots behave
identically for `scan` and `clean`" is not true, and a change to absolutization
must be reasoned about on the cleanup side only.

### 4.4 Absolutize vs canonicalize, and `collapse_roots`

`absolutize_root` is applied at `main.rs:890`, and `collapse_roots` then
canonicalizes. Its first act is a *different* operation:
`.map(|p| std::fs::canonicalize(&p).unwrap_or(p))` (`main.rs:1330`).

| | `absolutize_root` (`src/cli.rs:773`) | `canonicalize` (`main.rs:1330`) |
|---|---|---|
| Symlinks in the path | left intact | resolved to their targets |
| `..` | folded lexically | folded *after* symlink resolution, so it means something different |
| Non-existent path | succeeds (string work) | fails → `.unwrap_or(p)` keeps the absolutized form |
| Failure mode | only `current_dir()` can fail, and it is an error | silently falls back |

- **A symlinked relative root** is absolutized to a path still containing the
  symlink; `collapse_roots` then canonicalizes it to the target, and the two
  values genuinely differ. Downstream a symlinked *root* is refused outright by
  discovery (`src/discovery.rs:522-529`), so the divergence is caught — by a
  later layer, not by `cli.rs`.
- **A non-existent relative root** survives `absolutize_root`, then survives
  `collapse_roots` via `.unwrap_or(p)`, reaching cleanup as an absolute path
  that does not exist. Nothing in `cli.rs` rejects it.
- **A relative root containing `..` through a symlink** is the genuinely
  ambiguous case: lexical `..` and post-resolution `..` can name different
  directories. `cli.rs` picks lexical, `collapse_roots` picks resolved, and no
  test covers the disagreement.

**Where `collapse_roots` runs.** It is **not** a `run_cleanup` step any more.
`collapse_roots` is defined at `main.rs:1327-1341` and is called *inside*
`resolve_cleanup_roots`, on the admitted root set of each scope: the explicit
arm at `main.rs:896`, the configured-root arm at `main.rs:984`, the
explicit-roots arm at `main.rs:1007`, and the automatic (Routine/Full) arm at
`main.rs:1189` inside `admit_automatic`. `run_cleanup` consumes the already
collapsed `resolved.roots` (`main.rs:426` for the empty-scope check,
`main.rs:1240-1246` for the `combined roots …` preamble, which the verbose
surface prints and the quiet one does not).

That placement is deliberate and is stated in the code: C028 classifies the
**raw** candidates and collapses only the *admitted* paths, because collapsing
the canonicalized `policy.scope` set first would have resolved a symlinked root
into its target and hidden the invalid identity (`main.rs:1019-1022`).

---

## 5. Internals worth knowing

### 5.1 `parse_normalized` uses clap's own exit path

`Cli::parse_normalized` returns `Self`, not `Result<Self, clap::Error>`, because
it delegates to `Parser::parse_from` (`src/cli.rs:369`), which is
`try_parse_from(…).unwrap_or_else(|e| e.exit())`. On a usage error clap
**prints its own message and exits the process**; it never returns to `main()`.
There is no funnel from parse failure into `AppError`, which is visible in
`main.rs:11-37`:

```rust
fn main() {
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("cargo-cleanme: {e}");     // main.rs:32 — runtime errors only
            std::process::exit(2)
        }
    }
}
```

(The real body reads `match run(cli)` at `main.rs:19` and branches on
`format == OutputFormat::Log` at `main.rs:22`; in log mode the human prose line
is replaced by one bounded `output::log::fatal` line at `main.rs:27-30`.)

The `Err` arm only ever sees an `AppError` raised *after* parsing succeeded.

**Exit-code implications.** The project contract is `0` success, `1` partial
failure or blocked scope, `2` fatal error. clap's usage-error code is also `2`
and its help/version path is `0`, so the numbers line up — by coincidence of two
independent defaults, not by design in this file. Two observable consequences:
a usage error is **not** prefixed with `cargo-cleanme: ` the way every
`AppError` message is (`main.rs:32`), so scripts parsing stderr face two
formats; and because clap exits directly, any future attempt to catch a parse
error and route it through `AppError` would be a behaviour change, not a
refactor. `try_parse_normalized_from` is the escape hatch for callers that want
a `Result` — which is why it exists alongside `parse_normalized` rather than
the latter returning a `Result` that `main()` would immediately re-bury.

### 5.2 Scope conflicts are all clap errors now

Before M012A, `cli.rs` modelled only the *pairwise* exclusions, and "ROOT or
`--known` or `--full`" was enforced at runtime as
`AppError::Config("clean requires ROOT, --known, or --full")` → exit **2**. So
`clean --known --full` was a clap usage error while bare `clean` was an
`AppError`: same exit code, different message provenance.

M012A removed that runtime rule. `clean` with no selector now resolves to
`CleanupScope::Maintenance` (`src/cli.rs:538-542`), identical to bare
invocation, so **every** scope conflict is a clap usage error and there is no
third style to remember. The price is a real behaviour change, recorded in
`CHANGELOG.md`: `cargo cleanme clean` used to exit 2 and now cleans.

`main.rs:73` still resolves the config path *before* the `match` at
`main.rs:83`, so the path is resolved for every subcommand — including
`config path`, `config edit`, and `update` — and nothing checks existence at
that point.

### 5.3 `Invocation::UsageError` is a deliberate dead end

`Invocation` has five variants (`src/cli.rs:329-347`): `Cleanup`, `Scan`,
`Config`, `Update`, and `UsageError(String)`. The last one is **unreachable
through the parser** — `scan`'s `ArgGroup` guarantees one of ROOT / `--known` /
`--all` — and its doc comment (`src/cli.rs:336-344`) says why it exists: the
compiler cannot see clap's invariant, and the two available fallbacks were both
wrong. Defaulting to Full would silently run the machine-wide reconciliation the
flag exists to gate; panicking would exit 101, outside the 0/1/2 contract.
`main.rs:97` turns it into `AppError::Config(message)` — exit 2, the same code
clap uses for a usage error.

---

## 6. Invariants and edge cases

1. **Parser and generated docs must agree.** Completions and manpages are
   generated from this clap model, and CI fails on drift: `generated-docs`
   (`.github/workflows/ci.yml:96-108`) runs
   `cargo run --quiet --features dev-tools --bin generate-docs -- --check`,
   comparing regenerated output against the checked-in `man/` and
   `completions/` trees. `xtask/src/main.rs:96` builds the model from
   `cargo_cleanme::cli::Cli::command()`, wrapped in a synthetic `cargo` parent
   (`xtask/src/main.rs:109-113`) so completions offer the typeable
   `cargo cleanme` spelling. **Any flag add, rename, or doc-comment edit fails
   this job until `man/*.1` and `completions/*` are regenerated.**
   *The root manpage **does** carry `src/cli.rs`'s `long_about`: the generator
   uses the model's own `long_about`, falling back to `about`, as the page
   base (`xtask/src/main.rs:156-164`). That is why the stale `long_about`
   sentence at `src/cli.rs:27-28` never reached a generated artifact and no
   drift gate ever saw it — and why C040's rewrite of the `long_about` had to
   regenerate the manpage with it.*
2. **Direct and external forms must parse identically.** Unit guard:
   `direct_and_external_forms_parse_equivalently` (`src/cli.rs:1993`), comparing
   `Invocation` equality on the parsed value. Process guard:
   `tests/cli_contract.rs:115` and `:152`, diffing real stdout. A `PathBuf` or
   ordering-sensitive `Vec` value is where a normalization bug would hide. Since
   M012A the unit guard compares `invocation()`, not `Debug` output, so an
   `Override` value that parsed differently but rendered identically would now
   be caught.
3. **The `cleanme` strip is positional.** A new subcommand or positional that
   could legitimately be the literal string `cleanme` at index 1 would collide
   with the injection token. Only index ≥ 2 is currently reachable, which is
   why the `cleanme`-as-a-root tests pass.
4. **The `--` separator is clap's default, not a project decision.**
   `src/cli.rs` has no `trailing_var_arg` and no `allow_hyphen_values`. So the
   first `--` is consumed and all later tokens become positional values:
   `cleanme clean -- --yes` treats `--yes` as the `ROOT` path, not as a mode
   flag. (Derived from the absence of those attributes rather than an executed
   run; no test pins `--`.) The one `ArgGroup` in the file is the `scan-scope`
   group at `src/cli.rs:139`, and it constrains `scan`'s selectors only.
5. **Unknown flags fail closed.** Clap rejects them at parse time with exit 2
   before any filesystem or config access. No prefix-matching or
   suggestion-suppression is configured, so clap's did-you-mean output is
   available — a user-facing string that shifts with the flag set. M016's
   `--partitioned` is the worked example: it was removed before publication, so
   it is a plain unknown argument today
   (`partitioned_flag_is_gone_scoped_proof_is_the_only_behavior`,
   `src/cli.rs:1773`).
6. **Path-like values are unvalidated here.** `ROOT`, `--config`, the
   `--include`/`--exclude` globs and the `--package` spec are taken as typed. A
   value starting with `-` needs `--` or `=` (`--package=--weird`).
   `--profile`/`--package` additionally reject the empty string at parse time
   (`src/cli.rs:196`, `src/cli.rs:199`); nothing else does.
7. **Platform behaviour.** `absolutize_relative`'s `Prefix`/`RootDir` arms are
   the Windows path logic; `relative_root_becomes_absolute` (`src/cli.rs:2047`)
   mixes POSIX-only expectations (`/a/b`) with one `cfg!(windows)`-conditional
   assertion.
   Fifty-six `#[cfg(unix)]` attributes gate code in `tests/cli_contract.rs`,
   forty-four of them sitting directly on a `#[test]` function. They exist
   because those cases substitute a `#!/bin/sh` Cargo stub — a file that is not
   a Windows executable. Several of them additionally override `HOME`, which
   `directories` reads on Unix but which Windows resolves through a
   known-folder API that ignores the environment. Each `#[cfg(unix)]` carries
   the reason at the call site, so "deliberately scoped" cannot decay into
   "never executed".
8. **Threading a new flag** touches, in order: the parser field here → the
   destructuring pattern and branch in `Cli::invocation`
   (`src/cli.rs:428-460` enumerates every `Clean` field explicitly, so a new
   field is a compile error until handled) → `CleanupOverrides`
   (`src/cli.rs:283-291`) → `CleanupPolicy` / `CleanupSelector` construction at
   `main.rs:355-380`, which is also where the config fallback is decided →
   `output.rs` if it changes the machine contract, since
   `output::cleanup`/`output::scan` are the versioned `schema_version: 1`
   boundary and the overview's invariant 6 forbids letting an internal change
   reach the JSON shape implicitly.
9. **One mode, one definition site.** Adding a fourth cleanup mode must touch
   `cleanup::CleanMode` (`src/cleanup.rs:30-47`) and **both** mapping functions:
   `cli::clean_mode` (`src/cli.rs:352-364`) for the single-level case and
   `cli::merged_clean_mode` (`src/cli.rs:631-641`) for the root-plus-alias case.
   A new mode that is reachable from `clean` but not from the bare front door —
   or vice versa — is a safety bug, because those two functions are the only
   places the mapping exists.
10. **Bare invocation must never be weaker than `clean --known`.** They are the
    same `CleanupScope::Maintenance` value (`src/cli.rs:475-493`,
    `src/cli.rs:535-539`) reaching the same `run_cleanup`. A future change that
    special-cases `None` in `main.rs` would give the front door a weaker proof
    path than an explicit spelling, which is the one regression this milestone
    exists to make impossible.
11. **The working-directory anchor belongs to the maintenance spellings only.**
    Bare invocation and `clean --known` (and `clean` with no selector) resolve
    to `CleanupSelection::WorkingDirectory`; `clean ROOT` and `clean --full`
    resolve to `CleanupSelection::All` (`src/cli.rs:1543-1548`). The anchor is
    emitted **unresolved** — `cli.rs` must not do IO — and
    `CleanupSelection::admits` is fail-closed until `main.rs` resolves it
    (`src/cli.rs:1550`).

---

## 7. Testing

### 7.1 Inline module: 45 `#[test]` functions

`src/cli.rs` has **45** `#[test]` functions (plus two helper constructors and one
`argv` helper). `#[cfg(test)]` is at `src/cli.rs:1924`, `mod tests {` opens at
`src/cli.rs:824`, and the module runs to end of file (1196 of 1973 lines,
including helpers).

| Test | Line | What it pins |
|---|---|---|
| `the_root_full_flag_resolves_to_the_full_scope` | 854 | M019: `cargo cleanme --full` → `CleanupScope::Full` — the canonical Full spelling |
| `the_clean_alias_still_resolves_and_is_hidden_from_the_help` | 877 | M019: `clean` is hidden from the help and still parses to the same intent |
| `the_policy_overrides_work_at_the_root` | 916 | M019: the seven policy overrides are accepted at the root and reach `CleanupOverrides` |
| `the_root_positional_resolves_to_the_bounded_root_scope` | 963 | M019: `cargo cleanme ROOT` → `CleanupScope::Root(root)` |
| `a_subcommand_wins_over_a_positional_of_the_same_shape` | 988 | M019: a directory named `scan` is read as the `scan` subcommand |
| `the_dot_positional_is_a_bounded_cleanup_and_not_a_usage_error` | 1009 | M019: `cargo cleanme .` is a bounded cleanup of `.` — it used to be a usage error |
| `the_known_alias_is_not_orphaned_by_the_hiding` | 1031 | M019: `clean --known` still reaches the maintenance scope after the hiding |
| `the_root_help_names_the_canonical_full_cleanup_spelling` | 1047 | M019: the root `long_about` names `cargo cleanme --full` as the canonical spelling |
| `the_root_and_the_alias_resolve_to_the_same_intent` | 1067 | M019: `cargo cleanme ROOT` and `cargo cleanme clean ROOT` are the same `CleanupRequest` |
| `a_cleanup_flag_on_a_read_only_subcommand_is_a_usage_error` | 1097 | M019: `--full` / ROOT / a policy override handed to `scan`, `config` or `update` is a usage error, never a discarded flag |
| `the_same_override_at_both_levels_is_a_usage_error` | 1123 | M019: the same override at the root and on `clean` is a usage error |
| `a_root_level_override_wins_over_the_alias` | 1250 | M019: a root-level override wins over the alias's, and the alias's is used when the root gave none |
| `bare_invocation_is_routine_execute_maintenance_not_scan` | 1262 | `cargo-cleanme` with no argv → `Cleanup(Maintenance, Execute)`. Its comment names the pre-M012A dispatch it replaces |
| `bare_dry_run_is_simulate_and_the_legacy_alias_maps_to_it` | 1276 | `--dry-run` and `--dryrun` both → Simulate; the two spellings cannot be combined |
| `a_dry_run_before_the_subcommand_is_still_a_dry_run` | 1295 | root `--dry-run`/`--dryrun` **before** `clean ROOT` → `Simulate`, not the `Execute` it silently fell through to; the post-subcommand and no-flag spellings are unchanged |
| `a_dry_run_before_update_is_still_a_dry_run` | 1345 | root `--dry-run`/`--dryrun` before `update` → `Update { dry_run: true }`; bare `update` stays `false` |
| `a_root_dry_run_wins_over_cargo_preview_because_it_spawns_nothing` | 1368 | root `--dry-run` + `clean --cargo-preview` → `Simulate`; `clean --cargo-preview` alone is still `CargoPreview` |
| `scan_scope_selection_is_full_explicit_or_maintenance` | 1401 | bare `scan` is a **parse error**; `--all`, `-a`, and the hidden `--full` all → Full; `ROOT` → Explicit; `--known` → Maintenance |
| `the_interactive_flag_is_declared_advertised_and_not_implied` | 1437 | `-i`/`--interactive` appears in the long help, parses, is not implied by an ordinary invocation, reaches the cleanup intent, and does **not** pre-empt the anchor |
| `interactive_without_a_terminal_still_means_human_format_only` | 1492 | the flag coexists with all three output formats; the machine-format refusal is a runtime guard in `main`, not a parse-time one |
| `maintenance_spellings_anchor_selection_to_the_working_directory` | 1509 | bare, `clean`, and `clean --known` anchor to the working directory; `clean ROOT` and `clean --full` stay `All` |
| `unresolved_working_directory_anchor_selects_nothing` | 1550 | an unresolved `WorkingDirectory` anchor admits **nothing** — fail-closed against a caller that forgets to resolve it |
| `scan_scope_conflicts_fail_at_invocation_time` | 1561 | all six bad `scan` pairs are parse errors, before any traversal |
| `hidden_full_alias_is_not_advertised_in_scan_help` | 1578 | `scan --help` renders `--known` and never `--full` |
| `clean_scope_selectors_stay_mutually_exclusive` | 1621 | `ROOT`/`--known`/`--full` map to three `CleanupScope`s; bare `clean` equals bare invocation; all three bad pairs are errors |
| `the_full_cleanup_alias_still_resolves_after_the_root_moved` | 1665 | M019: `clean --full` still resolves to the Full scope after the canonical spelling moved to the root |
| `cleanup_modes_default_to_execute_and_expose_cargo_preview` | 1675 | no mode flag → Execute; `--dry-run`/cargo-preview/`--yes`/`--dryrun` map correctly; **all six** mode pairs are parse errors |
| `compatibility_aliases_are_hidden_but_accepted` | 1728 | `clean --help` shows `--dry-run` and `--cargo-preview` and hides `--yes`/`--dryrun` |
| `partitioned_flag_is_gone_scoped_proof_is_the_only_behavior` | 1773 | `--partitioned` is a usage error at the root and on `clean`; removal is total (ADR 007) |
| `update_keeps_its_own_dry_run_meaning` | 1786 | `update --dry-run` still parses into `Invocation::Update { dry_run: true }` |
| `format_log_parses_globally` | 1796 | `--format log` is accepted before and after a subcommand and through the external form; `log` is opt-in; `--format yaml` is an error |
| `config_command_parses` | 1826 | `config path`/`edit` ok; `config init` err; `config show` → `Invocation::Config(Show)` |
| `root_parses` | 1839 | `scan /tmp` → `ScanIntent::Explicit("/tmp")` |
| `cleanup_policy_options_are_repeatable_and_mode_independent` | 1848 | two `--include` values accumulate, `--exclude` lands, `--min-reclaimable-bytes`/`--older-than` land, and the whole set travels **inside** the `CleanupRequest`; `--profile`+`--package` is an error |
| `orchestration_roots_are_mutually_exclusive_and_external_forms_match` | 1908 | direct vs external `invocation()` equality for `--known`/`--full`; **the only caller of `try_parse_normalized_from`** in this file |
| `direct_argv_is_unchanged` | 1929 | no-arg and `scan /x` argv returned byte-identical |
| `cargo_external_argv_strips_exactly_one_documented_token` | 1941 | one token removed for `cleanme`, `cleanme scan`, `cleanme config show`, `cleanme clean /x --yes` |
| `later_literal_cleanme_values_are_preserved` | 1961 | `scan cleanme`, `cleanme scan cleanme`, `clean ./cleanme --yes` — the literal survives at every position ≥ 2 |
| `help_and_version_forms_normalize` | 1977 | `--help`, `--version`, `clean --help` all normalize identically |
| `direct_and_external_forms_parse_equivalently` | 1993 | six direct/external pairs, compared on `invocation()` |
| `config_init_is_removed_and_edit_parses` | 2031 | re-asserts `config init` err, `config edit` ok |
| `absolute_root_is_stable` | 2037 | `absolutize_root` returns an absolute path unchanged (POSIX and Windows spellings) |
| `relative_root_becomes_absolute` | 2047 | `./project` → `cwd/project`; three `absolutize_relative` `..` cases: `c/../d` → `/a/b/d`, `../outside` → `/a/outside`, `../../x` from `/a` → `/x` (root saturation) |
| `no_progress_flag_parses_globally` | 2061 | global before the subcommand, default false, legal with **no** subcommand — where it now means a Routine cleanup |
| `stats_flag_parses_globally_in_direct_and_external_forms` | 2075 | global before the subcommand; combinable with `--no-progress`; accepted after `clean` in all three modes and in external form |

(Two helpers, `maintenance(..)` and `cleanup(..)`, construct a `CleanupRequest`
with default overrides, and `argv(..)` builds an `OsString` vector; they are not
tests.)

### 7.2 Integration: `tests/cli_contract.rs`

The file holds **77** `#[test]` functions. The table below lists the ones that
pin a **CLI-surface** contract — argv shape, flag mapping, scope selection,
mode selection, or exit status. The remainder cover output rendering, Cargo
failure grouping, and the interactive TTY.

| Test | Line | Contract asserted |
|---|---|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 75 | **The premise of the external-form cases, as its own test.** Copies the built binary into a temp dir, prepends it to `PATH`, isolates `CARGO_HOME`, asserts via `assert_staged_is_resolved` (`tests/cli_contract.rs:58`) that the staged file is the first `cargo-cleanme<EXE_SUFFIX>` Cargo would resolve, *then* asserts `cargo cleanme --version` stdout == direct `--version` stdout |
| `cargo_external_subcommand_help_matches_direct_help` | 115 | `direct_stdout == external_stdout` for `--help`, and the output contains `cargo-cleanme` |
| `cargo_external_config_edit_help_matches_direct` | 149 | same equality for `config edit --help`, plus the text contains `edit` |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 172 | drives `--config <path> --no-progress [--stats] --format json scan <root>`; asserts `schema_version == 1`, `operation == "scan"`, `scope == "explicit"`, `--stats` leaves stdout byte-identical, stderr contains `scan stats:`, and stdout is exactly **one** newline |
| `json_scope_label_follows_the_resolved_scope_not_the_cli_flags` | 214 | with a configured `scan.root`, `scan --known` reports `scope == "explicit"` and `scan ROOT` still does. M012A note: the configured-root case moved to `--known` because rootless `scan` is now a usage error |
| `scan_scope_conflicts_are_rejected_before_any_traversal` | 664 | exit `2`, **empty stdout**, clap's `cannot be used with` on stderr, for all three bad pairs |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 696 | `clean <root> --dry-run --format json` → `json["mode"] == "simulate"` and `result.summary.simulated == 0` |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 735 | an invalid `Cargo.toml` gives `status.code() == Some(1)` together with `result.scope_blocked == true` |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` (`#[cfg(unix)]`) | 782 | against a `#!/bin/sh` Cargo stub: `--yes --profile dev` → `reason_code == "cleaned"`, `selector_kind == "profile"`, exactly **one** `clean` logged; `--dryrun` → `outcome == "simulated"`, count still 1; `--min-reclaimable-bytes 1 --yes` with a profile → `selector_unsupported`; `--package missing-package` → `selector_invalid`; `--package fixture` with `CARGO_VERSION=1.97.0` → `selector_unsupported`; `--package fixture@0.1.0` → `cleaned`, count reaching 2 |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 1106 | exit `17` when the editor fails; non-zero exit plus stderr containing `is invalid` on a malformed edit, and the bad file is *not* reverted; success plus `edited` on stdout when valid; a no-op edit leaves `recency_seconds == 42` |
| `working_directory_anchor_selects_without_narrowing_the_ownership_universe` (`#[cfg(unix)]`) | 1478 | the M014 anchor: from an unrelated cwd the unit is discovered, classified `private`, and reported as `skipped_outside_selection`; from inside it, the same unit is `simulated` — ownership is still proven across the complete universe |
| `interactive_without_a_terminal_is_a_usage_error_that_names_the_reason` (`#[cfg(unix)]`) | 1534 | `-i` with no terminal exits `2` **and names the reason** — a test that only checked "it failed" would pass against an unrelated crash |
| `interactive_refuses_the_machine_output_formats` (`#[cfg(unix)]`) | 1571 | `-i` with `--format json` or `--format log` is refused: a terminal session cannot be carried by an automation contract |
| `interactive_dry_run_still_requires_a_terminal` (`#[cfg(unix)]`) | 1623 | `--dry-run -i` still refuses a redirected terminal — the mode changes what a session *does*, not whether one exists |
| `bare_invocation_executes_routine_cleanup_and_is_not_a_scan` (`#[cfg(unix)]`) | 1641 | **the M012A premise-negative regression.** Bare argv with a configured `scan.root`: `operation == "clean"`, `result.units` is an array (not `groups`), `mode == "execute"`, `scope == "explicit"`, `summary.cleaned == 1`, **exactly one** `clean` call logged, and the artifact deleted |
| `bare_dry_run_simulates_with_zero_cargo_clean_processes` (`#[cfg(unix)]`) | 1678 | `--dry-run`: `mode == "simulate"`, `summary.simulated == 1`, `summary.cleaned == 0`, **zero** `clean` calls, artifact intact, and `metadata` called at least once |
| `routine_bare_dry_run_and_execute_use_real_cargo_on_default_seed` (`#[cfg(unix)]`) | 1717 | C034 positive control: the real Cargo executable builds and cleans a disposable crate under an isolated `HOME`, with a shell wrapper recording invocations and the staged plugin proving `cargo cleanme` resolves to the test binary |
| `bare_cleanup_with_no_known_roots_is_a_successful_no_op` (`#[cfg(unix)]`) | 1902 | empty `$HOME`: exit `0`, `operation == "clean"`, `scope == "routine"`, `summary.cleaned == 0`, `units == []` |
| `bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean` (`#[cfg(unix)]`) | 1942 | an unparsable manifest through the bare front door: exit `1`, `scope_blocked == true`, reason contains `ownership could not be proven`, zero `clean` calls |
| `advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit` (`#[cfg(unix)]`) | 2159 | `clean ROOT` → Execute + artifact removed; `--cargo-preview` → `mode == "preview"`, `summary.previewed == 1`, **artifact still present**, Cargo invoked a *second* time, and `--dry-run` appears in the stub's argv; `--dry-run` adds no third Cargo invocation |
| `hidden_compatibility_aliases_map_exactly_to_the_canonical_modes` (`#[cfg(unix)]`) | 2217 | `--yes` → Execute; `--dryrun` and `--dry-run` agree field-for-field; neither prints a `deprecat` string |
| `every_accepted_simulation_spelling_reaches_simulate_and_spawns_no_cargo_clean` (`#[cfg(unix)]`) | 2270 | every accepted simulation spelling reaches `simulate` **and** spawns no `cargo clean`; `--cargo-preview` is included because it is the one accepted combination that means two different things at once |
| `the_cargo_plugin_simulation_spelling_also_spawns_no_cargo_clean` (`#[cfg(unix)]`) | 2364 | the same guarantee through the Cargo plugin entry point, with the staged-binary premise asserted rather than assumed |
| `clean_rejects_simulate_and_cargo_preview_together_at_parse_time` (`#[cfg(unix)]`) | 2402 | `clean --dry-run --cargo-preview` exits `2` with clap's `cannot be used with`, spawns nothing, and leaves the artifact in place |
| `update_dry_run_reaches_the_same_non_mutating_path_from_either_spelling` | 2438 | both `--dry-run update` and `update --dry-run` resolve to a non-mutating updater request, through the production `try_parse_normalized_from` path |
| `known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one` (`#[cfg(unix)]`) | 2464 | `$HOME/projects/<fixture>` → `scope == "routine"` **and** `discovered_manifests == 1` (so the label is not vacuously right); the same fixture with a configured root → `explicit` |
| `json_scan_within_an_empty_root_is_a_successful_zero_result_report` | 2526 | platform-neutral zero-result scan: exit `0`, `groups == []`, `group_count == 0` |
| `json_and_human_output_are_unchanged_by_log_mode` (`#[cfg(unix)]`) | 3035 | M012B §8: adding `--format log` changes neither the JSON nor the human rendering |
| `log_is_an_explicit_value_and_invalid_formats_still_fail_closed` | 3081 | `--format jsonl` exits `2` with empty stdout — an invalid format is not a silently-accepted typo |
| `scan_emits_exactly_one_json_document` | 3116 | the one-document-per-invocation invariant a `clean --full` could break; asserted at the source-guard level, because the suite keeps every case bounded and cannot run a platform walk |
| `json_update_refusal_emits_no_document_and_exits_non_zero` | 3260 | a mutating update over forbidden provenance refuses **before** asking the registry anything: no document on stdout, non-zero exit, hermetic |
| `the_json_update_branch_prints_only_the_tested_seam` | 3311 | the JSON branch of `run_update` prints exactly the library stream and nothing else |
| `the_machine_contract_is_identical_with_and_without_verbose` (`#[cfg(unix)]`) | 4630 | M020: `--verbose` is global and changes the *human* surface only — the `json` and `log` documents are byte-identical with and without it, and so is the exit code |
| `a_bounded_cleanup_names_its_directories_relative_to_the_scope_root` (`#[cfg(unix)]`) | 4660 | M020: the human surface names cleaned directories relative to the scope root, so the absolute root does not leak into it |
| `the_quiet_human_surface_is_a_row_per_cleaned_directory_and_one_tally` (`#[cfg(unix)]`) | 4691 | M020: the default human surface is quiet — one row per cleaned directory plus the tally, and none of the verbose surface's census or per-unit detail |
| `a_duplicate_override_is_a_usage_error` | 4735 | M019: the same policy override at the root and on `clean` exits `2` with a diagnostic naming the flag — the discarded-flag failure mode ADR 008 §3 exists to prevent |
| `a_scope_selector_on_a_read_only_subcommand_is_a_usage_error` | 4785 | M019: `--full` or a ROOT positional handed to `scan`/`config`/`update` exits `2` and names the subcommand that cannot accept it |
| `a_policy_override_on_a_read_only_subcommand_is_a_usage_error` | 4826 | M019: a policy override handed to `scan`/`config`/`update` exits `2` and names the flag |

### 7.3 What is not covered

- **No test for the `--` separator**, in `cli.rs` or `cli_contract.rs`. The
  behaviour asserted in §6.4 rests on the absence of
  `trailing_var_arg`/`allow_hyphen_values`, not on a test.
- **No test for invalid numeric values**: a non-numeric, negative, or
  overflowing `--min-reclaimable-bytes` or `--older-than`. An invalid
  `--format` value *is* covered now, at both levels (`src/cli.rs:1822` for
  `yaml`, `tests/cli_contract.rs:3081` for `jsonl`).
  `NonEmptyStringValueParser` is exercised for the `--profile`/`--package`
  *conflict*, not for an actually-empty value.
- **No test for the real `Routine` scope with learned roots.** The
  maintenance-scope tests pin the *variant*, not the root list: with a bare
  fixture and a `$HOME` with no seed directories, the Routine scope is empty and
  the root set is not asserted anywhere. A learned root surviving retention is
  covered by `discovery_state`'s own tests, not through the front door.
- **No hosted integration case runs a Full scan.** `scan --all` walks the whole
  platform; the suite keeps every case bounded on purpose. The invariant it
  would prove is instead asserted where it is decided:
  `full_resolution_ignores_a_configured_root` (`src/policy.rs:961`) plus
  `scan_scope_selection_is_full_explicit_or_maintenance` (`src/cli.rs:1401`).
  That pair is an argument, not an observation — a reader should weigh it as
  such. (`scan_emits_exactly_one_json_document`,
  `tests/cli_contract.rs:3116`, records the same limitation for the
  `clean --full` one-document invariant.)
- **No test for `absolutize_root` under symlinks or a non-existent relative
  root**, and none for the absolutize-vs-`collapse_roots` disagreement in §4.4.
  `absolutize_root_for_test` has no call sites at all.
- **No negative test for a path-shaped flag value**, e.g.
  `cleanme --config --stats scan`, where `--stats` would be consumed as the
  config path.
- **No test for `config path` or `config show` output** — two of the three
  `ConfigCommand` arms are untested at the integration level. The `update`
  arm is covered for its *resolved request* and its refusal path, but no
  hosted case asserts the update plan's rendered output.

---

## 8. Review checklist

1. **Did you regenerate the generated docs?** Any flag add, rename, or
   doc-comment edit changes `man/*.1` and `completions/*`, and
   `.github/workflows/ci.yml:96-108` fails the build without it. The comparison
   model comes from `cargo_cleanme::cli::Cli::command()` at
   `xtask/src/main.rs:96`.
2. **Does the flag work in both invocation forms?** Add it to
   `direct_and_external_forms_parse_equivalently` (`src/cli.rs:1993`), or use
   the production `try_parse_normalized_from` path as `src/cli.rs:373` does. A
   `Vec` or `PathBuf` value is where an order-sensitive mismatch appears.
3. **Is `normalize_cargo_argv` still correct after your change?** Its whole
   safety argument is that no valid command can have `cleanme` at argv[1]
   (`src/cli.rs:743-753`). If a new subcommand or positional could produce that
   shape, the shim must change with it;
   `later_literal_cleanme_values_are_preserved` (`src/cli.rs:1961`) encodes the
   current position rule.
4. **Did you handle the field in `Cli::invocation`?** The `Clean` arm
   (`src/cli.rs:428-460`) destructures every field explicitly, so a new field is
   a compile error — verify you added the *semantics* at `main.rs:355-380`, not
   just the binding. Decide whether the new flag **replaces** or **merges**
   with the config value: `--include`/`--exclude` replace (`main.rs:360-369`)
   while `--min-reclaimable-bytes`/`--older-than` fall back
   (`main.rs:356-359`). A field added to `Command::Clean` must also appear in
   `CleanupOverrides` (`src/cli.rs:284-318`) or it will not reach the engine.
5. **Are you adding a mode or a spelling?** A *new spelling* belongs in
   `clean_mode` (`src/cli.rs:352-444`) and `merged_clean_mode`
   (`src/cli.rs:631-641`) with a hidden flag and a test proving it maps to an
   existing mode. A *new mode* is a different thing: it needs a
   `CleanMode` variant, a reason about what the cleanup engine may do, and a
   decision about whether the bare front door may reach it. Never let a new
   flag create a mode the front door cannot express — and never let it create a
   mode the front door *can* reach by accident.
6. **Does bare invocation still mean the same thing as `clean --known`?** They
   share the `CleanupScope::Maintenance` variant (`src/cli.rs:475-493`,
   `src/cli.rs:535-539`) **and** the same `CleanupSelection::WorkingDirectory`
   anchor (`src/cli.rs:618-623`). Any change that gives one of them a
   different root set, policy, or gate is a safety regression, not a
   convenience.
7. **Does the error path match your expectation?** Parse failures never reach
   `AppError`; clap prints and exits (`src/cli.rs:369` → `main.rs:11-37`).
   Anything that must read `cargo-cleanme: …` with exit 2 has to be raised from
   `main.rs`, not from a `value_parser` or a `conflicts_with`.
8. **Have you checked the root path arithmetic?** If you touch
   `absolutize_relative` (`src/cli.rs:796-1092`), re-assert the three
   properties: output is always absolute even for a relative `base`; `..`
   saturates at the root (`let _ = out.pop()`); symlinks are never followed.
   And remember `absolutize_root` leaves an already-absolute input entirely
   alone (`src/cli.rs:774-776`), `..` components included.
9. **Does your change alter the machine contract?** If it changes what
   `output::scan`/`output::cleanup`/`update_json` emit, it needs a
   `schema_version` decision in `output.rs`; the single-newline,
   one-document assertions at `tests/cli_contract.rs:696` and `:785` are the
   guard. A *value* correction inside v1 (like cleanup `scope == "routine"`
   replacing `"known"`) is a documentation change in
   `plans/output-schema-v1.md`, not a version bump — a *meaning* change is.
10. **Is there a test that asserts your new behaviour, or only one that
    exercises it?** A flag added without a case in `cli.rs` or
    `cli_contract.rs` is covered by nothing — §7.3 is the precedent for what
    silently rots. And when you add one, check what input makes it fail: a test
    that cannot fail for the right reason is decoration.
11. **Does the help text say anything the parser does not do?** The root
    `long_about` at `src/cli.rs:21-38` was the standing counter-example: it
    described a rootless `scan` that the `ArgGroup` at `src/cli.rs:139` rejects.
    C040 rewrote it *and* made the manpage render from `long_about`, so the
    two surfaces can no longer drift — but the question stands for the next
    sentence someone adds. A user-facing sentence about scope selection must
    state the condition under which it is true.
12. **Did you check the root/alias merge?** M019 gives the root and the hidden
    `clean` alias their own copies of every cleanup flag. A new cleanup flag
    needs a root declaration (`src/cli.rs:89-124`), a hidden alias copy
    (`src/cli.rs:173-215`), a slot in `CleanupOverrides`
    (`src/cli.rs:284-291`), a merge rule in `merge_overrides`
    (`src/cli.rs:683-707`), and — if it can be given at both levels — a
    `duplicate_override` rejection. A flag that is declared at one level and
    silently dropped at the other is the discarded-flag failure mode ADR 008 §3
    exists to prevent; `the_same_override_at_both_levels_is_a_usage_error`
    (`src/cli.rs:1123`) and
    `a_cleanup_flag_on_a_read_only_subcommand_is_a_usage_error`
    (`src/cli.rs:1097`) pin the two rejections.
