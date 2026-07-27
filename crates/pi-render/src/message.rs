//! Typed render-domain message and content-block types (ADR 0029).
//!
//! These are the typed payloads [`RenderEvent`](crate::event::RenderEvent)
//! carries, mirroring pi's content-block and message unions at the pinned
//! Oracle v0.82.0 (ADR 0007). Per PHILOSOPHY §9.5.
//!
//! `serde_json::Value` is used only for [`ContentBlock::ToolCall`] arguments
//! and tool-execution results (arbitrary model/tool data the renderer
//! displays but never interprets); everything else is typed.
//!
//! **Render projection, not a full mirror:** [`MessageRef`] carries only
//! render-relevant fields. The agent loop (Phase 3) keeps the full
//! `AssistantMessage` (usage, diagnostics, provider, model. Provider
//! concerns, not render state); `MessageRef` is what crosses into the render
//! thread. Documented divergence per §9.5.

use std::sync::Arc;

use serde_json::Value;

/// A content block of a message. Mirrors pi's content union at
/// `packages/ai/src/types.ts` [L329-L356](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L329-L356):
/// `TextContent | ThinkingContent | ImageContent | ToolCall`.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    /// `TextContent` (pi L330-L334). `text_signature` omitted (provider metadata,
    /// not rendered).
    Text { text: String },
    /// `ThinkingContent` (pi L336-L342). `thinking_signature` omitted (provider
    /// metadata). `redacted` preserved (affects display: redacted thinking is
    /// shown as a placeholder).
    Thinking { thinking: String, redacted: bool },
    /// `ImageContent` (pi L345-L348). `data` is base64; `Arc<str>` so cloning
    /// a block with a megabyte image is a refcount bump, not a copy (GOALS
    /// goal 1: no frame-path copy cost).
    Image { data: Arc<str>, mime_type: String },
    /// `ToolCall` (pi L351-L356). `arguments` is arbitrary model-emitted JSON;
    /// `Value` is the honest type (the renderer displays, never interprets).
    /// `thought_signature` omitted (provider metadata).
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
}

/// A message reference carried by [`RenderEvent`](crate::event::RenderEvent).
/// Mirrors pi's `Message` union at
/// `packages/ai/src/types.ts` [L423](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L423):
/// `UserMessage | AssistantMessage | ToolResultMessage`.
///
/// Per-role enum (not a flattened struct) so invalid states are
/// unrepresentable (PHILOSOPHY §4): `stop_reason` exists only on [`Self::Assistant`],
/// `is_error`/`tool_call_id`/`tool_name` only on [`Self::ToolResult`].
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRef {
    /// `UserMessage` (pi L384-L388). `content` is `string | (Text|Image)[]` in
    /// pi; normalized here to `Vec<ContentBlock>` (a plain string becomes a
    /// single `Text` block at the boundary).
    User {
        content: Vec<ContentBlock>,
        timestamp: u64,
    },
    /// `AssistantMessage` (pi L390-L403). `stop_reason` is `Option` (unset
    /// during streaming, `Some` on finalize). Provider fields (api, provider,
    /// model, responseModel, responseId, diagnostics, usage, errorMessage)
    /// omitted. Render projection, not a full mirror (§9.5).
    Assistant {
        content: Vec<ContentBlock>,
        stop_reason: Option<crate::stream::StopReason>,
        timestamp: u64,
    },
    /// `ToolResultMessage` (pi L405-L416). `details` and `usage` and
    /// `added_tool_names` omitted (not rendered). `is_error` preserved
    /// (affects display: errored tool output is styled differently).
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
        timestamp: u64,
    },
}

impl MessageRef {
    /// The content blocks of this message, regardless of role. All three
    /// roles carry `content: Vec<ContentBlock>` (pi L386, L392, L428); this
    /// de-duplicates the access so the render thread projects content without
    /// a 3-arm match at each call site.
    pub fn content(&self) -> &[ContentBlock] {
        match self {
            MessageRef::User { content, .. }
            | MessageRef::Assistant { content, .. }
            | MessageRef::ToolResult { content, .. } => content,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::stream::StopReason;

    use super::*;

    /// ContentBlock mirrors pi's content union (L329-L356): Text, Thinking,
    /// Image, ToolCall.
    #[test]
    fn content_block_variants_match_pi() {
        let _ = ContentBlock::Text { text: "hi".into() };
        let _ = ContentBlock::Thinking {
            thinking: "hmm".into(),
            redacted: false,
        };
        let _ = ContentBlock::Image {
            data: Arc::from(""),
            mime_type: "image/png".into(),
        };
        let _ = ContentBlock::ToolCall {
            id: "tc1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
        };
    }

    /// MessageRef mirrors pi's Message union (L423): User | Assistant |
    /// ToolResult. stop_reason only on Assistant, is_error only on ToolResult.
    #[test]
    fn message_ref_roles_match_pi() {
        let _ = MessageRef::User {
            content: vec![],
            timestamp: 0,
        };
        let _ = MessageRef::Assistant {
            content: vec![],
            stop_reason: None,
            timestamp: 0,
        };
        let _ = MessageRef::ToolResult {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            content: vec![],
            is_error: false,
            timestamp: 0,
        };
    }

    /// content() returns the blocks for any role without a 3-arm match at the
    /// call site.
    #[test]
    fn content_returns_blocks_for_any_role() {
        let blocks = vec![ContentBlock::Text { text: "x".into() }];
        let user = MessageRef::User {
            content: blocks.clone(),
            timestamp: 0,
        };
        let asst = MessageRef::Assistant {
            content: blocks.clone(),
            stop_reason: Some(StopReason::Stop),
            timestamp: 0,
        };
        let tool = MessageRef::ToolResult {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            content: blocks.clone(),
            is_error: false,
            timestamp: 0,
        };
        assert_eq!(user.content(), blocks);
        assert_eq!(asst.content(), blocks);
        assert_eq!(tool.content(), blocks);
    }

    /// Image data is Arc<str>: cloning a block with a large image is a
    /// refcount bump, not a megabyte copy.
    #[test]
    fn image_data_is_arc() {
        let big = ContentBlock::Image {
            data: Arc::from("x".repeat(1024)),
            mime_type: "image/png".into(),
        };
        let _clone = big.clone();
        // If this were String, clone would copy 1024 bytes. Arc clones the
        // pointer. We assert the clone shares the allocation (Arc Eq via ptr).
        match (&big, &_clone) {
            (ContentBlock::Image { data: a, .. }, ContentBlock::Image { data: b, .. }) => {
                assert!(Arc::ptr_eq(a, b), "Arc clone must share the allocation");
            }
            _ => panic!("must be Image"),
        }
    }
}
