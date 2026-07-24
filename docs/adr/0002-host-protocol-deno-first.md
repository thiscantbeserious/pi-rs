# Runtime-agnostic Host Protocol; Deno host first, Node as fallback

The Host Protocol (Core ↔ Extension Host IPC contract) is designed runtime-neutral so any JavaScript runtime can implement an Extension Host. The first host implementation targets Deno for its permissions sandbox (`--allow-read`, `--allow-net` per extension), V8 parity with Node, and `deno compile` single-binary distribution. If Deno's Node-compat gaps prove unresolvable for our dependency graph, we switch the host to plain Node without touching the Core or the protocol.

## Status

accepted — gated on a compat spike

## Considered Options

- Node host first — rejected for now: no permissions sandbox, weaker single-binary story; remains the designated fallback (zero compat drift, VS Code precedent)
- Bun host — rejected: JSC-vs-V8 behavioral drift from the compat target, no permissions model, stability concerns for a long-lived host process

## Consequences

- Known open risks under Deno as of research date (2026): undici proxy dispatcher failures (denoland/deno#30899), HTTP/2 gaps (#33153, #31357), @aws-sdk credential provider hangs (aws-sdk-js-v3#4405). A compat spike running the real extension corpus under Deno must pass before the Deno host is locked in.
- Protocol discipline required: no runtime-specific types or behaviors may leak into the Host Protocol.
