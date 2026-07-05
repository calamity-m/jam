//! Claude Code installer: non-destructively merges the embedded hooks
//! fragment into a settings.json.

use crate::cmd::setup::SetupArgs;
use crate::setup::{assets, json_hooks};
use serde_json::Value;
use std::path::PathBuf;

pub fn run(args: &SetupArgs) -> Result<(), String> {
    let fragment = embedded_fragment()?;
    let target = target_path(args.local)?;
    let existing = json_hooks::read(&target)?;
    let fragment: Value = serde_json::from_str(fragment).expect("embedded fragment is valid JSON");
    let plan = json_hooks::merge(existing, &fragment)?;
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
    json_hooks::install(&target, &plan.merged)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fragment() -> Value {
        serde_json::from_str(embedded_fragment().unwrap()).unwrap()
    }

    fn jam_entry(command: &str) -> Value {
        json!({ "hooks": [{ "type": "command", "command": command }] })
    }

    #[test]
    fn embedded_fragment_contains_all_claude_events() {
        let fragment = fragment();
        let hooks = fragment["hooks"].as_object().unwrap();
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
        let plan = json_hooks::merge(settings, &fragment()).unwrap();
        assert!(plan.changed());
        assert_eq!(plan.merged["permissions"]["deny"][0], "Read(**/.env*)");
        assert_eq!(plan.merged["model"], "claude-fable-5");
        let session_start = plan.merged["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2, "existing entry kept, jam appended");
        assert_eq!(session_start[0]["hooks"][0]["command"], "existing.sh");
    }

    #[test]
    fn merge_is_idempotent_for_claude_payload() {
        let once = json_hooks::merge(json!({}), &fragment()).unwrap();
        assert!(once.changed());
        let twice = json_hooks::merge(once.merged.clone(), &fragment()).unwrap();
        assert!(!twice.changed());
        assert_eq!(once.merged, twice.merged);
    }

    #[test]
    fn merge_upgrades_stale_claude_entry_in_place() {
        let settings = json!({
            "hooks": {
                "Stop": [jam_entry("jam notify --agent claude-code --event done --title \"Old\"")]
            }
        });
        let plan = json_hooks::merge(settings, &fragment()).unwrap();
        assert!(plan.describe().contains("remove stale jam entry"));
        let stop = plan.merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop, fragment()["hooks"]["Stop"].as_array().unwrap());
    }

    #[test]
    fn merge_removes_claude_entries_under_dropped_event_keys() {
        let settings = json!({
            "hooks": {
                "SubagentStop": [jam_entry("jam notify --agent claude-code --event done")]
            }
        });
        let plan = json_hooks::merge(settings, &fragment()).unwrap();
        assert!(plan.describe().contains("SubagentStop:"));
        assert!(plan.merged["hooks"].get("SubagentStop").is_none());
    }

    #[test]
    fn merge_preserves_mixed_and_foreign_claude_entries() {
        let settings = json!({
            "hooks": {
                "Stop": [
                    jam_entry("user-hook.sh"),
                    jam_entry("jam notify --agent claude-code --event done --stale"),
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "other.sh" }] }
                ],
                "SubagentStop": [{
                    "hooks": [
                        { "type": "command", "command": "jam notify --agent claude-code --event done" },
                        { "type": "command", "command": "cleanup.sh" }
                    ]
                }]
            }
        });
        let plan = json_hooks::merge(settings, &fragment()).unwrap();
        let stop = plan.merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"][0]["command"], "user-hook.sh");
        assert_eq!(stop[1]["matcher"], "*");
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
    fn stale_claude_upgrade_is_idempotent() {
        let stale = json!({
            "hooks": {
                "PostCompact": [jam_entry("jam notify --agent claude-code --event done --title \"Compacted\"")],
                "Notification": [jam_entry("jam notify --agent claude-code --event waiting_input")]
            }
        });
        let once = json_hooks::merge(stale, &fragment()).unwrap();
        assert!(once.changed());
        assert!(once.describe().contains("remove stale jam entry"));
        let twice = json_hooks::merge(once.merged.clone(), &fragment()).unwrap();
        assert!(!twice.changed());
        assert_eq!(once.merged, twice.merged);
    }

    #[test]
    fn merge_refuses_malformed_claude_hooks_key() {
        let settings = json!({ "hooks": "not an object" });
        assert!(json_hooks::merge(settings, &fragment()).is_err());
    }
}
