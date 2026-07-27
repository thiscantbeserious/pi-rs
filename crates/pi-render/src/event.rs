//! Render-thread channel contract (ADR 0013).
//!
//! Events flow one way: tokio tasks (the agent loop, the Host Protocol,
//! providers, tool subprocesses) send [`RenderEvent`]s over an mpsc channel
//! into the render thread, which owns the Retained Message Model and applies
//! them single-threaded (no locks, no torn reads by construction). The tokio
//! side keeps its own agent state and never queries display state (CQRS-like
//! split, ADR 0013).
//!
//! Step 2 defines the contract. Step 3 wires these events into the real
//! Retained Message Model (ADR 0004).
//!
//! No pi equivalent (pi is single-process JS; the render-thread/tokio split is
//! pi-rs-native, ADR 0013). Per PHILOSOPHY §9.5.

/// A state change the tokio side signals to the render thread. The render
/// thread drains these non-blocking at frame start (ADR 0013) and applies
/// them to the Retained Message Model (Step 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderEvent {
    /// A streaming token was appended to the current message.
    TokenAppended(String),
    /// A tool call finished (its result is available).
    ToolFinished,
    /// An extension frame buffer was updated (ADR 0003). In Phase 2 the source
    /// is synthetic; the Host Protocol wires real buffers in Phase 3.
    FrameBufferUpdated,
    /// The active theme changed (Step 6). Flushes the block cache (ADR 0010).
    ThemeChanged,
    /// The terminal was resized. The viewport re-wraps as a pure function of
    /// the Retained Message Model (ADR 0004, pitfall P5).
    Resize { cols: u16, rows: u16 },
    /// Stop the render thread. Drained at frame start; the loop exits cleanly.
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract variants the plan names exist and carry the intended
    /// payload shape (Step 2 deliverable: the channel contract).
    #[test]
    fn render_event_variants_match_plan() {
        let _ = RenderEvent::TokenAppended("tok".into());
        let _ = RenderEvent::ToolFinished;
        let _ = RenderEvent::FrameBufferUpdated;
        let _ = RenderEvent::ThemeChanged;
        let _ = RenderEvent::Resize { cols: 80, rows: 24 };
        let _ = RenderEvent::Quit;
    }

    /// Events are `Clone + Eq + Debug` so tests and the agent loop can compare,
    /// clone, and inspect them.
    #[test]
    fn render_event_is_clone_eq_debug() {
        let a = RenderEvent::TokenAppended("x".into());
        let b = a.clone();
        assert_eq!(a, b);
        assert!(format!("{a:?}").contains("TokenAppended"));
    }
}
