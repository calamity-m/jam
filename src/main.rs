use clap::{Parser, Subcommand};
use jam::cmd;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "jam", version, about = "Agent overview for your multiplexer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the bulletin-board daemon (usually auto-started on demand).
    Daemon,
    /// Send one agent event to the daemon; the hook-facing command.
    Notify(cmd::notify::NotifyArgs),
    /// Install notification hooks for a supported agent.
    Setup(cmd::setup::SetupArgs),
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Command::Daemon) => cmd::daemon::run(),
        Some(Command::Notify(args)) => cmd::notify::run(args),
        Some(Command::Setup(args)) => cmd::setup::run(args),
        None => jam::tui::run(),
    }
}
