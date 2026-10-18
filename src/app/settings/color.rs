use crossterm::event::KeyCode;
use ratatui::style::Color;

pub struct ColorPicker {
    pub value: Color,
    pub palette: bool,
    pub channel: usize,
}

impl ColorPicker {
    pub fn new(value: Color) -> Self {
        Self {
            value,
            palette: !matches!(value, Color::Rgb(..)),
            channel: 0,
        }
    }

    pub fn key(&mut self, key: KeyCode) {
        if key == KeyCode::Delete {
            self.value = Color::Reset;
            return;
        }
        if key == KeyCode::Tab {
            self.palette = !self.palette;
            self.value = if self.palette {
                Color::Indexed(self.index())
            } else {
                let [r, g, b] = rgb(self.value);
                Color::Rgb(r, g, b)
            };
            return;
        }
        if self.palette {
            let index = self.index();
            let next = match key {
                KeyCode::Left => index.saturating_sub(1),
                KeyCode::Right => index.saturating_add(1),
                KeyCode::Up if index >= 32 => index - 32,
                KeyCode::Down if index < 224 => index + 32,
                KeyCode::Up | KeyCode::Down => index,
                _ => return,
            };
            self.value = Color::Indexed(next);
            return;
        }
        match key {
            KeyCode::Up => self.channel = (self.channel + 2) % 3,
            KeyCode::Down => self.channel = (self.channel + 1) % 3,
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => {
                let mut values = rgb(self.value);
                if let Some(value) = values.get_mut(self.channel) {
                    *value = match key {
                        KeyCode::Home => 0,
                        KeyCode::End => 255,
                        KeyCode::Left => value.saturating_sub(1),
                        KeyCode::Right => value.saturating_add(1),
                        KeyCode::PageDown => value.saturating_sub(16),
                        _ => value.saturating_add(16),
                    };
                }
                let [r, g, b] = values;
                self.value = Color::Rgb(r, g, b);
            }
            _ => {}
        }
    }

    pub fn index(&self) -> u8 {
        if let Color::Indexed(index) = self.value {
            return index;
        }
        let current = rgb(self.value);
        (0..=255)
            .min_by_key(|&index| {
                rgb(Color::Indexed(index))
                    .into_iter()
                    .zip(current)
                    .map(|(a, b)| (i32::from(a) - i32::from(b)).pow(2))
                    .sum::<i32>()
            })
            .unwrap_or_default()
    }
}

// Match the xterm palette so switching to RGB preserves colors, e.g. index 196 is red.
pub fn rgb(color: Color) -> [u8; 3] {
    let index = match color {
        Color::Rgb(r, g, b) => return [r, g, b],
        Color::Indexed(index) => index,
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White | Color::Reset => 15,
    };
    if index < 16 {
        return [
            [0, 0, 0],
            [128, 0, 0],
            [0, 128, 0],
            [128, 128, 0],
            [0, 0, 128],
            [128, 0, 128],
            [0, 128, 128],
            [192, 192, 192],
            [128, 128, 128],
            [255, 0, 0],
            [0, 255, 0],
            [255, 255, 0],
            [0, 0, 255],
            [255, 0, 255],
            [0, 255, 255],
            [255, 255, 255],
        ]
        .get(usize::from(index))
        .copied()
        .unwrap_or_default();
    }
    if index >= 232 {
        return [8 + (index - 232) * 10; 3];
    }
    let cube = index - 16;
    [cube / 36, cube / 6 % 6, cube % 6].map(|component| {
        if component == 0 {
            0
        } else {
            55 + component * 40
        }
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn switching_palette_to_rgb_and_back_preserves_color(index in any::<u8>()) {
            let mut picker = ColorPicker::new(Color::Indexed(index));
            let original = rgb(picker.value);
            picker.key(KeyCode::Tab);
            let [r, g, b] = original;
            prop_assert_eq!(picker.value, Color::Rgb(r, g, b));
            picker.key(KeyCode::Tab);
            // Some palette entries share RGB, e.g. indices 9 and 196 are both red.
            prop_assert_eq!(rgb(picker.value), original);
        }
    }

    #[test]
    fn palette_rgb_and_reset_are_selected_without_text() {
        let mut edge = ColorPicker::new(Color::Indexed(224));
        edge.key(KeyCode::Down);
        assert_eq!(edge.value, Color::Indexed(224));
        edge.key(KeyCode::Up);
        assert_eq!(edge.value, Color::Indexed(192));
        let mut picker = ColorPicker::new(Color::Indexed(196));
        assert_eq!(rgb(picker.value), [255, 0, 0]);
        picker.key(KeyCode::Tab);
        assert_eq!(picker.value, Color::Rgb(255, 0, 0));
        picker.key(KeyCode::Right);
        assert_eq!(picker.value, Color::Rgb(255, 0, 0));
        picker.key(KeyCode::Down);
        picker.key(KeyCode::PageUp);
        assert_eq!(picker.value, Color::Rgb(255, 16, 0));
        picker.key(KeyCode::Home);
        assert_eq!(picker.value, Color::Rgb(255, 0, 0));
        picker.key(KeyCode::Tab);
        assert!(matches!(picker.value, Color::Indexed(_)));
        picker.key(KeyCode::Delete);
        assert_eq!(picker.value, Color::Reset);
    }
}
