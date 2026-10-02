use cargo_cleanme::{
    analysis::{self, ScanClock},
    config::ScanConfig,
    discovery,
    domain::{EligibleProject, ScanRequest},
    policy, report,
};
use std::{
    fs,
    time::{Duration, SystemTime},
};
use tempfile::tempdir;

#[test]
fn end_to_end_reports_only_inactive_artifact_projects() {
    let temp = tempdir().unwrap();
    let project = temp.path().join("old-project");
    fs::create_dir_all(project.join("target/debug")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(project.join("target/debug/app"), vec![0u8; 8192]).unwrap();
    let config = ScanConfig {
        root: Some(temp.path().to_path_buf()),
        ..Default::default()
    };
    let policy = policy::resolve(ScanRequest { cli_root: None }, &config).unwrap();
    let mut report = discovery::scan(&policy).unwrap();
    assert_eq!(report.eligible.len(), 1);
    let start = SystemTime::now() + Duration::from_secs(5);
    let clock = ScanClock::new(start, Duration::ZERO).unwrap();
    let candidate = report.eligible.pop().unwrap();
    let analyzed = analysis::analyze(
        &cargo_cleanme::domain::DiscoveredProject {
            project_root: candidate.project_root.clone(),
            manifest_path: candidate.project_root.join("Cargo.toml"),
            target_path: candidate.artifact.target_path,
        },
        &clock,
    )
    .unwrap()
    .unwrap();
    report.eligible = vec![EligibleProject {
        project_root: analyzed.project_root,
        artifact: analyzed.artifact,
    }];
    let output = report::render(&mut report);
    assert!(output.contains("old-project"));
    assert!(output.contains("1 inactive Cargo project"));
}
