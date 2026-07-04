//! `jam notify`: the entire agent integration surface.
//!
//! Builds one normalized event from flags, environment, and the hook's
//! stdin JSON, sends it to the daemon, and exits quietly. It must be fast
//! and must never fail the calling hook, so every error path degrades to a
//! stderr note and a zero exit. Copy-paste snippets live under
//! `jam setup <agent> --dry`.

use crate::cmd::notify::NotifyArgs;
use crate::proto::{Event, Mux, Request};
use serde::Deserialize;
use std::io::{IsTerminal, Read, Write};

/// Fields jam understands from an agent hook's stdin payload (Claude Code
/// hooks pipe a JSON object with these, among others).
#[derive(Deserialize, Default)]
struct HookStdin {
    session_id: Option<String>,
    cwd: Option<String>,
}

pub fn run(args: NotifyArgs) {
    if let Err(msg) = send(args) {
        // Never fail the hook: report to stderr and exit 0 regardless.
        eprintln!("jam notify: {msg}");
    }
}

fn send(args: NotifyArgs) -> Result<(), String> {
    let stdin = read_hook_stdin();
    let Some(session_id) = args.session.or(stdin.session_id) else {
        return Err("no session id: pass --session or pipe hook JSON on stdin".into());
    };
    let cwd = args.cwd.or(stdin.cwd).or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string())
    });
    let (mux, mux_session, env_pane) = detect_mux();
    let event = Event {
        session_id,
        agent: args.agent,
        event: args.event,
        title: args.title,
        cwd,
        mux,
        mux_session,
        pane_ref: args.pane_ref.or(env_pane),
    };

    let mut stream = crate::daemon::connect_or_spawn().map_err(|e| e.to_string())?;
    let line = serde_json::to_string(&Request::Event(event)).map_err(|e| e.to_string())?;
    writeln!(stream, "{line}").map_err(|e| e.to_string())
}

/// Hooks pipe a JSON payload on stdin; interactive runs have a tty there.
/// Only read when something is actually piped, so manual use never blocks.
fn read_hook_stdin() -> HookStdin {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return HookStdin::default();
    }
    let mut buf = String::new();
    if stdin.read_to_string(&mut buf).is_err() {
        return HookStdin::default();
    }
    serde_json::from_str(&buf).unwrap_or_default()
}

/// Detect the surrounding multiplexer, its session, and the pane from the
/// hook's environment. If sessions are nested, tmux wins arbitrarily;
/// --pane-ref can override.
fn detect_mux() -> (Option<Mux>, Option<String>, Option<String>) {
    if std::env::var_os("TMUX").is_some() {
        // No session needed: tmux pane ids are unique across the server.
        return (Some(Mux::Tmux), None, std::env::var("TMUX_PANE").ok());
    }
    if std::env::var_os("ZELLIJ").is_some() {
        return (
            Some(Mux::Zellij),
            std::env::var("ZELLIJ_SESSION_NAME").ok(),
            std::env::var("ZELLIJ_PANE_ID").ok(),
        );
    }
    (None, None, None)
}
