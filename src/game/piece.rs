#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Piece {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Piece {
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::O,
        Self::T,
        Self::S,
        Self::Z,
        Self::J,
        Self::L,
    ];

    pub fn cells(self, rotation: u8) -> [(i32, i32); 4] {
        let cells = match self {
            Self::I => [(0, 1), (1, 1), (2, 1), (3, 1)],
            Self::O => return [(1, 0), (2, 0), (1, 1), (2, 1)],
            Self::T => [(1, 0), (0, 1), (1, 1), (2, 1)],
            Self::S => [(1, 0), (2, 0), (0, 1), (1, 1)],
            Self::Z => [(0, 0), (1, 0), (1, 1), (2, 1)],
            Self::J => [(0, 0), (0, 1), (1, 1), (2, 1)],
            Self::L => [(2, 0), (0, 1), (1, 1), (2, 1)],
        };
        let edge = if self == Self::I { 3 } else { 2 };
        cells.map(|(mut x, mut y)| {
            for _ in 0..rotation % 4 {
                (x, y) = (edge - y, x);
            }
            (x, y)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivePiece {
    pub kind: Piece,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
}

impl ActivePiece {
    pub fn spawn(kind: Piece) -> Self {
        Self {
            kind,
            rotation: 0,
            x: 3,
            y: 1,
        }
    }

    pub fn cells(self) -> [(i32, i32); 4] {
        self.kind
            .cells(self.rotation)
            .map(|(x, y)| (x + self.x, y + self.y))
    }
}

pub(super) fn half_turn_kicks(from: u8) -> &'static [(i32, i32)] {
    // TETR.IO's six 180-degree tests use screen coordinates, e.g. (0, -1) kicks up.
    match from {
        0 => &[(0, 0), (0, -1), (1, -1), (-1, -1), (1, 0), (-1, 0)],
        1 => &[(0, 0), (1, 0), (1, -2), (1, -1), (0, -2), (0, -1)],
        2 => &[(0, 0), (0, 1), (-1, 1), (1, 1), (-1, 0), (1, 0)],
        _ => &[(0, 0), (-1, 0), (-1, -2), (-1, -1), (0, -2), (0, -1)],
    }
}

// SRS offsets use screen coordinates: e.g. an upward kick has a negative y.
pub(super) fn kicks(kind: Piece, from: u8, to: u8) -> [(i32, i32); 5] {
    if kind == Piece::I {
        match (from, to) {
            (0, 1) | (3, 2) => [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
            (1, 0) | (2, 3) => [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
            (1, 2) | (0, 3) => [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
            _ => [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
        }
    } else {
        match (from, to) {
            (0, 1) | (2, 1) => [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
            (1, 0) | (1, 2) => [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
            (2, 3) | (0, 3) => [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            _ => [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        }
    }
}
