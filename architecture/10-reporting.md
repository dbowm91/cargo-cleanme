# Reporting — human text and the versioned machine contract

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** nine of the ten schema divergences catalogued in §8.2 (D1-D10) are
> **closed** — `plans/output-schema-v1.md` was rewritten to describe the
> implementation as it actually is, so the analysis below is retained as the
> record of what diverged and why. D8 (no success indicator) is a design
> decision that stays open. §8.3 lists the six divergences that are **open
> today** (G1-G6), including two contract gaps in the JSON DTOs that no schema
> document covers.

Covers `src/report.rs` (387 lines, 190 production) and `src/output.rs` (1113
lines, 272 production) together. They are two projections of the same data, and
the contrast is the point: the human renderer predates the machine contract; the
DTO layer exists to protect it. `output.rs` now carries a third surface — the
`log` module (§7), a bounded single-line summary added in M012B. It is not a
machine contract and is deliberately not built on `EnvelopeV1`.

The **cleanup** human surface is a fourth projection, and since M020 it lives in
`src/cleanup.rs` rather than in `report.rs`: `CleanReport::render` dispatches to a
quiet shape and a verbose one (§3). `report.rs` owns the *scan* report and the
shared vocabulary — the byte formatter and the path escaping — that every human
surface uses.

**One structural fact about `output.rs` deserves stating up front**, because it
makes the module's size numbers misleading. `output.rs` is the **only** file in
the crate with a `#[cfg(test)]` module *nested inside* production code: the
`log` module's test module at `output.rs:309` sits between `log::line` and the
update document, and a second, small `path_tests` module sits at
`output.rs:273`. The overview's `prod` column therefore reports **272**, which
is the line count *before the first* `#[cfg(test)]` marker — not the count of
production code. 793 lines follow that marker, and only 525 of them are test
code; the `log` module (`output.rs:309-885`) and the update document
(`output.rs:887-943`) live in that tail. Any statement of the form "this file
is N lines of production" read off the first `#[cfg(test)]` is wrong here, and
the boundary has to be checked by hand.

---

## 1. Responsibility

Both modules are **presentation only**. Neither computes a size, classifies
ownership, decides a policy disposition, nor performs I/O.

| | `report.rs` | `output.rs` |
|---|---|---|
| Consumes | `ScanReport`, `HumanOptions` | `ScanReport`, `CleanReport` |
| Produces | `String` (prose) | `EnvelopeV1<T>` (typed DTO tree) |
| Encoding | hand-rolled `format!` | `#[derive(Serialize)]` only — **no serializer** |
| I/O | none | none |

`output.rs` derives `Serialize` for the DTOs and never calls `serde_json` for
them; its `log` module builds strings by `write!` and also never serialises.
(The one exception is the update document, §4, which is hand-assembled as a
`serde_json::Map`.) Encoding and the trailing newline are owned by `main.rs`:
`serde_json::to_string` plus `println!` for the two envelope surfaces
(`main.rs:1659-1667` scan, `main.rs:1186-1195` cleanup), and for log mode a single
`println!` of the returned line (`main.rs:1668-1672`, `main.rs:1196-1200`).
Keeping encoding out of the DTO module means a caller can serialise the same
envelope to JSON, or anything else, without touching the contract.

The `update` document is the one exception, and it became one deliberately in
C022 rather than by accident. It is not an envelope (an update resolves no scope
and runs no cleanup mode), so it is assembled as a literal object map and
encoded inside the module at `output.rs:903`, returning the document alone;
`update_json_stream` (`output.rs:941`) appends the single newline, and `main.rs`
prints exactly that (`main.rs:198-201`) with `print!`. The reason it lives in the
library at all is testability: `update_json` used to be a private function of the
binary, which meant the only way to assert anything about this surface was to run
a live update against crates.io. The seam lets the contract tests hold the exact
bytes, and the stream is a value rather than a `println!` convention.

**What they must not do:** change what was found. That holds outright for
`output.rs` — it takes `&ScanReport`/`&CleanReport` and cannot mutate. It holds
in *content* for `report.rs` but not in *effect*: `render` takes `&mut` and
reorders the caller's group vector (§3). Findings, byte counts, and ownership
classes are untouched; the order the caller sees afterwards is not.

**Dependency asymmetry** (from the import lines):

```
report.rs:1    use crate::domain::*;
output.rs:3    use crate::{cleanup::CleanReport, domain::ScanReport};
```

`report.rs` depends on `domain` alone. `output.rs` depends on `domain` *and*
`cleanup`, because the cleanup contract must name `CleanOutcome`,
`PolicyDisposition`, `CleanupSelector`, and `CleanMode`. The reason is historical
and benign: the human scan renderer was written first and only ever described a
`ScanReport`. When the JSON contract was added it had to cover a second command,
so it reached into the cleanup vocabulary. The `log` module repeats that reach
narrowly (`output.rs:310-311`): it needs `CleanMode`, `CleanOutcome`,
`CleanReport`, and `ScopeBlock` only, never `CleanupSelector`. The two modules
share no code; `output.rs` never calls `report::format_bytes`.

The dependency arrow that does exist points the opposite way from the obvious:
`cleanup.rs:738,713,722,822,830,835,847,854,861-863` all call
`crate::report::format_bytes`. The byte formatter is a shared library function, so
`report.rs` sits *below* `cleanup.rs` and `output.rs` in the layer map — which is why
it needs no cleanup types at all. (`progress.rs` used to sit on that arrow too,
through a one-line local wrapper; M020 retired the renderer's candidate rows and
with them the last production call, so its only remaining reference to the shared
formatter is the test at `progress.rs:829`.)

---

## 2. Two projections, one source

The module comment at `output.rs:1-2` states the intent:

> Versioned, stable machine-output DTOs. Internal domain structs are projected
> explicitly so implementation refactors do not silently change the CLI API.

The mechanism is **hand-written projection**, not derived serialization. Every
field of every `*V1` struct is assigned by hand at the projection site
(`output.rs:106-121`, `122-140`, `142-158`, `167-195`, `211-247`). `Serialize` is
derived on the *DTOs*; it is never derived on `domain` or `cleanup` types. (The
one `Serialize` derive in that layer is on `PolicyDisposition`
`cleanup.rs:411-412`, which the DTO layer does not use — `output.rs:249-260`
converts it by hand instead.)

**Why this matters.** `ScanReport` (`domain.rs:87-93`) and `CleanReport`
(`cleanup.rs:517-543`) are internal, freely refactorable, and full of fields that
are not part of the contract. `ScanReport` alone carries `visited_entries` and a
34-field `ScanCounters` (`domain.rs:307-346`) of prune tallies, Cargo subprocess
counts, and nanosecond timings. Deriving `Serialize` on the domain type would dump all
of it into the public API, and the next person to add a timing counter would
silently break every consumer. It would also make the wire format a function of
the internal model's shape, so renaming `PrivateBounded` to something prettier
would change the JSON.

The projection is also a **normalisation** step: `SizeMetric` and
`DiagnosticCategory` become strings (`output.rs:128-131`, `127-136`),
`SystemTime` becomes `Option<u64>` via a fallible conversion (`output.rs:267`,
`267-271`), and `PathBuf` becomes a `String` through a lossy escape hatch
(`output.rs:262-266`).

### The exclusion property holds

**Definitively: a new field on `ScanReport` does not appear in the JSON.** The
property is structural, not conventional. `ScanV1` (`output.rs:18-23`) is a
separate struct with four named fields, constructed by a struct literal
(`output.rs:148-157`). There is no `#[serde(flatten)]`, no `#[serde(untagged)]`,
no `..Default::default()` widening, no pass-through map. Adding a field to
`domain::ScanReport` is invisible to the wire format unless someone also writes a
line in `output.rs`. Same for `CleanReport` → `CleanupV1` (`output.rs:49-64`).

The one place the discipline is *not* structurally enforced is `CleanupSummaryV1`
(`output.rs:196-199`), which uses `..Default::default()` to fill the six fields
of the summary struct. Five of those are the outcome counters; the sixth is
`diagnostics`, which is always assigned explicitly. A field added to the
summary with a `Default` impl would emit a silent zero. The struct is small and
hand-listed, so the risk is contained — but it is the single spot where a future
field could appear without an explicit assignment.

Deliberately excluded, justified in §6: `ScanCounters`, `visited_entries`,
`artifact_entries`, `CleanReport.counters`, `CleanReport.failed`,
`CleanResult.workspace_roots`, `PhysicalOutputGroup`'s `covering_roots`,
`owners`, `uncertain` — and, newly, `UnresolvedOwnershipParticipant`'s
`capability` and `CleanReport`'s `out_of_scope_unresolved` (§8.3).

---

## 3. Human rendering

### `HumanOptions` — `report.rs:19-26`

M020 split the human surface in two, and `HumanOptions` is where the split is
carried. The struct holds two decisions that travel together — *which shape* the
report takes, and *how paths are spelled*:

```rust
pub struct HumanOptions {
    pub roots: Vec<PathBuf>,
    pub verbose: bool,
}
```

Two constructors:

- `HumanOptions::quiet(roots)` (`report.rs:34-39`) — the **default** shape, with the
  scope's roots as the base for relative paths.
- `HumanOptions::verbose()` (`report.rs:42-47`) — the old shape, with **no** roots, so
  every path stays absolute. That is deliberate: the verbose surface is the
  debugging surface, and an absolute path is unambiguous.

The binary builds the value in exactly one place, `main.rs:1235-1244`: `--verbose`
selects `HumanOptions::verbose()`, everything else selects
`HumanOptions::quiet(roots)`. The roots are a property of the **scope**, not of the
verbosity — `main.rs:1250-1256` for a cleanup (a Full cleanup is the platform, so it
has no single root and its list is empty) and `main.rs:1259-1274` for a scan (the
Full scope is the platform root, so its list is empty too). A scope root is spelled
absolutely before it is used as a base (`main.rs:1281-1283`), because the report's
own paths are absolute and a relative base would never match them.

### The relative-path rule — `report.rs:65-85`

`HumanOptions::relativized` is the whole rule, and it has four cases:

1. **A path under a scope root is shown relative to that root.** The root is
   stripped and the remainder is what is printed.
2. **A path under no scope root stays absolute.** A relative path with no root to
   be relative to is a guess, so the path is left alone (`report.rs:84`).
3. **A remainder of one component is qualified with the scope root's own name.**
   `~/projects/my-app` + `target` reads as `my-app/target`, not as a bare `target`
   (`report.rs:81`). Two reasons, and both are about the operator being able to
   tell the rows apart: a bare `target` says nothing about *which* project it
   belongs to, and two scope roots that each hold a `target` child would produce
   two indistinguishable rows.
4. **A path that *is* the scope root is spelled with the root's own name**
   (`report.rs:79`) — stripping the root would leave an empty path, which renders
   as an empty column rather than the directory it names.

Two edges are worth naming. A remainder of **two or more** components is printed
as-is (`report.rs:69-71`), so the qualification in case 3 never produces
`my-app/my-app/target`. And a scope root with no file name of its own — the
filesystem root — cannot qualify anything, so the bare remainder is returned
(`report.rs:72-76`).

The scope roots are consulted in order and the **first** one that contains the path
wins (`report.rs:66-83`); there is no longest-match rule.

A Full scope has no roots at all — `HumanOptions::quiet(Vec::new())` — so every
path in a Full report stays absolute. A relative path under `/` would name a
fragment, not a place.

### `format_bytes(n: u64) -> String` — `report.rs:109-127`

**Binary (IEC), not decimal.** `UNITS` is `["B", "KiB", "MiB", "GiB", "TiB",
"PiB", "EiB"]` and the divisor is hard-coded `1024` at `report.rs:115`, `121-123`.
There is no `kB`/`MB` in the ladder.

