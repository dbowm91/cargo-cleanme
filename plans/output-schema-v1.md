# cargo-cleanme JSON output schema, version 1

`--format json` emits exactly one UTF-8 JSON document followed by a newline. The top-level object contains `schema_version` (currently `1`), `cargo_cleanme_version`, `operation`, `scope`, optional cleanup `mode`, and `result`. Human prose and transient progress are not part of this contract. A backwards-incompatible field or meaning change requires a schema-version decision.

## Scan result

- `discovered_manifests`: manifests found in the requested scope.
- `groups`: deterministic physical output groups with `display_path`, `workspace_roots`, `physical_paths`, `bytes`, `size_metric`, optional `newest_activity_unix_seconds`, and `ownership`.
- `diagnostics`: severity, category, optional path, and explanatory message.
- `summary`: group count, inventory bytes, and diagnostic count. Inventory bytes describe observed output and are not recovered bytes.

## Cleanup result

- `discovered_manifests`, `resolved_workspaces`, and `units_considered` describe scope coverage.
- `scope_blocked`, `scope_reason`, and `unresolved_ownership` report incomplete ownership coverage.
- `units` contains the workspace root, affected output roots, ownership class, optional stable `policy_disposition`, operation outcome, distinct before/after/decrease byte fields, and optional human detail.
- `reason_code` is a stable machine value. Current values include the policy dispositions, `previewed`, `simulated`, `cleaned`, `measurement_failed`, `cargo_failed`, `skipped_shared`, `skipped_uncertain`, `skipped_unauthorized`, and `skipped_safety`.
- `summary` reports previewed, simulated, cleaned, skipped, failed, and diagnostic counts.

Stable policy disposition values are `selected`, `below_minimum_size`, `too_recent_for_policy`, `not_included`, and `excluded`. Paths are UTF-8 strings when representable; otherwise the display string uses the platform escaped debug form and is not a reversible identity encoding.

`--stats` is stderr-only and does not change JSON stdout. Progress is disabled for JSON output. The process exits 0 for completed requests with safe per-unit skips, 1 for incomplete cleanup scope or operational cleanup failure, and 2 for fatal invocation/configuration errors. External schedulers may call `--yes`; cargo-cleanme itself has no scheduler or daemon.
