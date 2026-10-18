use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, ModifierKeyCode};
use serde::{Deserialize, Deserializer};

use super::deserialize_from_str;
use crate::game::Action;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
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

impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from_str(deserializer)
    }
}

impl FromStr for KeyBinding {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        let code = match key.to_ascii_lowercase().as_str() {
            "space" => Some(KeyCode::Char(' ')),
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
            KeyCode::Char(c) => write!(f, "{}", c.to_uppercase()),
            KeyCode::Modifier(ModifierKeyCode::LeftShift) => f.write_str("SHIFT"),
            KeyCode::Modifier(ModifierKeyCode::LeftControl) => f.write_str("CTRL"),
            code => f.write_str(&format!("{code:?}").to_uppercase()),
        }
    }
}
