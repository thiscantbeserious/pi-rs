# Status: PROPOSED — needs research. Central trust layer: transparent sandboxing for all untrusted content and execution

**⚠ This ADR is open. It records a vision and open questions, not a decision. Research and a grilling session are required before implementation. This is foundational — it shapes tools, extensions, terminal output, and session files.**

P20 (ANSI injection from tool output) is the symptom, not the disease. The disease is that pi-rs has no central trust boundary: untrusted content (tool results, extension output, provider responses, markdown, session files) and untrusted execution (bash, write, edit, extension tools) flow into the system with ad-hoc or absent guards at each entry point. One missed boundary is a vulnerability. The owner's intent: a single, transparent, approachable abstraction that is the first-level building block for all trust decisions — not a scattered set of sanitization filters, and not an invisible labyrinth of seccomp profiles that only an expert can touch.

## The vision (not a decision — a direction to research)

A **central trust layer**: one abstraction that all untrusted content and all untrusted execution flows through. It is:

- **Central**: a single choke point, not N guards. Tool results, extension frame buffers, provider output, markdown, session-file writes, and tool execution all pass through it.
- **Transparent**: the user sees what the layer is doing. Not a black box that silently strips or permits, but something that surfaces its decisions — what was blocked, what was allowed, why. The user has a feeling for what's happening.
- **Approachable**: "most sandboxing approaches are far too hard to touch." The abstraction is simple enough that a user can understand and configure it, not a labyrinth of seccomp profiles, namespace configs, and capability flags that only an expert can reason about.
- **A first-level building block**: not a post-parity nice-to-have, but a foundational primitive that everything else composes on top of. Tools, extensions, the terminal, and session I/O all build on this layer.
- **Deterministically rule-based, default-deny**: the core security model is **default-deny**, not default-allow. The system maintains a **whitelist** of explicitly approved actions (e.g. `ls`, `cat`, `grep`, `cargo test` for bash; specific paths for file access) and a **blacklist** of explicitly forbidden actions (e.g. `rm -rf /`, writes to `/etc`). Everything not on either list requires **human approval** before execution. The system never needs to enumerate every dangerous thing — it only needs to know what's safe (small, enumerable) and what's dangerous (small, enumerable). The vast unknown middle goes to the human. This is the opposite of Codex/Claude Code's model, which is **default-allow with a blacklist**: the system must know every dangerous command to block it, and anything not on the blacklist is allowed. That model is inverted: it can never be complete, and new attacks bypass the blacklist by definition. Default-deny is the same model as firewall rules, Deno's permission system, and macOS Gatekeeper.

## The trust boundaries (what flows through the layer)

1. **Tool results** — output of bash, read, write, edit, grep, and extension tools. Can contain arbitrary bytes including raw ANSI sequences (P20). The terminal-output dimension.
2. **Extension frame buffers** (ADR 0003) — extension UI arrives as pre-rendered retained buffers. An extension could emit raw ANSI or malicious content.
3. **Provider streaming output** — LLM responses. A hostile provider or MITM could inject content.
4. **Markdown content** — user and assistant messages parsed by pulldown-cmark. Malicious markdown could carry escape sequences.
5. **Session files** (ADR 0008/0016) — JSONL read on `/resume`. Must be safe for both pi-rs and pi.
6. **Tool execution** (the big one) — bash runs arbitrary commands, write/edit modify the filesystem, extension tools can do anything. The terminal-output dimension does not address this. This is OS-level sandboxing: process isolation, filesystem permissions, network access. ADR 0002 discusses per-extension Worker sandboxing (post-parity); tool-call sandboxing is a separate, harder problem.

## Open questions (need research + grilling before this ADR is decided)

1. **What is the abstraction?** A trait (`TrustBoundary`)? A crate (`pi-trust`)? A process boundary? A module in `pi-core`? The abstraction must be the single choke point, but its shape determines how everything composes on top of it. Too abstract and it's useless; too concrete and it's not a building block.
2. **What is the transparency mechanism?** How does the user see what's happening? Options: a UI surface (a status bar showing "blocked 3 sequences, allowed 12"), per-action prompts (like macOS Gatekeeper), logs (auditable but invisible), or a dashboard. "The user gets a feeling about what's happening" — what does that look like concretely?
3. **What does the layer actually do?** For terminal output: strip raw ANSI? Escape it? Render it inert via the cell grid (ADR 0004)? For tool execution: the default-deny model means every bash command, every file write, every network request is checked against the whitelist (auto-allow), the blacklist (auto-deny), or goes to human approval. How are rules expressed (glob patterns? regex? command prefix matching? path allowlists?)? How are rules stored (config file? session state? learned from past approvals?)? How does the user manage the whitelist (add a command permanently? per-session? per-project?)?
4. **How is it enforced?** Code review is not a security boundary. Type-level guarantees (the only `Write` impl that reaches stdout is the renderer's)? Runtime assertions? OS-level isolation (separate processes, seccomp, landlock)? The enforcement mechanism IS the security boundary — it must be structural, not conventional.
5. **What is the threat model?** Accidental (P20: test output, not malicious) vs hostile (attacker controls tool output) vs compromised extension vs malicious session file vs hostile provider. Different threats may need different guards. The layer must be designed for the strongest threat it addresses.
6. **Relationship to ADR 0002 (extension Worker sandboxing, post-parity) and ADR 0009 (fail-closed hooks).** ADR 0002 defers per-extension sandboxing to post-parity. Does this ADR pull it forward? ADR 0009 says hooks fail closed — does the trust layer also fail closed (refuse to render/execute rather than permit untrusted content)?
7. **How does it stay approachable?** seccomp, namespaces, landlock, capabilities — these are powerful but opaque. The owner's requirement is that the abstraction is touchable: a user can understand it, configure it, and trust it without being an expert. What does "approachable sandboxing" look like in practice? (Research: Deno's permission model, VS Code's trust model, browser sandboxing, Firejail/nsjail configs — what's the UX, not just the mechanism?)
8. **What is the MVP?** The full vision (central trust layer + OS-level tool sandboxing + transparency UI) is large. What's the first building block? P20's fix (sanitize tool output before storing in session JSONL) is the minimum, but the owner wants the architecture, not just the patch.

## What is NOT open (settled by P20 evidence)

- The P20 vulnerability is real and observed. Raw ANSI in tool output hijacks the terminal.
- pi's inline renderer is NOT sandboxed. pi-rs must not repeat this.
- The session file must be safe for pi (ADR 0008 interop). Sanitization on write is required regardless of the trust-layer decision.
- Scattered sanitization at N trust boundaries is insufficient (one miss is a vulnerability). A central choke point is the direction.

## Relationship to ADR 0027

ADR 0027 (the render-path sandbox) is a subset of this ADR. It addresses only the terminal-output dimension (boundaries 1-4). This ADR broadens the scope to include tool execution (boundary 6) and the transparency/approachability requirements. If this ADR is decided, ADR 0027 is subsumed. Until then, ADR 0027 remains a proposed partial fix for the terminal-output dimension only.
