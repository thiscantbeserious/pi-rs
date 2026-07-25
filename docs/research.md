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

Benchmarked `@msgpack/msgpack` 3.1.3 vs `msgpackr` 2.0.4 on the actual Phase 1 protocol payload mix (small control messages + a 1 MiB binary tool-output payload), Deno 2.9.4, 200 warmup + 2000 iterations (50 for the 1 MiB payload), synchronous codec operations (both libs are sync; async wrappers were removed because their per-call Promise overhead diluted the small-message differences the benchmark exists to measure).

**Decision: the host codec is `@msgpack/msgpack` (ADR 0006 default).** The codec-swap trigger did not fire.

Geomean combined encode+decode ratio (`@msgpack/msgpack` / `msgpackr`) measured consistently in the **1.7x-1.8x range across runs**, under the 2x trigger threshold. The benchmark has run-to-run variance (JIT, GC, system load) high enough that exact per-payload numbers are not stable between runs; the geomean is consistently under 2x across multiple runs, which is what the decision rests on.

Observed per-run pattern: msgpackr is faster on small messages by ~1.3x-2.7x (varies by run). The 1 MiB binary decode case is the most volatile: msgpackr's native-acceleration decode path sometimes beats `@msgpack/msgpack` by ~2.9x, sometimes `@msgpack/msgpack` wins. This refines P17: the msgpack win is binary-safety and payload size, but msgpackr's native acceleration makes the large-binary-decode case volatile and closest to the 2x threshold. Re-benchmark if the protocol payload mix shifts toward large binary frames.

The codec stays behind the `Codec` interface in `host/codec.ts` so a future swap is one line, not a protocol change. Full methodology and the decision threshold live in `docs/plans/step-4-host-codec-benchmark.md`; the benchmark is `host/codec_bench.ts`.

V8's `JSON.parse` is heavily optimized. Msgpack does **not** win on raw text decode speed. ADR 0006's justification is binary-safety (ANSI/UTF-8 blobs without escaping) and payload size - which holds. Codec stays behind an interface (pitfall P17).

## Deno compile for the host binary - ADR 0021, Phase 1 step 5 ✅ verified

`deno compile` produces a standalone binary from the Phase 1 host entrypoint (codec + framing + protocol types, no full pi runtime yet). Verified: compiles clean, runs standalone, encodes a Heartbeat in 16 bytes. The Core spawns this binary with `PI_RS_HOST_SOCKET` in env, making the Deno dependency build-time (cannot be forgotten at deploy, unlike `deno run` relying on `$PATH`).

Known future cost: when Phase 3 imports the full pi runtime (AWS SDK, Google genai, OpenAI, Anthropic, MCP SDK), the compile step and binary size will grow dramatically. Re-evaluate at Phase 3 against ADR 0014/0020's distribution story. ([deno compile docs](https://docs.deno.com/runtime/reference/cli/compile/))

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
