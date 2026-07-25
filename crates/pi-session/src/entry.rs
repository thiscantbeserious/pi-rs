//! Session entry taxonomy (ADR 0008, contract §4-5). Nine entry types over a
//! shared base, plus `FileEntry` (header or entry) for line-by-line parsing.
//!
//! Mirrors the Oracle's `SessionEntry` union and the nine entry interfaces.
//! `custom` does NOT participate in LLM context; `custom_message` DOES. The
//! distinction is parity-critical (contract §4).
//!
//! The `type` tag lives on the enum (`#[serde(tag = "type")]`); `EntryBase`
//! carries only the shared `id`/`parentId`/`timestamp`. serde routes the tag
//! to the variant and flattens the base alongside the variant's own fields.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Base fields every non-header entry carries. `id`/`parentId` form the tree.
/// The `type` tag is owned by the `SessionEntry` enum, not here.
///
/// `id` defaults to an empty string so v1 entries (which predate `id`/`parentId`)
/// parse; migration assigns real ids. A v3 entry always has a non-empty `id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EntryBase {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "parentId", default)]
    pub parent_id: Option<String>,
    pub timestamp: String,
}

/// A session entry. The `type` field selects the variant. Unknown types
/// (extension-defined future entries) round-trip as `Unknown` so re-save is
/// byte-identical even for entries pi-rs does not understand.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum SessionEntry {
    /// `message`: an `AgentMessage` (user/assistant/toolResult). The message
    /// body is preserved as opaque JSON until `pi-messages` types land
    /// (contract §10 open question).
    #[serde(rename = "message")]
    Message {
        #[serde(flatten)]
        base: EntryBase,
        message: Value,
    },
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(rename = "thinkingLevel")]
        thinking_level: String,
    },
    #[serde(rename = "model_change")]
    ModelChange {
        #[serde(flatten)]
        base: EntryBase,
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
    /// Compaction boundary. Context reconstruction replaces the prefix before
    /// this entry with `[compaction, firstKeptEntryId..]` (contract §6.2).
    ///
    /// `first_kept_entry_id` is Option because v1 sessions carry
    /// `firstKeptEntryIndex` (a positional number) instead; migration resolves
    /// the index to an id. A v3 compaction always has `first_kept_entry_id =
    /// Some(..)` and `first_kept_entry_index = None`.
    #[serde(rename = "compaction")]
    Compaction {
        #[serde(flatten)]
        base: EntryBase,
        summary: String,
        #[serde(
            rename = "firstKeptEntryId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        first_kept_entry_id: Option<String>,
        /// v1 legacy positional index. Captured at parse time so migration can
        /// resolve it to `first_kept_entry_id`. Never written by pi-rs (v3).
        #[serde(
            rename = "firstKeptEntryIndex",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        first_kept_entry_index: Option<u64>,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<Value>,
        #[serde(rename = "fromHook", default, skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    #[serde(rename = "branch_summary")]
    BranchSummary {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(rename = "fromId")]
        from_id: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<Value>,
        #[serde(rename = "fromHook", default, skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    /// Extension state. Does NOT participate in LLM context.
    #[serde(rename = "custom")]
    Custom {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(rename = "customType")]
        custom_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
    /// Extension message injected into LLM context as a user message.
    #[serde(rename = "custom_message")]
    CustomMessage {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(rename = "customType")]
        custom_type: String,
        content: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        display: bool,
    },
    /// User bookmark on an entry. `label = None` clears.
    #[serde(rename = "label")]
    Label {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(rename = "targetId")]
        target_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Session display name. Latest wins, including explicit clears.
    #[serde(rename = "session_info")]
    SessionInfo {
        #[serde(flatten)]
        base: EntryBase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

impl SessionEntry {
    /// The entry's `id` (tree node identity).
    pub fn id(&self) -> &str {
        &self.base_ref().id
    }

    /// The entry's `parentId` (tree parent, None for root).
    pub fn parent_id(&self) -> Option<&str> {
        self.base_ref().parent_id.as_deref()
    }

    fn base_ref(&self) -> &EntryBase {
        match self {
            SessionEntry::Message { base, .. }
            | SessionEntry::ThinkingLevelChange { base, .. }
            | SessionEntry::ModelChange { base, .. }
            | SessionEntry::Compaction { base, .. }
            | SessionEntry::BranchSummary { base, .. }
            | SessionEntry::Custom { base, .. }
            | SessionEntry::CustomMessage { base, .. }
            | SessionEntry::Label { base, .. }
            | SessionEntry::SessionInfo { base, .. } => base,
        }
    }
}

/// A file entry: the header (first line) or any subsequent entry line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEntry {
    Header(crate::header::SessionHeader),
    Entry(SessionEntry),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(id: &str, parent: Option<&str>) -> EntryBase {
        EntryBase {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            timestamp: "t".into(),
        }
    }

    #[test]
    fn custom_entry_round_trips_with_opaque_data() {
        // Extension data must survive byte-identical (contract §10).
        let json = r#"{"type":"custom","id":"c1","parentId":null,"timestamp":"t","customType":"myext","data":{"any":[1,"x",true]}}"#;
        let entry: SessionEntry = serde_json::from_str(json).unwrap();
        match &entry {
            SessionEntry::Custom {
                custom_type, data, ..
            } => {
                assert_eq!(custom_type, "myext");
                assert_eq!(*data, Some(serde_json::json!({"any":[1,"x",true]})));
            }
            _ => panic!("wrong variant"),
        }
        let re = serde_json::to_string(&entry).unwrap();
        assert_eq!(re, json);
    }

    #[test]
    fn custom_vs_custom_message_distinct() {
        let custom = serde_json::from_str::<SessionEntry>(
            r#"{"type":"custom","id":"c","parentId":null,"timestamp":"t","customType":"x"}"#,
        )
        .unwrap();
        let custom_msg = serde_json::from_str::<SessionEntry>(
            r#"{"type":"custom_message","id":"c","parentId":null,"timestamp":"t","customType":"x","content":"hi","display":true}"#,
        )
        .unwrap();
        assert!(matches!(custom, SessionEntry::Custom { .. }));
        assert!(matches!(custom_msg, SessionEntry::CustomMessage { .. }));
    }

    #[test]
    fn label_with_none_clears() {
        let json = r#"{"type":"label","id":"l","parentId":null,"timestamp":"t","targetId":"e1"}"#;
        let entry: SessionEntry = serde_json::from_str(json).unwrap();
        match entry {
            SessionEntry::Label { label, .. } => assert_eq!(label, None),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn compaction_carries_first_kept_entry_id() {
        let json = r#"{"type":"compaction","id":"k","parentId":null,"timestamp":"t","summary":"s","firstKeptEntryId":"kept","tokensBefore":1000}"#;
        let entry: SessionEntry = serde_json::from_str(json).unwrap();
        match entry {
            SessionEntry::Compaction {
                first_kept_entry_id,
                tokens_before,
                ..
            } => {
                assert_eq!(first_kept_entry_id.as_deref(), Some("kept"));
                assert_eq!(tokens_before, 1000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn entry_id_and_parent_id_accessors() {
        let entry = SessionEntry::ThinkingLevelChange {
            base: base("e1", Some("p1")),
            thinking_level: "high".into(),
        };
        assert_eq!(entry.id(), "e1");
        assert_eq!(entry.parent_id(), Some("p1"));
    }

    #[test]
    fn base_helper_compiles() {
        let _ = base("custom", None);
    }
}
