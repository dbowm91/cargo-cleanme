# cargo-cleanme JSON output schema, version 1

`--format json` emits exactly one UTF-8 JSON document followed by a newline. The top-level object contains `schema_version` (currently `1`), `cargo_cleanme_version`, `operation`, `scope`, `mode`, and `result`. `mode` is always present and is `null` for a scan. Human prose and transient progress are not part of this contract. A backwards-incompatible field or meaning change requires a schema-version decision.

## `operation`, `scope`, and `mode`

- `operation` is `scan`, `clean`, `config`, or `update`. Bare `cargo cleanme` emits
  `clean`; there is no separate `maintenance` operation, because a consumer that
  had to special-case the front door could not trust the front door.
- `scope` describes **the scope that was resolved, not the flags that were
  passed**, for both operations. It is one of `full` (the full platform roots),
  `explicit` (one bounded root supplied on the command line, or a configured
  `scan.root` override), or `routine` (seed roots plus active learned roots).
- `mode` is `execute`, `simulate`, or `preview` for cleanup. `execute` is the
  default. `simulate` is `--dry-run` and spawns no `cargo clean` process at all.
  `preview` is `--cargo-preview`: the same complete proof, followed by Cargo's
  own `clean --dry-run --verbose`.

M012A clarification, schema_version unchanged at 1: cleanup `scope` previously
emitted `known` for the `--known` spelling. That was a description of argv, not
of the resolved scope, and it is now `routine` or `explicit` according to what
`policy::resolve` actually returned — the same vocabulary cleanup already used
for `full` and `explicit`. No field changed meaning and no field was added or
removed, so this is a value correction inside v1.

Where this document and the implementation disagree, **the implementation is authoritative** and this document is corrected to match it. A consumer may treat this file as descriptive rather than normative.

The `update` operation reuses `schema_version: 1` without the `EnvelopeV1` shape: it has no `scope`, and its `result` differs. A consumer must not treat `schema_version == 1` as implying the envelope described here.

## Scan result

- `discovered_manifests`: manifests found in the requested scope.
- `groups`: deterministic physical output groups with `display_path`, `workspace_roots`, `physical_paths`, `bytes`, `size_metric`, optional `newest_activity_unix_seconds`, and `ownership`.
- `diagnostics`: severity, category, optional path, and explanatory message.
- `summary`: group count, inventory bytes, and diagnostic count. Inventory bytes describe observed output and are not recovered bytes.

## Cleanup result

- `discovered_manifests`, `resolved_workspaces`, and `units_considered` describe scope coverage.
- `selected_roots` and optional `state_generation_last_full_at` identify the cleanup scope; learned-state generation is discovery metadata only.
- `effective_policy` carries the four effective selection inputs: `min_reclaimable_bytes`, optional `min_inactive_seconds`, `include`, and `exclude`.
- Optional `selector_kind` and `selector_value` identify a Cargo profile or package selector. `before_bytes` is `null` for selector cleanup. `output_union_before_bytes` is only whole-output context and is never the selector's reclaimable estimate.
- `selector_estimate_bytes` is `null` **exactly when a selector is active**, because selector-specific bytes are unknown. On a cleanup with **no** selector it carries the whole-output `before_bytes`. Treat its absence as ambiguous between "a selector was used" and "no trustworthy estimate was available"; do not read its presence as selector-specific bytes.
- `scope_blocked`, `scope_reason`, and `unresolved_ownership` report incomplete ownership coverage.
- `unresolved_participants` retains each unresolved manifest, resolution stage, and diagnostic reason.
- `units` contains the workspace root, affected output roots, ownership class, optional stable `policy_disposition`, `outcome`, distinct before/after/decrease byte fields, and a human `detail` string that is always present (possibly empty), not optional.
- `outcome` is one of `previewed`, `simulated`, `cleaned`, `skipped`, or `failed`.
- `reason_code` is a stable machine value. Current values are `previewed`, `simulated`, `cleaned`, `below_minimum_size`, `too_recent_for_policy`, `not_included`, `excluded`, `skipped_active`, `skipped_shared`, `skipped_uncertain`, `skipped_unauthorized`, `skipped_marker_invalid`, `skipped_changed_before_cleanup`, `skipped_ownership_unproven`, `skipped_safety`, `cargo_failed`, `measurement_failed`, `selector_unsupported`, and `selector_invalid`. `skipped_ownership_unproven` means ownership could not be re-proven at all, which is distinct from `skipped_changed_before_cleanup`, where a change was demonstrated; the two warrant different operator responses.
- `summary` reports previewed, simulated, cleaned, skipped, failed, and diagnostic counts. The `failed` count is tallied from the per-unit `outcome` values in this document and is expected to agree with a non-zero process exit code, but that agreement is a property of the current implementation rather than an enforced invariant.

Cargo profile selection is supported only for the exact qualified releases 1.89.0, 1.90.0, 1.91.1, 1.92.0, 1.93.1, 1.94.1, 1.95.0, 1.98.1, and 1.99.0. Package selection is supported only for 1.98.1 and 1.99.0, where configured and explicit target cleanup previews agreed. Unknown/unqualified versions fail closed before `cargo clean`. Package specs must uniquely identify a resolved workspace package; duplicate package names fail closed. Cargo may remove shared dependency outputs. Any nonzero minimum-size policy with a selector is rejected because selector-specific bytes are unknown. Preview delegates `--dry-run` to Cargo; Simulate invokes no Cargo clean; Execute delegates deletion to Cargo after the unchanged complete-workspace proof. cargo-cleanme does not parse Cargo's private artifact layout.

Stable policy disposition values are `selected`, `below_minimum_size`, `too_recent_for_policy`, `not_included`, `excluded`, and `selector_estimate_unavailable`. Paths are UTF-8 strings when representable; otherwise the display string uses the platform escaped debug form and is not a reversible identity encoding.

`--format log` is **not** part of this contract. It emits one bounded ASCII
summary line for an unattended history pane and drops every per-unit field.
It shares no serialization code path with this document, adds no field here,
and never changes `schema_version`. Anything that needs complete data must use
`--format json`; see `docs/AUTOMATION.md`.

`--stats` is stderr-only and does not change JSON stdout. Progress is disabled for JSON output. The process exits 0 for completed requests with safe per-unit skips, 1 for incomplete cleanup scope or operational cleanup failure, and 2 for fatal invocation/configuration errors. A scan additionally exits 1 when a Full scan hit an unreadable platform root. A failure to persist discovery state is reported on stderr but does not change the exit code, because that state is an optimization only. External schedulers may call bare `cargo cleanme` (Execute) or `cargo cleanme --dry-run` (Simulate); cargo-cleanme itself has no scheduler or daemon.
