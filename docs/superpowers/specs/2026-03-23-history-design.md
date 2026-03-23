# cooee Notification History: Design Spec

**Date:** 2026-03-23
**Status:** Approved

---

## Overview

Add an in-memory notification history to cooee. Every notification received is recorded regardless of DND mode. History is queryable via a new `cooee history` CLI subcommand and is cleared when the daemon stops.

---

## Configuration

Add `HistoryConfig` to `src/config.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "HistoryConfig::default_max_entries")]
    pub max_entries: usize,
}
impl HistoryConfig {
    fn default_max_entries() -> usize { 50 }
}
impl Default for HistoryConfig {
    fn default() -> Self { Self { max_entries: 50 } }
}
```

New TOML section:

```toml
[history]
max_entries = 50   # 0 = history disabled
```

Add `pub history: HistoryConfig` to the `Config` struct.

---

## Recording policy

History records notifications after a full `Notification` struct is constructed — which happens **after the DND `full` early-return**. Concretely:

- **DND `full`**: the handler assigns an ID and returns early before building a `Notification`. These are **not** recorded in history (no struct to record).
- **DND `silent`** and **DND `off`**: a full `Notification` is built; it is recorded before the DND branch that decides whether to show it.

This means history reflects notifications that were *constructed*, not those that were *shown*. A `silent` DND notification appears in history even though it was not spoken or played. A `full` DND notification does not appear in history.

### `replaces_id` deduplication

When `replaces_id > 0`: if an entry with `id == replaces_id` exists in the history deque, **replace it in-place** (swap content, preserve position). If no such entry exists, append as normal. This prevents progress-style notifications from flooding history.

---

## State (`src/daemon/state.rs`)

Add to `AppState`:

```rust
pub history: VecDeque<Notification>,
pub history_max: usize,
```

`history_max` is initialised from `config.history.max_entries` in `AppState::new`. On config hot-reload (§2.3 of polish spec), the watcher task locks `SharedState` and updates `history_max`; if it decreased below `history.len()`, drain entries from the front until `history.len() == history_max`.

Add helper methods:

```rust
/// Append or replace; trims to max_entries. No-op if history_max == 0.
pub fn push_history(&mut self, n: Notification) {
    if self.history_max == 0 { return; }
    if n.replaces_id > 0 {
        if let Some(pos) = self.history.iter().position(|e| e.id == n.replaces_id) {
            self.history[pos] = n;
            return;
        }
    }
    if self.history.len() >= self.history_max {
        self.history.pop_front();
    }
    self.history.push_back(n);
}

/// Returns up to `count` most recent entries, newest first.
pub fn get_history(&self, count: Option<usize>) -> Vec<&Notification> {
    let iter = self.history.iter().rev();
    match count {
        Some(n) => iter.take(n).collect(),
        None => iter.collect(),
    }
}
```

---

## Timestamp

Add `received_at: chrono::DateTime<chrono::Local>` to `Notification`. Set to `chrono::Local::now()` in `dbus.rs::notify` at the point of reception. `Local` is used (not `Utc`) so display requires no conversion.

Add to `Cargo.toml`:

```toml
chrono = { version = "0", features = ["serde"] }
```

The `serde` feature serialises `DateTime<Local>` to ISO 8601 strings in JSON responses.

---

## Serde and `Display` on `Notification`

### Serde derives

`Notification` currently derives `Debug, Clone`. Add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification { ... }
```

Also add `Serialize, Deserialize` derives to `Action`.

Exclude `image_data` from JSON (can be large; not useful in history output):

```rust
#[serde(skip)]
pub image_data: Option<ImageData>,
```

### `Urgency::Display`

`Notification` stores `urgency: u8`. `Urgency` is a separate enum with `From<u8>`. The CLI formatter converts: `Urgency::from(notification.urgency)`. Add `Display` to `Urgency`:

```rust
impl fmt::Display for Urgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Urgency::Low      => write!(f, "low"),
            Urgency::Normal   => write!(f, "normal"),
            Urgency::Critical => write!(f, "critical"),
        }
    }
}
```

`Urgency` does not need serde derives. The `u8` field serialises correctly as-is.

---

## Socket protocol

### `Command` (`src/daemon/socket.rs`)

Add variant to the existing enum (which uses `#[serde(tag = "cmd", rename_all = "snake_case")]`):

