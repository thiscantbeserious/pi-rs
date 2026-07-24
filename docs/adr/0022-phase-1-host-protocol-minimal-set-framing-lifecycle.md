# Phase 1 Host Protocol minimal message set, framing, and lifecycle semantics

The Host Protocol's wire format, message envelope, and lifecycle semantics are hard to reverse once messages flow, so they are fixed before implementation (Phase 1 step 2 grill). Every decision below had genuine alternatives and was picked for specific reasons. The minimal Phase 1 set is nine messages; the full extension-facing surface (docs/extension-api-surface.md) lands in Phase 3.

## Framing (Q1)

Each frame on the wire is a 4-byte big-endian `u32` length prefix followed by that many bytes of MessagePack-encoded message body. The length counts the body only, not itself. Both sides own a small `read_frame`/`write_frame` helper. This is the one thing both sides must agree on byte-for-byte; the conformance fixtures (Q10) lock it in.

## Connection direction and first speaker (Q2)

The Core binds the UDS path *before* spawning the host and passes the path via the `PI_RS_HOST_SOCKET` environment variable. The host connects and sends `Handshake` as the first frame. The listener survives `kill -9` of the host (the host only ever held a connection, not the listener), so respawn lands on the same path with zero rebind race. This directly serves exit gate 2 (Core survives host `kill -9`).

## Message envelope (Q5)

All messages are variants of one Rust enum with `#[serde(tag = "type")]` (internally tagged), using the verbatim PascalCase Rust variant names as the wire tag strings. Wire form: `{"type":"Handshake", "protocol_version":1, "host_pid":123}`. Unit variants render as `{"type":"Heartbeat"}`.

Chosen over externally-tagged `[variant_index, data]` (rmp-serde default) because ADR 0006 specifies a `--protocol-json` debug flag for human-readable tracing, and integer tags make that trace opaque. Chosen over adjacently-tagged `{tag, content}` because our variant shapes (structs and units only) need no wrapper. Internally-tagged enums require every variant to be a struct or unit, which all nine messages satisfy.

## The nine messages

1. `Handshake { protocol_version: u32, host_pid: u32 }` (host → Core, first frame)
2. `HandshakeAck` (Core → host, on accepted handshake)
3. `Heartbeat` (Core → host, periodic)
4. `Pong` (host → Core, heartbeat reply)
5. `Shutdown { drain: bool }` (Core → host)
6. `ShutdownAck` (host → Core, only for `drain: true`)
7. `EchoRequest { request_id: u64, payload: Vec<u8> }` (bidirectional)
8. `EchoResponse { request_id: u64, payload: Vec<u8> }` (bidirectional, reply)
9. `ProtocolError { code: ProtocolErrorCode, message: String }` (bidirectional)

### Handshake (Q3)

