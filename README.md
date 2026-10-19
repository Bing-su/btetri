# btetri

A deterministic Tetris library and keyboard-controlled terminal game inspired by [tetrigo](https://github.com/Broderick-Westrope/tetrigo).

Requires Rust 1.95 or newer, a C compiler for the CLI build, and an interactive terminal of at least **80 columns × 28 rows**.

```sh
cargo run
cargo run -- play 40l --name Alice
cargo run -- records 40l
```

| Mode                      | Goal                                                                |
| ------------------------- | ------------------------------------------------------------------- |
| 40 LINES (`40l`, default) | Clear 40 lines as fast as possible                                  |
| BLITZ (`blitz`)           | Score as much as possible in two minutes                            |
| MARATHON (`marathon`)     | Survive increasing gravity; level up every 10 lines, up to level 20 |
| PRACTICE (`practice`)     | Play without a time limit; the board resets after top-out           |

Select SETTINGS to change controls and colors. Settings and records are stored locally in SQLite; `--db` selects the database file.

| Documentation                        | Contents                                                          |
| ------------------------------------ | ----------------------------------------------------------------- |
| [Playing btetri](docs/usage.md)      | Commands, controls, settings, records, and database compatibility |
| [Engine and library](docs/engine.md) | Library usage, game rules, and scoring                            |
| [Development](docs/development.md)   | Architecture, runtime, and verification commands                  |
| [Glossary](GLOSSARY.md)              | Domain terms                                                      |
