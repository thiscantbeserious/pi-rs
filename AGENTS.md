# pi-rs - Agent Instructions

Rust rewrite of the [pi coding agent](https://github.com/badlogic/pi-mono): native Core (TUI, renderer, agent loop) + TypeScript Extension Host running existing pi extensions unmodified. **Status: planning complete, implementation not started.** The next milestone is Phase 0 (compat spike) in **docs/ROADMAP.md** - phases are dependency-ordered with explicit exit gates. Do not start a phase before its gate.

## Read before working

1. **docs/SOUL.md** - the working philosophy, sourced: design in short bursts at the last responsible moment, grow working systems, types as the design language, tests as usage specifications, honesty over marketing. Carries the concrete code rules.
2. **docs/GOALS.md** - three goals in strict priority: (1) streaming smoothness, (2) concurrent-core quality, (3) feature parity. Lower number wins conflicts.
3. **CONTEXT.md** - canonical terms (Core, Extension Host, Host Protocol, Retained Message Model, Render Thread, Provider, Host Proxy, Oracle, Session Corpus, Dogfood Checkpoint). Use them exactly. Avoid the listed synonyms.
4. **docs/adr/** - 18 accepted decisions. Do not contradict an ADR silently. Propose a superseding ADR instead.
5. **docs/ROADMAP.md** - phase order and exit gates.
6. **docs/pitfalls.md** - P1-P17 verified failure modes with mandatory guards. New pitfall observed: record it with evidence before fixing.
7. **docs/research.md** - tooling verdicts and working defaults (ts-rs, grammar bundling, codec benchmarking). Findings that change a decision need a superseding ADR.

## Architecture in one breath

Alt-screen Rust TUI rendering from the Retained Message Model on a dedicated synchronous Render Thread (never awaits). Tokio for everything async. Extensions in a separate Deno process (Node fallback) speaking length-prefixed MessagePack over UDS. Protocol types defined once in Rust, TypeScript generated, Providers behind a trait (Host Proxy first, Rust-native majors later). Built-in tools Rust-native in the Core. Pi session files read/written bidirectionally with the Core as sole writer. Hooks awaited unbounded with heartbeat liveness, fail-closed, /reload = host restart. Subagents stay an extension but headless mode is a Core design constraint.

## Layout (target: cargo workspace, ADR 0011)

- `crates/pi-core` - TUI, renderer, agent loop
- `crates/pi-protocol` - Host Protocol source of truth (+ TS codegen)
- `crates/pi-replay` - session-corpus replay harness
- `host/` - Deno/TS Extension Host (generated types)
- Currently a single-crate skeleton. Migrate to the workspace with the first real module.

## Commands & gates

- `cargo fmt --all -- --check` and `cargo clippy -- -D warnings` must pass (CI enforces)
- `cargo test` (unit + integration), `cargo insta test --check` (snapshots), `./tests/e2e_test.sh` against the release binary
- Coverage floor 70% (tarpaulin), ASan runs on main pushes and `ready-to-merge` PRs, SonarCloud gate: zero new issues
- Never commit failing gates. Never lower a gate to pass

## Code rules (docs/SOUL.md section 5, enforced in review)

- Files ~400 lines max, functions ~20 lines max (dispatch-only routers exempt), nesting 3 levels max
- Single responsibility: a function described with "and" gets split
- Parse, don't validate: validity lives in types at boundaries, invalid states unrepresentable
- Document the non-obvious only (connections, side effects, constraints)
- One concern per PR, reviewable in one sitting
- Docs style: no em dashes, no semicolons in prose

## Non-negotiables

- Nothing blocks the render thread - no locks on the frame path, no IPC waits, no async
- Fail closed on hook/host failure. A human prompt is the only bypass
- pi-rs must not write session entries pi cannot read (ADR 0008) while interop holds
- Only the Core writes session files. AppendEntry routes over the protocol (ADR 0016)
- v1 platforms: Linux, macOS, WSL. Native Windows is post-parity (ADR 0014)
- Parity target is a pinned pi version (record the pin when the spike starts - ROADMAP Phase 0)

## Open operational items (not in code)

- SonarCloud project import + SONAR_TOKEN secret, Codecov enable + CODECOV_TOKEN secret (CI gates fail until then)
- Oracle pin: record exact pi version at spike start (ADR 0007 + here)
- `docs/extension-api-surface.md`: extract from the pinned pi during Phase 0 - the Host Protocol coverage checklist
