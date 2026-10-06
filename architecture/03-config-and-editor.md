# Config & Editor — policy input and safe in-place editing

> Component deep dive · part of the [architecture overview](overview.md)

Covers `src/config.rs` (1408 lines: 559 production, 19 inline tests), the
repository-root `config.toml` (45 lines), and `src/editor.rs` (215 lines,
2 inline tests).

## 1. Responsibility

**Owns** — the four config structs, their `serde` attributes and defaults
(`src/config.rs:12-69`); reading, validating and normalizing a config file
(`load`, `src/config.rs:90-205`); first-use creation from an embedded template
(`create_initial`, `src/config.rs:410-555`); which config file an invocation
means (`ConfigPathResolver`, `src/config.rs:70-89`); the `config show`
projection (`show`, `src/config.rs:557-559`); editor resolution
(`editor::resolve_editor`, `src/editor.rs:7-37`).

**Does not own** — any policy *decision* (`config.rs` validates shape and range;
scope and recency belong to `policy::resolve`, `src/policy.rs:19-58`, and the
CLI-over-config merge to `src/main.rs:155-170`; see
[Policy & scope](04-policy-and-scope.md)); any authorization decision
(`cleanup.allowed_output_roots` is carried and absoluteness-checked here,
interpreted by `cleanup::is_authorized`, `src/cleanup.rs:453-462`; see
[Cleanup](09-cleanup.md)); process launching (`editor.rs` resolves,
`src/main.rs:49-58` spawns); persisted discovery state, a separate file with its
own atomic publish (see [Discovery state](06-discovery-state.md)).

**The template is not the runtime config.** The most load-bearing fact about
`config.toml`, enforced from both sides: `src/config.rs:12` embeds the repository
file verbatim at compile time (`include_str!("../config.toml")`), and the only
reader of those checked-in bytes is `create_initial` (`src/config.rs:473`).
`config.toml:5-8` states it directly — runtime configuration "is resolved from
the platform application config directory (or `--config PATH`); this repository
file is a distribution template and is never loaded implicitly from the current
working directory" — and `Cargo.toml:18-21` calls `/config.toml` "a hard
requirement, not a nicety", with the allowlist entry at `Cargo.toml:24`. Running
the binary from inside a checkout therefore does **not** pick up that checkout's
`config.toml`.

## 2. The configuration schema

Four structs, all `#[serde(deny_unknown_fields)]` (`src/config.rs:13`, `:21`,
`:29`, `:40`), so a mistyped key is a parse error rather than a silently ignored
setting.

| Section | Field | Type | Default | Meaning |
|---|---|---|---|---|
| `[scan]` | `recency_seconds` | `u64` | `300` (`src/config.rs:64-66`, `:43`) | Inactivity window: a candidate must be untouched at least this long. `0` is rejected; values above `u64::MAX / 1_000_000_000` are rejected (`:99-106`). |
| `[scan]` | `root` | `Option<PathBuf>` | `None` (`:45`, `:57`) | Exclusive scan scope. Must be absolute (`:112-114`). A CLI root takes precedence (`src/policy.rs:30`). |
| `[scan]` | `ignore` | `Vec<String>` | `[]` (`:44-45`, `:56`) | Absolute glob patterns pruned from global/routine discovery. Compiled with `globset` (`:146-149`) and matched against **canonical** paths (`src/discovery.rs:38-45`). |
| `[scan]` | `unignore` | `Vec<PathBuf>` | `[]` (`:46-47`, `:57`) | Exact absolute directories re-included beneath ignored trees. Not globs — a component-wise prefix test (`p.starts_with(u)`) resolved by `Filters::disposition` (`src/discovery.rs:68-99`). Must be absolute (`:120-122`). |
| `[scan]` | `learned_root_retention_days` | `u32` | `30` (`src/config.rs:67-69`, `:50`) | How long a learned developer root survives after the last project observation. `0` disables expiration (`config.toml:14-15`). Accepted range `0..=3650`, enforced in `load` (`:107-111`). |
| `[cleanup]` | `allowed_output_roots` | `Vec<PathBuf>` | `[]` (`:22-23`) | Authorization for already-private output outside the `clean ROOT` sandbox. Must be absolute (`:123-125`). See below. |
| `[cleanup.policy]` | `min_reclaimable_bytes` | `u64` | `0` (`:30-31`) | Minimum reclaimable size for a group. `--min-reclaimable-bytes` replaces it when supplied (`src/main.rs:157-158`). |
| `[cleanup.policy]` | `min_inactive_seconds` | `Option<u64>` | `None` (`:32`) | Optional *extra* quiet age on top of the safety recency guard. `--older-than` wins (`src/main.rs:159`). Values above `u64::MAX / 1_000_000_000` are rejected (`:126-132`). |
| `[cleanup.policy]` | `include` | `Vec<String>` | `[]` (`:33-34`) | Globs a candidate's physical output root must match to be selected at all (`src/cleanup.rs:881`). |
| `[cleanup.policy]` | `exclude` | `Vec<String>` | `[]` (`:35-36`) | Globs that prune a candidate's output root (`src/cleanup.rs:891`). |

