//! Syntax highlighting for fenced code and diffs, via `syntect`.
//!
//! The syntax and theme sets are heavy embedded dumps, so they load lazily on
//! first use behind a [`OnceLock`] — never at startup. Highlighting is a pure
//! color overlay: the visible characters are unchanged (so ANSI-stripped
//! snapshots don't move), only foreground colors are added. On a terminal
//! without truecolor we skip syntect entirely and fall back to the theme's
//! flat `code` style, since RGB escapes render wrong on a 16-color terminal.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, Theme as SynTheme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::theme;

/// Skip highlighting for code bodies larger than this (bounds worst-case cost
/// for a huge complete fence, which is highlighted once and cached).
const MAX_HIGHLIGHT_BYTES: usize = 40_000;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// A neutral dark syntect theme with good token contrast, matched loosely to
/// our dark palettes. `None` only if the embedded set is somehow empty.
fn syntect_theme() -> Option<&'static SynTheme> {
    let set = theme_set();
    set.themes
        .get("base16-ocean.dark")
        .or_else(|| set.themes.values().next())
}

/// Map a syntect token style to a ratatui style: foreground color plus
/// bold/italic. We never paint a background, so the terminal/theme background
/// shows through the code block.
fn syn_to_ratatui(syn: SynStyle) -> Style {
    let fg = syn.foreground;
    let mut style = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));
    if syn.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if syn.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    style
}

/// A stateful line-by-line highlighter for one code block or diff.
pub(super) struct Highlighter {
    inner: HighlightLines<'static>,
}

impl Highlighter {
    /// Highlight one already-width-fitted line into ratatui spans. The
    /// concatenated span text equals `text` (trailing newline stripped), so
    /// callers keep identical visible output. Falls back to a single
    /// theme-`code` span if syntect errors.
    pub(super) fn line(&mut self, text: &str) -> Vec<Span<'static>> {
        let with_nl = format!("{text}\n");
        match self.inner.highlight_line(&with_nl, syntaxes()) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(syn, piece)| {
                    Span::styled(piece.trim_end_matches('\n').to_owned(), syn_to_ratatui(syn))
                })
                .collect(),
            Err(_) => vec![Span::styled(text.to_owned(), theme::code())],
        }
    }
}

/// A highlighter for a fenced block, keyed by the fence's language token
/// (`rust`, `python`, …). `None` when highlighting is disabled (non-truecolor
/// terminal), the body is too large, or the theme set is unavailable — callers
/// then render the flat `code` style.
pub(super) fn for_language(lang: Option<&str>, body_len: usize) -> Option<Highlighter> {
    if !theme::is_truecolor() || body_len > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let theme = syntect_theme()?;
    let set = syntaxes();
    let syntax = lang
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .and_then(|l| set.find_syntax_by_token(l))
        .unwrap_or_else(|| set.find_syntax_plain_text());
    Some(Highlighter {
        inner: HighlightLines::new(syntax, theme),
    })
}

/// A highlighter for a file's diff, keyed by the file extension. `None` under
/// the same conditions as [`for_language`], or when the path has no usable
/// extension.
pub(super) fn for_path(path: Option<&str>, body_len: usize) -> Option<Highlighter> {
    if !theme::is_truecolor() || body_len > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let theme = syntect_theme()?;
    let set = syntaxes();
    let syntax = path
        .and_then(|p| std::path::Path::new(p).extension())
        .and_then(|ext| ext.to_str())
        .and_then(|ext| set.find_syntax_by_extension(ext))?;
    Some(Highlighter {
        inner: HighlightLines::new(syntax, theme),
    })
}
