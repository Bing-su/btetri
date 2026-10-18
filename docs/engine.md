# Engine and library

[README](../README.md) · [Playing btetri](usage.md) · [Development](development.md)

## Library usage

```toml
[dependencies]
btetri = { version = "0.1", default-features = false }
```

```rust
use std::time::Duration;
use btetri::game::{Action, Game, Mode};

let mut game = Game::new(Mode::FortyLines, 42);
game.apply(Action::Left);
game.advance(Duration::from_millis(16));
game.apply(Action::HardDrop);
println!("Score: {}", game.score());
```

| Interface                                          | Contract                                                                            |
| -------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `Game::new(mode, seed)`                            | Infallible; the same seed, inputs, and elapsed time produce the same state          |
| `str::parse::<Mode>()`                             | Accepts `40l`, `blitz`, or `practice`; otherwise returns `InvalidMode`              |
| `Game::apply(action)`                              | Apply one input; `Action::SoftDrop` moves one cell                                  |
| `Game::advance(duration)`                          | Advance caller-supplied time without I/O                                            |
| `Game::set_soft_drop(pressed)`                     | Set held soft-drop state                                                            |
| `Game::board()` / `active()` / `next()` / `held()` | Read board, active piece, previews, and hold                                        |
| `Game::score()` / `last_score()` / `status()`      | Read score, latest lock award, and outcome                                          |
| `Game::remaining()`                                | Remaining BLITZ time; `None` for untimed modes                                      |
| `Game::practice_resets()`                          | Automatic resets during the practice session                                        |
| `cli` feature (default)                            | Builds the terminal game; disable to omit terminal, SQLite, and miette dependencies |

## Rules

| Rule              | Behavior                                                                                                                                             |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pieces            | Seeded seven-bag generation; hold and ghost support                                                                                                  |
| Board             | 10 columns; 20 hidden + 20 visible rows; coordinates include hidden rows                                                                             |
| Visible rows      | Use `HIDDEN` and `VISIBLE_HEIGHT` to select rows from `board()`                                                                                      |
| Spawn             | Spawn above the visible board; descend one row immediately if free                                                                                   |
| Top-out           | Spawn overlap, or locking entirely above the visible board without clearing a line                                                                   |
| Clutch clear      | A line clear prevents hidden lock-out and may lift the next spawn to a free position; holds receive no rescue; cells cannot exceed the hidden buffer |
| Lock delay        | 500 ms after contact; 15 successful moves/rotations, including airborne moves after contact                                                          |
| Lock reset        | A new lowest row restores timer and move budget; returning to an old height does not; an exhausted budget locks on contact                           |
| 90° rotation      | SRS; I-piece uses SRS+ kick order                                                                                                                    |
| 180° rotation     | TETR.IO kicks                                                                                                                                        |
| BLITZ progression | Start at level 1; each level needs `ceil(2.1 × level)` more lines: cumulative 3, 8, 15, …                                                            |
| BLITZ gravity     | `(0.65 − 0.007 × (level − 1))^(level − 1)` seconds per row; capped at 20G                                                                            |
| Other modes       | Level 1 and fixed 0.02G                                                                                                                              |

## Scoring

| Action                             | Base points                                            |
| ---------------------------------- | ------------------------------------------------------ |
| Ordinary 1 / 2 / 3 / 4 lines       | 100 / 300 / 500 / 800                                  |
| T-spin, 0 / 1 / 2 / 3 lines        | 400 / 800 / 1200 / 1600                                |
| Mini spin, 0 / 1 / 2 / 3 / 4 lines | 100 / 200 / 400 / 800 / 1600                           |
| Back-to-Back (B2B)                 | ×1.5 on consecutive difficult clears                   |
| Combo                              | +50 × combo count; first clear is 0                    |
| Perfect clear                      | +3500 after clearing every cell, including hidden rows |
| Soft / hard drop                   | 1 / 2 per cell; no multipliers                         |

| Rule                    | Behavior                                                                                                                                            |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Level multiplier        | Apply the pre-clear level to clear, combo, and perfect-clear points                                                                                 |
| Difficult clears        | Tetrises and spin/Mini line clears                                                                                                                  |
| Empty lock              | Reset combo; preserve B2B without awarding its bonus                                                                                                |
| Ordinary 1–3-line clear | Break B2B                                                                                                                                           |
| Perfect-clear bonus     | Never multiplied by B2B                                                                                                                             |
| T-spin                  | Last successful move is rotation; at least three occupied T corners; walls/floor count                                                              |
| Full / Mini T-spin      | Both front corners filled, or fifth 90° SRS kick: full; otherwise Mini                                                                              |
| All-Mini+               | In 40 LINES / PRACTICE, an immobile rotated piece qualifies as Mini; T corner classification takes precedence; O rotation counts                    |
| Rotation history        | Successful translation/gravity or positive-distance drop cancels it; blocked actions and zero-distance hard drops preserve it; spawn/hold resets it |
| Score panel             | Latest lock award; `C1` means combo 1; excludes drop points                                                                                         |

| Level-1 example                  | Points, excluding drops            |
| -------------------------------- | ---------------------------------- |
| Two consecutive tetrises         | 800, then 1200 + 50 = 1250         |
| Two consecutive perfect tetrises | 4300, then 1200 + 3500 + 50 = 4750 |

References: [solo scoring and BLITZ](https://tetris.wiki/TETR.IO), [SRS+ I kicks](https://tetris.wiki/images/7/7d/TETR.IO_SRS%2Bkicks.png), [180° kicks](https://tetris.wiki/images/5/52/TETR.IO_180kicks.png).
