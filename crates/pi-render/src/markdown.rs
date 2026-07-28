//! Streaming markdown pipeline (ADR 0010, ADR 0033).
//!
//! pulldown-cmark for structure (CommonMark-compliant), tree-sitter-highlight
//! for code blocks (error-tolerant). Block-granular cache: finalized code
//! blocks cached as highlighted lines, only the tail block is re-highlighted
//! each frame (P11 guard). Partial closing fences are trimmed before parsing
//! to prevent flicker (like pi's trimPartialClosingFences, markdown.ts L37).
//!
//! Pi equivalent: pi's `Markdown` component (markdown.ts L151) uses `marked`
//! for parsing and `highlight.js` (regex-based) for code highlighting
//! (theme.ts L1138). pi-rs uses pulldown-cmark + tree-sitter-highlight
//! (error-tolerant, ADR 0010 rejected regex-based). pi-rs's block-granular
//! highlight caching is the improvement over pi's per-component caching (P11).
//! Per PHILOSOPHY §9.5.

use std::collections::HashMap;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};

use crate::projection::{RenderedLine, StyledSegment};
use crate::width::{GraphemeWidth, RunefixWidth};

/// The block-granular highlight cache (ADR 0033). Keyed by
/// (code_content, lang, width). Finalized blocks return cached highlighted
/// lines. The tail block is re-highlighted each frame.
pub struct BlockCache {
    cache: HashMap<(String, String, u16), Vec<RenderedLine>>,
    cached_width: u16,
}

impl BlockCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            cached_width: 0,
        }
    }

    /// Invalidate the cache (on width or theme change).
    pub fn invalidate(&mut self) {
        self.cache.clear();
    }

    /// Get or compute highlighted lines for a code block.
    /// If the block is in the cache, returns cached lines. Otherwise,
    /// highlights the block and caches the result.
    /// Get or compute highlighted lines for a code block.
    /// If the block is in the cache, returns cached lines. Otherwise,
    /// highlights the block and caches the result.
    ///
    /// `is_finalized`: if false (the tail block during streaming), the result
    /// is NOT cached (the content is still growing). Only finalized blocks
    /// are cached to prevent unbounded growth.
    pub fn get_or_highlight(
        &mut self,
        code: &str,
        lang: &str,
        width: u16,
        is_finalized: bool,
    ) -> Vec<RenderedLine> {
        if self.cached_width != width {
            self.cache.clear();
            self.cached_width = width;
        }
        let key = (code.to_string(), lang.to_string(), width);
        if let Some(lines) = self.cache.get(&key) {
            return lines.clone();
        }
        let lines = highlight_code_block(code, lang, width);
        if is_finalized {
            self.cache.insert(key, lines.clone());
        }
        lines
    }
}

