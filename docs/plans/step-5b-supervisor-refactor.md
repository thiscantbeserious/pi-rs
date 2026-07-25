# Step 5b plan: supervisor refactor — extract policy from I/O glue

The supervisor's `run()` function fuses policy decisions (which state to transition to, how to route messages, when to declare Hung) with I/O orchestration (spawn process, accept connection, channel read/write, sleep). This causes two CI failures: 0/122 coverage (policy only tested via integration tests that `--lib` skips) and cognitive complexity 19 (max 15). Both are symptoms of the same structural flaw.

## Problems

1. **Coverage**: `host_supervisor.rs` has 0/122 lines covered because tarpaulin runs `--lib` (unit tests only). The supervisor's tests are integration tests in `tests/supervisor_integration.rs`. The `--lib` flag was inherited from the step-1 migration and never reconsidered when integration tests landed.
2. **Cognitive complexity**: `run()` is 19 (max 15), `mock_host::main()` is 22 (max 15). Both do too much in one match/loop.
3. **No unit tests for policy**: the supervisor's policy logic (transition decisions, message routing, boot validation) is only exercised through the full integration test. A policy bug can't be caught in isolation.

## Fix

### 1. Tarpaulin: run all tests, not just lib

Change `cargo tarpaulin --workspace --lib` to `cargo tarpaulin --workspace` (no `--lib`). The `exclude-files` in `tarpaulin.toml` already excludes test files from coverage measurement. Integration tests that spawn processes are slower but the supervisor tests are ~3s total. This is a config fix, not a workaround: the intent was always to measure all test-covered code.

### 2. Extract policy from the supervisor's `run()` loop

Split `run()` into:

- **Policy functions** (pure, unit-testable in the lib): given a state + an event, return the next state. These already partially exist in `host_state.rs` (transition methods on inner types). The missing piece is the *event routing* policy: given a `Message` from upstream, what should the supervisor do? Extract this as a pure function.
- **I/O orchestration** (integration-tested): the `run()` loop spawns, accepts, reads channels, sleeps. It calls the policy functions to decide transitions, then executes them.

Specifically, extract from `run()`:

- `fn handle_boot_result(boot_result, crash_count, config) -> HostState` — decides BackingOff vs CrashLooping vs Ready
- `fn handle_ready_result(ready_result, config) -> HostState` — decides Reconnecting vs Stopped
- `fn route_ready_message(msg, ready) -> ReadyAction` — given an upstream Message in Ready, return the action (send EchoResponse, reset misses, declare Hung, etc.)

Each gets unit tests in the lib. The `run()` loop becomes thin: call I/O, get an event, call the policy function, execute the returned action.

### 3. Split `mock_host::main()` complexity

Extract the message-handling loop into an `async fn handle_message(stream: &mut UnixStream, msg: Message) -> HandleResult` helper, where `HandleResult` is an enum (`Continue`, `Exit(ExitCode)`). `main()` becomes: parse args, connect, handshake, loop calling `handle_message`. The pre/post-handshake mode dispatch is further split into `handle_pre_handshake_mode` and `handle_post_handshake_mode` to keep `main()` under the cognitive complexity limit.

### 4. Reduce `run()` cognitive complexity

Extract each match arm into a named async method:

- `fn transition_from_stopped(&self) -> HostState`
- `fn transition_from_booting(&self, listener, booting) -> HostState`
- `fn transition_from_ready(&self, ready) -> HostState`
- `fn transition_from_backing_off(&self, bo) -> HostState`
- `fn transition_from_reconnecting(&self, re) -> HostState`
- `fn transition_from_crash_looping(&self, cl) -> HostState`

`run()` becomes: bind, loop { state = transition(state); if Stopped break }. Each transition method is under the complexity limit.

## TDD order

1. Extract policy functions + unit tests (RED: no functions. GREEN: functions + tests).
2. Refactor `run()` to call the policy functions (existing integration tests still pass).
3. Extract `mock_host::handle_message` (RED: no function. GREEN: extracted).
4. Split `run()` match arms into transition methods (existing tests still pass).
5. Fix tarpaulin config: `--workspace` without `--lib`.
6. Verify coverage >= 70% and cognitive complexity <= 15.

## Oracle citations

The supervisor is pi-rs-native (pi is single-process). No Oracle permalink. ADR 0023 is the design source.

## Out of scope

- The real host binary (`host/main.ts`) — still step 10, not affected by this refactor.
- The exit-gate-2 chaos test — still step 11, depends on the supervisor working but not on its internal structure.
- `reload()` — still step 8, lands after the refactor.
