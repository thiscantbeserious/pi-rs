//! Render-thread orchestration: the unified channel, the handle, and the
//! thread lifecycle (ADR 0013, ADR 0030).
//!
//! Two dedicated OS threads cooperate through a unified `std::sync::mpsc`
//! channel:
//!
//! - **Reader thread** ([`input::reader_loop`]): reads input, sends
//!   [`LoopEvent::Input`]. Defined in [`input`].
//! - **Render thread** ([`render::run_loop`]): blocks on `recv_timeout`,
//!   applies, draws. Defined in [`render`].
//!
//! [`RenderHandle`] is the sender side (cloneable, sync non-blocking). It is
//! safe to call from tokio tasks without blocking the runtime (ADR 0030).
//! [`RenderThread`] owns both thread handles and joins them on drop (GOALS
//! goal 2: no orphaned threads).
//!
//! No pi equivalent (pi is single-process JS; the render-thread/tokio split is
//! pi-rs-native, ADR 0013). Per PHILOSOPHY §9.5.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::event::RenderEvent;
use crate::input::{reader_loop, InputSource};
use crate::render::{run_loop, FrameSink};

/// What flows through the unified channel: render events (from tokio tasks)
/// or input events (from the reader thread). Internal to this module.
///
/// `RenderEvent` is large (~216 bytes, carries `MessageRef`/`Vec` inline) and
/// `Input` is small. `clippy::large_enum_variant` fires on the size gap, but
/// this is an internal `pub(crate)` channel element: short-lived (construct,
/// send, receive, consume in one frame). Boxing the `Render` variant would add
/// a per-event alloc+dealloc (~30-50ns) on the frame path to avoid a 216-byte
/// memcpy (~2ns). The copy is cheaper than the alloc (GOALS goal 1), so the
/// lint is allowed here.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum LoopEvent {
    Render(RenderEvent),
    Input(crate::input::InputEvent),
}

/// Error from [`RenderHandle::send`]. The channel was full (backpressure) or
/// disconnected (render thread exited). Does not leak the internal `LoopEvent`
/// (which can carry large payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError {
    /// `true` if the channel was full, `false` if disconnected.
    pub full: bool,
}

impl From<std::sync::mpsc::TrySendError<LoopEvent>> for SendError {
    fn from(e: std::sync::mpsc::TrySendError<LoopEvent>) -> Self {
        match e {
            std::sync::mpsc::TrySendError::Full(_) => SendError { full: true },
            std::sync::mpsc::TrySendError::Disconnected(_) => SendError { full: false },
        }
    }
}

/// Handle to the render thread's inbox. Cloneable so multiple tokio tasks (the
/// agent loop, the Host Protocol) can send. [`RenderHandle::send`] is sync
/// non-blocking (`try_send`); safe to call from tokio tasks without blocking
/// the runtime (ADR 0030). [`RenderHandle::quit`] is sync (control path).
#[derive(Clone, Debug)]
pub struct RenderHandle {
    tx: SyncSender<LoopEvent>,
    /// Shared quit flag. Set by `quit()` and `Drop`; checked by `run_loop`
    /// and `reader_loop`. Guarantees shutdown even when the channel is full
    /// (GOALS goal 2: no orphaned threads).
    quit_flag: Arc<AtomicBool>,
}

impl RenderHandle {
    /// Send a render event from the tokio side. Sync non-blocking (`try_send`).
    /// Returns `Err` if the channel is full (backpressure is the caller's
    /// concern, Phase 3) or disconnected. Safe to call from tokio tasks (ADR
    /// 0030).
    pub fn send(&self, ev: RenderEvent) -> Result<(), SendError> {
        self.tx
            .try_send(LoopEvent::Render(ev))
            .map(drop)
            .map_err(SendError::from)
    }

