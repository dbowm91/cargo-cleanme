use crate::error::AppError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const CONFIG_TEMPLATE: &str = include_str!("../config.toml");
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub scan: ScanConfig,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanConfig {
    #[serde(default = "default_recency")]
    pub recency_seconds: u64,
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub unignore: Vec<PathBuf>,
}
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            recency_seconds: 300,
            root: None,
            ignore: vec![],
            unignore: vec![],
        }
    }
}
fn default_recency() -> u64 {
    300
}
#[derive(Clone, Debug)]
pub struct ConfigPathResolver {
    override_path: Option<PathBuf>,
}
impl ConfigPathResolver {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            override_path: path,
        }
    }
    pub fn path(&self) -> Result<PathBuf, AppError> {
        if let Some(p) = &self.override_path {
            return Ok(p.clone());
        }
        let d = ProjectDirs::from("", "", "cargo-cleanme").ok_or_else(|| {
            AppError::Config("platform config directory is unavailable; use --config".into())
        })?;
        Ok(d.config_dir().join("config.toml"))
    }
}
pub fn load(path: &Path) -> Result<Config, AppError> {
    let c = if path.exists() {
        let s = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("cannot read {}: {e}", path.display())))?;
        toml::from_str(&s)
            .map_err(|e| AppError::Config(format!("cannot parse {}: {e}", path.display())))?
    } else {
        Config::default()
    };
    if c.scan.recency_seconds > u64::MAX / 1_000_000_000 {
        return Err(AppError::Config("scan.recency_seconds is too large".into()));
    }
    if let Some(p) = &c.scan.root {
        require_absolute(p, "scan.root")?;
    }
    for p in &c.scan.ignore {
        if !Path::new(p).is_absolute() {
            return Err(AppError::Config(format!(
                "ignore pattern must be absolute: {p}"
            )));
        }
    }
    for p in &c.scan.unignore {
        require_absolute(p, "unignore")?;
    }
    Ok(c)
}
fn require_absolute(p: &Path, field: &str) -> Result<(), AppError> {
    if !p.is_absolute() {
        Err(AppError::Config(format!(
            "{field} must be absolute: {}",
            p.display()
        )))
    } else {
        Ok(())
    }
}
pub fn init(path: &Path, force: bool) -> Result<(), AppError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    if !force {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    AppError::Config(format!(
                        "refusing to overwrite existing config {}; rerun with --force to replace it",
                        path.display()
                    ))
                } else {
                    AppError::Io(e)
                }
            })?;
        f.write_all(CONFIG_TEMPLATE.as_bytes())?;
        f.sync_all()?;
        return Ok(());
    }
    init_force(path)
}

