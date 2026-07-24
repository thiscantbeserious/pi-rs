# Rendering pitfalls — documented failures to design and test against

Field-verified failure modes from existing agent TUIs. Each entry names the guard (ADR or practice) that must prevent it in pi-rs, and should get an explicit regression test or manual check before the dogfood checkpoint (ADR 0007).

## From Claude Code (Ink/React)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P1 | Severe flicker under tmux/screen multiplexers from full-viewport redraws | [claude-code#37076](https://github.com/anthropics/claude-code/issues/37076) | Cell-diff frames + synchronized output (ADR 0004); **test explicitly inside tmux and screen** |
| P2 | Typing lag: rendering blocks keystroke processing, worsens under load | [claude-code#31194](https://github.com/anthropics/claude-code/issues/31194) | Input handling decoupled from frame drawing; input latency budget measured in CI-able benchmark |
| P3 | Screen corruption on exit/restart; stale session content left visible | [claude-code#42087](https://github.com/anthropics/claude-code/issues/42087) | Disciplined alt-screen enter/leave + terminal state restore on every exit path incl. panic hook (ADR 0004 exit transcript dump) |
| P4 | Status-indicator flashing (accessibility hazard for light-sensitive users) | claude-code accessibility reports | Spinner/status updates must repaint only their own cells, never trigger wide redraws (ADR 0003 damage granularity) |

## From Codex CLI (Rust/ratatui — proof native isn't sufficient)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P5 | Unstable scrollback: transcript content redraws above itself in long sessions | [codex discussion #1174](https://github.com/openai/codex/discussions/1174) | Retained message model owns scroll state; viewport is a pure function of it (ADR 0004); long-session soak test via replay harness (ADR 0007) |
| P6 | No way to scroll/review long responses exceeding the viewport | codex discussion #1174 | First-class scrollback viewport + search + copy-mode are v1 scope, not extras (ADR 0004 consequences) |
| P7 | Platform-dependent rendering (fine on macOS, broken on Windows Terminal/WSL) | codex discussion #1174 | Terminal capability detection + CI/testing matrix must include Windows Terminal and WSL, not just macOS/Linux |
| P8 | Unreliable auxiliary state display (context-window indicator) during agent execution | codex issue reports | Footer/status widgets read from the same retained state as everything else — no parallel ad-hoc state paths |

## From pi (differential line renderer — the closest baseline)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P9 | Line-granular diffing repaints whole lines for single-cell changes | pi tui docs (line-array component model) | Cell-granular diffing (ADR 0004) |
| P10 | JS runtime cost in the render path (GC pauses, event-loop scheduling between data and draw) | pi architecture (Node TUI) | No JS in the render path — extension UI arrives pre-rendered as retained buffers (ADR 0003) |
| P11 | Streaming markdown re-render cost grows with message size | observed streaming jank in JS agent TUIs | Frame coalescing + block-granular highlight caching + incremental tree-sitter on the tail block only (ADR 0010) |

## Process rule

When a new pitfall is observed in the wild (any agent TUI, incl. pi-rs itself), add it here with evidence and a named guard **before** fixing it.
