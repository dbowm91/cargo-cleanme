use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CURRENT_SCHEMA: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateProblem {
    Invalid { path: PathBuf, detail: String },
    Unavailable { path: PathBuf, detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateLoad {
    Missing,
    Loaded(DiscoveryState),
    RecoverableInvalid(StateProblem),
    UnsupportedNewer {
        path: PathBuf,
        schema: u64,
        current: u32,
    },
    Unavailable(StateProblem),
}

impl StateLoad {
    pub fn diagnostic(&self) -> Option<String> {
        match self {
            Self::RecoverableInvalid(StateProblem::Invalid { path, detail }) => Some(format!(
                "ignoring unusable discovery state {}: {detail}",
                path.display()
            )),
            Self::UnsupportedNewer { path, schema, .. } => Some(format!(
                "discovery state {} uses unsupported schema {schema}; update cargo-cleanme",
                path.display()
            )),
            Self::Unavailable(StateProblem::Unavailable { path, detail }) => Some(format!(
                "cannot read discovery state {}: {detail}",
                path.display()
            )),
            _ => None,
        }
    }
}

/// Select the only prior states that Full reconciliation may safely publish
/// over. `None` means the state is newer than this binary or unavailable.
pub fn full_reconciliation_prior(load: &StateLoad) -> Option<(DiscoveryState, bool)> {
    match load {
        StateLoad::Loaded(state) => Some((state.clone(), false)),
        StateLoad::Missing => Some((DiscoveryState::default(), false)),
        StateLoad::RecoverableInvalid(_) => Some((DiscoveryState::default(), true)),
        StateLoad::UnsupportedNewer { .. } | StateLoad::Unavailable(_) => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryState {
    pub schema_version: u32,
    pub last_full_at: Option<u64>,
    pub projects: Vec<ProjectRecord>,
    pub learned_roots: Vec<LearnedRoot>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectRecord {
    /// Workspace identity when Cargo resolution succeeded, otherwise the manifest directory.
    pub workspace: PathBuf,
    #[serde(default)]
    pub manifest: Option<PathBuf>,
    #[serde(default)]
    pub resolution: ResolutionStatus,
    #[serde(default)]
    pub observed_at: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    #[default]
    Resolved,
    ManifestObservedUnresolved,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LearnedRoot {
    pub path: PathBuf,
    pub last_project_seen_at: u64,
}

#[derive(Clone, Debug)]
pub struct ProjectObservation {
    pub manifest: PathBuf,
    pub workspace: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reconciliation {
    Publish(DiscoveryState),
    NoPublication(&'static str),
}

pub fn uncertainty_intersects(root: &Path, uncertain: &Path) -> bool {
    root == uncertain || root.starts_with(uncertain) || uncertain.starts_with(root)
}

/// Reconcile only after a complete Full traversal. Localized uncertainty retains
/// intersecting old roots while positive observations are always merged.
pub fn reconcile_full(
    prior: &DiscoveryState,
    observations: &[ProjectObservation],
    uncertainty: &[PathBuf],
    complete: bool,
    now: u64,
    retention_days: u16,
    home: Option<&Path>,
) -> Reconciliation {
    if !complete {
        return Reconciliation::NoPublication("Full traversal was incomplete");
    }
    let mut next = prior.clone();
    let observed_at = now.max(prior.last_full_at.unwrap_or(0));
    next.schema_version = CURRENT_SCHEMA;
    next.last_full_at = Some(observed_at);
    next.projects = observations
        .iter()
        .map(|o| ProjectRecord {
            workspace: o
                .workspace
                .clone()
                .unwrap_or_else(|| o.manifest.parent().unwrap_or(&o.manifest).to_path_buf()),
            manifest: Some(o.manifest.clone()),
            resolution: if o.workspace.is_some() {
                ResolutionStatus::Resolved
            } else {
                ResolutionStatus::ManifestObservedUnresolved
            },
            observed_at,
        })
        .collect();
    let mut learned = Vec::<LearnedRoot>::new();
    for o in observations {
        let project = o.workspace.as_deref().or_else(|| o.manifest.parent());
        if let Some(project) = project {
            let candidate = project
                .parent()
                .filter(|p| Some(*p) != home && !is_broad_root(p))
                .unwrap_or(project);
            let path = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
            learned.push(LearnedRoot {
                path,
                last_project_seen_at: observed_at,
            });
        }
    }
    // "Is there a fresh root at or below this path?" answered by one lookup
    // instead of a scan of every fresh root. Every ancestor of every fresh root
    // is indexed; the walk stops as soon as it reaches an already-indexed
    // ancestor, because that ancestor's own ancestors are indexed too (L5/O2).
    let mut covered: HashSet<PathBuf> = HashSet::new();
    for root in &learned {
        let mut cursor = Some(root.path.as_path());
        while let Some(candidate) = cursor {
            if !covered.insert(candidate.to_path_buf()) {
                break;
            }
            cursor = candidate.parent();
        }
    }
    for old in &prior.learned_roots {
        if covered.contains(&old.path) {
            continue;
        }
        if uncertainty
            .iter()
            .any(|p| uncertainty_intersects(&old.path, p))
            || old.last_project_seen_at > observed_at
            || retention_days == 0
            || observed_at.saturating_sub(old.last_project_seen_at)
                <= u64::from(retention_days) * 86400
        {
            learned.push(old.clone());
        }
    }
    learned.sort_by(|a, b| a.path.cmp(&b.path));
    // Collapse by nearest already-represented ancestor. `learned` is sorted, and
    // a parent always sorts before its children, so every possible parent has
    // been seen when its child is reached (L5/O2).
    let mut collapsed: Vec<LearnedRoot> = Vec::new();
    let mut representative: HashMap<PathBuf, usize> = HashMap::new();
    for root in learned {
        // The walk starts at the root itself, not at its parent, so an exact
        // duplicate folds too. A workspace with N members contributes N
        // observations that all resolve to one root, and the parent-only walk
        // folded nesting while publishing the identical path N times.
        let mut cursor = Some(root.path.as_path());
        let parent = loop {
            let Some(candidate) = cursor else { break None };
            if let Some(&index) = representative.get(candidate) {
                break Some(index);
            }
            cursor = candidate.parent();
        };
        match parent {
            Some(index) => {
                collapsed[index].last_project_seen_at = collapsed[index]
                    .last_project_seen_at
                    .max(root.last_project_seen_at);
            }
            None => {
                representative.insert(root.path.clone(), collapsed.len());
                collapsed.push(root);
            }
        }
    }
    next.learned_roots = collapsed;
    Reconciliation::Publish(next)
}

fn is_broad_root(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    #[cfg(unix)]
    if [
        "/Users",
        "/home",
        "/Volumes",
        "/Applications",
        "/Library",
        "/opt",
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/dev",
        "/proc",
        "/sys",
        "/run",
        "/etc",
    ]
    .iter()
    .any(|p| path == Path::new(p))
    {
        return true;
    }
    #[cfg(windows)]
    if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        matches!(
            n.to_ascii_lowercase().as_str(),
            "users"
                | "documents and settings"
                | "windows"
                | "program files"
                | "program files (x86)"
                | "programdata"
        )
    }) {
        return true;
    }
    false
}

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn state_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "cargo-cleanme")?;
    Some(
        dirs.state_dir()
            .or_else(|| Some(dirs.data_local_dir()))?
            .join("discovery-state.json"),
    )
}

/// Loading distinguishes absence, supported state, recoverable invalid data,
/// newer schemas, and I/O failures so callers never infer overwrite safety from text.
pub fn load_default() -> StateLoad {
    let Some(path) = state_path() else {
        return StateLoad::Missing;
    };
    load_at(&path)
}
pub fn load_at(path: &Path) -> StateLoad {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StateLoad::Missing,
        Err(e) => {
            return StateLoad::Unavailable(StateProblem::Unavailable {
                path: path.to_path_buf(),
                detail: e.to_string(),
            });
        }
    };
    // Check the version before decoding the current representation. A newer
    // writer may have added fields or changed record structure that this
    // binary must leave untouched.
    let version = match serde_json::from_slice::<serde_json::Value>(&data) {
        Ok(serde_json::Value::Object(object)) => object
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        _ => None,
    };
    if let Some(schema) = version.filter(|schema| *schema > u64::from(CURRENT_SCHEMA)) {
        return StateLoad::UnsupportedNewer {
            path: path.to_path_buf(),
            schema,
            current: CURRENT_SCHEMA,
        };
    }
    let invalid = |detail: String| {
        StateLoad::RecoverableInvalid(StateProblem::Invalid {
            path: path.to_path_buf(),
            detail,
        })
    };
    let mut state: DiscoveryState = match serde_json::from_slice(&data) {
        Ok(state) => state,
        Err(error) => return invalid(error.to_string()),
    };
    if state.schema_version == 0 {
        return invalid("invalid schema 0".into());
    }
    // v1 records deserialize with defaults as authoritative resolved projects.
    state.schema_version = CURRENT_SCHEMA;
    StateLoad::Loaded(state)
}

pub fn publish(state: &DiscoveryState) -> Result<(), String> {
    let Some(path) = state_path() else {
        return Err("machine-local state directory is unavailable".into());
    };
    publish_at(&path, state)
}
// State is an optimization only: concurrent publishers use atomic last-writer-wins replacement.
fn publish_at(path: &Path, state: &DiscoveryState) -> Result<(), String> {
    let parent = path.parent().ok_or("state path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    let (tmp, mut file) = create_unique_temp(path)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(())
}
fn create_unique_temp(path: &Path) -> Result<(PathBuf, fs::File), String> {
    let parent = path.parent().ok_or("state path has no parent")?;
    let name = path
        .file_name()
        .ok_or("state path has no file name")?
        .to_string_lossy();
    for attempt in 0..32u32 {
        let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), attempt));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("could not allocate a unique discovery-state temporary file after 32 attempts".into())
}
#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}
#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v1_migrates_in_memory_and_newer_schema_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        fs::write(&p, r#"{"schema_version":1,"last_full_at":12,"projects":[{"workspace":"/work/a"}],"learned_roots":[]}"#).unwrap();
        let StateLoad::Loaded(loaded) = load_at(&p) else {
            panic!("expected loaded state")
        };
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA);
        assert_eq!(loaded.projects[0].resolution, ResolutionStatus::Resolved);
        fs::write(&p, r#"{"schema_version":99}"#).unwrap();
        assert!(matches!(
            load_at(&p),
            StateLoad::UnsupportedNewer { schema: 99, .. }
        ));
    }

    #[test]
    fn state_load_classifies_absent_invalid_newer_and_unavailable_paths() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        assert_eq!(load_at(&p), StateLoad::Missing);

        fs::write(&p, br#"{"schema_version":0}"#).unwrap();
        assert!(matches!(load_at(&p), StateLoad::RecoverableInvalid(_)));
        fs::write(&p, b"not json").unwrap();
        assert!(matches!(load_at(&p), StateLoad::RecoverableInvalid(_)));
        fs::write(&p, br#"{"schema_version":18446744073709551615}"#).unwrap();
        assert!(matches!(
            load_at(&p),
            StateLoad::UnsupportedNewer {
                schema: u64::MAX,
                ..
            }
        ));

        let directory = d.path().join("unreadable.json");
        fs::create_dir(&directory).unwrap();
        assert!(matches!(load_at(&directory), StateLoad::Unavailable(_)));
    }

    #[test]
    fn supported_schema_with_invalid_structure_is_recoverable() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        fs::write(
            &p,
            br#"{"schema_version":2,"projects":[{"not_workspace":"/work"}]}"#,
        )
        .unwrap();
        assert!(matches!(load_at(&p), StateLoad::RecoverableInvalid(_)));
    }

    #[test]
    fn full_reconciliation_recovers_invalid_only_after_complete_scan() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        for invalid in [b"{\"schema_version\":0}".as_slice(), b"corrupt"] {
            fs::write(&p, invalid).unwrap();
            let load = load_at(&p);
            let (prior, replacing_invalid) = full_reconciliation_prior(&load).unwrap();
            assert!(replacing_invalid);
            assert_eq!(
                reconcile_full(&prior, &[], &[], false, 10, 30, None),
                Reconciliation::NoPublication("Full traversal was incomplete")
            );
            assert_eq!(fs::read(&p).unwrap(), invalid);

            let Reconciliation::Publish(next) =
                reconcile_full(&prior, &[], &[], true, 10, 30, None)
            else {
                panic!("complete Full reconciliation should produce a generation")
            };
            publish_at(&p, &next).unwrap();
            let StateLoad::Loaded(recovered) = load_at(&p) else {
                panic!("recovered state should use the current schema")
            };
            assert_eq!(recovered.schema_version, CURRENT_SCHEMA);
        }
    }

    #[test]
    fn full_reconciliation_never_selects_newer_or_unavailable_state_for_publication() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        let bytes = br#"{"schema_version":99,"future":"preserve me"}"#;
        fs::write(&p, bytes).unwrap();
        assert!(full_reconciliation_prior(&load_at(&p)).is_none());
        assert_eq!(fs::read(&p).unwrap(), bytes);

        let directory = d.path().join("directory.json");
        fs::create_dir(&directory).unwrap();
        assert!(full_reconciliation_prior(&load_at(&directory)).is_none());
    }

    #[test]
    fn full_reconciliation_creates_missing_and_migrates_v1_state() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        for initial in [
            None,
            Some(
                br#"{"schema_version":1,"last_full_at":4,"projects":[],"learned_roots":[]}"#
                    .as_slice(),
            ),
        ] {
            if let Some(bytes) = initial {
                fs::write(&p, bytes).unwrap();
            } else {
                let _ = fs::remove_file(&p);
            }
            let (prior, replacing_invalid) =
                full_reconciliation_prior(&load_at(&p)).expect("safe Full prior");
            assert!(!replacing_invalid);
            let Reconciliation::Publish(next) =
                reconcile_full(&prior, &[], &[], true, 10, 30, None)
            else {
                panic!("complete Full reconciliation should publish")
            };
            publish_at(&p, &next).unwrap();
            let StateLoad::Loaded(reconciled) = load_at(&p) else {
                panic!("published state should be readable")
            };
            assert_eq!(reconciled.schema_version, CURRENT_SCHEMA);
            assert_eq!(reconciled.last_full_at, Some(10));
        }
    }
    #[test]
    fn unique_temp_attempts_replace_existing_state() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        let state = DiscoveryState {
            schema_version: CURRENT_SCHEMA,
            last_full_at: Some(12),
            ..Default::default()
        };
        let collision = d
            .path()
            .join(format!(".state.json.{}.0.tmp", std::process::id()));
        fs::write(&collision, b"busy").unwrap();
        publish_at(&p, &state).unwrap();
        let StateLoad::Loaded(loaded) = load_at(&p) else {
            panic!("expected loaded state")
        };
        assert_eq!(loaded.last_full_at, Some(12));
        assert_eq!(fs::read(&collision).unwrap(), b"busy");
    }

    #[test]
    fn localized_uncertainty_retains_intersecting_roots_and_publishes_positives() {
        let prior = DiscoveryState {
            schema_version: 1,
            learned_roots: vec![
                LearnedRoot {
                    path: "/work/uncertain".into(),
                    last_project_seen_at: 1,
                },
                LearnedRoot {
                    path: "/work/expired".into(),
                    last_project_seen_at: 1,
                },
            ],
            ..Default::default()
        };
        let result = reconcile_full(
            &prior,
            &[ProjectObservation {
                manifest: "/new/app/Cargo.toml".into(),
                workspace: None,
            }],
            &["/work/uncertain/nested".into()],
            true,
            100_000_000,
            30,
            None,
        );
        let Reconciliation::Publish(state) = result else {
            panic!("completed Full should publish")
        };
        assert!(
            state
                .projects
                .iter()
                .any(|p| p.resolution == ResolutionStatus::ManifestObservedUnresolved)
        );
        assert!(
            state
                .learned_roots
                .iter()
                .any(|r| r.path == Path::new("/work/uncertain"))
        );
        assert!(
            !state
                .learned_roots
                .iter()
                .any(|r| r.path == Path::new("/work/expired"))
        );
    }

    #[test]
    fn zero_retention_disables_expiration_and_nested_roots_collapse() {
        let prior = DiscoveryState {
            learned_roots: vec![
                LearnedRoot {
                    path: "/work".into(),
                    last_project_seen_at: 1,
                },
                LearnedRoot {
                    path: "/work/old".into(),
                    last_project_seen_at: 1,
                },
            ],
            ..Default::default()
        };
        let result = reconcile_full(&prior, &[], &[], true, 100_000_000, 0, None);
        let Reconciliation::Publish(state) = result else {
            panic!("completed Full should publish")
        };
        assert_eq!(state.learned_roots.len(), 1);
        assert_eq!(state.learned_roots[0].path, Path::new("/work"));
    }

    #[test]
    fn uncertainty_containment_is_symmetric() {
        let root = Path::new("/work/a");
        assert!(uncertainty_intersects(root, Path::new("/work/a")));
        assert!(uncertainty_intersects(root, Path::new("/work")));
        assert!(uncertainty_intersects(root, Path::new("/work/a/nested")));
        assert!(!uncertainty_intersects(root, Path::new("/workspace/a")));
    }

    #[test]
    fn failed_replacement_preserves_existing_destination_and_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("state.json");
        fs::create_dir(&destination).unwrap();
        let sentinel = destination.join("preserve");
        fs::write(&sentinel, b"existing").unwrap();
        let state = DiscoveryState {
            schema_version: CURRENT_SCHEMA,
            ..Default::default()
        };
        assert!(publish_at(&destination, &state).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"existing");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
