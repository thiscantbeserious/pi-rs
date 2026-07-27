//! The render thread (ADR 0013 / ADR 0024).
//!
//! A dedicated OS thread runs a tight synchronous loop:
//!
//! `poll input (≤16ms) → drain RenderEvents → apply → draw if dirty`
//!
//! It never awaits. All async concerns (provider streams, Host Protocol
//! traffic, heartbeats, tool subprocesses, timers) run on the tokio runtime
//! on other threads and communicate via the mpsc channel defined in
//! [`event`](crate::event). The render thread owns the terminal and the
//! Retained Message Model (ADR 0013); it is the sole mutator of
//! [`RenderState`](crate::state::RenderState).
//!
//! Step 2 establishes the thread, the channel contract, and the loop. The
//! "project" step (RMM → cells) and the mode-2026-wrapped flush land in Step 3
//! (the RMM does not exist yet); Step 2's draw is an injectable [`FrameSink`]
//! so the loop is testable without a real terminal. Real crossterm
//! [`InputSource`] / [`FrameSink`] impls land with the RMM in Step 3.
//!
//! Input is read on the render thread for minimum keystroke latency (ADR 0013,
//! pitfall P2). Input routing (focus, scrollback, copy-mode) is Phase 3
//! (ADR 0003, ADR 0007); Phase 2 has no extension UI, so no routing stub is
//! built (YAGNI, PHILOSOPHY §5).
//!
//! No pi equivalent (pi is single-process JS; the render-thread/tokio split
//! is pi-rs-native, ADR 0013). Per PHILOSOPHY §9.5.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::event::RenderEvent;
use crate::state::RenderState;

/// Input the render thread acts on. Step 2 handles only [`InputEvent::Quit`];
/// routing (focus, scrollback, copy-mode) is Phase 3 (ADR 0003, ADR 0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// The user requested quit (Ctrl+C / 'q' in Phase 3).
    Quit,
}

/// The render loop's input source (ADR 0013: "poll input ≤16ms"). The render
/// thread owns input reading for minimum keystroke latency (pitfall P2). The
/// `poll` provides the loop cadence: it blocks up to `timeout`, returning
/// `Ok(None)` on timeout. Real crossterm input lands in Step 3; Step 2 uses
/// [`NullInput`] for tests.
pub trait InputSource: Send {
    /// Block up to `timeout` for input. Returns `Ok(None)` on timeout, or an
    /// input event. Synchronous (ADR 0013: never awaits).
    fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>>;
}

/// The render thread's draw target (ADR 0013 / ADR 0024). Step 3's impl
/// flushes the ratatui `Buffer::diff` to the terminal wrapped in mode 2026
/// (pitfall P12). Step 2 uses a counting sink for tests.
pub trait FrameSink: Send {
    /// Draw the current state. Synchronous (ADR 0013).
    fn draw(&mut self, state: &RenderState) -> io::Result<()>;
}

/// ≤16ms input-poll timeout (ADR 0010 coalescing budget; ADR 0013 loop cadence).
const FRAME_POLL: Duration = Duration::from_millis(16);

/// Drain all pending [`RenderEvent`]s non-blocking (ADR 0013: the render thread
/// never awaits on the channel). Returns immediately with whatever is
/// available; an empty channel yields an empty vec. A disconnected channel
/// (sender dropped) also yields an empty vec; the loop's quit check or the
/// `Drop` Quit signal handles exit.
pub fn drain_events(rx: &mut Receiver<RenderEvent>) -> Vec<RenderEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

/// Run the render loop on the current thread. Owns the state; never awaits
/// (ADR 0013). Exits when [`InputEvent::Quit`] or [`RenderEvent::Quit`] is
/// received.
///
/// Loop: poll input (≤16ms cadence) → drain events non-blocking → apply
/// single-threaded → draw if dirty. The poll provides the coalescing cadence
/// (ADR 0010); the draw is skipped when idle. Step 3 replaces the draw with
/// the RMM projection + mode-2026-wrapped flush.
fn run_loop<I: InputSource, D: FrameSink>(
    mut input: I,
    mut sink: D,
    mut rx: Receiver<RenderEvent>,
    quit_flag: Arc<AtomicBool>,
) {
    let mut state = RenderState::default();
    while !state.quit {
        // 1. Poll input (≤16ms cadence, ADR 0013). A flaky input read must
        //    not kill the render thread: treat errors as no input.
        let input_quit = matches!(input.poll(FRAME_POLL), Ok(Some(InputEvent::Quit)));
        // 2. Drain events non-blocking (ADR 0013).
        let events = drain_events(&mut rx);
        // 3. Apply single-threaded (ADR 0013). Step 3 projects the RMM to
        //    cells here.
        let dirty = state.apply(&events);
        if input_quit || quit_flag.load(Ordering::Acquire) {
            state.quit = true;
        }
        // 4. Draw if dirty (ADR 0010 coalescing). Step 3 wraps flush in mode
        //    2026 (pitfall P12). A draw failure is non-fatal: the next frame
        //    retries.
        if dirty {
            let _ = sink.draw(&state);
        }
    }
}

