# Reporting — human text and the versioned machine contract

> Component deep dive · part of the [architecture overview](overview.md)
> **Status:** the ten schema divergences catalogued in §7 (D1-D10) have been
> **resolved** by treating the implementation as authoritative and correcting
> `plans/output-schema-v1.md` to match. The analysis below is retained as the
> record of what diverged and why.

Covers `src/report.rs` (195 lines) and `src/output.rs` (265 lines) together. They
are two projections of the same data, and the contrast is the point: the human
renderer predates the machine contract; the DTO layer exists to protect it.

---

## 1. Responsibility

Both modules are **presentation only**. Neither computes a size, classifies
ownership, decides a policy disposition, nor performs I/O.

| | `report.rs` | `output.rs` |
|---|---|---|
| Consumes | `ScanReport` | `ScanReport`, `CleanReport` |
| Produces | `String` (prose) | `EnvelopeV1<T>` (typed DTO tree) |
| Encoding | hand-rolled `format!` | `#[derive(Serialize)]` only — **no serializer** |
| I/O | none | none |

`output.rs` derives `Serialize` but never calls `serde_json`. Serialization and
the trailing newline are owned by `main.rs:499` (scan), `main.rs:217` (cleanup),
`main.rs:184` (empty-roots cleanup). Keeping encoding out of the DTO module means
a caller can serialise the same envelope to JSON, or anything else, without
touching the contract.

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
so it reached into the cleanup vocabulary. The two modules share no code;
`output.rs` never calls `report::format_bytes`.

The dependency arrow that does exist points the opposite way from the obvious:
`cleanup.rs:316,324,329,341,348,355-357` and `progress.rs:171` all call
`crate::report::format_bytes`. The byte formatter is a shared library function, so
`report.rs` sits *below* both `cleanup.rs` and `output.rs` in the layer map —
which is why it needs no cleanup types at all.

---

## 2. Two projections, one source

The module comment at `output.rs:1-2` states the intent:

> Versioned, stable machine-output DTOs. Internal domain structs are projected
> explicitly so implementation refactors do not silently change the CLI API.

The mechanism is **hand-written projection**, not derived serialization. Every
field of every `*V1` struct is assigned by hand at the projection site
(`output.rs:107-118`, `123-136`, `167-188`, `221-226`, `228-233`). `Serialize` is
derived on the *DTOs*; it is never derived on `domain` or `cleanup` types. (The
one `Serialize` derive in that layer is on `PolicyDisposition`
`cleanup.rs:165-166`, which the DTO layer does not use — `output.rs:243-254`
converts it by hand instead.)

**Why this matters.** `ScanReport` (`domain.rs:84-90`) and `CleanReport`
(`cleanup.rs:177-193`) are internal, freely refactorable, and full of fields that
are not part of the contract. `ScanReport` alone carries `visited_entries` and a
35-field `ScanCounters` (`domain.rs:238-277`) of prune tallies, Cargo subprocess
counts, and nanosecond timings. Deriving `Serialize` on the domain type would
dump all of it into the public API, and the next person to add a timing counter
would silently break every consumer. It would also make the wire format a
function of the internal model's shape, so renaming `PrivateBounded` to something
prettier would change the JSON.

The projection is also a **normalisation** step: `SizeMetric` and
`DiagnosticCategory` become strings (`output.rs:112-115`, `125-133`),
`SystemTime` becomes `Option<u64>` via a fallible conversion (`output.rs:116`,
`261-265`), and `PathBuf` becomes a `String` through a lossy escape hatch
(`output.rs:256-260`).

### The exclusion property holds

**Definitively: a new field on `ScanReport` does not appear in the JSON.** The
property is structural, not conventional. `ScanV1` (`output.rs:18-23`) is a
separate struct with four named fields, constructed by a struct literal
(`output.rs:145-154`). There is no `#[serde(flatten)]`, no `#[serde(untagged)]`,
no `..Default::default()` widening, no pass-through map. Adding a field to
`domain::ScanReport` is invisible to the wire format unless someone also writes a
line in `output.rs`. Same for `CleanReport` → `CleanupV1` (`output.rs:211-239`).

The one place the discipline is *not* structurally enforced is `CleanupSummaryV1`
(`output.rs:190-193`), which uses `..Default::default()` to fill the five outcome
counters. A field added there with a `Default` impl would emit a silent zero. The
struct is small and hand-listed, so the risk is contained — but it is the single
spot where a future field could appear without an explicit assignment.

Deliberately excluded, justified in §6: `ScanCounters`, `visited_entries`,
`artifact_entries`, `CleanReport.counters`, `CleanReport.failed`,
`CleanResult.workspace_roots`, and `PhysicalOutputGroup`'s `covering_roots`,
`owners`, `uncertain`.

---

## 3. Human rendering

### `format_bytes(n: u64) -> String` — `report.rs:25-43`

**Binary (IEC), not decimal.** `UNITS` is `["B", "KiB", "MiB", "GiB", "TiB",
"PiB", "EiB"]` and the divisor is hard-coded `1024` at `report.rs:31`, `37-38`.
There is no `kB`/`MB` in the ladder.

