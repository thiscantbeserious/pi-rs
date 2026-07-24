# Step 3 plan: pi-protocol message types + Core UDS/msgpack transport

Implements ADR 0022 (the grilled design). No new design decisions. This plan exists so the MR review can check the implementation against it.

## Scope

1. The 9 message types in `crates/pi-protocol/src/lib.rs` with `#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]` and `#[ts(export)]`.
2. The `Message` enum, internally tagged via `#[serde(tag = "type")]`, verbatim PascalCase variant names.
3. The `ProtocolErrorCode` enum (transport-only, per ADR 0022 Q9).
4. Length-prefix framing helpers (`read_frame`/`write_frame`) in pi-protocol: 4-byte BE u32 length + msgpack body, length counts body only.
5. Core-side UDS listener (tokio `UnixListener`) + msgpack encode/decode, in pi-core.
6. Conformance fixtures generator: Rust writes `host/protocol/fixtures.bin` as length-prefixed msgpack frames. (The Deno side that reads it lands in step 6.)

## Research findings (verified against current sources, not training data)

- rmp-serde 1.3.1 is current (crates.io). Serde support for MessagePack. Depends on rmp 0.8, serde 1, byteorder 1. This is the crate ADR 0006 names.
- tokio 1.53.1 is current. `UnixListener` is behind the `net` feature.
- ts-rs 12 (already pinned in step 1). `serde-compat` feature is on.

## TDD order (RED before GREEN for each)

1. Framing: test that `write_frame` produces `[BE u32 length][body]` and `read_frame` round-trips it. RED: no functions exist. GREEN: implement. This is the one thing both sides must agree on byte-for-byte.
2. Message encode/decode: test that each of the 9 messages serializes to msgpack and deserializes back equal. RED: no types exist. GREEN: define types + enum.
3. Internally-tagged envelope: test that a serialized `Heartbeat` is `{"type":"Heartbeat"}` and `Handshake` is `{"type":"Handshake","protocol_version":N,"host_pid":N}`. This locks ADR 0022 Q5.
4. Fixtures generator: test that `cargo test` writes `host/protocol/fixtures.bin` as a sequence of length-prefixed frames. RED: no generator. GREEN: a test that writes the file.
5. UDS listener: test that the Core binds a path, accepts a connection, reads a frame, echoes it back. RED: no listener. GREEN: tokio UnixListener + framing.

## Oracle citations

The Host Protocol wire format is pi-rs-native. pi is single-process, so there is no pi equivalent to cite (ADR 0022, ADR 0006). The framing, envelope, and message set are new design recorded in ADR 0022. No Oracle permalink applies. This absence is stated explicitly per workflow step 9.

## Finding during implementation (RED phase)

rmp-serde does not honor `#[serde(tag = "type")]` on enums (3Hren/msgpack-rust issues #130, #153, #250, #327, #225). serde_json produces `{"type":"Heartbeat"}` correctly, rmp-serde produces `["Heartbeat"]`. ADR 0022 Q5 assumed rmp-serde would honor the tag.

Resolution (chosen by the user): hand-roll the `Message` serde impl. The enum keeps `#[ts(tag = "type")]` (ts-rs-native, independent of serde) so the generated `Message.ts` matches the wire shape `{"type":"Handshake", ...fields}`. The inner structs keep their serde derives. No ADR change: the wire shape is exactly as ADR 0022 specified, only the implementation path differs. This finding is recorded inline in the `Message` doc comment with the issue numbers.

## Out of scope

- Host-side codec (step 4).
- Host lifecycle, heartbeat loop, restart (step 5).
- CI Deno lane, Deno conformance test reader (step 6).
- The `--protocol-json` debug flag (ADR 0006 mentions it; lands when a human needs to trace, not now).
