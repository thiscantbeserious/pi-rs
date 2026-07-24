# Tool-call hooks: unbounded await, heartbeat liveness, fail-closed

Extensions intercept tool calls across the Host Protocol, and legitimate hooks wait on human decisions (e.g. safety-guard confirmation dialogs), so hook verdicts are awaited without a timeout. Liveness is a separate concern: the Core heartbeats the Extension Host; a hook counts as hung only when heartbeats stop, not when it is slow. On a hung or dead host, intercepted tool calls fail closed (denied) and the Core surfaces a native prompt — restart host / bypass once / abort turn. Security extensions keep their guarantees; a human choice, never a silent bypass, is the only escape hatch.

## Considered Options

- Fail-open on host failure — rejected: the moment a safety extension crashes would be the moment destructive commands run unguarded
- Bounded per-hook timeout — rejected: structurally incompatible with user-interactive hooks; would force re-authoring existing extensions
- Declared hook classes (fast/interactive) — rejected for now: existing extensions declare nothing, so the required default collapses to this ADR's behavior; may return as an opt-in protocol extension

## Consequences

- The Host Protocol carries heartbeats independent of request/response traffic
- The agent loop can block indefinitely on a hook while rendering continues untouched (ADRs 0003/0004)
- The Core needs native (host-independent) prompt UI for the failure path
