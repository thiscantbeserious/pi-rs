//! SyncBackend: mode 2026 synchronized-output wrapper + real terminal impls
//! (ADR 0024, ADR 0031, ADR 0032).
//!
//! `SyncBackend` wraps `CrosstermBackend` and injects `BeginSynchronizedUpdate`
//! before `Backend::draw` and `EndSynchronizedUpdate` after. This is the
//! tightest BSU/ESU pair (pitfall P12): ratatui calls `backend.draw(diff_iter)`
//! inside `Terminal::flush`, so BSU/ESU wraps the cell writes.
//!
//! `TerminalFrameSink` is the real `FrameSink` impl: owns a
//! `Terminal<SyncBackend<Stdout>>`, calls `try_draw` with the `RmmProjection`.
//! `CountingSink` (render.rs) stays for tests.
//!
//! `CrosstermInput` is the real `InputSource` impl: polls crossterm events
//! and translates to `InputEvent`. `NullInput` (input.rs) stays for tests.
//!
//! Terminal-query methods (`size`, `get_cursor_position`, `window_size`) use
//! `size_override`/`cursor_pos_override`/`window_size_override` fields so the
//! backend can be constructed without a real TTY (for tests). In production
//! these are `None` and the real crossterm query runs.
//!
//! No pi equivalent (pi is single-process JS; the terminal backend is
//! pi-rs-native, ADR 0013). Per PHILOSOPHY §9.5.

use std::io::{self, Write};

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::QueueableCommand;
use ratatui::backend::{Backend, CrosstermBackend, WindowSize};
use ratatui::layout::{Position, Size};
use ratatui::Terminal;

use crate::input::InputEvent;
use crate::input::InputSource;
use crate::projection::RmmProjection;
use crate::render::FrameSink;
use crate::state::RenderState;
use std::time::Duration;

/// A `CrosstermBackend` wrapper that injects mode 2026 synchronized output
/// (BSU/ESU) around `Backend::draw` (ADR 0024, ADR 0031, P12).
///
/// When `mode_2026_enabled` is true, BSU is queued before the cell writes
/// and ESU after. When false (terminal does not support mode 2026), the
/// wrapper passes through (cell-diff still minimizes tearing, P12 graceful
/// degradation).
///
/// `size_override`, `cursor_pos_override`, and `window_size_override` are
/// `None` in production (the real crossterm query runs). In tests they are
/// `Some` so the backend can be constructed without a real TTY.
pub struct SyncBackend<W: Write> {
    writer: W,
    mode_2026_enabled: bool,
    size_override: Option<(u16, u16)>,
    cursor_pos_override: Option<Position>,
    window_size_override: Option<WindowSize>,
}

impl<W: Write> SyncBackend<W> {
    /// Create a new SyncBackend wrapping a writer.
    /// `mode_2026_enabled` controls whether BSU/ESU is injected.
    pub fn new(writer: W, mode_2026_enabled: bool) -> Self {
        Self {
            writer,
            mode_2026_enabled,
            size_override: None,
            cursor_pos_override: None,
            window_size_override: None,
        }
    }

    /// Create a SyncBackend with terminal-query overrides (for tests).
    /// The overrides let `size`, `get_cursor_position`, and `window_size`
    /// return canned values without a real TTY.
    #[cfg(test)]
    fn new_for_test(
        writer: W,
        mode_2026_enabled: bool,
        size: (u16, u16),
        cursor_pos: Position,
        window_size: WindowSize,
    ) -> Self {
        Self {
            writer,
            mode_2026_enabled,
            size_override: Some(size),
            cursor_pos_override: Some(cursor_pos),
            window_size_override: Some(window_size),
        }
    }
    /// Access the writer (for testing).
    #[cfg(test)]
    fn writer_ref(&self) -> &W {
        &self.writer
    }
}

