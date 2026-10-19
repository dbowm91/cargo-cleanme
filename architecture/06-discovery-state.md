# Discovery State — learned roots, uncertainty, and safe publication

> Component deep dive · part of the [architecture overview](overview.md)

Subject: `src/discovery_state.rs` (1129 lines, of which 549 are inline tests —
17 tests).

## 1. Responsibility

`discovery_state.rs` owns two things and is the only place in the crate that
does:

1. **The persisted learned-root state file** — its schema, its location, its
   read classification, and its write mechanics.
2. **The rules for changing that state** — specifically `reconcile_full`, the
   function that decides whether a Full scan's evidence is strong enough to
   overwrite a previous generation.

What it does **not** own, and this boundary is the point of the module:

- It discovers nothing. It never walks a filesystem to find manifests. It
  receives an already-computed `&[ProjectObservation]` and applies arithmetic to
  it.
- It makes no safety decision about deletion. A learned root answers *where to
  search*; it is never evidence of ownership, recency, or authorization. This is
  ADR 002 §8 (`plans/adr/002-adaptive-routine-full-discovery-state.md:100-113`)
  and it is enforced structurally, not by convention: the module contains no
  reference to `cleanup`, `OutputOwnershipClass`, or any authorization type. It
  decides what is **remembered**, not what is **removable**.

### The leaf property — no longer true, and the exception is narrow

An earlier revision of this document claimed the module was a leaf: no
`crate::` import, no `super::` reference outside its own test module (which
uses `use super::*;` at `:583`). **That claim is now false.** Since C030 the
module calls into a sibling:

```rust
if let Some(cargo_home) = crate::discovery::effective_cargo_home() {   // :143
```

That is the module's **only** crate-internal reference, and it is a single
path-qualified call — not an `use` statement — to one pure function
(`discovery.rs:164`) that reads `CARGO_HOME` or the OS home directory.

The dependency is also now **cyclic**, and the cycle involves three modules:

```text
discovery_state.rs:143  →  crate::discovery::effective_cargo_home()
discovery.rs:337        →  crate::policy::global_discovery_policy()
policy.rs:238, policy.rs:263, policy.rs:325, policy.rs:529
                          →  crate::discovery_state::automatic_root_is_maintainable()
```

All three edges are production (the test modules of `discovery.rs` and
`policy.rs` start at `discovery.rs:1001` and `policy.rs:703`, far below those
lines). So the production import graph contains
`discovery_state → discovery → policy → discovery_state`, a three-module cycle
that did not exist when this document was written. The
[overview](overview.md) §5.2 now records **two** production cycles — the
`discovery ↔ policy` pair and the three-module
`discovery → policy → discovery_state → discovery` that shares its edges.

