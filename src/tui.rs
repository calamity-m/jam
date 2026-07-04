pub mod agent_pane;
pub mod event_loop;
pub mod header_line;
pub mod status_line;

use std::process::ExitCode;

/// `jam` with no args: the live monitor.
pub fn run() -> ExitCode {
    match event_loop::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jam: {e}");
            ExitCode::FAILURE
        }
    }
}
