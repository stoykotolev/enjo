#![allow(dead_code)]
// TODO(phase1): drop once app/ui call the store

//! Local SQLite storage layer for enjo.
//!
//! The relational SQLite DB is the source of truth for the UI. Access goes
//! through the object-safe [`Store`] trait so the backend stays swappable (a Go
//! equivalent arrives in Phase 2).
//!
//! ## Design choices
//! - The connection is wrapped in a `Mutex<Connection>` so [`SqliteStore`] is
//!   `Sync` and can later be shared with a background sync thread. enjo is
//!   single-user, so lock contention is a non-issue.
//! - `upsert` uses `INSERT ... ON CONFLICT(id) DO UPDATE` (a true upsert) so the
//!   `PRIMARY KEY` row is updated in place without delete/reinsert churn.
//! - Migrations are applied via `execute_batch` on every `open` (idempotent
//!   `CREATE ... IF NOT EXISTS`). refinery is deferred to Phase 1's later need;
//!   this is an intentional deviation.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::model::{Priority, Status, Task};

/// Storage backend for tasks. Object-safe so it can be boxed behind a `dyn`.
pub trait Store {
    /// All non-deleted tasks (`deleted = 0`), ordered by `created_at` for a
    /// deterministic base order. Final Today/All sorting happens in the app layer.
    fn list_active(&self) -> Result<Vec<Task>>;

    /// Fetch a single task by id, or `None` if no such row exists.
    fn get(&self, id: Uuid) -> Result<Option<Task>>;

    /// Insert-or-replace by id, updating every column in place on conflict.
    fn upsert(&self, task: &Task) -> Result<()>;

    /// Mark a task deleted and advance its `updated_at` to now (RFC3339), so the
    /// last-write-wins clock moves forward.
    fn soft_delete(&self, id: Uuid) -> Result<()>;
}

/// SQLite-backed [`Store`].
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open (or create) the DB at `path`, configure pragmas, and apply the
    /// migration.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Open an in-memory DB running the same migration. For tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

/// Columns in stable order, shared by SELECT and the row mapper.
const COLUMNS: &str = "id, title, notes, status, priority, due_date, project, \
     created_at, updated_at, completed_at, deleted, server_seq";

impl Store for SqliteStore {
    fn list_active(&self) -> Result<Vec<Task>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let sql = format!("SELECT {COLUMNS} FROM tasks WHERE deleted = 0 ORDER BY created_at, id");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_task)?;
        let mut tasks = Vec::new();
        for task in rows {
            tasks.push(task?);
        }
        Ok(tasks)
    }

    fn get(&self, id: Uuid) -> Result<Option<Task>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let sql = format!("SELECT {COLUMNS} FROM tasks WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let task = stmt.query_row([id.to_string()], row_to_task).optional()?;
        Ok(task)
    }

    fn upsert(&self, task: &Task) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO tasks (\
                 id, title, notes, status, priority, due_date, project, \
                 created_at, updated_at, completed_at, deleted, server_seq\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)\
             ON CONFLICT(id) DO UPDATE SET \
                 title = excluded.title, \
                 notes = excluded.notes, \
                 status = excluded.status, \
                 priority = excluded.priority, \
                 due_date = excluded.due_date, \
                 project = excluded.project, \
                 created_at = excluded.created_at, \
                 updated_at = excluded.updated_at, \
                 completed_at = excluded.completed_at, \
                 deleted = excluded.deleted, \
                 server_seq = excluded.server_seq",
            rusqlite::params![
                task.id.to_string(),
                task.title,
                task.notes,
                task.status.as_str(),
                task.priority.as_str(),
                task.due_date.map(|d| d.format("%Y-%m-%d").to_string()),
                task.project,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
                task.completed_at.map(|t| t.to_rfc3339()),
                task.deleted as i64,
                task.server_seq,
            ],
        )?;
        Ok(())
    }

    fn soft_delete(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "UPDATE tasks SET deleted = 1, updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id.to_string(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}

/// Map a SQLite row (in [`COLUMNS`] order) to a [`Task`], translating encoding
/// failures into `rusqlite` errors rather than panicking.
fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
    let id = parse_uuid(row, 0)?;
    let title: String = row.get(1)?;
    let notes: Option<String> = row.get(2)?;
    let status = parse_status(row, 3)?;
    let priority = parse_priority(row, 4)?;
    let due_date = match row.get::<_, Option<String>>(5)? {
        Some(s) => Some(parse_naive_date(row, 5, &s)?),
        None => None,
    };
    let project: Option<String> = row.get(6)?;
    let created_at = parse_rfc3339(row, 7)?;
    let updated_at = parse_rfc3339(row, 8)?;
    let completed_at = match row.get::<_, Option<String>>(9)? {
        Some(s) => Some(parse_rfc3339_str(row, 9, &s)?),
        None => None,
    };
    let deleted = row.get::<_, i64>(10)? != 0;
    let server_seq: Option<i64> = row.get(11)?;

    Ok(Task {
        id,
        title,
        notes,
        status,
        priority,
        due_date,
        project,
        created_at,
        updated_at,
        completed_at,
        deleted,
        server_seq,
    })
}

