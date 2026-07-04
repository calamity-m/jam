//! Claude Code installer: non-destructively merges the embedded hooks
//! fragment into a settings.json.

use crate::cmd::setup::SetupArgs;
use crate::setup::assets;
use serde_json::{Map, Value};
use std::path::PathBuf;

pub fn run(args: &SetupArgs) -> Result<(), String> {
    let fragment = embedded_fragment()?;
    let target = target_path(args.local)?;
    if args.dry {
        println!("# Claude Code — jam setup claude-code would merge this into {}:", target.display());
        println!("{fragment}");
        return Ok(());
    }
    if args.ask && !crate::setup::confirm(&target, fragment)? {
        println!("aborted; nothing written");
        return Ok(());
    }
    install(&target, fragment)
}

fn embedded_fragment() -> Result<&'static str, String> {
    assets::CLAUDE_CODE
        .iter()
        .find(|(name, _)| *name == "settings-fragment.json")
        .map(|(_, contents)| *contents)
        .ok_or_else(|| "no claude-code hook payload embedded in this build".into())
}

fn target_path(local: bool) -> Result<PathBuf, String> {
    if local {
        Ok(PathBuf::from(".claude/settings.local.json"))
    } else {
        Ok(crate::setup::home_dir()?.join(".claude/settings.json"))
    }
}

fn install(target: &std::path::Path, fragment: &str) -> Result<(), String> {
    let existing: Value = match std::fs::read_to_string(target) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} is not valid JSON ({e}); not touching it", target.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", target.display())),
    };
    let fragment: Value = serde_json::from_str(fragment).expect("embedded fragment is valid JSON");

    let (merged, changed) = merge_hooks(existing, &fragment)?;
    if !changed {
        println!("jam hooks already installed in {}", target.display());
        return Ok(());
    }

    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Transient safety net: keep a copy of the original while we rewrite,
    // remove it once the write has succeeded.
    let backup = target.with_extension("json.jam-bak");
    let had_original = target.exists();
    if had_original {
        std::fs::copy(target, &backup).map_err(|e| format!("backup failed: {e}"))?;
    }
    let pretty = serde_json::to_string_pretty(&merged).expect("merged settings serialize");
    std::fs::write(target, pretty + "\n").map_err(|e| {
        format!(
            "write failed: {e}{}",
            if had_original {
                format!("; original preserved at {}", backup.display())
            } else {
                String::new()
            }
        )
    })?;
    if had_original {
        let _ = std::fs::remove_file(&backup);
    }
    println!("installed jam hooks into {}", target.display());
    Ok(())
}

/// Merge the fragment's `hooks` object into `settings`, appending entries
/// that are not already present. Every other key in `settings` is left
/// untouched. Returns the merged value and whether anything changed.
fn merge_hooks(mut settings: Value, fragment: &Value) -> Result<(Value, bool), String> {
    let Some(fragment_hooks) = fragment.get("hooks").and_then(Value::as_object) else {
        return Err("embedded fragment has no hooks object".into());
    };
    if !settings.is_object() {
        return Err("settings root is not a JSON object; not touching it".into());
    }
    let hooks = settings
        .as_object_mut()
        .expect("checked object above")
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("existing \"hooks\" key is not an object; not touching it".into());
    };

    let mut changed = false;
    for (event, entries) in fragment_hooks {
        let Some(entries) = entries.as_array() else {
            continue;
        };
        let existing = hooks
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(existing) = existing.as_array_mut() else {
            return Err(format!(
                "existing hooks.{event} is not an array; not touching the file"
            ));
        };
        for entry in entries {
            if !existing.contains(entry) {
                existing.push(entry.clone());
                changed = true;
            }
        }
    }
    Ok((settings, changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fragment() -> Value {
        serde_json::from_str(
            assets::CLAUDE_CODE
                .iter()
                .find(|(n, _)| *n == "settings-fragment.json")
                .unwrap()
                .1,
        )
        .unwrap()
    }

    #[test]
    fn merge_into_empty_settings_adds_all_events() {
        let (merged, changed) = merge_hooks(json!({}), &fragment()).unwrap();
        assert!(changed);
        let hooks = merged["hooks"].as_object().unwrap();
        for event in ["SessionStart", "UserPromptSubmit", "Notification", "Stop", "SessionEnd"] {
            assert!(hooks[event].is_array(), "missing {event}");
        }
    }

    #[test]
    fn merge_preserves_unrelated_keys_and_existing_hooks() {
        let settings = json!({
            "permissions": { "deny": ["Read(**/.env*)"] },
            "model": "claude-fable-5",
            "hooks": {
                "SessionStart": [
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "existing.sh" }] }
                ]
            }
        });
        let (merged, changed) = merge_hooks(settings, &fragment()).unwrap();
        assert!(changed);
        assert_eq!(merged["permissions"]["deny"][0], "Read(**/.env*)");
        assert_eq!(merged["model"], "claude-fable-5");
        let session_start = merged["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2, "existing entry kept, jam appended");
        assert_eq!(session_start[0]["hooks"][0]["command"], "existing.sh");
    }

    #[test]
    fn merge_is_idempotent() {
        let (once, changed) = merge_hooks(json!({}), &fragment()).unwrap();
        assert!(changed);
        let (twice, changed_again) = merge_hooks(once.clone(), &fragment()).unwrap();
        assert!(!changed_again);
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_refuses_malformed_hooks_key() {
        let settings = json!({ "hooks": "not an object" });
        assert!(merge_hooks(settings, &fragment()).is_err());
    }
}
