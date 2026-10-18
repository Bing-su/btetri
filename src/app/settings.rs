mod color;

pub use color::{ColorPicker, rgb};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use super::{SaveState, edit_name_input};
use crate::config::{Config, KeyBinding, validate_player};

pub enum Edit {
    Name(String),
    Keys {
        bindings: Vec<KeyBinding>,
        selected: usize,
        capturing: bool,
    },
    Color(ColorPicker),
}

pub struct Settings {
    pub config: Config,
    pub selected: usize,
    pub edit: Option<Edit>,
    pub error: Option<String>,
    pub save: SaveState,
}

impl Settings {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            selected: 0,
            edit: None,
            error: None,
            save: SaveState::None,
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let entries = self.config.entries();
        if self.edit.is_some() {
            if let Some((name, _)) = entries.get(self.selected) {
                self.edit_key(key, name);
            }
            return false;
        }
        let count = entries.len() + 3;
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Up => self.selected = (self.selected + count - 1) % count,
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % count,
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = count - 1,
            KeyCode::F(2) | KeyCode::Enter
                if key.code == KeyCode::F(2) || self.selected == entries.len() =>
            {
                match self.config.validate() {
                    Ok(()) => {
                        self.save = SaveState::Pending;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Enter if self.selected < entries.len() => {
                if let Some((name, _)) = entries.get(self.selected) {
                    self.change(name, key.code);
                }
            }
            KeyCode::Enter if self.selected == entries.len() + 1 => {
                self.config = Config::default();
                self.error = None;
            }
            KeyCode::Enter => return true,
            _ => {}
        }
        false
    }

    fn change(&mut self, name: &str, key: KeyCode) {
        let delta = if key == KeyCode::Left { -1 } else { 1 };
        match name {
            "mode" => self.config.mode = self.config.mode.cycle(delta),
            "ghost_enabled" => self.config.ghost_enabled = !self.config.ghost_enabled,
            "next_queue_length" => {
                self.config.next_queue_length = self
                    .config
                    .next_queue_length
                    .saturating_add_signed(delta as isize)
                    .clamp(1, 6)
            }
            "das_ms" | "arr_ms" | "soft_drop_ms" => {
                let ms = match name {
                    "das_ms" => &mut self.config.das_ms,
                    "arr_ms" => &mut self.config.arr_ms,
                    _ => &mut self.config.soft_drop_ms,
                };
                *ms = ms.saturating_add_signed(i64::from(delta) * 10).min(1000);
            }
            "player" if key == KeyCode::Enter => {
                self.edit = Some(Edit::Name(self.config.player.clone()))
            }
            _ if key == KeyCode::Enter => {
                self.edit = if let Some(bindings) = self.config.keys.bindings_mut(name) {
                    Some(Edit::Keys {
                        bindings: bindings.clone(),
                        selected: 0,
                        capturing: false,
                    })
                } else {
                    self.config
                        .theme
                        .color_mut(name)
                        .map(|color| Edit::Color(ColorPicker::new(*color)))
                };
            }
            _ => {}
        }
        self.error = None;
    }

    fn capture_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        let Some(Edit::Keys {
            bindings,
            selected,
            capturing,
        }) = &mut self.edit
        else {
            return;
        };
        match KeyBinding::from_event(key) {
            Ok(binding)
                if bindings
                    .iter()
                    .enumerate()
                    .any(|(index, &existing)| index != *selected && existing == binding) =>
            {
                self.error = Some("This key is already in this list".into());
                *capturing = false;
            }
            Ok(binding) => {
                if let Some(slot) = bindings.get_mut(*selected) {
                    *slot = binding;
                } else {
                    bindings.push(binding);
                }
                *capturing = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn edit_key(&mut self, key: KeyEvent, name: &str) {
        // Capture before dialog shortcuts, e.g. Esc and F2 can themselves be bound.
        if matches!(
            self.edit,
            Some(Edit::Keys {
                capturing: true,
                ..
            })
        ) {
            self.capture_key(key);
            return;
        }
        if key.code == KeyCode::Esc {
            self.edit = None;
            self.error = None;
            return;
        }
        match &mut self.edit {
            Some(Edit::Name(text)) => match key.code {
                KeyCode::Enter => match validate_player(text.trim()) {
                    Ok(()) => {
                        self.config.player = text.trim().to_owned();
                        self.edit = None;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                },
                _ => edit_name_input(text, key),
            },
            Some(Edit::Color(picker)) if key.code == KeyCode::Enter => {
                if let Some(color) = self.config.theme.color_mut(name) {
                    *color = picker.value;
                    self.edit = None;
                }
            }
            Some(Edit::Color(picker)) => picker.key(key.code),
            Some(Edit::Keys {
                bindings,
                selected,
                capturing,
            }) => match key.code {
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Down => *selected = (*selected + 1).min(bindings.len()),
                KeyCode::Enter => *capturing = true,
                KeyCode::Delete if bindings.len() > 1 && *selected < bindings.len() => {
                    bindings.remove(*selected);
                    *selected = (*selected).min(bindings.len());
                }
                KeyCode::Delete => self.error = Some("Keep at least one key per action".into()),
                KeyCode::F(2) => {
                    if let Some(target) = self.config.keys.bindings_mut(name) {
                        *target = bindings.clone();
                    }
                    self.edit = None;
                    self.error = None;
                }
                _ => {}
            },
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use ratatui::style::Color;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[rstest::rstest]
    #[case("김", KeyCode::Backspace, KeyModifiers::NONE, "")]
    #[case("김", KeyCode::Delete, KeyModifiers::NONE, "")]
    #[case("", KeyCode::Char('김'), KeyModifiers::NONE, "김")]
    #[case("", KeyCode::Char('A'), KeyModifiers::SHIFT, "A")]
    #[case("", KeyCode::Char('c'), KeyModifiers::CONTROL, "")]
    #[case("", KeyCode::Char('c'), KeyModifiers::ALT, "")]
    #[case("", KeyCode::Char('\n'), KeyModifiers::NONE, "")]
    #[case(
        "가나다라마바사아자차카타파하가끝",
        KeyCode::Char('김'),
        KeyModifiers::NONE,
        "가나다라마바사아자차카타파하가끝"
    )]
    fn player_editors_share_text_input_rules(
        #[case] initial: &str,
        #[case] code: KeyCode,
        #[case] modifiers: KeyModifiers,
        #[case] expected: &str,
        #[values(false, true)] settings_screen: bool,
    ) {
        let mut app = crate::app::App::try_from(Config::default()).unwrap();
        if settings_screen {
            app.screen = crate::app::Screen::Settings;
            let mut settings = Settings::new(app.config.clone());
            settings.edit = Some(Edit::Name(initial.into()));
            app.settings = Some(settings);
        } else {
            app.name_draft = Some(initial.into());
        }
        app.key(KeyEvent::new(code, modifiers)).unwrap();
        let actual = if settings_screen {
            let Some(Edit::Name(text)) = &app.settings.as_ref().unwrap().edit else {
                panic!("player editor closed unexpectedly");
            };
            text
        } else {
            app.name_draft.as_ref().unwrap()
        };
        assert_eq!(actual, expected);
        assert_eq!(app.config, Config::default());
    }

    #[test]
    fn duplicate_capture_returns_to_the_list_so_escape_can_cancel() {
        let mut settings = Settings::new(Config::default());
        settings.selected = 15;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Down));
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Esc));
        assert!(settings.error.is_some());
        assert!(matches!(
            settings.edit,
            Some(Edit::Keys {
                capturing: false,
                ..
            })
        ));
        settings.key(key(KeyCode::Esc));
        assert!(settings.edit.is_none());
        assert_eq!(settings.config, Config::default());
    }

