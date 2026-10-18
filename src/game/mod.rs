mod piece;
mod scoring;

use std::collections::VecDeque;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

pub use piece::{ActivePiece, Piece};
use rand::seq::SliceRandom;
use rand_philox::Philox;
use scoring::Scoring;
pub use scoring::{ScoreEvent, Spin};

pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 22;
pub const HIDDEN: usize = 2;
pub const VISIBLE_HEIGHT: usize = HEIGHT - HIDDEN;
const LOCK_DELAY: Duration = Duration::from_millis(500);
const ULTRA_LIMIT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Marathon,
    Sprint,
    Ultra,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Marathon, Self::Sprint, Self::Ultra];

    pub fn cycle(self, delta: i32) -> Self {
        let current = match self {
            Self::Marathon => 0,
            Self::Sprint => 1,
            Self::Ultra => 2,
        };
        match (current + delta).rem_euclid(3) {
            0 => Self::Marathon,
            1 => Self::Sprint,
            _ => Self::Ultra,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Marathon => "marathon",
            Self::Sprint => "sprint",
            Self::Ultra => "ultra",
        })
    }
}

impl FromStr for Mode {
    type Err = InvalidMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "marathon" => Ok(Self::Marathon),
            "sprint" => Ok(Self::Sprint),
            "ultra" => Ok(Self::Ultra),
            _ => Err(InvalidMode),
        }
    }
}

/// A mode name was rejected; use `marathon`, `sprint`, or `ultra`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected marathon, sprint, or ultra")]
pub struct InvalidMode;

#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub start_level: u32,
    pub max_level: u32,
    pub end_on_max_level: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            start_level: 1,
            max_level: 20,
            end_on_max_level: false,
        }
    }
}

/// Game creation rejected the level range; for example, level zero is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("levels must satisfy 1 <= start level <= maximum level <= 20")]
pub struct InvalidLevel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Left,
    Right,
    Clockwise,
    CounterClockwise,
    Rotate180,
    HardDrop,
    SoftDrop,
    Hold,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Playing,
    TopOut,
    Complete,
}

#[derive(Clone, Copy)]
enum LastMove {
    Translation,
    Rotation { kick: usize },
}

pub struct Game {
    board: [[Option<Piece>; WIDTH]; HEIGHT],
    active: ActivePiece,
    held: Option<Piece>,
    hold_available: bool,
    queue: VecDeque<Piece>,
    rng: Philox,
    mode: Mode,
    rules: Rules,
    score: u64,
    scoring: Scoring,
    last_move: LastMove,
    lines: u32,
    elapsed: Duration,
    gravity_elapsed: Duration,
    grounded_elapsed: Duration,
    lock_resets: u8,
    soft_drop: bool,
    status: Status,
}

impl Game {
    pub fn new(mode: Mode, rules: Rules, seed: u64) -> Result<Self, InvalidLevel> {
        if rules.start_level == 0 || rules.start_level > rules.max_level || rules.max_level > 20 {
            return Err(InvalidLevel);
        }
        let mut game = Self {
            board: [[None; WIDTH]; HEIGHT],
            active: ActivePiece::spawn(Piece::I),
            held: None,
            hold_available: true,
            queue: VecDeque::new(),
            rng: Philox::from_u64_seed(seed),
            mode,
            rules,
            score: 0,
            scoring: Scoring::default(),
            last_move: LastMove::Translation,
            lines: 0,
            elapsed: Duration::ZERO,
            gravity_elapsed: Duration::ZERO,
            grounded_elapsed: Duration::ZERO,
            lock_resets: 0,
            soft_drop: false,
            status: Status::Playing,
        };
        game.spawn_next();
        Ok(game)
    }

    pub fn board(&self) -> &[[Option<Piece>; WIDTH]; HEIGHT] {
        &self.board
    }

    pub fn active(&self) -> ActivePiece {
        self.active
    }

    pub fn held(&self) -> Option<Piece> {
        self.held
    }

    pub fn next(&self) -> impl Iterator<Item = Piece> + '_ {
        self.queue.iter().copied()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn score(&self) -> u64 {
        self.score
    }

    pub fn last_score(&self) -> Option<ScoreEvent> {
        self.scoring.last
    }

    pub fn lines(&self) -> u32 {
        self.lines
    }

    pub fn level(&self) -> u32 {
        (self.rules.start_level + self.lines / 10).min(self.rules.max_level)
    }

