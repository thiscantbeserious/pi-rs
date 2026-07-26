//! pi-rs render subsystem (ADR 0026).
//!
//! The render thread, Retained Message Model, streaming markdown pipeline,
//! grapheme-width engine, and theme loader. See ADRs 0024-0026 and
//! `docs/plans/phase-2-render-core.md`. The render thread OWNS the terminal
//! and never awaits (ADR 0013); everything async runs on the tokio runtime on
//! other threads and communicates via channels.

pub mod suspend;
pub mod terminal;