**No float — a deliberate, tested choice.** The comment at `report.rs:28-29`
gives the reason (*"`n as f64` silently loses bytes above 2^53, so the largest
units could not be reported accurately at all"*) and the code matches it. The
whole ladder runs in `u128`:

```rust
let mut hundredths = u128::from(n) * 100;           // report.rs:29
while unit + 1 < UNITS.len() && hundredths >= 1024 * 100 {
    hundredths = (hundredths + 512) / 1024;         // report.rs:32 — round-half-up
    unit += 1;
}
```

Scaling is repeated division on a scaled integer, round-half-up at each step
(`+512` before `/1024`). A second pass (`report.rs:37-40`) catches a value that
rounded onto exactly `1024.00` and promotes the unit rather than printing
`1024.00 KiB`; `report.rs:149` pins `1_048_575` → `1.00 MiB`.

Rendering is two decimal places via `format!("{}.{:02}", hundredths / 100,
hundredths % 100)` (`report.rs:41`) — integer division. **There is no `as f64`
anywhere in the function**, so the `u64 → f64 → string` precision hazard does not
apply. `u64::MAX` renders as `16.00 EiB` (`report.rs:152`): the ladder saturates
at `EiB`, and 2^64 bytes is exactly 16 EiB, so the top of the range is shown at
16.00 rather than overflowing the unit list. The `unit + 1 < UNITS.len()` guards
on both passes mean `UNITS[unit]` at `report.rs:42` can never be out of bounds.

Output is right-aligned to width 6 (`report.rs:42`). That is best-effort: a
7-character value like `1234.56` simply widens the column. Assertions must
`.trim()`, as the tests do at `report.rs:145-152`.

### `render(report: &mut ScanReport) -> String` — `report.rs:44-71`

**Why the `&mut`:** it sorts, and that is the only mutation
(`report.rs:45-49`):

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
`tests/end_to_end.rs:193` calls `report::render(&mut report)` and then serialises
**the same report** at `end_to_end.rs:202`, so the human renderer's sort
determines the JSON group order in that test. In the binary the two paths are
mutually exclusive (`main.rs:496`, `main.rs:511`), so shipped output is
unaffected. A `&[EligibleOutputGroup]` parameter plus a local sorted index would
fix it at zero cost.

**Output structure.** No header, no section banner, no trailing rule — a flat
list plus one summary line:

1. One line per group: `format_bytes` (width 6), two spaces, display path, two
   spaces, `[ownership-label]` (`report.rs:56-61`).
2. Blank line, then `"{total} inventory estimate across {n} inactive Cargo
   output groups\n"` (`report.rs:65-69`).

Groups are ordered **bytes descending, then `display_path` ascending**
(`report.rs:46-48`); the tie-break is what makes equal-sized groups
deterministic, pinned at `report.rs:130`.

The total is a `fold` with `saturating_add` over group bytes (`report.rs:50-53`),
so it cannot wrap. It is *not* re-derived from `physical_paths`: a group listing
the same canonical path twice is counted once, because grouping deduplicated it
before `bytes` was measured. That is the "deduplicated total" checked at
`report.rs:184-194`.

**Diagnostics are not rendered at all** — `render` uses only `report.groups`.
Diagnostics reach the user from `main.rs:515-538` on **stderr**: a count, the
first 10, and `… N more`. So JSON carries the complete diagnostic list while the
human stream truncates at 10. The human `stdout` document is not the whole
human-visible story.

**Ownership labels** come from `OutputOwnershipClass::label` (`domain.rs:194-201`):
`private`, `external-unproven`, `shared`, `uncertain`; pinned at `report.rs:176-178`.

**Activity state is not rendered per group.** There is no `inactive`/`active`/
`uncertain` word on any line. Groups only reach `ScanReport` if they already
passed the recency and ownership gates in `workspace::analyze_groups`; active and
uncertain groups are filtered upstream. The only activity signal a user reads is
the phrase *"N inactive Cargo output groups"* in the summary line, backed by
`end_to_end.rs:197-198` (a freshly built project is absent).

**What the user reads to decide whether to run `clean`:** the `[ownership]`
label. Per `cleanup.rs:437-442` and `493-495`, only `PrivateBounded` is ever
eligible; `ExternalUnproven`, `Shared`, and `Uncertain` are inventory-only under
any authorization-root configuration. A `[private]` row is a candidate; any other
label is informational. This invariant is *not* restated in the report's prose.

The summary wording is itself a safety property, asserted twice (`report.rs:133-142`,
`end_to_end.rs:199`): "inventory estimate" is used for the empty and non-empty
cases alike, and neither ever says "reclaimed" or "recoverable". These are
observed bytes, not bytes freed.

### Escaping

`path_text` (`report.rs:10-24`) is the terminal-safety boundary. It converts
lossily, checks `chars().any(char::is_control)`, and if any control character is
present rewrites **every** control character as `\u{hex}` (`report.rs:18`).
Non-UTF-8 bytes degrade to U+FFFD via `to_string_lossy` and are not re-escaped,
since U+FFFD is not a control character. Two tests cover this (`report.rs:90-100`
for ESC/BEL in a valid-UTF-8 name, `report.rs:103-121` for a raw `\xff`). The
invariant: a directory name cannot drive the terminal.

---

## 4. The machine envelope

`EnvelopeV1<T>` — `output.rs:7-15`:

| Field | Type | Set by | Notes |
|---|---|---|---|
| `schema_version` | `u32` | `output.rs:140`, `207` | Hard-coded literal `1` at both sites |
| `cargo_cleanme_version` | `&'static str` | `env!("CARGO_PKG_VERSION")` | Compile-time, from `Cargo.toml` |
| `operation` | `&'static str` | `output.rs:142` / `208` | `"scan"` or `"clean"` |
| `scope` | `String` | caller parameter | Passed in by `main.rs`, *not* derived |
| `mode` | `Option<String>` | `output.rs:144` / `210` | `None` for scan, `Some(mode)` for cleanup |
| `result` | `T` | `ScanV1` or `CleanupV1` | The generic parameter |

**The `#[serde(...)]` inventory is empty.** `output.rs` uses a bare
`#[derive(Serialize)]` on every struct — no `rename`, `rename_all`, `default`,
`skip_serializing_if`, `flatten`, or `deny_unknown_fields` anywhere. Two
consequences: JSON field names are the Rust names verbatim, so there is no rename
layer to keep in sync and no snake_case conversion to get wrong; and **`mode` is
always present as a key**, because `Option<String>` without
`skip_serializing_if` serialises `None` as `null`, not as an absent key. A scan
document contains `"mode": null`. `plans/output-schema-v1.md:3` describes it as
"optional cleanup `mode`", which reads as "absent unless cleanup" — a consumer
testing `'mode' in doc` would be wrong (§7 D6).

**How the version is chosen: it isn't.** `schema_version: 1` is a literal at
`output.rs:140` and `output.rs:207`. No `const`, no feature gate, no
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
`output.rs:144`) and `Some(mode)` (cleanup, `output.rs:210`), where `mode` comes
from `CleanMode::as_str` (`cleanup.rs:45-51`) — `preview`, `simulate`, `execute`.
`Option` is therefore a *discriminator*, not an "unknown" channel: for
`operation == "scan"` the field is not applicable, and no reachable state makes it
`Some`. It is `Option<String>` rather than `Option<&'static str>`, so the cleanup
path allocates for one of three constants (`output.rs:210`).

**`scope` is caller-supplied, not derived.** `main.rs` passes a literal:

| Call site | Value |
|---|---|
| `main.rs:501-507` (scan) | `"full"`, `"explicit"`, or `"routine"` |
| `main.rs:186` (empty-roots cleanup) | `"full"` or `"known"` |
| `main.rs:221-227` (cleanup) | `"full"`, `"known"`, or `"explicit"` |

The set is *not* uniform: scan has `routine`, cleanup has `known`, neither has the
other's. A consumer must treat `scope` as an open string, not a closed enum.

**One more envelope that is not an envelope.** `update_json` (`main.rs:568-590`)
builds a hand-rolled `serde_json::Map` with `schema_version: 1` and
`operation: "update"` — but **no `scope`, no `mode`, and a different `result`
shape**. It shares the version number without sharing the envelope (§7 D8).

---

## 5. DTO inventory

All structs are `#[derive(Serialize)]`, all fields `pub`. No `Default` except
`CleanupSummaryV1` (`output.rs:93`).

### `EnvelopeV1<T>` — `output.rs:7-15`
See §4.

### `ScanV1` — `output.rs:18-23`

