//! End-to-end coverage of the *production* scan pipeline.
//!
//! This test deliberately drives the same sequence the binary runs —
//! `policy::resolve` → `discovery::discover_manifests` →
//! `workspace::resolve_workspaces` → `workspace::build_groups` →
//! `workspace::analyze_groups` → `report::render` — so it can fail when
//! production is broken. (It previously drove a legacy `discovery::scan` →
//! `analysis::analyze` path the binary never executes, which is why a dead
//! implementation could stay green for so long.)
use cargo_cleanme::{config::ScanConfig, discovery, domain, policy, progress, report, workspace};
use std::{
    fs,
    time::{Duration, SystemTime},
};
use tempfile::tempdir;

/// One inactive project plus one that was just built.
fn fixture(temp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let old = temp.join("old-project");
    fs::create_dir_all(old.join("src")).unwrap();
    fs::create_dir_all(old.join("target/debug")).unwrap();
    fs::write(
        old.join("Cargo.toml"),
        "[package]\nname='old-project'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(old.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        old.join("target/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(old.join("target/debug/app"), vec![0u8; 8192]).unwrap();

    let fresh = temp.join("fresh-project");
    fs::create_dir_all(fresh.join("src")).unwrap();
    fs::create_dir_all(fresh.join("target/debug")).unwrap();
    fs::write(
        fresh.join("Cargo.toml"),
        "[package]\nname='fresh-project'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(fresh.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(fresh.join("target/debug/app"), vec![0u8; 8192]).unwrap();
    (old, fresh)
}

fn backdate(project: &std::path::Path) {
    let old = SystemTime::now() - Duration::from_secs(3600);
    for file in [
        project.join("Cargo.toml"),
        project.join("src/main.rs"),
        project.join("target/CACHEDIR.TAG"),
        project.join("target/debug/app"),
    ] {
        fs::OpenOptions::new()
            .write(true)
            .open(file)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    for directory in [
        project.to_path_buf(),
        project.join("src"),
        project.join("target"),
        project.join("target/debug"),
    ] {
        fs::File::open(directory)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
}

/// Minimal Cargo stand-in: resolution must never need the network or a build.
struct MetadataOnly;

impl workspace::CargoRunner for MetadataOnly {
    fn run(
        &self,
        cwd: &std::path::Path,
        args: &[std::ffi::OsString],
    ) -> std::io::Result<workspace::ProcessOutput> {
        let root = cwd.join("Cargo.toml");
        let json = match args.first().map(|a| a.to_string_lossy()) {
            Some(ref first) if first == "locate-project" => {
                serde_json::json!({ "root": root.to_string_lossy() })
            }
            Some(ref first) if first == "metadata" => serde_json::json!({
                "packages": [{
                    "id": "path+file:///fixture#0.1.0",
                    "name": "fixture",
                    "version": "0.1.0",
                    "manifest_path": root.to_string_lossy(),
                }],
                "workspace_members": ["path+file:///fixture#0.1.0"],
                "workspace_root": cwd.to_string_lossy(),
                "target_directory": cwd.join("target").to_string_lossy(),
            }),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unexpected cargo invocation: {args:?}"),
                ));
            }
        };
        Ok(workspace::ProcessOutput {
            success: true,
            code: Some(0),
            stdout: serde_json::to_vec(&json).unwrap(),
            stderr: Vec::new(),
        })
    }
}

#[test]
fn end_to_end_reports_only_inactive_artifact_projects() {
    let temp = tempdir().unwrap();
    let (old, _fresh) = fixture(temp.path());
    backdate(&old);

    // 1. Policy resolution, exactly as the CLI does.
    let config = ScanConfig {
        root: Some(temp.path().to_path_buf()),
        ..Default::default()
    };
    let policy = policy::resolve(
        domain::ScanRequest {
            cli_root: None,
            full: false,
        },
        &config,
    )
    .unwrap();
    let observer = progress::NoopObserver;

    // 2. Manifest discovery.
    let discovered = discovery::discover_manifests(&policy, &observer).unwrap();
    assert!(
        discovered.manifests.contains(&old.join("Cargo.toml")),
        "production discovery must find the manifest: {:?}",
        discovered.manifests
    );

    // 3. Workspace resolution + 4. physical grouping.
    let mut counters = discovered.counters.clone();
    let mut diagnostics = discovered.diagnostics.clone();
    let workspaces = workspace::resolve_workspaces(
        &discovered.manifests,
        &MetadataOnly,
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    assert_eq!(workspaces.len(), 2, "both real Cargo workspaces resolve");
    let groups = workspace::build_groups(&workspaces);
    assert_eq!(groups.len(), 2, "one physical group per target dir");
    assert!(
        groups
            .iter()
            .all(|g| { g.ownership == domain::OutputOwnershipClass::PrivateBounded })
    );

    // 5. Activity/sizing analysis, then 6. the report the user sees.
    let start = SystemTime::now();
    let cutoff = start - Duration::from_secs(config.recency_seconds);
    let physical = workspace::analyze_groups(
        &workspaces,
        groups,
        start,
        cutoff,
        Duration::from_secs(config.recency_seconds),
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    let mut report = domain::ScanReport {
        groups: physical
            .into_iter()
            .map(|g| domain::EligibleOutputGroup {
                display_path: g.display_path,
                workspace_roots: g.owners.iter().map(|w| w.0.clone()).collect(),
                physical_paths: g.physical_paths,
                bytes: g.bytes,
                metric: g.metric,
                newest_mtime: g.newest_mtime,
                artifact_entries: g.artifact_entries,
                ownership: g.ownership,
            })
            .collect(),
        diagnostics,
        discovered: discovered.manifests.len() as u64,
        visited_entries: discovered.visited_entries,
        counters,
    };
    let output = report::render(&mut report);
    // Only the backdated project is an inactive candidate: the freshly built
    // one is protected by the recency guard, through production code.
    assert!(output.contains("old-project"), "{output}");
    assert!(!output.contains("fresh-project"), "{output}");
    assert!(output.contains("1 inactive Cargo output group"), "{output}");
    assert!(output.contains("inventory estimate"), "{output}");

    // The machine projection agrees with the human one.
    let json = serde_json::to_value(cargo_cleanme::output::scan(&report, "routine")).unwrap();
    assert_eq!(json["result"]["groups"][0]["ownership"], "private");
    assert_eq!(json["result"]["summary"]["group_count"], 1);
    let bytes = json["result"]["groups"][0]["bytes"].as_u64().unwrap();
    // The metric is allocated size (block-rounded), so assert the lower bound and
    // exact agreement with the human view rather than a filesystem-dependent
    // number.
    assert!(bytes >= 8192, "the artifact is sized, not skipped: {bytes}");
    assert!(output.contains(report::format_bytes(bytes).trim()));
}
