# Dedicated synchronous render thread; tokio for everything async

The render loop owns a dedicated OS thread running a tight synchronous loop (poll input ≤16ms → drain state changes → cell-diff → synchronized write) and never awaits. All async concerns — provider streams, Host Protocol traffic, heartbeats, tool subprocesses, timers — run on a tokio runtime on other threads. The sides communicate via lock-free channels feeding the retained model; the render thread consumes state non-blocking. Frame timing is immune to executor scheduling by construction, and input reading stays on the render thread for minimum keystroke latency.

## Considered Options

- Render loop as a tokio task — rejected: frame timing would depend on executor fairness; rare small jitter is exactly the jank this project exists to eliminate
- No async runtime, blocking threads everywhere — rejected: hand-rebuilding cancellation, timeouts, and backpressure for network-bound paths where async overhead is irrelevant

## Consequences

- The retained model needs a clear ownership story between the tokio side (writers) and render thread (reader) — channel of state deltas or snapshot swap, never shared mutable locking on the frame path
- Panic/exit handling must restore the terminal from the render thread's owner (pitfall P3)
- Guards pitfall P2: input handling is structurally decoupled from async load
