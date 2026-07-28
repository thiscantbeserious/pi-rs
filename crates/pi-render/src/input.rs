//! The input reader thread (ADR 0030).
//!
//! A dedicated OS thread reads input via [`InputSource`] and sends
//! [`LoopEvent::Input`](crate::thread::LoopEvent) over the unified channel.
//! This decouples input reading from the render loop so the render thread can
//! block on the channel (waking immediately for both input and render events)
//! without blocking on `input.poll`.
//!
//! Input latency is ~0ms: a keystroke is read immediately by the reader and
//! delivered through the channel. Quit latency on the reader side is <=4ms
//! (the reader checks the quit flag every [`READER_POLL`]).
//!
//! No pi equivalent (pi is single-process JS; the reader thread is pi-rs-native,
//! ADR 0030). Per PHILOSOPHY §9.5.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::thread::LoopEvent;

/// Input the render thread acts on. Step 2 handles only [`InputEvent::Quit`];
/// routing (focus, scrollback, copy-mode) is Phase 3 (ADR 0003, ADR 0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// The user requested quit (Ctrl+C / 'q' in Phase 3).
    Quit,
}

/// The reader thread's input source (ADR 0013, ADR 0030). The reader thread
/// owns input reading. `poll` provides the reader's cadence: it blocks up to
/// `timeout`, returning `Ok(None)` on timeout. Real crossterm input lands in
/// Step 3; Step 2 uses [`NullInput`] for tests.
pub trait InputSource: Send {
    /// Block up to `timeout` for input. Returns `Ok(None)` on timeout, or an
    /// input event. Synchronous (ADR 0013: never awaits).
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>>;
}

/// 4ms reader-thread poll (quit-check cadence for the reader; ADR 0030).
pub(crate) const READER_POLL: Duration = Duration::from_millis(4);

/// The input reader thread loop (ADR 0030). Reads input and sends
/// [`LoopEvent::Input`] over the channel. Exits when `quit_flag` is set.
/// `send` blocks when the channel is full (backpressure: a slow render thread
/// throttles input reading, which is correct).
pub(crate) fn reader_loop<I: InputSource>(
    mut input: I,
    tx: SyncSender<LoopEvent>,
    quit_flag: Arc<AtomicBool>,
) {
    while !quit_flag.load(Ordering::Acquire) {
        match input.poll(READER_POLL) {
            Ok(Some(ev)) => {
                if tx.send(LoopEvent::Input(ev)).is_err() {
                    break; // channel closed (render thread exited)
                }
            }
            Ok(None) => {}
            Err(_) => {} // flaky input read; continue
        }
    }
}

/// Test input source: blocks for the timeout (cadence) then returns no input.
/// Mimics `crossterm::event::poll`'s blocking-with-timeout without touching
/// stdin.
pub struct NullInput;

impl InputSource for NullInput {
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        thread::sleep(timeout);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `NullInput` blocks for the timeout then returns None (cadence provider).
    #[test]
    fn null_input_blocks_then_returns_none() {
        let mut input = NullInput;
        let start = std::time::Instant::now();
        let res = input.poll(Duration::from_millis(10)).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(8));
        assert_eq!(res, None);
    }

    /// reader_loop exits when quit_flag is set.
    #[test]
    fn reader_loop_exits_on_quit_flag() {
        let (tx, _rx) = std::sync::mpsc::sync_channel::<LoopEvent>(16);
        let quit_flag = Arc::new(AtomicBool::new(true));
        reader_loop(NullInput, tx, quit_flag);
        // Returns immediately because quit_flag is already set.
    }

    /// reader_loop handles input errors gracefully (continues).
    #[test]
    fn reader_loop_handles_input_error() {
        struct ErrorInput(Arc<AtomicBool>);
        impl InputSource for ErrorInput {
            fn poll(&mut self, _timeout: Duration) -> io::Result<Option<InputEvent>> {
                self.0.store(true, Ordering::Release);
                Err(io::Error::other("flaky"))
            }
        }
        let (tx, _rx) = std::sync::mpsc::sync_channel::<LoopEvent>(16);
        let quit_flag = Arc::new(AtomicBool::new(false));
        let qf = quit_flag.clone();
        // ErrorInput sets quit_flag on first poll, so the loop exits.
        reader_loop(ErrorInput(qf), tx, quit_flag);
        // No panic = pass.
    }

    /// reader_loop sends InputEvent::Quit through the channel.
    #[test]
    fn reader_loop_sends_input() {
        struct QuitInput(Arc<AtomicBool>);
        impl InputSource for QuitInput {
            fn poll(&mut self, _timeout: Duration) -> io::Result<Option<InputEvent>> {
                self.0.store(true, Ordering::Release);
                Ok(Some(InputEvent::Quit))
            }
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<LoopEvent>(16);
        let quit_flag = Arc::new(AtomicBool::new(false));
        let qf = quit_flag.clone();
        let handle = thread::spawn(move || reader_loop(QuitInput(qf), tx, quit_flag));
        // Receive the InputEvent::Quit.
        let ev = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(ev, LoopEvent::Input(InputEvent::Quit)));
        // The QuitInput sets quit_flag on first poll, so the loop exits.
        handle.join().unwrap();
    }

    /// reader_loop exits when channel is closed (send fails).
    #[test]
    fn reader_loop_exits_on_channel_closed() {
        struct AlwaysInput;
        impl InputSource for AlwaysInput {
            fn poll(&mut self, _timeout: Duration) -> io::Result<Option<InputEvent>> {
                Ok(Some(InputEvent::Quit))
            }
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<LoopEvent>(16);
        let quit_flag = Arc::new(AtomicBool::new(false));
        drop(rx); // close the channel
        reader_loop(AlwaysInput, tx, quit_flag);
        // Returns because send fails (channel closed).
    }
}
