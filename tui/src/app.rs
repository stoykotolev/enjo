//! Terminal-independent application state and key handling for enjo.
//!
//! [`App`] owns a boxed [`Store`] plus in-memory view state. Every mutation goes
//! through the store and is followed by a [`App::reload`] that refreshes the
//! in-memory task list, so the whole interaction model can be driven and asserted
//! in tests with an in-memory store and *no* terminal involved.

use anyhow::Result;
use chrono::{Local, NaiveDate};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use crate::model::{Priority, Status, Task};
use crate::store::Store;

/// The top-level screen the user is looking at. `Today` and `All` are the two
/// list views; `Edit` and `Help` are overlays drawn on top of the last list view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Today,
    All,
    Edit,
    Help,
}

/// Status filter cycled by `/` on the All view: All -> Todo -> InProgress -> Done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Todo,
    InProgress,
    Done,
}

impl StatusFilter {
    /// Human-readable label shown in the header/footer.
    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All",
            StatusFilter::Todo => "Todo",
            StatusFilter::InProgress => "In progress",
            StatusFilter::Done => "Done",
        }
    }

    fn next(self) -> Self {
        match self {
            StatusFilter::All => StatusFilter::Todo,
            StatusFilter::Todo => StatusFilter::InProgress,
            StatusFilter::InProgress => StatusFilter::Done,
            StatusFilter::Done => StatusFilter::All,
        }
    }

    fn matches(self, status: Status) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Todo => status == Status::Todo,
            StatusFilter::InProgress => status == Status::InProgress,
            StatusFilter::Done => status == Status::Done,
        }
    }
}

/// Focusable field in the edit form, in `Tab` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Notes,
    Project,
    DueDate,
    Priority,
    Status,
}

impl EditField {
    fn next(self) -> Self {
        match self {
            EditField::Title => EditField::Notes,
            EditField::Notes => EditField::Project,
            EditField::Project => EditField::DueDate,
            EditField::DueDate => EditField::Priority,
            EditField::Priority => EditField::Status,
            EditField::Status => EditField::Title,
        }
    }

    fn prev(self) -> Self {
        match self {
            EditField::Title => EditField::Status,
            EditField::Notes => EditField::Title,
            EditField::Project => EditField::Notes,
            EditField::DueDate => EditField::Project,
            EditField::Priority => EditField::DueDate,
            EditField::Status => EditField::Priority,
        }
    }
}

/// Mutable backing state for the edit form. Text fields are kept as plain strings
/// and only parsed/validated on save.
#[derive(Debug, Clone)]
pub struct EditState {
    /// `Some` when editing an existing task, `None` for a brand-new one.
    editing_id: Option<Uuid>,
    title: String,
    notes: String,
    project: String,
    due_date: String,
    priority: Priority,
    status: Status,
    field: EditField,
    /// List screen to return to on save/cancel.
    return_screen: Screen,
}

impl EditState {
    fn new(return_screen: Screen) -> Self {
        Self {
            editing_id: None,
            title: String::new(),
            notes: String::new(),
            project: String::new(),
            due_date: String::new(),
            priority: Priority::Medium,
            status: Status::Todo,
            field: EditField::Title,
            return_screen,
        }
    }

    fn from_task(task: &Task, return_screen: Screen) -> Self {
        Self {
            editing_id: Some(task.id),
            title: task.title.clone(),
            notes: task.notes.clone().unwrap_or_default(),
            project: task.project.clone().unwrap_or_default(),
            due_date: task
                .due_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            priority: task.priority,
            status: task.status,
            field: EditField::Title,
            return_screen,
        }
    }

    /// Mutable handle to the buffer of the focused text field, or `None` when the
    /// focus is on a non-text (priority/status) field.
    fn text_field_mut(&mut self) -> Option<&mut String> {
        match self.field {
            EditField::Title => Some(&mut self.title),
            EditField::Notes => Some(&mut self.notes),
            EditField::Project => Some(&mut self.project),
            EditField::DueDate => Some(&mut self.due_date),
            EditField::Priority | EditField::Status => None,
        }
    }

