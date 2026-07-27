# Phase 2 — Render core: plan doc

The reason this project exists (GOALS.md goal 1: streaming smoothness). Retires the rendering-performance risk with measured evidence, not belief. Decisions D-A through D-G were grilled and settled 2026-07-26; the hard-to-reverse architecture is in ADRs 0024–0026, the reversible crate/tool choices and verification in `docs/research.md`. Every external/pi-codebase fact below carries an inline `[[n]](url)` citation matching the Sources list; internal repo cross-refs (ADR numbers, pitfall P-numbers, ROADMAP, PHILOSOPHY §x, crate paths) are named in place per §9.

## Settled decisions

| # | Decision | Where recorded |
| --- | --- | --- |
| D-A | Terminal backend: ratatui on crossterm. `Buffer::diff` is the tested cell-diff (P9/P13) [[1]](https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html). Mode 2026 wrapped by us around `terminal.flush()` [[5]](https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html). Kitty keyboard via crossterm directly [[4]](https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs). Workspace-unified crossterm + `cargo deny` (P16). Width NOT delegated to ratatui [[2]](https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs) [[3]](https://github.com/ratatui/ratatui/issues/75). | ADR 0024 |
| D-B | Tree-sitter grammar bundling: static per-language grammar crates behind Cargo features, minimal daily-driver set first (rust, TS/JS, bash, json), `tree-sitter-highlight` as the highlight layer [[13]](https://docs.rs/tree-sitter-highlight/latest/tree_sitter_highlight/). Measure binary/build size; WASM fallback trigger armed. | research.md |
| D-C | Grapheme width: owned at the projection layer (not ratatui's `unicode-width`-based `CellWidth` [[2]](https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs)). `runefix-core` behind a `GraphemeWidth` trait [[15]](https://crates.io/crates/runefix-core). P13 corpus as regression spec [[16]](https://mitchellh.com/writing/grapheme-clusters-in-terminals). | ADR 0025 (architecture), research.md (crate) |
| D-D | Crate layout: new `pi-render` crate (amends ADR 0011). Render deps isolated from `pi-core`'s host supervisor and `mock-host`. Agent-loop↔renderer boundary structural via the dependency graph. | ADR 0026 |
| D-E | Replay message type: `RenderMessage` in `pi-render`, parsed from the opaque `serde_json::Value` that `pi-session::Message` carries (`crates/pi-session/src/entry.rs`). Minimal shape (role + Text/Thinking/ToolCall/ToolResult) verified against `packages/ai/src/types.ts` at v0.82.0 [[21]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts); the `AgentMessage` union lives in `packages/coding-agent/docs/session-format.md` [[22]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/docs/session-format.md). `pi-messages` crate deferred to Phase 3. | research.md |
| D-F | Theme loading: typed `Theme` struct in `pi-render` (parse-don't-validate, tolerant of unknown keys per ADR 0020). Capture→palette as a `match` (tree-sitter's standard capture set (~27 names) [[14]](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html) → pi's 9 `syntax*` keys [[20]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/modes/interactive/theme/theme-schema.json), plain-text fallback). Themes read-only from `~/.pi/agent/themes/*.json`. | research.md |
| D-G | Exit-gate measurement: real tmux in CI (ubuntu-latest) + regression suite (P13 corpus, balanced BSU/ESU, terminal-restore probe P15, frame-time bench, no-panic soak, final-frame snapshots) + recorded replay for human sign-off. Synthetic 20MB generator for CI + real Corpus session for sign-off. Budgets: frame-time p99 < 16ms, input-latency p99 < 16ms, baselines committed, criterion regression detection [[25]](https://docs.rs/criterion). | research.md |

## Steps (each lands as its own MR on a feature branch)

Ordered so the riskiest unknown retires first. Each step: write the failing test first (RED), make it pass with minimum code (GREEN), refactor under the passing test. Cite the pinned Oracle (`v0.82.0`, ADR 0007) with a permalink for any behavior with a pi equivalent; state explicitly where there is no pi equivalent (PHILOSOPHY §9.5).

### Step 1 — Crate skeleton + panic-safe terminal restore + cargo-deny guard

- Create `crates/pi-render` per ADR 0026. Workspace `Cargo.toml` gains `pi-render = { path = "crates/pi-render" }`. Add ratatui + crossterm only (later steps add their specific deps incrementally, all workspace-unified from the first addition).
- Terminal backend: enter/leave alt screen, raw mode, mouse capture. **Single owned restore path installed as a panic hook before first draw** (P15, P3). Zero-size terminal guard in the resize handler (P15).
- **SIGTSTP/SIGCONT suspend/resume handling** (P14, ADR 0024): install signal handlers mirroring Codex-RS's `SuspendContext` [[7]](https://github.com/openai/codex/blob/31519549/codex-rs/tui/src/tui/job_control.rs). On SIGTSTP: `PopKeyboardEnhancementFlags` + leave alt screen + disable raw mode + `libc::kill(0, SIGTSTP)`. On SIGCONT: re-enter alt screen + enable raw mode + `PushKeyboardEnhancementFlags`. crossterm exposes the full push/pop pair [[4]](https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs); it has no built-in signal handling (issue #494 open) [[6]](https://github.com/crossterm-rs/crossterm/issues/494), so the app installs handlers itself.
- **`cargo deny` guard lands here** (P16, ADR 0024 "starts here"): create `deny.toml` with `[bans] multiple-versions = "deny"` [[8]](https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html) scoped to crossterm/terminal-backend, and add the `EmbarkStudios/cargo-deny-action` CI job [[9]](https://github.com/EmbarkStudios/cargo-deny-action) in this step so Steps 1-7 run under the guard.
- Failing test first: assert terminal state is restored after a panic, after normal exit, after a zero-size resize, and after a SIGTSTP/SIGCONT cycle (pty harness: spawn process, send SIGTSTP, probe terminal state, send SIGCONT, assert clean). Tests need a pty harness (the existing supervisor chaos test already uses process spawning — `crates/pi-core/tests/supervisor_integration.rs`).
- **No pi equivalent** (pi is single-process JS; render thread, terminal backend, signal handling are pi-rs-native, ADR 0013). State explicitly per §9.5.
- `pi-core` depends on `pi-render` (one-way). `mock-host` binary stays render-free. README mermaid diagram already updated this session (ADR 0026); re-confirm in the Step 1 PR.

### Step 2 — Render thread + tokio split + channel contract

- Dedicated OS thread running the synchronous loop (poll input ≤16ms → drain `RenderEvent`s → project → `terminal.flush()` wrapped in mode 2026 [[5]](https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html) → write). Never awaits (ADR 0013, ADR 0024). The ≥16ms coalescing budget is ADR 0010's.
- `RenderEvent` contract mirrors pi's two-layer streaming model (ADR 0029): the 11 agent-loop variants of pi's `AgentEvent` [[28]](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437) with `MessageUpdate` nesting pi's 12-variant `AssistantMessageEvent` [[29]](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503) (the nesting mirrors pi's `message_update.assistantMessageEvent` [[30]](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432)), plus four pi-rs-native render controls (`Resize`, `ThemeChanged`, `FrameBufferUpdated`, `Quit`). The `partial: AssistantMessage` snapshot pi carries on every streaming variant is omitted (documented divergence: ADR 0013 makes the render thread the sole mutator, so the snapshot is redundant dead data, PHILOSOPHY §5). Complex payloads (`AgentMessage`, `ToolCall`) are opaque `serde_json::Value` until Step 7 (D-E). `mpsc` from the tokio side into the render thread (ADR 0013 cites tokio channels). Single-threaded mutation of the RMM, no locks (ADR 0013).
- Input read on the render thread (ADR 0013: minimum keystroke latency; guards P2). Focus routing is **Phase 3** (ROADMAP Phase 3 deliverable: "ctx.ui frame buffers and focus routing, ADR 0003"); Phase 2 has no extension UI so focus is trivially always-transcript, no stub built (YAGNI, PHILOSOPHY §5).
- Failing test: assert a `RenderEvent` sent from a tokio task is applied before the next frame; assert the render thread never blocks on the channel (non-blocking drain).
- **Parity**: the agent-loop and streaming variants mirror pi's `AgentEvent`/`AssistantMessageEvent` at v0.82.0 (ADR 0029, permalinks L422-L437 / L491-L503). The render-thread/tokio split itself is pi-rs-native (pi is single-process JS, ADR 0013). State explicitly per §9.5.

### Step 3 — Retained Message Model + cell grid + cell diff + mode 2026 + frame-buffer compositing

- The RMM the render thread owns (ADR 0004). Viewport is a pure function of it (P5 guard). **Auto-scroll viewport only in Phase 2** (content arrives, viewport follows the tail, resize re-wraps without corruption — P5). Interactive scrollback, search, copy-mode are **Phase 3** (P6; ADR 0007 says copy-mode surfaces during dogfood).
- ratatui `Buffer` + `Buffer::diff` (ADR 0024) [[1]](https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html). Mode 2026 capability query at startup (`CSI ? 2026 $ p`), graceful degrade where unsupported (P12). BSU/ESU pairs tight around each flush (P12).
- **Frame-buffer compositing path** (ADR 0003): the cell grid composites a frame-buffer region from `RenderEvent::FrameBufferUpdated` into the grid at the render thread's own pace. Source is **synthetic** in Phase 2 (a test fixture, not the Host Protocol — ROADMAP Phase 2: "host frame buffers may be faked"). Focus routing stays Phase 3.
- **Tests are ASCII-scoped** (width is trivial in ASCII): ratatui's `Buffer` uses `unicode-width` (the P13 failure class, verified) [[2]](https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs) before Step 4 introduces the projection-layer width. Step 4 introduces the grapheme corpus that exposes the `unicode-width` limitation and replaces it.
- Failing test: assert a single-cell change produces a single-cell diff (P9); assert resize re-wraps without scrollback corruption (P5); assert mode 2026 degrade path works without tearing; assert a synthetic frame buffer composites into the grid.
- **Pi equivalent**: pi's `TUI` class does line-granular differential rendering via `previousLines: string[]` (L297) with `MIN_RENDER_INTERVAL_MS = 16` (L309) and `previousViewportTop` auto-scroll (L315) [[18]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/tui/src/tui.ts). pi-rs's cell-granular diff is the improvement over pi's line-granular diff (P9). Cite with line anchors.

### Step 4 — Grapheme-width engine + corpus

- `GraphemeWidth` trait (ADR 0025). `runefix-core` impl (research.md) [[15]](https://crates.io/crates/runefix-core). Query mode 2027 where available (P13).
- Projection layer: segment with the width crate's `graphemes()`/`atoms()` [[15]](https://crates.io/crates/runefix-core), compute cluster width, feed pre-widthed spans into ratatui.
- P13 corpus as snapshot tests: single-codepoint emoji, ZWJ family, VS16, CJK (Hiragana/Katakana/Han), East-Asian-ambiguous, combining marks, halfwidth katakana [[16]](https://mitchellh.com/writing/grapheme-clusters-in-terminals). Assert the cell count the projection assigns. **Cell-diff corruption from width drift is the failure mode the test catches.**
- Failing test: each corpus class asserts its expected width; a ratatui upgrade that changes cell assignment is caught.
- **No pi equivalent for the grapheme-aware width engine** (pi uses JS string width; the projection-layer grapheme engine is pi-rs-native, ADR 0025). The P13 corpus itself is pi-rs's regression spec, not a pi port. State explicitly per §9.5.

### Step 5 — Streaming markdown pipeline

- pulldown-cmark re-parse per coalesced frame (ADR 0010) [[23]](https://github.com/pulldown-cmark/pulldown-cmark). tree-sitter highlight on code blocks via `tree-sitter-highlight` [[13]](https://docs.rs/tree-sitter-highlight/latest/tree_sitter_highlight/) (tree-sitter is incremental and error-tolerant, ADR 0010 [[24]](https://github.com/tree-sitter/tree-sitter)). Finalized-block cache; incremental tree edit on the tail block only (P11, P18).
- Incomplete-code corpus per grammar (P18: tree-sitter error recovery varies by grammar, "catastrophically bad" recovery in some grammars on mid-typing edits [[17]](https://github.com/tree-sitter/tree-sitter/issues/2404); a grammar with bad recovery falls back to plain styling for the growing tail block).
- Failing test: assert a finalized block is cached (not re-highlighted); assert the tail block re-highlights incrementally; assert incomplete code doesn't panic.
- **Pi equivalent**: pi's `Markdown` component (L257) + `getMarkdownTheme()` (L449) [[19]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/docs/tui.md). pi-rs's block-granular highlight caching is the improvement (P11). Cite the component.

### Step 6 — Theme loader

- Typed `Theme` struct (D-F). Read `~/.pi/agent/themes/*.json` (ADR 0020). `vars` resolved at render time. Capture→palette `match` (D-F). Tolerant of unknown keys, never destructively rewritten (ADR 0020).
- Live theme switch: `RenderEvent::ThemeChanged` flushes the block cache (ADR 0010) and re-projects.
- Failing test: assert a theme loads and resolves vars; assert live switch restyles the RMM without a full re-parse; assert unknown keys are tolerated.
- **Pi equivalent**: pi's `theme-schema.json` (the 9 `syntax*` keys at L74-82) [[20]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/modes/interactive/theme/theme-schema.json), cited in research.md. pi-rs loads themes unchanged (ADR 0012).

### Step 7 — Replay wiring + 20MB gate

- `RenderMessage` in `pi-render` parsed from `pi-session::Message`'s opaque `Value` (D-E; the opaque `Value` is in `crates/pi-session/src/entry.rs`). `pi-replay` depends on `pi-render` + `pi-session` (ADR 0026), feeds entries into the RMM at full speed.
- Synthetic 20MB generator in `pi-replay` (real `pi-session` entry types, realistic content: large bash outputs, file reads). **20MB = 20MB of JSONL** (thousands of entries with large tool outputs). Real Corpus session from `~/.pi/agent/sessions` for human sign-off if available (ADR 0020 shared tree).
- Failing test: the 20MB replay runs to completion without panic; final-frame snapshots match.
- **No pi equivalent** (pi-replay is a pi-rs-native tool; pi has no equivalent replay harness, ADR 0007). State explicitly per §9.5.

### Step 8 — Benchmarks + CI tmux + screen lanes

- Criterion: frame-time (p99 < 16ms per coalesced frame) and input-latency (p99 < 16ms keystroke-to-frame) under synthetic streaming workloads [[25]](https://docs.rs/criterion). Baselines committed. The 16ms budget is the 60fps frame budget from ADR 0010's ≥16ms coalescing; input-latency guards P2 (input read on the render thread, ADR 0013).
- **CI lanes (ubuntu-latest, both apt-installed — neither tmux nor screen is pre-installed on GH Actions ubuntu-24.04, verified [[12]](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md)):**
  - **tmux lane**: tests synchronized output (mode 2026 pass-through, tmux PR #4744 [[10]](https://github.com/tmux/tmux/pull/4744)). Runs the 20MB replay inside tmux, regression suite (P13 corpus, balanced BSU/ESU, terminal-restore probe P15, no-panic soak, final-frame snapshots).
  - **screen lane**: tests the **degrade path** (GNU screen does not support DECSET 2026; Claude Code #19533 documents the corruption [[11]](https://github.com/anthropics/claude-code/issues/19533)). Runs the 20MB replay under screen, asserts no panic + terminal restored, asserts cell-diff-only path runs without tearing (the `?2026` capability query returns false → degrade, P12).
- `cargo deny` CI job (set up in Step 1 [[8]](https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html) [[9]](https://github.com/EmbarkStudios/cargo-deny-action)) continues to run here.
- Recorded replay (asciinema) committed for human sign-off on the gate PR.
- Failing test: benchmarks produce a baseline; regression detection wired.
- **No pi equivalent** (the benchmark suite is pi-rs-native; pi has no equivalent frame-time/input-latency benchmark suite). State explicitly per §9.5.

## Exit gate (ROADMAP Phase 2)

| Gate check | Proof | Status |
| --- | --- | --- |
| 20MB-class session replays at full speed inside tmux (and screen degrade path), zero visual artifacts | Recorded replay run + regression suite green (tmux synchronized-output lane + screen degrade lane) | ⬜ |
| Frame-time and input-latency benchmarks green | CI baselines committed | ⬜ |

## Open sub-items (research, not blockers)

- Oracle check: pi's tree-sitter grammar set at `v0.82.0` (which languages pi highlights) — finalizes the D-B minimal set. A find-docs/librarian task.
- Oracle check: `RenderMessage` shape verified against `packages/ai/src/types.ts` at v0.82.0 with line anchors (D-E, §9.5) — content blocks at L329 (TextContent), L335 (ThinkingContent), L345 (ImageContent), L351 (ToolCall), L384 (UserMessage), L390 (AssistantMessage), L405 (ToolResultMessage), `Message` union L423 [[21]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts); `AgentMessage` union at `packages/coding-agent/docs/session-format.md` L162 [[22]](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/docs/session-format.md).
- `runefix-core` maintenance watch: if it stalls, fall back to `unicode-display-width` behind the `GraphemeWidth` trait (research.md).
- `tests/e2e_test.sh` expansion (drift audit D7, `docs/oracle-drift-audit.md`): step 7/8 should drive the replay gate, not just the version line.

## Tracked open item: first-class Core-owned intercom (future ADR)

A first-class, Core-owned session intercom (sessions auto-discover each other and message without FS writes) is a pi-rs-native addition with **no pi equivalent**. The existing implementations are extension-level workarounds:

- `pi-intercom` [[26]](https://github.com/nicobailon/pi-intercom/blob/5caa4aa1/README.md): a separate extension that spawns a standalone TypeScript broker process. Sessions connect over a Unix socket (named pipe on Windows) with a length-prefixed JSON protocol. The broker routes 1:1 messages by session ID. `ask` blocks up to 10 min. Messages persist to Pi session history (FS). Config at `~/.pi/agent/intercom/config.json`.
- `pi-subagents` native supervisor channel [[27]](https://github.com/nicobailon/pi-subagents/blob/main/README.md) (no `pi-intercom` needed): `contact_supervisor` (child to parent, `need_decision`/`interview_request`/`progress_update`) and `subagent_supervisor({ action: "reply" })` (parent to child), scoped to the exact spawning session via `PI_SUBAGENT_PARENT_SESSION`. Exposes `intercom` as a fallback name if no external tool owns it.

Both of these documented implementations are extension-level; neither has Core support. In `pi-subagents`, `pi.events` is in-process only and explicitly "does not reach separate Pi processes or child subagents" [[27]](https://github.com/nicobailon/pi-subagents/blob/main/README.md). In `pi-intercom`, cross-process coordination requires the standalone broker process or the FS [[26]](https://github.com/nicobailon/pi-intercom/blob/5caa4aa1/README.md). No documented pi implementation provides Core-owned, in-process session messaging without FS writes.

The owner's intent: make this a first-class citizen. The Core runtime maintains a session registry (live sessions by ID) and an in-process message bus (tokio mpsc/broadcast). Sessions register on start and message without FS writes. Subagents spawned as headless children register back over their stdio/UDS. This is hard-to-reverse, shapes the Core's concurrency model (GOALS goal 2), and warrants a dedicated ADR + plan before implementation.

**Status:** tracked here so it is not lost. Scoping deferred to a dedicated session (research the broker-vs-Core-registry trade-off, the message-bus topology, the auto-discovery mechanism, and the relationship to ADR 0018's "Core is designed subagent-aware"). Not a Phase 2 blocker; the render thread and RMM do not depend on it.

## Explicitly out (ROADMAP Phase 2)

Agent loop, providers, extension execution (host frame buffers may be faked). The `pi-messages` crate (full `AgentMessage` mirror) is Phase 3 (contract §10 open question, `docs/session-format-contract.md`).

## Triggers armed

- Tree-sitter grammar bundling bloats binary/build unacceptably → Zed-style WASM grammar loading (ROADMAP Phase 2).

## Sources

1. ratatui `Buffer::diff`, cell-granular diff with multi-width handling (the P9/P13 guard): <https://docs.rs/ratatui/latest/ratatui/prelude/buffer/struct.Buffer.html>
2. ratatui `CellWidth` trait, delegates to `unicode-width` (the P13 limitation we bypass): <https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs>
3. ratatui #75, `unicode-width` and emojis (the open failure class, no override hook): <https://github.com/ratatui/ratatui/issues/75>
4. crossterm `PushKeyboardEnhancementFlags` / `PopKeyboardEnhancementFlags` (kitty keyboard protocol, P14 guard): <https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs>
5. crossterm `SynchronizedUpdate` (mode 2026, P12 guard): <https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html>
6. crossterm issue #494, no built-in SIGTSTP/SIGCONT signal handling: <https://github.com/crossterm-rs/crossterm/issues/494>
7. Codex-RS `SuspendContext`, the SIGTSTP/SIGCONT restore pattern in a ratatui/crossterm TUI: <https://github.com/openai/codex/blob/31519549/codex-rs/tui/src/tui/job_control.rs>
8. cargo-deny `[bans]` `multiple-versions` (P16 duplicate-version guard): <https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html>
9. `EmbarkStudios/cargo-deny-action` GitHub Action: <https://github.com/EmbarkStudios/cargo-deny-action>
10. tmux PR #4744, DECSET 2026 synchronized-output pass-through: <https://github.com/tmux/tmux/pull/4744>
11. Claude Code #19533, GNU screen lacks DECSET 2026 (display corruption): <https://github.com/anthropics/claude-code/issues/19533>
12. GitHub Actions ubuntu-24.04 runner image readme (apt list: neither tmux nor screen pre-installed): <https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md>
13. `tree-sitter-highlight` crate, the highlight layer: <https://docs.rs/tree-sitter-highlight/latest/tree_sitter_highlight/>
14. tree-sitter syntax highlighting, the standard capture name set (~27 names): <https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html>
15. `runefix-core`, grapheme-cluster-aware width engine (chosen, behind `GraphemeWidth` trait): <https://crates.io/crates/runefix-core>
16. Mitchell Hashimoto, grapheme clusters in terminals (P13 evidence): <https://mitchellh.com/writing/grapheme-clusters-in-terminals>
17. tree-sitter #2404, error recovery quality varies by grammar (P18): <https://github.com/tree-sitter/tree-sitter/issues/2404>
18. pi `packages/tui/src/tui.ts` at v0.82.0 (`previousLines` L297, `MIN_RENDER_INTERVAL_MS = 16` L309, `previousViewportTop` L315, differential rendering L293): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/tui/src/tui.ts>
19. pi `packages/coding-agent/docs/tui.md` at v0.82.0 (`Markdown` component L257, `getMarkdownTheme()` L449): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/docs/tui.md>
20. pi `theme-schema.json` at v0.82.0 (the 9 `syntax*` keys L74-82): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/modes/interactive/theme/theme-schema.json>
21. pi `packages/ai/src/types.ts` at v0.82.0 (`TextContent` L329, `ThinkingContent` L335, `ImageContent` L345, `ToolCall` L351, `UserMessage` L384, `AssistantMessage` L390, `ToolResultMessage` L405, `Message` union L423): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts>
22. pi `packages/coding-agent/docs/session-format.md` at v0.82.0 (`AgentMessage` union L162): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/docs/session-format.md>
23. `pulldown-cmark`, CommonMark-compliant parser (ADR 0010 structure layer): <https://github.com/pulldown-cmark/pulldown-cmark>
24. tree-sitter, incremental and error-tolerant parsing (ADR 0010 highlight engine): <https://github.com/tree-sitter/tree-sitter>
25. Criterion, Rust benchmarking crate: <https://docs.rs/criterion>
26. `pi-intercom` README, the broker-based 1:1 inter-session messaging extension: <https://github.com/nicobailon/pi-intercom/blob/5caa4aa1/README.md>
27. `pi-subagents` README, native supervisor channel and `pi.events` in-process limitation: <https://github.com/nicobailon/pi-subagents/blob/main/README.md>
28. pi `AgentEvent`, the 11-variant agent-loop event the UI subscribes to (Oracle v0.82.0, commit `083e6162`, ADR 0029): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L422-L437>
29. pi `AssistantMessageEvent`, the 12-variant provider streaming protocol (ADR 0029): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/ai/src/types.ts#L491-L503>
30. pi `message_update` carries `assistantMessageEvent` (the nesting bridge, ADR 0029): <https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/agent/src/types.ts#L432>
