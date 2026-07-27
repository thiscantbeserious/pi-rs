# Status: ACCEPTED. Render-thread event contract: typed payloads mirroring pi's two-layer streaming model

ADR 0013 established the render-thread/tokio split and named the channel ("token appended, tool finished, frame buffer updated") as an illustration, not a formal contract. ADR 0026 assigned `pi-render` ownership of the `RenderEvent` enum and stated "the agent loop translates host/extension events into pi-render's `RenderEvent` enum." Neither defined the variant set or the payload types. This ADR defines both: `RenderEvent` mirrors pi's two-layer streaming event model at the pinned Oracle (ADR 0007, v0.82.0), plus four pi-rs-native render controls, with **typed Rust payloads** (not `serde_json::Value`).

The decision was forced by ADR 0010. The streaming markdown pipeline caches finalized blocks and re-highlights only the growing tail block; that requires `*_start`/`*_end` boundaries per content block, which a coarse `TokenAppended(String)` cannot provide. The contract had to be defined before Step 3 (Retained Message Model) and Step 5 (markdown pipeline) target it. The payload-type question (typed vs. `Value`) was forced by PHILOSOPHY §4 (parse, don't validate) and GOALS goal 1 (no frame-path serialization tax): the agent loop (Phase 3) is Rust-native with typed structs, so carrying `Value` would force a serialize/deserialize round-trip on every event at the boundary where type safety should be strongest.

## The two layers pi defines

pi emits streaming events at two layers, both at v0.82.0 (commit `083e6162`):

1. **`AssistantMessageEvent`** — the raw provider streaming protocol. `packages/ai/src/types.ts` [L491-L503](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503). Twelve variants: `start`, `text_start`/`text_delta`/`text_end`, `thinking_start`/`thinking_delta`/`thinking_end`, `toolcall_start`/`toolcall_delta`/`toolcall_end`, `done`, `error`. Each carries `contentIndex` (which content block of the in-flight assistant message) and a `partial: AssistantMessage` snapshot.
2. **`AgentEvent`** — the agent-loop events the UI subscribes to. `packages/agent/src/types.ts` [L422-L437](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437). Eleven variants: `agent_start`/`agent_end`, `turn_start`/`turn_end`, `message_start`/`message_update`/`message_end`, `tool_execution_start`/`tool_execution_update`/`tool_execution_end`. The bridge: `message_update` carries the raw `AssistantMessageEvent` as `assistantMessageEvent` ([L432](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432)) alongside the in-flight `message`, so pi's TUI receives the streaming granularity through it.

pi also defines `AgentSessionEvent` (`packages/coding-agent/src/core/agent-session.ts` [L139-L181](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/core/agent-session.ts#L139-L181)), which extends `AgentEvent` with session-level concerns (compaction, retry, queue updates, entry-appended). These are out of scope for `RenderEvent`: they are session-lifecycle events, not render state changes. The agent loop translates them into `RenderEvent` variants where they affect the display (Phase 3).

## Decision

`RenderEvent` (in `crates/pi-render/src/event.rs`) has 15 variants:

- **11 agent-loop lifecycle variants** mirroring `AgentEvent` (L422-L437): `AgentStart`, `AgentEnd`, `TurnStart`, `TurnEnd`, `MessageStart`, `MessageUpdate`, `MessageEnd`, `ToolExecutionStart`, `ToolExecutionUpdate`, `ToolExecutionEnd`. `MessageUpdate` nests `AssistantMessageEvent`, mirroring pi's `message_update.assistantMessageEvent` structure (L432).
- **4 render-control variants** (pi-rs-native, ADR 0013): `Resize`, `ThemeChanged`, `FrameBufferUpdated` (ADR 0003), `Quit`.

`AssistantMessageEvent` (in `crates/pi-render/src/stream.rs`) mirrors pi's 12 variants (L491-L503), nested inside `MessageUpdate`.

### Typed payloads

Messages and content blocks are **typed Rust types**, not `serde_json::Value`:

- **`ContentBlock`** (in `crates/pi-render/src/message.rs`) mirrors pi's content union (`packages/ai/src/types.ts` [L329-L356](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L329-L356)): `Text | Thinking | ToolCall | Image`. `Image::data` is `Arc<str>` (base64, potentially megabytes; `Arc` so cloning a block with a large image is a refcount bump, not a copy — GOALS goal 1).
- **`MessageRef`** mirrors pi's `Message` union ([L423](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L423)) as a per-role enum: `User | Assistant | ToolResult`. This makes invalid states unrepresentable (PHILOSOPHY §4): `stop_reason` exists only on `Assistant`, `is_error`/`tool_call_id`/`tool_name` only on `ToolResult`. A `content()` method de-duplicates content-block access across roles.
- **`MessageRef` is a render projection, not a full mirror (§9.5).** It carries only render-relevant fields (role, content blocks, `stop_reason`, `is_error`, `timestamp`). The provider concerns (`usage`, `diagnostics`, `provider`, `model`, `responseModel`, `responseId`, `errorMessage`) are omitted — the agent loop (Phase 3) keeps the full `AssistantMessage`; `MessageRef` is what crosses into the render thread.

`serde_json::Value` is used **only** for `ToolCall::arguments` (pi L354, `Record<string, any>` — arbitrary model-emitted JSON) and tool-execution `result`/`partial_result` (pi L436-L437, `any` — arbitrary tool data). Both are data the renderer displays but never interprets; `Value` is the honest type at that boundary.

### The mid-stream metadata channel (narrowed divergence)

pi carries two snapshots on `message_update` (L432): `message` (the in-flight `AssistantMessage`) and `assistantMessageEvent` (which itself carries `partial: AssistantMessage` on every variant). This ADR narrows the divergence:

- **`partial` on each `AssistantMessageEvent` variant is omitted.** Under ADR 0013 the render thread owns the Retained Message Model and applies deltas incrementally, so the per-delta content snapshot is redundant dead data (PHILOSOPHY §5).
- **`message` on `MessageUpdate` is kept.** It is not a content snapshot — it is the mid-stream metadata channel. `usage`, `stopReason`, `diagnostics`, and `responseModel` change during the stream (token counts accumulate, stop reason is set on done, diagnostics appear on error/recovery), and the delta events (`text_delta`, etc.) carry none of those fields. Dropping `message` would forfeit the live-usage-display path (pi shows token usage during the stream). Keeping `message` preserves the metadata channel and is faithful to pi's structure (L432 carries both for a reason).

## Considered options

- **Coarse contract (`TokenAppended`, `ToolFinished`, ...)** — rejected: collapses pi's streaming lifecycle, losing the `contentIndex` target and the `*_start`/`*_end` boundaries. ADR 0010's block cache cannot gate on a boundary it never receives.
- **`serde_json::Value` for all complex payloads** — rejected: the agent loop (Phase 3) is Rust-native with typed structs; carrying `Value` forces a serialize/deserialize round-trip on the frame path and throws away type safety (PHILOSOPHY §4) at the boundary where it should be strongest. `Value` is only justified at the replay boundary (Step 7, `pi-replay` parses JSONL), not inside the in-process channel. The agent loop sends typed structs with zero serialization; `pi-replay` parses `Value` → typed at the boundary.
- **Full mirror of `AgentEvent` + `AssistantMessageEvent` with typed payloads (chosen)** — accepted: the contract carries the granularity ADR 0010 requires, typed payloads keep the frame path type-safe and zero-cost, and mirroring pi's structure means the agent loop (Phase 3) translates `AgentEvent` 1:1 into `RenderEvent` with no lossy collapse. The 11 agent-loop variants have no Phase 2 producer (ROADMAP Phase 2: "explicitly out: agent loop"), but adding variants later is additive (senders keep working; the render thread gains a match arm), and the variants are Oracle-cited (ported, not guessed), so defining the full contract now costs nothing later and gives Steps 3-7 one stable surface to target.
- **Mirror only `AssistantMessageEvent` + controls, defer `AgentEvent` to Phase 3** — rejected: would split the contract across two steps and require a second ADR. The full mirror is decided once, here.
- **Flatten the streaming lifecycle into `RenderEvent` (no nesting)** — rejected: pi nests `AssistantMessageEvent` inside `message_update` for a reason — the streaming event is scoped to the in-flight assistant message and carries the message snapshot. Mirroring the nesting keeps the translation from `AgentEvent` to `RenderEvent` structural (a field rename), not a re-derivation.
- **`MessageRef` as a single struct with optional fields** — rejected: permits invalid states (`stop_reason: Some` on a `UserMessage`). The per-role enum makes invalid states unrepresentable (PHILOSOPHY §4); `stop_reason` only on `Assistant`, `is_error` only on `ToolResult`. The content-access boilerplate is de-duplicated by a `content()` method.
- **`Image::data` as `String`** — rejected: `RenderEvent` was `Clone` (since revised, see below); cloning a megabyte image event would copy the whole base64 string. `Arc<str>` makes image cloning a refcount bump. Images are immutable once produced, so `Arc` is the honest type.
- **`RenderEvent: Clone`** — rejected: events flow one-way into the render thread (ADR 0013); the render thread consumes them, never clones them. `Clone` was only used in one test assertion. Dropping it removes the footgun of accidentally copying megabyte image payloads on the frame path (GOALS goal 1).

## Consequences

- **Typed payloads survive Phase 3 unchanged.** The agent loop sends typed Rust structs; `pi-replay` (Step 7) parses `Value` → typed at the replay boundary. The contract does not reverse when the real producer lands. Step 7's `RenderMessage` (plan doc D-E) becomes the typed `MessageRef`/`ContentBlock` already defined here, not a `Value` replacement.
- **`serde_json` stays in `pi-render`** but only for `ToolCall::arguments` and tool-execution `result`/`partial_result` — a narrower, justified use than "opaque everything." Step 7 may narrow it further or move it to `pi-replay`.
- **The `partial` divergence is narrowed, not total.** Only the per-delta content snapshot is omitted (redundant under ADR 0013). The `message` field on `MessageUpdate` is kept as the mid-stream metadata channel. This is documented and cited per §9.5, not a silent deviation.
- **`MessageRef` is a render projection.** Provider concerns (`usage`, `diagnostics`, `provider`, `model`) are omitted. The agent loop keeps the full `AssistantMessage`; `MessageRef` is the render-domain projection. Documented divergence per §9.5.
- **No Phase 2 producer for the 11 agent-loop variants.** ROADMAP Phase 2 excludes the agent loop. Steps 3-7 exercise the streaming variants (via synthetic fixtures and replay) and the render controls; the agent-loop variants are matched (the `apply` default arm marks them dirty) but not produced until Phase 3. This is the YAGNI trade-off accepted for a stable contract; the variants are Oracle-cited, not guessed.
- **`AgentSessionEvent` is out of scope.** Session-lifecycle events (compaction, retry, queue updates) are not render state changes. The agent loop translates them into `RenderEvent` variants where they affect the display (Phase 3).
- **`RenderEvent` is not `Clone`.** Events flow one-way into the render thread (ADR 0013); the render thread consumes, never clones. This removes the footgun of copying megabyte image payloads on the frame path. `ContentBlock` remains `Clone` (the RMM in Step 3 may need to clone blocks for its own purposes; that's a Step 3 decision).
- **ADR 0013's illustrative list ("token appended, tool finished") is superseded by this contract.** ADR 0013 is not amended otherwise; the split, ownership, and channel mechanism are unchanged.

## Sources

1. pi `AssistantMessageEvent`, the 12-variant provider streaming protocol (Oracle v0.82.0, commit `083e6162`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503>
2. pi `AgentEvent`, the 11-variant agent-loop event the UI subscribes to: <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437>
3. pi `message_update` carries `assistantMessageEvent` and `message` (the nesting bridge and metadata channel): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432>
4. pi `StopReason` (the five stop reasons used by `done`/`error`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L382>
5. pi content-block union (`TextContent | ThinkingContent | ImageContent | ToolCall`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L329-L356>
6. pi `Message` union (`UserMessage | AssistantMessage | ToolResultMessage`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L423>
7. pi `AgentSessionEvent`, the session-level extension of `AgentEvent` (out of scope for `RenderEvent`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/core/agent-session.ts#L139-L181>
8. ADR 0013, the render-thread/tokio split this contract serves: `docs/adr/0013-render-thread-plus-tokio.md`
9. ADR 0010, the streaming markdown pipeline whose block-cache requires the `*_start`/`*_end` boundaries: `docs/adr/0010-streaming-markdown-pipeline.md`
10. ADR 0026, which assigns `RenderEvent` ownership to `pi-render`: `docs/adr/0026-phase-2-render-subsystem-pi-render-crate.md`
