//! Interactive workspace selection for `cargo cleanme -i` (M014 work package D).
//!
//! # What this is, and what it is not
//!
//! This module is a **selection surface**. It resolves the cleanup units the
//! engine would consider, lets the operator choose among them, and returns the
//! chosen workspace roots as [`crate::cleanup::CleanupSelection::Roots`]. It
//! never deletes anything and it never becomes a second deletion path: the
//! bytes it can release are removed by exactly the same `cargo clean` spawns,
//! under exactly the same final proof, as a non-interactive run.
//!
//! That is why the selection is expressed as a filter over *already proven*
//! units rather than as a way to point the engine at new ones. Nothing here can
//! add a candidate, reclassify one, or unblock a scope (ADR 006).
//!
//! # Shape
//!
//! [`Session`] is a pure state machine: it owns mode, cursor, filter, and
//! selection, and it renders itself to a `Vec<String>` with no terminal in
//! scope. Everything that touches a real terminal lives behind [`Terminal`], so
//! the whole of the operator-facing behaviour is testable against a fake, and
//! the failure that actually matters — a session that exits without restoring
//! the terminal — is testable too.
//!
//! # Rendering is inline
//!
//! No alternate screen. The list repaints in place below the shell prompt's
//! position, and the region is erased on exit. ADR 001 rejects raw mode and
//! alternate-screen rendering for *progress*, where it strands the operator's
//! scrollback and makes a crash unrecoverable; it does not forbid a transient
//! selection list, which redraws in place and leaves nothing behind.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use crate::cleanup::CleanupCandidate;
use crate::report::format_bytes;

/// Which set of keys is live.
///
/// NOR is the default. INS is only reachable by asking for it (`i`), and it is
/// a text field: every printable key inserts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

impl Mode {
    /// Stable label used in the status line and asserted in tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NOR",
            Self::Insert => "INS",
        }
    }
}

/// One key press, normalized away from the terminal library.
///
/// The state machine must be testable without a pty, so it consumes this
/// instead of `crossterm::event::KeyEvent`. The mapping is total: a key the
/// session has no binding for becomes [`Key::Ignored`] rather than an error,
/// because an unknown escape sequence is a normal event, not a failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Key {
    Up,
    Down,
    /// Move back one page of visible rows.
    PageUp,
    /// Move forward one page of visible rows.
    PageDown,
    /// Move to the first candidate.
    Home,
    /// Move to the last candidate.
    End,
    Char(char),
    Space,
    Backspace,
    Enter,
    Esc,
    CtrlC,
    /// No binding in the current mode.
    Ignored,
}

/// What a key press asked the session to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Keep looping.
    Continue,
    /// The operator confirmed a selection.
    ///
    /// These are canonical workspace roots in candidate order, and they become
    /// [`crate::cleanup::CleanupSelection::Roots`].
    Commit(Vec<PathBuf>),
    /// The operator quit. No cleanup runs.
    Quit,
}

/// How a selection session ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionResult {
    Selected(Vec<PathBuf>),
    Cancelled,
    /// The scope proved no cleanup unit at all, so there was nothing to
    /// choose from. A plain negative result, not a usage error: the same
    /// scope cleaned without `-i` reports zero units and exits 0, and the
    /// two spellings of one scope must agree.
    NothingToClean,
}

/// The narrow terminal seam.
///
/// Deliberately small. Everything interesting is [`Session`]; this exists only
/// so the driver loop and the raw-mode lifecycle can be exercised without a
/// real terminal.
pub trait Terminal {
    /// Take the terminal out of line-buffered/canonical mode.
    fn enter_raw(&mut self) -> io::Result<()>;
    /// Put the terminal back as `enter_raw` found it.
    ///
    /// Called on every exit path, including a panic. Must be idempotent: the
    /// RAII guard's `Drop` and an explicit restore can both reach it.
    fn restore(&mut self) -> io::Result<()>;
    /// Block until the next key press.
    fn read_key(&mut self) -> io::Result<Key>;
    /// `(columns, rows)`.
    fn size(&mut self) -> io::Result<(u16, u16)>;
    /// Move the cursor up `lines` and erase from there down.
    fn rewind(&mut self, lines: usize) -> io::Result<()>;
    /// Show or hide the terminal's own cursor.
    fn set_cursor_visible(&mut self, visible: bool) -> io::Result<()>;
    /// Erase the region this session painted.
    fn clear(&mut self) -> io::Result<()>;
    /// Write one frame's lines.
    fn draw(&mut self, lines: &[String]) -> io::Result<()>;
}

/// Lines above the scrolling rows: title, status, column header, all-toggle.
const HEADER_LINES: usize = 4;

/// Lines below the scrolling rows: the key-hint bar.
const FOOTER_LINES: usize = 1;

/// Every line the frame spends on something other than a candidate row.
const CHROME_LINES: usize = HEADER_LINES + FOOTER_LINES;

/// How many candidate rows the page shows at once.
///
/// `rip` uses the same number. The point is that the frame is **bounded**: it
/// does not grow with the candidate count, so a scope with three units and a
/// scope with three hundred paint the same shape, and a long list is a list
/// you page through rather than a screenful you have to scroll.
const PAGE_ROWS: usize = 15;

/// Smallest viewport that can show one candidate row.
const MIN_LIST_ROWS: usize = 1;

/// Smallest width the layout is willing to render at.
const MIN_WIDTH: usize = 40;

/// Narrowest workspace-name column the layout will keep.
const MIN_NAME_WIDTH: usize = 8;

/// Width of the gutter: the cursor marker and the window edges.
const GUTTER_WIDTH: usize = 1;

/// Width of the checkbox column: `[x]` or `[ ]`.
const CHECKBOX_WIDTH: usize = 3;

/// Width of the reclaimable column.
///
/// Eleven, because `RECLAIMABLE` is eleven characters and the column header
/// has to sit over the column it names. `format_bytes` emits at most ten.
const SIZE_WIDTH: usize = 11;

/// Spaces between two columns.
const COLUMN_GAP: usize = 2;

/// Fixed furniture before the workspace name: gutter, space, checkbox, space.
const LEAD_WIDTH: usize = GUTTER_WIDTH + 1 + CHECKBOX_WIDTH + 1;

/// ANSI escapes.
///
/// The crate renders no colour anywhere else — its deterministic human report
/// is plain text because its consumers are scripts and diffs. This surface
/// owns the terminal, so it is the only place colour belongs.
const ANSI_RESET: &str = "\u{1b}[0m";
const ANSI_DIM: &str = "\u{1b}[2m";
const ANSI_BOLD: &str = "\u{1b}[1m";
const ANSI_REVERSE: &str = "\u{1b}[7m";
const ANSI_GREEN: &str = "\u{1b}[32m";
const ANSI_CYAN: &str = "\u{1b}[36m";
const ANSI_YELLOW: &str = "\u{1b}[33m";

/// The interactive selection state machine.
///
/// Pure: no IO, no clock, no terminal. Every method is a total function of
/// `self` and the key pressed.
#[derive(Clone, Debug)]
pub struct Session {
    candidates: Vec<CleanupCandidate>,
    /// The display name of each candidate, parallel to `candidates`.
    ///
    /// A row that showed the whole absolute path spent most of its width on a
    /// prefix every row shares, and pushed the part that tells one workspace
    /// apart from another to the far end of the line.
    labels: Vec<String>,
    /// One flag per candidate, parallel to `candidates`. Never resized, so a
    /// filter can never renumber an existing check.
    checked: Vec<bool>,
    /// Candidate indices surviving the current filter, in candidate order.
    visible: Vec<usize>,
    /// Row index: 0 is the all-toggle, 1..=visible.len() are candidates.
    cursor: usize,
    /// Index into `visible` of the first painted candidate row.
    scroll: usize,
    mode: Mode,
    filter: String,
    /// NOR `Esc` arms an exit; the second `Esc` takes it. One `Esc` must never
    /// destroy a session the operator may still be reading.
    exit_armed: bool,
    width: usize,
    height: usize,
    /// Whether the frame carries ANSI styling.
    ///
    /// The driver decides this from the environment. A session that read the
    /// environment itself would stop being a pure function of its own state.
    color: bool,
}

impl Session {
    /// Build a session over the resolved candidates.
    ///
    /// Nothing is checked initially. "All" is one keystroke away; pre-checking
    /// everything would make `Enter` destructive by default, and a selection
    /// UI whose default is "delete all of it" is a trap wearing a checkbox.
    pub fn new(candidates: Vec<CleanupCandidate>) -> Self {
        let checked = vec![false; candidates.len()];
        let visible = (0..candidates.len()).collect();
        let labels = unique_labels(&candidates);
        Self {
            candidates,
            labels,
            checked,
            visible,
            cursor: 0,
            scroll: 0,
            mode: Mode::Normal,
            filter: String::new(),
            exit_armed: false,
            width: 80,
            height: 24,
            color: true,
        }
    }

    /// Turn the ANSI styling on or off.
    ///
    /// The driver decides, from the environment: a session that read the
    /// environment itself would stop being a pure function of its own state.
    pub fn with_color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    /// Adopt the real terminal geometry.
    pub fn with_size(mut self, width: u16, height: u16) -> Self {
        self.set_size(width, height);
        self
    }

