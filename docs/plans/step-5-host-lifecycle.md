# Step 5 plan: host lifecycle (handshake, heartbeat, restart, /reload)

Implements ADR 0023 (the grilled mechanics) on top of the step-3 transport and step-4 codec. This is the step that delivers exit gate 2 (`kill -9` the host, Core survives + auto-respawns). No new design decisions; ADR 0023 settled them.

## Scope

1. **The 8-state machine** (`HostState`: Stopped, Booting, Ready, Hung, Draining, BackingOff, Reconnecting, CrashLooping) with typestate-style transitions where illegal transitions do not compile (PHILOSOPHY.md §4).
2. **The supervisor task** (owns state, crash count, backoff timer; spawns connection tasks; receives events via channel; single writer of lifecycle state per ADR 0023 Q5).
3. **The connection task** (socket I/O + heartbeat writes via `tokio::select!`; reports `HandshakeReceived`/`PongReceived`/`PongTimedOut`/`ConnectionClosed` upstream).
4. **The handshake dance** (host sends `Handshake`, Core checks `protocol_version` against `PROTOCOL_VERSION`, exact-match-or-refuse per ADR 0022 Q3, sends `HandshakeAck` on accept).
5. **The heartbeat loop** (Core sends `Heartbeat` at 5s, host replies `Pong`, 3 consecutive missed `Pong`s = Hung per ADR 0022 Q7).
6. **Restart backoff** (exponential 1→2→4→8→30s cap, 5 consecutive failed boots → `CrashLooping` → native prompt, crash count resets on successful handshake per ADR 0023 Q2).
7. **`/reload` path** (graceful `Shutdown{drain:true}` → `ShutdownAck` → `Stopped` → respawn per ADR 0017/0022 Q8).
8. **The native prompt stub** (log to stderr + read `r`/`b`/`a` from stdin, returns `FailureDecision::{Restart,BypassOnce,AbortTurn}` per ADR 0023 Q3).
9. **The host binary** (`host/main.ts`: connects to `PI_RS_HOST_SOCKET`, sends `Handshake`, replies `Pong` to heartbeats, handles `Shutdown{drain:true}` with `ShutdownAck`, echoes `EchoRequest`→`EchoResponse`). Compiled with `deno compile` per ADR 0023 Q1.
10. **Exit gate 2 chaos test** (`kill -9` the host → Core detects `Hung` within ~15s → `Reconnecting` → backoff → respawn → new host handshakes → Core back in `Ready`).

## Research findings (verified against current sources, not training data)

- `deno compile` works for the Phase 1 host (codec + framing + protocol types, no full pi runtime). Verified in the grill: compiles clean, runs standalone, encodes a `Heartbeat` in 16 bytes. Recorded in `docs/research.md`.
- tokio 1.53.1 (already a dep): `tokio::process::Command` for spawning, `tokio::time::{interval, sleep}` for heartbeats/backoff, `tokio::select!` for the connection loop, `mpsc` channel for events upstream. Features needed: `process`, `time`, `macros`, `rt`, `sync` (plus the existing `net`, `io-util`).
- The `FailureDecision` enum is the Phase 3 handoff point (ADR 0023 Q3).

## TDD order (RED before GREEN for each)

1. **State machine**: a test that each legal transition is callable and each illegal transition does not compile (a compile-fail test, or an exhaustive match that the type system enforces). RED: no `HostState` type. GREEN: the type + transitions.
2. **Handshake validation**: a test that a `Handshake` with a matching `protocol_version` yields `HandshakeAck`, and a mismatching one yields `HandshakeRejected` + connection close. RED: no handshake logic. GREEN: the validate function.
3. **Heartbeat miss counting**: a test that 3 consecutive `PongTimedOut` events → `Hung`. RED: no counter. GREEN: the counter (supervisor policy).
4. **Backoff calculator**: a test that the backoff sequence is 1, 2, 4, 8, 30, 30, ... and resets on successful handshake. RED: no calculator. GREEN: the calculator (pure function, easy to test).
5. **Crash-loop detection**: a test that 5 consecutive failed boots → `CrashLooping` → calls the prompt. RED: no crash counter. GREEN: the counter + prompt call.
6. **Connection task**: a test that it sends `Heartbeat`, reads `Pong`, reports `PongReceived`/`PongTimedOut`/`ConnectionClosed` upstream. Uses a duplex or an in-process mock host. RED: no task. GREEN: the task.
7. **Supervisor integration**: a test that the supervisor drives the full lifecycle: spawn (mock host) → `Booting` → `Handshake` → `Ready` → heartbeat loop → `Hung` (mock host stops replying) → `Reconnecting` → respawn → `Ready`. RED: no supervisor. GREEN: the supervisor.
8. **Host binary** (`host/main.ts`): a Deno test that the binary connects, handshakes, pongs, and echoes. RED: no binary. GREEN: `host/main.ts` + `deno compile`.
9. **Exit gate 2 chaos test**: spawn the real host binary via the supervisor, `kill -9` it, assert the Core detects `Hung` and respawns to `Ready`. This is the gate proof.

## Oracle citations

The host lifecycle is pi-rs-native. pi is single-process; there is no pi equivalent of host supervision, heartbeat, or restart to cite (ADR 0023, ADR 0009, ADR 0017). No Oracle permalink applies. This absence is stated explicitly per workflow step 9.

## Out of scope

- Wiring the codec into the host binary's actual message handling beyond echo (the codec is chosen per step 4; the host uses it for framing, but real extension messages are Phase 3).
- The TUI prompt (Phase 3 replaces the stdin stub per ADR 0023 Q3).
- Hooks/tool-call interception (Phase 3; ADR 0009's fail-closed semantics apply to tools, which don't exist yet).
- The CI Deno lane running the host binary test (step 6).
- `deno compile` in CI (step 6; for now the chaos test compiles the host as a test fixture or the binary is pre-built).
