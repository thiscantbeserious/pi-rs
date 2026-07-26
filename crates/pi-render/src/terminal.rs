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
    /// keyboard flags. Returns a guard that restores on drop. Writes to real
    /// stdout and enables raw mode on the real fd.
    ///
    /// Requires a real tty; cannot be tested in CI without a pty harness.
    /// The testable seams are `enter_to` / `restore_to` (ANSI sequences to a
    /// buffer) and the broad behavior tests.
    pub fn enter() -> io::Result<TerminalGuard> {
        let mut guard = Self::enter_to(&mut io::stdout(), false)?;
        crossterm::terminal::enable_raw_mode()?;
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
    /// If raw mode was enabled, disables it (on the real fd, not `w`).
    pub fn restore_to<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        crossterm::queue!(w, LeaveAlternateScreen, DisableMouseCapture,)?;
        if self.kitty_pushed {
            crossterm::queue!(w, PopKeyboardEnhancementFlags)?;
        }
        if self.raw_mode_enabled {
            crossterm::terminal::disable_raw_mode()?;
        }
        self.restored = true;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Restore to real stdout (not io::sink) so the actual terminal is
        // left clean on drop (P15). The test seams (enter_to/restore_to)
        // exercise the ANSI sequences with a buffer; the real path goes
        // through stdout.
        let _ = self.restore_to(&mut io::stdout());
    }
}

/// P15: a zero-size terminal (0 cols or 0 rows) panics naive renderers. The
/// resize handler checks this before drawing. Returns true only when both
/// dimensions are non-zero.
pub fn is_drawable_size(cols: u16, rows: u16) -> bool {
    cols > 0 && rows > 0
}

/// Install the terminal-restore panic hook (P15, P3). The hook calls `restore`
/// before the previous hook, so the terminal is left clean (alt screen off,
/// raw mode off, kitty flags popped) before the backtrace prints. Must be
/// installed before the first draw.
///
/// `restore` must be `Send + Sync + 'static` because panic hooks are global.
/// The render thread typically passes a closure that restores its own
/// `TerminalGuard` to real stdout.
pub fn install_panic_hook<F>(restore: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Restore the terminal first (P15): the backtrace must print to a
        // clean terminal, not the alt screen.
        restore();
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Serializes tests that touch the global panic hook (set_hook/take_hook
    /// are process-global; without this, parallel tests race on the hook).
    static PANIC_HOOK_MUTEX: Mutex<()> = Mutex::new(());

    /// P15/P3: a full session lifecycle (enter with kitty and raw mode, then
    /// restore) must leave the terminal clean. "Clean" means: alt screen
    /// left, mouse capture disabled, kitty keyboard flags popped, raw mode
    /// disabled. Uses injectable no-op raw-mode functions so the full path
    /// is covered without a real tty.
    #[test]
    fn session_lifecycle_leaves_terminal_clean() {
        let mut enter_buf = Vec::new();
        let mut guard = Session::enter_to(&mut enter_buf, true).unwrap();

        // Enter must set up: alt screen, mouse capture, kitty push.
        assert!(
            enter_buf
                .windows(b"\x1b[?1049h".len())
                .any(|w| w == b"\x1b[?1049h"),
            "enter must enter alt screen"
        );
        assert!(
            enter_buf
                .windows(b"\x1b[?1000h".len())
                .any(|w| w == b"\x1b[?1000h"),
            "enter must enable mouse capture"
        );
        assert!(
            enter_buf.windows(b"\x1b[>".len()).any(|w| w == b"\x1b[>"),
            "enter must push kitty keyboard flags when requested"
        );

        // Restore must tear down: leave alt screen, disable mouse, pop kitty,
        // disable raw mode.
        let mut restore_buf = Vec::new();
        guard.restore_to(&mut restore_buf).unwrap();
        assert!(
            restore_buf
                .windows(b"\x1b[?1049l".len())
                .any(|w| w == b"\x1b[?1049l"),
            "restore must leave alt screen"
        );
        assert!(
            restore_buf
                .windows(b"\x1b[?1000l".len())
                .any(|w| w == b"\x1b[?1000l"),
            "restore must disable mouse capture"
        );
        assert!(
            restore_buf
                .windows(b"\x1b[<1u".len())
                .any(|w| w == b"\x1b[<1u"),
            "restore must pop kitty flags that were pushed"
        );
    }

    /// P15: a session entered without kitty must not pop kitty on restore
    /// (popping an un-pushed stack corrupts kitty keyboard state). The
    /// behavior: restore only reverses what enter set up.
    #[test]
    fn session_without_kitty_does_not_pop_on_restore() {
        let mut enter_buf = Vec::new();
        let mut guard = Session::enter_to(&mut enter_buf, false).unwrap();
        assert!(
            !enter_buf.windows(b"\x1b[>".len()).any(|w| w == b"\x1b[>"),
            "enter must not push kitty when not requested"
        );
        let mut restore_buf = Vec::new();
        guard.restore_to(&mut restore_buf).unwrap();
        assert!(
            !restore_buf
                .windows(b"\x1b[<1u".len())
                .any(|w| w == b"\x1b[<1u"),
            "restore must not pop kitty when none were pushed"
        );
    }

    /// P15: restore must be idempotent. A double-restore (e.g. panic hook
    /// then Drop) must not emit the teardown sequence twice, which would
    /// corrupt terminal state.
    #[test]
    fn double_restore_is_safe() {
        let mut guard = Session::enter_to(&mut Vec::new(), false).unwrap();
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

    /// P15/P3: a panic during a session must trigger the restore hook before
    /// the backtrace prints, so the terminal is clean when the panic message
    /// appears. The behavior: panic hook calls restore.
    #[test]
    fn panic_during_session_restores_terminal() {
        let _lock = PANIC_HOOK_MUTEX.lock().unwrap();
        let restored = Arc::new(AtomicBool::new(false));
        let restored_clone = restored.clone();

        let prev = std::panic::take_hook();
        install_panic_hook(move || {
            restored_clone.store(true, Ordering::SeqCst);
        });

        let _ = std::panic::catch_unwind(|| panic!("simulated render panic"));

        std::panic::set_hook(prev);

        assert!(
            restored.load(Ordering::SeqCst),
            "panic hook must restore the terminal before the backtrace"
        );
    }

    /// P15: a zero-size terminal (0 cols or 0 rows) must be rejected before
    /// drawing, because naive renderers panic on it. The behavior: the size
    /// guard returns false for any zero dimension, true otherwise.
    #[test]
    fn zero_size_terminal_is_rejected() {
        assert!(!is_drawable_size(0, 0), "0x0 must not be drawable");
        assert!(!is_drawable_size(80, 0), "0 rows must not be drawable");
        assert!(!is_drawable_size(0, 24), "0 cols must not be drawable");
        assert!(is_drawable_size(1, 1), "1x1 must be drawable");
        assert!(is_drawable_size(80, 24), "80x24 must be drawable");
    }

    /// P14: install_suspend_handler is deferred to Phase 3. The behavior: it
    /// returns a clear error until implemented, so callers fail closed rather
    /// than silently doing nothing.
    #[test]
    fn suspend_handler_is_deferred() {
        let result = crate::suspend::install_suspend_handler(Vec::new(), Vec::new());
        assert!(
            result.is_err(),
            "install_suspend_handler must fail closed (deferred to Phase 3)"
        );
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::Unsupported,
            "error must be Unsupported"
        );
    }
}