What the module still does **not** depend on is the machinery that could bias
what it remembers: no `cleanup`, no `workspace`, no `output`, no `policy`. The
C030 dependency on `discovery` is a *location* lookup (where Cargo's home is),
not a discovery call — `effective_cargo_home` walks nothing.

The module's import block is still three external crates plus `std` (`:1-9`):

| Import | Line | Use |
|---|---|---|
| `directories::ProjectDirs` | `:1` | `state_path()` |
| `serde::{Deserialize, Serialize}` | `:2` | state record derives |
| `serde_json` (path-qualified) | `:442`, `:461`, `:484` | version pre-check, decode, encode |
| `windows_sys::…::MoveFileExW` | `:562-564` | `#[cfg(windows)]` atomic replace |
| `std::{collections, fs, io::Write, path, time}` | `:3-9` | — |

The module is not pure either — `fs::canonicalize` (`:120`, `:263`),
`fs::read` (`:429`), `fs::create_dir_all` (`:482`), `fs::OpenOptions` (`:543`)
and the rename family (`:557`, `:560-578`) all perform real I/O. It is a
near-leaf in the *dependency* graph, not a pure function module.

## 2. The on-disk schema

`CURRENT_SCHEMA: u32 = 2` (`:11`).

### `DiscoveryState` (`:63-70`)

```rust
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryState {
    pub schema_version: u32,
    pub last_full_at: Option<u64>,
    pub projects: Vec<ProjectRecord>,
    pub learned_roots: Vec<LearnedRoot>,
}
```

| Field | Type | Meaning | When written |
|---|---|---|---|
| `schema_version` | `u32` | Format generation. `0` is treated as invalid on read (`:465-466`). | Every `reconcile_full` publish (`:225`); normalized in memory on every successful load (`:468-469`); passthrough on a Routine touch. |
| `last_full_at` | `Option<u64>` | The generation stamp: wall-clock seconds of the newest **complete** Full reconciliation that published. `None` = never reconciled. | `reconcile_full` only (`:226`). Never written by the Routine/Explicit touch branch. |
| `projects` | `Vec<ProjectRecord>` | Exact project inventory from the last complete Full scan. Fully replaced, never merged. | `reconcile_full` only (`:227-242`). |
| `learned_roots` | `Vec<LearnedRoot>` | Containers the Routine scan should search, each with a positive-observation stamp. | `reconcile_full` (`:243-357`), and `last_project_seen_at` only via the Routine touch in `main.rs:1148-1163`. |

`#[serde(default)]` sits on the **struct** (`:64`), so all four fields may be
absent from the JSON and decode to `Default`. That is what lets a v1 file with
only `{"schema_version":1,"last_full_at":12,"projects":[…],"learned_roots":[]}`
load at all.

### `ProjectRecord` (`:71-81`)

| Field | Type | Meaning | When written |
|---|---|---|---|
| `workspace` | `PathBuf` | Workspace identity when Cargo resolution succeeded, otherwise the manifest's parent directory (doc comment `:73`). **Required** — no `#[serde(default)]`. | `reconcile_full` (`:230-233`) |
| `manifest` | `Option<PathBuf>` | The observed `Cargo.toml` path. `#[serde(default)]` (`:75-76`). | `reconcile_full`, always `Some` (`:234`) |
| `resolution` | `ResolutionStatus` | Whether Cargo answered. `#[serde(default)]` (`:77-78`). | `reconcile_full` (`:235-239`) |
| `observed_at` | `u64` | The generation stamp of the scan that produced this record. `#[serde(default)]` (`:79-80`), so v1 records decode as `0`. | `reconcile_full` (`:240`) |

Note that the `projects` array is written but **never read back by any consumer
in the crate**. Grepping `\.projects` across `src/` outside this module returns
nothing. It is inventory retained for the ADR's "exact project inventory"
concept (ADR §4, `:57-68`); the Routine path uses `learned_roots` only.

### `ResolutionStatus` (`:82-88`)

`#[serde(rename_all = "snake_case")]` + `#[default] Resolved`. On disk the two
variants are `"resolved"` and `"manifest_observed_unresolved"`.

### `LearnedRoot` (`:89-93`)

| Field | Type | Meaning | When written |
|---|---|---|---|
| `path` | `PathBuf` | Canonicalized search container. **Required.** | `reconcile_full` (`:263-267`) |
| `last_project_seen_at` | `u64` | Positive-observation stamp, seconds. **Required.** | `reconcile_full` (`:266`); advanced by the Routine touch |

Since C030, only observations with a successfully resolved Cargo workspace can
create a new `LearnedRoot`. The exact manifest inventory still records
unresolved manifests. The rule is `automatic_root_is_maintainable` (`:114-207`)
— a **scope-hygiene** rule, not an ownership rule, and it is applied in three
distinct places:

- **Fresh candidates** (`:248` on the resolved project, `:260` on the promoted
  container): a non-maintainable location never becomes a learned root.
- **Legacy learned roots** (`:298-300`): a known-disallowed root is removed on
  a complete Full reconciliation, but only after the uncertainty veto at
  `:288-294` has had its chance — an uncertain legacy root is retained even when
  its provenance is known-bad.
- **Routine reads** (`policy.rs:529`) and **cleanup admission**
  (`policy.rs:238`, `policy.rs:263`): the same check filters legacy learned
  roots out of automatic Routine use and decides whether an automatic cleanup
  root is admitted, omitted, or blocked.

Recognizable package caches (including macOS `~/Library/Caches` and pnpm
stores), Trash, temporary locations (platform temp root plus the `/tmp`,
`/private/tmp`, `/var/tmp` aliases), system software locations (`/usr/local`,
`/opt`), the `~/Downloads` drop zone, `node_modules`, and transient Codex
worktrees are not promoted into Routine scope. Entries are compared by both
canonical and raw spelling (`:185-197`), so a missing candidate spelled through
a symlink (e.g. `/tmp` → `/private/tmp` on macOS) is still recognised. Two
details of the implementation are load-bearing and easy to misread:

- **A symlink is *maintainable* by construction** (`:117-119`). The function
  answers "is this a durable developer location?", and a symlink's identity is
  not a location claim the function is willing to make. It returns `true` so
  the symlink is *not* silently dropped; blocking it is the admission layer's
  job (`policy.rs:229-236`).
- **The temp-family exemption is conditional** (`:162-178`). When the resolved
  home sits under the canonical temp root — the normal shape of a test harness
  home — the whole temp tree is fixture namespace and the temp entries do not
  apply. In real usage the home is never under the temp root.

Unknown coverage still prevents publication, and an uncertainty intersecting a
legacy root preserves it.

`LearnedRoot` carries **no** `#[serde(default)]` on either field. An entry
missing either one makes the whole file `RecoverableInvalid` — a strictness the
`DiscoveryState` struct-level default does not extend to array elements.

### Migration policy — read the code, do not assume

Three distinct outcomes, and only one of them is "migrate":

| On-disk `schema_version` | Outcome | Behaviour |
|---|---|---|
| `> 2` | `StateLoad::UnsupportedNewer` (`:448-454`) | **Rejected.** Never overwritten, never used as a prior (`full_reconciliation_prior` returns `None`, `:59`). Matches ADR §7 (`:98`). |
| `1` | `StateLoad::Loaded`, silently upgraded | **Migrated in memory only.** `:469` sets `state.schema_version = CURRENT_SCHEMA` in the loaded value; the file on disk is unchanged until some `publish` rewrites it. |
| `0`, or `schema_version` absent | `StateLoad::RecoverableInvalid("invalid schema 0")` (`:465-466`) | **Recoverable.** An absent key decodes to `0` through the struct-level `#[serde(default)]`, so it lands on the *same* branch as a literal `0` — not on the serde-message branch below. |
| non-numeric, or undecodable | `StateLoad::RecoverableInvalid(serde message)` (`:461-463`) | **Recoverable.** |
| structurally invalid at the current version | `StateLoad::RecoverableInvalid(serde message)` (`:461-463`) | **Recoverable.** |

The 1 → 2 bump is **semantic, not structural**. There is no field in `ProjectRecord`
that a v1 file could carry and a v2 file could not. What changed is meaning: a v1
record deserialized without `resolution` defaults to `Resolved` (`:85-86`), and
`load_at` treats that default as authoritative — the comment at `:468` says so
("v1 records deserialize with defaults as authoritative resolved projects"). The
discriminating marker on disk is therefore the *presence* of
`"resolution": "manifest_observed_unresolved"`, not any layout difference.

A negative or float `schema_version` is a genuinely under-specified path: the
pre-check's `as_u64` returns `None` (`:444-445`), so the file falls through to the
typed decode, which fails, and the resulting message is serde's, **not**
`"invalid schema 0"`. Only a literal `0` — or an absent key — gets the clear
message.

## 3. Loading

### `StateLoad` (`:19-30`)

| Variant | Meaning | May be overwritten? |
|---|---|---|
| `Missing` | No file, or no locatable state directory. | Yes — treated as first run. |
| `Loaded(DiscoveryState)` | Supported schema, structurally valid. | Yes. |
| `RecoverableInvalid(StateProblem::Invalid)` | Present but unusable: bad JSON, schema 0, or structural mismatch. | Yes, **but only** by a *complete* Full reconciliation. |
| `UnsupportedNewer { path, schema, current }` | Written by a newer binary. | **Never.** |
| `Unavailable(StateProblem::Unavailable)` | I/O failure that is not `NotFound` (e.g. the path is a directory, or EACCES). | **Never.** |

`StateProblem` (`:13-17`) is the payload pair `Invalid` / `Unavailable`, and
each carries a `path` and a `detail` string (`:15-16`) — the `detail` is what
the diagnostic prints. The pairing with `StateLoad` variants is exact by
construction: `RecoverableInvalid` only ever carries `Invalid` (`:455-460`),
`Unavailable` only ever carries `Unavailable` (`:432-437`).

### The load-bearing distinction

**"No state exists" and "state exists but is unreadable" are different outcomes
and the difference decides whether bytes are destroyed.**

Both `Missing` and `Unavailable` fail to produce a usable prior, but only
`Missing` and `RecoverableInvalid` may be replaced. `full_reconciliation_prior`
(`:54-61`) encodes the whole policy in five lines:

| `StateLoad` | Returns | Replaces an existing file? |
|---|---|---|
| `Loaded(s)` | `Some((s.clone(), false))` | Yes |
| `Missing` | `Some((DiscoveryState::default(), false))` | Nothing to replace |
| `RecoverableInvalid(_)` | `Some((default(), true))` | **Yes** — deliberately |
| `UnsupportedNewer` | `None` | **No** |
| `Unavailable(_)` | `None` | **No** |

The boolean is `replacing_invalid`. `main.rs:1123-1128` uses it to print
"replaced unusable discovery state after successful Full reconciliation" — an
operator-visible acknowledgement that stale bytes were clobbered, so the
replacement is never silent.

`Unavailable` and `UnsupportedNewer` returning `None` is the fail-closed half of
the design: an unreadable file and a future-schema file are both treated as
**unknown**, and unknown is never a licence to write. `main.rs:1141-1147` turns
that `None` into a stderr warning and a skipped reconciliation.

### `load_default` (`:422-427`)

`state_path()?` → if `None`, return `StateLoad::Missing`. Note this collapses
"the OS gave us no home directory" into `Missing`, i.e. fail-soft first-run
behaviour, not `Unavailable`. Everything else delegates to `load_at`.

### `load_at` (`:428-471`) — parse order

1. `fs::read`. `NotFound` → `Missing` (`:431`). Any other error → `Unavailable`
   with the OS message (`:432-437`).
2. **Version pre-check before typed decode** (`:442-454`). The bytes are parsed
   as `serde_json::Value`; only if the top level is an `Object` with a numeric
   `schema_version > 2` does it return `UnsupportedNewer`. The comment at
   `:439-441` states the intent: a newer writer "may have added fields or
   changed record structure that this binary must leave untouched." Checking
   before decoding is what makes that promise meaningful — a v3 file whose
   `LearnedRoot` shape this binary cannot parse is still recognised and
   preserved rather than mangled into `RecoverableInvalid` and then replaced.
3. Typed decode into `DiscoveryState`; failure → `RecoverableInvalid` with
   serde's message (`:461-463`).
4. `schema_version == 0` → `RecoverableInvalid("invalid schema 0")` (`:465-466`).
5. Normalize `schema_version` to `CURRENT_SCHEMA` in memory; return `Loaded`
   (`:468-470`).

### serde strictness — what is actually there

**There is no `#[serde(deny_unknown_fields)]` anywhere in this file** (verified
by grep over the whole module). Unknown fields are tolerated and silently
dropped.

That is a deliberate and *sufficient* design here, but only because step 2
carries the forward-compatibility load. The v1→v2 test at `:732` and the
structural test at `:773` both rely on tolerance: `{"schema_version":2,
"projects":[{"not_workspace":"/work"}]}` is invalid not because `not_workspace`
is unknown, but because the **required** `workspace` field is missing
(`:74` has no `#[serde(default)]`).

The honest limit of that tolerance: a future writer that bumped nothing but
changed `LearnedRoot`'s shape would be silently accepted by this binary. The
version number is the only forward-compat mechanism; strictness is not a
backstop.

### `state_path` (`:411-418`) and what `None` means

```rust
let dirs = ProjectDirs::from("", "", "cargo-cleanme")?;          // :412
Some(
    dirs.state_dir()
        .or_else(|| Some(dirs.data_local_dir()))?                // :414-415
        .join("discovery-state.json"),
)
```

It returns `Option<PathBuf>`, and the two `?` operators have **different**
reasons. Worth stating precisely because the intuitive reading is wrong:

- `state_dir()` is `None` **on macOS and Windows** — the `directories` 6.0.0
  docs table lists state_dir as `–` for both (`src/lib.rs:520-521`). That is
  exactly the case the `.or_else(|| Some(dirs.data_local_dir()))` fallback
  exists for. The `or_else` arm is `Some(..)`, so it can never produce `None`;
  that `?` is dead in practice.
- Therefore the **only** live source of `None` is the first `?`: `ProjectDirs::from`
  returning `None` when the OS yields no home directory (`directories`
  `src/lib.rs:405-412`). That is the condition to name in a bug report.

Concretely, with an empty qualifier and organization, the file is
`$XDG_STATE_HOME/cargo-cleanme/discovery-state.json` (or
`~/.local/state/cargo-cleanme/…`) on Linux,
`~/Library/Application Support/cargo-cleanme/discovery-state.json` on macOS, and
`%LOCALAPPDATA%\cargo-cleanme\discovery-state.json` on Windows. This matches
ADR §7 (`:90-98`), which names `ProjectDirs::state_dir()` with
`data_local_dir()` as the fallback.

### `now_seconds` (`:405-410`)

`SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()`.
A pre-epoch clock yields `0` rather than panicking. The caller supplies this
value; the module never reads the clock itself inside `reconcile_full`, which is
what makes the reconciliation unit-testable with a synthetic `now`.

### `StateLoad::diagnostic` (`:33-49`)

Returns `Option<String>`: `None` for `Missing` and `Loaded` (both are normal
outcomes that need no explanation), and a distinct sentence per failure —
"ignoring unusable discovery state …" / "… uses unsupported schema N; update
cargo-cleanme" / "cannot read discovery state …". It is consumed by
`main.rs:1144` and by `policy.rs:492` and `policy.rs:539`.

## 4. Reconciliation

### `reconcile_full` — signature and inputs (`:211-219`)

```rust
pub fn reconcile_full(
    prior: &DiscoveryState,
    observations: &[ProjectObservation],
    uncertainty: &[PathBuf],
    complete: bool,
    now: u64,
    retention_days: u16,
    home: Option<&Path>,
) -> Reconciliation
```

| Parameter | Contributes |
|---|---|
| `prior` | The generation being superseded: old `learned_roots` (retain-or-drop candidates) and `last_full_at` (a monotonic floor on `now`). |
| `observations` | One entry per discovered `Cargo.toml` (`ProjectObservation`, `:95-99`: `manifest` + `workspace: Option<PathBuf>`). Drives both `projects` and the fresh learned-root candidates. |
| `uncertainty` | Paths the traversal could not read. A learned root is retained if any of these intersects it. |
| `complete` | The single global gate. `false` → immediate `NoPublication`. |
| `now` | Wall-clock seconds; the new generation stamp. |
| `retention_days` | Age budget. `0` disables expiration entirely (`:319`). |
| `home` | The user's home directory. Suppresses promotion to `$HOME` itself as a learned root (`:252`) and supplies the forbidden-location list for C030 (`:122-141`). |

### The `Reconciliation` decision

`Reconciliation` (`:101-105`) is `Publish(DiscoveryState)` or
`NoPublication(&'static str)`. There is exactly **one** production
`NoPublication` construction site in the whole module:

```rust
if !complete {
    return Reconciliation::NoPublication("Full traversal was incomplete");   // :220-222
}
```

`complete == false` is the *only* condition. Uncertainty, an empty
observation set, a corrupt prior, and retention pressure all still publish.
The `&'static str` is a borrowed literal, so a caller cannot allocate a
reason string here; adding a second refusal reason means touching the enum.

### Step-by-step

**1. Gate (`:220-222`).** `!complete` → `NoPublication`. Nothing is mutated;
`prior` is only read.

**2. Clone and stamp (`:223-226`).** `next = prior.clone()`, then
`observed_at = now.max(prior.last_full_at.unwrap_or(0))`, `schema_version =
CURRENT_SCHEMA`, `last_full_at = Some(observed_at)`. Every one of the four
`DiscoveryState` fields is subsequently overwritten, so `next` is entirely
derived from `observations` plus the surviving old roots — the clone is
bookkeeping, not carry-over.

The `max` at `:224` is the clock-regression guard: the generation stamp never
goes backwards, which is what makes the future-timestamp retention rule at
`:318` meaningful.

**3. Project records (`:227-242`).** One `ProjectRecord` per observation.
`workspace` is `o.workspace` if present, else `o.manifest.parent()`, else the
manifest path itself (`:230-233`). `resolution` is `Resolved` iff
`o.workspace.is_some()` (`:235-239`). `manifest` is always `Some`.

**4. Fresh learned-root candidates (`:244-269`).** Per observation, `project` =
`o.workspace`. Then, in order:

```rust
if let Some(project) = o.workspace.as_deref()                      // :247
    && automatic_root_is_maintainable(project, home)                // :248
{
    let candidate = project
        .parent()
        .filter(|p| Some(*p) != home && !is_broad_root(p))          // :252
        .unwrap_or(project);                                        // :253
    if is_broad_root(candidate) { continue; }                       // :257-259
    if !automatic_root_is_maintainable(candidate, home) { continue; }// :260-262
    let path = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf()); // :263
    …
}
```

Five facts, in the order the code applies them:

- **Only Cargo-resolved workspaces seed Routine roots** (`:247-248`). A raw
  manifest is positive inventory, not proof of a maintainable project location;
  the comment at `:245-246` says so. An unresolved manifest still appears in
  `projects`, but never in `learned_roots`.
- **The project root's direct parent is the search container** — ADR §4's
  "candidate learned search container, normally the workspace root's direct
  parent" (`:62-64`). The `.unwrap_or(project)` is the refusal case: when
  promoting to the parent would be too broad, the project root itself is
  retained instead of the parent (ADR §2, `:47`, and §4, `:64`).
- **M-7's post-fallback recheck** (`:257-259`). The `unwrap_or(project)` fallback
  can yield `/` itself (parent `None`); `is_broad_root("/")` is true but was
  never re-tested on the fallback path. Broad roots are withheld from learning.
- **C030's second check** (`:260-262`). The promoted *container* is checked
  separately from the *project*. **This check cannot independently fire today**,
  and that is worth knowing rather than assuming. The forbidden-location test
  (`:185-197`) is `starts_with`-based in both the canonical and the raw
  spelling, and the `node_modules` component test (`:200-205`) also propagates
  downward; since `candidate` is either `project` itself or `project`'s parent,
  and `canonical(project)` always lies under `canonical(candidate)`, a
  non-maintainable container implies a non-maintainable project. The one escape
  — a symlink between the two — is neutralised by the symlink early return at
  `:117-119`, which makes a symlink *maintainable*. So `:260-262` is
  **defence in depth**: it would start to matter if the forbidden list ever
  gained an exact-match entry, or if the comparison stopped being monotone.
  Removing it would not change today's behaviour; keeping it means a future
  change to the list has a second gate.
- **Canonicalization is best-effort** (`:263`): on failure the raw spelling is
  stored, which is the only way a never-yet-created path can become a learned
  root.

**5. The `covered` ancestor index (`:274-283`).** For every fresh root, walk
upward inserting each ancestor into a `HashSet`, stopping at the first
already-indexed ancestor. `covered` answers "is there a fresh root at or below
this path?" in one lookup instead of a scan per old root. The comment at
`:270-273` justifies the early `break` (`:279`): if an ancestor is already
indexed, its own ancestors are too, so the rest of the walk is redundant.

**6. Old roots: five rules, in a fixed order (`:284-325`).** For each `old` in
`prior.learned_roots`, the rules are applied in this sequence, and the order is
the single most consequential detail in the function:

| # | Rule | Lines | Effect on `old` |
|---|---|---|---|
| 1 | **Covered** — a fresh root exists at or below `old.path` | `:285-287` | **Dropped.** `continue`, before anything else is consulted. |
| 2 | **Uncertainty** — any uncertain path intersects `old.path` | `:288-294` | **Retained** (pushed verbatim), `continue`. |
| 3 | **C030 provenance** — `automatic_root_is_maintainable(&old.path, home)` is false | `:298-300` | **Dropped**, independent of retention. Access failure alone is never negative evidence; only a *recognized* non-maintainable location is. |
| 4 | **C028 absence** — `symlink_metadata` fails with exactly `ErrorKind::NotFound`, and no uncertain path intersects | `:308-317` | **Dropped**, independent of retention. |
| 5 | **Retention** — `old.last_project_seen_at > observed_at`, or `retention_days == 0`, or the age is within budget | `:318-324` | **Retained**, else dropped. |

Two qualifications on rule 4, both load-bearing:

- It requires **no intersecting uncertainty** (`:308-310`). That precondition
  is redundant in production — see §5 — but it is the mechanism that makes the
  rule safe in principle.
- It uses `symlink_metadata` and matches **only** `ErrorKind::NotFound`
  (`:311-314`) — never `Path::exists`, which conflates errors. "Exists but no
  project was found" falls through to rule 5; symlinks, regular files,
  permission failures, and indeterminate errors are never positive absence, so
  they keep the old retention behaviour.

Rule 3 sits *after* rule 2 deliberately: an old root that both intersects
uncertainty and sits at a known-bad location is **retained** (rule 2 wins),
because uncertainty means "this scan could not judge it".

Retention arithmetic: `u64::from(retention_days) * 86400` is overflow-free
because `config::load` caps the value at 3650 (`config.rs:107-110`), giving at
most 315,360,000 — which is also what makes `policy.rs:46`'s `as u16` cast
lossless. `saturating_sub` means a future `old.last_project_seen_at` cannot
wrap; it is caught by the explicit `>` guard at `:318` first anyway. This is the
ADR §5 rule "Future timestamps fail conservative: they do not cause expiration"
(`:80`) implemented literally. The C028 absence probe is deliberately *not*
arithmetic: it distinguishes "physically absent" from "present but
unobserved", and only the former bypasses retention.

**7. Sort and containment-collapse (`:326-356`).** `learned` is sorted by path;
since a parent always sorts before its children, every possible parent is
already represented when its child is reached. Each root is then folded into its
nearest already-represented ancestor, taking the **max** of the two
`last_project_seen_at` values (`:347-349`). The walk starts at the root itself
rather than its parent (`:337`), so an exact duplicate folds too — a workspace
with N members contributes N observations that all resolve to one root. Note the
direction: collapse **prefers the ancestor**, keeping the broader container.
This is the opposite direction from the `covered` skip in rule 1, which prefers
the descendant. The two rules pull in opposite directions and both are
observable.

**8. Publish decision (`:357-358`).** `next.learned_roots = collapsed;
Reconciliation::Publish(next)`.

### `full_reconciliation_prior` (`:54-61`) — why it is a separate step

It is a separate `pub` function because it answers a question `reconcile_full`
deliberately cannot: *is it safe to overwrite these particular bytes?*
`reconcile_full` is pure over a `DiscoveryState` and has no idea whether that
state came from a good file, a corrupt one, or a newer binary. Overwrite safety
is a property of the **load**, not the **reconciliation**, and only the caller
holds both facts.

`Option<(DiscoveryState, bool)>` splits the two things a caller needs: the state
(reconciliation input, or a fresh default for `Missing`/`RecoverableInvalid`),
and a boolean reporting whether that state *replaces* something unusable — a
purely operator-communication concern, used at `main.rs:1123-1128` and nowhere
else. Because the gate lives outside, a caller that forgets to consult it cannot
silently clobber a newer binary's file. The one in-tree reconciliation caller
does consult it (`main.rs:1080`); `policy.rs` never reconciles.

### How `main.rs` supplies the two gate inputs

`uncertainty` — `main.rs:1041-1053`: `discovered.diagnostics` filtered to
`DiagnosticCategory::PermissionDenied | Metadata | Vanished`, then
`filter_map(|d| d.path.clone())`. `InvalidEntry`, `PlatformRoot`, and
`CandidateUncertain` diagnostics are **excluded** and play no part in
reconciliation. A diagnostic with `path: None` is silently dropped, which is
fail-open for that entry — see §5, where this turns out to be load-bearing.

`complete` — `main.rs:1032-1036`:
`discovered.coverage_complete && !discovered.diagnostics.iter().any(|d| d.category == PlatformRoot && d.severity == Error)`.
Only an `Error`-severity `PlatformRoot` diagnostic, or an incomplete traversal,
makes a Full scan incomplete. Everything else — including every
`Warning`-severity diagnostic — leaves the scan publishable. `main.rs:1198`
recomputes the identical predicate into `full_incomplete` for the exit code;
the duplication is redundant but harmless.

## 5. Uncertainty handling

### `uncertainty_intersects` (`:107-109`)

```rust
root == uncertain || root.starts_with(uncertain) || uncertain.starts_with(root)
```

| Property | Value |
|---|---|
| Semantics | Symmetric path containment on **components**, not on string prefixes. `/work/a` does not match `/workspace/a` (tested at `:1029`). |
| Canonicalization | **None.** It is pure `Path` arithmetic; it never touches the filesystem. |
| Symmetry | Explicitly both directions, tested at `:1028-1036`. |

Symmetry is the design choice. The two operands are not peers: `root` is a
learned container to be retained, `uncertain` is a location the traversal could
not read. A learned root is retained when the unreadable region is **inside it**
(it cannot prove the root is empty) *or* when the learned root is **inside the
unreadable region** (a learned root discovered inside a region we cannot read
should not be judged by this scan). One-directional `starts_with` would cover
only the first case and would let the second prune real roots.

### Why one unreadable subtree must block a larger tree

Suppose `/data` was learned as a container and this scan read `/data/team-a`
fine but was refused on `/data/team-b`. Absence of a `Cargo.toml` under
`/data/team-b` is not evidence that none exists — it is evidence that the scan
could not look. Dropping `/data` would convert a read failure into a negative
observation, which is precisely the class of error the module exists to prevent
(ADR §6, `:86`: "absence beneath that uncertain coverage is not proof that the
root is empty of Rust projects").

### What "conservatively" concretely means here

**It publishes, and refuses to drop the intersecting roots.** It does **not**
block publication. There is no code path in which a non-empty `uncertainty`
yields `NoPublication`; the only `NoPublication` is the `!complete` gate at
`:220-222`. When `complete == true` and `uncertainty` is non-empty, the result
is a full `Publish` including every positive observation, with the intersecting
old roots carried forward (`:292`). This matches the doc comment at `:209-210`
("Reconcile only after a complete Full traversal. Localized uncertainty retains
intersecting old roots while positive observations are always merged").

So the design property is not *block*, it is **localized conservatism over a
published generation**: positives are always merged, negatives are only
applicable outside uncertain coverage.

### The uncertainty vector is empty in production — a real finding

An earlier revision of this document reported that the production path supplies
*coarser* uncertainty than the unit tests suggest (a walk root of `/`). That is
no longer the case, and the current behaviour is stronger than the old finding:
**the production path supplies no uncertainty at all.**

Three facts, each independently verifiable:

1. **Every producer of the three uncertainty categories emits `path: None`.**
   The aggregate global-walk diagnostic is built at `discovery.rs:783-797` with
   `path: None` (`discovery.rs:792`), and the explicit-root diagnostic at
   `discovery.rs:978-996` with `path: None` (`discovery.rs:987`). The comment at
   `discovery.rs:775-778` states the reason: dua-core's error event carries no
   child path, only a root index, and naming the root "would falsely identify
   the entire root as unreadable". These are the only two production emitters of
   `PermissionDenied` / `Vanished` / `Metadata` anywhere in `src/`.
2. **`main.rs:1052` drops them.** `filter_map(|d| d.path.clone())` discards
   every diagnostic with no path, so the vector handed to `reconcile_full` is
   empty whenever those diagnostics are the only ones present.
3. **The same events make the scan incomplete anyway.** A traversal error sets
   `*coverage_complete = false` at `discovery.rs:638` (global walk) and
   `discovery.rs:912` (explicit walk), so `full_traversal_complete` is false at
   `main.rs:1032-1036`, `complete` is false at `main.rs:1103`, and
   `reconcile_full` returns `NoPublication` at `:221`.

Therefore, in production, `reconcile_full` is **always** called with
`complete == true` and `uncertainty == []`. The uncertainty machinery — the
veto at `:288-294`, and the C028 probe's no-intersecting-uncertainty
precondition at `:308-310` — is reachable only from the unit tests.

The consequences are worth stating plainly, because they cut both ways:

- **The safety property still holds, by a different route.** A Full scan that
  could not read part of the machine does not publish at all, so it cannot
  convert a read failure into a negative observation. That is the ADR §6
  property, enforced by the completeness gate rather than by the uncertainty
  veto.
- **The unit test no longer describes a reachable behaviour.**
  `localized_uncertainty_retains_intersecting_roots_and_publishes_positives`
  (`:880`) asserts a behaviour that is correct for the function but that
  `main.rs` cannot currently produce. It is a green test whose premise the
  pipeline does not supply — the failure class `plans/registry.md` warns about.
  The same is true of `uncertainty_containment_is_symmetric` (`:1029`).
- **The fail-open edge is now the only path, and it is the live one.** A
  diagnostic with `path: None` is dropped from `uncertainty` at `main.rs:1052`
  silently, with no way for the module to know. That is not a latent gap: it
  is the current, and only, production behaviour. The defence is the
  completeness gate, not the uncertainty veto.

The registry's current wording for this area is C029 — "bounded pathless
coverage diagnostics and partial Full gating"
(`plans/registry.md:60`) — which is exactly the change that produced the
above. The older C005 row that this document used to cite is no longer in the
registry.

### `is_broad_root` (`:361-403`)

A private helper with **two** call sites: the `.filter()` in the parent
promotion (`:252`) and the M-7 post-fallback recheck (`:257`).

| Rule | Line | Detail |
|---|---|---|
| Filesystem root | `:362-364` | `path.parent().is_none()` — catches `/` and `C:\`. |
| unix system list | `:366-387` | **15** exact-match literals: `/Users`, `/home`, `/Volumes`, `/Applications`, `/Library`, `/opt`, `/System`, `/usr`, `/bin`, `/sbin`, `/dev`, `/proc`, `/sys`, `/run`, `/etc`. |
| Windows directory names | `:389-401` | `users`, `documents and settings`, `windows`, `program files`, `program files (x86)`, `programdata` — matched by **final component**, case-insensitively, so `C:\Users\Alice` is caught by name. |

Its role is to be the guard that stops a learned root from being too broad to be
trustworthy, which is ADR §2 (`:47`) and §4's "refuse promotion across
broad/platform boundaries" (`:64`) in code form. Without it, a project at
`/usr/local/thing` would promote `/usr/local` to a learned root, and the Routine
scan would then walk a system directory.

Three honest limits:

- The unix list is **exact equality, not prefix**. `/opt/homebrew/…` does not
  match, and neither does `/var`, `/tmp`, or `/usr/local` (which *is* an added
  global discovery root at `policy.rs:587`). A project directly under
  `/usr/local` would learn `/usr/local` as a root, because `project.parent()`
  there is `/usr/local` itself and `is_broad_root("/usr/local")` is `false`.
  (`automatic_root_is_maintainable` blocks `/usr/local` separately, at
  `discovery_state.rs:184` — the two rules are independent, and either one can
  withhold a root.)
- The `home` comparison at `:252` is raw `Path` equality against an
  **un-canonicalized** `p` (`project.parent()`, canonicalized only afterwards at
  `:263`) and against `$HOME` as the OS reports it. If `$HOME` is a symlink, or
  if the project path arrived un-canonicalized, the comparison misses and the
  home-directory guard silently fails. The `is_broad_root` list does not cover
  `$HOME` either.
- It is a *conservative-in-the-right-direction* heuristic, not an authority. The
  real containment guarantee for the Routine walk is
  `canonical_dedup_roots` in `policy.rs:653-695`, not this function.

## 6. Publication

### Atomicity — yes

`publish` (`:473-478`) resolves the path and delegates to `publish_at`
(`:480-497`):

1. `fs::create_dir_all(parent)` (`:482`).
2. `sweep_own_stale_temps(path)` (`:483`) — see below.
3. `serde_json::to_vec_pretty(state)` (`:484`).
4. `create_unique_temp` (`:485`, defined `:535-554`).
5. `file.write_all(&bytes)`, `file.sync_all()`, `drop(file)`, `replace_file(&tmp, path)`
   (`:486-491`).
6. On any failure, `fs::remove_file(&tmp)` and return `Err` (`:492-495`).

**It is temp-file + rename, never truncate-in-place.** `replace_file` is
`fs::rename` on non-Windows (`:555-558`) and `MoveFileExW` with
`MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` on Windows
(`:559-579`). A reader therefore sees either the complete old generation or the
complete new one, never a truncated file. This is ADR §6's "built in memory and
atomically replaces the old state" (`:88`).

Temp naming is `.{name}.{pid}.{attempt}.tmp` with `create_new(true)` and up to 32
attempts (`:541-553`), which is what makes two concurrent publishers in
different processes collide-free, and what the test at `:859` pins.

### Orphaned temp files are now swept — own-pid only

`sweep_own_stale_temps` (`:506-534`) runs before every publish. It removes files
in the state directory whose name matches `.{state-file-name}.{pid}.*.tmp`
**for the current process id** and whose modification time is more than an hour
old (`:520-529`). Two properties matter:

- **Own-pid only** (`:511`). Another live publisher's temp files are never
  touched, so the sweep cannot race a concurrent process.
- **An hour old only** (`:526`). The 32-attempt skip in `create_unique_temp`
  preserves a *fresh* colliding temp, and the sweep preserves it too.

The limitation is explicit in the doc comment at `:498-505`: only pid reuse
could exhaust the 32-attempt space, so this is a tidiness measure, not a
correctness one. A crashed publisher under a *different* pid leaves its temp
file behind permanently — the sweep will not remove it.

Durability caveat: `sync_all()` flushes the temp file, but the **containing
directory is never fsynced** after the rename. A crash immediately after a
successful `publish` can, on a journaling filesystem, leave the rename
unrecorded. The window is narrow and the consequence is bounded (see below), but
the code does not close it.

### Permissions — none are set

There is no `set_permissions` call anywhere in the module (verified by grep).
`fs::OpenOptions::new().write(true).create_new(true)` (`:543-546`) creates the
file with the process umask, i.e. typically `0644` on unix — **world-readable**.
The file contains the canonical path of every Rust project on the machine, which
is a mild but real information disclosure on a shared host. This is consistent
with `config.rs:494-497`, which uses the identical temp+`create_new` pattern for
`config.toml` and also sets no permissions, so it reads as an existing repo
convention rather than an oversight in this module. It is still worth a decision.

### Failure surfacing

`publish` returns `Result<(), String>`. Both call sites in `main.rs` treat
failure as a warning, never as a scan failure:

- Full branch: `pending_notices.push(format!("discovery state was not saved: {error}"))`
  (`main.rs:1130-1133`).
- Routine/Explicit touch: the same message at `main.rs:1159-1163`, guarded by
  `!observed.is_empty()` so an empty scan performs no write at all.

`NoPublication` prints `"discovery state was not reconciled: {reason}"`
(`main.rs:1136-1139`), and a `None` prior prints the `StateLoad::diagnostic()`
message plus "; Full state reconciliation was skipped" (`main.rs:1141-1147`).
Both Full-branch notices are collected into `pending_notices` and fanned out to
stderr only after the progress renderer has been cleared (`main.rs:1074-1078`) —
a raw `eprintln!` while the bars are live would corrupt the frame.

**One correction to the usual framing — and the correction has since been
applied.** A failed save *used to* change the exit code for a Full scan. The
block was

```rust
if full_incomplete || (full && !state_reconciled) { return Ok(1); }
```

so `cargo-cleanme scan --all` (the canonical spelling; `--full` is a hidden
alias) exited **1** when state could not be persisted, even though the scan
report was complete and correct — and the Routine branch, which never cleared
the same flag, exited **0** for the byte-identical warning.
That contradicted the module's own stated invariant, *"State is an optimization
only"*, which sits directly above `publish_at` in this file (`:479`).

The `state_reconciled` flag has been **removed**. The exit decision is now
`if full_incomplete { return Ok(1); }` (`main.rs:1361-1367`) and nothing else, so
both branches leave the exit code alone and a failed save is a stderr notice
only. The comment at `main.rs:1354-1360` records the reasoning: exit 1 means
"the requested scan did not fully complete", and a failed state save is not
that. A genuinely incomplete Full scan still exits 1, through `full_incomplete`
— which is computed from the same predicate as `complete`, not from the save
result.

### The degradation property, confirmed

> A failure to persist state degrades future scan performance but must never
> corrupt a scan result.

**Confirmed in code.** The scan pipeline's own results — `manifests`,
`counters`, `diagnostics`, `workspaces` (`main.rs:1061`), `groups`
(`main.rs:1171`), `physical` (`main.rs:1187`), `report` (`main.rs:1199-1218`) —
are computed and rendered independently of `discovery_state`. The state write
happens at `main.rs:1070-1164`, strictly *between* workspace resolution and
grouping, and its result feeds only `full_state`, `publish_ok`, and stderr text.
No discovery, measurement, grouping, or report value is read from or derived
from the published state. `clean --full` is the strongest case: it calls
`run_scan(ScanIntent::Full, …, EmitReport::No)` (`main.rs:729`), refuses to
clean when the scan reported a nonzero code or produced no generation
(`main.rs:730-737`), and consumes the **in-memory** generation the reconciliation
proved — never a disk reload — via `policy::full_cleanup_roots`
(`policy.rs:430-454`). A failed publish can never hand a half-built root list
to `cleanup`.

The cost of a failure is bounded and matches ADR §3 (`:51-53`): learned roots
expire from Routine scope and are rediscoverable by a later Full scan.

C033's active reporting slice groups positively omitted automatic roots by
reason and bounds the displayed path sample. It does not mutate this state
generation: a missing hint can therefore be reconsidered on a later Routine
invocation until the established complete-Full pruning or retention rule removes
it. Persistent targeted retirement remains decision-gated by proposed ADR 005.

## 7. Consumers

| Consumer | Call site | Reads | Notes |
|---|---|---|---|
| `policy::routine_roots` | `policy.rs:497` | `load_default()` | Routine scan root set. |
| `policy::routine_roots_from_state` | `policy.rs:512-542` | `StateLoad`, `learned_roots`, `last_project_seen_at`, `now_seconds()` | Re-implements the retention filter independently at `policy.rs:528-534` (same three clauses as `discovery_state.rs:318-321`, plus its own `now < r.last_project_seen_at` future-timestamp clause) and applies C030's maintainability check at `policy.rs:529`. Seeds are always unioned in first (`policy.rs:517-520`); a non-`Loaded` load falls back to seeds + one warning (`policy.rs:539-540`). |
| `policy::routine_cleanup_candidates` | `policy.rs:465` | `StateLoad`, `learned_roots`, `last_project_seen_at`, `now_seconds()` | C028: the cleanup-only variant. Same age filter (`policy.rs:482-487`), but raw paths with provenance and no canonicalization, so classification (`policy.rs:216`) sees a symlink before any collapse could resolve it. |
| `clean --full` roots | `policy.rs:430`, `main.rs:736` | `learned_roots[].path` of the in-memory generation | C028: `full_cleanup_roots` consumes the generation the just-finished reconciliation proved — never a disk reload — then `admit_automatic` (`main.rs:737`) classifies before collapsing. A missing generation or non-zero reconciliation refuses cleanup. |
| `clean --full` / maintenance generation | `main.rs:763-768`, `main.rs:808-813` | `last_full_at` | `Option<u64>` → `state_generation`. |
| `run_scan`, Full branch | `main.rs:1079-1147` | everything | `load_default`, `full_reconciliation_prior`, `now_seconds`, `ProjectObservation`, `reconcile_full`, `publish`. Returns the proven generation in `ScanOutcome::full_state` (`main.rs:1120`) even when the save fails, so `clean --full` never falls back to a stale disk generation. |
| `run_scan`, Routine/Explicit branch | `main.rs:1148-1163` | `learned_roots[].last_project_seen_at` | The touch branch. Mutates only timestamps; never adds roots, never removes them, never touches `projects` or `last_full_at`. Advances a root when `project.starts_with(&root.path)` (`main.rs:1154`) for an observed workspace root. |
| Human output | `main.rs:457` | `state_generation` | `println!("state generation last_full_at={generation} (Unix seconds)")` — Human format only, and only when a generation exists. |
| JSON output | `output.rs:242` | `state_generation` | Serialized as `state_generation_last_full_at: Option<u64>` (`output.rs:61`, `output.rs:242`). |

`cleanup.rs` has **no** import of this module (verified by grep over `src/`).
Learned roots reach it only as admitted root paths plus an `AdmissionPremise`
of safely omitted roots through
`clean_with_roots_policy_selector_guarded` (`cleanup.rs:1288`, called from
`run_engine_guarded` at `main.rs:606`). That is the structural expression of
ADR §8: the module that authorizes deletion has no access to the state that
decides where to look, and the omitted set travels alongside only so the
engine can re-prove the boundary before spawning.

### If a consumer reads an incompatible future schema

`load_at` returns `UnsupportedNewer` and no `StateLoad` variant yields a
partially-valid state. The consequences diverge sharply by consumer, and that
divergence is intentional:

- **Routine scope** (`policy.rs:521-541`): falls back to seed/configured roots
  with one warning. The scan still works; it is merely no longer adaptive.
- **`clean --full`** (`main.rs:729-737`): `Loaded` is required, so roots become
  empty. `run_scan` has already returned 0 (it did not reconcile), so execution
  reaches `main.rs:413` and prints "no bounded cleanup roots are known; no
  cleanup commands were run", exit 0. **Fails safe: nothing is deleted.**
- **`run_scan` Full branch** (`main.rs:1141-1147`): skipped entirely with a
  warning. The exit code stays 0 unless the scan was independently incomplete.
- **Routine touch** (`main.rs:1148`): the `if let StateLoad::Loaded(prior)`
  pattern simply does not match, so no write occurs. Silent, and correctly so.

In every case the newer file is left byte-for-byte intact. The test at
`:813-825` pins exactly this: after a `schema_version: 99` file with an unknown
`"future"` field, the bytes are asserted unchanged.

## 8. Invariants and edge cases

### Can a Full reconciliation with `uncertain` non-empty shrink the learned-root set?

**Yes. Definitively yes — the set can shrink, even with uncertainty present,
because uncertainty is a per-root retention veto, not a publication gate.**

Trace, with `complete == true`:

`Reconciliation::Publish` is returned unconditionally at `:358` once the
`complete` gate passes; uncertainty never reaches that gate. Shrinkage then comes
from exactly three places, none of which consults `uncertainty` for the paths
that matter:

1. **The `covered` skip (`:285-287`).** An old root is dropped whenever it is an
   ancestor-or-equal of a fresh root — and this `continue` executes *before* the
   uncertainty test at `:288`. A root that both intersects uncertainty and sits
   above a fresh observation is dropped, and the uncertainty veto never gets a
   chance to fire. Concretely: old `/work` plus a fresh observation at
   `/work/a/b` (candidate `/work/a`) means `covered` contains `/work/a` and
   `/work`, so `/work` is dropped and the learned set narrows from `/work` to
   `/work/a`. Uncertainty is irrelevant to this transition.
2. **Containment collapse (`:330-356`).** A child root is folded into an already
   represented ancestor and disappears from the array. Uncertainty-retained roots
   are subject to this: a fresh `/work` plus an uncertain old `/work/a` retains
   `/work/a` at `:292`, then collapse drops the *path* `/work/a` in favour of
   `/work`. Coverage is subsumed rather than lost — the conservative direction —
   but the path is gone.
3. **Retention expiry (`:318-324`).** Independent of uncertainty, applied to
   every root that is neither `covered` nor retained.

What uncertainty *does* guarantee, precisely: **it is never itself a reason to
drop a root.** A root that intersects uncertainty and is not `covered` and not
collapse-subsumed is always retained. So the answer to "can uncertainty shrink
the set?" is no; the answer to "can the set shrink while uncertainty is
non-empty?" is yes.

**The dismissal rationale still holds.** The overview records this ordering as
"not a bug — checked and dismissed", on the grounds that "the collapse always
replaces an ancestor with a *descendant*, so the remembered set only ever
narrows. Narrowing means less cleanup on the next run, which is fail-safe."
Re-verified against the current code:

- `covered` is built **only** from roots that were actually admitted
  (`:274-283` runs before the old-root loop, and only admitted roots are in
  `learned` at that point). So a root that failed C030 (`:248`, `:260`) or
  M-7's broad-root recheck (`:257`) never enters `covered` and can never cause
  an old root to be dropped by rule 1.
- `covered.contains(&old.path)` is true only when `old.path` is an
  ancestor-or-equal of some fresh root. If equal, the old root is replaced by
  the same path with a fresher stamp — a refresh, not a narrowing. If a strict
  ancestor, it is replaced by a **descendant**.
- A descendant is always a subset of its ancestor, so the remembered set
  narrows in scope. Less scope means less cleanup next run, which is the
  fail-safe direction.

The reasoning is sound, and the current code is *safer* than when the dismissal
was made, because the C030 and M-7 gates now prevent a non-admitted root from
contributing to `covered` at all.

Two further qualifications, both load-bearing:

- **In production the whole question is moot** (§5): `reconcile_full` is called
  with `uncertainty == []` or not at all, so rule 2 of the old-root loop never
  fires and no root is retained for uncertainty. The behaviour the dismissal
  reasons about is reachable only from unit tests.
- **A diagnostic with `path: None` is dropped from `uncertainty`** at
  `main.rs:1052` via `filter_map`, silently, with no way for the module to
  know. That fail-open edge is now the *only* production path, and it is
  covered by the completeness gate rather than by the uncertainty veto.

### Can a Full scan with `complete == false` publish at all?

**No.** `:220-222` returns before any mutation, and it is the sole
production `NoPublication` site. `main.rs:1136-1139` prints the reason, and the
exit code is carried by the independently computed `full_incomplete` at
`main.rs:1361-1367`. The existing file is untouched — asserted directly at
`:797`.

### What happens on the very first run?

`load_default` → `Missing` → `full_reconciliation_prior` returns
`(DiscoveryState::default(), false)` (`:57`). `DiscoveryState::default()` gives
`schema_version: 0`, `last_full_at: None`, empty vectors (`:63`). Reconciliation
with a complete scan produces a `schema_version: 2` state with
`last_full_at: Some(now)` and roots derived purely from this scan's
observations. `replacing_invalid == false`, so **no** "replaced unusable state"
message is printed. `publish` creates the state directory if needed (`:482`).
Pinned by `full_reconciliation_creates_missing_and_migrates_v1_state` (`:827-857`).

### Two concurrent processes reconciling

**There is no locking of any kind.** No lock file, no advisory `flock`, no
`O_EXCL` on the destination, no CAS on the generation stamp. The module states
the trade-off explicitly at `:479`: *"State is an optimization only: concurrent
publishers use atomic last-writer-wins replacement."*

Therefore a **lost update is possible**: two Full scans both load generation *N*
at `main.rs:1071`, both reconcile, and the second rename wins. The first scan's
learned roots vanish from the file. The second scan's `last_full_at` is the one
that survives, and because it is computed as `now.max(prior.last_full_at)`
(`:224`) the stamp itself is monotone, so no generation appears to go backwards.

The consequence is bounded and non-destructive:

- A project observed only by the losing scan may be absent from the learned set,
  so the Routine scope is narrower than ideal until the next Full scan. This is a
  recall regression, not a safety one.
- No scan result is affected; nothing is deleted on the strength of this state
  (ADR §8, `:100-113`).
- The file is never corrupted: `create_unique_temp` uses `create_new` with
  PID+attempt naming (`:541-553`) so the writers' temp files cannot collide, and
  the rename is atomic.

The only genuinely contested resource is the file contents; the module has
deliberately chosen availability over serialisation. Note the asymmetry with
`create_unique_temp`'s 32-attempt budget (`:541-553`): that defends temp-name
allocation, not the destination. `sweep_own_stale_temps` (`:506-534`) is scoped
to the current pid for the same reason — it must not become a lever that lets
one publisher delete another's in-flight work.

### Is `now` trusted blindly?

Not entirely. `now` is clamped: `observed_at = now.max(prior.last_full_at.unwrap_or(0))`
(`:224`). The retention check additionally uses `saturating_sub` (`:320`) and an
explicit future-timestamp guard (`:318`).

- **Clock moves backwards.** `observed_at` is held at the prior generation
  value, so `last_full_at` never regresses. Every old root stamped above
  `observed_at` is retained by `:318`. Roots are effectively *frozen* — which is
  the conservative outcome and matches ADR §5 (`:80`). Nothing is prematurely
  expired and nothing is destroyed.
- **Clock moves forwards.** This is the real exposure. A forward jump (NTP
  correction, a wrong RTC, a timezone-adjacent mistake) inflates `observed_at`,
  so every root's computed age exceeds the retention window and **all learned
  roots expire on the next complete Full scan**. Consequence: Routine scope
  collapses to seed roots only (`policy.rs:517-520`) until the next Full scan
  re-learns them. Not destructive — learned roots are search scope, and Full
  scope is unaffected (ADR §3, `:49-53`) — but it silently removes the
  performance benefit the module exists to provide, and nothing warns. There is
  no sanity bound on `now_seconds()` (`:405-410`) and no plausibility check on
  a stored `last_full_at` relative to the current clock.
- `now_seconds` on a pre-epoch clock returns `0` (`unwrap_or_default`, `:408`),
  which would make every root look infinitely old and expire them all.

### Blast radius of a corrupted state file

**Self-healing, by design, with one deliberate exception.** A corrupt file
yields `RecoverableInvalid`, not an error, and read-only scanning stays fully
functional: `policy.rs:521-541` falls back to seed/configured roots with one
warning. The *next complete Full scan* replaces it, sets
`schema_version: CURRENT_SCHEMA`, and reports the replacement
(`main.rs:1123-1128`). Pinned by
`full_reconciliation_recovers_invalid_only_after_complete_scan` (`:785-810`),
which also asserts the corrupt bytes are untouched while the scan is incomplete
(`:797`).

The exception: a `RecoverableInvalid` file is replaced *only* by a complete Full
reconciliation. A user who never runs `--all` keeps getting the warning and the
seed-root fallback indefinitely. That is the correct trade (never destroy
unparseable bytes on a partial scan) but it is a permanent-degradation path, not
a transient one.

A *truncated* file is indistinguishable from a corrupt one, and the rename
protocol means truncation should not occur from this module — but a crash between
`create_unique_temp` and `replace_file` can leave orphaned
`.discovery-state.json.<pid>.<n>.tmp` files in the state directory. The failure
path at `:492-495` cleans up, but a hard kill does not. `sweep_own_stale_temps`
(`:506-534`) reclaims those **only** when the next publisher happens to run
under the same pid and the file is over an hour old; another pid's litter
remains.

### Does the module ever delete anything from disk?

**Only its own temp files.** The only production `fs::remove_file` in the file
is `:493`, inside `publish_at`'s error path, and it removes the module's *own*
temp file; `sweep_own_stale_temps` adds a second one at `:531`, scoped to the
current pid and to files over an hour old. `grep` for `remove_dir`,
`remove_dir_all`, and `remove_file` finds nothing else. This is the code-level
expression of ADR §8: a module that decides what is remembered cannot decide
what is removable.

## 9. Testing

`discovery_state.rs` **does** have an inline `#[cfg(test)]` module, at
**`:581-1129`** — 549 of the file's 1129 lines, and **17 tests**:

| # | Test | Line | What it pins |
|---|---|---|---|
| 1 | `unresolved_and_cache_manifests_never_create_routine_roots` | `:586` | An unresolved manifest and a cache/transient-location project create no learned root, while `projects` still keeps all four manifests. |
| 2 | `system_and_transient_locations_never_create_routine_roots` | `:647` | C030: platform caches, the download drop zone, and system software locations never become learned roots, while a durable developer location still does. |
| 3 | `shared_temp_aliases_are_non_maintainable` | `:723` | `/private/tmp/app`, `/tmp/app`, `/var/tmp/app` are non-maintainable with `home: None`. |
| 4 | `v1_migrates_in_memory_and_newer_schema_is_rejected` | `:732` | v1 loads, `schema_version` becomes `CURRENT_SCHEMA`, `resolution` defaults to `Resolved`; v99 → `UnsupportedNewer`. |
| 5 | `state_load_classifies_absent_invalid_newer_and_unavailable_paths` | `:749` | All five `StateLoad` outcomes, including a directory-as-file → `Unavailable` and `u64::MAX` → `UnsupportedNewer`. |
| 6 | `supported_schema_with_invalid_structure_is_recoverable` | `:773` | Current version + structurally invalid record → `RecoverableInvalid`. |
| 7 | `full_reconciliation_recovers_invalid_only_after_complete_scan` | `:785` | `replacing_invalid == true`; `complete=false` → `NoPublication` **and the corrupt bytes survive** (`:797`); then recovery to v2. |
| 8 | `full_reconciliation_never_selects_newer_or_unavailable_state_for_publication` | `:813` | `full_reconciliation_prior` → `None` for `UnsupportedNewer` and `Unavailable`; the `"future"` field's bytes are preserved verbatim. |
| 9 | `full_reconciliation_creates_missing_and_migrates_v1_state` | `:827` | First run and v1 run both publish; `replacing_invalid == false`; `last_full_at == Some(10)`. |
| 10 | `unique_temp_attempts_replace_existing_state` | `:859` | A colliding `.{name}.{pid}.0.tmp` is skipped, not clobbered; the destination is replaced. |
| 11 | `localized_uncertainty_retains_intersecting_roots_and_publishes_positives` | `:880` | Uncertain root retained, expired root dropped, `ManifestObservedUnresolved` recorded — from a complete scan. |
| 12 | `zero_retention_disables_expiration_and_nested_roots_collapse` | `:931` | **Does not demonstrate what its name says** — see the coverage finding below. The assertion is `learned_roots.is_empty()`. |
| 13 | `complete_certain_full_prunes_only_positively_absent_learned_roots` | `:966` | **Does not discriminate the rule it names** — see the coverage finding below. Both fixture roots are pruned, but by the C030 rule, not by the C028 absence probe. |
| 14 | `uncertainty_containment_is_symmetric` | `:1029` | Containment both directions; `/workspace/a` correctly does not match. |
| 15 | `failed_replacement_preserves_existing_destination_and_cleans_temp` | `:1038` | A failed replace leaves the destination intact **and** removes the temp file. |
| 16 | `duplicate_observations_for_one_workspace_fold_into_one_learned_root` | `:1064` | N members of one workspace contribute N observations that fold into one root. |
| 17 | `filesystem_root_workspace_is_never_a_learned_root` | `:1101` | M-7: a workspace at `/` is never learned. |

The count and the split agree with [the overview](overview.md) §3.1, which is the
source of truth for module line counts. The module has exactly one `saturating_sub`
(`:320`), which `architecture/01-domain-and-errors.md:518-521` now cites
correctly.

### Integration coverage

**There is now integration coverage for the read path**, added with C028 — the
previous revision of this document claimed there was none, and that claim is
obsolete. `tests/cli_contract.rs` has a dedicated C028 block
(`tests/cli_contract.rs:3304-3313`)
whose fixture (`StaleLearnedFixture`, `tests/cli_contract.rs:3323-3419`)
overrides `$HOME` and `$XDG_STATE_HOME` and writes a real
`discovery-state.json` at the path `state_path()` would resolve to
(`tests/cli_contract.rs:3365-3378`). Ten cases follow:

| Test | Line | What it pins |
|---|---|---|
| `stale_learned_root_is_omitted_and_bare_routine_execute_cleans_the_sibling` | `tests/cli_contract.rs:3560` | A deleted learned root is omitted, the surviving sibling is cleaned. |
| `stale_learned_root_simulate_agrees_with_execute_and_spawns_no_cargo_clean` | `tests/cli_contract.rs:3616` | Simulation parity; zero `cargo clean` spawns. |
| `repeated_routine_runs_bound_stale_root_noise_without_mutating_state` | `tests/cli_contract.rs:3647` | 100 stale roots produce one bounded summary line, and the state file is **byte-identical** afterwards (`tests/cli_contract.rs:3680`). |
| `missing_explicit_roots_stay_fatal_with_zero_cargo_clean_spawns` | `tests/cli_contract.rs:3689` | Explicit roots stay authoritative even alongside available learned paths. |
| `all_automatic_roots_missing_is_a_successful_empty_no_op` | `tests/cli_contract.rs:3729` | Every automatic root missing is a successful no-op. |
| `automatic_file_root_is_omitted_and_symlink_root_blocks_without_traversal` | `tests/cli_contract.rs:3756` | A file root is omitted; a symlink root blocks. |
| `out_of_anchor_poison_does_not_block_bare_routine_cleanup` | `tests/cli_contract.rs:3820` | ADR 007 scoped proof: an out-of-boundary unprovable is reported, not blocking. |
| `unreadable_automatic_root_blocks_instead_of_omitting` | `tests/cli_contract.rs:3859` | An unreadable automatic root blocks rather than being silently omitted. |
| `incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean` | `tests/cli_contract.rs:3892` | An incomplete discovery reports real counts and spawns no clean. |
| `stale_and_blocked_automatic_roots_keep_the_bounded_log_contract` | `tests/cli_contract.rs:3928` | Stale and blocked roots keep the bounded log contract. |

What that block does **not** cover is the write path: it seeds a state file and
reads it, but never exercises `reconcile_full` or `publish` end to end, and
`repeated_routine_runs_bound_stale_root_noise_without_mutating_state`
(`tests/cli_contract.rs:3647`) asserts the opposite for the Routine branch — the
state file must be untouched. There is no integration test that observes
`state generation last_full_at=` (`main.rs:457`), the `state_generation_last_full_at`
JSON field (`output.rs:61`), the "replaced unusable discovery state" message, or
the "Full state reconciliation was skipped" message.

The nearest external coverage of the *classification* path is one unit test in a
neighbour: `policy::tests::routine_state_errors_fall_back_to_seeds_with_one_warning`
constructs `RecoverableInvalid` and `UnsupportedNewer` by hand and asserts the
seed-root fallback with one warning — but it never exercises a real `load_at`.

### The genuine coverage gap

The inline tests are good at the *classification* and *publication* logic and
weak exactly where the module is subtle. Concretely under-tested:

0. **The C028 narrow-absence rule (`:308-317`) has no discriminating test at
   all.** This is the most consequential gap in the module, and it was
   confirmed by mutation, not by reading. The rule says a learned root
   positively confirmed nonexistent (`ENOENT` via `symlink_metadata`) is pruned
   *independent of the age window* — the one behaviour that distinguishes it
   from ordinary retention. Its nominal test,
   `complete_certain_full_prunes_only_positively_absent_learned_roots` (`:966`),
   stages its two roots inside a `tempfile::tempdir()` and calls
   `reconcile_full` with `home: None`. **Both fixture paths are therefore
   non-maintainable** — with no home, the canonical temp root is in the
   forbidden list (`:165-166`), and `shared_temp_aliases_are_non_maintainable`
   (`:723`) pins exactly that predicate. So the C030 rule at `:298-300` drops
   both roots *before* the C028 probe at `:308-317` is ever evaluated.

   Mutation check (run against a scratch copy of the crate, outside this
   repository, so nothing here was modified): deleting the C028 probe
   `:308-317` leaves **all 17 unit tests green**, including
   `complete_certain_full_prunes_only_positively_absent_learned_roots`, and
   leaves **all 62 integration tests in `tests/cli_contract.rs` green**. The
   symmetric mutation confirms the mechanism: deleting the C030 old-root rule
   `:298-300` instead makes **two** tests fail —
   `complete_certain_full_prunes_only_positively_absent_learned_roots` and
   `zero_retention_disables_expiration_and_nested_roots_collapse`. So the C030
   rule is covered and the C028 rule is not: the nominal C028 test is
   currently pinning C030.

   The same mutation also shows that
   `zero_retention_disables_expiration_and_nested_roots_collapse` (`:931`) no
   longer demonstrates either of the two behaviours in its name. Its fixture is
   likewise under the system temp root, so C030 empties the set before the
   retention rule or the collapse can be observed; the test's own comment at
   `:932-935` shows it was *rewritten* to cope with C028 and now asserts
   `learned_roots.is_empty()` — the opposite of "zero retention disables
   expiration". The zero-retention path and the containment collapse are
   therefore unexercised in the module, except for the exact-duplicate fold
   that `duplicate_observations_for_one_workspace_fold_into_one_learned_root`
   (`:1064`) does cover.

1. **The `covered`-skip-before-uncertainty ordering (`:285` vs `:288`).** No test
   constructs an old root that both intersects uncertainty *and* is an ancestor
   of a fresh root. This is the ordering that answers the §8 safety question, and
   nothing pins it. A refactor that moved the uncertainty test above the
   `continue` would pass all 17 tests.
2. **`is_broad_root`'s Windows name list is untested** — the `#[cfg(windows)]`
   arm (`:389-401`) has no test anywhere in-tree. The unix literals are now
   exercised indirectly by `system_and_transient_locations_never_create_routine_roots`
   (`:647`) and `filesystem_root_workspace_is_never_a_learned_root` (`:1101`).
3. **The `home` filter at `:252` is untested**, including the symlinked-`$HOME`
   miss described in §5.
4. **Clock regression** (`:224`, `:318`) and **the forward-jump mass-expiry**
   case from §8 have no test. `now` is always a small literal or `10` in every
   existing case.
5. **Retention boundary arithmetic at `±1` second** is untested. The
   `retention_days == 0` path is reached by `:931` only nominally (item 0
   above), and `:880` uses a far-past stamp.
6. **The collapse direction versus the `covered` direction** (§8 items 1 and 2) is
   never tested in combination. The containment collapse itself is only covered
   for the *exact-duplicate* case, by `:1064`; the ancestor-folds-into-descendant
   direction is not.
7. **Uncertainty granularity.** Test `:880` feeds a deep path the real pipeline
   never produces (§5), so nothing would fail if `discovery.rs` changed the
   diagnostic path — it already has, to `None`.
8. **The Windows rename path** (`:559-579`) and the `MOVEFILE_WRITE_THROUGH`
   durability claim are asserted nowhere in this file.
9. **The own-pid temp sweep** (`:506-534`) has no test at all: neither the
   "removes own stale temp" path nor the "never removes another pid's temp"
   path.

**Registry context for why this logic is subtle.** The registry's current rows
are C028 (closed, shipped in v0.2.4, `plans/registry.md:65`), C029 — "bounded
pathless coverage diagnostics and partial Full gating" (`plans/registry.md:60`),
C030 — "resolved durable workspaces only create Routine roots, known
cache/runtime locations are filtered without suppressing Full inventory"
(`plans/registry.md:61`), and C031 ("accurate resolution/unit counts precede
the still-fail-closed whole-scope block", `plans/registry.md:62`). The C005 and
C007 rows this document used to cite are no longer in the registry.

The pattern is that the *conservative-on-uncertainty* behaviour was arrived at
through corrective work after the first implementation published unsafely, and
the discovery-state recovery corrective followed. Per the repo's own stated
lesson — a green test proves only that its own premises hold
([the overview](overview.md) §11) — a module with 17 passing tests, no integration
coverage of the write path, and no test for the ordering that answers its safety
question should be reviewed against the code, not against its test count.

## 10. Review checklist

1. **Does anything still return `NoPublication` for a reason other than
   `!complete`?** `reconcile_full`'s only production refusal site is
   `:220-222`. A new refusal reason must be added to `Reconciliation`
   (`:101-105`) and will need a matching `main.rs:1136-1139` message.
2. **Is the five-rule order intact — covered, uncertainty, C030 provenance, C028
   absence, then retention?** `discovery_state.rs:285` must precede
   `:288` (uncertainty), which must precede `:298` (C030 provenance), which must
   precede `:308` (positive `ENOENT`), which must precede `:318` (retention).
   Coverage short-circuits before every drop/retain decision; the absence probe
   carries its own no-intersecting-uncertainty precondition (`:308-310`) and
   `ENOENT`-only positive absence (`:311-314`). Hoisting the retention rule
   ahead of the probe reintroduces age-based retention for physically absent
   roots — the C028 defect.
3. **Does `full_reconciliation_prior` still return `None` for
   `UnsupportedNewer` and `Unavailable`?** `discovery_state.rs:54-61`. This is
   the whole overwrite-safety gate;
   `full_reconciliation_never_selects_newer_or_unavailable_state_for_publication`
   (`:813`) is the test.
4. **Is the version pre-check still before the typed decode?**
   `discovery_state.rs:442-454` must stay ahead of `:461`. Reversed, a v3 file
   that this binary cannot parse becomes `RecoverableInvalid` and is then
   overwritten.
5. **Does the publish still go through temp + rename, and still remove its temp
   on failure?** `discovery_state.rs:473-497` and `replace_file` at `:555-579`.
   The success-path test is `unique_temp_attempts_replace_existing_state`
   (`:859`); its failure twin is
   `failed_replacement_preserves_existing_destination_and_cleans_temp`
   (`:1038`). A truncate-in-place regression would leave `last_full_at`
   corruptible.
6. **Has anything besides the module's own temp file become deletable?**
   `discovery_state.rs:493` and `:531` are the only production `remove_file`
   calls in the file, and `:531` is scoped to the current pid and to files over
   an hour old. The sweep must never widen to another publisher's temps.
7. **Is `observed_at` still `now.max(prior.last_full_at.unwrap_or(0))`?**
   `discovery_state.rs:224`. Removing the `max` reintroduces a regressing
   generation stamp and makes the future-timestamp guard at `:318` the only
   remaining defence.
8. **Do `policy.rs:528-534` and `discovery_state.rs:318-321` still agree?**
   The retention predicate is duplicated across the module boundary. Any change
   to one must be made to the other, and the fact that Routine applies it at
   read time while Full applies it at write time is easy to miss.
9. **Did a change to `discovery.rs` diagnostic `path` values alter what reaches
   `uncertain`?** `discovery.rs:783-797` and `discovery.rs:978-996` now carry
   **`path: None`** — the walk root is named in the message, not the path field.
   A `None` path is dropped at `main.rs:1052` and cannot retain anything. The
   defence is the completeness gate at `discovery.rs:638` and `discovery.rs:912`,
   which makes such a scan incomplete and therefore unpublishable. Check this
   whenever diagnostics are refactored: removing the `coverage_complete = false`
   assignments without restoring a path on the diagnostic would make an
   incomplete scan publishable.
10. **Has the save-failure exit-code coupling come back?** It is now
    structurally absent: the exit decision reads only `full_incomplete`
    (`main.rs:1361-1367`) and there is no flag to set. The regression to watch
    for is someone reintroducing a save-result branch into that `if` — the
    module's own `publish` unavailability error at `discovery_state.rs:474-476`
    is the authority to cite when arguing against it.
11. **Does a new caller of `automatic_root_is_maintainable` understand what
    `true` means?** A symlink is *maintainable* by construction
    (`discovery_state.rs:117-119`) so the admission layer can block it; `true`
    means "this is a durable developer location, or a judgement the admission
    layer must make" — not "this root is safe to clean".
12. **Is the C028 absence rule still discriminating?** It is live production
    code at `discovery_state.rs:308-317` and it has **no** test that would
    notice its removal (see §9 item 0). Any change to it — or to the C030 rule
    at `:298-300` that currently runs in front of it — should come with a test
    that stages a learned root that is **absent** but **inside the retention
    window**, at a **maintainable** location (not under the system temp root,
    which C030 excludes). Until that exists, treat this rule as uncovered.