```rust
History { count: Option<usize> },
```

Wire format:

```json
{"cmd": "history"}
{"cmd": "history", "count": 10}
```

### `Response` (`src/daemon/socket.rs`)

Add `#[derive(Default)]` to `Response`. Add field:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub history: Option<Vec<Notification>>,
```

Add constructor:

```rust
pub fn ok_history(entries: Vec<Notification>) -> Self {
    Self { ok: true, history: Some(entries), ..Default::default() }
}
```

Example response (note: `received_at` is included in serialised `Notification` and is intentional — it is displayed in CLI output):

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
      "expire_timeout": 5000,
      "received_at": "2026-03-23T15:42:03+11:00"
    }
  ]
}
```

---

## `handle_command` (`src/daemon/mod.rs`)

```rust
Command::History { count } => {
    let entries = state.lock().unwrap()
        .get_history(count)
        .into_iter().cloned().collect();
    Response::ok_history(entries)
}
```

---

## CLI

### New subcommand

In `src/lib.rs`, add to the `Command` enum:

```rust
/// Print notification history
History {
    #[arg(long, value_name = "N", help = "Show only the N most recent entries")]
    last: Option<usize>,
}
```

In `src/client.rs`, translate and print:

```rust
Command::History { last } => {
    let resp = send_command(SocketCommand::History { count: last })?;
    for n in resp.history.unwrap_or_default() {
        let time = n.received_at.format("%H:%M:%S");
        let urgency = Urgency::from(n.urgency);
        println!("[{time}] {:<14} ({urgency:<8}) {}", n.app_name, n.summary);
    }
}
```

### Output format

```
[15:42:03] Slack         (normal)   New message from Alice
[15:41:10] Firefox       (low)      Download complete
[15:39:55] System        (critical) Disk space low on /home
```

Newest first. Body omitted. Empty history prints nothing.

---

## Recording point in `dbus.rs::notify`

```
Notify() received
    │
    ├─ Assign ID (replaces_id > 0 → reuse existing, else next_id)
    │
    ├─ DND = full   → emit NotificationClosed, return id   ← NOT recorded
    │
    ├─ Build Notification (received_at = Local::now(), replaces_id stored)
    │
    ├─ state.push_history(notification.clone())   ← recorded here (silent + off only)
    │
    ├─ DND = silent → send UiEvent::ShowNotification
    │
    └─ DND = off    → play sound, speak, send UiEvent::ShowNotification
```

---

## Module changes summary

| File | Change |
|---|---|
| `src/config.rs` | Add `HistoryConfig`; add `pub history: HistoryConfig` to `Config` |
| `src/notification.rs` | Add `received_at: DateTime<Local>`, `replaces_id: u32` fields; add `Serialize`/`Deserialize` to `Notification` and `Action`; `#[serde(skip)]` on `image_data`; add `Display` to `Urgency` |
| `src/daemon/state.rs` | Add `history: VecDeque<Notification>`, `history_max: usize` to `AppState`; add `push_history`, `get_history` |
| `src/daemon/dbus.rs` | Set `received_at`, call `push_history` before DND filter |
| `src/daemon/socket.rs` | Add `Command::History`; add `#[derive(Default)]` and `history` field to `Response`; add `ok_history` |
| `src/daemon/mod.rs` | Handle `Command::History` in `handle_command` |
| `src/lib.rs` | Add `Command::History { last: Option<usize> }` |
| `src/client.rs` | Translate and print history |

---

## Dependencies

| Crate | New | Version |
|---|---|---|
| `chrono` | Yes | `0` (latest 0.x, serde feature) |

---

## Out of Scope

- Persistent history (survives restart)
- Filtering by app name or urgency
- History in the GTK UI
- Read/unread marking
