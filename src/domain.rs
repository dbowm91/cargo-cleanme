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
    pub groups: Vec<EligibleOutputGroup>,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub discovered: u64,
    pub visited_entries: u64,
    pub counters: ScanCounters,
}

/// Stable workspace identity: canonical workspace root directory.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct WorkspaceId(pub PathBuf);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMember {
    pub manifest_path: PathBuf,
    pub source_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CargoBuildDirCapability {
    /// Pre-1.91 metadata without `build_directory` and no env evidence.
    Unavailable,
    /// Modern Cargo reports equal target/build.
    Equal,
    /// Modern Cargo reports distinct target/build.
    Distinct,
    /// Malformed, unknown, or env-only evidence; conservative handling.
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CargoCapabilities {
    pub build_dir: CargoBuildDirCapability,
    pub metadata_had_build_directory: bool,
    pub env_build_dir_set: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputRootKind {
    Target,
    Build,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRoot {
    pub kind: OutputRootKind,
    pub logical_path: PathBuf,
    pub physical_path: Option<PathBuf>,
    pub exists: bool,
    pub is_symlink: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSet {
    pub target: OutputRoot,
    pub build: OutputRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedWorkspace {
    pub id: WorkspaceId,
    pub root: PathBuf,
    pub root_manifest: PathBuf,
    pub members: Vec<WorkspaceMember>,
    pub output: OutputSet,
    pub capability: CargoCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputOwnershipClass {
    PrivateBounded,
    ExternalUnproven,
    Shared,
    Uncertain,
}

impl OutputOwnershipClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::PrivateBounded => "private",
            Self::ExternalUnproven => "external-unproven",
            Self::Shared => "shared",
            Self::Uncertain => "uncertain",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalOutputGroup {
    /// Minimal covering canonical roots for measurement (outermost only).
    pub covering_roots: Vec<PathBuf>,
    /// All canonical physical paths contributing to the group.
    pub physical_paths: Vec<PathBuf>,
    /// Primary display path (first covering root, deterministic).
    pub display_path: PathBuf,
    pub owners: Vec<WorkspaceId>,
    pub ownership: OutputOwnershipClass,
    pub bytes: u64,
    pub metric: SizeMetric,
    pub newest_mtime: Option<SystemTime>,
    pub artifact_entries: u64,
    pub uncertain: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleOutputGroup {
    pub display_path: PathBuf,
    pub workspace_roots: Vec<PathBuf>,
    pub physical_paths: Vec<PathBuf>,
    pub bytes: u64,
    pub metric: SizeMetric,
    pub newest_mtime: Option<SystemTime>,
    pub artifact_entries: u64,
    pub ownership: OutputOwnershipClass,
}

/// Fail-fast gate instrumentation. Counts workspaces/groups exiting at each gate.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanCounters {
    pub directories_visited: u64,
    pub directories_pruned: u64,
    pub manifests_found: u64,
    pub unique_workspaces: u64,
    pub cargo_locate_calls: u64,
    pub cargo_metadata_calls: u64,
    pub cargo_failures: u64,
    pub empty_no_output_skipped: u64,
    pub active_skipped: u64,
    pub groups_measured: u64,
    pub bytes_measured: u64,
    pub reportable_groups: u64,
    pub pruned_no_cargo: u64,
    pub deduped_workspace_hits: u64,
    pub missing_output_skipped: u64,
    pub uncertain_skipped: u64,
}
