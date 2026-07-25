//! File and directory naming (ADR 0008, contract §1).
//!
//! Mirrors the Oracle's `getDefaultSessionDirPath` (slug) and `newSession`
//! file-name construction.

/// The session directory slug for a cwd: `--<cwd-with-separators-as-dashes>--`.
///
/// Oracle: `getDefaultSessionDirPath`:
/// `"--" + resolvedCwd.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-") + "--"`.
pub fn session_dir_slug(cwd: &str) -> String {
    let stripped = cwd.strip_prefix(|c| c == '/' || c == '\\').unwrap_or(cwd);
    let dashed: String = stripped
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' => '-',
            other => other,
        })
        .collect();
    format!("--{dashed}--")
}

/// The session file name: `<iso-timestamp-with-dashes>_<sessionId>.jsonl`.
///
/// Oracle: `newSession`:
/// `fileTimestamp = timestamp.replace(/[:.]/g, "-")` (ISO 8601 colons/dots
/// become dashes), then `${fileTimestamp}_${sessionId}.jsonl`.
pub fn session_file_name(iso_timestamp: &str, session_id: &str) -> String {
    let file_timestamp: String = iso_timestamp
        .chars()
        .map(|c| match c {
            ':' | '.' => '-',
            other => other,
        })
        .collect();
    format!("{file_timestamp}_{session_id}.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_strips_leading_separator_and_dashes_the_rest() {
        // Oracle: replace(/^[/\\]/, "") strips ONE leading separator, then
        // replace(/[/\\:]/g, "-") replaces each remaining separator/colon.
        assert_eq!(
            session_dir_slug("/Users/simonsanladerer/git/pi-rs"),
            "--Users-simonsanladerer-git-pi-rs--"
        );
        // C:\Users\simon: no leading separator to strip; ':' and '\' both dash.
        assert_eq!(session_dir_slug("C:\\Users\\simon"), "--C--Users-simon--");
        assert_eq!(session_dir_slug("/a:b"), "--a-b--");
    }

    #[test]
    fn file_name_replaces_colons_and_dots_in_timestamp() {
        let name = session_file_name("2026-07-25T12:34:56.789Z", "0197d001-uuidv7");
        assert_eq!(name, "2026-07-25T12-34-56-789Z_0197d001-uuidv7.jsonl");
    }
}
