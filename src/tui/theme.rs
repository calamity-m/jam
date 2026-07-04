//! Shared style constants; named ANSI colours only, so the TUI follows the
//! user's terminal palette.

use ratatui::style::{Color, Modifier, Style};

/// Selection bar and key-hint keys.
pub const ACCENT: Color = Color::Cyan;
/// Frame border.
pub const BORDER: Color = Color::DarkGray;
/// Selected-row background.
pub const HIGHLIGHT_BG: Color = Color::DarkGray;
/// Placeholder and secondary text.
pub const MUTED: Style = Style::new().add_modifier(Modifier::DIM);
