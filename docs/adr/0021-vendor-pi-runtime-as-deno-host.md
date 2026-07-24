# Vendor pi's extension runtime as the Deno Host, with a Rust protocol backend

Supersedes the open question in ADR 0002's gate and ROADMAP Phase 0 deliverable 5. The pi-rs Extension Host vendors pi's MIT-licensed extension runtime (loader, `ExtensionRuntime`, and the `ExtensionActions` / `ExtensionContextActions` / `ExtensionCommandContextActions` seams) and binds its action methods to a Host Protocol backend implemented in Rust. A clean-room shim is rejected: the spike proves pi's runtime loads the full real extension corpus under Deno unmodified, so reimplementing the surface would replicate hundreds of methods (sections B through H of `docs/extension-api-surface.md`) for zero compatibility benefit.

## Spike evidence (2026-07-24)

A Deno 2.9.4 script importing pi's own `discoverAndLoadExtensions` from the pinned `@earendil-works/pi-coding-agent@0.82.0` loaded the author's real extension corpus from `~/.pi/agent/extensions`:

- 14 of 14 extensions loaded (100%, ADR 0002's bar was ≥90%), including `pi-subagents` (ADR 0018) and the `local-models.ts` custom provider (ADR 0019, which calls `pi.registerProvider` and `pi.registerCommand`)
- The full npm dependency tree resolved under Deno's npm compat (`@aws-sdk`, `@google/genai`, `openai`, `@anthropic-ai/sdk`, `@mistralai/mistralai`, `@modelcontextprotocol/sdk`, `jiti`, `undici`, etc.)
- 0 BLOCKERs

### Failure categorization (ROADMAP Phase 0 deliverable 5)

| Extension | Symptom | Category | Resolution |
| --- | --- | --- | --- |
| `subagent/src/extension/index.ts` (pi-subagents, ADR 0018) | `NotCapable: Requires write access to .../pi-subagents-uid-501/async-subagent-results` | shimmable | Deno `--allow-write` on the host's scoped temp tree. Loads clean on the second run |
| `local-models.ts` (custom provider, ADR 0019) | `NotCapable: Requires write access to "/Users/.../.pi/agent"` while saving endpoints config | shimmable | Same: scoped `--allow-write` to the `~/.pi` tree. Loads clean on the second run |

Both failures were Deno filesystem-permission denials, not compatibility or shim problems. Neither is a BLOCKER. No Node-host fallback trigger fired.

## Considered Options

- Clean-room shim of the extension API - rejected: would reimplement `ExtensionAPI` (38 event subscriptions, 30+ `pi.*` methods), `ExtensionContext`, `ExtensionUIContext`, `ExtensionCommandContext`, the tool definition surface, and the `ExtensionRuntime` action seams, all of which pi already provides and which the spike proved load under Deno. Zero compat benefit, large ongoing maintenance cost, and it would diverge from pi on every Oracle re-baseline
- Vendor pi's runtime with a protocol backend - accepted: pi's runtime works unmodified under Deno. The host adds only the thin binding layer that implements the `ExtensionActions` / context-action seams (section H of the API surface doc) over the Host Protocol, routing each `pi.*` call and `ctx.*` call to the Core (ADR 0016 for `appendEntry`, ADR 0003 for `ctx.ui` frame buffers, ADR 0009 for hook verdicts)

## Consequences

- Vendored code ships pi's MIT notice alongside pi-rs's own license. The vendored tree is the pinned Oracle version (0.82.0, ADR 0007), re-vendored deliberately on each re-baseline
- The host is `pi runtime + protocol binding`, not `pi runtime + pi Core`. The Core stays Rust-native: built-in tools (ADR 0015), providers (ADR 0019), session writing (ADR 0016), rendering (ADR 0004). The vendored runtime only executes extensions
- Permissions: the v1 host runs with the union of extension needs, scoped to the `~/.pi` tree and a temp-results directory. Per-extension Worker sandboxing remains a post-parity option (ADR 0002). The spike's two permission denials confirm the scope is small and knowable
- Upgrade discipline: a pi version bump is a re-vendor plus a re-run of this spike's loader test against the real corpus. Drift is caught by the Oracle parity suite (ADR 0007), not by the shim
- ADR 0002 is unconditionally accepted: Deno host locked in, Node fallback stands down. The known Deno risks listed in ADR 0002 (undici proxy/HTTP-2, `@aws-sdk` credential hangs) did not surface during corpus loading and remain tracked for the provider-streaming work in Phase 3, where they would actually be exercised

## Sources

1. pi MIT license (vendoring premise): <https://github.com/earendil-works/pi/blob/v0.82.0/LICENSE>
2. Host Protocol coverage checklist, extracted from the pinned 0.82.0 dist type declarations: `docs/extension-api-surface.md`
3. Deno permissions model, scoped allow flags resolve the two shimmable failures: <https://docs.deno.com/runtime/fundamentals/security/>
