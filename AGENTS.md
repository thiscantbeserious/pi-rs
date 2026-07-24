# pi-rs — Agent Instructions

Rust rewrite of the [pi coding agent](https://github.com/badlogic/pi-mono): native Core (TUI, renderer, agent loop) + TypeScript Extension Host running existing pi extensions unmodified. **Status: planning complete, implementation not started.** The next milestone is Phase 0 (compat spike) in **docs/ROADMAP.md** — phases are dependency-ordered with explicit exit gates; do not start a phase before its gate.

## Read before working

1. **docs/GOALS.md** — three goals in strict priority: (1) streaming smoothness, (2) concurrent-core quality, (3) feature parity. Lower number wins conflicts.
2. **CONTEXT.md** — canonical terms (Core, Extension Host, Host Protocol, Retained Message Model, Render Thread, Provider, Host Proxy, Oracle, Session Corpus, Dogfood Checkpoint). Use them exactly; avoid the listed synonyms.
3. **docs/adr/** — 18 accepted decisions. Do not contradict an ADR silently; propose a superseding ADR instead.
4. **docs/ROADMAP.md** — phase order and exit gates.
5. **docs/pitfalls.md** — P1–P17 verified failure modes with mandatory guards. New pitfall observed → record it with evidence before fixing.
6. **docs/research.md** — tooling verdicts and working defaults (ts-rs, grammar bundling, codec benchmarking); findings that change a decision need a superseding ADR.

## Architecture in one breath

Alt-screen Rust TUI rendering from the Retained Message Model on a dedicated synchronous Render Thread (never awaits); tokio for everything async; extensions in a separate Deno process (Node fallback) speaking length-prefixed MessagePack over UDS; protocol types defined once in Rust, TypeScript generated; Providers behind a trait (Host Proxy first, Rust-native majors later); built-in tools Rust-native in the Core; pi session files read/written bidirectionally with the Core as sole writer; hooks awaited unbounded with heartbeat liveness, fail-closed; /reload = host restart; subagents stay an extension but headless mode is a Core design constraint.

## Layout (target: cargo workspace, ADR 0011)

- `crates/pi-core` — TUI, renderer, agent loop
- `crates/pi-protocol` — Host Protocol source of truth (+ TS codegen)
- `crates/pi-replay` — session-corpus replay harness
- `host/` — Deno/TS Extension Host (generated types)
- Currently a single-crate skeleton; migrate to the workspace with the first real module.

## Commands & gates

- `cargo fmt --all -- --check` and `cargo clippy -- -D warnings` must pass (CI enforces)
- `cargo test` (unit + integration), `cargo insta test --check` (snapshots), `./tests/e2e_test.sh` against the release binary
- Coverage floor 70% (tarpaulin); ASan runs on main pushes and `ready-to-merge` PRs; SonarCloud gate: zero new issues
- Never commit failing gates; never lower a gate to pass

## Non-negotiables

- Nothing blocks the render thread — no locks on the frame path, no IPC waits, no async
- Fail closed on hook/host failure; a human prompt is the only bypass
- pi-rs must not write session entries pi cannot read (ADR 0008) while interop holds
- Only the Core writes session files; appendEntry routes over the protocol (ADR 0016)
- v1 platforms: Linux, macOS, WSL. Native Windows is post-parity (ADR 0014)
- Parity target is a pinned pi version (record the pin when the spike starts — ROADMAP Phase 0)

## Open operational items (not in code)

- SonarCloud project import + SONAR_TOKEN secret; Codecov enable + CODECOV_TOKEN secret (CI gates fail until then)
- Oracle pin: record exact pi version at spike start (ADR 0007 + here)
- `docs/extension-api-surface.md`: extract from the pinned pi during Phase 0 — the Host Protocol coverage checklist
