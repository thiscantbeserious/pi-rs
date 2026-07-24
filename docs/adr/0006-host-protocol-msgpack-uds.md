# Host Protocol: length-prefixed MessagePack over Unix domain sockets

The Host Protocol uses length-prefixed MessagePack frames over a Unix domain socket (named pipe on Windows). Binary encoding keeps large payloads (tool output, extension UI frame buffers, ANSI/UTF-8 blobs) cheap to move - no JSON string escaping - and a socket keeps the transport independent of the child's stdio, allowing hosts to be spawned or attached (future remote hosts). A --protocol-json debug flag provides human-readable tracing.

## Considered Options

- JSON-RPC over stdio ([LSP-style](https://microsoft.github.io/language-server-protocol/specifications/base/0.9/specification/)) - rejected: JSON-escaping large ANSI/binary payloads wastes CPU. Stdio doubles as the child's stdout, so any stray console.log corrupts the stream
- Shared memory ring + control channel - rejected for v1: overkill for ≤10KB@60Hz payloads, poor portability across Deno/Node, permanent debugging tax. Kept as a future escape hatch

## Consequences

- Both sides use first-class libraries (rmp-serde in the Core, @msgpack/msgpack in the host). UDS works in both host runtimes ([Deno node:net compatibility](https://docs.deno.com/api/node/net/))
- Protocol messages must stay runtime-neutral per ADR 0002 (no engine-specific types)
- Windows support requires a named-pipe transport variant