**No float — a deliberate, tested choice.** The comment at `report.rs:111-112`
gives the reason (*"`n as f64` silently loses bytes above 2^53, so the largest
units could not be reported accurately at all"*) and the code matches it. The
whole ladder runs in `u128`:

```rust
let mut hundredths = u128::from(n) * 100;           // report.rs:113
while unit + 1 < UNITS.len() && hundredths >= 1024 * 100 {
    hundredths = (hundredths + 512) / 1024;         // report.rs:116 — round-half-up
    unit += 1;
}
```

Scaling is repeated division on a scaled integer, round-half-up at each step
(`+512` before `/1024`). A second pass (`report.rs:121-124`) catches a value that
rounded onto exactly `1024.00` and promotes the unit rather than printing
`1024.00 KiB`; `report.rs:246` pins `1_048_575` → `1.00 MiB`.

Rendering is two decimal places via `format!("{}.{:02}", hundredths / 100,
hundredths % 100)` (`report.rs:125`) — integer division. **There is no `as f64`
anywhere in the function**, so the `u64 → f64 → string` precision hazard does not
apply. `u64::MAX` renders as `16.00 EiB` (`report.rs:249`): the ladder saturates
at `EiB`, and 2^64 bytes is exactly 16 EiB, so the top of the range is shown at
16.00 rather than overflowing the unit list. The `unit + 1 < UNITS.len()` guards
on both passes mean `UNITS[unit]` at `report.rs:126` can never be out of bounds.

Output is right-aligned to width 6 (`report.rs:126`). That is best-effort: a
7-character value like `1234.56` simply widens the column. Assertions must
`.trim()`, as the tests do at `report.rs:242-249`.

### `render(report: &mut ScanReport, options: &HumanOptions) -> String` — `report.rs:134-168`

The scan report **takes the options** now; it used to take only the report. That
is the whole difference in shape: the group list is unchanged in content, and only
the spelling of the paths and the order of the columns moved.

**Why the `&mut`:** it sorts, and that is the only mutation
(`report.rs:135-139`):

```rust
report.groups.sort_by(|a, b| {
    b.bytes.cmp(&a.bytes)
        .then_with(|| a.display_path.cmp(&b.display_path))
});
```

Nothing else in the function writes to `report`. So this is not normalisation of
content and not an incidental signature slip — it is a function imposing display
order on data it does not own. Judged as design, a real smell: a function named
"render" with a "render" return value should be pure, and the `sort_by` on a
caller's `Vec` is a side effect that outlives the call. It is also load-bearing
for the *other* consumer in a way that is easy to miss:
`tests/end_to_end.rs:193` calls `report::render(&mut report, &HumanOptions::default())`
and then serialises **the same report** at `tests/end_to_end.rs:202`, so the human
renderer's sort determines the JSON group order in that test. In the binary the
two paths are mutually exclusive (`main.rs:1675`, `main.rs:1659-1667`), so shipped
output is unaffected. A `&[EligibleOutputGroup]` parameter plus a local sorted index
would fix it at zero cost.

**Output structure.** No header, no section banner, no trailing rule — a flat
list plus one summary line:

1. One line per group: the path (spelled per the relative-path rule above),
   two spaces, `format_bytes` (width 6), two spaces, `[ownership-label]`
   (`report.rs:152-159`). The path column is width-aligned across the whole table
   (`report.rs:146-151`), so the sizes line up.
2. Blank line, then `"{total} inventory estimate across {n} inactive Cargo
output groups\n"` (`report.rs:162-166`).

**The group list is unchanged in content.** Every eligible output group is still
listed, every ownership label is still shown, the tally still reads *"N inactive
Cargo output groups"*, and the order is still bytes-descending then path-ascending
(`report.rs:136-138`); the tie-break is what makes equal-sized groups
deterministic, pinned at `report.rs:227`. What changed is the **column order** —
the path now comes first — and the **path spelling**, which is relative when the
scope is bounded.

The total is a `fold` with `saturating_add` over group bytes (`report.rs:140-143`),
so it cannot wrap. It is *not* re-derived from `physical_paths`: a group listing
the same canonical path twice is counted once, because grouping deduplicated it
before `bytes` was measured. That is the "deduplicated total" checked at
`report.rs:281-291`.

**Diagnostics are not rendered at all** — `render` uses only `report.groups`.
Diagnostics reach the user from `main.rs:1680-1768` on **stderr**. The shape of
that fan-out has changed twice and is no longer "a count, the first 10, and
`… N more`": Cargo-resolution failures are now grouped by cause (at most eight
distinct causes — `main.rs:1733` — each with up to three example paths —
`main.rs:1698` — then a `… N additional Cargo failure cause(s)` line at
`main.rs:1745`), and *non-Cargo* diagnostics are capped separately at ten
(`main.rs:1749`) with a `… N more non-Cargo diagnostic(s)` line
(`main.rs:1757`). So JSON carries the complete diagnostic list while the
human stream truncates on two independent axes. The human `stdout` document is
not the whole human-visible story. That fan-out block is gated on
`options.format != OutputFormat::Log` (`main.rs:1680`), because log mode carries
`diagnostics=N` in its one line instead; see §7. M020 added a second gate: the
*count* line stays on the default surface, but the diagnostic **detail** is now
verbose-only (`main.rs:1688`).

**Ownership labels** come from `OutputOwnershipClass::label` (`domain.rs:263-270`):
`private`, `external-unproven`, `shared`, `uncertain`; pinned at `report.rs:273-275`.

**Activity state is not rendered per group.** There is no `inactive`/`active`/
`uncertain` word on any line. Groups only reach `ScanReport` if they already
passed the recency and ownership gates in `workspace::analyze_groups`; active and
uncertain groups are filtered upstream. The only activity signal a user reads is
the phrase *"N inactive Cargo output groups"* in the summary line, backed by
`tests/end_to_end.rs:197-198` (a freshly built project is absent).

**What the user reads to decide whether to run `clean`:** the `[ownership]`
label. Per `cleanup.rs:1125-1127` and `cleanup.rs:1259-1267`, only `PrivateBounded`
is ever eligible; `ExternalUnproven`, `Shared`, and `Uncertain` are inventory-only
under any authorization-root configuration. A `[private]` row is a candidate; any
other label is informational. This invariant is *not* restated in the report's
prose.

The summary wording is itself a safety property, asserted twice
(`report.rs:230-239`, `tests/end_to_end.rs:199`): "inventory estimate" is used
for the empty and non-empty cases alike, and neither ever says "reclaimed" or
"recoverable". These are observed bytes, not bytes freed.

### The cleanup report: two shapes — `cleanup.rs:640-991`

`CleanReport::render` takes the options and dispatches on `options.verbose`
(`cleanup.rs:640-646`):

| Shape | Renderer | Selected by |
|---|---|---|
| **Quiet — the default** | `render_quiet` (`cleanup.rs:691-764`) | `HumanOptions::quiet(roots)` |
| Verbose | `render_verbose` (`cleanup.rs:854-991`) | `HumanOptions::verbose()` |

#### The quiet surface

**One row per unit the run acted on** — `Cleaned` (Execute), `Simulated`
(Simulate), `Previewed` (CargoPreview), and `Failed` (`cleanup.rs:706-717`). A
failure is never silent: it is a row, not a footnote.

**Skipped units are not listed.** Their count appears in the tally as
`(N skipped)` (`cleanup.rs:811-813`). This is the point of the quiet surface: a
machine with a thousand Cargo workspaces would otherwise print a thousand lines
saying "skipped" (`report.rs:9-11`).

**The row shows the physical output root** — the directory Cargo emptied — not the
workspace it belongs to (`cleanup.rs:723-731`), spelled per the relative-path rule
above. When a unit has several output roots they are joined with `, `; when it has
none, the workspace path is shown instead. **The size** is the observed decrease
for a `Cleaned` row and the before-size otherwise (`cleanup.rs:732-739`), rendered
as `—` when neither is known. The columns are aligned (`cleanup.rs:743-756`).

**The tally** (`cleanup.rs:771-851`) is one line:

| Mode | Tally |
|---|---|
| Execute | `"{bytes} reclaimed from {n} directories"` |
| Simulate / CargoPreview | `"{bytes} would be reclaimed from {n} directories"` |
| Either, nothing acted on | `"Nothing was cleaned"` / `"Nothing would be cleaned"` |

The tally also carries the mode note: Simulate adds
``(simulation only — no `cargo clean` command was invoked)``, CargoPreview adds
`(preview only — no cleanup executed)` (`cleanup.rs:795-807`), and the counts
`(N failed)` and `(N skipped)` follow (`cleanup.rs:808-813`). The word choice is
the safety property: a simulation's bytes are *not* reclaimed bytes, and the
execute tally is pinned never to borrow the simulation's wording
(`cleanup.rs:6699`).

**With a selector** (`--profile` / `--package`) in Simulate or CargoPreview mode,
the byte figure is the **output union**, not the selector's share — the selector's
own reclaimable size is not measured (`cleanup.rs:826-837`). So the tally says
`"{n} selected; the selector's reclaimable size is not measured (output union
{bytes})"` rather than claiming "would be reclaimed". The JSON projection says
the same thing (§6, D1); the human surface must not over-claim by the whole union.

#### The verbose surface

The old surface, unchanged in shape (`cleanup.rs:854-991`): the `combined scope: …`
census line (`cleanup.rs:856-866`), one row per unit **including the skipped
ones** (`cleanup.rs:887-964`), the ownership class, the output roots, the
before/after figures, the `observed decrease` phrase, the per-unit detail, and the
per-mode summary lines (`cleanup.rs:965-989`) — `cleanup: …`, `simulation: …`,
`cargo preview: …`.

It is selected by `--verbose`, and its paths are **absolute**
(`HumanOptions::verbose()` carries no roots, `report.rs:42-47`).

#### Both shapes keep the ADR 007 safety disclosures

`append_disclosures` (`cleanup.rs:652-688`) is called by both renderers, and it
carries the scope-block message, the unresolved-participant listing, and the
out-of-scope unprovable count. The source says why, and the reason is the right
one: those disclosures say *what the report means*, not how long it is, and
dropping them because the report got shorter would turn "boundary clean" into
"system clean" (`cleanup.rs:636-639`).

### Escaping

`path_text` (`report.rs:94-108`) is the terminal-safety boundary. It converts
lossily (`report.rs:95`), checks `chars().any(char::is_control)`, and if any
control character is present rewrites **every** control character as `\u{hex}`
(`report.rs:102`). Non-UTF-8 bytes degrade to U+FFFD via `to_string_lossy` and
are not re-escaped, since U+FFFD is not a control character. Two tests cover this
(`report.rs:187-197` for ESC/BEL in a valid-UTF-8 name, `report.rs:200-219` for a
raw `\xff`). The invariant: a directory name cannot drive the terminal.

**Both surfaces go through it.** The cleanup report used to have its own private
escaping helper, which quoted non-UTF-8 paths with Rust's `Debug` form and wrote
control characters **raw**. M020 removed it: the quiet surface renders paths
through `HumanOptions::path` (`report.rs:50-52`) and the verbose surface does the
same, so both land in `path_text` and neither can drive a terminal. The helper's
old `Debug` quoting is gone with it — the lossy U+FFFD form is now the only
non-UTF-8 spelling on the human surface.

The machine projection now renders a non-UTF-8 path **identically** — see §6,
"Non-UTF-8 path rendering". The two surfaces no longer disagree here.

---

## 4. The machine envelope

`EnvelopeV1<T>` — `output.rs:7-15`:

| Field | Type | Set by | Notes |
|---|---|---|---|
| `schema_version` | `u32` | `output.rs:9`, `212` | Hard-coded literal `1` at both sites |
| `cargo_cleanme_version` | `&'static str` | `env!("CARGO_PKG_VERSION")` | Compile-time, from `Cargo.toml` |
| `operation` | `&'static str` | `output.rs:11` / `214` | `"scan"` or `"clean"` — and nothing else |
| `scope` | `String` | caller parameter | Passed in by `main.rs`, *not* derived |
| `mode` | `Option<String>` | `output.rs:13` / `216` | `None` for scan, `Some(mode)` for cleanup |
| `result` | `T` | `ScanV1` or `CleanupV1` | The generic parameter |

**The `#[serde(...)]` inventory is empty.** `output.rs` uses a bare
`#[derive(Serialize)]` on every struct — no `rename`, `rename_all`, `default`,
`skip_serializing_if`, `flatten`, or `deny_unknown_fields` anywhere. Two
consequences: JSON field names are the Rust names verbatim, so there is no rename
layer to keep in sync and no snake_case conversion to get wrong; and **`mode` is
always present as a key**, because `Option<String>` without
`skip_serializing_if` serialises `None` as `null`, not as an absent key. A scan
document contains `"mode": null`. `plans/output-schema-v1.md:3` now says exactly
that ("`mode` is always present and is `null` for a scan"), so the old reading
of it as "optional cleanup `mode`" is closed (§8, D6).

**How the version is chosen: it isn't.** `schema_version: 1` is a literal at
`output.rs:143` and `output.rs:212`. No `const`, no feature gate, no
negotiation. A bump is a two-site edit plus a test edit.

**`EnvelopeV1<T>` is a stability device.** The parameter lets one envelope shape
carry two unrelated result trees, so `schema_version` means the same thing for
both commands. A consumer can branch on it: reject anything above the highest
version it knows, and treat an unexpected `operation` as a distinct document type.
The obligation this creates is `plans/output-schema-v1.md:3`: *"A
backwards-incompatible field or meaning change requires a schema-version
decision."* A bump means every consumer that hard-codes a version check must be
revisited, while consumers reading only known keys can ignore it. Field removal
or meaning change is the trigger, not a tool release.

**`mode` presence/absence.** Only two values are reachable: `None` (scan,
`output.rs:147`) and `Some(mode)` (cleanup, `output.rs:216`), where `mode` comes
from `CleanMode::as_str` (`cleanup.rs:50-56`) — `preview`, `simulate`,
`execute`. `Option` is therefore a *discriminator*, not an "unknown" channel: for
`operation == "scan"` the field is not applicable, and no reachable state makes it
`Some`. It is `Option<String>` rather than `Option<&'static str>`, so the cleanup
path allocates for one of three constants (`output.rs:216`).

**`scope` is caller-supplied, not derived.** `main.rs` passes a literal, and M012A
changed what those literals are: the `scope` value now names the scope that was
*resolved*, never the flag that selected it.

| Call site | Value |
|---|---|
| `main.rs:1662-1665` (scan, via `scope_label`) | `"full"`, `"explicit"`, or `"routine"` |
| `main.rs:1189-1194` (cleanup, empty-roots and blocked paths alike) | whatever `resolve_cleanup_roots` resolved — `"routine"`, `"explicit"`, or `"full"` |
| `main.rs:1196-1200` (cleanup, log mode) | the same `resolved.scope_label` |

The set is **uniform between the two commands** — `full`, `explicit`,
`routine` — which it was not before M012A, when cleanup emitted a `known` label
that scan could never produce. Bare `cargo-cleanme` and `clean --known` both
resolve to the maintenance scope and both report `scope: "routine"`, unless a
configured legacy `scan.root` wins as an Explicit override and the label is
`"explicit"` (`main.rs:797-816`). `"known"` is no longer emitted on any path.

