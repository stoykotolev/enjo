# Sync protocol — Phase 2 DRAFT stub

> **STATUS: DRAFT / Phase 2.** This document describes future work. None of it is
> implemented in Phase 1. Phase 1 is local-only: no backend, no network, no
> sync, no auth. Details below are subject to change.

## Overview

Sync is a single endpoint that performs **push-then-pull** in one round trip:

```
POST /sync
```

1. **Push:** the client sends its locally-changed records (those with a
   `server_seq` of `NULL`, or changed since the last sync).
2. The server applies **last-write-wins** by `updated_at`: for each incoming
   record, the newer `updated_at` wins.
3. The server assigns a **monotonic `server_seq`** to each accepted change.
4. **Pull:** the server returns all records with `server_seq > client.last_pulled_seq`,
   **including tombstones** (`deleted = true`).
5. The client persists returned records and advances its `last_pulled_seq` to the
   highest `server_seq` it received.

## Change-feed cursor

`server_seq` is a server-assigned, monotonically increasing integer. The client
tracks `last_pulled_seq` locally and uses it to request only deltas on the next
sync.

## Conflict resolution

Last-write-wins keyed on `updated_at` (RFC3339 UTC). Ties are not expected given
client clocks plus UUIDv7 identity, but resolution rules will be pinned down
before implementation.

## Auth

Ed25519 per-device request signing. Each device holds a keypair; requests are
signed and verified server-side.

## Hosting

Public deployment on Fly.io with TLS.

## Open questions (to resolve in Phase 2)

- Batch size / pagination for large pulls.
- Clock-skew tolerance for LWW.
- Device registration and key distribution.
- Handling of hard deletes / tombstone garbage collection.
