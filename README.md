# pi-rs

[![Created with pi](https://img.shields.io/badge/created%20with-pi-blueviolet)](https://github.com/badlogic/pi-mono)
![Created with GLM5.2](https://img.shields.io/badge/created%20with-GLM5.2-00b4ab)

A Rust rewrite of the [pi coding agent](https://github.com/badlogic/pi-mono): native core for the terminal UI and agent loop, full compatibility with existing TypeScript pi extensions via a separate Extension Host process.

**Status: planning.** See [CONTEXT.md](./CONTEXT.md) for the domain language and [docs/adr/](./docs/adr/) for architectural decisions.

## Why

- Terminal rendering and input latency in a native core (cell-diff rendering, synchronized output/DEC 2026, microsecond frames)
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
