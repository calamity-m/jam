//! Multiplexer focus backends: the only multiplexer-aware code.
//!
//! Each backend implements one operation — bring a recorded pane into view
//! and focus it. Everything else in jam is multiplexer-agnostic.

pub mod tmux;
pub mod zellij;

use crate::proto::Mux;

#[derive(Debug, PartialEq, Eq)]
pub enum FocusError {
    /// The recorded pane no longer exists; callers should mark the session
    /// stale (SPEC "session→pane mapping drift").
    PaneGone,
    Failed(String),
}

impl std::fmt::Display for FocusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusError::PaneGone => write!(f, "pane no longer exists"),
            FocusError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

/// Focus the pane recorded for a session. `mux_session` names the
/// multiplexer session holding the pane where the backend needs one
/// (zellij); tmux pane ids resolve server-wide without it.
pub fn focus(mux: Mux, mux_session: Option<&str>, pane_ref: &str) -> Result<(), FocusError> {
    match mux {
        Mux::Tmux => tmux::focus(pane_ref),
        Mux::Zellij => zellij::focus(mux_session, pane_ref),
    }
}

/// Run a multiplexer CLI command, capturing output instead of touching the
/// caller's terminal. Returns (exit_ok, stderr).
fn run(program: &str, args: &[&str]) -> Result<(bool, String), FocusError> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| FocusError::Failed(format!("failed to run {program}: {e}")))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}
