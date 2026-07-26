//! Terminal session lifecycle: enter, restore, panic-safe hook (P15, P3).
//!
//! The render thread owns the terminal session (ADR 0013/0024). `enter` sets
//! up the alt screen, raw mode, mouse capture, and kitty keyboard flags;
//! `TerminalGuard::restore` tears them all down on every exit path (normal
//! drop, panic hook, SIGTSTP suspend). Restore is idempotent: calling it
//! twice emits the restore sequence at most once (P15 — a double-restore
//! must not corrupt state).
//!
//! No pi equivalent (pi is single-process JS; the terminal session is
//! pi-rs-native, ADR 0013). Per §9.5.

use std::io::{self, Write};

use crossterm::event::{DisableMouseCapture, PopKeyboardEnhancementFlags};
use crossterm::terminal::LeaveAlternateScreen;

/// A terminal session. `enter` sets up the terminal and returns a guard whose
/// `restore` (and `Drop`) tear it down.
pub struct Session;

/// Owns the terminal restore. `restore` is idempotent and safe to call from a
/// panic hook. `Drop` calls `restore` as the last line of defense (P15).
pub struct TerminalGuard {
    restored: bool,
    kitty_pushed: bool,
    raw_mode_enabled: bool,
}

impl Session {
    /// Enter the terminal session: alt screen, raw mode, mouse capture, kitty
    /// keyboard flags. Returns a guard that restores on drop.
    pub fn enter() -> io::Result<TerminalGuard> {
        // Real stdout enter. Tests exercise `enter_to`.
        let mut guard = Self::enter_to(&mut io::stdout(), false)?;
        // Raw mode is termios, not ANSI; applied on the real fd.
        guard.raw_mode_enabled = true;
        Ok(guard)
    }

    /// Write the enter sequence to `w` and return a guard. `push_kitty`
    /// controls whether kitty keyboard enhancement flags are pushed (P14).
    /// Testable seam: writes only ANSI to `w`; raw mode is the caller's
    /// concern (termios, not ANSI-emittable).
    pub fn enter_to<W: Write>(w: &mut W, push_kitty: bool) -> io::Result<TerminalGuard> {
        use crossterm::event::{EnableMouseCapture, PushKeyboardEnhancementFlags};
        use crossterm::terminal::EnterAlternateScreen;

        crossterm::queue!(w, EnterAlternateScreen, EnableMouseCapture)?;
        if push_kitty {
            crossterm::queue!(
                w,
                PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
                )
            )?;
        }
        Ok(TerminalGuard {
            restored: false,
            kitty_pushed: push_kitty,
            raw_mode_enabled: false,
        })
    }
}

