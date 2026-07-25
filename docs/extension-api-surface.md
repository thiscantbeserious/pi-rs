# Extension API surface (Host Protocol coverage checklist)

Extracted from the pinned Oracle (pi `0.82.0`, ADR 0007) installed type declarations at `node_modules/@earendil-works/pi-coding-agent/dist/core/extensions/{types,runner,loader}.d.ts`, verified present locally. Every entry below is part of the extension-facing contract: either a call an extension makes (host must service it over the Host Protocol, or own it as host-local state), a registration (extension contributes host state), or an event the Core emits to extensions. The Phase 0 spike checks each one loads under Deno; Phase 3 wires each one end-to-end.

Direction key: `E→C` extension calls Core (Host Protocol request), `C→E` Core notifies extension (Host Protocol event), `local` host-owned state the extension mutates in-process, `reg` extension contributes a registration the host stores and the Core reads back.

## A. Factory and lifecycle (loader)

Source: `loader.d.ts`. Loader uses `jiti` to import TypeScript extension modules. Discovery walks configured paths plus the standard `~/.pi` tree.

- [ ] `loadExtensions(paths, cwd, eventBus?, runtime?)` loads a list of extension modules
- [ ] `loadExtensionsCached(...)` memoizes by path
- [ ] `discoverAndLoadExtensions(configuredPaths, cwd, agentDir?, eventBus?)` walks standard locations
- [ ] `loadExtensionFromFactory(factory, cwd, eventBus, runtime, extensionPath?)` for inline extensions
- [ ] `createExtensionRuntime()` builds the runtime with throwing action stubs (runner binds real actions later)
- [ ] `clearExtensionCache()` invalidates the jiti module cache (serves `/reload`, ADR 0017)
- [ ] Factory signature: `ExtensionFactory = (pi: ExtensionAPI) => void | Promise<void>` (sync or async init)
- [ ] Inline extension form: `{ name, factory, hidden? }`

## B. ExtensionAPI (`pi.*`) call surface

Source: `ExtensionAPI` in `types.d.ts`. Each `pi.*` method is a Host Protocol call the Core must service, except registrations which are host-local state the Core later reads.

### Messaging and session persistence

