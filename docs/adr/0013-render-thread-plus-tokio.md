# Dedicated synchronous render thread. Tokio for everything async

The render loop owns a dedicated OS thread running a tight synchronous loop (poll input ≤16ms → drain state changes → cell-diff → synchronized write) and never awaits. All async concerns - provider streams, Host Protocol traffic, heartbeats, tool subprocesses, timers - run on a tokio runtime on other threads. The sides communicate via channels feeding the retained model [[1]](https://tokio.rs/tokio/tutorial/channels). The render thread consumes state non-blocking (precedent: Bevy's pipelined rendering uses the same main/render thread split [[2]](https://github.com/bevyengine/bevy/blob/main/crates/bevy_render/src/pipelined_rendering.rs)). Frame timing is immune to executor scheduling by construction, and input reading stays on the render thread for minimum keystroke latency.

## Considered Options

- Render loop as a tokio task - rejected: frame timing would depend on executor fairness. Rare small jitter is exactly the jank this project exists to eliminate
- No async runtime, blocking threads everywhere - rejected: hand-rebuilding cancellation, timeouts, and backpressure for network-bound paths where async overhead is irrelevant

## Consequences

- Ownership resolved: the render thread OWNS the Retained Message Model. Tokio tasks never touch it - they send domain events (token appended, tool finished, frame buffer updated) over an mpsc channel, applied at frame start before drawing. Single-threaded mutation, no locks, no torn reads by construction. The tokio side keeps its own agent state and must never need to query display state (CQRS-like split)
- Panic/exit handling must restore the terminal from the render thread's owner (pitfall P3)
- Guards pitfall P2: input handling is structurally decoupled from async load

## Sources

1. tokio channels for cross-thread messaging: https://tokio.rs/tokio/tutorial/channels
2. Bevy pipelined rendering, main/render thread split precedent: https://github.com/bevyengine/bevy/blob/main/crates/bevy_render/src/pipelined_rendering.rs
