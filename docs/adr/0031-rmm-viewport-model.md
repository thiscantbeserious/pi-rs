# Status: ACCEPTED. RMM viewport model: visible-window rendering with message-granular line cache

ADR 0004 decided the Retained Message Model owns scroll state and the viewport is a pure function of it. ADR 0024 chose ratatui on crossterm as the terminal backend. Neither specified HOW the RMM and ratatui coexist: ratatui has no scrollback, no virtualization, and its `Buffer` is terminal-sized (width x height cells). This ADR specifies the viewport model, the rendering strategy, and the line cache that make a 10M+ token session render in O(viewport_height) per frame, not O(total_lines).

## Context

Researched against the pinned Oracle (v0.82.0, commit `083e6162`) and ratatui 0.30.2 / ratatui-core 0.1.2:

- **pi re-renders ALL components every frame.** `TUI.render(width)` walks all children ([L280-L289](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L280-L289)), and `doRender` ([L1258](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L1258)) diffs the result against `previousLines` ([L297](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L297)). pi relies on per-component caching: `Markdown.cachedLines` ([L152-L157](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L152-L157)) returns pre-rendered lines if `text` and `width` are unchanged. pi's sessions are bounded by compaction.
- **ratatui has no scrollback and no virtualization.** Its `Buffer` is sized to the terminal viewport (`Frame::area`). `Terminal::try_draw` ([render.rs L81](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/terminal/render.rs#L81)) runs a render callback that populates the buffer, then `Terminal::flush` ([buffers.rs L97](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/terminal/buffers.rs#L97)) diffs the new buffer against the previous one via `Buffer::diff` ([buffer.rs L471](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/buffer.rs#L471)) and sends only changed cells to `Backend::draw`. ratatui renders only what's in `Frame::area`; it does not model content outside the viewport.
- **ratatui's `CrosstermBackend::draw` does NOT wrap with mode 2026.** It writes cell-by-cell via `queue!` ([crossterm.rs L151](https://github.com/ratatui/ratatui/blob/0.29.0/src/backend/crossterm.rs#L151)). BSU/ESU must be injected around `backend.draw()`.

## Decision

### Visible-window rendering

The RMM holds the message list (`Vec<MessageRef>`) and a scroll offset (`usize`). The projection renders only the visible window (`scroll_offset..scroll_offset + viewport_height`) into ratatui's terminal-sized `Buffer` each frame. ratatui's `Buffer::diff` handles the cell-granular diff. This is O(viewport_height) per frame, not O(total_lines).

- The viewport is a pure function of `(message_list, scroll_offset, width, height)` (ADR 0004).
- ratatui renders only `Frame::area`; we feed it only the visible slice.
- No offscreen buffer for scrollback (memory: O(total_messages), not O(total_lines *width* cell_size)).

### Pin-to-tail auto-scroll (Phase 2)

In Phase 2, `scroll_offset` is always pinned to the tail: `scroll_offset = max(0, total_rendered_lines - viewport_height)`. When new content arrives (dirty), the offset is recomputed. Interactive scrollback, search, copy-mode are Phase 3 (ADR 0007).

### Message-granular line cache

The RMM caches rendered lines per finalized message, keyed by `(message_content, width, theme)`. Like pi's `Markdown.cachedLines` (markdown.ts L152-L157). On resize (width changed) or theme change, the cache is invalidated. The streaming tail message is never cached (re-renders each frame). This makes the projection walk O(visible_lines) because cached messages return pre-rendered lines.

Step 5 (markdown pipeline, ADR 0010) formalizes this as block-granular caching (finalized blocks cached, tail block re-highlighted). Step 3 does message-granular as a seed.

### SyncBackend wrapper for mode 2026

A custom `Backend` wrapper (`SyncBackend<W>`) wraps `CrosstermBackend<W>`. It overrides `Backend::draw` to queue `BeginSynchronizedUpdate` before delegating to `inner.draw()` and `EndSynchronizedUpdate` after. This is the exact injection point: ratatui calls `backend.draw(diff_iter)` inside `Terminal::flush`, so BSU/ESU wraps the cell writes tightly (P12).

Mode 2026 capability is queried at startup (`CSI ? 2026 $ p`). The response arrives on the event stream. Where supported, `SyncBackend` injects BSU/ESU. Where unsupported, it passes through (cell-diff still minimizes tearing, P12 graceful degradation).

### FrameSink wraps Terminal

The real `FrameSink` impl owns a `Terminal<SyncBackend<W>>`. Its `draw()` calls `terminal.try_draw(|frame| project(state, frame))`. ratatui does `Buffer::diff` + `Backend::draw` + `Backend::flush` internally. We do not call `Buffer::diff` ourselves.

### Frame-buffer compositing

`RenderEvent::FrameBufferUpdated` carries a synthetic frame buffer (lines + region). The projection composites it into the visible window: the frame buffer lines are written into the ratatui `Buffer` at the region's coordinates, overwriting transcript cells there. The render thread caches the latest frame buffer (ADR 0003: "composites them into frames at its own pace"). Focus routing is Phase 3. In Phase 2 the source is synthetic (ROADMAP: "host frame buffers may be faked").

### Resize

On resize, the line cache is invalidated (width changed, cached lines are wrong). The projection re-renders the visible window at the new width. ratatui's `autoresize` handles the `Buffer` dimensions; our cache invalidation handles content correctness. This is required: without invalidation, the projection feeds old-width lines into the new-width `Buffer`.

## Considered options

- **Render all messages every frame (like pi)**: rejected. pi re-renders all components (with caching) and its `previousLines` array grows with content. For 10M+ token sessions without compaction, the projection walk is O(N) per frame even with caching. The visible-window model is O(viewport_height).
- **Offscreen Buffer for scrollback (tui-scrollview pattern)**: rejected. Allocates a `Buffer` sized to the entire content (O(total_lines *width* cell_size)). For 10M tokens that is gigabytes. The visible-window model needs no offscreen buffer.
- **Let ratatui manage the viewport**: rejected. ratatui has no scrollback concept, no auto-scroll-to-tail, no resize re-wrap. P5 (unstable scrollback) would not be guarded. Contradicts ADR 0004.
- **No line cache (re-render from scratch each frame)**: rejected. Re-parses and re-wraps every visible message every frame. The message-granular cache makes finalized messages O(1) to render (return cached lines).
- **Block-granular cache from the start**: rejected. Requires the markdown pipeline (pulldown-cmark + tree-sitter) which is Step 5. Step 3 is ASCII-scoped. Message-granular is the seed; Step 5 formalizes it (ADR 0010).
- **Wrap try_draw externally with BSU/ESU**: rejected. BSU before `try_draw` wraps cursor commands too (wasteful); ESU after wraps everything. The pair is looser than P12 wants ("tight around each flush"). The `SyncBackend` wrapper injects BSU/ESU around `backend.draw()` only (the cell writes), which is the tightest possible pair.
- **Drop mode 2026**: rejected. ADR 0024 decided to use it. pi uses it (tui.ts L1329/L1349). Cell-diff without 2026 flickers on multi-cell updates (GOALS goal 1).

## Consequences

- **Frame cost is O(viewport_height), not O(total_lines).** A 24-row viewport renders 24 rows of cells per frame regardless of session size. ratatui's `Buffer::diff` then reduces the terminal writes to only changed cells.
- **Memory is O(total_messages) for the message list, not O(total_lines) for rendered content.** The line cache holds rendered lines only for finalized messages in the visible window (evicted when scrolled out of view in Phase 3; in Phase 2 all messages are cached since there's no interactive scroll). Step 7 (replay) reads entries incrementally.
- **The RMM owns scroll state.** `scroll_offset` is part of `RenderState`. The viewport is a pure function of `(message_list, scroll_offset, width, height)`. Phase 3 adds user-controlled `scroll_offset` (interactive scrollback, search, copy-mode).
- **`pi-render` gains ratatui `Terminal` + `Backend` types in the real `FrameSink` impl.** Step 2's `CountingSink` stays for tests. The `FrameSink` trait is unchanged (`draw(&RenderState)`); the real impl owns the `Terminal` internally.
- **`SyncBackend` is a new struct in `pi-render`.** It wraps `CrosstermBackend` and is the sole place mode 2026 is injected. The capability flag is set at startup from the mode 2026 query.
- **Resize invalidates the line cache.** This is required for correctness (old-width lines in a new-width Buffer are wrong). ratatui's `autoresize` handles the Buffer dimensions automatically.
- **Selection/copy-mode is Phase 3.** Alt screen + mouse capture means native selection is eaten (ADR 0004). Copy-mode and yank commands are the answer. Step 3 renders the visible window; selection is an input-routing concern that needs the RMM to exist first.

## Sources

1. pi `TUI.render(width)` walks all children (the re-render-all pattern): `packages/tui/src/tui.ts` [L280-L289](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L280-L289)
2. pi `doRender` (the differential rendering method with mode 2026, viewport tracking): `packages/tui/src/tui.ts` [L1258](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L1258)
3. pi line-diff loop (firstChanged/lastChanged): `packages/tui/src/tui.ts` [L1378](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L1378)
4. pi BSU (`\x1b[?2026h`): `packages/tui/src/tui.ts` [L1329](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L1329)
5. pi ESU (`\x1b[?2026l`): `packages/tui/src/tui.ts` [L1349](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/tui.ts#L1349)
6. pi `Markdown.cachedLines` (per-component line cache, invalidated on text/width change): `packages/tui/src/components/markdown.ts` [L152-L157](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L152-L157)
7. ratatui `Terminal::try_draw` (the render pipeline: autoresize, callback, flush, cursor, swap, backend.flush): `ratatui-core/src/terminal/render.rs` [L81](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/terminal/render.rs#L81)
8. ratatui `Terminal::flush` (calls `backend.draw(diff_iter)`): `ratatui-core/src/terminal/buffers.rs` [L97](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/terminal/buffers.rs#L97)
9. ratatui `Buffer::diff` (cell-granular diff with multi-width handling): `ratatui-core/src/buffer/buffer.rs` [L471](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/buffer.rs#L471)
10. ratatui `CrosstermBackend::draw` (cell-by-cell queue, no mode 2026): `ratatui-0.29.0/src/backend/crossterm.rs` [L151](https://github.com/ratatui/ratatui/blob/0.29.0/src/backend/crossterm.rs#L151)
11. ratatui rendering under the hood (immediate-mode, double-buffer, diff): <https://ratatui.rs/concepts/rendering/under-the-hood/>
12. ratatui `Backend` trait (the `draw` method we wrap): <https://docs.rs/ratatui/latest/ratatui/backend/trait.Backend.html>
13. ADR 0004 (RMM owns scroll state, viewport is a pure function): `docs/adr/0004-alternate-screen-retained-message-model.md`
14. ADR 0024 (ratatui on crossterm, mode 2026 is ours to wrap): `docs/adr/0024-terminal-backend-ratatui-on-crossterm.md`
15. ADR 0010 (streaming markdown pipeline, block-granular cache): `docs/adr/0010-streaming-markdown-pipeline.md`
16. ADR 0003 (frame-buffer compositing): `docs/adr/0003-retained-frame-buffers-for-extension-ui.md`
