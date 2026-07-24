# The Core is the sole session writer

Only the Core writes session JSONL files. Extension calls to pi.appendEntry() are routed over the Host Protocol to the Core, which serializes all appends through one writer. Single-writer discipline prevents interleaving corruption on an append-only file and keeps ADR 0008's byte-identical re-save property mechanically testable.

## Considered Options

- Host writes entries directly to the shared file — rejected: two writers on one append-only file is a corruption factory and bypasses the retained model as single source of truth

## Consequences

- The Host Protocol carries an append-entry message; entry ordering is the Core's responsibility
- Extension-persisted state remains fully compatible (same entries, same format)
