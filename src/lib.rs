//! Deterministic Tetris engine, independent of terminal input, rendering, and storage.
//!
//! Supply a seed and elapsed time to drive the game from your own frontend:
//!
//! ```
//! use std::time::Duration;
//! use btetri::game::{Action, Game, Rules};
//!
//! let mut game = Game::new("marathon".parse()?, Rules::default(), 42)?;
//! game.apply(Action::Left);
//! game.advance(Duration::from_millis(16));
//! game.apply(Action::HardDrop);
//! assert!(game.score() > 0);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod game;
