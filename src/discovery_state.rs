use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscoveryState {
    pub schema_version: u32,
    pub last_full_at: Option<u64>,
    pub projects: Vec<ProjectRecord>,
    pub learned_roots: Vec<LearnedRoot>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub workspace: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearnedRoot {
    pub path: PathBuf,
    pub last_project_seen_at: u64,
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
pub fn load_default() -> Result<Option<DiscoveryState>, String> {
    let Some(path) = state_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read(&path)
        .map_err(|e| format!("cannot read discovery state {}: {e}", path.display()))?;
    let state: DiscoveryState = serde_json::from_slice(&data)
        .map_err(|e| format!("ignoring corrupt discovery state {}: {e}", path.display()))?;
    if state.schema_version > 1 {
        return Err(format!(
            "discovery state {} uses unsupported schema {}; update cargo-cleanme",
            path.display(),
            state.schema_version
        ));
    }
    Ok(Some(state))
}
pub fn publish(state: &DiscoveryState) -> Result<(), String> {
    let Some(path) = state_path() else {
        return Err("machine-local state directory is unavailable".into());
    };
    let parent = path.parent().ok_or("state path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    publish_at(&path, state)
}

fn publish_at(path: &std::path::Path, state: &DiscoveryState) -> Result<(), String> {
    let parent = path.parent().ok_or("state path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut tmp = path.to_path_buf();
    tmp.set_extension(format!("tmp.{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(tmp);
        return Err(e.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versioned_state_round_trips_and_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = DiscoveryState {
            schema_version: 1,
            last_full_at: Some(12),
            projects: vec![ProjectRecord {
                workspace: PathBuf::from("/work/a"),
            }],
            learned_roots: vec![LearnedRoot {
                path: PathBuf::from("/work"),
                last_project_seen_at: 10,
            }],
        };
        publish_at(&path, &state).unwrap();
        let loaded: DiscoveryState = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.projects[0].workspace, PathBuf::from("/work/a"));
        let replacement = DiscoveryState {
            last_full_at: Some(13),
            ..state
        };
        publish_at(&path, &replacement).unwrap();
        let loaded: DiscoveryState = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.last_full_at, Some(13));
        assert!(!dir.path().join("state.tmp").exists());
    }
}
