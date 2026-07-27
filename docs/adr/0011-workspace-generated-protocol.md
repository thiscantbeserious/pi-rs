# Cargo workspace with a single-source-of-truth, codegen'd Host Protocol

**📝 AMENDED BY [ADR 0026](./0026-phase-2-render-subsystem-pi-render-crate.md):** the renderer moved from `pi-core` to a new `pi-render` crate. `pi-core` becomes "agent loop, host supervision, session writing, the render-thread lifecycle owner." The pi-session amendment below also stands.

---

The repo is a Cargo workspace - pi-core (TUI, renderer, agent loop), pi-protocol (Host Protocol message types), pi-replay (session replay harness over the JSONL corpus, ADR 0007), pi-session (pi's on-disk session format: header, entry types, migrations, naming, context reconstruction, ADR 0008) - plus host/ containing the TypeScript Extension Host in the same repo. Protocol messages are defined once in Rust (pi-protocol) and TypeScript definitions are generated from them, so schema drift between Core and host is a build failure, not a runtime surprise.

## Amendment 2026-07-25: pi-session crate

Added a fifth crate, `crates/pi-session`, holding the typed session-format contract (`docs/session-format-contract.md`). Both `pi-core` (sole session writer, ADR 0016) and `pi-replay` (replay reader) depend on it. The session format is a parity contract with the pinned Oracle (ADR 0008), not a pi-rs-native artifact, so it does not belong in `pi-protocol` (which is the Host Protocol wire, ADR 0022) and must not be duplicated across `pi-core` and `pi-replay`. A shared crate makes the format a single source of truth on the Rust side, mirroring how `pi-protocol` is the single source for wire types.

Chosen over putting the types in `pi-replay` with `pi-core` depending on it: that pulls a 'replay' crate into the runtime core dependency graph, blurring the boundary ADR 0011 warns about (the agent loop must not depend on the renderer; by the same logic the writer must not depend on the replay harness). Chosen over putting the types in `pi-core` with `pi-replay` depending on it: `pi-core` is the TUI/agent-loop crate, not a format library, and `pi-replay` depending on `pi-core` drags the whole runtime into the replay tool. A dedicated crate is the honest boundary.

## Considered Options

- Single crate until it hurts - rejected: the protocol single-source-of-truth question would get answered implicitly by the first hand-written host message
- Separate core and host repos - rejected: protocol changes would become cross-repo lockstep PRs during the phase of fastest protocol churn

## Consequences

- CI grows a Deno lane (lint, test, protocol conformance) when host/ lands
- The msgpack wire layer (ADR 0006) is exercised by conformance tests generated from the same definitions
- Workspace migration of the current single-crate skeleton is mechanical and should happen with the first real module
- Within pi-core, the agent loop must not depend on the renderer: headless mode (ADR 0018) requires the loop to run with no TUI attached - enforce the boundary from the first commit, split into a separate crate if it blurs
