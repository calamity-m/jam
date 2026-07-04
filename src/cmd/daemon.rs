//! CLI entry point for `jam daemon`.

use std::process::ExitCode;

pub fn run() -> ExitCode {
    match crate::daemon::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jam daemon: {e}");
            ExitCode::FAILURE
        }
    }
}
