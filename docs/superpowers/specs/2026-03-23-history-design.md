# cooee Notification History: Design Spec

**Date:** 2026-03-23
**Status:** Approved

---

## Overview

Add an in-memory notification history to cooee. Every notification received by the daemon is recorded, regardless of DND mode. History is queryable via a new `cooee history` CLI subcommand and is cleared when the daemon stops.

---

## Configuration

New `[history]` section in `config.toml`:

```toml
[history]
max_entries = 50   # 0 = history disabled
```

Default: 50. When at capacity, the oldest entry is evicted.

---

## State

Add to `AppState` (`src/daemon/state.rs`):

```rust
pub history: VecDeque<Notification>,
pub history_max: usize,
```

`history_max` is set from `config.history.max_entries` at startup. On config hot-reload, if `max_entries` decreases below the current history length, excess entries are trimmed from the front (oldest first).

---

## Recording point

History is appended in `dbus.rs::notify` **immediately after the notification ID is assigned**, before the DND filter branch. This ensures:

- DND `full` notifications appear in history even though they are not displayed
- DND `silent` notifications appear in history
- The record reflects what was *received*, not what was *shown*

---

## Socket protocol

### New `Command` variant (`src/daemon/socket.rs`)

```rust
History { count: Option<usize> }
```

- `count = None` — return all history entries
- `count = Some(n)` — return the `n` most recent entries

### New `Response` field

```rust
pub fn ok_history(entries: &[Notification]) -> Self
```

Serialised response:

```json
{
  "ok": true,
  "history": [
    {
      "id": 5,
      "app_name": "Slack",
      "summary": "New message from Alice",
      "body": "Are you free at 3?",
      "urgency": 1,
      "timestamp": "2026-03-23T15:42:03Z"
    }
  ]
}
```

History is returned newest-first.

---

## Timestamp

Add a `received_at: chrono::DateTime<chrono::Utc>` field to `Notification`, set in `dbus.rs::notify` at the moment the notification arrives. Used for display and serialisation.

This adds `chrono` as a dependency (lightweight, widely used).

---

## CLI

### New subcommand (`src/main.rs`, `src/lib.rs`, `src/client.rs`)

```
cooee history [--last N]
```

- `--last N` — limit output to N most recent entries (default: all)
- Output: human-readable, newest first, one entry per line

### Output format

```
[15:42:03] Slack         (normal)  New message from Alice
[15:41:10] Firefox       (low)     Download complete
[15:39:55] System        (urgent)  Disk space low on /home
```

Columns: timestamp (local time), app name, urgency, summary. Body is omitted from the default one-line format to keep output scannable.

---

## Module changes summary

| File | Change |
|---|---|
| `src/config.rs` | Add `HistoryConfig` struct with `max_entries: usize` field; include in `Config` |
| `src/notification.rs` | Add `received_at: DateTime<Utc>` field to `Notification` |
| `src/daemon/state.rs` | Add `history: VecDeque<Notification>` and `history_max: usize` to `AppState`; add `push_history`, `get_history` methods |
| `src/daemon/dbus.rs` | Append to history before DND filter; set `received_at` |
| `src/daemon/socket.rs` | Add `Command::History`, `Response::ok_history` |
| `src/daemon/mod.rs` | Handle `Command::History` in `handle_command` |
| `src/lib.rs` | Add `Command::History { last: Option<usize> }` CLI variant |
| `src/client.rs` | Translate CLI `History` to socket `Command::History`; format and print output |
| `src/main.rs` | Add `history` subcommand with `--last` option |

---

## Dependencies

| Crate | New? | Purpose |
|---|---|---|
| `chrono` | Yes | `DateTime<Utc>` timestamp on `Notification` |

---

## Out of Scope

- Persistent history (survives restart)
- History filtering by app name or urgency
- History in the GTK UI (popup history panel)
- Marking notifications as read/unread
