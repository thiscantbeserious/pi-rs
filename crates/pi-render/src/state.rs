//! The Retained Message Model (ADR 0004, ADR 0031).
//!
//! The render thread owns the RMM and is its sole mutator (ADR 0013). The RMM
//! holds the message list (`Vec<MessageRef>`), the scroll offset, the line
//! cache (message-granular, invalidated on width/theme change), and synthetic
//! frame buffers (ADR 0003, Phase 2).
//!
//! Visible-window rendering (ADR 0031): the projection renders only
//! `scroll_offset..scroll_offset + viewport_height` into ratatui's
//! terminal-sized `Buffer`. O(viewport_height) per frame, not O(total_lines).
//!
//! No pi equivalent (pi is single-process JS; the RMM is pi-rs-native,
//! ADR 0004). Per PHILOSOPHY §9.5.

use crate::event::RenderEvent;
use crate::message::{ContentBlock, MessageRef};
use crate::projection::{render_message, FrameBuffer, RenderedLine};
use crate::stream::AssistantMessageEvent;

/// The width the line cache was rendered at. If the terminal width changes,
/// the cache is invalid and must be re-rendered.
const INITIAL_WIDTH: u16 = 80;

/// Render-thread-owned state. The Retained Message Model (ADR 0004, ADR 0031).
/// Mutated only on the render thread (ADR 0013).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderState {
    /// Control: set by [`RenderEvent::Quit`]; the loop exits when true.
    pub quit: bool,

    /// The message list. Grows as messages arrive.
    messages: Vec<MessageRef>,

    /// The streaming tail message index (if any). When a `MessageUpdate`
    /// arrives, deltas are appended here. `None` between turns.
    streaming_index: Option<usize>,

    /// Scroll offset: the first visible line index. Phase 2 pins to tail.
    scroll_offset: usize,

    /// Cached rendered lines for all messages, at `cached_width`.
    cached_lines: Vec<RenderedLine>,

    /// The width the cache was rendered at. If it changes, invalidate.
    cached_width: u16,

    /// Synthetic frame buffers (ADR 0003). Phase 2 only.
    frame_buffers: Vec<FrameBuffer>,

    /// Dirty flag: set when messages change, cleared after rendering.
    dirty: bool,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            quit: false,
            messages: Vec::new(),
            streaming_index: None,
            scroll_offset: 0,
            cached_lines: Vec::new(),
            cached_width: INITIAL_WIDTH,
            frame_buffers: Vec::new(),
            dirty: true,
        }
    }
}

impl RenderState {
    /// Apply a batch of drained events single-threaded (ADR 0013). Returns
    /// whether the state changed (dirty) so the loop can skip a redraw when
    /// idle (ADR 0010 coalescing budget).
    ///
    /// `Quit` is a control signal: sets the quit flag, does not mark dirty.
    pub fn apply(&mut self, events: &[RenderEvent]) -> bool {
        let mut dirty = false;
        for ev in events {
            match ev {
                // Streaming deltas append to the current message.
                RenderEvent::MessageUpdate {
                    event:
                        AssistantMessageEvent::TextDelta { delta, .. }
                        | AssistantMessageEvent::ThinkingDelta { delta, .. }
                        | AssistantMessageEvent::ToolCallDelta { delta, .. },
                    ..
                } => {
                    self.append_delta(delta);
                    dirty = true;
                }
                // Other MessageUpdate events (start/end, done, error) mark dirty.
                RenderEvent::MessageUpdate { .. } => {
                    dirty = true;
                }
                RenderEvent::MessageStart { message } => {
                    self.push_message(message.clone());
                    dirty = true;
                }
                RenderEvent::MessageEnd { .. } => {
                    self.streaming_index = None;
                    dirty = true;
                }
                RenderEvent::FrameBufferUpdated => {
                    dirty = true;
                }
                RenderEvent::Resize { cols, rows: _ } => {
                    if *cols != self.cached_width {
                        self.cached_width = *cols;
                        self.cached_lines.clear();
                        self.dirty = true;
                    }
                    dirty = true;
                }
                RenderEvent::Quit => {
                    self.quit = true;
                }
                _ => {
                    dirty = true;
                }
            }
        }
        if dirty {
            self.dirty = true;
        }
        dirty
    }

    /// Append a streaming delta to the current message.
    fn append_delta(&mut self, delta: &str) {
        if let Some(idx) = self.streaming_index {
            if let Some(MessageRef::Assistant { content, .. }) = self.messages.get_mut(idx) {
                if let Some(ContentBlock::Text { text }) = content.last_mut() {
                    text.push_str(delta);
                }
            }
        }
    }

    /// Push a new message onto the list.
    pub fn push_message(&mut self, msg: MessageRef) {
        self.messages.push(msg);
        self.streaming_index = Some(self.messages.len() - 1);
        self.dirty = true;
    }

    /// Add a synthetic frame buffer (ADR 0003).
    pub fn add_frame_buffer(&mut self, fb: FrameBuffer) {
        self.frame_buffers.push(fb);
        self.dirty = true;
    }

    /// Recompute the scroll offset for pin-to-tail (Phase 2, ADR 0031).
    pub fn recompute_scroll(&mut self, viewport_height: usize) {
        let total = self.cached_lines.len();
        self.scroll_offset = total.saturating_sub(viewport_height);
    }

    /// Render all messages at `width`, updating the line cache.
    pub fn render_at_width(&mut self, width: u16) {
        if self.cached_width == width && !self.dirty {
            return;
        }
        self.cached_width = width;
        self.cached_lines.clear();
        for msg in &self.messages {
            let mut lines = render_message(msg, width);
            self.cached_lines.append(&mut lines);
        }
        self.dirty = false;
    }

    // --- Accessors for the projection ---

    pub fn messages(&self) -> &[MessageRef] {
        &self.messages
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn rendered_lines(&self) -> &[RenderedLine] {
        &self.cached_lines
    }

    pub fn frame_buffers(&self) -> &[FrameBuffer] {
        &self.frame_buffers
    }
}

#[cfg(test)]
mod tests {
    use crate::message::MessageRef;

    use super::*;

    fn asst_ref() -> MessageRef {
        MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        }
    }

    /// A streaming text delta (nested in MessageUpdate) appends and marks dirty.
    #[test]
    fn apply_text_delta_appends_and_marks_dirty() {
        let mut s = RenderState::default();
        s.push_message(MessageRef::Assistant {
            content: vec![crate::message::ContentBlock::Text { text: "".into() }],
            stop_reason: None,
            timestamp: 0,
        });
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "hi".into(),
            },
        }]);
        assert!(dirty, "appending a delta must mark the state dirty");
    }

    /// Quit sets the quit flag but does not mark dirty.
    #[test]
    fn apply_quit_sets_flag_without_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::Quit]);
        assert!(s.quit, "Quit must set the quit flag");
        assert!(!dirty, "Quit must not mark the state dirty");
    }

    /// An empty event batch changes nothing and is not dirty.
    #[test]
    fn apply_empty_is_noop() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[]);
        assert!(!dirty);
    }
}
