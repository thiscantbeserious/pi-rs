# Themes use pi's native JSON format with a tree-sitter capture mapping

pi-rs loads pi theme files unchanged (same $schema, same directories), so existing curated themes work on day one and theme configs stay shared between pi and pi-rs during dogfooding. Tree-sitter highlighting (ADR 0010) speaks [capture names](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html), a richer vocabulary than [pi's theme schema](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/src/modes/interactive/theme/theme-schema.json), so a fixed internal table maps captures onto the pi palette (@keyword → keyword, @string → string, …) with a plain-text fallback for unmapped captures. Live theme switching restyles the entire retained model (ADR 0004).

## Considered Options

- Helix/tree-sitter-native theme format - rejected for v1: richer highlighting ceiling but forces converting existing themes and diverges configs during dogfooding
- Supporting both formats - rejected: two loaders and two mapping paths for a v1 whose bar is parity

## Consequences

- Highlighting granularity is capped by pi's syntax palette until an extended-keys superset (capture-level overrides, ignored by pi) is added post-parity
- The capture mapping table is a single place to tune when highlights look wrong

## Sources

- tree-sitter syntax highlighting and capture names: https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html
- pi theme schema: https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/src/modes/interactive/theme/theme-schema.json
