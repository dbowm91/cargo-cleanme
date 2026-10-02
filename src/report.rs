use crate::domain::*;
use std::path::Path;
fn path_text(p: &Path) -> String {
    p.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{:?}", p.as_os_str()))
}
fn human(n: u64) -> String {
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
            human(p.artifact.bytes),
            path_text(&p.project_root)
        ));
    }
    let count = report.eligible.len();
    let noun = if count == 1 { "project" } else { "projects" };
    o.push_str(&format!(
        "\n{} reclaimable across {count} inactive Cargo {noun}\n",
        human(total)
    ));
    o
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
