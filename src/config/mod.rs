mod keys;
mod theme;

use std::path::PathBuf;

use directories::ProjectDirs;
pub use keys::Keys;
pub use theme::Theme;

use crate::game::Mode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub player: String,
    pub mode: Mode,
    pub ghost_enabled: bool,
    pub next_queue_length: usize,
    pub keys: Keys,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            player: "PLAYER".into(),
            mode: Mode::default(),
            ghost_enabled: true,
            next_queue_length: 6,
            keys: Keys::default(),
            theme: Theme::default(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        validate_player(&self.player)?;
        if !(1..=6).contains(&self.next_queue_length) {
            return Err("next_queue_length must be between 1 and 6".into());
        }
        self.keys.validate()
    }

    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut entries = vec![
            ("player", self.player.clone()),
            ("mode", self.mode.to_string()),
            ("ghost_enabled", self.ghost_enabled.to_string()),
            ("next_queue_length", self.next_queue_length.to_string()),
        ];
        entries.extend(self.keys.entries());
        entries.extend(self.theme.entries());
        entries
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        match name {
            "player" => {
                validate_player(value)?;
                self.player = value.to_owned();
            }
            "mode" => self.mode = value.parse().map_err(|error| format!("{error}"))?,
            "ghost_enabled" => {
                self.ghost_enabled = value
                    .parse()
                    .map_err(|error| format!("{error}; use true or false"))?
            }
            "next_queue_length" => {
                let count = value
                    .parse()
                    .map_err(|error| format!("{error}; use a number from 1 to 6"))?;
                if !(1..=6).contains(&count) {
                    return Err("use a number from 1 to 6".into());
                }
                self.next_queue_length = count;
            }
            name if name.starts_with("keys.") => self.keys.set(name, value)?,
            name if name.starts_with("theme.") => self.theme.set(name, value)?,
            _ => return Err(format!("unknown setting {name}")),
        }
        Ok(())
    }
}

pub fn validate_player(player: &str) -> Result<(), String> {
    if player.trim().is_empty()
        || player.chars().count() > 16
        || player.chars().any(char::is_control)
    {
        return Err("player must contain 1–16 printable characters".into());
    }
    Ok(())
}

pub fn database_path(db: Option<PathBuf>) -> miette::Result<PathBuf> {
    db.or_else(|| {
        ProjectDirs::from("", "", "btetri").map(|dirs| dirs.data_local_dir().join("records.db"))
    })
    .ok_or_else(|| miette::miette!("cannot locate data directory; pass --db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_reject_invalid_values() {
        let config = Config::default();
        let mut restored = Config::default();
        for (name, value) in config.entries() {
            restored.set(name, &value).unwrap();
        }
        assert_eq!(restored, config);
        for (name, value) in [
            ("player", ""),
            ("mode", "sprint"),
            ("ghost_enabled", "yes"),
            ("next_queue_length", "7"),
            ("keys.left", "ctrl+a"),
            ("theme.i", "oops"),
            ("unknown", "true"),
        ] {
            restored.set(name, value).unwrap_err();
        }
        assert_eq!(restored, config);
    }
}
