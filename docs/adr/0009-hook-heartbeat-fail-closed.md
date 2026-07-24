# Tool-call hooks: unbounded await, heartbeat liveness, fail-closed

Extensions intercept tool calls across the Host Protocol (pi's event interception lets extensions block or modify tool calls [[1]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)), and legitimate hooks wait on human decisions (e.g. safety-guard confirmation dialogs), so hook verdicts are awaited without a timeout. Liveness is a separate concern: the Core heartbeats the Extension Host. A hook counts as hung only when heartbeats stop, not when it is slow. On a hung or dead host, intercepted tool calls fail closed (denied) and the Core surfaces a native prompt - restart host / bypass once / abort turn. Security extensions keep their guarantees. A human choice, never a silent bypass, is the only escape hatch.

## Considered Options

- Fail-open on host failure - rejected: the moment a safety extension crashes would be the moment destructive commands run unguarded
- Bounded per-hook timeout - rejected: structurally incompatible with user-interactive hooks. Would force re-authoring existing extensions
- Declared hook classes (fast/interactive) - rejected for now: existing extensions declare nothing, so the required default collapses to this ADR's behavior. May return as an opt-in protocol extension

## In-flight tool executions

Hook verdicts fail closed, but a tool already executing in the host when the host dies resolves differently: the orphaned call returns a structured error result ("extension host terminated during execution") to the LLM, which can react, retry, or adapt - matching how tool failures already flow. The native restart prompt appears in parallel. The turn continues, the session stays intact, and re-execution is never automatic (the dead tool may have had side effects).

## Consequences

- The Host Protocol carries heartbeats independent of request/response traffic
- The agent loop can block indefinitely on a hook while rendering continues untouched (ADRs 0003/0004)
- The Core needs native (host-independent) prompt UI for the failure path

## Sources

1. pi extensions, event interception (block or modify tool calls): https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md
