use crate::domain::*;
use std::path::{Path, PathBuf};

/// The human surface's rendering options (M020).
///
/// Two decisions travel together here: **which shape** the report takes, and
/// **how paths are spelled**.
///
/// - **Quiet** names only what the run acted on. It is the default, because a
///   machine with a thousand Cargo workspaces would otherwise print a thousand
///   lines saying "skipped".
/// - **Verbose** names everything: every workspace considered, the scope
///   census, the counters, the diagnostic detail.
///
/// The path base is a property of the **scope**, not of the verbosity: a
/// bounded scope has a root to be relative to and a Full scope does not. The
/// verbose surface always spells paths absolutely — it is the debugging
/// surface, and an absolute path is unambiguous.
#[derive(Clone, Debug, Default)]
pub struct HumanOptions {
    /// Scope roots. A path under one of them is shown relative to it; a path
    /// under none of them stays absolute. Empty means "always absolute".
    pub roots: Vec<PathBuf>,
    /// Show the detailed surface.
    pub verbose: bool,
}

impl HumanOptions {
    /// The quiet surface of a scope.
    ///
    /// `roots` are the scope's roots. Pass an empty list for a Full scope:
    /// there is no single root to be relative to, and a relative path under
    /// `/` would be a lie.
    pub fn quiet(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            verbose: false,
        }
    }

    /// The verbose surface. Paths are absolute.
    pub fn verbose() -> Self {
        Self {
            roots: Vec::new(),
            verbose: true,
        }
    }

    /// The path as the human surface spells it.
    pub fn path(&self, p: &Path) -> String {
        path_text(&self.relativized(p))
    }

    /// The path relative to the scope root that contains it.
    ///
    /// A path no scope root contains stays absolute: a relative path with no
    /// root to be relative to is a guess.
    ///
    /// A result of **one** component is qualified with the scope root's own
    /// name — `~/projects/my-app` + `target` reads as `my-app/target`, not as
    /// a bare `target`. Two reasons, and both are about the operator being
    /// able to tell the rows apart: a bare `target` says nothing about
    /// *which* project it belongs to, and two scope roots that each hold a
    /// `target` child would produce two indistinguishable rows.
    pub fn relativized(&self, p: &Path) -> PathBuf {
        for root in &self.roots {
            if let Ok(rest) = p.strip_prefix(root) {
                let components = rest.components().count();
                if components >= 2 {
                    return rest.to_path_buf();
                }
                let Some(name) = root.file_name() else {
                    // The scope root is the filesystem root: it has no name to
                    // qualify the path with.
                    return rest.to_path_buf();
                };
                if components == 0 {
                    // The path *is* the scope root.
                    return PathBuf::from(name);
                }
                return PathBuf::from(name).join(rest);
            }
        }
        p.to_path_buf()
    }
}

/// Render a path for human stdout.
///
/// A hostile directory name must not be able to drive the terminal, so control
/// characters are escaped instead of written raw. Non-UTF-8 bytes degrade to
/// U+FFFD here; only the machine-readable projection keeps the lossy form
/// stable, and serde escapes it a second time.
fn path_text(p: &Path) -> String {
    let text = p.to_string_lossy();
    escape_path_text(&text).into_owned()
}

