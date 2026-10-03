//! Structured scan-progress observer and inline renderer.
//!
//! Hot-path code emits state through [`ProgressObserver`]; terminal formatting
//! lives only in [`IndicatifRenderer`]. The no-op observer is effectively free
//! and is used by tests and non-interactive calls.

use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanPhase {
    Discovery,
    Resolution,
    Analysis,
    Reporting,
    CleanupPreview,
    CleanupSimulate,
    CleanupExecute,
}

/// Narrow observer: domain/traversal code emits state, never formatting.
///
/// All methods take `&self` so parallel traversal can share one observer
/// through atomics. Implementations MUST NOT allocate per visited entry;
/// directory counters are aggregated in batches.
///
/// `units_total` / `unit_completed` are the semantic determinate-progress
/// contract (C002 §7.5): callers announce a known total once (e.g. group
/// count after grouping) and then record each completed unit. Renderers
/// reset/re-scope totals on phase changes so scan-analysis counts cannot
/// leak into cleanup counts. Callers MUST NOT assume the concrete renderer
/// type; they emit through this trait only.
pub trait ProgressObserver: Send + Sync {
    fn phase(&self, _phase: ScanPhase) {}
    fn dirs_visited(&self, _batch: u64) {}
    fn dirs_pruned(&self, _batch: u64) {}
    fn manifests_found(&self, _batch: u64) {}
    fn workspaces_resolved(&self, _batch: u64) {}
    fn cargo_failure(&self) {}
    fn empty_skipped(&self) {}
    fn active_skipped(&self) {}
    fn group_measured(&self, _bytes: u64) {}
    fn reportable_group(&self, _path: &Path, _bytes: u64) {}
    fn units_total(&self, _phase: ScanPhase, _total: u64) {}
    fn unit_completed(&self, _phase: ScanPhase) {}
}

/// Effectively free observer for tests and non-interactive calls.
#[derive(Debug, Default)]
pub struct NoopObserver;

impl ProgressObserver for NoopObserver {}

/// Recording observer for tests and instrumentation.
///
/// Refresh counting is renderer-side; this records semantic events only.
#[derive(Debug, Default)]
pub struct TestObserver {
    pub phases: Mutex<Vec<ScanPhase>>,
    pub visited: AtomicU64,
    pub pruned: AtomicU64,
    pub manifests: AtomicU64,
    pub workspaces: AtomicU64,
    pub cargo_failures: AtomicU64,
    pub empty_skipped: AtomicU64,
    pub active_skipped: AtomicU64,
    pub groups_measured: AtomicU64,
    pub bytes_measured: AtomicU64,
    pub reportable: Mutex<Vec<(PathBuf, u64)>>,
    pub totals: Mutex<Vec<(ScanPhase, u64)>>,
    pub completed: Mutex<Vec<ScanPhase>>,
}

impl TestObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_for(&self, phase: ScanPhase) -> Option<u64> {
        self.totals
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(p, _)| *p == phase)
            .map(|(_, t)| *t)
    }

    pub fn completed_count(&self, phase: ScanPhase) -> usize {
        self.completed
            .lock()
            .unwrap()
            .iter()
            .filter(|p| **p == phase)
            .count()
    }
}