    // --- Accessors used by the UI layer. ---
    pub fn is_new(&self) -> bool {
        self.editing_id.is_none()
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn notes(&self) -> &str {
        &self.notes
    }
    pub fn project(&self) -> &str {
        &self.project
    }
    pub fn due_date(&self) -> &str {
        &self.due_date
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn field(&self) -> EditField {
        self.field
    }
}

/// One rendered section of the Today view, in display order.
pub struct TodaySection {
    pub title: &'static str,
    pub tasks: Vec<Task>,
}

/// The whole application: store-backed state plus transient UI state.
pub struct App {
    store: Box<dyn Store>,
    tasks: Vec<Task>,
    screen: Screen,
    selected: usize,
    edit: EditState,
    filter: StatusFilter,
    status_message: Option<String>,
    should_quit: bool,
    /// List screen to return to when leaving the Help overlay.
    help_return: Screen,
}

impl App {
    /// Build an `App`, loading the initial active task list from the store.
    pub fn new(store: Box<dyn Store>) -> Result<Self> {
        let tasks = store.list_active()?;
        Ok(Self {
            store,
            tasks,
            screen: Screen::Today,
            selected: 0,
            edit: EditState::new(Screen::Today),
            filter: StatusFilter::All,
            status_message: None,
            should_quit: false,
            help_return: Screen::Today,
        })
    }

    // ----- Public accessors for the UI / main loop. -----

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
    pub fn screen(&self) -> Screen {
        self.screen
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }
    pub fn filter(&self) -> StatusFilter {
        self.filter
    }
    pub fn edit_state(&self) -> &EditState {
        &self.edit
    }

    // ----- Derived views. -----

    /// The list screen currently in effect (Edit/Help overlays defer to the
    /// screen they were opened from). The renderer uses this to draw the list
    /// behind a modal overlay.
    pub fn list_screen(&self) -> Screen {
        match self.screen {
            Screen::Today => Screen::Today,
            Screen::All => Screen::All,
            Screen::Edit => self.edit.return_screen,
            Screen::Help => self.help_return,
        }
    }

    /// The three Today sections, each already sorted by the shared sort key.
    pub fn today_sections(&self) -> Vec<TodaySection> {
        let today = Local::now().date_naive();
        let mut in_progress = Vec::new();
        let mut overdue = Vec::new();
        let mut next_up = Vec::new();
        for task in &self.tasks {
            if task.deleted {
                continue;
            }
            match task.status {
                Status::InProgress => in_progress.push(task.clone()),
                Status::Todo => match task.due_date {
                    Some(d) if d <= today => overdue.push(task.clone()),
                    _ => next_up.push(task.clone()),
                },
                Status::Done => {}
            }
        }
        sort_tasks(&mut in_progress);
        sort_tasks(&mut overdue);
        sort_tasks(&mut next_up);
        vec![
            TodaySection {
                title: "In progress",
                tasks: in_progress,
            },
            TodaySection {
                title: "Overdue / due today",
                tasks: overdue,
            },
            TodaySection {
                title: "Next up",
                tasks: next_up,
            },
        ]
    }

    /// The in-progress tasks, in Today sort order (priority desc, due asc,
    /// created asc). Used by the headless `enjo status` command to surface
    /// "what am I working on right now" outside the TUI (e.g. a tmux status bar).
    pub fn in_progress_tasks(&self) -> Vec<Task> {
        self.today_sections()
            .into_iter()
            .next()
            .map(|s| s.tasks)
            .unwrap_or_default()
    }

    /// Flattened Today list (section1 ++ section2 ++ section3) the cursor indexes.
    fn today_visible(&self) -> Vec<Task> {
        self.today_sections()
            .into_iter()
            .flat_map(|s| s.tasks)
            .collect()
    }

    /// Every non-deleted task, for the Today screen's read-only overview pane:
    /// active tasks in sort order first, then completed ones (also sorted), so
    /// done items collect at the bottom. Ignores the All-view status filter.
    pub fn overview_tasks(&self) -> Vec<Task> {
        let (mut active, mut done): (Vec<Task>, Vec<Task>) = self
            .tasks
            .iter()
            .filter(|t| !t.deleted)
            .cloned()
            .partition(|t| t.status != Status::Done);
        sort_tasks(&mut active);
        sort_tasks(&mut done);
        active.append(&mut done);
        active
    }

    /// All active tasks passing the current status filter, in sort order.
    pub fn all_tasks(&self) -> Vec<Task> {
        let mut tasks: Vec<Task> = self
            .tasks
            .iter()
            .filter(|t| !t.deleted && self.filter.matches(t.status))
            .cloned()
            .collect();
        sort_tasks(&mut tasks);
        tasks
    }

    /// The flat list the selection cursor indexes for the active list screen.
    pub fn visible_tasks(&self) -> Vec<Task> {
        match self.list_screen() {
            Screen::All => self.all_tasks(),
            _ => self.today_visible(),
        }
    }

    /// The task under the cursor, if any.
    pub fn selected_task(&self) -> Option<Task> {
        self.visible_tasks().into_iter().nth(self.selected)
    }

    // ----- Key handling. -----

    /// Handle one key press. Dispatched by current screen. Store errors are
    /// surfaced into `status_message` rather than propagated, so the loop keeps
    /// running. The transient message is cleared at the top of every keypress.
    pub fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        self.status_message = None;
        match self.screen {
            Screen::Edit => self.on_key_edit(key)?,
            Screen::Help => self.on_key_help(key),
            Screen::Today | Screen::All => self.on_key_list(key)?,
        }
        Ok(())
    }

    fn on_key_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                self.screen = self.help_return;
            }
            _ => {}
        }
    }

    fn on_key_list(&mut self, key: KeyEvent) -> Result<()> {
        // Ctrl-S is the force-sync placeholder (frees plain `S` for reverse
        // status cycling). Some terminals send it as 's', others 'S'.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.status_message = Some("Sync arrives in Phase 3 (local-only build)".to_string());
            return Ok(());
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.select_prev(),
            KeyCode::Char('n') => self.open_new(),
            KeyCode::Char('e') | KeyCode::Enter => self.open_edit_selected(),
            KeyCode::Char(' ') => self.toggle_selected_done()?,
            // Lowercase steps forward through the cycle, Shift steps backward.
            KeyCode::Char('s') => self.cycle_selected_status(true)?,
            KeyCode::Char('S') => self.cycle_selected_status(false)?,
            KeyCode::Char('p') => self.cycle_selected_priority(true)?,
            KeyCode::Char('P') => self.cycle_selected_priority(false)?,
            KeyCode::Char('d') => self.delete_selected()?,
            KeyCode::Char('/') => self.cycle_filter(),
            KeyCode::Tab => self.toggle_view(),
            KeyCode::Char('?') => {
                self.help_return = self.screen;
                self.screen = Screen::Help;
            }
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
        Ok(())
    }

    fn on_key_edit(&mut self, key: KeyEvent) -> Result<()> {
        // Ctrl-S saves regardless of focused field.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            return self.save_edit();
        }
        match key.code {
            KeyCode::Esc => self.screen = self.edit.return_screen,
            KeyCode::Enter => return self.save_edit(),
            KeyCode::Tab | KeyCode::Down => self.edit.field = self.edit.field.next(),
            KeyCode::BackTab | KeyCode::Up => self.edit.field = self.edit.field.prev(),
            KeyCode::Left => self.edit_cycle_value(false),
            KeyCode::Right => self.edit_cycle_value(true),
            KeyCode::Backspace => {
                if let Some(buf) = self.edit.text_field_mut() {
                    buf.pop();
                }
            }
            KeyCode::Char(c) => match self.edit.field {
                EditField::Priority => {
                    if c == 'p' || c == 'P' {
                        self.edit.priority = self.edit.priority.next();
                    }
                }
                EditField::Status => {
                    if c == 's' || c == 'S' {
                        self.edit.status = self.edit.status.next();
                    }
                }
                _ => {
                    if let Some(buf) = self.edit.text_field_mut() {
                        buf.push(c);
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }

    /// Cycle the value of the focused priority/status field (left = back).
    fn edit_cycle_value(&mut self, forward: bool) {
        match self.edit.field {
            EditField::Priority => {
                self.edit.priority = if forward {
                    self.edit.priority.next()
                } else {
                    prev_priority(self.edit.priority)
                };
            }
            EditField::Status => {
                self.edit.status = if forward {
                    self.edit.status.next()
                } else {
                    prev_status(self.edit.status)
                };
            }
            _ => {}
        }
    }

    // ----- Mutations (all go through the store, then reload). -----

    fn open_new(&mut self) {
        self.edit = EditState::new(self.list_screen());
        self.screen = Screen::Edit;
    }

    fn open_edit_selected(&mut self) {
        match self.selected_task() {
            Some(task) => {
                self.edit = EditState::from_task(&task, self.list_screen());
                self.screen = Screen::Edit;
            }
            None => self.status_message = Some("No task selected".to_string()),
        }
    }

    /// Validate and persist the edit form, then return to the prior screen.
    /// Validation failures leave the form open with a status message.
    fn save_edit(&mut self) -> Result<()> {
        let title = self.edit.title.trim().to_string();
        if title.is_empty() {
            self.status_message = Some("Title cannot be empty".to_string());
            return Ok(());
        }
        let due_date = match self.edit.due_date.trim() {
            "" => None,
            s => match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                Ok(d) => Some(d),
                Err(_) => {
                    self.status_message = Some(format!("Invalid date '{s}' (expected YYYY-MM-DD)"));
                    return Ok(());
                }
            },
        };

        let mut task = match self.edit.editing_id {
            Some(id) => match self.store.get(id) {
                Ok(Some(t)) => t,
                Ok(None) => {
                    self.status_message = Some("Task no longer exists".to_string());
                    self.screen = self.edit.return_screen;
                    self.reload();
                    self.clamp_selection();
                    return Ok(());
                }
                Err(e) => {
                    // A transient read error must not crash the app; stay on the
                    // form and surface the failure.
                    self.status_message = Some(format!("Failed to load task: {e}"));
                    return Ok(());
                }
            },
            None => Task::new(title.clone()),
        };

        // Enforce a single work-in-progress task: refuse to save this one as
        // InProgress while another already is, keeping the form open (like the
        // other validation failures above). Editing the in-progress task itself
        // is fine — it's excluded via `editing_id`.
        if self.edit.status == Status::InProgress {
            if let Some(other) = self.other_in_progress_title(self.edit.editing_id) {
                self.status_message = Some(format!(
                    "Already in progress: '{other}'. Finish or pause it first."
                ));
                return Ok(());
            }
        }

        task.title = title;
        task.notes = non_empty(&self.edit.notes);
        task.project = non_empty(&self.edit.project);
        task.due_date = due_date;
        task.priority = self.edit.priority;
        task.set_status(self.edit.status);
        task.touch();

        if let Err(e) = self.store.upsert(&task) {
            self.status_message = Some(format!("Save failed: {e}"));
            return Ok(());
        }

        let return_screen = self.edit.return_screen;
        self.screen = return_screen;
        if self.reload() {
            self.select_task_by_id(task.id);
            self.status_message = Some("Saved".to_string());
        }
        Ok(())
    }

    /// The title of another (non-deleted) task already in progress, excluding
    /// `except`. enjo enforces a single work-in-progress task, so a task may
    /// only move to `InProgress` when this returns `None`.
    fn other_in_progress_title(&self, except: Option<Uuid>) -> Option<String> {
        self.tasks
            .iter()
            .find(|t| !t.deleted && t.status == Status::InProgress && Some(t.id) != except)
            .map(|t| t.title.clone())
    }

    fn toggle_selected_done(&mut self) -> Result<()> {
        if let Some(mut task) = self.selected_task() {
            task.toggle_done();
            task.touch();
            self.persist_and_reload(task)?;
        }
        Ok(())
    }

    fn cycle_selected_status(&mut self, forward: bool) -> Result<()> {
        if let Some(mut task) = self.selected_task() {
            let next = if forward {
                task.status.next()
            } else {
                prev_status(task.status)
            };
            // Only one task may be in progress at a time: block the move and
            // point at the task that's already in progress. The guard keys off
            // the *target* status, so it covers stepping backward into
            // in-progress (Done -> InProgress) as well as forward.
            if next == Status::InProgress {
                if let Some(other) = self.other_in_progress_title(Some(task.id)) {
                    self.status_message = Some(format!(
                        "Already in progress: '{other}'. Finish or pause it first."
                    ));
                    return Ok(());
                }
            }
            task.set_status(next);
            task.touch();
            self.persist_and_reload(task)?;
        }
        Ok(())
    }

    fn cycle_selected_priority(&mut self, forward: bool) -> Result<()> {
        if let Some(mut task) = self.selected_task() {
            task.priority = if forward {
                task.priority.next()
            } else {
                prev_priority(task.priority)
            };
            task.touch();
            self.persist_and_reload(task)?;
        }
        Ok(())
    }

    fn delete_selected(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task() {
            if let Err(e) = self.store.soft_delete(task.id) {
                self.status_message = Some(format!("Delete failed: {e}"));
                return Ok(());
            }
            if self.reload() {
                self.clamp_selection();
                self.status_message = Some("Deleted".to_string());
            }
        }
        Ok(())
    }

    fn cycle_filter(&mut self) {
        self.filter = self.filter.next();
        self.clamp_selection();
    }

    fn toggle_view(&mut self) {
        self.screen = match self.screen {
            Screen::Today => Screen::All,
            _ => Screen::Today,
        };
        self.selected = 0;
    }

    /// Upsert and reload, surfacing store errors into the status message.
    fn persist_and_reload(&mut self, task: Task) -> Result<()> {
        if let Err(e) = self.store.upsert(&task) {
            self.status_message = Some(format!("Update failed: {e}"));
            return Ok(());
        }
        let id = task.id;
        self.reload();
        // Keep the cursor on the edited task even if its new priority/status
        // reordered the list. Falls back to clamping if it left the view.
        self.select_task_by_id(id);
        Ok(())
    }

    /// Refresh the in-memory task list from the store. A transient read error is
    /// surfaced into `status_message` (keeping the previous in-memory `tasks`)
    /// rather than propagated, so an in-loop reload can never crash the TUI.
    /// Returns `true` on success so callers can skip success-path follow-ups.
    fn reload(&mut self) -> bool {
        match self.store.list_active() {
            Ok(tasks) => {
                self.tasks = tasks;
                true
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to reload tasks: {e}"));
                false
            }
        }
    }

    // ----- Selection helpers (clamp, never wrap). -----

    fn select_next(&mut self) {
        let len = self.visible_tasks().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_tasks().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn select_task_by_id(&mut self, id: Uuid) {
        match self.visible_tasks().iter().position(|t| t.id == id) {
            Some(idx) => self.selected = idx,
            None => self.clamp_selection(),
        }
    }
}

/// Shared sort key for Today and All: priority descending, then due date
/// ascending (None last), then created_at ascending.
fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| due_key(a.due_date).cmp(&due_key(b.due_date)))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
}

