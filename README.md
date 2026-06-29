# enjo

A local-first TUI task manager you open at the start of each day to answer three
questions: what have I started, what's coming up, and what's next.

enjo is fully offline in Phase 1. Your local SQLite database is the source of
truth for the UI — no backend, no network, no auth required.

## Repo structure

```
enjo/
├── README.md
├── docs/
│   ├── architecture.md       # system overview
│   ├── data-model.md         # the Task model
│   └── sync-protocol.md      # Phase 2 DRAFT stub
├── tui/                      # enjo — Rust + Ratatui client (crate: enjo)
│   ├── Cargo.toml
│   ├── migrations/0001_init.sql
│   └── src/
│       ├── main.rs           # terminal + event loop
│       ├── app.rs            # App state + key handling
│       ├── ui/mod.rs         # Today / All / Edit / Help rendering
│       ├── store/mod.rs      # Store trait + SqliteStore
│       ├── model.rs          # Task, Status, Priority
│       └── config.rs         # data dir + DB path resolution
└── server/                   # enjod — Go backend (Phase 2 placeholder)
    ├── go.mod
    └── cmd/enjod/main.go
```

## Components

- **enjo** — Rust + Ratatui client. Local SQLite is the source of truth for the
  UI. Fully offline.
- **enjod** — Go `net/http` + pure-Go SQLite backend. Phase 2.
- **sync** — last-write-wins protocol with a server-assigned change-feed cursor.
  Phase 2/3.

## How to run

```sh
cd tui && cargo run
```

The database lives at the per-user data dir (`~/Library/Application Support/enjo/enjo.db`
on macOS). Set `ENJO_DATA_DIR` to override the location.

## Keybindings

The app opens on the **Today** view (in-progress → overdue/due-today → next up).

| key | action |
|---|---|
| `j` / `k` (or ↓ / ↑) | move selection |
| `n` | new task |
| `e` / `Enter` | edit selected task |
| `Space` | toggle done |
| `s` | cycle status (todo → in_progress → done) |
| `p` | cycle priority (low → medium → high → urgent) |
| `d` | delete (soft) selected task |
| `Tab` | switch Today ↔ All tasks |
| `/` | cycle the status filter (All view) |
| `S` | force sync (local-only build; arrives in Phase 3) |
| `?` | help |
| `q` | quit |

In the edit form: `Tab` / `Shift-Tab` move between fields, type to edit text
fields, `s` / `p` cycle status / priority, `Enter` or `Ctrl-S` saves, `Esc`
cancels.

## Status bar integration

`enjo status` is a headless command that prints the current in-progress task on
one line — the same task that heads the "In progress" section in the TUI — or
`idle` when none is. (enjo enforces a single in-progress task at a time.) It
reads the same local database the app writes, so it stays in sync.

```sh
enjo status                  # e.g. "Refactor sync engine"
enjo status --max-len 40     # truncate the title (default 40)
enjo status --tmux           # escape '#' as '##' so titles are tmux-safe
```

### tmux

Install the binary and point tmux at it:

```sh
cargo install --path tui     # installs `enjo` to ~/.cargo/bin
```

Append the segment to your status line and set a refresh interval. Using
`set -ga` adds to whatever your theme already defines, so the task lands at the
end of that side — e.g. on the right, after the rest of `status-right`:

```tmux
set -g status-interval 1
set -ga status-right "#[bold] ▶ #(enjo status --tmux --max-len 40) "
```

(Use `set -ga status-left` instead to place it on the left.)

tmux runs `#()` commands asynchronously and refreshes them every
`status-interval` seconds, so the displayed task updates shortly after you
change it in the app.

## Phase status

- **Phase 1 — local-only TUI:** ✅ complete. Full task CRUD, Today/Next view,
  All-tasks view with filtering, edit form, keybindings, help — all backed by a
  relational local SQLite DB. No backend, no network, no sync, no auth.
- **Phase 2 — Go backend (`enjod`):** deferred (placeholder only).
- **Phase 3 — sync:** deferred.

Full plan: Inkdrop note inkdrop://note/ZTrjGL-_
