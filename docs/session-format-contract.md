# Session format contract (parity with the pinned Oracle)

The on-disk session format is a parity contract: pi-rs reads and writes the same JSONL files pi does, bidirectionally (ADR 0008), and re-saving a session must be byte-identical (ADR 0007 exit gate). This document is the single typed specification of that format, extracted from the pinned Oracle's `packages/coding-agent/src/core/session-manager.ts` at `v0.82.0` (ADR 0007). The Rust types live in the `pi-session` crate (ADR 0011 amendment); both `pi-core` (sole writer, ADR 0016) and `pi-replay` (replay reader) depend on it.

Every shape below is sourced to the Oracle with a permalink. Where pi-rs adds nothing (it mirrors pi exactly), that is stated. Where there is no pi equivalent, that is stated explicitly (PHILOSOPHY §9.5).

## 1. File and directory layout

Sessions are append-only JSONL files, one JSON object per line, under `~/.pi/agent/sessions/` (ADR 0020 shared tree).

- **Session directory** for a cwd: `~/.pi/agent/sessions/<slug>` where `slug = "--" + cwd.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-") + "--"`. The leading slash/backslash is stripped, then every remaining path separator or colon becomes `-`, and the whole thing is wrapped in `--...--`. [Oracle: `getDefaultSessionDirPath`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L331-L339)
- **Session file name**: `${fileTimestamp}_${sessionId}.jsonl` where `fileTimestamp = new Date().toISOString().replace(/[:.]/g, "-")` (colons and dots in the ISO timestamp become dashes) and `sessionId` is a UUIDv7. [Oracle: `newSession`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L421-L426)
- **Session ID**: UUIDv7 (`uuidv7()` from `@earendil-works/pi-ai`). Validated by `/^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/` (alphanumeric, `-`, `_`, `.`, must start and end alphanumeric). [Oracle: `createSessionId`, `assertValidSessionId`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L248-L260)
- **Short entry IDs** (v1 migration only): 8 hex chars from `randomUUID().slice(0,8)`, collision-checked against existing IDs. [Oracle: `generateId`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L262-L272)

pi-rs must replicate naming and discovery (`findMostRecentSession` filters by `cwd` match, sorts by mtime) for resume parity.

## 2. Versioning and migrations

`CURRENT_SESSION_VERSION = 3`. The header's `version?` is optional: v1 sessions omit it. Two migrations run on load, in order, mutating in place. [Oracle: `migrateToCurrentVersion`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L289-L302)]

- **v1 → v2** (`migrateV1ToV2`): set header `version = 2`; give every non-header entry an `id` (8-hex, collision-checked) and `parentId` = previous entry's id (null for the first); convert `compaction.firstKeptEntryIndex` (a positional number) to `firstKeptEntryId` (the resolved entry id), deleting the index. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L274-L287)
- **v2 → v3** (`migrateV2ToV3`): set header `version = 3`; for `message` entries whose `message.role === "hookMessage"`, rename the role to `"custom"`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L289-L302)

pi-rs must read v1/v2/v3 and apply the same migrations in the same order. It must not write entries pi cannot read while bidirectional interop holds (ADR 0008). pi-rs always writes v3.

## 3. Header

```rust
pub struct SessionHeader {
    pub r#type: () /* always "session" */,
    pub version: Option<u32>,        // None on v1, Some(3) on v3 writes
    pub id: String,                  // UUIDv7
    pub timestamp: String,           // ISO 8601
    pub cwd: String,
    pub parent_session: Option<String>, // present on forked sessions
}
```

[Oracle: `SessionHeader`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L20-L27). The `parentSession?` field (forked sessions) is absent from `docs/research.md`'s note and must be carried.

Wire field names are `parentSession` (camelCase), not `parent_session`. The Rust struct uses `#[serde(rename = "parentSession")]`.

## 4. Entry taxonomy (nine types)

Every entry (non-header) shares a base: `type` (discriminator), `id: String`, `parentId: Option<String>`, `timestamp: String`. The `type` field selects the variant. Entries form a tree via `id`/`parentId`; the session manager tracks a `leafId` pointer for the current position.

[Oracle: `SessionEntryBase` and the nine entry interfaces](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L29-L139)

1. **`message`** — `SessionMessageEntry`. Carries `message: AgentMessage` (user/assistant/toolResult, see `@earendil-works/pi-ai` `Message`). Null content on user/assistant/toolResult is normalized to `[]` on read. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L37-L40)
2. **`thinking_level_change`** — `thinkingLevel: string` (one of `off|minimal|low|medium|high|xhigh|max`). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L42-L45)
3. **`model_change`** — `provider: string`, `modelId: string`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L47-L50)
4. **`compaction`** — `summary: string`, `firstKeptEntryId: string`, `tokensBefore: number`, `details?: T`, `usage?: Usage`, `fromHook?: bool`. Marks a compaction boundary; context reconstruction replaces everything before it (see §6). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L52-L62)
5. **`branch_summary`** — `fromId: string`, `summary: string`, `details?: T`, `usage?: Usage`, `fromHook?: bool`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L64-L71)
6. **`custom`** — `customType: string`, `data?: T`. Extension state persistence. Does NOT participate in LLM context. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L84-L93)
7. **`custom_message`** — `customType: string`, `content: string | (TextContent|ImageContent)[]`, `details?: T`, `display: bool`. DOES participate in LLM context (projected to a user message). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L109-L118)
8. **`label`** — `targetId: string`, `label: Option<String>` (None/undefined clears). User bookmark on an entry. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L120-L124)
9. **`session_info`** — `name?: string`. Session display name (latest wins, including explicit clears). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L127-L130)

