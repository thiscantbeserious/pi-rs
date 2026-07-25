# Providers are Rust-native from day one, the host is strictly extensions-only

Supersedes the bootstrap phase of ADR 0005. No pi core functionality ever runs in the Extension Host: the Core implements pi's API types natively in Rust. The Oracle (`packages/ai/src/types.ts` at the pinned version, [ADR 0007](./0007-oracle-guided-full-parity.md)) defines ten `KnownApi` values, each with its own options type and implementation module under `packages/ai/src/api/` [[1]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md):

1. `openai-completions` (daily driver, broadest compat: Ollama, vLLM, OpenRouter, proxies, local servers)
2. `anthropic-messages` (daily driver, OAuth, the author's real auth path)
3. `openai-responses`
4. `google-generative-ai`
5. `mistral-conversations`
6. `azure-openai-responses`
7. `openai-codex-responses`
8. `bedrock-converse-stream`
9. `google-vertex`
10. `pi-messages` (pi's own unified internal API)

The four daily-driver types (1-4) land first in priority order; the remaining six are full-parity work tracked in ROADMAP Phase 4. The host-provider slot in the protocol survives solely for extension-registered custom providers [[2]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/custom-provider.md), which is extension support and therefore belongs in the host by definition.

> Corrected 2026-07-25 by `docs/oracle-drift-audit.md` finding D2: this ADR previously claimed "four API types" cover the catalog. The Oracle has ten `KnownApi`. The four-type priority order stands; only the parity total was wrong.

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
- OAuth flows (Anthropic subscription) are Rust work from the start and must be scheduled with anthropic-messages in Phase 3: dogfooding on API keys would not exercise the author's real auth path
- Auth storage interops with pi's auth.json bidirectionally during dogfood (same spirit as ADR 0008 for sessions): switching tools mid-day must not require re-login

## Sources

1. pi models.json, four API types cover the catalog: <https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md>
2. pi custom providers, registered by extensions: <https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/custom-provider.md>
