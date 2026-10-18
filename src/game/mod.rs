mod mode;
mod piece;
mod scoring;

use std::collections::VecDeque;
use std::time::Duration;

pub use mode::{InvalidMode, Mode};
pub use piece::{ActivePiece, Piece};
use rand::seq::SliceRandom;
use rand_philox::Philox;
use scoring::Scoring;
pub use scoring::{ScoreEvent, Spin};

pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 40;
pub const HIDDEN: usize = 20;
pub const VISIBLE_HEIGHT: usize = HEIGHT - HIDDEN;
const LOCK_DELAY: Duration = Duration::from_millis(500);
const BLITZ_LIMIT: Duration = Duration::from_secs(120);

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
    score: u64,
    scoring: Scoring,
    last_move: LastMove,
    lines: u32,
    pieces: u64,
    practice_resets: u32,
    elapsed: Duration,
    gravity_elapsed: Duration,
    grounded_elapsed: Duration,
    lock_resets: u8,
    lowest_y: i32,
    lock_started: bool,
    soft_drop: bool,
    soft_drop_interval: Option<Duration>,
    status: Status,
}

impl Game {
    pub fn new(mode: Mode, seed: u64) -> Self {
        let mut game = Self {
            board: [[None; WIDTH]; HEIGHT],
            active: ActivePiece::spawn(Piece::I),
            held: None,
            hold_available: true,
            queue: VecDeque::new(),
            rng: Philox::from_u64_seed(seed),
            mode,
            score: 0,
            scoring: Scoring::default(),
            last_move: LastMove::Translation,
            lines: 0,
            pieces: 0,
            practice_resets: 0,
            elapsed: Duration::ZERO,
            gravity_elapsed: Duration::ZERO,
            grounded_elapsed: Duration::ZERO,
            lock_resets: 0,
            lowest_y: 0,
            lock_started: false,
            soft_drop: false,
            soft_drop_interval: None,
            status: Status::Playing,
        };
        game.spawn_next(false);
        game
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

    /// Locked pieces in this session; for example, holding a piece does not count.
    pub fn pieces(&self) -> u64 {
        self.pieces
    }

    pub fn level(&self) -> u32 {
        self.mode.level(self.lines)
    }

    pub fn practice_resets(&self) -> u32 {
        self.practice_resets
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

    /// Override held soft-drop speed; for example, 30 ms moves a row every 30 ms.
    /// Zero drops to the floor on the next tick. Normal gravity is always the minimum speed.
    pub fn set_soft_drop_interval(&mut self, interval: Duration) {
        if self.soft_drop_interval != Some(interval) {
            self.soft_drop_interval = Some(interval);
            if self.soft_drop {
                self.gravity_elapsed = Duration::ZERO;
            }
        }
    }

    pub fn remaining(&self) -> Option<Duration> {
        (self.mode == Mode::Blitz).then(|| BLITZ_LIMIT.saturating_sub(self.elapsed))
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
                    self.reset_lock_at_new_low();
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
        self.update_lock(Duration::ZERO);
        self.recover_practice();
    }

    pub fn advance(&mut self, mut duration: Duration) {
        // Small steps preserve gravity/lock ordering after a slow frame, e.g. a resize.
        while !duration.is_zero() && self.status == Status::Playing {
            let mut step = duration.min(Duration::from_millis(10));
            if self.mode == Mode::Blitz {
                step = step.min(BLITZ_LIMIT.saturating_sub(self.elapsed));
            }
            self.elapsed += step;
            duration = duration.saturating_sub(step);
            if self.mode == Mode::Blitz && self.elapsed >= BLITZ_LIMIT {
                self.status = Status::Complete;
                break;
            }
            self.advance_piece(step);
        }
    }

    fn advance_piece(&mut self, step: Duration) {
        self.gravity_elapsed += step;
        let interval = self.gravity_interval();
        // Zero drains the free rows this tick; an obstacle ends the loop, e.g. the floor.
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
            self.reset_lock_at_new_low();
            self.last_move = LastMove::Translation;
            if self.soft_drop {
                self.score += 1;
            }
        }
        self.update_lock(step);
        self.recover_practice();
    }

    fn recover_practice(&mut self) {
        if self.mode == Mode::Practice && self.status == Status::TopOut {
            // Keep session statistics and the seeded bag sequence, e.g. after a failed hold.
            self.board = [[None; WIDTH]; HEIGHT];
            self.held = None;
            self.scoring = Scoring::default();
            self.practice_resets += 1;
            self.status = Status::Playing;
            self.spawn_next(false);
        }
    }

    fn reset_lock_at_new_low(&mut self) {
        // Only a new lowest row restores the budget, e.g. falling back after an upward kick does not.
        if self.active.y > self.lowest_y {
            self.lowest_y = self.active.y;
            self.grounded_elapsed = Duration::ZERO;
            self.lock_resets = 0;
        }
    }

    fn update_lock(&mut self, elapsed: Duration) {
        if self.status == Status::Playing && self.grounded() {
            self.lock_started = true;
            self.grounded_elapsed += elapsed;
            if self.lock_resets >= 15 || self.grounded_elapsed >= LOCK_DELAY {
                self.lock();
            }
        }
    }

    fn gravity_interval(&self) -> Duration {
        let seconds = self.mode.gravity_seconds(self.level());
        let gravity = Duration::from_secs_f64(seconds);
        if self.soft_drop {
            self.soft_drop_interval
                .unwrap_or_else(|| Duration::from_secs_f64(seconds / 20.0))
                .min(gravity)
        } else {
            gravity
        }
    }

    fn occupied(&self, x: i32, y: i32) -> bool {
        if x < 0
            || x >= i32::try_from(WIDTH).unwrap_or_default()
            || y < 0
            || y >= i32::try_from(HEIGHT).unwrap_or_default()
        {
            return true;
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
        self.lock_started |= self.grounded();
        if self.lock_started && self.lock_resets < 15 {
            self.grounded_elapsed = Duration::ZERO;
            self.lock_resets += 1;
        }
        self.active = candidate;
        self.reset_lock_at_new_low();
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
            self.moved(self.active);
            self.last_move = LastMove::Rotation { kick: 0 };
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
            self.spawn(held, false);
        } else {
            self.spawn_next(false);
        }
        self.held = Some(old);
        self.hold_available = false;
    }

    fn spawn_next(&mut self, clutch_clear: bool) {
        if self.queue.len() <= 7 {
            let mut bag = Piece::ALL;
            bag.shuffle(&mut self.rng);
            self.queue.extend(bag);
        }
        if let Some(kind) = self.queue.pop_front() {
            self.spawn(kind, clutch_clear);
        }
    }

    fn spawn(&mut self, kind: Piece, clutch_clear: bool) {
        self.active = ActivePiece::spawn(kind);
        // A line clear earns a higher spawn, e.g. when the stack still blocks the normal cells.
        while clutch_clear && !self.fits(self.active) {
            let raised = ActivePiece {
                y: self.active.y - 1,
                ..self.active
            };
            if raised.cells().iter().any(|&(_, y)| y < 0) {
                break;
            }
            self.active = raised;
        }
        self.last_move = LastMove::Translation;
        self.gravity_elapsed = Duration::ZERO;
        self.grounded_elapsed = Duration::ZERO;
        self.lock_resets = 0;
        self.lowest_y = self.active.y;
        self.lock_started = false;
        self.hold_available = true;
        if !self.fits(self.active) {
            self.status = Status::TopOut;
        } else {
            // A blocked initial descent keeps the piece above the skyline, e.g. on a high stack.
            let lowered = ActivePiece {
                y: self.active.y + 1,
                ..self.active
            };
            if self.fits(lowered) {
                self.active = lowered;
                self.lowest_y = self.active.y;
            }
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
        self.pieces += 1;
        let cleared = self.clear_lines();
        let perfect_clear = cleared > 0 && self.board.iter().flatten().all(Option::is_none);
        self.score += self
            .scoring
            .award(spin, cleared, self.level(), perfect_clear)
            .points;
        self.lines += cleared;
        if self.mode == Mode::FortyLines && self.lines >= 40 {
            self.status = Status::Complete;
        } else if locked_out && cleared == 0 {
            self.status = Status::TopOut;
        } else {
            self.spawn_next(cleared > 0);
        }
    }

    fn spin(&self) -> Spin {
        let LastMove::Rotation { kick } = self.last_move else {
            return Spin::None;
        };
        let immobile = self.mode != Mode::Blitz
            && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .all(|(dx, dy)| {
                    !self.fits(ActivePiece {
                        x: self.active.x + dx,
                        y: self.active.y + dy,
                        ..self.active
                    })
                });
        if self.active.kind != Piece::T {
            return if immobile { Spin::Mini } else { Spin::None };
        }
        let ActivePiece { x, y, rotation, .. } = self.active;
        let corners = [
            self.occupied(x, y),
            self.occupied(x + 2, y),
            self.occupied(x, y + 2),
            self.occupied(x + 2, y + 2),
        ];
        if corners.into_iter().filter(|&occupied| occupied).count() < 3 {
            return if immobile { Spin::Mini } else { Spin::None };
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
