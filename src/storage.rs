//! SQLite persistence, transactional settings, and competitive record ranking.

use std::cmp::Ordering;
use std::fs;
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use miette::{IntoDiagnostic, WrapErr};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};

use crate::config::Config;
use crate::game::{Game, Mode, Status};

const SAVE_RECORD: &str = "INSERT INTO game_records
    (id, mode, player, level, lines, score, elapsed_ms, completed_at, outcome)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
    ON CONFLICT(id) DO UPDATE SET
    mode = excluded.mode, player = excluded.player,
    level = excluded.level, lines = excluded.lines, score = excluded.score,
    elapsed_ms = excluded.elapsed_ms, completed_at = excluded.completed_at,
    outcome = excluded.outcome";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub id: String,
    pub mode: Mode,
    pub player: String,
    pub level: i64,
    pub lines: i64,
    pub score: i64,
    pub elapsed_ms: i64,
    pub completed_at: i64,
    pub outcome: String,
}

impl Record {
    /// Snapshot a competitive game with a fresh retry-stable ID and completion timestamp.
    /// Rejects practice and values too large for SQLite integers.
    /// A game not marked complete is recorded as top-out; callers finish the game first.
    pub fn from_game(game: &Game, player: &str) -> miette::Result<Self> {
        if game.mode() == Mode::Practice {
            return Err(miette::miette!("practice has no ranked records"));
        }
        Ok(Self {
            id: format!("{:032x}", rand::random::<u128>()),
            mode: game.mode(),
            player: player.to_owned(),
            level: i64::from(game.level()),
            lines: i64::from(game.lines()),
            score: game.score().try_into().into_diagnostic()?,
            elapsed_ms: game.elapsed().as_millis().try_into().into_diagnostic()?,
            completed_at: Utc::now().timestamp(),
            outcome: if game.status() == Status::Complete {
                "complete"
            } else {
                "top_out"
            }
            .into(),
        })
    }
}

pub struct Store {
    db: Connection,
}

