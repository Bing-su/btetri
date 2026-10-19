//! Held movement owns repeat timing; screen transitions only clear or drive it.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::config::Config;
use crate::game::{Action, Game, Status};

#[derive(Default)]
pub(super) struct HeldInput {
    soft_drop_keys: Vec<KeyCode>,
    horizontal_keys: Vec<(KeyCode, Action)>,
    horizontal_wait: Duration,
}

impl HeldInput {
    /// Advance held movement alongside gravity so slow frames preserve input order.
    pub(super) fn advance(&mut self, game: &mut Game, config: &Config, mut duration: Duration) {
        // Interleave repeats with game time, e.g. a slow frame must not drop before moving.
        while !duration.is_zero() && game.status() == Status::Playing {
            let horizontal = self.horizontal_keys.last().map(|&(_, action)| action);
            let mut step = duration.min(Duration::from_millis(10));
            if horizontal.is_some() {
                step = step.min(self.horizontal_wait);
            }
            game.advance(step);
            duration = duration.saturating_sub(step);
            self.horizontal_wait = self.horizontal_wait.saturating_sub(step);
            if let Some(action) = horizontal
                && self.horizontal_wait.is_zero()
                && game.status() == Status::Playing
            {
                repeat_action(game, action, config.arr_ms == 0);
                self.horizontal_wait = Duration::from_millis(config.arr_ms.max(1));
            }
        }
    }

    /// Apply a fresh movement press; additional soft-drop bindings share one timer.
    pub(super) fn press(
        &mut self,
        game: &mut Game,
        config: &Config,
        key: KeyEvent,
        action: Action,
    ) {
        let code = normalized_code(key.code);
        if key.kind != KeyEventKind::Press
            || self.horizontal_keys.iter().any(|&(held, _)| held == code)
            || self.soft_drop_keys.contains(&code)
        {
            return;
        }
        if action == Action::SoftDrop && !self.soft_drop_keys.is_empty() {
            self.soft_drop_keys.push(code);
            return;
        }
        if action == Action::SoftDrop {
            self.soft_drop_keys.push(code);
        } else {
            self.horizontal_keys.push((code, action));
            self.horizontal_wait = Duration::from_millis(config.das_ms);
        }
        if action == Action::SoftDrop {
            game.set_soft_drop_interval(Duration::from_millis(config.soft_drop_ms));
            game.set_soft_drop(true);
        }
        repeat_action(
            game,
            action,
            action == Action::SoftDrop && config.soft_drop_ms == 0
                || action != Action::SoftDrop && config.das_ms == 0 && config.arr_ms == 0,
        );
    }

    /// Release a physical key, restarting DAS only when the newest direction changes.
    pub(super) fn release(&mut self, game: Option<&mut Game>, config: &Config, code: KeyCode) {
        let code = normalized_code(code);
        let previous = self.horizontal_keys.last().copied();
        self.horizontal_keys.retain(|&(held, _)| held != code);
        if self.horizontal_keys.last().copied() != previous {
            self.horizontal_wait = Duration::from_millis(config.das_ms);
        }
        self.soft_drop_keys.retain(|&held| held != code);
        if let Some(game) = game {
            game.set_soft_drop(!self.soft_drop_keys.is_empty());
        }
    }

    /// Stop movement after focus loss or a dialog may have swallowed key releases.
    pub(super) fn clear(&mut self, game: Option<&mut Game>) {
        *self = Self::default();
        if let Some(game) = game {
            game.set_soft_drop(false);
        }
    }
}

/// Match presses and releases even when Shift changes a letter's case.
pub(super) fn normalized_code(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        code => code,
    }
}

fn repeat_action(game: &mut Game, action: Action, instant: bool) {
    let limit = if instant { crate::game::HEIGHT } else { 1 };
    let board = instant.then(|| *game.board());
    for _ in 0..limit {
        let before = game.active();
        game.apply(action);
        let after = game.active();
        // Stop at an obstacle or a newly spawned piece, e.g. an exhausted lock delay.
        let moved = match action {
            Action::Left => after.x == before.x - 1 && after.y == before.y,
            Action::Right => after.x == before.x + 1 && after.y == before.y,
            Action::SoftDrop => after.y == before.y + 1 && after.x == before.x,
            _ => false,
        };
        if !moved
            || game.status() != Status::Playing
            || board.as_ref().is_some_and(|before| before != game.board())
        {
            break;
        }
    }
}
