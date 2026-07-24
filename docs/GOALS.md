# Goals

The three goals of pi-rs, in priority order. When goals conflict, the lower number wins. Every ADR, every review, and every merge should be answerable to these.

## 1. Absolute smooth performance — especially when streaming

The reason this project exists. Token streaming, input handling, and scrolling must feel instantaneous and stay that way under load — no flicker, no restyle jank, no keystroke lag, no frame drops while a 20MB tool output scrolls past.

- The render loop never waits on anything: not extensions, not the network, not the async runtime (ADRs 0003, 0013)
- Cell-diff frames, synchronized output, frame coalescing, block-granular highlight caching (ADRs 0004, 0010)
- Measured, not felt: frame-time and input-latency benchmarks under synthetic streaming workloads; the pitfalls watchlist (docs/pitfalls.md) is the regression suite's backbone

## 2. Strong quality in the concurrent core — multi-process, multi-threading, IPC

pi-rs is a system of cooperating processes and threads: the Core's render thread and tokio runtime, the Extension Host, tool subprocesses, subagents. The failure modes that matter live at these seams — races, deadlocks, orphaned processes, silent hook bypasses, corrupted protocol streams.

- Every boundary has defined failure semantics: heartbeat liveness and fail-closed hooks (ADR 0009), crash-isolated host (ADR 0001), schema-drift-proof protocol (ADRs 0006, 0011)
- Subprocess and subagent lifecycles are owned: no zombies, no leaked PTYs, cancellation and cleanup on every exit path including panics
- Enforced by the CI gates: sanitizers (ASan/LSan), coverage thresholds, conformance tests generated from the protocol source of truth

## 3. Feature parity with pi

Full parity with a pinned pi version is the release bar — pi-rs is a better pi, not a different tool (ADR 0007).

- Existing extensions run unmodified (ADR 0001); sessions interop bidirectionally (ADR 0008); themes load unchanged (ADR 0012)
- Parity is measured: ported test suite + session-corpus replay with byte-identical re-save
- Parity never justifies violating goals 1 or 2 — a feature ported with jank or a race is not ported

## Priority in practice

- A feature that would compromise streaming smoothness gets redesigned, not shipped (1 > 3)
- A shortcut across a process/thread boundary to hit parity faster is rejected (2 > 3)
- Performance work that undermines correctness at the seams is not performance work (1 loses to 2 only when smoothness is bought with unsoundness)
