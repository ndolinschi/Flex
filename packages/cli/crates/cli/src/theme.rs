//! The color theme engine. One place to retheme.
//!
//! Every view reads its styles through the accessor functions here
//! ([`user`], [`assistant`], …). Those read from a process-global active
//! [`Theme`], so switching themes at runtime (`/theme`) just swaps the global
//! and repaints — no signatures thread a `&Theme`.
//!
//! A [`Theme`] is derived from a [`Palette`] of named truecolor colors. Themes
//! ship as [`BuiltinTheme`] data. On terminals without truecolor support the
//! renderer degrades to a 16-color ANSI theme so nothing looks broken.

use std::sync::{OnceLock, RwLock};

use ratatui::style::{Color, Modifier, Style};

/// Pack an `0xRRGGBB` literal into a truecolor [`Color`].
const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A named palette. Truecolor for built-in themes; ANSI names for the fallback.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Primary background (usually the terminal default; used sparingly).
    pub bg: Color,
    /// Panel / code-block background tint.
    pub bg_alt: Color,
    /// Primary foreground.
    pub fg: Color,
    /// Secondary / dim text.
    pub fg_dim: Color,
    /// Primary accent — user prefix, titles, active model.
    pub accent: Color,
    /// Secondary accent — tool headers.
    pub accent2: Color,
    /// Tertiary accent — inline code, fenced code fallback.
    pub secondary: Color,
    /// Success / completed.
    pub success: Color,
    /// Warnings / pending.
    pub warning: Color,
    /// Errors / failures.
    pub error: Color,
    /// Diff additions.
    pub added: Color,
    /// Diff removals.
    pub removed: Color,
    /// Borders and rules.
    pub border: Color,
    /// Selected-row background.
    pub selection_bg: Color,
    /// Selected-row foreground.
    pub selection_fg: Color,
}

/// Fully-resolved styles the render code consumes, derived from a [`Palette`].
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Whether this theme renders truecolor (false for the ANSI fallback).
    pub is_truecolor: bool,
    /// The palette this was derived from — kept for syntect/gauge color reads.
    pub palette: Palette,
    /// User message prefix and text.
    pub user: Style,
    /// User message body in the transcript (`> …` rows).
    pub user_text: Style,
    /// Assistant message body.
    pub assistant: Style,
    /// Fenced code block body (fallback when not syntax-highlighted).
    pub code: Style,
    /// Fenced code block border / language tag.
    pub code_border: Style,
    /// Dim secondary text: info lines, hints, collapsed thinking.
    pub dim: Style,
    /// Thinking/reasoning body text.
    pub thinking: Style,
    /// Errors, denied/failed tool calls.
    pub error: Style,
    /// Completed tool calls, success notices.
    pub success: Style,
    /// Added lines in Edit/Write diffs.
    pub diff_add: Style,
    /// Deleted lines in Edit/Write diffs.
    pub diff_del: Style,
    /// Tool row summary while the call is running.
    pub tool_running: Style,
    /// Pending/awaiting states, notices.
    pub warn: Style,
    /// Tool header line.
    pub tool: Style,
    /// Status-bar base style.
    pub status: Style,
    /// Highlighted row in pickers and dialogs.
    pub selected: Style,
    /// Overlay/popup borders.
    pub border: Style,
    /// Overlay titles.
    pub title: Style,
    /// Bare accent foreground (splash logo, status accents, gauges).
    pub accent: Style,
}

impl Theme {
    /// Derive the full style set from a palette. Maps 1:1 to the historical
    /// style constants, so only the concrete colors change — never glyphs.
    fn from_palette(p: Palette, is_truecolor: bool) -> Self {
        Theme {
            is_truecolor,
            palette: p,
            user: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
            user_text: Style::new().fg(p.fg_dim),
            assistant: Style::new().fg(p.fg),
            code: Style::new().fg(p.secondary),
            code_border: Style::new().fg(p.border),
            dim: Style::new().fg(p.fg_dim),
            thinking: Style::new().fg(p.fg_dim).add_modifier(Modifier::ITALIC),
            error: Style::new().fg(p.error),
            success: Style::new().fg(p.success),
            diff_add: Style::new().fg(p.added),
            diff_del: Style::new().fg(p.removed),
            tool_running: Style::new().fg(p.fg),
            warn: Style::new().fg(p.warning),
            tool: Style::new().fg(p.accent2),
            status: Style::new().fg(p.fg_dim),
            selected: Style::new()
                .fg(p.selection_fg)
                .bg(p.selection_bg)
                .add_modifier(Modifier::BOLD),
            border: Style::new().fg(p.border),
            title: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
            accent: Style::new().fg(p.accent),
        }
    }
}

