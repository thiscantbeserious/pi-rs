# Status: ACCEPTED. Markdown pipeline implementation: block-granular cache, partial fences, grammar set

ADR 0010 decided the architecture (pulldown-cmark for structure, tree-sitter-highlight for code, block-granular caching). This ADR specifies the implementation details ADR 0010 left open: where the block cache lives, how partial closing fences are handled, the grammar set, and the "incremental" re-highlight semantics.

## Context

Researched against pi v0.82.0 (commit `083e6162`), pulldown-cmark, and tree-sitter-highlight:

- **pi uses `highlight.js` (regex-based) for code highlighting**, not tree-sitter. `highlightCode()` in `packages/coding-agent/src/modes/interactive/theme/theme.ts` [L1138](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L1138) calls `hljs.highlight()` from `highlight.js`. ADR 0010 explicitly rejected regex-based highlighting ("state mis-locks on the incomplete code that streaming produces"). pi-rs uses tree-sitter-highlight (error-tolerant). This is a documented improvement, not a parity gap.
- **pi uses `marked` (JS markdown parser)**, pi-rs uses `pulldown-cmark`. ADR 0010 chose pulldown-cmark (CommonMark-compliant, used by rustdoc/mdBook).
- **pi has `trimPartialClosingFences`** (`packages/tui/src/components/markdown.ts` [L37-L55](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L37-L55)): trims streamed partial closing fences so code blocks don't shrink/flicker when the final fence character arrives.
- **tree-sitter-highlight's `highlight()`** takes `&[u8]` source and re-parses internally. It does NOT expose the incremental `old_tree` API of `tree_sitter::Parser`. "Incremental" in ADR 0010 means "only the tail block is re-highlighted, not all blocks" (block-granular caching), not "incremental tree edit within a block."
- **pi's Markdown caching** is per-component (`cachedText`/`cachedWidth`/`cachedLines`, invalidated on text or width change, markdown.ts [L152-L157](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L152-L157)). Step 3 has message-granular caching (ADR 0031). Step 5 formalizes as block-granular.

## Decision

### Block-granular cache in the pipeline module

