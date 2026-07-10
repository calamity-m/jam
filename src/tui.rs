pub mod agent_pane;
pub mod event_loop;
pub mod header_line;
pub mod status_line;
pub mod theme;

use crate::config::{self, TuiConfig};
use std::process::ExitCode;

/// `jam` with no args: the live monitor. `cli` carries the flag overrides;
/// the config file is loaded and merged here (fail-fast on a malformed file,
/// before any terminal takeover).
pub fn run(cli: TuiConfig) -> ExitCode {
    let settings = match config::load() {
        Ok(cfg) => config::resolve(cfg, cli),
        Err(e) => {
            eprintln!("jam: {e}");
            return ExitCode::FAILURE;
        }
    };
    match event_loop::run(&settings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jam: {e}");
            ExitCode::FAILURE
        }
    }
}
