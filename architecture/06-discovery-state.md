# Discovery State — learned roots, uncertainty, and safe publication

> Component deep dive · part of the [architecture overview](overview.md)

Subject: `src/discovery_state.rs` (656 lines, of which 255 are inline tests).

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

### The leaf property

Verified: `discovery_state.rs` contains **no `crate::` import and no `super::`
reference outside its own test module** (which uses `use super::*;` at
`:404`). Its entire import block is three external crates plus `std`
(`:1-9`):

| Import | Line | Use |
|---|---|---|
| `directories::ProjectDirs` | `:1` | `state_path()` |
| `serde::{Deserialize, Serialize}` | `:2` | state record derives |
| `serde_json` (path-qualified) | `:301`, `:320`, `:342` | version pre-check, decode, encode |
| `windows_sys::…::MoveFileExW` | `:383-385` | `#[cfg(windows)]` atomic replace |
| `std::{collections, fs, io::Write, path, time}` | `:3-9` | — |

This is architecturally load-bearing in the sense the [overview](overview.md)
claims at its §2: the state that gates the fast path has no dependency on the discovery
machinery (`discovery.rs`), the workspace resolver (`workspace.rs`), or the
cleanup authorizer that could bias what it remembers. A reviewer who adds a
`crate::` import here should be asked what invariant that import threatens.

The module is not pure either — `fs::canonicalize` (`:153`), `fs::read`
(`:288`), `fs::create_dir_all` (`:341`), `fs::OpenOptions` (`:364`) and the
rename family (`:377`, `:388-394`) all perform real I/O. It is a leaf in the
*dependency* graph, not a pure function module.

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
| `schema_version` | `u32` | Format generation. `0` is treated as invalid on read (`:324-326`). | Every `reconcile_full` publish (`:127`); normalized in memory on every successful load (`:328`); passthrough on a Routine touch. |
| `last_full_at` | `Option<u64>` | The generation stamp: wall-clock seconds of the newest **complete** Full reconciliation that published. `None` = never reconciled. | `reconcile_full` only (`:126-128`). Never written by the Routine/Explicit touch branch. |
| `projects` | `Vec<ProjectRecord>` | Exact project inventory from the last complete Full scan. Fully replaced, never merged. | `reconcile_full` only (`:129-144`). |
| `learned_roots` | `Vec<LearnedRoot>` | Containers the Routine scan should search, each with a positive-observation stamp. | `reconcile_full` (`:145-216`), and `last_project_seen_at` only via the Routine touch in `main.rs:427-433`. |

`#[serde(default)]` sits on the **struct** (`:64`), so all four fields may be
absent from the JSON and decode to `Default`. That is what lets a v1 file with
only `{"schema_version":1,"last_full_at":12,"projects":[…],"learned_roots":[]}`
load at all.

### `ProjectRecord` (`:71-81`)

| Field | Type | Meaning | When written |
|---|---|---|---|
| `workspace` | `PathBuf` | Workspace identity when Cargo resolution succeeded, otherwise the manifest's parent directory (doc comment `:73`). **Required** — no `#[serde(default)]`. | `reconcile_full` (`:132-135`) |
| `manifest` | `Option<PathBuf>` | The observed `Cargo.toml` path. `#[serde(default)]` (`:75-76`). | `reconcile_full`, always `Some` (`:136`) |
| `resolution` | `ResolutionStatus` | Whether Cargo answered. `#[serde(default)]` (`:77-78`). | `reconcile_full` (`:137-141`) |
| `observed_at` | `u64` | The generation stamp of the scan that produced this record. `#[serde(default)]` (`:79-80`), so v1 records decode as `0`. | `reconcile_full` (`:142`) |

Note that the `projects` array is written but **never read back by any consumer
in the crate**. Grepping `last_full_at` and `learned_roots` across `src/` shows
`projects` has zero readers. It is inventory retained for the ADR's "exact
project inventory" concept (ADR §4, `:57-68`); the Routine path uses
`learned_roots` only.

### `ResolutionStatus` (`:82-88`)

`#[serde(rename_all = "snake_case")]` + `#[default] Resolved`. On disk the two
variants are `"resolved"` and `"manifest_observed_unresolved"`.

### `LearnedRoot` (`:89-93`)

| Field | Type | Meaning | When written |
|---|---|---|---|
| `path` | `PathBuf` | Canonicalized search container. **Required.** | `reconcile_full` (`:153-155`) |
| `last_project_seen_at` | `u64` | Positive-observation stamp, seconds. **Required.** | `reconcile_full` (`:156`); advanced by the Routine touch |

`LearnedRoot` carries **no** `#[serde(default)]` on either field. An entry
missing either one makes the whole file `RecoverableInvalid` — a strictness the
`DiscoveryState` struct-level default does not extend to array elements.

### Migration policy — read the code, do not assume

Three distinct outcomes, and only one of them is "migrate":

| On-disk `schema_version` | Outcome | Behaviour |
|---|---|---|
| `> 2` | `StateLoad::UnsupportedNewer` (`:307-313`) | **Rejected.** Never overwritten, never used as a prior (`full_reconciliation_prior` returns `None`, `:59`). Matches ADR §7 (`:98`). |
| `1` | `StateLoad::Loaded`, silently upgraded | **Migrated in memory only.** `:328` sets `state.schema_version = CURRENT_SCHEMA` in the loaded value; the file on disk is unchanged until some `publish` rewrites it. |
| `0` | `StateLoad::RecoverableInvalid("invalid schema 0")` (`:324-326`) | **Recoverable.** |
| absent, or non-numeric, or undecodable | `StateLoad::RecoverableInvalid(serde message)` (`:320-323`) | **Recoverable.** |
| structurally invalid at the current version | `StateLoad::RecoverableInvalid(serde message)` | **Recoverable.** |

