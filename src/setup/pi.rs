//! Pi installer: generic plumbing that installs whatever files are shipped
//! in hooks/pi/ verbatim into pi's extensions directory — global
//! (~/.pi/agent/extensions/) or, with --local, project-local
//! (./.pi/extensions/). The extension content itself (a *.ts file) is
//! authored separately; hooks/pi is currently empty, so this reports that
//! instead of writing anything.

use crate::cmd::setup::SetupArgs;
use crate::setup::assets;
use std::path::PathBuf;

// Pi discovers extensions as *.ts (or */index.ts) under these directories;
// jam ships flat .ts files, so installing into the directory root suffices.
const GLOBAL_EXTENSIONS_DIR: &str = ".pi/agent/extensions";
const LOCAL_EXTENSIONS_DIR: &str = ".pi/extensions";

pub fn run(args: &SetupArgs) -> Result<(), String> {
    if assets::PI.is_empty() {
        println!(
            "no pi hook payload is shipped in this build yet (hooks/pi is empty).\n\
             In the meantime, call jam directly from pi's hook mechanism:\n\
             jam notify --agent pi --event <start|working|waiting_input|done|error|end> \\\n\
               --session \"<stable-session-id>\" [--title \"<label>\"]"
        );
        return Ok(());
    }
    let target_dir = target_dir(args.local)?;
    if args.dry {
        for (name, contents) in assets::PI {
            println!(
                "# Pi — jam setup pi would write {}:",
                target_dir.join(name).display()
            );
            println!("{contents}");
        }
        return Ok(());
    }
    if args.ask {
        let listing: Vec<String> = assets::PI
            .iter()
            .map(|(name, contents)| format!("=== {name} ===\n{contents}"))
            .collect();
        if !crate::setup::confirm(&target_dir, &listing.join("\n"))? {
            println!("aborted; nothing written");
            return Ok(());
        }
    }
    install(&target_dir)
}

fn target_dir(local: bool) -> Result<PathBuf, String> {
    if local {
        Ok(PathBuf::from(LOCAL_EXTENSIONS_DIR))
    } else {
        Ok(crate::setup::home_dir()?.join(GLOBAL_EXTENSIONS_DIR))
    }
}

/// Write each embedded file, never overwriting existing content that
/// differs: identical files are no-ops, differing ones are skipped loudly.
fn install(target_dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    for (name, contents) in assets::PI {
        let target = target_dir.join(name);
        match std::fs::read_to_string(&target) {
            Ok(existing) if existing == *contents => {
                println!("{} already installed", target.display());
            }
            Ok(_) => {
                println!(
                    "skipping {}: exists with different content; not overwriting",
                    target.display()
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::write(&target, contents).map_err(|e| e.to_string())?;
                println!("installed {}", target.display());
            }
            Err(e) => return Err(format!("cannot read {}: {e}", target.display())),
        }
    }
    Ok(())
}
