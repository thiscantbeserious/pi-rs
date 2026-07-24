# pi-rs - Agent Instructions

Rust rewrite of the [pi coding agent](https://github.com/earendil-works/pi). **Status: Phase 1 (walking skeleton) active.** Next milestone, deliverables, and exit gates live in **docs/ROADMAP.md** — phases are dependency-ordered; do not start a phase before its gate passes.

## Read first (in order)

1. **docs/PHILOSOPHY.md** — working philosophy: design at the last responsible moment, grow working systems, types as the design language. Carries the code rules (§5), the TDD loop (§6), and the sourced-facts rule (§9).
2. **docs/GOALS.md** — three goals in strict priority: (1) streaming smoothness, (2) concurrent-core quality, (3) feature parity. Lower number wins conflicts.
3. **CONTEXT.md** — canonical terms (Core, Extension Host, Host Protocol, Retained Message Model, Render Thread, Provider, Oracle, Session Corpus, Dogfood Checkpoint). Use them exactly. Avoid the listed synonyms.
4. **docs/adr/** — all accepted decisions (superseded ones carry a status note). Never contradict an ADR silently; propose a superseding ADR instead.
5. **docs/ROADMAP.md** — phases with objectives, deliverables, exit gates, scope fences, and armed triggers. Phase status and gate checkboxes are updated in the same PR that completes them.
6. **docs/pitfalls.md** — P1-P18 verified failure modes with mandatory guards. New pitfall observed: record it with evidence before fixing it.
7. **docs/research.md** — tooling verdicts and working defaults. Findings that change a decision need a superseding ADR.

## Workflow

- Implementation work lands via a feature branch + MR, reviewed before merge to `main`. Trivial docs/process commits (these rules, ADRs, ROADMAP checkbox updates tracking a completed phase) may go directly on `main`.
- TDD is the loop (PHILOSOPHY.md §6): RED (failing test specifying the behavior) → GREEN (minimum code to pass) → REFACTOR (under the passing test). No implementation lands without a failing test first.
- Parity-relevant code cites the pinned Oracle via git permalinks to the pinned version (recorded in ADR 0007) with line anchors (PHILOSOPHY.md §9.5). Where pi has no equivalent, say so explicitly rather than papering over with an analogy.
- Architecture changes update the README mermaid diagram FIRST, in the same PR as the change. Component status tracks ROADMAP phase exits. A stale diagram is a review-blocking defect.

## Non-negotiable invariants (see the cited ADR, not a paraphrase)

- Nothing blocks the render thread — no locks on the frame path, no IPC waits, no async (ADR 0013)
- Fail closed on hook/host failure. A human prompt is the only bypass (ADR 0009)
- Only the Core writes session files. AppendEntry routes over the protocol (ADR 0016)
- pi-rs must not write session entries pi cannot read while interop holds (ADR 0008)
- v1 platforms: Linux, macOS, WSL. Native Windows is post-parity (ADR 0014)
- Parity target is a pinned pi version, recorded and re-baselined in ADR 0007

## Commands & gates

- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass (CI enforces)
- `cargo test --workspace`, `cargo insta test --check --workspace`, `./tests/e2e_test.sh` against the release binary
- Coverage floor 70% (tarpaulin), ASan runs on main pushes and `ready-to-merge` PRs, SonarCloud gate: zero new issues
- Never commit failing gates. Never lower a gate to pass.

## Layout (ADR 0011)

`crates/{pi-core, pi-protocol, pi-replay, pi-rs}` + `host/` (Deno, vendored pi runtime per ADR 0021). Protocol types are defined once in `pi-protocol`, TypeScript definitions generated via ts-rs into `host/protocol/`, committed and freshness-checked by CI so schema drift is a build failure (ADR 0011).

## Open operational items (not in code)

- SonarCloud project import + `SONAR_TOKEN` secret, Codecov enable + `CODECOV_TOKEN` secret (CI gates fail until then)
