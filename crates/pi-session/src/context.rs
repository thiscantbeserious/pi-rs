//! Compaction-aware context reconstruction (ADR 0008, contract §6).
//!
//! Mirrors the Oracle's `buildSessionPath`, `buildContextEntries`, and
//! `sessionEntryToContextMessages`. The projection from on-disk entries to
//! LLM context messages is parity-critical: pi-replay and the agent loop must
//! reproduce it or sessions will not replay green (ADR 0007).
//!
//! The message-level projection (`AgentMessage`, content blocks) is deferred
//! to a future `pi-messages` crate (contract §10). This module returns the
//! selected entries and a marker for each; the caller projects to messages.

use crate::entry::SessionEntry;
use std::collections::HashMap;

/// Build the leaf-to-root path of entries, reversed to root-first.
/// `leaf_id` selects the leaf; None means the last entry; Some(null) is
/// represented as None here (empty path).
///
/// Oracle: `buildSessionPath`.
pub fn build_session_path<'a>(
    entries: &'a [SessionEntry],
    leaf_id: Option<&str>,
    by_id: Option<&HashMap<String, &'a SessionEntry>>,
) -> Vec<&'a SessionEntry> {
    let owned_index: HashMap<String, &'a SessionEntry>;
    let index = match by_id {
        Some(b) => b,
        None => {
            owned_index = build_index(entries);
            &owned_index
        }
    };
    let leaf = match leaf_id {
        None | Some("") => entries.last(),
        Some(id) => index.get(id).copied().or_else(|| entries.last()),
    };
    let mut path = Vec::new();
    let mut current = leaf;
    while let Some(entry) = current {
        path.push(entry);
        current = entry.parent_id().and_then(|pid| index.get(pid).copied());
    }
    path.reverse();
    path
}

/// Build the compaction-aware context entries: the leaf path, with everything
/// before the latest compaction replaced by `[compaction, firstKeptEntryId..]`.
///
/// Oracle: `buildContextEntries`.
pub fn build_context_entries<'a>(
    entries: &'a [SessionEntry],
    leaf_id: Option<&str>,
    by_id: Option<&HashMap<String, &'a SessionEntry>>,
) -> Vec<&'a SessionEntry> {
    let path = build_session_path(entries, leaf_id, by_id);
    let compaction = path
        .iter()
        .rev()
        .find(|e| matches!(e, SessionEntry::Compaction { .. }));
    let Some(compaction) = compaction else {
        return path;
    };
    let compaction_id = compaction.id().to_string();
    let compaction_idx = path.iter().position(|e| e.id() == compaction_id).unwrap();
    let first_kept_entry_id = match compaction {
        SessionEntry::Compaction {
            first_kept_entry_id,
            ..
        } => first_kept_entry_id.as_deref(),
        _ => unreachable!("matched compaction"),
    };
    let Some(first_kept_entry_id) = first_kept_entry_id else {
        // A compaction without firstKeptEntryId (un-migrated v1). Nothing to
        // keep before it; return the compaction plus everything after.
        let mut context: Vec<&SessionEntry> = vec![compaction];
        context.extend(path.iter().skip(compaction_idx + 1));
        return context;
    };
    let mut context: Vec<&SessionEntry> = vec![compaction];
    let mut found_first_kept = false;
    for entry in path.iter().take(compaction_idx) {
        if entry.id() == first_kept_entry_id {
            found_first_kept = true;
        }
        if found_first_kept {
            context.push(entry);
        }
    }
    context.extend(path.iter().skip(compaction_idx + 1));
    context
}

/// The kind of context contribution an entry makes. `None` for entries that
/// do not participate in LLM context (custom, label, session_info, etc.).
///
/// Oracle: `sessionEntryToContextMessages`. The actual `AgentMessage`
/// projection is deferred (contract §10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextContribution {
    /// A `message` entry, projected verbatim (null content normalized to []).
    Message,
    /// A `custom_message` entry, projected as a user message.
    CustomMessage,
    /// A `branch_summary` entry with a non-empty summary.
    BranchSummary,
    /// A `compaction` entry, projected as a compaction summary message.
    Compaction,
}