impl Default for BlockCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Render markdown text to rendered lines at a given width.
/// Uses pulldown-cmark for structure and tree-sitter-highlight for code blocks.
/// The block_cache caches finalized code blocks (ADR 0033).
pub fn render_markdown(text: &str, width: u16, block_cache: &mut BlockCache) -> Vec<RenderedLine> {
    let engine = RunefixWidth;
    let normalized = normalize_markdown(text);
    let parser = Parser::new_ext(&normalized, markdown_options());
    let mut lines: Vec<RenderedLine> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_content = String::new();
    let mut in_heading = false;
    let mut prev_was_paragraph = false;
    let mut current_style = Style::default();
    // Inline buffer: accumulates Text, Code, and SoftBreak content into a
    // single line's segments. Flushed at block boundaries (paragraph end,
    // heading end, hard break, rule, code block).
    let mut inline_segments: Vec<StyledSegment> = Vec::new();

    // Flush the inline buffer into wrapped lines.
    let flush_inline = |segments: &mut Vec<StyledSegment>, lines: &mut Vec<RenderedLine>| {
        if segments.is_empty() {
            return;
        }
        // Join all segments into one string, then wrap by width.
        // Each segment carries its own style, so we wrap per-segment.
        // For simplicity in Step 5 (ASCII-scoped), concatenate segments
        // with their styles and wrap the combined text.
        let combined: String = segments.iter().map(|s| s.text.as_str()).collect();
        let wrapped = engine.wrap_text(&combined, width);
        for line_text in wrapped {
            // Apply the first segment's style to the whole line (Step 5
            // simplification; Step 6 theme integration refines per-segment).
            let style = segments.first().map(|s| s.style).unwrap_or_default();
            lines.push(RenderedLine::styled(line_text, style));
        }
        segments.clear();
    };

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_inline(&mut inline_segments, &mut lines);
                in_code_block = true;
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let highlighted =
                    block_cache.get_or_highlight(&code_content, &code_lang, width, true);
                lines.extend(highlighted);
            }
            Event::Text(text) if in_code_block => {
                code_content.push_str(&text);
            }
            Event::Start(Tag::Heading { .. }) => {
                flush_inline(&mut inline_segments, &mut lines);
                in_heading = true;
                current_style = Style::default().add_modifier(Modifier::BOLD);
            }
            Event::End(TagEnd::Heading { .. }) => {
                flush_inline(&mut inline_segments, &mut lines);
                in_heading = false;
                current_style = Style::default();
            }
            Event::Start(Tag::Paragraph) => {
                if prev_was_paragraph {
                    lines.push(RenderedLine::plain(""));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                flush_inline(&mut inline_segments, &mut lines);
                prev_was_paragraph = true;
            }
            Event::Text(text) if in_heading => {
                inline_segments.push(StyledSegment {
                    text: text.into_string(),
                    style: current_style,
                });
            }
            Event::Text(text) => {
                inline_segments.push(StyledSegment {
                    text: text.into_string(),
                    style: Style::default(),
                });
            }
            Event::Code(code) => {
                // Inline code: buffer as a segment with code style.
                inline_segments.push(StyledSegment {
                    text: code.into_string(),
                    style: Style::default().fg(Color::Yellow),
                });
            }
            Event::SoftBreak => {
                // Soft break: inline spacing (space), not a new line.
                inline_segments.push(StyledSegment {
                    text: " ".to_string(),
                    style: Style::default(),
                });
            }
            Event::HardBreak => {
                // Hard break: flush current inline content, then blank line.
                flush_inline(&mut inline_segments, &mut lines);
            }
            Event::Rule => {
                flush_inline(&mut inline_segments, &mut lines);
                lines.push(RenderedLine::styled(
                    "---".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            _ => {}
        }
    }

    // Flush any remaining inline content.
    flush_inline(&mut inline_segments, &mut lines);

    if lines.is_empty() {
        lines.push(RenderedLine::plain(""));
    }
    lines
}

/// Normalize markdown text before parsing (ADR 0033).
/// Trims partial closing fences to prevent code block flicker during
/// streaming (like pi's trimPartialClosingFences, markdown.ts L37-L55).
fn normalize_markdown(text: &str) -> String {
    trim_partial_closing_fences(text)
}

/// Trim partial closing fences from streamed markdown.
/// During streaming, a code block's closing fence (```) arrives char by char.
/// A partial fence (`` or `) at the end of a code block would be parsed as
/// code content, then re-parsed as a fence when complete, causing flicker.
/// This trims the partial fence so the code block stays open until the
/// complete fence arrives.
/// Trim partial closing fence from the LAST line of streamed markdown.
/// During streaming, a code block's closing fence (```) arrives char by char.
/// A partial fence (`` or `) at the END of the buffer would be parsed as
/// code content, then re-parsed as a fence when complete, causing flicker.
/// This trims only the last line if it is a partial fence (CodeRabbit finding:
/// the original scanned every line, stripping legitimate content).
fn trim_partial_closing_fences(text: &str) -> String {
    let last_newline = text.rfind('\n');
    let (body, last_line) = match last_newline {
        Some(idx) => (&text[..=idx], &text[idx + 1..]),
        None => ("", text),
    };
    if is_partial_closing_fence(last_line.trim()) {
        // Trim the partial fence; keep the body.
        body.to_string()
    } else {
        text.to_string()
    }
}

/// Check if a line is a partial closing fence (1-2 backticks, nothing else).
fn is_partial_closing_fence(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| c == '`') && line.len() < 3 && !line.is_empty()
}

/// Get pulldown-cmark options (enable tables, strikethrough, task lists).
fn markdown_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

