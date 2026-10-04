# cargo-cleanme JSON output schema, version 1

`--format json` emits exactly one UTF-8 JSON document followed by a newline. The top-level object contains `schema_version` (currently `1`), `cargo_cleanme_version`, `operation`, `scope`, optional cleanup `mode`, and `result`. Human prose and transient progress are not part of this contract. A backwards-incompatible field or meaning change requires a schema-version decision.

## Scan result

- `discovered_manifests`: manifests found in the requested scope.
- `groups`: deterministic physical output groups with `display_path`, `workspace_roots`, `physical_paths`, `bytes`, `size_metric`, optional `newest_activity_unix_seconds`, and `ownership`.
- `diagnostics`: severity, category, optional path, and explanatory message.
- `summary`: group count, inventory bytes, and diagnostic count. Inventory bytes describe observed output and are not recovered bytes.

## Cleanup result

- `discovered_manifests`, `resolved_workspaces`, and `units_considered` describe scope coverage.
- `selected_roots`, `effective_policy`, and optional `state_generation_last_full_at` identify the cleanup scope and policy inputs; learned-state generation is discovery metadata only.
- Optional `selector_kind` and `selector_value` identify a Cargo profile or package selector. A selector's `selector_estimate_bytes` is `null` unless a trustworthy estimate exists; `output_union_before_bytes` is only whole-output context and is never the selector's reclaimable estimate. `before_bytes` is `null` for selector cleanup.
- `scope_blocked`, `scope_reason`, and `unresolved_ownership` report incomplete ownership coverage.
- `unresolved_participants` retains each unresolved manifest, resolution stage, and diagnostic reason.
- `units` contains the workspace root, affected output roots, ownership class, optional stable `policy_disposition`, operation outcome, distinct before/after/decrease byte fields, and optional human detail.
- `reason_code` is a stable machine value. Current values include the policy dispositions, `previewed`, `simulated`, `cleaned`, `measurement_failed`, `cargo_failed`, `skipped_shared`, `skipped_uncertain`, `skipped_unauthorized`, and `skipped_safety`.
- `summary` reports previewed, simulated, cleaned, skipped, failed, and diagnostic counts.

Cargo profile selection is supported only for runtime Cargo versions 1.89 through 1.99, the qualified range recorded for M008C. Unknown/newer versions fail closed before `cargo clean`. Package selection remains a typed deferred result (`selector_unsupported`) because qualified Cargo versions remove shared dependency artifacts and older versions mishandle configured `build.target`. Any nonzero minimum-size policy with a selector is rejected because selector-specific bytes are unknown. Preview delegates `--dry-run` to Cargo; Simulate invokes no Cargo clean; Execute delegates deletion to Cargo after the unchanged complete-workspace proof. cargo-cleanme does not parse Cargo's private artifact layout.

Stable policy disposition values are `selected`, `below_minimum_size`, `too_recent_for_policy`, `not_included`, and `excluded`. Paths are UTF-8 strings when representable; otherwise the display string uses the platform escaped debug form and is not a reversible identity encoding.

`--stats` is stderr-only and does not change JSON stdout. Progress is disabled for JSON output. The process exits 0 for completed requests with safe per-unit skips, 1 for incomplete cleanup scope or operational cleanup failure, and 2 for fatal invocation/configuration errors. External schedulers may call `--yes`; cargo-cleanme itself has no scheduler or daemon.
