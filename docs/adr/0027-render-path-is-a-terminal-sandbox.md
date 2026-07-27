# Status: PROPOSED — needs research. Terminal security: ANSI injection, trust boundaries, and tool-call sandboxing

**⚠ This ADR is open. It records the problem and the open questions, not a final decision. The render-path sandbox invariant is a candidate, not a settled design. Research and a grilling session are required before implementation.**

P20 (observed in production) is an ANSI injection vulnerability: tool results containing raw escape sequences (`\x1b[?1049h`) are rendered to the terminal by pi's inline renderer, which interprets them as real terminal commands. This trapped the user in alt-screen mode (no scrollback) and persisted across `/resume` because the sequences were stored in the session JSONL. An attacker who controls tool output can hijack the terminal.

This ADR attempts to scope the full security surface, not just the ANSI injection fix. The open questions span: where untrusted content enters the system, how each entry point is guarded, whether tool calls need OS-level sandboxing (not just terminal-output sandboxing), and how the session file stays safe for bidirectional pi interop (ADR 0008).

## The trust boundaries (open: which need guards, and what kind)

Untrusted content enters pi-rs at several points. Each is a potential attack surface:

1. **Tool results** — the output of bash, read, write, edit, grep, and extension tools. Can contain arbitrary bytes including raw ANSI sequences. This is the P20 vector.
2. **Extension frame buffers** (ADR 0003) — extension UI arrives as pre-rendered retained buffers (lines of text). An extension could emit raw ANSI in its `render(width)` output.
3. **Provider streaming output** — LLM responses (assistant messages, thinking blocks, tool calls). A hostile provider or a man-in-the-middle could inject escape sequences.
4. **Markdown content** — user messages and assistant messages are parsed by pulldown-cmark and rendered. Malicious markdown could carry escape sequences in code blocks or inline code.
5. **Session files** (ADR 0008/0016) — JSONL read on `/resume`. If the file contains raw ANSI (from a prior unsanitized tool result), pi's inline renderer will execute it. pi-rs must write sanitized content (ADR 0016: Core is sole session writer) so the file is safe for both pi-rs and pi.
6. **Tool execution itself** (open: this is the big one) — bash runs arbitrary commands, write/edit modify the filesystem, extension tools can do anything. The terminal-output sandbox does not address this. Does pi-rs need OS-level process sandboxing for tool calls? ADR 0002 discusses per-extension Worker sandboxing (post-parity). Tool-call sandboxing is a separate, harder question.

## Candidate guard: the render path is a sandbox (NOT DECIDED)

The render path could be structured as a sandbox: untrusted content enters cells as **data**, and the only bytes that reach stdout are the renderer's own controlled diff output (cursor moves, cell writes via `Buffer::diff`). No cell content is ever passed through to stdout as a raw terminal command. This is one invariant that covers boundaries 1-4 above.

**Why this is a candidate, not a decision:**

- It does not address boundary 5 (session files) — that needs sanitization on write, which is a separate code path (ADR 0016).
- It does not address boundary 6 (tool execution) — that needs OS-level sandboxing, which is a different and much harder problem.
- The invariant "stdout is only ever written via the renderer's controlled diff" must be **enforced**, not just documented. How? Code review? A type-level guarantee (the only `Write` impl that reaches stdout is the renderer's)? A test that asserts no raw stdout writes outside the render thread?
- The cell-diff renderer (ADR 0004) makes this *possible* by construction, but "possible" is not "guaranteed." The invariant must be designed into the code, not assumed.

## Open questions (need research + grilling before this ADR is decided)

1. **Is the render-path sandbox sufficient, or do we need defense in depth** (sanitize at storage AND render)? If the render path is truly sandboxed, storage sanitization is only needed for pi interop (ADR 0008), not for pi-rs's own safety. But a miss in the render-path sandbox is a vulnerability — defense in depth may be worth the cost.
2. **How is the sandbox invariant enforced?** A `Write` trait wrapper that only the renderer can construct? A compile-time guarantee (the stdout handle is never accessible outside the render thread)? A runtime assertion? Code review alone is not a security boundary.
3. **What about the session file?** Sanitize on write (ADR 0016) is the candidate, but what exactly is stripped? All `\x1b` sequences? Only terminal-control sequences (CSI, OSC, DCS)? Preserve formatting ANSI (colors, bold) for re-rendering, or strip everything? This affects pi interop (ADR 0008: pi reads the same files).
4. **Do tool calls need OS-level sandboxing?** bash runs arbitrary code. write/edit modify the filesystem. Extension tools can do anything. The terminal-output sandbox does not protect against a malicious bash command that deletes files, exfiltrates data, or launches processes. Is this ADR's scope (terminal security) or a separate ADR (tool-call sandboxing)? ADR 0002 discusses extension Worker sandboxing (post-parity); tool-call sandboxing may need its own ADR.
5. **What is the threat model?** Is the attacker a malicious tool result (accidental or hostile), a compromised extension, a hostile provider, or a malicious session file? Different threat models may need different guards. The P20 incident was accidental (test output), not malicious — but the same vector is exploitable.
6. **Relationship to ADR 0009 (fail-closed hooks).** ADR 0009 says hooks fail closed. Does the sandbox also fail closed (if sanitization fails, refuse to render rather than render raw content)?

## What is NOT open (settled by P20 evidence)

- The P20 vulnerability is real and observed. Raw ANSI in tool output hijacks the terminal. This is not theoretical.
- pi's inline renderer is NOT sandboxed (it writes content to stdout directly). pi-rs must not repeat this.
- The session file must be safe for pi (ADR 0008 interop). Whatever pi-rs stores, pi must be able to render without hijacking. This means sanitization on write is required regardless of the render-path sandbox decision.
