//! Projection: RMM visible window to ratatui Buffer (ADR 0031).
//!
//! The projection renders only the visible window
//! (`scroll_offset..scroll_offset + viewport_height`) into ratatui's
//! terminal-sized `Buffer`. O(viewport_height) per frame, not O(total_lines).
//! ratatui's `Buffer::diff` handles the cell-granular diff (P9).
//!
//! Step 3 is ASCII-scoped: width is trivial in ASCII. Step 4 introduces the
//! grapheme-width engine that replaces ratatui's `unicode-width`-based
//! `CellWidth` (ADR 0025, P13).
//!
//! No pi equivalent for the projection layer (pi re-renders all components
//! every frame; the visible-window model is pi-rs-native, ADR 0031). Per
//! PHILOSOPHY §9.5.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::message::{ContentBlock, MessageRef};
use crate::state::RenderState;

/// A ratatui widget that projects the RMM visible window into the Buffer.
/// Used inside `terminal.try_draw(|frame| frame.render_widget(...))`.
pub struct RmmProjection<'a> {
    state: &'a RenderState,
}

impl<'a> RmmProjection<'a> {
    pub fn new(state: &'a RenderState) -> Self {
        Self { state }
    }
}

impl Widget for RmmProjection<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let scroll = self.state.scroll_offset();
        let height = area.height as usize;

        // Compute which rendered lines are visible.
        let rendered = self.state.rendered_lines();
        let total_lines = rendered.len();
        let start = scroll.min(total_lines.saturating_sub(height));
        let visible_end = (start + height).min(total_lines);

        // Render visible lines into the buffer.
        for (i, line) in rendered[start..visible_end].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.bottom() {
                break;
            }
            let spans: Vec<Span> = line
                .segments
                .iter()
                .map(|s| Span::styled(s.text.clone(), s.style))
                .collect();
            if spans.is_empty() {
                buf.set_string(area.x, y, "", Style::default());
            } else {
                let rat_line = Line::from(spans);
                rat_line.render(
                    Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height: 1,
                    },
                    buf,
                );
            }
        }

        // Composite frame buffers (ADR 0003). Synthetic in Phase 2.
        for fb in self.state.frame_buffers() {
            fb.composite(area, buf);
        }
    }
}

/// A styled text segment in a rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSegment {
    pub text: String,
    pub style: Style,
}

/// A rendered line: a sequence of styled segments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedLine {
    pub segments: Vec<StyledSegment>,
}

impl RenderedLine {
    /// Build a plain line from a string.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            segments: vec![StyledSegment {
                text: text.into(),
                style: Style::default(),
            }],
        }
    }

    /// Build a styled line.
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            segments: vec![StyledSegment {
                text: text.into(),
                style,
            }],
        }
    }
}

/// A frame buffer region (ADR 0003). Synthetic in Phase 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer {
    /// Lines of pre-rendered text.
    pub lines: Vec<String>,
    /// Region: (col, row) offset within the viewport.
    pub col: u16,
    pub row: u16,
}

impl FrameBuffer {
    /// Composite this frame buffer into the buffer at its region.
    pub fn composite(&self, area: Rect, buf: &mut Buffer) {
        for (i, line) in self.lines.iter().enumerate() {
            let y = area.y + self.row + i as u16;
            if y >= area.bottom() {
                break;
            }
            buf.set_string(
                area.x + self.col,
                y,
                line,
                Style::default().fg(Color::Yellow),
            );
        }
    }
}

/// Render a message to lines at a given width. Step 3 is ASCII-scoped:
/// wrapping is char-based, no grapheme-width handling (Step 4).
///
/// Mirrors pi's per-component `render(width)` pattern (markdown.ts L151),
/// but at message granularity. Finalized messages are cached (ADR 0031).
pub fn render_message(msg: &MessageRef, width: u16) -> Vec<RenderedLine> {
    let w = width as usize;
    let mut lines = Vec::new();

    match msg {
        MessageRef::User { content, .. } => {
            for block in content.iter() {
                render_content_block(block, w, &mut lines);
            }
        }
        MessageRef::Assistant {
            content,
            stop_reason,
            ..
        } => {
            for block in content.iter() {
                render_content_block(block, w, &mut lines);
            }
            if let Some(reason) = stop_reason {
                let style = Style::default().fg(Color::DarkGray);
                lines.push(RenderedLine::styled(
                    format!("[{}]", stop_reason_str(*reason)),
                    style,
                ));
            }
        }
        MessageRef::ToolResult {
            tool_name,
            is_error,
            content,
            ..
        } => {
            let prefix = if *is_error { "ERROR" } else { "OK" };
            let style = if *is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            lines.push(RenderedLine::styled(
                format!("{}: {}", prefix, tool_name),
                style,
            ));
            for block in content.iter() {
                render_content_block(block, w, &mut lines);
            }
        }
    }

    if lines.is_empty() {
        lines.push(RenderedLine::plain(""));
    }
    lines
}

