//! pi-rs Host Protocol: message types, the single source of truth (ADR 0011).
//!
//! Protocol messages are defined once here in Rust and TypeScript definitions
//! are generated from them via ts-rs (ADR 0011), so schema drift between the
//! Core and the Extension Host is a build failure, not a runtime surprise. The
//! wire layer is length-prefixed MessagePack over Unix domain sockets
//! (ADR 0006). The message set, envelope, and lifecycle semantics are fixed by
//! ADR 0022.
//!
//! Phase 1 ships only the minimal set needed to prove the typed round-trip and
//! host lifecycle (ADR 0009, ADR 0017). The full extension-facing surface
//! (docs/extension-api-surface.md) lands in Phase 3.

pub mod fixtures;
pub mod framing;
pub mod messages;

pub use messages::{
    EchoRequest, EchoResponse, Handshake, HandshakeAck, Heartbeat, Message, Pong, ProtocolError,
    ProtocolErrorCode, Shutdown, ShutdownAck, PROTOCOL_VERSION,
};
