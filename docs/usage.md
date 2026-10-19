# Playing btetri

[README](../README.md) · [Engine and library](engine.md) · [Development](development.md)

## Commands

| Command                              | Action                                   |
| ------------------------------------ | ---------------------------------------- |
| `cargo run`                          | Open the main menu                       |
| `cargo run -- play 40l --name Alice` | Start 40 LINES as Alice                  |
| `cargo run -- play blitz`            | Start BLITZ                              |
| `cargo run -- play practice`         | Start PRACTICE                           |
| `cargo run -- records 40l`           | Browse 40 LINES records                  |
| `cargo run -- --db ./records.db`     | Choose the settings and records database |

## Modes

| Mode            | Rules                                                                    | Ranking                                               |
| --------------- | ------------------------------------------------------------------------ | ----------------------------------------------------- |
| `40l` (default) | Clear 40 lines; fixed 0.02G; All-Mini+ spins                             | Completed runs by time, then unfinished runs by lines |
| `blitz`         | Score for 120 seconds; increasing gravity; T-spins only                  | Score descending, then shorter duration               |
| `practice`      | Adjustable gravity (default 0.02G); All-Mini+ spins; reset after top-out | No records                                            |

Practice resets the board, hold, and scoring chains; score, lines, elapsed time, and bag sequence continue. The sidebar counts resets.

## Controls

| Key       | During play                                                 |
| --------- | ----------------------------------------------------------- |
| ← / →     | Move; held keys use DAS/ARR when key releases are supported |
| Z / Ctrl  | Rotate counterclockwise                                     |
| ↑ / X     | Rotate clockwise                                            |
| A         | Rotate 180 degrees                                          |
| Space     | Hard drop and lock                                          |
| ↓         | Soft drop                                                   |
| C / Shift | Hold, once per locked piece                                 |
| Esc       | Pause / resume                                              |
| ?         | Show help and pause                                         |

| Context         | Controls / behavior                                                                            |
| --------------- | ---------------------------------------------------------------------------------------------- |
| Menu            | ↑ / ↓ selects; Enter opens; QUIT exits                                                         |
| Player name     | Enter accepts; Esc cancels; Backspace deletes; Delete clears; limit: 16 characters             |
| Records         | ← / → changes mode; ↑ / ↓ or PgUp / PgDn browses; Esc returns                                  |
| Small terminal  | Below 80 × 28 cells pauses play; resume explicitly after resizing                              |
| Key releases    | Supported terminals use configured movement/drop timers; others move one cell per press/repeat |
| Modifier keys   | Standalone Ctrl/Shift require terminal support; Ctrl+C holds by default                        |
| Focus / dialogs | Focus loss, pause, and help clear held movement and soft drop                                  |

## Results

| Feature             | Contents / controls                                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| End-of-game summary | Score, cleared lines, and elapsed time after top-out or completion                                                                   |
| STATISTICS          | Separate screen opened from the result menu; Esc or Enter returns to the summary                                                     |
| Pieces placed       | Counts locked pieces, including the final lock; hold and an unplaced active piece do not count                                       |
| PPS                 | Pieces placed divided by active play time in seconds; zero when no time has elapsed                                                  |
| Personal best       | Previous records for the exact same player name and mode, excluding the current run                                                  |
| 40 LINES comparison | Previous fastest completed run; signed time difference after completion (negative is faster); unfinished runs cannot set a best time |
| BLITZ comparison    | Previous highest score, including top-outs; signed score difference (positive is higher)                                             |
| Query failure       | Statistics remain available; R retries loading the previous best                                                                     |
| Save failure        | Statistics remain available; return to the summary and press R to retry saving before leaving the result                             |

Piece count and PPS are available for the current result only. Existing stored records and settings keep their format; PRACTICE continues through automatic resets without an end-of-game screen.

## Settings

Open SETTINGS from the menu. Changes remain a draft until saved.

| Context                 | Controls                                             |
| ----------------------- | ---------------------------------------------------- |
| Navigation              | ↑ / ↓; Home / End jumps to first / last item         |
| Mode / ghost / previews | ← / → or Enter changes the value; previews: 1–6      |
| DAS / ARR / soft drop   | ← / → adjusts by 10 ms; range: 0–1000 ms             |
| PRACTICE SPEED          | ← / → adjusts by 10%; range: 10–1000%; default: 100% |
| Player                  | Enter opens the name editor                          |
| Save draft              | F2 outside an editor, or SAVE                        |
| Discard draft           | Esc outside an editor, or CANCEL                     |
| Reset draft             | RESET DEFAULTS; save to apply                        |
| Save failure            | Draft remains available; select SAVE to retry        |

PRACTICE SPEED scales automatic falling only in Practice, independently of terminal key-release support. For example, 50% is half speed and 200% is double speed. The saved speed applies when starting or restarting a game; automatic board resets retain it. Other modes keep their usual gravity.

| Input timing | Default | Behavior                                                                                                                  |
| ------------ | ------- | ------------------------------------------------------------------------------------------------------------------------- |
| DAS          | 150 ms  | First horizontal move is immediate; wait this long before repeating                                                       |
| ARR          | 30 ms   | Horizontal repeat interval after DAS; 0 moves to the wall                                                                 |
| SOFT DROP    | 30 ms   | First step is immediate, then repeat at this interval without DAS; 0 drops to the floor, preserving the normal lock delay |

For example, DAS 100 ms and ARR 20 ms move once on press, again at 100 ms, then at 120 ms, 140 ms, and so on. With both directions held, the newest key wins; releasing it restarts DAS for the remaining key. Multiple soft-drop bindings keep dropping until all are released. Held soft drop uses a single timer and awards one point per descended cell; it never falls slower than normal gravity. OS repeat events do not add extra steps when key releases are supported. Terminals without key releases keep OS repeat for both movement and soft drop; these timing settings do not apply there.

| Editor                   | Controls / constraints                                                                             |
| ------------------------ | -------------------------------------------------------------------------------------------------- |
| Key bindings             | Enter opens the list; select a binding or ADD KEY, press Enter, then press the key                 |
| Remove binding           | Delete; at least one binding per action must remain                                                |
| Accept / cancel bindings | F2 / Esc; during capture, these keys can also be assigned                                          |
| Duplicate bindings       | Rejected on SAVE; change both actions before saving a swap                                         |
| Color picker             | Enter opens; Tab switches palette / RGB                                                            |
| Palette / RGB            | Arrows select colors or adjust channels; PgUp / PgDn adjusts RGB by 16; Home / End selects 0 / 255 |
| Accept / cancel color    | Enter / Esc; Delete selects the terminal default                                                   |

## Stored data

| Item                   | Behavior                                                                                                            |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Location               | OS-standard data directory; `--db` overrides the SQLite file                                                        |
| Defaults / overrides   | First launch uses defaults; CLI play arguments override loaded settings for the session                             |
| Settings writes        | SAVE commits the complete draft; menu player/mode changes save automatically; R retries failures                    |
| Override persistence   | A settings save or menu change also persists the current CLI overrides                                              |
| Record writes          | Competitive results save automatically; on failure, keep the result open and press R to retry                       |
| Shared database        | Writes wait up to 5 seconds for a lock; retry after failure                                                         |
| Dates                  | Stored in UTC; displayed in local time                                                                              |
| Abandon game           | Returning to the menu from pause discards the active game                                                           |
| Database compatibility | Opening schema versions 0–2 deletes their records and settings; version 3 is preserved; newer versions are rejected |

Back up an older database before opening it. Saved-game resume, input replay, and data import are unsupported.