/// Escape the control characters in a path (or any) text for a human surface.
///
/// Every human surface routes path text through here, so a directory name can
/// never carry a raw newline, carriage return or escape byte into a terminal.
/// The escaped form is printable, which is also what the frame geometry
/// assumes: a frame line is one physical row, and a raw newline inside one
/// would paint a row the frame never counted.
///
/// Public because the binary's stderr surfaces are human surfaces too, and
/// they are a separate crate from this module.
pub fn escape_path_text(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if c.is_control() {
            escaped.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            escaped.push(c);
        }
    }
    std::borrow::Cow::Owned(escaped)
}
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    // Scaled in integer arithmetic: `n as f64` silently loses bytes above
    // 2^53, so the largest units could not be reported accurately at all.
    //
    // The loop's own re-check is what promotes a value that rounds up to
    // exactly 1024.00: the body rounds to the next unit's hundredths, and the
    // condition is re-evaluated, so "1024.00 KiB" can never be printed. There
    // is no second pass for it — one that could not run, because the loop
    // exits either with the largest unit reached (no unit left to promote
    // into) or with the value already below the threshold.
    let mut hundredths = u128::from(n) * 100;
    let mut unit = 0usize;
    while unit + 1 < UNITS.len() && hundredths >= 1024 * 100 {
        hundredths = (hundredths + 512) / 1024;
        unit += 1;
    }
    let number = format!("{}.{:02}", hundredths / 100, hundredths % 100);
    format!("{number:>6} {}", UNITS[unit])
}
/// The human scan report.
///
/// One line per eligible output group, then one tally. The group list *is*
/// the answer a scan gives — the groups are what a cleanup would pick — so
/// the quiet surface keeps every group, with its ownership class, and only
/// shortens the paths.
pub fn render(report: &mut ScanReport, options: &HumanOptions) -> String {
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
    // Each path is rendered once and kept: the column width and the row need
    // the same text, and rendering it twice doubled the path work — a
    // relativization pass and a control-character scan per group — for no gain.
    let rendered: Vec<String> = report
        .groups
        .iter()
        .map(|g| options.path(&g.display_path))
        .collect();
    // One column width for the whole table, so the sizes line up.
    let width = rendered
        .iter()
        .map(|path| path.chars().count())
        .max()
        .unwrap_or(0);
    for (g, path) in report.groups.iter().zip(rendered.iter()) {
        o.push_str(&format!(
            "{:<width$}  {}  [{}]\n",
            path,
            format_bytes(g.bytes).trim(),
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
        let text = render(&mut r, &HumanOptions::default());
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
        let text = render(&mut r, &HumanOptions::default());
        // Only the report's own newlines survive; the path contributes nothing
        // that a terminal would interpret.
        assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
        assert!(text.contains('\u{fffd}'));
    }
    #[test]
    fn report_ordering_and_zero_report() {
        let mut r = ScanReport::default();
        assert!(render(&mut r, &HumanOptions::default()).contains("0.00 B inventory estimate"));
        r.groups.push(group("/z", 3));
        r.groups.push(group("/a", 3));
        let text = render(&mut r, &HumanOptions::default());
        assert!(text.find("/a").unwrap() < text.find("/z").unwrap());
    }
    #[test]
    fn zero_state_uses_the_same_inventory_wording_as_results() {
        let empty = render(&mut ScanReport::default(), &HumanOptions::default());
        let mut r = ScanReport::default();
        r.groups.push(group("/a", 10));
        let with_results = render(&mut r, &HumanOptions::default());
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
        let text = render(&mut r, &HumanOptions::default());
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
        let text = render(&mut r, &HumanOptions::default());
        assert!(text.contains("500"));
        assert!(text.contains("1 inactive Cargo output group"));
    }

    /// A bounded scope spells its paths relative to the scope root.
    ///
    /// *Fails if:* the relativization never happens, or if it happens for a
    /// path no root contains.
    #[test]
    fn a_bounded_scope_spells_its_paths_relative_to_the_scope_root() {
        let mut r = ScanReport::default();
        r.groups.push(group("/work/alpha/target", 10));
        r.groups.push(group("/elsewhere/beta/target", 20));
        let text = render(&mut r, &HumanOptions::quiet(vec![PathBuf::from("/work")]));
        assert!(
            text.contains("alpha/target"),
            "a path under the scope root is relative: {text}"
        );
        assert!(
            text.contains("/elsewhere/beta/target"),
            "a path under no scope root stays absolute: {text}"
        );
        assert!(
            !text.contains("/work/alpha/target"),
            "the relative path must not keep the root prefix: {text}"
        );
    }

    /// A Full scope has no root to be relative to, so its paths stay absolute.
    ///
    /// *Fails if:* the Full scope's paths are relativized against `/`, which
    /// would turn every path into a fragment and lose the machine it names.
    #[test]
    fn a_full_scope_spells_its_paths_absolutely() {
        let mut r = ScanReport::default();
        r.groups.push(group("/home/user/alpha/target", 10));
        let text = render(&mut r, &HumanOptions::quiet(Vec::new()));
        assert!(
            text.contains("/home/user/alpha/target"),
            "the Full scope's paths are absolute: {text}"
        );
    }

    /// A one-component path is qualified with the scope root's own name.
    ///
    /// `~/projects/my-app` + `target` must read as `my-app/target`: a bare
    /// `target` says nothing about which project it belongs to, and two scope
    /// roots that each hold a `target` child would produce two
    /// indistinguishable rows. *Fails if:* the qualification never happens.
    #[test]
    fn a_one_component_path_is_qualified_with_the_scope_root_name() {
        let mut r = ScanReport::default();
        r.groups.push(group("/projects/my-app/target", 10));
        let text = render(
            &mut r,
            &HumanOptions::quiet(vec![PathBuf::from("/projects/my-app")]),
        );
        // The renderer joins with the platform's separator, so the expected
        // spelling is built the same way rather than written out: a literal
        // "my-app/target" passes on Linux and fails on Windows, where the
        // same qualification reads "my-app\target".
        let qualified = PathBuf::from("my-app").join("target");
        assert!(
            text.contains(qualified.to_string_lossy().as_ref()),
            "{text}"
        );
        assert!(!text.contains("/projects/my-app/target"), "{text}");
    }

    /// The scope root itself is spelled with the root's own name.
    ///
    /// *Fails if:* stripping the root leaves an empty path, which renders as
    /// an empty column rather than the directory it names.
    #[test]
    fn the_scope_root_itself_is_spelled_with_its_own_name() {
        let mut r = ScanReport::default();
        r.groups.push(group("/work", 10));
        let text = render(&mut r, &HumanOptions::quiet(vec![PathBuf::from("/work")]));
        assert!(text.contains("work"), "the scope root names itself: {text}");
        assert!(
            !text.contains("  /work"),
            "the scope root is not spelled absolutely: {text}"
        );
    }
}
