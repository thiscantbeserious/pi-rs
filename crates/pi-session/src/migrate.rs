//! v1 -> v2 and v2 -> v3 migrations (ADR 0008, contract §2).
//!
//! Mirrors the Oracle's `migrateV1ToV2`, `migrateV2ToV3`, and
//! `migrateToCurrentVersion`. Mutates in place.

use crate::entry::FileEntry;
use crate::header::{SessionHeader, CURRENT_SESSION_VERSION};
use serde_json::Value;
use std::collections::HashSet;

/// A migration failure: a mutated JSON value could not be reparsed as a
/// `SessionHeader` or `SessionEntry`. Surfaced rather than silently swallowed
/// so a corrupt migration does not produce a fabricated fallback entry.
#[derive(Debug)]
pub struct MigrationError {
    pub source: serde_json::Error,
    pub context: String,
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "migration error ({}): {}", self.context, self.source)
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Run all necessary migrations to bring entries to the current version.
/// Mutates in place. Returns `Ok(true)` if any migration was applied, `Ok(false)`
/// if the file was already at the current version.
///
/// Oracle: `migrateToCurrentVersion` in session-manager.ts at v0.82.0.
pub fn migrate_to_current_version(entries: &mut [FileEntry]) -> Result<bool, MigrationError> {
    let version = header_version(entries).unwrap_or(1);
    if version >= CURRENT_SESSION_VERSION {
        return Ok(false);
    }
    if version < 2 {
        migrate_v1_to_v2(entries)?;
    }
    if version < 3 {
        migrate_v2_to_v3(entries)?;
    }
    Ok(true)
}

fn header_version(entries: &[FileEntry]) -> Option<u32> {
    entries.iter().find_map(|e| match e {
        FileEntry::Header(h) => h.version,
        _ => None,
    })
}

/// v1 -> v2: set header version to 2; assign id/parentId to every non-header
/// entry (8-hex, collision-checked); convert compaction.firstKeptEntryIndex
/// to firstKeptEntryId.
///
/// Two passes: (1) assign every entry its final id and parentId, recording the
/// final id by index; (2) resolve compaction.firstKeptEntryIndex through the
/// completed id map so firstKeptEntryId holds the target's actual (possibly
/// newly-assigned) id. The Oracle assigns all ids before resolving the index.
fn migrate_v1_to_v2(entries: &mut [FileEntry]) -> Result<(), MigrationError> {
    // Set header version.
    for entry in entries.iter_mut() {
        if let FileEntry::Header(h) = entry {
            h.version = Some(2);
        }
    }
    // Pass 1: assign id/parentId, recording the final id for each index.
    let mut final_ids: Vec<Option<String>> = Vec::with_capacity(entries.len());
    let mut used: HashSet<String> = HashSet::new();
    let mut prev_id: Option<String> = None;
    for entry in entries.iter_mut() {
        if let FileEntry::Header(_) = entry {
            final_ids.push(None);
            continue;
        }
        let mut obj = entry_to_json(entry)
            .as_object()
            .cloned()
            .unwrap_or_default();
        let id = match obj.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                let new = generate_short_id(&used);
                used.insert(new.clone());
                new
            }
        };
        obj.insert("id".into(), Value::String(id.clone()));
        obj.insert(
            "parentId".into(),
            prev_id.take().map(Value::String).unwrap_or(Value::Null),
        );
        prev_id = Some(id.clone());
        final_ids.push(Some(id));
        *entry = json_to_entry(&Value::Object(obj))?;
    }
    // Pass 2: resolve compaction.firstKeptEntryIndex -> firstKeptEntryId using
    // the completed id map, so a target that had no v1 id resolves to its
    // newly-assigned id rather than an empty string.
    for entry in entries.iter_mut() {
        let is_compaction = matches!(
            entry,
            FileEntry::Entry(crate::entry::SessionEntry::Compaction { .. })
        );
        if !is_compaction {
            continue;
        }
        let mut obj = entry_to_json(entry)
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(idx) = obj.get("firstKeptEntryIndex").and_then(|v| v.as_u64()) {
            if let Some(Some(target_id)) = final_ids.get(idx as usize) {
                obj.insert("firstKeptEntryId".into(), Value::String(target_id.clone()));
            }
            obj.remove("firstKeptEntryIndex");
        }
        *entry = json_to_entry(&Value::Object(obj))?;
    }
    Ok(())
}

