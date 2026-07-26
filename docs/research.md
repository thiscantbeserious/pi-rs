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

pi providers are (baseUrl + `api` type + auth), where `api: "openai-completions"` is documented as "most compatible" and covers Ollama, vLLM, SGLang, OpenRouter, proxies, and local servers, with `compat` flags (supportsDeveloperRole, supportsReasoningEffort). The Oracle (`packages/ai/src/types.ts`) defines ten `KnownApi` values: `openai-completions`, `mistral-conversations`, `openai-responses`, `azure-openai-responses`, `openai-codex-responses`, `anthropic-messages`, `bedrock-converse-stream`, `google-generative-ai`, `google-vertex`, `pi-messages`. Native Rust providers are therefore implemented per API type: the four daily drivers land first (`openai-completions` broadest coverage, `anthropic-messages` incl. Claude Pro/Max OAuth, then `openai-responses` and `google-generative-ai`); the remaining six are full-parity work (ROADMAP Phase 4). See `docs/oracle-drift-audit.md` finding D2. (pi docs/models.md, docs/custom-provider.md)

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

## Terminal backend - ratatui on crossterm (ADR 0024) ✅ verified 2026-07-26

Verified against current docs (find-docs/ctx7, not training data):

- **crossterm exposes mode 2026 + kitty keyboard directly.** `SynchronizedUpdate`/`sync_update` wraps a block of writes in BSU/ESU ([crossterm event.rs](https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs)). `PushKeyboardEnhancementFlags` pushes the kitty protocol flags (DISAMBIGUATE_ESCAPE_CODES, REPORT_EVENT_TYPES, REPORT_ALTERNATE_KEYS, REPORT_ALL_KEYS_AS_ESCAPE_CODES); supported terminals listed as kitty, foot, WezTerm, alacritty. `Event` enum covers Key (with kind/state for press/release), Mouse, Paste, Resize, FocusGained/Lost. No IME/preedit variant (a known limitation, not a Phase 2 blocker).
- **ratatui's `Buffer::diff` is the cell-diff ADR 0004/P9 require**, with explicit multi-width handling: a double-width `コ` at index 0 skips index 1; a double-width at index 1 skips index 2 ([ratatui Buffer](https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html)). `Terminal::flush` computes the diff and passes it to the backend's `draw`. `draw`/`flush`/`resize`/`autoresize` are synchronous `io::Result` (no `async`), so ADR 0013's render-thread-never-awaits holds.
- **ratatui's `CellWidth` is `unicode-width`-based (per-codepoint)**, the P13 failure class, with no override hook and open ZWJ emoji bugs (#75, #925, #1332; PR #1089 improved truncation but did not fix the width mapping). Width is therefore owned at the projection layer (ADR 0025), not delegated to ratatui. `Terminal::backend_mut()` reaches the raw crossterm backend for direct commands (mode 2026 wrap, kitty keyboard).
- **Codex failure class (P5 unstable scrollback, P7 platform-dependent rendering) is not inherited by adopting ratatui.** P5's guard is RMM-owned scroll state (a model property); P7's guard is the CI terminal matrix. Both are independent of the backend choice.

## Grapheme-cluster width crate - runefix-core (ADR 0025) ✅ verified 2026-07-26

July 2026 maintenance status verified per-crate against crates.io (not training data). The grapheme-aware width crate sits behind the `GraphemeWidth` trait (ADR 0025), so this choice is reversible in one impl.

| Crate | Latest | Last activity | Unicode | Grapheme-aware? | crates.io? | Deps |
| --- | --- | --- | --- | --- | --- | --- |
| **runefix-core** (chosen) | 0.1.10 | 11 releases since May 2025 (active) | UAX#29 + curated emoji/CJK tables | YES (`graphemes()` + `atoms()`) | YES | zero |
| unicode-display-width | 0.3.0 | Oct 2023 (~3y stale) | 15.1.0 | YES | YES | — |
| unicode-width | 0.2.2 | ~Sept 2025 (active) | UAX#11 | NO (per-codepoint) | YES | zero |
| grapheme-width-rs | n/a | Aug 2023 (~3y stale) | ~14 | YES | NO (unpublished, 6 stars) | ucd-tri |
| widecharwidth (Helix) | generated | — | generated | NO (per-codepoint) | NO (vendored .rs) | zero |

