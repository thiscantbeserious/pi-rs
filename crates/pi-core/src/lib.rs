//! pi-rs Core: TUI, renderer, agent loop.
//!
//! See CONTEXT.md for the domain language and docs/adr/ for architectural
//! decisions. The agent loop lives here but must not depend on the renderer
//! (ADR 0011, ADR 0018 headless constraint).

pub mod host_connection;
pub mod host_state;
pub mod host_transport;

/// Returns the crate version (workspace package version).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn version_snapshot() {
        insta::assert_snapshot!(version(), @"0.1.0-dev");
    }
}
