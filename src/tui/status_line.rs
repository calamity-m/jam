//! Builds the frame's bottom border title: key hints, or a transient message.

use super::theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn title(message: Option<&str>) -> Line<'static> {
    let Some(message) = message else {
        return hints();
    };
    Line::styled(format!(" {message} "), Style::new().fg(Color::Yellow))
}

fn hints() -> Line<'static> {
    let key = Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD);
    let sep = Span::styled(" · ", theme::MUTED);
    Line::from(vec![
        Span::raw(" "),
        Span::styled("↵", key),
        Span::raw(" go"),
        sep.clone(),
        Span::styled("x", key),
        Span::raw(" dismiss"),
        sep,
        Span::styled("q", key),
        Span::raw(" quit "),
    ])
}
