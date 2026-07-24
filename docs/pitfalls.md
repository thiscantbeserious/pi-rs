# Rendering pitfalls - documented failures to design and test against

Field-verified failure modes from existing agent TUIs. Each entry names the guard (ADR or practice) that must prevent it in pi-rs, and should get an explicit regression test or manual check before the dogfood checkpoint (ADR 0007).

## From Claude Code (Ink/React)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P1 | Severe flicker under tmux/screen multiplexers from full-viewport redraws | [claude-code#37076](https://github.com/anthropics/claude-code/issues/37076) | Cell-diff frames + synchronized output (ADR 0004), **test explicitly inside tmux and screen** |
| P2 | Typing lag: rendering blocks keystroke processing, worsens under load | [claude-code#31194](https://github.com/anthropics/claude-code/issues/31194) | Input handling decoupled from frame drawing. Input latency budget measured in CI-able benchmark |
| P3 | Screen corruption on exit/restart. Stale session content left visible | [claude-code#42087](https://github.com/anthropics/claude-code/issues/42087) | Disciplined alt-screen enter/leave + terminal state restore on every exit path incl. panic hook (ADR 0004 exit transcript dump) |
| P4 | Status-indicator flashing (accessibility hazard for light-sensitive users) | claude-code accessibility reports | Spinner/status updates must repaint only their own cells, never trigger wide redraws (ADR 0003 damage granularity) |

## From Codex CLI (Rust/ratatui - proof native isn't sufficient)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P5 | Unstable scrollback: transcript content redraws above itself in long sessions | [codex discussion #1174](https://github.com/openai/codex/discussions/1174) | Retained message model owns scroll state. Viewport is a pure function of it (ADR 0004). Long-session soak test via replay harness (ADR 0007) |
| P6 | No way to scroll/review long responses exceeding the viewport | codex discussion #1174 | First-class scrollback viewport + search + copy-mode are v1 scope, not extras (ADR 0004 consequences) |
| P7 | Platform-dependent rendering (fine on macOS, broken on Windows Terminal/WSL) | codex discussion #1174 | Terminal capability detection + CI/testing matrix must include Windows Terminal and WSL, not just macOS/Linux |
| P8 | Unreliable auxiliary state display (context-window indicator) during agent execution | codex issue reports | Footer/status widgets read from the same retained state as everything else - no parallel ad-hoc state paths |

## From pi (differential line renderer - the closest baseline)

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P9 | Line-granular diffing repaints whole lines for single-cell changes | [pi tui docs, render(width) returns line arrays](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/tui.md) | Cell-granular diffing (ADR 0004) |
| P10 | JS runtime cost in the render path (GC pauses, event-loop scheduling between data and draw) | [pi is a TypeScript/Node TUI](https://github.com/badlogic/pi-mono) | No JS in the render path - extension UI arrives pre-rendered as retained buffers (ADR 0003) |
| P11 | Streaming markdown re-render cost grows with message size | Adjacent evidence: [claude-code#31194](https://github.com/anthropics/claude-code/issues/31194) (render-blocked typing under streaming). For pi this is an observation, to be quantified in Phase 2 benchmarks | Frame coalescing + block-granular highlight caching + incremental tree-sitter on the tail block only (ADR 0010) |

## Technique pitfalls - traps inside our own chosen techniques

Research-verified pitfalls in the exact mechanisms pi-rs claims as its advantages. These are the ways our own architecture can fail if implemented naively.

| # | Pitfall | Evidence | Guard in pi-rs |
|---|---------|----------|----------------|
| P12 | Synchronized output (mode 2026) is not universal: tmux buffers with a 1s timeout, clears the mode on pane resize, and has a known leak of partial frames when BSU/ESU spans pane reads | [tmux#4744](https://github.com/tmux/tmux/pull/4744), [tmux#4983](https://github.com/tmux/tmux/issues/4983) | Query support via `CSI ? 2026 $ p` at startup. Degrade gracefully (cell-diff still minimizes tearing without 2026). Keep frames small and BSU/ESU pairs tight. Tmux in the test matrix (extends P1) |
| P13 | Terminals disagree wildly on character width: the same ZWJ emoji advances the cursor 2, 4, 5, or 6 cells depending on the terminal. Wcwidth() is insufficient for grapheme clusters, VS16, and East-Asian-ambiguous width | [Grapheme clusters in terminals (Mitchell Hashimoto)](https://mitchellh.com/writing/grapheme-clusters-in-terminals) | Grapheme-cluster-aware width (unicode-segmentation), detect mode 2027 where available. A width-testing corpus (emoji, ZWJ, CJK, combining marks) in snapshot tests - cell-diff corruption from width drift is the failure mode |
| P14 | Advanced keyboard protocols (kitty) have edge cases: modifier-only keypresses clearing selection, protocol state leaking into the shell after suspend (CTRL-Z) | [kitty keyboard protocol docs](https://sw.kovidgoyal.net/kitty/keyboard-protocol/), vim reports | Enable/disable protocol state on every suspend/resume path (SIGTSTP/SIGCONT), not just enter/exit |
| P15 | Panic/exit paths that skip terminal restore leave raw mode + alt screen stuck. Cursor position bugs overlap the backtrace with the shell prompt. Zero-size terminals panic naive renderers | [ratatui panic-hooks recipe](https://ratatui.rs/recipes/apps/panic-hooks/) | Single owned restore path installed as a panic hook before first draw (extends P3, ADR 0013). Explicit zero-size guard in the resize handler |
| P16 | Duplicate crossterm/terminal-backend versions in one binary cause event-queue mismatches and state-restore races | [ratatui-website#876](https://github.com/ratatui/ratatui-website/issues/876) | Workspace-level dependency unification, `cargo deny` duplicate-version check in CI |
| P17 | MessagePack decode in JS is not automatically faster than JSON.parse (V8 optimizes JSON heavily for text payloads). The msgpack win is binary-safety and payload size, not raw text decode speed | [msgpack-javascript](https://github.com/msgpack/msgpack-javascript), V8 JSON.parse benchmarks | ADR 0006 stands on binary-safety. Benchmark host-side codecs (@msgpack/msgpack vs msgpackr) during protocol bring-up. Keep the codec behind an interface so it is swappable without protocol changes |
| P18 | Tree-sitter error recovery quality varies by grammar: mid-typing edits can trigger "catastrophically bad" recovery in some grammars, exactly the incomplete-code state streaming produces | [tree-sitter#2404](https://github.com/tree-sitter/tree-sitter/issues/2404) | Per-language highlight quality tests with a truncated/incomplete code corpus. A grammar with bad recovery falls back to plain styling for the growing tail block (ADR 0010) |

## Process rule

When a new pitfall is observed in the wild (any agent TUI, incl. pi-rs itself), add it here with evidence and a named guard **before** fixing it.