The 1 → 2 bump is **semantic, not structural**. There is no field in `ProjectRecord`
that a v1 file could carry and a v2 file could not. What changed is meaning: a v1
record deserialized without `resolution` defaults to `Resolved` (`:85-86`), and
`load_at` treats that default as authoritative — the comment at `:327` says so
("v1 records deserialize with defaults as authoritative resolved projects"). The
discriminating marker on disk is therefore the *presence* of
`"resolution": "manifest_observed_unresolved"`, not any layout difference.

A negative or float `schema_version` is a genuinely under-specified path: the
pre-check's `as_u64` returns `None` (`:304`), so the file falls through to the
typed decode, which fails, and the resulting message is serde's, **not**
`"invalid schema 0"`. Only a literal `0` gets the clear message.

## 3. Loading

### `StateLoad` (`:19-30`)

| Variant | Meaning | May be overwritten? |
|---|---|---|
| `Missing` | No file, or no locatable state directory. | Yes — treated as first run. |
| `Loaded(DiscoveryState)` | Supported schema, structurally valid. | Yes. |
| `RecoverableInvalid(StateProblem::Invalid)` | Present but unusable: bad JSON, schema 0, or structural mismatch. | Yes, **but only** by a *complete* Full reconciliation. |
| `UnsupportedNewer { path, schema, current }` | Written by a newer binary. | **Never.** |
| `Unavailable(StateProblem::Unavailable)` | I/O failure that is not `NotFound` (e.g. the path is a directory, or EACCES). | **Never.** |

`StateProblem` (`:13-17`) is the payload pair `Invalid` / `Unavailable`, and the
pairing with `StateLoad` variants is exact by construction: `RecoverableInvalid`
only ever carries `Invalid` (`:314-319`), `Unavailable` only ever carries
`Unavailable` (`:291-296`).

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

The boolean is `replacing_invalid`. `main.rs:402-406` uses it to print
"replaced unusable discovery state after successful Full reconciliation" — an
operator-visible acknowledgement that stale bytes were clobbered, so the
replacement is never silent.

`Unavailable` and `UnsupportedNewer` returning `None` is the fail-closed half of
the design: an unreadable file and a future-schema file are both treated as
**unknown**, and unknown is never a licence to write. `main.rs:417-422` turns
that `None` into a stderr warning and a skipped reconciliation.

### `load_default` (`:281-286`)

`state_path()?` → if `None`, return `StateLoad::Missing`. Note this collapses
"the OS gave us no home directory" into `Missing`, i.e. fail-soft first-run
behaviour, not `Unavailable`. Everything else delegates to `load_at`.

### `load_at` (`:287-330`) — parse order

1. `fs::read`. `NotFound` → `Missing` (`:290`). Any other error → `Unavailable`
   with the OS message (`:291-296`).
2. **Version pre-check before typed decode** (`:301-313`). The bytes are parsed
   as `serde_json::Value`; only if the top level is an `Object` with a numeric
   `schema_version > 2` does it return `UnsupportedNewer`. The comment at
   `:298-300` states the intent: a newer writer "may have added fields or
   changed record structure that this binary must leave untouched." Checking
   before decoding is what makes that promise meaningful — a v3 file whose
   `LearnedRoot` shape this binary cannot parse is still recognised and
   preserved rather than mangled into `RecoverableInvalid` and then replaced.
3. Typed decode into `DiscoveryState`; failure → `RecoverableInvalid` with
   serde's message (`:320-323`).
4. `schema_version == 0` → `RecoverableInvalid("invalid schema 0")` (`:324-326`).
5. Normalize `schema_version` to `CURRENT_SCHEMA` in memory; return `Loaded`
   (`:328-329`).

### serde strictness — what is actually there

**There is no `#[serde(deny_unknown_fields)]` anywhere in this file** (verified
by grep over the whole module). Unknown fields are tolerated and silently
dropped.

That is a deliberate and *sufficient* design here, but only because step 2
carries the forward-compatibility load. The v1→v2 test at `:409` and the
structural test at `:452` both rely on tolerance: `{"schema_version":2,
"projects":[{"not_workspace":"/work"}]}` is invalid not because `not_workspace`
is unknown, but because the **required** `workspace` field is missing
(`:74` has no `#[serde(default)]`).

The honest limit of that tolerance: a future writer that bumped nothing but
changed `LearnedRoot`'s shape would be silently accepted by this binary. The
version number is the only forward-compat mechanism; strictness is not a
backstop.

### `state_path` (`:270-277`) and what `None` means

