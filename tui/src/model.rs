use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle state of a [`Task`].
///
/// The snake_case spellings produced by [`Status::as_str`] are the canonical
/// wire/SQLite encoding; the `serde` derives use the same spellings via
/// `rename_all = "snake_case"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Todo,
    InProgress,
    Done,
}

impl Status {
    /// snake_case TEXT encoding used for SQLite columns and the sync wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::InProgress => "in_progress",
            Status::Done => "done",
        }
    }

    /// Parse the snake_case TEXT encoding produced by [`Status::as_str`].
    ///
    /// Returns `None` for unknown values so callers can decide how to handle
    /// corrupt/legacy data. Round-trips with [`Status::as_str`].
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "todo" => Some(Status::Todo),
            "in_progress" => Some(Status::InProgress),
            "done" => Some(Status::Done),
            _ => None,
        }
    }

    /// Cycle to the next status, wrapping around: Todo -> InProgress -> Done -> Todo.
    /// Used by the `s` keybinding.
    pub fn next(self) -> Self {
        match self {
            Status::Todo => Status::InProgress,
            Status::InProgress => Status::Done,
            Status::Done => Status::Todo,
        }
    }
}

/// Task priority.
///
/// Variants are declared in ascending importance (`Low` < `Medium` < `High` <
/// `Urgent`), and `PartialOrd`/`Ord` are derived from that declaration order so
/// `Urgent` is the greatest. The Today view sorts "priority descending" by
/// reversing this natural order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
    Urgent,
}

impl Priority {
    /// snake_case TEXT encoding used for SQLite columns and the sync wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
            Priority::Urgent => "urgent",
        }
    }

    /// Parse the snake_case TEXT encoding produced by [`Priority::as_str`].
    /// Round-trips with [`Priority::as_str`].
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Priority::Low),
            "medium" => Some(Priority::Medium),
            "high" => Some(Priority::High),
            "urgent" => Some(Priority::Urgent),
            _ => None,
        }
    }

    /// Numeric rank for sorting (higher = more important). Mirrors the derived
    /// `Ord`; provided as a convenience for explicit sort keys.
    // The app sorts via the derived `Ord` directly; kept for tests / Phase 2/3.
    #[allow(dead_code)]
    pub fn rank(&self) -> i32 {
        match self {
            Priority::Low => 0,
            Priority::Medium => 1,
            Priority::High => 2,
            Priority::Urgent => 3,
        }
    }

    /// Cycle to the next priority, wrapping around:
    /// Low -> Medium -> High -> Urgent -> Low. Used by the `p` keybinding.
    pub fn next(self) -> Self {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Urgent,
            Priority::Urgent => Priority::Low,
        }
    }
}

/// A single task. The `serde` derives define the future sync wire format, so
/// field names and enum spellings are part of the contract.
///
/// Timestamps are stored in UTC; rendering converts to local time elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Client-generated, time-sortable UUIDv7.
    pub id: Uuid,
    /// Required short title.
    pub title: String,
    /// Optional longer description.
    pub notes: Option<String>,
    pub status: Status,
    pub priority: Priority,
    /// Optional due date (date only, no time).
    pub due_date: Option<NaiveDate>,
    /// Optional free-text project grouping.
    pub project: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Last-write-wins clock (Phase 3).
    pub updated_at: DateTime<Utc>,
    /// Set when status becomes `Done`, cleared otherwise.
    pub completed_at: Option<DateTime<Utc>>,
    /// Tombstone / soft-delete flag.
    pub deleted: bool,
    /// Server cursor; `None` until synced (Phase 3).
    pub server_seq: Option<i64>,
}

