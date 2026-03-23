# cooee Polish — Code Quality + Missing Features: Design Spec

**Date:** 2026-03-23
**Status:** Approved

---

## Overview

1. **Code quality** — decompose `build_notification_window`; collapse serde default boilerplate
2. **Missing features** — `image-data`/`image-path` hint rendering; `replaces_id` in-place replacement; config hot-reload

---

## Section 1: Code Quality

### 1.1 Decompose `build_notification_window` (`src/daemon/ui.rs`)

Extract five private free functions. `build_notification_window` becomes a ~15-line orchestrator calling them in order.

| Function | Signature | Responsibility |
|---|---|---|
| `setup_layer_shell` | `(win: &ApplicationWindow, config: &Config)` | `init_layer_shell`, `set_layer(Overlay)`, `set_namespace("cooee")`, monitor selection. Does **not** set anchors or margins — those remain in `NotificationManager::position_window`. |
| `build_header` | `(notification: &Notification, config: &Config, win: ApplicationWindow) -> GtkBox` | App icon (from `app_icon`), summary label, dismiss button. `win` is passed as a GTK ref-counted clone (cheap); the dismiss button closure captures `win.clone()` and calls `win.destroy()`. |
| `build_body` | `(notification: &Notification) -> Option<Label>` | Pango markup label. Returns `None` when `body` is empty. |
| `build_image` | `(notification: &Notification) -> Option<gtk4::Image>` | Notification content image from hints. Returns `None` when neither `image_data` nor `image_path` is set. See §2.1. |
| `build_actions` | `(notification: &Notification, action_tx: mpsc::UnboundedSender<(u32, String)>) -> Option<GtkBox>` | Action button row. Returns `None` when `actions` is empty. |
| `setup_auto_dismiss` | `(win: &ApplicationWindow, notification: &Notification, config: &Config)` | Registers `glib::timeout_add_local_once` if `display_duration_ms` returns `Some`. Captures `win.clone()` (GTK ref-count clone) — a borrow is not `'static` and cannot be captured by the required `'static` closure. |

Layout order in `build_notification_window`:
1. `setup_layer_shell`
2. `build_header` → append to vbox
3. `build_image` → append to vbox (when `Some`)
4. `build_body` → append to vbox (when `Some`)
5. `build_actions` → append to vbox (when `Some`)
6. `setup_auto_dismiss`

No behaviour changes — pure restructuring.

### 1.2 Collapse serde default helpers (`src/config.rs`)

Introduce a `default_val!` macro to replace the 15 boilerplate `fn default_*()` helper functions:

```rust
macro_rules! default_val {
    ($name:ident, $ty:ty, $val:expr) => {
        fn $name() -> $ty { $val }
    }
}
```

All 15 existing helpers become one-line macro invocations. Behaviour is identical.

---

## Section 2: Missing Features

### 2.1 Image hint rendering

**Problem:** `Notification.image_data` is always `None`. The `image-data` and `image-path` D-Bus hints are not parsed.

**Clarification:** Two separate visual elements exist:
- **App icon** (`app_icon` parameter) — small branding icon in the header row. Already implemented. Unchanged.
- **Notification image** (`image-data` / `image-path` hints) — large content image in its own row. This is what this section adds.

#### `ImageData` type (`src/notification.rs`)

Replace `image_data: Option<Vec<u8>>` with `Option<ImageData>`. Add `image_path: Option<String>`.

```rust
pub struct ImageData {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub n_channels: i32,
    pub data: Vec<u8>,
}
```

Update `Notification::new()`:

```rust
pub fn new(
    id: u32, app_name: String, app_icon: String, summary: String, body: String,
    actions: Vec<String>, urgency: u8, expire_timeout: i32,
    image_data: Option<ImageData>, image_path: Option<String>,
    replaces_id: u32,   // §2.2
) -> Self
```

Update the single call site in `dbus.rs::notify` and all test-helper constructors in `notification.rs` (pass `None, None, 0`).

#### Hint parsing (`src/daemon/dbus.rs`)

Parse in priority order per the freedesktop spec. `image-data` takes precedence over `image-path`.

1. `image-data` hint: zvariant type `(iiibiiay)`. Destructure into the seven fields of `ImageData`.
2. `image-path` hint: `String`. Stored in `image_path`. Decision rule for rendering (see below).
3. If neither hint is present: `image_data = None`, `image_path = None`.

Do **not** add `"body-images"` to `get_capabilities()`. Per the freedesktop spec, `"body-images"` refers to inline `<img>` tags in body markup — not to the `image-data` hint. The `image-data` hint is supported unconditionally and requires no capability advertisement.

#### Rendering (`src/daemon/ui.rs`) — `build_image`