**One more envelope that is not an envelope.** `update_json`
(`output.rs:903-931`) builds a hand-rolled `serde_json::Map` with
`schema_version: 1` (`output.rs:905`) and `operation: "update"`
(`output.rs:910`) — but **no `scope`, no `mode`, and a different `result` shape**
(`output.rs:912-929`). It shares the version number without sharing the envelope
(§8, D8). Since C022 it lives in this module rather than in the binary, which
is what made it testable at all.

**And one command with no envelope at all.** `config` refuses `--format json`
outright (`main.rs:238-242`, `AppError::Config` → exit 2), because `config
path` and `config show` are a raw path and TOML text and emitting those on a
JSON stdout would break the one-document automation contract. So the
`operation` value `config` — which `plans/output-schema-v1.md:7` lists among the
possible operations — is **never** produced by an envelope. `EnvelopeV1.operation`
is only ever `"scan"` or `"clean"`.

---

## 5. DTO inventory

All structs are `#[derive(Serialize)]`, all fields `pub`. No `Default` except
`CleanupSummaryV1` (`output.rs:96`).

### `EnvelopeV1<T>` — `output.rs:7-15`
See §4.

### `ScanV1` — `output.rs:18-23`

| Field | Type | Meaning |
|---|---|---|
| `discovered_manifests` | `u64` | `Cargo.toml` manifests found (`ScanReport.discovered`) |
| `groups` | `Vec<ScanGroupV1>` | Inactive, reportable output groups |
| `diagnostics` | `Vec<DiagnosticV1>` | Complete, untruncated diagnostic list |
| `summary` | `ScanSummaryV1` | Aggregate counts |

### `ScanGroupV1` — `output.rs:25-33`

| Field | Type | Meaning |
|---|---|---|
| `display_path` | `String` | Primary display path (first covering root) |
| `workspace_roots` | `Vec<String>` | Workspaces that own this group |
| `physical_paths` | `Vec<String>` | All canonical paths contributing, deduplicated |
| `bytes` | `u64` | Measured size, exact; formatting is a consumer concern |
| `size_metric` | `&'static str` | `allocated` or `apparent` |
| `newest_activity_unix_seconds` | `Option<u64>` | Newest mtime as Unix seconds |
| `ownership` | `&'static str` | `private` / `external-unproven` / `shared` / `uncertain` |

`newest_activity_unix_seconds` is `None` when the group has no known mtime
(`output.rs:118`) **or** when the `SystemTime` predates the Unix epoch
(`output.rs:267-271`, `duration_since(...).ok()`). Those two "unknown" causes are
indistinguishable to a consumer.

### `DiagnosticV1` — `output.rs:35-40`

| Field | Type | Meaning |
|---|---|---|
| `severity` | `String` | `info` / `warning` / `error` (`DiagnosticSeverity::as_str`, `domain.rs:59-65`) |
| `category` | `String` | **Seven**-value snake_case set (`output.rs:37-46`) |
| `path` | `Option<String>` | Subject of the diagnostic, when path-scoped |
| `message` | `String` | Human explanation, not a stable token |

`path: None` means "not path-scoped" (e.g. a Cargo metadata failure), not "path
unknown". `message` is free text by design and the one field a consumer must not
match on.

The category set grew with ADR 007: `permission_denied`, `vanished`, `metadata`,
`invalid_entry`, `platform_root`, `candidate_uncertain`, and **`out_of_scope`**
(`domain.rs:68-78`, projected at `output.rs:134`). `out_of_scope` is the
category for an unprovable participant *outside* the selected proof boundary —
reported, never blocking. It is informational by construction, and it is the
only machine-visible trace of ADR 007's "the system is not clean, the boundary
is" rule in the **scan** document.

### `ScanSummaryV1` — `output.rs:42-46`

| Field | Type | Meaning |
|---|---|---|
| `group_count` | `usize` | `groups.len()`, computed at projection |
| `inventory_bytes` | `u64` | Saturating sum of group bytes; **observed, not recovered** |
| `diagnostic_count` | `usize` | `diagnostics.len()` |

### `CleanupV1` — `output.rs:49-64`

| Field | Type | Meaning |
|---|---|---|
| `discovered_manifests` | `usize` | Note `usize` here vs `u64` in `ScanV1` |
| `resolved_workspaces` | `usize` | Workspaces that resolved authoritatively |
| `units_considered` | `usize` | CleanupUnits sized, **before** per-unit outcomes |
| `scope_blocked` | `bool` | Boundary-scoped block; from `Option::is_some()`. Set when the selected scope cannot be proven (in-boundary poison, zero-unit edge) or a mid-run gate fires; out-of-boundary poison never sets it, so exit 1 falls out structurally only for the boundary |
| `scope_reason` | `Option<String>` | Prose reason for the block, cloned from `ScopeBlock.message` |
| `unresolved_ownership` | `usize` | Count of boundary-relevant unresolved manifests; out-of-boundary unprovables travel in `diagnostics` as `out_of_scope` |
| `unresolved_participants` | `Vec<UnresolvedParticipantV1>` | Per-manifest detail (boundary-relevant only) |
| `selected_roots` | `Vec<String>` | Bounded cleanup roots in effect |
| `effective_policy` | `Option<PolicyV1>` | Policy after CLI-over-config merge |
| `selector_kind` | `Option<&'static str>` | `profile` or `package` |
| `selector_value` | `Option<String>` | The profile/package spec |
| `state_generation_last_full_at` | `Option<u64>` | Discovery-state generation; metadata only |
| `units` | `Vec<CleanupUnitV1>` | Per-CleanupUnit results |
| `summary` | `CleanupSummaryV1` | Outcome tallies |

`scope_blocked: false` with `scope_reason: null` is the normal state. `true`
implies a non-null `scope_reason`, since both read the same `Option<ScopeBlock>`
(`output.rs:221-222`). **M012A changed the domain type this splits.**
`CleanReport.scope_blocked` is `Option<ScopeBlock>` (`cleanup.rs:527`), a struct
of `{ reason: ScopeBlockReason, message: String }` (`cleanup.rs:452-455`) — no
longer `Option<String>`. The DTO is unchanged: `output.rs:222` still projects
`b.message.clone()`, the human prose. **The typed `reason` is dropped from JSON
entirely.** `output::log::cleanup` does read it (`output.rs:392-394`), which is
where a consumer of a retained history line gets `reason=ownership_unproven` or
`reason=incomplete_discovery`; a JSON consumer must parse prose to recover the
same information. That asymmetry is worth naming: the machine contract is the
*less* machine-readable of the two surfaces here.

`effective_policy: None` is reachable only if the policy were omitted from the
`CleanReport` construction; live paths always set it (`cleanup.rs:1651`).

**`out_of_scope_unresolved` has no field here.** `CleanReport` carries it
(`cleanup.rs:536`) and the human renderer prints it (`cleanup.rs:657-662`), but
`CleanupV1` has nowhere to put it. See §8.3.

### `UnresolvedParticipantV1` — `output.rs:68-72`

| Field | Type | Meaning |
|---|---|---|
| `manifest` | `String` | The manifest that failed to resolve |
| `stage` | `&'static str` | Resolution stage, passed through unvalidated (`output.rs:70`) |
| `reason` | `String` | Prose diagnostic reason |

`stage` is a bare `&'static str` with no type-level enumeration in `output.rs` or
the schema document. I did not trace its producer in `discovery.rs`/`workspace.rs`,
so the legal value set is **not determined here**.

**The `capability` field is missing.** The domain type carries four fields
(`domain.rs:194-199`): `manifest`, `stage`, `reason`, and
`capability: UnknownCapability`. The DTO projects three. See §8.3 — this is the
most consequential gap in the machine contract.

### `PolicyV1` — `output.rs:74-79`

| Field | Type | Meaning |
|---|---|---|
| `min_reclaimable_bytes` | `u64` | Size floor; `0` means no floor |
| `min_inactive_seconds` | `Option<u64>` | Recency floor; `None` = none configured |
| `include` | `Vec<String>` | Workspace-root include patterns |
| `exclude` | `Vec<String>` | Workspace-root exclude patterns |

`min_inactive_seconds: None` is a real configuration state, not a measurement
failure — it means no recency policy is in force.

### `CleanupUnitV1` — `output.rs:81-94`

| Field | Type | Meaning |
|---|---|---|
| `workspace_root` | `String` | Unit's workspace root, from `CleanResult.display_path` |
| `output_roots` | `Vec<String>` | Every physical covering root the invocation can affect |
| `ownership` | `&'static str` | Ownership class of the unit |
| `policy_disposition` | `Option<String>` | Six-value typed disposition; `None` when no policy evaluated the unit |
| `outcome` | `String` | `previewed`/`simulated`/`cleaned`/`skipped`/`failed` |
| `reason_code` | `String` | **Twenty**-value stable machine code |
| `before_bytes` | `Option<u64>` | **`None` whenever a selector is active** |
| `selector_estimate_bytes` | `Option<u64>` | Non-null only when **no** selector is active (§8, D1) |
| `output_union_before_bytes` | `Option<u64>` | Whole-output context; never a reclaimable estimate |
| `after_bytes` | `Option<u64>` | Post-clean measurement, when taken |
| `observed_decrease_bytes` | `Option<u64>` | Measured delta, when taken |
| `detail` | `String` | Human prose; **always present, possibly `""`** — not `Option` |

The `Option` fields split into two groups. `None` = **not applicable to this run's
mode** (`after_bytes`/`observed_decrease_bytes` in preview or simulate;
`before_bytes`/`selector_estimate_bytes` under a selector) versus `None` = **not
measured** (`before_bytes` when `CleanResult.before_bytes` was `None`). A consumer
cannot tell those apart for `before_bytes`; `selector_kind` is the only
disambiguator, and only for the selector case.

### `CleanupSummaryV1` — `output.rs:96-103`

| Field | Type | Meaning |
|---|---|---|
| `previewed` | `usize` | Units outcome `previewed` |
| `simulated` | `usize` | Units outcome `simulated` |
| `cleaned` | `usize` | Units outcome `cleaned` |
| `skipped` | `usize` | Units outcome `skipped` |
| `failed` | `usize` | Units outcome `failed` — **recomputed**, see §8, D10 |
| `diagnostics` | `usize` | Copied from `CleanReport.diagnostics` |

The five outcome counters are tallied by matching `CleanOutcome`
(`output.rs:200-209`), not read from the domain. They are mutually exclusive and
every result gets exactly one, so they sum to `units.len()`.

---

## 6. Field mapping

### `ScanReport` → `ScanV1`

| Domain field | DTO field | Transform |
|---|---|---|
| `discovered: u64` | `discovered_manifests: u64` | direct |
| `groups: Vec<EligibleOutputGroup>` | `groups: Vec<ScanGroupV1>` | per-element projection |
| `diagnostics: Vec<ScanDiagnostic>` | `diagnostics: Vec<DiagnosticV1>` | per-element projection |
| — | `summary.group_count` | **computed**: `groups.len()` |
| — | `summary.inventory_bytes` | **computed**: `saturating_add` fold |
| — | `summary.diagnostic_count` | **computed**: `diagnostics.len()` |
| `visited_entries: u64` | *dropped* | — |
| `counters: ScanCounters` | *dropped* | — |

`ScanReport` has no summary field; `ScanSummaryV1` is entirely computed at
projection time. The fold (`output.rs:141`) sums the *projected* DTOs rather than
the domain structs — same values, but the JSON total and the human total are
computed by two separate folds over two separate collections (`output.rs:141`,
`report.rs:140-143`) that happen to agree.

### `EligibleOutputGroup` → `ScanGroupV1`

The source type is **`EligibleOutputGroup` (`domain.rs:291-300`)**, not
`PhysicalOutputGroup`. `PhysicalOutputGroup` (`domain.rs:274-288`) is the
*measurement-stage* type and never reaches either output module — it is confined
to `workspace.rs` (1526, 1826, 1862-1863, 1945) plus one `cleanup.rs` test
fixture (`cleanup.rs:9188`). `main.rs:1626-1644` and `tests/end_to_end.rs:174-192`
both perform the narrowing conversion, dropping `covering_roots`, `owners`, and
`uncertain`, and lowering `owners: Vec<WorkspaceId>` to
`workspace_roots: Vec<PathBuf>` by taking `.0` of each id. `report.rs` and
`output.rs` never see the richer type and could not emit its fields even in
principle.

| Domain field | DTO field | Transform |
|---|---|---|
| `display_path: PathBuf` | `display_path: String` | `output::path` — lossy |
| `workspace_roots: Vec<PathBuf>` | `workspace_roots: Vec<String>` | per-element `output::path` |
| `physical_paths: Vec<PathBuf>` | `physical_paths: Vec<String>` | per-element `output::path` |
| `bytes: u64` | `bytes: u64` | direct — number, not a formatted string |
| `metric: SizeMetric` | `size_metric: &'static str` | enum → `"allocated"`/`"apparent"` |
| `newest_mtime: Option<SystemTime>` | `newest_activity_unix_seconds: Option<u64>` | `SystemTime` → Unix seconds, fallible |
| `ownership: OutputOwnershipClass` | `ownership: &'static str` | `.label()` (`domain.rs:263-270`) |
| `artifact_entries: u64` | *dropped* | — |

**Type change worth naming:** the human renderer formats bytes into
`"  1.50 MiB"`; the machine projection emits a raw `u64`. Correct — byte
formatting is presentation and belongs to the consumer, and precisely why
`format_bytes` is not shared with `output.rs`.

**`output::path` is lossy and irreversible** (`output.rs:262-266`):
`p.to_string_lossy().into_owned()`. Non-UTF-8 bytes become U+FFFD. A consumer
must not use these strings as join keys, because the mapping is not injective.