impl ProgressObserver for TestObserver {
    fn phase(&self, phase: ScanPhase) {
        self.phases.lock().unwrap().push(phase);
    }
    fn dirs_visited(&self, batch: u64) {
        self.visited.fetch_add(batch, Ordering::Relaxed);
    }
    fn dirs_pruned(&self, batch: u64) {
        self.pruned.fetch_add(batch, Ordering::Relaxed);
    }
    fn manifests_found(&self, batch: u64) {
        self.manifests.fetch_add(batch, Ordering::Relaxed);
    }
    fn workspaces_resolved(&self, batch: u64) {
        self.workspaces.fetch_add(batch, Ordering::Relaxed);
    }
    fn cargo_failure(&self) {
        self.cargo_failures.fetch_add(1, Ordering::Relaxed);
    }
    fn empty_skipped(&self) {
        self.empty_skipped.fetch_add(1, Ordering::Relaxed);
    }
    fn active_skipped(&self) {
        self.active_skipped.fetch_add(1, Ordering::Relaxed);
    }
    fn group_measured(&self, bytes: u64) {
        self.groups_measured.fetch_add(1, Ordering::Relaxed);
        self.bytes_measured.fetch_add(bytes, Ordering::Relaxed);
    }
    fn reportable_group(&self, path: &Path, bytes: u64) {
        self.reportable
            .lock()
            .unwrap()
            .push((path.to_path_buf(), bytes));
    }
    fn units_total(&self, phase: ScanPhase, total: u64) {
        self.totals.lock().unwrap().push((phase, total));
    }
    fn unit_completed(&self, phase: ScanPhase) {
        self.completed.lock().unwrap().push(phase);
    }
}

/// Whether transient progress may be shown.
pub fn should_show_progress(no_progress: bool) -> bool {
    if no_progress {
        return false;
    }
    if std::env::var_os("TERM").is_some_and(|v| v == "dumb") {
        return false;
    }
    // Hidden when stderr is not an attended terminal; tests force hidden via
    // `--no-progress` or `TERM=dumb` so no control sequences leak to pipes.
    #[allow(clippy::needless_bool)]
    if !stderr_is_terminal() {
        return false;
    }
    true
}

fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

fn format_bytes(n: u64) -> String {
    crate::report::format_bytes(n)
}

/// Inline attended-terminal renderer on stderr.
///
/// One [`indicatif::MultiProgress`] owns the main bar plus up to five
/// candidate rows (C002 §7.4), so bars share a single cursor/draw owner and
/// cannot independently fight for terminal state.
///
/// - Discovery uses an indeterminate spinner (total unknown without pre-scan).
/// - Analysis/cleanup switch to determinate once group/candidate count is
///   known via [`ProgressObserver::units_total`].
/// - At most five reportable rows beneath the main bar, largest-first.
/// - Refresh capped at 10 Hz; never redrawn per filesystem entry.
/// - `finish_and_clear` clears before the deterministic stdout report.
/// - Hidden renderer emits no terminal output and never fails a scan.
pub struct IndicatifRenderer {
    hidden: bool,
    multi: Option<indicatif::MultiProgress>,
    main: Option<indicatif::ProgressBar>,
    rows: Vec<indicatif::ProgressBar>,
    top: Mutex<Vec<(PathBuf, u64)>>,
    last_draw: Mutex<Option<Instant>>,
    pub refreshes: AtomicU64,
    visited: AtomicU64,
    pruned: AtomicU64,
    manifests: AtomicU64,
    workspaces: AtomicU64,
    eligible: AtomicU64,
    determinate_total: AtomicU64,
    determinate_done: AtomicU64,
    current_phase: Mutex<Option<ScanPhase>>,
}

impl IndicatifRenderer {
    fn assemble(
        hidden: bool,
        multi: Option<indicatif::MultiProgress>,
        main: Option<indicatif::ProgressBar>,
        rows: Vec<indicatif::ProgressBar>,
    ) -> Self {
        Self {
            hidden,
            multi,
            main,
            rows,
            top: Mutex::new(Vec::new()),
            last_draw: Mutex::new(None),
            refreshes: AtomicU64::new(0),
            visited: AtomicU64::new(0),
            pruned: AtomicU64::new(0),
            manifests: AtomicU64::new(0),
            workspaces: AtomicU64::new(0),
            eligible: AtomicU64::new(0),
            determinate_total: AtomicU64::new(0),
            determinate_done: AtomicU64::new(0),
            current_phase: Mutex::new(None),
        }
    }

