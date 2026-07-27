# Status: ACCEPTED. Input reader thread: decouple input reading from the render loop

Supersedes two clauses of ADR 0013: "input reading stays on the render thread for minimum keystroke latency" and the tokio-channel citation. The render-thread/tokio split itself stands. Only the input-reading location and the channel mechanism change.

## Context

ADR 0013 put input reading on the render thread: the loop blocked on `input.poll(16ms)` then drained events then drew. This gave ~0ms input latency (crossterm's `event::poll` returns the instant a key is pressed) but made quit latency depend on `input.poll` respecting its timeout. A `Quit` event sent through the channel was only seen after `poll` returned, so a poll that blocked past its timeout would hang shutdown. The quit-flag check (added in the CodeRabbit fix pass) runs after `poll` returns, so it inherits the same dependency.

The tension is structural: on a single synchronous thread, you can only block on one primitive per iteration. If you block on `input.poll`, you cannot be woken by a channel event (Quit). If you block on the channel, input is delayed until the block returns. You cannot have both ~0ms input and immediate quit on one thread with two blocking sources.

## Decision

A dedicated **input reader thread** reads input and sends `InputEvent`s over the same `std::sync::mpsc` channel that carries `RenderEvent`s. The render thread blocks on `recv_timeout(16ms)` on that unified channel, which carries both event types (wrapped in an internal `LoopEvent` enum). A keystroke wakes the block immediately (reader thread reads and sends it). A `Quit` wakes the block immediately (it arrives through the channel). Input latency is <=4ms (reader polls every 4ms, then the channel wakes `recv_timeout` instantly). Quit latency on the render thread is ~0ms (`Quit` wakes `recv_timeout`). Quit latency on the reader side is <=4ms when the channel is not full; when full, the reader is blocked in `tx.send` and checks the flag only after the send succeeds (backpressure).

- **Channel:** `std::sync::mpsc::sync_channel(cap)`. Chosen over `tokio::sync::mpsc` because the render thread is synchronous and needs `recv_timeout` (a sync timed blocking recv). `tokio::sync::mpsc::Receiver` has no sync `recv_timeout` (only `blocking_recv` which blocks forever, and async `recv` which needs a runtime). The sender (`RenderHandle`) uses `try_send` (non-blocking), so tokio tasks can send without blocking a runtime worker. Backpressure on a full channel is the caller's concern (Phase 3).
- **Reader thread:** loops `input.poll(4ms)` then `tx.send(LoopEvent::Input(ev))`. The 4ms poll provides the reader's quit-check cadence (quit latency on the reader side is <=4ms when the channel is not full). `send` blocks when the channel is full (backpressure: a slow render thread throttles input reading, which is correct). The reader exits when the shared quit flag is set.
- **Render thread:** blocks on `rx.recv_timeout(16ms)`, drains the rest non-blocking, applies render events, handles input events, draws if dirty. The 16ms timeout provides the coalescing cadence (ADR 0010). `Quit` (from the channel or the quit flag) exits the loop.
- **Quit path:** `RenderHandle::quit` sets the quit flag and `try_send`s `Quit`. The `Quit` on the channel wakes `recv_timeout` immediately. The reader thread sees the quit flag within 4ms and exits. `Drop` sets the flag, joins the render thread, then joins the reader thread (GOALS goal 2: no orphaned threads).

## Why this supersedes ADR 0013's input clause

ADR 0013 said "input reading stays on the render thread for minimum keystroke latency." The reader thread achieves the same latency (a keystroke is read immediately by the reader and delivered to the render thread within one frame) while decoupling it from the blocking primitive, so quit no longer depends on `input.poll` behavior. The stated rationale (minimum latency) is preserved. The mechanism (which thread reads input) changes.

ADR 0013's tokio-channel citation is superseded: the render-thread channel is `std::sync::mpsc`, not `tokio::sync::mpsc`, because the render thread is synchronous and needs `recv_timeout`. The tokio runtime is unaffected (senders use non-blocking `try_send`). `pi-render` no longer depends on tokio.

## Considered options

- **Keep input on the render thread, channel as blocking primitive, non-blocking input poll** (rejected): would make quit immune to `input.poll`, but input latency degrades from ~0ms to 16ms (input polled only after `recv_timeout` returns). GOALS goal 1 prioritizes input latency. The reader thread preserves ~0ms.
- **Keep current design (poll blocks, quit checked after)** (rejected): quit latency depends on `input.poll` respecting its timeout. Within budget (16ms) for a correct poll, but not immune to a buggy one. The reader thread makes quit structurally immune.

## Consequences

- `pi-render` gains a second OS thread (the input reader). Both are owned by `RenderThread` and joined on `Drop` (GOALS goal 2).
- `pi-render` drops its `tokio` dependency (the channel is `std::sync::mpsc`, threads are `std::thread`, tests use `std::thread::sleep`). The tokio runtime lives in `pi-core` (Phase 3); `RenderHandle::send` is sync non-blocking and safe to call from tokio tasks.
- The render loop no longer takes an `InputSource`; the reader thread does. `RenderThread::spawn` still takes `input: I` but passes it to the reader thread, not the render loop.
- `InputEvent::Quit` arrives through the channel (sent by the reader thread) and via the quit flag (checked after `recv_timeout`). Both paths exit the loop.
- ADR 0013 is amended: "input reading stays on the render thread" becomes "input reading stays on a dedicated reader thread owned by the render subsystem." The render-thread/tokio split, the retained-model ownership, and the "never awaits" invariant are unchanged.

## Sources

1. ADR 0013, the render-thread/tokio split (input clause and tokio-channel citation superseded here): `docs/adr/0013-render-thread-plus-tokio.md`
2. ADR 0010, the streaming markdown pipeline (16ms coalescing cadence the `recv_timeout` provides): `docs/adr/0010-streaming-markdown-pipeline.md`
3. ADR 0029, the render-event contract (the `Quit` variant that now wakes the channel): `docs/adr/0029-render-event-contract-mirrors-pi-streaming-model.md`
