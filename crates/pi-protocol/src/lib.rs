//! pi-rs Host Protocol: message types, the single source of truth (ADR 0011).
//!
//! Protocol messages are defined once here in Rust and TypeScript definitions
//! are generated from them via ts-rs (ADR 0011), so schema drift between the
//! Core and the Extension Host is a build failure, not a runtime surprise. The
//! wire layer is length-prefixed MessagePack over Unix domain sockets
//! (ADR 0006).
//!
//! Phase 1 ships only the minimal set needed to prove the typed round-trip and
//! host lifecycle (ADR 0009, ADR 0017). The full extension-facing surface
//! (docs/extension-api-surface.md) lands in Phase 3.

/// Trivial round-trippable type proving the ts-rs codegen pipeline end-to-end.
/// Real protocol messages arrive in step 2 (grill) and step 3.
#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Ping {
    pub id: u64,
}