/// A ships-with-the-binary theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTheme {
    /// Tokyo Night (default).
    Tokyonight,
    /// Catppuccin Mocha.
    CatppuccinMocha,
    /// Gruvbox Dark.
    GruvboxDark,
    /// Nord.
    Nord,
    /// Atom One Dark.
    OneDark,
    /// opencode's warm-orange look.
    Opencode,
    /// 16-color fallback that matches a plain terminal.
    Ansi,
}

impl BuiltinTheme {
    /// The theme applied when none is stored.
    pub const DEFAULT: Self = Self::Tokyonight;

    /// Every built-in, in picker order.
    pub fn all() -> &'static [BuiltinTheme] {
        &[
            Self::Tokyonight,
            Self::CatppuccinMocha,
            Self::GruvboxDark,
            Self::Nord,
            Self::OneDark,
            Self::Opencode,
            Self::Ansi,
        ]
    }

    /// The persisted / displayed id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Tokyonight => "tokyonight",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::GruvboxDark => "gruvbox-dark",
            Self::Nord => "nord",
            Self::OneDark => "one-dark",
            Self::Opencode => "opencode",
            Self::Ansi => "ansi",
        }
    }

    /// Parse an id back into a built-in.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|t| t.id() == id)
    }

    /// This theme's truecolor palette.
    fn palette(self) -> Palette {
        match self {
            Self::Tokyonight => Palette {
                bg: rgb(0x1a1b26),
                bg_alt: rgb(0x24283b),
                fg: rgb(0xc0caf5),
                fg_dim: rgb(0x565f89),
                accent: rgb(0x7aa2f7),
                accent2: rgb(0xbb9af7),
                secondary: rgb(0x7dcfff),
                success: rgb(0x9ece6a),
                warning: rgb(0xe0af68),
                error: rgb(0xf7768e),
                added: rgb(0x9ece6a),
                removed: rgb(0xf7768e),
                border: rgb(0x3b4261),
                selection_bg: rgb(0x7aa2f7),
                selection_fg: rgb(0x1a1b26),
            },
            Self::CatppuccinMocha => Palette {
                bg: rgb(0x1e1e2e),
                bg_alt: rgb(0x313244),
                fg: rgb(0xcdd6f4),
                fg_dim: rgb(0x6c7086),
                accent: rgb(0x89b4fa),
                accent2: rgb(0xcba6f7),
                secondary: rgb(0x94e2d5),
                success: rgb(0xa6e3a1),
                warning: rgb(0xf9e2af),
                error: rgb(0xf38ba8),
                added: rgb(0xa6e3a1),
                removed: rgb(0xf38ba8),
                border: rgb(0x45475a),
                selection_bg: rgb(0x89b4fa),
                selection_fg: rgb(0x1e1e2e),
            },
            Self::GruvboxDark => Palette {
                bg: rgb(0x282828),
                bg_alt: rgb(0x3c3836),
                fg: rgb(0xebdbb2),
                fg_dim: rgb(0x928374),
                accent: rgb(0x83a598),
                accent2: rgb(0xd3869b),
                secondary: rgb(0x8ec07c),
                success: rgb(0xb8bb26),
                warning: rgb(0xfabd2f),
                error: rgb(0xfb4934),
                added: rgb(0xb8bb26),
                removed: rgb(0xfb4934),
                border: rgb(0x504945),
                selection_bg: rgb(0x83a598),
                selection_fg: rgb(0x282828),
            },
            Self::Nord => Palette {
                bg: rgb(0x2e3440),
                bg_alt: rgb(0x3b4252),
                fg: rgb(0xd8dee9),
                fg_dim: rgb(0x616e88),
                accent: rgb(0x88c0d0),
                accent2: rgb(0xb48ead),
                secondary: rgb(0x8fbcbb),
                success: rgb(0xa3be8c),
                warning: rgb(0xebcb8b),
                error: rgb(0xbf616a),
                added: rgb(0xa3be8c),
                removed: rgb(0xbf616a),
                border: rgb(0x434c5e),
                selection_bg: rgb(0x88c0d0),
                selection_fg: rgb(0x2e3440),
            },
            Self::OneDark => Palette {
                bg: rgb(0x282c34),
                bg_alt: rgb(0x2c313a),
                fg: rgb(0xabb2bf),
                fg_dim: rgb(0x5c6370),
                accent: rgb(0x61afef),
                accent2: rgb(0xc678dd),
                secondary: rgb(0x56b6c2),
                success: rgb(0x98c379),
                warning: rgb(0xe5c07b),
                error: rgb(0xe06c75),
                added: rgb(0x98c379),
                removed: rgb(0xe06c75),
                border: rgb(0x3e4451),
                selection_bg: rgb(0x61afef),
                selection_fg: rgb(0x282c34),
            },
            Self::Opencode => Palette {
                bg: rgb(0x0f0f0f),
                bg_alt: rgb(0x1a1a1a),
                fg: rgb(0xeeeeee),
                fg_dim: rgb(0x808080),
                accent: rgb(0xfab283),
                accent2: rgb(0x9d7cd8),
                secondary: rgb(0x5c9cf5),
                success: rgb(0x7fd88f),
                warning: rgb(0xfab283),
                error: rgb(0xe06c75),
                added: rgb(0x7fd88f),
                removed: rgb(0xe06c75),
                border: rgb(0x2a2a2a),
                selection_bg: rgb(0xfab283),
                selection_fg: rgb(0x0f0f0f),
            },
            Self::Ansi => ansi_palette(),
        }
    }

    /// Resolve into a renderable [`Theme`]. On non-truecolor terminals (or for
    /// [`BuiltinTheme::Ansi`]) this returns the 16-color fallback so the
    /// selected theme still "exists" but renders safely.
    pub fn resolve(self, truecolor: bool) -> Theme {
        if truecolor && self != Self::Ansi {
            Theme::from_palette(self.palette(), true)
        } else {
            Theme::from_palette(ansi_palette(), false)
        }
    }
}