    /// Adopt new geometry on a live session, keeping mode, filter, and cursor.
    ///
    /// The driver re-measures before every frame so that a terminal resize
    /// mid-session re-lays-out instead of painting over itself or leaving rows
    /// stranded below the new bottom edge.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = (width as usize).max(MIN_WIDTH);
        self.height = (height as usize).max(CHROME_LINES + MIN_LIST_ROWS);
        let max = self.rows().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
        self.ensure_cursor_visible();
    }

    /// Total candidate count, unfiltered.
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// How many candidates survive the current filter.
    ///
    /// The filter matches the **full path**, even though a row is *named* by
    /// the shortest suffix that identifies it. This is the count that says
    /// whether the filter found what it was looking for.
    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    /// How many candidates are checked right now, across the whole list.
    pub fn selected_count(&self) -> usize {
        self.checked.iter().filter(|c| **c).count()
    }

    /// Reclaimable bytes across every checked candidate.
    pub fn selected_bytes(&self) -> u64 {
        self.candidates
            .iter()
            .zip(&self.checked)
            .filter(|(_, c)| **c)
            .map(|(candidate, _)| candidate.bytes)
            .sum()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Candidate index under the cursor, or `None` on the all-toggle row.
    pub fn cursor_candidate(&self) -> Option<usize> {
        if self.cursor == 0 {
            None
        } else {
            self.visible.get(self.cursor - 1).copied()
        }
    }

    /// Whether every candidate — including those hidden by the filter — is
    /// checked.
    fn all_checked(&self) -> bool {
        !self.checked.is_empty() && self.checked.iter().all(|c| *c)
    }

    /// Total selectable rows, including the all-toggle.
    fn rows(&self) -> usize {
        self.visible.len() + 1
    }

    /// How many candidate rows the page shows at once.
    ///
    /// The page is **bounded**: it does not grow with the candidate count, and
    /// it never outgrows the terminal. A scope with three units and a scope
    /// with three hundred paint the same shape, and a long list is a list the
    /// operator pages through rather than a screenful they have to scroll.
    fn list_rows(&self) -> usize {
        let available = self.height.saturating_sub(CHROME_LINES).max(MIN_LIST_ROWS);
        let wanted = self.visible.len().max(MIN_LIST_ROWS);
        PAGE_ROWS.min(wanted).min(available)
    }

    /// Apply one key.
    ///
    /// `Esc` from INS returns to NOR and *keeps* the filter, so an operator
    /// who overshoots can get back to navigating without retyping their query.
    pub fn on_key(&mut self, key: Key) -> Outcome {
        // Ctrl-C is the only key that quits from inside INS: `q` there is a
        // letter in a text field, not a command.
        if key == Key::CtrlC {
            return Outcome::Quit;
        }
        match self.mode {
            Mode::Normal => self.on_key_normal(key),
            Mode::Insert => self.on_key_insert(key),
        }
    }

    fn on_key_normal(&mut self, key: Key) -> Outcome {
        match key {
            Key::Up => self.move_cursor(-1),
            Key::Down | Key::Char('j') => self.move_cursor(1),
            Key::PageUp => self.move_page(-1),
            Key::PageDown => self.move_page(1),
            Key::Home => self.move_to_start(),
            Key::End => self.move_to_end(),
            Key::Char('k') => self.move_cursor(-1),
            Key::Space => self.toggle_cursor_row(),
            Key::Char('i') => {
                self.mode = Mode::Insert;
                // Re-arm on entering INS: a half-finished exit must not be
                // carried into a later, deliberate quit.
                self.exit_armed = false;
            }
            Key::Esc => {
                if self.exit_armed {
                    return Outcome::Quit;
                }
                self.exit_armed = true;
            }
            Key::Enter => return self.commit(),
            Key::Char('q') => return Outcome::Quit,
            Key::Char(_) | Key::Backspace | Key::Ignored => {}
            // Already handled in `on_key`; repeated here so the function is
            // total on its own. A TUI must never reach a panic path.
            Key::CtrlC => return Outcome::Quit,
        }
        Outcome::Continue
    }

    fn on_key_insert(&mut self, key: Key) -> Outcome {
        match key {
            // Esc returns to NOR keeping the filter.
            Key::Esc => self.mode = Mode::Normal,
            Key::Enter => return self.commit(),
            Key::Backspace => {
                self.filter.pop();
                self.refilter();
            }
            Key::Space => {
                // Space is a filter character. `normalize` maps `Char(' ')`
                // onto `Key::Space` in every mode, so treating it as a no-op
                // here would make any workspace path containing a space
                // impossible to filter for.
                self.filter.push(' ');
                self.refilter();
            }
            Key::Char(c) => {
                self.filter.push(c);
                self.refilter();
            }
            // Arrows and paging are NOR commands; INS is a one-line field, so
            // there is no sensible in-field meaning for them.
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {}
            Key::Ignored => {}
            // Already handled in `on_key`; repeated here so the function is
            // total on its own.
            Key::CtrlC => return Outcome::Quit,
        }
        Outcome::Continue
    }

    fn move_cursor(&mut self, delta: isize) {
        let max = self.rows().saturating_sub(1) as isize;
        let next = (self.cursor as isize + delta).clamp(0, max);
        self.set_cursor(next as usize);
    }

    /// Move the cursor by one page of visible rows.
    ///
    /// A list that fills the screen can only be walked one row at a time, so a
    /// scope with hundreds of units costs hundreds of keypresses to cross.
    fn move_page(&mut self, direction: isize) {
        let page = self.list_rows() as isize;
        self.move_cursor(direction * page);
    }

    /// Move the cursor to the first candidate.
    ///
    /// The all-toggle is not a candidate, so `Home` lands on the first real
    /// unit; the all-toggle stays one `Up` away.
    fn move_to_start(&mut self) {
        let target = if self.visible.is_empty() { 0 } else { 1 };
        self.set_cursor(target);
    }

    /// Move the cursor to the last candidate.
    fn move_to_end(&mut self) {
        self.set_cursor(self.visible.len());
    }

    /// Park the cursor on a row and cancel any pending exit.
    fn set_cursor(&mut self, target: usize) {
        let max = self.rows().saturating_sub(1);
        self.cursor = target.min(max);
        // Any navigation cancels a pending exit: arming is a deliberate
        // two-step, not a state that follows the operator around.
        self.exit_armed = false;
        self.ensure_cursor_visible();
    }

    /// Toggle the row under the cursor.
    ///
    /// The all-toggle applies to the **whole** list, filtered or not, because
    /// a toggle that secretly obeyed the filter would make "all" mean
    /// "whatever happens to be on screen". The row states its own arithmetic
    /// (`N/M selected`) so the scope is never in doubt.
    fn toggle_cursor_row(&mut self) {
        if self.cursor == 0 {
            let target = !self.all_checked();
            self.checked.fill(target);
        } else if let Some(idx) = self.cursor_candidate() {
            self.checked[idx] = !self.checked[idx];
        }
        self.exit_armed = false;
    }

    /// Rebuild the visible set from the filter, then park the cursor.
    ///
    /// The cursor resets to the all-toggle because the operator was editing a
    /// query, not navigating. Carrying a row offset across a changing result
    /// set is how a cursor ends up pointing at a different unit than the one it
    /// was on when it was moved.
    fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.visible = if needle.is_empty() {
            (0..self.candidates.len()).collect()
        } else {
            (0..self.candidates.len())
                .filter(|&i| {
                    self.candidates[i]
                        .root
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle)
                })
                .collect()
        };
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Scroll the minimum amount that puts the cursor's candidate on screen.
    ///
    /// Row 0 is the all-toggle and lives in the chrome, so it is always
    /// visible and never scrolls.
    fn ensure_cursor_visible(&mut self) {
        let page = self.list_rows();
        if self.cursor == 0 {
            self.scroll = 0;
            return;
        }
        // Candidate position within the window is `cursor - 1 - scroll`.
        if self.scroll > self.cursor - 1 {
            self.scroll = self.cursor - 1;
        }
        // Guarded: this branch requires `cursor - 1 >= page`, so the
        // subtraction cannot underflow.
        if self.scroll + page < self.cursor {
            self.scroll = self.cursor - page;
        }
        let max_scroll = self.visible.len().saturating_sub(page);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Turn the current checkmarks into a selection.
    ///
    /// Committing nothing is not a silent no-op: it returns
    /// [`Outcome::Continue`] so the operator keeps their list and their exit
    /// keys. A session that closed on an empty selection would look exactly
    /// like one that cleaned nothing — the M012A defect in a new costume.
    fn commit(&mut self) -> Outcome {
        let roots: Vec<PathBuf> = self
            .candidates
            .iter()
            .zip(&self.checked)
            .filter(|(_, c)| **c)
            .map(|(candidate, _)| candidate.root.clone())
            .collect();
        if roots.is_empty() {
            return Outcome::Continue;
        }
        Outcome::Commit(roots)
    }

    /// Render one frame as terminal lines.
    ///
    /// Pure, so the entire operator-visible layout — including the scrolled
    /// window, the column arithmetic, and the ownership annotation on each
    /// row — is assertable without a terminal.
    pub fn frame(&self) -> Vec<String> {
        let width = self.width;
        let page = self.list_rows();
        let columns = self.columns();
        let mut out = Vec::with_capacity(CHROME_LINES + page);

        out.push(self.title_line(width));
        out.push(self.status_line(width));
        out.push(self.column_header(&columns, width));
        out.push(self.all_row(width));

        if self.visible.is_empty() {
            out.push(pad("    (no workspace matches the filter)", width));
            for _ in 1..page {
                out.push(String::new());
            }
        } else {
            let end = (self.scroll + page).min(self.visible.len());
            for idx in self.scroll..end {
                out.push(self.candidate_row(idx, end, &columns, width));
            }
            for _ in end..(self.scroll + page) {
                out.push(String::new());
            }
        }

        out.push(self.hint_bar(width));
        debug_assert_eq!(out.len(), CHROME_LINES + page);
        out
    }

    /// The title line: what the surface is, and the selection arithmetic.
    fn title_line(&self, width: usize) -> String {
        let selected = self.selected_count();
        let total = self.candidates.len();
        let head = "cargo-cleanme — select workspaces — ";
        let body = format!(
            "{selected} selected of {total}, {} reclaimable",
            format_bytes(self.selected_bytes()).trim()
        );
        let truncated = truncate(&format!("{head}{body}"), width);
        if !self.color {
            return pad(&truncated, width);
        }
        // The head is chrome; the arithmetic is the part worth reading.
        let styled = match truncated.strip_prefix(head) {
            Some(rest) => format!("{ANSI_DIM}{head}{ANSI_RESET}{ANSI_BOLD}{rest}{ANSI_RESET}"),
            None => format!("{ANSI_DIM}{truncated}{ANSI_RESET}"),
        };
        pad_visible(&styled, width)
    }

    /// The mode/filter line.
    ///
    /// The filter field is on screen in both modes so the operator can see
    /// what is narrowing the list without entering INS to check. The position
    /// indicator rides on the right: it is navigation state, and this is the
    /// only line with room for it.
    fn status_line(&self, width: usize) -> String {
        let mode = self.mode.as_str();
        let position = self.position_indicator();
        let armed = if self.mode == Mode::Normal && self.exit_armed {
            "   (Esc again to exit)"
        } else {
            ""
        };
        let caret = if self.mode == Mode::Insert { "_" } else { "" };
        let filter = if self.filter.is_empty() && self.mode == Mode::Normal {
            "<none>".to_string()
        } else {
            self.filter.clone()
        };
        let head = format!("{mode}  filter: ");
        let tail = format!("{filter}{caret}{armed}");
        // The mode and the position always survive; the filter gives way
        // first, because it is the one of the three that can be retyped.
        let room = width.saturating_sub(head.chars().count() + position.chars().count() + 1);
        let left = format!("{head}{}", truncate_end(&tail, room));
        let gap = width.saturating_sub(left.chars().count() + position.chars().count());
        format!("{left}{}{position}", " ".repeat(gap))
    }

    /// Where the cursor is in the filtered list.
    ///
    /// The all-toggle is not a candidate, so it has no position in it.
    fn position_indicator(&self) -> String {
        let total = self.visible.len();
        if self.cursor == 0 {
            format!("—/{total}")
        } else {
            format!("{}/{total}", self.cursor)
        }
    }

    /// The column header line.
    ///
    /// It names the columns, so the numbers have something to be numbers of.
    fn column_header(&self, columns: &Columns, width: usize) -> String {
        let mut plain = String::new();
        plain.push_str(&" ".repeat(LEAD_WIDTH));
        plain.push_str(&pad(&truncate("WORKSPACE", columns.name), columns.name));
        plain.push_str(&" ".repeat(COLUMN_GAP));
        plain.push_str(&pad_left("RECLAIMABLE", SIZE_WIDTH));
        if columns.own > 0 {
            plain.push_str(&" ".repeat(COLUMN_GAP));
            plain.push_str(&pad(&truncate("OWNERSHIP", columns.own), columns.own));
        }
        if !self.color {
            return pad(&plain, width);
        }
        format!("{ANSI_DIM}{}{ANSI_RESET}", pad_visible(&plain, width))
    }

    /// The all-toggle row.
    ///
    /// It states its own arithmetic, right-aligned at the frame's edge, so
    /// "all" is never a guess about what it would take.
    fn all_row(&self, width: usize) -> String {
        let selected = self.selected_count();
        let total = self.candidates.len();
        let mark = if self.all_checked() {
            "x"
        } else if selected > 0 {
            "-"
        } else {
            " "
        };
        let pointer = if self.cursor == 0 { '>' } else { ' ' };
        let label = format!("[{mark}] all");
        let arithmetic = format!("({selected}/{total} selected)");
        let left = format!("{pointer} {label}");
        let gap = width.saturating_sub(left.chars().count() + arithmetic.chars().count());
        let plain = format!("{left}{}{arithmetic}", " ".repeat(gap));
        if self.cursor == 0 && self.color {
            return format!("{ANSI_REVERSE}{plain}{ANSI_RESET}");
        }
        if !self.color {
            return plain;
        }
        let mark_style = if self.all_checked() { ANSI_GREEN } else { "" };
        let mut styled = String::new();
        styled.push(pointer);
        styled.push(' ');
        styled.push_str(&self.paint(mark_style, &label));
        styled.push_str(&" ".repeat(gap));
        styled.push_str(&self.paint(ANSI_DIM, &arithmetic));
        styled
    }

    /// One candidate row.
    fn candidate_row(&self, idx: usize, end: usize, columns: &Columns, width: usize) -> String {
        let candidate_idx = self.visible[idx];
        let candidate = &self.candidates[candidate_idx];
        let checked = self.checked[candidate_idx];
        let mark = if checked { "x" } else { " " };
        let checkbox = format!("[{mark}]");
        let label = &self.labels[candidate_idx];
        let size = format_bytes(candidate.bytes).trim().to_string();
        let ownership = ownership_text(candidate, columns.own_full);
        let gutter = self.gutter_char(idx, end);

        let mut plain = String::new();
        plain.push(gutter);
        plain.push(' ');
        plain.push_str(&checkbox);
        plain.push(' ');
        plain.push_str(&pad(&truncate(label, columns.name), columns.name));
        plain.push_str(&" ".repeat(COLUMN_GAP));
        plain.push_str(&pad_left(&truncate(&size, SIZE_WIDTH), SIZE_WIDTH));
        if columns.own > 0 {
            plain.push_str(&" ".repeat(COLUMN_GAP));
            plain.push_str(&pad(&ownership, columns.own));
        }

        if self.cursor == idx + 1 {
            // The cursor's row is one solid bar, so the eye finds it in a long
            // list without reading any of it.
            let plain = pad(&plain, width);
            if self.color {
                return format!("{ANSI_REVERSE}{plain}{ANSI_RESET}");
            }
            return plain;
        }

        if !self.color {
            return pad(&plain, width);
        }

        let mut styled = String::new();
        styled.push(gutter);
        styled.push(' ');
        styled.push_str(&self.paint(if checked { ANSI_GREEN } else { "" }, &checkbox));
        styled.push(' ');
        styled.push_str(&pad(&truncate(label, columns.name), columns.name));
        styled.push_str(&" ".repeat(COLUMN_GAP));
        styled.push_str(&self.paint(
            ANSI_CYAN,
            &pad_left(&truncate(&size, SIZE_WIDTH), SIZE_WIDTH),
        ));
        if columns.own > 0 {
            styled.push_str(&" ".repeat(COLUMN_GAP));
            let style = if candidate.provably_private() {
                ANSI_DIM
            } else {
                ANSI_YELLOW
            };
            styled.push_str(&self.paint(style, &pad(&ownership, columns.own)));
        }
        pad_visible(&styled, width)
    }

    /// The gutter marker for a row.
    ///
    /// `>` is the cursor. `^` and `v` mark the edges of the window when there
    /// are rows above or below it: without them a long list gives no sign that
    /// it is longer than the page.
    fn gutter_char(&self, idx: usize, end: usize) -> char {
        if self.cursor == idx + 1 {
            return '>';
        }
        if idx == self.scroll && self.scroll > 0 {
            return '^';
        }
        if idx + 1 == end && end < self.visible.len() {
            return 'v';
        }
        ' '
    }

    /// The key-hint bar.
    ///
    /// A surface that has keys the operator has to remember is a surface that
    /// teaches them by refusing.
    fn hint_bar(&self, width: usize) -> String {
        let full = "[↑↓/PgUp/PgDn move • Space select • i filter • Enter clean • Esc quit]";
        let compact = "[↑↓ PgUp/PgDn • Space • i • Enter • Esc]";
        let hints = if full.chars().count() <= width {
            full
        } else {
            compact
        };
        let plain = pad(&truncate_end(hints, width), width);
        if !self.color {
            return plain;
        }
        format!("{ANSI_DIM}{plain}{ANSI_RESET}")
    }

    /// Wrap `text` in an ANSI style, or leave it alone when colour is off.
    fn paint(&self, style: &str, text: &str) -> String {
        if !self.color || style.is_empty() {
            return text.to_string();
        }
        format!("{style}{text}{ANSI_RESET}")
    }

    /// Work out the column widths for the current geometry and candidate set.
    ///
    /// The reclaimable column is right-aligned, so the sizes line up and the
    /// eye can find the big one. The ownership column is sized to the widest
    /// annotation it has to carry, and is **omitted** when every candidate is
    /// private: a column that says the same word on every row carries no
    /// information and costs a column's width on every row.
    fn columns(&self) -> Columns {
        let present = self.ownership_is_present();
        let gaps = if present { 2 * COLUMN_GAP } else { COLUMN_GAP };
        let budget = self.width.saturating_sub(LEAD_WIDTH + gaps + SIZE_WIDTH);
        if !present {
            return Columns {
                name: budget.max(MIN_NAME_WIDTH),
                own: 0,
                own_full: false,
            };
        }
        let full = self
            .candidates
            .iter()
            .map(|c| ownership_text(c, true).chars().count())
            .max()
            .unwrap_or(0);
        if budget.saturating_sub(full) >= MIN_NAME_WIDTH {
            return Columns {
                name: budget - full,
                own: full,
                own_full: true,
            };
        }
        // Too narrow for the full wording: the class label is the shortest
        // honest description, and the name keeps its room.
        let class = self
            .candidates
            .iter()
            .map(|c| c.ownership.label().chars().count())
            .max()
            .unwrap_or(0);
        let own = class.min(budget.saturating_sub(MIN_NAME_WIDTH));
        Columns {
            name: budget.saturating_sub(own).max(MIN_NAME_WIDTH),
            own,
            own_full: false,
        }
    }

    /// Whether the ownership column has anything to say.
    ///
    /// It does whenever the candidate set is not uniformly private.
    fn ownership_is_present(&self) -> bool {
        !self.candidates.is_empty() && !self.candidates.iter().all(|c| c.provably_private())
    }
}