The `custom` vs `custom_message` distinction is parity-critical: `custom` is invisible to the LLM, `custom_message` is injected as a user message. Confusing them breaks both extension state and LLM context.

## 5. File entry (header or entry)

```rust
pub enum FileEntry {
    Header(SessionHeader),
    Entry(SessionEntry),
}
```

A file is a sequence of `FileEntry` lines, first line is the header. Parsing skips blank and malformed lines (best-effort). [Oracle: `parseSessionEntries`, `parseSessionEntryLine`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L304-L323)]

## 6. Context reconstruction (the parity-critical projection)

Replaying a session for the LLM is not "concatenate all messages." Three steps, all of which pi-rs must reproduce or sessions will not replay green (ADR 0007).

### 6.1 Path from leaf to root

`buildSessionPath(entries, leafId)`: index entries by id; starting from `leafId` (or the last entry if unspecified, or empty if null), walk `parentId` to root, reverse. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L325-L349)]

### 6.2 Compaction-aware context entries

`buildContextEntries(entries, leafId)`: take the path; if a `compaction` entry is on it, replace everything before the compaction with `[compaction, firstKeptEntryId.., entries after compaction]`. The compaction entry itself represents the summarized prefix; kept entries start at `firstKeptEntryId`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L454-L489)]

### 6.3 Entry → context messages

`sessionEntryToContextMessages(entry)` projects each entry type to zero or more `AgentMessage`:

- `message` → `[message]` (null content normalized to `[]` for user/assistant/toolResult). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L358-L374)
- `custom_message` → `[createCustomMessage(customType, content, display, details, timestamp)]` (injected as a user message). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L376-L379)
- `branch_summary` (with summary) → `[createBranchSummaryMessage(summary, fromId, timestamp)]`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L380-L382)
- `compaction` → `[createCompactionSummaryMessage(summary, tokensBefore, timestamp)]`. [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L383-L385)
- `thinking_level_change`, `model_change`, `custom`, `label`, `session_info` → `[]` (not LLM context). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L386-L388)

### 6.4 Session context settings

`getSessionContextSettings(path)` walks the path and records the latest `thinkingLevel` (from `thinking_level_change`, default `"off"`) and `model` (from `model_change`, or from the latest `assistant` message's `provider`/`model`). [Oracle](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L351-L374)]

## 7. Append-only, sole-writer discipline

The file is append-only. Only the Core writes it (ADR 0016); extension `appendEntry` calls route over the Host Protocol to the Core. Rewrites happen only on migration (`_rewriteFile`) or when an empty file is initialized. [Oracle: `_persist`, `_rewriteFile`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts#L470-L500)]

## 8. Rust module layout (pi-session crate)

Proposed layout, mirroring the Oracle's responsibilities split:

```
crates/pi-session/
  Cargo.toml
  src/
    lib.rs          // re-exports
    header.rs       // SessionHeader, CURRENT_SESSION_VERSION
    entry.rs        // SessionEntry (9 variants), FileEntry
    parse.rs        // line-by-line JSONL parse, skip blank/malformed
    migrate.rs      // v1->v2, v2->v3, migrateToCurrentVersion
    naming.rs       // session dir slug, file name, discovery
    context.rs      // buildSessionPath, buildContextEntries, sessionEntryToContextMessages
```

Dependencies: `serde`, `serde_json`, `uuid` (v7). No tokio (parse/read are sync, matching the Oracle's sync `loadEntriesFromFile`). `pi-core` and `pi-replay` depend on `pi-session` via the workspace table.

## 9. What pi-rs adds (no Oracle equivalent)

None at the format level. pi-rs mirrors pi exactly for bidirectional interop. The Host Protocol message that carries an `appendEntry` request (ADR 0016) is pi-rs-native (pi is single-process, no protocol), and is specified in the Host Protocol, not here. A future purpose-built indexed format is a post-parity option (ADR 0008).

## 10. Open questions for Phase 3

- **Message type fidelity.** `SessionMessageEntry.message` is an `AgentMessage` from `@earendil-works/pi-ai`. The `Message` union (`UserMessage | AssistantMessage | ToolResultMessage`) and its content block types (`TextContent`, `ThinkingContent`, `ImageContent`, `ToolCall`) must be mirrored in Rust or shared via ts-rs from `pi-protocol`-adjacent types. Decide whether `pi-session` owns these message types or depends on a future `pi-messages` crate. The Oracle source: [`packages/ai/src/types.ts`](https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts).
- **`details?: T` and `data?: T` generics.** Extension-defined opaque payloads. The Rust types must preserve unknown JSON (e.g. `serde_json::Value`) without dropping fields, or byte-identical re-save fails.
- **`Usage` type.** Shared between compaction, branch_summary, and tool results. Mirror from `@earendil-works/pi-ai`.
- **Discovery and `findMostRecentSession`.** Needed for resume; replicate the cwd-filter + mtime sort.

## Sources

1. Oracle session manager (the reference implementation), v0.82.0: <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts>
2. Oracle message and content block types (`AgentMessage`, `Message`, `Usage`), v0.82.0: <https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts>
3. pi session file format docs: <https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/session-format.md>
