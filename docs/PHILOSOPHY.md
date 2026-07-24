# Architectural Philosophy

How this project works. Not what it builds (GOALS.md) or what was decided (docs/adr/), but the working philosophy every session and every contributor operates by. Claims here are sourced, not vibes.

## 1. Design is the work, code is an artifact

Design happens in short bursts, hours at most, immediately before implementation, not months ahead [[1]](https://holub.com/the-problem-with-design/). Big upfront specification is waterfall in disguise: the larger a spec, the less accurate it becomes [[1]](https://holub.com/the-problem-with-design/). This repo's ADRs do not contradict this. Each one was forced by a real dependency at its last responsible moment, the point where not deciding would have eliminated alternatives [[4]](https://blog.codinghorror.com/the-last-responsible-moment/). They record rationale so future readers do not re-fight settled battles [[6]](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions), and every one of them is cheap to supersede (ROADMAP path-change triggers exist precisely so no decision becomes sunk cost).

## 2. Grow a working system, never assemble a designed one

A complex system that works is invariably found to have evolved from a simple system that worked [[2]](https://blog.holub.com/p/galls-law). The ROADMAP is Gall's law operationalized: each phase exits with a smaller working system (host round-trip, renderer replaying real sessions, dogfoodable slice) that the next phase grows. Phase 1 is a walking skeleton, the thinnest end-to-end line through Core, protocol, and host, built first so integration risk surfaces before feature work [[5]](https://codeclimate.com/legacy/kickstart-your-next-project-with-a-walking-skeleton/). We never build layers in isolation and integrate at the end.

## 3. The architect codes

Architecture divorced from implementation is fantasy. Decisions in this repo are validated by executable evidence: the compat spike before the runtime is locked, benchmarks before performance claims, the dogfood checkpoint before UX bets are declared won (ADR 0007). A decision that cannot be tested by running something is a guess and gets labeled as one.

## 4. Types are the design language

The Rust discipline here is type-driven correctness: parse, don't validate. Data is validated once at a boundary and its validity is encoded in the type, so invalid states are unrepresentable downstream [[3]](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/). Applied concretely:

- Protocol messages are typed once in pi-protocol and generated outward, drift is a compile error (ADR 0011)
- State machines (host lifecycle, hook verdicts, focus ownership) use typestate-style APIs where transitions that must not happen do not compile [[3]](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)
- Public APIs follow the Rust API Guidelines [[7]](https://rust-lang.github.io/api-guidelines/)
- unsafe is justified only by FFI, a novel abstraction, or measured performance need, always with a documented invariant [[8]](https://microsoft.github.io/rust-guidelines/)

## 5. Code rules (carried from agent-session-recorder, proven in production)

- Files: ~400 lines max, no exceptions. Split into modules by responsibility.
- Functions: ~20 lines max. Dispatch-only routers may exceed.
- Single responsibility: if describing a function needs "and", split it.
- Nesting: 3 levels max. Use early returns and extraction.
- Document the non-obvious only: connections, side effects, constraints. Never restate the signature.
- SOLID adapted for Rust: one reason to change per module, extend by composition and new types, abstractions (traits) at system boundaries only, concrete internals.
- KISS, YAGNI, DRY in that spirit: complexity must justify itself, do not build for hypothetical futures, duplication beats the wrong abstraction.
- Trade-off priority: Simplicity > Maintainability > Testability > Consistency. Performance is designed at the architecture level (GOALS.md goal 1) and otherwise optimized when data exists.

## 6. Tests are usage specifications

TDD here is a design strategy, not a QA afterthought: tests describe how a component is meant to be used, and stay stable across refactoring so the code underneath can change safely [[1]](https://holub.com/the-problem-with-design/). The parity suite (ported oracle tests plus session-corpus replay, ADR 0007) is the project-level version of the same idea: an executable specification of what "done" means.

The operative loop is RED-GREEN-REFACTOR: write the failing test first (RED) so it specifies the behavior before any implementation exists, make it pass with the minimum code (GREEN), then refactor under the passing test. A change that adds implementation without a failing test first is out of process — the test is the specification, and a specification written after the fact rationalizes rather than specifies.

## 7. Small iterations, one concern each

Each PR addresses one concern, small enough to review in one sitting. Related changes stay together, unrelated refactors get their own branch. Many small shippable cycles beat one big-bang change. The CI gates (fmt, clippy -D warnings, tests, snapshots, coverage floor, sanitizers, zero new Sonar issues) are non-negotiable and never lowered to pass.

## 8. The language is the model

CONTEXT.md is a ubiquitous language in the domain-driven sense: one canonical term per concept, synonyms explicitly banned, and the code uses the same words as the documentation [[9]](https://martinfowler.com/bliki/UbiquitousLanguage.html). When a conversation and the glossary disagree, one of them is wrong and the session stops to fix it.

## 9. Honesty over marketing

Claims get verified against evidence before they enter documentation, and corrections are made in public (the README's Codex claim was corrected when research contradicted it, msgpack's speed claim was narrowed when benchmarks said otherwise, the Deno sandbox claim was re-scoped when permission granularity was checked). docs/research.md and docs/pitfalls.md exist so that what we learned stays sourced and testable.

This is a standing rule, not a habit:

1. Whenever a document states a fact about the outside world (a tool's behavior, an API's capability, a performance characteristic, a comparison), that fact gets verified by research before it lands, and the source is linked inline where the claim is made. Citation style: numbered inline links in the form [[n]](url) at the exact claim, with a matching numbered Sources list at the bottom of the document, so every number maps one claim to one source. Code-level sources (a permalink to the implementing file) are preferred where they exist.
2. A claim that cannot be sourced is either rewritten as an explicit assumption ("we assume X, to be verified in phase Y") or removed.
3. When verification contradicts an existing claim, the document is corrected immediately. If the correction touches an accepted decision (an ADR, a roadmap gate, a goal), it is not silently fixed: the finding goes to the project owner, who decides between correcting in place and superseding the decision.
4. Research findings live in docs/research.md with their sources, so the evidence trail survives the session that found it.
5. Implementation parity against the pinned Oracle (ADR 0007) follows the same sourcing discipline. When implementing behavior that has a pi equivalent — session format, the extension API, provider model, built-in tool defaults — the implementation is verified against the pinned version's actual code at `earendil-works/pi@v0.82.0`, and the test or doc cites the permalink with line anchors. Where there is no pi equivalent (the Host Protocol wire format is pi-rs-native, since pi is single-process), that absence is stated explicitly rather than papering over with an analogy. The Oracle is the parity target; citing it is how parity stays measured at the code level, not just at the replay level.

## Sources

1. The problem with design (and how agile design actually works): <https://holub.com/the-problem-with-design/>
2. Gall's law, complex systems evolve from simple working systems: <https://blog.holub.com/p/galls-law>
3. Parse, don't validate (type-driven design): <https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/>
4. The last responsible moment (lean decision timing): <https://blog.codinghorror.com/the-last-responsible-moment/>
5. Walking skeleton (Cockburn) and tracer bullets (The Pragmatic Programmer): <https://codeclimate.com/legacy/kickstart-your-next-project-with-a-walking-skeleton/> and <https://www.artima.com/articles/tracer-bullets-and-prototypes>
6. Documenting architecture decisions (Nygard, 2011): <https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions>
7. Rust API Guidelines: <https://rust-lang.github.io/api-guidelines/>
8. Unsafe discipline, Pragmatic Rust Guidelines: <https://microsoft.github.io/rust-guidelines/>
9. Ubiquitous language (domain-driven design): <https://martinfowler.com/bliki/UbiquitousLanguage.html>
