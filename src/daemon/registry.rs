//! In-memory session registry: the daemon's only state.

use crate::proto::{Event, EventKind, Session, SessionState};
use std::collections::HashMap;

#[derive(Default)]
pub struct Registry {
    sessions: HashMap<String, Session>,
}

impl Registry {
    /// Apply one event. Returns true if the registry changed.
    pub fn apply(&mut self, event: &Event, now: u64) -> bool {
        if event.event == EventKind::End {
            return self.sessions.remove(&event.session_id).is_some();
        }
        let state = match event.event {
            EventKind::Start | EventKind::Working => SessionState::Working,
            EventKind::WaitingInput => SessionState::WaitingInput,
            EventKind::Done => SessionState::Done,
            EventKind::Error => SessionState::Error,
            EventKind::End => unreachable!(),
        };
        let entry = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| Session {
                session_id: event.session_id.clone(),
                agent: event.agent,
                state,
                title: None,
                cwd: None,
                mux: None,
                mux_session: None,
                pane_ref: None,
                last_event: event.event,
                last_event_at: now,
            });
        entry.agent = event.agent;
        entry.state = state;
        entry.last_event = event.event;
        entry.last_event_at = now;
        // Optional fields only overwrite when the event carries them, so a
        // later hook that lacks e.g. pane_ref does not erase what we know.
        if event.title.is_some() {
            entry.title = event.title.clone();
        }
        if event.cwd.is_some() {
            entry.cwd = event.cwd.clone();
        }
        if event.mux.is_some() {
            entry.mux = event.mux;
        }
        if event.mux_session.is_some() {
            entry.mux_session = event.mux_session.clone();
        }
        if event.pane_ref.is_some() {
            entry.pane_ref = event.pane_ref.clone();
        }
        true
    }

    /// Mark sessions with no event within `timeout` seconds as stale.
    /// Returns true if any session changed.
    pub fn mark_stale(&mut self, now: u64, timeout: u64) -> bool {
        let mut changed = false;
        for session in self.sessions.values_mut() {
            if session.state != SessionState::Stale
                && now.saturating_sub(session.last_event_at) >= timeout
            {
                session.state = SessionState::Stale;
                changed = true;
            }
        }
        changed
    }

    /// Force a session stale (e.g. its pane is gone). Returns true on change.
    pub fn set_stale(&mut self, session_id: &str) -> bool {
        match self.sessions.get_mut(session_id) {
            Some(session) if session.state != SessionState::Stale => {
                session.state = SessionState::Stale;
                true
            }
            _ => false,
        }
    }

    /// Remove a session by id. Returns true if it existed.
    pub fn dismiss(&mut self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    pub fn snapshot(&self) -> Vec<Session> {
        let mut sessions: Vec<Session> = self.sessions.values().cloned().collect();
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Agent, Mux};

    fn event(session_id: &str, kind: EventKind) -> Event {
        Event {
            session_id: session_id.into(),
            agent: Agent::ClaudeCode,
            event: kind,
            title: None,
            cwd: None,
            mux: None,
            mux_session: None,
            pane_ref: None,
        }
    }

    #[test]
    fn start_creates_working_session() {
        let mut reg = Registry::default();
        assert!(reg.apply(&event("s1", EventKind::Start), 100));
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].state, SessionState::Working);
        assert_eq!(snap[0].last_event, EventKind::Start);
        assert_eq!(snap[0].last_event_at, 100);
    }

    #[test]
    fn end_removes_session() {
        let mut reg = Registry::default();
        reg.apply(&event("s1", EventKind::Start), 100);
        assert!(reg.apply(&event("s1", EventKind::End), 101));
        assert!(reg.snapshot().is_empty());
        // Ending an unknown session is a no-op, not a change.
        assert!(!reg.apply(&event("s2", EventKind::End), 102));
    }

    #[test]
    fn sparse_events_keep_earlier_optional_fields() {
        let mut reg = Registry::default();
        let mut first = event("s1", EventKind::Start);
        first.pane_ref = Some("%5".into());
        first.mux = Some(Mux::Tmux);
        first.mux_session = Some("work".into());
        reg.apply(&first, 100);
        reg.apply(&event("s1", EventKind::WaitingInput), 200);
        let snap = reg.snapshot();
        assert_eq!(snap[0].state, SessionState::WaitingInput);
        assert_eq!(snap[0].pane_ref.as_deref(), Some("%5"));
        assert_eq!(snap[0].mux, Some(Mux::Tmux));
        assert_eq!(snap[0].mux_session.as_deref(), Some("work"));
    }

    #[test]
    fn quiet_sessions_go_stale() {
        let mut reg = Registry::default();
        reg.apply(&event("s1", EventKind::Done), 100);
        assert!(!reg.mark_stale(150, 100));
        assert!(reg.mark_stale(200, 100));
        assert_eq!(reg.snapshot()[0].state, SessionState::Stale);
        // Already stale: no further change reported.
        assert!(!reg.mark_stale(300, 100));
    }

    #[test]
    fn dismiss_removes_session() {
        let mut reg = Registry::default();
        reg.apply(&event("s1", EventKind::Done), 100);
        assert!(reg.dismiss("s1"));
        assert!(!reg.dismiss("s1"));
        assert!(reg.snapshot().is_empty());
    }
}