> **This changed.** The projection used to be
> `to_str()` if representable, else `format!("{:?}", p.as_os_str())` — a Rust
> *debug* form. It is now lossy in exactly the same way the human renderer is
> (`report.rs:95`), and the source says why: *"JSON must be valid UTF-8: lossy
> `U+FFFD` like the human report, never Rust `Debug` quoting."* The
> `path_tests` module at `output.rs:274-300` pins both halves: a non-UTF-8 path
> contains U+FFFD and contains no `"` and no `\x` escape
> (`output.rs:282-294`), and a UTF-8 path passes through unchanged
> (`output.rs:297-299`).
>
> The consequence: **the two projections now render the same non-UTF-8 path
> identically** — machine `"/tmp/bad-\u{FFFD}"`, human `"/tmp/bad-\u{FFFD}"` —
> and the "human and JSON disagree about non-UTF-8 paths" entry in §9 is
> closed. `plans/output-schema-v1.md:53` still says *"the display string uses
> the platform escaped debug form"*; that sentence is now stale (§8.3, G1).

### `ScanDiagnostic` → `DiagnosticV1`

| Domain field | DTO field | Transform |
|---|---|---|
| `severity: DiagnosticSeverity` | `severity: String` | `.as_str()` (`domain.rs:59-65`) |
| `category: DiagnosticCategory` | `category: String` | 7-arm `match` (`output.rs:127-136`) |
| `path: Option<PathBuf>` | `path: Option<String>` | `.as_deref().map(path)` |
| `message: String` | `message: String` | `.clone()` |

Both enum conversions are hand-written. `DiagnosticSeverity::as_str` carries the
comment *"Stable machine label; the JSON contract must not depend on `Debug`"*
(`domain.rs:58`); `DiagnosticCategory` has no such helper, so its mapping lives
inline in the projection. The same stability argument applies to both, but only one
was hoisted into the domain type.

### `CleanReport` → `CleanupV1`

| Domain field | DTO field | Transform |
|---|---|---|
| `discovered_manifests: usize` | `discovered_manifests: usize` | direct (note `u64` in scan) |
| `resolved_workspaces: usize` | `resolved_workspaces: usize` | direct |
| `units_considered: usize` | `units_considered: usize` | direct |
| `scope_blocked: Option<ScopeBlock>` | `scope_blocked: bool` + `scope_reason` | **split**: `bool` from `is_some()`, string from `.message`; `reason` dropped |
| `unresolved_ownership: Vec<…>` | `unresolved_ownership: usize` | **count only** |
| `unresolved_ownership: Vec<…>` | `unresolved_participants: Vec<…>` | **same data twice** |
| `selected_roots: Vec<PathBuf>` | `selected_roots: Vec<String>` | per-element `output::path` |
| `effective_policy: Option<CleanupPolicy>` | `effective_policy: Option<PolicyV1>` | struct projection |
| `selector: Option<CleanupSelector>` | `selector_kind` + `selector_value` | **one enum split into two keys** |
| `results: Vec<CleanResult>` | `units: Vec<CleanupUnitV1>` | per-element projection |
| `mode: CleanMode` | envelope `mode: Option<String>` | `.as_str()` (`cleanup.rs:50-56`) |
| `diagnostics: usize` | `summary.diagnostics: usize` | direct |
| `out_of_scope_unresolved: usize` | *dropped* | see §8.3, G4 |
| — | `summary.{previewed,…,failed}` | **computed**: counted from `results` |
| `failed: usize` | *dropped* (recomputed) | see §8, D10 |
| `counters: ScanCounters` | *dropped* | — |

**The `scope_blocked` split is a real semantic change.** The domain carries a typed
`ScopeBlock`; the contract carries a `bool` and a nullable *prose* string. The
`bool` is `is_some()` (`output.rs:221`) and the string is a clone of the same
block's `message` (`output.rs:39`), so they cannot disagree — but the boolean is
*derived state*, and the enum half of the domain value is discarded. Before M012A
the domain value was itself a bare `Option<String>`, so nothing was lost at the
boundary; now the projection is lossy in a way the DTO cannot express.

**`selector` becomes two keys.** `CleanupSelector` is a two-variant enum
(`cleanup.rs:546-549`); `kind()` and `value()` (`cleanup.rs:552-562`) are applied
independently at `output.rs:240-241`, and both are `None` together. The string
form is not a serde tag, which is why there is no `#[serde(tag = ...)]` here.

### Policy, selector, and the code enums

| Domain | DTO | Transform |
|---|---|---|
| `CleanupPolicy` | `PolicyV1` | all four fields direct; no loss |
| `CleanupSelector::Profile(s)`/`::Package(s)` | `selector_kind` + `selector_value` | enum split; **lossless** |
| `CleanupReasonCode` (21 variants) | `reason_code: String` | `.as_str()` (`cleanup.rs:384-405`) |
| `PolicyDisposition` (6 variants) | `policy_disposition: Option<String>` | hand-written `policy_code` (`output.rs:249-260`) |
| `CleanOutcome` (5 variants) | `outcome: String` | `.as_str()` (`cleanup.rs:376-382`) |

Note the inconsistency in how these three enums reach the wire. `CleanOutcome` and
`CleanupReasonCode` go through `as_str()` methods on the type;
`PolicyDisposition` — which *has* `#[derive(Serialize)]` with
`rename_all = "snake_case"` available at `cleanup.rs:411-412` — is converted by a
private helper instead. The helper produces the same six strings serde would, so
behaviour matches; the helper is simply where a seventh variant would fail
visibly in the contract layer, which is arguably the point.

**The reason-code set is twenty-one, not twenty.** `CleanupReasonCode`
(`cleanup.rs:306-341`) gained `SkippedOutsideSelection` with M014: the unit was
discovered, resolved, sized and proven, but the operator's selection removed it
from the candidate set. It is deliberately *not* an ownership or safety code —
it exists so that a selection can never produce a silent no-op the way a
configured `scan.root` once made `clean --known`. Its machine token is
`skipped_outside_selection` (`cleanup.rs:401`), and it is pinned by
`tests/cli_contract.rs:1494`. `plans/output-schema-v1.md:48` still lists only
nineteen values (§8.3, G2).

**M018 added a twenty-first: `SkippedIncompleteCoverage`
(`cleanup.rs:341`), spelled `skipped_incomplete_coverage`
(`cleanup.rs:402`).** It says: the unit was under a learned root whose own
traversal did **not** complete, so its ownership could not be proven and it was
removed from the candidate set by the coverage exclusion
(`cleanup.rs:232-251`). It is distinct from `SkippedOwnershipUnproven` on
purpose. `SkippedOwnershipUnproven` means *this unit's* ownership proof failed;
`SkippedIncompleteCoverage` means *the root's* subtree was never fully read, so
the unit was never a candidate in the first place. The two are different
questions about different objects, and a consumer reading the reason code can
tell which one it is looking at.

The distinction is also what keeps the exclusion honest: a unit removed by the
coverage filter is still discovered, resolved, sized and classified — it is
removed from the *candidate* set, not from the discovery set — so it is
reported, never dropped.

### `UnresolvedOwnershipParticipant` → `UnresolvedParticipantV1`

| Domain | DTO | Transform |
|---|---|---|
| `manifest: PathBuf` | `manifest: String` | `output::path` (lossy) |
| `stage: &'static str` | `stage: &'static str` | **verbatim pass-through** |
| `reason: String` | `reason: String` | `.clone()` |
| `capability: UnknownCapability` | **nothing** | **dropped** — see §8.3, G3 |

`stage` is the only field in the whole contract that crosses the boundary
*unconverted*. The type permits any `&'static str`; nothing in `output.rs` or the
schema document constrains it.

### `CleanResult` → `CleanupUnitV1`

| Domain field | DTO field | Transform |
|---|---|---|
| `display_path: PathBuf` | `workspace_root: String` | **renamed** + lossy |
| `output_roots: Vec<PathBuf>` | `output_roots: Vec<String>` | per-element lossy |
| `ownership` | `ownership: &'static str` | `.label()` |
| `policy_disposition: Option<PolicyDisposition>` | `policy_disposition: Option<String>` | `policy_code` |
| `outcome: CleanOutcome` | `outcome: String` | `.as_str()` |
| `reason_code: CleanupReasonCode` | `reason_code: String` | `.as_str()` |
| `before_bytes: Option<u64>` | `before_bytes` | **`None` if `report.selector.is_some()`** |
| `before_bytes: Option<u64>` | `selector_estimate_bytes` | **`Some` iff `!report.selector.is_some()`** |
| `before_bytes: Option<u64>` | `output_union_before_bytes` | **always `r.before_bytes`** |
| `after_bytes: Option<u64>` | `after_bytes: Option<u64>` | direct |
| `observed_decrease: Option<u64>` | `observed_decrease_bytes: Option<u64>` | renamed |
| `detail: String` | `detail: String` | `.clone()` |
| `workspace_roots: Vec<PathBuf>` | *dropped* | superseded by `output_roots` |

`before_bytes` is projected into **three** keys, gated on
`report.selector.is_some()` per result (`output.rs:177-190`):

| Run shape | `before_bytes` | `selector_estimate_bytes` | `output_union_before_bytes` |
|---|---|---|---|
| Selector active | `null` | `null` | value |
| No selector | value | **value** | value |

Without a selector all three carry the identical `u64`; `selector_estimate_bytes`
is then a "selector estimate" for a run that has no selector. A selector run
cannot attribute bytes to the selector, so the estimate reads `null`, while the
output-union measurement is known in both modes (matching the human
"output-union before" line).

### Dropped domain fields, and why that is correct

| Dropped | Where | Why correct |
|---|---|---|
| `ScanReport.visited_entries` | `domain.rs:91` | Internal walk accounting; no consumer decision depends on it |
| `ScanReport.counters` | `domain.rs:92` | Prune tallies and nanosecond timings are `--stats` stderr material (`main.rs:1772-1789`); the schema keeps them off the contract |
| `EligibleOutputGroup.artifact_entries` | `domain.rs:286` | Count of walked entries; diagnostic, not a reclaim-decision input |
| `PhysicalOutputGroup.covering_roots` / `owners` / `uncertain` | `domain.rs:276`, `236`, `242` | Never reach these modules — dropped upstream at `main.rs:1626-1644` |
| `CleanReport.counters` | `cleanup.rs:522` | Same as `ScanReport.counters`; cleanup `--stats` is stderr (`main.rs:506-519`) |
| `CleanReport.failed` | `cleanup.rs:520` | Recomputed from results (`output.rs:200-209`); the domain field remains an exit-code input |
| `CleanResult.workspace_roots` | `cleanup.rs:291` | Redundant with `workspace_root` + `output_roots` |
| `ScopeBlock.reason` | `cleanup.rs:453` | Typed block code; JSON carries only `.message`. Read by log mode (`output.rs:387`) |
| `PathBuf` types generally | — | Contract is `String`; identity is deliberately not re-derivable |

All are either `--stats` stderr material or measurement internals. The schema
document covers the principle for the one that reaches it at line 35: *"Inventory
bytes describe observed output and are not recovered bytes."*

---

## 7. The log surface — `output.rs:309-885`

Added in M012B. The module's own doc comment states its status precisely
(`output.rs:302-308`):

> This is **not** a third machine contract. JSON remains the complete,
> versioned one; this module exists because a retained scheduler-history tail
> is 512 bytes and a JSON envelope does not fit in it. Everything here is
> projected from report state that already exists — no walk, no Cargo, no
> state re-read, no selector arithmetic.

That last clause is the load-bearing design property and it is verifiable: every
field is read from an already-built `ScanReport` or `CleanReport`. There is no
second measurement, so the two surfaces cannot disagree about a number.

### The line format

`line()` (`output.rs:470-507`) joins `key=value` pairs under a fixed prefix
(`output.rs:321`) and splits them into two classes:

- **Structural** fields are emitted unconditionally, in the order given. A
`debug_assert!` (`output.rs:471-478`) pins them to the fixed set
`op | status | scope | mode`, each a single non-empty whitespace-free token.
- **Optional** fields are appended **while the result fits** `MAX_BYTES`
(`output.rs:319`, a `pub const` = 384) and **dropped whole** once it does not
(`output.rs:494-501`). Nothing is ever truncated.

Dropping whole rather than truncating is the correct trade here, and it is also
the honest one: a truncated `reason=` would produce a *different, still-parseable*
token, whereas an absent field is unambiguous. The cost is that a line can be
structurally valid and materially incomplete — the worst case is an operator
losing `reclaimed_bytes`, not losing `status=blocked`. The ordering
(`output.rs:385-404`) is chosen for that failure mode: `reason` first among the
optionals, because a typed code worth alerting on must survive a crowded line
while a byte count need not.

**Every operation has a line.** `config` and `update` were the gap: `op` is
documented to admit both (`docs/AUTOMATION.md:67`) but `main.rs` had no `Log`
branch for either, so `--format log config path` printed a raw path on stdout and
`--format log config show` printed the config text — precisely the "no paths, no
config text" the format refuses (`docs/AUTOMATION.md:45-48`). Both commands still
do their job in human form; in log mode they report the action and nothing about
what it produced (`main.rs:251`, `251`, `288`; `main.rs:206-214`). `config`'s
`action` is the one honest resolution: the *content* of `config path` **is** the
path, so a format that forbids paths cannot also answer that command.

**ASCII is a contract here, not an aspiration.** No value is quoted, so the
format depends on values containing no space, quote, or newline. Rather than
sanitise, the module commits to sources that are already ASCII tokens — enum
labels, integers, and the resolved scope string. That is why no path, no
`detail`, and no prose message ever reaches a log line, and why the `main.rs`
fatal path passes only a typed `error_code` (`main.rs:27-30`) rather than the
`AppError` display.

