//! Renders the one-line header: `jam  N agents`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, count: usize) {
    let noun = if count == 1 { "agent" } else { "agents" };
    let line = Line::from(vec![
        Span::styled("jam", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {count} {noun}")),
    ]);
    frame.render_widget(line, area);
}