| Field | Type | Meaning |
|---|---|---|
| `discovered_manifests` | `u64` | `Cargo.toml` manifests found (`ScanReport.discovered`) |
| `groups` | `Vec<ScanGroupV1>` | Inactive, reportable output groups |
| `diagnostics` | `Vec<DiagnosticV1>` | Complete, untruncated diagnostic list |
| `summary` | `ScanSummaryV1` | Aggregate counts |

### `ScanGroupV1` — `output.rs:24-33`

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
(`output.rs:116`) **or** when the `SystemTime` predates the Unix epoch
(`output.rs:261-265`, `duration_since(...).ok()`). Those two "unknown" causes are
indistinguishable to a consumer.

### `DiagnosticV1` — `output.rs:34-40`

| Field | Type | Meaning |
|---|---|---|
| `severity` | `String` | `info` / `warning` / `error` (`DiagnosticSeverity::as_str`, `domain.rs:59-65`) |
| `category` | `String` | Six-value snake_case set (`output.rs:125-133`) |
| `path` | `Option<String>` | Subject of the diagnostic, when path-scoped |
| `message` | `String` | Human explanation, not a stable token |

`path: None` means "not path-scoped" (e.g. a Cargo metadata failure), not "path
unknown". `message` is free text by design and the one field a consumer must not
match on.

### `ScanSummaryV1` — `output.rs:41-46`

| Field | Type | Meaning |
|---|---|---|
| `group_count` | `usize` | `groups.len()`, computed at projection |
| `inventory_bytes` | `u64` | Saturating sum of group bytes; **observed, not recovered** |
| `diagnostic_count` | `usize` | `diagnostics.len()` |

### `CleanupV1` — `output.rs:48-64`

| Field | Type | Meaning |
|---|---|---|
| `discovered_manifests` | `usize` | Note `usize` here vs `u64` in `ScanV1` |
| `resolved_workspaces` | `usize` | Workspaces that resolved authoritatively |
| `units_considered` | `usize` | CleanupUnits sized, **before** per-unit outcomes |
| `scope_blocked` | `bool` | Whole-scope block; from `Option::is_some()` |
| `scope_reason` | `Option<String>` | Prose reason for the block |
| `unresolved_ownership` | `usize` | Count of unresolved manifests |
| `unresolved_participants` | `Vec<UnresolvedParticipantV1>` | Per-manifest detail |
| `selected_roots` | `Vec<String>` | Bounded cleanup roots in effect |
| `effective_policy` | `Option<PolicyV1>` | Policy after CLI-over-config merge |
| `selector_kind` | `Option<&'static str>` | `profile` or `package` |
| `selector_value` | `Option<String>` | The profile/package spec |
| `state_generation_last_full_at` | `Option<u64>` | Discovery-state generation; metadata only |
| `units` | `Vec<CleanupUnitV1>` | Per-CleanupUnit results |
| `summary` | `CleanupSummaryV1` | Outcome tallies |

`scope_blocked: false` with `scope_reason: null` is the normal state. `true`
implies a non-null `scope_reason`, since both read the same `Option<String>`
(`output.rs:215-216`). `effective_policy: None` is reachable only if the policy
were omitted from the `CleanReport::default()` construction at `main.rs:176-181`;
live paths always set it (`cleanup.rs:836`).

### `UnresolvedParticipantV1` — `output.rs:65-70`

| Field | Type | Meaning |
|---|---|---|
| `manifest` | `String` | The manifest that failed to resolve |
| `stage` | `&'static str` | Resolution stage, passed through unvalidated (`output.rs:223`) |
| `reason` | `String` | Prose diagnostic reason |

`stage` is a bare `&'static str` with no type-level enumeration in `output.rs` or
the schema document. I did not trace its producer in `discovery.rs`/`workspace.rs`,
so the legal value set is **not determined here**.

### `PolicyV1` — `output.rs:71-77`

| Field | Type | Meaning |
|---|---|---|
| `min_reclaimable_bytes` | `u64` | Size floor; `0` means no floor |
| `min_inactive_seconds` | `Option<u64>` | Recency floor; `None` = none configured |
| `include` | `Vec<String>` | Workspace-root include patterns |
| `exclude` | `Vec<String>` | Workspace-root exclude patterns |

`min_inactive_seconds: None` is a real configuration state, not a measurement
failure — it means no recency policy is in force.

### `CleanupUnitV1` — `output.rs:78-92`

| Field | Type | Meaning |
|---|---|---|
| `workspace_root` | `String` | Unit's workspace root, from `CleanResult.display_path` |
| `output_roots` | `Vec<String>` | Every physical covering root the invocation can affect |
| `ownership` | `&'static str` | Ownership class of the unit |
| `policy_disposition` | `Option<String>` | Six-value typed disposition; `None` when no policy evaluated the unit |
| `outcome` | `String` | `previewed`/`simulated`/`cleaned`/`skipped`/`failed` |
| `reason_code` | `String` | 19-value stable machine code |
| `before_bytes` | `Option<u64>` | **`None` whenever a selector is active** |
| `selector_estimate_bytes` | `Option<u64>` | Non-null only when **no** selector is active (§7 D1) |
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

### `CleanupSummaryV1` — `output.rs:93-101`

| Field | Type | Meaning |
|---|---|---|
| `previewed` | `usize` | Units outcome `previewed` |
| `simulated` | `usize` | Units outcome `simulated` |
| `cleaned` | `usize` | Units outcome `cleaned` |
| `skipped` | `usize` | Units outcome `skipped` |
| `failed` | `usize` | Units outcome `failed` — **recomputed**, see §7 D10 |
| `diagnostics` | `usize` | Copied from `CleanReport.diagnostics` |

The five outcome counters are tallied by matching `CleanOutcome`
(`output.rs:194-203`), not read from the domain. They are mutually exclusive and
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
projection time. The fold (`output.rs:138`) sums the *projected* DTOs rather than
the domain structs — same values, but the JSON total and the human total are
computed by two separate folds over two separate collections (`output.rs:138`,
`report.rs:50-53`) that happen to agree.

### `EligibleOutputGroup` → `ScanGroupV1`

The source type is **`EligibleOutputGroup` (`domain.rs:222-231`)**, not
`PhysicalOutputGroup`. `PhysicalOutputGroup` (`domain.rs:205-219`) is the
*measurement-stage* type and never reaches either output module — it is confined
to `workspace.rs` (906, 1196, 1232-1233, 1298) plus one `cleanup.rs` test fixture.
`main.rs:471-483` and `tests/end_to_end.rs:174-187` both perform the narrowing
conversion, dropping `covering_roots`, `owners`, and `uncertain`, and lowering
`owners: Vec<WorkspaceId>` to `workspace_roots: Vec<PathBuf>` by taking `.0` of
each id. `report.rs` and `output.rs` never see the richer type and could not emit
its fields even in principle.

