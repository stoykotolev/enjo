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
│   └── src/main.rs
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

## Phase status

- **Phase 1 — local-only TUI:** in progress. No backend, no network, no sync,
  no auth.
- **Phase 2 — Go backend (`enjod`):** deferred (placeholder only).
- **Phase 3 — sync:** deferred.

Full plan: Inkdrop note inkdrop://note/ZTrjGL-_