/// Map a due date to a sort key, sending `None` to the far future ("last").
fn due_key(d: Option<NaiveDate>) -> NaiveDate {
    d.unwrap_or(NaiveDate::MAX)
}

/// Trim and convert an empty string to `None`, else `Some(trimmed)`.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn prev_priority(p: Priority) -> Priority {
    // 4-variant wrapping cycle: three forward steps == one back.
    p.next().next().next()
}

fn prev_status(s: Status) -> Status {
    // 3-variant wrapping cycle: two forward steps == one back.
    s.next().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;
    use chrono::Duration;

    fn app() -> App {
        App::new(Box::new(SqliteStore::open_in_memory().unwrap())).unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.on_key(key(KeyCode::Char(c))).unwrap();
        }
    }

    /// Seed a task straight through the store, then reload the app.
    fn seed(app: &mut App, task: Task) {
        app.store.upsert(&task).unwrap();
        assert!(app.reload(), "in-memory reload should succeed");
    }

    /// A `Store` whose reads always fail, used to prove the app degrades
    /// gracefully on a transient read error instead of crashing.
    struct FailingReadStore;

    impl Store for FailingReadStore {
        fn list_active(&self) -> Result<Vec<Task>> {
            Err(anyhow::anyhow!("boom"))
        }
        fn get(&self, _id: Uuid) -> Result<Option<Task>> {
            Err(anyhow::anyhow!("boom"))
        }
        fn upsert(&self, _task: &Task) -> Result<()> {
            Ok(())
        }
        fn soft_delete(&self, _id: Uuid) -> Result<()> {
            Ok(())
        }
    }

    fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    #[test]
    fn add_task_via_form_appears_in_today() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        assert_eq!(a.screen(), Screen::Edit);
        type_str(&mut a, "Buy milk");
        a.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(a.screen(), Screen::Today);
        let titles: Vec<_> = a.visible_tasks().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["Buy milk".to_string()]);
    }

    #[test]
    fn empty_title_keeps_form_open_with_message() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        a.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(a.screen(), Screen::Edit);
        assert!(a.status_message().unwrap().contains("Title"));
        assert!(a.visible_tasks().is_empty());
    }

    #[test]
    fn invalid_due_date_keeps_form_open() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut a, "Has date");
        // Move to the due-date field and type garbage.
        a.on_key(key(KeyCode::Tab)).unwrap(); // notes
        a.on_key(key(KeyCode::Tab)).unwrap(); // project
        a.on_key(key(KeyCode::Tab)).unwrap(); // due date
        type_str(&mut a, "not-a-date");
        a.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(a.screen(), Screen::Edit);
        assert!(a.status_message().unwrap().contains("date"));
    }

    #[test]
    fn navigation_clamps_at_both_ends() {
        let mut a = app();
        seed(&mut a, Task::new("a".into()));
        seed(&mut a, Task::new("b".into()));
        seed(&mut a, Task::new("c".into()));
        assert_eq!(a.selected(), 0);

        // Up at the top stays at 0.
        a.on_key(key(KeyCode::Char('k'))).unwrap();
        assert_eq!(a.selected(), 0);

        a.on_key(key(KeyCode::Char('j'))).unwrap();
        a.on_key(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(a.selected(), 2);
        // Down at the bottom stays at the last index (clamp, no wrap).
        a.on_key(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(a.selected(), 2);
        assert!(a.selected_task().is_some());
    }

    #[test]
    fn toggle_done_removes_task_from_today() {
        let mut a = app();
        seed(&mut a, Task::new("finish me".into()));
        assert_eq!(a.visible_tasks().len(), 1);

        a.on_key(key(KeyCode::Char(' '))).unwrap();
        // Done tasks are excluded from Today.
        assert!(a.visible_tasks().is_empty());

        // ...but they remain in the store with Done status (visible on All).
        let all: Vec<_> = a.tasks.clone();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, Status::Done);
    }

    #[test]
    fn cycle_status_persists() {
        let mut a = app();
        seed(&mut a, Task::new("cycle".into()));
        let id = a.tasks[0].id;
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // Todo -> InProgress
        let got = a.store.get(id).unwrap().unwrap();
        assert_eq!(got.status, Status::InProgress);
    }

    #[test]
    fn cannot_start_second_in_progress_via_s_key() {
        let mut a = app();
        seed(&mut a, Task::new("first".into()));
        seed(&mut a, Task::new("second".into()));

        // Start the first task (Todo -> InProgress) — allowed.
        a.on_key(key(KeyCode::Char('s'))).unwrap();
        // Move to the second task and try to start it too — blocked.
        a.on_key(key(KeyCode::Char('j'))).unwrap();
        a.on_key(key(KeyCode::Char('s'))).unwrap();

        let in_progress: Vec<_> = a
            .tasks
            .iter()
            .filter(|t| t.status == Status::InProgress)
            .map(|t| t.title.clone())
            .collect();
        assert_eq!(in_progress, vec!["first".to_string()]);
        assert!(a.status_message().unwrap().contains("Already in progress"));
    }

    #[test]
    fn in_progress_task_can_still_advance_to_done() {
        let mut a = app();
        seed(&mut a, Task::new("solo".into()));
        let id = a.tasks[0].id;
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // Todo -> InProgress
        assert_eq!(a.store.get(id).unwrap().unwrap().status, Status::InProgress);
        // The single-WIP guard only blocks entering InProgress; advancing the
        // in-progress task itself to Done is still allowed.
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // InProgress -> Done
        assert_eq!(a.store.get(id).unwrap().unwrap().status, Status::Done);
    }

    #[test]
    fn cannot_set_in_progress_via_edit_form_when_one_exists() {
        let mut a = app();
        seed(&mut a, Task::new("active".into()));
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // active -> InProgress

        // Create a new task and try to mark it InProgress in the form.
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut a, "newbie");
        a.on_key(key(KeyCode::BackTab)).unwrap(); // Title -> Status
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // Todo -> InProgress (form buffer)
        a.on_key(key(KeyCode::Enter)).unwrap(); // save -> blocked

        assert_eq!(a.screen(), Screen::Edit);
        assert!(a.status_message().unwrap().contains("Already in progress"));
        let titles: Vec<_> = a.tasks.iter().map(|t| t.title.clone()).collect();
        assert!(!titles.contains(&"newbie".to_string()));
    }

    #[test]
    fn editing_in_progress_task_can_keep_it_in_progress() {
        let mut a = app();
        seed(&mut a, Task::new("focus".into()));
        a.on_key(key(KeyCode::Char('s'))).unwrap(); // focus -> InProgress
        let id = a.tasks[0].id;

        // Editing the in-progress task itself (status stays InProgress) must not
        // be blocked — it is excluded from the "other in progress" check.
        a.on_key(key(KeyCode::Char('e'))).unwrap();
        type_str(&mut a, " edited");
        a.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(a.screen(), Screen::Today);
        let got = a.store.get(id).unwrap().unwrap();
        assert_eq!(got.status, Status::InProgress);
        assert_eq!(got.title, "focus edited");
    }

    #[test]
    fn cycle_priority_persists() {
        let mut a = app();
        seed(&mut a, Task::new("cycle".into()));
        let id = a.tasks[0].id;
        // Default Medium -> High.
        a.on_key(key(KeyCode::Char('p'))).unwrap();
        let got = a.store.get(id).unwrap().unwrap();
        assert_eq!(got.priority, Priority::High);
    }

    #[test]
    fn soft_delete_removes_and_reclamps() {
        let mut a = app();
        seed(&mut a, Task::new("one".into()));
        seed(&mut a, Task::new("two".into()));
        // Select the last item, then delete it.
        a.on_key(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(a.selected(), 1);
        a.on_key(key(KeyCode::Char('d'))).unwrap();

        assert_eq!(a.visible_tasks().len(), 1);
        // Selection re-clamped to the remaining valid index.
        assert_eq!(a.selected(), 0);
        assert!(a.selected_task().is_some());
    }

    #[test]
    fn today_sectioning_and_ordering() {
        let mut a = app();
        let t = today();

        // Section 1: in progress (two, to check priority tie-break).
        let mut ip_low = Task::new("ip-low".into());
        ip_low.set_status(Status::InProgress);
        ip_low.priority = Priority::Low;
        let mut ip_high = Task::new("ip-high".into());
        ip_high.set_status(Status::InProgress);
        ip_high.priority = Priority::High;

        // Section 2: overdue todo (due yesterday).
        let mut overdue = Task::new("overdue".into());
        overdue.due_date = Some(t - Duration::days(1));

        // Section 3: future todo, plus an undated todo.
        let mut future = Task::new("future".into());
        future.due_date = Some(t + Duration::days(10));
        let undated = Task::new("undated".into());

        // Excluded entirely.
        let mut done = Task::new("done".into());
        done.set_status(Status::Done);

        for task in [
            ip_low.clone(),
            ip_high.clone(),
            overdue.clone(),
            future.clone(),
            undated.clone(),
            done.clone(),
        ] {
            seed(&mut a, task);
        }

        let sections = a.today_sections();
        assert_eq!(sections[0].title, "In progress");
        // Priority-desc tie-break: High before Low.
        assert_eq!(
            sections[0]
                .tasks
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            vec!["ip-high", "ip-low"]
        );
        assert_eq!(
            sections[1]
                .tasks
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            vec!["overdue"]
        );
        // Next up holds both the future-dated and undated todos (done excluded).
        let next_titles: Vec<_> = sections[2].tasks.iter().map(|t| t.title.as_str()).collect();
        assert!(next_titles.contains(&"future"));
        assert!(next_titles.contains(&"undated"));
        assert!(!next_titles.contains(&"done"));

        // Flattened visible order = s1 ++ s2 ++ s3.
        let flat: Vec<_> = a.visible_tasks().into_iter().map(|t| t.title).collect();
        assert_eq!(flat[0], "ip-high");
        assert_eq!(flat[1], "ip-low");
        assert_eq!(flat[2], "overdue");
        // No Done task anywhere in Today.
        assert!(!flat.contains(&"done".to_string()));
        assert_eq!(flat.len(), 5);
    }

    #[test]
    fn all_view_status_filter_cycles() {
        let mut a = app();
        let mut todo = Task::new("todo".into());
        todo.set_status(Status::Todo);
        let mut prog = Task::new("prog".into());
        prog.set_status(Status::InProgress);
        let mut done = Task::new("done".into());
        done.set_status(Status::Done);
        seed(&mut a, todo);
        seed(&mut a, prog);
        seed(&mut a, done);

        // Switch to All view.
        a.on_key(key(KeyCode::Tab)).unwrap();
        assert_eq!(a.screen(), Screen::All);
        assert_eq!(a.filter(), StatusFilter::All);
        assert_eq!(a.visible_tasks().len(), 3);

        // All -> Todo.
        a.on_key(key(KeyCode::Char('/'))).unwrap();
        assert_eq!(a.filter(), StatusFilter::Todo);
        assert_eq!(a.visible_tasks().len(), 1);
        assert_eq!(a.visible_tasks()[0].status, Status::Todo);

        // Todo -> InProgress.
        a.on_key(key(KeyCode::Char('/'))).unwrap();
        assert_eq!(a.filter(), StatusFilter::InProgress);
        assert_eq!(a.visible_tasks()[0].status, Status::InProgress);

        // InProgress -> Done.
        a.on_key(key(KeyCode::Char('/'))).unwrap();
        assert_eq!(a.filter(), StatusFilter::Done);
        assert_eq!(a.visible_tasks()[0].status, Status::Done);

        // Done -> All (wrap).
        a.on_key(key(KeyCode::Char('/'))).unwrap();
        assert_eq!(a.filter(), StatusFilter::All);
        assert_eq!(a.visible_tasks().len(), 3);
    }

    #[test]
    fn edit_existing_task_updates_fields() {
        let mut a = app();
        seed(&mut a, Task::new("orig".into()));
        a.on_key(key(KeyCode::Char('e'))).unwrap();
        assert_eq!(a.screen(), Screen::Edit);
        // Clear the title and retype.
        for _ in 0.."orig".len() {
            a.on_key(key(KeyCode::Backspace)).unwrap();
        }
        type_str(&mut a, "updated");
        a.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(a.screen(), Screen::Today);
        assert_eq!(a.visible_tasks()[0].title, "updated");
    }

    #[test]
    fn sync_key_shows_local_only_message() {
        let mut a = app();
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(a.status_message().unwrap().contains("Phase 3"));
    }

    #[test]
    fn shift_s_steps_status_backward() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut a, "task");
        a.on_key(key(KeyCode::Enter)).unwrap();
        // Forward once: Todo -> InProgress.
        a.on_key(key(KeyCode::Char('s'))).unwrap();
        assert_eq!(a.visible_tasks()[0].status, Status::InProgress);
        // Shift+S steps back: InProgress -> Todo, without passing through Done.
        a.on_key(key(KeyCode::Char('S'))).unwrap();
        assert_eq!(a.visible_tasks()[0].status, Status::Todo);
    }

    #[test]
    fn overview_lists_all_tasks_with_done_last() {
        let mut a = app();
        let mut done = Task::new("finished".into());
        done.set_status(Status::Done);
        seed(&mut a, done);
        seed(&mut a, Task::new("todo".into()));
        let mut wip = Task::new("working".into());
        wip.set_status(Status::InProgress);
        seed(&mut a, wip);

        let titles: Vec<_> = a
            .overview_tasks()
            .into_iter()
            .map(|t| t.title)
            .collect();
        // All three present (done is excluded from Today but shown here)…
        assert_eq!(titles.len(), 3);
        // …and the completed task sorts to the bottom.
        assert_eq!(titles.last().unwrap(), "finished");
    }

    #[test]
    fn cursor_follows_task_when_priority_reorders() {
        let mut a = app();
        // Two tasks at default priority (Medium); insertion order is the tie-break,
        // so "a" sorts first and "b" second.
        seed(&mut a, Task::new("a".into()));
        seed(&mut a, Task::new("b".into()));
        // Select "b" (index 1) and bump its priority up so it sorts to the top.
        a.on_key(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(a.visible_tasks()[a.selected()].title, "b");
        a.on_key(key(KeyCode::Char('p'))).unwrap(); // Medium -> High, "b" moves up
        // Cursor follows "b" to its new position (now index 0).
        assert_eq!(a.visible_tasks()[a.selected()].title, "b");
        assert_eq!(a.selected(), 0);
    }

    #[test]
    fn shift_p_steps_priority_backward() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut a, "task");
        a.on_key(key(KeyCode::Enter)).unwrap();
        // Default priority is Medium; one step back -> Low.
        a.on_key(key(KeyCode::Char('P'))).unwrap();
        assert_eq!(a.visible_tasks()[0].priority, Priority::Low);
    }

    #[test]
    fn shift_s_into_in_progress_respects_single_wip() {
        let mut a = app();
        // "first" is already in progress; "second" is Done. Stepping "second"
        // backward (Done -> InProgress) must be blocked by the single-WIP guard.
        let mut first = Task::new("first".into());
        first.set_status(Status::InProgress);
        seed(&mut a, first);
        let mut second = Task::new("second".into());
        second.set_status(Status::Done);
        seed(&mut a, second);
        let second_id = a.tasks.iter().find(|t| t.title == "second").unwrap().id;

        // Done tasks are hidden on Today; switch to All to select "second".
        a.on_key(key(KeyCode::Tab)).unwrap();
        let second_idx = a
            .visible_tasks()
            .iter()
            .position(|t| t.title == "second")
            .unwrap();
        for _ in 0..second_idx {
            a.on_key(key(KeyCode::Char('j'))).unwrap();
        }
        a.on_key(key(KeyCode::Char('S'))).unwrap(); // Done -> InProgress, blocked

        assert_eq!(a.store.get(second_id).unwrap().unwrap().status, Status::Done);
        assert!(a.status_message().unwrap().contains("Already in progress"));
    }

    #[test]
    fn quit_sets_flag() {
        let mut a = app();
        assert!(!a.should_quit());
        a.on_key(key(KeyCode::Char('q'))).unwrap();
        assert!(a.should_quit());
    }

    #[test]
    fn reload_read_error_keeps_tasks_and_sets_status() {
        let mut a = app();
        seed(&mut a, Task::new("survivor".into()));
        assert_eq!(a.visible_tasks().len(), 1);

        // Swap in a store whose reads fail, then force a reload.
        a.store = Box::new(FailingReadStore);
        let ok = a.reload();
        assert!(!ok, "reload must report failure");
        // Previous in-memory task list is retained, app stays usable.
        assert_eq!(a.tasks.len(), 1);
        assert!(a
            .status_message()
            .unwrap()
            .contains("Failed to reload tasks"));
    }

    #[test]
    fn edit_save_read_error_keeps_form_open_with_status() {
        let mut a = app();
        seed(&mut a, Task::new("orig".into()));
        a.on_key(key(KeyCode::Char('e'))).unwrap();
        assert_eq!(a.screen(), Screen::Edit);

        // The edit-save path reads the task by id before writing; a read error
        // there must surface a message and leave the form open, not propagate.
        a.store = Box::new(FailingReadStore);
        let res = a.on_key(key(KeyCode::Enter));
        assert!(res.is_ok(), "read error must not propagate out of on_key");
        assert_eq!(a.screen(), Screen::Edit);
        assert!(a.status_message().unwrap().contains("Failed to load task"));
    }

    #[test]
    fn help_toggles_and_dismisses() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('?'))).unwrap();
        assert_eq!(a.screen(), Screen::Help);
        a.on_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(a.screen(), Screen::Today);
    }
}