### The two defaults that are not `Default::default()`

`Config`, `CleanupConfig` and `CleanupPolicyConfig` derive `Default`
(`src/config.rs:12`, `:20`, `:28`), so a missing table yields zeroed/empty
values. `ScanConfig` deliberately does **not** derive it: the hand-written
`impl Default` at `src/config.rs:53-63` restores `recency_seconds = 300` and
`learned_root_retention_days = 30`, which is what makes an absent `[scan]` table
safe — otherwise a `[cleanup]`-only config would silently get
`recency_seconds = 0`, which `load` then rejects at `:98-102`.

`ScanConfig` is also the only struct with field-level serde defaults, because
serde's `Default` is not consulted for a *present* table with a missing key:
`default_recency` (`:41`, `:62-64`) and `default_retention` (`:48`, `:65-67`)
make those two keys optional inside `[scan]`. `min_inactive_seconds` is the one
field with **no** `#[serde(default)]` (`:32`) — the only tri-state field,
relying on serde treating `Option<T>` as optional-on-missing. No test isolates
"present `[cleanup.policy]` table, absent `min_inactive_seconds`"; the nearest
one omits the whole table (`:480-486`). Treat that combination as untested.

### `CONFIG_TEMPLATE`

`pub const CONFIG_TEMPLATE: &str` (`src/config.rs:12`) is a `&'static str`
compiled into the binary and written byte-for-byte on first use
(`f.write_all(CONFIG_TEMPLATE.as_bytes())`, `:473`). Three tests hold it honest:
`bootstrap_bytes_equal_checked_in_template` (`:510-519`) compares it against the
file actually written, `config_template_matches_repository_file` (`:471-478`)
against the checked-in `config.toml`, and
`checked_in_template_parses_and_matches_defaults` (`:452-469`) parses it and
compares every field against `Config::default()`. The template writes
`recency_seconds` and `learned_root_retention_days` explicitly and leaves
`scan.root` and `cleanup.policy.min_inactive_seconds` commented out
(`config.toml:19`, `:43`), so its effective values are exactly the defaults.

### `cleanup.allowed_output_roots` — authorization, never proof

