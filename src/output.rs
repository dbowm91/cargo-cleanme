//! Versioned, stable machine-output DTOs. Internal domain structs are projected
//! explicitly so implementation refactors do not silently change the CLI API.
use crate::{cleanup::CleanReport, domain::ScanReport};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct EnvelopeV1<T> {
    pub schema_version: u32,
    pub cargo_cleanme_version: &'static str,
    pub operation: &'static str,
    pub scope: String,
    pub mode: Option<String>,
    pub result: T,
}

#[derive(Serialize)]
pub struct ScanV1 {
    pub discovered_manifests: u64,
    pub groups: Vec<ScanGroupV1>,
    pub diagnostics: Vec<DiagnosticV1>,
    pub summary: ScanSummaryV1,
}
#[derive(Serialize)]
pub struct ScanGroupV1 {
    pub display_path: String,
    pub workspace_roots: Vec<String>,
    pub physical_paths: Vec<String>,
    pub bytes: u64,
    pub size_metric: &'static str,
    pub newest_activity_unix_seconds: Option<u64>,
    pub ownership: &'static str,
}
#[derive(Serialize)]
pub struct DiagnosticV1 {
    pub severity: String,
    pub category: String,
    pub path: Option<String>,
    pub message: String,
}
#[derive(Serialize)]
pub struct ScanSummaryV1 {
    pub group_count: usize,
    pub inventory_bytes: u64,
    pub diagnostic_count: usize,
}

#[derive(Serialize)]
pub struct CleanupV1 {
    pub discovered_manifests: usize,
    pub resolved_workspaces: usize,
    pub units_considered: usize,
    pub scope_blocked: bool,
    pub scope_reason: Option<String>,
    pub unresolved_ownership: usize,
    pub unresolved_participants: Vec<UnresolvedParticipantV1>,
    pub selected_roots: Vec<String>,
    pub effective_policy: Option<PolicyV1>,
    pub selector_kind: Option<&'static str>,
    pub selector_value: Option<String>,
    pub state_generation_last_full_at: Option<u64>,
    pub units: Vec<CleanupUnitV1>,
    pub summary: CleanupSummaryV1,
}
#[derive(Serialize)]
pub struct UnresolvedParticipantV1 {
    pub manifest: String,
    pub stage: &'static str,
    pub reason: String,
}
#[derive(Serialize)]
pub struct PolicyV1 {
    pub min_reclaimable_bytes: u64,
    pub min_inactive_seconds: Option<u64>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}
#[derive(Serialize)]
pub struct CleanupUnitV1 {
    pub workspace_root: String,
    pub output_roots: Vec<String>,
    pub ownership: &'static str,
    pub policy_disposition: Option<String>,
    pub outcome: String,
    pub reason_code: String,
    pub before_bytes: Option<u64>,
    pub selector_estimate_bytes: Option<u64>,
    pub output_union_before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub observed_decrease_bytes: Option<u64>,
    pub detail: String,
}
#[derive(Default, Serialize)]
pub struct CleanupSummaryV1 {
    pub previewed: usize,
    pub simulated: usize,
    pub cleaned: usize,
    pub skipped: usize,
    pub failed: usize,
    pub diagnostics: usize,
}