    pub fn start_level(&self) -> u32 {
        self.rules.start_level
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn soft_drop(&self) -> bool {
        self.soft_drop
    }

    pub fn set_soft_drop(&mut self, pressed: bool) {
        if self.soft_drop != pressed {
            self.soft_drop = pressed;
            self.gravity_elapsed = Duration::ZERO;
        }
    }

    pub fn remaining(&self) -> Duration {
        ULTRA_LIMIT.saturating_sub(self.elapsed)
    }

    pub fn ghost(&self) -> ActivePiece {
        let mut ghost = self.active;
        while self.fits(ActivePiece {
            y: ghost.y + 1,
            ..ghost
        }) {
            ghost.y += 1;
        }
        ghost
    }

    pub fn apply(&mut self, action: Action) {
        if self.status != Status::Playing {
            return;
        }
        match action {
            Action::Left => self.shift(-1),
            Action::Right => self.shift(1),
            Action::Clockwise => self.rotate(1),
            Action::CounterClockwise => self.rotate(3),
            Action::Rotate180 => self.rotate(2),
            Action::SoftDrop => {
                let candidate = ActivePiece {
                    y: self.active.y + 1,
                    ..self.active
                };
                if self.fits(candidate) {
                    self.active = candidate;
                    self.last_move = LastMove::Translation;
                    self.score += 1;
                }
            }
            Action::HardDrop => {
                let ghost = self.ghost();
                self.score += u64::try_from(ghost.y - self.active.y).unwrap_or_default() * 2;
                // A zero-distance drop locks a spin without changing its last maneuver.
                if ghost.y != self.active.y {
                    self.last_move = LastMove::Translation;
                }
                self.active = ghost;
                self.lock();
            }
            Action::Hold => self.hold(),
        }
    }

    pub fn advance(&mut self, mut duration: Duration) {
        // Small steps preserve gravity/lock ordering after a slow frame, e.g. a resize.
        while !duration.is_zero() && self.status == Status::Playing {
            let mut step = duration.min(Duration::from_millis(10));
            if self.mode == Mode::Ultra {
                step = step.min(self.remaining());
            }
            self.elapsed += step;
            duration = duration.saturating_sub(step);
            if self.mode == Mode::Ultra && self.elapsed >= ULTRA_LIMIT {
                self.status = Status::Complete;
                break;
            }
            self.advance_piece(step);
        }
    }

    fn advance_piece(&mut self, step: Duration) {
        self.gravity_elapsed += step;
        let interval = self.gravity_interval();
        while self.gravity_elapsed >= interval {
            self.gravity_elapsed -= interval;
            let lowered = ActivePiece {
                y: self.active.y + 1,
                ..self.active
            };
            if !self.fits(lowered) {
                break;
            }
            self.active = lowered;
            self.last_move = LastMove::Translation;
            if self.soft_drop {
                self.score += 1;
            }
        }
        if self.grounded() {
            self.grounded_elapsed += step;
            if self.grounded_elapsed >= LOCK_DELAY {
                self.lock();
            }
        }
    }

    fn gravity_interval(&self) -> Duration {
        let level = f64::from(self.level() - 1);
        let seconds = (0.8 - level * 0.007).powf(level).max(0.001);
        Duration::from_secs_f64(if self.soft_drop {
            seconds / 20.0
        } else {
            seconds
        })
    }

    fn occupied(&self, x: i32, y: i32) -> bool {
        if x < 0
            || x >= i32::try_from(WIDTH).unwrap_or_default()
            || y >= i32::try_from(HEIGHT).unwrap_or_default()
        {
            return true;
        }
        if y < 0 {
            return false;
        }
        usize::try_from(y)
            .ok()
            .and_then(|row| self.board.get(row))
            .and_then(|row| usize::try_from(x).ok().and_then(|column| row.get(column)))
            .is_none_or(Option::is_some)
    }

    fn fits(&self, active: ActivePiece) -> bool {
        active
            .cells()
            .into_iter()
            .all(|(x, y)| !self.occupied(x, y))
    }

    fn grounded(&self) -> bool {
        !self.fits(ActivePiece {
            y: self.active.y + 1,
            ..self.active
        })
    }

    fn moved(&mut self, candidate: ActivePiece) {
        if self.grounded() && self.lock_resets < 15 {
            self.grounded_elapsed = Duration::ZERO;
            self.lock_resets += 1;
        }
        self.active = candidate;
    }

    fn shift(&mut self, x: i32) {
        let candidate = ActivePiece {
            x: self.active.x + x,
            ..self.active
        };
        if self.fits(candidate) {
            self.moved(candidate);
            self.last_move = LastMove::Translation;
        }
    }

    fn rotate(&mut self, delta: u8) {
        if self.active.kind == Piece::O {
            return;
        }
        let rotation = (self.active.rotation + delta) % 4;
        let kicks = if delta == 2 {
            piece::half_turn_kicks(self.active.rotation)
        } else {
            &piece::kicks(self.active.kind, self.active.rotation, rotation)
        };
        for (kick, &(x, y)) in kicks.iter().enumerate() {
            // Only the fifth 90-degree SRS test promotes a Mini, e.g. a T-spin double.
            let kick = if delta == 2 { 0 } else { kick };
            let candidate = ActivePiece {
                x: self.active.x + x,
                y: self.active.y + y,
                rotation,
                ..self.active
            };
            if self.fits(candidate) {
                self.moved(candidate);
                self.last_move = LastMove::Rotation { kick };
                break;
            }
        }
    }

    fn hold(&mut self) {
        if !self.hold_available {
            return;
        }
        let old = self.active.kind;
        if let Some(held) = self.held {
            self.spawn(held);
        } else {
            self.spawn_next();
        }
        self.held = Some(old);
        self.hold_available = false;
    }

    fn spawn_next(&mut self) {
        if self.queue.len() <= 7 {
            let mut bag = Piece::ALL;
            bag.shuffle(&mut self.rng);
            self.queue.extend(bag);
        }
        if let Some(kind) = self.queue.pop_front() {
            self.spawn(kind);
        }
    }

    fn spawn(&mut self, kind: Piece) {
        self.active = ActivePiece::spawn(kind);
        self.last_move = LastMove::Translation;
        self.gravity_elapsed = Duration::ZERO;
        self.grounded_elapsed = Duration::ZERO;
        self.lock_resets = 0;
        self.hold_available = true;
        if !self.fits(self.active) {
            self.status = Status::TopOut;
        }
    }

    fn lock(&mut self) {
        let spin = self.spin();
        let hidden = i32::try_from(HIDDEN).unwrap_or_default();
        let locked_out = self.active.cells().into_iter().all(|(_, y)| y < hidden);
        for (x, y) in self.active.cells() {
            let cell = usize::try_from(y)
                .ok()
                .and_then(|row| self.board.get_mut(row))
                .and_then(|row| {
                    usize::try_from(x)
                        .ok()
                        .and_then(|column| row.get_mut(column))
                });
            if let Some(cell) = cell {
                *cell = Some(self.active.kind);
            } else {
                self.status = Status::TopOut;
                return;
            }
        }
        let cleared = self.clear_lines();
        let perfect_clear = cleared > 0 && self.board.iter().flatten().all(Option::is_none);
        self.score += self
            .scoring
            .award(spin, cleared, self.level(), perfect_clear)
            .points;
        self.lines += cleared;
        if (self.mode == Mode::Sprint && self.lines >= 40)
            || (self.mode == Mode::Marathon
                && self.rules.end_on_max_level
                && self.level() >= self.rules.max_level)
        {
            self.status = Status::Complete;
        } else if locked_out {
            self.status = Status::TopOut;
        } else {
            self.spawn_next();
        }
    }

    fn spin(&self) -> Spin {
        let LastMove::Rotation { kick } = self.last_move else {
            return Spin::None;
        };
        if self.active.kind != Piece::T {
            return Spin::None;
        }
        let ActivePiece { x, y, rotation, .. } = self.active;
        let corners = [
            self.occupied(x, y),
            self.occupied(x + 2, y),
            self.occupied(x, y + 2),
            self.occupied(x + 2, y + 2),
        ];
        if corners.into_iter().filter(|&occupied| occupied).count() < 3 {
            return Spin::None;
        }
        let [top_left, top_right, bottom_left, bottom_right] = corners;
        let front_filled = match rotation {
            0 => top_left && top_right,
            1 => top_right && bottom_right,
            2 => bottom_left && bottom_right,
            _ => top_left && bottom_left,
        };
        // The fifth SRS test's 1-by-2 kick promotes a Mini to a full T-spin.
        if front_filled || kick == 4 {
            Spin::Full
        } else {
            Spin::Mini
        }
    }

    fn clear_lines(&mut self) -> u32 {
        let mut packed = [[None; WIDTH]; HEIGHT];
        let mut write = HEIGHT;
        for row in self
            .board
            .iter()
            .rev()
            .filter(|row| row.iter().any(Option::is_none))
        {
            write -= 1;
            if let Some(destination) = packed.get_mut(write) {
                *destination = *row;
            }
        }
        self.board = packed;
        u32::try_from(write).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
