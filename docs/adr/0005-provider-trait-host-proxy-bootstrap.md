# Provider trait with host-proxy bootstrap; Rust-native majors as destination

The Core defines a Provider trait from day one. Its first implementation is a host-proxy that streams through pi-ai in the Extension Host (fastest path to a working end-to-end system, 100% provider and auth compat). Major providers (Anthropic first, then OpenAI, Gemini, Bedrock) are then moved into native Rust implementations one at a time; the proxy path remains permanently as the compatibility slot for custom/exotic TS providers.

## Considered Options

- All providers in the host forever — rejected: credentials would permanently live in the same process as third-party extensions (VS Code-style process.env exposure), and the Core would die with the host
- All providers in Rust, no proxy — rejected: breaks pi's custom-provider TS API and blocks shipping on reimplementing every wire format and OAuth flow up front

## Consequences

- Credentials migrate out of the Extension Host as each native provider lands; the endgame is credential isolation extensions cannot read
- The Core owns the wire-format maintenance treadmill for native providers (streaming deltas, thinking blocks, cache headers, OAuth refresh)
- Token streaming over the proxy crosses IPC; acceptable because provider traffic is network-bound, but the Host Protocol must handle high-frequency small messages efficiently
