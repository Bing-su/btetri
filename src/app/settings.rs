use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::SaveState;
use crate::config::Config;

pub struct Settings {
    pub config: Config,
    pub selected: usize,
    pub edit: Option<String>,
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

    // Return true only when the user discards the draft, e.g. Esc outside a field.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let entries = self.config.entries();
        if self.edit.is_some() {
            if let Some((name, _)) = entries.get(self.selected) {
                self.edit_key(key, name);
            }
            return false;
        }
        self.navigate(key, &entries)
    }

    fn edit_key(&mut self, key: KeyEvent, name: &str) {
        let Some(text) = &mut self.edit else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.edit = None;
                self.error = None;
            }
            KeyCode::Enter => match self.config.set(name, text.trim()) {
                Ok(()) => {
                    self.edit = None;
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            },
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Delete => text.clear(),
            KeyCode::Char(c)
                if !c.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && text.chars().count() < 128 =>
            {
                text.push(c)
            }
            _ => {}
        }
    }

    fn navigate(&mut self, key: KeyEvent, entries: &[(&str, String)]) -> bool {
        let count = entries.len() + 3;
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Up => self.selected = (self.selected + count - 1) % count,
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % count,
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = count - 1,
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                match entries.get(self.selected).map(|(name, _)| *name) {
                    Some("mode") => self.config.mode = self.config.mode.cycle(delta),
                    Some("ghost_enabled") => self.config.ghost_enabled = !self.config.ghost_enabled,
                    Some("next_queue_length") => {
                        self.config.next_queue_length = self
                            .config
                            .next_queue_length
                            .saturating_add_signed(delta as isize)
                            .clamp(1, 6)
                    }
                    _ => {}
                }
                self.error = None;
            }
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
            KeyCode::Enter if self.selected < entries.len() => {
                self.edit = entries.get(self.selected).map(|(_, value)| value.clone());
                self.error = None;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn edits_validate_and_cancel_without_losing_the_draft() {
        let mut settings = Settings::new(Config::default());
        settings.selected = 3;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Char('9')));
        settings.key(key(KeyCode::Enter));
        assert!(settings.error.is_some());
        assert_eq!(settings.config.next_queue_length, 6);
        settings.key(key(KeyCode::Esc));
        assert!(settings.edit.is_none());
        settings.key(key(KeyCode::Left));
        assert_eq!(settings.config.next_queue_length, 5);
        settings.selected = 0;
        settings.key(key(KeyCode::Enter));
        settings.key(key(KeyCode::Delete));
        settings.key(key(KeyCode::Char('김')));
        settings.key(key(KeyCode::Enter));
        assert_eq!(settings.config.player, "김");
        settings.key(key(KeyCode::End));
        settings.key(key(KeyCode::Up));
        settings.key(key(KeyCode::Enter));
        assert_eq!(settings.config, Config::default());
        assert!(matches!(settings.save, SaveState::None));
        settings.key(key(KeyCode::F(2)));
        assert!(matches!(settings.save, SaveState::Pending));
    }
}
