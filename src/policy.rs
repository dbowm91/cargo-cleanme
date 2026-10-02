use crate::{
    config::ScanConfig,
    domain::{DiscoveryFilters, EffectiveScanPolicy, ScanRequest, ScanScope},
    error::AppError,
};
use std::{fs, path::PathBuf, time::Duration};
pub fn resolve(request: ScanRequest, config: &ScanConfig) -> Result<EffectiveScanPolicy, AppError> {
    let explicit = request.cli_root.or_else(|| config.root.clone());
    let (scope, discovery_filters) = if let Some(root) = explicit {
        let m = fs::symlink_metadata(&root).map_err(|e| AppError::InvalidRoot {
            path: root.display().to_string(),
            reason: e.to_string(),
        })?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(AppError::InvalidRoot {
                path: root.display().to_string(),
                reason: "expected a real directory".into(),
            });
        }
        (ScanScope::Explicit(root), DiscoveryFilters::Bypassed)
    } else {
        (
            ScanScope::Global(global_roots()),
            DiscoveryFilters::Active {
                ignore: config.ignore.clone(),
                unignore: config.unignore.clone(),
            },
        )
    };
    Ok(EffectiveScanPolicy {
        recency: Duration::from_secs(config.recency_seconds),
        scope,
        discovery_filters,
    })
}
#[cfg(unix)]
fn global_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}
#[cfg(windows)]
fn global_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::{
        Storage::FileSystem::{DRIVE_FIXED, DRIVE_REMOVABLE, GetDriveTypeW},
        System::SystemInformation::GetLogicalDrives,
    };
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter_map(|i| {
            if mask & (1 << i) == 0 {
                return None;
            }
            let letter = char::from(b'A' + i as u8);
            let path = format!("{letter}:\\");
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (kind == DRIVE_FIXED || kind == DRIVE_REMOVABLE).then(|| PathBuf::from(path))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfig;
    use tempfile::tempdir;
    #[test]
    fn cli_root_wins_and_bypasses_filters() {
        let d = tempdir().unwrap();
        let cli = d.path().to_path_buf();
        let configured = PathBuf::from("/tmp");
        let c = ScanConfig {
            root: Some(configured),
            ignore: vec!["/tmp/*".into()],
            unignore: vec![PathBuf::from("/tmp/keep")],
            ..Default::default()
        };
        let p = resolve(
            ScanRequest {
                cli_root: Some(cli.clone()),
            },
            &c,
        )
        .unwrap();
        assert_eq!(p.scope, ScanScope::Explicit(cli));
        assert_eq!(p.discovery_filters, DiscoveryFilters::Bypassed);
        assert_eq!(p.recency, Duration::from_secs(300));
    }
    #[test]
    fn configured_root_wins_over_global() {
        let d = tempdir().unwrap();
        let c = ScanConfig {
            root: Some(d.path().into()),
            ..Default::default()
        };
        assert!(matches!(
            resolve(ScanRequest { cli_root: None }, &c).unwrap().scope,
            ScanScope::Explicit(_)
        ));
    }
}
