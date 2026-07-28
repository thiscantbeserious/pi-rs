//! The render thread loop (ADR 0013, ADR 0030).
//!
//! The render thread blocks on `recv_timeout(16ms)` on the unified channel,
//! drains the rest non-blocking, applies render events to the Retained Message
//! Model single-threaded, draws if dirty. Never awaits (ADR 0013).
//!
//! `Quit` arrives through the channel (waking `recv_timeout` immediately) and
//! via the shared quit flag (checked after `recv_timeout` returns). Both paths
//! exit the loop. Quit latency is ~0ms: a `Quit` on the channel wakes the
//! block instantly.
//!
//! Step 2 establishes the loop. The "project" step (RMM to cells) and the
//! mode-2026-wrapped flush land in Step 3. Step 2's draw is an injectable
//! [`FrameSink`] so the loop is testable without a real terminal.
//!
//! No pi equivalent (pi is single-process JS; the render thread is pi-rs-native,
//! ADR 0013). Per PHILOSOPHY §9.5.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use crate::event::RenderEvent;
use crate::input::InputEvent;
use crate::state::RenderState;
use crate::thread::LoopEvent;

/// The render thread's draw target (ADR 0013 / ADR 0024). Step 3's impl
/// flushes the ratatui `Buffer::diff` to the terminal wrapped in mode 2026
/// (pitfall P12). Step 2 uses a counting sink for tests.
pub trait FrameSink: Send {
    /// Draw the current state. Synchronous (ADR 0013).
    fn draw(&mut self, state: &RenderState) -> io::Result<()>;
}

/// 16ms channel-block timeout (ADR 0010 coalescing cadence; ADR 0013 loop).
pub(crate) const FRAME_POLL: Duration = Duration::from_millis(16);

/// Drain all pending [`LoopEvent`]s non-blocking (ADR 0013). Returns
/// immediately with whatever is available; an empty channel yields an empty
/// vec.
pub(crate) fn drain_events(rx: &Receiver<LoopEvent>) -> Vec<LoopEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

/// The render thread loop (ADR 0013, ADR 0030). Blocks on `recv_timeout`
/// (wakes immediately for input, render events, or Quit), drains the rest,
/// applies render events single-threaded, draws if dirty. Exits on Quit.
pub(crate) fn run_loop<D: FrameSink>(
    mut sink: D,
    rx: Receiver<LoopEvent>,
    quit_flag: Arc<AtomicBool>,
) {
    let mut state = RenderState::default();
    while !state.quit {
        // 1. Block on the channel (ADR 0030). Wakes immediately for input,
        //    render events, or Quit. Timeout provides the 16ms cadence.
        let mut events = match rx.recv_timeout(FRAME_POLL) {
            Ok(ev) => vec![ev],
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => {
                state.quit = true;
                break;
            }
        };
        // 2. Drain remaining non-blocking (coalescing, ADR 0010).
        events.extend(drain_events(&rx));
        // 3. Partition into render events and input signals.
        let mut input_quit = false;
        let render_events: Vec<RenderEvent> = events
            .into_iter()
            .filter_map(|e| match e {
                LoopEvent::Render(re) => Some(re),
                LoopEvent::Input(InputEvent::Quit) => {
                    input_quit = true;
                    None
                }
            })
            .collect();
        // 4. Apply single-threaded (ADR 0013). Step 3 projects the RMM here.
        let dirty = state.apply(&render_events);
        if input_quit || quit_flag.load(Ordering::Acquire) {
            state.quit = true;
        }
        // 5. Draw if dirty (ADR 0010). Step 3 wraps flush in mode 2026 (P12).
        if dirty {
            let _ = sink.draw(&state);
        }
    }
}

/// Test frame sink: publishes `state.applied` into an atomic so a test can
/// observe "event applied before the next frame" without a real terminal.
pub struct CountingSink {
    observed: Arc<AtomicBool>,
}

impl CountingSink {
    /// Build a sink that sets `observed` to true once it has drawn a frame
    /// reflecting at least one applied event.
    pub fn new(observed: Arc<AtomicBool>) -> Self {
        Self { observed }
    }
}

impl FrameSink for CountingSink {
    fn draw(&mut self, state: &RenderState) -> io::Result<()> {
        if !state.messages().is_empty() {
            self.observed.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    /// ADR 0013: the render thread drains its event channel non-blocking. An
    /// empty channel returns immediately (no await, no block).
    #[test]
    fn drain_is_nonblocking_when_empty() {
        let (_tx, rx) = mpsc::sync_channel::<LoopEvent>(16);
        let drained = drain_events(&rx);
        assert!(
            drained.is_empty(),
            "drain on empty channel must return immediately"
        );
    }

    /// ADR 0013: drain collects ALL pending events in one pass (coalescing).
    #[test]
    fn drain_collects_all_pending() {
        use crate::event::RenderEvent;
        let (tx, rx) = mpsc::sync_channel::<LoopEvent>(16);
        tx.send(LoopEvent::Render(RenderEvent::Quit)).unwrap();
        tx.send(LoopEvent::Render(RenderEvent::Quit)).unwrap();
        tx.send(LoopEvent::Render(RenderEvent::Quit)).unwrap();
        let drained = drain_events(&rx);
        assert_eq!(drained.len(), 3, "drain must collect all pending events");
    }
}
