# Built-in tools are Rust-native in the Core

pi's built-in tools (read, edit, write, bash, grep/find) are ported to Rust inside the agent loop: no IPC on the hottest tool path, functional when the Extension Host is down, and no credentials or file access routed through the extension process. Default behavior must match pi exactly (truncation limits, output formats) — verified by the ported oracle test suite (ADR 0007).

Want-to-have, explicitly not hard-tied: token-optimized output modes (the current pi tool outputs are sometimes suboptimal for context budgets). Any such optimization is opt-in and must never change the parity defaults.

## Considered Options

- All tools in the host reusing pi's TS implementations — rejected: every file read crosses IPC and the Core is helpless without the host

## Consequences

- Tool behavior parity is test-enforced, not assumed; output-format drift is a parity bug
- Extension-registered tools still execute in the host (ADR 0001)
