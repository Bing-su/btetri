use ratatui::style::Color;
use serde::Deserialize;

use super::deserialize_from_str;
use crate::game::Piece;

// Resolve the palette at load time so rendering cannot silently replace invalid colors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    #[serde(deserialize_with = "deserialize_from_str")]
    pub background: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub foreground: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub muted: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub border: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub accent: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub grid: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub ghost: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub i: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub o: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub t: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub s: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub z: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub j: Color,
    #[serde(deserialize_with = "deserialize_from_str")]
    pub l: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(0x10, 0x11, 0x16),
            foreground: Color::Rgb(0xE9, 0xE9, 0xED),
            muted: Color::Rgb(0xA7, 0xAB, 0xB5),
            border: Color::Rgb(0x88, 0x8C, 0x96),
            accent: Color::Rgb(0x64, 0xC4, 0xEB),
            grid: Color::Rgb(0x2A, 0x2D, 0x35),
            ghost: Color::Rgb(0x8C, 0x71, 0x89),
            i: Color::Rgb(0x64, 0xC4, 0xEB),
            o: Color::Rgb(0xF1, 0xD4, 0x48),
            t: Color::Rgb(0xA1, 0x53, 0x98),
            s: Color::Rgb(0x64, 0xB4, 0x52),
            z: Color::Rgb(0xDC, 0x3A, 0x35),
            j: Color::Rgb(0x5C, 0x65, 0xA8),
            l: Color::Rgb(0xE0, 0x7F, 0x3A),
        }
    }
}

impl Theme {
    pub fn piece(&self, piece: Piece) -> Color {
        match piece {
            Piece::I => self.i,
            Piece::O => self.o,
            Piece::T => self.t,
            Piece::S => self.s,
            Piece::Z => self.z,
            Piece::J => self.j,
            Piece::L => self.l,
        }
    }
}