### Fields per line

| Function | Line | Structural | Optional, in order |
|---|---|---|---|
| `scan` | `output.rs:105-159` | `op`, `status`, `scope` | `manifests`, `groups`, `bytes`, `diagnostics` |
| `cleanup` | `output.rs:161-247` | `op`, `status`, `scope`, `mode` | `reason` (only when blocked), `cleaned`, `skipped`, `failed`, `reclaimed_bytes` (Execute only), `diagnostics` |
| `fatal` | `output.rs:422-427` | `op`, `status` | `reason` |
| `config` | `output.rs:436-441` | `op`, `status` | `action` (`path` / `show` / `edit`) |
| `update` | `output.rs:449-462` | `op`, `status` | `from`, `to`, `target`, `mode` (`simulate` / `execute`) |

Three semantics are worth stating because they are *narrower* than the JSON:

1. **`status` is computed, not read** (`output.rs:337`, `371-377`). For a scan
it is `failed` when the caller says the Full traversal did not complete, and `ok`
otherwise — the caller computes `full_incomplete` from the typed diagnostic set
(`main.rs:1624`) and passes it in (`main.rs:1671`), so the line cannot disagree
with the exit code it accompanies. For a cleanup, `blocked` outranks `failed`,
which outranks `ok`, and cleanup consults both `results[].outcome` and the domain
`failed` counter. There is no domain field for this; it is derived here, which
means a third consumer could derive it differently.
2. **`reclaimed_bytes` is omitted, not zeroed** (`output.rs:399-401`). A Simulate
or Cargo-preview run recovered nothing, and a blocked scope cleaned nothing;
printing `reclaimed_bytes=0` would present an unmeasured value as a
measurement. This is the opposite of `CleanupUnitV1`, where the same fact
arrives as `observed_decrease_bytes: null`. The condition is exact: the mode is
`Execute` **and** the scope is unblocked **or** at least one unit executed — a
blocked scope that had already cleaned a unit still reports its measured
decrease.
3. **`diagnostics` is a count** and nothing else (`output.rs:156-159`), summing
`report.diagnostics` with the unresolved-ownership participants. It is the
only record of diagnostics on this surface, which is exactly why `main.rs`
suppresses the human fan-out in log mode (`main.rs:1680`) rather than merely
adding a line.

### How `main.rs` selects it

Three dispatch points, all `format == OutputFormat::Log` arms:

| Surface | Emission site |
|---|---|
| scan | `main.rs:1668-1672` |
| cleanup, including the empty-roots and blocked short circuits | `main.rs:1196-1200`, reached from `main.rs:417`, `main.rs:442-448`, and `main.rs:505` |
| fatal error before any report exists | `main.rs:22-30` |

The fatal path is the reason `main()` parses argv itself rather than leaving it
inside `run` (`main.rs:11-18`). To report a failure format-aware you must know the
format before the operation runs, and a `try_parse` inside `run` could not
distinguish "clap rejected this" from "the filesystem is broken". `main` therefore
captures `cli.invocation()` **before** the move into `run` (`main.rs:17`) purely
so `operation_name` (`main.rs:41-50`) can name the operation on the failure line.
The cost is that clap's own usage errors still bypass this path entirely and exit
through clap — unchanged from before.

Progress is disabled in log mode for free: both renderers gate on
`format == OutputFormat::Human` (`main.rs:474-475`, `main.rs:1408-1409`).
`--stats` is the explicit exception — it still writes its detailed line to stderr
in log mode (`main.rs:506-519`, `main.rs:1772-1789`) because the operator asked
for it.

### Honest limits

- **It is not versioned.** No `schema_version`, no negotiation, no stability
promise. A field added or removed is silent. This is deliberate and stated
(`plans/output-schema-v1.md:55-59` says so in the schema document itself), but
it means a consumer who scrapes `status=` is on an unhonoured surface.
- **`bytes` and `reclaimed_bytes` are raw `u64`**, unformatted — consistent with
JSON (§6) and inconsistent with the human surface, which prints `1.50 MiB`.
- **Field presence is data-dependent**, which is the sharpest edge. The same
command emits different key sets depending on whether a scope blocked, which
mode ran, and how long the values are. A consumer doing `line.split()` gets a
variable-length record. The module's tests pin two exact lines for the common
shapes (`output.rs:585-588`, `605-608`), which is the right place for that
promise to live.
- **`status=ok` on a scan is not a health claim.** A scan whose diagnostics were
all `PermissionDenied` reports `status=ok` and exits 0 (the `failed` status is
reserved for an incomplete Full traversal). The log surface carries
`diagnostics=N`, and nothing says whether those diagnostics were benign.
- **The optional-field order is not what the operator documentation shows.**
  `docs/AUTOMATION.md:39` prints a blocked cleanup line with `reason` *after*
  `failed`; the code emits `reason` first (`output.rs:71-73`). The
  documentation example is stale; the code is the contract, and the line is
  order-sensitive for a `key=value` parser.

---

## 8. Schema conformance

Compared against `plans/output-schema-v1.md` (61 lines, rewritten since the
previous revision of this document). The schema document now states the
implementation's actual behaviour, so most of the divergences this section used
to catalogue are closed. §8.1 is the coverage map, §8.2 is the historical
record, and §8.3 lists what is still open.

### 8.1 Coverage map

| Contract element | In code? | Notes |
|---|---|---|
| `schema_version` | yes | `output.rs:9,143,212`; literal `1`, two sites |
| `cargo_cleanme_version` | yes | `env!("CARGO_PKG_VERSION")` |
| `operation` | yes | `scan` / `clean` only — `config` refuses JSON, `update` is not an envelope |
| `scope` | yes | caller-supplied; value set is `full` / `explicit` / `routine` |
| "optional cleanup `mode`" | yes | always emitted, `null` for scan — the document now says "always present" |
| `result` | yes | generic parameter |
| Scan `discovered_manifests` | yes | |
| Scan `groups` (7 keys) | yes | all 7 present, names match exactly |
| Scan `diagnostics` (4 keys) | yes | |
| Scan `summary` (3 keys) | yes | |
| Cleanup scope-coverage trio | yes | `discovered_manifests` / `resolved_workspaces` / `units_considered` |
| Cleanup `selected_roots` | yes | |
| Cleanup `effective_policy` | yes | 4 inner fields now enumerated |
| Cleanup `state_generation_last_full_at` | yes | |
| Cleanup `selector_kind` / `selector_value` | yes | |
| `selector_estimate_bytes` nullity rule | yes | the document now describes the code's rule |
| `output_union_before_bytes` semantics | yes | `r.before_bytes` in both modes |
| `before_bytes` null for selector cleanup | yes | |
| Cleanup `scope_blocked` / `scope_reason` / `unresolved_ownership` | yes | |
| Cleanup `unresolved_participants` (3 keys) | yes | but see G3 |
| Cleanup `units` keys | yes | 12 keys; `detail` optionality corrected in the document |
| `unresolved_ownership` / `effective_policy` / `state_generation_last_full_at` | yes | present in the DTO, but **no value assertion anywhere** in the integration tests |
| `reason_code` value list | **partial** | the document lists 19, the code emits 20 → **G2** |
| `units[].outcome` value set | yes | now enumerated in the document |
| `summary` (6 keys) | yes | `failed` source now described |
| Policy disposition stable values (6 listed) | yes | |
| "one UTF-8 JSON document + newline" | yes | `println!` `main.rs:1660`, `1034`; asserted `tests/cli_contract.rs:199,731` |
| Path encoding rule (line 53) | **no** | the document describes the debug form; the code is lossy → **G1** |
| "`--stats` is stderr-only" | yes | `main.rs:506`, `1585`; asserted `tests/cli_contract.rs:193` |
| "Progress is disabled for JSON output" | yes | `main.rs:474-475,1246-1247` gate on `Human` — which also disables it for log |
| Exit codes 0 / 1 / 2 | **partial** | `config edit` → 1 and `clean --full` incomplete scan → 2 are unlisted → **G6** |
| Success/failure indicator | **absent** | no such field anywhere → **D8** |
| `out_of_scope_unresolved` | **absent** | in the human report, not in the JSON → **G4** |
| `UnresolvedParticipantV1.capability` | **absent** | in the domain, not in the JSON → **G3** |

### 8.2 Divergences, document → code

**D1 — `selector_estimate_bytes` nullity is inverted relative to the document.**
The previous revision of the schema document promised: *"A selector's
`selector_estimate_bytes` is `null` unless a trustworthy estimate exists."* The
code (`output.rs:177-190`) made it `null` **iff a selector is active**, inverting
both halves:

- For a selector run it is *always* `null`. The "unless a trustworthy estimate
exists" escape hatch has **no implementation** — no reachable state emits a
non-null estimate for a selector run. (Consistent with
`plans/output-schema-v1.md:51`, "Any nonzero minimum-size policy with a selector
is rejected because selector-specific bytes are unknown", but the document
promised a possibility the code never exercised.)
- For a **non**-selector run it is **populated** with `r.before_bytes` — a
"selector estimate" emitted for a cleanup with no selector. The document never
described this case.

Consequence: the field is only meaningful as a *negative* signal. Its presence
tells you nothing; its absence tells you either "a selector was used" or "no
trustworthy estimate was available". A consumer reading it as selector bytes for a
selector run gets `null` and correctly refuses to estimate; a consumer reading it
on a plain run gets a duplicate of `output_union_before_bytes` and may mistake it
for something selector-specific. Pinned by `tests/cli_contract.rs:879-886`.

**Status: closed.** The schema document now states the code's rule verbatim
(`plans/output-schema-v1.md:43`): *"`selector_estimate_bytes` is `null`
**exactly when a selector is active** … On a cleanup with **no** selector it
carries the whole-output `before_bytes`. Treat its absence as ambiguous … do not
read its presence as selector-specific bytes."*

**D2 — the "stable policy disposition values" enumeration is short one value.**
Line 25 listed five: `selected`, `below_minimum_size`, `too_recent_for_policy`,
`not_included`, `excluded`. The code emits six — `policy_code`
(`output.rs:249-260`) also produces `selector_estimate_unavailable`, from
`PolicyDisposition::SelectorEstimateUnavailable` (`cleanup.rs:419`).
`tests/cli_contract.rs` pins it (`tests/cli_contract.rs:970-973`).

**Status: closed.** `plans/output-schema-v1.md:53` now lists all six.

**D3 — the `reason_code` enumeration omits a value the code emits.** The old
document listed 18 values. `CleanupReasonCode` had 19 variants and emitted
`skipped_ownership_unproven` (`cleanup.rs:399`), documented in the source as
*"Ownership could not be re-proven at all … Distinct from a workspace that
demonstrably changed."* A consumer building a closed enum from the old list had
no arm for it.

**Status: closed.** `plans/output-schema-v1.md:48` now lists 19 values and
explains the `skipped_ownership_unproven` / `skipped_changed_before_cleanup`
distinction. **But the enumeration is short again, for a different value** — see
G2.

**D4 — `effective_policy` internals are undocumented.** The code emits four
fields (`output.rs:73-79`): `min_reclaimable_bytes`, `min_inactive_seconds`,
`include`, `exclude`. A consumer could not write against `effective_policy` from
the old document.

**Status: closed.** `plans/output-schema-v1.md:41` now names all four.

**D5 — `units[].detail` is described as optional but is always a string.**
`CleanupUnitV1.detail` is `String` (`output.rs:93`), populated by `.clone()`
(`output.rs:193`). The key is always present; the "no detail" state is `""`. A
consumer testing `unit["detail"] == null` never matches; one testing
`"detail" in unit` always matches.

**Status: closed.** `plans/output-schema-v1.md:46` now says the detail string
"is always present (possibly empty), not optional".

**D6 — `mode` is described as conditionally present but is always emitted.**
`EnvelopeV1.mode` is `Option<String>` with **no** `skip_serializing_if`
(`output.rs:13`), so serde emits `"mode": null` on every scan document
(`output.rs:147`). A consumer branching on key presence rather than value is
wrong.

**Status: closed.** `plans/output-schema-v1.md:3` now says "`mode` is always
present and is `null` for a scan".

**D7 — the exit-code contract is stated for cleanup only, and scan has an extra
`1`.** The old line 27 read: *"The process exits 0 for completed requests with
safe per-unit skips, 1 for incomplete cleanup scope or operational cleanup
failure, and 2 for fatal invocation/configuration errors."* Cleanup matches:
`main.rs:495-499` returns 1 when `report.failed > 0 || report.scope_blocked.is_some()`;
`main.rs:34` returns 2 for any `AppError`. Two uncovered cases:

- The **scan** path also returns 1, for `full_incomplete` (`main.rs:1797-1802`) —
a condition unrelated to cleanup scope or unit failure. A `scan` on a machine
with a `PlatformRoot` error exits 1 with a structurally normal JSON document.
(An unreconcilable *state* file no longer does this: that exit-code
coupling was removed, because state is an optimization only.)
- A scan with permission-denied, metadata, or vanished diagnostics exits **0**. A
consumer treating exit 0 as "nothing was wrong" is misled; it must read
`result.diagnostics`.

**Status: closed for the scan case.** `plans/output-schema-v1.md:61` now adds
"A scan additionally exits 1 when a Full scan hit an unreadable platform root",
and the state-persistence decoupling is stated in the same paragraph.

