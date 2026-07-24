# Roadmap

Phases in strict dependency order. Every phase follows the same template: Status, Objective (the risk this phase retires), Deliverables (each one testable), Exit gate (measurable, checked off in the PR that passes it), Explicitly out (the scope fence), Triggers armed (which pre-agreed fallbacks can fire). A phase starts only when the previous exit gate is checked. Goals priority (docs/GOALS.md) arbitrates all within-phase trade-offs. Status values: pending, active, done.

## Phase 0 - Compat spike

**Status:** pending

**Objective:** retire the two biggest unknowns before any architecture hardens: does the Deno host survive contact with the real extension corpus, and is pi's extension runtime vendorable or does the host need a clean-room shim.

**Deliverables:**

- [ ] Oracle pin recorded: latest pi release on spike start day, written into ADR 0007 and AGENTS.md
- [ ] `docs/extension-api-surface.md` extracted from the pinned version: the checklist the Host Protocol must cover
- [ ] pi API stubbed under Deno, full real extension corpus loaded (incl. pi-subagents per ADR 0018)
- [ ] Custom-provider extensions (local-models.ts) register against the stub (ADR 0019)
- [ ] Every failure categorized: shimmable vs BLOCKER
- [ ] Host implementation strategy recommendation with evidence: vendor pi's MIT runtime with protocol backend vs clean-room shim (recorded as an ADR, vendored code ships pi's MIT notice)

**Exit gate:**

- [ ] ADR 0002 status resolved: unconditionally accepted (Deno) or superseded (Node)
- [ ] Host implementation strategy ADR written

**Explicitly out:** any Rust code beyond what the stub needs, protocol design, rendering work.

**Triggers armed:** spike BLOCKER without shim in the 2-day timebox fires the Node host fallback.

## Phase 1 - Walking skeleton

**Status:** pending

**Objective:** retire the integration risk: the thinnest end-to-end line through Core, protocol, and host exists and survives violence, before any feature work.

**Deliverables:**

- [ ] Workspace migration (ADR 0011): pi-core, pi-protocol, pi-replay, host/
- [ ] pi-protocol message types with ts-rs codegen, msgpack over UDS transport (ADR 0006)
- [ ] Host-side codec benchmark (@msgpack/msgpack vs msgpackr, pitfall P17)
- [ ] Host lifecycle: boot, handshake, heartbeat (ADR 0009), restart and /reload path (ADR 0017)
- [ ] CI Deno lane with protocol conformance tests generated from pi-protocol

**Exit gate:**

- [ ] Round-trip demo: a message crosses Core to host and back through the typed protocol
- [ ] kill -9 the host: Core survives, surfaces the native prompt, respawns the host

**Explicitly out:** rendering, providers, real extension API coverage, tools.

**Triggers armed:** codec benchmark failure fires the codec swap.

## Phase 2 - Render core

**Status:** pending

**Objective:** retire the rendering-performance risk, the reason this project exists (GOALS.md goal 1), with measured evidence instead of belief.

**Deliverables:**

- [ ] Render thread + tokio split with render-thread-owned Retained Message Model (ADR 0013)
- [ ] Alt screen, cell diff, synchronized-output detection with graceful degradation (ADR 0004, pitfall P12)
- [ ] Grapheme-width handling with emoji/ZWJ/CJK test corpus (pitfall P13)
- [ ] Panic-safe terminal restore on every exit path (pitfall P15)
- [ ] Streaming markdown pipeline: pulldown-cmark + tree-sitter with block caching (ADR 0010), incomplete-code highlight tests (pitfall P18)
- [ ] Theme loading from the shared ~/.pi tree (ADRs 0012/0020)
- [ ] Frame-time and input-latency benchmarks under synthetic streaming workloads, wired into CI

**Exit gate:**

- [ ] A large stored session (20MB-class) replays through the renderer at full speed inside tmux with zero visual artifacts
- [ ] Benchmarks green in CI with recorded baselines

**Explicitly out:** agent loop, providers, extension execution (host frame buffers may be faked).

**Triggers armed:** grammar bundling bloat fires the WASM-grammar fallback.

## Phase 3 - Dogfood slice

**Status:** pending

**Objective:** retire the daily-usability risk and deliver the verdict on the alt-screen UX bet (ADR 0004) with real work, not demos.

**Deliverables:**

