# V1 bar is full parity, oracle-guided and measured by session replay

The first public release of pi-rs targets full feature parity with a pinned pi version - no reduced-scope release. This is viable because the rewrite is oracle-guided: pi's working implementation and test suite serve as a behavioral reference, and the port is executed with an LLM harness against that oracle. Parity is measured, not asserted: pi's test suite is ported alongside the code, and a corpus of real-world pi session files (JSONL, ADR 0008) is replayed through pi-rs - every entry must load and render without error, and re-saving must produce byte-identical JSONL (parity = the session corpus replays green).

## Oracle pin

Recorded at Phase 0 spike start (2026-07-24):

- Version: `0.82.0` of `@earendil-works/pi-coding-agent` (npm `latest` tag on spike-start day, [registry record](https://registry.npmjs.org/@earendil-works/pi-coding-agent/latest))
- Canonical repo: [`earendil-works/pi`](https://github.com/earendil-works/pi) (pi migrated from `badlogic/pi-mono` to `earendil-works/pi` before the spike, [v0.82.0 release notes](https://github.com/earendil-works/pi/releases/tag/v0.82.0)). The `badlogic/pi-mono` repo remains as a stale mirror with v0.81.1 tagged latest there
- License: MIT ([LICENSE](https://github.com/earendil-works/pi/blob/v0.82.0/LICENSE)), the vendoring premise of the Phase 0 host-impl strategy decision
- Type declarations: `./dist/index.d.ts` present in the published package (the Phase 0 API-surface extraction source)

Re-baseline is deliberate: a later milestone may re-pin by recording a new entry here and superseding this one, never by silent edit.

### Post-spike drift (2026-07-25 audit)

npm `latest` moved to `0.82.1` the day after the spike (2026-07-25). v0.82.1 adds Claude Opus 5, `ANTHROPIC_AUTH_TOKEN` bearer auth for Anthropic-compatible gateways, and llama.cpp catalog persistence ([release notes](https://github.com/earendil-works/pi/releases)). The Oracle is **held at 0.82.0** deliberately: the spike evidence, the extension API surface extraction, and the vendored runtime all target 0.82.0, and re-baselining mid-Phase-1 would churn the contract for no parity gain. 0.82.1's auth/provider additions are noted as forward-compat items to fold in at Phase 3 provider/auth work. Re-baseline is deferred until a milestone where the drift is worth the re-vendor cost, recorded here, never silent. See `docs/oracle-drift-audit.md` finding D1.

## Considered Options

- Dogfoodable daily-driver v1 with deferred features - rejected: the author's release bar is full parity, and oracle-guided LLM porting compresses the usual multi-month parity cost
- Unpinned parity ("match pi as it evolves") - rejected: a receding horizon. The oracle is a pinned pi version, re-baselined deliberately per milestone

## Consequences

- Ordering constraint: the daily-driveable slice (TUI, built-in tools, sessions, Extension Host) is built first and dogfooded while remaining parity work proceeds - a validation checkpoint, not a scope cut, so foundation-level UX mistakes (alt-screen selection/copy-mode, Deno host under real load, terminal quirks) surface in weeks, not at the end
- Code-generation speed does not compress calendar-time validation. The dogfood checkpoint exists precisely because real daily use is the only test for ADR 0004's UX bets
- A session replay harness over the real-world JSONL corpus becomes a first-class test asset of the project. Streaming performance tests use synthetic token-timing workloads
- Nothing is published to registries (crates.io, brew) before the parity bar is met