pub fn scan(report: &ScanReport, scope: impl Into<String>) -> EnvelopeV1<ScanV1> {
    let groups = report
        .groups
        .iter()
        .map(|g| ScanGroupV1 {
            display_path: path(&g.display_path),
            workspace_roots: g.workspace_roots.iter().map(|p| path(p)).collect(),
            physical_paths: g.physical_paths.iter().map(|p| path(p)).collect(),
            bytes: g.bytes,
            size_metric: match g.metric {
                crate::domain::SizeMetric::Allocated => "allocated",
                crate::domain::SizeMetric::Apparent => "apparent",
            },
            newest_activity_unix_seconds: g.newest_mtime.and_then(unix_seconds),
            ownership: g.ownership.label(),
        })
        .collect::<Vec<_>>();
    let diagnostics = report
        .diagnostics
        .iter()
        .map(|d| DiagnosticV1 {
            severity: d.severity.as_str().into(),
            category: match d.category {
                crate::domain::DiagnosticCategory::PermissionDenied => "permission_denied",
                crate::domain::DiagnosticCategory::Vanished => "vanished",
                crate::domain::DiagnosticCategory::Metadata => "metadata",
                crate::domain::DiagnosticCategory::InvalidEntry => "invalid_entry",
                crate::domain::DiagnosticCategory::PlatformRoot => "platform_root",
                crate::domain::DiagnosticCategory::CandidateUncertain => "candidate_uncertain",
            }
            .into(),
            path: d.path.as_deref().map(path),
            message: d.message.clone(),
        })
        .collect::<Vec<_>>();
    let total = groups.iter().fold(0u64, |a, g| a.saturating_add(g.bytes));
    EnvelopeV1 {
        schema_version: 1,
        cargo_cleanme_version: env!("CARGO_PKG_VERSION"),
        operation: "scan",
        scope: scope.into(),
        mode: None,
        result: ScanV1 {
            discovered_manifests: report.discovered,
            summary: ScanSummaryV1 {
                group_count: groups.len(),
                inventory_bytes: total,
                diagnostic_count: diagnostics.len(),
            },
            groups,
            diagnostics,
        },
    }
}

pub fn cleanup(
    report: &CleanReport,
    scope: impl Into<String>,
    state_generation: Option<u64>,
) -> EnvelopeV1<CleanupV1> {
    use crate::cleanup::CleanOutcome;
    let units = report
        .results
        .iter()
        .map(|r| CleanupUnitV1 {
            workspace_root: path(&r.display_path),
            output_roots: r.output_roots.iter().map(|p| path(p)).collect(),
            ownership: r.ownership.label(),
            policy_disposition: r.policy_disposition.map(policy_code),
            outcome: r.outcome.as_str().into(),
            reason_code: r.reason_code.as_str().into(),
            before_bytes: if report.selector.is_some() {
                None
            } else {
                r.before_bytes
            },
            selector_estimate_bytes: if report.selector.is_some() {
                None
            } else {
                r.before_bytes
            },
            output_union_before_bytes: r.before_bytes,
            after_bytes: r.after_bytes,
            observed_decrease_bytes: r.observed_decrease,
            detail: r.detail.clone(),
        })
        .collect::<Vec<_>>();
    let mut summary = CleanupSummaryV1 {
        diagnostics: report.diagnostics,
        ..Default::default()
    };
    for result in &report.results {
        let slot = match result.outcome {
            CleanOutcome::Previewed => &mut summary.previewed,
            CleanOutcome::Simulated => &mut summary.simulated,
            CleanOutcome::Cleaned => &mut summary.cleaned,
            CleanOutcome::Skipped => &mut summary.skipped,
            CleanOutcome::Failed => &mut summary.failed,
        };
        *slot += 1;
    }
    let mode = report.mode.as_str();
    EnvelopeV1 {
        schema_version: 1,
        cargo_cleanme_version: env!("CARGO_PKG_VERSION"),
        operation: "clean",
        scope: scope.into(),
        mode: Some(mode.into()),
        result: CleanupV1 {
            discovered_manifests: report.discovered_manifests,
            resolved_workspaces: report.resolved_workspaces,
            units_considered: report.units_considered,
            scope_blocked: report.scope_blocked.is_some(),
            scope_reason: report.scope_blocked.as_ref().map(|b| b.message.clone()),
            unresolved_ownership: report.unresolved_ownership.len(),
            unresolved_participants: report
                .unresolved_ownership
                .iter()
                .map(|p| UnresolvedParticipantV1 {
                    manifest: path(&p.manifest),
                    stage: p.stage,
                    reason: p.reason.clone(),
                })
                .collect(),
            selected_roots: report.selected_roots.iter().map(|p| path(p)).collect(),
            effective_policy: report.effective_policy.as_ref().map(|p| PolicyV1 {
                min_reclaimable_bytes: p.min_reclaimable_bytes,
                min_inactive_seconds: p.min_inactive_seconds,
                include: p.include.clone(),
                exclude: p.exclude.clone(),
            }),
            selector_kind: report.selector.as_ref().map(|s| s.kind()),
            selector_value: report.selector.as_ref().map(|s| s.value().to_owned()),
            state_generation_last_full_at: state_generation,
            units,
            summary,
        },
    }
}