/// Classify an entry's context contribution. Returns None for entries that
/// do not participate in LLM context.
///
/// Oracle: `sessionEntryToContextMessages` (the variant-to-message mapping).
pub fn session_entry_to_context_messages(entry: &SessionEntry) -> Option<ContextContribution> {
    match entry {
        SessionEntry::Message { .. } => Some(ContextContribution::Message),
        SessionEntry::CustomMessage { .. } => Some(ContextContribution::CustomMessage),
        SessionEntry::BranchSummary { summary, .. } if !summary.is_empty() => {
            Some(ContextContribution::BranchSummary)
        }
        SessionEntry::Compaction { .. } => Some(ContextContribution::Compaction),
        // thinking_level_change, model_change, custom, label, session_info,
        // branch_summary with empty summary: no context contribution.
        _ => None,
    }
}

fn build_index(entries: &[SessionEntry]) -> HashMap<String, &SessionEntry> {
    entries.iter().map(|e| (e.id().to_string(), e)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryBase;

    fn base(id: &str, parent: Option<&str>) -> EntryBase {
        EntryBase {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            timestamp: "t".into(),
        }
    }

    fn msg(id: &str, parent: Option<&str>) -> SessionEntry {
        SessionEntry::Message {
            base: base(id, parent),
            message: serde_json::json!({"role":"user","content":"hi"}),
        }
    }

    #[test]
    fn path_walks_leaf_to_root_reversed() {
        let entries = vec![
            msg("m1", None),
            msg("m2", Some("m1")),
            msg("m3", Some("m2")),
        ];
        let path = build_session_path(&entries, Some("m3"), None);
        assert_eq!(
            path.iter().map(|e| e.id()).collect::<Vec<_>>(),
            vec!["m1", "m2", "m3"]
        );
    }

    #[test]
    fn context_entries_without_compaction_is_the_path() {
        let entries = vec![msg("m1", None), msg("m2", Some("m1"))];
        let ctx = build_context_entries(&entries, Some("m2"), None);
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn context_entries_replaces_prefix_before_compaction() {
        // m1, compaction(kept=m2), m2, m3 -> [compaction, m2, m3]
        let entries = vec![
            msg("m1", None),
            SessionEntry::Compaction {
                base: base("c1", Some("m1")),
                summary: "s".into(),
                first_kept_entry_id: Some("m2".into()),
                first_kept_entry_index: None,
                tokens_before: 100,
                details: None,
                usage: None,
                from_hook: None,
            },
            msg("m2", Some("c1")),
            msg("m3", Some("m2")),
        ];
        let ctx = build_context_entries(&entries, Some("m3"), None);
        let ids: Vec<&str> = ctx.iter().map(|e| e.id()).collect();
        assert_eq!(
            ids,
            vec!["c1", "m2", "m3"],
            "m1 dropped, compaction + kept + after"
        );
    }

    #[test]
    fn custom_does_not_contribute_custom_message_does() {
        let custom = SessionEntry::Custom {
            base: base("c1", None),
            custom_type: "x".into(),
            data: None,
        };
        let custom_msg = SessionEntry::CustomMessage {
            base: base("cm1", None),
            custom_type: "x".into(),
            content: serde_json::Value::String("hi".into()),
            details: None,
            display: true,
        };
        assert_eq!(session_entry_to_context_messages(&custom), None);
        assert_eq!(
            session_entry_to_context_messages(&custom_msg),
            Some(ContextContribution::CustomMessage)
        );
    }

    #[test]
    fn label_and_session_info_do_not_contribute() {
        let label = SessionEntry::Label {
            base: base("l1", None),
            target_id: "x".into(),
            label: Some("m".into()),
        };
        let info = SessionEntry::SessionInfo {
            base: base("s1", None),
            name: Some("n".into()),
        };
        assert_eq!(session_entry_to_context_messages(&label), None);
        assert_eq!(session_entry_to_context_messages(&info), None);
    }
}
