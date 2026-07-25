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
        match classify(&value) {
            Some(FileEntry::Header(h)) => out.push(FileEntry::Header(h)),
            Some(FileEntry::Entry(e)) => out.push(FileEntry::Entry(e)),
            None => continue, // not a session header or known entry
        }
    }
    out
}

/// Classify a parsed JSON object as a header or an entry by its `type` field.
fn classify(value: &serde_json::Value) -> Option<FileEntry> {
    let obj = value.as_object()?;
    let ty = obj.get("type").and_then(|v| v.as_str())?;
    if ty == "session" {
        let header: SessionHeader = serde_json::from_value(value.clone()).ok()?;
        Some(FileEntry::Header(header))
    } else {
        let entry: crate::entry::SessionEntry = serde_json::from_value(value.clone()).ok()?;
        Some(FileEntry::Entry(entry))
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
    fn unknown_entry_type_is_skipped_not_error() {
        // Future entry types preserve forward-compat by skipping, not failing.
        let contents = "{\"type\":\"session\",\"version\":3,\"id\":\"h\",\"timestamp\":\"t\",\"cwd\":\"/c\"}\n\
            {\"type\":\"future_unknown_type\",\"id\":\"x\",\"parentId\":null,\"timestamp\":\"t\",\"weird\":42}\n";
        let entries = parse_session_str(contents);
        assert_eq!(entries.len(), 1, "unknown entry skipped");
    }
}
