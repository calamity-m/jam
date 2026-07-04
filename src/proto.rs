//! Shared event model and socket wire protocol.
//!
//! Everything that crosses the Unix socket between `jam notify`, the daemon,
//! and TUI clients lives here: the normalized agent event, the daemon's
//! per-session snapshot, and the request/response messages. The wire format
//! is newline-delimited JSON, one message per line.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    Pi,
    ClaudeCode,
    Codex,
    Opencode,
    /// Anything wired up by hand with `jam notify` that has no dedicated
    /// kind; jam only uses this as a display label.
    Custom,
}

/// Normalized event names shared by every agent integration. Agents emit
/// only the subset their hooks can express — thin adapter logic (or the
/// hook configuration itself) maps native events onto these; there is no
/// per-agent capability negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")] // CLI names match the wire/spec names
pub enum EventKind {
    Start,
    Working,
    WaitingInput,
    Done,
    Error,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mux {
    Tmux,
    Zellij,
}

/// One agent event, as sent by `jam notify` to the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// The *agent's* session id (from its hook environment), not anything
    /// multiplexer-related; the multiplexer side lives in `mux_session`.
    pub session_id: String,
    pub agent: Agent,
    pub event: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mux: Option<Mux>,
    /// Multiplexer session holding the pane. Zellij needs it because pane
    /// ids are only unique per session; tmux pane ids are server-global.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mux_session: Option<String>,
    /// Pane id within the multiplexer: `$TMUX_PANE` / `$ZELLIJ_PANE_ID`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_ref: Option<String>,
}

/// Session status as tracked by the daemon, derived from the last event:
/// `start`/`working` → `working`, the rest map one-to-one, and `end`
/// removes the session. `stale` is the exception — the daemon sets it
/// itself on event timeout or when a focus attempt finds the pane gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Working,
    WaitingInput,
    Done,
    Error,
    Stale,
}

/// The daemon's last known state for one agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub agent: Agent,
    pub state: SessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mux: Option<Mux>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mux_session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_ref: Option<String>,
    pub last_event: EventKind,
    /// Unix epoch seconds.
    pub last_event_at: u64,
}

/// Client → daemon messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Record an agent event. Fire-and-forget; no response.
    Event(Event),
    /// Reply with one `Response::Sessions` and close nothing else.
    Snapshot,
    /// Reply with `Response::Sessions` now and again on every change.
    Subscribe,
    /// Drop a session from the registry (TUI `x` key).
    Dismiss { session_id: String },
    /// Force a session stale (its pane was verified missing at focus time).
    MarkStale { session_id: String },
}

/// Daemon → client messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Sessions { sessions: Vec<Session> },
}

/// Socket location: `$JAM_SOCKET` override, else `$XDG_RUNTIME_DIR/jam.sock`,
/// else a per-user path under /tmp.
pub fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("JAM_SOCKET") {
        return PathBuf::from(p);
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("jam.sock");
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "default".into());
    PathBuf::from(format!("/tmp/jam-{user}.sock"))
}

/// Current time as unix epoch seconds, the timestamp unit used on the wire.
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_round_trips_with_all_fields() {
        let event = Event {
            session_id: "abc".into(),
            agent: Agent::ClaudeCode,
            event: EventKind::WaitingInput,
            title: Some("fix auth bug".into()),
            cwd: Some("/home/x/code/api".into()),
            mux: Some(Mux::Tmux),
            mux_session: None,
            pane_ref: Some("%5".into()),
        };
        let json = serde_json::to_string(&Request::Event(event.clone())).unwrap();
        assert!(json.contains(r#""agent":"claude-code""#));
        assert!(json.contains(r#""event":"waiting_input""#));
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Request::Event(event));
    }

    #[test]
    fn event_optional_fields_default_to_none() {
        let json = r#"{"type":"event","session_id":"s1","agent":"pi","event":"done"}"#;
        let Request::Event(event) = serde_json::from_str(json).unwrap() else {
            panic!("expected event request");
        };
        assert_eq!(event.agent, Agent::Pi);
        assert_eq!(event.event, EventKind::Done);
        assert_eq!(event.title, None);
        assert_eq!(event.mux, None);
    }

    #[test]
    fn requests_are_tagged_by_type() {
        assert_eq!(
            serde_json::to_string(&Request::Subscribe).unwrap(),
            r#"{"type":"subscribe"}"#
        );
        let dismiss: Request =
            serde_json::from_str(r#"{"type":"dismiss","session_id":"s1"}"#).unwrap();
        assert_eq!(
            dismiss,
            Request::Dismiss {
                session_id: "s1".into()
            }
        );
    }
}