impl Store {
    /// Open a local SQLite file, creating parent directories and initializing its schema.
    /// Schema versions 0–2 are replaced, discarding retired records and settings.
    /// Rejects future or incompatible schemas; initialization is transactional.
    pub fn open(path: &Path) -> miette::Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err_with(|| format!("cannot create {}", parent.display()))?;
        }
        // Use an absolute, non-URI filename so even `:memory:` and `file:scores.db` stay files.
        let path = std::path::absolute(path).into_diagnostic()?;
        let mut db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .into_diagnostic()
        .wrap_err("cannot open local records database")?;
        db.busy_timeout(Duration::from_secs(5)).into_diagnostic()?;
        // Serialize initialization, e.g. when two games first open the same new database.
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .into_diagnostic()?;
        let version: u32 = tx
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .into_diagnostic()?;
        match version {
            0..=2 => {
                // Replace retired formats outright, e.g. discard their scores and settings together.
                tx.execute_batch(
                    "DROP TABLE IF EXISTS game_records; DROP TABLE IF EXISTS app_settings;",
                )
                .into_diagnostic()?;
                tx.execute_batch(include_str!("schema.sql"))
                    .into_diagnostic()
                    .wrap_err("cannot initialize records schema")?;
                tx.pragma_update(None, "user_version", 3)
                    .into_diagnostic()?;
            }
            3 => {}
            _ => {
                return Err(miette::miette!(
                    "unsupported records schema version {version}"
                ));
            }
        }
        // Reject incompatible tables before play, e.g. missing columns or a non-unique ID.
        tx.prepare(SAVE_RECORD)
            .into_diagnostic()
            .wrap_err("records schema cannot store game results")?;
        tx.commit().into_diagnostic()?;
        Ok(Self { db })
    }

    /// Insert or replace a result by ID so retries cannot duplicate a game.
    /// Returns SQLite failures without consuming the record, allowing a later retry.
    pub fn save(&self, record: &Record) -> miette::Result<()> {
        // Reuse the game ID on retries so an uncertain commit cannot duplicate a score.
        self.db
            .execute(
                SAVE_RECORD,
                params![
                    record.id,
                    record.mode.to_string(),
                    record.player,
                    record.level,
                    record.lines,
                    record.score,
                    record.elapsed_ms,
                    record.completed_at,
                    record.outcome,
                ],
            )
            .into_diagnostic()
            .wrap_err("cannot save completed game")?;
        Ok(())
    }

    /// Load saved entries over defaults and validate the complete configuration.
    /// Returns an error for unknown or invalid saved values rather than silently ignoring them.
    pub fn load_config(&self) -> miette::Result<Config> {
        let mut config = Config::default();
        let mut statement = self
            .db
            .prepare("SELECT name, value FROM app_settings")
            .into_diagnostic()?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .into_diagnostic()?;
        for row in rows {
            let (name, value) = row.into_diagnostic()?;
            config
                .set(&name, &value)
                .map_err(|error| miette::miette!("invalid saved setting {name}: {error}"))?;
        }
        config
            .validate()
            .map_err(|error| miette::miette!("invalid saved settings: {error}"))?;
        Ok(config)
    }

    /// Validate and atomically replace the complete saved configuration.
    /// A failed write rolls back the snapshot, preserving previously saved settings.
    pub fn save_config(&self, config: &Config) -> miette::Result<()> {
        config
            .validate()
            .map_err(|error| miette::miette!("{error}"))?;
        // Commit one snapshot, e.g. a crash cannot save only half of a key swap.
        let tx = self.db.unchecked_transaction().into_diagnostic()?;
        tx.execute("DELETE FROM app_settings", [])
            .into_diagnostic()?;
        for (name, value) in config.entries() {
            tx.execute(
                "INSERT INTO app_settings (name, value) VALUES (?1, ?2)",
                params![name, value],
            )
            .into_diagnostic()?;
        }
        tx.commit().into_diagnostic()?;
        Ok(())
    }

    /// Find the player's best earlier result in this mode, excluding the current ID.
    /// Only completed 40 LINES runs qualify; score modes also include top-outs.
    /// Returns query failures so the caller can offer a retry.
    pub fn personal_best(&self, current: &Record) -> miette::Result<Option<Record>> {
        let mode = current.mode;
        // Exclude this run even on a query retry after saving; unfinished sprints have no PB time.
        Ok(self.records(mode)?.into_iter().find(|row| {
            row.id != current.id
                && row.player == current.player
                && (mode != Mode::FortyLines || row.outcome == "complete")
        }))
    }

    /// Load ranked results for a competitive mode, including deterministic tie-breaks.
    /// 40 LINES ranks completions by time, then partial runs by lines; other modes use score.
    /// Rejects practice and propagates database/query errors.
    pub fn records(&self, mode: Mode) -> miette::Result<Vec<Record>> {
        if mode == Mode::Practice {
            return Err(miette::miette!("practice has no ranked records"));
        }
        let mut statement = self
            .db
            .prepare(
                "SELECT id, mode, player, level, lines, score, elapsed_ms, completed_at,
                 outcome FROM game_records WHERE mode = ?1",
            )
            .into_diagnostic()
            .wrap_err("cannot query local records")?;
        let mut records = statement
            .query_map([mode.to_string()], |row| {
                Ok(Record {
                    id: row.get(0)?,
                    // The query already selects this mode; keep SQL names at the storage seam.
                    mode,
                    player: row.get(2)?,
                    level: row.get(3)?,
                    lines: row.get(4)?,
                    score: row.get(5)?,
                    elapsed_ms: row.get(6)?,
                    completed_at: row.get(7)?,
                    outcome: row.get(8)?,
                })
            })
            .into_diagnostic()?
            .collect::<rusqlite::Result<Vec<_>>>()
            .into_diagnostic()
            .wrap_err("cannot load local records")?;
        records.sort_by(|a, b| rank_records(mode, a, b));
        Ok(records)
    }
}

fn rank_records(mode: Mode, a: &Record, b: &Record) -> Ordering {
    let primary = if mode == Mode::FortyLines {
        let a_done = a.outcome == "complete";
        let b_done = b.outcome == "complete";
        b_done.cmp(&a_done).then_with(|| {
            if a_done && b_done {
                a.elapsed_ms.cmp(&b.elapsed_ms)
            } else {
                b.lines.cmp(&a.lines)
            }
        })
    } else {
        b.score.cmp(&a.score)
    };
    primary
        .then_with(|| a.elapsed_ms.cmp(&b.elapsed_ms))
        .then_with(|| a.completed_at.cmp(&b.completed_at))
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
mod tests;
