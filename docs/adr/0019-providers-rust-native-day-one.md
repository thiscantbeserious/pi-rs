# Providers are Rust-native from day one, the host is strictly extensions-only

Supersedes the bootstrap phase of ADR 0005. No pi core functionality ever runs in the Extension Host: the Core implements pi's API types natively in Rust, openai-completions and anthropic-messages first (the daily drivers), openai-responses and google-generative-ai after. This is tractable because pi's entire provider catalog rests on just these four API types [[1]](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md). The host-provider slot in the protocol survives solely for extension-registered custom providers [[2]](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/custom-provider.md), which is extension support and therefore belongs in the host by definition.

## Status

accepted, supersedes the host-proxy bootstrap in ADR 0005

## Considered Options

- Keep the pi-ai proxy bootstrap (ADR 0005) - rejected: core functionality temporarily living in the TS host contradicts the project's premise that the host exists for extension support only. The original justification (avoiding N wire formats up front) dissolved once the API-type model reduced N to four
- Hybrid (two native types, proxy the rest) - rejected: retains the impurity for marginal schedule gain

## Consequences

- The ADR 0002 spike no longer needs to prove pi-ai streams under Deno. The gate reduces to extension corpus loading and registration, including custom-provider extensions
- The bootstrap-brick risk documented in ADR 0005 disappears: a dead host never takes LLM streaming down, only extensions
- Credentials never enter the Extension Host for built-in API types, from the first commit
- First end-to-end chat requires the first native API type to land (openai-completions, testable against local Ollama without cost)
- OAuth flows (Anthropic subscription) are Rust work from the start and must be scheduled with anthropic-messages

## Sources

1. pi models.json, four API types cover the catalog: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md
2. pi custom providers, registered by extensions: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/custom-provider.md
