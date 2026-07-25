//! Conformance fixtures generator (ADR 0022 Q10).
//!
//! A test that writes `host/protocol/fixtures.bin` as a sequence of
//! length-prefixed MessagePack frames (the real wire format from ADR 0006 /
//! ADR 0022, not just msgpack decode). The Deno conformance test (step 6)
//! reads it frame-by-frame, decodes each Message, asserts the shape matches
//! the generated TS type, re-encodes, and the Rust side asserts byte-identity.
//!
//! Committed and freshness-checked like the `.ts` files (ADR 0011: drift is a
//! build failure). Running `cargo test` regenerates it; CI fails if the
//! committed copy differs.

use crate::messages::Message;
use crate::{
    EchoRequest, EchoResponse, Handshake, ProtocolError, ProtocolErrorCode, Shutdown,
    PROTOCOL_VERSION,
};

/// The fixture set: one of each message variant, exercising every field.
fn fixture_messages() -> Vec<Message> {
    vec![
        Message::Handshake {
            inner: Handshake {
                protocol_version: PROTOCOL_VERSION,
                host_pid: 1234,
            },
        },
        Message::HandshakeAck,
        Message::Heartbeat,
        Message::Pong,
        Message::Shutdown {
            inner: Shutdown { drain: true },
        },
        Message::Shutdown {
            inner: Shutdown { drain: false },
        },
        Message::ShutdownAck,
        Message::EchoRequest {
            inner: EchoRequest {
                request_id: 42,
                payload: b"hello".to_vec(),
            },
        },
        Message::EchoResponse {
            inner: EchoResponse {
                request_id: 42,
                payload: b"hello".to_vec(),
            },
        },
        // Boundary fixture: 2^53 - 1 is the largest integer JavaScript's
        // number type represents exactly. request_id is u64 on the wire but
        // maps to TS number (TS_RS_LARGE_INT=number). Senders must keep it
        // at or below this value; this fixture proves the boundary round-trips.
        Message::EchoRequest {
            inner: EchoRequest {
                request_id: 9_007_199_254_740_991,
                payload: b"max-safe".to_vec(),
            },
        },
        Message::EchoResponse {
            inner: EchoResponse {
                request_id: 9_007_199_254_740_991,
                payload: b"max-safe".to_vec(),
            },
        },
        Message::ProtocolError {
            inner: ProtocolError {
                code: ProtocolErrorCode::UnknownMessageType,
                message: "unknown".into(),
            },
        },
        Message::ProtocolError {
            inner: ProtocolError {
                code: ProtocolErrorCode::MalformedFrame,
                message: "bad frame".into(),
            },
        },
        Message::ProtocolError {
            inner: ProtocolError {
                code: ProtocolErrorCode::UnexpectedMessage,
                message: "unexpected".into(),
            },
        },
        Message::ProtocolError {
            inner: ProtocolError {
                code: ProtocolErrorCode::HandshakeRejected,
                message: "rejected".into(),
            },
        },
    ]
}

/// Encode the fixture set as a sequence of length-prefixed msgpack frames.
/// Public so the round-trip test (and step 6's Deno reader) can reuse it.
pub fn encode_fixtures() -> Vec<u8> {
    let mut out = Vec::new();
    for msg in fixture_messages() {
        let body = rmp_serde::to_vec(&msg).expect("message serializes");
        let len = u32::try_from(body.len()).expect("frame fits in u32");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::read_frame;
    use std::path::PathBuf;
    use tokio::io::AsyncReadExt;

    /// Where the fixtures file lives. Mirrors TS_RS_EXPORT_DIR (host/protocol/).
    fn fixtures_path() -> PathBuf {
        // Workspace root is two levels up from crates/pi-protocol/.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("host")
            .join("protocol")
            .join("fixtures.bin")
    }

    #[test]
    fn fixtures_file_is_fresh() {
        // RED-GREEN: regenerate fixtures.bin and assert it matches the committed copy.
        let path = fixtures_path();
        let expected = encode_fixtures();
        if path.exists() {
            let committed = std::fs::read(&path).expect("read committed fixtures");
            assert_eq!(
                committed, expected,
                "host/protocol/fixtures.bin is stale; run `cargo test` to regenerate and commit the result"
            );
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).expect("create host/protocol dir");
            std::fs::write(&path, &expected).expect("write fixtures.bin");
            panic!("host/protocol/fixtures.bin did not exist; created it, commit the new file");
        }
    }

    #[tokio::test]
    async fn fixtures_decode_back_to_the_original_messages() {
        // The fixtures must round-trip: each frame decodes to the Message that
        // produced it, and re-encoding the decoded message yields the same
        // bytes (byte-identity, ADR 0022 Q10 dual assertion). This is the
        // assertion the Deno side (step 6) mirrors.
        let bytes = encode_fixtures();
        let mut reader = &bytes[..];
        let original = fixture_messages();
        for expected in &original {
            let frame = read_frame(&mut reader).await.expect("read a frame");
            let got: Message = rmp_serde::from_slice(&frame).expect("decode a message");
            assert_eq!(got, *expected, "fixture did not round-trip");
            // Byte-identity: a serializer change that preserves values but
            // alters the wire representation is caught here.
            let reencoded = rmp_serde::to_vec(&got).expect("re-encode the message");
            assert_eq!(
                reencoded, frame,
                "re-encoding produced different bytes than the fixture"
            );
        }
        // No trailing bytes: every frame was consumed.
        let mut tail = [0u8; 1];
        let n = reader
            .read(&mut tail)
            .await
            .expect("probe for trailing bytes");
        assert_eq!(n, 0, "fixtures.bin has trailing bytes after the last frame");
    }
}
