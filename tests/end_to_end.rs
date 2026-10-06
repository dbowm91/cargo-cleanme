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

mod common;

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
        common::backdate_file(&file, old);
    }
    for directory in [
        project.to_path_buf(),
        project.join("src"),
        project.join("target"),
        project.join("target/debug"),
    ] {
        // The workspace directory's own mtime is part of the recency verdict,
        // so this must genuinely apply on every platform (C009).
        common::backdate_file(&directory, old);
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

/// Drive the production pipeline over one policy and return what the report
/// proves: the rendered text, the discovered manifests, and the ownership class
/// of every group that survived the whole chain.
fn run_filtered_scope(
    scope: domain::ScanScope,
    filters: domain::DiscoveryFilters,
) -> (
    String,
    Vec<std::path::PathBuf>,
    Vec<domain::OutputOwnershipClass>,
) {
    let policy = domain::EffectiveScanPolicy {
        recency: Duration::from_secs(300),
        scope,
        discovery_filters: filters,
    };
    let observer = progress::NoopObserver;
    let discovered = discovery::discover_manifests(&policy, &observer).unwrap();
    let mut counters = discovered.counters.clone();
    let mut diagnostics = discovered.diagnostics.clone();
    let workspaces = workspace::resolve_workspaces(
        &discovered.manifests,
        &MetadataOnly,
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    let groups = workspace::build_groups(&workspaces);
    let start = SystemTime::now();
    let physical = workspace::analyze_groups(
        &workspaces,
        groups,
        start,
        start - Duration::from_secs(300),
        Duration::from_secs(300),
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    let ownership = physical.iter().map(|g| g.ownership).collect::<Vec<_>>();
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
    (output, discovered.manifests, ownership)
}

/// C019 §12: correcting the search scope must remove the unintended candidate
/// *earlier*, and must not create a shortcut around any later proof.
///
/// The tree is the canonical case — a literal ignored ancestor with one exact
/// exception — and both projects are complete, inactive, buildable crates. If
/// the correction had weakened anything downstream, the surviving project would
/// come back with a different ownership class or a different eligibility
/// verdict, and the explicit-scope contrast below would stop matching.
#[test]
fn a_literal_ignored_ancestor_yields_one_workspace_while_an_explicit_root_yields_two() {
    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let archive = root.join("archive");
    fs::create_dir_all(&archive).unwrap();
    let keep = common::inactive_project(&archive, "keep");
    let other = common::inactive_project(&archive, "other");
    // Canonical spelling: the walker compares canonical paths, and on macOS the
    // temp root is reached through a symlink.
    let canonical_root = fs::canonicalize(&root).unwrap();
    let canonical_archive = canonical_root.join("archive");

    // 1. Routine scope with the literal ancestor ignored and one exact
    //    exception: the sibling is not a candidate at all.
    let (output, manifests, ownership) = run_filtered_scope(
        domain::ScanScope::Routine(vec![canonical_root.clone()]),
        domain::DiscoveryFilters::Active {
            ignore: vec![canonical_archive.to_string_lossy().into_owned()],
            unignore: vec![canonical_archive.join("keep")],
        },
    );
    assert_eq!(
        manifests,
        vec![canonical_archive.join("keep/Cargo.toml")],
        "the ignored sibling must not become a cleanup candidate"
    );
    assert_eq!(
        ownership,
        vec![domain::OutputOwnershipClass::PrivateBounded]
    );
    assert!(output.contains("keep"), "{output}");
    assert!(
        !output.contains("other"),
        "the sibling is absent from the report, not merely ineligible: {output}"
    );

    // 2. An explicit root still bypasses the user filters entirely, so the
    //    sibling is in scope again — the pre-existing contract, unchanged.
    let (explicit_output, explicit_manifests, explicit_ownership) = run_filtered_scope(
        domain::ScanScope::Explicit(canonical_archive.clone()),
        domain::DiscoveryFilters::Bypassed,
    );
    let mut expected = vec![canonical_archive.join("keep/Cargo.toml")];
    expected.push(canonical_archive.join("other/Cargo.toml"));
    expected.sort();
    assert_eq!(explicit_manifests, expected);
    assert_eq!(
        explicit_ownership,
        vec![
            domain::OutputOwnershipClass::PrivateBounded,
            domain::OutputOwnershipClass::PrivateBounded
        ],
        "an explicit root sees the whole scope, and the surviving project keeps \
         exactly the ownership class it had in the filtered scope: {explicit_output}"
    );
    assert!(explicit_output.contains("keep"), "{explicit_output}");
    assert!(explicit_output.contains("other"), "{explicit_output}");

    // The ignored sibling's bytes are real and untouched: C019 removed the
    // candidate from scope, it did not delete or neutralize anything.
    assert!(other.join("target/debug/app").exists());
    assert!(keep.join("target/debug/app").exists());
}

// ---------------------------------------------------------------------------
// C023 §5/§7/§10: a neighbouring workspace's sources inside a covering output
// root.
//
// 0.2.0 classified such a group `PrivateBounded`, ran a real
// `cargo clean --target-dir outer/out`, and deleted `inner`'s Cargo.toml,
// Cargo.lock, `.cargo/config.toml`, and `src/main.rs` — reporting success and
// exit 0. That was proved against the immutable published binary, in a
// disposable fixture; it is not reproducible here without shipping a
// destructive proof in ordinary CI, so what this suite owns is the property
// that makes it unreachable, exercised through the production cleanup entry
// point.
//
// The runner below resolves workspaces and deletes output the way Cargo does,
// so a defect that let this through would destroy the fixture's own files and
// fail. Nothing is asserted on a label alone.
// ---------------------------------------------------------------------------

/// Cargo resolution plus Cargo's own deletion, driven off real config files.
///
/// `build.target-dir` is honoured per manifest directory exactly as Cargo
/// honours it, so the two workspaces resolve to genuinely distinct output roots.
/// If they did not, they would form one `shared` group and the refusal under
/// test would be the shared-ownership rule instead — the case would pass while
/// measuring the wrong thing, which is the C012 shape this fixture exists to
/// avoid.
struct RealisticCargo {
    clean_calls: std::sync::Mutex<Vec<std::path::PathBuf>>,
}

impl RealisticCargo {
    fn new() -> Self {
        Self {
            clean_calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Cargo reads `.cargo/config.toml` from the manifest's directory upward.
    fn target_dir_for(manifest_dir: &std::path::Path) -> std::path::PathBuf {
        let mut dir = Some(manifest_dir);
        while let Some(current) = dir {
            let config = current.join(".cargo/config.toml");
            if config.is_file() {
                let text = fs::read_to_string(&config).unwrap();
                for line in text.lines() {
                    if let Some(value) = line.trim().strip_prefix("target-dir") {
                        let value = value.trim_start().trim_start_matches('=').trim();
                        let value = value.trim_matches('"');
                        let path = std::path::PathBuf::from(value);
                        return if path.is_absolute() {
                            path
                        } else {
                            current.join(path)
                        };
                    }
                }
            }
            dir = current.parent();
        }
        manifest_dir.join("target")
    }
}

impl workspace::CargoRunner for RealisticCargo {
    fn run(
        &self,
        cwd: &std::path::Path,
        args: &[std::ffi::OsString],
    ) -> std::io::Result<workspace::ProcessOutput> {
        let first = args.first().map(|a| a.to_string_lossy().to_string());
        let manifest = args
            .iter()
            .skip_while(|a| *a != "--manifest-path")
            .nth(1)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| cwd.join("Cargo.toml"));
        let manifest_dir = manifest.parent().unwrap_or(cwd).to_path_buf();
        let json = match first.as_deref() {
            Some("locate-project") => serde_json::json!({
                "root": manifest.to_string_lossy(),
            }),
            Some("metadata") => serde_json::json!({
                "packages": [{
                    "id": format!("path+file://{}#0.1.0", manifest_dir.display()),
                    "name": manifest_dir.file_name().unwrap().to_string_lossy(),
                    "version": "0.1.0",
                    "manifest_path": manifest.to_string_lossy(),
                }],
                "workspace_members": [format!("path+file://{}#0.1.0", manifest_dir.display())],
                "workspace_root": manifest_dir.to_string_lossy(),
                "target_directory": RealisticCargo::target_dir_for(&manifest_dir).to_string_lossy(),
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

impl cargo_cleanme::cleanup::CleanupRunner for RealisticCargo {
    fn run(
        &self,
        cwd: &std::path::Path,
        args: &[std::ffi::OsString],
    ) -> std::io::Result<cargo_cleanme::cleanup::ProcessOutput> {
        // `clean` deletes the way Cargo deletes. This is what makes the case
        // able to fail destructively: if classification ever regressed, this
        // runner would take the neighbour's sources with it and the byte
        // comparison below would fail with real damage, exactly as 0.2.0 did.
        if args.first().map(|a| a.to_string_lossy()) == Some("clean".into()) {
            let target = args
                .iter()
                .skip_while(|a| *a != "--target-dir")
                .nth(1)
                .map(std::path::PathBuf::from);
            if let Some(target) = &target {
                self.clean_calls.lock().unwrap().push(target.clone());
                if target.exists() {
                    fs::remove_dir_all(target).unwrap();
                }
            }
            return Ok(cargo_cleanme::cleanup::ProcessOutput {
                success: true,
                code: Some(0),
                stdout: b"Removed 1 file, done.\n".to_vec(),
                stderr: Vec::new(),
            });
        }
        <Self as workspace::CargoRunner>::run(self, cwd, args).map(|out| {
            cargo_cleanme::cleanup::ProcessOutput {
                success: out.success,
                code: out.code,
                stdout: out.stdout,
                stderr: out.stderr,
            }
        })
    }

    fn run_with_env(
        &self,
        cwd: &std::path::Path,
        args: &[std::ffi::OsString],
        _env: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> std::io::Result<cargo_cleanme::cleanup::ProcessOutput> {
        self.run(cwd, args)
    }
}

fn write_crate(root: &std::path::Path, name: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
}

fn set_target_dir(project: &std::path::Path, target: &std::path::Path) {
    fs::create_dir_all(project.join(".cargo")).unwrap();
    let value = toml::Value::String(target.to_string_lossy().replace('\\', "\\\\"));
    fs::write(
        project.join(".cargo/config.toml"),
        format!("[build]\ntarget-dir = {value}\n"),
    )
    .unwrap();
}

fn backdate_tree(root: &std::path::Path) {
    let old = SystemTime::now() - Duration::from_secs(3600);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                common::backdate_file(&path, old);
            }
        }
        common::backdate_file(&dir, old);
    }
}

/// `(relative path, contents)` for every regular file under `root`.
fn fingerprint(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push((
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    walk(root, root, &mut out);
    out
}

/// `outer`'s declared output root contains `inner`'s sources. `inner` must
/// survive, byte-identical, and no Cargo clean may be spawned for `outer`.
#[test]
fn a_neighbouring_projects_sources_inside_a_covering_output_root_are_never_cleaned() {
    use cargo_cleanme::cleanup::{CleanMode, CleanOutcome, CleanupPolicy};

    let temp = tempdir().unwrap();
    let case = temp.path();
    let outer = case.join("outer");
    let out = outer.join("out");
    let inner = out.join("inner");
    let inner_target = case.join("inner-target");

    write_crate(&outer, "outer");
    set_target_dir(&outer, &out);
    write_crate(&inner, "inner");
    set_target_dir(&inner, &inner_target);
    fs::write(inner.join("Cargo.lock"), "version = 3\n").unwrap();

    fs::create_dir_all(out.join("debug")).unwrap();
    fs::write(
        out.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(out.join("debug/outer"), vec![0u8; 8192]).unwrap();
    fs::create_dir_all(&inner_target).unwrap();

    backdate_tree(&outer);
    backdate_tree(&inner_target);

    let before = fingerprint(&inner);
    assert_eq!(
        before.len(),
        4,
        "premise: the neighbour holds the four files whose deletion was the \
         observed 0.2.0 defect (Cargo.toml, Cargo.lock, src/main.rs, and \
         .cargo/config.toml): {before:?}"
    );

    let runner = RealisticCargo::new();
    let report = cargo_cleanme::cleanup::clean_with_roots_policy_selector(
        &[case.to_path_buf()],
        300,
        // Authorization is granted deliberately: `allowed_output_roots`
        // authorizes, it never manufactures ownership proof, so this fixture
        // cannot pass because the region was unauthorized.
        std::slice::from_ref(&out),
        CleanMode::Execute,
        &runner,
        &progress::NoopObserver,
        &CleanupPolicy {
            min_reclaimable_bytes: 0,
            min_inactive_seconds: None,
            include: Vec::new(),
            exclude: Vec::new(),
        },
        None,
    )
    .unwrap();

    // The neighbour survived byte-for-byte.
    assert_eq!(
        fingerprint(&inner),
        before,
        "C023: a neighbouring project's sources inside a cleaned region must be \
         byte-identical afterwards"
    );

    let rendered = report.render();
    assert!(
        !report
            .results
            .iter()
            .any(|unit| unit.outcome == CleanOutcome::Cleaned),
        "no unit may be reported cleaned while it contains a neighbour's sources: {rendered}"
    );
    assert!(
        !rendered.contains("Cleaned"),
        "nothing may be reported as cleaned for this scope: {rendered}"
    );
    assert!(
        !runner.clean_calls.lock().unwrap().iter().any(|c| c == &out),
        "no cargo clean may name the dangerous region"
    );

    // Two independent layers can refuse this region, so the case asserts the
    // invariant rather than one particular route: the classifier drops the group
    // with a diagnostic, and the final preflight refuses a group that slipped
    // through. Both must leave the same user-visible truth -- no clean, no bytes
    // gone -- so the assertions here are deliberately route-agnostic.
    assert!(
        report.diagnostics >= 1 || rendered.contains("Skipped"),
        "refusing the region must leave a trace, not silence: {rendered}"
    );

    // Layer 1, pinned directly: classification may not emit `PrivateBounded`
    // for a covering root holding a resolved source tree. Asserting it
    // separately matters because the preflight would otherwise mask a
    // classifier regression behind a still-safe outcome -- a green test that no
    // longer tests what its name says.
    let scan = run_scan(case, &workspace::SystemCargoRunner);
    assert!(
        !scan.groups.iter().any(|group| {
            group.ownership == domain::OutputOwnershipClass::PrivateBounded
                && group.display_path.ends_with("outer/out")
        }),
        "C023: the classifier must never emit PrivateBounded for a covering \
         root containing a resolved source tree"
    );
    // And the diagnostic must name source containment, not an unrelated reason.
    // `scan` is the documented qualification surface; the cleanup envelope
    // carries the same diagnostic only as a count.
    assert!(
        scan.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("contains a workspace source tree")),
        "the diagnostic must name source containment rather than an unrelated \
         reason: {:?}",
        scan.diagnostics
    );
}

/// Drive the production scan pipeline over one explicit root with the given
/// runner, exactly as `discover_manifests → resolve_workspaces → build_groups →
/// analyze_groups` does for the binary.
fn run_scan(root: &std::path::Path, runner: &dyn workspace::CargoRunner) -> domain::ScanReport {
    let policy = domain::EffectiveScanPolicy {
        recency: Duration::from_secs(300),
        scope: domain::ScanScope::Explicit(root.to_path_buf()),
        discovery_filters: domain::DiscoveryFilters::Bypassed,
    };
    let observer = progress::NoopObserver;
    let discovered = discovery::discover_manifests(&policy, &observer).unwrap();
    let mut counters = discovered.counters.clone();
    let mut diagnostics = discovered.diagnostics.clone();
    let workspaces = workspace::resolve_workspaces(
        &discovered.manifests,
        runner,
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    let groups = workspace::build_groups(&workspaces);
    let start = SystemTime::now();
    let physical = workspace::analyze_groups(
        &workspaces,
        groups,
        start,
        start - Duration::from_secs(300),
        Duration::from_secs(300),
        &mut counters,
        &mut diagnostics,
        &observer,
    );
    let groups = physical
        .into_iter()
        .map(|group| domain::EligibleOutputGroup {
            display_path: group.display_path,
            workspace_roots: group.owners.iter().map(|w| w.0.clone()).collect(),
            physical_paths: group.physical_paths,
            bytes: group.bytes,
            metric: group.metric,
            newest_mtime: group.newest_mtime,
            artifact_entries: group.artifact_entries,
            ownership: group.ownership,
        })
        .collect();
    domain::ScanReport {
        groups,
        diagnostics,
        discovered: discovered.manifests.len() as u64,
        visited_entries: discovered.visited_entries,
        counters,
    }
}

/// An unrelated sibling workspace next to the dangerous one is still eligible.
///
/// Without this the previous case would pass on a fixture where *everything* is
/// refused for an unrelated reason. `PrivateBounded` must remain reachable.
#[test]
fn an_unrelated_sibling_workspace_stays_eligible_alongside_a_dangerous_one() {
    use cargo_cleanme::cleanup::{CleanMode, CleanOutcome, CleanupPolicy};

    let temp = tempdir().unwrap();
    let case = temp.path();

    // A plain, private, inactive project: the ordinary happy path.
    let sibling = common::inactive_project(case, "sibling");
    let sibling_target = sibling.join("target");

    // And the dangerous pair from the previous case, side by side.
    let outer = case.join("outer");
    let out = outer.join("out");
    let inner = out.join("inner");
    let inner_target = case.join("inner-target");
    write_crate(&outer, "outer");
    set_target_dir(&outer, &out);
    write_crate(&inner, "inner");
    set_target_dir(&inner, &inner_target);
    fs::create_dir_all(out.join("debug")).unwrap();
    fs::write(
        out.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(out.join("debug/outer"), vec![0u8; 8192]).unwrap();
    fs::create_dir_all(&inner_target).unwrap();
    backdate_tree(&outer);
    backdate_tree(&inner_target);

    let runner = RealisticCargo::new();
    let report = cargo_cleanme::cleanup::clean_with_roots_policy_selector(
        &[case.to_path_buf()],
        300,
        &[out.clone(), sibling_target.clone()],
        CleanMode::Simulate,
        &runner,
        &progress::NoopObserver,
        &CleanupPolicy {
            min_reclaimable_bytes: 0,
            min_inactive_seconds: None,
            include: Vec::new(),
            exclude: Vec::new(),
        },
        None,
    )
    .unwrap();

    assert!(
        report.results.iter().any(|unit| {
            unit.output_roots.iter().any(|root| root == &sibling_target)
                && unit.outcome == CleanOutcome::Simulated
        }),
        "the unrelated sibling must remain an eligible candidate: {}",
        report.render()
    );
    assert!(
        !report.results.iter().any(|unit| {
            unit.output_roots.iter().any(|root| root == &out)
                && unit.outcome == CleanOutcome::Simulated
        }),
        "the dangerous region must still be refused: {}",
        report.render()
    );
}
