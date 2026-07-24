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
