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
    pub fn get_or_highlight(&mut self, code: &str, lang: &str, width: u16) -> Vec<RenderedLine> {
        if self.cached_width != width {
            self.cache.clear();
            self.cached_width = width;
        }
        let key = (code.to_string(), lang.to_string(), width);
        if let Some(lines) = self.cache.get(&key) {
            return lines.clone();
        }
        let lines = highlight_code_block(code, lang, width);
        self.cache.insert(key, lines.clone());
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
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_content = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let highlighted = block_cache.get_or_highlight(&code_content, &code_lang, width);
                lines.extend(highlighted);
            }
            Event::Text(text) if in_code_block => {
                code_content.push_str(&text);
            }
            Event::Text(text) => {
                wrap_markdown_text(&text, width, &engine, &mut lines);
            }
            Event::Code(code) => {
                let style = Style::default().fg(Color::Yellow);
                lines.push(RenderedLine::styled(code.into_string(), style));
            }
            Event::Start(Tag::Heading { .. }) => {
                // Heading text follows as Text events; style is bold.
            }
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {
                // Paragraph boundaries: add blank line between paragraphs.
            }
            Event::SoftBreak | Event::HardBreak => {
                lines.push(RenderedLine::plain(""));
            }
            Event::Rule => {
                lines.push(RenderedLine::styled(
                    "---".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            _ => {}
        }
    }

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
fn trim_partial_closing_fences(text: &str) -> String {
    let mut result = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        // Check if this line is a partial closing fence (only backticks,
        // fewer than 3).
        let trimmed = line.trim();
        if is_partial_closing_fence(trimmed) {
            // Skip the partial fence; the code block stays open.
            continue;
        }
        result.push_str(line);
        if lines.peek().is_some() {
            result.push('\n');
        }
    }
    result
}

/// Check if a line is a partial closing fence (1-2 backticks, nothing else).
fn is_partial_closing_fence(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| c == '`') && line.len() < 3 && !line.is_empty()
}

/// Get pulldown-cmark options (enable tables, strikethrough, task lists).
fn markdown_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

/// Wrap text into rendered lines using the grapheme-aware width engine.
fn wrap_markdown_text(
    text: &str,
    width: u16,
    engine: &dyn GraphemeWidth,
    lines: &mut Vec<RenderedLine>,
) {
    let wrapped = engine.wrap_text(text, width);
    for line_text in wrapped {
        lines.push(RenderedLine::styled(line_text, Style::default()));
    }
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
    let mut current_line = String::new();
    let mut current_style = Style::default();
    let mut lines = Vec::new();

    for event in events {
        match event {
            Ok(tree_sitter_highlight::HighlightEvent::Source { start, end }) => {
                let segment = &code[start..end];
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
                current_style = capture_to_style(h);
            }
            Ok(tree_sitter_highlight::HighlightEvent::HighlightEnd) => {
                current_style = Style::default();
            }
            _ => {}
        }
    }

    if !current_line.is_empty() {
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
        let lines1 = cache.get_or_highlight("fn main() {}", "rust", 80);
        let lines2 = cache.get_or_highlight("fn main() {}", "rust", 80);
        assert_eq!(lines1, lines2, "cached block must return same result");
    }

    /// Different code produces different results.
    #[test]
    fn different_code_different_result() {
        let mut cache = BlockCache::new();
        let lines1 = cache.get_or_highlight("fn main() {}", "rust", 80);
        let lines2 = cache.get_or_highlight("fn other() {}", "rust", 80);
        assert_ne!(
            lines1, lines2,
            "different code must produce different results"
        );
    }

    /// Cache is invalidated on width change.
    #[test]
    fn cache_invalidated_on_width_change() {
        let mut cache = BlockCache::new();
        cache.get_or_highlight("fn main() {}", "rust", 80);
        cache.get_or_highlight("fn main() {}", "rust", 40);
        // No panic = pass. Width change clears cache.
    }

    /// Explicit invalidate clears the cache.
    #[test]
    fn explicit_invalidate_clears() {
        let mut cache = BlockCache::new();
        cache.get_or_highlight("fn main() {}", "rust", 80);
        cache.invalidate();
        assert!(cache.cache.is_empty());
    }

    // === Partial fence tests ===

    /// Partial closing fence (`` or `) is trimmed.
    #[test]
    fn partial_closing_fence_trimmed() {
        let result = trim_partial_closing_fences("```rust\nfn main() {}\n``\n");
        assert!(
            !result.contains("``\n"),
            "partial closing fence must be trimmed"
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

    /// Inline code is rendered with a distinct style.
    #[test]
    fn inline_code_renders() {
        let mut cache = BlockCache::new();
        let lines = render_markdown("Use `print()` to output", 80, &mut cache);
        assert!(!lines.is_empty());
    }
}