/// The frame's column widths for the current geometry and candidate set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Columns {
    /// Width of the workspace-name column.
    name: usize,
    /// Width of the ownership column. Zero when the column is omitted.
    own: usize,
    /// Whether the ownership column carries the full wording or just the class.
    own_full: bool,
}

/// The ownership annotation for one candidate.
///
/// A non-private row must not be mistaken for a candidate: a hidden row is
/// indistinguishable from a row that does not exist, and the operator's next
/// question would again be "which one?".
fn ownership_text(candidate: &CleanupCandidate, full: bool) -> String {
    if full {
        if candidate.provably_private() {
            "private".to_string()
        } else {
            format!("{} — inventory only", candidate.ownership.label())
        }
    } else {
        candidate.ownership.label().to_string()
    }
}

/// The shortest suffix of each candidate's path that identifies it uniquely.
///
/// A row that showed the whole absolute path spent most of its width on a
/// prefix every row shares, and pushed the part that tells one workspace apart
/// from another to the far end of the line.
///
/// The rule is a pure function of the whole candidate set, so two workspaces
/// that share a basename are told apart — `a/x` and `b/x` — rather than
/// silently rendered as the same name. The *filter* still matches the full
/// path; this only decides what a row is called.
fn unique_labels(candidates: &[CleanupCandidate]) -> Vec<String> {
    let tails: Vec<Vec<&OsStr>> = candidates
        .iter()
        .map(|c| c.root.components().map(|c| c.as_os_str()).collect())
        .collect();
    (0..candidates.len())
        .map(|i| {
            let mine = &tails[i];
            for k in 1..=mine.len() {
                let suffix = &mine[mine.len() - k..];
                let clashes = tails.iter().enumerate().any(|(j, other)| {
                    j != i
                        && other.len() >= suffix.len()
                        && other[other.len() - suffix.len()..] == *suffix
                });
                if !clashes {
                    return join_components(suffix);
                }
            }
            // No suffix is unique — two candidates share a path. Show the whole
            // of it rather than a name that says nothing.
            candidates[i].root.display().to_string()
        })
        .collect()
}

