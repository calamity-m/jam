//! Renders the flat agent list: one row per session, selection highlighted.

use crate::proto::{Agent, Session, SessionState};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(frame: &mut Frame, area: Rect, sessions: &[Session], selected: usize) {
    let items: Vec<ListItem> = sessions.iter().map(row).collect();
    let list = List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default();
    state.select((!sessions.is_empty()).then_some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row(session: &Session) -> ListItem<'static> {
    let (symbol, color) = state_glyph(session.state);
    let line = Line::from(vec![
        Span::styled(format!("{symbol} "), Style::default().fg(color)),
        Span::styled(
            format!("{:<8}", state_label(session.state)),
            Style::default().fg(color),
        ),
        Span::raw(format!(" {:<9}", agent_label(session.agent))),
        Span::raw(format!(
            " {:<28}",
            session.cwd.as_deref().map(shorten_home).unwrap_or_default()
        )),
        Span::styled(
            format!(" {}", session.title.as_deref().unwrap_or_default()),
            Style::default().add_modifier(Modifier::DIM),
        ),
    ]);
    ListItem::new(line)
}

/// Symbol + color per state; plain ASCII fallbacks keep rows legible in
/// terminals without the unicode glyphs.
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
