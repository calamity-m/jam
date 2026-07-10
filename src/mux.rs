//! Multiplexer backends: the only multiplexer-aware code.
//!
//! Detects the surrounding multiplexer from the environment and dispatches
//! two pane operations to the backends — focus (bring a recorded pane into
//! view) and close. Everything else in jam is multiplexer-agnostic.

pub mod tmux;
pub mod zellij;

use crate::proto::Mux;

/// Detect the surrounding multiplexer, its session, and the pane from the
/// process environment. If sessions are nested, tmux wins arbitrarily.
pub fn detect() -> (Option<Mux>, Option<String>, Option<String>) {
    detect_from(|name| std::env::var(name).ok())
}

/// Some hosts (Claude Code's session workers) strip the `TMUX`/`ZELLIJ`
/// marker variables while the pane variables survive, so detection keys
/// on either.
fn detect_from(
    env: impl Fn(&str) -> Option<String>,
) -> (Option<Mux>, Option<String>, Option<String>) {
    let tmux_pane = env("TMUX_PANE");
    if env("TMUX").is_some() || tmux_pane.is_some() {
        // No session needed: tmux pane ids are unique across the server.
        return (Some(Mux::Tmux), None, tmux_pane);
    }
    let zellij_pane = env("ZELLIJ_PANE_ID");
    if env("ZELLIJ").is_some() || zellij_pane.is_some() {
        return (Some(Mux::Zellij), env("ZELLIJ_SESSION_NAME"), zellij_pane);
    }
    (None, None, None)
}

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

/// Close a pane by reference. Mirrors `focus`'s dispatch shape. A pane that
/// is already gone is the desired end state, so backends map "not found" to
/// `Ok(())`.
pub fn close(mux: Mux, mux_session: Option<&str>, pane_ref: &str) -> Result<(), FocusError> {
    match mux {
        Mux::Tmux => tmux::close(pane_ref),
        Mux::Zellij => zellij::close(mux_session, pane_ref),
    }
}

/// Best-effort: close jam's *own* multiplexer pane on exit (close-pane-on-quit).
/// No-op outside a multiplexer, and — deliberately stricter than `detect` —
/// only acts when the mux *marker* variable itself is set: `detect`'s
/// pane-var fallback exists for hook hosts that strip markers, but the TUI
/// always runs in a real interactive pane, and a stale inherited
/// `TMUX_PANE`/`ZELLIJ_PANE_ID` without its marker must never make jam kill
/// someone else's pane. The result is ignored: we are exiting, there is no UI
/// left to report to, and a lingering pane is merely the status quo.
pub fn close_own_pane() {
    let (Some(mux), mux_session, Some(pane_ref)) = detect() else {
        return;
    };
    let marker = match mux {
        Mux::Tmux => "TMUX",
        Mux::Zellij => "ZELLIJ",
    };
    if std::env::var_os(marker).is_none() {
        return;
    }
    let _ = close(mux, mux_session.as_deref(), &pane_ref);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn detect_finds_zellij_without_marker_var() {
        // Claude Code session workers strip ZELLIJ but keep the pane vars.
        let env = env_of(&[("ZELLIJ_SESSION_NAME", "work"), ("ZELLIJ_PANE_ID", "19")]);
        assert_eq!(
            detect_from(env),
            (Some(Mux::Zellij), Some("work".into()), Some("19".into()))
        );
    }

    #[test]
    fn detect_finds_tmux_without_marker_var() {
        let env = env_of(&[("TMUX_PANE", "%5")]);
        assert_eq!(detect_from(env), (Some(Mux::Tmux), None, Some("%5".into())));
    }

    #[test]
    fn detect_prefers_tmux_and_none_when_bare() {
        let both = env_of(&[("TMUX", "/tmp/t"), ("TMUX_PANE", "%5"), ("ZELLIJ", "0")]);
        assert_eq!(detect_from(both).0, Some(Mux::Tmux));
        assert_eq!(detect_from(env_of(&[])), (None, None, None));
    }
}
