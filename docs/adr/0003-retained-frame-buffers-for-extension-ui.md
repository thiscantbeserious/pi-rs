# Extension UI crosses the Host Protocol as retained frame buffers

The Core renders with a cell-diff, synchronized-output (DEC 2026) frame loop that must never block on the Extension Host. Extension-owned UI components (ctx.ui.custom) therefore render in the Extension Host on their own state changes and push the resulting lines over the Host Protocol; the Core caches these buffers and composites them into frames at its own pace.

## Considered Options

- Synchronous render RPC per frame — rejected: one slow extension janks the whole TUI, IPC round-trip inside the frame loop
- Declarative widget DTOs (VS Code-style) — rejected for compatibility: breaks every existing component that emits raw ANSI lines; may be added later as an optional fast path

## Consequences

- The frame loop is latency-isolated from extensions: worst case a component's region is one message stale
- Input events stream asynchronously from Core to the owning extension
- Input focus is Core-owned: exactly one focus owner at a time; modal extension UI (ctx.ui.select/confirm/custom) receives an explicit focus grant from the Core and returns it on close — focus routing is part of the Host Protocol, not an extension concern
- The existing render(width)/invalidate/requestRender extension API shape is preserved
