# cooee — Wayland Desktop Notification Daemon: Design Spec

**Date:** 2026-03-22
**Repository:** https://github.com/theophilusx/cooee
**License:** GPLv3
**Status:** Draft

---

## Overview

cooee is a Wayland desktop notification daemon for Hyprland-based desktops (primary target: Fedora 43, designed to work on any modern Linux system running a recent Hyprland). It implements the [freedesktop.org Desktop Notifications Specification v1.2](https://specifications.freedesktop.org/notification-spec/notification-spec-latest.html), providing:

- **Visual notifications** — card-style GTK4 popup windows rendered as Wayland overlays
- **Audio alerts** — plays a configurable sound file on each notification
- **TTS** — speaks the notification summary via speech-dispatcher on arrival; full body on demand
- **Do Not Disturb** — three levels: full (discard), silent (visual only), off (full)
- **Systemd integration** — starts as a user service at graphical session login
- **CLI control** — `cooee speak`, `cooee dnd`, `cooee dismiss`, `cooee action` for keybind integration
- **Notification actions** — actions provided by the sending app (e.g. "Open", "Snooze") selectable via a configurable external picker (rofi by default)

---

## Architecture

cooee is a single Rust binary with two runtime modes: **daemon** and **client**.

### Daemon (`cooee daemon`)

A long-running process that:

1. Registers `org.freedesktop.Notifications` on the D-Bus session bus
2. Receives notifications from any application via D-Bus
3. Applies the DND filter to decide what action to take
4. Renders GTK4 card popups via `gtk4-layer-shell` on the currently focused monitor
5. Plays a sound via `rodio`
6. Speaks the notification summary via `speech-dispatcher`
7. Listens on a Unix socket at `$XDG_RUNTIME_DIR/cooee.sock` for CLI commands

The GTK4 main loop runs on the main thread. D-Bus and Unix socket handling run on a `tokio` async runtime. A `glib::MainContext` channel bridges async events into the GTK main loop. Shared state (`DND mode`, `last notification`) is held in `Arc<Mutex<AppState>>`.

### Client (`cooee <subcommand>`)

Connects to `$XDG_RUNTIME_DIR/cooee.sock` and sends a command, printing the response to stdout. Returns a non-zero exit code if the daemon is not running.

---

## Module Structure

```
cooee/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point (clap), dispatches to daemon or client
│   ├── config.rs            # TOML config types + loader
│   ├── notification.rs      # Notification struct, urgency levels, XDG icon lookup
│   ├── daemon/
│   │   ├── mod.rs           # Daemon startup, wires all components together
│   │   ├── dbus.rs          # zbus server — org.freedesktop.Notifications
│   │   ├── socket.rs        # Unix socket server, handles CLI control commands
│   │   ├── state.rs         # AppState (DND mode, last notification)
│   │   ├── sound.rs         # rodio playback
│   │   ├── tts.rs           # speech-dispatcher client
│   │   └── ui.rs            # GTK4 window management + gtk4-layer-shell positioning
│   └── client.rs            # Connects to socket, sends command, prints response
├── data/
│   ├── cooee.service        # systemd user service file
│   └── sounds/
│       └── notify.ogg       # bundled default notification sound
└── docs/
    └── superpowers/
        └── specs/
            └── 2026-03-22-cooee-design.md
```

---

## Key Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `gtk4` | latest | Notification window rendering |
| `gtk4-layer-shell` | latest | Wayland overlay positioning via layer-shell protocol |
| `zbus` | latest | D-Bus server (async, pure Rust) |
| `tokio` | latest | Async runtime for D-Bus and socket handling |
| `rodio` | latest | Sound file playback (.ogg, .wav, .mp3) |
| `speech-dispatcher` | latest | Rust bindings to libspeechd for TTS |
| `clap` | latest | CLI argument parsing |
| `serde` + `toml` | latest | Config file parsing |

System requirements: `libgtk-4`, `libgtk4-layer-shell`, `libspeechd` (speech-dispatcher), all available as Fedora packages.

---

## Configuration

Config file location: `$XDG_CONFIG_HOME/cooee/config.toml` (defaults to `~/.config/cooee/config.toml`).

The daemon writes a default config on first run if none exists. The daemon watches the config file via inotify and reloads without restart when it changes. All keys are live-reloadable except `[dnd].mode`, which is runtime state managed via the socket and is not overwritten by a config reload.

```toml
[general]
position = "top-right"   # top-right | top-left | bottom-right | bottom-left
                         # center | center-top | center-bottom
margin_x = 16            # horizontal gap from screen edge (px)
margin_y = 48            # vertical gap from screen edge (px)
max_visible = 5          # max popups stacked at once
timeout = 5000           # display duration in ms (0 = persistent until dismissed)
icon_size = 36           # app icon size in px
width = 360              # notification card width in px

[sound]
enabled = true
file = "~/.config/cooee/sounds/notify.ogg"
volume = 0.8             # 0.0 – 1.0

[tts]
enabled = true
speak_summary = true     # speak notification summary on arrival
voice = ""               # speech-dispatcher voice name (empty = system default)
rate = 0                 # speech rate: -100 (slow) to 100 (fast), 0 = default

[dnd]
mode = "off"             # off | silent | full

[actions]
picker = "rofi -dmenu -p 'Action:'"   # command used to present action choices
                                       # receives labels on stdin, returns selected label on stdout
```

### DND Modes

| Mode | Visual | Sound | TTS |
|---|---|---|---|
| `off` | yes | yes | yes |
| `silent` | yes | no | no |
| `full` | no (discard) | no | no |

---

## D-Bus Interface

cooee registers as `org.freedesktop.Notifications` on the session bus.

### Methods

| Method | Behaviour |
|---|---|
| `Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout)` | Receive notification, apply DND filter, return `uint32` notification ID. If `replaces_id` is non-zero and a notification with that ID exists, replace it in-place (update content, reset timeout, reuse the same GTK window) rather than creating a new popup. |
| `CloseNotification(id: uint32)` | Close popup by ID, emit `NotificationClosed(id, reason)` |
| `GetCapabilities()` | Returns `["body", "body-markup", "body-images", "icon-static", "actions"]` |
| `GetServerInformation()` | Returns `(name="cooee", vendor="cooee", version, spec_version="1.2")` |

### Signals

| Signal | When |
|---|---|
| `NotificationClosed(id, reason)` | Popup dismissed (timeout, user, app request) |
| `ActionInvoked(id, action_key)` | User clicks an action button |

### Notification Lifecycle

```
Notify() received
    │
    ├─ DND = full   → discard silently, return next ID
    │
    ├─ DND = silent → show GTK4 popup only
    │
    └─ DND = off    → play sound → speak summary → show GTK4 popup
                           │
                     popup auto-dismisses after timeout (or app-supplied expiry)
                     or user clicks dismiss button
                           │
                     emit NotificationClosed signal
```

**Stacking:** popups stack vertically from the anchor corner with a small gap. When `max_visible` is reached, the oldest popup is dismissed to make room.

**Monitor:** the popup appears on the currently focused Hyprland monitor, determined by querying the Hyprland socket (`$HYPRLAND_INSTANCE_SIGNATURE`).

---

## Visual Design

Notification popups are card-style GTK4 windows:

- Rounded card with subtle shadow
- App icon (36px, from XDG icon theme or notification hint)
- **Summary** in bold
- **Body** text (Pango markup supported: bold, italic, links)
- Action buttons (one per action supplied by the sending app, rendered below the body text; clicking emits `ActionInvoked`)
- Timestamp ("just now", relative)
- Dismiss button (×)

The card width is fixed (configurable, default 360px). Body text wraps. Images in notification hints are displayed below the body text. If no actions are provided by the sender, the action button row is not rendered.

---

## Unix Socket Protocol

The daemon listens at `$XDG_RUNTIME_DIR/cooee.sock`. Messages are newline-delimited JSON.

### Commands (client → daemon)

```json
{"cmd": "speak"}
{"cmd": "dnd", "mode": "off|silent|full|toggle"}
{"cmd": "dismiss"}           // dismisses the most recently received popup
{"cmd": "action"}            // invoke action picker for the last notification
{"cmd": "status"}
```

### Responses (daemon → client)

```json
{"ok": true}
{"ok": true, "dnd": "silent"}
{"ok": false, "error": "no notification to speak"}
{"ok": false, "error": "last notification has no actions"}
```

### Action Picker Flow

When the daemon receives `{"cmd": "action"}`:

1. Retrieve the last notification's actions list (alternating `[key, label, key, label, ...]` per spec)
2. If the list is empty, return `{"ok": false, "error": "last notification has no actions"}`
3. Write one label per line to the picker command's stdin (configured via `[actions].picker`)
4. Read the selected label from the picker's stdout
5. Look up the corresponding action key for the selected label
6. Emit `ActionInvoked(id, action_key)` on D-Bus — the sending app handles the actual action
7. Return `{"ok": true}` to the client

If the picker exits with a non-zero code (user cancelled), no signal is emitted.

---

## CLI Interface

```
cooee daemon              Start the notification daemon
cooee speak               Speak the full body of the last notification
cooee dnd [off|silent|full|toggle]
                          Get or set DND mode (no argument = print current mode)
cooee dismiss             Dismiss the most recently received popup
cooee action              Open picker to select and invoke an action on the last notification
cooee status              Print daemon status and current DND mode
```

### Hyprland keybind examples

```
# ~/.config/hypr/hyprland.conf
bind = $mod, N, exec, cooee speak
bind = $mod SHIFT, N, exec, cooee dnd toggle
bind = $mod, X, exec, cooee dismiss
bind = $mod, A, exec, cooee action
```

---

## Systemd Service

File: `data/cooee.service` → installed to `~/.config/systemd/user/cooee.service`

```ini
[Unit]
Description=cooee notification daemon
Documentation=https://github.com/theophilusx/cooee
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/cooee daemon
Restart=on-failure
RestartSec=2
Type=dbus
BusName=org.freedesktop.Notifications

[Install]
WantedBy=graphical-session.target
```

`Type=dbus` with `BusName` ensures systemd waits for cooee to claim the D-Bus name before marking it started, preventing race conditions with apps that send notifications at login.

### Installation

```bash
cargo build --release
install -Dm755 target/release/cooee ~/.local/bin/cooee
install -Dm644 data/cooee.service ~/.config/systemd/user/cooee.service
install -Dm644 data/sounds/notify.ogg ~/.config/cooee/sounds/notify.ogg
systemctl --user daemon-reload
systemctl --user enable --now cooee
```

---

## XDG Compliance

| Convention | Implementation |
|---|---|
| Config | `$XDG_CONFIG_HOME/cooee/config.toml` |
| Runtime socket | `$XDG_RUNTIME_DIR/cooee.sock` |
| Data files (sounds) | `$XDG_DATA_HOME/cooee/` or bundled in `data/` |
| Icon lookup | XDG icon theme spec via GTK4 icon theme API |
| Notifications spec | freedesktop.org Desktop Notifications Spec v1.2 |
| Autostart | systemd user service (`graphical-session.target`) |

---

## Out of Scope

- Notification history/log (full DND discards; no persistent store)
- X11 / XWayland support
- GUI settings editor (config is TOML only)
