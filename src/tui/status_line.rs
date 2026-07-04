//! Renders the bottom line: key hints, or a transient message.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

pub fn render(frame: &mut Frame, area: Rect, message: Option<&str>) {
    let line = match message {
        Some(message) => Line::styled(message.to_string(), Style::default().fg(Color::Yellow)),
        None => Line::styled(
            "↵ go   x dismiss   q quit",
            Style::default().add_modifier(Modifier::DIM),
        ),
    };
    frame.render_widget(line, area);
}
