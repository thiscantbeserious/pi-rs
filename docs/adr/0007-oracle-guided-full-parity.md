# V1 bar is full parity, oracle-guided and measured by differential replay

The first public release of pi-rs targets full feature parity with a pinned pi version — no reduced-scope release. This is viable because the rewrite is oracle-guided: pi's working implementation and test suite serve as a behavioral reference, and the port is executed with an LLM harness against that oracle. Parity is measured, not asserted: pi's test suite is ported alongside the code, and real-world pi sessions recorded with agent-session-recorder are replayed differentially against pi-rs (parity = the recorded session corpus replays green).

## Considered Options

- Dogfoodable daily-driver v1 with deferred features — rejected: the author's release bar is full parity, and oracle-guided LLM porting compresses the usual multi-month parity cost
- Unpinned parity ("match pi as it evolves") — rejected: a receding horizon; the oracle is a pinned pi version, re-baselined deliberately per milestone

## Consequences

- Ordering constraint: the daily-driveable slice (TUI, built-in tools, sessions, Extension Host) is built first and dogfooded while remaining parity work proceeds — a validation checkpoint, not a scope cut, so foundation-level UX mistakes (alt-screen selection/copy-mode, Deno host under real load, terminal quirks) surface in weeks, not at the end
- Code-generation speed does not compress calendar-time validation; the dogfood checkpoint exists precisely because real daily use is the only test for ADR 0004's UX bets
- A differential replay harness (agr cast corpus as golden files) becomes a first-class test asset of the project
- Nothing is published to registries (crates.io, brew) before the parity bar is met