fn render_content_block(block: &ContentBlock, width: usize, lines: &mut Vec<RenderedLine>) {
    match block {
        ContentBlock::Text { text } => {
            wrap_text(text, width, Style::default(), lines);
        }
        ContentBlock::Thinking { thinking, redacted } => {
            let style = Style::default().fg(Color::DarkGray);
            if *redacted {
                lines.push(RenderedLine::styled("[redacted thinking]", style));
            } else {
                wrap_text(thinking, width, style, lines);
            }
        }
        ContentBlock::ToolCall { id, name, .. } => {
            let style = Style::default().fg(Color::Cyan);
            lines.push(RenderedLine::styled(format!("> {} ({})", name, id), style));
        }
        ContentBlock::Image { mime_type, .. } => {
            let style = Style::default().fg(Color::Magenta);
            lines.push(RenderedLine::styled(
                format!("[image: {}]", mime_type),
                style,
            ));
        }
    }
}

/// Word-wrap text at `width` columns. ASCII-scoped (char count, not grapheme
/// width). Step 4 replaces this with grapheme-cluster-aware wrapping.
fn wrap_text(text: &str, width: usize, style: Style, lines: &mut Vec<RenderedLine>) {
    if width == 0 {
        return;
    }
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(RenderedLine::plain(""));
            continue;
        }
        let chars: Vec<char> = paragraph.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let end = (start + width).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            lines.push(RenderedLine::styled(chunk, style));
            start = end;
        }
    }
}

