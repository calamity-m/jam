use clap::{Parser, Subcommand};
use jam::cmd;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "jam", version, about = "Agent overview for your multiplexer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Quit the TUI after Enter successfully focuses a pane.
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true,
        value_name = "BOOL"
    )]
    quit_on_focus: Option<bool>,
    /// Close jam's own multiplexer pane when the TUI exits.
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true,
        value_name = "BOOL"
    )]
    close_pane_on_quit: Option<bool>,
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
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Daemon) => cmd::daemon::run(),
        Some(Command::Notify(args)) => cmd::notify::run(args),
        Some(Command::Setup(args)) => cmd::setup::run(args),
        None => jam::tui::run(jam::config::TuiConfig {
            quit_on_focus: cli.quit_on_focus,
            close_pane_on_quit: cli.close_pane_on_quit,
        }),
    }
}