/// v2 -> v3: set header version to 3; rename message role hookMessage -> custom.
///
/// Oracle: `migrateV2ToV3`.
fn migrate_v2_to_v3(entries: &mut [FileEntry]) -> Result<(), MigrationError> {
    for entry in entries.iter_mut() {
        if let FileEntry::Header(h) = entry {
            h.version = Some(CURRENT_SESSION_VERSION);
            continue;
        }
        // Rename hookMessage -> custom on message entries.
        let value = entry_to_json(entry);
        let Some(obj) = value.as_object() else {
            continue;
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }
        let Some(msg) = obj.get("message").and_then(|v| v.as_object()) else {
            continue;
        };
        if msg.get("role").and_then(|v| v.as_str()) != Some("hookMessage") {
            continue;
        }
        let mut new_obj = obj.clone();
        if let Some(msg) = new_obj.get_mut("message").and_then(|v| v.as_object_mut()) {
            msg.insert("role".into(), Value::String("custom".into()));
        }
        *entry = json_to_entry(&Value::Object(new_obj))?;
    }
    Ok(())
}

fn entry_to_json(entry: &FileEntry) -> Value {
    match entry {
        FileEntry::Header(h) => serde_json::to_value(h).unwrap_or(Value::Null),
        FileEntry::Entry(e) => serde_json::to_value(e).unwrap_or(Value::Null),
    }
}

/// Classify a parsed JSON value as a header or an entry. Returns Err on
/// deserialization failure so callers can surface migration corruption rather
/// than fabricate a fallback entry.
fn json_to_entry(value: &Value) -> Result<FileEntry, MigrationError> {
    if value.get("type").and_then(|v| v.as_str()) == Some("session") {
        return serde_json::from_value::<SessionHeader>(value.clone())
            .map(FileEntry::Header)
            .map_err(|e| MigrationError {
                source: e,
                context: "session header".into(),
            });
    }
    serde_json::from_value::<crate::entry::SessionEntry>(value.clone())
        .map(FileEntry::Entry)
        .map_err(|e| MigrationError {
            source: e,
            context: "session entry".into(),
        })
}

