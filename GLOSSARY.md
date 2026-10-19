# btetri

Terms for the terminal game, its menu, and local results.

| Term       | Meaning                                                                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Game       | One play session. Competitive games end on completion or top-out; practice resets its board on top-out. Returning to the menu abandons it. |
| Mode       | A complete ruleset: 40 LINES speed challenge, BLITZ score challenge, MARATHON survival challenge, or unlimited PRACTICE.                   |
| Record     | A competitive game's result, including player, mode, score, level, lines, and duration. Practice creates no records.                       |
| Menu item  | One main-menu choice: player, mode, start game, records, settings, or quit.                                                                |
| Settings   | Editable draft of player, mode, display, bindings, and palette. SAVE writes the complete draft to SQLite; CANCEL discards it.              |
| Theme      | The terminal palette for the board, pieces, ghost, text, borders, and selected choices.                                                    |
| Held input | Movement keys tracked until release. The newest horizontal key controls DAS/ARR; soft-drop bindings share one held state.                  |
