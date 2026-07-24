# /reload restarts the Extension Host process

/reload performs a graceful Extension Host shutdown (drain in-flight hooks per ADR 0009), respawns the host, and re-registers extensions. The Core's session and UI state — the retained model — survives untouched. Crash recovery and /reload share one tested lifecycle path instead of maintaining a separate in-process hot-reload mechanism.

## Considered Options

- In-process module reload mirroring pi's hot-reload — rejected: a second lifecycle path plus JS module-cache invalidation quirks, for a marginal speed win on an infrequent operation

## Consequences

- Extension in-memory state does not survive /reload; state that matters must live in session entries (ADR 0016) — matching the discipline pi extensions should already follow
- Host restart time is the /reload latency budget; keep host boot fast
