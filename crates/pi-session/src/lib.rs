//! pi's on-disk session format: the typed parity contract (ADR 0008,
//! docs/session-format-contract.md). Header, nine entry types, v1/v2/v3
//! migrations, file/dir naming, and compaction-aware context reconstruction.
//!
//! Mirrors the pinned Oracle's `packages/coding-agent/src/core/session-manager.ts`
//! at v0.82.0 (ADR 0007). pi-rs reads and writes the same JSONL files pi does,
//! bidirectionally; re-saving is byte-identical (ADR 0007 exit gate). Both
//! `pi-core` (sole writer, ADR 0016) and `pi-replay` (replay reader) depend
//! on this crate so the format has one Rust source of truth.

pub mod context;
pub mod entry;
pub mod header;
pub mod migrate;
pub mod naming;
pub mod parse;

pub use context::{build_context_entries, build_session_path, session_entry_to_context_messages};
pub use entry::{FileEntry, SessionEntry};
pub use header::{SessionHeader, CURRENT_SESSION_VERSION};
pub use migrate::migrate_to_current_version;
pub use naming::{session_dir_slug, session_file_name};
pub use parse::{parse_session_lines, parse_session_str};
