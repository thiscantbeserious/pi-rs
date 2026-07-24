# pi-rs

[![Created with pi](https://img.shields.io/badge/created%20with-pi-blueviolet)](https://github.com/badlogic/pi-mono)
![Created with GLM5.2](https://img.shields.io/badge/created%20with-GLM5.2-00b4ab)

A Rust rewrite of the [pi coding agent](https://github.com/badlogic/pi-mono): native core for the terminal UI and agent loop, full compatibility with existing TypeScript pi extensions via a separate Extension Host process.

**Status: planning.** See [CONTEXT.md](./CONTEXT.md) for the domain language and [docs/adr/](./docs/adr/) for architectural decisions.

## Why

Today's agent TUIs leave rendering quality on the table, each for a different reason. Claude Code (Ink/React) redraws the full viewport on state changes — the documented flicker in multiplexers, typing lag, and exit corruption are architectural, not incidental. Codex CLI is Rust/ratatui and still ships unstable scrollback and platform-dependent rendering — native code is necessary but not sufficient. pi does better with differential line-based rendering, but remains bounded by its JavaScript runtime and line-granular diffing. [agent-session-recorder](https://github.com/thiscantbeserious/agent-session-recorder) demonstrated what the alternative feels like: a Rust render loop with cell-level diffing, synchronized output, and partial line updates draws in microseconds. pi-rs applies that render architecture to a full coding agent, without giving up pi's extension ecosystem.

- Native render core: cell-diff frames, synchronized output, latency isolated from extension code
- Existing pi extensions run unmodified in an Extension Host (VS Code-style architecture)
- Runtime-agnostic Host Protocol: Deno-first (permissions sandbox), Node fallback

## Decisions so far

- [ADR 0001](./docs/adr/0001-extension-host-process.md) — Extensions run in a separate Extension Host process
- [ADR 0002](./docs/adr/0002-host-protocol-deno-first.md) — Runtime-agnostic Host Protocol; Deno host first, Node as fallback
- [ADR 0003](./docs/adr/0003-retained-frame-buffers-for-extension-ui.md) — Extension UI crosses the Host Protocol as retained frame buffers
- [ADR 0004](./docs/adr/0004-alternate-screen-retained-message-model.md) — Alternate screen with a retained message model
- [ADR 0005](./docs/adr/0005-provider-trait-host-proxy-bootstrap.md) — Provider trait with host-proxy bootstrap; Rust-native majors as destination
- [ADR 0006](./docs/adr/0006-host-protocol-msgpack-uds.md) — Host Protocol: length-prefixed MessagePack over Unix domain sockets
- [ADR 0007](./docs/adr/0007-oracle-guided-full-parity.md) — V1 bar is full parity, oracle-guided and measured by differential replay
- [ADR 0008](./docs/adr/0008-native-pi-session-format.md) — Sessions use pi's native format, bidirectionally
- [ADR 0009](./docs/adr/0009-hook-heartbeat-fail-closed.md) — Tool-call hooks: unbounded await, heartbeat liveness, fail-closed
- [ADR 0010](./docs/adr/0010-streaming-markdown-pipeline.md) — Streaming markdown pipeline: pulldown-cmark structure, tree-sitter highlighting
- [ADR 0011](./docs/adr/0011-workspace-generated-protocol.md) — Cargo workspace with a single-source-of-truth, codegen'd Host Protocol
- [ADR 0012](./docs/adr/0012-native-pi-themes-capture-mapping.md) — Themes use pi's native JSON format with a tree-sitter capture mapping