```rust
let dirs = ProjectDirs::from("", "", "cargo-cleanme")?;
Some(dirs.state_dir().or_else(|| Some(dirs.data_local_dir()))?.join("discovery-state.json"))
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

### `now_seconds` (`:264-269`)

`SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()`.
A pre-epoch clock yields `0` rather than panicking. The caller supplies this
value; the module never reads the clock itself inside `reconcile_full`, which is
what makes the reconciliation unit-testable with a synthetic `now`.

### `StateLoad::diagnostic` (`:33-49`)

Returns `Option<String>`: `None` for `Missing` and `Loaded` (both are normal
outcomes that need no explanation), and a distinct sentence per failure —
"ignoring unusable discovery state …" / "… uses unsupported schema N; update
cargo-cleanme" / "cannot read discovery state …". It is consumed by
`main.rs:418-420` and by `policy.rs:122`.

## 4. Reconciliation

### `reconcile_full` — signature and inputs (`:113-121`)

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
| `retention_days` | Age budget. `0` disables expiration entirely (`:182`). |
| `home` | Suppresses promotion to `$HOME` itself as a learned root (`:151`). |

### The `Reconciliation` decision

`Reconciliation` (`:101-105`) is `Publish(DiscoveryState)` or
`NoPublication(&'static str)`. There is exactly **one** `NoPublication`
construction site in the whole module:

```rust
if !complete {
    return Reconciliation::NoPublication("Full traversal was incomplete");   // :122-124
}
```

`complete == false` is the *only* condition. Uncertainty, an empty
observation set, a corrupt prior, and retention pressure all still publish.
The `&'static str` is a borrowed literal, so a caller cannot allocate a
reason string here; adding a second refusal reason means touching the enum.

### Step-by-step

**1. Gate (`:122-124`).** `!complete` → `NoPublication`. Nothing is mutated;
`prior` is only read.

**2. Clone and stamp (`:125-128`).** `next = prior.clone()`, then
`observed_at = now.max(prior.last_full_at.unwrap_or(0))`, `schema_version =
CURRENT_SCHEMA`, `last_full_at = Some(observed_at)`. Every one of the four
`DiscoveryState` fields is subsequently overwritten, so `next` is entirely
derived from `observations` plus the surviving old roots — the clone is
bookkeeping, not carry-over.

The `max` at `:126` is the clock-regression guard: the generation stamp never
goes backwards, which is what makes the future-timestamp retention rule at
`:181` meaningful.

**3. Project records (`:129-144`).** One `ProjectRecord` per observation.
`workspace` is `o.workspace` if present, else `o.manifest.parent()`, else the
manifest path itself (`:132-135`). `resolution` is `Resolved` iff
`o.workspace.is_some()` (`:137-141`). `manifest` is always `Some`.

**4. Fresh learned-root candidates (`:145-159`).** Per observation, `project` =
`o.workspace`, else the manifest's parent. Then:

```rust
let candidate = project
    .parent()
    .filter(|p| Some(*p) != home && !is_broad_root(p))
    .unwrap_or(project);                                                    // :149-152
let path = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf()); // :153
```

The project root's **direct parent** is the search container — this is ADR §4's
"candidate learned search container, normally the workspace root's direct
parent" (`:62-64`). The `.unwrap_or(project)` is the refusal case: when
promoting to the parent would be too broad, the project root itself is retained
instead of the parent (ADR §2, `:47`, and §4, `:64`). Canonicalization is
best-effort: on failure the raw spelling is stored, which is the only way a
never-yet-created path can become a learned root.

**5. The `covered` ancestor index (`:164-173`).** For every fresh root, walk
upward inserting each ancestor into a `HashSet`, stopping at the first
already-indexed ancestor. `covered` answers "is there a fresh root at or below
this path?" in one lookup instead of a scan per old root. The comment at
`:160-163` justifies the early `break` (`:169`): if an ancestor is already
indexed, its own ancestors are too, so the rest of the walk is redundant.

**6. Old roots: retain or drop (`:174-188`).** For each `old` in
`prior.learned_roots`:

```rust
if covered.contains(&old.path) { continue; }                                 // :175-177
if uncertainty.iter().any(|p| uncertainty_intersects(&old.path, p))           // :178-180
    || old.last_project_seen_at > observed_at                                 // :181
    || retention_days == 0                                                   // :182
    || observed_at.saturating_sub(old.last_project_seen_at)
        <= u64::from(retention_days) * 86400                                 // :183-184
{ learned.push(old.clone()); }
```

Four independent reasons to keep an old root. Note the **ordering**: the
`covered` test at `:175` short-circuits *before* the uncertainty test at `:178`.
This ordering is the single most consequential detail in the function and is
analysed in §5 and §8.

Retention arithmetic: `u64::from(retention_days) * 86400` is overflow-free
because `config::load` caps the value at 3650 (`config.rs:105-109`), giving at
most 315,360,000 — which is also what makes `policy.rs:46`'s `as u16` cast
lossless. `saturating_sub` means a future `old.last_project_seen_at` cannot
wrap; it is caught by the explicit `>` guard at `:181` first anyway. This is the
ADR §5 rule "Future timestamps fail conservative: they do not cause expiration"
(`:80`) implemented literally.

**7. Sort and containment-collapse (`:189-215`).** `learned` is sorted by path;
since a parent always sorts before its children, every possible parent is
already represented when its child is reached. Each root is then folded into its
nearest already-represented ancestor, taking the **max** of the two
`last_project_seen_at` values (`:206-208`). Note the direction: collapse
**prefers the ancestor**, keeping the broader container. This is the opposite
direction from the `covered` skip in step 6, which prefers the descendant. The
two rules pull in opposite directions and both are observable.

**8. Publish decision (`:216-217`).** `next.learned_roots = collapsed;
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
purely operator-communication concern, used at `main.rs:402-406` and nowhere
else. Because the gate lives outside, a caller that forgets to consult it cannot
silently clobber a newer binary's file. The one in-tree reconciliation caller
does consult it (`main.rs:360`); `policy.rs` never reconciles.

### How `main.rs` supplies the two gate inputs

`uncertain` — `main.rs:327-339`: `discovered.diagnostics` filtered to
`DiagnosticCategory::PermissionDenied | Metadata | Vanished`, then
`filter_map(|d| d.path.clone())`. `InvalidEntry`, `PlatformRoot`, and
`CandidateUncertain` diagnostics are **excluded** and play no part in
reconciliation. (A diagnostic with `path: None` is silently dropped, which is
fail-open for that entry — see §8.)

`complete` — `main.rs:383-386`: `!diagnostics.iter().any(|d| d.category ==
PlatformRoot && d.severity == Error)`. Only an `Error`-severity
`PlatformRoot` diagnostic makes a Full scan incomplete. Everything else —
including every `Warning`-severity diagnostic — leaves the scan publishable.
`main.rs:465-469` recomputes the identical predicate into `full_incomplete` for
the exit code; the duplication is redundant but harmless.

## 5. Uncertainty handling

### `uncertainty_intersects` (`:107-109`)

```rust
root == uncertain || root.starts_with(uncertain) || uncertain.starts_with(root)
```

| Property | Value |
|---|---|
| Semantics | Symmetric path containment on **components**, not on string prefixes. `/work/a` does not match `/workspace/a` (tested at `:633`). |
| Canonicalization | **None.** It is pure `Path` arithmetic; it never touches the filesystem. |
| Symmetry | Explicitly both directions, tested at `:627-634`. |

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
`:123`. When `complete == true` and `uncertainty` is non-empty, the result is a
full `Publish` including every positive observation, with the intersecting old
roots carried forward (`:186`). This matches the doc comment at `:111-112` and
the registry's C005 closure wording: "Localized Full uncertainty reconciles
conservatively and publishes a useful generation"
(`plans/registry.md:62`).

So the design property is not *block*, it is **localized conservatism over a
published generation**: positives are always merged, negatives are only
appliable outside uncertain coverage.

### The input is coarser than the tests suggest — a real finding

The unit test at `:553-602` feeds `/work/uncertain/nested` as an uncertainty
path, exercising genuinely localized uncertainty. But the production path
supplies something much coarser: the diagnostic at `discovery.rs:843-848` sets
`path: Some(root.to_path_buf())` — the **scan root of the walk**, not the
unreadable subtree. The other uncertainty-producing site,
`discovery.rs:585-594`, also carries a root path
(`roots.get(root_idx).map(|(_, root)| root.clone())`).

For a Full scan the roots are `["/"]` on Linux and `["/", "/usr/local"]` on
macOS with `/usr/local` present (`policy.rs:141-156`, canonicalized at
`policy.rs:134`). Therefore, on both unix platforms, **any single unreadable
directory anywhere on the machine during a Full scan yields an uncertainty entry
of `/`**, and `uncertainty_intersects(any_absolute_root, "/")` is `true` for
every learned root via `root.starts_with("/")`.

Consequences, all verifiable from the code above:

- In practice, one permission-denied directory on a Full scan retains the entire
  learned-root set through rule 1 of `:178-180`. The retention/expiry path is
  unreachable for that generation.
- `localized_uncertainty_retains_intersecting_roots_and_publishes_positives`
  (`:553`) asserts a behaviour that is correct for the function but is not
  reachable through `main.rs` today. It is a green test whose premise the
  pipeline does not supply — exactly the failure class `plans/registry.md`
  warns about at the summary quoted in [overview](overview.md) §7.
- Because the walker never follows symlinks (`discovery.rs:818`,
  `discovery.rs:527`), root-anchored paths stay canonical and the
  un-canonicalized `starts_with` comparison is sound in practice. Had the walker
  crossed a symlink, the un-canonicalized comparison in
  `uncertainty_intersects` would silently stop matching.

### `is_broad_root` (`:220-262`)

A private helper, called from exactly one place: the `.filter()` at `:151`.

| Rule | Line | Detail |
|---|---|---|
| Filesystem root | `:221-223` | `path.parent().is_none()` — catches `/` and `C:\`. |
| unix system list | `:224-246` | 16 exact-match literals: `/Users`, `/home`, `/Volumes`, `/Applications`, `/Library`, `/opt`, `/System`, `/usr`, `/bin`, `/sbin`, `/dev`, `/proc`, `/sys`, `/run`, `/etc`. |
| Windows directory names | `:247-260` | `users`, `documents and settings`, `windows`, `program files`, `program files (x86)`, `programdata` — matched by **final component**, case-insensitively, so `C:\Users\Alice` is caught by name. |

Its role is to be the guard that stops a learned root from being too broad to be
trustworthy, which is ADR §2 (`:47`) and §4's "refuse promotion across
broad/platform boundaries" (`:64`) in code form. Without it, a project at
`/usr/local/thing` would promote `/usr/local` to a learned root, and the Routine
scan would then walk a system directory.

Three honest limits:

- The unix list is **exact equality, not prefix**. `/opt/homebrew/…` does not
  match, and neither does `/var`, `/tmp`, or `/usr/local` (which *is* an added
  global discovery root at `policy.rs:145`). A project directly under
  `/usr/local` would learn `/usr/local` as a root, because `project.parent()`
  there is `/usr/local` itself and `is_broad_root("/usr/local")` is `false`.
- The `home` comparison at `:151` is raw `Path` equality against an
  **un-canonicalized** `p` (`project.parent()`, canonicalized only afterwards at
  `:153`) and against `$HOME` as the OS reports it. If `$HOME` is a symlink, or
  if the project path arrived un-canonicalized, the comparison misses and the
  home-directory guard silently fails. The `is_broad_root` list does not cover
  `$HOME` either.
- It is a *conservative-in-the-right-direction* heuristic, not an authority. The
  real containment guarantee for the Routine walk is
  `canonical_dedup_roots` in `policy.rs:211-240`, not this function.

## 6. Publication

### Atomicity — yes

`publish` (`:332-337`) resolves the path and delegates to `publish_at`
(`:339-355`):

1. `fs::create_dir_all(parent)` (`:341`).
2. `serde_json::to_vec_pretty(state)` (`:342`).
3. `create_unique_temp` (`:343`, defined `:356-375`).
4. `file.write_all(&bytes)`, `file.sync_all()`, `drop(file)`, `replace_file(&tmp, path)`
   (`:345-348`).
5. On any failure, `fs::remove_file(&tmp)` and return `Err` (`:350-353`).

**It is temp-file + rename, never truncate-in-place.** `replace_file` is
`fs::rename` on non-Windows (`:376-379`) and `MoveFileExW` with
`MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` on Windows
(`:380-400`). A reader therefore sees either the complete old generation or the
complete new one, never a truncated file. This is ADR §6's "built in memory and
atomically replaces the old state" (`:88`).

Temp naming is `.{name}.{pid}.{attempt}.tmp` with `create_new(true)` and up to 32
attempts (`:362-374`), which is what makes two concurrent publishers in
different processes collide-free, and what the test at `:532-551` pins.

Durability caveat: `sync_all()` flushes the temp file, but the **containing
directory is never fsynced** after the rename. A crash immediately after a
successful `publish` can, on a journaling filesystem, leave the rename
unrecorded. The window is narrow and the consequence is bounded (see below), but
the code does not close it.

### Permissions — none are set

There is no `set_permissions` call anywhere in the module (verified by grep).
`fs::OpenOptions::new().write(true).create_new(true)` (`:364-368`) creates the
file with the process umask, i.e. typically `0644` on unix — **world-readable**.
The file contains the canonical path of every Rust project on the machine, which
is a mild but real information disclosure on a shared host. This is consistent
with `config.rs:265-268`, which uses the identical temp+`create_new` pattern for
`config.toml` and also sets no permissions, so it reads as an existing repo
convention rather than an oversight in this module. It is still worth a decision.

### Failure surfacing

`publish` returns `Result<(), String>`. Both call sites in `main.rs` treat
failure as a warning, never as a scan failure:

- Full branch: `eprintln!("cargo-cleanme: discovery state was not saved: {error}")`
  (`main.rs:408-410`).
- Routine/Explicit touch: the same message at `main.rs:434-438`, guarded by
  `!observed.is_empty()` so an empty scan performs no write at all.

`NoPublication` prints `"discovery state was not reconciled: {reason}"`
(`main.rs:413-415`), and a `None` prior prints the `StateLoad::diagnostic()`
message plus "; Full state reconciliation was skipped" (`main.rs:417-422`).

**One correction to the usual framing — and the correction has since been
applied.** A failed save *used to* change the exit code for a Full scan. The
block was

```rust
if full_incomplete || (full && !state_reconciled) { return Ok(1); }
```

so `cargo-cleanme scan --full` exited **1** when state could not be persisted,
even though the scan report was complete and correct — and the Routine branch,
which never cleared the same flag, exited **0** for the byte-identical warning.
That contradicted the module's own stated invariant, *"State is an optimization
only"*, which sits directly above `publish` in this file.

The `state_reconciled` flag has been **removed**. The exit decision is now
`if full_incomplete { return Ok(1); }` (`main.rs:576-578`) and nothing else, so
both branches leave the exit code alone and a failed save is a stderr notice
only. A genuinely incomplete Full scan still exits 1, through `full_incomplete`
— which is computed from the same `PlatformRoot`+`Error` predicate as
`complete`, not from the save result.

### The degradation property, confirmed

> A failure to persist state degrades future scan performance but must never
> corrupt a scan result.

**Confirmed in code.** The scan pipeline's own results — `manifests`,
`counters`, `diagnostics`, `groups`, `physical`, `report` (`main.rs:341-489`) —
are computed and rendered independently of `discovery_state`. The state write
happens at `main.rs:387-416`, strictly *between* workspace resolution and
grouping, and its result feeds only `state_reconciled` and stderr text. No
discovery, measurement, grouping, or report value is read from or derived from
the published state. `clean --full` is the strongest case: it calls
`run_scan(..., emit_output=false)` (`main.rs:116`), aborts on any nonzero exit
(`main.rs:117-119`), and only then re-reads state at `main.rs:120` — so a failed
publish can never hand a half-built root list to `cleanup`.

The cost of a failure is bounded and matches ADR §3 (`:51-53`): learned roots
expire from Routine scope and are rediscoverable by a later Full scan.

## 7. Consumers

| Consumer | Call site | Reads | Notes |
|---|---|---|---|
| `policy::routine_roots` | `policy.rs:88` | `load_default()` | Routine scope root set. |
| `policy::routine_roots_from_state` | `policy.rs:96-125` | `StateLoad`, `learned_roots`, `last_project_seen_at`, `now_seconds()` | Re-implements the retention filter independently at `policy.rs:112-118` (same three clauses as `discovery_state.rs:181-184`, plus its own `now < r.last_project_seen_at` future-timestamp clause). Seeds are always unioned in first (`policy.rs:101-104`); a non-`Loaded` load falls back to seeds + one warning (`policy.rs:105-123`). |
| `clean --full` roots | `main.rs:120-131` | `learned_roots[].path` | Becomes plain `Vec<PathBuf>` roots, canonicalized and containment-collapsed by `collapse_roots` (`main.rs:154`, `:269-283`). |
| `clean --full` / `--known` generation | `main.rs:125`, `main.rs:144-147` | `last_full_at` | `Option<u64>` → `state_generation`. `--known` takes roots from `policy::resolve` and reads *only* the stamp. Explicit `ROOT` sets it to `None` (`main.rs:114`). |
| `run_scan`, Full branch | `main.rs:357-422` | everything | `load_default`, `full_reconciliation_prior`, `now_seconds`, `ProjectObservation`, `reconcile_full`, `publish`. |
| `run_scan`, Routine/Explicit branch | `main.rs:423-438` | `learned_roots[].last_project_seen_at` | The touch branch. Mutates only timestamps; never adds roots, never removes them, never touches `projects` or `last_full_at`. Advances a root when `project.starts_with(&root.path)` (`main.rs:429`) for an observed workspace root. |
| Human output | `main.rs:211-215` | `state_generation` | `println!("state generation last_full_at={generation} (Unix seconds)")` — Human format only, and only when a generation exists, so it prints for `--full` and `--known` and is absent for an explicit root. |
| JSON output | `main.rs:187`, `main.rs:228` | `state_generation` | Serialized as `state_generation_last_full_at: Option<u64>` (`output.rs:61`, `output.rs:236`). |

`cleanup.rs` has **no** import of this module (verified by grep over `src/`).
Learned roots reach it only as opaque root paths through
`clean_with_roots_policy_selector` (`main.rs:200-209`). That is the structural
expression of ADR §8: the module that authorizes deletion has no access to the
state that decides where to look.

### If a consumer reads an incompatible future schema

`load_at` returns `UnsupportedNewer` and no `StateLoad` variant yields a
partially-valid state. The consequences diverge sharply by consumer, and that
divergence is intentional:

- **Routine scope** (`policy.rs:105-123`): falls back to seed/configured roots
  with one warning. The scan still works; it is merely no longer adaptive.
- **`clean --full`** (`main.rs:120-124`): `Loaded` is required, so roots become
  empty. `run_scan` has already returned 0 (it did not reconcile), so execution
  reaches `main.rs:174-195` and prints "no bounded cleanup roots are known; no
  cleanup commands were run", exit 0. **Fails safe: nothing is deleted.**
- **`run_scan` Full branch** (`main.rs:375`, `:432-437`): skipped entirely with a
  warning. The exit code stays 0 unless the scan was independently incomplete.
- **Routine touch** (`main.rs:423`): the `if let StateLoad::Loaded(prior)`
  pattern simply does not match, so no write occurs. Silent, and correctly so.

In every case the newer file is left byte-for-byte intact. The tests at
`:486-498` pin exactly this: after a `schema_version: 99` file with an unknown
`"future"` field, the bytes are asserted unchanged.

## 8. Invariants and edge cases

### Can a Full reconciliation with `uncertain` non-empty shrink the learned-root set?

**Yes. Definitively yes — the set can shrink, even with uncertainty present,
because uncertainty is a per-root retention veto, not a publication gate.**

Trace, with `complete == true`:

`Reconciliation::Publish` is returned unconditionally at `:217` once the
`complete` gate passes; uncertainty never reaches that gate. Shrinkage then comes
from exactly three places, none of which consults `uncertainty` for the paths
that matter:

1. **The `covered` skip (`:175-177`).** An old root is dropped whenever it is an
   ancestor-or-equal of a fresh root — and this `continue` executes *before* the
   uncertainty test at `:178`. A root that both intersects uncertainty and sits
   above a fresh observation is dropped, and the uncertainty veto never gets a
   chance to fire. Concretely: old `/work` plus a fresh observation at
   `/work/a/b` (candidate `/work/a`) means `covered` contains `/work/a` and
   `/work`, so `/work` is dropped and the learned set narrows from `/work` to
   `/work/a`. Uncertainty is irrelevant to this transition.
2. **Containment collapse (`:195-215`).** A child root is folded into an already
   represented ancestor and disappears from the array. Uncertainty-retained roots
   are subject to this: a fresh `/work` plus an uncertain old `/work/a` retains
   `/work/a` at `:186`, then collapse drops the *path* `/work/a` in favour of
   `/work`. Coverage is subsumed rather than lost — the conservative direction —
   but the path is gone.
3. **Retention expiry (`:181-184`).** Independent of uncertainty, applied to
   every root that is neither `covered` nor retained.

What uncertainty *does* guarantee, precisely: **it is never itself a reason to
drop a root.** A root that intersects uncertainty and is not `covered` and not
collapse-subsumed is always retained. So the answer to "can uncertainty shrink
the set?" is no; the answer to "can the set shrink while uncertainty is
non-empty?" is yes.

Two further qualifications, both load-bearing:

- **In production the distinction is mostly moot on unix.** Per §5, a Full
  scan's uncertainty entries are walk-root paths, i.e. `/`, so rule 1 retains
  *every* non-`covered` old root; shrinkage reduces to `covered`-narrowing and
  collapse, and expiry cannot fire at all.
- **A diagnostic with `path: None` is dropped from `uncertainty`** at
  `main.rs:338` via `filter_map`, silently, with no way for the module to know.
  This is a fail-open edge: an uncertainty that cannot be located cannot retain
  anything. The two diagnostics that feed the filter do carry a path
  (`discovery.rs:592`, `discovery.rs:847`), so the gap is latent rather than
  active today — but nothing in the type system prevents a future
  `PermissionDenied` / `Metadata` / `Vanished` emitter from omitting it.

### Can a Full scan with `complete == false` publish at all?

**No.** `:122-124` returns before any mutation, and it is the sole
`NoPublication` site. `main.rs:428-430` prints the reason, and the exit code is
carried by the independently computed `full_incomplete` at `main.rs:576-578`.
The existing file is untouched — asserted directly at `:471`.

### What happens on the very first run?

`load_default` → `Missing` → `full_reconciliation_prior` returns
`(DiscoveryState::default(), false)` (`:57`). `DiscoveryState::default()` gives
`schema_version: 0`, `last_full_at: None`, empty vectors (`:63`). Reconciliation
with a complete scan produces a `schema_version: 2` state with
`last_full_at: Some(now)` and roots derived purely from this scan's
observations. `replacing_invalid == false`, so **no** "replaced unusable state"
message is printed. `publish` creates the state directory if needed (`:341`).
Pinned by `full_reconciliation_creates_missing_and_migrates_v1_state` (`:500-531`).

### Two concurrent processes reconciling

**There is no locking of any kind.** No lock file, no advisory `flock`, no
`O_EXCL` on the destination, no CAS on the generation stamp. The module states
the trade-off explicitly at `:338`: *"State is an optimization only: concurrent
publishers use atomic last-writer-wins replacement."*

Therefore a **lost update is possible**: two Full scans both load generation *N*
at `main.rs:357`, both reconcile, and the second rename wins. The first scan's
learned roots vanish from the file. The second scan's `last_full_at` is the one
that survives, and because it is computed as `now.max(prior.last_full_at)`
(`:126`) the stamp itself is monotone, so no generation appears to go backwards.

The consequence is bounded and non-destructive:

- A project observed only by the losing scan may be absent from the learned set,
  so the Routine scope is narrower than ideal until the next Full scan. This is a
  recall regression, not a safety one.
- No scan result is affected; nothing is deleted on the strength of this state
  (ADR §8, §100-113).
- The file is never corrupted: `create_unique_temp` uses `create_new` with
  PID+attempt naming (`:362-372`) so the writers' temp files cannot collide, and
  the rename is atomic.

The only genuinely contested resource is the file contents; the module has
deliberately chosen availability over serialisation. Note the asymmetry with
`create_unique_temp`'s 32-attempt budget (`:362-374`): that defends temp-name
allocation, not the destination.

### Is `now` trusted blindly?

Not entirely. `now` is clamped: `observed_at = now.max(prior.last_full_at.unwrap_or(0))`
(`:126`). The retention check additionally uses `saturating_sub` (`:183`) and an
explicit future-timestamp guard (`:181`).

- **Clock moves backwards.** `observed_at` is held at the prior generation
  value, so `last_full_at` never regresses. Every old root stamped above
  `observed_at` is retained by `:181`. Roots are effectively *frozen* — which is
  the conservative outcome and matches ADR §5 (`:80`). Nothing is prematurely
  expired and nothing is destroyed.
- **Clock moves forwards.** This is the real exposure. A forward jump (NTP
  correction, a wrong RTC, a timezone-adjacent mistake) inflates `observed_at`,
  so every root's computed age exceeds the retention window and **all learned
  roots expire on the next complete Full scan**. Consequence: Routine scope
  collapses to seed roots only (`policy.rs:101-104`) until the next Full scan
  re-learns them. Not destructive — learned roots are search scope, and Full
  scope is unaffected (ADR §3, `:49-53`) — but it silently removes the
  performance benefit the module exists to provide, and nothing warns. There is
  no sanity bound on `now_seconds()` (`:264-269`) and no plausibility check on
  a stored `last_full_at` relative to the current clock.
- `now_seconds` on a pre-epoch clock returns `0` (`unwrap_or_default`, `:267`),
  which would make every root look infinitely old and expire them all.

### Blast radius of a corrupted state file

**Self-healing, by design, with one deliberate exception.** A corrupt file
yields `RecoverableInvalid`, not an error, and read-only scanning stays fully
functional: `policy.rs:105-123` falls back to seed/configured roots with one
warning. The *next complete Full scan* replaces it, sets
`schema_version: CURRENT_SCHEMA`, and reports the replacement
(`main.rs:402-406`). Pinned by
`full_reconciliation_recovers_invalid_only_after_complete_scan` (`:458-484`),
which also asserts the corrupt bytes are untouched while the scan is incomplete
(`:471`).

The exception: a `RecoverableInvalid` file is replaced *only* by a complete Full
reconciliation. A user who never runs `--full` keeps getting the warning and the
seed-root fallback indefinitely. That is the correct trade (never destroy
unparseable bytes on a partial scan) but it is a permanent-degradation path, not
a transient one.

A *truncated* file is indistinguishable from a corrupt one, and the rename
protocol means truncation should not occur from this module — but a crash between
`create_unique_temp` and `replace_file` can leave orphaned
`.discovery-state.json.<pid>.<n>.tmp` files in the state directory. Nothing ever
sweeps them. The failure path at `:350-353` cleans up, but a hard kill does not.

### Does the module ever delete anything from disk?

**No.** The only `fs::remove_file` in the file is `:351`, inside `publish_at`'s
error path, and it removes the module's *own* temp file. `grep` for
`remove_dir`, `remove_dir_all`, and `remove_file` finds nothing else. This is the
code-level expression of ADR §8: a module that decides what is remembered cannot
decide what is removable.

## 9. Testing

**The brief's premise is wrong and the correction matters.**
`discovery_state.rs` **does** have an inline `#[cfg(test)]` module, at
**`:402-656`** — 255 of the file's 656 lines, and **11 tests**:

| # | Test | Line | What it pins |
|---|---|---|---|
| 1 | `v1_migrates_in_memory_and_newer_schema_is_rejected` | `:405` | v1 loads, `schema_version` becomes `CURRENT_SCHEMA`, `resolution` defaults to `Resolved`; v99 → `UnsupportedNewer`. |
| 2 | `state_load_classifies_absent_invalid_newer_and_unavailable_paths` | `:422` | All five `StateLoad` outcomes, including a directory-as-file → `Unavailable` and `u64::MAX` → `UnsupportedNewer`. |
| 3 | `supported_schema_with_invalid_structure_is_recoverable` | `:446` | Current version + structurally invalid record → `RecoverableInvalid`. |
| 4 | `full_reconciliation_recovers_invalid_only_after_complete_scan` | `:458` | `replacing_invalid == true`; `complete=false` → `NoPublication` **and the corrupt bytes survive**; then recovery to v2. |
| 5 | `full_reconciliation_never_selects_newer_or_unavailable_state_for_publication` | `:486` | `full_reconciliation_prior` → `None` for `UnsupportedNewer` and `Unavailable`; the `"future"` field's bytes are preserved verbatim. |
| 6 | `full_reconciliation_creates_missing_and_migrates_v1_state` | `:500` | First run and v1 run both publish; `replacing_invalid == false`; `last_full_at == Some(10)`. |
| 7 | `unique_temp_attempts_replace_existing_state` | `:532` | A colliding `.{name}.{pid}.0.tmp` is skipped, not clobbered; the destination is replaced. |
| 8 | `localized_uncertainty_retains_intersecting_roots_and_publishes_positives` | `:553` | Uncertain root retained, expired root dropped, `ManifestObservedUnresolved` recorded — from a complete scan. |
| 9 | `zero_retention_disables_expiration_and_nested_roots_collapse` | `:604` | `retention_days == 0` never expires; `/work/old` collapses into `/work`. |
| 10 | `uncertainty_containment_is_symmetric` | `:627` | Containment both directions; `/workspace/a` correctly does not match. |
| 11 | `failed_replacement_preserves_existing_destination_and_cleans_temp` | `:636` | A failed replace leaves the destination intact **and** removes the temp file. |

**Documentation discrepancy worth fixing (not fixed here — `overview.md` is not
mine to modify):** `overview.md:101` lists the module as `656` with no
"`/ of which inline `#[cfg(test)]`" split, unlike every other module that has
tests (`discovery.rs` 1370/827, `cleanup.rs` 6161/2324). The 255-line test
module is invisible in the index. Also `architecture/01-domain-and-errors.md:502`
attributes 11 `saturating_sub` uses to `discovery_state.rs`; the module has
exactly one (`:183`).

### Integration coverage

**There is none, for this module.** Grepping `tests/` for `--full`, `full:`,
`learned_root`, `discovery.state`, `last_full_at`, and `XDG_STATE` finds:

- `tests/end_to_end.rs:113` `end_to_end_reports_only_inactive_artifact_projects` —
  the only Full-adjacent integration test, and it passes `full: false`
  (`end_to_end.rs:127`) against an explicit `ScanRequest`. No reconciliation.
- `tests/cli_contract.rs` — six tests covering external-subcommand resolution,
  help parity, JSON envelope shape, scope-block exit codes, unattended
  `clean --yes`, and `config edit`. None set `--full`, none seed or read a state
  file, and none set `XDG_STATE_HOME` or `HOME`. `tests/cli_contract.rs:140`
  isolates `CARGO_HOME` only.
- `tests/common/mod.rs` — timestamp fixtures; no state helpers.

No integration test observes `state generation last_full_at=`
(`main.rs:214`), the `state_generation_last_full_at` JSON field
(`output.rs:61`), the "replaced unusable discovery state" message, or the
"Full state reconciliation was skipped" message.

The nearest external coverage is one unit test in a neighbour:
`policy::tests::routine_state_errors_fall_back_to_seeds_with_one_warning`
(`policy.rs:325-348`) constructs `RecoverableInvalid` and `UnsupportedNewer` by
hand and asserts the seed-root fallback with one warning — but it never exercises
a real `load_at`.

### The genuine coverage gap

The inline tests are good at the *classification* and *publication* logic and
weak exactly where the module is subtle. Concretely under-tested:

1. **The `covered`-skip-before-uncertainty ordering (`:175` vs `:178`).** No test
   constructs an old root that both intersects uncertainty *and* is an ancestor
   of a fresh root. This is the ordering that answers the §8 safety question, and
   nothing pins it. A refactor that moved the uncertainty test above the
   `continue` would pass all 11 tests.
2. **`is_broad_root` (`:220`) has no direct test.** The `:553` test uses
   `home: None` and Unix-only `/work` paths, so neither the `home` filter at
   `:151` nor any of the 16 unix literals nor the Windows name list is exercised.
   The `#[cfg(windows)]` arm (`:247-260`) is entirely untested anywhere I can
   find in-tree.
3. **The `home` filter is untested**, including the symlinked-`$HOME` miss
   described in §5.
4. **Clock regression** (`:126`, `:181`) and **the forward-jump mass-expiry**
   case from §8 have no test. `now` is always `10`, `100_000_000`, or `12` in
   every existing case.
5. **Retention boundary arithmetic at `±1` second** is untested; `:604` uses
   `retention_days = 0` and `:553` uses a far-past stamp.
6. **The collapse direction versus the `covered` direction** (§8 items 1 and 2) is
   never tested in combination.
7. **Uncertainty granularity.** Test `:553` feeds a deep path the real pipeline
   never produces (§5), so nothing would fail if `discovery.rs:846` changed the
   diagnostic path to the failing subtree — or to `None`.
8. **The Windows rename path** (`:380-400`) and the `MOVEFILE_WRITE_THROUGH`
   durability claim are asserted nowhere in this file.

**Registry context for why this logic is subtle.** `plans/registry.md:64` records
C007 "discovery-state recovery + post-M007 planning cleanup" as **closed**, with
"Hosted platform/MSRV run 37152008602 passed". The immediately preceding
corrective, `plans/registry.md:62`, is C005 "uncertainty-aware state
reconciliation", closed with the note "Localized Full uncertainty reconciles
conservatively; isolated real Full published schema 2". The M006E row
(`plans/registry.md:58`) records the same corrective twice: "localized
uncertainty now reconciles conservatively and publishes a useful generation".

The pattern is that the *conservative-on-uncertainty* behaviour was arrived at
through corrective work after the first implementation published unsafely, and
the "discovery-state recovery" corrective followed. Per the repo's own stated
lesson — a green test proves only that its own premises hold
(`overview.md:221-225`) — a module with 11 passing tests, no integration coverage,
and no test for the ordering that answers its safety question should be reviewed
against the code, not against its test count.

## 10. Review checklist

1. **Does anything still return `NoPublication` for a reason other than
   `!complete`?** `reconcile_full`'s only refusal site is `:122-124`. A new
   refusal reason must be added to `Reconciliation` (`:101-105`) and will need
   a matching `main.rs:413-415` message.
2. **Is the `covered` skip still before the uncertainty test?** `discovery_state.rs:175-177`
   must precede `:178-180`. Reversing them lets a re-observed ancestor prune a
   root that the uncertainty veto was supposed to protect.
3. **Does `full_reconciliation_prior` still return `None` for
   `UnsupportedNewer` and `Unavailable`?** `discovery_state.rs:54-61`. This is
   the whole overwrite-safety gate; `:486-498` is the only test.
4. **Is the version pre-check still before the typed decode?**
   `discovery_state.rs:301-313` must stay ahead of `:320`. Reversed, a v3 file
   that this binary cannot parse becomes `RecoverableInvalid` and is then
   overwritten.
5. **Does the publish still go through temp + rename, and still remove its temp
   on failure?** `discovery_state.rs:343-353` and `replace_file` at `:376-400`.
   The success-path test is `:636`'s failure twin; a truncate-in-place
   regression would leave `last_full_at` corruptible.
6. **Has anything besides the module's own temp file become deletable?**
   `discovery_state.rs:351` is the only `remove_file` in the file, and
   `discovery_state.rs:1-9` is the only import block. Both are one-line
   invariants that a careless refactor breaks silently.
7. **Is `observed_at` still `now.max(prior.last_full_at.unwrap_or(0))`?**
   `discovery_state.rs:126`. Removing the `max` reintroduces a regressing
   generation stamp and makes the future-timestamp guard at `:181` the only
   remaining defence.
8. **Do `policy.rs:112-118` and `discovery_state.rs:181-184` still agree?** The
   retention predicate is duplicated across the module boundary. Any change to
   one must be made to the other, and the fact that Routine applies it at read
   time while Full applies it at write time is easy to miss.
9. **Did a change to `discovery.rs` diagnostic `path` values alter what reaches
   `uncertain`?** `discovery.rs:585-594` and `discovery.rs:843-848` currently
   carry a **walk root**, which on unix means `/` and retains everything
   (`main.rs:344-356`, `discovery_state.rs:108`). A `None` path is silently
   dropped at `main.rs:355` and cannot retain anything — check this whenever
   diagnostics are refactored.
10. **Has the save-failure exit-code coupling come back?** It is now
    structurally absent: the exit decision reads only `full_incomplete`
    (`main.rs:576-578`) and there is no flag to set. The regression to watch
    for is someone reintroducing a save-result branch into that `if` — the
    module's own doc comment at `discovery_state.rs:338` is the authority to
    cite when arguing against it.