`config.toml:28-35` carries the most important comment in the schema, and the
field is *currently inert*. It authorizes a **location** for deletion of output
already private but outside the `clean ROOT` sandbox; entries must be absolute
directory roots, not globs, and symlink roots are invalid
(`src/cleanup.rs:445-447`). Shared, uncertain and external-unproven output
"remains inventory-only even inside an allowed root": authorization "never
manufactures ownership proof and cannot promote ExternalUnproven to private"
(`config.toml:30-33`; ADR 001, C002), restated as a hard contract at
`src/cleanup.rs:451-452`. An empty list "preserves M004-style containment"
(`config.toml:33`, `src/cleanup.rs:450`) and is the shipped default. The
subtlety: the field "is currently operationally redundant because PrivateBounded
output is normally workspace-contained, but is preserved for schema
compatibility" (`config.toml:34-35`) — a retained extension point, not dead
weight to be deleted, and not a way to widen what the tool touches today. This
is [overview invariant 2](overview.md#6-cross-cutting-invariants).

### Glob syntax assumptions

Every pattern in `include`, `exclude` and `scan.ignore` is compiled with
`globset::Glob::new` (`src/config.rs:143`, `:148`; recompiled per use at
`src/discovery.rs:22`, `src/cleanup.rs:1248`), always with default `globset`
construction: no `literal_separator`, no `backslash_escape` override,
case-sensitive. So `*` crosses path separators, `/` is not special on Windows,
and matching is `globset`'s own: both sides are `/`-spelled, because
`Candidate::new` normalizes candidate separators and `backslash_escape`
defaults to `!is_separator('\\')` — with it off, a `\` in a pattern is compiled
to a `/` too. `scan.unignore` is not a glob and is never compiled as one.

Because matching is against canonicalized paths, `load` rewrites each pattern's
glob-free prefix through `canonical_pattern_prefix` (`src/config.rs:323-366`,
applied at `:154-186`). The rewrite is conservative: it stops at the first
`* ? [ {` (`:324`) and leaves the pattern alone if the prefix ends in a backslash
(splitting there would land inside an escape sequence), if the prefix is relative
(`:332-334`), or if `fs::canonicalize` fails (a non-existent path still means what
it says — it matches nothing); trailing separators are structural and preserved
around the canonicalized name (`:331`, `:335`, `:359`). Without it, a pattern
spelled the way the user's own shell shows it (`/tmp/...` where `/tmp` is a
symlink) would silently exclude nothing.

**The canonical spelling is escaped, because it is a path and not a pattern.**
`escape_glob_literal` (`src/config.rs:275-307`) bracket-escapes `* ? [ ]`. A
directory named `real[abc]`, reached through a symlink, used to splice in as a
character class: the rule stopped excluding the tree the user named and started
excluding two they never wrote down.

The spelling is also `glob_spelling` (`src/config.rs:219-266`) — `/`-normalized
— before it is escaped, and the order is load-bearing in both directions. A `\` in
the canonical name is what made the whole rewrite a no-op on Windows: every
canonical path there contains one, and `escape_glob_literal` used to reject it,
so the escape never ran. Normalizing first is what lets the escape run; escaping
first would double the separator into a path component that does not exist.
`glob_spelling_with` takes the separator as an argument rather than reading
`cfg!`, so the Windows answer is asserted on a Unix lane (C024).

**A Win32 verbatim prefix is stripped before that (`dereference`,
`src/config.rs:250-273`), and the hosted Windows lane is why it is there.**
`fs::canonicalize` answers `\\?\C:\…` whenever the path it was given was already
verbatim, and can answer it for paths that were not. The prefix is an API
artefact rather than part of the path, and it is actively harmful in a pattern
twice over: the `?` is a glob metacharacter, so the split at `:324` would land
*inside* the prefix and the rewrite would decline the whole pattern, and a
prefix that did survive would demand a candidate spelling no walk produces. It
is a property of the platform's spelling of a path, not of the user's pattern,
so removing it cannot change what any user wrote. `\\?\UNC\server\share`
becomes `\\server\share`, which normalizes to the `//server/share` a candidate
carries.

The `None` arm is now reachable for exactly one input pair. `{` opens an
alternation no bracket form closes, and it is left untouched rather than guessed
at. `\` is no longer in that set: after normalization it is a character of a
*file name*, never a separator, and it is doubled where `globset` treats it as an
escape character. That arm is unreachable on Windows because no file name there
may contain a backslash, and it is tested individually on Unix
(`a_backslash_in_a_canonical_directory_name_is_spliced_as_a_escape`).
`PatternKind` (`src/config.rs:207-217`) keeps `scan.unignore` out of this: it is a
literal path by design, so escaping it would corrupt the path it names — and its
separators are the filesystem's own, not `/`.

**The rewritten patterns are re-validated** (`src/config.rs:194-203`). The
rewrite manufactures text that appears nowhere in the file, so validating on the
way in does not certify what is compiled on the way out. No authorization depends
on this list — `ignore`/`include`/`exclude` filter discovery and policy, never
ownership. Tests: `a_canonical_spelling_cannot_inject_glob_characters` (Unix),
`a_spliced_canonical_spelling_matches_the_directory_it_names` (every lane),
`a_backslash_in_a_canonical_directory_name_is_spliced_as_a_escape` (Unix), and
`a_bracketed_canonical_spelling_is_matched_literally_on_windows` (Windows).

## 3. Loading, creation, and validation

### `load` — read, parse, validate, normalize

`load(path) -> Result<Config, AppError>` (`src/config.rs:90-205`): read to string
and `toml::from_str` when `path.exists()`, mapping failures to
`AppError::Config("cannot read …")` / `("cannot parse …")` (`:89-93`); **return
`Config::default()` when the file is absent** (`:94-96`); then check, in order
(`:97-132`), that `recency_seconds` is non-zero and not absurdly large,
`learned_root_retention_days <= 3650`, `scan.root` absolute, every
`scan.ignore` / `scan.unignore` / `cleanup.allowed_output_roots` entry absolute,
and `min_inactive_seconds` not absurdly large; then compile every pattern the
scanner and cleaner will compile (`:133-149`); then canonicalize pattern
prefixes (`:150-178`).

The absence rule and the range checks together are why deletion is dangerous: an
absent file yields `recency_seconds = 300`, which validates clean. **Absence is
indistinguishable from "all defaults"** — the whole reason `src/main.rs:66-73`
refuses to accept a deletion as an edit, and the reason `ensure_exists` exists.
Errors surface as `AppError::Config(String)`, printed as `cargo-cleanme:
configuration error: …` with exit code 2 (`src/main.rs:9-12`,
`src/error.rs:4-5`). The rationale for compiling globs here, at `:133-134`, is the
design rule: "`config edit` fails exactly as loudly as the next scan/clean would."

**Unknown keys are rejected, not ignored.** All four structs carry
`#[serde(deny_unknown_fields)]`, so a misspelled key fails at `toml::from_str`
as `cannot parse <path>: …`. There is no forward-compatibility allowance and no
migration shim.

### `load_or_create` and `ensure_exists`

`load_or_create` (`src/config.rs:368-373`) calls `create_initial` only when the
file is missing, then `load`s it. It never overwrites: a malformed file stays
malformed and stays on disk (`:521-531`, `:501-508`). Used by `scan`
(`src/main.rs:299`), `clean` (`src/main.rs:99`) and `config show`
(`src/main.rs:43`).

`ensure_exists` (`:224-229`), documented at `:223` as "Ensure a config editor
target exists without requiring its current contents to parse," creates the
template when missing and returns without reading or validating. That is what
makes `config edit` repairable: a user with a broken config can still open it,
where `load_or_create` would refuse to show them the file they need to fix.

### `create_initial` — atomic, verbatim, never overwriting

`src/config.rs:410-555`: `create_dir_all` the parent (`:411-415`, re-checked at
`:421-426`); write to `<file_name>.tmp.<pid>.<nonce>.<attempt>` in the **same
directory** (`:478-481`, so publication stays on one volume) opened
`create_new(true)` (`:297-301`); `write_all` the template verbatim and `sync_all`
it (`:322-325`); publish with `fs::hard_link(&temp, path)` then
`remove_file(&temp)` (`:329-330`). Linking is atomic and **cannot overwrite**. A
regular file already at the destination means a racer published first, which is
success (`:350-352`). After 100 attempts: `AppError::Config("could not create a
temporary config beside …")` (`:365-368`); and if the destination exists and is
not a regular file the error path refuses to replace it (`:355-359`).

So writes are atomic at publication, the template is byte-for-byte verbatim, and
an existing config is never replaced.

**The staging name must be unique among everything that can create a config at
once, or "lost the race" becomes a routine event dressed as an error.** The pid
separates *processes*, so threads inside one process were the only remaining
collision — and they collided on every concurrent first use, because they all
began at the same `attempt`. A process-wide `STAGING_NONCE` (`:253`) removes the
collision rather than teaching the code to recognise a lost one. The `attempt`
suffix stays for the case the nonce cannot cover: a stale `.tmp.` file left by a
crashed process whose pid the OS later reuses, where `AlreadyExists` is the
authoritative answer (`:318`).

**Where the filesystem is the discriminator and where it is not.** Both race
steps once reported "somebody else got there" only as `AlreadyExists`, and only
`AlreadyExists` was tolerated. Three hosted findings changed that:

- `create_new` on a taken staging name reports `AlreadyExists` on POSIX and
  `PermissionDenied` — os error 5 — on Windows, because the name is reserved
  before its metadata is committed. With the nonce, nothing else is competing,
  so this no longer arises; if it ever does, it is a genuine permissions problem
  and must hard-fail.
- Probing `temp.exists()` instead of reading the kind was tried and is **wrong
  on both platforms**: the winner unlinks its staging file, so the probe reads
  free and the real failure escapes. A syscall answers whether the name was
  taken *then*; a probe answers whether it is taken *now*. It was observed
  regressing Linux, macOS, and the 1.89 lane, and later leaking a Windows
  `PermissionDenied` through the same gap.
- The **publish** step is the mirror image and is decided by the filesystem
  (`:350-352`). The destination is never unlinked, so `path.is_file()` is a
  stable answer, not a race against a winner cleaning up. It is also the only
  file that can exist at that path, published solely by hard-linking a written
  and synced staging file, so a regular file there is always a complete
  template. The adjacent refusal of a non-file destination (`:355-359`) is
  unchanged: "exists" must not become "acceptable".

`step_error` (`:255-257`) names the call that failed while preserving
`ErrorKind` **exactly** — everything below decides by inspecting the kind, so a
wrapper that changed it would silently stop the race handling recognising its own
outcome. It is also what made the above diagnosable: the pre-fix Windows failure
was an unactionable `Io(Os { code: 5 })`, and became
`creating a staging file failed: Access is denied. (os error 5)`.

`concurrent_first_use_creates_one_complete_template` (`:599-632`) pins this: 16
workers over 24 rounds, a fresh nested path each round, asserting every racer
succeeds and the bytes are one complete template. One 8-thread round missed the
Windows failure most of the time, so the premise is strengthened deliberately —
a premise the defect can miss is not a premise, and a single green run of it is
not evidence either.

Two honest caveats remain: no directory-level `fsync` after the link, and the
mechanism is `hard_link` rather than `rename` (same directory, so still atomic,
but a link count is briefly 2).

`hard_link` is also the one thing a filesystem can refuse outright. FAT32 and
exFAT, several CIFS mounts, and WSL's `/mnt/c` drvfs report `EOPNOTSUPP` — after
`create_new`, `write_all` and `sync_all` have all succeeded — which used to make
**every** command fail at startup, because `load_or_create` is on every path.
The fallback (`src/config.rs:495-514`) is `OpenOptions::create_new` straight at
the destination. It keeps the property the design cannot give up: `create_new` is
exclusive, so a concurrent winner is never overwritten. What it gives up is
crash-atomicity of the *contents* — a crash mid-write leaves a truncated file,
which `load` refuses rather than reading as defaults. A failure *after* a
successful `create_new` removes its own partial file, so the lost-race check
below can never mistake this process's truncated write for a winner's complete
config.

### `show` — a projection, not a file dump

`show(&Config) -> Result<String, AppError>` is `toml::to_string_pretty`
(`src/config.rs:557-559`), serializing the **in-memory, validated, normalized**
config. Comments and original formatting are gone; `scan.ignore` and friends
appear in canonicalized spelling rather than the spelling the user typed; absent
optionals are omitted rather than written as null (TOML has no null, and the
serializer skips a `None` struct field instead of erroring); table order follows
struct declaration order.

## 4. Path resolution

`ConfigPathResolver` (`src/config.rs:70-89`) holds `override_path:
Option<PathBuf>`. `src/main.rs:17` constructs it once from `cli.config` (the
`--config` global flag, `src/cli.rs:19`) and resolves the path once per process,
before any command dispatch — including for `update`, which does not use the
config at all.

`path(&self) -> Result<PathBuf, AppError>` (`:78-86`) has exactly two
precedences. If `override_path` is `Some` it returns a clone verbatim — **no
absolutization, no canonicalization, no existence check, no parent creation**, so
a relative `--config` is used as written. Otherwise it takes
`ProjectDirs::from("", "", "cargo-cleanme").config_dir().join("config.toml")`
(`:82-85`), and a `None` from `ProjectDirs` becomes `AppError::Config("platform
config directory is unavailable; use --config")` (`:82-84`), exit code 2.

With `directories = "6"` (`Cargo.toml:52`) and an empty qualifier/organization,
the resolved base is:

| Platform | Config directory | Full config path |
|---|---|---|
| Linux | `$XDG_CONFIG_HOME/cargo-cleanme`, else `$HOME/.config/cargo-cleanme` | `…/cargo-cleanme/config.toml` |
| macOS | `$HOME/Library/Application Support/cargo-cleanme` | `…/Application Support/cargo-cleanme/config.toml` |
| Windows | `{FOLDERID_RoamingAppData}\cargo-cleanme\config` | `%APPDATA%\cargo-cleanme\config\config.toml` |

The Windows row is the surprising one: `ProjectDirs` appends a `config`
subdirectory under the roaming app-data folder, so the file sits one level
deeper there than on the Unix platforms; the empty `organization` argument means
no intermediate organization directory on any platform (`directories-6.0.0`
`src/win.rs:98-99`, `src/lin.rs:92-94`, `src/mac.rs:91-97`). `ProjectDirs` returns
`None` only when the home directory (Unix) or both known-folder lookups (Windows)
are unavailable — an environment failure, not a permissions one. `config path`
prints the path and nothing else (`src/main.rs:41`): it creates and reads nothing.

## 5. `config show` and `config edit`

All three subcommands are one match arm at `src/main.rs:39-84`, after the shared
path resolution at `:17`.

**`config path`** (`:41`) — `println!("{}", path.display())`. No load, no create.
The diagnostic answer to "which file is this?", which matters because that file
may not exist yet.

**`config show`** (`:42-44`) — `config::show(&config::load_or_create(&path)?)`:
bootstrap if missing, then print the projection from §3. It therefore creates the
file on first use, and fails loudly on an invalid config rather than printing
anything.

**`config edit`** (`:45-81`) — `config::ensure_exists(&path)?` first (`:46`), so
the editor always has something to open and the current bytes need not parse;
then `editor::resolve_editor()`, mapping its `String` error into
`AppError::Config` (`:47-48`) — resolution deliberately *after* creation, so a
user with no editor configured still ends up with a config file; then
`Command::new(&program).args(args).arg(&path).status()` (`:49-52`), with the
config path as the **last** argument after any editor flags, and a spawn failure
reported as `AppError::Config("cannot launch editor …")` (`:53-58`).

Then three post-edit checks, in this order, each catching a different failure:

**(a) Non-zero editor exit → warn, propagate the editor's code** (`:59-65`). The
message says the config "was not reverted"; the arm returns
`Ok(status.code().unwrap_or(1))`, so the process exit code is the editor's. An
editor that fails mid-session (bad swap file, crash, `:`-command error) may have
left the config untouched, half-written, or syntactically invalid, and the exit
code must not be flattened to 0 or 2 because the requested interactive operation
did not complete. This branch deliberately does **not** validate the file and does
not restore anything — the file on disk is exactly what the editor left, and "not
reverted" is a guarantee, not a repair.

**(b) Deleted file → refuse** (`:66-73`). The in-code reason: "`load` treats an
absent file as 'all defaults', so a deleted config would silently reset every
policy." This is the most dangerous failure in the surface — an editor that
renames badly, or a user who `:bdelete`d the buffer and quit, would otherwise leave
cargo-cleanme running on stock defaults while believing their policy is in force.
Because `load` cannot distinguish the two states (`src/config.rs:90-98`), this is
the only place that can, after the editor has exited. Hence a hard error rather
than a warning, and no silent re-creation from the template: that would replace
the user's intended deletion with a default file and hide it.

**(c) Modified but invalid → report as invalid** (`:74-79`). `config::load(&path)`
is run and its error wrapped as `"edited config <path> is invalid: <e>"`. It
reuses `load` rather than a lighter parse because `load` is the same function
every scan and clean will run — the edit-time instantiation of the rule at
`src/config.rs:135-136`, so a bad glob, a relative `allowed_output_roots` or
`recency_seconds = 0` is reported where it was typed rather than at the next
scan. The loaded value is discarded by the `?` at `:79`; `:80` prints
`edited <path>`.

The ordering matters: a failed editor is reported as a failed editor and the file
is not second-guessed; only a cleanly-exited editor can be caught having deleted
the file or corrupted it. All of it is pinned end-to-end by
`tests/cli_contract.rs:582-658`, which drives the real binary against a fake
editor reached through `VISUAL` with an argument (`"<path>" --wait`), sets
`EDITOR` to a value that must not win, and asserts: exit code 17 propagated for
the failing mode; `is invalid` on stderr with the file left containing
`invalid = [` for the corrupting mode; and `recency_seconds = 42` visible through
`config::load` for the valid and no-op modes.

## 6. Editor resolution

`resolve_editor() -> Result<(PathBuf, Vec<String>), String>` (`src/editor.rs:7-37`).
No shell is involved — the editor string is tokenized by the crate, not handed to
`/bin/sh`.

**Precedence** (`:8-27`): `VISUAL`, then `EDITOR`. For each key, in order,
`env::var` must succeed (so a non-UTF-8 value is skipped like any other unusable
one, `:9-11`); the trimmed value must be non-empty (`:10-11`); `split_words` must
succeed, with a tokenizer error `continue`-ing to the next candidate rather than
failing the command (`:12-15`); the token list must be non-empty (`:16-18`); and
`resolve_program(words[0])` must return `Some` (`:22-25`). The comment at `:19-20`
states the intent: "An unusable value in one variable must not hide the next
candidate."

**Why the tuple.** `(PathBuf, Vec<String>)` separates the program from its
arguments because the program is resolved and validated on its own while the
remaining tokens must reach the child verbatim. `VISUAL="code --wait"` resolves
`code` on `PATH` and keeps `["--wait"]` as an argument; `src/main.rs:49-52` then
spawns `code --wait <config path>`. Folding the string into one `PathBuf` would
force either a shell or a guess about which token is the program. The fallbacks
return an empty `Vec` because they take no flags (`:30`).

**Program resolution** (`resolve_program`, `:39-45`): a spec with more than one
component, or an absolute path, must be an existing file (`path.is_file()`) and
is returned as-is; anything else goes to `PATH` lookup. Note the asymmetry — an
explicit path is checked for existence only, a `PATH` hit for runnability —
because the user named that file deliberately and executability of an arbitrary
file is not reliably testable cross-platform.

**`PATH` lookup** (`find_on_path`, `:47-80`): `env::var_os("PATH")`, and if unset
it returns `None` rather than guessing a default path (`:48`). Directories are
walked in order via `env::split_paths`; on Windows each is tried with the bare
name plus each `PATHEXT` suffix, defaulting to `.COM;.EXE;.BAT;.CMD` when
`PATHEXT` is unset (`:50-70`), other platforms joining the bare name (`:71-77`).
Candidates must pass `runnable_file`: a regular file, and on Unix one with any
execute bit set (`:82-96`); on non-Unix `is_file()` alone suffices (`:92-95`).

**Fallback list** (`:28-32`): `hx`, `vim`, `vi`, `nano`, first hit wins, each with
an empty argument vector — ordered by preference, not alphabetically. **Error**
(`:33-36`): `"no editor found; set VISUAL or EDITOR to an executable, or install
hx, vim, vi, or nano"`, which `src/main.rs:48` wraps as `AppError::Config` so it
prints with the other configuration errors and exits 2 — a missing editor is
reported as a configuration problem, which is what it is.

**Tokenizer** (`split_words`, `:98-154`): a hand-rolled lexer. Single and double
quotes open a run that only the *matching* quote closes, and backslash escapes
are honoured only inside a double-quoted run (`:107-116`). Outside quotes a
backslash escapes only whitespace, another backslash, or a quote, and is otherwise
kept literally with its successor (`:122-134`) — that is what lets
`C:\Users\Jane\file.toml` survive. A trailing backslash is `Err("… unfinished
escape")` (`:130-132`) and an unterminated quote `Err("… unfinished quote")`
(`:147-149`); both fall through to the next candidate rather than aborting.

## 7. Invariants and edge cases

What must remain true for this surface to stay safe:

1. **The embedded template is a hard packaging requirement.**
   `include_str!("../config.toml")` (`src/config.rs:12`) fails the build if
   `/config.toml` is missing, so the allowlist entry at `Cargo.toml:24` is not
   optional, and `scripts/check-release-contract.py:460-464` asserts the
   allowlist exists and contains `config.toml`.
2. **The repository `config.toml` is never read at runtime.** No code path
   resolves a config relative to the process CWD (`config.toml:5-8`;
   `ConfigPathResolver::path` has no CWD input, `:78-86`).
3. **Editing must never silently reset policy to defaults.** `load` maps absence
   to `Config::default()` (`:89-96`); the defences are the non-zero-exit warning
   (`src/main.rs:59-65`), the deletion refusal (`:66-73`) and the post-edit
   `load` (`:74-79`). Removing any of them reopens a path to a silently degraded
   policy file.
4. **Creation never overwrites.** `load_or_create` acts only when the file is
   absent (`:216-221`) and `create_initial` publishes via a non-clobbering
   `hard_link` (`:280`); a malformed config is never replaced (`:521-531`).
5. **`allowed_output_roots` is authorization, not ownership proof.** It must
   never be read as a way to make an unproven class actionable (`config.toml:30-33`;
   `src/cleanup.rs:451-452`; [overview invariant
   2](overview.md#6-cross-cutting-invariants)). "Operationally redundant"
   (`config.toml:34-35`) does not mean unvalidated — it is still absoluteness-
   checked at `:123-125`.
6. **Retention bounds.** `learned_root_retention_days` is `u32`, range-checked to
   `0..=3650` at `:107-111`; `src/policy.rs:46` narrows it to `u16`, so that check
   is load-bearing for the cast, not just for policy sanity.
7. **`recency_seconds = 0` is rejected** (`:98-102`), not accepted as "no guard";
   there is no supported way to turn the inactivity guard off.
8. **Glob spelling is normalized, or exclusions silently exclude nothing.** Any new
   pattern list must pass through `canonical_pattern_prefix` (`:190-214`), because
   matching is against canonical paths (`src/discovery.rs:38-45`).
9. **Unknown keys are errors.** `deny_unknown_fields` on all four structs
   (`:12`, `:20`, `:28`, `:39`) means no field can ship without a struct change.
10. **Glob compilation happens at load, not at use** (`:133-149`), so
    `config edit` fails where the next run would.

**Adding a config field** — add it to the right table in `config.toml` with the
default spelled out and a comment stating what it does *and* what it deliberately
does not do; add the field, type and `#[serde(default = …)]` to the struct; add a
`default_*` fn or extend `impl Default for ScanConfig` (`:51-67`) if the default
is non-zero; add the validation in `load` in the existing order; confirm
`config show` surfaces the default (an `Option` that is `None` is omitted);
extend `checked_in_template_parses_and_matches_defaults` (`:452`) so template ↔
default drift is caught, and add a rejection test in the style of
`recency_seconds_of_zero_is_rejected` (`:383`);
`scripts/check-release-contract.py` needs no change for a field but must stay
aware of `/config.toml` — a *new* compile-time input file would need a new
allowlist entry and an assertion in the style of `:460-464`. If the field is
consumed elsewhere, update that consumer (`src/policy.rs`, `src/discovery.rs`,
`src/cleanup.rs`): this module enforces nothing about the field's effect.

**Edge cases**

| Case | Behaviour |
|---|---|
| `--config` points at a directory | `load` reads it and fails `cannot read …`; creation refuses to replace a non-file (`src/config.rs:540-547`) |
| `--config` with no file name | `file_name()` yields the last component; the fallback name is literally `config.toml` (`:257-260`) |
| Config file is not valid UTF-8 | `fs::read_to_string` fails → `cannot read …` (`:90-91`) |
| Non-UTF-8 directory name during glob matching | Matched through `to_string_lossy` in discovery, so a broad `*` cannot be escaped (`src/discovery.rs:30-45`) |
| Symlinked config file | `path.exists()` follows symlinks, so creation is skipped and the target's contents load |

## 8. Testing

`src/config.rs` has **19** `#[test]` functions. The module starts at
`src/config.rs:561` (`#[cfg(test)]`) / `:562` (`mod tests`); production code ends
at `:559`. Three are `#[cfg(unix)]`-gated (`:587`, `:673`, `:885`), one is
`#[cfg(windows)]` (`:928`), and one is
`#[cfg(all(target_os = "linux", target_arch = "x86_64"))]` (`:1352`), so a
Windows build compiles 15 and a macOS build 18.

| Test | Line | What it pins |
|---|---|---|
| `patterns_are_canonicalized_so_exclusions_actually_apply` | `:588` | The symlink-spelling regression for `ignore` and `exclude`; that a metacharacter-free pattern and a non-resolving pattern keep their spelling; that a relative pattern is never resolved against the CWD. Unix-only. |
| `a_canonical_spelling_cannot_inject_glob_characters` | `:674` | A bracketed canonical name is spliced escaped, so the named tree still matches and two siblings do not; `unignore` splices unescaped; a `{` spelling is left untouched. Unix-only, and its `#[cfg(unix)]` is the honest gate — see §2. |
| `a_spliced_canonical_spelling_matches_the_directory_it_names` | `:756` | The spliced spelling itself: `/`-spelled, brackets escaped, named tree matched and siblings not. Asserts the *premise* in the same test — the unescaped reading misses the named tree and matches a sibling — so the assertions cannot be decoration. Then the same splice with the Windows answer, asserted on every lane. |
| `a_backslash_in_a_canonical_directory_name_is_spliced_as_a_escape` | `:887` | The one remaining `escape_glob_literal` input on Unix: a `\` in a canonical file name is doubled, compiles to a literal `\`, and matches the directory it names. Unix-only — no Windows file name may contain one. |
| `a_bracketed_canonical_spelling_is_matched_literally_on_windows` | `:930` | The whole C024 chain on the platform that motivated it: a real canonical spelling, a real bracket in a real file name, a junction alias, and a `config.toml` read through `load`. Windows-only, because a Windows symlink needs elevation a hosted runner does not grant. |
| `recency_seconds_of_zero_is_rejected` | `:1010` | `0` is an error naming the inactivity guard; `1` is accepted. |
| `out_of_range_retention_reports_a_range_not_a_parse_error` | `:1024` | `99999` reports "must be between 0 and 3650" rather than a TOML type error; `7` is accepted. |
| `invalid_scan_ignore_glob_is_reported_at_load` | `:1040` | A malformed `scan.ignore` glob fails at load. The comment at `:1038-1043` is a premise guard: the pattern is made absolute so the test cannot pass for the wrong reason. |
| `default_recency_and_absent_config` | `:1058` | An absent file loads as all defaults with `recency_seconds = 300`. |
| `malformed_and_relative_paths_fail` | `:1064` | A non-numeric value, a relative `scan.root`, and a relative `allowed_output_roots` each fail. |
| `checked_in_template_parses_and_matches_defaults` | `:1079` | The template parses and agrees with `Config::default()` on every field it spells out. |
| `config_template_matches_repository_file` | `:1098` | `CONFIG_TEMPLATE` is byte-equal to the checked-in `config.toml`. |
| `legacy_cleanup_config_loads_with_neutral_policy` | `:1107` | A `[cleanup]`-only file yields a neutral policy: `0`, `None`, empty lists. |
| `cleanup_policy_globs_and_durations_are_validated` | `:1115` | `include = ["["]` and `min_inactive_seconds = u64::MAX` both fail. |
| `operational_bootstrap_refuses_overwrite` | `:1128` | `load_or_create` creates once, then preserves an edited `recency_seconds = 61`. |
| `bootstrap_bytes_equal_checked_in_template` | `:1137` | The created file is byte-equal to `CONFIG_TEMPLATE` and loads as defaults. |
| `malformed_config_is_not_replaced_automatically` | `:1148` | A malformed file errors *and* is left byte-identical on disk. |
| `concurrent_first_use_creates_one_complete_template` | `:1160` | 16 workers over 24 rounds, a fresh nested path each round; every racer succeeds and the bytes are one complete template. Strengthened deliberately — a single 8-thread round missed the Windows race most of the time. |
| `the_config_is_published_on_a_filesystem_without_hard_links` | `:1354` | The `create_new` fallback publishes a complete config where `link(2)` reports "unsupported". |

`config.toml:10` tells contributors to run `cargo test config_template` after
editing the template. That substring matches
`checked_in_template_parses_and_matches_defaults`,
`config_template_matches_repository_file` and
`bootstrap_bytes_equal_checked_in_template` — together covering "template
parses", "template matches the repository file", and "the file we write is the
template".

`src/editor.rs` **does** have inline tests — 2 of them, module starting at
`src/editor.rs:156`: `parser_preserves_arguments_and_windows_backslashes`
(`:160`), which checks `split_words("code --wait 'path with spaces'")` →
`["code", "--wait", "path with spaces"]`, a quoted Windows path surviving
backslashes, and an unfinished quote erroring; and
`unresolvable_visual_falls_through_to_the_next_candidate` (`:173`), where an
unusable `VISUAL` falls through to a usable `EDITOR` with an empty argument list.
The comment at `:179-184` is a premise guard: the fixture is never executed, and
the test asserts resolution only, never a spawn. Integration coverage lives
outside these files, in `tests/cli_contract.rs:582-658` (fake editor process,
three failure modes) and `tests/cli_contract.rs:161`/`:171` (the
`config edit --help` surface under both argv spellings).

## 9. Review checklist

1. Did any change make the repository `config.toml` loadable at runtime (a CWD
   lookup, a bare `config.toml` path)? The only legitimate readers are
   `CONFIG_TEMPLATE` (`src/config.rs:12`) and its test comparison (`:1098`).
2. Does a new or moved field still match between the struct, its serde default,
   `impl Default for ScanConfig` (`:53-63`) and the template (`config.toml:12-45`)?
   `checked_in_template_parses_and_matches_defaults` (`:1079`) is the guard.
3. Is `/config.toml` still in the `include` allowlist (`Cargo.toml:24`), given
   `check-release-contract.py:460-464`?
4. Does any new validation live outside `load` (`:90-205`), where `config edit`
   would not see it and the failure would be deferred to the next scan (the rule
   at `:135-136`)?
5. Is `recency_seconds == 0` still rejected (`:99-103`) and
   `learned_root_retention_days` still bounded at 3650 (`:107-111`) — the bound
   `src/policy.rs:46`'s `as u16` depends on?
6. Do new pattern lists pass through `canonical_pattern_prefix` (`:323-366`,
   applied at `:154-186`)? A list that skips it excludes nothing.
7. Does anything still create, link over, or truncate an existing config?
   Publication is `create_new` + `fs::hard_link` (`:478-517`) and
   `load_or_create` acts only when the file is absent (`:368-373`).
8. Are the three post-edit checks still present and still ordered non-zero-exit
   → deletion → validity (`src/main.rs:59-79`)? Reordering them changes which
   failure is reported for a broken editor.
9. Is `resolve_editor`'s candidate loop still fall-through rather than fail-fast
   (`src/editor.rs:8-27`), and is the program still separated from its arguments
   in the return type (`:7`)? A single-string return regresses `code --wait`.
10. Does `config show` still print the normalized projection rather than the file
    bytes (`src/config.rs:557-559`)? That difference is the only visible way
    canonicalized patterns show up.
