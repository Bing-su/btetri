use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, ModifierKeyCode};

use crate::game::Action;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keys {
    pub left: Vec<KeyBinding>,
    pub right: Vec<KeyBinding>,
    pub clockwise: Vec<KeyBinding>,
    pub counter_clockwise: Vec<KeyBinding>,
    pub rotate_180: Vec<KeyBinding>,
    pub hard_drop: Vec<KeyBinding>,
    pub soft_drop: Vec<KeyBinding>,
    pub hold: Vec<KeyBinding>,
    pub pause: Vec<KeyBinding>,
    pub help: Vec<KeyBinding>,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            left: vec![KeyBinding(KeyCode::Left)],
            right: vec![KeyBinding(KeyCode::Right)],
            clockwise: vec![KeyBinding(KeyCode::Up), KeyBinding(KeyCode::Char('x'))],
            counter_clockwise: vec![
                KeyBinding(KeyCode::Char('z')),
                KeyBinding(KeyCode::Modifier(ModifierKeyCode::LeftControl)),
            ],
            rotate_180: vec![KeyBinding(KeyCode::Char('a'))],
            hard_drop: vec![KeyBinding(KeyCode::Char(' '))],
            soft_drop: vec![KeyBinding(KeyCode::Down)],
            hold: vec![
                KeyBinding(KeyCode::Char('c')),
                KeyBinding(KeyCode::Modifier(ModifierKeyCode::LeftShift)),
            ],
            pause: vec![KeyBinding(KeyCode::Esc)],
            help: vec![KeyBinding(KeyCode::Char('?'))],
        }
    }
}

impl Keys {
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        [
            ("keys.left", &self.left),
            ("keys.right", &self.right),
            ("keys.clockwise", &self.clockwise),
            ("keys.counter_clockwise", &self.counter_clockwise),
            ("keys.rotate_180", &self.rotate_180),
            ("keys.hard_drop", &self.hard_drop),
            ("keys.soft_drop", &self.soft_drop),
            ("keys.hold", &self.hold),
            ("keys.pause", &self.pause),
            ("keys.help", &self.help),
        ]
        .into_iter()
        .map(|(name, keys)| {
            (
                name,
                keys.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
        .collect()
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let keys = value
            .split(',')
            .map(|key| key.trim().parse())
            .collect::<Result<Vec<_>, _>>()?;
        let target = match name {
            "keys.left" => &mut self.left,
            "keys.right" => &mut self.right,
            "keys.clockwise" => &mut self.clockwise,
            "keys.counter_clockwise" => &mut self.counter_clockwise,
            "keys.rotate_180" => &mut self.rotate_180,
            "keys.hard_drop" => &mut self.hard_drop,
            "keys.soft_drop" => &mut self.soft_drop,
            "keys.hold" => &mut self.hold,
            "keys.pause" => &mut self.pause,
            "keys.help" => &mut self.help,
            _ => return Err(format!("unknown key setting {name}")),
        };
        *target = keys;
        Ok(())
    }

    fn actions(&self) -> [(Action, &[KeyBinding]); 8] {
        [
            (Action::Left, &self.left),
            (Action::Right, &self.right),
            (Action::Clockwise, &self.clockwise),
            (Action::CounterClockwise, &self.counter_clockwise),
            (Action::Rotate180, &self.rotate_180),
            (Action::HardDrop, &self.hard_drop),
            (Action::SoftDrop, &self.soft_drop),
            (Action::Hold, &self.hold),
        ]
    }

    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        self.actions()
            .into_iter()
            .find_map(|(action, bindings)| Self::matches(bindings, key).then_some(action))
    }

    pub fn matches(bindings: &[KeyBinding], key: KeyEvent) -> bool {
        // Ctrl is also a rotation key, so holding it must still allow movement, e.g. Ctrl+Left.
        if key.modifiers.contains(KeyModifiers::ALT) {
            return false;
        }
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            KeyCode::Modifier(ModifierKeyCode::RightShift) => {
                KeyCode::Modifier(ModifierKeyCode::LeftShift)
            }
            KeyCode::Modifier(ModifierKeyCode::RightControl) => {
                KeyCode::Modifier(ModifierKeyCode::LeftControl)
            }
            code => code,
        };
        bindings.contains(&KeyBinding(code))
    }