/// The 16-color palette, chosen so a plain terminal renders like the pre-theme
/// build (cyan user, green code, magenta tools, …).
fn ansi_palette() -> Palette {
    Palette {
        bg: Color::Reset,
        bg_alt: Color::Black,
        fg: Color::White,
        fg_dim: Color::DarkGray,
        accent: Color::Cyan,
        accent2: Color::Magenta,
        secondary: Color::Green,
        success: Color::Green,
        warning: Color::Yellow,
        error: Color::Red,
        added: Color::Green,
        removed: Color::Red,
        border: Color::DarkGray,
        selection_bg: Color::Cyan,
        selection_fg: Color::Black,
    }
}

/// Whether the terminal advertises 24-bit truecolor support.
pub fn terminal_supports_truecolor() -> bool {
    for var in ["COLORTERM", "TERM"] {
        if let Ok(value) = std::env::var(var) {
            let value = value.to_ascii_lowercase();
            if value.contains("truecolor") || value.contains("24bit") {
                return true;
            }
        }
    }
    false
}

/// The process-global active theme.
fn cell() -> &'static RwLock<Theme> {
    static ACTIVE: OnceLock<RwLock<Theme>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(BuiltinTheme::DEFAULT.resolve(true)))
}

/// Swap the active theme. Callers repaint afterward.
pub fn set_active(theme: Theme) {
    match cell().write() {
        Ok(mut guard) => *guard = theme,
        Err(poisoned) => *poisoned.into_inner() = theme,
    }
}

