//! Grapheme-cluster-aware width engine (ADR 0025, ADR 0032).
//!
//! Width is computed at the projection layer, not delegated to ratatui's
//! `CellWidth` (which uses `unicode-width`, the P13 failure class). The
//! `GraphemeWidth` trait is the swap seam (ADR 0025); `runefix-core` is the
//! chosen impl (research.md). Default terminal policy: emoji=2, CJK=2.
//!
//! Pi equivalent: pi's `graphemeWidth()` in `packages/tui/src/utils.ts` L167,
//! using `Intl.Segmenter` (grapheme segmentation) + custom emoji/CJK/zero-width
//! width (emoji=2, CJK=2). pi-rs's `GraphemeWidth` trait + runefix-core impl
//! is the Rust-native equivalent. The P13 corpus is pi-rs's regression spec
//! (pi has no corpus). Per PHILOSOPHY §9.5.

/// Trait for grapheme-cluster-aware terminal width computation (ADR 0025).
/// The swap seam: a future crate swap is one impl, not a projection rewrite.
pub trait GraphemeWidth: Send {
    /// Segment text into grapheme clusters with their terminal display width.
    /// Returns `Vec<(&str, u16)>` where the `&str` is the grapheme cluster
    /// and `u16` is its width (0, 1, or 2).
    fn grapheme_widths<'a>(&self, text: &'a str) -> Vec<(&'a str, u16)>;

    /// Total display width of a string (sum of grapheme widths).
    fn display_width(&self, text: &str) -> u16 {
        self.grapheme_widths(text).iter().map(|(_, w)| *w).sum()
    }

    /// Wrap text into lines at `max_width` columns, preserving grapheme
    /// boundaries. Splits on `\n` first (paragraph breaks), then wraps each
    /// paragraph by display width.
    fn wrap_text(&self, text: &str, max_width: u16) -> Vec<String> {
        if max_width == 0 {
            return Vec::new();
        }
        // Expand tabs to 3 spaces (pi: utils.ts L232, replaces \t with "   ").
        // runefix-core returns width 0 for \t; expanding avoids invisible tabs.
        let expanded = text.replace('\t', "   ");
        let mw = max_width as usize;
        let mut result = Vec::new();
        for paragraph in expanded.split('\n') {
            if paragraph.is_empty() {
                result.push(String::new());
                continue;
            }
            let lines = self.wrap_paragraph(paragraph, mw);
            result.extend(lines);
        }
        result
    }

    /// Wrap a single paragraph (no newlines) by display width.
    fn wrap_paragraph(&self, text: &str, max_width: usize) -> Vec<String> {
        let mut result = Vec::new();
        let mut current_line = String::new();
        let mut current_width = 0usize;

        for (grapheme, width) in self.grapheme_widths(text) {
            let w = width as usize;
            if current_width + w > max_width && !current_line.is_empty() {
                result.push(current_line.clone());
                current_line.clear();
                current_width = 0;
            }
            current_line.push_str(grapheme);
            current_width += w;
        }

        if !current_line.is_empty() || result.is_empty() {
            result.push(current_line);
        }
        result
    }
}

/// `runefix-core` implementation of `GraphemeWidth` (ADR 0025, research.md).
/// Default terminal policy: emoji=2, CJK=2. Zero dependencies.
pub struct RunefixWidth;