| Domain field | DTO field | Transform |
|---|---|---|
| `display_path: PathBuf` | `display_path: String` | `output::path` — lossy |
| `workspace_roots: Vec<PathBuf>` | `workspace_roots: Vec<String>` | per-element `output::path` |
| `physical_paths: Vec<PathBuf>` | `physical_paths: Vec<String>` | per-element `output::path` |
| `bytes: u64` | `bytes: u64` | direct — number, not a formatted string |
| `metric: SizeMetric` | `size_metric: &'static str` | enum → `"allocated"`/`"apparent"` |
| `newest_mtime: Option<SystemTime>` | `newest_activity_unix_seconds: Option<u64>` | `SystemTime` → Unix seconds, fallible |
| `ownership: OutputOwnershipClass` | `ownership: &'static str` | `.label()` (`domain.rs:194-201`) |
| `artifact_entries: u64` | *dropped* | — |

**Type change worth naming:** the human renderer formats bytes into
`"  1.50 MiB"`; the machine projection emits a raw `u64`. Correct — byte
formatting is presentation and belongs to the consumer, and precisely why
`format_bytes` is not shared with `output.rs`.

**`output::path` is lossy and irreversible** (`output.rs:256-260`): `to_str()` if
representable, else `format!("{:?}", p.as_os_str())`. The schema document says so
at line 25: *"the display string uses the platform escaped debug form and is not a
reversible identity encoding."* A consumer must not use these strings as join
keys. The human path instead uses `to_string_lossy` (`report.rs:11`), so **the two
projections render the same non-UTF-8 path differently** — machine gets
`"/tmp/bad-\\xff"`, human gets `"/tmp/bad-\u{FFFD}"`. Deliberate, per the comment
at `report.rs:7-9`, but a real difference.

### `ScanDiagnostic` → `DiagnosticV1`

| Domain field | DTO field | Transform |
|---|---|---|
| `severity: DiagnosticSeverity` | `severity: String` | `.as_str()` (`domain.rs:59-65`) |
| `category: DiagnosticCategory` | `category: String` | 6-arm `match` (`output.rs:125-133`) |
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
| `scope_blocked: Option<String>` | `scope_blocked: bool` + `scope_reason` | **split**: one field into two |
| `unresolved_ownership: Vec<…>` | `unresolved_ownership: usize` | **count only** |
| `unresolved_ownership: Vec<…>` | `unresolved_participants: Vec<…>` | **same data twice** |
| `selected_roots: Vec<PathBuf>` | `selected_roots: Vec<String>` | per-element `output::path` |
| `effective_policy: Option<CleanupPolicy>` | `effective_policy: Option<PolicyV1>` | struct projection |
| `selector: Option<CleanupSelector>` | `selector_kind` + `selector_value` | **one enum split into two keys** |
| `results: Vec<CleanResult>` | `units: Vec<CleanupUnitV1>` | per-element projection |
| `mode: CleanMode` | envelope `mode: Option<String>` | `.as_str()` (`cleanup.rs:45-51`) |
| `diagnostics: usize` | `summary.diagnostics: usize` | direct |
| — | `summary.{previewed,…,failed}` | **computed**: counted from `results` |
| `failed: usize` | *dropped* (recomputed) | see §7 D10 |
| `counters: ScanCounters` | *dropped* | — |

**The `scope_blocked` split is a real semantic change.** The domain carries one
`Option<String>`; the contract carries a `bool` and a nullable string. The `bool`
is `is_some()` (`output.rs:215`) and the string is a clone of the same value
(`output.rs:216`), so they cannot disagree — but the boolean is *derived state*.

**`selector` becomes two keys.** `CleanupSelector` is a two-variant enum
(`cleanup.rs:196-199`); `kind()` and `value()` (`cleanup.rs:202-212`) are applied
independently at `output.rs:234-235`, and both are `None` together. The string
form is not a serde tag, which is why there is no `#[serde(tag = ...)]` here.

### Policy, selector, and the code enums

| Domain | DTO | Transform |
|---|---|---|
| `CleanupPolicy` | `PolicyV1` | all four fields direct; no loss |
| `CleanupSelector::Profile(s)`/`::Package(s)` | `selector_kind` + `selector_value` | enum split; **lossless** |
| `CleanupReasonCode` (19 variants) | `reason_code: String` | `.as_str()` (`cleanup.rs:140-162`) |
| `PolicyDisposition` (6 variants) | `policy_disposition: Option<String>` | hand-written `policy_code` (`output.rs:243-254`) |
| `CleanOutcome` (5 variants) | `outcome: String` | `.as_str()` (`cleanup.rs:66-74`) |

Note the inconsistency in how these three enums reach the wire. `CleanOutcome` and
`CleanupReasonCode` go through `as_str()` methods on the type;
`PolicyDisposition` — which *has* `#[derive(Serialize)]` with
`rename_all = "snake_case"` available at `cleanup.rs:165-166` — is converted by a
private helper instead. The helper produces the same six strings serde would, so
behaviour matches; the helper is simply where a seventh variant would fail
visibly in the contract layer, which is arguably the point.

### `UnresolvedOwnershipParticipant` → `UnresolvedParticipantV1`

| Domain | DTO | Transform |
|---|---|---|
| `manifest: PathBuf` | `manifest: String` | `output::path` (lossy) |
| `stage: &'static str` | `stage: &'static str` | **verbatim pass-through** |
| `reason: String` | `reason: String` | `.clone()` |

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
| `before_bytes: Option<u64>` | `output_union_before_bytes` | direct — the *third* copy |
| `after_bytes: Option<u64>` | `after_bytes: Option<u64>` | direct |
| `observed_decrease: Option<u64>` | `observed_decrease_bytes: Option<u64>` | renamed |
| `detail: String` | `detail: String` | `.clone()` |
| `workspace_roots: Vec<PathBuf>` | *dropped* | superseded by `output_roots` |

`before_bytes` is projected into **three** keys, gated on
`report.selector.is_some()` per result (`output.rs:174-183`):

| Run shape | `before_bytes` | `selector_estimate_bytes` | `output_union_before_bytes` |
|---|---|---|---|
| Selector active | `null` | `null` | value |
| No selector | value | **value** | value |

Without a selector all three carry the identical `u64`; `selector_estimate_bytes`
is then a "selector estimate" for a run that has no selector. This is the worst
divergence in the contract (§7 D1).

### Dropped domain fields, and why that is correct