impl Task {
    /// Create a new task with a fresh UUIDv7 id and sensible defaults.
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Task {
            id: Uuid::now_v7(),
            title,
            notes: None,
            status: Status::Todo,
            priority: Priority::Medium,
            due_date: None,
            project: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
            deleted: false,
            server_seq: None,
        }
    }

    /// Bump `updated_at` to now. Call after any user-visible edit so the
    /// last-write-wins clock advances.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Set the status while maintaining the `completed_at` invariant:
    /// entering `Done` stamps `completed_at`, leaving `Done` clears it.
    /// A redundant `Done -> Done` set preserves the original completion time.
    /// Always bumps `updated_at`.
    pub fn set_status(&mut self, status: Status) {
        let now = Utc::now();
        match status {
            // Stamp only on the transition *into* Done so a redundant
            // Done -> Done set keeps the original completion time.
            Status::Done if self.status != Status::Done => self.completed_at = Some(now),
            Status::Done => {}
            _ => self.completed_at = None,
        }
        self.status = status;
        self.updated_at = now;
    }

    /// Toggle between `Done` and `Todo` (the `space` key). Any non-done status
    /// becomes `Done`; `Done` becomes `Todo`.
    pub fn toggle_done(&mut self) {
        let next = match self.status {
            Status::Done => Status::Todo,
            _ => Status::Done,
        };
        self.set_status(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_str_roundtrip() {
        for s in [Status::Todo, Status::InProgress, Status::Done] {
            assert_eq!(Status::from_str(s.as_str()), Some(s));
        }
        assert_eq!(Status::Todo.as_str(), "todo");
        assert_eq!(Status::InProgress.as_str(), "in_progress");
        assert_eq!(Status::Done.as_str(), "done");
        assert_eq!(Status::from_str("bogus"), None);
    }

    #[test]
    fn priority_str_roundtrip() {
        for p in [
            Priority::Low,
            Priority::Medium,
            Priority::High,
            Priority::Urgent,
        ] {
            assert_eq!(Priority::from_str(p.as_str()), Some(p));
        }
        assert_eq!(Priority::Low.as_str(), "low");
        assert_eq!(Priority::Medium.as_str(), "medium");
        assert_eq!(Priority::High.as_str(), "high");
        assert_eq!(Priority::Urgent.as_str(), "urgent");
        assert_eq!(Priority::from_str("bogus"), None);
    }

    #[test]
    fn status_next_wraps() {
        assert_eq!(Status::Todo.next(), Status::InProgress);
        assert_eq!(Status::InProgress.next(), Status::Done);
        assert_eq!(Status::Done.next(), Status::Todo);
    }

    #[test]
    fn priority_next_wraps() {
        assert_eq!(Priority::Low.next(), Priority::Medium);
        assert_eq!(Priority::Medium.next(), Priority::High);
        assert_eq!(Priority::High.next(), Priority::Urgent);
        assert_eq!(Priority::Urgent.next(), Priority::Low);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Urgent > Priority::Low);
        assert!(Priority::Urgent > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
        assert!(Priority::Urgent.rank() > Priority::Low.rank());

        // "priority descending" sort, as used by the Today view.
        let mut ps = vec![
            Priority::Medium,
            Priority::Urgent,
            Priority::Low,
            Priority::High,
        ];
        ps.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            ps,
            vec![
                Priority::Urgent,
                Priority::High,
                Priority::Medium,
                Priority::Low
            ]
        );
    }

    #[test]
    fn defaults() {
        assert_eq!(Status::default(), Status::Todo);
        assert_eq!(Priority::default(), Priority::Medium);
    }

    #[test]
    fn new_task_defaults() {
        let t = Task::new("write tests".to_string());
        assert_eq!(t.title, "write tests");
        assert_eq!(t.status, Status::Todo);
        assert_eq!(t.priority, Priority::Medium);
        assert!(!t.deleted);
        assert!(t.notes.is_none());
        assert!(t.due_date.is_none());
        assert!(t.project.is_none());
        assert!(t.completed_at.is_none());
        assert!(t.server_seq.is_none());
        assert_eq!(t.created_at, t.updated_at);
        // UUIDv7 is version 7.
        assert_eq!(t.id.get_version_num(), 7);
    }

    #[test]
    fn set_status_done_stamps_completed_and_bumps_updated() {
        let mut t = Task::new("ship".to_string());
        let before = t.updated_at;
        t.set_status(Status::Done);
        assert_eq!(t.status, Status::Done);
        assert!(t.completed_at.is_some());
        assert!(t.updated_at >= before);
        let completed = t.completed_at.unwrap();
        assert!(t.updated_at >= completed - chrono::Duration::milliseconds(1));
    }

    #[test]
    fn set_status_done_to_done_preserves_completed_at() {
        let mut t = Task::new("ship".to_string());
        t.set_status(Status::Done);
        let first = t.completed_at.expect("completed_at set on entering Done");
        // Re-setting Done must not clobber the original completion time...
        t.set_status(Status::Done);
        assert_eq!(t.completed_at, Some(first));
        // ...but updated_at still advances (monotonic-ish via Utc::now()).
        assert!(t.updated_at >= first);
    }

    #[test]
    fn leaving_done_clears_completed() {
        let mut t = Task::new("ship".to_string());
        t.set_status(Status::Done);
        assert!(t.completed_at.is_some());
        t.set_status(Status::InProgress);
        assert_eq!(t.status, Status::InProgress);
        assert!(t.completed_at.is_none());
    }

    #[test]
    fn toggle_done_roundtrips() {
        let mut t = Task::new("ship".to_string());
        assert_eq!(t.status, Status::Todo);
        t.toggle_done();
        assert_eq!(t.status, Status::Done);
        assert!(t.completed_at.is_some());
        t.toggle_done();
        assert_eq!(t.status, Status::Todo);
        assert!(t.completed_at.is_none());
    }

    #[test]
    fn touch_bumps_updated_at() {
        let mut t = Task::new("ship".to_string());
        let before = t.updated_at;
        t.touch();
        assert!(t.updated_at >= before);
    }

    #[test]
    fn task_serde_roundtrip_uses_snake_case() {
        let mut t = Task::new("serialize me".to_string());
        t.notes = Some("a note".to_string());
        t.priority = Priority::Urgent;
        t.project = Some("enjo".to_string());
        t.due_date = Some(NaiveDate::from_ymd_opt(2026, 6, 27).unwrap());
        t.set_status(Status::InProgress);
        t.server_seq = Some(42);

        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"status\":\"in_progress\""));
        assert!(json.contains("\"priority\":\"urgent\""));

        let back: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn enum_serde_matches_wire_spelling() {
        assert_eq!(
            serde_json::to_string(&Status::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&Priority::Urgent).unwrap(),
            "\"urgent\""
        );
        let s: Status = serde_json::from_str("\"done\"").unwrap();
        assert_eq!(s, Status::Done);
        let p: Priority = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(p, Priority::Low);
    }
}