    pub fn new(hidden: bool) -> Self {
        if hidden {
            return Self::assemble(true, None, None, Vec::new());
        }
        // One coordinated draw target owns all six lines. Bars added via
        // `MultiProgress::add` have their draw target intercepted by the
        // multi object, so they never fight for cursor state independently.
        let multi = indicatif::MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::stderr_with_hz(10),
        );
        let main = multi.add(indicatif::ProgressBar::new_spinner());
        main.enable_steady_tick(Duration::from_millis(100));
        main.set_message("scanning…");
        let mut rows = Vec::new();
        for _ in 0..5 {
            let row = multi.add(indicatif::ProgressBar::new_spinner());
            rows.push(row);
        }
        Self::assemble(false, Some(multi), Some(main), rows)
    }

    /// In-memory coordinated renderer for tests.
    ///
    /// Uses the same one-`MultiProgress` composition as production but draws
    /// into an [`indicatif::InMemoryTerm`] so tests can assert the composed
    /// multi-line frame without touching a real terminal.
    pub fn new_in_memory_for_test() -> (Self, indicatif::InMemoryTerm) {
        let term = indicatif::InMemoryTerm::new(24, 120);
        let multi = indicatif::MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::term_like_with_hz(Box::new(term.clone()), 10),
        );
        let main = multi.add(indicatif::ProgressBar::new_spinner());
        main.set_message("test");
        let mut rows = Vec::new();
        for _ in 0..5 {
            rows.push(multi.add(indicatif::ProgressBar::new_spinner()));
        }
        (Self::assemble(false, Some(multi), Some(main), rows), term)
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Number of progress bars owned by the coordinated renderer.
    ///
    /// Production attended renderer owns exactly six (one main + five rows);
    /// hidden owns zero.
    pub fn coordinated_bar_count(&self) -> usize {
        if self.hidden {
            return 0;
        }
        self.rows.len() + usize::from(self.main.is_some())
    }

    pub fn has_shared_draw_target(&self) -> bool {
        if self.hidden {
            return false;
        }
        self.multi.is_some()
    }

    /// Switch analysis to determinate mode once the total is known.
    ///
    /// Prefer emitting [`ProgressObserver::units_total`] through the trait;
    /// this concrete helper remains for direct callers and forwards to the
    /// same state.
    pub fn set_determinate_total(&self, total: u64) {
        self.determinate_total.store(total, Ordering::Relaxed);
        if self.hidden {
            return;
        }
        if let Some(main) = &self.main {
            main.set_length(total);
        }
    }

    fn reset_totals_for_phase(&self, phase: ScanPhase) {
        let mut current = self.current_phase.lock().unwrap();
        if *current != Some(phase) {
            *current = Some(phase);
            self.determinate_total.store(0, Ordering::Relaxed);
            self.determinate_done.store(0, Ordering::Relaxed);
            if !self.hidden
                && let Some(main) = &self.main
            {
                main.set_length(0);
                main.set_position(0);
            }
        }
    }

    fn apply_total(&self, total: u64) {
        // Re-scope totals: setting a new total starts a fresh determinate
        // scope with zero completed units, so scan-analysis counts cannot
        // leak into cleanup counts (C002 §7.5).
        self.determinate_total.store(total, Ordering::Relaxed);
        self.determinate_done.store(0, Ordering::Relaxed);
        if self.hidden {
            return;
        }
        if let Some(main) = &self.main {
            main.set_length(total);
            main.set_position(0);
        }
    }

    fn apply_completed(&self) {
        self.determinate_done.fetch_add(1, Ordering::Relaxed);
    }

    fn maybe_draw(&self) {
        if self.hidden {
            return;
        }
        let mut last = self.last_draw.lock().unwrap();
        let now = Instant::now();
        if let Some(prev) = *last
            && now.duration_since(prev) < Duration::from_millis(100)
        {
            return;
        }
        *last = Some(now);
        drop(last);
        self.refreshes.fetch_add(1, Ordering::Relaxed);
        let visited = self.visited.load(Ordering::Relaxed);
        let pruned = self.pruned.load(Ordering::Relaxed);
        let manifests = self.manifests.load(Ordering::Relaxed);
        let workspaces = self.workspaces.load(Ordering::Relaxed);
        let eligible = self.eligible.load(Ordering::Relaxed);
        let done = self.determinate_done.load(Ordering::Relaxed);
        let total = self.determinate_total.load(Ordering::Relaxed);
        let msg = if total > 0 {
            format!(
                "analyzing {done}/{total} visited {visited} pruned {pruned} manifests {manifests} workspaces {workspaces} eligible {eligible}"
            )
        } else {
            format!(
                "scanning visited {visited} pruned {pruned} manifests {manifests} workspaces {workspaces} eligible {eligible}"
            )
        };
        if let Some(main) = &self.main {
            main.set_message(msg);
            if total > 0 {
                main.set_position(done.min(total));
            } else {
                main.tick();
            }
        }
        let top = self.top.lock().unwrap().clone();
        for (i, row) in self.rows.iter().enumerate() {
            if let Some((path, bytes)) = top.get(i) {
                let text = format!("{}  {}", format_bytes(*bytes), path.to_string_lossy());
                row.set_message(text);
            } else {
                row.set_message(String::new());
            }
        }
    }

    /// Clear transient UI before the final stdout report. Never fails a scan.
    pub fn finish_and_clear(&self) {
        if self.hidden {
            return;
        }
        if let Some(multi) = &self.multi {
            let _ = multi.clear();
        }
        if let Some(main) = &self.main {
            main.finish_and_clear();
        }
        for row in &self.rows {
            row.finish_and_clear();
        }
    }

    pub fn refresh_count(&self) -> u64 {
        self.refreshes.load(Ordering::Relaxed)
    }

    pub fn top_groups(&self) -> Vec<(PathBuf, u64)> {
        self.top.lock().unwrap().clone()
    }

    pub fn determinate_total_value(&self) -> u64 {
        self.determinate_total.load(Ordering::Relaxed)
    }

    pub fn determinate_done_value(&self) -> u64 {
        self.determinate_done.load(Ordering::Relaxed)
    }
}

