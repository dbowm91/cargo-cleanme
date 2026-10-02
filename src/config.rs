use crate::error::AppError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const CONFIG_TEMPLATE: &str = "[scan]\nrecency_seconds = 300\n\n# Optional exclusive scope. CLI root takes precedence.\n# root = \"/path/to/projects\"\n\nignore = []\nunignore = []\n";
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
pub fn init(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Config(format!(
                    "refusing to overwrite existing config {}",
                    path.display()
                ))
            } else {
                AppError::Io(e)
            }
        })?;
    f.write_all(CONFIG_TEMPLATE.as_bytes())?;
    f.sync_all()?;
    Ok(())
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
    fn init_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        init(&p).unwrap();
        assert!(init(&p).is_err());
    }
}