/// Atomically replace (or create) the resolved config path.
///
/// Writes the template to a temporary sibling file, syncs it, then renames it
/// over the destination. The existing file is never truncated before the
/// replacement content is fully written; a failed write leaves the previous
/// file intact and removes the temporary sibling on a best-effort basis.
fn init_force(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    // The parent was created by `init`; a missing parent here is a race.
    if !parent.exists() {
        return Err(AppError::Config(format!(
            "config directory is unavailable: {}",
            parent.display()
        )));
    }
    let file_name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("config.toml"));
    for attempt in 0..100 {
        let mut temp_name = file_name.clone();
        temp_name.push(format!(".tmp.{}.{}", std::process::id(), attempt));
        let temp = parent.join(temp_name);
        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp);
        let mut f = match created {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(AppError::Io(e)),
        };
        let write_result: std::io::Result<()> = (|| {
            f.write_all(CONFIG_TEMPLATE.as_bytes())?;
            f.sync_all()?;
            drop(f);
            fs::rename(&temp, path)?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&temp);
            // A rename over a directory (or other non-file destination) must
            // not destroy the previous filesystem entry.
            if e.kind() == std::io::ErrorKind::AlreadyExists
                || e.kind() == std::io::ErrorKind::PermissionDenied
                || e.kind() == std::io::ErrorKind::InvalidInput
            {
                // Fall through to a descriptive config error below when the
                // destination cannot be replaced; preserve the OS detail.
            }
            // If the destination is a directory, report it explicitly.
            if path.exists() && !path.is_file() {
                return Err(AppError::Config(format!(
                    "refusing to replace non-file config {}: {e}",
                    path.display()
                )));
            }
            return Err(AppError::Io(e));
        }
        return Ok(());
    }
    Err(AppError::Config(format!(
        "could not create a temporary config beside {}",
        path.display()
    )))
}
pub fn show(config: &Config) -> Result<String, AppError> {
    toml::to_string_pretty(config).map_err(|e| AppError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_recency_and_absent_config() {
        let d = tempfile::tempdir().unwrap();
        let c = load(&d.path().join("missing.toml")).unwrap();
        assert_eq!(c.scan.recency_seconds, 300);
    }
    #[test]
    fn malformed_and_relative_paths_fail() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bad.toml");
        fs::write(&p, "[scan]\nrecency_seconds = nope\n").unwrap();
        assert!(load(&p).is_err());
        fs::write(&p, "[scan]\nroot = \"relative\"\n").unwrap();
        assert!(load(&p).is_err());
    }
    #[test]
    fn checked_in_template_parses_and_matches_defaults() {
        let config: Config =
            toml::from_str(CONFIG_TEMPLATE).expect("checked-in template must parse");
        assert_eq!(config.scan.recency_seconds, 300);
        assert!(config.scan.root.is_none());
        assert!(config.scan.ignore.is_empty());
        assert!(config.scan.unignore.is_empty());
        let defaults = Config::default();
        assert_eq!(config.scan.recency_seconds, defaults.scan.recency_seconds);
        assert_eq!(config.scan.root, defaults.scan.root);
        assert_eq!(config.scan.ignore, defaults.scan.ignore);
        assert_eq!(config.scan.unignore, defaults.scan.unignore);
    }
    #[test]
    fn config_template_matches_repository_file() {
        let on_disk = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"))
            .expect("repository config.toml must exist");
        assert_eq!(
            CONFIG_TEMPLATE, on_disk,
            "config init source must equal the checked-in template"
        );
    }
    #[test]
    fn init_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        init(&p, false).unwrap();
        assert!(init(&p, false).is_err());
    }
    #[test]
    fn init_bytes_equal_checked_in_template() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        init(&p, false).unwrap();
        let written = fs::read_to_string(&p).unwrap();
        assert_eq!(written, CONFIG_TEMPLATE);
        // The written file must load and match defaults.
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.scan.recency_seconds, 300);
    }
    #[test]
    fn init_force_replaces_existing_config() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        init(&p, false).unwrap();
        fs::write(&p, "[scan]\nrecency_seconds = 60\n").unwrap();
        assert_eq!(load(&p).unwrap().scan.recency_seconds, 60);
        assert!(init(&p, false).is_err());
        init(&p, true).unwrap();
        let rewritten = fs::read_to_string(&p).unwrap();
        assert_eq!(rewritten, CONFIG_TEMPLATE);
        assert_eq!(load(&p).unwrap().scan.recency_seconds, 300);
    }
    #[test]
    fn malformed_config_requires_force_for_replacement() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[scan]\nrecency_seconds = nope\n").unwrap();
        assert!(load(&p).is_err());
        assert!(init(&p, false).is_err());
        init(&p, true).unwrap();
        assert_eq!(load(&p).unwrap().scan.recency_seconds, 300);
    }
    #[test]
    fn failed_force_leaves_previous_file_intact() {
        let d = tempfile::tempdir().unwrap();
        // A directory at the destination cannot be replaced by the file
        // rename; the call must fail without removing the directory.
        let dir = d.path().join("config.toml");
        fs::create_dir(&dir).unwrap();
        assert!(init(&dir, true).is_err());
        assert!(dir.is_dir());
        // No stray temporary sibling may be left behind.
        let leftovers: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.toml.tmp.")
            })
            .collect();
        assert!(leftovers.is_empty());

        // A normal file survives a refused non-force init byte-for-byte.
        let p = d.path().join("other.toml");
        fs::write(&p, "[scan]\nrecency_seconds = 61\n").unwrap();
        assert!(init(&p, false).is_err());
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "[scan]\nrecency_seconds = 61\n"
        );
    }
}
