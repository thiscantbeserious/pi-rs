# Runtime-agnostic Host Protocol, Deno host first, Node as fallback

The Host Protocol (Core ↔ Extension Host IPC contract) is designed runtime-neutral so any JavaScript runtime can implement an Extension Host. The first host implementation targets Deno for its permissions model (`--allow-read`, `--allow-net`) [[1]](https://docs.deno.com/runtime/fundamentals/security/), V8 parity with Node, and `deno compile` single-binary distribution [[2]](https://docs.deno.com/runtime/reference/cli/compile/). If Deno's Node-compat gaps prove unresolvable for our dependency graph, we switch the host to plain Node without touching the Core or the protocol.

## Status

accepted - compat spike passed 2026-07-24, Deno host locked in (see ADR 0021 for evidence and the host-implementation strategy)

## The gate: compat spike (2-day timebox)

Resolved 2026-07-24. Result: 14 of 14 real extensions load and register under Deno 2.9.4 via pi's own loader (100%, bar was ≥90%), including pi-subagents (ADR 0018) and the local-models.ts custom provider (ADR 0019). Zero BLOCKERs. The two non-loading cases on a default-permissions run were Deno write-permission denials (subagent temp-results dir, local-models endpoints config), both shimmable with scoped `--allow-write`, both resolved on the second run. Full evidence and the vendor-vs-clean-room decision live in ADR 0021. The original gate criteria are preserved below.

1. All real extensions from the existing pi setup (~13, incl. subagent, token-optimizer) load and register against a stubbed pi API under Deno - pass bar: ≥90%
2. Custom-provider extensions (e.g. local-models.ts) register successfully against the stubbed API - pass bar: zero blockers (providers are otherwise Rust-native per ADR 0019, so pi-ai streaming under Deno is no longer gated)
3. Every failure is categorized: shimmable (workaround inside the timebox) vs BLOCKER

Any BLOCKER in extension loading or custom-provider registration without a shim inside the timebox: switch to the Node host, no relitigating.

## Considered Options

- Node host first - rejected for now: no permissions sandbox, weaker single-binary story. Remains the designated fallback (zero compat drift, VS Code precedent)
- Bun host - rejected: JSC-vs-V8 behavioral drift from the compat target, no permissions model, stability concerns for a long-lived host process

## Consequences

- Known open risks under Deno as of research date (2026): undici proxy dispatcher failures [[3]](https://github.com/denoland/deno/issues/30899), HTTP/2 gaps [[4]](https://github.com/denoland/deno/issues/33153) [[5]](https://github.com/denoland/deno/issues/31357), @aws-sdk credential provider hangs [[6]](https://github.com/aws/aws-sdk-js-v3/issues/4405). A compat spike running the real extension corpus under Deno must pass before the Deno host is locked in.
- Protocol discipline required: no runtime-specific types or behaviors may leak into the Host Protocol.
- Permissions honesty: Deno permissions are per-process, so the v1 host runs with the union of extension needs - still stronger than VS Code's extension host (no sandbox at all [[7]](https://safeguard.sh/resources/blog/vscode-extension-security-development-guide)). Per-extension scoping is achievable post-parity via one Worker per extension (WorkerOptions.deno.permissions [[8]](https://docs.deno.com/api/web/~/WorkerOptions.deno), verified), at the cost of worker-context compat work.

## Sources

1. Deno security and permissions model, secure by default with granular allow/deny flags: <https://docs.deno.com/runtime/fundamentals/security/>
2. deno compile, single-binary distribution: <https://docs.deno.com/runtime/reference/cli/compile/>
3. undici proxy dispatcher failures under Deno: <https://github.com/denoland/deno/issues/30899>
4. undici HTTP/2 hangs under Deno: <https://github.com/denoland/deno/issues/33153>
5. undici HTTP/2 support gaps under Deno: <https://github.com/denoland/deno/issues/31357>
6. aws-sdk credential provider hangs under Deno: <https://github.com/aws/aws-sdk-js-v3/issues/4405>
7. VS Code extension host runs all extensions unsandboxed in one process: <https://safeguard.sh/resources/blog/vscode-extension-security-development-guide>
8. Deno Worker scoped permissions, never exceeding the parent: <https://docs.deno.com/api/web/~/WorkerOptions.deno>