**Decision: `runefix-core`** ([crates.io](https://crates.io/crates/runefix-core), [docs](https://docs.rs/runefix-core)). It is the only option both grapheme-cluster-aware AND actively maintained. Purpose-built for terminal CLI alignment, Markdown table rendering, and TUI layout engines. Provides `graphemes()` (UAX #29 compliant) and `atoms()` (width-driven segmentation that groups ZWJ/VS with their base, optimized for TUI layout). Runtime `WidthPolicy` (terminal: emoji=2/CJK=2, markdown: emoji=1/CJK=2, compact: all=1). Reproducible tables from [char-table](https://github.com/runefix-labs/char-table). Width values {1, 2}. MSRV 1.85. MIT/Apache-2.0. Zero dependencies (no P16 risk).

Honesty note (PHILOSOPHY §9): runefix-core is young (first release May 2025, 11 versions in 14 months, single author/org runefix-labs), not as battle-tested as unicode-width (19 versions since 2015, unicode-rs org). Chosen because it is the only crate that solves P13; the `GraphemeWidth` trait makes a swap one impl if it goes unmaintained. unicode-display-width is the fallback if runefix-core stalls (grapheme-aware but ~3y stale, Unicode 15.1.0).

Rejected: every maintained per-codepoint option (unicode-width, widecharwidth) is the P13 failure class by design — widecharwidth's own README states "a wcwidth-style per-codepoint API is fundamentally limited when it comes to composing codepoints" ([widecharwidth](https://github.com/ridiculousfish/widecharwidth)). Helix uses widecharwidth + mode 2027 query and still has open emoji bugs (#6012, #15599). grapheme-width-rs is unpublished and stale.

## Tree-sitter grammar bundling (ADR 0010) ✅ verified 2026-07-26

Landscape verified July 2026. The bundling mechanism is reversible behind per-language Cargo features; the WASM fallback trigger (ROADMAP Phase 2) stays armed.

- **`tree-sitter`** crate: incremental parsing (`parse` with `old_tree`), query cursors with `next_capture` (the highlight iteration path), queries from S-expressions. The engine ADR 0010 calls for.
- **`tree-sitter-highlight`** (v0.25.4/0.26.9): the highlight layer. Takes a `HighlightConfiguration` (language + highlights query + injections + locals), a `highlight_names` array, produces styled captures. Maps onto the pi theme palette (ADR 0012).
- **Bundling options surveyed:**
  - Helix-style: compile grammars from source in `build.rs`, fetch separately. Battle-tested; binary size is real (Helix ~30 MiB, runtime grammars ~95 MiB extracted).
  - `ae-tree-sitter-bundle` (v0.1.0, Jun 2026): single crate, per-language Cargo features, static compile. Matches research.md's prior default shape, but 1 month old with no adoption data (PHILOSOPHY §9 sourcing risk).
  - `tree-sitter-natives`: monolithic, all official grammars, cross-platform static/shared archives. Heaviest.
  - `tree-sitter-language-pack`: layered core crate + remote manifest download at runtime. Rejected: runtime network dependency, violates offline TUI (ADR 0014).
  - Zed-style WASM: runtime loading, decouples grammar updates from binary. The armed fallback trigger, not the default.

**Decision: static per-language grammar crates behind Cargo features** (`tree-sitter-rust`, `tree-sitter-javascript`, etc.), starting with a minimal daily-driver set (rust, TS/JS, bash, json) and expanding to the Oracle's grammar set once verified. `tree-sitter-highlight` as the highlight layer regardless of bundling. Measure binary/build size; fire the WASM fallback trigger only if measured bloat (per ROADMAP Phase 2 armed trigger). Uses established grammar crates with years of adoption; no runtime network; matches research.md default. `ae-tree-sitter-bundle` rejected as too new/unsourced.

Open sub-item: the grammar set is a parity surface (which languages pi highlights). Needs an Oracle check against the pinned pi `v0.82.0` tree-sitter config before the set is finalized — a find-docs/librarian task, not a guess.

## Phase 2 render-surface decisions (ADRs 0024–0026) ✅ grilled 2026-07-26

Reversible implementation decisions settled in the Phase 2 grill. Recorded here per §9 rule 4 (research findings live in research.md); the hard-to-reverse architecture is in ADRs 0024–0026, the reversible crate/tool choices are here.

### D-E: minimal seed message type — `RenderMessage` in `pi-render`

`pi-session::SessionEntry::Message` carries `message: serde_json::Value` (opaque, per contract §10: "The message body is preserved as opaque JSON until `pi-messages` types land"). The Phase 2 exit gate (20MB replay) needs a typed view over that blob. Decision: the seed lives in `pi-render` as `RenderMessage`, parsed from the opaque `Value`, NOT in `pi-session` (which stays format-pure) and NOT as a new `pi-messages` crate (Phase 3 scope, explicitly deferred). Rationale: ADR 0026 puts the message-to-cell projection in `pi-render`; the message shape it projects from is part of that projection. Phase 3's `pi-messages` crate replaces the `Value`-parse with a typed projection under the passing test — a refactor, not a rewrite. `pi-replay` (depends on both `pi-session` and `pi-render` per ADR 0026) does the parse.

Minimal shape (verified against [`packages/ai/src/types.ts`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts) at v0.82.0): role (user/assistant/toolResult) + `TextContent` + `ThinkingContent` + `ToolCall` + `ToolResult`. `ImageContent` deferred to a placeholder (Phase 2 replays text sessions; terminal image rendering — sixel/kitty graphics — is its own problem, additive later). Each field cited to the Oracle with a permalink per §9.5.

### D-F: theme loading — typed `Theme` struct + `match` capture mapping

Pi's theme schema verified against the pinned Oracle ([`theme-schema.json`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/modes/interactive/theme/theme-schema.json) at v0.82.0) and confirmed live locally (`~/.pi/agent/themes/` holds 9 theme JSON files: catppuccin-mocha, dracula, gruvbox, nord, etc.; catppuccin-mocha sample confirms `vars` + `colors` referencing vars by name). The schema has a 9-key `syntax*` palette (`syntaxComment`, `syntaxKeyword`, `syntaxFunction`, `syntaxVariable`, `syntaxString`, `syntaxNumber`, `syntaxType`, `syntaxOperator`, `syntaxPunctuation`). Color values are four-variant: hex (`#RRGGBB`), var reference (from `vars`), empty string (terminal default), 256-color index (0–255).

Decision: load each theme into a typed `Theme` struct in `pi-render` (parse-don't-validate, PHILOSOPHY §4), tolerant of unknown keys for forward-compat (ADR 0020: "pi-rs must tolerate unknown keys and never rewrite the file destructively"). Themes are read-only in pi-rs (pi-rs reads `~/.pi/agent/themes/*.json`, never writes them). Var references preserved unresolved in the struct, resolved at render time. The capture→palette mapping is a `match` expression in one module (ADR 0012's "single place to tune"): tree-sitter's ~27 standard captures ([tree-sitter highlighting docs](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html)) → pi's 9 `syntax*` keys, plain-text fallback for unmapped captures. A data file was rejected (adds a load step and parity surface for a small stable table). Live theme switch is a `RenderEvent::ThemeChanged` that flushes the block cache (ADR 0010) and re-projects.

Parity details the loader must replicate: `name` must not contain `/` (reserved for light/dark); `thinkingMax` optional, falls back to `thinkingXhigh`; `export` section (HTML) tolerated but not a Phase 2 concern.

### D-G: exit-gate measurement — real tmux + regression suite + human sign-off

The Phase 2 exit gate ("20MB-class session replays at full speed inside tmux, zero visual artifacts" + "benchmarks green") is NOT fully automatable. Decision: the automatable regressions run in CI on the ubuntu-latest lane (tmux is pre-installed on GH Actions Linux runners); the transient visual quality is human-signed via a recorded replay (asciinema/ttyrec) on the gate PR, like Phase 3's dogfood evidence.

CI regression suite (the automatable "zero artifacts"): (a) P13 width corpus snapshot (cell-diff corruption from width drift); (b) balanced BSU/ESU pairs (every flush wrapped in synchronized output, no torn frames); (c) terminal-restore probe after exit (P15: alt screen left, raw mode off, kitty flags popped); (d) frame-time benchmark under budget; (e) no-panic soak on the 20MB replay; (f) final-frame insta snapshots of representative replay states.

Session source: synthetic 20MB generator in `pi-replay` (real `pi-session` entry types, realistic content: large bash outputs, file reads) for CI — reproducible, size-controllable; plus any real Corpus session from `~/.pi/agent/sessions` for the human sign-off if available.

Benchmark budgets: frame-time p99 < 16ms per coalesced frame (the 60fps budget from ADR 0013's ≥16ms coalescing; dropped frames are the failure). Input-latency p99 < 16ms (keystroke-to-frame; input read on the render thread per ADR 0013, so near-zero minus the frame budget). Baselines committed; criterion regression detection for drift. "Full speed" replay = render every coalesced frame without artificial throttle. The 16ms number is tunable, not ADR-worthy.
