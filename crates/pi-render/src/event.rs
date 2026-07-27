//! The render-thread channel contract (ADR 0013).
//!
//! Events flow one way: tokio tasks (the agent loop, the Host Protocol,
//! providers, tool subprocesses) send [`RenderEvent`]s over an mpsc channel
//! into the render thread, which owns the Retained Message Model and applies
//! them single-threaded (no locks, no torn reads by construction). The tokio
//! side keeps its own agent state and never queries display state (CQRS-like
//! split, ADR 0013).
//!
//! **Parity:** the agent-loop lifecycle variants mirror pi's `AgentEvent` at
//! the pinned Oracle v0.82.0 (ADR 0007): `packages/agent/src/types.ts`
//! [L422-L437](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437).
//! [`RenderEvent::MessageUpdate`] nests pi's `AssistantMessageEvent` (see
//! [`stream`](crate::stream)), mirroring pi's `message_update` which carries
//! `assistantMessageEvent` ([L432](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432)).
//! Per PHILOSOPHY §9.5.
//!
//! **Typed payloads:** messages and content blocks are typed Rust types
//! ([`MessageRef`](crate::message::MessageRef),
//! [`ContentBlock`](crate::message::ContentBlock)), not `serde_json::Value`.
//! The agent loop (Phase 3) sends typed structs with zero serialization;
//! `pi-replay` (Step 7) parses `Value` → typed at the replay boundary. `Value`
//! is used only for `ToolCall::arguments` and tool-execution
//! `result`/`partial_result` (arbitrary model/tool data the renderer displays
//! but never interprets). See ADR 0029.
//!
//! **No pi equivalent:** the render-control variants ([`Resize`],
//! [`ThemeChanged`], [`FrameBufferUpdated`], [`Quit`]) are pi-rs-native
//! (pi is single-process JS; the render-thread/tokio split is pi-rs-native,
//! ADR 0013). [`FrameBufferUpdated`] is pi-rs-native to the protocol but
//! preserves pi's `render(width)` shape (ADR 0003).
//!
//! **Not `Clone`:** events flow one-way into the render thread (ADR 0013);
//! the render thread consumes them, never clones them. Dropping `Clone`
//! removes the footgun of accidentally copying megabyte image payloads on the
//! frame path (GOALS goal 1).
//!
//! [`Resize`]: RenderEvent::Resize
//! [`ThemeChanged`]: RenderEvent::ThemeChanged
//! [`FrameBufferUpdated`]: RenderEvent::FrameBufferUpdated
//! [`Quit`]: RenderEvent::Quit

use serde_json::Value;

use crate::message::MessageRef;
use crate::stream::AssistantMessageEvent;

/// A state change the tokio side signals to the render thread. The render
/// thread drains these non-blocking at frame start (ADR 0013) and applies
/// them to the Retained Message Model (Step 3).
///
/// The agent-loop lifecycle (variants through [`MessageEnd`]) mirrors pi's
/// `AgentEvent` (types.ts L422-L437). The render controls are pi-rs-native.
///
/// [`MessageEnd`]: RenderEvent::MessageEnd
#[derive(Debug, PartialEq)]
pub enum RenderEvent {
    // --- Agent lifecycle (pi AgentEvent L424-L425) ---
    /// `agent_start` (pi L424). An agent run began.
    AgentStart,
    /// `agent_end` (pi L425). An agent run ended; `messages` is the final
    /// `AgentMessage[]` (typed render projection).
    AgentEnd { messages: Vec<MessageRef> },

    // --- Turn lifecycle (pi AgentEvent L427-L428) ---
    /// `turn_start` (pi L427). A turn (one assistant response + tool calls)
    /// began.
    TurnStart,
    /// `turn_end` (pi L428). A turn ended with its assistant `message` and
    /// `tool_results` (typed render projections).
    TurnEnd {
        message: MessageRef,
        tool_results: Vec<MessageRef>,
    },

    // --- Message lifecycle (pi AgentEvent L430-L433) ---
    /// `message_start` (pi L430). A message (user, assistant, or toolResult)
    /// began; `message` is the typed render projection.
    MessageStart { message: MessageRef },
    /// `message_update` (pi L432). Only emitted for assistant messages during
    /// streaming. Carries the in-flight `message` (the mid-stream metadata
    /// channel: usage/stop_reason/diagnostics change during the stream; delta
    /// events carry none of those) and the streaming `event` (pi's
    /// `assistantMessageEvent`), nested per pi's structure.
    MessageUpdate {
        message: MessageRef,
        event: AssistantMessageEvent,
    },
    /// `message_end` (pi L433). A message finalized.
    MessageEnd { message: MessageRef },

