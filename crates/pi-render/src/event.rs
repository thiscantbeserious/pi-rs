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
//! **No pi equivalent:** the render-control variants ([`Resize`],
//! [`ThemeChanged`], [`FrameBufferUpdated`], [`Quit`]) are pi-rs-native
//! (pi is single-process JS; the render-thread/tokio split is pi-rs-native,
//! ADR 0013). [`FrameBufferUpdated`] is pi-rs-native to the protocol but
//! preserves pi's `render(width)` shape (ADR 0003).
//!
//! **Opaque payloads (§9.2 assumption):** `AgentMessage`, `ToolCall`,
//! `ToolResultMessage`, and the `args`/`result`/`partial_result` fields pi
//! types as `any` are carried as [`serde_json::Value`] until Step 7 (D-E)
//! defines the typed `RenderMessage`. Primitive fields are typed now.
//!
//! [`Resize`]: RenderEvent::Resize
//! [`ThemeChanged`]: RenderEvent::ThemeChanged
//! [`FrameBufferUpdated`]: RenderEvent::FrameBufferUpdated
//! [`Quit`]: RenderEvent::Quit

use serde_json::Value;

use crate::stream::AssistantMessageEvent;

/// A state change the tokio side signals to the render thread. The render
/// thread drains these non-blocking at frame start (ADR 0013) and applies
/// them to the Retained Message Model (Step 3).
///
/// The agent-loop lifecycle (variants through [`MessageEnd`]) mirrors pi's
/// `AgentEvent` (types.ts L422-L437). The render controls are pi-rs-native.
///
/// [`MessageEnd`]: RenderEvent::MessageEnd
#[derive(Debug, Clone, PartialEq)]
pub enum RenderEvent {
    // --- Agent lifecycle (pi AgentEvent L424-L425) ---
    /// `agent_start` (pi L424). An agent run began.
    AgentStart,
    /// `agent_end` (pi L425). An agent run ended; `messages` is the final
    /// `AgentMessage[]` (opaque until Step 7).
    AgentEnd { messages: Vec<Value> },

    // --- Turn lifecycle (pi AgentEvent L427-L428) ---
    /// `turn_start` (pi L427). A turn (one assistant response + tool calls)
    /// began.
    TurnStart,
    /// `turn_end` (pi L428). A turn ended with its assistant `message` and
    /// `tool_results` (opaque until Step 7).
    TurnEnd {
        message: Value,
        tool_results: Vec<Value>,
    },

    // --- Message lifecycle (pi AgentEvent L430-L433) ---
    /// `message_start` (pi L430). A message (user, assistant, or toolResult)
    /// began; `message` is the `AgentMessage` (opaque until Step 7).
    MessageStart { message: Value },
    /// `message_update` (pi L432). Only emitted for assistant messages during
    /// streaming. Carries the in-flight `message` and the streaming `event`
    /// (pi's `assistantMessageEvent`), nested per pi's structure.
    MessageUpdate {
        message: Value,
        event: AssistantMessageEvent,
    },
    /// `message_end` (pi L433). A message finalized.
    MessageEnd { message: Value },

    // --- Tool execution lifecycle (pi AgentEvent L435-L437) ---
    /// `tool_execution_start` (pi L435). A tool began executing. `args` is pi's
    /// `any` (opaque).
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    /// `tool_execution_update` (pi L436). A tool emitted a partial result.
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        partial_result: Value,
    },
    /// `tool_execution_end` (pi L437). A tool finished; `is_error` mirrors pi's
    /// `isError: boolean`.
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
    use crate::stream::StopReason;

    /// The 11 agent-loop variants mirror pi's AgentEvent (L422-L437), and the
    /// 4 render controls are pi-rs-native. 15 total.
    #[test]
    fn render_event_variants_match_pi_plus_controls() {
        let _ = RenderEvent::AgentStart;
        let _ = RenderEvent::AgentEnd { messages: vec![] };
        let _ = RenderEvent::TurnStart;
        let _ = RenderEvent::TurnEnd {
            message: json!({}),
            tool_results: vec![],
        };
        let _ = RenderEvent::MessageStart { message: json!({}) };
        let _ = RenderEvent::MessageUpdate {
            message: json!({}),
            event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "x".into(),
            },
        };
        let _ = RenderEvent::MessageEnd { message: json!({}) };
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
    /// message_update.assistantMessageEvent (L432).
    #[test]
    fn message_update_nests_streaming_event() {
        let ev = RenderEvent::MessageUpdate {
            message: json!({ "role": "assistant" }),
            event: AssistantMessageEvent::Done {
                reason: StopReason::Stop,
                message: json!({}),
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
}