The markdown pipeline module (new `markdown.rs`) owns a block cache: `HashMap<(code_content, lang, width, theme), Vec<RenderedLine>>`. When `render_message` encounters a code block, it checks the cache. Finalized blocks return cached highlighted lines. The tail block (the last code block in the streaming message, which may be incomplete) is re-highlighted each frame. The cache is invalidated on width or theme change (like pi's `cachedWidth`).

`RenderState` still caches rendered lines per message (ADR 0031), but now delegates code-block highlighting to the markdown pipeline's block cache. The message-granular cache wraps the block-granular cache: a message's rendered lines are cached, and within that, each code block's highlighted lines are cached.

### Partial closing fences

During streaming, a code block's closing fence (` ``` `) arrives character by character. Without trimming, the block flickers (the partial fence is parsed as code content, then re-parsed as a fence when complete). The pipeline trims partial closing fences before parsing, like pi's `trimPartialClosingFences` (markdown.ts L37-L55). This is a pre-processing step on the raw text before pulldown-cmark parses it.

### Grammar set

Minimal daily-driver set (research.md D-B): rust, TypeScript/JavaScript, bash, json. These cover the most common code blocks in an agent TUI. Languages without a bundled grammar fall back to plain styling (ADR 0010). The grammar set is behind Cargo features so it expands without code changes. The Oracle's full grammar set is a Phase 4 parity surface (open sub-item in research.md).

### Re-highlight semantics

"Incremental" (ADR 0010) means: only the tail block is re-highlighted each frame, not all blocks. Finalized blocks return cached highlighted lines. tree-sitter-highlight's `highlight()` re-parses the tail block's source each frame (it does not expose incremental `old_tree`). This is acceptable because tree-sitter is error-tolerant (ADR 0010) and the tail block is small (the growing end of one code block, not the whole message).

### P18 fallback

If tree-sitter error recovery is catastrophically bad for a grammar on mid-typing edits (P18: tree-sitter#2404), the tail block falls back to plain styling for that grammar. This is detected by a per-grammar incomplete-code test (the P18 corpus). A grammar with bad recovery is flagged in the test, and the fallback is automatic.

## Considered options

- **Switch to syntect/highlight.js (pi parity)**: rejected (ADR 0010 already rejected regex-based highlighting). Not error-tolerant on incomplete code (P18).
- **Use `tree_sitter::Parser` directly for incremental parsing**: rejected. Re-implements tree-sitter-highlight's logic. YAGNI (PHILOSOPHY section 5). tree-sitter-highlight re-parsing the tail block is fast enough.
- **Keep message-granular cache (no block-granular)**: rejected. P11 (streaming re-render cost grows with message size) is not guarded. Message-granular re-renders all blocks of a message, not just the tail.
- **Full Oracle grammar set**: rejected for Step 5. Large binary size for Phase 2. research.md says "measure binary/build size; fire the WASM fallback trigger only if measured bloat." Start minimal, expand in Phase 4.
- **Don't handle partial fences**: rejected. Code blocks would flicker during streaming. ADR 0010 says "visible restyle pop" is rejected. Flicker is worse than pop.

## Consequences

- **`pi-render` gains a `markdown.rs` module** with the block cache, the pulldown-cmark parsing, the tree-sitter-highlight integration, and the partial-fence trimming.
- **`pi-render` gains dependencies**: `pulldown-cmark`, `tree-sitter-highlight`, and the grammar crates (`tree-sitter-rust`, `tree-sitter-javascript`, `tree-sitter-bash`, `tree-sitter-json`). These are workspace-unified (P16).
- **The block cache is separate from `RenderState`'s message cache.** `RenderState` caches rendered lines per message. The markdown pipeline caches highlighted blocks. Both are invalidated on width/theme change.
- **Languages without a bundled grammar fall back to plain styling.** No error, no panic, just unstyled code (ADR 0010).
- **The P18 corpus tests per-grammar incomplete code.** A grammar with bad error recovery falls back to plain styling for the tail block.
- **pi uses highlight.js (regex), pi-rs uses tree-sitter (error-tolerant).** Documented divergence per ADR 0010. The plan's pi citation notes this.

## Sources

1. pi `highlightCode()` using `highlight.js` (regex-based): `packages/coding-agent/src/modes/interactive/theme/theme.ts` [L1138](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L1138)
2. pi `trimPartialClosingFences` (partial fence trimming): `packages/tui/src/components/markdown.ts` [L37-L55](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L37-L55)
3. pi `Markdown.cachedLines` (per-component caching): `packages/tui/src/components/markdown.ts` [L152-L157](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L152-L157)
4. pi `Markdown.render()` (full re-parse per render): `packages/tui/src/components/markdown.ts` [L151](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/components/markdown.ts#L151)
5. pulldown-cmark `Parser` (pull parser, Event iterator): <https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.Parser.html>
6. pulldown-cmark `Event` (Start/End/Text/Code): <https://docs.rs/pulldown-cmark/latest/pulldown_cmark/enum.Event.html>
7. tree-sitter-highlight `Highlighter::highlight` (takes `&[u8]`, re-parses internally): <https://docs.rs/tree-sitter-highlight/latest/tree_sitter_highlight/struct.Highlighter.html>
8. tree-sitter-highlight `HighlightConfiguration` (language + query): <https://docs.rs/tree-sitter-highlight/latest/tree_sitter_highlight/struct.HighlightConfiguration.html>
9. ADR 0010 (streaming markdown pipeline, the architecture decision): `docs/adr/0010-streaming-markdown-pipeline.md`
10. ADR 0031 (RMM viewport model, message-granular cache seed): `docs/adr/0031-rmm-viewport-model.md`
11. P11 (streaming markdown re-render cost grows with message size): `docs/pitfalls.md`
12. P18 (tree-sitter error recovery varies by grammar): `docs/pitfalls.md`
