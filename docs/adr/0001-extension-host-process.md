# Extensions run in a separate Extension Host process

Existing pi extensions are arbitrary TypeScript with deep Node ecosystem dependencies (node:fs, node:stream, undici, @aws-sdk, zod). Compiling them to native code via WASM is not feasible (no production-grade TS→WASM compiler for arbitrary code), and embedding a JS engine in the Rust binary requires re-implementing large parts of the Node API surface. We instead run extensions unmodified in a separate JavaScript runtime process (the Extension Host) that talks to the Rust Core over IPC - the same architecture VS Code uses for its extension host.

## Considered Options

- Embed deno_core/QuickJS in the binary - rejected: months of Node-compat integration work, undici/aws-sdk edge cases
- Embed libnode - rejected: painful C++ embedding, two event loops in one process, huge binary
- WASM-compiled extensions (QuickJS-in-wasmtime or TS→WASM AOT) - rejected: not native speed, ~70-85% compat at best, async/npm bridging pain
- New Rust/WASM-native plugin ABI only - rejected: abandons compatibility with existing extensions

## Consequences

- 100% extension compatibility, crash isolation, hot-reload by restarting the host
- ctx.ui and event-interception calls cross an IPC boundary. Hot paths (per-token rendering, tool_call hooks) must be designed to avoid chatty round-trips
- The Core's performance (rendering, diffing, agent loop) is independent of extension behavior