`protocol_version` is a single `u32` monotonic revision, exact-match-or-refuse (not semver). ADR 0011 generates Core and host types from one Rust source at one commit, so there is no designed version skew, only accidental drift from a stale build. A single integer that says "exact match or refuse" is the strongest guard against that. Mismatch → Core closes the socket, logs, respawns the host (treated as a failed boot, subject to step 5's restart backoff). The host exits non-zero on handshake rejection.

Handshake validates *identity and version*. Liveness is a separate concern owned by the heartbeat layer (ADR 0009): a slow hook is not a hung host. The two are never conflated.

`HandshakeAck` is a distinct message rather than folded into the first `Heartbeat`: the handshake dance (once per connection) and the liveness loop (periodic) have different invariants, and conflating them makes the state machine murkier. The host must know the Core *accepted its version* before it can consider itself booted.

### Heartbeat timing (Q7)

Core sends `Heartbeat` at 5000 ms intervals. Host replies `Pong`. A missed `Pong` = no reply within one interval (5000 ms). Three consecutive misses (15 s total) → the host is declared hung → fail-closed per ADR 0009 (deny intercepted tool calls, surface the native restart prompt) → Core closes the socket and respawns.

The calmer 5 s interval over the conventional 1 s is deliberate: false-positive respawns are destructive (respawning a healthy host mid-work), and the cost of a false positive (lost in-flight work) exceeds the cost of slower detection (a frozen screen the user can also resolve with a manual restart). 15 s detection is acceptable for an interactive TUI. Recorded here so the threshold is not mistaken for accidental.

### Shutdown drain (Q8)

`Shutdown { drain: bool }`. `drain: true` → the host finishes any in-flight *protocol-message handling*, sends `ShutdownAck`, exits. `drain: false` → the host exits immediately, no ack. Phase 1 has no in-flight hooks or tools (Phase 1 fences out tools), so the drain wait set is empty in Phase 1. The message shape and ack semantics are fixed now so Phase 3 extends only the *wait set* (adds hook/tool drain per ADR 0009) without changing the envelope. `drain: false`'s caller is "Core wants the host gone now but the host is still responsive" (e.g. a fatal host-side error); when the host is hung it won't process `Shutdown` anyway, so the Core just closes the socket.

### Echo and request correlation (Q6)

`EchoRequest`/`EchoResponse` carry a `request_id: u64` assigned by the sender, opaque to the receiver, matched on the reply. This establishes the request/response correlation pattern from message one, so Phase 3's tool calls (`ToolCall`/`ToolResult`) inherit it without redesign. Echo is two distinct types (request initiates, response replies) rather than one type with an `is_response` flag, mirroring how tool calls will look and splitting the "request and response" that the code rules require.

### ProtocolError scope (Q9)

`ProtocolError` is transport-only: unknown message type, malformed frame, unexpected message (e.g. `Pong` before `Handshake`), handshake-rejected detail. It is sent by whichever side detects a protocol-level problem. The `code` is a Rust enum (`UnknownMessageType`, `MalformedFrame`, `UnexpectedMessage`, `HandshakeRejected`) serialized as the variant-name string, consistent with the envelope's internally-tagged convention, and ts-rs generates it as a TS union.

ADR 0009's in-flight-tool-error ("extension host terminated during execution") is **not** a `ProtocolError`. It is a *tool result* with error semantics, which lands in Phase 3 as a `ToolResult` variant correlated by `request_id`, never a side error channel. The wire being fine while a tool execution fails is a different concern from the wire being broken.

## Conformance fixtures (Q10)

Rust generates `host/protocol/fixtures.bin` as a sequence of length-prefixed msgpack frames (exercising the real wire format from the framing decision, not just msgpack decode). The file is committed and freshness-checked like the `.ts` type files (ADR 0011: drift is a build failure). The Deno conformance test reads it frame-by-frame, decodes each `Message`, asserts the decoded shape matches the expected TS type (catches codegen drift), re-encodes it, and the Rust side asserts byte-identity on the round-trip (catches codec divergence). Dual assertion, one fixture file.

## Considered Options (rejected alternatives worth remembering)

- **Externally-tagged envelope** `[variant_index, data]`: rejected for opaque integer tags defeating the `--protocol-json` trace.
- **Semver `protocol_version`**: rejected because ADR 0011's single-source codegen means there is no designed version skew to support, only accidental drift to refuse.
- **1 s heartbeat / 1-miss threshold**: rejected as too false-positive-prone; the cost of a false respawn exceeds slower detection.
- **`drain` field removed, graceful shutdown as a future message**: rejected because ADR 0017 requires a graceful drain path for `/reload`, and adding a new variant after messages flow is exactly what fixing the envelope now avoids.
- **`ProtocolError` as a broad error channel including tool errors**: rejected for conflating transport health with application errors and bypassing `request_id` correlation.
- **Hand-written conformance fixtures on both sides**: rejected for reintroducing at the value layer the drift ADR 0011 rejects at the type layer.

## Consequences

- The nine-message set, the framing, and the envelope are the contract both sides implement in step 3. Adding a message is a new enum variant plus a regenerated `fixtures.bin`. Changing the envelope or framing is a protocol revision bump (the `u32` from the Handshake decision) and a coordinated Core+host rebuild.
- `request_id` correlation is the pattern every future request/response message inherits. Phase 3's `ToolCall`/`ToolResult` use it directly.
- The 5 s / 3-miss heartbeat threshold is a tunable, not a constant of nature. If the dogfood checkpoint (ADR 0007) finds 15 s detection too slow in practice, this ADR is the place that threshold is recorded and the place a superseding change would be noted.
- `fixtures.bin` is a second generated artifact alongside the `.ts` files. Both share the same freshness-check discipline.
