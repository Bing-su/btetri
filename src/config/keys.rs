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
    /// Serialize action bindings in editor order as comma-separated key labels.
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

    /// Replace one named binding list after parsing every comma-separated key.
    /// Unknown fields or unrecognized keys fail without changing the list.
    /// Call `validate` on the complete configuration to reject empty or duplicate bindings.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let keys = value
            .split(',')
            .map(|key| key.trim().parse())
            .collect::<Result<Vec<_>, _>>()?;
        let target = self
            .bindings_mut(name)
            .ok_or_else(|| format!("unknown key setting {name}"))?;
        *target = keys;
        Ok(())
    }

    /// Look up a binding list by its persisted name, such as `keys.left`.
    /// Returns None for unknown fields; callers must validate changes before saving.
    pub fn bindings_mut(&mut self, name: &str) -> Option<&mut Vec<KeyBinding>> {
        match name {
            "keys.left" => Some(&mut self.left),
            "keys.right" => Some(&mut self.right),
            "keys.clockwise" => Some(&mut self.clockwise),
            "keys.counter_clockwise" => Some(&mut self.counter_clockwise),
            "keys.rotate_180" => Some(&mut self.rotate_180),
            "keys.hard_drop" => Some(&mut self.hard_drop),
            "keys.soft_drop" => Some(&mut self.soft_drop),
            "keys.hold" => Some(&mut self.hold),
            "keys.pause" => Some(&mut self.pause),
            "keys.help" => Some(&mut self.help),
            _ => None,
        }
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

    /// Resolve a configured gameplay action using normalized key identity.
    /// Pause and help are screen commands and are handled separately.
    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        self.actions()
            .into_iter()
            .find_map(|(action, bindings)| Self::matches(bindings, key).then_some(action))
    }

    /// Match a terminal event against bindings using the same normalization as capture.
    /// For example, either physical Ctrl key resolves to the CTRL binding.
    pub fn matches(bindings: &[KeyBinding], key: KeyEvent) -> bool {
        // Capture and gameplay share normalization, e.g. either Ctrl key resolves to CTRL.
        KeyBinding::from_event(key).is_ok_and(|binding| bindings.contains(&binding))
    }

    /// Return the first binding label for compact hints, or an empty label for no bindings.
    pub fn label(bindings: &[KeyBinding]) -> String {
        bindings
            .first()
            .map_or_else(String::new, ToString::to_string)
    }

    /// Require a nonempty list per action and reject duplicate keys across all actions.
    pub(super) fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        let groups = self.actions().map(|(_, keys)| keys);
        let groups = groups
            .into_iter()
            .chain([self.pause.as_slice(), self.help.as_slice()]);
        if groups.clone().any(|keys| keys.is_empty()) {
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
            "backspace" => Some(KeyCode::Backspace),
            "delete" => Some(KeyCode::Delete),
            "insert" => Some(KeyCode::Insert),
            "home" => Some(KeyCode::Home),
            "end" => Some(KeyCode::End),
            "pageup" => Some(KeyCode::PageUp),
            "pagedown" => Some(KeyCode::PageDown),
            "tab" => Some(KeyCode::Tab),
            "backtab" => Some(KeyCode::BackTab),
            key if key.starts_with('f') && key.len() > 1 => key
                .strip_prefix('f')
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|n| (1..=24).contains(n))
                .map(KeyCode::F),
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