impl GraphemeWidth for RunefixWidth {
    fn grapheme_widths<'a>(&self, text: &'a str) -> Vec<(&'a str, u16)> {
        runefix_core::grapheme_widths(text)
            .into_iter()
            .map(|(g, w)| (g, w as u16))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: avoid temporary lifetime issues.
    fn widths(text: &str) -> Vec<(&str, u16)> {
        RunefixWidth.grapheme_widths(text)
    }

    fn wdisplay(text: &str) -> u16 {
        RunefixWidth.display_width(text)
    }

    fn wwrap(text: &str, max_width: u16) -> Vec<String> {
        RunefixWidth.wrap_text(text, max_width)
    }

    // === Width engine tests (P13 corpus) ===

    /// ASCII: each character is width 1.
    #[test]
    fn ascii_width_is_1() {
        let widths = widths("Hello");
        assert_eq!(
            widths,
            vec![("H", 1), ("e", 1), ("l", 1), ("l", 1), ("o", 1)]
        );
    }

    /// CJK Hiragana is width 2.
    #[test]
    fn hiragana_width_is_2() {
        let widths = widths("あいう");
        assert_eq!(widths, vec![("あ", 2), ("い", 2), ("う", 2)]);
    }

    /// CJK Katakana is width 2.
    #[test]
    fn katakana_width_is_2() {
        let widths = widths("カキク");
        assert_eq!(widths, vec![("カ", 2), ("キ", 2), ("ク", 2)]);
    }

    /// CJK Han (Chinese characters) is width 2.
    #[test]
    fn han_width_is_2() {
        let widths = widths("你好");
        assert_eq!(widths, vec![("你", 2), ("好", 2)]);
    }

    /// Single-codepoint emoji is width 2.
    #[test]
    fn single_emoji_width_is_2() {
        let widths = widths("😀");
        assert_eq!(widths.len(), 1);
        assert_eq!(widths[0].1, 2, "single emoji must be width 2");
    }

    /// ZWJ family emoji is a single grapheme cluster with width 2 (not split).
    #[test]
    fn zwj_family_is_single_grapheme_width_2() {
        let widths = widths("👨‍👩‍👧‍👦");
        assert_eq!(widths.len(), 1, "ZWJ family must be a single grapheme");
        assert_eq!(widths[0].1, 2, "ZWJ family must be width 2");
    }

    /// VS16 (variation selector) groups with the base grapheme.
    #[test]
    fn vs16_groups_with_base() {
        let widths = widths("❤️");
        assert_eq!(widths.len(), 1, "VS16 must group with base");
    }

    /// Combining marks group with the base (not dropped, width of the cluster).
    #[test]
    fn combining_marks_group_with_base() {
        // é as e + combining acute accent
        let widths = widths("e\u{0301}");
        assert_eq!(widths.len(), 1, "combining mark must group with base");
        assert_eq!(widths[0].0, "e\u{0301}");
    }

    /// East-Asian-ambiguous character (runefix-core decides; we just assert
    /// it does not panic and returns a width).
    #[test]
    fn east_asian_ambiguous_returns_width() {
        let widths = widths("±");
        assert_eq!(widths.len(), 1);
        // runefix-core's default policy decides; we just assert it returns.
        assert!(widths[0].1 == 1 || widths[0].1 == 2);
    }

    /// Halfwidth katakana is width 1.
    #[test]
    fn halfwidth_katakana_width_is_1() {
        let widths = widths("ｶｷｸ");
        for (_, w) in &widths {
            assert_eq!(*w, 1, "halfwidth katakana must be width 1");
        }
    }

    /// Mixed text: ASCII + CJK + emoji.
    #[test]
    fn mixed_text_widths() {
        let widths = widths("Hi你😀");
        assert_eq!(widths.len(), 4);
        assert_eq!(widths[0], ("H", 1));
        assert_eq!(widths[1], ("i", 1));
        assert_eq!(widths[2], ("你", 2));
        assert_eq!(widths[3].1, 2); // emoji
    }

    /// Empty string returns empty vec.
    #[test]
    fn empty_string_returns_empty() {
        let widths = widths("");
        assert!(widths.is_empty());
    }

    /// display_width sums grapheme widths.
    #[test]
    fn display_width_sums() {
        assert_eq!(wdisplay("Hello"), 5);
        assert_eq!(wdisplay("你好"), 4);
        assert_eq!(wdisplay(""), 0);
    }

    // === Wrapping tests ===

    /// Short text: one line.
    #[test]
    fn wrap_short_text_one_line() {
        let lines = wwrap("Hi", 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hi");
    }

    /// Exact width: one line.
    #[test]
    fn wrap_exact_width_one_line() {
        let lines = wwrap("Hello", 5);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hello");
    }

    /// Long text wraps.
    #[test]
    fn wrap_long_text_wraps() {
        let lines = wwrap("Hello World", 5);
        assert!(lines.len() >= 2);
    }

    /// Width-2 grapheme at boundary wraps to next line (not split).
    #[test]
    fn wrap_width_2_at_boundary_wraps() {
        // 3 CJK chars at width 3: line 1 = "你" (width 2), line 2 = "好" (2),
        // line 3 = "世" (2). Each is width 2, can't fit two in width 3.
        let lines = wwrap("你好世", 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "你");
        assert_eq!(lines[1], "好");
        assert_eq!(lines[2], "世");
    }

    /// Zero-width grapheme grouped with base (not dropped).
    #[test]
    fn wrap_zero_width_grouped() {
        // e + combining acute = 1 grapheme, width 1
        let lines = wwrap("e\u{0301}", 5);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "e\u{0301}");
    }

    /// Empty paragraph produces blank line.
    #[test]
    fn wrap_empty_paragraph_blank_line() {
        let lines = wwrap("", 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "");
    }

    /// Width 0 produces no output.
    #[test]
    fn wrap_width_zero_no_output() {
        let lines = wwrap("Hello", 0);
        assert!(lines.is_empty());
    }

    /// Newlines split paragraphs.
    #[test]
    fn wrap_newlines_split_paragraphs() {
        let lines = wwrap("a\nb\nc", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "a");
        assert_eq!(lines[1], "b");
        assert_eq!(lines[2], "c");
    }

    /// Mixed CJK + ASCII wrapping.
    #[test]
    fn wrap_mixed_cjk_ascii() {
        // "Hi你好" at width 4: "Hi你" (1+1+2=4), "好" (2)
        let lines = wwrap("Hi你好", 4);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Hi你");
        assert_eq!(lines[1], "好");
    }

    /// wrap_paragraph on empty returns one empty line.
    #[test]
    fn wrap_paragraph_empty() {
        let lines = RunefixWidth.wrap_paragraph("", 10);
        assert_eq!(lines.len(), 1);
    }

    // === Edge case tests ===

    /// Tab is expanded to 3 spaces (pi: utils.ts L232).
    #[test]
    fn wrap_expands_tab_to_3_spaces() {
        let lines = wwrap("a\tb", 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "a   b");
    }

    /// Single grapheme wider than max_width is placed on its own line.
    #[test]
    fn wrap_grapheme_wider_than_max_width() {
        // Emoji (width 2) at max_width 1: placed on its own line (overflow).
        let lines = wwrap("😀", 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "😀");
    }

    /// Consecutive newlines produce blank lines.
    #[test]
    fn wrap_consecutive_newlines() {
        let lines = wwrap("a\n\nb", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "a");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "b");
    }

    /// Trailing newline produces a trailing blank line.
    #[test]
    fn wrap_trailing_newline() {
        let lines = wwrap("hello\n", 10);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "");
    }

    /// Only newlines produces only blank lines.
    #[test]
    fn wrap_only_newlines() {
        let lines = wwrap("\n\n\n", 10);
        // split('\n') on "\n\n\n" gives 4 elements: ["", "", "", ""]
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|l| l.is_empty()));
    }
}