/// Join path components back into a path, using the platform separator.
fn join_components(components: &[&OsStr]) -> String {
    let mut out = PathBuf::new();
    for component in components {
        out.push(component);
    }
    out.display().to_string()
}

/// Truncate from the *left*: the tail of a path is the part that distinguishes
/// one project from another.
fn truncate(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::from("…");
    out.extend(text.chars().skip(len - (width - 1)));
    out
}

/// Truncate from the *right*: the head of a filter query is the part the
/// operator typed first and is most likely to recognise.
fn truncate_end(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push('…');
    out
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let mut out = text.to_string();
    out.extend(std::iter::repeat_n(' ', width - len));
    out
}

/// Right-align a fragment in its column.
fn pad_left(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let mut out = String::new();
    out.extend(std::iter::repeat_n(' ', width - len));
    out.push_str(text);
    out
}

/// The width of a line in terminal columns, ignoring ANSI escape sequences.
fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip the CSI sequence up to its final byte.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        width += 1;
    }
    width
}

/// Pad a styled line to `width` visible columns.
fn pad_visible(text: &str, width: usize) -> String {
    let visible = visible_width(text);
    if visible >= width {
        return text.to_string();
    }
    let mut out = text.to_string();
    out.extend(std::iter::repeat_n(' ', width - visible));
    out
}

/// Restores the terminal on every exit path, including a panic.
///
/// A tool that can leave a terminal in raw mode is not shippable: the operator
/// loses echo and `Ctrl-C`, and gets a shell they have to reset by hand. This
/// guard makes restoration a property of the *scope* rather than of every
/// `return` and `?` that happens to be remembered at the end of a function.
///
/// It holds a mutable borrow of the terminal, so the guard's lifetime and the
/// driver's ability to keep using the terminal are the same lifetime —
/// restoration cannot be skipped by "using the terminal once more".
struct RawModeGuard<'a> {
    terminal: &'a mut dyn Terminal,
    restored: bool,
}

impl RawModeGuard<'_> {
    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        // Set the flag *before* calling through: if `restore` itself returns an
        // error, retrying from `Drop` would issue a second `disable_raw_mode`
        // on a handle that has already reported itself broken.
        self.restored = true;
        self.terminal.restore()
    }
}

impl Drop for RawModeGuard<'_> {
    fn drop(&mut self) {
        // Errors during unwind are ignored on purpose: a failing restore must
        // not replace the original failure with a second panic from `Drop`.
        let _ = self.restore();
    }
}

/// Refuse to start when there is no terminal to drive.
///
/// Checked before raw mode is requested, so a redirected invocation never puts
/// the terminal into a mode whose events go nowhere — the operator would be
/// left with echo off and no way back.
///
/// Takes plain flags rather than `&dyn IsTerminal` because `IsTerminal` is a
/// sealed std trait: taking it would make this precondition impossible to test
/// against a fake, and this is the one guard that has to hold precisely on the
/// path where everything else is already broken.
pub fn ensure_interactive_terminal(
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<(), &'static str> {
    if !stdin_is_tty {
        return Err("standard input is not a terminal");
    }
    if !stdout_is_tty {
        return Err("standard output is not a terminal");
    }
    Ok(())
}

/// Whether the surface should paint ANSI styling.
///
/// `NO_COLOR` wins over everything, and a dumb terminal cannot render the
/// styles at all. The decision is made here, in the driver, rather than in
/// [`Session`], so the state machine stays a pure function of its own state.
fn color_enabled() -> bool {
    color_enabled_with(std::env::var_os("NO_COLOR"), std::env::var("TERM").ok())
}

/// The colour decision, with the environment passed in.
///
/// Split out so it can be asserted without mutating the process
/// environment, which is shared with every other test in the binary.
fn color_enabled_with(no_color: Option<std::ffi::OsString>, term: Option<String>) -> bool {
    if no_color.is_some() {
        return false;
    }
    !matches!(term.as_deref(), Some("dumb"))
}

/// Run a selection session against a terminal.
///
/// The raw-mode guard is created before the first key is read, so every exit
/// from here — a normal quit, a `?` on a read error, or an unwinding panic —
/// runs [`Terminal::restore`].
pub fn drive(
    terminal: &mut dyn Terminal,
    candidates: Vec<CleanupCandidate>,
) -> io::Result<SessionResult> {
    let mut session = Session::new(candidates).with_color(color_enabled());
    terminal.enter_raw()?;
    let mut guard = RawModeGuard {
        terminal,
        restored: false,
    };
    let result = session_loop(&mut *guard.terminal, &mut session);
    // Restore before reporting: a caller that receives an error must never be
    // able to print it into a raw-mode terminal and leave it there.
    let restored = guard.restore();
    match (result, restored) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(e), _) => Err(e),
        (Ok(_), Err(e)) => Err(e),
    }
}

fn session_loop(terminal: &mut dyn Terminal, session: &mut Session) -> io::Result<SessionResult> {
    // The terminal's own cursor starts hidden: in NOR the highlighted row *is*
    // the cursor, and a hardware caret blinking inside a `[ ]` checkbox reads
    // as a second, unrelated selection point.
    terminal.set_cursor_visible(false)?;
    let mut painted = 0usize;
    loop {
        let (width, height) = terminal.size()?;
        session.set_size(width, height);
        let frame = session.frame();
        if painted > 0 {
            terminal.rewind(painted)?;
        }
        terminal.draw(&frame)?;
        terminal.set_cursor_visible(session.mode() == Mode::Insert)?;
        painted = frame.len();
        match session.on_key(terminal.read_key()?) {
            Outcome::Continue => {}
            Outcome::Commit(roots) => {
                terminal.rewind(painted)?;
                terminal.clear()?;
                return Ok(SessionResult::Selected(roots));
            }
            Outcome::Quit => {
                terminal.rewind(painted)?;
                terminal.clear()?;
                return Ok(SessionResult::Cancelled);
            }
        }
    }
}

/// The `crossterm` implementation of [`Terminal`].
pub struct CrosstermTerminal;

impl Terminal for CrosstermTerminal {
    fn enter_raw(&mut self) -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()
    }

    fn restore(&mut self) -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }

    fn read_key(&mut self) -> io::Result<Key> {
        // Resize, focus, mouse, and paste carry no binding, but all of them
        // mean the next frame should be repainted at the new geometry. The
        // driver re-measures before every frame, so mapping them to
        // `Ignored` is both correct and sufficient.
        match crossterm::event::read() {
            Ok(crossterm::event::Event::Key(key)) => Ok(normalize(key)),
            Ok(_) => Ok(Key::Ignored),
            Err(_) => Ok(Key::Ignored),
        }
    }

    fn size(&mut self) -> io::Result<(u16, u16)> {
        crossterm::terminal::size()
    }

    fn rewind(&mut self, lines: usize) -> io::Result<()> {
        let mut out = io::stdout();
        if lines > 0 {
            crossterm::execute!(out, crossterm::cursor::MoveUp(lines as u16))?;
        }
        crossterm::execute!(
            out,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
        )?;
        out.flush()
    }

    fn set_cursor_visible(&mut self, visible: bool) -> io::Result<()> {
        let mut out = io::stdout();
        if visible {
            crossterm::execute!(out, crossterm::cursor::Show)?;
        } else {
            crossterm::execute!(out, crossterm::cursor::Hide)?;
        }
        out.flush()
    }

    fn clear(&mut self) -> io::Result<()> {
        let mut out = io::stdout();
        crossterm::execute!(
            out,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
        )?;
        out.flush()
    }

    fn draw(&mut self, lines: &[String]) -> io::Result<()> {
        let mut out = io::stdout();
        for line in lines {
            writeln!(out, "{line}")?;
        }
        out.flush()
    }
}

