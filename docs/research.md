# Implementation research notes

Verified findings that de-risk or refine ADR decisions. Each entry names the ADR it informs. Update when new research lands; findings that change a decision require a superseding ADR, not a silent edit.

## Deno Unix domain sockets — ADR 0006 ✅ de-risked

Deno's `node:net` compatibility supports Unix domain sockets via `net.createConnection()`/`net.createServer()` (the `fd` option is unsupported, which pi-rs does not need). The UDS transport works for both the Deno-first host and the Node fallback without divergence. ([Deno node:net docs](https://docs.deno.com/api/node/net/))

## Rust → TypeScript type generation — ADR 0011, choice open

- **ts-rs**: derive-macro (`#[derive(TS)]`), TypeScript-only, simple, well-established
- **specta**: type-introspection system, TS primary with other languages in progress, richer (function types via tauri-specta)

Working default: **ts-rs** for its simplicity — pi-protocol only needs message/DTO types, not function bindings. Revisit if the protocol needs more than plain types. ([ts-rs](https://docs.rs/ts-rs), [specta](https://docs.rs/specta))

## Tree-sitter grammar bundling — ADR 0010, options mapped

- **Helix**: compiles grammars from source at build time (needs a C compiler), fetches grammars separately
- **Zed**: compiles grammars to WASM, loaded at runtime — decouples grammar updates from the binary
- **ae-tree-sitter-bundle**: single crate bundling parsers with per-language Cargo features

Working default: static compilation via per-language grammar crates with Cargo features (agr-style simplicity, no runtime loading). Revisit toward the Zed WASM model only if grammar count/binary size becomes a problem.

## MessagePack in JS — ADR 0006, claim refined

V8's `JSON.parse` is heavily optimized; msgpack does **not** win on raw text decode speed. ADR 0006's justification is binary-safety (ANSI/UTF-8 blobs without escaping) and payload size — which holds. Benchmark `@msgpack/msgpack` vs `msgpackr` at protocol bring-up; codec stays behind an interface (pitfall P17).

## Terminal technique research — feeds docs/pitfalls.md P12–P16

Synchronized-output support querying (`CSI ? 2026 $ p`), tmux buffering/leak behavior, grapheme-cluster width chaos, kitty keyboard protocol suspend edge cases, panic-restore discipline, crossterm version unification. See the pitfalls table for guards.