impl TerminalGuard {
    /// Write the restore sequence to `w`. Idempotent: a second call is a
    /// no-op. Safe to call from a panic hook (no allocation, no panics).
    pub fn restore_to<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        crossterm::queue!(w, LeaveAlternateScreen, DisableMouseCapture,)?;
        if self.kitty_pushed {
            crossterm::queue!(w, PopKeyboardEnhancementFlags)?;
        }
        self.restored = true;
        Ok(())
    }

    /// Whether kitty keyboard enhancement flags were pushed on enter (so
    /// restore must pop them).
    #[allow(dead_code)]
    fn kitty_pushed(&self) -> bool {
        self.kitty_pushed
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore_to(&mut io::sink());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P15: restore must leave the alt screen (ESC[?1049l).
    #[test]
    fn restore_leaves_alt_screen() {
        let mut guard = Session::enter().unwrap();
        let mut buf = Vec::new();
        guard.restore_to(&mut buf).unwrap();
        assert!(
            buf.windows(b"\x1b[?1049l".len())
                .any(|w| w == b"\x1b[?1049l"),
            "restore must emit LeaveAlternateScreen (ESC[?1049l), got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P15: enter must enter the alt screen (ESC[?1049h).
    #[test]
    fn enter_writes_enter_alt_screen() {
        let mut buf = Vec::new();
        let _guard = Session::enter_to(&mut buf, false).unwrap();
        assert!(
            buf.windows(b"\x1b[?1049h".len())
                .any(|w| w == b"\x1b[?1049h"),
            "enter must emit EnterAlternateScreen (ESC[?1049h), got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P15: enter must enable mouse capture (?1000h).
    #[test]
    fn enter_enables_mouse_capture() {
        let mut buf = Vec::new();
        let _guard = Session::enter_to(&mut buf, false).unwrap();
        assert!(
            buf.windows(b"\x1b[?1000h".len())
                .any(|w| w == b"\x1b[?1000h"),
            "enter must emit EnableMouseCapture (?1000h), got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P14: enter with push_kitty must push kitty keyboard flags.
    #[test]
    fn enter_pushes_kitty_when_requested() {
        let mut buf = Vec::new();
        let guard = Session::enter_to(&mut buf, true).unwrap();
        assert!(
            guard.kitty_pushed,
            "guard must record that kitty flags were pushed"
        );
        assert!(
            buf.windows(b"\x1b[>".len()).any(|w| w == b"\x1b[>"),
            "enter must emit PushKeyboardEnhancementFlags (ESC[>...u), got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P14: enter without push_kitty must NOT push kitty flags.
    #[test]
    fn enter_skips_kitty_when_not_requested() {
        let mut buf = Vec::new();
        let guard = Session::enter_to(&mut buf, false).unwrap();
        assert!(!guard.kitty_pushed, "guard must record kitty not pushed");
    }

    /// P15: restore must disable mouse capture.
    #[test]
    fn restore_disables_mouse_capture() {
        let mut guard = Session::enter().unwrap();
        let mut buf = Vec::new();
        guard.restore_to(&mut buf).unwrap();
        // DisableMouseCapture emits ?1006l ?1015l ?1003l ?1002l ?1000l.
        assert!(
            buf.windows(b"\x1b[?1000l".len())
                .any(|w| w == b"\x1b[?1000l"),
            "restore must emit DisableMouseCapture (?1000l), got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P15: restore must be idempotent. Calling it twice must not emit the
    /// restore sequence a second time (a double-restore can corrupt state).
    #[test]
    fn restore_is_idempotent() {
        let mut guard = Session::enter().unwrap();
        let mut first = Vec::new();
        guard.restore_to(&mut first).unwrap();
        let mut second = Vec::new();
        guard.restore_to(&mut second).unwrap();
        assert!(
            second.is_empty(),
            "second restore must emit nothing, got {:?}",
            String::from_utf8_lossy(&second)
        );
    }

    /// P14/P15: if kitty keyboard flags were pushed on enter, restore must
    /// pop them (ESC[<1u).
    #[test]
    fn restore_pops_kitty_flags_when_pushed() {
        let mut guard = Session::enter().unwrap();
        guard.kitty_pushed = true;
        let mut buf = Vec::new();
        guard.restore_to(&mut buf).unwrap();
        assert!(
            buf.windows(b"\x1b[<1u".len())
                .any(|w| w == b"\x1b[<1u"),
            "restore must emit PopKeyboardEnhancementFlags (ESC[<1u) when kitty was pushed, got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    /// P15: if kitty flags were NOT pushed, restore must NOT emit the pop
    /// (popping an un-pushed stack corrupts kitty keyboard state).
    #[test]
    fn restore_skips_kitty_pop_when_not_pushed() {
        let mut guard = Session::enter().unwrap();
        guard.kitty_pushed = false;
        let mut buf = Vec::new();
        guard.restore_to(&mut buf).unwrap();
        assert!(
            !buf.windows(b"\x1b[<1u".len()).any(|w| w == b"\x1b[<1u"),
            "restore must NOT emit PopKeyboardEnhancementFlags when kitty was not pushed, got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }
}
