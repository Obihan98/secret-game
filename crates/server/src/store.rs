//! One SQLite file per world (TS6): the input log and snapshots, nothing else.
//!
//! WAL with `synchronous=FULL`, the only setting durable across power loss. A record counts as
//! durable once the transaction holding it has committed; `durable_len` tracks how much of the
//! log that is, and nothing may be acknowledged beyond it (see `host`).

use std::fmt;
use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags, Transaction};
use sim::codec::DecodeError;
use sim::log::{Record, Recorder, ReplayError};
use sim::world::SimError;

/// Table layout of the file, in `PRAGMA user_version`. The bytes inside each row carry their
/// own format version (TS6); this one covers only the tables around them.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE log (
        seq INTEGER PRIMARY KEY,
        record BLOB NOT NULL
    ) STRICT;
    CREATE TABLE snapshot (
        id INTEGER PRIMARY KEY,
        log_len INTEGER NOT NULL,
        game_time INTEGER NOT NULL,
        state BLOB NOT NULL
    ) STRICT;
";

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// `create` was pointed at a file that already holds data.
    Exists,
    /// The file's table layout is not the one this build knows.
    Schema(i64),
    /// The log has no record with this sequence number, though later ones exist.
    Gap(usize),
    /// The file holds no snapshot to start from.
    NoSnapshot,
    TooLarge,
    Record {
        index: usize,
        error: DecodeError,
    },
    Replay(ReplayError),
    Sim(SimError),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "sqlite: {e}"),
            Self::Exists => f.write_str("the file already holds a world"),
            Self::Schema(v) => write!(f, "unknown schema version {v}"),
            Self::Gap(seq) => write!(f, "log record {seq} is missing"),
            Self::NoSnapshot => f.write_str("no snapshot to start from"),
            Self::TooLarge => f.write_str("log too long for the file format"),
            Self::Record { index, error } => write!(f, "log record {index}: {error}"),
            Self::Replay(e) => write!(f, "replay: {e}"),
            Self::Sim(e) => write!(f, "sim: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<ReplayError> for StoreError {
    fn from(e: ReplayError) -> Self {
        Self::Replay(e)
    }
}

impl From<SimError> for StoreError {
    fn from(e: SimError) -> Self {
        Self::Sim(e)
    }
}

#[derive(Debug)]
pub struct Store {
    conn: Connection,
    /// Log records `0..durable_len` are committed.
    durable_len: usize,
}

impl Store {
    /// A new world file holding `rec`'s first snapshot and log.
    ///
    /// # Errors
    /// `Exists` if the file already holds data; any SQLite error.
    pub fn create(path: &Path, rec: &Recorder) -> Result<Self, StoreError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let conn = Connection::open_with_flags(path, flags)?;
        let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
        if tables != 0 || user_version(&conn)? != 0 {
            return Err(StoreError::Exists);
        }
        configure(&conn)?;
        let mut store = Self {
            conn,
            durable_len: 0,
        };
        let tx = store.conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        store.save_snapshot(rec)?;
        Ok(store)
    }

    /// Opens a world file and recovers it: the latest snapshot plus a replay of the log after
    /// it (TM3: no catch-up; the world resumes at its last logged second).
    ///
    /// # Errors
    /// A missing file, an unknown schema, a gap or bad bytes in the log, or a failed replay.
    pub fn open(path: &Path) -> Result<(Self, Recorder), StoreError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        configure(&conn)?;
        let version = user_version(&conn)?;
        if version != SCHEMA_VERSION {
            return Err(StoreError::Schema(version));
        }
        let snapshot: Vec<u8> = conn
            .query_row(
                "SELECT state FROM snapshot ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StoreError::NoSnapshot,
                e => StoreError::Sqlite(e),
            })?;
        let records = read_log(&conn)?;
        let rec = Recorder::replay(&snapshot, &records)?;
        let durable_len = records.len();
        Ok((Self { conn, durable_len }, rec))
    }

    /// Commits every log record of `rec` not yet in the file, in one transaction (the batched
    /// flush, about once a second). Returns the new durable length.
    ///
    /// # Errors
    /// Any SQLite error; the records stay pending and a later call retries them.
    pub fn persist(&mut self, rec: &Recorder) -> Result<usize, StoreError> {
        if rec.log().len() > self.durable_len {
            let tx = self.conn.transaction()?;
            write_log(&tx, rec.log(), self.durable_len)?;
            tx.commit()?;
            self.durable_len = rec.log().len();
        }
        Ok(self.durable_len)
    }

    /// Commits the pending log and a snapshot of `rec` together, so a stored snapshot never
    /// covers records the file lacks.
    ///
    /// # Errors
    /// Any SQLite error.
    pub fn save_snapshot(&mut self, rec: &Recorder) -> Result<usize, StoreError> {
        let len = rec.log().len();
        let tx = self.conn.transaction()?;
        write_log(&tx, rec.log(), self.durable_len)?;
        tx.execute(
            "INSERT INTO snapshot (log_len, game_time, state) VALUES (?1, ?2, ?3)",
            (sql_int(len)?, rec.world().now().seconds(), rec.snapshot()),
        )?;
        tx.commit()?;
        self.durable_len = len;
        Ok(len)
    }

    /// A consistent copy of everything committed so far, through SQLite's online backup API.
    ///
    /// # Errors
    /// Any SQLite error, e.g. an unwritable destination.
    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError> {
        self.conn.backup(MAIN_DB, path, None)?;
        Ok(())
    }

    #[must_use]
    pub const fn durable_len(&self) -> usize {
        self.durable_len
    }
}

/// Per connection: WAL is stored in the file, but `synchronous` is not.
fn configure(conn: &Connection) -> Result<(), StoreError> {
    let mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    conn.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

fn user_version(conn: &Connection) -> Result<i64, StoreError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

fn sql_int(n: usize) -> Result<i64, StoreError> {
    i64::try_from(n).map_err(|_| StoreError::TooLarge)
}

fn write_log(tx: &Transaction<'_>, log: &[Record], from: usize) -> Result<(), StoreError> {
    let mut insert = tx.prepare_cached("INSERT INTO log (seq, record) VALUES (?1, ?2)")?;
    for (seq, record) in log.iter().enumerate().skip(from) {
        insert.execute((sql_int(seq)?, record.encode()))?;
    }
    Ok(())
}

fn read_log(conn: &Connection) -> Result<Vec<Record>, StoreError> {
    let mut stmt = conn.prepare("SELECT seq, record FROM log ORDER BY seq")?;
    let mut rows = stmt.query([])?;
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let index = records.len();
        if row.get::<_, i64>(0)? != sql_int(index)? {
            return Err(StoreError::Gap(index));
        }
        let bytes: Vec<u8> = row.get(1)?;
        let record = Record::decode(&bytes).map_err(|error| StoreError::Record { index, error })?;
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `synchronous` lives on the connection, so it can only be checked from inside.
    #[test]
    fn connections_are_wal_and_fully_synchronous() {
        let dir = std::env::temp_dir().join(format!("store-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let scenario = sim::world::Scenario {
            nations: 1,
            ..Default::default()
        };
        let rec = Recorder::create(1, &scenario).unwrap();
        let store = Store::create(&dir.join("w.db"), &rec).unwrap();
        let sync: i64 = store
            .conn
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 2, "2 = FULL");
        drop(store);
        let (store, _) = Store::open(&dir.join("w.db")).unwrap();
        let sync: i64 = store
            .conn
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 2);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