/// The active theme (a cheap `Copy`).
pub fn active() -> Theme {
    match cell().read() {
        Ok(guard) => *guard,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

/// The active palette, for consumers that need raw colors (syntect, gauges).
pub fn palette() -> Palette {
    active().palette
}

/// Whether the active theme renders truecolor.
pub fn is_truecolor() -> bool {
    active().is_truecolor
}

// ── style accessors (read the active theme) ─────────────────────────────────

/// User message prefix and text.
pub fn user() -> Style {
    active().user
}
/// User message body in the transcript.
pub fn user_text() -> Style {
    active().user_text
}
/// Assistant message body.
pub fn assistant() -> Style {
    active().assistant
}
/// Fenced code block body (non-highlighted fallback).
pub fn code() -> Style {
    active().code
}
/// Fenced code block border / language tag.
pub fn code_border() -> Style {
    active().code_border
}
/// Dim secondary text.
pub fn dim() -> Style {
    active().dim
}
/// Thinking/reasoning body text.
pub fn thinking() -> Style {
    active().thinking
}
/// Errors, denied/failed tool calls.
pub fn error() -> Style {
    active().error
}
/// Completed tool calls, success notices.
pub fn success() -> Style {
    active().success
}
/// Added lines in diffs.
pub fn diff_add() -> Style {
    active().diff_add
}
/// Deleted lines in diffs.
pub fn diff_del() -> Style {
    active().diff_del
}
/// Tool row summary while running.
pub fn tool_running() -> Style {
    active().tool_running
}
/// Pending/awaiting states, notices.
pub fn warn() -> Style {
    active().warn
}
/// Tool header line.
pub fn tool() -> Style {
    active().tool
}
/// Status-bar base style.
pub fn status() -> Style {
    active().status
}
/// Highlighted row in pickers and dialogs.
pub fn selected() -> Style {
    active().selected
}
/// Overlay/popup borders.
pub fn border() -> Style {
    active().border
}
/// Overlay titles.
pub fn title() -> Style {
    active().title
}
/// Bare accent foreground (splash, status accents, gauges).
pub fn accent() -> Style {
    active().accent
}

// ── animation & glyph data (theme-independent) ──────────────────────────────

/// Spinner animation frames, indexed by tick count.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The spinner frame for a tick counter.
pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER[tick % SPINNER.len()]
}

/// Pulse animation frames for the busy line and thinking marker.
pub const PULSE: [&str; 8] = ["·", "✢", "✳", "✻", "✽", "✻", "✳", "✢"];

/// The pulse frame for a tick counter.
pub fn pulse_frame(tick: usize) -> &'static str {
    PULSE[tick % PULSE.len()]
}

/// The thinking block marker glyph.
pub const THINKING_MARK: &str = "✻";

/// Busy-line verbs; one is picked per turn and kept stable for that turn.
pub const SPINNER_VERBS: &[&str] = &[
    "Pondering",
    "Brewing",
    "Wrangling",
    "Percolating",
    "Conjuring",
    "Marinating",
    "Noodling",
    "Vibing",
    "Scheming",
    "Tinkering",
    "Ruminating",
    "Cogitating",
    "Assembling",
    "Distilling",
    "Untangling",
    "Spelunking",
    "Grokking",
    "Weaving",
    "Simmering",
    "Herding",
    "Mulling",
    "Whittling",
    "Sketching",
    "Charting",
    "Rummaging",
    "Stitching",
    "Polishing",
    "Deciphering",
    "Incubating",
    "Composing",
];

/// The busy-line verb for a per-turn index.
pub fn spinner_verb(idx: usize) -> &'static str {
    SPINNER_VERBS[idx % SPINNER_VERBS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_resolves_both_modes() {
        for theme in BuiltinTheme::all() {
            let truecolor = theme.resolve(true);
            let fallback = theme.resolve(false);
            assert!(!fallback.is_truecolor);
            // ANSI always degrades; everything else is truecolor when supported.
            if *theme == BuiltinTheme::Ansi {
                assert!(!truecolor.is_truecolor);
            } else {
                assert!(truecolor.is_truecolor);
            }
        }
    }

    #[test]
    fn id_round_trips() {
        for theme in BuiltinTheme::all() {
            assert_eq!(BuiltinTheme::from_id(theme.id()), Some(*theme));
        }
        assert_eq!(BuiltinTheme::from_id("nonexistent"), None);
    }

    #[test]
    fn set_active_swaps_the_global() {
        set_active(BuiltinTheme::Opencode.resolve(true));
        assert_eq!(active().palette.accent, rgb(0xfab283));
        set_active(BuiltinTheme::DEFAULT.resolve(true));
        assert_eq!(user(), active().user);
    }
}
