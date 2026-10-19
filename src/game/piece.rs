/// One of the seven tetromino shapes, independent of its position and rotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Piece {
    /// Straight four-cell piece.
    I,
    /// Square piece.
    O,
    /// T-shaped piece.
    T,
    /// Right-leaning zigzag piece.
    S,
    /// Left-leaning zigzag piece.
    Z,
    /// Piece with a block above the left end of its horizontal base.
    J,
    /// Piece with a block above the right end of its horizontal base.
    L,
}

impl Piece {
    /// One copy of every shape, used to build each shuffled bag.
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::O,
        Self::T,
        Self::S,
        Self::Z,
        Self::J,
        Self::L,
    ];

    /// Return four local `(column, row)` offsets after clockwise quarter turns modulo four.
    /// Coordinates increase rightward and downward; the square keeps the same offsets.
    ///
    /// ```
    /// use btetri::game::Piece;
    /// assert_eq!(Piece::I.cells(0), [(0, 1), (1, 1), (2, 1), (3, 1)]);
    /// assert_eq!(Piece::T.cells(4), Piece::T.cells(0));
    /// ```
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

/// A shape and local origin in board coordinates, including the hidden spawn rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivePiece {
    /// Tetromino shape.
    pub kind: Piece,
    /// Clockwise quarter turns; cell calculation reduces this modulo four.
    pub rotation: u8,
    /// Column of the shape's local origin, increasing to the right.
    pub x: i32,
    /// Row of the shape's local origin, increasing downward from the board's hidden top.
    pub y: i32,
}

impl ActivePiece {
    /// Start above the visible board; `Game` then descends one row if it is free.
    pub fn spawn(kind: Piece) -> Self {
        Self {
            kind,
            rotation: 0,
            x: 3,
            y: i32::try_from(super::HIDDEN).unwrap_or_default() - 2,
        }
    }

    /// Translate rotated local cells into board `(column, row)` coordinates without clipping.
    pub fn cells(self) -> [(i32, i32); 4] {
        self.kind
            .cells(self.rotation)
            .map(|(x, y)| (x + self.x, y + self.y))
    }
}

/// Return ordered half-turn offsets for a normalized starting rotation in `0..4`.
pub(super) fn half_turn_kicks(from: u8) -> &'static [(i32, i32)] {
    // TETR.IO's six 180-degree tests use screen coordinates, e.g. (0, -1) kicks up.
    match from {
        0 => &[(0, 0), (0, -1), (1, -1), (-1, -1), (1, 0), (-1, 0)],
        1 => &[(0, 0), (1, 0), (1, -2), (1, -1), (0, -2), (0, -1)],
        2 => &[(0, 0), (0, 1), (-1, 1), (1, 1), (-1, 0), (1, 0)],
        _ => &[(0, 0), (-1, 0), (-1, -2), (-1, -1), (0, -2), (0, -1)],
    }
}

// SRS+ uses screen coordinates; preserve osk's test order so mirrored I-spins agree.
// Reference: https://tetris.wiki/images/7/7d/TETR.IO_SRS%2Bkicks.png
/// Return ordered SRS+ offsets for adjacent normalized quarter-turn rotations in `0..4`.
pub(super) fn kicks(kind: Piece, from: u8, to: u8) -> [(i32, i32); 5] {
    if kind == Piece::I {
        match (from, to) {
            (0, 1) => [(0, 0), (1, 0), (-2, 0), (-2, 1), (1, -2)],
            (0, 3) => [(0, 0), (-1, 0), (2, 0), (2, 1), (-1, -2)],
            (1, 0) => [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
            (1, 2) => [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
            (2, 1) => [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
            (2, 3) => [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
            (3, 2) => [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
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