impl<W: Write> Backend for SyncBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        if self.mode_2026_enabled {
            self.writer.queue(BeginSynchronizedUpdate)?;
        }
        // Delegate cell writing to CrosstermBackend on our writer.
        // We reconstruct it per-call because writer_mut() is unstable.
        // This is cheap: CrosstermBackend::new is just wrapping the writer.
        let draw_result = CrosstermBackend::new(&mut self.writer).draw(content);
        // Always emit ESU if BSU was emitted, even on draw error, so the
        // terminal is never left stuck in synchronized-update mode (CodeRabbit).
        if self.mode_2026_enabled {
            self.writer.queue(EndSynchronizedUpdate)?;
        }
        draw_result
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        if let Some(pos) = self.cursor_pos_override {
            return Ok(pos);
        }
        CrosstermBackend::new(&mut self.writer).get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear()
    }

    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        if let Some((w, h)) = self.size_override {
            return Ok(Size::new(w, h));
        }
        crossterm::terminal::size().map(|(w, h)| Size::new(w, h))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        if let Some(ws) = &self.window_size_override {
            return Ok(*ws);
        }
        CrosstermBackend::new(&mut self.writer).window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// The real `FrameSink` impl: owns a `Terminal<SyncBackend<W>>` and
/// calls `try_draw` with the `RmmProjection` (ADR 0031).
///
/// Generic over `W: Write` so tests can use `Vec<u8>`. Production uses
/// `io::Stdout`.
pub struct TerminalFrameSink<W: Write> {
    terminal: Terminal<SyncBackend<W>>,
}

impl TerminalFrameSink<io::Stdout> {
    /// Create a new TerminalFrameSink with mode 2026 support.
    pub fn new(mode_2026_enabled: bool) -> io::Result<Self> {
        let backend = SyncBackend::new(io::stdout(), mode_2026_enabled);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl<W: Write> TerminalFrameSink<W> {
    /// Create a TerminalFrameSink from a pre-built backend (for tests).
    /// The backend must have `size_override` set so `Terminal::new` can
    /// query the size without a real TTY.
    #[cfg(test)]
    fn from_backend(backend: SyncBackend<W>) -> io::Result<Self> {
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Access the terminal for direct commands (resize, etc.).
    pub fn terminal_mut(&mut self) -> &mut Terminal<SyncBackend<W>> {
        &mut self.terminal
    }
}

impl<W: Write + Send> FrameSink for TerminalFrameSink<W> {
    fn draw(&mut self, state: &RenderState) -> io::Result<()> {
        self.terminal.try_draw(|frame| {
            frame.render_widget(RmmProjection::new(state), frame.area());
            Ok::<(), io::Error>(())
        })?;
        Ok(())
    }
}

/// Trait abstracting crossterm event reading, so `CrosstermInput` can be
/// tested without a real terminal (ADR 0030).
pub trait EventReader: Send {
    /// Poll for an event within `timeout`. Returns true if an event is ready.
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    /// Read one event. Only called after `poll` returned true.
    fn read(&mut self) -> io::Result<CrosstermEvent>;
}

/// Production event reader: delegates to crossterm's `event::poll`/`event::read`.
pub struct CrosstermEventReader;

impl EventReader for CrosstermEventReader {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<CrosstermEvent> {
        event::read()
    }
}

/// The real `InputSource` impl: polls crossterm events via an [`EventReader`]
/// and translates to `InputEvent` (ADR 0030).
///
/// Generic over `R: EventReader` so tests can inject a mock reader. Production
/// uses `CrosstermEventReader`.
pub struct CrosstermInput<R: EventReader = CrosstermEventReader> {
    reader: R,
}

impl CrosstermInput<CrosstermEventReader> {
    /// Create a new CrosstermInput with the real crossterm event reader.
    pub fn new() -> Self {
        Self {
            reader: CrosstermEventReader,
        }
    }
}

impl Default for CrosstermInput<CrosstermEventReader> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: EventReader> CrosstermInput<R> {
    /// Create a CrosstermInput with a custom event reader (for tests).
    #[cfg(test)]
    fn with_reader(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: EventReader> InputSource for CrosstermInput<R> {
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        if !self.reader.poll(timeout)? {
            return Ok(None);
        }
        // Fail closed (ADR 0009): propagate read errors, do not swallow them
        // into Ok(None) (CodeRabbit). An input read error stops the reader
        // thread via the Err return.
        match self.reader.read()? {
            CrosstermEvent::Key(key) => Ok(translate_key(key)),
            // Non-key events (mouse, resize, focus) are ignored in Phase 2.
            // Phase 3 adds mouse and resize routing.
            _ => Ok(None),
        }
    }
}

/// Translate a crossterm key event to an InputEvent.
/// Step 3 only handles Quit (Ctrl+C or 'q'). Phase 3 adds full routing.
fn translate_key(key: KeyEvent) -> Option<InputEvent> {
    // Ctrl+C quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(InputEvent::Quit);
    }
    // 'q' quits (only on press, not release).
    if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
        return Some(InputEvent::Quit);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ---- translate_key tests ----

    /// translate_key: Ctrl+C -> Quit.
    #[test]
    fn ctrl_c_quits() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(translate_key(key), Some(InputEvent::Quit));
    }

    /// translate_key: 'q' on press -> Quit.
    #[test]
    fn q_quits() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press);
        assert_eq!(translate_key(key), Some(InputEvent::Quit));
    }

