# Phase 1 host lifecycle: state machine, supervision, and restart policy

ADR 0009 (heartbeat liveness, fail-closed), ADR 0017 (/reload = host restart), and ADR 0022 (handshake, heartbeat timing, shutdown drain) settle the *semantics* of the host lifecycle but not the *mechanics*: how the Core spawns the host, how it supervises it, what happens on a crash loop, and how the lifecycle is structured as concurrent tasks. This ADR records those mechanics, fixed before implementation (Phase 1 step 5 grill). Every decision below had genuine alternatives and was picked for specific reasons.

## Q1: Host spawn — `deno compile` to a binary

The Core spawns a `deno compile`-produced binary, not `deno run`. The compile step runs at build time (verified for the Phase 1 host: codec + framing + protocol types, no full pi runtime yet) and produces a standalone binary. The Core spawns it with `PI_RS_HOST_SOCKET=<path>` in env.

Chosen over `deno run host/main.ts` because a runtime `deno` on `$PATH` "can be forgotten" at deploy: a deployment that omits Deno breaks silently at runtime, not at build. The compiled binary makes the dependency build-time, which fails loudly. Chosen over Node (ADR 0002 locked in Deno after Phase 0; Node fallback stood down).

Known future cost: when Phase 3 imports the full pi runtime (AWS SDK, Google genai, OpenAI, Anthropic, MCP SDK), the compile step and binary size grow dramatically. Re-evaluate at Phase 3 against ADR 0014/0020's distribution story. For Phase 1 the compile is cheap and the binary is small.

## Q2: Restart backoff — exponential, 5 fails to prompt

