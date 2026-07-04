//! CLI surface for `jam setup <agent>`.

use clap::Args;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Agent to install notification hooks for.
    #[arg(value_enum)]
    pub agent: SetupAgent,

    /// Print the hook payload and target for copy-paste setup; write nothing.
    #[arg(long)]
    pub dry: bool,

    /// Install into the current directory's config instead of the user root
    /// (e.g. ./.claude/settings.local.json instead of ~/.claude/settings.json).
    #[arg(long)]
    pub local: bool,

    /// Show the payload and target, then ask for confirmation before writing.
    #[arg(long)]
    pub ask: bool,
}

/// MVP supports pi and claude-code; other agents follow the same recipe
/// once their hooks/<agent>/ payloads exist.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum SetupAgent {
    Pi,
    ClaudeCode,
}

pub fn run(args: SetupArgs) -> ExitCode {
    let result = match args.agent {
        SetupAgent::Pi => crate::setup::pi::run(&args),
        SetupAgent::ClaudeCode => crate::setup::claude::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jam setup: {e}");
            ExitCode::FAILURE
        }
    }
}
