# Streaming markdown pipeline: pulldown-cmark structure, tree-sitter highlighting

The streaming render path coalesces tokens to frames (≥16ms), fully reparses the in-flight message with [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) (CommonMark-compliant, used by rustdoc and mdBook. The ~µs-at-chat-sizes cost is an assumption to verify in Phase 2 benchmarks), and highlights code blocks with [tree-sitter](https://github.com/tree-sitter/tree-sitter) (designed to be "robust enough to provide useful results even in the presence of syntax errors" with incremental re-parsing): finalized blocks are cached as styled lines, only the growing tail block takes an incremental tree edit and re-highlight. Finalized messages are fully cached in the retained model and re-render only on theme change or resize. Each layer uses the tool that is most robust for it: [tree-sitter-markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown) was rejected for structure (upstream itself states it is "not recommended for correctness-critical use"), and regex-based highlighting was rejected for code (state mis-locks on the incomplete code that streaming produces by definition - the visible restyle jank in existing agent TUIs).

## Considered Options

- Tree-sitter for everything (helix/zed style) - rejected for the markdown layer only: two-stage block/inline grammar with edge-case bugs. Incrementality buys nothing at chat sizes
- [syntect](https://github.com/trishume/syntect) (regex-based Sublime Text syntax definitions) for highlighting - rejected: not error-tolerant on unterminated constructs mid-stream. Kept as a possible fallback for languages without tree-sitter grammars
- Highlight only when finalized - rejected: visible restyle pop and unstyled streaming code, a visual regression vs pi

## Consequences

- Grammar bundling per language: binary size, build complexity, grammar version maintenance are owned costs
- Languages without a bundled grammar fall back to plain styling (or syntect if adopted later)
- The retained model + tree-sitter combination leaves editor-grade features (semantic selection in history, folding) open for post-parity work
