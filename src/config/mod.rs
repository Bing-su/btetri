mod keys;
mod theme;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, fs};

use directories::ProjectDirs;
pub use keys::Keys;
use miette::{Diagnostic, IntoDiagnostic, NamedSource, SourceSpan, WrapErr};
use serde::{Deserialize, Deserializer};
pub use theme::Theme;

use crate::game::Mode;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub player: String,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub mode: Mode,
    pub start_level: u32,
    pub max_level: u32,
    pub end_on_max_level: bool,
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
            start_level: 1,
            max_level: 20,
            end_on_max_level: false,
            ghost_enabled: true,
            next_queue_length: 6,
            keys: Keys::default(),
            theme: Theme::default(),
        }
    }
}

#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("invalid configuration: {message}")]
#[diagnostic(
    code(btetri::configuration),
    help("Compare this setting with config.example.toml.")
)]
pub struct ConfigError {
    message: String,
    #[source_code]
    contents: NamedSource<String>,
    #[label("check this setting")]
    span: SourceSpan,
}

impl Config {
    pub fn parse(source: &str, name: &str) -> miette::Result<Self> {
        let config: Self =
            toml::from_str(source).map_err(|error: toml::de::Error| ConfigError {
                message: error.message().to_owned(),
                contents: NamedSource::new(name, source.to_owned()),
                span: error.span().unwrap_or(0..source.len()).into(),
            })?;
        config.validate().map_err(|message| ConfigError {
            message,
            contents: NamedSource::new(name, source.to_owned()),
            span: (0, source.len()).into(),
        })?;
        Ok(config)
    }

