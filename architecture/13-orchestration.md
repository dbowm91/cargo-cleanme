# Orchestration — argument dispatch and the two pipelines

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** two findings in §7 and §8 have been **fixed** — the JSON `scope`
> label is now derived from the resolved `policy.scope` (with a regression test),
> and a failed discovery-state save no longer changes the exit code. Note that
> §8's claim that `tests/end_to_end.rs:202` "encodes the bug as expected" is
> **incorrect**: that test supplies its own scope label to exercise the DTO
> projection and never reaches this derivation. See [overview §7.2](overview.md).

`src/main.rs`, 606 lines, zero inline tests. The only module that reaches every
other one, and the only place the two pipelines are stitched together.

## 1. Responsibility

`main.rs` owns argument dispatch (`:21`), collaborator wiring, the exit-code
contract (`:6-14`), and the reconciliation branch deciding whether persisted
state is rewritten (`:373-454`). It owns **no domain logic**: scope to
`policy::resolve` (`:317`), classification to `workspace::analyze_groups`
(`:469`), authorization and mode semantics to
`cleanup::clean_with_roots_policy_selector` (`:200`), and even "is this
incomplete?" to merely *reading* a `PlatformRoot` + `Error` diagnostic
(`:399-402`, `:480-484`).

**It declares no types.** Verified: `grep -nE '^\s*(pub )?(struct|enum|trait|impl|type) ' src/main.rs`
returns nothing — no `struct`, `enum`, `impl`, or alias. It is a pure
composition root. **It has no inline tests**: `grep -cE '#\[cfg\(test\)\]|#\[test\]' src/main.rs`
→ `0` (see §10).

Its top `use` block is five lines (`:1-5`): `cli::{self, Cli, Command,
ConfigCommand, OutputFormat}`, `config::{self, ConfigPathResolver}`,
`error::AppError`. Two function-scoped imports live inside `run_scan` —
`use cargo_cleanme::{domain, progress};` (`:311`) and
`use cargo_cleanme::discovery_state::StateLoad;` (`:373`). Everything else is
named at the point of use (`cargo_cleanme::cleanup::CleanMode` `:103`,
`cargo_cleanme::update::update` `:23`, `cargo_cleanme::workspace::SystemCargoRunner`
`:363`, `serde_json::…` `:182`). Consequence: renaming a library function
breaks this file at many scattered sites, and there is no single list of what it
depends on. Modules reached: `cli`, `config`, `error`, `domain`, `discovery`,
`workspace`, `discovery_state`, `cleanup`, `update`, `editor`, `progress`,
`report`, `output`, plus `serde_json` and `directories` (`:410`).

Five free functions, no types: `main` (`:6`), `run` (`:15`), `collapse_roots`
(`:269`), `run_scan` (`:302`), `update_json` (`:584`).

## 2. Entry and exit

```rust
fn main() {                                   // :6-14
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => { eprintln!("cargo-cleanme: {e}"); std::process::exit(2) }
    }
}
```

`Err(e)` prints `cargo-cleanme: {e}` on **stderr** and exits **2**; every
`AppError` variant (`src/error.rs:3-16`) exits 2. A command can also return
`Ok(1)`. **Exit 1 means "ran, and something failed"; exit 2 means "could not
run."** The only `Ok(1)` producers: `:64` (editor passthrough), `:118` (scan
preflight failed), `:257-261` (cleanup partial failure), `:569-578` (scan
Full-state failure).