**M012A added one more divergence, on exit codes.** `clean --full` runs a Full
scan to refresh learned roots and then refuses to clean against them if that
scan did not fully complete: `policy::full_cleanup_roots` turns a non-zero
reconciliation exit into `AppError::Config` (`main.rs:778` →
`policy.rs:523-528`) → **exit 2**. Under the old line 27's wording that is
arguably "incomplete cleanup scope", which the document assigned to 1. The
change is a deliberate fail-closed improvement — the old code returned the
scan's exit code silently, which a caller could not distinguish from the
cleanup's own — but it moves a cleanup-scope failure from 1 to 2.

**Status: open (G6).** The rewritten schema document's exit-code paragraph does
not mention this case.

**D8 — no success indicator, and exit 2 produces no JSON at all.** Neither the
document nor `EnvelopeV1` carries `status`, `success`, or `exit_code`. Failure is
signalled only by `scope_blocked`/`scope_reason` and `summary.failed` *content*,
plus the process exit status. Worse, every `Err` return from `run()` — including
`serde_json::to_string` failures at `main.rs:1662-1666` and `main.rs:1189-1194` —
prints to **stderr** and exits 2 with **stdout empty** (`main.rs:19-35`). A
consumer reading stdout cannot distinguish "never got that far" from "found
nothing". Log mode narrows this: `main.rs:22-30` writes one `status=error` line
to stderr via `output::log::fatal` instead of the human `cargo-cleanme: {e}`
prose, still with empty stdout, but with a greppable `reason=` code.

**Status: open.** The schema document has no success indicator to carry, and
this is a design decision rather than doc rot — but it remains the sharpest edge
in the machine contract.

**D9 — `units[].outcome`'s value set was never enumerated.** The document named
`outcome` ("operation outcome") and separately enumerated `summary`'s five
counters, but never said what `outcome` may contain. The code emits
`previewed`, `simulated`, `cleaned`, `skipped`, `failed`
(`cleanup.rs:276-284`). A consumer had to infer the set from the summary field
names.

**Status: closed.** `plans/output-schema-v1.md:47` now enumerates the five
values.

**D10 — `summary.failed` is recomputed, not read from the domain field.**
`output.rs:196-209` builds `CleanupSummaryV1` and tallies all five outcome
counters by matching `CleanOutcome`, including `failed`. It never reads
`CleanReport.failed` (`cleanup.rs:520`). Meanwhile `main.rs:495-499` computes the
exit code from `report.failed`. Two independent mechanisms that must agree.
**They currently do, exactly**: `report.failed` is incremented at
`cleanup.rs:2151` and `cleanup.rs:2170`, and those are the only two sites, each
paired with a `CleanOutcome::Failed` push at `cleanup.rs:2157` and
`cleanup.rs:2176`. But nothing enforces the coupling — a future `Failed` push
that forgets the increment would make `summary.failed: 0` in a document whose
exit code is 1, with no test catching it.

**Status: closed in the document.** `plans/output-schema-v1.md:49` now says the
`failed` count is tallied from the per-unit `outcome` values *"and is expected to
agree with a non-zero process exit code, but that agreement is a property of the
current implementation rather than an enforced invariant."* That is exactly
right, and it is the schema document's own admission that the coupling is
unenforced. See §11 item 5.

### 8.3 Divergences and gaps that are open today

**G1 — the schema document's path-encoding rule is stale.** Line 53 still says
*"Paths are UTF-8 strings when representable; otherwise the display string uses
the platform escaped debug form and is not a reversible identity encoding."*
The code no longer uses the debug form: `output::path` is
`to_string_lossy().into_owned()` (`output.rs:262-266`), so a non-UTF-8 path
becomes a U+FFFD-bearing UTF-8 string, exactly like the human renderer
(`report.rs:95`). The first clause ("are UTF-8 strings when representable") is
also wrong as written — a path is *always* a UTF-8 `String`, representable or
not. `tests/cli_contract.rs` does not pin the escaped-debug form, so nothing
catches the drift.

**G2 — the `reason_code` enumeration is short two values again.**
`plans/output-schema-v1.md:48` lists nineteen values; the code emits **twenty-one**
(`cleanup.rs:384-405`). The missing ones are `skipped_outside_selection`
(`cleanup.rs:401`), added with M014 for a unit the operator's selection removed,
and `skipped_incomplete_coverage` (`cleanup.rs:402`), added with M018 for a unit
under a learned root whose own traversal did not complete. The document's
preamble ("Current values are …") is non-exhaustive phrasing, but it reads as a
closed list, and a consumer building an enum from it has no arm for either value
the selection and coverage paths actually emit. The first is pinned by
`tests/cli_contract.rs:1494`.

**G3 — the JSON contract drops the `capability` of an unresolvable manifest.**
This is the most consequential gap, and it is *not* a doc-rot issue: fixing it
requires a schema change.

`UnresolvedOwnershipParticipant` carries four fields
(`domain.rs:194-199`): `manifest`, `stage`, `reason`, and
`capability: UnknownCapability`. `UnknownCapability` is a two-variant enum
(`domain.rs:181-190`): `RefusedByCargo` (Cargo ran `locate-project` and refused
the manifest, so the manifest owns no bytes) and `Unproven` (byte-capability
unproven — transport failure, unparseable output, lost worker, metadata,
identity, or a discovery hole; this merges everything).

The capability is **load-bearing at runtime**.
`workspace::attribute_unknown` (`workspace.rs:419-438`) reads it at
`workspace.rs:423` and returns `UnknownDisposition::MergeAll` for anything that
is not `RefusedByCargo` — and `MergeAll` is the case that **blocks the run**.
A `RefusedByCargo` participant is downgraded to `PassengerOf` (it merges nothing
and blocks nothing) or `ScopeNote` (a scope-level note), depending on whether
Cargo named a workspace manifest that resolved in this run
(`workspace.rs:426-437`).

`UnresolvedParticipantV1` (`output.rs:68-72`) projects only `manifest` / `stage` /
`reason` (`output.rs:71-75`). **The `capability` is not in the JSON at all.** A
JSON consumer therefore cannot tell a Cargo-refused manifest (reported, not
blocking) from one whose byte-capability is unproven (blocking). It would have to
re-parse the free-text `reason` — and the domain type explicitly warns against
exactly that, at `domain.rs:178-179`: the capability is *"re-parsed from message
text: a copy-edit to a diagnostic must not silently reclassify an unknown from
merging to non-merging."* The code itself only parses `reason` to recover a
refused manifest's *named workspace* (`workspace.rs:426`), never its capability.
`reason` is also the one field the schema document says a consumer must not
match on. The human report has the same gap — it names the manifest and the
reason but not the capability (`cleanup.rs:578-620`).

**G4 — ADR 007's "the system is not clean, the boundary is" signal exists only
in prose.** `CleanReport.out_of_scope_unresolved` (`cleanup.rs:536`) counts
manifests that did not resolve *outside* the selected scope. Both human shapes
print it — the quiet one included, because the disclosure is about what the
report *means*, not how long it is (`cleanup.rs:657-662`):

> `… N out-of-scope unprovable participant(s) reported in diagnostics, not
> blocking: outside the selected scope`

`CleanupV1` has **no field** for it. A JSON consumer sees the count only
indirectly, if at all: the out-of-boundary participants travel in the *scan*
document as `out_of_scope` diagnostics (`domain.rs:77`, `output.rs:134`), but the
cleanup envelope's `unresolved_ownership` count is explicitly *boundary-relevant
only* (`output.rs:223`). So a cleanup JSON document is silent about the fact that
the wider system is unproven — the exact signal ADR 007 exists to produce, and
the one whose absence would let "boundary clean" read as "system clean".

**G5 — the schema document lists an `operation` value the tool cannot produce.**
`plans/output-schema-v1.md:7` says `operation` is `scan`, `clean`, `config`, or
`update`. `EnvelopeV1.operation` is only ever `"scan"` or `"clean"`
(`output.rs:145`, `214`). `config` refuses `--format json` outright
(`main.rs:238-242`, exit 2), and `update` emits the non-envelope document of §4.
The document's own §8 caveat ("A consumer must not treat `schema_version == 1`
as implying the envelope described here", `plans/output-schema-v1.md:28`) covers
`update` but not `config`.

**G6 — three cases are unlisted.** `plans/output-schema-v1.md:61` describes
0 / 1 / 2 but omits these cases (two distinct *exit-code values*, `1` and `2`,
across three invocations):

- `config edit` maps an editor failure to exit `1` (`main.rs:275-283`); the
editor's own status stays in the diagnostic prose, never in our exit code.
- `clean --full` maps an incomplete Full reconciliation to exit `2`
  (`main.rs:778` → `policy.rs:523-528`).
- `-i` with `--format json` or `--format log` is refused as a configuration
  error, exit `2` (`main.rs:581-587`) — the interactive TUI is human-only, and
  this is worth stating in the machine contract because it is the one machine
  format that a legitimate invocation cannot use.

One more refusal belongs in the same list, and it is not in the schema doc:
`config show --format json` exits `2` (`config command does not support --format
json`).

**One exit-code case has left the list.** The interactive path's exit codes now
agree with the non-interactive path's: `-i` on a scope with roots but no cleanup
units prints `Nothing to clean.` and exits `0`, the same as the same scope
cleaned *without* `-i`. It exited `2` before C038, which made a scope with
nothing in it look like an invalid invocation.

**G7 — M018's unproven learned roots are disclosed on stderr only.** When the
Full cleanup cannot prove a learned root's coverage, the root is excluded from
the candidate set and the exclusion is reported as a `Skipped` row with
`reason_code: "skipped_incomplete_coverage"` — but only for a root that has a
workspace under it. A learned root with **no** workspace produces no row at
all, and its exclusion reaches the operator only as a stderr line
(`main.rs:389-401`). Neither the JSON document nor the log line carries the
list of unproven roots: the JSON envelope has no field for it, and the log
format deliberately suppresses the stderr disclosure (`main.rs:393`). The exit
code does say something — `cleanup_exit_code` (`main.rs:812-821`) returns 1
when any root was unproven — so an unattended script learns *that* the boundary
was incomplete but not *which* roots were skipped. The stderr text
(`main.rs:395-399`) names them. This is a gap in the machine contract, not a
defect in the disclosure: the operator-facing surface says what happened, and
the machine surface says that something was.

### The stability promise, and whether the code honours it

`plans/output-schema-v1.md:3` carries the operative sentence:

> "A backwards-incompatible field or meaning change requires a schema-version
> decision."

This is a *procedural* promise, not a compatibility guarantee, and as a promise it
is currently honoured: `schema_version` is a pinned literal at both projection
sites (`output.rs:143`, `output.rs:212`), there is no runtime negotiation or
feature gating, and no field in any DTO is documented as deprecated.

Two qualifications. First, **D1 and D2 were live contract ambiguities, not just
doc rot**: a consumer written from the old lines 16 and 25 would implement
`selector_estimate_bytes` handling and disposition enums that did not match the
tool. Both are closed in the document now; the code never broke its promise, the
document had not been kept in step. Second, **`update_json`
(`output.rs:903-931`) reuses `schema_version: 1` without the envelope** — no
`scope`, no `mode`, different `result` shape. The document scopes itself to the
scan/cleanup surfaces, so this is not a violation as written, but a consumer
treating `schema_version == 1` as "the envelope in this document" will mis-parse
the `update` document. C022 added four contract cases for that document
(`output.rs:969`, `995`, `1009`, `1048`); it did not change its shape, so the
ambiguity is unchanged and is still recorded here rather than fixed.

### Practical consequence for a consumer

**A script written strictly against the schema document cannot rely on the tool's
current output in exactly three places, plus everything §8.3 leaves open.** The
scan surface is safe: its 24 field names (the envelope's six plus
`ScanV1`'s four, `ScanGroupV1`'s seven, `DiagnosticV1`'s four and
`ScanSummaryV1`'s three) match exactly, and the four span behaviours (single
newline, non-UTF-8 path form, `--stats` isolation, progress suppression) all
hold in code. The document breaks in three specific places: the path-encoding
rule (G1), the `reason_code` enumeration (G2), and the `operation` value set
(G5) — plus the exit-code list (G6). Everything outside those is safe today. A
consumer treating the document as *descriptive* and the tool as *authoritative*
is fine; one treating it as *normative and exhaustive* is not.

The two DTO-level gaps (G3, G4) are different in kind: no amount of reading the
schema document helps, because the information is not in the document or the
document — it is not in the JSON at all.

---

## 9. Invariants and edge cases

**Is the JSON valid for every reachable domain state?** Yes. Traced:

- *Empty scan* — `ScanReport::default()` → `output.rs:106-158` maps empty vecs to
empty vecs; `group_count: 0`, `inventory_bytes: 0` (fold over nothing). Valid.
- *Zero groups but diagnostics* — same, plus the full diagnostic list. Valid.
- *Cleanup that spawned nothing* — `main.rs:426-441` builds a `CleanReport`
naming every field explicitly (no `..Default::default()`; M012A removed the
spread so a new field is a compile error rather than a silent default).
`results` is empty, so `units: []` and all five summary counters are `0`
(`output.rs:200-209` over an empty iteration). `mode` is `Some`, `scope` is the
resolved scope. Valid, and asserted at `tests/cli_contract.rs:731`.
- *Scope-blocked cleanup* — the two early returns leave `results` **empty** while
`units_considered` is set from the live `units` vector at `cleanup.rs:1650`:
the incomplete-discovery block at `cleanup.rs:1666-1677` and the
ownership-unproven block at `cleanup.rs:1679-1694` both report the live
counts. A consumer can therefore see `units_considered: N, units: []` with
`scope_blocked: true` — structurally valid, semantically surprising, and not
warned about in the document.
- *Scope-blocked cleanup, third shape* — a **pre-run** whole-scope block (an
automatic root that is a symlink or indeterminately unreadable) never reaches the
engine at all: `main.rs:396-419` builds the `CleanReport` itself with
`results: []`, `units_considered: 0`, `unresolved_ownership: []`, and
`out_of_scope_unresolved: 0`, then exits 1. So the same `scope_blocked: true`
signal arrives with `units_considered: 0` here and with the live count from the
engine. Three shapes, one flag; a consumer cannot infer how much was considered
from `scope_blocked` alone.
- *No `Option` produces malformed output.* All `Option` fields serialise as `null`
under a plain derive; there is no `flatten` and no `skip_serializing_if` to
interact badly with, and serde's custom-error paths are unreachable from these
types (owned scalars, `String`, `Vec`, one `&'static str`).