On a boot crash, respawn immediately the first time (transient crashes happen). On repeated crashes, back off exponentially: 1s, 2s, 4s, 8s, capped at 30s. After 5 consecutive failed boots (crash before successful handshake), stop auto-respawning and surface the native restart prompt (ADR 0009's human escape hatch). The crash count resets to 0 on a successful handshake: a crash after that is a new incident, not a continuation of a boot loop. "5 consecutive failed boots" means 5 crashes before any successful handshake, which is the actual signal of a broken host.

Chosen over always-auto-respawn-no-backoff (tight crash loop burns CPU, fills logs, user has no signal until terminal is unresponsive, the exact failure ADR 0009's prompt exists to prevent). Chosen over always-prompt (a single transient crash forces a human decision, too noisy for a daily-driver, and contradicts having backoff at all). Chosen over fixed-delay-no-escalation (a genuinely broken host loops forever, just slower, no human escalation path).

## Q3: Native restart prompt (Phase 1 stub) — log + stdin

When the host is hung or crash-looping, the Core logs the reason to stderr and reads a single line from stdin: `r` = restart (resets backoff and crash count), `b` = bypass once, `a` = abort turn. The decision is returned as a `FailureDecision` enum (`Restart`, `BypassOnce`, `AbortTurn`).

Chosen over log + auto-restart (contradicts ADR 0009: "a human choice, never a silent bypass, is the only escape hatch"; even with no hooks in Phase 1, the prompt path must exist so Phase 3 wires hooks into it). Chosen over a trait abstraction with no impl (YAGNI; the log+stdin stub is concrete, testable, replaceable; a trait without an impl is dead code). Chosen over doing nothing (the exit gate text says "prompts, respawns"; skipping the prompt makes the gate text a lie, the same honesty problem Phase 0's "stubbed" checkbox had).

Phase 3 swaps the stdin read for the TUI prompt without changing the `FailureDecision` type. The stub produces it from stdin; Phase 3 produces it from a key event.

## Q4: State machine — 8 states

`Stopped`, `Booting`, `Ready`, `Hung`, `Draining`, `BackingOff`, `Reconnecting`, `CrashLooping`.

- `Stopped` — no host process, no pending action. Initial and terminal-after-graceful-shutdown.
- `Booting` — Core spawned the host, waiting for `Handshake`. A crash here is a "failed boot" (Q2's crash count increments).
- `Ready` — handshake accepted, `HandshakeAck` sent. Host is live and exchanging messages.
- `Hung` — 3 consecutive missed `Pong`s (ADR 0009, ADR 0022 Q7). Transient: the Core closes the socket and moves to `Reconnecting`. Kept as a named state for log clarity ("declared hung at T, closing socket") even though it immediately transitions.
- `Draining` — Core sent `Shutdown{drain:true}` (for `/reload`), waiting for `ShutdownAck` or host exit.
- `BackingOff` — a boot crash occurred (crash count < 5); wait the backoff timer, then respawn. Distinct from `Stopped` so the pending timer is explicit state, not hidden.
- `Reconnecting` — the host died at runtime (via `Hung`); wait the backoff timer, then respawn. Distinct from `BackingOff` because the *reason* differs (runtime death vs boot failure), which matters for logging, metrics, and debugging. The timer logic is shared; the semantic is not.
- `CrashLooping` — 5 consecutive failed boots; supervisor gave up auto-respawn, surfacing the native prompt.

Transitions:

- `Stopped → Booting`: spawn the host binary.
- `Booting → Ready`: valid `Handshake` received (version matches), `HandshakeAck` sent. Crash count resets.
- `Booting → BackingOff`: host exited before handshake, crash count < 5.
- `Booting → CrashLooping`: 5 consecutive failed boots. Surface prompt.
- `Ready → Hung`: 3 missed `Pong`s.
- `Ready → Draining`: Core sends `Shutdown{drain:true}`.
- `Hung → Reconnecting`: Core closes socket (no graceful drain for a hung host, it will not process `Shutdown`).
- `Draining → Stopped`: `ShutdownAck` received or host exits.
- `BackingOff → Booting`: backoff timer elapsed, respawn.
- `Reconnecting → Booting`: backoff timer elapsed, respawn.
- `CrashLooping → Booting`: human picks `Restart` (resets backoff and crash count).
- `CrashLooping → Stopped`: human picks `BypassOnce` or `AbortTurn`.

Chosen over folding `BackingOff` into `Stopped` (hidden state: `Stopped` would sometimes have a pending timer and sometimes not, the kind of conflation the parse-don't-validate rule warns against). Chosen over a separate `Reconnecting` only when it adds speculative precision: it does, because the post-`Hung` and post-boot-crash paths tell different stories even with shared timer logic.

## Q5: Concurrency — supervisor task owns state, connection tasks do I/O

A supervisor task owns the state machine (state, crash count, backoff timer) and spawns connection tasks. A connection task does the socket I/O and heartbeat writes via `tokio::select!` over the heartbeat interval and socket reads, and reports events upstream via a channel: `HandshakeReceived`, `PongReceived`, `PongTimedOut`, `ConnectionClosed`. The supervisor is the single writer of lifecycle state; connection tasks are ephemeral (killed on `Hung`/respawn).

The connection task sends heartbeats (it owns the socket) but the supervisor owns the miss count and the `Hung` decision (it owns the state). The connection task reports `PongReceived`/`PongTimedOut`; the supervisor increments the miss count and decides `Hung`. Clean separation: connection does I/O, supervisor does policy.

Chosen over a single task with `select!` (the state machine outlives any single connection; a connection task dying on `Hung` would lose the supervisor state, so the supervisor must be separate, which collapses to this option). Chosen over thread-per-connection with `std` sync (ADR 0013 mandates tokio for everything async; `std` threads + locks on the state machine is the failure mode GOALS.md goal 2 warns against).

## Q6: Respawn policy — auto-respawn first death, prompt on crash-loop

A single `kill -9` of a healthy host: Core detects `Hung` (3 missed `Pong`s, ~15s at the 5s interval) → closes socket → `Reconnecting` → backoff → respawns automatically. The native prompt only appears if respawn also crashes (crash-loop, 5 consecutive failed boots per Q2).

Chosen over always-prompt-on-any-death (every transient death becomes a human interruption, too noisy for a daily-driver, and contradicts Q2's auto-backoff design, which has no purpose if every death needs a human). Chosen over never-prompt (contradicts ADR 0009 and Q2's crash-loop → prompt path).

Exit gate 2 ("Core survives, prompts, respawns") is satisfied as: the chaos test asserts survival + auto-respawn on `kill -9`; a separate test exercises the prompt on crash-loop. The "prompts" in the gate text is the *capability*, exercised by the crash-loop test, not the kill-9 test.

## Consequences

- The supervisor is the single writer of lifecycle state. No locks on the state machine; events flow over channels. This matches ADR 0013's discipline (no locks on hot paths) and GOALS.md goal 2 (correctness at the seams).
- The `FailureDecision` type is the Phase 3 handoff: the TUI prompt produces the same enum from a key event that the Phase 1 stub produces from stdin.
- The `deno compile` build step is a new CI concern. Phase 1's compile is cheap; Phase 3's will not be. The compile output path and the spawn binary path are read by the supervisor at startup.
- The 8-state machine is the contract the chaos test and the crash-loop test exercise. Adding a state is a new variant plus new transitions; the typestate-style API makes illegal transitions not compile (PHILOSOPHY.md §4).
- Re-benchmark or re-evaluate the `deno compile` path at Phase 3 when the import graph grows (ADR 0014/0020).
