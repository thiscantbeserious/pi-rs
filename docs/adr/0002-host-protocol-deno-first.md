# Runtime-agnostic Host Protocol; Deno host first, Node as fallback

The Host Protocol (Core ↔ Extension Host IPC contract) is designed runtime-neutral so any JavaScript runtime can implement an Extension Host. The first host implementation targets Deno for its permissions sandbox (`--allow-read`, `--allow-net` per extension), V8 parity with Node, and `deno compile` single-binary distribution. If Deno's Node-compat gaps prove unresolvable for our dependency graph, we switch the host to plain Node without touching the Core or the protocol.

## Status

accepted — gated on the compat spike defined below

## The gate: compat spike (2-day timebox)

1. All real extensions from the existing pi setup (~13, incl. subagent, token-optimizer) load and register against a stubbed pi API under Deno — pass bar: ≥90%
2. pi-ai streams a real completion from each major provider under Deno — Anthropic (OAuth), OpenAI, Gemini, Bedrock (aws-sdk) — pass bar: zero blockers (this is the critical path per ADR 0005's host-proxy bootstrap)
3. Every failure is categorized: shimmable (workaround inside the timebox) vs BLOCKER

Any streaming BLOCKER without a shim inside the timebox ⇒ switch to the Node host, no relitigating.

## Considered Options

- Node host first — rejected for now: no permissions sandbox, weaker single-binary story; remains the designated fallback (zero compat drift, VS Code precedent)
- Bun host — rejected: JSC-vs-V8 behavioral drift from the compat target, no permissions model, stability concerns for a long-lived host process

## Consequences

- Known open risks under Deno as of research date (2026): undici proxy dispatcher failures (denoland/deno#30899), HTTP/2 gaps (#33153, #31357), @aws-sdk credential provider hangs (aws-sdk-js-v3#4405). A compat spike running the real extension corpus under Deno must pass before the Deno host is locked in.
- Protocol discipline required: no runtime-specific types or behaviors may leak into the Host Protocol.
- Permissions honesty: Deno permissions are per-process, so the v1 host runs with the union of extension needs — still stronger than VS Code's extension host (no sandbox at all). Per-extension scoping is achievable post-parity via one Worker per extension (WorkerOptions.deno.permissions, verified), at the cost of worker-context compat work.
