//! Renders the flat agent list: one row per session, selection highlighted.

use super::theme;
use crate::proto::{Agent, Session, SessionState};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(frame: &mut Frame, area: Rect, sessions: &[Session], selected: usize) {
    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, session)| row(session, i == selected))
        .collect();
    let list = List::new(items)
        .highlight_style(Style::new().bg(theme::HIGHLIGHT_BG))
        .highlight_symbol(Span::styled("▌", Style::new().fg(theme::ACCENT)));
    let mut state = ListState::default();
    state.select((!sessions.is_empty()).then_some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row(session: &Session, selected: bool) -> ListItem<'static> {
    let (symbol, mut color) = state_glyph(session.state);
    // DarkGray fg (stale) vanishes against the DarkGray highlight bg.
    if selected && color == theme::HIGHLIGHT_BG {
        color = Color::Gray;
    }
    let line = Line::from(vec![
        Span::styled(format!("{symbol} "), Style::default().fg(color)),
        Span::styled(
            format!("{:<8}", state_label(session.state)),
            Style::default().fg(color),
        ),
        Span::raw(format!(" {:<9}", agent_label(session.agent))),
        match session.cwd.as_deref().map(shorten_home) {
            Some(cwd) => Span::raw(format!(" {cwd:<28}")),
            None => Span::styled(format!(" {:<28}", "—"), theme::MUTED),
        },
        Span::styled(
            format!(" {}", session.title.as_deref().unwrap_or("—")),
            muted_style(selected),
        ),
    ]);
    ListItem::new(line)
}

/// Symbol + color per state; plain ASCII fallbacks keep rows legible in
/// terminals without the unicode glyphs.
fn muted_style(selected: bool) -> Style {
    if selected {
        // Dim default-colour text can disappear against the selected row's
        // DarkGray background; use explicit gray like selected stale rows.
        Style::default().fg(Color::Gray)
    } else {
        theme::MUTED
    }
}

fn state_glyph(state: SessionState) -> (&'static str, Color) {
    match state {
        SessionState::WaitingInput => ("●", Color::Yellow),
        SessionState::Error => ("●", Color::Red),
        SessionState::Start => ("○", Color::Cyan),
        SessionState::Working => ("○", Color::Blue),
        SessionState::Done => ("●", Color::Green),
        SessionState::Stale => ("·", Color::DarkGray),
    }
}

fn state_label(state: SessionState) -> &'static str {
    match state {
        SessionState::WaitingInput => "waiting",
        SessionState::Error => "error",
        SessionState::Start => "start",
        SessionState::Working => "working",
        SessionState::Done => "done",
        SessionState::Stale => "stale",
    }
}

fn agent_label(agent: Agent) -> &'static str {
    match agent {
        Agent::Pi => "pi",
        Agent::ClaudeCode => "claude",
        Agent::Codex => "codex",
        Agent::Opencode => "opencode",
        Agent::Custom => "custom",
    }
}

fn shorten_home(cwd: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && cwd.starts_with(&home) => {
            format!("~{}", &cwd[home.len()..])
        }
        _ => cwd.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    #[test]
    fn selected_title_uses_visible_gray_without_dim() {
        let style = muted_style(true);
        assert_eq!(style.fg, Some(Color::Gray));
        assert!(!style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn unselected_title_stays_muted() {
        assert_eq!(muted_style(false), theme::MUTED);
    }
}
