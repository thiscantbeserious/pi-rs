//! Host Protocol message types (ADR 0022).
//!
//! The nine-message minimal set for Phase 1. The full extension-facing
//! surface (docs/extension-api-surface.md) lands in Phase 3.

use serde::{Deserialize, Serialize};

/// Transport-level error codes. ADR 0022 Q9: transport-only. ADR 0009
/// in-flight-tool errors are Phase 3 ToolResult variants, never this.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub enum ProtocolErrorCode {
    /// Received a message type the receiver does not know.
    UnknownMessageType,
    /// A frame could not be decoded as MessagePack or as a valid Message.
    MalformedFrame,
    /// A message arrived in a state that does not expect it (e.g. Pong before Handshake).
    UnexpectedMessage,
    /// The handshake was rejected (version mismatch, malformed fields).
    HandshakeRejected,
}

/// The protocol revision. ADR 0022 Q3: exact-match-or-refuse, single u32
/// monotonic revision (not semver). Bump on every protocol change.
pub const PROTOCOL_VERSION: u32 = 1;

/// The host's first frame. Host -> Core. ADR 0022 Q3.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct Handshake {
    pub protocol_version: u32,
    pub host_pid: u32,
}

/// Core's acceptance of the host's Handshake. Core -> Host. ADR 0022 Q4b.
/// Unit variant in the Message enum (no fields).
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct HandshakeAck;

/// Core probes liveness. Core -> Host. ADR 0022 Q7.
/// Unit variant in the Message enum (no fields).
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct Heartbeat;

/// Host replies to a Heartbeat. Host -> Core. ADR 0022 Q7.
/// Unit variant in the Message enum (no fields).
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct Pong;

/// Core tells the host to shut down. Core -> Host. ADR 0022 Q8.
/// drain=true: finish in-flight protocol-message handling, send ShutdownAck, exit.
/// drain=false: exit immediately, no ack.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct Shutdown {
    pub drain: bool,
}

/// Host acknowledges a graceful Shutdown (drain=true only). Host -> Core. ADR 0022 Q8.
/// Unit variant in the Message enum (no fields).
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct ShutdownAck;

/// A request to echo a payload back. Establishes request_id correlation
/// (ADR 0022 Q6). Bidirectional.
///
/// `request_id` is `u64` on the wire but maps to TypeScript `number` (53-bit
/// safe) via `TS_RS_LARGE_INT=number` in `.cargo/config.toml`. Senders must
/// keep request_id below 2^53; these are monotonic counters, never large.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct EchoRequest {
    pub request_id: u64,
    pub payload: Vec<u8>,
}

/// The reply to an EchoRequest, carrying the same request_id. Bidirectional.
/// See `EchoRequest` for the 53-bit constraint on request_id.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct EchoResponse {
    pub request_id: u64,
    pub payload: Vec<u8>,
}

/// A transport-level error. Bidirectional. ADR 0022 Q9.
#[derive(Serialize, Deserialize, ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
}

/// Every Host Protocol message. ADR 0022 Q5: the wire envelope is a map
/// with a `"type"` field set to the verbatim PascalCase variant name, and the
/// variant's fields flattened alongside it (e.g. {"type":"Handshake",
/// "protocol_version":1, "host_pid":123}, {"type":"Heartbeat"}).
///
/// Implemented manually rather than via `#[serde(tag = "type")]` because
/// rmp-serde does not honor serde's internally-tagged enum representation
/// (3Hren/msgpack-rust issues #130, #153, #250, #327, #225). The inner structs
/// keep their derives for ts-rs codegen; only the enum's serde impl is manual.
/// `#[ts(tag = "type")]` is the ts-rs-native hint (independent of serde) that
/// emits the discriminated-union TS shape matching the wire.
#[derive(ts_rs::TS, Debug, Clone, PartialEq, Eq)]
#[ts(export, tag = "type")]
pub enum Message {
    Handshake {
        #[ts(flatten)]
        inner: Handshake,
    },
    HandshakeAck,
    Heartbeat,
    Pong,
    Shutdown {
        #[ts(flatten)]
        inner: Shutdown,
    },
    ShutdownAck,
    EchoRequest {
        #[ts(flatten)]
        inner: EchoRequest,
    },
    EchoResponse {
        #[ts(flatten)]
        inner: EchoResponse,
    },
    ProtocolError {
        #[ts(flatten)]
        inner: ProtocolError,
    },
}

impl Message {
    /// The wire tag for this variant (ADR 0022 Q5: verbatim PascalCase).
    pub fn type_tag(&self) -> &'static str {
        match self {
            Message::Handshake { .. } => "Handshake",
            Message::HandshakeAck => "HandshakeAck",
            Message::Heartbeat => "Heartbeat",
            Message::Pong => "Pong",
            Message::Shutdown { .. } => "Shutdown",
            Message::ShutdownAck => "ShutdownAck",
            Message::EchoRequest { .. } => "EchoRequest",
            Message::EchoResponse { .. } => "EchoResponse",
            Message::ProtocolError { .. } => "ProtocolError",
        }
    }
}

