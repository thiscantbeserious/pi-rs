//! SIGTSTP/SIGCONT suspend/resume handling (P14, ADR 0024).
//!
//! When the process receives SIGTSTP, the terminal must be restored before
//! the process suspends — otherwise raw mode, alt screen, and kitty keyboard
//! flags leak into the shell (P14). On SIGCONT, the terminal must be
//! re-entered so rendering resumes.
//!
//! Design: `signal_hook::iterator::Signals` bridges signals to a
//! self-pipe and delivers them as an iterator on a consumer thread. The
//! signal handler (inside the `signal-hook` crate, which owns the one
//! unavoidable `unsafe` for the async-signal-safe pipe write) only writes a
//! byte; our consumer thread reads the signal and does the safe work —
//! writing the pre-computed enter/restore ANSI bytes to stdout via
//! `io::Write`. This keeps all `unsafe` inside `signal-hook`; pi-rs code
//! has no `unsafe` blocks (PHILOSOPHY §4: no `unsafe` without project-owner
//! sign-off).
//!
//! No pi equivalent (signal handling is pi-rs-native, ADR 0013). Per §9.5.

use std::io::{self, Write};
use std::sync::OnceLock;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags, KeyboardEnhancementFlags,
};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use signal_hook::consts::{SIGCONT, SIGTSTP};
use signal_hook::iterator::Signals;

/// Pre-computed restore bytes (leave alt screen, disable mouse, pop kitty).
/// Set once at session setup; read by the consumer thread on SIGTSTP.
static RESTORE_BYTES: OnceLock<Vec<u8>> = OnceLock::new();

/// Pre-computed enter bytes (enter alt screen, enable mouse, push kitty).
/// Set once at session setup; read by the consumer thread on SIGCONT.
static ENTER_BYTES: OnceLock<Vec<u8>> = OnceLock::new();

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

/// Install the SIGTSTP/SIGCONT handlers. Pre-computes and stores the enter
/// and restore byte sequences, then spawns a consumer thread that reads
/// signals from a self-pipe and writes the sequences to stdout.
///
/// - On SIGTSTP: write restore bytes to stdout. The process then suspends
///   via the default action (SIGTSTP's default is "stop"; signal_hook's
///   iterator does not suppress default actions).
/// - On SIGCONT: write enter bytes to stdout so the terminal is re-entered.
///
/// Note: SIGTSTP's default action (stop the process) runs after the signal
/// is delivered to the iterator. The restore bytes are written before the
/// process actually stops. On SIGCONT, the process resumes and the enter
/// bytes are written.
pub fn install_suspend_handler(restore: Vec<u8>, enter: Vec<u8>) -> io::Result<()> {
    let _ = RESTORE_BYTES.set(restore);
    let _ = ENTER_BYTES.set(enter);

    let mut signals = Signals::new([SIGTSTP, SIGCONT])?;

    // Consumer thread: read signals from the self-pipe and write the
    // matching ANSI sequence to stdout. All work here is normal, safe code.
    std::thread::spawn(move || {
        for signal in signals.forever() {
            match signal {
                SIGTSTP => {
                    if let Some(restore) = RESTORE_BYTES.get() {
                        let _ = io::stdout().write_all(restore);
                        let _ = io::stdout().flush();
                    }
                }
                SIGCONT => {
                    if let Some(enter) = ENTER_BYTES.get() {
                        let _ = io::stdout().write_all(enter);
                        let _ = io::stdout().flush();
                    }
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P14: enter_ansi must contain the alt-screen enter sequence.
    #[test]
    fn enter_ansi_contains_alt_screen() {
        let bytes = enter_ansi(false);
        assert!(bytes.windows(b"\x1b[?1049h".len()).any(|w| w == b"\x1b[?1049h"));
    }

    /// P14: restore_ansi must contain the alt-screen leave sequence.
    #[test]
    fn restore_ansi_contains_alt_screen() {
        let bytes = restore_ansi(false);
        assert!(bytes.windows(b"\x1b[?1049l".len()).any(|w| w == b"\x1b[?1049l"));
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