    /// translate_key: 'q' on release -> None (ignore key release).
    #[test]
    fn q_on_release_ignored() {
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert_eq!(translate_key(key), None);
    }

    /// translate_key: other keys -> None.
    #[test]
    fn other_keys_ignored() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(translate_key(key), None);
    }

    // ---- SyncBackend construction tests ----

    /// SyncBackend constructs with mode 2026 enabled.
    #[test]
    fn sync_backend_constructs_with_2026() {
        let buf = Vec::new();
        let backend = SyncBackend::new(buf, true);
        assert!(backend.mode_2026_enabled);
    }

    /// SyncBackend constructs with mode 2026 disabled (degrade path).
    #[test]
    fn sync_backend_constructs_without_2026() {
        let buf = Vec::new();
        let backend = SyncBackend::new(buf, false);
        assert!(!backend.mode_2026_enabled);
    }

    // ---- SyncBackend::draw tests ----

    /// SyncBackend::draw with mode 2026 injects BSU/ESU.
    #[test]
    fn sync_backend_draw_injects_bsu_esu() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, true);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

        backend
            .draw(
                buffer
                    .diff(&Buffer::empty(Rect::new(0, 0, 5, 1)))
                    .iter()
                    .cloned(),
            )
            .unwrap();
        backend.flush().unwrap();

        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(output.contains("\x1b[?2026h"), "BSU must be present");
        assert!(output.contains("\x1b[?2026l"), "ESU must be present");
    }

    /// SyncBackend::draw without mode 2026 does NOT inject BSU/ESU.
    #[test]
    fn sync_backend_draw_no_2026_no_bsu() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

        backend
            .draw(
                buffer
                    .diff(&Buffer::empty(Rect::new(0, 0, 5, 1)))
                    .iter()
                    .cloned(),
            )
            .unwrap();
        backend.flush().unwrap();

        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(
            !output.contains("\x1b[?2026h"),
            "BSU must NOT be present in degrade mode"
        );
        assert!(
            !output.contains("\x1b[?2026l"),
            "ESU must NOT be present in degrade mode"
        );
    }

    /// SyncBackend::draw emits ESU after BSU (ordering check).
    #[test]
    fn sync_backend_draw_bsu_before_esu() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, true);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

        backend
            .draw(
                buffer
                    .diff(&Buffer::empty(Rect::new(0, 0, 5, 1)))
                    .iter()
                    .cloned(),
            )
            .unwrap();
        backend.flush().unwrap();

        let output = String::from_utf8_lossy(backend.writer_ref());
        let bsu_pos = output.find("\x1b[?2026h").expect("BSU must be present");
        let esu_pos = output.find("\x1b[?2026l").expect("ESU must be present");
        assert!(bsu_pos < esu_pos, "BSU must come before ESU");
    }

    // ---- SyncBackend Backend trait method tests ----

    /// hide_cursor writes the hide-cursor escape sequence.
    #[test]
    fn sync_backend_hide_cursor() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend.hide_cursor().unwrap();
        backend.flush().unwrap();
        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(
            output.contains("\x1b[?25l"),
            "hide-cursor escape must be present"
        );
    }

    /// show_cursor writes the show-cursor escape sequence.
    #[test]
    fn sync_backend_show_cursor() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend.show_cursor().unwrap();
        backend.flush().unwrap();
        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(
            output.contains("\x1b[?25h"),
            "show-cursor escape must be present"
        );
    }

    /// set_cursor_position writes the cursor-position escape sequence.
    #[test]
    fn sync_backend_set_cursor_position() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend.set_cursor_position(Position::new(5, 3)).unwrap();
        backend.flush().unwrap();
        let output = String::from_utf8_lossy(backend.writer_ref());
        // CSI 5;4H (0-indexed col 5, row 3 -> 1-indexed 6;4)
        assert!(
            output.contains("\x1b[4;6H") || output.contains("\x1b[6;4H"),
            "cursor-position escape must be present, got: {output:?}"
        );
    }

    /// clear writes the clear-screen escape sequence.
    #[test]
    fn sync_backend_clear() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend.clear().unwrap();
        backend.flush().unwrap();
        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(
            output.contains("\x1b[2J"),
            "clear-screen escape must be present"
        );
    }

    /// clear_region writes the appropriate escape sequence per ClearType.
    #[test]
    fn sync_backend_clear_region() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend
            .clear_region(ratatui::backend::ClearType::All)
            .unwrap();
        backend.flush().unwrap();
        let output = String::from_utf8_lossy(backend.writer_ref());
        assert!(output.contains("\x1b[2J"), "clear All must write \\x1b[2J");
    }

    /// size returns the override when set.
    #[test]
    fn sync_backend_size_no_override() {
        // Without override, size() calls crossterm::terminal::size() which
        // needs a real TTY. We only assert the method exists and the
        // override field starts as None.
        let backend = SyncBackend::new(Vec::<u8>::new(), false);
        assert!(backend.size_override.is_none());
    }

    /// size returns the override value from new_for_test.
    #[test]
    fn sync_backend_size_with_override() {
        let backend = SyncBackend::new_for_test(
            Vec::<u8>::new(),
            false,
            (80, 24),
            Position::new(0, 0),
            WindowSize {
                columns_rows: Size::new(80, 24),
                pixels: Size::new(800, 600),
            },
        );
        assert_eq!(backend.size().unwrap(), Size::new(80, 24));
    }

    /// get_cursor_position returns the override when set.
    #[test]
    fn sync_backend_cursor_pos_override() {
        let mut backend = SyncBackend::new_for_test(
            Vec::<u8>::new(),
            false,
            (80, 24),
            Position::new(10, 5),
            WindowSize {
                columns_rows: Size::new(80, 24),
                pixels: Size::new(800, 600),
            },
        );
        assert_eq!(backend.get_cursor_position().unwrap(), Position::new(10, 5));
    }

    /// window_size returns the override when set.
    #[test]
    fn sync_backend_window_size_override() {
        let expected = WindowSize {
            columns_rows: Size::new(80, 24),
            pixels: Size::new(800, 600),
        };
        let mut backend = SyncBackend::new_for_test(
            Vec::<u8>::new(),
            false,
            (80, 24),
            Position::new(0, 0),
            expected,
        );
        let ws = backend.window_size().unwrap();
        assert_eq!(ws.columns_rows, Size::new(80, 24));
        assert_eq!(ws.pixels, Size::new(800, 600));
    }

    /// flush flushes the writer.
    #[test]
    fn sync_backend_flush() {
        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, false);
        backend.flush().unwrap();
        // No panic = pass. Vec<u8>::flush is a no-op.
    }

    // ---- TerminalFrameSink tests ----

    /// TerminalFrameSink::from_backend constructs and can draw.
    #[test]
    fn terminal_frame_sink_draw() {
        let backend = SyncBackend::new_for_test(
            Vec::<u8>::new(),
            true,
            (40, 5),
            Position::new(0, 0),
            WindowSize {
                columns_rows: Size::new(40, 5),
                pixels: Size::new(400, 50),
            },
        );
        let mut sink = TerminalFrameSink::from_backend(backend).unwrap();

        let state = RenderState::default();
        sink.draw(&state).unwrap();

        // The draw should have written something to the buffer.
        let output = String::from_utf8_lossy(sink.terminal_mut().backend().writer_ref());
        assert!(
            output.contains("\x1b[?2026h"),
            "BSU must be present in TerminalFrameSink output"
        );
        assert!(
            output.contains("\x1b[?2026l"),
            "ESU must be present in TerminalFrameSink output"
        );
    }

    /// terminal_mut returns a mutable reference to the terminal.
    #[test]
    fn terminal_frame_sink_terminal_mut() {
        let backend = SyncBackend::new_for_test(
            Vec::<u8>::new(),
            false,
            (40, 5),
            Position::new(0, 0),
            WindowSize {
                columns_rows: Size::new(40, 5),
                pixels: Size::new(400, 50),
            },
        );
        let mut sink = TerminalFrameSink::from_backend(backend).unwrap();
        // Just verify we can access the terminal.
        let size = sink.terminal_mut().backend().size().unwrap();
        assert_eq!(size, Size::new(40, 5));
    }

    // ---- CrosstermInput tests (with mock EventReader) ----

    /// A mock EventReader that returns canned events from a queue.
    /// Uses Arc<Mutex> instead of Rc<RefCell> because EventReader: Send.
    struct MockEventReader {
        events: Arc<Mutex<Vec<CrosstermEvent>>>,
        poll_returns: Arc<Mutex<Vec<bool>>>,
    }

    impl MockEventReader {
        fn new(events: Vec<CrosstermEvent>, poll_results: Vec<bool>) -> Self {
            Self {
                events: Arc::new(Mutex::new(events)),
                poll_returns: Arc::new(Mutex::new(poll_results)),
            }
        }
    }

    impl EventReader for MockEventReader {
        fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
            let mut queue = self.poll_returns.lock().unwrap();
            if queue.is_empty() {
                Ok(false)
            } else {
                Ok(queue.remove(0))
            }
        }

        fn read(&mut self) -> io::Result<CrosstermEvent> {
            let mut queue = self.events.lock().unwrap();
            if queue.is_empty() {
                Err(io::Error::other("no events"))
            } else {
                Ok(queue.remove(0))
            }
        }
    }

    /// CrosstermInput returns None when poll returns false (timeout).
    #[test]
    fn crossterm_input_poll_timeout() {
        let reader = MockEventReader::new(vec![], vec![false]);
        let mut input = CrosstermInput::with_reader(reader);
        let result = input.poll(Duration::from_millis(1)).unwrap();
        assert_eq!(result, None);
    }

    /// CrosstermInput translates a Key event to Quit.
    #[test]
    fn crossterm_input_key_event() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press);
        let reader = MockEventReader::new(vec![CrosstermEvent::Key(key)], vec![true]);
        let mut input = CrosstermInput::with_reader(reader);
        let result = input.poll(Duration::from_millis(1)).unwrap();
        assert_eq!(result, Some(InputEvent::Quit));
    }

    /// CrosstermInput returns None for non-key events (mouse, resize).
    #[test]
    fn crossterm_input_non_key_event() {
        let mouse_event = CrosstermEvent::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let reader = MockEventReader::new(vec![mouse_event], vec![true]);
        let mut input = CrosstermInput::with_reader(reader);
        let result = input.poll(Duration::from_millis(1)).unwrap();
        assert_eq!(result, None);
    }

    /// CrosstermInput propagates read errors (ADR 0009 fail closed).
    #[test]
    fn crossterm_input_propagates_read_error() {
        // poll returns true, but read has no events -> error.
        let reader = MockEventReader::new(vec![], vec![true]);
        let mut input = CrosstermInput::with_reader(reader);
        let result = input.poll(Duration::from_millis(1));
        assert!(result.is_err(), "read error must propagate");
    }

    /// CrosstermInput propagates poll errors (ADR 0009 fail closed).
    #[test]
    fn crossterm_input_propagates_poll_error() {
        struct ErrorReader;
        impl EventReader for ErrorReader {
            fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
                Err(io::Error::other("poll failed"))
            }
            fn read(&mut self) -> io::Result<CrosstermEvent> {
                Err(io::Error::other("read failed"))
            }
        }
        let mut input = CrosstermInput::with_reader(ErrorReader);
        let result = input.poll(Duration::from_millis(1));
        assert!(result.is_err(), "poll error must propagate");
    }

    /// CrosstermInput::new constructs with the real crossterm event reader.
    #[test]
    fn crossterm_input_new_constructs() {
        let _input = CrosstermInput::new();
        // Construction without panic = pass.
    }

    /// CrosstermInput::default constructs with the real crossterm event reader.
    #[test]
    fn crossterm_input_default_constructs() {
        let _input = CrosstermInput::default();
        // Construction without panic = pass.
    }
}
