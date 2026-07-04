//! Claude Code installer: non-destructively merges the embedded hooks
//! fragment into a settings.json.

use crate::cmd::setup::SetupArgs;
use crate::setup::assets;
use serde_json::{Map, Value};
use std::path::PathBuf;

pub fn run(args: &SetupArgs) -> Result<(), String> {
    let fragment = embedded_fragment()?;
    let target = target_path(args.local)?;
    let existing = read_settings(&target)?;
    let fragment: Value = serde_json::from_str(fragment).expect("embedded fragment is valid JSON");
    let plan = merge_hooks(existing, &fragment)?;
    if !plan.changed() {
        println!("jam hooks already installed in {}", target.display());
        return Ok(());
    }
    if args.dry {
        println!(
            "# Claude Code — jam setup claude-code would change {}:",
            target.display()
        );
        print!("{}", plan.describe());
        return Ok(());
    }
    if args.ask && !crate::setup::confirm(&target, &plan.describe())? {
        println!("aborted; nothing written");
        return Ok(());
    }
    install(&target, &plan.merged)
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

fn read_settings(target: &std::path::Path) -> Result<Value, String> {
    match std::fs::read_to_string(target) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                target.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(e) => Err(format!("cannot read {}: {e}", target.display())),
    }
}

fn install(target: &std::path::Path, merged: &Value) -> Result<(), String> {
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
    let pretty = serde_json::to_string_pretty(merged).expect("merged settings serialize");
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

/// Result of merging the fragment into existing settings: the merged value
/// plus what the merge removed and added, for `--dry`/`--ask` display.
struct MergePlan {
    merged: Value,
    removed: Vec<String>,
    added: Vec<String>,
}

impl MergePlan {
    fn changed(&self) -> bool {
        !self.removed.is_empty() || !self.added.is_empty()
    }

    fn describe(&self) -> String {
        let mut out = String::new();
        for r in &self.removed {
            out.push_str(&format!("- remove stale jam entry  {r}\n"));
        }
        for a in &self.added {
            out.push_str(&format!("+ add jam entry           {a}\n"));
        }
        out
    }
}

/// An entry is jam-owned when every one of its hook commands is a
/// `jam notify` invocation; mixed or foreign entries are never touched.
fn jam_owned(entry: &Value) -> bool {
    let Some(hooks) = entry.get("hooks").and_then(Value::as_array) else {
        return false;
    };
    !hooks.is_empty()
        && hooks.iter().all(|h| {
            h.get("command")
                .and_then(Value::as_str)
                .is_some_and(|c| c == "jam notify" || c.starts_with("jam notify "))
        })
}

fn describe_entry(event: &str, entry: &Value) -> String {
    let commands: Vec<&str> = entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks
                .iter()
                .filter_map(|h| h.get("command").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    format!("{event}: {}", commands.join("; "))
}

/// Merge the fragment's `hooks` object into `settings`. Jam-owned entries
/// that no longer match the fragment are removed (upgrade-in-place), then
/// missing fragment entries are appended. Entries not owned by jam and every
/// other key in `settings` are left untouched.
fn merge_hooks(mut settings: Value, fragment: &Value) -> Result<MergePlan, String> {
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

    let mut removed = Vec::new();
    let mut added = Vec::new();

    // Upgrade pass: drop jam-owned entries the current fragment no longer
    // ships, across every event key (including events jam stopped using).
    let mut emptied = Vec::new();
    for (event, entries) in hooks.iter_mut() {
        let Some(entries) = entries.as_array_mut() else {
            continue;
        };
        let current: &[Value] = fragment_hooks
            .get(event)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice);
        let before = entries.len();
        entries.retain(|entry| {
            let stale = jam_owned(entry) && !current.contains(entry);
            if stale {
                removed.push(describe_entry(event, entry));
            }
            !stale
        });
        if before > entries.len() && entries.is_empty() {
            emptied.push(event.clone());
        }
    }
    // Event keys the upgrade pass emptied would otherwise linger as `[]`
    // (pre-existing empty arrays under other keys are left alone).
    for event in emptied {
        hooks.remove(&event);
    }

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
                added.push(describe_entry(event, entry));
            }
        }
    }
    Ok(MergePlan {
        merged: settings,
        removed,
        added,
    })
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

    fn jam_entry(command: &str) -> Value {
        json!({ "hooks": [{ "type": "command", "command": command }] })
    }

    #[test]
    fn merge_into_empty_settings_adds_all_events() {
        let plan = merge_hooks(json!({}), &fragment()).unwrap();
        assert!(plan.changed());
        assert!(plan.removed.is_empty());
        let hooks = plan.merged["hooks"].as_object().unwrap();
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreCompact",
            "PostCompact",
            "Notification",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionDenied",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ] {
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
        let plan = merge_hooks(settings, &fragment()).unwrap();
        assert!(plan.changed());
        assert!(plan.removed.is_empty());
        assert_eq!(plan.merged["permissions"]["deny"][0], "Read(**/.env*)");
        assert_eq!(plan.merged["model"], "claude-fable-5");
        let session_start = plan.merged["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2, "existing entry kept, jam appended");
        assert_eq!(session_start[0]["hooks"][0]["command"], "existing.sh");
    }

    #[test]
    fn merge_is_idempotent() {
        let once = merge_hooks(json!({}), &fragment()).unwrap();
        assert!(once.changed());
        let twice = merge_hooks(once.merged.clone(), &fragment()).unwrap();
        assert!(!twice.changed());
        assert_eq!(once.merged, twice.merged);
    }

    #[test]
    fn merge_upgrades_stale_jam_entry_in_place() {
        // An old-fragment entry (no timeout, wrong event mapping) under an
        // event the current fragment still ships.
        let settings = json!({
            "hooks": {
                "Stop": [jam_entry("jam notify --agent claude-code --event done --title \"Old\"")]
            }
        });
        let plan = merge_hooks(settings, &fragment()).unwrap();
        assert_eq!(plan.removed.len(), 1, "stale jam entry dropped");
        let stop = plan.merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop, fragment()["hooks"]["Stop"].as_array().unwrap());
    }

    #[test]
    fn merge_removes_jam_entries_under_dropped_event_keys() {
        let settings = json!({
            "hooks": {
                "SubagentStop": [jam_entry("jam notify --agent claude-code --event done")]
            }
        });
        let plan = merge_hooks(settings, &fragment()).unwrap();
        assert!(plan.removed.iter().any(|r| r.starts_with("SubagentStop:")));
        assert!(
            plan.merged["hooks"].get("SubagentStop").is_none(),
            "emptied event key removed entirely"
        );
    }

    #[test]
    fn merge_preserves_non_jam_entries_interleaved_with_stale_jam_ones() {
        let settings = json!({
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "user-hook.sh" }] },
                    jam_entry("jam notify --agent claude-code --event done --stale"),
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "other.sh" }] }
                ],
                // Mixed jam + non-jam commands in one entry: user-crafted, not ours to touch.
                "SubagentStop": [{
                    "hooks": [
                        { "type": "command", "command": "jam notify --agent claude-code --event done" },
                        { "type": "command", "command": "cleanup.sh" }
                    ]
                }]
            }
        });
        let plan = merge_hooks(settings, &fragment()).unwrap();
        let stop = plan.merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"][0]["command"], "user-hook.sh");
        assert_eq!(stop[1]["matcher"], "*");
        assert!(stop.iter().all(|e| describe_entry("Stop", e)
            != "Stop: jam notify --agent claude-code --event done --stale"));
        assert_eq!(
            plan.merged["hooks"]["SubagentStop"]
                .as_array()
                .unwrap()
                .len(),
            1,
            "mixed entry left untouched"
        );
    }

    #[test]
    fn upgrade_from_stale_install_is_idempotent() {
        let stale = json!({
            "hooks": {
                "PostCompact": [jam_entry("jam notify --agent claude-code --event done --title \"Compacted\"")],
                "Notification": [jam_entry("jam notify --agent claude-code --event waiting_input")]
            }
        });
        let once = merge_hooks(stale, &fragment()).unwrap();
        assert!(once.changed());
        assert!(!once.removed.is_empty());
        let twice = merge_hooks(once.merged.clone(), &fragment()).unwrap();
        assert!(!twice.changed());
        assert_eq!(once.merged, twice.merged);
    }

    #[test]
    fn merge_refuses_malformed_hooks_key() {
        let settings = json!({ "hooks": "not an object" });
        assert!(merge_hooks(settings, &fragment()).is_err());
    }
}
