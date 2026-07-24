# Alternate screen with a retained message model

The Core runs in the terminal's alternate screen and owns the entire grid, rendering from a retained message model rather than printing history into native scrollback. Chosen for maximum rendering control and performance: perfect re-wrap on resize, live theme switching, retroactive collapse/expand of tool output, marker-based turn navigation, and semantic search over message content - none of which an inline scrollback model can support. Precedent: [opencode's full-screen TUI](https://opencode.ai/docs/tui/) (which launches in the alternate screen buffer). Prior art: agent-session-recorder's viewport-over-buffer player. Honest counter-evidence: opencode users have requested a non-fullscreen inline mode for native scrolling and copying, confirming the trade-off is real and the copy-mode mitigations below are mandatory, not optional.

## Considered Options

- Inline + diffed live region (pi/Claude Code today) - rejected: history is write-once, permanently forfeiting restyling, re-wrap, folding, and content search
- Alt screen + scrollback sync hybrid - rejected: complex, terminal-dependent, mid-session native scrolling still broken

## Consequences

- Must implement: custom scrollback viewport, search, copy-mode
- Native mouse selection is eaten by mouse capture: copy-mode and yank commands (message / code block / tool output) are the primary answer. Modifier-key passthrough is terminal-dependent and must not be relied on (Shift in xterm-style terminals, [Option in iTerm2](https://iterm2.com/documentation-preferences-profiles-terminal.html), configurable in kitty)
- History is memory-bounded, not terminal-infinite
- On exit, nothing remains in the terminal: dump a transcript tail to the normal screen buffer