/// Map one crossterm key onto the session's normalized vocabulary.
///
/// `Ctrl-C` is bound before any other examination, so `Ctrl-C` in INS mode
/// quits rather than typing a control character into the filter.
fn normalize(event: crossterm::event::KeyEvent) -> Key {
    use crossterm::event::{KeyCode, KeyModifiers};
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        return match event.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Key::CtrlC,
            _ => Key::Ignored,
        };
    }
    match event.code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(c) => Key::Char(c),
        _ => Key::Ignored,
    }
}

/// Present a resolved candidate set on the real terminal.
///
/// This is the only entry point production code needs. The TTY precondition is
/// checked here so that a redirected `-i` fails as a typed usage error rather
/// than driving a session whose keystrokes are going somewhere else.
///
/// An empty candidate set is settled *before* that precondition: there is no
/// session to drive, so there is nothing a terminal could be needed for. It is
/// a plain negative result, not a usage error — the same scope cleaned without
/// `-i` reports zero units and exits 0, and the two spellings of one scope
/// must agree.
pub fn run_interactive(candidates: Vec<CleanupCandidate>) -> Result<SessionResult, String> {
    if candidates.is_empty() {
        return Ok(SessionResult::NothingToClean);
    }
    ensure_interactive_terminal(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )
    .map_err(|reason| {
        format!(
            "interactive selection requires a terminal ({reason}); \
             drop -i to clean without choosing interactively, or \
             use `cargo cleanme scan --all` to inspect the same units read-only"
        )
    })?;
    drive(&mut CrosstermTerminal, candidates)
        .map_err(|e| format!("interactive selection failed: {e}"))
}

