//! pi-rs core library.
//!
//! See CONTEXT.md for the domain language (Core, Extension Host, Host Protocol)
//! and docs/adr/ for architectural decisions.

/// Returns the crate version.
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
