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

1. Every versioned surface is **exactly one** `Enveloped`/`EnvelopeV1` JSON
   object printed to **stdout** in a single `println!`. Not NDJSON. Not one
   object plus a trailing line. Not pretty-printed across a pipe.
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

It is also the only output mode in the CLI that is **untested**. That is a known,
recorded gap, not an accident. If you are the next person to touch it: migrating
it to `EnvelopeV1` is a good change, but it is a contract change and needs a
plan and a version bump, not a quiet refactor. Adding a test for it is a good
change and needs neither.

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
| `2` | CLI usage error |
| `17` | config invalid or unreadable |
| `18` | environment not supported |

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

Cleanup emits typed `reason_code`s, and the distinctions exist because they
warrant **different operator responses**:

- `skipped_ownership_unproven` — an external participant is there. Understand it
  before proceeding.
- `skipped_changed_before_cleanup` — the candidate changed since measurement.
  Re-run.
- `skipped_permission_denied` / `skipped_io_error` / `skipped_never_authoritative` —
  an environment problem.
- `skipped_dryrun` — expected, in Simulate mode only.

Collapsing two of these into one generic `skipped` is a contract change that
degrades every consumer, not a simplification.

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
  explicitly scoped and recorded as the untested surface it is.
- `scripts/gen-release-workflow-shape.py --check` and the completions drift
  gate pass.

## Related

- [`test-evidence`](../test-evidence/SKILL.md) — proving parity rather than
  asserting it