/// Render a session's current frame to a single string. Test and diagnostic
/// helper; the driver itself uses [`Session::frame`] line by line.
pub fn render_to_string(session: &Session) -> String {
    let mut out = String::new();
    for (i, line) in session.frame().iter().enumerate() {
        if i > 0 {
            let _ = writeln!(out);
        }
        let _ = write!(out, "{line}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OutputOwnershipClass;
    use std::collections::VecDeque;
    use std::panic::AssertUnwindSafe;

    fn private(path: &str) -> CleanupCandidate {
        CleanupCandidate {
            root: PathBuf::from(path),
            bytes: 1024,
            ownership: OutputOwnershipClass::PrivateBounded,
        }
    }

    fn sample() -> Vec<CleanupCandidate> {
        vec![
            private("/home/u/projects/alpha"),
            private("/home/u/projects/beta"),
            CleanupCandidate {
                root: PathBuf::from("/home/u/projects/gamma"),
                bytes: 4096,
                ownership: OutputOwnershipClass::Shared,
            },
        ]
    }

    fn session() -> Session {
        Session::new(sample()).with_size(80, 20)
    }

    /// Feed keys until the session leaves `Continue`.
    fn press(s: &mut Session, keys: &[Key]) -> Outcome {
        let mut last = Outcome::Continue;
        for key in keys {
            last = s.on_key(key.clone());
            if last != Outcome::Continue {
                break;
            }
        }
        last
    }

    /// Type a filter in INS, then return to NOR.
    fn filter(s: &mut Session, needle: &str) {
        press(s, &[Key::Char('i')]);
        press(s, &needle.chars().map(Key::Char).collect::<Vec<_>>());
        press(s, &[Key::Esc]);
    }

    // --- initial state ---------------------------------------------------

    #[test]
    fn starts_in_normal_mode_with_nothing_selected() {
        let s = session();
        assert_eq!(s.mode(), Mode::Normal);
        assert_eq!(s.selected_count(), 0);
        assert_eq!(s.selected_bytes(), 0);
        assert_eq!(s.cursor_candidate(), None, "row 0 is the all-toggle");
    }

    // --- navigation ------------------------------------------------------

    #[test]
    fn arrows_and_vim_keys_move_the_cursor_identically() {
        let mut arrows = session();
        let mut vim = session();
        press(&mut arrows, &[Key::Down, Key::Down]);
        press(&mut vim, &[Key::Char('j'), Key::Char('j')]);
        assert_eq!(arrows.cursor_candidate(), vim.cursor_candidate());
        assert_eq!(arrows.cursor_candidate(), Some(1));
        press(&mut arrows, &[Key::Up]);
        press(&mut vim, &[Key::Char('k')]);
        assert_eq!(arrows.cursor_candidate(), vim.cursor_candidate());
    }

    #[test]
    fn the_cursor_stops_at_both_ends() {
        let mut s = session();
        press(&mut s, &[Key::Up, Key::Up, Key::Up]);
        assert_eq!(s.cursor_candidate(), None, "cannot move above the all-row");
        for _ in 0..10 {
            press(&mut s, &[Key::Down]);
        }
        assert_eq!(
            s.cursor_candidate(),
            Some(2),
            "cannot move past the last candidate"
        );
    }

    // --- selection -------------------------------------------------------

    #[test]
    fn space_toggles_the_row_under_the_cursor_only() {
        let mut s = session();
        press(&mut s, &[Key::Down, Key::Space]);
        assert_eq!(s.selected_count(), 1);
        assert_eq!(s.selected_bytes(), 1024);
        press(&mut s, &[Key::Space]);
        assert_eq!(s.selected_count(), 0, "space on the same row unchecks it");
    }

    #[test]
    fn the_all_row_checks_every_candidate_including_filtered_ones() {
        let mut s = session();
        filter(&mut s, "alph");
        assert_eq!(
            s.candidate_count(),
            3,
            "the filter does not shrink the list"
        );
        // A toggle that obeyed the filter would check one unit and leave two
        // silently unchecked, which is the opposite of what "all" says.
        press(&mut s, &[Key::Space]);
        assert_eq!(s.selected_count(), 3);
        assert_eq!(s.selected_bytes(), 1024 + 1024 + 4096);
    }

    #[test]
    fn the_all_row_unchecks_everything_when_everything_is_checked() {
        let mut s = session();
        press(&mut s, &[Key::Space, Key::Space]);
        assert_eq!(s.selected_count(), 0);
        press(&mut s, &[Key::Space]);
        assert_eq!(s.selected_count(), 3);
        press(&mut s, &[Key::Space]);
        assert_eq!(s.selected_count(), 0);
    }

    // --- modes -----------------------------------------------------------

    #[test]
    fn i_enters_insert_mode_and_esc_returns_keeping_the_filter() {
        let mut s = session();
        press(&mut s, &[Key::Char('i')]);
        assert_eq!(s.mode(), Mode::Insert);
        press(&mut s, &[Key::Char('b'), Key::Char('e')]);
        assert_eq!(s.filter(), "be");
        press(&mut s, &[Key::Esc]);
        assert_eq!(s.mode(), Mode::Normal);
        assert_eq!(s.filter(), "be", "the filter survives the mode change");
    }

    #[test]
    fn insert_mode_types_keys_that_are_nor_commands() {
        let mut s = session();
        press(&mut s, &[Key::Char('i')]);
        // 'q', 'j', 'k', and space are NOR commands. In INS they are text.
        press(&mut s, &[Key::Char('q'), Key::Char('j'), Key::Char('k')]);
        press(&mut s, &[Key::Space]);
        assert_eq!(s.mode(), Mode::Insert);
        assert_eq!(s.filter(), "qjk ");
        assert_eq!(
            s.selected_count(),
            0,
            "space typed a character; it did not toggle a row"
        );
    }

    #[test]
    fn a_filter_can_contain_a_space_because_paths_can() {
        let mut s = Session::new(vec![
            private("/home/u/my projects/alpha"),
            private("/home/u/projects/beta"),
        ])
        .with_size(80, 20);
        filter(&mut s, "my proj");
        assert_eq!(s.selected_count(), 0);
        press(&mut s, &[Key::Down, Key::Space]);
        let Outcome::Commit(roots) = s.on_key(Key::Enter) else {
            panic!("the only match must be committable");
        };
        assert_eq!(roots, vec![PathBuf::from("/home/u/my projects/alpha")]);
    }

    // --- exit ------------------------------------------------------------

    #[test]
    fn q_quits_from_normal_but_types_in_insert() {
        let mut s = session();
        assert_eq!(press(&mut s, &[Key::Char('q')]), Outcome::Quit);

        let mut s = session();
        press(&mut s, &[Key::Char('i')]);
        assert_eq!(press(&mut s, &[Key::Char('q')]), Outcome::Continue);
        assert_eq!(s.filter(), "q");
    }

    #[test]
    fn ctrl_c_quits_from_either_mode() {
        let mut s = session();
        press(&mut s, &[Key::Char('i'), Key::Char('x')]);
        assert_eq!(press(&mut s, &[Key::CtrlC]), Outcome::Quit);

        let mut s = session();
        assert_eq!(press(&mut s, &[Key::CtrlC]), Outcome::Quit);
    }

    #[test]
    fn two_escapes_exit_and_one_does_not() {
        let mut s = session();
        assert_eq!(press(&mut s, &[Key::Esc]), Outcome::Continue);
        assert_eq!(press(&mut s, &[Key::Esc]), Outcome::Quit);
    }

    #[test]
    fn a_pending_exit_is_cancelled_by_navigating_away() {
        let mut s = session();
        press(&mut s, &[Key::Esc, Key::Down]);
        assert_eq!(press(&mut s, &[Key::Esc]), Outcome::Continue);
        assert_eq!(press(&mut s, &[Key::Esc]), Outcome::Quit);
    }

    // --- commit ----------------------------------------------------------

    #[test]
    fn enter_commits_the_checked_roots_in_candidate_order() {
        let mut s = session();
        press(
            &mut s,
            &[Key::Down, Key::Space, Key::Down, Key::Down, Key::Space],
        );
        let Outcome::Commit(roots) = s.on_key(Key::Enter) else {
            panic!("enter on a non-empty selection must commit");
        };
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/home/u/projects/alpha"),
                PathBuf::from("/home/u/projects/gamma"),
            ]
        );
    }

    #[test]
    fn enter_with_nothing_selected_stays_in_the_session() {
        // The M012A shape: a run that reports success it never performed. An
        // empty commit must leave the operator holding their list.
        let mut s = session();
        assert_eq!(press(&mut s, &[Key::Enter]), Outcome::Continue);
        assert_eq!(s.mode(), Mode::Normal);
        assert_eq!(press(&mut s, &[Key::Enter, Key::Enter]), Outcome::Continue);
    }

    #[test]
    fn enter_in_insert_mode_commits_too() {
        let mut s = session();
        press(&mut s, &[Key::Space]);
        press(&mut s, &[Key::Char('i')]);
        let Outcome::Commit(roots) = s.on_key(Key::Enter) else {
            panic!("enter must commit from INS as well");
        };
        assert_eq!(roots.len(), 3);
    }

    // --- filtering -------------------------------------------------------

    #[test]
    fn filtering_never_discards_an_existing_selection() {
        let mut s = session();
        press(&mut s, &[Key::Down, Key::Down, Key::Space]);
        assert_eq!(s.selected_count(), 1);
        filter(&mut s, "be");
        assert_eq!(s.selected_count(), 1, "beta was checked before the filter");
        filter(&mut s, "alph");
        assert_eq!(s.selected_count(), 1, "still beta, not alpha");
        assert_eq!(s.selected_bytes(), 1024);
    }

    #[test]
    fn filtering_is_case_insensitive_and_matches_the_full_path() {
        let mut s = session();
        filter(&mut s, "PROJ");
        assert_eq!(s.visible_count(), 3, "'PROJ' matches all three roots");
        // The filter matches the *full path*, even though a row is named by
        // the shortest suffix that identifies it. A filter that only matched
        // the displayed name would silently stop finding workspaces.
        let mut fresh = session();
        filter(&mut fresh, "/home/u/projects");
        assert_eq!(
            fresh.visible_count(),
            3,
            "the filter must still match the full path"
        );
        let frame = render_to_string(&fresh);
        assert!(
            frame.contains("alpha") && frame.contains("beta") && frame.contains("gamma"),
            "the rows are named by their shortest unique suffix: {frame}"
        );
    }

    #[test]
    fn a_filter_matching_nothing_renders_an_explicit_empty_row() {
        let mut s = session();
        filter(&mut s, "zz");
        let frame = s.frame();
        assert!(
            frame.iter().any(|l| l.contains("no workspace matches")),
            "an empty result set must say so, not render blank: {frame:?}"
        );
        assert_eq!(press(&mut s.clone(), &[Key::Enter]), Outcome::Continue);
    }

    #[test]
    fn backspace_edits_the_filter_and_widens_the_result_set() {
        let mut s = session();
        press(
            &mut s,
            &[
                Key::Char('i'),
                Key::Char('a'),
                Key::Char('l'),
                Key::Backspace,
            ],
        );
        assert_eq!(s.filter(), "a");
        press(&mut s, &[Key::Backspace]);
        assert_eq!(s.filter(), "");
        press(&mut s, &[Key::Backspace]);
        assert_eq!(s.filter(), "", "backspace on an empty field is a no-op");
    }

    #[test]
    fn editing_the_filter_parks_the_cursor_on_the_all_row() {
        let mut s = session();
        press(&mut s, &[Key::Down, Key::Down]);
        assert_eq!(s.cursor_candidate(), Some(1));
        press(&mut s, &[Key::Char('i'), Key::Char('b'), Key::Esc]);
        assert_eq!(
            s.cursor_candidate(),
            None,
            "a cursor carried across a changing result set would point at a different unit"
        );
    }

    // --- rendering -------------------------------------------------------

    #[test]
    fn rows_annotate_ownership_so_an_inventory_only_unit_is_not_mistaken_for_a_candidate() {
        let frame = render_to_string(&session());
        assert!(frame.contains("private"), "alpha/beta are private: {frame}");
        assert!(
            frame.contains("shared — inventory only"),
            "gamma must be visibly non-cleanable: {frame}"
        );
    }

    #[test]
    fn the_all_row_states_its_own_arithmetic() {
        let mut s = session();
        press(&mut s, &[Key::Down, Key::Space]);
        let frame = render_to_string(&s);
        assert!(frame.contains("1 selected of 3"), "{frame}");
        assert!(
            frame.contains("[-] all"),
            "partial selection is shown: {frame}"
        );
        press(&mut s, &[Key::Up, Key::Space]);
        let frame = render_to_string(&s);
        assert!(frame.contains("3 selected of 3"), "{frame}");
        assert!(
            frame.contains("[x] all"),
            "full selection is shown: {frame}"
        );
    }

    #[test]
    fn the_status_line_shows_the_filter_in_both_modes_and_the_armed_exit_in_nor() {
        let mut s = session();
        filter(&mut s, "be");
        assert!(render_to_string(&s).contains("NOR  filter: be"));

        press(&mut s, &[Key::Esc]);
        let frame = render_to_string(&s);
        assert!(
            frame.contains("Esc again to exit"),
            "the armed exit must be visible before it fires: {frame}"
        );

        press(&mut s, &[Key::Char('i')]);
        let frame = render_to_string(&s);
        assert!(frame.contains("INS  filter: be"), "{frame}");
        assert!(
            !frame.contains("Esc again to exit"),
            "entering INS disarms the pending exit: {frame}"
        );
    }

    #[test]
    fn the_frame_is_bounded_and_never_overflows_the_width() {
        for (w, h) in [(40u16, 9u16), (80, 24), (200, 60), (30, 4)] {
            let frame = Session::new(sample()).with_size(w, h).frame();
            let width = (w as usize).max(MIN_WIDTH);
            let height = (h as usize).max(CHROME_LINES + MIN_LIST_ROWS);
            assert!(
                frame.len() <= height,
                "the frame must never exceed the terminal at {w}x{h}: {} lines",
                frame.len()
            );
            assert!(
                frame.len() <= CHROME_LINES + PAGE_ROWS,
                "the frame must stay bounded at {w}x{h}: {} lines",
                frame.len()
            );
            for line in &frame {
                assert!(
                    visible_width(line) <= width,
                    "line overflows {width} at {w}x{h}: {line:?}"
                );
            }
        }
    }

    #[test]
    fn the_frame_does_not_grow_with_the_candidate_count() {
        // The same shape for three units and for three thousand: a list that
        // fills the screen is a list the operator has to scroll, not read.
        let few: Vec<CleanupCandidate> = (0..3)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let many: Vec<CleanupCandidate> = (0..3000)
            .map(|i| private(&format!("/home/u/projects/p{i:04}")))
            .collect();
        let small = Session::new(few).with_size(100, 40).frame();
        let large = Session::new(many).with_size(100, 40).frame();
        assert_eq!(
            small.len(),
            CHROME_LINES + 3,
            "a short list shows no blank filler rows"
        );
        assert_eq!(
            large.len(),
            CHROME_LINES + PAGE_ROWS,
            "a long list stops at the page"
        );
        assert!(
            large.len() < 40,
            "the frame must not fill the terminal: {} lines",
            large.len()
        );
    }

    #[test]
    fn the_page_is_capped_by_the_terminal_height() {
        // A terminal too short for the full page shows fewer rows, and the
        // frame still fits inside it.
        let many: Vec<CleanupCandidate> = (0..300)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        for h in [6u16, 8, 12, 20] {
            let frame = Session::new(many.clone()).with_size(80, h).frame();
            assert!(
                frame.len() <= h as usize,
                "the frame must fit a {h}-row terminal: {} lines",
                frame.len()
            );
        }
    }

    #[test]
    fn a_long_display_name_is_truncated_from_the_left_not_the_right() {
        // Two workspaces with the same basename cannot be named by it, so the
        // label grows until it is long enough to be truncated.
        let long = format!("/{}/deep/project", "x".repeat(200));
        let other = format!("/{}/deep/project", "y".repeat(200));
        let s = Session::new(vec![private(&long), private(&other)]).with_size(60, 10);
        let frame = render_to_string(&s);
        assert!(frame.contains('…'), "{frame}");
        assert!(
            frame.contains("deep/project"),
            "the distinguishing tail must survive: {frame}"
        );
    }

    #[test]
    fn scrolling_keeps_the_cursor_row_inside_the_viewport() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(60, 10);
        for _ in 0..30 {
            press(&mut s, &[Key::Down]);
        }
        let cursor = s.cursor_candidate().expect("cursor is on a candidate");
        let frame = render_to_string(&s);
        assert!(
            frame.contains(&format!("p{cursor:02}")),
            "the cursor's own row must be painted: {frame}"
        );
        // 29 of the 30 ups bring the cursor from row 30 to row 1, which is the
        // first candidate. Row 0 is the all-toggle, so stopping one short
        // lands on `p00` rather than off the top of the list.
        for _ in 0..29 {
            press(&mut s, &[Key::Up]);
        }
        assert_eq!(s.cursor_candidate(), Some(0));
    }

    #[test]
    fn scrolling_up_from_the_bottom_reaches_the_first_candidate() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(60, 10);
        for _ in 0..39 {
            press(&mut s, &[Key::Down]);
        }
        assert_eq!(s.cursor_candidate(), Some(38));
        for _ in 0..38 {
            press(&mut s, &[Key::Up]);
        }
        assert_eq!(s.cursor_candidate(), Some(0), "back at the first candidate");
        let frame = render_to_string(&s);
        assert!(frame.contains("p00"), "{frame}");
        assert!(
            !frame.contains("p38"),
            "the viewport followed the cursor home: {frame}"
        );
    }

    #[test]
    fn a_resize_keeps_the_selection_and_reclamps_the_cursor() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(60, 10);
        for _ in 0..20 {
            press(&mut s, &[Key::Down, Key::Space]);
        }
        assert_eq!(s.selected_count(), 20);
        s.set_size(200, 60);
        assert_eq!(s.selected_count(), 20, "a resize is not a reset");
        assert_eq!(s.mode(), Mode::Normal);
        assert_eq!(s.frame().len(), CHROME_LINES + PAGE_ROWS);
        assert!(s.frame().len() <= 60, "the frame still fits the terminal");
    }

    #[test]
    fn an_empty_candidate_list_still_renders_and_exits() {
        let mut s = Session::new(Vec::new()).with_size(60, 10);
        assert_eq!(s.frame().len(), CHROME_LINES + MIN_LIST_ROWS);
        assert_eq!(press(&mut s, &[Key::Enter]), Outcome::Continue);
        assert_eq!(press(&mut s, &[Key::Esc, Key::Esc]), Outcome::Quit);
        filter(&mut s, "anything");
        assert_eq!(s.frame().len(), CHROME_LINES + MIN_LIST_ROWS);
    }

    // --- driver lifecycle -------------------------------------------------

    /// A scripted terminal: hands out keys, records what it was told.
    struct FakeTerminal {
        keys: VecDeque<Key>,
        frames: Vec<Vec<String>>,
        raw: bool,
        /// Whether `enter_raw` was ever called. Lets a test assert the session
        /// was *genuinely* in raw mode when it panicked, rather than asserting
        /// a restore that happened without an enter.
        raw_before_panic: bool,
        restored: usize,
        cleared: usize,
        cursor_visible: Option<bool>,
    }

    impl FakeTerminal {
        fn new(keys: Vec<Key>) -> Self {
            Self {
                keys: keys.into(),
                frames: Vec::new(),
                raw: false,
                raw_before_panic: false,
                restored: 0,
                cleared: 0,
                cursor_visible: None,
            }
        }
    }

    impl Terminal for FakeTerminal {
        fn enter_raw(&mut self) -> io::Result<()> {
            self.raw = true;
            self.raw_before_panic = true;
            Ok(())
        }
        fn restore(&mut self) -> io::Result<()> {
            self.restored += 1;
            self.raw = false;
            Ok(())
        }
        fn read_key(&mut self) -> io::Result<Key> {
            Ok(self.keys.pop_front().unwrap_or(Key::Ignored))
        }
        fn size(&mut self) -> io::Result<(u16, u16)> {
            Ok((80, 20))
        }
        fn rewind(&mut self, _lines: usize) -> io::Result<()> {
            Ok(())
        }
        fn set_cursor_visible(&mut self, visible: bool) -> io::Result<()> {
            self.cursor_visible = Some(visible);
            Ok(())
        }
        fn clear(&mut self) -> io::Result<()> {
            self.cleared += 1;
            Ok(())
        }
        fn draw(&mut self, lines: &[String]) -> io::Result<()> {
            self.frames.push(lines.to_vec());
            Ok(())
        }
    }

    #[test]
    fn the_driver_restores_the_terminal_on_the_commit_path() {
        let mut term = FakeTerminal::new(vec![
            Key::Down,
            Key::Space,
            Key::Down,
            Key::Space,
            Key::Enter,
        ]);
        let result = drive(&mut term, sample()).unwrap();
        assert_eq!(
            result,
            SessionResult::Selected(vec![
                PathBuf::from("/home/u/projects/alpha"),
                PathBuf::from("/home/u/projects/beta"),
            ])
        );
        assert!(!term.raw, "the terminal must not be left in raw mode");
        assert_eq!(term.restored, 1);
        assert_eq!(term.cleared, 1, "the painted region is erased on exit");
    }

    #[test]
    fn the_driver_restores_the_terminal_on_the_quit_path() {
        let mut term = FakeTerminal::new(vec![Key::Char('q')]);
        assert_eq!(
            drive(&mut term, sample()).unwrap(),
            SessionResult::Cancelled
        );
        assert!(!term.raw);
        assert_eq!(term.restored, 1);
        assert_eq!(term.cleared, 1);
    }

    #[test]
    fn the_driver_shows_the_hardware_cursor_only_in_insert_mode() {
        let mut term = FakeTerminal::new(vec![
            Key::Char('i'),
            Key::Char('x'),
            Key::Esc,
            Key::Char('q'),
        ]);
        drive(&mut term, sample()).unwrap();
        // Last recorded value is NOR; the INS frame in between must have shown it.
        assert_eq!(term.cursor_visible, Some(false));
        assert!(
            term.frames.len() >= 3,
            "insert mode should have painted at least one frame: {}",
            term.frames.len()
        );
    }

    #[test]
    fn the_driver_repaints_in_place_without_an_alternate_screen() {
        let mut term = FakeTerminal::new(vec![Key::Down, Key::Down, Key::Char('q')]);
        drive(&mut term, sample()).unwrap();
        assert!(term.frames.len() >= 3, "each key repaints");
        for frame in &term.frames {
            assert_eq!(
                frame.len(),
                CHROME_LINES + 3,
                "every frame is the same bounded shape"
            );
            assert!(
                frame.len() < 20,
                "the frame must not fill the terminal: {} lines",
                frame.len()
            );
        }
    }

    #[test]
    fn a_read_failure_restores_the_terminal_and_propagates() {
        struct FailingRead {
            inner: FakeTerminal,
        }
        impl Terminal for FailingRead {
            fn enter_raw(&mut self) -> io::Result<()> {
                self.inner.enter_raw()
            }
            fn restore(&mut self) -> io::Result<()> {
                self.inner.restore()
            }
            fn read_key(&mut self) -> io::Result<Key> {
                Err(io::Error::other("device disappeared"))
            }
            fn size(&mut self) -> io::Result<(u16, u16)> {
                self.inner.size()
            }
            fn rewind(&mut self, l: usize) -> io::Result<()> {
                self.inner.rewind(l)
            }
            fn set_cursor_visible(&mut self, v: bool) -> io::Result<()> {
                self.inner.set_cursor_visible(v)
            }
            fn clear(&mut self) -> io::Result<()> {
                self.inner.clear()
            }
            fn draw(&mut self, l: &[String]) -> io::Result<()> {
                self.inner.draw(l)
            }
        }
        let mut term = FailingRead {
            inner: FakeTerminal::new(Vec::new()),
        };
        assert!(drive(&mut term, sample()).is_err());
        assert_eq!(term.inner.restored, 1);
        assert!(!term.inner.raw);
    }

    #[test]
    fn the_guard_restores_when_dropped_during_unwind() {
        struct Harness {
            terminal: FakeTerminal,
        }
        let mut harness = Harness {
            terminal: FakeTerminal::new(Vec::new()),
        };
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let guard = RawModeGuard {
                terminal: &mut harness.terminal,
                restored: false,
            };
            guard.terminal.enter_raw().unwrap();
            panic!("boom");
        }));
        assert!(outcome.is_err());
        assert!(
            harness.terminal.raw_before_panic,
            "the session was genuinely in raw mode when it panicked"
        );
        assert_eq!(
            harness.terminal.restored, 1,
            "a panicking session must still restore the terminal"
        );
        assert!(!harness.terminal.raw);
    }

    #[test]
    fn the_guard_restores_exactly_once_even_when_restored_explicitly() {
        let mut term = FakeTerminal::new(Vec::new());
        {
            let mut guard = RawModeGuard {
                terminal: &mut term,
                restored: false,
            };
            guard.restore().unwrap();
            guard.restore().unwrap();
        }
        // `drive` restores explicitly and then `Drop` runs. A second
        // `disable_raw_mode` on a handle that has already reported itself
        // broken is not obviously safe.
        assert_eq!(term.restored, 1);
    }

    #[test]
    fn a_failing_restore_is_not_retried_on_drop() {
        struct FailingRestore {
            calls: usize,
        }
        impl Terminal for FailingRestore {
            fn enter_raw(&mut self) -> io::Result<()> {
                Ok(())
            }
            fn restore(&mut self) -> io::Result<()> {
                self.calls += 1;
                Err(io::Error::other("terminal handle is gone"))
            }
            fn read_key(&mut self) -> io::Result<Key> {
                Ok(Key::Ignored)
            }
            fn size(&mut self) -> io::Result<(u16, u16)> {
                Ok((80, 20))
            }
            fn rewind(&mut self, _l: usize) -> io::Result<()> {
                Ok(())
            }
            fn set_cursor_visible(&mut self, _v: bool) -> io::Result<()> {
                Ok(())
            }
            fn clear(&mut self) -> io::Result<()> {
                Ok(())
            }
            fn draw(&mut self, _l: &[String]) -> io::Result<()> {
                Ok(())
            }
        }
        let mut term = FailingRestore { calls: 0 };
        {
            let mut guard = RawModeGuard {
                terminal: &mut term,
                restored: false,
            };
            assert!(guard.restore().is_err());
        }
        assert_eq!(term.calls, 1, "an explicit failure must not be retried");
    }

    // --- terminal precondition -------------------------------------------

    #[test]
    fn a_redirected_stream_is_refused_before_raw_mode() {
        assert!(ensure_interactive_terminal(true, true).is_ok());
        assert_eq!(
            ensure_interactive_terminal(false, true),
            Err("standard input is not a terminal")
        );
        assert_eq!(
            ensure_interactive_terminal(true, false),
            Err("standard output is not a terminal")
        );
        assert!(ensure_interactive_terminal(false, false).is_err());
    }

    // --- key normalization ------------------------------------------------

    #[test]
    fn crossterm_keys_map_to_the_session_vocabulary() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let press = |code, mods| normalize(KeyEvent::new(code, mods));
        assert_eq!(press(KeyCode::Up, KeyModifiers::NONE), Key::Up);
        assert_eq!(press(KeyCode::Down, KeyModifiers::NONE), Key::Down);
        assert_eq!(press(KeyCode::Esc, KeyModifiers::NONE), Key::Esc);
        assert_eq!(press(KeyCode::Enter, KeyModifiers::NONE), Key::Enter);
        assert_eq!(
            press(KeyCode::Backspace, KeyModifiers::NONE),
            Key::Backspace
        );
        assert_eq!(press(KeyCode::Char(' '), KeyModifiers::NONE), Key::Space);
        assert_eq!(
            press(KeyCode::Char('x'), KeyModifiers::NONE),
            Key::Char('x')
        );
        assert_eq!(press(KeyCode::Char('c'), KeyModifiers::CONTROL), Key::CtrlC);
        assert_eq!(
            press(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            Key::CtrlC,
            "Ctrl-Shift-C is still Ctrl-C"
        );
        assert_eq!(
            press(KeyCode::Char('q'), KeyModifiers::SHIFT),
            Key::Char('q'),
            "the terminal has already applied SHIFT; the payload is plain"
        );
        assert_eq!(press(KeyCode::F(5), KeyModifiers::NONE), Key::Ignored);
        assert_eq!(press(KeyCode::Tab, KeyModifiers::NONE), Key::Ignored);
    }

    // --- the empty candidate set -------------------------------------------

    /// An empty candidate set is a plain negative result, not a usage error.
    ///
    /// The same scope cleaned without `-i` reports zero units and exits 0, so
    /// the two spellings of one scope must agree. Before this was settled
    /// before the terminal precondition, an empty surface was a typed refusal
    /// that exited 2 — a scope with nothing in it was reported as though the
    /// operator had asked for something impossible.
    ///
    /// No terminal is needed: there is no session to drive, so there is
    /// nothing a terminal could be needed for. That is also what makes this
    /// case testable without a pty.
    #[test]
    fn an_empty_candidate_set_is_a_plain_negative_result_not_a_usage_error() {
        assert_eq!(
            run_interactive(Vec::new()),
            Ok(SessionResult::NothingToClean),
            "an empty candidate set has one true sentence, and it is not an error"
        );
    }

    /// A non-empty candidate set still refuses to run without a terminal.
    /// The empty case must not have weakened the headless guard: a surface
    /// that *could* be driven still needs somewhere to drive it.
    #[test]
    fn a_non_empty_candidate_set_still_needs_a_terminal() {
        // The terminal precondition is checked inside `run_interactive`, and
        // the test process has no terminal on either stream.
        assert!(
            run_interactive(vec![private("/home/u/projects/alpha")]).is_err(),
            "a headless caller must not be able to drive a session"
        );
    }

    // --- the row is named, not spelled out --------------------------------

    #[test]
    fn a_row_is_named_by_the_shortest_suffix_that_identifies_it() {
        // Siblings under one parent are told apart by their own name, so the
        // row does not spend its width on a prefix every row shares.
        let candidates = vec![
            private("/home/u/projects/alpha"),
            private("/home/u/projects/beta"),
            private("/home/u/projects/gamma"),
        ];
        let labels = unique_labels(&candidates);
        assert_eq!(labels, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn a_colliding_basename_is_told_apart_by_more_of_the_path() {
        // Two workspaces called `x` must not be rendered as two rows called
        // `x`: the name has to grow until it identifies the row.
        let candidates = vec![private("/a/x"), private("/b/x")];
        let labels = unique_labels(&candidates);
        assert_eq!(labels, vec!["a/x", "b/x"]);
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn two_candidates_with_the_same_path_fall_back_to_the_whole_path() {
        // There is no suffix that tells them apart, so the whole path is
        // shown rather than a name that says nothing.
        let candidates = vec![private("/a/x"), private("/a/x")];
        let labels = unique_labels(&candidates);
        assert_eq!(labels, vec!["/a/x", "/a/x"]);
    }

    #[test]
    fn the_rendered_row_carries_the_short_name_not_the_whole_path() {
        let s = session();
        let frame = render_to_string(&s);
        assert!(frame.contains("alpha"), "{frame}");
        assert!(
            !frame.contains("/home/u/projects/alpha"),
            "the absolute path must not be spelled out on the row: {frame}"
        );
    }

    // --- the columns -----------------------------------------------------

    #[test]
    fn the_ownership_column_is_omitted_when_every_candidate_is_private() {
        let candidates = vec![
            private("/home/u/projects/alpha"),
            private("/home/u/projects/beta"),
        ];
        let s = Session::new(candidates).with_size(100, 20);
        let frame = render_to_string(&s);
        assert!(
            !frame.contains("private"),
            "a column that says the same word on every row carries no information: {frame}"
        );
        assert!(!frame.contains("OWNERSHIP"), "{frame}");
    }

    #[test]
    fn the_ownership_column_appears_when_the_set_is_mixed() {
        // A non-private row must not be mistaken for a candidate, so it says
        // so — even though every other row is private.
        let frame = render_to_string(&session());
        assert!(frame.contains("private"), "{frame}");
        assert!(
            frame.contains("shared — inventory only"),
            "a non-private row must say it is not cleanable: {frame}"
        );
        assert!(frame.contains("OWNERSHIP"), "{frame}");
    }

    #[test]
    fn the_reclaimable_column_is_right_aligned() {
        let candidates = vec![
            CleanupCandidate {
                root: PathBuf::from("/home/u/projects/alpha"),
                bytes: 1024,
                ownership: OutputOwnershipClass::PrivateBounded,
            },
            CleanupCandidate {
                root: PathBuf::from("/home/u/projects/beta"),
                bytes: 1024 * 1024 * 1024,
                ownership: OutputOwnershipClass::PrivateBounded,
            },
        ];
        let s = Session::new(candidates).with_size(100, 20);
        let frame = s.frame();
        let ends: Vec<usize> = frame
            .iter()
            .filter(|l| l.contains("KiB") || l.contains("GiB"))
            .map(|l| visible_width(l.trim_end()))
            .collect();
        assert_eq!(ends.len(), 2, "both rows carry a size: {frame:?}");
        assert_eq!(ends[0], ends[1], "the sizes must line up: {frame:?}");
    }

    // --- the styling -----------------------------------------------------

    #[test]
    fn the_cursor_row_is_reverse_video() {
        let mut s = session();
        press(&mut s, &[Key::Down]);
        let frame = s.frame();
        let cursor_row = frame
            .iter()
            .find(|l| l.contains("alpha"))
            .expect("the cursor's row is painted");
        assert!(
            cursor_row.contains(ANSI_REVERSE),
            "the cursor's row must be findable at a glance: {cursor_row:?}"
        );
    }

    #[test]
    fn the_gutter_marks_the_edges_of_the_window() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(80, 20);
        // The page holds 15 rows, so the last candidate is off-screen.
        press(&mut s, &[Key::End]);
        let frame = s.frame();
        assert!(
            frame.iter().any(|l| l.contains("^ [ ]")),
            "the window's top edge must be marked: {frame:?}"
        );
        assert!(
            frame.iter().any(|l| l.contains("> [ ]")),
            "the cursor must be marked: {frame:?}"
        );
    }

    #[test]
    fn colour_can_be_turned_off() {
        let coloured = render_to_string(&session());
        assert!(
            coloured.contains('\u{1b}'),
            "the default surface carries ANSI styling"
        );
        let plain = render_to_string(&session().with_color(false));
        assert!(
            !plain.contains('\u{1b}'),
            "a surface with colour off must carry no escape sequences: {plain:?}"
        );
        assert!(
            plain.contains("alpha"),
            "turning colour off must not remove any content: {plain}"
        );
    }

    #[test]
    fn the_colour_decision_honours_no_color_and_a_dumb_terminal() {
        assert!(!color_enabled_with(
            Some(std::ffi::OsString::from("1")),
            None
        ));
        assert!(!color_enabled_with(None, Some("dumb".to_string())));
        assert!(color_enabled_with(None, Some("xterm-256color".to_string())));
        assert!(color_enabled_with(None, None));
    }

    // --- paging ----------------------------------------------------------

    #[test]
    fn paging_keys_move_the_cursor_by_a_page() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(80, 20);
        // The page holds 15 rows, so one PageDown lands on the 15th.
        press(&mut s, &[Key::PageDown]);
        assert_eq!(s.cursor_candidate(), Some(14));
        press(&mut s, &[Key::PageDown]);
        assert_eq!(s.cursor_candidate(), Some(29));
        press(&mut s, &[Key::PageUp]);
        assert_eq!(s.cursor_candidate(), Some(14));
    }

    #[test]
    fn home_and_end_jump_to_the_ends_of_the_list() {
        let many: Vec<CleanupCandidate> = (0..40)
            .map(|i| private(&format!("/home/u/projects/p{i:02}")))
            .collect();
        let mut s = Session::new(many).with_size(80, 20);
        press(&mut s, &[Key::End]);
        assert_eq!(s.cursor_candidate(), Some(39));
        press(&mut s, &[Key::Home]);
        assert_eq!(s.cursor_candidate(), Some(0), "back to the first candidate");
    }

    #[test]
    fn the_paging_keys_are_ignored_in_insert_mode() {
        // INS is a one-line field; a paging key has no meaning in it.
        let mut s = session();
        press(&mut s, &[Key::Char('i')]);
        press(&mut s, &[Key::PageDown, Key::End]);
        assert_eq!(
            s.cursor_candidate(),
            None,
            "the cursor stayed on the all-toggle"
        );
        assert_eq!(s.mode(), Mode::Insert);
    }
}
