# Sessions use pi's native format, bidirectionally

pi-rs reads and writes pi's native session files (JSONL, same directory layout) rather than defining its own format. Any pi session resumes in pi-rs and vice versa — frictionless switching during the dogfood phase, extension state persisted via pi.appendEntry() keeps working unchanged, and the differential replay harness (ADR 0007) gets format identity for free.

## Considered Options

- Own format + one-way importer — rejected: dogfooding becomes one-way; falling back to pi mid-task is most valuable exactly when pi-rs has bugs
- Own format after parity — deferred, not rejected: a purpose-built format (indexed for the retained message model) may return as a post-parity optimization once pi-rs is the only daily tool

## Consequences

- pi's session entry semantics (entry types, branching, compaction entries) become a compatibility contract, tracked against the pinned oracle version and revisited at each re-baseline
- pi-rs must not write entries pi cannot read while bidirectional interop is a goal
