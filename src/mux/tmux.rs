//! tmux focus backend.
//!
//! `pane_ref` is a server-unique pane id like `%5` ($TMUX_PANE), so targets
//! resolve across sessions. Focusing is three steps: make the pane active in
//! its window, make the window active in its session, then point the current
//! client at that session.

use super::{run, FocusError};

pub fn focus(pane_ref: &str) -> Result<(), FocusError> {
    // Verify the pane still exists so a moved/closed pane surfaces as
    // PaneGone rather than a confusing tmux error.
    let (ok, _) = run("tmux", &["display-message", "-p", "-t", pane_ref, "#{pane_id}"])?;
    if !ok {
        return Err(FocusError::PaneGone);
    }
    for step in [
        ["select-pane", "-t", pane_ref],
        ["select-window", "-t", pane_ref],
        ["switch-client", "-t", pane_ref],
    ] {
        let (ok, stderr) = run("tmux", &step)?;
        if !ok {
            // switch-client fails when jam runs outside tmux (no client to
            // move); the pane is still selected, which is the best we can do.
            if step[0] == "switch-client" && std::env::var_os("TMUX").is_none() {
                return Ok(());
            }
            return Err(FocusError::Failed(format!(
                "tmux {}: {}",
                step[0],
                stderr.trim()
            )));
        }
    }
    Ok(())
}