| Dropped | Where | Why correct |
|---|---|---|
| `ScanReport.visited_entries` | `domain.rs:87` | Internal walk accounting; no consumer decision depends on it |
| `ScanReport.counters` | `domain.rs:89`, 35 fields | Prune tallies and nanosecond timings are `--stats` stderr material (`main.rs:542-549`); the schema keeps them off the contract |
| `EligibleOutputGroup.artifact_entries` | `domain.rs:229` | Count of walked entries; diagnostic, not a reclaim-decision input |
| `PhysicalOutputGroup.covering_roots` / `owners` / `uncertain` | `domain.rs:207,212,218` | Never reach these modules — dropped upstream at `main.rs:471-483` |
| `CleanReport.counters` | `cleanup.rs:182` | Same as `ScanReport.counters`; cleanup `--stats` is stderr (`main.rs:247-255`) |
| `CleanReport.failed` | `cleanup.rs:180` | Recomputed from results (`output.rs:194-203`); the domain field remains an exit-code input |
| `CleanResult.workspace_roots` | `cleanup.rs:81` | Redundant with `workspace_root` + `output_roots` |
| `PathBuf` types generally | — | Contract is `String`; identity is deliberately not re-derivable |

All are either `--stats` stderr material or measurement internals. The schema
document covers the principle for the one that reaches it at line 10: *"Inventory
bytes describe observed output and are not recovered bytes."*

---

## 7. Schema conformance

Compared against `plans/output-schema-v1.md` (27 lines). Divergences are
itemised below; the table is the coverage map.

| Contract element | In code? | Notes |
|---|---|---|
| `schema_version` | yes | `output.rs:9,140,207`; literal `1`, two sites |
| `cargo_cleanme_version` | yes | `env!("CARGO_PKG_VERSION")` |
| `operation` | yes | `scan` / `clean` |
| `scope` | yes | caller-supplied; value set is open |
| "optional cleanup `mode`" | **partial** | always emitted, `null` for scan → **D6** |
| `result` | yes | generic parameter |
| Scan `discovered_manifests` | yes | |
| Scan `groups` (7 keys) | yes | all 7 present, names match exactly |
| Scan `diagnostics` (4 keys) | yes | |
| Scan `summary` (3 keys) | yes | |
| Cleanup scope-coverage trio | yes | `discovered_manifests` / `resolved_workspaces` / `units_considered` |
| Cleanup `selected_roots` | yes | |
| Cleanup `effective_policy` | **partial** | key present, 4 inner fields undocumented → **D4** |
| Cleanup `state_generation_last_full_at` | yes | |
| Cleanup `selector_kind` / `selector_value` | yes | |
| `selector_estimate_bytes` nullity rule | **no** | **D1** — most significant divergence |
| `output_union_before_bytes` semantics | yes | always `r.before_bytes` |
| `before_bytes` null for selector cleanup | yes | |
| Cleanup `scope_blocked` / `scope_reason` / `unresolved_ownership` | yes | |
| Cleanup `unresolved_participants` (3 keys) | yes | |
| Cleanup `units` keys | yes | 12 keys; `detail` optionality wrong → **D5** |
| `reason_code` value list (18 listed) | **partial** | code emits a 19th → **D3** |
| `units[].outcome` value set | **no** | never enumerated → **D9** |
| `summary` (6 keys) | yes | `failed` source differs → **D10** |
| Policy disposition stable values (5 listed) | **partial** | code emits 6 → **D2** |
| "one UTF-8 JSON document + newline" | yes | `println!` `main.rs:497,217,182`; asserted `cli_contract.rs:204,242` |
| Path encoding rule (line 25) | yes | `output.rs:256-260` |
| "`--stats` is stderr-only" | yes | `main.rs:247,545`; asserted `cli_contract.rs:198` |
| "Progress is disabled for JSON output" | yes | `main.rs:196,311` gate on `Human` |
| Exit codes 0 / 1 / 2 | **partial** | right for cleanup; scan has an extra 1 → **D7** |
| Success/failure indicator | **absent** | no such field anywhere → **D8** |

### Divergences, document → code

**D1 — `selector_estimate_bytes` nullity is inverted relative to the document.**
`plans/output-schema-v1.md:16` promises: *"A selector's `selector_estimate_bytes`
is `null` unless a trustworthy estimate exists."* The code (`output.rs:179-183`)
makes it `null` **iff a selector is active**, inverting both halves:

