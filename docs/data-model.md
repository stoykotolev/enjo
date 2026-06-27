# Data model

Phase 1 has a single entity: `Task`.

## Task

| Field          | Type                 | Required | Notes                                                        |
| -------------- | -------------------- | -------- | ------------------------------------------------------------ |
| `id`           | TEXT (UUIDv7)        | yes      | Client-generated. Time-ordered.                              |
| `title`        | TEXT                 | yes      | The task summary.                                            |
| `notes`        | TEXT                 | no       | Free-form longer description.                                |
| `status`       | enum                 | yes      | `todo` \| `in_progress` \| `done`                            |
| `priority`     | enum                 | yes      | `low` \| `medium` \| `high` \| `urgent`                      |
| `due_date`     | DATE (`YYYY-MM-DD`)  | no       | Calendar day, no time component.                             |
| `project`      | TEXT                 | no       | Optional free-text grouping.                                 |
| `created_at`   | TEXT (RFC3339 UTC)   | yes      | Set on creation.                                             |
| `updated_at`   | TEXT (RFC3339 UTC)   | yes      | The **LWW clock**. Bumped on every change.                   |
| `completed_at` | TEXT (RFC3339 UTC)   | no       | Set when `status` transitions to `done`.                     |
| `deleted`      | BOOL                 | yes      | Tombstone flag. Soft delete; row retained for sync.          |
| `server_seq`   | INTEGER              | no       | Server-assigned change-feed cursor. `NULL` until synced.     |

## Enums

- **status:** `todo`, `in_progress`, `done`
- **priority:** `low`, `medium`, `high`, `urgent`

## Conventions

- **Identity:** `id` is a UUIDv7 generated on the client. Being time-ordered, it
  doubles as a natural creation ordering key.
- **Timestamps:** all stored as RFC3339 in UTC. Rendered in the user's local
  time zone in the UI.
- **Last-write-wins:** `updated_at` is the conflict-resolution clock. The record
  with the newer `updated_at` wins during sync (Phase 2/3).
- **Tombstones:** deletes are soft. `deleted = true` marks the row so the change
  can propagate during sync; rows are not hard-deleted.
- **Sync cursor:** `server_seq` is `NULL` for records that have never been
  synced. The server assigns a monotonic value on push (Phase 2).
