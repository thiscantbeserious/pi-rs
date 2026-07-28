# Status: ACCEPTED. Grapheme-width engine implementation: direct cell writing with ForcedWidth

ADR 0025 decided that width is computed at the projection layer (not delegated to ratatui's `CellWidth`), behind a `GraphemeWidth` trait, with `runefix-core` as the chosen crate (research.md). This ADR specifies HOW the projection feeds pre-widthed graphemes into ratatui's `Buffer`, resolving the integration gap ADR 0025 left open ("feed pre-widthed spans into ratatui" without specifying the mechanism).

## Context

Researched against ratatui-core 0.1.2 source and runefix-core 0.1.10 docs:

- **ratatui's `Buffer::set_string`** (buffer.rs [L324-L368](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/buffer.rs#L324-L368)) calls `cell_width()` (the `CellWidth` trait, `unicode-width`-based) to filter and compute widths. It **skips zero-width graphemes** and splits ZWJ families into multiple cells with wrong widths. It cannot use runefix-core's grapheme-aware width.
- **ratatui's `Cell` has `set_diff_option(CellDiffOption::ForcedWidth(NonZero<u16>))`** (cell.rs [L241](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/cell.rs#L241)) which forces the width for diffing. `cell_width()` returns the forced width when set (cell.rs [L309-L316](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/cell.rs#L309-L316)). This is the bypass.
- **runefix-core** provides `grapheme_widths(s)` returning `Vec<(&str, usize)>` (grapheme + width), `split_by_width(s, width)` wrapping by display width, and `RuneDisplayWidth` trait. Default policy is terminal-style (emoji=2, CJK=2), matching pi's `graphemeWidth()` (utils.ts [L167](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/utils.ts#L167)).
- **pi has a grapheme-aware width engine**: `graphemeWidth()` in `packages/tui/src/utils.ts` [L167](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/utils.ts#L167), using `Intl.Segmenter` (grapheme segmentation) + custom emoji/CJK/zero-width width (emoji=2, CJK=2, zero-width=0). The plan's "no pi equivalent" claim was a §9.5 citation error.

## Decision

### Direct cell writing via get_mut + ForcedWidth

The projection bypasses `set_string`/`set_line`/`set_span` entirely. For each grapheme cluster (segmented by runefix-core's `grapheme_widths`), the projection writes the cell directly:

1. `buf.get_mut(x, y)` to get the cell.
2. `.set_symbol(grapheme)` to set the grapheme text.
3. `.set_style(style)` to set the style.
4. `.set_diff_option(CellDiffOption::ForcedWidth(width))` to force the width for diffing.
5. For width-2 graphemes, reset the trailing cell at `(x+1, y)` (replicating `set_string`'s multi-width logic at buffer.rs L359-L362).

This is the only way to use runefix-core's grapheme-aware width with ratatui: `set_string` re-computes width via `unicode-width` and skips zero-width graphemes, which breaks ZWJ families and combining marks.

### GraphemeWidth trait

```rust
pub trait GraphemeWidth: Send {
    fn grapheme_widths(&self, text: &str) -> Vec<(&str, u16)>;
}
```

The `runefix-core` impl calls `runefix_core::grapheme_widths(text)` and maps `usize` to `u16`. The trait is the swap seam (ADR 0025): a future crate swap is one impl.

### Wrapping via split_by_width

The ASCII-scoped `wrap_text` (char count) is replaced by runefix-core's `split_by_width(text, width)`, which wraps by display width preserving grapheme boundaries. Each wrapped line becomes a `RenderedLine`.

### Mode 2027 deferred to Step 8

Mode 2027 (grapheme clustering) is a terminal capability query that needs the real crossterm event stream (`InputSource`). Step 4 uses runefix-core's default terminal policy without querying mode 2027. The query lands in Step 8 (CI tmux/screen lanes) where the real terminal wiring exists.

## Considered options

- **Use `set_string` per grapheme**: rejected. `set_string` calls `cell_width()` on each grapheme, which uses `unicode-width`, not runefix-core. A ZWJ family emoji (runefix says width=2) would be split by `unicode-width` into multiple cells with wrong widths.
- **Use `set_string` then set `ForcedWidth` after**: rejected. `set_string` already wrote the wrong cells (skipped zero-width, split ZWJ). Setting `ForcedWidth` after doesn't fix the cell content.
- **Fork ratatui's `CellWidth`**: rejected (ADR 0025 already rejected this). Couples to a fork, fights upstream, complicates P16 version unification.
- **Keep `wrap_text` (char-based) for ASCII, add grapheme path separately**: rejected. Conditional complexity. The plan says Step 4 replaces the ASCII-scoped wrapping.

## Consequences

- **The projection writes cells directly**, not via `set_string`. This is more code but is the only correct path. The `RmmProjection::render` method grows to handle the cell-writing loop.
- **`CellDiffOption::ForcedWidth` is set on every multi-width cell.** This tells `Buffer::diff` the correct width for multi-width cell skipping. Without it, `Buffer::diff` would use `unicode-width` (the P13 failure class).
- **`wrap_text` is removed.** `split_by_width` replaces it. The `render_message` and `render_content_block` functions call `split_by_width` instead.
- **Mode 2027 is not queried in Step 4.** runefix-core's default terminal policy applies. This is documented as a deferral to Step 8, not a gap.
- **The P13 corpus tests use `TestBackend` buffer inspection.** Each corpus class asserts the expected width via `grapheme_widths()` AND asserts the projection writes the correct number of cells (via `TestBackend` buffer cell inspection).

## Sources

1. ratatui `Buffer::set_string` (skips zero-width, uses `cell_width`): `ratatui-core/src/buffer/buffer.rs` [L324-L368](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/buffer.rs#L324-L368)
2. ratatui `Cell::set_diff_option` + `CellDiffOption::ForcedWidth`: `ratatui-core/src/buffer/cell.rs` [L241](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/cell.rs#L241)
3. ratatui `CellWidth for Cell` (returns ForcedWidth when set): `ratatui-core/src/buffer/cell.rs` [L309-L316](https://github.com/ratatui/ratatui-core/blob/0.1.2/src/buffer/cell.rs#L309-L316)
4. runefix-core `grapheme_widths` API: <https://docs.rs/runefix-core/latest/runefix_core/fn.grapheme_widths.html>
5. runefix-core `split_by_width` API: <https://docs.rs/runefix-core/latest/runefix_core/fn.split_by_width.html>
6. pi `graphemeWidth()` (the pi equivalent, grapheme-aware width): `packages/tui/src/utils.ts` [L167](https://github.com/earendil-works/pi/blob/083e61621276bff9f6faefab87ce07fcd98734e2/packages/tui/src/utils.ts#L167)
7. ADR 0025 (projection-layer grapheme-cluster width, the architecture decision): `docs/adr/0025-projection-layer-grapheme-cluster-width.md`
8. ADR 0024 (ratatui on crossterm, width NOT delegated to ratatui): `docs/adr/0024-terminal-backend-ratatui-on-crossterm.md`
