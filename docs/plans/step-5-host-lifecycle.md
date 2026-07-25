# Step 5 plan: host lifecycle (handshake, heartbeat, restart, /reload)

Implements ADR 0023 (the grilled mechanics) on top of the step-3 transport and step-4 codec. This is the step that delivers exit gate 2 (`kill -9` the host, Core survives + auto-respawns). No new design decisions; ADR 0023 settled the semantics, this grill (Q1-Q6 below) settled the mechanics the plan glossed over.

## Grill resolutions (the mechanics ADR 0023 left open)

- **Q1 — Hung as a state**: kept per ADR 0023 Q4. 8 states. A transient empty state is a deliberate ADR choice for log clarity; not superseded for marginal gain.
- **Q2 — Typestate pattern**: Pattern A. Inner typestate structs carry their invariants (`Booting` holds the child handle, `Ready` holds a `ConnTriple` + miss count, `BackingOff`/`Reconnecting` hold the deadline, `Draining` holds the `ConnTriple`). Transition methods on the inner types consume `self`. Enum-wrap (`enum HostState { Stopped(Stopped), Booting(Booting), ... }`) for supervisor storage. The single match site in the supervisor is the only mutation point (ADR 0023 Q5: single writer). Illegal transitions don't compile because the method doesn't exist on that inner type.
- **Q3 — Event routing**: all messages upstream, supervisor routes. One `Message` type each direction on two `mpsc` channels. The connection task is dumb I/O (~15 lines: read frame → upstream; downstream → write frame). The supervisor owns the heartbeat timing (5s interval via `select!`), the miss count, and the `Hung` decision. Echo is handled by the supervisor sending `EchoResponse` downstream on `EchoRequest` upstream. Simpler than custom event/command enums; the supervisor absorbs I/O timing state, acceptable since it's the single writer.
- **Q4 — Chaos test host source**: mock host as a spawned Rust process. The chaos test is a Core supervision property, not a host-binary property. The real host binary's correctness is step 6's Deno lane job. `cargo test` works without Deno installed.
- **Q5 — /reload in Phase 1**: expose `supervisor.reload()` (async, returns on `Ready` or errors on crash-loop), test it. The graceful-drain mechanism (`Shutdown{drain:true}` → `ShutdownAck` → `Stopped` → respawn) is built and proven now; Phase 3 wires the TUI trigger to call `reload()`. Honors ADR 0017's "crash recovery and /reload share one tested lifecycle path."
- **Q6 — Mock host structure**: `[[bin]]` `mock-host` at `crates/pi-core/src/bin/mock_host.rs` (`harness = false`), `--mode` arg: `normal` (handshake, pong, respond to Shutdown), `exit-immediately` (crash-loop test), `go-silent-after-handshake` (Hung test without kill -9).
- **Q7 — Heartbeat owner (design pass)**: the supervisor owns the heartbeat timer + miss count, NOT the connection task. This supersedes ADR 0023 Q5's original "connection task sends heartbeats" line, which contradicted the Q3 grill resolution. The plan's Q3 was right; the ADR was wrong on that line. ADR 0023 Q5 updated.
- **Q8 — ConnTriple home (design pass)**: `Ready` and `Draining` hold a `ConnTriple` struct (upstream receiver, downstream sender, connection task handle, child process). `Ready` also holds the miss count. Transitions move or drop the triple: `on_hung` drops it (cancels the task, kills the child), `on_drain` moves it into `Draining`. Honest typestate per Q2: the state owns what it needs, no `Option<ConnTriple>` side field hiding the connection's lifetime. ADR 0023 Q4/Q5 updated.

## Scope

1. **The 8-state machine** with Pattern A typestate (Q2): inner structs (`Stopped`, `Booting{child}`, `Ready{conn, misses}`, `Hung`, `Draining{conn}`, `BackingOff{deadline}`, `Reconnecting{deadline}`, `CrashLooping{crash_count}`) + `enum HostState` wrap. Transition methods on inner types consume `self`. Illegal transitions don't compile (PHILOSOPHY.md §4).
2. **The supervisor task** (owns `HostState`, crash count, backoff timer; spawns the host process + a connection task; receives `Message` upstream via channel; sends `Message` downstream via channel; single writer of lifecycle state per ADR 0023 Q5). Owns heartbeat timing (5s interval) and miss count (Q3).
3. **The connection task** (dumb I/O: read frame → upstream channel; downstream channel → write frame. ~15 lines. No logic, no heartbeat timer — that's the supervisor's per Q3).
4. **The handshake dance** (host sends `Handshake`, Core checks `protocol_version` against `PROTOCOL_VERSION`, exact-match-or-refuse per ADR 0022 Q3, sends `HandshakeAck` on accept, closes + `HandshakeRejected` on mismatch).
5. **The heartbeat loop** (supervisor sends `Heartbeat` at 5s via `select!`, counts missed `Pong`s, 3 consecutive → `Hung` per ADR 0022 Q7).
6. **Restart backoff** (exponential 1→2→4→8→30s cap, 5 consecutive failed boots → `CrashLooping` → native prompt, crash count resets on successful handshake per ADR 0023 Q2).
7. **`/reload` path** via `supervisor.reload()` (Q5): graceful `Shutdown{drain:true}` → `ShutdownAck` → `Stopped` → respawn → `Ready`.
8. **The native prompt stub** (log to stderr + read `r`/`b`/`a` from stdin, returns `FailureDecision::{Restart,BypassOnce,AbortTurn}` per ADR 0023 Q3).
9. **The mock-host binary** (`crates/pi-core/tests/mock_host.rs`, `[[bin]]` `mock-host`, `harness=false`, `--mode` arg per Q6): speaks the 9-message protocol, controllable misbehavior for the tests.
10. **The real host binary** (`host/main.ts`: connects to `PI_RS_HOST_SOCKET`, sends `Handshake`, replies `Pong`, handles `Shutdown{drain:true}` with `ShutdownAck`, echoes `EchoRequest`→`EchoResponse`). Compiled with `deno compile` per ADR 0023 Q1. Not used by the chaos test (Q4); smoke-tested in step 6's Deno lane.
11. **Exit gate 2 chaos test** (spawn `mock-host --mode normal` via the supervisor, `kill -9` it, assert Core detects `Hung` within ~15s → `Reconnecting` → backoff → respawn → new mock handshakes → Core back in `Ready`).

