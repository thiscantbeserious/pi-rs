# Roadmap

Phases in dependency order. A phase starts only when its gate condition is met. Goals priority (docs/GOALS.md) arbitrates all within-phase trade-offs.

## Phase 0 - Compat spike (gate for everything)

The ADR 0002 spike, 2-day timebox:

1. Record the oracle pin: latest pi release on spike start day (update ADR 0007 + AGENTS.md with the exact version)
2. Extract pi's extension API surface from the pinned version into `docs/extension-api-surface.md` - the checklist the Host Protocol must cover
3. Stub the pi API under Deno. Load the full real extension corpus (incl. pi-subagents per ADR 0018)
4. Stream real completions through pi-ai under Deno: Anthropic (OAuth), OpenAI, Gemini, Bedrock
5. Categorize failures shimmable vs BLOCKER. Streaming BLOCKER without shim ⇒ Node host

**Exit:** ADR 0002 status updated to unconditionally accepted (Deno) or superseded (Node).

## Phase 1 - Skeleton: workspace + protocol + host boot

- Workspace migration (ADR 0011): pi-core, pi-protocol, pi-replay, host/
- pi-protocol message types + ts-rs codegen + msgpack/UDS transport (ADR 0006) + codec benchmark (P17)
- Host boots, handshakes, heartbeats (ADR 0009), restarts (/reload path, ADR 0017)
- CI grows the Deno lane + protocol conformance tests

**Exit:** host round-trip demo. Kill -9 the host → Core survives, prompts, respawns.

## Phase 2 - Render core

- Render thread + tokio split (ADR 0013), alt screen + retained model (ADR 0004)
- Cell-diff, sync-output detection (P12), grapheme-width corpus (P13), panic restore (P15)
- Streaming markdown pipeline (ADR 0010) + theme loading (ADR 0012)
- Frame-time and input-latency benchmarks with synthetic streaming workloads

**Exit:** replay a large stored session through the renderer at full speed inside tmux without artifacts. Benchmarks in CI.

## Phase 3 - Agent loop: dogfood slice (ADR 0007 checkpoint)

- Session read/write, byte-identical re-save (ADRs 0008/0016), pi-replay harness over the JSONL corpus
- Provider trait + host-proxy (ADR 0005). Built-in tools Rust-native (ADR 0015)
- Extension API surface wired end-to-end: tools, commands, events, ctx.ui (ADR 0003), appendEntry routing
- Basic slash commands, compaction

**Exit:** the author's daily coding runs on pi-rs. Dogfood feedback loop opens, UX bets of ADR 0004 validated or revised here.

## Phase 4 - Parity march

- Port pi's test suite feature-by-feature against the pinned oracle
- Native Rust providers land per API type (ADR 0005): openai-completions first (covers Ollama/vLLM/OpenRouter/proxies in one implementation), anthropic-messages second. Credentials leave the host as each lands
- Remaining surface: skills, prompt templates, headless mode (ADR 0018 constraint), theme completeness, session branching edge cases
- Re-baseline the oracle pin deliberately if drift demands it

**Exit:** parity checklist green: ported tests pass + session corpus replays green (ADR 0007).

## Phase 5 - Release (v1.0)

- Distribution: install story, binaries (deno compile host or bundled runtime - decide here, not before)
- release.yml, changelog (cliff), docs - mirror agr's release machinery
- README support matrix honored (ADR 0014). Announce

## Path-change triggers (pre-agreed fallbacks)

Decided in advance to prevent sunk-cost stubbornness. Firing a trigger means writing the superseding ADR and taking the fallback - not relitigating.

| Trigger | Fallback |
|---|---|
| Spike streaming BLOCKER without shim (Phase 0) | Node host (ADR 0002's own gate) |
| Host-side msgpack codec loses the bring-up benchmark badly (P17) | Swap codec (msgpackr, JSON frames only if binary-safety is preserved another way) |
| Alt-screen UX rejected at the Dogfood Checkpoint - selection/copy-mode/scrollback genuinely hurts daily work after a real fix attempt | Inline + diffed live region (supersede ADR 0004. Retained model and pipeline survive, only the screen strategy changes) |
| Tree-sitter grammar bundling bloats binary/build unacceptably | Zed-style WASM grammar loading (research.md) |
| Per-extension Worker sandboxing infeasible post-parity (Node-API-in-workers compat) | Keep process-level permissions, document honestly |
| Deno named-pipe regression (#33366) unresolved when Windows work starts | Windows transport lands on the Node host first |

## Post-parity (explicitly deferred)

Native Windows (ADR 0014) · headless pi-rs as subagent child (ADR 0018) · per-extension Worker sandboxing (ADR 0002, research.md) · capture-level theme overrides (ADR 0012) · declarative extension UI fast path (ADR 0003) · own session format (ADR 0008) · Zed-style WASM grammars if binary size hurts (research.md)
