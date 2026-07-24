# Cargo workspace with a single-source-of-truth, codegen'd Host Protocol

The repo is a Cargo workspace — pi-core (TUI, renderer, agent loop), pi-protocol (Host Protocol message types), pi-replay (session replay harness over the JSONL corpus, ADR 0007) — plus host/ containing the TypeScript Extension Host in the same repo. Protocol messages are defined once in Rust (pi-protocol) and TypeScript definitions are generated from them, so schema drift between Core and host is a build failure, not a runtime surprise.

## Considered Options

- Single crate until it hurts — rejected: the protocol single-source-of-truth question would get answered implicitly by the first hand-written host message
- Separate core and host repos — rejected: protocol changes would become cross-repo lockstep PRs during the phase of fastest protocol churn

## Consequences

- CI grows a Deno lane (lint, test, protocol conformance) when host/ lands
- The msgpack wire layer (ADR 0006) is exercised by conformance tests generated from the same definitions
- Workspace migration of the current single-crate skeleton is mechanical and should happen with the first real module