## Research findings (verified against current sources, not training data)

- `deno compile` works for the Phase 1 host (codec + framing + protocol types, no full pi runtime). Verified in the ADR 0023 grill: compiles clean, runs standalone, encodes a `Heartbeat` in 16 bytes. Recorded in `docs/research.md`.
- tokio 1.53.1 (already a dep): `tokio::process::Command` for spawning, `tokio::time::{interval, sleep}` for heartbeats/backoff, `tokio::select!` for the supervisor loop, `mpsc` channel for upstream/downstream. Features needed: `process`, `time`, `macros`, `rt`, `sync` (plus the existing `net`, `io-util`).
- The `FailureDecision` enum is the Phase 3 handoff point (ADR 0023 Q3).
- `[[bin]]` with `harness = false` at a `tests/` path is the conventional Rust pattern for a test-helper binary.

## TDD order (RED before GREEN for each)

1. **State machine (Pattern A typestate)**: a test that each legal transition is callable. Illegal transitions are enforced by the method not existing on the inner type (compile-time, no runtime test needed). RED: no `HostState` type. GREEN: the inner structs + enum + transition methods.
2. **Handshake validation**: a test that a matching `protocol_version` yields `HandshakeAck`, a mismatch yields `HandshakeRejected` + close. RED: no validation. GREEN: the validate function (pure, takes `Handshake` + expected version).
3. **Backoff calculator**: a test that the sequence is 1, 2, 4, 8, 30, 30, ... and resets on successful handshake. RED: no calculator. GREEN: a pure function (easy to test in isolation).
4. **Crash-loop detection**: a test that 5 consecutive failed boots → `CrashLooping` → calls the prompt. RED: no counter. GREEN: the counter + prompt call.
5. **Connection task**: a test that it reads a frame → upstream, downstream → writes a frame. Uses a duplex (no real socket needed for the I/O-shape test). RED: no task. GREEN: the ~15-line task.
6. **Supervisor integration** (mock host, `go-silent-after-handshake` mode): spawn mock → `Booting` → `Handshake` → `Ready` → heartbeat loop → mock goes silent → `Hung` (3 missed Pongs) → `Reconnecting` → backoff → respawn → `Ready`. RED: no supervisor. GREEN: the supervisor.
7. **`reload()`**: Core is `Ready` → call `reload()` → `Shutdown{drain:true}` → mock sends `ShutdownAck` + exits → `Stopped` → respawn → `Ready`. RED: no `reload()`. GREEN: the method.
8. **Crash-loop + prompt**: mock `exit-immediately` mode → 5 boots fail → `CrashLooping` → prompt fires (stdin stubbed in the test). RED: no prompt integration. GREEN: the prompt path.
9. **Mock-host binary**: build it, verify each mode behaves (handshake/pong for normal, immediate exit for exit-immediately, silence after handshake for go-silent). RED: no binary. GREEN: `tests/mock_host.rs`.
10. **Real host binary** (`host/main.ts`): a Deno test that it connects, handshakes, pongs, echoes, acks Shutdown. RED: no binary. GREEN: `host/main.ts` + `deno compile` smoke (full Deno lane is step 6).
11. **Exit gate 2 chaos test**: spawn `mock-host --mode normal`, `kill -9` it, assert Core detects `Hung` and respawns to `Ready`. The gate proof.

## Oracle citations

The host lifecycle is pi-rs-native. pi is single-process; there is no pi equivalent of host supervision, heartbeat, or restart to cite (ADR 0023, ADR 0009, ADR 0017). No Oracle permalink applies. This absence is stated explicitly per workflow step 9.

## Out of scope

- Wiring the codec into the real host binary's message handling beyond echo (the codec is chosen per step 4; the host uses it for framing, but real extension messages are Phase 3).
- The TUI prompt (Phase 3 replaces the stdin stub per ADR 0023 Q3).
- Hooks/tool-call interception (Phase 3; ADR 0009's fail-closed semantics apply to tools, which don't exist yet).
- The CI Deno lane running the real host binary test (step 6).
- `deno compile` in CI (step 6; the chaos test uses the mock-host binary, not the real one).
