//! Session header and version constant (ADR 0008, contract §2-3).
//!
//! Mirrors the Oracle's `SessionHeader` and `CURRENT_SESSION_VERSION`.

use serde::{Deserialize, Serialize};

/// The current session file version. v3 at the pinned Oracle.
/// v1 sessions omit `version` on the header; v2 and v3 set it explicitly.
/// pi-rs always writes v3.
pub const CURRENT_SESSION_VERSION: u32 = 3;

/// The wire tag for a session header line.
pub const HEADER_TYPE: &str = "session";

/// The first line of a session file. `version` is optional because v1
/// sessions predate the field. `parent_session` is present on forked sessions.
///
/// Oracle: `SessionHeader` in
/// `packages/coding-agent/src/core/session-manager.ts` at v0.82.0.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    /// Always the literal `"session"`. The wire tag for a header line.
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
    #[serde(
        rename = "parentSession",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_session: Option<String>,
}

impl SessionHeader {
    /// Construct a v3 header for a new session.
    pub fn new(id: String, timestamp: String, cwd: String) -> Self {
        Self {
            r#type: HEADER_TYPE.into(),
            version: Some(CURRENT_SESSION_VERSION),
            id,
            timestamp,
            cwd,
            parent_session: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_serializes_with_camel_case_parent_session() {
        let header = SessionHeader {
            r#type: HEADER_TYPE.into(),
            version: Some(3),
            id: "0197d001-uuidv7".into(),
            timestamp: "2026-07-25T12:00:00.000Z".into(),
            cwd: "/tmp".into(),
            parent_session: Some("parent-uuid".into()),
        };
        let value: serde_json::Value = serde_json::to_value(&header).unwrap();
        assert_eq!(value["type"], "session");
        assert_eq!(value["version"], 3);
        assert_eq!(value["parentSession"], "parent-uuid");
        // camelCase, not snake_case
        assert!(value.get("parent_session").is_none());
    }

    #[test]
    fn header_omits_parent_session_when_none() {
        let header = SessionHeader::new("id".into(), "ts".into(), "/cwd".into());
        let value: serde_json::Value = serde_json::to_value(&header).unwrap();
        assert!(value.get("parentSession").is_none());
        assert_eq!(value["version"], CURRENT_SESSION_VERSION);
    }

    #[test]
    fn v1_header_has_no_version_field() {
        // v1 sessions omit version entirely. Parse must accept it.
        let json = r#"{"type":"session","id":"x","timestamp":"t","cwd":"/c"}"#;
        let header: SessionHeader = serde_json::from_str(json).unwrap();
        assert_eq!(header.version, None);
        assert_eq!(header.cwd, "/c");
    }
}
