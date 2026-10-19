---
name: json-and-exit-contract
description: cargo-cleanme machine-readable surfaces — the versioned envelope, typed reason codes, exit codes, and the human/JSON parity that a schema change must preserve
version: 1.0.0
tags:
  - json
  - schema
  - exit-codes
  - contract
---

# JSON and Exit Contract

Every machine-readable surface in this CLI is a **versioned envelope**. The
contract is normative in
[`plans/output-schema-v1.md`](../../plans/output-schema-v1.md); the rendering is
in `architecture/10-reporting.md`.

## When to Load

- Any change to `src/output.rs`, `src/domain.rs`, or `src/report.rs`
- Adding or renaming a field, a `reason_code`, or an exit code
- Touching `update_json` or the editor operation envelope
- Changing anything a user might parse

## The envelope rules

1. Every versioned surface is **exactly one** `EnvelopeV1` JSON object printed
   to **stdout** in a single `println!`. (`EnvelopeV1<T>` at
   `src/output.rs:8` is the only envelope type; there is no `Enveloped`.)
   Not NDJSON. Not one object plus a trailing line. Not pretty-printed across a
   pipe.
2. `schema_version` is an integer, an **untyped** `Serialize` value, and a
   literal `const`. Its value is a separate decision from the field set.
3. Diagnostics, statistics, and progress are **not** in the JSON document.
   Diagnostics go to stderr. `--stats` goes to stderr. Progress is a TTY
   renderer. "Make `--stats` part of the JSON" is a contract change.
4. Absent is not zero. An omitted field means the concept does not apply;
   `0` asserts a measured value. Do not emit a zero to fill a gap.
5. `bytes` are reported as **platform-independent integers** (u64). The
   display layer may abbreviate and add a unit; the JSON value never carries one.

## The exception you must know about

`update_json` is **not** an envelope. It bypasses `EnvelopeV1` and hand-rolls a
`serde_json::Map` with its own `schema_version` literal, no `scope`, no `mode`,
and a differently shaped `result` object.

It used to be the only output mode in the CLI that was **untested**. It is no
longer: C022 pinned the machine contract with tests, in the `update_tests`
module at `src/output.rs:946` —
`the_dry_run_document_pins_the_schema_and_the_identity_fields` and
`the_mutating_document_is_the_same_shape_with_changed_true` both assert the
document's shape field by field, and `update_json_stream`
(`src/output.rs:902`) makes the document-plus-newline stream contract a value
tests hold byte for byte.

Migrating it to `EnvelopeV1` is still a **contract change**: it needs a plan
and a version bump, not a quiet refactor. Adding a test is no longer the
obvious next step, because the obvious next step has been taken.

## Two known gaps in the envelope, recorded not fixed

The JSON contract **drops** two fields the domain carries. Both are open
findings in `architecture/overview.md` §10, not deliberate omissions:

- `capability` — present on the domain type (`src/domain.rs:198`), absent from
  the serialized participant (`src/output.rs:68-72`).
- `out_of_scope_unresolved` — present on the cleanup report
  (`src/cleanup.rs:414`), absent from the envelope.

A consumer that needs them today has to derive them, which means guessing. If
you close either gap, it is an additive field — the kind of change that is
allowed without a version bump — but it still needs the envelope's
"absent is not zero" rule respected: emit the real value, never a placeholder.

## `scope` is an open string

`scope` is `ScanScope::as_str()` in each surface and is typed from
`main::scope_label`, which derives it from the **resolved `policy.scope`** — not
from the CLI flags. It was derived from the flags, so a configured `scan.root`
resolved to `Explicit` and still reported `"routine"`. A consumer must treat
`scope` as an open string: do not add a closed-enum assumption to a consumer,
and do not derive the label from flags.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | success |
| `1` | `full_incomplete` on scan, or a zero-deletion-with-typed-blocker on cleanup (the `scope` block path) |
| `2` | **every** fatal error, including CLI usage errors |

**There are only three exit codes.** `src/main.rs:34` is the only place the
process exits non-zero, and it exits `2` for every `AppError` — configuration
errors, invalid roots, I/O errors, scan failures, update refusals, and update
failures alike. The `AppError` variants are distinguished by their
`reason=` **log-format** code (`config`, `io`, `scan`, `provenance`, `update`),
not by their exit status.