fn policy_code(code: crate::cleanup::PolicyDisposition) -> String {
    use crate::cleanup::PolicyDisposition::*;
    match code {
        Selected => "selected",
        BelowMinimumSize => "below_minimum_size",
        TooRecentForPolicy => "too_recent_for_policy",
        NotIncluded => "not_included",
        Excluded => "excluded",
        SelectorEstimateUnavailable => "selector_estimate_unavailable",
    }
    .into()
}

fn path(p: &Path) -> String {
    p.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{:?}", p.as_os_str()))
}
fn unix_seconds(t: std::time::SystemTime) -> Option<u64> {
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Bounded single-line summaries for unattended schedulers.
///
/// This is **not** a third machine contract. JSON remains the complete,
/// versioned one; this module exists because a retained scheduler-history tail
/// is 512 bytes and a JSON envelope does not fit in it. Everything here is
/// projected from report state that already exists — no walk, no Cargo, no
/// state re-read, no selector arithmetic.
pub mod log {
    use crate::cleanup::{CleanMode, CleanOutcome, CleanReport, ScopeBlock};
    use crate::domain::ScanReport;
    use std::fmt::Write as _;

    /// Hard cap on the whole line, excluding the terminating newline.
    ///
    /// Deliberately below a 512-byte published escaped tail: JSON escaping is
    /// skipped here because the output is ASCII by contract, so the raw bytes
    /// and the escaped bytes are the same size.
    pub const MAX_BYTES: usize = 384;

    const PREFIX: &str = "cargo-cleanme";

    /// One scan summary line.
    ///
    /// `full_incomplete` is computed by the caller from the typed diagnostic
    /// set rather than re-derived here, so this function cannot change a scan's
    /// meaning by disagreeing with the exit code it accompanies.
    pub fn scan(report: &ScanReport, scope: &str, full_incomplete: bool) -> String {
        let total: u64 = report
            .groups
            .iter()
            .fold(0u64, |a, g| a.saturating_add(g.bytes));
        let status = if full_incomplete { "failed" } else { "ok" };
        line(
            &[
                ("op", "scan".to_owned()),
                ("status", status.to_owned()),
                ("scope", scope.to_owned()),
            ],
            &[
                ("manifests", report.discovered.to_string()),
                ("groups", report.groups.len().to_string()),
                ("bytes", total.to_string()),
                ("diagnostics", report.diagnostics.len().to_string()),
            ],
        )
    }

    /// One cleanup summary line.
    ///
    /// `reclaimed_bytes` is the sum of *observed* decreases and is emitted only
    /// for Execute. A Simulate or Cargo-preview run that reported a
    /// reclaimed-bytes figure would be claiming bytes it did not recover.
    pub fn cleanup(report: &CleanReport, scope: &str) -> String {
        let block: Option<&ScopeBlock> = report.scope_blocked.as_ref();
        let failed = report
            .results
            .iter()
            .filter(|r| r.outcome == CleanOutcome::Failed)
            .count();
        let skipped = report
            .results
            .iter()
            .filter(|r| r.outcome == CleanOutcome::Skipped)
            .count();
        let executed = report
            .results
            .iter()
            .filter(|r| r.outcome == CleanOutcome::Cleaned)
            .count();
        let status = if block.is_some() {
            "blocked"
        } else if failed > 0 || report.failed > 0 {
            "failed"
        } else {
            "ok"
        };
        let reclaimed: u64 = report.results.iter().fold(0u64, |a, r| {
            a.saturating_add(r.observed_decrease.unwrap_or(0))
        });

        // Ordered most-valuable-first, because the bound drops from the tail:
        // a typed code an operator can alert on must survive a crowded line, a
        // byte count need not.
        let mut optional: Vec<(&str, String)> = Vec::new();
        if let Some(block) = block {
            optional.push(("reason", block.reason.as_str().to_owned()));
        }
        optional.push(("cleaned", executed.to_string()));
        optional.push(("skipped", skipped.to_string()));
        optional.push(("failed", failed.to_string()));
        // Only an executed cleanup measured a decrease. A blocked scope cleaned
        // nothing, and a Simulate or Cargo-preview run recovered nothing, so
        // neither may print a reclaimed-bytes figure at all.
        if report.mode == CleanMode::Execute && block.is_none() {
            optional.push(("reclaimed_bytes", reclaimed.to_string()));
        }
        optional.push((
            "diagnostics",
            (report.diagnostics + report.unresolved_ownership.len()).to_string(),
        ));

        line(
            &[
                ("op", "clean".to_owned()),
                ("status", status.to_owned()),
                ("scope", scope.to_owned()),
                ("mode", report.mode.as_str().to_owned()),
            ],
            &optional,
        )
    }

    /// One line for a failure raised before any report existed.
    ///
    /// Carries a typed code and nothing else. Paths, Cargo's stderr, and
    /// config text are deliberately absent: this line goes into a retained
    /// history pane where a path is both noise and a small disclosure.
    pub fn fatal(op: &str, reason: &str) -> String {
        line(
            &[("op", op.to_owned()), ("status", "error".to_owned())],
            &[("reason", reason.to_owned())],
        )
    }

    /// One line for a `config` subcommand.
    ///
    /// `config path` and `config show` exist precisely to put a path and the
    /// file's text on stdout, and in log mode that content is exactly what this
    /// format refuses to retain: a path in a history pane, and prose where a
    /// monitor expects `key=value`. The line therefore reports which action ran
    /// and nothing about what it produced.
    pub fn config(action: &str) -> String {
        line(
            &[("op", "config".to_owned()), ("status", "ok".to_owned())],
            &[("action", action.to_owned())],
        )
    }

    /// One line for an `update` run.
    ///
    /// `target` is a target triple, not a path, and the versions are the
    /// registry's own strings, so none of this is what the format refuses. The
    /// `mode` field uses the cleanup vocabulary — `simulate` for a resolved plan
    /// that acquired nothing, `execute` for a replacement that committed.
    pub fn update(from: &str, to: &str, target: &str, dry_run: bool) -> String {
        line(
            &[("op", "update".to_owned()), ("status", "ok".to_owned())],
            &[
                ("from", from.to_owned()),
                ("to", to.to_owned()),
                ("target", target.to_owned()),
                (
                    "mode",
                    if dry_run { "simulate" } else { "execute" }.to_owned(),
                ),
            ],
        )
    }

    /// Join `key=value` pairs under the fixed prefix.
    ///
    /// Structural fields are emitted unconditionally and in the order given.
    /// Optional fields are appended while the result fits, and **dropped whole**
    /// once it does not: nothing is ever truncated, so the line cannot cut a
    /// value in half and ASCII integrity is free rather than checked.
    pub fn line(structural: &[(&str, String)], optional: &[(&str, String)]) -> String {
        debug_assert!(
            structural
                .iter()
                .all(|(k, v)| matches!(*k, "op" | "status" | "scope" | "mode")
                    && !v.is_empty()
                    && !v.contains(char::is_whitespace)),
            "structural fields are a fixed, small set of single-token values"
        );
        let mut out = String::with_capacity(MAX_BYTES);
        out.push_str(PREFIX);
        let push = |out: &mut String, key: &str, value: &str| {
            let _ = write!(out, " {key}={value}");
        };
        for (key, value) in structural {
            push(&mut out, key, value);
        }
        for (key, value) in optional {
            // " key=value" plus the byte already spent.
            let cost = 1 + key.len() + 1 + value.len();
            if out.len() + cost > MAX_BYTES {
                break;
            }
            push(&mut out, key, value);
        }
        debug_assert!(out.len() <= MAX_BYTES, "log line exceeded its bound");
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::cleanup::{CleanReport, CleanupReasonCode, ScopeBlock, ScopeBlockReason};
        use crate::domain::{
            DiagnosticCategory, DiagnosticSeverity, EligibleOutputGroup, OutputOwnershipClass,
            ScanCounters, ScanDiagnostic, SizeMetric,
        };
        use std::path::PathBuf;

        fn empty_report() -> CleanReport {
            CleanReport {
                results: Vec::new(),
                diagnostics: 0,
                failed: 0,
                mode: CleanMode::Execute,
                counters: ScanCounters::default(),
                scope_blocked: None,
                unresolved_ownership: Vec::new(),
                selected_roots: Vec::new(),
                discovered_manifests: 0,
                resolved_workspaces: 0,
                units_considered: 0,
                effective_policy: None,
                selector: None,
            }
        }

        fn scan_report(groups: usize, bytes: u64, diagnostics: usize) -> ScanReport {
            ScanReport {
                groups: (0..groups)
                    .map(|i| EligibleOutputGroup {
                        display_path: PathBuf::from(format!("/tmp/group{i}")),
                        workspace_roots: vec![PathBuf::from("/ws")],
                        physical_paths: vec![PathBuf::from(format!("/tmp/group{i}"))],
                        bytes,
                        metric: SizeMetric::Allocated,
                        newest_mtime: None,
                        artifact_entries: 1,
                        ownership: OutputOwnershipClass::PrivateBounded,
                    })
                    .collect(),
                diagnostics: (0..diagnostics)
                    .map(|i| ScanDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        category: DiagnosticCategory::PermissionDenied,
                        path: None,
                        message: format!("diagnostic {i}"),
                    })
                    .collect(),
                discovered: 184,
                visited_entries: 900,
                counters: ScanCounters::default(),
            }
        }

        fn assert_ascii_and_bounded(line: &str) {
            assert!(line.is_ascii(), "log output is ASCII by contract: {line}");
            assert!(
                line.len() <= MAX_BYTES,
                "{} bytes exceeds the {MAX_BYTES}-byte bound: {line}",
                line.len()
            );
            assert!(!line.contains('"'), "values need no quoting: {line}");
            assert!(!line.contains('\n'), "one line, one newline: {line}");
            assert!(
                line.starts_with("cargo-cleanme "),
                "the prefix identifies the tool: {line}"
            );
        }

        #[test]
        fn scan_line_is_one_bounded_ascii_line() {
            let line = super::scan(&scan_report(3, 300, 2), "full", false);
            assert_ascii_and_bounded(&line);
            assert_eq!(
                line,
                "cargo-cleanme op=scan status=ok scope=full manifests=184 groups=3 bytes=900 diagnostics=2"
            );
        }

        #[test]
        fn scan_line_reports_failed_for_an_incomplete_full_scan() {
            let line = super::scan(&scan_report(3, 300, 1), "full", true);
            assert!(line.contains("status=failed"), "{line}");
            // Scope is still stated: an incomplete run is not a different scope.
            assert!(line.contains("scope=full"), "{line}");
            assert_ascii_and_bounded(&line);
        }

        #[test]
        fn cleanup_line_reports_the_canonical_ok_shape() {
            let mut report = empty_report();
            report.discovered_manifests = 12;
            let line = super::cleanup(&report, "routine");
            assert_eq!(
                line,
                "cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=0 skipped=0 failed=0 reclaimed_bytes=0 diagnostics=0"
            );
            assert_ascii_and_bounded(&line);
        }

        #[test]
        fn cleanup_line_omits_reclaimed_bytes_for_simulate_and_preview() {
            let mut report = empty_report();
            report.mode = CleanMode::Simulate;
            let simulated = super::cleanup(&report, "routine");
            assert!(simulated.contains("mode=simulate"), "{simulated}");
            assert!(
                !simulated.contains("reclaimed_bytes"),
                "simulation must not claim recovered bytes: {simulated}"
            );
            report.mode = CleanMode::CargoPreview;
            let preview = super::cleanup(&report, "explicit");
            assert!(preview.contains("mode=preview"), "{preview}");
            assert!(!preview.contains("reclaimed_bytes"), "{preview}");
            assert_ascii_and_bounded(&simulated);
            assert_ascii_and_bounded(&preview);
        }

        #[test]
        fn cleanup_line_reports_a_typed_reason_when_blocked() {
            let mut report = empty_report();
            report.scope_blocked = Some(ScopeBlock::new(
                ScopeBlockReason::OwnershipUnproven,
                "cleanup ownership could not be proven: 1 discovered Cargo manifest did not resolve; no cleanup commands were run".into(),
            ));
            report.unresolved_ownership = vec![crate::domain::UnresolvedOwnershipParticipant {
                manifest: PathBuf::from("/x/Cargo.toml"),
                stage: "locate",
                reason: "cargo locate-project failed".into(),
            }];
            let line = super::cleanup(&report, "routine");
            assert!(line.contains("status=blocked"), "{line}");
            assert!(
                line.contains("reason=ownership_unproven"),
                "the typed code, never the English prose: {line}"
            );
            assert!(
                !line.contains("ownership could not be"),
                "prose must never reach a retained history line: {line}"
            );
            assert_ascii_and_bounded(&line);
        }

        #[test]
        fn cleanup_line_reports_failed_without_a_scope_reason() {
            let mut report = empty_report();
            report.failed = 1;
            report.results.push(crate::cleanup::CleanResult {
                display_path: PathBuf::from("/ws"),
                workspace_roots: vec![PathBuf::from("/ws")],
                output_roots: vec![PathBuf::from("/ws/target")],
                ownership: OutputOwnershipClass::PrivateBounded,
                outcome: CleanOutcome::Failed,
                reason_code: CleanupReasonCode::CargoFailed,
                detail: "cargo clean exited with status 1".into(),
                policy_disposition: None,
                before_bytes: Some(1),
                after_bytes: None,
                observed_decrease: None,
            });
            let line = super::cleanup(&report, "explicit");
            assert!(line.contains("status=failed"), "{line}");
            assert!(line.contains("failed=1"), "{line}");
            assert_ascii_and_bounded(&line);
        }

        #[test]
        fn a_blocked_scope_never_reports_reclaimed_bytes() {
            let mut report = empty_report();
            report.scope_blocked = Some(ScopeBlock::new(
                ScopeBlockReason::IncompleteDiscovery,
                "combined cleanup ownership universe is incomplete: 3 discovery diagnostic(s); no cleanup commands were run".into(),
            ));
            let line = super::cleanup(&report, "routine");
            assert!(line.contains("reason=incomplete_discovery"), "{line}");
            assert!(
                !line.contains("reclaimed_bytes=0"),
                "a blocked run cleaned nothing; claiming zero bytes as a measurement is noise: {line}"
            );
        }

        /// Premise-negative: the bound is enforced by dropping a field whole,
        /// never by cutting a line. A reason source far longer than the cap must
        /// lose the optional field and keep every structural one.
        #[test]
        fn an_overlong_optional_value_is_dropped_whole_and_the_head_survives() {
            // A distinctive character, so the assertion below cannot be
            // satisfied by an incidental letter in the structural head.
            let flood = "Q".repeat(4096);
            let line = super::line(
                &[
                    ("op", "clean".to_owned()),
                    ("status", "blocked".to_owned()),
                    ("scope", "routine".to_owned()),
                    ("mode", "execute".to_owned()),
                ],
                &[("reason", flood.clone()), ("cleaned", "7".to_owned())],
            );
            assert_ascii_and_bounded(&line);
            assert!(
                line.contains("status=blocked") && line.contains("mode=execute"),
                "structural fields are never truncated: {line}"
            );
            assert!(
                !line.contains('Q'),
                "the overlong value is omitted entirely, not truncated: {line}"
            );
            assert!(
                !line.contains("cleaned=7"),
                "optional fields after the one that did not fit are dropped too"
            );
        }

        /// Premise-negative: the structural head must fit on its own. u64::MAX
        /// in every numeric field is the worst case the crate can produce.
        #[test]
        fn the_structural_head_fits_at_the_worst_case() {
            let line = super::line(
                &[
                    ("op", "clean".to_owned()),
                    ("status", "failed".to_owned()),
                    ("scope", "explicit".to_owned()),
                    ("mode", "cargo-preview-is-not-a-mode-value".to_owned()),
                ],
                &[("cleaned", u64::MAX.to_string())],
            );
            assert_ascii_and_bounded(&line);
        }

        #[test]
        fn field_order_is_deterministic_across_calls() {
            let first = super::cleanup(&empty_report(), "routine");
            let second = super::cleanup(&empty_report(), "routine");
            assert_eq!(first, second);
        }

        #[test]
        fn fatal_line_is_one_bounded_line_with_a_typed_reason() {
            let line = super::fatal("clean", "config");
            assert_eq!(line, "cargo-cleanme op=clean status=error reason=config");
            assert_ascii_and_bounded(&line);
        }
    }
}

