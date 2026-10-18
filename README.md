# btetri

A deterministic Tetris library and keyboard-controlled terminal game inspired by [tetrigo](https://github.com/Broderick-Westrope/tetrigo).

```sh
cargo run
cargo run -- play 40l --name Alice
cargo run -- play blitz
cargo run -- play practice
cargo run -- records 40l
cargo run -- --db ./records.db
```

Requires Rust 1.95 or newer and an interactive terminal of at least **80 columns × 28 rows**. The application name is omitted from the TUI. The selected designs are in `design/`; layout uses real terminal cells, including two character columns per board square. Key hints use compact brackets, such as `[Enter]`.

| Mode     | Goal                                                                    | Records order                                                   |
| -------- | ----------------------------------------------------------------------- | --------------------------------------------------------------- |
| 40 LINES (`40l`, default) | Clear 40 lines at fixed 0.02G; All-Mini+ spins | Completed games first, fastest first; unfinished games by lines |
| BLITZ (`blitz`) | Score during 120 seconds; increasing level and gravity; T-spins only | Score descending, then shorter duration |
| PRACTICE (`practice`) | Unlimited fixed 0.02G; All-Mini+ spins; clear board after top-out | No ranked records |

BLITZ starts at level 1. Each level requires `ceil(2.1 × level)` additional lines: the first goals are 3, 5, and 7 lines (cumulative 3, 8, and 15). Gravity uses `(0.65 − 0.007 × (level − 1))^(level − 1)` seconds per row, capped at 20G. These goals and solo scoring follow [Tetris Wiki's TETR.IO description](https://tetris.wiki/TETR.IO#BLITZ).

PRACTICE is a local training mode: top-out clears the board, hold, and scoring chains while preserving total score, lines, elapsed time, and the seeded bag sequence. The sidebar counts resets. It does not implement TETR.IO ZEN's persistence or progression.

Breaking configuration change: `marathon`, `sprint`, `ultra`, `--level`, `start_level`, `max_level`, and `end_on_max_level` are removed. Previous database formats are discarded rather than imported.

| Key           | During play                                             |
| ------------- | ------------------------------------------------------- |
| ← / →         | Move left / right; terminal/OS repeat applies           |
| Z / Ctrl      | Rotate counterclockwise with SRS+ kicks                 |
| ↑ / X         | Rotate clockwise with SRS+ kicks                        |
| A             | Rotate 180 degrees with TETR.IO kicks                    |
| Space         | Hard drop and lock                                      |
| ↓             | Soft drop while held; legacy terminals: one cell/event |
| C / Shift     | Hold, once per locked piece                             |
| Esc           | Pause / resume                                          |
| ?             | Show controls and pause                                 |

The main keyboard bindings follow [TETR.IO's Guideline controls](https://tetr.io/). Rotation uses SRS+ with [osk's I-piece kick order](https://tetris.wiki/images/7/7d/TETR.IO_SRS%2Bkicks.png) and [180-degree kicks](https://tetris.wiki/images/5/52/TETR.IO_180kicks.png); other pieces retain their 90-degree SRS kicks. This changes some I-spin outcomes from the previous SRS behavior. Standalone Ctrl/Shift and key releases require terminal support; the runtime requests the enhanced keyboard protocol and focus events. On terminals without release events, Down moves one cell per press or OS repeat, without latching soft drop. Pause, help, and focus loss clear held soft drop. Horizontal repeat still follows the terminal/OS; TETR.IO's DAS/ARR settings and numpad-specific aliases are not implemented.

Key bindings can be changed in SETTINGS. Every action needs a binding, and duplicate assignments are rejected when saving; swap both actions before SAVE if necessary. The engine uses `Action::SoftDrop` (one cell) and `Game::set_soft_drop(bool)` (held state).

Select SETTINGS in the main menu to edit player, mode, ghost visibility, preview count, keys, and all palette colors. Up/Down navigates and Home/End jumps to the first/last item. Left/Right changes mode, ghost, or preview count; Enter edits a value. Enter accepts the field, Esc cancels the field, and Delete clears it. Key names are comma-separated (e.g. `UP, X`, `C, SHIFT`, or `COMMA`); colors accept names, `#RRGGBB`, or indexes 0–255. Press F2 or select SAVE to commit the draft, or Esc outside a field to discard it. RESET DEFAULTS only resets the draft until saved. Save errors retain the draft for retry. Menu player/mode changes save automatically; R retries failures.

TOML configuration files, `config.example.toml`, and `--config` are removed. Existing files are ignored and are not imported or deleted. Settings live in the same SQLite file as records, so `--db` selects both. The first launch uses defaults. CLI play arguments override the loaded settings for the session; an explicit settings save or menu change then persists the current choices.

Menu and records navigation uses arrow keys, Enter, and Escape. To exit, return to the main menu and select QUIT. Ctrl+C follows the configured C binding during play (hold by default). In the menu, select PLAYER and press Enter to edit; Delete clears, Enter commits, and Escape cancels. In records, Left/Right changes mode and Up/Down or PageUp/PageDown browses. Resizing below the minimum pauses the game; resume explicitly after resizing.

| Setting           | Behavior                                                                                                                       |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Configuration     | SETTINGS edits a validated draft; SQLite transactions save all settings together |
| Paths             | `directories::ProjectDirs` chooses the OS-standard data directory; `--db` overrides the database path |
| Persistence       | Bundled SQLite through rusqlite; completed-game conditions, result, duration, and UTC completion time are stored               |
| Shared database   | Multiple processes can open the same local DB; SQLite serializes writes, with a 5-second lock wait and retry on failure        |
| Dates             | Displayed in the local timezone                                                                                                |
| Save failure      | The result stays open; R retries with the same game ID, avoiding duplicate records                                             |
| Interrupted games | Returning to the menu from pause abandons the active game; no save-game/resume or input replay                                 |

The engine implements seven-bag generation with Philox, hold, ghost, gravity, soft/hard drops, line clears, and level progression. Lock delay is 500 ms: after first contact, successful moves and rotations use a 15-action budget, including while airborne after leaving a surface. Reaching a new lowest row restores the budget and timer; returning to an earlier height does not. Exhausting the budget locks immediately on contact.

The board has 20 visible rows and 20 hidden rows. Pieces spawn above the visible board and descend one row immediately only if that row is free; a blocked descent alone is not game over. [Clutch Clears](https://tetr.io/about/patchnotes/#chlog_BETA_1_5_0) forgive locking entirely above the visible board when lines are cleared, and lift the next piece to the first free spawn position if the stack blocks its normal position. The rescue applies only to the next piece after a line clear, not to holds, and cannot push cells above the hidden buffer. Without a line clear, spawn overlap or locking entirely above the visible board ends the game. Engine consumers should use `HIDDEN` and `VISIBLE_HEIGHT` to select visible rows: `Game::board()` now returns 40 rows instead of 22, and active-piece coordinates include the expanded hidden region.

| Scoring action                     | Base points before level multiplier                                       |
| ---------------------------------- | ------------------------------------------------------------------------- |
| Ordinary 1 / 2 / 3 / 4 lines       | 100 / 300 / 500 / 800                                                     |
| T-spin, 0 / 1 / 2 / 3 lines        | 400 / 800 / 1200 / 1600                                                   |
| Mini spin, 0 / 1 / 2 / 3 / 4 lines | 100 / 200 / 400 / 800 / 1600 |
| Back-to-Back                       | 1.5 × base points for a difficult clear following another difficult clear |
| Combo                              | +50 × combo count; first consecutive clear is count 0, second is 1        |
| Perfect clear | +3500, regardless of lines cleared |
| Soft / hard drop                   | 1 / 2 points per cell, without level, B2B, or combo multipliers           |

Clear and combo points use the level **before** removing lines. Only BLITZ advances levels; the other modes stay at level 1. Difficult clears are tetrises and spin/Mini line clears. A lock without a line clear resets the combo and preserves B2B without earning a B2B bonus; an ordinary 1–3-line clear breaks B2B. For example, two consecutive tetrises at level 1 award 800, then 1200 + 50 points.

Under the T corner rule, a T-spin requires a last successful rotation and at least three occupied corners around the T's center, checked before line removal. Walls/floor count as occupied. Both front corners filled gives a full T-spin; otherwise it is Mini, except that the fifth SRS kick promotes it to full. Successful horizontal movement, gravity, or a positive-distance drop cancels the rotation history; blocked actions and zero-distance hard drops preserve it. Spawn/hold starts fresh rotation history. The score panel shows the last award (`C1` means combo 1).

40 LINES and PRACTICE use All-Mini+: following a rotation, any piece that cannot move left, right, up, or down qualifies as a Mini, while T-pieces satisfying the corner rule retain their full/Mini classification. O rotation preserves its shape but counts as a rotation. BLITZ uses only the T corner rule. The [official patch notes](https://tetr.io/about/patchnotes/#chlog_BETA_1_7_6) document All-Mini+ in 40 LINES.

Perfect-clear bonuses follow the [TETR.IO solo scoring table](https://tetris.wiki/TETR.IO#Solo): after a line clear, the entire board including hidden rows must be empty. Add 3500 to clear and combo points, multiply by the pre-clear level, and never multiply the bonus by B2B. Two consecutive perfect tetrises at level 1 award 4300, then 1200 + 3500 + 50 = 4750, excluding drop points.

Terminal cleanup uses `scopeguard::defer!` to restore the terminal on scope exit, including error returns and unwinding panics. The runtime is synchronous: Crossterm polls input with a maximum 16 ms wait, and database operations finish before input handling continues. DB access normally occurs on menu, settings, result, and records screens; a slow disk or database lock can delay input on those screens. Pending settings and results are saved before shutdown, including after a terminal error. If a save fails, the shutdown diagnostic includes every unsaved item together with any terminal error.

The default CLI build compiles SQLite from bundled C sources and requires a C compiler; no separate SQLite installation is needed. Engine-only builds omit SQLite. One schema in `src/schema.sql` defines records and settings. On first open, database versions 0–2 have their records and settings tables replaced with schema version 3; old records and settings are deleted. Replacement is transactional. Reopening a version 3 database preserves its contents, and unsupported newer versions are rejected. Current-format tables are checked before play to reject missing columns or a missing unique ID. There is no import, migration, or downgrade support.

## Library usage

Use the engine without terminal, SQLite, or miette dependencies:

```toml
[dependencies]
btetri = { version = "0.1", default-features = false }
```

```rust
use std::time::Duration;
use btetri::game::{Action, Game, Mode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut game = Game::new(Mode::FortyLines, 42);
    game.apply(Action::Left);
    game.advance(Duration::from_millis(16));
    game.apply(Action::HardDrop);
    println!("Score: {}", game.score());
    Ok(())
}
```

| Public API | Contract |
| ---------- | -------- |
| `Game::new(mode, seed)` | Infallible deterministic game creation; the mode owns its rules |
| `Mode::from_str` / `str::parse::<Mode>()` | Accepts `40l`, `blitz`, or `practice`; returns `InvalidMode` on failure |
| `Game::apply`, `advance`, `set_soft_drop` | Apply input and caller-supplied elapsed time without I/O |
| Game accessors, `Piece`, `ActivePiece`, `ScoreEvent`, `Spin`, `Status` | Read board, previews, score, and game outcome for your frontend |
| `Game::remaining()` | `Some(time)` for BLITZ; `None` for untimed modes |
| `Game::practice_resets()` | Number of automatic board resets in this practice session |
| `InvalidMode` | Typed `thiserror` error implementing `std::error::Error` |
| `cli` feature (enabled by default) | Builds the `btetri` binary and enables its terminal, configuration, and storage dependencies |

Breaking engine Interface changes: `Rules`, `InvalidLevel`, `Game::start_level()`, and the old mode variants are removed. `Game::new` returns `Game` directly. `btetri::run()` remains removed; launch the binary to run the TUI.

| Module           | Responsibility                                                           |
| ---------------- | ------------------------------------------------------------------------ |
| `game`           | Deterministic rules and game state, independent of terminal and database |
| `app`            | Screen transitions, menu settings, input, and record save/load lifecycle |
| `ui`             | Screen selection, menu/help, and shared Ratatui layout                   |
| `ui/gameplay`    | Playfield, piece previews, score feedback, pause, and result dialogs     |
| `ui/records`     | Records table, selection visibility, and empty/error states              |
| `storage`        | SQLite schema initialization, idempotent record writes, and ranking      |
| `runtime`        | Terminal setup/cleanup, synchronous Crossterm polling, and game clock    |
| `config` / `cli` | Validated setting values, database path, and usage-rs arguments |
| `app/settings` / `ui/settings` | Draft editing, validation, save/cancel, and scrolling settings screen |
| `config/keys`    | Key parsing, matching, labels, and duplicate-binding validation          |
| `config/theme`   | Terminal palette parsed into colors when configuration loads             |

The library exposes only the deterministic `game` module. The binary owns screen, configuration, rendering, persistence, and the synchronous runtime; miette diagnostics stay in the binary. Configuration parses mode, key, and color strings once using `FromStr`; rendering uses validated colors directly. Menu input and rendering share named `MenuItem` choices and read or edit the same `Config`, without separate copies of player or mode. App initialization uses `TryFrom<Config>`, and save results use `From<Result<(), String>>`. App processes pending saves and record queries synchronously through the concrete `Store`, including retry, current-result selection, and unsaved-result diagnostics on shutdown. Record requests, rows, and failures are exclusive enum states; no loading callback is needed. Help is part of the screen state so gameplay cannot advance while it is open. Tokio, futures, and background database jobs are not used. Domain terms are listed in [GLOSSARY.md](GLOSSARY.md).

```sh
cargo +nightly fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo check --no-default-features --all-targets
cargo test --no-default-features
cargo clippy --no-default-features --all-targets -- -D warnings
```

Tests use rstest for examples, proptest for bag/board invariants, Ratatui TestBackend for screen states, and tempfile for real SQLite persistence/reopen, shared-connection, lock/retry, and schema tests. Linux, macOS, and Windows are intended targets; validation performed on one platform does not establish runtime behavior on the others.
