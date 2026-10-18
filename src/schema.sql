CREATE TABLE IF NOT EXISTS game_records (
    id TEXT PRIMARY KEY NOT NULL,
    mode TEXT NOT NULL,
    player TEXT NOT NULL,
    level INTEGER NOT NULL,
    lines INTEGER NOT NULL,
    score INTEGER NOT NULL,
    elapsed_ms INTEGER NOT NULL,
    completed_at INTEGER NOT NULL,
    outcome TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS game_records_mode ON game_records (mode);
CREATE TABLE IF NOT EXISTS app_settings (
    name TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