| Code | Meaning | Produced by |
|---:|---|---|
| 0 | Success | `:579`; Update `:37`; Config `:83`; empty-roots cleanup `:194`; cleanup `:257-261` when `failed == 0 && scope_blocked.is_none()` |
| 1 | Ran, partially failed | editor's own code `:64`; `clean --full` preflight `:117-119`; cleanup `:257-261`; scan `:592-594` |
| 2 | Could not run | any `AppError` via `main` `:11`; clap usage errors (clap's own path) |
| 0 (clap) | `--help` / `--version` | clap, not this file |

| Command | Codes | Decided at |
|---|---|---|
| bare `cargo-cleanme` / `scan [ROOT] [--full]` | 0, 1, 2 | `:266` / `:264` |
| `clean ROOT …` / `--known` | 0, 1, 2 | `:257-261` |
| `clean --full …` | 0, 1, 2 | `:116-119` then `:257-261` |
| `clean` with no root source | 2 | `:149-153` |
| `config path` / `show` | 0, 2 | `:41`, `:43`, `:83` |
| `config edit` | 0, **editor's code**, 2 | `:64`, `:69-78` |
| `update [--dry-run]` | 0, 2 | `:37`, `:23` |
| any (clap usage error) | 0 (help/version), 2 | clap |

`update` can never return 1: `From<UpdateError> for AppError`
(`src/update.rs:355-363`) maps everything to `AppError::Provenance` or
`AppError::Update`, both exit 2 — **including the benign
`UpdateError::AlreadyCurrent` (`src/update.rs:792`)**. So "already up to date"
exits **2**, while `docs/TROUBLESHOOTING.md:98-102` calls that state success.

### Clap's errors bypass `AppError` entirely

`run` calls `Cli::parse_normalized()` (`:16`) = `Self::parse_from(…)`
(`src/cli.rs:115-117`), and unlike `try_parse_from` it **cannot return `Err`**:
on a parse error clap prints its message plus usage and calls its own
`process::exit`. So for a bad flag, unknown subcommand, or violated
`conflicts_with`, the `AppError` branch at `:9-12` is never reached, no
`cargo-cleanme: ` prefix is printed, and the exit code is **2** (clap 4.5's
`Error::exit`; `Cargo.toml:51` pins `clap = "4.5"`). `--help`/`--version` exit
**0**.

**Definitive:** `cargo cleanme clean /x --yes --dry-run` shows a clap usage
error and exit 2 — indistinguishable by status from a fatal `AppError`. A
script must match the message prefix to separate "you mistyped" from "the
filesystem is broken".

## 3. Command dispatch

`match cli.command` (`:21`): `Update` (`:22`), `Config` (`:39`), `Clean`
(`:85`), `Scan` (`:263`), `None` (`:266`). Global flags are lifted out *before*
the match (`:18-20`) and `config` is consumed at `:17`. Ordering consequence:
**the config path is resolved for every command, including `update`**, so if
`ProjectDirs::from` returns `None` (`src/config.rs:82-84`) then even `update`
and `config path` fail with exit 2 before reaching their arms.

**`Update`** (`:22-38`) — `update::update(dry_run)?` then JSON (`:24-25`),
human dry-run four-liner (`:26-33`), or one-line success (`:35`); `Ok(0)` at
`:37`. `--format` **is** honoured (`:24`).

**`Config`** (`:39-84`), three arms. **`Path`** (`:41`) prints the path and
never loads, creates, or validates the file, so it works when the file is
absent or corrupt — it needs only the *path* to be derivable. **`Show`**
(`:42-44`) is `config::show(&config::load_or_create(&path)?)?` and therefore
**creates** the file if missing (`src/config.rs:217-219`) — a write side
effect. **`Edit`** (`:45-81`) does `config::ensure_exists` (create-if-absent
*without* requiring current contents to parse, `src/config.rs:223-229`),
resolves `$VISUAL`/`$EDITOR` (`src/editor.rs:7`), spawns, then three checks.

**`--format` is not honoured for any `Config` arm** — all three print
unconditionally (`:41`, `:43`, `:80`); `format` is never consulted. Verified,
not inferred: `--format json config show` emits TOML.

**The three post-edit checks.** (1) *Non-zero editor exit* (`:59-65`) prints
`cargo-cleanme: editor exited with {status}; config was not reverted: {path}` to
stderr and returns `Ok(status.code().unwrap_or(1))` — the editor's code is
propagated verbatim, and a signal-killed editor (`code()` is `None`) collapses
to 1; pinned by `tests/cli_contract.rs:629-630` (`Some(17)`). (2) *File
deleted* (`:66-73`): `load` treats an absent file as "all defaults"
(`src/config.rs:94-96`), so a deleted config would silently reset every policy
→ `Err(AppError::Config(…))` → exit 2. (3) *File modified into an invalid
state* (`:74-79`): `config::load` re-wrapped as
`AppError::Config("edited config {path} is invalid: {e}")`; pinned by
`tests/cli_contract.rs:631-634`, which also asserts the bad content is left in
place (the message says "was not reverted"). Checks 2 and 3 exit 2; check 1
exits with the editor's code. `Ok(0)` at `:83` is reached only for `Path`,
`Show`, and a successful `Edit`.

**The `None` arm** (`:266`) — `run_scan(None, false, …, true)`. **A bare
`cargo-cleanme` is a Routine scan**, delegating to exactly the same function as
`scan` with no root and no `--full`. This is the most important default in the
file: the common case is "just show me what is stale", which needs no
arguments. `src/cli.rs:206-209` pins that the *parse* yields `command: None`;
the mapping to a Routine scan lives only here and is untested.

## 4. The scan pipeline

`run_scan` — `src/main.rs:302-580`, seven parameters, of which `emit_output` is
the subtle one. In order:

1. `root.is_some()` → `explicit_scope` (`:314`); `scan_start` (`:314`, the
   recency reference) and `wall_start` (`:315`, behind `--stats`).
2. `config::load_or_create(config_path)?` (`:316`) — may create the file.
3. `policy::resolve(domain::ScanRequest { cli_root: root, full }, &c.scan)?`
   (`:317-323`); `recency` bound at `:324`.
4. `IndicatifRenderer::new(!show)` (`:328-329`) and
   `observer.phase(Discovery)` (`:333`), where `show` = human format **and**
   `should_show_progress(no_progress)`.
5. `discovery::discover_manifests_with_attribution(&policy, observer, stats)?`
   (`:337-338`); `discovery_nanos += elapsed` saturating (`:341-343`);
   `_resolution_elapsed` is later measured at `:362`/`:371` and **discarded**.
6. `uncertainty` extracted by filtering `PermissionDenied | Metadata |
   Vanished` (`:344-356`); `manifests_found` set (`:359`).
7. `observer.phase(Resolution)` (`:361`) and
   `workspace::resolve_workspaces(…)` with `SystemCargoRunner` (`:363-370`),
   mutating `counters` and `diagnostics`.
8. State reconciliation (`:373-454`) — §7.
9. `build_groups(&workspaces)` (`:461`), `phase(Analysis)` (`:462`),
   `units_total(Analysis, groups.len())` (`:463`).
10. `clock_cutoff = scan_start.checked_sub(recency)` (`:465-467`).
11. `analyze_groups(&workspaces, groups, scan_start, clock_cutoff, recency, …)`
    (`:469-478`), mutating `counters` and `diagnostics`.
12. `full_incomplete` (`:480-484`); `domain::ScanReport` built (`:486-504`),
    carrying `visited_entries` through.
13. `phase(Reporting)` (`:506`), `finish_and_clear()` (`:507`), **conditional**
    output (`:510-523`), then diagnostics to stderr (`:524-547`) and `--stats` to
    stderr (`:551-568`).

**Step 6, in detail.** `uncertainty` (`:344-356`) filters
`PermissionDenied | Metadata | Vanished`, then `.filter_map(|d| d.path.clone())`.
It reads `discovered.diagnostics`, the **pre-resolution** vector — *not* the
`diagnostics` vec `resolve_workspaces` goes on to mutate at `:368`, so a
permission/metadata diagnostic raised during `cargo metadata` resolution cannot
reach `uncertainty` and cannot protect a learned root. And `filter_map`
silently drops matching diagnostics carrying no path, so `uncertainty` is
path-addressed only.

**Step 10.** `checked_sub` returns `None` if the clock is earlier than
`1970-01-01 + recency_seconds`, killing the scan with exit 2 and a
*config-flavoured* message. `config::load` already range-checks
`recency_seconds` (`src/config.rs:97-104`), so only a mis-set clock causes this.
The same condition is handled non-fatally in `now_seconds()`
(`src/discovery_state.rs:264-269`, `unwrap_or_default()` → 0) — one root cause,
two policies. `tests/end_to_end.rs:163` uses the *unchecked* subtraction, so
the production guard is untested.

### `emit_output`

`emit_output: bool` is `main.rs:309`. It guards exactly one thing — the report
emission block at `:510-523`, containing both the JSON (`:511-519`) and human
(`:520-522`) render. It does **not** suppress diagnostics (`:524`), `--stats`
(`:551`), or state reconciliation. Two call sites: `Scan` passes `true`
(`:264`) and the `None` arm `true` (`:266`); `clean --full` passes `false`
(`:116`).

**Why:** `clean --full` runs a complete Full scan purely to refresh
`discovery-state.json` before reading `learned_roots` back at `:120-131`. The
scan is a side effect of choosing cleanup roots, not something the user asked to
see. Printing a report first would put an unrelated document ahead of the
cleanup report on stdout, and in JSON mode would emit **two** JSON documents —
destroying the one-document-per-invocation invariant that
`tests/cli_contract.rs:204` and `:242` assert (exactly one `\n`).

What `clean --full` still emits with `emit_output == false`: diagnostics on
stderr (`:524-547`) and `--stats` on stderr (`:551-568`). Reasonable — both
stderr, and `--stats` is opt-in.

## 5. The cleanup pipeline

The `Clean` arm, `src/main.rs:85-262`.

**Mode selection** (`:102-110`): `yes` → `Execute`, `dryrun` → `Simulate`, else
`Preview`, with `let _ = dry_run;` (`:108`) an explicit discard. The comment at
`:100-101` names the split: "`--dry-run` (Cargo preview, default) vs `--dryrun`
(simulation)". **Deliberate but hazardous:** a user typing `--dry-run`
expecting a *simulation* — the project's own full ownership-proof pass with no
Cargo invocation (`src/cleanup.rs:36-38`) — instead gets `CleanMode::Preview`,
which delegates to Cargo's own `--dry-run --verbose` and never runs the same
decision path. The flags differ by one hyphen and produce materially different
evidence. `src/cli.rs:73-80` documents the distinction and clap enforces
pairwise conflict (`:75-83`), but nothing at runtime can detect the mix-up
because the flag has no effect.

**Root selection** (`:113-153`), mutually exclusive by clap
(`src/cli.rs:50-54`): `clean ROOT` → `vec![cli::absolutize_root(&root)]`
(`:113-114`); `clean --full` → run `run_scan(None, true, …, false)`, re-read
`discovery_state::load_default()`, take `learned_roots[*].path` (`:115-131`);
`clean --known` → `policy::resolve(ScanRequest{None, false}, &config.scan)`
keeping only `ScanScope::Routine(roots)`, else `Vec::new()` (`:132-148`); none of
the above → `Err(AppError::Config("clean requires ROOT, --known, or --full"))`
→ 2 (`:149-153`). Two things to notice: `clean --full` loads state **twice** —
indirectly via the scan at `:116`, then again at `:120` — so a scan that failed
to publish new roots silently cleans the *previous* generation; and
`state_generation` is `state.last_full_at` (`:125`, `:145`).

**Policy** (`:155-170`): `min_reclaimable_bytes` and `min_inactive_seconds`
override cleanly (`Option`-on-scalar, so "unset" is distinguishable from "set
to 0"). `include`/`exclude` are different — `Vec<String>`, and clap cannot
distinguish "no `--include`" from "zero values", so **an empty CLI list falls
back to the configured list**. Consequence: **`--include`/`--exclude` cannot
clear a configured list.** A user with `[cleanup.policy] exclude = ["**/target"]`
who wants a one-off unfiltered run has no flag for it. Defensible — the empty
list almost always means "I did not mention it", and treating it as "clear"
would be worse in the common case — but undocumented in `--help` and silently
surprising when it bites. A `clear-include` flag pair would fix it without
changing the default. Deliberate choice, not a defect.

**Selector** (`:171-173`): `profile.map(…).or_else(|| package.map(…))` looks
like silent-precedence, but **it is unreachable**. clap already enforces mutual
exclusion: `src/cli.rs:68` gives `profile` `conflicts_with = "package"` and
`:71` gives `package` `conflicts_with = "profile"`; `src/cli.rs:294-305` asserts
the combined form is `Err`. The `.or_else` is defensive dead code, and a user
supplying both gets a clap usage error and exit 2 — correct, by a different
mechanism than the expression suggests.

**Empty-roots short circuit** (`:174-195`). If `collapse_roots` returns nothing,
the pipeline stops before any Cargo invocation. JSON (`:175-190`) builds a
`CleanReport` with `..Default::default()` overriding only `mode`,
`effective_policy`, `selector`, labelled `if full {"full"} else {"known"}`
(`:186`) — the missing `"explicit"` case is safe because an explicit `ROOT`
always yields exactly one root. Human (`:191-193`) prints one fixed line. **Both
return `Ok(0)` (`:194`)** — "nothing to do" is success.

**Reporting** (`:196-242`): `show` requires human format **and** a capable
terminal (`:196-197`); `wall_start` at `:199`; the transaction call is
`:200-209`; `renderer.finish_and_clear()` at `:210` clears transient UI
**before** the report, matching the scan ordering.

**Exit code** (`:257-261`): `1` if `report.failed > 0 || report.scope_blocked.is_some()`.
`failed > 0` means at least one `CleanOutcome::Failed`, counted in
`CleanReport::render`'s tally (`src/cleanup.rs:286-288`) — "ran; at least one
workspace could not be cleaned". `scope_blocked.is_some()` means the scan
boundary itself was refused (`src/cleanup.rs:184`) and no work was attempted;
pinned by `tests/cli_contract.rs:246-270` (`Some(1)`, `scope_blocked == true`).

**`--stats`** (`:243-256`) goes to **stderr** via `eprintln!`, consistent with
`src/cli.rs:23-25`, and prints four counter strings plus mode and elapsed:
`stats_line()` (`:250`), `timings_line()` (`:251`), `proof_stats_line()` (`:252`),
`proof_timings_line()` (`:253`). The two `proof_*` lines are the point of the
comment at `:245-247` — "C003 §7.6: cleanup `--stats` must account for the
final ownership-universe proof work, not only the initial scan". The scan's
`--stats` (`:553-558`) has no proof counters, correctly: a scan performs no
proof. So the cleanup line is a superset and the C003 requirement is satisfied
at this call site rather than inside `cleanup.rs`.

## 6. Root resolution and collapse

```rust
// collapse_roots — src/main.rs:269-301
let mut roots: Vec<_> = roots.into_iter()
    .map(|p| std::fs::canonicalize(&p).unwrap_or(p)).collect();
roots.sort();
roots.dedup();
let mut collapsed: Vec<std::path::PathBuf> = Vec::new();
for path in roots {
    if !collapsed.iter().any(|parent| path.starts_with(parent)) { collapsed.push(path); }
}
collapsed
```

**Canonicalization is best-effort.** `unwrap_or(p)` means a non-existent or
unresolvable path passes through **unchanged and unvalidated**. The absolute-root
invariant the cleanup boundary depends on (`src/cli.rs:146-159`) is established
*earlier*, by `absolutize_root`, and only for the explicit-`ROOT` source. For
the other two sources the roots were canonicalized when *learned*
(`src/discovery_state.rs:153`), so in practice they resolve; a root deleted
between learning and cleaning falls through unchanged and is handed to the
cleanup layer as-is.

**The sort is load-bearing, not cosmetic.** `PathBuf` ordering is component-wise
and a parent always sorts before its children (`/a/b` < `/a/b/c`). The
containment filter at `:278` only works because of that: a child is tested only
after its parent has been visited and accepted. Remove the `sort()` and the same
input in reverse order keeps *both* `/a` and `/a/b` — collapse silently stops
collapsing. `reconcile_full` argues the same case about its own sort
(`src/discovery_state.rs:189-192`), so the codebase is internally consistent.
`Path::starts_with` is component-wise, not string-prefix (`/a/bc` does not
`starts_with` `/a/b`), which makes this correct rather than merely plausible.

**Safety property:** after collapse, no accepted root contains another, so
overlapping roots cannot produce overlapping deletion candidates — a single
physical `target/` cannot be reachable through two surviving roots, so it cannot
be counted twice, cleaned twice, or have its ownership universe computed against
two boundaries.

**Do the three sources mix badly?** In practice no: `absolutize_root` yields a
lexically absolute path, learned roots were canonicalized on write, Routine
roots were canonicalized by `policy::canonical_dedup_roots` (`src/policy.rs:124`,
`:134`). The residual case is a learned root that has since become a symlink or
whose canonicalization now fails — it passes through `unwrap_or` and may sit in
an uncollapsed overlap with a canonically-spelled sibling. Narrow,
low-likelihood; the deeper authorization in `cleanup.rs` is the real boundary.

## 7. State reconciliation

`src/main.rs:373-454` — the only genuinely intricate block, and where a
reviewer should spend the most time.

```rust
let loaded_state = discovery_state::load_default();        // :374
if full { … } else if let StateLoad::Loaded(prior) = loaded_state { … }  // :375, :438
```

There is no `state_reconciled` flag. It was removed when the failed-publish
exit-code escalation was fixed; see "A failed publish no longer changes the
exit code" below. Nothing in this block can now influence the process exit
status — the stderr notice is the whole of the consequence.

**Prior selection.** `full_reconciliation_prior` returns
`Option<(DiscoveryState, bool)>` (`src/discovery_state.rs:54-60`): `Loaded(s)` →
`Some((s.clone(), false))`; `Missing` → `Some((default, false))`;
`RecoverableInvalid(_)` → `Some((default, true))` where the `bool` is
`replacing_invalid`; `UnsupportedNewer` and `Unavailable` → `None`. The `None`
case is the important safety choice: **state written by a newer binary, or
simply unreadable, is never overwritten.** The `else` at `:432-437` prints
`cargo-cleanme: {message}; Full state reconciliation was skipped`, where
`message` is `loaded_state.diagnostic()` (`src/discovery_state.rs:33-49`) with a
generic fallback.

**The join** (`:379-388`) maps canonicalized member manifest → workspace root;
**observations** (`:389-398`) apply the same canonicalize-or-raw rule to each
discovered manifest, so the join succeeds whenever both sides are
canonicalizable. `cargo metadata` reports absolute real paths and discovery
manifests come from the walk, so both canonicalize to the same bytes in
practice. When they do not, the failure is a **safe degradation, not a wrong
answer**: `workspace: None` → `ResolutionStatus::ManifestObservedUnresolved` and
the learned-root candidate falls back to `manifest.parent()`
(`src/discovery_state.rs:129-147`) — a *broader* root, not a narrower one.

**`complete`** (`:399-402`) is true unless some diagnostic is `PlatformRoot` +
`Error`, and it gates everything: `reconcile_full` returns
`NoPublication("Full traversal was incomplete")` before touching anything
(`src/discovery_state.rs:122-124`). That is the counterpart of `full_incomplete`
at `:480-484`, which recomputes the *same predicate* for the exit code. Two
identical expressions in one file; they could share a helper.

**Publish** (`:414-431`): on `Reconciliation::Publish(next)`, `publish(&next)`
succeeding may print a "replaced unusable discovery state" notice if
`replacing_invalid` (`:417-421`); failing prints
`cargo-cleanme: discovery state was not saved: {error}` (`:424`) and **changes
nothing else**. `NoPublication(reason)` prints `… was not reconciled: {reason}`
(`:429`).

### A failed publish no longer changes the exit code

`src/main.rs:576-579` is the whole exit decision:

```rust
if full_incomplete {
    return Ok(1);
}
Ok(0)
```

**This was a real defect, and it is fixed.** The block previously read
`if full_incomplete || (full && !state_reconciled) { return Ok(1); }`, so a
failed state publish returned `Ok(1)`: a good scan report on stdout, one stderr
line explaining the state was not saved, and a non-zero exit. That contradicted
`src/discovery_state.rs:338`, directly above the publish implementation, which
states the module's own position: *"State is an optimization only: concurrent
publishers use atomic last-writer-wins replacement."* A read-only or full state
directory — plausible on a locked-down machine, in a container with a read-only
home, or under `chattr +i` — turned a **purely informational scan** into a
non-zero exit for a reason unrelated to what the scan found, and scripts gating
on status reported a scan failure that did not happen.

The `state_reconciled` flag is gone, so the two branches no longer disagree:
the Routine branch's byte-identical "state was not saved" warning (`:452`) and
the Full branch's (`:424`) now both leave the exit code alone. A genuinely
incomplete Full scan still exits `1`, via `full_incomplete`. Recorded in
[overview §7 item 8](overview.md).

### The Routine branch (`:438-453`)

Matches on `StateLoad::Loaded` specifically, so a Routine scan *silently*
ignores `Missing`, `RecoverableInvalid`, `UnsupportedNewer`, `Unavailable` — no
diagnostic at all, unlike the Full branch's `else`. It computes
`observed: Vec<_> = workspaces.iter().map(|w| w.root.clone())`, then for each
project bumps `last_project_seen_at` on every learned root for which
`project.starts_with(&root.path)`, and finally publishes only if
`!observed.is_empty()`, printing `discovery state was not saved: {error}` to
stderr on failure.

**The asymmetry.** This branch only ever *touches* `last_project_seen_at`. It
does not add or remove learned roots, does not update `projects`, does not touch
`last_full_at`. Learning and forgetting happen **only** in Full
(`src/discovery_state.rs:145-216`). That is the correct split — a Routine scan
is a cheap probe over a bounded scope and must not rewrite what it did not
observe — and the `!observed.is_empty()` guard prevents a probe that resolved
nothing from republishing unchanged state.

**`starts_with` is un-canonicalized.** `project` comes from `cargo metadata`;
`root.path` is canonicalized from when it was learned
(`src/discovery_state.rs:153`). A learned root spelled canonically and a
workspace root reached through a symlink fail to match — a **missed touch**, so
`last_project_seen_at` goes stale and the root eventually ages out of
`policy::routine_roots` (`src/policy.rs:112-118`). Self-limiting: the root is
dropped from Routine scope, not deleted, and a later Full scan re-learns it.
`main.rs` canonicalizes deliberately in the Full branch (`:367`, `:376`) and not
here. Small, real, one-sided in severity.

**No duplicate-publication exposure.** `manifests` is sorted and deduped by
discovery before return (`src/discovery.rs:662-665`), so `observations` is 1:1
with distinct manifests and cannot duplicate `ProjectRecord`s; and
`reconcile_full` separately collapses nested learned roots by nearest ancestor
(`src/discovery_state.rs:189-216`). `main.rs` needs no defence against either,
and has none.

## 8. Output selection

`OutputFormat` (`src/cli.rs:4-9`) is `Human` (default) or `Json`, and is a
**global** flag (`src/cli.rs:29`), so it may appear before or after the
subcommand.

| Path | Human | JSON |
|---|---|---|
| scan | `report::render(&mut report)` `:521` | `to_string(output::scan(&report, scope))` `:512-518` |
| cleanup | `println!("combined roots …\n{}", report.render())` `:233-241` | `to_string(output::cleanup(&report, scope, gen))` `:217-230` |
| cleanup, empty roots | fixed message `:192` | default `CleanReport` `:176-190` |
| update | three literal shapes `:26-36` | `update_json(&plan, dry_run)` `:584-606` |
| config | `println!` only | **not honoured** |

### The `scope` label — a real contract bug

`src/main.rs:516-522` picks `if full {"full"} else if explicit_scope {"explicit"} else {"routine"}`,
and `explicit_scope` is defined once, at `:314`, as `root.is_some()`.

**Confirmed: the scan `scope` label is derived from CLI flags, not from the
resolved policy.** It never consults `policy.scope`, even though
`policy::resolve` returned it at `:317` and `policy` is still in scope.
Reachable divergence:

| Invocation | `policy::resolve` yields | Label | Correct? |
|---|---|---|---|
| `scan /x` | `Explicit("/x")` (`src/policy.rs:42`) | `"explicit"` | yes |
| `scan` with `scan.root = "/x"` in config | `Explicit("/x")` (`src/policy.rs:30-42`) | `"routine"` | **no** |
| bare, `scan.root` set | `Explicit("/x")` | `"routine"` | **no** |
| `scan --full` | `Global(…)` | `"full"` | yes |
| bare, no config root | `Routine(…)` | `"routine"` | yes |

Mechanism: `policy::resolve` falls back to `config.root` via
`request.cli_root.or_else(|| config.root.clone())` (`src/policy.rs:30`), and
`config::load` even validates that root is absolute (`src/config.rs:110-112`) — a
fully supported, first-class configuration. But `root.is_some()` at `:314` knows
only about the *CLI* positional. `scope` is a documented field of the
`EnvelopeV1` machine contract (`tests/cli_contract.rs:202` asserts it), so a
consumer routing on `"routine"` is told something false — and that test pins only
the CLI-root case, which is the one that is correct. The fix is one line: match
on `policy.scope`.

Contrast the **cleanup** label (`:221-227`): `if full {"full"} else if known
{"known"} else {"explicit"}` — derived from the actual root *source* selected at
`:113-153`, so correct by construction. The two labels in one file are derived
from two different sources; only one reflects reality.

### `state generation last_full_at=` goes to stdout

`src/main.rs:211-215` uses `println!` — **stdout**, not stderr — guarded to human
format, so it cannot corrupt JSON. But it is machine-adjacent metadata
interleaved into a human report, on exactly the stream that pipelines capture.
The project's stated discipline is stdout = deterministic report, stderr =
transient/diagnostic (`src/main.rs:326-327`, `:548-550`). This line is neither.
It belongs on stderr beside `--stats`, or in the JSON envelope as a field —
`output::cleanup` already accepts a `state_generation` argument
(`src/output.rs:161`).

### Serialization failure

Two sites, inconsistent: `:518` and `:230` map to
`AppError::Config(format!("cannot serialize JSON report: {e}"))`, but `:189` (the
empty-roots cleanup branch) uses `AppError::Config(e.to_string())` with **no**
descriptive prefix. `AppError::Config` is also the wrong *variant* — the user
sees `cargo-cleanme: configuration error: cannot serialize JSON report: …`,
sending them to `config.toml` when the fault is in report serialization (it
does at least match the existing habit: `config::show` does the same at
`src/config.rs:304-306`). In practice `serde_json::to_string` on these DTOs
cannot fail — `EnvelopeV1` is owned primitives, strings and `Option`s, with no
map keys and no `f64` NaN/Infinity — so this is a defensive branch that is
effectively dead. The real risk is that the *human* path has no equivalent guard.

### `update_json` — the unversioned, untested surface

`update_json` (`:584-606`) hand-builds a `serde_json::Map` with
`schema_version: 1` and `operation: "update"`. The discipline is deliberate (the
doc comment at `:582-583` says so) but is not shared with `output.rs`'s
`EnvelopeV1` — it is a second, hand-maintained envelope. With the §10 finding
that this file has **zero** inline tests, `update_json` is the only
machine-readable output surface with **no test coverage of any kind**: no unit
test (no module exists) and no integration test (nothing in `tests/` invokes
`update`). A renamed `UpdatePlan` field or a changed key name fails nothing.

### Ordering divergence between the two cleanup projections

**Confirmed.** `CleanReport::render` takes `&self` (`src/cleanup.rs:224`) and
therefore **cannot** reorder its own data: it builds a local
`ordered: Vec<&CleanResult>` (`:252`), sorts by `before_bytes` descending with a
`display_path` ascending tie-break (`:253-258`), and iterates *that* (`:259`).
`output::cleanup` takes `&CleanReport` (`src/output.rs:158`) and iterates
`report.results` in **original insertion order** (`:164-189`) with no sort at
all.

So JSON `units[]` and the human unit rows are in different orders whenever
insertion order is not already size-descending, and a consumer matching the two
positionally or diffing them sees spurious differences. The paths never
co-execute (`:216` vs `:232`), so no test can observe it, and
`tests/cli_contract.rs` only ever indexes `units[0]` (`:395`, `:441`, `:474`,
`:512`) — order-independent in its fixtures.

The **scan** side is the opposite and self-consistent: `report::render` takes
`&mut ScanReport` and sorts `report.groups` **in place**
(`src/report.rs:44-48`), which `main.rs:530` relies on by passing `&mut report`.
The asymmetry between the two `render` signatures — `&self` + local copy for
cleanup, `&mut self` + in-place sort for scan — is the root of the finding.

## 9. Invariants and edge cases

**Is the exit code consistent? No — deliberately.** 1 = "ran, partially failed",
2 = "could not run". Two inconsistencies: `config edit` is the outlier, laundering
the editor's own status (`:64`, pinned by `tests/cli_contract.rs:630`); and
`update` cannot express partial failure at all.

**Can a command return 0 while having printed to stderr? Yes, routinely.**

| stderr message | Line | Exit effect |
|---|---:|---|
| `N filesystem diagnostics; rerun with a bounded root if needed` (+ ≤10 details) | `:525-546` | **none** — a Routine scan full of `PlatformRoot` errors still exits 0 |
| `scan stats: …` | `:553-558` | none (opt-in) |
| `discovery state was not saved: {error}` — **Routine** | `:452` | **none** |
| `{warning}; using seed/configured Routine roots` (from policy) | `src/policy.rs:91` | none |
| `replaced unusable discovery state after successful Full reconciliation` | `:418-420` | none (a *success* notice) |
| `discovery state was not reconciled: {reason}` | `:429` | **→ 1** |
| `discovery state was not saved: {error}` — **Full** | `:424` | **→ 1** |
| `{message}; Full state reconciliation was skipped` | `:436` | **→ 1** |
| `editor exited with {status}; …` | `:60-63` | **→ editor's code** |
| `cannot launch editor {program}: {e}` | `:53-58` | → 2 |

**This is the sharp inconsistency.** The *identical* message text — `discovery
state was not saved: {error}`, from the same `discovery_state::publish` — is
printed by the Routine branch at `:452` and the Full branch at `:424`, and the
two occurrences have **opposite** consequences: Routine ignores it, Full
escalates to a non-zero exit. A user with a broken state directory sees a
warning either way; whether their exit code is 0 or 1 depends solely on whether
they passed `--full`. Since the Routine branch's silence is arguably correct per
`src/discovery_state.rs:338`, the Full branch looks like the outlier.

**Can `run_scan` be reached twice in one process? No.** The `match` arms are
mutually exclusive, and the Clean arm is the only caller of `run_scan` from
inside `run`, calling it once at `:116`. So the "re-scoped totals" comment at
`:460-463` describes the phase sequence *within* one call, not a second call.
There is no double-initialised renderer, no global state, no `static mut` in the
file — the renderer is a plain local (`:329`) whose `finish_and_clear()` runs
once at `:507` before the report. The comment is about ordering *within* the
call: `phase(Analysis)` first (re-scopes totals), then `units_total` (starts
the fresh scope) — reversed, the first phase's total would be attributed to the
new scope.

**Is `checked_sub` the only overflow guard? Yes, in this file.** `scan_start`
(`:314`) is used unguarded at `:472` and guarded at `:465` (§4).

**Config directory read-only.** Only *creation* fails. `load_or_create` (`:316`,
`:99`) calls `create_initial`, which does `fs::create_dir_all(parent)?`
(`src/config.rs:244`) → `AppError::Io` via `#[from]` (`src/error.rs:9`) → exit 2.
If the file exists and is readable, `load` performs no writes and everything
works. Scope: `scan`, bare invocation, `clean …`, `config show` and `config edit`
all exit 2; `config path` works (resolves without touching the FS, `:41`) and
`update` works (never loads config, `:23`). `--config /other/path.toml` is the
documented escape hatch (`src/config.rs:82-84`).

**Can `process::exit` truncate buffered stdout? No.** `main` calls
`std::process::exit` at `:8` and `:11`, which does skip destructors — but
nothing is buffered waiting for them. Rust's `std::io::Stdout` is wrapped in a
`LineWriter`, so it flushes on every `\n` regardless of whether the target is a
TTY or a pipe. Every output statement here is a `println!`/`eprintln!` ending in
a newline (`:25`, `:27-33`, `:35`, `:41`, `:43`, `:60-63`, `:80`, `:182-190`,
`:192`, `:214`, `:217-231`, `:233-241`, `:247-255`, `:512-519`, `:521`,
`:525-546`, `:553-566`). The C-stdio block-until-`exit` failure mode does not
apply to Rust, and the one-newline assertions at
`tests/cli_contract.rs:204`/`:242` are consistent with nothing being lost. The
only theoretical exposure is a *partial* final line with no trailing newline,
which this file never produces. **Definitive: piped JSON output cannot be
truncated by `process::exit`.** The corollary is the actual risk: safety comes
from `LineWriter` plus newline-terminated writes, not from an explicit flush, so
adding a buffered writer or a non-`println!` path would remove it silently.

**Duplicate state publication from scanning twice?** Not reachable — and a
`--full` scan publishes at most once (single `publish` at `:415`). `clean --full`
does read state twice (`:120`, after `run_scan`'s read at `:374`), but the
second is a read-after-write of the freshly published value, which is intended.

**Redundant root echo in human cleanup output.** `main.rs:233-241` prints
`combined roots {…}` on stdout, then `report.render()` prints `combined scope: N
root(s) [same list], …` (`src/cleanup.rs:226-236`) from `report.selected_roots`,
which cleanup populates as `roots.clone()` (`src/cleanup.rs:799`, `:846`) — the
same collapsed vector `main.rs` passed in. A human `clean` run lists the roots
twice, in two formats, as the first two lines of the report.

**Input reaching `main.rs` unvalidated.** Yes, in one case: `clean ROOT` never
goes through `policy::resolve` — only the `--known` branch calls it (`:133`). The
`InvalidRoot` checks in `src/policy.rs:32-41` (exists, is a directory, is not a
symlink) therefore **do not run for the explicit `ROOT` source**; only the
lexical `absolutize_root` (`src/cli.rs:153-159`) applies.
`src/cli.rs:150-152` acknowledges this and defers to "the policy/cleanup
layers". `clean_with_roots_policy_selector` (`src/cleanup.rs:737-745`) takes
roots as an already-resolved `&[PathBuf]` and does not re-run the symlink check
at its entry. The deeper authorization in `cleanup.rs` is the real boundary, so
this is not a hole — but the comment's promise of "is a real directory, and is
not a symlink" is fulfilled by a different layer than the one named. `scan ROOT`
*is* fully validated, because `policy::resolve` runs at `:317`.

## 10. Testing

**`src/main.rs` has zero inline tests.** Verified:
`grep -cE '#\[cfg\(test\)\]|#\[test\]' src/main.rs` → `0`. There is no
`#[cfg(test)] mod tests` in the file; the `use` statements around lines 293-294
and 356 are ordinary function-scoped production imports inside `run_scan` — a
common source of the opposite impression. The same grep on the other two output
modules: `src/main.rs` **0**, `src/output.rs` **0** (no unit tests), and
`src/report.rs` **8** — it *does* have an inline suite (`src/report.rs:72-195`):
`report_escapes_control_bytes_in_paths` (`:90`),
`report_escapes_non_utf8_paths` (`:103`), `report_ordering_and_zero_report`
(`:124`), `zero_state_uses_the_same_inventory_wording_as_results` (`:133`),
`format_bytes_scales_to_eib_and_promotes_rounded_units` (`:144`),
`groups_render_every_group_with_ownership_and_deduped_total` (`:155`),
`groups_total_does_not_double_count_equal_roots` (`:184`).

**Consequently `update_json` (`:584-606`) is entirely untested** — no unit test
(no module exists) and no integration test (nothing in `tests/` invokes
`update`), and it is the only machine-readable surface hand-building its
envelope outside `output.rs`'s `EnvelopeV1` (§8).

### `tests/cli_contract.rs` — 658 lines, 8 tests

Drives `env!("CARGO_BIN_EXE_cargo-cleanme")` as a subprocess and asserts on
status, stdout and stderr. Verified line numbers:

| Test | Line | What it pins about `main.rs` |
|---|---:|---|
| `the_staged_binary_is_the_external_subcommand_cargo_will_run` | 79 | `cargo cleanme --version` (external) byte-identical to direct — i.e. the argv normalization in `parse_normalized` (`:16`) is correct; plus a premise guard that the staged binary is the one Cargo resolves |
| `cargo_external_subcommand_help_matches_direct_help` | 120 | `--help` exits 0, identical both forms |
| `cargo_external_config_edit_help_matches_direct` | 154 | `config edit --help` exits 0, identical both forms |
| `json_scan_is_one_versioned_document_and_stats_stay_on_stderr` | 177 | `schema_version == 1` (`:200`), `operation == "scan"` (`:201`), `scope == "explicit"` (`:202`); **`plain.stdout == with_stats.stdout`** (`:198`) — `--stats` cannot perturb stdout; stderr contains `scan stats:` (`:203`); **exactly one `\n`** (`:204`) |
| `json_cleanup_emits_the_requested_mode_and_machine_summary` | 208 | `schema_version == 1` (`:238`), `operation == "clean"` (`:239`), `mode == "simulate"` (`:240` — `--dryrun` → `CleanMode::Simulate`, `main.rs:105`), `summary.simulated == 0` (`:241`), one `\n` (`:242`) |
| `json_scope_block_is_emitted_with_nonzero_exit_status` | 246 | **`status.code() == Some(1)`** (`:268`) with `result.scope_blocked == true` (`:270`) — pins the `main.rs:257-261` exit rule |
| `json_unattended_yes_executes_through_cargo_and_emits_typed_result` | 286 (`#[cfg(unix)]`) | six invocations against a `/bin/sh` cargo stub: `summary.cleaned == 1`, `units[0].reason_code == "cleaned"`, `selector_kind == "profile"`, `selector_value == "dev"` (`:394-397`); `before_bytes` and `selector_estimate_bytes` both `null` under a selector while `output_union_before_bytes >= 4096` (`:398-411`); `units[0].outcome == "simulated"` under `--dryrun` (`:441`); `reason_code == "selector_unsupported"` + `policy_disposition == "selector_estimate_unavailable"` (`:473-480`); `reason_code == "selector_invalid"` (`:512`); `selector_kind == "package"` with `fixture@0.1.0` (`:581-583`). Also asserts the stub's call count after each run, so "did this actually spawn a clean?" is verified independently |
| `config_edit_uses_fake_editor_process_and_keeps_invalid_edits` | 582 | **`status.code() == Some(17)`** (`:646`) — the passthrough at `main.rs:64`; an invalid edit exits non-zero with `is invalid` in stderr (`:649`) and leaves the bad content in place (`:650`); a valid edit exits 0, prints `edited` (`:657`), reads back as `recency_seconds == 42` (`:658-664`); a no-op edit is still 0 and does not disturb the file (`:665-673`) |

**`tests/end_to_end.rs` — 211 lines, 1 test**
(`end_to_end_reports_only_inactive_artifact_projects`, `:114`). It does *not*
invoke the binary; it drives the library directly with a fake `CargoRunner`
(`:75-111`) through `policy::resolve` → `discovery` → `resolve_workspaces` →
`build_groups` → `analyze_groups` → `report::render`, reproducing `run_scan`'s
sequence by hand. Its JSON assertions: `result.groups[0].ownership == "private"`
(`:203`), `result.summary.group_count == 1` (`:204`), `bytes >= 8192` (`:209`),
and human/machine agreement (`:210`).

**`tests/common/mod.rs` — 102 lines, not a test target** (Cargo compiles only
top-level `tests/*.rs`; each suite pulls it in with `mod common;`). Exactly two
fixture helpers, `set_path_modified` (`:24`) and `backdate_file` (`:98`), both
cross-platform mtime. It asserts nothing about the binary.

### What is well pinned, and what is not

**Well pinned — the machine contract, and pinned thoroughly:** argument parsing
in both invocation forms; clap's exit code for help; the JSON envelope's
`schema_version` / `operation` / `scope` / `mode` by name, value and type;
per-unit `reason_code`, `policy_disposition`, `outcome`, `selector_kind`,
`selector_value`; the `before_bytes` vs `output_union_before_bytes` distinction
under a selector; the exit-1 rule for `scope_blocked`; the editor's exit-code
passthrough and its three post-edit checks; one trailing newline; and
stdout/stderr separation for `--stats`.

The key structural observation: **`output.rs` has no unit tests, yet its output
is pinned by name, value and type from the outside.** A subprocess test asserts
the real serialised bytes, which is defensible — but the coverage is *incidental*
to the assertions someone happened to write, not systematic. A field nobody
wrote an assertion for has no protection at all, which is exactly how the
`scope`-label bug survives a suite that *does* assert `scope` (§8).

**Only incidentally covered, or not at all:**

| Orchestration decision | Coverage |
|---|---|
| `scope` for a **configured** `scan.root` | **none.** `cli_contract.rs:202` pins only the CLI-root case — the one that is correct. `end_to_end.rs:202` sidesteps the bug by hardcoding `output::scan(&report, "routine")` as a literal, even though its fixture sets `ScanConfig::root` (`:121`) and so resolves to `ScanScope::Explicit`. The test encodes the bug's output as expected |
| `emit_output` suppression on `clean --full` | **none** — no test invokes `clean --full`; it needs a state file with learned roots |
| Exit code when a **Full** publish fails (`:569-578`) | **none** — needs an unwritable state directory |
| Routine-vs-Full divergence of the *same* message | **none** |
| `--stats` **content** | partial — `cli_contract.rs:203` greps `scan stats:`. The four counter strings on the cleanup line (`:250-253`) are never asserted |
| `collapse_roots` | **none** — private to this untested file, so the nest-collapse safety property is untested at every level |
| Root-source precedence | only via clap's conflict declarations (`src/cli.rs:50-54`, `:326-344`) — the parse-level guarantee, not the runtime selection |
| Bare invocation == Routine scan | **none** through the binary; `src/cli.rs:206-209` proves only that the parse yields `None` |
| `state generation last_full_at=` on stdout | **none** — no test runs cleanup with a `Some` generation |
| Human vs JSON cleanup **ordering** | **none** — never co-executed; every assertion indexes `units[0]` |
| Read-only config directory | **none** |
| `update` / `update_json` | **none** |

**Honest assessment.** `main.rs` is the composition root — the only place the
two pipelines are wired — and composition is precisely what unit tests with
fakes do not cover. `end_to_end.rs` re-implements `run_scan`'s call sequence in
the test body, which is itself a structural risk: the sequence can drift from
`main.rs` and the test stays green. It has already drifted once, per its own doc
comment (`:6-9`) — it previously drove a legacy path the binary never executed,
and a dead implementation stayed green for a long time. It also bypasses
`checked_sub`, the reconciliation block, and the entire `Clean` arm, none of
which it could observe anyway. What is genuinely covered is
`cli_contract.rs`'s subprocess assertions; what is not covered is every decision
that belongs to `main.rs` rather than to a module.

## 11. Review checklist

1. **Exit-code consistency for state warnings.** `main.rs:569-578` escalates a
   *failed state publish* to exit 1, while the byte-identical message in the
   Routine branch at `main.rs:452` is ignored — and `src/discovery_state.rs:338`
   says state is "an optimization only". Verify the exit code a read-only state
   directory produces for `scan --full` versus `scan`.
2. **`scope` label derivation.** `main.rs:516-522` uses `full` and
   `explicit_scope` (`main.rs:314`, `root.is_some()`) while `policy::resolve`
   already returned the real `policy.scope` at `main.rs:317`. A configured
   `scan.root` (a validated first-class option, `src/config.rs:110-112`) yields
   `Explicit` but the label `"routine"`. Compare `main.rs:221-227`, which gets
   it right.
3. **`emit_output` is load-bearing and untested.** `main.rs:309`, guarded at
   `:510`, `true` at `:264`/`:266`, `false` at `:116`. Confirm `clean --full`
   still writes exactly one JSON document to stdout, and that removing the
   guard would not produce two.
4. **`process::exit` and buffered stdout.** `main.rs:8`/`:11` skip destructors;
   safety comes from `Stdout` being a `LineWriter` plus newline-terminated
   writes, *not* from an explicit flush. A new buffered writer, a `write!`, or a
   non-newline-terminated final line removes that property silently.
5. **stdout/stderr interleaving.** `main.rs:214` puts
   `state generation last_full_at=` on stdout, against the project's own rule
   (`main.rs:326-327`, `:548-550`). Human-only today; a landmine if `:211` is
   ever relaxed.
6. **Serialization errors use the wrong variant.** `main.rs:518` and `:230`
   produce `configuration error: cannot serialize JSON report: …`; `main.rs:189`
   omits the prefix entirely, so the two JSON sites report the same failure
   class differently.
7. **Redundant root echo.** `main.rs:234` and `src/cleanup.rs:226-236` print the
   same root list, in two formats, as the first two lines of a human `clean`
   report (`selected_roots` is `roots.clone()`, `src/cleanup.rs:799`/`:846`).
8. **Human/JSON cleanup ordering divergence.** `src/cleanup.rs:252-258` sorts a
   *local* vec that `output::cleanup` (`src/output.rs:164-189`) never sees, so
   JSON `units[]` is insertion-ordered while human rows are size-descending. The
   scan side is self-consistent (`src/report.rs:44-48` sorts in place). Confirm
   the projections are not expected to agree positionally.
9. **`--include`/`--exclude` cannot clear a configured list.** `main.rs:160-169`
   falls back to config on an empty CLI vec. Deliberate but undocumented; clearing
   needs a distinct flag, not a reinterpretation of the empty list.
10. **`collapse_roots` canonicalization is best-effort.** `main.rs:272` uses
    `unwrap_or(p)`, so a missing or unresolvable root passes through unvalidated.
    Confirm the cleanup layer is the only thing between that and a deletion
    boundary, and that the `sort()` at `main.rs:274` is understood to be
    *required* by the containment filter at `:278`.
11. **Explicit `ROOT` skips `policy::resolve`.** Only the `--known` branch calls
    it (`main.rs:133`), so the exists/is-a-directory/is-not-a-symlink checks at
    `src/policy.rs:32-41` never run for `clean ROOT` — a promise
    `src/cli.rs:150-152` makes while naming a different layer.
12. **`main.rs` has no tests; `update_json` is uncovered.** Zero `#[cfg(test)]`
    here and in `output.rs`; 7 in `report.rs`. The machine contract *is* pinned
    by `tests/cli_contract.rs` (§10), so the gap is specifically the
    *orchestration* decisions — plus `update_json`, the one hand-rolled envelope
    with no test of any kind.
