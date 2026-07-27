//! Minimal render-thread state (Step 2 seed of the Retained Message Model,
//! ADR 0004).
//!
//! Step 3 replaces this with the full RMM (message list, cell grid, viewport,
//! scroll state). Step 2 needs just enough state for the channel-contract
//! tests: an applied-event counter the frame sink reads to prove "applied
//! before the next frame", and the last appended token stream.
//!
//! The render thread is the sole mutator (ADR 0013): single-threaded mutation,
//! no locks, no torn reads. The tokio side never touches this struct; it sends
//! [`RenderEvent`]s instead.
//!
//! No pi equivalent (pi is single-process JS; the Retained Message Model is
//! pi-rs-native, ADR 0004). Per PHILOSOPHY §9.5.

use crate::event::RenderEvent;
use crate::stream::AssistantMessageEvent;

/// Render-thread-owned state. Mutated only on the render thread (ADR 0013).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RenderState {
    /// Events applied since spawn. The frame sink reads this to prove an event
    /// was applied before the next frame drew (Step 2 spec). Step 3 removes
    /// this in favour of the real RMM.
    pub applied: u64,
    /// The concatenated stream of text/thinking/toolcall deltas (test
    /// observable). Step 3 grows this into the full message list.
    pub last_token: String,
    /// Set by [`RenderEvent::Quit`]; the loop exits when true.
    pub quit: bool,
}

impl RenderState {
    /// Apply a batch of drained events single-threaded (ADR 0013). Returns
    /// whether the state changed (dirty) so the loop can skip a redraw when
    /// idle (ADR 0010 coalescing budget).
    ///
    /// Streaming deltas (`TextDelta`, `ThinkingDelta`, `ToolCallDelta`) append
    /// to `last_token`. `Quit` is a control signal: it sets the quit flag but
    /// does not mark the state dirty (no draw needed on the exit frame). All
    /// other events mark the state dirty.
    pub fn apply(&mut self, events: &[RenderEvent]) -> bool {
        let mut dirty = false;
        for ev in events {
            match ev {
                // Streaming deltas append to the token stream.
                RenderEvent::MessageUpdate {
                    event:
                        AssistantMessageEvent::TextDelta { delta, .. }
                        | AssistantMessageEvent::ThinkingDelta { delta, .. }
                        | AssistantMessageEvent::ToolCallDelta { delta, .. },
                    ..
                } => {
                    self.last_token.push_str(delta);
                    self.applied += 1;
                    dirty = true;
                }
                // Other MessageUpdate events (start/end, done, error) mark
                // dirty; Step 3 uses their boundaries for block caching.
                RenderEvent::MessageUpdate { .. } => {
                    self.applied += 1;
                    dirty = true;
                }
                RenderEvent::Quit => {
                    self.quit = true;
                }
                _ => {
                    self.applied += 1;
                    dirty = true;
                }
            }
        }
        dirty
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A streaming text delta (nested in MessageUpdate) appends and marks dirty.
    #[test]
    fn apply_text_delta_appends_and_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: json!({}),
            event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "hi".into(),
            },
        }]);
        assert!(dirty, "appending a delta must mark the state dirty");
        assert_eq!(s.last_token, "hi");
        assert_eq!(s.applied, 1);
    }

    /// Multiple deltas in one batch concatenate.
    #[test]
    fn apply_batch_concatenates_deltas() {
        let mut s = RenderState::default();
        s.apply(&[
            RenderEvent::MessageUpdate {
                message: json!({}),
                event: AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: "foo".into(),
                },
            },
            RenderEvent::MessageUpdate {
                message: json!({}),
                event: AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: "bar".into(),
                },
            },
        ]);
        assert_eq!(s.last_token, "foobar");
        assert_eq!(s.applied, 2);
    }

    /// Thinking and toolcall deltas also append to the stream.
    #[test]
    fn apply_thinking_and_toolcall_deltas_append() {
        let mut s = RenderState::default();
        s.apply(&[
            RenderEvent::MessageUpdate {
                message: json!({}),
                event: AssistantMessageEvent::ThinkingDelta {
                    content_index: 1,
                    delta: "hmm".into(),
                },
            },
            RenderEvent::MessageUpdate {
                message: json!({}),
                event: AssistantMessageEvent::ToolCallDelta {
                    content_index: 2,
                    delta: "{}".into(),
                },
            },
        ]);
        assert_eq!(s.last_token, "hmm{}");
    }

    /// A streaming start/end boundary marks dirty (Step 3 uses it for caching).
    #[test]
    fn apply_streaming_boundary_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::MessageUpdate {
            message: json!({}),
            event: AssistantMessageEvent::TextStart { content_index: 0 },
        }]);
        assert!(dirty);
        assert_eq!(s.applied, 1);
        assert!(s.last_token.is_empty(), "boundaries carry no delta");
    }

    /// An agent-loop event (e.g. AgentStart) marks dirty.
    #[test]
    fn apply_agent_loop_event_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::AgentStart]);
        assert!(dirty);
        assert_eq!(s.applied, 1);
    }

    /// Quit sets the quit flag but does not mark dirty (no draw on exit frame).
    #[test]
    fn apply_quit_sets_flag_without_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::Quit]);
        assert!(s.quit, "Quit must set the quit flag");
        assert!(!dirty, "Quit must not mark the state dirty");
        assert_eq!(
            s.applied, 0,
            "Quit is a control signal, not an applied event"
        );
    }

    /// An empty event batch changes nothing and is not dirty.
    #[test]
    fn apply_empty_is_noop() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[]);
        assert!(!dirty);
        assert_eq!(s, RenderState::default());
    }
}
