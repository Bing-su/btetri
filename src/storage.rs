use std::cmp::Ordering;
use std::fs;
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use miette::{IntoDiagnostic, WrapErr};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};

use crate::game::{Game, Mode, Status};

const SAVE_RECORD: &str = "INSERT INTO game_records
    (id, mode, player, start_level, level, lines, score, elapsed_ms, completed_at, outcome)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
    ON CONFLICT(id) DO UPDATE SET
    mode = excluded.mode, player = excluded.player, start_level = excluded.start_level,
    level = excluded.level, lines = excluded.lines, score = excluded.score,
    elapsed_ms = excluded.elapsed_ms, completed_at = excluded.completed_at,
    outcome = excluded.outcome";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub id: String,
    pub mode: String,
    pub player: String,
    pub start_level: i64,
    pub level: i64,
    pub lines: i64,
    pub score: i64,
    pub elapsed_ms: i64,
    pub completed_at: i64,
    pub outcome: String,
}

impl Record {
    pub fn from_game(game: &Game, player: &str) -> miette::Result<Self> {
        Ok(Self {
            id: format!("{:032x}", rand::random::<u128>()),
            mode: game.mode().to_string(),
            player: player.to_owned(),
            start_level: i64::from(game.start_level()),
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
            0 => {
                tx.execute_batch(include_str!("../migrations/001_records.sql"))
                    .into_diagnostic()
                    .wrap_err("cannot initialize records schema")?;
                tx.pragma_update(None, "user_version", 1)
                    .into_diagnostic()?;
            }
            1 => {}
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
                    record.start_level,
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

    pub fn records(&self, mode: Mode) -> miette::Result<Vec<Record>> {
        let mut statement = self
            .db
            .prepare(
                "SELECT id, mode, player, start_level, level, lines, score, elapsed_ms, completed_at,
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
                    start_level: row.get(3)?,
                    level: row.get(4)?,
                    lines: row.get(5)?,
                    score: row.get(6)?,
                    elapsed_ms: row.get(7)?,
                    completed_at: row.get(8)?,
                    outcome: row.get(9)?,
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
    let primary = if mode == Mode::Sprint {
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
    use crate::game::Rules;

    #[test]
    fn sprint_ranks_finishes_before_partial_runs_and_score_modes_use_score() {
        let base = Record::from_game(
            &Game::new(Mode::Sprint, Rules::default(), 1).unwrap(),
            "Alice",
        )
        .unwrap();
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
        rows.sort_by(|a, b| rank_records(Mode::Sprint, a, b));
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["fast", "slow", "partial39", "partial20"]
        );
        for mode in [Mode::Marathon, Mode::Ultra] {
            rows.sort_by(|a, b| rank_records(mode, a, b));
            assert_eq!(
                rows.iter().map(|row| row.score).collect::<Vec<_>>(),
                [9000, 5000, 3000, 2000]
            );
        }
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
        let record = Record::from_game(
            &Game::new(Mode::Marathon, Rules::default(), 1).unwrap(),
            "Alice",
        )
        .unwrap();
        {
            let store = Store::open(&path).unwrap();
            store.save(&record).unwrap();
            store.save(&record).unwrap();
        }
        // Check the literal filename so reopening the same misparsed URL cannot hide a wrong DB.
        assert!(path.is_file(), "database missing at {}", path.display());
        let store = Store::open(&path).unwrap();
        let rows = store.records(Mode::Marathon).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows, [record]);
        assert!(store.records(Mode::Ultra).unwrap().is_empty());
    }

    #[test]
    fn two_connections_share_records_and_upserts_replace_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let first = Store::open(&path).unwrap();
        let second = Store::open(&path).unwrap();
        let mut record = Record::from_game(
            &Game::new(Mode::Marathon, Rules::default(), 1).unwrap(),
            "Alice",
        )
        .unwrap();
        first.save(&record).unwrap();
        assert_eq!(
            second.records(Mode::Marathon).unwrap().as_slice(),
            std::slice::from_ref(&record)
        );
        record.mode = "sprint".into();
        record.player = "Bob '테트리스'".into();
        record.start_level = 2;
        record.level = 3;
        record.lines = 40;
        record.score = 12345;
        record.elapsed_ms = 60000;
        record.completed_at += 1;
        record.outcome = "complete".into();
        second.save(&record).unwrap();
        assert!(first.records(Mode::Marathon).unwrap().is_empty());
        assert_eq!(first.records(Mode::Sprint).unwrap(), [record]);
    }

    #[test]
    fn locked_write_can_be_retried_without_losing_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let first = Store::open(&path).unwrap();
        let second = Store::open(&path).unwrap();
        second.db.busy_timeout(Duration::ZERO).unwrap();
        let record = Record::from_game(
            &Game::new(Mode::Marathon, Rules::default(), 1).unwrap(),
            "Alice",
        )
        .unwrap();
        first.save(&record).unwrap();
        let tx = first.db.unchecked_transaction().unwrap();
        tx.execute("UPDATE game_records SET score = score + 1", [])
            .unwrap();
        // Readers must see the last commit, e.g. while another player's write is uncommitted.
        assert_eq!(
            second.records(Mode::Marathon).unwrap().as_slice(),
            std::slice::from_ref(&record)
        );
        assert!(second.save(&record).is_err());
        tx.rollback().unwrap();
        second.save(&record).unwrap();
        assert_eq!(first.records(Mode::Marathon).unwrap(), [record]);
    }

    #[test]
    fn schema_initialization_is_transactional_and_rejects_future_versions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let db = Connection::open(&path).unwrap();
        db.execute("CREATE TABLE game_records (id TEXT)", [])
            .unwrap();
        assert!(Store::open(&path).is_err());
        let version: u32 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
        db.execute("DROP TABLE game_records", []).unwrap();
        let store = Store::open(&path).unwrap();
        let version: u32 = store
            .db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let record = Record::from_game(
            &Game::new(Mode::Marathon, Rules::default(), 1).unwrap(),
            "Alice",
        )
        .unwrap();
        store.save(&record).unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(store.records(Mode::Marathon).unwrap(), [record]);
    }

    #[rstest::rstest]
    #[case("id TEXT PRIMARY KEY, mode TEXT")]
    #[case(
        "id TEXT, mode TEXT, player TEXT, start_level INTEGER, level INTEGER,
         lines INTEGER, score INTEGER, elapsed_ms INTEGER, completed_at INTEGER, outcome TEXT"
    )]
    fn incompatible_record_tables_are_rejected_before_play(
        #[case] columns: &str,
        #[values(0, 1)] version: u32,
    ) {
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
            let record = Record::from_game(
                &Game::new(Mode::Marathon, Rules::default(), 1).unwrap(),
                player,
            )
            .unwrap();
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
            .records(Mode::Marathon)
            .unwrap()
            .into_iter()
            .map(|record| record.player)
            .collect();
        players.sort();
        assert_eq!(players, ["Alice", "Bob"]);
    }
}
