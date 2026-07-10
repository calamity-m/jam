//! `jam notify`: the entire agent integration surface.
//!
//! Builds one normalized event from flags, environment, and the hook's
//! stdin JSON, sends it to the daemon, and exits quietly. It must be fast
//! and must never fail the calling hook, so every error path degrades to a
//! stderr note and a zero exit. Copy-paste snippets live under
//! `jam setup <agent> --dry`.

use crate::cmd::notify::NotifyArgs;
use crate::proto::{Event, Request};
use serde::Deserialize;
use std::io::{IsTerminal, Read, Write};

/// Fields jam understands from an agent hook's stdin payload (Claude Code
/// hooks pipe a JSON object with these, among others).
#[derive(Deserialize, Default)]
struct HookStdin {
    session_id: Option<String>,
    cwd: Option<String>,
    /// Present in Claude Code and Codex payloads; pi and opencode do not
    /// pipe hook JSON, so it is never available there.
    prompt: Option<String>,
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
    let title = args.title.or_else(|| {
        args.title_from_prompt
            .then(|| stdin.prompt.as_deref().and_then(summarize_prompt))
            .flatten()
    });
    let (mux, mux_session, env_pane) = crate::mux::detect();
    let event = Event {
        session_id,
        agent: args.agent,
        event: args.event,
        title,
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

/// Collapse whitespace and truncate to 80 chars (77 + "..."). None for
/// empty/whitespace-only input so a blank prompt never blanks an
/// existing title.
fn summarize_prompt(prompt: &str) -> Option<String> {
    let collapsed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= 80 {
        return Some(collapsed);
    }
    let mut short: String = collapsed.chars().take(77).collect();
    short.push_str("...");
    Some(short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_prompt_passes_short_prompts_through() {
        assert_eq!(summarize_prompt("fix the bug"), Some("fix the bug".into()));
    }

    #[test]
    fn summarize_prompt_collapses_whitespace_and_newlines() {
        assert_eq!(summarize_prompt("a\n\n  b\tc"), Some("a b c".into()));
    }

    #[test]
    fn summarize_prompt_returns_none_for_empty_and_whitespace() {
        assert_eq!(summarize_prompt(""), None);
        assert_eq!(summarize_prompt("  \n\t "), None);
    }

    #[test]
    fn summarize_prompt_truncates_long_prompts_to_80_chars() {
        let long = "x".repeat(100);
        let short = summarize_prompt(&long).unwrap();
        assert_eq!(short.chars().count(), 80);
        assert!(short.ends_with("..."));
        assert!(short.starts_with(&"x".repeat(77)));
    }

    #[test]
    fn summarize_prompt_truncates_on_char_boundary() {
        let long = "日".repeat(100);
        let short = summarize_prompt(&long).unwrap();
        assert_eq!(short.chars().count(), 80);
        assert_eq!(short, format!("{}...", "日".repeat(77)));
    }
}
