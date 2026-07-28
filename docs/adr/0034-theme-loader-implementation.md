# Status: ACCEPTED. Theme loader implementation: typed struct, var resolution at load, ratatui Color mapping

ADR 0012 decided the architecture (pi's native JSON format, tree-sitter capture mapping, live theme switch). This ADR specifies the implementation details ADR 0012 left open: the ColorValue type, var resolution timing, ratatui Color bridging, and the Theme struct shape.

## Context

Researched against pi v0.82.0 (commit `083e6162`):

- **pi's ColorValue** is a union of `string` (hex `#RRGGBB`, var reference, or empty for terminal default) and `integer` (0-255, 256-color palette index). Defined in `theme-schema.json` `$defs/colorValue` ([source](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme-schema.json)).
- **pi resolves vars at load time**: `resolveThemeColors()` calls `resolveVarRefs()` with cycle detection (`theme.ts` [L293-L318](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L293-L318)). The resolved colors are stored and used at render time.
- **pi renders colors as ANSI escape sequences**: `fgAnsi()` / `bgAnsi()` produce `\x1b[38;5;Nm` or `\x1b[38;2;R;G;Bm` (theme.ts [L260-L280](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L260-L280)).
- **pi's theme schema** has ~50 required color keys: 9 `syntax*` keys, ~30 UI keys (accent, border, mdHeading, etc.), and ~7 thinking level keys. `additionalProperties: false` on colors but `additionalProperties` not set on the root (unknown root keys tolerated).
- **research.md D-F** says "var references preserved unresolved in the struct, resolved at render time." This contradicts pi, which resolves at load. This ADR corrects research.md.

## Decision

### ColorValue enum with 3 variants (vars resolved at load)

```rust
pub enum ColorValue {
    Hex(u8, u8, u8),    // #RRGGBB -> (r, g, b)
    Indexed(u8),        // 0-255
    Reset,              // empty string -> terminal default
}
```

Vars are resolved at load time (like pi's `resolveThemeColors`). The `Theme` struct stores only resolved `ColorValue` (no `Var` variant at runtime). Cycle detection at load time (visited set, like pi's `resolveVarRefs`). This is simpler at render time (no var map lookup per cell) and matches pi's behavior.

research.md D-F is updated: "var references resolved at load time (not render time)."

### Full typed Theme struct

```rust
pub struct Theme {
    pub name: String,
    pub colors: ThemeColors,
}

pub struct ThemeColors {
    pub accent: ColorValue,
    pub border: ColorValue,
    // ... all ~50 keys from the schema
    pub syntax_comment: ColorValue,
    pub syntax_keyword: ColorValue,
    // ... 9 syntax* keys
}
```

Unknown keys in the JSON are tolerated (skipped, ADR 0020). Missing required keys use `ColorValue::Reset` as default. This is parse-don't-validate (PHILOSOPHY section 4): the type encodes validity.

### ratatui Color mapping

`ColorValue` maps to `ratatui::style::Color`:

- `Hex(r, g, b)` -> `Color::Rgb(r, g, b)`
- `Indexed(n)` -> `Color::Indexed(n)`
- `Reset` -> `Color::Reset`

This bridges pi's color model to ratatui's `Color` enum without ANSI escape sequences.

### Capture-to-palette match expression

Step 5's hardcoded `capture_to_style` is replaced with a theme-driven mapping. tree-sitter's highlight names map to pi's 9 `syntax*` keys:

| tree-sitter capture | pi syntax* key |
| --- | --- |
| `comment` | `syntaxComment` |
| `keyword` | `syntaxKeyword` |
| `function` | `function` |
| `string` | `syntaxString` |
| `number` | `syntaxNumber` |
| `type` | `syntaxType` |
| `operator` | `syntaxOperator` |
| `punctuation` | `syntaxPunctuation` |
| `variable` | `syntaxVariable` |
| `property` | `syntaxVariable` (fallback) |
| `attribute` | `syntaxKeyword` (fallback) |
| unmapped | `Reset` (plain text) |

The mapping is a `match` expression (ADR 0012: "single place to tune").

### Live theme switch

`RenderEvent::ThemeChanged` flushes the block cache (ADR 0010) and re-projects. The `Theme` is stored in the RMM (or passed to `render_markdown`). On theme change, the block cache is invalidated and all messages are re-rendered with the new theme.

### Snapshot testing

Three snapshot test layers (using insta, already a dev-dep):

1. **Resolved Theme struct**: snapshot the `Theme` struct (after var resolution) for each theme file. Catches regressions in var resolution, missing keys, wrong color values. If a theme file or the loader changes, the snapshot diff shows exactly what moved.
2. **Rendered output per theme**: snapshot a `TestBackend` buffer rendering a sample markdown with code highlighting for each theme. Catches visual regressions in the capture-to-palette mapping (syntaxComment mapped to wrong color, etc.). The snapshot shows cell content + styles as text.
3. **Capture mapping**: snapshot the capture-to-palette `match` expression's output for each tree-sitter capture name. Catches regressions when the mapping table changes.

Direct pi vs pi-rs cross-comparison was rejected: pi produces ANSI escape sequences, pi-rs produces ratatui `Color` enum values. They are different representations of the same intent. A golden-file cross-compare (capture pi's resolved colors once, compare against pi-rs) is possible but adds complexity for marginal gain. The three snapshot layers above catch regressions within pi-rs without needing to run pi.

## Considered options

- **Keep vars unresolved (research.md original)**: rejected. pi resolves at load. Resolving at render time requires the vars map at every cell write. Simpler to resolve once at load.
- **HashMap<String, ColorValue>**: rejected by research.md D-F. Not parse-don't-validate. Typos in key names not caught.
- **Keep ANSI escape sequences**: rejected. ratatui's Buffer works with `Color` enum, not raw ANSI.
- **3-variant ColorValue (no Var at runtime)**: chosen. Vars resolved at load. Simpler than 4-variant (no Var resolution at render time).

## Consequences

- **`pi-render` gains a `theme.rs` module** with `ColorValue`, `Theme`, `ThemeColors`, `load_theme()`, and the capture-to-palette `match`.
- **`pi-render` gains `serde` + `serde_json` dependencies** for JSON parsing. (serde_json already a dependency.)
- **`capture_to_style` in markdown.rs is replaced** with a theme-driven mapping that takes a `&Theme`.
- **`render_markdown` takes a `&Theme`** parameter (or the block cache holds a theme reference).
- **research.md D-F is updated**: var resolution at load time, not render time.
- **Unknown keys tolerated**: the JSON deserializer skips unknown fields (serde `deny_unknown_fields` is NOT used; ADR 0020).
- **Missing required keys default to Reset**: the theme loads even if some keys are missing (forward-compat with future pi versions that add keys).

## Sources

1. pi theme schema (ColorValue, ~50 keys, additionalProperties): `packages/coding-agent/src/modes/interactive/theme/theme-schema.json` at v0.82.0
2. pi `resolveVarRefs` (var resolution with cycle detection): `packages/coding-agent/src/modes/interactive/theme/theme.ts` [L293-L318](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L293-L318)
3. pi `resolveThemeColors` (resolve at load): `packages/coding-agent/src/modes/interactive/theme/theme.ts` [L310-L322](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L310-L322)
4. pi `fgAnsi` / `bgAnsi` (ANSI escape sequences): `packages/coding-agent/src/modes/interactive/theme/theme.ts` [L260-L280](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/coding-agent/src/modes/interactive/theme/theme.ts#L260-L280)
5. ADR 0012 (pi native JSON format, capture mapping): `docs/adr/0012-native-pi-themes-capture-mapping.md`
6. ADR 0020 (shared ~/.pi tree, tolerate unknown keys): `docs/adr/0020-pi-rs-binary-shared-pi-tree.md`
7. research.md D-F (theme loading decision): `docs/research.md`
