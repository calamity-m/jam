//! Builds the frame's top border title: `jam · N agents`.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub fn title(count: usize) -> Line<'static> {
    let noun = if count == 1 { "agent" } else { "agents" };
    Line::from(vec![
        Span::styled(" jam", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(format!(" · {count} {noun} ")),
    ])
}
