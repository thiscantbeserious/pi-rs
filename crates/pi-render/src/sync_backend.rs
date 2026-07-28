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
//! No pi equivalent (pi is single-process JS; the terminal backend is
//! pi-rs-native, ADR 0013). Per PHILOSOPHY §9.5.

use std::io::{self, Write};

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::QueueableCommand;
use ratatui::backend::{Backend, CrosstermBackend};
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
pub struct SyncBackend<W: Write> {
    writer: W,
    mode_2026_enabled: bool,
}

impl<W: Write> SyncBackend<W> {
    /// Create a new SyncBackend wrapping a writer.
    /// `mode_2026_enabled` controls whether BSU/ESU is injected.
    pub fn new(writer: W, mode_2026_enabled: bool) -> Self {
        Self {
            writer,
            mode_2026_enabled,
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
        CrosstermBackend::new(&mut self.writer).draw(content)?;
        if self.mode_2026_enabled {
            self.writer.queue(EndSynchronizedUpdate)?;
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<ratatui::layout::Position> {
        CrosstermBackend::new(&mut self.writer).get_cursor_position()
    }

    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear()
    }

    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear_region(clear_type)
    }

    fn size(&self) -> io::Result<ratatui::layout::Size> {
        // crossterm::terminal::size uses the real terminal, not the writer.
        crossterm::terminal::size().map(|(w, h)| ratatui::layout::Size::new(w, h))
    }

    fn window_size(&mut self) -> io::Result<ratatui::backend::WindowSize> {
        CrosstermBackend::new(&mut self.writer).window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// The real `FrameSink` impl: owns a `Terminal<SyncBackend<Stdout>>` and
/// calls `try_draw` with the `RmmProjection` (ADR 0031).
pub struct TerminalFrameSink {
    terminal: Terminal<SyncBackend<io::Stdout>>,
}

impl TerminalFrameSink {
    /// Create a new TerminalFrameSink with mode 2026 support.
    pub fn new(mode_2026_enabled: bool) -> io::Result<Self> {
        let backend = SyncBackend::new(io::stdout(), mode_2026_enabled);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Access the terminal for direct commands (resize, etc.).
    pub fn terminal_mut(&mut self) -> &mut Terminal<SyncBackend<io::Stdout>> {
        &mut self.terminal
    }
}

impl FrameSink for TerminalFrameSink {
    fn draw(&mut self, state: &RenderState) -> io::Result<()> {
        self.terminal.try_draw(|frame| {
            frame.render_widget(RmmProjection::new(state), frame.area());
            Ok::<(), io::Error>(())
        })?;
        Ok(())
    }
}

/// The real `InputSource` impl: polls crossterm events and translates to
/// `InputEvent` (ADR 0030).
pub struct CrosstermInput;

impl InputSource for CrosstermInput {
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        if event::poll(timeout)? {
            if let Ok(CrosstermEvent::Key(key)) = event::read() {
                return Ok(translate_key(key));
            }
        }
        Ok(None)
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

    /// SyncBackend::draw with mode 2026 injects BSU/ESU.
    #[test]
    fn sync_backend_draw_injects_bsu_esu() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let buf = Vec::new();
        let mut backend = SyncBackend::new(buf, true);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

        // Draw: should inject BSU before and ESU after.
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
        // BSU = \x1b[?2026h, ESU = \x1b[?2026l
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
}