**Misleading `None`s.** The real hazard is D1's `selector_estimate_bytes`. Beyond
that, `newest_activity_unix_seconds: null` conflates "no mtime" with "mtime before
the Unix epoch" (`output.rs:118`, `267-271`), and `before_bytes: null` conflates
"selector active" with "not measured". Neither is malformed; both can be misread.

**Ordering determinism.**

- *Human*: fully deterministic. `render` sorts bytes-desc then path-asc
(`report.rs:135-139`), the tie-break makes equal-byte groups stable, and the total
is a pure fold. Pinned at `report.rs:227`, `tests/end_to_end.rs:210`.
- *JSON*: **not self-deterministic.** `output::scan` imposes no ordering at all —
it maps `report.groups` in place (`output.rs:106-158`) and inherits whatever
order the pipeline produced. The "deterministic" at `plans/output-schema-v1.md:33`
is a promise about `workspace::build_groups`/`analyze_groups`, not about this
module.
- *Cleanup is worse*: both human shapes sort only their own local `ordered` vec
(`cleanup.rs:697-703` for the quiet surface, `cleanup.rs:697-703` for the verbose
one) and leave `report.results` untouched, so the JSON `units`
array is in unsorted `results` order while the human rows are size-desc. **The
two projections genuinely disagree on ordering for cleanup.** I did not read
`workspace.rs` in depth, so I cannot certify the inherited scan order either;
the human surface is self-certifying, the machine surface is not.
- *Filesystem iteration*: neither module walks a directory. Both consume
already-collected `Vec`s. Ordering determinism is inherited, not established, for
JSON.

**Can `render` panic?** No. The only indexing is `UNITS[unit]`
(`report.rs:126`), bounded to `[0, 6]` by the `unit + 1 < UNITS.len()` guards at
`report.rs:115` and `121`. There is no division by a possibly-zero value — the only
divisors are the constants `1024` and `100`. No unwrap, no arithmetic overflow
(`saturating_add`, `report.rs:143`), no slice range. An empty group list yields
`"\n0.00 B inventory estimate across 0 inactive Cargo output groups\n"`, asserted
at `report.rs:223`. `u128::from(n) * 100` at `report.rs:113` cannot overflow:
`u64::MAX * 100` ≈ 1.8e21, far below `u128::MAX`.

**Precision loss in byte totals?** **No, definitively.** `format_bytes` contains
no float conversion — the ladder is `u128` integer arithmetic
(`report.rs:113-125`) and the comment at `report.rs:111-112` names the hazard
explicitly. `report.rs:249` pins `u64::MAX → "16.00 EiB"`, representable only in
integer form. In the machine projection bytes stay `u64` and are never formatted;
log mode sums them with `saturating_add` (`output.rs:332-335`, `378-380`) and
prints the raw integer. (The `f64` conversions that do exist —
`domain.rs:386-390` `timings_line`, `main.rs:517` and `main.rs:1778`
`as_secs_f64` — are all `--stats` stderr output, and `ScanCounters` never reaches
JSON.)

**Escaping hazards.** The two projections now differ in *mechanism* but agree in
*content* for non-UTF-8 paths:

- *Human*: `path_text` (`report.rs:94-108`) escapes **every** control character to
`\u{hex}` if any is present, and lossy-converts non-UTF-8. ANSI sequences are
neutralised — ESC is `0x1b`, a control character, so it is rewritten. Covered
by `report.rs:187-197`. Terminal injection through a directory name is closed.
- *JSON*: `output::path` (`output.rs:262-266`) does **no** escaping of its own; a
valid-UTF-8 path passes through verbatim, ANSI escapes included. Serde's string
escaper maps C0 control characters (including `0x1b`) to `\u00XX`, so the
*document* is always valid JSON and cannot be corrupted or broken out of — but
the ESC byte survives as content. That is correct for a machine contract
(faithfully reporting the path is the job), and it means a consumer that `cat`s
a path field to a terminal re-opens the hazard the human path closes. The
document has no consumer guidance on this.
- `detail` (`output.rs:93`) is cloned unfiltered; `cleanup.rs:961` strips
newlines for the *human* line only, so the stored value can contain `\n` and
reaches JSON as an escaped `\n`. Not a hazard. The quiet surface prints no
`detail` at all — the per-unit detail lives on the verbose shape only. Log mode
never prints `detail` either.
- Invalid UTF-8 cannot corrupt the JSON: `output::path` always produces a valid
`String`.

**Do human and JSON ever disagree about the same fact?** Yes, in four places:

1. **Ordering of cleanup units** — human size-desc with a path tie-break
(`cleanup.rs:697-703`, `758-764`), JSON raw `results` order.
2. **Byte representation** — human prints `"  1.50 MiB"`, JSON prints `1572864`.
One number, two representations; a textual diff will not match them. Log mode
prints a third representation, the raw `u64` (`output.rs:343`).
3. **Diagnostic completeness** — JSON has all of them, human stderr groups and
caps them (`main.rs:1680-1768`); log mode shows neither and carries
`diagnostics=N` only (`output.rs:344`, `401-404`).
4. **Scope-block identity** — human prints the block's prose message
(`cleanup.rs:667`), JSON carries that prose in `scope_reason`, and log
mode carries the typed code `reason=<code>` and no prose
(`output.rs:386-388`). Three surfaces, three different levels of
machine-readability for the same event.

The **non-UTF-8 path** entry that used to be on this list is closed: the
projections now agree (`output.rs:262-266` vs `report.rs:95`).

`tests/end_to_end.rs:193-210` is the one place the project actively checks the
two projections agree, and it checks the three things that matter: same group
membership, same `group_count`, and the human string containing the machine's
`bytes` value (`tests/end_to_end.rs:210`).

**Can `scope` be wrong?** The scan and cleanup branches both pass a value derived
from the resolved scope rather than the flags: `scope_label(&policy.scope)` for
scan (`main.rs:1664`, `1486`), whose function maps the resolved `ScanScope` to
`"full"` / `"explicit"` / `"routine"` (`main.rs:1325-1331`), and
`resolved.scope_label` for cleanup (`main.rs:1191`, `1038`), where the label comes
out of `resolve_cleanup_roots` (`main.rs:880`, `804`, `825`) or out of
`admit_automatic` (`main.rs:1165`). The empty-roots path cannot mislabel
either, because an explicit root always yields a non-empty `roots` vector. Two
residual observations. `clean --full` that falls back to zero learned roots
reports `scope: "full"`, which is accurate. And `scope` is a bare
caller-supplied `String` with no validation — nothing in `output.rs` would reject
a wrong value, so the guarantee is entirely the caller's. `scan ROOT` reports
`"explicit"` (`main.rs:1380`, `1167`). The four scan scope selectors — `root`,
`known`, `all`, `full` — are mutually exclusive and exactly one is required, by
a single clap `ArgGroup` (`cli.rs:139`); `scan ROOT --full` is therefore rejected
rather than resolved, and `--full` is a hidden alias (`cli.rs:155-156`) for the
rootless Full form.

**Exit-code contract, and is success in the JSON?** Exit codes are stable and
documented at `plans/output-schema-v1.md:61`, and the code matches for cleanup
(D7 and G6 qualify it). **There is no success or failure field in any
envelope.** A consumer must branch on the process exit status plus
`result.scope_blocked` and `result.summary.failed`. For `scan`, diagnostics never
affect the exit code, so `result.diagnostics` is the only signal. For exit code 2
there is no JSON at all (D8). Log mode is the only surface that carries a status
in-band (`status=ok|failed|blocked|error`), and it is explicitly not a machine
contract (§7).

---

## 10. Testing

**`report.rs` — 11 tests, all inline** (`#[cfg(test)] mod tests`, `report.rs:169-364`):

| Test | Line | Covers |
|---|---|---|
| `report_escapes_control_bytes_in_paths` | 187 | ESC/BEL in a UTF-8 name escaped, never raw |
| `report_escapes_non_utf8_paths` | 200 | raw `\xff` → U+FFFD; no control survives |
| `report_ordering_and_zero_report` | 221 | zero-state wording **and** path tie-break |
| `zero_state_uses_the_same_inventory_wording_as_results` | 230 | "inventory estimate" in both states; never "reclaimable" |
| `format_bytes_scales_to_eib_and_promotes_rounded_units` | 241 | whole ladder incl. `u64::MAX` and the 1024.00 promotion |
| `groups_render_every_group_with_ownership_and_deduped_total` | 252 | ordering, all three labels, every group printed, no "recovered" |
| `groups_total_does_not_double_count_equal_roots` | 281 | duplicate physical paths counted once; singular "1 group" |
| `a_bounded_scope_spells_its_paths_relative_to_the_scope_root` | 298 | a path under the scope root is relative; one under no root stays absolute |
| `a_full_scope_spells_its_paths_absolutely` | 322 | a Full scope's paths keep their absolute form |
| `a_one_component_path_is_qualified_with_the_scope_root_name` | 339 | `my-app/target`, not a bare `target` |
| `the_scope_root_itself_is_spelled_with_its_own_name` | 355 | the scope root names itself instead of rendering as an empty column |

The first two are `#[cfg(unix)]` (raw `OsString::from_vec`). The last four are
M020's, and they are the tests that make the relative-path rule a contract rather
than a habit. This is thorough for a renderer: escaping, ordering, zero state,
unit ladder, deduplication, and path spelling are all pinned. It covers `render`
and `format_bytes` only — there is no test for a multi-line `detail` or a
whitespace-only path.

**`cleanup.rs` — the human cleanup surface has its own tests.** M020 added six
inline tests for the two shapes and the tally, and two older tests changed their
assertions:

| Test | Line | Covers |
|---|---|---|
| `execution_reports_observed_decrease_deduped` | 6290 | the Execute tally says `reclaimed` and never "would be reclaimed"; the **verbose** surface keeps the `observed decrease` phrase |
| `final_report_lists_all_groups_and_deduped_total` | 6765 | the verbose surface lists every group and the deduplicated total |
| `the_quiet_cleanup_report_lists_only_what_the_run_acted_on` | 10263 | the quiet surface lists the acted-on unit, omits the skipped ones, and keeps their count as `(3 skipped)` |
| `the_quiet_tally_states_the_space_saved` | 10293 | the tally's byte figure and "2 directories", and the absence of the verbose surface's scope census |
| `execute_and_simulation_words_their_tallies_differently` | 10320 | Execute says `reclaimed`, Simulate says `would be reclaimed` and says that no `cargo clean` was invoked |
| `a_failed_unit_is_always_listed` | 10355 | a `Failed` unit is a row and a `(1 failed)` count, and its bytes are not counted as reclaimed |
| `the_verbose_surface_restores_the_skipped_rows_and_the_census` | 10386 | `--verbose` brings back the skipped rows, the `combined scope:` line, and the `CleanupUnit(s)` census |
| `the_safety_disclosures_survive_the_quiet_surface` | 10407 | the ADR 007 disclosures are present on the **quiet** shape as well as the verbose one |

Note the direction of the last one: it is the *quiet* surface that is the
adversary here. A shorter report that dropped the disclosures would read as
"system clean" when only the boundary is clean — which is the M012A defect in a
new place.

**`output.rs` — 0 tests for the DTO layer; 15 for `log`, 4 for the update
document, 2 for `path`.** The file has **three** test modules, and the boundary
is not where a reader would guess: the first `#[cfg(test)]` marker is at
`output.rs:273` (`path_tests`), but the bulk of the test code is the module
*nested inside* `pub mod log` at `output.rs:509-884`. The update document's tests
are a third module at `output.rs:945-1065`. **No test constructs an
`EnvelopeV1` for the scan side, and no test asserts a scan-side field name**;
the `selector_estimate_bytes` inversion (D1) has no test of its own — it is only
visible because `tests/cli_contract.rs` happens to run the selector path end to
end.

The fifteen log tests are real, and they test the right things: the canonical `ok`
shapes are asserted as exact strings (`output.rs:585-588`, `605-608`), so field
order and field names are pinned rather than merely "contains". Two are
premise-negative in the sense this repository values — one proves an overlong
optional value is dropped *whole* and never truncated
(`output.rs:771-798`), the other proves the structural head fits at the worst
case with `u64::MAX` in every numeric field (`output.rs:800-814`). The
ASCII/bound/no-quote/no-newline invariants are factored into one helper
(`output.rs:566-579`) and applied to every line, so a future line that violates
them fails loudly. The two premise-negative tests are labelled as such in their
own doc comments (`output.rs:768-770`, `800-801`), which is the discipline §4 of
[overview](overview.md) asks for.

