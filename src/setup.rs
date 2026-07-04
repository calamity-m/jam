pub mod claude;
pub mod codex;
pub mod opencode;
pub mod pi;

/// Hook payloads embedded at build time from ./hooks (see build.rs).
pub mod assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// `--ask`: show what will be installed and where, then require consent.
pub fn confirm(target: &Path, payload: &str) -> Result<bool, String> {
    println!("jam setup will write to: {}", target.display());
    println!("--- payload ---\n{payload}\n---------------");
    print!("Continue? [y/N] ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "cannot resolve $HOME".to_string())
}
