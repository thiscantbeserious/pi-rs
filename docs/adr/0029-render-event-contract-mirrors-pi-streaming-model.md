# Status: ACCEPTED. Render-thread event contract: mirror pi's two-layer streaming model

ADR 0013 established the render-thread/tokio split and named the channel ("token appended, tool finished, frame buffer updated") as an illustration, not a formal contract. ADR 0026 assigned `pi-render` ownership of the `RenderEvent` enum and stated "the agent loop translates host/extension events into pi-render's `RenderEvent` enum." Neither defined the variant set. This ADR defines it: `RenderEvent` mirrors pi's two-layer streaming event model at the pinned Oracle (ADR 0007, v0.82.0), plus four pi-rs-native render controls.

The decision was forced by ADR 0010. The streaming markdown pipeline caches finalized blocks and re-highlights only the growing tail block; that requires `*_start`/`*_end` boundaries per content block, which a coarse `TokenAppended(String)` cannot provide. The contract had to be defined before Step 3 (Retained Message Model) and Step 5 (markdown pipeline) target it.

## The two layers pi defines

pi emits streaming events at two layers, both at v0.82.0 (commit `083e6162`):

1. **`AssistantMessageEvent`** — the raw provider streaming protocol. `packages/ai/src/types.ts` [L491-L503](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503). Twelve variants: `start`, `text_start`/`text_delta`/`text_end`, `thinking_start`/`thinking_delta`/`thinking_end`, `toolcall_start`/`toolcall_delta`/`toolcall_end`, `done`, `error`. Each carries `contentIndex` (which content block of the in-flight assistant message) and a `partial: AssistantMessage` snapshot.
2. **`AgentEvent`** — the agent-loop events the UI subscribes to. `packages/agent/src/types.ts` [L422-L437](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437). Eleven variants: `agent_start`/`agent_end`, `turn_start`/`turn_end`, `message_start`/`message_update`/`message_end`, `tool_execution_start`/`tool_execution_update`/`tool_execution_end`. The bridge: `message_update` carries the raw `AssistantMessageEvent` as `assistantMessageEvent` ([L432](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432)), so pi's TUI receives the streaming granularity through it.