    #[test]
    fn choices_are_typed_and_player_is_the_only_text_field() {
        let mut settings = Settings::new(Config::default());
        settings.selected = 2;
        settings.key(key(KeyCode::Enter));
        assert!(!settings.config.ghost_enabled);
        assert!(settings.edit.is_none());
        settings.key(key(KeyCode::Char('t')));
        assert!(!settings.config.ghost_enabled);
        settings.selected = 3;
        for _ in 0..20 {
            settings.key(key(KeyCode::Left));
        }
        assert_eq!(settings.config.next_queue_length, 1);
        for _ in 0..20 {
            settings.key(key(KeyCode::Right));
        }
        assert_eq!(settings.config.next_queue_length, 6);
        settings.selected = 1;
        settings.key(key(KeyCode::Right));
        assert_eq!(settings.config.mode, crate::game::Mode::Blitz);
        assert!(settings.edit.is_none());
        settings.selected = 0;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Enter));
        assert!(settings.error.is_some());
        settings.key(key(KeyCode::Char('김')));
        settings.key(key(KeyCode::Enter));
        assert_eq!(settings.config.player, "김");
    }

    #[rstest::rstest]
    #[case("das_ms")]
    #[case("arr_ms")]
    #[case("soft_drop_ms")]
    fn input_timings_adjust_in_ten_ms_steps_and_clamp(#[case] name: &str) {
        let mut settings = Settings::new(Config::default());
        settings.selected = settings
            .config
            .entries()
            .iter()
            .position(|&(field, _)| field == name)
            .unwrap();
        let value = |settings: &Settings| {
            settings
                .config
                .entries()
                .into_iter()
                .find(|(field, _)| *field == name)
                .unwrap()
                .1
        };
        for _ in 0..110 {
            settings.key(key(KeyCode::Left));
        }
        assert_eq!(value(&settings), "0");
        settings.key(key(KeyCode::Right));
        assert_eq!(value(&settings), "10");
        for _ in 0..110 {
            settings.key(key(KeyCode::Right));
        }
        assert_eq!(value(&settings), "1000");
        assert!(settings.edit.is_none());
        settings.key(key(KeyCode::F(2)));
        assert!(matches!(settings.save, SaveState::Pending));
    }

    #[test]
    fn binding_editor_captures_adds_removes_and_cancels_keys() {
        let mut settings = Settings::new(Config::default());
        settings.selected = 9;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Down));
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Char('g')));
        settings.key(key(KeyCode::Down));
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::F(2)));
        let labels: Vec<_> = settings
            .config
            .keys
            .clockwise
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(labels, ["UP", "G", "DELETE"]);
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Esc));
        settings.key(key(KeyCode::Esc));
        assert_eq!(
            settings
                .config
                .keys
                .clockwise
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            labels
        );
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Delete));
        assert!(settings.error.is_some());
        settings.key(key(KeyCode::F(2)));
        assert_eq!(settings.config.keys.clockwise.len(), 1);
    }

    #[test]
    fn color_changes_apply_only_on_accept() {
        let mut settings = Settings::new(Config::default());
        settings.selected = 17;
        let original = settings.config.theme.background;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Right));
        settings.key(key(KeyCode::Esc));
        assert_eq!(settings.config.theme.background, original);
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Enter));
        assert_eq!(settings.config.theme.background, Color::Reset);
    }
}
