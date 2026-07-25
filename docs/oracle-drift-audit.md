# Oracle drift audit: pi-rs implementation vs pinned pi

Audit date: 2026-07-25. Method: compare every pi-rs doc claim and the Phase 1 implementation against the pinned Oracle (pi `0.82.0`, ADR 0007) source at the `v0.82.0` tag. Sourced per PHILOSOPHY §9. This doc feeds `docs/research.md` and the open-questions queue; findings that contradict an accepted ADR or roadmap gate are flagged for the project owner per §9 rule 3 (correct in place or supersede, never silent).

## Severity scale

- **HIGH**: contradicts an accepted ADR or roadmap gate. Blocks parity if unaddressed.
- **MEDIUM**: a parity contract is under-specified or incomplete. Will cause Phase 3/4 rework.
- **LOW**: minor omission or stale phrasing. No near-term blocker.

## Summary

| # | Finding | Severity | ADRs/docs affected |
| --- | --------- | ---------- | -------------------- |
| D1 | Oracle pin `0.82.0` no longer npm `latest`; `0.82.1` released | LOW | ADR 0007 |
| D2 | "Four API types" claim is wrong: Oracle has 10 `KnownApi` | HIGH | ADR 0019, ADR 0005, ROADMAP Phase 4, research.md |
| D3 | `pi.exec` missing from extension API surface checklist | MEDIUM | docs/extension-api-surface.md |
| D4 | Built-in tool set omits `ls` | LOW | ADR 0015 |
| D5 | Session format under-specified vs Oracle (versions, migrations, entry taxonomy, file naming) | MEDIUM | ADR 0008, research.md |
| D6 | `registerShortcut`/`registerFlag` `description?` omitted from surface doc | LOW | docs/extension-api-surface.md |
| D7 | `tests/e2e_test.sh` is a version-only stub, not a chaos test | LOW | AGENTS.md gates |

Phase 1 implementation (`pi-protocol`, `pi-core` host supervisor, host codec/conformance) is internally consistent with ADRs 0011/0022/0023 and was not found to drift from any Oracle behavior, because the Host Protocol is pi-rs-native by design (pi is single-process, no Oracle equivalent). The drift is in the **parity-target documentation**, not the Phase 1 code.

---

## D1 — Oracle pin no longer npm `latest`

