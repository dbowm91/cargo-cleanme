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
            scope_reason: report.scope_blocked.clone(),
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
