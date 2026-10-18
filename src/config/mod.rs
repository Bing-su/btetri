mod keys;
mod theme;

use std::path::PathBuf;

use directories::ProjectDirs;
pub use keys::{KeyBinding, Keys};
pub use theme::Theme;

use crate::game::Mode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub player: String,
    pub mode: Mode,
    pub ghost_enabled: bool,
    pub next_queue_length: usize,
    pub das_ms: u64,
    pub arr_ms: u64,
    pub soft_drop_ms: u64,
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
            das_ms: 150,
            arr_ms: 30,
            soft_drop_ms: 30,
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
        if [self.das_ms, self.arr_ms, self.soft_drop_ms]
            .into_iter()
            .any(|ms| ms > 1000)
        {
            return Err("input timings must be between 0 and 1000 ms".into());
        }
        self.keys.validate()
    }

    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut entries = vec![
            ("player", self.player.clone()),
            ("mode", self.mode.to_string()),
            ("ghost_enabled", self.ghost_enabled.to_string()),
            ("next_queue_length", self.next_queue_length.to_string()),
            ("das_ms", self.das_ms.to_string()),
            ("arr_ms", self.arr_ms.to_string()),
            ("soft_drop_ms", self.soft_drop_ms.to_string()),
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
            "das_ms" | "arr_ms" | "soft_drop_ms" => {
                let ms = value
                    .parse::<u64>()
                    .map_err(|error| format!("{error}; use a number from 0 to 1000 ms"))?;
                if ms > 1000 {
                    return Err("use a number from 0 to 1000 ms".into());
                }
                match name {
                    "das_ms" => self.das_ms = ms,
                    "arr_ms" => self.arr_ms = ms,
                    _ => self.soft_drop_ms = ms,
                }
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
    use proptest::prelude::*;
    use ratatui::style::Color;

    use super::*;

    #[rstest::rstest]
    #[case("player", "")]
    #[case("player", "                 ")]
    #[case("player", "12345678901234567")]
    #[case("player", "Alice\n")]
    #[case("mode", "sprint")]
    #[case("ghost_enabled", "yes")]
    #[case("next_queue_length", "0")]
    #[case("next_queue_length", "7")]
    #[case("das_ms", "1001")]
    #[case("arr_ms", "-1")]
    #[case("soft_drop_ms", "oops")]
    #[case("keys.left", "ctrl+a")]
    #[case("theme.i", "oops")]
    #[case("unknown", "true")]
    fn invalid_setting_leaves_config_unchanged(#[case] name: &str, #[case] value: &str) {
        let mut config = Config::default();
        config.set(name, value).unwrap_err();
        assert_eq!(config, Config::default());
    }

    #[rstest::rstest]
    #[case(1001, 30, 30)]
    #[case(150, 1001, 30)]
    #[case(150, 30, 1001)]
    fn invalid_input_timings_are_rejected(
        #[case] das_ms: u64,
        #[case] arr_ms: u64,
        #[case] soft_drop_ms: u64,
    ) {
        Config {
            das_ms,
            arr_ms,
            soft_drop_ms,
            ..Config::default()
        }
        .validate()
        .unwrap_err();
    }

    proptest! {
        #[test]
        fn settings_entries_restore_changed_values(
            player in "[A-Za-z가-힣]{1,16}",
            mode in prop::sample::select(&Mode::ALL),
            ghost_enabled in any::<bool>(),
            next_queue_length in 1_usize..=6,
            das_ms in 0_u64..=1000,
            arr_ms in 0_u64..=1000,
            soft_drop_ms in 0_u64..=1000,
            function_key in 1_u8..=24,
            rgb in any::<[u8; 3]>(),
            palette in any::<u8>(),
        ) {
            let mut config = Config {
                player,
                mode,
                ghost_enabled,
                next_queue_length,
                das_ms,
                arr_ms,
                soft_drop_ms,
                ..Config::default()
            };
            config.set("keys.left", &format!("F{function_key}")).unwrap();
            let [r, g, b] = rgb;
            config.theme.background = Color::Rgb(r, g, b);
            config.theme.i = Color::Indexed(palette);
            config.validate().unwrap();
            let mut restored = Config::default();
            for (name, value) in config.entries() {
                restored.set(name, &value).unwrap();
            }
            restored.validate().unwrap();
            prop_assert_eq!(restored, config);
        }
    }
}
