//! CLI surface for `jam notify`.

use crate::proto::{Agent, EventKind};
use clap::Args;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct NotifyArgs {
    /// Stable per-agent session id. Read from the hook's stdin JSON if omitted.
    #[arg(long)]
    pub session: Option<String>,

    /// Which agent this event is about.
    #[arg(long, value_enum)]
    pub agent: Agent,

    /// Normalized event name.
    #[arg(long, value_enum)]
    pub event: EventKind,

    /// Optional human label (e.g. current task or prompt snippet).
    #[arg(long)]
    pub title: Option<String>,

    /// Agent working directory. Defaults to stdin JSON, then the current dir.
    #[arg(long)]
    pub cwd: Option<String>,

    /// Pane reference. Defaults to $TMUX_PANE / $ZELLIJ_PANE_ID.
    #[arg(long)]
    pub pane_ref: Option<String>,
}

pub fn run(args: NotifyArgs) -> ExitCode {
    // Hooks must never be failed by jam; notify always exits 0.
    crate::notify::run(args);
    ExitCode::SUCCESS
}
