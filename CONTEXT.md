# pi-rs

A Rust rewrite of the pi coding agent: a native core for the terminal UI and agent loop, with compatibility for existing TypeScript pi extensions.

## Language

**Core**:
The Rust binary that owns the terminal UI, rendering, agent loop, and provider communication.
_Avoid_: engine, backend, main process

**Extension Host**:
The separate JavaScript runtime process in which pi extensions execute, isolated from the Core.
_Avoid_: sidecar, plugin runner, JS process

**Extension**:
A TypeScript module written against the existing pi extension API (tools, commands, events, UI components), running unmodified in the Extension Host.
_Avoid_: plugin, addon

**Host Protocol**:
The runtime-agnostic IPC contract between the Core and an Extension Host. Any conforming JavaScript runtime can implement it.
_Avoid_: bridge, RPC layer (as names for the contract itself)

**Retained Message Model**:
The Core-owned in-memory representation of the whole conversation from which every frame is rendered.
_Avoid_: scrollback buffer, history cache

**Render Thread**:
The dedicated synchronous thread that owns the terminal and produces frames. It never waits on anything.
_Avoid_: UI thread, main thread

**Provider**:
A Rust-native implementation of one of pi's four API types in the Core's LLM streaming interface.
_Avoid_: backend, model adapter

**Host Provider Slot**:
The protocol slot through which extension-registered custom providers stream, the only provider path that touches the Extension Host.
_Avoid_: host proxy, passthrough

**Oracle**:
The pinned pi version whose behavior, tests, and formats define parity.
_Avoid_: reference version, upstream

**Session Corpus**:
The collection of real-world pi session files used for replay-based parity testing.
_Avoid_: golden files, test sessions

**Dogfood Checkpoint**:
The milestone at which the author's daily work switches to pi-rs, opening the UX validation loop.
_Avoid_: beta, MVP
