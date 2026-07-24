# Sessions use pi's native format, bidirectionally

pi-rs reads and writes pi's native session files (JSONL under ~/.pi/agent/sessions/, entries forming a tree via id/parentId for in-place branching [[1]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/session-format.md)) rather than defining its own format. The reference implementation is pi's append-only JSONL storage and session manager [[2]](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/session-manager.ts). Any pi session resumes in pi-rs and vice versa - frictionless switching during the dogfood phase, extension state persisted via pi.appendEntry() keeps working unchanged, and the differential replay harness (ADR 0007) gets format identity for free.

## Considered Options

- Own format + one-way importer - rejected: dogfooding becomes one-way. Falling back to pi mid-task is most valuable exactly when pi-rs has bugs
- Own format after parity - deferred, not rejected: a purpose-built format (indexed for the retained message model) may return as a post-parity optimization once pi-rs is the only daily tool

## Consequences

- pi's session entry semantics (entry types, branching, compaction entries) become a compatibility contract, tracked against the pinned oracle version and revisited at each re-baseline
- pi-rs must not write entries pi cannot read while bidirectional interop is a goal

## Sources

1. pi session file format, JSONL with id/parentId tree and in-place branching: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/session-format.md
2. pi session manager, append-only tree with leaf pointer (reference implementation): https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/session-manager.ts
