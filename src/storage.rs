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
    pub mode: String,
    pub player: String,
    pub level: i64,
    pub lines: i64,
    pub score: i64,
    pub elapsed_ms: i64,
    pub completed_at: i64,
    pub outcome: String,
}

impl Record {
    pub fn from_game(game: &Game, player: &str) -> miette::Result<Self> {
        if game.mode() == Mode::Practice {
            return Err(miette::miette!("practice has no ranked records"));
        }
        Ok(Self {
            id: format!("{:032x}", rand::random::<u128>()),
            mode: game.mode().to_string(),
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

    pub fn save(&self, record: &Record) -> miette::Result<()> {
        // Reuse the game ID on retries so an uncertain commit cannot duplicate a score.
        self.db
            .execute(
                SAVE_RECORD,
                params![
                    record.id,
                    record.mode,
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

    pub fn personal_best(&self, current: &Record) -> miette::Result<Option<Record>> {
        let mode = current.mode.parse::<Mode>().into_diagnostic()?;
        // Exclude this run even on a query retry after saving; unfinished sprints have no PB time.
        Ok(self.records(mode)?.into_iter().find(|row| {
            row.id != current.id
                && row.player == current.player
                && (mode != Mode::FortyLines || row.outcome == "complete")
        }))
    }

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
                    mode: row.get(1)?,
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
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case(Mode::FortyLines)]
    #[case(Mode::Blitz)]
    fn personal_best_filters_player_mode_current_run_and_unfinished_sprints(#[case] mode: Mode) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let current = Record::from_game(&Game::new(mode, 42), "Alice").unwrap();
        assert_eq!(store.personal_best(&current).unwrap(), None);
        store.save(&current).unwrap();
        let best = Record {
            id: "best".into(),
            elapsed_ms: 60000,
            score: 1000,
            outcome: "complete".into(),
            ..current.clone()
        };
        for record in [
            best.clone(),
            Record {
                id: "slower".into(),
                elapsed_ms: 90000,
                score: 500,
                ..best.clone()
            },
            Record {
                id: "other-player".into(),
                player: "Bob".into(),
                elapsed_ms: 1,
                score: 9999,
                ..best.clone()
            },
            Record {
                id: "other-mode".into(),
                mode: if mode == Mode::Blitz { "40l" } else { "blitz" }.into(),
                elapsed_ms: 1,
                score: 9999,
                ..best.clone()
            },
        ] {
            store.save(&record).unwrap();
        }
        if mode == Mode::FortyLines {
            store
                .save(&Record {
                    id: "partial".into(),
                    outcome: "top_out".into(),
                    lines: 39,
                    elapsed_ms: 1,
                    ..best.clone()
                })
                .unwrap();
        }
        assert_eq!(store.personal_best(&current).unwrap(), Some(best.clone()));
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.personal_best(&current).unwrap(), Some(best));
    }

    #[test]
    fn settings_survive_reopen_and_failed_writes_roll_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.load_config().unwrap(), Config::default());
        let mut config = Config::default();
        for (name, value) in [
            ("player", "테트리스"),
            ("mode", "practice"),
            ("ghost_enabled", "false"),
            ("next_queue_length", "3"),
            ("das_ms", "120"),
            ("arr_ms", "0"),
            ("soft_drop_ms", "10"),
            ("practice_speed_percent", "200"),
            ("keys.left", "j, ß, COMMA, F2, DELETE"),
            ("theme.i", "red"),
            ("theme.o", "reset"),
            ("theme.background", "42"),
        ] {
            config.set(name, value).unwrap();
        }
        store.save_config(&config).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.load_config().unwrap(), config);
        store.db.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON app_settings WHEN NEW.name = 'theme.l' BEGIN SELECT RAISE(ABORT, 'full disk'); END;").unwrap();
        store.save_config(&Config::default()).unwrap_err();
        assert_eq!(store.load_config().unwrap(), config);
    }

    #[test]
    fn invalid_settings_and_practice_records_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("settings.db")).unwrap();
        let mut invalid = Config::default();
        invalid.keys.left = invalid.keys.right.clone();
        store.save_config(&invalid).unwrap_err();
        assert_eq!(store.load_config().unwrap(), Config::default());
        store
            .db
            .execute(
                "INSERT INTO app_settings VALUES ('next_queue_length', '0')",
                [],
            )
            .unwrap();
        store.load_config().unwrap_err();
        store.records(Mode::Practice).unwrap_err();
        Record::from_game(&Game::new(Mode::Practice, 42), "Alice").unwrap_err();
    }

    #[rstest::rstest]
    fn retired_formats_are_replaced_once_and_new_data_survives_reopen(
        #[values(0, 1, 2)] version: u32,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE game_records (id TEXT); INSERT INTO game_records VALUES ('old'); CREATE TABLE app_settings (name TEXT, value TEXT); INSERT INTO app_settings VALUES ('player', 'OLD');").unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        drop(db);
        let store = Store::open(&path).unwrap();
        assert!(store.records(Mode::FortyLines).unwrap().is_empty());
        assert_eq!(store.load_config().unwrap(), Config::default());
        let record = Record::from_game(&Game::new(Mode::Blitz, 42), "NEW").unwrap();
        store.save(&record).unwrap();
        let config = Config {
            player: "NEW".into(),
            ..Config::default()
        };
        store.save_config(&config).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.records(Mode::Blitz).unwrap(), [record]);
        assert_eq!(store.load_config().unwrap(), config);
    }

    #[test]
    fn forty_lines_ranks_finishes_before_partial_runs_and_blitz_uses_score() {
        let base = Record::from_game(&Game::new(Mode::FortyLines, 1), "Alice").unwrap();
        let mut rows = [
            Record {
                id: "partial39".into(),
                lines: 39,
                score: 9000,
                elapsed_ms: 1000,
                ..base.clone()
            },
            Record {
                id: "slow".into(),
                outcome: "complete".into(),
                lines: 40,
                score: 5000,
                elapsed_ms: 90000,
                ..base.clone()
            },
            Record {
                id: "partial20".into(),
                lines: 20,
                score: 2000,
                elapsed_ms: 500,
                ..base.clone()
            },
            Record {
                id: "fast".into(),
                outcome: "complete".into(),
                lines: 40,
                score: 3000,
                elapsed_ms: 60000,
                ..base
            },
        ];
        rows.sort_by(|a, b| rank_records(Mode::FortyLines, a, b));
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["fast", "slow", "partial39", "partial20"]
        );
        rows.sort_by(|a, b| rank_records(Mode::Blitz, a, b));
        assert_eq!(
            rows.iter().map(|row| row.score).collect::<Vec<_>>(),
            [9000, 5000, 3000, 2000]
        );
    }

    #[rstest::rstest]
    #[case("records.db")]
    #[case("scores%20archive.db")]
    #[case("scores%2Farchive.db")]
    #[case("점수 # archive.db")]
    #[cfg_attr(not(windows), case("file:scores.db"))]
    #[cfg_attr(not(windows), case("scores?archive.db"))]
    #[cfg_attr(not(windows), case(":memory:"))]
    fn reopening_preserves_records_and_retry_is_idempotent(#[case] filename: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(filename);
        let record = Record::from_game(&Game::new(Mode::FortyLines, 1), "Alice").unwrap();
        {
            let store = Store::open(&path).unwrap();
            store.save(&record).unwrap();
            store.save(&record).unwrap();
        }
        // Check the literal filename so reopening the same misparsed URL cannot hide a wrong DB.
        assert!(path.is_file(), "database missing at {}", path.display());
        let store = Store::open(&path).unwrap();
        let rows = store.records(Mode::FortyLines).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows, [record]);
        assert!(store.records(Mode::Blitz).unwrap().is_empty());
    }

    #[test]
    fn two_connections_share_records_and_upserts_replace_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let first = Store::open(&path).unwrap();
        let second = Store::open(&path).unwrap();
        let mut record = Record::from_game(&Game::new(Mode::FortyLines, 1), "Alice").unwrap();
        first.save(&record).unwrap();
        assert_eq!(
            second.records(Mode::FortyLines).unwrap().as_slice(),
            std::slice::from_ref(&record)
        );
        record.mode = "blitz".into();
        record.player = "Bob '테트리스'".into();
        record.level = 3;
        record.lines = 40;
        record.score = 12345;
        record.elapsed_ms = 60000;
        record.completed_at += 1;
        record.outcome = "complete".into();
        second.save(&record).unwrap();
        assert!(first.records(Mode::FortyLines).unwrap().is_empty());
        assert_eq!(first.records(Mode::Blitz).unwrap(), [record]);
    }

    #[test]
    fn locked_write_can_be_retried_without_losing_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let first = Store::open(&path).unwrap();
        let second = Store::open(&path).unwrap();
        second.db.busy_timeout(Duration::ZERO).unwrap();
        let record = Record::from_game(&Game::new(Mode::FortyLines, 1), "Alice").unwrap();
        first.save(&record).unwrap();
        let tx = first.db.unchecked_transaction().unwrap();
        tx.execute("UPDATE game_records SET score = score + 1", [])
            .unwrap();
        // Readers must see the last commit, e.g. while another player's write is uncommitted.
        assert_eq!(
            second.records(Mode::FortyLines).unwrap().as_slice(),
            std::slice::from_ref(&record)
        );
        assert!(second.save(&record).is_err());
        tx.rollback().unwrap();
        second.save(&record).unwrap();
        assert_eq!(first.records(Mode::FortyLines).unwrap(), [record]);
    }

    #[test]
    fn schema_replacement_is_transactional_and_rejects_future_versions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE game_records (id TEXT); INSERT INTO game_records VALUES ('existing'); CREATE VIEW app_settings AS SELECT 1;").unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
        Store::open(&path).err().unwrap();
        let id: String = db
            .query_row("SELECT id FROM game_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "existing");
        let version: u32 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        db.execute("DROP VIEW app_settings", []).unwrap();
        let store = Store::open(&path).unwrap();
        let record = Record::from_game(&Game::new(Mode::FortyLines, 1), "Alice").unwrap();
        store.save(&record).unwrap();
        db.pragma_update(None, "user_version", 4).unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(store.records(Mode::FortyLines).unwrap(), [record]);
    }

    #[rstest::rstest]
    #[case("id TEXT PRIMARY KEY, mode TEXT")]
    #[case(
        "id TEXT, mode TEXT, player TEXT, level INTEGER,
         lines INTEGER, score INTEGER, elapsed_ms INTEGER, completed_at INTEGER, outcome TEXT"
    )]
    fn incompatible_record_tables_are_rejected_before_play(#[case] columns: &str) {
        let version = 3;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let db = Connection::open(&path).unwrap();
        db.execute(&format!("CREATE TABLE game_records ({columns})"), [])
            .unwrap();
        db.execute(
            "INSERT INTO game_records (id, mode) VALUES ('existing', 'marathon')",
            [],
        )
        .unwrap();
        db.pragma_update(None, "user_version", version).unwrap();

        assert!(Store::open(&path).is_err());
        let after: u32 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(after, version);
        let id: String = db
            .query_row("SELECT id FROM game_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "existing");
    }

    #[test]
    fn simultaneous_first_opens_preserve_both_players() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let barrier = std::sync::Barrier::new(2);
        let save = |player| {
            let record = Record::from_game(&Game::new(Mode::FortyLines, 1), player).unwrap();
            barrier.wait();
            let store = Store::open(&path).unwrap();
            store.save(&record).unwrap();
        };
        std::thread::scope(|scope| {
            let first = scope.spawn(|| save("Alice"));
            save("Bob");
            first.join().unwrap();
        });
        let store = Store::open(&path).unwrap();
        let mut players: Vec<_> = store
            .records(Mode::FortyLines)
            .unwrap()
            .into_iter()
            .map(|record| record.player)
            .collect();
        players.sort();
        assert_eq!(players, ["Alice", "Bob"]);
    }
}