- For a selector run it is *always* `null`. The "unless a trustworthy estimate
  exists" escape hatch has **no implementation** — no reachable state emits a
  non-null estimate for a selector run. (Consistent with line 23, "Any nonzero
  minimum-size policy with a selector is rejected because selector-specific bytes
  are unknown", but the document promises a possibility the code never exercises.)
- For a **non**-selector run it is **populated** with `r.before_bytes` — a
  "selector estimate" emitted for a cleanup with no selector. The document never
  describes this case.

Consequence: the field is only meaningful as a *negative* signal. Its presence
tells you nothing; its absence tells you either "a selector was used" or "no
trustworthy estimate was available". A consumer reading it as selector bytes for a
selector run gets `null` and correctly refuses to estimate; a consumer reading it
on a plain run gets a duplicate of `output_union_before_bytes` and may mistake it
for something selector-specific. Pinned by `cli_contract.rs:386-389`, so the
behaviour is intended — but undocumented.

**D2 — the "stable policy disposition values" enumeration is short one value.**
Line 25 lists five: `selected`, `below_minimum_size`, `too_recent_for_policy`,
`not_included`, `excluded`. The code emits six — `policy_code`
(`output.rs:243-254`) also produces `selector_estimate_unavailable`, from
`PolicyDisposition::SelectorEstimateUnavailable` (`cleanup.rs:173`). The document
does acknowledge the variant at line 20 and `cli_contract.rs:462-465` pins it, so
the code is intended and the line-25 list is the stale one. As written it reads as
a closed enumeration and is wrong.

**D3 — the `reason_code` enumeration omits a value the code emits.** Line 20 lists
18 values, prefaced "Current values include" (non-exhaustive, so a soft miss).
`CleanupReasonCode` (`cleanup.rs:96-119`) has **19** variants and emits
`skipped_ownership_unproven` (`cleanup.rs:155`), documented in the source as
*"Ownership could not be re-proven at all … Distinct from a workspace that
demonstrably changed."* A consumer building a closed enum from line 20 has no arm
for it. The distinction is semantically load-bearing — "could not prove" and
"proved it changed" deserve different operator responses.

**D4 — `effective_policy` internals are undocumented.** Line 15 names
`effective_policy` as a scope/policy input but never enumerates its contents. The
code emits four fields (`output.rs:72-77`): `min_reclaimable_bytes`,
`min_inactive_seconds`, `include`, `exclude`. A consumer cannot write against
`effective_policy` from this document.

**D5 — `units[].detail` is described as optional but is always a string.**
Line 19 says *"optional human detail"*. `CleanupUnitV1.detail` is `String`
(`output.rs:91`), populated by `.clone()` (`output.rs:187`). The key is always
present; the "no detail" state is `""`. A consumer testing `unit["detail"] == null`
never matches; one testing `"detail" in unit` always matches.

**D6 — `mode` is described as conditionally present but is always emitted.**
Line 3 lists the top-level object as containing "optional cleanup `mode`".
`EnvelopeV1.mode` is `Option<String>` with **no** `skip_serializing_if`
(`output.rs:13`), so serde emits `"mode": null` on every scan document
(`output.rs:144`). A consumer branching on key presence rather than value is
wrong.

**D7 — the exit-code contract is stated for cleanup only, and scan has an extra
`1`.** Line 27: *"The process exits 0 for completed requests with safe per-unit
skips, 1 for incomplete cleanup scope or operational cleanup failure, and 2 for
fatal invocation/configuration errors."* Cleanup matches: `main.rs:257-261` returns
1 when `report.failed > 0 || report.scope_blocked.is_some()`; `main.rs:9-12`
returns 2 for any `AppError`. Two uncovered cases:

- The **scan** path also returns 1, for `full_incomplete || (full &&
  !state_reconciled)` (`main.rs:560-561`) — conditions unrelated to cleanup scope
  or unit failure. A `scan --full` on a machine with a `PlatformRoot` error, or an
  unreconcilable state file, exits 1 with a structurally normal JSON document.
- A scan with permission-denied, metadata, or vanished diagnostics exits **0**. A
  consumer treating exit 0 as "nothing was wrong" is misled; it must read
  `result.diagnostics`.
- `config edit` propagates the editor's own exit code (`main.rs:64`), outside the
  stated contract entirely.

**D8 — no success indicator, and exit 2 produces no JSON at all.** Neither the
document nor `EnvelopeV1` carries `status`, `success`, or `exit_code`. Failure is
signalled only by `scope_blocked`/`scope_reason` and `summary.failed` *content*,
plus the process exit status. Worse, every `Err` return from `run()` — including
`serde_json::to_string` failures at `main.rs:509` and `main.rs:230` — prints to
**stderr** and exits 2 with **stdout empty** (`main.rs:9-12`). A consumer reading
stdout cannot distinguish "never got that far" from "found nothing".

### Divergences, code → document

**D9 — `units[].outcome`'s value set is never enumerated.** The document names
`outcome` ("operation outcome", line 19) and separately enumerates `summary`'s five
counters (line 21), but never says what `outcome` may contain. The code emits
`previewed`, `simulated`, `cleaned`, `skipped`, `failed` (`cleanup.rs:66-74`). A
consumer must infer the set from the summary field names.

**D10 — `summary.failed` is recomputed, not read from the domain field.**
`output.rs:190-203` builds `CleanupSummaryV1` and tallies all five outcome
counters by matching `CleanOutcome`, including `failed`. It never reads
`CleanReport.failed` (`cleanup.rs:180`). Meanwhile `main.rs:257` computes the exit
code from `report.failed`. Two independent mechanisms that must agree. **They
currently do, exactly**: `report.failed` is incremented at `cleanup.rs:1072` and
`cleanup.rs:1091`, and those are the only two sites, each paired with a
`CleanOutcome::Failed` push at `cleanup.rs:1078` and `cleanup.rs:1097`. But nothing
enforces the coupling — a future `Failed` push that forgets the increment would
make `summary.failed: 0` in a document whose exit code is 1, with no test
catching it.

### The stability promise, and whether the code honours it

`plans/output-schema-v1.md:3` carries the operative sentence:

> "A backwards-incompatible field or meaning change requires a schema-version
> decision."

This is a *procedural* promise, not a compatibility guarantee, and as a promise it
is currently honoured: `schema_version` is a pinned literal at both projection
sites (`output.rs:140`, `output.rs:207`), there is no runtime negotiation or
feature gating, and no field in any DTO is documented as deprecated. Nothing in the
current code breaks the promise — D1 through D5 are defects in the *schema
document*, not changes to a shipped v1.

Two qualifications. First, **D1 and D2 are live contract ambiguities, not just
doc rot**: a consumer written from lines 16 and 25 would implement
`selector_estimate_bytes` handling and disposition enums that do not match the
tool. The code has not broken its promise; the document has not been kept in step.
Second, **`update_json` (`main.rs:568-590`) reuses `schema_version: 1` without the
envelope** — no `scope`, no `mode`, different `result` shape. The document scopes
itself to the scan/cleanup surfaces, so this is not a violation as written, but a
consumer treating `schema_version == 1` as "the envelope in this document" will
mis-parse the `update` document. `update_json` also has no tests at all (§9).

### Practical consequence for a consumer

**A script written strictly against the schema document cannot rely on the tool's
current output.** It is safe for the scan surface — the 13 scan-side field names
match exactly, and the four span behaviours (single newline, non-UTF-8 path form,
`--stats` isolation, progress suppression) all hold in code. It breaks in three
specific places: any use of `effective_policy`'s contents (D4); any `reason_code`
or `policy_disposition` switch exhaustive over the document's enumerations
(D2, D3); and any use of `selector_estimate_bytes` as a positive value (D1 — the
field's populated case is exactly the case the document never describes).
Everything outside those is safe today. A consumer treating the document as
*descriptive* and the tool as *authoritative* is fine; one treating it as
*normative and exhaustive* is not.

---

## 8. Invariants and edge cases

**Is the JSON valid for every reachable domain state?** Yes. Traced:

- *Empty scan* — `ScanReport::default()` → `output.rs:104-119` maps empty vecs to
  empty vecs; `group_count: 0`, `inventory_bytes: 0` (fold over nothing). Valid.
- *Zero groups but diagnostics* — same, plus the full diagnostic list. Valid.
- *Cleanup that spawned nothing* — `main.rs:174-193` builds
  `CleanReport { mode, effective_policy: Some(policy), selector, ..Default::default() }`.
  `results` is empty, so `units: []` and all five summary counters are `0`
  (`output.rs:190-203` over an empty iteration). `mode` is `Some`, `scope` is
  `"full"` or `"known"`. Valid, and asserted at `cli_contract.rs:241`.
- *Scope-blocked cleanup* — `cleanup.rs:839-853` returns early with `results`
  **empty** but `units_considered` already set (`cleanup.rs:833`). A consumer can
  see `units_considered: 7, units: []`. Structurally valid, semantically
  surprising, and not warned about in the document.
- *No `Option` produces malformed output.* All `Option` fields serialise as `null`
  under a plain derive; there is no `flatten` and no `skip_serializing_if` to
  interact badly with, and serde's custom-error paths are unreachable from these
  types (owned scalars, `String`, `Vec`, one `&'static str`).

**Misleading `None`s.** The real hazard is D1's `selector_estimate_bytes`. Beyond
that, `newest_activity_unix_seconds: null` conflates "no mtime" with "mtime before
the Unix epoch" (`output.rs:116`, `261-265`), and `before_bytes: null` conflates
"selector active" with "not measured". Neither is malformed; both can be misread.

**Ordering determinism.**

- *Human*: fully deterministic. `render` sorts bytes-desc then path-asc
  (`report.rs:45-49`), the tie-break makes equal-byte groups stable, and the total
  is a pure fold. Pinned at `report.rs:130`, `end_to_end.rs:210`.
- *JSON*: **not self-deterministic.** `output::scan` imposes no ordering at all —
  it maps `report.groups` in place (`output.rs:104-119`) and inherits whatever
  order the pipeline produced. The "deterministic" at `output-schema-v1.md:8` is
  a promise about `workspace::build_groups`/`analyze_groups`, not about this
  module.
- *Cleanup is worse*: `CleanReport::render` sorts only a local `ordered` vec
  (`cleanup.rs:252-258`) and leaves `report.results` untouched, so the JSON `units`
  array is in unsorted `results` order while the human rows are size-desc. **The
  two projections genuinely disagree on ordering for cleanup.** I did not read
  `workspace.rs` in depth, so I cannot certify the inherited scan order either;
  the human surface is self-certifying, the machine surface is not.
- *Filesystem iteration*: neither module walks a directory. Both consume
  already-collected `Vec`s. Ordering determinism is inherited, not established, for
  JSON.

**Can `render` panic?** No. The only indexing is `UNITS[unit]`
(`report.rs:42`), bounded to `[0, 6]` by the `unit + 1 < UNITS.len()` guards at
`report.rs:31` and `37`. There is no division by a possibly-zero value — the only
divisors are the constants `1024` and `100`. No unwrap, no arithmetic overflow
(`saturating_add`, `report.rs:53`), no slice range. An empty group list yields
`"\n0.00 B inventory estimate across 0 inactive Cargo output groups\n"`, asserted
at `report.rs:126`. `u128::from(n) * 100` at `report.rs:29` cannot overflow:
`u64::MAX * 100` ≈ 1.8e21, far below `u128::MAX`.

**Precision loss in byte totals?** **No, definitively.** `format_bytes` contains
no float conversion — the ladder is `u128` integer arithmetic
(`report.rs:29-41`) and the comment at `report.rs:28-29` names the hazard
explicitly. `report.rs:152` pins `u64::MAX → "16.00 EiB"`, representable only in
integer form. In the machine projection bytes stay `u64` and are never formatted.
(The `f64` conversions that do exist — `domain.rs:320-325` `timings_line`,
`main.rs:254` `as_secs_f64` — are all `--stats` stderr output, and
`ScanCounters` never reaches JSON.)

**Escaping hazards.** The two projections differ, deliberately:

- *Human*: `path_text` (`report.rs:10-24`) escapes **every** control character to
  `\u{hex}` if any is present, and lossy-converts non-UTF-8. ANSI sequences are
  neutralised — ESC is `0x1b`, a control character, so it is rewritten. Covered
  by `report.rs:90-100`. Terminal injection through a directory name is closed.
- *JSON*: `output::path` (`output.rs:256-260`) does **no** escaping of its own; a
  valid-UTF-8 path passes through verbatim, ANSI escapes included. Serde's string
  escaper maps C0 control characters (including `0x1b`) to `\u00XX`, so the
  *document* is always valid JSON and cannot be corrupted or broken out of — but
  the ESC byte survives as content. That is correct for a machine contract
  (faithfully reporting the path is the job), and it means a consumer that `cat`s
  a path field to a terminal re-opens the hazard the human path closes. The
  document has no consumer guidance on this.
- `detail` (`output.rs:187`) is cloned unfiltered; `cleanup.rs:333` strips
  newlines for the *human* line only, so the stored value can contain `\n` and
  reaches JSON as an escaped `\n`. Not a hazard.
- Invalid UTF-8 cannot corrupt the JSON: `output::path` always produces a valid
  `String`.

**Do human and JSON ever disagree about the same fact?** Yes, in four places:

1. **Ordering of cleanup units** — human size-desc with a path tie-break
   (`cleanup.rs:252-258`), JSON raw `results` order.
2. **Non-UTF-8 path rendering** — machine gets the escaped debug form, human gets
   U+FFFD (`output.rs:256-260` vs `report.rs:11`).
3. **Byte representation** — human prints `"  1.50 MiB"`, JSON prints `1572864`.
   One number, two representations; a textual diff will not match them.
4. **Diagnostic completeness** — JSON has all of them, human stderr shows 10 plus a
   count (`main.rs:523-537`).

`end_to_end.rs:193-210` is the one place the project actively checks the two
projections agree, and it checks the three things that matter: same group
membership, same `group_count`, and the human string containing the machine's
`bytes` value (`end_to_end.rs:210`).

**Can `scope` be wrong?** The three scan and three cleanup branches pass correct
values, and the cleanup empty-roots path correctly cannot produce `"explicit"`
(`main.rs:186`), because an explicit root always yields a non-empty `roots`
vector. Two observations. `clean --full` that falls back to zero learned roots
reports `scope: "full"`, which is accurate. And `scope` is a bare caller-supplied
`String` with no validation — nothing in `output.rs` would reject a wrong value,
so the guarantee is entirely the caller's. `scan` with an explicit root *and*
`--full` reports `"full"` (`main.rs:501-503` checks `full` first), losing the fact
that the user also bounded the root. Not wrong, but lossy.

**Exit-code contract, and is success in the JSON?** Exit codes are stable and
documented at line 27, and the code matches for cleanup (D7 qualifies scan).
**There is no success or failure field in any envelope.** A consumer must branch on
the process exit status plus `result.scope_blocked` and `result.summary.failed`.
For `scan`, diagnostics never affect the exit code, so `result.diagnostics` is the
only signal. For exit code 2 there is no JSON at all (D8).

---

## 9. Testing

**`report.rs` — 7 tests, all inline** (`#[cfg(test)] mod tests`, `report.rs:72-195`):

| Test | Line | Covers |
|---|---|---|
| `report_escapes_control_bytes_in_paths` | 90 | ESC/BEL in a UTF-8 name escaped, never raw |
| `report_escapes_non_utf8_paths` | 103 | raw `\xff` → U+FFFD; no control survives |
| `report_ordering_and_zero_report` | 124 | zero-state wording **and** path tie-break |
| `zero_state_uses_the_same_inventory_wording_as_results` | 133 | "inventory estimate" in both states; never "reclaimable" |
| `format_bytes_scales_to_eib_and_promotes_rounded_units` | 144 | whole ladder incl. `u64::MAX` and the 1024.00 promotion |
| `groups_render_every_group_with_ownership_and_deduped_total` | 155 | ordering, all three labels, every group printed, no "recovered" |
| `groups_total_does_not_double_count_equal_roots` | 184 | duplicate physical paths counted once; singular "1 group" |

The first two are `#[cfg(unix)]` (raw `OsString::from_vec`). This is thorough for a
renderer: escaping, ordering, zero state, unit ladder, and deduplication are all
pinned. It covers `render` and `format_bytes` only — there is no test for a
multi-line `detail` or a whitespace-only path.

**`output.rs` — 0 tests.** No `#[cfg(test)]` module in the file. Nothing in the
DTO layer is unit-tested: no test constructs an `EnvelopeV1`, no test asserts a
field name, and the `selector_estimate_bytes` inversion (D1) has no test of its
own — it is only visible because `tests/cli_contract.rs` happens to run the
selector path end to end.

**`main.rs` — 0 tests.** The task brief anticipated an inline test module here and
suggested `update_json` was covered by one. **It is not.** `main.rs` has no
`#[cfg(test)]` and no `mod tests`; `grep -c "#\[test\]" src/main.rs` returns `0`.
`update_json` (`main.rs:568-590`) is called from one place (`main.rs:25`) and
referenced nowhere in `tests/`. `UpdatePlan` is defined at `update.rs:247` and
tested inside `update.rs`'s own module, but the JSON *document* is untested. It
is also the only machine-readable surface that bypasses `EnvelopeV1`, so the
least-tested document is the one that diverges most from the shared envelope.

**`tests/cli_contract.rs` — pins real JSON shape, partially.** Shape assertions do
exist, which makes it a genuine contract:

| Line | Assertion |
|---|---|
| 198 | `--stats` does not change stdout bytes |
| 200-202 | `schema_version == 1`, `operation == "scan"`, `scope == "explicit"` |
| 204, 242 | stdout contains exactly one `\n` |
| 238-241 | `schema_version == 1`, `operation == "clean"`, `mode == "simulate"`, `summary.simulated == 0` |
| 270 | `result.scope_blocked == true` (with exit code 1) |
| 378-379 | `summary.cleaned == 1`, `units[0].reason_code == "cleaned"` |
| 380-381 | `selector_kind == "profile"`, `selector_value == "dev"` |
| 382-389 | `before_bytes` and `selector_estimate_bytes` both `Null` under a selector |
| 390-395 | `output_union_before_bytes >= 4096` |
| 426 | `units[0].outcome == "simulated"` |
| 458-465 | `reason_code == "selector_unsupported"`, `policy_disposition == "selector_estimate_unavailable"` |

So yes — **a test asserts JSON shape, and it is a real contract**: nine distinct
field paths are pinned by name, value, and type. But coverage is uneven and
follows the tests' *interests*, not the schema's. Pinned: the envelope (3 fields),
`scope_blocked`, the selector quartet, two reason codes. **Unasserted**:
`ScanV1`'s other fields, 14 of `CleanupV1`'s fields, and the shape of
`ScanGroupV1`, `DiagnosticV1`, `ScanSummaryV1`, `PolicyV1`,
`UnresolvedParticipantV1`, `CleanupSummaryV1` entirely. The non-selector branch of
the D1 inversion is never exercised, and nothing asserts the *absence* of a key.

**`tests/end_to_end.rs` — the only cross-projection check.** Lines 202-210 assert
`groups[0].ownership == "private"`, `summary.group_count == 1`, and that
`report::format_bytes(bytes)` appears in the human string. Four field paths, all
in the scan surface. No cleanup projection assertions.

**Honest assessment.** The contract's machine half is checked by exactly one
integration file, and only where that file's scenarios happen to reach the fields.
No test would fail if `PolicyV1` were renamed, if `DiagnosticV1::category` gained
a variant, if `update_json` changed shape, or if `summary.failed` desynchronised
from the exit code. D2 and D3 survive precisely because the only test touching
those enums (`cli_contract.rs:458-465`) happens to assert the one value the schema
document *does* mention. **This is the fragile case the brief anticipated**: a
versioned schema contract checked only by tests written against the
implementation, with no assertion that the implementation still matches the
document. The cheapest high-value fix is a fixture-driven conformance test that
reads the field names from `plans/output-schema-v1.md` and asserts each appears in
a real document.

---

## 10. Review checklist

1. **`selector_estimate_bytes` nullity, D1** — `output.rs:179-183` inverts the
   rule at `output-schema-v1.md:16`. Confirm the populated case (no selector →
   `Some`) is intended; if so the document needs rewriting, and
   `cli_contract.rs:386-389` pins only the null case.
2. **`render`'s `&mut` receiver, `report.rs:44-49`** — the only mutation is
   `report.groups.sort_by`. Confirm a pure `&ScanReport` plus a local sorted index
   is acceptable; note `end_to_end.rs:193` then `202` currently depends on the
   mutation reaching the JSON projection.
3. **Dropped fields, `output.rs:145-154` and `211-239`** — every `*V1` literal is
   hand-written with no `..Default`, except `CleanupSummaryV1` at
   `output.rs:190-193`. Confirm the exclusion property (a new `ScanReport` field
   cannot reach JSON) stays structural for fields added to the *DTOs*, where the
   `Default` fill makes it conventional instead.
4. **JSON ordering is inherited, not established, `output.rs:104-119`** — no sort
   here. Cross-check against the "deterministic" claim at `output-schema-v1.md:8`
   and against `cleanup.rs:252-258`, where the human renderer sorts a local vec
   that `output::cleanup` never sees, so cleanup `units` order differs between the
   two surfaces.
5. **`summary.failed` vs the exit code, `output.rs:194-203` vs `main.rs:257`** —
   two independent mechanisms that agree only because `cleanup.rs:1072/1078` and
   `1091/1097` are the sole paired sites. Re-verify that pairing on any diff
   touching failure paths.
6. **Enum enumerations** — `cleanup.rs:155` (`skipped_ownership_unproven`) is
   absent from `output-schema-v1.md:20`; `cleanup.rs:173` /
   `output.rs:251` (`selector_estimate_unavailable`) is absent from line 25's
   five-value list. Both are live values the code emits today.
7. **Empty and blocked states** — `main.rs:174-193` gives
   `units_considered: 0, units: []`; `cleanup.rs:839-853` gives
   `units_considered: N, units: []`. Confirm both are intended and that no
   consumer assumes `units.len() == units_considered`.
8. **`mode` and `detail` are not optional, `output.rs:13` and `output.rs:91`** —
   no `skip_serializing_if` anywhere, so `"mode": null` appears on every scan
   document and `detail` is `""` rather than absent. Any consumer written from the
   document's "optional" wording will mis-branch.
9. **No success field, and exit 2 emits no JSON, `main.rs:9-12` and
   `main.rs:509`** — a consumer reading stdout cannot distinguish "never ran" from
   "ran and found nothing". Consider whether a status field belongs in a future
   schema version rather than v1.
10. **`update_json` reuses `schema_version: 1` without the envelope,
    `main.rs:568-590`** — no `scope`, no `mode`, different `result` shape, zero
    tests. Confirm consumers of `schema_version == 1` are scoped per `operation`,
    and that the `update` document should be documented alongside the others.
11. **Keep byte formatting float-free, `report.rs:29-32`** — any change
    introducing `n as f64` silently breaks `report.rs:152`
    (`u64::MAX → 16.00 EiB`), the only guard at the `2^53` boundary.
