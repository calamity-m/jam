//! The TUI's central loop: subscribe to the daemon, redraw on updates,
//! handle keys. Rendering details live in the sibling modules.

use super::theme;
use crate::mux::{self, FocusError};
use crate::proto::{Request, Response, Session, SessionState};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType};
use ratatui::{DefaultTerminal, Frame};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub message: Option<String>,
}

pub fn run() -> io::Result<()> {
    let stream = crate::daemon::connect_or_spawn()?;
    let mut writer = stream.try_clone()?;
    send(&mut writer, &Request::Subscribe)?;

    // Reader thread turns broadcast lines into channel messages; the loop
    // below stays free to poll the keyboard.
    let (tx, rx) = mpsc::channel::<Vec<Session>>();
    let reader = BufReader::new(stream);
    std::thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Ok(Response::Sessions { sessions }) = serde_json::from_str(&line)
                && tx.send(sessions).is_err()
            {
                break;
            }
        }
    });

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &rx, &mut writer);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    rx: &mpsc::Receiver<Vec<Session>>,
    writer: &mut UnixStream,
) -> io::Result<()> {
    let mut app = App {
        sessions: Vec::new(),
        selected: 0,
        message: None,
    };
    loop {
        loop {
            match rx.try_recv() {
                Ok(sessions) => app.update_sessions(sessions),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    app.message = Some("daemon connection lost (q to quit)".into());
                    break;
                }
            }
        }
        terminal.draw(|frame| draw(frame, &app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
            KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
            KeyCode::Enter => focus_selected(&mut app, writer)?,
            KeyCode::Char('x') => {
                if let Some(session) = app.sessions.get(app.selected) {
                    let session_id = session.session_id.clone();
                    send(writer, &Request::Dismiss { session_id })?;
                }
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::BORDER))
        .title_top(super::header_line::title(app.sessions.len()))
        .title_bottom(super::status_line::title(app.message.as_deref()));
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());
    super::agent_pane::render(frame, inner, &app.sessions, app.selected);
}

/// Jump to the selected agent's pane via its multiplexer backend. A missing
/// pane is reported to the daemon as stale (SPEC: session→pane drift).
fn focus_selected(app: &mut App, writer: &mut UnixStream) -> io::Result<()> {
    let Some(session) = app.sessions.get(app.selected) else {
        return Ok(());
    };
    let (Some(mux), Some(pane_ref)) = (session.mux, session.pane_ref.as_deref()) else {
        app.message = Some("no pane recorded for this session".into());
        return Ok(());
    };
    match mux::focus(mux, session.mux_session.as_deref(), pane_ref) {
        Ok(()) => app.message = None,
        Err(FocusError::PaneGone) => {
            let session_id = session.session_id.clone();
            app.message = Some("pane is gone; session marked stale".into());
            send(writer, &Request::MarkStale { session_id })?;
        }
        Err(FocusError::Failed(msg)) => app.message = Some(msg),
    }
    Ok(())
}

fn send(writer: &mut UnixStream, request: &Request) -> io::Result<()> {
    writeln!(writer, "{}", serde_json::to_string(request).unwrap())
}

impl App {
    /// Adopt a fresh session list, keeping the cursor on the same session
    /// where possible so broadcasts don't yank the selection around.
    fn update_sessions(&mut self, mut sessions: Vec<Session>) {
        sessions.sort_by(|a, b| {
            priority(a.state)
                .cmp(&priority(b.state))
                .then(a.last_event_at.cmp(&b.last_event_at))
                .then(a.session_id.cmp(&b.session_id))
        });
        let selected_id = self
            .sessions
            .get(self.selected)
            .map(|s| s.session_id.clone());
        self.sessions = sessions;
        self.selected = selected_id
            .and_then(|id| self.sessions.iter().position(|s| s.session_id == id))
            .unwrap_or(0)
            .min(self.sessions.len().saturating_sub(1));
    }

    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.sessions.len().saturating_sub(1));
    }
}

/// Attention priority: rows needing the user first, stale last. Within a
/// bucket, older events sort first (they have waited the longest).
fn priority(state: SessionState) -> u8 {
    match state {
        SessionState::WaitingInput | SessionState::Error => 0,
        SessionState::Start | SessionState::Working => 1,
        SessionState::Done => 2,
        SessionState::Stale => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Agent, EventKind};

    fn session(id: &str, state: SessionState, at: u64) -> Session {
        Session {
            session_id: id.into(),
            agent: Agent::ClaudeCode,
            state,
            title: None,
            cwd: None,
            mux: None,
            mux_session: None,
            pane_ref: None,
            last_event: EventKind::Working,
            last_event_at: at,
        }
    }

    fn empty_app() -> App {
        App {
            sessions: Vec::new(),
            selected: 0,
            message: None,
        }
    }

    #[test]
    fn sorts_by_attention_priority_then_age() {
        let mut app = empty_app();
        app.update_sessions(vec![
            session("stale", SessionState::Stale, 10),
            session("done", SessionState::Done, 20),
            session("working", SessionState::Working, 30),
            session("waiting-new", SessionState::WaitingInput, 50),
            session("waiting-old", SessionState::WaitingInput, 40),
            session("error", SessionState::Error, 60),
        ]);
        let order: Vec<&str> = app.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(
            order,
            [
                "waiting-old",
                "waiting-new",
                "error",
                "working",
                "done",
                "stale"
            ]
        );
    }

    #[test]
    fn selection_follows_session_across_updates() {
        let mut app = empty_app();
        app.update_sessions(vec![
            session("a", SessionState::WaitingInput, 1),
            session("b", SessionState::Working, 2),
        ]);
        app.selected = 1; // "b"
        // "a" finishes; "b" now needs input and jumps to the top.
        app.update_sessions(vec![
            session("a", SessionState::Done, 3),
            session("b", SessionState::WaitingInput, 4),
        ]);
        assert_eq!(app.sessions[app.selected].session_id, "b");
    }

    #[test]
    fn selection_clamps_when_sessions_shrink() {
        let mut app = empty_app();
        app.update_sessions(vec![
            session("a", SessionState::Working, 1),
            session("b", SessionState::Working, 2),
        ]);
        app.selected = 1;
        app.update_sessions(vec![session("a", SessionState::Working, 3)]);
        assert_eq!(app.selected, 0);
        app.update_sessions(Vec::new());
        assert_eq!(app.selected, 0);
    }
}
