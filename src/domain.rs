use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanRequest {
    pub cli_root: Option<PathBuf>,
    pub full: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanScope {
    Explicit(PathBuf),
    /// Multiple explicit roots forming one discovery boundary (cleanup only).
    ExplicitRoots(Vec<PathBuf>),
    Global(Vec<PathBuf>),
    Routine(Vec<PathBuf>),
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

/// Why a discovered manifest did not receive authoritative Cargo coverage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedOwnershipParticipant {
    pub manifest: PathBuf,
    pub stage: &'static str,
    pub reason: String,
}

/// Workspace resolution plus explicit coverage of the discovered manifest set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolutionCoverage {
    pub workspaces: Vec<ResolvedWorkspace>,
    pub unresolved: Vec<UnresolvedOwnershipParticipant>,
    pub discovered_manifest_count: usize,
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
///
/// Timing fields (`*_nanos`) accumulate wall time spent in each phase/process
/// (C002 §7.7). They are `u64` nanosecond counts to preserve `Eq`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanCounters {
    pub directories_visited: u64,
    pub directories_pruned: u64,
    pub platform_system_prunes: u64,
    pub cargo_home_prunes: u64,
    pub rustup_home_prunes: u64,
    pub target_vcs_prunes: u64,
    pub user_ignore_prunes: u64,
    pub manifests_found: u64,
    pub unique_workspaces: u64,
    pub cargo_locate_calls: u64,
    pub cargo_metadata_calls: u64,
    pub cargo_failures: u64,
    /// Discovered cleanup-scope manifests not authoritatively covered by
    /// Cargo workspace metadata. Kept separate from subprocess failures.
    pub unresolved_ownership: u64,
    pub empty_no_output_skipped: u64,
    pub active_skipped: u64,
    pub groups_measured: u64,
    pub bytes_measured: u64,
    pub reportable_groups: u64,
    pub pruned_no_cargo: u64,
    pub deduped_workspace_hits: u64,
    pub missing_output_skipped: u64,
    pub uncertain_skipped: u64,
    pub discovery_nanos: u64,
    pub cargo_locate_nanos: u64,
    pub cargo_metadata_nanos: u64,
    pub source_activity_nanos: u64,
    pub output_sizing_nanos: u64,
    /// C003 §7.6: final-proof ownership-universe re-resolution work, kept
    /// separate from the initial discovery counters so `--stats` cannot hide it.
    pub proof_workspaces_refreshed: u64,
    pub proof_metadata_peak_concurrency: u64,
    pub proof_cargo_locate_calls: u64,
    pub proof_cargo_metadata_calls: u64,
    pub proof_cargo_locate_nanos: u64,
    pub proof_cargo_metadata_nanos: u64,
    pub proof_source_activity_nanos: u64,
    pub proof_output_sizing_nanos: u64,
}

impl ScanCounters {
    /// One-line semantic counters for `--stats` stderr output.
    pub fn stats_line(&self) -> String {
        format!(
            "visited_entries={} pruned_directories={} platform_prunes={} cargo_home_prunes={} rustup_prunes={} target_vcs_prunes={} user_ignore_prunes={} manifests={} workspaces={} locate={} metadata={} failures={} unresolved_ownership={} empty_skipped={} active_skipped={} groups_measured={} bytes={} reportable={} deduped_hits={} uncertain_skipped={}",
            self.directories_visited,
            self.directories_pruned,
            self.platform_system_prunes,
            self.cargo_home_prunes,
            self.rustup_home_prunes,
            self.target_vcs_prunes,
            self.user_ignore_prunes,
            self.manifests_found,
            self.unique_workspaces,
            self.cargo_locate_calls,
            self.cargo_metadata_calls,
            self.cargo_failures,
            self.unresolved_ownership,
            self.empty_no_output_skipped + self.missing_output_skipped,
            self.active_skipped,
            self.groups_measured,
            self.bytes_measured,
            self.reportable_groups,
            self.deduped_workspace_hits,
            self.uncertain_skipped,
        )
    }

    /// Phase/process timing line for `--stats` stderr output.
    pub fn timings_line(&self) -> String {
        format!(
            "discovery={:.2}ms locate={:.2}ms metadata={:.2}ms source_activity={:.2}ms output_sizing={:.2}ms",
            self.discovery_nanos as f64 / 1_000_000.0,
            self.cargo_locate_nanos as f64 / 1_000_000.0,
            self.cargo_metadata_nanos as f64 / 1_000_000.0,
            self.source_activity_nanos as f64 / 1_000_000.0,
            self.output_sizing_nanos as f64 / 1_000_000.0,
        )
    }

    /// Merge final-proof counters into the dedicated proof fields.
    ///
    /// C003-F4: final ownership-universe re-resolution work is real work that
    /// `--stats` must account for. It is merged into separate `proof_*` fields
    /// (never into the initial-scan counters) so the extra bounded Cargo cost
    /// at the destructive boundary stays visible.
    pub fn merge_proof(&mut self, proof: &ScanCounters) {
        self.proof_cargo_locate_calls = self
            .proof_cargo_locate_calls
            .saturating_add(proof.cargo_locate_calls);
        self.proof_cargo_metadata_calls = self
            .proof_cargo_metadata_calls
            .saturating_add(proof.cargo_metadata_calls);
        self.proof_cargo_locate_nanos = self
            .proof_cargo_locate_nanos
            .saturating_add(proof.cargo_locate_nanos);
        self.proof_cargo_metadata_nanos = self
            .proof_cargo_metadata_nanos
            .saturating_add(proof.cargo_metadata_nanos);
        self.proof_source_activity_nanos = self
            .proof_source_activity_nanos
            .saturating_add(proof.source_activity_nanos);
        self.proof_output_sizing_nanos = self
            .proof_output_sizing_nanos
            .saturating_add(proof.output_sizing_nanos);
    }

    /// Final-proof counter line for `cleanup --stats` stderr output.
    pub fn proof_stats_line(&self) -> String {
        format!(
            "proof_universes_refreshed={} proof_locate={} proof_metadata={} proof_metadata_peak={}",
            self.proof_workspaces_refreshed,
            self.proof_cargo_locate_calls,
            self.proof_cargo_metadata_calls,
            self.proof_metadata_peak_concurrency,
        )
    }

    /// Final-proof timing line for `cleanup --stats` stderr output.
    pub fn proof_timings_line(&self) -> String {
        format!(
            "proof_locate={:.2}ms proof_metadata={:.2}ms proof_source_activity={:.2}ms proof_output_sizing={:.2}ms",
            self.proof_cargo_locate_nanos as f64 / 1_000_000.0,
            self.proof_cargo_metadata_nanos as f64 / 1_000_000.0,
            self.proof_source_activity_nanos as f64 / 1_000_000.0,
            self.proof_output_sizing_nanos as f64 / 1_000_000.0,
        )
    }
}