> **Do not trust a table that claims `17` or `18`.** Those codes do not exist
> in this codebase. `grep -rn '\b17\b\|\b18\b' src/` finds nothing. An earlier
> version of this skill asserted them, and a consumer written against that
> table would branch on a status the tool never returns.

Two things to keep straight:

- **`1` is overloaded** across two commands with two different causes, and the
  `scope` block path is a *success-shaped* result with a typed blocker. The JSON
  block is emitted with a non-zero exit status. A consumer must read the block
  rather than inferring from the status.
- **State persistence does not affect the exit code.** It did once: a failed
  Full publish returned `Ok(1)`. The flag is gone. A scan is informational;
  state is an optimization. See
  [`discovery-and-ownership`](../discovery-and-ownership/SKILL.md).

## Reason codes

Cleanup emits typed `reason_code`s. The full, current set is the twenty
variants of `ReasonCode` in `src/cleanup.rs:193-286`:

```text
previewed  simulated  cleaned  below_minimum_size  too_recent_for_policy
not_included  excluded  skipped_active  skipped_shared  skipped_uncertain
skipped_unauthorized  skipped_marker_invalid  skipped_changed_before_cleanup
skipped_ownership_unproven  skipped_safety  skipped_outside_selection
cargo_failed  measurement_failed  selector_unsupported  selector_invalid
```

`plans/output-schema-v1.md` § *reason_code* lists nineteen of them — it is
missing `skipped_outside_selection`, which is a live documentation gap (C036
D11).

The distinctions exist because they warrant **different operator responses**:

- `skipped_ownership_unproven` — an external participant is there. Understand it
  before proceeding.
- `skipped_changed_before_cleanup` — the candidate changed since measurement.
  Re-run.
- `skipped_unauthorized` / `skipped_marker_invalid` / `skipped_safety` — a
  safety or environment problem.
- `skipped_outside_selection` — the candidate is outside the scope that was
  actually selected, so it was never a candidate. Not a failure.
- `previewed` and `simulated` are the expected outcomes in `CargoPreview` and
  `Simulate` mode respectively. There is **no** `skipped_dryrun` code.

> **Do not invent reason codes.** `skipped_permission_denied`,
> `skipped_io_error`, `skipped_never_authoritative`, and `skipped_dryrun` do
> not exist anywhere in this repository — not in `src/`, not in `tests/`, not in
> the schema doc. An earlier version of this skill listed all four. A consumer
> written against them would match nothing.

Collapsing two of the real codes into one generic `skipped` is a contract change
that degrades every consumer, not a simplification.

## Parity is the thing to test

The strongest available assertion is that **human and JSON are two renderings of
one result**:

1. Non-TTY stderr is empty, or the same for JSON.
2. The `scope` block and the first diagnostic have line-number parity, and their
   line numbers in stderr match the JSON diagnostic's.
3. The human path's `(envelope)` line and the JSON path's `source_activity`
   diagnostic describe the same span.
4. A mutation to the report's field order does not change either surface.

Regenerate derived artifacts after any change to the rendered surface:

```sh
cargo run --quiet --features dev-tools --bin generate-docs
```

That regenerates the five shell completions and the man pages. CI fails on
drift. The generated artifacts document the `cargo cleanme` spelling, which is
what works after a registry install — so if you change the CLI, the diff you
did not commit is a failing build, not a detail.

## Before you claim the change is safe

- The JSON document is still exactly one object on stdout, and
  `cargo cleanme scan --format json` round-trips.
- New fields are additive, or the version is bumped and the change has a plan.
- `cargo cleanme update --format json` is unchanged, or the change is
  explicitly scoped and recorded as a contract change.
- The reason codes you emit are **real** — cross-check them against the
  `ReasonCode` enum, not against this prose, which has been wrong before.
- The exit code you emit is `0`, `1`, or `2`. There is no fourth.
- `scripts/gen-release-workflow-shape.py --check` and the completions drift
  gate pass.

## Related

- [`test-evidence`](../test-evidence/SKILL.md) — proving parity rather than
  asserting it
- [`cli-surface-and-argv`](../cli-surface-and-argv/SKILL.md) — what selects the
  surface, and the usage error that shares exit code `2` with every fatal error
- [`terminal-and-interactive`](../terminal-and-interactive/SKILL.md) — the
  interactive path's exit codes, which do not always agree with the
  non-interactive path
