//! User configuration: ~/.config/jam/config.toml plus CLI overrides.
//!
//! jam's first (and, deliberately, only) config file. Precedence is
//! flag > file > default-off. The file is parsed leniently — unknown keys
//! are ignored so an older binary tolerates a newer config — but a malformed
//! file or wrong value type fails fast (see `load`), because the user just
//! edited it and wants to know about a typo rather than have it silently
//! ignored.

use serde::Deserialize;
use std::path::PathBuf;

/// The on-disk schema. `Option` fields mean "unset in the file", so the
/// precedence merge in `resolve` can tell "not configured" from `false`.
#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub tui: TuiConfig,
}

/// The `[tui]` table. Doubles as the CLI-override carrier: main.rs builds one
/// of these from the clap flags, so each new key is declared once, not twice.
#[derive(Deserialize, Default)]
pub struct TuiConfig {
    pub quit_on_focus: Option<bool>,
    pub close_pane_on_quit: Option<bool>,
}

/// The resolved settings the TUI consumes, after file + flags + defaults.
pub struct Settings {
    pub quit_on_focus: bool,
    pub close_pane_on_quit: bool,
}

/// Resolve the config file path: `$XDG_CONFIG_HOME/jam/config.toml` when
/// `XDG_CONFIG_HOME` is set, else `$HOME/.config/jam/config.toml`. `None`
/// when neither is set (no discoverable home — treated as "no config"). Env
/// is injected so this stays pure and unit-testable, matching
/// `mux::detect_from` / `proto::socket_path` style.
fn config_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(xdg) = env("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(xdg).join("jam").join("config.toml"));
    }
    let home = env("HOME").filter(|s| !s.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("jam")
            .join("config.toml"),
    )
}

/// Load the config file. A missing file (or no discoverable home) yields
/// defaults; a malformed file or wrong value type is a fatal error whose
/// message names the path so the caller can print it and exit.
pub fn load() -> Result<Config, String> {
    let Some(path) = config_path(|name| std::env::var(name).ok()) else {
        return Ok(Config::default());
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(format!("config {}: {e}", path.display())),
    };
    toml::from_str(&text).map_err(|e| format!("config {}: {e}", path.display()))
}

/// Precedence merge: CLI flag beats file value beats the default (off).
pub fn resolve(config: Config, cli: TuiConfig) -> Settings {
    Settings {
        quit_on_focus: cli
            .quit_on_focus
            .or(config.tui.quit_on_focus)
            .unwrap_or(false),
        close_pane_on_quit: cli
            .close_pane_on_quit
            .or(config.tui.close_pane_on_quit)
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn config_path_prefers_xdg_config_home() {
        let env = env_of(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/home/u")]);
        assert_eq!(config_path(env), Some(PathBuf::from("/x/jam/config.toml")));
    }

    #[test]
    fn config_path_falls_back_to_home() {
        let env = env_of(&[("HOME", "/home/u")]);
        assert_eq!(
            config_path(env),
            Some(PathBuf::from("/home/u/.config/jam/config.toml"))
        );
    }

    #[test]
    fn config_path_none_without_home_or_xdg() {
        assert_eq!(config_path(env_of(&[])), None);
        // Empty strings are treated as unset.
        assert_eq!(
            config_path(env_of(&[("XDG_CONFIG_HOME", ""), ("HOME", "")])),
            None
        );
    }

    #[test]
    fn minimal_tui_file_parses() {
        let cfg: Config = toml::from_str("[tui]\nquit_on_focus = true\n").unwrap();
        assert_eq!(cfg.tui.quit_on_focus, Some(true));
        assert_eq!(cfg.tui.close_pane_on_quit, None);
    }

    #[test]
    fn empty_file_is_all_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.tui.quit_on_focus, None);
        assert_eq!(cfg.tui.close_pane_on_quit, None);
    }

    #[test]
    fn unknown_keys_and_tables_are_ignored() {
        let cfg: Config =
            toml::from_str("[tui]\nquit_on_focus = true\nfuture_key = 3\n\n[daemon]\nx = 1\n")
                .unwrap();
        assert_eq!(cfg.tui.quit_on_focus, Some(true));
    }

    #[test]
    fn wrong_value_type_is_an_error() {
        assert!(toml::from_str::<Config>("[tui]\nquit_on_focus = \"yes\"\n").is_err());
    }

    #[test]
    fn resolve_flag_beats_file_beats_default() {
        // Flag wins in both directions over the file.
        let file = TuiConfig {
            quit_on_focus: Some(true),
            close_pane_on_quit: Some(false),
        };
        let cli = TuiConfig {
            quit_on_focus: Some(false),
            close_pane_on_quit: Some(true),
        };
        let s = resolve(
            Config {
                tui: TuiConfig {
                    quit_on_focus: file.quit_on_focus,
                    close_pane_on_quit: file.close_pane_on_quit,
                },
            },
            cli,
        );
        assert!(!s.quit_on_focus);
        assert!(s.close_pane_on_quit);
    }

    #[test]
    fn resolve_file_used_when_no_flag() {
        let s = resolve(
            Config {
                tui: TuiConfig {
                    quit_on_focus: Some(true),
                    close_pane_on_quit: None,
                },
            },
            TuiConfig::default(),
        );
        assert!(s.quit_on_focus);
        assert!(!s.close_pane_on_quit); // unset file + no flag => default off
    }

    #[test]
    fn resolve_defaults_to_off() {
        let s = resolve(Config::default(), TuiConfig::default());
        assert!(!s.quit_on_focus);
        assert!(!s.close_pane_on_quit);
    }
}
