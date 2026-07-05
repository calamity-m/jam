//! Shared non-destructive installer for agents configured by JSON hooks.

use serde_json::{Map, Value};
use std::path::Path;

/// A merged document and the jam-owned entries changed to produce it.
pub(super) struct MergePlan {
    pub(super) merged: Value,
    removed: Vec<String>,
    added: Vec<String>,
}

impl MergePlan {
    pub(super) fn changed(&self) -> bool {
        !self.removed.is_empty() || !self.added.is_empty()
    }

    pub(super) fn describe(&self) -> String {
        let mut out = String::new();
        for removed in &self.removed {
            out.push_str(&format!("- remove stale jam entry  {removed}\n"));
        }
        for added in &self.added {
            out.push_str(&format!("+ add jam entry           {added}\n"));
        }
        out
    }
}

/// Read a JSON hook file, returning an empty object when it does not exist.
///
/// Invalid JSON is rejected before any installation work begins so a user's
/// existing configuration is never replaced with a partially merged file.
pub(super) fn read(target: &Path) -> Result<Value, String> {
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

/// Write merged hooks while keeping the original recoverable during replacement.
///
/// Missing parent directories are created. Existing files are copied to a
/// temporary `.json.jam-bak` sibling before replacement; the backup is kept if
/// the write fails and removed only after the new file is complete.
pub(super) fn install(target: &Path, merged: &Value) -> Result<(), String> {
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Keep the original recoverable until its replacement is fully written.
    let backup = target.with_extension("json.jam-bak");
    let had_original = target.exists();
    if had_original {
        std::fs::copy(target, &backup).map_err(|e| format!("backup failed: {e}"))?;
    }
    // Serialize through serde to guarantee valid, consistently formatted JSON.
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

/// Merge a fragment's `hooks` object into an existing JSON document.
///
/// Jam-owned matcher groups that no longer match the fragment are removed,
/// while foreign groups and unrelated root keys remain untouched.
pub(super) fn merge(mut existing: Value, fragment: &Value) -> Result<MergePlan, String> {
    let Some(fragment_hooks) = fragment.get("hooks").and_then(Value::as_object) else {
        return Err("embedded fragment has no hooks object".into());
    };
    if !existing.is_object() {
        return Err("settings root is not a JSON object; not touching it".into());
    }
    let hooks = existing
        .as_object_mut()
        .expect("checked object above")
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("existing \"hooks\" key is not an object; not touching it".into());
    };

    let mut removed = Vec::new();
    let mut added = Vec::new();
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
    for event in emptied {
        hooks.remove(&event);
    }

    for (event, entries) in fragment_hooks {
        let Some(entries) = entries.as_array() else {
            continue;
        };
        let existing_entries = hooks
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(existing_entries) = existing_entries.as_array_mut() else {
            return Err(format!(
                "existing hooks.{event} is not an array; not touching the file"
            ));
        };
        for entry in entries {
            if !existing_entries.contains(entry) {
                existing_entries.push(entry.clone());
                added.push(describe_entry(event, entry));
            }
        }
    }

    Ok(MergePlan {
        merged: existing,
        removed,
        added,
    })
}

fn jam_owned(entry: &Value) -> bool {
    let Some(hooks) = entry.get("hooks").and_then(Value::as_array) else {
        return false;
    };
    !hooks.is_empty()
        && hooks.iter().all(|hook| {
            hook.get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| {
                    command == "jam notify" || command.starts_with("jam notify ")
                })
        })
}

fn describe_entry(event: &str, entry: &Value) -> String {
    let commands: Vec<&str> = entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks
                .iter()
                .filter_map(|hook| hook.get("command").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    format!("{event}: {}", commands.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn entry(command: &str) -> Value {
        json!({ "hooks": [{ "type": "command", "command": command }] })
    }

    fn fragment() -> Value {
        json!({ "hooks": { "Stop": [entry("jam notify --agent test --event done")] } })
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "jam-json-hooks-{name}-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn merge_preserves_foreign_entries_and_unrelated_keys() {
        let existing = json!({
            "model": "custom",
            "hooks": {
                "Stop": [
                    entry("user-hook.sh"),
                    entry("jam notify --agent test --event done --stale")
                ],
                "Other": [{ "hooks": [entry("jam notify")["hooks"][0].clone(), entry("cleanup")["hooks"][0].clone()] }]
            }
        });
        let plan = merge(existing, &fragment()).unwrap();
        assert_eq!(plan.merged["model"], "custom");
        assert_eq!(plan.merged["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(plan.merged["hooks"]["Other"].as_array().unwrap().len(), 1);
        assert!(plan.describe().contains("remove stale jam entry"));
    }

    #[test]
    fn merge_is_idempotent_and_removes_dropped_jam_events() {
        let existing = json!({ "hooks": { "OldEvent": [entry("jam notify --event end")] } });
        let once = merge(existing, &fragment()).unwrap();
        assert!(once.changed());
        assert!(once.merged["hooks"].get("OldEvent").is_none());
        let twice = merge(once.merged.clone(), &fragment()).unwrap();
        assert!(!twice.changed());
        assert_eq!(once.merged, twice.merged);
    }

    #[test]
    fn merge_refuses_invalid_shapes() {
        assert!(merge(json!([]), &fragment()).is_err());
        assert!(merge(json!({ "hooks": "bad" }), &fragment()).is_err());
        assert!(merge(json!({}), &json!({})).is_err());
    }

    #[test]
    fn read_and_install_round_trip_and_remove_backup() {
        let target = temp_file("round-trip");
        assert_eq!(read(&target).unwrap(), json!({}));
        std::fs::write(&target, "{\"old\":true}").unwrap();
        let merged = json!({ "hooks": {} });
        install(&target, &merged).unwrap();
        assert_eq!(read(&target).unwrap(), merged);
        assert!(!target.with_extension("json.jam-bak").exists());
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn install_creates_missing_parent_directories() {
        let root = temp_file("missing-parent");
        let target = root.join(".codex/hooks.json");
        install(&target, &json!({ "hooks": {} })).unwrap();
        assert_eq!(read(&target).unwrap(), json!({ "hooks": {} }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn read_refuses_malformed_json() {
        let target = temp_file("malformed");
        std::fs::write(&target, "not json").unwrap();
        assert!(read(&target).unwrap_err().contains("not valid JSON"));
        let _ = std::fs::remove_file(target);
    }
}
