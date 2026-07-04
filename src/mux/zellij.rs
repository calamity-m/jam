//! zellij focus backend.
//!
//! Plain zellij (>= 0.42) can focus a pane by id, including switching to the
//! tab that holds it, via `zellij --session <name> action focus-pane-id`.
//! No helper plugin is required; if one ever is, it slots in here behind the
//! same `focus` entry point.
//!
//! `pane_ref` is the bare pane id from $ZELLIJ_PANE_ID; the owning session
//! name arrives separately as `mux_session` ($ZELLIJ_SESSION_NAME), because
//! pane ids are only unique within a session. Without a session name the
//! command targets the current session.
//!
//! Constraint (of zellij itself, not of this MVP): focusing lands in the
//! right tab+pane of the target session, but zellij has no equivalent of
//! `tmux switch-client`, so it cannot re-attach the viewer's client to a
//! *different* zellij session.

use super::{run, FocusError};

pub fn focus(mux_session: Option<&str>, pane_ref: &str) -> Result<(), FocusError> {
    let pane_id = qualify_pane_id(pane_ref);
    let mut args: Vec<&str> = Vec::new();
    if let Some(session) = mux_session {
        args.extend(["--session", session]);
    }
    args.extend(["action", "focus-pane-id", &pane_id]);
    let (ok, stderr) = run("zellij", &args)?;
    // focus-pane-id exits non-zero for the harmless "already focused" case;
    // only the command's stderr distinguishes it from a missing pane.
    if ok || stderr.contains("already focused") {
        Ok(())
    } else if stderr.contains("not found") {
        Err(FocusError::PaneGone)
    } else {
        Err(FocusError::Failed(format!(
            "zellij focus-pane-id: {}",
            stderr.trim()
        )))
    }
}

/// Normalize a pane id to the `terminal_<n>` form focus-pane-id expects
/// ($ZELLIJ_PANE_ID is bare).
fn qualify_pane_id(pane_ref: &str) -> String {
    if pane_ref.starts_with("terminal_") || pane_ref.starts_with("plugin_") {
        pane_ref.to_string()
    } else {
        format!("terminal_{pane_ref}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_pane_ids_are_qualified() {
        assert_eq!(qualify_pane_id("3"), "terminal_3");
    }

    #[test]
    fn already_qualified_pane_ids_pass_through() {
        assert_eq!(qualify_pane_id("plugin_2"), "plugin_2");
        assert_eq!(qualify_pane_id("terminal_7"), "terminal_7");
    }

    /// Live test against a real zellij session; run inside zellij with
    /// `cargo test -- --ignored`. Focuses the current pane (a no-op) and a
    /// pane that cannot exist, so it never disturbs the user's layout.
    #[test]
    #[ignore = "requires running inside a zellij session"]
    fn live_focus_own_pane_and_missing_pane() {
        let session = std::env::var("ZELLIJ_SESSION_NAME").expect("run inside zellij");
        let pane = std::env::var("ZELLIJ_PANE_ID").expect("run inside zellij");
        assert_eq!(focus(Some(&session), &pane), Ok(()));
        assert_eq!(focus(Some(&session), "999999"), Err(FocusError::PaneGone));
    }
}