    pub fn load(path: &Path, required: bool) -> miette::Result<Self> {
        match fs::read_to_string(path) {
            Ok(source) => Self::parse(&source, &path.display().to_string()),
            Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            Err(error) => Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_player(&self.player)?;
        if self.start_level == 0 || self.start_level > self.max_level || self.max_level > 20 {
            return Err("levels must satisfy 1 <= start_level <= max_level <= 20".into());
        }
        if !(1..=6).contains(&self.next_queue_length) {
            return Err("next_queue_length must be between 1 and 6".into());
        }
        self.keys.validate()?;
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

pub struct Paths {
    pub config: PathBuf,
    pub db: PathBuf,
}

impl Paths {
    pub fn resolve(config: Option<PathBuf>, db: Option<PathBuf>) -> miette::Result<Self> {
        let dirs = ProjectDirs::from("", "", "btetri");
        let config = config
            .or_else(|| {
                dirs.as_ref()
                    .map(|dirs| dirs.config_dir().join("config.toml"))
            })
            .ok_or_else(|| {
                miette::miette!("cannot locate configuration directory; pass --config and --db")
            })?;
        let db = db
            .or_else(|| {
                dirs.as_ref()
                    .map(|dirs| dirs.data_local_dir().join("records.db"))
            })
            .ok_or_else(|| miette::miette!("cannot locate data directory; pass --db"))?;
        Ok(Self { config, db })
    }
}

fn deserialize_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::Color;
    use rstest::rstest;

    use super::keys::KeyBinding;
    use super::*;
    use crate::game::{Action, Piece};

    #[rstest]
    #[case("start_level = 0")]
    #[case("max_level = 21")]
    #[case("next_queue_length = 0")]
    #[case("player = ''")]
    #[case("typo = true")]
    #[case("[keys]\nleft = ['right']")]
    #[case("[theme]\ni = 'not-a-color'")]
    #[case("mode = 'unknown'")]
    #[case("[keys]\nleft = ['ctrl+a']")]
    #[case("[keys]\nleft = [' ']")]
    fn invalid_settings_are_reported(#[case] source: &str) {
        Config::parse(source, "test.toml").unwrap_err();
    }

    #[test]
    fn partial_config_keeps_defaults_and_key_aliases() {
        let config =
            Config::parse("player = 'Alice'\n[keys]\nleft = ['left']", "test.toml").unwrap();
        assert_eq!(
            config
                .keys
                .action(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(Action::Left)
        );
        assert_eq!(config.max_level, 20);
    }

    #[rstest]
    #[case(KeyCode::Left, KeyModifiers::NONE, Action::Left)]
    #[case(KeyCode::Left, KeyModifiers::CONTROL, Action::Left)]
    #[case(KeyCode::Right, KeyModifiers::NONE, Action::Right)]
    #[case(KeyCode::Down, KeyModifiers::NONE, Action::SoftDrop)]
    #[case(KeyCode::Up, KeyModifiers::NONE, Action::Clockwise)]
    #[case(KeyCode::Char('x'), KeyModifiers::NONE, Action::Clockwise)]
    #[case(KeyCode::Char('Z'), KeyModifiers::SHIFT, Action::CounterClockwise)]
    #[case(KeyCode::Char('a'), KeyModifiers::NONE, Action::Rotate180)]
    #[case(KeyCode::Char(' '), KeyModifiers::NONE, Action::HardDrop)]
    #[case(KeyCode::Char('c'), KeyModifiers::NONE, Action::Hold)]
    #[case(
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftControl),
        KeyModifiers::CONTROL,
        Action::CounterClockwise
    )]
    #[case(
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::RightControl),
        KeyModifiers::CONTROL,
        Action::CounterClockwise
    )]
    #[case(
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftShift),
        KeyModifiers::SHIFT,
        Action::Hold
    )]
    #[case(
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::RightShift),
        KeyModifiers::SHIFT,
        Action::Hold
    )]
    fn tetrio_default_keys(
        #[case] code: KeyCode,
        #[case] modifiers: KeyModifiers,
        #[case] action: Action,
    ) {
        let defaults = Config::default();
        defaults.validate().unwrap();
        let example = Config::parse(include_str!("../../config.example.toml"), "example").unwrap();
        for config in [defaults, example] {
            assert_eq!(
                config.keys.action(KeyEvent::new(code, modifiers)),
                Some(action)
            );
        }
    }

    #[rstest]
    #[case("A", "A", KeyCode::Char('a'))]
    #[case("SPACE", "SPACE", KeyCode::Char(' '))]
    #[case("enter", "ENTER", KeyCode::Enter)]
    #[case("left", "LEFT", KeyCode::Left)]
    #[case(
        "shift",
        "SHIFT",
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftShift)
    )]
    #[case(
        "ctrl",
        "CTRL",
        KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftControl)
    )]
    #[case("?", "?", KeyCode::Char('?'))]
    fn key_bindings_parse_display_and_match(
        #[case] source: &str,
        #[case] label: &str,
        #[case] code: KeyCode,
    ) {
        let binding: KeyBinding = source.parse().unwrap();
        assert_eq!(binding.to_string(), label);
        assert!(Keys::matches(
            &[binding],
            KeyEvent::new(code, KeyModifiers::NONE)
        ));
        assert!(!Keys::matches(
            &[binding],
            KeyEvent::new(code, KeyModifiers::ALT)
        ));
    }

    #[test]
    fn example_config_keeps_the_same_wire_format() {
        let config = Config::parse(
            include_str!("../../config.example.toml"),
            "config.example.toml",
        )
        .unwrap();
        assert_eq!(config.mode, Mode::Marathon);
        assert_eq!(Keys::label(&config.keys.hold), "C");
        assert_eq!(config.keys.hold.len(), 2);
        assert_eq!(config.theme, Theme::default());
    }

    #[rstest]
    #[case("#12abEF", Color::Rgb(0x12, 0xAB, 0xEF))]
    #[case("red", Color::Red)]
    #[case("42", Color::Indexed(42))]
    fn theme_colors_are_parsed_at_load_time(#[case] source: &str, #[case] expected: Color) {
        let config = Config::parse(&format!("[theme]\ni = '{source}'"), "theme.toml").unwrap();
        assert_eq!(config.theme.i, expected);
        assert_eq!(config.theme.piece(Piece::I), expected);
        assert_eq!(config.theme.background, Theme::default().background);
    }

    #[test]
    fn invalid_theme_color_reports_its_source_setting() {
        let error = Config::parse("[theme]\ni = 'invalid'", "theme.toml").unwrap_err();
        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("theme.toml"));
        assert!(diagnostic.contains("i = 'invalid'"));
    }

    #[test]
    fn only_implicit_missing_config_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        Config::load(&path, false).unwrap();
        Config::load(&path, true).unwrap_err();
    }
}
