//! The TUI's central loop: subscribe to the daemon, redraw on updates,
//! handle keys. Rendering details live in the sibling modules.

use super::theme;
use crate::config::Settings;
use crate::mux::{self, FocusError};
use crate::proto::{Mux, Request, Response, Session, SessionState};
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

pub fn run(settings: &Settings) -> io::Result<()> {
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
    let result = event_loop(&mut terminal, &rx, &mut writer, settings);
    ratatui::restore(); // ALWAYS before self-close: closing our pane may SIGHUP us.
    if settings.close_pane_on_quit {
        mux::close_own_pane(); // best-effort, and necessarily the last action.
    }
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    rx: &mpsc::Receiver<Vec<Session>>,
    writer: &mut UnixStream,
    settings: &Settings,
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
            KeyCode::Enter => {
                if focus_selected(&mut app, writer)? && settings.quit_on_focus {
                    return Ok(());
                }
            }
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
///
/// Returns `true` only when the focus both succeeded *and* actually landed
/// the user on the target (the landing rule) — the caller uses that to decide
/// whether quit-on-focus should exit. Focus failures and cross-session zellij
/// rows return `false` and leave a status message, keeping the TUI open.
fn focus_selected(app: &mut App, writer: &mut UnixStream) -> io::Result<bool> {
    let Some(session) = app.sessions.get(app.selected) else {
        return Ok(false);
    };
    let (Some(mux), Some(pane_ref)) = (session.mux, session.pane_ref.as_deref()) else {
        app.message = Some("no pane recorded for this session".into());
        return Ok(false);
    };
    let row_session = session.mux_session.clone();
    match mux::focus(mux, row_session.as_deref(), pane_ref) {
        Ok(()) => {
            let own_session = std::env::var("ZELLIJ_SESSION_NAME").ok();
            if landed(mux, row_session.as_deref(), own_session.as_deref()) {
                app.message = None;
                return Ok(true);
            }
            // zellij cross-session: focus succeeded but the client can't move
            // there, so quitting would strand the user. Stay open with a note.
            let name = row_session.as_deref().unwrap_or("other");
            app.message = Some(format!("focused in zellij session '{name}'"));
        }
        Err(FocusError::PaneGone) => {
            let session_id = session.session_id.clone();
            app.message = Some("pane is gone; session marked stale".into());
            send(writer, &Request::MarkStale { session_id })?;
        }
        Err(FocusError::Failed(msg)) => app.message = Some(msg),
    }
    Ok(false)
}

/// Whether a successful focus actually lands the viewer's client on the
/// target. tmux always lands (`switch-client` moves the client). zellij lands
/// only within the caller's own session — it has no cross-session
/// `switch-client`, so a row in a different session focuses but cannot pull
/// the client over. `row_session == None` means the backend targeted the
/// current session, which is by definition our own.
fn landed(mux: Mux, row_session: Option<&str>, own_session: Option<&str>) -> bool {
    match mux {
        Mux::Tmux => true,
        Mux::Zellij => match row_session {
            None => true,
            Some(row) => own_session == Some(row),
        },
    }
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
    fn tmux_always_lands() {
        assert!(landed(Mux::Tmux, None, None));
        assert!(landed(Mux::Tmux, Some("a"), Some("b")));
    }

    #[test]
    fn zellij_lands_within_own_session() {
        // None row session => backend targeted the current (our own) session.
        assert!(landed(Mux::Zellij, None, Some("work")));
        // Same named session lands.
        assert!(landed(Mux::Zellij, Some("work"), Some("work")));
    }

    #[test]
    fn zellij_does_not_land_cross_session() {
        assert!(!landed(Mux::Zellij, Some("other"), Some("work")));
        // Own session unknown (jam not in zellij): a named row can't land.
        assert!(!landed(Mux::Zellij, Some("other"), None));
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
