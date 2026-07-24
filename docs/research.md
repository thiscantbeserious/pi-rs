# Implementation research notes

Verified findings that de-risk or refine ADR decisions. Each entry names the ADR it informs. Update when new research lands. Findings that change a decision require a superseding ADR, not a silent edit.

## Deno Unix domain sockets - ADR 0006 ✅ de-risked

Deno's `node:net` compatibility supports Unix domain sockets via `net.createConnection()`/`net.createServer()` (the `fd` option is unsupported, which pi-rs does not need). The UDS transport works for both the Deno-first host and the Node fallback without divergence. ([Deno node:net docs](https://docs.deno.com/api/node/net/))

## Rust → TypeScript type generation - ADR 0011, choice open

- **ts-rs**: derive-macro (`#[derive(TS)]`), TypeScript-only, simple, well-established
- **specta**: type-introspection system, TS primary with other languages in progress, richer (function types via tauri-specta)

Working default: **ts-rs** for its simplicity - pi-protocol only needs message/DTO types, not function bindings. Revisit if the protocol needs more than plain types. ([ts-rs](https://docs.rs/ts-rs), [specta](https://docs.rs/specta))

## Tree-sitter grammar bundling - ADR 0010, options mapped

- **Helix**: compiles grammars from source at build time (needs a C compiler), fetches grammars separately
- **Zed**: compiles grammars to WASM, loaded at runtime - decouples grammar updates from the binary
- **ae-tree-sitter-bundle**: single crate bundling parsers with per-language Cargo features

Working default: static compilation via per-language grammar crates with Cargo features (agr-style simplicity, no runtime loading). Revisit toward the Zed WASM model only if grammar count/binary size becomes a problem.

## MessagePack in JS - ADR 0006, claim refined

V8's `JSON.parse` is heavily optimized. Msgpack does **not** win on raw text decode speed. ADR 0006's justification is binary-safety (ANSI/UTF-8 blobs without escaping) and payload size - which holds. Benchmark `@msgpack/msgpack` vs `msgpackr` at protocol bring-up. Codec stays behind an interface (pitfall P17).

## Deno per-extension sandboxing - ADR 0002 ✅ path verified, re-timed

Deno permissions are per-process, but Workers accept scoped permissions (`WorkerOptions.deno.permissions`: inherit/none/specific paths+hosts, never exceeding the parent). Per-extension sandboxing = one worker per extension - possible, but adds worker-context compat risk, so it is post-parity. The v1 host runs process-level union permissions (still stronger than VS Code, which research confirms runs all extensions unsandboxed with full system access in one process). ([WorkerOptions.deno](https://docs.deno.com/api/web/~/WorkerOptions.deno), [VS Code ext security](https://safeguard.sh/resources/blog/vscode-extension-security-development-guide))

## Windows named pipes in Deno - ADR 0014 ✅ deferral validated

Deno 2.7 added node:net named-pipe support ([PR #31624](https://github.com/denoland/deno/pull/31624)), but an active regression leaves clients hanging after the first disconnect ([#33366](https://github.com/denoland/deno/issues/33366)). Native Windows stays post-parity. Re-check this issue when it starts.

## Render thread + tokio precedent - ADR 0013 ✅ de-risked

The dedicated-render-thread-fed-by-channels pattern is standard (tokio channels tutorial, Bevy's pipelined rendering uses exactly this main/render thread split). No unusual constraints found.

## pi's provider model is API-type based - ADR 0005 refined

pi providers are (baseUrl + `api` type + auth), where `api: "openai-completions"` is documented as "most compatible" and covers Ollama, vLLM, SGLang, OpenRouter, proxies, and local servers, with `compat` flags (supportsDeveloperRole, supportsReasoningEffort). Native Rust providers are therefore implemented per API type: openai-completions first (broadest coverage), anthropic-messages second, then openai-responses and google-generative-ai. (pi docs/models.md, docs/custom-provider.md)

## Terminal technique research - feeds docs/pitfalls.md P12–P16

Synchronized-output support querying (`CSI ? 2026 $ p`), tmux buffering/leak behavior, grapheme-cluster width chaos, kitty keyboard protocol suspend edge cases, panic-restore discipline, crossterm version unification. See the pitfalls table for guards.
