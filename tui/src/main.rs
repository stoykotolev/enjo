mod app;
mod config;
mod model;
mod store;
mod ui;

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::config::Config;
use crate::store::SqliteStore;

fn main() -> Result<()> {
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
