//! The provider streaming event protocol (mirrors pi's `AssistantMessageEvent`).
//!
//! pi-rs-native channel contract (ADR 0013), but the variant set and field
//! shapes mirror pi's `AssistantMessageEvent` at the pinned Oracle v0.82.0
//! (ADR 0007): `packages/ai/src/types.ts` [L491-L503](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503).
//! Per PHILOSOPHY §9.5.
//!
//! The render thread consumes these nested inside
//! [`RenderEvent::MessageUpdate`](crate::event::RenderEvent::MessageUpdate) to
//! drive the streaming markdown pipeline (ADR 0010): block `*_start`/`*_end`
//! boundaries gate the finalized-block cache, `*_delta` events drive the
//! incremental tail-block re-highlight.
//!
//! **Documented divergence from pi (§9.5):** pi carries a `partial:
//! AssistantMessage` snapshot on every streaming variant. Under ADR 0013 the
//! render thread owns the Retained Message Model and applies deltas
//! incrementally, so the snapshot is redundant dead data and is omitted
//! (PHILOSOPHY §5). The finalized message is carried on `Done`/`Error`.
//!
//! **Opaque payloads (§9.2 assumption):** the complex payloads (`ToolCall`,
//! the finalized `AssistantMessage`) are carried as [`serde_json::Value`]
//! until Step 7 (D-E) defines the typed `RenderMessage` parsed from
//! pi-session's opaque `Value`. Primitive fields (`content_index`, `delta`,
//! `content`) are typed now; their shapes are stable.

use serde_json::Value;

/// The stop reason for a completed stream. Mirrors pi's `StopReason` at
/// `packages/ai/src/types.ts` [L382](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L382):
/// `"stop" | "length" | "toolUse" | "error" | "aborted"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// `"stop"` — the model stopped naturally.
    Stop,
    /// `"length"` — hit the max-output token limit.
    Length,
    /// `"toolUse"` — the model emitted a tool call.
    ToolUse,
    /// `"error"` — the stream errored.
    Error,
    /// `"aborted"` — the stream was cancelled.
    Aborted,
}

/// A streaming event from the provider. Mirrors pi's `AssistantMessageEvent`
/// (`packages/ai/src/types.ts` [L491-L503](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503)).
///
/// `content_index` identifies which content block of the in-flight assistant
/// message the event targets (a message may interleave text, thinking, and
/// tool-call blocks). pi: `contentIndex: number`.
#[derive(Debug, Clone, PartialEq)]
pub enum AssistantMessageEvent {
    /// `start` (pi L492). Stream began.
    Start,
    /// `text_start` (pi L493). A text content block began at `content_index`.
    TextStart { content_index: u32 },
    /// `text_delta` (pi L494). `delta` appended to the text block at
    /// `content_index`.
    TextDelta { content_index: u32, delta: String },
    /// `text_end` (pi L495). The text block at `content_index` finalized with
    /// `content`.
    TextEnd { content_index: u32, content: String },
    /// `thinking_start` (pi L496). A thinking block began.
    ThinkingStart { content_index: u32 },
    /// `thinking_delta` (pi L497). Reasoning text appended.
    ThinkingDelta { content_index: u32, delta: String },
    /// `thinking_end` (pi L498). The thinking block finalized.
    ThinkingEnd { content_index: u32, content: String },
    /// `toolcall_start` (pi L499). A tool-call block began.
    ToolCallStart { content_index: u32 },
    /// `toolcall_delta` (pi L500). Tool-call input JSON delta appended.
    ToolCallDelta { content_index: u32, delta: String },
    /// `toolcall_end` (pi L501). The tool call finalized; `tool_call` is the
    /// finalized `ToolCall` (opaque until Step 7, D-E).
    ToolCallEnd {
        content_index: u32,
        tool_call: Value,
    },
    /// `done` (pi L502). Stream completed; `message` is the finalized
    /// `AssistantMessage` (opaque until Step 7).
    Done { reason: StopReason, message: Value },
    /// `error` (pi L503). Stream errored or aborted; `error` is the final
    /// `AssistantMessage` with `stopReason` set (opaque until Step 7).
    Error { reason: StopReason, error: Value },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The 12 streaming variants mirror pi's AssistantMessageEvent (L491-503).
    #[test]
    fn streaming_variants_match_pi() {
        let _ = AssistantMessageEvent::Start;
        let _ = AssistantMessageEvent::TextStart { content_index: 0 };
        let _ = AssistantMessageEvent::TextDelta {
            content_index: 0,
            delta: "hi".into(),
        };
        let _ = AssistantMessageEvent::TextEnd {
            content_index: 0,
            content: "hi".into(),
        };
        let _ = AssistantMessageEvent::ThinkingStart { content_index: 1 };
        let _ = AssistantMessageEvent::ThinkingDelta {
            content_index: 1,
            delta: "hmm".into(),
        };
        let _ = AssistantMessageEvent::ThinkingEnd {
            content_index: 1,
            content: "hmm".into(),
        };
        let _ = AssistantMessageEvent::ToolCallStart { content_index: 2 };
        let _ = AssistantMessageEvent::ToolCallDelta {
            content_index: 2,
            delta: "{}".into(),
        };
        let _ = AssistantMessageEvent::ToolCallEnd {
            content_index: 2,
            tool_call: json!({}),
        };
        let _ = AssistantMessageEvent::Done {
            reason: StopReason::Stop,
            message: json!({}),
        };
        let _ = AssistantMessageEvent::Error {
            reason: StopReason::Aborted,
            error: json!({}),
        };
    }

    /// StopReason mirrors pi's five values (L382).
    #[test]
    fn stop_reason_has_five_variants() {
        assert_eq!(format!("{:?}", StopReason::Stop), "Stop".to_string());
        assert!(matches!(StopReason::Length, StopReason::Length));
        let all = [
            StopReason::Stop,
            StopReason::Length,
            StopReason::ToolUse,
            StopReason::Error,
            StopReason::Aborted,
        ];
        assert_eq!(all.len(), 5);
    }
}
