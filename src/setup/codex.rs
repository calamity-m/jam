//! Codex installer: non-destructively merges embedded hooks into hooks.json.

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
            "# Codex — jam setup codex would change {}:",
            target.display()
        );
        print!("{}", plan.describe());
        return Ok(());
    }
    if args.ask && !crate::setup::confirm(&target, &plan.describe())? {
        println!("aborted; nothing written");
        return Ok(());
    }
    json_hooks::install(&target, &plan.merged)?;
    println!("Review and trust the installed hooks with Codex's /hooks command.");
    Ok(())
}

fn embedded_fragment() -> Result<&'static str, String> {
    assets::CODEX
        .iter()
        .find(|(name, _)| *name == "hooks-fragment.json")
        .map(|(_, contents)| *contents)
        .ok_or_else(|| "no codex hook payload embedded in this build".into())
}

fn target_path(local: bool) -> Result<PathBuf, String> {
    if local {
        Ok(PathBuf::from(".codex/hooks.json"))
    } else {
        Ok(crate::setup::home_dir()?.join(".codex/hooks.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fragment() -> Value {
        serde_json::from_str(embedded_fragment().unwrap()).unwrap()
    }

    #[test]
    fn payload_maps_only_supported_codex_events() {
        let fragment = fragment();
        let hooks = fragment["hooks"].as_object().unwrap();
        let expected = [
            "SessionStart",
            "UserPromptSubmit",
            "PreCompact",
            "PostCompact",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
        ];
        assert_eq!(hooks.len(), expected.len());
        for event in expected {
            assert!(hooks[event].is_array(), "missing {event}");
        }
        assert_eq!(hooks["SessionStart"][0]["matcher"], "startup|resume|clear");
        let serialized = serde_json::to_string(&fragment).unwrap();
        assert!(!serialized.contains("--event error"));
        assert!(!serialized.contains("--event end"));
        assert!(serialized.contains("--event waiting_input"));
    }

    #[test]
    fn uses_codex_target_paths() {
        assert_eq!(
            target_path(true).unwrap(),
            PathBuf::from(".codex/hooks.json")
        );
        let global = target_path(false).unwrap();
        assert!(global.ends_with(".codex/hooks.json"));
        assert!(global.is_absolute());
    }

    #[test]
    fn payload_merges_through_shared_installer() {
        let plan = json_hooks::merge(json!({ "model": "custom" }), &fragment()).unwrap();
        assert!(plan.changed());
        assert_eq!(plan.merged["model"], "custom");
        assert!(plan.merged["hooks"]["Stop"].is_array());
    }
}
