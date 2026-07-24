# pi-rs Agent Instructions

Rust rewrite of [pi](https://github.com/earendil-works/pi). Phase 1 (walking skeleton) is active.

Write short, clearly instructed sentences. No em dashes. No LLM slang (e.g. "load bearing"). No superficial words. Keep it simple and structured.

## Before you work

Read these in order. They override anything you think you know:

1. `docs/PHILOSOPHY.md`: code rules (§5), TDD loop (§6), sourced-facts rule (§9)
2. `docs/GOALS.md`: streaming > concurrent-core > parity; lower number wins
3. `CONTEXT.md`: terms; use them exactly, avoid the listed synonyms
4. `docs/adr/`: accepted decisions; never contradict silently, propose a superseding ADR
5. `docs/ROADMAP.md`: phases, exit gates, armed triggers; don't start a phase before its gate passes
6. `docs/pitfalls.md`: P1-P18; new pitfall, record with evidence before fixing
7. `docs/research.md`: tooling verdicts; a finding that changes a decision needs a superseding ADR

## Workflow

Every change follows one loop: understand, design, implement, verify.

### Understand

Read the ADRs, ROADMAP, and CONTEXT.md for the area. Find the unknowns. Verify anything factual against current sources, not your training data. If a requirement is unclear, ask the user. Do not guess on decisions that are hard to reverse.

### Design

Run `/grill-with-docs` to test the design against the ADRs and CONTEXT.md. Write a plan doc: the steps, the open decisions, the research findings. If the architecture changes, update the README mermaid diagram first, in the same PR. A stale diagram blocks review.

### Implement

Write the failing test first. It specifies the behavior. Make it pass with the minimum code. Refactor under the passing test. No implementation without a failing test first. For any behavior with a pi equivalent, cite the pinned Oracle (ADR 0007) with a git permalink and line anchors. If there is no pi equivalent, say so.

### Land

Implementation goes on a feature branch + MR. Only docs, rules, and ROADMAP checkbox updates go on `main`. MR review checks the implementation against the plan doc. Reconcile drift or update the plan. Never ignore it.

## Non-negotiables

- Render thread never waits: no locks, no IPC, no async on the frame path (ADR 0013)
- Fail closed on hook/host failure; human prompt is the only bypass (ADR 0009)
- Core is the sole session writer; AppendEntry routes over the protocol (ADR 0016)
- Don't write session entries pi can't read while interop holds (ADR 0008)
- v1: Linux, macOS, WSL. Native Windows is post-parity (ADR 0014)
- Parity target is a pinned pi version (ADR 0007)

## Gates (never commit failing, never lower to pass)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo insta test --check --workspace`
- `./tests/e2e_test.sh`
- 70% coverage (tarpaulin), ASan on main + `ready-to-merge`, SonarCloud zero new issues

## Layout (ADR 0011, ADR 0021)

`crates/{pi-core, pi-protocol, pi-replay, pi-rs}` + `host/` (Deno, vendored pi runtime). Protocol types in `pi-protocol`, generated via ts-rs into `host/protocol/`, freshness-checked by CI.

## Open items

- SonarCloud project import + `SONAR_TOKEN`, Codecov + `CODECOV_TOKEN` (CI gates fail until then)
