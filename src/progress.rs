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
    fn discovery_issues(&self, _count: u64) {}
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
    pub discovery_issues: AtomicU64,
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
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .find(|(p, _)| *p == phase)
            .map(|(_, t)| *t)
    }

    pub fn completed_count(&self, phase: ScanPhase) -> usize {
        self.completed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|p| **p == phase)
            .count()
    }
}

impl ProgressObserver for TestObserver {
    fn phase(&self, phase: ScanPhase) {
        self.phases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(phase);
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
    fn discovery_issues(&self, count: u64) {
        self.discovery_issues.fetch_add(count, Ordering::Relaxed);
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
            .unwrap_or_else(|e| e.into_inner())
            .push((path.to_path_buf(), bytes));
    }
    fn units_total(&self, phase: ScanPhase, total: u64) {
        self.totals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((phase, total));
    }
    fn unit_completed(&self, phase: ScanPhase) {
        self.completed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(phase);
    }
}

/// Whether transient progress may be shown.
pub fn should_show_progress(no_progress: bool) -> bool {
    if no_progress {
        return false;
    }
    if term_is_dumb() {
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

/// Whether the terminal itself refuses progress output.
///
/// Split out of [`should_show_progress`] so this gate is testable on its own.
/// The public function also consults `stderr_is_terminal()`, which is false in
/// every captured or CI run, so an assertion on it held with this branch
/// deleted — it proved the environment, not the code.
fn term_is_dumb() -> bool {
    std::env::var_os("TERM").is_some_and(|v| v == "dumb")
}

fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

/// Inline attended-terminal renderer on stderr.
///
/// **One line, and one line only.** The renderer owns a single
/// [`indicatif::ProgressBar`]; there is no `MultiProgress` fan-out and no
/// candidate rows beneath the bar. M020 retired the five transient candidate
/// rows that M005A §7.4 specified: on a real run they were empty (a spinner
/// glyph and nothing else) for almost the whole traversal, so the "progress"
/// occupied six terminal lines and said nothing.
///
/// - The **phase** is the message. The phase transitions (`Discovery`,
///   `Resolution`, `Analysis`, `Cleanup`) are visible, which they were not
///   before: the phase used to be written into the bar's *prefix*, and
///   indicatif's default spinner template is `{spinner} {msg}` — it does not
///   render the prefix at all.
/// - The quiet surface shows the phase and, once the total is known, the
///   fraction done. `--verbose` adds the counters to the same line.
/// - Refresh capped at 10 Hz; never redrawn per filesystem entry.
/// - `finish_and_clear` clears before the deterministic stdout report.
/// - Hidden renderer emits no terminal output and never fails a scan.
pub struct IndicatifRenderer {
    hidden: bool,
    verbose: bool,
    multi: Option<indicatif::MultiProgress>,
    main: Option<indicatif::ProgressBar>,
    top: Mutex<Vec<(PathBuf, u64)>>,
    last_draw: Mutex<Option<Instant>>,
    pub refreshes: AtomicU64,
    visited: AtomicU64,
    pruned: AtomicU64,
    manifests: AtomicU64,
    discovery_issues: AtomicU64,
    workspaces: AtomicU64,
    cargo_failures: AtomicU64,
    eligible: AtomicU64,
    determinate_total: AtomicU64,
    determinate_done: AtomicU64,
    current_phase: Mutex<Option<ScanPhase>>,
}

impl IndicatifRenderer {
    fn assemble(
        hidden: bool,
        verbose: bool,
        multi: Option<indicatif::MultiProgress>,
        main: Option<indicatif::ProgressBar>,
    ) -> Self {
        Self {
            hidden,
            verbose,
            multi,
            main,
            top: Mutex::new(Vec::new()),
            last_draw: Mutex::new(None),
            refreshes: AtomicU64::new(0),
            visited: AtomicU64::new(0),
            pruned: AtomicU64::new(0),
            manifests: AtomicU64::new(0),
            discovery_issues: AtomicU64::new(0),
            workspaces: AtomicU64::new(0),
            cargo_failures: AtomicU64::new(0),
            eligible: AtomicU64::new(0),
            determinate_total: AtomicU64::new(0),
            determinate_done: AtomicU64::new(0),
            current_phase: Mutex::new(None),
        }
    }

    pub fn new(hidden: bool, verbose: bool) -> Self {
        if hidden {
            return Self::assemble(true, verbose, None, None);
        }
        // One coordinated draw target owns the single line. The `MultiProgress`
        // is retained so there is exactly one owner of terminal state, even
        // though there is only one bar in it.
        let multi = indicatif::MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::stderr_with_hz(10),
        );
        let main = multi.add(indicatif::ProgressBar::new_spinner());
        main.enable_steady_tick(Duration::from_millis(100));
        main.set_message("scanning…");
        Self::assemble(false, verbose, Some(multi), Some(main))
    }

    /// In-memory coordinated renderer for tests.
    ///
    /// Uses the same one-`MultiProgress` composition as production but draws
    /// into an [`indicatif::InMemoryTerm`] so tests can assert the composed
    /// frame without touching a real terminal.
    pub fn new_in_memory_for_test(verbose: bool) -> (Self, indicatif::InMemoryTerm) {
        let term = indicatif::InMemoryTerm::new(24, 120);
        let multi = indicatif::MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::term_like_with_hz(Box::new(term.clone()), 10),
        );
        let main = multi.add(indicatif::ProgressBar::new_spinner());
        main.set_message("scanning…");
        (
            Self::assemble(false, verbose, Some(multi), Some(main)),
            term,
        )
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Number of progress bars owned by the coordinated renderer.
    ///
    /// The attended renderer owns exactly **one** (M020 retired the five
    /// transient candidate rows); a hidden renderer owns zero.
    pub fn coordinated_bar_count(&self) -> usize {
        if self.hidden {
            return 0;
        }
        usize::from(self.main.is_some())
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
    }

    fn reset_totals_for_phase(&self, phase: ScanPhase) {
        let mut current = self.current_phase.lock().unwrap_or_else(|e| e.into_inner());
        if *current != Some(phase) {
            *current = Some(phase);
            self.determinate_total.store(0, Ordering::Relaxed);
            self.determinate_done.store(0, Ordering::Relaxed);
        }
    }

    fn apply_total(&self, total: u64) {
        // Re-scope totals: setting a new total starts a fresh determinate
        // scope with zero completed units, so scan-analysis counts cannot
        // leak into cleanup counts (C002 §7.5).
        self.determinate_total.store(total, Ordering::Relaxed);
        self.determinate_done.store(0, Ordering::Relaxed);
    }

    fn apply_completed(&self) {
        self.determinate_done.fetch_add(1, Ordering::Relaxed);
    }

    /// The phase's label, without the ellipsis.
    fn phase_label(phase: ScanPhase) -> &'static str {
        match phase {
            ScanPhase::Discovery => "scanning",
            ScanPhase::Resolution => "resolving workspaces",
            ScanPhase::Analysis => "analyzing output",
            ScanPhase::Reporting => "reporting",
            ScanPhase::CleanupPreview => "previewing cleanup",
            ScanPhase::CleanupSimulate => "simulating cleanup",
            ScanPhase::CleanupExecute => "cleaning",
        }
    }

    /// The one line the renderer draws.
    ///
    /// Quiet: the phase, plus the fraction once the total is known. Verbose:
    /// the phase and the counters, on the *same* line — the surface is never
    /// more than one line tall, whatever the verbosity.
    fn compose_message(&self) -> String {
        let phase = *self.current_phase.lock().unwrap_or_else(|e| e.into_inner());
        let label = Self::phase_label(phase.unwrap_or(ScanPhase::Discovery));
        let done = self.determinate_done.load(Ordering::Relaxed);
        let total = self.determinate_total.load(Ordering::Relaxed);
        let fraction = if total > 0 {
            format!(" {done}/{total}")
        } else {
            String::new()
        };
        if !self.verbose {
            return format!("{label}{fraction}…");
        }
        let visited = self.visited.load(Ordering::Relaxed);
        let pruned = self.pruned.load(Ordering::Relaxed);
        let manifests = self.manifests.load(Ordering::Relaxed);
        let discovery_issues = self.discovery_issues.load(Ordering::Relaxed);
        let workspaces = self.workspaces.load(Ordering::Relaxed);
        let cargo_failures = self.cargo_failures.load(Ordering::Relaxed);
        let eligible = self.eligible.load(Ordering::Relaxed);
        format!(
            "{label}{fraction}… visited {visited} pruned {pruned} manifests {manifests} workspaces {workspaces} discovery-errors {discovery_issues} cargo-failures {cargo_failures} eligible {eligible}"
        )
    }

    fn maybe_draw(&self) {
        if self.hidden {
            return;
        }
        let mut last = self.last_draw.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if let Some(prev) = *last
            && now.duration_since(prev) < Duration::from_millis(100)
        {
            return;
        }
        *last = Some(now);
        drop(last);
        self.refreshes.fetch_add(1, Ordering::Relaxed);
        let msg = self.compose_message();
        if let Some(main) = &self.main {
            main.set_message(msg);
            // The spinner advances in both modes, but by different hands.
            // Production installs a steady ticker (`enable_steady_tick`), and
            // indicatif's `tick` is a no-op once a ticker is installed — so
            // there the ticker owns the glyph. The in-memory renderer installs
            // no ticker, so the same call is what drives its glyph. What the
            // operator sees is the same moving spinner either way; the two
            // modes differ in who advances it, not in what they see.
            main.tick();
            let total = self.determinate_total.load(Ordering::Relaxed);
            if total > 0 {
                let done = self.determinate_done.load(Ordering::Relaxed);
                main.set_position(done.min(total));
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
    }

    pub fn refresh_count(&self) -> u64 {
        self.refreshes.load(Ordering::Relaxed)
    }

    pub fn top_groups(&self) -> Vec<(PathBuf, u64)> {
        self.top.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
        // The phase goes into the *message*, not the prefix: indicatif's
        // default spinner template is `{spinner} {msg}` and never renders the
        // prefix, so a phase written there was invisible on the terminal.
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
    fn discovery_issues(&self, count: u64) {
        self.discovery_issues.fetch_add(count, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn workspaces_resolved(&self, batch: u64) {
        self.workspaces.fetch_add(batch, Ordering::Relaxed);
        self.maybe_draw();
    }
    fn cargo_failure(&self) {
        self.cargo_failures.fetch_add(1, Ordering::Relaxed);
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
            let mut top = self.top.lock().unwrap_or_else(|e| e.into_inner());
            top.push((path.to_path_buf(), bytes));
            top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            top.truncate(5);
        }
        self.maybe_draw();
    }
    fn units_total(&self, phase: ScanPhase, total: u64) {
        // Re-scope on the phase carried by the event, not only on an explicit
        // `phase()` call: scan-analysis counts must never leak into cleanup
        // counts even if a caller forgets to announce the transition (M8).
        self.reset_totals_for_phase(phase);
        self.apply_total(total);
        self.maybe_draw();
    }
    fn unit_completed(&self, phase: ScanPhase) {
        self.reset_totals_for_phase(phase);
        self.apply_completed();
        self.maybe_draw();
    }
}

/// Test-only support for process-global environment mutation.
///
/// The environment is process-global: a test that sets a variable changes it
/// for every other test in the binary. Two rules follow, and both are
/// enforced here rather than at each call site:
///
/// - The restore must survive a **panic**. A failure between the set and the
///   restore left `TERM=dumb` for every later test in the binary.
/// - The mutation must be **serialised**. Two tests that set the same
///   variable at the same time are a race whose outcome depends on the
///   scheduler.
#[cfg(test)]
pub(crate) mod test_env {
    use std::ffi::OsStr;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// The one lock every environment-mutating test holds.
    fn lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Holds the environment lock, and restores every variable it touched.
    pub(crate) struct EnvRestore {
        keys: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvRestore {
        /// Takes the environment lock. Every variable set through this guard
        /// is restored when it is dropped.
        pub(crate) fn acquire() -> Self {
            Self {
                keys: Vec::new(),
                _lock: lock(),
            }
        }

        /// Sets `key`, remembering its previous value the first time.
        pub(crate) fn set(&mut self, key: &'static str, value: impl AsRef<OsStr>) {
            self.remember(key);
            unsafe { std::env::set_var(key, value) };
        }

        /// Removes `key`, remembering its previous value the first time.
        pub(crate) fn remove(&mut self, key: &'static str) {
            self.remember(key);
            unsafe { std::env::remove_var(key) };
        }

        fn remember(&mut self, key: &'static str) {
            if self.keys.iter().any(|(k, _)| *k == key) {
                return;
            }
            self.keys.push((key, std::env::var_os(key)));
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, prev) in &self.keys {
                match prev {
                    Some(v) => unsafe { std::env::set_var(key, v) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
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
        let hidden = IndicatifRenderer::new(true, false);
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

    /// M020: the attended renderer owns **one** bar, not six.
    ///
    /// The five transient candidate rows M005A §7.4 specified are retired: on a
    /// real run they were empty for almost the whole traversal, so the
    /// "progress" occupied six terminal lines and said nothing. *Fails if:* a
    /// second bar is ever added to the renderer.
    #[test]
    fn coordinated_renderer_owns_exactly_one_bar_on_one_draw_target() {
        let (r, _term) = IndicatifRenderer::new_in_memory_for_test(false);
        assert!(!r.is_hidden());
        assert_eq!(r.coordinated_bar_count(), 1);
        assert!(r.has_shared_draw_target());
        r.finish_and_clear();
    }

    /// The drawn frame is **one** line, and the phase is on it.
    ///
    /// Two properties, one test: the frame must not grow a second line, and it
    /// must name the phase. The phase used to be written into the bar's
    /// *prefix*, and indicatif's default spinner template (`{spinner} {msg}`)
    /// never renders the prefix — so the phase transitions were invisible on
    /// the terminal. *Fails if:* the phase is written anywhere the template
    /// does not render, or a second bar appears.
    #[test]
    fn in_memory_frame_contains_exactly_one_progress_line_naming_the_phase() {
        let (r, term) = IndicatifRenderer::new_in_memory_for_test(false);
        r.phase(ScanPhase::Analysis);
        r.discovery_issues(7);
        r.cargo_failure();
        r.cargo_failure();
        r.units_total(ScanPhase::Analysis, 6);
        r.unit_completed(ScanPhase::Analysis);
        for i in 0..6 {
            r.reportable_group(&PathBuf::from(format!("/tmp/g{i}")), (i as u64 + 1) * 100);
            std::thread::sleep(Duration::from_millis(110));
        }
        // Allow the 10 Hz draw target to flush.
        std::thread::sleep(Duration::from_millis(150));
        let contents = term.contents();
        let non_empty: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            non_empty.len() <= 1,
            "the composed frame must be one line, got {}: {contents:?}",
            non_empty.len()
        );
        assert!(
            contents.contains("analyzing output"),
            "the phase must be visible on the drawn line: {contents:?}"
        );
        assert!(
            contents.contains("1/6"),
            "the done fraction must be visible once the total is known: {contents:?}"
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
            after_lines.len() <= 1,
            "refreshes must not scroll unboundedly: {after:?}"
        );
        r.finish_and_clear();
    }

    /// The verbose surface adds the counters to the **same** line.
    ///
    /// Verbosity must never make the frame taller: it makes the one line
    /// *say more*. *Fails if:* the counters go onto a second line, or if the
    /// verbose renderer stops showing them.
    #[test]
    fn the_verbose_surface_adds_counters_to_the_same_single_line() {
        let (r, term) = IndicatifRenderer::new_in_memory_for_test(true);
        r.phase(ScanPhase::Analysis);
        r.units_total(ScanPhase::Analysis, 6);
        // The draw is throttled to 10 Hz, so the events have to straddle the
        // throttle window before the frame carries them.
        std::thread::sleep(Duration::from_millis(110));
        r.discovery_issues(7);
        r.cargo_failure();
        r.cargo_failure();
        std::thread::sleep(Duration::from_millis(110));
        r.maybe_draw();
        std::thread::sleep(Duration::from_millis(150));
        let contents = term.contents();
        let non_empty: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            non_empty.len() <= 1,
            "the verbose frame must still be one line, got {}: {contents:?}",
            non_empty.len()
        );
        assert!(
            contents.contains("discovery-errors 7"),
            "the verbose line carries the counters: {contents:?}"
        );
        assert!(
            contents.contains("cargo-failures 2"),
            "the verbose line carries the counters: {contents:?}"
        );
        assert!(
            contents.contains("analyzing output"),
            "the verbose line still names the phase: {contents:?}"
        );
        r.finish_and_clear();
    }

    /// The quiet surface carries no counters.
    ///
    /// The counters are debugging information; the default surface must not
    /// carry them. *Fails if:* a counter is added to the quiet message.
    #[test]
    fn the_quiet_surface_carries_no_counters() {
        let (r, term) = IndicatifRenderer::new_in_memory_for_test(false);
        r.phase(ScanPhase::Analysis);
        r.units_total(ScanPhase::Analysis, 6);
        std::thread::sleep(Duration::from_millis(110));
        r.discovery_issues(7);
        r.cargo_failure();
        std::thread::sleep(Duration::from_millis(110));
        r.maybe_draw();
        std::thread::sleep(Duration::from_millis(150));
        let contents = term.contents();
        assert!(
            !contents.contains("visited") && !contents.contains("cargo-failures"),
            "the quiet surface carries no counters: {contents:?}"
        );
        assert!(contents.contains("analyzing output"), "{contents:?}");
        r.finish_and_clear();
    }

    /// The largest candidates observed are still recorded.
    ///
    /// The transient rows that used to draw them are gone, but the renderer
    /// still observes every reportable group, and `top_groups` remains the
    /// accessor for the largest ones. *Fails if:* the observation is dropped.
    #[test]
    fn renderer_caps_the_observed_candidates_at_five_largest_first() {
        let r = IndicatifRenderer::new(true, false);
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
        assert_eq!(
            o.reportable.lock().unwrap_or_else(|e| e.into_inner()).len(),
            1
        );
        assert_eq!(o.total_for(ScanPhase::Analysis), Some(4));
        assert_eq!(o.completed_count(ScanPhase::Analysis), 1);
    }

    #[test]
    fn should_show_progress_respects_flag_and_dumb_term() {
        assert!(!should_show_progress(true), "the explicit flag is absolute");
        // TERM is process-global, so the restore has to survive a panic: a
        // failure between set and restore left TERM=dumb for every later test
        // in this binary.
        let mut env = crate::progress::test_env::EnvRestore::acquire();
        env.set("TERM", "dumb");
        // Asserted on the gate, not on `should_show_progress`: stderr is not a
        // terminal under `cargo test`, so the public function returns false
        // either way and this assertion used to hold with the branch deleted.
        assert!(term_is_dumb(), "TERM=dumb must refuse progress");
        env.set("TERM", "xterm-256color");
        assert!(!term_is_dumb(), "only the literal `dumb` refuses progress");
    }

    #[test]
    fn renderer_refresh_bounded_independently_of_entry_count() {
        // Hidden renderer proves zero refreshes for many entries; visible
        // renderer throttles to 10 Hz via `maybe_draw` time gate. Here we
        // assert the hidden bound and that visible construction does not
        // allocate per entry (batched counters only).
        let hidden = IndicatifRenderer::new(true, false);
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
        let r = IndicatifRenderer::new(true, false);
        r.phase(ScanPhase::Discovery);
        assert_eq!(
            r.determinate_total_value(),
            0,
            "discovery must not set a total (would require wasteful pre-scan)"
        );
        r.units_total(ScanPhase::Analysis, 7);
        assert_eq!(r.determinate_total_value(), 7);
        // Re-announcing the phase already in effect is a no-op, not a reset.
        r.phase(ScanPhase::Analysis);
        assert_eq!(r.determinate_total_value(), 7);
        r.units_total(ScanPhase::Analysis, 7);
        // Determinate progress advances only via `unit_completed`, never via
        // legacy `group_measured` / `active_skipped` bookkeeping.
        r.group_measured(100);
        assert_eq!(r.determinate_done_value(), 0);
        r.unit_completed(ScanPhase::Analysis);
        assert_eq!(r.determinate_done_value(), 1);
    }

    #[test]
    fn unit_events_rescope_totals_on_a_phase_change_without_an_explicit_phase_call() {
        // M8: the renderer honours the phase carried by the event itself, so
        // cleanup totals can never inherit analysis counts even if a caller
        // forgets to announce the transition.
        let r = IndicatifRenderer::new(true, false);
        r.units_total(ScanPhase::Analysis, 5);
        r.unit_completed(ScanPhase::Analysis);
        r.unit_completed(ScanPhase::Analysis);
        assert_eq!(r.determinate_total_value(), 5);
        assert_eq!(r.determinate_done_value(), 2);
        // No `phase()` call: the first cleanup event re-scopes on its own.
        r.units_total(ScanPhase::CleanupExecute, 2);
        assert_eq!(r.determinate_total_value(), 2);
        assert_eq!(r.determinate_done_value(), 0);
        r.unit_completed(ScanPhase::CleanupExecute);
        assert_eq!(r.determinate_done_value(), 1);
    }

    #[test]
    fn phase_total_resets_so_cleanup_cannot_reuse_analysis_counts() {
        let r = IndicatifRenderer::new(true, false);
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
        let r = IndicatifRenderer::new(true, false);
        r.reportable_group(Path::new("/tmp/big"), 2048);
        let top = r.top_groups();
        assert_eq!(top.len(), 1);
        // Sizes render via the shared byte formatter.
        assert!(crate::report::format_bytes(2048).contains("2.00"));
        // Final clear occurs before report write and never fails.
        r.finish_and_clear();
        r.finish_and_clear();
    }

    #[test]
    fn renderer_error_falls_back_without_failing_scan() {
        // Construction is infallible; hidden fallback never fails a scan even
        // if terminal state is unavailable (non-TTY, dumb, or --no-progress).
        let hidden = IndicatifRenderer::new(true, false);
        hidden.phase(ScanPhase::Discovery);
        hidden.dirs_visited(10);
        hidden.reportable_group(Path::new("/a"), 1);
        hidden.finish_and_clear();
        assert_eq!(hidden.refresh_count(), 0);
    }

    /// Draw a live cleanup bar, as the cleanup phases do.
    fn draw_cleanup_bar(renderer: &IndicatifRenderer) {
        renderer.phase(ScanPhase::CleanupExecute);
        renderer.units_total(ScanPhase::CleanupExecute, 2);
        renderer.dirs_visited(3);
        renderer.reportable_group(Path::new("/tmp/group-a"), 4096);
        renderer.unit_completed(ScanPhase::CleanupExecute);
    }

    /// Let the 10 Hz draw target flush; the tests below read what was drawn.
    fn flush_draw_target() {
        std::thread::sleep(Duration::from_millis(150));
    }

    /// Bars the renderer still holds open.
    ///
    /// `BarState::drop` *finishes* a live bar and leaves its line on screen, so
    /// what matters on the failure path is that none is left live when `?`
    /// escapes. `coordinated_bar_count` cannot tell the two apart, and neither
    /// can the drawn frame: the in-memory frame is blank once the renderer is
    /// dropped whether or not the clear ran, so bar liveness is the only
    /// observable here that separates cleared from live.
    fn live_bars(renderer: &IndicatifRenderer) -> usize {
        std::iter::once(renderer.main.as_ref())
            .flatten()
            .filter(|bar| !bar.is_finished())
            .count()
    }

    /// A cleared renderer holds no live bar, so a later error cannot resurrect
    /// one and the renderer's own drop cannot leave a line behind.
    ///
    /// `run_cleanup` binds its fallible cleanup call, takes the bar down, and
    /// only then applies `?` (`src/main.rs`). That ordering is what keeps the
    /// failure path clean: with the bar still live, `BarState::drop` finishes
    /// it and the retained line stays on screen above the `cargo-cleanme: …`
    /// message.
    ///
    /// What this does **not** prove is the ordering itself. That lives in the
    /// binary target, out of reach from here, and the bar only draws when stderr
    /// is an attended terminal — a captured test stderr is not one, so even an
    /// end-to-end run of a failing cleanup drives the *hidden* renderer and
    /// never reaches this code. Proving the ordering needs a pty harness, which
    /// this crate has no dependency for. What is proven is the renderer half it
    /// relies on, and the input that breaks it: a `finish_and_clear` that leaves
    /// any bar live, or a path that draws again after the clear.
    #[test]
    fn finish_and_clear_leaves_no_live_bar_for_a_later_error_or_drop() {
        let (r, term) = IndicatifRenderer::new_in_memory_for_test(false);
        draw_cleanup_bar(&r);
        flush_draw_target();
        // Preconditions, or every assertion below is true by construction: the
        // bar must have drawn for the cleared frame to mean anything, and it
        // must have been live for "no live bars" to mean anything.
        assert!(
            !term.contents().trim().is_empty(),
            "the cleanup bar must draw before the clear is exercised"
        );
        assert_eq!(
            live_bars(&r),
            1,
            "the single coordinated bar is live before the clear"
        );

        // Mirrors `run_cleanup` on the failure path: the fallible call is bound,
        // the bar is taken down, and only then is the `?` applied.
        let cleanup: Result<(), crate::error::AppError> = Err(crate::error::AppError::Io(
            std::io::Error::other("cleanup failed"),
        ));
        r.finish_and_clear();
        assert_eq!(
            term.contents().trim(),
            "",
            "the clear must take the drawn frame down"
        );
        assert_eq!(
            live_bars(&r),
            0,
            "the clear must leave no live bar for drop to finish into a retained line"
        );

        // The `?` that follows: it must resurrect neither a live bar nor a draw.
        let refreshes = r.refresh_count();
        let outcome = cleanup;
        assert!(outcome.is_err(), "the simulated cleanup must fail");
        assert_eq!(
            live_bars(&r),
            0,
            "an error raised after the clear must not leave a live bar"
        );
        assert_eq!(
            r.refresh_count(),
            refreshes,
            "an error raised after the clear must not redraw"
        );

        // The error then leaves `run_cleanup`, dropping the renderer. What that
        // drop draws is not observable from here — the frame is blank after a
        // drop either way — so the assertions above, not this one, are what hold
        // the property; this only catches a draw resurrected by the drop itself.
        drop(r);
        flush_draw_target();
        assert_eq!(term.contents().trim(), "");
    }
}
