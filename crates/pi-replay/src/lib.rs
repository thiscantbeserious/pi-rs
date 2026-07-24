//! pi-rs session-corpus replay harness (ADR 0007).
//!
//! Replays the real-world JSONL Session Corpus through the Core: every entry
//! must load and render without error, and re-saving must produce
//! byte-identical JSONL. Parity is measured, not asserted. Placeholder until
//! Phase 3 wires session read/write (ADRs 0008/0016).
