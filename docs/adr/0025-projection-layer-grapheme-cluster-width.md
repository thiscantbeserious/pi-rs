# Projection-layer grapheme-cluster width

Terminal column width is computed at the message-to-cell projection layer the Core owns, not delegated to ratatui's `CellWidth`. ratatui's `CellWidth` delegates to `unicode-width` (per-codepoint UAX #11), which is the pitfall P13 failure class: a ZWJ family emoji is multiple codepoints whose widths sum incorrectly, and there is no override hook in ratatui's buffer [[1]](https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs) [[2]](https://github.com/ratatui/ratatui/issues/75). The projection segments text into grapheme clusters (`unicode-segmentation` / the width crate's own segmentation), computes each cluster's width with a grapheme-aware width crate behind a `GraphemeWidth` trait, queries mode 2027 (grapheme clustering) where the terminal advertises it, and feeds pre-widthed spans into ratatui's `Buffer`. The P13 width corpus (single-codepoint emoji, ZWJ family, VS16, CJK Hiragana/Katakana/Han, East-Asian-ambiguous, combining marks, halfwidth katakana) is the regression spec: snapshot tests assert the cell count the projection assigns, and cell-diff corruption from width drift is the failure mode the test catches.

This is the architecture decision (where width lives and why). The specific width crate is reversible behind the `GraphemeWidth` trait and is recorded in `docs/research.md`, not here.

## Considered Options

- **Accept ratatui's `unicode-width`-based `CellWidth`** — rejected: ships the P13 failure class. ratatui has open issues on ZWJ emoji truncation (`👩‍🔬` in Kitty), leftover spacing after emojis, and poor multi-byte truncation; PR #1089 improved truncation but did not fix the underlying width mapping [[2]](https://github.com/ratatui/ratatui/issues/75). No native grapheme-width override exists in ratatui's buffer. This option fails the P13 guard and the grapheme-width corpus deliverable.
- **Fork/patch ratatui's `CellWidth` to be grapheme-aware** — rejected: couples us to a ratatui fork, fighting upstream and complicating the pitfall P16 version-unification guard. The projection layer achieves the same result without forking.
- **Per-codepoint `widecharwidth` (Helix's approach) + mode 2027 query** — rejected as the primary engine: `widecharwidth` is per-codepoint, the same fundamental limitation as `unicode-width`; its own README states "a wcwidth-style per-codepoint API is fundamentally limited when it comes to composing codepoints" [[3]](https://github.com/ridiculousfish/widecharwidth). Helix uses it and still has open emoji/grapheme alignment issues. Grapheme-cluster-aware width is the P13 guard's requirement.

## Consequences

- **Width is a Core-owned concern at the projection boundary**, not a ratatui concern. The projection produces spans ratatui treats as opaque; ratatui's `Buffer::diff` (ADR 0024) still handles the double-width cell skipping correctly because the projection has already assigned correct widths.
- **The `GraphemeWidth` trait is the swap seam.** The chosen crate (see `docs/research.md`) sits behind it; a future swap for a better-maintained or Unicode-newer crate is one impl, not a projection rewrite.
- **Mode 2027 is queried at startup where available** (P13: "detect mode 2027 where available"); where unsupported, the grapheme-aware crate's default policy applies. This does not make width perfect — P13 is explicit that terminals disagree (the same ZWJ emoji advances 2, 4, 5, or 6 cells across terminals) — but it pins our chosen approximation and makes drift a detected regression, not a silent visual bug.
- **The corpus is a snapshot test, not a correctness proof.** No crate perfectly mirrors terminal rendering (P13). The corpus pins behavior so a width-crate or ratatui upgrade that changes cell assignment is caught before it ships.
- **`unicode-segmentation` coexists with the width crate's own segmentation.** ratatui already depends on `unicode-segmentation` for `Line::styled_graphemes`; the projection may use either. Whichever is touched is version-unified at the workspace level (P16 discipline).

## Sources

1. ratatui `CellWidth` trait, delegates to `unicode-width` (the limitation we bypass): <https://github.com/ratatui/ratatui/blob/1ce29d66/ratatui-core/src/buffer/cell_width.rs>
2. ratatui #75, `unicode-width` and emojis (the open failure class, no override hook): <https://github.com/ratatui/ratatui/issues/75>
3. `widecharwidth`, per-codepoint limitation stated in its own README: <https://github.com/ridiculousfish/widecharwidth>
4. Mitchell Hashimoto, grapheme clusters in terminals (the P13 evidence): <https://mitchellh.com/writing/grapheme-clusters-in-terminals>
