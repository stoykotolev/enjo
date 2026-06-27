# Architecture

enjo is a local-first TUI task manager. You open it at the start of each day to
answer: what have I started, what's coming up, what's next.

## Components

The system is made of three pieces. Only the first ships in Phase 1.

### enjo (Rust + Ratatui client) — Phase 1

The interactive terminal client. Built with [Ratatui](https://ratatui.rs/).
A local SQLite database is the **source of truth for the UI**. The client is
fully offline: no backend, no network, no sync, and no auth in Phase 1.

- Persistence: SQLite via `rusqlite` (bundled).
- IDs: UUIDv7, client-generated.
- Timestamps: stored as RFC3339 UTC, rendered in the user's local time zone.
- Config: stored under the platform config dir (`directories`), TOML format.

### enjod (Go backend) — Phase 2

A Go service built on `net/http` with a pure-Go SQLite backend. It accepts sync
requests, applies last-write-wins, and assigns a monotonic change-feed cursor.
Not implemented in Phase 1 — see `cmd/enjod/main.go` for the placeholder.

### Sync protocol — Phase 2/3

A last-write-wins sync protocol with a server-assigned change-feed cursor.
See `sync-protocol.md` (DRAFT). Deferred.

## Data flow (Phase 1)

```
┌────────────────────┐
│   enjo (Ratatui)   │
│  ┌──────────────┐  │
│  │   UI state   │  │
│  └──────┬───────┘  │
│         │ read/write
│  ┌──────▼───────┐  │
│  │ local SQLite │  │  ← source of truth
│  └──────────────┘  │
└────────────────────┘
```

All reads and writes hit local SQLite directly. There is no network path in
Phase 1.

## Phase boundaries

- **Phase 1:** local-only TUI. No backend, no network, no sync, no auth.
- **Phase 2:** `enjod` backend + `POST /sync`.
- **Phase 3:** full sync rollout, hosted on Fly.io with TLS.
