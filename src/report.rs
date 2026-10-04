use crate::domain::*;
use std::path::Path;

/// Render a path for human stdout.
///
/// A hostile directory name must not be able to drive the terminal, so control
/// characters are escaped instead of written raw. Non-UTF-8 bytes degrade to
/// U+FFFD here; only the machine-readable projection keeps the lossy form
/// stable, and serde escapes it a second time.
fn path_text(p: &Path) -> String {
    let text = p.to_string_lossy();
    if !text.chars().any(char::is_control) {
        return text.into_owned();
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if c.is_control() {
            escaped.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            escaped.push(c);
        }
    }
    escaped
}
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    // Scaled in integer arithmetic: `n as f64` silently loses bytes above
    // 2^53, so the largest units could not be reported accurately at all.
    let mut hundredths = u128::from(n) * 100;
    let mut unit = 0usize;
    while unit + 1 < UNITS.len() && hundredths >= 1024 * 100 {
        hundredths = (hundredths + 512) / 1024;
        unit += 1;
    }
    // A value that rounds up to exactly 1024.00 must change unit, not print
    // "1024.00 KiB".
    if unit + 1 < UNITS.len() && hundredths >= 1024 * 100 {
        hundredths /= 1024;
        unit += 1;
    }
    let number = format!("{}.{:02}", hundredths / 100, hundredths % 100);
    format!("{number:>6} {}", UNITS[unit])
}
pub fn render(report: &mut ScanReport) -> String {
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
    // One phrasing for every state: the total is an inventory estimate of
    // inactive artifact groups, never bytes already recovered.
    o.push_str(&format!(
        "\n{} inventory estimate across {} inactive Cargo output groups\n",
        format_bytes(total),
        report.groups.len()
    ));
    o
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn group(path: &str, bytes: u64) -> EligibleOutputGroup {
        EligibleOutputGroup {
            display_path: PathBuf::from(path),
            workspace_roots: vec![PathBuf::from("/ws")],
            physical_paths: vec![PathBuf::from(path)],
            bytes,
            metric: SizeMetric::Allocated,
            newest_mtime: None,
            artifact_entries: 1,
            ownership: OutputOwnershipClass::PrivateBounded,
        }
    }
    #[cfg(unix)]
    #[test]
    fn report_escapes_control_bytes_in_paths() {
        let mut r = ScanReport::default();
        // A valid-UTF-8 name carrying terminal control bytes.
        r.groups.push(group("/tmp/esc\u{1b}[2J-bell\u{7}", 1));
        let text = render(&mut r);
        // Escaped, never written raw: a terminal cannot be driven by the name.
        assert!(text.contains("\\u{1b}"));
        assert!(text.contains("\\u{7}"));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
    }
    #[cfg(unix)]
    #[test]
    fn report_escapes_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let mut r = ScanReport::default();
        let p = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/bad-\xff".to_vec()));
        r.groups.push(EligibleOutputGroup {
            display_path: p,
            workspace_roots: vec![PathBuf::from("/ws")],
            physical_paths: vec![PathBuf::from("/ws/target")],
            bytes: 1,
            metric: SizeMetric::Allocated,
            newest_mtime: None,
            artifact_entries: 1,
            ownership: OutputOwnershipClass::PrivateBounded,
        });
        let text = render(&mut r);
        // Only the report's own newlines survive; the path contributes nothing
        // that a terminal would interpret.
        assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
        assert!(text.contains('\u{fffd}'));
    }
    #[test]
    fn report_ordering_and_zero_report() {
        let mut r = ScanReport::default();
        assert!(render(&mut r).contains("0.00 B inventory estimate"));
        r.groups.push(group("/z", 3));
        r.groups.push(group("/a", 3));
        let text = render(&mut r);
        assert!(text.find("/a").unwrap() < text.find("/z").unwrap());
    }
    #[test]
    fn zero_state_uses_the_same_inventory_wording_as_results() {
        let empty = render(&mut ScanReport::default());
        let mut r = ScanReport::default();
        r.groups.push(group("/a", 10));
        let with_results = render(&mut r);
        assert!(with_results.contains("inventory estimate"));
        assert!(empty.contains("inventory estimate"));
        // The zero state must never claim bytes were reclaimed.
        assert!(!empty.contains("reclaimable"));
    }
    #[test]
    fn format_bytes_scales_to_eib_and_promotes_rounded_units() {
        assert_eq!(format_bytes(0).trim(), "0.00 B");
        assert_eq!(format_bytes(500).trim(), "500.00 B");
        assert_eq!(format_bytes(1024).trim(), "1.00 KiB");
        // Rounds up to 1024.00 KiB: must become 1.00 MiB instead.
        assert_eq!(format_bytes(1_048_575).trim(), "1.00 MiB");
        assert_eq!(format_bytes(1024u64.pow(6)).trim(), "1.00 EiB");
        // Exact above 2^53, where an f64 mantissa would drop bytes.
        assert_eq!(format_bytes(u64::MAX).trim(), "16.00 EiB");
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
                ownership,
                ..group(path, bytes)
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
            physical_paths: vec![PathBuf::from("/ws/target"), PathBuf::from("/ws/target")],
            artifact_entries: 2,
            ..group("/ws/target", 500)
        });
        let text = render(&mut r);
        assert!(text.contains("500"));
        assert!(text.contains("1 inactive Cargo output group"));
    }
}