- [ ] Session read/write with byte-identical re-save (ADRs 0008/0016), pi-replay harness over the Session Corpus
- [ ] Native providers: openai-completions (verified against local Ollama), anthropic-messages incl. subscription OAuth with auth.json interop (ADR 0019)
- [ ] Built-in tools Rust-native with pi-parity defaults (ADR 0015)
- [ ] Extension API wired end-to-end: tools, commands, events, ctx.ui frame buffers and focus routing (ADR 0003), appendEntry routing (ADR 0016)
- [ ] Slash commands and compaction at daily-work fidelity
- [ ] Dogfood journal (docs/dogfood-journal.md) for fallback events, evidence via existing screenpipe + agr recordings

**Exit gate:**

- [ ] 5 consecutive workdays where all interactive agent work ran on pi-rs
- [ ] 0 blocking fallbacks to pi in that window (curiosity visits allowed, a blocking fallback resets the streak)
- [ ] Alt-screen verdict recorded: keep ADR 0004 or fire its trigger

**Explicitly out:** remaining API types, skills, prompt templates, headless mode, theme completeness, full parity surface.

**Triggers armed:** alt-screen UX rejection fires the inline live-region fallback.

## Phase 4 - Parity march

**Status:** pending

**Objective:** retire the parity claim itself: turn "pi-rs equals pi" from intention into a measured, green checklist against the pinned Oracle.

**Deliverables:**

- [ ] pi's test suite ported feature-by-feature against the Oracle
- [ ] Remaining native API types: openai-responses, google-generative-ai (ADR 0019)
- [ ] Remaining surface: skills, prompt templates, headless mode (ADR 0018), theme completeness, session branching edge cases
- [ ] Oracle re-baseline performed deliberately if drift demands it, delta-ported and recorded

**Exit gate:**

- [ ] Ported oracle tests pass
- [ ] Session Corpus replays green with byte-identical re-save (ADR 0007)

**Explicitly out:** distribution, announcement, post-parity list.

**Triggers armed:** none (parity failures here are work, not path changes).

## Phase 5 - Release v1.0

**Status:** pending

**Objective:** retire the stranger-install risk: prove the release works on a machine that has never seen the project, not just on the dev machine.

**Deliverables:**

- [ ] Documented install path and binaries (host distribution decided here: deno compile vs bundled runtime)
- [ ] Release machinery mirrored from agent-session-recorder: release.yml, changelog (cliff), tag validation hooks
- [ ] Announcement artifact

**Exit gate:**

- [ ] Clean VM/container: install via the documented path succeeds
- [ ] Smoke script passes: starts, loads the extension corpus, streams a completion, resumes a pi session
- [ ] Release pipeline green: tagged, changelog, binaries attached
- [ ] Support matrix verified per ADR 0014

**Explicitly out:** everything in the post-parity list.

## Path-change triggers (pre-agreed fallbacks)

Decided in advance to prevent sunk-cost stubbornness. Firing a trigger means writing the superseding ADR and taking the fallback, not relitigating.

| Trigger | Armed in | Fallback |
|---|---|---|
| Spike BLOCKER in extension loading or custom-provider registration without shim | Phase 0 | Node host (ADR 0002's own gate) |
| Host-side msgpack codec loses the bring-up benchmark badly (P17) | Phase 1 | Swap codec (msgpackr, JSON frames only if binary-safety is preserved another way) |
| Tree-sitter grammar bundling bloats binary/build unacceptably | Phase 2 | Zed-style WASM grammar loading (research.md) |
| Alt-screen UX rejected at the dogfood gate after a real fix attempt | Phase 3 | Inline + diffed live region (supersede ADR 0004, retained model and pipeline survive) |
| Per-extension Worker sandboxing infeasible (Node-API-in-workers compat) | Post-parity | Keep process-level permissions, document honestly |
| Deno named-pipe regression (deno#33366) unresolved when Windows work starts | Post-parity | Windows transport lands on the Node host first |

## Post-parity (explicitly deferred)

Native Windows (ADR 0014) · headless pi-rs as subagent child (ADR 0018) · per-extension Worker sandboxing (ADR 0002, research.md) · capture-level theme overrides (ADR 0012) · declarative extension UI fast path (ADR 0003) · own session format (ADR 0008) · Zed-style WASM grammars if binary size hurts (research.md) · pi alias for the binary (ADR 0020)
