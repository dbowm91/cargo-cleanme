use crate::domain::*;
use std::path::Path;
fn path_text(p: &Path) -> String {
    p.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{:?}", p.as_os_str()))
}
pub fn format_bytes(n: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:>6.2} {}", units[i])
}
pub fn render(report: &mut ScanReport) -> String {
    if !report.groups.is_empty() {
        return render_groups(report);
    }
    report.eligible.sort_by(|a, b| {
        b.artifact
            .bytes
            .cmp(&a.artifact.bytes)
            .then_with(|| a.project_root.cmp(&b.project_root))
    });
    let total = report
        .eligible
        .iter()
        .fold(0u64, |s, p| s.saturating_add(p.artifact.bytes));
    let mut o = String::new();
    for p in &report.eligible {
        o.push_str(&format!(
            "{}  {}\n",
            format_bytes(p.artifact.bytes),
            path_text(&p.project_root)
        ));
    }
    let count = report.eligible.len();
    let noun = if count == 1 { "project" } else { "projects" };
    o.push_str(&format!(
        "\n{} reclaimable across {count} inactive Cargo {noun}\n",
        format_bytes(total)
    ));
    o
}

fn render_groups(report: &mut ScanReport) -> String {
    report.groups.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.display_path.cmp(&b.display_path))
    });
    let total = report
        .groups
        .iter()
        .fold(0u64, |s, g| s.saturating_add(g.bytes));
    let mut o = String::new();
    for g in &report.groups {
        o.push_str(&format!(
            "{}  {}  [{}]\n",
            format_bytes(g.bytes),
            path_text(&g.display_path),
            g.ownership.label(),
        ));
    }
    let count = report.groups.len();
    let noun = if count == 1 {
        "output group"
    } else {
        "output groups"
    };
    o.push_str(&format!(
        "\n{} inventory estimate across {count} inactive Cargo {noun}\n",
        format_bytes(total)
    ));
    o
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[cfg(unix)]
    #[test]
    fn report_escapes_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let mut r = ScanReport::default();
        let p = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/bad-\xff".to_vec()));
        r.eligible.push(EligibleProject {
            project_root: p.clone(),
            artifact: ArtifactAnalysis {
                target_path: p.join("target"),
                bytes: 1,
                metric: SizeMetric::Allocated,
                newest_mtime: None,
                artifact_entries: 1,
            },
        });
        assert!(render(&mut r).contains("\\x"));
    }
    #[test]
    fn report_ordering_and_zero_report() {
        let mut r = ScanReport::default();
        assert!(render(&mut r).contains("0.00 B reclaimable"));
        r.eligible.push(EligibleProject {
            project_root: Path::new("/z").into(),
            artifact: ArtifactAnalysis {
                target_path: Path::new("/z/target").into(),
                bytes: 3,
                metric: SizeMetric::Apparent,
                newest_mtime: None,
                artifact_entries: 1,
            },
        });
        r.eligible.push(EligibleProject {
            project_root: Path::new("/a").into(),
            artifact: ArtifactAnalysis {
                target_path: Path::new("/a/target").into(),
                bytes: 3,
                metric: SizeMetric::Apparent,
                newest_mtime: None,
                artifact_entries: 1,
            },
        });
        let text = render(&mut r);
        assert!(text.find("/a").unwrap() < text.find("/z").unwrap());
    }
    #[test]
    fn groups_render_every_group_with_ownership_and_deduped_total() {
        let mut r = ScanReport::default();
        for (path, bytes, ownership) in [
            ("/z/shared", 300u64, OutputOwnershipClass::Shared),
            ("/a/private", 300u64, OutputOwnershipClass::PrivateBounded),
            (
                "/e/external",
                100u64,
                OutputOwnershipClass::ExternalUnproven,
            ),
        ] {
            r.groups.push(EligibleOutputGroup {
                display_path: PathBuf::from(path),
                workspace_roots: vec![PathBuf::from("/ws")],
                physical_paths: vec![PathBuf::from(path)],
                bytes,
                metric: SizeMetric::Allocated,
                newest_mtime: None,
                artifact_entries: 1,
                ownership,
            });
        }
        let text = render(&mut r);
        // Deterministic size-desc with path tie-break: /a before /z at equal size.
        assert!(text.find("/a/private").unwrap() < text.find("/z/shared").unwrap());
        // Every group printed, not only transient top rows.
        assert!(text.contains("/e/external"));
        assert!(text.contains("[private]"));
        assert!(text.contains("[shared]"));
        assert!(text.contains("[external-unproven]"));
        // Total is deduplicated sum, labeled inventory estimate, never recovered.
        assert!(text.contains("inventory estimate"));
        assert!(!text.contains("recovered"));
    }
    #[test]
    fn groups_total_does_not_double_count_equal_roots() {
        let mut r = ScanReport::default();
        r.groups.push(EligibleOutputGroup {
            display_path: PathBuf::from("/ws/target"),
            workspace_roots: vec![PathBuf::from("/ws")],
            physical_paths: vec![PathBuf::from("/ws/target"), PathBuf::from("/ws/target")],
            bytes: 500,
            metric: SizeMetric::Allocated,
            newest_mtime: None,
            artifact_entries: 2,
            ownership: OutputOwnershipClass::PrivateBounded,
        });
        let text = render(&mut r);
        assert!(text.contains("500"));
        assert!(text.contains("1 inactive Cargo output group"));
    }
}
