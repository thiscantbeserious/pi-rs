//! SIGTSTP/SIGCONT suspend/resume handling (P14, ADR 0024) — **DEFERRED**.
//!
//! When the process receives SIGTSTP, the terminal must be restored before
//! the process suspends — otherwise raw mode, alt screen, and kitty keyboard
//! flags leak into the shell (P14). On SIGCONT, the terminal must be
//! re-entered so rendering resumes.
//!
//! **Status: deferred to Phase 3.** The testable units (`enter_ansi`,
//! `restore_ansi`) are implemented and unit-tested here. The full signal
//! wiring is deferred because `signal_hook::iterator::Signals` does not
//! reliably deliver SIGTSTP to the consumer thread on this platform (the
//! kernel's job-control handling interferes with the self-pipe delivery;
//! only SIGCONT arrives). The signal_hook docs' canonical fix
//! (`low_level::emulate_default_handler`) is `unsafe`, which the no-unsafe
//! rule (PHILOSOPHY §4) forbids without project-owner sign-off. P14 is an
//! interactive concern (a human hits Ctrl+Z); the Phase 2 replay gate does
//! not exercise it. It will be implemented in Phase 3 dogfood where a
//! real-terminal interactive test is meaningful and where the `unsafe`
//! decision can be made with the full context.
//!
//! No pi equivalent (signal handling is pi-rs-native, ADR 0013). Per §9.5.

use std::io;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

/// Compute the enter ANSI sequence (alt screen + mouse capture + optional
/// kitty keyboard push) as bytes, without touching real stdout.
pub fn enter_ansi(push_kitty: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = crossterm::queue!(buf, EnterAlternateScreen, EnableMouseCapture);
    if push_kitty {
        let _ = crossterm::queue!(
            buf,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    buf
}

/// Compute the restore ANSI sequence (leave alt screen + disable mouse +
/// optional kitty pop) as bytes, without touching real stdout.
pub fn restore_ansi(kitty_pushed: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = crossterm::queue!(buf, LeaveAlternateScreen, DisableMouseCapture);
    if kitty_pushed {
        let _ = crossterm::queue!(buf, PopKeyboardEnhancementFlags);
    }
    buf
}

/// Install the SIGTSTP/SIGCONT handlers.
///
/// **DEFERRED TO PHASE 3.** The signal wiring is not implemented in Phase 2
/// because `signal_hook::iterator::Signals` does not reliably deliver SIGTSTP
/// to the consumer thread (kernel job-control interference with the
/// self-pipe; only SIGCONT arrives). The canonical fix
/// (`signal_hook::low_level::emulate_default_handler`) is `unsafe`, which
/// the no-unsafe rule (PHILOSOPHY §4) forbids without project-owner
/// sign-off. P14 is an interactive concern; the Phase 2 replay gate does not
/// exercise it. Returns `Err` with a clear message until implemented in
/// Phase 3 dogfood.
pub fn install_suspend_handler(_restore: Vec<u8>, _enter: Vec<u8>) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "install_suspend_handler is deferred to Phase 3 (P14); see crates/pi-render/src/suspend.rs",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P14: enter_ansi must contain the alt-screen enter sequence.
    #[test]
    fn enter_ansi_contains_alt_screen() {
        let bytes = enter_ansi(false);
        assert!(bytes
            .windows(b"\x1b[?1049h".len())
            .any(|w| w == b"\x1b[?1049h"));
    }

    /// P14: restore_ansi must contain the alt-screen leave sequence.
    #[test]
    fn restore_ansi_contains_alt_screen() {
        let bytes = restore_ansi(false);
        assert!(bytes
            .windows(b"\x1b[?1049l".len())
            .any(|w| w == b"\x1b[?1049l"));
    }

    /// P14: enter_ansi with kitty must contain the push sequence.
    #[test]
    fn enter_ansi_pushes_kitty_when_requested() {
        let bytes = enter_ansi(true);
        assert!(bytes.windows(b"\x1b[>".len()).any(|w| w == b"\x1b[>"));
    }

    /// P14: restore_ansi with kitty must contain the pop sequence.
    #[test]
    fn restore_ansi_pops_kitty_when_pushed() {
        let bytes = restore_ansi(true);
        assert!(bytes.windows(b"\x1b[<1u".len()).any(|w| w == b"\x1b[<1u"));
    }
}