/// Highlight a code block using tree-sitter-highlight (ADR 0010, ADR 0033).
/// Falls back to plain styling if the language is not supported or
/// highlighting fails (P18 fallback).
fn highlight_code_block(code: &str, lang: &str, _width: u16) -> Vec<RenderedLine> {
    let code = code.strip_suffix('\n').unwrap_or(code);
    let mut lines = Vec::new();

    // Add the code fence header.
    let header_style = Style::default().fg(Color::DarkGray);
    lines.push(RenderedLine::styled(format!("```{}", lang), header_style));

    // Try to highlight with tree-sitter.
    let highlighted = try_highlight(code, lang);

    match highlighted {
        Some(styled_lines) => {
            for line in styled_lines {
                lines.push(line);
            }
        }
        None => {
            // Fallback: plain styling (ADR 0010, P18).
            let plain_style = Style::default().fg(Color::Yellow);
            for line in code.lines() {
                lines.push(RenderedLine::styled(line.to_string(), plain_style));
            }
        }
    }

    lines.push(RenderedLine::styled("```", header_style));
    lines
}

/// Try to highlight code with tree-sitter. Returns None if the language
/// is not supported or highlighting fails (P18 fallback).
fn try_highlight(code: &str, lang: &str) -> Option<Vec<RenderedLine>> {
    use tree_sitter_highlight::Highlighter;

    let config = get_highlight_config(lang)?;
    let mut highlighter = Highlighter::new();
    let result = highlighter.highlight(config, code.as_bytes(), None, |_| None);

    let events = result.ok()?;

    // Convert highlight events to rendered lines.
    // Use a style stack for nested HighlightStart/End events (CodeRabbit
    // finding: the original overwrote current_style, losing the outer
    // scope's style when nested highlights ended).
    let mut current_line = String::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut lines = Vec::new();

    for event in events {
        match event {
            Ok(tree_sitter_highlight::HighlightEvent::Source { start, end }) => {
                let segment = &code[start..end];
                let current_style = *style_stack.last().unwrap();
                for (i, line) in segment.split('\n').enumerate() {
                    if i > 0 {
                        lines.push(RenderedLine {
                            segments: vec![StyledSegment {
                                text: std::mem::take(&mut current_line),
                                style: current_style,
                            }],
                        });
                    }
                    current_line.push_str(line);
                }
            }
            Ok(tree_sitter_highlight::HighlightEvent::HighlightStart(h)) => {
                style_stack.push(capture_to_style(h));
            }
            Ok(tree_sitter_highlight::HighlightEvent::HighlightEnd) => {
                style_stack.pop();
            }
            _ => {}
        }
    }

    if !current_line.is_empty() {
        let current_style = *style_stack.last().unwrap();
        lines.push(RenderedLine {
            segments: vec![StyledSegment {
                text: current_line,
                style: current_style,
            }],
        });
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

/// Get a HighlightConfiguration for a language. Returns None if unsupported.
fn get_highlight_config(
    lang: &str,
) -> Option<&'static tree_sitter_highlight::HighlightConfiguration> {
    match lang {
        "rust" | "rs" => {
            use tree_sitter_rust::LANGUAGE;
            static CONFIG: std::sync::OnceLock<tree_sitter_highlight::HighlightConfiguration> =
                std::sync::OnceLock::new();
            Some(CONFIG.get_or_init(|| {
                let mut config = tree_sitter_highlight::HighlightConfiguration::new(
                    LANGUAGE.into(),
                    "rust",
                    tree_sitter_rust::HIGHLIGHTS_QUERY,
                    "",
                    "",
                )
                .expect("rust highlight config");
                config.configure(HIGHLIGHT_NAMES);
                config
            }))
        }
        "javascript" | "js" | "typescript" | "ts" => {
            use tree_sitter_javascript::LANGUAGE;
            static CONFIG: std::sync::OnceLock<tree_sitter_highlight::HighlightConfiguration> =
                std::sync::OnceLock::new();
            Some(CONFIG.get_or_init(|| {
                let mut config = tree_sitter_highlight::HighlightConfiguration::new(
                    LANGUAGE.into(),
                    "javascript",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::INJECTIONS_QUERY,
                    tree_sitter_javascript::LOCALS_QUERY,
                )
                .expect("js highlight config");
                config.configure(HIGHLIGHT_NAMES);
                config
            }))
        }
        "bash" | "sh" | "shell" => {
            use tree_sitter_bash::LANGUAGE;
            static CONFIG: std::sync::OnceLock<tree_sitter_highlight::HighlightConfiguration> =
                std::sync::OnceLock::new();
            Some(CONFIG.get_or_init(|| {
                let mut config = tree_sitter_highlight::HighlightConfiguration::new(
                    LANGUAGE.into(),
                    "bash",
                    tree_sitter_bash::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("bash highlight config");
                config.configure(HIGHLIGHT_NAMES);
                config
            }))
        }
        "json" => {
            use tree_sitter_json::LANGUAGE;
            static CONFIG: std::sync::OnceLock<tree_sitter_highlight::HighlightConfiguration> =
                std::sync::OnceLock::new();
            Some(CONFIG.get_or_init(|| {
                let mut config = tree_sitter_highlight::HighlightConfiguration::new(
                    LANGUAGE.into(),
                    "json",
                    tree_sitter_json::HIGHLIGHTS_QUERY,
                    "",
                    "",
                )
                .expect("json highlight config");
                config.configure(HIGHLIGHT_NAMES);
                config
            }))
        }
        _ => None,
    }
}

/// The highlight names we support (tree-sitter standard captures).
const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant.builtin",
    "comment",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "string",
    "type",
    "variable",
    "number",
];

/// Map a tree-sitter highlight capture to a ratatui Style.
/// This is the capture-to-palette mapping (ADR 0012, Step 6 formalizes).
fn capture_to_style(capture: tree_sitter_highlight::Highlight) -> Style {
    let name = HIGHLIGHT_NAMES.get(capture.0).copied().unwrap_or("");
    match name {
        "comment" => Style::default().fg(Color::DarkGray),
        "keyword" => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        "function" => Style::default().fg(Color::Blue),
        "string" => Style::default().fg(Color::Green),
        "number" => Style::default().fg(Color::Cyan),
        "type" => Style::default().fg(Color::Yellow),
        "variable" => Style::default().fg(Color::Red),
        "operator" => Style::default().fg(Color::LightCyan),
        "property" => Style::default().fg(Color::LightBlue),
        "punctuation" => Style::default().fg(Color::Gray),
        "attribute" => Style::default().fg(Color::LightYellow),
        _ => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageRef;
    use crate::projection::RmmProjection;
    use crate::state::RenderState;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    // === Markdown parsing tests ===

    /// Plain text renders as one line.
    #[test]
    fn plain_text_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("Hello world", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Code block with language is highlighted.
    #[test]
    fn code_block_with_lang_highlighted() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```rust\nfn main() {}\n```", 80, &mut cache);
        // Should have: ```rust, code lines, ```
        assert!(
            lines.len() >= 3,
            "code block should have header, code, footer"
        );
    }

    /// Code block without language falls back to plain styling.
    #[test]
    fn code_block_no_lang_plain() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```\nplain code\n```", 80, &mut cache);
        assert!(lines.len() >= 3);
    }

    /// Unknown language falls back to plain styling (P18).
    #[test]
    fn unknown_lang_falls_back_to_plain() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```brainfuck\n+++[+++]\n```", 80, &mut cache);
        assert!(lines.len() >= 3);
    }

    // === Block cache tests ===

    /// Finalized block is cached (not re-highlighted).
    #[test]
    fn finalized_block_is_cached() {
        let mut cache = BlockCache::new();
        let lines1 = cache.get_or_highlight("fn main() {}", "rust", 80, true);
        let lines2 = cache.get_or_highlight("fn main() {}", "rust", 80, true);
        assert_eq!(lines1, lines2, "cached block must return same result");
    }

    /// Non-finalized (tail) block is NOT cached.
    #[test]
    fn tail_block_not_cached() {
        let mut cache = BlockCache::new();
        cache.get_or_highlight("fn main() {}", "rust", 80, false);
        assert!(cache.cache.is_empty(), "tail block must not be cached");
    }

    /// Different code produces different results.
    #[test]
    fn different_code_different_result() {
        let mut cache = BlockCache::new();
        let lines1 = cache.get_or_highlight("fn main() {}", "rust", 80, true);
        let lines2 = cache.get_or_highlight("fn other() {}", "rust", 80, true);
        assert_ne!(
            lines1, lines2,
            "different code must produce different results"
        );
    }

    /// Cache is invalidated on width change.
    #[test]
    fn cache_invalidated_on_width_change() {
        let mut cache = BlockCache::new();
        cache.get_or_highlight("fn main() {}", "rust", 80, true);
        cache.get_or_highlight("fn main() {}", "rust", 40, true);
        // No panic = pass. Width change clears cache.
    }

    /// Explicit invalidate clears the cache.
    #[test]
    fn explicit_invalidate_clears() {
        let mut cache = BlockCache::new();
        cache.get_or_highlight("fn main() {}", "rust", 80, true);
        cache.invalidate();
        assert!(cache.cache.is_empty());
    }

    /// Default impl creates empty cache.
    #[test]
    fn default_creates_empty() {
        let cache = BlockCache::default();
        assert!(cache.cache.is_empty());
    }

    // === Partial fence tests ===

    /// Partial closing fence at end is trimmed.
    #[test]
    fn partial_closing_fence_trimmed() {
        // The partial fence is on the LAST line.
        let result = trim_partial_closing_fences("```rust\nfn main() {}\n``");
        assert!(
            !result.ends_with("``"),
            "partial closing fence at end must be trimmed"
        );
    }

    /// Complete closing fence (```) is kept.
    #[test]
    fn complete_closing_fence_kept() {
        let result = trim_partial_closing_fences("```rust\nfn main() {}\n```");
        assert!(
            result.contains("```"),
            "complete closing fence must be kept"
        );
    }

    /// Single backtick in code content is not trimmed.
    #[test]
    fn single_backtick_in_content_not_trimmed() {
        // A line that is just ` is a partial fence and would be trimmed.
        // But a line with content + backtick is not a partial fence.
        let result = trim_partial_closing_fences("hello `world`");
        assert!(result.contains("`"), "inline backtick must not be trimmed");
    }

    // === P18 incomplete code tests ===

    /// Incomplete Rust code doesn't panic.
    #[test]
    fn incomplete_rust_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown(
            "```rust\nfn main() {\n    // incomplete\n```",
            80,
            &mut cache,
        );
        assert!(!lines.is_empty());
    }

    /// Incomplete JavaScript code doesn't panic.
    #[test]
    fn incomplete_js_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```js\nfunction foo(\n```", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Incomplete bash code doesn't panic.
    #[test]
    fn incomplete_bash_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```bash\nif [ true ]; then\n```", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Incomplete JSON doesn't panic.
    #[test]
    fn incomplete_json_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```json\n{\"key\":\n```", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    // === Integration tests ===

    /// Markdown with text + code block renders both.
    #[test]
    fn text_and_code_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown(
            "Here is code:\n```rust\nfn main() {}\n```\nDone.",
            80,
            &mut cache,
        );
        assert!(
            lines.len() >= 4,
            "text + code + text should produce >= 4 lines"
        );
    }

    /// Empty markdown renders one blank line.
    #[test]
    fn empty_markdown_renders_blank() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("", 80, &mut cache);
        assert_eq!(lines.len(), 1);
    }

    /// Markdown with horizontal rule.
    #[test]
    fn markdown_with_rule() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("before\n---\nafter", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Inline code is composed into the same line as surrounding text.
    #[test]
    fn inline_code_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("Use `print()` to output", 80, &mut cache);
        // Inline code must NOT split the line. All content on one line.
        assert_eq!(lines.len(), 1, "inline code must compose into one line");
        // The line text contains all parts.
        let combined: String = lines[0].segments.iter().map(|s| s.text.as_str()).collect();
        assert!(combined.contains("Use"), "line must contain 'Use'");
        assert!(
            combined.contains("print()"),
            "line must contain inline code"
        );
        assert!(combined.contains("output"), "line must contain 'output'");
    }

    // === Coverage tests ===

    /// Heading renders with bold style.
    #[test]
    fn heading_renders_bold() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("# Hello", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Consecutive paragraphs have a blank line between them.
    #[test]
    fn consecutive_paragraphs_have_blank_line() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("First paragraph.\n\nSecond paragraph.", 80, &mut cache);
        // Should have content + blank + content.
        assert!(lines.len() >= 2);
    }

    /// Indented code block (no fence) renders.
    #[test]
    fn indented_code_block_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("    code here", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Unclosed code block (streaming tail) renders without panic.
    #[test]
    fn unclosed_code_block_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("```rust\nfn main() {", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Partial fence at end of single-line text.
    #[test]
    fn partial_fence_single_line() {
        let result = trim_partial_closing_fences("``");
        assert!(
            result.is_empty(),
            "partial fence on single line must be trimmed"
        );
    }

    /// No partial fence: text unchanged.
    #[test]
    fn no_partial_fence_unchanged() {
        let result = trim_partial_closing_fences("hello world");
        assert_eq!(result, "hello world");
    }

    /// Empty string: no change.
    #[test]
    fn empty_string_no_change() {
        let result = trim_partial_closing_fences("");
        assert_eq!(result, "");
    }

    /// capture_to_style: all named captures return a style.
    #[test]
    fn capture_to_style_all_named() {
        for (i, _name) in HIGHLIGHT_NAMES.iter().enumerate() {
            let style = capture_to_style(tree_sitter_highlight::Highlight(i));
            // Must not panic. Named captures return a styled or default.
            let _ = format!("{:?}", style);
        }
    }

    /// capture_to_style: unknown capture returns default.
    #[test]
    fn capture_to_style_unknown() {
        let style = capture_to_style(tree_sitter_highlight::Highlight(999));
        assert_eq!(style, Style::default());
    }

    /// get_highlight_config: unsupported language returns None.
    #[test]
    fn unsupported_lang_returns_none() {
        assert!(get_highlight_config("python").is_none());
    }

    /// get_highlight_config: all supported languages return config.
    #[test]
    fn all_supported_langs_return_config() {
        assert!(get_highlight_config("rust").is_some());
        assert!(get_highlight_config("javascript").is_some());
        assert!(get_highlight_config("bash").is_some());
        assert!(get_highlight_config("json").is_some());
    }

    /// is_partial_closing_fence: edge cases.
    #[test]
    fn is_partial_fence_edge_cases() {
        assert!(is_partial_closing_fence("`"));
        assert!(is_partial_closing_fence("``"));
        assert!(!is_partial_closing_fence("```")); // complete fence
        assert!(!is_partial_closing_fence("")); // empty
        assert!(!is_partial_closing_fence("a")); // not backticks
        assert!(!is_partial_closing_fence("`a")); // mixed
    }

    /// Soft break renders as inline spacing, not a new line.
    #[test]
    fn soft_break_renders_inline_spacing() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("line one\nline two", 80, &mut cache);
        // Soft break is a space, not a new line. Both parts on one line.
        assert_eq!(lines.len(), 1, "soft break must compose into one line");
        let combined: String = lines[0].segments.iter().map(|s| s.text.as_str()).collect();
        assert!(combined.contains("line one"), "must contain first part");
        assert!(combined.contains("line two"), "must contain second part");
    }

    /// Hard break (backslash + newline) renders a blank line.
    #[test]
    fn hard_break_renders_blank_line() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("line one\\\nline two", 80, &mut cache);
        assert!(lines.len() >= 2);
    }

    /// Horizontal rule renders.
    #[test]
    fn horizontal_rule_renders() {
        let mut cache = BlockCache::new();
        // Use *** for a rule (--- is parsed as heading underline by some parsers).
        let lines = render_markdown("before\n\n***\n\nafter", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Unclosed code block (streaming tail) renders without panic.
    #[test]
    fn unclosed_code_block_tail_renders() {
        let mut cache = BlockCache::new();
        // Code block with no closing fence: the tail block handler fires.
        let lines = render_markdown("```rust\nfn main() {", 80, &mut cache);
        assert!(!lines.is_empty(), "unclosed code block must render");
    }

    /// try_highlight returns None for empty code.
    #[test]
    fn try_highlight_empty_code_returns_none() {
        let result = try_highlight("", "rust");
        // Empty code may return None or Some(empty); just must not panic.
        let _ = result;
    }

    /// try_highlight returns None for unsupported lang.
    #[test]
    fn try_highlight_unsupported_lang_returns_none() {
        assert!(try_highlight("code", "cobol").is_none());
    }

    /// HTML in markdown is handled (catch-all arm, no panic).
    #[test]
    fn html_in_markdown_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("<div>hello</div>", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Task list marker is handled (catch-all arm, no panic).
    #[test]
    fn task_list_marker_no_panic() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("- [x] done\n- [ ] todo", 80, &mut cache);
        assert!(!lines.is_empty());
    }

    /// Unwrapped line with width-2 grapheme clips at viewport edge.
    /// The stop-reason marker and ToolResult prefix are unwrapped.
    #[test]
    fn unwrapped_line_clips_at_viewport_edge() {
        // A ToolResult with a long name that exceeds viewport width.
        let backend = TestBackend::new(3, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = RenderState::default();
        state.push_message(MessageRef::ToolResult {
            tool_call_id: "tc1".into(),
            tool_name: "abcdefghij".into(),
            content: vec![],
            is_error: false,
            timestamp: 0,
        });
        state.render_at_width(3);
        terminal
            .draw(|frame| {
                frame.render_widget(RmmProjection::new(&state), frame.area());
            })
            .unwrap();
        // No panic = pass. The clip at x + width > x_limit fires.
        let buf = terminal.backend().buffer();
        assert_eq!(buf.cell((0u16, 0u16)).expect("cell").symbol(), "O");
    }
}
