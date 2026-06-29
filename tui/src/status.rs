//! Headless `enjo status` output for embedding in a status bar (e.g. tmux).
//!
//! Prints a one-line summary of the current in-progress work so a status bar
//! can answer "what am I doing right now?" at a glance. It reuses the same
//! Today ordering as the TUI (priority desc, then due date asc, then created
//! asc), so the task shown here is exactly the one that tops the In-progress
//! section in the app.

use crate::model::Task;

/// Default maximum width (in characters) of the rendered title before it is
/// truncated with an ellipsis.
pub const DEFAULT_MAX_LEN: usize = 40;

/// Text shown when nothing is in progress.
pub const IDLE_TEXT: &str = "idle";

/// Build the one-line status string from the in-progress tasks (already in
/// Today sort order). enjo enforces a single work-in-progress task, so this
/// shows the one task's title (truncated to `max_len`), or `idle` when none is
/// in progress. If several somehow exist, only the leading one is shown.
pub fn format_status(in_progress: &[Task], max_len: usize) -> String {
    match in_progress.first() {
        None => IDLE_TEXT.to_string(),
        Some(top) => truncate(&top.title, max_len),
    }
}

/// Escape `#` as `##` so that arbitrary task titles cannot inject tmux format
/// sequences (`#[...]`, `#{...}`, `#(...)`) into the status line. In tmux `##`
/// renders as a single literal `#`.
pub fn escape_tmux(s: &str) -> String {
    s.replace('#', "##")
}

/// Truncate `s` to at most `max_len` characters (counting Unicode scalar
/// values, not bytes), appending `…` when anything was dropped. Surrounding
/// whitespace is trimmed first.
fn truncate(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    // Reserve one column for the ellipsis.
    let take = max_len - 1;
    let mut out: String = trimmed.chars().take(take).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Priority, Task};

    fn task(title: &str) -> Task {
        let mut t = Task::new(title.to_string());
        t.priority = Priority::Medium;
        t
    }

    #[test]
    fn idle_when_nothing_in_progress() {
        assert_eq!(format_status(&[], DEFAULT_MAX_LEN), "idle");
    }

    #[test]
    fn single_task_shows_title() {
        let tasks = vec![task("Write the parser")];
        assert_eq!(format_status(&tasks, DEFAULT_MAX_LEN), "Write the parser");
    }

    #[test]
    fn multiple_in_progress_shows_only_the_first() {
        // The single-WIP rule means this shouldn't occur in normal use, but if
        // several are present we still render just the leading task (no counter).
        let tasks = vec![task("Top task"), task("Second"), task("Third")];
        assert_eq!(format_status(&tasks, DEFAULT_MAX_LEN), "Top task");
    }

    #[test]
    fn long_title_is_truncated_with_ellipsis() {
        let tasks = vec![task("abcdefghij")];
        let out = format_status(&tasks, 5);
        assert_eq!(out, "abcd…");
        assert_eq!(out.chars().count(), 5);
    }

    #[test]
    fn truncation_counts_chars_not_bytes() {
        // Each "é" is multi-byte; truncating must not split a scalar.
        let tasks = vec![task("ééééééé")];
        let out = format_status(&tasks, 3);
        assert_eq!(out, "éé…");
        assert_eq!(out.chars().count(), 3);
    }

    #[test]
    fn title_is_trimmed() {
        let tasks = vec![task("  spaced  ")];
        assert_eq!(format_status(&tasks, DEFAULT_MAX_LEN), "spaced");
    }

    #[test]
    fn zero_max_len_yields_empty() {
        let tasks = vec![task("anything")];
        assert_eq!(format_status(&tasks, 0), "");
    }

    #[test]
    fn escape_tmux_doubles_hashes() {
        assert_eq!(escape_tmux("fix #42 and #[bold]"), "fix ##42 and ##[bold]");
        assert_eq!(escape_tmux("no hashes"), "no hashes");
    }
}