pi also defines `AgentSessionEvent` (`packages/coding-agent/src/core/agent-session.ts` [L139-L181](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/core/agent-session.ts#L139-L181)), which extends `AgentEvent` with session-level concerns (compaction, retry, queue updates, entry-appended). These are out of scope for `RenderEvent`: they are session-lifecycle events, not render state changes. The agent loop translates them into `RenderEvent` variants where they affect the display (e.g. `compaction_start` may become a render-control event in Phase 3).

## Decision

`RenderEvent` (in `crates/pi-render/src/event.rs`) has 15 variants:

- **11 agent-loop lifecycle variants** mirroring `AgentEvent` (L422-L437): `AgentStart`, `AgentEnd`, `TurnStart`, `TurnEnd`, `MessageStart`, `MessageUpdate`, `MessageEnd`, `ToolExecutionStart`, `ToolExecutionUpdate`, `ToolExecutionEnd`. `MessageUpdate` nests `AssistantMessageEvent`, mirroring pi's `message_update.assistantMessageEvent` structure (L432).
- **4 render-control variants** (pi-rs-native, ADR 0013): `Resize`, `ThemeChanged`, `FrameBufferUpdated` (ADR 0003), `Quit`.

`AssistantMessageEvent` (in `crates/pi-render/src/stream.rs`) mirrors pi's 12 variants (L491-L503), nested inside `MessageUpdate`.

## Considered options

- **Coarse contract (the Step 2 initial draft: `TokenAppended`, `ToolFinished`, `FrameBufferUpdated`, `ThemeChanged`, `Resize`, `Quit`)** — rejected: collapses pi's streaming lifecycle into `TokenAppended`, losing the `contentIndex` target and the `*_start`/`*_end` boundaries. ADR 0010's finalized-block cache cannot gate on a boundary it never receives. The markdown pipeline (Step 5) would have to re-derive boundaries from delta content, which is lossy and fragile.
- **Full mirror of `AgentEvent` + `AssistantMessageEvent` (chosen)** — accepted: the contract carries the granularity ADR 0010 requires, and mirroring pi's structure means the agent loop (Phase 3) translates `AgentEvent` 1:1 into `RenderEvent` with no lossy collapse. The 11 agent-loop variants have no Phase 2 producer (ROADMAP Phase 2: "explicitly out: agent loop"), but adding variants later is additive (senders keep working; the render thread gains a match arm), so defining the full contract now costs nothing later and gives Steps 3-7 one stable surface to target. The project owner chose this over a Phase-2-only subset to avoid contract churn across steps.
- **Mirror only `AssistantMessageEvent` + controls, defer `AgentEvent` to Phase 3** — rejected by the project owner: would split the contract across two steps and require a second ADR. The full mirror is decided once, here.
- **Flatten the streaming lifecycle into `RenderEvent` (no nesting)** — rejected: pi nests `AssistantMessageEvent` inside `message_update` for a reason — the streaming event is scoped to the in-flight assistant message and carries the message snapshot. Mirroring the nesting keeps the translation from `AgentEvent` to `RenderEvent` structural (a field rename), not a re-derivation.

## Consequences

- **The `partial: AssistantMessage` snapshot is omitted (documented divergence, §9.5).** pi carries the in-flight message snapshot on every streaming variant. Under ADR 0013 the render thread owns the Retained Message Model and applies deltas incrementally, so the snapshot is redundant dead data (PHILOSOPHY §5). The finalized message is carried on `Done`/`Error`. This is a pi-rs divergence from the Oracle, documented and cited, not a silent deviation.
- **Opaque `serde_json::Value` payloads until Step 7 (§9.2 assumption).** `AgentMessage`, `ToolCall`, `ToolResultMessage`, and the `args`/`result`/`partialResult` fields pi types as `any` are carried as `serde_json::Value` until Step 7 (plan doc D-E) defines the typed `RenderMessage` parsed from `pi-session::Message`'s opaque `Value`. Primitive fields (`content_index`, `delta`, `content`, `is_error`, `tool_call_id`, `tool_name`, `cols`, `rows`) are typed now; their shapes are stable across the Oracle.
- **`AgentSessionEvent` is out of scope.** Session-lifecycle events (compaction, retry, queue updates) are not render state changes. The agent loop translates them into `RenderEvent` variants where they affect the display (Phase 3).
- **No Phase 2 producer for the 11 agent-loop variants.** ROADMAP Phase 2 excludes the agent loop. Steps 3-7 exercise the streaming variants (via synthetic fixtures and replay) and the render controls; the agent-loop variants are matched (the `apply` default arm marks them dirty) but not produced until Phase 3. This is the YAGNI trade-off the project owner accepted for a stable contract.
- **`pi-render` gains a `serde_json` dependency** for the opaque payloads. Step 7 replaces `Value` with typed `RenderMessage`; the dependency may move or narrow then.
- **ADR 0013's illustrative list ("token appended, tool finished") is superseded by this contract.** ADR 0013 is not amended otherwise; the split, ownership, and channel mechanism are unchanged.

## Sources

1. pi `AssistantMessageEvent`, the 12-variant provider streaming protocol (Oracle v0.82.0, commit `083e6162`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503>
2. pi `AgentEvent`, the 11-variant agent-loop event the UI subscribes to: <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437>
3. pi `message_update` carries `assistantMessageEvent` (the nesting bridge): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432>
4. pi `StopReason` (the five stop reasons used by `done`/`error`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L382>
5. pi `AgentSessionEvent`, the session-level extension of `AgentEvent` (out of scope for `RenderEvent`): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/core/agent-session.ts#L139-L181>
6. ADR 0013, the render-thread/tokio split this contract serves: `docs/adr/0013-render-thread-plus-tokio.md`
7. ADR 0010, the streaming markdown pipeline whose block-cache requires the `*_start`/`*_end` boundaries: `docs/adr/0010-streaming-markdown-pipeline.md`
8. ADR 0026, which assigns `RenderEvent` ownership to `pi-render`: `docs/adr/0026-phase-2-render-subsystem-pi-render-crate.md`