ADR 0007 records the Oracle as `0.82.0`, "npm `latest` tag on spike-start day (2026-07-24)". As of 2026-07-25 the npm registry `latest` resolves to `0.82.1` [1](https://registry.npmjs.org/@earendil-works/pi-coding-agent/latest), and the GitHub releases page lists `v0.82.1` (Claude Opus 5, Anthropic gateway bearer auth, model catalog `If-None-Match` revalidation) [2](https://github.com/earendil-works/pi/releases). `0.82.1` shipped after the spike.

This is **not a violation**: ADR 0007 states re-baseline is deliberate and never silent. The finding is that the "latest tag on spike-start day" phrasing is now stale and a `0.82.1` re-baseline decision is pending. The v0.82.1 changelog adds provider/auth surface (Opus 5, `ANTHROPIC_AUTH_TOKEN` bearer, llama.cpp catalog persistence) that Phase 3 provider/auth work would inherit if re-baselined.

**Action for project owner**: decide before Phase 3 provider work whether to re-pin to `0.82.1` (re-vendor, re-extract the API surface, re-run the loader spike) or hold at `0.82.0`. Either is fine; the decision must be recorded in ADR 0007.

---

## D2 — "Four API types" claim is wrong (HIGH)

ADR 0019 states "pi's entire provider catalog rests on just these four API types" and names `openai-completions`, `anthropic-messages`, `openai-responses`, `google-generative-ai`. ADR 0005, `docs/research.md` ("pi providers are (baseUrl + `api` type + auth)... Native Rust providers are therefore implemented per API type: openai-completions first... anthropic-messages second, then openai-responses and google-generative-ai"), and ROADMAP Phase 4 ("completing pi's four-type provider catalog") all repeat the four-type claim.

The pinned Oracle source contradicts this. `packages/ai/src/types.ts` at `v0.82.0` defines:

```ts
export type KnownApi =
 | "openai-completions"
 | "mistral-conversations"
 | "openai-responses"
 | "azure-openai-responses"
 | "openai-codex-responses"
 | "anthropic-messages"
 | "bedrock-converse-stream"
 | "google-generative-ai"
 | "google-vertex"
 | "pi-messages";
```

That is **10** known API types, each with its own options type in `ApiOptionsMap` and its own implementation module under `packages/ai/src/api/` [3](https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts). The four named in ADR 0019 are the author's daily drivers and the right **priority order**, but they are not the full catalog. Full parity (ROADMAP Phase 4 exit gate "Ported oracle tests pass") requires the other six as well: `mistral-conversations`, `azure-openai-responses`, `openai-codex-responses`, `bedrock-converse-stream`, `google-vertex`, `pi-messages`. `pi-messages` is pi's own unified internal API and may warrant special attention.

Per PHILOSOPHY §9 rule 3, this contradicts accepted ADRs (0019, 0005) and a roadmap gate (Phase 4). It is not silently fixed here.

**Action for project owner**: choose one.

1. Correct ADR 0019 in place: "four daily-driver API types first, ten total for full parity", update ROADMAP Phase 4 deliverable to enumerate all ten, update research.md.
2. Supersede ADR 0019 with a new ADR that re-scopes the provider parity surface to all ten `KnownApi` values, keeping the four-type priority order.

The four-type priority ordering is sound and unchanged; only the parity total is wrong.

---

## D3 — `pi.exec` missing from the extension API surface checklist

`docs/extension-api-surface.md` opens with "Every entry below is part of the extension-facing contract" and Section B enumerates the `pi.*` call surface. It omits `pi.exec`.

The Oracle `ExtensionAPI` interface declares [4](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/types.ts):

```ts
/** Execute a shell command. */
exec(command: string, args: string[], options?: ExecOptions): Promise<ExecResult>;
```

The loader wires it host-local (delegates to `execCommand`, not via `ExtensionActions`) [5](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/loader.ts). So `pi.exec` is **host-local state/action, not a Host Protocol message**. But it is still part of the extension-facing contract: extensions can and do call `pi.exec`, and a Phase 3 host binding that misses it breaks those extensions. The surface doc's completeness claim is wrong.

**Action**: add `pi.exec` to Section B of `docs/extension-api-surface.md`, marked `local` (host-owned), with a note that it is host-local exec, not a protocol call. Cross-check whether ADR 0021's "thin binding layer" covers it.

---

## D4 — Built-in tool set omits `ls`

ADR 0015 lists the built-in tools as "read, edit, write, bash, grep/find". The Oracle `types.ts` defines `LsToolCallEvent`, `LsToolResultEvent`, `LsToolInput`, `LsToolDetails` alongside bash/read/edit/write/grep/find [4](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/types.ts). `ls` is a built-in tool in the Oracle. ADR 0015 omits it.

**Action**: amend ADR 0015's tool list to "read, edit, write, bash, grep, find, ls". Confirm against `packages/coding-agent/src/core/tools/` before Phase 3.

---

## D5 — Session format under-specified vs Oracle

ADR 0008 and `docs/research.md` ("Real pi session JSONL inspected: header entry (cwd, id, timestamp, type, version), subsequent entries carry id + parentId") describe the session format at a high level. The Oracle `packages/coding-agent/src/core/session-manager.ts` at `v0.82.0` [6](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts) reveals parity-critical detail the doc trail does not capture. For byte-identical re-save (ADR 0007 exit gate) and bidirectional interop (ADR 0008), pi-rs must replicate all of the following:

1. **Versioning and migrations.** `CURRENT_SESSION_VERSION = 3`. `version?` is optional on the header (v1 sessions omit it). Two migrations exist and run on load:
   - `migrateV1ToV2`: add `id`/`parentId` tree, convert `compaction.firstKeptEntryIndex` → `firstKeptEntryId`.
   - `migrateV2ToV3`: rename message role `hookMessage` → `custom`.
   pi-rs must read v1/v2/v3 and apply the same migrations in the same order, and must not write entries pi cannot read (ADR 0008).

2. **Header shape.** `SessionHeader = { type: "session"; version?: number; id: string; timestamp: string; cwd: string; parentSession?: string }`. The `parentSession?` field (forked sessions) is absent from the research note. `version` is not always present.

3. **Entry taxonomy.** Nine entry types, not "entries with id + parentId": `message`, `thinking_level_change`, `model_change`, `compaction`, `branch_summary`, `custom`, `custom_message`, `label`, `session_info`. Each has its own shape (`compaction` carries `firstKeptEntryId`, `tokensBefore`, `details?`, `usage?`, `fromHook?`; `branch_summary` carries `fromId`, `summary`, `details?`, `usage?`, `fromHook?`; `custom` and `custom_message` are distinct: `custom` does not participate in LLM context, `custom_message` does).

4. **File and directory naming.** Session file: `${fileTimestamp}_${sessionId}.jsonl` where `fileTimestamp = new Date().toISOString().replace(/[:.]/g, "-")`. Session directory: `~/.pi/agent/sessions/<slug>` where `slug = "--" + cwd.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-") + "--"`. Discovery (`findMostRecentSession`) filters by `cwd` match and sorts by mtime. pi-rs must replicate naming and discovery for resume parity.

5. **Context reconstruction.** `buildContextEntries` walks the leaf-to-root path, and if a `compaction` entry is on the path, replaces everything before it with `[compaction, firstKeptEntryId.., entries after compaction]`. `sessionEntryToContextMessages` projects each entry type to LLM messages (`custom_message` → user message via `createCustomMessage`; `branch_summary` → `createBranchSummaryMessage`; `compaction` → `createCompactionSummaryMessage`; null content on user/assistant/toolResult normalized to `[]`). pi-replay and the agent loop must reproduce this projection or sessions will not replay green.

**Action**: expand ADR 0008 (or a new companion ADR) to capture versions, migrations, the nine entry types, file/dir naming, and context reconstruction. Pull the entry-type shapes into a typed contract (this is the "single types contract" the session work needs). Schedule a session-corpus replay spike before Phase 3 session writing lands.

---

## D6 — `registerShortcut` / `registerFlag` `description?` omitted

`docs/extension-api-surface.md` Section B lists `registerShortcut(shortcut, options)` with `handler(ctx)` and `registerFlag(name, options)` with `type: boolean|string`, `default?`. The Oracle includes `description?` on both [4](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/types.ts). Minor; the surface doc is a checklist not a type mirror, but `description` is user-visible (shortcut/flag help) so a host binding should carry it.

**Action**: add `description?` to both entries in the surface doc.

---

## D7 — `tests/e2e_test.sh` is a version-only stub

AGENTS.md lists `./tests/e2e_test.sh` as a non-negotiable gate. The script currently asserts only that the binary prints a `pi-rs ...` version line. The Phase 1 chaos-test gate ("kill -9 the host: Core survives, prompts, respawns") is exercised by `crates/pi-core/tests/supervisor_integration.rs`, not by `e2e_test.sh`. Not an Oracle drift (pi has no equivalent), but the gate named in AGENTS.md is thinner than its name implies.

**Action**: either rename the gate to reflect what it checks, or expand `e2e_test.sh` to invoke the supervisor chaos path against the compiled host binary. Low priority; flagged for honesty per §9.

---

## What was verified and found faithful

To avoid implying the docs are broadly wrong, the following were checked against the `v0.82.0` source and match:

- **Loader signatures** (surface doc Section A): `loadExtensions`, `loadExtensionsCached`, `discoverAndLoadExtensions(configuredPaths, cwd, agentDir?, eventBus?)`, `loadExtensionFromFactory(factory, cwd, eventBus, runtime, extensionPath?)`, `createExtensionRuntime`, `clearExtensionCache`, `ExtensionFactory = (pi) => void | Promise<void>`, inline `{ name, factory, hidden? }` — all match [5](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/loader.ts).
- **Event result types** (Section C): `project_trust` → `{trusted: yes|no|undecided, remember?}`; `resources_discover` → `{skillPaths?, promptPaths?, themePaths?}`; `session_before_switch` → `{cancel?}`; `session_before_fork` → `{cancel?, skipConversationRestore?}`; `session_before_compact` → `{cancel?, compaction?}`; `session_before_tree` → `{cancel?, summary?, customInstructions?, replaceInstructions?, label?}`; `context` → `{messages?}`; `before_agent_start` → `{message?, systemPrompt?}`; `message_end` → `{message?}`; `tool_call` → `{block?, reason?}` with mutable `event.input`; `tool_result` → `{content?, details?, isError?, usage?}`; `user_bash` → `{operations?, result?}`; `input` → `continue|transform|handled` — all match [4](https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/types.ts).
- **ExtensionContext / ExtensionUIContext / ExtensionCommandContext / ReplacedSessionContext** (Sections D, E, F): method sets and signatures match.
- **ToolDefinition** (Section G): `name`, `label`, `description`, `parameters`, `promptSnippet?`, `promptGuidelines?`, `constrainedSampling?: false | ConstrainedSamplingConfig`, `renderShell?: default|self`, `prepareArguments?`, `executionMode?: sequential|parallel`, `execute(toolCallId, params, signal, onUpdate, ctx)`, `renderCall?`, `renderResult?`, `defineTool` — match.
- **Runtime seams** (Section H): `ExtensionActions`, `ExtensionContextActions`, `ExtensionCommandContextActions`, `ExtensionRuntimeState` member lists match.
- **Phase 1 implementation vs ADRs 0011/0022/0023**: `pi-protocol` messages/framing/fixtures, `pi-core` host supervisor state machine, host codec/conformance are internally consistent with their ADRs. No Oracle equivalent exists (Host Protocol is pi-rs-native), so no parity drift is possible here.

## Sources

1. npm registry, `@earendil-works/pi-coding-agent` `latest` manifest (now `0.82.1`): <https://registry.npmjs.org/@earendil-works/pi-coding-agent/latest>
2. pi GitHub releases (v0.82.1 listed above v0.82.0): <https://github.com/earendil-works/pi/releases>
3. pi-ai `KnownApi` type, 10 API types at v0.82.0: <https://github.com/earendil-works/pi/blob/v0.82.0/packages/ai/src/types.ts>
4. pi coding-agent extension types at v0.82.0 (ExtensionAPI, events, ExtensionUIContext, ToolDefinition, runtime seams): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/types.ts>
5. pi coding-agent extension loader at v0.82.0 (signatures, `pi.exec` wiring, `createExtensionRuntime`): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/extensions/loader.ts>
6. pi coding-agent session manager at v0.82.0 (version 3, migrations, entry types, file/dir naming, context reconstruction): <https://github.com/earendil-works/pi/blob/v0.82.0/packages/coding-agent/src/core/session-manager.ts>