    pub fn label(bindings: &[KeyBinding]) -> String {
        bindings
            .first()
            .map_or_else(String::new, ToString::to_string)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        let mut groups: Vec<&[KeyBinding]> =
            self.actions().into_iter().map(|(_, keys)| keys).collect();
        groups.extend([self.pause.as_slice(), self.help.as_slice()]);
        if groups.iter().any(|keys| keys.is_empty()) {
            return Err("every action needs at least one key".into());
        }
        for key in groups.into_iter().flatten() {
            if !seen.insert(key) {
                return Err(format!("key {key} is assigned to more than one action"));
            }
        }
        Ok(())
    }
}

// Parse once at the configuration seam; input matching then compares key codes directly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyBinding(KeyCode);

impl FromStr for KeyBinding {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        let code = match key.to_ascii_lowercase().as_str() {
            "space" => Some(KeyCode::Char(' ')),
            "comma" => Some(KeyCode::Char(',')),
            "enter" => Some(KeyCode::Enter),
            "esc" => Some(KeyCode::Esc),
            "left" => Some(KeyCode::Left),
            "right" => Some(KeyCode::Right),
            "up" => Some(KeyCode::Up),
            "down" => Some(KeyCode::Down),
            "shift" => Some(KeyCode::Modifier(ModifierKeyCode::LeftShift)),
            "ctrl" => Some(KeyCode::Modifier(ModifierKeyCode::LeftControl)),
            _ => {
                let mut chars = key.chars();
                chars
                    .next()
                    .filter(|c| !c.is_control() && chars.next().is_none())
                    .map(|c| KeyCode::Char(c.to_ascii_lowercase()))
            }
        };
        code.map(Self)
            .ok_or_else(|| format!("unsupported key {key:?}"))
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            KeyCode::Char(' ') => f.write_str("SPACE"),
            KeyCode::Char(',') => f.write_str("COMMA"),
            KeyCode::Char(c) => write!(f, "{}", c.to_ascii_uppercase()),
            KeyCode::Modifier(ModifierKeyCode::LeftShift) => f.write_str("SHIFT"),
            KeyCode::Modifier(ModifierKeyCode::LeftControl) => f.write_str("CTRL"),
            code => f.write_str(&format!("{code:?}").to_uppercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_controls_and_modifier_aliases_keep_working() {
        let keys = Keys::default();
        keys.validate().unwrap();
        for (code, action) in [
            (KeyCode::Left, Action::Left),
            (KeyCode::Right, Action::Right),
            (KeyCode::Up, Action::Clockwise),
            (KeyCode::Char('X'), Action::Clockwise),
            (KeyCode::Char('z'), Action::CounterClockwise),
            (KeyCode::Char('a'), Action::Rotate180),
            (KeyCode::Char(' '), Action::HardDrop),
            (KeyCode::Down, Action::SoftDrop),
            (KeyCode::Char('c'), Action::Hold),
            (KeyCode::Modifier(ModifierKeyCode::RightShift), Action::Hold),
            (
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                Action::CounterClockwise,
            ),
        ] {
            assert_eq!(
                keys.action(KeyEvent::new(code, KeyModifiers::CONTROL)),
                Some(action)
            );
            assert_eq!(keys.action(KeyEvent::new(code, KeyModifiers::ALT)), None);
        }
        for name in [
            "SPACE", "COMMA", "ß", "김", "CTRL", "SHIFT", "ESC", "?", "ENTER", "LEFT",
        ] {
            let key: KeyBinding = name.parse().unwrap();
            assert_eq!(key.to_string().parse::<KeyBinding>().unwrap(), key);
        }
    }
}
