use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanRequest {
    pub cli_root: Option<PathBuf>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanScope {
    Explicit(PathBuf),
    Global(Vec<PathBuf>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryFilters {
    Active {
        ignore: Vec<String>,
        unignore: Vec<PathBuf>,
    },
    Bypassed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveScanPolicy {
    pub recency: Duration,
    pub scope: ScanScope,
    pub discovery_filters: DiscoveryFilters,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredProject {
    pub project_root: PathBuf,
    pub manifest_path: PathBuf,
    pub target_path: PathBuf,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityState {
    Active,
    Inactive,
    Uncertain,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SizeMetric {
    Allocated,
    Apparent,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactAnalysis {
    pub target_path: PathBuf,
    pub bytes: u64,
    pub metric: SizeMetric,
    pub newest_mtime: Option<SystemTime>,
    pub artifact_entries: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleProject {
    pub project_root: PathBuf,
    pub artifact: ArtifactAnalysis,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCategory {
    PermissionDenied,
    Vanished,
    Metadata,
    InvalidEntry,
    PlatformRoot,
    CandidateUncertain,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanDiagnostic {
    pub severity: DiagnosticSeverity,
    pub category: DiagnosticCategory,
    pub path: Option<PathBuf>,
    pub message: String,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanReport {
    pub eligible: Vec<EligibleProject>,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub discovered: u64,
    pub visited_entries: u64,
}