mod wire {
    //! Manual serde impl for `Message` producing the ADR 0022 Q5 envelope:
    //! a msgpack map with "type" = variant name, then the variant's fields.
    //! rmp-serde ignores `#[serde(tag)]`, so this is hand-rolled.
    use super::Message;
    use serde::{ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for Message {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let tag = self.type_tag();
            match self {
                Message::Handshake { inner: m } => serialize_tagged_struct(s, tag, m),
                Message::Shutdown { inner: m } => serialize_tagged_struct(s, tag, m),
                Message::EchoRequest { inner: m } => serialize_tagged_struct(s, tag, m),
                Message::EchoResponse { inner: m } => serialize_tagged_struct(s, tag, m),
                Message::ProtocolError { inner: m } => serialize_tagged_struct(s, tag, m),
                Message::HandshakeAck
                | Message::Heartbeat
                | Message::Pong
                | Message::ShutdownAck => {
                    let mut map = s.serialize_map(Some(1))?;
                    map.serialize_entry("type", tag)?;
                    map.end()
                }
            }
        }
    }

    fn serialize_tagged_struct<S: Serializer, T: Serialize>(
        s: S,
        tag: &'static str,
        payload: &T,
    ) -> Result<S::Ok, S::Error> {
        // Serialize the payload to a serde_json::Value to count its fields,
        // then emit a map with "type" + the payload's fields flattened.
        let payload = serde_json::to_value(payload).map_err(serde::ser::Error::custom)?;
        let obj = payload
            .as_object()
            .ok_or_else(|| serde::ser::Error::custom("expected a struct payload, got non-map"))?;
        let mut map = s.serialize_map(Some(obj.len() + 1))?;
        map.serialize_entry("type", tag)?;
        for (k, v) in obj {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }

    impl<'de> Deserialize<'de> for Message {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let value = serde_json::Value::deserialize(d)?;
            let obj = value
                .as_object()
                .ok_or_else(|| serde::de::Error::custom("expected a map with a \"type\" field"))?;
            let tag = obj
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| serde::de::Error::custom("missing \"type\" field"))?;
            // Re-serialize the map minus "type" to deserialize the payload struct.
            let mut payload = obj.clone();
            payload.remove("type");
            let payload = serde_json::Value::Object(payload);
            match tag {
                "Handshake" => Ok(Message::Handshake {
                    inner: serde_json::from_value(payload).map_err(serde::de::Error::custom)?,
                }),
                "HandshakeAck" => Ok(Message::HandshakeAck),
                "Heartbeat" => Ok(Message::Heartbeat),
                "Pong" => Ok(Message::Pong),
                "Shutdown" => Ok(Message::Shutdown {
                    inner: serde_json::from_value(payload).map_err(serde::de::Error::custom)?,
                }),
                "ShutdownAck" => Ok(Message::ShutdownAck),
                "EchoRequest" => Ok(Message::EchoRequest {
                    inner: serde_json::from_value(payload).map_err(serde::de::Error::custom)?,
                }),
                "EchoResponse" => Ok(Message::EchoResponse {
                    inner: serde_json::from_value(payload).map_err(serde::de::Error::custom)?,
                }),
                "ProtocolError" => Ok(Message::ProtocolError {
                    inner: serde_json::from_value(payload).map_err(serde::de::Error::custom)?,
                }),
                other => Err(serde::de::Error::custom(format!(
                    "unknown message type: {other}"
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(msg: &Message) -> Message {
        let bytes = rmp_serde::to_vec(msg).unwrap();
        rmp_serde::from_slice(&bytes).unwrap()
    }

    #[test]
    fn all_nine_messages_round_trip() {
        let cases = vec![
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
            Message::ShutdownAck,
            Message::EchoRequest {
                inner: EchoRequest {
                    request_id: 42,
                    payload: b"hi".to_vec(),
                },
            },
            Message::EchoResponse {
                inner: EchoResponse {
                    request_id: 42,
                    payload: b"hi".to_vec(),
                },
            },
            Message::ProtocolError {
                inner: ProtocolError {
                    code: ProtocolErrorCode::MalformedFrame,
                    message: "bad frame".into(),
                },
            },
        ];
        for msg in &cases {
            assert_eq!(round_trip(msg), *msg, "round-trip failed for {:?}", msg);
        }
    }

    #[test]
    fn unit_variant_serializes_as_type_tagged_object() {
        // ADR 0022 Q5: unit variants render as {"type":"Heartbeat"} on the wire.
        let bytes = rmp_serde::to_vec(&Message::Heartbeat).unwrap();
        let value: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], serde_json::json!("Heartbeat"));
        assert!(
            value.as_object().map(|o| o.len()) == Some(1),
            "unit variant has only the type tag, got: {value}"
        );
    }

    #[test]
    fn struct_variant_serializes_as_type_tagged_with_fields() {
        // ADR 0022 Q5: struct variants render as {"type":"Handshake", "protocol_version":.., "host_pid":..}.
        let msg = Message::Handshake {
            inner: Handshake {
                protocol_version: 1,
                host_pid: 99,
            },
        };
        let bytes = rmp_serde::to_vec(&msg).unwrap();
        let value: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], serde_json::json!("Handshake"));
        assert_eq!(value["protocol_version"], serde_json::json!(1));
        assert_eq!(value["host_pid"], serde_json::json!(99));
    }

    #[test]
    fn protocol_error_code_round_trips_as_variant_name() {
        // ADR 0022 Q9: code serializes as the variant-name string.
        let msg = Message::ProtocolError {
            inner: ProtocolError {
                code: ProtocolErrorCode::UnknownMessageType,
                message: "nope".into(),
            },
        };
        let bytes = rmp_serde::to_vec(&msg).unwrap();
        let value: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(value["code"], serde_json::json!("UnknownMessageType"));
    }
}
