//! Minimal render-thread state (Step 2 seed of the Retained Message Model,
//! ADR 0004).
//!
//! Step 3 replaces this with the full RMM (message list, cell grid, viewport,
//! scroll state). Step 2 needs just enough state for the channel-contract
//! tests: an applied-event counter the frame sink reads to prove "applied
//! before the next frame", and the last appended token.
//!
//! The render thread is the sole mutator (ADR 0013): single-threaded mutation,
//! no locks, no torn reads. The tokio side never touches this struct; it sends
//! [`RenderEvent`]s instead.
//!
//! No pi equivalent (pi is single-process JS; the Retained Message Model is
//! pi-rs-native, ADR 0004). Per PHILOSOPHY §9.5.

use crate::event::RenderEvent;

/// Render-thread-owned state. Mutated only on the render thread (ADR 0013).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RenderState {
    /// Events applied since spawn. The frame sink reads this to prove an event
    /// was applied before the next frame drew (Step 2 spec). Step 3 removes
    /// this in favour of the real RMM.
    pub applied: u64,
    /// The last token appended (test observable). Step 3 grows this into the
    /// full message list.
    pub last_token: String,
    /// Set by [`RenderEvent::Quit`]; the loop exits when true.
    pub quit: bool,
}

impl RenderState {
    /// Apply a batch of drained events single-threaded (ADR 0013). Returns
    /// whether the state changed (dirty) so the loop can skip a redraw when
    /// idle (ADR 0010 coalescing budget).
    ///
    /// `Quit` is a control signal: it sets the quit flag but does not mark the
    /// state dirty (no draw needed on the exit frame). All other events mark
    /// the state dirty.
    pub fn apply(&mut self, events: &[RenderEvent]) -> bool {
        let mut dirty = false;
        for ev in events {
            match ev {
                RenderEvent::TokenAppended(tok) => {
                    self.last_token.push_str(tok);
                    self.applied += 1;
                    dirty = true;
                }
                RenderEvent::ToolFinished
                | RenderEvent::FrameBufferUpdated
                | RenderEvent::ThemeChanged
                | RenderEvent::Resize { .. } => {
                    self.applied += 1;
                    dirty = true;
                }
                RenderEvent::Quit => {
                    self.quit = true;
                }
            }
        }
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RED: applying a token marks the state dirty and records the token.
    #[test]
    fn apply_token_appends_and_marks_dirty() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[RenderEvent::TokenAppended("hi".into())]);
        assert!(dirty, "appending a token must mark the state dirty");
        assert_eq!(s.last_token, "hi");
        assert_eq!(s.applied, 1);
    }

    /// RED: applying multiple tokens in one batch concatenates them.
    #[test]
    fn apply_batch_concatenates_tokens() {
        let mut s = RenderState::default();
        s.apply(&[
            RenderEvent::TokenAppended("foo".into()),
            RenderEvent::TokenAppended("bar".into()),
        ]);
        assert_eq!(s.last_token, "foobar");
        assert_eq!(s.applied, 2);
    }

    /// RED: Quit sets the quit flag but does not mark the state dirty (no draw
    /// needed on the exit frame).
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

    /// RED: an empty event batch changes nothing and is not dirty.
    #[test]
    fn apply_empty_is_noop() {
        let mut s = RenderState::default();
        let dirty = s.apply(&[]);
        assert!(!dirty);
        assert_eq!(s, RenderState::default());
    }
}