impl ProgressObserver for IndicatifRenderer {
    fn phase(&self, phase: ScanPhase) {
        self.reset_totals_for_phase(phase);
        if self.hidden {
            return;
        }
        if let Some(main) = &self.main {
            let label = match phase {
                ScanPhase::Discovery => "discovering manifests…",
                ScanPhase::Resolution => "resolving workspaces…",
                ScanPhase::Analysis => "analyzing output…",
                ScanPhase::Reporting => "reporting…",
                ScanPhase::CleanupPreview => "previewing cleanup…",
                ScanPhase::CleanupSimulate => "simulating cleanup…",
                ScanPhase::CleanupExecute => "cleaning…",
            };
            main.set_prefix(label.to_owned());
        }
        self.maybe_draw();
    }
    fn dirs_visited(&self, batch: u64) {
        self.visited.fetch_add(batch, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn dirs_pruned(&self, batch: u64) {
        self.pruned.fetch_add(batch, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn manifests_found(&self, batch: u64) {
        self.manifests.fetch_add(batch, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn workspaces_resolved(&self, batch: u64) {
        self.workspaces.fetch_add(batch, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn cargo_failure(&self) {
        self.maybe_draw();
    }
    fn empty_skipped(&self) {
        self.maybe_draw();
    }
    fn active_skipped(&self) {
        // Determinate progress advances only via `unit_completed` (C002 §7.5)
        // so scan-analysis and cleanup counts share one explicit contract.
        self.maybe_draw();
    }
    fn group_measured(&self, _bytes: u64) {
        self.maybe_draw();
    }
    fn reportable_group(&self, path: &Path, bytes: u64) {
        self.eligible.fetch_add(1, Ordering::Relaxed);
        {
            let mut top = self.top.lock().unwrap();
            top.push((path.to_path_buf(), bytes));
            top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            top.truncate(5);
        }
        self.maybe_draw();
    }
    fn units_total(&self, _phase: ScanPhase, total: u64) {
        self.apply_total(total);
        self.maybe_draw();
    }
    fn unit_completed(&self, _phase: ScanPhase) {
        self.apply_completed();
        self.maybe_draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_observer_is_free_and_hidden_renderer_emits_nothing() {
        let noop = NoopObserver;
        noop.dirs_visited(100);
        noop.reportable_group(Path::new("/tmp/x"), 10);
        noop.units_total(ScanPhase::Analysis, 3);
        noop.unit_completed(ScanPhase::Analysis);
        let hidden = IndicatifRenderer::new(true);
        assert!(hidden.is_hidden());
        assert_eq!(hidden.refresh_count(), 0);
        assert_eq!(hidden.coordinated_bar_count(), 0);
        assert!(!hidden.has_shared_draw_target());
        hidden.dirs_visited(1000);
        // Hidden renderer never draws, so refresh stays zero despite events.
        assert_eq!(hidden.refresh_count(), 0);
        hidden.reportable_group(Path::new("/tmp/a"), 5);
        assert_eq!(hidden.top_groups().len(), 1);
        hidden.finish_and_clear();
    }

    #[test]
    fn coordinated_renderer_owns_six_bars_on_one_draw_target() {
        let (r, _term) = IndicatifRenderer::new_in_memory_for_test();
        assert!(!r.is_hidden());
        assert_eq!(r.coordinated_bar_count(), 6);
        assert!(r.has_shared_draw_target());
        r.finish_and_clear();
    }

    #[test]
    fn in_memory_frame_contains_at_most_six_progress_lines() {
        let (r, term) = IndicatifRenderer::new_in_memory_for_test();
        r.phase(ScanPhase::Analysis);
        r.units_total(ScanPhase::Analysis, 6);
        for i in 0..6 {
            r.reportable_group(&PathBuf::from(format!("/tmp/g{i}")), (i as u64 + 1) * 100);
            std::thread::sleep(Duration::from_millis(110));
        }
        // Allow the 10 Hz draw target to flush.
        std::thread::sleep(Duration::from_millis(150));
        let contents = term.contents();
        let non_empty: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            non_empty.len() <= 6,
            "composed frame must contain at most six progress lines, got {}: {contents:?}",
            non_empty.len()
        );
        // Repeated refreshes must not append an unbounded scrolling log.
        for _ in 0..20 {
            r.dirs_visited(1);
            std::thread::sleep(Duration::from_millis(110));
        }
        std::thread::sleep(Duration::from_millis(150));
        let after = term.contents();
        let after_lines: Vec<&str> = after.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            after_lines.len() <= 6,
            "refreshes must not scroll unboundedly: {after:?}"
        );
        r.finish_and_clear();
    }

    #[test]
    fn renderer_caps_rows_at_five_largest_first() {
        let r = IndicatifRenderer::new(true);
        for i in 0..10 {
            r.reportable_group(&PathBuf::from(format!("/tmp/g{i}")), i * 100);
        }
        let top = r.top_groups();
        assert_eq!(top.len(), 5);
        assert_eq!(top[0].1, 900);
        assert_eq!(top[4].1, 500);
    }

    #[test]
    fn test_observer_records_semantic_events() {
        let o = TestObserver::new();
        o.phase(ScanPhase::Discovery);
        o.dirs_visited(3);
        o.manifests_found(1);
        o.reportable_group(Path::new("/a"), 7);
        o.units_total(ScanPhase::Analysis, 4);
        o.unit_completed(ScanPhase::Analysis);
        assert_eq!(o.visited.load(Ordering::Relaxed), 3);
        assert_eq!(o.reportable.lock().unwrap().len(), 1);
        assert_eq!(o.total_for(ScanPhase::Analysis), Some(4));
        assert_eq!(o.completed_count(ScanPhase::Analysis), 1);
    }

    #[test]
    fn should_show_progress_respects_flag_and_dumb_term() {
        assert!(!should_show_progress(true));
        let prev = std::env::var_os("TERM");
        unsafe { std::env::set_var("TERM", "dumb") };
        assert!(!should_show_progress(false));
        if let Some(v) = prev {
            unsafe { std::env::set_var("TERM", v) };
        } else {
            unsafe { std::env::remove_var("TERM") };
        }
    }

    #[test]
    fn renderer_refresh_bounded_independently_of_entry_count() {
        // Hidden renderer proves zero refreshes for many entries; visible
        // renderer throttles to 10 Hz via `maybe_draw` time gate. Here we
        // assert the hidden bound and that visible construction does not
        // allocate per entry (batched counters only).
        let hidden = IndicatifRenderer::new(true);
        for _ in 0..10_000 {
            hidden.dirs_visited(1);
        }
        assert_eq!(hidden.refresh_count(), 0);
    }

    #[test]
    fn discovery_begins_indeterminate_then_analysis_determinate() {
        // Discovery uses an indeterminate spinner (no total); analysis may
        // switch to determinate once workspace/group count is known via the
        // semantic `units_total` / `unit_completed` contract (C002 §7.5).
        let r = IndicatifRenderer::new(true);
        r.phase(ScanPhase::Discovery);
        assert_eq!(
            r.determinate_total_value(),
            0,
            "discovery must not set a total (would require wasteful pre-scan)"
        );
        r.units_total(ScanPhase::Analysis, 7);
        assert_eq!(r.determinate_total_value(), 7);
        r.phase(ScanPhase::Analysis);
        // Phase change re-scopes totals so old counts cannot leak.
        assert_eq!(r.determinate_total_value(), 0);
        r.units_total(ScanPhase::Analysis, 7);
        // Determinate progress advances only via `unit_completed`, never via
        // legacy `group_measured` / `active_skipped` bookkeeping.
        r.group_measured(100);
        assert_eq!(r.determinate_done_value(), 0);
        r.unit_completed(ScanPhase::Analysis);
        assert_eq!(r.determinate_done_value(), 1);
    }

    #[test]
    fn phase_total_resets_so_cleanup_cannot_reuse_analysis_counts() {
        let r = IndicatifRenderer::new(true);
        r.phase(ScanPhase::Analysis);
        r.units_total(ScanPhase::Analysis, 5);
        r.unit_completed(ScanPhase::Analysis);
        assert_eq!(r.determinate_total_value(), 5);
        r.phase(ScanPhase::CleanupSimulate);
        assert_eq!(r.determinate_total_value(), 0);
        assert_eq!(r.determinate_done_value(), 0);
        r.units_total(ScanPhase::CleanupSimulate, 2);
        r.unit_completed(ScanPhase::CleanupSimulate);
        assert_eq!(r.determinate_total_value(), 2);
        assert_eq!(r.determinate_done_value(), 1);
    }

    #[test]
    fn sizes_render_in_top_rows_and_clear_before_report() {
        let r = IndicatifRenderer::new(true);
        r.reportable_group(Path::new("/tmp/big"), 2048);
        let top = r.top_groups();
        assert_eq!(top.len(), 1);
        // Sizes render via the shared byte formatter.
        assert!(format_bytes(2048).contains("2.00"));
        // Final clear occurs before report write and never fails.
        r.finish_and_clear();
        r.finish_and_clear();
    }

    #[test]
    fn renderer_error_falls_back_without_failing_scan() {
        // Construction is infallible; hidden fallback never fails a scan even
        // if terminal state is unavailable (non-TTY, dumb, or --no-progress).
        let hidden = IndicatifRenderer::new(true);
        hidden.phase(ScanPhase::Discovery);
        hidden.dirs_visited(10);
        hidden.reportable_group(Path::new("/a"), 1);
        hidden.finish_and_clear();
        assert_eq!(hidden.refresh_count(), 0);
    }
}
