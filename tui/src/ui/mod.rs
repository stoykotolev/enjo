//! Ratatui rendering for enjo. Pure functions of `&App` — no state of their own,
//! so the whole module can be exercised against a `TestBackend`.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, EditField, Screen};
use crate::model::{Priority, Status, Task};

/// Top-level entry point: draw the active screen for the current frame.
pub fn render(f: &mut Frame, app: &App) {
    match app.screen() {
        Screen::Edit => render_edit(f, app),
        Screen::Help => render_help(f, app),
        Screen::Today | Screen::All => render_list(f, app),
    }
}

fn render_list(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(f.area());

    render_header(f, app, chunks[0]);
    match app.screen() {
        Screen::All => render_all(f, app, chunks[1]),
        _ => render_today(f, app, chunks[1]),
    }
    render_footer(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let label = match app.screen() {
        Screen::All => format!("enjo · All tasks · filter: {}", app.filter().label()),
        _ => "enjo · Today / Next".to_string(),
    };
    let line = Line::from(Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn render_today(f: &mut Frame, app: &App, area: Rect) {
    let sections = app.today_sections();
    let total: usize = sections.iter().map(|s| s.tasks.len()).sum();
    let block = Block::default().borders(Borders::ALL).title("Today / Next");
    // Visible content height inside the bordered block; the Paragraph CLIPS, so
    // we scroll to keep the selected row on screen.
    let inner_height = block.inner(area).height as usize;

    let mut lines: Vec<Line> = Vec::new();
    // Line index of the selected task row, tracked while flattening the sections
    // (headers, "(none)" placeholders and blank spacers are non-selectable).
    let mut selected_line: usize = 0;
    if total == 0 {
        lines.push(Line::from("No tasks — press n to add one").dim());
    } else {
        let mut idx = 0;
        for section in &sections {
            lines.push(Line::from(Span::styled(
                format!("── {} ({}) ──", section.title, section.tasks.len()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            if section.tasks.is_empty() {
                lines.push(Line::from("   (none)").dim());
            }
            for task in &section.tasks {
                let is_selected = idx == app.selected();
                if is_selected {
                    selected_line = lines.len();
                }
                lines.push(today_row(task, is_selected));
                idx += 1;
            }
            lines.push(Line::from(""));
        }
    }

    let offset = scroll_offset(selected_line, lines.len(), inner_height);
    f.render_widget(Paragraph::new(lines).block(block).scroll((offset, 0)), area);
}

/// Compute a vertical scroll offset that keeps `selected_line` within the
/// visible window `[offset, offset + height)` of a `total_lines`-tall content
/// area. Saturating throughout, so it is panic-safe on tiny terminals and empty
/// lists.
fn scroll_offset(selected_line: usize, total_lines: usize, height: usize) -> u16 {
    // Everything fits (or no room to scroll): no offset needed.
    if height == 0 || total_lines <= height {
        return 0;
    }
    let max_offset = total_lines - height;
    // Keep the selection just inside the bottom edge once it scrolls past it.
    let offset = if selected_line < height {
        0
    } else {
        selected_line + 1 - height
    };
    offset.min(max_offset) as u16
}

fn render_all(f: &mut Frame, app: &App, area: Rect) {
    let tasks = app.visible_tasks();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("All tasks [{}]", app.filter().label()));

    if tasks.is_empty() {
        let para = Paragraph::new(Line::from("No tasks — press n to add one").dim()).block(block);
        f.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = tasks
        .iter()
        .map(|t| ListItem::new(Line::from(task_spans(t))))
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected().min(tasks.len() - 1)));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut state);
}

/// A Today row, with an explicit selection marker and reverse-video highlight.
fn today_row(task: &Task, selected: bool) -> Line<'static> {
    let mut spans = vec![Span::raw(if selected { "▶ " } else { "  " })];
    spans.extend(task_spans(task));
    let mut line = Line::from(spans);
    if selected {
        line.style = Style::default().add_modifier(Modifier::REVERSED);
    }
    line
}

/// The styled spans describing a single task row (status, priority, title, due,
/// project), without any selection marker.
fn task_spans(task: &Task) -> Vec<Span<'static>> {
    let check = match task.status {
        Status::Done => "[x]",
        Status::InProgress => "[~]",
        Status::Todo => "[ ]",
    };
    let due = task
        .due_date
        .map(|d| format!("  due:{d}"))
        .unwrap_or_default();
    let project = task
        .project
        .as_deref()
        .map(|p| format!("  #{p}"))
        .unwrap_or_default();

    vec![
        Span::styled(format!("{check} "), status_style(task.status)),
        Span::styled(
            format!("{} ", priority_tag(task.priority)),
            priority_style(task.priority),
        ),
        Span::raw(task.title.clone()),
        Span::styled(due, Style::default().fg(Color::Yellow)),
        Span::styled(project, Style::default().fg(Color::Magenta)),
    ]
}

fn render_edit(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(f.area());
    let es = app.edit_state();
    let title = if es.is_new() { "New task" } else { "Edit task" };

    let mut lines = vec![
        field_line("Title", es.title(), es.field() == EditField::Title, true),
        field_line("Notes", es.notes(), es.field() == EditField::Notes, true),
        field_line(
            "Project",
            es.project(),
            es.field() == EditField::Project,
            true,
        ),
        field_line(
            "Due date",
            es.due_date(),
            es.field() == EditField::DueDate,
            true,
        ),
        field_line(
            "Priority",
            es.priority().as_str(),
            es.field() == EditField::Priority,
            false,
        ),
        field_line(
            "Status",
            es.status().as_str(),
            es.field() == EditField::Status,
            false,
        ),
    ];
    lines.push(Line::from(""));
    lines.push(
        Line::from("Tab/Shift-Tab or ↑/↓ move · ←/→ cycle priority/status · type to edit").dim(),
    );

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let hint = Line::from(" Enter / Ctrl-S save · Esc cancel ").dim();
    f.render_widget(Paragraph::new(hint), chunks[1]);
}

/// Render a single labelled form field. `text_field` shows a fake cursor when
/// focused.
fn field_line<'a>(label: &'a str, value: &'a str, focused: bool, text_field: bool) -> Line<'a> {
    let marker = if focused { "> " } else { "  " };
    let cursor = if focused && text_field { "_" } else { "" };
    let content = format!("{marker}{label:>9}: {value}{cursor}");
    let style = if focused {
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Cyan)
    } else {
        Style::default()
    };
    Line::from(Span::styled(content, style))
}

fn render_help(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(f.area());

    let bindings = [
        ("j / k, ↓ / ↑", "move selection"),
        ("n", "new task"),
        ("e / Enter", "edit selected task"),
        ("Space", "toggle done"),
        ("s / S", "cycle status forward / backward"),
        ("p / P", "cycle priority forward / backward"),
        ("d", "soft-delete selected"),
        ("/", "cycle All-view status filter"),
        ("Tab", "switch Today ↔ All"),
        ("Ctrl-S", "force sync (local-only build; sync in Phase 3)"),
        ("?", "toggle this help"),
        ("q", "quit"),
        ("", ""),
        ("In the edit form:", ""),
        ("Tab / Shift-Tab", "move between fields"),
        ("← / →", "cycle priority / status"),
        ("Enter / Ctrl-S", "save"),
        ("Esc", "cancel"),
    ];

    let mut lines = vec![Line::from(Span::styled(
        "Keybindings",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(""));
    for (keys, desc) in bindings {
        if keys.is_empty() && desc.is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("  {keys:<18}"), Style::default().fg(Color::Yellow)),
                Span::raw(desc.to_string()),
            ]));
        }
    }

    let block = Block::default().borders(Borders::ALL).title("Help");
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let _ = app;
    let hint = Line::from(" ? / Esc / q to dismiss ").dim();
    f.render_widget(Paragraph::new(hint), chunks[1]);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let count = app.visible_tasks().len();
    let view = match app.screen() {
        Screen::All => format!("All [{}]", app.filter().label()),
        _ => "Today".to_string(),
    };
    let mut status = format!(" {view} · {count} tasks · local-only");
    if let Some(msg) = app.status_message() {
        status.push_str(" · ");
        status.push_str(msg);
    }

    let hint = " j/k move · n new · e edit · space done · s status · p prio · d del · / filter · Tab view · S sync · ? help · q quit";

    let lines = vec![
        Line::from(Span::styled(
            status,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(hint).dim(),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn priority_tag(p: Priority) -> &'static str {
    match p {
        Priority::Low => "(L)",
        Priority::Medium => "(M)",
        Priority::High => "(H)",
        Priority::Urgent => "(U)",
    }
}

fn priority_style(p: Priority) -> Style {
    let color = match p {
        Priority::Low => Color::Blue,
        Priority::Medium => Color::Green,
        Priority::High => Color::Yellow,
        Priority::Urgent => Color::Red,
    };
    Style::default().fg(color)
}

fn status_style(s: Status) -> Style {
    let color = match s {
        Status::Todo => Color::White,
        Status::InProgress => Color::Cyan,
        Status::Done => Color::DarkGray,
    };
    Style::default().fg(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::store::SqliteStore;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};

    fn app() -> App {
        App::new(Box::new(SqliteStore::open_in_memory().unwrap())).unwrap()
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE)).unwrap();
    }

    #[test]
    fn renders_every_screen_without_panicking() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut a = app();

        // Today, empty.
        terminal.draw(|f| render(f, &a)).unwrap();

        // Add a task through the edit form, then render Today populated.
        press(&mut a, KeyCode::Char('n'));
        for c in "Render me".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        press(&mut a, KeyCode::Enter);
        terminal.draw(|f| render(f, &a)).unwrap();

        // All view.
        press(&mut a, KeyCode::Tab);
        terminal.draw(|f| render(f, &a)).unwrap();

        // Edit overlay (edit the selected task).
        press(&mut a, KeyCode::Char('e'));
        terminal.draw(|f| render(f, &a)).unwrap();
        press(&mut a, KeyCode::Esc);

        // Help overlay.
        press(&mut a, KeyCode::Char('?'));
        terminal.draw(|f| render(f, &a)).unwrap();
    }

    #[test]
    fn renders_in_a_tiny_terminal() {
        // Guards against panics from over-tight layouts.
        let mut terminal = Terminal::new(TestBackend::new(10, 4)).unwrap();
        let a = app();
        terminal.draw(|f| render(f, &a)).unwrap();
    }

    #[test]
    fn scroll_offset_keeps_selection_visible() {
        // height >= total: no scrolling.
        assert_eq!(scroll_offset(0, 5, 10), 0);
        assert_eq!(scroll_offset(4, 5, 5), 0);
        // Selection near the top, plenty of lines below: no offset.
        assert_eq!(scroll_offset(0, 100, 10), 0);
        assert_eq!(scroll_offset(9, 100, 10), 0);
        // Selection one past the bottom edge: offset advances by one.
        assert_eq!(scroll_offset(10, 100, 10), 1);
        // Selection in the middle keeps it inside the window.
        let off = scroll_offset(50, 100, 10) as usize;
        assert!(off <= 50 && 50 < off + 10, "selection must be in window");
        // Last line: clamped so the window ends exactly at the bottom.
        assert_eq!(scroll_offset(99, 100, 10), 90);
        // Degenerate inputs must not panic and yield 0.
        assert_eq!(scroll_offset(0, 0, 0), 0);
        assert_eq!(scroll_offset(5, 5, 0), 0);
    }

    fn add_task(a: &mut App, title: &str) {
        press(a, KeyCode::Char('n'));
        for c in title.chars() {
            press(a, KeyCode::Char(c));
        }
        press(a, KeyCode::Enter);
    }

    #[test]
    fn renders_tall_today_list_in_short_terminal_without_panic() {
        // Far more tasks than fit, in a short terminal: must not panic, and the
        // selection-driven scroll offset keeps things in bounds.
        let mut a = app();
        for i in 0..50 {
            add_task(&mut a, &format!("task {i}"));
        }
        // Move the cursor well past the visible window.
        for _ in 0..40 {
            press(&mut a, KeyCode::Char('j'));
        }
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal.draw(|f| render(f, &a)).unwrap();
    }
}