impl KeyBinding {
    /// Normalize a terminal key for capture and matching, ignoring Shift and Ctrl flags.
    /// Rejects Alt combinations and key codes that cannot be represented as a supported binding.
    pub fn from_event(key: KeyEvent) -> Result<Self, String> {
        if key.modifiers.contains(KeyModifiers::ALT) {
            return Err("Use a single key without Alt".into());
        }
        let code = match key.code {
            KeyCode::Char(c) if !c.is_control() => KeyCode::Char(c.to_ascii_lowercase()),
            KeyCode::Modifier(ModifierKeyCode::RightControl | ModifierKeyCode::LeftControl) => {
                KeyCode::Modifier(ModifierKeyCode::LeftControl)
            }
            KeyCode::Modifier(ModifierKeyCode::RightShift | ModifierKeyCode::LeftShift) => {
                KeyCode::Modifier(ModifierKeyCode::LeftShift)
            }
            KeyCode::F(1..=24)
            | KeyCode::Enter
            | KeyCode::Esc
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Insert
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down => key.code,
            _ => return Err("Unsupported key; choose another key".into()),
        };
        Ok(Self(code))
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            KeyCode::Char(' ') => f.write_str("SPACE"),
            KeyCode::Char(',') => f.write_str("COMMA"),
            KeyCode::Char(c) => write!(f, "{}", c.to_ascii_uppercase()),
            KeyCode::F(number) => write!(f, "F{number}"),
            KeyCode::Modifier(ModifierKeyCode::LeftShift) => f.write_str("SHIFT"),
            KeyCode::Modifier(ModifierKeyCode::LeftControl) => f.write_str("CTRL"),
            code => f.write_str(&format!("{code:?}").to_uppercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(KeyCode::Left, Action::Left)]
    #[case(KeyCode::Right, Action::Right)]
    #[case(KeyCode::Up, Action::Clockwise)]
    #[case(KeyCode::Char('X'), Action::Clockwise)]
    #[case(KeyCode::Char('z'), Action::CounterClockwise)]
    #[case(KeyCode::Char('a'), Action::Rotate180)]
    #[case(KeyCode::Char(' '), Action::HardDrop)]
    #[case(KeyCode::Down, Action::SoftDrop)]
    #[case(KeyCode::Char('c'), Action::Hold)]
    #[case(KeyCode::Modifier(ModifierKeyCode::RightShift), Action::Hold)]
    #[case(
        KeyCode::Modifier(ModifierKeyCode::RightControl),
        Action::CounterClockwise
    )]
    fn default_controls_and_modifier_aliases_keep_working(
        #[case] code: KeyCode,
        #[case] action: Action,
        #[values(KeyModifiers::NONE, KeyModifiers::CONTROL, KeyModifiers::SHIFT)]
        modifiers: KeyModifiers,
    ) {
        let keys = Keys::default();
        keys.validate().unwrap();
        assert_eq!(keys.action(KeyEvent::new(code, modifiers)), Some(action));
        assert_eq!(keys.action(KeyEvent::new(code, KeyModifiers::ALT)), None);
    }

    #[rstest]
    fn captured_keys_survive_storage_round_trip(
        #[values(
            KeyCode::F(2),
            KeyCode::F(24),
            KeyCode::Esc,
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Char(','),
            KeyCode::Char('ß'),
            KeyCode::Char('김'),
            KeyCode::Char(' '),
            KeyCode::Char('?'),
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Modifier(ModifierKeyCode::RightControl),
            KeyCode::Modifier(ModifierKeyCode::RightShift)
        )]
        code: KeyCode,
    ) {
        let key = KeyBinding::from_event(KeyEvent::new(code, KeyModifiers::NONE)).unwrap();
        assert_eq!(key.to_string().parse::<KeyBinding>().unwrap(), key);
        assert!(Keys::matches(
            &[key],
            KeyEvent::new(code, KeyModifiers::NONE)
        ));
    }

    #[rstest]
    fn empty_or_duplicate_bindings_are_rejected(
        #[values("keys.left", "keys.pause", "keys.help")] name: &str,
        #[values("", "right", "g, g")] value: &str,
    ) {
        let mut keys = Keys::default();
        *keys.bindings_mut(name).unwrap() = if value.is_empty() {
            Vec::new()
        } else {
            value.split(", ").map(|key| key.parse().unwrap()).collect()
        };
        keys.validate().unwrap_err();
    }

    proptest! {
        #[test]
        fn printable_keys_and_function_keys_round_trip(
            code in prop_oneof![
                any::<char>().prop_filter("printable key", |c| !c.is_control()).prop_map(KeyCode::Char),
                (1_u8..=24).prop_map(KeyCode::F),
            ],
        ) {
            let event = KeyEvent::new(code, KeyModifiers::NONE);
            let binding = KeyBinding::from_event(event).unwrap();
            let restored: KeyBinding = binding.to_string().parse().unwrap();
            prop_assert_eq!(restored, binding);
            prop_assert!(Keys::matches(&[restored], event));
            prop_assert!(!Keys::matches(&[restored], KeyEvent::new(code, KeyModifiers::ALT)));
        }
    }
}
