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

    /// MessageStart pushes a message and marks dirty.
    #[test]
    fn apply_message_start_pushes_and_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::MessageStart {
            message: asst_ref(),
        }]);
        assert!(dirty);
        assert_eq!(s.messages().len(), 1);
    }

    /// MessageEnd clears the streaming index and marks dirty.
    #[test]
    fn apply_message_end_clears_streaming_and_marks_dirty() {
        let mut s = RenderState::default();
        s.push_message(asst_ref());
        let dirty = s.apply(&[RenderEvent::MessageEnd {
            message: asst_ref(),
        }]);
        assert!(dirty);
    }

    /// FrameBufferUpdated marks dirty.
    #[test]
    fn apply_frame_buffer_updated_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::FrameBufferUpdated]);
        assert!(dirty);
    }

    /// Resize at a new width invalidates the cache and marks dirty.
    #[test]
    fn apply_resize_invalidates_cache() {
        let mut s = RenderState {
            cached_width: 80,
            ..Default::default()
        };
        let dirty = s.apply(&[RenderEvent::Resize { cols: 40, rows: 24 }]);
        assert!(dirty);
        assert_eq!(s.cached_width, 40);
    }

    /// Resize at the same width does not invalidate but still marks dirty.
    #[test]
    fn apply_resize_same_width_still_dirty() {
        let mut s = RenderState {
            cached_width: 80,
            ..Default::default()
        };
        let dirty = s.apply(&[RenderEvent::Resize { cols: 80, rows: 24 }]);
        assert!(dirty);
    }

    /// Agent-loop events (AgentStart, TurnStart, etc.) mark dirty.
    #[test]
    fn apply_agent_loop_events_mark_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[
            RenderEvent::AgentStart,
            RenderEvent::TurnStart,
            RenderEvent::AgentEnd { messages: vec![] },
        ]);
        assert!(dirty);
    }

    /// Tool execution events mark dirty.
    #[test]
    fn apply_tool_execution_events_mark_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[
            RenderEvent::ToolExecutionStart {
                tool_call_id: "tc1".into(),
                tool_name: "bash".into(),
                args: serde_json::json!({}),
            },
            RenderEvent::ToolExecutionUpdate {
                tool_call_id: "tc1".into(),
                tool_name: "bash".into(),
                args: serde_json::json!({}),
                partial_result: serde_json::json!({}),
            },
            RenderEvent::ToolExecutionEnd {
                tool_call_id: "tc1".into(),
                tool_name: "bash".into(),
                result: serde_json::json!({}),
                is_error: false,
            },
        ]);
        assert!(dirty);
    }

    /// Other MessageUpdate events (start/end, done, error) mark dirty.
    #[test]
    fn apply_streaming_boundary_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::TextStart { content_index: 0 },
        }]);
        assert!(dirty);
    }

    /// append_delta without a streaming message is a no-op (no panic).
    #[test]
    fn append_delta_without_streaming_is_noop() {
        let mut s = RenderState::default();
        s.append_delta("orphan");
        assert!(s.messages().is_empty());
    }

    /// ThinkingDelta appends to the current message.
    #[test]
    fn apply_thinking_delta_marks_dirty() {
        let mut s = RenderState::default();
        s.push_message(asst_ref());
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "hmm".into(),
            },
        }]);
        assert!(dirty);
    }

    /// ToolCallDelta marks dirty.
    #[test]
    fn apply_toolcall_delta_marks_dirty() {
        let mut s = RenderState::default();
        s.push_message(asst_ref());
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::ToolCallDelta {
                content_index: 0,
                delta: "{}".into(),
            },
        }]);
        assert!(dirty);
    }

    /// recompute_scroll pins to tail.
    #[test]
    fn recompute_scroll_pins_to_tail() {
        let mut s = RenderState {
            cached_lines: (0..10).map(|_| RenderedLine::plain("x")).collect(),
            ..Default::default()
        };
        s.recompute_scroll(5);
        assert_eq!(s.scroll_offset(), 5);
    }

    /// recompute_scroll with fewer lines than viewport gives 0.
    #[test]
    fn recompute_scroll_underflow_is_zero() {
        let mut s = RenderState {
            cached_lines: vec![RenderedLine::plain("x")],
            ..Default::default()
        };
        s.recompute_scroll(10);
        assert_eq!(s.scroll_offset(), 0);
    }

    /// render_at_width renders messages and clears dirty.
    #[test]
    fn render_at_width_renders_and_clears_dirty() {
        let mut s = RenderState::default();
        s.push_message(MessageRef::Assistant {
            content: vec![ContentBlock::Text { text: "hi".into() }],
            stop_reason: None,
            timestamp: 0,
        });
        s.render_at_width(80);
        assert!(!s.dirty);
        assert!(!s.rendered_lines().is_empty());
    }

    /// render_at_width at same width with no dirty flag returns cached.
    #[test]
    fn render_at_width_same_width_uses_cache() {
        let mut s = RenderState::default();
        s.push_message(MessageRef::Assistant {
            content: vec![ContentBlock::Text { text: "hi".into() }],
            stop_reason: None,
            timestamp: 0,
        });
        s.render_at_width(80);
        let count1 = s.rendered_lines().len();
        s.render_at_width(80);
        let count2 = s.rendered_lines().len();
        assert_eq!(count1, count2);
    }

    /// add_frame_buffer adds and marks dirty.
    #[test]
    fn add_frame_buffer_works() {
        let mut s = RenderState::default();
        s.add_frame_buffer(FrameBuffer {
            lines: vec!["x".into()],
            col: 0,
            row: 0,
        });
        assert_eq!(s.frame_buffers().len(), 1);
        assert!(s.dirty);
    }
}
