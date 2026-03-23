# cooee Polish — Code Quality + Missing Features: Design Spec

**Date:** 2026-03-23
**Status:** Approved

---

## Overview

This spec covers two related areas:

1. **Code quality** — decompose `build_notification_window` and collapse serde default boilerplate
2. **Missing features** — implement `image-data`/`image-path` hint rendering, `replaces_id` in-place replacement, and config hot-reload

These are grouped together because they are all about making existing behaviour correct and the code easier to work in, rather than adding new capabilities.

---

## Section 1: Code Quality

### 1.1 Decompose `build_notification_window` (`src/daemon/ui.rs`)

**Problem:** `build_notification_window` is ~95 lines handling five distinct concerns in one function, making it hard to read, test, and extend (particularly when adding image rendering in section 2).

**Solution:** Extract five focused private functions; `build_notification_window` becomes a ~15-line orchestrator.

| New function | Responsibility |
|---|---|
| `setup_layer_shell(win, config)` | Layer-shell init, anchor/margin, monitor selection |
| `build_header(notification, config, win_ref) -> GtkBox` | App icon, summary label, dismiss button |
| `build_body(notification) -> Option<Label>` | Pango markup body label; `None` when body is empty |
| `build_actions(notification, action_tx) -> Option<GtkBox>` | Action button row; `None` when actions list is empty |
| `setup_auto_dismiss(win, notification, config)` | Auto-dismiss timeout timer |

No behaviour changes — pure restructuring.

### 1.2 Collapse serde default helpers (`src/config.rs`)

**Problem:** Each default config value requires its own named function (serde restriction). There are currently 9 such `fn default_*()` helpers producing significant repetition.

**Solution:** Introduce a `default_val!` macro:

```rust
macro_rules! default_val {
    ($name:ident, $ty:ty, $val:expr) => {
        fn $name() -> $ty { $val }
    }
}
```

All existing `default_*` functions are replaced with `default_val!` invocations. Behaviour is identical.

---

## Section 2: Missing Features

### 2.1 Image hint rendering

**Problem:** `dbus.rs::notify` has `// TODO: parse "image-data" hint`. `Notification.image_data` is always `None`. The `"body-images"` capability is not advertised.

**Solution:**

#### 2.1.1 `ImageData` type (`src/notification.rs`)

Replace `image_data: Option<Vec<u8>>` with:

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

`Notification.image_data` becomes `Option<ImageData>`.

#### 2.1.2 Hint parsing (`src/daemon/dbus.rs`)

Parse hints in freedesktop priority order:

1. `image-data` hint — zvariant struct `(iiibiiay)` → `ImageData`
2. `image-path` hint — string path or icon name → stored as `Option<String>` in `Notification` (new field: `image_path`)
3. `app_icon` parameter — already handled

`get_capabilities()` adds `"body-images"` to its return list.

#### 2.1.3 Rendering (`src/daemon/ui.rs`)

Inside `build_notification_window` (after the restructuring in 1.1), insert an image widget below the header row when image data is present:

- `image_data` → `gdk4::MemoryTexture::new(width, height, format, bytes, stride)` → `gtk4::Image::from_paintable()`. No new dependency — `gdk4` is a transitive dep of `gtk4`.
- `image_path` (when `image_data` is absent) → `gtk4::Image::from_file(path)` or `gtk4::Image::from_icon_name(name)`.

Image widget gets CSS class `notification-image` for styling.

---

### 2.2 `replaces_id` in-place replacement

**Problem:** When `replaces_id > 0`, the ID is reused but the existing window is never closed, so two popup windows appear for what should be one notification.

**Solution:**

1. Add `replaces_id: u32` field to `Notification` (0 = no replacement).
2. In `NotificationManager::show()`: if `notification.replaces_id > 0`, call `self.close(notification.replaces_id)` before showing the new window. `NotificationManager::close()` only destroys the GTK window — it does not emit D-Bus signals (those come from `dbus.rs::close_notification`, not the UI layer), so no spurious `NotificationClosed` signal is emitted.
3. The new window appears at the correct stack position (inserted fresh, same as any new notification).

---

### 2.3 Config hot-reload

**Problem:** The design spec requires the daemon to reload config on file change without restart. This is not implemented.

**Solution:**

#### 2.3.1 `SharedConfig` type

Introduce:
```rust
pub type SharedConfig = Arc<RwLock<Config>>;
```

All components that currently hold `Arc<Config>` are updated to hold `SharedConfig`. Config values are accessed via `config.read().unwrap()`.

#### 2.3.2 File watcher

Add the `notify` crate as a dependency. In the tokio runtime (inside the background thread in `daemon/mod.rs::run`), start a watcher task that monitors the config file path. On a change event:

1. Call `Config::load()`.
2. On success: write-lock `SharedConfig` and replace the inner value.
3. On error: log to stderr, keep the existing config.
4. Send `UiEvent::ReloadCss` to the GTK thread.

#### 2.3.3 CSS reload

Add `UiEvent::ReloadCss` variant. In the GTK event loop (`daemon/mod.rs::run`), handle it by calling `manager.init_css()` to re-apply the stylesheet from the current config.

#### 2.3.4 What reloads

| Config field | Reload behaviour |
|---|---|
| All `[general]` fields | Effective on next notification |
| `[sound]` | Effective on next notification |
| `[tts]` | Effective on next notification |
| `[general]` font/colour (CSS) | Immediate via `ReloadCss` event |
| `[dnd].mode` | Ignored on reload — runtime state in `AppState` |
| `[history].max_entries` | Effective immediately (excess history trimmed) |

---

## Dependencies

| Crate | New? | Purpose |
|---|---|---|
| `notify` | Yes | File-system watcher for config hot-reload |
| `gdk4` | No (transitive) | `MemoryTexture` for image-data rendering |

---

## Out of Scope

- Animated images (GIF)
- `icon_data` hint (deprecated alias for `image-data`) — can be added later
- Per-urgency CSS classes (separate feature)
