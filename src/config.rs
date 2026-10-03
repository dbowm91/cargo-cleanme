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
    #[serde(default)]
    pub cleanup: CleanupConfig,
}
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CleanupConfig {
    #[serde(default)]
    pub allowed_output_roots: Vec<PathBuf>,
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
    #[serde(default = "default_retention")]
    pub learned_root_retention_days: u16,
}
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            recency_seconds: 300,
            root: None,
            ignore: vec![],
            unignore: vec![],
            learned_root_retention_days: 30,
        }
    }
}
fn default_recency() -> u64 {
    300
}
fn default_retention() -> u16 {
    30
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
    if c.scan.learned_root_retention_days > 3650 {
        return Err(AppError::Config(
            "scan.learned_root_retention_days must be between 0 and 3650".into(),
        ));
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
    for p in &c.cleanup.allowed_output_roots {
        require_absolute(p, "cleanup.allowed_output_roots")?;
    }
    Ok(c)
}

pub fn load_or_create(path: &Path) -> Result<Config, AppError> {
    if !path.exists() {
        create_initial(path)?;
    }
    load(path)
}

/// Ensure a config editor target exists without requiring its current contents to parse.
pub fn ensure_exists(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        create_initial(path)?;
    }
    Ok(())
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
fn create_initial(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
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
            // Creating the final link is atomic and never overwrites a
            // concurrent winner. Both names are siblings on the same volume.
            fs::hard_link(&temp, path)?;
            fs::remove_file(&temp)?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&temp);
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                return Ok(());
            }
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
        fs::write(
            &p,
            "[cleanup]\nallowed_output_roots = [\"relative/root\"]\n",
        )
        .unwrap();
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
        assert!(config.cleanup.allowed_output_roots.is_empty());
        let defaults = Config::default();
        assert_eq!(config.scan.recency_seconds, defaults.scan.recency_seconds);
        assert_eq!(config.scan.root, defaults.scan.root);
        assert_eq!(config.scan.ignore, defaults.scan.ignore);
        assert_eq!(config.scan.unignore, defaults.scan.unignore);
        assert_eq!(
            config.cleanup.allowed_output_roots,
            defaults.cleanup.allowed_output_roots
        );
    }
    #[test]
    fn config_template_matches_repository_file() {
        let on_disk = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"))
            .expect("repository config.toml must exist");
        assert_eq!(
            CONFIG_TEMPLATE, on_disk,
            "bootstrap source must equal the checked-in template"
        );
    }
    #[test]
    fn operational_bootstrap_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let initial = load_or_create(&p).unwrap();
        assert_eq!(initial.scan.recency_seconds, 300);
        fs::write(&p, "[scan]\nrecency_seconds = 61\n").unwrap();
        assert_eq!(load_or_create(&p).unwrap().scan.recency_seconds, 61);
    }
    #[test]
    fn bootstrap_bytes_equal_checked_in_template() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        load_or_create(&p).unwrap();
        let written = fs::read_to_string(&p).unwrap();
        assert_eq!(written, CONFIG_TEMPLATE);
        // The written file must load and match defaults.
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.scan.recency_seconds, 300);
    }
    #[test]
    fn malformed_config_is_not_replaced_automatically() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[scan]\nrecency_seconds = nope\n").unwrap();
        assert!(load(&p).is_err());
        assert!(load_or_create(&p).is_err());
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "[scan]\nrecency_seconds = nope\n"
        );
    }
    #[test]
    fn concurrent_first_use_creates_one_complete_template() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("nested/config.toml");
        let path = p.clone();
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || load_or_create(&path).unwrap())
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().scan.recency_seconds, 300);
        }
        assert_eq!(fs::read(&p).unwrap(), CONFIG_TEMPLATE.as_bytes());
    }
}
