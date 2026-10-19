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

/// Lines the session spends on chrome, never on candidate rows.
const CHROME_LINES: usize = 3;

/// Smallest viewport that can show one candidate row.
const MIN_LIST_ROWS: usize = 1;

/// Smallest width the layout is willing to render at.
const MIN_WIDTH: usize = 40;

/// The interactive selection state machine.
///
/// Pure: no IO, no clock, no terminal. Every method is a total function of
/// `self` and the key pressed.
#[derive(Clone, Debug)]
pub struct Session {
    candidates: Vec<CleanupCandidate>,
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
        Self {
            candidates,
            checked,
            visible,
            cursor: 0,
            scroll: 0,
            mode: Mode::Normal,
            filter: String::new(),
            exit_armed: false,
            width: 80,
            height: 24,
        }
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

    fn list_rows(&self) -> usize {
        self.height.saturating_sub(CHROME_LINES).max(MIN_LIST_ROWS)
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
            // Arrows are NOR commands; INS is a one-line field, so there is no
            // sensible in-field meaning for them.
            Key::Up | Key::Down | Key::Ignored => {}
            // Already handled in `on_key`; repeated here so the function is
            // total on its own.
            Key::CtrlC => return Outcome::Quit,
        }
        Outcome::Continue
    }

    fn move_cursor(&mut self, delta: isize) {
        let max = self.rows().saturating_sub(1) as isize;
        let next = (self.cursor as isize + delta).clamp(0, max);
        self.cursor = next as usize;
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
    /// window and the ownership annotation on each row — is assertable without
    /// a terminal.
    pub fn frame(&self) -> Vec<String> {
        let width = self.width;
        let page = self.list_rows();
        let selected = self.selected_count();
        let total = self.candidates.len();
        let mut out = Vec::with_capacity(self.height);

        out.push(pad(
            &truncate(
                &format!(
                    "cargo-cleanme — select workspaces — {selected} selected of {total}, {} reclaimable",
                    format_bytes(self.selected_bytes()).trim()
                ),
                width,
            ),
            width,
        ));

        out.push(pad(&truncate(&self.status_line(), width), width));

        let all_mark = if self.all_checked() {
            "x"
        } else if selected > 0 {
            "-"
        } else {
            " "
        };
        let pointer = if self.cursor == 0 { '>' } else { ' ' };
        out.push(pad(&format!("{pointer}[{all_mark}] all"), width));

        if self.visible.is_empty() {
            out.push(pad("    (no workspace matches the filter)", width));
            for _ in 1..page {
                out.push(String::new());
            }
        } else {
            let end = (self.scroll + page).min(self.visible.len());
            for idx in self.scroll..end {
                let candidate_idx = self.visible[idx];
                let candidate = &self.candidates[candidate_idx];
                let mark = if self.checked[candidate_idx] {
                    "x"
                } else {
                    " "
                };
                let pointer = if self.cursor == idx + 1 { '>' } else { ' ' };
                let ownership = if candidate.provably_private() {
                    "private".to_string()
                } else {
                    format!("{} — inventory only", candidate.ownership.label())
                };
                out.push(pad(
                    &truncate(
                        &format!(
                            "{pointer}[{mark}] {}  {}  {ownership}",
                            candidate.root.display(),
                            format_bytes(candidate.bytes).trim(),
                        ),
                        width,
                    ),
                    width,
                ));
            }
            for _ in end..(self.scroll + page) {
                out.push(String::new());
            }
        }
        debug_assert_eq!(out.len(), CHROME_LINES + page);
        out
    }

    /// The mode/filter line.
    ///
    /// The filter field is on screen in both modes so the operator can see
    /// what is narrowing the list without entering INS to check.
    fn status_line(&self) -> String {
        let mode = self.mode.as_str();
        if self.mode == Mode::Insert {
            // The underscore is the INS caret; the real terminal cursor is
            // also shown here, so this is the shape, not the cursor itself.
            return format!("{mode}  filter: {}_", self.filter);
        }
        let armed = if self.exit_armed {
            "   (Esc again to exit)"
        } else {
            ""
        };
        let filter = if self.filter.is_empty() {
            "<none>".to_string()
        } else {
            self.filter.clone()
        };
        format!("{mode}  filter: {filter}{armed}")
    }
}

fn truncate(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    // Truncate from the *left*: the tail of a path is the part that
    // distinguishes one project from another.
    let mut out = String::from("…");
    out.extend(text.chars().skip(len - (width - 1)));
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

/// Run a selection session against a terminal.
///
/// The raw-mode guard is created before the first key is read, so every exit
/// from here — a normal quit, a `?` on a read error, or an unwinding panic —
/// runs [`Terminal::restore`].
pub fn drive(
    terminal: &mut dyn Terminal,
    candidates: Vec<CleanupCandidate>,
) -> io::Result<SessionResult> {
    let mut session = Session::new(candidates);
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
        let rows = s.frame().iter().filter(|l| l.contains("/home/u/")).count();
        assert_eq!(rows, 3, "'PROJ' matches all three roots");
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
    fn the_frame_is_exactly_the_terminal_height_and_never_overflows_the_width() {
        for (w, h) in [(40u16, 9u16), (80, 24), (200, 60), (30, 4)] {
            let frame = Session::new(sample()).with_size(w, h).frame();
            let width = (w as usize).max(MIN_WIDTH);
            let height = (h as usize).max(CHROME_LINES + MIN_LIST_ROWS);
            assert_eq!(frame.len(), height, "wrong height at {w}x{h}");
            for line in &frame {
                assert!(
                    line.chars().count() <= width,
                    "line overflows {width} at {w}x{h}: {line:?}"
                );
            }
        }
    }

    #[test]
    fn a_very_long_root_is_truncated_from_the_left_not_the_right() {
        let long = format!("/{}/deep/project", "x".repeat(200));
        let s = Session::new(vec![private(&long)]).with_size(60, 10);
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
        assert_eq!(s.frame().len(), 60);
    }

    #[test]
    fn an_empty_candidate_list_still_renders_and_exits() {
        let mut s = Session::new(Vec::new()).with_size(60, 10);
        assert_eq!(s.frame().len(), 10);
        assert_eq!(press(&mut s, &[Key::Enter]), Outcome::Continue);
        assert_eq!(press(&mut s, &[Key::Esc, Key::Esc]), Outcome::Quit);
        filter(&mut s, "anything");
        assert_eq!(s.frame().len(), 10);
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
            assert_eq!(frame.len(), 20, "every frame is the full terminal height");
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
}