// ------------------------------------------------------- the update document

/// One stable JSON object for `update`, and exactly the string production
/// prints.
///
/// Deliberately **not** an `EnvelopeV1`. An update resolves no cleanup scope
/// and runs no cleanup mode, so an envelope would carry two fields that are
/// always `null` while hiding the fact that `result` is a different projection.
/// Its shape is schema_version 1, and this document is the only place it is
/// written: the binary calls it, and the contract tests call it, so a test can
/// never agree with production by sharing a second serializer.
///
/// The body is spelled as a literal object map rather than a derived struct for
/// the same reason the rest of this module is hand-projected: field presence
/// and field names are the published contract, and a `#[derive(Serialize)]`
/// change to an unrelated struct must not be able to move them.
pub fn update_json(plan: &crate::update::UpdatePlan, dry_run: bool) -> String {
    let mut document = serde_json::Map::new();
    document.insert("schema_version".into(), serde_json::json!(1));
    document.insert(
        "cargo_cleanme_version".into(),
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    document.insert("operation".into(), serde_json::json!("update"));
    document.insert("dry_run".into(), serde_json::json!(dry_run));
    document.insert(
        "result".into(),
        serde_json::json!({
            "from_version": plan.from_version,
            "to_version": plan.to_version,
            "target": plan.target,
            "asset": plan.asset,
            "tag": plan.tag,
            // The domain's stable code, never the human remediation prose: a
            // refusal reason has to be greppable and it has to be the same
            // token the reason codes already use.
            "provenance": plan.provenance.code(),
            // `changed` is derived from the requested mode, never from what the
            // transaction happened to do. A dry run that reported `changed`
            // would be claiming bytes it never touched.
            "changed": !dry_run,
        }),
    );
    serde_json::Value::Object(document).to_string()
}

/// The exact bytes the `update` JSON path writes to stdout.
///
/// `run_update` prints this with `print!` rather than composing the trailing
/// newline itself, so the document-plus-newline stream contract is a value that
/// tests can hold and compare byte for byte instead of a convention that only
/// the `println!` macro's behaviour guarantees. One document, one newline, and
/// no room for a human status line to be mixed in: a JSON consumer reads this
/// stream with a parser, not with a grep.
pub fn update_json_stream(plan: &crate::update::UpdatePlan, dry_run: bool) -> String {
    format!("{}\n", update_json(plan, dry_run))
}

#[cfg(test)]
mod update_tests {
    use super::update_json_stream;
    use crate::update::{Provenance, UpdatePlan};
    use std::path::PathBuf;

    /// A plan with every identity field distinct, so a test that asserts "the
    /// document equals its input" cannot pass by swapping two fields.
    fn plan() -> UpdatePlan {
        UpdatePlan {
            from_version: "0.1.6".to_owned(),
            to_version: "0.2.0".to_owned(),
            target: "x86_64-unknown-linux-gnu".to_owned(),
            asset: "cargo-cleanme-x86_64-unknown-linux-gnu".to_owned(),
            tag: "v0.2.0".to_owned(),
            provenance: Provenance::VerifiableSelfManaged { digest: [7u8; 32] },
        }
    }

    fn parse(stream: &str) -> serde_json::Value {
        serde_json::from_str(stream.trim_end_matches('\n')).expect("the document is one JSON value")
    }

    #[test]
    fn the_dry_run_document_pins_the_schema_and_the_identity_fields() {
        let document = parse(&update_json_stream(&plan(), true));
        assert_eq!(document["schema_version"], serde_json::json!(1));
        assert_eq!(document["operation"], serde_json::json!("update"));
        assert_eq!(
            document["cargo_cleanme_version"],
            serde_json::json!(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(document["dry_run"], serde_json::json!(true));
        assert_eq!(document["result"]["changed"], serde_json::json!(false));
        let result = &document["result"];
        assert_eq!(result["from_version"], serde_json::json!("0.1.6"));
        assert_eq!(result["to_version"], serde_json::json!("0.2.0"));
        assert_eq!(
            result["target"],
            serde_json::json!("x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            result["asset"],
            serde_json::json!("cargo-cleanme-x86_64-unknown-linux-gnu")
        );
        assert_eq!(result["tag"], serde_json::json!("v0.2.0"));
        assert_eq!(result["provenance"], serde_json::json!("self_managed"));
    }

    #[test]
    fn the_mutating_document_is_the_same_shape_with_changed_true() {
        let dry = parse(&update_json_stream(&plan(), true));
        let mutating = parse(&update_json_stream(&plan(), false));
        assert_eq!(mutating["dry_run"], serde_json::json!(false));
        assert_eq!(mutating["result"]["changed"], serde_json::json!(true));
        // `changed` and `dry_run` are the only two fields allowed to differ, so
        // that is asserted as a fact rather than re-listed field by field.
        let mut dry = dry;
        dry["dry_run"] = serde_json::json!(false);
        dry["result"]["changed"] = serde_json::json!(true);
        assert_eq!(mutating, dry);
    }

    #[test]
    fn every_public_provenance_code_is_emitted_as_its_own_stable_token() {
        let cases = [
            (
                Provenance::CargoManaged {
                    bin_root: PathBuf::from("/cargo/bin"),
                    version: "0.1.6".into(),
                },
                "cargo_managed",
            ),
            (
                Provenance::VerifiableSelfManaged { digest: [0u8; 32] },
                "self_managed",
            ),
            (
                Provenance::CargoInstallOnlyHost {
                    triple_hint: "armv7-unknown-linux-gnueabihf".into(),
                },
                "cargo_install_only_host",
            ),
            (
                Provenance::UnprovableOwnership {
                    detail: "no digest matched".into(),
                },
                "unprovable_ownership",
            ),
        ];
        for (provenance, code) in cases {
            let mut plan = plan();
            plan.provenance = provenance;
            let document = parse(&update_json_stream(&plan, true));
            assert_eq!(
                document["result"]["provenance"],
                serde_json::json!(code),
                "the document must carry the domain code, not prose"
            );
        }
    }

    #[test]
    fn the_stream_is_one_document_plus_exactly_one_newline() {
        let stream = update_json_stream(&plan(), true);
        assert!(
            stream.ends_with('\n'),
            "the document must be newline-terminated"
        );
        assert_eq!(
            stream.matches('\n').count(),
            1,
            "a JSON document has no interior raw newline, so a second one is a second line"
        );
        // `serde_json` rejects trailing content after a value, so this is the
        // assertion that the stream is one document and not one document plus a
        // status line the parser would have to skip.
        serde_json::from_str::<serde_json::Value>(&stream)
            .expect("the whole stream, newline included, must parse as exactly one document");
    }
}
