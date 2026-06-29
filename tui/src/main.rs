mod app;
mod config;
mod model;
mod status;
mod store;
mod ui;

use std::time::Duration;

use anyhow::{bail, Context, Result};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::config::Config;
use crate::store::SqliteStore;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => run_tui(),
        Some("status") => run_status(&args[1..]),
        Some("-h") | Some("--help") => {
            print_help();
            Ok(())
        }
        Some("-V") | Some("--version") => {
            println!("enjo {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => {
            eprintln!("enjo: unknown command '{other}'\n");
            print_help();
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "enjo {} — a local-first TUI task manager\n\
         \n\
         USAGE:\n    \
         enjo                 Launch the interactive TUI (default)\n    \
         enjo status [opts]   Print the current in-progress task (for status bars)\n    \
         enjo --help          Show this help\n    \
         enjo --version       Show the version\n\
         \n\
         `status` options:\n    \
         --tmux               Escape '#' as '##' so titles are tmux-safe\n    \
         --max-len <N>        Truncate the title to N characters (default {})\n",
        env!("CARGO_PKG_VERSION"),
        status::DEFAULT_MAX_LEN,
    );
}

/// Headless one-line summary of the current in-progress task, for embedding in
/// a status bar (e.g. tmux `status-left`/`status-right`). Reads the same local
/// database the TUI uses; prints `idle` when nothing is in progress.
fn run_status(args: &[String]) -> Result<()> {
    let mut tmux = false;
    let mut max_len = status::DEFAULT_MAX_LEN;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tmux" => tmux = true,
            "--max-len" => {
                i += 1;
                let value = args
                    .get(i)
                    .context("--max-len requires a value, e.g. --max-len 40")?;
                max_len = value.parse::<usize>().with_context(|| {
                    format!("--max-len must be a non-negative integer, got '{value}'")
                })?;
            }
            other => bail!("unknown flag for 'enjo status': {other}"),
        }
        i += 1;
    }

    let config = Config::load()?;
    let store = SqliteStore::open(&config.db_path())?;
    let app = App::new(Box::new(store))?;

    let mut line = status::format_status(&app.in_progress_tasks(), max_len);
    if tmux {
        line = status::escape_tmux(&line);
    }
    println!("{line}");
    Ok(())
}

fn run_tui() -> Result<()> {
    let config = Config::load()?;
    let store = SqliteStore::open(&config.db_path())?;
    let mut app = App::new(Box::new(store))?;

    // `ratatui::init` enters the alternate screen and installs a panic hook that
    // restores the terminal, so a panic never leaves the user's terminal wedged.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    // Always restore on the normal exit path too, before propagating any error.
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.should_quit() {
        terminal.draw(|f| ui::render(f, app))?;

        // Poll with a timeout so the loop stays responsive and can later service
        // background (sync) messages; for now it just bounds redraw latency.
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key)?;
                }
            }
        }
    }
    Ok(())
}
