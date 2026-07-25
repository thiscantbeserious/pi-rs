//! Line-by-line JSONL parsing (ADR 0008, contract §5). Blank and malformed
//! lines are skipped, matching the Oracle's best-effort parse. The first
//! parseable line must be a session header or the file is not a pi session.

use crate::entry::FileEntry;
use crate::header::SessionHeader;

/// Parse a session file's contents (newline-delimited JSON) into file entries.
/// Skips blank and malformed lines. Does NOT apply migrations (call
/// `migrate_to_current_version` on the result).
pub fn parse_session_str(contents: &str) -> Vec<FileEntry> {
    parse_session_lines(contents.lines())
}

/// Parse an iterator of lines into file entries. Same semantics as
/// `parse_session_str`. Public so callers can stream a file without buffering
/// the whole string.
pub fn parse_session_lines<'a, I: IntoIterator<Item = &'a str>>(lines: I) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed lines
        };
        // classify returns Header, Entry, or Unknown (never None): every
        // parseable JSON object is retained so re-save is byte-identical.
        out.push(classify(&value));
    }
    out
}

/// Classify a parsed JSON object as a header, a known entry, or an unknown
/// entry. Unknown entries (unrecognized `type`, or a known `type` that fails
/// to deserialize into the typed shape) are retained as `Unknown` with the raw
/// map so they survive re-save. Returns `Unknown(empty)` only if the value is
/// not a JSON object at all.
fn classify(value: &serde_json::Value) -> FileEntry {
    let Some(obj) = value.as_object() else {
        return FileEntry::Unknown(serde_json::Map::new());
    };
    let ty = obj.get("type").and_then(|v| v.as_str());
    match ty {
        Some("session") => match serde_json::from_value::<SessionHeader>(value.clone()) {
            Ok(h) => FileEntry::Header(h),
            Err(_) => FileEntry::Unknown(obj.clone()),
        },
        Some(_) => match serde_json::from_value::<crate::entry::SessionEntry>(value.clone()) {
            Ok(e) => FileEntry::Entry(e),
            Err(_) => FileEntry::Unknown(obj.clone()),
        },
        None => FileEntry::Unknown(obj.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::SessionEntry;

    #[test]
    fn parses_header_then_entries() {
        let contents = "{\"type\":\"session\",\"version\":3,\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n\
            {\"type\":\"message\",\"id\":\"m1\",\"parentId\":null,\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n\
            {\"type\":\"label\",\"id\":\"l1\",\"parentId\":\"m1\",\"timestamp\":\"t\",\"targetId\":\"m1\"}\n";
        let entries = parse_session_str(contents);
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], FileEntry::Header(_)));
        assert!(matches!(
            entries[1],
            FileEntry::Entry(SessionEntry::Message { .. })
        ));
        assert!(matches!(
            entries[2],
            FileEntry::Entry(SessionEntry::Label { .. })
        ));
    }

    #[test]
    fn skips_blank_and_malformed_lines() {
        let contents = "\n\
            {\"type\":\"session\",\"version\":3,\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n\
            this is not json\n\
            \n\
            {\"type\":\"label\",\"id\":\"l\",\"parentId\":null,\"timestamp\":\"t\",\"targetId\":\"x\"}\n";
        let entries = parse_session_str(contents);
        assert_eq!(entries.len(), 2, "blank and malformed lines skipped");
    }

    #[test]
    fn empty_input_yields_no_entries() {
        assert!(parse_session_str("").is_empty());
        assert!(parse_session_str("\n\n  \n").is_empty());
    }

    #[test]
    fn unknown_entry_type_is_retained_for_resave() {
        // Future entry types are retained as Unknown so re-save is byte-identical
        // (the Oracle's JSON.parse keeps every line; dropping would be data loss).
        let line = "{\"type\":\"future_unknown_type\",\"id\":\"x\",\"parentId\":null,\"timestamp\":\"t\",\"weird\":42}";
        let contents =
            "{\"type\":\"session\",\"version\":3,\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n"
                .to_string()
                + line + "\n";
        let entries = parse_session_str(&contents);
        assert_eq!(entries.len(), 2, "header + unknown retained");
        match &entries[1] {
            FileEntry::Unknown(map) => {
                assert_eq!(
                    map.get("type").and_then(|v| v.as_str()),
                    Some("future_unknown_type")
                );
                assert_eq!(map.get("weird").and_then(|v| v.as_u64()), Some(42));
                // Re-serialize byte-identically (preserve_order keeps key order).
                let re = serde_json::to_string(map).unwrap();
                assert_eq!(re, line, "unknown entry round-trips byte-identically");
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }
}
