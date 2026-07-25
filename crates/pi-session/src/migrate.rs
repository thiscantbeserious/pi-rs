//! v1 -> v2 and v2 -> v3 migrations (ADR 0008, contract §2).
//!
//! Mirrors the Oracle's `migrateV1ToV2`, `migrateV2ToV3`, and
//! `migrateToCurrentVersion`. Mutates in place, returns whether any migration
//! was applied.

use crate::entry::FileEntry;
use crate::header::{SessionHeader, CURRENT_SESSION_VERSION};

/// Run all necessary migrations to bring entries to the current version.
/// Mutates in place. Returns true if any migration was applied.
///
/// Oracle: `migrateToCurrentVersion` in session-manager.ts at v0.82.0.
pub fn migrate_to_current_version(entries: &mut [FileEntry]) -> bool {
    let version = header_version(entries).unwrap_or(1);
    if version >= CURRENT_SESSION_VERSION {
        return false;
    }
    if version < 2 {
        migrate_v1_to_v2(entries);
    }
    if version < 3 {
        migrate_v2_to_v3(entries);
    }
    true
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
/// Oracle: `migrateV1ToV2`. The Oracle generates short ids from randomUUID;
/// pi-rs generates them from uuid v7 truncated to 8 hex chars (collision
/// checked). The id format is validated by the same regex either way.
fn migrate_v1_to_v2(entries: &mut [FileEntry]) {
    // Set header version.
    for entry in entries.iter_mut() {
        if let FileEntry::Header(h) = entry {
            h.version = Some(2);
        }
    }
    // Assign ids. The Oracle assigns sequentially to non-header entries.
    // pi-rs does the same, generating a short id per entry (collision-checked).
    // Pre-build an index->id map (immutable) so the mutable loop can look up
    // firstKeptEntryIndex targets without borrowing `entries` again.
    let id_by_index: Vec<Option<String>> = entries
        .iter()
        .map(|e| entry_id(e).map(|s| s.to_string()))
        .collect();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut prev_id: Option<String> = None;
    for entry in entries.iter_mut() {
        if let FileEntry::Header(_) = entry {
            continue;
        }
        // For migration we operate on the JSON value to handle the v1 shape
        // (no id/parentId, possibly firstKeptEntryIndex on compaction).
        // Re-serialize to a value, mutate, re-parse. This is O(n) per entry
        // but migrations run once per file load.
        let value = entry_to_json(entry);
        let mut obj = value.as_object().cloned().unwrap_or_default();
        let first_kept_entry_id_from_index =
            if obj.get("type").and_then(|v| v.as_str()) == Some("compaction") {
                obj.get("firstKeptEntryIndex")
                    .and_then(|v| v.as_u64())
                    .and_then(|idx| id_by_index.get(idx as usize).cloned().flatten())
            } else {
                None
            };
        if obj
            .get("id")
            .and_then(|v| v.as_str())
            .is_none_or(|s| s.is_empty())
        {
            let id = generate_short_id(&used);
            used.insert(id.clone());
            obj.insert("id".into(), serde_json::Value::String(id));
        } else if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            used.insert(id.to_string());
        }
        obj.insert(
            "parentId".into(),
            prev_id
                .take()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        // Track current id as prev for the next entry.
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            prev_id = Some(id.to_string());
        }
        // Convert compaction.firstKeptEntryIndex -> firstKeptEntryId.
        if obj.get("type").and_then(|v| v.as_str()) == Some("compaction") {
            if let Some(target_id) = first_kept_entry_id_from_index {
                obj.insert(
                    "firstKeptEntryId".into(),
                    serde_json::Value::String(target_id),
                );
                obj.remove("firstKeptEntryIndex");
            }
        }
        *entry = json_to_entry(&serde_json::Value::Object(obj));
    }
}

/// v2 -> v3: set header version to 3; rename message role hookMessage -> custom.
///
/// Oracle: `migrateV2ToV3`.
fn migrate_v2_to_v3(entries: &mut [FileEntry]) {
    for entry in entries.iter_mut() {
        if let FileEntry::Header(h) = entry {
            h.version = Some(CURRENT_SESSION_VERSION);
            continue;
        }
        // Rename hookMessage -> custom on message entries.
        let value = entry_to_json(entry);
        if let Some(obj) = value.as_object() {
            if obj.get("type").and_then(|v| v.as_str()) == Some("message") {
                if let Some(msg) = obj.get("message").and_then(|v| v.as_object()) {
                    if msg.get("role").and_then(|v| v.as_str()) == Some("hookMessage") {
                        let mut new_obj = obj.clone();
                        if let Some(msg) =
                            new_obj.get_mut("message").and_then(|v| v.as_object_mut())
                        {
                            msg.insert("role".into(), serde_json::Value::String("custom".into()));
                        }
                        *entry = json_to_entry(&serde_json::Value::Object(new_obj));
                    }
                }
            }
        }
    }
}

fn entry_to_json(entry: &FileEntry) -> serde_json::Value {
    match entry {
        FileEntry::Header(h) => serde_json::to_value(h).unwrap_or(serde_json::Value::Null),
        FileEntry::Entry(e) => serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
    }
}

fn json_to_entry(value: &serde_json::Value) -> FileEntry {
    if value.get("type").and_then(|v| v.as_str()) == Some("session") {
        if let Ok(h) = serde_json::from_value::<SessionHeader>(value.clone()) {
            return FileEntry::Header(h);
        }
    }
    match serde_json::from_value::<crate::entry::SessionEntry>(value.clone()) {
        Ok(e) => FileEntry::Entry(e),
        Err(_) => FileEntry::Entry(crate::entry::SessionEntry::Label {
            base: crate::entry::EntryBase {
                id: "migration-fallback".into(),
                parent_id: None,
                timestamp: String::new(),
            },
            target_id: String::new(),
            label: None,
        }),
    }
}

fn entry_id(entry: &FileEntry) -> Option<&str> {
    match entry {
        FileEntry::Entry(e) => Some(e.id()),
        _ => None,
    }
}

/// Generate an 8-hex-char id, collision-checked against `used`.
/// Oracle: `generateId` uses randomUUID().slice(0,8).
fn generate_short_id(used: &std::collections::HashSet<String>) -> String {
    for _ in 0..100 {
        let id = uuid::Uuid::now_v7().simple().to_string();
        let short = &id[..8];
        if !used.contains(short) {
            return short.to_string();
        }
    }
    // Fallback: full uuid.
    uuid::Uuid::now_v7().to_string()
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
        assert!(!migrate_to_current_version(&mut entries));
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
        assert!(migrate_to_current_version(&mut entries));
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
        assert!(migrate_to_current_version(&mut entries));
        match &entries[1] {
            FileEntry::Entry(crate::entry::SessionEntry::Message { message, .. }) => {
                assert_eq!(message["role"], "custom", "hookMessage -> custom");
            }
            _ => panic!("entry moved"),
        }
    }
}
