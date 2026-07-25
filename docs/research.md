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

## MessagePack in JS - ADR 0006, benchmarked (Phase 1 step 4)

Benchmarked `@msgpack/msgpack` 3.1.3 vs `msgpackr` 2.0.4 on the actual Phase 1 protocol payload mix (small control messages + a 1 MiB binary tool-output payload), Deno 2.9.4, 200 warmup + 2000 iterations (50 for the 1 MiB payload), synchronous codec operations (both libs are sync; async wrappers were removed because their per-call Promise overhead diluted the small-message differences the benchmark exists to measure). Geomean combined encode+decode ratio (@msgpack/msgpack / msgpackr): **1.69x**. Within the 2x trigger threshold, so ADR 0006's default (`@msgpack/msgpack`) holds. The codec-swap trigger did not fire.

Per-payload results (combined encode+decode ratio, >1 means @msgpack/msgpack is slower):

| Payload | Ratio | Notes |
| --- | --- | --- |
| Handshake | ~1.5x | small control message |
| Heartbeat | ~1.5x | small control message |
| Shutdown | ~1.5x | small control message |
| EchoRequest-small | ~1.3x | small with binary payload |
| ProtocolError | ~1.7x | small with string message |
| EchoRequest-1MiB-binary | 0.83x | msgpackr faster on large binary decode (2.86x), @msgpack/msgpack faster on encode |

Notable: on the 1 MiB binary payload, msgpackr decodes ~2.9x faster than @msgpack/msgpack (its native-acceleration path), while @msgpack/msgpack is faster on encode. On small messages msgpackr wins by ~1.3-1.7x. This refines P17's claim: the msgpack win is binary-safety and payload size, but msgpackr's native acceleration makes the large-binary-decode case closer than expected. The codec stays behind the `Codec` interface in `host/codec.ts` so a future swap is one line, not a protocol change.

Full methodology and the decision threshold live in `docs/plans/step-4-host-codec-benchmark.md`; the benchmark is `host/codec_bench.ts`.

V8's `JSON.parse` is heavily optimized. Msgpack does **not** win on raw text decode speed. ADR 0006's justification is binary-safety (ANSI/UTF-8 blobs without escaping) and payload size - which holds. Codec stays behind an interface (pitfall P17).

## Deno per-extension sandboxing - ADR 0002 ✅ path verified, re-timed

Deno permissions are per-process, but Workers accept scoped permissions (`WorkerOptions.deno.permissions`: inherit/none/specific paths+hosts, never exceeding the parent). Per-extension sandboxing = one worker per extension - possible, but adds worker-context compat risk, so it is post-parity. The v1 host runs process-level union permissions (still stronger than VS Code, which research confirms runs all extensions unsandboxed with full system access in one process). ([WorkerOptions.deno](https://docs.deno.com/api/web/~/WorkerOptions.deno), [VS Code ext security](https://safeguard.sh/resources/blog/vscode-extension-security-development-guide))

## Windows named pipes in Deno - ADR 0014 ✅ deferral validated

Deno 2.7 added node:net named-pipe support ([PR #31624](https://github.com/denoland/deno/pull/31624)), but an active regression leaves clients hanging after the first disconnect ([#33366](https://github.com/denoland/deno/issues/33366)). Native Windows stays post-parity. Re-check this issue when it starts.

## Render thread + tokio precedent - ADR 0013 ✅ de-risked

The dedicated-render-thread-fed-by-channels pattern is standard (tokio channels tutorial, Bevy's pipelined rendering uses exactly this main/render thread split). No unusual constraints found.

## pi's provider model is API-type based - ADR 0005 refined

pi providers are (baseUrl + `api` type + auth), where `api: "openai-completions"` is documented as "most compatible" and covers Ollama, vLLM, SGLang, OpenRouter, proxies, and local servers, with `compat` flags (supportsDeveloperRole, supportsReasoningEffort). Native Rust providers are therefore implemented per API type: openai-completions first (broadest coverage), anthropic-messages second, then openai-responses and google-generative-ai. (pi docs/models.md, docs/custom-provider.md)

## Local first-hand verifications (author's machine, 2026-07-24)

Stronger than web sources where they apply, per the sourced-facts rule:

- Real pi session JSONL inspected: header entry (cwd, id, timestamp, type, version), subsequent entries carry id + parentId. Confirms ADR 0008's tree premise against actual data
- session-format.md, models.md, providers.md, custom-provider.md all ship inside the installed npm package: the Oracle's specs are local at the pin
- dist/*.d.ts type declarations present in the installed package: Phase 0 API-surface extraction runs against the pinned package itself
- ~/.pi/agent/auth.json exists with an anthropic entry: the OAuth dogfood path (ADR 0019) is the author's real daily auth
- Shared tree confirmed live: settings.json, local-models.json, models-store.json, keybindings.json, sessions/ per-directory layout (ADR 0020)
- Corrections found: screenpipe was NOT running at check time (ROADMAP claim adjusted), Ollama not installed (now an explicit Phase 3 test dependency)

## Terminal technique research - feeds docs/pitfalls.md P12–P16

Synchronized-output support querying (`CSI ? 2026 $ p`), tmux buffering/leak behavior, grapheme-cluster width chaos, kitty keyboard protocol suspend edge cases, panic-restore discipline, crossterm version unification. See the pitfalls table for guards.