- [ ] `sendMessage(message, options?)` custom message into the session (`triggerTurn?`, `deliverAs: steer|followUp|nextTurn`)
- [ ] `sendUserMessage(content, options?)` user message, always triggers a turn (`deliverAs: steer|followUp`)
- [ ] `appendEntry(customType, data?)` custom session entry for state persistence, not sent to LLM (ADR 0016: routes to Core as sole writer)
- [ ] `setSessionName(name)` / `getSessionName()` session display name
- [ ] `setLabel(entryId, label?)` bookmark label on an entry
- [ ] `exec(command, args, options?)` execute a shell command, returns `ExecResult` — `local` (host-owned exec, delegates to `execCommand`, not a Host Protocol message; ADR 0021's binding layer must preserve it)

### Tools and commands

- [ ] `registerTool(tool)` LLM-callable tool (see section G for the definition shape)
- [ ] `registerCommand(name, options)` slash command (`handler`, `description?`, `getArgumentCompletions?`)
- [ ] `getCommands()` available slash commands
- [ ] `getActiveTools()` / `setActiveTools(toolNames)` / `getAllTools()` tool set management
- [ ] `registerShortcut(shortcut, options)` keybinding (`handler(ctx)`, `description?`)
- [ ] `registerFlag(name, options)` CLI flag (`type: boolean|string`, `default?`, `description?`)
- [ ] `getFlag(name)` read a registered flag value

### Renderers

- [ ] `registerMessageRenderer(customType, renderer)` custom `CustomMessageEntry` renderer
- [ ] `registerEntryRenderer(customType, renderer)` custom `CustomEntry` renderer (not in LLM context)

### Model and thinking

- [ ] `setModel(model)` returns false if no API key (Host Proxy path, ADR 0005)
- [ ] `getThinkingLevel()` / `setThinkingLevel(level)` clamped to model caps

### Providers (ADR 0019)

- [ ] `registerProvider(provider: Provider)` native pi-ai provider registration
- [ ] `registerProvider(name, config: ProviderConfig)` legacy config form (`baseUrl`, `apiKey`, `api`, `models?`, `headers?`, `authHeader?`, `streamSimple?`, `refreshModels?`, `oauth?`)
- [ ] `unregisterProvider(name)` removes provider models, restores overridden built-ins
- [ ] `events` shared `EventBus` for extension-to-extension communication

## C. Events (`pi.on`) — Core to extension

Source: `ExtensionAPI.on` overloads in `types.d.ts`. Each is a `C→E` notification the Core must deliver. Handlers may return results that mutate behavior (noted where applicable).

### Session lifecycle

- [ ] `project_trust` (handler is `ProjectTrustHandler`, returns `trusted: yes|no|undecided`, `remember?`)
- [ ] `resources_discover` returns `{ skillPaths?, promptPaths?, themePaths? }`
- [ ] `session_start` (`reason: startup|reload|new|resume|fork`, `previousSessionFile?`)
- [ ] `session_info_changed` (`name?`)
- [ ] `session_before_switch` result `{ cancel? }` (`reason: new|resume`, `targetSessionFile?`)
- [ ] `session_before_fork` result `{ cancel?, skipConversationRestore? }`
- [ ] `session_before_compact` result `{ cancel?, compaction? }` (`reason: manual|threshold|overflow`, `willRetry`, `signal`)
- [ ] `session_compact` (`compactionEntry`, `fromExtension`, `reason`, `willRetry`)
- [ ] `session_shutdown` (`reason: quit|reload|new|resume|fork`, `targetSessionFile?`)
- [ ] `session_before_tree` result `{ cancel?, summary?, customInstructions?, replaceInstructions?, label? }`
- [ ] `session_tree` (`newLeafId`, `oldLeafId`, `summaryEntry?`, `fromExtension?`)

### Agent loop and provider

- [ ] `context` result `{ messages? }` fired before each LLM call, can modify messages
- [ ] `before_provider_request` (`payload`) can replace the payload
- [ ] `before_provider_headers` mutate `headers` in place (null deletes), ADR 0019 header injection
- [ ] `after_provider_response` (`status`, `headers`)
- [ ] `before_agent_start` result `{ message?, systemPrompt? }` (`prompt`, `images?`, `systemPrompt`, `systemPromptOptions`)
- [ ] `agent_start` / `agent_end` (`messages`) / `agent_settled`
- [ ] `turn_start` (`turnIndex`, `timestamp`) / `turn_end` (`turnIndex`, `message`, `toolResults`)
- [ ] `message_start` / `message_update` (`assistantMessageEvent`) / `message_end` result `{ message? }`

### Tool execution

- [ ] `tool_execution_start` / `tool_execution_update` (`partialResult`) / `tool_execution_end` (`result`, `isError`)
- [ ] `tool_call` result `{ block?, reason? }`, `event.input` mutable in place (hooks, ADR 0009 fail-closed)
- [ ] `tool_result` result `{ content?, details?, isError?, usage? }`

### Input and model selection

- [ ] `model_select` (`model`, `previousModel`, `source: set|cycle|restore`)
- [ ] `thinking_level_select` (`level`, `previousLevel`)
- [ ] `user_bash` result `{ operations?, result? }` (`command`, `excludeFromContext`, `cwd`)
- [ ] `input` result `continue|transform|handled` (`text`, `images?`, `source: interactive|rpc|extension`, `streamingBehavior?`)

## D. ExtensionContext (`ctx.*`) — event handler context

Source: `ExtensionContext` in `types.d.ts`. Provided to every event handler and tool `execute`.

- [ ] `ctx.ui` the `ExtensionUIContext` (section E)
- [ ] `ctx.mode` `tui|rpc|json|print`
- [ ] `ctx.hasUI` dialog-capable UI available (true in TUI and RPC)
- [ ] `ctx.cwd` current working directory
- [ ] `ctx.sessionManager` `ReadonlySessionManager`
- [ ] `ctx.modelRegistry` `ModelRegistry` for API key resolution
- [ ] `ctx.model` current `Model<any> | undefined`
- [ ] `ctx.thinkingLevel?` current thinking level
- [ ] `ctx.isIdle()` / `ctx.isProjectTrusted()` / `ctx.hasPendingMessages()`
- [ ] `ctx.signal` current `AbortSignal | undefined`
- [ ] `ctx.abort()` abort the current agent operation
- [ ] `ctx.shutdown()` graceful shutdown
- [ ] `ctx.getContextUsage()` returns `{ tokens, contextWindow, percent } | undefined`
- [ ] `ctx.compact(options?)` trigger compaction without awaiting (`onComplete?`, `onError?`)
- [ ] `ctx.getSystemPrompt()` current effective system prompt

## E. ExtensionUIContext (`ctx.ui.*`) — UI primitives (ADR 0003)

Source: `ExtensionUIContext` in `types.d.ts`. Frame buffers and focus routing. Interactive mode only unless noted.

- [ ] `select(title, options, opts?)` / `confirm(title, message, opts?)` / `input(title, placeholder?, opts?)` dialogs (`signal?`, `timeout?`)
- [ ] `notify(message, type?)` notification (`info|warning|error`)
- [ ] `onTerminalInput(handler)` raw terminal input (interactive only), returns unsubscribe
- [ ] `setStatus(key, text|undefined)` footer status bar slot
- [ ] `setWorkingMessage(message?)` / `setWorkingVisible(visible)` / `setWorkingIndicator(options?)` streaming loader
- [ ] `setHiddenThinkingLabel(label?)` hidden thinking block label
- [ ] `setWidget(key, content, options?)` widget above/below editor (`placement: aboveEditor|belowEditor`), string array or component factory
- [ ] `setFooter(factory|undefined)` / `setHeader(factory|undefined)` custom footer/header components
- [ ] `setTitle(title)` terminal window title
- [ ] `custom<T>(factory, options?)` focused custom component (`overlay?`, `overlayOptions?`, `onHandle?`)
- [ ] `pasteToEditor(text)` / `setEditorText(text)` / `getEditorText()` / `editor(title, prefill?)` editor interaction
- [ ] `addAutocompleteProvider(factory)` stack autocomplete behavior
- [ ] `setEditorComponent(factory|undefined)` / `getEditorComponent()` custom editor component
- [ ] `ctx.ui.theme` (readonly) / `getAllThemes()` / `getTheme(name)` / `setTheme(name|Theme)` theme access (ADR 0012)
- [ ] `getToolsExpanded()` / `setToolsExpanded(expanded)` tool output expansion state

## F. ExtensionCommandContext — command handler session control

Source: `ExtensionCommandContext` and `ReplacedSessionContext` in `types.d.ts`. Only safe in user-initiated commands.

- [ ] `getSystemPromptOptions()` base system-prompt construction options
- [ ] `waitForIdle()` wait for streaming to finish
- [ ] `newSession(options?)` (`parentSession?`, `setup?`, `withSession?`) returns `{ cancelled }`
- [ ] `fork(entryId, options?)` (`position: before|at`, `withSession?`)
- [ ] `navigateTree(targetId, options?)` (`summarize?`, `customInstructions?`, `replaceInstructions?`, `label?`)
- [ ] `switchSession(sessionPath, options?)` (`withSession?`)
- [ ] `reload()` reload extensions, skills, prompts, themes, context files (ADR 0017: host restart)
- [ ] `ReplacedSessionContext.sendMessage(message, options?)` / `sendUserMessage(content, options?)` bound to the replacement session

## G. Tool definition surface (ADR 0015 parity)

Source: `ToolDefinition` in `types.d.ts`. Built-in tools are Rust-native in the Core; the host must accept the same definition shape from extension-registered tools.

- [ ] `name`, `label`, `description`, `parameters` (TypeBox `TSchema`)
- [ ] `promptSnippet?`, `promptGuidelines?` system-prompt contributions
- [ ] `constrainedSampling?: false | ConstrainedSamplingConfig` provider-side constrained sampling
- [ ] `renderShell?: default|self`
- [ ] `prepareArguments?(args)` compatibility shim before schema validation
- [ ] `executionMode?: sequential|parallel`
- [ ] `execute(toolCallId, params, signal, onUpdate, ctx)` returns `AgentToolResult`
- [ ] `renderCall?(args, theme, context)` / `renderResult?(result, options, theme, context)` custom TUI rendering
- [ ] `defineTool(tool)` preserves parameter inference for standalone definitions

## H. Runtime and actions (host implementation input)

Source: `ExtensionRuntime`, `ExtensionActions`, `ExtensionContextActions`, `ExtensionCommandContextActions` in `types.d.ts`. These are the seams the Phase 0 host-impl strategy (vendor vs clean-room) must satisfy: the loader creates a runtime with throwing stubs, the runner binds real actions. A vendored pi runtime satisfies them for free; a clean-room shim must implement every action.

- [ ] `ExtensionActions`: `sendMessage`, `sendUserMessage`, `appendEntry`, `setSessionName`, `getSessionName`, `setLabel`, `getActiveTools`, `getAllTools`, `setActiveTools`, `refreshTools`, `getCommands`, `setModel`, `getThinkingLevel`, `setThinkingLevel`
- [ ] `ExtensionContextActions`: `getModel`, `isIdle`, `isProjectTrusted`, `getSignal`, `abort`, `hasPendingMessages`, `shutdown`, `getContextUsage`, `compact`, `getSystemPrompt`, `getSystemPromptOptions?`
- [ ] `ExtensionCommandContextActions`: `waitForIdle`, `newSession`, `fork`, `navigateTree`, `switchSession`, `reload`
- [ ] `ExtensionRuntimeState`: `flagValues`, pending provider registration queues, `assertActive`, `invalidate`, `registerProvider`, `registerNativeProvider`, `unregisterProvider`
- [ ] `ExtensionRunner.bindCore(...)` / `bindCommandContext(...)` / `setUIContext(...)` binding seams
- [ ] `ExtensionRunner` emit methods per event with combined-result semantics (e.g. `emitBeforeAgentStart` chains system-prompt overrides)

## Out of scope for this checklist

- Built-in tool internals (`createBashTool`, `createReadTool`, etc.) are Core-native per ADR 0015, not host protocol messages.
- Provider streaming internals (`streamSimple`, OAuth callbacks) are host-side concerns surfaced via `registerProvider`; the Host Protocol carries only the registration and the resulting provider/model metadata.
- TUI component types (`Component`, `EditorComponent`, `OverlayOptions`) are render-thread concerns in the Core, exchanged as serialized frame buffers over the protocol per ADR 0003, not as live objects.