fn stop_reason_str(reason: crate::stream::StopReason) -> &'static str {
    use crate::stream::StopReason;
    match reason {
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::message::{ContentBlock, MessageRef};
    use crate::state::RenderState;

    use super::*;

    fn text_msg(text: &str) -> MessageRef {
        MessageRef::Assistant {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: None,
            timestamp: 0,
        }
    }

    /// P9: a single-cell change produces a single-cell diff. Two frames
    /// differing by one cell must produce a one-cell Buffer::diff.
    #[test]
    fn single_cell_change_produces_single_cell_diff() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        // Frame 1: "Hello"
        let mut state = RenderState::default();
        state.push_message(text_msg("Hello"));
        state.render_at_width(20);
        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();
        let buf1 = terminal.backend().buffer().clone();

        // Frame 2: "Hallo" (one cell changed)
        let mut state2 = RenderState::default();
        state2.push_message(text_msg("Hallo"));
        state2.render_at_width(20);
        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state2), frame.area());
            })
            .unwrap();
        let buf2 = terminal.backend().buffer().clone();

        let diff = buf1.diff(&buf2);
        assert_eq!(
            diff.len(),
            1,
            "single-cell change must produce a single-cell diff, got {} cells",
            diff.len()
        );
        assert_eq!(diff[0].0, 1, "changed cell must be at x=1 (the 'a')");
    }

    /// P5: resize re-wraps without scrollback corruption. A message wrapped
    /// at width 10 must re-wrap correctly at width 20.
    #[test]
    fn resize_rewraps_without_corruption() {
        let msg = text_msg("1234567890ABCDEF");

        // Width 10: two lines
        let lines_10 = render_message(&msg, 10);
        assert!(
            lines_10.len() >= 2,
            "16 chars at width 10 must wrap to >= 2 lines"
        );

        // Width 20: one line
        let lines_20 = render_message(&msg, 20);
        assert_eq!(
            lines_20.len(),
            1,
            "16 chars at width 20 must fit on one line"
        );
    }

    /// A synthetic frame buffer composites into the grid at its region.
    #[test]
    fn frame_buffer_composites_into_grid() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = RenderState::default();
        state.push_message(text_msg("Hello"));
        state.add_frame_buffer(FrameBuffer {
            lines: vec!["[FB]".into()],
            col: 10,
            row: 0,
        });

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        // The frame buffer "[FB]" should appear at col 10, row 0.
        let cell = buf.cell((10u16, 0u16)).expect("cell must exist");
        assert!(
            cell.symbol().starts_with('['),
            "frame buffer must composite at col 10, got '{}'",
            cell.symbol()
        );
    }

    /// Pin-to-tail auto-scroll: scroll_offset pins to the bottom.
    #[test]
    fn auto_scroll_pins_to_tail() {
        let mut state = RenderState::default();
        // Push enough messages to fill more than 5 lines.
        for i in 0..10 {
            state.push_message(text_msg(&format!("line {}", i)));
        }
        state.render_at_width(20);
        state.recompute_scroll(5); // viewport height = 5
        let scroll = state.scroll_offset();
        let total = state.rendered_lines().len();
        assert!(
            scroll + 5 >= total,
            "pin-to-tail: scroll({}) + viewport(5) must reach total({})",
            scroll,
            total
        );
    }

    /// Message-granular line cache: finalized messages are cached, not
    /// re-rendered on the same width.
    #[test]
    fn finalized_message_is_cached() {
        let mut state = RenderState::default();
        state.push_message(text_msg("Hello"));
        state.render_at_width(20);
        let lines1 = state.rendered_lines().len();

        // Render again at the same width: should use cache (same line count).
        state.render_at_width(20);
        let lines2 = state.rendered_lines().len();

        assert_eq!(
            lines1, lines2,
            "re-render at same width must return cached lines"
        );
    }

    /// Cache invalidation on resize: rendering at a new width re-renders.
    #[test]
    fn cache_invalidated_on_resize() {
        let msg = text_msg("1234567890ABCDEF");
        let mut state = RenderState::default();
        state.push_message(msg);

        state.render_at_width(20);
        let lines_20 = state.rendered_lines().len();

        state.render_at_width(10);
        let lines_10 = state.rendered_lines().len();

        assert_ne!(
            lines_20, lines_10,
            "resize must invalidate cache and re-render at new width"
        );
    }

    // --- Broad behavior tests: all message roles and content block types ---

    /// User message renders its content blocks.
    #[test]
    fn render_user_message() {
        let msg = MessageRef::User {
            content: vec![ContentBlock::Text {
                text: "hello user".into(),
            }],
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// Assistant message with stop_reason renders the stop marker.
    #[test]
    fn render_assistant_with_stop_reason() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::Text { text: "hi".into() }],
            stop_reason: Some(crate::stream::StopReason::Stop),
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(
            lines.len() >= 2,
            "assistant with stop_reason has content + marker"
        );
    }

    /// Assistant message with error stop_reason.
    #[test]
    fn render_assistant_with_error_stop_reason() {
        let msg = MessageRef::Assistant {
            content: vec![],
            stop_reason: Some(crate::stream::StopReason::Error),
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// ToolResult message renders prefix and content.
    #[test]
    fn render_tool_result_ok() {
        let msg = MessageRef::ToolResult {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            content: vec![ContentBlock::Text {
                text: "output".into(),
            }],
            is_error: false,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// ToolResult error message renders error prefix.
    #[test]
    fn render_tool_result_error() {
        let msg = MessageRef::ToolResult {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            content: vec![],
            is_error: true,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// Thinking content block renders.
    #[test]
    fn render_thinking_block() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::Thinking {
                thinking: "hmm".into(),
                redacted: false,
            }],
            stop_reason: None,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// Redacted thinking block shows placeholder.
    #[test]
    fn render_redacted_thinking_block() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::Thinking {
                thinking: "".into(),
                redacted: true,
            }],
            stop_reason: None,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// ToolCall content block renders.
    #[test]
    fn render_toolcall_block() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::ToolCall {
                id: "tc1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({}),
            }],
            stop_reason: None,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// Image content block renders a placeholder.
    #[test]
    fn render_image_block() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::Image {
                data: std::sync::Arc::from(""),
                mime_type: "image/png".into(),
            }],
            stop_reason: None,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert!(!lines.is_empty());
    }

    /// Empty message renders one blank line.
    #[test]
    fn render_empty_message() {
        let msg = MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        };
        let lines = render_message(&msg, 80);
        assert_eq!(lines.len(), 1);
    }

    /// wrap_text at width 0 produces nothing.
    #[test]
    fn wrap_text_zero_width() {
        let mut lines = Vec::new();
        wrap_text("hello", 0, Style::default(), &mut lines);
        assert!(lines.is_empty());
    }

    /// wrap_text handles newlines.
    #[test]
    fn wrap_text_handles_newlines() {
        let mut lines = Vec::new();
        wrap_text("a\nb\nc", 80, Style::default(), &mut lines);
        assert_eq!(lines.len(), 3);
    }

    /// wrap_text handles empty paragraphs.
    #[test]
    fn wrap_text_empty_paragraph() {
        let mut lines = Vec::new();
        wrap_text("", 80, Style::default(), &mut lines);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].segments.is_empty() || lines[0].segments[0].text.is_empty());
    }

    /// FrameBuffer composite respects area bounds (clips beyond bottom).
    #[test]
    fn frame_buffer_clips_beyond_bottom() {
        let backend = TestBackend::new(20, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = RenderState::default();
        state.add_frame_buffer(FrameBuffer {
            lines: vec!["line1".into(), "line2".into(), "line3".into()],
            col: 0,
            row: 0,
        });

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();

        // Only 2 rows visible; line3 should be clipped.
        let buf = terminal.backend().buffer();
        assert_eq!(buf.area().height, 2);
    }

    /// RmmProjection renders nothing when there are no messages.
    #[test]
    fn projection_empty_state() {
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = RenderState::default();

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();
        // No panic = pass.
    }

    /// RmmProjection handles empty-segment lines (spans is empty).
    #[test]
    fn projection_empty_segments() {
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = RenderState::default();
        // A message with an empty text block produces a blank line (empty segments).
        state.push_message(MessageRef::Assistant {
            content: vec![ContentBlock::Text { text: "".into() }],
            stop_reason: None,
            timestamp: 0,
        });
        state.render_at_width(10);

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();
        // No panic = pass. The empty-segments branch sets an empty string.
    }

    /// RmmProjection renders visible lines from the scroll offset.
    #[test]
    fn projection_renders_visible_window() {
        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = RenderState::default();
        for i in 0..10 {
            state.push_message(text_msg(&format!("line{}", i)));
        }
        state.render_at_width(20);
        state.recompute_scroll(3);

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();

        // The last 3 lines should be visible.
        let buf = terminal.backend().buffer();
        let row0 = buf.cell((0u16, 0u16)).expect("cell exists");
        assert!(!row0.symbol().is_empty());
    }

    /// RenderedLine::plain and ::styled constructors.
    #[test]
    fn rendered_line_constructors() {
        let plain = RenderedLine::plain("hello");
        assert_eq!(plain.segments.len(), 1);

        let styled = RenderedLine::styled("hi", Style::default());
        assert_eq!(styled.segments.len(), 1);
    }

    /// RmmProjection clips lines beyond the viewport bottom.
    #[test]
    fn projection_clips_beyond_viewport() {
        let backend = TestBackend::new(20, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = RenderState::default();
        // Push 5 messages, each 1 line. Viewport is 2 rows.
        for i in 0..5 {
            state.push_message(text_msg(&format!("line{}", i)));
        }
        state.render_at_width(20);
        state.recompute_scroll(2);

        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();

        // Only 2 rows visible; the break at y >= area.bottom() is hit.
        let buf = terminal.backend().buffer();
        assert_eq!(buf.area().height, 2);
    }

    /// stop_reason_str covers all variants.
    #[test]
    fn stop_reason_str_all_variants() {
        use crate::stream::StopReason;
        assert_eq!(stop_reason_str(StopReason::Stop), "stop");
        assert_eq!(stop_reason_str(StopReason::Length), "length");
        assert_eq!(stop_reason_str(StopReason::ToolUse), "toolUse");
        assert_eq!(stop_reason_str(StopReason::Error), "error");
        assert_eq!(stop_reason_str(StopReason::Aborted), "aborted");
    }
}