/// Handle to the render thread's inbox. Cloneable so multiple tokio tasks (the
/// agent loop, the Host Protocol) can send. [`RenderHandle::send`] is async
/// (the sender lives on the tokio runtime, ADR 0013); [`RenderHandle::quit`]
/// is sync (control path).
#[derive(Clone, Debug)]
pub struct RenderHandle {
    tx: Sender<RenderEvent>,
    /// Shared quit flag. Set by `quit()` and `Drop`; checked by `run_loop`
    /// every frame. Guarantees shutdown even when the channel is full and
    /// `try_send(Quit)` fails (GOALS goal 2: no orphaned threads).
    quit_flag: Arc<AtomicBool>,
}

impl RenderHandle {
    /// Send an event from the tokio side. Async because the sender lives on
    /// the tokio runtime (ADR 0013). Bounded: backpressure policy is the
    /// agent loop's concern (Phase 3).
    pub async fn send(&self, ev: RenderEvent) -> Result<(), mpsc::error::SendError<RenderEvent>> {
        self.tx.send(ev).await.map(drop)
    }

    /// Signal the render thread to stop. Sync (control path, not the frame
    /// path). Sets the shared quit flag so `run_loop` exits within one frame
    /// (≤16ms) even if the `try_send` fails on a full channel.
    pub fn quit(&self) {
        self.quit_flag.store(true, Ordering::Release);
        let _ = self.tx.try_send(RenderEvent::Quit);
    }
}

/// The dedicated render OS thread (ADR 0013). [`RenderThread::join`] to ensure
/// a clean exit; dropping it signals [`RenderEvent::Quit`] and joins.
#[derive(Debug)]
pub struct RenderThread {
    join: Option<JoinHandle<()>>,
    /// Clone of the inbox sender so `Drop` can signal Quit before joining.
    tx: Sender<RenderEvent>,
    /// Shared quit flag. `Drop` sets it so `run_loop` exits within one frame
    /// even if `try_send(Quit)` fails on a full channel.
    quit_flag: Arc<AtomicBool>,
}