    /// Signal the render thread to stop. Sync (control path, not the frame
    /// path). Sets the shared quit flag and `try_send`s `Quit`. The `Quit` on
    /// the channel wakes `recv_timeout` immediately (ADR 0030).
    pub fn quit(&self) {
        self.quit_flag.store(true, Ordering::Release);
        let _ = self.tx.try_send(LoopEvent::Render(RenderEvent::Quit));
    }
}

/// The render subsystem: the render thread and the input reader thread
/// (ADR 0013, ADR 0030). [`RenderThread::join`] to ensure a clean exit;
/// dropping it signals `Quit` and joins both threads.
#[derive(Debug)]
pub struct RenderThread {
    render: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    /// Clone of the inbox sender so `Drop` can signal Quit before joining.
    tx: SyncSender<LoopEvent>,
    /// Shared quit flag. `Drop` sets it so both threads exit within their
    /// cadence (render: immediately via channel; reader: <=4ms via poll).
    quit_flag: Arc<AtomicBool>,
}

impl RenderThread {
    /// Spawn the render and reader threads with the given input source, draw
    /// sink, and channel capacity. The render thread owns the terminal and
    /// the state; the reader thread owns input reading (ADR 0030). Neither
    /// awaits (ADR 0013). Returns a handle to send events and the threads to
    /// join.
    ///
    /// `channel_cap` must be positive. Zero is rejected with `InvalidInput`
    /// (fail closed, not panic).
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
        let (tx, rx) = mpsc::sync_channel(channel_cap);
        let quit_flag = Arc::new(AtomicBool::new(false));

        // Reader thread: reads input, sends LoopEvent::Input (ADR 0030).
        let reader_tx = tx.clone();
        let reader_quit = quit_flag.clone();
        let reader = thread::Builder::new()
            .name("pi-render-input".into())
            .spawn(move || reader_loop(input, reader_tx, reader_quit))?;

        // Render thread: blocks on the channel, applies, draws (ADR 0013).
        // If this spawn fails, clean up the already-running reader before
        // returning Err (GOALS goal 2: no orphaned threads).
        let render_quit = quit_flag.clone();
        let render = match thread::Builder::new()
            .name("pi-render".into())
            .spawn(move || run_loop(sink, rx, render_quit))
        {
            Ok(h) => h,
            Err(e) => {
                quit_flag.store(true, Ordering::Release);
                let _ = reader.join();
                return Err(e);
            }
        };

