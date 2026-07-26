# Terminal backend: ratatui on crossterm

The render thread (ADR 0013) drives the terminal through ratatui on crossterm, not by re-implementing the cell layer directly on crossterm. ratatui's `Buffer::diff` is the cell-granular diff ADR 0004 and pitfall P9 require, already tested for the double-width/CJK cases pitfall P13 names as the diff-corruption failure mode; crossterm exposes synchronized output (mode 2026), the kitty keyboard protocol, raw mode, and resize events directly; ratatui's `flush`/`draw` are synchronous (no `async`), so ADR 0013's "render thread never awaits" holds. The render thread still OWNS the terminal and the Retained Message Model (ADR 0013) — it does so *through* ratatui/crossterm, not by hand-rolling a cell grid and ANSI emitter. This refines, not supersedes, ADR 0013's "owns the terminal": ownership is about which thread mutates terminal state and that it never blocks on async, not about re-implementing the cell layer.

## Considered Options

- **crossterm direct, no ratatui** — rejected: re-derives the tested cell-diff that P9/P13 flag as the bug-prone surface. `Buffer::diff` already handles the multi-width cases (`コ` skipping the trailing cell, double-width at index 1 skipping index 2) that a naive diff gets wrong and that P13's corpus exists to catch [[1]](https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html). Re-implementing it buys no measured upside over ratatui and re-exposes the failure class the project exists to eliminate.
- **ratatui + custom `Backend` trait impl (not `CrosstermBackend`)** — rejected for now: would let us own the ANSI emit and mode 2026 path while reusing ratatui's `Buffer`/`diff`, but `CrosstermBackend` already wraps crossterm and we reach the raw crossterm via `Terminal::backend_mut()` for direct commands. Justified only if `CrosstermBackend` blocks something we find in implementation; YAGNI until then (PHILOSOPHY §5).

## Consequences

- **One crossterm, workspace-unified.** ratatui pulls crossterm transitively; any other crate that touches the terminal must use the same crossterm version. Pitfall P16 (duplicate crossterm versions cause event-queue mismatches and restore races) is guarded by workspace-level dependency unification and a `cargo deny` duplicate-version check in CI, starting here.
- **Mode 2026 is ours to wrap, not ratatui's.** ratatui does not drive synchronized output itself. The render thread wraps each `terminal.flush()` in crossterm's `BeginSynchronizedUpdate`/`EndSynchronizedUpdate` (or `SynchronizedUpdate`), keeping BSU/ESU pairs tight per pitfall P12. Capability is queried at startup (`CSI ? 2026 $ p`); where unsupported, cell-diff still minimizes tearing (P12 graceful degradation).
- **Kitty keyboard protocol via crossterm directly.** `PushKeyboardEnhancementFlags` / pop on every enter/exit and suspend/resume path (pitfall P14), driven through `Terminal::backend_mut()`, not through a ratatui API (ratatui has none).
- **The Codex failure class (P5 unstable scrollback, P7 platform-dependent rendering) is not inherited by adopting ratatui.** P5's guard is "the Retained Message Model owns scroll state; the viewport is a pure function of it" (ADR 0004), a model property independent of the backend. P7's guard is the CI terminal matrix including Windows Terminal and WSL, not the backend choice. ratatui is the backend Codex uses, but their failures trace to model and testing gaps, not to `CrosstermBackend` per se.
- **Panic-safe restore (P15) is installed before the first draw** as a single owned restore path: leave alt screen, disable raw mode, pop keyboard flags, disable mouse capture. The render thread owns this hook because it owns the terminal session (ADR 0013). Zero-size terminal guard in the resize handler (P15).
- **Width is NOT delegated to ratatui.** ratatui's `CellWidth` is `unicode-width`-based (per-codepoint), the P13 failure class, with no override hook and known ZWJ emoji bugs [[2]](https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs) [[3]](https://github.com/ratatui/ratatui/issues/75). Width is computed at our message-to-cell projection layer (ADR 0025); ratatui receives pre-widthed spans. This is the one place we deliberately bypass ratatui's layer.

## Sources

1. ratatui `Buffer::diff`, multi-width cell handling (the P9/P13 guard): <https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html>
2. ratatui `CellWidth` trait, delegates to `unicode-width` (the P13 limitation we bypass): <https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs>
3. ratatui #75, `unicode-width` and emojis (the open failure class): <https://github.com/ratatui/ratatui/issues/75>
4. ratatui `Terminal::flush`/`backend_mut`/`autoresize`, synchronous signatures (ADR 0013 satisfied): <https://docs.rs/ratatui/latest/ratatui/type.DefaultTerminal.html>
5. crossterm `PushKeyboardEnhancementFlags` (kitty keyboard protocol, P14 guard): <https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs>
6. crossterm `SynchronizedUpdate` (mode 2026, P12 guard): <https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html>
