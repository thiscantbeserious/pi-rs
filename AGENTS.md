# pi-rs Agent Instructions

Rust rewrite of [pi](https://github.com/earendil-works/pi). Phase 2 (render core) is active.

Write short, clearly instructed sentences. No em dashes. No LLM slang (e.g. "load bearing"). No superficial words. Keep it simple and structured.

## Before you work

Read these in order. They override anything you think you know:

1. `docs/PHILOSOPHY.md`: code rules (§5), TDD loop (§6), sourced-facts rule (§9)
2. `docs/GOALS.md`: streaming > concurrent-core > parity; lower number wins
3. `CONTEXT.md`: terms; use them exactly, avoid the listed synonyms
4. `docs/adr/`: accepted decisions; never contradict silently, propose a superseding ADR
5. `docs/ROADMAP.md`: phases, exit gates, armed triggers; don't start a phase before its gate passes
6. `docs/pitfalls.md`: P1-P20; new pitfall, record with evidence before fixing
7. `docs/research.md`: tooling verdicts; a finding that changes a decision needs a superseding ADR

## Setup

Git hooks (pre-push: fmt + clippy + test) auto-install via `cargo-husky`
when you run `cargo test` or `cargo build` for the first time. No manual
setup needed. Skip on a single push with `git push --no-verify`.

## Workflow

Every change follows one loop: understand, design, implement, land.

1. Read the ADRs, ROADMAP, and CONTEXT.md for the area you are working in.
2. Find the unknowns. Verify anything factual against current sources, not your training data.
3. Ask the user when a requirement is unclear. Do not guess on decisions that are hard to reverse.
4. Run `/grill-with-docs` to test the design against the ADRs and CONTEXT.md.
5. Write a plan doc with the steps, the open decisions, and the research findings.
6. If the architecture changes, update the README mermaid diagram first, in the same PR.
7. Write the failing test first. It specifies the behavior.
8. Make the test pass with the minimum code. Refactor under the passing test.
9. For any behavior with a pi equivalent, cite the pinned Oracle (ADR 0007) with a git permalink and line anchors. If there is no pi equivalent, say so.
10. Put implementation on a feature branch and open an MR. Only docs, rules, and ROADMAP checkbox updates go on `main`.
11. In MR review, check the implementation against the plan doc. Reconcile drift or update the plan. Never ignore it.

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

`crates/{pi-core, pi-render, pi-protocol, pi-replay, pi-rs, pi-session}` + `host/` (Deno, vendored pi runtime). Protocol types in `pi-protocol`, generated via ts-rs into `host/protocol/`, freshness-checked by CI. Renderer in `pi-render` (ADR 0026).

## Open items

- SonarCloud project import + `SONAR_TOKEN`, Codecov + `CODECOV_TOKEN` (CI gates fail until then)