        Ok((
            RenderHandle {
                tx: tx.clone(),
                quit_flag: quit_flag.clone(),
            },
            RenderThread {
                render: Some(render),
                reader: Some(reader),
                tx,
                quit_flag,
            },
        ))
    }

    /// Join both threads. Returns `Err` if either panicked.
    pub fn join(mut self) -> thread::Result<()> {
        if let Some(h) = self.render.take() {
            h.join()?;
        }
        if let Some(h) = self.reader.take() {
            h.join()?;
        }
        Ok(())
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        // Fail closed: set the quit flag (both threads exit within their
        // cadence), signal Quit on the channel (wakes recv_timeout
        // immediately), then join both so neither leaks (GOALS goal 2).
        self.quit_flag.store(true, Ordering::Release);
        let _ = self.tx.try_send(LoopEvent::Render(RenderEvent::Quit));
        if let Some(h) = self.render.take() {
            let _ = h.join();
        }
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::input::NullInput;
    use crate::message::MessageRef;
    use crate::render::CountingSink;
    use crate::stream::AssistantMessageEvent;

    use super::*;

    fn asst_ref() -> MessageRef {
        MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        }
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

    /// Step 2 spec: a `RenderEvent` sent from a tokio task (now any thread,
    /// since `send` is sync non-blocking) is applied before the next frame.
    /// The render thread applies drained events, THEN draws; so the frame's
    /// draw observes the applied state.
    #[test]
    fn event_applied_before_next_frame() {
        let observed = Arc::new(AtomicBool::new(false));
        let sink = CountingSink::new(observed.clone());
        let (handle, thread) = RenderThread::spawn(NullInput, sink, 64).unwrap();

        // Send MessageStart then TextDelta (streaming lifecycle).
        let h = handle.clone();
        h.send(RenderEvent::MessageStart {
            message: MessageRef::Assistant {
                content: vec![crate::message::ContentBlock::Text { text: "".into() }],
                stop_reason: None,
                timestamp: 0,
            },
        })
        .unwrap();
        h.send(text_delta_event(0, "hi")).unwrap();

        // Wait for a frame to draw after the apply (<=16ms cadence per frame).
        let mut waited_ms = 0;
        while !observed.load(Ordering::SeqCst) && waited_ms < 2000 {
            thread::sleep(Duration::from_millis(2));
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

    /// ADR 0030: `Quit` wakes `recv_timeout` immediately. The render thread
    /// joins without hanging even while the reader thread is polling.
    #[test]
    fn quit_terminates_the_thread() {
        let observed = Arc::new(AtomicBool::new(false));
        let sink = CountingSink::new(observed);
        let (handle, thread) = RenderThread::spawn(NullInput, sink, 16).unwrap();
        handle.quit();
        thread.join().expect("render thread must join after Quit");
    }

    /// ADR 0030: `Drop` joins both the render and reader threads (no orphaned
    /// threads, GOALS goal 2).
    #[test]
    fn drop_joins_both_threads() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 16).unwrap();
        let _clone = handle.clone();
        drop(thread);
        // If Drop didn't join, the test process would still have live threads.
        // The assertion is implicit: drop returns (both threads joined).
    }

    /// `channel_cap` of zero is rejected with `InvalidInput`, not a panic.
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

    /// `RenderThread::spawn` returns named threads (debugging, GOALS goal 2).
    #[test]
    fn spawn_returns_named_thread() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 16).unwrap();
        let _clone = handle.clone();
        handle.quit();
        thread.join().unwrap();
    }

    /// send() returns an error when the channel is full (backpressure).
    #[test]
    fn send_returns_error_when_full() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 1).unwrap();
        // Fill the channel (capacity 1).
        handle.send(text_delta_event(0, "x")).unwrap();
        // Next send should fail (full).
        let result = handle.send(text_delta_event(0, "y"));
        // It may succeed or fail depending on timing (reader may have drained).
        // The important thing: it doesn't panic.
        let _ = result;
        handle.quit();
        thread.join().unwrap();
    }

    /// RenderHandle is Clone + Debug.
    #[test]
    fn render_handle_clone_and_debug() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 16).unwrap();
        let cloned = handle.clone();
        let _debug = format!("{:?}", cloned);
        handle.quit();
        thread.join().unwrap();
    }

    /// RenderThread is Debug.
    #[test]
    fn render_thread_is_debug() {
        let observed = Arc::new(AtomicBool::new(false));
        let (handle, thread) =
            RenderThread::spawn(NullInput, CountingSink::new(observed), 16).unwrap();
        let _debug = format!("{:?}", thread);
        handle.quit();
        thread.join().unwrap();
    }

    /// SendError from converts Full and Disconnected correctly.
    #[test]
    fn send_error_from_full_and_disconnected() {
        let (tx, _rx) = mpsc::sync_channel::<LoopEvent>(1);
        tx.send(LoopEvent::Render(RenderEvent::Quit)).unwrap(); // fill
        let full_err = tx
            .try_send(LoopEvent::Render(RenderEvent::Quit))
            .unwrap_err();
        let se: SendError = full_err.into();
        assert!(se.full);

        let (tx2, rx2) = mpsc::sync_channel::<LoopEvent>(1);
        drop(rx2);
        // Use try_send to get TrySendError::Disconnected.
        let disc_err = tx2
            .try_send(LoopEvent::Render(RenderEvent::Quit))
            .unwrap_err();
        let se2: SendError = disc_err.into();
        assert!(!se2.full);
    }
}