**One log test is a contract test, not a formatting test.**
`cleanup_json_result_key_set_is_stable` (`output.rs:663-693`) serialises a real
`EnvelopeV1<CleanupV1>` and asserts the **exact set of fourteen `result` keys**,
sorted. It is the only place in the crate where the shape of a DTO is pinned
wholesale, and its doc comment records why it exists: the M015 → M016 → M017
partition report added and then removed an `islands` axis, and the list is
*"updated deliberately, not deleted: if a field is ever added to the machine
result, this fails and forces a conscious contract decision — not a quiet schema
drift."* That is exactly the guard the DTO layer otherwise lacks, and it should
be the model for the other nine DTOs.

**Not covered in `output.rs`:** the interaction between the `MAX_BYTES` cap and a
*structural* field that would not fit (there are only four, all short tokens,
and the `debug_assert!` at `output.rs:471-478` assumes that), and the fact that
log mode suppresses the human diagnostic fan-out and progress renderer — that
lives in `main.rs` and is only tested through the binary.

**`main.rs` — 0 tests.** `main.rs` has no `#[cfg(test)]` and no `mod tests`;
`grep -c "#\[test\]" src/main.rs` returns `0`. The `update_json` document used to
live here as a private function of the binary, which meant the only way to assert
anything about this surface was to run a live update against crates.io. C022
moved it into `output.rs` specifically to give it a testable seam, and it now
has four contract cases at `output.rs:969`, `995`, `1009`, `1048` (the last of
which asserts the *whole* stream — newline included — parses as exactly one JSON
document). `main.rs` calls it from one place (`main.rs:198-201`), and
`UpdatePlan` is defined at `update.rs:247` and tested inside `update.rs`'s own
module.

The document remains the only machine-readable surface that bypasses
`EnvelopeV1`, so it is still the one that diverges most from the shared
envelope — but it is no longer the one with the least evidence. What is
genuinely unpinned is a *successful* run: every success path resolves crates.io
and replaces bytes, so the hermetic cases prove the failure shape and the stream
bytes, and the successful live document is only observable through the
post-release smoke.

**`tests/cli_contract.rs` — pins real JSON shape, partially.** Shape assertions do
exist, which makes it a genuine contract:

| Line | Assertion |
|---|---|
| 193 | `--stats` does not change stdout bytes |
| 195-197 | `schema_version == 1`, `operation == "scan"`, `scope == "explicit"` |
| 199, 731 | stdout contains exactly one `\n` |
| 245-247 | configured `scan.root` resolves to `scope == "explicit"` — the regression for the label bug, §3 of [13-orchestration](13-orchestration.md) |
| 253 | the CLI root still resolves to `scope == "explicit"` |
| 618-629 | the JSON carries all ten diagnostics and `summary.diagnostic_count == 10` while the human stream shows at most eight causes |
| 727-730 | `schema_version == 1`, `operation == "clean"`, `mode == "simulate"`, `summary.simulated == 0` |
| 757-759 | `status.code() == Some(1)` with `result.scope_blocked == true` |
| 875-878 | `summary.cleaned == 1`, `units[0].reason_code == "cleaned"`, `selector_kind == "profile"`, `selector_value == "dev"` |
| 879-886 | `before_bytes` and `selector_estimate_bytes` both `Null` under a selector |
| 890-893 | `output_union_before_bytes` numeric under a selector |
| 929 | `units[0].outcome == "simulated"` |
| 966-973 | `reason_code == "selector_unsupported"`, `policy_disposition == "selector_estimate_unavailable"` |
| 1494 | `reason_code == "skipped_outside_selection"` (M014) |
| 1500, 1504 | `discovered_manifests == 1`, `resolved_workspaces == 1` for an out-of-scope participant |
| 2029-2038 | a refused orphan is **named** in `unresolved_participants` while `scope_blocked == false` — ADR 007's "reported, never blocking" |
| 2083-2092 | exactly one participant, the refused orphan, and the run still exits 0 |
| 350 | `discovered_manifests == 1_001` — a large count, not just a presence check |
| 3633, 3712 | `selected_roots` contains / does not contain a given path |
| 3949-3951 | `discovered_manifests`, `resolved_workspaces`, and `units_considered` all `== 1` |

The file also pins the **log** surface, which the previous revision of this
document did not record at all: a `--format log` scan must be ASCII, within
`MAX_BYTES`, one line, and must not contain the Cargo failure paths
(`tests/cli_contract.rs:644-654`); and an interactive selection refused in log
mode must still be one greppable bounded line carrying `op=clean` and
`status=error`, must not carry the prose refusal, and must spawn no Cargo
(`tests/cli_contract.rs:1596-1613`).

So yes — **a test asserts JSON shape, and it is a real contract**: about twenty
distinct field paths are pinned by name, value, and type, plus the whole
`CleanupV1` key set from inside `output.rs`. But coverage is uneven and
follows the tests' *interests*, not the schema's. Pinned: the envelope (3
fields), eleven of `CleanupV1`'s fourteen result fields, the selector quartet,
three reason codes, and the cleanup result key set. **Never asserted at all**:
`unresolved_ownership`, `effective_policy`, and
`state_generation_last_full_at` — the three `CleanupV1` result fields with no
value assertion anywhere in the integration tests. Also unpinned: `ScanV1`'s
other fields, and the shape of `ScanGroupV1`, `DiagnosticV1`, `ScanSummaryV1`,
`PolicyV1`, `CleanupSummaryV1` entirely. `UnresolvedParticipantV1` is
partially pinned — `tests/cli_contract.rs:2035-2044` and `2080-2089` assert
that the refused orphan is *named* and that it is the only participant — but
never its `stage` or `reason` values, and of course not a `capability` the DTO
does not have. The non-selector branch of the D1 inversion is never exercised,
and nothing asserts the *absence* of a key.

**`tests/end_to_end.rs` — the only cross-projection check.** Lines 202-210 assert
`groups[0].ownership == "private"`, `summary.group_count == 1`, `bytes >= 8192`, and
that `report::format_bytes(bytes)` appears in the human string. Five field paths,
all in the scan surface. No cleanup projection assertions.

**Honest assessment.** The contract's machine half is checked by exactly one
integration file, and only where that file's scenarios happen to reach the fields.
No test would fail if `PolicyV1` were renamed, if `DiagnosticV1::category` gained
a variant, or if `summary.failed` desynchronised from the exit code. The
`update_json` document is the exception — four cases in `output.rs` pin it
exactly. D2 and D3 survived for a reason worth stating precisely: the tests were
*right* and the *document* was wrong. `tests/cli_contract.rs:966-973` asserts
`policy_disposition == "selector_estimate_unavailable"` — the one value the
document's six-item list omitted (D2) — and `skipped_ownership_unproven` (D3)
had no test at all, so nothing caught it. A divergence between a test and a
document is only found when someone reads both. The same is true of the
twentieth value, `skipped_outside_selection` (§8.3, G2): it is pinned by
`tests/cli_contract.rs:1494` while the document still omits it. The
twenty-first, `skipped_incomplete_coverage` (M018), is pinned only by
`cleanup.rs`'s own unit tests — `the_coverage_exclusion_removes_candidates_and_names_the_cause`
(`cleanup.rs:4984`) and `a_unit_under_an_unproven_root_is_reported_as_incomplete_coverage`
(`cleanup.rs:4858`) — and by **no** integration test, so the JSON projection of
that code is unverified end to end.

**This is the fragile case**: a versioned schema contract checked only by tests
written against the implementation, with no assertion that the implementation
still matches the document. The cheapest high-value fix is to extend the
`cleanup_json_result_key_set_is_stable` pattern
(`output.rs:662-693`) to the other nine DTOs, and a second one to read the field
names from `plans/output-schema-v1.md` and assert each appears in a real
document.

---

## 11. Review checklist

1. **`selector_estimate_bytes` nullity, D1** — `output.rs:89-102` inverts the
rule the schema document used to state. **Closed in the document**
(`plans/output-schema-v1.md:43` now describes the code's rule). Confirm the
populated case (no selector → `Some`) is intended anyway;
`tests/cli_contract.rs:879-886` pins only the null case.
2. **`render`'s `&mut` receiver, `report.rs:135-139`** — the only mutation is
`report.groups.sort_by`. Confirm a pure `&ScanReport` plus a local sorted index
is acceptable; note `tests/end_to_end.rs:193` then `202` currently depends on the
mutation reaching the JSON projection.
3. **Dropped fields, `output.rs:106-121` and `211-247`** — every `*V1` literal is
hand-written with no `..Default`, except `CleanupSummaryV1` at
`output.rs:196-199`. Confirm the exclusion property (a new `ScanReport` field
cannot reach JSON) stays structural for fields added to the *DTOs*, where the
`Default` fill makes it conventional instead.
4. **JSON ordering is inherited, not established, `output.rs:106-158`** — no sort
here. Cross-check against the "deterministic" claim at `plans/output-schema-v1.md:33`
and against `cleanup.rs:697-703` and `cleanup.rs:697-703`, where the two human
shapes each sort a local vec
that `output::cleanup` never sees, so cleanup `units` order differs between the
two surfaces.
5. **`summary.failed` vs the exit code, `output.rs:200-209` vs `main.rs:495`** —
two independent mechanisms that agree only because `cleanup.rs:2151/2026` and
`cleanup.rs:2170/2045` are the sole paired sites. Re-verify that pairing on any
diff touching failure paths.
6. **Enum enumerations** — `cleanup.rs:399` (`skipped_ownership_unproven`) and
`cleanup.rs:401` (`skipped_outside_selection`) are the two codes a consumer must
handle; the first is now in `plans/output-schema-v1.md:48`, the second is **not**
(G2). `cleanup.rs:419` / `output.rs:257` (`selector_estimate_unavailable`) is
in the document's six-value list (line 53).
7. **Empty and blocked states** — `main.rs:426-441` gives
`units_considered: 0, units: []`; `cleanup.rs:1679-1694` gives
`units_considered: N, units: []`. Confirm both are intended and that no
consumer assumes `units.len() == units_considered`.
8. **`mode` and `detail` are not optional, `output.rs:93` and `output.rs:93`** —
no `skip_serializing_if` anywhere, so `"mode": null` appears on every scan
document and `detail` is `""` rather than absent. The document now says so
(`plans/output-schema-v1.md:3` and `plans/output-schema-v1.md:46`); any consumer
written from the older wording will mis-branch.
9. **No success field, and exit 2 emits no JSON, `main.rs:19-35` and
`main.rs:1662-1666`** — a consumer reading stdout cannot distinguish "never ran"
from "ran and found nothing". Log mode narrows it to a greppable stderr line
(`main.rs:22-30`) but still writes nothing to stdout. Consider whether a status
field belongs in a future schema version rather than v1.
10. **`update_json` reuses `schema_version: 1` without the envelope,
`output.rs:903-931`** — no `scope`, no `mode`, different `result` shape. C022
gave it four cases (`output.rs:969`, `995`, `1009`, `1048`) but did **not**
reconcile it with the envelope, because that is a schema change needing its own
plan and a version bump. Confirm consumers of `schema_version == 1` are scoped
per `operation`, and that the `update` document should be documented alongside
the others.
11. **Keep byte formatting float-free, `report.rs:113-116`** — any change
introducing `n as f64` silently breaks `report.rs:249`
(`u64::MAX → 16.00 EiB`), the only guard at the `2^53` boundary.
12. **`ScopeBlock.reason` is lost in JSON, `cleanup.rs:453` vs `output.rs:222`** —
the domain gained a typed block code in M012A and the envelope still
projects only `.message`. Log mode reads it (`output.rs:387`) and JSON does
not, so the versioned contract is the less machine-readable of the two. Adding
a `scope_reason_code: Option<&'static str>` is a schema-version decision, not a
patch.
13. **`clean --full` exit code moved from 1 to 2, `main.rs:778` →
`policy.rs:523-528`** — an incomplete Full scan now raises `AppError::Config`
instead of returning the scan's code silently. Deliberate fail-closed, but it is
not in the schema document's exit-code list (G6).
14. **`pub mod log` is not a contract, `output.rs:302-308`** — its own doc comment
says so, and the schema document repeats it
(`plans/output-schema-v1.md:55-59`). Two risks follow from
adding a third surface: a consumer treating `status=ok` as authoritative for a
*result* rather than a *summary*, and the field-drop policy at
`output.rs:494-501` silently losing `reclaimed_bytes` on a crowded line. The
384-byte cap is enforced by dropping fields whole, not by erroring, so a line
can be structurally valid and materially incomplete.
15. **`UnresolvedParticipantV1` drops `capability`, `domain.rs:198` vs
`output.rs:227-231`** — a JSON consumer cannot tell a Cargo-refused manifest
(reported) from a byte-unproven one (blocking). This is a schema change, not a
patch, and it is the gap most likely to cause a consumer to draw the wrong
conclusion about what was cleaned. See §8.3, G3.
16. **`CleanupV1` has no `out_of_scope_unresolved`, `cleanup.rs:536` vs
`output.rs:217-245`** — ADR 007's "the system is not clean, the boundary is"
signal exists only in the human report (`cleanup.rs:657-662`). See §8.3, G4.
17. **The schema document's path rule is stale, `plans/output-schema-v1.md:53` vs
`output.rs:262-266`** — the code is lossy, the document still describes the Rust
debug form. Correct the document or restore the code, deliberately. See §8.3, G1.
18. **`operation` can be `config` only in the document, `plans/output-schema-v1.md:7`
vs `main.rs:238-242`** — `config` refuses `--format json` and `update` is not an
envelope. See §8.3, G5.
19. **The interactive TUI is human-only, `main.rs:581-587`** — `-i` with
`--format json` or `--format log` is a configuration error (exit 2). Any
automation that offers a selection must not offer it in a machine format; the
schema document does not say so. See §8.3, G6.