/// Generate an 8-hex-char id, collision-checked against `used`.
/// Oracle: `generateId` uses `randomUUID().slice(0,8)` (random, not
/// time-ordered). pi-rs uses uuid v4 to match the randomness characteristic.
fn generate_short_id(used: &HashSet<String>) -> String {
    for _ in 0..100 {
        let candidate = uuid::Uuid::new_v4().simple().to_string();
        let short = &candidate[..8];
        if !used.contains(short) {
            return short.to_string();
        }
    }
    // Fallback: a fresh full v4 uuid (still random, just longer).
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1_header() -> FileEntry {
        // v1: no version field
        FileEntry::Header(SessionHeader {
            r#type: crate::header::HEADER_TYPE.into(),
            version: None,
            id: "h".into(),
            timestamp: "t".into(),
            cwd: "/c".into(),
            parent_session: None,
        })
    }

    #[test]
    fn no_migration_needed_at_v3() {
        let mut entries = vec![FileEntry::Header(SessionHeader::new(
            "h".into(),
            "t".into(),
            "/c".into(),
        ))];
        assert!(!migrate_to_current_version(&mut entries).unwrap());
    }

    #[test]
    fn v1_headed_to_v3_gets_version_and_ids() {
        let mut entries = vec![
            v1_header(),
            FileEntry::Entry(crate::entry::SessionEntry::Label {
                base: crate::entry::EntryBase {
                    id: String::new(), // v1: no id
                    parent_id: None,
                    timestamp: "t".into(),
                },
                target_id: "x".into(),
                label: Some("mark".into()),
            }),
        ];
        assert!(migrate_to_current_version(&mut entries).unwrap());
        match &entries[0] {
            FileEntry::Header(h) => assert_eq!(h.version, Some(3)),
            _ => panic!("header moved"),
        }
        match &entries[1] {
            FileEntry::Entry(crate::entry::SessionEntry::Label { base, .. }) => {
                assert!(!base.id.is_empty(), "id assigned");
                assert_eq!(base.parent_id, None, "first entry has no parent");
            }
            _ => panic!("entry moved"),
        }
    }

    #[test]
    fn v2_hook_message_role_renamed_to_custom() {
        let mut entries = vec![
            FileEntry::Header(SessionHeader {
                r#type: crate::header::HEADER_TYPE.into(),
                version: Some(2),
                id: "h".into(),
                timestamp: "t".into(),
                cwd: "/c".into(),
                parent_session: None,
            }),
            FileEntry::Entry(crate::entry::SessionEntry::Message {
                base: crate::entry::EntryBase {
                    id: "m1".into(),
                    parent_id: None,
                    timestamp: "t".into(),
                },
                message: serde_json::json!({"role":"hookMessage","content":"hi"}),
            }),
        ];
        assert!(migrate_to_current_version(&mut entries).unwrap());
        match &entries[1] {
            FileEntry::Entry(crate::entry::SessionEntry::Message { message, .. }) => {
                assert_eq!(message["role"], "custom", "hookMessage -> custom");
            }
            _ => panic!("entry moved"),
        }
    }

    /// Helper: parse a v1 session JSONL string into FileEntry, exercising the
    /// real parse->migrate path (not a hand-built typed struct).
    fn parse_v1(contents: &str) -> Vec<FileEntry> {
        let mut entries = crate::parse::parse_session_str(contents);
        migrate_to_current_version(&mut entries).unwrap();
        entries
    }

    #[test]
    fn v1_compaction_index_resolves_to_existing_target_id() {
        // Target entry already has an id in v1. firstKeptEntryIndex points at
        // it positionally; migration must resolve to that existing id.
        let contents = "{\"type\":\"session\",\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n\
            {\"type\":\"message\",\"id\":\"m1\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"a\"}}\n\
            {\"type\":\"message\",\"id\":\"m2\",\"parentId\":\"m1\",\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":\"b\"}}\n\
            {\"type\":\"compaction\",\"id\":\"k1\",\"parentId\":\"m2\",\"timestamp\":\"t\",\"summary\":\"s\",\"firstKeptEntryIndex\":1,\"tokensBefore\":10}\n";
        let entries = parse_v1(contents);
        // Find the compaction entry.
        let compaction = entries
            .iter()
            .find(|e| {
                matches!(
                    e,
                    FileEntry::Entry(crate::entry::SessionEntry::Compaction { .. })
                )
            })
            .expect("compaction present after migration");
        match compaction {
            FileEntry::Entry(crate::entry::SessionEntry::Compaction {
                first_kept_entry_id,
                first_kept_entry_index,
                ..
            }) => {
                assert_eq!(
                    first_kept_entry_id.as_deref(),
                    Some("m1"),
                    "index 1 -> m1's id"
                );
                assert_eq!(*first_kept_entry_index, None, "index field cleared");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn v1_compaction_index_resolves_to_newly_generated_target_id() {
        // Target entry has NO id in v1. firstKeptEntryIndex points at it;
        // migration assigns it an id in pass 1, then pass 2 resolves the
        // index to that newly-assigned id (not an empty string).
        let contents = "{\"type\":\"session\",\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n\
            {\"type\":\"message\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"a\"}}\n\
            {\"type\":\"message\",\"timestamp\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":\"b\"}}\n\
            {\"type\":\"compaction\",\"timestamp\":\"t\",\"summary\":\"s\",\"firstKeptEntryIndex\":1,\"tokensBefore\":10}\n";
        let entries = parse_v1(contents);
        // The target is index 1 in the full entries array (the first message;
        // the Oracle indexes the array including the header at 0). It got an
        // id assigned in pass 1.
        let target_id = match &entries[1] {
            FileEntry::Entry(crate::entry::SessionEntry::Message { base, .. }) => base.id.clone(),
            _ => panic!("entry 1 is the target message"),
        };
        assert!(!target_id.is_empty(), "target got an id in pass 1");
        let compaction = match &entries[3] {
            FileEntry::Entry(crate::entry::SessionEntry::Compaction {
                first_kept_entry_id,
                first_kept_entry_index,
                ..
            }) => (first_kept_entry_id.clone(), *first_kept_entry_index),
            _ => panic!("entry 3 is the compaction"),
        };
        assert_eq!(
            compaction.0.as_deref(),
            Some(target_id.as_str()),
            "index 1 -> target's new id"
        );
        assert_eq!(compaction.1, None, "index field cleared");
    }
}