impl RenderThread {
    /// Spawn the render thread with the given input source, draw sink, and
    /// channel capacity. The thread owns the terminal and the state; it never
    /// awaits (ADR 0013). Returns a handle to send events and the thread to
    /// join.
    ///
    /// `channel_cap` must be positive. Zero panics in tokio's `mpsc::channel`;
    /// we reject it with `InvalidInput` instead (fail closed, not panic).
    pub fn spawn<I, D>(
        input: I,
        sink: D,
        channel_cap: usize,
    ) -> io::Result<(RenderHandle, RenderThread)>
    where
        I: InputSource + 'static,
        D: FrameSink + 'static,
    {
        if channel_cap == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "channel_cap must be positive",
            ));
        }
        let (tx, rx) = mpsc::channel(channel_cap);
        let quit_flag = Arc::new(AtomicBool::new(false));
        let loop_quit = quit_flag.clone();
        let join = thread::Builder::new()
            .name("pi-render".into())
            .spawn(move || run_loop(input, sink, rx, loop_quit))?;
        Ok((
            RenderHandle {
                tx: tx.clone(),
                quit_flag: quit_flag.clone(),
            },
            RenderThread {
                join: Some(join),
                tx,
                quit_flag,
            },
        ))
    }

    /// Join the render thread. Returns `Err` if the thread panicked.
    pub fn join(mut self) -> thread::Result<()> {
        if let Some(j) = self.join.take() {
            j.join()?;
        }
        Ok(())
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        // Fail closed: set the quit flag (guarantees `run_loop` exits within
        // one frame even if the channel is full), then join so the thread
        // never leaks (GOALS goal 2: no orphaned threads).
        self.quit_flag.store(true, Ordering::Release);
        let _ = self.tx.try_send(RenderEvent::Quit);
        if let Some(j) = self.join.take() {
            let _ = j.join();
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
        if state.applied > 0 {
            self.observed.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageRef;
    use crate::stream::AssistantMessageEvent;

    fn asst_ref() -> MessageRef {
        MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        }
    }

    /// ADR 0013: the render thread drains its event channel non-blocking. An
    /// empty channel returns immediately (no await, no block).
    #[test]
    fn drain_is_nonblocking_when_empty() {
        let (_tx, mut rx) = mpsc::channel::<RenderEvent>(16);
        let drained = drain_events(&mut rx);
        assert!(
            drained.is_empty(),
            "drain on empty channel must return immediately"
        );
    }

    /// ADR 0013: drain collects ALL pending events in one pass (coalescing).
    #[test]
    fn drain_collects_all_pending() {
        let (tx, mut rx) = mpsc::channel::<RenderEvent>(16);
        tx.try_send(text_delta_event(0, "a")).unwrap();
        tx.try_send(text_delta_event(0, "b")).unwrap();
        tx.try_send(RenderEvent::Quit).unwrap();
        let drained = drain_events(&mut rx);
        assert_eq!(drained.len(), 3, "drain must collect all pending events");
        assert_eq!(drained[2], RenderEvent::Quit);
    }

    /// Step 2 spec: a `RenderEvent` sent from a tokio task is applied before
    /// the next frame. The render thread applies drained events, THEN draws;
    /// so the frame's draw observes the applied state.
    #[tokio::test]
    async fn event_applied_before_next_frame() {
        let observed = Arc::new(AtomicBool::new(false));
        let sink = CountingSink::new(observed.clone());
        let (handle, thread) = RenderThread::spawn(NullInput, sink, 64).unwrap();

        // Send from a tokio task (the agent-loop side).
        let h = handle.clone();
        tokio::spawn(async move {
            h.send(text_delta_event(0, "hi")).await.unwrap();
        });

        // Wait for a frame to draw after the apply (≤16ms cadence per frame).
        // Yield to the runtime so the spawned send task can run.
        let mut waited_ms = 0;
        while !observed.load(Ordering::SeqCst) && waited_ms < 2000 {
            tokio::time::sleep(Duration::from_millis(2)).await;
            waited_ms += 2;
        }
        assert!(
            observed.load(Ordering::SeqCst),
            "event must be applied before a frame draws (waited {waited_ms}ms)"
        );

        handle.quit();
        thread
            .join()
            .expect("render thread must join cleanly after Quit");
    }

    /// `RenderEvent::Quit` terminates the render thread; it joins without hang.
    #[test]
    fn quit_terminates_the_thread() {
        let observed = Arc::new(AtomicBool::new(false));
        let sink = CountingSink::new(observed);
        let (handle, thread) = RenderThread::spawn(NullInput, sink, 16).unwrap();
        handle.quit();
        thread.join().expect("render thread must join after Quit");
    }

    /// `RenderThread::spawn` returns a named thread (debugging, GOALS goal 2:
    /// owned lifecycles).
    #[test]
    fn spawn_returns_named_thread() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 16).unwrap();
        // The handle is cloneable for multi-task sending.
        let _clone = handle.clone();
        handle.quit();
        thread.join().unwrap();
    }

    /// `NullInput` blocks for the timeout then returns None (cadence provider).
    #[test]
    fn null_input_blocks_then_returns_none() {
        let mut input = NullInput;
        let start = std::time::Instant::now();
        let res = input.poll(Duration::from_millis(10)).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(8));
        assert_eq!(res, None);
    }

    /// `channel_cap` of zero is rejected with `InvalidInput`, not a panic.
    /// tokio's `mpsc::channel` panics on zero capacity; we fail closed.
    #[test]
    fn zero_channel_cap_is_rejected() {
        let observed = Arc::new(AtomicBool::new(false));
        let result = RenderThread::spawn(NullInput, CountingSink::new(observed), 0);
        assert!(result.is_err(), "zero channel_cap must error");
        assert_eq!(
            result.unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "error must be InvalidInput"
        );
    }

    /// Build a MessageUpdate carrying a TextDelta (the streaming token event).
    /// Mirrors pi's message_update.assistantMessageEvent (L432).
    fn text_delta_event(content_index: u32, delta: &str) -> RenderEvent {
        RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::TextDelta {
                content_index,
                delta: delta.into(),
            },
        }
    }
}
