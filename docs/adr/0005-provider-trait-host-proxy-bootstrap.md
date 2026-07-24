# Provider trait with host-proxy bootstrap, Rust-native majors as destination

## Status

partially superseded by ADR 0019: the Provider trait and the per-API-type implementation order stand, the host-proxy bootstrap phase is dropped. The host-provider slot remains for extension-registered custom providers only

The Core defines a Provider trait from day one. Its first implementation is a host-proxy that streams through pi-ai in the Extension Host (fastest path to a working end-to-end system, 100% provider and auth compat). Native Rust implementations then land one at a time - implemented per API type, not per brand, mirroring pi's own model (`api: "openai-completions"` etc. with baseUrl + compat flags) [[1]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md): `openai-completions` first (broadest coverage per implementation - Ollama, vLLM, OpenRouter, proxies, local servers), `anthropic-messages` second (daily driver, OAuth), then `openai-responses` and `google-generative-ai`. The proxy path remains permanently as the compatibility slot for custom/exotic TS providers.

## Considered Options

- All providers in the host forever - rejected: credentials would permanently live in the same process as third-party extensions (VS Code-style process.env exposure [[2]](https://safeguard.sh/resources/blog/vscode-extension-security-development-guide)), and the Core would die with the host
- All providers in Rust, no proxy - rejected: breaks pi's custom-provider TS API and blocks shipping on reimplementing every wire format and OAuth flow up front

## Consequences

- Credentials migrate out of the Extension Host as each native provider lands. The endgame is credential isolation extensions cannot read
- The Core owns the wire-format maintenance treadmill for native providers (streaming deltas, thinking blocks, cache headers, OAuth refresh)
- Token streaming over the proxy crosses IPC. Acceptable because provider traffic is network-bound, but the Host Protocol must handle high-frequency small messages efficiently
- Accepted bootstrap risk: while all providers are host-proxied, a dead host means no LLM and no hooks - the Core can only render and prompt for host restart (ADR 0009). Each native API type landing shrinks this window. Compat flags (supportsDeveloperRole, supportsReasoningEffort) must be honored for parity

## Sources

1. pi provider and API-type model (openai-completions, baseUrl, compat flags): https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md
2. VS Code extension host shares process.env with all extensions: https://safeguard.sh/resources/blog/vscode-extension-security-development-guide
