use ratatui::style::Color;

use crate::game::Piece;

// Resolve the palette at load time so rendering cannot silently replace invalid colors.
#[derive(Clone, Debug, Eq, PartialEq)]

pub struct Theme {
    pub background: Color,

    pub foreground: Color,

    pub muted: Color,

    pub border: Color,

    pub accent: Color,

    pub grid: Color,

    pub ghost: Color,

    pub i: Color,

    pub o: Color,

    pub t: Color,

    pub s: Color,

    pub z: Color,

    pub j: Color,

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
    /// Look up a palette entry by its persisted name, such as `theme.background`.
    pub fn color_mut(&mut self, name: &str) -> Option<&mut Color> {
        match name {
            "theme.background" => Some(&mut self.background),
            "theme.foreground" => Some(&mut self.foreground),
            "theme.muted" => Some(&mut self.muted),
            "theme.border" => Some(&mut self.border),
            "theme.accent" => Some(&mut self.accent),
            "theme.grid" => Some(&mut self.grid),
            "theme.ghost" => Some(&mut self.ghost),
            "theme.i" => Some(&mut self.i),
            "theme.o" => Some(&mut self.o),
            "theme.t" => Some(&mut self.t),
            "theme.s" => Some(&mut self.s),
            "theme.z" => Some(&mut self.z),
            "theme.j" => Some(&mut self.j),
            "theme.l" => Some(&mut self.l),
            _ => None,
        }
    }

    /// Serialize the palette in editor order using round-trippable color labels.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        [
            ("theme.background", self.background.to_string()),
            ("theme.foreground", self.foreground.to_string()),
            ("theme.muted", self.muted.to_string()),
            ("theme.border", self.border.to_string()),
            ("theme.accent", self.accent.to_string()),
            ("theme.grid", self.grid.to_string()),
            ("theme.ghost", self.ghost.to_string()),
            ("theme.i", self.i.to_string()),
            ("theme.o", self.o.to_string()),
            ("theme.t", self.t.to_string()),
            ("theme.s", self.s.to_string()),
            ("theme.z", self.z.to_string()),
            ("theme.j", self.j.to_string()),
            ("theme.l", self.l.to_string()),
        ]
        .into()
    }

    /// Parse and replace a named color; invalid colors or names leave the palette unchanged.
    /// Accepts color names, RGB hex values such as #64C4EB, and indices from 0 to 255.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let color = value
            .parse::<Color>()
            .map_err(|error| format!("{error}; use a color name, #RRGGBB, or 0–255"))?;
        let target = self
            .color_mut(name)
            .ok_or_else(|| format!("unknown theme setting {name}"))?;
        *target = color;
        Ok(())
    }

    /// Resolve a tetromino color from the current palette.
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