```
if image_data is Some(d):
    format = if d.has_alpha { MemoryFormat::R8g8b8a8 } else { MemoryFormat::R8g8b8 }
    texture = gtk4::gdk::MemoryTexture::new(
        d.width, d.height, format,
        &glib::Bytes::from(&d.data),
        d.rowstride as usize,
    )
    return Some(gtk4::Image::from_paintable(Some(&texture)))

else if image_path is Some(p):
    if p starts with '/' or "file://" → return Some(gtk4::Image::from_file(expand_path(p)))
    else → return Some(gtk4::Image::from_icon_name(p))
        # image-path that is not a file path is an icon theme name per the freedesktop spec

else:
    return None
```

No new dependency: `gtk4::gdk` and `glib` are available via the existing `gtk4` crate.

Image widget gets CSS class `notification-image`. Max pixel size: `config.general.icon_size * 4` (capped, not stretched).

---

### 2.2 `replaces_id` in-place replacement

**Current state:** `dbus.rs::notify` already reuses the ID (`let id = if replaces_id > 0 { replaces_id } else { state.next_notification_id() }`). The bug is that the existing popup is never closed — two windows appear.

**Fix:**

Add `replaces_id: u32` to `Notification` (0 = not a replacement). Set from the D-Bus parameter in `dbus.rs::notify`. See updated `Notification::new()` signature in §2.1.

In `NotificationManager::show()`, add before building the new window:

```rust
if notification.replaces_id > 0 {
    self.close(notification.replaces_id);
}
```

`NotificationManager::close()` only destroys the GTK window and removes it from `self.windows`. It does not touch `AppState` — `last_notification` lives in `AppState` (managed by `dbus.rs`), not in `NotificationManager`. No D-Bus `NotificationClosed` signal is emitted.

---

### 2.3 Config hot-reload

#### `SharedConfig` type

Add to `src/config.rs`:

```rust
pub type SharedConfig = Arc<RwLock<Config>>;
```

All components change from `Arc<Config>` to `SharedConfig`. The locking rule is: **acquire read lock, read the needed value(s), release the lock, then do all other work**. Never hold the read lock across an `await` point, a GTK callback, or a blocking call.

For constructors that take owned sub-configs (`SoundPlayer::new(SoundConfig)`, `TtsClient::new(TtsConfig)`):

```rust
let sound_config = config.read().unwrap().sound.clone();
let sound = SoundPlayer::new(sound_config);
```

For the GTK event loop, which currently accesses config in `build_notification_window`: the `SharedConfig` is passed to `NotificationManager`. At the start of `NotificationManager::show()`, snapshot the needed config fields before calling GTK functions.

Existing windows on screen at the time of a hot-reload continue to use their creation-time config (CSS and layout). Only new windows and new sound/TTS calls pick up the updated config. This is acceptable behaviour.

#### File watcher

Add to `[dependencies]` in `Cargo.toml` (this crate is not currently present):

```toml
notify = "6"
```

In `daemon/mod.rs::run`, after starting D-Bus and the socket server, spawn a tokio task:

```rust
tokio::spawn(async move {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).expect("watcher");
    watcher.watch(&Config::config_path(), notify::RecursiveMode::NonRecursive).ok();
    loop {
        match rx.recv() {
            Ok(Ok(event)) if matches!(event.kind, notify::EventKind::Modify(_)) => {
                match Config::load() {
                    Ok(new_cfg) => {
                        *shared_config.write().unwrap() = new_cfg;
                        event_queue.lock().unwrap().push_back(UiEvent::ReloadCss);
                    }
                    Err(e) => eprintln!("cooee: config reload failed: {e}"),
                }
            }
            _ => {}
        }
    }
});
```

The watcher task also holds a clone of `SharedState` and updates `history_max` on reload (see history spec §State).

#### `UiEvent::ReloadCss`

Add variant to `UiEvent` enum in `src/daemon/ui.rs`:

```rust
pub enum UiEvent {
    ShowNotification(Notification),
    CloseNotification(u32),
    DismissLatest,
    ReloadCss,         // ← new
    Shutdown,
}
```

Add match arm in the GTK event loop in `daemon/mod.rs`:

```rust
UiEvent::ReloadCss => mgr.init_css(),
```

#### What reloads

| Config section | Behaviour |
|---|---|
| Font, colours (CSS) | Immediate — `ReloadCss` triggers `init_css()` |
| Layout (width, max_visible, timeout, etc.) | Next notification onwards |
| `[sound]`, `[tts]`, `[actions]` | Next event onwards |
| `[dnd].mode` | Ignored — runtime state in `AppState` |
| `[history].max_entries` | Immediate — watcher task updates `AppState.history_max` and trims |

---

## Dependencies

| Crate | New | Version |
|---|---|---|
| `notify` | Yes | `6` |

---

## Out of Scope

- Animated images (GIF)
- `icon_data` hint (deprecated alias for `image-data`)
- Per-urgency CSS classes
- Updating in-flight notification windows on hot-reload