    // --- Tool execution lifecycle (pi AgentEvent L435-L437) ---
    /// `tool_execution_start` (pi L435). A tool began executing. `args` is pi's
    /// `any` (arbitrary model-emitted JSON; the renderer displays, never
    /// interprets).
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    /// `tool_execution_update` (pi L436). A tool emitted a partial result.
    /// `partial_result` is arbitrary tool data (displayed, not interpreted).
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        partial_result: Value,
    },
    /// `tool_execution_end` (pi L437). A tool finished; `is_error` mirrors pi's
    /// `isError: boolean`. `result` is arbitrary tool data (displayed, not
    /// interpreted).
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: Value,
        is_error: bool,
    },

    // --- Render controls (pi-rs-native, ADR 0013) ---
    /// The terminal was resized. The viewport re-wraps as a pure function of
    /// the Retained Message Model (ADR 0004, pitfall P5).
    Resize { cols: u16, rows: u16 },
    /// The active theme changed (Step 6). Flushes the block cache (ADR 0010)
    /// and re-projects.
    ThemeChanged,
    /// An extension frame buffer was updated (ADR 0003). In Phase 2 the source
    /// is synthetic; the Host Protocol wires real buffers in Phase 3.
    FrameBufferUpdated,
    /// Stop the render thread. Drained at frame start; the loop exits cleanly.
    Quit,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::message::{ContentBlock, MessageRef};
    use crate::stream::StopReason;

    fn asst_ref() -> MessageRef {
        MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        }
    }

    /// The 11 agent-loop variants mirror pi's AgentEvent (L422-L437), and the
    /// 4 render controls are pi-rs-native. 15 total.
    #[test]
    fn render_event_variants_match_pi_plus_controls() {
        let _ = RenderEvent::AgentStart;
        let _ = RenderEvent::AgentEnd { messages: vec![] };
        let _ = RenderEvent::TurnStart;
        let _ = RenderEvent::TurnEnd {
            message: asst_ref(),
            tool_results: vec![],
        };
        let _ = RenderEvent::MessageStart {
            message: asst_ref(),
        };
        let _ = RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "x".into(),
            },
        };
        let _ = RenderEvent::MessageEnd {
            message: asst_ref(),
        };
        let _ = RenderEvent::ToolExecutionStart {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            args: json!({}),
        };
        let _ = RenderEvent::ToolExecutionUpdate {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            args: json!({}),
            partial_result: json!({}),
        };
        let _ = RenderEvent::ToolExecutionEnd {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            result: json!({}),
            is_error: false,
        };
        // Render controls (no pi equivalent).
        let _ = RenderEvent::Resize { cols: 80, rows: 24 };
        let _ = RenderEvent::ThemeChanged;
        let _ = RenderEvent::FrameBufferUpdated;
        let _ = RenderEvent::Quit;
    }

    /// MessageUpdate nests AssistantMessageEvent, mirroring pi's
    /// message_update.assistantMessageEvent (L432). The message field is the
    /// mid-stream metadata channel (kept, not dropped — see ADR 0029).
    #[test]
    fn message_update_nests_streaming_event() {
        let ev = RenderEvent::MessageUpdate {
            message: asst_ref(),
            event: AssistantMessageEvent::Done {
                reason: StopReason::Stop,
                message: asst_ref(),
            },
        };
        match ev {
            RenderEvent::MessageUpdate { event, .. } => {
                assert!(matches!(
                    event,
                    AssistantMessageEvent::Done {
                        reason: StopReason::Stop,
                        ..
                    }
                ));
            }
            _ => panic!("must be MessageUpdate"),
        }
    }

    /// MessageUpdate's message carries typed content (not Value): the render
    /// thread reads content blocks directly, no deserialization.
    #[test]
    fn message_update_carries_typed_content() {
        let msg = MessageRef::Assistant {
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            stop_reason: None,
            timestamp: 0,
        };
        let ev = RenderEvent::MessageUpdate {
            message: msg,
            event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "!".into(),
            },
        };
        match ev {
            RenderEvent::MessageUpdate { message, .. } => {
                assert_eq!(message.content().len(), 1);
            }
            _ => panic!("must be MessageUpdate"),
        }
    }
}