fn parse_uuid(row: &Row, idx: usize) -> rusqlite::Result<Uuid> {
    let s: String = row.get(idx)?;
    Uuid::parse_str(&s).map_err(|e| from_sql_err(idx, e))
}

fn parse_status(row: &Row, idx: usize) -> rusqlite::Result<Status> {
    let s: String = row.get(idx)?;
    Status::from_str(&s).ok_or_else(|| invalid_value(idx, "status", &s))
}

fn parse_priority(row: &Row, idx: usize) -> rusqlite::Result<Priority> {
    let s: String = row.get(idx)?;
    Priority::from_str(&s).ok_or_else(|| invalid_value(idx, "priority", &s))
}

fn parse_naive_date(_row: &Row, idx: usize, s: &str) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| from_sql_err(idx, e))
}

fn parse_rfc3339(row: &Row, idx: usize) -> rusqlite::Result<DateTime<Utc>> {
    let s: String = row.get(idx)?;
    parse_rfc3339_str(row, idx, &s)
}

fn parse_rfc3339_str(_row: &Row, idx: usize, s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| from_sql_err(idx, e))
}

/// Wrap an arbitrary parse error as a `FromSqlConversionFailure`.
fn from_sql_err<E>(idx: usize, err: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err))
}

/// Build a conversion error for an out-of-domain enum value.
fn invalid_value(idx: usize, field: &str, value: &str) -> rusqlite::Error {
    let msg = format!("invalid {field} value in column {idx}: {value:?}");
    rusqlite::Error::FromSqlConversionFailure(
        idx,
        Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, msg)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn full_task() -> Task {
        let mut t = Task::new("fully populated".to_string());
        t.notes = Some("some notes".to_string());
        t.priority = Priority::Urgent;
        t.due_date = Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        t.project = Some("enjo".to_string());
        t.set_status(Status::Done); // stamps completed_at
        t.server_seq = Some(99);
        t
    }

    #[test]
    fn upsert_get_roundtrip_full_task() {
        let store = SqliteStore::open_in_memory().unwrap();
        let t = full_task();
        store.upsert(&t).unwrap();
        let got = store.get(t.id).unwrap().expect("task present");
        assert_eq!(got, t);
        // Every Option is Some and preserved exactly.
        assert_eq!(got.notes, t.notes);
        assert_eq!(got.due_date, t.due_date);
        assert_eq!(got.project, t.project);
        assert_eq!(got.completed_at, t.completed_at);
        assert_eq!(got.server_seq, t.server_seq);
    }

    #[test]
    fn upsert_get_roundtrip_minimal_task() {
        let store = SqliteStore::open_in_memory().unwrap();
        let t = Task::new("minimal".to_string());
        assert!(t.notes.is_none());
        assert!(t.due_date.is_none());
        assert!(t.project.is_none());
        assert!(t.completed_at.is_none());
        assert!(t.server_seq.is_none());
        store.upsert(&t).unwrap();
        let got = store.get(t.id).unwrap().expect("task present");
        assert_eq!(got, t);
    }

    #[test]
    fn list_active_excludes_soft_deleted() {
        let store = SqliteStore::open_in_memory().unwrap();
        let active = Task::new("active".to_string());
        let mut gone = Task::new("gone".to_string());
        gone.deleted = true;
        store.upsert(&active).unwrap();
        store.upsert(&gone).unwrap();

        let listed = store.list_active().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, active.id);
    }

    #[test]
    fn upsert_twice_updates_in_place() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut t = Task::new("v1".to_string());
        store.upsert(&t).unwrap();

        t.title = "v2".to_string();
        t.priority = Priority::High;
        store.upsert(&t).unwrap();

        let listed = store.list_active().unwrap();
        assert_eq!(listed.len(), 1, "no duplicate row");
        let got = store.get(t.id).unwrap().unwrap();
        assert_eq!(got.title, "v2");
        assert_eq!(got.priority, Priority::High);
    }

    #[test]
    fn soft_delete_sets_flag_and_advances_updated_at() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut t = Task::new("delete me".to_string());
        // Pin updated_at to the past so the advancement assertion is unambiguous.
        let old = Utc::now() - Duration::seconds(60);
        t.updated_at = old;
        store.upsert(&t).unwrap();

        store.soft_delete(t.id).unwrap();

        // Excluded from active list.
        assert!(store.list_active().unwrap().is_empty());
        // Row still present, flagged, with an advanced clock.
        let got = store.get(t.id).unwrap().expect("row still present");
        assert!(got.deleted);
        assert!(got.updated_at >= old);
        assert!(got.updated_at > old, "updated_at must strictly advance");
    }

    #[test]
    fn get_unknown_id_returns_none() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.get(Uuid::now_v7()).unwrap().is_none());
    }

    #[test]
    fn sub_second_timestamp_precision_roundtrips() {
        use chrono::Timelike;
        let store = SqliteStore::open_in_memory().unwrap();
        let mut t = Task::new("precise".to_string());
        // A deliberate nanosecond-resolution instant must survive the
        // RFC3339 TEXT encoding byte-for-byte.
        let precise = DateTime::parse_from_rfc3339("2026-06-27T12:34:56.123456789Z")
            .unwrap()
            .with_timezone(&Utc);
        t.created_at = precise;
        t.updated_at = precise;
        t.completed_at = Some(precise);
        store.upsert(&t).unwrap();

        let got = store.get(t.id).unwrap().expect("task present");
        assert_eq!(got.created_at, precise);
        assert_eq!(got.created_at.nanosecond(), 123_456_789);
        assert_eq!(got.completed_at, Some(precise));
        assert_eq!(got, t);
    }

    #[test]
    fn check_constraint_rejects_invalid_status() {
        let store = SqliteStore::open_in_memory().unwrap();
        let conn = store.conn.lock().unwrap();
        // Bypass the typed write path to prove the schema CHECK guards the
        // status domain even against raw/corrupt inserts.
        let res = conn.execute(
            "INSERT INTO tasks (id, title, status, priority, created_at, updated_at, deleted) \
             VALUES (?1, 'x', 'bogus', 'low', \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0)",
            rusqlite::params![Uuid::now_v7().to_string()],
        );
        assert!(res.is_err(), "CHECK must reject an out-of-domain status");
    }

    #[test]
    fn corrupt_timestamp_propagates_error_not_panic() {
        let store = SqliteStore::open_in_memory().unwrap();
        let id = Uuid::now_v7();
        {
            // created_at has no CHECK, so an unparseable value can land in the
            // row; reading it back must surface an Err, never panic.
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO tasks (id, title, status, priority, created_at, updated_at, deleted) \
                 VALUES (?1, 'x', 'todo', 'low', 'not-a-timestamp', \
                         '2026-01-01T00:00:00Z', 0)",
                rusqlite::params![id.to_string()],
            )
            .unwrap();
        }
        let res = store.get(id);
        assert!(
            res.is_err(),
            "corrupt created_at must surface as Err, not panic"
        );
    }

    #[test]
    fn normal_writes_satisfy_check_constraints() {
        let store = SqliteStore::open_in_memory().unwrap();
        // One of every status/priority round-trips through the CHECK constraints.
        for status in [Status::Todo, Status::InProgress, Status::Done] {
            for priority in [
                Priority::Low,
                Priority::Medium,
                Priority::High,
                Priority::Urgent,
            ] {
                let mut t = Task::new("check".to_string());
                t.status = status;
                t.priority = priority;
                store.upsert(&t).expect("write satisfies CHECK constraints");
            }
        }
    }
}
